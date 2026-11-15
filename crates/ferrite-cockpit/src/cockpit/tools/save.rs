//! How the Files editor writes its buffer back: one write at a time, tied to
//! the edit revision it carries, refused when the disk moved on since the
//! buffer last matched it.

use std::hash::{DefaultHasher, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
    /// Taking the disk's text failed; the buffer is still the conflicted one.
    ReloadFailed(String),
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

/// Replace the file whole: a sibling of the real file (a symlink is followed,
/// as a plain write would) is written and renamed over it, so a failure
/// part-way leaves the old file. Where no sibling can be created, the file is
/// written in place.
pub(crate) fn replace_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let permissions = std::fs::metadata(&target).ok().map(|meta| meta.permissions());
    if permissions.as_ref().is_some_and(|permissions| permissions.readonly()) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the file is read-only"));
    }
    let mut staging = target.as_os_str().to_owned();
    staging.push(format!(
        ".ferrite-save-{}-{}",
        std::process::id(),
        WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    let staging = PathBuf::from(staging);
    if std::fs::write(&staging, bytes).is_err() {
        let _ = std::fs::remove_file(&staging);
        return std::fs::write(&target, bytes);
    }
    let result = permissions
        .map_or(Ok(()), |permissions| std::fs::set_permissions(&staging, permissions))
        .and_then(|()| std::fs::rename(&staging, &target));
    if result.is_err() {
        let _ = std::fs::remove_file(&staging);
    }
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrite-save-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saving_through_a_symlink_writes_the_target() {
        let dir = scratch("link");
        let target = dir.join("AGENTS.md");
        let link = dir.join("CLAUDE.md");
        std::fs::write(&target, "old").unwrap();
        symlink(&target, &link).unwrap();
        replace_file(&link, b"new").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    }

    #[test]
    fn a_writable_file_in_a_read_only_directory_still_saves() {
        let dir = scratch("readonly-dir");
        let file = dir.join("notes.md");
        std::fs::write(&file, "old").unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = replace_file(&file, b"new");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        result.unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new");
    }

    #[test]
    fn a_read_only_file_is_refused_and_left_alone() {
        let dir = scratch("readonly-file");
        let file = dir.join("notes.md");
        std::fs::write(&file, "old").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o444)).unwrap();
        assert!(replace_file(&file, b"new").is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "old");
    }
}
