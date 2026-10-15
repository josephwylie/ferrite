//! A Server the window is not showing (ADR 0026): attached and kept
//! attached like the shown one, but it watches no Thread, so what streams
//! is each open Thread's state, the Decisions and the Notices — never a
//! transcript. What the nav says of a machine the operator is not on, and
//! what the bell hears of it.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::PathBuf;

use super::remote::{Arrival, Attach, Endpoint, Remote, ServerState};
use crate::activity::DecisionHandle;
use crate::engine::{SpawnRequest, Spawner};
use crate::notifications::Notifications;
use crate::protocol::{Peer, ServerMessage, Snapshot, ThreadState, WireDecision, WireHandle};
use crate::store::Provider;
use crate::ThreadId;

pub struct ServerStatus {
    remote: Remote,
    /// The open Threads, in the Roster's order.
    open: Vec<ThreadId>,
    states: HashMap<ThreadId, ThreadState>,
    /// The pending Decisions.
    waiting: BTreeMap<(ThreadId, WireHandle), WireDecision>,
    /// The Server's Notices, read-only; `None` until the first snapshot.
    notifications: Option<Notifications>,
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
            waiting: BTreeMap::new(),
            notifications: None,
            revision: 0,
        }
    }

    /// Take what arrived. True when a row, the Notices or the connection
    /// changed.
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
                    waiting: self.waiting.keys().any(|(waits, _)| waits == thread),
                }
            })
            .collect()
    }

    /// The Server's Notices and live requests, as its windows read them;
    /// `None` until it was first heard from. A Server that went keeps the
    /// ones it last had.
    pub fn notifications(&self) -> Option<&Notifications> {
        self.notifications.as_ref()
    }

    /// An open Thread's title, as far as it has one.
    pub fn title(&self, thread: ThreadId) -> Option<&str> {
        self.states.get(&thread)?.title.as_deref()
    }

    pub fn provider(&self, thread: ThreadId) -> Option<Provider> {
        Some(self.states.get(&thread)?.provider)
    }

    /// A pending Decision, as it was asked.
    pub fn decision(&self, thread: ThreadId, handle: &DecisionHandle) -> Option<&WireDecision> {
        self.waiting.get(&(thread, handle.into()))
    }

    /// Fixtures only: take `message` as if the Server had sent it, a
    /// `Snapshot` as an attach's.
    pub fn fixture_receive(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Snapshot(snapshot) => self.reset(snapshot),
            message => {
                self.heard(message);
            }
        }
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
            .map(|pending| ((pending.thread, pending.handle), pending.decision))
            .collect();
        self.notifications = Some(Notifications::from_wire(
            snapshot.notices.as_deref().unwrap_or_default(),
        ));
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
            ServerMessage::Notices { revision, notices } if revision >= self.revision => {
                self.notifications = Some(Notifications::from_wire(&notices));
                true
            }
            ServerMessage::Decision {
                thread,
                handle,
                decision,
                ..
            } => self.waiting.insert((thread, handle), decision).is_none(),
            ServerMessage::DecisionClosed { thread, handle, .. } => {
                self.waiting.remove(&(thread, handle)).is_some()
            }
            _ => false,
        }
    }
}
