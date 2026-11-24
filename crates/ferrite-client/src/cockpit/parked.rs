//! The Threads not open, as every window lists them (ADR 0026): the rows
//! its Server last published. Nothing here reads a log — on a machine the
//! logs are not here, and on this computer a log read per row held the UI
//! thread for hundreds of milliseconds with a few hundred parked Threads.

use std::collections::BTreeMap;
use std::io;

use ferrite_core::header::{LoadError, ThreadMeta};
use ferrite_core::protocol::{ParkedThread, ServerMessage, Snapshot};
use ferrite_core::ThreadId;

#[derive(Default)]
pub(crate) struct Parked {
    /// By Thread, which is creation order.
    rows: BTreeMap<ThreadId, ParkedThread>,
    /// The rows moved since the window last asked.
    changed: bool,
    /// Its Server has said which Threads it parked.
    heard: bool,
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
        self.heard = true;
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

    /// Whether `thread` is parked: `None` until its Server has said which
    /// Threads it parked.
    pub(crate) fn has(&self, thread: ThreadId) -> Option<bool> {
        self.heard.then(|| self.rows.contains_key(&thread))
    }

    /// A row's meta says no model, effort or summary: the window reads
    /// none of a parked Thread.
    pub(crate) fn peek(&self, thread: ThreadId) -> Result<ThreadMeta, LoadError> {
        let Some((row, Some(provider))) = self.rows.get(&thread).map(|row| (row, row.provider))
        else {
            return Err(unsaid(thread));
        };
        Ok(ThreadMeta {
            snooze: None,
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
            owner: row.owner.clone(),
            delegated_from: None,
            request: None,
            cap: None,
            pr_watch: Default::default(),
            delegates: Default::default(),
            summary: None,
        })
    }

    /// When its log was last written, in milliseconds since the epoch on
    /// its Server's clock.
    pub(crate) fn last_used_ms(&self, thread: ThreadId) -> Option<i64> {
        let ms = self.rows.get(&thread)?.last_used_ms?;
        Some(i64::try_from(ms).unwrap_or(i64::MAX))
    }

    /// The first prompt, shortened: what an untitled Thread is called.
    pub(crate) fn prompt_title(&self, thread: ThreadId) -> Option<String> {
        self.rows.get(&thread)?.prompt_title.clone()
    }

    /// Whose a parked Thread is when not the operator's (ADR 0037).
    pub(crate) fn owner(&self, thread: ThreadId) -> Option<ferrite_core::ThreadOwner> {
        self.rows.get(&thread)?.owner.clone()
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
