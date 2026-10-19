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
    // How much of the viewport's top the pinned band and the cut line under
    // it hide (`band_mask`), sampled after layout and painted next frame.
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

    /// Hold the list at `item_ix`, `offset` into it (a scene's turn band, a
    /// minimap jump): tail following pauses until the reader returns to the
    /// end.
    pub(crate) fn scroll_to(&self, item_ix: usize, offset: Pixels) {
        self.list.scroll_to(gpui::ListOffset {
            item_ix,
            offset_in_item: offset,
        });
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

    /// The pinned band hides the viewport's top down to its foot, `band`
    /// below the viewport's top (`None` when no band is pinned). Whatever
    /// row passes under that foot is hidden down to its next whole line
    /// (`band_mask`), so no partly cut glyph line shows beneath the band and
    /// no void taller than one line opens. `content_top` is where a row's
    /// lines start, from its top (its gap, and a prompt band's half line);
    /// `line` the transcript's line. Whether the mask changed, so the caller
    /// repaints once; an unchanged frame schedules nothing.
    pub(crate) fn settle_top(
        &self,
        band: Option<Pixels>,
        content_top: impl Fn(usize) -> f32,
        line: f32,
    ) -> bool {
        let mask = band.map_or(px(0.), |band| {
            let viewport = self.list.viewport_bounds();
            let edge = viewport.top() + band.max(px(0.));
            let mut index = self.list.logical_scroll_top().item_ix;
            // The row the foot falls in: under a pinned band that is a row
            // or two below the list's own first.
            while self
                .list
                .bounds_for_item(index)
                .is_some_and(|row| row.bottom() <= edge)
            {
                index += 1;
            }
            let cut = self.list.bounds_for_item(index).map_or(px(0.), |row| {
                band_mask(
                    edge,
                    row.top(),
                    row.bottom(),
                    row.top() + px(content_top(index)),
                    px(line),
                )
            });
            edge - viewport.top() + cut
        });
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

/// How far below the pinned band's foot (`edge`) to hide the row passing
/// under it, whatever its height: down to the row's next whole line
/// boundary, the lines sitting on the grid from `content_top` every `line`.
/// A foot in the row's gap, or exactly on a line boundary, hides nothing;
/// the mask never reaches past the row's bottom, and is always shorter than
/// one line.
pub(crate) fn band_mask(
    edge: Pixels,
    row_top: Pixels,
    row_bottom: Pixels,
    content_top: Pixels,
    line: Pixels,
) -> Pixels {
    if edge <= row_top || edge >= row_bottom || edge <= content_top || line <= px(0.) {
        return px(0.);
    }
    let into = f32::from(edge - content_top);
    let line = f32::from(line);
    let lines = (into / line).ceil();
    let boundary = content_top + px(lines * line);
    let mask = boundary.min(row_bottom) - edge;
    if mask < px(0.5) {
        px(0.)
    } else {
        mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CT-7: scrolling a row of any height under the band's foot one pixel
    /// at a time never leaves part of a glyph line showing below the foot,
    /// and never hides more than one line.
    #[test]
    fn the_band_hides_down_to_the_next_whole_line_at_every_offset() {
        let line = 20.;
        // A code block of 30 lines, a 9-row table on a 20px pitch, a long
        // paragraph of 40 lines: rows that sit on the line grid from their
        // content top, a gap above.
        for (lines, gap) in [(30usize, 20.), (9, 20.), (40, 0.), (1, 20.)] {
            let height = gap + lines as f32 * line;
            for offset in 0..(height as i32 + 40) {
                let row_top = px(100. - offset as f32);
                let row_bottom = row_top + px(height);
                let content_top = row_top + px(gap);
                let edge = px(100.);
                let mask = band_mask(edge, row_top, row_bottom, content_top, px(line));
                assert!(mask < px(line), "{lines}x{offset}: a void of {mask:?}");
                assert!(mask >= px(0.));
                let reveal = edge + mask;
                if reveal < row_bottom && reveal > content_top {
                    // What shows first below the mask starts on a line.
                    let into = f32::from(reveal - content_top);
                    let rem = into % line;
                    assert!(
                        rem < 0.5 || line - rem < 0.5,
                        "{lines} lines at {offset}: a cut line shows ({rem}px into one)"
                    );
                }
            }
        }
        // A foot in the gap, or over nothing, hides nothing.
        assert_eq!(
            band_mask(px(105.), px(100.), px(160.), px(120.), px(20.)),
            px(0.)
        );
        assert_eq!(
            band_mask(px(50.), px(100.), px(160.), px(120.), px(20.)),
            px(0.)
        );
        // Six pixels into a line: the other fourteen go.
        assert_eq!(
            band_mask(px(126.), px(100.), px(160.), px(120.), px(20.)),
            px(14.)
        );
    }
}
