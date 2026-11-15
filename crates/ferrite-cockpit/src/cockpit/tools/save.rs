//! How the Files editor writes its buffer back: one write at a time, tied to
//! the edit revision it carries, refused when the disk moved on since the
//! buffer last matched it.

use std::hash::{DefaultHasher, Hasher};
use std::io;
use std::path::Path;

/// Where the buffer stands against the disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SaveState {
    /// Nothing to report: opened, or edited since the last result.
    Idle,
    /// A write of this edit revision is in flight.
    Saving(u64),
    Saved,
    /// The write was refused; the buffer stays as typed.
    Failed(String),
    /// The file on disk is no longer the one the buffer was read from.
    Conflict,
}

/// The write's result, for the revision that started it.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// Written; the disk now holds text with this fingerprint.
    Written(u64),
    Changed,
    Refused(String),
}

pub(crate) type Writer = fn(&Path, &[u8]) -> io::Result<()>;

pub(crate) fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

/// Write `text` over the file, unless `expected` (the fingerprint the buffer
/// was read with) no longer matches what is on disk. `None` overwrites.
pub(crate) fn write(path: &Path, text: &str, expected: Option<u64>, writer: Writer) -> Outcome {
    if let Some(expected) = expected {
        match std::fs::read(path) {
            Ok(bytes) if fingerprint(&bytes) != expected => return Outcome::Changed,
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                return Outcome::Refused(error.to_string())
            }
            _ => {}
        }
    }
    match writer(path, text.as_bytes()) {
        Ok(()) => Outcome::Written(fingerprint(text.as_bytes())),
        Err(error) => Outcome::Refused(error.to_string()),
    }
}

/// Replace the file whole: a sibling is written and renamed over it, so a
/// failure part-way leaves the old file.
pub(crate) fn replace_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut staging = path.as_os_str().to_owned();
    staging.push(".ferrite-save");
    let staging = std::path::PathBuf::from(staging);
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.permissions().readonly() {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the file is read-only"));
        }
    }
    let result = std::fs::write(&staging, bytes).and_then(|()| {
        if let Ok(meta) = std::fs::metadata(path) {
            std::fs::set_permissions(&staging, meta.permissions())?;
        }
        std::fs::rename(&staging, path)
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&staging);
    }
    result
}
