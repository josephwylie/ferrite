//! One Thread's identity — the key every module files a Thread under.
//!
//! There is exactly one of these because there is exactly one Thread: the
//! store names its log directory with it, the cockpit keys its Panes on it,
//! and a Thread revived after a restart is the same Thread it was before.
//!
//! A `ThreadId` is unique only within its machine's Store: Thread 3 here
//! and Thread 3 on box are two Threads. A window that holds several
//! machines names a Thread by its `ThreadKey`, and saves it as a
//! `SavedThread` (ADR 0045).

use std::sync::atomic::{AtomicU32, Ordering};

use crate::workspace::registry::ProjectId;

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
    pub const fn new(id: u64) -> Self {
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

/// A machine the window holds, for this run: minted as the Cockpit opens
/// its link, and never minted again, so a key from a machine that was
/// removed names nothing rather than a Thread of the next one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MachineId(u32);

impl MachineId {
    pub(crate) fn mint() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// A Thread and the machine it lives on. Only core makes one, stamping
/// the machine whose Server reported the Thread, so a key can never pair
/// a Thread with another machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThreadKey {
    machine: MachineId,
    thread: ThreadId,
}

impl ThreadKey {
    pub(crate) fn new(machine: MachineId, thread: ThreadId) -> Self {
        Self { machine, thread }
    }

    /// A key for a test with no Cockpit behind it.
    #[cfg(any(test, feature = "test-support"))]
    pub fn fixture(machine: u32, thread: u64) -> Self {
        Self::new(MachineId(machine), ThreadId::new(thread))
    }

    pub fn machine(self) -> MachineId {
        self.machine
    }

    pub fn thread(self) -> ThreadId {
        self.thread
    }
}

/// A Project and the machine it is registered on: a `ProjectId` is per
/// machine too, so the same repo on two machines is two Projects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectKey {
    machine: MachineId,
    project: ProjectId,
}

impl ProjectKey {
    pub(crate) fn new(machine: MachineId, project: ProjectId) -> Self {
        Self { machine, project }
    }

    pub fn machine(self) -> MachineId {
        self.machine
    }

    pub fn project(self) -> ProjectId {
        self.project
    }
}

/// A machine as it is saved and as alerts name it (ADR 0017): its target,
/// which an index moving in Settings does not change. `None` is this
/// computer.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct MachineKey(pub Option<String>);

/// A Thread as it is saved across runs: a `MachineId` lasts one run, so
/// the machine is named by its `MachineKey`.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct SavedThread {
    pub machine: MachineKey,
    pub thread: ThreadId,
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

/// The Thread on another machine whose agent started this one to do a
/// piece of work (a Delegate): that machine's Server, the label
/// it knows this machine by, and its Thread by id and title. Written with
/// the Thread and kept for its whole life; a fork is the operator's own
/// and carries none.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct DelegatedFrom {
    /// The delegating Thread's Server (`welcome`'s id).
    #[schemars(with = "String")]
    pub server: crate::server::ServerId,
    /// What the delegating machine is called there.
    pub label: String,
    /// The delegating Thread, by that Server's id for it.
    pub thread: ThreadId,
    pub title: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_thread_reads_back_as_it_was_written() {
        for machine in [MachineKey(None), MachineKey(Some("me@box".into()))] {
            let saved = SavedThread {
                machine,
                thread: ThreadId::new(3),
            };
            let json = serde_json::to_string(&saved).unwrap();
            assert_eq!(serde_json::from_str::<SavedThread>(&json).unwrap(), saved);
        }
        assert_eq!(
            serde_json::to_string(&SavedThread {
                machine: MachineKey(Some("me@box".into())),
                thread: ThreadId::new(3),
            })
            .unwrap(),
            r#"{"machine":"me@box","thread":3}"#
        );
    }
}
