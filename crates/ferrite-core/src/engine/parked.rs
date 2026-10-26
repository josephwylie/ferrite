//! The `parked` message (ADR 0026): every window lists the Threads not
//! open off the rows its Server publishes. A row is read off its log once.
//! It is read again only when its Thread is parked again or changes while
//! parked. The reads run on a thread of their own, not the pump: a row
//! waits for a park's final mark, and a log from before schema 13 is
//! replayed once for its counts. An Engine its window pumps reads them in
//! place.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use crate::protocol::ParkedThread;
use crate::store::Store;
use crate::ThreadId;

pub(super) struct ParkedListing {
    /// The parked Threads' rows as last read.
    rows: BTreeMap<ThreadId, ParkedThread>,
    /// Threads whose rows must be read again.
    stale: BTreeSet<ThreadId>,
    /// The parked set may have moved since it was last listed.
    moved: bool,
    /// The parked Threads, in creation order, as last listed.
    parked: Vec<ThreadId>,
    /// Something changed since the rows were last handed out.
    unpublished: bool,
    reading: Option<Reading>,
    /// Read on the caller's thread rather than one of its own.
    in_place: bool,
}

struct Reading {
    threads: Vec<ThreadId>,
    rows: Receiver<Vec<ParkedThread>>,
}

impl Default for ParkedListing {
    fn default() -> Self {
        Self {
            rows: BTreeMap::new(),
            stale: BTreeSet::new(),
            moved: true,
            parked: Vec::new(),
            unpublished: true,
            reading: None,
            in_place: false,
        }
    }
}

impl ParkedListing {
    /// Read rows on the caller's thread: an Engine its window pumps.
    pub(super) fn read_in_place(&mut self) {
        self.in_place = true;
    }

    /// The parked set may have moved: a Thread parked, revived, imported
    /// or deleted. A row already read stays as it is.
    pub(super) fn moved(&mut self) {
        self.moved = true;
    }

    /// `thread`'s log moved — it was parked, or changed while parked: its
    /// row is read again.
    pub(super) fn reread(&mut self, thread: ThreadId) {
        self.stale.insert(thread);
    }

    /// The rows to publish, once every parked Thread's is read and none is
    /// due again; `None` while a read is out or nothing moved. Starts the
    /// read of the rows that are due. `parked` lists the parked Threads,
    /// asked only when the set may have moved.
    pub(super) fn poll(
        &mut self,
        store: &Store,
        parked: impl FnOnce() -> Vec<ThreadId>,
    ) -> Option<Vec<ParkedThread>> {
        if let Some(reading) = &self.reading {
            match reading.rows.try_recv() {
                Err(TryRecvError::Empty) => return None,
                Ok(rows) => self.adopt(rows),
                // The read died: its rows are due again.
                Err(TryRecvError::Disconnected) => {
                    self.stale.extend(reading.threads.iter().copied())
                }
            }
            self.reading = None;
        }
        if std::mem::take(&mut self.moved) {
            self.parked = parked();
            let listed: BTreeSet<ThreadId> = self.parked.iter().copied().collect();
            let before = self.rows.len();
            self.rows.retain(|thread, _| listed.contains(thread));
            self.stale.retain(|thread| listed.contains(thread));
            self.unpublished |= self.rows.len() != before;
        }
        let due: Vec<ThreadId> = self
            .parked
            .iter()
            .copied()
            .filter(|thread| self.stale.contains(thread) || !self.rows.contains_key(thread))
            .collect();
        if !due.is_empty() {
            self.stale.clear();
            if self.in_place {
                let rows = super::parked_rows(store, due);
                self.adopt(rows);
            } else {
                self.read(store, due);
                return None;
            }
        }
        if !std::mem::take(&mut self.unpublished) {
            return None;
        }
        Some(
            self.parked
                .iter()
                .filter_map(|thread| self.rows.get(thread).cloned())
                .collect(),
        )
    }

    /// Keep what a read brought back, but for a row that changed again
    /// while it was out: that one is read again.
    fn adopt(&mut self, rows: Vec<ParkedThread>) {
        for row in rows {
            if !self.stale.contains(&row.thread) {
                self.rows.insert(row.thread, row);
            }
        }
        self.unpublished = true;
    }

    fn read(&mut self, store: &Store, threads: Vec<ThreadId>) {
        let store = store.clone();
        let (rows, reading) = channel();
        let wanted = threads.clone();
        let read = std::thread::Builder::new()
            .name("ferrite-parked-rows".into())
            .spawn(move || {
                let _ = rows.send(super::parked_rows(&store, wanted));
            });
        match read {
            Ok(_) => {
                self.reading = Some(Reading {
                    threads,
                    rows: reading,
                })
            }
            Err(error) => {
                crate::log!("the parked Threads could not be read: {error}");
                self.stale.extend(threads);
            }
        }
    }
}
