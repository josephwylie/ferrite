//! The automation API (ADR 0027): how a script, the `ferrite` CLI or an
//! agent in one Thread starts, prompts and waits on other Threads. One
//! JSON object per line on a socket of its own beside the windows'
//! (`api.sock`), in Herdr's request shape:
//!
//! - request `{"id":"1","method":"agent.prompt","params":{…}}`
//! - success `{"id":"1","result":{…}}`
//! - failure `{"id":"1","error":{"code":"agent_blocked","message":"…"}}`
//! - event   `{"id":"1","event":{"event":"attention",…}}`, after a
//!   subscription's success, under its id
//!
//! Each operation is declared once, in [`schema`]. A connection may ask
//! several things; a wait holds its own answer, not the connection. A
//! connection that closes ends its waits and subscriptions.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::engine::Engine;
use crate::link::{self, TryClone};
use crate::protocol::{
    self, ClosedBy, ErrorCode, PendingEntry, ServerMessage, WireError, WireHandle,
};
use crate::store::Provider;
use crate::{Attention, ThreadId};

pub mod cli;
pub mod schema;

pub use schema::Method;

/// How long a prompt sent with `wait` may go without its agent seen at
/// work, once its Session is up, before the wait gives up
/// `agent_prompt_stalled` (Herdr's five seconds).
pub const STALL: Duration = Duration::from_secs(5);

/// API connections are numbered apart from windows' (from 1): a Decision's
/// `answered {client}` names either.
const FIRST_CALLER: CallerId = 1 << 32;

/// Where the Server for the Store in `store_dir` takes API connections:
/// beside the windows' socket.
pub fn address(store_dir: &Path) -> PathBuf {
    store_dir.join("api.sock")
}

/// What every Session's agent is told about where it runs: its own Thread,
/// and, where the Server takes API connections, the socket and the binary
/// that speaks to it — the bundle's executable is rarely on `PATH`.
pub fn caller_environment(
    thread: ThreadId,
    socket: Option<&Path>,
) -> Vec<(&'static str, OsString)> {
    let mut env = vec![("FERRITE_THREAD_ID", thread.to_string().into())];
    if let Some(socket) = socket {
        env.push(("FERRITE_SOCKET", socket.into()));
        if let Ok(bin) = std::env::current_exe() {
            env.push(("FERRITE_BIN", bin.into()));
        }
    }
    env
}

// ------------------------------------------------------------- the wire

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Request {
    /// The caller's own name for the request: its answer, and a
    /// subscription's events, carry it back.
    pub id: String,
    #[serde(flatten)]
    pub method: Method,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Response {
    Result { id: String, result: Value },
    Error { id: String, error: ApiError },
    Event { id: String, event: Event },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApiError {
    pub code: Code,
    pub message: String,
}

impl ApiError {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    /// The line is not a request.
    InvalidRequest,
    UnknownMethod,
    /// No open Thread has that id.
    UnknownThread,
    /// The Thread has no pending Decision with that id.
    UnknownDecision,
    /// A Decision waits on the Thread: the prompt was not sent.
    AgentBlocked,
    /// The Thread's Session is starting and already holds a prompt: this
    /// one was not sent.
    AgentStarting,
    /// The agent was not seen at work within [`STALL`] of its prompt. The
    /// prompt may still have arrived.
    AgentPromptStalled,
    /// The caller's timeout ran out. Nothing was undone.
    Timeout,
    /// The Thread closed (parked or deleted) while the wait ran.
    ThreadClosed,
    AlreadyAnswered,
    InvalidAnswer,
    /// The Thread leads no Project's workers.
    NotACoordinator,
    Failed,
    /// The CLI's own: no Server answers at the socket.
    ServerUnavailable,
}

impl From<WireError> for ApiError {
    fn from(error: WireError) -> Self {
        let code = match error.code {
            ErrorCode::UnknownThread => Code::UnknownThread,
            ErrorCode::AlreadyAnswered => Code::AlreadyAnswered,
            ErrorCode::InvalidAnswer => Code::InvalidAnswer,
            _ => Code::Failed,
        };
        Self::new(code, error.message)
    }
}

/// One open Thread, as every answer about one names it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThreadInfo {
    pub thread: ThreadId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub provider: Provider,
    pub attention: Attention,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
}

pub fn thread_info(engine: &Engine, thread: ThreadId) -> Option<ThreadInfo> {
    let view = engine.thread(thread)?;
    Some(ThreadInfo {
        thread,
        title: view.title().map(Into::into),
        provider: view.provider(),
        attention: engine.attention(thread)?,
        cwd: view.workspace().map(|binding| binding.cwd().to_path_buf()),
    })
}

/// `{"thread": …}`: what every operation on one Thread answers.
pub fn thread_result(engine: &Engine, thread: ThreadId) -> Result<Value, ApiError> {
    thread_info(engine, thread)
        .map(|info| json!({ "thread": info }))
        .ok_or_else(unknown_thread)
}

pub fn unknown_thread() -> ApiError {
    ApiError::new(Code::UnknownThread, "no open Thread has that id")
}

/// What a subscription hears.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    ThreadOpened {
        thread: ThreadInfo,
    },
    /// Parked or deleted.
    ThreadClosed {
        thread: ThreadId,
    },
    Attention {
        thread: ThreadId,
        attention: Attention,
    },
    DecisionRequested {
        decision: PendingEntry,
    },
    DecisionClosed {
        thread: ThreadId,
        handle: WireHandle,
        by: ClosedBy,
    },
}

impl Event {
    fn thread(&self) -> ThreadId {
        match self {
            Event::ThreadOpened { thread } => thread.thread,
            Event::ThreadClosed { thread }
            | Event::Attention { thread, .. }
            | Event::DecisionClosed { thread, .. } => *thread,
            Event::DecisionRequested { decision } => decision.thread,
        }
    }
}

/// A line as a request, or the error that answers it: an unknown method
/// is told apart from a malformed request. An id that is not a string (so
/// refused) comes back as its JSON text, for the caller to match.
pub fn decode(line: &[u8]) -> Result<Request, Response> {
    serde_json::from_slice(line).map_err(|error| {
        let value: Value = serde_json::from_slice(line).unwrap_or_default();
        let id = match &value["id"] {
            Value::String(id) => id.clone(),
            Value::Null => String::new(),
            id => id.to_string(),
        };
        let known = value["method"]
            .as_str()
            .map(|method| Method::OPERATIONS.iter().any(|op| op.name == method));
        let code = match known {
            Some(false) => Code::UnknownMethod,
            _ => Code::InvalidRequest,
        };
        Response::Error {
            id,
            error: ApiError::new(code, error.to_string()),
        }
    })
}

// ------------------------------------------------------------ waiting

/// What a wait waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Until {
    /// Idle, done or blocked: anything but working.
    Settled,
    Attention(Attention),
}

impl Until {
    fn reached(self, attention: Attention) -> bool {
        match self {
            Until::Settled => attention != Attention::Working,
            Until::Attention(wanted) => attention == wanted,
        }
    }
}

/// What a wait sees of its Thread on one pass.
#[derive(Debug, Clone, Copy)]
pub struct Seen {
    pub attention: Attention,
    /// Its Session is starting: a prompt sent now is held until it is up.
    pub starting: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settled {
    Reached,
    Stalled,
    TimedOut,
}

/// `agent.wait`, and `agent.prompt` with `wait`, as Herdr waits: a prompt's
/// wait settles only once its agent was seen working (or blocked), so the
/// idle it was sent from never answers it, and it gives up
/// `agent_prompt_stalled` when the agent shows no work within [`STALL`] of
/// its Session being up.
#[derive(Debug, Clone)]
pub struct Wait {
    until: Until,
    deadline: Option<Instant>,
    /// A prompt's: the agent has not yet been seen at work.
    unseen: bool,
    /// When the stall clock started: the first pass the agent was still
    /// unseen with its Session up.
    stall_from: Option<Instant>,
}

impl Wait {
    pub fn until(until: Until, timeout: Option<Duration>, now: Instant) -> Self {
        Self {
            until,
            deadline: timeout.map(|timeout| now + timeout),
            unseen: false,
            stall_from: None,
        }
    }

    pub fn after_prompt(timeout: Option<Duration>, now: Instant) -> Self {
        Self {
            unseen: true,
            ..Self::until(Until::Settled, timeout, now)
        }
    }

    /// One pass: the answer once there is one.
    pub fn observe(&mut self, seen: Seen, now: Instant) -> Option<Settled> {
        if matches!(seen.attention, Attention::Working | Attention::Blocked) {
            self.unseen = false;
        }
        if !self.unseen && self.until.reached(seen.attention) {
            return Some(Settled::Reached);
        }
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            return Some(Settled::TimedOut);
        }
        if self.unseen && !seen.starting {
            let from = *self.stall_from.get_or_insert(now);
            if now.duration_since(from) >= STALL {
                return Some(Settled::Stalled);
            }
        }
        None
    }
}

// ------------------------------------------------------------ transport

/// An API connection's number.
pub type CallerId = u64;

/// What the Server hears from API connections, in arrival order, on the
/// same channel as its windows.
pub enum Heard {
    /// A connection opened: where its answers go, one encoded line each.
    /// Always heard before anything it asks.
    Joined(CallerId, Sender<Vec<u8>>),
    Request(CallerId, Request),
    /// The connection asks nothing more: its reading side closed. It may
    /// still be listening (`echo … | nc -U` half-closes once it has asked).
    Hungup(CallerId),
}

/// Serve every API connection `listener` accepts, for as long as the pump
/// listens.
pub fn accept_all(listener: link::Listener, heard: Sender<link::Heard>) -> io::Result<()> {
    std::thread::Builder::new()
        .name("ferrite-api-accept".into())
        .spawn(move || {
            for caller in FIRST_CALLER.. {
                match listener.accept() {
                    Ok(stream) => {
                        if let Err(error) = serve(caller, stream, heard.clone()) {
                            crate::log!("api: a connection could not be served: {error}");
                        }
                    }
                    Err(error) => {
                        crate::log!("api: stopped accepting: {error}");
                        return;
                    }
                }
            }
        })?;
    Ok(())
}

/// A reader thread that hands each request to the pump (and answers a line
/// that is not one itself), and a writer thread that sends each answer
/// until neither the pump nor the reader holds its channel.
fn serve(caller: CallerId, stream: link::Stream, heard: Sender<link::Heard>) -> io::Result<()> {
    let (answers, outgoing) = mpsc::channel::<Vec<u8>>();
    let mut writer = stream.try_clone()?;
    std::thread::Builder::new()
        .name(format!("ferrite-api-{caller}-out"))
        .spawn(move || {
            for line in outgoing {
                if writer
                    .write_all(&line)
                    .and_then(|()| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
            let _ = writer.shutdown();
        })?;
    if heard
        .send(link::Heard::Api(Heard::Joined(caller, answers.clone())))
        .is_err()
    {
        return Ok(());
    }
    std::thread::Builder::new()
        .name(format!("ferrite-api-{caller}-in"))
        .spawn(move || {
            let mut lines = link::Lines::new(stream);
            while let Ok(Some(line)) = lines.next_line() {
                match decode(&line) {
                    Ok(request) => {
                        if heard
                            .send(link::Heard::Api(Heard::Request(caller, request)))
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(refusal) => {
                        let _ = answers.send(protocol::encode(&refusal));
                    }
                }
            }
            let _ = heard.send(link::Heard::Api(Heard::Hungup(caller)));
        })?;
    Ok(())
}

// ---------------------------------------------------------- the Server's

struct Waiting {
    caller: CallerId,
    id: String,
    thread: ThreadId,
    wait: Wait,
}

struct Subscription {
    caller: CallerId,
    id: String,
    /// Every Thread's when empty.
    threads: Vec<ThreadId>,
}

impl Subscription {
    fn wants(&self, thread: ThreadId) -> bool {
        self.threads.is_empty() || self.threads.contains(&thread)
    }
}

/// The Server's side of the API connections: who is connected, the waits
/// still running and the subscriptions, and what the subscribers were last
/// told, so each pass tells them only what changed.
#[derive(Default)]
pub struct Api {
    callers: BTreeMap<CallerId, Sender<Vec<u8>>>,
    waits: Vec<Waiting>,
    subscriptions: Vec<Subscription>,
    open: Vec<ThreadId>,
    attention: BTreeMap<ThreadId, Attention>,
    /// Callers that ask nothing more, kept while a wait or subscription
    /// still owes them lines.
    hung_up: BTreeSet<CallerId>,
}

impl Api {
    pub fn join(&mut self, caller: CallerId, answers: Sender<Vec<u8>>) {
        self.callers.insert(caller, answers);
    }

    /// The connection asks nothing more: it is let go once nothing it
    /// asked still owes it an answer. A subscription keeps it until an
    /// event cannot be written.
    pub fn hang_up(&mut self, caller: CallerId) {
        self.hung_up.insert(caller);
        self.release_hung_up();
    }

    fn release_hung_up(&mut self) {
        let owed = |caller: &CallerId| {
            self.waits.iter().any(|waiting| waiting.caller == *caller)
                || self
                    .subscriptions
                    .iter()
                    .any(|subscription| subscription.caller == *caller)
        };
        let done: Vec<CallerId> = self
            .hung_up
            .iter()
            .copied()
            .filter(|caller| !owed(caller))
            .collect();
        for caller in done {
            self.leave(caller);
        }
    }

    /// The connection is gone: so are its waits and subscriptions.
    fn leave(&mut self, caller: CallerId) {
        self.callers.remove(&caller);
        self.hung_up.remove(&caller);
        self.waits.retain(|waiting| waiting.caller != caller);
        self.subscriptions
            .retain(|subscription| subscription.caller != caller);
    }

    pub fn reply(&mut self, caller: CallerId, id: String, result: Result<Value, ApiError>) {
        let response = match result {
            Ok(result) => Response::Result { id, result },
            Err(error) => Response::Error { id, error },
        };
        self.send(caller, &response);
    }

    fn send(&mut self, caller: CallerId, response: &Response) {
        let sent = self
            .callers
            .get(&caller)
            .is_some_and(|answers| answers.send(protocol::encode(response)).is_ok());
        if !sent {
            self.leave(caller);
        }
    }

    /// Start a wait on an open Thread, looking at it at once: a prompt just
    /// delivered shows its agent working now, and a turn could end before
    /// the next pass sees it.
    pub fn wait(
        &mut self,
        engine: &Engine,
        caller: CallerId,
        id: String,
        thread: ThreadId,
        wait: Wait,
    ) {
        let mut waiting = Waiting {
            caller,
            id,
            thread,
            wait,
        };
        match settle(engine, &mut waiting, Instant::now()) {
            Some(result) => self.reply(caller, waiting.id, result),
            None => self.waits.push(waiting),
        }
    }

    /// Subscribe, then answer with the Threads and Decisions as they are:
    /// subscribed before the snapshot is read, so no change after it is
    /// missed (one may be told twice).
    pub fn subscribe(
        &mut self,
        engine: &Engine,
        caller: CallerId,
        id: String,
        threads: Vec<ThreadId>,
    ) {
        let subscription = Subscription {
            caller,
            id: id.clone(),
            threads,
        };
        let infos: Vec<ThreadInfo> = engine
            .open_threads()
            .iter()
            .filter(|thread| subscription.wants(**thread))
            .filter_map(|thread| thread_info(engine, *thread))
            .collect();
        let decisions: Vec<PendingEntry> = engine
            .pending_decisions()
            .into_iter()
            .filter(|pending| subscription.wants(pending.thread))
            .collect();
        self.subscriptions.push(subscription);
        self.reply(
            caller,
            id,
            Ok(json!({ "threads": infos, "decisions": decisions })),
        );
    }

    /// A Decision the windows are told of, told to the subscribers too.
    pub fn relay(&mut self, message: &ServerMessage) {
        let event = match message {
            ServerMessage::Decision {
                thread,
                subject,
                handle,
                decision,
            } => Event::DecisionRequested {
                decision: PendingEntry {
                    thread: *thread,
                    subject: subject.clone(),
                    handle: handle.clone(),
                    decision: decision.clone(),
                },
            },
            ServerMessage::DecisionClosed { thread, handle, by } => Event::DecisionClosed {
                thread: *thread,
                handle: handle.clone(),
                by: by.clone(),
            },
            _ => return,
        };
        self.publish(event);
    }

    /// One pass, after the Engine's: tell the subscribers which Threads
    /// opened, closed or changed Attention, and answer the waits that
    /// settled.
    pub fn pump(&mut self, engine: &Engine) {
        let open = engine.open_threads().to_vec();
        let mut events: Vec<Event> = open
            .iter()
            .filter(|thread| !self.open.contains(thread))
            .filter_map(|thread| thread_info(engine, *thread))
            .map(|thread| Event::ThreadOpened { thread })
            .collect();
        events.extend(
            self.open
                .iter()
                .filter(|thread| !open.contains(thread))
                .map(|thread| Event::ThreadClosed { thread: *thread }),
        );
        self.attention.retain(|thread, _| open.contains(thread));
        for &thread in &open {
            let Some(attention) = engine.attention(thread) else {
                continue;
            };
            if self
                .attention
                .insert(thread, attention)
                .is_some_and(|before| before != attention)
            {
                events.push(Event::Attention { thread, attention });
            }
        }
        self.open = open;
        for event in events {
            self.publish(event);
        }
        let now = Instant::now();
        for mut waiting in std::mem::take(&mut self.waits) {
            match settle(engine, &mut waiting, now) {
                Some(result) => self.reply(waiting.caller, waiting.id, result),
                None => self.waits.push(waiting),
            }
        }
        self.release_hung_up();
    }

    fn publish(&mut self, event: Event) {
        let thread = event.thread();
        let told: Vec<(CallerId, String)> = self
            .subscriptions
            .iter()
            .filter(|subscription| subscription.wants(thread))
            .map(|subscription| (subscription.caller, subscription.id.clone()))
            .collect();
        for (caller, id) in told {
            self.send(
                caller,
                &Response::Event {
                    id,
                    event: event.clone(),
                },
            );
        }
    }
}

/// A wait's answer, once it has one.
fn settle(engine: &Engine, waiting: &mut Waiting, now: Instant) -> Option<Result<Value, ApiError>> {
    let (Some(view), Some(attention)) = (
        engine.thread(waiting.thread),
        engine.attention(waiting.thread),
    ) else {
        return Some(Err(ApiError::new(
            Code::ThreadClosed,
            "the Thread closed while the wait ran",
        )));
    };
    let seen = Seen {
        attention,
        starting: view.starting(),
    };
    Some(match waiting.wait.observe(seen, now)? {
        Settled::Reached => thread_result(engine, waiting.thread),
        Settled::Stalled => Err(ApiError::new(
            Code::AgentPromptStalled,
            "the agent showed no work within 5 s of its prompt; the prompt may still \
             have arrived: read the Thread before sending it again",
        )),
        Settled::TimedOut => Err(ApiError::new(
            Code::Timeout,
            "the timeout ran out; this does not mean the prompt was not delivered",
        )),
    })
}

#[cfg(test)]
mod tests;
