//! Explicit Project editing modal, on the sheet recipe (`prefs::sheet`).

use gpui::component::{button::Button, Disableable};
use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, App, Div, SharedString};

use crate::pointer::Pointer;

use crate::components;
use crate::prefs;
use crate::theme::*;

const WIDTH: f32 = 600.;
/// The sheet sizes to its content up to here; past it the body scrolls.
const MAX_HEIGHT: f32 = 520.;

pub fn veil() -> Div {
    prefs::veil()
}

/// The sheet, as tall as its content up to `MAX_HEIGHT` (and the viewport
/// share every sheet keeps, of a window `viewport_h` tall), past which the
/// body scrolls between the fixed head and footer.
pub fn card(viewport_h: f32) -> Div {
    prefs::sheet_fit(WIDTH, MAX_HEIGHT.min(viewport_h * MODAL_VIEWPORT_FRACTION))
}

pub fn head(title: SharedString, close: impl IntoElement) -> Div {
    prefs::sheet_head(title, close)
}

pub fn close_button(cx: &App) -> Button {
    prefs::sheet_close("project-editor-close", "Close project editor", cx)
}

/// The card's form fields scroll between its fixed head and action footer.
/// The body takes its content's height and gives way (`flex_shrink`,
/// `min_h_0`) only when the sheet reaches its cap, so a short list draws a
/// short sheet and a long one scrolls. `MODAL_PAD` under the last field.
pub fn body() -> gpui::Stateful<Div> {
    div()
        .id("project-editor-body")
        .flex()
        .flex_col()
        .flex_shrink(1.)
        .min_h_0()
        .overflow_y_scroll()
        .px(px(MODAL_PAD))
        .pt(px(MODAL_GAP))
        .pb(px(MODAL_PAD))
        .gap(px(SPACE_2))
}

/// A section header: the UI section label, and its hint in prose.
pub fn section_label(title: &'static str, hint: &'static str) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .pt(px(SPACE_2))
        .child(components::section_label(title).pb(px(SPACE_0_5)))
        .child(
            div()
                .font_family(FONT_UI)
                .text_size(px(FS_PROSE_SM))
                .line_height(px(LH_PROSE_SM))
                .text_color(rgb(TEXT_MUTED))
                .child(hint),
        )
}

/// The Project name row: a section label over a field one step up from the
/// sheet (`RAISED_2`, like every control on it), recognizable before the
/// live editor holds any text. While the keyboard is in it, it wears the
/// one focus ring (`components::control_focus`), which takes no layout.
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
                .map(|field| components::focused(field, focused))
                .when(focused, |field| {
                    field.debug_selector(|| "project-name-focused".into())
                })
                .bg(rgb(RAISED_2))
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

/// The directory list: no frame and no rules, the rows `GAP_ROW` apart.
pub fn directory_list(rows: Vec<gpui::Stateful<Div>>) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(px(GAP_ROW))
        .children(rows)
}

/// The quiet text control under the list that adds a directory (`+ Add
/// directory`, or `+ Add main directory` while there is none): `TEXT_MUTED`,
/// brightening to `TEXT` under the pointer, on the text's own edge.
pub fn add_directory(label: &'static str, cx: &App) -> Button {
    components::faded_button(
        "add-project-directory",
        rgba(TRANSPARENT).into(),
        rgb(HOVER_RAISED).into(),
        rgb(FILL_HOVER).into(),
        rgb(TEXT_MUTED).into(),
        cx,
    )
    .tab_stop(true)
    .debug_selector(|| "project-add-directory".into())
    .group(ADD_GROUP)
    .h(px(FORM_CONTROL_H))
    .ml(px(-SPACE_2))
    .px(px(SPACE_2))
    .child(
        components::text_ui()
            .text_color(rgb(TEXT_MUTED))
            .group_hover(ADD_GROUP, |style| style.text_color(rgb(TEXT)))
            .child(label),
    )
}

const ADD_GROUP: &str = "project-add-directory";
const DESTRUCTIVE_GROUP: &str = "project-destructive";

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

/// One directory: its name, `main` beside the first, and its full path
/// under it in the code face, wrapping anywhere rather than cut — a path is
/// machine text. The row's height is its content's. Under the pointer it
/// takes a `FILL` ground hung `SPACE_2` outside the text's edge, so the
/// text stays on the sheet's column.
pub fn directory_row(
    index: usize,
    path: SharedString,
    main: bool,
    actions: impl IntoElement,
) -> gpui::Stateful<Div> {
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
    let selector: SharedString = format!("project-directory:{path}").into();
    div()
        .id(("project-directory-row", index))
        .flex()
        .items_center()
        .flex_shrink_0()
        .ml(px(-SPACE_2))
        .px(px(SPACE_2))
        .py(px(SPACE_1))
        .gap(px(SPACE_3))
        .rounded(px(R_CONTROL))
        .hover_raised(format!("project-directory-row-{index}"))
        .cursor_default()
        .child(
            div()
                .id(selector.clone())
                .debug_selector(move || selector.to_string())
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(SPACE_2))
                        .child(components::text_ui().min_w_0().truncate().child(name))
                        .when(main, |line| {
                            line.child(
                                components::text_meta()
                                    .flex_shrink_0()
                                    .debug_selector(|| "project-directory-main".into())
                                    .child("main"),
                            )
                        }),
                )
                .child(
                    div()
                        .font_family(FONT_CODE)
                        .text_size(px(FS_SM))
                        .line_height(px(LH_META))
                        .text_color(rgb(TEXT_MUTED))
                        .whitespace_normal()
                        .child(path),
                ),
        )
        .child(div().flex_shrink_0().child(actions))
}

/// A destructive action: quiet like any sheet control — `TEXT_MUTED` at
/// rest, `TEXT` on a `FILL` ground under the pointer, `FILL_HOVER` pressed.
/// No red and no wash: colour is state, and a verb is not one. Disabled, it
/// stays `TEXT_MUTED` and takes no face.
pub fn destructive_button(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    disabled: bool,
    cx: &App,
) -> Button {
    let (hover, press) = if disabled {
        (rgba(TRANSPARENT).into(), rgba(TRANSPARENT).into())
    } else {
        (rgb(HOVER_RAISED).into(), rgb(FILL_HOVER).into())
    };
    components::faded_button(
        id,
        rgba(TRANSPARENT).into(),
        hover,
        press,
        rgb(TEXT_MUTED).into(),
        cx,
    )
    .tab_stop(true)
    .group(DESTRUCTIVE_GROUP)
    .disabled(disabled)
    .when(disabled, |button| button.cursor_default())
    .h(px(FORM_CONTROL_H))
    .px(px(FORM_BUTTON_PAD_X))
    .child(
        components::text_ui()
            .font_weight(W_BODY)
            .text_color(rgb(TEXT_MUTED))
            .when(!disabled, |text| {
                text.group_hover(DESTRUCTIVE_GROUP, |style| style.text_color(rgb(TEXT)))
            })
            .child(label),
    )
}

/// Completion actions remain visible while the directory list scrolls.
pub fn footer(left: impl IntoElement, right: impl IntoElement) -> Div {
    prefs::sheet_footer(left, right)
}
