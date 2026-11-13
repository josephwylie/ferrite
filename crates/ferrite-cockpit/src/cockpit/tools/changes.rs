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

use std::collections::HashSet;

use ferrite_reviewr::ferrite::{review_against, Read};
use ferrite_ui::icons;
use ferrite_ui::theme::{self, paint};
use gpui::{list, ListAlignment, ListOffset, ListState};

use ferrite_pane::composer::{Composer, Role};

use super::*;
use crate::cockpit::surfaces::SurfaceId;
use crate::cockpit::context_menu::Submenu;
use crate::cockpit::panel::hunks::{hunks_face, piece_element, pieces, shown, Line, Piece, Shown, Sign};

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

    fn read(&self) -> (Read, Option<String>) {
        match self {
            Scope::All => (Read::Branch, None),
            Scope::Uncommitted => (Read::Uncommitted, None),
            Scope::Against(branch) => (Read::Branch, Some(branch.clone())),
        }
    }
}

/// One of a Thread's diffs, each with its own scope, folds and filter: the
/// Changes Surface's, in the scope the operator picks, and a pull request's
/// Code view, always the branch's whole diff.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Diff {
    Changes(ThreadId),
    PullRequest(ThreadId),
}

impl Diff {
    pub(crate) fn thread(self) -> ThreadId {
        match self {
            Diff::Changes(thread) | Diff::PullRequest(thread) => thread,
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

/// How many branches `Compare against ›` lists.
const BRANCHES_LISTED: usize = 20;

/// How far past the viewport the list lays out rows.
const OVERDRAW: Pixels = px(200.);

struct File {
    path: String,
    added: u32,
    removed: u32,
    hunks: Shown,
    /// `hunks` without the changes that are only whitespace.
    trimmed: Shown,
    /// The file's change is in git's index (`git add`).
    staged: bool,
}

/// One row of the list, the unit `list` lays out only while it is in view.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Row {
    /// A file's head, by its place in the read.
    Head(usize),
    /// A row of an unfolded file's hunks, by the file's place in the read.
    Hunk(usize, Piece),
}

enum Load {
    Reading,
    Read(Vec<File>),
    Failed(SharedString),
}

pub(crate) struct Changes {
    scope: Scope,
    load: Load,
    /// Files marked reviewed: folded, their circle filled.
    reviewed: HashSet<String>,
    folded: HashSet<String>,
    wrap: bool,
    hide_whitespace: bool,
    /// Test files left out of the list (`is_test_path`).
    hide_tests: bool,
    /// What the list draws, as of the last change to the read, the folds,
    /// the filter or the view tools.
    rows: Vec<Row>,
    list: ListState,
    /// The branches `Compare against ›` lists, as of the last read.
    branches: Vec<String>,
    /// Bumped by every read, so a late answer for an old scope is dropped.
    generation: u64,
    /// The filter field over the files: only names it matches are listed.
    filter: Entity<Composer>,
}

impl Changes {
    pub(super) fn new(scope: Scope, filter: Entity<Composer>) -> Self {
        Self {
            scope,
            load: Load::Reading,
            reviewed: HashSet::new(),
            folded: HashSet::new(),
            wrap: true,
            hide_whitespace: false,
            hide_tests: false,
            rows: Vec::new(),
            list: ListState::new(0, ListAlignment::Top, OVERDRAW),
            branches: Vec::new(),
            generation: 0,
            filter,
        }
    }

    /// Each listed file's head, then, unless it is folded, its hunk rows.
    fn rows(&self, wanted: &str) -> Vec<Row> {
        let Load::Read(files) = &self.load else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for (at, file) in files.iter().enumerate() {
            if !wanted.is_empty() && !file.path.to_lowercase().contains(wanted) {
                continue;
            }
            if self.hide_tests && is_test_path(&file.path) {
                continue;
            }
            rows.push(Row::Head(at));
            if !self.folded.contains(&file.path) {
                let hunks = if self.hide_whitespace { &file.trimmed } else { &file.hunks };
                rows.extend(pieces(hunks).into_iter().map(|piece| Row::Hunk(at, piece)));
            }
        }
        rows
    }

    /// Lay out `rows` in place of the last: only the run between what they
    /// share at either end is measured again, so the rows above it keep
    /// their place on screen.
    fn relist(&mut self, rows: Vec<Row>) {
        let start = self.rows.iter().zip(&rows).take_while(|(old, new)| old == new).count();
        let end = self.rows[start..]
            .iter()
            .rev()
            .zip(rows[start..].iter().rev())
            .take_while(|(old, new)| old == new)
            .count();
        self.list.splice(start..self.rows.len() - end, rows.len() - start - end);
        self.rows = rows;
    }
}

/// Whether `path` reads as a test file (T3's Hide test files): under a
/// `test`/`tests`/`__tests__`/`spec` directory, or named as one
/// (`*_test.*`, `test_*`, `*.test.*`, `*.spec.*`, `tests.rs`).
pub(crate) fn is_test_path(path: &str) -> bool {
    let mut parts = path.split('/').collect::<Vec<_>>();
    let name = parts.pop().unwrap_or_default();
    if parts.iter().any(|dir| matches!(*dir, "test" | "tests" | "__tests__" | "spec")) {
        return true;
    }
    let stem = name.split('.').next().unwrap_or(name);
    stem.ends_with("_test")
        || stem.starts_with("test_")
        || stem == "tests"
        || name.contains(".test.")
        || name.contains(".spec.")
}

/// The paths whose change is staged, as git's index holds them.
fn staged_paths(root: &std::path::Path) -> HashSet<String> {
    std::process::Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(root)
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

/// `hunks` without the changes that are only whitespace: a changed line
/// blank after trimming goes, and a hunk left with no change goes with it.
pub(crate) fn without_whitespace(hunks: Vec<Vec<Line>>) -> Vec<Vec<Line>> {
    hunks
        .into_iter()
        .map(|hunk| {
            hunk.into_iter()
                .filter(|line| line.sign == Sign::Context || !line.text.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|hunk| hunk.iter().any(|line| line.sign != Sign::Context))
        .collect()
}

impl CockpitView {
    fn changes_of(&mut self, diff: Diff) -> Option<&mut Changes> {
        self.tools.changes.get_mut(&diff)
    }

    /// The Changes Surface's diff of `thread`, read the first time it is
    /// made.
    pub(in crate::cockpit) fn ensure_changes(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.ensure_diff(Diff::Changes(thread), cx);
    }

    /// The branch's whole diff of `thread`: what a pull request's Code view
    /// shows, whatever the Changes Surface was left on.
    pub(in crate::cockpit) fn ensure_branch_changes(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.ensure_diff(Diff::PullRequest(thread), cx);
    }

    fn ensure_diff(&mut self, diff: Diff, cx: &mut Context<Self>) {
        if !self.tools.changes.contains_key(&diff) {
            let filter = cx.new(|cx| {
                let mut field = Composer::new(cx);
                field.set_role(Role::Field(None), cx);
                field
            });
            cx.subscribe(&filter, move |view, _, _: &ferrite_pane::composer::Edited, cx| {
                view.relist_changes(diff, cx);
                cx.notify();
            })
            .detach();
            self.tools.changes.insert(diff, Changes::new(diff.scope(), filter));
            self.refresh_changes(diff, cx);
        }
    }

    /// Whether the last read of `thread`'s Changes lists the file at `path`
    /// (relative to the checkout).
    pub(in crate::cockpit) fn changes_list(&self, thread: ThreadId, path: &str) -> bool {
        matches!(
            self.tools.changes.get(&Diff::Changes(thread)).map(|changes| &changes.load),
            Some(Load::Read(files)) if files.iter().any(|file| file.path == path)
        )
    }

    /// A Files tab's `Show changes`: the Changes column opens on that file,
    /// its diff unfolded and in view.
    pub(in crate::cockpit) fn show_changes_of(&mut self, thread: ThreadId, path: &str, cx: &mut Context<Self>) {
        self.upsert_surface(thread, SurfaceId::Changes, cx);
        let Some(changes) = self.changes_of(Diff::Changes(thread)) else {
            return;
        };
        changes.folded.remove(path);
        let head = match &changes.load {
            Load::Read(files) => files.iter().position(|file| file.path == path).map(Row::Head),
            _ => None,
        };
        self.relist_changes(Diff::Changes(thread), cx);
        if let (Some(changes), Some(head)) = (self.changes_of(Diff::Changes(thread)), head) {
            if let Some(item_ix) = changes.rows.iter().position(|row| *row == head) {
                changes.list.scroll_to(ListOffset { item_ix, offset_in_item: px(0.) });
            }
        }
        cx.notify();
    }

    /// Recount the list's rows after the read, a fold, the filter or a view
    /// tool changed them. A read still out keeps the last rows in place, so
    /// the scroll holds through a refresh.
    fn relist_changes(&mut self, diff: Diff, cx: &mut Context<Self>) {
        let Some(changes) = self.tools.changes.get_mut(&diff) else {
            return;
        };
        if !matches!(changes.load, Load::Read(_)) {
            return;
        }
        let wanted = changes.filter.read(cx).text().trim().to_lowercase();
        let rows = changes.rows(&wanted);
        changes.relist(rows);
    }

    /// A file's stage button: its change goes into git's index, or comes
    /// out of it, and the list is read again.
    fn stage_file(&mut self, diff: Diff, path: String, stage: bool, cx: &mut Context<Self>) {
        let Some(root) = self.thread_path(diff.thread()) else {
            return;
        };
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .spawn(async move {
                    let mut git = std::process::Command::new("git");
                    git.current_dir(&root);
                    if stage {
                        git.args(["add", "--"]);
                    } else {
                        git.args(["restore", "--staged", "--"]);
                    }
                    let _ = git.arg(path).output();
                })
                .await;
            view.update(cx, |view, cx| view.refresh_changes(diff, cx)).ok();
        })
        .detach();
    }

    /// Read the diff again, for the current scope.
    fn refresh_changes(&mut self, diff: Diff, cx: &mut Context<Self>) {
        let Some(root) = self.thread_path(diff.thread()) else {
            return;
        };
        let Some(changes) = self.changes_of(diff) else {
            return;
        };
        changes.generation += 1;
        changes.load = Load::Reading;
        changes.branches = ferrite_reviewr::git::list_branches(&root)
            .unwrap_or_default()
            .into_iter()
            .take(BRANCHES_LISTED)
            .map(|branch| branch.name)
            .collect();
        let generation = changes.generation;
        let (read, base) = changes.scope.read();
        cx.spawn(async move |view, cx| {
            let files = cx
                .background_executor()
                .spawn(async move {
                    let staged = staged_paths(&root);
                    review_against(&root, &read, base.as_deref()).map(|files| {
                        files
                            .iter()
                            .map(|sides| {
                                let hunks = shown(Some(sides));
                                let trimmed = match &hunks {
                                    Shown::Hunks(hunks) => Shown::Hunks(without_whitespace(hunks.clone())),
                                    note => note.clone(),
                                };
                                File {
                                    path: sides.file.path.clone(),
                                    added: sides.file.additions,
                                    removed: sides.file.deletions,
                                    hunks,
                                    trimmed,
                                    staged: staged.contains(&sides.file.path),
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .await;
            view.update(cx, |view, cx| {
                if let Some(changes) = view.changes_of(diff).filter(|c| c.generation == generation) {
                    changes.load = match files {
                        Ok(files) => Load::Read(files),
                        Err(why) => Load::Failed(why.into()),
                    };
                    view.relist_changes(diff, cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(in crate::cockpit) fn run_changes_verb(
        &mut self,
        diff: Diff,
        verb: Verb,
        cx: &mut Context<Self>,
    ) {
        let Some(changes) = self.changes_of(diff) else {
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
                self.refresh_changes(diff, cx);
                return;
            }
            Verb::CollapseAll => {
                if let Load::Read(files) = &changes.load {
                    changes.folded = files.iter().map(|file| file.path.clone()).collect();
                }
            }
            Verb::ExpandAll => changes.folded.clear(),
            Verb::ToggleWrap => {
                changes.wrap = !changes.wrap;
                changes.list.remeasure();
            }
            Verb::ToggleWhitespace => changes.hide_whitespace = !changes.hide_whitespace,
            Verb::ToggleTests => changes.hide_tests = !changes.hide_tests,
            Verb::Refresh => {
                self.refresh_changes(diff, cx);
                return;
            }
        }
        self.relist_changes(diff, cx);
        cx.notify();
    }

    fn toggle_fold(&mut self, diff: Diff, path: &str, cx: &mut Context<Self>) {
        if let Some(changes) = self.changes_of(diff) {
            if !changes.folded.remove(path) {
                changes.folded.insert(path.to_string());
            }
            self.relist_changes(diff, cx);
            cx.notify();
        }
    }

    fn toggle_reviewed(&mut self, diff: Diff, path: &str, cx: &mut Context<Self>) {
        if let Some(changes) = self.changes_of(diff) {
            if changes.reviewed.remove(path) {
                changes.folded.remove(path);
            } else {
                changes.reviewed.insert(path.to_string());
                changes.folded.insert(path.to_string());
            }
            self.relist_changes(diff, cx);
            cx.notify();
        }
    }

    /// The rows of the scope dropdown.
    pub(in crate::cockpit) fn changes_scope_rows(&self, diff: Diff) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let Some(changes) = self.tools.changes.get(&diff) else {
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
    pub(in crate::cockpit) fn changes_branch_rows(&self, diff: Diff) -> Vec<(menu::Item, MenuVerb)> {
        let Some(changes) = self.tools.changes.get(&diff) else {
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
    pub(in crate::cockpit) fn changes_more_rows(&self, diff: Diff) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let Some(changes) = self.tools.changes.get(&diff) else {
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
            row("Hide whitespace changes", changes.hide_whitespace, Verb::ToggleWhitespace),
            row("Hide test files", changes.hide_tests, Verb::ToggleTests),
            None,
            row("Refresh", false, Verb::Refresh),
        ]
    }

    // ------------------------------------------------------------ drawing

    pub(in crate::cockpit) fn changes_pane(&self, diff: Diff, cx: &mut Context<Self>) -> Div {
        let Some(changes) = self.tools.changes.get(&diff) else {
            return div().flex_1();
        };
        let body = match &changes.load {
            Load::Reading => self.changes_note("Reading changes\u{2026}").into_any_element(),
            Load::Failed(why) => self.changes_note(why.clone()).into_any_element(),
            Load::Read(files) if files.is_empty() => {
                self.changes_empty(diff, &changes.scope, cx).into_any_element()
            }
            Load::Read(_) => {
                let list = if changes.rows.is_empty() {
                    self.changes_note("No files match").into_any_element()
                } else {
                    let view = cx.entity();
                    div()
                        .debug_selector(|| "changes-scroll".into())
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .child(
                            list(changes.list.clone(), move |ix, _, cx| {
                                view.update(cx, |view, cx| view.changes_row(diff, ix, cx))
                            })
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
                            .child(icons::icon(icons::SEARCH, theme::ICON_BUTTON_GLYPH, theme::INK_SUBTLE))
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
            .child(self.changes_head(diff, changes, cx))
            .child(body)
    }

    /// Nothing changed in the scope: its note, and for the uncommitted scope
    /// a link to the whole branch's diff.
    fn changes_empty(&self, diff: Diff, scope: &Scope, cx: &mut Context<Self>) -> Div {
        let note = self.changes_note("No changes");
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
                    view.run_changes_verb(diff, Verb::All, cx)
                })),
        )
    }

    fn changes_note(&self, note: impl Into<SharedString>) -> Div {
        div()
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .text_size(px(theme::FS_LABEL))
            .text_color(rgb(theme::INK_SUBTLE))
            .child(note.into())
    }

    /// T3's diff toolbar: the scope and what it compares at the left, the
    /// view's tools at the right.
    fn changes_head(&self, diff: Diff, changes: &Changes, cx: &mut Context<Self>) -> Div {
        use theme::*;
        let all_folded = matches!(&changes.load, Load::Read(files)
            if !files.is_empty() && files.iter().all(|file| changes.folded.contains(&file.path)));
        let tool = |key: &'static str, glyph: &'static str, tip: &'static str, on: bool, verb: Verb| {
            div()
                .id(key)
                .debug_selector(move || key.into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .size(px(SURFACE_PLUS))
                .rounded(px(R_CONTROL))
                .when(on, |tool| tool.bg(paint::SELECTED))
                .hover_control(key)
                .child(icons::icon(glyph, SURFACE_PLUS_GLYPH, if on { INK_STRONG } else { INK_SUBTLE }))
                .tooltip(ferrite_ui::menu::tooltip(tip))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.run_changes_verb(diff, verb, cx);
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
            .child(tool("changes-refresh", icons::REFRESH, "Refresh", false, Verb::Refresh))
            .child(tool("changes-fold-all", icons::COLLAPSE_ALL, fold_tip, false, fold_verb))
            .child(tool("changes-wrap", icons::TEXT_WRAP, "Word wrap", changes.wrap, Verb::ToggleWrap))
            .child(tool(
                "changes-whitespace",
                icons::PILCROW,
                "Hide whitespace changes",
                changes.hide_whitespace,
                Verb::ToggleWhitespace,
            ))
            .child(tool(
                "changes-hide-tests",
                icons::FLASK_OFF,
                "Hide test files",
                changes.hide_tests,
                Verb::ToggleTests,
            ));
        let base = match &changes.scope {
            Scope::Uncommitted => None,
            Scope::All => Some(
                self.facts
                    .get(diff.thread())
                    .and_then(|facts| facts.default_branch.clone())
                    .map_or_else(|| "the base".to_string(), |base| base.to_string()),
            ),
            Scope::Against(branch) => Some(branch.clone()),
        };
        let comparing = base.map(|base| {
            let head = self
                .thread_branch(diff.thread())
                .map_or_else(|| "HEAD".to_string(), |head| head.to_string());
            div()
                .debug_selector(|| "changes-comparing".into())
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(SPACE_1))
                .text_size(px(FS_BADGE))
                .text_color(rgb(INK_SUBTLE))
                .child(div().min_w_0().truncate().child(SharedString::from(format!("Comparing {head} against"))))
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
                        .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            view.open_context_menu(MenuTarget::ChangesBase(diff), event.position(), cx);
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
                    // A pull request's Code view is the branch's diff: no
                    // scope to pick, nothing else to compare against.
                    .when(matches!(diff, Diff::Changes(_)), |left| left.child(
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
                            .child(icons::icon(icons::CHEVRON_DOWN, SURFACE_PLUS_GLYPH, INK_SUBTLE))
                            .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                view.open_context_menu(MenuTarget::ChangesScope(diff), event.position(), cx);
                            })),
                    )
                    .children(comparing)),
            )
            .child(
                tools.child(
                    ferrite_pane::preview::head_door("changes-more", "changes-more".into(), icons::ELLIPSIS)
                        .tooltip(ferrite_ui::menu::tooltip("More"))
                        .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            view.open_context_menu(MenuTarget::ChangesMore(diff), event.position(), cx);
                        })),
                ),
            )
    }

    /// Row `ix` of `diff`'s list: a file's head, or a row of its hunks on
    /// the diff face, padded where the file's hunks begin and end.
    fn changes_row(&self, diff: Diff, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        use theme::*;
        let Some(changes) = self.tools.changes.get(&diff) else {
            return div().into_any_element();
        };
        let (Load::Read(files), Some(row)) = (&changes.load, changes.rows.get(ix)) else {
            return div().into_any_element();
        };
        match row {
            Row::Head(at) => self.changes_head_row(diff, changes, *at, &files[*at], cx).into_any_element(),
            Row::Hunk(at, piece) => {
                let at = *at;
                let first = matches!(ix.checked_sub(1).map(|before| &changes.rows[before]), Some(Row::Head(_)));
                let last = !matches!(changes.rows.get(ix + 1), Some(Row::Hunk(..)));
                hunks_face(changes.wrap)
                    .when(first, |face| {
                        face.debug_selector(move || format!("panel-hunks-{at}")).pt(px(PANEL_HUNK_PAD_T))
                    })
                    .when(last, |face| face.pb(px(PANEL_HUNK_PAD_B)))
                    .child(piece_element(at, piece, changes.wrap))
                    .into_any_element()
            }
        }
    }

    fn changes_head_row(
        &self,
        diff: Diff,
        changes: &Changes,
        at: usize,
        file: &File,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        use theme::*;
        let folded = changes.folded.contains(&file.path);
        let reviewed = changes.reviewed.contains(&file.path);
        let (name, dir) = match file.path.rsplit_once('/') {
            Some((dir, name)) => (name.to_string(), dir.to_string()),
            None => (file.path.clone(), String::new()),
        };
        let (fold_path, review_path, open_path) = (file.path.clone(), file.path.clone(), file.path.clone());
        let (stage_path, staged) = (file.path.clone(), file.staged);
        let stat = div()
            .flex()
            .flex_shrink_0()
            .gap(px(SPACE_1))
            .text_size(px(FS_META))
            .child(div().text_color(rgb(GREEN)).child(SharedString::from(format!("+{}", file.added))))
            .children((file.removed > 0).then(|| {
                div()
                    .text_color(rgb(RED))
                    .child(SharedString::from(format!("\u{2212}{}", file.removed)))
            }));
        // A door of this file's head, its key naming the file's row.
        let door = |key: String, glyph: &'static str, tip: &'static str, ink: u32| {
            let id = SharedString::from(key);
            let selector = id.to_string();
            div()
                .id(id.clone())
                .debug_selector(move || selector.clone())
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .size(px(ICON_BUTTON))
                .rounded(px(R_CONTROL))
                .hover_control(id)
                .child(icons::icon(glyph, ICON_BUTTON_GLYPH, ink))
                .tooltip(ferrite_ui::menu::tooltip(tip))
        };
        div()
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
                    .child(div().text_color(rgb(INK_SUBTLE)).child(SharedString::from(dir))),
            )
            .child(stat)
            .child(
                door(
                    format!("changes-stage-{at}"),
                    if file.staged { icons::CHECK } else { icons::PLUS },
                    if file.staged { "Unstage" } else { "Stage" },
                    if file.staged { GREEN } else { INK_SUBTLE },
                )
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.stage_file(diff, stage_path.clone(), !staged, cx);
                })),
            )
            .child(
                door(
                    format!("changes-reviewed-{at}"),
                    if reviewed { icons::CHECK_CIRCLE } else { icons::CIRCLE_DASHED },
                    if reviewed { "Mark as not reviewed" } else { "Mark as reviewed" },
                    if reviewed { GREEN } else { INK_SUBTLE },
                )
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.toggle_reviewed(diff, &review_path, cx);
                })),
            )
            .child(
                door(format!("changes-open-{at}"), icons::EXTERNAL, "Open file", INK_SUBTLE).on_click(
                    cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        if let Some(root) = view.thread_path(diff.thread()) {
                            view.open_in_files(diff.thread(), root.join(&open_path), None, false, cx);
                        }
                    }),
                ),
            )
    }
}

#[cfg(test)]
impl CockpitView {
    /// How many files the last read found, once it has landed.
    pub(in crate::cockpit) fn changes_read(&self, diff: Diff) -> Option<usize> {
        match &self.tools.changes.get(&diff)?.load {
            Load::Read(files) => Some(files.len()),
            _ => None,
        }
    }

    pub(in crate::cockpit) fn changes_filter(&self, diff: Diff) -> Option<Entity<Composer>> {
        Some(self.tools.changes.get(&diff)?.filter.clone())
    }

    pub(in crate::cockpit) fn changes_scope_label(&self, diff: Diff) -> Option<String> {
        Some(self.tools.changes.get(&diff)?.scope.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(sign: Sign, text: &str) -> Line {
        Line { sign, text: text.to_string().into() }
    }

    #[test]
    fn hiding_whitespace_drops_blank_changes_and_the_hunks_they_leave_bare() {
        let kept = vec![line(Sign::Context, "a"), line(Sign::Added, "b")];
        let bare = vec![line(Sign::Context, "c"), line(Sign::Added, "  "), line(Sign::Removed, "")];
        let mixed = vec![line(Sign::Added, ""), line(Sign::Removed, "x")];
        let out = without_whitespace(vec![kept.clone(), bare, mixed]);
        assert_eq!(out, [kept, vec![line(Sign::Removed, "x")]]);
    }

    #[test]
    fn test_files_are_told_by_their_path() {
        use super::is_test_path;
        for test in ["tests/a.rs", "src/cockpit/tests/pr.rs", "a_test.go", "test_a.py", "a.test.ts", "a.spec.js", "src/tests.rs"] {
            assert!(is_test_path(test), "{test}");
        }
        for code in ["src/attest.rs", "contest.md", "src/testing.rs", "a.rs"] {
            assert!(!is_test_path(code), "{code}");
        }
    }

    #[test]
    fn a_scope_says_what_it_reads() {
        assert_eq!(Scope::Uncommitted.read(), (Read::Uncommitted, None));
        assert_eq!(Scope::All.read(), (Read::Branch, None));
        assert_eq!(Scope::Against("dev".into()).read(), (Read::Branch, Some("dev".into())));
        assert_eq!(Scope::All.label(), "All changes");
    }
}
