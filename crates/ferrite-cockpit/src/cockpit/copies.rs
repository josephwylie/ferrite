//! The copies of this computer's Projects to Machines (ADR 0048 §2). A
//! copy runs off the window's thread over its own connection
//! (`Cockpit::tree`) and outlives the card that began it, so the window
//! keeps every one here, one at a time for each Project and Machine, each
//! a state machine from listing the Project's files to how it ended. The
//! setup sheet's card, a toast and the draft's environment chip are three
//! views of one; none holds a copy's state of its own. How a copy ended
//! stays until the operator has seen it.

use super::placement_view::Placing;
use super::*;
use ferrite_client::cockpit::ServerState;
use ferrite_core::chunked::Cancellation;
use ferrite_core::placement::{self, Copied, Copying};
use ferrite_core::tree::{Connect, Stopped};
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

/// Which copy: of a Project of this computer, to a Machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CopyKey {
    pub(crate) project: ProjectKey,
    pub(crate) machine: MachineId,
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
}

/// Where a copy is. One at a time, so the card, the toast and the chip
/// can never disagree.
pub(crate) enum Phase {
    /// The Project's files being listed, this many so far, and the
    /// destination looked at, before anything is sent there.
    Listing { files: usize },
    /// `sent` of `total` bytes there, the last of them of `file`.
    Copying {
        sent: u64,
        total: u64,
        file: Option<SharedString>,
        meter: Meter,
        /// How fast, and how long it has left, as `meter` last read.
        reading: Reading,
    },
    /// Copied, `bytes` sent; the window is adding it on the Machine. What
    /// could not be read here, was changed there or could not be made
    /// there stays as it was. `used`: nothing was copied, what was there
    /// is added as it is.
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
    /// Why it stopped, whether what it sent is kept for a Resume, and
    /// whether it waits on the Machine coming back.
    Failed {
        reason: SharedString,
        resumable: bool,
        when_back: bool,
    },
    /// The operator stopped it. What it sent stays, for its Resume.
    Cancelled { resumable: bool },
}

impl Phase {
    /// While it runs: until it is a Project there, or stopped.
    pub(crate) fn live(&self) -> bool {
        matches!(
            self,
            Phase::Listing { .. } | Phase::Copying { .. } | Phase::Finishing { .. }
        )
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
    run: Option<Run>,
    /// Where the Project's unfinished copies go on the Machine, as the
    /// copy last found them: its own among them once it sends.
    pub(crate) unfinished: Vec<PathBuf>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        }
    }

    /// The copy's thread said `heard` at `now`, the Machine `attached` or
    /// not: where the copy is now, and what the window does about it.
    pub(super) fn hear(&mut self, heard: Heard, now: Instant, attached: bool) -> Option<Landed> {
        let target = self.order.target.clone();
        match heard {
            Heard::Copying(Copying::Listing(files)) => {
                self.phase = Phase::Listing { files };
                None
            }
            Heard::Copying(Copying::Sending { progress, file }) => {
                if !self.unfinished.contains(&target) {
                    self.unfinished.push(target);
                }
                let file = file.map(SharedString::from);
                match &mut self.phase {
                    Phase::Copying {
                        sent,
                        total,
                        file: on,
                        meter,
                        reading,
                    } => {
                        (*sent, *total) = (progress.sent, progress.total);
                        if file.is_some() {
                            *on = file;
                        }
                        meter.heard(now, progress.sent);
                        *reading = meter.read(now, progress.total);
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
                    resumable: unfinished.contains(&target),
                    when_back: true,
                };
                self.unfinished = unfinished;
                Some(Landed::Ended(self.key()))
            }
            Heard::Copied { copied, unfinished } => {
                self.run = None;
                let resumable = unfinished.contains(&target);
                self.unfinished = unfinished;
                let machine = self.order.placing.machine_name.clone();
                self.phase = match copied {
                    Ok(Copied::Occupied) => Phase::Occupied { of: None },
                    Ok(Copied::AnotherCopy { of }) => {
                        let of = of.file_name().map(|name| name.to_string_lossy().into_owned());
                        Phase::Occupied {
                            of: Some(of.unwrap_or_else(|| "another Project".into())),
                        }
                    }
                    Ok(Copied::Sent { sent, changed }) => {
                        let skipped = NotCopied {
                            lines: words::not_copied(&sent, &changed, &machine),
                            entries: sent.problems.len() + sent.unmade.len() + changed.len(),
                        };
                        self.phase = Phase::Finishing {
                            bytes: sent.bytes,
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
                            resumable,
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

    pub(crate) fn get(&self, key: CopyKey) -> Option<&Entry> {
        self.entries
            .iter()
            .find_map(|(at, entry)| (*at == key).then_some(entry))
    }

    fn get_mut(&mut self, key: CopyKey) -> Option<&mut Entry> {
        self.entries
            .iter_mut()
            .find_map(|(at, entry)| (*at == key).then_some(entry))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (CopyKey, &Entry)> {
        self.entries.iter().map(|(key, entry)| (*key, entry))
    }

    /// The copy of `project` that runs, to whichever Machine.
    pub(crate) fn running_of(&self, project: ProjectKey) -> Option<(CopyKey, &Entry)> {
        self.iter()
            .find(|(key, entry)| key.project == project && entry.phase.live())
    }

    /// Copy as `order` says, unless a copy of its Project to its Machine
    /// runs: one at a time for each. One that ended goes.
    pub(crate) fn start(&mut self, order: Order, cx: &mut Context<Self>) {
        let key = order.placing.key();
        if self.get(key).is_some_and(|entry| entry.phase.live()) {
            return;
        }
        let machine = order.placing.machine;
        let connect = self.cockpit.update(cx, |cockpit, _| cockpit.tree(machine));
        let run = spawn(&order, connect);
        let entry = Entry::new(order, Some(run), cx.background_executor().now());
        self.entries.retain(|(at, _)| *at != key);
        self.entries.push((key, entry));
        self.tick_while_copying(cx);
        cx.notify();
    }

    /// The copy of `key` that stopped, again: it finishes what it kept.
    pub(crate) fn again(&mut self, key: CopyKey, cx: &mut Context<Self>) {
        let Some(entry) = self.get(key).filter(|entry| !entry.phase.live()) else {
            return;
        };
        let mut order = entry.order.clone();
        order.resumes = matches!(
            entry.phase,
            Phase::Failed { resumable: true, .. } | Phase::Cancelled { resumable: true }
        );
        self.start(order, cx);
    }

    /// Stop the copy of `key`; what it sent stays, for its Resume.
    pub(crate) fn cancel(&mut self, key: CopyKey, cx: &mut Context<Self>) {
        let Some(entry) = self.get_mut(key) else {
            return;
        };
        let Some(run) = entry.run.take() else {
            return;
        };
        run.cancel.cancel();
        entry.phase = Phase::Cancelled {
            resumable: entry.order.resumes || matches!(entry.phase, Phase::Copying { .. }),
        };
        cx.notify();
    }

    /// Let go of how the copy of `key` ended, once the operator has seen
    /// it; one that runs stays. What it found unfinished comes back.
    pub(crate) fn forget(&mut self, key: CopyKey, cx: &mut Context<Self>) -> Option<Vec<PathBuf>> {
        let at = self
            .entries
            .iter()
            .position(|(at, entry)| *at == key && !entry.phase.live())?;
        let (_, entry) = self.entries.remove(at);
        cx.notify();
        Some(entry.unfinished)
    }

    /// The folder at the destination is added as it is, copying nothing:
    /// the copy of `key` found it occupied, by nothing of another
    /// Project's.
    pub(crate) fn use_as_it_is(&mut self, key: CopyKey, cx: &mut Context<Self>) -> bool {
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

    /// The copy of `key` is `project` on its Machine, the draft that asked
    /// for it `moved` there or not. False when it copied nothing, using
    /// what was there: it goes, with nothing to tell.
    pub(crate) fn done(&mut self, key: CopyKey, project: ProjectKey, moved: bool, cx: &mut Context<Self>) -> bool {
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
            self.entries.retain(|(at, _)| *at != key);
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

    /// The copy of `key` could not be added on its Machine, for `reason`.
    pub(crate) fn failed(&mut self, key: CopyKey, reason: String, cx: &mut Context<Self>) {
        let Some(entry) = self.get_mut(key) else {
            return;
        };
        entry.run = None;
        entry.phase = Phase::Failed {
            reason: reason.into(),
            resumable: false,
            when_back: false,
        };
        cx.notify();
    }

    /// Hear each copy's thread: where each is now, and what the window
    /// does about those that moved.
    pub(crate) fn poll(&mut self, cx: &mut Context<Self>) -> Vec<Landed> {
        let now = cx.background_executor().now();
        let cockpit = self.cockpit.read(cx);
        let mut landed = Vec::new();
        let mut moved = false;
        for (key, entry) in &mut self.entries {
            let attached = matches!(cockpit.server(key.machine), Some(ServerState::Attached { .. }));
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
                                resumable: false,
                                when_back: false,
                            };
                            landed.push(Landed::Ended(*key));
                        }
                        moved = true;
                        break;
                    }
                };
                moved = true;
                landed.extend(entry.hear(heard, now, attached));
            }
        }
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
