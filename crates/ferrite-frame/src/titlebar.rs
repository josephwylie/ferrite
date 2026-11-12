//! The window's own titlebar, on the platforms that make an app draw one.
//!
//! macOS keeps the host's traffic lights, but the app owns the drag
//! (`app_owns_titlebar_drag`): AppKit would otherwise move the window from
//! any press in the titlebar's height, and Pane heads, which are dragged to
//! rearrange the board, must remain interactive. An empty strip across the
//! whole window starts the native move (`mac_drag_region`), with the board
//! below it. Windows gives nothing — a window either wears the
//! whole system titlebar or draws every part of one itself. Ferrite draws
//! the titlebar there: one empty band the window drags by, and the caption
//! buttons at the window's top-right corner where Windows puts them.
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
//! The band says nothing (ADR 0014): no location, no doors. The view head
//! names the Thread over it; there is no bottom bar (the view switcher is
//! the nav foot's).
//!
//! Drawing only: the Cockpit places these (`ferrite_ui::frame`) and owns
//! the state they read.

use gpui::prelude::*;
use gpui::{div, px, rgb, Div, MouseButton, Stateful, WindowControlArea};

use ferrite_ui::frame::CUSTOM;

use ferrite_ui::icons::{self, icon};
use ferrite_ui::pointer::{Pointer, PointerPressed};
use ferrite_ui::theme::*;

/// The head row's drag strip across the whole window, as an overlay: the
/// cards and the nav's card start under it (`board_top`), so this adds no
/// layout and paints nothing — it claims what the window already left
/// empty, the traffic lights' strip included.
///
/// It is the window's titlebar and no more: the strip the window drags by
/// and, on Windows, the caption buttons at its right end.
///
/// `draggable` is false while a menu, popover or the settings panel is
/// open. Such an overlay can reach into the band, and the drag would take
/// the press meant for the row under the pointer.
pub fn strip(draggable: bool, maximized: bool) -> Div {
    let stretch = head_drag(draggable, maximized);
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(WIN_CHROME_H))
        .flex()
        .flex_row()
        .child(stretch)
        .children(CUSTOM.then(|| caption_buttons(maximized)))
}

/// The strip's empty stretch: what the window drags by, the traffic
/// lights over it on macOS. `draggable` is `strip`'s.
fn head_drag(draggable: bool, maximized: bool) -> Div {
    if !draggable {
        div().flex_1().h_full()
    } else if CUSTOM {
        drag_region(maximized)
    } else if cfg!(target_os = "macos") {
        mac_drag_region()
    } else {
        div().flex_1().h_full()
    }
}

/// The empty stretch macOS drags the window by: a press starts AppKit's
/// window drag, a double-click takes the system's titlebar action (zoom or
/// minimise). The traffic lights are AppKit's own buttons over it.
fn mac_drag_region() -> Div {
    div()
        .debug_selector(|| "titlebar-drag".into())
        .flex_1()
        .h_full()
        .occlude()
        .on_mouse_down(MouseButton::Left, |event, window, cx| {
            gpui::base::GlobalState::suppress_text_selection(cx);
            if event.click_count == 2 {
                window.titlebar_double_click();
            } else {
                window.start_window_move();
            }
        })
}

/// An empty stretch Windows drags the window by. The tagged part starts
/// below the resize edge on a restored window, so the top border still
/// resizes; maximized, there is no border to preserve and it runs flush.
fn drag_region(maximized: bool) -> Div {
    let inset = if maximized { 0.0 } else { CAPTION_RESIZE_EDGE };
    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .child(div().flex_shrink_0().h(px(inset)))
        .child(
            div()
                .id("titlebar-drag")
                .debug_selector(|| "titlebar-drag".into())
                .flex_1()
                .w_full()
                // See `button`: the root's focus hitbox must not count as
                // hovered under a caption region, or the press is marked
                // handled and Windows never starts the move.
                .occlude()
                // Window-level text selection otherwise treats this
                // out-of-transcript press as a drag from the nearest run.
                // Suppression does not consume the non-client event, so
                // Windows still receives it and starts the window move.
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    gpui::base::GlobalState::suppress_text_selection(cx);
                })
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
            false,
        ))
        .child(button(zoom_id, WindowControlArea::Max, zoom_glyph, false))
        // Close takes Windows' own red field and white mark under the
        // pointer: platform muscle memory, not a Ferrite state colour.
        .child(button(
            "caption-close",
            WindowControlArea::Close,
            icons::WINDOW_CLOSE,
            true,
        ))
}

/// One caption button: square-cornered and edge-to-edge, unlike every other
/// control, because the pointer stops at the window's corner and the hover
/// face has to be there when it does. Full height — the buttons win the top
/// edge from the resize border, as Windows' own do. The mark rests at
/// `INK_SUBTLE` and brightens under the pointer; `close` lays the platform
/// red under it.
fn button(
    id: &'static str,
    area: WindowControlArea,
    glyph: &'static str,
    close: bool,
) -> Stateful<Div> {
    let hover_ink = if close { CAPTION_CLOSE_INK } else { INK_STRONG };
    div()
        .id(id)
        .debug_selector(move || id.into())
        .group(id)
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(CAPTION_W))
        .h_full()
        .hover_control(id)
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
        // Caption presses share the window's pointer stream with native
        // selectable text. Keep them from anchoring a selection at the
        // nearest transcript run without handling the frame event itself.
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            gpui::base::GlobalState::suppress_text_selection(cx);
        })
        .window_control_area(area)
        .when(close, |button| {
            button.child(
                div()
                    .id("caption-close-field")
                    .absolute()
                    .inset_0()
                    .group_hover(id, |style| style.bg(rgb(CAPTION_CLOSE)))
                    .group_active(id, |style| style.bg(rgb(CAPTION_CLOSE_PRESSED))),
            )
        })
        .child(
            icon(glyph, CAPTION_GLYPH, INK_SUBTLE)
                .group_hover(id, move |style| style.text_color(rgb(hover_ink))),
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
            true,
        );
        assert_eq!(close.style().size.width, Some(px(CAPTION_W).into()));
        assert!(
            close.style().corner_radii.top_right.is_none(),
            "a caption button reaches the window's corner, so it has none"
        );
    }

    /// The strip claims the band the window already leaves empty — it must
    /// take no layout of its own, or every Pane would move down by
    /// `WIN_CHROME_H`.
    #[gpui::test]
    fn the_strip_is_an_overlay_of_the_band(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            ferrite_pane::pane::init_components(cx);
            let mut strip = strip(true, false);
            assert_eq!(strip.style().size.height, Some(px(WIN_CHROME_H).into()));
            assert_eq!(strip.style().position, Some(gpui::Position::Absolute));
        });
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
