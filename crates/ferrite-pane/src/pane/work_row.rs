//! T3 Code's work row (`WorkLogLine`), the one anatomy every call, thought,
//! notice and group header in the transcript is drawn on: an icon gutter,
//! a one-line label, the time under the pointer, and a chevron slot at the
//! right edge (the theme's work-row section).

use super::*;
use ferrite_core::transcript::tools::ToolIcon;

/// The group a work row hovers as: its time shows while the pointer is on
/// it, nowhere else.
pub(crate) const WORK_ROW: &str = "work-row";

/// A work row's gutter glyph for `icon`.
pub(crate) fn glyph(icon: ToolIcon) -> &'static str {
    match icon {
        ToolIcon::Terminal => icons::TERMINAL,
        ToolIcon::SquarePen => icons::SQUARE_PEN,
        ToolIcon::Eye => icons::EYE,
        ToolIcon::Search => icons::SEARCH,
        ToolIcon::Globe => icons::GLOBE,
        ToolIcon::Brain => icons::BRAIN,
        ToolIcon::Hammer => icons::HAMMER,
        ToolIcon::Wrench => icons::WRENCH,
        ToolIcon::Bot => icons::BOT,
        ToolIcon::Zap => icons::ZAP,
        ToolIcon::MessageCircle => icons::MESSAGE_CIRCLE,
        ToolIcon::ListTodo => icons::LIST_TODO,
        ToolIcon::PullRequest => icons::GIT_PULL_REQUEST,
        ToolIcon::App => icons::FERRITE_MONO,
        ToolIcon::Browser => icons::APP_WINDOW,
        ToolIcon::Alert => icons::CIRCLE_ALERT,
        ToolIcon::Check => icons::CHECK,
    }
}

/// The icon a group's header wears (T3's `toolGroupSummaryIconName`): its
/// members' one kind, a brain for thoughts alone, else the hammer.
pub(crate) fn group_icon(members: &[Block]) -> ToolIcon {
    let tools: Vec<_> = text::group_tools(members).collect();
    let Some(first) = tools.first() else {
        return ToolIcon::Brain;
    };
    let icon = ferrite_core::transcript::tools::policy(&first.name).icon;
    if tools
        .iter()
        .all(|tool| ferrite_core::transcript::tools::policy(&tool.name).icon == icon)
    {
        icon
    } else {
        ToolIcon::Hammer
    }
}

/// What stands at a work row's right edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// A group header's: nothing; its whole row is the toggle.
    None,
    /// An entry that cannot open: the slot is kept, empty, so labels and
    /// times align down a list.
    Empty,
    /// An entry that opens: its chevron, turned a quarter when `open`.
    Chevron { open: bool },
}

/// One work row's face.
pub(crate) struct WorkRow {
    /// The row's element id and selector.
    pub key: SharedString,
    pub icon: ToolIcon,
    /// A failed call: its icon in `TOOL_ERROR_ICON`.
    pub failed: bool,
    /// A failure the row reports (an error notice): the alert in
    /// `BLOCKED`, its words medium, its time always shown.
    pub error: bool,
    pub label: AnyElement,
    /// Its words' ink: `INK_SUBTLE`, an update's `INK_STRONG` at
    /// `WORK_UPDATE_ALPHA`.
    pub ink: gpui::Hsla,
    /// When the work began (`Block::at`); `None` shows no time.
    pub at: Option<String>,
    pub slot: Slot,
    /// It stands in an open group's list: no padding of its own.
    pub listed: bool,
    /// The pointer's toggle over the whole row, laid under the label so a
    /// link in it (a path) is pressed itself.
    pub press: Option<AnyElement>,
}

/// `INK_SUBTLE`, the words of a call, a thought and a group's header.
pub(crate) fn tool_ink() -> gpui::Hsla {
    rgb(INK_SUBTLE).into()
}

/// An update's words: `INK_STRONG` at `WORK_UPDATE_ALPHA`.
pub(crate) fn update_ink() -> gpui::Hsla {
    gpui::Hsla::from(rgb(INK_STRONG)).opacity(theme::WORK_UPDATE_ALPHA)
}

/// A work row (T3's `WorkLogLine`): at least `WORK_ROW_H` tall, its label
/// one line cut with `…`, nothing selectable but what the label registers.
/// The caller adds its press target and its body under it.
pub(crate) fn work_row(row: WorkRow) -> Stateful<Div> {
    let WorkRow {
        key,
        icon,
        failed,
        error,
        label,
        ink,
        at,
        slot,
        listed,
        press,
    } = row;
    let icon_ink = if error {
        rgb(BLOCKED).into()
    } else if failed {
        gpui::Hsla::from(rgb(theme::TOOL_ERROR_ICON)).opacity(theme::TOOL_ERROR_ALPHA)
    } else {
        gpui::Hsla::from(rgb(INK_SUBTLE)).opacity(theme::WORK_ICON_ALPHA)
    };
    let icon = if error { ToolIcon::Alert } else { icon };
    let gutter = div()
        .debug_selector(|| "work-row-icon".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(theme::WORK_GUTTER))
        .child(
            icons::icon_stroked(glyph(icon), theme::WORK_ICON, theme::WORK_ICON_STROKE, INK_SUBTLE)
                .text_color(icon_ink),
        );
    let selector = key.clone();
    div()
        .id(key.clone())
        .debug_selector(move || selector.to_string())
        .group(WORK_ROW)
        .relative()
        .flex()
        .items_center()
        .gap(px(theme::WORK_GAP))
        .w_full()
        .min_w_0()
        .min_h(px(theme::WORK_ROW_H))
        .px(px(theme::WORK_ROW_PAD_X))
        .when(!listed, |row| row.py(px(theme::WORK_ROW_PAD_Y)))
        .rounded(px(theme::WORK_ROW_RADIUS))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::WORK_ROW_TEXT))
        .line_height(px(theme::WORK_ROW_LINE))
        .text_color(ink)
        .when(error, |row| row.font_weight(theme::W_LABEL))
        .children(press)
        .child(gutter)
        .child(
            div()
                .debug_selector(|| "work-row-label".into())
                .flex()
                .items_center()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(label),
        )
        .children(at.as_deref().and_then(|at| hover_time(&key, at, error)))
        .children(chevron_slot(&key, slot))
}

/// A work row's time (T3's `TimelineRowTimestamp`): `WORK_TIME` tabular in
/// `INK_SUBTLE`, `clock::day_aware_label`, its tooltip the full day. At
/// rest it is laid out of the flow, unseen, so it takes no room; under the
/// pointer it snaps into the flow, where it takes its room. `always`: it
/// stands in the flow, seen (an error's).
pub(crate) fn hover_time(key: &SharedString, at: &str, always: bool) -> Option<AnyElement> {
    let at = ferrite_core::clock::parse_local(at)?;
    let label = ferrite_core::clock::day_aware_label(at, ferrite_core::clock::now());
    let tooltip = ferrite_core::clock::day_aware_tooltip(at);
    let time = components::tabular(
        div()
            .id(SharedString::from(format!("{key}-time")))
            .debug_selector(|| "work-row-time".into())
            .flex_shrink_0()
            .whitespace_nowrap()
            .mr(px(theme::WORK_TIME_END))
            .text_size(px(theme::WORK_TIME))
            .text_color(rgb(INK_SUBTLE))
            .tooltip(ferrite_ui::menu::tooltip(tooltip))
            .child(SharedString::from(label)),
    );
    Some(
        if always {
            time
        } else {
            time.absolute()
                .right_0()
                .invisible()
                .group_hover(WORK_ROW, |style| style.relative().visible())
        }
        .into_any_element(),
    )
}

/// A work row's chevron slot (`WORK_CHEVRON_SLOT`): its `WORK_CHEVRON` at
/// `WORK_ICON_ALPHA`, turned a quarter over `MOTION_WORK_CHEVRON_MS` when
/// open; kept, empty, on a row that cannot open; none on a group header.
fn chevron_slot(key: &SharedString, slot: Slot) -> Option<AnyElement> {
    let open = match slot {
        Slot::None => return None,
        Slot::Empty => None,
        Slot::Chevron { open } => Some(open),
    };
    let ink = INK_SUBTLE;
    Some(
        div()
            .debug_selector(|| "work-row-chevron".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(theme::WORK_CHEVRON_SLOT))
            .opacity(theme::WORK_ICON_ALPHA)
            .children(open.map(move |open| {
                ferrite_ui::motion::settled(
                    SharedString::from(format!("{key}-chevron")),
                    open,
                    ferrite_ui::motion::WORK_CHEVRON,
                    move |turn| components::chevron_at(turn, theme::WORK_CHEVRON, ink),
                )
            }))
            .into_any_element(),
    )
}
