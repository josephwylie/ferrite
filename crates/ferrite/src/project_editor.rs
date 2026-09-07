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
                .child(format!("Edit {title}")),
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

pub fn body() -> Scrollable<Div> {
    div()
        .flex()
        .flex_col()
        .min_h_0()
        .overflow_y_scrollbar()
        .px(px(PAD))
        .pb(px(PAD))
        .gap(px(8.))
}

pub fn section_label() -> Div {
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
                .child("Assigned directories"),
        )
        .child(
            div()
                .text_size(px(FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child("The original directory stays primary. Add or remove the others."),
        )
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
