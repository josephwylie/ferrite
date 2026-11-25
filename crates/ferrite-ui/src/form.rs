//! A sheet's form, on the sheet recipe (`crate::sheet`), in the float
//! grammar (theme WP-E): its card, head, scrolling body and footer, a
//! section label, the refusal line and the confirming and destructive
//! buttons. One face on the grid, section labels in `INK_SUBTLE`, words
//! for buttons. Every sheet's card draws with these.

use gpui::component::{button::Button, Disableable};
use gpui::prelude::*;
use gpui::{div, px, rgb, App, Div, SharedString};

use crate::components;
use crate::sheet;
use crate::theme::*;

const WIDTH: f32 = 600.;

/// The sheet sizes to its content up to here; past it the body scrolls. A
/// machine's folder browser and the rows around it fit under it.
const MAX_HEIGHT: f32 = 640.;

/// The sheet, as tall as its content up to `MAX_HEIGHT` (and the viewport
/// share every sheet keeps, of a window `viewport_h` tall), past which the
/// body scrolls between the fixed head and footer.
pub fn card(viewport_h: f32) -> Div {
    sheet::sheet_fit(WIDTH, MAX_HEIGHT.min(viewport_h * MODAL_VIEWPORT_FRACTION))
}

/// The least room above a draft's band a card is fitted into; with less,
/// it is centred on the window as any other.
const LEAST_ROOM: f32 = 320.;

/// `card`, over a draft whose band starts `band_top` down the window: no
/// taller than the room above the band, `SPACE_2` short of it, and centred
/// there, a bottom margin as tall as the window under the room lifting it
/// in the veil.
pub fn card_over(viewport_h: f32, band_top: Option<f32>) -> Div {
    let (height, lift) = room(viewport_h, band_top);
    sheet::sheet_fit(WIDTH, height).mb(px(lift))
}

/// `card_over` at its full height whatever it holds: a card whose
/// content moves as the operator works in it, a Machine's folder browser,
/// holds still.
pub fn steady_card_over(viewport_h: f32, band_top: Option<f32>) -> Div {
    let (height, lift) = room(viewport_h, band_top);
    sheet::sheet(WIDTH, height).mb(px(lift))
}

/// How tall a card over a draft whose band starts `band_top` down a window
/// `viewport_h` tall may be, and how far it is lifted to stay clear of the
/// band: not at all with no band, or too little room above it.
fn room(viewport_h: f32, band_top: Option<f32>) -> (f32, f32) {
    let room = band_top
        .map(|top| top - SPACE_2)
        .filter(|room| (LEAST_ROOM..viewport_h).contains(room));
    match room {
        Some(room) => (MAX_HEIGHT.min(room * MODAL_VIEWPORT_FRACTION), viewport_h - room),
        None => (MAX_HEIGHT.min(viewport_h * MODAL_VIEWPORT_FRACTION), 0.),
    }
}

pub fn head(title: SharedString, close: impl IntoElement) -> Div {
    sheet::sheet_head(title, close)
}

/// The card's form fields scroll between its fixed head and action footer.
/// The body takes its content's height and gives way (`flex_shrink`,
/// `min_h_0`) only when the sheet reaches its cap, so a short list draws a
/// short sheet and a long one scrolls. A row of air under the last field.
pub fn body() -> gpui::Stateful<Div> {
    div()
        .id("project-editor-body")
        .flex()
        .flex_col()
        .flex_shrink(1.)
        .min_h_0()
        .overflow_y_scroll()
        .px(px(MODAL_PAD))
        .pt(px(HALF_ROW))
        .pb(px(ROW))
        .gap(px(HALF_ROW))
}

/// A section header: its title as a `INK_SUBTLE` row, its hint under it in
/// the same ink.
pub fn section_label(title: &'static str, hint: &'static str) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .pt(px(HALF_ROW))
        .child(components::section_label(title).pb(px(0.)))
        .child(
            components::text_ui()
                .text_color(rgb(INK_SUBTLE))
                .child(hint),
        )
}

/// A refusal from the registry, shown on the card that caused it rather
/// than on the nav behind it: the one blocked line.
pub fn error_line(message: SharedString) -> Div {
    components::text_ui()
        .flex_shrink_0()
        .text_color(rgb(BLOCKED))
        .child(message)
}

/// The directory list: no frame, the rows flush on the grid.
pub fn directory_list(rows: impl IntoIterator<Item = impl IntoElement>) -> Div {
    div().flex_shrink_0().flex().flex_col().children(rows)
}

/// The confirming button: the one filled control on the card, `INK_STRONG`
/// with dark ink.
pub fn primary_button(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    disabled: bool,
    cx: &App,
) -> Button {
    components::primary_button(id, disabled, cx)
        .h(px(FORM_CONTROL_H))
        .px(px(FORM_BUTTON_PAD_X))
        .child(components::form_label(
            label,
            if disabled { INK_SUBTLE } else { ON_PRIMARY },
        ))
}

/// A destructive action, Claude's (claude/confirm-dialog.png `Delete this
/// session`): its word in `BLOCKED` in a 1px `BLOCKED` box, no fill at
/// rest, `BLOCKED_WASH` under the pointer and pressed. Disabled, it is
/// `INK_SUBTLE` in a `paint::FLOAT_EDGE` box and takes no face.
pub fn destructive_button(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    disabled: bool,
    cx: &App,
) -> Button {
    let none: gpui::Hsla = gpui::rgba(TRANSPARENT).into();
    let wash: gpui::Hsla = gpui::rgba(BLOCKED_WASH).into();
    let (hover, ink) = if disabled {
        (none, INK_SUBTLE)
    } else {
        (wash, BLOCKED)
    };
    components::faded_button(id, none, hover, hover, rgb(ink).into(), cx)
        .tab_stop(true)
        .disabled(disabled)
        .when(disabled, |button| button.cursor_default())
        .h(px(FORM_CONTROL_H))
        .px(px(FORM_BUTTON_PAD_X))
        .border_1()
        .border_color(if disabled {
            paint::FLOAT_EDGE.into()
        } else {
            gpui::Hsla::from(rgb(BLOCKED))
        })
        .child(
            components::text_ui()
                .font_weight(W_BODY)
                .text_color(rgb(ink))
                .child(label),
        )
}

/// Completion actions remain visible while the directory list scrolls.
pub fn footer(left: impl IntoElement, right: impl IntoElement) -> Div {
    sheet::sheet_footer(left, right)
}
