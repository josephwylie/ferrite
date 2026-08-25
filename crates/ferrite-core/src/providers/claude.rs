//! Claude provider: the pinned `claude` CLI spoken over stdio stream-json.
//!
//! Spawn checks the CLI version pin before any process starts a conversation;
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

use crate::SessionEvent;

/// Minimum `claude` CLI version for stable stream-json + stdio control
/// protocol. Vendor releases below this break loudly at spawn, not weirdly
/// mid-conversation.
pub const CLAUDE_CLI_MIN_VERSION: [u64; 3] = [2, 1, 224];

/// `CLAUDE_CLI_MIN_VERSION` as it is shown to operators.
const MIN_VERSION_DISPLAY: &str = "2.1.224";

/// One frame of UI drains far less than this; the depth exists so a stalled
/// frame throttles the CLI instead of losing its output.
const EVENT_CHANNEL_CAPACITY: usize = 1024;

/// Enough stderr to explain a crash, bounded so a chatty CLI cannot grow
/// memory for the life of a Session.
const STDERR_TAIL_LINES: usize = 20;

/// How to spawn a Claude Session.
#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    /// Program to exec. Tests point this at a stub binary.
    pub program: String,
    /// Working directory for the CLI process (the Thread's workspace binding).
    pub cwd: Option<PathBuf>,
    /// Model override passed through to the CLI.
    pub model: Option<String>,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            program: "claude".into(),
            cwd: None,
            model: None,
        }
    }
}

/// Spawn failed before a Session existed.
#[derive(Debug)]
pub enum SpawnError {
    /// The CLI program was not found on this machine.
    CliNotFound {
        program: String,
    },
    /// The CLI is older than the pin.
    CliVersionUnmet {
        found: String,
        required: &'static str,
    },
    /// `--version` ran but produced nothing parseable.
    VersionCheckFailed {
        detail: String,
    },
    Io(io::Error),
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnError::CliNotFound { program } => {
                write!(f, "claude CLI not found: `{program}`")
            }
            SpawnError::CliVersionUnmet { found, required } => {
                write!(
                    f,
                    "claude CLI {found} is older than the pinned minimum {required}"
                )
            }
            SpawnError::VersionCheckFailed { detail } => {
                write!(f, "claude CLI version check failed: {detail}")
            }
            SpawnError::Io(e) => write!(f, "io error spawning claude CLI: {e}"),
        }
    }
}

impl std::error::Error for SpawnError {}

/// A live Claude Session: one CLI process serving one Thread.
pub struct ClaudeSession {
    child: Arc<Mutex<Child>>,
    /// Held open for the life of the Session: closing it ends the
    /// conversation, so multi-turn depends on this staying alive.
    stdin: ChildStdin,
    events: Receiver<SessionEvent>,
    next_request_id: u64,
}

impl ClaudeSession {
    /// Version-check the CLI, then spawn it in stream-json mode.
    pub fn spawn(config: ClaudeConfig) -> Result<Self, SpawnError> {
        check_version(&config.program)?;

        let mut command = Command::new(&config.program);
        command.args([
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--include-partial-messages",
            "--verbose",
        ]);
        if let Some(model) = &config.model {
            command.args(["--model", model.as_str()]);
        }
        if let Some(cwd) = &config.cwd {
            command.current_dir(cwd);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| spawn_error(&config.program, e))?;

        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        let stderr_tail = Arc::new(Mutex::new(StderrTail::default()));
        drain_stderr(stderr, Arc::clone(&stderr_tail));

        let (sender, events) = sync_channel(EVENT_CHANNEL_CAPACITY);
        let child = Arc::new(Mutex::new(child));
        read_stdout(stdout, sender, Arc::clone(&child), stderr_tail);

        Ok(Self {
            child,
            stdin,
            events,
            next_request_id: 1,
        })
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
        let request_id = format!("req_{}", self.next_request_id);
        self.next_request_id += 1;
        self.write_line(&serde_json::json!({
            "type": "control_request",
            "request_id": request_id,
            "request": {"subtype": "interrupt"},
        }))
    }

    /// The bounded event stream. Poll with `try_recv`/`try_iter`; the UI
    /// drains this per frame.
    pub fn events(&self) -> &Receiver<SessionEvent> {
        &self.events
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
        let mut child = lock(&self.child);
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn read_stdout(
    stdout: ChildStdout,
    sender: SyncSender<SessionEvent>,
    child: Arc<Mutex<Child>>,
    stderr_tail: Arc<Mutex<StderrTail>>,
) {
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            // Lossy rather than strict: a byte the CLI mangles must not end a
            // Session.
            let text = String::from_utf8_lossy(&line);
            if let Some(event) = wire::parse_line(text.trim_end()) {
                // A full channel parks this thread, the OS pipe fills, and the
                // CLI blocks on its own write. Backpressure, never loss.
                if sender.send(event).is_err() {
                    return;
                }
            }
        }
        let _ = sender.send(closed_event(&child, &stderr_tail));
    });
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

fn spawn_error(program: &str, e: io::Error) -> SpawnError {
    if e.kind() == io::ErrorKind::NotFound {
        SpawnError::CliNotFound {
            program: program.to_string(),
        }
    } else {
        SpawnError::Io(e)
    }
}

fn check_version(program: &str) -> Result<(), SpawnError> {
    let output = Command::new(program)
        .arg("--version")
        .output()
        .map_err(|e| spawn_error(program, e))?;
    if !output.status.success() {
        return Err(SpawnError::VersionCheckFailed {
            detail: format!("`{program} --version` {}", output.status),
        });
    }

    let reported = String::from_utf8_lossy(&output.stdout);
    let Some((found, version)) = parse_version(&reported) else {
        return Err(SpawnError::VersionCheckFailed {
            detail: format!(
                "unrecognised `{program} --version` output: {:?}",
                reported.trim()
            ),
        });
    };
    if version < CLAUDE_CLI_MIN_VERSION {
        return Err(SpawnError::CliVersionUnmet {
            found,
            required: MIN_VERSION_DISPLAY,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_displayed_minimum_matches_the_pin() {
        assert_eq!(
            parse_version(MIN_VERSION_DISPLAY).map(|(_, v)| v),
            Some(CLAUDE_CLI_MIN_VERSION)
        );
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
