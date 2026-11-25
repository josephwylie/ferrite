//! How the Files editor writes its buffer back: one write at a time, tied to
//! the edit revision it carries, refused when the disk moved on since the
//! buffer last matched it.

pub(crate) use ferrite_core::file_operations::{fingerprint, replace_file, Fingerprint};
use std::io;
use std::path::Path;
#[cfg(all(test, unix))]
use std::path::PathBuf;

/// Where the buffer stands against the disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveState {
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
    Written(Fingerprint),
    Changed,
    Refused(String),
}

pub(crate) type Writer = fn(&Path, &[u8]) -> io::Result<()>;

/// Whether the file on disk is no longer the one `expected` was read from.
pub(crate) fn changed_on_disk(path: &Path, expected: Fingerprint) -> bool {
    std::fs::read(path).is_ok_and(|bytes| fingerprint(&bytes) != expected)
}

/// Write `text` over the file, unless `expected` (the fingerprint the buffer
/// was read with) no longer matches what is on disk. `None` overwrites.
pub(crate) fn write(
    path: &Path,
    text: &str,
    expected: Option<Fingerprint>,
    writer: Writer,
) -> Outcome {
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
    fn saving_through_a_dangling_symlink_creates_its_target() {
        let dir = scratch("dangling");
        let target = dir.join("AGENTS.md");
        let link = dir.join("CLAUDE.md");
        symlink(&target, &link).unwrap();
        replace_file(&link, b"new").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
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
