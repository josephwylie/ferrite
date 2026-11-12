//! The transcript's small scroll facade over GPUI's lazy variable-height list.

use ferrite_ui::motion::{MotionSpec, Tween, EASE_STANDARD};
use gpui::{px, FollowMode, ListAlignment, ListOffset, ListState, Pixels};
#[cfg(any(test, feature = "test-support"))]
use gpui::{Bounds, Point};
use std::{cell::Cell, rc::Rc, time::Instant};

use super::rows::RowDelta;

/// GPUI measures the viewport plus this small buffer, not the full transcript.
const OVERDRAW: Pixels = px(200.);

/// One transcript viewport's native scroll state.
#[derive(Clone, Debug)]
pub struct TranscriptScroll {
    list: ListState,
    // A fully visible row's inset, sampled from native layout for width reflow.
    resize_anchor: Rc<Cell<Option<(Pixels, usize, Pixels)>>>,
    // How much of the viewport's top the pinned band covers, sampled after
    // layout (find reveals below it).
    top_mask: Rc<Cell<Pixels>>,
    // A programmatic scroll easing to its target (`ease_to`).
    ease: Rc<Cell<Option<ScrollEase>>>,
}

/// Where a programmatic scroll eases to: a row's offset (a send's prompt),
/// or the tail (Scroll to end).
#[derive(Clone, Copy, Debug)]
pub(crate) enum EaseTarget {
    Row(ListOffset),
    End,
}

/// A programmatic scroll on its way: a tween on the distance still to go,
/// re-measured every frame, so rows growing under it are absorbed (T3's
/// settle loop). Its tween starts on the first frame that can measure the
/// target.
#[derive(Clone, Copy, Debug)]
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
    /// The operator took the scroll (a scrollbar drag): it stopped where it
    /// stands.
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
            resize_anchor: Default::default(),
            top_mask: Rc::new(Cell::new(px(0.))),
            ease: Default::default(),
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

    /// Resume native tail following and reveal the newest row.
    pub(crate) fn scroll_to_bottom(&self) {
        self.ease.set(None);
        self.list.set_follow_mode(FollowMode::Tail);
        self.list.scroll_to_end();
    }

    /// Hold the list at `item_ix`, `offset` into it (a scene's turn band, a
    /// minimap jump): tail following pauses until the reader returns to the
    /// end.
    pub(crate) fn scroll_to(&self, item_ix: usize, offset: Pixels) {
        self.ease.set(None);
        self.list.scroll_to(ListOffset {
            item_ix,
            offset_in_item: offset,
        });
    }

    /// Scroll a page up or down (`by` a viewport less the line kept in
    /// view of the page before): reaching the end resumes following it.
    pub(crate) fn scroll_page(&self, down: bool, keep: Pixels) {
        self.ease.set(None);
        let page = (self.list.viewport_bounds().size.height - keep).max(keep);
        self.list.scroll_by(if down { page } else { -page });
    }

    /// Ease to `target` over the frames to come (`step_ease` moves it),
    /// replacing any ease on its way. Tail following pauses meanwhile: the
    /// ease, not the list, moves the offset, and landing on the end resumes
    /// it.
    pub(crate) fn ease_to(&self, target: EaseTarget) {
        self.list.pause_following_tail();
        self.ease.set(Some(ScrollEase {
            target,
            tween: None,
        }));
    }

    /// Whether an ease is on its way.
    pub fn easing(&self) -> bool {
        self.ease.get().is_some()
    }

    /// Stop an ease where it stands (the operator scrolled): whether one
    /// was running.
    pub(crate) fn cancel_ease(&self) -> bool {
        self.ease.take().is_some()
    }

    /// One frame of the ease, after a layout: re-measure what is left to
    /// go, and scroll so that what is left is the tween's value now. `foot`
    /// is the list's padding past its last row (the end lies under it). A
    /// target the list has not measured is snapped to.
    pub(crate) fn step_ease(&self, now: Instant, foot: Pixels) -> EaseStep {
        let Some(mut ease) = self.ease.get() else {
            return EaseStep::Idle;
        };
        if self.list.is_scrollbar_dragging() {
            self.ease.set(None);
            return EaseStep::Cancelled;
        }
        let viewport = self.list.viewport_bounds();
        let left = match ease.target {
            EaseTarget::Row(offset) => {
                // The end can reach the offset before the room under it is
                // laid out: the list would follow it, and the ease moves it.
                self.list.pause_following_tail();
                self.list
                    .bounds_for_item(offset.item_ix)
                    .map(|bounds| bounds.top() + offset.offset_in_item - viewport.top())
            }
            EaseTarget::End if self.list.is_following_tail() => Some(px(0.)),
            EaseTarget::End => Some(
                self.list.max_offset_for_scrollbar().y
                    + self.list.scroll_px_offset_for_scrollbar().y
                    + foot,
            ),
        };
        let Some(mut left) = left else {
            self.land(ease.target);
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
            self.land(ease.target);
            return EaseStep::Landed;
        }
        self.list.scroll_by(left - px(tween.value(now, false)));
        self.ease.set(Some(ease));
        EaseStep::Moving
    }

    /// The ease is done: its target exactly, and at the end following it.
    fn land(&self, target: EaseTarget) {
        match target {
            EaseTarget::Row(offset) => self.scroll_to(offset.item_ix, offset.offset_in_item),
            EaseTarget::End => self.scroll_to_bottom(),
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

    /// Retain the next fully visible row when wrapping above it changes.
    /// GPUI still owns every offset, measurement and tail-follow transition.
    pub(crate) fn did_layout(&self) -> bool {
        let viewport = self.list.viewport_bounds();
        if self.list.is_following_tail() {
            self.resize_anchor.set(None);
            return false;
        }
        let mut adjusted = false;
        if let Some((width, index, inset)) = self.resize_anchor.get() {
            if width != viewport.size.width && index < self.list.item_count() {
                if let Some(bounds) = self.list.bounds_for_item(index) {
                    let delta = bounds.top() - viewport.top() - inset;
                    if delta.abs() > px(0.5) {
                        self.list.scroll_by(delta);
                        adjusted = true;
                    }
                } else {
                    // Reflow can move the held row beyond native overdraw.
                    // Measure it before restoring the original inset.
                    self.list.scroll_to_reveal_item(index);
                    return true;
                }
            }
        }
        let anchor =
            (self.list.logical_scroll_top().item_ix..self.list.item_count()).find_map(|index| {
                let bounds = self.list.bounds_for_item(index)?;
                (bounds.top() >= viewport.top() && bounds.top() < viewport.bottom()).then_some((
                    viewport.size.width,
                    index,
                    bounds.top() - viewport.top(),
                ))
            });
        self.resize_anchor.set(anchor);
        adjusted
    }

    /// The rows' inset moved with the width (`theme::tx_inset_at`), a frame
    /// after the width did: measure every row again, holding the row the
    /// resize held where it stands.
    pub(crate) fn remeasure_held(&self) {
        if let Some((_, index, inset)) = self.resize_anchor.get() {
            self.resize_anchor.set(Some((px(-1.), index, inset)));
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
