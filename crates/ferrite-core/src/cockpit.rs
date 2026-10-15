//! The Cockpit: one window's hold on the Engine (ADR 0025). What a window
//! keeps for itself — its Roster's focus, View and fullscreen, its Drafts,
//! whether it has the operator's focus — lives here; everything that must
//! keep running without a window is the Engine's, and the Cockpit changes
//! it only through the Engine's acts. The Engine runs in the Server, and
//! the acts are commands over its socket (`remote`); in `--demo`, `--load`
//! and every test it runs in this process, and an act is a direct call.
//! Either way the window reads what the Engine's messages built: each open
//! Thread's mirror, and the shared state's copies. The open Threads are
//! shared: the Roster follows them (`sync_roster`).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use crate::activity::{DecisionHandle, Subject};
pub use crate::engine::{
    title_from_prompt, BootstrapResult, Bootstrapped, DeleteError, Engine, HistoryDirection,
    LogReader, PaneUpdate, Preparation, ProviderChoice, ProvisionError, Restart, ReviveGroupError,
    RssSampler, SpawnRequest, Spawner, SubjectUpdate, ToolTiming,
};
use crate::groups::{
    Applied, ApplyError, Dissolved, Drag, DropTarget, GroupChange, GroupId, Groups, Plan,
};
use crate::notifications::{DecisionNoticeId, NoticeId, Notifications};
use crate::protocol::{Done, NoticeAction, Op, ProjectAction, ServerMessage, When, WireError};
use crate::roster::{DraftId, DraftScope, PaneIdentity, Roster, View};
use crate::store::{LoadError, Provider, Store};
use crate::transcript::Input;
use crate::workspace::registry::{ProjectId, Registry};
use crate::workspace::WorkspaceChoice;
use crate::{Decision, DecisionAnswer, ModelInfo, ThreadId};
pub use mirror::ThreadView;
pub use remote::{Attach, Endpoint, ServerState};
pub use status::{ServerStatus, StatusRow};

pub(crate) mod mirror;
mod parked;
mod remote;
mod shared;
mod status;

pub struct Cockpit {
    link: Link,
    /// What a parked Thread says.
    parked: parked::Parked,
    /// What is on screen (#28): the roster of Panes, focus, view, fullscreen
    /// and park order. Read through `roster()`; changed only by the acts
    /// below, which keep it showing exactly the open Threads plus drafts.
    pub(crate) roster: Roster,
    /// The open Threads as the Roster last followed them.
    synced: Vec<ThreadId>,
    /// Each open Thread as the Engine's messages built it: what `thread`
    /// reads and `pump` reports.
    mirrors: mirror::Mirrors,
    /// What every window shares, as the Engine's messages left it.
    shared: shared::Shared,
    bootstrap_results: Vec<BootstrapResult>,
    allowance_warnings: Vec<crate::providers::limits::AllowanceWarning>,
    /// What API callers asked of the windows, oldest first.
    asked: Vec<Asked>,
    /// The Subject each Pane shows, where it is not Main: what this window
    /// reads when the operator looks at it.
    visible_subjects: std::collections::BTreeMap<ThreadId, Subject>,
    /// Whether the window has the operator's focus: what is on screen is
    /// seen only while it does (`acknowledge_seen`), so a finish or a
    /// request on screen stays unread until they come back (ADR 0017). True
    /// until the window says otherwise, so a headless Cockpit reads what it
    /// shows.
    window_active: bool,
    /// The Threads whose Composer in this window holds unsent text, as the
    /// window says (`set_composing`).
    composing: std::collections::BTreeSet<ThreadId>,
    /// What the Engine was last told of this window (`tell_window`); `None`
    /// until told, and again after every attach: a Server forgets a window
    /// that left.
    told: Option<Told>,
}

/// What only a window knows and the API reports (ADR 0030): the Thread it
/// shows the operator, and its Composers holding unsent text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Told {
    focused: Option<ThreadId>,
    composing: std::collections::BTreeSet<ThreadId>,
}

/// The window number an Engine in this process knows its one window by.
const IN_PROCESS_WINDOW: u64 = 0;

/// Where the Engine this window drives runs.
enum Link {
    /// In this process, pumped by the window.
    InProcess(Box<Engine>),
    /// In the Server, over its socket.
    Server(Box<remote::Remote>),
}

impl Cockpit {
    /// A window driving an Engine of its own, in this process.
    pub fn try_new(store: Store, spawner: Box<dyn Spawner>) -> io::Result<Self> {
        let reader = parked::Parked::Store(store.clone());
        let dir = store.dir().to_path_buf();
        let engine = Engine::try_new(store, spawner)?;
        let mut cockpit = Self::with(Link::InProcess(Box::new(engine)), reader, &dir);
        cockpit.receive();
        Ok(cockpit)
    }

    /// Test and embedding convenience. The application uses `try_new` so a
    /// protected or incompatible registry is reported instead of replaced.
    pub fn new(store: Store, spawner: Box<dyn Spawner>) -> Self {
        Self::try_new(store, spawner).expect("the workspace registry opens")
    }

    /// A window attached to its Server: this computer's, starting one when
    /// none answers, or a machine's, connected to in the background. Without
    /// one it starts detached (`server`) and keeps trying; it never runs
    /// the agents itself. A machine's parked Threads are what its Server
    /// publishes: their logs stay there.
    pub fn attach(attach: Attach) -> io::Result<Self> {
        let local = matches!(attach.server, Endpoint::Local { .. });
        let parked = if local {
            parked::Parked::Store(Store::reader(&attach.store)?)
        } else {
            parked::Parked::published()
        };
        let dir = attach.store.clone();
        let mut remote = remote::Remote::new(attach);
        let arrived = if local { remote.attach_now() } else { None };
        let mut cockpit = Self::with(Link::Server(Box::new(remote)), parked, &dir);
        cockpit.arrive(arrived.into_iter().collect());
        Ok(cockpit)
    }

    fn with(link: Link, parked: parked::Parked, dir: &Path) -> Self {
        let local = matches!(parked, parked::Parked::Store(_));
        Self {
            link,
            shared: shared::Shared::new(dir, local),
            parked,
            roster: Roster::default(),
            synced: Vec::new(),
            mirrors: mirror::Mirrors::default(),
            bootstrap_results: Vec::new(),
            allowance_warnings: Vec::new(),
            asked: Vec::new(),
            visible_subjects: std::collections::BTreeMap::new(),
            window_active: true,
            composing: std::collections::BTreeSet::new(),
            told: None,
        }
    }

    /// The Server this window is attached to, or why it is not; `None` for
    /// an Engine in this process.
    pub fn server(&self) -> Option<ServerState> {
        match &self.link {
            Link::InProcess(_) => None,
            Link::Server(server) => Some(server.state()),
        }
    }

    /// The machine this window is on (ADR 0026); `None` for this computer.
    pub fn machine(&self) -> Option<&std::sync::Arc<crate::machine::Bridge>> {
        match &self.link {
            Link::InProcess(_) => None,
            Link::Server(server) => server.bridge(),
        }
    }

    /// Attach again now, rather than at the next retry.
    pub fn retry_server(&mut self) {
        if let Link::Server(server) = &mut self.link {
            server.retry();
        }
    }

    /// Stop the Server — at once, or once every agent is idle.
    pub fn stop_server(&mut self, when: When) -> io::Result<()> {
        self.server_command(Op::Stop { when })
    }

    /// Stop the Server at once and wait, up to `within`, for it to be gone:
    /// an installer is about to replace the executable it runs.
    pub fn stop_server_and_wait(&mut self, within: Duration) -> io::Result<()> {
        let deadline = Instant::now() + within;
        let server = self.remote()?;
        server
            .execute(Op::Stop { when: When::Now })
            .map_err(io::Error::from)?;
        server.wait_gone(deadline)
    }

    /// Restart the Server into its executable as it is on disk now — at
    /// once, or once every agent is idle — and resume every open Thread.
    pub fn restart_server(&mut self, when: When) -> io::Result<()> {
        self.server_command(Op::Restart { when })
    }

    fn server_command(&mut self, op: Op) -> io::Result<()> {
        self.remote()?
            .execute(op)
            .map(drop)
            .map_err(io::Error::from)
    }

    fn remote(&mut self) -> io::Result<&mut remote::Remote> {
        match &mut self.link {
            Link::Server(server) => Ok(server),
            Link::InProcess(_) => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "this window runs its agents itself",
            )),
        }
    }

    /// One act where the Engine runs: a direct call in this process, a
    /// command to the Server. Either way the window is up to date with
    /// what it changed when it returns.
    fn act<T>(
        &mut self,
        local: impl FnOnce(&mut Engine) -> T,
        remote: impl FnOnce(&mut remote::Remote) -> T,
    ) -> T {
        let result = match &mut self.link {
            Link::InProcess(engine) => local(engine),
            Link::Server(server) => remote(server),
        };
        self.settle();
        result
    }

    /// The Engine in this process: what only `--demo`, `--load` and the
    /// tests drive — fixtures, injected inputs, the launch-time revival a
    /// Server does for itself.
    fn in_process(&mut self) -> &mut Engine {
        match &mut self.link {
            Link::InProcess(engine) => engine,
            Link::Server(_) => panic!("only an Engine in this process takes this act"),
        }
    }

    /// The Roster follows the open Threads: a Thread that opened gets a
    /// Pane at the end, one that closed loses its own. A Thread a Draft is
    /// starting takes the Draft's slot when it settles instead.
    fn sync_roster(&mut self) {
        let open = self.shared.open.clone();
        for gone in self.synced.iter().filter(|thread| !open.contains(thread)) {
            self.roster.remove_thread(*gone);
        }
        for thread in open.iter().filter(|thread| !self.synced.contains(thread)) {
            if !self.mirrors.for_draft(*thread) {
                self.roster.insert_thread(*thread);
            }
        }
        self.synced = open;
    }

    /// Bring this window up to the Engine after one of its own acts. What
    /// the act changed is the window's to show, as it always was: only the
    /// pump reports Pane updates.
    fn settle(&mut self) {
        self.receive();
        self.mirrors.take_updates();
    }

    /// Bring this window up to the Engine: its mirrors and copies from the
    /// messages it published, then its Roster. In debug builds each
    /// in-process message crosses as its encoded line, so nothing escapes
    /// the contract.
    fn receive(&mut self) {
        let arrived: Vec<remote::Arrival> = match &mut self.link {
            Link::InProcess(engine) => engine
                .take_messages()
                .into_iter()
                .map(|message| {
                    #[cfg(debug_assertions)]
                    let message = {
                        let line = crate::protocol::encode(&message);
                        crate::protocol::decode(&line[..line.len() - 1])
                            .expect("every message decodes as it encoded")
                    };
                    remote::Arrival::Message(message)
                })
                .chain(
                    engine
                        .take_bootstrap_results()
                        .into_iter()
                        .map(remote::Arrival::Started),
                )
                .collect(),
            Link::Server(server) => server.poll(),
        };
        self.arrive(arrived);
        #[cfg(test)]
        self.assert_mirrors_match();
    }

    fn arrive(&mut self, arrived: Vec<remote::Arrival>) {
        let boot = match &self.link {
            Link::InProcess(engine) => engine.boot(),
            Link::Server(server) => server.boot(),
        };
        let mut settled = Vec::new();
        for arrival in arrived {
            match arrival {
                remote::Arrival::Message(message) => self.message(message, boot),
                remote::Arrival::Attached { snapshot, new_boot } => {
                    self.told = None;
                    if new_boot {
                        self.mirrors = mirror::Mirrors::default();
                    }
                    self.shared.attached(&snapshot);
                    self.parked.attached(&snapshot);
                    self.mirrors.retain(&self.shared.open);
                    for state in snapshot.threads {
                        self.mirrors.receive(
                            ServerMessage::Thread {
                                revision: snapshot.revision,
                                state,
                            },
                            boot,
                        );
                    }
                }
                remote::Arrival::Started(result) => settled.push(result),
            }
        }
        // A settled first send completes its Draft in this window — its
        // slot becomes the Thread and its deferred leave applies — once
        // the window holds what the Engine said before it.
        for mut result in settled {
            if let (Some(draft), Ok(done)) = (result.draft, &result.result) {
                let thread = done.thread;
                result.result = Ok(self.complete_draft(draft, thread));
            }
            self.bootstrap_results.push(result);
        }
        self.sync_roster();
        if let Link::Server(server) = &mut self.link {
            let mirrors = &self.mirrors;
            server.watch(&self.shared.open, |thread| mirrors.seq(thread));
        }
    }

    fn message(&mut self, message: ServerMessage, boot: Instant) {
        if let ServerMessage::AllowanceWarning {
            provider,
            window,
            threshold,
            used_fraction,
        } = &message
        {
            if let Ok(window) = (*window).try_into() {
                self.allowance_warnings
                    .push(crate::providers::limits::AllowanceWarning {
                        provider: *provider,
                        window,
                        threshold: *threshold,
                        used_fraction: *used_fraction,
                    });
            }
        }
        match &message {
            ServerMessage::Focus { thread } => self.asked.push(Asked::Focus(*thread)),
            ServerMessage::Notification {
                title,
                body,
                sound,
                important,
            } => self.asked.push(Asked::Notification {
                title: title.clone(),
                body: body.clone(),
                sound: sound.sound(),
                important: *important,
            }),
            ServerMessage::NavFocus { project } => {
                self.asked.push(Asked::NavFocus(project.clone()))
            }
            _ => {}
        }
        self.shared.receive(&message);
        self.parked.receive(&message);
        self.mirrors.receive(message, boot);
    }

    /// One frame: the Engine's pump, then what changed for each Pane of
    /// this window, as its mirrors saw it.
    pub fn pump(&mut self) -> Vec<PaneUpdate> {
        // Fold highlighter answers from the previous frame first, as the
        // Engine does for its own.
        self.mirrors.drain_highlights();
        if let Link::InProcess(engine) = &mut self.link {
            engine.pump();
        }
        self.receive();
        // What is on screen while the window has focus is seen. Here
        // rather than only at the focus doors, so a Thread that finishes in
        // view, or a focus moved by a close or a Group entry, reads the same.
        self.acknowledge_seen();
        self.tell_window();
        self.mirrors.take_updates()
    }

    /// Whether this window's Composer for `thread` holds text not yet sent:
    /// the window says so as it changes, and the API reports it as the
    /// Thread's `draft` (ADR 0030), so nothing types over the operator.
    pub fn set_composing(&mut self, thread: ThreadId, unsent: bool) {
        if unsent {
            self.composing.insert(thread);
        } else {
            self.composing.remove(&thread);
        }
    }

    /// Tell the Engine what changed of what only this window knows: the
    /// Thread its focused Pane shows while it has the operator's focus, and
    /// its Composers holding unsent text.
    fn tell_window(&mut self) {
        // A Pane that left took its Composer, and the unsent text with it:
        // a Thread revived later starts with an empty one.
        let roster = &self.roster;
        self.composing
            .retain(|thread| roster.pane_of(*thread).is_some());
        let now = Told {
            focused: self
                .window_active
                .then(|| self.roster.focused_thread())
                .flatten(),
            composing: self.composing.clone(),
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

    /// An open Thread's facts, as this window's mirror holds them.
    pub fn thread(&self, thread: ThreadId) -> Option<ThreadView<'_>> {
        self.mirrors.view(thread)
    }

    /// A Group change (#28), and what it does to the view: the Group the
    /// operator was looking at may just have dissolved under them. Its
    /// survivor is the one Thread still on screen, so that is where they
    /// land — never the Thread that left, and never a blank Cockpit.
    pub fn apply_group(&mut self, change: GroupChange) -> Result<Applied, ApplyError> {
        let applied = self.act(
            |engine| engine.apply_group(change.clone()),
            |server| match server.execute(Op::Group {
                change: (&change).into(),
            }) {
                Ok(Some(Done::Grouped { group, dissolved })) => Ok(Applied {
                    group,
                    dissolved: dissolved
                        .into_iter()
                        .map(|dissolved| Dissolved {
                            group: dissolved.group,
                            survivor: dissolved.survivor,
                        })
                        .collect(),
                }),
                Ok(_) => Ok(Applied::default()),
                Err(error) => Err(ApplyError::Io(error.message)),
            },
        )?;
        if let View::Group(active) = self.roster.view() {
            if let Some(dissolved) = applied
                .dissolved
                .iter()
                .find(|dissolved| dissolved.group == active)
            {
                self.roster.set_view(View::Solo);
                self.roster.focus(PaneIdentity::Thread(dissolved.survivor));
            }
        }
        self.roster.heal_focus(&self.shared.groups);
        Ok(applied)
    }

    /// Close a Pane: the Session ends, the log is flushed, and the Thread
    /// keeps nothing in memory until it is opened again. A parked Group
    /// member's tile (`enter_group`) has a Pane and no Session: parking it
    /// again only takes the Pane away.
    pub fn park(&mut self, thread: ThreadId) -> io::Result<()> {
        let parked = self.act(
            |engine| engine.park(thread),
            |server| run(server, Op::Park { thread }),
        );
        if !self.shared.open.contains(&thread) {
            self.roster.remove_thread(thread);
        }
        parked
    }

    pub fn ensure_subject_history(
        &mut self,
        thread: ThreadId,
        subject: &Subject,
    ) -> io::Result<bool> {
        self.load_subject(thread, subject, false)
    }

    pub fn subject_history_error(&self, thread: ThreadId, subject: &Subject) -> Option<&str> {
        self.mirrors.history_error(thread, subject)
    }

    pub fn retry_subject_history(
        &mut self,
        thread: ThreadId,
        subject: &Subject,
    ) -> io::Result<bool> {
        self.load_subject(thread, subject, true)
    }

    fn load_subject(
        &mut self,
        thread: ThreadId,
        subject: &Subject,
        retry: bool,
    ) -> io::Result<bool> {
        self.act(
            |engine| {
                if retry {
                    engine.retry_subject_history(thread, subject)
                } else {
                    engine.ensure_subject_history(thread, subject)
                }
            },
            |server| {
                changed(server.execute(Op::LoadSubject {
                    thread,
                    subject: subject.into(),
                    retry,
                }))
            },
        )
    }

    /// CLI session files to adopt, found off the UI thread. Discovery reads
    /// the operator's own files: a window does it, wherever its Engine runs.
    pub fn discover_sessions(
        &mut self,
        roots: Vec<(Provider, PathBuf)>,
        cap: usize,
    ) -> Option<Receiver<io::Result<Vec<crate::import::Candidate>>>> {
        match &mut self.link {
            Link::InProcess(engine) => engine.discover_sessions(roots, cap),
            Link::Server(server) => server.discovery().discover_sessions(roots, cap),
        }
    }

    /// The operator opened a Notice — from a toast or the bell: it is
    /// read, and they land on its Thread's Pane in the view that shows
    /// it, reviving a parked one. The Thread landed on, or None when it
    /// is gone.
    pub fn open_notice(&mut self, id: NoticeId) -> Option<ThreadId> {
        let known = self
            .shared
            .notifications
            .get(id)
            .map(|notice| notice.thread);
        let thread = self.act(
            |engine| engine.open_notice(id),
            |server| {
                let thread = known?;
                notice(server, NoticeAction::Open { notice: id.get() });
                Some(thread)
            },
        )?;
        self.land_on(thread)
    }

    /// Open one live Decision attention record: its Subject is shown, and
    /// the operator lands on its Thread as `open_notice` does.
    pub fn open_decision_notice(&mut self, id: &DecisionNoticeId) -> Option<ThreadId> {
        let live = self.shared.notifications.decision(id).is_some();
        let thread = self.act(
            |engine| engine.open_decision_notice(id),
            |server| {
                live.then_some(())?;
                notice(
                    server,
                    NoticeAction::OpenDecision {
                        thread: id.thread,
                        handle: (&id.handle).into(),
                    },
                );
                Some(id.thread)
            },
        )?;
        self.land_on(thread)
    }

    fn land_on(&mut self, thread: ThreadId) -> Option<ThreadId> {
        if !self.focus_thread(thread) {
            self.reopen(thread).ok()?;
        }
        self.acknowledge_seen();
        Some(thread)
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
        self.acknowledge_seen()
    }

    /// Everything on screen while the window has focus is seen: each
    /// visible Thread's Notices are read, and so are the live Decisions of
    /// the Subject its Pane shows. Fullscreen shows its one Pane. True when
    /// anything was read.
    fn acknowledge_seen(&mut self) -> bool {
        if !self.window_active {
            return false;
        }
        let shown: Vec<ThreadId> = match self.roster.fullscreen() {
            Some(fullscreen) => vec![fullscreen],
            None => self.visible(),
        }
        .into_iter()
        .filter_map(PaneIdentity::thread)
        .collect();
        match &mut self.link {
            Link::InProcess(engine) => {
                let read = engine.acknowledge_seen(&shown);
                // The Notices read come back as the Engine's messages.
                self.receive();
                read
            }
            Link::Server(_) => {
                // Only what would read anything goes to the Server: this
                // runs every frame.
                let unread: Vec<ThreadId> = shown
                    .into_iter()
                    .filter(|thread| self.would_read(*thread))
                    .collect();
                if unread.is_empty() {
                    return false;
                }
                self.act(
                    |_| (),
                    |server| {
                        for thread in unread {
                            notice(server, NoticeAction::Seen { thread });
                        }
                    },
                );
                true
            }
        }
    }

    /// Whether looking at `thread` reads anything: an unread Notice, or an
    /// unread request of the Subject its Pane shows.
    fn would_read(&self, thread: ThreadId) -> bool {
        let notifications = &self.shared.notifications;
        let shown = self
            .visible_subjects
            .get(&thread)
            .and_then(|subject| {
                let view = self.thread(thread)?.activity();
                let subject = view.canonical_subject(subject);
                view.subject(&subject).is_some().then_some(subject)
            })
            .unwrap_or(Subject::Main);
        notifications
            .notices()
            .any(|notice| notice.thread == thread && !notice.read)
            || notifications.decisions().any(|notice| {
                notice.id.thread == thread
                    && !notice.read
                    && notice.subject.as_ref() == Some(&shown)
            })
    }
}

/// A git read on a thread of its own, answered as a Server's would be.
fn read_here<T: Send + 'static>(
    read: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Receiver<io::Result<T>> {
    let (answer, answered) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = answer.send(read().map_err(io::Error::other));
    });
    answered
}

/// A command whose reply carries nothing the window needs.
fn run(server: &mut remote::Remote, op: Op) -> io::Result<()> {
    server.execute(op).map(drop).map_err(io::Error::from)
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
fn notice(server: &mut remote::Remote, action: NoticeAction) {
    if let Err(error) = server.execute(Op::Notice { action }) {
        crate::log!("a notice act did not reach the server: {}", error.message);
    }
}

/// Mirror parity (ADR 0025): after every message, each open Thread's
/// mirror reads exactly as the Engine's own Thread does. Every Cockpit
/// test checks it as it runs.
#[cfg(test)]
impl Cockpit {
    pub(crate) fn set_session_project_root(
        &mut self,
        thread: ThreadId,
        root: Option<PathBuf>,
    ) -> Result<(), LoadError> {
        let set = self.in_process().set_session_project_root(thread, root);
        self.settle();
        set
    }

    /// The Engine in this process, for a test to reach into.
    pub(crate) fn engine(&self) -> &Engine {
        match &self.link {
            Link::InProcess(engine) => engine,
            Link::Server(_) => panic!("this window's Engine runs in its Server"),
        }
    }

    pub(crate) fn engine_mut(&mut self) -> &mut Engine {
        self.in_process()
    }

    fn assert_mirrors_match(&self) {
        let Link::InProcess(engine) = &self.link else {
            return;
        };
        let engine: &Engine = engine;
        assert_eq!(self.shared.groups.to_wire(), engine.groups().to_wire());
        assert_eq!(self.shared.registry.to_wire(), engine.registry().to_wire());
        assert_eq!(
            self.shared.notifications.to_wire(),
            engine.notifications().to_wire()
        );
        for provider in [Provider::Claude, Provider::Codex] {
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
            let engine = engine.thread(thread).expect("open in the Engine");
            let mirror = self
                .mirrors
                .view(thread)
                .unwrap_or_else(|| panic!("thread {thread} has no mirror"));
            assert_eq!(
                parity::seen!(mirror),
                parity::seen!(engine),
                "thread {thread}: the mirror drifted from the Engine"
            );
        }
    }
}

#[cfg(test)]
mod parity {
    use std::fmt::Write;

    use crate::activity::ActivityView;

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
                view.queued_all(),
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
    pub fn registry(&self) -> &Registry {
        &self.shared.registry
    }

    pub fn groups(&self) -> &Groups {
        &self.shared.groups
    }

    pub fn plan_group_drop(&self, drag: Drag, target: DropTarget) -> Plan {
        self.shared.groups.preview_drop(drag, target)
    }

    pub fn register_project(&mut self, root: &Path) -> io::Result<ProjectId> {
        self.act(
            |engine| engine.register_project(root),
            |server| {
                registered(server.execute(Op::Project {
                    action: ProjectAction::Register {
                        roots: vec![root.to_path_buf()],
                    },
                }))
            },
        )
    }

    pub fn register_project_directories(&mut self, roots: &[PathBuf]) -> io::Result<ProjectId> {
        self.act(
            |engine| engine.register_project_directories(roots),
            |server| {
                registered(server.execute(Op::Project {
                    action: ProjectAction::Register {
                        roots: roots.to_vec(),
                    },
                }))
            },
        )
    }

    pub fn add_project_directories(
        &mut self,
        project: ProjectId,
        roots: &[PathBuf],
    ) -> io::Result<()> {
        self.act(
            |engine| engine.add_project_directories(project, roots),
            |server| {
                run(
                    server,
                    Op::Project {
                        action: ProjectAction::AddDirectories {
                            project,
                            roots: roots.to_vec(),
                        },
                    },
                )
            },
        )
    }

    pub fn replace_project_directory(
        &mut self,
        project: ProjectId,
        index: usize,
        root: &Path,
    ) -> io::Result<()> {
        self.act(
            |engine| engine.replace_project_directory(project, index, root),
            |server| {
                run(
                    server,
                    Op::Project {
                        action: ProjectAction::ReplaceDirectory {
                            project,
                            index,
                            root: root.to_path_buf(),
                        },
                    },
                )
            },
        )
    }

    pub fn remove_project_directory(&mut self, project: ProjectId, index: usize) -> io::Result<()> {
        self.act(
            |engine| engine.remove_project_directory(project, index),
            |server| {
                run(
                    server,
                    Op::Project {
                        action: ProjectAction::RemoveDirectory { project, index },
                    },
                )
            },
        )
    }

    pub fn group_layout(
        &self,
        group: GroupId,
        bounds: crate::layout::Rect,
    ) -> Option<crate::layout::Tree> {
        self.shared.groups.layout(group, bounds)
    }

    pub fn reset_group_layout(&mut self, group: GroupId) -> Result<(), ApplyError> {
        self.act(
            |engine| engine.reset_group_layout(group),
            |server| {
                run(server, Op::Layout { group, tree: None })
                    .map_err(|error| ApplyError::Io(error.to_string()))
            },
        )
    }

    pub fn set_group_layout(
        &mut self,
        group: GroupId,
        tree: crate::layout::Tree,
    ) -> Result<(), ApplyError> {
        self.act(
            |engine| engine.set_group_layout(group, tree.clone()),
            |server| {
                run(
                    server,
                    Op::Layout {
                        group,
                        tree: Some(tree.clone()),
                    },
                )
                .map_err(|error| ApplyError::Io(error.to_string()))
            },
        )
    }

    pub fn remove_project(&mut self, project: ProjectId) -> io::Result<()> {
        self.act(
            |engine| engine.remove_project(project),
            |server| {
                run(
                    server,
                    Op::Project {
                        action: ProjectAction::Remove { project },
                    },
                )
            },
        )
    }

    pub fn rename_project(&mut self, project: ProjectId, title: &str) -> io::Result<()> {
        self.act(
            |engine| engine.rename_project(project, title),
            |server| {
                run(
                    server,
                    Op::Project {
                        action: ProjectAction::Rename {
                            project,
                            title: title.to_string(),
                        },
                    },
                )
            },
        )
    }

    /// Watch Session memory in this process. A Server watches its own.
    pub fn watch_memory(&mut self, sampler: Box<dyn RssSampler>, limit: u64) {
        self.in_process().watch_memory(sampler, limit);
        self.settle();
    }

    /// Replace any Session grown past the memory limit. A Server sweeps on
    /// its own, so a window attached to one has nothing to do.
    pub fn sweep(&mut self) -> Vec<Restart> {
        self.act(Engine::sweep, |_| Vec::new())
    }

    /// The provider's CLI was replaced on disk: its Sessions restart on the
    /// new release as they idle, and its models are asked for again.
    pub fn cli_replaced(&mut self, provider: Provider) {
        self.act(
            |engine| {
                engine.restart_sessions(provider);
                engine.rediscover_models();
            },
            |server| {
                if let Err(error) = run(server, Op::CliReplaced { provider }) {
                    crate::log!("the server did not hear of the new {provider} CLI: {error}");
                }
            },
        );
    }

    pub fn open(&mut self, provider: Provider, workspace: WorkspaceChoice) -> io::Result<ThreadId> {
        self.act(
            |engine| engine.open(provider, workspace.clone()),
            |server| {
                started(server.execute(Op::Open {
                    provider,
                    workspace: (&workspace).into(),
                }))
            },
        )
    }

    /// A first send that blocks until its Thread opens: the tests' own.
    /// A window sends a Draft's first message with `bootstrap_draft`.
    #[cfg(test)]
    pub fn bootstrap(
        &mut self,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
    ) -> io::Result<ThreadId> {
        let started = self.in_process().bootstrap(choice, workspace, prompt);
        self.settle();
        started
    }

    #[cfg(test)]
    pub fn bootstrap_in_group(
        &mut self,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
        group: GroupId,
    ) -> io::Result<ThreadId> {
        let started = self
            .in_process()
            .bootstrap_in_group(choice, workspace, prompt, group);
        self.settle();
        started
    }

    pub fn set_provider(
        &mut self,
        thread: ThreadId,
        choice: ProviderChoice,
    ) -> Result<(), ProvisionError> {
        self.act(
            |engine| engine.set_provider(thread, choice.clone()),
            |server| {
                provisioned(server.execute(Op::SetProvider {
                    thread,
                    provider: choice.provider,
                    model: choice.model.clone(),
                }))
            },
        )
    }

    pub fn project_id(&self, thread: ThreadId) -> Option<ProjectId> {
        self.try_project_id(thread).ok().flatten()
    }

    pub fn try_project_id(&self, thread: ThreadId) -> Result<Option<ProjectId>, LoadError> {
        self.parked.peek(thread).map(|meta| meta.project_id)
    }

    pub fn delete(&mut self, thread: ThreadId) -> Result<(), DeleteError> {
        self.act(
            |engine| engine.delete(thread),
            |server| run(server, Op::Delete { thread }).map_err(DeleteError::Io),
        )
    }

    pub fn revive(&mut self, thread: ThreadId) -> Result<(), LoadError> {
        self.act(
            |engine| engine.revive(thread),
            |server| run(server, Op::Revive { thread }).map_err(LoadError::Io),
        )
    }

    /// Revive a Thread `delay` from now: the launch's stagger (ADR 0018). A
    /// Server revives its open Threads itself.
    pub fn revive_after(&mut self, thread: ThreadId, delay: Duration) -> Result<(), LoadError> {
        let revived = self.in_process().revive_after(thread, delay);
        self.settle();
        revived
    }

    /// The tests' own: no window revives a whole Group.
    #[cfg(test)]
    pub fn revive_group(&mut self, group: GroupId) -> Result<Vec<ThreadId>, ReviveGroupError> {
        let revived = self.in_process().revive_group(group);
        self.settle();
        revived
    }

    pub fn import(&mut self, path: &Path) -> Result<ThreadId, crate::import::ImportError> {
        self.act(
            |engine| engine.import(path),
            |server| {
                started(server.execute(Op::Import {
                    path: path.to_path_buf(),
                }))
                .map_err(crate::import::ImportError::Io)
            },
        )
    }

    pub fn send(&mut self, thread: ThreadId, text: String) {
        self.act(
            |engine| engine.send(thread, text.clone()),
            |server| {
                said(
                    server,
                    Op::Send {
                        thread,
                        text: text.clone(),
                    },
                )
            },
        );
    }

    pub fn interrupt(&mut self, thread: ThreadId) {
        self.act(
            |engine| engine.interrupt(thread),
            |server| said(server, Op::Interrupt { thread }),
        );
    }

    /// Answer the pending Decision `decision` names, if it still waits.
    pub fn respond(&mut self, thread: ThreadId, decision: &Decision, answer: DecisionAnswer) {
        let handle = self.thread(thread).and_then(|open| {
            open.activity()
                .decisions()
                .iter()
                .find(|pending| pending.decision == *decision)
                .map(|pending| pending.handle.clone())
        });
        if let Some(handle) = handle {
            let _ = self.respond_decision(thread, &handle, answer);
        }
    }

    /// Answer exactly the request the operator saw. False when it no longer
    /// waits — answered already, here or in another window. An answer is
    /// never sent twice.
    pub fn respond_decision(
        &mut self,
        thread: ThreadId,
        handle: &DecisionHandle,
        answer: DecisionAnswer,
    ) -> io::Result<bool> {
        self.act(
            |engine| engine.respond_decision(thread, handle, answer.clone()),
            |server| match server.execute(Op::Answer {
                thread,
                handle: handle.into(),
                answer: (&answer).into(),
            }) {
                Ok(_) => Ok(true),
                Err(error) if error.code == crate::protocol::ErrorCode::AlreadyAnswered => {
                    Ok(false)
                }
                Err(error) => Err(error.into()),
            },
        )
    }

    /// Something only `--demo` and the tests put in a Pane directly.
    pub fn apply_input(&mut self, thread: ThreadId, input: Input) {
        self.in_process().apply_input(thread, input);
        self.settle();
    }

    /// Ferrite's own words in a Thread's Pane.
    pub fn say(&mut self, thread: ThreadId, text: String) {
        self.act(
            |engine| engine.apply_input(thread, Input::Notice(text.clone())),
            |server| {
                said(
                    server,
                    Op::Say {
                        thread,
                        text: text.clone(),
                    },
                )
            },
        );
    }

    pub fn search_files(
        &mut self,
        thread: ThreadId,
        query: &str,
    ) -> io::Result<Receiver<io::Result<Vec<crate::providers::FileSuggestion>>>> {
        self.act(
            |engine| engine.search_files(thread, query),
            |server| server.search_files(thread, query),
        )
    }

    /// A directory's entries on the computer the Server runs on — on a
    /// machine, the machine's (ADR 0026). Answered later: a machine is a
    /// round trip away. Only a Server answers: a window that runs its
    /// agents itself has this computer's folders at hand.
    pub fn list_dir(
        &mut self,
        path: &Path,
    ) -> io::Result<Receiver<io::Result<crate::workspace::Listing>>> {
        self.remote()?.list_dir(path)
    }

    /// What `project`'s repo holds for the workspace chip, read by git
    /// where the Server runs (ADR 0026). Answered later, as `list_dir`.
    pub fn list_repo(
        &mut self,
        project: ProjectId,
    ) -> io::Result<Receiver<io::Result<crate::workspace::RepoListing>>> {
        self.remote()?.list_repo(project)
    }

    /// A review's `diff` of the checkout at `path`, read by git where the
    /// Server runs — on a machine, the machine's checkout (ADR 0021) — or
    /// here, off this thread, when this window runs its agents itself.
    pub fn review_diff(
        &mut self,
        path: &Path,
        diff: crate::review::git::Diff,
    ) -> io::Result<Receiver<io::Result<Vec<crate::FileEdit>>>> {
        match &mut self.link {
            Link::Server(server) => server.review_diff(path, &diff),
            Link::InProcess(_) => {
                let path = path.to_path_buf();
                Ok(read_here(move || {
                    crate::review::git::read(&path, &diff)
                        .map(|text| crate::review::git::parse(&text))
                }))
            }
        }
    }

    /// The branch's commits for a review's commits scope, read where
    /// `review_diff` reads.
    pub fn review_commits(
        &mut self,
        path: &Path,
    ) -> io::Result<Receiver<io::Result<Vec<crate::review::git::Commit>>>> {
        match &mut self.link {
            Link::Server(server) => server.review_commits(path),
            Link::InProcess(_) => {
                let path = path.to_path_buf();
                Ok(read_here(move || crate::review::git::commits(&path)))
            }
        }
    }

    /// The PR of the branch checked out at `path`, read by `gh` where
    /// `review_diff` reads git: on the Server that owns the checkout, or
    /// here when this window runs its agents itself. `None` whenever `gh`
    /// has nothing to say.
    pub fn pull_request(
        &mut self,
        path: &Path,
    ) -> io::Result<Receiver<io::Result<Option<crate::workspace::PullRequest>>>> {
        match &mut self.link {
            Link::Server(server) => server.pull_request(path),
            Link::InProcess(_) => {
                let path = path.to_path_buf();
                Ok(read_here(move || Ok(crate::workspace::pull_request(&path))))
            }
        }
    }

    pub fn control(&mut self, thread: ThreadId, action: crate::SessionControl) -> io::Result<()> {
        self.act(
            |engine| engine.control(thread, action.clone()),
            |server| {
                run(
                    server,
                    Op::Control {
                        thread,
                        control: (&action).into(),
                    },
                )
            },
        )
    }

    /// The Threads not open, in creation order.
    pub fn parked(&self) -> io::Result<Vec<ThreadId>> {
        Ok(self
            .parked
            .thread_ids()?
            .into_iter()
            .filter(|thread| self.thread(*thread).is_none())
            .collect())
    }

    /// Whether some Thread's binding waits on a fresh worktree listing:
    /// the driver's cue to list now rather than on its next tick.
    pub fn wants_worktree_listing(&self) -> bool {
        self.mirrors.wants_listing()
    }

    pub fn worktrees_listed(
        &mut self,
        thread: ThreadId,
        listed: Vec<PathBuf>,
        taken_at: Instant,
    ) -> bool {
        self.act(
            |engine| engine.worktrees_listed(thread, listed.clone(), taken_at),
            |server| {
                changed(server.execute(Op::WorktreesListed {
                    thread,
                    listed: listed.clone(),
                    age_ms: taken_at.elapsed().as_millis() as u64,
                }))
                .unwrap_or(false)
            },
        )
    }

    pub fn peek(&self, thread: ThreadId) -> Result<crate::store::ThreadMeta, LoadError> {
        self.parked.peek(thread)
    }

    /// The subagents a Thread knows, live or parked. A parked Thread's are
    /// read off its log: callers cache this rather than ask per frame.
    pub fn subagent_count(&self, thread: ThreadId) -> Result<usize, LoadError> {
        if let Some(open) = self.thread(thread) {
            return Ok(open.activity().children().len());
        }
        self.log_reader().subagent_count(thread)
    }

    /// A read-only handle on the Thread logs, for the parked-Thread reads
    /// too slow for the UI thread: it can be moved to a background task.
    pub fn log_reader(&self) -> LogReader {
        self.parked.log_reader()
    }

    /// When a Thread was last used, live or parked — the nav's default
    /// order and its "3 days" line. One `stat`, so callers still cache it
    /// rather than ask per frame.
    pub fn last_used(&self, thread: ThreadId) -> Option<std::time::SystemTime> {
        self.parked.last_used(thread)
    }

    /// The open Threads.
    pub fn threads(&self) -> Vec<ThreadId> {
        self.mirrors.threads()
    }

    /// The Project the operator was last working in: the Project of the
    /// most recently written Thread the store holds, skipping Threads
    /// whose Project the registry no longer knows — a Project removed from
    /// the menu must not come back as a draft's default. `None` when no
    /// Thread names a registered Project, which is what a fresh
    /// installation looks like.
    ///
    /// One `stat` and one header line per Thread, and it stops at the
    /// first answer, so the common case is two reads. Launch-time only:
    /// callers keep the answer rather than ask it per frame.
    pub fn last_worked_project(&self) -> Option<ProjectId> {
        let mut threads: Vec<(Option<std::time::SystemTime>, ThreadId)> = self
            .parked
            .thread_ids()
            .unwrap_or_default()
            .into_iter()
            .map(|thread| (self.parked.last_used(thread), thread))
            .collect();
        // Newest first, and a Thread whose log cannot be stat'd sorts last
        // rather than counting as the epoch's most recent.
        threads
            .sort_by_key(|(used, thread)| (std::cmp::Reverse(*used), std::cmp::Reverse(*thread)));
        threads.into_iter().find_map(|(_, thread)| {
            let project = self.project_id(thread)?;
            self.shared.registry.project(project).map(|_| project)
        })
    }

    /// The window is going. Sessions in this process end with it, so no
    /// provider process outlives Ferrite; a Server's keep running.
    pub fn halt_sessions(&mut self) {
        if let Link::InProcess(engine) = &mut self.link {
            engine.halt_sessions();
            self.settle();
        }
    }

    /// The durable operator title, whether this Thread is live or parked.
    pub fn thread_title(&self, thread: ThreadId) -> Result<Option<String>, LoadError> {
        if let Some(open) = self.thread(thread) {
            return Ok(open.title().map(str::to_string));
        }
        self.parked.peek(thread).map(|meta| meta.title)
    }

    /// What a Thread is called on screen: the title the operator gave it;
    /// else, when `auto` is on, the first line of its first prompt,
    /// shortened; else its number. Reads one header line (and at most
    /// the first few records) off disk for a parked Thread — callers cache
    /// it by moment, never per frame.
    pub fn display_title(&self, thread: ThreadId, auto: bool) -> String {
        let numbered = || format!("thread-{}", thread.get());
        if let Some(open) = self.thread(thread) {
            if let Some(title) = open.title() {
                return title.to_string();
            }
            if !auto {
                return numbered();
            }
            return open
                .transcript()
                .blocks()
                .iter()
                .find_map(|block| match &block.body {
                    crate::transcript::Body::Prompt(text) => Some(title_from_prompt(text)),
                    _ => None,
                })
                .unwrap_or_else(numbered);
        }
        let Ok(meta) = self.parked.peek(thread) else {
            return numbered();
        };
        if let Some(title) = meta.title {
            return title;
        }
        if !auto {
            return numbered();
        }
        self.parked.prompt_title(thread).unwrap_or_else(numbered)
    }

    pub fn rename_thread(&mut self, thread: ThreadId, title: &str) -> io::Result<()> {
        self.act(
            |engine| engine.rename_thread(thread, title),
            |server| {
                run(
                    server,
                    Op::Rename {
                        thread,
                        title: title.to_string(),
                    },
                )
            },
        )
    }

    pub fn queue(&mut self, thread: ThreadId, text: String) -> bool {
        self.act(
            |engine| engine.queue(thread, text.clone()),
            |server| {
                run(
                    server,
                    Op::Queue {
                        thread,
                        text: text.clone(),
                    },
                )
                .is_ok()
            },
        )
    }

    pub fn unqueue(&mut self, thread: ThreadId) -> Option<String> {
        self.act(
            |engine| engine.unqueue(thread),
            |server| match server.execute(Op::Unqueue { thread }) {
                Ok(Some(Done::Text { text })) => Some(text),
                _ => None,
            },
        )
    }

    pub fn cancel_queued(&mut self, thread: ThreadId, restore: bool) {
        self.act(
            |engine| engine.cancel_queued(thread, restore),
            |server| {
                said(
                    server,
                    Op::CancelQueued {
                        thread,
                        restore,
                        edit: false,
                    },
                )
            },
        );
    }

    /// Up edits the visible queued prompt ahead of the current draft.
    pub fn edit_queued(&mut self, thread: ThreadId) {
        self.act(
            |engine| engine.edit_queued(thread),
            |server| {
                said(
                    server,
                    Op::CancelQueued {
                        thread,
                        restore: true,
                        edit: true,
                    },
                )
            },
        );
    }

    /// A prompt the provider handed back, for the Composer, and whether it
    /// goes ahead of the draft.
    pub fn take_retrieved_prompt(&mut self, thread: ThreadId) -> Option<(String, bool)> {
        if let Link::Server(_) = self.link {
            // Asked every frame: only a Thread holding one is asked.
            self.thread(thread)?.recovered()?;
        }
        self.act(
            |engine| engine.take_retrieved_prompt(thread),
            |server| match server.execute(Op::TakeRecovered { thread }) {
                Ok(Some(Done::Recovered { text, prepend })) => Some((text, prepend)),
                _ => None,
            },
        )
    }

    pub fn recall_prompt(
        &mut self,
        thread: ThreadId,
        direction: HistoryDirection,
        current_draft: &str,
    ) -> Option<String> {
        self.act(
            |engine| engine.recall_prompt(thread, direction, current_draft),
            |server| {
                let direction = match direction {
                    HistoryDirection::Older => crate::protocol::WireDirection::Older,
                    HistoryDirection::Newer => crate::protocol::WireDirection::Newer,
                };
                match server.execute(Op::Recall {
                    thread,
                    direction,
                    draft: current_draft.to_string(),
                }) {
                    Ok(Some(Done::Text { text })) => Some(text),
                    _ => None,
                }
            },
        )
    }

    pub fn answer(&mut self, thread: ThreadId, id: &str) -> bool {
        let answered = self.in_process().answer(thread, id);
        self.settle();
        answered
    }

    /// Whether completed turns predict a follow-up, in this process; a
    /// window attached to a Server says `settings_changed`.
    pub fn set_suggestions_enabled(&mut self, enabled: bool) {
        self.in_process().set_suggestions_enabled(enabled);
        self.settle();
    }

    pub fn deliver_suggestion(&mut self, thread: ThreadId, text: String) {
        self.in_process().deliver_suggestion(thread, text)
    }

    /// Settings changed on disk: whether completed turns predict a
    /// follow-up, and where an allowance warns, among them.
    pub fn settings_changed(&mut self, settings: &crate::settings::Settings) {
        self.act(
            |engine| {
                engine.set_suggestions_enabled(settings.placeholder_suggestions);
                engine.set_allowance_thresholds(settings.allowance_warnings.clone());
            },
            |server| said(server, Op::SettingsChanged),
        );
    }

    pub fn live_sessions(&self, provider: Provider) -> usize {
        self.shared.live_sessions(provider)
    }

    pub fn announced_models(&self, provider: Provider) -> Vec<ModelInfo> {
        self.shared.announced_models(provider)
    }

    pub fn account_limits(&self, provider: Provider) -> crate::transcript::RateLimits {
        self.shared.account_limits(provider)
    }

    /// Whether a machine's parked Threads moved since the last ask: its
    /// Server said so. This computer's are read at the window's moments.
    pub fn take_parked_changed(&mut self) -> bool {
        self.parked.take_changed()
    }

    pub fn take_models_changed(&mut self) -> bool {
        self.shared.take_models_changed() | self.mirrors.take_models_changed()
    }

    /// The allowance warnings due since the last take, oldest first.
    pub fn take_allowance_warnings(&mut self) -> Vec<crate::providers::limits::AllowanceWarning> {
        std::mem::take(&mut self.allowance_warnings)
    }

    /// What API callers asked of the windows since the last take.
    pub fn take_asked(&mut self) -> Vec<Asked> {
        std::mem::take(&mut self.asked)
    }

    /// Request workspace commands without creating a Thread or Session.
    pub fn discover_commands(
        &mut self,
        provider: Provider,
        cwd: &std::path::Path,
    ) -> Option<crate::providers::commands::Discovery> {
        match &mut self.link {
            Link::InProcess(engine) => engine.discover_commands(provider, cwd),
            Link::Server(server) => server.discovery().discover_commands(provider, cwd),
        }
    }

    /// The provider's announced model rows, else the fallback catalog.
    pub fn model_catalog(&self, provider: Provider) -> Vec<ModelInfo> {
        crate::providers::models::catalog(provider, &self.announced_models(provider))
    }

    pub fn set_model(
        &mut self,
        thread: ThreadId,
        model: Option<String>,
    ) -> Result<(), ProvisionError> {
        self.act(
            |engine| engine.set_model(thread, model.clone()),
            |server| {
                provisioned(server.execute(Op::SetModel {
                    thread,
                    model: model.clone(),
                }))
            },
        )
    }

    pub fn set_effort(
        &mut self,
        thread: ThreadId,
        effort: Option<String>,
    ) -> Result<(), ProvisionError> {
        self.act(
            |engine| engine.set_effort(thread, effort.clone()),
            |server| {
                provisioned(server.execute(Op::SetEffort {
                    thread,
                    effort: effort.clone(),
                }))
            },
        )
    }

    pub fn next_blocked(&self, from: Option<ThreadId>) -> Option<ThreadId> {
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
    pub fn needs_you(&self) -> Vec<ThreadId> {
        let mut queue = self.blocked();
        queue.sort_by_key(|thread| {
            (
                self.shared
                    .notifications
                    .first_request(*thread)
                    .map_or(u64::MAX, |notice| notice.seq),
                *thread,
            )
        });
        queue
    }

    /// The Thread the answer keys act on when the focused Pane waits on
    /// nothing: the Needs-you strip's first row.
    pub fn answer_target(&self) -> Option<ThreadId> {
        self.needs_you().first().copied()
    }

    /// Threads waiting on the operator — what the wall badges.
    pub fn blocked(&self) -> Vec<ThreadId> {
        self.threads()
            .into_iter()
            .filter(|thread| {
                self.thread(*thread)
                    .is_some_and(|open| !open.activity().decisions().is_empty())
            })
            .collect()
    }

    pub fn notifications(&self) -> &Notifications {
        &self.shared.notifications
    }

    pub fn set_notification_grace(&mut self, grace: Duration) {
        self.in_process().set_notification_grace(grace);
        self.settle();
    }

    pub fn dismiss_notice(&mut self, id: NoticeId) -> bool {
        let known = self.shared.notifications.get(id).is_some();
        self.act(
            |engine| engine.dismiss_notice(id),
            |server| {
                notice(server, NoticeAction::Dismiss { notice: id.get() });
                known
            },
        )
    }

    pub fn dismiss_decision_notice(&mut self, id: &DecisionNoticeId) -> bool {
        let shown = self.shared.notifications.decision(id).is_some();
        self.act(
            |engine| engine.dismiss_decision_notice(id),
            |server| {
                notice(
                    server,
                    NoticeAction::DismissDecision {
                        thread: id.thread,
                        handle: (&id.handle).into(),
                    },
                );
                shown
            },
        )
    }

    pub fn clear_notices(&mut self) {
        self.act(Engine::clear_notices, |server| {
            notice(server, NoticeAction::Clear)
        });
    }

    pub fn mark_notices_read(&mut self) {
        self.act(Engine::mark_notices_read, |server| {
            notice(server, NoticeAction::Read)
        });
    }

    pub fn fixture_backdate_notices(&mut self, thread: ThreadId, at: std::time::SystemTime) {
        self.in_process().fixture_backdate_notices(thread, at);
        self.settle();
    }

    pub fn fixture_notice(&mut self, thread: ThreadId, outcome: crate::TurnOutcome) {
        self.in_process().fixture_notice(thread, outcome);
        self.settle();
    }

    pub fn draft_starting(&self, draft: DraftId) -> bool {
        match &self.link {
            Link::InProcess(engine) => engine.draft_starting(draft),
            Link::Server(server) => server.draft_starting(draft),
        }
    }
}

/// A command that changes something the window will hear about. A refusal
/// is logged: the act's own effect (or its absence) is what the window
/// shows.
fn said(server: &mut remote::Remote, op: Op) {
    if let Err(error) = server.execute(op) {
        crate::log!("the server refused an act: {}", error.message);
    }
}

/// A command that answers with a new or adopted Thread.
fn started(executed: Result<Option<Done>, WireError>) -> io::Result<ThreadId> {
    match executed.map_err(io::Error::from)? {
        Some(Done::Thread { thread }) => Ok(thread),
        _ => Err(io::Error::other("the server answered with no Thread")),
    }
}

/// A command that answers with a registered Project.
fn registered(executed: Result<Option<Done>, WireError>) -> io::Result<ProjectId> {
    match executed.map_err(io::Error::from)? {
        Some(Done::Project { project }) => Ok(project),
        _ => Err(io::Error::other("the server answered with no Project")),
    }
}

/// A provider, model or effort change: the Server's refusal in its words.
fn provisioned(executed: Result<Option<Done>, WireError>) -> Result<(), ProvisionError> {
    executed
        .map(drop)
        .map_err(|error| ProvisionError::Tune(error.into()))
}

/// What an API caller asked the windows to do (ADR 0027).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// Land on this Thread (`thread.focus`).
    Focus(ThreadId),
    /// Show a toast (`notification.show`), ringing its chime; an
    /// important one stays up until dismissed.
    Notification {
        title: String,
        body: Option<String>,
        sound: Option<crate::sound::Sound>,
        important: bool,
    },
    /// Show only this Projects project's block in the nav, by its slug,
    /// its coordinator first; none shows them all (`nav.focus`).
    NavFocus(Option<String>),
}

/// What `Cockpit::close` can refuse: the park that ends a Solo Thread's
/// Session — the Pane is gone either way — or the Group change that takes
/// a member out of the view.
#[derive(Debug)]
pub enum CloseError {
    Park(io::Error),
    Group(ApplyError),
}

impl std::fmt::Display for CloseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CloseError::Park(error) => write!(f, "did not park cleanly: {error}"),
            CloseError::Group(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CloseError {}

/// A session file adopted through `adopt_into` (#11): durable the moment
/// this exists, and open unless `not_opened` says why not.
#[derive(Debug)]
pub struct Adopted {
    pub thread: ThreadId,
    /// The imported Thread would not open: it is durable and parked, exactly
    /// like a launch-time import that would not open.
    pub not_opened: Option<LoadError>,
    /// The blank Thread the door was opened from stayed open: its deletion
    /// was refused.
    pub blank_kept: Option<DeleteError>,
}

/// The operator's acts on what is on screen (#28): every rule about Solo,
/// Group, focus, fullscreen and park order runs here, against the roster,
/// with the reviving, parking and Group changes it takes. The window only
/// mirrors the roster and paints.
impl Cockpit {
    /// What is on screen: read-only. Every change is one of the acts below.
    pub fn roster(&self) -> &Roster {
        &self.roster
    }

    /// The Panes on screen, in order — see `Roster::visible`.
    pub fn visible(&self) -> Vec<PaneIdentity> {
        self.roster.visible(&self.shared.groups)
    }

    /// The one door to focus: every move — keys, clicks, nav rows — lands
    /// here, so fullscreen re-aims with focus. False for a Pane that is not
    /// open.
    pub fn focus(&mut self, identity: PaneIdentity) -> bool {
        let landed = self.roster.focus(identity);
        self.acknowledge_seen();
        landed
    }

    /// A running nav row's click (#21): land on that Thread's Pane, in the
    /// view that shows it — its Group, or Solo.
    pub fn focus_thread(&mut self, thread: ThreadId) -> bool {
        let identity = PaneIdentity::Thread(thread);
        if self.roster.index_of(identity).is_none() {
            return false;
        }
        self.roster.set_view(self.view_showing(thread));
        let landed = self.roster.focus(identity);
        self.acknowledge_seen();
        landed
    }

    /// The view that shows `thread` (R1, Groups are non-exclusive): the
    /// Group on screen when it holds the Thread, else the Thread's home
    /// Group, else Solo — a Thread that is only a guest of Groups lives
    /// loose.
    fn view_showing(&self, thread: ThreadId) -> View {
        match self.roster.view() {
            View::Group(group)
                if self
                    .shared
                    .groups
                    .get(group)
                    .is_some_and(|group| group.members.contains(&thread)) =>
            {
                View::Group(group)
            }
            _ => self
                .shared
                .groups
                .home(thread)
                .map_or(View::Solo, |group| View::Group(group.id)),
        }
    }

    /// Record the Subject the renderer is actually showing. Invalid or stale
    /// children fall back to Main, so a replacement Session cannot suppress a
    /// later Main request with an old selection.
    pub fn set_visible_subject(&mut self, thread: ThreadId, subject: Subject) {
        if subject == Subject::Main {
            self.visible_subjects.remove(&thread);
        } else {
            self.visible_subjects.insert(thread, subject.clone());
        }
        self.act(
            |engine| engine.set_visible_subject(thread, subject.clone()),
            |server| {
                said(
                    server,
                    Op::ShowSubject {
                        thread,
                        subject: (&subject).into(),
                    },
                )
            },
        );
        self.acknowledge_seen();
    }

    /// cmd-] / cmd-[: walk the visible Panes, wrapping.
    pub fn step_focus(&mut self, delta: isize) {
        self.roster.step(delta, &self.shared.groups);
    }

    /// ⌘⇧⏎ (#20): the focused Pane takes the whole cockpit; ⌘⇧⏎ again
    /// restores the grid.
    pub fn toggle_fullscreen(&mut self) {
        self.roster.toggle_fullscreen();
    }

    /// Jump to the next Thread waiting on the operator — the whole point of
    /// a Group you cannot read all of at once. The Thread landed on, if any.
    pub fn next_decision(&mut self) -> Option<ThreadId> {
        let next = self.next_blocked(self.roster.focused_thread())?;
        self.focus_thread(next).then_some(next)
    }

    /// Open a Group (#28): every member gets a Pane, parked ones included.
    /// A Group *is* its membership on screen, so entering one that has a
    /// parked member and showing the rest would be showing a different
    /// Group — but entering spends no Session on a Thread the operator put
    /// away: a parked member's Pane is its parked tile (`Parked 2h ago ·
    /// 11 turns`), and ⏎ on it wakes it (`wake`). Focus stays put when the
    /// operator was already on a member, so entering from one of its own
    /// rows does not jump them.
    pub fn enter_group(&mut self, group: GroupId) -> Result<(), ReviveGroupError> {
        let members = self
            .shared
            .groups
            .get(group)
            .ok_or(ReviveGroupError::MissingGroup)?
            .members
            .clone();
        let retain = self
            .roster
            .focused_thread()
            .filter(|thread| members.contains(thread));
        for thread in &members {
            // Open or parked, the member holds its slot on the board.
            self.roster.insert_thread(*thread);
        }
        self.roster.set_view(View::Group(group));
        if let Some(thread) = retain.or_else(|| members.first().copied()) {
            self.roster.focus(PaneIdentity::Thread(thread));
        }
        Ok(())
    }

    /// ⏎ on a parked Group member's tile: revive it on a fresh Session in
    /// place — its Pane keeps the slot the board gave it, takes focus, and
    /// the park order forgets it. An open Thread is left alone.
    pub fn wake(&mut self, thread: ThreadId) -> Result<(), LoadError> {
        if self.thread(thread).is_some() {
            return Ok(());
        }
        self.revive(thread)?;
        self.roster.note_revived(thread);
        self.roster.focus(PaneIdentity::Thread(thread));
        Ok(())
    }

    /// Revive one parked Thread into the view that shows it (#21): a Pane,
    /// focus, and the park order forgetting it — cmd-o must not revive it a
    /// second time. The shared tail of cmd-o and a parked nav row's click.
    pub fn reopen(&mut self, thread: ThreadId) -> Result<(), LoadError> {
        self.revive(thread)?;
        self.roster.set_view(self.view_showing(thread));
        self.roster.note_revived(thread);
        self.roster.focus(PaneIdentity::Thread(thread));
        Ok(())
    }

    /// cmd-o (#17): reopen the Thread parked most recently — the one the
    /// operator just closed, which is the one they want back. The order is
    /// remembered only for this launch: once it is drained — Threads parked
    /// before a relaunch are never in it — the newest-created parked Thread
    /// is next (accepted v1 behavior). A Thread whose revive fails keeps its
    /// park but loses its slot in the order: cmd-o moves on rather than
    /// jamming on it. None with nothing parked at all.
    pub fn reopen_last(&mut self) -> Option<(ThreadId, Result<(), LoadError>)> {
        let thread = self
            .roster
            .pop_park_order()
            .or_else(|| self.parked().unwrap_or_default().last().copied())?;
        Some((thread, self.reopen(thread)))
    }

    /// cmd-t (#29): open a loose Draft and switch the Cockpit to Solo, even
    /// when the action is invoked from inside a Group. Nothing durable exists
    /// until the first send; the Draft takes focus.
    pub fn open_draft(&mut self) -> DraftId {
        let draft = self.roster.open_draft(DraftScope::default());
        self.roster.set_view(View::Solo);
        draft
    }

    /// Open a Draft in the current view. Shortcuts use this path so invoking
    /// one from a Group preserves that context and the Thread joins the Group
    /// after its first send succeeds.
    pub fn open_draft_in_current_view(&mut self) -> DraftId {
        let group = match self.roster.view() {
            View::Group(group) => Some(group),
            View::Solo => None,
        };
        self.roster.open_draft(DraftScope {
            group,
            new_group_with: None,
            pending_leave: None,
        })
    }

    /// Open a focused draft beside one loose Thread as a provisional pair.
    /// Its first successful send creates the durable Group.
    pub fn open_draft_for_new_group(&mut self, first: ThreadId) -> DraftId {
        self.roster.open_draft(DraftScope {
            group: None,
            new_group_with: Some(first),
            pending_leave: None,
        })
    }

    /// Re-aim the standing draft the way `open_draft` would have aimed a
    /// fresh one: loose, and the Cockpit back to Solo.
    pub fn aim_draft_loose(&mut self, draft: DraftId) {
        self.roster.rescope_draft(draft, None, None);
        self.roster.set_view(View::Solo);
    }

    /// Re-aim the standing draft at the current view — the Group the
    /// operator is looking at, or nothing when they are Solo.
    pub fn aim_draft_at_current_view(&mut self, draft: DraftId) {
        let group = match self.roster.view() {
            View::Group(group) => Some(group),
            View::Solo => None,
        };
        self.roster.rescope_draft(draft, group, None);
    }

    /// Re-aim the standing draft beside one loose Thread as a provisional
    /// pair, as `open_draft_for_new_group` would have.
    pub fn aim_draft_at_new_group(&mut self, draft: DraftId, first: ThreadId) {
        self.roster.rescope_draft(draft, None, Some(first));
    }

    /// The first send (#29): bootstrap the Thread — create, worktree, spawn
    /// — and only then let the prompt go; the Thread takes the draft's own
    /// slot, and joins the Group the draft was pending in. The leave the
    /// draft was holding open (see `close`) applies now, and only now: the
    /// new member is already in, so the pair never dissolves. On any
    /// failure nothing is half-born: no Thread, the Pane stays draft, and
    /// the prompt stays with the caller. `None` means startup is pending;
    /// the draft stays in place until `take_bootstrap_results` reports it.
    pub fn bootstrap_draft(
        &mut self,
        draft: DraftId,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
        effort: Option<String>,
    ) -> io::Result<Option<Bootstrapped>> {
        if self.draft_starting(draft) {
            return Ok(None);
        }
        let scope = self
            .roster
            .draft_scope(draft)
            .ok_or_else(|| io::Error::other("no such draft"))?;
        let started = self.act(
            |engine| {
                engine.bootstrap_draft(
                    draft,
                    scope,
                    choice.clone(),
                    workspace.clone(),
                    prompt,
                    effort.clone(),
                )
            },
            |server| {
                let op = Op::Bootstrap {
                    provider: choice.provider,
                    model: choice.model.clone(),
                    effort: effort.clone(),
                    workspace: (&workspace).into(),
                    prompt: prompt.to_string(),
                    group: scope.group,
                    new_group_with: scope.new_group_with,
                };
                server.bootstrap(draft, prompt, op).map(|()| None)
            },
        );
        Ok(started?.map(|thread| self.complete_draft(draft, thread)))
    }

    fn complete_draft(&mut self, draft: DraftId, thread: ThreadId) -> Bootstrapped {
        let scope = self
            .roster
            .draft_scope(draft)
            .expect("draft retained during startup");
        self.roster.draft_became(draft, thread);
        if scope.new_group_with.is_some() {
            if let Some(group) = self.shared.groups.home(thread).map(|group| group.id) {
                self.roster.set_view(View::Group(group));
                self.roster.focus(PaneIdentity::Thread(thread));
            }
        }
        let refused_leave = scope.pending_leave.and_then(|leaving| {
            self.apply_group(GroupChange::Leave { thread: leaving })
                .err()
        });
        Bootstrapped {
            thread,
            applied_leave: scope.pending_leave.is_some() && refused_leave.is_none(),
            refused_leave,
        }
    }

    /// Cancel a submitted startup before changing a draft's choices. The
    /// draft slot and Composer remain; cancellation emits no completion.
    pub fn cancel_draft_start(&mut self, draft: DraftId) -> io::Result<bool> {
        self.act(
            |engine| engine.cancel_draft_start(draft),
            |server| server.cancel_bootstrap(draft),
        )
    }

    /// The first sends settled since the last take, each Draft's already
    /// completed in this window.
    pub fn take_bootstrap_results(&mut self) -> Vec<BootstrapResult> {
        self.receive();
        std::mem::take(&mut self.bootstrap_results)
    }

    /// Discard a draft Pane (#29): nothing durable dies with it. A leave it
    /// was holding open applies now — nothing came to replace the leaving
    /// Thread, so the pair dissolves exactly as closing that Pane would
    /// have done without a draft in the way.
    pub fn discard_draft(&mut self, draft: DraftId) -> Result<(), ApplyError> {
        self.cancel_draft_start(draft)
            .map_err(|error| ApplyError::Io(error.to_string()))?;
        let Some(scope) = self.roster.remove_draft(draft) else {
            return Ok(());
        };
        if let (Some(_), Some(leaving)) = (scope.group, scope.pending_leave) {
            self.apply_group(GroupChange::Leave { thread: leaving })?;
        }
        Ok(())
    }

    /// Close a Pane — cmd-w. A draft is discarded. A Thread is taken out of
    /// whatever the operator is looking at: in Solo that is a park, plain —
    /// the Session ends, the log stays, and reopening revives it. In a
    /// Group it is a Leave, and the Thread stays open — and the survivor
    /// the operator lands on is the Thread that took this one's ordinal,
    /// not the first member: closing the middle Pane of three should leave
    /// the pointer where it was, the way closing a browser tab does.
    ///
    /// The exception is a pair with a draft pending in it. A Group needs
    /// two members, so leaving a pair dissolves it — which would tear down
    /// the very Group the draft is waiting to join. The leave is therefore
    /// *deferred* onto the draft, and the Group stays whole and visible
    /// until the draft either sends (`bootstrap_draft` applies the leave;
    /// membership never dips below two) or is discarded (the leave applies
    /// then, and the pair dissolves as it always would).
    pub fn close(&mut self, identity: PaneIdentity) -> Result<(), CloseError> {
        match identity {
            PaneIdentity::Draft(draft) => self.discard_draft(draft).map_err(CloseError::Group),
            PaneIdentity::Thread(thread) => self.park_thread(thread),
        }
    }

    /// Park a Thread in one act, wherever it sits — cmd-w and every "Park
    /// thread". A Group member leaves its Group first (onto the ordinal
    /// survivor when its Group is on screen, deferred onto a pending draft
    /// in a pair), then parks: it lands in the parked rows at once, never
    /// as a loose open Thread that needs a second park.
    pub fn park_thread(&mut self, thread: ThreadId) -> Result<(), CloseError> {
        if self.shared.groups.of(thread).is_some() {
            // Leave takes the Thread out of every Group; when the Group on
            // screen held it, its ordinal survivor takes focus.
            let on_screen = matches!(
                self.roster.view(),
                View::Group(group) if self
                    .shared
            .groups
                    .get(group)
                    .is_some_and(|group| group.members.contains(&thread))
            );
            if on_screen {
                self.close_thread(thread)?;
            } else {
                self.apply_group(GroupChange::Leave { thread })
                    .map_err(CloseError::Group)?;
            }
        }
        self.park_noted(thread)
    }

    /// Close a Thread's Pane in the view it is in: in a Group that is a
    /// leave (the drag-out door, which follows the Thread to Solo), in Solo
    /// a park. cmd-w goes through `park_thread`, which always parks.
    fn close_thread(&mut self, thread: ThreadId) -> Result<(), CloseError> {
        if let View::Group(group) = self.roster.view() {
            let members = self
                .shared
                .groups
                .get(group)
                .map(|group| group.members.clone())
                .unwrap_or_default();
            let ordinal = members
                .iter()
                .position(|member| *member == thread)
                .unwrap_or(0);
            if members.len() == 2 {
                if let Some(draft) = self.roster.pending_draft(group) {
                    self.roster.defer_leave(draft, thread);
                    let survivor = members[usize::from(members[0] == thread)];
                    self.roster.focus(PaneIdentity::Thread(survivor));
                    return Ok(());
                }
            }
            let applied = self
                .apply_group(GroupChange::Leave { thread })
                .map_err(CloseError::Group)?;
            // A dissolved Group already landed on its survivor.
            if !applied.dissolved.iter().any(|item| item.group == group) {
                if let Some(group) = self.shared.groups.get(group) {
                    let next = group.members[ordinal.min(group.members.len() - 1)];
                    self.roster.focus(PaneIdentity::Thread(next));
                }
            }
            return Ok(());
        }
        self.park_noted(thread)
    }

    /// Solo's park. Parked even on a flush error — the Session is gone
    /// either way, so cmd-o should still bring this Thread back first —
    /// and the clamped survivor takes focus and, while fullscreen, the
    /// screen (#20).
    fn park_noted(&mut self, thread: ThreadId) -> Result<(), CloseError> {
        let re_aim = self.roster.fullscreen() == Some(PaneIdentity::Thread(thread));
        let parked = self.park(thread);
        self.roster.note_parked(thread);
        if re_aim {
            self.roster.fullscreen_focused();
        }
        parked.map_err(CloseError::Park)
    }

    /// A nav drag's drop (#28): the plan the Groups module makes of it,
    /// applied in the View the drag started from — the row's own mouse-down
    /// fires before the drag does, so by the time the drop lands the view
    /// is wherever the *press* took the operator, not where they picked the
    /// row up. Dragging a member out of a Group is the pointer's spelling
    /// of closing its Pane, so it takes the same door — which defers the
    /// leave when a draft is pending in a pair, and keeps the
    /// ordinal-preserving focus. A refused or empty plan changes nothing.
    pub fn drop(&mut self, drag: Drag, origin: View, target: DropTarget) -> Result<(), ApplyError> {
        self.roster.set_view(origin);
        let outcome = match self.shared.groups.preview_drop(drag, target) {
            Plan::Change(GroupChange::Leave { thread })
                if matches!(drag, Drag::Thread { group: Some(_), .. }) =>
            {
                let Drag::Thread {
                    group: Some(group), ..
                } = drag
                else {
                    unreachable!("the guard above matched this exact shape");
                };
                self.roster.set_view(View::Group(group));
                let closed = self.close_thread(thread).map_err(|error| match error {
                    CloseError::Group(error) => error,
                    CloseError::Park(error) => error.into(),
                });
                // Out of the Group for real — either the leave applied, or
                // it is parked on a draft. Either way the dragged Thread is
                // no longer a member, so the operator follows it to Solo.
                if self.roster.pending_leave(group) == Some(thread)
                    || self.shared.groups.of(thread).is_none()
                {
                    self.roster.set_view(View::Solo);
                    self.roster.focus(PaneIdentity::Thread(thread));
                }
                closed
            }
            Plan::Change(change) => self.apply_group(change).map(|_| ()),
            Plan::Refused(_) | Plan::Nothing => Ok(()),
        };
        self.roster.heal_focus(&self.shared.groups);
        outcome
    }

    /// Adopt a CLI session file (#11), in place of the Pane it was picked
    /// for or, with none, alongside the Panes shown: a draft becomes the
    /// imported Thread; a still-blank Thread yields its slot and is deleted
    /// — clean exactly while it is blank, which the picker's own invariant
    /// guarantees and this re-checks. Import creates the Thread; revive
    /// opens it — the same replay-and-resume any parked Thread gets — and
    /// it takes focus. A refusal is the import module's, unchanged, and no
    /// Thread was created.
    pub fn adopt_into(
        &mut self,
        into: Option<PaneIdentity>,
        path: &Path,
    ) -> Result<Adopted, crate::import::ImportError> {
        let thread = self.import(path)?;
        let mut adopted = Adopted {
            thread,
            not_opened: None,
            blank_kept: None,
        };
        let Some(into) = into else {
            adopted.not_opened = self.reopen(thread).err();
            return Ok(adopted);
        };
        match self.revive(thread) {
            Ok(()) => {
                match into {
                    PaneIdentity::Draft(draft) => {
                        self.cancel_draft_start(draft)
                            .map_err(crate::import::ImportError::Io)?;
                        self.roster.draft_became(draft, thread);
                    }
                    PaneIdentity::Thread(blank) => {
                        if self
                            .thread(blank)
                            .is_some_and(|open| open.transcript().offers_import())
                        {
                            adopted.blank_kept = self.delete(blank).err();
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
    pub fn parked_in_order(&self) -> io::Result<Vec<ThreadId>> {
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
