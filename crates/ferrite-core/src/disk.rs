//! Free space on a volume, for the bar's `N GB free` (SPEC §5.7, §8), the
//! removal of stale files from a folder Ferrite rewrites as it goes, and
//! the replacement of a small whole file that a crash cannot tear.
//!
//! The free space is a blocking operating-system query (`statvfs`,
//! `GetDiskFreeSpaceExW`): callers run it off the UI thread. It answers the
//! bytes an unprivileged writer may still use, which is what fills up under a
//! Thread's checkout.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The bytes free to this user on the volume holding `path`; `None` when
/// the system cannot say (the path is gone, the query failed).
pub fn free_bytes(path: &Path) -> Option<u64> {
    imp::free_bytes(path)
}

/// Removes the files in `dir` last written at least `age` ago.
pub fn remove_stale(dir: &Path, age: std::time::Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|elapsed| elapsed >= age);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The least free space across the volumes holding `paths`: the one that
/// fills first. `None` when no path could be read.
pub fn least_free<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Option<u64> {
    paths.into_iter().filter_map(free_bytes).min()
}

/// Put `bytes` in place of a small whole-file document (the settings, the
/// registry) so that a crash or power cut at any moment leaves
/// the old file or the new one, never a torn one: written beside, synced
/// behind a barrier so the contents reach the drive before the rename can
/// (ADR 0009), renamed over, the rename made durable by syncing the
/// directory. A symlinked file is written through to its target, so the
/// link the operator made survives.
///
/// Each document has one writer (the UI thread), so its temp file has one
/// fixed name: one a crash left behind is simply written over next time.
/// Once the rename is done the new file is the answer; a directory that
/// will not sync is logged, not returned, so a caller never rolls back to
/// a state the disk no longer holds.
pub fn replace_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let path = if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        dunce::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    let mut tmp = path.clone().into_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    (|| {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        sync_ordered(&file)?;
        rename_over(&tmp, &path)
    })()
    .inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;
    if let Err(error) = sync_dir(path.parent().unwrap_or(Path::new("."))) {
        crate::log!("{} replaced but not synced: {error}", path.display());
    }
    Ok(())
}

/// Sync a file behind a barrier: its writes reach the drive before any
/// that follow (ADR 0009). For files beside the logs, like the queue.
pub fn sync_ordered(file: &File) -> io::Result<()> {
    barrier(file)
}

#[cfg(target_os = "macos")]
fn barrier(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    const F_BARRIERFSYNC: std::ffi::c_int = 85;
    extern "C" {
        fn fcntl(fd: std::ffi::c_int, cmd: std::ffi::c_int, ...) -> std::ffi::c_int;
    }
    // SAFETY: F_BARRIERFSYNC takes no argument and only reads the
    // descriptor, which `file` keeps open for the call.
    if unsafe { fcntl(file.as_raw_fd(), F_BARRIERFSYNC) } == -1 {
        // A filesystem without barriers still gets its writes to disk.
        return file.sync_data();
    }
    Ok(())
}

/// Elsewhere the data sync orders writes already (`fdatasync`,
/// `FlushFileBuffers`).
#[cfg(not(target_os = "macos"))]
fn barrier(file: &File) -> io::Result<()> {
    file.sync_data()
}

/// Rename `from` over `to`. The rename survives a crash once its
/// directory is synced (`sync_dir`).
#[cfg(not(windows))]
pub fn rename_over(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

/// Windows has no directory sync; the move itself is written through.
#[cfg(windows)]
pub(crate) fn rename_over(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<u16>>()
    };
    let (from, to) = (wide(from), wide(to));
    // SAFETY: both paths are NUL-terminated wide strings alive for the call.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Make a rename inside `dir` durable: sync the directory itself.
#[cfg(unix)]
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    extern "C" {
        fn fsync(fd: std::ffi::c_int) -> std::ffi::c_int;
    }
    let dir = File::open(dir)?;
    // SAFETY: fsync only reads the descriptor, which `dir` keeps open.
    if unsafe { fsync(dir.as_raw_fd()) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Windows renames write through (`rename_over`); there is nothing to sync.
#[cfg(not(unix))]
pub(crate) fn sync_dir(_: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
mod imp {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    pub fn free_bytes(path: &Path) -> Option<u64> {
        let path = CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: `path` is a NUL-terminated string that outlives the call,
        // and `stat` is a writable buffer of the struct's size, read only
        // after the call says it filled it.
        if unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) } != 0 {
            return None;
        }
        // SAFETY: `statvfs` returned 0, so it wrote the whole struct.
        let stat = unsafe { stat.assume_init() };
        #[allow(clippy::unnecessary_cast)] // the field widths differ by platform
        Some(stat.f_bavail as u64 * stat.f_frsize as u64)
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    pub fn free_bytes(path: &Path) -> Option<u64> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut free = 0u64;
        // SAFETY: `wide` is NUL-terminated and outlives the call; the one
        // out-pointer is a live u64, the others may be null.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(free)
    }
}

#[cfg(not(any(unix, windows)))]
mod imp {
    pub fn free_bytes(_: &std::path::Path) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_directory_has_a_reading_and_a_missing_one_none() {
        let here = std::env::temp_dir();
        assert!(free_bytes(&here).is_some());
        let gone = here.join("ferrite-disk-no-such-dir-4f1c");
        assert_eq!(free_bytes(&gone), None);
        assert_eq!(least_free([gone.as_path()]), None);
        // A missing path is skipped, not the least. Free space moves under
        // a live disk, so two readings need not agree to the byte.
        assert!(least_free([gone.as_path(), here.as_path()]).is_some());
    }
}
