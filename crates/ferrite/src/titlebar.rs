//! The window's own titlebar, on the platforms that make an app draw one.
//!
//! macOS is given its titlebar: `appears_transparent` hides the face, the
//! traffic lights stay AppKit's, and the band above the board is already a
//! drag strip the system owns. Windows gives nothing — a window either
//! wears the whole system titlebar or draws every part of one itself. It
//! wore the system's, which put a second bar above the app's own band and
//! left the nav's chrome row reserving 77px for lights that are not there
//! (`theme::TRAFFIC_RESERVE`, macOS only now). Ferrite draws the titlebar
//! instead: one band, the sidebar and settings buttons at its left where
//! the nav's own rows line up, and the caption buttons at the window's
//! top-right corner where Windows puts them.
//!
//! gpui does the platform half. A div tagged `window_control_area(..)`
//! answers `WM_NCHITTEST` with the matching non-client code, so **Windows**
//! drags, minimises, maximises and closes — the same path its own caption
//! buttons take, snap-layout flyout included. Nothing here calls a window
//! verb: these are faces, and the OS acts on the hit test.
//!
//! Two consequences shape everything below. A drag region is non-client to
//! Windows, so **nothing interactive may sit under one** — the press never
//! reaches the client and the control cannot be clicked. And the control
//! area is consulted *before* gpui's own resize fallback, so a region flush
//! to y = 0 would eat the top resize edge; `CAPTION_RESIZE_EDGE` is the
//! inset that gives it back.
//!
//! Drawing only, like `nav.rs`: the cockpit places these and owns the state
//! they read.

use gpui::prelude::*;
use gpui::{div, px, rgb, Div, Stateful, WindowControlArea};

use crate::icons::{self, icon};
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::{
    BLOCKED, CAPTION_GLYPH, CAPTION_RESIZE_EDGE, CAPTION_W, TEXT, TEXT_MUTED, WIN_CHROME_H,
};

/// Whether this build draws its own titlebar. macOS keeps the host's, and
/// hiding it there would take the traffic lights with it.
pub const CUSTOM: bool = cfg!(target_os = "windows");

/// The band above the Pane board, as an overlay: the board's geometry
/// already reserves `WIN_CHROME_H` at the top (`board_bounds`) and the nav
/// draws its own chrome row inside the column, so this adds no layout — it
/// claims what the window already left empty.
///
/// The nav's width is skipped rather than covered: the collapse and gear
/// buttons live under it, and a drag region over them would make both
/// unclickable. The nav band's own empty stretch is draggable through
/// `drag_region`, which the cockpit puts between those two buttons.
///
/// `draggable` is false while a menu, popover or the settings panel is
/// open. Such an overlay can reach into the band, and Windows would route
/// the press to the frame instead of to the row under the pointer.
pub fn strip(nav_width: f32, draggable: bool, maximized: bool) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(WIN_CHROME_H))
        .flex()
        .flex_row()
        .child(div().flex_shrink_0().w(px(nav_width)))
        .child(if draggable {
            drag_region("titlebar-drag", maximized)
        } else {
            div().flex_1()
        })
        .child(caption_buttons(maximized))
}

/// An empty stretch Windows drags the window by. The tagged part starts
/// below the resize edge on a restored window, so the top border still
/// resizes; maximized, there is no border to preserve and it runs flush.
pub fn drag_region(id: &'static str, maximized: bool) -> Div {
    let inset = if maximized { 0.0 } else { CAPTION_RESIZE_EDGE };
    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .child(div().flex_shrink_0().h(px(inset)))
        .child(
            div()
                .id(id)
                .flex_1()
                .w_full()
                // See `button`: the root's focus hitbox must not count as
                // hovered under a caption region, or the press is marked
                // handled and Windows never starts the move.
                .occlude()
                .window_control_area(WindowControlArea::Drag),
        )
}

/// Minimise, maximise/restore and close, in the platform's order, flush to
/// the corner. The maximise mark becomes the restore mark while the window
/// is maximized — the button says what the click will do.
fn caption_buttons(maximized: bool) -> Div {
    let (zoom_glyph, zoom_id) = if maximized {
        (icons::WINDOW_RESTORE, "caption-restore")
    } else {
        (icons::WINDOW_MAXIMIZE, "caption-maximize")
    };
    div()
        .flex()
        .flex_shrink_0()
        .h_full()
        .child(button(
            "caption-minimize",
            WindowControlArea::Min,
            icons::WINDOW_MINIMIZE,
            TEXT,
        ))
        .child(button(zoom_id, WindowControlArea::Max, zoom_glyph, TEXT))
        // Soft's hover is achromatic and never borrows a signal colour, so
        // close does not take Windows' red field. The mark takes the red
        // instead: the danger still reads, and the band keeps one hover
        // face across all three.
        .child(button(
            "caption-close",
            WindowControlArea::Close,
            icons::WINDOW_CLOSE,
            BLOCKED,
        ))
}

/// One caption button: square-cornered and edge-to-edge, unlike every other
/// Soft control, because the pointer stops at the window's corner and the
/// hover face has to be there when it does. Full height — the buttons win
/// the top edge from the resize border, as Windows' own do.
fn button(
    id: &'static str,
    area: WindowControlArea,
    glyph: &'static str,
    ink: u32,
) -> Stateful<Div> {
    div()
        .id(id)
        .group(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(CAPTION_W))
        .h_full()
        .hover_control()
        .press_control()
        // A caption press arrives as a *non-client* press, which gpui
        // dispatches into the tree first and Windows acts on only if the
        // tree left it alone. The root tracks focus, and gpui's focus
        // transfer calls `prevent_default()` for every press over a
        // focus-tracked hitbox it counts as hovered — which would mark
        // every caption press handled and swallow it. Occluding stops the
        // hover count at this hitbox, so the root's never runs and the
        // press reaches the frame.
        .occlude()
        .window_control_area(area)
        .child(
            icon(glyph, CAPTION_GLYPH, TEXT_MUTED)
                .group_hover(id, move |style| style.text_color(rgb(ink))),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The buttons are the platform's, so their width is the platform's
    /// too: a narrower one would hang Windows' own snap-layout flyout off
    /// the maximise mark it opens under.
    #[test]
    fn a_caption_button_is_the_platform_width_and_square() {
        let mut close = button(
            "caption-close",
            WindowControlArea::Close,
            icons::WINDOW_CLOSE,
            BLOCKED,
        );
        assert_eq!(close.style().size.width, Some(px(CAPTION_W).into()));
        assert!(
            close.style().corner_radii.top_right.is_none(),
            "a caption button reaches the window's corner, so it has none"
        );
    }

    /// The strip claims the band the board already leaves empty — it must
    /// take no layout of its own, or every Pane would move down by 42px.
    #[test]
    fn the_strip_is_an_overlay_of_the_band_the_board_reserves() {
        let mut strip = strip(crate::nav::WIDTH, true, false);
        assert_eq!(strip.style().size.height, Some(px(WIN_CHROME_H).into()));
        assert_eq!(strip.style().position, Some(gpui::Position::Absolute));
    }

    /// A restored window keeps its top resize edge, which is an inset the
    /// drag region gives up; maximized there is no edge to preserve.
    #[test]
    fn a_restored_window_keeps_its_top_resize_edge() {
        assert!(
            CAPTION_RESIZE_EDGE > 0.0,
            "a drag region flush to y = 0 eats the top border"
        );
    }
}
