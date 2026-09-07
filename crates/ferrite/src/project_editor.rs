//! Explicit Project editing modal.

use gpui::component::{
    button::Button,
    scroll::{Scrollable, ScrollableElement},
    Disableable,
};
use gpui::prelude::*;
use gpui::{div, point, px, rgb, rgba, BoxShadow, Div, FontWeight, SharedString};

use crate::components;
use crate::icons::{self, icon};
use crate::theme::{
    BLOCKED, FONT_MONO, FONT_UI, FS_LG, FS_MD, FS_MONO, ICON_BUTTON, ICON_BUTTON_GLYPH, MENU,
    RAISED, R_CONTROL, R_MENU, SHADOW_FAR, SHADOW_FAR_BLUR, SHADOW_FAR_SPREAD, SHADOW_FAR_Y,
    SHADOW_NEAR, SHADOW_NEAR_BLUR, SHADOW_NEAR_Y, TEXT, TEXT_2, TEXT_MUTED, TEXT_STRONG,
};

const WIDTH: f32 = 640.;
/// A definite height, as on Settings: the body scrolls inside the card, and
/// a scroll container only knows what to scroll against a parent that has
/// already been given a height.
const HEIGHT: f32 = 420.;
const PAD: f32 = 18.;

pub fn veil() -> Div {
    div()
        .absolute()
        .inset_0()
        .cursor_default()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x0000008c))
}

pub fn card() -> Div {
    div()
        .flex()
        .flex_col()
        .w(px(WIDTH))
        .max_w(gpui::relative(0.94))
        .h(px(HEIGHT))
        .max_h(gpui::relative(0.84))
        .overflow_hidden()
        .rounded(px(R_MENU))
        .bg(rgb(MENU))
        .font_family(FONT_UI)
        .text_size(px(FS_MD))
        .text_color(rgb(TEXT))
        .shadow(vec![
            BoxShadow {
                inset: false,
                color: rgba(SHADOW_FAR).into(),
                offset: point(px(0.), px(SHADOW_FAR_Y)),
                blur_radius: px(SHADOW_FAR_BLUR),
                spread_radius: px(SHADOW_FAR_SPREAD),
            },
            BoxShadow {
                inset: false,
                color: rgba(SHADOW_NEAR).into(),
                offset: point(px(0.), px(SHADOW_NEAR_Y)),
                blur_radius: px(SHADOW_NEAR_BLUR),
                spread_radius: px(0.),
            },
        ])
}

pub fn head(title: SharedString, close: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(48.))
        .px(px(PAD))
        .gap(px(10.))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(px(FS_LG))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_STRONG))
                .child(title),
        )
        .child(
            div()
                .font_family(FONT_MONO)
                .text_size(px(FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child("esc close"),
        )
        .child(close)
}

pub fn close_button() -> Button {
    components::button("project-editor-close")
        .tab_stop(true)
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .tooltip("Close project editor")
        .child(icon(icons::CLOSE, ICON_BUTTON_GLYPH, TEXT_MUTED))
}

/// The card's content: everything under the head, scrolling within it.
/// `flex_1` against the card's definite height is what gives the scroll
/// container something to scroll inside — without it the body claims no
/// height at all and the card draws as a bare title bar.
pub fn body() -> Scrollable<Div> {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scrollbar()
        .px(px(PAD))
        .pb(px(PAD))
        .gap(px(8.))
}

pub fn section_label(title: &'static str, hint: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(3.))
        .pt(px(4.))
        .pb(px(2.))
        .child(
            div()
                .text_size(px(FS_MD))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_STRONG))
                .child(title),
        )
        .child(
            div()
                .text_size(px(FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child(hint),
        )
}

/// The Project name row: a label above the live editor, boxed like every
/// other control on the card so the caret has somewhere to sit.
pub fn name_field(editor: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .pt(px(4.))
        .child(
            div()
                .text_size(px(FS_MD))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_STRONG))
                .child("Name"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .min_h(px(34.))
                .px(px(10.))
                .rounded(px(R_CONTROL))
                .bg(rgb(RAISED))
                .child(div().min_w_0().flex_1().child(editor)),
        )
}

/// A refusal from the registry, shown on the card that caused it rather
/// than on the nav behind it.
pub fn error_line(message: SharedString) -> Div {
    div()
        .pt(px(4.))
        .font_family(FONT_MONO)
        .text_size(px(FS_MONO))
        .text_color(rgb(BLOCKED))
        .child(message)
}

/// The create card's empty state: no directory has been picked yet, so
/// there is nothing to be primary.
pub fn empty_directories() -> Div {
    div()
        .flex()
        .items_center()
        .min_h(px(52.))
        .px(px(12.))
        .rounded(px(R_CONTROL))
        .bg(rgb(RAISED))
        .font_family(FONT_MONO)
        .text_size(px(FS_MONO))
        .text_color(rgb(TEXT_MUTED))
        .child("No directory yet — add the main directory to begin.")
}

/// The confirming button: the one filled control on the card.
pub fn primary_button(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    disabled: bool,
) -> Button {
    components::button(id)
        .tab_stop(true)
        .disabled(disabled)
        .h(px(28.))
        .px(px(11.))
        .child(components::label(
            label,
            if disabled { TEXT_MUTED } else { TEXT_STRONG },
        ))
}

pub fn directory_row(path: SharedString, role: &'static str, actions: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .min_h(px(52.))
        .px(px(12.))
        .gap(px(12.))
        .rounded(px(R_CONTROL))
        .bg(rgb(RAISED))
        .child(icon(icons::FOLDER, 14., TEXT_MUTED))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(2.))
                .child(
                    div()
                        .font_family(FONT_MONO)
                        .text_size(px(FS_MONO))
                        .text_color(rgb(TEXT_2))
                        .truncate()
                        .child(path),
                )
                .child(
                    div()
                        .text_size(px(FS_MONO))
                        .text_color(rgb(TEXT_MUTED))
                        .child(role),
                ),
        )
        .child(actions)
}

pub fn action_button(id: impl Into<gpui::ElementId>, label: &'static str) -> Button {
    components::button(id)
        .tab_stop(true)
        .debug_selector(move || format!("project-{label}"))
        .h(px(28.))
        .px(px(9.))
        .child(components::label(label, TEXT_2))
}

pub fn destructive_button(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    disabled: bool,
) -> Button {
    components::button(id)
        .tab_stop(true)
        .disabled(disabled)
        .h(px(28.))
        .px(px(9.))
        .child(components::label(
            label,
            if disabled { TEXT_MUTED } else { BLOCKED },
        ))
}

pub fn footer(left: impl IntoElement, right: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .pt(px(8.))
        .child(left)
        .child(right)
}
