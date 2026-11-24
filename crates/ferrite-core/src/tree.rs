//! A directory tree moved to another computer over the Ferrite protocol
//! (ADR 0048). Each side lists what it holds as a [`Manifest`]; [`plan`]
//! says what makes the far side's equal this side's, and [`send`] does it
//! through the far side's Server, one [`Request`] at a time. What is sent
//! is worked out from the two manifests alone, never from a record of an
//! earlier run, so a rerun after any interruption sends only what still
//! differs.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::chunked::{self, Cancellation, Progress};
use crate::link;
use crate::protocol::{Done, Op};

mod scan;
mod serve;
#[cfg(test)]
mod tests;

pub use scan::scan;
pub use serve::Writes;

/// What a tree holds, by path: relative, `/`-separated, never absolute and
/// never through `..`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub entries: BTreeMap<String, Entry>,
    /// Whether this side keeps an executable bit (Windows does not). The bit
    /// is a difference only between two sides that both keep one.
    #[serde(default)]
    pub modes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    Dir,
    File {
        size: u64,
        sha256: String,
        executable: bool,
    },
    Symlink {
        target: String,
    },
    #[serde(other)]
    Unknown,
}

/// What a tree op works on. Each side resolves it itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "path", rename_all = "snake_case")]
pub enum Root {
    /// A directory of the Server's computer, `~` its home.
    Path(PathBuf),
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Request {
    /// Replied `scanned`. A root that is not there yet holds nothing.
    Scan {
        root: Root,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skip: Vec<String>,
    },
    /// Start writing a file, replied `begun`: its bytes go to a temporary
    /// file beside it until `finish` renames it into place.
    Begin {
        root: Root,
        path: String,
        size: u64,
        executable: bool,
    },
    /// Base64, at most [`chunked::CHUNK_SIZE`] bytes. Replied `written`.
    Chunk {
        write: u64,
        offset: u64,
        data: String,
    },
    Finish {
        write: u64,
        sha256: String,
    },
    /// Up to a chunk of a file from `offset`, replied `bytes`.
    Read {
        root: Root,
        path: String,
        offset: u64,
    },
    MakeDir {
        root: Root,
        path: String,
    },
    Link {
        root: Root,
        path: String,
        target: String,
    },
    /// The entry and all under it. One already gone is removed.
    Remove {
        root: Root,
        path: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Reply {
    Scanned { manifest: Manifest },
    Begun { write: u64 },
    Written { offset: u64 },
    Bytes { data: String, size: u64 },
    Done,
    #[serde(other)]
    Unknown,
}

/// How long a Server is given to answer a chunk or any one entry's op.
const PATIENCE: Duration = Duration::from_secs(30);
/// A scan hashes every file it has not seen before.
const SCAN_PATIENCE: Duration = Duration::from_secs(600);

/// One request, answered: what a tree op is asked through.
pub type Exchange = Box<dyn FnMut(Request) -> io::Result<Reply>>;

/// Opens an [`Exchange`], off the window's thread: it may run ssh.
pub type Connect = Box<dyn FnOnce() -> io::Result<Exchange> + Send>;

/// The Exchange over `client`, a connection of its own to a Server.
pub fn exchange(mut client: link::Client) -> Exchange {
    let mut next = 0;
    Box::new(move |request| {
        next += 1;
        let patience = match request {
            Request::Scan { .. } => SCAN_PATIENCE,
            _ => PATIENCE,
        };
        match client.ask(next, Op::Tree { request }, patience)? {
            Some(Done::Tree { reply }) => Ok(reply),
            _ => Err(unanswered()),
        }
    })
}

/// One thing that makes the far side's tree more like this side's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Remove(String),
    MakeDir(String),
    Write {
        path: String,
        size: u64,
        executable: bool,
    },
    Link {
        path: String,
        target: String,
    },
}

/// What makes `dest` equal `source`: first what must go, the outermost
/// entry of each removal only, then what is made, in path order so a
/// directory is made before what it holds. A file of the same kind is
/// rewritten in place.
pub fn plan(source: &Manifest, dest: &Manifest) -> Vec<Step> {
    let modes = source.modes && dest.modes;
    let mut removed = BTreeSet::new();
    for (path, there) in &dest.entries {
        if inside_any(path, &removed) {
            continue;
        }
        let stays = match (source.entries.get(path), there) {
            (Some(Entry::Dir), Entry::Dir) | (Some(Entry::File { .. }), Entry::File { .. }) => true,
            (Some(here @ Entry::Symlink { .. }), there) => here == there,
            _ => false,
        };
        if !stays {
            removed.insert(path.as_str());
        }
    }
    let mut steps: Vec<Step> = removed.iter().map(|path| Step::Remove(path.to_string())).collect();
    for (path, here) in &source.entries {
        let there = dest
            .entries
            .get(path)
            .filter(|_| !removed.contains(path.as_str()) && !inside_any(path, &removed));
        match here {
            Entry::Dir if there.is_none() => steps.push(Step::MakeDir(path.clone())),
            Entry::File {
                size,
                sha256,
                executable,
            } => {
                let same = matches!(
                    there,
                    Some(Entry::File { size: s, sha256: h, executable: x })
                        if s == size && h == sha256 && (!modes || x == executable)
                );
                if !same {
                    steps.push(Step::Write {
                        path: path.clone(),
                        size: *size,
                        executable: *executable,
                    });
                }
            }
            Entry::Symlink { target } if there.is_none() => steps.push(Step::Link {
                path: path.clone(),
                target: target.clone(),
            }),
            _ => {}
        }
    }
    steps
}

fn inside_any(path: &str, dirs: &BTreeSet<&str>) -> bool {
    path.match_indices('/').any(|(at, _)| dirs.contains(&path[..at]))
}

/// What a [`send`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Sent {
    pub removed: usize,
    pub made: usize,
    pub written: usize,
    pub linked: usize,
    pub bytes: u64,
}

/// Take each of `steps` to `dest` through `exchange`, the files read from
/// under `source`. `progress` hears the bytes written of all the steps'.
pub fn send(
    source: &Path,
    steps: &[Step],
    dest: &Root,
    exchange: &mut dyn FnMut(Request) -> io::Result<Reply>,
    progress: &mut dyn FnMut(Progress),
    cancel: &Cancellation,
) -> io::Result<Sent> {
    let total = steps
        .iter()
        .map(|step| match step {
            Step::Write { size, .. } => *size,
            _ => 0,
        })
        .sum();
    let mut sent = Sent::default();
    progress(Progress { sent: 0, total });
    for step in steps {
        cancel.check()?;
        let root = dest.clone();
        let (path, taken) = match step {
            Step::Remove(path) => (path, done(exchange(Request::Remove { root, path: path.clone() }))),
            Step::MakeDir(path) => (path, done(exchange(Request::MakeDir { root, path: path.clone() }))),
            Step::Link { path, target } => (
                path,
                done(exchange(Request::Link {
                    root,
                    path: path.clone(),
                    target: target.clone(),
                })),
            ),
            Step::Write {
                path,
                size,
                executable,
            } => {
                let before = sent.bytes;
                let written = (|| {
                    let file = std::fs::File::open(local(source, path))?;
                    let Reply::Begun { write } = exchange(Request::Begin {
                        root,
                        path: path.clone(),
                        size: *size,
                        executable: *executable,
                    })?
                    else {
                        return Err(unanswered());
                    };
                    let sha256 = chunked::send(
                        file,
                        *size,
                        |offset, data| match exchange(Request::Chunk { write, offset, data })? {
                            Reply::Written { offset } => Ok(offset),
                            _ => Err(unanswered()),
                        },
                        |at| progress(Progress { sent: before + at, total }),
                        cancel,
                    )?;
                    done(exchange(Request::Finish { write, sha256 }))
                })();
                (path, written)
            }
        };
        taken.map_err(|error| io::Error::new(error.kind(), format!("{path}: {error}")))?;
        match step {
            Step::Remove(_) => sent.removed += 1,
            Step::MakeDir(_) => sent.made += 1,
            Step::Link { .. } => sent.linked += 1,
            Step::Write { size, .. } => {
                sent.written += 1;
                sent.bytes += size;
            }
        }
    }
    Ok(sent)
}

/// Make `dest`, through `exchange`, hold what `source` holds, `skip` (paths
/// in both) left as each side has it: [`plan`] from both sides' manifests,
/// then [`send`].
pub fn mirror(
    source: &Path,
    dest: &Root,
    skip: &[String],
    exchange: &mut dyn FnMut(Request) -> io::Result<Reply>,
    progress: &mut dyn FnMut(Progress),
    cancel: &Cancellation,
) -> io::Result<Sent> {
    let here = scan(source, skip)?;
    let Reply::Scanned { manifest: there } = exchange(Request::Scan {
        root: dest.clone(),
        skip: skip.to_vec(),
    })?
    else {
        return Err(unanswered());
    };
    send(source, &plan(&here, &there), dest, exchange, progress, cancel)
}

/// Refuse a path a manifest cannot hold: empty, absolute, through `.` or
/// `..`, or more than one name where this computer reads it (a `\` or a
/// drive on Windows).
pub fn check_path(path: &str) -> io::Result<()> {
    let one_name = |part: &str| {
        !matches!(part, "" | "." | "..")
            && !part.contains('\0')
            && !(cfg!(windows) && part.contains([':', '\\']))
            && matches!(
                Path::new(part).components().collect::<Vec<_>>()[..],
                [Component::Normal(name)] if name == part
            )
    };
    if path.split('/').all(one_name) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{path}` is not a relative path inside the tree"),
        ))
    }
}

/// `path` as this computer spells it under `root`.
fn local(root: &Path, path: &str) -> PathBuf {
    path.split('/').fold(root.to_path_buf(), |at, part| at.join(part))
}

fn done(reply: io::Result<Reply>) -> io::Result<()> {
    match reply? {
        Reply::Done => Ok(()),
        _ => Err(unanswered()),
    }
}

fn unanswered() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "The server did not answer the tree operation",
    )
}
