//! A Thread's Header as a window reads it without its history: the facts
//! a parked row shows (`ThreadMeta`) and the counts it adds
//! (`ThreadSummary`), and why reading a Thread's log failed
//! (`LoadError`). The Store that writes and reads the log is the
//! Server's (`store`).

use std::io;
use std::path::PathBuf;

use crate::checkout::WorkspaceBinding;
use crate::project::ProjectId;
use crate::provider::Provider;

/// Loading one Thread failed. Errors are per-Thread by design: one damaged
/// log must never take the rest of the store down with it.
#[derive(Debug)]
pub enum LoadError {
    /// The log was written by a newer Ferrite than this one. Refused whole:
    /// half-reading a schema from the future would show the operator a
    /// Thread that quietly is not theirs. The operator upgrades Ferrite —
    /// the log is fine.
    FutureSchema {
        found: u32,
        supported: u32,
    },
    /// The log's own header could not be read — the file was damaged at or
    /// before the first line, which only a crash inside `create` can leave
    /// behind. Everything after the header heals silently; the header is the
    /// one line with nothing before it to recover to.
    Corrupt {
        detail: String,
    },
    Io(io::Error),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::FutureSchema { found, supported } => write!(
                f,
                "thread log is schema {found}, newer than the supported {supported}; \
                 upgrade Ferrite"
            ),
            LoadError::Corrupt { detail } => write!(f, "thread log corrupt: {detail}"),
            LoadError::Io(e) => write!(f, "io error loading thread: {e}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<io::Error> for LoadError {
    fn from(e: io::Error) -> Self {
        LoadError::Io(e)
    }
}

/// What a parked Thread's row says without replaying its log (ADR 0009).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSummary {
    /// Prompts since the last conversation reset.
    pub turns: usize,
    /// Whether the operator ever sent a prompt (the provider lock).
    pub prompted: bool,
    /// The subagents the Thread knew when it was last written, when known.
    pub subagents: Option<usize>,
}

/// One Thread's header facts, read without its history — what a nav row can
/// say about a parked Thread (#21). Everything here is the log's first line.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadMeta {
    pub provider: Provider,
    /// `None` for a log from before schema 3, which never recorded one.
    pub workspace: Option<WorkspaceBinding>,
    /// `None` — including every pre-v4 log — means work in the binding
    /// itself.
    pub session_project_root: Option<PathBuf>,
    /// `None` — including every pre-v5 log — means the provider's default
    /// model.
    pub model: Option<String>,
    /// `None` — including every pre-v6 log — means unregistered (#29): the
    /// binding's resolved paths still say where work happens.
    pub project_id: Option<ProjectId>,
    pub title: Option<String>,
    pub snooze: Option<crate::snooze::Snooze>,
    /// `None` — including every pre-v8 log — means the provider's default
    /// reasoning effort.
    pub effort: Option<String>,
    /// Whose the Thread is when not the operator's (ADR 0037).
    pub owner: Option<crate::ThreadOwner>,
    /// The Thread on another machine that delegated it.
    pub delegated_from: Option<crate::DelegatedFrom>,
    /// The key of the `thread.start` that made it.
    pub request: Option<String>,
    /// The most its agent may do without asking.
    pub cap: Option<crate::runtime_mode::RuntimeMode>,
    pub pr_watch: crate::pull_request::WatchFact,
    pub delegates: crate::delegate::Delegates,
    /// The counts a parked row shows, read off the log's tail; `None` for
    /// a log from before schema 13 (see `Store::summary`).
    pub summary: Option<ThreadSummary>,
}
