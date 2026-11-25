//! The Project card's own rows, on a sheet's form (`ferrite_ui::form`):
//! a machine's folder browser and the Project's directories.

use gpui::component::button::Button;
use gpui::prelude::*;
use gpui::{div, px, rgb, App, Div, SharedString};

use ferrite_ui::pointer::Pointer;

use ferrite_ui::components;
use ferrite_ui::sheet;
use ferrite_ui::theme::*;

pub fn close_button(cx: &App) -> Button {
    sheet::sheet_close("project-editor-close", "Close project editor", cx)
}

/// One row of a folder browser, the Files tree's row (`FILES_ROW_H`, its
/// mark in a chevron-wide column, `FS_LABEL` in `INK_STRONG`) as the shared
/// press: a folder to go into, or `..` out of one. The row the arrows rest
/// on is `paint::SELECTED`, as the tree's open file is.
pub fn folder_row(
    id: impl Into<gpui::ElementId>,
    mark: &'static str,
    label: SharedString,
    cursor: bool,
    cx: &App,
) -> Button {
    let rest = if cursor {
        paint::SELECTED
    } else {
        Paint(TRANSPARENT)
    };
    components::form_button_on(id, rest, cx)
        .w_full()
        .h(px(FILES_ROW_H))
        .p_0()
        .rounded(px(R_CONTROL))
        .child(
            folder_line(mark).child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(FS_LABEL))
                    .line_height(px(LH_UI))
                    .text_color(rgb(INK_STRONG))
                    .child(label),
            ),
        )
}

/// A new folder's name, typed in a row of the browser's: the field's ring
/// a row's height inside it, `Folder name` while it is empty.
pub fn folder_name_row(editor: impl IntoElement, focused: bool, empty: bool) -> Div {
    folder_line(ferrite_ui::icons::FOLDER_PLUS)
        .h(px(FILES_ROW_H))
        .child(
            div()
                .relative()
                .flex()
                .items_center()
                .flex_1()
                .min_w_0()
                .h(px(FILES_ROW_H - SPACE_1))
                .px(px(FORM_FIELD_PAD_X))
                .rounded(px(R_CONTROL))
                .border_1()
                .border_color(if focused {
                    gpui::Hsla::from(rgb(CONTROL_FOCUS))
                } else {
                    paint::FIELD_RING.into()
                })
                .child(div().flex_1().min_w_0().child(editor))
                .when(empty, |field| {
                    field.child(
                        components::text_ui()
                            .absolute()
                            .left(px(FORM_FIELD_PAD_X))
                            .text_color(rgb(INK_SUBTLE))
                            .child("Folder name"),
                    )
                }),
        )
}

/// A note in a folder browser's list, a row's height at least, on the
/// edge its rows' names start from: two lines at most, whole in its
/// tooltip.
pub fn folder_note(note: Div, text: SharedString) -> gpui::Stateful<Div> {
    div()
        .id("folder-note")
        .flex()
        .items_center()
        .min_h(px(FILES_ROW_H))
        .pl(px(SPACE_1 + ICON_CHEVRON + SPACE_1 + SPACE_1_5))
        .pr(px(SPACE_2))
        .tooltip(ferrite_ui::menu::tooltip(text))
        .child(div().flex_1().min_w_0().line_clamp(2).child(note))
}

/// A folder row's line: its mark in a chevron-wide column, as the Files
/// tree sets its rows.
fn folder_line(mark: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .gap(px(SPACE_1_5))
        .pl(px(SPACE_1))
        .pr(px(SPACE_2))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .justify_center()
                .w(px(ICON_CHEVRON + SPACE_1))
                .child(ferrite_ui::icons::icon(mark, ICON_CHEVRON, INK_SUBTLE)),
        )
}

/// The quiet text control under the list that adds a directory (`+ Add
/// directory`, or `+ Add main directory` while there is none): `INK_SUBTLE`,
/// `INK_STRONG` on `paint::RAISED` under the pointer, on the text's own edge.
pub fn add_directory(label: &'static str, cx: &App) -> Button {
    components::form_button("add-project-directory", cx)
        .debug_selector(|| "project-add-directory".into())
        .group(ADD_GROUP)
        .h(px(ROW))
        .ml(px(-CH))
        .px(px(CH))
        .child(
            components::text_ui()
                .text_color(rgb(INK_SUBTLE))
                .group_hover(ADD_GROUP, |style| style.text_color(rgb(INK_STRONG)))
                .child(label),
        )
}

const ADD_GROUP: &str = "project-add-directory";

/// One directory: its name, `main` beside the first, and its path under
/// it as the operator reads it (`shown`, home spelled `~`) in `INK_SUBTLE`,
/// one line cut from its start — a path's end names it — and whole in the
/// tooltip. Under the pointer the row takes `paint::RAISED`, hung a cell
/// outside the text's edge, so the text stays on the sheet's column.
pub fn directory_row(
    index: usize,
    path: SharedString,
    shown: SharedString,
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
        .tooltip(ferrite_ui::menu::tooltip(path.clone()))
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
                                .text_color(rgb(INK_STRONG))
                                .child(name),
                        )
                        .when(main, |line| {
                            line.child(
                                components::text_ui()
                                    .flex_shrink_0()
                                    .text_color(rgb(INK_SUBTLE))
                                    .debug_selector(|| "project-directory-main".into())
                                    .child("main"),
                            )
                        }),
                )
                .child(
                    components::text_ui()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis_start()
                        .text_color(rgb(INK_SUBTLE))
                        .child(shown),
                ),
        )
        .child(div().flex_shrink_0().child(actions))
}
