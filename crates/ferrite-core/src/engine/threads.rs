//! The open Threads, and which of them may have changed since their
//! windows were last told their state. Every way to change a Thread goes
//! through here and says so, so `Engine::publish` builds the state of only
//! those: a Thread nothing happened to costs nothing to publish, however
//! many are open. Reads are the map's own.

use std::collections::BTreeMap;
use std::ops::Deref;

use super::Thread;
use crate::ThreadId;

#[derive(Default)]
pub(super) struct Threads(BTreeMap<ThreadId, Thread>);

impl Deref for Threads {
    type Target = BTreeMap<ThreadId, Thread>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Threads {
    /// A Thread to change: its state is told again.
    pub fn get_mut(&mut self, id: &ThreadId) -> Option<&mut Thread> {
        let thread = self.0.get_mut(id)?;
        thread.state_changed = true;
        Some(thread)
    }

    /// A Thread to change only when `wants` says so of it as it is: one
    /// it does not want is left as told.
    pub fn get_mut_if(
        &mut self,
        id: &ThreadId,
        wants: impl FnOnce(&Thread) -> bool,
    ) -> Option<&mut Thread> {
        if self.0.get(id).is_some_and(wants) {
            self.get_mut(id)
        } else {
            None
        }
    }

    /// Every Thread, to change each: every state is told again.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Thread> {
        for thread in self.0.values_mut() {
            thread.state_changed = true;
        }
        self.0.values_mut()
    }

    /// Every Thread, to look at each and change some: the walker sets
    /// `Thread::state_changed` on each it changes. For the walks of every
    /// pass, which change few Threads or none.
    pub fn walk_mut(&mut self) -> impl Iterator<Item = (&ThreadId, &mut Thread)> {
        self.0.iter_mut()
    }

    pub fn insert(&mut self, id: ThreadId, mut thread: Thread) {
        thread.state_changed = true;
        self.0.insert(id, thread);
    }

    pub fn remove(&mut self, id: &ThreadId) -> Option<Thread> {
        self.0.remove(id)
    }

    /// A Thread's state changed by what the Engine holds beside it (a
    /// Draft starting it): it is told again.
    pub fn mark_changed(&mut self, id: ThreadId) {
        self.get_mut(&id);
    }

    /// The Threads changed since the last call, in order; from here on
    /// each is as told.
    pub fn take_changed(&mut self) -> Vec<ThreadId> {
        self.0
            .iter_mut()
            .filter_map(|(&id, thread)| std::mem::take(&mut thread.state_changed).then_some(id))
            .collect()
    }
}
