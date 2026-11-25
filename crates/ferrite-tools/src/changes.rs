//! Changes (Claude's, `changes-split.png`, `changes-more-menu.png`): the
//! checkout's diff as one scroll of files: the Panel's Changes Surface (ADR
//! 0039).
//!
//! - **Its toolbar** (T3's diff toolbar, 28 tall under the Panel's head):
//!   the scope as a dropdown at the left — `All changes` (against the base
//!   branch), `Uncommitted changes` (the default) or `Compare against ›` a
//!   branch — and, against a branch, `Comparing <head> against <base>`, the
//!   base a dropdown of its own. At the right, 14px tools: Refresh,
//!   Collapse or Expand all, Wrap, Whitespace, Hide test files, then `⋯`.
//!   T3's Stacked/Split toggle and File tree wait for a split diff and a
//!   tree of the changed files, which the diff body does not have yet.
//! - **Each file:** `▾ name dir`, its `+N` (and `−N`), a circle that marks it
//!   reviewed (and folds it away), `↗` to open it in Files, then its hunks
//!   as the panel draws them (`panel::hunks`).
//! - **`⋯`:** Collapse all files, Expand all files, Word wrap, Hide
//!   whitespace changes, Refresh.
//!
//! The diff is the Review core's (`ferrite_reviewr::ferrite::review_against`),
//! read off the UI thread whenever the pane opens, the scope changes or
//! Refresh is pressed.
//!
//! Every diff lives in one view of its own (ADR 0041), `ChangesView`: a
//! Thread's Changes and each pull request's Code, which the root draws
//! wherever it shows through `pane`. What it draws of the window (the
//! branch names, the `⋯` Menu button) comes with `ChangesInputs`; what it
//! asks of the window (a read, a stage, a file opened, a menu) it says as a
//! `ChangesEvent`. Git and the forge are the window's, so the window asks
//! them and hands the answers to the view, which keeps each read's number
//! and drops an answer once the window's `Source` says the diff is read
//! from another checkout or machine.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use ferrite_core::changes::Scope as ReadScope;
use ferrite_core::machine::Bridge;
use ferrite_core::pull_request::{PrSource, PrTarget};
use ferrite_core::ThreadKey;
use ferrite_pane::composer::{Composer, Role};
use ferrite_pane::hunks::{hunks_element, shown, Layout, Line, Shown, Sign};
use ferrite_ui::answer::answered;
use ferrite_ui::components::{IconState, MenuDoor};
use ferrite_ui::source::Source;
use ferrite_ui::theme::{self, paint};
use ferrite_ui::{icons, menu};
use gpui::prelude::*;
use gpui::{
    div, list, px, rgb, AnyElement, App, ClickEvent, Context, Div, Entity, EventEmitter, ListAlignment, ListOffset,
    ListState, Pixels, Point, SharedString,
};

/// What the Changes diff is against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The base branch's merge base, to the worktree.
    All,
    /// `HEAD` to the worktree.
    Uncommitted,
    /// A branch picked by name, in `All changes`' place.
    Against(String),
}

impl Scope {
    fn label(&self) -> String {
        match self {
            Scope::All => "All changes".into(),
            Scope::Uncommitted => "Uncommitted changes".into(),
            Scope::Against(branch) => format!("Compared with {branch}"),
        }
    }

    fn read(&self) -> ReadScope {
        match self {
            Scope::All => ReadScope::All,
            Scope::Uncommitted => ReadScope::Uncommitted,
            Scope::Against(branch) => ReadScope::Against(branch.clone()),
        }
    }
}

/// One of a Thread's diffs, each with its own scope, folds and filter: the
/// Changes Surface's, in the scope the operator picks, and a pull request's
/// Code view, keyed by the requested PR and read from its published revisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Diff {
    Changes(ThreadKey),
    PullRequest(PrTarget),
}

impl Diff {
    /// The machine whose checkout or forge the diff is read from.
    pub fn machine(self) -> ferrite_core::MachineId {
        match self {
            Diff::Changes(thread) => thread.machine(),
            Diff::PullRequest(target) => target.source.machine(),
        }
    }

    pub fn local_thread(self) -> Option<ThreadKey> {
        match self {
            Diff::Changes(thread) => Some(thread),
            Diff::PullRequest(_) => None,
        }
    }

    /// The Thread whose diff this is: its own, or a pull request read in
    /// its checkout. `None` for a Project's pull request.
    pub fn thread(self) -> Option<ThreadKey> {
        match self {
            Diff::Changes(thread) => Some(thread),
            Diff::PullRequest(target) => match target.source {
                PrSource::Thread(thread) => Some(thread),
                PrSource::Project(_) => None,
            },
        }
    }

    fn scope(self) -> Scope {
        match self {
            Diff::Changes(_) => Scope::Uncommitted,
            Diff::PullRequest(_) => Scope::All,
        }
    }
}

/// What a Changes menu row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    All,
    Uncommitted,
    /// The branch at this place in the listed branches.
    Compare(usize),
    CollapseAll,
    ExpandAll,
    ToggleWrap,
    ToggleWhitespace,
    ToggleTests,
    Refresh,
}

/// A parent row's submenu: the branches to compare against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sub;

/// A row of the scope dropdown: a verb, or `Compare against ›`'s submenu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Verb(Verb),
    Sub(Sub),
}

/// The toolbar's dropdowns, each a menu the window opens at the press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dropdown {
    /// The scope: All, Uncommitted, Compare against.
    Scope,
    /// `Comparing <head> against <base>`'s base: the branches.
    Base,
}

/// How far past the list's viewport files are laid out.
const OVERDRAW: f32 = 400.0;

/// How many branches `Compare against ›` lists.
const BRANCHES_LISTED: usize = 20;

struct File {
    path: String,
    added: u32,
    removed: u32,
    hunks: Shown,
    /// `hunks` without its whitespace-only changes, worked out off the UI
    /// thread with the diff.
    hunks_without_whitespace: Shown,
    /// The file's change is in git's index (`git add`).
    staged: bool,
    /// What its hunks say, so a mark made on other content is not this one's.
    revision: u64,
}

impl File {
    fn of(sides: &ferrite_reviewr::ferrite::FileSides, staged: bool) -> Self {
        let hunks = shown(Some(sides));
        let hunks_without_whitespace = match &hunks {
            Shown::Hunks(hunks) => Shown::Hunks(without_whitespace(hunks.clone())),
            other => other.clone(),
        };
        Self {
            path: sides.file.path.clone(),
            added: sides.file.additions,
            removed: sides.file.deletions,
            revision: revision_of(&hunks),
            hunks,
            hunks_without_whitespace,
            staged,
        }
    }
}

fn revision_of(hunks: &Shown) -> u64 {
    let mut hasher = DefaultHasher::new();
    hunks.hash(&mut hasher);
    hasher.finish()
}

enum Load {
    Reading,
    Read(Vec<File>),
    Failed(SharedString),
}

pub struct Changes {
    scope: Scope,
    load: Load,
    /// Files marked reviewed (folded, their circle filled), each with the
    /// revision of the hunks that were reviewed.
    reviewed: HashMap<String, u64>,
    folded: HashSet<String>,
    wrap: bool,
    hide_whitespace: bool,
    /// Test files left out of the list (`is_test_path`).
    hide_tests: bool,
    /// The list of files, one item each: only the files in view are laid
    /// out.
    scroll: ListState,
    /// Where in the read's files each item of the list is: the files the
    /// filter and Hide test files leave.
    listed: Vec<usize>,
    /// The branches `Compare against ›` lists, as of the last read.
    branches: Vec<String>,
    /// Bumped by every read, so a late answer for an old scope is dropped.
    generation: u64,
    stage_generation: u64,
    /// The filter field over the files: only names it matches are listed.
    filter: Entity<Composer>,
    /// The last stage or unstage git refused, until the next one replaces it
    /// or Refresh clears it.
    stage_failure: Option<StageFailure>,
    refreshing: bool,
    refresh_failure: Option<SharedString>,
}

/// A stage or unstage git refused: the file it was for, and git's word.
struct StageFailure {
    path: String,
    stage: bool,
    message: SharedString,
}

impl Changes {
    /// A mark made on content other than what `files` now holds goes, and its
    /// file opens again so the new hunks show.
    fn forget_stale_reviews(&mut self, files: &[File]) {
        let current: HashMap<&str, u64> = files
            .iter()
            .map(|file| (file.path.as_str(), file.revision))
            .collect();
        let stale: Vec<String> = self
            .reviewed
            .iter()
            .filter(|(path, revision)| current.get(path.as_str()) != Some(revision))
            .map(|(path, _)| path.clone())
            .collect();
        for path in stale {
            self.reviewed.remove(&path);
            self.folded.remove(&path);
        }
    }

    /// List the files the filter and Hide test files leave, every one
    /// measured again on the next frame (a fold, the wrap or the read may
    /// have changed its height), at the same place in the list. Until a
    /// read lands the list stands as it was.
    fn relist(&mut self, cx: &App) {
        let Load::Read(files) = &self.load else {
            return;
        };
        let wanted = self.filter.read(cx).text().trim().to_lowercase();
        let listed: Vec<usize> = files
            .iter()
            .enumerate()
            .filter(|(_, file)| wanted.is_empty() || file.path.to_lowercase().contains(&wanted))
            .filter(|(_, file)| !self.hide_tests || !is_test_path(&file.path))
            .map(|(at, _)| at)
            .collect();
        if listed.len() == self.scroll.item_count() {
            self.scroll.remeasure_items(0..listed.len());
        } else {
            let top = self.scroll.logical_scroll_top();
            self.scroll.reset(listed.len());
            if top.item_ix < listed.len() {
                self.scroll.scroll_to(top);
            }
        }
        self.listed = listed;
    }

    fn new(scope: Scope, filter: Entity<Composer>) -> Self {
        Self {
            scope,
            load: Load::Reading,
            reviewed: HashMap::new(),
            folded: HashSet::new(),
            wrap: true,
            hide_whitespace: false,
            hide_tests: false,
            scroll: ListState::new(0, ListAlignment::Top, px(OVERDRAW)),
            listed: Vec::new(),
            branches: Vec::new(),
            generation: 0,
            stage_generation: 0,
            filter,
            stage_failure: None,
            refreshing: false,
            refresh_failure: None,
        }
    }
}

/// Whether `path` reads as a test file (T3's Hide test files): under a
/// `test`/`tests`/`__tests__`/`spec` directory, or named as one
/// (`*_test.*`, `test_*`, `*.test.*`, `*.spec.*`, `tests.rs`).
pub fn is_test_path(path: &str) -> bool {
    let mut parts = path.split('/').collect::<Vec<_>>();
    let name = parts.pop().unwrap_or_default();
    if parts
        .iter()
        .any(|dir| matches!(*dir, "test" | "tests" | "__tests__" | "spec"))
    {
        return true;
    }
    let stem = name.split('.').next().unwrap_or(name);
    stem.ends_with("_test")
        || stem.starts_with("test_")
        || stem == "tests"
        || name.contains(".test.")
        || name.contains(".spec.")
}

/// The most removed-by-added line pairs one changed block is matched over.
const MATCHED_CELLS: usize = 1_000_000;

/// A line with every whitespace character taken out, which is how
/// `git diff -w` reads two lines as the same.
fn bare(line: &Line) -> String {
    line.text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// `hunks` without the changes that are only whitespace: a changed line
/// blank after trimming goes, a removed line and an added one that differ
/// only in whitespace become one unchanged line, and a hunk left with no
/// change goes with it.
pub fn without_whitespace(hunks: Vec<Vec<Line>>) -> Vec<Vec<Line>> {
    hunks
        .into_iter()
        .map(|hunk| {
            let mut out = Vec::with_capacity(hunk.len());
            let mut lines = hunk.into_iter().peekable();
            while let Some(line) = lines.next() {
                if line.sign == Sign::Context {
                    out.push(line);
                    continue;
                }
                let mut block = vec![line];
                while let Some(next) = lines.next_if(|next| next.sign != Sign::Context) {
                    block.push(next);
                }
                out.extend(unchanged_in_block(block));
            }
            out
        })
        .filter(|hunk| hunk.iter().any(|line| line.sign != Sign::Context))
        .collect()
}

/// One run of changed lines, less its blank lines. The removed and added
/// lines are matched in order (a longest common subsequence, whitespace
/// ignored), each match shown as one unchanged line, so a swap of two lines
/// stays a change and the lines keep their file order. A block too big to
/// match cheaply keeps its changes as they are.
fn unchanged_in_block(block: Vec<Line>) -> Vec<Line> {
    let (removed, added): (Vec<Line>, Vec<Line>) = block
        .into_iter()
        .filter(|line| !line.text.trim().is_empty())
        .partition(|line| line.sign == Sign::Removed);
    if removed.len().saturating_mul(added.len()) > MATCHED_CELLS {
        let mut out = removed;
        out.extend(added);
        return out;
    }
    let removed_keys: Vec<String> = removed.iter().map(bare).collect();
    let added_keys: Vec<String> = added.iter().map(bare).collect();
    let (rows, columns) = (removed.len(), added.len());
    let mut common = vec![vec![0usize; columns + 1]; rows + 1];
    for i in (0..rows).rev() {
        for j in (0..columns).rev() {
            common[i][j] = if removed_keys[i] == added_keys[j] {
                common[i + 1][j + 1] + 1
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let (mut removed, mut added) = (removed.into_iter(), added.into_iter());
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < rows && j < columns {
        if removed_keys[i] == added_keys[j] {
            removed.next();
            let line = added.next().expect("j < columns");
            out.push(Line {
                sign: Sign::Context,
                ..line
            });
            i += 1;
            j += 1;
        } else if common[i + 1][j] >= common[i][j + 1] {
            out.extend(removed.next());
            i += 1;
        } else {
            out.extend(added.next());
            j += 1;
        }
    }
    out.extend(removed);
    out.extend(added);
    out
}

/// Where a diff is read: the checkout, and for a Thread's own diff the
/// machine it is on. An answer read elsewhere is no longer this diff's.
pub struct Origin {
    root: PathBuf,
    machine: Option<Arc<Bridge>>,
}

impl Origin {
    /// The checkout at `root`, read over `machine`'s connection (`None`
    /// for a read that names none, as a pull request's).
    pub fn new(root: PathBuf, machine: Option<Arc<Bridge>>) -> Self {
        Self { root, machine }
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }
}

impl PartialEq for Origin {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
            && match (&self.machine, &other.machine) {
                (None, None) => true,
                (Some(current), Some(requested)) => Arc::ptr_eq(current, requested),
                _ => false,
            }
    }
}

/// What the Changes draw from the window: the branch names `Comparing`
/// shows, and the `⋯` Menu button's state.
pub struct ChangesInputs {
    pub head: Option<SharedString>,
    pub default_branch: Option<SharedString>,
    /// The `⋯` button's door, for the diff drawn.
    pub more: MenuDoor,
}

/// A diff drawn as a view of its own, for a view of another crate to hold
/// as its child: the PR reader's Code. Which diff it shows, and what it
/// draws of the window with it, it reads through its `Source`; the diff
/// itself is the Changes' (`pane`).
pub struct ChangesPane {
    changes: Entity<ChangesView>,
    shown: Source<(Diff, ChangesInputs)>,
}

impl ChangesPane {
    pub fn new(changes: Entity<ChangesView>, shown: Source<(Diff, ChangesInputs)>) -> Self {
        Self { changes, shown }
    }
}

impl Render for ChangesPane {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.shown.read(&(), cx) {
            Some((diff, inputs)) => self
                .changes
                .update(cx, |changes, cx| changes.pane(diff, inputs, cx)),
            None => div(),
        }
    }
}

/// What the Changes ask of the window.
#[derive(Clone, Debug, PartialEq)]
pub enum ChangesEvent {
    /// Read `diff` again in its scope. `keep_shown`: the last read stands
    /// until the new one lands.
    Read { diff: Diff, keep_shown: bool },
    /// A file's stage button: its change into git's index, or out of it.
    Stage { diff: Diff, path: String, stage: bool },
    /// A file's `↗`: it opens in Files.
    Open { thread: ThreadKey, path: String },
    /// The scope or the base dropdown, at the press.
    Menu { dropdown: Dropdown, diff: Diff, at: Point<Pixels> },
    /// A press on the `⋯` button.
    More { diff: Diff },
}


/// Every diff the window has read, a Thread's Changes and each pull
/// request's Code, each with its own scope, folds and filter.
pub struct ChangesView {
    /// Where the window reads each diff now, so an answer read elsewhere
    /// is dropped.
    origin: Source<Origin, Diff>,
    diffs: HashMap<Diff, Changes>,
}

impl EventEmitter<ChangesEvent> for ChangesView {}

impl ChangesView {
    pub fn new(origin: Source<Origin, Diff>) -> Self {
        Self {
            origin,
            diffs: HashMap::new(),
        }
    }

    /// Whether `diff` has been made.
    pub fn holds(&self, diff: Diff) -> bool {
        self.diffs.contains_key(&diff)
    }

    /// Whether the last read of `thread`'s Changes lists the file at `path`
    /// (relative to the checkout).
    pub fn lists(&self, thread: ThreadKey, path: &str) -> bool {
        matches!(
            self.diffs.get(&Diff::Changes(thread)).map(|changes| &changes.load),
            Some(Load::Read(files)) if files.iter().any(|file| file.path == path)
        )
    }

    /// Every diff of a Thread `keep` refuses goes.
    pub fn forget(&mut self, keep: impl Fn(ThreadKey) -> bool) {
        self.diffs.retain(|diff, _| diff.thread().is_none_or(&keep));
    }

    /// `diff`, with its filter field, unless it is made already: whether it
    /// was made now, and so is to be read.
    pub fn make(&mut self, diff: Diff, cx: &mut Context<Self>) -> bool {
        if self.diffs.contains_key(&diff) {
            return false;
        }
        let filter = cx.new(|cx| {
            let mut field = Composer::new(cx);
            field.set_role(Role::Field(None), cx);
            field
        });
        cx.subscribe(&filter, move |view, _, _: &ferrite_pane::composer::Edited, cx| {
            if let Some(changes) = view.diffs.get_mut(&diff) {
                changes.relist(cx);
            }
            cx.notify()
        })
        .detach();
        self.diffs.insert(diff, Changes::new(diff.scope(), filter));
        true
    }

    /// A read of `diff` starts: its number, so a late answer for an older
    /// one is dropped, and the scope to read.
    pub fn begin_read(
        &mut self,
        diff: Diff,
        keep_shown: bool,
        cx: &mut Context<Self>,
    ) -> Option<(u64, ReadScope)> {
        let changes = self.diffs.get_mut(&diff)?;
        changes.generation += 1;
        match diff {
            Diff::Changes(_) => {
                if !(keep_shown && matches!(changes.load, Load::Read(_))) {
                    changes.load = Load::Reading;
                }
            }
            Diff::PullRequest(_) => {
                changes.refreshing = matches!(changes.load, Load::Read(_));
                changes.refresh_failure = None;
                if !changes.refreshing {
                    changes.load = Load::Reading;
                }
            }
        }
        cx.notify();
        Some((changes.generation, changes.scope.read()))
    }

    /// Whether `origin` is still where `diff` is read.
    fn current(&self, diff: Diff, origin: &Origin, cx: &App) -> bool {
        self.origin.read(&diff, cx).as_ref() == Some(origin)
    }

    /// Wait on git's `answer` for a Thread's own diff.
    pub fn wait_local(
        &mut self,
        diff: Diff,
        generation: u64,
        origin: Origin,
        answer: std::io::Result<
            std::sync::mpsc::Receiver<std::io::Result<ferrite_core::changes::Snapshot>>,
        >,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |view, cx| {
            let snapshot = answered(answer, cx).await;
            let outcome = cx.background_executor().spawn(async move {
                snapshot.map(|snapshot| {
                    let files = snapshot
                        .files
                        .iter()
                        .map(|sides| File::of(sides, snapshot.staged.contains(&sides.file.path)))
                        .collect::<Vec<_>>();
                    (files, snapshot.branches.into_iter().take(BRANCHES_LISTED).collect())
                })
            }).await;
            view.update(cx, |view, cx| {
                if !view.current(diff, &origin, cx) {
                    return;
                }
                if let Some(changes) = view.diffs.get_mut(&diff).filter(|c| c.generation == generation) {
                    changes.load = match outcome {
                        Ok((files, branches)) => {
                            changes.branches = branches;
                            changes.forget_stale_reviews(&files);
                            Load::Read(files)
                        }
                        Err(why) => Load::Failed(why.to_string().into()),
                    };
                    changes.relist(cx);
                    cx.notify();
                }
            }).ok();
        }).detach();
    }

    /// Wait on the forge's `answer` for a pull request's published files.
    pub fn wait_published(
        &mut self,
        diff: Diff,
        generation: u64,
        origin: Origin,
        answer: std::io::Result<
            std::sync::mpsc::Receiver<std::io::Result<Vec<ferrite_reviewr::ferrite::FileSides>>>,
        >,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |view, cx| {
            let files = answered(answer, cx).await;
            let files = cx
                .background_executor()
                .spawn(async move {
                    files.map(|files| files.iter().map(|sides| File::of(sides, false)).collect::<Vec<_>>())
                })
                .await;
            view.update(cx, |view, cx| {
                if !view.current(diff, &origin, cx) {
                    return;
                }
                if let Some(changes) = view
                    .diffs
                    .get_mut(&diff)
                    .filter(|changes| changes.generation == generation)
                {
                    changes.refreshing = false;
                    match files {
                        Ok(files) => {
                            changes.forget_stale_reviews(&files);
                            changes.load = Load::Read(files);
                            changes.relist(cx);
                        }
                        Err(error) if matches!(changes.load, Load::Read(_)) => {
                            changes.refresh_failure = Some(error.to_string().into());
                        }
                        Err(error) => changes.load = Load::Failed(error.to_string().into()),
                    }
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// A stage or unstage of `diff` starts: its number, so only the last
    /// one's refusal is said.
    pub fn begin_stage(&mut self, diff: Diff) -> Option<u64> {
        let changes = self.diffs.get_mut(&diff)?;
        changes.stage_generation += 1;
        Some(changes.stage_generation)
    }

    /// Wait on git's `outcome` for a stage or unstage, then read the list
    /// again.
    #[allow(clippy::too_many_arguments)]
    pub fn wait_stage(
        &mut self,
        diff: Diff,
        generation: u64,
        origin: Origin,
        path: String,
        stage: bool,
        outcome: std::io::Result<std::sync::mpsc::Receiver<std::io::Result<()>>>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |view, cx| {
            let outcome = answered(outcome, cx).await;
            view.update(cx, |view, cx| {
                if !view.current(diff, &origin, cx) {
                    return;
                }
                if let Some(changes) = view.diffs.get_mut(&diff).filter(|changes| changes.stage_generation == generation) {
                    changes.stage_failure = outcome.err().map(|message| StageFailure {
                        path,
                        stage,
                        message: message.to_string().into(),
                    });
                }
                cx.emit(ChangesEvent::Read { diff, keep_shown: true });
            }).ok();
        }).detach();
    }

    /// The file at `path` unfolded and brought into view.
    pub fn reveal(&mut self, diff: Diff, path: &str, cx: &mut Context<Self>) {
        let Some(changes) = self.diffs.get_mut(&diff) else {
            return;
        };
        changes.folded.remove(path);
        changes.relist(cx);
        if let Load::Read(files) = &changes.load {
            let item = files
                .iter()
                .position(|file| file.path == path)
                .and_then(|at| changes.listed.iter().position(|&listed| listed == at));
            if let Some(item) = item {
                // A file the list has not laid out has no height yet, so a
                // reveal would stop short of it.
                if changes.scroll.bounds_for_item(item).is_some() {
                    changes.scroll.scroll_to_reveal_item(item);
                } else {
                    changes.scroll.scroll_to(ListOffset { item_ix: item, offset_in_item: px(0.) });
                }
            }
        }
        cx.notify();
    }

    pub fn run_verb(&mut self, diff: Diff, verb: Verb, cx: &mut Context<Self>) {
        let Some(changes) = self.diffs.get_mut(&diff) else {
            return;
        };
        match verb {
            Verb::All | Verb::Uncommitted | Verb::Compare(_) => {
                changes.scope = match verb {
                    Verb::All => Scope::All,
                    Verb::Uncommitted => Scope::Uncommitted,
                    Verb::Compare(at) => match changes.branches.get(at) {
                        Some(branch) => Scope::Against(branch.clone()),
                        None => return,
                    },
                    _ => unreachable!(),
                };
                cx.emit(ChangesEvent::Read { diff, keep_shown: false });
                return;
            }
            Verb::CollapseAll => {
                if let Load::Read(files) = &changes.load {
                    changes.folded = files.iter().map(|file| file.path.clone()).collect();
                }
            }
            Verb::ExpandAll => changes.folded.clear(),
            Verb::ToggleWrap => changes.wrap = !changes.wrap,
            Verb::ToggleWhitespace => changes.hide_whitespace = !changes.hide_whitespace,
            Verb::ToggleTests => changes.hide_tests = !changes.hide_tests,
            Verb::Refresh => {
                changes.stage_failure = None;
                cx.emit(ChangesEvent::Read { diff, keep_shown: false });
                return;
            }
        }
        changes.relist(cx);
        cx.notify();
    }

    fn toggle_fold(&mut self, diff: Diff, path: &str, cx: &mut Context<Self>) {
        if let Some(changes) = self.diffs.get_mut(&diff) {
            if !changes.folded.remove(path) {
                changes.folded.insert(path.to_string());
            }
            changes.relist(cx);
            cx.notify();
        }
    }

    fn toggle_reviewed(&mut self, diff: Diff, path: &str, cx: &mut Context<Self>) {
        if let Some(changes) = self.diffs.get_mut(&diff) {
            if changes.reviewed.remove(path).is_some() {
                changes.folded.remove(path);
            } else if let Load::Read(files) = &changes.load {
                if let Some(file) = files.iter().find(|file| file.path == path) {
                    changes.reviewed.insert(path.to_string(), file.revision);
                    changes.folded.insert(path.to_string());
                }
            }
            changes.relist(cx);
            cx.notify();
        }
    }

    /// The rows of the scope dropdown.
    pub fn scope_rows(&self, diff: Diff) -> Vec<Option<(menu::Item, Row)>> {
        let Some(changes) = self.diffs.get(&diff) else {
            return Vec::new();
        };
        let row = |label: &'static str, scope: Scope, verb: Verb| {
            let mut item = menu::Item::new(label);
            item.checked = changes.scope == scope;
            Some((item, Row::Verb(verb)))
        };
        vec![
            row("All changes", Scope::All, Verb::All),
            row("Uncommitted changes", Scope::Uncommitted, Verb::Uncommitted),
            Some((menu::Item::new("Compare against"), Row::Sub(Sub))),
        ]
    }

    /// `Compare against ›`: the branches, the standing one ticked.
    pub fn branch_rows(&self, diff: Diff) -> Vec<(menu::Item, Verb)> {
        let Some(changes) = self.diffs.get(&diff) else {
            return Vec::new();
        };
        changes
            .branches
            .iter()
            .enumerate()
            .map(|(at, branch)| {
                let mut item = menu::Item::new(branch.clone());
                item.checked = changes.scope == Scope::Against(branch.clone());
                (item, Verb::Compare(at))
            })
            .collect()
    }

    /// The Changes `⋯` menu's rows.
    pub fn more_rows(&self, diff: Diff) -> Vec<Option<(menu::Item, Verb)>> {
        let Some(changes) = self.diffs.get(&diff) else {
            return Vec::new();
        };
        let row = |label: &'static str, checked: bool, verb: Verb| {
            let mut item = menu::Item::new(label);
            item.checked = checked;
            Some((item, verb))
        };
        vec![
            row("Collapse all files", false, Verb::CollapseAll),
            row("Expand all files", false, Verb::ExpandAll),
            None,
            row("Word wrap", changes.wrap, Verb::ToggleWrap),
            row(
                "Hide whitespace changes",
                changes.hide_whitespace,
                Verb::ToggleWhitespace,
            ),
            row("Hide test files", changes.hide_tests, Verb::ToggleTests),
            None,
            row("Refresh", false, Verb::Refresh),
        ]
    }

    // ------------------------------------------------------------ drawing

    pub fn pane(&self, diff: Diff, inputs: ChangesInputs, cx: &mut Context<Self>) -> Div {
        let Some(changes) = self.diffs.get(&diff) else {
            return div().flex_1();
        };
        let body = match &changes.load {
            Load::Reading => note("Reading changes\u{2026}").into_any_element(),
            Load::Failed(why) => note(why.clone()).into_any_element(),
            Load::Read(files) if files.is_empty() => {
                Self::empty(diff, &changes.scope, cx).into_any_element()
            }
            Load::Read(_) => {
                let list = if changes.listed.is_empty() {
                    note("No files match").into_any_element()
                } else {
                    div()
                        .debug_selector(|| "changes-scroll".into())
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .child(
                            list(
                                changes.scroll.clone(),
                                cx.processor(move |view, item, _, cx| view.item(diff, item, cx)),
                            )
                            .size_full(),
                        )
                        .into_any_element()
                };
                let empty = changes.filter.read(cx).text().trim().is_empty();
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .children(changes.stage_failure.as_ref().map(stage_failure_note))
                    .child(
                        div()
                            .debug_selector(|| "changes-filter".into())
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(theme::SPACE_2))
                            .h(px(theme::ICON_BUTTON))
                            .mx(px(theme::SPACE_2))
                            .px(px(theme::SPACE_2))
                            .rounded(px(theme::R_CONTROL))
                            .bg(paint::FIELD)
                            .child(icons::icon(
                                icons::SEARCH,
                                theme::ICON_BUTTON_GLYPH,
                                theme::INK_SUBTLE,
                            ))
                            .child(
                                div()
                                    .relative()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(theme::FS_LABEL))
                                    .child(changes.filter.clone())
                                    .when(empty, |line| {
                                        line.child(
                                            div()
                                                .absolute()
                                                .top_0()
                                                .left_0()
                                                .text_color(rgb(theme::INK_SUBTLE))
                                                .child("Filter files"),
                                        )
                                    }),
                            ),
                    )
                    .child(list)
                    .into_any_element()
            }
        };
        div()
            .debug_selector(|| "changes".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(Self::head(diff, changes, inputs, cx))
            .when(changes.refreshing, |pane| {
                pane.child(
                    div()
                        .debug_selector(|| "pr-code-refreshing".into())
                        .px(px(theme::SPACE_2))
                        .py(px(theme::SPACE_1))
                        .text_color(rgb(theme::INK_SUBTLE))
                        .child("Refreshing changes…"),
                )
            })
            .children(changes.refresh_failure.as_ref().map(|why| {
                div()
                    .debug_selector(|| "pr-code-refresh-failure".into())
                    .px(px(theme::SPACE_2))
                    .py(px(theme::SPACE_1))
                    .text_color(rgb(theme::RED))
                    .child(why.clone())
            }))
            .child(body)
    }

    /// Nothing changed in the scope: its note, and for the uncommitted scope
    /// a link to the whole branch's diff.
    fn empty(diff: Diff, scope: &Scope, cx: &mut Context<Self>) -> Div {
        let note = note("No changes");
        if *scope != Scope::Uncommitted {
            return note;
        }
        note.flex_col().gap(px(theme::SPACE_1)).child(
            div()
                .id("changes-view-branch")
                .debug_selector(|| "changes-view-branch".into())
                .cursor_pointer()
                .text_color(rgb(theme::LINK_INK))
                .child("View branch diff")
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.run_verb(diff, Verb::All, cx)
                })),
        )
    }

    /// T3's diff toolbar: the scope and what it compares at the left, the
    /// view's tools at the right.
    fn head(diff: Diff, changes: &Changes, inputs: ChangesInputs, cx: &mut Context<Self>) -> Div {
        use theme::*;
        let all_folded = matches!(&changes.load, Load::Read(files)
            if !files.is_empty() && files.iter().all(|file| changes.folded.contains(&file.path)));
        let tool =
            |key: &'static str, glyph: &'static str, tip: &'static str, state: IconState, verb: Verb| {
                use ferrite_ui::components::{icon_button_as, IconSize, Tip as _};
                icon_button_as(key, glyph, IconSize::Compact, tip, state, cx)
                    .tip(tip)
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.run_verb(diff, verb, cx);
                    }))
            };
        let press = cx.listener(move |_, _: &(), _, cx| cx.emit(ChangesEvent::More { diff }));
        let (fold_tip, fold_verb) = if all_folded {
            ("Expand all files", Verb::ExpandAll)
        } else {
            ("Collapse all files", Verb::CollapseAll)
        };
        let tools = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_0_5))
            .child(tool(
                "changes-refresh",
                icons::REFRESH,
                "Refresh",
                IconState::Plain,
                Verb::Refresh,
            ))
            .child(tool(
                "changes-fold-all",
                icons::COLLAPSE_ALL,
                fold_tip,
                IconState::Plain,
                fold_verb,
            ))
            .child(tool(
                "changes-wrap",
                icons::TEXT_WRAP,
                "Word wrap",
                IconState::Toggle(changes.wrap),
                Verb::ToggleWrap,
            ))
            .child(tool(
                "changes-whitespace",
                icons::PILCROW,
                "Hide whitespace changes",
                IconState::Toggle(changes.hide_whitespace),
                Verb::ToggleWhitespace,
            ))
            .child(tool(
                "changes-hide-tests",
                icons::FLASK_OFF,
                "Hide test files",
                IconState::Toggle(changes.hide_tests),
                Verb::ToggleTests,
            ));
        let ChangesInputs { head, default_branch, more } = inputs;
        let base = match &changes.scope {
            Scope::Uncommitted => None,
            Scope::All => Some(
                default_branch
                    .as_ref()
                    .map_or_else(|| "the base".to_string(), |base| base.to_string()),
            ),
            Scope::Against(branch) => Some(branch.clone()),
        };
        let comparing = base.filter(|_| diff.local_thread().is_some()).map(|base| {
            let head = head
                .as_ref()
                .map_or_else(|| "HEAD".to_string(), |head| head.to_string());
            div()
                .debug_selector(|| "changes-comparing".into())
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(SPACE_1))
                .text_size(px(FS_BADGE))
                .text_color(rgb(INK_SUBTLE))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(SharedString::from(format!("Comparing {head} against"))),
                )
                .child(
                    div()
                        .id("changes-base")
                        .debug_selector(|| "changes-base".into())
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(SPACE_0_5))
                        .px(px(SPACE_1))
                        .rounded(px(R_CONTROL))
                        .text_color(rgb(INK_STRONG))
                        .cursor_pointer()
                        .hover(|base| base.bg(paint::RAISED))
                        .child(SharedString::from(base))
                        .child(icons::icon(icons::CHEVRON_DOWN, ICON_CHEVRON, INK_SUBTLE))
                        .on_click(cx.listener(move |_, event: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            cx.emit(ChangesEvent::Menu {
                                dropdown: Dropdown::Base,
                                diff,
                                at: event.position(),
                            });
                        })),
                )
        });
        div()
            .debug_selector(|| "changes-toolbar".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .gap(px(SPACE_2))
            .h(px(SURFACE_TOOLBAR_H))
            .px(px(SPACE_2))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(SPACE_2))
                    // A published PR cannot switch to a local checkout scope.
                    .when(matches!(diff, Diff::Changes(_)), |left| {
                        left.child(
                            div()
                                .id("changes-scope")
                                .debug_selector(|| "changes-scope".into())
                                .flex()
                                .flex_shrink_0()
                                .items_center()
                                .gap(px(SPACE_1))
                                .h(px(SURFACE_TAB_H))
                                .px(px(SPACE_2))
                                .rounded(px(R_CONTROL))
                                .bg(paint::RAISED)
                                .text_size(px(FS_BADGE))
                                .font_weight(W_LABEL)
                                .cursor_pointer()
                                .hover(|scope| scope.bg(paint::SELECTED))
                                .child(SharedString::from(changes.scope.label()))
                                .child(icons::icon(
                                    icons::CHEVRON_DOWN,
                                    SURFACE_PLUS_GLYPH,
                                    INK_SUBTLE,
                                ))
                                .on_click(cx.listener(move |_, event: &ClickEvent, _, cx| {
                                    cx.stop_propagation();
                                    cx.emit(ChangesEvent::Menu {
                                        dropdown: Dropdown::Scope,
                                        diff,
                                        at: event.position(),
                                    });
                                })),
                        )
                        .children(comparing)
                    }),
            )
            .child(tools.child(more.button(
                ferrite_ui::components::icon_button(
                    "changes-more",
                    icons::ELLIPSIS,
                    ferrite_ui::components::IconSize::Standard,
                    "More",
                    cx,
                ),
                "More",
                move |window, cx| press(&(), window, cx),
            )))
    }

    /// The list's `item`th file, as wide as the list so an unwrapped
    /// file's own scroller carries its long lines sideways.
    fn item(&self, diff: Diff, item: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(changes) = self.diffs.get(&diff) else {
            return div().into_any_element();
        };
        let Load::Read(files) = &changes.load else {
            return div().into_any_element();
        };
        let Some((at, file)) = changes.listed.get(item).and_then(|&at| Some((at, files.get(at)?))) else {
            return div().into_any_element();
        };
        Self::file(diff, changes, at, file, cx).w_full().into_any_element()
    }

    fn file(
        diff: Diff,
        changes: &Changes,
        at: usize,
        file: &File,
        cx: &mut Context<Self>,
    ) -> Div {
        use theme::*;
        let folded = changes.folded.contains(&file.path);
        let reviewed = changes.reviewed.get(&file.path) == Some(&file.revision);
        let (name, dir) = match file.path.rsplit_once('/') {
            Some((dir, name)) => (name.to_string(), dir.to_string()),
            None => (file.path.clone(), String::new()),
        };
        let (fold_path, review_path, open_path) =
            (file.path.clone(), file.path.clone(), file.path.clone());
        let (stage_path, staged) = (file.path.clone(), file.staged);
        let stat = div()
            .flex()
            .flex_shrink_0()
            .gap(px(SPACE_1))
            .text_size(px(FS_META))
            .child(
                div()
                    .text_color(rgb(GREEN))
                    .child(SharedString::from(format!("+{}", file.added))),
            )
            .children((file.removed > 0).then(|| {
                div()
                    .text_color(rgb(RED))
                    .child(SharedString::from(format!("\u{2212}{}", file.removed)))
            }));
        // A door of this file's head, its key naming the file's row.
        let door = |key: String, glyph: &'static str, tip: &'static str, state: IconState| {
            use ferrite_ui::components::{icon_button_as, IconSize, Tip as _};
            icon_button_as(key, glyph, IconSize::Standard, tip, state, cx).tip(tip)
        };
        let head = div()
            .id(SharedString::from(format!("changes-file-{at}")))
            .debug_selector(move || format!("changes-file-{at}"))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(PANEL_SECTION_H))
            .px(px(SIDE_CHAT_INSET))
            .cursor_pointer()
            .text_size(px(FS_LABEL))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.toggle_fold(diff, &fold_path, cx)
            }))
            .child(ferrite_ui::components::chevron_at(
                if folded { 0. } else { 1. },
                ICON_CHEVRON,
                INK_SUBTLE,
            ))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(SPACE_1))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(
                        div()
                            .text_color(rgb(if reviewed { INK_SUBTLE } else { LINK_INK }))
                            .child(SharedString::from(name)),
                    )
                    .child(
                        div()
                            .text_color(rgb(INK_SUBTLE))
                            .child(SharedString::from(dir)),
                    ),
            )
            .child(stat)
            .when(diff.local_thread().is_some(), |head| {
                head.child(
                    door(
                        format!("changes-stage-{at}"),
                        if file.staged {
                            icons::CHECK
                        } else {
                            icons::PLUS
                        },
                        if file.staged { "Unstage" } else { "Stage" },
                        if file.staged { IconState::Done } else { IconState::Plain },
                    )
                    .on_click(cx.listener(
                        move |_, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            cx.emit(ChangesEvent::Stage { diff, path: stage_path.clone(), stage: !staged });
                        },
                    )),
                )
            })
            .child(
                door(
                    format!("changes-reviewed-{at}"),
                    if reviewed {
                        icons::CHECK_CIRCLE
                    } else {
                        icons::CIRCLE_DASHED
                    },
                    if reviewed {
                        "Mark as not reviewed"
                    } else {
                        "Mark as reviewed"
                    },
                    if reviewed { IconState::Done } else { IconState::Plain },
                )
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.toggle_reviewed(diff, &review_path, cx);
                })),
            )
            .when_some(diff.local_thread(), |head, thread| {
                head.child(
                    door(
                        format!("changes-open-{at}"),
                        icons::EXTERNAL,
                        "Open file",
                        IconState::Plain,
                    )
                    .on_click(cx.listener(
                        move |_, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            cx.emit(ChangesEvent::Open { thread, path: open_path.clone() });
                        },
                    )),
                )
            });
        let hunks = (!folded).then(|| {
            let hunks = if changes.hide_whitespace {
                &file.hunks_without_whitespace
            } else {
                &file.hunks
            };
            hunks_element(at, hunks, Layout::Whole { wrap: changes.wrap })
        });
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(head)
            .children(hunks)
    }
}

/// Why the last stage or unstage did nothing, naming its file; pressing
/// the file's button again retries.
fn stage_failure_note(failure: &StageFailure) -> Div {
    use theme::*;
    let verb = if failure.stage { "stage" } else { "unstage" };
    div()
        .debug_selector(|| "changes-stage-failure".into())
        .flex_shrink_0()
        .mx(px(SPACE_2))
        .mb(px(SPACE_1))
        .px(px(SPACE_2))
        .py(px(SPACE_1))
        .rounded(px(R_CONTROL))
        .bg(paint::RAISED)
        .text_size(px(FS_META))
        .text_color(rgb(RED))
        .child(SharedString::from(format!(
            "Couldn't {verb} {}: {}",
            failure.path,
            if failure.message.is_empty() {
                "git refused"
            } else {
                &failure.message
            },
        )))
}

fn note(note: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .text_size(px(theme::FS_LABEL))
        .text_color(rgb(theme::INK_SUBTLE))
        .child(note.into())
}

#[cfg(any(test, feature = "test-support"))]
impl ChangesView {
    /// How many files the last read found, once it has landed.
    pub fn files_read(&self, diff: Diff) -> Option<usize> {
        match &self.diffs.get(&diff)?.load {
            Load::Read(files) => Some(files.len()),
            _ => None,
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ChangesView {
    /// How many of `thread`'s diffs are held.
    pub fn held(&self, thread: ThreadKey) -> usize {
        self.diffs.keys().filter(|diff| diff.thread() == Some(thread)).count()
    }

    pub fn reviewed_for_test(&self, diff: Diff, path: &str) -> bool {
        self.diffs
            .get(&diff)
            .is_some_and(|changes| changes.reviewed.contains_key(path))
    }

    pub fn stage_failure_for_test(&self, diff: Diff) -> Option<String> {
        let failure = self.diffs.get(&diff)?.stage_failure.as_ref()?;
        Some(format!("{} {}", failure.path, failure.message))
    }

    pub fn toggle_fold_for_test(&mut self, diff: Diff, path: &str, cx: &mut Context<Self>) {
        self.toggle_fold(diff, path, cx);
    }

    pub fn filter(&self, diff: Diff) -> Option<Entity<Composer>> {
        Some(self.diffs.get(&diff)?.filter.clone())
    }

    pub fn scope_label(&self, diff: Diff) -> Option<String> {
        Some(self.diffs.get(&diff)?.scope.label())
    }
}

#[cfg(test)]
mod tests;
