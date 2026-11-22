//! A server wake: a prompt the Server sends a Thread's agent on its own,
//! with no one at the Composer. A watched PR's news is one (T3 Code's
//! `creationSource: "server"` message with a notification); a Delegate's
//! result is the other (ADR 0047).
//!
//! It reaches the agent as any prompt does, but it is not the operator's:
//! the log keeps it as a `wake` record, never a `prompt`, and the
//! transcript marks its block with [`WakeNote`], so a window can draw it as
//! the compact row T3 draws (`#426: checks failed`) instead of the
//! operator's bubble.

use serde::{Deserialize, Serialize};

/// One wake: what the agent is told, and the line a notice shows for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wake {
    pub text: String,
    pub outcome: Outcome,
    /// `#426: checks failed, new comments`.
    pub summary: String,
    pub source: Source,
}

impl Wake {
    pub fn note(&self) -> WakeNote {
        WakeNote {
            summary: self.summary.clone(),
            outcome: self.outcome,
            source: self.source,
        }
    }
}

/// What a wake's notice row draws: its line, how it reads the news, and
/// whose news it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakeNote {
    pub summary: String,
    pub outcome: Outcome,
    pub source: Source,
}

/// Whose news a wake carries (T3's notification `source.kind`): its row's
/// icon says it, an eye for a watch (`monitor`) and a bot for Delegates
/// (`delegated_task`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// A wake logged before a wake said whose it was was a watch's.
    #[default]
    PrWatch,
    Delegates,
}

impl Source {
    pub fn is_pr_watch(&self) -> bool {
        *self == Self::PrWatch
    }
}

/// How the notice reads the news (T3's notification outcome).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Failed,
    Completed,
    Updated,
}
