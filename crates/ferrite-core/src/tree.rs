//! A directory tree moved to another computer over the Ferrite protocol
//! (ADR 0048). Each side lists what it holds as a [`Manifest`]; [`plan`]
//! says what makes the far side's equal this side's, and [`send`] does it
//! through the far side's Server, asking ahead of the replies. What is sent
//! is worked out from the two manifests alone, never from a record of an
//! earlier run, so a rerun after any interruption sends only what still
//! differs.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::chunked::{self, Cancellation, Progress};
use crate::link;
use crate::protocol::{ClientMessage, Command, Done, Op};

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

/// What a tree op works on. Each side resolves it itself. A kind this side
/// does not know, whatever it carries, reads as `Unknown`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Root {
    /// A directory of the Server's computer, `~` its home.
    Path { path: PathBuf },
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
    /// Start writing a file as `write`, a number the sender gives it, so
    /// its chunks can follow without waiting for the reply: its bytes go to
    /// a temporary file beside it until `finish` renames it into place.
    Begin {
        write: u64,
        root: Root,
        path: String,
        size: u64,
        executable: bool,
    },
    /// Base64, at most [`chunked::CHUNK_SIZE`] bytes, in order. Replied
    /// `written`.
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
    Written { offset: u64 },
    Bytes { data: String, size: u64 },
    Done,
    #[serde(other)]
    Unknown,
}

/// How long a Server is given to answer a chunk or any one entry's op,
/// once what was asked before it is answered.
const PATIENCE: Duration = Duration::from_secs(30);
/// A scan hashes every file it has not seen before.
const SCAN_PATIENCE: Duration = Duration::from_secs(600);
/// How far a send asks ahead of the replies: at most this many requests,
/// holding at most this many bytes of files.
const AHEAD: usize = 64;
const AHEAD_BYTES: u64 = 4 << 20;
/// Each write's number, never the same twice in this process: a send cut
/// short leaves its last write under way on a connection the next send
/// may use.
static WRITES: AtomicU64 = AtomicU64::new(0);

/// What tree ops are asked through: a request goes out at once and its
/// reply is waited for later, the replies coming in the order asked.
pub trait Exchange {
    fn ask(&mut self, request: Request) -> io::Result<Asked>;

    /// The reply to `asked`. Replies to what was asked before it and never
    /// waited for are passed over.
    fn answer(&mut self, asked: Asked) -> io::Result<Reply>;

    fn call(&mut self, request: Request) -> io::Result<Reply> {
        let asked = self.ask(request)?;
        self.answer(asked)
    }
}

/// A request asked and not yet answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asked {
    id: u64,
    patience: Duration,
}

/// Opens an [`Exchange`], off the window's thread: it may run ssh.
pub type Connect = Box<dyn FnOnce() -> io::Result<Box<dyn Exchange>> + Send>;

/// The Exchange over a connection of its own to a Server.
pub struct Remote {
    client: link::Client,
    last: u64,
}

impl Remote {
    pub fn new(client: link::Client) -> Self {
        Self { client, last: 0 }
    }
}

impl Exchange for Remote {
    fn ask(&mut self, request: Request) -> io::Result<Asked> {
        let patience = match request {
            Request::Scan { .. } => SCAN_PATIENCE,
            _ => PATIENCE,
        };
        self.last += 1;
        self.client.send(&ClientMessage::Command(Command {
            id: self.last,
            op: Op::Tree { request },
        }))?;
        Ok(Asked {
            id: self.last,
            patience,
        })
    }

    fn answer(&mut self, asked: Asked) -> io::Result<Reply> {
        match self.client.answer(asked.id, asked.patience)? {
            Some(Done::Tree { reply }) => Ok(reply),
            _ => Err(unanswered()),
        }
    }
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

impl Step {
    fn path(&self) -> &str {
        match self {
            Step::Remove(path) | Step::MakeDir(path) => path,
            Step::Link { path, .. } | Step::Write { path, .. } => path,
        }
    }
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

impl Sent {
    fn took(&mut self, step: &Step) {
        match step {
            Step::Remove(_) => self.removed += 1,
            Step::MakeDir(_) => self.made += 1,
            Step::Link { .. } => self.linked += 1,
            Step::Write { size, .. } => {
                self.written += 1;
                self.bytes += size;
            }
        }
    }
}

/// Take each of `steps` to `dest` through `exchange`, the files read from
/// under `source`, asking ahead of the replies. `progress` hears how many
/// bytes of all the steps' files the far side has. An error names the
/// entry it was for; what was asked after it may still be taken there,
/// since each step is whole on its own.
pub fn send(
    source: &Path,
    steps: &[Step],
    dest: &Root,
    exchange: &mut dyn Exchange,
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
    progress(Progress { sent: 0, total });
    let mut flight = Flight {
        exchange,
        progress,
        waiting: VecDeque::new(),
        ahead: 0,
        received: 0,
        total,
        sent: Sent::default(),
    };
    for step in steps {
        cancel.check()?;
        let root = dest.clone();
        let request = match step {
            Step::Remove(path) => Request::Remove {
                root,
                path: path.clone(),
            },
            Step::MakeDir(path) => Request::MakeDir {
                root,
                path: path.clone(),
            },
            Step::Link { path, target } => Request::Link {
                root,
                path: path.clone(),
                target: target.clone(),
            },
            Step::Write {
                path,
                size,
                executable,
            } => {
                let write = WRITES.fetch_add(1, Ordering::Relaxed) + 1;
                let file =
                    std::fs::File::open(local(source, path)).map_err(|error| named(step, error))?;
                let begin = Request::Begin {
                    write,
                    root,
                    path: path.clone(),
                    size: *size,
                    executable: *executable,
                };
                flight.ask(step, begin, Expect::Begun)?;
                let mut pieces = chunked::Pieces::new(file, *size);
                for piece in &mut pieces {
                    cancel.check()?;
                    let (piece, data) = piece.map_err(|error| named(step, error))?;
                    let chunk = Request::Chunk {
                        write,
                        offset: piece.start,
                        data,
                    };
                    flight.ask(step, chunk, Expect::Written(piece))?;
                }
                let sha256 = pieces.sha256().map_err(|error| named(step, error))?;
                Request::Finish { write, sha256 }
            }
        };
        flight.ask(step, request, Expect::Taken)?;
    }
    flight.land()
}

/// What a send has asked and not yet heard the replies to, oldest first.
struct Flight<'a> {
    exchange: &'a mut dyn Exchange,
    progress: &'a mut dyn FnMut(Progress),
    waiting: VecDeque<(Asked, &'a Step, Expect)>,
    /// Bytes of files in what `waiting` asked.
    ahead: u64,
    /// Bytes of files the far side has.
    received: u64,
    total: u64,
    sent: Sent,
}

/// What a reply says when all went well.
enum Expect {
    /// `done`: the step taken.
    Taken,
    /// `done`: the write under way.
    Begun,
    /// `written`: the file received to the end of this piece.
    Written(Range<u64>),
}

impl<'a> Flight<'a> {
    /// Ask `request` for `step`, first hearing the oldest replies while
    /// as much as may be is ahead.
    fn ask(&mut self, step: &'a Step, request: Request, expect: Expect) -> io::Result<()> {
        let bytes = match &expect {
            Expect::Written(piece) => piece.end - piece.start,
            _ => 0,
        };
        while self.waiting.len() >= AHEAD
            || (!self.waiting.is_empty() && self.ahead + bytes > AHEAD_BYTES)
        {
            self.hear()?;
        }
        let asked = self.exchange.ask(request).map_err(|error| named(step, error))?;
        self.ahead += bytes;
        self.waiting.push_back((asked, step, expect));
        Ok(())
    }

    fn hear(&mut self) -> io::Result<()> {
        let Some((asked, step, expect)) = self.waiting.pop_front() else {
            return Ok(());
        };
        let reply = self.exchange.answer(asked).map_err(|error| named(step, error))?;
        match (expect, reply) {
            (Expect::Taken, Reply::Done) => self.sent.took(step),
            (Expect::Begun, Reply::Done) => {}
            (Expect::Written(piece), Reply::Written { offset }) if offset == piece.end => {
                let bytes = piece.end - piece.start;
                self.ahead -= bytes;
                self.received += bytes;
                (self.progress)(Progress {
                    sent: self.received,
                    total: self.total,
                });
            }
            _ => return Err(named(step, unanswered())),
        }
        Ok(())
    }

    /// What the send did, once every reply is heard.
    fn land(mut self) -> io::Result<Sent> {
        while !self.waiting.is_empty() {
            self.hear()?;
        }
        Ok(self.sent)
    }
}

/// Make `dest`, through `exchange`, hold what `source` holds, `skip` (paths
/// in both) left as each side has it: both sides scanned at once, then
/// [`plan`] and [`send`].
pub fn mirror(
    source: &Path,
    dest: &Root,
    skip: &[String],
    exchange: &mut dyn Exchange,
    progress: &mut dyn FnMut(Progress),
    cancel: &Cancellation,
) -> io::Result<Sent> {
    let there = exchange.ask(Request::Scan {
        root: dest.clone(),
        skip: skip.to_vec(),
    })?;
    let here = scan(source, skip)?;
    let Reply::Scanned { manifest: there } = exchange.answer(there)? else {
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

fn named(step: &Step, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{}: {error}", step.path()))
}

fn unanswered() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "The server did not answer the tree operation",
    )
}
