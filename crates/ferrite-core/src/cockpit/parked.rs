//! The Threads not open, as every window lists them (ADR 0026): the rows
//! its Server last published. Nothing here reads a log — on a machine the
//! logs are not here, and on this computer a log read per row held the UI
//! thread for hundreds of milliseconds with a few hundred parked Threads.

use std::collections::BTreeMap;
use std::io;
use std::time::{Duration, SystemTime};

use crate::protocol::{ParkedThread, ServerMessage, Snapshot};
use crate::store::{LoadError, ThreadMeta};
use crate::ThreadId;

#[derive(Default)]
pub(crate) struct Parked {
    /// By Thread, which is creation order.
    rows: BTreeMap<ThreadId, ParkedThread>,
    /// The rows moved since the window last asked.
    changed: bool,
}

impl Parked {
    pub(crate) fn attached(&mut self, snapshot: &Snapshot) {
        if let Some(rows) = &snapshot.parked {
            self.adopt(rows);
        }
    }

    pub(crate) fn receive(&mut self, message: &ServerMessage) {
        if let ServerMessage::Parked { threads, .. } = message {
            self.adopt(threads);
        }
    }

    fn adopt(&mut self, rows: &[ParkedThread]) {
        self.rows = rows.iter().map(|row| (row.thread, row.clone())).collect();
        self.changed = true;
    }

    /// The window deleted `thread`: its row goes now, not when the Server
    /// next publishes.
    pub(crate) fn forget(&mut self, thread: ThreadId) {
        self.rows.remove(&thread);
    }

    /// Whether the published rows moved since the last ask.
    pub(crate) fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// The parked Threads the Server published, in creation order.
    pub(crate) fn thread_ids(&self) -> impl Iterator<Item = ThreadId> + '_ {
        self.rows.keys().copied()
    }

    /// A row's meta says no model, effort or summary: the window reads
    /// none of a parked Thread.
    pub(crate) fn peek(&self, thread: ThreadId) -> Result<ThreadMeta, LoadError> {
        let Some((row, Some(provider))) = self.rows.get(&thread).map(|row| (row, row.provider))
        else {
            return Err(unsaid(thread));
        };
        Ok(ThreadMeta {
            provider,
            workspace: row
                .workspace
                .as_ref()
                .and_then(|binding| binding.try_into().ok()),
            session_project_root: row.session_project_root.clone(),
            model: None,
            project_id: row.project,
            title: row.title.clone(),
            effort: None,
            summary: None,
        })
    }

    pub(crate) fn last_used(&self, thread: ThreadId) -> Option<SystemTime> {
        self.rows
            .get(&thread)?
            .last_used_ms
            .map(|ms| SystemTime::UNIX_EPOCH + Duration::from_millis(ms))
    }

    /// The first prompt, shortened: what an untitled Thread is called.
    pub(crate) fn prompt_title(&self, thread: ThreadId) -> Option<String> {
        self.rows.get(&thread)?.prompt_title.clone()
    }

    /// The turns the Server counted off the log's tail.
    pub(crate) fn turns(&self, thread: ThreadId) -> Option<usize> {
        self.rows.get(&thread)?.turns
    }

    /// The Subagents the Server counted off the log's tail.
    pub(crate) fn subagents(&self, thread: ThreadId) -> Option<usize> {
        self.rows.get(&thread)?.subagents
    }
}

/// What a Thread the Server published no row for says: nothing.
fn unsaid(thread: ThreadId) -> LoadError {
    LoadError::Io(io::Error::new(
        io::ErrorKind::NotFound,
        format!("thread {thread}: the Server did not say"),
    ))
}
