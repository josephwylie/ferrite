//! The list's two fades that read what only the frame knows.
//!
//! - [`title_line`] fades a cut title's own ink, as T3's mask does, so it
//!   melts into whatever is under it: the column's grain, a card's fill at
//!   any point of its hover, a lost machine's dimmed row. A painted ground
//!   can only guess at those, and shows as a box where it guesses wrong.
//! - [`past_edge`] draws a scroll edge's fade only once layout has clamped
//!   the scroll: a render reads the offset of the frame before, so a list
//!   grown back to fit would keep the fade of when it was scrolled.

use gpui::{
    px, AnyElement, App, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, ScrollHandle, SharedString, Style,
    TextAlign, Window,
};

use ferrite_ui::theme::LINE_FADE;

/// A one-line title filling its box's width. Where the box cuts it, its
/// last `LINE_FADE` fades out.
pub(super) fn title_line(text: impl Into<SharedString>) -> TitleLine {
    TitleLine(text.into())
}

pub(super) struct TitleLine(SharedString);

impl IntoElement for TitleLine {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

/// The columns of the fade ending at `right`, one per device pixel, left
/// to right: each one's edges and how much of the ink it keeps. They start
/// and end on the device grid, so each pixel is drawn exactly once.
fn fade_columns(right: f32, scale: f32) -> impl Iterator<Item = (f32, f32, f32)> {
    let end = (right * scale).round();
    let count = (LINE_FADE * scale).round().max(1.0);
    (0..count as u32).map(move |column| {
        let at = column as f32;
        (
            (end - count + at) / scale,
            (end - count + at + 1.0) / scale,
            1.0 - (at + 0.5) / count,
        )
    })
}

impl Element for TitleLine {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let text: SharedString = match self.0.find('\n') {
            Some(end) => self.0[..end].to_string().into(),
            None => self.0.clone(),
        };
        if text.is_empty() {
            return;
        }
        let style = window.text_style();
        let size = style.font_size.to_pixels(window.rem_size());
        let height = window.line_height();
        let run = style.to_run(text.len());
        let line = window.text_system().shape_line(text, size, &[run], None);
        let draw = |window: &mut Window, cx: &mut App| {
            let _ = line.paint(bounds.origin, height, TextAlign::Left, None, window, cx);
        };
        if line.width <= bounds.size.width {
            draw(window, cx);
            return;
        }
        let scale = window.scale_factor();
        let column = |left: f32, right: f32| ContentMask {
            bounds: Bounds::from_corners(
                gpui::point(px(left), bounds.top()),
                gpui::point(px(right), bounds.bottom()),
            ),
        };
        let columns: Vec<_> = fade_columns(f32::from(bounds.right()), scale).collect();
        let solid_end = columns.first().map_or(f32::from(bounds.right()), |first| first.0);
        window.with_content_mask(Some(column(f32::from(bounds.left()), solid_end)), |window| {
            draw(window, cx)
        });
        for (left, right, ink) in columns {
            window.with_content_mask(Some(column(left, right)), |window| {
                window.with_element_opacity(Some(ink), |window| {
                    let _ = line.paint_again(bounds.origin, height, window, cx);
                });
            });
        }
    }
}

/// `fade`, drawn only while `past` says the scroll has rows beyond its
/// edge, asked once this frame's layout has clamped the scroll. It must
/// come after the scroller among its siblings.
pub(super) fn past_edge(
    scroll: &ScrollHandle,
    past: fn(&ScrollHandle) -> bool,
    fade: impl IntoElement,
) -> PastEdge {
    PastEdge {
        scroll: scroll.clone(),
        past,
        fade: fade.into_any_element(),
    }
}

pub(super) struct PastEdge {
    scroll: ScrollHandle,
    past: fn(&ScrollHandle) -> bool,
    fade: AnyElement,
}

impl IntoElement for PastEdge {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for PastEdge {
    type RequestLayoutState = ();
    /// Whether the fade is drawn this frame.
    type PrepaintState = bool;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.fade.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let shown = (self.past)(&self.scroll);
        if shown {
            self.fade.prepaint(window, cx);
        }
        shown
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        shown: &mut bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        if *shown {
            self.fade.paint(window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fade covers its width once, on the device grid, its ink falling
    /// from nearly whole to nearly none.
    #[test]
    fn a_cut_title_fades_its_own_ink_across_every_device_pixel() {
        for scale in [1.0, 1.25, 1.44, 2.0] {
            let columns: Vec<_> = fade_columns(200.0, scale).collect();
            assert_eq!(columns.len(), (LINE_FADE * scale).round() as usize);
            for pair in columns.windows(2) {
                assert_eq!(pair[0].1, pair[1].0, "no gap and no overlap at {scale}x");
                assert!(pair[1].2 < pair[0].2, "the ink only falls");
            }
            let (first, last) = (columns[0], columns[columns.len() - 1]);
            assert!(first.2 > 0.9 && last.2 < 0.1, "from ink to nothing at {scale}x");
            let on_grid = |x: f32| ((x * scale).round() - x * scale).abs() < 1e-3;
            assert!(on_grid(first.0) && on_grid(last.1));
        }
    }
}
