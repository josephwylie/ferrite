//! The shared choice controls: a chooser and a segmented control, the
//! faces Settings drew first, used by Settings and the Factory alike. Each
//! is real buttons (tab stops, named for assistive technology, a hover
//! ground); the caller says what a press does and which menu opens.

use gpui::component::button::Button;
use gpui::component::Selectable;
use gpui::prelude::*;
use gpui::{div, px, rgb, App, Div, ElementId, SharedString};

use crate::components;
use crate::icons::{self, icon};
use crate::theme::*;

/// How a control stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Look {
    /// What it holds is invalid: its ring and value are `BLOCKED`.
    pub flagged: bool,
    /// As wide as its parent gives it, rather than as wide as its value.
    pub full_width: bool,
    /// Nothing to choose now: its value is `INK_SUBTLE`.
    pub disabled: bool,
}

impl Look {
    fn ink(self) -> u32 {
        if self.flagged {
            BLOCKED
        } else if self.disabled {
            INK_SUBTLE
        } else {
            INK_STRONG
        }
    }
}

/// A choice from a list: the current value and a chevron in a
/// `FLOAT_EDGE` ring, `paint::RAISED` under the pointer, named
/// `title: value` (the value alone with no title). As wide as its value
/// (at most `SETTINGS_MENU_MAX_W`), or full width. The caller hangs its
/// menu on it, anchored to it, the standing value checked.
pub fn chooser(
    id: impl Into<ElementId>,
    title: &str,
    selected: SharedString,
    look: Look,
    cx: &App,
) -> Button {
    let ink = look.ink();
    let ring: gpui::Hsla = if look.flagged {
        rgb(BLOCKED).into()
    } else {
        paint::FLOAT_EDGE.into()
    };
    let name = if title.is_empty() {
        selected.to_string()
    } else {
        format!("{title}: {selected}")
    };
    components::form_button(id, cx)
        .accessibility_label(name)
        .h(px(SETTINGS_BUTTON_H))
        .when(look.full_width, |chooser| chooser.w_full().min_w_0())
        .when(!look.full_width, |chooser| {
            chooser.max_w(px(SETTINGS_MENU_MAX_W))
        })
        .px(px(SETTINGS_SEGMENT_PAD_X))
        .rounded(px(R_CONTROL))
        .border_1()
        .border_color(ring)
        .child(
            div()
                .flex()
                .items_center()
                .min_w_0()
                .gap(px(CH))
                .when(look.full_width, |row| row.w_full())
                .child(
                    div()
                        .debug_selector(|| "chooser-value".into())
                        .min_w_0()
                        .truncate()
                        .when(look.full_width, |value| value.flex_1())
                        .child(components::form_label(selected, ink)),
                )
                .child(icon(icons::CHEVRON_DOWN, ICON_CHEVRON, ink).flex_shrink_0()),
        )
}

/// A short choice side by side: its segments (`segment`) in a
/// `FLOAT_EDGE` ring; full width, they share it.
pub fn segments(full_width: bool) -> Div {
    div()
        .flex()
        .items_center()
        .p(px(SPACE_0_5))
        .gap(px(SPACE_0_5))
        .rounded(px(R_CONTROL))
        .border_1()
        .border_color(paint::FLOAT_EDGE)
        .when(full_width, |track| track.w_full())
}

/// One segment: a tab stop, the chosen one lit `FLOAT_SEL`, the others
/// `paint::HOVER` under the pointer. Selected as `{id}-{at}`.
pub fn segment(
    id: (&'static str, usize),
    label: impl Into<SharedString>,
    selected: bool,
    full_width: bool,
    cx: &App,
) -> Button {
    let (rest, hover, ink): (gpui::Hsla, gpui::Hsla, u32) = if selected {
        (paint::FLOAT_SEL.into(), paint::FLOAT_SEL.into(), INK_STRONG)
    } else {
        (
            gpui::rgba(TRANSPARENT).into(),
            paint::HOVER.into(),
            INK_SUBTLE,
        )
    };
    components::faded_button(id, rest, hover, paint::PRESS.into(), rgb(ink).into(), cx)
        .tab_stop(true)
        .selected(selected)
        .toggled(selected)
        .debug_selector(move || format!("{}-{}", id.0, id.1))
        .h(px(SETTINGS_SEGMENT_H))
        .px(px(SETTINGS_SEGMENT_PAD_X))
        .rounded(px(R_CONTROL))
        .when(full_width, |segment| segment.flex_1().min_w_0())
        .child(
            components::text_ui()
                .font_weight(if selected { W_LABEL } else { W_BODY })
                .text_color(rgb(ink))
                .when(full_width, |label| label.truncate())
                .child(label.into()),
        )
}
