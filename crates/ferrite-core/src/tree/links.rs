//! A link a tree op makes, as the Server's computer makes one. On Unix it
//! is a symlink to the target as sent. On Windows it is made as Windows'
//! own tools make one (`mklink /J`, pnpm), and only to a place inside the
//! root: a link to a folder is a junction, which needs no privilege; one to
//! a file is a symlink where the account may make one, else a copy of the
//! file. So Windows makes no link that leads out of the root, and none to
//! nothing.

/// Make the link at `path`, in `root`, to `target`: whether it was not
/// already so.
#[cfg(unix)]
pub(crate) fn link(_: &std::path::Path, path: &std::path::Path, target: &str) -> Result<bool, super::Refusal> {
    if std::fs::read_link(path).is_ok_and(|there| there == std::path::Path::new(target)) {
        return Ok(false);
    }
    std::os::unix::fs::symlink(target, path).map_err(super::Refusal::failed)?;
    Ok(true)
}

#[cfg(windows)]
pub(crate) use windows::link;

#[cfg(windows)]
mod windows {
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    use super::super::{holds, Refusal};
    use crate::chunked;

    /// Make the link at `path`, in `root`, to `target` (`/`-separated, as
    /// a Mac sends it) by what it leads to: whether it was not already so.
    pub(crate) fn link(root: &Path, path: &Path, target: &str) -> Result<bool, Refusal> {
        let to = leads(root, path.parent().unwrap_or(root), target).ok_or(Refusal::LeadsOut)?;
        let real = match dunce::canonicalize(&to) {
            Ok(real) => real,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(Refusal::LeadsNowhere),
            Err(error) => return Err(Refusal::failed(error)),
        };
        if !holds(root, &real) {
            return Err(Refusal::LeadsOut);
        }
        let folder = fs::metadata(&real).map_err(Refusal::failed)?.is_dir();
        let relative = target.replace('/', "\\");
        if let Ok(there) = fs::symlink_metadata(path) {
            let linked = fs::read_link(path).is_ok_and(|there| {
                there == Path::new(&relative) || (holds(&there, &to) && holds(&to, &there))
            });
            let copied = there.is_file() && matches!((fs::read(path), fs::read(&real)), (Ok(ours), Ok(theirs)) if ours == theirs);
            if linked || copied {
                return Ok(false);
            }
            return Err(Refusal::failed(io::ErrorKind::AlreadyExists.into()));
        }
        let made = if folder {
            junction(&to, path)
        } else {
            match std::os::windows::fs::symlink_file(&relative, path) {
                Err(error) if error.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD) => copy(&real, path),
                made => made,
            }
        };
        made.map(|()| true).map_err(Refusal::failed)
    }

    /// The account may not make a symlink: no Developer Mode, no elevation.
    const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;

    /// Where `target`, a link's in `folder`, leads under `root`, `.` and
    /// `..` taken by name as Windows takes them: none once it climbs out of
    /// the root, or when it names a whole path.
    fn leads(root: &Path, folder: &Path, target: &str) -> Option<PathBuf> {
        if target.starts_with(['/', '\\']) || target.contains(':') {
            return None;
        }
        let mut at = folder.to_path_buf();
        let mut depth = folder.components().count().checked_sub(root.components().count())?;
        for part in target.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    depth = depth.checked_sub(1)?;
                    at.pop();
                }
                name => {
                    depth += 1;
                    at.push(name);
                }
            }
        }
        Some(at)
    }

    /// A copy of the file at `from` at `to`, put in place whole.
    fn copy(from: &Path, to: &Path) -> io::Result<()> {
        let (temporary, file) = chunked::temporary_file(to.parent().unwrap_or(to), None)?;
        drop(file);
        let copied = fs::copy(from, &temporary).and_then(|_| fs::rename(&temporary, to));
        if copied.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        copied
    }

    /// A junction at `path` to the folder `to`, as `mklink /J` makes one:
    /// an empty folder that leads to `to` by its whole path.
    fn junction(to: &Path, path: &Path) -> io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT};
        use windows_sys::Win32::System::IO::DeviceIoControl;
        const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
        const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;

        let shown = dunce::simplified(to).as_os_str().to_os_string();
        let mut nt: Vec<u16> = "\\??\\".encode_utf16().collect();
        nt.extend(shown.encode_wide());
        let shown: Vec<u16> = shown.encode_wide().collect();
        // The reparse data: four lengths and offsets, then both names, each
        // ended by a NUL.
        let long = || io::Error::other("the path is too long for a junction");
        let length = u16::try_from(8 + (nt.len() + 1 + shown.len() + 1) * 2).map_err(|_| long())?;
        let (nt_bytes, shown_bytes) = ((nt.len() * 2) as u16, (shown.len() * 2) as u16);
        let mut buffer = Vec::with_capacity(8 + usize::from(length));
        buffer.extend(IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        buffer.extend(length.to_le_bytes());
        buffer.extend(0u16.to_le_bytes());
        for field in [0, nt_bytes, nt_bytes + 2, shown_bytes] {
            buffer.extend(field.to_le_bytes());
        }
        for unit in nt.iter().chain(&[0u16]).chain(&shown).chain(&[0u16]) {
            buffer.extend(unit.to_le_bytes());
        }

        fs::create_dir(path)?;
        let set = fs::OpenOptions::new()
            .write(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .and_then(|folder| {
                let mut returned = 0u32;
                // SAFETY: `buffer` is a whole mount point reparse buffer
                // for the open handle, which outlives the call; nothing is
                // read back.
                let done = unsafe {
                    DeviceIoControl(
                        folder.as_raw_handle(),
                        FSCTL_SET_REPARSE_POINT,
                        buffer.as_ptr().cast(),
                        buffer.len() as u32,
                        std::ptr::null_mut(),
                        0,
                        &mut returned,
                        std::ptr::null_mut(),
                    )
                };
                if done == 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        if set.is_err() {
            let _ = fs::remove_dir(path);
        }
        set
    }
}
