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

mod links;
mod scan;
mod serve;
#[cfg(test)]
mod tests;

pub use scan::{scan, scan_watched};
pub(crate) use links::link as make_link;
pub(crate) use scan::{millis, mode, setup as scan_setup, Text};
pub(crate) use serve::{inside, permissions, set_mode, MOST_WRITES};
pub use serve::{Session, HOLD_QUIET, LAPSED};

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
        /// The SHA-256 of its text with each CR before an LF left out,
        /// when it is UTF-8 (a Setup scan's): two copies that differ only
        /// in their line ends hold the same.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
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
    /// Setup sync's first table, Ferrite 0.4.0's (ADR 0048): refused, so a
    /// Ferrite that syncs by it never moves what the tables name otherwise.
    #[serde(rename = "setup")]
    FirstSetup,
    /// The operator's agent setup on that computer, by the paths table
    /// `table` lists (`setup_sync::ITEMS`, ADR 0050). A Server that syncs
    /// by another table refuses it.
    #[serde(rename = "setup_table")]
    Setup { table: u32 },
    #[serde(other)]
    Unknown,
}

impl Root {
    /// The setup as this Ferrite's table names it.
    pub const SETUP: Root = Root::Setup {
        table: crate::setup_sync::TABLE,
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Request {
    /// Hold the Setup root's home for this connection's sync (ADR 0050):
    /// no other sync or install writes it, in any process, until the
    /// connection goes or asks nothing for [`HOLD_QUIET`]. Replied
    /// `done`, or `busy` while another holds it. Every other op on the
    /// Setup root is refused on a connection that does not hold it.
    Hold {
        root: Root,
    },
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
    /// Another sync or install holds the root: nothing was done.
    Busy,
    /// This side did not make the entry, and why: the sender leaves it as
    /// this side has it and goes on.
    Unmade(Refusal),
    #[serde(other)]
    Unknown,
}

/// Why a far side did not make an entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "why", rename_all = "snake_case")]
pub enum Refusal {
    /// A link that leads out of the tree.
    LeadsOut,
    /// A link that leads to nothing that side has.
    LeadsNowhere,
    /// A name that side's file system cannot hold.
    Name,
    /// That side's file system would not make it.
    Failed { error: String },
    #[serde(other)]
    Unknown,
}

impl Refusal {
    pub(crate) fn failed(error: io::Error) -> Self {
        Refusal::Failed {
            error: error.to_string(),
        }
    }
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

    /// The entry it makes, as this side lists it.
    fn entry(&self) -> Option<Entry> {
        match self {
            Step::Remove(_) => None,
            Step::MakeDir(_) => Some(Entry::Dir),
            Step::Write { size, sha256, mode, .. } => Some(Entry::File {
                size: *size,
                sha256: sha256.clone(),
                mode: *mode,
                modified: None,
                text: None,
            }),
            Step::Link { target, .. } => Some(Entry::Symlink { target: target.clone() }),
        }
    }
}

/// What makes `dest` equal `source`: first what must go, the outermost
/// entry of each removal only, then what is made, in path order so a
/// directory is made before what it holds, and each link last, after what
/// it leads to, a link that leads through another after that one: a side
/// that makes a link by what it leads to (Windows) finds it there. A file
/// of the same kind is rewritten in place, and a link is there when the
/// far side holds it as it can ([`Manifest::holds_link`]). What `skip`
/// names, what either side could not list, and each directory that holds
/// one of those are left as each side has them.
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
            (Some(Entry::Symlink { target }), there) => source.holds_link(path, target, there),
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
    let mut links = Vec::new();
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
            (Entry::Symlink { target }, None) => {
                let through = source.lead(path, target).map_or(0, |(_, through)| through);
                links.push((
                    through,
                    Step::Link {
                        path: path.clone(),
                        target: target.clone(),
                    },
                ));
            }
            _ => {}
        }
    }
    links.sort_by_key(|(through, _)| *through);
    steps.extend(links.into_iter().map(|(_, link)| link));
    steps
}

/// The most links one target is followed through, as a system's own
/// lookup gives up.
const MOST_LINKS: usize = 40;

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
    /// Where the link at `path`, to `target`, leads in this tree: the path
    /// it ends at, each link on the way followed as a lookup follows it,
    /// and how many it followed. None when it leads out of the tree, or
    /// through more links than a lookup takes.
    pub(crate) fn lead<'a>(&'a self, path: &'a str, target: &'a str) -> Option<(String, usize)> {
        if target.starts_with('/') {
            return None;
        }
        let mut at: Vec<&str> = path.split('/').collect();
        at.pop();
        let mut ahead: VecDeque<&str> = target.split('/').collect();
        let mut through = 0;
        while let Some(part) = ahead.pop_front() {
            match part {
                "" | "." => {}
                ".." => {
                    at.pop()?;
                }
                name => {
                    at.push(name);
                    if let Some(Entry::Symlink { target }) = self.entries.get(&at.join("/")) {
                        through += 1;
                        if through > MOST_LINKS || target.starts_with('/') {
                            return None;
                        }
                        at.pop();
                        for part in target.split('/').rev() {
                            ahead.push_front(part);
                        }
                    }
                }
            }
        }
        Some((at.join("/"), through))
    }

    /// Whether `there`, the far side's entry at `path`, holds this side's
    /// link there to `target`: a link that leads the same way, however its
    /// target is spelled with `.` and `/`, or a copy of the file it leads
    /// to here, as a side that makes no file link holds one (Windows).
    pub(crate) fn holds_link(&self, path: &str, target: &str, there: &Entry) -> bool {
        fn spelled(target: &str) -> (bool, Vec<&str>) {
            let parts = target.split('/').filter(|part| !matches!(*part, "" | ".")).collect();
            (target.starts_with('/'), parts)
        }
        match there {
            Entry::Symlink { target: theirs } => spelled(target) == spelled(theirs),
            Entry::File { size, sha256, .. } => matches!(
                self.lead(path, target).and_then(|(to, _)| self.entries.get(&to)),
                Some(Entry::File { size: ours, sha256: hash, .. }) if ours == size && hash == sha256
            ),
            _ => false,
        }
    }

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

/// What a [`send`] did, what each side could not list or read, and what
/// the far side did not make.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Sent {
    pub removed: usize,
    pub made: usize,
    pub written: usize,
    pub linked: usize,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<Problem>,
    /// Each outermost: nothing under a folder left unmade is listed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unmade: Vec<Unmade>,
}

/// An entry the far side did not make, left as it has it: what it is
/// here, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Unmade {
    pub path: String,
    pub entry: Entry,
    #[serde(flatten)]
    pub why: Refusal,
}

/// A send that stopped before its last step: why, and what it had taken
/// there by then.
#[derive(Debug)]
pub struct Stopped {
    pub error: io::Error,
    pub sent: Sent,
}

impl Stopped {
    /// Whether nothing reached the far side: every reply was heard, and
    /// none took a step.
    pub fn took_nothing(&self) -> bool {
        let Sent {
            removed,
            made,
            written,
            linked,
            ..
        } = self.sent;
        !line_down(&self.error) && removed + made + written + linked == 0
    }
}

impl From<io::Error> for Stopped {
    fn from(error: io::Error) -> Self {
        Self {
            error,
            sent: Sent::default(),
        }
    }
}

impl From<Stopped> for io::Error {
    fn from(stopped: Stopped) -> Self {
        stopped.error
    }
}

impl std::fmt::Display for Stopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

/// An entry a side could not list, or a file this side could not read as
/// it listed it: it was left as each side has it.
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
    /// What this send did, and what each side, as listed here and there,
    /// could not list.
    pub fn with_problems(mut self, here: Manifest, there: Manifest) -> Sent {
        for (side, manifest) in [(Side::Here, here), (Side::There, there)] {
            self.problems.extend(manifest.problems.into_iter().map(|(path, error)| Problem {
                path,
                side,
                error,
            }));
        }
        self
    }

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

/// Where a [`send`] reads the files its steps write: a directory's, one
/// that gives each file as it is to land there, or the operator's setup as
/// `setup_sync` spells it.
pub trait Source {
    fn open(&self, path: &str) -> io::Result<Box<dyn io::Read + '_>>;
}

impl<T: AsRef<Path> + ?Sized> Source for T {
    fn open(&self, path: &str) -> io::Result<Box<dyn io::Read + '_>> {
        Ok(Box::new(std::fs::File::open(local(self.as_ref(), path))?))
    }
}

/// How far a [`send`] has come: the bytes of all its steps' files the far
/// side has, and the file the last of them were of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sending<'a> {
    pub progress: Progress,
    pub file: Option<&'a str>,
}

/// Take each of `steps` to `dest` through `exchange`, the files read from
/// `source`, asking ahead of the replies. `progress` hears how far it has
/// come ([`Sending`]). A file that cannot be
/// read here as its step lists it (gone, held by another program, changed
/// since it was listed) is left as the far side has it, a problem of this
/// side, and the send goes on. So is an entry the far side says it did not
/// make (`unmade`), with all under it, which is not asked for. An error
/// names the entry it was for, and comes with what was taken by then; what
/// was asked after it may still be taken there, since each step is whole
/// on its own.
pub fn send(
    source: &(impl Source + ?Sized),
    steps: &[Step],
    dest: &Root,
    exchange: &mut dyn Exchange,
    progress: &mut dyn FnMut(Sending),
    cancel: &Cancellation,
) -> Result<Sent, Stopped> {
    let total = steps
        .iter()
        .map(|step| match step {
            Step::Write { size, .. } => *size,
            _ => 0,
        })
        .sum();
    progress(Sending {
        progress: Progress { sent: 0, total },
        file: None,
    });
    let mut flight = Flight {
        exchange,
        progress,
        waiting: VecDeque::new(),
        ahead: 0,
        received: 0,
        total,
        sent: Sent::default(),
        unmade: None,
        unmade_dirs: BTreeSet::new(),
    };
    for step in steps {
        if let Err(error) = cancel.check() {
            return Err(flight.stop(error));
        }
        if inside_any(step.path(), &flight.unmade_dirs) {
            continue;
        }
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
                let file = match source.open(path) {
                    Ok(file) => file,
                    Err(error) => {
                        flight.left_here(path, error);
                        continue;
                    }
                };
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
                    flight.ask(step, begin, Expect::Begun).map_err(Halt::Stop)?;
                    let mut pieces = chunked::Pieces::new(file, *size);
                    for piece in &mut pieces {
                        cancel.check().map_err(Halt::Stop)?;
                        if flight.is_unmade(step) {
                            return Err(Halt::Unmade);
                        }
                        let (piece, data) = piece.map_err(Halt::Unread)?;
                        let chunk = Request::Chunk {
                            write,
                            offset: piece.start,
                            data,
                        };
                        flight.ask(step, chunk, Expect::Written(piece)).map_err(Halt::Stop)?;
                    }
                    if pieces.sha256().ok().as_ref() != Some(sha256) {
                        return Err(Halt::Unread(changed()));
                    }
                    flight.ask(step, finish.clone(), Expect::Taken).map_err(Halt::Stop)
                })();
                if sent.is_err() {
                    // The far side lets go of a write only once it is
                    // finished, and puts in place only a whole file that
                    // hashes as it was listed.
                    let _ = flight.exchange.ask(finish);
                }
                match sent {
                    Ok(()) | Err(Halt::Unmade) => {}
                    Err(Halt::Unread(error)) => flight.left_here(path, error),
                    Err(Halt::Stop(error)) => return Err(flight.stop(error)),
                }
                continue;
            }
        };
        if let Err(error) = flight.ask(step, request, Expect::Taken) {
            return Err(flight.stop(error));
        }
    }
    flight.land()
}

/// Why a file's write ended early: it could not be read here as listed,
/// or the far side did not make it, and the send goes on; or the send
/// itself must stop.
enum Halt {
    Unread(io::Error),
    Unmade,
    Stop(io::Error),
}

/// What a send has asked and not yet heard the replies to, oldest first.
struct Flight<'a> {
    exchange: &'a mut dyn Exchange,
    progress: &'a mut dyn FnMut(Sending),
    waiting: VecDeque<(Asked, &'a Step, Expect)>,
    /// Bytes of files in what `waiting` asked.
    ahead: u64,
    /// Bytes of files the far side has.
    received: u64,
    total: u64,
    sent: Sent,
    /// The step the far side last said it did not make: the rest asked for
    /// it is refused there, and passed over here.
    unmade: Option<&'a Step>,
    /// Each folder the far side did not make.
    unmade_dirs: BTreeSet<&'a str>,
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
        let bytes = match &expect {
            Expect::Written(piece) => piece.end - piece.start,
            _ => 0,
        };
        self.ahead -= bytes;
        let reply = self.exchange.answer(asked).map_err(|error| named(step, Side::There, error));
        if self.is_unmade(step) {
            return reply.map(drop).or_else(|error| if line_down(&error) { Err(error) } else { Ok(()) });
        }
        match (expect, reply?) {
            (Expect::Taken, Reply::Done) => self.sent.took(step),
            (Expect::Taken, Reply::Unchanged) | (Expect::Begun, Reply::Done) => {}
            (Expect::Taken | Expect::Begun, Reply::Unmade(why)) => {
                let entry = step.entry().ok_or_else(|| named(step, Side::There, unanswered()))?;
                if let Step::MakeDir(path) = step {
                    self.unmade_dirs.insert(path);
                }
                self.unmade = Some(step);
                self.sent.unmade.push(Unmade {
                    path: step.path().to_string(),
                    entry,
                    why,
                });
            }
            (Expect::Written(piece), Reply::Written { offset }) if offset == piece.end => {
                self.received += bytes;
                (self.progress)(Sending {
                    progress: Progress {
                        sent: self.received,
                        total: self.total,
                    },
                    file: Some(step.path()),
                });
            }
            _ => return Err(named(step, Side::There, unanswered())),
        }
        Ok(())
    }

    /// Whether the far side said it did not make `step`'s entry.
    fn is_unmade(&self, step: &Step) -> bool {
        self.unmade.is_some_and(|unmade| std::ptr::eq(unmade, step))
    }

    /// What the send did, once every reply is heard.
    fn land(mut self) -> Result<Sent, Stopped> {
        while !self.waiting.is_empty() {
            if let Err(error) = self.hear() {
                return Err(self.stop(error));
            }
        }
        Ok(self.taken())
    }

    /// What was taken there, and each outermost entry that was not: one
    /// asked before the folder it is in was refused fails with it.
    fn taken(&mut self) -> Sent {
        let dirs = &self.unmade_dirs;
        self.sent.unmade.retain(|unmade| !inside_any(&unmade.path, dirs));
        std::mem::take(&mut self.sent)
    }

    /// The file at `path` is left as the far side has it.
    fn left_here(&mut self, path: &str, error: io::Error) {
        self.sent.problems.push(Problem {
            path: path.to_string(),
            side: Side::Here,
            error: error.to_string(),
        });
    }

    /// The send stops for `error`, with what was taken there once the
    /// replies already on their way are heard, a refusal among them passed
    /// over. A line that is down is not waited on again.
    fn stop(&mut self, error: io::Error) -> Stopped {
        if !line_down(&error) {
            while !self.waiting.is_empty() {
                if self.hear().is_err_and(|error| line_down(&error)) {
                    break;
                }
            }
        }
        Stopped {
            error,
            sent: self.taken(),
        }
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
    progress: &mut dyn FnMut(Sending),
    cancel: &Cancellation,
) -> Result<Sent, Stopped> {
    let asked = exchange.ask(scan_request(dest, skip, None))?;
    let here = scan(source, skip)?;
    let there = pages(exchange, dest, skip, asked)?;
    let sent = send(source, &plan(&here, &there, skip), dest, exchange, progress, cancel)?;
    Ok(sent.with_problems(here, there))
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
pub fn overlap(one: &Path, other: &Path) -> io::Result<bool> {
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

/// The target that leads from the link at `path` to `to`, both in the
/// tree: up from the link's folder to where the two part, then down.
fn relative(path: &str, to: &[String]) -> String {
    let mut from: Vec<&str> = path.split('/').collect();
    from.pop();
    let fold = |name: &str| {
        if cfg!(any(windows, target_os = "macos")) {
            name.to_lowercase()
        } else {
            name.to_string()
        }
    };
    let common = from.iter().zip(to).take_while(|(here, there)| fold(here) == fold(there)).count();
    let parts: Vec<&str> = std::iter::repeat_n("..", from.len() - common)
        .chain(to[common..].iter().map(String::as_str))
        .collect();
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}

/// Whether Windows can hold a file named `name`: none with `<>:"/\|?*` or
/// a control character, none that is a device name (`aux`, `com1.txt`),
/// none ending in a dot or a space.
pub(crate) fn windows_name(name: &str) -> bool {
    const DEVICES: &[&str] = &["con", "prn", "aux", "nul"];
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_lowercase();
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix)
            .is_some_and(|digit| digit.len() == 1 && digit.chars().all(|c| c.is_ascii_digit() && c != '0'))
    };
    let device = DEVICES.contains(&stem.as_str()) || numbered("com") || numbered("lpt");
    let refused = name.chars().any(|c| c < ' ' || "<>:\"/\\|?*".contains(c));
    !(device || refused || name.ends_with('.') || name.ends_with(' '))
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
        "it changed after it was listed",
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
