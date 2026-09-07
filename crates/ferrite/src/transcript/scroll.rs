//! The transcript's small scroll facade over GPUI's lazy variable-height list.

use gpui::{px, FollowMode, ListAlignment, ListState, Pixels};
#[cfg(test)]
use gpui::{Bounds, Point};
use std::{cell::Cell, rc::Rc};

use super::rows::RowDelta;

/// GPUI measures the viewport plus this small buffer, not the full transcript.
const OVERDRAW: Pixels = px(200.);

/// One transcript viewport's native scroll state.
#[derive(Clone, Debug)]
pub(crate) struct TranscriptScroll {
    list: ListState,
    // A fully visible row's inset, sampled from native layout for width reflow.
    resize_anchor: Rc<Cell<Option<(Pixels, usize, Pixels)>>>,
}

impl TranscriptScroll {
    /// Start at the live tail. A user scroll away suspends tail following until
    /// they return to the end; GPUI owns that transition.
    pub(crate) fn new(row_count: usize) -> Self {
        let list = ListState::new(row_count, ListAlignment::Top, OVERDRAW);
        list.set_follow_mode(FollowMode::Tail);
        Self {
            list,
            resize_anchor: Default::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn offset(&self) -> Point<Pixels> {
        self.list.scroll_px_offset_for_scrollbar()
    }

    /// Sets a list-native pixel offset, used by scrollbar gestures and tests.
    #[cfg(test)]
    pub(crate) fn set_offset(&self, offset: Point<Pixels>) {
        self.list.set_offset_from_scrollbar(offset);
    }

    #[cfg(test)]
    pub(crate) fn max_offset(&self) -> Point<Pixels> {
        self.list.max_offset_for_scrollbar()
    }

    #[cfg(test)]
    pub(crate) fn bounds(&self) -> Bounds<Pixels> {
        self.list.viewport_bounds()
    }

    /// Resume native tail following and reveal the newest row.
    pub(crate) fn scroll_to_bottom(&self) {
        self.list.set_follow_mode(FollowMode::Tail);
        self.list.scroll_to_end();
    }

    pub(crate) fn is_following_tail(&self) -> bool {
        self.list.is_following_tail()
    }

    pub(crate) fn item_is_visible(&self, index: usize) -> bool {
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
        let anchor = (self.list.logical_scroll_top().item_ix..self.list.item_count())
            .find_map(|index| {
                let bounds = self.list.bounds_for_item(index)?;
                (bounds.top() >= viewport.top() && bounds.top() < viewport.bottom())
                    .then_some((viewport.size.width, index, bounds.top() - viewport.top()))
            });
        self.resize_anchor.set(anchor);
        adjusted
    }

    /// Freeze tail following while retaining its automatic re-engagement rule.
    #[cfg(test)]
    pub(crate) fn pause_following_tail(&self) {
        self.list.pause_following_tail();
    }

    /// The native state passed to `gpui::list` and the existing scrollbar.
    pub(crate) fn list_state(&self) -> &ListState {
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
        if !delta.splices.is_empty() {
            // The renderer puts the inter-row gap on every row except the
            // final one. Structural edits can therefore change the measured
            // height of the final survivor and its neighbour without changing
            // either row's own content.
            if let Some(last) = self.list.item_count().checked_sub(1) {
                self.list.remeasure_items(last..last + 1);
                if last > 0 {
                    self.list.remeasure_items(last - 1..last);
                }
            }
        }
    }
}
