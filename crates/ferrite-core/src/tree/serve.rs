//! The Server's half of a tree's ops, for one connection, run in the order
//! they were asked (ADR 0048). A chunk or finish of a write that failed,
//! expired or never began is refused, so what a sender asked after a
//! failure never puts part of a file in place. Nothing is read or written
//! through a symlink inside a root. A root is resolved once, links
//! followed, and refused if it is the home directory, an ancestor of it, or
//! the filesystem's root, which a mirror would empty, however it is
//! spelled.

use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use base64::Engine as _;

use super::{at, canonical, check_path, holds, local, scan, Manifest, Reply, Request, Root};
use crate::chunked::{self, CHUNK_SIZE};

/// At most this many files are written at once on one connection.
const MOST_WRITES: usize = 4;

/// A scan's reply goes in pages of at most about this many bytes, far
/// below what a connection may fall behind by (`link::LAGGED_BYTES`).
pub(super) const PAGE_BYTES: usize = 4 << 20;

struct Target {
    path: PathBuf,
    /// The permission bits it ends with, where this side keeps them.
    mode: Option<u32>,
}

/// One connection's tree ops: the files it is writing, each received
/// beside where it goes, and the scan whose pages it is reading.
pub struct Session {
    writes: chunked::Incoming<Target>,
    scanned: Option<Scanned>,
}

/// A scan not yet read to its last page.
struct Scanned {
    root: PathBuf,
    skip: Vec<String>,
    manifest: Manifest,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            writes: chunked::Incoming::new(MOST_WRITES),
            scanned: None,
        }
    }
}

impl Session {
    pub fn expire(&mut self) {
        self.writes.expire();
    }

    pub fn execute(&mut self, request: Request) -> io::Result<Reply> {
        match request {
            Request::Scan { root, skip, after } => {
                let root = resolve(&root)?;
                let manifest = match self.scanned.take() {
                    Some(kept) if after.is_some() && kept.root == root && kept.skip == skip => {
                        kept.manifest
                    }
                    _ => match fs::symlink_metadata(&root) {
                        Err(error) if error.kind() == io::ErrorKind::NotFound => Manifest::default(),
                        _ => scan::sweeping(&root, &skip)?,
                    },
                };
                let (page, more) = manifest.page(after.as_deref(), PAGE_BYTES);
                if more {
                    self.scanned = Some(Scanned {
                        root,
                        skip,
                        manifest,
                    });
                }
                Ok(Reply::Scanned {
                    manifest: page,
                    more,
                })
            }
            Request::Begin {
                write,
                root,
                path,
                size,
                mode,
            } => {
                let path = inside(&made(&root)?, &path)?;
                let there = fs::symlink_metadata(&path).ok();
                if there.as_ref().is_some_and(fs::Metadata::is_dir) {
                    return Err(invalid(format!("{} is a directory", path.display())));
                }
                let mode = permissions(there.as_ref(), mode);
                let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
                self.writes.begin(write, &dir, size, mode, Target { path, mode })?;
                Ok(Reply::Done)
            }
            Request::Chunk {
                write,
                offset,
                data,
            } => Ok(Reply::Written {
                offset: self.writes.chunk(write, offset, &data)?,
            }),
            Request::Finish { write, sha256 } => {
                let received = self.writes.finish(write, &sha256)?;
                let target = received.what();
                if let Some(mode) = target.mode {
                    set_mode(received.path(), mode)?;
                }
                fs::rename(received.path(), &target.path).map_err(|error| at(&target.path, error))?;
                Ok(Reply::Done)
            }
            Request::Read { root, path, offset } => {
                let path = inside(&resolve(&root)?, &path)?;
                let meta = fs::symlink_metadata(&path).map_err(|error| at(&path, error))?;
                if !meta.is_file() || offset > meta.len() {
                    return Err(invalid(format!("{} has no file to read there", path.display())));
                }
                let mut file = fs::File::open(&path).map_err(|error| at(&path, error))?;
                file.seek(SeekFrom::Start(offset))?;
                let mut bytes = Vec::with_capacity(CHUNK_SIZE);
                file.take(CHUNK_SIZE as u64).read_to_end(&mut bytes)?;
                Ok(Reply::Bytes {
                    data: base64::engine::general_purpose::STANDARD.encode(bytes),
                    size: meta.len(),
                })
            }
            Request::MakeDir { root, path } => {
                let path = inside(&made(&root)?, &path)?;
                match fs::create_dir(&path) {
                    Ok(()) => Ok(Reply::Done),
                    Err(error)
                        if error.kind() == io::ErrorKind::AlreadyExists
                            && fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir()) =>
                    {
                        Ok(Reply::Unchanged)
                    }
                    Err(error) => Err(at(&path, error)),
                }
            }
            Request::Link { root, path, target } => {
                let made = link(&target, &inside(&made(&root)?, &path)?)?;
                Ok(if made { Reply::Done } else { Reply::Unchanged })
            }
            Request::Remove { root, path } => {
                let removed = remove(&resolve(&root)?, &path)?;
                Ok(if removed { Reply::Done } else { Reply::Unchanged })
            }
            Request::Unknown => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "This server does not know that tree operation",
            )),
        }
    }
}

fn resolve(root: &Root) -> io::Result<PathBuf> {
    let Root::Path { path: given } = root else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "This server does not know that kind of root",
        ));
    };
    let given = crate::workspace::expand_home(given);
    if !given.is_absolute() || given.components().any(|part| part == Component::ParentDir) {
        return Err(invalid(format!("{} is not an absolute path", given.display())));
    }
    let path = canonical(&given).map_err(|error| at(&given, error))?;
    if path.parent().is_none() || holds_home(&path) {
        return Err(invalid(format!(
            "{} holds the home directory: name a folder inside it",
            given.display()
        )));
    }
    Ok(path)
}

/// Whether `root`, resolved, is the home directory or holds it.
fn holds_home(root: &Path) -> bool {
    holds_dir(root, &crate::workspace::expand_home(Path::new("~")))
}

/// Whether `root`, resolved, is `home` or holds it, by either spelling of
/// `home`: a home that is itself a link lives where the link points and
/// where the link stands, and a mirror into the directory holding the link
/// would remove it.
pub(super) fn holds_dir(root: &Path, home: &Path) -> bool {
    let resolved = canonical(home).unwrap_or_else(|_| home.to_path_buf());
    home.ancestors().chain(resolved.ancestors()).any(|dir| {
        let dir = canonical(dir).unwrap_or_else(|_| dir.to_path_buf());
        (holds(root, &dir) && holds(&dir, root)) || same_dir(root, &dir)
    })
}

/// Whether two paths name one directory, by its device and inode: what a
/// bind mount or a macOS firmlink spells twice.
#[cfg(unix)]
fn same_dir(one: &Path, other: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::metadata(one), fs::metadata(other)) {
        (Ok(one), Ok(other)) => (one.dev(), one.ino()) == (other.dev(), other.ino()),
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_dir(_: &Path, _: &Path) -> bool {
    false
}

/// The root, made if it is not there yet: a tree is often sent to a
/// directory that does not exist.
fn made(root: &Root) -> io::Result<PathBuf> {
    let root = resolve(root)?;
    fs::create_dir_all(&root).map_err(|error| at(&root, error))?;
    Ok(root)
}

/// `path` in `root`, refused unless every directory on the way is a real
/// one there.
fn inside(root: &Path, path: &str) -> io::Result<PathBuf> {
    check_path(path)?;
    let mut at = root.to_path_buf();
    if let Some((dirs, _)) = path.rsplit_once('/') {
        for part in dirs.split('/') {
            at.push(part);
            let meta = fs::symlink_metadata(&at)?;
            if !meta.is_dir() {
                return Err(invalid(format!(
                    "`{path}` lies through {}, which is not a directory",
                    at.display()
                )));
            }
        }
    }
    Ok(local(root, path))
}

/// Remove the entry at `path` and all under it: whether anything was there.
fn remove(root: &Path, path: &str) -> io::Result<bool> {
    let gone = |error: &io::Error| error.kind() == io::ErrorKind::NotFound;
    let path = match inside(root, path) {
        Err(error) if gone(&error) => return Ok(false),
        inside => inside?,
    };
    let meta = match fs::symlink_metadata(&path) {
        Err(error) if gone(&error) => return Ok(false),
        meta => meta?,
    };
    let removed = if meta.is_dir() {
        fs::remove_dir_all(&path)
    } else {
        // A Windows symlink to a directory is removed as a directory.
        fs::remove_file(&path).or_else(|error| {
            if cfg!(windows) {
                fs::remove_dir(&path)
            } else {
                Err(error)
            }
        })
    };
    removed.map(|()| true).map_err(|error| at(&path, error))
}

/// Make the symlink: whether it was not already so.
#[cfg(unix)]
fn link(target: &str, path: &Path) -> io::Result<bool> {
    if fs::read_link(path).is_ok_and(|there| there == Path::new(target)) {
        return Ok(false);
    }
    std::os::unix::fs::symlink(target, path).map_err(|error| at(path, error))?;
    Ok(true)
}

#[cfg(not(unix))]
fn link(_: &str, path: &Path) -> io::Result<bool> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{}: this Server does not make symlinks on Windows", path.display()),
    ))
}

/// The permission bits a file written over `there` ends with. A file
/// already there keeps its own, its executable bits following the
/// sender's where the sender keeps them; a new one takes the sender's.
/// `None`: as it is made, by the process's umask.
#[cfg(unix)]
fn permissions(there: Option<&fs::Metadata>, sent: Option<u32>) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    let kept = there
        .filter(|meta| meta.is_file())
        .map(|meta| meta.permissions().mode() & 0o777);
    match (kept, sent) {
        (Some(kept), Some(sent)) => Some(kept & !0o111 | sent & 0o111),
        (Some(kept), None) => Some(kept),
        (None, sent) => sent.map(|sent| sent & 0o777),
    }
}

#[cfg(not(unix))]
fn permissions(_: Option<&fs::Metadata>, _: Option<u32>) -> Option<u32> {
    None
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_: &Path, _: u32) -> io::Result<()> {
    Ok(())
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
