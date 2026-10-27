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
//! (`bottom_bar`, SPEC §5.7): `1 Solo` and a tab per Group, then the
//! vitals — who needs you, usage, free disk and the clock.
//!
//! Drawing only: the Cockpit places these (`ferrite_ui::frame`) and owns
//! the state they read.

use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, Div, MouseButton, SharedString, Stateful, WindowControlArea};

use ferrite_ui::frame::{Bar, RollUp, CUSTOM};
use ferrite_ui::theme::paint;

use ferrite_ui::components;
use ferrite_ui::icons::{self, icon};
use ferrite_ui::pointer::{Pointer, PointerPressed};
use ferrite_ui::theme::*;

/// The head row's drag strip over the board, as an overlay: the cards
/// start at `board_top`, so this adds no layout and paints nothing — it
/// claims what the window already left empty, from `board_left`, whose
/// frame the board column paints (`components::frame_around`). Over the
/// nav, the nav's own head drags the window (`nav::head`, its stretch
/// `head_drag`) around its sidebar toggle.
///
/// It is the window's titlebar and no more: the strip the window drags by
/// and, on Windows, the caption buttons at its right end.
///
/// `draggable` is false while a menu, popover or the settings panel is
/// open. Such an overlay can reach into the band, and the drag would take
/// the press meant for the row under the pointer.
pub fn strip(board_left: f32, draggable: bool, maximized: bool) -> Div {
    let stretch = head_drag(draggable, maximized);
    div()
        .absolute()
        .top_0()
        .left(px(board_left))
        .right_0()
        .h(px(WIN_CHROME_H))
        .flex()
        .flex_row()
        .child(stretch)
        .children(CUSTOM.then(|| caption_buttons(maximized)))
}

/// The nav head's empty stretch (SPEC §5.1.1): what the window drags by
/// over the nav, the traffic lights over it on macOS — the platform's
/// stretch, as `strip` lays it over the board. `draggable` is `strip`'s.
pub fn head_drag(draggable: bool, maximized: bool) -> Div {
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

// ------------------------------------------------------------- bottom bar

/// The bottom bar as the Cockpit fills it: its view tabs, then the
/// vitals — who needs you, usage, free disk and the clock.
pub fn bar(bar: Bar) -> Div {
    let tabs = bar
        .tabs
        .into_iter()
        .map(|tab| {
            bar_tab(tab.id, tab.ordinal, tab.label, tab.current, tab.roll_up)
                .on_click(tab.press)
                .into_any_element()
        })
        .collect();
    let needs_you = bar.needs_you.and_then(|(count, press)| {
        bar_needs_you(count).map(|count| count.on_click(press).into_any_element())
    });
    let usage = bar
        .usage
        .iter()
        .filter_map(|(provider, limits)| bar_usage(*provider, limits))
        .map(IntoElement::into_any_element);
    let disk = bar
        .free_disk
        .and_then(bar_free_disk)
        .map(IntoElement::into_any_element);
    let items = needs_you
        .into_iter()
        .chain(usage)
        .chain(disk)
        .chain(Some(bar_clock(bar.clock).into_any_element()))
        .collect();
    bottom_bar(tabs, vitals(items).into_any_element())
}

/// The bottom bar (SPEC §5.7), the window's one bar (ADR 0014, amended by
/// ADR 0033): full width on `paint::BAR`, closed above by a `BAR_TOP`
/// hairline, `BAR_PAD_X` in from each end, `BAR_GAP` between its items,
/// Geist `FS_META` in `TEXT_DIM` (`CHROME_MUTED`, its step up on glass).
/// Left the view `tabs`; the `vitals` pushed to the right end.
///
/// The hairline lies on the frame's last row above the bar, as the
/// mockups draw it, not inside the bar's 30px: inside, it would leave 29
/// for the 22px tabs, and the half pixel the snap rounds down would seat
/// them 4 under the line and 3 over the window's foot.
fn bottom_bar(tabs: Vec<AnyElement>, vitals: AnyElement) -> Div {
    div()
        .debug_selector(|| "bottom-bar".into())
        .relative()
        .flex_shrink_0()
        .w_full()
        .h(px(STATUS_BAR_H))
        .bg(paint::BAR)
        .child(
            div()
                .debug_selector(|| "bottom-bar-rule".into())
                .absolute()
                .left_0()
                .right_0()
                .top(px(-HAIRLINE_W))
                .h(px(HAIRLINE_W))
                .bg(paint::BAR_TOP),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .size_full()
                .px(px(BAR_PAD_X))
                .gap(px(BAR_GAP))
                .font_family(FONT_UI)
                .text_size(px(FS_META))
                .line_height(px(LH_META))
                .text_color(rgb(CHROME_MUTED))
                .whitespace_nowrap()
                .overflow_hidden()
                .child(
                    div()
                        .debug_selector(|| "bottom-bar-tabs".into())
                        .flex()
                        .flex_row()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(SPACE_0_5))
                        .children(tabs),
                )
                .child(div().flex_1())
                .child(vitals),
        )
}

/// The bar's right end, in order: the need-you count, a usage segment per
/// provider, free disk, the clock — `BAR_GAP` apart, never shrunk.
fn vitals(items: Vec<AnyElement>) -> Div {
    div()
        .debug_selector(|| "bottom-bar-vitals".into())
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .gap(px(BAR_GAP))
        .children(items)
}

/// A round state dot `size` across in `ink`.
fn bar_dot(size: f32, ink: u32) -> Div {
    div()
        .flex_shrink_0()
        .size(px(size))
        .rounded_full()
        .bg(rgb(ink))
}

/// One view's tab (SPEC §5.7): `1 Solo`, `2 Work` — `BAR_TAB_H` high,
/// `BAR_TAB_PAD_X` in, radius `R_SMALL`, the digit in `TEXT_DIM` and the
/// name in `TEXT_BAR`, then the view's roll-up dot. The current view's
/// sits on `paint::SELECTED` with its name `TEXT_STRONG` at the label
/// weight; the rest take the hover fill under the pointer. The cockpit
/// wires the press.
fn bar_tab(
    id: SharedString,
    ordinal: usize,
    label: SharedString,
    current: bool,
    roll_up: Option<RollUp>,
) -> Stateful<Div> {
    let selector = id.clone();
    let key = id.clone();
    let key_dot = id.clone();
    div()
        .id(gpui::ElementId::Name(id))
        .debug_selector(move || selector.to_string())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(SPACE_1_5))
        .h(px(BAR_TAB_H))
        .px(px(BAR_TAB_PAD_X))
        .rounded(px(R_SMALL))
        .map(|tab| {
            if current {
                tab.cursor_pointer()
                    .bg(paint::SELECTED)
                    .text_color(rgb(TEXT_STRONG))
                    .font_weight(W_LABEL)
            } else {
                tab.hover_row(key).press_row().text_color(rgb(TEXT_BAR))
            }
        })
        .child(
            components::tabular(div().text_color(rgb(CHROME_MUTED)))
                .child(SharedString::from(ordinal.to_string())),
        )
        .child(label)
        .children(roll_up.map(|dot| {
            bar_dot(BAR_TAB_DOT, dot.ink()).debug_selector(move || format!("{key_dot}-dot"))
        }))
}

/// The need-you count (SPEC §5.7): a `BAR_NEED_DOT` `ATTENTION` dot and
/// `N need you` in `TEXT_BAR`. `None` at zero. The cockpit wires the press
/// to the Needs-you view. A press target among the vitals, it answers the
/// pointer as the nav's doors do — its words brighten to `TEXT_STRONG`
/// over the hover blend — rather than with a ground: a ground would need
/// padding the vitals' even gaps cannot spare.
fn bar_needs_you(count: usize) -> Option<Stateful<Div>> {
    (count > 0).then(|| {
        let blend = SharedString::from("bottom-bar-needs-you");
        let ink =
            ferrite_ui::motion::hover_blend(&blend, rgb(TEXT_BAR).into(), rgb(TEXT_STRONG).into());
        div()
            .id("bottom-bar-needs-you")
            .debug_selector(|| "bottom-bar-needs-you".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1_5))
            .cursor_pointer()
            .on_hover(ferrite_ui::motion::hover_listener(blend))
            .child(bar_dot(BAR_NEED_DOT, ATTENTION))
            .child(
                components::tabular(div().text_color(ink))
                    .child(SharedString::from(format!("{count} need you"))),
            )
    })
}

/// The window a provider's usage segment reads and its words: the more
/// used one, `this week` or `of 5h` (the five-hour window on a tie).
/// `None` when it has reported neither.
fn usage_window(limits: &ferrite_core::transcript::RateLimits) -> Option<(&'static str, f32)> {
    let five = limits
        .five_hour
        .map(|window| window.used_fraction.clamp(0.0, 1.0));
    let week = limits
        .weekly
        .map(|window| window.used_fraction.clamp(0.0, 1.0));
    match (five, week) {
        (Some(five), Some(week)) if week > five => Some(("this week", week)),
        (Some(five), _) => Some(("of 5h", five)),
        (None, Some(week)) => Some(("this week", week)),
        (None, None) => None,
    }
}

/// One provider's usage segment (SPEC §5.7): its glyph (`✻` Claude, `◉`
/// Codex) in its ink, then `14% this week` or `41% of 5h`. `None` when the
/// provider has reported no window.
fn bar_usage(
    provider: ferrite_core::store::Provider,
    limits: &ferrite_core::transcript::RateLimits,
) -> Option<Div> {
    let (window, used) = usage_window(limits)?;
    let (glyph, ink, name) = match provider {
        ferrite_core::store::Provider::Claude => ("\u{273b}", PROVIDER_CLAUDE, "claude"),
        ferrite_core::store::Provider::Codex => ("\u{25c9}", PROVIDER_CODEX, "codex"),
    };
    Some(
        div()
            .debug_selector(move || format!("bottom-bar-usage-{name}"))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1_5))
            .child(components::glyph(glyph, ink))
            .child(components::tabular(div()).child(SharedString::from(format!(
                "{}% {window}",
                (used * 100.).round() as u32
            )))),
    )
}

/// The free-disk segment: `free_disk_words`, or `None` when it says
/// nothing.
fn bar_free_disk(bytes: u64) -> Option<Div> {
    let (words, ink) = ferrite_ui::frame::free_disk_words(bytes)?;
    Some(
        components::tabular(
            div()
                .debug_selector(|| "bottom-bar-disk".into())
                .flex_shrink_0()
                .text_color(rgb(ink)),
        )
        .child(SharedString::from(words)),
    )
}

/// The clock (`progress::clock_label`) in `TEXT_BAR`, tabular figures.
fn bar_clock(clock: SharedString) -> Div {
    components::tabular(
        div()
            .debug_selector(|| "bottom-bar-clock".into())
            .flex_shrink_0()
            .text_color(rgb(TEXT_BAR)),
    )
    .child(clock)
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
            ferrite_pane::pane::init_components(cx);
            let mut strip = strip(ferrite_ui::theme::NAV_WIDTH, true, false);
            assert_eq!(strip.style().size.height, Some(px(WIN_CHROME_H).into()));
            assert_eq!(strip.style().position, Some(gpui::Position::Absolute));
        });
    }

    /// SPEC §5.7: a usage segment reads the provider's more used window,
    /// the five-hour one on a tie.
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
            Some(("of 5h", 0.41))
        );
        assert_eq!(
            usage_window(&limits(window(0.10), window(0.12))),
            Some(("this week", 0.12))
        );
        assert_eq!(
            usage_window(&limits(window(0.3), window(0.3))),
            Some(("of 5h", 0.3))
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
