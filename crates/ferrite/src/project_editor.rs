//! Explicit Project editing modal.

use gpui::component::{
    button::Button,
    scroll::{Scrollable, ScrollableElement},
    Disableable,
};
use gpui::prelude::*;
use gpui::{div, point, px, rgb, rgba, App, BoxShadow, Div, FontWeight, SharedString};

use crate::components;
use crate::icons::{self, icon};
use crate::theme::{
    BLOCKED, FILL, FONT_MONO, FONT_UI, FORM_CONTROL_H, FS_LG, FS_MD, FS_MONO, GROUND, ICON_BUTTON,
    ICON_BUTTON_GLYPH, MENU, MODAL_GAP, MODAL_HEAD_H, MODAL_PAD, MODAL_VIEWPORT_FRACTION, PANE,
    RAISED, R_CONTROL, R_MENU, SHADOW_FAR, SHADOW_FAR_BLUR, SHADOW_FAR_SPREAD, SHADOW_FAR_Y,
    SHADOW_NEAR, SHADOW_NEAR_BLUR, SHADOW_NEAR_Y, TEXT, TEXT_2, TEXT_MUTED, TEXT_STRONG,
};

const WIDTH: f32 = 600.;
/// The first directory has room for its labels and the Name field. Each
/// additional directory adds one complete row until the body needs to scroll.
const BASE_HEIGHT: f32 = 320.;
const DIRECTORY_H: f32 = 52.;
const MAX_HEIGHT: f32 = 520.;

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

pub fn card(directory_count: usize) -> Div {
    let height = (BASE_HEIGHT + directory_count.saturating_sub(1) as f32 * (DIRECTORY_H + 8.))
        .min(MAX_HEIGHT);
    div()
        .flex()
        .flex_col()
        .w(px(WIDTH))
        .max_w(gpui::relative(MODAL_VIEWPORT_FRACTION))
        .h(px(height))
        .max_h(gpui::relative(MODAL_VIEWPORT_FRACTION))
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
        .h(px(MODAL_HEAD_H))
        .px(px(MODAL_PAD))
        .gap(px(MODAL_GAP))
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

pub fn close_button(cx: &App) -> Button {
    components::form_button("project-editor-close", cx)
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .tooltip("Close project editor")
        .child(icon(icons::CLOSE, ICON_BUTTON_GLYPH, TEXT_MUTED))
}

/// The card's form fields scroll between its fixed head and action footer.
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
        .px(px(MODAL_PAD))
        .pb(px(MODAL_PAD))
        .gap(px(8.))
}

pub fn section_label(title: &'static str, hint: &'static str) -> Div {
    div()
        .flex_shrink_0()
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

/// The Project name row: a label above a bounded input that remains
/// recognizable before the live editor contains any text.
pub fn name_field(editor: impl IntoElement) -> Div {
    div()
        .flex_shrink_0()
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
                .h(px(FORM_CONTROL_H))
                .flex_shrink_0()
                .px(px(10.))
                .rounded(px(R_CONTROL))
                .border_1()
                .border_color(rgb(FILL))
                .bg(rgb(PANE))
                .child(div().min_w_0().flex_1().child(editor)),
        )
}

/// A refusal from the registry, shown on the card that caused it rather
/// than on the nav behind it.
pub fn error_line(message: SharedString) -> Div {
    div()
        .flex_shrink_0()
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
        .min_h(px(DIRECTORY_H))
        .flex_shrink_0()
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
    cx: &App,
) -> Button {
    components::primary_button(id, disabled, cx)
        .debug_selector(|| "project-confirm".into())
        .h(px(FORM_CONTROL_H))
        .px(px(11.))
        .child(components::form_label(
            label,
            if disabled { TEXT_MUTED } else { GROUND },
        ))
}

pub fn directory_row(path: SharedString, role: &'static str, actions: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .min_h(px(DIRECTORY_H))
        .flex_shrink_0()
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
        .child(div().flex_shrink_0().child(actions))
}

pub fn action_button(id: impl Into<gpui::ElementId>, label: &'static str, cx: &App) -> Button {
    components::form_button(id, cx)
        .debug_selector(move || format!("project-{label}"))
        .h(px(FORM_CONTROL_H))
        .px(px(9.))
        .child(components::form_label(label, TEXT_2))
}

pub fn destructive_button(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    disabled: bool,
    cx: &App,
) -> Button {
    components::form_button(id, cx)
        .disabled(disabled)
        .when(disabled, |button| button.cursor_default())
        .h(px(FORM_CONTROL_H))
        .px(px(9.))
        .child(components::form_label(
            label,
            if disabled { TEXT_MUTED } else { BLOCKED },
        ))
}

/// Completion actions remain visible while the directory list scrolls.
pub fn footer(left: impl IntoElement, right: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .flex_shrink_0()
        .gap(px(MODAL_GAP))
        .px(px(MODAL_PAD))
        .py(px(MODAL_GAP))
        .child(left)
        .child(right)
}
