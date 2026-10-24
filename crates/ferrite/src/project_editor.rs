//! Explicit Project editing modal, on the sheet recipe (`prefs::sheet`), in
//! the float grammar (theme WP-E): one face on the grid, section labels in
//! `TEXT_DIM`, a field as a 1px box, words for buttons.

use gpui::component::{button::Button, Disableable};
use gpui::prelude::*;
use gpui::{div, px, rgb, App, Div, SharedString};

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

/// A section header: its title as a `TEXT_DIM` row, its hint under it in
/// the same ink.
pub fn section_label(title: &'static str, hint: &'static str) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .pt(px(HALF_ROW))
        .child(components::section_label(title).pb(px(0.)))
        .child(components::text_ui().text_color(rgb(TEXT_DIM)).child(hint))
}

/// The Project name row: its label as a `TEXT_DIM` row over a field — a
/// box a cell in, one row high, rounded as every control is (`R_CONTROL`)
/// and ringed with the Composer's hairline (`FIELD_RING`), the strong edge
/// (`CONTROL_FOCUS`) while the keyboard is in it — recognizable before the
/// live editor holds any text.
pub fn name_field(editor: impl IntoElement, focused: bool) -> Div {
    field("Name".into(), editor, focused, "project-name-focused")
}

/// On a machine (ADR 0026), where no folder dialog can browse: the row a
/// directory on the machine is typed into, in the name row's field.
pub fn path_field(machine: &str, editor: impl IntoElement, focused: bool) -> Div {
    field(
        format!("Directory on {machine}").into(),
        editor,
        focused,
        "project-path-focused",
    )
}

fn field(
    label: SharedString,
    editor: impl IntoElement,
    focused: bool,
    focused_selector: &'static str,
) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .child(components::section_label(label).pb(px(0.)))
        .child(
            div()
                .flex()
                .items_center()
                .h(px(FORM_CONTROL_H))
                .flex_shrink_0()
                .px(px(FORM_FIELD_PAD_X))
                .rounded(px(R_CONTROL))
                .border_1()
                .map(|field| {
                    if focused {
                        field
                            .border_color(rgb(CONTROL_FOCUS))
                            .debug_selector(|| focused_selector.into())
                    } else {
                        field.border_color(gpui::rgba(FIELD_RING))
                    }
                })
                .child(div().min_w_0().flex_1().child(editor)),
        )
}

/// One directory the machine's Server listed under what the path field
/// holds: its name and a `/`, `TEXT_DIM`, `paint::RAISED` under the
/// pointer, hung a cell outside the text's edge like a directory row.
pub fn path_completion(index: usize, name: &str) -> gpui::Stateful<Div> {
    let selector: SharedString = format!("project-path-completion:{name}").into();
    div()
        .id(("project-path-completion", index))
        .debug_selector(move || selector.to_string())
        .flex()
        .items_center()
        .flex_shrink_0()
        .h(px(ROW))
        .ml(px(-CH))
        .px(px(CH))
        .hover_raised(format!("project-path-completion-{index}"))
        .cursor_default()
        .child(
            components::text_ui()
                .min_w_0()
                .truncate()
                .text_color(rgb(TEXT_DIM))
                .child(format!("{name}/")),
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
pub fn directory_list(rows: Vec<gpui::Stateful<Div>>) -> Div {
    div().flex_shrink_0().flex().flex_col().children(rows)
}

/// The quiet text control under the list that adds a directory (`+ Add
/// directory`, or `+ Add main directory` while there is none): `TEXT_DIM`,
/// `TEXT` on `paint::RAISED` under the pointer, on the text's own edge.
pub fn add_directory(label: &'static str, cx: &App) -> Button {
    components::form_button("add-project-directory", cx)
        .debug_selector(|| "project-add-directory".into())
        .group(ADD_GROUP)
        .h(px(ROW))
        .ml(px(-CH))
        .px(px(CH))
        .child(
            components::text_ui()
                .text_color(rgb(TEXT_DIM))
                .group_hover(ADD_GROUP, |style| style.text_color(rgb(TEXT)))
                .child(label),
        )
}

const ADD_GROUP: &str = "project-add-directory";
const DESTRUCTIVE_GROUP: &str = "project-destructive";

/// The confirming button: the one filled control on the card, `TEXT_STRONG`
/// with dark ink.
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
            if disabled { TEXT_DIM } else { ON_PRIMARY },
        ))
}

/// One directory: its name, `main` beside the first, and its full path
/// under it in `TEXT_DIM`, wrapping anywhere rather than cut — a path is
/// machine text. The row's height is its content's. Under the pointer it
/// takes `paint::RAISED`, hung a cell outside the text's edge, so the text
/// stays on the sheet's column.
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
        .ml(px(-CH))
        .px(px(CH))
        .py(px(SPACE_1))
        .gap(px(2.0 * CH))
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
                        .gap(px(2.0 * CH))
                        .child(
                            components::text_ui()
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(TEXT))
                                .child(name),
                        )
                        .when(main, |line| {
                            line.child(
                                components::text_ui()
                                    .flex_shrink_0()
                                    .text_color(rgb(TEXT_DIM))
                                    .debug_selector(|| "project-directory-main".into())
                                    .child("main"),
                            )
                        }),
                )
                .child(
                    components::text_ui()
                        .text_color(rgb(LINK_INK))
                        .whitespace_normal()
                        .child(path),
                ),
        )
        .child(div().flex_shrink_0().child(actions))
}

/// A destructive action: a word in a 1px `paint::LINE2` box (the
/// prototype's quick buttons) — `TEXT_DIM` at rest, `TEXT` on
/// `paint::RAISED` under the pointer. No red and no wash: colour is state,
/// and a verb is not one. Disabled, it stays `TEXT_DIM` and takes no face.
pub fn destructive_button(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    disabled: bool,
    cx: &App,
) -> Button {
    let none: gpui::Hsla = gpui::rgba(TRANSPARENT).into();
    let (hover, press): (gpui::Hsla, gpui::Hsla) = if disabled {
        (none, none)
    } else {
        (paint::RAISED.into(), paint::PRESS.into())
    };
    components::faded_button(id, none, hover, press, rgb(TEXT_DIM).into(), cx)
        .tab_stop(true)
        .group(DESTRUCTIVE_GROUP)
        .disabled(disabled)
        .when(disabled, |button| button.cursor_default())
        .h(px(FORM_CONTROL_H))
        .px(px(FORM_BUTTON_PAD_X))
        .border_1()
        .border_color(paint::LINE2)
        .child(
            components::text_ui()
                .font_weight(W_BODY)
                .text_color(rgb(TEXT_DIM))
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
