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
//! wherever it shows through `changes_pane`. What it draws of the window
//! (the branch names, the `⋯` Menu button) comes with `ChangesInputs`; what
//! it asks of the window (a read, a stage, a file opened, a menu) it says
//! as a `ChangesEvent`. Git and the forge are the window's, so the window
//! asks them and hands the answers to the view, which keeps each read's
//! number and drops an answer from another checkout or machine.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use ferrite_core::changes::Scope as ReadScope;
use ferrite_core::machine::Bridge;
use crate::cockpit::menu_button::{Door, Doors, Edge, MenuHost};
use ferrite_ui::components::IconState;
use ferrite_ui::icons;
use ferrite_ui::theme::{self, paint};

use ferrite_pane::composer::{Composer, Role};
use gpui::{list, App, ListAlignment, ListOffset, ListState, WeakEntity};

use super::*;
use crate::cockpit::context_menu::Submenu;
use crate::cockpit::panel::hunks::{Layout, Line, Shown, Sign, hunks_element, shown};
use crate::cockpit::panel::pr_read::PrTarget;
use crate::cockpit::surfaces::SurfaceId;

/// What the Changes diff is against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
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
pub(crate) enum Diff {
    Changes(ThreadId),
    PullRequest(PrTarget),
}

impl Diff {
    pub(crate) fn local_thread(self) -> Option<ThreadId> {
        match self {
            Diff::Changes(thread) => Some(thread),
            Diff::PullRequest(_) => None,
        }
    }

    /// The Thread whose diff this is: its own, or a pull request read in
    /// its checkout. `None` for a Project's pull request.
    pub(super) fn thread(self) -> Option<ThreadId> {
        match self {
            Diff::Changes(thread) => Some(thread),
            Diff::PullRequest(target) => match target.source {
                crate::cockpit::panel::pr_read::PrSource::Thread(thread) => Some(thread),
                crate::cockpit::panel::pr_read::PrSource::Project(_) => None,
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
pub(crate) enum Verb {
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
pub(crate) struct Sub;

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

pub(crate) struct Changes {
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

    pub(super) fn new(scope: Scope, filter: Entity<Composer>) -> Self {
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
pub(crate) fn is_test_path(path: &str) -> bool {
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
pub(crate) fn without_whitespace(hunks: Vec<Vec<Line>>) -> Vec<Vec<Line>> {
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
pub(crate) struct Origin {
    root: PathBuf,
    machine: Option<Arc<Bridge>>,
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
pub(crate) struct ChangesInputs {
    head: Option<SharedString>,
    default_branch: Option<SharedString>,
    doors: Doors,
}

/// What the Changes ask of the window.
pub(in crate::cockpit) enum ChangesEvent {
    /// Read `diff` again in its scope. `keep_shown`: the last read stands
    /// until the new one lands.
    Read { diff: Diff, keep_shown: bool },
    /// A file's stage button: its change into git's index, or out of it.
    Stage { diff: Diff, path: String, stage: bool },
    /// A file's `↗`: it opens in Files.
    Open { thread: ThreadId, path: String },
    /// The scope or the base dropdown, at the press.
    Menu { target: MenuTarget, at: gpui::Point<Pixels> },
    /// A press on the `⋯` button.
    Door(Door),
}

impl CockpitView {
    /// Where `diff` is read: its Thread's checkout on the window's machine,
    /// or the checkout or Project its pull request is read in.
    pub(in crate::cockpit) fn changes_origin(&self, diff: Diff) -> Option<Origin> {
        match diff {
            Diff::Changes(thread) => Some(Origin {
                root: self.thread_path(thread)?,
                machine: self.cockpit.machine().cloned(),
            }),
            Diff::PullRequest(target) => Some(Origin {
                root: self.pr_source_path(target.source)?,
                machine: None,
            }),
        }
    }

    /// `diff` as the Changes draw it, wherever it shows: the Panel's
    /// Changes Surface, a pull request's Code.
    pub(in crate::cockpit) fn changes_pane(&self, diff: Diff, cx: &mut Context<Self>) -> Div {
        let thread = diff.local_thread();
        let inputs = ChangesInputs {
            head: thread.and_then(|thread| self.thread_branch(thread)),
            default_branch: thread
                .and_then(|thread| self.facts.get(thread))
                .and_then(|facts| facts.default_branch.clone()),
            doors: self.doors(),
        };
        self.changes
            .update(cx, |changes, cx| changes.pane(diff, &inputs, cx))
    }

    /// `diff`, made and read the first time it is asked for.
    pub(in crate::cockpit) fn ensure_diff(&mut self, diff: Diff, cx: &mut Context<Self>) {
        if self.changes.update(cx, |changes, cx| changes.make(diff, cx)) {
            self.read_diff(diff, false, cx);
        }
    }

    /// Ask for `diff` in its scope, and hand the answer to the Changes.
    pub(in crate::cockpit) fn read_diff(&mut self, diff: Diff, keep_shown: bool, cx: &mut Context<Self>) {
        let Some(origin) = self.changes_origin(diff) else {
            return;
        };
        let Some((generation, scope)) = self
            .changes
            .update(cx, |changes, cx| changes.begin_read(diff, keep_shown, cx))
        else {
            return;
        };
        match diff {
            Diff::Changes(_) => {
                let answer = self.cockpit.changes(&origin.root, scope);
                self.changes.update(cx, |changes, cx| {
                    changes.wait_local(diff, generation, origin, answer, cx)
                });
            }
            Diff::PullRequest(target) => {
                let answer = if cfg!(any(test, feature = "visual-reference"))
                    && !self.cockpit.has_forge_fixture(&origin.root)
                {
                    Err(std::io::Error::other(
                        "No published diff fixture for this pull request",
                    ))
                } else {
                    self.cockpit.review_pull_request_diff(&origin.root, target.number)
                };
                self.changes.update(cx, |changes, cx| {
                    changes.wait_published(diff, generation, origin, answer, cx)
                });
            }
        }
    }

    /// Stage or unstage `path` in `diff`'s checkout; the Changes read the
    /// list again once git answers.
    fn stage_file(&mut self, diff: Diff, path: String, stage: bool, cx: &mut Context<Self>) {
        let Some(origin) = diff.local_thread().and_then(|_| self.changes_origin(diff)) else {
            return;
        };
        let Some(generation) = self.changes.update(cx, |changes, _| changes.begin_stage(diff)) else {
            return;
        };
        let outcome = self.cockpit.set_file_staged(&origin.root, path.clone(), stage);
        self.changes.update(cx, |changes, cx| {
            changes.wait_stage(diff, generation, origin, path, stage, outcome, cx)
        });
    }

    /// A Files tab's `Show changes`: the Changes column opens on that file,
    /// its diff unfolded and in view.
    pub(in crate::cockpit) fn show_changes_of(
        &mut self,
        thread: ThreadId,
        path: &str,
        cx: &mut Context<Self>,
    ) {
        self.upsert_surface(thread, SurfaceId::Changes, cx);
        self.changes
            .update(cx, |changes, cx| changes.reveal(Diff::Changes(thread), path, cx));
    }

    pub(in crate::cockpit) fn on_changes_event(
        &mut self,
        _: Entity<ChangesView>,
        event: &ChangesEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ChangesEvent::Read { diff, keep_shown } => self.read_diff(*diff, *keep_shown, cx),
            ChangesEvent::Stage { diff, path, stage } => {
                self.stage_file(*diff, path.clone(), *stage, cx)
            }
            ChangesEvent::Open { thread, path } => {
                if let Some(root) = self.thread_path(*thread) {
                    self.open_in_files(*thread, root.join(path), None, false, cx);
                }
            }
            ChangesEvent::Menu { target, at } => self.open_context_menu(*target, *at, cx),
            ChangesEvent::Door(door) => self.toggle_door(*door, cx),
        }
    }
}

/// Every diff the window has read, a Thread's Changes and each pull
/// request's Code, each with its own scope, folds and filter.
pub(crate) struct ChangesView {
    cockpit: WeakEntity<CockpitView>,
    diffs: HashMap<Diff, Changes>,
}

impl EventEmitter<ChangesEvent> for ChangesView {}

impl MenuHost for ChangesView {
    fn press_door(&mut self, door: Door, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ChangesEvent::Door(door));
    }

    fn tip<E: InteractiveElement>(element: E, text: SharedString) -> E {
        <CockpitView as MenuHost>::tip(element, text)
    }
}

impl ChangesView {
    pub(in crate::cockpit) fn new(cockpit: WeakEntity<CockpitView>) -> Self {
        Self {
            cockpit,
            diffs: HashMap::new(),
        }
    }

    /// Whether `diff` has been made.
    pub(in crate::cockpit) fn holds(&self, diff: Diff) -> bool {
        self.diffs.contains_key(&diff)
    }

    /// Whether the last read of `thread`'s Changes lists the file at `path`
    /// (relative to the checkout).
    pub(in crate::cockpit) fn lists(&self, thread: ThreadId, path: &str) -> bool {
        matches!(
            self.diffs.get(&Diff::Changes(thread)).map(|changes| &changes.load),
            Some(Load::Read(files)) if files.iter().any(|file| file.path == path)
        )
    }

    /// Every diff of a Thread `keep` refuses goes.
    pub(in crate::cockpit) fn forget(&mut self, keep: impl Fn(ThreadId) -> bool) {
        self.diffs.retain(|diff, _| diff.thread().is_none_or(&keep));
    }

    /// `diff`, with its filter field, unless it is made already: whether it
    /// was made now, and so is to be read.
    fn make(&mut self, diff: Diff, cx: &mut Context<Self>) -> bool {
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
    fn begin_read(
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
        self.cockpit
            .upgrade()
            .and_then(|cockpit| cockpit.read(cx).changes_origin(diff))
            .as_ref()
            == Some(origin)
    }

    /// Wait on git's `answer` for a Thread's own diff.
    fn wait_local(
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
            let snapshot = crate::cockpit::beside::answered(answer, cx).await;
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
    fn wait_published(
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
            let files = crate::cockpit::beside::answered(answer, cx).await;
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
    fn begin_stage(&mut self, diff: Diff) -> Option<u64> {
        let changes = self.diffs.get_mut(&diff)?;
        changes.stage_generation += 1;
        Some(changes.stage_generation)
    }

    /// Wait on git's `outcome` for a stage or unstage, then read the list
    /// again.
    #[allow(clippy::too_many_arguments)]
    fn wait_stage(
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
            let outcome = crate::cockpit::beside::answered(outcome, cx).await;
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
    fn reveal(&mut self, diff: Diff, path: &str, cx: &mut Context<Self>) {
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

    pub(in crate::cockpit) fn run_verb(&mut self, diff: Diff, verb: Verb, cx: &mut Context<Self>) {
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
    pub(in crate::cockpit) fn scope_rows(&self, diff: Diff) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let Some(changes) = self.diffs.get(&diff) else {
            return Vec::new();
        };
        let row = |label: &'static str, scope: Scope, verb: Verb| {
            let mut item = menu::Item::new(label);
            item.checked = changes.scope == scope;
            Some((item, MenuVerb::Changes(verb)))
        };
        vec![
            row("All changes", Scope::All, Verb::All),
            row("Uncommitted changes", Scope::Uncommitted, Verb::Uncommitted),
            Some((
                menu::Item::new("Compare against"),
                MenuVerb::Submenu(Submenu::Changes(Sub)),
            )),
        ]
    }

    /// `Compare against ›`: the branches, the standing one ticked.
    pub(in crate::cockpit) fn branch_rows(&self, diff: Diff) -> Vec<(menu::Item, MenuVerb)> {
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
                (item, MenuVerb::Changes(Verb::Compare(at)))
            })
            .collect()
    }

    /// The Changes `⋯` menu's rows.
    pub(in crate::cockpit) fn more_rows(&self, diff: Diff) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let Some(changes) = self.diffs.get(&diff) else {
            return Vec::new();
        };
        let row = |label: &'static str, checked: bool, verb: Verb| {
            let mut item = menu::Item::new(label);
            item.checked = checked;
            Some((item, MenuVerb::Changes(verb)))
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

    fn pane(&self, diff: Diff, inputs: &ChangesInputs, cx: &mut Context<Self>) -> Div {
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
    fn head(diff: Diff, changes: &Changes, inputs: &ChangesInputs, cx: &mut Context<Self>) -> Div {
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
        let base = match &changes.scope {
            Scope::Uncommitted => None,
            Scope::All => Some(
                inputs
                    .default_branch
                    .as_ref()
                    .map_or_else(|| "the base".to_string(), |base| base.to_string()),
            ),
            Scope::Against(branch) => Some(branch.clone()),
        };
        let comparing = base.filter(|_| diff.local_thread().is_some()).map(|base| {
            let head = inputs
                .head
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
                                target: MenuTarget::ChangesBase(diff),
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
                                        target: MenuTarget::ChangesScope(diff),
                                        at: event.position(),
                                    });
                                })),
                        )
                        .children(comparing)
                    }),
            )
            .child(
                tools.child(inputs.doors.button(
                    ferrite_ui::components::icon_button(
                        "changes-more",
                        icons::ELLIPSIS,
                        ferrite_ui::components::IconSize::Standard,
                        "More",
                        cx,
                    ),
                    Door::new(MenuTarget::ChangesMore(diff), Edge::Right),
                    "More",
                    cx,
                )),
            )
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

#[cfg(any(test, feature = "headless"))]
impl ChangesView {
    /// How many files the last read found, once it has landed.
    pub(in crate::cockpit) fn files_read(&self, diff: Diff) -> Option<usize> {
        match &self.diffs.get(&diff)?.load {
            Load::Read(files) => Some(files.len()),
            _ => None,
        }
    }
}

#[cfg(test)]
impl ChangesView {
    /// How many of `thread`'s diffs are held.
    pub(in crate::cockpit) fn held(&self, thread: ThreadId) -> usize {
        self.diffs.keys().filter(|diff| diff.thread() == Some(thread)).count()
    }

    pub(in crate::cockpit) fn reviewed_for_test(&self, diff: Diff, path: &str) -> bool {
        self.diffs
            .get(&diff)
            .is_some_and(|changes| changes.reviewed.contains_key(path))
    }

    pub(in crate::cockpit) fn stage_failure_for_test(&self, diff: Diff) -> Option<String> {
        let failure = self.diffs.get(&diff)?.stage_failure.as_ref()?;
        Some(format!("{} {}", failure.path, failure.message))
    }

    pub(in crate::cockpit) fn toggle_fold_for_test(&mut self, diff: Diff, path: &str, cx: &mut Context<Self>) {
        self.toggle_fold(diff, path, cx);
    }

    pub(in crate::cockpit) fn filter(&self, diff: Diff) -> Option<Entity<Composer>> {
        Some(self.diffs.get(&diff)?.filter.clone())
    }

    pub(in crate::cockpit) fn scope_label(&self, diff: Diff) -> Option<String> {
        Some(self.diffs.get(&diff)?.scope.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(sign: Sign, text: &str) -> Line {
        Line {
            sign,
            text: text.to_string().into(),
        }
    }

    #[test]
    fn a_block_too_big_to_match_keeps_its_changes() {
        let side = 1_001;
        let block: Vec<Line> = (0..side)
            .map(|n| line(Sign::Removed, &format!("line {n}")))
            .chain((0..side).map(|n| line(Sign::Added, &format!("  line {n}"))))
            .collect();
        let out = without_whitespace(vec![block.clone()]);
        assert_eq!(out, vec![block]);
    }

    #[test]
    fn hiding_whitespace_keeps_a_swap_of_two_lines() {
        let swap = vec![
            line(Sign::Removed, "x"),
            line(Sign::Removed, "y"),
            line(Sign::Added, "y"),
            line(Sign::Added, "x"),
        ];
        let out = without_whitespace(vec![swap]);
        assert_eq!(out.len(), 1, "a swap is still a change");
        assert!(out[0].iter().any(|line| line.sign != Sign::Context));
    }

    #[test]
    fn hiding_whitespace_keeps_the_lines_in_file_order() {
        let block = vec![
            line(Sign::Removed, "let a = 1;"),
            line(Sign::Removed, "let b = 2;"),
            line(Sign::Added, "    let a = 1;"),
        ];
        let out = without_whitespace(vec![block]);
        let shown: Vec<_> = out[0]
            .iter()
            .map(|l| (l.sign, l.text.trim().to_string()))
            .collect();
        assert_eq!(
            shown,
            [
                (Sign::Context, "let a = 1;".into()),
                (Sign::Removed, "let b = 2;".into())
            ]
        );
    }

    #[test]
    fn hiding_whitespace_drops_blank_changes_and_the_hunks_they_leave_bare() {
        let kept = vec![line(Sign::Context, "a"), line(Sign::Added, "b")];
        let bare = vec![
            line(Sign::Context, "c"),
            line(Sign::Added, "  "),
            line(Sign::Removed, ""),
        ];
        let mixed = vec![line(Sign::Added, ""), line(Sign::Removed, "x")];
        let out = without_whitespace(vec![kept.clone(), bare, mixed]);
        assert_eq!(out, [kept, vec![line(Sign::Removed, "x")]]);
    }

    #[test]
    fn hiding_whitespace_drops_reindented_and_trailing_space_changes() {
        let reindented = vec![
            line(Sign::Context, "fn a() {"),
            line(Sign::Removed, "    let x = 1;"),
            line(Sign::Added, "\tlet x = 1; "),
            line(Sign::Context, "}"),
        ];
        let substantive = vec![
            line(Sign::Removed, "    let y = 1;"),
            line(Sign::Added, "        let y = 2;"),
        ];
        let out = without_whitespace(vec![reindented, substantive.clone()]);
        assert_eq!(out, [substantive]);
    }

    #[test]
    fn hiding_whitespace_keeps_the_real_edit_beside_a_reindented_line() {
        let hunk = vec![
            line(Sign::Removed, "  a"),
            line(Sign::Removed, "b"),
            line(Sign::Added, "a"),
            line(Sign::Added, "c"),
        ];
        let out = without_whitespace(vec![hunk]);
        assert_eq!(
            out,
            [vec![
                line(Sign::Context, "a"),
                line(Sign::Removed, "b"),
                line(Sign::Added, "c")
            ]]
        );
    }

    #[test]
    fn test_files_are_told_by_their_path() {
        use super::is_test_path;
        for test in [
            "tests/a.rs",
            "src/cockpit/tests/pr.rs",
            "a_test.go",
            "test_a.py",
            "a.test.ts",
            "a.spec.js",
            "src/tests.rs",
        ] {
            assert!(is_test_path(test), "{test}");
        }
        for code in ["src/attest.rs", "contest.md", "src/testing.rs", "a.rs"] {
            assert!(!is_test_path(code), "{code}");
        }
    }

    #[test]
    fn a_scope_says_what_it_reads() {
        assert_eq!(Scope::Uncommitted.read(), ReadScope::Uncommitted);
        assert_eq!(Scope::All.read(), ReadScope::All);
        assert_eq!(Scope::Against("dev".into()).read(), ReadScope::Against("dev".into()));
        assert_eq!(Scope::All.label(), "All changes");
    }
}
