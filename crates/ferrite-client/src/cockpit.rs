//! The Cockpit: one window's hold on the Engine (ADR 0025). What a window
//! keeps for itself — its Roster's focus and its Drafts,
//! whether it has the operator's focus — lives here; everything that must
//! keep running without a window is the Engine's, and the Cockpit changes
//! it only through the Engine's acts. The Engine runs in the Server, and
//! the acts are commands over its socket (`remote`); in `--demo`, `--load`
//! and every test it runs in this process, and an act is the same command,
//! answered by the same Engine code the Server runs (`Link::execute`).
//! Either way the window reads what the Engine's messages built: each open
//! Thread's mirror, and the shared state's copies. The open Threads are
//! shared: the Roster follows them (`sync_roster`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use ferrite_core::activity::{AgentKey, DecisionHandle, Subject};
pub use ferrite_core::engine::{
    title_from_prompt, Admission, AgentLaunch, Engine, HistoryDirection, Preparation,
    ProviderChoice, RssSampler, SpawnRequest, Spawner, SubjectUpdate, ToolTiming,
};
use ferrite_core::notifications::{DecisionNoticeId, NoticeId, Notifications};
use ferrite_core::protocol::{
    Done, ErrorCode, NoticeAction, Op, ProjectAction, ServerMessage, When, WireError,
};
use ferrite_core::roster::{DraftId, PaneIdentity, Roster};
use ferrite_core::store::{LoadError, Provider, Store};
use ferrite_core::transcript::Input;
use ferrite_core::workspace::registry::{ProjectId, Registry};
use ferrite_core::workspace::WorkspaceChoice;
use ferrite_core::{
    Decision, DecisionAnswer, MachineId, MachineKey, ModelInfo, ProjectKey, SavedThread, ThreadId,
    ThreadKey,
};
pub use ask::Ask;
use held::{commanded, replied, Drawn, Executed, HeldMachine, Link};
pub use mirror::ThreadView;
pub use remote::{Attach, Endpoint, ServerRestart, ServerState};
#[cfg(any(test, feature = "test-support"))]
pub use remote::waits;

mod ask;
mod factory_fixture;
mod held;
pub mod mirror;
mod parked;
mod remote;
mod shared;

/// What changed for one of the window's Panes in a frame, its Thread named
/// by its key.
pub type PaneUpdate = ferrite_core::engine::PaneUpdate<ThreadKey>;
/// A draft's first send, done: the Thread it became, by its key.
pub type Bootstrapped = ferrite_core::engine::Bootstrapped<ThreadKey>;
/// A first send settled by `pump`, its Thread named by its key.
pub type BootstrapResult = ferrite_core::engine::BootstrapResult<ThreadKey>;
/// A Session the watchdog replaced, its Thread named by its key.
pub type Restart = ferrite_core::engine::Restart<ThreadKey>;

/// One window's hold on every machine it holds (ADR 0045): each machine's
/// link and what its Engine said (`HeldMachine`), and what exists once per
/// window — the Roster, focus, the Subjects shown, the Composers holding
/// text. Every Thread is named by its `ThreadKey`, which says whose it is.
pub struct Cockpit {
    /// Every machine the window holds, by the id its keys carry: every
    /// one at once, each reached and kept apart by its own link.
    machines: BTreeMap<MachineId, HeldMachine>,
    /// What a reading of a machine the window does not hold reads: nothing.
    nothing: shared::Shared,
    /// What is on screen (#28): the roster of Panes, focus and park order. Read through `roster()`; changed only by the acts
    /// below, which keep it showing exactly the open Threads plus drafts.
    pub(crate) roster: Roster,
    /// What the machines' Servers said for the window to take as it comes.
    said: Said,
    /// What the model does of its own once a Server says an act was done.
    follows: Follows,
    /// A fixture world's Factory, answering its Factory asks as a
    /// Server's would; a real window has none.
    factory_fixture: Option<factory_fixture::FactoryFixture>,
    /// The Subject each Pane shows, where it is not Main: what this window
    /// reads when the operator looks at it.
    visible_subjects: BTreeMap<ThreadKey, Subject>,
    /// Whether the window has the operator's focus: what is on screen is
    /// seen only while it does (`acknowledge_seen`), so a finish or a
    /// request on screen stays unread until they come back (ADR 0017). True
    /// until the window says otherwise, so a headless Cockpit reads what it
    /// shows.
    window_active: bool,
    /// The Threads whose Composer in this window holds unsent text, as the
    /// window says (`set_composing`).
    composing: BTreeSet<ThreadKey>,
    /// The self-hosted forges a review's pull request is read from here,
    /// when this window runs its agents itself (`settings_changed`).
    forge: ferrite_reviewr::config::PluginConfig,
    /// What a fixture world's forge says (`fixture_forge`), by checkout:
    /// answered in place of its CLI. Empty outside fixtures.
    forge_fixture: HashMap<PathBuf, ferrite_reviewr::forge::PrView>,
    /// A fixture world's pull requests read by number, and its lists.
    forge_numbered: HashMap<(PathBuf, u64), ferrite_core::protocol::ReviewPullRequest>,
    forge_diffs: HashMap<(PathBuf, u64), Vec<ferrite_reviewr::ferrite::FileSides>>,
    forge_lists:
        HashMap<(PathBuf, ferrite_core::protocol::WirePrFilter), ferrite_core::protocol::ReviewPullRequests>,
    /// The checkouts whose whole-PR reads wait for a test's answer
    /// (`fixture_hold_pr_reads`), and the reads waiting.
    forge_held: std::collections::HashSet<PathBuf>,
    held_reads: Vec<HeldRead>,
    /// What a fixture world's disk says is free (`fixture_free_disk`):
    /// answered in place of the volume. `None` outside fixtures.
    free_disk_fixture: Option<u64>,
    /// A fixture world's Server accepts every browser act
    /// (`fixture_browser_accepts`), answering an empty browser.
    browser_fixture: bool,
}

/// What the model does of its own once an act is answered (ADR 0041): the
/// part of an act the window used to wait for that is the model's, not the
/// window's.
#[derive(Clone, Copy)]
enum Follow {
    /// A parked Thread leaves the roster, however its park went: its
    /// Session is gone either way.
    Parked(ThreadKey),
    /// A deleted Thread's parked row goes.
    Deleted(ThreadKey),
    /// A Thread brought back takes the screen, and the park order forgets
    /// it.
    Reopened(ThreadKey),
}

/// The acts a Server answered, each with whether it was done, for the
/// model to follow on the beat that brought the answer.
struct Follows {
    done: std::sync::mpsc::Sender<(Follow, bool)>,
    to_do: std::sync::mpsc::Receiver<(Follow, bool)>,
}

impl Default for Follows {
    fn default() -> Self {
        let (done, to_do) = std::sync::mpsc::channel();
        Self { done, to_do }
    }
}

/// What the machines' Servers said that the window takes as it comes, each
/// Thread in it named by its key.
#[derive(Default)]
struct Said {
    bootstrap_results: Vec<BootstrapResult>,
    /// The browser messages, each with the Thread its page is of.
    browser: Vec<(ThreadKey, ServerMessage)>,
    allowance_warnings: Vec<ferrite_core::providers::limits::AllowanceWarning>,
    /// What API callers asked of the windows, oldest first.
    asked: Vec<Asked>,
}

/// What one machine's Server said that every window shares — its registry,
/// its Notices, its Factory — read with the machine it is of, so an id read
/// in it names a Thread or Project of that machine and no other
/// (`key`, `project_key`). Its own names stay clear of the reading's, which
/// it derefs to.
pub struct OnMachine<'a, T> {
    machine: MachineId,
    reading: &'a T,
}

impl<'a, T> OnMachine<'a, T> {
    pub fn machine(&self) -> MachineId {
        self.machine
    }

    /// The Thread `thread` names in this reading.
    pub fn key(&self, thread: ThreadId) -> ThreadKey {
        ThreadKey::new(self.machine, thread)
    }

    /// The Project `project` names in this reading.
    pub fn project_key(&self, project: ProjectId) -> ProjectKey {
        ProjectKey::new(self.machine, project)
    }

    /// The reading itself, for as long as the Cockpit is borrowed.
    pub fn reading(&self) -> &'a T {
        self.reading
    }
}

impl<T> std::ops::Deref for OnMachine<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.reading
    }
}

impl<T> Clone for OnMachine<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for OnMachine<'_, T> {}

/// An act's reply as the text it carried, if it carried one.
fn text_of(done: Executed) -> io::Result<Option<String>> {
    Ok(match done {
        Ok(Some(Done::Text { text })) => Some(text),
        _ => None,
    })
}

/// An act on a machine the window no longer holds.
/// What became of a send, as its reply says.
fn admission(sent: Executed) -> Admission {
    match sent {
        Ok(Some(Done::Held)) => Admission::Held,
        Ok(_) => Admission::Delivered,
        Err(error) if error.code == ErrorCode::Unconfirmed => Admission::Unconfirmed(error.message),
        Err(error) => Admission::Refused(error.message),
    }
}

fn gone() -> WireError {
    WireError {
        code: ErrorCode::UnknownThread,
        message: "that machine is not held".to_string(),
    }
}

impl Cockpit {
    /// A window driving an Engine of its own, in this process: this
    /// computer.
    pub fn try_new(store: Store, spawner: Box<dyn Spawner>) -> io::Result<Self> {
        let mut cockpit = Self::empty();
        cockpit.hold_machine(Self::in_process_machine(store, spawner, MachineKey(None))?);
        Ok(cockpit)
    }

    /// An Engine of its own in this process, saved as `saved_as`.
    fn in_process_machine(
        store: Store,
        spawner: Box<dyn Spawner>,
        saved_as: MachineKey,
    ) -> io::Result<HeldMachine> {
        let logs = store.clone();
        let dir = store.dir().to_path_buf();
        let mut engine = Engine::try_new(store, spawner)?;
        // Its window pumps it: a park is listed by the pump that parked it.
        engine.read_parked_in_place();
        Ok(HeldMachine::new(
            Link::InProcess(Box::new(engine)),
            Some(logs),
            &dir,
            saved_as,
        ))
    }

    /// Test and embedding convenience. The application uses `try_new` so a
    /// protected or incompatible registry is reported instead of replaced.
    pub fn new(store: Store, spawner: Box<dyn Spawner>) -> Self {
        Self::try_new(store, spawner).expect("the workspace registry opens")
    }

    /// A window holding no machine: every one in Settings is turned off.
    /// It lists nothing and starts nothing until one is held.
    pub fn empty() -> Self {
        Self {
            machines: BTreeMap::new(),
            nothing: shared::Shared::new(Path::new("")),
            roster: Roster::default(),
            said: Said::default(),
            follows: Follows::default(),
            factory_fixture: None,
            visible_subjects: BTreeMap::new(),
            window_active: true,
            composing: BTreeSet::new(),
            forge: Default::default(),
            forge_fixture: HashMap::new(),
            forge_numbered: HashMap::new(),
            forge_diffs: HashMap::new(),
            forge_lists: HashMap::new(),
            forge_held: Default::default(),
            held_reads: Vec::new(),
            free_disk_fixture: None,
            browser_fixture: false,
        }
    }

    /// A window holding one machine's Server (`hold`).
    pub fn attach(attach: Attach) -> io::Result<Self> {
        let mut cockpit = Self::empty();
        cockpit.hold(attach)?;
        Ok(cockpit)
    }

    /// Hold another machine beside the rest: this computer's Server,
    /// starting one when none answers, or a machine's, connected to in the
    /// background. Without one it stays detached and keeps trying; it
    /// never runs the agents itself. Its parked Threads are what its Server
    /// publishes (ADR 0026), wherever the logs are. Its open Threads join
    /// the Roster as its Server says them, beside every other machine's.
    pub fn hold(&mut self, attach: Attach) -> io::Result<MachineId> {
        let local = matches!(attach.server, Endpoint::Local { .. });
        let logs = local.then(|| Store::reader(&attach.store)).transpose()?;
        let dir = attach.store.clone();
        let saved_as = attach.saved_as.clone();
        let mut remote = remote::Remote::new(attach);
        let arrived = if local { remote.attach_now() } else { None };
        let mut machine = HeldMachine::new(Link::Server(Box::new(remote)), logs, &dir, saved_as);
        // A window streams only what it draws, from its first attach on: it
        // draws nothing yet (ADR 0032).
        machine.drawn = Some(Drawn::default());
        let id = machine.id;
        self.machines.insert(id, machine);
        self.roster.set_start(self.start_machine());
        self.arrive(id, arrived.into_iter().collect());
        Ok(id)
    }

    fn hold_machine(&mut self, machine: HeldMachine) -> MachineId {
        let id = machine.id;
        self.machines.insert(id, machine);
        self.roster.set_start(self.start_machine());
        self.receive_from(id);
        id
    }

    /// Let `machine` go: turned off or removed in Settings. Its Panes leave
    /// the Roster and every key of it names nothing from now on; its
    /// Server keeps its Threads.
    pub fn let_go(&mut self, machine: MachineId) {
        let Some(held) = self.machines.remove(&machine) else {
            return;
        };
        self.roster.set_start(self.start_machine());
        for thread in &held.shared.open {
            self.roster.remove_thread(held.key(*thread));
        }
        let gone = |key: &ThreadKey| key.machine() == machine;
        self.visible_subjects.retain(|key, _| !gone(key));
        self.composing.retain(|key| !gone(key));
        self.said.browser.retain(|(key, _)| !gone(key));
        ferrite_core::doorbell::ring();
    }

    /// Every machine the window holds, in the order it took them.
    pub fn machines(&self) -> impl Iterator<Item = MachineId> + '_ {
        self.machines.keys().copied()
    }

    /// This computer, while the window holds it: where a new Thread with
    /// no Project starts, and whose files the window's own pickers read.
    /// `None` while it is turned off in Settings.
    pub fn this_computer(&self) -> Option<MachineId> {
        self.machine_saved_as(&MachineKey(None))
    }

    /// Where an act with no Thread or Project of its own goes (a new Thread
    /// before any Project, an import into no Pane), and the one machine
    /// whose Threads take an empty screen: this computer; with it turned
    /// off, the first machine the window holds.
    pub fn start_machine(&self) -> Option<MachineId> {
        self.this_computer()
            .or_else(|| self.machines.keys().next().copied())
    }

    /// `thread` as saved files name it, by its machine's saved name;
    /// `None` for a machine the window no longer holds.
    pub fn saved(&self, thread: ThreadKey) -> Option<SavedThread> {
        Some(SavedThread {
            machine: self.saved_as(thread.machine())?.clone(),
            thread: thread.thread(),
        })
    }

    /// What saved files name `machine`, fixed when the window attached it
    /// (F-A8); `None` for a machine the window no longer holds.
    pub fn saved_as(&self, machine: MachineId) -> Option<&MachineKey> {
        Some(&self.machines.get(&machine)?.saved_as)
    }

    /// The machine saved files call `saved`, while the window holds it.
    pub fn machine_saved_as(&self, saved: &MachineKey) -> Option<MachineId> {
        self.machines
            .values()
            .find(|machine| machine.saved_as == *saved)
            .map(|machine| machine.id)
    }

    /// The Thread a saved file names, if the window holds its machine.
    pub fn resolve(&self, saved: &SavedThread) -> Option<ThreadKey> {
        let machine = self.machine_saved_as(&saved.machine)?;
        Some(self.machines[&machine].key(saved.thread))
    }

    /// Project `project` of the machine saved as `machine`, if the window
    /// holds it.
    pub fn resolve_project(&self, machine: &MachineKey, project: ProjectId) -> Option<ProjectKey> {
        let machine = self.machine_saved_as(machine)?;
        Some(self.machines[&machine].project_key(project))
    }

    /// Whether `machine` is this computer, reached: what it names on disk
    /// is this computer's. Another machine's never stands in local files
    /// or defaults for its own (ADR 0026).
    pub fn local(&self, machine: MachineId) -> bool {
        self.machines.get(&machine).is_some_and(HeldMachine::local)
    }

    /// Whether `machine`'s Server has said which Threads it has: until it
    /// has, a Thread missing from `known_threads` is no deletion.
    pub fn heard(&self, machine: MachineId) -> bool {
        self.machines
            .get(&machine)
            .is_some_and(|machine| machine.known().next().is_some())
    }

    /// `machine`, held, or why not.
    fn held_mut(&mut self, machine: MachineId) -> io::Result<&mut HeldMachine> {
        self.machines
            .get_mut(&machine)
            .ok_or_else(|| io::Error::from(gone()))
    }

    /// The machine `key` is on, and its Thread there: the one place a key
    /// is split. `None` once the window no longer holds that machine.
    fn at(&self, key: ThreadKey) -> Option<(&HeldMachine, ThreadId)> {
        Some((self.machines.get(&key.machine())?, key.thread()))
    }

    fn at_mut(&mut self, key: ThreadKey) -> Option<(&mut HeldMachine, ThreadId)> {
        Some((self.machines.get_mut(&key.machine())?, key.thread()))
    }

    /// What `machine`'s Server shares, as its messages left it; nothing for
    /// a machine the window does not hold.
    fn shared(&self, machine: MachineId) -> &shared::Shared {
        self.machines
            .get(&machine)
            .map_or(&self.nothing, |held| &held.shared)
    }

    /// A reading of `machine`'s shared state.
    fn reading<'a, T>(
        &'a self,
        machine: MachineId,
        read: impl FnOnce(&'a shared::Shared) -> &'a T,
    ) -> OnMachine<'a, T> {
        OnMachine {
            machine,
            reading: read(self.shared(machine)),
        }
    }

    /// Window adapter receives metadata and actions for its exact owned
    /// pages, each with the Thread its page is of.
    pub fn take_browser_messages(&mut self) -> Vec<(ThreadKey, ServerMessage)> {
        std::mem::take(&mut self.said.browser)
    }

    // A Browser's asks of the Server (ADR 0041, 0036): each returns at once
    // and is answered through its `Ask` by the pump, so the window's thread
    // never waits on the Server for its pages.

    pub fn browser_attach(&mut self, thread: ThreadKey) -> Ask<ferrite_core::browser::State> {
        self.browser_op(thread, |thread| Op::BrowserAttach { thread })
    }

    pub fn browser(
        &mut self,
        thread: ThreadKey,
        action: ferrite_core::browser::Action,
    ) -> Ask<ferrite_core::browser::State> {
        self.browser_op(thread, |thread| Op::Browser { thread, action })
    }

    fn browser_op(
        &mut self,
        key: ThreadKey,
        op: impl FnOnce(ThreadId) -> Op,
    ) -> Ask<ferrite_core::browser::State> {
        if self.browser_fixture {
            return Ask::answered(Ok(ferrite_core::browser::State::default()));
        }
        self.browser_ask(key, op, |done| match done {
            Some(Done::Browser { state }) => Ok(state),
            _ => Err(io::Error::other(
                "The Server does not support the Conversation browser; update Ferrite",
            )),
        })
    }

    /// One Browser ask of the Server; an Engine in this process has no
    /// Browser, which its `Ask` answers at once.
    fn browser_ask<T: Send + 'static>(
        &mut self,
        key: ThreadKey,
        op: impl FnOnce(ThreadId) -> Op,
        read: fn(Option<Done>) -> io::Result<T>,
    ) -> Ask<T> {
        match self.remote_of(key) {
            Ok(remote) => remote.ask_in_time(op(key.thread()), read),
            Err(error) => Ask::answered(Err(error)),
        }
    }

    /// One Browser act whose reply nobody reads: the Server's broadcasts
    /// say what it changed.
    fn browser_tell(&mut self, key: ThreadKey, op: impl FnOnce(ThreadId) -> Op) {
        if let Ok(remote) = self.remote_of(key) {
            remote.tell(op(key.thread()));
        }
    }

    pub fn browser_capture(
        &mut self,
        thread: ThreadKey,
        owner: ferrite_core::browser::Owner,
        lease: ferrite_core::browser::Lease,
        capture: ferrite_core::browser_annotations::Capture,
        png: Option<ferrite_core::protocol::WirePng>,
    ) -> Ask<ferrite_core::browser_annotations::Annotation> {
        match self
            .remote_of(thread)
            .and_then(|remote| remote.browser_capture(thread.thread(), owner, lease, capture, png))
        {
            Ok(answer) => Ask::of(answer),
            Err(error) => Ask::answered(Err(error)),
        }
    }

    /// Image reads share the existing asynchronous Server read path. The
    /// returned display cache never replaces the Server-owned provider path.
    pub fn browser_annotation_image(
        &mut self,
        thread: ThreadKey,
        annotation: ferrite_core::browser_annotations::Annotation,
    ) -> Ask<PathBuf> {
        let Some((machine, thread)) = self.at_mut(thread) else {
            return Ask::answered(Err(gone().into()));
        };
        match &mut machine.link {
            Link::Server(server) => {
                let Some(hash) = annotation.original_hash() else {
                    return Ask::answered(Err(io::Error::other("This annotation has no screenshot")));
                };
                let image = match server.browser_capture_image(thread, hash.to_owned()) {
                    Ok(image) => image,
                    Err(error) => return Ask::answered(Err(error)),
                };
                Ask::of(read_here(move || {
                    let png = image
                        .recv_timeout(std::time::Duration::from_secs(30))
                        .map_err(|error| error.to_string())?
                        .map_err(|error| error.to_string())?;
                    annotation
                        .cache_wire_image(&png)
                        .map_err(|error| error.to_string())
                }))
            }
            Link::InProcess(_) => Ask::of(read_here(move || {
                annotation
                    .local_image()
                    .and_then(|bytes| annotation.cache_image(&bytes))
                    .map_err(|error| error.to_string())
            })),
        }
    }

    pub fn browser_protect(
        &mut self,
        thread: ThreadKey,
        owner: ferrite_core::browser::Owner,
        document: ferrite_core::browser::Document,
        protection: ferrite_core::browser::Protection,
    ) -> Ask<ferrite_core::browser::Lease> {
        self.browser_ask(
            thread,
            |thread| Op::BrowserProtect {
                thread,
                owner,
                document,
                protection,
            },
            |done| match done {
                Some(Done::BrowserProtection { lease }) => Ok(lease),
                _ => Err(io::Error::other(
                    "The Server could not protect this browser page",
                )),
            },
        )
    }

    pub fn browser_release(
        &mut self,
        thread: ThreadKey,
        owner: ferrite_core::browser::Owner,
        lease: ferrite_core::browser::Lease,
    ) {
        self.browser_tell(thread, |thread| Op::BrowserRelease {
            thread,
            owner,
            lease,
        });
    }

    pub fn browser_reclaim(
        &mut self,
        thread: ThreadKey,
        owner: ferrite_core::browser::Owner,
        document: ferrite_core::browser::Document,
    ) -> Ask<Option<ferrite_core::browser::Lease>> {
        self.browser_ask(
            thread,
            |thread| Op::BrowserReclaim {
                thread,
                owner,
                document,
            },
            |done| match done {
                Some(Done::BrowserReclaim { lease }) => Ok(lease),
                _ => Err(io::Error::other(
                    "The Server could not reserve this page for unloading",
                )),
            },
        )
    }

    pub fn browser_reclaimed(
        &mut self,
        thread: ThreadKey,
        owner: ferrite_core::browser::Owner,
        lease: ferrite_core::browser::Lease,
        released: bool,
    ) -> Ask<ferrite_core::browser::State> {
        self.browser_op(thread, |thread| Op::BrowserReclaimed {
            thread,
            owner,
            lease,
            released,
        })
    }

    pub fn browser_page(
        &mut self,
        thread: ThreadKey,
        owner: ferrite_core::browser::Owner,
        page: ferrite_core::browser::PageState,
    ) -> Ask<ferrite_core::browser::State> {
        self.browser_op(thread, |thread| Op::BrowserPage {
            thread,
            owner,
            page,
        })
    }

    pub fn browser_dispatch(
        &mut self,
        request: u64,
        boot_id: String,
        thread: ThreadKey,
        owner: ferrite_core::browser::Owner,
        document: ferrite_core::browser::Document,
        revision: u64,
    ) -> Ask<u64> {
        match self.remote_of(thread).and_then(|remote| {
            remote.browser_dispatch(request, boot_id, thread.thread(), owner, document, revision)
        }) {
            Ok(answer) => Ask::of(answer),
            Err(error) => Ask::answered(Err(error)),
        }
    }

    pub fn browser_reply(
        &mut self,
        request: u64,
        boot_id: Option<String>,
        thread: ThreadKey,
        owner: ferrite_core::browser::Owner,
        document: ferrite_core::browser::Document,
        revision: u64,
        outcome: ferrite_core::browser::Outcome,
    ) {
        if let Ok(remote) = self.remote_of(thread) {
            remote.say(&ferrite_core::protocol::ClientMessage::BrowserReply {
                request,
                boot_id,
                thread: thread.thread(),
                owner,
                document,
                revision,
                outcome,
            });
        }
    }

    pub fn browser_settled(
        &mut self,
        thread: ThreadKey,
        owner: ferrite_core::browser::Owner,
        document: ferrite_core::browser::Document,
        revision: u64,
    ) {
        self.browser_tell(thread, |thread| Op::BrowserSettled {
            thread,
            owner,
            document,
            revision,
        });
    }

    pub fn browser_input(
        &mut self,
        thread: ThreadKey,
        owner: ferrite_core::browser::Owner,
        document: ferrite_core::browser::Document,
    ) -> Ask<u64> {
        self.browser_ask(
            thread,
            |thread| Op::BrowserInput {
                thread,
                owner,
                document,
            },
            |done| match done {
                Some(Done::BrowserInput { revision }) => Ok(revision),
                _ => Err(io::Error::other(
                    "The Server did not authorize browser input",
                )),
            },
        )
    }

    /// Pump until `ask` is answered: a test's or a driver's way to take an
    /// answer in one step, which the window never does.
    #[cfg(any(test, feature = "test-support"))]
    pub fn wait_for<T>(&mut self, mut ask: Ask<T>) -> io::Result<T> {
        let mut answer = None;
        ferrite_core::doorbell::until("an ask was never answered", || {
            self.receive();
            answer = ask.answer();
            answer.is_some()
        });
        answer.expect("answered")
    }

    /// The Server `machine` is attached to, or why it is not; `None` for
    /// an Engine in this process, or a machine the window does not hold.
    pub fn server(&self, machine: MachineId) -> Option<ServerState> {
        match &self.machines.get(&machine)?.link {
            Link::InProcess(_) => None,
            Link::Server(server) => Some(server.state()),
        }
    }

    /// Whether the window holds any machine: with every one turned off in
    /// Settings it has nowhere to start a Thread.
    pub fn environment_available(&self) -> bool {
        !self.machines.is_empty()
    }

    /// The bridge `machine` is reached through; `None` for this computer.
    pub fn bridge(&self, machine: MachineId) -> Option<&std::sync::Arc<ferrite_core::machine::Bridge>> {
        match &self.machines.get(&machine)?.link {
            Link::InProcess(_) => None,
            Link::Server(server) => server.bridge(),
        }
    }

    /// Attach `machine` again now, rather than at the next retry.
    pub fn retry_server(&mut self, machine: MachineId) {
        if let Some(Link::Server(server)) = self.machines.get_mut(&machine).map(|held| &mut held.link) {
            server.retry();
        }
    }

    /// Stop `machine`'s Server — at once, or once every agent is idle.
    pub fn stop_server(&mut self, machine: MachineId, when: When) -> io::Result<()> {
        self.server_command(machine, Op::Stop { when })
    }

    /// Stop `machine`'s Server at once and wait, up to `within`, for it to
    /// be gone: an installer is about to replace the executable it runs.
    pub fn stop_server_and_wait(&mut self, machine: MachineId, within: Duration) -> io::Result<()> {
        let deadline = Instant::now() + within;
        let server = self.held_mut(machine)?.remote()?;
        server
            .execute(Op::Stop { when: When::Now })
            .map_err(io::Error::from)?;
        server.wait_gone(deadline)
    }

    /// Restart `machine`'s Server into its executable as it is on disk now
    /// — at once, or once every agent is idle — and resume every open
    /// Thread.
    pub fn restart_server(&mut self, machine: MachineId, when: When) -> io::Result<()> {
        // Asked again while one is under way: that one is the restart.
        if matches!(self.server_restart(machine), Some(ServerRestart::Restarting)) {
            return Ok(());
        }
        let result = self.server_command(machine, Op::Restart { when });
        if let Ok(server) = self.held_mut(machine).and_then(HeldMachine::remote) {
            match &result {
                Ok(()) => server.restarting(),
                Err(error) => server.restart_refused(error.to_string()),
            }
        }
        result
    }

    /// A restart of `machine`'s Server, as far as this window has seen it
    /// through: asked for or heard of, back on a new boot, or failed and
    /// why.
    pub fn server_restart(&self, machine: MachineId) -> Option<&ServerRestart> {
        match &self.machines.get(&machine)?.link {
            Link::Server(server) => server.restart(),
            Link::InProcess(_) => None,
        }
    }

    fn server_command(&mut self, machine: MachineId, op: Op) -> io::Result<()> {
        self.held_mut(machine)?
            .remote()?
            .execute(op)
            .map(drop)
            .map_err(io::Error::from)
    }

    /// The Server `key`'s machine is attached to.
    fn remote_of(&mut self, key: ThreadKey) -> io::Result<&mut remote::Remote> {
        self.machines
            .get_mut(&key.machine())
            .ok_or_else(|| io::Error::from(gone()))?
            .remote()
    }

    /// One act where `machine`'s Engine runs, as a command: the Engine in
    /// this process answers it as the Server's does. Either way the window
    /// is up to date with what it changed when it returns.
    /// Every act rings the window's bell: what it moved is the pump's to
    /// read, as an arrival is (ADR 0041).
    fn act_at(&mut self, machine: MachineId, op: Op) -> Executed {
        let executed = match self.machines.get_mut(&machine) {
            Some(held) => held.link.execute(op),
            None => Err(gone()),
        };
        self.receive_from(machine);
        ferrite_core::doorbell::ring();
        executed
    }

    /// An act on one Thread, where its machine's Engine runs.
    fn act_on(&mut self, key: ThreadKey, op: impl FnOnce(ThreadId) -> Op) -> Executed {
        self.act_at(key.machine(), op(key.thread()))
    }

    /// One act of `machine`'s whose reply nobody reads (ADR 0041): a
    /// Server is told and what it changed arrives as its messages do, so
    /// the window never waits for it. An Engine in this process carries it
    /// out now.
    fn tell_at(&mut self, machine: MachineId, op: Op) {
        let Some(held) = self.machines.get_mut(&machine) else {
            return;
        };
        match &mut held.link {
            Link::InProcess(_) => said(self.act_at(machine, op)),
            Link::Server(remote) => {
                remote.tell(op);
                ferrite_core::doorbell::ring();
            }
        }
    }

    /// One act on a Thread whose reply the window reads (ADR 0041): a
    /// Server is asked and answers through the `Ask`, so the window never
    /// waits for it. An Engine in this process carries it out now, and the
    /// `Ask` is answered already.
    fn ask_on<T: Send + 'static>(
        &mut self,
        key: ThreadKey,
        op: impl FnOnce(ThreadId) -> Op,
        read: fn(Executed) -> io::Result<T>,
    ) -> Ask<T> {
        self.ask_at(key.machine(), op(key.thread()), read)
    }

    /// `ask_on` for an act of `machine`'s that is on no one Thread.
    fn ask_at<T: Send + 'static>(
        &mut self,
        machine: MachineId,
        op: Op,
        read: impl FnOnce(Executed) -> io::Result<T> + Send + 'static,
    ) -> Ask<T> {
        match self.machines.get_mut(&machine).map(|held| &mut held.link) {
            Some(Link::Server(remote)) => {
                let ask = remote.ask_act(op, read);
                ferrite_core::doorbell::ring();
                ask
            }
            _ => Ask::answered(read(self.act_at(machine, op))),
        }
    }

    /// `ask_on`, and the model does `follow` itself when the act was done:
    /// before the window reads the answer, on the beat that brings it.
    fn ask_on_then<T: Send + 'static>(
        &mut self,
        key: ThreadKey,
        op: impl FnOnce(ThreadId) -> Op,
        read: fn(Executed) -> io::Result<T>,
        follow: Follow,
    ) -> Ask<T> {
        let (machine, op) = (key.machine(), op(key.thread()));
        match self.machines.get_mut(&machine).map(|held| &mut held.link) {
            Some(Link::Server(remote)) => {
                let done = self.follows.done.clone();
                let ask = remote.ask_act(op, move |reply| {
                    let answer = read(reply);
                    let _ = done.send((follow, answer.is_ok()));
                    answer
                });
                ferrite_core::doorbell::ring();
                ask
            }
            _ => {
                let answer = read(self.act_at(machine, op));
                self.follow(follow, answer.is_ok());
                Ask::answered(answer)
            }
        }
    }

    /// Follow each act a Server has answered.
    fn follow_answered(&mut self) {
        while let Ok((follow, done)) = self.follows.to_do.try_recv() {
            self.follow(follow, done);
        }
    }

    fn follow(&mut self, follow: Follow, done: bool) {
        match follow {
            // A Thread still open is another window's to show.
            Follow::Parked(thread) => {
                if !self.is_open(thread) {
                    self.roster.remove_thread(thread);
                }
            }
            // Its parked row goes at once, not when the Server next
            // publishes the rows.
            Follow::Deleted(thread) if done => {
                if let Some((machine, thread)) = self.at_mut(thread) {
                    machine.parked.forget(thread);
                }
            }
            Follow::Reopened(thread) if done => {
                self.roster.note_revived(thread);
                self.roster.focus(PaneIdentity::Thread(thread));
            }
            Follow::Deleted(_) | Follow::Reopened(_) => {}
        }
    }

    /// A tell on one Thread, to its machine.
    fn tell_on(&mut self, key: ThreadKey, op: impl FnOnce(ThreadId) -> Op) {
        self.tell_at(key.machine(), op(key.thread()))
    }

    /// The first machine the window holds: what a fixture world with one
    /// machine names when it names none.
    fn first(&self) -> MachineId {
        *self.machines.keys().next().expect("the window holds a machine")
    }

    /// The Engine in this process of the first machine the window holds:
    /// what only `--demo`, `--load` and the tests drive — fixtures,
    /// injected inputs, the launch-time revival a Server does for itself.
    /// It rings the bell, as an act does.
    fn in_process(&mut self) -> &mut Engine {
        let first = self.first();
        self.machines.get_mut(&first).expect("held").in_process()
    }

    /// The Engine in this process that `key`'s Thread runs in.
    fn in_process_of(&mut self, key: ThreadKey) -> &mut Engine {
        self.machines
            .get_mut(&key.machine())
            .expect("the Thread's machine is held")
            .in_process()
    }

    /// Bring this window up to every machine's Engine.
    fn receive(&mut self) {
        let machines: Vec<MachineId> = self.machines.keys().copied().collect();
        for machine in machines {
            self.receive_from(machine);
        }
    }

    /// Bring this window up to `machine`'s Engine: its mirrors and copies
    /// from the messages it published, then the Roster. What they moved
    /// waits for the pump to report, wherever they were received: a
    /// Server's message can arrive during any of the window's own acts, and
    /// its Pane update is the only word the window's Facts get of it (a PR
    /// watch's digest). In debug builds each in-process message crosses as
    /// its encoded line, so nothing escapes the contract.
    fn receive_from(&mut self, machine: MachineId) {
        let Some(held) = self.machines.get_mut(&machine) else {
            return;
        };
        let arrived = held.take_arrivals();
        self.arrive(machine, arrived);
        #[cfg(test)]
        self.machines[&machine].assert_mirrors_match();
    }

    /// A message a fixture world's Server (its first machine's) would have
    /// sent.
    fn receive_fixture(&mut self, message: ServerMessage) {
        // It rings as a Server's message does (ADR 0041).
        ferrite_core::doorbell::ring();
        self.arrive(self.first(), vec![remote::Arrival::Message(message)]);
    }

    fn arrive(&mut self, machine: MachineId, arrived: Vec<remote::Arrival>) {
        let Some(held) = self.machines.get_mut(&machine) else {
            return;
        };
        let settled = held.arrive(arrived, &mut self.said);
        // A settled first send completes its Draft in this window — its
        // slot becomes the Thread — once the window holds what the Engine
        // said before it.
        for result in settled {
            let mut result = held.keyed(result);
            if let (Some(draft), Ok(done)) = (result.draft, &result.result) {
                let thread = done.thread;
                result.result = Ok(complete_draft(&mut self.roster, draft, thread));
            }
            self.said.bootstrap_results.push(result);
        }
        held.sync_roster(&mut self.roster);
        self.follow_answered();
    }

    /// The window draws exactly these Threads at Transcript level now. It
    /// streams them whole while it does, and each other of this computer's
    /// while it is at work and for `UNWATCH_AFTER` after (ADR 0032; F-A2).
    pub fn draws_transcripts(&mut self, threads: impl IntoIterator<Item = ThreadKey>) {
        let threads: Vec<ThreadKey> = threads.into_iter().collect();
        let at = ferrite_core::clock::instant();
        for machine in self.machines.values_mut() {
            let drawn = threads
                .iter()
                .filter(|key| key.machine() == machine.id)
                .map(|key| key.thread())
                .collect();
            machine
                .drawn
                .get_or_insert_with(Drawn::default)
                .draws(drawn, at);
        }
    }

    /// One frame: each Engine's pump, then what changed for each Pane of
    /// this window, as its mirrors saw it.
    pub fn pump(&mut self) -> Vec<PaneUpdate> {
        for machine in self.machines.values_mut() {
            // Fold highlighter answers from the previous frame first, as
            // the Engine does for its own.
            machine.mirrors.drain_highlights();
            if let Link::InProcess(engine) = &mut machine.link {
                engine.pump();
            }
        }
        self.receive();
        // What is on screen while the window has focus is seen. Here
        // rather than only at the focus doors, so a Thread that finishes in
        // view, or a focus moved by a close, reads the same.
        self.acknowledge_seen();
        self.tell_window();
        self.machines
            .values_mut()
            .flat_map(|machine| machine.take_updates().collect::<Vec<_>>())
            .collect()
    }

    /// Whether this window's Composer for `thread` holds text not yet sent:
    /// the window says so as it changes, and the API reports it as the
    /// Thread's `draft` (ADR 0030), so nothing types over the operator.
    pub fn set_composing(&mut self, thread: ThreadKey, unsent: bool) {
        let changed = if unsent {
            self.composing.insert(thread)
        } else {
            self.composing.remove(&thread)
        };
        // The pump tells the Engine on its next beat (ADR 0041).
        if changed {
            ferrite_core::doorbell::ring();
        }
    }

    /// Tell each machine's Engine what changed of what only this window
    /// knows: the Thread its focused Pane shows while it has the operator's
    /// focus, and its Composers holding unsent text.
    fn tell_window(&mut self) {
        // A Pane that left took its Composer, and the unsent text with it:
        // a Thread revived later starts with an empty one.
        let roster = &self.roster;
        self.composing
            .retain(|thread| roster.pane_of(*thread).is_some());
        let focused = self
            .window_active
            .then(|| self.roster.focused_thread())
            .flatten();
        for machine in self.machines.values_mut() {
            machine.tell_window(focused, &self.composing);
        }
    }

    /// An open Thread's facts, as this window's mirror holds them.
    pub fn thread(&self, thread: ThreadKey) -> Option<ThreadView<'_>> {
        let (machine, thread) = self.at(thread)?;
        machine.mirrors.view(thread)
    }

    /// Whether `thread` is open on its machine.
    fn is_open(&self, thread: ThreadKey) -> bool {
        self.at(thread)
            .is_some_and(|(machine, thread)| machine.shared.open.contains(&thread))
    }

    /// Close a Pane: the Session ends, the log is flushed, and the Thread
    /// keeps nothing in memory until it is opened again.
    /// An `Err` answer says why it did not park cleanly. The window never
    /// waits for it.
    pub fn park(&mut self, thread: ThreadKey) -> Ask<()> {
        self.ask_on_then(thread, |thread| Op::Park { thread }, done, Follow::Parked(thread))
    }

    pub fn ensure_subject_history(
        &mut self,
        thread: ThreadKey,
        subject: &Subject,
    ) -> io::Result<bool> {
        self.load_subject(thread, subject, false)
    }

    pub fn subject_history_error(&self, thread: ThreadKey, subject: &Subject) -> Option<&str> {
        let (machine, thread) = self.at(thread)?;
        machine.mirrors.history_error(thread, subject)
    }

    pub fn retry_subject_history(
        &mut self,
        thread: ThreadKey,
        subject: &Subject,
    ) -> io::Result<bool> {
        self.load_subject(thread, subject, true)
    }

    fn load_subject(
        &mut self,
        thread: ThreadKey,
        subject: &Subject,
        retry: bool,
    ) -> io::Result<bool> {
        changed(self.act_on(thread, |thread| Op::LoadSubject {
            thread,
            subject: subject.into(),
            retry,
        }))
    }

    /// CLI session files to adopt into `machine`, found off the UI thread.
    /// Discovery reads the operator's own files: a window does it, wherever
    /// its Engine runs.
    pub fn discover_sessions(
        &mut self,
        machine: MachineId,
        roots: Vec<(Provider, PathBuf)>,
        cap: usize,
    ) -> Option<Receiver<io::Result<Vec<ferrite_core::import::Candidate>>>> {
        match &mut self.machines.get_mut(&machine)?.link {
            Link::InProcess(engine) => engine.discover_sessions(roots, cap),
            Link::Server(server) => server.discovery().discover_sessions(roots, cap),
        }
    }

    /// The operator opened a Notice of `machine`'s — from a toast or the
    /// bell: it is read, and they land on its Thread's Pane in the view
    /// that shows it, reviving a parked one. The Thread landed on, with
    /// the ask that says whether it took the screen, or None when the
    /// Notice is gone.
    pub fn open_notice(&mut self, machine: MachineId, id: NoticeId) -> Option<(ThreadKey, Ask<()>)> {
        let thread = self.shared(machine).notifications.get(id)?.thread;
        self.tell_at(
            machine,
            Op::Notice {
                action: NoticeAction::Open { notice: id.get() },
            },
        );
        Some(self.land_on(ThreadKey::new(machine, thread)))
    }

    /// Open one live Decision attention record of `machine`'s: its Subject
    /// is shown, and the operator lands on its Thread as `open_notice`
    /// does.
    pub fn open_decision_notice(
        &mut self,
        machine: MachineId,
        id: &DecisionNoticeId,
    ) -> Option<(ThreadKey, Ask<()>)> {
        self.shared(machine).notifications.decision(id)?;
        self.tell_at(
            machine,
            Op::Notice {
                action: NoticeAction::OpenDecision {
                    thread: id.thread,
                    handle: (&id.handle).into(),
                },
            },
        );
        Some(self.land_on(ThreadKey::new(machine, id.thread)))
    }

    /// Land on `thread`, reviving a parked one: the ask says whether it
    /// took the screen. What is on screen then is seen by the pump.
    fn land_on(&mut self, thread: ThreadKey) -> (ThreadKey, Ask<()>) {
        let landed = if self.focus_thread(thread) {
            Ask::answered(Ok(()))
        } else {
            self.reopen(thread)
        };
        self.acknowledge_seen();
        (thread, landed)
    }

    /// Whether the operator is looking: the window has the system's focus.
    pub fn window_active(&self) -> bool {
        self.window_active
    }

    /// The window gained or lost the operator's focus. Gaining it, what
    /// is on screen is seen at once: true when that read anything.
    pub fn set_window_active(&mut self, active: bool) -> bool {
        if self.window_active == active {
            return false;
        }
        self.window_active = active;
        // The pump tells the Engine which Thread the operator is looking
        // at on its next beat (ADR 0041).
        ferrite_core::doorbell::ring();
        self.acknowledge_seen()
    }

    /// Everything on screen while the window has focus is seen: each
    /// visible Thread's Notices are read, and so are the live Decisions of
    /// the Subject its Pane shows. True when anything was read.
    fn acknowledge_seen(&mut self) -> bool {
        if !self.window_active {
            return false;
        }
        // Only what would read anything is sent: this runs every frame.
        let unread: Vec<(ThreadKey, Vec<Unread>)> = self
            .visible()
            .into_iter()
            .filter_map(PaneIdentity::thread)
            .map(|thread| (thread, self.unread(thread)))
            .collect();
        for machine in self.machines.values_mut() {
            let id = machine.id;
            machine.seen_told.retain(|thread, _| {
                unread.iter().any(|(shown, unread)| {
                    shown.machine() == id && shown.thread() == *thread && !unread.is_empty()
                })
            });
        }
        let mut read = false;
        for (key, unread) in unread {
            if unread.is_empty() {
                continue;
            }
            let Some((machine, thread)) = self.at_mut(key) else {
                continue;
            };
            let op = Op::Notice {
                action: NoticeAction::Seen { thread },
            };
            match &mut machine.link {
                Link::InProcess(_) => {
                    notice(machine.link.execute(op));
                    read = true;
                }
                // A Server is told once for what is unread: its answer is
                // the Notices it sends back read, a beat or more later.
                Link::Server(remote) => {
                    let told = machine.seen_told.get(&thread);
                    if !remote.reaches_server()
                        || told.is_some_and(|told| unread.iter().all(|one| told.contains(one)))
                    {
                        continue;
                    }
                    remote.tell(op);
                    machine.seen_told.insert(thread, unread);
                }
            }
        }
        if read {
            self.receive();
        }
        read
    }

    /// What looking at `key`'s Thread reads: its unread Notices, and the
    /// unread requests of the Subject its Pane shows.
    fn unread(&self, key: ThreadKey) -> Vec<Unread> {
        let Some((machine, thread)) = self.at(key) else {
            return Vec::new();
        };
        let notifications = &machine.shared.notifications;
        let shown = self
            .visible_subjects
            .get(&key)
            .and_then(|subject| {
                let view = machine.mirrors.view(thread)?.activity();
                let subject = view.canonical_subject(subject);
                view.subject(&subject).is_some().then_some(subject)
            })
            .unwrap_or(Subject::Main);
        notifications
            .notices()
            .filter(|notice| notice.thread == thread && !notice.read)
            .map(|notice| Unread::Notice(notice.id))
            .chain(
                notifications
                    .decisions()
                    .filter(|notice| {
                        notice.id.thread == thread
                            && !notice.read
                            && notice.subject.as_ref() == Some(&shown)
                    })
                    .map(|notice| Unread::Decision(notice.id.clone())),
            )
            .collect()
    }
}

/// One thing on screen the operator has not read (`Cockpit::unread`).
#[derive(PartialEq)]
enum Unread {
    Notice(NoticeId),
    Decision(DecisionNoticeId),
}

/// The first send made a Thread of `draft`: its slot becomes the Thread.
fn complete_draft(roster: &mut Roster, draft: DraftId, thread: ThreadKey) -> Bootstrapped {
    roster.draft_became(draft, thread);
    Bootstrapped { thread }
}

/// A git read on a thread of its own, answered as a Server's would be.
fn read_here<T: Send + 'static>(
    read: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Receiver<io::Result<T>> {
    let (answer, answered) = ferrite_core::doorbell::channel();
    std::thread::spawn(move || {
        let _ = answer.send(read().map_err(io::Error::other));
    });
    answered
}

/// A whole-PR read a fixture world holds (`Cockpit::fixture_hold_pr_reads`):
/// the window waits on it until a test answers.
pub struct HeldRead {
    pub path: PathBuf,
    answer: ferrite_core::doorbell::Sender<io::Result<ferrite_core::protocol::ReviewPullRequest>>,
}

impl HeldRead {
    pub fn answer(self, read: ferrite_core::protocol::ReviewPullRequest) {
        let _ = self.answer.send(Ok(read));
    }

    pub fn fail(self, why: &str) {
        let _ = self.answer.send(Err(io::Error::other(why.to_string())));
    }
}

/// A command whose reply carries nothing the window needs.
fn done(executed: Executed) -> io::Result<()> {
    executed.map(drop).map_err(io::Error::from)
}

/// A command whose reply says whether it changed anything.
fn changed(executed: Result<Option<Done>, WireError>) -> io::Result<bool> {
    match executed.map_err(io::Error::from)? {
        Some(Done::Changed { changed }) => Ok(changed),
        _ => Ok(false),
    }
}

/// A Notice act. Its reply carries nothing: what it changed is the next
/// `notices` message.
fn notice(executed: Executed) {
    if let Err(error) = executed {
        ferrite_core::log!("a notice act did not reach the server: {}", error.message);
    }
}

/// Mirror parity (ADR 0025): after every message, each open Thread's
/// mirror reads exactly as the Engine's own Thread does. Every Cockpit
/// test checks it as it runs.
#[cfg(feature = "test-support")]
impl Cockpit {
    /// A UI test's park that fails: the Engine reports this Thread's park
    /// as refused.
    pub fn refuse_park(&mut self, thread: ThreadKey) {
        self.in_process_of(thread).refuse_park(thread.thread());
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Cockpit {
    /// Thread `thread` of the first machine the window holds, for a test
    /// that names one by its number, or made it in a Store before the
    /// Cockpit held it.
    pub fn key(&self, thread: ThreadId) -> ThreadKey {
        ThreadKey::new(self.first(), thread)
    }

    /// The first machine the window holds: a fixture world's one machine.
    pub fn first_machine(&self) -> MachineId {
        self.first()
    }

    /// A window driving an Engine of its own that stands in for the
    /// machine saved as `saved_as`: a view test's window on that machine.
    pub fn new_saved_as(store: Store, spawner: Box<dyn Spawner>, saved_as: MachineKey) -> Self {
        let machine = Self::in_process_machine(store, spawner, saved_as)
            .expect("the workspace registry opens");
        let mut cockpit = Self::empty();
        cockpit.hold_machine(machine);
        cockpit
    }

    /// Another machine beside the rest, with an Engine of its own in this
    /// process saved as `saved_as`: a test's second machine.
    pub fn hold_in_process(
        &mut self,
        store: Store,
        spawner: Box<dyn Spawner>,
        saved_as: MachineKey,
    ) -> io::Result<MachineId> {
        let machine = Self::in_process_machine(store, spawner, saved_as)?;
        Ok(self.hold_machine(machine))
    }
}

#[cfg(test)]
impl Cockpit {
    pub(crate) fn set_session_project_root(
        &mut self,
        thread: ThreadKey,
        root: Option<PathBuf>,
    ) -> Result<(), LoadError> {
        let set = self
            .in_process_of(thread)
            .set_session_project_root(thread.thread(), root);
        self.receive_from(thread.machine());
        set
    }

    /// The first machine's Engine in this process, for a test to reach
    /// into.
    pub(crate) fn engine(&self) -> &Engine {
        self.engine_of(self.first())
    }

    /// `machine`'s Engine in this process, for a test to reach into.
    pub(crate) fn engine_of(&self, machine: MachineId) -> &Engine {
        match &self.machines[&machine].link {
            Link::InProcess(engine) => engine,
            Link::Server(_) => panic!("this window's Engine runs in its Server"),
        }
    }

    pub(crate) fn engine_mut(&mut self) -> &mut Engine {
        self.in_process()
    }
}

#[cfg(test)]
mod parity {
    use std::fmt::Write;

    use ferrite_core::activity::ActivityView;

    /// What a window could read of one Thread, as text.
    macro_rules! seen {
        ($view:expr) => {{
            let view = $view;
            format!(
                "generation={} provider={:?} model={:?} effort={:?} title={:?} \
                 suggestion={:?} pending={:?} queued={:?} starting={} busy={} working={} \
                 background={} tuning={} needs_queue={} workspace={:?} root={:?} \
                 first_prompt={} history={} commands={:?} models={:?} mode={:?} modes={:?} \
                 store_error={:?}\n{}",
                view.generation(),
                view.provider(),
                view.model(),
                view.effort(),
                view.title(),
                view.suggestion(),
                view.pending(),
                view.queue(),
                view.starting(),
                view.busy(),
                view.working(),
                view.background_work(),
                view.tuning(),
                view.needs_queue(),
                view.workspace(),
                view.session_project_root(),
                view.first_prompt_sent(),
                view.has_prompt_history(),
                view.commands(),
                view.models(),
                view.permission_mode(),
                view.permission_modes(),
                view.store_error(),
                crate::cockpit::parity::activity(view.activity()),
            )
        }};
    }
    pub(super) use seen;

    pub(super) fn activity(view: ActivityView<'_>) -> String {
        let main = view.main();
        let transcript = main.transcript();
        let mut out = String::new();
        let mut timings: Vec<String> = main
            .tool_timings()
            .iter()
            .map(|(id, timing)| format!("{id}={timing:?}"))
            .collect();
        timings.sort();
        writeln!(
            out,
            "main status={:?} transcript={:?} outcome={:?} model={:?} session={:?} \
             usage={:?} limits={:?} timings={timings:?}",
            main.status(),
            transcript.status(),
            transcript.turn_outcome(),
            transcript.model(),
            transcript.session_id(),
            transcript.usage(),
            transcript.rate_limits(),
        )
        .unwrap();
        for block in transcript.blocks() {
            writeln!(out, "  {:?} {:?}", block.id, block.body).unwrap();
        }
        writeln!(out, "decisions={:?}", view.decisions()).unwrap();
        for child in view.children() {
            writeln!(
                out,
                "child {} {:?} status={:?} coverage={:?} retained={}",
                child.key().as_str(),
                child.info(),
                child.status(),
                child.coverage(),
                child.retained(),
            )
            .unwrap();
            for block in child.transcript().blocks() {
                writeln!(out, "    {:?} {:?}", block.id, block.body).unwrap();
            }
        }
        out
    }
}

/// What the Engine answers, as this window asks it.
impl Cockpit {
    /// `machine`'s Projects.
    pub fn registry(&self, machine: MachineId) -> OnMachine<'_, Registry> {
        self.reading(machine, |shared| &shared.registry)
    }

    /// The Project `key` names, while its machine lists it.
    pub fn project(&self, key: ProjectKey) -> Option<&ferrite_core::workspace::registry::Project> {
        self.shared(key.machine()).registry.project(key.project())
    }

    /// Register `root` as a Project of `machine`: the answer is the
    /// Project, or why not. The window never waits for it.
    pub fn register_project(&mut self, machine: MachineId, root: &Path) -> Ask<ProjectKey> {
        self.register_project_directories(machine, &[root.to_path_buf()])
    }

    /// Register `roots` as one Project of `machine`, the first its
    /// primary directory.
    pub fn register_project_directories(&mut self, machine: MachineId, roots: &[PathBuf]) -> Ask<ProjectKey> {
        self.ask_at(
            machine,
            Op::Project {
                action: ProjectAction::Register {
                    roots: roots.to_vec(),
                },
            },
            move |reply| Ok(ProjectKey::new(machine, registered(reply)?)),
        )
    }

    /// One act on a Project, where its machine's Engine runs: an `Err`
    /// answer says why not. The window never waits for it.
    fn act_on_project(
        &mut self,
        key: ProjectKey,
        action: impl FnOnce(ProjectId) -> ProjectAction,
    ) -> Ask<()> {
        self.ask_at(
            key.machine(),
            Op::Project {
                action: action(key.project()),
            },
            done,
        )
    }

    pub fn add_project_directories(
        &mut self,
        project: ProjectKey,
        roots: &[PathBuf],
    ) -> Ask<()> {
        self.act_on_project(project, |project| ProjectAction::AddDirectories {
            project,
            roots: roots.to_vec(),
        })
    }

    pub fn replace_project_directory(
        &mut self,
        project: ProjectKey,
        index: usize,
        root: &Path,
    ) -> Ask<()> {
        self.act_on_project(project, |project| ProjectAction::ReplaceDirectory {
            project,
            index,
            root: root.to_path_buf(),
        })
    }

    pub fn remove_project_directory(
        &mut self,
        project: ProjectKey,
        index: usize,
    ) -> Ask<()> {
        self.act_on_project(project, |project| ProjectAction::RemoveDirectory {
            project,
            index,
        })
    }

    pub fn remove_project(&mut self, project: ProjectKey) -> Ask<()> {
        self.act_on_project(project, |project| ProjectAction::Remove { project })
    }

    pub fn rename_project(&mut self, project: ProjectKey, title: &str) -> Ask<()> {
        self.act_on_project(project, |project| ProjectAction::Rename {
            project,
            title: title.to_string(),
        })
    }

    /// Watch Session memory in this process. A Server watches its own.
    pub fn watch_memory(&mut self, sampler: Box<dyn RssSampler>, limit: u64) {
        self.in_process().watch_memory(sampler, limit);
        self.receive();
    }

    /// Replace any Session grown past the memory limit. A Server sweeps on
    /// its own, so a window attached to one has nothing to do.
    pub fn sweep(&mut self) -> Vec<Restart> {
        let mut restarts = Vec::new();
        for held in self.machines.values_mut() {
            if let Link::InProcess(engine) = &mut held.link {
                let id = held.id;
                restarts.extend(engine.sweep().into_iter().map(|restart| Restart {
                    thread: ThreadKey::new(id, restart.thread),
                    rss: restart.rss,
                }));
            }
        }
        self.receive();
        restarts
    }

    /// The provider's CLI was replaced on `machine`'s disk: its Sessions
    /// end as they settle, so the next start runs the new release, and its
    /// models are asked for again.
    pub fn cli_replaced(&mut self, machine: MachineId, provider: Provider) {
        // Nothing follows the answer but its log line.
        let _ = self.ask_at(machine, Op::CliReplaced { provider }, move |reply| {
            if let Err(error) = &reply {
                ferrite_core::log!(
                    "the server did not hear of the new {provider} CLI: {}",
                    error.message
                );
            }
            done(reply)
        });
    }

    /// Open a Thread on `machine`, which the window holds.
    pub fn open(
        &mut self,
        machine: MachineId,
        provider: Provider,
        workspace: WorkspaceChoice,
    ) -> io::Result<ThreadKey> {
        let op = Op::Open {
            provider,
            workspace: (&workspace).into(),
        };
        let thread = started(self.act_at(machine, op))?;
        Ok(ThreadKey::new(machine, thread))
    }

    /// A first send on the first machine that blocks until its Thread
    /// opens: the tests' own. A window sends a Draft's first message with
    /// `bootstrap_draft`.
    #[cfg(test)]
    pub fn bootstrap(
        &mut self,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
    ) -> io::Result<ThreadKey> {
        let first = self.first();
        let started = self.in_process().bootstrap(choice, workspace, prompt);
        self.receive_from(first);
        Ok(ThreadKey::new(first, started?))
    }

    pub fn set_provider(&mut self, thread: ThreadKey, choice: ProviderChoice) -> Ask<()> {
        self.ask_on(
            thread,
            |thread| Op::SetProvider {
                thread,
                provider: choice.provider,
                model: choice.model,
            },
            done,
        )
    }

    /// Continue a Thread whose provider lost its saved conversation on a
    /// fresh one, the earlier turns going with the next prompt as context.
    pub fn start_fresh_conversation(&mut self, thread: ThreadKey) -> Ask<()> {
        self.ask_on(thread, |thread| Op::StartFresh { thread }, done)
    }

    /// The Project `thread` is in, on its machine.
    pub fn project_of(&self, thread: ThreadKey) -> Option<ProjectKey> {
        self.try_project_of(thread).ok().flatten()
    }

    pub fn try_project_of(&self, thread: ThreadKey) -> Result<Option<ProjectKey>, LoadError> {
        let project = match self.thread(thread) {
            Some(open) => open.project_id(),
            None => self.peek(thread)?.project_id,
        };
        Ok(project.map(|project| ProjectKey::new(thread.machine(), project)))
    }

    /// Delete a Thread for good: an `Err` answer says why not. The window
    /// never waits for it.
    pub fn delete(&mut self, thread: ThreadKey) -> Ask<()> {
        self.ask_on_then(thread, |thread| Op::Delete { thread }, done, Follow::Deleted(thread))
    }

    /// Open a parked Thread again: an `Err` answer says why not. The
    /// window never waits for it.
    pub fn revive(&mut self, thread: ThreadKey) -> Ask<()> {
        self.ask_on(thread, |thread| Op::Revive { thread }, done)
    }

    /// `revive`, waited for: a launch opens what it imported before it has
    /// a window, and an import still waits for each of its acts.
    pub fn revive_and_wait(&mut self, thread: ThreadKey) -> io::Result<()> {
        done(self.act_on(thread, |thread| Op::Revive { thread }))
    }

    /// Adopt a CLI session file of this computer's as a Thread of
    /// `machine`, whose Server reads it: this computer's.
    pub fn import(&mut self, machine: MachineId, path: &Path) -> io::Result<ThreadKey> {
        let thread = started(self.act_at(
            machine,
            Op::Import {
                path: path.to_path_buf(),
            },
        ))?;
        Ok(ThreadKey::new(machine, thread))
    }

    /// Send `text` as `thread`'s next prompt. What became of it is the
    /// `Ask`'s to say (`Ask::admission`); the window never waits for it.
    pub fn send(&mut self, thread: ThreadKey, text: String) -> Ask<Admission> {
        self.ask_on(thread, |thread| Op::Send { thread, text }, |sent| Ok(admission(sent)))
    }

    /// Go back to before the prompt `prompt` names (`Engine::rewind`):
    /// an `Err` answer says why not, in words.
    pub fn rewind(&mut self, thread: ThreadKey, prompt: &str, files: bool, edit: bool) -> Ask<()> {
        self.ask_on(
            thread,
            |thread| Op::Rewind {
                thread,
                prompt: prompt.to_string(),
                files,
                edit,
            },
            done,
        )
    }

    /// A new Thread from `thread`'s conversation before the prompt `prompt`
    /// names, or all of it, `draft` in its Composer (`Engine::fork`).
    pub fn fork(&mut self, thread: ThreadKey, prompt: Option<&str>, draft: Option<String>) -> Ask<()> {
        self.ask_on(
            thread,
            |thread| Op::Fork {
                thread,
                prompt: prompt.map(str::to_string),
                draft,
            },
            done,
        )
    }

    pub fn interrupt(&mut self, thread: ThreadKey) {
        self.tell_on(thread, |thread| Op::Interrupt { thread });
    }

    /// Stop one Subagent of `thread`; Main's turn and the others run on.
    pub fn stop_subagent(&mut self, thread: ThreadKey, key: &AgentKey) {
        self.tell_on(thread, |thread| Op::StopSubagent {
            thread,
            key: key.as_str().into(),
        });
    }

    /// Answer the pending Decision `decision` names, if it still waits.
    pub fn respond(&mut self, thread: ThreadKey, decision: &Decision, answer: DecisionAnswer) {
        let handle = self.thread(thread).and_then(|open| {
            open.pending_decisions()
                .iter()
                .find(|pending| pending.decision == *decision)
                .map(|pending| pending.handle.clone())
        });
        if let Some(handle) = handle {
            let _ = self.respond_decision(thread, &handle, answer);
        }
    }

    /// Answer exactly the request the operator saw. Answered false when
    /// it no longer waits: answered already, here or in another window. An
    /// answer is never sent twice.
    pub fn respond_decision(
        &mut self,
        thread: ThreadKey,
        handle: &DecisionHandle,
        answer: DecisionAnswer,
    ) -> Ask<bool> {
        self.ask_on(
            thread,
            |thread| Op::Answer {
                thread,
                handle: handle.into(),
                answer: (&answer).into(),
            },
            |answered| match answered {
                Ok(_) => Ok(true),
                Err(error) if error.code == ErrorCode::AlreadyAnswered => Ok(false),
                Err(error) => Err(error.into()),
            },
        )
    }

    /// Something only `--demo` and the tests put in a Pane directly.
    pub fn apply_input(&mut self, thread: ThreadKey, input: Input) {
        self.in_process_of(thread)
            .apply_input(thread.thread(), input);
        self.receive_from(thread.machine());
    }

    /// Ferrite's own words in a Thread's Pane.
    pub fn say(&mut self, thread: ThreadKey, text: String) {
        self.tell_on(thread, |thread| Op::Say { thread, text });
    }

    pub fn search_files(
        &mut self,
        key: ThreadKey,
        query: &str,
    ) -> io::Result<Receiver<io::Result<Vec<ferrite_core::providers::FileSuggestion>>>> {
        let (machine, thread) = self.at_mut(key).ok_or_else(|| io::Error::from(gone()))?;
        let asked = match &mut machine.link {
            Link::InProcess(engine) => engine.search_files(thread, query),
            Link::Server(server) => server.search_files(thread, query),
        };
        self.receive_from(key.machine());
        asked
    }

    pub fn upload_attachment(
        &mut self,
        machine: MachineId,
        path: PathBuf,
    ) -> io::Result<ferrite_core::attachment_transfer::Upload> {
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.upload_attachment(path),
            Link::InProcess(engine) => {
                let store = engine.browser_store().dir().to_path_buf();
                Ok(ferrite_core::attachment_transfer::upload(path, move || {
                    let mut session = ferrite_core::attachment_transfer::Session::new(store);
                    Ok(Box::new(move |request| session.execute(request)))
                }))
            }
        }
    }

    pub fn read_attachment(&mut self, machine: MachineId, path: &Path) -> io::Result<Receiver<io::Result<Vec<u8>>>> {
        let path = path.to_path_buf();
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.read_attachment(path),
            Link::InProcess(engine) => {
                let store = engine.browser_store().dir().to_path_buf();
                Ok(read_here(move || {
                    let mut session = ferrite_core::attachment_transfer::Session::new(store);
                    ferrite_core::attachment_transfer::read(path, |request| session.execute(request)).map_err(|error| error.to_string())
                }))
            }
        }
    }

    pub fn changes(
        &mut self,
        machine: MachineId,
        root: &Path,
        scope: ferrite_core::changes::Scope,
    ) -> io::Result<Receiver<io::Result<ferrite_core::changes::Snapshot>>> {
        let request = ferrite_core::changes::Request::Read {
            root: root.to_path_buf(),
            scope,
        };
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.changes(request),
            Link::InProcess(_) => Ok(read_here(move || {
                ferrite_core::changes::execute(request)?.snapshot().ok_or_else(|| "Missing Changes snapshot".into())
            })),
        }
    }

    pub fn set_file_staged(
        &mut self,
        machine: MachineId,
        root: &Path,
        path: String,
        staged: bool,
    ) -> io::Result<Receiver<io::Result<()>>> {
        let request = ferrite_core::changes::Request::SetStaged {
            root: root.to_path_buf(),
            path,
            staged,
        };
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.set_file_staged(request),
            Link::InProcess(_) => Ok(read_here(move || ferrite_core::changes::execute(request).map(|_| ()))),
        }
    }

    /// Read or save Files content on the computer that owns the workspace.
    pub fn file_operation(
        &mut self,
        machine: MachineId,
        request: ferrite_core::file_operations::Request,
    ) -> io::Result<Receiver<io::Result<ferrite_core::file_operations::Reply>>> {
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.file_operation(request),
            Link::InProcess(_) => Ok(read_here(move || {
                ferrite_core::file_operations::execute(request).map_err(|error| error.to_string())
            })),
        }
    }

    /// Opens, when run, a connection of its own to `machine`'s Server that
    /// carries a tree's ops there (ADR 0048), so a long copy holds up
    /// neither the window nor its acts. Only a Server answers, as
    /// `list_dir`.
    pub fn tree(&mut self, machine: MachineId) -> io::Result<ferrite_core::tree::Connect> {
        self.held_mut(machine)?.remote()?.tree()
    }

    /// A directory's entries on the computer the Server runs on — on a
    /// machine, the machine's (ADR 0026). Answered later: a machine is a
    /// round trip away. Only a Server answers: a window that runs its
    /// agents itself has this computer's folders at hand.
    pub fn list_dir(
        &mut self,
        machine: MachineId,
        path: &Path,
    ) -> io::Result<Receiver<io::Result<ferrite_core::workspace::Listing>>> {
        self.held_mut(machine)?.remote()?.list_dir(path)
    }

    /// What `project`'s repo holds for the workspace chip, read by git
    /// where its machine's Server runs (ADR 0026). Answered later, as
    /// `list_dir`.
    pub fn list_repo(
        &mut self,
        project: ProjectKey,
    ) -> io::Result<Receiver<io::Result<ferrite_core::workspace::RepoListing>>> {
        self.machines
            .get_mut(&project.machine())
            .ok_or_else(|| io::Error::from(gone()))?
            .remote()?
            .list_repo(project.project())
    }

    /// A review's `read` of the checkout at `path`: its changed files with
    /// both sides, read by the vendored review core where the Server runs —
    /// on a machine, the machine's checkout (ADR 0021) — or here, off this
    /// thread, when this window runs its agents itself.
    pub fn review_diff(
        &mut self,
        machine: MachineId,
        path: &Path,
        read: ferrite_reviewr::ferrite::Read,
    ) -> io::Result<Receiver<io::Result<Vec<ferrite_reviewr::ferrite::FileSides>>>> {
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.review_diff(path, &read),
            Link::InProcess(_) => {
                let path = path.to_path_buf();
                Ok(read_here(move || {
                    ferrite_reviewr::ferrite::review(&path, &read)
                }))
            }
        }
    }

    /// The branch's commits for a review's commits scope, read where
    /// `review_diff` reads.
    pub fn review_commits(
        &mut self,
        machine: MachineId,
        path: &Path,
    ) -> io::Result<Receiver<io::Result<Vec<ferrite_reviewr::git::CommitRow>>>> {
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.review_commits(path),
            Link::InProcess(_) => {
                let path = path.to_path_buf();
                Ok(read_here(move || ferrite_reviewr::ferrite::commits(&path)))
            }
        }
    }

    /// Files and lines matching `query` in the checkout at `path`, from
    /// the search index where `review_diff` reads.
    pub fn search_repo(
        &mut self,
        machine: MachineId,
        path: &Path,
        query: &str,
    ) -> io::Result<Receiver<io::Result<ferrite_reviewr::search::SearchResults>>> {
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.search_repo(path, query),
            Link::InProcess(_) => {
                let (path, query) = (path.to_path_buf(), query.to_string());
                Ok(read_here(move || {
                    ferrite_reviewr::ferrite::search(&path, &query)
                }))
            }
        }
    }

    /// The worktree file `file` of the checkout at `path`, for a review's
    /// file view, read where `review_diff` reads.
    pub fn read_review_file(
        &mut self,
        machine: MachineId,
        path: &Path,
        file: &str,
    ) -> io::Result<Receiver<io::Result<String>>> {
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.read_review_file(path, file),
            Link::InProcess(_) => {
                let (path, file) = (path.to_path_buf(), file.to_string());
                Ok(read_here(move || {
                    ferrite_reviewr::ferrite::read_file(&path, &file)
                        .map_err(|notice| notice.message().to_string())
                }))
            }
        }
    }

    pub fn review_pull_request_diff(
        &mut self,
        machine: MachineId,
        path: &Path,
        number: u64,
    ) -> io::Result<Receiver<io::Result<Vec<ferrite_reviewr::ferrite::FileSides>>>> {
        if let Some(files) = self.forge_diffs.get(&(path.to_path_buf(), number)).cloned() {
            return Ok(read_here(move || Ok(files)));
        }
        if self.has_forge_fixture(path) {
            return Ok(read_here(|| Err("No published diff fixture for this pull request".to_string())));
        }
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.review_pull_request_diff(path, number),
            Link::InProcess(_) => {
                let (path, forge) = (path.to_path_buf(), self.forge.clone());
                Ok(read_here(move || {
                    ferrite_reviewr::forge::diff_numbered(&path, &forge, number)
                }))
            }
        }
    }

    pub fn fixture_forge_diff(&mut self, path: PathBuf, number: u64, files: Vec<ferrite_reviewr::ferrite::FileSides>) {
        self.forge_diffs.insert((path, number), files);
    }

    /// The pull request of the branch checked out at `path`, whole, read by
    /// its forge's CLI for a review, where `review_diff` reads.
    pub fn review_pull_request(
        &mut self,
        machine: MachineId,
        path: &Path,
        number: Option<u64>,
    ) -> io::Result<Receiver<io::Result<ferrite_core::protocol::ReviewPullRequest>>> {
        if self.forge_held.contains(path) {
            let (answer, answered) = ferrite_core::doorbell::channel();
            self.held_reads.push(HeldRead {
                path: path.to_path_buf(),
                answer,
            });
            return Ok(answered);
        }
        if let Some(read) =
            number.and_then(|number| self.forge_numbered.get(&(path.to_path_buf(), number)))
        {
            let read = read.clone();
            return Ok(read_here(move || Ok(read)));
        }
        if let Some(view) = self.forge_fixture.get(path).cloned() {
            let view = if number.is_none_or(|number| matches!(&view, ferrite_reviewr::forge::PrView::Pr(pr) if pr.number == number)) {
                view
            } else {
                ferrite_reviewr::forge::PrView::NoPr
            };
            return Ok(read_here(move || {
                Ok(ferrite_core::protocol::ReviewPullRequest::of(view, "PR"))
            }));
        }
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.review_pull_request(path, number),
            Link::InProcess(_) => {
                let (path, forge) = (path.to_path_buf(), self.forge.clone());
                Ok(read_here(move || {
                    let (view, noun) = match number {
                        Some(number) => {
                            ferrite_reviewr::ferrite::pull_request_numbered(&path, &forge, number)
                        }
                        None => ferrite_reviewr::ferrite::pull_request(&path, &forge),
                    };
                    Ok(ferrite_core::protocol::ReviewPullRequest::of(view, noun))
                }))
            }
        }
    }

    /// The pull requests of the repository the checkout at `path` belongs
    /// to, as `filter` admits them, read where `review_pull_request` reads.
    pub fn list_pull_requests(
        &mut self,
        machine: MachineId,
        path: &Path,
        filter: ferrite_core::protocol::WirePrFilter,
    ) -> io::Result<Receiver<io::Result<ferrite_core::protocol::ReviewPullRequests>>> {
        if let Some(list) = self.forge_lists.get(&(path.to_path_buf(), filter)).cloned() {
            return Ok(read_here(move || Ok(list)));
        }
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.list_pull_requests(path, filter),
            Link::InProcess(_) => {
                let (path, forge) = (path.to_path_buf(), self.forge.clone());
                Ok(read_here(move || {
                    let list = ferrite_reviewr::forge::list(&path, &forge, filter.into());
                    Ok(ferrite_core::protocol::ReviewPullRequests::of(list, "PR"))
                }))
            }
        }
    }

    /// Every file of the checkout at `path`, for a review's all-files
    /// list, read where `review_diff` reads.
    pub fn review_files(
        &mut self,
        machine: MachineId,
        path: &Path,
    ) -> io::Result<Receiver<io::Result<Vec<ferrite_reviewr::git::WorktreeEntry>>>> {
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.review_files(path),
            Link::InProcess(_) => {
                let path = path.to_path_buf();
                Ok(read_here(move || {
                    ferrite_reviewr::ferrite::all_files(&path)
                }))
            }
        }
    }

    /// The bases a review's branch scope can read against, read where
    /// `review_diff` reads.
    pub fn review_bases(
        &mut self,
        machine: MachineId,
        path: &Path,
    ) -> io::Result<Receiver<io::Result<ferrite_reviewr::ferrite::Bases>>> {
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.review_bases(path),
            Link::InProcess(_) => {
                let path = path.to_path_buf();
                Ok(read_here(move || ferrite_reviewr::ferrite::bases(&path)))
            }
        }
    }

    /// Make `base` the branch scope's base for the checkout at `path`,
    /// written where `review_diff` reads.
    pub fn pick_review_base(
        &mut self,
        machine: MachineId,
        path: &Path,
        base: &str,
    ) -> io::Result<Receiver<io::Result<()>>> {
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.pick_review_base(path, base),
            Link::InProcess(_) => {
                let (path, base) = (path.to_path_buf(), base.to_string());
                Ok(read_here(move || {
                    ferrite_reviewr::ferrite::pick_base(&path, &base)
                }))
            }
        }
    }

    /// The PR of the branch checked out at `path`, read by `gh` where
    /// `review_diff` reads git: on the Server that owns the checkout, or
    /// here when this window runs its agents itself. `None` whenever `gh`
    /// has nothing to say.
    pub fn pull_request(
        &mut self,
        machine: MachineId,
        path: &Path,
    ) -> io::Result<Receiver<io::Result<Option<ferrite_core::workspace::PullRequest>>>> {
        if let Some(view) = self.forge_fixture.get(path) {
            let reading = ferrite_core::workspace::PullRequest::of_view(view);
            return Ok(read_here(move || Ok(reading)));
        }
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.pull_request(path),
            Link::InProcess(_) => {
                let path = path.to_path_buf();
                Ok(read_here(move || Ok(ferrite_core::workspace::pull_request(&path))))
            }
        }
    }

    /// Whether a PR watch can start on `machine`: only a Server runs the
    /// watcher (T3's `canStartWatch`). Any window can stop one.
    pub fn starts_pr_watches(&self, machine: MachineId) -> bool {
        self.machines
            .get(&machine)
            .is_some_and(|held| matches!(held.link, Link::Server(_)))
    }

    /// Watch the PR at `url` for `key`'s agent, or stop watching it
    /// (ADR 0046), as T3's details card and PR menu do: whether it is
    /// watched once its machine's Server has it. A new watch is the
    /// Server's watcher's, which reads the PR first; an Engine in this
    /// process only stops one.
    pub fn watch_pr(
        &mut self,
        key: ThreadKey,
        url: &str,
        watching: bool,
    ) -> io::Result<Receiver<io::Result<bool>>> {
        let (machine, thread) = self.at_mut(key).ok_or_else(|| io::Error::from(gone()))?;
        let op = Op::WatchPr {
            thread,
            url: url.to_string(),
            watching,
        };
        match &mut machine.link {
            Link::Server(server) => server.watch_pr(op),
            Link::InProcess(_) => {
                let done = self.act_at(key.machine(), op);
                Ok(read_here(move || match done {
                    Ok(Some(Done::PrWatch { watching })) => Ok(watching),
                    Ok(_) => Err("the server answered something else".to_string()),
                    Err(error) => Err(error.message),
                }))
            }
        }
    }

    /// Cancel `key`'s Delegate `delegate` (ADR 0047), as the press on its
    /// Lineage row asks: the agent's `delegate.cancel`, on the Server that
    /// follows it. An Engine in this process follows none, and says so.
    pub fn cancel_delegate(
        &mut self,
        key: ThreadKey,
        delegate: ferrite_core::delegate::DelegateId,
    ) -> io::Result<Receiver<io::Result<()>>> {
        let (machine, thread) = self.at_mut(key).ok_or_else(|| io::Error::from(gone()))?;
        let op = Op::CancelDelegate { thread, delegate };
        match &mut machine.link {
            Link::Server(server) => server.cancel_delegate(op),
            Link::InProcess(_) => {
                let done = self.act_at(key.machine(), op);
                Ok(read_here(move || done.map(drop).map_err(|error| error.message)))
            }
        }
    }

    /// What a GitHub start's picker lists, read by `gh` where the Server
    /// runs (or here, when this window runs its agents itself).
    pub fn github_names(
        &mut self,
        machine: MachineId,
        ask: ferrite_core::protocol::GitHubAsk,
    ) -> io::Result<Receiver<io::Result<Vec<String>>>> {
        use ferrite_core::protocol::GitHubAsk;
        if let Some(names) = self
            .factory_fixture
            .as_ref()
            .and_then(|fixture| fixture.github_names(&ask))
        {
            return Ok(read_here(move || Ok(names)));
        }
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.github_names(ask),
            Link::InProcess(_) => Ok(read_here(move || {
                use ferrite_core::server::github::GitHub;
                let gh = ferrite_core::server::github::Gh;
                match ask {
                    GitHubAsk::Repositories => gh.repositories(),
                    GitHubAsk::Labels { repository } => gh.labels(&repository),
                    GitHubAsk::Unknown => Ok(Vec::new()),
                }
            })),
        }
    }

    /// Ask `thread`'s Session to do `action`: an `Err` answer says why
    /// not. The window never waits for it.
    pub fn control(&mut self, thread: ThreadKey, action: ferrite_core::SessionControl) -> Ask<()> {
        self.ask_on(
            thread,
            |thread| Op::Control {
                thread,
                control: (&action).into(),
            },
            done,
        )
    }

    /// The operator's Threads not open, each machine's in creation order,
    /// as its Server published them: an owned Thread (ADR 0037) is its
    /// owner's to list.
    pub fn parked(&self) -> io::Result<Vec<ThreadKey>> {
        Ok(self
            .machines
            .values()
            .flat_map(|machine| {
                machine
                    .parked
                    .thread_ids()
                    .filter(|thread| {
                        machine.mirrors.view(*thread).is_none()
                            && machine.parked.owner(*thread).is_none()
                    })
                    .map(|thread| machine.key(thread))
            })
            .collect())
    }

    /// Every Thread the Servers have, open or parked, owned ones included:
    /// a Thread in none of them is gone.
    pub fn known_threads(&self) -> std::collections::HashSet<ThreadKey> {
        self.machines
            .values()
            .flat_map(|machine| machine.known().map(|thread| machine.key(thread)))
            .collect()
    }

    /// Whether `thread`'s Server still has it, open or parked: `None`
    /// until it has said what it parked, or for a machine not held. A
    /// deleted Thread is neither.
    pub fn holds(&self, thread: ThreadKey) -> Option<bool> {
        let (machine, thread) = self.at(thread)?;
        if machine.shared.open.contains(&thread) {
            return Some(true);
        }
        machine.parked.has(thread)
    }

    /// Whose `thread` is when not the operator's (ADR 0037), open or
    /// parked: a Worker's Task, a Coordinator's Factory.
    pub fn owner(&self, thread: ThreadKey) -> Option<ferrite_core::ThreadOwner> {
        match self.thread(thread) {
            Some(open) => open.owner().cloned(),
            None => {
                let (machine, thread) = self.at(thread)?;
                machine.parked.owner(thread)
            }
        }
    }

    /// Whether `thread` is the operator's own: what the nav, the palette's
    /// Thread search, the Inbox, the Attention list and the alerts list.
    /// An owned Thread is reached through its owner instead.
    pub fn operators(&self, thread: ThreadKey) -> bool {
        self.owner(thread).is_none()
    }

    /// Whether some Thread's binding waits on a fresh worktree listing:
    /// the driver's cue to list now rather than on its next tick.
    pub fn wants_worktree_listing(&self) -> bool {
        self.machines
            .values()
            .any(|machine| machine.mirrors.wants_listing())
    }

    pub fn worktrees_listed(
        &mut self,
        thread: ThreadKey,
        listed: Vec<PathBuf>,
        taken_at: Instant,
    ) -> bool {
        let age = ferrite_core::clock::instant().saturating_duration_since(taken_at);
        changed(self.act_on(thread, |thread| Op::WorktreesListed {
            thread,
            listed,
            age_ms: age.as_millis() as u64,
        }))
        .unwrap_or(false)
    }

    /// A Thread's header: a parked one's from its published row; an open
    /// one's off its log's header line, on this computer only.
    pub fn peek(&self, thread: ThreadKey) -> Result<ferrite_core::store::ThreadMeta, LoadError> {
        let Some((machine, thread)) = self.at(thread) else {
            return Err(LoadError::Io(gone().into()));
        };
        match machine.open_log(thread) {
            Some(logs) => logs.peek(thread),
            None => machine.parked.peek(thread),
        }
    }

    /// The log of an open Thread, to read its whole history on a worker;
    /// `None` when the log is on another computer.
    pub fn history_log(&self, thread: ThreadKey) -> Option<&Store> {
        let (machine, thread) = self.at(thread)?;
        machine.open_log(thread)
    }

    /// The subagents a Thread knows, live or parked: a parked Thread's as
    /// its Server counted them.
    pub fn subagent_count(&self, thread: ThreadKey) -> Option<usize> {
        match self.thread(thread) {
            Some(open) => Some(open.subagents()),
            None => {
                let (machine, thread) = self.at(thread)?;
                machine.parked.subagents(thread)
            }
        }
    }

    /// The turns a parked Thread holds, as its Server counted them — the
    /// wall's parked tile reads `11 turns`.
    pub fn parked_turns(&self, thread: ThreadKey) -> Option<usize> {
        let (machine, thread) = self.at(thread)?;
        machine.parked.turns(thread)
    }

    /// When a Thread was last used, live or parked — the nav's default
    /// order and its "3 days" line — on this computer's clock. A parked
    /// Thread's is in its row; an open one's is a `stat` of its log, so
    /// callers still cache it rather than ask per frame. Another machine's
    /// open Thread has no log here: its digest's newest activity stands in.
    pub fn last_used(&self, key: ThreadKey) -> Option<std::time::SystemTime> {
        let (machine, thread) = self.at(key)?;
        if let Some(logs) = machine.open_log(thread) {
            return logs.last_used(thread);
        }
        let server_ms = match machine.parked.last_used_ms(thread) {
            Some(ms) => ms,
            None => self.thread(key)?.digest().active_ms?,
        };
        Some(machine.wall(server_ms))
    }

    /// When an open Thread last had a prompt sent or a turn end
    /// (`Store::last_activity`): what the bell groups by, not the log's
    /// write time. It reads an idle Thread's log tail, so callers cache it.
    pub fn last_activity(&self, key: ThreadKey) -> Option<std::time::SystemTime> {
        // A running Thread's writes are its activity: no tail to read.
        let working = self.thread(key).is_some_and(|open| open.busy());
        let (machine, thread) = self.at(key)?;
        match machine.open_log(thread) {
            Some(logs) if !working => logs.last_activity(thread),
            _ => self.last_used(key),
        }
    }

    /// The open Threads, each machine's in the order its mirrors hold them.
    pub fn threads(&self) -> Vec<ThreadKey> {
        self.machines
            .values()
            .flat_map(|machine| {
                machine
                    .mirrors
                    .threads()
                    .into_iter()
                    .map(|thread| machine.key(thread))
            })
            .collect()
    }

    /// The Project the operator was last working in: the Project of the
    /// most recently written Thread the stores hold, skipping Threads
    /// whose Project their registry no longer knows — a Project removed
    /// from the menu must not come back as a draft's default. `None` when
    /// no Thread names a registered Project, which is what a fresh
    /// installation looks like.
    ///
    /// A parked Thread answers from its row; an open one costs a `stat`
    /// and a header line. Launch-time only: callers keep the answer rather
    /// than ask it per frame.
    pub fn last_worked_project(&self, machine: MachineId) -> Option<ProjectKey> {
        let mut threads: Vec<(Option<std::time::SystemTime>, ThreadKey)> = self
            .parked()
            .unwrap_or_default()
            .into_iter()
            .chain(self.threads())
            .filter(|thread| thread.machine() == machine)
            .map(|thread| (self.last_used(thread), thread))
            .collect();
        // Newest first, and a Thread whose log cannot be stat'd sorts last
        // rather than counting as the epoch's most recent.
        threads
            .sort_by_key(|(used, thread)| (std::cmp::Reverse(*used), std::cmp::Reverse(*thread)));
        threads.into_iter().find_map(|(_, thread)| {
            let project = self.project_of(thread)?;
            self.machines[&project.machine()]
                .shared
                .registry
                .project(project.project())
                .map(|_| project)
        })
    }

    /// The window is going. Sessions in this process end with it, so no
    /// provider process outlives Ferrite; a Server's keep running.
    pub fn halt_sessions(&mut self) {
        let machines: Vec<MachineId> = self.machines.keys().copied().collect();
        for machine in machines {
            if let Link::InProcess(engine) = &mut self.machines.get_mut(&machine).unwrap().link {
                engine.halt_sessions();
                self.receive_from(machine);
            }
        }
    }

    /// The durable operator title, whether this Thread is live or parked.
    pub fn thread_title(&self, thread: ThreadKey) -> Result<Option<String>, LoadError> {
        if let Some(open) = self.thread(thread) {
            return Ok(open.title().map(str::to_string));
        }
        let Some((machine, thread)) = self.at(thread) else {
            return Err(LoadError::Io(gone().into()));
        };
        machine.parked.peek(thread).map(|meta| meta.title)
    }

    /// What a Thread is called on screen: the title the operator gave it;
    /// else, when `auto` is on, the first line of its first prompt,
    /// shortened; else its number. A parked Thread's is in its row.
    pub fn display_title(&self, key: ThreadKey, auto: bool) -> String {
        let numbered = || format!("thread-{}", key.thread().get());
        if let Some(open) = self.thread(key) {
            if let Some(title) = open.title() {
                return title.to_string();
            }
            if !auto {
                return numbered();
            }
            return open.prompt_title().unwrap_or_else(numbered);
        }
        let Some((machine, thread)) = self.at(key) else {
            return numbered();
        };
        let Ok(meta) = machine.parked.peek(thread) else {
            return numbered();
        };
        if let Some(title) = meta.title {
            return title;
        }
        if !auto {
            return numbered();
        }
        machine.parked.prompt_title(thread).unwrap_or_else(numbered)
    }

    /// Snooze `thread` until `until`, or wake it: an `Err` answer says
    /// why not. The window never waits for it.
    pub fn snooze_thread(
        &mut self,
        thread: ThreadKey,
        until: Option<std::time::SystemTime>,
    ) -> Ask<()> {
        self.ask_on(thread, |thread| Op::Snooze { thread, until }, done)
    }

    /// Name `thread` `title`: an `Err` answer says why not. The window
    /// never waits for it.
    pub fn rename_thread(&mut self, thread: ThreadKey, title: &str) -> Ask<()> {
        self.ask_on(
            thread,
            |thread| Op::Rename {
                thread,
                title: title.to_string(),
            },
            done,
        )
    }

    /// Hold `text` behind `thread`'s running turn. Whether the queue took
    /// it is the `Ask`'s to say; the window never waits for it.
    pub fn queue(&mut self, thread: ThreadKey, text: String) -> Ask<bool> {
        self.ask_on(thread, |thread| Op::Queue { thread, text }, |queued| Ok(queued.is_ok()))
    }

    /// Take `thread`'s held prompt back: the answer is its text, or
    /// `None` when it held none. The window never waits for it.
    pub fn unqueue(&mut self, thread: ThreadKey) -> Ask<Option<String>> {
        self.ask_on(thread, |thread| Op::Unqueue { thread }, text_of)
    }

    /// Remove the queued prompt `id` names (`QueueItem::id`), back to the
    /// Composer when `restore`.
    pub fn remove_queued_item(&mut self, thread: ThreadKey, id: &str, restore: bool) {
        self.take_back_queued(thread, id, restore, false);
    }

    /// Send the queued prompt `id` names next (`Engine::send_queued_now`).
    pub fn send_queued_now(&mut self, thread: ThreadKey, id: &str) {
        self.tell_on(thread, |thread| Op::SendQueuedNow {
            thread,
            item: id.to_string(),
        });
    }

    /// Edit the queued prompt `id` names, ahead of the current draft.
    pub fn edit_queued_item(&mut self, thread: ThreadKey, id: &str) {
        self.take_back_queued(thread, id, true, true);
    }

    fn take_back_queued(&mut self, thread: ThreadKey, id: &str, restore: bool, edit: bool) {
        // A Server too old to send ids listed its queue by text alone
        // (`mirror::queue_of`): its items take back the latest.
        let id = (!id.is_empty()).then_some(id);
        self.tell_on(thread, |thread| Op::CancelQueued {
            thread,
            restore,
            edit,
            item: id.map(str::to_string),
        });
    }

    /// Ask for a prompt the provider handed back, for the Composer, and
    /// whether it goes ahead of the draft. `None` when `thread` holds
    /// none to ask for. The window never waits for the answer.
    pub fn take_retrieved_prompt(&mut self, thread: ThreadKey) -> Option<Ask<Option<(String, bool)>>> {
        self.thread(thread)?.recovered()?;
        Some(self.ask_on(
            thread,
            |thread| Op::TakeRecovered { thread },
            |taken| {
                Ok(match taken {
                    Ok(Some(Done::Recovered { text, prepend })) => Some((text, prepend)),
                    _ => None,
                })
            },
        ))
    }

    /// Step through `thread`'s prompt history from `current_draft`: the
    /// answer is the prompt stepped to, or `None` at an end. The window
    /// never waits for it.
    pub fn recall_prompt(
        &mut self,
        thread: ThreadKey,
        direction: HistoryDirection,
        current_draft: &str,
    ) -> Ask<Option<String>> {
        let direction = match direction {
            HistoryDirection::Older => ferrite_core::protocol::WireDirection::Older,
            HistoryDirection::Newer => ferrite_core::protocol::WireDirection::Newer,
        };
        self.ask_on(
            thread,
            |thread| Op::Recall {
                thread,
                direction,
                draft: current_draft.to_string(),
            },
            text_of,
        )
    }

    pub fn answer(&mut self, thread: ThreadKey, id: &str) -> bool {
        let answered = self.in_process_of(thread).answer(thread.thread(), id);
        self.receive_from(thread.machine());
        answered
    }

    /// Whether completed turns predict a follow-up, in this process; a
    /// window attached to a Server says `settings_changed`.
    pub fn set_suggestions_enabled(&mut self, enabled: bool) {
        self.in_process().set_suggestions_enabled(enabled);
        self.receive();
    }

    pub fn deliver_suggestion(&mut self, thread: ThreadKey, text: String) {
        self.in_process_of(thread)
            .deliver_suggestion(thread.thread(), text)
    }

    /// Settings changed on disk: whether completed turns predict a
    /// follow-up, where an allowance warns, and the self-hosted forges a
    /// review reads, among them.
    pub fn settings_changed(&mut self, settings: &ferrite_core::settings::Settings) {
        self.forge = settings.forge_hosts.review_config();
        let machines: Vec<MachineId> = self.machines.keys().copied().collect();
        for machine in machines {
            match &mut self.machines.get_mut(&machine).unwrap().link {
                Link::InProcess(engine) => engine.apply_settings(settings),
                Link::Server(server) => server.tell(Op::SettingsChanged),
            }
            self.receive_from(machine);
        }
    }

    /// The defaults of `machine`'s Server, as its own `settings.json` says
    /// — written there first when `set`. A window that runs its agents
    /// itself has no Server to ask.
    pub fn server_defaults(
        &mut self,
        machine: MachineId,
        set: Option<ferrite_core::settings::ServerDefaults>,
    ) -> io::Result<Receiver<io::Result<ferrite_core::settings::ServerDefaults>>> {
        self.held_mut(machine)?.remote()?.server_defaults(set)
    }

    /// `machine`'s provider CLIs: asked of its Server, or of this computer
    /// when the window runs its agents itself. The answer is `Done::Clis`
    /// or `Done::CliUpdated`.
    pub fn cli(
        &mut self,
        machine: MachineId,
        action: ferrite_core::protocol::CliAction,
    ) -> io::Result<Receiver<io::Result<Done>>> {
        match &mut self.held_mut(machine)?.link {
            Link::Server(server) => server.cli(action),
            Link::InProcess(_) => Ok(read_here(move || {
                ferrite_core::server::clis::perform(&ferrite_core::server::clis::Host, &action)
                    .map_err(|error| error.message)
            })),
        }
    }

    pub fn live_sessions(&self, machine: MachineId, provider: Provider) -> usize {
        self.shared(machine).live_sessions(provider)
    }

    pub fn announced_models(&self, machine: MachineId, provider: Provider) -> Vec<ModelInfo> {
        self.shared(machine).announced_models(provider)
    }

    pub fn account_limits(
        &self,
        machine: MachineId,
        provider: Provider,
    ) -> ferrite_core::transcript::RateLimits {
        self.shared(machine).account_limits(provider)
    }

    /// Whether a Server said anything since the last ask: a message, a
    /// start's answer, an attach. What a window draws from the mirrors may
    /// have moved even when no Pane's did.
    pub fn take_arrived(&mut self) -> bool {
        self.machines.values_mut().fold(false, |arrived, machine| {
            std::mem::take(&mut machine.arrived) | arrived
        })
    }

    /// Whether the parked Threads' rows moved since the last ask: a
    /// Server published them.
    pub fn take_parked_changed(&mut self) -> bool {
        self.machines.values_mut().fold(false, |changed, machine| {
            machine.parked.take_changed() | changed
        })
    }

    pub fn take_models_changed(&mut self) -> bool {
        self.machines.values_mut().fold(false, |changed, machine| {
            machine.shared.take_models_changed() | machine.mirrors.take_models_changed() | changed
        })
    }

    /// The allowance warnings due since the last take, oldest first.
    pub fn take_allowance_warnings(&mut self) -> Vec<ferrite_core::providers::limits::AllowanceWarning> {
        std::mem::take(&mut self.said.allowance_warnings)
    }

    /// What API callers asked of the windows since the last take.
    pub fn take_asked(&mut self) -> Vec<Asked> {
        std::mem::take(&mut self.said.asked)
    }

    /// Whether API callers asked anything of the windows since the last take.
    pub fn has_asked(&self) -> bool {
        !self.said.asked.is_empty()
    }

    /// Every Factory's Tasks on `machine`, as its Server last said them
    /// (ADR 0037).
    pub fn factory(&self, machine: MachineId) -> OnMachine<'_, ferrite_core::factory::Board> {
        self.reading(machine, |shared| &shared.factory)
    }

    /// Whether the Tasks changed since the last ask.
    pub fn take_factory_changed(&mut self) -> bool {
        self.machines.values_mut().fold(false, |changed, machine| {
            machine.shared.take_factory_changed() | changed
        })
    }

    /// A Factory command, of `machine`'s Factory. The Factory runs in the
    /// Server: a window that runs its agents itself has none.
    pub fn task(
        &mut self,
        machine: MachineId,
        action: ferrite_core::protocol::TaskAction,
    ) -> io::Result<Option<Done>> {
        self.factory_ask(machine, Op::Task { action })
    }

    /// A Workflow authoring command (#221), answered by `machine`'s
    /// Factory.
    pub fn workflow(
        &mut self,
        machine: MachineId,
        action: ferrite_core::protocol::WorkflowAction,
    ) -> io::Result<Option<Done>> {
        self.factory_ask(machine, Op::Workflow { action })
    }

    /// A Coordinator command (#224), answered by `machine`'s Factory.
    pub fn coordinator(
        &mut self,
        machine: MachineId,
        action: ferrite_core::protocol::CoordinatorAction,
    ) -> io::Result<Option<Done>> {
        self.factory_ask(machine, Op::Coordinator { action })
    }

    /// One Factory ask, of `machine`'s Server (or a fixture world's own
    /// Factory).
    fn factory_ask(&mut self, machine: MachineId, op: Op) -> io::Result<Option<Done>> {
        if let Some((done, board)) = self
            .factory_fixture
            .as_mut()
            .and_then(|fixture| fixture.execute(op.clone()))
        {
            if let Some(board) = board {
                self.receive_fixture(ServerMessage::Factory { revision: 0, board });
            }
            return done;
        }
        let done = self
            .held_mut(machine)?
            .remote()?
            .execute(op)
            .map_err(io::Error::from);
        self.receive_from(machine);
        done
    }

    /// The `window.*` actions every machine's Server handed this window
    /// since the last take, each by its machine and request;
    /// [`Self::driven`] answers it.
    pub fn take_drives(&mut self) -> Vec<(MachineId, u64, ferrite_core::drive::Action)> {
        self.machines
            .values_mut()
            .flat_map(|held| {
                let id = held.id;
                std::mem::take(&mut held.drives)
                    .into_iter()
                    .map(move |(request, action)| (id, request, action))
            })
            .collect()
    }

    pub fn driven(
        &mut self,
        machine: MachineId,
        request: u64,
        answer: Result<ferrite_core::drive::Answer, String>,
    ) {
        if let Some(Link::Server(server)) = self.machines.get_mut(&machine).map(|held| &mut held.link) {
            let (ok, error) = match answer {
                Ok(answer) => (Some(answer), None),
                Err(error) => (None, Some(error)),
            };
            server.say(&ferrite_core::protocol::ClientMessage::Driven { request, ok, error });
        }
    }

    /// Request workspace commands without creating a Thread or Session.
    pub fn discover_commands(
        &mut self,
        machine: MachineId,
        provider: Provider,
        cwd: &std::path::Path,
    ) -> Option<ferrite_core::providers::commands::Discovery> {
        match &mut self.machines.get_mut(&machine)?.link {
            Link::InProcess(engine) => engine.discover_commands(provider, cwd),
            Link::Server(server) => server.discovery().discover_commands(provider, cwd),
        }
    }

    /// `machine`'s provider's announced model rows, else the fallback
    /// catalog.
    pub fn model_catalog(&self, machine: MachineId, provider: Provider) -> Vec<ModelInfo> {
        ferrite_core::providers::models::catalog(provider, &self.announced_models(machine, provider))
    }

    pub fn set_model(&mut self, thread: ThreadKey, model: Option<String>) -> Ask<()> {
        self.ask_on(thread, |thread| Op::SetModel { thread, model }, done)
    }

    pub fn set_effort(&mut self, thread: ThreadKey, effort: Option<String>) -> Ask<()> {
        self.ask_on(thread, |thread| Op::SetEffort { thread, effort }, done)
    }

    pub fn next_blocked(&self, from: Option<ThreadKey>) -> Option<ThreadKey> {
        let queue = self.needs_you();
        let next = from
            .and_then(|from| queue.iter().position(|thread| *thread == from))
            .map_or(0, |at| (at + 1) % queue.len().max(1));
        queue.get(next).copied()
    }

    /// **The answer order** (C8): every Thread waiting on the operator, the
    /// first request raised first. It is the nav's Needs-you strip, top to
    /// bottom, so the strip's first row is exactly what ⌘D and the wall's
    /// `y`/`n`/`a` act on. A Thread whose request has not reached the
    /// notification record yet queues behind the rest, in Thread order.
    pub fn needs_you(&self) -> Vec<ThreadKey> {
        let mut queue = self.blocked();
        // An owned Thread's Decision is answered through its owner.
        queue.retain(|thread| self.operators(*thread));
        queue.sort_by_key(|key| {
            (
                self.machines[&key.machine()]
                    .shared
                    .notifications
                    .first_request(key.thread())
                    .map_or(u64::MAX, |notice| notice.seq),
                *key,
            )
        });
        queue
    }

    /// The Thread the answer keys act on when the focused Pane waits on
    /// nothing: the Needs-you strip's first row.
    pub fn answer_target(&self) -> Option<ThreadKey> {
        self.needs_you().first().copied()
    }

    /// Threads waiting on the operator — what the wall badges.
    pub fn blocked(&self) -> Vec<ThreadKey> {
        self.threads()
            .into_iter()
            .filter(|thread| {
                self.thread(*thread)
                    .is_some_and(|open| !open.pending_decisions().is_empty())
            })
            .collect()
    }

    /// `machine`'s Notices.
    pub fn notifications(&self, machine: MachineId) -> OnMachine<'_, Notifications> {
        self.reading(machine, |shared| &shared.notifications)
    }

    /// Every machine's Notices, each read with its machine: what the
    /// Inbox, the toasts and the bell hear.
    pub fn every_notifications(&self) -> impl Iterator<Item = OnMachine<'_, Notifications>> {
        self.machines.values().map(|held| OnMachine {
            machine: held.id,
            reading: &held.shared.notifications,
        })
    }

    pub fn set_notification_grace(&mut self, grace: Duration) {
        self.in_process().set_notification_grace(grace);
        self.receive();
    }

    pub fn dismiss_notice(&mut self, machine: MachineId, id: NoticeId) -> bool {
        let known = self.shared(machine).notifications.get(id).is_some();
        self.tell_at(
            machine,
            Op::Notice {
                action: NoticeAction::Dismiss { notice: id.get() },
            },
        );
        known
    }

    pub fn dismiss_decision_notice(&mut self, machine: MachineId, id: &DecisionNoticeId) -> bool {
        let shown = self.shared(machine).notifications.decision(id).is_some();
        self.tell_at(
            machine,
            Op::Notice {
                action: NoticeAction::DismissDecision {
                    thread: id.thread,
                    handle: (&id.handle).into(),
                },
            },
        );
        shown
    }

    /// Every machine's Notices cleared.
    pub fn clear_notices(&mut self) {
        self.tell_every(NoticeAction::Clear);
    }

    /// Every machine's Notices and requests read, as the bell's `mark all
    /// read` asks. A machine with nothing unread is not asked.
    pub fn mark_notices_read(&mut self) {
        self.tell_every(NoticeAction::Read);
    }

    fn tell_every(&mut self, action: NoticeAction) {
        let machines: Vec<MachineId> = self
            .machines
            .values()
            .filter(|held| action != NoticeAction::Read || held.shared.notifications.unread() > 0)
            .map(|held| held.id)
            .collect();
        for machine in machines {
            self.tell_at(
                machine,
                Op::Notice {
                    action: action.clone(),
                },
            );
        }
    }

    /// Claude's `Mark as unread` (`Notifications::mark_unread`).
    pub fn mark_unread(&mut self, thread: ThreadKey) {
        self.tell_on(thread, |thread| Op::Notice {
            action: NoticeAction::Unseen { thread },
        });
    }

    /// A fixture world's PR watches for `thread` (ADR 0046), as the
    /// Server's watcher would have saved them.
    pub fn fixture_pr_watch(&mut self, thread: ThreadKey, fact: ferrite_core::pr_watch::WatchFact) {
        if let Err(error) = self.in_process_of(thread).set_pr_watch(thread.thread(), fact) {
            ferrite_core::log!("thread {}: a fixture watch was not saved: {error}", thread.thread());
        }
        // The digest it sends is left for the window's pump, which reads
        // the watch from it as a window of the Server does.
        self.receive_from(thread.machine());
    }

    /// A fixture world's Delegates for `thread` (ADR 0047), changed as its
    /// Server's follower would have changed them.
    pub fn fixture_delegates(
        &mut self,
        thread: ThreadKey,
        change: impl FnOnce(&mut ferrite_core::delegate::Delegates) -> bool,
    ) {
        if let Err(error) = self.in_process_of(thread).amend_delegates(thread.thread(), change) {
            ferrite_core::log!("thread {}: fixture Delegates were not saved: {error}", thread.thread());
        }
        self.receive_from(thread.machine());
    }

    /// A fixture world's `thread`, delegated from `from` (ADR 0047).
    pub fn fixture_delegated_from(&mut self, thread: ThreadKey, from: ferrite_core::DelegatedFrom) {
        self.in_process_of(thread)
            .fixture_delegated_from(thread.thread(), from);
        self.receive_from(thread.machine());
    }

    pub fn fixture_backdate_notices(&mut self, thread: ThreadKey, at: std::time::SystemTime) {
        self.in_process_of(thread)
            .fixture_backdate_notices(thread.thread(), at);
        self.receive_from(thread.machine());
    }

    /// A fixture world's forge: from now on both pull request reads of the
    /// checkout at `path` — the Review's whole snapshot
    /// (`review_pull_request`) and the nav label's (`pull_request`) — answer
    /// `view`, as its forge's CLI would, without running one.
    pub fn fixture_forge(&mut self, path: PathBuf, view: ferrite_reviewr::forge::PrView) {
        self.forge_fixture.insert(path, view);
    }

    /// A fixture world's pull request `number` of the repository the
    /// checkout at `path` belongs to, as the read by number answers it.
    pub fn fixture_forge_numbered(
        &mut self,
        path: PathBuf,
        number: u64,
        read: ferrite_core::protocol::ReviewPullRequest,
    ) {
        self.forge_numbered.insert((path, number), read);
    }

    /// A fixture world's list of the repository the checkout at `path`
    /// belongs to, as `filter` shows it.
    pub fn fixture_pr_list(
        &mut self,
        path: PathBuf,
        filter: ferrite_core::protocol::WirePrFilter,
        list: ferrite_core::protocol::ReviewPullRequests,
    ) {
        self.forge_lists.insert((path, filter), list);
    }

    /// From now on the whole-PR reads of the checkout at `path` (a
    /// `fixture_forge` one) wait, each, for a test to answer it, in any
    /// order (`take_held_reads`).
    pub fn fixture_hold_pr_reads(&mut self, path: PathBuf) {
        self.forge_held.insert(path);
    }

    /// The whole-PR reads held since the last call, oldest first.
    pub fn take_held_reads(&mut self) -> Vec<HeldRead> {
        std::mem::take(&mut self.held_reads)
    }

    /// A fixture world's GitHub login: the repositories it reaches and
    /// each one's labels, answered in place of `gh` (#225).
    pub fn fixture_github(&mut self, repositories: Vec<(String, Vec<String>)>) {
        self.factory_fixture.get_or_insert_default().github = Some(repositories);
    }

    /// Whether a fixture world's forge answers for the checkout at `path`:
    /// the one PR read tests and captures make, since they never run a CLI.
    pub fn has_forge_fixture(&self, path: &Path) -> bool {
        self.forge_fixture.contains_key(path)
            || self
                .forge_numbered
                .keys()
                .any(|(fixture, _)| fixture == path)
            || self.forge_lists.keys().any(|(fixture, _)| fixture == path)
            || self.forge_diffs.keys().any(|(fixture, _)| fixture == path)
    }

    /// The Server's message about a browser, as a window linked to it
    /// would hear it; an in-process window hears none.
    pub fn fixture_browser_message(&mut self, message: ServerMessage) {
        let (ServerMessage::Browser { thread, .. } | ServerMessage::BrowserRequest { thread, .. }) =
            &message
        else {
            panic!("a browser message names its Thread");
        };
        let key = ThreadKey::new(self.first(), *thread);
        self.said.browser.push((key, message));
    }

    /// A fixture world's browser acts succeed, as a linked Server's would.
    pub fn fixture_browser_accepts(&mut self) {
        self.browser_fixture = true;
    }

    /// A fixture world's Server is one from before `Capability::ForkOf`:
    /// its focus for a fork that opened names no `fork_of`.
    pub fn fixture_server_before_fork_of(&mut self) {
        let first = self.first();
        self.machines.get_mut(&first).expect("held").before_fork_of = true;
    }

    /// Whether `machine`'s Server names the Thread a fork that opened was
    /// made from (`Capability::ForkOf`). An Engine in this process is this
    /// build's, so it does.
    pub fn names_fork_owners(&self, machine: MachineId) -> bool {
        let Some(machine) = self.machines.get(&machine) else {
            return false;
        };
        match &machine.link {
            Link::InProcess(_) => !machine.before_fork_of,
            Link::Server(server) => server.names_fork_owners(),
        }
    }

    /// A fixture world's free disk: from now on the bar's `N GB free`
    /// reads `bytes` (`free_disk_fixture`) instead of the volume.
    pub fn fixture_free_disk(&mut self, bytes: u64) {
        self.free_disk_fixture = Some(bytes);
    }

    /// The free bytes a fixture world stands in for the volume, if any.
    pub fn free_disk_fixture(&self) -> Option<u64> {
        self.free_disk_fixture
    }

    /// A fixture world's owned Thread (ADR 0037): one a Task or a
    /// Factory started, as the Server's API starts one for it.
    pub fn fixture_open_owned(
        &mut self,
        provider: Provider,
        workspace: WorkspaceChoice,
        owner: ferrite_core::ThreadOwner,
    ) -> io::Result<ThreadKey> {
        let launch = ferrite_core::engine::Launch {
            owner: Some(owner),
            ..Default::default()
        };
        let first = self.first();
        let opened = self.in_process().open_with(provider, workspace, launch);
        self.receive_from(first);
        Ok(ThreadKey::new(first, opened?))
    }

    /// A fixture world's Factory board, as a Server would say it.
    pub fn fixture_factory(&mut self, board: ferrite_core::factory::Board) {
        self.receive_fixture(ServerMessage::Factory { revision: 0, board });
    }

    /// A fixture world with a Factory of its own that answers Workflow
    /// asks as a Server's does, starting empty.
    pub fn fixture_live_factory(&mut self) {
        let factory = ferrite_core::factory::Factory::in_memory();
        let board = factory.board();
        self.factory_fixture.get_or_insert_default().factory = Some(factory);
        self.receive_fixture(ServerMessage::Factory { revision: 0, board });
    }

    /// A fixture world keeps its Task asks, each answered as changed, to
    /// be read back with `fixture_task_asks`.
    pub fn fixture_record_tasks(&mut self) {
        self.factory_fixture.get_or_insert_default().asks = Some(Vec::new());
    }

    /// `fixture_record_tasks`, with each Start answered as a Task made.
    pub fn fixture_start_tasks(&mut self) {
        self.fixture_record_tasks();
        self.factory_fixture.get_or_insert_default().starts = true;
    }

    pub fn fixture_task_asks(&self) -> &[ferrite_core::protocol::TaskAction] {
        self.factory_fixture
            .as_ref()
            .and_then(|fixture| fixture.asks.as_deref())
            .unwrap_or_default()
    }

    /// A fixture world's Factory hears `command` as a Server's would
    /// (a GitHub read's answer, say), and the window is told the board.
    pub fn fixture_factory_command(&mut self, command: ferrite_core::factory::Command) {
        if let Some(board) = self
            .factory_fixture
            .as_mut()
            .and_then(|fixture| fixture.hear(command))
        {
            self.receive_fixture(ServerMessage::Factory { revision: 0, board });
        }
    }

    /// A fixture world's Task alert, as a Server would say it.
    pub fn fixture_task_alert(&mut self, alert: ServerMessage) {
        self.receive_fixture(alert);
    }

    pub fn fixture_notice(&mut self, thread: ThreadKey, outcome: ferrite_core::TurnOutcome) {
        self.in_process_of(thread)
            .fixture_notice(thread.thread(), outcome);
        self.receive_from(thread.machine());
    }

    /// Whether `draft`'s first send is under way, on whichever machine it
    /// went to.
    pub fn draft_starting(&self, draft: DraftId) -> bool {
        self.machines.values().any(|held| match &held.link {
            Link::InProcess(engine) => engine.draft_starting(draft),
            Link::Server(server) => server.draft_starting(draft),
        })
    }
}

/// A command that changes something the window will hear about. A refusal
/// is logged: the act's own effect (or its absence) is what the window
/// shows.
fn said(executed: Executed) {
    if let Err(error) = executed {
        ferrite_core::log!("the server refused an act: {}", error.message);
    }
}

/// A command that answers with a new or adopted Thread.
fn started(executed: Result<Option<Done>, WireError>) -> io::Result<ThreadId> {
    match executed.map_err(io::Error::from)? {
        Some(Done::Thread { thread }) => Ok(thread),
        _ => Err(io::Error::other("the server answered with no Thread")),
    }
}

/// A first send's reply: the Thread it became, or `None` while it starts.
fn started_now(executed: Executed) -> io::Result<Option<ThreadId>> {
    match executed.map_err(io::Error::from)? {
        Some(Done::Thread { thread }) => Ok(Some(thread)),
        _ => Ok(None),
    }
}

/// A command that answers with a registered Project.
fn registered(executed: Result<Option<Done>, WireError>) -> io::Result<ProjectId> {
    match executed.map_err(io::Error::from)? {
        Some(Done::Project { project }) => Ok(project),
        _ => Err(io::Error::other("the server answered with no Project")),
    }
}

/// What an API caller asked the windows to do (ADR 0027).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// Land on this Thread (`thread.focus`), or a fork of `fork_of` that
    /// has opened.
    Focus {
        thread: ThreadKey,
        fork_of: Option<ThreadKey>,
    },
    /// Show a toast (`notification.show`), ringing its chime; an
    /// important one stays up until dismissed.
    Notification {
        title: String,
        body: Option<String>,
        sound: Option<ferrite_core::sound::Sound>,
        important: bool,
    },
    /// A Task needs the operator, stopped or completed (ADR 0037): a
    /// toast, a chime and a system notification.
    TaskAlert {
        project: ProjectKey,
        task: String,
        status: ferrite_core::factory::Status,
        title: String,
        detail: Option<String>,
    },
}

/// A session file adopted through `adopt_into` (#11): durable the moment
/// this exists, and open unless `not_opened` says why not.
#[derive(Debug)]
pub struct Adopted {
    pub thread: ThreadKey,
    /// The imported Thread would not open: it is durable and parked, exactly
    /// like a launch-time import that would not open.
    pub not_opened: Option<io::Error>,
    /// The blank Thread the door was opened from stayed open: its deletion
    /// was refused.
    pub blank_kept: Option<io::Error>,
}

/// The operator's acts on what is on screen (#28): every rule about focus
/// and park order runs here, against the roster, with the reviving and
/// parking it takes. The window only mirrors the roster and paints.
impl Cockpit {
    /// What is on screen: read-only. Every change is one of the acts below.
    pub fn roster(&self) -> &Roster {
        &self.roster
    }

    /// The Pane on screen — see `Roster::visible`.
    pub fn visible(&self) -> Vec<PaneIdentity> {
        self.roster.visible()
    }

    /// The one door to focus: every move — keys, clicks, nav rows — lands
    /// here. False for a Pane that is not open.
    pub fn focus(&mut self, identity: PaneIdentity) -> bool {
        let landed = self.roster.focus(identity);
        self.acknowledge_seen();
        landed
    }

    /// A running nav row's click (#21): land on that Thread's Pane.
    pub fn focus_thread(&mut self, thread: ThreadKey) -> bool {
        let identity = PaneIdentity::Thread(thread);
        if self.roster.index_of(identity).is_none() {
            return false;
        }
        let landed = self.roster.focus(identity);
        self.acknowledge_seen();
        landed
    }

    /// Record the Subject the renderer is actually showing. Invalid or stale
    /// children fall back to Main, so a replacement Session cannot suppress a
    /// later Main request with an old selection.
    pub fn set_visible_subject(&mut self, thread: ThreadKey, subject: Subject) {
        if subject == Subject::Main {
            self.visible_subjects.remove(&thread);
        } else {
            self.visible_subjects.insert(thread, subject.clone());
        }
        self.tell_on(thread, |thread| Op::ShowSubject {
            thread,
            subject: (&subject).into(),
        });
        self.acknowledge_seen();
    }

    /// Jump to the next Thread waiting on the operator. The Thread landed
    /// on, if any.
    pub fn next_decision(&mut self) -> Option<ThreadKey> {
        let next = self.next_blocked(self.roster.focused_thread())?;
        self.focus_thread(next).then_some(next)
    }

    /// Take an open Thread out of the roster: it stays open and mirrored,
    /// but has no Pane and no focus. A window's side
    /// chat holds the fork it asked for this way.
    pub fn set_aside(&mut self, thread: ThreadKey) {
        self.roster.remove_thread(thread);
    }

    /// Revive one parked Thread onto the screen, alone (#21): a Pane,
    /// focus, and the park order forgetting it — cmd-o must not revive it a
    /// second time. The shared tail of cmd-o and a parked nav row's click.
    /// An open Thread is only landed on: one a window has no Pane drawn for
    /// yet (a `thread.focus` from Projects) is not started again.
    /// An `Err` answer says why it did not come back. The window never
    /// waits for it: the Thread takes the screen on the beat that brings
    /// the answer.
    pub fn reopen(&mut self, thread: ThreadKey) -> Ask<()> {
        if self.thread(thread).is_some() {
            self.roster.focus(PaneIdentity::Thread(thread));
            return Ask::answered(Ok(()));
        }
        self.ask_on_then(thread, |thread| Op::Revive { thread }, done, Follow::Reopened(thread))
    }

    /// `reopen`, waited for: an import's, which still waits for each of
    /// its acts.
    fn reopen_and_wait(&mut self, thread: ThreadKey) -> io::Result<()> {
        if self.thread(thread).is_none() {
            self.revive_and_wait(thread)?;
        }
        self.follow(Follow::Reopened(thread), true);
        Ok(())
    }

    /// cmd-o (#17): reopen the Thread parked most recently — the one the
    /// operator just closed, which is the one they want back. The order is
    /// remembered only for this launch: once it is drained — Threads parked
    /// before a relaunch are never in it — the newest-created parked Thread
    /// is next (accepted v1 behavior). A Thread whose revive fails keeps its
    /// park but loses its slot in the order: cmd-o moves on rather than
    /// jamming on it. None with nothing parked at all.
    pub fn reopen_last(&mut self) -> Option<(ThreadKey, Ask<()>)> {
        let thread = self
            .roster
            .pop_park_order()
            .or_else(|| self.parked().unwrap_or_default().last().copied())?;
        Some((thread, self.reopen(thread)))
    }

    /// cmd-t (#29): open a Draft. Nothing durable exists until the first
    /// send; the Draft takes focus.
    pub fn open_draft(&mut self) -> DraftId {
        self.roster.open_draft()
    }

    /// The first send (#29): bootstrap the Thread on `machine` — its
    /// Project's, or this computer for none — create, worktree, spawn —
    /// and only then let the prompt go; the Thread takes the draft's own
    /// slot. On any failure nothing is half-born: no Thread, the Pane stays
    /// draft, and the prompt stays with the caller. `None` means startup is
    /// pending; the draft stays in place until `take_bootstrap_results`
    /// reports it.
    pub fn bootstrap_draft(
        &mut self,
        machine: MachineId,
        draft: DraftId,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
        effort: Option<String>,
    ) -> io::Result<Option<Bootstrapped>> {
        if self.draft_starting(draft) {
            return Ok(None);
        }
        if !self.roster.has_draft(draft) {
            return Err(io::Error::other("no such draft"));
        }
        let op = Op::Bootstrap {
            provider: choice.provider,
            model: choice.model,
            effort,
            workspace: (&workspace).into(),
            prompt: prompt.to_string(),
            group: None,
            new_group_with: None,
        };
        // A Server answers a first send that starts at once in its reply,
        // and one still starting as it settles: both as `Done::Thread`.
        let started = match &mut self.held_mut(machine)?.link {
            Link::InProcess(engine) => {
                let started = engine
                    .bootstrap_command(draft, &commanded(op))
                    .map(|thread| thread.map(|thread| Done::Thread { thread }));
                started_now(replied(started))
            }
            Link::Server(server) => server.bootstrap(draft, prompt, op).map(|()| None),
        };
        self.receive_from(machine);
        let started = started?.map(|thread| ThreadKey::new(machine, thread));
        Ok(started.map(|thread| complete_draft(&mut self.roster, draft, thread)))
    }

    /// Cancel a submitted startup before changing a draft's choices,
    /// wherever it went. The draft slot and Composer remain; cancellation
    /// emits no completion.
    pub fn cancel_draft_start(&mut self, draft: DraftId) -> io::Result<bool> {
        let mut cancelled = false;
        let machines: Vec<MachineId> = self.machines.keys().copied().collect();
        for machine in machines {
            let held = self.machines.get_mut(&machine).expect("held");
            cancelled |= match &mut held.link {
                Link::InProcess(engine) => engine.cancel_draft_start(draft),
                Link::Server(server) => server.cancel_bootstrap(draft),
            }?;
            self.receive_from(machine);
        }
        Ok(cancelled)
    }

    /// The first sends settled since the last take, each Draft's already
    /// completed in this window.
    pub fn take_bootstrap_results(&mut self) -> Vec<BootstrapResult> {
        self.receive();
        std::mem::take(&mut self.said.bootstrap_results)
    }

    /// Discard a draft Pane (#29): nothing durable dies with it.
    pub fn discard_draft(&mut self, draft: DraftId) -> io::Result<()> {
        self.cancel_draft_start(draft)?;
        self.roster.remove_draft(draft);
        Ok(())
    }

    /// Close a Pane — cmd-w. A draft is discarded; a Thread is parked.
    pub fn close(&mut self, identity: PaneIdentity) -> Ask<()> {
        match identity {
            PaneIdentity::Draft(draft) => Ask::answered(self.discard_draft(draft)),
            PaneIdentity::Thread(thread) => self.park_thread(thread),
        }
    }

    /// Park a Thread — cmd-w and every "Park thread": the Session ends, the
    /// log stays, and reopening revives it. Parked even on a flush error —
    /// the Session is gone either way, so cmd-o should still bring this
    /// Thread back first.
    pub fn park_thread(&mut self, thread: ThreadKey) -> Ask<()> {
        let parked = self.park(thread);
        self.roster.note_parked(thread);
        parked
    }

    /// Adopt a CLI session file (#11) into `machine` (this computer, whose
    /// files it is), in place of the Pane it was picked
    /// for or, with none, alongside the Panes shown: a draft becomes the
    /// imported Thread; a still-blank Thread yields its slot and is deleted
    /// — clean exactly while it is blank, which the picker's own invariant
    /// guarantees and this re-checks. Import creates the Thread; revive
    /// opens it — the same replay-and-resume any parked Thread gets — and
    /// it takes focus. A refusal is the import module's, in its words, and no
    /// Thread was created.
    pub fn adopt_into(
        &mut self,
        machine: MachineId,
        into: Option<PaneIdentity>,
        path: &Path,
    ) -> io::Result<Adopted> {
        let thread = self.import(machine, path)?;
        let mut adopted = Adopted {
            thread,
            not_opened: None,
            blank_kept: None,
        };
        let Some(into) = into else {
            adopted.not_opened = self.reopen_and_wait(thread).err();
            return Ok(adopted);
        };
        match self.revive_and_wait(thread) {
            Ok(()) => {
                match into {
                    PaneIdentity::Draft(draft) => {
                        self.cancel_draft_start(draft)?;
                        self.roster.draft_became(draft, thread);
                    }
                    PaneIdentity::Thread(blank) => {
                        if self
                            .thread(blank)
                            .is_some_and(|open| open.transcript().offers_import())
                        {
                            // An import still waits for each of its acts.
                            let deleted = done(self.act_on(blank, |thread| Op::Delete { thread }));
                            self.follow(Follow::Deleted(blank), deleted.is_ok());
                            adopted.blank_kept = deleted.err();
                        }
                    }
                }
                self.roster.focus(PaneIdentity::Thread(thread));
            }
            Err(error) => adopted.not_opened = Some(error),
        }
        Ok(adopted)
    }

    /// The nav's parked rows (#21), in stable, append-only order: Threads
    /// parked before this launch keep creation order, and this launch's
    /// parks append below in park order — a fresh park lands at the bottom
    /// of the section instead of re-sorting it.
    pub fn parked_in_order(&self) -> io::Result<Vec<ThreadKey>> {
        let parked = self.parked()?;
        let order = self.roster.park_order();
        Ok(parked
            .iter()
            .filter(|thread| !order.contains(thread))
            .copied()
            .chain(
                order
                    .iter()
                    .filter(|thread| parked.contains(thread))
                    .copied(),
            )
            .collect())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod native_tests;
