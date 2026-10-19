//! Where a window reads the Threads not open (ADR 0026): this computer's
//! Store, read-only, or — on a machine, whose logs stay there — the rows
//! its Server last published.

use std::io;
use std::time::{Duration, SystemTime};

use crate::engine::{title_from_prompt, LogReader};
use crate::protocol::{ParkedThread, ServerMessage, Snapshot};
use crate::store::{LoadError, Store, ThreadMeta};
use crate::ThreadId;

pub(crate) enum Parked {
    Store(Store),
    /// `changed`: the rows moved since the window last asked.
    Published {
        threads: Vec<ParkedThread>,
        changed: bool,
    },
}

impl Parked {
    /// Nothing yet: a machine's rows before its Server's first snapshot.
    pub(crate) fn published() -> Self {
        Self::Published {
            threads: Vec::new(),
            changed: false,
        }
    }

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
        if let Self::Published { threads, changed } = self {
            *threads = rows.to_vec();
            *changed = true;
        }
    }

    /// Whether the published rows moved since the last ask. A Store says
    /// nothing: the window reads it again at its own moments.
    pub(crate) fn take_changed(&mut self) -> bool {
        match self {
            Self::Store(_) => false,
            Self::Published { changed, .. } => std::mem::take(changed),
        }
    }

    /// The Threads this window can read, in creation order: every one in
    /// the Store; the parked ones the Server published.
    pub(crate) fn thread_ids(&self) -> io::Result<Vec<ThreadId>> {
        match self {
            Self::Store(store) => store.thread_ids(),
            Self::Published { threads, .. } => Ok(threads.iter().map(|row| row.thread).collect()),
        }
    }

    /// A published row's meta says no model or effort: the window reads
    /// neither of a parked Thread.
    pub(crate) fn peek(&self, thread: ThreadId) -> Result<ThreadMeta, LoadError> {
        let threads = match self {
            Self::Store(store) => return store.peek(thread),
            Self::Published { threads, .. } => threads,
        };
        let found = threads.iter().find(|row| row.thread == thread);
        let Some((row, Some(provider))) = found.map(|row| (row, row.provider)) else {
            return Err(LoadError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!("thread {thread}: the machine did not say"),
            )));
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
        match self {
            Self::Store(store) => store.last_used(thread),
            Self::Published { threads, .. } => threads
                .iter()
                .find(|row| row.thread == thread)?
                .last_used_ms
                .map(|ms| SystemTime::UNIX_EPOCH + Duration::from_millis(ms)),
        }
    }

    /// The first prompt, shortened: what an untitled Thread is called.
    pub(crate) fn prompt_title(&self, thread: ThreadId) -> Option<String> {
        match self {
            Self::Store(store) => store
                .peek_first_prompt(thread)
                .ok()
                .flatten()
                .map(|text| title_from_prompt(&text)),
            Self::Published { threads, .. } => threads
                .iter()
                .find(|row| row.thread == thread)?
                .prompt_title
                .clone(),
        }
    }

    pub(crate) fn log_reader(&self) -> LogReader {
        match self {
            Self::Store(store) => LogReader::new(store.clone()),
            Self::Published { threads, .. } => LogReader::published(threads.clone()),
        }
    }
}
