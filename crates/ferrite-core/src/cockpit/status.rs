//! A Server the window is not showing (ADR 0026): attached and kept
//! attached like the shown one, but it watches no Thread, so what streams
//! is each open Thread's state and the Decisions — never a transcript.
//! What the nav says of a machine the operator is not on.

use std::collections::{BTreeSet, HashMap};
use std::io;
use std::path::PathBuf;

use super::remote::{Arrival, Attach, Endpoint, Remote, ServerState};
use crate::engine::{SpawnRequest, Spawner};
use crate::protocol::{Peer, ServerMessage, Snapshot, ThreadState, WireHandle};
use crate::ThreadId;

pub struct ServerStatus {
    remote: Remote,
    /// The open Threads, in the Roster's order.
    open: Vec<ThreadId>,
    states: HashMap<ThreadId, ThreadState>,
    /// The pending Decisions.
    waiting: BTreeSet<(ThreadId, WireHandle)>,
    /// The snapshot's: a state message older than it is dropped.
    revision: u64,
}

/// One open Thread, as the nav lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusRow {
    pub thread: ThreadId,
    pub title: Option<String>,
    pub working: bool,
    /// A Decision waits on the operator.
    pub waiting: bool,
}

/// A status link finds nothing on its own.
struct NoDiscovery;

impl Spawner for NoDiscovery {
    fn spawn(&mut self, _: SpawnRequest) -> io::Result<Box<dyn crate::providers::Session>> {
        Err(io::Error::other("a status link spawns no Session"))
    }
}

impl ServerStatus {
    /// Attached in the background from the first `poll`. `store` is the
    /// Store an `Endpoint::Local` Server serves.
    pub fn new(store: PathBuf, server: Endpoint, client: Peer) -> Self {
        let remote = Remote::new(Attach {
            store,
            server,
            client,
            renderer: None,
            discovery: Box::new(NoDiscovery),
        });
        Self {
            remote,
            open: Vec::new(),
            states: HashMap::new(),
            waiting: BTreeSet::new(),
            revision: 0,
        }
    }

    /// Take what arrived. True when a row or the connection changed.
    pub fn poll(&mut self) -> bool {
        let was = self.remote.state();
        let mut changed = false;
        for arrival in self.remote.poll() {
            changed |= match arrival {
                Arrival::Attached { snapshot, .. } => {
                    self.reset(snapshot);
                    true
                }
                Arrival::Message(message) => self.heard(message),
                Arrival::Started(_) => false,
            };
        }
        changed || self.remote.state() != was
    }

    pub fn server(&self) -> ServerState {
        self.remote.state()
    }

    /// Attach again now, rather than at the next retry.
    pub fn retry(&mut self) {
        self.remote.retry();
    }

    /// Every open Thread, in the Roster's order. A Server that went keeps
    /// the rows it last had.
    pub fn rows(&self) -> Vec<StatusRow> {
        self.open
            .iter()
            .map(|thread| {
                let state = self.states.get(thread);
                StatusRow {
                    thread: *thread,
                    title: state.and_then(|state| state.title.clone()),
                    working: state.is_some_and(|state| state.working || state.starting),
                    waiting: self.waiting.iter().any(|(waits, _)| waits == thread),
                }
            })
            .collect()
    }

    fn reset(&mut self, snapshot: Snapshot) {
        self.revision = snapshot.revision;
        self.open = snapshot.roster.open;
        self.states = snapshot
            .threads
            .into_iter()
            .map(|state| (state.thread, state))
            .collect();
        self.waiting = snapshot
            .decisions
            .into_iter()
            .map(|pending| (pending.thread, pending.handle))
            .collect();
    }

    fn heard(&mut self, message: ServerMessage) -> bool {
        match message {
            ServerMessage::Thread { revision, state } if revision >= self.revision => {
                self.states.insert(state.thread, state);
                true
            }
            ServerMessage::Roster { revision, roster } if revision >= self.revision => {
                self.open = roster.open;
                true
            }
            ServerMessage::Decision { thread, handle, .. } => self.waiting.insert((thread, handle)),
            ServerMessage::DecisionClosed { thread, handle, .. } => {
                self.waiting.remove(&(thread, handle))
            }
            _ => false,
        }
    }
}
