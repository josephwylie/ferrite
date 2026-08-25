//! Claude provider: the pinned `claude` CLI spoken over stdio stream-json.
//!
//! Spawn checks the CLI version pin before any process starts a conversation;
//! a reader thread parses stdout lines into SessionEvents on a bounded
//! channel. Backpressure is stated and simple: when the channel is full the
//! reader thread blocks, the pipe fills, and the CLI stalls — nothing is
//! dropped.

use std::io;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use crate::SessionEvent;

/// Minimum `claude` CLI version for stable stream-json + stdio control
/// protocol. Vendor releases below this break loudly at spawn, not weirdly
/// mid-conversation.
pub const CLAUDE_CLI_MIN_VERSION: [u64; 3] = [2, 1, 224];

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
    CliNotFound { program: String },
    /// The CLI is older than the pin.
    CliVersionUnmet {
        found: String,
        required: &'static str,
    },
    /// `--version` ran but produced nothing parseable.
    VersionCheckFailed { detail: String },
    Io(io::Error),
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnError::CliNotFound { program } => {
                write!(f, "claude CLI not found: `{program}`")
            }
            SpawnError::CliVersionUnmet { found, required } => {
                write!(f, "claude CLI {found} is older than the pinned minimum {required}")
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
    _todo: (),
}

impl ClaudeSession {
    /// Version-check the CLI, then spawn it in stream-json mode.
    pub fn spawn(config: ClaudeConfig) -> Result<Self, SpawnError> {
        let _ = config;
        todo!("implemented by the providers slice")
    }

    /// Send one user prompt; the CLI starts (or queues) a turn.
    pub fn send(&mut self, text: &str) -> io::Result<()> {
        let _ = text;
        todo!("implemented by the providers slice")
    }

    /// Interrupt the running turn over the stdio control protocol.
    pub fn interrupt(&mut self) -> io::Result<()> {
        todo!("implemented by the providers slice")
    }

    /// The bounded event stream. Poll with `try_recv`/`try_iter`; the UI
    /// drains this per frame.
    pub fn events(&self) -> &Receiver<SessionEvent> {
        todo!("implemented by the providers slice")
    }
}
