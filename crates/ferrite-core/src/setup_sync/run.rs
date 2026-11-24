//! One sync of this computer's setup with one Machine's (ADR 0048): both
//! sides scanned, [`reconcile`]d with the state they last agreed on, and
//! what changed moved each way. Each side is rescanned once all of it is
//! done, and only then is the base written, from what the two now hold
//! alike: a sync cut short anywhere leaves the base it began from, and the
//! next one finishes it.

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::reconcile::{agreed, reconcile, Action, Hold};
use super::store::{self, Home};
use crate::chunked::{Cancellation, CHUNK_SIZE};
use crate::tree::{self, Asked, Entry, Exchange, Manifest, Problem, Reply, Request, Root, Session, Side, Step};

/// The state a Machine and this computer last agreed on, in the Machine's
/// own folder of this computer's settings.
pub const BASE: &str = "setup-sync.json";
/// Held while a sync of that Machine is under way, by any process.
const LOCK: &str = "setup-sync.lock";
/// Where each conflict's losing copy is kept, in Ferrite's settings folder.
pub const CONFLICTS: &str = "sync-conflicts";
/// How far the files pulled here are asked ahead of the replies.
const AHEAD: usize = 64;
const AHEAD_BYTES: u64 = 4 << 20;

pub struct Job {
    /// This computer's setup.
    pub home: Home,
    /// The Machine's folder of this computer's settings, where its base is.
    pub dir: PathBuf,
    /// Where the losing copy of each conflict is kept, under the time of
    /// the sync.
    pub conflicts: PathBuf,
    /// Remove whatever the plan removes: the operator said so.
    pub allow_removals: bool,
    /// Leave the Machine be when nothing here changed since the base: a
    /// change this computer saw that turned out to be none.
    pub only_if_changed_here: bool,
}

impl Job {
    /// A sync of `home` with the Machine `target` names, whose base and
    /// conflicts are kept in `settings_dir`.
    pub fn for_machine(home: Home, settings_dir: &Path, target: &str) -> Self {
        Self {
            home,
            dir: crate::machine::dir(settings_dir, target),
            conflicts: settings_dir.join(CONFLICTS),
            allow_removals: false,
            only_if_changed_here: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Synced(Synced),
    /// Nothing was done: the plan removed too much.
    Stopped(Valve),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Synced {
    pub pushed: usize,
    pub pulled: usize,
    pub removed_here: usize,
    pub removed_there: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<Kept>,
    /// What changed and was left as each side has it, by path.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub held: BTreeMap<String, Hold>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<Problem>,
}

impl Synced {
    /// How many entries it moved or removed.
    pub fn changed(&self) -> usize {
        self.pushed + self.pulled + self.removed_here + self.removed_there
    }
}

/// A file both sides changed: the `winner`'s is on both now, and the
/// other's is kept at `kept`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Kept {
    pub path: String,
    pub winner: Side,
    pub kept: PathBuf,
}

/// The plan would remove `removing` of the `holding` files on `side`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Valve {
    pub side: Side,
    pub removing: usize,
    pub holding: usize,
}

/// Sync `job`'s setup with the Machine `there` reaches. `planned` hears
/// how many entries the sync moves or removes, once it knows.
pub fn run(
    job: &Job,
    there: &mut dyn Exchange,
    planned: &mut dyn FnMut(usize),
    cancel: &Cancellation,
) -> io::Result<Outcome> {
    fs::create_dir_all(&job.dir).map_err(|error| tree::at(&job.dir, error))?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(job.dir.join(LOCK))?;
    lock.try_lock().map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "this Machine's setup is syncing already"))?;
    let base = base(&job.dir);
    let (here, far) = if job.only_if_changed_here {
        let here = job.home.scan();
        if reconcile(&base, &here, &base).is_empty() {
            return Ok(Outcome::Synced(Synced::default()));
        }
        (here, tree::scan_far(there, &Root::Setup, &[])?)
    } else {
        let asked = there.ask(tree::scan_request(&Root::Setup, &[], None))?;
        let here = job.home.scan();
        (here, tree::pages(there, &Root::Setup, &[], asked)?)
    };
    let actions = reconcile(&base, &here, &far);
    if !job.allow_removals && !base.entries.is_empty() {
        if let Some(valve) = valve(&actions, &here, &far) {
            return Ok(Outcome::Stopped(valve));
        }
    }
    let mut synced = Synced::default();
    for action in &actions {
        match action {
            Action::Push(_) | Action::Conflict { winner: Side::Here, .. } => synced.pushed += 1,
            Action::Pull(_) | Action::Conflict { winner: Side::There, .. } => synced.pulled += 1,
            Action::RemoveHere(_) => synced.removed_here += 1,
            Action::RemoveThere(_) => synced.removed_there += 1,
            Action::Hold { path, why } => {
                synced.held.insert(path.clone(), *why);
            }
        }
    }
    planned(synced.changed());

    let stamp = crate::clock::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    for action in &actions {
        if let Action::Conflict { path, winner } = action {
            cancel.check()?;
            let kept = local(&job.conflicts.join(&stamp), path);
            let loser = match winner {
                Side::Here => fetch(there, path)?,
                Side::There => {
                    let mut bytes = Vec::new();
                    tree::Source::open(&job.home, path)?.read_to_end(&mut bytes)?;
                    bytes
                }
            };
            keep(&kept, &job.home.written(path, loser))?;
            synced.conflicts.push(Kept {
                path: path.clone(),
                winner: *winner,
                kept,
            });
        }
    }

    let steps = steps_there(&actions, &here, &far);
    tree::send(&job.home, &steps, &Root::Setup, there, &mut |_| {}, cancel)?;
    take_here(job, &actions, &here, &far, there, cancel)?;

    let here = job.home.scan();
    let far = tree::scan_far(there, &Root::Setup, &[])?;
    store::write_over(&job.dir.join(BASE), &serde_json::to_vec(&agreed(&base, &here, &far))?)?;
    for (side, manifest) in [(Side::Here, here), (Side::There, far)] {
        synced.problems.extend(manifest.problems.into_iter().map(|(path, error)| Problem { path, side, error }));
    }
    Ok(Outcome::Synced(synced))
}

/// The base, or none: a base that is not there or does not read is a
/// first sync, which removes nothing.
fn base(dir: &Path) -> Manifest {
    fs::read(dir.join(BASE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// The first side the plan would remove more than half the files of.
fn valve(actions: &[Action], here: &Manifest, there: &Manifest) -> Option<Valve> {
    [(Side::Here, here), (Side::There, there)].into_iter().find_map(|(side, manifest)| {
        let files = |under: Option<&str>| {
            manifest
                .entries
                .iter()
                .filter(|(path, entry)| {
                    matches!(entry, Entry::File { .. })
                        && under.is_none_or(|at| *path == at || path.starts_with(&format!("{at}/")))
                })
                .count()
        };
        let removing: usize = actions
            .iter()
            .filter_map(|action| match (action, side) {
                (Action::RemoveHere(path), Side::Here) | (Action::RemoveThere(path), Side::There) => Some(files(Some(path))),
                _ => None,
            })
            .sum();
        let holding = files(None);
        (2 * removing > holding).then_some(Valve { side, removing, holding })
    })
}

/// What the actions do on the Machine, as a send's steps: what goes first,
/// then what is made, in path order.
fn steps_there(actions: &[Action], here: &Manifest, far: &Manifest) -> Vec<Step> {
    let mut removed = Vec::new();
    let mut made = Vec::new();
    for action in actions {
        let path = match action {
            Action::RemoveThere(path) => {
                removed.push(Step::Remove(path.clone()));
                continue;
            }
            Action::Push(path) | Action::Conflict { path, winner: Side::Here } => path,
            _ => continue,
        };
        let (Some(entry), there) = (here.entries.get(path), far.entries.get(path)) else {
            continue;
        };
        if there.is_some_and(|there| !same_kind(entry, there)) {
            removed.push(Step::Remove(path.clone()));
        }
        match entry {
            Entry::Dir => made.push(Step::MakeDir(path.clone())),
            Entry::File { size, sha256, mode, .. } => made.push(Step::Write {
                path: path.clone(),
                size: *size,
                sha256: sha256.clone(),
                mode: *mode,
            }),
            _ => {}
        }
    }
    removed.extend(made);
    removed
}

/// Do here what the actions do on this computer: what goes first, then
/// each folder, then each file, read from the Machine and asked ahead of
/// the replies.
fn take_here(
    job: &Job,
    actions: &[Action],
    here: &Manifest,
    far: &Manifest,
    there: &mut dyn Exchange,
    cancel: &Cancellation,
) -> io::Result<()> {
    let mut taken = Vec::new();
    for action in actions {
        cancel.check()?;
        let path = match action {
            Action::RemoveHere(path) => {
                job.home.remove(path).map_err(|error| named(path, error))?;
                continue;
            }
            Action::Pull(path) | Action::Conflict { path, winner: Side::There } => path,
            _ => continue,
        };
        let (Some(entry), mine) = (far.entries.get(path), here.entries.get(path)) else {
            continue;
        };
        if mine.is_some_and(|mine| !same_kind(entry, mine)) {
            job.home.remove(path).map_err(|error| named(path, error))?;
        }
        taken.push((path.as_str(), entry));
    }
    for (path, entry) in &taken {
        if matches!(entry, Entry::Dir) {
            job.home.make_dir(path).map_err(|error| named(path, error))?;
        }
    }
    let mut local = Session::with_home(job.home.clone());
    let mut pulls = Pulls {
        local: &mut local,
        there,
        waiting: VecDeque::new(),
        ahead: 0,
    };
    for (write, (path, entry)) in taken.into_iter().enumerate() {
        let Entry::File { size, sha256, mode, .. } = entry else {
            continue;
        };
        cancel.check()?;
        pulls.begin(write as u64, path, *size, sha256, *mode)?;
    }
    while !pulls.waiting.is_empty() {
        pulls.hear()?;
    }
    Ok(())
}

/// Files read from the Machine into this computer's setup, a chunk at a
/// time, asked ahead of the replies.
struct Pulls<'a> {
    local: &'a mut Session,
    there: &'a mut dyn Exchange,
    waiting: VecDeque<Piece<'a>>,
    /// Bytes of files in what `waiting` asked.
    ahead: u64,
}

struct Piece<'a> {
    asked: Asked,
    path: &'a str,
    write: u64,
    offset: u64,
    size: u64,
    /// The file's hash, on its last piece.
    last: Option<&'a str>,
}

impl<'a> Pulls<'a> {
    fn begin(&mut self, write: u64, path: &'a str, size: u64, sha256: &'a str, mode: Option<u32>) -> io::Result<()> {
        let begin = Request::Begin {
            write,
            root: Root::Setup,
            path: path.to_string(),
            size,
            mode,
        };
        self.local.execute(begin).map_err(|error| named(path, error))?;
        if size == 0 {
            return self.finish(write, path, sha256);
        }
        let mut offset = 0;
        while offset < size {
            let piece = (size - offset).min(CHUNK_SIZE as u64);
            while self.waiting.len() >= AHEAD || (!self.waiting.is_empty() && self.ahead + piece > AHEAD_BYTES) {
                self.hear()?;
            }
            let read = Request::Read {
                root: Root::Setup,
                path: path.to_string(),
                offset,
            };
            let asked = self.there.ask(read).map_err(|error| named(path, error))?;
            self.ahead += piece;
            offset += piece;
            self.waiting.push_back(Piece {
                asked,
                path,
                write,
                offset: offset - piece,
                size: piece,
                last: (offset == size).then_some(sha256),
            });
        }
        Ok(())
    }

    fn hear(&mut self) -> io::Result<()> {
        let Some(piece) = self.waiting.pop_front() else {
            return Ok(());
        };
        self.ahead -= piece.size;
        let reply = self.there.answer(piece.asked).map_err(|error| named(piece.path, error))?;
        let Reply::Bytes { data, .. } = reply else {
            return Err(named(piece.path, unanswered()));
        };
        let chunk = Request::Chunk {
            write: piece.write,
            offset: piece.offset,
            data,
        };
        self.local.execute(chunk).map_err(|error| named(piece.path, error))?;
        match piece.last {
            Some(sha256) => self.finish(piece.write, piece.path, sha256),
            None => Ok(()),
        }
    }

    fn finish(&mut self, write: u64, path: &str, sha256: &str) -> io::Result<()> {
        let finish = Request::Finish {
            write,
            sha256: sha256.to_string(),
        };
        self.local.execute(finish).map(|_| ()).map_err(|error| named(path, error))
    }
}

/// All of `path` on the Machine, as it is carried.
fn fetch(there: &mut dyn Exchange, path: &str) -> io::Result<Vec<u8>> {
    use base64::Engine as _;
    let mut bytes = Vec::new();
    loop {
        let read = Request::Read {
            root: Root::Setup,
            path: path.to_string(),
            offset: bytes.len() as u64,
        };
        let Reply::Bytes { data, size } = there.call(read).map_err(|error| named(path, error))? else {
            return Err(named(path, unanswered()));
        };
        let chunk = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| named(path, unanswered()))?;
        let done = chunk.is_empty();
        bytes.extend(chunk);
        if bytes.len() as u64 >= size || done {
            return Ok(bytes);
        }
    }
}

/// A losing copy, owner-only in owner-only folders: it may be a sign-in.
fn keep(at: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = at.parent().unwrap_or(Path::new("."));
    for made in dir.ancestors().collect::<Vec<_>>().into_iter().rev() {
        if !made.exists() {
            fs::create_dir(made).map_err(|error| tree::at(made, error))?;
            tree::set_mode(made, 0o700)?;
        }
    }
    store::write_over(at, bytes)
}

fn same_kind(one: &Entry, other: &Entry) -> bool {
    std::mem::discriminant(one) == std::mem::discriminant(other)
}

/// `path`, a setup path, under `root` as this computer spells it.
fn local(root: &Path, path: &str) -> PathBuf {
    path.split('/').fold(root.to_path_buf(), |at, part| at.join(part))
}

fn named(path: &str, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{path}: {error}"))
}

fn unanswered() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "the Machine did not answer the setup sync")
}
