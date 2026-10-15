//! The Review slot's body (ADR 0021): a diff beside the Thread, scoped to
//! the uncommitted work, the branch against its base, one of the branch's
//! commits, or the last turn, whose lines take comments. The bar and footer are chrome; the diff reads
//! at the operator's reading size, like every diff in a transcript.
//!
//! The bar names the scopes (`menu::token`, the chosen one inverse) and
//! what the diff adds up to; under it, one row per diff line — numbered on
//! both sides, signed and washed like every diff Ferrite draws — with the
//! file's path over its first line and each hunk's section over its own.
//! A press picks a line, a shift-press or a drag stretches the pick to a
//! range in the same file; ↑↓ and ⇧↑↓ do the same from the keyboard. A
//! right-click asks the cockpit for the slot's context menu (`MenuAsked`):
//! comment, open the file at the line, copy the picked lines. `c` (or ⏎,
//! or the `+ comment` beside the pick) opens a one-line field under it; ⏎
//! (or its `add`) there adds the comment to the Pane's Composer, where it
//! waits on the shelf for the next send (`review::attach`). ⌫ (or a
//! comment's `×`) takes back the latest comment under the pick. The commits
//! scope first lists the branch's commits, newest first: ↑↓ or a click
//! picks one, ⏎ (or the click) reviews it, and the scope again goes back to
//! the list. The cockpit loads each scope's edits (`cockpit/beside.rs`)
//! and hands them over with `show`; they are a snapshot, read again on `r`
//! or on picking the scope again.

use ferrite_core::review::git::Commit;
use ferrite_core::review::{self, Line, Scope, Selection};
use ferrite_core::FileEdit;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, rgba, AnyElement, ClickEvent, Context, Entity, EventEmitter, FocusHandle,
    Focusable, HighlightStyle, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, ScrollHandle, SharedString, StyledText, Window,
};
use std::ops::Range;

use crate::composer::{Composer, Role};
use crate::pane::{DiffKind, Grid};
use crate::pointer::Pointer as _;
use crate::theme::*;

gpui::actions!(
    review,
    [
        // In the `Review` context: walk and stretch the pick, comment on
        // it or take a comment back, step or reread the scope, close the
        // slot.
        Next,
        Previous,
        ExtendNext,
        ExtendPrevious,
        Comment,
        Uncomment,
        NextScope,
        Reread,
        Close,
        // In the comment field's `ReviewComment` context.
        SaveComment,
        CancelComment,
    ]
);

/// The slot's keys, as its footer spells them.
const FOOTER: [(&str, &str); 7] = [
    ("\u{2191}\u{2193}", "line"),
    ("\u{21e7}\u{2191}\u{2193}", "range"),
    ("c", "comment"),
    ("\u{232b}", "uncomment"),
    ("\u{21e5}", "scope"),
    ("r", "reread"),
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

/// The operator picked another scope; the cockpit loads it and calls
/// `show` (`show_commits` for the commits scope).
pub(crate) struct ScopePicked(pub Scope);

/// The operator picked one of the listed commits, by its id; the cockpit
/// loads its diff and calls `show`.
pub(crate) struct CommitPicked(pub String);

/// The operator right-clicked diff line `line` (the pick holds it); the
/// cockpit opens the slot's context menu `at` the pointer.
pub(crate) struct MenuAsked {
    pub at: Point<Pixels>,
    pub line: usize,
}

pub(crate) struct ReviewView {
    /// The Pane's own prompt: where comments gather, and are read back to
    /// mark the lines they are about.
    composer: Entity<Composer>,
    /// The reading size's grid the diff rows sit on.
    grid: Grid,
    focus: FocusHandle,
    scroll: ScrollHandle,
    scope: Scope,
    /// `None` while the scope's edits are being read.
    edits: Option<Vec<FileEdit>>,
    /// Why the scope could not be read (git's own words).
    error: Option<String>,
    lines: Vec<Line>,
    /// Each line's syntax colours, worked out once per read.
    highlights: Vec<Vec<(Range<usize>, HighlightStyle)>>,
    selection: Option<Selection>,
    /// A press on a line is held: dragging over lines stretches the pick.
    dragging: bool,
    /// The comment being written under the pick.
    editor: Option<Entity<Composer>>,
    /// The commits scope's list; `None` while it is being read.
    commits: Option<Vec<Commit>>,
    /// The listed commit the arrows are on.
    commit_pick: usize,
    /// The commit under review; `None` while the list shows.
    commit: Option<Commit>,
}

impl EventEmitter<ScopePicked> for ReviewView {}
impl EventEmitter<CommitPicked> for ReviewView {}
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
            edits: None,
            error: None,
            lines: Vec::new(),
            highlights: Vec::new(),
            selection: None,
            dragging: false,
            editor: None,
            commits: None,
            commit_pick: 0,
            commit: None,
        }
    }

    pub(crate) fn scope(&self) -> Scope {
        self.scope
    }

    /// The id of the commit under review, if one is.
    pub(crate) fn commit(&self) -> Option<&str> {
        self.commit.as_ref().map(|commit| commit.id.as_str())
    }

    /// The commits scope's list has been read: show it, the newest picked.
    pub(crate) fn show_commits(&mut self, commits: Vec<Commit>, cx: &mut Context<Self>) {
        self.commits = Some(commits);
        self.commit_pick = 0;
        self.error = None;
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    /// Whether the commits scope lists commits rather than one's diff.
    fn listing_commits(&self) -> bool {
        self.scope == Scope::Commits && self.commit.is_none()
    }

    /// Review the listed commit at `index`: its diff when it is read.
    fn open_commit(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(commit) = self.commits.as_ref().and_then(|commits| commits.get(index)) else {
            return;
        };
        self.commit_pick = index;
        self.commit = Some(commit.clone());
        self.edits = None;
        self.error = None;
        cx.emit(CommitPicked(commit.id.clone()));
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

    /// The handle holding the keyboard in this review — its comment field,
    /// or the review itself — so the cockpit keeps it there.
    pub(crate) fn held_focus(&self, window: &Window, cx: &gpui::App) -> Option<FocusHandle> {
        self.editor
            .as_ref()
            .map(|editor| editor.focus_handle(cx))
            .into_iter()
            .chain([self.focus.clone()])
            .find(|focus| focus.is_focused(window))
    }

    /// `scope`'s edits have been read: draw them, the first line picked.
    pub(crate) fn show(&mut self, scope: Scope, edits: Vec<FileEdit>, cx: &mut Context<Self>) {
        self.scope = scope;
        self.lines = review::lines(&edits);
        self.highlights = self
            .lines
            .iter()
            .map(|line| {
                let path = std::path::Path::new(&edits[line.edit].path);
                let language = ferrite_core::transcript::language_for_path(path);
                let body = line.text.get(1..).unwrap_or_default();
                let tokens = ferrite_core::transcript::highlight_tokens(language, body);
                crate::pane::code(body, Some(tokens.as_slice()))
            })
            .collect();
        self.edits = Some(edits);
        self.error = None;
        self.selection = (!self.lines.is_empty()).then(|| Selection::at(0));
        self.editor = None;
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    /// The scope asked for could not be read: say why in place of the diff.
    pub(crate) fn fail(&mut self, error: String, cx: &mut Context<Self>) {
        self.error = Some(error);
        cx.notify();
    }

    /// Ask for `scope` (the one shown again reads it afresh): the bar
    /// moves at once, the diff when it is read.
    pub(crate) fn pick_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        self.scope = scope;
        self.edits = None;
        self.error = None;
        self.lines.clear();
        self.highlights.clear();
        self.selection = None;
        self.editor = None;
        self.commits = None;
        self.commit = None;
        cx.emit(ScopePicked(scope));
        cx.notify();
    }

    /// Pick `line` alone, or stretch the pick to it.
    fn pick(&mut self, line: usize, extend: bool, cx: &mut Context<Self>) {
        if line >= self.lines.len() {
            return;
        }
        let picked = match self.selection {
            Some(selection) if extend => selection.extend_to(line, &self.lines),
            _ => Selection::at(line),
        };
        if self.selection == Some(picked) {
            return;
        }
        self.selection = Some(picked);
        self.scroll.scroll_to_item(picked.head);
        cx.notify();
    }

    /// The picked lines as the file reads them: no signs, no numbers.
    pub(crate) fn picked_text(&self) -> Option<String> {
        let selection = self.selection?;
        let lines = &self.lines[selection.first()..=selection.last()];
        Some(
            lines
                .iter()
                .map(|line| line.text.get(1..).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    /// Where diff line `line` sits: its file's path, as git names it, and
    /// its number there (the old one for a removed line).
    pub(crate) fn place(&self, line: usize) -> Option<(&str, u32)> {
        let line = self.lines.get(line)?;
        let edit = self.edits.as_ref()?.get(line.edit)?;
        Some((edit.path.as_str(), line.place().1))
    }

    fn step(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
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

    pub(crate) fn open_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.listing_commits() {
            self.open_commit(self.commit_pick, cx);
            return;
        }
        if self.selection.is_none() {
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
        let (Some(editor), Some(edits), Some(selection)) =
            (&self.editor, &self.edits, self.selection)
        else {
            return;
        };
        let text = editor.read(cx).text().to_string();
        if let Some(comment) = review::Comment::on(edits, &self.lines, selection, &text) {
            self.composer
                .update(cx, |composer, cx| composer.add_review_comment(comment, cx));
        }
        self.close_editor(window, cx);
    }

    /// ⌫: the latest comment hanging under the pick's last line leaves the
    /// Composer's shelf.
    fn uncomment(&mut self, cx: &mut Context<Self>) {
        let (Some(edits), Some(selection)) = (&self.edits, self.selection) else {
            return;
        };
        let line = &self.lines[selection.last()];
        let path = &edits[line.edit].path;
        let latest = self
            .composer
            .read(cx)
            .review_comments()
            .iter()
            .rposition(|comment| comment.ends_at(path, line));
        if let Some(index) = latest {
            self.composer
                .update(cx, |composer, cx| composer.remove_review_comment(index, cx));
        }
    }

    /// One diff line's row, with whatever hangs off it: the file's path
    /// over its first line, its hunk's section over a hunk's first, and
    /// under it the comments that end on it and the open field.
    fn line_row(
        &self,
        at: usize,
        comments: &[review::Comment],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let edits = self.edits.as_deref().unwrap_or_default();
        let line = &self.lines[at];
        let edit = &edits[line.edit];
        let before = at.checked_sub(1).map(|prev| &self.lines[prev]);
        let first_of_file = before.is_none_or(|prev| prev.edit != line.edit);
        let hunk = before
            .is_none_or(|prev| (prev.edit, prev.hunk) != (line.edit, line.hunk))
            .then(|| &edit.hunks[line.hunk]);
        let picked = self.selection.is_some_and(|picked| picked.contains(at));
        let last_picked = self.selection.is_some_and(|picked| picked.last() == at);
        let ink = DiffKind::of(&line.text).paint();
        let body = line.text.get(1..).unwrap_or_default().to_string();
        let highlights = self.highlights[at].clone();
        let (cell, line_h, number_w) = (self.grid.cell(), self.grid.line, self.number_w());
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
        let row = div()
            .id(("review-line", at))
            .debug_selector(move || format!("review-line-{at}"))
            .flex()
            .w_full()
            .h(px(line_h))
            .whitespace_nowrap()
            .cursor_pointer()
            .map(|row| match (picked, ink.wash) {
                (true, _) => row.bg(paint::SELECTION),
                (false, Some(wash)) => row.bg(rgba(wash)),
                (false, None) => row,
            })
            .child(number(line.old))
            .child(number(line.new))
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
                    .child(StyledText::new(SharedString::from(body)).with_highlights(highlights)),
            )
            .when(last_picked && self.editor.is_none(), |row| {
                row.child(
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
        let indent = 2. * number_w + 2. * cell;
        let mut stack = div().flex().flex_col().w_full();
        if first_of_file {
            stack = stack.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(line_h))
                    .mt(px(if at == 0 { 0. } else { self.grid.half() }))
                    .px(px(cell))
                    .whitespace_nowrap()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(W_LABEL)
                            .text_color(rgb(TEXT_STRONG))
                            .child(SharedString::from(edit.path.clone())),
                    )
                    .child(crate::components::tabular(
                        div()
                            .flex_shrink_0()
                            .pl(px(2. * cell))
                            .child(file_stat(edit)),
                    )),
            );
        }
        if let Some(hunk) = hunk {
            stack = stack.child(
                div()
                    .h(px(line_h))
                    .pl(px(indent))
                    .truncate()
                    .text_color(rgb(TEXT_MUTED))
                    .child(SharedString::from(hunk.section.clone().unwrap_or_else(
                        || format!("@@ \u{2212}{} +{} @@", hunk.old_start, hunk.new_start),
                    ))),
            );
        }
        stack = stack.child(row);
        for (index, comment) in comments.iter().enumerate() {
            if comment.ends_at(&edit.path, line) {
                stack = stack.child(self.comment_row(index, comment, cx));
            }
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
    /// that takes it off the Composer's shelf. `index` is its place there.
    fn comment_row(
        &self,
        index: usize,
        comment: &review::Comment,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                    .id(("review-uncomment", index))
                    .flex_shrink_0()
                    .px(px(cell))
                    .text_color(rgb(TEXT_MUTED))
                    .cursor_pointer()
                    .child("\u{d7}")
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        composer
                            .update(cx, |composer, cx| composer.remove_review_comment(index, cx));
                    })),
            )
            .into_any_element()
    }

    /// A listed commit: its short id and subject; a click reviews it.
    fn commit_row(&self, index: usize, commit: &Commit, cx: &mut Context<Self>) -> AnyElement {
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
                    .child(SharedString::from(short(&commit.id).to_string())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(TEXT))
                    .child(SharedString::from(commit.subject.clone())),
            )
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                window.focus(&view.focus, cx);
                view.open_commit(index, cx);
            }))
            .into_any_element()
    }
}

/// A commit's id as git abbreviates it for people.
fn short(id: &str) -> &str {
    id.get(..7).unwrap_or(id)
}

/// A file's `+N −N`, the way a tool row's trail reads it.
fn file_stat(edit: &FileEdit) -> StyledText {
    let lines = || edit.hunks.iter().flat_map(|hunk| hunk.lines.iter());
    crate::pane::trail_diff_stat(
        lines().filter(|line| line.starts_with('+')).count(),
        lines().filter(|line| line.starts_with('-')).count(),
    )
}

impl Focusable for ReviewView {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ReviewView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut bar = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(CH))
            .h(px(FLOAT_ROW_H))
            .px(px(CH))
            .whitespace_nowrap();
        for scope in Scope::ALL {
            bar = bar.child(
                crate::menu::token(
                    SharedString::from(format!("review-scope-{}", scope.label())),
                    scope.label(),
                    scope == self.scope,
                )
                .debug_selector(move || format!("review-scope-{}", scope.label()))
                .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                    window.focus(&view.focus, cx);
                    view.pick_scope(scope, cx);
                })),
            );
        }
        bar = bar
            .children(self.commit.as_ref().map(|commit| {
                div()
                    .debug_selector(|| "review-commit-reviewed".into())
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(TEXT_MUTED))
                    .child(SharedString::from(format!(
                        "{} {}",
                        short(&commit.id),
                        commit.subject
                    )))
            }))
            .child(div().flex_1())
            .children(self.edits.as_ref().map(|edits| {
                div()
                    .flex_shrink_0()
                    .text_color(rgb(TEXT_MUTED))
                    .child(review::stat(edits))
            }));
        let comments = self.composer.read(cx).review_comments().to_vec();
        let listing = self.listing_commits();
        let body = match (&self.edits, &self.error) {
            (_, Some(error)) => crate::menu::note(format!(
                "can\u{2019}t read {} \u{b7} {error}",
                self.scope.label()
            ))
            .into_any_element(),
            _ if listing => match &self.commits {
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
            },
            (None, None) => crate::menu::note("reading\u{2026}").into_any_element(),
            (Some(edits), None) if edits.is_empty() => {
                crate::menu::note(format!("nothing changed \u{b7} {}", self.scope.label()))
                    .into_any_element()
            }
            (Some(_), None) => div()
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
                .children((0..self.lines.len()).map(|at| self.line_row(at, &comments, cx)))
                .into_any_element(),
        };
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
            .on_action(cx.listener(|view, _: &Reread, _, cx| view.pick_scope(view.scope, cx)))
            .on_action(
                cx.listener(|view, _: &NextScope, _, cx| view.pick_scope(view.scope.next(), cx)),
            )
            .on_action(cx.listener(|view, _: &SaveComment, window, cx| view.save(window, cx)))
            .on_action(
                cx.listener(|view, _: &CancelComment, window, cx| view.close_editor(window, cx)),
            )
            .child(bar)
            .child(body)
            .child(crate::menu::footer(if listing {
                &COMMITS_FOOTER[..]
            } else {
                &FOOTER[..]
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    fn edits() -> Vec<FileEdit> {
        vec![FileEdit {
            path: "src/a.rs".into(),
            hunks: vec![ferrite_core::Hunk {
                old_start: 1,
                old_lines: 2,
                new_start: 1,
                new_lines: 2,
                lines: vec!["-let a = 1;".into(), "+let a = 2;".into(), " a".into()],
                section: None,
            }],
        }]
    }

    /// A Review in a window of its own, showing `edits` and holding the
    /// keyboard, the production keys bound.
    fn open(
        edits: Vec<FileEdit>,
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
                    Scope::LastTurn,
                    ferrite_core::settings::ReadingSize::STANDARD,
                    cx,
                )
            });
            Host(review)
        });
        let mut cx = gpui::VisualTestContext::from_window(*window, cx);
        let review = window.update(&mut cx, |host, _, _| host.0.clone()).unwrap();
        review.update_in(&mut cx, |review, window, cx| {
            review.show(Scope::LastTurn, edits, cx);
            window.focus(&review.focus, cx);
        });
        cx.run_until_parked();
        (review, cx)
    }

    /// The keys walk and stretch the pick, `c` opens the field, ⏎ there
    /// (or its `↵ add`) puts the comment on the Composer's shelf, the next
    /// send carries it after the prompt's own words, and ⌫ takes comments
    /// back one at a time.
    #[gpui::test]
    fn a_comment_from_the_keys_rides_the_next_prompt(cx: &mut TestAppContext) {
        let (review, mut cx) = open(edits(), cx);
        let cx = &mut cx;
        cx.simulate_keystrokes("down shift-down c");
        cx.simulate_input("why two?");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        review.read_with(cx, |review, cx| {
            assert!(review.editor.is_none(), "the field closes on ⏎");
            let composer = review.composer.read(cx);
            assert_eq!(composer.attachment_count(), 1);
            let [comment] = composer.review_comments() else {
                panic!("one comment: {:?}", composer.review_comments());
            };
            assert_eq!(comment.location(), "src/a.rs:1-2");
            assert_eq!(comment.text, "why two?");
            assert!(composer
                .prompt()
                .starts_with("Review comments on your changes:\n\nsrc/a.rs:1-2\n```diff\n+let a = 2;\n a\n```\nwhy two?"));
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
            assert_eq!(review.scope(), Scope::Uncommitted, "⇥ steps the scope");
            assert!(review.edits.is_none(), "until the cockpit reads it");
        });
    }

    /// A press dragged over the lines stretches the pick, held inside the
    /// file it began in, and a release leaves it with its `+ comment`; a
    /// press alone picks one line again.
    #[gpui::test]
    fn a_drag_stretches_the_pick_within_its_file(cx: &mut TestAppContext) {
        let mut two_files = edits();
        two_files.push(FileEdit {
            path: "src/b.rs".into(),
            ..edits().remove(0)
        });
        let (review, mut cx) = open(two_files, cx);
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
        assert_eq!(
            picked(cx),
            Some(Selection { anchor: 0, head: 1 }),
            "the drag stretches the pick"
        );
        cx.simulate_mouse_move(past, MouseButton::Left, none);
        cx.simulate_mouse_up(past, MouseButton::Left, none);
        cx.run_until_parked();
        assert_eq!(
            picked(cx),
            Some(Selection { anchor: 0, head: 2 }),
            "held at the file's last line"
        );
        let comment = cx.debug_bounds("review-add-comment").expect("shown");
        assert!(
            line(cx, "review-line-2").contains(&comment.center()),
            "on the range's last line"
        );
        cx.simulate_mouse_move(from, None, none);
        assert_eq!(
            picked(cx).map(|picked| picked.head),
            Some(2),
            "no drag after the release"
        );
        cx.simulate_click(over, none);
        assert_eq!(picked(cx), Some(Selection::at(1)), "a press picks one line");
    }

    struct Host(Entity<ReviewView>);

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().flex().child(self.0.clone())
        }
    }
}
