//! The store's worker (ADR 0008): every sync of every log runs here, on a
//! cloned handle and outside the writer's lock, so no keystroke ever waits
//! on a drive. Records are written where they are accepted — into the page
//! cache, which every reader sees at once — and synced here: behind a
//! barrier as soon as they are written, through the drive's cache at least
//! every 30 s. The marks that vouch for what precedes them are appended
//! here, only once that is synced; and a parked Thread is finished here:
//! its open marker removed, a final mark, a full sync.

use super::{Shared, SyncLevel, WriterState, OPEN_MARKER, OPEN_STATE_MARKER};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

/// How often the worker looks without being asked: for full syncs that
/// have come due and syncs that failed and are retried.
const SWEEP: Duration = Duration::from_millis(500);

pub(super) struct Worker {
    queue: Mutex<Queue>,
    wake: Condvar,
    /// Notified after every pass, for those waiting on a Thread to settle.
    pub(super) passed: Condvar,
}

#[derive(Default)]
pub(super) struct Queue {
    live: Vec<Weak<Mutex<WriterState>>>,
    /// Writers their owners let go of — parked Threads, dropped writers —
    /// held until everything they wrote is synced.
    pub(super) held: Vec<Arc<Mutex<WriterState>>>,
    dirty: bool,
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Queue {
    /// The live writers still alive.
    #[cfg(test)]
    pub(super) fn live_writers(&self) -> Vec<Arc<Mutex<WriterState>>> {
        self.live.iter().filter_map(Weak::upgrade).collect()
    }
}

impl Worker {
    /// Start the worker for one store. It lives while the store or any of
    /// its writers does.
    pub(super) fn start(shared: Weak<Shared>) -> Arc<Self> {
        let worker = Arc::new(Self {
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
            passed: Condvar::new(),
        });
        let running = worker.clone();
        std::thread::Builder::new()
            .name("ferrite-store".into())
            .spawn(move || running.run(shared))
            .expect("the store's worker thread starts");
        worker
    }

    pub(super) fn queue(&self) -> MutexGuard<'_, Queue> {
        lock(&self.queue)
    }

    /// Follow a live writer: sync what it writes.
    pub(super) fn follow(&self, writer: &Arc<Mutex<WriterState>>) {
        self.queue().live.push(Arc::downgrade(writer));
    }

    /// Hold a writer its owner let go of until it is finished.
    pub(super) fn hold(&self, writer: Arc<Mutex<WriterState>>) {
        let mut queue = self.queue();
        queue.held.push(writer);
        queue.dirty = true;
        self.wake.notify_all();
    }

    /// Something was written: look now.
    pub(super) fn poke(&self) {
        self.queue().dirty = true;
        self.wake.notify_all();
    }

    fn run(&self, shared: Weak<Shared>) {
        loop {
            let writers: Vec<_> = {
                let mut queue = self.queue();
                if !queue.dirty {
                    queue = self
                        .wake
                        .wait_timeout(queue, SWEEP)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;
                }
                queue.dirty = false;
                queue.live.retain(|writer| writer.strong_count() > 0);
                if shared.strong_count() == 0 && queue.live.is_empty() && queue.held.is_empty() {
                    return;
                }
                queue
                    .live
                    .iter()
                    .filter_map(Weak::upgrade)
                    .chain(queue.held.iter().cloned())
                    .collect()
            };
            let mut again = false;
            for writer in &writers {
                again |= step(writer);
            }
            drop(writers);
            let mut queue = self.queue();
            queue.held.retain(|writer| !lock(writer).finished());
            queue.dirty |= again;
            drop(queue);
            self.passed.notify_all();
        }
    }
}

/// One step of one writer's syncing; whether it has more to do at once.
fn step(writer: &Mutex<WriterState>) -> bool {
    let (file, len, level, shared) = {
        let mut state = lock(writer);
        if let Some((marker, state_marker)) = state.unmark.take() {
            // Parked: the operator's choice is durable first, and the next
            // launch reopens nothing it should not.
            if std::fs::File::create(&state_marker).is_err() {
                state.unmark = Some((marker, state_marker));
                return false;
            }
            let _ = std::fs::remove_file(&marker);
        }
        // A mark vouches for every line before it, so it follows their sync.
        if state.synced == state.len && (state.mark_due() || state.parking && !state.park_marked) {
            match state.mark() {
                Ok(()) => state.park_marked |= state.parking,
                Err(error) => {
                    state.failure = Some(error.to_string());
                    return false;
                }
            }
        }
        // A park ends with one full sync, after its mark: barrier first.
        let full_due = state.full_synced < state.len
            && (state.parking && state.park_marked
                || state.fully_synced.elapsed() >= state.full_sync_interval);
        let level = if full_due {
            SyncLevel::Full
        } else if state.synced < state.len {
            SyncLevel::Barrier
        } else {
            return false;
        };
        let file = match state.file.try_clone() {
            Ok(file) => file,
            Err(error) => {
                state.failure = Some(error.to_string());
                return false;
            }
        };
        (file, state.len, level, state.shared.clone())
    };
    let synced = shared.sync_in_worker(&file, level);
    let mut state = lock(writer);
    match synced {
        Ok(()) => {
            state.synced = state.synced.max(len);
            if level == SyncLevel::Full {
                state.full_synced = state.full_synced.max(len);
                state.fully_synced = std::time::Instant::now();
            }
            state.failure = None;
            #[cfg(test)]
            state.shared.journal("synced", len);
            true
        }
        Err(error) => {
            state.failure = Some(error.to_string());
            false
        }
    }
}

impl WriterState {
    /// Whether the worker is done with a writer its owner let go of.
    pub(super) fn finished(&self) -> bool {
        self.unmark.is_none()
            && self.synced >= self.len
            && (!self.parking || self.park_marked && self.full_synced >= self.len)
    }

    /// Hand this parked Thread's writer to the worker: remove its open
    /// marker, mark the log, sync it through the drive's cache.
    pub(super) fn park(&mut self) {
        let thread = self.path.parent().map(std::path::Path::to_path_buf);
        if let Some(thread) = thread {
            let store = thread.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
            self.unmark = Some((thread.join(OPEN_MARKER), store.join(OPEN_STATE_MARKER)));
        }
        self.parking = true;
    }
}
