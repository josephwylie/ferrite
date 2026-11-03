//! The Settings sheet: the Cockpit's pages (`ferrite_ui::frame::settings`)
//! laid out, searched and drawn. Values and persistence remain the
//! Cockpit's (`CockpitView::change_settings`). The sheet's rules live in
//! `theme.rs` (`Settings sheet`, WP-E); it is the sheet recipe
//! (`ferrite_ui::sheet`) at `SETTINGS_W` × `SETTINGS_H`.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    deferred, div, px, rgb, AnyElement, App, Div, Entity, KeyDownEvent, MouseButton, SharedString,
    Window,
};

use gpui::component::button::Button;
use gpui::component::input::{Input, InputState};
use gpui::component::menu::{DropdownMenu, PopupMenuItem};
use gpui::component::{Disableable, Selectable, Sizable};

use ferrite_ui::components::{self, MenuItem};
use ferrite_ui::frame::settings::{Control, Group, Page, PageKey, Row, CLI_DEFAULT};
use ferrite_ui::frame::SettingsSheet;
use ferrite_ui::icons::{self, icon};
use ferrite_ui::pointer::Pointer;
use ferrite_ui::sheet::{sheet, sheet_close, sheet_head, veil};
use ferrite_ui::theme::*;

/// The Settings sheet: the sheet recipe at `SETTINGS_W` × `SETTINGS_H`
/// (capped to the window).
fn settings_sheet() -> Div {
    sheet(SETTINGS_W, SETTINGS_H)
}

/// The Settings head: `Settings`, its close control, the same rule.
fn settings_head(close: impl IntoElement) -> Div {
    sheet_head("Settings", close)
}

/// The searchable Settings sheet over its veil, drawn above the Cockpit's
/// overlays. A press on the veil or the close button closes it; ↑/↓ step
/// the sidebar's pages while the sheet itself holds focus (a field or a
/// control keeps its own arrows).
pub fn draw(sheet: SettingsSheet, window: &mut Window, cx: &mut App) -> AnyElement {
    let SettingsSheet {
        pages,
        selected,
        search,
        focus,
        select,
        step,
        close,
    } = sheet;
    let body = body(pages, selected, &search, select, window, cx);
    let card = settings_sheet()
        .id("settings-card")
        .debug_selector(|| "settings-card".into())
        .track_focus(&focus)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            let delta = match event.keystroke.key.as_str() {
                "up" => -1,
                "down" => 1,
                _ => return,
            };
            if event.keystroke.modifiers.modified() || !focus.is_focused(window) {
                return;
            }
            cx.stop_propagation();
            step(delta, window, cx);
        })
        .child(settings_head(
            sheet_close("settings-close", "Close settings", cx).on_click({
                let close = close.clone();
                move |_, window, cx| {
                    cx.stop_propagation();
                    close(window, cx);
                }
            }),
        ))
        .child(body);
    deferred(ferrite_ui::motion::veil_in(
        "settings-veil",
        veil()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                close(window, cx);
            })
            .child(ferrite_ui::motion::dialog_in("settings-card-in", card)),
    ))
    .with_priority(3)
    .into_any_element()
}

/// What the content column shows: the selected page, or, while the search
/// holds a query, every matching row of every page, each group labelled
/// `Page · Group`.
struct Shown {
    title: SharedString,
    groups: Vec<(Option<SharedString>, Group)>,
    /// The pages with a match (all of them without a query).
    hit: Vec<PageKey>,
}

fn shown(pages: &[Page], selected: PageKey, query: &str) -> Shown {
    if query.is_empty() {
        let page = pages.iter().find(|page| page.key == selected);
        return Shown {
            title: selected.title().into(),
            groups: page
                .map(|page| {
                    page.groups
                        .iter()
                        .map(|group| (group.title.map(SharedString::from), group.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            hit: PageKey::ALL.to_vec(),
        };
    }
    let mut groups = Vec::new();
    let mut hit = Vec::new();
    for page in pages {
        for group in &page.groups {
            let rows: Vec<Row> = group
                .rows
                .iter()
                .filter(|row| row.matches(query, page.key.title(), group.title))
                .cloned()
                .collect();
            if rows.is_empty() {
                continue;
            }
            if !hit.contains(&page.key) {
                hit.push(page.key);
            }
            let label = match group.title {
                Some(title) => format!("{} · {title}", page.key.title()),
                None => page.key.title().to_string(),
            };
            groups.push((
                Some(label.into()),
                Group {
                    rows,
                    ..group.clone()
                },
            ));
        }
    }
    Shown {
        title: "Search results".into(),
        groups,
        hit,
    }
}

/// The Settings body under its head: the sidebar (the search line, the
/// pages, `About` at the foot) beside the selected page, or the search's
/// results, split by a `FLOAT_RULE`.
fn body(
    pages: Vec<Page>,
    selected: PageKey,
    search: &Entity<InputState>,
    select: Rc<dyn Fn(PageKey, &mut Window, &mut App)>,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let query = search.read(cx).value().trim().to_string();
    let shown = shown(&pages, selected, &query);
    let searching = !query.is_empty();
    let nav_row = |key: PageKey, cx: &App| {
        let select = select.clone();
        nav_row(
            key,
            !searching && key == selected,
            shown.hit.contains(&key),
            cx,
        )
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            select(key, window, cx);
        })
    };
    let sidebar = div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .w(px(SETTINGS_SIDEBAR_W))
        .min_h_0()
        .border_r_1()
        .border_color(FLOAT_RULE)
        .pb(px(HALF_ROW))
        .child(search_field(search, window, cx))
        .child(
            div()
                .flex()
                .flex_col()
                .pt(px(HALF_ROW))
                .px(px(FLOAT_PAD))
                .children(
                    [
                        PageKey::NewThreads,
                        PageKey::Permissions,
                        PageKey::Behaviour,
                        PageKey::Notifications,
                        PageKey::Server,
                    ]
                    .map(|key| nav_row(key, cx)),
                ),
        )
        .child(div().flex_1())
        .child(div().px(px(FLOAT_PAD)).child(nav_row(PageKey::About, cx)));

    let content = div()
        .id("settings-content")
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(MODAL_PAD))
        .pt(px(HALF_ROW))
        .pb(px(ROW))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .h(px(ROW))
                .px(px(SETTINGS_ROW_PAD_X))
                .font_weight(W_STRONG)
                .text_color(rgb(TEXT_STRONG))
                .child(shown.title.clone()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .pt(px(SETTINGS_TITLE_GAP))
                .gap(px(SETTINGS_GROUP_GAP))
                .when(searching && shown.groups.is_empty(), |list| {
                    list.child(
                        components::text_ui()
                            .px(px(SETTINGS_ROW_PAD_X))
                            .text_color(rgb(TEXT_DIM))
                            .child("No matching settings"),
                    )
                })
                .children(
                    shown
                        .groups
                        .iter()
                        .map(|(label, group)| group_rows(label.clone(), group, window, cx)),
                ),
        );

    // The page scrolls under a fade at its foot, so a row cut by the
    // sheet's edge reads as more below rather than as clipped.
    let ground: gpui::Hsla = FLOAT_GROUND.into();
    let content = div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .child(content)
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(px(ROW))
                .bg(gpui::linear_gradient(
                    180.,
                    gpui::linear_color_stop(gpui::Hsla { a: 0., ..ground }, 0.),
                    gpui::linear_color_stop(ground, 1.),
                )),
        );
    div()
        .flex()
        .flex_1()
        .min_h_0()
        .child(sidebar)
        .child(content)
}

/// The search line atop the sidebar: the `search` icon, then the field,
/// over a `HAIRLINE` — the palette's own input line. The icon stands on
/// the page names' edge (`FLOAT_PAD_X`).
fn search_field(search: &Entity<InputState>, window: &Window, cx: &App) -> Div {
    let focused = gpui::Focusable::focus_handle(search.read(cx), cx).is_focused(window);
    components::focused(
        div()
            .debug_selector(|| "settings-search".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(ROW + 2.0 * SPACE_1))
            .px(px(FLOAT_PAD_X))
            .border_b_1()
            .border_color(paint::HAIRLINE)
            .child(ferrite_ui::icons::icon(
                ferrite_ui::icons::SEARCH,
                FIND_ICON,
                TEXT_DIM,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(search).appearance(false).small().cleanable(true)),
            ),
        focused,
    )
}

/// A sidebar row, a menu row of the sheet's float: inset `FLOAT_PAD` from
/// its edges, radius `R_MENU_ROW`, the page's name. The page shown is
/// `FLOAT_SEL` in `TEXT_STRONG`; the rest `TEXT`, `FLOAT_HOVER` under the
/// pointer — the float's own steps, which read on every platform (the
/// frame's `RAISED` is the float's own grey once opaque). While a search
/// runs, a page with no match reads `TEXT_DIM`.
fn nav_row(key: PageKey, selected: bool, hit: bool, cx: &App) -> Button {
    let (rest, hover): (gpui::Hsla, gpui::Hsla) = if selected {
        (FLOAT_SEL.into(), FLOAT_SEL.into())
    } else {
        (gpui::rgba(TRANSPARENT).into(), FLOAT_HOVER.into())
    };
    let ink = match (selected, hit) {
        (true, _) => TEXT_STRONG,
        (false, true) => TEXT,
        (false, false) => TEXT_DIM,
    };
    components::faded_button(
        key.selector(),
        rest,
        hover,
        FLOAT_SEL.into(),
        rgb(ink).into(),
        cx,
    )
    .selected(selected)
    .debug_selector(move || key.selector().into())
    .accessibility_label(key.title())
    .w_full()
    .h(px(SETTINGS_NAV_ROW_H))
    .px(px(MENU_ROW_PAD_X))
    .rounded(px(R_MENU_ROW))
    .child(
        div()
            .flex()
            .flex_1()
            .items_center()
            .child(components::form_label(key.title(), ink)),
    )
}

/// A group: its label (and a provider's mark) as a `TEXT_DIM` row, then
/// its rows, each over a `FLOAT_RULE` hairline. The title, the label and
/// every row's text stand on one edge, `SETTINGS_ROW_PAD_X` in; only the
/// hairlines reach the column's edges.
fn group_rows(
    label: Option<SharedString>,
    group: &Group,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let rows: Vec<AnyElement> = group
        .rows
        .iter()
        .map(|row| setting_row(row, window, cx).into_any_element())
        .collect();
    div()
        .flex()
        .flex_col()
        .children(label.map(|label| {
            components::text_ui()
                .flex()
                .items_center()
                .gap(px(CH))
                .h(px(ROW))
                .px(px(SETTINGS_ROW_PAD_X))
                .text_color(rgb(TEXT_DIM))
                .children(group.mark.map(|(path, ink)| icon(path, STATUS_LOGO, ink)))
                .child(section_words(&label))
        }))
        .children(rows)
        // What an empty list says, in a sentence, where its rows would be.
        .children(group.note.clone().map(|note| {
            components::text_ui()
                .px(px(SETTINGS_ROW_PAD_X))
                .text_size(px(FS_META))
                .line_height(px(LH_META))
                .text_color(rgb(TEXT_DIM))
                .child(note)
        }))
}

/// `Page · Group`, the `·` in structure ink.
fn section_words(label: &str) -> Div {
    let mut line = div().flex().items_center().gap(px(CH));
    for (at, part) in label.split(" · ").enumerate() {
        if at > 0 {
            line = line.child(div().text_color(rgb(TEXT_FAINT)).child("·"));
        }
        line = line.child(SharedString::from(part.to_string()));
    }
    line
}

/// One setting row: label over hint at the left, the control right and
/// centred; one row in a quarter row of air each side, or two with a hint.
fn setting_row(row: &Row, window: &mut Window, cx: &mut App) -> Div {
    let hinted = !row.hint.is_empty();
    let fact = row.is_fact();
    div()
        .flex()
        .items_start()
        .justify_between()
        .gap(px(SETTINGS_ROW_GAP))
        .px(px(SETTINGS_ROW_PAD_X))
        .py(px(SETTINGS_ROW_PAD_Y))
        // A dependent setting stands under its parent, indented.
        .when(row.nested, |line| {
            line.pl(px(SETTINGS_ROW_PAD_X + SETTINGS_NEST))
        })
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .map(|text| {
                    if fact {
                        text.flex_shrink_0().w(px(SETTINGS_FACT_KEY_W))
                    } else {
                        text.flex_1()
                    }
                })
                .child(
                    components::text_ui()
                        .truncate()
                        .text_size(px(FS_TITLE))
                        .line_height(px(LH_UI))
                        .text_color(rgb(TEXT_STRONG))
                        .child(row.title.clone()),
                )
                .when(hinted, |text| text.child(hint(row.hint.clone()))),
        )
        .child(
            // The control stands on the label's line, at the right.
            div()
                .flex()
                .items_center()
                .justify_end()
                .min_h(px(LH_UI))
                .map(|control| {
                    if fact {
                        control.flex_1().min_w_0()
                    } else {
                        control.flex_shrink_0()
                    }
                })
                .child(control(row, window, cx)),
        )
}

/// A row's description: `TEXT_DIM` at 13, wrapping to a second line where
/// it needs one. One that opens with a key table's chord (`cmd-B toggles it
/// any time`) draws the chord as keys (`⌘B`, `components::key_combo`): the
/// face has no command glyph.
fn hint(detail: SharedString) -> Div {
    let line = components::text_ui().min_w_0().text_color(rgb(TEXT_DIM));
    match detail.split_once(' ') {
        Some((chord, rest)) if chord.starts_with("cmd-") => line
            .flex()
            .items_center()
            .gap(px(CH))
            .child(components::key_combo(chord, TEXT_DIM))
            .child(SharedString::from(rest.to_string())),
        _ => line.child(detail),
    }
}

/// A row's control, drawn from what the Cockpit says it is.
fn control(row: &Row, _window: &mut Window, cx: &mut App) -> AnyElement {
    let title = row.title.clone();
    match row.control.clone() {
        // Two options: a token each, the chosen one inverse.
        // A short choice: a segmented control, the chosen segment lit.
        Control::Tokens { id, options, pick } => div()
            .flex()
            .items_center()
            .p(px(SPACE_0_5))
            .gap(px(SPACE_0_5))
            .rounded(px(R_CONTROL))
            .border_1()
            .border_color(gpui::rgba(BORDER))
            .children(
                options
                    .into_iter()
                    .enumerate()
                    .map(|(at, (label, selected))| {
                        let pick = pick.clone();
                        segment((id, at), label, selected, cx).on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            pick(at, cx);
                        })
                    }),
            )
            .into_any_element(),
        // A choice from a list: the current value and a chevron, as wide as
        // its value (at most `SETTINGS_MENU_MAX_W`), `paint::RAISED` under
        // the pointer. The menu keeps every value, the standing one checked.
        Control::Chooser {
            id,
            selected,
            options,
            pick,
        } => components::form_button(id, cx)
            .debug_selector(move || id.into())
            .accessibility_label(format!("{title}: {selected}"))
            .h(px(SETTINGS_BUTTON_H))
            .max_w(px(SETTINGS_MENU_MAX_W))
            .px(px(SETTINGS_SEGMENT_PAD_X))
            .rounded(px(R_CONTROL))
            .border_1()
            .border_color(gpui::rgba(BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .gap(px(CH))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(components::form_label(selected.clone(), TEXT)),
                    )
                    .child(icon(icons::CHEVRON_DOWN, ICON_CHEVRON, TEXT_DIM).flex_shrink_0()),
            )
            .dropdown_menu_with_anchor(gpui::Anchor::TopRight, move |menu, _, _| {
                options.iter().enumerate().fold(
                    menu.min_w(px(CHOICE_MENU_MIN_W))
                        .max_w(px(CHOICE_MENU_MAX_W))
                        .max_h(px(MENU_MAX_H))
                        .scrollable(true),
                    |menu, (at, (label, selected))| {
                        let pick = pick.clone();
                        // The CLI's own default says whose choice it is.
                        let mut item = MenuItem::new(label.clone()).checked(*selected);
                        if label.as_ref() == CLI_DEFAULT {
                            item = item.detail(CLI_DEFAULT_NOTE);
                        }
                        menu.item(
                            PopupMenuItem::element(move |_, _| {
                                components::kit_row(ferrite_ui::menu::row_face(&item, false, false))
                            })
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                pick(at, cx);
                            }),
                        )
                    },
                )
            })
            .into_any_element(),
        // An on/off setting: the one switch (`components::switch`), a tab
        // stop that flips the setting on a click, Enter or Space. It is its
        // own face, so no hover ground lies behind it.
        Control::Toggle {
            id,
            checked,
            change,
        } => {
            let none: gpui::Hsla = gpui::rgba(TRANSPARENT).into();
            components::faded_button(id, none, none, none, rgb(TEXT).into(), cx)
                .tab_stop(true)
                .selected(checked)
                .debug_selector(move || id.into())
                .accessibility_label(format!("{title}: {}", if checked { "on" } else { "off" }))
                .h(px(SETTINGS_CONTROL_H))
                .px(px(SPACE_1))
                .child(components::switch(
                    SharedString::from(format!("{id}-switch")),
                    checked,
                ))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    change(!checked, cx);
                })
                .into_any_element()
        }
        // A size stepper: `‹ 14px ›` — the ends are tokens (a tab stop
        // each), the value tabular between them in its fixed slot. An end
        // with nowhere to go is `TEXT_FAINT` and takes no press. The keys
        // that do the same ride the hint.
        Control::Stepper {
            id,
            value,
            can_down,
            can_up,
            step,
        } => {
            let button = |at: usize, label: &'static str, delta: i32, live: bool| {
                let step = step.clone();
                let ink = if live { TEXT } else { TEXT_FAINT };
                let none: gpui::Hsla = gpui::rgba(TRANSPARENT).into();
                let (hover, press): (gpui::Hsla, gpui::Hsla) = if live {
                    (paint::HOVER.into(), paint::PRESS.into())
                } else {
                    (none, none)
                };
                components::faded_button((id, at), none, hover, press, rgb(ink).into(), cx)
                    .tab_stop(live)
                    .disabled(!live)
                    .when(!live, |button| button.cursor_default())
                    .debug_selector(move || format!("{id}-{at}"))
                    .h(px(SETTINGS_CONTROL_H))
                    .px(px(TOKEN_PAD_X))
                    .child(components::text_ui().text_color(rgb(ink)).child(label))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        step(delta, cx);
                    })
            };
            div()
                .flex()
                .items_center()
                .child(button(0, "\u{2039}", -1, can_down))
                .child(components::tabular(
                    div()
                        .debug_selector(move || format!("{id}-value"))
                        .min_w(px(SETTINGS_STEPPER_VALUE_W))
                        .flex()
                        .justify_center()
                        .child(components::form_label(value.clone(), TEXT)),
                ))
                .child(button(1, "\u{203a}", 1, can_up))
                .into_any_element()
        }
        // A file in place of a built-in: its name as a fact reads, then a
        // token per act.
        Control::FileChoice { id, value, acts } => div()
            .flex()
            .items_center()
            .min_w_0()
            .gap(px(CH))
            .child(fact_value(&title, value))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .flex_shrink_0()
                    .children(acts.into_iter().enumerate().map(|(at, (label, act))| {
                        quiet_button((id, at), label.into(), cx)
                            .debug_selector(move || format!("{id}-{at}"))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                act(cx);
                            })
                    })),
            )
            .into_any_element(),
        // The operator's own words, and a token per act.
        Control::Named { id, nth, acts } => {
            let count = acts.len();
            div()
                .flex()
                .items_center()
                .gap(px(SPACE_1_5))
                .flex_shrink_0()
                .children(acts.into_iter().enumerate().map(|(at, (label, act))| {
                    quiet_button((id, nth * count + at), label, cx)
                        .debug_selector(move || format!("{id}-{}", nth * count + at))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            act(cx);
                        })
                }))
                .into_any_element()
        }
        // A field, and the token that takes what is typed in it.
        Control::Entry {
            id,
            field,
            label,
            act,
        } => div()
            .flex()
            .items_center()
            .gap(px(CH))
            .child(
                div()
                    .w(px(SETTINGS_FIELD_W))
                    .child(Input::new(&field).small()),
            )
            .child(
                quiet_button((id, 0usize), label.into(), cx)
                    .debug_selector(move || format!("{id}-0"))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        act(window, cx);
                    }),
            )
            .into_any_element(),
        // A read-only fact (About): its value at the right in `TEXT_DIM`
        // (versions are machine text), wrapping anywhere rather than cut.
        Control::Fact { value } => fact_value(&title, value).into_any_element(),
        // The one button that does something about the row: the primary
        // face, one row high, hugging its label. With nothing to do, the
        // value as a fact reads.
        Control::Action {
            id,
            value,
            button,
            act,
        } => match button {
            Some(label) => quiet_button(id, label, cx)
                .debug_selector(move || id.into())
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    act(cx);
                })
                .into_any_element(),
            None => fact_value(&title, value).into_any_element(),
        },
        // A path fact, copied whole on a click. The copy mark sits after it
        // in structure ink, brightening under the pointer, and turns to a
        // check once copied.
        Control::PathFact {
            path,
            shown,
            copied,
            on_copy,
        } => {
            let id = gpui::ElementId::from(SharedString::from(format!("settings-copy-{title}")));
            let key = ferrite_ui::pointer::hover_key(&id);
            div()
                .id(id)
                .flex()
                .items_center()
                .min_w_0()
                .gap(px(CH))
                .px(px(CH))
                .mr(px(-CH))
                .group(COPY_GROUP)
                .hover_raised(key)
                // The check confirms the copy, as `components::copy_button`'s
                // does: gpui hides a tooltip on the press and builds the next
                // once, so a swapped one would show late and go stale.
                .tooltip(ferrite_ui::menu::tooltip("Copy path"))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(path.clone()));
                    on_copy(cx);
                })
                // One line, cut from its start: a path's end names it.
                .child(
                    fact_value(&title, shown)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis_start(),
                )
                .child(
                    icon(
                        if copied { icons::CHECK } else { icons::COPY },
                        ICON_CHEVRON,
                        if copied { RUNNING } else { TEXT_FAINT },
                    )
                    .flex_shrink_0()
                    .group_hover(COPY_GROUP, |style| style.text_color(rgb(TEXT_DIM))),
                )
                .into_any_element()
        }
    }
}

/// What a chooser's `CLI default` row says about itself.
const CLI_DEFAULT_NOTE: &str = "follows the CLI";

/// One segment of a short choice (theme `SETTINGS_SEGMENT_*`): chosen, it
/// is lit — `paint::SELECTED` with `TEXT_STRONG` at `W_LABEL`; the rest
/// `TEXT_DIM` on nothing, `paint::HOVER` under the pointer. A tab stop.
fn segment(id: (&'static str, usize), label: SharedString, selected: bool, cx: &App) -> Button {
    let (rest, hover, ink): (gpui::Hsla, gpui::Hsla, u32) = if selected {
        (paint::SELECTED.into(), paint::SELECTED.into(), TEXT_STRONG)
    } else {
        (
            gpui::rgba(TRANSPARENT).into(),
            paint::HOVER.into(),
            TEXT_DIM,
        )
    };
    components::faded_button(id, rest, hover, paint::PRESS.into(), rgb(ink).into(), cx)
        .tab_stop(true)
        .selected(selected)
        .toggled(selected)
        .debug_selector(move || format!("{}-{}", id.0, id.1))
        .h(px(SETTINGS_SEGMENT_H))
        .px(px(SETTINGS_SEGMENT_PAD_X))
        .rounded(px(R_SMALL))
        .child(
            components::text_ui()
                .font_weight(if selected { W_LABEL } else { W_BODY })
                .text_color(rgb(ink))
                .child(label),
        )
}

/// An action on a row (`Copy`, `Restart`, `Add`, `Remove`): the quiet
/// bordered button — `TEXT` on nothing in a `BORDER` ring, radius
/// `R_CONTROL`, `paint::HOVER` under the pointer. Settings has no filled
/// button; what changes state is a switch or a segment.
fn quiet_button(id: impl Into<gpui::ElementId>, label: SharedString, cx: &App) -> Button {
    components::faded_button(
        id,
        gpui::rgba(TRANSPARENT).into(),
        paint::HOVER.into(),
        paint::PRESS.into(),
        rgb(TEXT).into(),
        cx,
    )
    .tab_stop(true)
    .h(px(SETTINGS_BUTTON_H))
    .px(px(SETTINGS_SEGMENT_PAD_X))
    .rounded(px(R_CONTROL))
    .border_1()
    .border_color(gpui::rgba(BORDER))
    .child(components::form_label(label, TEXT))
}

/// A path fact's hover reaches its copy mark through this group.
const COPY_GROUP: &str = "settings-copy";

fn fact_value(title: &SharedString, value: SharedString) -> Div {
    let title = title.clone();
    components::text_ui()
        .debug_selector(move || format!("settings-fact-{title}"))
        .min_w_0()
        .text_right()
        .font_family(FONT_CODE)
        .font_weight(W_BODY)
        .text_color(rgb(TEXT_DIM))
        .child(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Settings is the same recipe, and every sheet's head stands over a
    /// rule.
    #[test]
    fn the_settings_sheet_is_the_sheet_recipe() {
        let mut drawn = settings_sheet();
        let style = drawn.style();
        assert_eq!(style.background, Some(FLOAT_GROUND.into()));
        assert_eq!(style.border_color, Some(FLOAT_EDGE.into()));
        assert_eq!(style.box_shadow, Some(components::float_shadow()));
        for head in [settings_head(div()), sheet_head("Project", div())] {
            let mut head = head;
            assert_eq!(head.style().border_widths.bottom, Some(px(1.).into()));
        }
    }

    #[test]
    fn rows_sit_on_the_grid_and_the_sheet_fits_its_pages() {
        assert_eq!(SETTINGS_NAV_ROW_H, FLOAT_ROW_H);
        assert_eq!(SETTINGS_CONTROL_H, ROW);
        // Checked at compile time: a control never outgrows a row's air.
        const _: () = assert!(SETTINGS_BUTTON_H <= LH_UI + 2.0 * SETTINGS_ROW_PAD_Y);
        const _: () = assert!(
            SETTINGS_W <= 800.0 && SETTINGS_H <= 600.0,
            "sized to content"
        );
    }

    /// Search spans every page: a row matches on its label, its hint, an
    /// option label, or the group and page it sits in; each result card is
    /// labelled `Page · Group`, and only pages with a hit stay lit.
    #[test]
    fn search_filters_every_page_and_labels_each_result() {
        use ferrite_ui::frame::settings::page;
        let row = |title: &'static str, hint: &'static str, words: &[&'static str]| {
            Row::new(
                title,
                hint,
                words.iter().map(|word| SharedString::from(*word)).collect(),
                Control::Fact {
                    value: SharedString::default(),
                },
            )
        };
        let pages = vec![
            page(
                PageKey::NewThreads,
                vec![
                    Group::new(None).rows([row("Provider", "What a new thread starts on", &[])]),
                    Group::new(Some("Codex")).rows([row("Model", "", &["GPT Future Model"])]),
                ],
            ),
            page(
                PageKey::Behaviour,
                vec![Group::new(Some("Reading")).rows([row("Solo answer size", "", &[])])],
            ),
        ];
        let at_rest = shown(&pages, PageKey::Behaviour, "");
        assert_eq!(at_rest.title.as_ref(), "Behaviour");
        assert_eq!(at_rest.groups.len(), 1);
        assert_eq!(at_rest.hit, PageKey::ALL.to_vec());

        let found = shown(&pages, PageKey::Behaviour, "future model");
        assert_eq!(found.hit, vec![PageKey::NewThreads]);
        assert_eq!(found.groups.len(), 1);
        assert_eq!(
            found.groups[0].0.as_deref(),
            Some("New threads · Codex"),
            "a result names its page and group"
        );
        let by_hint = shown(&pages, PageKey::NewThreads, "STARTS ON");
        assert_eq!(by_hint.groups[0].0.as_deref(), Some("New threads"));
        let by_group = shown(&pages, PageKey::NewThreads, "codex");
        assert_eq!(by_group.groups.len(), 1);
        assert_eq!(by_group.groups[0].1.rows.len(), 1);
        assert!(shown(&pages, PageKey::NewThreads, "nothing like it")
            .groups
            .is_empty());
    }

    /// The chosen segment of a short choice is lit — the selected fill —
    /// and the rest lie on no ground at all.
    #[gpui::test]
    fn a_chosen_segment_is_lit_and_the_rest_lie_flat(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            ferrite_pane::pane::init_components(cx);
            let mut on = segment(("c", 0), "Claude".into(), true, cx);
            let mut off = segment(("c", 1), "Codex".into(), false, cx);
            assert_eq!(on.style().background, Some(paint::SELECTED.into()));
            assert_eq!(off.style().background, None);
        });
    }
}
