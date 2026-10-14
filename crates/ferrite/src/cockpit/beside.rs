//! What opens beside a Pane on the board (F-14, F-15): a reader on a file
//! at a line, a compare of the branch against its base, or another Thread.
//!
//! A reader and a compare take the reader slot the Pane already has
//! (`attachment_preview::Preview`'s document, laid out by `board_tree`): in
//! Solo the board becomes `Pane | reader`, in a Group the Pane's own cell
//! splits, half and half (`READER_SHARE`), the seam between them dragging
//! like any other (22–78%). Escape in the reader — or ⌘W while it holds
//! focus — closes it, and the tree it was spliced into closes back over it,
//! restoring the board as it was.
//!
//! Another Thread opens as a second Pane: beside the current one in Solo (a
//! pair on the Solo board, for as long as focus stays on one of the two),
//! or in a Group as a member right after the focused one
//! (`GroupChange::Include`, non-exclusive, R1) — focused when it is already
//! a member.

use super::*;
use std::path::{Path, PathBuf};

/// Find in the reader (⌘F when the reader took the last press or holds the
/// keyboard; the bar is the transcript's, `crate::find`): a file's lines, or
/// a rendered Markdown document's text. A compare is not searched — ⌘F there
/// finds in the transcript.
pub(crate) struct ReaderFind {
    bar: crate::find::Find,
    /// The document it searches: the bar closes with it, and another one
    /// opening in the reader closes it too (`tend_reader_finds`).
    path: PathBuf,
    /// Frames left to settle a Markdown occurrence into view.
    reveal: u8,
    /// A new query in a Markdown document, waiting for its paint to say
    /// which occurrence is the first in view.
    start_in_view: bool,
}

impl ReaderFind {
    /// Whether the bar's line holds the keyboard.
    pub(crate) fn holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.bar.holds_focus(window, cx)
    }
}

#[cfg(test)]
impl ReaderFind {
    /// What the bar reads, and where a Markdown occurrence last painted.
    pub(crate) fn state(&self) -> (Option<SharedString>, Option<gpui::Bounds<Pixels>>) {
        (self.bar.count(), self.bar.report().revealed.get())
    }
}

/// Whether the reader renders `document` as Markdown (a file opened at a
/// line reads as numbered lines, to mark it).
fn renders_markdown(document: &crate::attachment_preview::Document) -> bool {
    document.is_markdown() && document.line.is_none()
}

/// What `open_beside` opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Beside {
    /// A file, marked and scrolled to `line` (1-based) when there is one.
    Reader { path: PathBuf, line: Option<u32> },
    /// The branch in `workspace` against `base` (`git diff base...HEAD`).
    Compare { workspace: PathBuf, base: String },
    /// Another Thread, as its own Pane.
    Thread(ThreadId),
}

impl CockpitView {
    /// Open something beside the focused Pane (see the module doc).
    pub(crate) fn open_beside(
        &mut self,
        beside: Beside,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match beside {
            Beside::Reader { path, line } => self.open_reader(path, line, window, cx),
            Beside::Compare { workspace, base } => self.open_compare(workspace, base, cx),
            Beside::Thread(thread) => self.open_thread_beside(thread, cx),
        }
    }

    /// The reader beside the focused Pane, at `line`.
    fn open_reader(
        &mut self,
        path: PathBuf,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.empty_shown = false;
        let index = self.focused();
        // The head reads the path from the Thread's workspace, else `~`.
        let workspace = self
            .panes
            .get(index)
            .and_then(PaneView::thread)
            .and_then(|thread| self.thread_path(thread));
        let Some(pane) = self.panes.get(index) else {
            return;
        };
        let title = workspace
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
            .map(|relative| relative.display().to_string())
            .unwrap_or_else(|| home_relative(&path));
        if !pane.preview.open_text_at(path.clone(), title.clone(), line) {
            // Not a UTF-8 file: the image preview, or the OS, takes it.
            pane.preview.open_document(path, title, window, cx);
            cx.notify();
            return;
        }
        if let Some(line) = line.filter(|line| *line > 0) {
            pane.reader_scroll
                .scroll_to_item(line as usize - 1, gpui::ScrollStrategy::Center);
        }
        cx.notify();
    }

    /// The compare beside the focused Pane: `git diff --no-color
    /// base...HEAD` in `workspace`, off the UI thread, parsed into file
    /// edits and drawn by the transcript's own diff document.
    fn open_compare(&mut self, workspace: PathBuf, base: String, cx: &mut Context<Self>) {
        self.empty_shown = false;
        let Some(owner) = self.panes.get(self.focused()).map(|pane| pane.identity) else {
            return;
        };
        let task = cx.background_executor().spawn(async move {
            let diff = git_diff(&workspace, &base);
            let branch = git_branch(&workspace).unwrap_or_else(|| "HEAD".to_string());
            (workspace, compare_of(&branch, &base, &diff))
        });
        cx.spawn(async move |this, cx| {
            let (workspace, compare) = task.await;
            this.update(cx, |view, cx| {
                if let Some(index) = view.index_of(owner) {
                    view.panes[index].preview.open_compare(workspace, compare);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Another Thread as its own Pane beside the focused one (F-15).
    fn open_thread_beside(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.empty_shown = false;
        let Some(owner) = self.cockpit.roster().focused() else {
            return;
        };
        if owner == PaneIdentity::Thread(thread) {
            return;
        }
        match self.cockpit.roster().view() {
            View::Group(group) => {
                let members = self
                    .cockpit
                    .groups()
                    .get(group)
                    .map(|group| group.members.clone())
                    .unwrap_or_default();
                if members.contains(&thread) {
                    self.focus_thread_in_view(thread);
                    cx.notify();
                    return;
                }
                let index = owner
                    .thread()
                    .and_then(|focused| members.iter().position(|member| *member == focused))
                    .map(|at| at + 1);
                if let Err(error) = self.cockpit.apply_group(GroupChange::Include {
                    thread,
                    group,
                    index,
                }) {
                    self.group_error = Some(error.to_string().into());
                    cx.notify();
                    return;
                }
                self.group_error = None;
                // Re-entering lays the new member's Pane (a parked Thread's
                // as its tile) and keeps focus where the operator was.
                if let Err(error) = self.cockpit.enter_group(group) {
                    self.group_error = Some(error.to_string().into());
                }
                self.sync_panes(cx);
            }
            View::Solo => {
                // A parked Thread wakes to stand beside; focus stays home.
                if self.cockpit.thread(thread).is_none() {
                    if let Err(error) = self.cockpit.wake(thread) {
                        ferrite_core::log!("thread {thread} could not be opened beside: {error:?}");
                        return;
                    }
                }
                self.cockpit.focus(owner);
                self.solo_beside = Some((owner, thread));
                self.sync_panes(cx);
            }
        }
        cx.notify();
    }

    /// Focus a Pane already on the board without changing the view.
    fn focus_thread_in_view(&mut self, thread: ThreadId) {
        if let Some(index) = self.pane_for(thread) {
            self.focus_pane(index);
        }
    }

    /// The Solo pair `open_thread_beside` made, while it still holds: the
    /// board is Solo, nothing is fullscreen, both Panes are open and focus
    /// is on one of them.
    pub(super) fn solo_pair(&self) -> Option<(PaneIdentity, PaneIdentity)> {
        let (owner, companion) = self.solo_beside?;
        let companion = PaneIdentity::Thread(companion);
        let roster = self.cockpit.roster();
        let focused = roster.focused()?;
        (roster.view() == View::Solo
            && roster.fullscreen().is_none()
            && (focused == owner || focused == companion)
            && self.index_of(owner).is_some()
            && self.index_of(companion).is_some())
        .then_some((owner, companion))
    }

    /// ⌘F in Pane `index`'s reader: the bar over the reader (or the keyboard
    /// back to it). False when the reader shows nothing to search.
    pub(super) fn open_reader_find(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane) = self.panes.get_mut(index) else {
            return false;
        };
        let Some(document) = pane.preview.document().filter(|doc| doc.compare.is_none()) else {
            return false;
        };
        let owner = pane.identity;
        if pane
            .reader_find
            .as_ref()
            .is_none_or(|find| find.path != document.path)
        {
            pane.reader_find = Some(ReaderFind {
                bar: crate::find::Find::new(cx, move |view: &mut Self, cx| {
                    view.refind_reader(owner, cx)
                }),
                path: document.path,
                reveal: 0,
                start_in_view: false,
            });
            cx.notify();
        }
        if let Some(find) = &pane.reader_find {
            find.bar.focus(window, cx);
        }
        true
    }

    /// Search the reader's document again (its line changed): a new query
    /// starts on its first occurrence in view, or the first below it.
    fn refind_reader(&mut self, owner: PaneIdentity, cx: &mut Context<Self>) {
        let Some(pane) = self.index_of(owner).map(|index| &mut self.panes[index]) else {
            return;
        };
        let (Some(document), Some(find)) = (pane.preview.document(), pane.reader_find.as_mut())
        else {
            return;
        };
        let changed = if renders_markdown(&document) {
            let text = gpui::base::text::TextView::markdown_find_text(&document.source);
            find.bar.search([text.as_str()], 0, cx)
        } else {
            let top = -pane.reader_scroll.0.borrow().base_handle.offset().y;
            let from = (f32::from(top) / crate::theme::LH_UI) as usize;
            find.bar.search(document.source.lines(), from, cx)
        };
        if changed {
            reveal_in_reader(pane, &document, true);
        }
        cx.notify();
    }

    /// ⏎ / ⇧⏎ in the reader's bar.
    fn step_reader_find(&mut self, owner: PaneIdentity, forward: bool, cx: &mut Context<Self>) {
        let Some(pane) = self.index_of(owner).map(|index| &mut self.panes[index]) else {
            return;
        };
        let Some(document) = pane.preview.document() else {
            return;
        };
        if pane
            .reader_find
            .as_mut()
            .is_some_and(|find| find.bar.step(forward).is_some())
        {
            reveal_in_reader(pane, &document, false);
            cx.notify();
        }
    }

    /// Esc in the reader's bar: it closes, the reader stays, and the
    /// keyboard goes back to the Pane. The line held it inside the reader,
    /// where the focus snap (`render`) leaves the reader's own text alone,
    /// so it is let go first.
    fn close_reader_find(
        &mut self,
        owner: PaneIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self.index_of(owner).map(|index| &mut self.panes[index]) {
            pane.reader_find = None;
            window.blur(cx);
            cx.notify();
        }
    }

    /// Each frame, before the board draws: a reader's bar closes with its
    /// document (esc, its ×, another file opening), and a Markdown reveal
    /// that is due settles.
    pub(super) fn tend_reader_finds(&mut self, window: &mut Window) {
        for pane in &mut self.panes {
            let Some(find) = &mut pane.reader_find else {
                continue;
            };
            if pane
                .preview
                .document()
                .is_none_or(|document| document.path != find.path)
            {
                pane.reader_find = None;
                continue;
            }
            settle_markdown_found(find, &pane.reader_markdown_scroll, window);
        }
    }

    /// Close the reader beside `index` (escape in it, ⌘W while it holds
    /// focus): its slot closes and the board is as it was.
    pub(super) fn close_reader(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get(index) {
            pane.preview.close_reader();
            self.focus_pane(index);
            cx.notify();
        }
    }

    /// A Pane's reader as its own board slot (F-14, F-15): the reader shell
    /// (`Preview::reader` — its head the path in `PATH_INK` and `:line` dim,
    /// or a compare's `dev ↔ main · N files +A −R`, the slot's drag handle)
    /// over the body: the file's numbered lines in syntax colour with the
    /// target line marked, rendered Markdown, or the compare's diff. A press
    /// lands on the Pane that opened it; escape closes it.
    pub(super) fn reader_cell(
        &self,
        owner: PaneIdentity,
        leaf: ThreadId,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let index = self.index_of(owner)?;
        let pane = &self.panes[index];
        let document = pane.preview.document()?;
        let find = pane.reader_find.as_ref();
        let face = GhostFace {
            title: document.title.clone().into(),
            detail: Some(SharedString::from(document.kind())),
            reader: true,
            size: self.slot_size(leaf),
        };
        let body = match &document.compare {
            Some(compare) => div()
                .id(("compare-body", leaf.get() as usize))
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(pane::diff_document(
                    &compare.edits,
                    self.prefs.settings.reading_size,
                    self.slot_size(leaf).0 >= crate::theme::SPLIT_DIFF_MIN_W,
                ))
                .into_any_element(),
            None if renders_markdown(&document) => {
                pane.document_rich
                    .file_context(document.path.parent(), &pane.preview);
                let marks = find.and_then(|find| find.bar.marks(0));
                div()
                    .id(("markdown-reader-scroll", leaf.get() as usize))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&pane.reader_markdown_scroll)
                    .px(px(crate::theme::TX_PAD_L))
                    .pt(px(crate::theme::BODY_PAD_T))
                    .pb(px(crate::theme::HALF_ROW))
                    .child(crate::rich::document_body(
                        document.clone(),
                        pane.document_rich.clone(),
                        marks,
                    ))
                    .into_any_element()
            }
            None => numbered_lines(
                leaf,
                &document,
                pane.reader_scroll.clone(),
                find.map(|find| &find.bar),
            ),
        };
        // The bar hangs over the body's top right.
        let body = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(body)
            .children(find.map(|find| find.bar.bar()))
            .into_any_element();
        let preview = pane.preview.clone();
        let slot = pane
            .preview
            .reader(body, head_drag(leaf, face))?
            .on_key_down(move |event, window, _| {
                if event.keystroke.key == "escape" {
                    preview.close_document(window);
                }
            })
            .on_action(cx.listener(move |view, _: &crate::find::Next, _, cx| {
                view.step_reader_find(owner, true, cx)
            }))
            .on_action(cx.listener(move |view, _: &crate::find::Previous, _, cx| {
                view.step_reader_find(owner, false, cx)
            }))
            .on_action(
                cx.listener(move |view, _: &crate::find::Close, window, cx| {
                    view.close_reader_find(owner, window, cx)
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    view.reader_pressed = Some(owner);
                    if let Some(index) = view.index_of(owner) {
                        view.focus_pane(index);
                        cx.notify();
                    }
                }),
            );
        // The reader settles in from its Pane's side each time a document
        // opens in it (keyed by the document, so a new one replays).
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            document.path.hash(&mut hash);
            document.line.hash(&mut hash);
            hash.finish()
        };
        let reader = div().size_full().flex().child(crate::motion::slot_in(
            SharedString::from(format!("reader-in-{}-{key}", leaf.get())),
            slot.flex_1(),
        ));
        Some(self.slot_drop_target(reader, leaf, cx))
    }
}

/// A file's lines, numbered (F-14): a `READER_NUMBER_W` column of numbers,
/// right-aligned and dim, `READER_NUMBER_GAP` before the code, the code in
/// its syntax colours (`highlight_tokens` for the path's language). The
/// target line lies on `paint::SELECTION` with its number in `ACCENT`.
/// A line wider than the reader is cut at its edge; the one holding the
/// find's occurrence starts further in when that would lie past it
/// (`shown_from`). Virtualised: only the rows in view are laid out.
fn numbered_lines(
    leaf: ThreadId,
    document: &crate::attachment_preview::Document,
    scroll: gpui::UniformListScrollHandle,
    find: Option<&crate::find::Find>,
) -> AnyElement {
    use crate::theme::*;
    let lines: std::rc::Rc<Vec<String>> =
        std::rc::Rc::new(document.source.lines().map(str::to_string).collect());
    let language = ferrite_core::transcript::language_for_path(&document.path);
    let target = document.line;
    let count = lines.len();
    // A find's occurrences: its query, and the line and ordinal it is on.
    let find = find.map(|find| {
        (
            find.query().clone(),
            find.active().map(|hit| (hit.unit, hit.ordinal)),
        )
    });
    let list = scroll.clone();
    gpui::uniform_list(
        ("reader-lines", leaf.get() as usize),
        count,
        move |range, _window, _cx| {
            // The cells a line shows before the reader's edge cuts it.
            let width = list.0.borrow().base_handle.bounds().size.width;
            let columns = ((f32::from(width) - READER_NUMBER_W) / CH).max(0.) as usize;
            range
                .map(|at| {
                    let text = lines[at].clone();
                    let tokens = ferrite_core::transcript::highlight_tokens(language, &text);
                    let highlights = pane::code(&text, Some(tokens.as_slice()));
                    let mut washes = Vec::new();
                    let mut from = 0;
                    if let Some((query, active)) = &find {
                        for (ordinal, range) in query.matches(&text).into_iter().enumerate() {
                            let on = *active == Some((at, ordinal));
                            if on {
                                from = shown_from(&text, &range, columns);
                            }
                            washes.push((range, if on { FIND_ACTIVE_WASH } else { FIND_WASH }));
                        }
                    }
                    let (text, highlights) =
                        cut_head(text, crate::find::washed(highlights, &washes), from);
                    let number = at + 1;
                    let marked = target == Some(number as u32);
                    div()
                        .flex()
                        .w_full()
                        .h(px(LH_UI))
                        .whitespace_nowrap()
                        .when(marked, |row| row.bg(paint::SELECTION))
                        .child(
                            div()
                                .flex()
                                .flex_shrink_0()
                                .justify_end()
                                .w(px(READER_NUMBER_W))
                                .pr(px(READER_NUMBER_GAP))
                                .text_color(rgb(if marked { ACCENT } else { TEXT_MUTED }))
                                .child(SharedString::from(number.to_string())),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .text_color(rgb(SYN_PLAIN))
                                .child(
                                    gpui::StyledText::new(SharedString::from(text))
                                        .with_highlights(highlights),
                                ),
                        )
                })
                .collect::<Vec<_>>()
        },
    )
    .track_scroll(&scroll)
    .flex_1()
    .min_h_0()
    .w_full()
    .py(px(HALF_ROW))
    .into_any_element()
}

/// Where a file's line starts drawing so the occurrence the find is on
/// (`found`) shows in the `columns` cells before the reader's edge: its
/// head (0) when it fits, else the occurrence a third of the way in.
fn shown_from(text: &str, found: &std::ops::Range<usize>, columns: usize) -> usize {
    if columns == 0 || text[..found.end].chars().count() <= columns {
        return 0;
    }
    let lead = (text[..found.start].chars().count()).saturating_sub(columns / 3);
    text.char_indices().nth(lead).map_or(0, |(at, _)| at)
}

/// `text` from byte `from` on, a `…` in place of the head it skips, its
/// highlights moved with it.
fn cut_head(
    text: String,
    highlights: Vec<(std::ops::Range<usize>, gpui::HighlightStyle)>,
    from: usize,
) -> (String, Vec<(std::ops::Range<usize>, gpui::HighlightStyle)>) {
    if from == 0 {
        return (text, highlights);
    }
    let shift = |at: usize| at.max(from) - from + '…'.len_utf8();
    let highlights = highlights
        .into_iter()
        .filter(|(range, _)| range.end > from)
        .map(|(range, style)| (shift(range.start)..shift(range.end), style))
        .collect();
    (format!("…{}", &text[from..]), highlights)
}

/// Bring the reader's active occurrence into view: a file's line centred
/// (the list scrolls itself), or a Markdown occurrence once it has painted
/// (`settle_markdown_found`). A new query (`start_in_view`) in a Markdown
/// document is on its first occurrence until that paint says which is the
/// first in view.
fn reveal_in_reader(
    pane: &mut PaneView,
    document: &crate::attachment_preview::Document,
    start_in_view: bool,
) {
    let Some(find) = &mut pane.reader_find else {
        return;
    };
    let Some(hit) = find.bar.active().copied() else {
        return;
    };
    if renders_markdown(document) {
        let report = find.bar.report();
        report.revealed.set(None);
        report.ahead.set(None);
        find.reveal = crate::find::REVEAL_FRAMES;
        find.start_in_view = start_in_view;
    } else {
        pane.reader_scroll
            .scroll_to_item(hit.unit, gpui::ScrollStrategy::Center);
    }
}

/// Each frame while a Markdown reveal is due: once the active occurrence
/// has painted, a new query moves to the first occurrence in view (or the
/// first below it) and paints again; then an occurrence outside the
/// reader's view scrolls a third of the way down.
fn settle_markdown_found(find: &mut ReaderFind, scroll: &gpui::ScrollHandle, window: &mut Window) {
    if find.reveal == 0 {
        return;
    }
    let report = find.bar.report().clone();
    let Some(found) = report.revealed.get() else {
        find.reveal -= 1;
        if find.reveal > 0 {
            window.request_animation_frame();
        }
        return;
    };
    let start = std::mem::take(&mut find.start_in_view);
    let on = find.bar.active().map(|hit| hit.ordinal);
    if let Some(ahead) = report
        .ahead
        .get()
        .filter(|ahead| start && Some(*ahead) != on)
    {
        find.bar.go_to(ahead);
        report.revealed.set(None);
        find.reveal = crate::find::REVEAL_FRAMES;
        window.request_animation_frame();
        return;
    }
    find.reveal = 0;
    let view = scroll.bounds();
    if found.top() < view.top() || found.bottom() > view.bottom() {
        let goal = view.top() + view.size.height / 3.;
        let offset = scroll.offset();
        let lowest = -scroll.max_offset().y;
        let y = (offset.y - (found.top() - goal)).clamp(lowest, px(0.));
        scroll.set_offset(gpui::point(offset.x, y));
    }
}

/// `git diff --no-color base...HEAD` in `workspace`; empty when git is not
/// there or refuses.
fn git_diff(workspace: &Path, base: &str) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(["diff", "--no-color"])
        .arg(format!("{base}...HEAD"))
        // A read, never a lock on the index an agent may be writing.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default()
}

/// The branch `workspace` is on.
fn git_branch(workspace: &Path) -> Option<String> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        // A read, never a lock on the index an agent may be writing.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|branch| !branch.is_empty())
}

/// A compare from git's multi-file unified diff: one file edit per `diff
/// --git` section (named by its `+++ b/` path, else its `--- a/` path for
/// a deletion), its hunks as git printed them, and the head that names it:
/// `dev ↔ main · 3 files +40 −12`.
pub(crate) fn compare_of(
    branch: &str,
    base: &str,
    diff: &str,
) -> crate::attachment_preview::Compare {
    let edits = parse_git_diff(diff);
    let (added, removed) = edits
        .iter()
        .flat_map(|edit| edit.hunks.iter())
        .flat_map(|hunk| hunk.lines.iter())
        .fold((0usize, 0usize), |(added, removed), line| {
            match line.as_bytes().first() {
                Some(b'+') => (added + 1, removed),
                Some(b'-') => (added, removed + 1),
                _ => (added, removed),
            }
        });
    let files = edits.len();
    crate::attachment_preview::Compare {
        head: format!(
            "{branch} \u{2194} {base} \u{b7} {files} file{} +{added} \u{2212}{removed}",
            if files == 1 { "" } else { "s" }
        ),
        edits,
    }
}

/// git's multi-file diff into file edits (see `compare_of`).
pub(crate) fn parse_git_diff(diff: &str) -> Vec<ferrite_core::FileEdit> {
    let mut edits: Vec<ferrite_core::FileEdit> = Vec::new();
    let mut old_path: Option<String> = None;
    let mut hunk: Option<ferrite_core::Hunk> = None;
    let flush = |edits: &mut Vec<ferrite_core::FileEdit>, hunk: &mut Option<ferrite_core::Hunk>| {
        if let (Some(done), Some(edit)) = (hunk.take(), edits.last_mut()) {
            edit.hunks.push(done);
        }
    };
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            flush(&mut edits, &mut hunk);
            old_path = None;
            continue;
        }
        if let Some(path) = line.strip_prefix("--- ") {
            if hunk.is_none() {
                old_path = path.strip_prefix("a/").map(str::to_string);
                continue;
            }
        }
        if let Some(path) = line.strip_prefix("+++ ") {
            if hunk.is_none() {
                flush(&mut edits, &mut hunk);
                let path = path
                    .strip_prefix("b/")
                    .map(str::to_string)
                    .or_else(|| old_path.clone())
                    .unwrap_or_else(|| path.to_string());
                edits.push(ferrite_core::FileEdit {
                    path,
                    hunks: Vec::new(),
                });
                continue;
            }
        }
        if let Some((old_start, old_lines, new_start, new_lines, section)) = hunk_header(line) {
            flush(&mut edits, &mut hunk);
            hunk = Some(ferrite_core::Hunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                section,
                lines: Vec::new(),
            });
            continue;
        }
        if let Some(open) = hunk.as_mut() {
            if matches!(line.as_bytes().first(), Some(b' ' | b'+' | b'-')) {
                open.lines.push(line.to_string());
            }
        }
    }
    flush(&mut edits, &mut hunk);
    edits.retain(|edit| !edit.hunks.is_empty());
    edits
}

/// `@@ -208,7 +208,11 @@ fn thread_row` → (208, 7, 208, 11).
/// A hunk header's ranges and the section git names after it
/// (`@@ -88,6 +88,9 @@ fn draw`).
fn hunk_header(line: &str) -> Option<(u32, u32, u32, u32, Option<String>)> {
    let (old, rest) = line.strip_prefix("@@ -")?.split_once(" +")?;
    let (new, section) = rest.split_once(" @@")?;
    let section = Some(section.trim())
        .filter(|section| !section.is_empty())
        .map(str::to_string);
    let range = |range: &str| -> Option<(u32, u32)> {
        let (start, count) = range.split_once(',').unwrap_or((range, "1"));
        Some((start.parse().ok()?, count.parse().ok()?))
    };
    let (old_start, old_lines) = range(old)?;
    let (new_start, new_lines) = range(new)?;
    Some((old_start, old_lines, new_start, new_lines, section))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "\
diff --git a/crates/ferrite/src/nav.rs b/crates/ferrite/src/nav.rs
index 1111111..2222222 100644
--- a/crates/ferrite/src/nav.rs
+++ b/crates/ferrite/src/nav.rs
@@ -208,7 +208,8 @@ fn thread_row
     let facts = self.facts.get(&t.id);
-    let mut row = div();
+    let live = t.session.is_some();
+    let mut row = div().h(px(ROW_H));
     row
diff --git a/old.rs b/old.rs
deleted file mode 100644
--- a/old.rs
+++ /dev/null
@@ -1,2 +0,0 @@
-fn gone() {}
-
";

    /// F-15: git's multi-file diff parses into one edit per file — a
    /// deletion named by its old path — and the head counts files and lines.
    #[test]
    fn a_compare_parses_git_diff_into_file_edits() {
        let edits = parse_git_diff(DIFF);
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].path, "crates/ferrite/src/nav.rs");
        assert_eq!(edits[0].hunks.len(), 1);
        let hunk = &edits[0].hunks[0];
        assert_eq!(
            (
                hunk.old_start,
                hunk.old_lines,
                hunk.new_start,
                hunk.new_lines
            ),
            (208, 7, 208, 8)
        );
        assert_eq!(hunk.lines.len(), 5);
        assert_eq!(edits[1].path, "old.rs");
        let compare = compare_of("dev", "main", DIFF);
        assert_eq!(
            compare.head,
            "dev \u{2194} main \u{b7} 2 files +2 \u{2212}3"
        );
        assert_eq!(
            compare_of("dev", "main", "").head,
            "dev \u{2194} main \u{b7} 0 files +0 \u{2212}0"
        );
    }

    /// A file's line too long for the reader starts further in on the
    /// line the find is on, so its occurrence shows: a third of the way
    /// across, a `…` for the head, the highlights moved with the text.
    #[test]
    fn a_long_line_slides_its_occurrence_into_view() {
        let text = format!("{}needle()", "x".repeat(100));
        let found = 100..106;
        assert_eq!(shown_from(&text, &found, 120), 0, "it fits");
        let from = shown_from(&text, &found, 60);
        assert_eq!(from, 80, "a third of 60 cells before it");
        let wash = gpui::HighlightStyle::default();
        let (shown, highlights) = cut_head(text, vec![(0..4, wash), (100..106, wash)], from);
        assert_eq!(shown, format!("…{}needle()", "x".repeat(20)));
        let at = &highlights[0].0;
        assert_eq!(highlights.len(), 1, "the skipped head's highlight is gone");
        assert_eq!(&shown[at.clone()], "needle");
    }
}
