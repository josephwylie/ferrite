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
    // How much of the viewport's top a cut row's remnant is hidden under
    // (`cut_row_mask`), sampled after layout and painted next frame.
    top_mask: Rc<Cell<Pixels>>,
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
            top_mask: Rc::new(Cell::new(px(0.))),
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

    /// The first visible row is never cut under the head rule while the
    /// tail is followed: after layout, the top row's remnant — when its
    /// content, not just its gap, is cut — is measured for the mask the
    /// view paints over it (`cut_row_mask`). `gap_of` is a row's space
    /// above its content. Whether the mask changed, so the caller repaints
    /// once; an unchanged frame schedules nothing.
    pub(crate) fn settle_top(&self, gap_of: impl Fn(usize) -> f32) -> bool {
        let mask = if self.list.is_following_tail() {
            let viewport = self.list.viewport_bounds();
            let top = self.list.logical_scroll_top().item_ix;
            self.list.bounds_for_item(top).map_or(px(0.), |row| {
                cut_row_mask(viewport.top(), row.top(), row.bottom(), px(gap_of(top)))
            })
        } else {
            px(0.)
        };
        let changed = (self.top_mask.get() - mask).abs() > px(0.5);
        self.top_mask.set(mask);
        changed
    }

    /// The mask over the viewport's top (`settle_top`).
    pub(crate) fn top_mask(&self) -> Pixels {
        self.top_mask.get()
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
        // Nothing else moves: a row's gap is part of the row (see
        // `rows::gap_before`), so a neighbour's arrival or eviction reaches
        // the list as that row's own change in `remeasure`.
    }
}

/// How much of a viewport's top to hide so its first row is never cut: the
/// remnant of a row whose content (below its `gap`) starts above
/// `viewport_top`, so the body reads from the next whole row. A row cut
/// only in its gap is whole already. A remnant taller than
/// `theme::TRANSCRIPT_TOP_SNAP_MAX` is a long block read mid-way, and
/// hiding it would open a void, so it stays.
pub(crate) fn cut_row_mask(
    viewport_top: Pixels,
    row_top: Pixels,
    row_bottom: Pixels,
    gap: Pixels,
) -> Pixels {
    let remnant = row_bottom - viewport_top;
    if row_top + gap >= viewport_top || remnant <= px(0.) {
        return px(0.);
    }
    if remnant <= px(crate::theme::TRANSCRIPT_TOP_SNAP_MAX) {
        remnant
    } else {
        px(0.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::TRANSCRIPT_TOP_SNAP_MAX;

    /// group9's clipped prompt: a one-line row cut under the head rule is
    /// hidden whole; a row cut only in its gap, a row wholly below the top
    /// and a long block read mid-way are left alone.
    #[test]
    fn a_row_cut_under_the_head_rule_is_hidden_whole() {
        let top = px(100.);
        // A prompt: 32px turn gap, a 22px line; its content starts 6px
        // above the viewport, so 16px of it would show cut.
        assert_eq!(cut_row_mask(top, px(62.), px(116.), px(32.)), px(16.));
        // Cut only in its gap: the content is whole.
        assert_eq!(cut_row_mask(top, px(80.), px(134.), px(32.)), px(0.));
        // Starts below the top: nothing is cut.
        assert_eq!(cut_row_mask(top, px(100.), px(154.), px(32.)), px(0.));
        // Scrolled wholly past.
        assert_eq!(cut_row_mask(top, px(20.), px(100.), px(12.)), px(0.));
        // A long answer read mid-way keeps its lines rather than a void.
        assert_eq!(
            cut_row_mask(
                top,
                px(-400.),
                px(100. + TRANSCRIPT_TOP_SNAP_MAX + 1.),
                px(12.)
            ),
            px(0.)
        );
        assert_eq!(
            cut_row_mask(top, px(-400.), px(100. + TRANSCRIPT_TOP_SNAP_MAX), px(12.)),
            px(TRANSCRIPT_TOP_SNAP_MAX)
        );
    }
}
