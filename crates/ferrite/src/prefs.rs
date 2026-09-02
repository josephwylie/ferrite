//! The Settings panel: drawing only, like `nav.rs`. A card floated over
//! the Cockpit on the menu ground, sections of rows, each row a label and
//! its choices as chips — the one on wears the fill. The cockpit owns the
//! values and wires every chip.

use gpui::prelude::*;
use gpui::{div, point, px, rgb, rgba, BoxShadow, Div, FontWeight, SharedString, Stateful};

use crate::icons::{self, icon};
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::{
    FILL, FONT_MONO, FONT_UI, FS_LG, FS_MD, FS_MONO, FS_SM, ICON_BUTTON, ICON_BUTTON_GLYPH,
    LINE_UI, MENU, MENU_PAD, RAISED, R_CHIP, R_CONTROL, R_MENU, SHADOW_FAR, SHADOW_FAR_BLUR,
    SHADOW_FAR_SPREAD, SHADOW_FAR_Y, SHADOW_NEAR, SHADOW_NEAR_BLUR, SHADOW_NEAR_Y, TEXT, TEXT_2,
    TEXT_MUTED, TEXT_STRONG,
};

/// The card's width; tall enough sections scroll inside it.
pub const WIDTH: f32 = 600.0;
const HEAD_H: f32 = 44.0;
const PAD: f32 = 16.0;
const ROW_GAP: f32 = 8.0;
const CHIP_H: f32 = 22.0;

/// The dim veil over the Cockpit while the panel is up: a press on it
/// closes the panel (the cockpit wires that).
pub fn veil() -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x0000008c))
}

/// The card: menu ground, the float shadow, no border.
pub fn card() -> Div {
    div()
        .flex()
        .flex_col()
        .w(px(WIDTH))
        .max_h(relative_h())
        .rounded(px(R_MENU))
        .bg(rgb(MENU))
        .font_family(FONT_UI)
        .shadow(vec![
            BoxShadow {
                color: rgba(SHADOW_FAR).into(),
                offset: point(px(0.), px(SHADOW_FAR_Y)),
                blur_radius: px(SHADOW_FAR_BLUR),
                spread_radius: px(SHADOW_FAR_SPREAD),
            },
            BoxShadow {
                color: rgba(SHADOW_NEAR).into(),
                offset: point(px(0.), px(SHADOW_NEAR_Y)),
                blur_radius: px(SHADOW_NEAR_BLUR),
                spread_radius: px(0.),
            },
        ])
}

fn relative_h() -> gpui::DefiniteLength {
    gpui::relative(0.86)
}

/// The head: the title, the escape hint, the close button (wired by the
/// caller).
pub fn head(close: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(HEAD_H))
        .pl(px(PAD))
        .pr(px(MENU_PAD + 4.))
        .gap(px(ROW_GAP))
        .child(
            div()
                .text_size(px(FS_LG))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_STRONG))
                .child("Settings"),
        )
        .child(div().flex_1())
        .child(
            div()
                .text_size(px(FS_MONO))
                .font_family(FONT_MONO)
                .text_color(rgb(TEXT_MUTED))
                .child("esc close"),
        )
        .child(close)
}

/// The 28px close button.
pub fn close_button() -> Stateful<Div> {
    div()
        .id("settings-close")
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .rounded(px(R_CONTROL))
        .text_size(px(FS_LG))
        .text_color(rgb(TEXT_MUTED))
        .hover_control()
        .press_control()
        .child("✕")
}

/// The scrolling body the sections stack in.
pub fn body(scroll: &gpui::ScrollHandle) -> Stateful<Div> {
    div()
        .id("settings-body")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(scroll)
        .px(px(PAD))
        .pb(px(PAD))
        .gap(px(PAD))
}

/// A section: a small title, then its rows.
pub fn section(title: &'static str, rows: Vec<gpui::AnyElement>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(ROW_GAP))
        .child(
            div()
                .text_size(px(FS_SM))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_MUTED))
                .child(title),
        )
        .children(rows)
}

/// A row: the label and its detail on the left, the choices on the right.
pub fn row(label: &'static str, detail: SharedString, choices: Vec<gpui::AnyElement>) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(PAD))
        .py(px(4.))
        .child(
            div()
                .flex()
                .flex_col()
                .w(px(180.))
                .flex_shrink_0()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(FS_MD))
                        .line_height(gpui::relative(LINE_UI))
                        .text_color(rgb(TEXT))
                        .child(label),
                )
                .when(!detail.is_empty(), |column| {
                    column.child(
                        div()
                            .text_size(px(FS_SM))
                            .line_height(gpui::relative(LINE_UI))
                            .text_color(rgb(TEXT_MUTED))
                            .child(detail),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .flex_wrap()
                .gap(px(6.))
                .children(choices),
        )
}

/// One choice chip: the selected one carries the fill and the strong ink.
pub fn chip(id: (&'static str, usize), label: SharedString, selected: bool) -> Stateful<Div> {
    let chip = div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(CHIP_H))
        .px(px(9.))
        .rounded(px(R_CHIP))
        .text_size(px(FS_SM))
        .child(label);
    if selected {
        chip.bg(rgb(FILL))
            .text_color(rgb(TEXT_STRONG))
            .hover_carried()
            .press_row()
    } else {
        chip.bg(rgb(RAISED))
            .text_color(rgb(TEXT_2))
            .hover_raised()
            .press_raised()
    }
}

/// A read-only fact row: label left, value right in the mono face.
pub fn fact(label: &'static str, value: SharedString) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(PAD))
        .py(px(2.))
        .child(
            div()
                .w(px(180.))
                .flex_shrink_0()
                .text_size(px(FS_MD))
                .text_color(rgb(TEXT))
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_family(FONT_MONO)
                .text_size(px(FS_MONO))
                .text_color(rgb(TEXT_2))
                .child(value),
        )
}

/// The nav chrome's gear: the door to this panel.
pub fn gear_button() -> Stateful<Div> {
    div()
        .id("settings-gear")
        .group("settings-gear")
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .rounded(px(R_CONTROL))
        .hover_control()
        .press_control()
        .child(
            icon(icons::GEAR, ICON_BUTTON_GLYPH, TEXT_MUTED)
                .group_hover("settings-gear", |style| style.text_color(rgb(TEXT))),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_selected_chip_carries_the_fill_and_the_rest_the_raised_ground() {
        let mut on = chip(("chip", 0), "Claude".into(), true);
        assert_eq!(on.style().background, Some(rgb(FILL).into()));
        let mut off = chip(("chip", 1), "Codex".into(), false);
        assert_eq!(off.style().background, Some(rgb(RAISED).into()));
    }
}
