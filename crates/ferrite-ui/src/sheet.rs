//! Ferrite's sheets: the modal recipe Settings, the Project editor and the
//! image preview share. The Settings sheet's rules live in `theme.rs`
//! (`Settings sheet`, WP-E).
//!
//! **The sheet recipe**: a `VEIL` over the Cockpit; the sheet is the
//! float's shell at sheet size — `FLOAT_GROUND`, radius `R_BLOCK`, a 1px
//! `FLOAT_EDGE` ring, the float shadow; a `MODAL_HEAD_H` head (the title
//! in `TEXT_STRONG` at `W_STRONG` and the one close control, `×` with the
//! tooltip `Close esc`) over a `FLOAT_RULE`; a scrolling body; a footer
//! pinned under a `FLOAT_RULE`. Inside: rows and rules, words for controls.

use gpui::prelude::*;
use gpui::{div, px, rgb, App, Div, SharedString};

use gpui::component::button::Button;

use crate::components;
use crate::icons::{self, icon};
use crate::theme::*;

/// The dim veil over the Cockpit while a sheet is up: a press on it
/// closes the sheet (the cockpit wires that). It covers selectable
/// transcript text, so it owns the neutral cursor outside the sheet too.
pub fn veil() -> Div {
    components::veil().cursor_default()
}

/// A modal sheet, `width` × `height` at most `MODAL_VIEWPORT_FRACTION` of
/// the window: the float at sheet size. Its definite height is what lets
/// the body scroll.
pub fn sheet(width: f32, height: f32) -> Div {
    sheet_frame(width)
        .h(px(height))
        .max_h(gpui::relative(MODAL_VIEWPORT_FRACTION))
}

/// `sheet` sized to its content, up to `max_height`: a short form draws a
/// short sheet; past the cap its body scrolls.
pub fn sheet_fit(width: f32, max_height: f32) -> Div {
    sheet_frame(width).max_h(px(max_height))
}

fn sheet_frame(width: f32) -> Div {
    components::text_ui()
        .flex()
        .flex_col()
        .w(px(width))
        .max_w(gpui::relative(MODAL_VIEWPORT_FRACTION))
        .overflow_hidden()
        .rounded(px(R_BLOCK))
        .bg(FLOAT_GROUND)
        .border_1()
        .border_color(FLOAT_EDGE)
        .shadow(components::elevation(components::Elevation::Sheet))
}

/// A sheet's head: its title, then the close button, over a `FLOAT_RULE`.
/// The close button's tooltip names `esc`; no keycap repeats it.
pub fn sheet_head(title: impl Into<SharedString>, close: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(MODAL_GAP))
        .h(px(MODAL_HEAD_H))
        .pl(px(MODAL_PAD))
        .pr(px(SPACE_1))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .font_weight(W_STRONG)
                .text_color(rgb(TEXT_STRONG))
                .child(title.into()),
        )
        // The kit button's own tooltip takes only a string; the chord
        // suffix rides a wrapper, as on every keyed control.
        .child(
            div()
                .id("sheet-close-tip")
                .flex_shrink_0()
                .tooltip(crate::menu::action_tooltip("Close", "cockpit::Interrupt"))
                .child(close),
        )
        .border_b_1()
        .border_color(FLOAT_RULE)
}

/// A sheet's close button, the one close control on every sheet: the close
/// glyph in a 28px square, `paint::RAISED` under the pointer. `sheet_head`
/// gives it its tooltip, `Close` with `esc` (esc closes it too); `label` is
/// the longer name a screen reader hears.
pub fn sheet_close(id: &'static str, label: &'static str, cx: &App) -> Button {
    components::form_button(id, cx)
        .debug_selector(move || id.into())
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .accessibility_label(label)
        .child(icon(icons::CLOSE, ICON_BUTTON_GLYPH, TEXT_DIM))
}

/// The pinned footer under a `FLOAT_RULE`: secondary actions left, the
/// completing action right. It never scrolls with the body.
pub fn sheet_footer(left: impl IntoElement, right: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap(px(MODAL_GAP))
        .px(px(MODAL_PAD))
        .py(px(MODAL_GAP))
        .border_t_1()
        .border_color(FLOAT_RULE)
        .child(left)
        .child(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sheet is the float's shell at sheet size — the float ground, the
    /// `FLOAT_EDGE` ring, radius `R_BLOCK`, the one float shadow — over the
    /// veil.
    #[test]
    fn the_sheet_is_the_float_over_the_veil() {
        let mut drawn = sheet(SETTINGS_W, SETTINGS_H);
        let style = drawn.style();
        assert_eq!(style.background, Some(FLOAT_GROUND.into()));
        assert_eq!(style.border_color, Some(FLOAT_EDGE.into()));
        assert_eq!(style.box_shadow, Some(components::float_shadow()));
        assert_eq!(style.corner_radii.top_left, Some(px(R_BLOCK).into()));
        assert_eq!(veil().style().background, Some(gpui::rgba(VEIL).into()));
    }
}
