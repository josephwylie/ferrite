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
//! The band says nothing (ADR 0014): no location, no doors. The nav's
//! selected row names the Thread; the window's one bar is the bottom bar
//! (`bottom_bar`, F-9) — the sidebar toggle and the bell, the session,
//! `1 solo` and a tab per Group, the focused Pane's status line, usage, `?`
//! and the clock: tmux's grammar on the chrome.
//!
//! Drawing only, like `nav.rs`: the cockpit places these and owns the state
//! they read.

use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, Div, MouseButton, SharedString, Stateful, WindowControlArea};

use crate::theme::paint;

use crate::components;
use crate::icons::{self, icon};
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::*;

/// Whether this build is an unreleased one. `--release` is not the
/// question — a locally built release binary is still a dev build, and
/// wants the `dev` mark. Only the release pipeline ships without it, which it
/// says by setting `FERRITE_RELEASE` for the compile (`build.rs` tracks
/// the variable so a cached build cannot keep a stale answer). Settings ›
/// About reports it; the titlebar carries no badge.
pub const DEV: bool = option_env!("FERRITE_RELEASE").is_none();

/// Whether this build draws its own titlebar. macOS keeps the host's, and
/// hiding it there would take the traffic lights with it.
pub const CUSTOM: bool = cfg!(target_os = "windows");

/// Keep a dedicated window-drag strip above interactive Pane content on
/// macOS and Windows, independent of whether the nav is expanded.
pub fn band_over_board() -> bool {
    CUSTOM || cfg!(target_os = "macos")
}

/// Where the Pane board starts: under the band, or at the window's top.
pub fn board_top() -> f32 {
    if band_over_board() {
        WIN_CHROME_H
    } else {
        0.0
    }
}

/// The titlebar band, as an overlay: the nav leaves its top `WIN_CHROME_H`
/// empty (`nav::content`) and the board starts at `board_top`, so this adds
/// no layout — it claims what the window already left empty. Over the nav
/// alone it ends at `board_left`; across the board it paints the reading
/// plane (`paint::PLANE`) from there.
///
/// The band says nothing (ADR 0014): it is the window's titlebar and no
/// more — the strip the window drags by and, on Windows, the caption
/// buttons.
///
/// `draggable` is false while a menu, popover or the settings panel is
/// open. Such an overlay can reach into the band, and the drag would take
/// the press meant for the row under the pointer.
pub fn strip(board_left: f32, draggable: bool, maximized: bool) -> Div {
    let over_board = band_over_board();
    let stretch = if !draggable {
        div().flex_1().h_full()
    } else if CUSTOM {
        drag_region(maximized)
    } else if cfg!(target_os = "macos") {
        mac_drag_region()
    } else {
        div().flex_1().h_full()
    };
    div()
        .absolute()
        .top_0()
        .left_0()
        .map(|band| {
            if over_board {
                band.right_0()
            } else {
                band.w(px(board_left))
            }
        })
        .h(px(WIN_CHROME_H))
        .flex()
        .flex_row()
        .when(over_board, |band| {
            band.child(
                div()
                    .debug_selector(|| "titlebar-plane".into())
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(board_left))
                    .right_0()
                    .bg(paint::PLANE),
            )
        })
        .child(stretch)
        .children(CUSTOM.then(|| caption_buttons(maximized)))
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

// ------------------------------------------------------------- bottom bar

/// The bottom bar (theme WP-C, F-9), the window's one bar (ADR 0014): one
/// row on the chrome (`paint::CHROME`), closed above by a `paint::LINE`
/// rule, tmux's grammar in the one face — the window's `doors` (the
/// sidebar toggle, the bell and, off macOS, the gear), `ferrite` in
/// `TEXT_STRONG` at the strong weight, `1 solo` and a tab per Group (the current
/// view's bright on a band), the stretch holding the focused Pane's
/// `status` line, a usage segment per provider, the `?` door and the clock
/// in `TEXT`. No state counts: the sidebar and the bell hold those. The
/// status line gives way first when the window is narrow.
pub fn bottom_bar(
    doors: Vec<AnyElement>,
    tabs: Vec<AnyElement>,
    status: Div,
    usage: Vec<AnyElement>,
    help: AnyElement,
    clock: SharedString,
) -> Div {
    div()
        .debug_selector(|| "bottom-bar".into())
        .flex()
        .flex_row()
        .flex_shrink_0()
        .w_full()
        .h(px(STATUS_BAR_H))
        .bg(paint::CHROME)
        .border_t_1()
        .border_color(paint::LINE)
        .font_family(FONT_UI)
        .text_size(px(FS_UI))
        .line_height(px(LH_UI))
        .text_color(rgb(CHROME_MUTED))
        .whitespace_nowrap()
        .overflow_hidden()
        .children(doors)
        .child(
            div()
                .debug_selector(|| "bottom-bar-session".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .px(px(BAR_SEG_PAD_X))
                .font_weight(W_STRONG)
                .text_color(rgb(TEXT_STRONG))
                .child(components::cells("ferrite")),
        )
        .children(tabs)
        .child(status)
        .children(usage)
        .child(help)
        .child(
            components::tabular(
                div()
                    .debug_selector(|| "bottom-bar-clock".into())
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .pl(px(BAR_SEG_PAD_X))
                    .pr(px(BAR_SEG_PAD_TAIL))
                    .text_color(rgb(TEXT)),
            )
            .child(components::cells(clock)),
        )
}

/// The `?` door (ADR 0014): the one way in to the keys, a bar segment in
/// `CHROME_MUTED` that lifts to `TEXT` on `paint::HOVER` under the pointer.
/// The cockpit wires the press to the shortcuts sheet.
pub fn help_door() -> Stateful<Div> {
    let key = SharedString::from("bottom-bar-help");
    div()
        .id(gpui::ElementId::Name(key.clone()))
        .debug_selector(|| "bottom-bar-help".into())
        .group(key.clone())
        .flex()
        .flex_shrink_0()
        .items_center()
        .px(px(BAR_SEG_PAD_X))
        .cursor_pointer()
        .hover_row(key.clone())
        .tooltip(crate::menu::action_tooltip(
            "Shortcuts",
            "shortcuts::Toggle",
        ))
        .child(
            components::cells("?")
                .text_color(rgb(CHROME_MUTED))
                .group_hover(key, |style| style.text_color(rgb(TEXT))),
        )
}

/// One view's tab: ` 1 solo `, ` 2 perf sweep ` — the words a cell in from
/// the tab's own cell of padding, and the roll-up `mark` of the Threads it
/// shows after them (` 2 perf sweep ✓ `) while it says anything. The
/// current view's is `TEXT_STRONG` on `paint::BAND2`; the rest are dim and
/// lift to `TEXT` on `paint::HOVER` under the pointer. The cockpit wires
/// the press.
pub fn bar_tab(
    id: SharedString,
    ordinal: usize,
    label: SharedString,
    current: bool,
    mark: Option<crate::cockpit::ThreadStatus>,
) -> Stateful<Div> {
    let selector = id.clone();
    let key = id.clone();
    let words = format!("{ordinal} {label}");
    // The prototype's ` 2 perf sweep `, one run, in a cell of padding
    // each side; a mark takes a space and its cell more.
    let width = components::run_width(&format!(" {words} "))
        + 2.0 * BAR_SEG_PAD_X
        + if mark.is_some() { 2.0 * CH } else { 0.0 };
    div()
        .id(gpui::ElementId::Name(id))
        .debug_selector(move || selector.to_string())
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(width))
        .pl(px(2.0 * BAR_SEG_PAD_X))
        .map(|tab| {
            if current {
                tab.cursor_pointer()
                    .bg(paint::BAND2)
                    .text_color(rgb(TEXT_STRONG))
            } else {
                tab.hover_row(key.clone()).group(key.clone()).press_row()
            }
        })
        .child(
            div()
                .when(!current, |label| {
                    label.group_hover(key, |style| style.text_color(rgb(TEXT)))
                })
                .child(components::cells(words)),
        )
        .children(mark.map(|mark| div().flex_shrink_0().ml(px(CH)).child(mark.dot())))
}

/// The window a provider's usage segment reads: its more used one, the 5h
/// window on a tie (`5h 41%`, `wk 12%`). `None` when it has reported
/// neither.
pub fn usage_window(limits: &ferrite_core::transcript::RateLimits) -> Option<(&'static str, f32)> {
    let five = limits
        .five_hour
        .map(|window| window.used_fraction.clamp(0.0, 1.0));
    let week = limits
        .weekly
        .map(|window| window.used_fraction.clamp(0.0, 1.0));
    match (five, week) {
        (Some(five), Some(week)) if week > five => Some(("wk", week)),
        (Some(five), _) => Some(("5h", five)),
        (None, Some(week)) => Some(("wk", week)),
        (None, None) => None,
    }
}

/// One provider's usage segment (F-9): its 11px mark, a cell, and its more
/// used window (`5h 41%`), a cell of padding each side, `CHROME_MUTED` —
/// `TEXT` on `paint::HOVER` under the pointer. `None` when the provider has
/// reported no window.
pub fn bar_usage(
    provider: ferrite_core::store::Provider,
    limits: &ferrite_core::transcript::RateLimits,
) -> Option<Stateful<Div>> {
    let (window, used) = usage_window(limits)?;
    let (glyph, ink, name) = match provider {
        ferrite_core::store::Provider::Claude => (icons::CLAUDE, PROVIDER_CLAUDE, "claude"),
        ferrite_core::store::Provider::Codex => (icons::CODEX, PROVIDER_CODEX, "codex"),
    };
    let key = SharedString::from(format!("bottom-bar-usage-{name}"));
    let selector = key.clone();
    Some(
        div()
            .id(gpui::ElementId::Name(key.clone()))
            .debug_selector(move || selector.to_string())
            .group(key.clone())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(BAR_SEG_PAD_X))
            .pl(px(BAR_SEG_PAD_X))
            .pr(px(BAR_SEG_PAD_TAIL))
            .hover_row(key.clone())
            .child(icon(glyph, BAR_MARK, ink))
            .child(components::tabular(
                components::cells(format!("{window} {}%", (used * 100.).round() as u32))
                    .text_color(rgb(CHROME_MUTED))
                    .group_hover(key, |style| style.text_color(rgb(TEXT))),
            )),
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
/// `TEXT_DIM` and brightens under the pointer; `close` lays the platform
/// red under it.
fn button(
    id: &'static str,
    area: WindowControlArea,
    glyph: &'static str,
    close: bool,
) -> Stateful<Div> {
    let hover_ink = if close {
        CAPTION_CLOSE_INK
    } else {
        TEXT_STRONG
    };
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
            icon(glyph, CAPTION_GLYPH, TEXT_DIM)
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
            crate::theme::init_components(cx);
            let mut strip = strip(crate::nav::WIDTH, true, false);
            assert_eq!(strip.style().size.height, Some(px(WIN_CHROME_H).into()));
            assert_eq!(strip.style().position, Some(gpui::Position::Absolute));
        });
    }

    /// The window-drag strip has its own space above the board's controls.
    #[test]
    fn the_board_starts_at_the_top_unless_the_band_crosses_it() {
        let expected = if CUSTOM || cfg!(target_os = "macos") {
            WIN_CHROME_H
        } else {
            0.0
        };
        assert_eq!(board_top(), expected);
    }

    /// F-9: a usage segment reads the provider's more used window, the 5h
    /// one on a tie.
    #[test]
    fn a_usage_segment_reads_the_more_used_window() {
        use ferrite_core::transcript::RateLimits;
        use ferrite_core::RateLimitWindow;
        let window = |used: f32| {
            Some(RateLimitWindow {
                used_fraction: used,
                resets_at: None,
            })
        };
        let limits = |five_hour, weekly| RateLimits { five_hour, weekly };
        assert_eq!(
            usage_window(&limits(window(0.41), window(0.12))),
            Some(("5h", 0.41))
        );
        assert_eq!(
            usage_window(&limits(window(0.10), window(0.12))),
            Some(("wk", 0.12))
        );
        assert_eq!(
            usage_window(&limits(window(0.3), window(0.3))),
            Some(("5h", 0.3))
        );
        assert_eq!(usage_window(&limits(None, None)), None);
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
