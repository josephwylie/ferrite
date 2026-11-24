//! Setup sync (ADR 0048) as the window runs it: this computer's agent
//! setup kept the same as each connected Machine's, both ways, with no
//! step of the operator's. A Machine syncs as it connects, as a Thread
//! starts on it, a few seconds after the setup here changes, and every
//! five minutes while it stays connected; one sync at a time each, off
//! the window's thread. Each Machine's last sync is its line in Settings ›
//! Connections, and a file both sides changed is a toast that names it.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::*;
use ferrite_core::chunked::Cancellation;
use ferrite_core::cockpit::ServerState;
use ferrite_core::setup_sync::{self, Home, Job, Outcome, Valve};
use ferrite_core::tree::Side;
use gpui::App;

/// How often a connected Machine syncs with nothing seen to change.
const EVERY: Duration = Duration::from_secs(5 * 60);
/// How long the setup here is left to settle after a change before it
/// syncs: an edit is often several writes.
const SETTLE: Duration = Duration::from_secs(3);

pub(crate) struct SetupSync {
    /// This computer's setup; `None` syncs nothing (tests, the demo).
    home: Option<Home>,
    settings_dir: PathBuf,
    cockpit: Entity<Cockpit>,
    /// Each Machine's, by its saved target.
    machines: HashMap<String, Track>,
    /// When the setup here last changed, as the watcher saw it.
    changed: Arc<Mutex<Option<Instant>>>,
    _watcher: Option<notify::RecommendedWatcher>,
    toasts: Vec<Toast>,
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
    /// Asked while a sync ran: run once it ends.
    again: Option<Ask>,
    next: Option<Instant>,
    status: Status,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Ask {
    only_if_changed_here: bool,
    allow_removals: bool,
}

impl Ask {
    const WHOLE: Ask = Ask {
        only_if_changed_here: false,
        allow_removals: false,
    };
    const HERE: Ask = Ask {
        only_if_changed_here: true,
        allow_removals: false,
    };

    /// One sync that does what both would.
    fn and(self, other: Ask) -> Ask {
        Ask {
            only_if_changed_here: self.only_if_changed_here && other.only_if_changed_here,
            allow_removals: self.allow_removals || other.allow_removals,
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
    Waiting,
    Syncing(Option<usize>),
    /// Synced, but for this many files left as each side has them.
    InSync(usize),
    Stopped(Valve),
    Failed(String),
    /// The Machine's Ferrite is older and syncs no setup.
    Older,
}

impl Status {
    pub(crate) fn line(&self) -> String {
        let files = |count: usize| if count == 1 { "1 file".to_string() } else { format!("{count} files") };
        match self {
            Status::Off => "Off".into(),
            Status::Waiting => "Not synced yet".into(),
            Status::Syncing(None) => "Syncing…".into(),
            Status::Syncing(Some(count)) => format!("Syncing {}", files(*count)),
            Status::InSync(0) => "In sync".into(),
            Status::InSync(held) => format!("In sync except {}", files(*held)),
            Status::Stopped(valve) => format!(
                "Stopped: it would remove {} of {} on {}",
                valve.removing,
                files(valve.holding),
                side(valve.side)
            ),
            Status::Failed(error) => error.lines().next().unwrap_or_default().to_string(),
            Status::Older => "Update Ferrite there to sync".into(),
        }
    }

    pub(crate) fn failed(&self) -> bool {
        matches!(self, Status::Failed(_) | Status::Stopped(_) | Status::Older)
    }
}

pub(crate) enum Toast {
    /// Both sides changed `path`: the `winner`'s is on both now, and the
    /// other is `kept`.
    Kept {
        machine: String,
        path: String,
        winner: Side,
        kept: PathBuf,
    },
    Stopped { machine: String, valve: Valve },
}

/// A toast's kind: each file's and each Machine's is its own, so one
/// never takes another's place.
struct SetupSyncToast;

fn side(side: Side) -> &'static str {
    match side {
        Side::Here => "this computer",
        Side::There => "the Machine",
    }
}

impl SetupSync {
    pub(crate) fn new(home: Option<Home>, settings_dir: PathBuf, cockpit: Entity<Cockpit>) -> Self {
        let changed = Arc::default();
        let watcher = home.as_ref().and_then(|home| watch(home, &changed));
        Self {
            home,
            settings_dir,
            cockpit,
            machines: HashMap::new(),
            changed,
            _watcher: watcher,
            toasts: Vec::new(),
        }
    }

    /// What the Machine `target` names' line says; `None` when this window
    /// syncs no setup.
    pub(crate) fn status(&self, target: &str) -> Option<Status> {
        self.home.as_ref()?;
        Some(self.machines.get(target).map_or(Status::Waiting, |track| track.status.clone()))
    }

    pub(crate) fn take_toasts(&mut self) -> Vec<Toast> {
        std::mem::take(&mut self.toasts)
    }

    pub(crate) fn has_toasts(&self) -> bool {
        !self.toasts.is_empty()
    }

    /// Hear each sync under way, and start each that is due: a Machine
    /// that connected, one due its five minutes, and every connected one
    /// once the setup here settles after a change. Whether a line moved.
    pub(crate) fn tick(&mut self, wanted: Vec<Wanted>, cx: &mut Context<Self>) -> bool {
        let Some(home) = self.home.clone() else {
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
            let track = self.machines.entry(want.target.clone()).or_insert_with(|| Track {
                machine: want.machine,
                name: want.name.clone(),
                attached: false,
                running: None,
                again: None,
                next: None,
                status: Status::Waiting,
            });
            track.machine = want.machine;
            track.name = want.name;
            track.hear(&mut self.toasts, now);
            if !want.on {
                if let Some(running) = track.running.take() {
                    running.cancel.cancel();
                }
                track.again = None;
                track.next = None;
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
                track.next = None;
                continue;
            }
            let ask = if connected || track.next.is_some_and(|next| now >= next) {
                Some(Ask::WHOLE)
            } else if settled {
                Some(Ask::HERE)
            } else if track.running.is_none() {
                track.again.take()
            } else {
                None
            };
            if let Some(ask) = ask {
                track.start(ask, &home, &self.settings_dir, &want.target, &self.cockpit, cx);
            }
        }
        self.statuses() != before
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

    fn ask(&mut self, which: impl Fn(&str, &Track) -> bool, ask: Ask, cx: &mut Context<Self>) {
        let Some(home) = self.home.clone() else {
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
    /// What the sync under way said since the last look.
    fn hear(&mut self, toasts: &mut Vec<Toast>, now: Instant) {
        let Some(running) = &self.running else {
            return;
        };
        loop {
            match running.heard.try_recv() {
                Ok(Heard::Planned(0)) => {}
                Ok(Heard::Planned(files)) => self.status = Status::Syncing(Some(files)),
                Ok(Heard::Done(done)) => {
                    let ask = running.ask;
                    self.running = None;
                    self.next = Some(now + EVERY);
                    self.finished(done, ask, toasts);
                    return;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.running = None;
                    self.status = Status::Failed("The sync stopped before it finished".into());
                    return;
                }
            }
        }
    }

    fn finished(&mut self, done: std::io::Result<Outcome>, ask: Ask, toasts: &mut Vec<Toast>) {
        self.status = match done {
            Ok(Outcome::Synced(synced)) => {
                for kept in synced.conflicts.iter() {
                    toasts.push(Toast::Kept {
                        machine: self.name.clone(),
                        path: kept.path.clone(),
                        winner: kept.winner,
                        kept: kept.kept.clone(),
                    });
                }
                let quiet = ask.only_if_changed_here && synced == Default::default();
                match &self.status {
                    Status::InSync(_) if quiet => self.status.clone(),
                    _ => Status::InSync(synced.held.len() + synced.problems.len()),
                }
            }
            Ok(Outcome::Stopped(valve)) => {
                if self.status != Status::Stopped(valve) {
                    toasts.push(Toast::Stopped {
                        machine: self.name.clone(),
                        valve,
                    });
                }
                Status::Stopped(valve)
            }
            Err(error) if error.kind() == std::io::ErrorKind::Unsupported => Status::Older,
            Err(error) => Status::Failed(error.to_string()),
        };
    }

    /// Start a sync of `target`, or ask for one after the sync under way.
    fn start(&mut self, ask: Ask, home: &Home, settings_dir: &Path, target: &str, cockpit: &Entity<Cockpit>, cx: &mut App) {
        if self.running.is_some() {
            self.again = Some(self.again.map_or(ask, |again| again.and(ask)));
            return;
        }
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
        // the five-minute sync.
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

    /// A line moved: Settings › Connections draws it, and a toast is
    /// presented as the window draws.
    fn setup_sync_moved(&mut self, cx: &mut Context<Self>) {
        if let Some(settings) = self.settings_sheet(cx) {
            settings.update(cx, |_, cx| cx.notify());
        }
        if self.setup_sync.read(cx).has_toasts() {
            cx.notify();
        }
    }

    pub(super) fn sync_setup_anyway(&mut self, target: &str, cx: &mut Context<Self>) {
        self.setup_sync.update(cx, |sync, cx| sync.anyway(target, cx));
        self.setup_sync_moved(cx);
    }

    pub(super) fn sync_setup_before_start(&mut self, machine: MachineId, cx: &mut Context<Self>) {
        self.setup_sync.update(cx, |sync, cx| sync.before_start(machine, cx));
        self.setup_sync_moved(cx);
    }

    /// Say what the syncs had to say.
    pub(super) fn present_setup_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use gpui::component::notification::{Notification, NotificationType};
        use gpui::component::WindowExt as _;
        let toasts = self.setup_sync.update(cx, |sync, _| sync.take_toasts());
        if toasts.is_empty() {
            return;
        }
        for toast in toasts {
            let notification = match toast {
                Toast::Kept {
                    machine,
                    path,
                    winner,
                    kept,
                } => Notification::new()
                    .id1::<SetupSyncToast>(SharedString::from(format!("kept:{machine}:{path}")))
                    .title(format!("~/{path} changed here and on {machine}"))
                    .message(format!(
                        "The newer copy, from {}, is on both now. The other is kept at {}.",
                        match winner {
                            Side::Here => "this computer".to_string(),
                            Side::There => machine.clone(),
                        },
                        kept.display()
                    ))
                    .with_type(NotificationType::Warning)
                    .autohide(false),
                Toast::Stopped { machine, valve } => Notification::new()
                    .id1::<SetupSyncToast>(SharedString::from(format!("stopped:{machine}")))
                    .title(format!("Setup sync with {machine} stopped"))
                    .message(format!(
                        "It would remove {} of {} files on {}. Sync anyway from Settings › Connections.",
                        valve.removing,
                        valve.holding,
                        side(valve.side)
                    ))
                    .with_type(NotificationType::Warning)
                    .autohide(false),
            };
            window.push_notification(notification, cx);
        }
        cx.notify();
    }
}
