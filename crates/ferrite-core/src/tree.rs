//! A directory tree moved to another computer over the Ferrite protocol
//! (ADR 0048). Each side lists what it holds as a [`Manifest`]; [`plan`]
//! says what makes the far side's equal this side's, and [`send`] does it
//! through the far side's Server, asking ahead of the replies. What is sent
//! is worked out from the two manifests alone, never from a record of an
//! earlier run, so a rerun after any interruption sends only what still
//! differs.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io;
use std::ops::{Bound, Range};
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
pub(crate) use scan::{millis, mode, setup as scan_setup};
pub(crate) use serve::{inside, permissions, remove as remove_entry, set_mode, MOST_WRITES};
pub use serve::Session;

/// What a tree holds, by path: relative, `/`-separated, never absolute and
/// never through `..`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub entries: BTreeMap<String, Entry>,
    /// What this side could not list, by path, and why: each is left as
    /// each side has it, with all under it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub problems: BTreeMap<String, String>,
    /// Each path this side reached through a symlink, which it read
    /// through and never writes through (a Setup scan's).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub links: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    Dir,
    File {
        size: u64,
        sha256: String,
        /// Its permission bits, on a side that keeps them (Windows does
        /// not).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<u32>,
        /// When it last changed, in milliseconds since the epoch (a Setup
        /// scan's): the newer of two edits wins.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        modified: Option<u64>,
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
    /// The operator's agent setup on that computer, by the paths
    /// `setup_sync::ITEMS` lists (ADR 0048).
    Setup,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Request {
    /// Replied `scanned`, a page at a time: the entries and problems after
    /// `after`, in path order. A root that is not there yet holds nothing.
    Scan {
        root: Root,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skip: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<String>,
    },
    /// Start writing a file as `write`, a number the sender gives it, so
    /// its chunks can follow without waiting for the reply: its bytes go to
    /// a temporary file beside it until `finish` renames it into place.
    /// `mode` is the sender's permission bits, where it keeps them.
    Begin {
        write: u64,
        root: Root,
        path: String,
        size: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<u32>,
    },
    /// Base64, at most [`chunked::CHUNK_SIZE`] bytes, in order. Replied
    /// `written`.
    Chunk {
        write: u64,
        offset: u64,
        data: String,
    },
    /// Put the file in place if all of it arrived and it hashes to
    /// `sha256`; either way the write is over.
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
    /// The entry and all under it.
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
    /// A page of a scan, and whether more follow it.
    Scanned {
        manifest: Manifest,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        more: bool,
    },
    Written {
        offset: u64,
    },
    Bytes {
        data: String,
        size: u64,
    },
    Done,
    /// It was already so: nothing was done.
    Unchanged,
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

#[cfg(test)]
impl Asked {
    /// The request numbered `id` of an Exchange that numbers its own.
    pub(crate) fn numbered(id: u64) -> Self {
        Self { id, patience: PATIENCE }
    }

    pub(crate) fn id(self) -> u64 {
        self.id
    }
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
    /// The file as this side listed it: it is taken there only if what is
    /// read of it now hashes to `sha256`.
    Write {
        path: String,
        size: u64,
        sha256: String,
        mode: Option<u32>,
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
/// rewritten in place. What `skip` names, what either side could not list,
/// and each directory that holds one of those are left as each side has
/// them.
pub fn plan(source: &Manifest, dest: &Manifest, skip: &[String]) -> Vec<Step> {
    let held = Held(
        skip.iter()
            .chain(source.problems.keys())
            .chain(dest.problems.keys())
            .map(String::as_str)
            .collect(),
    );
    let mut removed = BTreeSet::new();
    for (path, there) in &dest.entries {
        if inside_any(path, &removed) || held.covers(path) {
            continue;
        }
        let same_kind = match (source.entries.get(path), there) {
            (Some(Entry::Dir), Entry::Dir) | (Some(Entry::File { .. }), Entry::File { .. }) => true,
            (Some(here @ Entry::Symlink { .. }), there) => here == there,
            _ => false,
        };
        let stays = same_kind
            || matches!(there, Entry::Unknown)
            || (matches!(there, Entry::Dir) && held.under(path));
        if !stays {
            removed.insert(path.as_str());
        }
    }
    let mut steps: Vec<Step> = removed.iter().map(|path| Step::Remove(path.to_string())).collect();
    for (path, here) in &source.entries {
        if held.covers(path) {
            continue;
        }
        let there = dest
            .entries
            .get(path)
            .filter(|_| !removed.contains(path.as_str()) && !inside_any(path, &removed));
        match (here, there) {
            (Entry::Dir, None) => steps.push(Step::MakeDir(path.clone())),
            (Entry::File { size, sha256, mode, .. }, None | Some(Entry::File { .. })) => {
                let same = matches!(
                    there,
                    Some(Entry::File { size: s, sha256: h, mode: m, .. })
                        if s == size && h == sha256 && !executable_differs(*mode, *m)
                );
                if !same {
                    steps.push(Step::Write {
                        path: path.clone(),
                        size: *size,
                        sha256: sha256.clone(),
                        mode: *mode,
                    });
                }
            }
            (Entry::Symlink { target }, None) => steps.push(Step::Link {
                path: path.clone(),
                target: target.clone(),
            }),
            _ => {}
        }
    }
    steps
}

/// The paths a plan leaves as each side has them.
struct Held<'a>(BTreeSet<&'a str>);

impl Held<'_> {
    /// Whether `path` is held or lies under one that is.
    fn covers(&self, path: &str) -> bool {
        self.0.contains(path) || inside_any(path, &self.0)
    }

    /// Whether a held path lies under `path`.
    fn under(&self, path: &str) -> bool {
        let dir = format!("{path}/");
        self.0.iter().any(|held| held.starts_with(&dir))
    }
}

/// The executable bits differ, and both sides keep them.
pub(crate) fn executable_differs(here: Option<u32>, there: Option<u32>) -> bool {
    matches!((here, there), (Some(here), Some(there)) if here & 0o111 != there & 0o111)
}

fn inside_any(path: &str, dirs: &BTreeSet<&str>) -> bool {
    path.match_indices('/').any(|(at, _)| dirs.contains(&path[..at]))
}

impl Manifest {
    /// The entries and problems after `after`, in path order, as many as
    /// take at most `budget` bytes of JSON (and at least one), and whether
    /// more follow them.
    fn page(&self, after: Option<&str>, budget: usize) -> (Manifest, bool) {
        let from = (after.map_or(Bound::Unbounded, Bound::Excluded), Bound::Unbounded);
        let mut entries = self.entries.range::<str, _>(from).peekable();
        let mut problems = self.problems.range::<str, _>(from).peekable();
        let mut page = Manifest::default();
        let mut used = 0;
        let more = loop {
            let entry_next = match (entries.peek(), problems.peek()) {
                (None, None) => break false,
                (Some((entry, _)), Some((problem, _))) => entry <= problem,
                (entry, _) => entry.is_some(),
            };
            let size = if entry_next {
                serde_json::to_vec(&entries.peek()).map_or(0, |json| json.len())
            } else {
                serde_json::to_vec(&problems.peek()).map_or(0, |json| json.len())
            };
            if used > 0 && used + size > budget {
                break true;
            }
            used += size;
            if entry_next {
                if let Some((path, entry)) = entries.next() {
                    page.entries.insert(path.clone(), entry.clone());
                }
            } else if let Some((path, why)) = problems.next() {
                page.problems.insert(path.clone(), why.clone());
            }
        };
        let last = page.entries.keys().chain(page.problems.keys()).max().cloned();
        let to = match (more, &last) {
            (true, Some(last)) => Bound::Included(last.as_str()),
            _ => Bound::Unbounded,
        };
        page.links = self.links.range::<str, _>((from.0, to)).cloned().collect();
        (page, more)
    }
}

/// What a [`send`] did, and what each side could not list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Sent {
    pub removed: usize,
    pub made: usize,
    pub written: usize,
    pub linked: usize,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<Problem>,
}

/// An entry a side could not list: it was left as each side has it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub path: String,
    pub side: Side,
    pub error: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// This computer, which sends.
    Here,
    /// The Server's computer, which receives.
    There,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Here => Side::There,
            Side::There => Side::Here,
        }
    }
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

/// Where a [`send`] reads the files its steps write: a directory's, or
/// the operator's setup as `setup_sync` spells it.
pub trait Source {
    fn open(&self, path: &str) -> io::Result<Box<dyn io::Read + '_>>;
}

impl<T: AsRef<Path> + ?Sized> Source for T {
    fn open(&self, path: &str) -> io::Result<Box<dyn io::Read + '_>> {
        Ok(Box::new(std::fs::File::open(local(self.as_ref(), path))?))
    }
}

/// Take each of `steps` to `dest` through `exchange`, the files read from
/// `source`, asking ahead of the replies. `progress` hears how many
/// bytes of all the steps' files the far side has. An error names the
/// entry it was for; what was asked after it may still be taken there,
/// since each step is whole on its own.
pub fn send(
    source: &(impl Source + ?Sized),
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
                sha256,
                mode,
            } => {
                let file = source.open(path).map_err(|error| named(step, Side::Here, error))?;
                let write = WRITES.fetch_add(1, Ordering::Relaxed) + 1;
                let finish = Request::Finish {
                    write,
                    sha256: sha256.clone(),
                };
                let sent = (|| {
                    let begin = Request::Begin {
                        write,
                        root,
                        path: path.clone(),
                        size: *size,
                        mode: *mode,
                    };
                    flight.ask(step, begin, Expect::Begun)?;
                    let mut pieces = chunked::Pieces::new(file, *size);
                    for piece in &mut pieces {
                        cancel.check()?;
                        let (piece, data) = piece.map_err(|error| named(step, Side::Here, error))?;
                        let chunk = Request::Chunk {
                            write,
                            offset: piece.start,
                            data,
                        };
                        flight.ask(step, chunk, Expect::Written(piece))?;
                    }
                    if pieces.sha256().ok().as_ref() != Some(sha256) {
                        return Err(named(step, Side::Here, changed()));
                    }
                    flight.ask(step, finish.clone(), Expect::Taken)
                })();
                if sent.is_err() {
                    // The far side lets go of a write only once it is
                    // finished, and puts in place only a whole file that
                    // hashes as it was listed.
                    let _ = flight.exchange.ask(finish);
                }
                sent?;
                continue;
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
    /// `done`, or `unchanged` when it was already so: the step taken.
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
        let asked = self.exchange.ask(request).map_err(|error| named(step, Side::There, error))?;
        self.ahead += bytes;
        self.waiting.push_back((asked, step, expect));
        Ok(())
    }

    fn hear(&mut self) -> io::Result<()> {
        let Some((asked, step, expect)) = self.waiting.pop_front() else {
            return Ok(());
        };
        let reply = self.exchange.answer(asked).map_err(|error| named(step, Side::There, error))?;
        match (expect, reply) {
            (Expect::Taken, Reply::Done) => self.sent.took(step),
            (Expect::Taken, Reply::Unchanged) | (Expect::Begun, Reply::Done) => {}
            (Expect::Written(piece), Reply::Written { offset }) if offset == piece.end => {
                let bytes = piece.end - piece.start;
                self.ahead -= bytes;
                self.received += bytes;
                (self.progress)(Progress {
                    sent: self.received,
                    total: self.total,
                });
            }
            _ => return Err(named(step, Side::There, unanswered())),
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
    let asked = exchange.ask(scan_request(dest, skip, None))?;
    let here = scan(source, skip)?;
    let there = pages(exchange, dest, skip, asked)?;
    let mut sent = send(source, &plan(&here, &there, skip), dest, exchange, progress, cancel)?;
    for (side, manifest) in [(Side::Here, here), (Side::There, there)] {
        sent.problems.extend(manifest.problems.into_iter().map(|(path, error)| Problem {
            path,
            side,
            error,
        }));
    }
    Ok(sent)
}

/// What the far side holds at `root`: its scan, every page of it.
pub fn scan_far(exchange: &mut dyn Exchange, root: &Root, skip: &[String]) -> io::Result<Manifest> {
    let asked = exchange.ask(scan_request(root, skip, None))?;
    pages(exchange, root, skip, asked)
}

pub(crate) fn scan_request(root: &Root, skip: &[String], after: Option<String>) -> Request {
    Request::Scan {
        root: root.clone(),
        skip: skip.to_vec(),
        after,
    }
}

/// The scan whose first page `asked` asked for, each later page asked in
/// turn after the last path the one before held.
pub(crate) fn pages(exchange: &mut dyn Exchange, root: &Root, skip: &[String], mut asked: Asked) -> io::Result<Manifest> {
    let mut whole = Manifest::default();
    loop {
        let Reply::Scanned { manifest, more } = exchange.answer(asked)? else {
            return Err(unanswered());
        };
        let last = manifest.entries.keys().chain(manifest.problems.keys()).max().cloned();
        whole.entries.extend(manifest.entries);
        whole.problems.extend(manifest.problems);
        whole.links.extend(manifest.links);
        match (more, last) {
            (false, _) => return Ok(whole),
            (true, Some(last)) => asked = exchange.ask(scan_request(root, skip, Some(last)))?,
            (true, None) => return Err(unanswered()),
        }
    }
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

/// Whether two directories of this computer are one, or one holds the
/// other, however each is spelled.
pub(crate) fn overlap(one: &Path, other: &Path) -> io::Result<bool> {
    let one = canonical(one).map_err(|error| at(one, error))?;
    let other = canonical(other).map_err(|error| at(other, error))?;
    Ok(holds(&one, &other) || holds(&other, &one))
}

/// Whether `inner` is `outer` or lies in it, however each is spelled.
pub(crate) fn within(inner: &Path, outer: &Path) -> io::Result<bool> {
    Ok(holds(&canonical(outer)?, &canonical(inner)?))
}

/// `path` as this computer names it: its deepest ancestor that is there
/// resolved, links followed and, on Windows, in its true case with no
/// `\\?\`, and the rest of it joined on.
fn canonical(path: &Path) -> io::Result<PathBuf> {
    let mut missing = Vec::new();
    let mut at = path;
    let found = loop {
        match dunce::canonicalize(at) {
            Ok(found) => break found,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let (Some(parent), Some(name)) = (at.parent(), at.file_name()) else {
                    return Err(error);
                };
                missing.push(name);
                at = parent;
            }
            Err(error) => return Err(error),
        }
    };
    Ok(missing.into_iter().rev().fold(found, |path, name| path.join(name)))
}

/// Whether `outer` is `inner` or holds it, compared without case where the
/// filesystem usually has none.
fn holds(outer: &Path, inner: &Path) -> bool {
    if cfg!(any(windows, target_os = "macos")) {
        let fold = |path: &Path| PathBuf::from(path.to_string_lossy().to_lowercase());
        fold(inner).starts_with(fold(outer))
    } else {
        inner.starts_with(outer)
    }
}

/// `path` as this computer spells it under `root`.
fn local(root: &Path, path: &str) -> PathBuf {
    path.split('/').fold(root.to_path_buf(), |at, part| at.join(part))
}

fn named(step: &Step, side: Side, error: io::Error) -> io::Error {
    Stuck::at(step.path(), side, error)
}

/// What cut a send or a sync short: `error`, met at `path` on `side`. It
/// reads as `<path>: <error>`.
#[derive(Debug)]
pub struct Stuck {
    pub path: String,
    pub side: Side,
    pub error: io::Error,
}

impl Stuck {
    pub(crate) fn at(path: &str, side: Side, error: io::Error) -> io::Error {
        let stuck = Stuck {
            path: path.to_string(),
            side,
            error,
        };
        io::Error::new(stuck.error.kind(), stuck)
    }
}

impl std::fmt::Display for Stuck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.error)
    }
}

impl std::error::Error for Stuck {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// `error`, saying it was at `path`.
pub(crate) fn at(path: &Path, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{}: {error}", path.display()))
}

fn changed() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "it changed after it was listed: send again",
    )
}

fn unanswered() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "The server did not answer the tree operation",
    )
}

/// The exchange itself failed, rather than the far side refusing a step.
pub(crate) fn line_down(error: &io::Error) -> bool {
    use io::ErrorKind::*;
    matches!(
        error.kind(),
        TimedOut | NotConnected | BrokenPipe | ConnectionReset | ConnectionAborted | UnexpectedEof
    )
}
