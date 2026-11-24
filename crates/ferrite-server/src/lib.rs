//! The Server (ADR 0025): `ferrite server`, the headless process that owns
//! every Session, the Store and the pending Decisions, so the agents keep
//! running when no window is open. Windows attach over `link`.
//!
//! One thread owns the Engine and pumps it: each pass applies what the
//! clients said, in arrival order, drains the Sessions, and hands every
//! message the Engine published to the clients that want it. It waits on
//! the clients for at most a frame while any Thread is open.

#[cfg(test)]
#[path = "../../ferrite-core/src/test_temp.rs"]
mod test_temp;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ferrite_core::api::Api;
use ferrite_core::engine::Engine;
use ferrite_core::link::{self, ClientId, Connection, Heard, Ring};
use ferrite_core::protocol::{
    self, Capability, ClientMessage, ClosedBy, Command, Done, ErrorCode, Op, ServerInfo,
    ServerMessage, When, WireError, WireFile, WireHandle,
};
use ferrite_core::provider::Provider;
use ferrite_core::roster::DraftId;
use ferrite_core::server::clis::{Clis, Host};
use ferrite_core::server::github::{GitHub, Gh};
use ferrite_core::server::{settings_dir, store_dir, ServerId};
use ferrite_core::settings::{ServerDefaults, SessionDefaults, Settings};
use ferrite_core::spawner::{ProcessRss, Spawn, RSS_LIMIT};
use ferrite_core::store::Store;
use ferrite_core::ThreadId;

mod bridge;
mod browser;
mod calls;
mod checkouts;
mod clis;
mod coordinator;
mod delegates;
mod drive;
mod factory;
mod github;
pub mod peers;
mod pr_watch;
mod render;
pub mod remote;
mod server_id;
mod signals;
mod tree;
mod wake;

/// How long the pump waits for a client while any Thread is open: a frame.
const PUMP: Duration = Duration::from_millis(16);
/// Yield to Sessions and pending replies even while clients keep sending.
const INTAKE_BATCH: usize = 64;
/// How long it waits with none: only to notice a signal.
const IDLE: Duration = Duration::from_millis(250);
/// How often the memory watchdog samples the Sessions.
const SWEEP_INTERVAL: Duration = Duration::from_secs(2);
/// How long a going Server waits for its clients to hear it go.
const LAST_WORDS: Duration = Duration::from_millis(500);

pub struct Options {
    /// Where the Threads live (`FERRITE_STORE`, else `~/.ferrite/threads`).
    pub store: PathBuf,
    /// `~/.ferrite`: settings, and the Server's log under `logs/`.
    pub settings_dir: PathBuf,
    /// This build's version, as `welcome` names it.
    pub version: String,
    /// `--after <pid>`: the Server this one replaces, on Windows, where a
    /// restart cannot `exec`. It must be gone before the Store is claimed.
    pub after: Option<u32>,
}

/// `ferrited` (ADR 0026): the Server alone, for a machine a window reaches
/// over ssh — no window, so none of its libraries. `ferrited server` runs
/// it; `ferrited bridge` is what the window's ssh runs, and `ferrited
/// api-bridge` the CLI's (ADR 0027); on Linux, `ferrited service
/// install|uninstall` has systemd keep it running.
pub fn ferrited(args: &[String]) -> i32 {
    let executable = std::env::current_exe().unwrap_or_else(|_| "ferrited".into());
    match args.get(1).map(String::as_str) {
        Some("--version" | "-V") => {
            println!("ferrited {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Some("server") => run(Options {
            store: store_dir(),
            settings_dir: settings_dir(),
            version: env!("CARGO_PKG_VERSION").into(),
            after: args
                .iter()
                .position(|arg| arg == "--after")
                .and_then(|at| args.get(at + 1))
                .and_then(|pid| pid.parse().ok()),
        }),
        Some("remote") => remote::cli(&args[2..], &store_dir(), &executable),
        Some("--remote-terminal") => ferrite_core::machine::remote::terminal_dispatch(args).unwrap_or(2),
        Some("api-bridge") => bridge::api(&store_dir()),
        Some("bridge") => bridge::run(&store_dir(), &executable),
        #[cfg(target_os = "linux")]
        Some("service") => ferrite_core::server::agent::service(&executable, args.get(2).map(String::as_str)),
        _ => {
            eprintln!(
                "usage: ferrited --version | ferrited server | ferrited bridge | ferrited api-bridge{USAGE_SERVICE}"
            );
            2
        }
    }
}

/// `ferrited service`, where there is one.
#[cfg(target_os = "linux")]
const USAGE_SERVICE: &str = " | ferrited service install|uninstall";
#[cfg(not(target_os = "linux"))]
const USAGE_SERVICE: &str = "";

/// Run the Server until it is stopped. The process's exit code: 0 for a
/// clean stop, and for a second Server that found the Store taken — a
/// failure would have launchd (or systemd) start it again and again.
pub fn run(options: Options) -> i32 {
    ferrite_core::diagnostics::open_server(&options.settings_dir);
    ferrite_core::login_shell::adopt_login_environment();
    if let Some(pid) = options.after {
        wait_for_exit(pid);
    }
    let store = match Store::open(&options.store) {
        Ok(store) => store,
        Err(error) => {
            ferrite_core::log!("server: cannot open the Thread store: {error}");
            return 1;
        }
    };
    if store.read_only().is_some() {
        ferrite_core::log!("server: not starting: another Ferrite server holds this store");
        return 0;
    }
    let id = match server_id::open(&options.store) {
        Ok(id) => id,
        Err(error) => {
            ferrite_core::log!("server: cannot keep the Server's id: {error}");
            return 1;
        }
    };
    let reopen = launch_threads(&store);
    let listener = match link::listen(&link::address(&options.store)) {
        Ok(listener) => listener,
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
            ferrite_core::log!("server: not starting: {error}");
            return 0;
        }
        Err(error) => {
            ferrite_core::log!("server: cannot listen: {error}");
            return 1;
        }
    };
    // The API is the agents' (ADR 0027): without it they still run.
    let api_address = ferrite_core::api::address(&options.store);
    let api = match link::listen(&api_address) {
        Ok(listener) => Some(listener),
        Err(error) => {
            ferrite_core::log!("server: the API is unavailable: {error}");
            None
        }
    };
    let settings = Settings::load(&options.settings_dir).unwrap_or_else(|error| {
        ferrite_core::log!("server: settings.json could not be read: {error}");
        Settings::default()
    });
    let defaults = Arc::new(Mutex::new(SessionDefaults::from_settings(&settings)));
    let (renderer, renders) = render::remote();
    let (browser_client, browser_requests) = ferrite_core::browser::channel();
    let (heard, hear) = mpsc::channel();
    let spawner = Spawn {
        browser: Some(browser_client),
        api: api.is_some().then_some(api_address),
        api_tools: Some(ferrite_core::api::tools::Tools::new(heard.clone())),
        ..Spawn::new(defaults.clone(), Some(Arc::new(renderer)))
    };
    let mut engine = match Engine::try_new(store, Box::new(spawner)) {
        Ok(engine) => engine,
        Err(error) => {
            ferrite_core::log!("server: cannot open the workspace registry: {error}");
            return 1;
        }
    };
    engine.watch_memory(Box::new(ProcessRss::default()), RSS_LIMIT);
    if let Some(api) = api {
        if let Err(error) = ferrite_core::api::accept_all(api, heard.clone()) {
            ferrite_core::log!("server: cannot accept API connections: {error}");
        }
    }
    if let Err(error) = link::accept_all(listener, heard.clone(), link::LAGGED_BYTES) {
        ferrite_core::log!("server: cannot accept connections: {error}");
        return 1;
    }
    signals::watch();
    let info = ServerInfo {
        version: options.version,
        executable: std::env::current_exe().unwrap_or_default(),
    };
    let mut server = Server::new(engine, info, id, renders, defaults, options.settings_dir, heard);
    server.browser_requests = browser_requests;
    server.apply_settings(&settings);
    revive_launch(&mut server.engine, reopen);
    ferrite_core::log!(
        "server: {} listening, boot {}",
        server.info.version,
        server.engine.boot_id()
    );
    server.serve(hear)
}

/// A machine for the window's tests (ADR 0026): a Server on a thread of
/// this process, for the Store at `store`, whose Sessions `spawner` makes,
/// and the Dial that reaches it as ssh reaches a machine's. It serves until
/// the process ends.
#[cfg(feature = "test-support")]
pub fn fixture_machine(
    store: PathBuf,
    spawner: Box<dyn ferrite_core::engine::Spawner + Send>,
) -> io::Result<ferrite_core::machine::Dial> {
    fixture_browser_machine(store, |_| spawner)
}

/// The same machine/Server fixture, with its real Thread-bound browser channel
/// available to a Provider spawner. The Server owns the receiving end, exactly
/// as production startup does; no test-only browser request router is involved.
#[cfg(feature = "test-support")]
pub fn fixture_browser_machine(
    store: PathBuf,
    spawner_factory: impl FnOnce(ferrite_core::browser::Client) -> Box<dyn ferrite_core::engine::Spawner + Send>,
) -> io::Result<ferrite_core::machine::Dial> {
    let (browser, browser_requests) = ferrite_core::browser::channel();
    let spawner = spawner_factory(browser);
    /// The window's end: dropped, it hangs up, as ssh does.
    struct Upload(link::Stream);
    impl io::Write for Upload {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }
    impl Drop for Upload {
        fn drop(&mut self) {
            let _ = link::TryClone::shutdown(&self.0);
        }
    }
    let address = link::address(&store);
    let (heard, hear) = mpsc::channel();
    link::accept_all(link::listen(&address)?, heard.clone(), link::LAGGED_BYTES)?;
    std::thread::spawn(move || {
        let id = server_id::open(&store).expect("the Store keeps an id");
        let engine = Engine::try_new(Store::open(&store).expect("the Store opens"), spawner)
            .expect("the registry opens");
        let info = ServerInfo {
            version: env!("CARGO_PKG_VERSION").into(),
            executable: PathBuf::new(),
        };
        let (_, renders) = render::remote();
        let settings_dir = store.parent().unwrap_or(&store).to_path_buf();
        let mut server = Server::new(engine, info, id, renders, Arc::default(), settings_dir, heard);
        server.browser_requests = browser_requests;
        server.serve(hear)
    });
    /// The Server's words, each read held back by `FERRITE_FIXTURE_LINK_DELAY_MS`
    /// when a run sets it: a slow link, which shows a test that acts on
    /// the window before the window has what the Server said.
    struct Slow(link::Stream, Option<Duration>);
    impl io::Read for Slow {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if let Some(delay) = self.1 {
                std::thread::sleep(delay);
            }
            self.0.read(buf)
        }
    }
    let delay = std::env::var("FERRITE_FIXTURE_LINK_DELAY_MS")
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map(Duration::from_millis);
    Ok(Box::new(move || {
        let stream = link::connect(&address)?;
        Ok(ferrite_core::machine::Tunnel {
            from: Box::new(Slow(link::TryClone::try_clone(&stream)?, delay)),
            to: Box::new(Upload(stream)),
            ended: Box::new(|| None),
        })
    }))
}

/// The Threads whose Panes were open when the last Server stopped — after
/// a crash, a reboot or a restart — which resume without a window. A store
/// from before open-state persistence falls back once to its most recently
/// used Thread, creation order breaking ties; an intentionally all-parked
/// store opens none, and a window offers a Draft instead (#29).
fn launch_threads(store: &Store) -> Vec<ThreadId> {
    if store.tracks_open_state() {
        return store.open_threads().unwrap_or_default();
    }
    store
        .thread_ids()
        .unwrap_or_default()
        .into_iter()
        .max_by_key(|thread| (store.last_used(*thread), *thread))
        .into_iter()
        .collect()
}

/// Reopen each Thread with nothing running in it (ADR 0032). No window is
/// attached yet: each is let go to its digest as soon as it is read, so
/// only one Thread's history is held at a time.
fn revive_launch(engine: &mut Engine, threads: Vec<ThreadId>) {
    engine.set_watched(BTreeSet::new());
    for thread in threads {
        if let Err(error) = engine.revive_unwatched(thread) {
            ferrite_core::log!("thread {thread} could not be revived: {error:?}");
        }
        // Nobody to tell: a window attaching reads the snapshot.
        drop(engine.take_messages());
    }
}

struct Client {
    connection: Connection,
    /// Said hello in a generation this Server speaks.
    welcomed: bool,
    renders_visuals: bool,
    browser_capable: bool,
    browser_dispatch: bool,
    /// A `ferrite --headless` window: the `window.*` actions are its.
    driven: bool,
    watching: BTreeSet<ThreadId>,
}

/// How a Server starts its successor, given the executable.
type Relaunch = dyn FnMut(&std::path::Path) -> io::Result<()>;

/// A file search a Session is answering: who asked, in which command.
type Search = (
    ClientId,
    u64,
    Receiver<io::Result<Vec<ferrite_core::prompt_files::FileSuggestion>>>,
);

/// A browser capture's image read off the pump: who asked, in which
/// command, and the reply it makes.
type Read = (ClientId, u64, Receiver<Result<Option<Done>, WireError>>);

/// An API request answered off the pump: who asked, under which id, and
/// the reply with the Activity it read back, for the Engine to hold.
type ApiRead = (
    ferrite_core::api::CallerId,
    String,
    Receiver<(
        Result<serde_json::Value, ferrite_core::api::ApiError>,
        Option<ferrite_core::engine::ReadThread>,
    )>,
);

struct Server {
    engine: Engine,
    info: ServerInfo,
    id: ServerId,
    clients: BTreeMap<ClientId, Client>,
    /// Each watched open Thread's recent inputs, for a client that
    /// reconnects. A Thread no client has watched has none: nobody holds a
    /// `seq` to resume from, so a boot's replay never fills one.
    rings: HashMap<ThreadId, Ring>,
    /// The pending Decisions the clients were told of.
    decisions: BTreeSet<(ThreadId, WireHandle)>,
    /// What the Engine was last told the windows watch (`tell_watched`).
    told_watched: Option<BTreeSet<ThreadId>>,
    /// Who answered each of them that an answer was delivered for.
    answered: BTreeMap<(ThreadId, WireHandle), ClientId>,
    renders: render::Renders,
    drives: drive::Drives,
    browser: ferrite_core::browser::Service,
    browser_requests: ferrite_core::browser::Requests,
    browser_disk: browser::Disk,
    browser_write: Option<browser::Write>,
    browser_captures: Vec<browser::CaptureWrite>,
    browser_changes: std::collections::VecDeque<(ClientId, Option<u64>, Op)>,
    browser_pending: BTreeMap<u64, browser::Pending>,
    browser_next: u64,
    browser_uncertain: BTreeMap<ThreadId, ferrite_core::browser::Lease>,
    browser_observed:
        BTreeMap<ThreadId, (ferrite_core::browser::Document, u64, ferrite_core::browser::Observation)>,
    searches: Vec<Search>,
    reads: Vec<Read>,
    /// The pump's own channel: what is answered off the pump is heard on
    /// it, and replied to at once.
    heard: mpsc::Sender<Heard>,
    /// No connection can reach this Server any more: it ends, so a window
    /// starts one that listens.
    deaf: bool,
    attachments: HashMap<ClientId, Arc<Mutex<ferrite_core::attachment_transfer::Session>>>,
    trees: HashMap<ClientId, tree::Lane>,
    api_reads: Vec<ApiRead>,
    /// First sends still starting, by the tag the Engine knows them by:
    /// who asked, in which command.
    starting: HashMap<DraftId, (ClientId, u64)>,
    next_start: u64,
    swept: Instant,
    defaults: Arc<Mutex<SessionDefaults>>,
    settings_dir: PathBuf,
    /// `stop` was asked for: at once, or once every agent is idle.
    stopping: Option<When>,
    /// `restart` was asked for, likewise.
    restarting: Option<When>,
    /// The automation API's connections, waits and subscriptions.
    api: Api,
    /// The operator's Provider, for a `thread.start` that names none.
    default_provider: Provider,
    /// How this Server starts its successor (`relaunch`; a test's own).
    relaunch: Box<Relaunch>,
    /// The self-hosted forges a review's pull request is read from.
    forge: ferrite_reviewr::config::PluginConfig,
    /// Every Project's Factory (ADR 0037), stepped once a pass.
    factory: ferrite_core::factory::Factory,
    /// The board revision the windows were last told.
    told_factory: Option<u64>,
    /// Task alerts held until the board that shows them goes, so a window
    /// never hears one before its Task is in that state.
    task_alerts: Vec<ServerMessage>,
    /// The Archive's revision the windows were last sent.
    told_archive: Option<u64>,
    /// What a one-off Task's Worker runs on: the operator's defaults.
    factory_agent: ferrite_core::factory_board::Agent,
    /// The settings each Coordinator's Session was last tuned to (#224),
    /// so one it refused is not tried again every pass.
    coordinators_tuned: std::collections::BTreeMap<ferrite_core::ThreadId, ferrite_core::factory_board::Agent>,
    /// What the Factory's GitHub starts are read through (#225): the
    /// operator's `gh`, or a test's scripted reader.
    github: Arc<dyn GitHub>,
    /// The provider CLIs this machine has, as a window asks after them
    /// (`Op::Cli`): the machine's own, or a test's scripted ones.
    clis: Arc<dyn Clis>,
    /// GitHub reads still out.
    github_reads: Vec<github::Reading>,
    /// Task PR head reads still out (#223).
    head_reads: Vec<github::HeadReading>,
    /// Each Project root's skills per Provider, as the Server last listed
    /// them and when (`None`: it could not), for a Workflow's check (G7).
    skills_seen: Vec<factory::SkillsSeen>,
    /// The Server's wall clock, in milliseconds (the Factory's, the PR
    /// watcher's, and the welcome's `now_ms`): a test passes its own.
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    /// The PR watcher (ADR 0046).
    watcher: pr_watch::Watcher,
    /// Each open Thread's branch, for the windows on other computers.
    checkouts: checkouts::Checkouts,
    /// Server wakes waiting for their Thread.
    wakes: wake::Wakes,
    /// The Delegates this Server follows (ADR 0047), through its client
    /// of the saved machines: the machines' own, or a test's.
    delegation: delegates::Delegation,
}

impl Server {
    fn new(
        engine: Engine,
        info: ServerInfo,
        id: ServerId,
        renders: render::Renders,
        defaults: Arc<Mutex<SessionDefaults>>,
        settings_dir: PathBuf,
        heard: mpsc::Sender<Heard>,
    ) -> Self {
        let browser = browser::load(engine.browser_store());
        let browser_disk = browser::Disk::new(engine.browser_store().clone());
        let factory = factory::load(engine.browser_store().dir());
        Self {
            engine,
            info,
            id,
            clients: BTreeMap::new(),
            rings: HashMap::new(),
            decisions: BTreeSet::new(),
            told_watched: None,
            answered: BTreeMap::new(),
            renders,
            drives: Default::default(),
            browser,
            browser_disk,
            browser_write: None,
            browser_captures: Vec::new(),
            browser_changes: Default::default(),
            browser_requests: Default::default(),
            browser_pending: BTreeMap::new(),
            browser_next: 0,
            browser_uncertain: BTreeMap::new(),
            browser_observed: BTreeMap::new(),
            searches: Vec::new(),
            reads: Vec::new(),
            heard,
            deaf: false,
            attachments: HashMap::new(),
            trees: HashMap::new(),
            api_reads: Vec::new(),
            starting: HashMap::new(),
            next_start: 0,
            swept: Instant::now(),
            defaults,
            stopping: None,
            restarting: None,
            api: Api::default(),
            default_provider: Provider::Claude,
            relaunch: Box::new(relaunch),
            forge: Default::default(),
            factory,
            told_factory: None,
            task_alerts: Vec::new(),
            told_archive: None,
            factory_agent: factory::default_agent(&Settings::default()),
            coordinators_tuned: Default::default(),
            github: Arc::new(Gh),
            clis: Arc::new(Host),
            github_reads: Vec::new(),
            head_reads: Vec::new(),
            skills_seen: Vec::new(),
            clock: Arc::new(|| ferrite_core::clock::now().timestamp_millis()),
            watcher: Default::default(),
            checkouts: Default::default(),
            wakes: Default::default(),
            delegation: delegates::Delegation::new(peers::Peers::new(settings_dir.clone())),
            settings_dir,
        }
    }

    fn serve(mut self, hear: Receiver<Heard>) -> i32 {
        ferrite_core::spawn::serve_the_operator();
        loop {
            let wait = if self.engine.threads().is_empty() {
                IDLE
            } else {
                PUMP
            };
            match hear.recv_timeout(wait) {
                Ok(heard) => self.hear(heard),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => self.deaf = true,
            }
            for _ in 1..INTAKE_BATCH {
                let Ok(heard) = hear.try_recv() else {
                    break;
                };
                self.hear(heard);
            }
            if self.deaf {
                ferrite_core::log!("server: the listener stopped");
                self.stop();
                return 1;
            }
            self.tell_watched();
            for (thread, branch) in self.checkouts.answered() {
                self.engine.set_branch(thread, branch);
            }
            self.engine.pump();
            self.step_factory();
            self.serve_pr_watches();
            self.serve_delegates();
            self.serve_wakes();
            if self.swept.elapsed() >= SWEEP_INTERVAL {
                self.swept = Instant::now();
                self.engine.sweep();
                if self.clients.values().any(|client| client.welcomed) {
                    self.checkouts.look(self.engine.checkouts());
                }
                for uploads in self.attachments.values() {
                    if let Ok(mut uploads) = uploads.try_lock() { uploads.expire(); }
                }
            }
            self.answer_bootstraps();
            self.answer_searches();
            self.answer_reads();
            self.answer_api_reads();
            self.answer_browser_disk();
            self.forward_renders();
            self.forward_browser_requests();
            self.broadcast();
            self.api.pump(&self.engine);
            if let Some(why) = signals::stop_requested() {
                ferrite_core::log!("server: stopping: {why}");
                self.stop();
                return 0;
            }
            if let Some(when) = self.stopping {
                if when == When::Now || self.engine.idle() {
                    ferrite_core::log!("server: stopping, as a window asked");
                    self.stop();
                    return 0;
                }
            }
            if let Some(when) = self.restarting {
                if when == When::Now || self.engine.idle() {
                    if let Some(code) = self.restart(when) {
                        return code;
                    }
                }
            }
        }
    }

    fn hear(&mut self, heard: Heard) {
        match heard {
            Heard::Joined(id, connection) => {
                self.clients.insert(
                    id,
                    Client {
                        connection,
                        welcomed: false,
                        renders_visuals: false,
                        browser_capable: false,
                        browser_dispatch: false,
                        driven: false,
                        watching: BTreeSet::new(),
                    },
                );
            }
            Heard::Gone(id) => self.gone(id),
            Heard::Message(id, message) => self.message(id, message),
            Heard::Api(heard) => self.api_heard(heard),
            Heard::Answered(id, request, answer) => self.reply(id, request, answer),
            Heard::Stopped => self.deaf = true,
        }
    }

    fn message(&mut self, id: ClientId, message: ClientMessage) {
        let Some(client) = self.clients.get_mut(&id) else {
            return;
        };
        if !client.welcomed {
            if let ClientMessage::Hello {
                generation,
                capabilities,
                ..
            } = message
            {
                let uptime = protocol::ms_since(ferrite_core::clock::instant(), self.engine.boot());
                let welcome = protocol::welcome(
                    generation,
                    self.info.clone(),
                    self.engine.boot_id().into(),
                    uptime,
                    self.id.clone(),
                    (self.clock)(),
                );
                let accepted = !matches!(welcome, ServerMessage::Welcome { error: Some(_), .. });
                let _ = client.connection.send(welcome);
                if accepted {
                    // What is already published goes to the others: this
                    // client's stream starts with its snapshot.
                    self.broadcast();
                    let Some(client) = self.clients.get_mut(&id) else {
                        return;
                    };
                    client.welcomed = true;
                    client.renders_visuals = capabilities.contains(&Capability::RenderVisual);
                    client.browser_capable =
                        capabilities.contains(&Capability::ConversationBrowser);
                    client.browser_dispatch = capabilities.contains(&Capability::BrowserDispatch);
                    client.driven = capabilities.contains(&Capability::Driven);
                    let mut snapshot = self.engine.snapshot();
                    // It watches nothing yet.
                    snapshot.threads = snapshot
                        .threads
                        .into_iter()
                        .map(protocol::ThreadState::unwatched)
                        .collect();
                    snapshot.factory = Some(self.factory.board());
                    self.send(id, ServerMessage::Snapshot(snapshot));
                }
            }
            return;
        }
        match message {
            ClientMessage::Watch { thread, resume } => self.watch(id, thread, resume),
            ClientMessage::Unwatch { thread } => {
                client.watching.remove(&thread);
            }
            ClientMessage::Command(command) => self.command(id, command),
            ClientMessage::BrowserReply {
                request,
                boot_id,
                thread,
                owner,
                document,
                revision,
                outcome,
            } => {
                self.answer_browser(id, request, boot_id.as_deref(), thread, owner, document, revision, outcome);
            }
            ClientMessage::BrowserPage {
                thread,
                owner,
                page,
            } => {
                // Kept for older window peers; new windows synchronously ACK
                // the document generation through Op::BrowserPage.
                if self.browser_write.is_some() {
                    self.browser_changes.push_back((
                        id,
                        None,
                        Op::BrowserPage {
                            thread,
                            owner,
                            page,
                        },
                    ));
                } else {
                    let _ = self.browser_page(id, thread, owner, page);
                }
            }
            ClientMessage::Rendered {
                request,
                png,
                error,
            } => self.renders.answer(request, png, error),
            ClientMessage::Driven { request, ok, error } => self.driven(id, request, ok, error),
            ClientMessage::Hello { .. } | ClientMessage::Unknown => {}
        }
    }

    /// Stream a Thread to a client: from the ring when it resumes this boot
    /// within it, otherwise its history first, read from the log on a
    /// thread of its own and sent in pieces as they come.
    fn watch(&mut self, id: ClientId, thread: ThreadId, resume: Option<protocol::Resume>) {
        self.broadcast();
        let missed = resume
            .filter(|resume| resume.boot_id == self.engine.boot_id())
            .and_then(|resume| self.rings.get(&thread)?.since(resume.after));
        match missed {
            Some(missed) => {
                for message in missed {
                    self.send(id, message);
                }
            }
            None => {
                let Some(history) = self.engine.history(thread) else {
                    return;
                };
                self.rings
                    .entry(thread)
                    .or_insert_with(|| Ring::after(history.seq()));
                let Some(client) = self.clients.get(&id) else {
                    return;
                };
                if client
                    .connection
                    .history(thread, Box::new(move |send| history.send(send)))
                    .is_err()
                {
                    self.lagged(id);
                    return;
                }
            }
        }
        if let Some(client) = self.clients.get_mut(&id) {
            client.watching.insert(thread);
        }
        if let Some(whole) = self.engine.published_thread(thread) {
            self.send(id, whole);
        }
        self.publish_browser(thread, None);
    }

    fn command(&mut self, id: ClientId, Command { id: request, op }: Command) {
        if self.browser_write.is_some()
            && matches!(
                &op,
                Op::Browser { .. }
                    | Op::BrowserPage { .. }
                    | Op::BrowserProtect { .. }
                    | Op::BrowserReclaim { .. }
                    | Op::BrowserReclaimed { .. }
            )
        {
            self.browser_changes.push_back((id, Some(request), op));
            return;
        }
        let executed = match &op {
            Op::BrowserDispatch { request, boot_id, thread, owner, document, revision } => {
                self.dispatch_browser(id, *request, boot_id, *thread, *owner, *document, *revision)
            }
            Op::BrowserCapture {
                thread,
                owner,
                lease,
                capture,
                png,
            } => self.browser_capture(id, request, *thread, *owner, *lease, capture.clone(), png.as_ref()),
            Op::BrowserCaptureImage { thread, hash } => {
                self.browser_capture_image(id, request, *thread, hash)
            }
            Op::BrowserProtect {
                thread,
                owner,
                document,
                protection,
            } => self.browser_owned(id, *thread, *owner).and_then(|_| {
                let lease = self
                    .browser
                    .protect(*thread, *owner, *document, *protection)
                    .map_err(failed)?;
                self.publish_browser(*thread, None);
                Ok(Some(Done::BrowserProtection { lease }))
            }),
            Op::BrowserRelease {
                thread,
                owner,
                lease,
            } => self.browser_owned(id, *thread, *owner).and_then(|_| {
                if let Some(capture) = self.browser_captures.iter_mut().find(|capture| {
                    capture.thread == *thread && capture.owner == *owner && capture.lease == *lease
                }) {
                    capture.release = true;
                } else {
                    self.browser
                        .release(*thread, *owner, *lease)
                        .map_err(failed)?;
                }
                self.publish_browser(*thread, None);
                Ok(None)
            }),
            Op::BrowserReclaim {
                thread,
                owner,
                document,
            } => self.browser_owned(id, *thread, *owner).and_then(|_| {
                let lease = self
                    .browser
                    .begin_reclaim(*thread, *owner, *document)
                    .map_err(failed)?;
                Ok(Some(Done::BrowserReclaim { lease }))
            }),
            Op::BrowserReclaimed {
                thread,
                owner,
                lease,
                released,
            } => self.browser_owned(id, *thread, *owner).and_then(|_| {
                let state = self
                    .browser
                    .finish_reclaim(*thread, *owner, *lease, *released)
                    .map_err(failed)?;
                self.publish_browser(*thread, None);
                Ok(Some(Done::Browser { state }))
            }),
            Op::BrowserPage {
                thread,
                owner,
                page,
            } => self.browser_page(id, *thread, *owner, page.clone()),
            Op::BrowserSettled {
                thread,
                owner,
                document,
                revision,
            } => self.browser_owned(id, *thread, *owner).and_then(|_| {
                if self
                    .browser
                    .operator_settled(*thread, *owner, *document, *revision)
                    .map_err(failed)?
                {
                    self.browser_observed.remove(thread);
                    self.publish_browser(*thread, None);
                }
                Ok(None)
            }),
            Op::BrowserInput {
                thread,
                owner,
                document,
            } => self.browser_owned(id, *thread, *owner).and_then(|_| {
                let revision = self
                    .browser
                    .operator_input(*thread, *owner, *document)
                    .map_err(failed)?;
                self.browser_observed.remove(thread);
                self.publish_browser(*thread, None);
                Ok(Some(Done::BrowserInput { revision }))
            }),
            Op::BrowserAttach { thread } => self.attach_browser(id, *thread),
            Op::Browser { thread, action } => self.browser_action(id, *thread, action.clone()),
            Op::ServerDefaults { set } => self.server_defaults(set.as_ref()).map(Some),
            Op::Cli { action } => {
                self.cli(id, request, action);
                return;
            }
            Op::SettingsChanged => match Settings::load(&self.settings_dir) {
                Ok(settings) => {
                    self.apply_settings(&settings);
                    Ok(None)
                }
                Err(error) => Err(failed(error)),
            },
            Op::SearchFiles { thread, query } => match self.engine.search_files(*thread, query) {
                Ok(answer) => {
                    self.searches.push((id, request, answer));
                    return;
                }
                Err(error) => Err(failed(error)),
            },
            Op::ListRepo { project } => match self.engine.registry().project(*project) {
                Some(project) => {
                    let root = project.root.clone();
                    self.read_off_pump(id, request, move || {
                        Ok(Some(ferrite_core::workspace::repo_listing(&root).into()))
                    });
                    return;
                }
                None => Err(failed("no such Project")),
            },
            Op::Repositories => {
                let read = self.engine.repositories();
                self.read_off_pump(id, request, move || Ok(Some(read().into())));
                return;
            }
            Op::Attachment { request: operation } => {
                if !self.attachments.contains_key(&id) && self.attachments.len() >= 16 {
                    self.reply(id, request, Err(failed("Too many attachment transfers are active. Retry shortly.")));
                    return;
                }
                let store = self.engine.browser_store().dir().to_path_buf();
                let uploads = self.attachments.entry(id).or_insert_with(|| {
                    Arc::new(Mutex::new(ferrite_core::attachment_transfer::Session::new(store)))
                }).clone();
                let operation = operation.clone();
                self.read_off_pump(id, request, move || {
                    uploads.lock().map_err(|_| failed("Attachment worker stopped"))?
                        .execute(operation)
                        .map(|reply| Some(Done::Attachment { reply })).map_err(failed)
                });
                return;
            }
            Op::Tree { request: operation } => {
                if !self.trees.contains_key(&id) && self.trees.len() >= tree::MOST {
                    self.reply(id, request, Err(failed("Too many tree transfers are active. Retry shortly.")));
                    return;
                }
                let heard = self.heard.clone();
                self.trees.entry(id).or_insert_with(|| tree::Lane::open(id, heard)).ask(request, operation.clone());
                return;
            }
            Op::Changes { request: operation } => {
                let operation = operation.clone();
                self.read_off_pump(id, request, move || {
                    ferrite_core::changes::execute(operation)
                        .map(|reply| Some(Done::Changes { reply })).map_err(failed)
                });
                return;
            }
            Op::FileOperation { request: operation } => {
                let operation = operation.clone();
                self.read_off_pump(id, request, move || {
                    ferrite_core::file_operations::execute(operation)
                        .map(|reply| Some(Done::FileOperation { reply }))
                        .map_err(failed)
                });
                return;
            }
            Op::ListDir { path } => {
                let path = path.clone();
                self.read_off_pump(id, request, move || {
                    ferrite_core::workspace::list_dir(&path)
                        .map(|listing| Some(listing.into()))
                        .map_err(failed)
                });
                return;
            }
            Op::ReviewDiff {
                path,
                scope,
                commit,
            } => match scope.read(commit.clone(), None) {
                Some(read) => {
                    let path = path.clone();
                    self.read_off_pump(id, request, move || {
                        ferrite_reviewr::ferrite::review(&path, &read)
                            .map(|files| {
                                Some(Done::Diff {
                                    text: ferrite_core::protocol::unified(&files),
                                })
                            })
                            .map_err(failed)
                    });
                    return;
                }
                None => Err(failed("this server cannot read that scope")),
            },
            Op::ReviewRead {
                path,
                scope,
                commit,
                since,
            } => match scope.read(commit.clone(), since.clone()) {
                Some(read) => {
                    let path = path.clone();
                    self.read_off_pump(id, request, move || {
                        ferrite_reviewr::ferrite::review(&path, &read)
                            .map(|files| {
                                Some(Done::Review {
                                    files: files.into_iter().map(Into::into).collect(),
                                })
                            })
                            .map_err(failed)
                    });
                    return;
                }
                None => Err(failed("this server cannot read that scope")),
            },
            Op::ListCommits { path } => {
                let path = path.clone();
                self.read_off_pump(id, request, move || {
                    ferrite_reviewr::ferrite::commits(&path)
                        .map(|commits| {
                            Some(Done::Commits {
                                commits: commits.into_iter().map(Into::into).collect(),
                            })
                        })
                        .map_err(failed)
                });
                return;
            }
            Op::SearchRepo { path, query } => {
                let (path, query) = (path.clone(), query.clone());
                self.read_off_pump(id, request, move || {
                    ferrite_reviewr::ferrite::search(&path, &query)
                        .map(|results| Some(results.into()))
                        .map_err(failed)
                });
                return;
            }
            Op::ReadFile { path, file } => {
                let (path, file) = (path.clone(), file.clone());
                self.read_off_pump(id, request, move || {
                    ferrite_reviewr::ferrite::read_file(&path, &file)
                        .map(|text| Some(Done::Text { text }))
                        .map_err(|notice| failed(notice.message()))
                });
                return;
            }
            Op::ReviewPullRequestDiff { path, number } => {
                let (path, number, forge) = (path.clone(), *number, self.forge.clone());
                self.read_off_pump(id, request, move || {
                    ferrite_reviewr::forge::diff_numbered(&path, &forge, number)
                        .map(|files| Some(Done::Review {
                            files: files.into_iter().map(Into::into).collect(),
                        }))
                        .map_err(failed)
                });
                return;
            }
            Op::ReviewPullRequest { path, number } => {
                let (path, number, forge) = (path.clone(), *number, self.forge.clone());
                self.read_off_pump(id, request, move || {
                    let (view, noun) = match number {
                        Some(number) => {
                            ferrite_reviewr::ferrite::pull_request_numbered(&path, &forge, number)
                        }
                        None => ferrite_reviewr::ferrite::pull_request(&path, &forge),
                    };
                    Ok(Some(
                        ferrite_core::protocol::ReviewPullRequest::of(view, noun).into(),
                    ))
                });
                return;
            }
            Op::ListPullRequests { path, filter } => {
                let (path, filter, forge) = (path.clone(), *filter, self.forge.clone());
                self.read_off_pump(id, request, move || {
                    let list = ferrite_reviewr::forge::list(&path, &forge, filter.into());
                    Ok(Some(
                        ferrite_core::protocol::ReviewPullRequests::of(list, "PR").into(),
                    ))
                });
                return;
            }
            Op::ListFiles { path } => {
                let path = path.clone();
                self.read_off_pump(id, request, move || {
                    ferrite_reviewr::ferrite::all_files(&path)
                        .map(|entries| {
                            Some(Done::AllFiles {
                                entries: entries.into_iter().map(Into::into).collect(),
                            })
                        })
                        .map_err(failed)
                });
                return;
            }
            Op::ListBases { path } => {
                let path = path.clone();
                self.read_off_pump(id, request, move || {
                    ferrite_reviewr::ferrite::bases(&path)
                        .map(|bases| Some(bases.into()))
                        .map_err(failed)
                });
                return;
            }
            Op::PickBase { path, base } => {
                let (path, base) = (path.clone(), base.clone());
                self.read_off_pump(id, request, move || {
                    ferrite_reviewr::ferrite::pick_base(&path, &base)
                        .map(|()| Some(Done::Changed { changed: true }))
                        .map_err(failed)
                });
                return;
            }
            Op::PullRequest { path } => {
                let path = path.clone();
                self.read_off_pump(id, request, move || {
                    Ok(Some(Done::PullRequest {
                        json: ferrite_core::workspace::pr_view(&path),
                    }))
                });
                return;
            }
            Op::WatchPr {
                thread,
                url,
                watching: true,
            } => {
                self.window_watch_pr(id, request, *thread, url);
                return;
            }
            // The operator's cancel is the agent's `delegate.cancel`.
            Op::CancelDelegate { thread, delegate } => self
                .api_delegate_cancel(
                    None,
                    &ferrite_core::api::schema::DelegateCancelParams {
                        delegate: ferrite_core::api::schema::DelegateParams {
                            thread: Some(*thread),
                            delegate: *delegate,
                        },
                        reason: None,
                    },
                )
                .map(|_| Some(Done::Changed { changed: true }))
                .map_err(|error| failed(error.message)),
            Op::Focused { .. } | Op::Composing { .. } => {
                self.engine.window_says(id, &op);
                Ok(None)
            }
            Op::Bootstrap { .. } => {
                self.next_start += 1;
                let tag = DraftId::new(self.next_start);
                match self.engine.bootstrap_command(tag, &op) {
                    Ok(Some(thread)) => Ok(Some(Done::Thread { thread })),
                    Ok(None) => {
                        self.starting.insert(tag, (id, request));
                        self.broadcast();
                        return;
                    }
                    Err(error) => Err(error),
                }
            }
            Op::CancelBootstrap { request: started } => {
                let tag = self
                    .starting
                    .iter()
                    .find(|(_, asked)| **asked == (id, *started))
                    .map(|(tag, _)| *tag);
                match tag {
                    Some(tag) => {
                        self.starting.remove(&tag);
                        let cancelled = self.engine.cancel_draft_start(tag);
                        self.reply(id, *started, Err(failed("the first send was cancelled")));
                        cancelled
                            .map(|changed| Some(Done::Changed { changed }))
                            .map_err(failed)
                    }
                    None => Ok(Some(Done::Changed { changed: false })),
                }
            }
            Op::Stop { when } => {
                self.stopping = Some(*when);
                Ok(None)
            }
            Op::Task { action } => self.task_command(action),
            Op::Workflow { action } => self.workflow_command(action),
            Op::Coordinator { action } => self.coordinator_command(action),
            Op::GitHub { ask } => {
                let github = Arc::clone(&self.github);
                let ask = ask.clone();
                self.read_off_pump(id, request, move || github::list(&*github, &ask));
                return;
            }
            Op::Restart { when } => match self.successor() {
                Ok(_) => {
                    self.restarting = Some(*when);
                    Ok(None)
                }
                Err(error) => Err(failed(error)),
            },
            op => {
                let executed = self.engine.execute(op);
                match (op, &executed) {
                    (Op::Answer { thread, handle, .. }, Ok(_)) => {
                        self.answered.insert((*thread, handle.clone()), id);
                    }
                    (Op::Send { thread, text } | Op::Queue { thread, text }, Ok(_)) => {
                        self.heard_operator(*thread, text);
                    }
                    _ => {}
                }
                executed
            }
        };
        if executed.is_ok() {
            if matches!(&op, Op::Browser { .. } | Op::BrowserPage { .. }) {
                if let Some(write) = &mut self.browser_write {
                    write.reply = browser::Reply::Window(id, request);
                    return;
                }
            }
            if matches!(
                &op,
                Op::BrowserCapture { .. } | Op::BrowserCaptureImage { .. }
            ) {
                return;
            }
        }
        self.reply(id, request, executed);
    }

    fn reply(&mut self, id: ClientId, request: u64, executed: Result<Option<Done>, WireError>) {
        let (ok, error) = match executed {
            Ok(done) => (done, None),
            Err(error) => (None, Some(error)),
        };
        // What the command changed goes out before its reply.
        self.broadcast();
        self.send(
            id,
            ServerMessage::Reply {
                id: request,
                ok,
                error,
            },
        );
    }

    /// This machine's defaults as its `settings.json` says: with `set`,
    /// written first, so the answer is what a later read finds.
    fn server_defaults(
        &mut self,
        set: Option<&ServerDefaults>,
    ) -> Result<Done, WireError> {
        let mut settings = Settings::load(&self.settings_dir).map_err(failed)?;
        if let Some(defaults) = set {
            settings.set_server_defaults(defaults.clone());
            settings.save(&self.settings_dir).map_err(failed)?;
            settings = Settings::load(&self.settings_dir).map_err(failed)?;
            self.apply_settings(&settings);
        }
        Ok(Done::ServerDefaults {
            defaults: settings.server_defaults(),
        })
    }

    /// Settings apply to the next Session, as they always have.
    fn apply_settings(&mut self, settings: &Settings) {
        *self.defaults.lock().unwrap_or_else(|e| e.into_inner()) =
            SessionDefaults::from_settings(settings);
        self.engine.apply_settings(settings);
        self.default_provider = settings.default_provider;
        self.forge = settings.forge_hosts.review_config();
        self.factory_agent = factory::default_agent(settings);
        self.factory.set_limit(settings.factory_tasks_at_once);
    }

    /// A first send that settled answers the command that started it.
    fn answer_bootstraps(&mut self) {
        for settled in self.engine.take_bootstrap_results() {
            let asked = settled.draft.and_then(|tag| self.starting.remove(&tag));
            match (asked, settled.result) {
                (Some((client, request)), result) => {
                    let executed = result
                        .map(|started| {
                            Some(Done::Thread {
                                thread: started.thread,
                            })
                        })
                        .map_err(failed);
                    self.reply(client, request, executed);
                }
                (None, Err(error)) => {
                    ferrite_core::log!("server: a first send did not start: {error}");
                }
                (None, Ok(_)) => {}
            }
        }
    }

    fn answer_searches(&mut self) {
        let mut answered = Vec::new();
        self.searches
            .retain(|(client, request, answer)| match answer.try_recv() {
                Ok(found) => {
                    answered.push((*client, *request, found));
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => {
                    answered.push((
                        *client,
                        *request,
                        Err(io::Error::other("the Session ended before answering")),
                    ));
                    false
                }
            });
        for (client, request, found) in answered {
            let executed = found
                .map(|files| {
                    Some(Done::Files {
                        files: files
                            .into_iter()
                            .map(|file| WireFile {
                                path: file.path,
                                is_directory: file.is_directory,
                                matched: file.matched,
                            })
                            .collect(),
                    })
                })
                .map_err(failed);
            self.reply(client, request, executed);
        }
    }

    /// Read on a thread of its own, so a slow disk or a hung mount holds
    /// up only the window that asked, not every Thread on the pump.
    fn read_off_pump(
        &mut self,
        client: ClientId,
        request: u64,
        read: impl FnOnce() -> Result<Option<Done>, WireError> + Send + 'static,
    ) {
        let heard = self.heard.clone();
        std::thread::spawn(move || {
            ferrite_core::spawn::serve_the_operator();
            let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(read))
                .unwrap_or_else(|_| Err(failed("the read ended")));
            let _ = heard.send(Heard::Answered(client, request, answer));
        });
    }

    /// Reply to each browser capture's image read that finished.
    fn answer_reads(&mut self) {
        let mut answered = Vec::new();
        self.reads
            .retain(|(client, request, answer)| match answer.try_recv() {
                Ok(executed) => {
                    answered.push((*client, *request, executed));
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => {
                    answered.push((*client, *request, Err(failed("the read ended"))));
                    false
                }
            });
        for (client, request, executed) in answered {
            self.reply(client, request, executed);
        }
    }

    /// Visuals to draw go to the window that can, most recently attached;
    /// with none, the agent is told at once.
    fn forward_renders(&mut self) {
        let drawer = self
            .clients
            .iter()
            .rev()
            .find(|(_, client)| client.welcomed && client.renders_visuals)
            .map(|(id, _)| *id);
        for message in self.renders.take(drawer) {
            if let Some(drawer) = drawer {
                self.send(drawer, message);
            }
        }
    }

    /// Tell the Engine what the windows watch, when that changed: what
    /// nobody watches may be let go to its digest (ADR 0032).
    fn tell_watched(&mut self) {
        let watched: BTreeSet<ThreadId> = self
            .clients
            .values()
            .filter(|client| client.welcomed)
            .flat_map(|client| client.watching.iter().copied())
            .collect();
        if self.told_watched.as_ref() != Some(&watched) {
            self.engine.set_watched(watched.clone());
            self.told_watched = Some(watched);
        }
    }

    /// Hand every message the Engine published to the clients that want
    /// it: a Thread's stream to its watchers, the rest to everyone.
    fn broadcast(&mut self) {
        let mut messages = self.engine.take_messages();
        messages.extend(self.decision_changes());
        messages.extend(self.factory_changes());
        messages.append(&mut self.task_alerts);
        for message in messages {
            self.api.relay(&message);
            match &message {
                ServerMessage::Input { thread, seq, .. } => {
                    if let Some(ring) = self.rings.get_mut(thread) {
                        ring.push(*seq, message.clone());
                    }
                }
                // A new Activity: a resume cannot cross its start.
                ServerMessage::HistoryEnd { thread, seq, .. } => {
                    if let Some(ring) = self.rings.get_mut(thread) {
                        *ring = Ring::after(*seq);
                    }
                }
                ServerMessage::Roster { roster, .. } => {
                    self.rings.retain(|thread, _| roster.open.contains(thread));
                    for client in self.clients.values_mut() {
                        client
                            .watching
                            .retain(|thread| roster.open.contains(thread));
                    }
                }
                _ => {}
            }
            let scope = match &message {
                ServerMessage::Input { thread, .. }
                | ServerMessage::ToolDraft { thread, .. }
                | ServerMessage::History { thread, .. }
                | ServerMessage::HistoryEnd { thread, .. } => Some(*thread),
                _ => None,
            };
            let receivers: Vec<ClientId> = self
                .clients
                .iter()
                .filter(|(_, client)| {
                    client.welcomed && scope.is_none_or(|thread| client.watching.contains(&thread))
                })
                .map(|(id, _)| *id)
                .collect();
            for id in receivers {
                let watched = |thread| {
                    self.clients
                        .get(&id)
                        .is_some_and(|client| client.watching.contains(&thread))
                };
                let sent = match &message {
                    ServerMessage::Thread { revision, state } if !watched(state.thread) => {
                        ServerMessage::Thread {
                            revision: *revision,
                            state: state.clone().unwatched(),
                        }
                    }
                    message => message.clone(),
                };
                self.send(id, sent);
            }
        }
    }

    /// Each Decision asked since the last pass, and each that closed, and
    /// why: the answer one client delivered, the provider withdrawing it,
    /// or its Session ending.
    fn decision_changes(&mut self) -> Vec<ServerMessage> {
        let pending = self.engine.pending_decisions();
        let now: BTreeSet<_> = pending
            .iter()
            .map(|entry| (entry.thread, entry.handle.clone()))
            .collect();
        let mut changes = Vec::new();
        for (thread, handle) in self.decisions.difference(&now) {
            let session_lives = self.engine.thread(*thread).is_some_and(|open| {
                open.generation() == handle.generation && open.activity().connected()
            });
            let by = match self.answered.get(&(*thread, handle.clone())) {
                Some(&client) => ClosedBy::Answered { client },
                None if session_lives => ClosedBy::Cancelled,
                None => ClosedBy::SessionEnded,
            };
            changes.push(ServerMessage::DecisionClosed {
                thread: *thread,
                handle: handle.clone(),
                by,
            });
        }
        for entry in pending {
            if !self
                .decisions
                .contains(&(entry.thread, entry.handle.clone()))
            {
                changes.push(ServerMessage::Decision {
                    thread: entry.thread,
                    subject: entry.subject,
                    handle: entry.handle,
                    decision: entry.decision,
                });
            }
        }
        self.answered.retain(|asked, _| now.contains(asked));
        self.decisions = now;
        changes
    }

    /// Close every pending Decision for the clients: this Server is going.
    fn close_decisions(&mut self) {
        for (thread, handle) in std::mem::take(&mut self.decisions) {
            self.say(ServerMessage::DecisionClosed {
                thread,
                handle,
                by: ClosedBy::ServerRestart,
            });
        }
    }

    /// Queue a message for one client; one too far behind is let go, and
    /// reconnects.
    fn send(&mut self, id: ClientId, message: ServerMessage) {
        let Some(client) = self.clients.get(&id) else {
            return;
        };
        if client.connection.send(message).is_err() {
            self.lagged(id);
        }
    }

    fn lagged(&mut self, id: ClientId) {
        ferrite_core::log!("server: client {id} fell too far behind");
        self.gone(id);
    }

    /// A client's connection ended: what it held or was asked goes too.
    fn gone(&mut self, id: ClientId) {
        self.clients.remove(&id);
        self.attachments.remove(&id);
        self.trees.remove(&id);
        self.engine.window_gone(id);
        self.renders.client_gone(id);
        self.undriven(id);
        self.browser_owner_gone(id);
    }

    /// Flush every writer and end every Session without parking it: the
    /// open markers stay, so the next Server resumes the same Threads.
    fn stop(&mut self) {
        self.close_decisions();
        self.say(ServerMessage::Stopping);
        self.halt();
    }

    /// Tell every client that said hello.
    fn say(&mut self, message: ServerMessage) {
        let welcomed: Vec<ClientId> = self
            .clients
            .iter()
            .filter(|(_, client)| client.welcomed)
            .map(|(id, _)| *id)
            .collect();
        for id in welcomed {
            self.send(id, message.clone());
        }
    }

    /// End every Session, then let every client go once it has heard the
    /// last words, before the process takes them.
    fn halt(&mut self) {
        self.engine.halt_sessions();
        let deadline = Instant::now() + LAST_WORDS;
        for (_, client) in std::mem::take(&mut self.clients) {
            client.connection.finish(deadline);
        }
    }
}

impl Server {
    /// What a restart starts: this executable, as it is on disk now. An
    /// update may have replaced it — that is the point — but one that is
    /// gone would leave no Server at all.
    fn successor(&self) -> io::Result<PathBuf> {
        let executable = std::env::current_exe()?;
        if executable.exists() {
            Ok(executable)
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "this server's executable is gone",
            ))
        }
    }

    /// Restart into this executable: windows hear it, every Session ends
    /// as on SIGTERM (a turn cut off reads interrupted when it is revived),
    /// and the successor revives the open Threads. `None` when it cannot,
    /// and this Server keeps running.
    fn restart(&mut self, when: When) -> Option<i32> {
        self.restarting = None;
        let executable = match self.successor() {
            Ok(executable) => executable,
            Err(error) => {
                ferrite_core::log!("server: not restarting: {error}");
                return None;
            }
        };
        ferrite_core::log!("server: restarting ({when:?})");
        self.close_decisions();
        self.say(ServerMessage::Restarting { when });
        self.halt();
        match (self.relaunch)(&executable) {
            Ok(()) => Some(0),
            Err(error) => {
                // Sessions are already gone: a failure exit has launchd or
                // systemd (or the next window) start a Server again.
                ferrite_core::log!("server: could not restart: {error}");
                Some(1)
            }
        }
    }
}

/// Start the successor. On Unix the same process becomes it (`exec`: same
/// pid, so launchd and systemd see no exit; the Store's claim is
/// close-on-exec, so the new image takes it). On Windows, a detached
/// Server that waits for this one to exit before it claims the Store.
fn relaunch(executable: &std::path::Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(std::process::Command::new(executable).arg("server").exec())
    }
    #[cfg(not(unix))]
    {
        let pid = std::process::id().to_string();
        ferrite_core::server::agent::start_detached(executable, &["--after", &pid])
    }
}

/// Wait for the Server this one replaces to exit.
fn wait_for_exit(pid: u32) {
    #[cfg(unix)]
    // SAFETY: signal 0 only asks whether the process exists.
    while unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
        std::thread::sleep(Duration::from_millis(50));
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE,
        };
        // SAFETY: a handle opened only to wait on, and closed after.
        unsafe {
            let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if !process.is_null() {
                WaitForSingleObject(process, INFINITE);
                CloseHandle(process);
            }
        }
    }
}

fn failed(error: impl ToString) -> WireError {
    WireError {
        code: ErrorCode::Failed,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests;
