//! A window's link to its Server (ADR 0025): the socket, the acts as
//! commands, and staying attached. An act waits for its reply, reading
//! what the Server says before it into the inbox, so the window is up to
//! date when the act returns. An ask (`Ask`, ADR 0041), a first send and a
//! file search answer later.
//! When the Server goes, the window keeps what it drew and attaches again
//! in the background, resuming each Thread where it left off.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

use base64::Engine as _;

use super::held::Link;

use ferrite_core::ThreadId;
use ferrite_core::clock::ServerClock;
// A Remote's answers ring as the pump routes them (ADR 0041).
use ferrite_core::checkout::{ListedWorktree, RepoListing};
use ferrite_core::doorbell::Sender;
use ferrite_core::folder::{DirEntry, Listing};
use ferrite_core::link;
use ferrite_core::machine::{self, Bridge, Failure};
use ferrite_core::project::ProjectId;
use ferrite_core::prompt_files::FileSuggestion;
use ferrite_core::protocol::{
    self, Capability, ClientMessage, Command, Done, ErrorCode, Op, Peer, Resume, ServerInfo,
    ServerMessage, Snapshot, WireError, WirePng,
};
use ferrite_core::protocol::{ReviewPullRequest, ReviewPullRequests, WirePrFilter, WirePrList};
use ferrite_core::pull_request::{parse_pr_json, PullRequest};
use ferrite_core::roster::{BootstrapResult, Bootstrapped, DraftId};
use ferrite_core::server::ServerId;
use ferrite_core::visual::{RenderRequest, Renderer};
use ferrite_reviewr::ferrite::{Bases, FileSides, Read};

/// Where a review's read goes, and what it read: asked again as a
/// `review_diff` of a Server from before `review_read`.
type Review = (PathBuf, Read, Sender<io::Result<Vec<FileSides>>>);
use ferrite_reviewr::git::{CommitRow, WorktreeEntry};
use ferrite_reviewr::search::SearchResults;

/// Where a later-answered command's reply goes (`Remote::ask`).
pub(super) type Answer = Box<dyn FnOnce(Result<Option<Done>, WireError>) + Send>;

/// A command whose reply the window takes later: where it goes, and when
/// it is answered that the Server did not answer, if ever.
struct Waiting {
    answer: Answer,
    by: Option<Instant>,
}

/// How long an act waits for the Server's reply, and an ask (`ask_in_time`)
/// before it is answered that the Server did not answer.
const REPLY_WAIT: Duration = Duration::from_secs(10);
/// How long the window waits before attaching again, at first and at most;
/// a machine's attempts each start an ssh, so they back off further.
const RETRY_FIRST: Duration = Duration::from_millis(250);
const RETRY_MOST: Duration = Duration::from_secs(5);
const MACHINE_RETRY_MOST: Duration = Duration::from_secs(30);
/// How long a restarting Server is given to come back before the window
/// starts one of its own.
const RESTART_GRACE: Duration = Duration::from_secs(2);

/// What attaching needs: where the Server is, how to start one, and who
/// this window is.
pub struct Attach {
    /// For this computer's Server, the Store it serves, its socket beside
    /// it, which the window reads an open Thread's header from. A
    /// machine's path only names the window's copy of its Projects.
    /// Either's parked Threads are what its Server publishes (ADR 0026).
    pub store: PathBuf,
    pub server: Endpoint,
    /// What saved files name the machine (ADR 0045).
    pub saved_as: ferrite_core::MachineKey,
    pub client: Peer,
    /// Draws the Visuals the Server's agents ask for; none, and this window
    /// does not offer to.
    pub renderer: Option<Arc<dyn Renderer>>,
    /// This window can realize general website pages.
    pub browser: bool,
    /// This window takes the API's `window.*` actions: a `ferrite
    /// --headless` one (ADR 0038).
    pub driven: bool,
}

/// Where the Server is.
#[derive(Clone)]
pub enum Endpoint {
    /// This computer's, started as `<executable> server` when none answers.
    Local { executable: PathBuf },
    /// A machine's, through its bridge (ADR 0026). Never started from here.
    Machine(Arc<Bridge>),
}

/// A restart of the Server this window asked for or heard of, as far as
/// the window has seen it through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerRestart {
    /// Asked for, or the Server said it is restarting: not back yet.
    Restarting,
    /// The window reached the successor, a new boot running `version`.
    Restarted { version: String },
    /// Refused, or no successor within `RESTART_TIMEOUT`: why.
    Failed { reason: String },
}

/// A restart being seen through: where it stands, the boot it restarts
/// away from (none for one the Server refused: nothing restarts), and
/// when it was asked for.
struct Watch {
    state: ServerRestart,
    from: Option<String>,
    asked: Instant,
}

/// How long a restart is given to come back before the window says it
/// did not.
const RESTART_TIMEOUT: Duration = Duration::from_secs(30);

/// What a window knows of its Server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerState {
    Attached {
        server: ServerInfo,
        /// Which Server it is; `None` for a Server from before every
        /// Server had an id.
        id: Option<ServerId>,
        /// When the Server started, on this side's clock.
        since: Instant,
        /// How its wall clock reads on this side's.
        clock: ServerClock,
    },
    /// No Server answers: why, and the window keeps trying. `attention`:
    /// what the operator must do first (sign in, install `ferrited`) for
    /// trying again to help.
    Detached {
        reason: String,
        attention: Option<Failure>,
    },
}

/// What arrived from the Server for the window.
pub(crate) enum Arrival {
    Message(ServerMessage),
    /// Attached afresh: the shared state is this snapshot's. `new_boot`
    /// means a different Server than before, so every mirror starts over.
    Attached {
        snapshot: Snapshot,
        new_boot: bool,
    },
    /// A first send settled: its Thread, or why not.
    Started(BootstrapResult),
    /// A command the window takes the reply of later was answered, or ran
    /// out of time: what asked hears it (ADR 0041).
    Answered,
}

/// The open connection.
struct Attachment {
    client: link::Client,
    boot_id: String,
    server: ServerInfo,
    id: Option<ServerId>,
    /// The Server's boot on this side's clock: `at_ms` counts from it.
    boot: Instant,
    /// It welcomed this connection with `Capability::ForkOf`.
    names_fork_owners: bool,
}

pub(crate) struct Remote {
    attach: Attach,
    attachment: Option<Attachment>,
    /// How the Server's wall clock reads on this side's, as last measured:
    /// a time it stamped keeps reading on this computer's clock after its
    /// link has gone.
    clock: ServerClock,
    /// Why there is no attachment, while there is none.
    detached: String,
    /// The last attach failed in a way only the operator can fix.
    attention: Option<Failure>,
    /// An attach running in the background.
    attaching: Option<Receiver<io::Result<link::Attached>>>,
    /// A retry was asked for while `attaching` ran. That attach began
    /// before the ask, so unless it attaches, the next starts once it ends.
    retry_asked: bool,
    retry_at: Instant,
    backoff: Duration,
    /// The boot the window's mirrors were built from.
    last_boot: Option<String>,
    /// The restart being seen through.
    restart: Option<Watch>,
    next_request: u64,
    /// What the Server said while an act waited for its reply.
    inbox: VecDeque<ServerMessage>,
    /// First sends in flight, by command.
    starting: BTreeMap<u64, (DraftId, String)>,
    /// First sends the connection took with it, for the next `poll`.
    abandoned: Vec<Arrival>,
    searches: BTreeMap<u64, Sender<io::Result<Vec<FileSuggestion>>>>,
    /// Commands whose reply the window takes later, each with where its
    /// answer goes and by when (`ask`, `ask_in_time`).
    answers: BTreeMap<u64, Waiting>,
    /// `review_read`s in flight, by command.
    reviews: BTreeMap<u64, Review>,
    watching: BTreeSet<ThreadId>,
    /// Visuals drawn here, answered from the renderer's threads.
    rendered: (
        ferrite_core::doorbell::Sender<ClientMessage>,
        Receiver<ClientMessage>,
    ),
}

impl Remote {
    /// Detached, attaching from the first `poll`.
    pub(crate) fn new(attach: Attach) -> Self {
        let detached = match &attach.server {
            Endpoint::Local { .. } => "attaching to the Ferrite server".into(),
            Endpoint::Machine(bridge) => format!("connecting to {}", bridge.name()),
        };
        Self {
            attach,
            attachment: None,
            clock: ServerClock::default(),
            detached,
            attention: None,
            attaching: None,
            retry_asked: false,
            retry_at: Instant::now(),
            backoff: RETRY_FIRST,
            last_boot: None,
            restart: None,
            next_request: 0,
            inbox: VecDeque::new(),
            starting: BTreeMap::new(),
            abandoned: Vec::new(),
            searches: BTreeMap::new(),
            answers: BTreeMap::new(),
            reviews: BTreeMap::new(),
            watching: BTreeSet::new(),
            rendered: ferrite_core::doorbell::channel(),
        }
    }

    /// Attach now, waiting for it: when a Server answers or can be
    /// started. Otherwise the window stays detached and keeps trying.
    pub(crate) fn attach_now(&mut self) -> Option<Arrival> {
        let attached = attach_to(&self.attach.store, &self.attach.server, hello(&self.attach), None);
        self.attached(attached)
    }

    pub(crate) fn state(&self) -> ServerState {
        match &self.attachment {
            Some(attachment) => ServerState::Attached {
                server: attachment.server.clone(),
                id: attachment.id.clone(),
                since: attachment.boot,
                clock: self.clock,
            },
            None => ServerState::Detached {
                reason: self.detached.clone(),
                attention: self.attention.clone(),
            },
        }
    }

    /// Attach again now rather than at the next retry. An attach under way
    /// began before this ask: its failure does not answer it.
    pub(crate) fn retry(&mut self) {
        self.retry_at = Instant::now();
        self.backoff = RETRY_FIRST;
        self.attention = None;
        self.retry_asked = self.attaching.is_some();
    }

    /// Whether attaching waits for the operator: a machine that needs a
    /// sign-in is not asked again (each ask is a refused login on it) until
    /// they have signed in, or ask.
    fn waits_for_operator(&mut self) -> bool {
        if !self.attention.as_ref().is_some_and(Failure::needs_sign_in) {
            return false;
        }
        if self.bridge().is_some_and(|bridge| bridge.take_signed_in()) {
            self.retry();
            return false;
        }
        true
    }

    /// Each ask whose time is up is answered so; true when one was.
    fn time_out_asks(&mut self) -> bool {
        let now = ferrite_core::clock::instant();
        let late: Vec<u64> = self
            .answers
            .iter()
            .filter(|(_, waiting)| waiting.by.is_some_and(|by| now >= by))
            .map(|(id, _)| *id)
            .collect();
        for id in &late {
            if let Some(waiting) = self.answers.remove(id) {
                (waiting.answer)(Err(WireError {
                    code: ErrorCode::Unconfirmed,
                    message: "the Ferrite server did not answer in time".into(),
                }));
            }
        }
        !late.is_empty()
    }

    /// Wait, until `deadline`, for a stopping Server to be gone: it lets
    /// its windows go once the Store is flushed, and its socket stops
    /// answering when its process ends. Nothing attaches again meanwhile.
    pub(crate) fn wait_gone(&mut self, deadline: Instant) -> io::Result<()> {
        if let Some(attachment) = &self.attachment {
            while let Ok(message) = attachment
                .client
                .messages
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                self.inbox.push_back(message);
            }
        }
        self.detach("the Ferrite server stopped");
        let address = link::address(&self.attach.store);
        loop {
            match link::connect(&address) {
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                    ) =>
                {
                    return Ok(());
                }
                _ if Instant::now() >= deadline => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "the Ferrite server is still running",
                    ));
                }
                _ => std::thread::sleep(Duration::from_millis(25)),
            }
        }
    }

    /// A connection of its own to this Server, attached when the returned
    /// function runs, for a transfer that must neither wait behind the
    /// window's acts nor hold them up. Its endpoint goes with it: a
    /// machine's bridge carries it.
    /// A connection of its own to the Server, `patience` how long the
    /// Server is given to welcome it, if any.
    fn side_connection(
        &self,
        patience: Option<Duration>,
    ) -> io::Result<impl FnOnce() -> io::Result<(link::Attached, Endpoint)> + Send + 'static> {
        if self.attachment.is_none() {
            return Err(io::Error::new(io::ErrorKind::NotConnected, &*self.detached));
        }
        let endpoint = self.attach.server.clone();
        let store = self.attach.store.clone();
        let hello = ClientMessage::Hello {
            generation: protocol::GENERATION,
            client: self.attach.client.clone(),
            capabilities: Vec::new(),
        };
        Ok(move || Ok((attach_to(&store, &endpoint, hello, patience)?, endpoint)))
    }

    fn attachment_connection(&self) -> io::Result<impl FnOnce() -> io::Result<ferrite_core::attachment_transfer::Exchange> + Send + 'static> {
        let expected_boot = self.attachment.as_ref().ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, &*self.detached))?.boot_id.clone();
        let connect = self.side_connection(None)?;
        Ok(move || {
            let (attached, endpoint) = connect()?;
            if attached.snapshot.boot_id != expected_boot {
                return Err(io::Error::new(io::ErrorKind::NotConnected, "The attachment server changed. Reconnect and retry."));
            }
            let mut client = attached.client;
            let mut next = 0;
            Ok(Box::new(move |request| {
                let _route = &endpoint;
                next += 1;
                match client.ask(next, Op::Attachment { request }, Duration::from_secs(30))? {
                    Some(Done::Attachment { reply }) => Ok(reply),
                    _ => Err(io::Error::new(io::ErrorKind::InvalidData, "The server did not answer the attachment operation")),
                }
            }) as ferrite_core::attachment_transfer::Exchange)
        })
    }

    /// A tree's own connection: a Server that takes it and never welcomes
    /// it is given `tree::PATIENCE`, so a copy cancelled meanwhile ends.
    pub(crate) fn tree(&self) -> io::Result<ferrite_core::tree::Connect> {
        let connect = self.side_connection(Some(ferrite_core::tree::PATIENCE))?;
        Ok(Box::new(move || {
            let (attached, endpoint) = connect()?;
            Ok(Box::new(Routed {
                exchange: ferrite_core::tree::Remote::new(attached.client),
                _route: endpoint,
            }) as Box<dyn ferrite_core::tree::Exchange>)
        }))
    }

    pub(crate) fn list_dir(&mut self, path: &Path) -> io::Result<Receiver<io::Result<Listing>>> {
        let op = Op::ListDir {
            path: path.to_path_buf(),
        };
        self.ask(op, |done| match done {
            Done::Listing { path, entries } => Some(Listing {
                path,
                entries: entries
                    .into_iter()
                    .map(|entry| DirEntry {
                        name: entry.name,
                        directory: entry.directory,
                    })
                    .collect(),
            }),
            _ => None,
        })
    }

    pub(crate) fn list_repo(
        &mut self,
        project: ProjectId,
    ) -> io::Result<Receiver<io::Result<RepoListing>>> {
        self.ask(Op::ListRepo { project }, |done| match done {
            Done::RepoListing {
                checked_out,
                worktrees,
                branches,
            } => Some(RepoListing {
                checked_out,
                worktrees: worktrees
                    .into_iter()
                    .map(|worktree| ListedWorktree {
                        path: worktree.path,
                        branch: worktree.branch,
                    })
                    .collect(),
                branches,
            }),
            _ => None,
        })
    }

    /// A `review_read`'s reply: its files, or — from a Server that does
    /// not know the command — what its `review_diff` answers.
    fn reviewed(&mut self, (path, read, answer): Review, reply: Result<Option<Done>, WireError>) {
        let files = match reply {
            Err(error) if error.code == ErrorCode::Unsupported => {
                return self.review_as_before(&path, &read, answer);
            }
            Err(error) => Err(error.into()),
            Ok(Some(Done::Review { files })) => Ok(files.into_iter().map(Into::into).collect()),
            Ok(_) => Err(io::Error::other("the server answered something else")),
        };
        let _ = answer.send(files);
    }

    /// `read` by a Server from before `review_read`: its `review_diff`'s
    /// hunks as the files' sides. A last turn or a run of commits it cannot
    /// read, so it is never asked one as something else.
    fn review_as_before(
        &mut self,
        path: &Path,
        read: &Read,
        answer: Sender<io::Result<Vec<FileSides>>>,
    ) {
        let Some(op) = Op::review_diff(path, read) else {
            let _ = answer.send(Err(io::Error::other(
                "this Ferrite server is too old to review that: update it",
            )));
            return;
        };
        match self.send_command(op) {
            Ok(id) => self.answer(id, answer, |done| match done {
                Done::Diff { text } => Some(protocol::sides_of_unified(&text)),
                _ => None,
            }),
            Err(error) => {
                let _ = answer.send(Err(error.into()));
            }
        }
    }

    pub(crate) fn browser_dispatch(
        &mut self,
        request: u64,
        boot_id: String,
        thread: ThreadId,
        owner: ferrite_core::browser::Owner,
        document: ferrite_core::browser::Document,
        revision: u64,
    ) -> io::Result<Receiver<io::Result<u64>>> {
        self.ask(
            Op::BrowserDispatch { request, boot_id, thread, owner, document, revision },
            |done| match done {
                Done::BrowserDispatch { remaining_ms } => Some(remaining_ms),
                _ => None,
            },
        )
    }

    pub(crate) fn browser_capture(
        &mut self,
        thread: ThreadId,
        owner: ferrite_core::browser::Owner,
        lease: ferrite_core::browser::Lease,
        capture: ferrite_core::browser_annotations::Capture,
        png: Option<ferrite_core::protocol::WirePng>,
    ) -> io::Result<Receiver<io::Result<ferrite_core::browser_annotations::Annotation>>> {
        let attachment = self
            .attachment
            .as_mut()
            .ok_or_else(|| io::Error::from(gone(&self.detached)))?;
        self.next_request += 1;
        let id = self.next_request;
        let op = Op::BrowserCapture {
            thread,
            owner,
            lease,
            capture,
            png,
        };
        let sent = {
            #[cfg(any(test, feature = "test-support"))]
            let _sent = waits::Timing::of(&op, false);
            attachment
                .client
                .send_capture(ClientMessage::Command(Command { id, op }))
        };
        if let Err(error) = sent {
            self.detach(&format!("the capture upload could not be queued: {error}"));
            return Err(error);
        }
        let (answer, answered) = ferrite_core::doorbell::channel();
        self.answer(id, answer, |done| match done {
            Done::BrowserCapture { annotation } => Some(annotation),
            _ => None,
        });
        Ok(answered)
    }

    pub(crate) fn browser_capture_image(
        &mut self,
        thread: ThreadId,
        hash: String,
    ) -> io::Result<Receiver<io::Result<ferrite_core::protocol::WirePng>>> {
        self.ask(
            Op::BrowserCaptureImage { thread, hash },
            |done| match done {
                Done::BrowserCaptureImage { png } => Some(png),
                _ => None,
            },
        )
    }

    /// The Server's defaults as its own file says, or, with `set`, after
    /// writing them there.
    pub(crate) fn server_defaults(
        &mut self,
        set: Option<ferrite_core::settings::ServerDefaults>,
    ) -> io::Result<Receiver<io::Result<ferrite_core::settings::ServerDefaults>>> {
        self.ask(Op::ServerDefaults { set }, |done| match done {
            Done::ServerDefaults { defaults } => Some(defaults),
            _ => None,
        })
    }

    /// Send `op` and answer its reply later, as `read` makes of it.
    fn ask<T: Send + 'static>(
        &mut self,
        op: Op,
        read: fn(Done) -> Option<T>,
    ) -> io::Result<Receiver<io::Result<T>>> {
        let id = self.send_command(op).map_err(io::Error::from)?;
        let (answer, answered) = ferrite_core::doorbell::channel();
        self.answer(id, answer, read);
        Ok(answered)
    }

    /// Command `id`'s reply, read by `read`, goes to `answer`.
    fn answer<T: Send + 'static>(
        &mut self,
        id: u64,
        answer: Sender<io::Result<T>>,
        read: fn(Done) -> Option<T>,
    ) {
        self.answers.insert(
            id,
            Waiting {
                answer: Box::new(move |result| {
                    let read = result.map_err(io::Error::from).and_then(|done| {
                        done.and_then(read)
                            .ok_or_else(|| io::Error::other("the server answered something else"))
                    });
                    let _ = answer.send(read);
                }),
                by: None,
            },
        );
    }

    /// Stream exactly `threads` to this window: each from where its mirror
    /// left off when the boot is the same, else from its history. The rest
    /// stop streaming.
    pub(crate) fn watch(&mut self, threads: &[ThreadId], seq: impl Fn(ThreadId) -> Option<u64>) {
        let Some(attachment) = &self.attachment else {
            return;
        };
        let boot_id = attachment.boot_id.clone();
        let dropped: Vec<ThreadId> = self
            .watching
            .iter()
            .filter(|thread| !threads.contains(thread))
            .copied()
            .collect();
        for thread in dropped {
            self.watching.remove(&thread);
            self.say(&ClientMessage::Unwatch { thread });
        }
        for &thread in threads {
            if self.watching.contains(&thread) {
                continue;
            }
            let resume = seq(thread).map(|after| Resume {
                boot_id: boot_id.clone(),
                after,
            });
            self.watching.insert(thread);
            self.say(&ClientMessage::Watch { thread, resume });
        }
    }

    fn send_command(&mut self, op: Op) -> Result<u64, WireError> {
        #[cfg(any(test, feature = "test-support"))]
        let _sent = waits::Timing::of(&op, false);
        if self.attachment.is_none() {
            return Err(WireError {
                code: ErrorCode::Failed,
                message: self.detached.clone(),
            });
        }
        self.next_request += 1;
        let id = self.next_request;
        self.say(&ClientMessage::Command(Command { id, op }));
        match self.attachment {
            Some(_) => Ok(id),
            None => Err(gone(&self.detached)),
        }
    }

    pub(super) fn say(&mut self, message: &ClientMessage) {
        let Some(attachment) = &mut self.attachment else {
            return;
        };
        if let Err(error) = attachment.client.send(message) {
            self.detach(&format!("the Ferrite server went away: {error}"));
        }
    }

    fn attached(&mut self, attached: io::Result<link::Attached>) -> Option<Arrival> {
        let link::Attached {
            client,
            welcome,
            clock,
            snapshot,
        } = match attached {
            Ok(attached) => attached,
            Err(error) => {
                let most = match &self.attach.server {
                    Endpoint::Local { .. } => {
                        self.detached = format!("no Ferrite server: {error}");
                        RETRY_MOST
                    }
                    Endpoint::Machine(bridge) => {
                        self.detached = format!("{}: {error}", bridge.name());
                        MACHINE_RETRY_MOST
                    }
                };
                self.attention = machine::failure(&error)
                    .filter(|failure| failure.needs_operator())
                    .cloned();
                self.retry_at = Instant::now() + self.backoff;
                self.backoff = (self.backoff * 2).min(most);
                return None;
            }
        };
        let ServerMessage::Welcome {
            server,
            uptime_ms,
            capabilities,
            server_id,
            ..
        } = welcome
        else {
            return None;
        };
        let now = ferrite_core::clock::instant();
        let boot = now
            .checked_sub(Duration::from_millis(uptime_ms.unwrap_or(0)))
            .unwrap_or(now);
        let new_boot = self.last_boot.as_ref() != Some(&snapshot.boot_id);
        // A new boot sees a restart through, even one that came back
        // after the window said it had not.
        if let Some(watch) = &mut self.restart {
            if !matches!(watch.state, ServerRestart::Restarted { .. })
                && watch
                    .from
                    .as_ref()
                    .is_some_and(|from| *from != snapshot.boot_id)
            {
                watch.state = ServerRestart::Restarted {
                    version: server.version.clone(),
                };
                ferrite_core::log!("the Ferrite server restarted: {}", server.version);
            }
        }
        self.last_boot = Some(snapshot.boot_id.clone());
        self.clock = clock;
        self.attachment = Some(Attachment {
            client,
            boot_id: snapshot.boot_id.clone(),
            server,
            id: server_id,
            boot,
            names_fork_owners: capabilities.contains(&Capability::ForkOf),
        });
        self.backoff = RETRY_FIRST;
        self.attention = None;
        self.watching.clear();
        ferrite_core::log!("attached to the Ferrite server, boot {}", snapshot.boot_id);
        Some(Arrival::Attached { snapshot, new_boot })
    }

    /// The connection is gone: what was in flight is answered here, since
    /// the Server will not answer it. Nothing is sent again.
    fn detach(&mut self, reason: &str) {
        if self.attachment.take().is_none() {
            return;
        }
        ferrite_core::log!("{reason}");
        self.detached = reason.to_string();
        self.retry_at = Instant::now();
        self.watching.clear();
        self.searches.clear();
        self.answers.clear();
        self.reviews.clear();
        self.abandon_starting();
    }

    fn heard(&mut self, message: ServerMessage) -> Vec<Arrival> {
        match message {
            ServerMessage::Reply { id, ok, error } => {
                if let Some(error) = error
                    .as_ref()
                    .filter(|error| error.code == ErrorCode::Lagged)
                {
                    self.detach(&error.message);
                    return Vec::new();
                }
                if let Some(review) = self.reviews.remove(&id) {
                    let reply = error.map_or(Ok(ok), Err);
                    self.reviewed(review, reply);
                    return vec![Arrival::Answered];
                }
                let result = error.map_or(Ok(ok), Err);
                if let Some((draft, prompt)) = self.starting.remove(&id) {
                    let result = result.map_err(io::Error::from).and_then(|done| match done {
                        Some(Done::Thread { thread }) => Ok(Bootstrapped { thread }),
                        _ => Err(io::Error::other("the server started no Thread")),
                    });
                    return vec![Arrival::Started(BootstrapResult {
                        draft: Some(draft),
                        prompt,
                        result,
                    })];
                }
                let answered = vec![Arrival::Answered];
                if let Some(answer) = self.searches.remove(&id) {
                    let _ = answer.send(result.map_err(io::Error::from).map(|done| {
                        match done {
                            Some(Done::Files { files }) => files
                                .into_iter()
                                .map(|file| FileSuggestion {
                                    path: file.path,
                                    is_directory: file.is_directory,
                                    matched: file.matched,
                                })
                                .collect(),
                            _ => Vec::new(),
                        }
                    }));
                    return answered;
                } else if let Some(waiting) = self.answers.remove(&id) {
                    (waiting.answer)(result);
                    return answered;
                }
                Vec::new()
            }
            ServerMessage::Render {
                request,
                id,
                title,
                html,
            } => {
                self.render(request, RenderRequest { id, title, html });
                Vec::new()
            }
            // The connection closes after either; attaching again follows.
            ServerMessage::Restarting { .. } => {
                self.restarting();
                self.detach("the Ferrite server is restarting");
                self.retry_at = Instant::now() + RESTART_GRACE;
                Vec::new()
            }
            ServerMessage::Stopping => {
                self.detach("the Ferrite server stopped");
                Vec::new()
            }
            message => vec![Arrival::Message(message)],
        }
    }

    /// A restart is under way: seen through once the window reaches a new
    /// boot. Asked for again while one is, it keeps the first's clock.
    pub(crate) fn restarting(&mut self) {
        if self
            .restart
            .as_ref()
            .is_some_and(|watch| watch.state == ServerRestart::Restarting)
        {
            return;
        }
        let from = self
            .attachment
            .as_ref()
            .map(|attachment| attachment.boot_id.clone())
            .or_else(|| self.last_boot.clone());
        ferrite_core::log!("the Ferrite server is restarting");
        self.restart = Some(Watch {
            state: ServerRestart::Restarting,
            from,
            asked: ferrite_core::clock::instant(),
        });
    }

    /// The Server refused the restart.
    pub(crate) fn restart_refused(&mut self, reason: String) {
        ferrite_core::log!("the Ferrite server did not restart: {reason}");
        self.restart = Some(Watch {
            state: ServerRestart::Failed { reason },
            from: None,
            asked: ferrite_core::clock::instant(),
        });
    }

    /// The restart as far as the window has seen it through.
    pub(crate) fn restart(&self) -> Option<&ServerRestart> {
        self.restart.as_ref().map(|watch| &watch.state)
    }

    /// A restart not back within `RESTART_TIMEOUT` did not happen, as far
    /// as anyone can tell from here.
    fn time_out_restart(&mut self) {
        let detached = &self.detached;
        let attached = self.attachment.is_some();
        let Some(watch) = &mut self.restart else {
            return;
        };
        if watch.state != ServerRestart::Restarting
            || ferrite_core::clock::instant().saturating_duration_since(watch.asked) < RESTART_TIMEOUT
        {
            return;
        }
        // Attached to another boot, it would read restarted (`attached`).
        let reason = if attached {
            "Still the same server after 30 s".to_string()
        } else {
            format!("Not back after 30 s ({detached})")
        };
        ferrite_core::log!("the Ferrite server did not restart: {reason}");
        // The boot it left stays known: a late new one still reads restarted.
        watch.state = ServerRestart::Failed { reason };
    }

    /// First sends the Server will never answer now: each Draft hears it
    /// may not have started, at the next `poll` — wherever the connection
    /// was found gone.
    fn abandon_starting(&mut self) {
        let abandoned = std::mem::take(&mut self.starting)
            .into_values()
            .map(|(draft, prompt)| {
                Arrival::Started(BootstrapResult {
                    draft: Some(draft),
                    prompt,
                    result: Err(io::Error::other(
                        "the Ferrite server went away; the Thread may not have started",
                    )),
                })
            });
        self.abandoned.extend(abandoned);
    }

    /// Draw a Visual for the Server's agent and answer it from the
    /// renderer's thread.
    fn render(&mut self, request: u64, visual: RenderRequest) {
        let Some(renderer) = &self.attach.renderer else {
            self.say(&ClientMessage::Rendered {
                request,
                png: None,
                error: Some("this window cannot draw visuals".into()),
            });
            return;
        };
        let (reply, drawn) = ferrite_core::visual::reply();
        renderer.render(visual, reply);
        let answers = self.rendered.0.clone();
        let waiter = std::thread::Builder::new()
            .name("ferrite-render-answer".into())
            .spawn(move || {
                let answer = match drawn.recv() {
                    Ok(Ok(rendered)) => ClientMessage::Rendered {
                        request,
                        png: Some(WirePng {
                            base64: base64::engine::general_purpose::STANDARD.encode(rendered.png),
                            width: rendered.width,
                            height: rendered.height,
                        }),
                        error: None,
                    },
                    Ok(Err(error)) => ClientMessage::Rendered {
                        request,
                        png: None,
                        error: Some(error),
                    },
                    Err(_) => ClientMessage::Rendered {
                        request,
                        png: None,
                        error: Some("the window could not draw it".into()),
                    },
                };
                let _ = answers.send(answer);
            });
        if let Err(error) = waiter {
            ferrite_core::log!("a visual could not be answered: {error}");
        }
    }
}

/// A Server's link: every act a command over its socket.
impl Link for Remote {
    /// Everything that arrived since the last call, attaching again first
    /// when the Server is gone and it is time.
    fn poll(&mut self) -> Vec<Arrival> {
        self.time_out_restart();
        let mut arrivals = Vec::new();
        if self.time_out_asks() {
            arrivals.push(Arrival::Answered);
        }
        if let Some(attaching) = &self.attaching {
            match attaching.try_recv() {
                Ok(attached) => {
                    self.attaching = None;
                    arrivals.extend(self.attached(attached));
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.attaching = None,
            }
            // Ended without attaching, it leaves a retry asked meanwhile
            // to the next attach, which starts below.
            if self.attaching.is_none()
                && std::mem::take(&mut self.retry_asked)
                && self.attachment.is_none()
            {
                self.retry();
            }
        }
        if self.attachment.is_none()
            && self.attaching.is_none()
            && Instant::now() >= self.retry_at
            && !self.waits_for_operator()
        {
            let (done, attaching) = ferrite_core::doorbell::channel();
            let store = self.attach.store.clone();
            let server = self.attach.server.clone();
            let hello = hello(&self.attach);
            let started = std::thread::Builder::new()
                .name("ferrite-attach".into())
                .spawn(move || {
                    let _ = done.send(attach_to(&store, &server, hello, None));
                });
            if started.is_ok() {
                self.attaching = Some(attaching);
            }
        }
        while let Ok(answer) = self.rendered.1.try_recv() {
            self.say(&answer);
        }
        let mut heard: Vec<ServerMessage> = self.inbox.drain(..).collect();
        let mut gone = false;
        if let Some(attachment) = &self.attachment {
            loop {
                match attachment.client.messages.try_recv() {
                    Ok(message) => heard.push(message),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        gone = true;
                        break;
                    }
                }
            }
        }
        for message in heard {
            arrivals.extend(self.heard(message));
        }
        if gone {
            self.detach("the Ferrite server went away");
        }
        arrivals.append(&mut self.abandoned);
        arrivals
    }

    /// Where this boot's `at_ms` count from.
    fn boot(&self) -> Instant {
        self.attachment
            .as_ref()
            .map_or_else(ferrite_core::clock::instant, |attachment| attachment.boot)
    }

    /// How the Server's wall clock reads on this side's, as last measured.
    fn clock(&self) -> ServerClock {
        self.clock
    }

    /// The machine's bridge, when it is not this computer.
    fn bridge(&self) -> Option<&Arc<Bridge>> {
        match &self.attach.server {
            Endpoint::Local { .. } => None,
            Endpoint::Machine(bridge) => Some(bridge),
        }
    }

    /// Whether a command sent now reaches a Server.
    fn reaches_server(&self) -> bool {
        self.attachment.is_some()
    }

    /// Whether the Server attached now names a fork's owner in its focus
    /// (`Capability::ForkOf`); false while detached.
    fn names_fork_owners(&self) -> bool {
        self.attachment
            .as_ref()
            .is_some_and(|attachment| attachment.names_fork_owners)
    }

    /// One act: sent, and its reply waited for. What the Server said
    /// before the reply waits in the inbox for the next `poll`.
    fn execute(&mut self, op: Op) -> Result<Option<Done>, WireError> {
        #[cfg(any(test, feature = "test-support"))]
        let _waited = waits::Timing::of(&op, true);
        let deadline = Instant::now() + REPLY_WAIT;
        let id = self.send_command(op)?;
        loop {
            let Some(attachment) = &self.attachment else {
                return Err(gone(&self.detached));
            };
            let left = deadline.saturating_duration_since(Instant::now());
            let received = if left.is_zero() {
                Err(RecvTimeoutError::Timeout)
            } else {
                attachment.client.messages.recv_timeout(left)
            };
            match received {
                Ok(ServerMessage::Reply {
                    id: replied,
                    ok,
                    error,
                }) if replied == id => {
                    return match error {
                        Some(error) => Err(error),
                        None => Ok(ok),
                    };
                }
                Ok(message) => self.inbox.push_back(message),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(WireError {
                        code: ErrorCode::Unconfirmed,
                        message: "the Ferrite server did not answer in time".into(),
                    });
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.detach("the Ferrite server went away");
                    return Err(gone(&self.detached));
                }
            }
        }
    }

    /// One act whose reply nobody waits for: `poll` passes it over. Not
    /// sent while detached; the caller tells it again once attached.
    fn tell(&mut self, op: Op) {
        let _ = self.send_command(op);
    }

    /// An ask whose answer is all of the Server's reply, a refusal's code
    /// included (ADR 0041): sent now, and answered as it comes or, without
    /// a reply in `REPLY_WAIT`, that the Server did not answer in time. It
    /// never waits: a send that cannot go is answered at once.
    fn ask_act(&mut self, op: Op, answer: Answer) {
        match self.send_command(op) {
            Ok(id) => {
                self.answers.insert(
                    id,
                    Waiting {
                        answer,
                        by: Some(ferrite_core::clock::instant() + REPLY_WAIT),
                    },
                );
            }
            Err(error) => answer(Err(error)),
        }
    }

    fn stream(&mut self, watched: &[ThreadId], mirrors: &mut super::mirror::Mirrors) {
        self.watch(watched, |thread| mirrors.seq(thread));
        mirrors.watch_only(watched);
    }

    /// A Draft's first send: answered when the Thread starts, in `poll`.
    fn bootstrap(&mut self, draft: DraftId, prompt: &str, op: Op) -> io::Result<Option<ThreadId>> {
        let id = self.send_command(op).map_err(io::Error::from)?;
        self.starting.insert(id, (draft, prompt.to_string()));
        Ok(None)
    }

    fn draft_starting(&self, draft: DraftId) -> bool {
        self.starting
            .values()
            .any(|(starting, _)| *starting == draft)
    }

    /// Take back a first send still starting. True when one was.
    fn cancel_bootstrap(&mut self, draft: DraftId) -> io::Result<bool> {
        let Some(request) = self
            .starting
            .iter()
            .find(|(_, (starting, _))| *starting == draft)
            .map(|(request, _)| *request)
        else {
            return Ok(false);
        };
        self.starting.remove(&request);
        match self.execute(Op::CancelBootstrap { request })? {
            Some(Done::Changed { changed }) => Ok(changed),
            _ => Ok(false),
        }
    }

    /// The Server reads its own settings again.
    fn settings_changed(&mut self, _: &ferrite_core::settings::Settings) {
        self.tell(Op::SettingsChanged);
    }

    fn factory(&mut self, op: Op) -> io::Result<Option<Done>> {
        self.execute(op).map_err(io::Error::from)
    }

    /// Untimed, as `list_dir`: the Server's listing keeps deadlines of its
    /// own, as long as `REPLY_WAIT`.
    fn list_sessions(&mut self, cap: usize) -> super::Ask<ferrite_core::session_file::Found> {
        let asked = self.ask(Op::ListSessions { cap }, |done| match done {
            Done::Sessions { sessions, roots } => Some(ferrite_core::session_file::Found {
                sessions: sessions.into_iter().map(Into::into).collect(),
                roots,
            }),
            _ => None,
        });
        match asked {
            Ok(answer) => super::Ask::of(answer),
            Err(error) => super::Ask::answered(Err(error)),
        }
    }

    fn list_commands(
        &mut self,
        provider: ferrite_core::provider::Provider,
        cwd: &Path,
    ) -> super::Ask<Vec<ferrite_core::SessionCommand>> {
        let op = Op::ListCommands {
            provider,
            cwd: cwd.to_path_buf(),
        };
        let asked = self.ask(op, |done| match done {
            Done::Commands { commands } => Some(commands.iter().map(Into::into).collect()),
            _ => None,
        });
        match asked {
            Ok(answer) => super::Ask::of(answer),
            Err(error) => super::Ask::answered(Err(error)),
        }
    }

    /// Untimed, as `list_commands` is: a long log replays for a while.
    fn read_transcript(&mut self, thread: ThreadId) -> super::Ask<String> {
        let asked = self.ask(Op::ReadTranscript { thread }, |done| match done {
            Done::Text { text } => Some(text),
            _ => None,
        });
        match asked {
            Ok(answer) => super::Ask::of(answer),
            Err(error) => super::Ask::answered(Err(error)),
        }
    }

    fn search_files(
        &mut self,
        thread: ThreadId,
        query: &str,
    ) -> io::Result<Receiver<io::Result<Vec<FileSuggestion>>>> {
        let id = self
            .send_command(Op::SearchFiles {
                thread,
                query: query.to_string(),
            })
            .map_err(io::Error::from)?;
        let (answer, answered) = ferrite_core::doorbell::channel();
        self.searches.insert(id, answer);
        Ok(answered)
    }

    fn upload_attachment(&self, path: PathBuf) -> io::Result<ferrite_core::attachment_transfer::Upload> {
        Ok(ferrite_core::attachment_transfer::upload(path, self.attachment_connection()?))
    }

    fn read_attachment(&self, path: PathBuf) -> io::Result<Receiver<io::Result<Vec<u8>>>> {
        let connect = self.attachment_connection()?;
        Ok(super::read_here(move || {
            ferrite_core::attachment_transfer::read(path, connect().map_err(|error| error.to_string())?).map_err(|error| error.to_string())
        }))
    }

    /// Read as the Server's image of it, the asynchronous Server read
    /// path. The display cache never replaces the Server-owned provider
    /// path.
    fn annotation_image(
        &mut self,
        thread: ThreadId,
        annotation: ferrite_core::browser_annotations::Annotation,
    ) -> super::Ask<PathBuf> {
        let Some(hash) = annotation.original_hash() else {
            return super::Ask::answered(Err(io::Error::other("This annotation has no screenshot")));
        };
        let image = match self.browser_capture_image(thread, hash.to_owned()) {
            Ok(image) => image,
            Err(error) => return super::Ask::answered(Err(error)),
        };
        super::Ask::of(super::read_here(move || {
            let png = image
                .recv_timeout(Duration::from_secs(30))
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?;
            annotation
                .cache_wire_image(&png)
                .map_err(|error| error.to_string())
        }))
    }

    fn changes(&mut self, request: ferrite_core::changes::Request) -> io::Result<Receiver<io::Result<ferrite_core::changes::Snapshot>>> {
        self.ask(Op::Changes { request }, |done| match done {
            Done::Changes { reply } => reply.snapshot(),
            _ => None,
        })
    }

    fn set_file_staged(&mut self, request: ferrite_core::changes::Request) -> io::Result<Receiver<io::Result<()>>> {
        self.ask(Op::Changes { request }, |done| match done {
            Done::Changes { reply: ferrite_core::changes::Reply::Staged } => Some(()),
            _ => None,
        })
    }

    fn file_operation(
        &mut self,
        request: ferrite_core::file_operations::Request,
    ) -> io::Result<Receiver<io::Result<ferrite_core::file_operations::Reply>>> {
        self.ask(Op::FileOperation { request }, |done| match done {
            Done::FileOperation { reply } => Some(reply),
            _ => None,
        })
    }

    fn repositories(&mut self) -> io::Result<Receiver<io::Result<ferrite_core::protocol::Repositories>>> {
        self.ask(Op::Repositories, |done| match done {
            Done::Repositories { projects } => Some(
                projects
                    .into_iter()
                    .map(|known| (known.project, known.repository))
                    .collect(),
            ),
            _ => None,
        })
    }

    fn review_diff(
        &mut self,
        path: &Path,
        read: &Read,
    ) -> io::Result<Receiver<io::Result<Vec<FileSides>>>> {
        let id = self
            .send_command(Op::review_read(path, read))
            .map_err(io::Error::from)?;
        let (answer, answered) = ferrite_core::doorbell::channel();
        self.reviews
            .insert(id, (path.to_path_buf(), read.clone(), answer));
        Ok(answered)
    }

    fn review_commits(
        &mut self,
        path: &Path,
    ) -> io::Result<Receiver<io::Result<Vec<CommitRow>>>> {
        let op = Op::ListCommits {
            path: path.to_path_buf(),
        };
        self.ask(op, |done| match done {
            Done::Commits { commits } => Some(commits.into_iter().map(Into::into).collect()),
            _ => None,
        })
    }

    fn search_repo(
        &mut self,
        path: &Path,
        query: &str,
    ) -> io::Result<Receiver<io::Result<SearchResults>>> {
        let op = Op::SearchRepo {
            path: path.to_path_buf(),
            query: query.to_string(),
        };
        self.ask(op, |done| match done {
            Done::Found {
                files,
                code,
                file_total,
                code_more,
            } => Some(ferrite_core::protocol::search_results(
                files, code, file_total, code_more,
            )),
            _ => None,
        })
    }

    fn read_review_file(
        &mut self,
        path: &Path,
        file: &str,
    ) -> io::Result<Receiver<io::Result<String>>> {
        let op = Op::ReadFile {
            path: path.to_path_buf(),
            file: file.to_string(),
        };
        self.ask(op, |done| match done {
            Done::Text { text } => Some(text),
            _ => None,
        })
    }

    fn review_pull_request_diff(
        &mut self,
        path: &Path,
        number: u64,
    ) -> io::Result<Receiver<io::Result<Vec<FileSides>>>> {
        self.ask(
            Op::ReviewPullRequestDiff {
                path: path.to_path_buf(),
                number,
            },
            |done| match done {
                Done::Review { files } => Some(files.into_iter().map(Into::into).collect()),
                _ => None,
            },
        )
    }

    fn review_pull_request(
        &mut self,
        path: &Path,
        number: Option<u64>,
    ) -> io::Result<Receiver<io::Result<ReviewPullRequest>>> {
        let op = Op::ReviewPullRequest {
            path: path.to_path_buf(),
            number,
        };
        self.ask(op, |done| match done {
            Done::ReviewPullRequest {
                noun,
                pr,
                note,
                unread,
            } => Some(ReviewPullRequest {
                noun,
                pr: pr.ok_or_else(|| note.unwrap_or_default()),
                unread,
            }),
            _ => None,
        })
    }

    fn list_pull_requests(
        &mut self,
        path: &Path,
        filter: WirePrFilter,
    ) -> io::Result<Receiver<io::Result<ReviewPullRequests>>> {
        let op = Op::ListPullRequests {
            path: path.to_path_buf(),
            filter,
        };
        self.ask(op, |done| match done {
            Done::ReviewPullRequests {
                rows,
                more,
                note,
                unread,
            } => Some(ReviewPullRequests {
                list: match note {
                    Some(note) => Err(note),
                    None => Ok(WirePrList { rows, more }),
                },
                unread,
            }),
            _ => None,
        })
    }

    fn review_files(
        &mut self,
        path: &Path,
    ) -> io::Result<Receiver<io::Result<Vec<WorktreeEntry>>>> {
        let op = Op::ListFiles {
            path: path.to_path_buf(),
        };
        self.ask(op, |done| match done {
            Done::AllFiles { entries } => Some(entries.into_iter().map(Into::into).collect()),
            _ => None,
        })
    }

    fn review_bases(&mut self, path: &Path) -> io::Result<Receiver<io::Result<Bases>>> {
        let op = Op::ListBases {
            path: path.to_path_buf(),
        };
        self.ask(op, |done| match done {
            Done::Bases { bases, picked } => Some(ferrite_core::protocol::bases(bases, picked)),
            _ => None,
        })
    }

    fn pick_review_base(
        &mut self,
        path: &Path,
        base: &str,
    ) -> io::Result<Receiver<io::Result<()>>> {
        let op = Op::PickBase {
            path: path.to_path_buf(),
            base: base.to_string(),
        };
        self.ask(op, |done| match done {
            Done::Changed { .. } => Some(()),
            _ => None,
        })
    }

    fn pull_request(
        &mut self,
        path: &Path,
    ) -> io::Result<Receiver<io::Result<Option<PullRequest>>>> {
        let op = Op::PullRequest {
            path: path.to_path_buf(),
        };
        self.ask(op, |done| match done {
            Done::PullRequest { json } => Some(json.as_deref().and_then(parse_pr_json)),
            _ => None,
        })
    }

    /// A `watch_pr`: whether the PR is watched once the Server has it.
    fn watch_pr(&mut self, op: Op) -> io::Result<Receiver<io::Result<bool>>> {
        self.ask(op, |done| match done {
            Done::PrWatch { watching } => Some(watching),
            _ => None,
        })
    }

    /// A `cancel_delegate`: answered once the Server has cancelled it.
    fn cancel_delegate(&mut self, op: Op) -> io::Result<Receiver<io::Result<()>>> {
        self.ask(op, |done| matches!(done, Done::Changed { .. }).then_some(()))
    }

    /// What a GitHub start's picker lists (#225).
    fn github_names(
        &mut self,
        ask: ferrite_core::protocol::GitHubAsk,
    ) -> io::Result<Receiver<io::Result<Vec<String>>>> {
        self.ask(Op::GitHub { ask }, |done| match done {
            Done::Names { names } => Some(names),
            _ => None,
        })
    }

    /// What the Server's machine has of the provider CLIs, or the update it
    /// ran: the Server's `Done`, for the caller to read.
    fn cli(
        &mut self,
        action: ferrite_core::protocol::CliAction,
    ) -> io::Result<Receiver<io::Result<Done>>> {
        self.ask(Op::Cli { action }, Some)
    }

    /// A step of signing the Server's machine's `provider` CLI in: an
    /// older Server answers `unsupported`.
    fn sign_in(
        &mut self,
        provider: ferrite_core::provider::Provider,
        step: ferrite_core::protocol::SignInStep,
    ) -> io::Result<Receiver<io::Result<Done>>> {
        self.ask(Op::SignIn { provider, step }, Some)
    }

    fn remote(&mut self) -> Option<&mut Remote> {
        Some(self)
    }

    fn remote_ref(&self) -> Option<&Remote> {
        Some(self)
    }
}

/// A tree's side connection, and the endpoint that carries it.
struct Routed {
    exchange: ferrite_core::tree::Remote,
    _route: Endpoint,
}

impl ferrite_core::tree::Exchange for Routed {
    fn ask(&mut self, request: ferrite_core::tree::Request) -> io::Result<ferrite_core::tree::Asked> {
        self.exchange.ask(request)
    }

    fn answer(&mut self, asked: ferrite_core::tree::Asked) -> io::Result<ferrite_core::tree::Reply> {
        self.exchange.answer(asked)
    }
}

/// Attach to the Server: this computer's, started first when none runs
/// for `store`, or a machine's through its bridge. `patience`: how long
/// the Server is given to welcome it, if any.
fn attach_to(
    store: &std::path::Path,
    server: &Endpoint,
    hello: ClientMessage,
    patience: Option<Duration>,
) -> io::Result<link::Attached> {
    match server {
        Endpoint::Local { executable } => {
            ferrite_core::server::agent::ensure_running(store, executable)?;
            match patience {
                Some(patience) => link::Client::attach_within(&link::address(store), hello, patience),
                None => link::Client::attach(&link::address(store), hello),
            }
        }
        Endpoint::Machine(bridge) => bridge.attach_within(hello, patience),
    }
}

/// Who this window is, that it draws Visuals when it can, and that it
/// lists the parked Threads its Server publishes (a Server from before
/// every window did sends them only to a window that says so).
fn hello(attach: &Attach) -> ClientMessage {
    let mut capabilities = Vec::new();
    if attach.renderer.is_some() {
        capabilities.push(Capability::RenderVisual);
    }
    if attach.browser {
        capabilities.push(Capability::ConversationBrowser);
        capabilities.push(Capability::BrowserDispatch);
    }
    capabilities.push(Capability::ParkedThreads);
    if attach.driven {
        capabilities.push(Capability::Driven);
    }
    ClientMessage::Hello {
        generation: protocol::GENERATION,
        client: attach.client.clone(),
        capabilities,
    }
}

/// An act in flight when the connection went: the Server may have taken
/// it, and it is never sent again.
fn gone(reason: &str) -> WireError {
    WireError {
        code: ErrorCode::Unconfirmed,
        message: format!("{reason}; this may not have arrived"),
    }
}

/// How long the window's thread spent on each command it sent its Server:
/// waiting for the reply (`execute`), or only sending it (an ask). What the
/// tests read to show which asks hold the window (ADR 0041). It records
/// only between `start` and `take`, and only in tests and test-support
/// builds.
#[cfg(any(test, feature = "test-support"))]
pub mod waits {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use ferrite_core::protocol::Op;

    /// One command: its `op` as the wire names it, whether the window
    /// waited for its reply, and how long the window's thread spent on it.
    #[derive(Debug, Clone)]
    pub struct Wait {
        pub op: String,
        pub waited: bool,
        pub took: Duration,
    }

    static RECORDING: AtomicBool = AtomicBool::new(false);
    static WAITS: Mutex<Vec<Wait>> = Mutex::new(Vec::new());

    /// Record from now on, forgetting what was recorded before.
    pub fn start() {
        WAITS.lock().unwrap_or_else(|e| e.into_inner()).clear();
        RECORDING.store(true, Ordering::SeqCst);
    }

    /// What was recorded since `start`, which stops recording.
    pub fn take() -> Vec<Wait> {
        RECORDING.store(false, Ordering::SeqCst);
        std::mem::take(&mut *WAITS.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Times one command from its making to its drop.
    pub(crate) struct Timing(Option<(String, bool, Instant)>);

    impl Timing {
        pub(crate) fn of(op: &Op, waited: bool) -> Self {
            if !RECORDING.load(Ordering::SeqCst) {
                return Self(None);
            }
            let name = serde_json::to_value(op)
                .ok()
                .and_then(|value| value["op"].as_str().map(str::to_owned))
                .unwrap_or_default();
            Self(Some((name, waited, Instant::now())))
        }
    }

    impl Drop for Timing {
        fn drop(&mut self) {
            if let Some((op, waited, from)) = self.0.take() {
                let took = from.elapsed();
                WAITS
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(Wait { op, waited, took });
            }
        }
    }
}

#[cfg(test)]
mod tests;
