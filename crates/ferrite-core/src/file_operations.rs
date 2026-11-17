use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub type Fingerprint = [u8; 32];
pub const MAX_TEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;
const MAX_DIRECTORIES: usize = 128;
const MAX_PATH_BYTES: usize = 4096;
const MAX_LIST_BYTES: usize = 4 * 1024 * 1024;
const NOT_SEARCHED: &[&str] = &[".git", "target", "node_modules", ".worktrees"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Request {
    Snapshot {
        root: PathBuf,
        #[serde(default)]
        directories: Vec<PathBuf>,
        #[serde(default)]
        search: bool,
    },
    Read {
        root: PathBuf,
        path: PathBuf,
    },
    Write {
        root: PathBuf,
        path: PathBuf,
        text: String,
        #[serde(default)]
        expected: Option<Fingerprint>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Snapshot(Snapshot),
    Text {
        text: String,
        fingerprint: Fingerprint,
    },
    Binary,
    Written {
        fingerprint: Fingerprint,
    },
    Conflict,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub directories: Vec<Directory>,
    #[serde(default)]
    pub files: Vec<Entry>,
    #[serde(default)]
    pub status: Vec<(PathBuf, Status)>,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Directory {
    pub path: PathBuf,
    #[serde(default)]
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    #[serde(default)]
    pub directory: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Modified,
    Added,
    Deleted,
    #[serde(other)]
    Unknown,
}

pub fn fingerprint(bytes: &[u8]) -> Fingerprint {
    Sha256::digest(bytes).into()
}

pub fn execute(request: Request) -> io::Result<Reply> {
    match request {
        Request::Snapshot {
            root,
            directories,
            search,
        } => snapshot(&root, directories, search).map(Reply::Snapshot),
        Request::Read { root, path } => {
            let bytes = read_text_bytes(&checked_path(&root, &path)?)?;
            let fingerprint = fingerprint(&bytes);
            Ok(match String::from_utf8(bytes) {
                Ok(text) => Reply::Text { text, fingerprint },
                Err(_) => Reply::Binary,
            })
        }
        Request::Write {
            root,
            path,
            text,
            expected,
        } => {
            if text.len() > MAX_TEXT_BYTES {
                return Err(io::Error::other(
                    "Files supports text files up to 2 MiB. Use the remote terminal for this file.",
                ));
            }
            static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());
            let _guard = WRITES.lock().unwrap_or_else(|error| error.into_inner());
            let path = checked_path(&root, &path)?;
            if let Some(expected) = expected {
                if fingerprint(&read_text_bytes(&path)?) != expected {
                    return Ok(Reply::Conflict);
                }
            }
            replace_file(&path, text.as_bytes())?;
            Ok(Reply::Written {
                fingerprint: fingerprint(text.as_bytes()),
            })
        }
        Request::Unknown => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Update Ferrite on this machine to use Files.",
        )),
    }
}

fn checked_path(root: &Path, path: &Path) -> io::Result<PathBuf> {
    if !root.is_absolute()
        || root.as_os_str().len() > MAX_PATH_BYTES
        || path.as_os_str().len() > MAX_PATH_BYTES
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Files requires an absolute workspace path of at most 4096 bytes.",
        ));
    }
    let root = root.canonicalize()?;
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let path = path.canonicalize()?;
    if !path.starts_with(root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "This file is outside the remote workspace. Open its workspace or use the remote terminal.",
        ));
    }
    Ok(path)
}

fn read_text_bytes(path: &Path) -> io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("This path is not a regular file."));
    }
    let mut bytes = Vec::new();
    file.take(MAX_TEXT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(io::Error::other(
            "Files supports text files up to 2 MiB. Use the remote terminal for this file.",
        ));
    }
    Ok(bytes)
}

#[derive(Default)]
struct Budget {
    entries: usize,
    bytes: usize,
    truncated: bool,
}

impl Budget {
    fn take(&mut self, path: &Path) -> bool {
        let bytes = path.as_os_str().len();
        if self.entries >= MAX_ENTRIES
            || self.bytes + bytes > MAX_LIST_BYTES
            || bytes > MAX_PATH_BYTES
        {
            self.truncated = true;
            return false;
        }
        self.entries += 1;
        self.bytes += bytes;
        true
    }
}

fn directory(root: &Path, path: &Path, budget: &mut Budget) -> io::Result<Vec<Entry>> {
    let target = checked_path(root, path)?;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(target)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        let path = path.join(&name);
        if !budget.take(&path) {
            break;
        }
        // A symlink can open as a file, but must never make recursive search loop.
        let directory = entry.file_type()?.is_dir();
        entries.push(Entry {
            path,
            name,
            directory,
        });
    }
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

fn snapshot(root: &Path, mut directories: Vec<PathBuf>, search: bool) -> io::Result<Snapshot> {
    checked_path(root, root)?;
    if directories.len() > MAX_DIRECTORIES {
        return Err(io::Error::other(
            "Collapse some folders before refreshing. Files supports 128 open folders.",
        ));
    }
    if !directories.iter().any(|path| path == root) {
        directories.insert(0, root.to_path_buf());
    }
    let mut budget = Budget::default();
    let mut snapshot = Snapshot::default();
    for path in directories {
        let entries = match directory(root, &path, &mut budget) {
            Ok(entries) => entries,
            Err(error) if path == root => return Err(error),
            Err(_) => {
                snapshot.truncated = true;
                Vec::new()
            }
        };
        snapshot.directories.push(Directory { path, entries });
        if budget.truncated {
            break;
        }
    }
    if search {
        let mut stack = vec![root.to_path_buf()];
        while let Some(path) = stack.pop() {
            let entries = match directory(root, &path, &mut budget) {
                Ok(entries) => entries,
                Err(error) if path == root => return Err(error),
                Err(_) => {
                    snapshot.truncated = true;
                    continue;
                }
            };
            for entry in entries {
                if entry.directory {
                    if !NOT_SEARCHED.contains(&entry.name.as_str()) {
                        stack.push(entry.path);
                    }
                } else {
                    snapshot.files.push(entry);
                }
            }
            if budget.truncated {
                break;
            }
        }
    }
    snapshot.status = status(root);
    snapshot.truncated |= budget.truncated;
    Ok(snapshot)
}

fn status(root: &Path) -> Vec<(PathBuf, Status)> {
    use std::process::{Command, Stdio};
    let Ok(mut child) = Command::new("git")
        .args(["status", "--porcelain=v1", "-z"])
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        let _ = stdout
            .take(MAX_LIST_BYTES as u64 + 1)
            .read_to_end(&mut bytes);
    }
    if bytes.len() > MAX_LIST_BYTES {
        let _ = child.kill();
    }
    let Ok(result) = child.wait() else {
        return Vec::new();
    };
    if !result.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut entries = text.split('\0').filter(|part| !part.is_empty());
    let mut found = Vec::new();
    while let Some(entry) = entries.next() {
        let (Some(code), Some(path)) = (entry.get(..2), entry.get(3..)) else {
            continue;
        };
        let state = if code.contains('R') || code.contains('C') {
            entries.next();
            Status::Modified
        } else if code.contains('D') {
            Status::Deleted
        } else if code == "??" || code.contains('A') {
            Status::Added
        } else {
            Status::Modified
        };
        found.push((PathBuf::from(path.trim_end_matches('/')), state));
        if found.len() == MAX_ENTRIES {
            break;
        }
    }
    found
}

/// Replace the target atomically where its directory permits staging.
pub fn replace_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let Ok(target) = std::fs::canonicalize(path) else {
        return std::fs::write(path, bytes);
    };
    let permissions = std::fs::metadata(&target)
        .ok()
        .map(|meta| meta.permissions());
    if permissions
        .as_ref()
        .is_some_and(|permissions| permissions.readonly())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the file is read-only",
        ));
    }
    let mut staging = target.as_os_str().to_owned();
    staging.push(format!(
        ".ferrite-save-{}-{}",
        std::process::id(),
        WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    let staging = PathBuf::from(staging);
    let mut file = match std::fs::File::create_new(&staging) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            return std::fs::write(&target, bytes);
        }
        Err(error) => return Err(error),
    };
    let result = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| {
            drop(file);
            permissions.map_or(Ok(()), |permissions| {
                std::fs::set_permissions(&staging, permissions)
            })
        })
        .and_then(|()| std::fs::rename(&staging, &target));
    if result.is_err() {
        let _ = std::fs::remove_file(&staging);
    }
    result
}

#[cfg(test)]
mod tests;
