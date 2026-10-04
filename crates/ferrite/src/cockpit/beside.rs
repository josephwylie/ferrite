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
                        eprintln!("ferrite: thread {thread} could not be opened beside: {error:?}");
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
            None if document.is_markdown() && document.line.is_none() => {
                pane.document_rich
                    .file_context(document.path.parent(), &pane.preview);
                div()
                    .id(("markdown-reader-scroll", leaf.get() as usize))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(crate::theme::TX_PAD_L))
                    .pt(px(crate::theme::BODY_PAD_T))
                    .pb(px(crate::theme::HALF_ROW))
                    .child(crate::rich::document_body(
                        document.clone(),
                        pane.document_rich.clone(),
                    ))
                    .into_any_element()
            }
            None => numbered_lines(leaf, &document, pane.reader_scroll.clone()),
        };
        let preview = pane.preview.clone();
        let slot = pane
            .preview
            .reader(body, head_drag(leaf, face))?
            .on_key_down(move |event, window, _| {
                if event.keystroke.key == "escape" {
                    preview.close_document(window);
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
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
/// Virtualised: only the rows in view are laid out.
fn numbered_lines(
    leaf: ThreadId,
    document: &crate::attachment_preview::Document,
    scroll: gpui::UniformListScrollHandle,
) -> AnyElement {
    use crate::theme::*;
    let lines: std::rc::Rc<Vec<String>> =
        std::rc::Rc::new(document.source.lines().map(str::to_string).collect());
    let language = ferrite_core::transcript::language_for_path(&document.path);
    let target = document.line;
    let count = lines.len();
    gpui::uniform_list(
        ("reader-lines", leaf.get() as usize),
        count,
        move |range, _window, _cx| {
            range
                .map(|at| {
                    let text = lines[at].clone();
                    let tokens = ferrite_core::transcript::highlight_tokens(language, &text);
                    let highlights = pane::code(&text, Some(tokens.as_slice()));
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

/// `git diff --no-color base...HEAD` in `workspace`; empty when git is not
/// there or refuses.
fn git_diff(workspace: &Path, base: &str) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(["diff", "--no-color"])
        .arg(format!("{base}...HEAD"))
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
}
