//! The Project card's own rows, on a sheet's form (`ferrite_ui::form`):
//! the Project's directories.

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
