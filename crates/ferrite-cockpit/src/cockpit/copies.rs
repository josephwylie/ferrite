//! The copies of this computer's Projects to Machines (ADR 0048 §2). A
//! copy runs off the window's thread over its own connection
//! (`Cockpit::tree`) and outlives the card that began it, so the window
//! keeps every one here, one at a time for each Project and Machine, each
//! a state machine from listing the Project's files to how it ended. The
//! setup sheet's card, a toast and the draft's environment chip are three
//! views of one; none holds a copy's state of its own. How a copy ended
//! stays until the operator has seen it, and one that stopped short stays
//! while the draft that asked for it waits on it.

use super::placement_view::Placing;
use super::*;
use ferrite_client::cockpit::ServerState;
use ferrite_core::chunked::Cancellation;
use ferrite_core::placement::{self, Copied, Copying};
use ferrite_core::tree::{Connect, Stopped};
use ferrite_core::MachineKey;
use gpui::App;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

mod meter;
pub(crate) mod words;
pub(crate) use meter::Reading;
use meter::Meter;

#[cfg(test)]
mod tests;

/// Which copy: of a Project of this computer, to a Machine as it is
/// saved. Turned off and on again, a Machine is held under another
/// `MachineId`, and its copies are still its own.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CopyKey {
    pub(crate) project: ProjectKey,
    pub(crate) machine: MachineKey,
}

impl CopyKey {
    /// Its name in a toast's id and a system notification's tag.
    pub(crate) fn tag(&self) -> String {
        format!("ferrite-copy-{:?}-{:?}", self.project, self.machine)
    }
}

/// What a copy copies, where to, and for whom.
#[derive(Clone)]
pub(crate) struct Order {
    pub(crate) placing: Placing,
    /// Where it goes as the card shows it, the Machine's home as `~/`.
    pub(crate) dest: String,
    /// Where it goes as the Machine spells it.
    pub(crate) target: PathBuf,
    /// What stays here: git-ignored entries left unticked, worktrees.
    pub(crate) skip: Vec<String>,
    /// It goes where an unfinished copy of the Project is, to finish it.
    pub(crate) resumes: bool,
    /// The Machine's home as its Server spells it, once known: a path in
    /// it is said under `~/`.
    pub(crate) home: Option<PathBuf>,
    /// The draft that asked for it waits on it: it cannot be sent until
    /// the copy is added, and then moves there. A copy that stops short
    /// keeps it waiting; only the operator picking an environment on its
    /// chip lets it go.
    pub(crate) waits: bool,
}

/// Where a copy is. One at a time, so the card, the toast and the chip
/// can never disagree.
pub(crate) enum Phase {
    /// The Project's files being listed, this many so far, and the
    /// destination looked at, before anything is sent there.
    Listing { files: usize },
    /// `sent` of `total` bytes there, the last of them of `file`: of
    /// everything the copy carries, what was there before it counting as
    /// sent.
    Copying {
        sent: u64,
        total: u64,
        file: Option<SharedString>,
        meter: Meter,
        /// How fast, and how long it has left, as `meter` last read.
        reading: Reading,
    },
    /// Copied, `bytes` the copy's whole size; the window is adding it on
    /// the Machine. What could not be read here, was changed there or
    /// could not be made there stays as it was. `used`: nothing was
    /// copied, what was there is added as it is.
    Finishing {
        bytes: u64,
        skipped: NotCopied,
        used: bool,
    },
    Done(Summary),
    /// The destination holds what no copy of this Project put there:
    /// nothing was sent. `of`, when it is another Project's unfinished
    /// copy: that Project's folder name.
    Occupied { of: Option<String> },
    /// Why it stopped, what its Resume or Retry does, none once its
    /// Machine was removed, and whether it waits on the Machine coming
    /// back.
    Failed {
        reason: SharedString,
        again: Option<Again>,
        when_back: bool,
    },
    /// The operator stopped it. What it sent stays, for its Resume.
    Cancelled { again: Again },
}

/// What a stopped copy's Resume or Retry does.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Again {
    /// Copy from the start: nothing it sent is kept.
    Retry,
    /// Send what is not there yet.
    Resume,
    /// Everything landed and only adding it failed: add it.
    Add { bytes: u64, skipped: NotCopied },
}

impl Again {
    /// The button's word.
    pub(crate) fn verb(&self) -> &'static str {
        match self {
            Again::Retry => "Retry",
            Again::Resume | Again::Add { .. } => "Resume",
        }
    }

    /// What pressing it does.
    fn does(&self) -> &'static str {
        match self {
            Again::Retry => "Try again",
            Again::Resume => "Resume finishes the copy",
            Again::Add { .. } => "Resume adds the copy",
        }
    }
}

impl Phase {
    /// While it runs: until it is a Project there, or stopped.
    pub(crate) fn live(&self) -> bool {
        matches!(
            self,
            Phase::Listing { .. } | Phase::Copying { .. } | Phase::Finishing { .. }
        )
    }

    /// What its Resume or Retry does, once it stopped: none while it runs,
    /// once it is done, or once its Machine was removed.
    pub(crate) fn again(&self) -> Option<&Again> {
        match self {
            Phase::Failed { again, .. } => again.as_ref(),
            Phase::Cancelled { again } => Some(again),
            _ => None,
        }
    }

    /// How far it is, out of 1, once that can be said.
    pub(crate) fn done(&self) -> Option<f32> {
        match *self {
            Phase::Copying { sent, total, .. } if total > 0 => Some((sent as f32 / total as f32).min(1.)),
            Phase::Copying { .. } | Phase::Finishing { .. } | Phase::Done(_) => Some(1.),
            _ => None,
        }
    }
}

/// What a finished copy did.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Summary {
    /// The copy's whole size, what an earlier run sent included.
    pub(crate) bytes: u64,
    /// From Copy to the Project being there.
    pub(crate) took: Duration,
    pub(crate) skipped: NotCopied,
    /// The Project it is on the Machine.
    pub(crate) project: ProjectKey,
    /// Whether the draft that asked for it moved there.
    pub(crate) moved: bool,
}

/// What a copy left as it was: in lines with why, many alike as one, and
/// how many entries that is.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct NotCopied {
    pub(crate) lines: Vec<String>,
    pub(crate) entries: usize,
}

/// One copy: what was asked, and where it is.
pub(crate) struct Entry {
    pub(crate) order: Order,
    pub(crate) phase: Phase,
    began: Instant,
    /// Its thread, heard until it ends: after the window stopped it too,
    /// since what it says then still counts (`hear_late`).
    run: Option<Run>,
    /// Where the Project's unfinished copies go on the Machine, as the
    /// copy last found them: its own among them once it sends.
    pub(crate) unfinished: Vec<PathBuf>,
    /// The copy's whole size, as its last reading said: what a copy
    /// stopped once all of it landed adds up to.
    whole: Option<u64>,
    /// How it ended was seen, its toast or card closed, while its draft
    /// still waits on it: no toast tells of it again.
    seen: bool,
    /// Its Machine was turned off, or stopped answering, since the copy
    /// stopped: once it answers again, it is back. One that never went is
    /// never said to be back.
    away: bool,
}

/// A copy's thread: what it hears from it, and how it is stopped.
struct Run {
    hear: Receiver<Heard>,
    cancel: Cancellation,
}

/// What the copy's thread hands back.
pub(super) enum Heard {
    /// No connection to the Machine could be made.
    Unreached {
        error: io::Error,
        unfinished: Vec<PathBuf>,
    },
    Copying(Copying),
    /// How the copy ended, and where the Project's unfinished copies go
    /// now.
    Copied {
        copied: Result<Copied, Stopped>,
        unfinished: Vec<PathBuf>,
    },
}

/// What the window does about a copy that moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Landed {
    /// The folder is there: add it as a Project on the Machine.
    Sent(CopyKey),
    /// It ended there being none: tell the operator.
    Ended(CopyKey),
}

/// How often the thread tells the window how far it is: often enough to
/// draw, not once a piece.
const TELL_EVERY: Duration = Duration::from_millis(100);
/// How often the window reads the meters again with nothing heard: a copy
/// that stalls slows to nothing on screen.
const TICK: Duration = Duration::from_secs(1);

impl Entry {
    fn new(order: Order, run: Option<Run>, now: Instant) -> Self {
        Self {
            order,
            phase: Phase::Listing { files: 0 },
            began: now,
            run,
            unfinished: Vec::new(),
            whole: None,
            seen: false,
            away: false,
        }
    }

    /// Whether how it ended was seen while its draft waits on it.
    pub(crate) fn seen(&self) -> bool {
        self.seen
    }

    /// The copy's thread said `heard` at `now`, the Machine `attached` or
    /// not: where the copy is now, and what the window does about it. Once
    /// copying it never goes back: a second pass listing the Project again,
    /// or a reading of less sent, leaves how far it is as it was.
    pub(super) fn hear(&mut self, heard: Heard, now: Instant, attached: bool) -> Option<Landed> {
        if !self.phase.live() {
            self.hear_late(heard);
            return None;
        }
        let target = self.order.target.clone();
        match heard {
            Heard::Copying(Copying::Listing(files)) => {
                if let Phase::Listing { files: listed } = &mut self.phase {
                    *listed = files;
                }
                None
            }
            Heard::Copying(Copying::Sending { progress, file }) => {
                if !self.unfinished.contains(&target) {
                    self.unfinished.push(target);
                }
                self.whole = Some(progress.total);
                let file = file.map(SharedString::from);
                match &mut self.phase {
                    Phase::Copying {
                        sent,
                        total,
                        file: on,
                        meter,
                        reading,
                    } => {
                        (*sent, *total) = (progress.sent.max(*sent), progress.total);
                        if file.is_some() {
                            *on = file;
                        }
                        meter.heard(now, *sent);
                        *reading = meter.read(now, *total);
                    }
                    phase => {
                        let mut meter = Meter::default();
                        meter.heard(now, progress.sent);
                        *phase = Phase::Copying {
                            sent: progress.sent,
                            total: progress.total,
                            file,
                            meter,
                            reading: Reading::default(),
                        };
                    }
                }
                None
            }
            Heard::Unreached { error, unfinished } => {
                ferrite_core::log!("a copy could not reach {}: {error}", self.order.placing.machine_name);
                self.run = None;
                self.phase = Phase::Failed {
                    reason: format!("Can't reach {}.", self.order.placing.machine_name).into(),
                    again: Some(again_for(unfinished.contains(&target))),
                    when_back: true,
                };
                self.unfinished = unfinished;
                Some(Landed::Ended(self.key()))
            }
            Heard::Copied { copied, unfinished } => {
                self.run = None;
                let resumable = unfinished.contains(&target);
                self.unfinished = unfinished;
                self.phase = match copied {
                    Ok(Copied::Occupied) => Phase::Occupied { of: None },
                    Ok(Copied::AnotherCopy { of }) => {
                        let of = of.file_name().map(|name| name.to_string_lossy().into_owned());
                        Phase::Occupied {
                            of: Some(of.unwrap_or_else(|| "another Project".into())),
                        }
                    }
                    Ok(Copied::Sent { sent, changed }) => {
                        let (bytes, skipped) = self.landed(&sent, &changed);
                        self.phase = Phase::Finishing {
                            bytes,
                            skipped,
                            used: false,
                        };
                        return Some(Landed::Sent(self.key()));
                    }
                    Err(stopped) => {
                        ferrite_core::log!("a copy to {} stopped: {}", target.display(), stopped.error);
                        let lost = cut_off(&stopped.error) || !attached;
                        Phase::Failed {
                            reason: self.stopped(&stopped.error, lost).into(),
                            again: Some(again_for(resumable)),
                            when_back: lost,
                        }
                    }
                };
                Some(Landed::Ended(self.key()))
            }
        }
    }

    fn key(&self) -> CopyKey {
        self.order.placing.key()
    }

    /// What the copy's thread said after the window stopped it: how the
    /// copy truly ended, which its Resume follows. One whose files had all
    /// landed is only added on its Resume, with what it left as it was;
    /// one that left part of it there resumes; one that left nothing
    /// starts over. Once its Machine was removed nothing it says counts.
    fn hear_late(&mut self, heard: Heard) {
        let (copied, unfinished) = match heard {
            Heard::Copying(Copying::Sending { progress, .. }) => {
                self.whole = Some(progress.total);
                return;
            }
            Heard::Copying(Copying::Listing(_)) => return,
            Heard::Unreached { unfinished, .. } => (None, unfinished),
            Heard::Copied { copied, unfinished } => (Some(copied), unfinished),
        };
        self.run = None;
        let resumable = unfinished.contains(&self.order.target);
        self.unfinished = unfinished;
        let again = match copied {
            Some(Ok(Copied::Sent { sent, changed })) => {
                let (bytes, skipped) = self.landed(&sent, &changed);
                Again::Add { bytes, skipped }
            }
            _ => again_for(resumable),
        };
        if let Phase::Failed { again: Some(was), .. } | Phase::Cancelled { again: was } = &mut self.phase {
            *was = again;
        }
    }

    /// A copy whose files all landed: its whole size, as its readings
    /// counted it, and what it left as it was.
    fn landed(&self, sent: &ferrite_core::tree::Sent, changed: &[String]) -> (u64, NotCopied) {
        let skipped = NotCopied {
            lines: words::not_copied(sent, changed, &self.order.placing.machine_name),
            entries: sent.problems.len() + sent.unmade.len() + changed.len(),
        };
        (self.whole.unwrap_or(sent.bytes), skipped)
    }

    /// The copy stops at once because its Machine went, `why` in the
    /// operator's words: what it sent stays for its Resume, and one whose
    /// files all landed is only added. What its thread still says counts
    /// (`hear_late`). A Machine `removed` takes the copy with it: nothing
    /// is resumed or added, whatever became of it. False when nothing
    /// changed.
    pub(super) fn machine_went(&mut self, why: String, removed: bool) -> bool {
        self.stop();
        if removed {
            self.run = None;
            if matches!(self.phase, Phase::Done(_) | Phase::Failed { again: None, .. }) {
                return false;
            }
            self.phase = Phase::Failed {
                reason: why.into(),
                again: None,
                when_back: false,
            };
            return true;
        }
        let again = match &self.phase {
            Phase::Finishing { .. } => {
                self.away = self.unadded(why, true);
                return self.away;
            }
            Phase::Copying { .. } => Again::Resume,
            Phase::Listing { .. } => again_for(self.order.resumes),
            _ => return false,
        };
        self.phase = Phase::Failed {
            reason: why.into(),
            again: Some(again),
            when_back: true,
        };
        self.away = true;
        true
    }

    /// The operator stops the copy while it lists or sends: what it sent
    /// stays, for its Resume. What its thread still says counts
    /// (`hear_late`). False when it was doing neither.
    pub(super) fn cancel(&mut self) -> bool {
        let again = match self.phase {
            Phase::Copying { .. } => Again::Resume,
            Phase::Listing { .. } => again_for(self.order.resumes),
            _ => return false,
        };
        self.stop();
        self.phase = Phase::Cancelled { again };
        true
    }

    /// Tell the copy's thread to stop. The window hears it on until it
    /// ends.
    fn stop(&mut self) {
        if let Some(run) = &self.run {
            run.cancel.cancel();
        }
    }

    /// Adding the copy on its Machine failed, for `reason`: its files are
    /// all there, so its Resume only adds it.
    pub(super) fn unadded(&mut self, reason: String, when_back: bool) -> bool {
        let Phase::Finishing { bytes, skipped, used } = &mut self.phase else {
            return false;
        };
        let again = match used {
            true => Again::Retry,
            false => Again::Add {
                bytes: *bytes,
                skipped: std::mem::take(skipped),
            },
        };
        self.phase = Phase::Failed {
            reason: reason.into(),
            again: Some(again),
            when_back,
        };
        true
    }

    /// A Resume of a copy whose files all landed: it is added again, as
    /// it was when it finished.
    fn add_again(&mut self) -> bool {
        let Some(Again::Add { bytes, skipped }) = self.phase.again().cloned() else {
            return false;
        };
        self.phase = Phase::Finishing {
            bytes,
            skipped,
            used: false,
        };
        true
    }

    /// How a copy that stopped stands, as its card and its toast both say
    /// it, its Machine `back` (on, its Server answering) or not: the words,
    /// and whether they tell of a failure. None while it runs, and once it
    /// is done.
    pub(crate) fn standing(&self, back: bool) -> Option<(String, bool)> {
        let machine = &self.order.placing.machine_name;
        Some(match &self.phase {
            Phase::Occupied { .. } => (self.why()?, true),
            Phase::Cancelled { again } => (
                match again {
                    Again::Retry => "Copy cancelled before anything was sent.".into(),
                    Again::Resume => "Copy cancelled. Resume finishes it from where it stopped.".into(),
                    Again::Add { .. } => "Copy cancelled once all of it had landed. Resume adds it.".into(),
                },
                false,
            ),
            Phase::Failed {
                again: Some(again),
                when_back: true,
                ..
            } if back => (format!("{machine} is back. {}.", again.does()), false),
            Phase::Failed {
                reason,
                again: Some(again),
                when_back: true,
            } => (format!("{reason} {} once it is back.", again.does()), true),
            Phase::Failed { reason, .. } => (reason.to_string(), true),
            _ => return None,
        })
    }

    /// Why the copy ended with nothing added, in the operator's words: the
    /// destination held what it would not copy over, or what stopped it.
    pub(crate) fn why(&self) -> Option<String> {
        let dest = self.order.dest.trim_end_matches(['/', '\\']);
        let name = dest.rsplit(['/', '\\']).next().unwrap_or(dest);
        match &self.phase {
            Phase::Occupied { of: None } => Some(format!("{name} is not empty. Nothing there is changed.")),
            Phase::Occupied { of: Some(of) } => Some(format!(
                "{name} holds an unfinished copy of {of}. Nothing there is changed."
            )),
            Phase::Failed { reason, .. } => Some(reason.to_string()),
            _ => None,
        }
    }

    /// Why the copy stopped on `error`, in the operator's words: the
    /// Machine went (`lost`), or what it could not write and why.
    fn stopped(&self, error: &io::Error, lost: bool) -> String {
        let machine = &self.order.placing.machine_name;
        if lost {
            return format!("The connection to {machine} was lost.");
        }
        let target = &self.order.target;
        // The Machine says `entry: Why (os error N)`, naming where it
        // failed between when that is not the entry: `entry: /where: Why`.
        let said = error.to_string();
        let mut parts: Vec<&str> = said.split(": ").collect();
        let why = words::plainly(parts.pop().unwrap_or_default());
        let why = if why.is_empty() { "it stopped".into() } else { why };
        let shown = |path: &Path| tilde(self.order.home.as_deref(), path);
        let absolute = |part: &&&str| part.starts_with(['/', '~']) || part.get(1..3) == Some(":\\");
        match (parts.iter().find(absolute), parts.first().filter(|entry| !entry.is_empty())) {
            (Some(failed), _) => format!("Can't write {}: {why}.", shown(Path::new(failed))),
            (None, Some(entry)) => {
                let written = target.join(entry);
                let folder = written.parent().unwrap_or(target);
                format!("Can't write in {}: {why}.", shown(folder))
            }
            (None, None) => format!("Not copied: {why}."),
        }
    }
}

/// Why a copy to `machine` stopped, or cannot go now.
pub(crate) fn turned_off(machine: &str) -> String {
    format!("{machine} was turned off.")
}

/// What a copy that stopped does again: Resume what it kept, else Retry.
fn again_for(resumable: bool) -> Again {
    match resumable {
        true => Again::Resume,
        false => Again::Retry,
    }
}

/// `path`, as the Machine spells it, under `~/` where it is in `home`.
pub(super) fn tilde(home: Option<&Path>, path: &Path) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) => {
            let parts: Vec<_> = rest.iter().map(|part| part.to_string_lossy()).collect();
            format!("~/{}", parts.join("/"))
        }
        None => path.display().to_string(),
    }
}

/// Whether `error` is the connection to the Machine ending under the copy.
fn cut_off(error: &io::Error) -> bool {
    use io::ErrorKind::*;
    matches!(
        error.kind(),
        BrokenPipe | ConnectionReset | ConnectionAborted | NotConnected | UnexpectedEof | TimedOut
    )
}

/// Every copy, in the order each began.
pub(crate) struct Copies {
    cockpit: Entity<Cockpit>,
    entries: Vec<(CopyKey, Entry)>,
    /// The clock that reads the meters again is running.
    ticking: bool,
}

impl Copies {
    pub(crate) fn new(cockpit: Entity<Cockpit>) -> Self {
        Self {
            cockpit,
            entries: Vec::new(),
            ticking: false,
        }
    }

    pub(crate) fn get(&self, key: &CopyKey) -> Option<&Entry> {
        self.entries
            .iter()
            .find_map(|(at, entry)| (at == key).then_some(entry))
    }

    fn get_mut(&mut self, key: &CopyKey) -> Option<&mut Entry> {
        self.entries
            .iter_mut()
            .find_map(|(at, entry)| (at == key).then_some(entry))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&CopyKey, &Entry)> {
        self.entries.iter().map(|(key, entry)| (key, entry))
    }

    /// The copy the draft in Pane `draft`, on `project`, waits on: one
    /// under way, or one that stopped short, until it is added or the
    /// operator picks where the draft runs (`let_draft_go`).
    pub(crate) fn waited_on_by(&self, draft: PaneIdentity, project: ProjectKey) -> Option<(&CopyKey, &Entry)> {
        self.iter().find(|(key, entry)| {
            key.project == project
                && entry.order.waits
                && entry.order.placing.draft == draft
                && !matches!(entry.phase, Phase::Done(_))
        })
    }

    /// The draft that asked for the copy of `key` waits on it no longer:
    /// it is sent where the operator picked. How a copy that stopped
    /// ended, seen already, goes with it.
    pub(crate) fn let_draft_go(&mut self, key: &CopyKey, cx: &mut Context<Self>) {
        let Some(at) = self.entries.iter().position(|(at, _)| at == key) else {
            return;
        };
        let entry = &mut self.entries[at].1;
        entry.order.waits = false;
        if entry.seen && !entry.phase.live() {
            self.entries.remove(at);
        }
        cx.notify();
    }

    /// The Machine saved as `saved`, as the window holds it now: none while
    /// it is turned off.
    fn held(&self, saved: &MachineKey, cx: &App) -> Option<MachineId> {
        self.cockpit.read(cx).machine_saved_as(saved)
    }

    /// Whether the Machine saved as `saved` answers now: on, and its
    /// Server attached.
    fn answers(&self, saved: &MachineKey, cx: &App) -> bool {
        self.held(saved, cx).is_some_and(|machine| {
            matches!(self.cockpit.read(cx).server(machine), Some(ServerState::Attached { .. }))
        })
    }

    /// Whether the Machine the copy of `key` waits on is back: it went
    /// after the copy stopped, and answers again.
    pub(crate) fn back(&self, key: &CopyKey, cx: &App) -> bool {
        self.get(key).is_some_and(|copy| copy.away) && self.answers(&key.machine, cx)
    }

    /// Note each copy that waits on its Machine whose Machine does not
    /// answer now: it is away, and back once it does (`back`). Whether
    /// any was not noted already.
    fn note_away(&mut self, cx: &App) -> bool {
        let away: Vec<CopyKey> = self
            .iter()
            .filter(|(key, copy)| {
                !copy.away
                    && matches!(copy.phase, Phase::Failed { when_back: true, .. })
                    && !self.answers(&key.machine, cx)
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in &away {
            if let Some(copy) = self.get_mut(key) {
                copy.away = true;
            }
        }
        !away.is_empty()
    }

    /// Copy as `order` says, to its Machine as the window holds it now,
    /// unless a copy of its Project to its Machine runs: one at a time for
    /// each. One that ended goes.
    pub(crate) fn start(&mut self, mut order: Order, cx: &mut Context<Self>) {
        let key = order.placing.key();
        if self.get(&key).is_some_and(|entry| entry.phase.live()) {
            return;
        }
        let now = cx.background_executor().now();
        let entry = match self.held(&key.machine, cx) {
            Some(machine) => {
                order.placing.machine = machine;
                let connect = self.cockpit.update(cx, |cockpit, _| cockpit.tree(machine));
                let run = spawn(&order, connect);
                Entry::new(order, Some(run), now)
            }
            None => {
                let again = again_for(order.resumes);
                let mut entry = Entry::new(order, None, now);
                entry.phase = Phase::Failed {
                    reason: turned_off(&entry.order.placing.machine_name).into(),
                    again: Some(again),
                    when_back: true,
                };
                entry
            }
        };
        self.entries.retain(|(at, _)| *at != key);
        self.entries.push((key, entry));
        self.note_away(cx);
        self.tick_while_copying(cx);
        cx.notify();
    }

    /// The copy of `key` that stopped, again: Resume finishes what it
    /// kept, Retry starts over, each on its Machine as it is now. One
    /// whose files all landed is only added: the window adds it
    /// (`Landed::Sent`). One whose Machine was removed cannot go again.
    pub(crate) fn again(&mut self, key: &CopyKey, cx: &mut Context<Self>) -> Option<Landed> {
        let machine = self.held(&key.machine, cx);
        let entry = self.get_mut(key)?;
        let again = entry.phase.again()?.clone();
        if let Again::Add { .. } = again {
            let Some(machine) = machine else {
                entry.phase = Phase::Failed {
                    reason: turned_off(&entry.order.placing.machine_name).into(),
                    again: Some(again),
                    when_back: true,
                };
                entry.away = true;
                cx.notify();
                return None;
            };
            entry.order.placing.machine = machine;
            entry.add_again();
            cx.notify();
            return Some(Landed::Sent(key.clone()));
        }
        let mut order = entry.order.clone();
        order.resumes = again == Again::Resume;
        self.start(order, cx);
        None
    }

    /// Stop the copy of `key` while it lists or sends (`Entry::cancel`).
    pub(crate) fn cancel(&mut self, key: &CopyKey, cx: &mut Context<Self>) {
        if self.get_mut(key).is_some_and(Entry::cancel) {
            cx.notify();
        }
    }

    /// Each copy running to one of `gone`, Machines turned off or removed,
    /// stops at once and says so; each copy to a Machine `removed`, off
    /// before or not, can never go again and says so: the copies to tell
    /// of.
    pub(crate) fn machines_went(
        &mut self,
        gone: &[MachineId],
        removed: impl Fn(&MachineKey) -> bool,
        cx: &mut Context<Self>,
    ) -> Vec<CopyKey> {
        let mut stopped = Vec::new();
        for (key, entry) in &mut self.entries {
            let removed = removed(&key.machine);
            if !removed && !gone.contains(&entry.order.placing.machine) {
                continue;
            }
            let name = &entry.order.placing.machine_name;
            let why = match removed {
                true => format!("{name} was removed."),
                false => turned_off(name),
            };
            if entry.machine_went(why, removed) {
                stopped.push(key.clone());
            }
        }
        if !stopped.is_empty() {
            cx.notify();
        }
        stopped
    }

    /// Let go of how the copy of `key` ended, once the operator has seen
    /// it; one that runs stays, and one that stopped short stays, seen,
    /// while its draft waits on it. What it found unfinished comes back.
    pub(crate) fn forget(&mut self, key: &CopyKey, cx: &mut Context<Self>) -> Option<Vec<PathBuf>> {
        let at = self
            .entries
            .iter()
            .position(|(at, entry)| at == key && !entry.phase.live())?;
        cx.notify();
        let entry = &mut self.entries[at].1;
        if entry.order.waits && !matches!(entry.phase, Phase::Done(_)) {
            entry.seen = true;
            return Some(entry.unfinished.clone());
        }
        let (_, entry) = self.entries.remove(at);
        Some(entry.unfinished)
    }

    /// The folder at the destination is added as it is, copying nothing:
    /// the copy of `key` found it occupied, by nothing of another
    /// Project's.
    pub(crate) fn use_as_it_is(&mut self, key: &CopyKey, cx: &mut Context<Self>) -> bool {
        let Some(entry) = self.get_mut(key) else {
            return false;
        };
        if !matches!(entry.phase, Phase::Occupied { of: None }) {
            return false;
        }
        entry.phase = Phase::Finishing {
            bytes: 0,
            skipped: NotCopied::default(),
            used: true,
        };
        cx.notify();
        true
    }

    /// Whether the copy of `key` is being added on its Machine: what the
    /// Machine answers of that counts only then.
    pub(crate) fn finishing(&self, key: &CopyKey) -> bool {
        self.get(key)
            .is_some_and(|entry| matches!(entry.phase, Phase::Finishing { .. }))
    }

    /// The copy of `key` is `project` on its Machine, the draft that asked
    /// for it `moved` there or not. False when it copied nothing, using
    /// what was there: it goes, with nothing to tell.
    pub(crate) fn done(&mut self, key: &CopyKey, project: ProjectKey, moved: bool, cx: &mut Context<Self>) -> bool {
        let now = cx.background_executor().now();
        let Some(entry) = self.get_mut(key) else {
            return false;
        };
        let Phase::Finishing { bytes, skipped, used } = &mut entry.phase else {
            return false;
        };
        let (bytes, skipped, used) = (*bytes, std::mem::take(skipped), *used);
        cx.notify();
        if used {
            self.entries.retain(|(at, _)| at != key);
            return false;
        }
        entry.phase = Phase::Done(Summary {
            bytes,
            took: now.saturating_duration_since(entry.began),
            skipped,
            project,
            moved,
        });
        true
    }

    /// The copy of `key` could not be added on its Machine, for `reason`,
    /// `when_back` once the Machine is: its Resume only adds it.
    pub(crate) fn unadded(&mut self, key: &CopyKey, reason: String, when_back: bool, cx: &mut Context<Self>) {
        if self.get_mut(key).is_some_and(|entry| entry.unadded(reason, when_back)) {
            self.note_away(cx);
            cx.notify();
        }
    }

    /// Hear each copy's thread: where each is now, and what the window
    /// does about those that moved.
    pub(crate) fn poll(&mut self, cx: &mut Context<Self>) -> Vec<Landed> {
        let now = cx.background_executor().now();
        let cockpit = self.cockpit.read(cx);
        let mut landed = Vec::new();
        let mut moved = false;
        for (key, entry) in &mut self.entries {
            let attached = matches!(
                cockpit.server(entry.order.placing.machine),
                Some(ServerState::Attached { .. })
            );
            loop {
                let Some(run) = &entry.run else {
                    break;
                };
                let heard = match run.hear.try_recv() {
                    Ok(heard) => heard,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        entry.run = None;
                        if matches!(entry.phase, Phase::Listing { .. } | Phase::Copying { .. }) {
                            entry.phase = Phase::Failed {
                                reason: "The copy stopped.".into(),
                                again: Some(Again::Retry),
                                when_back: false,
                            };
                            landed.push(Landed::Ended(key.clone()));
                        }
                        moved = true;
                        break;
                    }
                };
                moved = true;
                landed.extend(entry.hear(heard, now, attached));
            }
        }
        // The Cockpit has just heard its Servers: a Machine that stopped
        // answering under a stopped copy shows now.
        moved |= self.note_away(cx);
        if moved {
            cx.notify();
        }
        landed
    }

    /// While a copy sends, its meter is read again each `TICK` with
    /// nothing heard, so a stall shows.
    fn tick_while_copying(&mut self, cx: &mut Context<Self>) {
        if self.ticking {
            return;
        }
        self.ticking = true;
        cx.spawn(async move |copies, cx| loop {
            cx.background_executor().timer(TICK).await;
            let going = copies.update(cx, |copies, cx| {
                let now = cx.background_executor().now();
                let mut going = false;
                for (_, entry) in &mut copies.entries {
                    going |= entry.phase.live();
                    if let Phase::Copying {
                        total,
                        meter,
                        reading,
                        ..
                    } = &mut entry.phase
                    {
                        let read = meter.read(now, *total);
                        if read != *reading {
                            *reading = read;
                            cx.notify();
                        }
                    }
                }
                copies.ticking = going;
                going
            });
            if !matches!(going, Ok(true)) {
                break;
            }
        })
        .detach();
    }
}

/// The copy's thread, copying as `order` says over what `connect` makes.
fn spawn(order: &Order, connect: io::Result<Connect>) -> Run {
    let (source, kept, to, skip) = (
        order.placing.root.clone(),
        order.placing.kept.clone(),
        order.target.clone(),
        order.skip.clone(),
    );
    let (heard, hear) = ferrite_core::doorbell::channel();
    let cancel = Cancellation::default();
    let cancelled = cancel.clone();
    std::thread::spawn(move || {
        let mut exchange = match connect.and_then(|connect| connect()) {
            Ok(exchange) => exchange,
            Err(error) => {
                let unfinished = placement::unfinished(&kept, &source);
                let _ = heard.send(Heard::Unreached { error, unfinished });
                return;
            }
        };
        let job = placement::CopyJob {
            source: &source,
            dest: &to,
            skip: &skip,
            kept: &kept,
        };
        let mut told: Option<Instant> = None;
        let mut progress = |copying: Copying| {
            if let Copying::Sending { .. } = copying {
                let now = Instant::now();
                if told.is_some_and(|at| now.duration_since(at) < TELL_EVERY) {
                    return;
                }
                told = Some(now);
            }
            let _ = heard.send(Heard::Copying(copying));
        };
        let copied = placement::copy(&job, &mut *exchange, &mut progress, &cancelled);
        let unfinished = placement::unfinished(&kept, &source);
        let _ = heard.send(Heard::Copied { copied, unfinished });
    });
    Run { hear, cancel }
}
