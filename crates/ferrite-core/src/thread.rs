//! One Thread's identity — the key every module files a Thread under.
//!
//! There is exactly one of these because there is exactly one Thread: the
//! store names its log directory with it, the cockpit keys its Panes on it,
//! and a Thread revived after a restart is the same Thread it was before.

/// A Thread's identity. The store numbers Threads as it creates them; tests
/// mint their own.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct ThreadId(u64);

impl ThreadId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for ThreadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Whose a Thread is when it is not the operator's own (ADR 0037): a
/// Worker is its Task's, a Coordinator its Project's Factory's. Written
/// with the Thread and kept for its whole life. An owned Thread is an
/// ordinary Thread that the operator's own lists (the nav, the palette's
/// Thread search, the Inbox, the Attention list) and alerts leave out;
/// the API lists it, naming its owner.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadOwner {
    /// A Worker: the Task it runs a Step for, by the Task's id.
    Task(String),
    /// A Coordinator: the Project whose Factory it directs.
    Factory(crate::workspace::registry::ProjectId),
}
