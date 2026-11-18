//! The transcript's small scroll facade over GPUI's lazy variable-height list.

use ferrite_ui::motion::{MotionSpec, Tween, EASE_STANDARD};
use gpui::{px, FollowMode, ListAlignment, ListOffset, ListState, Pixels};
#[cfg(any(test, feature = "test-support"))]
use gpui::{Bounds, Point};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Instant,
};

use super::rows::{RowDelta, RowId, TranscriptRows};

/// GPUI measures the viewport plus this small buffer, not the full transcript.
const OVERDRAW: Pixels = px(200.);

/// One transcript viewport's native scroll state.
#[derive(Clone, Debug)]
pub struct TranscriptScroll {
    list: ListState,
    // The passage the reader holds, sampled from native layout for width reflow.
    reading: Rc<RefCell<Option<ReadingPlace>>>,
    // How much of the viewport's top the pinned band covers, sampled after
    // layout (find reveals below it).
    top_mask: Rc<Cell<Pixels>>,
    // A programmatic scroll easing to its target (`ease_to`).
    ease: Rc<RefCell<Option<ScrollEase>>>,
    // How many scrollbar gestures the list had counted when last asked
    // (`take_scrollbar_gesture`).
    gestures_seen: Rc<Cell<u64>>,
}

/// Where in the readable view the held passage is read: a third of the way
/// down, where Find sets a hit it reveals.
const READING_LINE: f32 = 1. / 3.;

/// The passage the reader holds while the layout reflows: the point `along`
/// of the way down row `row` (0 its top, 1 its foot) sits `line` below the
/// viewport's top, as measured at `width`. The row is named by identity, so a
/// row spliced in or out above it cannot aim the restore at another row, and
/// the point is a fraction of the row, so a tall answer that rewraps taller
/// or shorter keeps the same passage under the reader's eye instead of the
/// same pixel depth.
#[derive(Clone, Debug)]
struct ReadingPlace {
    row: RowId,
    along: f32,
    line: Pixels,
    width: Pixels,
}

/// What a layout did to the viewport (`TranscriptScroll::settle_layout`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Settled {
    /// The reader's passage was put back after a reflow: lay out again.
    pub anchored: bool,
    pub ease: EaseStep,
}

/// Where a programmatic scroll eases to: a row's offset (a send's prompt),
/// or the tail (Scroll to end). A row is named by its identity, not its
/// index: rows spliced in or out above it move the index, not the row.
#[derive(Clone, Debug)]
pub(crate) enum EaseTarget {
    Row { id: RowId, offset: Pixels },
    End,
}

/// A programmatic scroll on its way: a tween on the distance still to go,
/// re-measured every frame, so rows growing under it are absorbed (T3's
/// settle loop). Its tween starts on the first frame that can measure the
/// target.
#[derive(Clone, Debug)]
struct ScrollEase {
    target: EaseTarget,
    tween: Option<Tween>,
}

/// What a frame did with the ease (`TranscriptScroll::step_ease`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EaseStep {
    /// No ease is running.
    Idle,
    /// It moved and wants the next frame.
    Moving,
    /// It reached its target: the offset is exact from here.
    Landed,
    /// The operator took the scroll, or its row left the transcript: it
    /// stopped where it stands.
    Cancelled,
}

/// A programmatic scroll's motion for a move of `distance` px: Chromium's
/// smooth scroll, `min(√|Δ|, 12) / 60` s (at most `MOTION_SCROLL_MAX_MS`,
/// reached from 144px), on `EASE_STANDARD` (T3's smooth `scrollTo`).
pub(crate) fn scroll_spec(distance: f32) -> MotionSpec {
    let distance = distance.abs();
    let ms = if distance < 0.5 {
        0
    } else {
        ((distance.sqrt().min(12.) / 60. * 1000.).round() as u64)
            .min(ferrite_ui::theme::MOTION_SCROLL_MAX_MS)
    };
    MotionSpec::new(ms, EASE_STANDARD)
}

impl TranscriptScroll {
    /// Start at the live tail, top-aligned: rows shorter than the viewport
    /// read down from its top, as a new conversation does. A
    /// user scroll away suspends tail following until they return to the
    /// end; GPUI owns that transition.
    pub(crate) fn new(row_count: usize) -> Self {
        let list = ListState::new(row_count, ListAlignment::Top, OVERDRAW);
        list.set_follow_mode(FollowMode::Tail);
        Self {
            list,
            reading: Default::default(),
            top_mask: Rc::new(Cell::new(px(0.))),
            ease: Default::default(),
            gestures_seen: Rc::new(Cell::new(0)),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn offset(&self) -> Point<Pixels> {
        self.list.scroll_px_offset_for_scrollbar()
    }

    /// Sets a list-native pixel offset, used by scrollbar gestures and tests.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_offset(&self, offset: Point<Pixels>) {
        self.list.set_offset_from_scrollbar(offset);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn max_offset(&self) -> Point<Pixels> {
        self.list.max_offset_for_scrollbar()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.list.viewport_bounds()
    }

    /// Whether the reader used the scrollbar (a thumb grab or move, a track
    /// click) since this was last asked. The scrollbar writes the offset
    /// itself, so this is how the transcript learns it lost the scroll.
    pub(crate) fn take_scrollbar_gesture(&self) -> bool {
        let now = self.list.scrollbar_gestures();
        self.gestures_seen.replace(now) != now
    }

    /// Resume native tail following and reveal the newest row.
    pub(crate) fn scroll_to_bottom(&self) {
        self.ease.take();
        self.list.set_follow_mode(FollowMode::Tail);
        self.list.scroll_to_end();
    }

    /// Hold the list at `item_ix`, `offset` into it (a scene's turn band, a
    /// minimap jump): tail following pauses until the reader returns to the
    /// end.
    pub(crate) fn scroll_to(&self, item_ix: usize, offset: Pixels) {
        self.ease.take();
        self.list.scroll_to(ListOffset {
            item_ix,
            offset_in_item: offset,
        });
    }

    /// Bring row `index`'s top just below the pinned band, unless its top is
    /// already readable there. Says whether it moved the view.
    pub(crate) fn scroll_top_below_band(&self, index: usize) -> bool {
        let viewport = self.list.viewport_bounds();
        let floor = viewport.top() + self.top_mask.get();
        let placed = self
            .list
            .bounds_for_item(index)
            .is_some_and(|bounds| bounds.top() >= floor && bounds.top() < viewport.bottom());
        if placed {
            return false;
        }
        self.scroll_to(index, px(0.));
        self.list.scroll_by(-self.top_mask.get());
        true
    }

    /// Scroll a page up or down: the viewport below the pinned band (the
    /// rows pass under it), less the line kept in view of the page before.
    /// Reaching the end resumes following it.
    pub(crate) fn scroll_page(&self, down: bool, keep: Pixels) {
        self.ease.take();
        let readable = self.list.viewport_bounds().size.height - self.top_mask.get();
        let page = (readable - keep).max(keep);
        self.list.scroll_by(if down { page } else { -page });
    }

    /// Ease to `target` over the frames to come (`step_ease` moves it),
    /// replacing any ease on its way. Tail following pauses meanwhile: the
    /// ease, not the list, moves the offset, and landing on the end resumes
    /// it.
    pub(crate) fn ease_to(&self, target: EaseTarget) {
        self.list.pause_following_tail();
        *self.ease.borrow_mut() = Some(ScrollEase {
            target,
            tween: None,
        });
    }

    /// Whether an ease is on its way.
    pub fn easing(&self) -> bool {
        self.ease.borrow().is_some()
    }

    /// Stop an ease where it stands (the operator scrolled): whether one
    /// was running.
    pub(crate) fn cancel_ease(&self) -> bool {
        self.ease.take().is_some()
    }

    /// One frame of the ease, after a layout: re-measure what is left to
    /// go, and scroll so that what is left is the tween's value now. A
    /// target the list has not measured is snapped to; one whose row has
    /// left the transcript ends the ease where it stands.
    pub(crate) fn step_ease(&self, now: Instant, rows: &TranscriptRows) -> EaseStep {
        let Some(mut ease) = self.ease.borrow().clone() else {
            return EaseStep::Idle;
        };
        if self.list.is_scrollbar_dragging() {
            self.ease.take();
            return EaseStep::Cancelled;
        }
        let viewport = self.list.viewport_bounds();
        let mut row = None;
        let left = match &ease.target {
            EaseTarget::Row { id, offset } => {
                let Some(item_ix) = rows.position(id) else {
                    self.ease.take();
                    return EaseStep::Cancelled;
                };
                row = Some((item_ix, *offset));
                // The end can reach the offset before the room under it is
                // laid out: the list would follow it, and the ease moves it.
                self.list.pause_following_tail();
                self.list
                    .bounds_for_item(item_ix)
                    .map(|bounds| bounds.top() + *offset - viewport.top())
            }
            EaseTarget::End if self.list.is_following_tail() => Some(px(0.)),
            EaseTarget::End => Some(
                self.list.max_offset_for_scrollbar().y
                    + self.list.scroll_px_offset_for_scrollbar().y,
            ),
        };
        let Some(mut left) = left else {
            self.land(row);
            return EaseStep::Landed;
        };
        let tween = match ease.tween {
            Some(tween) => tween,
            None => {
                // A far end: the rows between are not measured, so come
                // within a viewport of it before easing the rest.
                let reach = viewport.size.height.max(px(1.));
                if left.abs() > reach {
                    let jump = left - reach * left.signum();
                    self.list.scroll_by(jump);
                    left -= jump;
                }
                let tween = Tween::new(f32::from(left), 0., scroll_spec(f32::from(left)), now);
                ease.tween = Some(tween);
                tween
            }
        };
        if !tween.running(now, false) {
            self.land(row);
            return EaseStep::Landed;
        }
        self.list.scroll_by(left - px(tween.value(now, false)));
        *self.ease.borrow_mut() = Some(ease);
        EaseStep::Moving
    }

    /// The ease is done: its target exactly (`row`, an index and offset;
    /// the end when there is none), and at the end following it.
    fn land(&self, row: Option<(usize, Pixels)>) {
        match row {
            Some((item_ix, offset)) => self.scroll_to(item_ix, offset),
            None => self.scroll_to_bottom(),
        }
    }

    pub fn is_following_tail(&self) -> bool {
        self.list.is_following_tail()
    }

    pub fn item_is_visible(&self, index: usize) -> bool {
        let viewport = self.list.viewport_bounds();
        self.list.bounds_for_item(index).is_some_and(|bounds| {
            bounds.bottom() > viewport.top() && bounds.top() < viewport.bottom()
        })
    }

    /// What this layout moves of the viewport on its own, in one place: the
    /// reader's passage put back after a reflow, then the ease on its way.
    pub(crate) fn settle_layout(&self, now: Instant, rows: &TranscriptRows) -> Settled {
        let anchored = self.hold_reading_place(rows);
        Settled {
            anchored,
            ease: self.step_ease(now, rows),
        }
    }

    /// The navigation took the viewport: no passage is held until the next
    /// layout samples one.
    pub(crate) fn release_reading_place(&self) {
        self.reading.take();
    }

    /// How far below the viewport's top a passage is read.
    fn reading_line(&self) -> Pixels {
        let mask = self.top_mask.get();
        mask + (self.list.viewport_bounds().size.height - mask) * READING_LINE
    }

    /// The passage drawn at window height `y`, as the place to hold it at
    /// the reading line.
    fn place_at(&self, y: Pixels, rows: &TranscriptRows) -> Option<ReadingPlace> {
        let top = self.list.logical_scroll_top();
        (top.item_ix..self.list.item_count()).find_map(|index| {
            let bounds = self.list.bounds_for_item(index)?;
            let depth = y - bounds.top();
            if depth < px(0.) || depth >= bounds.size.height {
                return None;
            }
            Some(ReadingPlace {
                row: rows.get(index)?.id().clone(),
                along: depth / bounds.size.height.max(px(1.)),
                line: self.reading_line(),
                width: self.list.viewport_bounds().size.width,
            })
        })
    }

    /// Bring the passage drawn at window height `y` to the reading line and
    /// hold it there. The move is a place in the passage's row as well as a
    /// distance: `y` was painted by the frame before, and a layout that
    /// rewraps the row before the move shows (a column sliding in or out)
    /// would land that distance on another passage.
    pub(crate) fn settle_at_reading_line(&self, y: Pixels, rows: &TranscriptRows) {
        let place = self.place_at(y, rows);
        let line = self.list.viewport_bounds().top() + self.reading_line();
        self.list.scroll_by(y - line);
        *self.reading.borrow_mut() = place;
    }

    /// Put the passage the reader holds back where it was when wrapping
    /// changes it, then sample the passage now held. GPUI still owns every
    /// offset, measurement and tail-follow transition.
    fn hold_reading_place(&self, rows: &TranscriptRows) -> bool {
        let viewport = self.list.viewport_bounds();
        if self.list.is_following_tail() {
            self.reading.take();
            return false;
        }
        let mut adjusted = false;
        let held = self.reading.borrow().clone();
        if let Some(place) = held.filter(|place| place.width != viewport.size.width) {
            if let Some(index) = rows.position(&place.row) {
                if let Some(bounds) = self.list.bounds_for_item(index) {
                    let delta = bounds.top() + bounds.size.height * place.along
                        - viewport.top()
                        - place.line;
                    if delta.abs() > px(0.5) {
                        self.list.scroll_by(delta);
                        adjusted = true;
                    }
                } else {
                    // Reflow can move the held row beyond native overdraw.
                    // Measure it before restoring the original place.
                    self.list.scroll_to_reveal_item(index);
                    return true;
                }
            }
        }
        let top = self.list.logical_scroll_top();
        let place = (top.item_ix != 0 || top.offset_in_item > px(0.))
            .then(|| self.place_at(viewport.top() + self.reading_line(), rows))
            .flatten();
        *self.reading.borrow_mut() = place;
        adjusted
    }

    /// The rows' inset moved with the width (`theme::tx_inset_at`), a frame
    /// after the width did: measure every row again, holding the reader's
    /// passage where it stands.
    pub(crate) fn remeasure_held(&self) {
        if let Some(place) = self.reading.borrow_mut().as_mut() {
            place.width = px(-1.);
        }
        self.remeasure_all();
    }

    /// How much of the viewport's top the pinned band covers (`band`, its
    /// foot below the viewport's top; `None` when no band is pinned). Rows
    /// pass under it into its fade (`EDGE_FADE`), so nothing is cut on a
    /// line. Whether it changed, so the caller repaints once.
    pub(crate) fn set_top_mask(&self, band: Option<Pixels>) -> bool {
        let mask = band.map_or(px(0.), |band| band.max(px(0.)));
        let changed = (self.top_mask.get() - mask).abs() > px(0.5);
        self.top_mask.set(mask);
        changed
    }

    /// What the pinned band covers of the viewport's top (`set_top_mask`).
    pub fn top_mask(&self) -> Pixels {
        self.top_mask.get()
    }

    /// Freeze tail following while retaining its automatic re-engagement rule.
    #[cfg(any(test, feature = "test-support"))]
    pub fn pause_following_tail(&self) {
        self.list.pause_following_tail();
    }

    /// The native state passed to `gpui::list` and the existing scrollbar.
    pub fn list_state(&self) -> &ListState {
        &self.list
    }

    /// Disclosure changes invalidate heights while preserving the pixel anchor.
    pub(crate) fn remeasure_all(&self) {
        self.list.remeasure_items(0..self.list.item_count());
    }

    /// Apply row changes without resetting the logical scroll anchor.
    pub(crate) fn reconcile(&self, delta: &RowDelta) {
        for splice in &delta.splices {
            self.list.splice(splice.old_range.clone(), splice.new_count);
        }
        for &index in &delta.remeasure {
            if index < self.list.item_count() {
                self.list.remeasure_items(index..index + 1);
            }
        }
        // Nothing else moves: a row's gap is part of the row (see
        // `rows::gap_before`), so a neighbour's arrival or eviction reaches
        // the list as that row's own change in `remeasure`.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::rows::{RowKind, RowShape};
    use ferrite_core::transcript::{Input, Transcript};

    fn rows_of(prompts: &[&str]) -> TranscriptRows {
        let mut transcript = Transcript::default();
        for prompt in prompts {
            transcript.apply(Input::Prompt((*prompt).into()));
        }
        TranscriptRows::new(
            transcript.blocks(),
            &RowShape::default(),
            ferrite_ui::theme::FS_PROSE,
        )
    }

    /// A prompt's ease on its way when the prompt leaves the transcript (a
    /// rewind): the ease ends where it stands and nothing takes the viewport
    /// later, not even the tail.
    #[test]
    fn an_ease_whose_row_left_the_transcript_ends_where_it_stands() {
        let scroll = TranscriptScroll::new(2);
        let before = rows_of(&["one", "two"]);
        let id = before
            .rows()
            .iter()
            .rfind(|row| row.kind() == RowKind::Prompt)
            .unwrap()
            .id()
            .clone();
        scroll.ease_to(EaseTarget::Row { id, offset: px(0.) });
        assert!(scroll.easing());
        let stood = scroll.offset();
        let after = rows_of(&["one"]);
        assert_eq!(
            scroll.step_ease(Instant::now(), &after),
            EaseStep::Cancelled
        );
        assert!(!scroll.easing());
        assert_eq!(scroll.step_ease(Instant::now(), &after), EaseStep::Idle);
        assert_eq!(scroll.offset(), stood);
        assert!(!scroll.is_following_tail(), "the tail did not take it");
    }

    #[test]
    fn a_programmatic_scroll_takes_chromiums_smooth_scroll_length() {
        assert_eq!(scroll_spec(0.).duration_ms, 0);
        assert_eq!(scroll_spec(36.).duration_ms, 100);
        assert_eq!(scroll_spec(-36.).duration_ms, 100);
        assert_eq!(scroll_spec(144.).duration_ms, 200);
        assert_eq!(scroll_spec(2000.).duration_ms, 200);
        assert_eq!(scroll_spec(144.).curve, ferrite_ui::motion::EASE_STANDARD);
    }
}
