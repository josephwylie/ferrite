//! What opens beside a Pane on the board (F-15): a review of the Thread's
//! changes (ADR 0021). A file, an image or a command's output opens in the
//! Panel instead (`panel/preview.rs`).
//!
//! The board a reader opens is drawn here too: the shown Pane and its
//! reader at their tree's rects (`tree_board`), its seam dragged, and a
//! slot dropped on another (`drop_pane`).
//!
//! A review takes the reader slot the Pane has (`ReviewSlot`, laid out by
//! `board_tree`): it needs the board's width, and takes line comments. The
//! board becomes `Pane | review`, half and half (`READER_SHARE`), the seam
//! between them dragging (22–78%). Escape in it, its
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
//! and a drag is never tweened. The narrow-window
//! rail is measured where the slide ends. Each frame of a slide redraws
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

use super::*;
use super::slot_drag::{GhostFace, PaneGhost};
use ferrite_review::{
    BasePicked, BasesAsked, CommitPicked, FileOpened, FilesAsked, MenuAsked, PrAsked, ReviewView,
    ScopePicked, Searched,
};
use ferrite_pane::preview::reading_head;
use ferrite_reviewr::ferrite::Read;
use ferrite_reviewr::model::{CommitPick, Scope};
use ferrite_ui::motion::{Reveal, SLOT_CLOSE, SLOT_OPEN};
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
    slides: HashMap<ThreadKey, Slide>,
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
    leaf: Leaf,
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

/// What `open_beside` opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Beside {
    /// The focused Thread's changes, at a scope.
    Review(Scope),
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
        }
    }

    /// The review beside the focused Pane, at `scope`, holding the
    /// keyboard. A review already open there switches scope (and reads it
    /// afresh) rather than opening another.
    fn open_review(&mut self, scope: Scope, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.panes.get(self.focused(cx)) else {
            return;
        };
        let Some((thread, workspace)) = pane
            .thread()
            .and_then(|thread| Some((thread, self.thread_path(thread, cx)?)))
        else {
            return;
        };
        let review = pane.review.review().unwrap_or_else(|| {
            let composer = pane.read(cx).composer.clone();
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
        let index = self.focused(cx);
        let Some(pane) = self.panes.get_mut(index) else {
            return;
        };
        pane.review.show(review.clone(), title);
        window.focus(&review.focus_handle(cx), cx);
        review.update(cx, |review, cx| review.pick_scope(scope, cx));
        cx.notify();
    }

    /// The review open beside `thread`'s Pane, if one is.
    pub(super) fn review_of(&self, thread: ThreadKey) -> Option<Entity<ReviewView>> {
        self.panes.get(self.pane_for(thread)?)?.review.review()
    }

    /// Read `scope` for `thread` into `review`, where the checkout is — by
    /// the Server, which on a machine reads the machine's — landing later.
    /// A read that lands after the operator moved on is dropped.
    fn read_review(
        &mut self,
        thread: ThreadKey,
        review: Entity<ReviewView>,
        scope: Scope,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread, cx) else {
            return;
        };
        let read = match scope {
            Scope::Commits => {
                let commits = self.cockpit.update(cx, |cockpit, _| cockpit.review_commits(thread.machine(), &workspace));
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
        let files = self.cockpit.update(cx, |cockpit, _| cockpit.review_diff(thread.machine(), &workspace, read));
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
        thread: ThreadKey,
        review: Entity<ReviewView>,
        pick: CommitPick,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread, cx) else {
            return;
        };
        let files = self
            .cockpit
            .update(cx, |cockpit, _| cockpit.review_diff(thread.machine(), &workspace, Read::Commits(pick.clone())));
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
        thread: ThreadKey,
        review: Entity<ReviewView>,
        query: String,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread, cx) else {
            return;
        };
        let found = self.cockpit.update(cx, |cockpit, _| cockpit.search_repo(thread.machine(), &workspace, &query));
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
        thread: ThreadKey,
        review: Entity<ReviewView>,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread, cx) else {
            return;
        };
        let text = self.cockpit.update(cx, |cockpit, _| cockpit.read_review_file(thread.machine(), &workspace, &path));
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
        thread: ThreadKey,
        review: Entity<ReviewView>,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread, cx) else {
            return;
        };
        let pr = self.cockpit.update(cx, |cockpit, _| cockpit.review_pull_request(thread.machine(), &workspace, None));
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
        thread: ThreadKey,
        review: Entity<ReviewView>,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread, cx) else {
            return;
        };
        let files = self.cockpit.update(cx, |cockpit, _| cockpit.review_files(thread.machine(), &workspace));
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
        thread: ThreadKey,
        review: Entity<ReviewView>,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread, cx) else {
            return;
        };
        let bases = self.cockpit.update(cx, |cockpit, _| cockpit.review_bases(thread.machine(), &workspace));
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
        thread: ThreadKey,
        review: Entity<ReviewView>,
        base: String,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.thread_path(thread, cx) else {
            return;
        };
        let picked = self.cockpit.update(cx, |cockpit, _| cockpit.pick_review_base(thread.machine(), &workspace, &base));
        land(
            review,
            picked,
            cx,
            |_| true,
            |review, (), cx| review.pick_scope(Scope::Branch, cx),
            ReviewView::fail,
        );
    }

    /// Close the review beside `index` (escape in it, ⌘W while it holds
    /// focus): the keyboard is the Pane's at once, and its slot slides shut
    /// until the board is as it was.
    pub(super) fn close_review(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get_mut(index) {
            pane.review.close();
            self.focus_pane(index, cx);
            cx.notify();
        }
    }

    /// Whether Pane `index` has a reader slot on the board: a review open
    /// in it, or one sliding shut.
    pub(super) fn has_reader(&self, index: usize) -> bool {
        self.panes.get(index).is_some_and(|pane| {
            pane.review.is_open()
                || pane.thread().is_some_and(|thread| {
                    let owner = thread;
                    self.reader_slides.slides.contains_key(&owner)
                })
        })
    }

    /// Each frame, before the board is measured: a review that opened
    /// starts sliding in (in place on first paint), one that closed starts
    /// sliding out, one reopened mid-slide turns around, and a slot that
    /// has slid shut lets its review go. Then what this frame draws of
    /// each.
    pub(super) fn tend_reader_slides(&mut self, cx: &gpui::App) {
        let now = cx.background_executor().now();
        let reduced = ferrite_ui::motion::reduced_motion(cx);
        let panes = &mut self.panes;
        let slides = &mut self.reader_slides;
        let first = !std::mem::replace(&mut slides.tended, true);
        let owner_of = |pane: &CockpitPane| pane.thread();
        slides
            .slides
            .retain(|owner, _| panes.iter().any(|pane| owner_of(pane) == Some(*owner)));
        for pane in panes {
            let Some(owner) = owner_of(pane) else {
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
                let owns = |pane: &&mut CockpitPane| {
                    pane.thread() == Some(*owner)
                };
                if let Some(pane) = panes.iter_mut().find(owns) {
                    pane.review.let_go();
                }
            }
            slide.reveal.is_open()
        });
    }

    /// `tree` as this frame draws it: each sliding review as far in as it
    /// is (`Tree::reveal`). A seam drag draws its tree as it is.
    pub(super) fn slid_tree(&self, mut tree: Tree, bounds: layout::Rect) -> Tree {
        if self.board_layouts.seam_held() {
            return tree;
        }
        let reveals: Vec<(Leaf, f32)> = self
            .reader_slides
            .slides
            .iter()
            .filter(|(_, slide)| slide.drawn < 1.0)
            .map(|(owner, slide)| (reader_leaf(*owner), slide.drawn))
            .collect();
        if !reveals.is_empty() {
            tree.reveal(&reveals, bounds, ferrite_ui::theme::BOARD_SEAM);
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
    /// its head (`reading_head` — `Review · dev` and its ×, the slot's drag
    /// handle). A press lands on the Pane that opened it; escape closes it.
    /// Mid-slide the slot lies at `pin` (`reader_pin`), the cell clipping
    /// it.
    pub(super) fn reader_cell(
        &self,
        owner: ThreadKey,
        leaf: Leaf,
        pin: Option<layout::Rect>,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        use ferrite_ui::theme::*;
        let index = self.pane_for(owner)?;
        let slot = &self.panes[index].review;
        let shown = slot.shown()?;
        let face = GhostFace {
            title: shown.title.clone().into(),
            detail: Some("DIFF".into()),
            reader: true,
            size: self.slot_drag.size(leaf).unwrap_or((super::slot_drag::GHOST_MAX_W, super::slot_drag::GHOST_MAX_H)),
        };
        // `Review · main`: the surface's name in the head's ink, what it
        // reads dim after it, as the preview's head names its file.
        let label = div()
            .flex()
            .min_w_0()
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(W_LABEL)
                    .text_color(rgb(INK_STRONG))
                    .child("Review"),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(INK_SUBTLE))
                    .child(format!(" \u{b7} {}", shown.title)),
            );
        let close = ferrite_ui::components::icon_button(
            "close-review",
            ferrite_ui::icons::CLOSE,
            ferrite_ui::components::IconSize::Standard,
            "Close the review",
            cx,
        )
        .tip_with_key(
            "Close the review",
            ferrite_ui::components::bound_chord_in(
                "reader::Close",
                Some(ferrite_ui::keymap::READER),
            ),
        )
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
            .key_context(ferrite_ui::keymap::READER)
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
            .on_action(cx.listener(move |view, _: &ferrite_review::Close, _, cx| {
                if let Some(index) = view.pane_for(owner) {
                    view.close_review(index, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    if let Some(index) = view.pane_for(owner) {
                        view.focus_pane(index, cx);
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
        Some(Self::slot_drop_target(reader, leaf, cx))
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
        let read = ferrite_ui::answer::answered(answer, cx).await;
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

/// The board laid out by a split tree: the shown Pane's, while it has a
/// reader open beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Board(pub(super) PaneIdentity);

/// What one leaf of a board's tree stands for: a Pane, or the reader slot
/// a Thread's Pane has open, its Review (one per Pane).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Slot {
    Pane(PaneIdentity),
    Reader(ThreadKey),
}

/// How wide the grab band over a seam is, centred on the gutter: the
/// prototype's 9px band (`.seam{width:9px;margin-left:-4px}`), a pixel past
/// the 8px gutter.
pub(super) const SEAM_GRAB: f32 = 9.0;

/// A slot's leaf in a board's tree. A Thread's Pane is its own id; drafts
/// and reader slots are no Threads and never persist, so stand-in ids
/// nothing real reaches name them: a draft counts down from the top, and a
/// reader slot is its Thread's number under the `READER_LEAF` bit. A board
/// holds one Pane and its reader, so a leaf maps back to its slot through
/// the board (`leaf_slot`).
pub(super) type Leaf = ThreadId;

pub(super) const READER_LEAF: u64 = 1 << 62;

/// A Review's share of its Pane's slot when it first opens beside it.
pub(super) const READER_SHARE: f32 = ferrite_ui::theme::READER_SHARE;

pub(super) fn pane_leaf(identity: PaneIdentity) -> Leaf {
    match identity {
        PaneIdentity::Thread(thread) => thread.thread(),
        PaneIdentity::Draft(draft) => ThreadId::new(u64::MAX - draft.get()),
    }
}

pub(super) fn reader_leaf(owner: ThreadKey) -> Leaf {
    ThreadId::new(READER_LEAF | owner.thread().get())
}

/// A drag move's pointer and the hovered slot's bounds, in the tree's units.
pub(super) fn drop_geometry(
    position: gpui::Point<Pixels>,
    bounds: gpui::Bounds<Pixels>,
) -> (layout::Point, layout::Rect) {
    (
        layout::Point {
            x: f32::from(position.x),
            y: f32::from(position.y),
        },
        layout::Rect {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            w: f32::from(bounds.size.width),
            h: f32::from(bounds.size.height),
        },
    )
}

/// A slot's head as its drag handle: the whole band picks the slot up,
/// with its ghost riding the pointer.
pub(super) fn head_drag(leaf: Leaf, face: GhostFace) -> impl FnOnce(Div) -> AnyElement {
    move |head: Div| {
        head.id(("slot-head", leaf.get() as usize))
            .cursor(gpui::CursorStyle::OpenHand)
            .on_drag(PaneDrag { leaf }, move |_, grab, _, cx| {
                let face = face.clone();
                cx.new(|_| PaneGhost {
                    face,
                    grab,
                })
            })
            .into_any_element()
    }
}

impl CockpitView {
    /// The board the cards lay out in, in window coordinates (theme WP-C):
    /// flush right of the nav, from under the head row
    /// (`frame::board_top`) to the window's foot, and `board_right` short
    /// of its right edge (the panel, or the gutter).
    pub(super) fn board_bounds(&self, window: &Window, cx: &gpui::App) -> layout::Rect {
        let bounds = self.board_rect(window.viewport_size(), cx);
        self.board.set(bounds);
        bounds
    }

    /// The board at a window of `viewport`'s size (`board_bounds`), kept
    /// nowhere: what a read of the root may take.
    pub(super) fn board_rect(&self, viewport: gpui::Size<Pixels>, cx: &gpui::App) -> layout::Rect {
        let top = ferrite_ui::frame::board_top();
        let left = self.nav_width();
        let bounds = layout::Rect {
            x: left,
            y: top,
            w: (f32::from(viewport.width) - left - self.board_right(cx)).max(0.0),
            h: (f32::from(viewport.height) - top).max(0.0),
        };
        bounds
    }

    /// The board on screen, when a tree lays it out: the shown Pane's,
    /// while it has a reader slot. A plain Pane has none.
    pub(super) fn board(&self, cx: &gpui::App) -> Option<Board> {
        match self.visible_indices(cx).as_slice() {
            [index] if self.has_reader(*index) => Some(Board(self.panes[*index].identity)),
            _ => None,
        }
    }

    /// The slot `leaf` stands for on `board`: its Pane, or the reader slot
    /// its Thread has open. None for a leaf of neither. Only the board can
    /// say: a leaf is a bare id, and two machines' Threads share them.
    pub(super) fn leaf_slot(board: Board, leaf: Leaf) -> Option<Slot> {
        if pane_leaf(board.0) == leaf {
            return Some(Slot::Pane(board.0));
        }
        let PaneIdentity::Thread(owner) = board.0 else {
            return None;
        };
        (reader_leaf(owner) == leaf).then_some(Slot::Reader(owner))
    }

    /// A board's tree as it should draw right now (`BoardLayouts::tree`),
    /// its readers the shown Panes' with a document open or sliding shut.
    pub(super) fn board_tree(&self, board: Board, cx: &gpui::App) -> Option<Tree> {
        Some(self.board_layouts.tree(board, || {
            self.visible_indices(cx)
                .into_iter()
                .filter(|index| self.has_reader(*index))
                .filter_map(|index| self.panes[index].thread())
                .collect()
        }))
    }

    /// The shown Pane's rect on the board, in window coordinates — from the
    /// board's tree as this frame draws it (or, measuring at rest, where
    /// the reader's slide ends), or the whole board. The reader slot is no
    /// Pane and is left out.
    pub(super) fn pane_rects(&self, window: &Window, cx: &gpui::App) -> Vec<(usize, layout::Rect)> {
        self.pane_rects_in(self.board_bounds(window, cx), cx)
    }

    /// The shown Panes' rects (`pane_rects`) on a board at `bounds`.
    pub(super) fn pane_rects_in(&self, bounds: layout::Rect, cx: &gpui::App) -> Vec<(usize, layout::Rect)> {
        if let Some(board) = self.board(cx) {
            let Some(tree) = self.board_tree(board, cx) else {
                return Vec::new();
            };
            let tree = if self.columns.measuring_rest() {
                self.rest_tree(tree)
            } else {
                self.slid_tree(tree, bounds)
            };
            return tree
                .rects(bounds, ferrite_ui::theme::BOARD_SEAM)
                .into_iter()
                .filter_map(|(leaf, rect)| match Self::leaf_slot(board, leaf)? {
                    Slot::Pane(identity) => self.index_of(identity).map(|index| (index, rect)),
                    Slot::Reader(_) => None,
                })
                .collect();
        }
        self.visible_indices(cx)
            .first()
            .map(|index| vec![(*index, bounds)])
            .unwrap_or_default()
    }

    /// A board: Panes and readers at their tree rects as this frame draws
    /// them (`slid_tree`: a reader sliding takes the room it has so far),
    /// seams over the gaps, the drop preview on top. Coordinates are the
    /// board's own — the frame is `relative`, its origin the nav's right
    /// edge — so the rects the tree computes for the window are shifted
    /// back by that origin.
    pub(super) fn tree_board(
        &self,
        on: Board,
        tree: Tree,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let bounds = self.board_bounds(window, cx);
        let origin_x = self.nav_width();
        let local = |rect: layout::Rect| layout::Rect {
            x: rect.x - origin_x,
            ..rect
        };
        let mut board = div().relative().flex_1().min_w_0().min_h_0();
        let rest: std::collections::HashMap<Leaf, layout::Rect> = tree
            .rects(bounds, ferrite_ui::theme::BOARD_SEAM)
            .into_iter()
            .collect();
        let tree = self.slid_tree(tree, bounds);
        // A slot slid all the way shut takes no room and draws nothing.
        let rects: Vec<(Leaf, layout::Rect)> = tree
            .rects(bounds, ferrite_ui::theme::BOARD_SEAM)
            .into_iter()
            .filter(|(_, rect)| rect.w > 0.0 && rect.h > 0.0)
            .collect();
        self.slot_drag
            .laid_out(rects.iter().map(|(leaf, rect)| (*leaf, (rect.w, rect.h))));
        for (leaf, rect) in rects {
            let Some(slot) = Self::leaf_slot(on, leaf) else {
                continue;
            };
            let cell = match slot {
                Slot::Pane(identity) => {
                    let Some(index) = self.index_of(identity) else {
                        continue;
                    };
                    self.pane_slot(index, window, cx)
                }
                Slot::Reader(owner) => {
                    let pin = rest
                        .get(&leaf)
                        .and_then(|at_rest| beside::reader_pin(&tree, leaf, rect, *at_rest));
                    let Some(cell) = self.reader_cell(owner, leaf, pin, cx) else {
                        continue;
                    };
                    // A reader is a card beside the Pane, at rest.
                    cell.relative()
                        .child(pane::card_ring(ferrite_ui::theme::paint::CARD_RING))
                }
            };
            // The cell lies on its track as laid out; gpui rounds its edges
            // to the pixel where it lands, and its lines from the track's
            // own fraction, as the prototype's browser lays a grid's cells.
            let rect = local(rect);
            // The grabbed slot stays put, dimmed as lifted out of its slot,
            // while its ghost travels: the board keeps its shape until the
            // drop changes it.
            let cell = if self.slot_drag.lifted() == Some(leaf) {
                cell.opacity(ferrite_ui::theme::DRAG_SOURCE_OPACITY)
                    .debug_selector(move || format!("pane-drag-source-{}", leaf.get()))
            } else {
                cell
            };
            board = board.child(
                cell.absolute()
                    .left(px(rect.x))
                    .top(px(rect.y))
                    .w(px(rect.w))
                    .h(px(rect.h)),
            );
        }
        // The seams (F-18): one line per linked set down the gutter's
        // centre — a default grid's aligned column seams are one line from
        // the board's top to its foot — and a 9px grab band over each seam.
        // At rest the line is nothing: the gutter separates the cards.
        // Under the pointer (any band of the set) and while held, the whole
        // line draws `SEAM_LINE_W` of `SEAM_HOVER`, fading in over
        // `motion::HOVER_FADE`.
        let seams = tree.seams(bounds, ferrite_ui::theme::BOARD_SEAM, SEAM_GRAB);
        let mut drawn: Vec<SeamId> = Vec::new();
        for seam in &seams {
            if drawn.contains(&seam.id) {
                continue;
            }
            let group = tree.linked(&seam.id, bounds, ferrite_ui::theme::BOARD_SEAM);
            let members: Vec<&layout::Seam> = seams
                .iter()
                .filter(|other| group.contains(&other.id))
                .collect();
            drawn.extend(group.iter().cloned());
            let key = SharedString::from(format!(
                "seam-{}",
                seam.id
                    .0
                    .iter()
                    .map(|step| if *step { '1' } else { '0' })
                    .collect::<String>()
            ));
            let dragging = self
                .board_layouts
                .holds_seam(on, members.iter().map(|member| &member.id));
            // The line spans the union of the set's bands, along its axis.
            let line_w = ferrite_ui::theme::SEAM_LINE_W;
            let line = match seam.axis {
                layout::Axis::Row => {
                    let top = members
                        .iter()
                        .map(|member| member.band.y)
                        .fold(f32::INFINITY, f32::min);
                    let bottom = members
                        .iter()
                        .map(|member| member.band.y + member.band.h)
                        .fold(f32::NEG_INFINITY, f32::max);
                    let band = local(seam.band);
                    layout::Rect {
                        x: band.x + (band.w - line_w) / 2.0,
                        y: top,
                        w: line_w,
                        h: bottom - top,
                    }
                }
                layout::Axis::Column => {
                    let left = members
                        .iter()
                        .map(|member| local(member.band).x)
                        .fold(f32::INFINITY, f32::min);
                    let right = members
                        .iter()
                        .map(|member| local(member.band).x + member.band.w)
                        .fold(f32::NEG_INFINITY, f32::max);
                    layout::Rect {
                        x: left,
                        y: seam.band.y + (seam.band.h - line_w) / 2.0,
                        w: right - left,
                        h: line_w,
                    }
                }
            };
            let accent = if dragging {
                rgba(ferrite_ui::theme::SEAM_HOVER).into()
            } else {
                ferrite_ui::motion::hover_blend(
                    &key,
                    rgba(ferrite_ui::theme::TRANSPARENT).into(),
                    rgba(ferrite_ui::theme::SEAM_HOVER).into(),
                )
            };
            board = board.child(
                div()
                    .debug_selector({
                        let key = key.clone();
                        move || format!("board-{key}")
                    })
                    .absolute()
                    .left(px(line.x))
                    .top(px(line.y))
                    .w(px(line.w))
                    .h(px(line.h))
                    .bg(accent),
            );
            for (at, member) in members.iter().enumerate() {
                let band = local(member.band);
                let id = member.id.clone();
                let cursor = match member.axis {
                    layout::Axis::Row => gpui::CursorStyle::ResizeLeftRight,
                    layout::Axis::Column => gpui::CursorStyle::ResizeUpDown,
                };
                board = board.child(
                    div()
                        .id(gpui::ElementId::Name(SharedString::from(format!(
                            "{key}-band-{at}"
                        ))))
                        .debug_selector({
                            let key = key.clone();
                            move || format!("board-{key}-band-{at}")
                        })
                        .absolute()
                        .left(px(band.x))
                        .top(px(band.y))
                        .w(px(band.w))
                        .h(px(band.h))
                        .cursor(cursor)
                        .on_hover(ferrite_ui::motion::hover_listener(key.clone()))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                view.begin_seam_drag(on, id.clone(), cx);
                            }),
                        ),
                );
            }
        }
        if let Some((target, zone)) = self.slot_drag.preview() {
            if let Some((_, rect)) = tree
                .rects(bounds, ferrite_ui::theme::BOARD_SEAM)
                .into_iter()
                .find(|(leaf, _)| *leaf == target)
            {
                // The slot a release would fill: a white wash at the card's
                // radius with the focus ring's edge, and a raised tag naming
                // the verb.
                let wash = local(layout::zone_rect(rect, zone));
                board = board.child(
                    div()
                        .absolute()
                        .left(px(wash.x))
                        .top(px(wash.y))
                        .w(px(wash.w))
                        .h(px(wash.h))
                        .rounded(px(ferrite_ui::theme::R_CARD))
                        .bg(rgba(ferrite_ui::theme::DROP_WASH))
                        .border_1()
                        .border_color(rgba(ferrite_ui::theme::DROP_EDGE))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .px(px(ferrite_ui::theme::DROP_LABEL_PAD_X))
                                .py(px(ferrite_ui::theme::DROP_LABEL_PAD_Y))
                                .bg(ferrite_ui::theme::paint::FLOAT)
                                .rounded(px(ferrite_ui::theme::R_CONTROL))
                                .border_1()
                                .border_color(ferrite_ui::theme::paint::FLOAT_EDGE)
                                .shadow(ferrite_ui::components::float_shadow())
                                .font_family(ferrite_ui::theme::FONT_UI)
                                .text_size(px(ferrite_ui::theme::FS_UI))
                                .line_height(px(ferrite_ui::theme::LH_UI))
                                .text_color(rgb(ferrite_ui::theme::INK_STRONG))
                                .child(match zone {
                                    Zone::Swap => "swap",
                                    Zone::Split(Edge::Left) => "split left",
                                    Zone::Split(Edge::Right) => "split right",
                                    Zone::Split(Edge::Top) => "split above",
                                    Zone::Split(Edge::Bottom) => "split below",
                                }),
                        ),
                );
            }
        }
        board
    }

    /// A press on a seam: the drag begins from the persisted tree, and
    /// the moves until release re-derive the ratio from the pointer.
    pub(super) fn begin_seam_drag(&mut self, board: Board, seam: SeamId, cx: &mut Context<Self>) {
        self.land_reader_slides(cx);
        let Some(tree) = self.board_tree(board, cx) else {
            return;
        };
        self.board_layouts.begin_seam_drag(board, seam, tree);
        cx.notify();
    }

    /// The pointer moved with a seam held: the seam follows it, and only
    /// the two Panes touching the seam change size — the tree keeps every
    /// other Pane where it was and neither neighbour under its floor.
    pub(super) fn drag_seam(
        &mut self,
        position: gpui::Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.board_bounds(window, cx);
        let pointer = layout::Point {
            x: f32::from(position.x),
            y: f32::from(position.y),
        };
        if self.board_layouts.drag_seam(bounds, pointer) {
            cx.notify();
        }
    }

    /// The release: the dragged tree persists, once.
    pub(super) fn end_seam_drag(&mut self, cx: &mut Context<Self>) {
        if self.board_layouts.end_seam_drag() {
            cx.notify();
        }
    }

    /// A slot dragged over another: what a release here would do, for the
    /// wash — nothing over itself, nothing across boards.
    pub(super) fn preview_pane_drop(
        &mut self,
        source: Leaf,
        target: Leaf,
        position: gpui::Point<Pixels>,
        bounds: gpui::Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let on_board = self
            .board(cx)
            .and_then(|board| self.board_tree(board, cx))
            .is_some_and(|tree| tree.contains(source) && tree.contains(target));
        let preview = (source != target && on_board).then(|| {
            let (pointer, rect) = drop_geometry(position, bounds);
            (target, layout::zone(pointer, rect))
        });
        if self.slot_drag.set_preview(preview) {
            cx.notify();
        }
    }

    /// The release of a dragged slot on another: the centre swaps the two
    /// leaves, an edge moves the source beside the target — the tree is
    /// kept either way. Reads the preview the last move computed.
    pub(super) fn drop_pane(&mut self, source: Leaf, target: Leaf, cx: &mut Context<Self>) {
        let Some(zone) = self.slot_drag.drop_on(target) else {
            cx.notify();
            return;
        };
        self.apply_pane_drop(source, target, zone, cx);
    }

    /// The tree change a drop makes, persisted through the core door.
    pub(super) fn apply_pane_drop(
        &mut self,
        source: Leaf,
        target: Leaf,
        zone: Zone,
        cx: &mut Context<Self>,
    ) {
        let Some(board) = self.board(cx) else {
            return;
        };
        let Some(mut tree) = self.board_tree(board, cx) else {
            return;
        };
        if source == target || !tree.contains(source) || !tree.contains(target) {
            return;
        }
        let changed = match zone {
            Zone::Swap => tree.swap(source, target),
            Zone::Split(edge) => tree.split(target, edge, source),
        };
        if changed {
            self.board_layouts.commit(board, tree);
        }
        cx.notify();
    }

    /// A board slot as a drop target for a reader dragged by its head: the
    /// moves over it compute the preview, the release applies it.
    pub(super) fn slot_drop_target(cell: Div, target: Leaf, cx: &mut Context<Self>) -> Div {
        cell.on_drag_move(
            cx.listener(move |view, event: &gpui::DragMoveEvent<PaneDrag>, _, cx| {
                let source = event.drag(cx).leaf;
                view.slot_dragged(source, target, event.event.position, event.bounds, cx);
            }),
        )
        .on_drop(cx.listener(move |view, drag: &PaneDrag, _, cx| {
            view.drop_pane(drag.leaf, target, cx);
        }))
    }

    /// Slot `source`, dragged by its head, moved with the pointer at `at`
    /// while `target`'s slot stood at `bounds`. gpui hands every drag move
    /// to every listener, not just the one under the pointer: a slot only
    /// previews while the pointer is inside it, and drops its own preview
    /// once the pointer has left — else the first slot crossed keeps
    /// saying "split left" while the drag is somewhere else.
    pub(super) fn slot_dragged(
        &mut self,
        source: Leaf,
        target: Leaf,
        at: gpui::Point<Pixels>,
        bounds: gpui::Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.slot_drag.lift(source) {
            cx.notify();
        }
        if bounds.contains(&at) {
            self.preview_pane_drop(source, target, at, bounds, cx);
        } else if self.slot_drag.left(target) {
            cx.notify();
        }
    }
}
