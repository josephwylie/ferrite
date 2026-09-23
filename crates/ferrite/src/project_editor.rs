//! Explicit Project editing modal, on the sheet recipe (`prefs::sheet`).

use gpui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    scroll::{Scrollable, ScrollableElement},
    Disableable,
};
use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, App, Div, SharedString};

use crate::components;
use crate::icons::{self, icon};
use crate::prefs;
use crate::theme::*;

const WIDTH: f32 = 600.;
/// The first directory has room for its labels and the Name field. Each
/// additional directory adds one complete row until the body needs to scroll.
const BASE_HEIGHT: f32 = 320.;
const DIRECTORY_H: f32 = 52.;
const MAX_HEIGHT: f32 = 520.;

pub fn veil() -> Div {
    prefs::veil()
}

/// The sheet, grown by one directory row (and its rule) per additional
/// directory until the body scrolls.
pub fn card(directory_count: usize) -> Div {
    let height = (BASE_HEIGHT + directory_count.saturating_sub(1) as f32 * (DIRECTORY_H + 1.))
        .min(MAX_HEIGHT);
    prefs::sheet(WIDTH, height)
}

pub fn head(title: SharedString, close: impl IntoElement) -> Div {
    prefs::sheet_head(title, close)
}

pub fn close_button(cx: &App) -> Button {
    prefs::sheet_close("project-editor-close", "Close project editor", cx)
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
        .pt(px(MODAL_GAP))
        .pb(px(MODAL_PAD))
        .gap(px(SPACE_2))
}

/// A section header: the mono section label, and its hint in prose.
pub fn section_label(title: &'static str, hint: &'static str) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .pt(px(SPACE_2))
        .child(components::section_label(title).pb(px(SPACE_0_5)))
        .child(
            div()
                .font_family(FONT_PROSE)
                .text_size(px(FS_PROSE_SM))
                .line_height(px(LH_PROSE_SM))
                .text_color(rgb(TEXT_MUTED))
                .child(hint),
        )
}

/// The Project name row: a section label over a field recessed into the
/// sheet (`PANE`, the strong hairline edge), recognizable before the live
/// editor holds any text. While the keyboard is in it the edge is the
/// focus ink, like every other field's; the edge is always in layout.
pub fn name_field(editor: impl IntoElement, focused: bool) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .child(components::section_label("Name"))
        .child(
            div()
                .flex()
                .items_center()
                .h(px(FORM_CONTROL_H))
                .flex_shrink_0()
                .px(px(FORM_FIELD_PAD_X))
                .rounded(px(R_CONTROL))
                .border_1()
                .border_color(if focused {
                    rgb(FOCUS_RING)
                } else {
                    rgba(HAIRLINE_STRONG)
                })
                .when(focused, |field| {
                    field.debug_selector(|| "project-name-focused".into())
                })
                .bg(rgb(PANE))
                .child(div().min_w_0().flex_1().child(editor)),
        )
}

/// A refusal from the registry, shown on the card that caused it rather
/// than on the nav behind it: the one blocked line.
pub fn error_line(message: SharedString) -> Div {
    components::text_meta()
        .flex_shrink_0()
        .pt(px(SPACE_1))
        .text_color(rgb(BLOCKED))
        .child(message)
}

/// The directory list: one edged group, its rows split by the rule weight,
/// no slab per row.
pub fn directory_list(rows: Vec<Div>) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .rounded(px(R_CONTROL))
        .border_1()
        .border_color(rgba(HAIRLINE_STRONG))
        .overflow_hidden()
        .children(rows.into_iter().enumerate().map(|(index, row)| {
            row.when(index > 0, |row| {
                row.border_t_1().border_color(rgba(HAIRLINE))
            })
        }))
}

/// The create card's empty state: no directory has been picked yet, so
/// there is nothing to be primary.
pub fn empty_directories() -> Div {
    div()
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(SPACE_0_5))
        .min_h(px(DIRECTORY_H))
        .px(px(SPACE_3))
        .child(
            components::text_ui()
                .text_color(rgb(TEXT_2))
                .child("No directory yet"),
        )
        .child(components::text_meta().child("Add the main directory to begin."))
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
        .px(px(FORM_BUTTON_PAD_X))
        .child(components::form_label(
            label,
            if disabled { TEXT_MUTED } else { ON_ACCENT },
        ))
}

/// One directory: its folder mark, its name over its role and full path
/// (the path's tooltip holds the whole of it), and its actions.
pub fn directory_row(path: SharedString, role: &'static str, actions: impl IntoElement) -> Div {
    // The final directory component distinguishes neighboring project roots;
    // a shared parent prefix does not. Native Path semantics also preserve
    // Windows drive/UNC roots, which have no file name, through the fallback.
    let name: SharedString = std::path::Path::new(path.as_ref())
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path.as_ref())
        .to_string()
        .into();
    let tooltip = path.clone();
    let selector: SharedString = format!("project-directory:{path}").into();
    div()
        .flex()
        .items_center()
        .min_h(px(DIRECTORY_H))
        .flex_shrink_0()
        .px(px(SPACE_3))
        .gap(px(SPACE_3))
        .child(icon(icons::FOLDER, ROW_ICON, TEXT_MUTED))
        .child(
            div()
                .id(selector.clone())
                .debug_selector(move || selector.to_string())
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .tooltip(crate::menu::tooltip(tooltip))
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(SPACE_2))
                        .child(components::text_ui().min_w_0().truncate().child(name))
                        .child(components::text_meta().flex_shrink_0().child(role)),
                )
                .child(components::text_meta().truncate().child(path)),
        )
        .child(div().flex_shrink_0().child(actions))
}

pub fn action_button(id: impl Into<gpui::ElementId>, label: &'static str, cx: &App) -> Button {
    components::form_button(id, cx)
        .debug_selector(move || format!("project-{label}"))
        .h(px(FORM_CONTROL_H))
        .px(px(FORM_BUTTON_PAD_X))
        .child(components::form_label(label, TEXT_2))
}

/// A destructive action: quiet at rest (colour is state), the blocked wash
/// under the pointer. Disabled, it explains itself in `TEXT_MUTED`.
pub fn destructive_button(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    disabled: bool,
    cx: &App,
) -> Button {
    components::form_button(id, cx)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(rgb(TEXT_2).into())
                .hover(rgba(BLOCKED_WASH).into())
                .active(rgba(BLOCKED_WASH).into()),
        )
        .disabled(disabled)
        .when(disabled, |button| button.cursor_default())
        .h(px(FORM_CONTROL_H))
        .px(px(FORM_BUTTON_PAD_X))
        .child(components::form_label(
            label,
            if disabled { TEXT_MUTED } else { TEXT_2 },
        ))
}

/// Completion actions remain visible while the directory list scrolls.
pub fn footer(left: impl IntoElement, right: impl IntoElement) -> Div {
    prefs::sheet_footer(left, right)
}
