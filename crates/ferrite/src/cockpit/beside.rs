//! What opens beside a Pane on the board (F-14, F-15): a reader on a file
//! at a line, a review of the Thread's changes (ADR 0021), or another
//! Thread.
//!
//! A reader and a review take the reader slot the Pane already has
//! (`attachment_preview::Preview`'s document, laid out by `board_tree`): in
//! Solo the board becomes `Pane | reader`, in a Group the Pane's own cell
//! splits, half and half (`READER_SHARE`), the seam between them dragging
//! like any other (22–78%). Escape in the reader, its ×, or ⌘W while it
//! holds focus closes it, and the tree it was spliced into closes back over
//! it, restoring the board as it was.
//!
//! The slot slides (`ReaderSlides`). Opening, its split's share rides a
//! `motion::Reveal` from nothing to rest (`Tree::reveal`) over `SLOT_OPEN`:
//! the Pane narrows every frame, re-wrapping as it does under a cmd-B
//! fold, while the reader slides in from its far edge — its content laid
//! out at its size at rest and pinned to the seam (`reader_pin`), the cell
//! clipping what is not in yet, so the reader's own text never re-wraps.
//! Closing is the reverse over the quicker `SLOT_CLOSE`: the keyboard is
//! the Pane's at once, the document stays drawn (`Preview::shown`) until
//! the slot has slid shut, and only then is it removed from the tree. A
//! flip mid-slide turns around from where it is; another document opening
//! in an open reader (or a review's scope changing) does not slide again,
//! the new document's body fading in (`motion::swap_in`) instead; a seam
//! grabbed mid-slide lands it, and a drag is never tweened. The board's
//! Level and the narrow-window rail are measured where the slide ends.
//! Each frame of a slide redraws the frame around the parts, so only the
//! Pane whose cell moves is built again; reduced motion snaps.
//!
//! A review reads here, where the checkout is — by the Server that owns
//! the Thread, or off the UI thread when this window runs its agents
//! itself — with the vendored review core (`Cockpit::review_diff`,
//! `review_commits`, `search_repo`, `read_review_file`,
//! `review_pull_request`, `review_files`, `review_bases`,
//! `pick_review_base`). Picking another scope, a commit, a search's file,
//! the pull request, every file or a base comes back here to be read.
//!
//! Another Thread opens as a second Pane: beside the current one in Solo (a
//! pair on the Solo board, for as long as focus stays on one of the two),
//! or in a Group as a member right after the focused one
//! (`GroupChange::Include`, non-exclusive, R1) — focused when it is already
//! a member.

use super::*;
use crate::motion::{Reveal, SLOT_CLOSE, SLOT_OPEN};
use crate::review::{
    BasePicked, BasesAsked, CommitPicked, FileOpened, FilesAsked, MenuAsked, PrAsked, ReviewView,
    ScopePicked, Searched,
};
use ferrite_reviewr::ferrite::Read;
use ferrite_reviewr::model::{CommitPick, Scope};
use std::collections::HashMap;
use std::path::PathBuf;

/// Every Pane's reader slot while it is open or sliding (see the module
/// doc), keyed by the Pane that owns it, and what this frame draws of each.
#[derive(Default)]
pub(crate) struct ReaderSlides {
    slides: HashMap<PaneIdentity, Slide>,
    /// The first frame has been tended: a reader open by then arrives in
    /// place, with no entrance (nothing animates on first paint).
    tended: bool,
    /// A slide mid-flight this frame: the board moves, so frames keep
    /// coming.
    moving: bool,
    /// A scene's held reveal (`fixture_hold_reader`).
    hold: Option<f32>,
}

impl ReaderSlides {
    /// Whether a slot is sliding this frame.
    pub(crate) fn moving(&self) -> bool {
        self.moving
    }
}

/// One reader slot's slide.
struct Slide {
    reveal: Reveal,
    /// How far in this frame draws it (the board's geometry between frames
    /// reads what is on screen).
    drawn: f32,
    /// The document it shows (`swap_key`).
    shown: u64,
    /// That document replaced another while the slot stayed open: its body
    /// fades in.
    swapped: bool,
}

/// Which document a reader shows, for telling one opening over another:
/// its path and line (a review is its workspace, whatever its scope).
fn swap_key(document: &crate::attachment_preview::Document) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    document.path.hash(&mut hash);
    document.line.hash(&mut hash);
    hash.finish()
}

/// Where a sliding reader's content lies in its cell `drawn`: at its size
/// at rest (`rest`) along the axis it slides on, pinned to the seam it
/// slides from, so its text never re-wraps while the cell grows or
/// shrinks and the cell clips what is not in yet. `None` at rest.
pub(super) fn reader_pin(
    tree: &Tree,
    leaf: ThreadId,
    drawn: layout::Rect,
    rest: layout::Rect,
) -> Option<layout::Rect> {
    let (axis, first) = tree.side_of(leaf)?;
    match axis {
        layout::Axis::Row if rest.w > drawn.w => Some(layout::Rect {
            x: if first { drawn.w - rest.w } else { 0.0 },
            y: 0.0,
            w: rest.w,
            h: drawn.h,
        }),
        layout::Axis::Column if rest.h > drawn.h => Some(layout::Rect {
            x: 0.0,
            y: if first { drawn.h - rest.h } else { 0.0 },
            w: drawn.w,
            h: rest.h,
        }),
        _ => None,
    }
}

/// Find in the reader (⌘F when the reader took the last press or holds the
/// keyboard; the bar is the transcript's, `crate::find`): a file's lines, or
/// a rendered Markdown document's text. A review finds in its own rows
/// (`review.rs`), while it holds the keyboard.
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

/// How often a read on another thread (a review's git, a PR's `gh`) is
/// looked at for its answer.
const ANSWER_POLL: std::time::Duration = std::time::Duration::from_millis(30);

/// What `open_beside` opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Beside {
    /// A file, marked and scrolled to `line` (1-based) when there is one.
    Reader { path: PathBuf, line: Option<u32> },
    /// The focused Thread's changes, at a scope.
    Review(Scope),
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
            Beside::Review(scope) => self.open_review(scope, window, cx),
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

    /// The review beside the focused Pane, at `scope`, holding the
    /// keyboard. A review already open there switches scope (and reads it
    /// afresh) rather than opening another.
    fn open_review(&mut self, scope: Scope, window: &mut Window, cx: &mut Context<Self>) {
        self.empty_shown = false;
        let Some(pane) = self.panes.get(self.focused()) else {
            return;
        };
        let Some((thread, workspace)) = pane
            .thread()
            .and_then(|thread| Some((thread, self.thread_path(thread)?)))
        else {
            return;
        };
        let review = pane.preview.review().unwrap_or_else(|| {
            let composer = pane.composer.clone();
            let reading = self.prefs.settings.reading_size;
            let review = cx.new(|cx| ReviewView::new(composer, scope, reading, cx));
            cx.subscribe(&review, move |view, review, ScopePicked(scope), cx| {
                view.read_review(thread, review, *scope, cx)
            })
            .detach();
            cx.subscribe(&review, move |view, review, CommitPicked(commit), cx| {
                view.read_commit(thread, review, commit.clone(), cx)
            })
            .detach();
            cx.subscribe(&review, move |view, review, Searched(query), cx| {
                view.search_review(thread, review, query.clone(), cx)
            })
            .detach();
            cx.subscribe(&review, move |view, review, FileOpened { path }, cx| {
                view.read_review_file(thread, review, path.clone(), cx)
            })
            .detach();
            cx.subscribe(&review, move |view, review, _: &PrAsked, cx| {
                view.read_pull_request(thread, review, cx)
            })
            .detach();
            cx.subscribe(&review, move |view, review, _: &FilesAsked, cx| {
                view.read_review_files(thread, review, cx)
            })
            .detach();
            cx.subscribe(&review, move |view, review, _: &BasesAsked, cx| {
                view.read_review_bases(thread, review, cx)
            })
            .detach();
            cx.subscribe(&review, move |view, review, BasePicked(base), cx| {
                view.pick_review_base(thread, review, base.clone(), cx)
            })
            .detach();
            cx.subscribe(&review, move |view, _, MenuAsked { at, line }, cx| {
                let target = MenuTarget::Review {
                    thread,
                    line: *line,
                };
                view.open_context_menu(target, *at, cx)
            })
            .detach();
            review
        });
        let title = self
            .facts
            .get(thread)
            .and_then(|facts| facts.branch.clone())
            .map_or_else(|| home_relative(&workspace), |branch| branch.to_string());
        pane.preview.open_review(workspace, title, review.clone());
        window.focus(&review.focus_handle(cx), cx);
        review.update(cx, |review, cx| review.pick_scope(scope, cx));
        cx.notify();
    }

    /// The review open beside `thread`'s Pane, if one is.
    pub(super) fn review_of(&self, thread: ThreadId) -> Option<Entity<ReviewView>> {
        self.panes.get(self.pane_for(thread)?)?.preview.review()
    }

    /// Read `scope` for `thread` into `review`, where the checkout is — by
    /// the Server, which on a machine reads the machine's — landing later.
    /// A read that lands after the operator moved on is dropped.
    fn read_review(
        &mut self,
        thread: ThreadId,
        review: Entity<ReviewView>,
        scope: Scope,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread) else {
            return;
        };
        let read = match scope {
            Scope::Commits => {
                let commits = self.cockpit.review_commits(&workspace);
                land(
                    review,
                    commits,
                    cx,
                    move |review| review.scope() == scope && review.commit().is_none(),
                    |review, commits, cx| review.show_commits(commits, cx),
                    ReviewView::fail,
                );
                return;
            }
            Scope::Uncommitted => Read::Uncommitted,
            Scope::Branch => Read::Branch,
            Scope::LastTurn => Read::LastTurn,
        };
        let files = self.cockpit.review_diff(&workspace, read);
        land(
            review,
            files,
            cx,
            move |review| review.scope() == scope,
            move |review, files, cx| review.show(scope, files, cx),
            ReviewView::fail,
        );
    }

    /// Read the diff of `pick`, commits listed in the commits scope.
    fn read_commit(
        &mut self,
        thread: ThreadId,
        review: Entity<ReviewView>,
        pick: CommitPick,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread) else {
            return;
        };
        let files = self
            .cockpit
            .review_diff(&workspace, Read::Commits(pick.clone()));
        land(
            review,
            files,
            cx,
            move |review| review.commit() == Some(&pick),
            |review, files, cx| review.show(Scope::Commits, files, cx),
            ReviewView::fail,
        );
    }

    /// Search the checkout for `query`; its answer lands while the field
    /// still reads it.
    fn search_review(
        &mut self,
        thread: ThreadId,
        review: Entity<ReviewView>,
        query: String,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread) else {
            return;
        };
        let found = self.cockpit.search_repo(&workspace, &query);
        let (shown, failed) = (query.clone(), query.clone());
        land(
            review,
            found,
            cx,
            ReviewView::searching,
            move |review, results, cx| review.show_found(&shown, Ok(results), cx),
            move |review, error, cx| review.show_found(&failed, Err(error), cx),
        );
    }

    /// Read file `path` of the checkout for the review's file view.
    fn read_review_file(
        &mut self,
        thread: ThreadId,
        review: Entity<ReviewView>,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread) else {
            return;
        };
        let text = self.cockpit.read_review_file(&workspace, &path);
        let opened = path.clone();
        land(
            review,
            text,
            cx,
            move |review| review.file_open() == Some(opened.as_str()),
            move |review, text, cx| review.show_file(&path, text, cx),
            ReviewView::fail,
        );
    }

    /// Read the pull request of the branch checked out for `thread`.
    fn read_pull_request(
        &mut self,
        thread: ThreadId,
        review: Entity<ReviewView>,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread) else {
            return;
        };
        let pr = self.cockpit.review_pull_request(&workspace);
        land(
            review,
            pr,
            cx,
            ReviewView::pr_open,
            |review, pr, cx| review.show_pr(pr, cx),
            ReviewView::fail,
        );
    }

    /// Read every file of the checkout for the review's all-files list.
    fn read_review_files(
        &mut self,
        thread: ThreadId,
        review: Entity<ReviewView>,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread) else {
            return;
        };
        let files = self.cockpit.review_files(&workspace);
        land(
            review,
            files,
            cx,
            ReviewView::tree_open,
            |review, files, cx| review.show_tree(Ok(files), cx),
            |review, error, cx| review.show_tree(Err(error), cx),
        );
    }

    /// Read the bases the branch scope can read against, for the picker.
    fn read_review_bases(
        &mut self,
        thread: ThreadId,
        review: Entity<ReviewView>,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread) else {
            return;
        };
        let bases = self.cockpit.review_bases(&workspace);
        land(
            review,
            bases,
            cx,
            ReviewView::bases_open,
            |review, bases, cx| review.show_bases(Ok(bases), cx),
            |review, error, cx| review.show_bases(Err(error), cx),
        );
    }

    /// Make `base` the branch scope's base, then read the branch over it.
    fn pick_review_base(
        &mut self,
        thread: ThreadId,
        review: Entity<ReviewView>,
        base: String,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread) else {
            return;
        };
        let picked = self.cockpit.pick_review_base(&workspace, &base);
        land(
            review,
            picked,
            cx,
            |_| true,
            |review, (), cx| review.pick_scope(Scope::Branch, cx),
            ReviewView::fail,
        );
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
        let Some(document) = pane.preview.document().filter(|doc| doc.review.is_none()) else {
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
    /// focus): the keyboard is the Pane's at once, and its slot slides shut
    /// until the board is as it was.
    pub(super) fn close_reader(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get(index) {
            pane.preview.close_reader();
            self.focus_pane(index);
            cx.notify();
        }
    }

    /// Whether Pane `index` has a reader slot on the board: a document
    /// open in it, or one sliding shut.
    pub(super) fn has_reader(&self, index: usize) -> bool {
        self.panes.get(index).is_some_and(|pane| {
            pane.preview.is_open() || self.reader_slides.slides.contains_key(&pane.identity)
        })
    }

    /// Each frame, before the board is measured: a reader that opened
    /// starts sliding in (in place on first paint), one that closed starts
    /// sliding out, one reopened mid-slide turns around, another document
    /// opening in an open one is marked to fade in, and a slot that has
    /// slid shut lets its document go. Then what this frame draws of each.
    pub(super) fn tend_reader_slides(&mut self, cx: &gpui::App) {
        let now = cx.background_executor().now();
        let reduced = crate::motion::reduced_motion(cx);
        let panes = &self.panes;
        let slides = &mut self.reader_slides;
        let first = !std::mem::replace(&mut slides.tended, true);
        slides
            .slides
            .retain(|owner, _| panes.iter().any(|pane| pane.identity == *owner));
        for pane in panes {
            let owner = pane.identity;
            let shown = pane.preview.document().map(|document| swap_key(&document));
            match (slides.slides.get_mut(&owner), shown) {
                (None, Some(shown)) => {
                    let reveal = if first {
                        Reveal::open(SLOT_OPEN, SLOT_CLOSE, now)
                    } else {
                        Reveal::opening(SLOT_OPEN, SLOT_CLOSE, now)
                    };
                    let slide = Slide {
                        reveal,
                        drawn: 0.0,
                        shown,
                        swapped: false,
                    };
                    slides.slides.insert(owner, slide);
                }
                (Some(slide), Some(shown)) => {
                    if !slide.reveal.is_open() {
                        slide.reveal.set(true, now, reduced);
                        pane.preview.let_go();
                    }
                    if slide.shown != shown {
                        slide.shown = shown;
                        slide.swapped = true;
                    }
                }
                (Some(slide), None) => {
                    slide.reveal.set(false, now, reduced);
                    if slide.reveal.shut(now, reduced) {
                        slides.slides.remove(&owner);
                        pane.preview.let_go();
                    }
                }
                // Closed before a frame drew it: nothing to slide.
                (None, None) => pane.preview.let_go(),
            }
        }
        let hold = slides.hold;
        let mut moving = false;
        for slide in slides.slides.values_mut() {
            slide.drawn = hold.unwrap_or_else(|| slide.reveal.value(now, reduced));
            moving |= hold.is_none() && slide.reveal.running(now, reduced);
        }
        slides.moving = moving;
    }

    /// A seam grabbed mid-slide: every slot lands where it was going — a
    /// closing one is gone — so the drag starts from the board at rest.
    pub(super) fn land_reader_slides(&mut self, cx: &gpui::App) {
        let now = cx.background_executor().now();
        let panes = &self.panes;
        self.reader_slides.slides.retain(|owner, slide| {
            slide.reveal.land(now);
            slide.drawn = 1.0;
            if !slide.reveal.is_open() {
                if let Some(pane) = panes.iter().find(|pane| pane.identity == *owner) {
                    pane.preview.let_go();
                }
            }
            slide.reveal.is_open()
        });
    }

    /// `tree` as this frame draws it: each sliding reader as far in as it
    /// is (`Tree::reveal`). A seam drag draws its tree as it is.
    pub(super) fn slid_tree(&self, mut tree: Tree, bounds: layout::Rect) -> Tree {
        if self.seam_drag.is_some() {
            return tree;
        }
        let reveals: Vec<(ThreadId, f32)> = self
            .reader_slides
            .slides
            .iter()
            .filter(|(_, slide)| slide.drawn < 1.0)
            .map(|(owner, slide)| (reader_leaf(*owner), slide.drawn))
            .collect();
        if !reveals.is_empty() {
            tree.reveal(&reveals, bounds, crate::theme::BOARD_SEAM);
        }
        tree
    }

    /// `tree` where every slide ends: the readers sliding shut gone, the
    /// ones sliding in at rest.
    pub(super) fn rest_tree(&self, mut tree: Tree) -> Tree {
        for (owner, slide) in &self.reader_slides.slides {
            if !slide.reveal.is_open() {
                tree.remove(reader_leaf(*owner));
            }
        }
        tree
    }

    /// Hold every reader's slide at `reveal` (0 shut → 1 at rest) so a
    /// scene can capture the board mid-slide; `None` lets it go.
    #[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
    pub(crate) fn fixture_hold_reader(&mut self, reveal: Option<f32>) {
        self.reader_slides.hold = reveal;
    }

    /// A Pane's reader as its own board slot (F-14, F-15): the reader shell
    /// (`Preview::reader` — its head the path in `LINK_INK` and `:line` dim,
    /// or `review · dev`, the slot's drag handle) over the body: the file's
    /// numbered lines in syntax colour with the target line marked,
    /// rendered Markdown, or the review. A press lands on the Pane that
    /// opened it; escape closes it. Mid-slide the shell lies at `pin`
    /// (`reader_pin`), the cell clipping it.
    pub(super) fn reader_cell(
        &self,
        owner: PaneIdentity,
        leaf: ThreadId,
        pin: Option<layout::Rect>,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let index = self.index_of(owner)?;
        let pane = &self.panes[index];
        let document = pane.preview.shown()?;
        let find = pane.reader_find.as_ref();
        let face = GhostFace {
            title: document.title.clone().into(),
            detail: Some(SharedString::from(document.kind())),
            reader: true,
            size: self.slot_size(leaf),
        };
        let body = match &document.review {
            Some(review) => review.clone().into_any_element(),
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
                    .px(px(crate::theme::TX_INSET_SOLO))
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
            .children(find.map(|find| find.bar.bar()));
        // A document that replaced another while the slot stayed open
        // fades in where the last one was; the slot holds still.
        let body = match self
            .reader_slides
            .slides
            .get(&owner)
            .filter(|slide| slide.swapped)
        {
            Some(slide) => crate::motion::swap_in(
                SharedString::from(format!("reader-swap-{}-{}", leaf.get(), slide.shown)),
                body,
            )
            .into_any_element(),
            None => body.into_any_element(),
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
            .on_action(cx.listener(move |view, _: &crate::review::Close, _, cx| {
                if let Some(index) = view.index_of(owner) {
                    view.close_reader(index, cx);
                }
            }))
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
        let reader = match pin {
            None => div().size_full().flex().child(slot.flex_1()),
            Some(pin) => div().size_full().relative().overflow_hidden().child(
                slot.absolute()
                    .left(px(pin.x))
                    .top(px(pin.y))
                    .w(px(pin.w))
                    .h(px(pin.h)),
            ),
        };
        Some(self.slot_drop_target(reader, leaf, cx))
    }
}

/// A file's lines, numbered (F-14): a `READER_NUMBER_W` column of numbers,
/// right-aligned and dim, `READER_NUMBER_GAP` before the code, the code in
/// its syntax colours (`highlight_tokens` for the path's language). The
/// target line lies on `paint::SELECTED` with its number in `TEXT_STRONG`.
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
                        .when(marked, |row| row.bg(paint::SELECTED))
                        .child(
                            div()
                                .flex()
                                .flex_shrink_0()
                                .justify_end()
                                .w(px(READER_NUMBER_W))
                                .pr(px(READER_NUMBER_GAP))
                                .text_color(rgb(if marked { TEXT_STRONG } else { TEXT_DIM }))
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

/// The answer to a read on another thread (`Cockpit::review_diff`,
/// `Cockpit::pull_request`), awaited without holding the UI thread.
pub(super) async fn answered<T>(
    answer: std::io::Result<std::sync::mpsc::Receiver<std::io::Result<T>>>,
    cx: &gpui::AsyncApp,
) -> std::io::Result<T> {
    let answer = answer?;
    loop {
        cx.background_executor().timer(ANSWER_POLL).await;
        match answer.try_recv() {
            Ok(read) => return read,
            Err(std::sync::mpsc::TryRecvError::Empty) => continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err(std::io::Error::other("the read ended"))
            }
        }
    }
}

/// Hand a review's read to `show` once it lands — or say why it could
/// not be read — unless `current` says the review moved on meanwhile.
fn land<T: 'static>(
    review: Entity<ReviewView>,
    answer: std::io::Result<std::sync::mpsc::Receiver<std::io::Result<T>>>,
    cx: &mut Context<CockpitView>,
    current: impl Fn(&ReviewView) -> bool + 'static,
    show: impl FnOnce(&mut ReviewView, T, &mut Context<ReviewView>) + 'static,
    fail: impl FnOnce(&mut ReviewView, String, &mut Context<ReviewView>) + 'static,
) {
    let review = review.downgrade();
    cx.spawn(async move |_, cx| {
        let read = answered(answer, cx).await;
        review
            .update(cx, |review, cx| {
                if !current(review) {
                    return;
                }
                match read {
                    Ok(read) => show(review, read, cx),
                    Err(error) => fail(review, error.to_string(), cx),
                }
            })
            .ok();
    })
    .detach();
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

#[cfg(test)]
mod tests {
    use super::*;

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
