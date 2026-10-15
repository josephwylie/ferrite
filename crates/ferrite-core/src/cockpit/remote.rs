//! A window's link to its Server (ADR 0025): the socket, the acts as
//! commands, and staying attached. An act waits for its reply, reading
//! what the Server says before it into the inbox, so the window is up to
//! date when the act returns. A first send and a file search answer later.
//! When the Server goes, the window keeps what it drew and attaches again
//! in the background, resuming each Thread where it left off.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;

use crate::engine::{BootstrapResult, Bootstrapped};
use crate::link;
use crate::machine::{self, Bridge, Failure};
use crate::protocol::{
    self, Capability, ClientMessage, Command, Done, ErrorCode, Op, Peer, Resume, ServerInfo,
    ServerMessage, Snapshot, WireError, WirePng,
};
use crate::providers::FileSuggestion;
use crate::review::git::{Commit, Diff};
use crate::roster::DraftId;
use crate::visual::{RenderRequest, Renderer};
use crate::workspace::registry::ProjectId;
use crate::workspace::{
    parse_pr_json, DirEntry, ListedWorktree, Listing, PullRequest, RepoListing,
};
use crate::{FileEdit, ThreadId};

/// Where a later-answered command's reply goes (`Remote::ask`).
type Answer = Box<dyn FnOnce(io::Result<Option<Done>>) + Send>;

/// How long an act waits for the Server's reply.
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
    /// it, which the window reads parked Threads from. A machine's parked
    /// Threads are what its Server publishes (ADR 0026); its path only
    /// names the window's copies of the machine's Groups and Projects.
    pub store: PathBuf,
    pub server: Endpoint,
    pub client: Peer,
    /// Draws the Visuals the Server's agents ask for; none, and this window
    /// does not offer to.
    pub renderer: Option<Arc<dyn Renderer>>,
    /// What the window finds on its own: CLI session files to adopt, a
    /// workspace's commands. It spawns no Session.
    pub discovery: Box<dyn crate::engine::Spawner>,
}

/// Where the Server is.
#[derive(Clone)]
pub enum Endpoint {
    /// This computer's, started as `<executable> server` when none answers.
    Local { executable: PathBuf },
    /// A machine's, through its bridge (ADR 0026). Never started from here.
    Machine(Arc<Bridge>),
}

/// What a window knows of its Server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerState {
    Attached {
        server: ServerInfo,
        /// When the Server started, on this side's clock.
        since: Instant,
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
}

/// The open connection.
struct Attachment {
    client: link::Client,
    boot_id: String,
    server: ServerInfo,
    /// The Server's boot on this side's clock: `at_ms` counts from it.
    boot: Instant,
}

pub(crate) struct Remote {
    attach: Attach,
    attachment: Option<Attachment>,
    /// Why there is no attachment, while there is none.
    detached: String,
    /// The last attach failed in a way only the operator can fix.
    attention: Option<Failure>,
    /// An attach running in the background.
    attaching: Option<Receiver<io::Result<link::Attached>>>,
    retry_at: Instant,
    backoff: Duration,
    /// The boot the window's mirrors were built from.
    last_boot: Option<String>,
    next_request: u64,
    /// What the Server said while an act waited for its reply.
    inbox: VecDeque<ServerMessage>,
    /// First sends in flight, by command.
    starting: BTreeMap<u64, (DraftId, String)>,
    /// First sends the connection took with it, for the next `poll`.
    abandoned: Vec<Arrival>,
    searches: BTreeMap<u64, Sender<io::Result<Vec<FileSuggestion>>>>,
    /// Commands whose reply the window takes later, each with where its
    /// answer goes (`ask`).
    answers: BTreeMap<u64, Answer>,
    watching: BTreeSet<ThreadId>,
    /// Visuals drawn here, answered from the renderer's threads.
    rendered: (Sender<ClientMessage>, Receiver<ClientMessage>),
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
            detached,
            attention: None,
            attaching: None,
            retry_at: Instant::now(),
            backoff: RETRY_FIRST,
            last_boot: None,
            next_request: 0,
            inbox: VecDeque::new(),
            starting: BTreeMap::new(),
            abandoned: Vec::new(),
            searches: BTreeMap::new(),
            answers: BTreeMap::new(),
            watching: BTreeSet::new(),
            rendered: mpsc::channel(),
        }
    }

    /// Attach now, waiting for it: when a Server answers or can be
    /// started. Otherwise the window stays detached and keeps trying.
    pub(crate) fn attach_now(&mut self) -> Option<Arrival> {
        let attached = attach_to(&self.attach.store, &self.attach.server, hello(&self.attach));
        self.attached(attached)
    }

    pub(crate) fn state(&self) -> ServerState {
        match &self.attachment {
            Some(attachment) => ServerState::Attached {
                server: attachment.server.clone(),
                since: attachment.boot,
            },
            None => ServerState::Detached {
                reason: self.detached.clone(),
                attention: self.attention.clone(),
            },
        }
    }

    /// Where this boot's `at_ms` count from.
    pub(crate) fn boot(&self) -> Instant {
        self.attachment
            .as_ref()
            .map_or_else(crate::clock::instant, |attachment| attachment.boot)
    }

    /// Attach again now rather than at the next retry.
    pub(crate) fn retry(&mut self) {
        self.retry_at = Instant::now();
        self.backoff = RETRY_FIRST;
        self.attention = None;
    }

    /// The machine this window is on, when it is not this computer.
    pub(crate) fn bridge(&self) -> Option<&Arc<Bridge>> {
        match &self.attach.server {
            Endpoint::Local { .. } => None,
            Endpoint::Machine(bridge) => Some(bridge),
        }
    }

    /// Whether attaching waits for the operator: a machine that needs a
    /// sign-in is not asked again (each ask is a refused login on it) until
    /// they have signed in, or ask.
    fn waits_for_operator(&mut self) -> bool {
        if self.attention != Some(Failure::SignIn) {
            return false;
        }
        if self.bridge().is_some_and(|bridge| bridge.take_signed_in()) {
            self.retry();
            return false;
        }
        true
    }

    /// Everything that arrived since the last call, attaching again first
    /// when the Server is gone and it is time.
    pub(crate) fn poll(&mut self) -> Vec<Arrival> {
        let mut arrivals = Vec::new();
        if let Some(attaching) = &self.attaching {
            match attaching.try_recv() {
                Ok(attached) => {
                    self.attaching = None;
                    arrivals.extend(self.attached(attached));
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.attaching = None,
            }
        }
        if self.attachment.is_none()
            && self.attaching.is_none()
            && Instant::now() >= self.retry_at
            && !self.waits_for_operator()
        {
            let (done, attaching) = mpsc::channel();
            let store = self.attach.store.clone();
            let server = self.attach.server.clone();
            let hello = hello(&self.attach);
            let started = std::thread::Builder::new()
                .name("ferrite-attach".into())
                .spawn(move || {
                    let _ = done.send(attach_to(&store, &server, hello));
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

    /// One act: sent, and its reply waited for. What the Server said
    /// before the reply waits in the inbox for the next `poll`.
    pub(crate) fn execute(&mut self, op: Op) -> Result<Option<Done>, WireError> {
        let id = self.send_command(op)?;
        loop {
            let Some(attachment) = &self.attachment else {
                return Err(gone(&self.detached));
            };
            match attachment.client.messages.recv_timeout(REPLY_WAIT) {
                Ok(ServerMessage::Reply {
                    id: replied,
                    ok,
                    error,
                }) if replied == id => {
                    return match error {
                        Some(error) => Err(error),
                        None => Ok(ok),
                    }
                }
                Ok(message) => self.inbox.push_back(message),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(WireError {
                        code: ErrorCode::Failed,
                        message: "the Ferrite server did not answer in time".into(),
                    })
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
    pub(crate) fn tell(&mut self, op: Op) {
        let _ = self.send_command(op);
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
                    return Ok(())
                }
                _ if Instant::now() >= deadline => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "the Ferrite server is still running",
                    ))
                }
                _ => std::thread::sleep(Duration::from_millis(25)),
            }
        }
    }

    /// A Draft's first send: answered when the Thread starts, in `poll`.
    pub(crate) fn bootstrap(&mut self, draft: DraftId, prompt: &str, op: Op) -> io::Result<()> {
        let id = self.send_command(op).map_err(io::Error::from)?;
        self.starting.insert(id, (draft, prompt.to_string()));
        Ok(())
    }

    pub(crate) fn discovery(&mut self) -> &mut dyn crate::engine::Spawner {
        self.attach.discovery.as_mut()
    }

    pub(crate) fn draft_starting(&self, draft: DraftId) -> bool {
        self.starting
            .values()
            .any(|(starting, _)| *starting == draft)
    }

    /// Take back a first send still starting. True when one was.
    pub(crate) fn cancel_bootstrap(&mut self, draft: DraftId) -> io::Result<bool> {
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

    pub(crate) fn search_files(
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
        let (answer, answered) = mpsc::channel();
        self.searches.insert(id, answer);
        Ok(answered)
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

    pub(crate) fn review_diff(
        &mut self,
        path: &Path,
        diff: &Diff,
    ) -> io::Result<Receiver<io::Result<Vec<FileEdit>>>> {
        self.ask(Op::review_diff(path, diff), |done| match done {
            Done::Diff { text } => Some(crate::review::git::parse(&text)),
            _ => None,
        })
    }

    pub(crate) fn review_commits(
        &mut self,
        path: &Path,
    ) -> io::Result<Receiver<io::Result<Vec<Commit>>>> {
        let op = Op::ListCommits {
            path: path.to_path_buf(),
        };
        self.ask(op, |done| match done {
            Done::Commits { commits } => Some(commits.into_iter().map(Into::into).collect()),
            _ => None,
        })
    }

    pub(crate) fn pull_request(
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

    /// Send `op` and answer its reply later, as `read` makes of it.
    fn ask<T: Send + 'static>(
        &mut self,
        op: Op,
        read: fn(Done) -> Option<T>,
    ) -> io::Result<Receiver<io::Result<T>>> {
        let id = self.send_command(op).map_err(io::Error::from)?;
        let (answer, answered) = mpsc::channel();
        self.answers.insert(
            id,
            Box::new(move |result| {
                let read = result.and_then(|done| {
                    done.and_then(read)
                        .ok_or_else(|| io::Error::other("the server answered something else"))
                });
                let _ = answer.send(read);
            }),
        );
        Ok(answered)
    }

    /// Stream every open Thread to this window: from where its mirror left
    /// off when the boot is the same, else from its history.
    pub(crate) fn watch(&mut self, open: &[ThreadId], seq: impl Fn(ThreadId) -> Option<u64>) {
        let Some(attachment) = &self.attachment else {
            return;
        };
        self.watching.retain(|thread| open.contains(thread));
        let boot_id = attachment.boot_id.clone();
        for &thread in open {
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

    fn say(&mut self, message: &ClientMessage) {
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
            server, uptime_ms, ..
        } = welcome
        else {
            return None;
        };
        let now = crate::clock::instant();
        let boot = now
            .checked_sub(Duration::from_millis(uptime_ms.unwrap_or(0)))
            .unwrap_or(now);
        let new_boot = self.last_boot.as_ref() != Some(&snapshot.boot_id);
        self.last_boot = Some(snapshot.boot_id.clone());
        self.attachment = Some(Attachment {
            client,
            boot_id: snapshot.boot_id.clone(),
            server,
            boot,
        });
        self.backoff = RETRY_FIRST;
        self.attention = None;
        self.watching.clear();
        crate::log!("attached to the Ferrite server, boot {}", snapshot.boot_id);
        Some(Arrival::Attached { snapshot, new_boot })
    }

    /// The connection is gone: what was in flight is answered here, since
    /// the Server will not answer it. Nothing is sent again.
    fn detach(&mut self, reason: &str) {
        if self.attachment.take().is_none() {
            return;
        }
        crate::log!("{reason}");
        self.detached = reason.to_string();
        self.retry_at = Instant::now();
        self.watching.clear();
        self.searches.clear();
        self.answers.clear();
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
                let result = match error {
                    Some(error) => Err(io::Error::from(error)),
                    None => Ok(ok),
                };
                if let Some((draft, prompt)) = self.starting.remove(&id) {
                    let result = result.and_then(|done| match done {
                        Some(Done::Thread { thread }) => Ok(Bootstrapped {
                            thread,
                            refused_leave: None,
                            applied_leave: false,
                        }),
                        _ => Err(io::Error::other("the server started no Thread")),
                    });
                    return vec![Arrival::Started(BootstrapResult {
                        draft: Some(draft),
                        prompt,
                        result,
                    })];
                }
                if let Some(answer) = self.searches.remove(&id) {
                    let _ = answer.send(result.map(|done| {
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
                } else if let Some(answer) = self.answers.remove(&id) {
                    answer(result);
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
        let (reply, drawn) = crate::visual::reply();
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
            crate::log!("a visual could not be answered: {error}");
        }
    }
}

/// Attach to the Server: this computer's, started first when none runs
/// for `store`, or a machine's through its bridge.
fn attach_to(
    store: &std::path::Path,
    server: &Endpoint,
    hello: ClientMessage,
) -> io::Result<link::Attached> {
    match server {
        Endpoint::Local { executable } => {
            crate::server::agent::ensure_running(store, executable)?;
            link::Client::attach(&link::address(store), hello)
        }
        Endpoint::Machine(bridge) => bridge.attach(hello),
    }
}

/// Who this window is, that it draws Visuals when it can, and that on a
/// machine it lists the parked Threads its Server publishes.
fn hello(attach: &Attach) -> ClientMessage {
    let mut capabilities = Vec::new();
    if attach.renderer.is_some() {
        capabilities.push(Capability::RenderVisual);
    }
    if matches!(attach.server, Endpoint::Machine(_)) {
        capabilities.push(Capability::ParkedThreads);
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
        code: ErrorCode::Failed,
        message: format!("{reason}; this may not have arrived"),
    }
}

impl From<WireError> for io::Error {
    fn from(error: WireError) -> Self {
        io::Error::other(error.message)
    }
}

#[cfg(test)]
mod tests;
