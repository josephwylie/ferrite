//! Claude provider: the pinned `claude` CLI spoken over stdio stream-json.
//!
//! Spawn checks the CLI version pin before any process serves a Thread;
//! a reader thread parses stdout lines into SessionEvents on a bounded
//! channel. Backpressure is stated and simple: when the channel is full the
//! reader thread blocks, the pipe fills, and the CLI stalls — nothing is
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

/// Minimum `claude` CLI version for stable stream-json + stdio control
/// protocol. Vendor releases below this break loudly at spawn, not weirdly
/// mid-turn.
pub const CLAUDE_CLI_MIN_VERSION: [u64; 3] = [2, 1, 224];

/// Exclusive ceiling: Ferrite is proven against the 2.x wire, and a new major
/// is a new protocol until someone re-runs the fixture captures against it. A
/// 3.x CLI is refused at spawn rather than trusted into a Session, because a
/// silently changed wire fails somewhere deep in a turn where the cause is
/// invisible.
pub const CLAUDE_CLI_MAX_VERSION_EXCLUSIVE: [u64; 3] = [3, 0, 0];

/// The supported window as it is shown to operators.
const MIN_VERSION_DISPLAY: &str = "2.1.224";
const MAX_VERSION_DISPLAY: &str = "3.0.0";

/// One frame of UI drains far less than this; the depth exists so a stalled
/// frame throttles the CLI instead of losing its output.
const EVENT_CHANNEL_CAPACITY: usize = 1024;

/// Enough stderr to explain a crash, bounded so a chatty CLI cannot grow
/// memory for the life of a Session.
const STDERR_TAIL_LINES: usize = 20;

/// How long spawn waits for the initialize control response. Measured against
/// `claude` 2.1.243, which answers in well under a second from a cold start;
/// the budget is generous because overrunning it costs only unknown
/// capabilities, never a failed spawn.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// The request id spawn uses for the handshake, before any operator traffic.
const HANDSHAKE_REQUEST_ID: &str = "req_1";

/// How to spawn a Claude Session.
#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    /// Program to exec. Tests point this at a stub binary.
    pub program: String,
    /// Working directory for the CLI process (the Thread's workspace binding).
    pub cwd: Option<PathBuf>,
    /// Model override passed through to the CLI.
    pub model: Option<String>,
    /// Reasoning effort (`"low"`, `"medium"`, `"high"`, `"xhigh"`, `"max"`)
    /// passed through as `--effort`. `None` leaves the CLI's own default.
    pub effort: Option<String>,
    /// The Thread's title, handed to the CLI as the session's display name
    /// (`--name`) so its own session list reads like Ferrite's. Spawn-time
    /// only: the CLI takes no rename over the wire, so a later title waits
    /// for the next Session.
    pub name: Option<String>,
    /// Permission posture for this Thread (`"default"`, `"acceptEdits"`,
    /// `"plan"`, …). `None` leaves the CLI's own configuration alone — which
    /// on a machine configured to bypass permissions means no Decision will
    /// ever arrive. `capabilities().permission_mode` reports what took effect.
    pub permission_mode: Option<String>,
    /// Resume this provider-native session id (from a previous Session's
    /// `Init`) instead of starting fresh: the CLI reloads the conversation
    /// from its own session files. Probed on 2.1.243: the resumed process
    /// announces the *same* session id in its init line, so the target stays
    /// stable across any number of resumes.
    pub resume: Option<String>,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            program: "claude".into(),
            cwd: None,
            model: None,
            effort: None,
            name: None,
            permission_mode: None,
            resume: None,
        }
    }
}

/// Spawn failed before a Session existed.
#[derive(Debug)]
pub enum ClaudeSpawnError {
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
    Io(io::Error),
}

impl std::fmt::Display for ClaudeSpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClaudeSpawnError::CliNotFound { program } => {
                write!(f, "claude CLI not found: `{program}`")
            }
            ClaudeSpawnError::CliVersionUnmet { found, required } => {
                write!(
                    f,
                    "claude CLI {found} is older than the pinned minimum {required}; \
                     upgrade the CLI"
                )
            }
            ClaudeSpawnError::CliVersionUnsupported {
                found,
                supported_below,
            } => {
                write!(
                    f,
                    "claude CLI {found} is a newer major release than Ferrite is proven \
                     against (below {supported_below}); upgrade Ferrite"
                )
            }
            ClaudeSpawnError::VersionCheckFailed { detail } => {
                write!(f, "claude CLI version check failed: {detail}")
            }
            ClaudeSpawnError::Io(e) => write!(f, "io error spawning claude CLI: {e}"),
        }
    }
}

impl std::error::Error for ClaudeSpawnError {}

/// What the CLI answered at initialize: feature detection, so a Pane never
/// offers what this install cannot do.
///
/// Only what Ferrite acts on is kept; the response also carries the CLI's
/// slash commands, subagents and output styles, which nothing reads yet.
/// Note what is *not* here: the `interrupt_receipt_v1` family of capability
/// tokens is announced on the `system:init` line at the head of a turn, not in
/// this handshake, so it cannot be known before the first prompt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClaudeCapabilities {
    /// The permission mode the CLI started in. `"bypassPermissions"` means no
    /// Decision will ever arrive; every other mode means they can.
    pub permission_mode: String,
    /// Model values this install offers (`"haiku"`, `"opus[1m]"`, …) — the
    /// menu an operator may pick from, not what Ferrite hardcodes.
    pub models: Vec<crate::ModelInfo>,
    /// The CLI's effective slash-command menu — built-ins, skills, project
    /// commands and plugins in one list, disabled overrides already honoured
    /// (#23). The reader announces it as `SessionEvent::Commands`; submitting
    /// `/name args` as plain prompt text is how one is invoked.
    pub commands: Vec<crate::SessionCommand>,
}

/// A live Claude Session: one CLI process serving one Thread.
pub struct ClaudeSession {
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
    capabilities: ClaudeCapabilities,
    next_request_id: u64,
}

impl ClaudeSession {
    /// Version-check the CLI, then spawn it in stream-json mode.
    pub fn spawn(config: ClaudeConfig) -> Result<Self, ClaudeSpawnError> {
        // On Windows an npm install is a `claude.cmd` shim a bare name
        // cannot exec; everything spawns through the resolved answer.
        let program = super::spawnable_program(&config.program);
        check_version(&program)?;

        let mut command = Command::new(&program);
        command.args([
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--include-partial-messages",
            "--verbose",
            // Route tool permissions to Ferrite over the control protocol
            // instead of letting the CLI answer them alone. A cockpit whose
            // whole job is surfacing Decisions cannot leave this to config.
            // Undocumented in `--help` on 2.1.243; `stdio` is the magic value
            // that means "ask the host on stdin", verified by capture.
            "--permission-prompt-tool",
            "stdio",
        ]);
        if let Some(session_id) = &config.resume {
            // Continue the named conversation instead of starting one. The
            // CLI reloads history from its own session files; probed on
            // 2.1.243, the resumed process keeps the same session id.
            command.args(["--resume", session_id.as_str()]);
        }
        if let Some(model) = &config.model {
            command.args(["--model", model.as_str()]);
        }
        if let Some(effort) = &config.effort {
            command.args(["--effort", effort.as_str()]);
        }
        if let Some(mode) = &config.permission_mode {
            command.args(["--permission-mode", mode.as_str()]);
        }
        if let Some(name) = config
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            command.args(["--name", name]);
        }
        if let Some(cwd) = &config.cwd {
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
            super::job::SessionJob::assign_or_reap(&mut child).map_err(ClaudeSpawnError::Io)?;

        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        let stderr_tail = Arc::new(Mutex::new(StderrTail::default()));
        drain_stderr(stderr, Arc::clone(&stderr_tail));

        let (sender, events) = sync_channel(EVENT_CHANNEL_CAPACITY);
        let child = Arc::new(Mutex::new(child));
        let capabilities = read_stdout(stdout, sender, Arc::clone(&child), stderr_tail);

        let mut session = Self {
            child,
            #[cfg(windows)]
            job,
            stdin,
            events,
            capabilities: ClaudeCapabilities::default(),
            next_request_id: 1,
        };
        // Before the operator is offered anything: ask the CLI what it can do.
        // A write failure here is a CLI that died on startup, which the reader
        // thread is already turning into a Closed event — spawn still hands
        // back a Session so that reason reaches the Pane.
        let id = session.take_request_id();
        debug_assert_eq!(id, HANDSHAKE_REQUEST_ID);
        let _ = session.write_line(&serde_json::json!({
            "type": "control_request",
            "request_id": id,
            "request": {"subtype": "initialize"},
        }));
        session.capabilities = capabilities
            .recv_timeout(HANDSHAKE_TIMEOUT)
            .unwrap_or_default();
        Ok(session)
    }

    /// Send one user prompt; the CLI starts (or queues) a turn.
    pub fn send(&mut self, text: &str) -> io::Result<()> {
        self.write_line(&serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{"type": "text", "text": text}],
            },
        }))
    }

    /// Interrupt the running turn over the stdio control protocol.
    pub fn interrupt(&mut self) -> io::Result<()> {
        let request_id = self.take_request_id();
        self.write_line(&serde_json::json!({
            "type": "control_request",
            "request_id": request_id,
            "request": {"subtype": "interrupt"},
        }))
    }

    /// What the CLI said it can do, answered at spawn. Empty when this install
    /// did not answer the handshake — unknown, never assumed.
    pub fn capabilities(&self) -> &ClaudeCapabilities {
        &self.capabilities
    }

    /// Answer a `DecisionRequested`, quoting the id it arrived with.
    ///
    /// The CLI blocks the turn until this lands: `Allow` runs the tool,
    /// `Deny` feeds the message back as the tool's error and the turn carries
    /// on. Unlike `interrupt`, the request id is the CLI's, not Ferrite's —
    /// this is a response to its question, so it must not be renumbered.
    pub fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()> {
        let body = match answer {
            DecisionAnswer::Allow { input } => {
                serde_json::json!({"behavior": "allow", "updatedInput": input})
            }
            DecisionAnswer::Deny { message } => {
                serde_json::json!({"behavior": "deny", "message": message})
            }
            // `updatedPermissions` carries the CLI's own suggestion back to
            // it; the permission-always capture proves a second call in the
            // same turn is then not gated at all.
            DecisionAnswer::AllowAlways { input, suggestion } => serde_json::json!({
                "behavior": "allow",
                "updatedInput": input,
                "updatedPermissions": [suggestion],
            }),
        };
        self.write_line(&serde_json::json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": id,
                "response": body,
            },
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

    /// Ferrite numbers its own control requests; the CLI's ids are its own and
    /// are echoed back untouched (see `respond_to_decision`).
    fn take_request_id(&mut self) -> String {
        let id = format!("req_{}", self.next_request_id);
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

impl Drop for ClaudeSession {
    fn drop(&mut self) {
        // A `.cmd` shim's Session is a tree; killing only the wrapper would
        // orphan the CLI. The job takes all of it down, wrapper included.
        #[cfg(windows)]
        self.job.terminate();
        let mut child = lock(&self.child);
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Returns the channel the initialize answer will arrive on. It is a channel
/// rather than a return value because only this thread ever reads stdout:
/// letting spawn read it directly would race the reader for lines.
fn read_stdout(
    stdout: ChildStdout,
    sender: SyncSender<SessionEvent>,
    child: Arc<Mutex<Child>>,
    stderr_tail: Arc<Mutex<StderrTail>>,
) -> Receiver<ClaudeCapabilities> {
    let (handshake, capabilities) = sync_channel(1);
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        let mut handshake = Some(handshake);
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            // Lossy rather than strict: a byte the CLI mangles must not end a
            // Session.
            let text = String::from_utf8_lossy(&line);
            let text = text.trim_end();
            if handshake.is_some() {
                if let Some(capabilities) = wire::parse_capabilities(text, HANDSHAKE_REQUEST_ID) {
                    // The command menu rides the same handshake line; announce
                    // it on the event stream so the cockpit can fold it (#23).
                    // An install that lists none announces nothing.
                    if !capabilities.commands.is_empty() {
                        let announced = SessionEvent::Commands {
                            commands: capabilities.commands.clone(),
                        };
                        if sender.send(announced).is_err() {
                            return;
                        }
                    }
                    // So does the permission mode — the meta row's chip.
                    // Display-only; a CLI that named none shows none.
                    if !capabilities.permission_mode.is_empty() {
                        let announced = SessionEvent::PermissionMode {
                            mode: capabilities.permission_mode.clone(),
                        };
                        if sender.send(announced).is_err() {
                            return;
                        }
                    }
                    // And the model menu — the provider picker's rows (#25).
                    // An install that lists none announces nothing.
                    if !capabilities.models.is_empty() {
                        let announced = SessionEvent::Models {
                            models: capabilities.models.clone(),
                        };
                        if sender.send(announced).is_err() {
                            return;
                        }
                    }
                    // Unblocks spawn. Dropping the sender afterwards is what
                    // stops a second control response being mistaken for it.
                    let _ = handshake.take().expect("just checked").send(capabilities);
                    continue;
                }
            }
            if let Some(event) = wire::parse_line(text) {
                // A full channel parks this thread, the OS pipe fills, and the
                // CLI blocks on its own write. Backpressure, never loss.
                if sender.send(event).is_err() {
                    return;
                }
            }
        }
        let _ = sender.send(closed_event(&child, &stderr_tail));
    });
    capabilities
}

/// The last of the CLI's stderr, and whether there is any more coming.
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
        Ok(status) => format!("claude CLI exited: {status}"),
        Err(e) => format!("claude CLI exit status unknown: {e}"),
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

/// Polled, never blocking, so `Drop` can always take this lock and kill a CLI
/// that closed stdout without exiting.
fn reap(child: &Mutex<Child>) -> io::Result<ExitStatus> {
    loop {
        if let Some(status) = lock(child).try_wait()? {
            return Ok(status);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// A panicking thread must not take the Session down with it: the data behind
/// this lock is a process handle and a stderr tail, both still usable.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn spawn_error(program: &str, e: io::Error) -> ClaudeSpawnError {
    if e.kind() == io::ErrorKind::NotFound {
        ClaudeSpawnError::CliNotFound {
            program: program.to_string(),
        }
    } else {
        ClaudeSpawnError::Io(e)
    }
}

fn check_version(program: &str) -> Result<(), ClaudeSpawnError> {
    let output = Command::new(program)
        .arg("--version")
        .output()
        .map_err(|e| spawn_error(program, e))?;
    if !output.status.success() {
        return Err(ClaudeSpawnError::VersionCheckFailed {
            detail: format!("`{program} --version` {}", output.status),
        });
    }

    let reported = String::from_utf8_lossy(&output.stdout);
    let Some((found, version)) = parse_version(&reported) else {
        return Err(ClaudeSpawnError::VersionCheckFailed {
            detail: format!(
                "unrecognised `{program} --version` output: {:?}",
                reported.trim()
            ),
        });
    };
    if version < CLAUDE_CLI_MIN_VERSION {
        return Err(ClaudeSpawnError::CliVersionUnmet {
            found,
            required: MIN_VERSION_DISPLAY,
        });
    }
    if version >= CLAUDE_CLI_MAX_VERSION_EXCLUSIVE {
        return Err(ClaudeSpawnError::CliVersionUnsupported {
            found,
            supported_below: MAX_VERSION_DISPLAY,
        });
    }
    Ok(())
}

/// `--version` prints `2.1.243 (Claude Code)`; only the leading semver is
/// meaningful, and a pre-release suffix on any component is ignored.
fn parse_version(reported: &str) -> Option<(String, [u64; 3])> {
    let token = reported.split_whitespace().next()?;
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

/// Claude Code's way of titling a Thread: `claude -p` in print mode.
pub mod title {
    use crate::titler::TitleForm;

    /// The cheapest alias, so a Thread's name costs nothing an operator
    /// would notice.
    pub const MODEL: &str = "haiku";
    pub const EFFORT: &str = "low";

    /// Print mode, the cheap model, text output, no tools (the title must
    /// not be a Bash call), no saved session (a title turn is not a
    /// conversation to resume), no settings sources (so no project or user
    /// hooks run in the throwaway directory), and nobody to answer prompts
    /// (`--tools ""` should leave none, but one that did appear must be
    /// denied rather than hang). Each flag verified against `claude --help`
    /// of 2.1.259; `--max-turns` does not exist there, and the tool-less
    /// turn is single anyway. The prompt is the positional argument.
    pub fn fill(program: &str, prompt: &str) -> TitleForm {
        TitleForm {
            program: program.to_string(),
            args: [
                "-p",
                "--model",
                MODEL,
                "--effort",
                EFFORT,
                "--output-format",
                "text",
                "--tools",
                "",
                "--no-session-persistence",
                "--setting-sources",
                "",
                "--permission-prompts",
                "none",
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
            Some(CLAUDE_CLI_MIN_VERSION)
        );
        assert_eq!(
            parse_version(MAX_VERSION_DISPLAY).map(|(_, v)| v),
            Some(CLAUDE_CLI_MAX_VERSION_EXCLUSIVE)
        );
        assert!(CLAUDE_CLI_MIN_VERSION < CLAUDE_CLI_MAX_VERSION_EXCLUSIVE);
    }

    /// The window is closed at the bottom and open at the top.
    #[test]
    fn the_next_major_is_out_and_the_release_before_it_is_in() {
        let last_supported = parse_version("2.99.99").unwrap().1;
        let next_major = parse_version("3.0.0").unwrap().1;
        assert!(last_supported < CLAUDE_CLI_MAX_VERSION_EXCLUSIVE);
        assert!(next_major >= CLAUDE_CLI_MAX_VERSION_EXCLUSIVE);
    }

    /// The version Ferrite is developed against has to sit inside its own pins.
    #[test]
    fn the_captured_fixture_version_is_supported() {
        let captured = parse_version("2.1.243").unwrap().1;
        assert!(captured >= CLAUDE_CLI_MIN_VERSION);
        assert!(captured < CLAUDE_CLI_MAX_VERSION_EXCLUSIVE);
    }

    #[test]
    fn parses_the_real_version_banner() {
        assert_eq!(
            parse_version("2.1.243 (Claude Code)\n"),
            Some(("2.1.243".to_string(), [2, 1, 243]))
        );
    }

    #[test]
    fn the_pinned_boundary_is_met_and_one_below_is_not() {
        let at_pin = parse_version("2.1.224").unwrap().1;
        let below_pin = parse_version("2.1.223").unwrap().1;
        assert!(at_pin >= CLAUDE_CLI_MIN_VERSION);
        assert!(below_pin < CLAUDE_CLI_MIN_VERSION);
    }

    #[test]
    fn older_major_and_minor_lines_are_below_the_pin() {
        for older in ["1.9.999", "2.0.999"] {
            assert!(parse_version(older).unwrap().1 < CLAUDE_CLI_MIN_VERSION);
        }
    }

    #[test]
    fn a_prerelease_suffix_still_parses() {
        assert_eq!(
            parse_version("2.2.0-beta.3 (Claude Code)"),
            Some(("2.2.0-beta.3".to_string(), [2, 2, 0]))
        );
    }

    #[test]
    fn unparseable_banners_yield_nothing() {
        for garbage in ["", "\n", "Claude Code", "2.1", "v2.1.243", "x.y.z", "..."] {
            assert_eq!(
                parse_version(garbage),
                None,
                "should not parse: {garbage:?}"
            );
        }
    }
}
