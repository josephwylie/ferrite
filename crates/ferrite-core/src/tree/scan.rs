//! A directory listed as a [`Manifest`], hashing only the files that moved.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

use super::{check_path, Entry, Manifest};
use crate::chunked::{self, hex};

struct Hashed {
    size: u64,
    modified: SystemTime,
    sha256: String,
}

/// What each tree's files hashed to when this process last listed it.
static HASHED: Mutex<BTreeMap<PathBuf, BTreeMap<String, Hashed>>> = Mutex::new(BTreeMap::new());

/// A file modified this close to its listing may change again within the
/// same tick of its clock, unseen, so its hash is not kept.
const SETTLED: Duration = Duration::from_secs(2);

/// `root` as a Manifest, with `skip` (paths in it) and everything under
/// them left out. Symlinks are listed and never followed; what is none of a
/// file, a directory or a symlink (a socket, a FIFO), and a file's temporary
/// file while it is received there, is left out. A file is read only when
/// its size or modification time moved since this process last listed
/// `root`.
pub fn scan(root: &Path, skip: &[String]) -> io::Result<Manifest> {
    for path in skip {
        check_path(path)?;
    }
    if !fs::metadata(root)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            format!("{} is not a directory", root.display()),
        ));
    }
    let started = SystemTime::now();
    let known = lock().remove(root).unwrap_or_default();
    let mut kept = BTreeMap::new();
    let mut entries = BTreeMap::new();
    let mut dirs = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = dirs.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = utf8(entry.file_name().into(), &dir)?;
            let path = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if skip.contains(&path) {
                continue;
            }
            let meta = entry.metadata()?;
            let kind = meta.file_type();
            let listed = if kind.is_symlink() {
                let target = utf8(fs::read_link(entry.path())?, &dir)?;
                Entry::Symlink {
                    target: if cfg!(windows) { target.replace('\\', "/") } else { target },
                }
            } else if kind.is_dir() {
                dirs.push((entry.path(), path.clone()));
                Entry::Dir
            } else if kind.is_file() && !chunked::receiving(&entry.file_name()) {
                let (size, modified) = (meta.len(), meta.modified()?);
                let sha256 = match known.get(&path) {
                    Some(hashed) if hashed.size == size && hashed.modified == modified => {
                        hashed.sha256.clone()
                    }
                    _ => hash(&entry.path())?,
                };
                if modified.checked_add(SETTLED).is_some_and(|settled| settled < started) {
                    kept.insert(
                        path.clone(),
                        Hashed {
                            size,
                            modified,
                            sha256: sha256.clone(),
                        },
                    );
                }
                Entry::File {
                    size,
                    sha256,
                    executable: executable(&meta),
                }
            } else {
                continue;
            };
            entries.insert(path, listed);
        }
    }
    lock().insert(root.to_path_buf(), kept);
    Ok(Manifest {
        entries,
        modes: cfg!(unix),
    })
}

fn lock() -> std::sync::MutexGuard<'static, BTreeMap<PathBuf, BTreeMap<String, Hashed>>> {
    HASHED.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn utf8(name: PathBuf, dir: &Path) -> io::Result<String> {
    name.into_os_string().into_string().map_err(|name| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} in {} is not UTF-8", Path::new(&name).display(), dir.display()),
        )
    })
}

fn hash(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        match file.read(&mut buffer)? {
            0 => return Ok(hex(&hash.finalize())),
            count => hash.update(&buffer[..count]),
        }
    }
}

#[cfg(unix)]
fn executable(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable(_: &fs::Metadata) -> bool {
    false
}
