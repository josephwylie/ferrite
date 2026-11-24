//! What a test in another crate reads and bends of the Engine's insides.
//! The window model's tests (`ferrite-client`) drive a real Engine and
//! need these few seams; nothing else may reach past the Engine's own API.

use super::{history, ActivityInput, Engine, Subject};
use crate::store::Store;
use crate::suggest::Suggestion;
use crate::ThreadId;

impl Engine {
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Hold every history read until the returned release is called with
    /// the Store to read from.
    pub fn gate_history(&mut self) -> impl FnOnce(&Store) {
        let (loader, finish) = history::gated_loader();
        self.history_loader = Some(loader);
        finish
    }

    pub fn history_read_pending(&self, thread: ThreadId) -> bool {
        !self.threads[&thread].history.is_empty()
    }

    pub fn has_session(&self, thread: ThreadId) -> bool {
        self.threads[&thread].session.is_some()
    }

    pub fn generation(&self, thread: ThreadId) -> u64 {
        self.threads[&thread].generation
    }

    pub fn suggestion_revision(&self, thread: ThreadId) -> u64 {
        self.threads[&thread].suggestion_revision
    }

    /// Drop a subject's loaded history, as memory pressure does.
    pub fn evict_history(&mut self, thread: ThreadId, subject: &Subject) {
        self.threads
            .get_mut(&thread)
            .unwrap()
            .activity
            .apply(ActivityInput::Evict(subject.clone()));
    }

    /// A prediction arriving from the suggester, as its worker sends one.
    pub fn land_suggestion(&self, suggestion: Suggestion) {
        self.suggestions.0.send(suggestion).unwrap();
    }

    /// Report this Thread's next park as refused.
    pub fn refuse_park(&mut self, thread: ThreadId) {
        self.refuse_park.insert(thread);
    }

    /// Make every later write to this Thread's log fail.
    pub fn break_writes(&self, thread: ThreadId) {
        self.threads[&thread].writer.break_writes();
    }
}
