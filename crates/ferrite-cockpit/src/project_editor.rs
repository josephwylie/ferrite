//! Explicit Project editing modal, on the sheet recipe (`ferrite_ui::sheet`), in
//! the float grammar (theme WP-E): one face on the grid, section labels in
//! `INK_SUBTLE`, a field as a 1px box, words for buttons.

use gpui::component::{button::Button, Disableable};
use gpui::prelude::*;
use gpui::{div, px, rgb, App, Div, SharedString};

use ferrite_ui::pointer::Pointer;

use ferrite_ui::components;
use ferrite_ui::sheet;
use ferrite_ui::theme::*;

pub const WIDTH: f32 = 600.;
/// A card that reports rather than asks (the setup sheet's progress and
/// what a copy did): as wide as the Background work sheet.
pub const COMPACT_WIDTH: f32 = BACKGROUND_DIALOG_W;
/// The sheet sizes to its content up to here; past it the body scrolls. A
/// machine's folder browser and the rows around it fit under it.
const MAX_HEIGHT: f32 = 640.;

/// The sheet, as tall as its content up to `MAX_HEIGHT` (and the viewport
/// share every sheet keeps, of a window `viewport_h` tall), past which the
/// body scrolls between the fixed head and footer.
pub fn card(viewport_h: f32) -> Div {
    sheet::sheet_fit(WIDTH, MAX_HEIGHT.min(viewport_h * MODAL_VIEWPORT_FRACTION))
}

/// The least room above a draft's band a card is fitted into; with less,
/// it is centred on the window as any other.
const LEAST_ROOM: f32 = 320.;

/// `card`, over a draft whose band starts `band_top` down the window: no
/// taller than the room above the band, `SPACE_2` short of it, and centred
/// there, a bottom margin as tall as the window under the room lifting it
/// in the veil.
pub fn card_over(viewport_h: f32, band_top: Option<f32>) -> Div {
    let (height, lift) = room(viewport_h, band_top);
    sheet::sheet_fit(WIDTH, height).mb(px(lift))
}

/// `card_over` at its full height whatever it holds: a card whose
/// content moves as the operator works in it, a Machine's folder browser,
/// holds still.
pub fn steady_card_over(viewport_h: f32, band_top: Option<f32>) -> Div {
    let (height, lift) = room(viewport_h, band_top);
    sheet::sheet(WIDTH, height).mb(px(lift))
}

/// How tall a card over a draft whose band starts `band_top` down a window
/// `viewport_h` tall may be, and how far it is lifted to stay clear of the
/// band: not at all with no band, or too little room above it.
pub fn room(viewport_h: f32, band_top: Option<f32>) -> (f32, f32) {
    let room = band_top
        .map(|top| top - SPACE_2)
        .filter(|room| (LEAST_ROOM..viewport_h).contains(room));
    match room {
        Some(room) => (MAX_HEIGHT.min(room * MODAL_VIEWPORT_FRACTION), viewport_h - room),
        None => (MAX_HEIGHT.min(viewport_h * MODAL_VIEWPORT_FRACTION), 0.),
    }
}

pub fn head(title: SharedString, close: impl IntoElement) -> Div {
    sheet::sheet_head(title, close)
}

pub fn close_button(cx: &App) -> Button {
    sheet::sheet_close("project-editor-close", "Close project editor", cx)
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

/// A section header: its title as a `INK_SUBTLE` row, its hint under it in
/// the same ink.
pub fn section_label(title: &'static str, hint: &'static str) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .pt(px(HALF_ROW))
        .child(components::section_label(title).pb(px(0.)))
        .child(
            components::text_ui()
                .text_color(rgb(INK_SUBTLE))
                .child(hint),
        )
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

/// A refusal from the registry, shown on the card that caused it rather
/// than on the nav behind it: the one blocked line.
pub fn error_line(message: SharedString) -> Div {
    components::text_ui()
        .flex_shrink_0()
        .text_color(rgb(BLOCKED))
        .child(message)
}

/// The directory list: no frame, the rows flush on the grid.
pub fn directory_list(rows: impl IntoIterator<Item = impl IntoElement>) -> Div {
    div().flex_shrink_0().flex().flex_col().children(rows)
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

/// The confirming button: the one filled control on the card, `INK_STRONG`
/// with dark ink.
pub fn primary_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    disabled: bool,
    cx: &App,
) -> Button {
    components::primary_button(id, disabled, cx)
        .debug_selector(|| "project-confirm".into())
        .h(px(FORM_CONTROL_H))
        .px(px(FORM_BUTTON_PAD_X))
        .child(components::form_label(
            label,
            if disabled { INK_SUBTLE } else { ON_PRIMARY },
        ))
}

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

/// A destructive action, Claude's (claude/confirm-dialog.png `Delete this
/// session`): its word in `BLOCKED` in a 1px `BLOCKED` box, no fill at
/// rest, `BLOCKED_WASH` under the pointer and pressed. Disabled, it is
/// `INK_SUBTLE` in a `paint::FLOAT_EDGE` box and takes no face.
pub fn destructive_button(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    disabled: bool,
    cx: &App,
) -> Button {
    let none: gpui::Hsla = gpui::rgba(TRANSPARENT).into();
    let wash: gpui::Hsla = gpui::rgba(BLOCKED_WASH).into();
    let (hover, ink) = if disabled {
        (none, INK_SUBTLE)
    } else {
        (wash, BLOCKED)
    };
    components::faded_button(id, none, hover, hover, rgb(ink).into(), cx)
        .tab_stop(true)
        .disabled(disabled)
        .when(disabled, |button| button.cursor_default())
        .h(px(FORM_CONTROL_H))
        .px(px(FORM_BUTTON_PAD_X))
        .border_1()
        .border_color(if disabled {
            paint::FLOAT_EDGE.into()
        } else {
            gpui::Hsla::from(rgb(BLOCKED))
        })
        .child(
            components::text_ui()
                .font_weight(W_BODY)
                .text_color(rgb(ink))
                .child(label),
        )
}

/// Completion actions remain visible while the directory list scrolls.
pub fn footer(left: impl IntoElement, right: impl IntoElement) -> Div {
    sheet::sheet_footer(left, right)
}
