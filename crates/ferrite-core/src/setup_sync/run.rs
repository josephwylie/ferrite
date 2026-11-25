//! One sync of this computer's setup with one Machine's (ADR 0048, ADR
//! 0050): both sides scanned, [`reconcile`]d with the state they last
//! agreed on, and what changed moved each way. Each side is rescanned once
//! all of it is done, when anything was, and only then is the base
//! written, from what the two now hold alike: a sync cut short anywhere
//! leaves the base it began from, and the next one finishes it. What it
//! removes or writes over on either side goes to that side's trash, not
//! away. Both sides scan by the same [`TABLE`], or the Machine refuses the
//! scan and nothing moves, and a base is read only by the table that wrote
//! it. One sync of a home runs at a time, whichever computer drives it.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::lock::Held;
use super::reconcile::{agreed, reconcile, Action, Hold};
use super::store::{self, visible, Home, STAMP};
use super::{file_of, sign_in, Item, ITEMS, TABLE, UNREAD_WITHOUT_SIGN_INS};
use crate::chunked::{Cancellation, CHUNK_SIZE};
use crate::tree::{self, Asked, Entry, Exchange, Manifest, Problem, Reply, Request, Root, Session, Side, Step};

/// The state a Machine and this computer last agreed on, in the Machine's
/// own folder of this computer's settings.
pub const BASE: &str = "setup-sync.json";
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
    /// conflicts, trash and lock are kept in `settings_dir`.
    pub fn for_machine(home: Home, settings_dir: &Path, target: &str) -> Self {
        Self {
            home: home.with_settings_in(settings_dir),
            dir: crate::machine::dir(settings_dir, target),
            conflicts: settings_dir.join(CONFLICTS),
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
            false => UNREAD_WITHOUT_SIGN_INS.iter().map(|path| path.to_string()).collect(),
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
    /// Another sync of this home, to any Machine, or an install in it was
    /// under way: this one did nothing, and is asked again later.
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub replaced: Vec<Replaced>,
    /// What changed and was left as each side has it, by path.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub held: BTreeMap<String, Hold>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<Problem>,
    /// Where what was removed or written over here was moved.
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

/// A chosen key both sides held differently: the `winner`'s value is on
/// both now, and the other side's was `was`, kept in that side's trash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Replaced {
    pub path: String,
    pub winner: Side,
    pub was: String,
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
/// how many entries the sync moves or removes, once it knows. Both homes
/// are held from the first scan to the base, this computer's and the
/// Machine's, so one sync of either runs at a time, whichever computer
/// drives it: one that finds either held is busy, and wrote nothing.
pub fn run(
    job: &Job,
    there: &mut dyn Exchange,
    planned: &mut dyn FnMut(usize),
    cancel: &Cancellation,
) -> io::Result<Outcome> {
    let Some(held) = job.home.hold()? else {
        return Ok(Outcome::Busy);
    };
    fs::create_dir_all(&job.dir).map_err(|error| tree::at(&job.dir, error))?;
    let base = base(&job.dir);
    let unread = job.unread();
    let (mut here, mut far) = if job.only_if_changed_here {
        let mut here = job.home.scan(&unread);
        as_base(&mut here, &base, &unread);
        if reconcile(&base, &here, &base).is_empty() {
            return Ok(Outcome::Synced(Synced::default()));
        }
        if !hold_there(there)? {
            return Ok(Outcome::Busy);
        }
        (here, tree::scan_far(there, &Root::SETUP, &unread)?)
    } else {
        if !hold_there(there)? {
            return Ok(Outcome::Busy);
        }
        let asked = there.ask(tree::scan_request(&Root::SETUP, &unread, None))?;
        let here = job.home.scan(&unread);
        (here, tree::pages(there, &Root::SETUP, &unread, asked)?)
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
                tree::Source::open(&job.home, path)
                    .and_then(|mut file| file.read_to_end(&mut bytes))
                    .map_err(|error| named(path, Side::Here, error))?;
                bytes
            }
        };
        let loser = job.home.written(path, loser);
        if super::chosen(path).is_some() {
            let was = serde_json::from_slice::<serde_json::Value>(&loser)
                .map_or_else(|_| String::from_utf8_lossy(&loser).into_owned(), |was| was.to_string());
            synced.replaced.push(Replaced {
                path: path.clone(),
                winner: *winner,
                was,
            });
            continue;
        }
        let kept = match kept_already(&job.conflicts, path, &loser) {
            Some(kept) => kept,
            None => {
                let kept = visible(&job.conflicts.join(&stamp), path);
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
    let sent = tree::send(&job.home, &steps, &Root::SETUP, there, &mut |_| {}, cancel)?;
    let pulled = take_here(job, &held, &actions, &here, &far, there, cancel)?;
    synced.trashed = job.home.trashed();

    if !actions.is_empty() {
        here = job.home.scan(&unread);
        far = tree::scan_far(there, &Root::SETUP, &unread)?;
        settle(job, &base, &mut here, &mut far, &unread);
    }
    let agreed = agreed(&base, &here, &far);
    if agreed != base {
        store::write_over(&job.dir.join(BASE), &serde_json::to_vec(&Base { table: TABLE, agreed })?)?;
    }
    for (side, manifest) in [(Side::Here, here), (Side::There, far)] {
        let said = manifest
            .problems
            .into_iter()
            .filter(|(path, error)| !(sign_in(path) && error == store::MID_WRITE) && error != store::OWN);
        synced.problems.extend(said.map(|(path, error)| Problem { path, side, error }));
    }
    synced.problems.extend(sent.problems.into_iter().chain(pulled));
    Ok(Outcome::Synced(synced))
}

/// Hold the Machine's home for this sync: whether it was free. A Machine
/// from before the hold refuses it as unsupported, so the row says to
/// update Ferrite there.
fn hold_there(there: &mut dyn Exchange) -> io::Result<bool> {
    match there.call(Request::Hold { root: Root::SETUP })? {
        Reply::Done => Ok(true),
        Reply::Busy => Ok(false),
        _ => Err(unanswered()),
    }
}

/// What a base file holds: the state both sides last agreed on, and the
/// table that read them.
#[derive(Serialize, Deserialize)]
pub(super) struct Base {
    pub table: u32,
    pub agreed: Manifest,
}

/// The base, or none: a base that is not there, does not read, or that
/// another table wrote is a first sync, which removes nothing. Another
/// table's base is never read in part: it agreed by other rules, and a
/// removal planned from it could take what this table never synced.
pub(super) fn base(dir: &Path) -> Manifest {
    fs::read(dir.join(BASE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Base>(&bytes).ok())
        .filter(|base| base.table == TABLE)
        .map(|base| base.agreed)
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
        for path in far.entries.keys().filter(|path| super::chosen(path).is_none()) {
            if let Some(name) = path.split('/').find(|name| !tree::windows_name(name)) {
                held.entry(path.clone()).or_insert_with(|| format!("this computer cannot hold the name `{name}`"));
            }
        }
    }
    let mut seen: BTreeMap<String, &str> = here
        .entries
        .keys()
        .map(|path| (path.to_lowercase(), path.as_str()))
        .collect();
    for path in far.entries.keys().filter(|path| super::chosen(path).is_none()) {
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

/// `action`, with two sign-ins that differ settled as the operator chose.
fn chosen(action: Action, sign_ins: Option<Side>) -> Action {
    match (action, sign_ins) {
        (Action::Hold { path, why: Hold::SignIn }, Some(Side::Here)) => Action::Push(path),
        (Action::Hold { path, why: Hold::SignIn }, Some(Side::There)) => Action::Pull(path),
        (action, _) => action,
    }
}

/// The table's item `path` is, lies in, or is a key of: its deepest.
fn owner(path: &str) -> Option<&'static Item> {
    let path = file_of(path);
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
    for item in ITEMS {
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
/// then what is made, in path order, then each link, once what it points
/// at is there.
fn steps_there(actions: &[Action], here: &Manifest, far: &Manifest) -> Vec<Step> {
    let mut removed = Vec::new();
    let mut made = Vec::new();
    let mut linked = Vec::new();
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
            Entry::Symlink { target } => linked.push(Step::Link {
                path: path.clone(),
                target: target.clone(),
            }),
            Entry::Unknown => {}
        }
    }
    removed.extend(made);
    removed.extend(linked);
    removed
}

/// Do here what the actions do on this computer: what goes first, moved
/// to the trash, then each folder, then each file, read from the Machine
/// and asked ahead of the replies, then each link, all under `held`. A
/// file that does not come across whole is left as each side has it, and
/// said.
fn take_here(
    job: &Job,
    held: &Held,
    actions: &[Action],
    here: &Manifest,
    far: &Manifest,
    there: &mut dyn Exchange,
    cancel: &Cancellation,
) -> io::Result<Vec<Problem>> {
    let mut taken = Vec::new();
    for action in actions {
        cancel.check()?;
        let path = match action {
            Action::RemoveHere(path) => {
                job.home.remove(path).map_err(|error| named(path, Side::Here, error))?;
                continue;
            }
            Action::Pull(path) | Action::Conflict { path, winner: Side::There } => path,
            _ => continue,
        };
        let (Some(entry), mine) = (far.entries.get(path), here.entries.get(path)) else {
            continue;
        };
        if mine.is_some_and(|mine| !same_kind(entry, mine)) {
            job.home.remove(path).map_err(|error| named(path, Side::Here, error))?;
        }
        taken.push((path.as_str(), entry));
    }
    for (path, entry) in &taken {
        if matches!(entry, Entry::Dir) {
            job.home.make_dir(path).map_err(|error| named(path, Side::Here, error))?;
        }
    }
    let mut local = Session::held(job.home.clone(), held.clone());
    let mut pulls = Pulls {
        local: &mut local,
        there,
        waiting: VecDeque::new(),
        ahead: 0,
        open: BTreeMap::new(),
        left: Vec::new(),
    };
    for (write, (path, entry)) in taken.iter().copied().enumerate() {
        let Entry::File { size, sha256, mode, .. } = entry else {
            continue;
        };
        cancel.check()?;
        pulls.begin(write as u64, path, *size, sha256, *mode)?;
    }
    while !pulls.waiting.is_empty() {
        pulls.hear()?;
    }
    for (path, entry) in &taken {
        if let Entry::Symlink { target } = entry {
            job.home.link(path, target).map_err(|error| named(path, Side::Here, error))?;
        }
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
            root: Root::SETUP,
            path: path.to_string(),
            size,
            mode,
        };
        self.local.execute(begin).map_err(|error| named(path, Side::Here, error))?;
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
                root: Root::SETUP,
                path: path.to_string(),
                offset,
            };
            let asked = self.there.ask(read).map_err(|error| named(path, Side::There, error))?;
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
            reply => reply.map_err(|error| named(piece.path, Side::There, error))?,
        };
        let Reply::Bytes { data, .. } = reply else {
            return Err(named(piece.path, Side::There, unanswered()));
        };
        let Some(hash) = self.open.get_mut(&piece.write) else {
            return Ok(());
        };
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&data)
            .map_err(|_| named(piece.path, Side::There, unanswered()))?;
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
        self.local.execute(chunk).map_err(|error| named(piece.path, Side::Here, error))?;
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
        self.local.execute(finish).map(|_| ()).map_err(|error| named(path, Side::Here, error))
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
            root: Root::SETUP,
            path: path.to_string(),
            offset: bytes.len() as u64,
        };
        let Reply::Bytes { data, size } = there.call(read).map_err(|error| named(path, Side::There, error))? else {
            return Err(named(path, Side::There, unanswered()));
        };
        let chunk = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| named(path, Side::There, unanswered()))?;
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
        .map(|sync| visible(&sync.path(), path))
        .find(|kept| fs::read(kept).is_ok_and(|held| held == bytes))
}

fn same_kind(one: &Entry, other: &Entry) -> bool {
    std::mem::discriminant(one) == std::mem::discriminant(other)
}

fn named(path: &str, side: Side, error: io::Error) -> io::Error {
    tree::Stuck::at(path, side, error)
}

fn unanswered() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "no answer came for it")
}

fn changed_there() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "it changed while it was read: the next sync carries it")
}
