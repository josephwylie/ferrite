//! One Thread's identity — the key every module files a Thread under.
//!
//! There is exactly one of these because there is exactly one Thread: the
//! store names its log directory with it, the cockpit keys its Panes on it,
//! and a Thread revived after a restart is the same Thread it was before.

/// A Thread's identity. The store numbers Threads as it creates them; tests
/// mint their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
