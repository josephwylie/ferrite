//! One writer of a home's setup at a time, by any process (ADR 0050): a
//! sync holds the home from its first scan to its base, an install holds it
//! while its tools run, and each write a Machine is asked for holds it for
//! that write. Two syncs to different Machines never cross in a home, and
//! no tool rewrites a file a sync is writing. A sync that finds the home
//! held is busy, and is asked again once it is let go.

use std::fs::{self, File};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError, Weak};

/// The file a home is held by, in its Ferrite settings folder.
pub(super) const LOCK: &str = "setup-sync.lock";

/// A home's lock, as one view of the home holds it: a sync's writes are
/// its own, and every other writer waits.
pub(super) struct Lock {
    at: PathBuf,
    /// The sync under way through this view of the home, if any.
    sync: Mutex<Weak<File>>,
}

/// The home held, until the last clone of this is dropped.
#[derive(Clone)]
pub(super) struct Held {
    _file: Arc<File>,
}

impl Lock {
    pub(super) fn at(at: PathBuf) -> Arc<Self> {
        Arc::new(Self { at, sync: Mutex::new(Weak::new()) })
    }

    /// The home held for a sync through this view of it, its writes then
    /// its own: `None` while anything else holds it.
    pub(super) fn sync(&self) -> io::Result<Option<Held>> {
        let mut sync = self.sync.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(held) = self.try_hold()? else {
            return Ok(None);
        };
        *sync = Arc::downgrade(&held._file);
        Ok(Some(held))
    }

    /// The home held, or `None` while anything holds it.
    pub(super) fn try_hold(&self) -> io::Result<Option<Held>> {
        let file = self.open()?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Held { _file: Arc::new(file) })),
            Err(fs::TryLockError::WouldBlock) => Ok(None),
            Err(fs::TryLockError::Error(error)) => Err(error),
        }
    }

    /// The home held, once whatever holds it lets go.
    pub(super) fn hold(&self) -> io::Result<Held> {
        let file = self.open()?;
        file.lock()?;
        Ok(Held { _file: Arc::new(file) })
    }

    /// The home held for one write: nothing to take when it is the write of
    /// the sync under way through this view of it.
    pub(super) fn write(&self) -> io::Result<Option<Held>> {
        if self.sync.lock().unwrap_or_else(PoisonError::into_inner).strong_count() > 0 {
            return Ok(None);
        }
        self.hold().map(Some)
    }

    fn open(&self) -> io::Result<File> {
        if let Some(parent) = self.at.parent() {
            super::store::private_dirs(parent)?;
        }
        fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&self.at)
            .map_err(|error| crate::tree::at(&self.at, error))
    }
}
