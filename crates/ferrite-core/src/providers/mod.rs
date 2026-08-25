//! Provider Sessions: one implementation per agent backend, end-to-end
//! (process + protocol + quirks). No shared transport middle layer unless
//! real duplication demands one.

use std::io;
use std::sync::mpsc::Receiver;

use crate::{DecisionAnswer, SessionEvent};

mod claude;
mod codex;

/// A live provider Session, whatever backend serves it. Each provider
/// implements this end to end — process, protocol and quirks — and nothing
/// above this line knows which one it is holding.
///
/// No park: a Thread is parked by dropping its Session, which is the whole
/// lifecycle the caller needs and the only one a provider can honour.
pub trait Session {
    /// The bounded event stream. The pump drains this per frame.
    fn events(&self) -> &Receiver<SessionEvent>;
    fn send(&mut self, text: &str) -> io::Result<()>;
    fn interrupt(&mut self) -> io::Result<()>;
    fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()>;

    /// The Session's own process, for whoever is watching memory. `None` when
    /// there is no process — a scripted Session in a test, for instance.
    fn pid(&self) -> Option<u32> {
        None
    }
}

impl Session for ClaudeSession {
    fn events(&self) -> &Receiver<SessionEvent> {
        ClaudeSession::events(self)
    }

    fn send(&mut self, text: &str) -> io::Result<()> {
        ClaudeSession::send(self, text)
    }

    fn interrupt(&mut self) -> io::Result<()> {
        ClaudeSession::interrupt(self)
    }

    fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()> {
        ClaudeSession::respond_to_decision(self, id, answer)
    }

    fn pid(&self) -> Option<u32> {
        ClaudeSession::pid(self)
    }
}

impl Session for CodexSession {
    fn events(&self) -> &Receiver<SessionEvent> {
        CodexSession::events(self)
    }

    fn send(&mut self, text: &str) -> io::Result<()> {
        CodexSession::send(self, text)
    }

    fn interrupt(&mut self) -> io::Result<()> {
        CodexSession::interrupt(self)
    }

    fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()> {
        CodexSession::respond_to_decision(self, id, answer)
    }

    fn pid(&self) -> Option<u32> {
        CodexSession::pid(self)
    }
}
pub use claude::{
    ClaudeCapabilities, ClaudeConfig, ClaudeSession, ClaudeSpawnError,
    CLAUDE_CLI_MAX_VERSION_EXCLUSIVE, CLAUDE_CLI_MIN_VERSION,
};
pub use codex::{
    CodexCapabilities, CodexConfig, CodexSession, CodexSpawnError, CODEX_CLI_MAX_VERSION_EXCLUSIVE,
    CODEX_CLI_MIN_VERSION,
};
