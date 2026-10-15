//! The Review slot's body (ADR 0021): herdr-reviewr's review, vendored
//! (`ferrite_reviewr`), drawn here beside the Thread. The bar names the
//! scopes — the uncommitted work, the branch against its base, the last
//! turn, one of the branch's commits — and the branch's pull request
//! (`PR`), with what the shown diff adds up to. The bar and footer are
//! chrome; the rows read at the operator's reading size, like every diff in
//! a transcript.
//!
//! Under the bar, each changed file's rows as the vendored diff model
//! builds them: numbered on both sides, signed and washed like every diff
//! Ferrite draws, the changed words washed deeper, long unchanged runs
//! folded (`⋯ N lines`: a click, or ⏎ on it, opens it). A press picks a
//! row, a shift-press or a drag stretches the pick to a range in the same
//! file; ↑↓ and ⇧↑↓ do the same from the keyboard. A right-click asks the
//! cockpit for the slot's context menu (`MenuAsked`). `c` (or ⏎, or the
//! `+ comment` beside the pick) opens a one-line field under it; ⏎ there
//! adds the comment to the Pane's Composer, where it waits for the next
//! send (`ferrite_reviewr::ferrite::attach`). ⌫ (or a comment's `×`) takes
//! back the latest comment under the pick.
//!
//! `/` searches the checkout — file names and lines, by the vendored
//! `fff-search` — and a result opens its file whole (the file view), whose
//! lines take comments too; esc goes back to the diff. ⌘F finds in the
//! rows shown. `m` renders each Markdown file's new side. `p` shows the
//! pull request, as its forge's CLI reads it. The commits scope first lists
//! the branch's commits, newest first: ↑↓ or a click picks one, ⏎ (or the
//! click) reviews it, and the scope again goes back to the list. The
//! cockpit reads each of these where the checkout is (`cockpit/beside.rs`)
//! and hands it over; each is a snapshot, read again on `r`.

use std::ops::Range;

use ferrite_core::protocol::{
    PullRequest, WireCheckStatus, WireMerge, WirePr, WirePrCommentKind, WirePrState, WireSync,
};
use ferrite_reviewr::diff::{FileDiff, Row};
use ferrite_reviewr::ferrite::{self as reviewr, FileSides};
use ferrite_reviewr::git::{abbreviate_oid, CommitRow};
use ferrite_reviewr::model::{Comment as ReviewComment, CommitPick, Rev, Scope};
use ferrite_reviewr::search::SearchResults;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, rgba, AnyElement, ClickEvent, Context, Entity, EventEmitter, FocusHandle,
    Focusable, HighlightStyle, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, ScrollHandle, SharedString, StyledText, Window,
};

use crate::composer::{Composer, Edited, Role};
use crate::pane::{DiffKind, Grid};
use crate::pointer::Pointer as _;
use crate::theme::*;

gpui::actions!(
    review,
    [
        // In the `Review` context: walk and stretch the pick, comment on
        // it or take a comment back, step or reread the scope, search the
        // checkout, render Markdown, show the pull request, go back or
        // close the slot.
        Next,
        Previous,
        ExtendNext,
        ExtendPrevious,
        Comment,
        Uncomment,
        NextScope,
        Reread,
        Search,
        ToggleMarkdown,
        ShowPullRequest,
        Close,
        // In the comment field's `ReviewComment` context.
        SaveComment,
        CancelComment,
        // In the search field's `ReviewSearch` context.
        OpenFound,
        CloseSearch,
    ]
);

/// The slot's keys, as its footer spells them.
const FOOTER: [(&str, &str); 9] = [
    ("\u{2191}\u{2193}", "line"),
    ("\u{21e7}\u{2191}\u{2193}", "range"),
    ("c", "comment"),
    ("\u{232b}", "uncomment"),
    ("\u{21e5}", "scope"),
    ("/", "search"),
    ("m", "markdown"),
    ("p", "PR"),
    ("esc", ""),
];

/// The slot's keys while it lists the branch's commits.
const COMMITS_FOOTER: [(&str, &str); 5] = [
    ("\u{2191}\u{2193}", "commit"),
    ("\u{23ce}", "review"),
    ("\u{21e5}", "scope"),
    ("r", "reread"),
    ("esc", ""),
];

/// The search field's keys.
const SEARCH_FOOTER: [(&str, &str); 3] = [
    ("\u{2191}\u{2193}", "result"),
    ("\u{23ce}", "open"),
    ("esc", ""),
];

/// The keys while a file or the pull request is shown.
const BACK_FOOTER: [(&str, &str); 3] = [("r", "reread"), ("/", "search"), ("esc", "back")];

/// The operator picked another scope; the cockpit loads it and calls
/// `show` (`show_commits` for the commits scope).
pub(crate) struct ScopePicked(pub Scope);

/// The operator picked one of the listed commits, by its id; the cockpit
/// loads its diff and calls `show`.
pub(crate) struct CommitPicked(pub String);

/// The search field reads `0`; the cockpit searches the checkout and calls
/// `show_found`.
pub(crate) struct Searched(pub String);

/// The operator opened file `path` of the checkout (at `line`, a found
/// line); the cockpit reads it and calls `show_file`.
pub(crate) struct FileOpened {
    pub path: String,
}

/// The operator asked for the pull request; the cockpit reads it and calls
/// `show_pr`.
pub(crate) struct PrAsked;

/// The operator right-clicked row `line` (the pick holds it); the cockpit
/// opens the slot's context menu `at` the pointer.
pub(crate) struct MenuAsked {
    pub at: Point<Pixels>,
    pub line: usize,
}

/// One file as the slot draws it: the vendored model's rows, what it adds
/// up to, and its new side for the Markdown view.
struct ReviewFile {
    diff: FileDiff,
    additions: u32,
    deletions: u32,
    new: Option<String>,
}

impl ReviewFile {
    fn of(sides: FileSides) -> Self {
        Self {
            diff: sides.rows(),
            additions: sides.file.additions,
            deletions: sides.file.deletions,
            new: sides.sides.ok().map(|(_, new)| new),
        }
    }

    fn is_markdown(&self) -> bool {
        std::path::Path::new(&self.diff.path)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
            })
    }
}

/// A file opened whole from a search: its path, the line a found line
/// names, and its rows once read.
struct FileView {
    path: String,
    line: Option<u64>,
    file: Option<ReviewFile>,
}

/// The search over the slot: its field, the results for the query it last
/// ran, and the result the arrows are on.
struct SearchPanel {
    field: Entity<Composer>,
    results: Option<Result<SearchResults, String>>,
    pick: usize,
    _edited: gpui::Subscription,
}

impl SearchPanel {
    /// The results as rows: each path match, then each line match.
    fn rows(&self) -> Vec<Found> {
        let Some(Ok(results)) = &self.results else {
            return Vec::new();
        };
        let files = results.files.iter().map(|hit| Found {
            path: hit.path.clone(),
            line: None,
            text: None,
        });
        let code = results.code.iter().map(|hit| Found {
            path: hit.path.clone(),
            line: Some(hit.line),
            text: Some(hit.text.clone()),
        });
        files.chain(code).collect()
    }
}

/// One search result row.
#[derive(Clone)]
struct Found {
    path: String,
    line: Option<u64>,
    text: Option<String>,
}

/// One drawn child of the rows' list: a row (its index in `lines`), a
/// file's notice in place of its rows, or a Markdown file rendered.
#[derive(Clone, Copy)]
enum Child {
    Row(usize),
    Notice(usize),
    Markdown(usize),
}

/// A row of the shown files: its file and its index in that file's rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shown {
    file: usize,
    row: usize,
}

/// A run of picked rows: where it was started (`anchor`) and where it ends
/// now (`head`), both indices into `lines`. A pick never leaves the file
/// it started in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    fn at(line: usize) -> Self {
        Self {
            anchor: line,
            head: line,
        }
    }

    fn first(self) -> usize {
        self.anchor.min(self.head)
    }

    fn last(self) -> usize {
        self.anchor.max(self.head)
    }

    fn contains(self, line: usize) -> bool {
        (self.first()..=self.last()).contains(&line)
    }
}

pub(crate) struct ReviewView {
    /// The Pane's own prompt: where comments gather, and are read back to
    /// mark the lines they are about.
    composer: Entity<Composer>,
    /// The reading size's grid the rows sit on.
    grid: Grid,
    focus: FocusHandle,
    scroll: ScrollHandle,
    scope: Scope,
    /// The scope's files; `None` while they are being read.
    diff: Option<Vec<ReviewFile>>,
    /// Why the scope (or the file, or the pull request) could not be read.
    error: Option<String>,
    /// A file opened whole, shown in the diff's place.
    file: Option<FileView>,
    /// The pull request is shown in the diff's place.
    pr_open: bool,
    /// The pull request; `None` while it is being read.
    pr: Option<PullRequest>,
    /// Markdown files read rendered rather than as rows.
    markdown: bool,
    /// The shown files' rows, in drawing order.
    lines: Vec<Shown>,
    /// What the list draws, in order, and the child each row is.
    children: Vec<Child>,
    child_of: Vec<usize>,
    /// Each row's syntax colours and changed-word washes, worked out once
    /// per read.
    highlights: Vec<Vec<(Range<usize>, HighlightStyle)>>,
    selection: Option<Selection>,
    /// A press on a line is held: dragging over lines stretches the pick.
    dragging: bool,
    /// The comment being written under the pick.
    editor: Option<Entity<Composer>>,
    /// The commits scope's list; `None` while it is being read.
    commits: Option<Vec<CommitRow>>,
    /// The listed commit the arrows are on.
    commit_pick: usize,
    /// The commit under review; `None` while the list shows.
    commit: Option<CommitRow>,
    search: Option<SearchPanel>,
    /// Find in the rows shown (⌘F).
    find: Option<crate::find::Find>,
    /// Native text for the Markdown view.
    rich: crate::rich::TextCache,
}

impl EventEmitter<ScopePicked> for ReviewView {}
impl EventEmitter<CommitPicked> for ReviewView {}
impl EventEmitter<Searched> for ReviewView {}
impl EventEmitter<FileOpened> for ReviewView {}
impl EventEmitter<PrAsked> for ReviewView {}
impl EventEmitter<MenuAsked> for ReviewView {}

impl ReviewView {
    pub(crate) fn new(
        composer: Entity<Composer>,
        scope: Scope,
        reading: ferrite_core::settings::ReadingSize,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            composer,
            grid: Grid::of(reading),
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            scope,
            diff: None,
            error: None,
            file: None,
            pr_open: false,
            pr: None,
            markdown: false,
            lines: Vec::new(),
            children: Vec::new(),
            child_of: Vec::new(),
            highlights: Vec::new(),
            selection: None,
            dragging: false,
            editor: None,
            commits: None,
            commit_pick: 0,
            commit: None,
            search: None,
            find: None,
            rich: crate::rich::TextCache::default(),
        }
    }

    pub(crate) fn scope(&self) -> Scope {
        self.scope
    }

    /// The id of the commit under review, if one is.
    pub(crate) fn commit(&self) -> Option<&str> {
        self.commit.as_ref().map(|commit| commit.sha.as_str())
    }

    /// The file open whole, if one is.
    pub(crate) fn file_open(&self) -> Option<&str> {
        self.file.as_ref().map(|file| file.path.as_str())
    }

    /// Whether the search is open: its answers still land.
    pub(crate) fn searching(&self) -> bool {
        self.search.is_some()
    }

    /// Whether the pull request is shown.
    pub(crate) fn pr_open(&self) -> bool {
        self.pr_open
    }

    /// The commits scope's list has been read: show it, the newest picked.
    pub(crate) fn show_commits(&mut self, commits: Vec<CommitRow>, cx: &mut Context<Self>) {
        self.commits = Some(commits);
        self.commit_pick = 0;
        self.error = None;
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    /// Whether the commits scope lists commits rather than one's diff.
    fn listing_commits(&self) -> bool {
        self.scope == Scope::Commits && self.commit.is_none() && self.showing_diff()
    }

    /// Whether the scope's diff is what the slot shows.
    fn showing_diff(&self) -> bool {
        !self.pr_open && self.file.is_none()
    }

    /// Review the listed commit at `index`: its diff when it is read.
    fn open_commit(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(commit) = self
            .commits
            .as_ref()
            .and_then(|commits| commits.get(index))
            .cloned()
        else {
            return;
        };
        self.commit_pick = index;
        let sha = commit.sha.clone();
        self.commit = Some(commit);
        self.diff = None;
        self.error = None;
        self.rebuild();
        cx.emit(CommitPicked(sha));
        cx.notify();
    }

    /// The operator changed the reading size: the rows follow it.
    pub(crate) fn set_reading(
        &mut self,
        reading: ferrite_core::settings::ReadingSize,
        cx: &mut Context<Self>,
    ) {
        let grid = Grid::of(reading);
        if grid != self.grid {
            self.grid = grid;
            cx.notify();
        }
    }

    /// The line-number columns: as wide as a tool row's diff numbers.
    fn number_w(&self) -> f32 {
        DIFF_NUMBER_CELLS * self.grid.cell()
    }

    /// The handle holding the keyboard in this review — a field of it, or
    /// the review itself — so the cockpit keeps it there.
    pub(crate) fn held_focus(&self, window: &Window, cx: &gpui::App) -> Option<FocusHandle> {
        let fields = [
            self.editor.as_ref().map(|editor| editor.focus_handle(cx)),
            self.search
                .as_ref()
                .map(|search| search.field.focus_handle(cx)),
        ];
        if self
            .find
            .as_ref()
            .is_some_and(|find| find.holds_focus(window, cx))
        {
            return Some(window.focused(cx)?);
        }
        fields
            .into_iter()
            .flatten()
            .chain([self.focus.clone()])
            .find(|focus| focus.is_focused(window))
    }

    /// `scope`'s files have been read: draw them, the first row picked —
    /// once back from a file open meanwhile.
    pub(crate) fn show(&mut self, scope: Scope, files: Vec<FileSides>, cx: &mut Context<Self>) {
        self.scope = scope;
        self.diff = Some(files.into_iter().map(ReviewFile::of).collect());
        self.error = None;
        if self.file.is_none() {
            self.editor = None;
            self.rebuild();
            self.selection = self.first_content().map(Selection::at);
            self.scroll.scroll_to_item(0);
        }
        cx.notify();
    }

    /// File `path` has been read: show it whole, at the found line.
    pub(crate) fn show_file(&mut self, path: &str, text: String, cx: &mut Context<Self>) {
        let Some(view) = self.file.as_mut().filter(|view| view.path == path) else {
            return;
        };
        view.file = Some(ReviewFile {
            diff: reviewr::file_rows(path, &text),
            additions: 0,
            deletions: 0,
            new: Some(text),
        });
        let line = view.line;
        self.error = None;
        self.editor = None;
        self.rebuild();
        let at = line
            .and_then(|line| {
                self.lines
                    .iter()
                    .position(|shown| self.row(*shown).new_no() == Some(line as u32))
            })
            .or_else(|| self.first_content());
        self.selection = at.map(Selection::at);
        if let Some(at) = at {
            self.scroll_to(at);
        }
        cx.notify();
    }

    /// The pull request has been read.
    pub(crate) fn show_pr(&mut self, pr: PullRequest, cx: &mut Context<Self>) {
        self.pr = Some(pr);
        self.error = None;
        cx.notify();
    }

    /// A search has answered `query`: shown while the field still reads it.
    pub(crate) fn show_found(
        &mut self,
        query: &str,
        results: Result<SearchResults, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(search) = self.search.as_mut() else {
            return;
        };
        if search.field.read(cx).text() != query {
            return;
        }
        search.results = Some(results);
        search.pick = 0;
        cx.notify();
    }

    /// What was asked could not be read: say why in its place.
    pub(crate) fn fail(&mut self, error: String, cx: &mut Context<Self>) {
        self.error = Some(error);
        cx.notify();
    }

    /// Ask for `scope` (the one shown again reads it afresh): the bar
    /// moves at once, the diff when it is read.
    pub(crate) fn pick_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        self.scope = scope;
        self.diff = None;
        self.error = None;
        self.file = None;
        self.pr_open = false;
        self.selection = None;
        self.editor = None;
        self.commits = None;
        self.commit = None;
        self.rebuild();
        cx.emit(ScopePicked(scope));
        cx.notify();
    }

    /// `r`: read again what the slot shows.
    fn reread(&mut self, cx: &mut Context<Self>) {
        if self.pr_open {
            self.open_pr(cx);
        } else if let Some(path) = self.file.as_ref().map(|file| file.path.clone()) {
            let line = self.file.as_ref().and_then(|file| file.line);
            self.open_file(path, line, cx);
        } else {
            self.pick_scope(self.scope, cx);
        }
    }

    /// `p`, or the bar's `PR`: the pull request, read afresh.
    fn open_pr(&mut self, cx: &mut Context<Self>) {
        self.pr_open = true;
        self.pr = None;
        self.error = None;
        self.editor = None;
        cx.emit(PrAsked);
        cx.notify();
    }

    /// File `path` whole, at `line` once read.
    fn open_file(&mut self, path: String, line: Option<u64>, cx: &mut Context<Self>) {
        self.pr_open = false;
        self.error = None;
        self.editor = None;
        self.file = Some(FileView {
            path: path.clone(),
            line,
            file: None,
        });
        self.rebuild();
        cx.emit(FileOpened { path });
        cx.notify();
    }

    /// Esc: back from the pull request or a file to the diff. False when
    /// the diff is what shows, so the slot closes.
    fn back(&mut self, cx: &mut Context<Self>) -> bool {
        if self.pr_open {
            self.pr_open = false;
        } else if self.file.take().is_some() {
            self.rebuild();
            self.selection = self.first_content().map(Selection::at);
        } else {
            return false;
        }
        self.error = None;
        self.editor = None;
        cx.notify();
        true
    }

    /// The files the slot shows: the one open whole, else the scope's.
    fn files(&self) -> &[ReviewFile] {
        match &self.file {
            Some(view) => view.file.as_slice(),
            None => self.diff.as_deref().unwrap_or_default(),
        }
    }

    fn files_mut(&mut self) -> Option<&mut Vec<ReviewFile>> {
        match &mut self.file {
            Some(_) => None,
            None => self.diff.as_mut(),
        }
    }

    fn row(&self, shown: Shown) -> &Row {
        &self.files()[shown.file].diff.rows[shown.row]
    }

    /// The first row that is not a fold.
    fn first_content(&self) -> Option<usize> {
        self.lines
            .iter()
            .position(|shown| self.row(*shown).is_content())
    }

    /// Lay the shown files out as rows again: what the list draws, and
    /// each row's colours.
    fn rebuild(&mut self) {
        let mut lines = Vec::new();
        let mut children = Vec::new();
        let mut child_of = Vec::new();
        let mut highlights = Vec::new();
        for (index, file) in self.files().iter().enumerate() {
            if file.diff.notice.is_some() {
                children.push(Child::Notice(index));
                continue;
            }
            if self.markdown && file.is_markdown() && file.new.is_some() {
                children.push(Child::Markdown(index));
                continue;
            }
            let language =
                ferrite_core::transcript::language_for_path(std::path::Path::new(&file.diff.path));
            for (at, row) in file.diff.rows.iter().enumerate() {
                child_of.push(children.len());
                children.push(Child::Row(lines.len()));
                lines.push(Shown {
                    file: index,
                    row: at,
                });
                highlights.push(row_highlights(row, language));
            }
        }
        self.lines = lines;
        self.children = children;
        self.child_of = child_of;
        self.highlights = highlights;
        self.selection = self
            .selection
            .filter(|picked| picked.last() < self.lines.len());
    }

    fn scroll_to(&self, line: usize) {
        if let Some(child) = self.child_of.get(line) {
            self.scroll.scroll_to_item(*child);
        }
    }

    /// Pick `line` alone, or stretch the pick to it within its file.
    fn pick(&mut self, line: usize, extend: bool, cx: &mut Context<Self>) {
        if line >= self.lines.len() {
            return;
        }
        let picked = match self.selection {
            Some(selection) if extend => self.extend(selection, line),
            _ => Selection::at(line),
        };
        if self.selection == Some(picked) {
            return;
        }
        self.selection = Some(picked);
        self.scroll_to(picked.head);
        cx.notify();
    }

    /// `selection` with its head moved to `line`, held inside its file.
    fn extend(&self, selection: Selection, line: usize) -> Selection {
        let file = self.lines[selection.anchor].file;
        let in_file = |at: &usize| self.lines[*at].file == file;
        let head = if in_file(&line) {
            line
        } else if line > selection.anchor {
            (selection.anchor..self.lines.len())
                .rev()
                .find(in_file)
                .unwrap_or(selection.anchor)
        } else {
            (0..=selection.anchor)
                .find(in_file)
                .unwrap_or(selection.anchor)
        };
        Selection {
            anchor: selection.anchor,
            head,
        }
    }

    /// The picked rows.
    fn picked(&self) -> Option<Vec<Row>> {
        let selection = self.selection?;
        Some(
            self.lines[selection.first()..=selection.last()]
                .iter()
                .map(|shown| self.row(*shown).clone())
                .collect(),
        )
    }

    /// The picked lines as the file reads them: no signs, no numbers.
    pub(crate) fn picked_text(&self) -> Option<String> {
        let rows = self.picked()?;
        Some(
            rows.iter()
                .filter(|row| row.is_content())
                .map(Row::text)
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    /// Where row `line` sits: its file's path, as git names it, and its
    /// number there (the old one for a removed line).
    pub(crate) fn place(&self, line: usize) -> Option<(&str, u32)> {
        let shown = *self.lines.get(line)?;
        let row = self.row(shown);
        let number = row.new_no().or_else(|| row.old_no())?;
        Some((self.files()[shown.file].diff.path.as_str(), number))
    }

    /// Where comments made now were read: a commit's own lines, else the
    /// worktree's.
    fn rev(&self) -> Rev {
        match (&self.commit, self.scope, &self.file) {
            (Some(commit), Scope::Commits, None) => Rev::Commit(CommitPick::single(&commit.sha)),
            _ => Rev::Worktree,
        }
    }

    /// Whether `comment` belongs in what the slot shows: a diff comment in
    /// the diff it was made on, a file comment in its file.
    fn in_view(&self, comment: &ReviewComment) -> bool {
        comment.diff_anchored == self.file.is_none() && comment.rev == self.rev()
    }

    fn step(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        if let Some(search) = self.search.as_mut() {
            let last = search.rows().len().saturating_sub(1);
            search.pick = match down {
                true => (search.pick + 1).min(last),
                false => search.pick.saturating_sub(1),
            };
            cx.notify();
            return;
        }
        if self.pr_open {
            return;
        }
        if self.listing_commits() {
            let last = self
                .commits
                .as_ref()
                .map_or(0, |commits| commits.len().saturating_sub(1));
            self.commit_pick = match down {
                true => (self.commit_pick + 1).min(last),
                false => self.commit_pick.saturating_sub(1),
            };
            self.scroll.scroll_to_item(self.commit_pick);
            cx.notify();
            return;
        }
        let Some(selection) = self.selection else {
            self.pick(0, false, cx);
            return;
        };
        let line = if down {
            selection.head + 1
        } else {
            selection.head.saturating_sub(1)
        };
        self.pick(line.min(self.lines.len().saturating_sub(1)), extend, cx);
    }

    /// Open the fold at row `line`: its hidden lines take its place.
    fn expand(&mut self, line: usize, cx: &mut Context<Self>) {
        let Some(shown) = self.lines.get(line).copied() else {
            return;
        };
        let Some(files) = self.files_mut() else {
            return;
        };
        let rows = &mut files[shown.file].diff.rows;
        let Row::Fold { lines } = rows[shown.row].clone() else {
            return;
        };
        rows.splice(shown.row..=shown.row, lines);
        self.rebuild();
        self.selection = Some(Selection::at(line));
        cx.notify();
    }

    /// `c` or ⏎: review the listed commit, open the fold, or open the
    /// comment field under the pick.
    pub(crate) fn open_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.listing_commits() {
            self.open_commit(self.commit_pick, cx);
            return;
        }
        let Some(selection) = self.selection else {
            return;
        };
        if selection.first() == selection.last()
            && !self.row(self.lines[selection.head]).is_content()
        {
            self.expand(selection.head, cx);
            return;
        }
        let editor = cx.new(Composer::new);
        editor.update(cx, |editor, cx| {
            editor.set_role(Role::Field(Some("ReviewComment")), cx)
        });
        window.focus(&editor.focus_handle(cx), cx);
        self.editor = Some(editor);
        cx.notify();
    }

    fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// ⏎ in the field: the comment joins the Composer's shelf. Blank words
    /// add nothing; the field closes either way.
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(editor), Some(selection), Some(rows)) =
            (&self.editor, self.selection, self.picked())
        else {
            return;
        };
        let text = editor.read(cx).text().trim().to_string();
        let file = self.files()[self.lines[selection.first()].file]
            .diff
            .path
            .clone();
        if let (false, Some((side, start, end, lines))) = (text.is_empty(), reviewr::anchor(&rows))
        {
            let comment = ReviewComment {
                file,
                side,
                start,
                end,
                lines,
                text,
                diff_anchored: self.file.is_none(),
                rev: self.rev(),
            };
            self.composer
                .update(cx, |composer, cx| composer.add_review_comment(comment, cx));
        }
        self.close_editor(window, cx);
    }

    /// The comments hanging under row `line`, with their ids.
    fn comments_at(&self, line: usize, cx: &gpui::App) -> Vec<(u64, ReviewComment)> {
        let shown = self.lines[line];
        let (path, row) = (&self.files()[shown.file].diff.path, self.row(shown));
        let comments = self.composer.read(cx).review_comments();
        comments
            .iter()
            .enumerate()
            .filter(|(_, comment)| {
                comment.file == *path && self.in_view(comment) && reviewr::ends_at(comment, row)
            })
            .filter_map(|(index, comment)| Some((comments.id(index)?, comment.clone())))
            .collect()
    }

    /// ⌫: the latest comment hanging under the pick's last row leaves the
    /// Composer's shelf.
    fn uncomment(&mut self, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else {
            return;
        };
        if let Some((id, _)) = self.comments_at(selection.last(), cx).pop() {
            self.composer
                .update(cx, |composer, cx| composer.remove_review_comment(id, cx));
        }
    }

    /// `/`: the search field over the slot, holding the keyboard.
    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let field = match &self.search {
            Some(search) => search.field.clone(),
            None => {
                let field = cx.new(|cx| {
                    let mut line = Composer::new(cx);
                    line.set_role(Role::Field(Some("ReviewSearch")), cx);
                    line.set_visible_row_limit(1, cx);
                    line
                });
                let edited = cx.subscribe(&field, |view, field, _: &Edited, cx| {
                    let query = field.read(cx).text().to_string();
                    cx.emit(Searched(query));
                    if let Some(search) = view.search.as_mut() {
                        search.pick = 0;
                    }
                });
                self.search = Some(SearchPanel {
                    field: field.clone(),
                    results: None,
                    pick: 0,
                    _edited: edited,
                });
                // An empty query lists the files the engine ranks first.
                cx.emit(Searched(String::new()));
                field
            }
        };
        window.focus(&field.focus_handle(cx), cx);
        cx.notify();
    }

    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// ⏎ in the search, or a click on a result: its file, whole.
    fn open_found(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(found) = self
            .search
            .as_ref()
            .and_then(|search| search.rows().get(index).cloned())
        else {
            return;
        };
        self.close_search(window, cx);
        self.open_file(found.path, found.line, cx);
    }

    /// `m`: Markdown files rendered, or back to their rows.
    fn toggle_markdown(&mut self, cx: &mut Context<Self>) {
        self.markdown = !self.markdown;
        self.editor = None;
        self.rebuild();
        cx.notify();
    }

    /// ⌘F: the find bar over the rows (or the keyboard back to it).
    fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.is_none() {
            self.find = Some(crate::find::Find::new(cx, |view: &mut Self, cx| {
                view.refind(cx)
            }));
        }
        if let Some(find) = &self.find {
            find.focus(window, cx);
        }
        cx.notify();
    }

    /// Search the rows again for what the bar reads.
    fn refind(&mut self, cx: &mut Context<Self>) {
        let texts: Vec<String> = self
            .lines
            .iter()
            .map(|shown| self.row(*shown).text())
            .collect();
        let from = self.selection.map_or(0, |picked| picked.head);
        let Some(find) = self.find.as_mut() else {
            return;
        };
        find.search(texts.iter().map(String::as_str), from, cx);
        let hit = find.active().copied();
        if let Some(hit) = hit {
            self.scroll_to(hit.unit);
        }
        cx.notify();
    }

    fn step_find(&mut self, forward: bool, cx: &mut Context<Self>) {
        let hit = self
            .find
            .as_mut()
            .and_then(|find| find.step(forward).copied());
        if let Some(hit) = hit {
            self.scroll_to(hit.unit);
            cx.notify();
        }
    }

    fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Row `at`'s highlights, with the find's occurrences washed in.
    fn highlights_at(&self, at: usize, text: &str) -> Vec<(Range<usize>, HighlightStyle)> {
        let runs = self.highlights[at].clone();
        let Some(find) = &self.find else {
            return runs;
        };
        let active = find.active().filter(|hit| hit.unit == at);
        let washes: Vec<(Range<usize>, u32)> = find
            .query()
            .matches(text)
            .into_iter()
            .enumerate()
            .map(|(ordinal, range)| {
                let strong = active.is_some_and(|hit| hit.ordinal == ordinal);
                (range, if strong { FIND_ACTIVE_WASH } else { FIND_WASH })
            })
            .collect();
        crate::find::washed(runs, &washes)
    }

    /// A file's head: its path (a rename's `old → new`), and what it adds
    /// up to.
    fn file_head(&self, file: &ReviewFile, first: bool) -> AnyElement {
        let path = match &file.diff.previous_path {
            Some(previous) => format!("{previous} \u{2192} {}", file.diff.path),
            None => file.diff.path.clone(),
        };
        div()
            .flex()
            .items_center()
            .h(px(self.grid.line))
            .mt(px(if first { 0. } else { self.grid.half() }))
            .px(px(self.grid.cell()))
            .whitespace_nowrap()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(W_LABEL)
                    .text_color(rgb(TEXT_STRONG))
                    .child(SharedString::from(path)),
            )
            .when(self.file.is_none(), |head| {
                head.child(crate::components::tabular(
                    div().flex_shrink_0().pl(px(2. * self.grid.cell())).child(
                        crate::pane::trail_diff_stat(
                            file.additions as usize,
                            file.deletions as usize,
                        ),
                    ),
                ))
            })
            .into_any_element()
    }

    /// One row, with whatever hangs off it: the file's head over its first
    /// row, and under it the comments that end on it and the open field.
    fn line_row(&self, at: usize, cx: &mut Context<Self>) -> AnyElement {
        let shown = self.lines[at];
        let file = &self.files()[shown.file];
        let row = self.row(shown);
        let first_of_file = at == 0 || self.lines[at - 1].file != shown.file;
        let picked = self.selection.is_some_and(|picked| picked.contains(at));
        let last_picked = self.selection.is_some_and(|picked| picked.last() == at);
        let (cell, line_h, number_w) = (self.grid.cell(), self.grid.line, self.number_w());
        let indent = 2. * number_w + 2. * cell;
        let mut stack = div().flex().flex_col().w_full();
        if first_of_file {
            stack = stack.child(self.file_head(file, shown.file == 0));
        }
        if let Row::Fold { lines } = row {
            let hidden = lines.len();
            stack = stack.child(
                div()
                    .id(("review-line", at))
                    .debug_selector(move || format!("review-line-{at}"))
                    .flex()
                    .items_center()
                    .h(px(line_h))
                    .pl(px(indent))
                    .cursor_pointer()
                    .text_color(rgb(TEXT_MUTED))
                    .when(picked, |row| row.bg(paint::SELECTION))
                    .child(SharedString::from(format!(
                        "\u{22ef} {hidden} line{}",
                        if hidden == 1 { "" } else { "s" }
                    )))
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        window.focus(&view.focus, cx);
                        view.expand(at, cx);
                    })),
            );
            return stack.into_any_element();
        }
        let ink = DiffKind::of(&row.marker().to_string()).paint();
        let text = row.text();
        let highlights = self.highlights_at(at, &text);
        let number = |number: Option<u32>| {
            crate::components::tabular(
                div()
                    .flex_shrink_0()
                    .w(px(number_w))
                    .pr(px(cell))
                    .text_right()
                    .text_color(rgb(if picked { ACCENT } else { TEXT_MUTED }))
                    .children(number.map(|number| SharedString::from(number.to_string()))),
            )
        };
        // A removed row has no new number; a context row shows both.
        let old = match row {
            Row::Insertion { .. } => None,
            _ => row.old_no(),
        };
        let line = div()
            .id(("review-line", at))
            .debug_selector(move || format!("review-line-{at}"))
            .flex()
            .w_full()
            .h(px(line_h))
            .whitespace_nowrap()
            .cursor_pointer()
            .map(|line| match (picked, ink.wash) {
                (true, _) => line.bg(paint::SELECTION),
                (false, Some(wash)) => line.bg(rgba(wash)),
                (false, None) => line,
            })
            .child(number(old))
            .child(number(row.new_no()))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(2. * cell))
                    .text_color(rgb(ink.sign_color))
                    .child(ink.sign),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(ink.code_color))
                    .child(StyledText::new(SharedString::from(text)).with_highlights(highlights)),
            )
            .when(last_picked && self.editor.is_none(), |line| {
                line.child(
                    div()
                        .id(("review-comment-on", at))
                        .debug_selector(|| "review-add-comment".into())
                        .flex_shrink_0()
                        .px(px(cell))
                        .text_color(rgb(ACCENT))
                        .hover_control(format!("review-comment-on-{at}"))
                        .child("+ comment")
                        // On the press, before the row's own press re-picks one line.
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                view.open_editor(window, cx);
                            }),
                        ),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                    window.focus(&view.focus, cx);
                    view.pick(at, event.modifiers.shift, cx);
                    view.dragging = true;
                }),
            )
            .on_mouse_move(cx.listener(move |view, event: &MouseMoveEvent, _, cx| {
                if view.dragging && event.dragging() {
                    view.pick(at, true, cx);
                }
            }))
            // A right-click keeps a pick that holds the line, else picks it.
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.focus(&view.focus, cx);
                    if !view.selection.is_some_and(|picked| picked.contains(at)) {
                        view.pick(at, false, cx);
                    }
                    cx.emit(MenuAsked {
                        at: event.position,
                        line: at,
                    });
                }),
            );
        stack = stack.child(line);
        for (id, comment) in self.comments_at(at, cx) {
            stack = stack.child(self.comment_row(id, &comment, cx));
        }
        if let Some(editor) = self.editor.as_ref().filter(|_| last_picked) {
            stack = stack.child(
                div()
                    .debug_selector(|| "review-comment-field".into())
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .py(px(self.grid.half() / 2.))
                    .pl(px(indent))
                    .pr(px(cell))
                    .bg(paint::BAND2)
                    .child(div().min_w_0().flex_1().child(editor.clone()))
                    .child(
                        div()
                            .id("review-comment-save")
                            .debug_selector(|| "review-comment-save".into())
                            .flex_shrink_0()
                            .text_color(rgb(TEXT_MUTED))
                            .hover_control("review-comment-save")
                            .child("\u{21b5} add")
                            .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                view.save(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("review-comment-cancel")
                            .flex_shrink_0()
                            .text_color(rgb(TEXT_MUTED))
                            .hover_control("review-comment-cancel")
                            .child("esc cancel")
                            .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                view.close_editor(window, cx);
                            })),
                    ),
            );
        }
        stack.into_any_element()
    }

    /// A comment under the line it ends on: its words, quiet, with a `×`
    /// that takes it, by `id`, off the Composer's shelf.
    fn comment_row(&self, id: u64, comment: &ReviewComment, cx: &mut Context<Self>) -> AnyElement {
        let cell = self.grid.cell();
        let composer = self.composer.clone();
        div()
            .debug_selector(|| "review-comment".into())
            .flex()
            .items_center()
            .min_h(px(self.grid.line))
            .pl(px(2. * self.number_w() + 2. * cell))
            .pr(px(cell))
            .bg(paint::BAND2)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(rgb(TEXT))
                    .child(SharedString::from(comment.text.clone())),
            )
            .child(
                div()
                    .id(("review-uncomment", id as usize))
                    .flex_shrink_0()
                    .px(px(cell))
                    .text_color(rgb(TEXT_MUTED))
                    .cursor_pointer()
                    .child("\u{d7}")
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        composer.update(cx, |composer, cx| composer.remove_review_comment(id, cx));
                    })),
            )
            .into_any_element()
    }

    /// A file whose rows the vendored model declined: its head and why.
    fn notice_block(&self, index: usize) -> AnyElement {
        let file = &self.files()[index];
        let message = file
            .diff
            .notice
            .map(|notice| notice.message())
            .unwrap_or_default();
        div()
            .flex()
            .flex_col()
            .child(self.file_head(file, index == 0))
            .child(
                div()
                    .h(px(self.grid.line))
                    .pl(px(2. * self.number_w() + 2. * self.grid.cell()))
                    .text_color(rgb(TEXT_MUTED))
                    .child(message),
            )
            .into_any_element()
    }

    /// A Markdown file's new side, rendered (`m`).
    fn markdown_block(&self, index: usize) -> AnyElement {
        let file = &self.files()[index];
        div()
            .debug_selector(move || format!("review-markdown-{index}"))
            .flex()
            .flex_col()
            .child(self.file_head(file, index == 0))
            .child(
                div()
                    .px(px(TX_PAD_L))
                    .pb(px(self.grid.half()))
                    .font_family(FONT_UI)
                    .text_size(px(FS_PROSE))
                    .line_height(px(LH_PROSE))
                    .child(crate::rich::Markdown::new(
                        format!("review-markdown-{}", file.diff.path),
                        file.new.clone().unwrap_or_default(),
                        self.rich.clone(),
                    )),
            )
            .into_any_element()
    }

    /// A listed commit: its short id, subject and author; a click reviews it.
    fn commit_row(&self, index: usize, commit: &CommitRow, cx: &mut Context<Self>) -> AnyElement {
        let cell = self.grid.cell();
        div()
            .id(("review-commit", index))
            .debug_selector(move || format!("review-commit-{index}"))
            .flex()
            .gap(px(2. * cell))
            .h(px(self.grid.line))
            .px(px(cell))
            .whitespace_nowrap()
            .cursor_pointer()
            .when(index == self.commit_pick, |row| row.bg(paint::SELECTION))
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(rgb(TEXT_MUTED))
                    .child(SharedString::from(abbreviate_oid(&commit.sha))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(TEXT))
                    .child(SharedString::from(commit.subject.clone())),
            )
            .when(!commit.author.is_empty(), |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .text_color(rgb(TEXT_MUTED))
                        .child(SharedString::from(commit.author.clone())),
                )
            })
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                window.focus(&view.focus, cx);
                view.open_commit(index, cx);
            }))
            .into_any_element()
    }

    /// The rows list: each file's head and rows, notices and Markdown.
    fn rows_list(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("review-lines")
            .debug_selector(|| "review-lines".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .font_family(FONT_CODE)
            .text_size(px(self.grid.size))
            .line_height(px(self.grid.line))
            .children(self.children.clone().into_iter().map(|child| match child {
                Child::Row(at) => self.line_row(at, cx),
                Child::Notice(file) => self.notice_block(file),
                Child::Markdown(file) => self.markdown_block(file),
            }))
            .into_any_element()
    }

    /// The pull request: its head, its checks and its conversation.
    fn pr_body(&self) -> AnyElement {
        let Some(pr) = &self.pr else {
            return crate::menu::note("reading\u{2026}").into_any_element();
        };
        let pr = match &pr.pr {
            Ok(pr) => pr,
            Err(note) => {
                return div()
                    .debug_selector(|| "review-pr-note".into())
                    .child(crate::menu::note(note.clone()))
                    .into_any_element()
            }
        };
        let cell = self.grid.cell();
        let muted = |text: String| {
            div()
                .text_color(rgb(TEXT_MUTED))
                .child(SharedString::from(text))
        };
        let mut body = div()
            .id("review-pr")
            .debug_selector(|| "review-pr".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(cell))
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .child(
                div()
                    .font_weight(W_LABEL)
                    .text_color(rgb(TEXT_STRONG))
                    .child(SharedString::from(format!("#{} {}", pr.number, pr.title))),
            )
            .child(muted(pr_facts(pr)))
            .child(muted(pr.url.clone()));
        if !pr.body.is_empty() {
            body = body.child(
                div()
                    .mt(px(self.grid.half()))
                    .text_color(rgb(TEXT))
                    .child(SharedString::from(pr.body.clone())),
            );
        }
        if !pr.checks.is_empty() {
            body = body.child(div().mt(px(self.grid.half())).child(muted("checks".into())));
            for check in &pr.checks {
                let (glyph, color) = match check.status {
                    WireCheckStatus::Success => ("\u{2713}", GREEN),
                    WireCheckStatus::Failure => ("\u{2717}", RED),
                    WireCheckStatus::Running | WireCheckStatus::Pending => ("\u{25cb}", YELLOW),
                    WireCheckStatus::Skipped | WireCheckStatus::Unknown => ("\u{2013}", TEXT_MUTED),
                };
                body = body.child(
                    div()
                        .flex()
                        .gap(px(cell))
                        .child(div().text_color(rgb(color)).child(glyph))
                        .child(
                            div()
                                .text_color(rgb(TEXT))
                                .child(SharedString::from(check.name.clone())),
                        ),
                );
            }
        }
        for (index, comment) in pr.comments.iter().enumerate() {
            let kind = match comment.kind {
                WirePrCommentKind::Review => "review",
                WirePrCommentKind::Finding => "finding",
                WirePrCommentKind::Comment | WirePrCommentKind::Unknown => "comment",
            };
            let mut state = Vec::new();
            if comment.resolved {
                state.push("resolved");
            }
            if comment.outdated {
                state.push("outdated");
            }
            let mut card = div()
                .debug_selector(move || format!("review-pr-comment-{index}"))
                .mt(px(self.grid.half()))
                .p(px(cell))
                .bg(paint::BAND2)
                .child(muted(format!(
                    "{} \u{b7} {kind} \u{b7} {}{}",
                    comment.author,
                    comment.anchor,
                    if state.is_empty() {
                        String::new()
                    } else {
                        format!(" \u{b7} {}", state.join(", "))
                    }
                )))
                .child(
                    div()
                        .text_color(rgb(TEXT))
                        .child(SharedString::from(comment.body.clone())),
                );
            for reply in &comment.replies {
                card = card.child(
                    div()
                        .pl(px(2. * cell))
                        .child(muted(reply.author.clone()))
                        .child(
                            div()
                                .text_color(rgb(TEXT))
                                .child(SharedString::from(reply.body.clone())),
                        ),
                );
            }
            body = body.child(card);
        }
        if pr.truncated {
            body = body.child(muted("more on the forge".into()));
        }
        body.into_any_element()
    }

    /// The search over the slot: its field, then the results.
    fn search_body(&self, search: &SearchPanel, cx: &mut Context<Self>) -> AnyElement {
        let cell = self.grid.cell();
        let results: AnyElement = match &search.results {
            None => crate::menu::note("searching\u{2026}").into_any_element(),
            Some(Err(error)) => crate::menu::note(error.clone()).into_any_element(),
            Some(Ok(_)) => {
                let rows = search.rows();
                if rows.is_empty() {
                    crate::menu::note("nothing found").into_any_element()
                } else {
                    div()
                        .id("review-found")
                        .debug_selector(|| "review-found".into())
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .font_family(FONT_CODE)
                        .text_size(px(self.grid.size))
                        .line_height(px(self.grid.line))
                        .children(rows.into_iter().enumerate().map(|(index, found)| {
                            div()
                                .id(("review-found", index))
                                .debug_selector(move || format!("review-found-{index}"))
                                .flex()
                                .gap(px(2. * cell))
                                .h(px(self.grid.line))
                                .px(px(cell))
                                .whitespace_nowrap()
                                .cursor_pointer()
                                .when(index == search.pick, |row| row.bg(paint::SELECTION))
                                .child(div().flex_shrink_0().text_color(rgb(PATH_INK)).child(
                                    SharedString::from(match found.line {
                                        Some(line) => format!("{}:{line}", found.path),
                                        None => found.path.clone(),
                                    }),
                                ))
                                .children(found.text.map(|text| {
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_color(rgb(TEXT))
                                        .child(SharedString::from(text))
                                }))
                                .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                                    view.open_found(index, window, cx);
                                }))
                        }))
                        .into_any_element()
                }
            }
        };
        div()
            .debug_selector(|| "review-search".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(FLOAT_ROW_H))
                    .px(px(CH))
                    .gap(px(CH))
                    .child(crate::components::prompt_mark(ACCENT))
                    .child(div().flex_1().min_w_0().child(search.field.clone())),
            )
            .child(results)
            .into_any_element()
    }

    /// What the slot shows under the bar.
    fn body(&self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(search) = &self.search {
            return self.search_body(search, cx);
        }
        if let Some(error) = &self.error {
            let what = if self.pr_open {
                "the pull request".to_string()
            } else if let Some(file) = &self.file {
                file.path.clone()
            } else {
                self.scope.label().to_string()
            };
            return crate::menu::note(format!("can\u{2019}t read {what} \u{b7} {error}"))
                .into_any_element();
        }
        if self.pr_open {
            return self.pr_body();
        }
        if let Some(file) = &self.file {
            return match &file.file {
                None => crate::menu::note("reading\u{2026}").into_any_element(),
                Some(_) => self.rows_list(cx),
            };
        }
        if self.listing_commits() {
            return match &self.commits {
                None => crate::menu::note("reading\u{2026}").into_any_element(),
                Some(commits) if commits.is_empty() => {
                    crate::menu::note("no commits on this branch").into_any_element()
                }
                Some(commits) => div()
                    .id("review-commits")
                    .debug_selector(|| "review-commits".into())
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .font_family(FONT_CODE)
                    .text_size(px(self.grid.size))
                    .line_height(px(self.grid.line))
                    .children(
                        commits
                            .iter()
                            .enumerate()
                            .map(|(index, commit)| self.commit_row(index, commit, cx)),
                    )
                    .into_any_element(),
            };
        }
        match &self.diff {
            None => crate::menu::note("reading\u{2026}").into_any_element(),
            Some(files) if files.is_empty() => {
                crate::menu::note(format!("nothing changed \u{b7} {}", self.scope.label()))
                    .into_any_element()
            }
            Some(_) => self.rows_list(cx),
        }
    }

    /// The bar: the scopes and `PR` as tokens, then what is shown and what
    /// the diff adds up to.
    fn bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut bar = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(CH))
            .h(px(FLOAT_ROW_H))
            .px(px(CH))
            .whitespace_nowrap();
        let mut scope = Scope::Uncommitted;
        for _ in 0..4 {
            let this = scope;
            bar = bar.child(
                crate::menu::token(
                    SharedString::from(format!("review-scope-{}", this.name())),
                    this.label(),
                    this == self.scope && self.showing_diff(),
                )
                .debug_selector(move || format!("review-scope-{}", this.name()))
                .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                    window.focus(&view.focus, cx);
                    view.pick_scope(this, cx);
                })),
            );
            scope = scope.cycle();
        }
        bar = bar.child(
            crate::menu::token("review-pr-token", "PR", self.pr_open)
                .debug_selector(|| "review-pr-token".into())
                .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                    window.focus(&view.focus, cx);
                    view.open_pr(cx);
                })),
        );
        let shown = match (&self.file, &self.commit) {
            (Some(file), _) => Some(("review-file-shown", file.path.clone())),
            (None, Some(commit)) if self.showing_diff() && self.scope == Scope::Commits => Some((
                "review-commit-reviewed",
                format!("{} {}", abbreviate_oid(&commit.sha), commit.subject),
            )),
            _ => None,
        };
        bar = bar
            .children(shown.filter(|_| !self.pr_open).map(|(selector, text)| {
                div()
                    .debug_selector(move || selector.into())
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(TEXT_MUTED))
                    .child(SharedString::from(text))
            }))
            .child(div().flex_1())
            .children(
                self.diff
                    .as_ref()
                    .filter(|_| self.showing_diff())
                    .map(|files| {
                        div()
                            .flex_shrink_0()
                            .text_color(rgb(TEXT_MUTED))
                            .child(stat(files))
                    }),
            );
        bar.into_any_element()
    }
}

/// `3 files +40 −12`: what a diff adds up to.
fn stat(files: &[ReviewFile]) -> String {
    let added: u32 = files.iter().map(|file| file.additions).sum();
    let removed: u32 = files.iter().map(|file| file.deletions).sum();
    let count = files.len();
    format!(
        "{count} file{} +{added} \u{2212}{removed}",
        if count == 1 { "" } else { "s" }
    )
}

/// A pull request's facts on one line: state, branches, mergeability and
/// how the checkout stands against it.
fn pr_facts(pr: &WirePr) -> String {
    let state = match (pr.state, pr.draft) {
        (WirePrState::Open, true) => "draft",
        (WirePrState::Open, false) => "open",
        (WirePrState::Merged, _) => "merged",
        (WirePrState::Closed, _) => "closed",
        (WirePrState::Unknown, _) => "unknown",
    };
    let merge = match pr.merge {
        WireMerge::Conflicting => " \u{b7} conflicts",
        WireMerge::Blocked => " \u{b7} blocked",
        WireMerge::Clean | WireMerge::Unknown => "",
    };
    let sync = match pr.sync {
        WireSync::Unpushed { commits } => format!(" \u{b7} {commits} unpushed"),
        WireSync::Behind { commits } => format!(" \u{b7} {commits} behind"),
        WireSync::InSync | WireSync::Unknown => String::new(),
    };
    format!(
        "{state} \u{b7} {} \u{2192} {}{merge}{sync}",
        pr.head, pr.base
    )
}

/// A row's syntax colours (Ferrite's lexer, for the path's language), with
/// the words the vendored model found changed washed deeper.
fn row_highlights(
    row: &Row,
    language: Option<&'static str>,
) -> Vec<(Range<usize>, HighlightStyle)> {
    if !row.is_content() {
        return Vec::new();
    }
    let text = row.text();
    let tokens = ferrite_core::transcript::highlight_tokens(language, &text);
    let runs = crate::pane::code(&text, Some(tokens.as_slice()));
    let word = match row {
        Row::Insertion { .. } => DIFF_ADDED_WORD,
        Row::Deletion { .. } => DIFF_REMOVED_WORD,
        _ => return runs,
    };
    // The model counts chars; the text's highlights count bytes.
    let byte = |char_at: u32| {
        text.char_indices()
            .nth(char_at as usize)
            .map_or(text.len(), |(at, _)| at)
    };
    let washes: Vec<(Range<usize>, u32)> = row
        .emphasis()
        .iter()
        .map(|&(start, end)| (byte(start)..byte(end), word))
        .collect();
    crate::find::washed(runs, &washes)
}

impl Focusable for ReviewView {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ReviewView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let footer: &[(&str, &str)] = if self.search.is_some() {
            &SEARCH_FOOTER
        } else if self.listing_commits() {
            &COMMITS_FOOTER
        } else if self.pr_open || self.file.is_some() {
            &BACK_FOOTER
        } else {
            &FOOTER
        };
        let body = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(self.body(cx))
            .children(
                self.find
                    .as_ref()
                    .filter(|_| self.search.is_none() && !self.pr_open)
                    .map(|find| find.bar()),
            );
        div()
            .debug_selector(|| "review".into())
            .track_focus(&self.focus)
            .key_context("Review")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _: &MouseUpEvent, _, _| view.dragging = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, _: &MouseUpEvent, _, _| view.dragging = false),
            )
            .on_action(cx.listener(|view, _: &Next, _, cx| view.step(true, false, cx)))
            .on_action(cx.listener(|view, _: &Previous, _, cx| view.step(false, false, cx)))
            .on_action(cx.listener(|view, _: &ExtendNext, _, cx| view.step(true, true, cx)))
            .on_action(cx.listener(|view, _: &ExtendPrevious, _, cx| view.step(false, true, cx)))
            .on_action(cx.listener(|view, _: &Comment, window, cx| view.open_editor(window, cx)))
            .on_action(cx.listener(|view, _: &Uncomment, _, cx| view.uncomment(cx)))
            .on_action(cx.listener(|view, _: &Reread, _, cx| view.reread(cx)))
            .on_action(
                cx.listener(|view, _: &NextScope, _, cx| view.pick_scope(view.scope.cycle(), cx)),
            )
            .on_action(cx.listener(|view, _: &Search, window, cx| view.open_search(window, cx)))
            .on_action(cx.listener(|view, _: &ToggleMarkdown, _, cx| view.toggle_markdown(cx)))
            .on_action(cx.listener(|view, _: &ShowPullRequest, _, cx| view.open_pr(cx)))
            // Back from the pull request or a file; at the diff, the slot
            // closes (the reader's own handler, above).
            .on_action(cx.listener(|view, _: &Close, _, cx| {
                if !view.back(cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|view, _: &SaveComment, window, cx| view.save(window, cx)))
            .on_action(
                cx.listener(|view, _: &CancelComment, window, cx| view.close_editor(window, cx)),
            )
            .on_action(cx.listener(|view, _: &OpenFound, window, cx| {
                let pick = view.search.as_ref().map_or(0, |search| search.pick);
                view.open_found(pick, window, cx)
            }))
            .on_action(
                cx.listener(|view, _: &CloseSearch, window, cx| view.close_search(window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &crate::find::Open, window, cx| view.open_find(window, cx)),
            )
            .on_action(cx.listener(|view, _: &crate::find::Next, _, cx| view.step_find(true, cx)))
            .on_action(
                cx.listener(|view, _: &crate::find::Previous, _, cx| view.step_find(false, cx)),
            )
            .on_action(
                cx.listener(|view, _: &crate::find::Close, window, cx| view.close_find(window, cx)),
            )
            .child(self.bar(cx))
            .child(body)
            .child(crate::menu::footer(footer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_reviewr::model::{ChangeKind, ChangedFile};
    use gpui::TestAppContext;

    fn sides(path: &str, old: &str, new: &str) -> FileSides {
        FileSides {
            file: ChangedFile {
                path: path.into(),
                kind: ChangeKind::Modified,
                additions: 1,
                deletions: 1,
                previous_path: None,
                binary: false,
                old_size: 0,
                new_size: None,
            },
            sides: Ok((old.into(), new.into())),
        }
    }

    fn files() -> Vec<FileSides> {
        vec![sides("src/a.rs", "let a = 1;\na\n", "let a = 2;\na\n")]
    }

    /// A Review in a window of its own, showing `files` and holding the
    /// keyboard, the production keys bound.
    fn open(
        files: Vec<FileSides>,
        cx: &mut TestAppContext,
    ) -> (Entity<ReviewView>, gpui::VisualTestContext) {
        cx.update(|cx| {
            crate::theme::init_components(cx);
            let table = crate::keymap::bindings(crate::keymap::Platform::Mac);
            let bindings = crate::load_bindings(table, cx);
            cx.bind_keys(bindings);
        });
        let window = cx.add_window(|_, cx| {
            let composer = cx.new(Composer::new);
            let review = cx.new(|cx| {
                ReviewView::new(
                    composer,
                    Scope::Uncommitted,
                    ferrite_core::settings::ReadingSize::STANDARD,
                    cx,
                )
            });
            Host(review)
        });
        let mut cx = gpui::VisualTestContext::from_window(*window, cx);
        let review = window.update(&mut cx, |host, _, _| host.0.clone()).unwrap();
        review.update_in(&mut cx, |review, window, cx| {
            review.show(Scope::Uncommitted, files, cx);
            window.focus(&review.focus, cx);
        });
        cx.run_until_parked();
        (review, cx)
    }

    /// The keys walk and stretch the pick, `c` opens the field, ⏎ there
    /// (or its `↵ add`) puts the comment on the Composer's shelf, the next
    /// send carries it after the prompt's own words as herdr-reviewr
    /// formats it, and ⌫ takes comments back one at a time.
    #[gpui::test]
    fn a_comment_from_the_keys_rides_the_next_prompt(cx: &mut TestAppContext) {
        let (review, mut cx) = open(files(), cx);
        let cx = &mut cx;
        cx.simulate_keystrokes("down shift-down c");
        cx.simulate_input("why two?");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        review.read_with(cx, |review, cx| {
            assert!(review.editor.is_none(), "the field closes on ⏎");
            let composer = review.composer.read(cx);
            assert_eq!(composer.attachment_count(), 1);
            let comment = composer.review_comments().get(0).expect("one comment");
            assert_eq!(comment.location(), "src/a.rs:1-2");
            assert_eq!(comment.lines, "+let a = 2;\n a");
            assert_eq!(comment.text, "why two?");
            assert_eq!(composer.prompt(), "src/a.rs:1-2\n+let a = 2;\n a\nwhy two?");
        });
        // A twin by the mouse: the field's `↵ add` saves it too, and ⌫
        // takes back only the latest under the pick.
        cx.simulate_keystrokes("c");
        cx.simulate_input("why two?");
        let add = cx
            .debug_bounds("review-comment-save")
            .expect("the field's add");
        cx.simulate_click(add.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        let count = |cx: &mut gpui::VisualTestContext| {
            review.read_with(cx, |review, cx| {
                review.composer.read(cx).review_comments().len()
            })
        };
        assert_eq!(count(cx), 2);
        cx.simulate_keystrokes("backspace");
        assert_eq!(count(cx), 1);
        cx.simulate_keystrokes("backspace backspace");
        assert_eq!(count(cx), 0);
        cx.simulate_keystrokes("tab");
        review.read_with(cx, |review, _| {
            assert_eq!(review.scope(), Scope::Branch, "⇥ steps the scope");
            assert!(review.diff.is_none(), "until the cockpit reads it");
        });
    }

    /// A press dragged over the rows stretches the pick, held inside the
    /// file it began in; a press alone picks one row again.
    #[gpui::test]
    fn a_drag_stretches_the_pick_within_its_file(cx: &mut TestAppContext) {
        let mut two = files();
        two.push(sides("src/b.rs", "let b = 1;\nb\n", "let b = 2;\nb\n"));
        let (review, mut cx) = open(two, cx);
        let cx = &mut cx;
        let line =
            |cx: &mut gpui::VisualTestContext, selector| cx.debug_bounds(selector).expect("drawn");
        let none = gpui::Modifiers::none();
        let (from, over) = (
            line(cx, "review-line-0").center(),
            line(cx, "review-line-1").center(),
        );
        let past = line(cx, "review-line-4").center();
        cx.simulate_mouse_down(from, MouseButton::Left, none);
        cx.simulate_mouse_move(over, MouseButton::Left, none);
        let picked = |cx: &mut gpui::VisualTestContext| review.read_with(cx, |r, _| r.selection);
        assert_eq!(picked(cx), Some(Selection { anchor: 0, head: 1 }));
        cx.simulate_mouse_move(past, MouseButton::Left, none);
        cx.simulate_mouse_up(past, MouseButton::Left, none);
        cx.run_until_parked();
        assert_eq!(
            picked(cx),
            Some(Selection { anchor: 0, head: 2 }),
            "held at the file's last row"
        );
        let comment = cx.debug_bounds("review-add-comment").expect("shown");
        assert!(line(cx, "review-line-2").contains(&comment.center()));
        cx.simulate_click(over, none);
        assert_eq!(picked(cx), Some(Selection::at(1)), "a press picks one row");
    }

    /// A long unchanged run folds; ⏎ on the fold opens it in place.
    #[gpui::test]
    fn a_fold_opens_in_place(cx: &mut TestAppContext) {
        let old: String = (1..=20).map(|n| format!("line {n}\n")).collect();
        let new = old.replace("line 20\n", "line twenty\n");
        let (review, mut cx) = open(vec![sides("notes.txt", &old, &new)], cx);
        let cx = &mut cx;
        let rows = |cx: &mut gpui::VisualTestContext| review.read_with(cx, |r, _| r.lines.len());
        let folded = rows(cx);
        assert!(folded < 21, "the unchanged head folds: {folded} rows");
        assert_eq!(
            review.read_with(cx, |r, _| r.selection),
            Some(Selection::at(1)),
            "the first line picked, past the fold"
        );
        cx.simulate_keystrokes("up enter");
        cx.run_until_parked();
        assert_eq!(rows(cx), 21, "every line, the change two rows");
    }

    /// `/` opens the search; its results open a file whole, whose lines
    /// take a file comment; esc goes back to the diff.
    #[gpui::test]
    fn a_found_file_opens_whole_and_takes_comments(cx: &mut TestAppContext) {
        let (review, mut cx) = open(files(), cx);
        let cx = &mut cx;
        cx.simulate_keystrokes("/");
        cx.run_until_parked();
        assert!(cx.debug_bounds("review-search").is_some());
        review.update(cx, |review, cx| {
            let results = SearchResults {
                files: vec![ferrite_reviewr::search::FileHit {
                    path: "README.md".into(),
                    spans: Vec::new(),
                }],
                ..SearchResults::default()
            };
            review.show_found("", Ok(results), cx);
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            review.read_with(cx, |r, _| r.file_open().map(str::to_string)),
            Some("README.md".into())
        );
        review.update(cx, |review, cx| {
            review.show_file("README.md", "# Ferrite\nreviews\n".into(), cx)
        });
        cx.run_until_parked();
        cx.simulate_keystrokes("down c");
        cx.simulate_input("say more");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        review.read_with(cx, |review, cx| {
            let comment = review.composer.read(cx).review_comments().get(0).cloned();
            let comment = comment.expect("a file comment");
            assert_eq!(comment.location(), "README.md:2");
            assert!(!comment.diff_anchored);
        });
        cx.simulate_keystrokes("m");
        cx.run_until_parked();
        assert!(cx.debug_bounds("review-markdown-0").is_some(), "rendered");
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(review.read_with(cx, |r, _| r.file_open().is_none()), "back");
    }

    /// `p` shows the pull request; a note stands in when there is none.
    #[gpui::test]
    fn the_pull_request_shows_in_the_slot(cx: &mut TestAppContext) {
        let (review, mut cx) = open(files(), cx);
        let cx = &mut cx;
        cx.simulate_keystrokes("p");
        cx.run_until_parked();
        assert!(review.read_with(cx, |r, _| r.pr_open()));
        review.update(cx, |review, cx| {
            review.show_pr(
                PullRequest {
                    noun: "PR".into(),
                    pr: Err("No PR yet. Ready to ship?".into()),
                },
                cx,
            )
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("review-pr-note").is_some());
        cx.simulate_keystrokes("escape");
        assert!(
            !review.read_with(cx, |r, _| r.pr_open()),
            "back to the diff"
        );
    }

    /// ⌘F finds in the rows and steps between occurrences.
    #[gpui::test]
    fn find_steps_through_the_rows(cx: &mut TestAppContext) {
        let (review, mut cx) = open(files(), cx);
        let cx = &mut cx;
        cx.simulate_keystrokes("cmd-f");
        cx.simulate_input("let");
        cx.run_until_parked();
        let count = |cx: &mut gpui::VisualTestContext| {
            review.read_with(cx, |r, _| r.find.as_ref().and_then(|find| find.count()))
        };
        assert_eq!(count(cx), Some("1/2".into()));
        cx.simulate_keystrokes("enter");
        assert_eq!(count(cx), Some("2/2".into()));
        cx.simulate_keystrokes("escape");
        assert!(review.read_with(cx, |r, _| r.find.is_none()));
    }

    struct Host(Entity<ReviewView>);

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().flex().child(self.0.clone())
        }
    }
}
