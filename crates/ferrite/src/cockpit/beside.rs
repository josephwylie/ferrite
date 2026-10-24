//! What opens beside a Pane on the board (F-15): a review of the Thread's
//! changes (ADR 0021), or another Thread. A file, an image or a command's
//! output opens in the Panel instead (`panel/preview.rs`).
//!
//! A review takes the reader slot the Pane has (`ReviewSlot`, laid out by
//! `board_tree`): it needs the board's width, and takes line comments. In
//! Solo the board becomes `Pane | review`, in a Group (or a fullscreen
//! Pane) the Pane's own cell splits, half and half (`READER_SHARE`), the
//! seam between them dragging like any other (22–78%). Escape in it, its
//! ×, or ⌘W while it holds focus closes it, and the tree it was spliced
//! into closes back over it, restoring the board as it was.
//!
//! The slot slides (`ReaderSlides`). Opening, its split's share rides a
//! `motion::Reveal` from nothing to rest (`Tree::reveal`) over `SLOT_OPEN`:
//! the Pane narrows every frame, re-wrapping as it does under a cmd-B
//! fold, while the review slides in from its far edge — its content laid
//! out at its size at rest and pinned to the seam (`reader_pin`), the cell
//! clipping what is not in yet, so its own text never re-wraps. Closing is
//! the reverse over the quicker `SLOT_CLOSE`: the keyboard is the Pane's at
//! once, the review stays drawn (`ReviewSlot::shown`) until the slot has
//! slid shut, and only then is it removed from the tree. A flip mid-slide
//! turns around from where it is; a review switching scope reads in place,
//! and the slot does not slide again; a seam grabbed mid-slide lands it,
//! and a drag is never tweened. The board's Level and the narrow-window
//! rail are measured where the slide ends. Each frame of a slide redraws
//! the frame around the parts, so only the Pane whose cell moves is built
//! again; reduced motion snaps. A review is a Thread's — a draft has none
//! — so a slot is keyed by its Thread.
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
use crate::preview::{head_door, reading_head};
use crate::review::{
    BasePicked, BasesAsked, CommitPicked, FileOpened, FilesAsked, MenuAsked, PrAsked, ReviewView,
    ScopePicked, Searched,
};
use ferrite_reviewr::ferrite::Read;
use ferrite_reviewr::model::{CommitPick, Scope};
use std::collections::HashMap;

/// A Pane's Review on the board (ADR 0021): open in its reader slot, or
/// sliding shut.
pub(crate) struct ReviewSlot {
    open: Option<OpenReview>,
    /// The review just closed, still drawn while its slot slides shut; the
    /// cockpit lets it go when the slide ends (`let_go`).
    closing: Option<OpenReview>,
    /// Tracked by the slot, so ⌘W in it closes the review rather than its
    /// Pane.
    focus: FocusHandle,
}

#[derive(Clone)]
struct OpenReview {
    view: Entity<ReviewView>,
    /// What its head reads after `review ·`: the branch, else the workspace.
    title: String,
}

impl ReviewSlot {
    pub(crate) fn new(cx: &mut gpui::App) -> Self {
        Self {
            open: None,
            closing: None,
            focus: cx.focus_handle(),
        }
    }

    /// The review open in the slot.
    pub(crate) fn review(&self) -> Option<Entity<ReviewView>> {
        self.open.as_ref().map(|open| open.view.clone())
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Show `view` in the slot under `title`.
    pub(crate) fn show(&mut self, view: Entity<ReviewView>, title: String) {
        self.open = Some(OpenReview { view, title });
        self.closing = None;
    }

    /// What the slot draws: the open review, else the one sliding shut.
    fn shown(&self) -> Option<&OpenReview> {
        self.open.as_ref().or(self.closing.as_ref())
    }

    /// Close it: closed at once, the keyboard the Pane's, while it stays
    /// drawn until its slot has slid shut.
    fn close(&mut self) {
        if let Some(open) = self.open.take() {
            self.closing = Some(open);
        }
    }

    /// The slot has slid shut: stop holding the review.
    fn let_go(&mut self) {
        self.closing = None;
    }

    /// Whether the review (or anything in it) holds the keyboard.
    pub(crate) fn focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.is_open() && self.focus.contains_focused(window, cx)
    }

    /// Whether something inside the open review holds the keyboard — its
    /// text, a field — rather than the slot itself, taking focus on a
    /// click that hands it back to the Pane.
    pub(crate) fn inner_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.focused(window, cx) && !self.focus.is_focused(window)
    }

    /// The keyboard focus the open review holds (a comment field's too),
    /// kept from its first frame.
    pub(crate) fn held_focus(&self, window: &Window, cx: &gpui::App) -> Option<FocusHandle> {
        self.review()?.read(cx).held_focus(window, cx)
    }
}

/// Every Thread's reader slot while it is open or sliding (see the module
/// doc), keyed by the Thread whose Pane owns it, and what this frame draws
/// of each.
#[derive(Default)]
pub(crate) struct ReaderSlides {
    slides: HashMap<ThreadId, Slide>,
    /// The first frame has been tended: a review open by then arrives in
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
}

/// Where a sliding review's content lies in its cell `drawn`: at its size
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

/// How often a read on another thread (a review's git, a PR's `gh`) is
/// looked at for its answer.
const ANSWER_POLL: std::time::Duration = std::time::Duration::from_millis(30);

/// What `open_beside` opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Beside {
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
            Beside::Review(scope) => self.open_review(scope, window, cx),
            Beside::Thread(thread) => self.open_thread_beside(thread, cx),
        }
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
        let review = pane.review.review().unwrap_or_else(|| {
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
        let index = self.focused();
        self.panes[index].review.show(review.clone(), title);
        window.focus(&review.focus_handle(cx), cx);
        review.update(cx, |review, cx| review.pick_scope(scope, cx));
        cx.notify();
    }

    /// The review open beside `thread`'s Pane, if one is.
    pub(super) fn review_of(&self, thread: ThreadId) -> Option<Entity<ReviewView>> {
        self.panes.get(self.pane_for(thread)?)?.review.review()
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

    /// Close the review beside `index` (escape in it, ⌘W while it holds
    /// focus): the keyboard is the Pane's at once, and its slot slides shut
    /// until the board is as it was.
    pub(super) fn close_review(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get_mut(index) {
            pane.review.close();
            self.focus_pane(index);
            cx.notify();
        }
    }

    /// Whether Pane `index` has a reader slot on the board: a review open
    /// in it, or one sliding shut.
    pub(super) fn has_reader(&self, index: usize) -> bool {
        self.panes.get(index).is_some_and(|pane| {
            pane.review.is_open()
                || pane
                    .thread()
                    .is_some_and(|thread| self.reader_slides.slides.contains_key(&thread))
        })
    }

    /// Each frame, before the board is measured: a review that opened
    /// starts sliding in (in place on first paint), one that closed starts
    /// sliding out, one reopened mid-slide turns around, and a slot that
    /// has slid shut lets its review go. Then what this frame draws of
    /// each.
    pub(super) fn tend_reader_slides(&mut self, cx: &gpui::App) {
        let now = cx.background_executor().now();
        let reduced = crate::motion::reduced_motion(cx);
        let panes = &mut self.panes;
        let slides = &mut self.reader_slides;
        let first = !std::mem::replace(&mut slides.tended, true);
        slides
            .slides
            .retain(|owner, _| panes.iter().any(|pane| pane.thread() == Some(*owner)));
        for pane in panes {
            let Some(owner) = pane.thread() else {
                continue;
            };
            match (slides.slides.get_mut(&owner), pane.review.is_open()) {
                (None, true) => {
                    let reveal = if first {
                        Reveal::open(SLOT_OPEN, SLOT_CLOSE, now)
                    } else {
                        Reveal::opening(SLOT_OPEN, SLOT_CLOSE, now)
                    };
                    slides.slides.insert(owner, Slide { reveal, drawn: 0.0 });
                }
                (Some(slide), true) => {
                    if !slide.reveal.is_open() {
                        slide.reveal.set(true, now, reduced);
                        pane.review.let_go();
                    }
                }
                (Some(slide), false) => {
                    slide.reveal.set(false, now, reduced);
                    if slide.reveal.shut(now, reduced) {
                        slides.slides.remove(&owner);
                        pane.review.let_go();
                    }
                }
                // Closed before a frame drew it: nothing to slide.
                (None, false) => pane.review.let_go(),
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
        let panes = &mut self.panes;
        self.reader_slides.slides.retain(|owner, slide| {
            slide.reveal.land(now);
            slide.drawn = 1.0;
            if !slide.reveal.is_open() {
                if let Some(pane) = panes.iter_mut().find(|pane| pane.thread() == Some(*owner)) {
                    pane.review.let_go();
                }
            }
            slide.reveal.is_open()
        });
    }

    /// `tree` as this frame draws it: each sliding review as far in as it
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

    /// `tree` where every slide ends: the reviews sliding shut gone, the
    /// ones sliding in at rest.
    pub(super) fn rest_tree(&self, mut tree: Tree) -> Tree {
        for (owner, slide) in &self.reader_slides.slides {
            if !slide.reveal.is_open() {
                tree.remove(reader_leaf(*owner));
            }
        }
        tree
    }

    /// Hold every review's slide at `reveal` (0 shut → 1 at rest) so a
    /// scene can capture the board mid-slide; `None` lets it go.
    #[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
    pub(crate) fn fixture_hold_reader(&mut self, reveal: Option<f32>) {
        self.reader_slides.hold = reveal;
    }

    /// A Thread's review as its own board slot (ADR 0021): the review under
    /// its head (`reading_head` — `review · dev` and its ×, the slot's drag
    /// handle). A press lands on the Pane that opened it; escape closes it.
    /// Mid-slide the slot lies at `pin` (`reader_pin`), the cell clipping
    /// it.
    pub(super) fn reader_cell(
        &self,
        owner: ThreadId,
        leaf: ThreadId,
        pin: Option<layout::Rect>,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        use crate::theme::*;
        let index = self.pane_for(owner)?;
        let slot = &self.panes[index].review;
        let shown = slot.shown()?;
        let face = GhostFace {
            title: shown.title.clone().into(),
            detail: Some("DIFF".into()),
            reader: true,
            size: self.slot_size(leaf),
        };
        let label = div()
            .min_w_0()
            .truncate()
            .text_color(rgb(TEXT_DIM))
            .child(format!("review \u{b7} {}", shown.title));
        let close = head_door(
            "close-review",
            format!("review-close-{}", owner.get()),
            crate::icons::CLOSE,
        )
        .tooltip(crate::menu::tooltip_with_key(
            "Close the review",
            crate::components::bound_chord_in("reader::Close", Some(crate::keymap::READER)),
        ))
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            if let Some(index) = view.pane_for(owner) {
                view.close_review(index, cx);
            }
        }))
        .into_any_element();
        let slot = div()
            .debug_selector(|| "review-slot".into())
            .track_focus(&slot.focus)
            .key_context(crate::keymap::READER)
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(paint::CARD)
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .child(head_drag(leaf, face)(reading_head(label, [close])))
            .child(shown.view.clone())
            .on_action(cx.listener(move |view, _: &panel::preview::Close, _, cx| {
                if let Some(index) = view.pane_for(owner) {
                    view.close_review(index, cx);
                }
            }))
            .on_action(cx.listener(move |view, _: &crate::review::Close, _, cx| {
                if let Some(index) = view.pane_for(owner) {
                    view.close_review(index, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    if let Some(index) = view.pane_for(owner) {
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
