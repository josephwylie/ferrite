//! The Server's half of a tree's ops, for one connection, run in the order
//! they were asked (ADR 0048). A chunk or finish of a write that failed,
//! expired or never began is refused, so what a sender asked after a
//! failure never puts part of a file in place. Nothing is read or written
//! through a symlink inside a root, and no root is the home directory, an
//! ancestor of it, or the filesystem's root, which a mirror would empty.

use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use base64::Engine as _;

use super::{check_path, local, scan, Manifest, Reply, Request, Root};
use crate::chunked::{self, CHUNK_SIZE};

/// At most this many files are written at once on one connection.
const MOST_WRITES: usize = 4;

struct Target {
    path: PathBuf,
    executable: bool,
}

/// The files one connection is writing, each received beside where it goes.
pub struct Writes(chunked::Incoming<Target>);

impl Default for Writes {
    fn default() -> Self {
        Self(chunked::Incoming::new(MOST_WRITES))
    }
}

impl Writes {
    pub fn expire(&mut self) {
        self.0.expire();
    }

    pub fn execute(&mut self, request: Request) -> io::Result<Reply> {
        match request {
            Request::Scan { root, skip } => {
                let root = resolve(&root)?;
                let manifest = match fs::symlink_metadata(&root) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Manifest {
                        modes: cfg!(unix),
                        ..Manifest::default()
                    },
                    _ => scan(&root, &skip)?,
                };
                Ok(Reply::Scanned { manifest })
            }
            Request::Begin {
                write,
                root,
                path,
                size,
                executable,
            } => {
                let path = inside(&made(&root)?, &path)?;
                if fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir()) {
                    return Err(invalid(format!("{} is a directory", path.display())));
                }
                let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
                self.0.begin(write, &dir, size, None, Target { path, executable })?;
                Ok(Reply::Done)
            }
            Request::Chunk {
                write,
                offset,
                data,
            } => Ok(Reply::Written {
                offset: self.0.chunk(write, offset, &data)?,
            }),
            Request::Finish { write, sha256 } => {
                let received = self.0.finish(write, &sha256)?;
                let target = received.what();
                if target.executable {
                    executable(received.path())?;
                }
                fs::rename(received.path(), &target.path)?;
                Ok(Reply::Done)
            }
            Request::Read { root, path, offset } => {
                let path = inside(&resolve(&root)?, &path)?;
                let meta = fs::symlink_metadata(&path)?;
                if !meta.is_file() || offset > meta.len() {
                    return Err(invalid(format!("{} has no file to read there", path.display())));
                }
                let mut file = fs::File::open(&path)?;
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
                    Err(error)
                        if error.kind() == io::ErrorKind::AlreadyExists
                            && fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir()) => {}
                    made => made?,
                }
                Ok(Reply::Done)
            }
            Request::Link { root, path, target } => {
                link(&target, &inside(&made(&root)?, &path)?)?;
                Ok(Reply::Done)
            }
            Request::Remove { root, path } => {
                remove(&resolve(&root)?, &path)?;
                Ok(Reply::Done)
            }
            Request::Unknown => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "This server does not know that tree operation",
            )),
        }
    }
}

fn resolve(root: &Root) -> io::Result<PathBuf> {
    let Root::Path { path } = root else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "This server does not know that kind of root",
        ));
    };
    let path = crate::workspace::expand_home(path);
    let home = crate::workspace::expand_home(Path::new("~"));
    if !path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
        return Err(invalid(format!("{} is not an absolute path", path.display())));
    }
    if path.parent().is_none() || home.starts_with(&path) {
        return Err(invalid(format!(
            "{} holds the home directory: name a folder inside it",
            path.display()
        )));
    }
    Ok(path)
}

/// The root, made if it is not there yet: a tree is often sent to a
/// directory that does not exist.
fn made(root: &Root) -> io::Result<PathBuf> {
    let root = resolve(root)?;
    fs::create_dir_all(&root)?;
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

fn remove(root: &Path, path: &str) -> io::Result<()> {
    let gone = |error: &io::Error| error.kind() == io::ErrorKind::NotFound;
    let path = match inside(root, path) {
        Err(error) if gone(&error) => return Ok(()),
        inside => inside?,
    };
    let meta = match fs::symlink_metadata(&path) {
        Err(error) if gone(&error) => return Ok(()),
        meta => meta?,
    };
    if meta.is_dir() {
        return fs::remove_dir_all(&path);
    }
    // A Windows symlink to a directory is removed as a directory.
    fs::remove_file(&path).or_else(|error| {
        if cfg!(windows) {
            fs::remove_dir(&path)
        } else {
            Err(error)
        }
    })
}

#[cfg(unix)]
fn link(target: &str, path: &Path) -> io::Result<()> {
    if fs::read_link(path).is_ok_and(|there| there == Path::new(target)) {
        return Ok(());
    }
    std::os::unix::fs::symlink(target, path)
}

#[cfg(not(unix))]
fn link(_: &str, path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{}: this Server does not make symlinks on Windows", path.display()),
    ))
}

/// Executable by whoever may read it.
#[cfg(unix)]
fn executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)?.permissions().mode();
    fs::set_permissions(path, fs::Permissions::from_mode(mode | (mode & 0o444) >> 2))
}

#[cfg(not(unix))]
fn executable(_: &Path) -> io::Result<()> {
    Ok(())
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
