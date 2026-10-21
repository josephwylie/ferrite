//! Free space on a volume, for the bar's `N GB free` (SPEC §5.7, §8).
//!
//! A blocking operating-system query (`statvfs`, `GetDiskFreeSpaceExW`):
//! callers run it off the UI thread. It answers the bytes an unprivileged
//! writer may still use, which is what fills up under a Thread's checkout.

use std::path::Path;

/// The bytes free to this user on the volume holding `path`; `None` when
/// the system cannot say (the path is gone, the query failed).
pub fn free_bytes(path: &Path) -> Option<u64> {
    imp::free_bytes(path)
}

/// The least free space across the volumes holding `paths`: the one that
/// fills first. `None` when no path could be read.
pub fn least_free<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Option<u64> {
    paths.into_iter().filter_map(free_bytes).min()
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
        assert_eq!(
            least_free([gone.as_path(), here.as_path()]),
            free_bytes(&here)
        );
    }
}
