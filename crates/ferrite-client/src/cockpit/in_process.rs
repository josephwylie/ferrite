//! An Engine in this process (ADR 0049): what the tests, the demo and the
//! visual reference run in place of a Server, behind the same `Link` a
//! Server is reached through. It is a fixture: only a build with
//! `test-support` compiles it, and a released window holds only Servers.
//! An act is the command a Server reads off its socket, answered by the
//! Engine code the Server runs; in debug builds each command, reply and
//! message crosses as its encoded line, so nothing escapes the contract.
//! It also stands in for what a Server's machine has besides its Engine —
//! the forge, the GitHub login, the Factory, the disk — with what a
//! fixture world says of them.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use ferrite_core::engine::{AgentLaunch, Engine, Preparation, RssSampler, SpawnRequest, Spawner};
use ferrite_core::checkout::WorkspaceChoice;
use ferrite_core::clock::ServerClock;
use ferrite_core::engine::Launch;
use ferrite_core::machine::Bridge;
use ferrite_core::protocol::{
    CliAction, ClientMessage, Command, Done, GitHubAsk, Op, ReviewPullRequest, ReviewPullRequests,
    ServerMessage, WirePrFilter,
};
use ferrite_core::provider::Provider;
use ferrite_core::roster::DraftId;
use ferrite_core::store::Store;
use ferrite_core::transcript::Input;
use ferrite_core::{MachineId, MachineKey, ThreadId, ThreadKey};
use ferrite_reviewr::ferrite::{Bases, FileSides, Read};
use ferrite_reviewr::forge::PrView;
use ferrite_reviewr::git::{CommitRow, WorktreeEntry};
use ferrite_reviewr::search::SearchResults;

use super::held::{Executed, HeldMachine, Link};
use super::remote::{Answer, Arrival};
use super::{mirror, read_here, Ask, Cockpit};

mod factory;

/// The window number an Engine in this process knows its one window by.
const IN_PROCESS_WINDOW: u64 = 0;

/// One machine's Engine, run by its window, and the fixture world around
/// it.
pub(super) struct InProcess {
    pub(super) engine: Engine,
    /// Its Server is one from before `Capability::ForkOf`
    /// (`fixture_server_before_fork_of`).
    before_fork_of: bool,
    /// The self-hosted forges a review's pull request is read from
    /// (`settings_changed`).
    forge: ferrite_reviewr::config::PluginConfig,
    /// What its forge says (`fixture_forge`), by checkout: answered in
    /// place of its CLI.
    forge_fixture: HashMap<PathBuf, PrView>,
    /// Its pull requests read by number, their diffs, and its lists.
    forge_numbered: HashMap<(PathBuf, u64), ReviewPullRequest>,
    forge_diffs: HashMap<(PathBuf, u64), Vec<FileSides>>,
    forge_lists: HashMap<(PathBuf, WirePrFilter), ReviewPullRequests>,
    /// The checkouts whose whole-PR reads wait for a test's answer
    /// (`fixture_hold_pr_reads`), and the reads waiting.
    forge_held: HashSet<PathBuf>,
    held_reads: Vec<HeldRead>,
    /// What its disk says is free (`fixture_free_disk`), in place of the
    /// volume.
    free_disk: Option<u64>,
    /// Where its sessions to import are (`fixture_session_roots`): none
    /// until a test gives it some.
    session_roots: Vec<(Provider, PathBuf)>,
    /// Its Server accepts every browser act (`fixture_browser_accepts`),
    /// answering an empty browser.
    browser_accepts: bool,
    /// Its Factory, answering Factory asks as a Server's would.
    factory: factory::FactoryFixture,
    /// What its Server said besides its Engine, for the next `poll`.
    heard: Vec<ServerMessage>,
}

impl InProcess {
    fn new(engine: Engine) -> Self {
        Self {
            engine,
            before_fork_of: false,
            forge: Default::default(),
            forge_fixture: HashMap::new(),
            forge_numbered: HashMap::new(),
            forge_diffs: HashMap::new(),
            forge_lists: HashMap::new(),
            forge_held: HashSet::new(),
            held_reads: Vec::new(),
            free_disk: None,
            session_roots: Vec::new(),
            browser_accepts: false,
            factory: Default::default(),
            heard: Vec::new(),
        }
    }

    /// Whether its forge answers for the checkout at `path`.
    fn has_forge_fixture(&self, path: &Path) -> bool {
        self.forge_fixture.contains_key(path)
            || self
                .forge_numbered
                .keys()
                .any(|(fixture, _)| fixture == path)
            || self.forge_lists.keys().any(|(fixture, _)| fixture == path)
            || self.forge_diffs.keys().any(|(fixture, _)| fixture == path)
    }
}

impl Link for InProcess {
    /// What its Engine published, each message crossing as its encoded
    /// line in debug builds, then the first sends that settled and what
    /// its fixture world's Server said.
    fn poll(&mut self) -> Vec<Arrival> {
        let before_fork_of = self.before_fork_of;
        let mut arrived: Vec<Arrival> = self
            .engine
            .take_messages()
            .into_iter()
            .map(|message| match message {
                ServerMessage::Focus { thread, .. } if before_fork_of => ServerMessage::Focus {
                    thread,
                    fork_of: None,
                },
                message => message,
            })
            .map(|message| Arrival::Message(crossed(message)))
            .collect();
        arrived.extend(
            self.engine
                .take_bootstrap_results()
                .into_iter()
                .map(Arrival::Started),
        );
        arrived.extend(self.heard.drain(..).map(Arrival::Message));
        arrived
    }

    fn boot(&self) -> Instant {
        self.engine.boot()
    }

    /// This computer's.
    fn clock(&self) -> ServerClock {
        ServerClock::default()
    }

    fn bridge(&self) -> Option<&Arc<Bridge>> {
        None
    }

    fn reaches_server(&self) -> bool {
        true
    }

    /// It is this build's, so it does, unless its fixture world's Server
    /// is older.
    fn names_fork_owners(&self) -> bool {
        !self.before_fork_of
    }

    fn answers_now(&self) -> bool {
        true
    }

    fn execute(&mut self, op: Op) -> Executed {
        replied(self.engine.execute(&commanded(op)))
    }

    fn tell(&mut self, op: Op) {
        match op {
            // What only a window knows, as a Server hears it of each.
            Op::Focused { .. } | Op::Composing { .. } => {
                self.engine.window_says(IN_PROCESS_WINDOW, &op)
            }
            op => said(self.execute(op)),
        }
    }

    fn ask_act(&mut self, op: Op, answer: Answer) {
        let reply = match op {
            Op::BrowserAttach { .. }
            | Op::Browser { .. }
            | Op::BrowserReclaimed { .. }
            | Op::BrowserPage { .. }
                if self.browser_accepts =>
            {
                Ok(Some(Done::Browser {
                    state: Default::default(),
                }))
            }
            op => self.execute(op),
        };
        answer(reply);
    }

    /// As a Server streams a watch: what the Engine published was taken
    /// this pump, so the history meets it exactly.
    fn stream(&mut self, watched: &[ThreadId], mirrors: &mut mirror::Mirrors) {
        let fresh: Vec<ThreadId> = watched
            .iter()
            .filter(|thread| !mirrors.watched(**thread))
            .copied()
            .collect();
        mirrors.watch_only(watched);
        let boot = self.engine.boot();
        for thread in fresh {
            if let Some(history) = self.engine.history(thread) {
                let _ = history.send(&mut |message| {
                    mirrors.receive(message, boot);
                    Ok(())
                });
            }
        }
    }

    fn bootstrap(&mut self, draft: DraftId, _: &str, op: Op) -> io::Result<Option<ThreadId>> {
        let started = self
            .engine
            .bootstrap_command(draft, &commanded(op))
            .map(|thread| thread.map(|thread| Done::Thread { thread }));
        started_now(replied(started))
    }

    fn draft_starting(&self, draft: DraftId) -> bool {
        self.engine.draft_starting(draft)
    }

    fn cancel_bootstrap(&mut self, draft: DraftId) -> io::Result<bool> {
        self.engine.cancel_draft_start(draft)
    }

    fn settings_changed(&mut self, settings: &ferrite_core::settings::Settings) {
        self.forge = settings.forge_hosts.review_config();
        self.engine.apply_settings(settings);
    }

    fn pump(&mut self) {
        self.engine.pump();
    }

    fn sweep(&mut self) -> Vec<ferrite_core::memory::Restart> {
        self.engine.sweep()
    }

    fn halt_sessions(&mut self) {
        self.engine.halt_sessions();
    }

    /// Answered by its fixture world's Factory, which tells the window its
    /// board as a Server's does; an Engine has no Factory of its own.
    fn factory(&mut self, op: Op) -> io::Result<Option<Done>> {
        let Some((done, board)) = self.factory.execute(op) else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "this window runs its agents itself",
            ));
        };
        if let Some(board) = board {
            // It rings as a Server's message does (ADR 0041).
            ferrite_core::doorbell::ring();
            self.heard.push(ServerMessage::Factory { revision: 0, board });
        }
        done
    }

    /// What its Spawner lists, else the files under its roots, at once.
    fn list_sessions(&mut self, cap: usize) -> Ask<ferrite_core::session_file::Found> {
        let roots = self.session_roots.clone();
        match self.engine.discover_sessions(roots.clone(), cap) {
            Some(listing) => Ask::of(listing),
            None => Ask::answered(Ok(ferrite_core::session_file::Found {
                sessions: ferrite_core::session_file::candidates(&roots, cap),
                roots: roots.into_iter().map(|(_, root)| root).collect(),
            })),
        }
    }

    fn list_commands(&mut self, provider: Provider, cwd: &Path) -> Ask<Vec<ferrite_core::SessionCommand>> {
        match self.engine.discover_commands(provider, cwd) {
            Some(listing) => Ask::of(listing),
            None => Ask::answered(Ok(Vec::new())),
        }
    }

    fn search_files(
        &mut self,
        thread: ThreadId,
        query: &str,
    ) -> io::Result<Receiver<io::Result<Vec<ferrite_core::prompt_files::FileSuggestion>>>> {
        self.engine.search_files(thread, query)
    }

    fn upload_attachment(&self, path: PathBuf) -> io::Result<ferrite_core::attachment_transfer::Upload> {
        let store = self.engine.browser_store().dir().to_path_buf();
        Ok(ferrite_core::attachment_transfer::upload(path, move || {
            let mut session = ferrite_core::attachment_transfer::Session::new(store);
            Ok(Box::new(move |request| session.execute(request)))
        }))
    }

    fn read_attachment(&self, path: PathBuf) -> io::Result<Receiver<io::Result<Vec<u8>>>> {
        let store = self.engine.browser_store().dir().to_path_buf();
        Ok(read_here(move || {
            let mut session = ferrite_core::attachment_transfer::Session::new(store);
            ferrite_core::attachment_transfer::read(path, |request| session.execute(request))
                .map_err(|error| error.to_string())
        }))
    }

    /// Read from where its browser left it on this disk.
    fn annotation_image(
        &mut self,
        _: ThreadId,
        annotation: ferrite_core::browser_annotations::Annotation,
    ) -> Ask<PathBuf> {
        Ask::of(read_here(move || {
            annotation
                .local_image()
                .and_then(|bytes| annotation.cache_image(&bytes))
                .map_err(|error| error.to_string())
        }))
    }

    // Its machine's reads, each here on a thread of its own, as a Server's
    // would be answered.

    fn changes(
        &mut self,
        request: ferrite_core::changes::Request,
    ) -> io::Result<Receiver<io::Result<ferrite_core::changes::Snapshot>>> {
        Ok(read_here(move || {
            ferrite_core::changes::execute(request)?
                .snapshot()
                .ok_or_else(|| "Missing Changes snapshot".into())
        }))
    }

    fn set_file_staged(&mut self, request: ferrite_core::changes::Request) -> io::Result<Receiver<io::Result<()>>> {
        Ok(read_here(move || ferrite_core::changes::execute(request).map(|_| ())))
    }

    fn file_operation(
        &mut self,
        request: ferrite_core::file_operations::Request,
    ) -> io::Result<Receiver<io::Result<ferrite_core::file_operations::Reply>>> {
        Ok(read_here(move || {
            ferrite_core::file_operations::execute(request).map_err(|error| error.to_string())
        }))
    }

    fn repositories(&mut self) -> io::Result<Receiver<io::Result<ferrite_core::protocol::Repositories>>> {
        let read = self.engine.repositories();
        Ok(read_here(move || Ok(read())))
    }

    fn review_diff(&mut self, path: &Path, read: &Read) -> io::Result<Receiver<io::Result<Vec<FileSides>>>> {
        let (path, read) = (path.to_path_buf(), read.clone());
        Ok(read_here(move || ferrite_reviewr::ferrite::review(&path, &read)))
    }

    fn review_commits(&mut self, path: &Path) -> io::Result<Receiver<io::Result<Vec<CommitRow>>>> {
        let path = path.to_path_buf();
        Ok(read_here(move || ferrite_reviewr::ferrite::commits(&path)))
    }

    fn search_repo(&mut self, path: &Path, query: &str) -> io::Result<Receiver<io::Result<SearchResults>>> {
        let (path, query) = (path.to_path_buf(), query.to_string());
        Ok(read_here(move || ferrite_reviewr::ferrite::search(&path, &query)))
    }

    fn read_review_file(&mut self, path: &Path, file: &str) -> io::Result<Receiver<io::Result<String>>> {
        let (path, file) = (path.to_path_buf(), file.to_string());
        Ok(read_here(move || {
            ferrite_reviewr::ferrite::read_file(&path, &file)
                .map_err(|notice| notice.message().to_string())
        }))
    }

    fn review_pull_request_diff(
        &mut self,
        path: &Path,
        number: u64,
    ) -> io::Result<Receiver<io::Result<Vec<FileSides>>>> {
        if let Some(files) = self.forge_diffs.get(&(path.to_path_buf(), number)).cloned() {
            return Ok(read_here(move || Ok(files)));
        }
        if self.has_forge_fixture(path) {
            return Ok(read_here(|| Err("No published diff fixture for this pull request".to_string())));
        }
        let (path, forge) = (path.to_path_buf(), self.forge.clone());
        Ok(read_here(move || {
            ferrite_reviewr::forge::diff_numbered(&path, &forge, number)
        }))
    }

    fn review_pull_request(
        &mut self,
        path: &Path,
        number: Option<u64>,
    ) -> io::Result<Receiver<io::Result<ReviewPullRequest>>> {
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
            let view = if number.is_none_or(|number| matches!(&view, PrView::Pr(pr) if pr.number == number)) {
                view
            } else {
                PrView::NoPr
            };
            return Ok(read_here(move || Ok(ReviewPullRequest::of(view, "PR"))));
        }
        let (path, forge) = (path.to_path_buf(), self.forge.clone());
        Ok(read_here(move || {
            let (view, noun) = match number {
                Some(number) => ferrite_reviewr::ferrite::pull_request_numbered(&path, &forge, number),
                None => ferrite_reviewr::ferrite::pull_request(&path, &forge),
            };
            Ok(ReviewPullRequest::of(view, noun))
        }))
    }

    fn list_pull_requests(
        &mut self,
        path: &Path,
        filter: WirePrFilter,
    ) -> io::Result<Receiver<io::Result<ReviewPullRequests>>> {
        if let Some(list) = self.forge_lists.get(&(path.to_path_buf(), filter)).cloned() {
            return Ok(read_here(move || Ok(list)));
        }
        let (path, forge) = (path.to_path_buf(), self.forge.clone());
        Ok(read_here(move || {
            let list = ferrite_reviewr::forge::list(&path, &forge, filter.into());
            Ok(ReviewPullRequests::of(list, "PR"))
        }))
    }

    fn review_files(&mut self, path: &Path) -> io::Result<Receiver<io::Result<Vec<WorktreeEntry>>>> {
        let path = path.to_path_buf();
        Ok(read_here(move || ferrite_reviewr::ferrite::all_files(&path)))
    }

    fn review_bases(&mut self, path: &Path) -> io::Result<Receiver<io::Result<Bases>>> {
        let path = path.to_path_buf();
        Ok(read_here(move || ferrite_reviewr::ferrite::bases(&path)))
    }

    fn pick_review_base(&mut self, path: &Path, base: &str) -> io::Result<Receiver<io::Result<()>>> {
        let (path, base) = (path.to_path_buf(), base.to_string());
        Ok(read_here(move || ferrite_reviewr::ferrite::pick_base(&path, &base)))
    }

    fn pull_request(
        &mut self,
        path: &Path,
    ) -> io::Result<Receiver<io::Result<Option<ferrite_core::pull_request::PullRequest>>>> {
        if let Some(view) = self.forge_fixture.get(path) {
            let reading = ferrite_core::pull_request::PullRequest::of_view(view);
            return Ok(read_here(move || Ok(reading)));
        }
        let path = path.to_path_buf();
        Ok(read_here(move || Ok(ferrite_core::workspace::pull_request(&path))))
    }

    /// Only stopped here: a new watch is a Server's watcher's.
    fn watch_pr(&mut self, op: Op) -> io::Result<Receiver<io::Result<bool>>> {
        let done = self.execute(op);
        Ok(read_here(move || match done {
            Ok(Some(Done::PrWatch { watching })) => Ok(watching),
            Ok(_) => Err("the server answered something else".to_string()),
            Err(error) => Err(error.message),
        }))
    }

    /// Refused: an Engine in this process follows no Delegate.
    fn cancel_delegate(&mut self, op: Op) -> io::Result<Receiver<io::Result<()>>> {
        let done = self.execute(op);
        Ok(read_here(move || done.map(drop).map_err(|error| error.message)))
    }

    /// Its fixture world's GitHub login, else `gh` on this computer.
    fn github_names(&mut self, ask: GitHubAsk) -> io::Result<Receiver<io::Result<Vec<String>>>> {
        if let Some(names) = self.factory.github_names(&ask) {
            return Ok(read_here(move || Ok(names)));
        }
        Ok(read_here(move || {
            use ferrite_core::github::GitHub;
            let gh = ferrite_core::github::Gh;
            match ask {
                GitHubAsk::Repositories => gh.repositories(),
                GitHubAsk::Labels { repository } => gh.labels(&repository),
                GitHubAsk::Unknown => Ok(Vec::new()),
            }
        }))
    }

    fn cli(&mut self, action: CliAction) -> io::Result<Receiver<io::Result<Done>>> {
        Ok(read_here(move || {
            ferrite_core::providers::clis::perform(&ferrite_core::providers::clis::Host, &action)
                .map_err(|error| error.message)
        }))
    }

    fn in_process(&mut self) -> Option<&mut InProcess> {
        Some(self)
    }

    fn in_process_ref(&self) -> Option<&InProcess> {
        Some(self)
    }
}

/// An Op as the Server reads it off the socket.
fn commanded(op: Op) -> Op {
    let ClientMessage::Command(Command { op, .. }) =
        crossed(ClientMessage::Command(Command { id: 0, op }))
    else {
        unreachable!("a command decodes as a command")
    };
    op
}

/// A command's outcome as the window reads the Server's reply.
fn replied(executed: Executed) -> Executed {
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

/// A message as the other end of a socket reads it: in debug builds, its
/// encoded line decoded again.
fn crossed<T: serde::Serialize + serde::de::DeserializeOwned>(message: T) -> T {
    #[cfg(debug_assertions)]
    let message = {
        let line = ferrite_core::protocol::encode(&message);
        ferrite_core::protocol::decode(&line[..line.len() - 1])
            .expect("every message decodes as it encoded")
    };
    message
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

/// A first send's reply: the Thread it became, or `None` while it starts.
fn started_now(executed: Executed) -> io::Result<Option<ThreadId>> {
    match executed.map_err(io::Error::from)? {
        Some(Done::Thread { thread }) => Ok(Some(thread)),
        _ => Ok(None),
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

/// A window driving Engines of its own, in this process: a fixture world.
impl Cockpit {
    /// A window driving an Engine of its own, in this process: this
    /// computer.
    pub fn try_new(store: Store, spawner: Box<dyn Spawner>) -> io::Result<Self> {
        let mut cockpit = Self::empty();
        cockpit.hold_machine(Self::in_process_machine(store, spawner, MachineKey(None))?);
        Ok(cockpit)
    }

    /// Test and embedding convenience. The application uses `try_new` so a
    /// protected or incompatible registry is reported instead of replaced.
    pub fn new(store: Store, spawner: Box<dyn Spawner>) -> Self {
        Self::try_new(store, spawner).expect("the workspace registry opens")
    }

    /// An Engine of its own in this process, saved as `saved_as`.
    fn in_process_machine(
        store: Store,
        spawner: Box<dyn Spawner>,
        saved_as: MachineKey,
    ) -> io::Result<HeldMachine> {
        let logs = store.clone();
        let mut engine = Engine::try_new(store, spawner)?;
        // Its window pumps it: a park is listed by the pump that parked it.
        engine.read_parked_in_place();
        Ok(HeldMachine::new(
            Box::new(InProcess::new(engine)),
            Some(logs),
            saved_as,
        ))
    }

    fn hold_machine(&mut self, machine: HeldMachine) -> MachineId {
        let id = machine.id;
        self.machines.insert(id, machine);
        self.roster.set_start(self.start_machine());
        self.receive_from(id);
        id
    }

    /// The first machine the window holds: what a fixture world with one
    /// machine names when it names none.
    fn first(&self) -> MachineId {
        *self.machines.keys().next().expect("the window holds a machine")
    }

    /// The Engine in this process of the first machine the window holds:
    /// what only the demo and the tests drive — fixtures, injected inputs,
    /// the launch-time revival a Server does for itself. It rings the
    /// bell, as an act does.
    fn in_process(&mut self) -> &mut Engine {
        let first = self.first();
        self.in_process_at(first)
    }

    /// The Engine in this process that `key`'s Thread runs in.
    fn in_process_of(&mut self, key: ThreadKey) -> &mut Engine {
        self.in_process_at(key.machine())
    }

    fn in_process_at(&mut self, machine: MachineId) -> &mut Engine {
        ferrite_core::doorbell::ring();
        let link = &mut self
            .machines
            .get_mut(&machine)
            .expect("the Thread's machine is held")
            .link;
        &mut link
            .in_process()
            .expect("only an Engine in this process takes this act")
            .engine
    }

    /// The fixture world: what the first machine's Engine in this process
    /// stands in for besides itself.
    fn fixture(&mut self) -> &mut InProcess {
        let first = self.first();
        self.machines
            .get_mut(&first)
            .expect("held")
            .link
            .in_process()
            .expect("a fixture world runs its Engine in this process")
    }

    /// The fixture world, when the window's first machine is one.
    fn fixture_ref(&self) -> Option<&InProcess> {
        self.machines.values().next()?.link.in_process_ref()
    }

    /// A message a fixture world's Server (its first machine's) would have
    /// sent.
    fn receive_fixture(&mut self, message: ServerMessage) {
        // It rings as a Server's message does (ADR 0041).
        ferrite_core::doorbell::ring();
        self.arrive(self.first(), vec![Arrival::Message(message)]);
    }
}

/// What only an Engine in this process is told: injected input and the
/// fixtures a test or a scene sets in it, each taken in at once.
impl Cockpit {
    /// Watch Session memory in this process. A Server watches its own.
    pub fn watch_memory(&mut self, sampler: Box<dyn RssSampler>, limit: u64) {
        self.in_process().watch_memory(sampler, limit);
        self.receive();
    }

    /// Something only `--demo` and the tests put in a Pane directly.
    pub fn apply_input(&mut self, thread: ThreadKey, input: Input) {
        self.in_process_of(thread)
            .apply_input(thread.thread(), input);
        self.receive_from(thread.machine());
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

    pub fn set_notification_grace(&mut self, grace: Duration) {
        self.in_process().set_notification_grace(grace);
        self.receive();
    }

    /// A fixture world's owned Thread (ADR 0037): one a Task or a
    /// Factory started, as the Server's API starts one for it.
    pub fn fixture_open_owned(
        &mut self,
        provider: Provider,
        workspace: WorkspaceChoice,
        owner: ferrite_core::ThreadOwner,
    ) -> io::Result<ThreadKey> {
        let launch = Launch {
            owner: Some(owner),
            ..Default::default()
        };
        let first = self.first();
        let opened = self.in_process().open_with(provider, workspace, launch);
        self.receive_from(first);
        Ok(ThreadKey::new(first, opened?))
    }

    pub fn fixture_notice(&mut self, thread: ThreadKey, outcome: ferrite_core::TurnOutcome) {
        self.in_process_of(thread)
            .fixture_notice(thread.thread(), outcome);
        self.receive_from(thread.machine());
    }

    /// A fixture world's PR watches for `thread` (ADR 0046), as the
    /// Server's watcher would have saved them.
    pub fn fixture_pr_watch(&mut self, thread: ThreadKey, fact: ferrite_core::pull_request::WatchFact) {
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

    /// A fixture world's Server is one from before `Capability::ForkOf`:
    /// its focus for a fork that opened names no `fork_of`.
    pub fn fixture_server_before_fork_of(&mut self) {
        self.fixture().before_fork_of = true;
    }
}

/// What a fixture world's Server has besides its Engine — its forge, its
/// GitHub login, its Factory, its disk, its browser — said here in place
/// of a Server's.
impl Cockpit {
    /// A fixture world's forge: from now on both pull request reads of the
    /// checkout at `path` — the Review's whole snapshot
    /// (`review_pull_request`) and the nav label's (`pull_request`) — answer
    /// `view`, as its forge's CLI would, without running one.
    pub fn fixture_forge(&mut self, path: PathBuf, view: ferrite_reviewr::forge::PrView) {
        self.fixture().forge_fixture.insert(path, view);
    }

    /// A fixture world's pull request `number` of the repository the
    /// checkout at `path` belongs to, as the read by number answers it.
    pub fn fixture_forge_numbered(
        &mut self,
        path: PathBuf,
        number: u64,
        read: ferrite_core::protocol::ReviewPullRequest,
    ) {
        self.fixture().forge_numbered.insert((path, number), read);
    }

    pub fn fixture_forge_diff(&mut self, path: PathBuf, number: u64, files: Vec<ferrite_reviewr::ferrite::FileSides>) {
        self.fixture().forge_diffs.insert((path, number), files);
    }

    /// A fixture world's list of the repository the checkout at `path`
    /// belongs to, as `filter` shows it.
    pub fn fixture_pr_list(
        &mut self,
        path: PathBuf,
        filter: ferrite_core::protocol::WirePrFilter,
        list: ferrite_core::protocol::ReviewPullRequests,
    ) {
        self.fixture().forge_lists.insert((path, filter), list);
    }

    /// From now on the whole-PR reads of the checkout at `path` (a
    /// `fixture_forge` one) wait, each, for a test to answer it, in any
    /// order (`take_held_reads`).
    pub fn fixture_hold_pr_reads(&mut self, path: PathBuf) {
        self.fixture().forge_held.insert(path);
    }

    /// The whole-PR reads held since the last call, oldest first.
    pub fn take_held_reads(&mut self) -> Vec<HeldRead> {
        std::mem::take(&mut self.fixture().held_reads)
    }

    /// Whether a fixture world's forge answers for the checkout at `path`:
    /// the one PR read tests and captures make, since they never run a CLI.
    pub fn has_forge_fixture(&self, path: &Path) -> bool {
        self.fixture_ref()
            .is_some_and(|fixture| fixture.has_forge_fixture(path))
    }

    /// A fixture world's GitHub login: the repositories it reaches and
    /// each one's labels, answered in place of `gh` (#225).
    pub fn fixture_github(&mut self, repositories: Vec<(String, Vec<String>)>) {
        self.fixture().factory.github = Some(repositories);
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
        self.fixture().browser_accepts = true;
    }

    /// A fixture world's free disk: from now on the bar's `N GB free`
    /// reads `bytes` (`free_disk_fixture`) instead of the volume.
    pub fn fixture_free_disk(&mut self, bytes: u64) {
        self.fixture().free_disk = Some(bytes);
    }

    /// A fixture world's sessions to import: the files under `roots`,
    /// unless its Spawner lists its own.
    pub fn fixture_session_roots(&mut self, roots: Vec<(Provider, PathBuf)>) {
        self.fixture().session_roots = roots;
    }

    /// The free bytes a fixture world stands in for the volume, if any.
    pub fn free_disk_fixture(&self) -> Option<u64> {
        self.fixture_ref()?.free_disk
    }

    /// A fixture world's Factory board, as a Server would say it.
    pub fn fixture_factory(&mut self, board: ferrite_core::factory_board::Board) {
        self.receive_fixture(ServerMessage::Factory { revision: 0, board });
    }

    /// A fixture world with a Factory of its own that answers Workflow
    /// asks as a Server's does, starting empty.
    pub fn fixture_live_factory(&mut self) {
        let factory = ferrite_core::factory::Factory::in_memory();
        let board = factory.board();
        self.fixture().factory.factory = Some(factory);
        self.receive_fixture(ServerMessage::Factory { revision: 0, board });
    }

    /// A fixture world keeps its Task asks, each answered as changed, to
    /// be read back with `fixture_task_asks`.
    pub fn fixture_record_tasks(&mut self) {
        self.fixture().factory.asks = Some(Vec::new());
    }

    /// `fixture_record_tasks`, with each Start answered as a Task made.
    pub fn fixture_start_tasks(&mut self) {
        self.fixture_record_tasks();
        self.fixture().factory.starts = true;
    }

    pub fn fixture_task_asks(&self) -> &[ferrite_core::protocol::TaskAction] {
        self.fixture_ref()
            .and_then(|fixture| fixture.factory.asks.as_deref())
            .unwrap_or_default()
    }

    /// A fixture world's Factory hears `command` as a Server's would
    /// (a GitHub read's answer, say), and the window is told the board.
    pub fn fixture_factory_command(&mut self, command: ferrite_core::factory::Command) {
        if let Some(board) = self.fixture().factory.hear(command) {
            self.receive_fixture(ServerMessage::Factory { revision: 0, board });
        }
    }

    /// A fixture world's Task alert, as a Server would say it.
    pub fn fixture_task_alert(&mut self, alert: ServerMessage) {
        self.receive_fixture(alert);
    }
}

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
    ) -> Result<(), ferrite_core::header::LoadError> {
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
        &self.machines[&machine]
            .link
            .in_process_ref()
            .expect("this window's Engine runs in its Server")
            .engine
    }

    pub(crate) fn engine_mut(&mut self) -> &mut Engine {
        self.in_process()
    }

    /// A first send on the first machine that blocks until its Thread
    /// opens: the tests' own. A window sends a Draft's first message with
    /// `bootstrap_draft`.
    pub fn bootstrap(
        &mut self,
        choice: ferrite_core::provider::ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
    ) -> io::Result<ThreadKey> {
        let first = self.first();
        let started = self.in_process().bootstrap(choice, workspace, prompt);
        self.receive_from(first);
        Ok(ThreadKey::new(first, started?))
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
                crate::cockpit::in_process::parity::activity(view.activity()),
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

/// Mirror parity (ADR 0025): after every message, each open Thread's
/// mirror reads exactly as the Engine's own Thread does. Every Cockpit
/// test checks it as it runs.
#[cfg(test)]
impl HeldMachine {
    pub(in crate::cockpit) fn assert_mirrors_match(&self) {
        let Some(InProcess { engine, .. }) = self.link.in_process_ref() else {
            return;
        };
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
                parity::seen!(mirror),
                parity::seen!(engine),
                "thread {thread}: the mirror drifted from the Engine"
            );
        }
    }
}
