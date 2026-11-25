//! A field on a sheet: its label as an `INK_SUBTLE` row over a box that
//! holds a live editor, one row high or three for prose, ringed with the
//! Composer's hairline and the strong edge while the keyboard is in it.

use gpui::prelude::*;
use gpui::{div, px, rgb, Div, SharedString};

use crate::components;
use crate::theme::*;

/// A one-line field whose value follows `prefix`, drawn in the box before
/// it in `INK_SUBTLE` and cut from its start: a name in the folder the
/// prefix names.
pub fn prefixed_field(
    label: SharedString,
    prefix: SharedString,
    editor: impl IntoElement,
    focused: bool,
    focused_selector: &'static str,
) -> Div {
    field(
        label,
        editor,
        focused,
        focused_selector,
        false,
        Some(prefix),
    )
}

/// A path shown rather than typed, in a field's box with no label: the
/// folder it is in as `prefix` in `INK_SUBTLE`, cut from its start, then
/// `name` in `INK_STRONG`, and `action` at the box's end.
pub fn shown_path(prefix: SharedString, name: SharedString, action: impl IntoElement) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(SPACE_2))
        .h(px(FORM_CONTROL_H))
        .pl(px(FORM_FIELD_PAD_X))
        .pr(px(SPACE_1))
        .rounded(px(R_CONTROL))
        .border_1()
        .border_color(paint::FIELD_RING)
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(
                    components::text_ui()
                        .flex_shrink_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis_start()
                        .text_color(rgb(INK_SUBTLE))
                        .child(prefix),
                )
                .child(components::text_ui().flex_shrink_0().child(name)),
        )
        .child(div().flex_shrink_0().child(action))
}

/// A one-line field: its label as a `INK_SUBTLE` row over a box a cell
/// in, one row high, rounded as every control is (`R_CONTROL`) and ringed
/// with the Composer's hairline (`FIELD_RING`), the strong edge
/// (`CONTROL_FOCUS`, named `focused_selector`) while the keyboard is in
/// it, recognizable before the live editor holds any text.
pub fn text_field(
    label: SharedString,
    editor: impl IntoElement,
    focused: bool,
    focused_selector: &'static str,
) -> Div {
    field(label, editor, focused, focused_selector, false, None)
}

/// A field for prose: three lines tall at rest, growing with what is
/// written (to the Composer's own row limit, then it scrolls).
pub fn text_area(
    label: SharedString,
    editor: impl IntoElement,
    focused: bool,
    focused_selector: &'static str,
) -> Div {
    field(label, editor, focused, focused_selector, true, None)
}

/// A text area's least height: three lines and its padding.
const AREA_MIN_H: f32 = 3.0 * LH_UI + 2.0 * SPACE_1_5 + 2.0;

fn field(
    label: SharedString,
    editor: impl IntoElement,
    focused: bool,
    focused_selector: &'static str,
    area: bool,
    prefix: Option<SharedString>,
) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .child(components::section_label(label).pb(px(0.)))
        .child(
            div()
                .flex()
                .map(|field| {
                    if area {
                        field.items_start().min_h(px(AREA_MIN_H)).py(px(SPACE_1_5))
                    } else {
                        field.items_center().h(px(FORM_CONTROL_H))
                    }
                })
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
                        field.border_color(paint::FIELD_RING)
                    }
                })
                .children(prefix.map(|prefix| {
                    components::text_ui()
                        .flex_shrink_1()
                        .min_w_0()
                        .max_w(gpui::relative(0.7))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis_start()
                        .text_color(rgb(INK_SUBTLE))
                        .child(prefix)
                }))
                .child(div().min_w(px(8.0 * CH)).flex_1().child(editor)),
        )
}
