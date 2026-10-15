//! The Server (ADR 0025): `ferrite server`, the headless process that owns
//! every Session, the Store and the pending Decisions, so the agents keep
//! running when no window is open. Windows attach over `link`.
//!
//! One thread owns the Engine and pumps it: each pass applies what the
//! clients said, in arrival order, drains the Sessions, and hands every
//! message the Engine published to the clients that want it. It waits on
//! the clients for at most a frame while any Thread is open.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::api::Api;
use crate::engine::Engine;
use crate::link::{self, ClientId, Connection, Heard, Ring};
use crate::protocol::{
    self, Capability, ClientMessage, ClosedBy, Command, Done, ErrorCode, Op, ServerInfo,
    ServerMessage, When, WireError, WireFile, WireHandle,
};
use crate::roster::DraftId;
use crate::settings::Settings;
use crate::spawner::{ProcessRss, SessionDefaults, Spawn, RSS_LIMIT};
use crate::store::{Provider, Store};
use crate::ThreadId;

pub mod agent;
mod bridge;
mod calls;
mod coordinate;
mod render;
mod signals;

/// How long the pump waits for a client while any Thread is open: a frame.
const PUMP: Duration = Duration::from_millis(16);
/// How long it waits with none: only to notice a signal.
const IDLE: Duration = Duration::from_millis(250);
/// The open Threads are revived this far apart at boot (ADR 0018).
const LAUNCH_STAGGER: Duration = Duration::from_millis(100);
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

/// Where the Threads live: `FERRITE_STORE`, else `~/.ferrite/threads`.
pub fn store_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("FERRITE_STORE") {
        return dir.into();
    }
    // Windows spells the home directory USERPROFILE, not HOME.
    let base = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .unwrap_or_else(|| ".".into());
    PathBuf::from(base).join(".ferrite").join("threads")
}

/// `~/.ferrite`: the Store's parent, where the settings and logs live.
pub fn settings_dir() -> PathBuf {
    let store = store_dir();
    store
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or(store)
}

/// `ferrited` (ADR 0026): the Server alone, for a machine a window reaches
/// over ssh — no window, so none of its libraries. `ferrited server` runs
/// it; `ferrited bridge` is what the window's ssh runs, and `ferrited
/// api-bridge` the CLI's (ADR 0027); on Linux, `ferrited service
/// install|uninstall` has systemd keep it running.
pub fn ferrited(args: &[String]) -> i32 {
    let executable = std::env::current_exe().unwrap_or_else(|_| "ferrited".into());
    match args.get(1).map(String::as_str) {
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
        Some("api-bridge") => bridge::api(&store_dir()),
        Some("bridge") => bridge::run(&store_dir(), &executable),
        #[cfg(target_os = "linux")]
        Some("service") => agent::service(&executable, args.get(2).map(String::as_str)),
        _ => {
            eprintln!(
                "usage: ferrited server | ferrited bridge | ferrited api-bridge{USAGE_SERVICE}"
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
    crate::diagnostics::open_as(&options.settings_dir, "server.log");
    crate::login_shell::adopt_login_environment();
    if let Some(pid) = options.after {
        wait_for_exit(pid);
    }
    let store = match Store::open(&options.store) {
        Ok(store) => store,
        Err(error) => {
            crate::log!("server: cannot open the Thread store: {error}");
            return 1;
        }
    };
    if store.read_only().is_some() {
        crate::log!("server: not starting: another Ferrite server holds this store");
        return 0;
    }
    let reopen = launch_threads(&store);
    let listener = match link::listen(&link::address(&options.store)) {
        Ok(listener) => listener,
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
            crate::log!("server: not starting: {error}");
            return 0;
        }
        Err(error) => {
            crate::log!("server: cannot listen: {error}");
            return 1;
        }
    };
    // The API is the agents' (ADR 0027): without it they still run.
    let api_address = crate::api::address(&options.store);
    let api = match link::listen(&api_address) {
        Ok(listener) => Some(listener),
        Err(error) => {
            crate::log!("server: the API is unavailable: {error}");
            None
        }
    };
    let settings = Settings::load(&options.settings_dir).unwrap_or_else(|error| {
        crate::log!("server: settings.json could not be read: {error}");
        Settings::default()
    });
    let defaults = Arc::new(Mutex::new(SessionDefaults::from_settings(&settings)));
    let (renderer, renders) = render::remote();
    let spawner = Spawn {
        api: api.is_some().then_some(api_address),
        ..Spawn::new(defaults.clone(), Some(Arc::new(renderer)))
    };
    let mut engine = match Engine::try_new(store, Box::new(spawner)) {
        Ok(engine) => engine,
        Err(error) => {
            crate::log!("server: cannot open the workspace registry: {error}");
            return 1;
        }
    };
    engine.watch_memory(Box::new(ProcessRss::default()), RSS_LIMIT);
    let (heard, hear) = mpsc::channel();
    if let Some(api) = api {
        if let Err(error) = crate::api::accept_all(api, heard.clone()) {
            crate::log!("server: cannot accept API connections: {error}");
        }
    }
    if let Err(error) = link::accept_all(listener, heard) {
        crate::log!("server: cannot accept connections: {error}");
        return 1;
    }
    signals::watch();
    let info = ServerInfo {
        version: options.version,
        executable: std::env::current_exe().unwrap_or_default(),
    };
    let mut server = Server::new(engine, info, renders, defaults, options.settings_dir);
    server.apply_settings(&settings);
    revive_launch(&mut server.engine, reopen);
    crate::log!(
        "server: {} listening, boot {}",
        server.info.version,
        server.boot_id
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
    spawner: Box<dyn crate::engine::Spawner + Send>,
) -> io::Result<crate::machine::Dial> {
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
    link::accept_all(link::listen(&address)?, heard)?;
    std::thread::spawn(move || {
        let engine = Engine::try_new(Store::open(&store).expect("the Store opens"), spawner)
            .expect("the registry opens");
        let info = ServerInfo {
            version: env!("CARGO_PKG_VERSION").into(),
            executable: PathBuf::new(),
        };
        let (_, renders) = render::remote();
        let settings_dir = store.parent().unwrap_or(&store).to_path_buf();
        Server::new(engine, info, renders, Arc::default(), settings_dir).serve(hear)
    });
    Ok(Box::new(move || {
        let stream = link::connect(&address)?;
        Ok(crate::machine::Tunnel {
            from: Box::new(link::TryClone::try_clone(&stream)?),
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

/// Revive each Thread `LAUNCH_STAGGER` after the one before (ADR 0018):
/// the Threads coming back do not start every CLI in one instant.
fn revive_launch(engine: &mut Engine, threads: Vec<ThreadId>) {
    for (nth, thread) in threads.into_iter().enumerate() {
        if let Err(error) = engine.revive_after(thread, LAUNCH_STAGGER * nth as u32) {
            crate::log!("thread {thread} could not be revived: {error:?}");
        }
    }
}

/// A boot's identity: a resume names it, and a Server that restarted has
/// another, so no client resumes into a stream that is not there.
fn boot_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}-{:x}", std::process::id(), nanos)
}

struct Client {
    connection: Connection,
    /// Said hello in a generation this Server speaks.
    welcomed: bool,
    renders_visuals: bool,
    lists_parked: bool,
    watching: BTreeSet<ThreadId>,
}

/// How a Server starts its successor, given the executable.
type Relaunch = dyn FnMut(&std::path::Path) -> io::Result<()>;

/// A file search a Session is answering: who asked, in which command.
type Search = (
    ClientId,
    u64,
    Receiver<io::Result<Vec<crate::providers::FileSuggestion>>>,
);

/// A read off the pump — git's for a `list_repo`, `review_diff` or
/// `list_commits`, `gh`'s for a `pull_request`, a folder's for a `list_dir`: who asked, in which command, and the reply it makes.
type Read = (ClientId, u64, Receiver<Result<Option<Done>, WireError>>);

struct Server {
    engine: Engine,
    boot_id: String,
    info: ServerInfo,
    clients: BTreeMap<ClientId, Client>,
    /// Each open Thread's recent inputs, for a client that reconnects.
    rings: HashMap<ThreadId, Ring>,
    /// The pending Decisions the clients were told of.
    decisions: BTreeSet<(ThreadId, WireHandle)>,
    /// Who answered each of them that an answer was delivered for.
    answered: BTreeMap<(ThreadId, WireHandle), ClientId>,
    renders: render::Renders,
    searches: Vec<Search>,
    reads: Vec<Read>,
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
    /// The Projects' coordinators and their workers (ADR 0028).
    coordinating: coordinate::Coordinating,
}

impl Server {
    fn new(
        engine: Engine,
        info: ServerInfo,
        renders: render::Renders,
        defaults: Arc<Mutex<SessionDefaults>>,
        settings_dir: PathBuf,
    ) -> Self {
        Self {
            coordinating: coordinate::Coordinating::load(engine.store_dir()),
            engine,
            boot_id: boot_id(),
            info,
            clients: BTreeMap::new(),
            rings: HashMap::new(),
            decisions: BTreeSet::new(),
            answered: BTreeMap::new(),
            renders,
            searches: Vec::new(),
            reads: Vec::new(),
            starting: HashMap::new(),
            next_start: 0,
            swept: Instant::now(),
            defaults,
            settings_dir,
            stopping: None,
            restarting: None,
            api: Api::default(),
            default_provider: Provider::Claude,
            relaunch: Box::new(relaunch),
        }
    }

    fn serve(mut self, hear: Receiver<Heard>) -> i32 {
        loop {
            let wait = if self.engine.threads().is_empty() {
                IDLE
            } else {
                PUMP
            };
            match hear.recv_timeout(wait) {
                Ok(heard) => self.hear(heard),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    crate::log!("server: the listener stopped");
                    self.stop();
                    return 1;
                }
            }
            while let Ok(heard) = hear.try_recv() {
                self.hear(heard);
            }
            self.engine.pump();
            if self.swept.elapsed() >= SWEEP_INTERVAL {
                self.swept = Instant::now();
                self.engine.sweep();
            }
            self.answer_bootstraps();
            self.answer_searches();
            self.answer_reads();
            self.forward_renders();
            self.coordinate();
            self.broadcast();
            self.api.pump(&self.engine);
            if let Some(why) = signals::stop_requested() {
                crate::log!("server: stopping: {why}");
                self.stop();
                return 0;
            }
            if let Some(when) = self.stopping {
                if when == When::Now || self.engine.idle() {
                    crate::log!("server: stopping, as a window asked");
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
                        lists_parked: false,
                        watching: BTreeSet::new(),
                    },
                );
            }
            Heard::Gone(id) => {
                self.clients.remove(&id);
                self.renders.client_gone(id);
                self.list_parked();
            }
            Heard::Message(id, message) => self.message(id, message),
            Heard::Api(heard) => self.api_heard(heard),
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
                let uptime = protocol::ms_since(crate::clock::instant(), self.engine.boot());
                let welcome =
                    protocol::welcome(generation, self.info.clone(), self.boot_id.clone(), uptime);
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
                    client.lists_parked = capabilities.contains(&Capability::ParkedThreads);
                    self.list_parked();
                    let snapshot = self.engine.snapshot(&self.boot_id);
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
            ClientMessage::Rendered {
                request,
                png,
                error,
            } => self.renders.answer(request, png, error),
            ClientMessage::Hello { .. } | ClientMessage::Unknown => {}
        }
    }

    /// Stream a Thread to a client: from the ring when it resumes this boot
    /// within it, otherwise its history first, which the client's writer
    /// thread reads from the log and sends in pieces.
    fn watch(&mut self, id: ClientId, thread: ThreadId, resume: Option<protocol::Resume>) {
        self.broadcast();
        let missed = resume
            .filter(|resume| resume.boot_id == self.boot_id)
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
                let Some(client) = self.clients.get(&id) else {
                    return;
                };
                if client
                    .connection
                    .defer(Box::new(move |send| history.send(send)))
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
    }

    fn command(&mut self, id: ClientId, Command { id: request, op }: Command) {
        let executed = match &op {
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
                        Ok(Some(crate::workspace::repo_listing(&root).into()))
                    });
                    return;
                }
                None => Err(failed("no such Project")),
            },
            Op::ListDir { path } => {
                let path = path.clone();
                self.read_off_pump(id, request, move || {
                    crate::workspace::list_dir(&path)
                        .map(|listing| Some(listing.into()))
                        .map_err(failed)
                });
                return;
            }
            Op::ReviewDiff {
                path,
                scope,
                commit,
            } => match scope.diff(commit.clone()) {
                Some(diff) => {
                    let path = path.clone();
                    self.read_off_pump(id, request, move || {
                        crate::review::git::read(&path, &diff)
                            .map(|text| Some(Done::Diff { text }))
                            .map_err(failed)
                    });
                    return;
                }
                None => Err(failed("this server cannot read that scope")),
            },
            Op::ListCommits { path } => {
                let path = path.clone();
                self.read_off_pump(id, request, move || {
                    crate::review::git::commits(&path)
                        .map(|commits| {
                            Some(Done::Commits {
                                commits: commits.into_iter().map(Into::into).collect(),
                            })
                        })
                        .map_err(failed)
                });
                return;
            }
            Op::PullRequest { path } => {
                let path = path.clone();
                self.read_off_pump(id, request, move || {
                    Ok(Some(Done::PullRequest {
                        json: crate::workspace::pr_view(&path),
                    }))
                });
                return;
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
            Op::Coordinate { project, provider } => self
                .start_coordinator(*project, *provider, Default::default())
                .map(|thread| Some(Done::Thread { thread }))
                .map_err(|error| failed(error.message)),
            Op::Stop { when } => {
                self.stopping = Some(*when);
                Ok(None)
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
                if let (Op::Answer { thread, handle, .. }, Ok(_)) = (op, &executed) {
                    self.answered.insert((*thread, handle.clone()), id);
                }
                executed
            }
        };
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

    /// Settings apply to the next Session, as they always have.
    fn apply_settings(&mut self, settings: &Settings) {
        *self.defaults.lock().unwrap_or_else(|e| e.into_inner()) =
            SessionDefaults::from_settings(settings);
        self.engine
            .set_suggestions_enabled(settings.placeholder_suggestions);
        self.engine
            .set_allowance_thresholds(settings.allowance_warnings.clone());
        self.default_provider = settings.default_provider;
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
                    crate::log!("server: a first send did not start: {error}");
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
        let (answer, answered) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = answer.send(read());
        });
        self.reads.push((client, request, answered));
    }

    /// Reply to each read off the pump that finished.
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

    /// Read the parked Threads' rows only while a window lists them.
    fn list_parked(&mut self) {
        let wanted = self
            .clients
            .values()
            .any(|client| client.welcomed && client.lists_parked);
        self.engine.list_parked(wanted);
    }

    /// Hand every message the Engine published to the clients that want
    /// it: a Thread's stream to its watchers, the rest to everyone.
    fn broadcast(&mut self) {
        let mut messages = self.engine.take_messages();
        messages.extend(self.decision_changes());
        for message in messages {
            self.api.relay(&message);
            match &message {
                ServerMessage::Input { thread, seq, .. } => {
                    self.rings
                        .entry(*thread)
                        .or_default()
                        .push(*seq, message.clone());
                }
                // A new Activity: a resume cannot cross its start.
                ServerMessage::HistoryEnd { thread, seq, .. } => {
                    self.rings.insert(*thread, Ring::after(*seq));
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
                self.send(id, message.clone());
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
        crate::log!("server: client {id} fell too far behind");
        self.clients.remove(&id);
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
                crate::log!("server: not restarting: {error}");
                return None;
            }
        };
        crate::log!("server: restarting ({when:?})");
        self.close_decisions();
        self.say(ServerMessage::Restarting { when });
        self.halt();
        match (self.relaunch)(&executable) {
            Ok(()) => Some(0),
            Err(error) => {
                // Sessions are already gone: a failure exit has launchd or
                // systemd (or the next window) start a Server again.
                crate::log!("server: could not restart: {error}");
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
        agent::start_detached(executable, &["--after", &pid])
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

/// Whether a window should ask its Server to restart into a newer build:
/// only when this window is newer (equal versions never restart on their
/// own) and the Server runs this window's own executable — a Server always
/// restarts into its own path, so any other would be asked again after
/// every restart.
pub fn wants_restart(
    app_version: &str,
    app_executable: &std::path::Path,
    server: &ServerInfo,
) -> bool {
    let same = |a: &std::path::Path, b: &std::path::Path| {
        a == b
            || a.canonicalize()
                .ok()
                .zip(b.canonicalize().ok())
                .is_some_and(|(a, b)| a == b)
    };
    newer(app_version, &server.version) && same(app_executable, &server.executable)
}

/// Whether semantic version `a` is newer than `b`. Anything unreadable is
/// not newer.
fn newer(a: &str, b: &str) -> bool {
    let parse = |version: &str| -> Option<(u64, u64, u64)> {
        let core = version.split(['-', '+']).next()?;
        let mut parts = core.split('.').map(str::parse::<u64>);
        Some((
            parts.next()?.ok()?,
            parts.next()?.ok()?,
            parts.next()?.ok()?,
        ))
    };
    matches!((parse(a), parse(b)), (Some(a), Some(b)) if a > b)
}

fn failed(error: impl ToString) -> WireError {
    WireError {
        code: ErrorCode::Failed,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests;
