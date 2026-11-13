//! The column's chrome around the list (`docs/design/t3-reference/sidebar.md`
//! §9-12): the search row under the head (the inline field, Filter, Add
//! project and New thread), the results that replace the list while the
//! field holds text, the foot (Settings, Pull requests, Usage, the undo
//! notice, the update door and free disk), and a card's hover detail.

use gpui::prelude::*;
use gpui::{
    anchored, deferred, div, point, px, rgb, rgba, AnyElement, Div, FontWeight, SharedString,
    Stateful,
};

use ferrite_ui::icons::{self, icon};
use ferrite_ui::project_badge::project_badge;
use ferrite_ui::theme::*;
use ferrite_ui::{components, motion};

use super::{icon_button, NavTooltip, ThreadRow, INNER_W};

// --------------------------------------------------------- the search row

/// The fixed row under the head (T3's `SidebarThreadHeader`): the field
/// taking the slack, then the 28px buttons, 4px apart, the column's inset
/// at the sides and below.
pub fn search_row(field: impl IntoElement, buttons: Vec<AnyElement>) -> Div {
    div()
        .debug_selector(|| "nav-search-row".into())
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .gap(px(NAV_SEARCH_ROW_GAP))
        .px(px(NAV_COL_PAD))
        .pb(px(NAV_COL_PAD))
        .child(field)
        .children(buttons)
}

/// The inline search field: 32px, radius 6, the 16px glyph in the icon
/// ink and the line, `Search` over it while it is empty, the clear button
/// while it is not. Under the pointer or focused, the hover fill with
/// strong ink; focused, a 1px ring too.
pub fn search_field(
    line: impl IntoElement,
    empty: bool,
    focused: bool,
    clear: Option<AnyElement>,
) -> Stateful<Div> {
    let key = SharedString::from("nav-search");
    let lit = focused || motion::hover_t(&key) > 0.0;
    div()
        .id("nav-search")
        .debug_selector(|| "nav-search".into())
        .relative()
        .flex()
        .flex_row()
        .flex_1()
        .min_w_0()
        .items_center()
        .h(px(NAV_SEARCH_H))
        .gap(px(NAV_SEARCH_GAP))
        .px(px(NAV_SEARCH_PAD_X))
        .rounded(px(R_CONTROL))
        .when(lit, |field| field.bg(rgba(NAV_CARD_HOVER)))
        .when(focused, |field| field.border_1().border_color(rgba(NAV_SEARCH_RING)))
        .on_hover(motion::hover_snap(key))
        .text_size(px(FS_NAV_SEARCH))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(if lit { INK_STRONG } else { INK_SUBTLE }))
        .child(icon(icons::SEARCH, NAV_SEARCH_GLYPH, INK_SUBTLE).text_color(rgba(NAV_ICON_INK)))
        .child(
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .child(line)
                .when(empty, |line| {
                    line.child(
                        div()
                            .debug_selector(|| "nav-search-placeholder".into())
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .text_color(rgb(INK_SUBTLE))
                            .child("Search"),
                    )
                }),
        )
        .children(clear)
}

/// The field's clear button: a 12px close glyph, ghost, muted.
pub fn search_clear() -> Stateful<Div> {
    icon_button(
        "nav-search-clear",
        icons::CLOSE,
        NAV_SEARCH_CLEAR_GLYPH,
        NAV_SEARCH_CLEAR_GLYPH + 2.0 * NAV_SEARCH_ROW_GAP,
        R_CONTROL,
    )
    .nav_tooltip("Clear search")
}

/// A head button: 28px, radius 6, a 16px glyph.
pub fn head_button(key: &'static str, glyph: &'static str) -> Stateful<Div> {
    icon_button(key, glyph, ICON_BUTTON_GLYPH, NAV_HEAD_BUTTON, R_CONTROL)
}

/// The Filter button: the scoped Project's badge, else the filter glyph.
pub fn filter_button(project: Option<&str>) -> Stateful<Div> {
    let button = head_button("nav-filter", icons::LIST_FILTER).nav_tooltip("Filter by project");
    match project {
        None => button,
        Some(name) => {
            let badge = project_badge(name).debug_selector(|| "nav-filter-badge".into());
            button.relative().child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(paint::SIDEBAR)
                    .rounded(px(R_CONTROL))
                    .child(badge),
            )
        }
    }
}

/// The Factory button: the board's glyph, with an attention dot at its
/// corner while a Task needs the operator.
pub fn factory_button(needs_you: bool) -> Stateful<Div> {
    let button = head_button("nav-factory", icons::FACTORY).nav_tooltip("Factory");
    if !needs_you {
        return button;
    }
    button.relative().child(
        components::status_dot(ATTENTION)
            .debug_selector(|| "nav-factory-attention".into())
            .text_size(px(NAV_FACTORY_DOT))
            .line_height(px(NAV_FACTORY_DOT))
            .absolute()
            .top(px(NAV_FACTORY_DOT_INSET))
            .right(px(NAV_FACTORY_DOT_INSET)),
    )
}

// --------------------------------------------------------- the results

/// The results that replace the list while the field holds text: 36px
/// rows, the highlighted one in the active fill.
pub fn results(rows: Vec<AnyElement>) -> Div {
    div()
        .debug_selector(|| "nav-results".into())
        .flex()
        .flex_col()
        .gap(px(NAV_LIST_GAP))
        .px(px(NAV_COL_PAD))
        .children(rows)
}

/// One result (T3's `SidebarSearchResultRow`): the badge, the title, the
/// PR's number, and the age in a fixed 32px column at 12px, muted at 55%.
pub fn result_row(row: &ThreadRow, age: SharedString, lit: bool) -> Stateful<Div> {
    let thread = row.thread.get();
    let key = SharedString::from(format!("nav-result-{thread}"));
    let hovered = motion::hover_t(&key) > 0.0;
    let selector = key.clone();
    div()
        .id(("nav-result", thread as usize))
        .debug_selector(move || selector.to_string())
        .flex()
        .flex_row()
        .items_center()
        .min_h(px(NAV_RESULT_H))
        .gap(px(NAV_RESULT_GAP))
        .px(px(NAV_RESULT_PAD_X))
        .rounded(px(R_CONTROL))
        .when(lit || hovered, |row| {
            row.bg(rgba(if lit { NAV_CARD_ACTIVE } else { NAV_CARD_HOVER }))
        })
        .on_hover(motion::hover_snap(key))
        .text_size(px(FS_NAV_SEARCH))
        .text_color(rgb(INK_STRONG))
        .children(row.project.as_deref().map(project_badge))
        .child(div().flex_1().min_w_0().truncate().child(row.name.clone()))
        .children(row.pr.as_ref().map(|pr| {
            div()
                .flex_shrink_0()
                .text_size(px(FS_META))
                .text_color(rgb(INK_SECONDARY_LABEL))
                .child(format!("#{}", pr.number))
        }))
        .child(components::tabular(
            div()
                .flex_shrink_0()
                .w(px(NAV_RESULT_AGE_W))
                .flex()
                .justify_end()
                .text_size(px(FS_META))
                .text_color(rgb(INK_SUBTLE))
                .opacity(NAV_RESULT_AGE_OPACITY)
                .child(age),
        ))
}

// ------------------------------------------------------------- the foot

/// The foot (T3's `SidebarChrome` foot): its buttons, the undo notice in
/// the free middle, then free disk and the update door at the right.
pub fn foot(buttons: Vec<AnyElement>, notice: Option<AnyElement>, end: Vec<AnyElement>) -> Div {
    div()
        .debug_selector(|| "nav-foot".into())
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .px(px(NAV_COL_PAD))
        .pt(px(NAV_FOOT_PAD_TOP))
        .pb(px(NAV_FOOT_PAD_BOTTOM))
        .children(buttons)
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .justify_center()
                .children(notice),
        )
        .children(end)
}

/// A foot button: 32px, radius 8, a 16px glyph.
pub fn foot_button(key: &'static str, glyph: &'static str) -> Stateful<Div> {
    icon_button(key, glyph, ICON_BUTTON_GLYPH, NAV_FOOT_BUTTON, NAV_FOOT_BUTTON_R)
}

/// The undo notice (T3's `SidebarThreadUndoNotice`): `Parked, Undo ⌘Z` at
/// 11px in the muted ink, at `opacity` of its fade.
pub fn undo_notice(words: SharedString, opacity: f32) -> Div {
    div()
        .debug_selector(|| "nav-undo-notice".into())
        .truncate()
        .text_size(px(FS_NAV_NOTICE))
        .line_height(px(NAV_NOTICE_LH))
        .text_color(rgb(INK_SUBTLE))
        .opacity(opacity)
        .child(words)
}

/// Free disk, once it runs low (`frame::free_disk_words`).
pub fn free_disk(words: String, ink: u32) -> Div {
    components::tabular(
        div()
            .debug_selector(|| "nav-free-disk".into())
            .flex_shrink_0()
            .px(px(NAV_SEARCH_ROW_GAP))
            .text_size(px(FS_META))
            .text_color(rgb(ink))
            .child(words),
    )
}

// ------------------------------------------------------------ the hover card

/// What a card's hover detail says beyond the card: lines of a glyph and
/// words.
pub struct HoverLines {
    pub lines: Vec<(&'static str, SharedString)>,
}

/// A card's hover detail (T3's row tooltip): placed to the right of the
/// card, aligned to its top, 4px away, at most 320px wide, in the glass
/// face (`menu::glass`). The Title, a line per fact, then the pull
/// request under a rule.
pub fn hover_card(row: &ThreadRow, lines: HoverLines) -> AnyElement {
    let body = ferrite_ui::menu::glass()
        .debug_selector(|| "nav-hover-card".into())
        .flex()
        .flex_col()
        .gap(px(NAV_SEARCH_GAP))
        .max_w(px(NAV_HOVER_CARD_MAX_W))
        .px(px(NAV_HOVER_CARD_PAD_X))
        .py(px(NAV_HOVER_CARD_PAD_Y))
        .font_family(FONT_UI)
        .text_size(px(FS_NAV_TOOLTIP))
        .child(
            div()
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(INK_STRONG))
                .child(row.name.clone()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(NAV_HOVER_CARD_LINE_GAP))
                .children(row.project.as_deref().map(|project| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(NAV_HOVER_CARD_LINE_GAP))
                        .text_color(rgba(NAV_HOVER_CARD_INK))
                        .child(project_badge(project))
                        .child(SharedString::from(project.to_string()))
                }))
                .children(lines.lines.into_iter().map(|(glyph, words)| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(NAV_HOVER_CARD_LINE_GAP))
                        .text_color(rgba(NAV_HOVER_CARD_INK))
                        .child(icon(glyph, NAV_HOVER_CARD_ICON, INK_SUBTLE))
                        .child(div().truncate().child(words))
                })),
        )
        .children(row.pr.as_ref().map(|pr| {
            div()
                .debug_selector(|| "nav-hover-card-prs".into())
                .flex()
                .pt(px(NAV_HOVER_CARD_LINE_GAP))
                .border_t_1()
                .border_color(rgba(NAV_HOVER_CARD_RULE))
                .child(super::card::pr_line(pr))
        }));
    // Absolute, so its origin is the card's corner and not inside its pad.
    div()
        .absolute()
        .top_0()
        .left_0()
        .child(
            deferred(
                anchored()
                    .position_mode(gpui::AnchoredPositionMode::Local)
                    .position(point(px(INNER_W + NAV_HOVER_CARD_GAP), px(0.)))
                    .child(body),
            )
            .with_priority(1),
        )
        .into_any_element()
}
