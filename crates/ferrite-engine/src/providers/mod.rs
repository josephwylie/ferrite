//! Provider Sessions: one implementation per agent backend, end-to-end
//! (process + protocol + quirks). No shared transport middle layer unless
//! real duplication demands one.

use std::io;
use std::sync::mpsc::{sync_channel, Receiver};

use ferrite_core::activity::AgentKey;
use ferrite_core::prompt_files::FileSuggestion;
use ferrite_core::{ControlKind, DecisionAnswer, PermissionModeChoice, SessionControl, SessionEvent};

mod claude;
pub mod clis;
mod codex;
pub mod commands;
pub mod discover;
mod elicitation;
pub mod limits;
pub(crate) mod model_cache;
pub(crate) mod oneshot;
mod plugins;
mod resume;
pub mod update;
// The Win32 calls are cfg(windows); the pid-selection logic inside is pure
// and part of the host suite, like `spawn::cmd_shim`.
#[cfg(any(windows, test))]
mod job;

/// Read a provider's next stdout line into `line`, replacing what it held.
/// Answers the bytes read; 0 at the end of the stream. A buffer a rare
/// huge line (a large tool result) grew past `LINE_KEEP` is released to a
/// working size first, so it is not held for the rest of the Session.
/// Rings the window's bell first: what the last line handed on (an event,
/// or an ack on a channel of the Session's own) is there to read.
pub(crate) fn read_line(reader: &mut impl io::BufRead, line: &mut Vec<u8>) -> io::Result<usize> {
    ferrite_core::doorbell::ring();
    line.clear();
    if line.capacity() > LINE_KEEP {
        line.shrink_to(LINE_START);
    }
    reader.read_until(b'\n', line)
}

/// The most a provider's line buffer keeps between lines, and the size it
/// returns to once a line has grown it past that.
const LINE_KEEP: usize = 1024 * 1024;
const LINE_START: usize = 64 * 1024;

/// The command that runs `program` through `launcher` (`bwrap … --`,
/// `thread.start --launcher`): the launcher's words, then the program;
/// the program itself without one. Its arguments follow. An agent's
/// child (`spawn::agent`), whichever runs first.
pub(crate) fn launched(launcher: &[String], program: &str) -> std::process::Command {
    match launcher.split_first() {
        Some((first, rest)) => {
            let mut command = ferrite_core::spawn::agent(first);
            command.args(rest).arg(program);
            command
        }
        None => ferrite_core::spawn::agent(program),
    }
}

/// A setting change's answer: `Ok` once the change holds for the next
/// turn. A Session that keeps the setting itself has answered by the time
/// it hands this back; one whose provider must accept it answers when the
/// provider does, and nobody waits on it.
pub type SettingAck = Receiver<io::Result<()>>;

/// The answer to a change the Session applied itself.
pub fn applied() -> SettingAck {
    let (answer, ack) = sync_channel(1);
    let _ = answer.send(Ok(()));
    ack
}

/// A live provider Session, whatever backend serves it. Each provider
/// implements this end to end — process, protocol and quirks — and nothing
/// above this line knows which one it is holding.
///
/// No park: a Thread is parked by dropping its Session, which is the whole
/// lifecycle the caller needs and the only one a provider can honour.
pub trait Session {
    /// Provider-ranked file matches, answered on `reply`. Replies are
    /// transient, correlated to this request, and never enter the
    /// transcript. Unsupported Sessions let the caller use local discovery.
    fn search_files(&mut self, _query: &str, _reply: FileSearchReply) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native file search unavailable",
        ))
    }
    fn supports_control(&self, _kind: ControlKind) -> bool {
        false
    }
    fn control(&mut self, _action: SessionControl) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this Session does not support that control",
        ))
    }
    /// The bounded event stream. The pump drains this per frame.
    fn events(&self) -> &Receiver<SessionEvent>;
    fn send(&mut self, text: &str) -> io::Result<()>;
    /// Send a prompt under Ferrite's id `id`, what a rewind or a fork names
    /// it by (`rewind`, `fork`). A provider that names prompts itself just
    /// sends, and maps `id` to its own name for them.
    fn send_as(&mut self, _id: &str, text: &str) -> io::Result<()> {
        self.send(text)
    }
    /// Cut the conversation back to before the prompt `request` names,
    /// answered on the ack once the provider has (`Rewound`).
    fn rewind(&mut self, _request: RewindRequest) -> io::Result<RewindAck> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this agent cannot rewind its conversation",
        ))
    }
    /// Where a new conversation holding this one up to the prompt
    /// `request.before` names, without it — or the whole of it — comes
    /// from (`Forked`).
    fn fork(&mut self, _request: ForkRequest) -> io::Result<ForkAck> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this agent cannot fork its conversation",
        ))
    }
    fn enqueue(&mut self, _client_id: &str, _text: &str) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native queuing is unavailable in this Session",
        ))
    }
    /// Whether input enqueued while a turn runs steers that turn — folded
    /// in at its next tool boundary — rather than waiting for the next
    /// one: then a queued prompt sent now needs no interrupt.
    fn steers(&self) -> bool {
        false
    }
    /// Whether what waits in its own queue while a turn runs joins that
    /// turn at its next tool round (`Delivery::Folding`), not the next.
    fn queue_folds(&self) -> bool {
        false
    }
    fn cancel_queued(&mut self, _id: &str) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native queue cancellation is unavailable",
        ))
    }
    /// Change effort for the next turn without replacing this Session or
    /// its conversation. `None` restores the provider's configured default.
    fn set_effort(&mut self, _effort: Option<&str>) -> io::Result<SettingAck> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this Session cannot change effort",
        ))
    }
    /// Select the model for subsequent turns on this Session.
    fn set_model(&mut self, _model: Option<&str>) -> io::Result<SettingAck> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this Session cannot change model",
        ))
    }
    fn permission_modes(&self) -> Vec<PermissionModeChoice> {
        Vec::new()
    }
    /// The sandbox the provider says this Session runs in, in `read-only`,
    /// `workspace-write` or `danger-full-access` words. `None` when the
    /// provider has none or has not said: a default is never guessed.
    fn sandbox(&self) -> Option<String> {
        None
    }
    /// Native follow-ups are optional; providers without them use the
    /// one-shot prediction path. These never enter the durable transcript.
    fn set_suggestions_enabled(&mut self, _enabled: bool) -> io::Result<()> {
        Ok(())
    }
    fn take_suggestion(&mut self) -> Option<String> {
        None
    }
    /// Interrupt Main's turn, and only that: Subagents, background tasks and
    /// workflows keep running (stopping one is `stop_subagent` or
    /// `SessionControl::StopTask`).
    fn interrupt(&mut self) -> io::Result<()>;
    /// Stop one Subagent, leaving Main and every other Subagent running.
    fn stop_subagent(&mut self, _key: &AgentKey) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this Session cannot stop a subagent",
        ))
    }
    fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()>;

    /// Tell the provider what the Thread is now called, so its own session
    /// list agrees with Ferrite's.
    fn set_name(&mut self, _name: &str) -> io::Result<()> {
        Ok(())
    }

    /// The process whoever is watching memory should meter. `None` when
    /// there is no process — a scripted Session in a test, for instance.
    /// Providers answer the process worth metering: under a Windows `.cmd`
    /// shim that is the CLI beneath the cmd.exe wrapper, not the ~5MB
    /// wrapper itself.
    fn pid(&self) -> Option<u32> {
        None
    }
}

/// What a rewind asks of a Session (`Session::rewind`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewindRequest {
    /// The prompt to go back to before, by the id it was sent under.
    pub prompt: String,
    /// The latest prompt the operator saw, when it has an id: a provider
    /// that heard a later one refuses rather than cut what was not seen.
    pub latest: Option<String>,
    /// Restore the files the provider checkpointed to how they were when
    /// the prompt was sent, before the conversation is cut.
    pub files: bool,
    /// The provider's id of the conversation, as its Init named it.
    pub session: Option<String>,
}

/// A fork the provider is asked where to start (`Session::fork`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkRequest {
    /// The prompt the fork stops before, by the id it was sent under;
    /// `None` takes the whole conversation.
    pub before: Option<String>,
    /// The provider's id of the conversation, as its Init named it.
    pub session: Option<String>,
}

/// What the provider made of a rewind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewound {
    pub cut: Cut,
    /// What restoring the files changed, when they were asked for and the
    /// restore ran.
    pub files: Option<FilesRestored>,
}

/// How the conversation was cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cut {
    /// In place, in the live Session.
    InPlace,
    /// Not in place: the next Session resumes `session` at its entry `at`
    /// instead (`ResumePoint`), and this one should end.
    ResumeAt { session: String, at: String },
    /// Refused, in the provider's words; nothing was cut.
    Refused(String),
}

/// What a file restore changed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilesRestored {
    pub files: usize,
    pub insertions: u64,
    pub deletions: u64,
}

pub type RewindAck = Receiver<io::Result<Rewound>>;

/// Where a fork's conversation comes from (`Session::fork`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Forked {
    /// A new provider conversation already made, by its id.
    Thread(String),
    /// Made by the next Session: `session` resumed at its entry `at` (at
    /// its end for `None`), forked (`ResumePoint`).
    ResumeAt { session: String, at: Option<String> },
    /// Nothing before the prompt: the fork starts a conversation afresh.
    Fresh,
}

pub type ForkAck = Receiver<io::Result<Forked>>;

/// Where a file search is answered (`Session::search_files`): the answer
/// rings the window's bell (ADR 0041).
pub type FileSearchReply = ferrite_core::doorbell::SyncSender<io::Result<Vec<FileSuggestion>>>;

impl Session for ClaudeSession {
    fn search_files(&mut self, query: &str, reply: FileSearchReply) -> io::Result<()> {
        ClaudeSession::search_files(self, query, reply)
    }

    fn supports_control(&self, kind: ControlKind) -> bool {
        ClaudeSession::supports_control(self, kind)
    }
    fn control(&mut self, action: SessionControl) -> io::Result<()> {
        ClaudeSession::control(self, action)
    }
    fn enqueue(&mut self, client_id: &str, text: &str) -> io::Result<()> {
        ClaudeSession::enqueue(self, client_id, text)
    }
    /// An `isAsync` message sent while a turn runs is folded into it
    /// between tool rounds (`claude-steer-2.1.318`: its lifecycle starts
    /// mid-turn, the turn's `user_message_uuids` name it, one result ends
    /// both); with no further tool round it runs as the next turn.
    fn steers(&self) -> bool {
        true
    }
    fn queue_folds(&self) -> bool {
        true
    }
    fn cancel_queued(&mut self, id: &str) -> io::Result<()> {
        ClaudeSession::cancel_queued(self, id)
    }
    fn set_suggestions_enabled(&mut self, enabled: bool) -> io::Result<()> {
        ClaudeSession::set_suggestions_enabled(self, enabled)
    }
    fn take_suggestion(&mut self) -> Option<String> {
        ClaudeSession::take_suggestion(self)
    }

    fn set_effort(&mut self, effort: Option<&str>) -> io::Result<SettingAck> {
        ClaudeSession::set_effort(self, effort)
    }
    fn set_model(&mut self, model: Option<&str>) -> io::Result<SettingAck> {
        ClaudeSession::set_model(self, model)
    }
    fn permission_modes(&self) -> Vec<PermissionModeChoice> {
        ClaudeSession::permission_modes(self)
    }
    fn set_name(&mut self, name: &str) -> io::Result<()> {
        ClaudeSession::set_name(self, name)
    }
    fn events(&self) -> &Receiver<SessionEvent> {
        ClaudeSession::events(self)
    }

    fn send(&mut self, text: &str) -> io::Result<()> {
        ClaudeSession::send(self, text)
    }
    fn send_as(&mut self, id: &str, text: &str) -> io::Result<()> {
        ClaudeSession::send_as(self, id, text)
    }
    fn rewind(&mut self, request: RewindRequest) -> io::Result<RewindAck> {
        ClaudeSession::rewind(self, request)
    }
    fn fork(&mut self, request: ForkRequest) -> io::Result<ForkAck> {
        ClaudeSession::fork(self, request)
    }

    fn interrupt(&mut self) -> io::Result<()> {
        ClaudeSession::interrupt(self)
    }

    fn stop_subagent(&mut self, key: &AgentKey) -> io::Result<()> {
        ClaudeSession::stop_subagent(self, key)
    }

    fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()> {
        ClaudeSession::respond_to_decision(self, id, answer)
    }

    fn pid(&self) -> Option<u32> {
        ClaudeSession::pid(self)
    }
}

impl Session for CodexSession {
    fn search_files(&mut self, query: &str, reply: FileSearchReply) -> io::Result<()> {
        CodexSession::search_files(self, query, reply)
    }

    fn permission_modes(&self) -> Vec<PermissionModeChoice> {
        CodexSession::permission_modes(self)
    }

    fn sandbox(&self) -> Option<String> {
        CodexSession::sandbox(self)
    }

    fn supports_control(&self, kind: ControlKind) -> bool {
        CodexSession::supports_control(self, kind)
    }
    fn control(&mut self, action: SessionControl) -> io::Result<()> {
        CodexSession::control(self, action)
    }
    fn enqueue(&mut self, client_id: &str, text: &str) -> io::Result<()> {
        CodexSession::enqueue(self, client_id, text)
    }
    /// `turn/steer`: what is enqueued while a turn runs joins it.
    fn steers(&self) -> bool {
        true
    }
    fn cancel_queued(&mut self, id: &str) -> io::Result<()> {
        CodexSession::cancel_queued(self, id)
    }
    fn set_effort(&mut self, effort: Option<&str>) -> io::Result<SettingAck> {
        CodexSession::set_effort(self, effort).map(|()| applied())
    }
    fn set_model(&mut self, model: Option<&str>) -> io::Result<SettingAck> {
        CodexSession::set_model(self, model).map(|()| applied())
    }
    fn events(&self) -> &Receiver<SessionEvent> {
        CodexSession::events(self)
    }

    fn send(&mut self, text: &str) -> io::Result<()> {
        CodexSession::send(self, text)
    }
    fn send_as(&mut self, id: &str, text: &str) -> io::Result<()> {
        CodexSession::send_as(self, id, text)
    }
    fn rewind(&mut self, request: RewindRequest) -> io::Result<RewindAck> {
        CodexSession::rewind(self, request)
    }
    fn fork(&mut self, request: ForkRequest) -> io::Result<ForkAck> {
        CodexSession::fork(self, request)
    }

    fn interrupt(&mut self) -> io::Result<()> {
        CodexSession::interrupt(self)
    }

    fn stop_subagent(&mut self, key: &AgentKey) -> io::Result<()> {
        CodexSession::stop_subagent(self, key)
    }

    fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()> {
        CodexSession::respond_to_decision(self, id, answer)
    }

    fn set_name(&mut self, name: &str) -> io::Result<()> {
        CodexSession::set_name(self, name)
    }

    fn pid(&self) -> Option<u32> {
        CodexSession::pid(self)
    }
}
pub use claude::discovery::list as claude_sessions;
/// What installs the plugins a setup sync turns on (ADR 0050).
pub use plugins::installs as plugin_installs;
pub use resume::{ResumeFailure, ResumeRefused};
/// Each Provider's title filler, beside its Session — the titler runs
/// whichever the Thread's Provider is.
pub use claude::title as claude_title;
pub use claude::{
    ClaudeCapabilities, ClaudeConfig, ClaudeSession, ClaudeSpawnError, CLAUDE_CLI_MIN_VERSION,
    CLAUDE_CLI_TESTED_BELOW,
};
pub use codex::catalog::list as codex_models;
pub use codex::discovery::list as codex_sessions;
pub(crate) use codex::is_steer_id;
pub(crate) use codex::launch as codex_launch;
pub use codex::title as codex_title;
pub use codex::{
    CodexCapabilities, CodexConfig, CodexSession, CodexSpawnError, CODEX_CLI_MIN_VERSION,
    CODEX_CLI_TESTED_BELOW,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// One huge line (a large tool result) must not keep its buffer for
    /// the rest of the Session's life.
    #[test]
    fn a_huge_line_does_not_keep_its_buffer_for_the_lines_after_it() {
        let huge = "x".repeat(5 * 1024 * 1024);
        let stream = format!("{huge}\nsmall\nsmaller\n");
        let mut reader = io::BufReader::new(stream.as_bytes());
        let mut line = Vec::new();
        assert_eq!(read_line(&mut reader, &mut line).unwrap(), huge.len() + 1);
        assert_eq!(read_line(&mut reader, &mut line).unwrap(), 6);
        assert_eq!(line, b"small\n");
        assert!(
            line.capacity() <= 1024 * 1024,
            "a {} byte buffer kept after the huge line",
            line.capacity()
        );
        assert_eq!(read_line(&mut reader, &mut line).unwrap(), 8);
        assert_eq!(read_line(&mut reader, &mut line).unwrap(), 0);
    }

}
