//! Where a float hangs (theme WP-E): the palette, the pickers, the
//! notifications list, a toast, the shortcuts sheet and the hover card are
//! each a view, placed against an anchor in window coordinates.
//!
//! Floats draw in-window, over everything (`deferred`), as opaque surfaces.
//! The prototype frosts them (`.glass .float`, a 30px backdrop blur). gpui
//! blurs no element, so the only way to frost one is to give it a window of
//! its own with `WindowBackgroundAppearance::Blurred` — and that path is
//! blocked in gpui-pre-macos 0.3.3: see the FL-15 record in theme WP-E
//! (`FLOAT_FROSTED`). The opaque inks (`paint::FLOAT_*`) are the floats'
//! whole grammar until it lands.

use gpui::prelude::*;
use gpui::{
    anchored, deferred, point, px, Anchor, AnyElement, AnyView, App, Bounds, Pixels, Window,
};

/// How a float stands against its anchor's bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FloatPlace {
    /// Under the anchor, its left edge on the anchor's, `gap` below it.
    Below { gap: f32 },
    /// Over the anchor, its left edge on the anchor's, its foot `gap` above.
    Above { gap: f32 },
    /// Under the anchor, or over it when the window has no room below (the
    /// path hover card's, core-transcript's to hang).
    #[allow(dead_code)]
    BelowOrAbove { gap: f32 },
    /// Centred on the anchor (the board), its head `top` below the anchor's
    /// top: the palette and the shortcuts sheet.
    BoardTop { top: f32 },
}

/// The priority every float paints at: over the Panes' own deferred menus
/// and over the veil (`VEIL_PRIORITY`).
pub(crate) const FLOAT_PRIORITY: usize = 4;
/// The palette's veil: over the board's Panes, under the float.
pub(crate) const VEIL_PRIORITY: usize = 3;

/// The anchored corner, the point it is pinned to and the offset for a
/// placement — pure, so the geometry is testable without a window.
/// `BelowOrAbove` pins the foot above the anchor and offsets the head below
/// it: gpui's anchored element tries the offset placement first and, when
/// that overflows the window, the flipped corner at the bare position.
pub(crate) fn placement(
    anchor: Bounds<Pixels>,
    place: FloatPlace,
) -> (Anchor, gpui::Point<Pixels>, gpui::Point<Pixels>) {
    let zero = point(px(0.), px(0.));
    match place {
        FloatPlace::Below { gap } => (
            Anchor::TopLeft,
            point(anchor.left(), anchor.bottom() + px(gap)),
            zero,
        ),
        FloatPlace::Above { gap } => (
            Anchor::BottomLeft,
            point(anchor.left(), anchor.top() - px(gap)),
            zero,
        ),
        FloatPlace::BelowOrAbove { gap } => (
            Anchor::TopLeft,
            point(anchor.left(), anchor.top() - px(gap)),
            point(px(0.), anchor.size.height + px(2. * gap)),
        ),
        FloatPlace::BoardTop { top } => (
            Anchor::TopCenter,
            point(anchor.center().x, anchor.top() + px(top)),
            zero,
        ),
    }
}

/// Hang `view` against `anchor` (window coordinates): deferred so it paints
/// over every Pane, anchored so it never leaves the window. A placement
/// that may flip (`BelowOrAbove`) switches corners; the rest snap inside
/// the window.
pub(crate) fn hang(
    view: AnyView,
    anchor: Bounds<Pixels>,
    place: FloatPlace,
    _window: &mut Window,
    _cx: &mut App,
) -> AnyElement {
    hang_element(view.into_any_element(), anchor, place)
}

/// `hang` for a float drawn as an element rather than a view of its own
/// (a toast, the notifications list): the same placement, the same layer.
pub(crate) fn hang_element(
    float: AnyElement,
    anchor: Bounds<Pixels>,
    place: FloatPlace,
) -> AnyElement {
    let (corner, at, offset) = placement(anchor, place);
    let anchored = anchored().anchor(corner).position(at).offset(offset);
    let anchored = match place {
        FloatPlace::BelowOrAbove { .. } => anchored,
        _ => anchored.snap_to_window(),
    };
    deferred(anchored.child(float))
        .with_priority(FLOAT_PRIORITY)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    /// The prototype's placements: the palette centred on the board 56px
    /// under its top, the notifications list 4px under the bell, a picker
    /// 5px over the Composer's band.
    #[test]
    fn each_place_pins_its_corner() {
        let board = bounds(281.8, 32., 1158.2, 844.);
        let (corner, at, offset) = placement(board, FloatPlace::BoardTop { top: 56. });
        assert_eq!(corner, Anchor::TopCenter);
        assert_eq!(at, point(px(281.8 + 1158.2 / 2.), px(88.)));
        assert_eq!(offset, point(px(0.), px(0.)));

        let bell = bounds(245., 4., 28., 24.);
        let (corner, at, _) = placement(bell, FloatPlace::Below { gap: 4. });
        assert_eq!(corner, Anchor::TopLeft);
        assert_eq!(at, point(px(245.), px(32.)));

        let band = bounds(419., 815., 100., 0.);
        let (corner, at, _) = placement(band, FloatPlace::Above { gap: 5. });
        assert_eq!(corner, Anchor::BottomLeft);
        assert_eq!(at, point(px(419.), px(810.)));

        let path = bounds(100., 200., 80., 20.);
        let (corner, at, offset) = placement(path, FloatPlace::BelowOrAbove { gap: 6. });
        assert_eq!(corner, Anchor::TopLeft);
        assert_eq!(at + offset, point(px(100.), px(226.)), "the head 6px under");
        assert_eq!(at, point(px(100.), px(194.)), "the flipped foot 6px over");
    }
}
