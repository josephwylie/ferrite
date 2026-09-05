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

mod wire;

use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
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
    stdin: ChildStdin,
    events: Receiver<SessionEvent>,
    capabilities: CodexCapabilities,
    thread_id: String,
    model: String,
    effort: Option<String>,
    models: Arc<Mutex<Vec<crate::ModelInfo>>>,
    /// The running turn's id, tracked by the reader from turn/started:
    /// turn/interrupt must name the turn it stops.
    current_turn: Arc<Mutex<Option<String>>>,
    /// The server's skills, filled by the reader from the skills/list answer
    /// (#23). `send` translates a leading `/name` against this list into the
    /// typed skill item — slash text is never intercepted server-side.
    skills: Arc<Mutex<Vec<crate::SessionCommand>>>,
    /// The thread's cwd, kept for resolving `@path` mention tokens.
    cwd: Option<PathBuf>,
    next_request_id: u64,
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

        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        let stderr_tail = Arc::new(Mutex::new(StderrTail::default()));
        drain_stderr(stderr, Arc::clone(&stderr_tail));

        let (sender, events) = sync_channel(EVENT_CHANNEL_CAPACITY);
        let child = Arc::new(Mutex::new(child));
        let current_turn = Arc::new(Mutex::new(None));
        let skills = Arc::new(Mutex::new(Vec::new()));
        let models = Arc::new(Mutex::new(Vec::new()));
        let handshake = read_stdout(
            stdout,
            sender,
            Arc::clone(&child),
            Arc::clone(&stderr_tail),
            Arc::clone(&current_turn),
            Arc::clone(&skills),
            Arc::clone(&models),
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
            effort: config.effort.clone(),
            models,
            current_turn,
            skills,
            cwd: config.cwd.clone(),
            next_request_id: 1,
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
                            row.value == self.model
                                || row.resolved.as_deref() == Some(self.model.as_str())
                        })
                        .and_then(|row| row.default_effort.clone())
                })
                .ok_or_else(|| io::Error::other("Codex has not reported its default effort"))?,
        };
        self.effort = Some(effort);
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
        let mut params = serde_json::json!({"threadId": self.thread_id, "input": input});
        if let Some(effort) = &self.effort {
            params["effort"] = serde_json::json!(effort);
        }
        let id = self.take_request_id();
        self.write_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "turn/start",
            "params": params,
        }))
    }

    /// Interrupt the running turn. Codex addresses interrupts to a turn id,
    /// so before the first turn/started has arrived there is nothing to name
    /// and this is a no-op — the same harmless outcome as interrupting an
    /// idle Claude Session.
    pub fn interrupt(&mut self) -> io::Result<()> {
        let Some(turn_id) = lock(&self.current_turn).clone() else {
            return Ok(());
        };
        let id = self.take_request_id();
        self.write_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "turn/interrupt",
            "params": {"threadId": self.thread_id, "turnId": turn_id},
        }))
    }

    /// What the thread/start response said this install can do, answered at
    /// spawn — never assumed.
    pub fn capabilities(&self) -> &CodexCapabilities {
        &self.capabilities
    }

    /// Rename the thread server-side (`thread/name/set`), so the server's
    /// own thread list carries the Thread's title. The acknowledgement is
    /// an empty result nothing waits on.
    pub fn set_name(&mut self, name: &str) -> io::Result<()> {
        let id = self.take_request_id();
        self.write_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "thread/name/set",
            "params": {"threadId": self.thread_id, "name": name},
        }))
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
        let decision = match &answer {
            DecisionAnswer::Allow { .. } => serde_json::json!("accept"),
            DecisionAnswer::Deny { .. } => serde_json::json!("decline"),
            // The standing answer is one of the request's own
            // `availableDecisions`, echoed back whole: the server takes the
            // object exactly as it offered it.
            DecisionAnswer::AllowAlways { suggestion, .. } => suggestion.clone(),
        };
        // The server's id space is its own: 0.149.1 numbers requests with
        // integers, which `DecisionRequested` carried as text. Echo back the
        // original type, not Ferrite's.
        let id = match id.parse::<u64>() {
            Ok(number) => serde_json::json!(number),
            Err(_) => serde_json::json!(id),
        };
        self.write_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"decision": decision},
        }))
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
        let mut line = serde_json::to_string(value).map_err(io::Error::other)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.flush()
    }
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
    sender: SyncSender<SessionEvent>,
    child: Arc<Mutex<Child>>,
    stderr_tail: Arc<Mutex<StderrTail>>,
    current_turn: Arc<Mutex<Option<String>>>,
    skills: Arc<Mutex<Vec<crate::SessionCommand>>>,
    model_catalog: Arc<Mutex<Vec<crate::ModelInfo>>>,
) -> Receiver<Result<HandshakeStep, String>> {
    let (step_sender, steps) = sync_channel(2);
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        // Which handshake response is awaited: request 1, then request 2,
        // then none.
        let mut handshake = Some((step_sender, 1u64));
        // Once the thread is up, the skills/list answer (request 3) and the
        // model/list answer (request 4) are still owed; correlated here like
        // the handshake, but never blocking — a server without either
        // method just leaves that menu empty.
        let mut menu_pending = false;
        let mut models_pending = false;
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
                            // The Session announces itself the way every
                            // provider does; the values are the wire's, only
                            // the correlation is Ferrite's.
                            let _ = sender.send(SessionEvent::Init {
                                session_id: thread.thread_id.clone(),
                                model: thread.model.clone(),
                            });
                            let _ = step_sender.send(Ok(HandshakeStep::Thread(Box::new(thread))));
                            menu_pending = true;
                            models_pending = true;
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
            if menu_pending {
                if let Some(response) = wire::parse_response(text, SKILLS_REQUEST_ID) {
                    menu_pending = false;
                    if let Ok(result) = response {
                        let commands = wire::parse_skills(&result);
                        *lock(&skills) = commands.clone();
                        // Announce the menu on the event stream so the
                        // cockpit can fold it (#23); a server listing no
                        // skills announces nothing.
                        if !commands.is_empty()
                            && sender.send(SessionEvent::Commands { commands }).is_err()
                        {
                            return;
                        }
                    }
                    continue;
                }
            }
            if models_pending {
                if let Some(response) = wire::parse_response(text, MODELS_REQUEST_ID) {
                    models_pending = false;
                    if let Ok(result) = response {
                        let models = wire::parse_models(&result);
                        *lock(&model_catalog) = models.clone();
                        // The picker's rows (#25); a server listing none
                        // announces nothing and the fallback catalog stands.
                        if !models.is_empty()
                            && sender.send(SessionEvent::Models { models }).is_err()
                        {
                            return;
                        }
                    }
                    continue;
                }
            }
            track_turn(text, &current_turn);
            if let Some(event) = wire::parse_line(text) {
                // A full channel parks this thread, the OS pipe fills, and the
                // server blocks on its own write. Backpressure, never loss.
                if sender.send(event).is_err() {
                    return;
                }
            }
        }
        if let Some((step_sender, _)) = handshake {
            let _ = step_sender.send(Err("server closed stdout before answering".into()));
        }
        let _ = sender.send(closed_event(&child, &stderr_tail));
    });
    steps
}

/// The running turn's id, tracked for `interrupt` off the turn lifecycle
/// notifications: turn/started names it, turn/completed retires it, so a
/// late interrupt is a no-op instead of a request naming a dead turn.
/// Session state, not a SessionEvent: no Pane renders it.
fn track_turn(line: &str, current_turn: &Mutex<Option<String>>) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return;
    };
    match value.get("method").and_then(serde_json::Value::as_str) {
        Some("turn/started") => {
            let turn_id = value
                .get("params")
                .and_then(|p| p.get("turn"))
                .and_then(|t| t.get("id"))
                .and_then(serde_json::Value::as_str);
            if let Some(turn_id) = turn_id {
                *lock(current_turn) = Some(turn_id.to_string());
            }
        }
        Some("turn/completed") => *lock(current_turn) = None,
        _ => {}
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
    use crate::titler::TitleForm;

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
        let effort = format!("model_reasoning_effort=\"{EFFORT}\"");
        TitleForm {
            program: program.to_string(),
            args: [
                "exec",
                "--model",
                MODEL,
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
            model: MODEL,
            effort: EFFORT,
        }
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
