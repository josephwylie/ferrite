//! Codex provider: the pinned `codex` CLI's app-server spoken over stdio
//! JSON-RPC.
//!
//! Spawn checks the CLI version pin, then holds a two-request handshake —
//! initialize, then thread/start (or thread/resume) — before any Session
//! exists: a Codex Session without a thread id cannot say anything, so unlike
//! Claude a failed handshake is a typed spawn error, not a half-alive
//! Session. A reader thread parses stdout lines into SessionEvents on a
//! bounded channel; backpressure is stated and simple: when the channel is
//! full the reader blocks, the pipe fills, and the server stalls — nothing is
//! dropped.

mod activity;
pub(super) mod catalog;
mod live_catalogs;
mod questions;
mod requests;
mod wire;

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::thread;
use std::time::{Duration, Instant};

use crate::{DecisionAnswer, SessionEvent};

use wire::ThreadHandshake;

/// Minimum `codex` CLI version for the app-server wire the fixtures record.
/// Vendor releases below this break loudly at spawn, not weirdly mid-turn.
pub const CODEX_CLI_MIN_VERSION: [u64; 3] = [0, 149, 1];

/// Exclusive ceiling: Ferrite is proven against the 0.x wire, and a new major
/// is a new protocol until someone re-runs the fixture captures against it.
pub const CODEX_CLI_MAX_VERSION_EXCLUSIVE: [u64; 3] = [1, 0, 0];

/// The supported window as it is shown to operators.
const MIN_VERSION_DISPLAY: &str = "0.149.1";
const MAX_VERSION_DISPLAY: &str = "1.0.0";

/// One frame of UI drains far less than this; the depth exists so a stalled
/// frame throttles the server instead of losing its output.
const EVENT_CHANNEL_CAPACITY: usize = 1024;

/// Enough stderr to explain a crash, bounded so a chatty server cannot grow
/// memory for the life of a Session.
const STDERR_TAIL_LINES: usize = 20;

/// How long spawn waits for each of its two handshake responses. Measured
/// against `codex` 0.149.1, which answers both well under a second from a
/// cold start; the budget is generous because overrunning it fails the spawn.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// A resume can emit turn notifications before its response identifies Main.
/// Bound those candidates; overflow stays non-interruptible until new evidence.
const EARLY_TURN_THREAD_LIMIT: usize = 64;

/// The request id spawn numbers its skills/list with — always the request
/// after the two handshake steps, which is what lets the reader correlate
/// the answer without a shared table (#23).
const SKILLS_REQUEST_ID: u64 = 3;

/// And the one after it: the model/list the picker's rows come from. Sent
/// after the thread is up rather than between initialize and thread/start
/// so the handshake's own ids (1, 2) stay what every committed capture
/// answers.
const MODELS_REQUEST_ID: u64 = 4;

/// How to spawn a Codex Session.
#[derive(Debug, Clone)]
pub struct CodexConfig {
    /// Program to exec. Tests point this at a stub binary.
    pub program: String,
    /// Working directory for the thread (the Thread's workspace binding),
    /// passed in thread/start rather than inherited from the process.
    pub cwd: Option<PathBuf>,
    /// Model override passed through in thread/start.
    pub model: Option<String>,
    /// Reasoning effort (`"low"` … `"xhigh"`, `"max"`, `"ultra"` where the
    /// model takes it), passed on turn/start. `None` leaves the server's own
    /// default; `capabilities().reasoning_effort` reports that default.
    pub effort: Option<String>,
    /// Approval posture for this Thread (`"untrusted"`, `"on-request"`,
    /// `"never"`). `None` leaves the server's own configuration alone — which
    /// on a machine configured to never ask means no Decision will ever
    /// arrive. `capabilities().approval_policy` reports what took effect.
    pub approval_policy: Option<String>,
    /// Sandbox for tool runs (`"read-only"`, `"workspace-write"`,
    /// `"danger-full-access"`). `None` keeps the server's configuration;
    /// `capabilities().sandbox` reports what took effect.
    pub sandbox: Option<String>,
    /// Resume this provider-native thread id (from a previous Session's
    /// `Init`) instead of starting a fresh thread: the server reloads the
    /// conversation from its own rollout files.
    pub resume: Option<String>,
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            program: "codex".into(),
            cwd: None,
            model: None,
            effort: None,
            approval_policy: None,
            sandbox: None,
            resume: None,
        }
    }
}

/// Spawn failed before a Session existed.
#[derive(Debug)]
pub enum CodexSpawnError {
    /// The CLI program was not found on this machine.
    CliNotFound {
        program: String,
    },
    /// The CLI is older than the pin. The operator upgrades the CLI.
    CliVersionUnmet {
        found: String,
        required: &'static str,
    },
    /// The CLI is a major release beyond what Ferrite has been proven against.
    /// The operator upgrades Ferrite — the CLI is fine.
    CliVersionUnsupported {
        found: String,
        supported_below: &'static str,
    },
    /// `--version` ran but produced nothing parseable.
    VersionCheckFailed {
        detail: String,
    },
    /// The server did not complete the initialize/thread-start handshake: it
    /// answered with an error, answered nonsense, exited, or timed out. A
    /// Session with no thread cannot speak, so this fails spawn instead of
    /// handing back something mute.
    HandshakeFailed {
        detail: String,
    },
    Io(io::Error),
}

impl std::fmt::Display for CodexSpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodexSpawnError::CliNotFound { program } => {
                write!(f, "codex CLI not found: `{program}`")
            }
            CodexSpawnError::CliVersionUnmet { found, required } => {
                write!(
                    f,
                    "codex CLI {found} is older than the pinned minimum {required}; \
                     upgrade the CLI"
                )
            }
            CodexSpawnError::CliVersionUnsupported {
                found,
                supported_below,
            } => {
                write!(
                    f,
                    "codex CLI {found} is a newer major release than Ferrite is proven \
                     against (below {supported_below}); upgrade Ferrite"
                )
            }
            CodexSpawnError::VersionCheckFailed { detail } => {
                write!(f, "codex CLI version check failed: {detail}")
            }
            CodexSpawnError::HandshakeFailed { detail } => {
                write!(f, "codex app-server handshake failed: {detail}")
            }
            CodexSpawnError::Io(e) => write!(f, "io error spawning codex CLI: {e}"),
        }
    }
}

impl std::error::Error for CodexSpawnError {}

/// What the thread/start response answered: feature detection, so a Pane
/// never offers what this install cannot do. Every field is the server's own
/// word — Codex has no dollar cost, no thinking stream, no input-editing on
/// approvals, and nothing here pretends otherwise.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CodexCapabilities {
    /// The model actually serving the thread.
    pub model: String,
    /// Which backend serves it (`"openai"`, or a configured alternative).
    pub model_provider: String,
    /// The approval policy in force. `"never"` means no Decision will ever
    /// arrive; every other value means they can.
    pub approval_policy: String,
    /// The sandbox policy's own tag: `"readOnly"`, `"workspaceWrite"` or
    /// `"dangerFullAccess"`.
    pub sandbox: String,
    /// The reasoning effort in force, when the server states one.
    pub reasoning_effort: Option<String>,
}

/// A live Codex Session: one app-server process serving one Thread.
pub struct CodexSession {
    child: Arc<Mutex<Child>>,
    /// The Session's whole process tree. An npm `.cmd` install makes `child`
    /// cmd.exe with the real CLI beneath it; the job is how kill reaches the
    /// CLI and how the watchdog learns which pid to meter.
    #[cfg(windows)]
    job: super::job::SessionJob,
    /// Held open for the life of the Session: closing it ends the Session,
    /// so multi-turn depends on this staying alive.
    stdin: Arc<Mutex<ChildStdin>>,
    events: Receiver<SessionEvent>,
    capabilities: CodexCapabilities,
    thread_id: String,
    model: String,
    model_override: Option<String>,
    effort: Option<String>,
    models: Arc<Mutex<Vec<crate::ModelInfo>>>,
    /// Ordinary host requests and Main's native interrupt target. Child turns
    /// must never become this Session's interrupt target.
    requests: Arc<Mutex<requests::Requests>>,
    /// The server's skills, filled by the reader from the skills/list answer
    /// (#23). `send` translates a leading `/name` against this list into the
    /// typed skill item — slash text is never intercepted server-side.
    skills: Arc<Mutex<Vec<crate::SessionCommand>>>,
    /// The thread's cwd, kept for resolving `@path` mention tokens.
    cwd: Option<PathBuf>,
    next_request_id: u64,
    question_replies: Arc<Mutex<questions::Replies>>,
    native_questions: Arc<Mutex<questions::NativeRequests>>,
}

impl CodexSession {
    /// Version-check the CLI, spawn its app-server, and hold the handshake:
    /// initialize, initialized, then thread/start — or thread/resume when the
    /// config names a thread to pick back up.
    pub fn spawn(config: CodexConfig) -> Result<Self, CodexSpawnError> {
        // On Windows an npm install is a `codex.cmd` shim a bare name
        // cannot exec; everything spawns through the resolved answer.
        let program = super::spawnable_program(&config.program);
        check_version(&program)?;

        let mut command = Command::new(&program);
        command.arg("app-server");
        if let Some(cwd) = &config.cwd {
            // The thread's cwd travels in thread/start; the process gets the
            // same one so anything the server resolves against itself agrees.
            command.current_dir(cwd);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| spawn_error(&program, e))?;

        // Into the job as CreateProcess returns — in practice before a
        // `.cmd` shim's cmd.exe has executed a line, though nothing suspends
        // the child, so a CLI it somehow started first would sit outside the
        // job (accepted residual risk; airtight needs CREATE_SUSPENDED,
        // which std does not expose). A Session whose kill cannot work is
        // refused.
        #[cfg(windows)]
        let job =
            super::job::SessionJob::assign_or_reap(&mut child).map_err(CodexSpawnError::Io)?;

        let stdin = Arc::new(Mutex::new(child.stdin.take().expect("stdin was piped")));
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        let stderr_tail = Arc::new(Mutex::new(StderrTail::default()));
        drain_stderr(stderr, Arc::clone(&stderr_tail));

        let (sender, events) = sync_channel(EVENT_CHANNEL_CAPACITY);
        let child = Arc::new(Mutex::new(child));
        let requests = Arc::new(Mutex::new(requests::Requests::default()));
        let skills = Arc::new(Mutex::new(Vec::new()));
        let models = Arc::new(Mutex::new(Vec::new()));
        let question_replies = Arc::new(Mutex::new(questions::Replies::default()));
        let native_questions = Arc::new(Mutex::new(questions::NativeRequests::default()));
        let handshake = read_stdout(
            stdout,
            Arc::downgrade(&stdin),
            sender,
            Arc::clone(&child),
            Arc::clone(&stderr_tail),
            Arc::clone(&requests),
            Arc::clone(&skills),
            Arc::clone(&models),
            Arc::clone(&question_replies),
            Arc::clone(&native_questions),
            config.cwd.clone(),
        );

        let mut session = Self {
            child,
            #[cfg(windows)]
            job,
            stdin,
            events,
            capabilities: CodexCapabilities::default(),
            thread_id: String::new(),
            model: String::new(),
            model_override: None,
            effort: config.effort.clone(),
            models,
            requests,
            skills,
            cwd: config.cwd.clone(),
            next_request_id: 1,
            question_replies,
            native_questions,
        };

        // The handshake, in the server's required order. A failed one must
        // not leak a live process: kill it and fold whatever it said on
        // stderr into the explanation.
        session.handshake(&config, &handshake).map_err(|detail| {
            let mut child = lock(&session.child);
            let _ = child.kill();
            let _ = child.wait();
            let stderr = settled_stderr(&stderr_tail);
            CodexSpawnError::HandshakeFailed {
                detail: if stderr.is_empty() {
                    detail
                } else {
                    format!("{detail}\nstderr: {}", stderr.join("\n"))
                },
            }
        })?;
        // Ask for the `/` menu (#23) — after the handshake, before the
        // operator can speak. The answer arrives on the reader's own thread
        // and is announced as `SessionEvent::Commands`; a write failure here
        // is a server already dying, which the reader is turning into a
        // Closed event, so the Session is still handed back.
        let id = session.take_request_id();
        debug_assert_eq!(id, SKILLS_REQUEST_ID);
        let mut params = serde_json::json!({});
        if let Some(cwd) = &session.cwd {
            params["cwds"] = serde_json::json!([cwd.display().to_string()]);
        }
        let _ = session.write_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "skills/list",
            "params": params,
        }));
        // And the model menu (#25), answered the same way and announced as
        // `SessionEvent::Models`; a server without the method, or one that
        // never answers, just leaves the picker on the fallback catalog.
        let id = session.take_request_id();
        debug_assert_eq!(id, MODELS_REQUEST_ID);
        let _ = session.write_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "model/list",
            "params": {},
        }));
        Ok(session)
    }

    fn handshake(
        &mut self,
        config: &CodexConfig,
        steps: &Receiver<Result<HandshakeStep, String>>,
    ) -> Result<(), String> {
        // Ids 1 and 2 by construction — the reader correlates exactly these,
        // and the committed captures use the same sequence so replayed
        // responses answer the session's own requests.
        let id = self.take_request_id();
        debug_assert_eq!(id, 1);
        self.write_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {"clientInfo": {"name": "ferrite", "version": env!("CARGO_PKG_VERSION")}},
        }))
        .map_err(|e| format!("could not write initialize: {e}"))?;
        match await_step(steps, "initialize")? {
            HandshakeStep::Initialized => {}
            HandshakeStep::Thread(_) => return Err("thread answered before initialize".into()),
        }
        // Nothing in the initialize response is acted on — it names the
        // server's home and user agent — but the protocol requires the
        // acknowledgement before any thread traffic.
        self.write_line(&serde_json::json!({"jsonrpc": "2.0", "method": "initialized"}))
            .map_err(|e| format!("could not write initialized: {e}"))?;

        let id = self.take_request_id();
        let (method, mut params) = match &config.resume {
            Some(thread_id) => ("thread/resume", serde_json::json!({"threadId": thread_id})),
            None => ("thread/start", serde_json::json!({})),
        };
        if let Some(cwd) = &config.cwd {
            params["cwd"] = serde_json::json!(cwd.display().to_string());
        }
        if let Some(model) = &config.model {
            params["model"] = serde_json::json!(model);
        }
        if let Some(policy) = &config.approval_policy {
            params["approvalPolicy"] = serde_json::json!(policy);
        }
        if let Some(sandbox) = &config.sandbox {
            params["sandbox"] = serde_json::json!(sandbox);
        }
        // Capture the CLI's default independently of Ferrite's override;
        // turn/start carries the chosen effort.
        self.write_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .map_err(|e| format!("could not write {method}: {e}"))?;
        match await_step(steps, method)? {
            HandshakeStep::Thread(thread) => {
                self.thread_id = thread.thread_id;
                self.model = thread.model;
                self.capabilities = thread.capabilities;
                Ok(())
            }
            HandshakeStep::Initialized => Err("initialize answered twice".into()),
        }
    }

    /// Set effort on the existing thread's next turn. Omitting effort after
    /// an override retains it, so Default must name the server's default.
    pub fn set_effort(&mut self, effort: Option<&str>) -> io::Result<()> {
        let effort = match effort {
            Some(effort) => effort.to_string(),
            None => self
                .capabilities
                .reasoning_effort
                .clone()
                .or_else(|| {
                    lock(&self.models)
                        .iter()
                        .find(|row| {
                            let model = self.model_override.as_deref().unwrap_or(&self.model);
                            row.value == model || row.resolved.as_deref() == Some(model)
                        })
                        .and_then(|row| row.default_effort.clone())
                })
                .ok_or_else(|| io::Error::other("Codex has not reported its default effort"))?,
        };
        self.effort = Some(effort);
        Ok(())
    }

    /// Select a model for the next turn on this thread. The app-server keeps
    /// the process and thread alive; the choice travels on `turn/start`.
    pub fn set_model(&mut self, model: Option<&str>) -> io::Result<()> {
        let Some(model) = model else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Codex cannot restore an unknown default model",
            ));
        };
        self.model_override = Some(model.to_string());
        Ok(())
    }

    /// Send one user prompt; the server starts a turn on the Session's thread.
    ///
    /// The text is translated to typed input items first (#23): a leading
    /// `/name` naming a listed skill rides as a `{"type":"skill"}` item and
    /// `@path` tokens naming real files ride as `{"type":"mention"}` items —
    /// the server never intercepts slash text, so this seam is where the
    /// Composer's picks become real.
    pub fn send(&mut self, text: &str) -> io::Result<()> {
        let input = wire::input_items(text, &lock(&self.skills), self.cwd.as_deref());
        // The Pane makes its own compact preview; request the detailed
        // provider summary so expanding it can reveal additional text.
        let mut params =
            serde_json::json!({"threadId": self.thread_id, "input": input, "summary": "detailed"});
        if let Some(effort) = &self.effort {
            params["effort"] = serde_json::json!(effort);
        }
        if let Some(model) = &self.model_override {
            params["model"] = serde_json::json!(model);
        }
        let id = self.take_request_id();
        lock(&self.requests).start(id)?;
        let result = self.write_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "turn/start",
            "params": params,
        }));
        if result.is_err() {
            lock(&self.requests).discard(id);
        }
        result
    }

    /// Interrupt Main. When a sent start has not yet named its turn, retain
    /// one interruption until its acknowledgement or turn/started supplies it.
    pub fn interrupt(&mut self) -> io::Result<()> {
        let id = self.take_request_id();
        let request = lock(&self.requests).interrupt(id, &self.thread_id)?;
        let Some(request) = request else {
            return Ok(());
        };
        let result = self.write_line(&request);
        if result.is_err() {
            lock(&self.requests).discard(id);
        }
        result
    }

    /// What the thread/start response said this install can do, answered at
    /// spawn — never assumed.
    pub fn capabilities(&self) -> &CodexCapabilities {
        &self.capabilities
    }

    /// Rename the thread server-side (`thread/name/set`), so the server's
    /// own thread list carries the Thread's title. A refusal is reported as
    /// a Main notice without ending its turn.
    pub fn set_name(&mut self, name: &str) -> io::Result<()> {
        let id = self.take_request_id();
        lock(&self.requests).rename(id)?;
        let result = self.write_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "thread/name/set",
            "params": {"threadId": self.thread_id, "name": name},
        }));
        if result.is_err() {
            lock(&self.requests).discard(id);
        }
        result
    }

    /// Answer a `DecisionRequested`, quoting the id it arrived with.
    ///
    /// The server blocks the turn until this lands. Two Codex capability
    /// gaps are surfaced here rather than papered over: an `Allow` cannot
    /// edit the tool's input (the wire's answer is a bare "accept", so
    /// `input` is ignored), and a `Deny` cannot carry the operator's message
    /// to the model (the wire's "decline" takes no text — the model learns
    /// only that the tool was rejected).
    pub fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()> {
        let rpc = self.take_request_id();
        let turn = lock(&self.requests).current_turn.clone();
        let request = lock(&self.question_replies).prepare(
            id,
            &answer,
            rpc,
            &self.thread_id,
            turn.as_deref(),
        )?;
        if let Some(mut request) = request {
            if request["method"] == "turn/start" {
                if let Some(effort) = &self.effort {
                    request["params"]["effort"] = effort.clone().into();
                }
                if let Some(model) = &self.model_override {
                    request["params"]["model"] = model.clone().into();
                }
            }
            let result = self.write_line(&request);
            if result.is_err() {
                lock(&self.question_replies).discard(rpc);
            }
            return result;
        }

        let native = { lock(&self.native_questions).response(id, &answer) };
        if let Some(result) = native {
            let handle = id.to_owned();
            let id = wire::decision_request_id(&handle)?;
            let result = result?;
            self.write_line(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result,
            }))?;
            lock(&self.native_questions).resolved(&handle);
            return Ok(());
        }

        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Decision is not pending",
        ))
    }

    /// The process this Session runs, for a watchdog counting its memory.
    #[cfg(not(windows))]
    pub fn pid(&self) -> Option<u32> {
        self.child.lock().ok().map(|child| child.id())
    }

    /// The process the watchdog should meter. Under an npm `.cmd` shim the
    /// child is a ~5MB cmd.exe and the CLI leaks beneath it, so the job
    /// answers with the wrapper's child instead.
    #[cfg(windows)]
    pub fn pid(&self) -> Option<u32> {
        let wrapper = self.child.lock().ok().map(|child| child.id())?;
        Some(self.job.watchdog_pid(wrapper))
    }

    /// The bounded event stream. Poll with `try_recv`/`try_iter`; the UI
    /// drains this per frame.
    pub fn events(&self) -> &Receiver<SessionEvent> {
        &self.events
    }

    /// Ferrite numbers its own requests; the server's ids are its own and are
    /// echoed back untouched (see `respond_to_decision`).
    fn take_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id += 1;
        id
    }

    fn write_line(&mut self, value: &serde_json::Value) -> io::Result<()> {
        write_request(&self.stdin, value)
    }
}

/// Session commands and reader-owned history requests share one writer. Hold
/// its lock for the complete JSON-RPC frame so neither can interleave bytes.
fn write_request(stdin: &Mutex<ChildStdin>, value: &serde_json::Value) -> io::Result<()> {
    let mut line = serde_json::to_string(value).map_err(io::Error::other)?;
    line.push('\n');
    let mut stdin = lock(stdin);
    stdin.write_all(line.as_bytes())?;
    stdin.flush()
}

impl Drop for CodexSession {
    fn drop(&mut self) {
        // A `.cmd` shim's Session is a tree; killing only the wrapper would
        // orphan the CLI. The job takes all of it down, wrapper included.
        // The handshake-failure path in `spawn` kills only the wrapper; the
        // rest of its tree dies here when the failed Session is dropped.
        #[cfg(windows)]
        self.job.terminate();
        let mut child = lock(&self.child);
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// What the reader hands spawn while the handshake is open: the initialize
/// acknowledgement, then the thread handshake itself.
enum HandshakeStep {
    Initialized,
    Thread(Box<ThreadHandshake>),
}

/// One handshake response, or why there will not be one: the server's own
/// error, its silence past the budget, or its death (the reader hangs up).
fn await_step(
    steps: &Receiver<Result<HandshakeStep, String>>,
    waiting_on: &str,
) -> Result<HandshakeStep, String> {
    match steps.recv_timeout(HANDSHAKE_TIMEOUT) {
        Ok(step) => step,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(format!(
            "no {waiting_on} response within {HANDSHAKE_TIMEOUT:?}"
        )),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err(format!("server closed before answering {waiting_on}"))
        }
    }
}

/// Returns the channel the handshake steps arrive on. It is a channel rather
/// than return values because only this thread ever reads stdout: letting
/// spawn read it directly would race the reader for lines.
fn read_stdout(
    stdout: ChildStdout,
    stdin: Weak<Mutex<ChildStdin>>,
    sender: SyncSender<SessionEvent>,
    child: Arc<Mutex<Child>>,
    stderr_tail: Arc<Mutex<StderrTail>>,
    requests: Arc<Mutex<requests::Requests>>,
    skills: Arc<Mutex<Vec<crate::SessionCommand>>>,
    model_catalog: Arc<Mutex<Vec<crate::ModelInfo>>>,
    question_replies: Arc<Mutex<questions::Replies>>,
    native_questions: Arc<Mutex<questions::NativeRequests>>,
    cwd: Option<PathBuf>,
) -> Receiver<Result<HandshakeStep, String>> {
    let (step_sender, steps) = sync_channel(2);
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        let mut activity = activity::Router::default();
        let mut turns = MainTurnTracker {
            main_thread_id: None,
            early_turns: HashMap::new(),
            requests: requests.clone(),
        };
        // Which handshake response is awaited: request 1, then request 2,
        // then none.
        let mut handshake = Some((step_sender, 1u64));
        let mut catalogs = live_catalogs::Catalogs::new(cwd.as_deref());
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            // Lossy rather than strict: a byte the server mangles must not
            // end a Session.
            let text = String::from_utf8_lossy(&line);
            let text = text.trim_end();
            if let Some((step_sender, pending)) = handshake.take() {
                match wire::parse_response(text, pending) {
                    Some(Ok(_)) if pending == 1 => {
                        let _ = step_sender.send(Ok(HandshakeStep::Initialized));
                        handshake = Some((step_sender, 2));
                        continue;
                    }
                    Some(Ok(result)) => match wire::parse_thread_response(&result) {
                        Some(thread) => {
                            turns.identify_main(&thread.thread_id);
                            // The Session announces itself the way every
                            // provider does; the values are the wire's, only
                            // the correlation is Ferrite's.
                            let _ = sender.send(SessionEvent::Init {
                                session_id: thread.thread_id.clone(),
                                model: thread.model.clone(),
                            });
                            let update = activity.identify_main(&thread.thread_id);
                            // Release spawn before publishing a potentially
                            // large resumed tree into the bounded event stream.
                            // Main's interrupt owner is already authoritative.
                            let _ = step_sender.send(Ok(HandshakeStep::Thread(Box::new(thread))));
                            if !publish_activity(update, &mut activity, &sender, &stdin) {
                                return;
                            }
                            let update = activity.root_history(&result["thread"]);
                            if !publish_activity(update, &mut activity, &sender, &stdin) {
                                return;
                            }
                            continue;
                        }
                        None => {
                            let _ = step_sender
                                .send(Err(format!("thread response carried no thread: {result}")));
                            return;
                        }
                    },
                    Some(Err(error)) => {
                        let _ = step_sender.send(Err(error));
                        return;
                    }
                    None => handshake = Some((step_sender, pending)),
                }
            }
            turns.observe(text);
            if let Ok(frame) = serde_json::from_str(text) {
                if let Some(update) = catalogs.observe(&frame) {
                    for event in update.events {
                        match &event {
                            SessionEvent::Commands { commands } => {
                                *lock(&skills) = commands.clone();
                            }
                            SessionEvent::Models { models } => {
                                *lock(&model_catalog) = models.clone();
                            }
                            _ => {}
                        }
                        if sender.send(event).is_err() {
                            return;
                        }
                    }
                    if let Some(request) = update.request {
                        let Some(stdin) = stdin.upgrade() else {
                            return;
                        };
                        if let Err(error) = write_request(&stdin, &request) {
                            if sender
                                .send(catalogs.write_failed(&request, &error))
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                    continue;
                }
                let (response, interrupt) = {
                    let mut requests = lock(&requests);
                    let response = requests.response(&frame);
                    let interrupt = turns
                        .main_thread_id
                        .as_deref()
                        .and_then(|thread| requests.take_interrupt(thread));
                    (response, interrupt)
                };
                if let Some(interrupt) = interrupt {
                    let Some(stdin) = stdin.upgrade() else {
                        return;
                    };
                    if let Err(error) = write_request(&stdin, &interrupt) {
                        lock(&requests).discard(interrupt["id"].as_u64().expect("host request ID"));
                        if sender
                            .send(requests::notice(format!(
                                "Could not interrupt Codex: {error}"
                            )))
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                if let Some(events) = response {
                    for event in events {
                        if sender.send(event).is_err() {
                            return;
                        }
                    }
                    continue;
                }
                lock(&native_questions).observe(&frame);
                if let Some(decision) = questions::decode(&frame["params"]) {
                    lock(&question_replies).register(&decision);
                }
                let reply = lock(&question_replies).observe(&frame);
                if let Some(reply) = reply {
                    if sender.send(reply).is_err() {
                        return;
                    }
                    continue;
                }
                let update = activity.observe(frame);
                if !publish_activity(update, &mut activity, &sender, &stdin) {
                    return;
                }
            }
        }
        if let Some((step_sender, _)) = handshake {
            let _ = step_sender.send(Err("server closed stdout before answering".into()));
        }
        *lock(&requests) = requests::Requests::default();
        let _ = sender.send(closed_event(&child, &stderr_tail));
    });
    steps
}

fn publish_activity(
    update: activity::Update,
    router: &mut activity::Router,
    sender: &SyncSender<SessionEvent>,
    stdin: &Weak<Mutex<ChildStdin>>,
) -> bool {
    for event in update.events {
        // A full channel parks the reader and backpressures the provider.
        if sender.send(event).is_err() {
            return false;
        }
    }
    for request in update.requests {
        let Some(stdin) = stdin.upgrade() else {
            return false;
        };
        if write_request(&stdin, &request).is_err() {
            for event in router.request_failed(&request).events {
                if sender.send(event).is_err() {
                    return false;
                }
            }
        }
    }
    true
}

/// Main's interrupt target. Child lifecycle shares this connection but cannot
/// replace or retire Main's turn. The handshake is the sole identity authority.
struct MainTurnTracker {
    main_thread_id: Option<String>,
    early_turns: HashMap<String, String>,
    requests: Arc<Mutex<requests::Requests>>,
}

impl MainTurnTracker {
    fn identify_main(&mut self, thread_id: &str) {
        lock(&self.requests).current_turn = self.early_turns.remove(thread_id);
        self.early_turns.clear();
        self.main_thread_id = Some(thread_id.to_owned());
    }

    fn observe(&mut self, line: &str) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            return;
        };
        let started = match value.get("method").and_then(serde_json::Value::as_str) {
            Some("turn/started") => true,
            Some("turn/completed") => false,
            _ => return,
        };
        let params = &value["params"];
        let (Some(thread_id), Some(turn_id)) =
            (activity::frame_scope(&value), params["turn"]["id"].as_str())
        else {
            return;
        };
        if let Some(main_thread_id) = &self.main_thread_id {
            if thread_id != main_thread_id {
                return;
            }
            let mut requests = lock(&self.requests);
            if started {
                requests.started(turn_id);
            } else {
                requests.completed(turn_id);
            }
        } else if started {
            if self.early_turns.len() < EARLY_TURN_THREAD_LIMIT
                || self.early_turns.contains_key(thread_id)
            {
                self.early_turns
                    .insert(thread_id.to_owned(), turn_id.to_owned());
            }
        } else if self.early_turns.get(thread_id).map(String::as_str) == Some(turn_id) {
            self.early_turns.remove(thread_id);
        }
    }
}

/// The last of the server's stderr, and whether there is any more coming.
#[derive(Default)]
struct StderrTail {
    lines: Vec<String>,
    finished: bool,
}

fn drain_stderr(stderr: ChildStderr, tail: Arc<Mutex<StderrTail>>) {
    thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let mut tail = lock(&tail);
            if tail.lines.len() == STDERR_TAIL_LINES {
                tail.lines.remove(0);
            }
            tail.lines.push(line);
        }
        lock(&tail).finished = true;
    });
}

fn closed_event(child: &Mutex<Child>, stderr_tail: &Mutex<StderrTail>) -> SessionEvent {
    let status = reap(child);
    let mut reason = match &status {
        Ok(status) => format!("codex app-server exited: {status}"),
        Err(e) => format!("codex app-server exit status unknown: {e}"),
    };
    if !matches!(&status, Ok(status) if status.success()) {
        let lines = settled_stderr(stderr_tail);
        if !lines.is_empty() {
            reason.push_str("\nstderr: ");
            reason.push_str(&lines.join("\n"));
        }
    }
    SessionEvent::Closed { reason }
}

/// The drain thread reaches EOF just after the child exits; wait for it rather
/// than explaining a crash with the reason cut off. Bounded, because a
/// surviving grandchild can hold the inherited stderr open indefinitely.
fn settled_stderr(stderr_tail: &Mutex<StderrTail>) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_millis(250);
    loop {
        let tail = lock(stderr_tail);
        if tail.finished || Instant::now() >= deadline {
            return tail.lines.clone();
        }
        drop(tail);
        thread::sleep(Duration::from_millis(5));
    }
}

/// Polled, never blocking, so `Drop` can always take this lock and kill a
/// server that closed stdout without exiting.
fn reap(child: &Mutex<Child>) -> io::Result<ExitStatus> {
    loop {
        if let Some(status) = lock(child).try_wait()? {
            return Ok(status);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// A panicking thread must not take the Session down with it: the data behind
/// this lock is a process handle, a stderr tail or a turn id, all still
/// usable.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn spawn_error(program: &str, e: io::Error) -> CodexSpawnError {
    if e.kind() == io::ErrorKind::NotFound {
        CodexSpawnError::CliNotFound {
            program: program.to_string(),
        }
    } else {
        CodexSpawnError::Io(e)
    }
}

fn check_version(program: &str) -> Result<(), CodexSpawnError> {
    let output = Command::new(program)
        .arg("--version")
        .output()
        .map_err(|e| spawn_error(program, e))?;
    if !output.status.success() {
        return Err(CodexSpawnError::VersionCheckFailed {
            detail: format!("`{program} --version` {}", output.status),
        });
    }

    let reported = String::from_utf8_lossy(&output.stdout);
    let Some((found, version)) = parse_version(&reported) else {
        return Err(CodexSpawnError::VersionCheckFailed {
            detail: format!(
                "unrecognised `{program} --version` output: {:?}",
                reported.trim()
            ),
        });
    };
    if version < CODEX_CLI_MIN_VERSION {
        return Err(CodexSpawnError::CliVersionUnmet {
            found,
            required: MIN_VERSION_DISPLAY,
        });
    }
    if version >= CODEX_CLI_MAX_VERSION_EXCLUSIVE {
        return Err(CodexSpawnError::CliVersionUnsupported {
            found,
            supported_below: MAX_VERSION_DISPLAY,
        });
    }
    Ok(())
}

/// `--version` prints `codex-cli 0.149.1`: the semver is not the first token,
/// so the first token that reads as one is taken, and a pre-release suffix on
/// any component is ignored.
pub(crate) fn parse_version(reported: &str) -> Option<(String, [u64; 3])> {
    reported.split_whitespace().find_map(parse_version_token)
}

fn parse_version_token(token: &str) -> Option<(String, [u64; 3])> {
    let mut components = token.split('.');
    let mut version = [0u64; 3];
    for component in version.iter_mut() {
        let digits: String = components
            .next()?
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        *component = digits.parse().ok()?;
    }
    Some((token.to_string(), version))
}

/// Codex's way of titling a Thread: `codex exec`, whose stdout is the
/// final message alone (the banner goes to stderr).
pub mod title {
    use crate::providers::oneshot::Form as TitleForm;

    /// The small model in Codex's own catalogue.
    pub const MODEL: &str = "gpt-5.4-mini";
    pub const EFFORT: &str = "low";

    /// Non-interactive, the cheap model at low reasoning, no session files
    /// (`--ephemeral`), no user config or rules (so the operator's own
    /// model, hooks and policies stay out of it), a read-only sandbox in
    /// case the model reaches for a shell anyway, no colour codes in the
    /// reply, and no git-repo requirement for the throwaway cwd. Each flag
    /// verified against `codex exec --help` of 0.144.4. The prompt is the
    /// positional argument.
    pub fn fill(program: &str, prompt: &str) -> TitleForm {
        fill_with_model(program, prompt, MODEL)
    }

    pub(super) fn fill_with_model(program: &str, prompt: &str, model: &'static str) -> TitleForm {
        let effort = format!("model_reasoning_effort=\"{EFFORT}\"");
        TitleForm {
            program: program.to_string(),
            args: [
                "exec",
                "--model",
                model,
                "-c",
                effort.as_str(),
                "--ephemeral",
                "--ignore-user-config",
                "--ignore-rules",
                "--skip-git-repo-check",
                "--sandbox",
                "read-only",
                "--color",
                "never",
                prompt,
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            model,
            effort: EFFORT,
        }
    }
}

/// Follow-ups use the same isolated exec policy as Titles, with the
/// operator-requested GPT-5.5 at its lowest supported reasoning effort.
pub(super) mod followup {
    use super::title;
    use crate::providers::oneshot::Form;

    pub fn fill(program: &str, system: &str) -> Form {
        let mut form = title::fill_with_model(program, system, "gpt-5.5");
        // No local tools or external connectors are needed for a prediction.
        form.args.splice(
            1..1,
            [
                "-c",
                "features.shell_tool=false",
                "-c",
                "features.apps=false",
                "-c",
                "features.multi_agent=false",
                "-c",
                "web_search=\"disabled\"",
            ]
            .map(str::to_string),
        );
        form
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_displayed_window_matches_the_pins() {
        assert_eq!(
            parse_version(MIN_VERSION_DISPLAY).map(|(_, v)| v),
            Some(CODEX_CLI_MIN_VERSION)
        );
        assert_eq!(
            parse_version(MAX_VERSION_DISPLAY).map(|(_, v)| v),
            Some(CODEX_CLI_MAX_VERSION_EXCLUSIVE)
        );
        assert!(CODEX_CLI_MIN_VERSION < CODEX_CLI_MAX_VERSION_EXCLUSIVE);
    }

    /// The window is closed at the bottom and open at the top.
    #[test]
    fn the_next_major_is_out_and_the_release_before_it_is_in() {
        let last_supported = parse_version("0.999.999").unwrap().1;
        let next_major = parse_version("1.0.0").unwrap().1;
        assert!(last_supported < CODEX_CLI_MAX_VERSION_EXCLUSIVE);
        assert!(next_major >= CODEX_CLI_MAX_VERSION_EXCLUSIVE);
    }

    /// The version Ferrite is developed against has to sit inside its own
    /// pins.
    #[test]
    fn the_captured_fixture_version_is_supported() {
        let captured = parse_version("codex-cli 0.149.1").unwrap().1;
        assert!(captured >= CODEX_CLI_MIN_VERSION);
        assert!(captured < CODEX_CLI_MAX_VERSION_EXCLUSIVE);
    }

    #[test]
    fn parses_the_real_version_banner() {
        assert_eq!(
            parse_version("codex-cli 0.149.1\n"),
            Some(("0.149.1".to_string(), [0, 149, 1]))
        );
    }

    #[test]
    fn the_pinned_boundary_is_met_and_one_below_is_not() {
        let at_pin = parse_version("codex-cli 0.149.1").unwrap().1;
        let below_pin = parse_version("codex-cli 0.149.0").unwrap().1;
        assert!(at_pin >= CODEX_CLI_MIN_VERSION);
        assert!(below_pin < CODEX_CLI_MIN_VERSION);
    }

    #[test]
    fn older_minor_lines_are_below_the_pin() {
        for older in ["codex-cli 0.99.999", "codex-cli 0.148.999"] {
            assert!(parse_version(older).unwrap().1 < CODEX_CLI_MIN_VERSION);
        }
    }

    #[test]
    fn a_prerelease_suffix_still_parses() {
        assert_eq!(
            parse_version("codex-cli 0.150.0-alpha.1"),
            Some(("0.150.0-alpha.1".to_string(), [0, 150, 0]))
        );
    }

    #[test]
    fn unparseable_banners_yield_nothing() {
        for garbage in ["", "\n", "codex-cli", "codex-cli 0.149", "x.y.z", "..."] {
            assert_eq!(
                parse_version(garbage),
                None,
                "should not parse: {garbage:?}"
            );
        }
    }
}
