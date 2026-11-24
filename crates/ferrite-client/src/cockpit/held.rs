//! One machine as a window holds it (ADR 0045): the link to its Engine and
//! everything that Engine's messages built — each open Thread's mirror,
//! the shared state's copies, the parked rows, what it was told of the
//! window and what the window streams from it. A Cockpit holds one per
//! machine, by `MachineId`, and names every Thread of one by a key that
//! carries its id, so two machines' Threads of the same number never meet.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant, SystemTime};

use super::{mirror, parked, remote, shared, Asked, Said, Unread};
use ferrite_core::engine::{BootstrapResult, Bootstrapped, Engine, PaneUpdate};
use ferrite_core::protocol::{ClientMessage, Command, Done, Op, ServerMessage, WireError};
use ferrite_core::roster::Roster;
use ferrite_core::store::Store;
use ferrite_core::{MachineId, MachineKey, ProjectKey, ThreadId, ThreadKey};

/// How long a Thread off screen and at rest stays streamed whole: as long
/// as the Server keeps its Session (ADR 0032). One at work stays however
/// long it works, drawn here or not.
const UNWATCH_AFTER: Duration = ferrite_core::engine::HELD_AT_REST;

/// The window number an Engine in this process knows its one window by.
const IN_PROCESS_WINDOW: u64 = 0;

pub(super) struct HeldMachine {
    /// What every key of this machine's Threads carries.
    pub(super) id: MachineId,
    /// What saved files name this machine.
    pub(super) saved_as: MachineKey,
    pub(super) link: Link,
    /// The Server said something since the window last asked
    /// (`take_arrived`).
    pub(super) arrived: bool,
    /// What a parked Thread says: the rows its Server published.
    pub(super) parked: parked::Parked,
    /// This computer's logs, read-only, for what an open Thread's header
    /// and last write say, which no message carries; `None` on a machine.
    pub(super) logs: Option<Store>,
    /// The open Threads, as the Roster last followed them (`sync_roster`).
    synced: Vec<ThreadId>,
    /// Each open Thread as the Engine's messages built it.
    pub(super) mirrors: mirror::Mirrors,
    /// What every window shares, as the Engine's messages left it.
    pub(super) shared: shared::Shared,
    /// What the Engine was last told of this window (`tell_window`); `None`
    /// until told, and again after every attach: a Server forgets a window
    /// that left.
    told: Option<Told>,
    /// What its Server was last told is seen on each visible Thread, so
    /// the same unread things are told once (`acknowledge_seen`); emptied
    /// with `told` at every attach.
    pub(super) seen_told: BTreeMap<ThreadId, Vec<Unread>>,
    /// What this window draws of this machine's Threads at Transcript
    /// level (`draws_transcripts`). `None` for an Engine in this process
    /// until its window first says: until then it streams every open
    /// Thread.
    pub(super) drawn: Option<Drawn>,
    /// The `window.*` actions this machine's Server handed the window and
    /// it has not done: only a `ferrite --headless` one is (ADR 0038).
    pub(super) drives: Vec<(u64, ferrite_core::drive::Action)>,
    /// A fixture world's Server is one from before `Capability::ForkOf`
    /// (`fixture_server_before_fork_of`).
    pub(super) before_fork_of: bool,
}

impl HeldMachine {
    pub(super) fn new(
        link: Link,
        logs: Option<Store>,
        dir: &std::path::Path,
        saved_as: MachineKey,
    ) -> Self {
        let limits = logs
            .as_ref()
            .map(Store::activity_limits)
            .unwrap_or_default();
        Self {
            id: MachineId::mint(),
            saved_as,
            link,
            arrived: false,
            parked: parked::Parked::default(),
            logs,
            synced: Vec::new(),
            mirrors: mirror::Mirrors::retaining(limits),
            shared: shared::Shared::new(dir),
            told: None,
            seen_told: BTreeMap::new(),
            drawn: None,
            drives: Vec::new(),
            before_fork_of: false,
        }
    }

    /// `thread` of this machine.
    pub(super) fn key(&self, thread: ThreadId) -> ThreadKey {
        ThreadKey::new(self.id, thread)
    }

    pub(super) fn project_key(&self, project: ferrite_core::project::ProjectId) -> ProjectKey {
        ProjectKey::new(self.id, project)
    }

    /// `server_ms`, milliseconds since the epoch on this machine's clock, as
    /// a time on this computer's (F-A4, T3 Code's `toServerClock`): two
    /// machines' times compare as they happened, however far apart their
    /// clocks are. An Engine in this process reads this computer's clock.
    pub(super) fn wall(&self, server_ms: i64) -> SystemTime {
        let local_ms = match &self.link {
            Link::InProcess(_) => server_ms,
            Link::Server(server) => server.clock().local_ms(server_ms),
        };
        SystemTime::UNIX_EPOCH + Duration::from_millis(local_ms.max(0) as u64)
    }

    /// Whether this machine is this computer, reached: a path it names is
    /// a path on this disk. A machine's is not (ADR 0026).
    pub(super) fn local(&self) -> bool {
        match &self.link {
            Link::InProcess(_) => true,
            Link::Server(server) => server.bridge().is_none(),
        }
    }

    /// What its Engine said since the last take, each in-process message
    /// crossing as its encoded line in debug builds, so nothing escapes
    /// the contract.
    pub(super) fn take_arrivals(&mut self) -> Vec<remote::Arrival> {
        let before_fork_of = self.before_fork_of;
        match &mut self.link {
            Link::InProcess(engine) => engine
                .take_messages()
                .into_iter()
                .map(|message| match message {
                    ServerMessage::Focus { thread, .. } if before_fork_of => ServerMessage::Focus {
                        thread,
                        fork_of: None,
                    },
                    message => message,
                })
                .map(|message| remote::Arrival::Message(crossed(message)))
                .chain(
                    engine
                        .take_bootstrap_results()
                        .into_iter()
                        .map(remote::Arrival::Started),
                )
                .collect(),
            Link::Server(server) => server.poll(),
        }
    }

    /// Take in what arrived: the mirrors, the copies, what the window is
    /// to hear, and which Threads to stream. The first sends that settled
    /// are the Cockpit's to complete, in its Roster.
    pub(super) fn arrive(
        &mut self,
        arrived: Vec<remote::Arrival>,
        said: &mut Said,
    ) -> Vec<BootstrapResult> {
        self.arrived |= !arrived.is_empty();
        let open_before = self.shared.open.clone();
        let attached = arrived
            .iter()
            .any(|arrival| matches!(arrival, remote::Arrival::Attached { .. }));
        let boot = self.boot();
        let mut settled = Vec::new();
        for arrival in arrived {
            match arrival {
                remote::Arrival::Message(message) => self.message(message, boot, said),
                remote::Arrival::Attached { snapshot, new_boot } => {
                    self.told = None;
                    self.seen_told.clear();
                    if new_boot {
                        self.mirrors = mirror::Mirrors::retaining(self.mirrors.limits());
                    }
                    self.shared.attached(&snapshot);
                    self.parked.attached(&snapshot);
                    self.mirrors.retain(&self.shared.open);
                    self.mirrors.restore_decisions(&snapshot.decisions);
                    for state in snapshot.threads {
                        self.mirrors.receive(
                            ServerMessage::Thread {
                                revision: snapshot.revision,
                                state,
                            },
                            boot,
                        );
                    }
                    self.mirrors.attached(snapshot.digests, boot);
                }
                remote::Arrival::Started(result) => settled.push(result),
                remote::Arrival::Answered => {}
            }
        }
        // What a Server's snapshot lists is not new.
        if !attached {
            let now = ferrite_core::clock::instant();
            if let Some(drawn) = &mut self.drawn {
                for &thread in &self.shared.open {
                    if !open_before.contains(&thread) {
                        drawn.hold(thread, now);
                    }
                }
            }
        }
        self.follow_attention();
        settled
    }

    /// The Engine's clock zero on this side.
    fn boot(&self) -> Instant {
        match &self.link {
            Link::InProcess(engine) => engine.boot(),
            Link::Server(server) => server.boot(),
        }
    }

    fn message(&mut self, message: ServerMessage, boot: Instant, said: &mut Said) {
        if let ServerMessage::Drive { request, action } = message {
            self.drives.push((request, action));
            return;
        }
        if let ServerMessage::Browser { thread, .. }
        | ServerMessage::BrowserRequest { thread, .. } = &message
        {
            said.browser.push((self.key(*thread), message));
            return;
        }
        if let ServerMessage::AllowanceWarning {
            provider,
            window,
            threshold,
            used_fraction,
        } = &message
        {
            if let Ok(window) = (*window).try_into() {
                said.allowance_warnings
                    .push(ferrite_core::providers::limits::AllowanceWarning {
                        provider: *provider,
                        window,
                        threshold: *threshold,
                        used_fraction: *used_fraction,
                    });
            }
        }
        match &message {
            ServerMessage::Focus { thread, fork_of } => said.asked.push(Asked::Focus {
                thread: self.key(*thread),
                fork_of: fork_of.map(|fork_of| self.key(fork_of)),
            }),
            ServerMessage::Notification {
                title,
                body,
                sound,
                important,
            } => said.asked.push(Asked::Notification {
                title: title.clone(),
                body: body.clone(),
                sound: sound.sound(),
                important: *important,
            }),
            ServerMessage::TaskAlert {
                project,
                task,
                status,
                title,
                detail,
            } => said.asked.push(Asked::TaskAlert {
                project: self.project_key(*project),
                task: task.clone(),
                status: *status,
                title: title.clone(),
                detail: detail.clone(),
            }),
            _ => {}
        }
        self.shared.receive(&message);
        self.parked.receive(&message);
        self.mirrors.receive(message, boot);
    }

    /// The Roster follows this machine's open Threads, whoever changed
    /// them — this window, another, or an API caller. A Thread that opened
    /// gets a Pane at the end (a Thread a Draft is starting takes the
    /// Draft's slot when it settles instead); one that closed loses its
    /// Pane.
    pub(super) fn sync_roster(&mut self, roster: &mut Roster) {
        let open = self.shared.open.clone();
        for gone in self.synced.iter().filter(|thread| !open.contains(thread)) {
            roster.remove_thread(self.key(*gone));
        }
        for thread in open.iter().filter(|thread| !self.synced.contains(thread)) {
            if !self.mirrors.for_draft(*thread) {
                roster.insert_thread(self.key(*thread));
            }
        }
        self.synced = open;
    }

    /// What changed for each of this machine's Panes since the last take.
    pub(super) fn take_updates(&mut self) -> impl Iterator<Item = PaneUpdate<ThreadKey>> + '_ {
        let id = self.id;
        self.mirrors
            .take_updates()
            .into_iter()
            .map(move |update| PaneUpdate {
                thread: ThreadKey::new(id, update.thread),
                dirty: update.dirty,
                evicted: update.evicted,
                subjects: update.subjects,
                activity_changed: update.activity_changed,
                redirects: update.redirects,
            })
    }

    /// The Threads this window streams whole from this machine: those it
    /// draws at Transcript level, and on this computer those drawn or at
    /// work within `UNWATCH_AFTER`. Another machine is a round trip and a
    /// link away, so its Threads stream whole only while drawn (F-A2):
    /// their status, Decisions and Notices arrive all the same. Until the
    /// window says what it draws, or while its Engine sends no digests to
    /// read the rest from, every open one.
    fn watched(&self) -> Vec<ThreadId> {
        let open = &self.shared.open;
        let Some(drawn) = self.drawn.as_ref().filter(|_| self.mirrors.digested()) else {
            return open.clone();
        };
        let now = ferrite_core::clock::instant();
        let lingers = self.local();
        open.iter()
            .copied()
            .filter(|thread| {
                if lingers {
                    drawn.holds(*thread, now)
                } else {
                    drawn.shows(*thread)
                }
            })
            .collect()
    }

    /// Stream what the window draws, and let the rest go.
    pub(super) fn follow_attention(&mut self) {
        if let Some(drawn) = &mut self.drawn {
            let now = ferrite_core::clock::instant();
            for thread in self.mirrors.at_work() {
                drawn.hold(thread, now);
            }
        }
        let watched = self.watched();
        if let Some(drawn) = &mut self.drawn {
            drawn.retain(&self.shared.open);
        }
        match &mut self.link {
            Link::Server(server) => {
                let mirrors = &self.mirrors;
                server.watch(&watched, |thread| mirrors.seq(thread));
                self.mirrors.watch_only(&watched);
            }
            Link::InProcess(engine) => {
                // As the Server streams a watch: what the Engine published
                // was taken this pump, so the history meets it exactly.
                let fresh: Vec<ThreadId> = watched
                    .iter()
                    .filter(|thread| !self.mirrors.watched(**thread))
                    .copied()
                    .collect();
                self.mirrors.watch_only(&watched);
                let boot = engine.boot();
                for thread in fresh {
                    if let Some(history) = engine.history(thread) {
                        let mirrors = &mut self.mirrors;
                        let _ = history.send(&mut |message| {
                            mirrors.receive(message, boot);
                            Ok(())
                        });
                    }
                }
            }
        }
    }

    /// Tell the Engine what changed of what only this window knows of this
    /// machine's Threads: the one its focused Pane shows while it has the
    /// operator's focus, and its Composers holding unsent text.
    pub(super) fn tell_window(
        &mut self,
        focused: Option<ThreadKey>,
        composing: &BTreeSet<ThreadKey>,
    ) {
        let mine = |key: ThreadKey| (key.machine() == self.id).then(|| key.thread());
        let now = Told {
            focused: focused.and_then(mine),
            composing: composing.iter().copied().filter_map(mine).collect(),
        };
        let before = self.told.replace(now.clone());
        let mut said = Vec::new();
        if before.as_ref().map(|told| told.focused) != Some(now.focused) {
            said.push(Op::Focused {
                thread: now.focused,
            });
        }
        let composed = before.map(|told| told.composing).unwrap_or_default();
        for (threads, unsent) in [
            (now.composing.difference(&composed), true),
            (composed.difference(&now.composing), false),
        ] {
            said.extend(threads.map(|thread| Op::Composing {
                thread: *thread,
                unsent,
            }));
        }
        for op in said {
            match &mut self.link {
                Link::InProcess(engine) => engine.window_says(IN_PROCESS_WINDOW, &op),
                Link::Server(server) => server.tell(op),
            }
        }
    }

    /// Every Thread its Server has, open or parked, owned ones included.
    pub(super) fn known(&self) -> impl Iterator<Item = ThreadId> + '_ {
        self.mirrors
            .threads()
            .into_iter()
            .chain(self.parked.thread_ids())
    }

    /// This computer's logs, when `thread` is open and its log is here.
    pub(super) fn open_log(&self, thread: ThreadId) -> Option<&Store> {
        self.logs
            .as_ref()
            .filter(|_| self.mirrors.view(thread).is_some())
    }

    pub(super) fn remote(&mut self) -> std::io::Result<&mut remote::Remote> {
        match &mut self.link {
            Link::Server(server) => Ok(server),
            Link::InProcess(_) => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "this window runs its agents itself",
            )),
        }
    }

    /// The Engine in this process: what only `--demo`, `--load` and the
    /// tests drive. It rings the bell, as an act does.
    pub(super) fn in_process(&mut self) -> &mut Engine {
        ferrite_core::doorbell::ring();
        match &mut self.link {
            Link::InProcess(engine) => engine,
            Link::Server(_) => panic!("only an Engine in this process takes this act"),
        }
    }

    /// A first send that settled, as the window names its Thread.
    pub(super) fn keyed(&self, result: BootstrapResult) -> BootstrapResult<ThreadKey> {
        BootstrapResult {
            draft: result.draft,
            prompt: result.prompt,
            result: result.result.map(|done| Bootstrapped {
                thread: self.key(done.thread),
            }),
        }
    }
}

/// Mirror parity (ADR 0025): after every message, each open Thread's
/// mirror reads exactly as the Engine's own Thread does. Every Cockpit
/// test checks it as it runs.
#[cfg(test)]
impl HeldMachine {
    pub(super) fn assert_mirrors_match(&self) {
        let Link::InProcess(engine) = &self.link else {
            return;
        };
        let engine: &Engine = engine;
        assert_eq!(self.shared.registry.to_wire(), engine.registry().to_wire());
        assert_eq!(
            self.shared.notifications.to_wire(),
            engine.notifications().to_wire()
        );
        for provider in [
            ferrite_core::provider::Provider::Claude,
            ferrite_core::provider::Provider::Codex,
        ] {
            assert_eq!(
                self.shared.announced_models(provider),
                engine.announced_models(provider)
            );
            assert_eq!(
                self.shared.live_sessions(provider),
                engine.live_sessions(provider)
            );
            assert_eq!(
                self.shared.account_limits(provider),
                engine.account_limits(provider)
            );
        }
        for &thread in engine.open_threads() {
            // A Thread this window does not stream reads from its digest.
            if !self.mirrors.watched(thread) {
                continue;
            }
            let engine = engine.thread(thread).expect("open in the Engine");
            let mirror = self
                .mirrors
                .view(thread)
                .unwrap_or_else(|| panic!("thread {thread} has no mirror"));
            assert_eq!(
                super::parity::seen!(mirror),
                super::parity::seen!(engine),
                "thread {thread}: the mirror drifted from the Engine"
            );
        }
    }
}

/// Where a machine's Engine runs.
pub(super) enum Link {
    /// In this process, pumped by the window.
    InProcess(Box<Engine>),
    /// In the Server, over its socket.
    Server(Box<remote::Remote>),
}

impl Link {
    /// One command, carried out where the Engine runs and answered as the
    /// Server answers it. In debug builds an in-process command and its
    /// reply cross as their encoded lines, so nothing escapes the contract.
    pub(super) fn execute(&mut self, op: Op) -> Executed {
        match self {
            Link::InProcess(engine) => replied(engine.execute(&commanded(op))),
            Link::Server(server) => server.execute(op),
        }
    }
}

/// An Op as the Server reads it off the socket.
pub(super) fn commanded(op: Op) -> Op {
    let ClientMessage::Command(Command { op, .. }) =
        crossed(ClientMessage::Command(Command { id: 0, op }))
    else {
        unreachable!("a command decodes as a command")
    };
    op
}

/// A command's outcome as the window reads the Server's reply.
pub(super) fn replied(executed: Executed) -> Executed {
    let (ok, error) = match executed {
        Ok(ok) => (ok, None),
        Err(error) => (None, Some(error)),
    };
    let ServerMessage::Reply { ok, error, .. } = crossed(ServerMessage::Reply { id: 0, ok, error })
    else {
        unreachable!("a reply decodes as a reply")
    };
    error.map_or(Ok(ok), Err)
}

/// What a command came to, as its reply says.
pub(super) type Executed = Result<Option<Done>, WireError>;

/// A message as the other end of a socket reads it: in debug builds, its
/// encoded line decoded again.
pub(super) fn crossed<T: serde::Serialize + serde::de::DeserializeOwned>(message: T) -> T {
    #[cfg(debug_assertions)]
    let message = {
        let line = ferrite_core::protocol::encode(&message);
        ferrite_core::protocol::decode(&line[..line.len() - 1])
            .expect("every message decodes as it encoded")
    };
    message
}

/// The Threads a window draws of one machine at Transcript level, and
/// since when each other it holds has been off screen and at rest.
#[derive(Default)]
pub(super) struct Drawn {
    now: BTreeSet<ThreadId>,
    left: HashMap<ThreadId, Instant>,
}

impl Drawn {
    pub(super) fn draws(&mut self, threads: BTreeSet<ThreadId>, at: Instant) {
        for &gone in self.now.difference(&threads) {
            self.left.insert(gone, at);
        }
        for thread in &threads {
            self.left.remove(thread);
        }
        self.now = threads;
    }

    /// Hold `thread` as if it had just left the screen: one that opened
    /// while the window is up, which the operator is about to look at, or
    /// one at work, whose time at rest starts again.
    fn hold(&mut self, thread: ThreadId, at: Instant) {
        if !self.now.contains(&thread) {
            self.left.insert(thread, at);
        }
    }

    /// Whether `thread` is drawn now.
    fn shows(&self, thread: ThreadId) -> bool {
        self.now.contains(&thread)
    }

    /// Whether `thread` is drawn, or left the screen within `UNWATCH_AFTER`.
    fn holds(&self, thread: ThreadId, at: Instant) -> bool {
        self.now.contains(&thread)
            || self
                .left
                .get(&thread)
                .is_some_and(|left| at.saturating_duration_since(*left) < UNWATCH_AFTER)
    }

    fn retain(&mut self, open: &[ThreadId]) {
        self.now.retain(|thread| open.contains(thread));
        self.left.retain(|thread, _| open.contains(thread));
    }
}

/// What only a window knows and the API reports (ADR 0030): the Thread it
/// shows the operator, and its Composers holding unsent text — of one
/// machine, as that machine's Server is told.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Told {
    focused: Option<ThreadId>,
    composing: BTreeSet<ThreadId>,
}
