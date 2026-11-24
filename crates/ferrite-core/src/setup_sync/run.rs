//! One sync of this computer's setup with one Machine's (ADR 0048): both
//! sides scanned, [`reconcile`]d with the state they last agreed on, and
//! what changed moved each way. Each side is rescanned once all of it is
//! done, when anything was, and only then is the base written, from what
//! the two now hold alike: a sync cut short anywhere leaves the base it began from, and the
//! next one finishes it. What it removes on this computer goes to a trash
//! folder, not away.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::reconcile::{agreed, reconcile, seeded, Action, Hold};
use super::store::{self, local, Home};
use super::{sign_in, Item, Source, CLAUDE_SIGN_IN, ITEMS};
use crate::chunked::{Cancellation, CHUNK_SIZE};
use crate::tree::{self, Asked, Entry, Exchange, Manifest, Problem, Reply, Request, Root, Session, Side, Step};

/// The state a Machine and this computer last agreed on, in the Machine's
/// own folder of this computer's settings.
pub const BASE: &str = "setup-sync.json";
/// Held while a sync of that Machine is under way, by any process.
const LOCK: &str = "setup-sync.lock";
/// Where each conflict's losing copy is kept, in Ferrite's settings folder.
pub const CONFLICTS: &str = "sync-conflicts";
/// Where what a sync removes on this computer is moved, in Ferrite's
/// settings folder, a folder for each sync.
pub const TRASH: &str = "sync-trash";
/// How long a sync's trash folder is kept.
const KEPT: Duration = Duration::from_secs(14 * 24 * 60 * 60);
/// How far the files pulled here are asked ahead of the replies.
const AHEAD: usize = 64;
const AHEAD_BYTES: u64 = 4 << 20;
/// How a sync's folder of conflicts or trash is named: its time.
const STAMP: &str = "%Y%m%d-%H%M%S-%3f";

pub struct Job {
    /// This computer's setup.
    pub home: Home,
    /// The Machine's folder of this computer's settings, where its base is.
    pub dir: PathBuf,
    /// Where the losing copy of each conflict is kept, under the time of
    /// the sync.
    pub conflicts: PathBuf,
    /// Where what the sync removes here is moved, under the time of the
    /// sync.
    pub trash: PathBuf,
    /// Remove whatever the plan removes: the operator said so.
    pub allow_removals: bool,
    /// Leave the Machine be when nothing here changed since the base: a
    /// change this computer saw that turned out to be none.
    pub only_if_changed_here: bool,
    /// Read Claude's sign-in. macOS keeps it in a Keychain item that is
    /// Claude Code's, whose every read may ask the operator, so a sync
    /// that a change here started leaves it as each side has it.
    pub read_sign_ins: bool,
    /// Whose sign-in each side keeps where two differ with none agreed
    /// before; `None` leaves them as they are.
    pub sign_ins: Option<Side>,
    /// What names this computer's files can have.
    pub names: Names,
}

impl Job {
    /// A whole sync of `home` with the Machine `target` names, whose base,
    /// conflicts and trash are kept in `settings_dir`.
    pub fn for_machine(home: Home, settings_dir: &Path, target: &str) -> Self {
        Self {
            home,
            dir: crate::machine::dir(settings_dir, target),
            conflicts: settings_dir.join(CONFLICTS),
            trash: settings_dir.join(TRASH),
            allow_removals: false,
            only_if_changed_here: false,
            read_sign_ins: true,
            sign_ins: None,
            names: Names::HERE,
        }
    }

    /// The table's paths this sync does not read.
    fn unread(&self) -> Vec<String> {
        match self.read_sign_ins {
            true => Vec::new(),
            false => vec![CLAUDE_SIGN_IN.to_string()],
        }
    }
}

/// What a file system lets a file be named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Names {
    /// Any name a Machine has, each case its own (Linux).
    Any,
    /// Two names that differ only in case are one file (macOS).
    OneCase,
    /// And no `<>:"/\|?*`, no device name, no name ending in a dot or a
    /// space (Windows).
    Windows,
}

impl Names {
    pub const HERE: Names = if cfg!(windows) {
        Names::Windows
    } else if cfg!(target_os = "macos") {
        Names::OneCase
    } else {
        Names::Any
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Synced(Synced),
    /// Nothing was done: the plan removed too much.
    Stopped(Valve),
    /// Another sync of this Machine was under way: this one did nothing.
    Busy,
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
    /// Where what was removed here was moved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trashed: Option<PathBuf>,
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

/// The plan would remove `removing` of the `holding` files of `item` on
/// `side`: more than half of them, or all an item the other side lost
/// whole (`vanished`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Valve {
    pub side: Side,
    pub item: String,
    pub removing: usize,
    pub holding: usize,
    pub vanished: bool,
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
    match lock.try_lock() {
        Ok(()) => {}
        Err(fs::TryLockError::WouldBlock) => return Ok(Outcome::Busy),
        Err(fs::TryLockError::Error(error)) => return Err(error),
    }
    prune(&job.trash);
    let base = base(&job.dir);
    let unread = job.unread();
    let (mut here, mut far) = if job.only_if_changed_here {
        let mut here = job.home.scan(&unread);
        as_base(&mut here, &base, &unread);
        if reconcile(&base, &here, &base).iter().all(|action| seeded(action.path())) {
            return Ok(Outcome::Synced(Synced::default()));
        }
        (here, tree::scan_far(there, &Root::Setup, &unread)?)
    } else {
        let asked = there.ask(tree::scan_request(&Root::Setup, &unread, None))?;
        let here = job.home.scan(&unread);
        (here, tree::pages(there, &Root::Setup, &unread, asked)?)
    };
    settle(job, &base, &mut here, &mut far, &unread);
    let actions: Vec<Action> = reconcile(&base, &here, &far)
        .into_iter()
        .map(|action| chosen(action, job.sign_ins))
        .collect();
    if !job.allow_removals && !base.entries.is_empty() {
        if let Some(valve) = valve(&actions, &base, &here, &far) {
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

    let stamp = crate::clock::now().format(STAMP).to_string();
    for action in &actions {
        // A sign-in's loser is never kept: the newer wins, as a refreshed
        // token must, and the older is of no use.
        let Action::Conflict { path, winner } = action else {
            continue;
        };
        if sign_in(path) {
            continue;
        }
        cancel.check()?;
        let loser = match winner {
            Side::Here => fetch(there, path)?,
            Side::There => {
                let mut bytes = Vec::new();
                tree::Source::open(&job.home, path)?.read_to_end(&mut bytes)?;
                bytes
            }
        };
        let loser = job.home.written(path, loser);
        let kept = match kept_already(&job.conflicts, path, &loser) {
            Some(kept) => kept,
            None => {
                let kept = local(&job.conflicts.join(&stamp), path);
                store::private_dirs(kept.parent().unwrap_or(&job.conflicts))?;
                store::write_over(&kept, &loser)?;
                kept
            }
        };
        synced.conflicts.push(Kept {
            path: path.clone(),
            winner: *winner,
            kept,
        });
    }

    let steps = steps_there(&actions, &here, &far);
    let sent = tree::send(&job.home, &steps, &Root::Setup, there, &mut |_| {}, cancel)?;
    let trash = job.trash.join(&stamp);
    let pulled = take_here(job, &actions, &here, &far, there, &trash, cancel)?;
    synced.trashed = trash.exists().then_some(trash);

    if !actions.is_empty() {
        here = job.home.scan(&unread);
        far = tree::scan_far(there, &Root::Setup, &unread)?;
        settle(job, &base, &mut here, &mut far, &unread);
    }
    let agreed = agreed(&base, &here, &far);
    if agreed != base {
        store::write_over(&job.dir.join(BASE), &serde_json::to_vec(&agreed)?)?;
    }
    for (side, manifest) in [(Side::Here, here), (Side::There, far)] {
        synced.problems.extend(manifest.problems.into_iter().map(|(path, error)| Problem { path, side, error }));
    }
    synced.problems.extend(sent.problems.into_iter().chain(pulled));
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

/// Both sides as reconcile reads them: what this sync did not read is as
/// the base has it on both, and what this computer cannot name as the
/// Machine does is held.
fn settle(job: &Job, base: &Manifest, here: &mut Manifest, far: &mut Manifest, unread: &[String]) {
    as_base(here, base, unread);
    as_base(far, base, unread);
    unholdable(far, here, job.names);
}

/// `manifest` with each of `paths` as `base` has it.
fn as_base(manifest: &mut Manifest, base: &Manifest, paths: &[String]) {
    for path in paths {
        match base.entries.get(path) {
            Some(entry) => manifest.entries.insert(path.clone(), entry.clone()),
            None => manifest.entries.remove(path),
        };
        manifest.problems.remove(path);
    }
}

/// Hold each path of the Machine's that this computer cannot name as it
/// does: a name its file system refuses, or one that differs only in case
/// from another there or here, where case is not told apart. The rest
/// syncs.
pub(crate) fn unholdable(far: &mut Manifest, here: &Manifest, names: Names) {
    if names == Names::Any {
        return;
    }
    let mut held: BTreeMap<String, String> = BTreeMap::new();
    if names == Names::Windows {
        for path in far.entries.keys() {
            if let Some(name) = path.split('/').find(|name| !windows_name(name)) {
                held.entry(path.clone()).or_insert_with(|| format!("this computer cannot hold the name `{name}`"));
            }
        }
    }
    let mut seen: BTreeMap<String, &str> = here
        .entries
        .keys()
        .map(|path| (path.to_lowercase(), path.as_str()))
        .collect();
    for path in far.entries.keys() {
        match seen.get(&path.to_lowercase()) {
            Some(other) if *other != path => {
                let why = format!("it differs only in case from `{other}`, and this computer tells no case apart");
                held.entry(path.clone()).or_insert(why);
            }
            _ => {
                seen.insert(path.to_lowercase(), path);
            }
        }
    }
    let outermost: BTreeSet<String> = held
        .keys()
        .filter(|path| !held.keys().any(|other| path.starts_with(&format!("{other}/"))))
        .cloned()
        .collect();
    for path in outermost {
        if let Some(why) = held.remove(&path) {
            far.problems.insert(path, why);
        }
    }
}

/// Whether Windows can hold a file named `name`.
fn windows_name(name: &str) -> bool {
    const DEVICES: &[&str] = &["con", "prn", "aux", "nul"];
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_lowercase();
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix)
            .is_some_and(|digit| digit.len() == 1 && digit.chars().all(|c| c.is_ascii_digit() && c != '0'))
    };
    let device = DEVICES.contains(&stem.as_str()) || numbered("com") || numbered("lpt");
    let refused = name.chars().any(|c| c < ' ' || "<>:\"\\|?*".contains(c));
    !(device || refused || name.ends_with('.') || name.ends_with(' '))
}

/// `action`, with two sign-ins that differ settled as the operator chose.
fn chosen(action: Action, sign_ins: Option<Side>) -> Action {
    match (action, sign_ins) {
        (Action::Hold { path, why: Hold::SignIn }, Some(Side::Here)) => Action::Push(path),
        (Action::Hold { path, why: Hold::SignIn }, Some(Side::There)) => Action::Pull(path),
        (action, _) => action,
    }
}

/// The table's item `path` is, or lies in: its deepest.
fn owner(path: &str) -> Option<&'static Item> {
    ITEMS
        .iter()
        .filter(|item| path == item.path || path.strip_prefix(item.path).is_some_and(|rest| rest.starts_with('/')))
        .max_by_key(|item| item.path.len())
}

/// The first item the plan would remove more than half the files of, on
/// either side, or remove any of on one side where the other lost all of
/// it since the base.
fn valve(actions: &[Action], base: &Manifest, here: &Manifest, there: &Manifest) -> Option<Valve> {
    let counts = |manifest: &Manifest| {
        let mut files: BTreeMap<&str, usize> = BTreeMap::new();
        for (path, entry) in &manifest.entries {
            if let Some(item) = owner(path) {
                *files.entry(item.path).or_default() += usize::from(matches!(entry, Entry::File { .. }));
            }
        }
        files
    };
    let in_base = counts(base);
    let sides = [(Side::Here, here, counts(here)), (Side::There, there, counts(there))];
    let held = |side: Side| &sides[usize::from(side == Side::There)].2;
    for item in ITEMS.iter().filter(|item| item.source != Source::Seed) {
        for (side, manifest, holding) in &sides {
            let removing: usize = actions
                .iter()
                .filter_map(|action| match (action, side) {
                    (Action::RemoveHere(path), Side::Here) | (Action::RemoveThere(path), Side::There) => Some(path),
                    _ => None,
                })
                .flat_map(|path| {
                    let under = format!("{path}/");
                    manifest
                        .entries
                        .iter()
                        .filter(move |(at, entry)| (*at == path || at.starts_with(&under)) && matches!(entry, Entry::File { .. }))
                })
                .filter(|(at, _)| owner(at).is_some_and(|owner| owner.path == item.path))
                .count();
            if removing == 0 {
                continue;
            }
            let holding = holding.get(item.path).copied().unwrap_or(0);
            let vanished = in_base.contains_key(item.path) && !held(side.other()).contains_key(item.path);
            if vanished || 2 * removing > holding {
                return Some(Valve {
                    side: *side,
                    item: item.path.to_string(),
                    removing,
                    holding,
                    vanished,
                });
            }
        }
    }
    None
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

/// Do here what the actions do on this computer: what goes first, moved
/// to `trash`, then each folder, then each file, read from the Machine and
/// asked ahead of the replies. A file that does not come across whole is
/// left as each side has it, and said.
fn take_here(
    job: &Job,
    actions: &[Action],
    here: &Manifest,
    far: &Manifest,
    there: &mut dyn Exchange,
    trash: &Path,
    cancel: &Cancellation,
) -> io::Result<Vec<Problem>> {
    let mut taken = Vec::new();
    for action in actions {
        cancel.check()?;
        let path = match action {
            Action::RemoveHere(path) => {
                job.home.trash(path, trash).map_err(|error| named(path, error))?;
                continue;
            }
            Action::Pull(path) | Action::Conflict { path, winner: Side::There } => path,
            _ => continue,
        };
        let (Some(entry), mine) = (far.entries.get(path), here.entries.get(path)) else {
            continue;
        };
        if mine.is_some_and(|mine| !same_kind(entry, mine)) {
            job.home.trash(path, trash).map_err(|error| named(path, error))?;
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
        open: BTreeMap::new(),
        left: Vec::new(),
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
    Ok(pulls.left)
}

/// Files read from the Machine into this computer's setup, a chunk at a
/// time, asked ahead of the replies, and written here a few at a time, as
/// many as a Session writes at once.
struct Pulls<'a> {
    local: &'a mut Session,
    there: &'a mut dyn Exchange,
    waiting: VecDeque<Piece<'a>>,
    /// Bytes of files in what `waiting` asked.
    ahead: u64,
    /// Each file begun here and not yet finished, by its write: what it
    /// hashes to so far.
    open: BTreeMap<u64, Sha256>,
    /// The Machine's files that did not come across whole.
    left: Vec<Problem>,
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
        while self.open.len() >= tree::MOST_WRITES && !self.waiting.is_empty() {
            self.hear()?;
        }
        let begin = Request::Begin {
            write,
            root: Root::Setup,
            path: path.to_string(),
            size,
            mode,
        };
        self.local.execute(begin).map_err(|error| named(path, error))?;
        self.open.insert(write, Sha256::new());
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
        use base64::Engine as _;
        let Some(piece) = self.waiting.pop_front() else {
            return Ok(());
        };
        self.ahead -= piece.size;
        let reply = match self.there.answer(piece.asked) {
            Err(error) if !tree::line_down(&error) => {
                self.leave(piece.write, piece.path, error);
                return Ok(());
            }
            reply => reply.map_err(|error| named(piece.path, error))?,
        };
        let Reply::Bytes { data, .. } = reply else {
            return Err(named(piece.path, unanswered()));
        };
        let Some(hash) = self.open.get_mut(&piece.write) else {
            return Ok(());
        };
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&data)
            .map_err(|_| named(piece.path, unanswered()))?;
        if bytes.len() as u64 != piece.size {
            self.leave(piece.write, piece.path, changed_there());
            return Ok(());
        }
        hash.update(&bytes);
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
        let read = self.open.get(&write).map(|hash| crate::chunked::hex(&hash.clone().finalize()));
        if read.as_deref() != Some(sha256) {
            self.leave(write, path, changed_there());
            return Ok(());
        }
        self.open.remove(&write);
        let finish = Request::Finish {
            write,
            sha256: sha256.to_string(),
        };
        self.local.execute(finish).map(|_| ()).map_err(|error| named(path, error))
    }

    /// The Machine's file at `path` did not come across whole: it is left
    /// as each side has it, and its write here let go, once.
    fn leave(&mut self, write: u64, path: &str, error: io::Error) {
        if self.open.remove(&write).is_some() {
            let _ = self.local.execute(Request::Finish { write, sha256: String::new() });
            self.left.push(Problem {
                path: path.to_string(),
                side: Side::There,
                error: error.to_string(),
            });
        }
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

/// A copy of `path` holding `bytes` that an earlier sync kept already: one
/// cut short before it agreed keeps the same loser again.
fn kept_already(conflicts: &Path, path: &str, bytes: &[u8]) -> Option<PathBuf> {
    fs::read_dir(conflicts)
        .ok()?
        .flatten()
        .map(|sync| local(&sync.path(), path))
        .find(|kept| fs::read(kept).is_ok_and(|held| held == bytes))
}

/// Remove each sync's trash folder older than [`KEPT`].
fn prune(trash: &Path) {
    let Ok(folders) = fs::read_dir(trash) else {
        return;
    };
    let now = crate::clock::now().naive_local();
    for folder in folders.flatten() {
        let name = folder.file_name();
        let when = name.to_str().and_then(|name| chrono::NaiveDateTime::parse_from_str(name, STAMP).ok());
        if when.is_some_and(|when| (now - when).to_std().is_ok_and(|age| age > KEPT)) {
            let _ = fs::remove_dir_all(folder.path());
        }
    }
}

fn same_kind(one: &Entry, other: &Entry) -> bool {
    std::mem::discriminant(one) == std::mem::discriminant(other)
}

fn named(path: &str, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{path}: {error}"))
}

fn unanswered() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "the Machine did not answer the setup sync")
}

fn changed_there() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "it changed while it was read: the next sync carries it")
}
