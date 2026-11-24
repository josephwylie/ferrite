//! Setup sync (ADR 0048) as the window runs it: this computer's agent
//! setup kept the same as each connected Machine's, both ways, with no
//! step of the operator's. A Machine syncs as it connects, as a Thread
//! starts on it, a few seconds after the setup here changes, and once a
//! minute while it stays connected, on a clock of its own that no other
//! sync moves; one sync at a time each, off the window's thread. Each
//! Machine's last sync is its line in Settings › Connections, and what
//! needs the operator's eye is one toast for them all.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::*;
use ferrite_core::chunked::Cancellation;
use ferrite_core::cockpit::ServerState;
use ferrite_core::setup_sync::{self, Hold, Home, Job, Kept, Outcome, Synced, Valve};
use ferrite_core::tree::Side;
use gpui::App;

/// How often a connected Machine is looked at for what changed there.
const CHECK: Duration = Duration::from_secs(60);
/// How long the setup here is left to settle after a change before it
/// syncs: an edit is often several writes. A sync that found another
/// under way tries again after as long.
const SETTLE: Duration = Duration::from_secs(3);

pub(crate) struct SetupSync {
    /// This computer's setup, or why this window syncs none of it; `None`
    /// syncs nothing and says nothing (tests, the demo).
    home: Option<Result<Home, String>>,
    settings_dir: PathBuf,
    cockpit: Entity<Cockpit>,
    /// Each Machine's, by its saved target.
    machines: HashMap<String, Track>,
    /// When the setup here last changed, as the watcher saw it.
    changed: Arc<Mutex<Option<Instant>>>,
    _watcher: Option<notify::RecommendedWatcher>,
    /// What the toast says, each Machine's by its name.
    news: BTreeMap<String, News>,
    /// The news moved since the toast was last put up.
    untold: bool,
}

/// A Machine as the window holds it now.
pub(crate) struct Wanted {
    pub target: String,
    pub name: String,
    pub machine: MachineId,
    pub attached: bool,
    /// Its "Sync my setup" switch.
    pub on: bool,
}

struct Track {
    machine: MachineId,
    name: String,
    attached: bool,
    running: Option<Running>,
    /// Asked while a sync ran, or when one found another under way: run
    /// once it may.
    again: Option<Ask>,
    /// When the clock next looks at the Machine.
    check: Option<Instant>,
    /// Before this, `again` waits: another sync held the Machine.
    retry: Option<Instant>,
    status: Status,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ask {
    only_if_changed_here: bool,
    read_sign_ins: bool,
    allow_removals: bool,
    sign_ins: Option<Side>,
}

impl Ask {
    /// As it connects and on the clock: all of it, Claude's sign-in with
    /// it, which macOS asks the Keychain for.
    const CHECK: Ask = Ask {
        only_if_changed_here: false,
        read_sign_ins: true,
        allow_removals: false,
        sign_ins: None,
    };
    /// The setup here settled after a change: what changed, if anything
    /// did, and what changed there with it.
    const HERE: Ask = Ask {
        only_if_changed_here: true,
        read_sign_ins: false,
        allow_removals: false,
        sign_ins: None,
    };
    /// A Thread starts there, or the operator said to go on.
    const WHOLE: Ask = Ask {
        only_if_changed_here: false,
        read_sign_ins: false,
        allow_removals: false,
        sign_ins: None,
    };

    /// One sync that does what both would.
    fn and(self, other: Ask) -> Ask {
        Ask {
            only_if_changed_here: self.only_if_changed_here && other.only_if_changed_here,
            read_sign_ins: self.read_sign_ins || other.read_sign_ins,
            allow_removals: self.allow_removals || other.allow_removals,
            sign_ins: other.sign_ins.or(self.sign_ins),
        }
    }
}

struct Running {
    heard: mpsc::Receiver<Heard>,
    cancel: Cancellation,
    ask: Ask,
}

enum Heard {
    Planned(usize),
    Done(std::io::Result<Outcome>),
}

/// What a Machine's line says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Off,
    /// Why this window syncs no setup.
    Refused(String),
    Waiting,
    Syncing(Option<usize>),
    /// Synced, but for each path left as each side has it, with why, and
    /// two sign-ins for the operator to choose between.
    InSync { held: Vec<(String, String)>, sign_ins: bool },
    Stopped(Valve),
    Failed(String),
    /// The Machine's Ferrite is older and syncs no setup.
    Older,
}

impl Status {
    pub(crate) fn line(&self) -> String {
        match self {
            Status::Off => "Off".into(),
            Status::Refused(why) => why.clone(),
            Status::Waiting => "Not synced yet".into(),
            Status::Syncing(None) => "Syncing…".into(),
            Status::Syncing(Some(count)) => format!("Syncing {}", files(*count)),
            Status::InSync { sign_ins: true, .. } => {
                "Signed in differently on each side: choose which to keep".into()
            }
            Status::InSync { held, .. } => match held.as_slice() {
                [] => "In sync".into(),
                [(path, why)] => format!("In sync except ~/{path}: {why}"),
                [(path, why), ..] => format!("In sync except {} paths. ~/{path}: {why}", held.len()),
            },
            Status::Stopped(valve) => format!("Stopped: {}", stopped(valve)),
            Status::Failed(error) => error.clone(),
            Status::Older => "Update Ferrite there to sync".into(),
        }
    }

    pub(crate) fn failed(&self) -> bool {
        matches!(self, Status::Failed(_) | Status::Stopped(_) | Status::Older)
    }
}

fn files(count: usize) -> String {
    if count == 1 {
        "1 file".to_string()
    } else {
        format!("{count} files")
    }
}

/// What a stopped sync would have done, as a sentence's end.
fn stopped(valve: &Valve) -> String {
    if valve.vanished {
        format!(
            "~/{} is gone from {}, and syncing would remove its {} on {}",
            valve.item,
            side(valve.side.other()),
            files(valve.holding),
            side(valve.side)
        )
    } else {
        format!(
            "syncing would remove {} of the {} in ~/{} on {}",
            valve.removing,
            files(valve.holding),
            valve.item,
            side(valve.side)
        )
    }
}

fn side(side: Side) -> &'static str {
    match side {
        Side::Here => "this computer",
        Side::There => "the Machine",
    }
}

/// What the toast says of one Machine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct News {
    /// Each file both sides changed: the newer of each is on both, and
    /// the other is kept in `kept`.
    conflicts: Vec<String>,
    kept: Option<PathBuf>,
    stopped: Option<Valve>,
    failed: Option<String>,
}

impl News {
    /// Note a sync's conflicts: the folder its losing copies are in, or
    /// the folder of every sync's when they are in more than one.
    fn conflicted(&mut self, kept: &[Kept]) {
        for kept in kept {
            if !self.conflicts.contains(&kept.path) {
                self.conflicts.push(kept.path.clone());
            }
            let Some(sync) = kept.kept.ancestors().nth(kept.path.split('/').count()) else {
                continue;
            };
            self.kept = match &self.kept {
                Some(folder) if folder != sync => sync.parent().map(Path::to_path_buf),
                _ => Some(sync.to_path_buf()),
            };
        }
    }

    fn is_empty(&self) -> bool {
        self.conflicts.is_empty() && self.stopped.is_none() && self.failed.is_none()
    }
}

/// The toast for `news`: its title, its message, and the folder its press
/// opens. `None` when there is nothing to say.
fn toast(news: &BTreeMap<String, News>, settings_dir: &Path) -> Option<(String, String, Option<PathBuf>)> {
    let shown = |folder: &Path| {
        let inside = folder.strip_prefix(settings_dir).unwrap_or(folder);
        format!("{}, in Ferrite's settings folder", inside.to_string_lossy().replace('\\', "/"))
    };
    let mut said: Vec<(String, String, String)> = Vec::new();
    let mut folder = None;
    for (machine, news) in news {
        if let [path, ..] = news.conflicts.as_slice() {
            let title = match news.conflicts.len() {
                1 => format!("~/{path} changed on both sides"),
                count => format!("{count} files changed on both sides"),
            };
            let kept = news.kept.as_deref().map(shown).unwrap_or_default();
            let message = match news.conflicts.len() {
                1 => format!("Here and on {machine}. The newer copy is on both now; the other is kept in {kept}."),
                _ => format!("Here and on {machine}. The newer copy of each is on both now; the others are kept in {kept}."),
            };
            let line = format!("{machine}: {title}");
            said.push((title, message, line));
            folder = news.kept.clone();
        }
        if let Some(valve) = &news.stopped {
            said.push((
                format!("Setup sync with {machine} stopped"),
                format!("{}. Sync anyway in Settings › Connections, or put it back.", capitalised(&stopped(valve))),
                format!("{machine}: stopped, {}", stopped(valve)),
            ));
        }
        if let Some(error) = &news.failed {
            said.push((format!("Setup sync with {machine} failed"), error.clone(), format!("{machine}: failed, {error}")));
        }
    }
    match said.as_slice() {
        [] => None,
        [(title, message, _)] => Some((title.clone(), message.clone(), folder)),
        _ => Some((
            format!("Setup sync: {} things to look at", said.len()),
            said.iter().map(|(_, _, line)| line.as_str()).collect::<Vec<_>>().join("\n"),
            folder,
        )),
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

/// The toast's kind: one for every Machine's news, so one never hides
/// another.
struct SetupSyncToast;

impl SetupSync {
    pub(crate) fn new(home: Option<Result<Home, String>>, settings_dir: PathBuf, cockpit: Entity<Cockpit>) -> Self {
        let changed = Arc::default();
        let watcher = match &home {
            Some(Ok(home)) => watch(home, &changed),
            _ => None,
        };
        Self {
            home,
            settings_dir,
            cockpit,
            machines: HashMap::new(),
            changed,
            _watcher: watcher,
            news: BTreeMap::new(),
            untold: false,
        }
    }

    /// What the Machine `target` names' line says; `None` when this window
    /// syncs no setup.
    pub(crate) fn status(&self, target: &str) -> Option<Status> {
        Some(match self.home.as_ref()? {
            Err(why) => Status::Refused(why.clone()),
            Ok(_) => self.machines.get(target).map_or(Status::Waiting, |track| track.status.clone()),
        })
    }

    /// The toast, when what it says moved since it was put up.
    fn untold(&mut self) -> Option<Option<(String, String, Option<PathBuf>)>> {
        std::mem::take(&mut self.untold).then(|| toast(&self.news, &self.settings_dir))
    }

    /// The operator closed the toast: what it said is heard.
    fn heard(&mut self) {
        self.news.clear();
        self.untold = false;
    }

    /// Hear each sync under way, and start each that is due: a Machine
    /// that connected, one whose clock came round, and every connected one
    /// once the setup here settles after a change. Whether a line moved.
    pub(crate) fn tick(&mut self, wanted: Vec<Wanted>, cx: &mut Context<Self>) -> bool {
        let Some(Ok(home)) = self.home.clone() else {
            return false;
        };
        let now = Instant::now();
        let settled = {
            let mut changed = self.changed.lock().unwrap_or_else(PoisonError::into_inner);
            let settled = changed.is_some_and(|at| now.duration_since(at) >= SETTLE);
            if settled {
                *changed = None;
            }
            settled
        };
        let before = self.statuses();
        self.machines.retain(|target, _| wanted.iter().any(|want| want.target == *target));
        for want in wanted {
            let track = self.machines.entry(want.target.clone()).or_insert_with(|| Track::new(want.machine, &want.name));
            track.machine = want.machine;
            track.name = want.name;
            if let Some((done, ask)) = track.hear() {
                let news = self.news.entry(track.name.clone()).or_default();
                self.untold |= track.finished(done, ask, now, news);
                if news.is_empty() {
                    self.news.remove(&track.name);
                }
            }
            if !want.on {
                if let Some(running) = track.running.take() {
                    running.cancel.cancel();
                }
                track.again = None;
                track.check = None;
                track.retry = None;
                track.attached = want.attached;
                track.status = Status::Off;
                continue;
            }
            if track.status == Status::Off {
                // Turned on again: it syncs as if it had just connected.
                track.status = Status::Waiting;
                track.attached = false;
            }
            let connected = want.attached && !track.attached;
            track.attached = want.attached;
            if !want.attached {
                track.check = None;
                continue;
            }
            if let Some(ask) = track.due(now, connected, settled) {
                track.start(ask, &home, &self.settings_dir, &want.target, &self.cockpit, cx);
            }
        }
        self.statuses() != before || self.untold
    }

    /// Sync the Machine the Cockpit holds as `machine` now: a Thread starts
    /// there, so its setup should be this computer's.
    pub(crate) fn before_start(&mut self, machine: MachineId, cx: &mut Context<Self>) {
        self.ask(|_, track| track.machine == machine, Ask::WHOLE, cx);
    }

    /// Sync the Machine `target` names, removing what the plan removes.
    pub(crate) fn anyway(&mut self, target: &str, cx: &mut Context<Self>) {
        let ask = Ask {
            allow_removals: true,
            ..Ask::WHOLE
        };
        self.ask(|named, _| named == target, ask, cx);
    }

    /// Sync the Machine `target` names, each sign-in that differs with
    /// none agreed taken from `side`.
    pub(crate) fn use_sign_ins(&mut self, target: &str, side: Side, cx: &mut Context<Self>) {
        let ask = Ask {
            sign_ins: Some(side),
            ..Ask::CHECK
        };
        self.ask(|named, _| named == target, ask, cx);
    }

    fn ask(&mut self, which: impl Fn(&str, &Track) -> bool, ask: Ask, cx: &mut Context<Self>) {
        let Some(Ok(home)) = self.home.clone() else {
            return;
        };
        for (target, track) in &mut self.machines {
            if which(target, track) && track.attached && track.status != Status::Off {
                track.start(ask, &home, &self.settings_dir, target, &self.cockpit, cx);
            }
        }
    }

    fn statuses(&self) -> BTreeMap<String, Status> {
        self.machines.iter().map(|(target, track)| (target.clone(), track.status.clone())).collect()
    }
}

impl Track {
    fn new(machine: MachineId, name: &str) -> Self {
        Self {
            machine,
            name: name.to_string(),
            attached: false,
            running: None,
            again: None,
            check: None,
            retry: None,
            status: Status::Waiting,
        }
    }

    /// The sync due `now`, if any: all of it as the Machine connects and
    /// each time the clock comes round, what changed here once that
    /// settles, and else what waited. Nothing but the clock moves it.
    fn due(&mut self, now: Instant, connected: bool, settled: bool) -> Option<Ask> {
        if connected || self.check.is_some_and(|check| now >= check) {
            self.check = Some(now + CHECK);
            return Some(Ask::CHECK);
        }
        if settled {
            return Some(Ask::HERE);
        }
        let free = self.running.is_none() && self.retry.is_none_or(|retry| now >= retry);
        free.then(|| self.again.take()).flatten()
    }

    /// What the sync under way came to, once it is done.
    fn hear(&mut self) -> Option<(std::io::Result<Outcome>, Ask)> {
        let running = self.running.as_ref()?;
        loop {
            match running.heard.try_recv() {
                Ok(Heard::Planned(0)) => {}
                Ok(Heard::Planned(files)) => self.status = Status::Syncing(Some(files)),
                Ok(Heard::Done(done)) => {
                    let ask = running.ask;
                    self.running = None;
                    return Some((done, ask));
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    let ask = running.ask;
                    self.running = None;
                    return Some((Err(std::io::Error::other("The sync stopped before it finished")), ask));
                }
            }
        }
    }

    /// Take in what a sync came to: the line it leaves, and what `news`
    /// says of it. Whether the news moved. A sync of what changed here
    /// that found nothing changed learned nothing of the Machine, so the
    /// line and the news stand; one that found another under way looks at
    /// all of it next, since the other may have moved anything.
    fn finished(&mut self, done: std::io::Result<Outcome>, ask: Ask, now: Instant, news: &mut News) -> bool {
        if matches!(&done, Ok(Outcome::Synced(synced)) if ask.only_if_changed_here && *synced == Synced::default()) {
            return false;
        }
        let before = news.clone();
        self.status = match done {
            Ok(Outcome::Synced(synced)) => {
                news.stopped = None;
                news.failed = None;
                news.conflicted(&synced.conflicts);
                in_sync(&synced)
            }
            Ok(Outcome::Stopped(valve)) => {
                news.failed = None;
                if self.status != Status::Stopped(valve.clone()) {
                    news.stopped = Some(valve.clone());
                }
                Status::Stopped(valve)
            }
            Ok(Outcome::Busy) => {
                let ask = Ask {
                    only_if_changed_here: false,
                    ..ask
                };
                self.again = Some(self.again.map_or(ask, |again| again.and(ask)));
                self.retry = Some(now + SETTLE);
                Status::Syncing(None)
            }
            Err(error) if error.kind() == std::io::ErrorKind::Unsupported => Status::Older,
            Err(error) => {
                let error = error.to_string().lines().next().unwrap_or_default().to_string();
                if !matches!(self.status, Status::Failed(_)) {
                    news.failed = Some(error.clone());
                }
                Status::Failed(error)
            }
        };
        *news != before
    }

    /// Start a sync of `target`, or ask for one after the sync under way.
    fn start(&mut self, ask: Ask, home: &Home, settings_dir: &Path, target: &str, cockpit: &Entity<Cockpit>, cx: &mut App) {
        if self.running.is_some() {
            self.again = Some(self.again.map_or(ask, |again| again.and(ask)));
            return;
        }
        let ask = self.again.take().map_or(ask, |again| again.and(ask));
        self.retry = None;
        let machine = self.machine;
        let connect = match cockpit.update(cx, |cockpit, _| cockpit.tree(machine)) {
            Ok(connect) => connect,
            Err(error) => {
                self.status = Status::Failed(error.to_string());
                return;
            }
        };
        let job = Job {
            allow_removals: ask.allow_removals,
            only_if_changed_here: ask.only_if_changed_here,
            read_sign_ins: ask.read_sign_ins,
            sign_ins: ask.sign_ins,
            ..Job::for_machine(home.clone(), settings_dir, target)
        };
        let (said, heard) = ferrite_core::doorbell::channel();
        let cancel = Cancellation::default();
        let stop = cancel.clone();
        let spawned = std::thread::Builder::new().name("setup-sync".into()).spawn(move || {
            let planned = said.clone();
            let done = connect().and_then(|mut there| {
                setup_sync::run(&job, &mut *there, &mut |files| drop(planned.send(Heard::Planned(files))), &stop)
            });
            let _ = said.send(Heard::Done(done));
        });
        if let Err(error) = spawned {
            self.status = Status::Failed(error.to_string());
            return;
        }
        self.running = Some(Running { heard, cancel, ask });
        if self.status == Status::Waiting {
            self.status = Status::Syncing(None);
        }
    }
}

/// The line a sync that finished leaves.
fn in_sync(synced: &Synced) -> Status {
    let mut held: Vec<(String, String)> = synced
        .problems
        .iter()
        .map(|problem| {
            let error = problem.error.lines().next().unwrap_or_default();
            (problem.path.clone(), format!("{error} ({})", side(problem.side)))
        })
        .collect();
    let mut sign_ins = false;
    for (path, why) in &synced.held {
        let why = match why {
            Hold::SignIn => {
                sign_ins = true;
                continue;
            }
            Hold::Linked(on) => format!("it lies through a link on {}, and Setup sync never writes through one", side(*on)),
            Hold::Kinds => "it is a file on one side and a folder on the other".to_string(),
        };
        held.push((path.clone(), why));
    }
    Status::InSync { held, sign_ins }
}

/// A watcher that notes when anything the table names changes here.
fn watch(home: &Home, changed: &Arc<Mutex<Option<Instant>>>) -> Option<notify::RecommendedWatcher> {
    use notify::Watcher as _;
    let seen = changed.clone();
    let named = home.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok_and(|event| event.paths.iter().any(|path| named.names(path))) {
            *seen.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
        }
    })
    .ok()?;
    for (dir, all) in home.watched() {
        let mode = if all {
            notify::RecursiveMode::Recursive
        } else {
            notify::RecursiveMode::NonRecursive
        };
        // A folder not there yet is not watched: its making is seen from
        // the folder holding it, and what is written in it after waits for
        // the clock.
        let _ = watcher.watch(&dir, mode);
    }
    Some(watcher)
}

impl CockpitView {
    /// Each saved Machine the window holds, as Setup sync wants it.
    fn setup_sync_wanted(&self, cx: &App) -> Vec<Wanted> {
        let Some(machines) = self.machines.as_ref() else {
            return Vec::new();
        };
        let cockpit = self.cockpit.read(cx);
        machines
            .rows(cockpit)
            .into_iter()
            .filter_map(|row| {
                let saved = machines.machine(row.index)?;
                let machine = row.held?;
                Some(Wanted {
                    target: saved.target.clone(),
                    name: saved.name.clone(),
                    machine,
                    attached: matches!(cockpit.server(machine), Some(ServerState::Attached { .. })),
                    on: saved.sync_setup,
                })
            })
            .collect()
    }

    pub(super) fn tick_setup_sync(&mut self, cx: &mut Context<Self>) {
        let wanted = self.setup_sync_wanted(cx);
        if self.setup_sync.update(cx, |sync, cx| sync.tick(wanted, cx)) {
            self.setup_sync_moved(cx);
        }
    }

    /// A line or the news moved: Settings › Connections draws it, and the
    /// toast is put up as the window draws.
    fn setup_sync_moved(&mut self, cx: &mut Context<Self>) {
        if let Some(settings) = self.settings_sheet(cx) {
            settings.update(cx, |_, cx| cx.notify());
        }
        if self.setup_sync.read(cx).untold {
            cx.notify();
        }
    }

    pub(super) fn sync_setup_anyway(&mut self, target: &str, cx: &mut Context<Self>) {
        self.setup_sync.update(cx, |sync, cx| sync.anyway(target, cx));
        self.setup_sync_moved(cx);
    }

    pub(super) fn sync_setup_sign_ins(&mut self, target: &str, side: Side, cx: &mut Context<Self>) {
        self.setup_sync.update(cx, |sync, cx| sync.use_sign_ins(target, side, cx));
        self.setup_sync_moved(cx);
    }

    pub(super) fn sync_setup_before_start(&mut self, machine: MachineId, cx: &mut Context<Self>) {
        self.setup_sync.update(cx, |sync, cx| sync.before_start(machine, cx));
        self.setup_sync_moved(cx);
    }

    /// Put up what the syncs have to say, in place of what it said before,
    /// or take it down once there is nothing.
    pub(super) fn present_setup_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use gpui::component::button::Button;
        use gpui::component::notification::{Notification, NotificationType};
        use gpui::component::WindowExt as _;
        let Some(toast) = self.setup_sync.update(cx, |sync, _| sync.untold()) else {
            return;
        };
        let Some((title, message, folder)) = toast else {
            window.remove_notification::<SetupSyncToast>(cx);
            return;
        };
        let sync = self.setup_sync.downgrade();
        let notification = Notification::new()
            .id::<SetupSyncToast>()
            .title(title)
            .message(message)
            .with_type(NotificationType::Warning)
            .autohide(false)
            .on_close(move |_, cx| {
                sync.update(cx, |sync, _| sync.heard()).ok();
            });
        let notification = match folder {
            Some(folder) => notification.action(move |_, _, _| {
                let folder = folder.clone();
                Button::new("setup-sync-open-kept")
                    .label("Open folder")
                    .on_click(move |_, _, cx| cx.open_with_system(&folder))
            }),
            None => notification,
        };
        window.push_notification(notification, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track::new(MachineId::unheld(), "box")
    }

    fn kept(path: &str, sync: &str) -> Kept {
        let kept = path.split('/').fold(PathBuf::from("/settings/sync-conflicts").join(sync), |at, part| at.join(part));
        Kept {
            path: path.into(),
            winner: Side::There,
            kept,
        }
    }

    #[test]
    fn the_clock_comes_round_every_minute_whatever_else_syncs() {
        let mut track = track();
        let start = Instant::now();
        assert_eq!(track.due(start, true, false), Some(Ask::CHECK), "as it connects");
        for second in [3, 23, 43] {
            assert_eq!(track.due(start + Duration::from_secs(second), false, true), Some(Ask::HERE), "a change here at {second}s");
        }
        assert_eq!(track.due(start + Duration::from_secs(59), false, false), None);
        assert_eq!(track.due(start + CHECK, false, false), Some(Ask::CHECK), "not moved by the syncs a change here started");
        assert_eq!(track.due(start + CHECK + Duration::from_secs(1), false, false), None);
        assert_eq!(track.due(start + 2 * CHECK, false, true), Some(Ask::CHECK), "the clock's sync carries a change here too");
    }

    #[test]
    fn only_the_connection_and_the_clock_read_the_sign_ins() {
        assert!(Ask::CHECK.read_sign_ins);
        assert!(!Ask::HERE.read_sign_ins && !Ask::WHOLE.read_sign_ins);
        assert!(Ask::HERE.and(Ask::CHECK).read_sign_ins);
        assert!(!Ask::HERE.and(Ask::CHECK).only_if_changed_here);
    }

    #[test]
    fn a_sync_that_finds_another_under_way_shows_syncing_and_tries_again() {
        let mut track = track();
        let mut news = News::default();
        let now = Instant::now();
        track.status = Status::InSync { held: Vec::new(), sign_ins: false };
        assert!(!track.finished(Ok(Outcome::Busy), Ask::HERE, now, &mut news));
        assert_eq!(track.status, Status::Syncing(None));
        assert!(!track.status.failed());
        assert_eq!(track.due(now + Duration::from_secs(1), false, false), None, "it waits a moment");
        assert_eq!(track.due(now + SETTLE, false, false), Some(Ask::WHOLE), "all of it, as the other sync may have moved anything");
    }

    #[test]
    fn a_change_here_that_changed_nothing_leaves_a_stop_standing() {
        let mut track = track();
        let mut news = News::default();
        let now = Instant::now();
        let valve = Valve {
            side: Side::Here,
            item: ".claude/skills".into(),
            removing: 410,
            holding: 410,
            vanished: true,
        };
        assert!(track.finished(Ok(Outcome::Stopped(valve.clone())), Ask::CHECK, now, &mut news));
        assert!(!track.finished(Ok(Outcome::Synced(Synced::default())), Ask::HERE, now, &mut news));
        assert_eq!(track.status, Status::Stopped(valve.clone()));
        assert_eq!(news.stopped, Some(valve));
    }

    #[test]
    fn one_toast_says_a_syncs_conflicts_and_a_failure_raises_one() {
        let mut track = track();
        let mut news = News::default();
        let now = Instant::now();
        let synced = Synced {
            conflicts: vec![kept(".claude/CLAUDE.md", "1"), kept(".claude/agents/a.md", "1"), kept(".codex/AGENTS.md", "1")],
            ..Synced::default()
        };
        assert!(track.finished(Ok(Outcome::Synced(synced)), Ask::CHECK, now, &mut news));
        let all = BTreeMap::from([("box".to_string(), news.clone())]);
        let (title, message, folder) = toast(&all, Path::new("/settings")).unwrap();
        assert_eq!(title, "3 files changed on both sides");
        assert!(message.contains("kept in sync-conflicts/1, in Ferrite's settings folder"), "{message}");
        assert_eq!(folder, Some(PathBuf::from("/settings/sync-conflicts/1")));

        let failed = || Err(std::io::Error::other("connection reset\nmore"));
        assert!(track.finished(failed(), Ask::CHECK, now, &mut news));
        assert_eq!(track.status.line(), "connection reset");
        assert!(!track.finished(failed(), Ask::CHECK, now, &mut news), "still failing: no second toast");
        let all = BTreeMap::from([("box".to_string(), news.clone())]);
        let (title, message, _) = toast(&all, Path::new("/settings")).unwrap();
        assert_eq!(title, "Setup sync: 2 things to look at", "one toast, the first not hidden");
        assert_eq!(message, "box: 3 files changed on both sides\nbox: failed, connection reset");

        assert!(track.finished(Ok(Outcome::Synced(Synced::default())), Ask::CHECK, now, &mut news));
        assert_eq!(news.failed, None, "it synced again");
        assert_eq!(news.conflicts.len(), 3);
    }

    #[test]
    fn a_line_says_what_was_held_and_where_a_sync_stopped() {
        let synced = Synced {
            problems: vec![ferrite_core::tree::Problem {
                path: ".claude/skills".into(),
                side: Side::There,
                error: "it is a link to something not there".into(),
            }],
            ..Synced::default()
        };
        assert_eq!(
            in_sync(&synced).line(),
            "In sync except ~/.claude/skills: it is a link to something not there (the Machine)"
        );
        let signed_in = Synced {
            held: BTreeMap::from([(".claude/.credentials.json".to_string(), Hold::SignIn)]),
            ..Synced::default()
        };
        assert!(matches!(in_sync(&signed_in), Status::InSync { sign_ins: true, .. }));
        let gone = Valve {
            side: Side::Here,
            item: ".claude/skills".into(),
            removing: 4,
            holding: 4,
            vanished: true,
        };
        assert_eq!(
            Status::Stopped(gone).line(),
            "Stopped: ~/.claude/skills is gone from the Machine, and syncing would remove its 4 files on this computer"
        );
    }
}
