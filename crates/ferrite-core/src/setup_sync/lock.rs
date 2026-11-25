//! One writer of a home's setup at a time, by any process (ADR 0050): a
//! sync holds both its homes from its first scan to its base, this
//! computer's itself and the Machine's by the connection it syncs over
//! (`tree::Request::Hold`), and an install holds its home while its tools
//! run. So two syncs never cross in a home, whichever computer drives each,
//! and no tool rewrites a file a sync is writing. A sync that finds either
//! home held is busy, and is asked again once it is let go.

use std::fs::{self, File};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

/// The file a home is held by, in its Ferrite settings folder.
pub(super) const LOCK: &str = "setup-sync.lock";

pub(super) struct Lock {
    at: PathBuf,
}

/// The home held, until the last clone of this is dropped.
#[derive(Clone)]
pub(crate) struct Held {
    _file: Arc<File>,
}

impl Lock {
    pub(super) fn at(at: PathBuf) -> Arc<Self> {
        Arc::new(Self { at })
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
