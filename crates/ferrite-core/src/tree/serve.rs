//! The Server's half of a tree's ops, for one connection, run in the order
//! they were asked (ADR 0048). A chunk or finish of a write that failed,
//! expired or never began is refused, so what a sender asked after a
//! failure never puts part of a file in place. An entry of a `Root::Path`
//! that this side cannot make, by its name, its place or what it is, is
//! answered `unmade`, and the sender goes on without it; a root that cannot
//! be made fails every op. Nothing is written through a symlink inside a
//! root, and only a link this side makes reads through one
//! (`links::link`). A root is resolved once, links
//! followed, and refused if it is the home directory, an ancestor of it, or
//! the filesystem's root, which a mirror would empty, however it is
//! spelled. The Setup root is the operator's setup in this side's home
//! directory, as `setup_sync` spells it, and each write to it is on disk
//! before it is in place.

use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use base64::Engine as _;

use super::links::link;
use super::{at, canonical, check_path, holds, local, scan, windows_name, Manifest, Refusal, Reply, Request, Root};
use crate::chunked::{self, CHUNK_SIZE};
use crate::setup_sync::{Home, Place};

/// At most this many files are written at once on one connection.
pub(crate) const MOST_WRITES: usize = 4;

/// A scan's reply goes in pages of at most about this many bytes, far
/// below what a connection may fall behind by (`link::LAGGED_BYTES`).
pub(super) const PAGE_BYTES: usize = 4 << 20;

/// The bits a directory a tree op makes ends with, whatever the umask: open
/// to read, as the usual umask makes one. The Server's own umask (077,
/// `machine/bootstrap.sh`) is for its state, and a manifest lists no
/// directory's bits for a sender to give.
const DIR_MODE: u32 = 0o755;

enum Target {
    Path {
        path: PathBuf,
        /// The permission bits it ends with, where this side keeps them.
        mode: Option<u32>,
    },
    Setup(Place),
}

/// One connection's tree ops: the files it is writing, each received
/// beside where it goes, and the scan whose pages it is reading.
pub struct Session {
    writes: chunked::Incoming<Target>,
    scanned: Option<Scanned>,
    /// The setup the Setup root names: this process's home directory's
    /// unless given.
    home: Option<Home>,
}

/// A scan not yet read to its last page.
struct Scanned {
    root: Resolved,
    skip: Vec<String>,
    manifest: Manifest,
}

#[derive(PartialEq, Eq)]
enum Resolved {
    Path(PathBuf),
    Setup,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            writes: chunked::Incoming::new(MOST_WRITES),
            scanned: None,
            home: None,
        }
    }
}

impl Session {
    /// Ops on the Setup root work on `home`.
    pub fn with_home(home: Home) -> Self {
        Self {
            home: Some(home),
            ..Self::default()
        }
    }

    pub fn expire(&mut self) {
        self.writes.expire();
    }

    fn home(&mut self) -> io::Result<&Home> {
        let home = match self.home.take() {
            Some(home) => home,
            None => Home::from_env()?,
        };
        Ok(self.home.insert(home))
    }

    pub fn execute(&mut self, request: Request) -> io::Result<Reply> {
        match request {
            Request::Scan { root, skip, after } => {
                let root = match root {
                    Root::Setup => Resolved::Setup,
                    root => Resolved::Path(resolve(&root)?),
                };
                let manifest = match self.scanned.take() {
                    Some(kept) if after.is_some() && kept.root == root && kept.skip == skip => {
                        kept.manifest
                    }
                    _ => match &root {
                        Resolved::Setup => self.home()?.scan(&skip),
                        Resolved::Path(path) => match fs::symlink_metadata(path) {
                            Err(error) if error.kind() == io::ErrorKind::NotFound => Manifest::default(),
                            _ => scan::sweeping(path, &skip)?,
                        },
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
                root: Root::Setup,
                path,
                size,
                mode,
            } => {
                let place = self.home()?.receive(&path, mode)?;
                let (dir, made) = (place.dir.clone(), place.mode);
                self.writes.begin(write, &dir, size, made, Target::Setup(place))?;
                Ok(Reply::Done)
            }
            Request::Begin {
                write,
                root,
                path,
                size,
                mode,
            } => {
                let path = match placed(&made(&root)?, &path)? {
                    Ok(path) => path,
                    Err(why) => return Ok(Reply::Unmade(why)),
                };
                let there = fs::symlink_metadata(&path).ok();
                if there.as_ref().is_some_and(fs::Metadata::is_dir) {
                    return Err(invalid(format!("{} is a directory", path.display())));
                }
                let mode = permissions(there.as_ref(), mode);
                let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
                self.writes.begin(write, &dir, size, mode, Target::Path { path, mode })?;
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
                match received.what() {
                    Target::Path { path, mode } => {
                        if let Some(mode) = mode {
                            set_mode(received.path(), *mode)?;
                        }
                        if let Err(error) = fs::rename(received.path(), path) {
                            return Ok(Reply::Unmade(Refusal::failed(error)));
                        }
                    }
                    Target::Setup(place) => {
                        received.sync()?;
                        self.home()?.place(place, received.path())?;
                    }
                }
                Ok(Reply::Done)
            }
            Request::Read {
                root: Root::Setup,
                path,
                offset,
            } => {
                let (bytes, size) = self.home()?.read(&path, offset)?;
                Ok(Reply::Bytes {
                    data: base64::engine::general_purpose::STANDARD.encode(bytes),
                    size,
                })
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
            Request::MakeDir {
                root: Root::Setup,
                path,
            } => Ok(done(self.home()?.make_dir(&path)?)),
            Request::MakeDir { root, path } => {
                let path = match placed(&made(&root)?, &path)? {
                    Ok(path) => path,
                    Err(why) => return Ok(Reply::Unmade(why)),
                };
                match fs::create_dir(&path) {
                    Ok(()) => {
                        set_mode(&path, DIR_MODE).map_err(|error| at(&path, error))?;
                        Ok(Reply::Done)
                    }
                    Err(error)
                        if error.kind() == io::ErrorKind::AlreadyExists
                            && fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir()) =>
                    {
                        Ok(Reply::Unchanged)
                    }
                    Err(error) => Ok(Reply::Unmade(Refusal::failed(error))),
                }
            }
            Request::Link { root: Root::Setup, .. } => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Setup sync makes no links",
            )),
            Request::Link { root, path, target } => {
                let root = made(&root)?;
                Ok(match placed(&root, &path)?.and_then(|path| link(&root, &path, &target)) {
                    Ok(changed) => done(changed),
                    Err(why) => Reply::Unmade(why),
                })
            }
            Request::Remove {
                root: Root::Setup,
                path,
            } => Ok(done(self.home()?.remove(&path)?)),
            Request::Remove { root, path } => Ok(done(remove(&resolve(&root)?, &path)?)),
            Request::Unknown => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "This server does not know that tree operation",
            )),
        }
    }
}

/// `done` when it was done, `unchanged` when it was already so.
fn done(changed: bool) -> Reply {
    if changed {
        Reply::Done
    } else {
        Reply::Unchanged
    }
}

fn resolve(root: &Root) -> io::Result<PathBuf> {
    let Root::Path { path: given } = root else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "This server does not know that kind of root",
        ));
    };
    let given = crate::folder::expand_home(given);
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
    holds_dir(root, &crate::folder::expand_home(Path::new("~")))
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

/// The root, made with each directory on the way to it that is not there
/// yet: a tree is often sent to a directory that does not exist.
fn made(root: &Root) -> io::Result<PathBuf> {
    let root = resolve(root)?;
    let missing: Vec<PathBuf> = root
        .ancestors()
        .take_while(|dir| fs::symlink_metadata(dir).is_err())
        .map(Path::to_path_buf)
        .collect();
    fs::create_dir_all(&root).map_err(|error| at(&root, error))?;
    for dir in missing {
        set_mode(&dir, DIR_MODE).map_err(|error| at(&dir, error))?;
    }
    Ok(root)
}

/// Where the entry at `path` is made in `root`, or why this side leaves it
/// unmade: a name it cannot hold, or a folder on the way that is not one
/// here. A path that is not in the tree is refused.
fn placed(root: &Path, path: &str) -> io::Result<Result<PathBuf, Refusal>> {
    let named = |name: &str| matches!(name, "" | "." | "..") || windows_name(name);
    if cfg!(windows) && !path.split('/').all(named) {
        return Ok(Err(Refusal::Name));
    }
    check_path(path)?;
    Ok(inside(root, path).map_err(Refusal::failed))
}

/// `path` in `root`, refused unless every directory on the way is a real
/// one there.
pub(crate) fn inside(root: &Path, path: &str) -> io::Result<PathBuf> {
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
pub(crate) fn remove(root: &Path, path: &str) -> io::Result<bool> {
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

/// The permission bits a file written over `there` ends with. A file
/// already there keeps its own, its executable bits following the
/// sender's where the sender keeps them; a new one takes the sender's.
/// `None`: as it is made, by the process's umask.
#[cfg(unix)]
pub(crate) fn permissions(there: Option<&fs::Metadata>, sent: Option<u32>) -> Option<u32> {
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
pub(crate) fn permissions(_: Option<&fs::Metadata>, _: Option<u32>) -> Option<u32> {
    None
}

#[cfg(unix)]
pub(crate) fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
pub(crate) fn set_mode(_: &Path, _: u32) -> io::Result<()> {
    Ok(())
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
