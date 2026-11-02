//! The transcript's small scroll facade over GPUI's lazy variable-height list.

use gpui::{px, FollowMode, ListAlignment, ListState, Pixels};
#[cfg(any(test, feature = "test-support"))]
use gpui::{Bounds, Point};
use std::{cell::Cell, rc::Rc};

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
        self.list.set_follow_mode(FollowMode::Tail);
        self.list.scroll_to_end();
    }

    /// Hold the list at `item_ix`, `offset` into it (a scene's turn band, a
    /// minimap jump): tail following pauses until the reader returns to the
    /// end.
    pub(crate) fn scroll_to(&self, item_ix: usize, offset: Pixels) {
        self.list.scroll_to(gpui::ListOffset {
            item_ix,
            offset_in_item: offset,
        });
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

    /// How much of the viewport's top the pinned band covers (`band`, its
    /// foot below the viewport's top; `None` when no band is pinned). Rows
    /// pass under it into its fade (`TX_TOP_FADE`), so nothing is cut on a
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
