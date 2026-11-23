use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    div, px, rgb, AnyElement, App, Div, Entity, KeyDownEvent, MouseButton, SharedString,
    Window,
};

use gpui::component::button::Button;
use gpui::component::input::{Input, InputState};
use gpui::component::menu::{DropdownMenu, PopupMenuItem};
use gpui::component::{Disableable, Selectable, Sizable};

use ferrite_ui::components::{self, MenuItem};
use ferrite_ui::controls;
use ferrite_ui::frame::settings::{Control, GivesWay, Group, Page, PageKey, Row, CLI_DEFAULT};
use ferrite_ui::frame::SettingsSheet;
use ferrite_ui::icons::{self, icon};
use ferrite_ui::pointer::Pointer;
use ferrite_ui::sheet::{sheet, sheet_close, sheet_head, Drawn};
use ferrite_ui::theme::*;

mod connections;

/// The Settings sheet: the sheet recipe at `SETTINGS_W` × `SETTINGS_H`
/// (capped to the window).
fn settings_sheet() -> Div {
    sheet(SETTINGS_W, SETTINGS_H)
}

/// The Settings head: `Settings`, its close control, the same rule.
fn settings_head(close: impl IntoElement) -> Div {
    sheet_head("Settings", close)
}

/// The searchable Settings sheet's card, for the Cockpit's sheet host to
/// lay over its veil, and the connection form above both while one is
/// being edited. The close button closes it; ↑/↓ step the sidebar's pages
/// while the sheet itself holds focus (a field or a control keeps its own
/// arrows).
pub fn draw(sheet: SettingsSheet, window: &mut Window, cx: &mut App) -> Drawn {
    let SettingsSheet {
        pages,
        selected,
        search,
        focus,
        select,
        step,
        close,
    } = sheet;
    let connection_form = pages
        .iter()
        .flat_map(|page| &page.groups)
        .flat_map(|group| &group.rows)
        .find_map(|row| match &row.control {
            Control::ConnectionForm(
                form @ ferrite_ui::frame::settings::ConnectionForm::Editing { .. },
            ) => Some(form.clone()),
            _ => None,
        });
    let body = body(pages, selected, &search, select, window, cx);
    let card = settings_sheet()
        .when(selected == PageKey::Connections, |card| {
            card.w(px(1120.)).h(px(700.))
        })
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
            sheet_close("settings-close", "Close settings", cx).on_click(move |_, window, cx| {
                cx.stop_propagation();
                close(window, cx);
            }),
        ))
        .child(body);
    Drawn {
        card,
        above: connection_form.map(|form| connections::form(form, window, cx)),
    }
}

/// What the content column shows: the selected page, or, while the search
/// holds a query, every matching row of every page, each group labelled
/// `Page · Group`.
struct Shown {
    title: SharedString,
    /// What the page's settings apply to; a search's results say none.
    scope: Option<SharedString>,
    groups: Vec<(Option<SharedString>, Group)>,
    /// The pages with a match (all of them without a query).
    hit: Vec<PageKey>,
}

fn shown(pages: &[Page], selected: PageKey, query: &str) -> Shown {
    if query.is_empty() {
        let page = pages.iter().find(|page| page.key == selected);
        return Shown {
            title: selected.title().into(),
            scope: page.and_then(|page| page.scope.clone()),
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
        scope: None,
        groups,
        hit,
    }
}

/// The Settings body under its head: the sidebar (the search line, the
/// pages, `About` at the foot) beside the selected page, or the search's
/// results, split by a `FLOAT_EDGE` rule.
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
        .border_color(paint::FLOAT_EDGE)
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
                        PageKey::Connections,
                        PageKey::Labs,
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
        .px(px(if selected == PageKey::Connections {
            24.
        } else {
            MODAL_PAD
        }))
        .pt(px(if selected == PageKey::Connections {
            24.
        } else {
            HALF_ROW
        }))
        .pb(px(if selected == PageKey::Connections {
            48.
        } else {
            ROW
        }))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .h(px(if selected == PageKey::Connections && !searching {
                    0.
                } else {
                    ROW
                }))
                .overflow_hidden()
                .px(px(SETTINGS_ROW_PAD_X))
                .font_weight(W_LABEL)
                .text_color(rgb(INK_STRONG))
                .child(shown.title.clone()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .pt(px(if selected == PageKey::Connections && !searching {
                    0.
                } else {
                    SETTINGS_TITLE_GAP
                }))
                .gap(px(if selected == PageKey::Connections {
                    32.
                } else {
                    SETTINGS_GROUP_GAP
                }))
                // Atop the page, as T3 Code's settings say the environment
                // they apply to.
                .children(shown.scope.clone().map(|scope| {
                    components::text_ui()
                        .debug_selector(|| "settings-scope".into())
                        .px(px(SETTINGS_ROW_PAD_X))
                        .text_color(rgb(INK_SUBTLE))
                        .child(scope)
                }))
                .when(searching && shown.groups.is_empty(), |list| {
                    list.child(
                        components::text_ui()
                            .px(px(SETTINGS_ROW_PAD_X))
                            .text_color(rgb(INK_SUBTLE))
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
    let ground: gpui::Hsla = paint::FLOAT.into();
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
/// over a `FLOAT_EDGE` rule — the palette's own input line. The icon stands on
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
            .border_color(paint::FLOAT_EDGE)
            .child(ferrite_ui::icons::icon(
                ferrite_ui::icons::SEARCH,
                FIND_ICON,
                INK_SUBTLE,
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
/// its edges, radius `R_CONTROL`, the page's name. The page shown is
/// `FLOAT_SEL` in `INK_STRONG`; the rest `INK_STRONG`, `FLOAT_HOVER` under the
/// pointer — the float's own steps, which read on every platform (the
/// frame's `RAISED` is the float's own grey once opaque). While a search
/// runs, a page with no match reads `INK_SUBTLE`.
fn nav_row(key: PageKey, selected: bool, hit: bool, cx: &App) -> Button {
    let (rest, hover): (gpui::Hsla, gpui::Hsla) = if selected {
        (paint::FLOAT_SEL.into(), paint::FLOAT_SEL.into())
    } else {
        (gpui::rgba(TRANSPARENT).into(), paint::FLOAT_HOVER.into())
    };
    let ink = if selected || hit {
        INK_STRONG
    } else {
        INK_SUBTLE
    };
    components::faded_button(
        key.selector(),
        rest,
        hover,
        paint::FLOAT_SEL.into(),
        rgb(ink).into(),
        cx,
    )
    .selected(selected)
    .debug_selector(move || key.selector().into())
    .accessibility_label(key.title())
    .w_full()
    .h(px(SETTINGS_NAV_ROW_H))
    .px(px(MENU_ROW_PAD_X))
    .rounded(px(R_CONTROL))
    .child(
        div()
            .flex()
            .flex_1()
            .items_center()
            .child(components::form_label(key.title(), ink)),
    )
}

/// A group: its label (and a provider's mark) as a `INK_SUBTLE` row, then
/// its rows, each over a `FLOAT_EDGE` rule. The title, the label and
/// every row's text stand on one edge, `SETTINGS_ROW_PAD_X` in; only the
/// hairlines reach the column's edges.
fn group_rows(
    label: Option<SharedString>,
    group: &Group,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    if group.rows.iter().any(|row| {
        matches!(
            row.control,
            Control::ConnectionForm(_) | Control::ConnectionRow(_)
        )
    }) {
        return connections::group(label, group, cx);
    }
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
                .text_color(rgb(INK_SUBTLE))
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
                .text_color(rgb(INK_SUBTLE))
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
fn setting_row(row: &Row, window: &mut Window, cx: &mut App) -> AnyElement {
    match &row.control {
        Control::ConnectionForm(form) => return connections::form(form.clone(), window, cx),
        Control::ConnectionRow(connection) => {
            return connections::row(connection.clone(), cx).into_any_element()
        }
        _ => {}
    }
    let hinted = !row.hint.is_empty();
    let gives_way = row.gives_way();
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
                .debug_selector({
                    let title = row.title.clone();
                    move || format!("settings-label-{title}")
                })
                .map(|text| match gives_way {
                    GivesWay::FactValue => text.flex_shrink_0().w(px(SETTINGS_FACT_KEY_W)),
                    GivesWay::ActionValue => text.flex_shrink_0(),
                    GivesWay::Label => text.flex_1(),
                })
                .child(
                    components::text_ui()
                        .truncate()
                        .text_size(px(FS_TITLE))
                        .line_height(px(LH_UI))
                        .text_color(rgb(INK_STRONG))
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
                .map(|control| match gives_way {
                    GivesWay::Label => control.flex_shrink_0(),
                    GivesWay::FactValue | GivesWay::ActionValue => control.flex_1().min_w_0(),
                })
                .child(control(row, window, cx)),
        )
        .into_any_element()
}

/// A row's description: `INK_SUBTLE` at 13, wrapping to a second line where
/// it needs one. One that opens with a key table's chord (`cmd-B toggles it
/// any time`) draws the chord as keys (`⌘B`, `components::key_combo`): the
/// face has no command glyph.
fn hint(detail: SharedString) -> Div {
    let line = components::text_ui().min_w_0().text_color(rgb(INK_SUBTLE));
    match detail.split_once(' ') {
        Some((chord, rest)) if chord.starts_with("cmd-") => line
            .flex()
            .items_center()
            .gap(px(CH))
            .child(components::key_combo(chord, INK_SUBTLE))
            .child(SharedString::from(rest.to_string())),
        _ => line.child(detail),
    }
}

/// A row's control, drawn from what the Cockpit says it is.
fn control(row: &Row, window: &mut Window, cx: &mut App) -> AnyElement {
    let title = row.title.clone();
    match row.control.clone() {
        Control::ConnectionForm(form) => connections::form(form, window, cx).into_any_element(),
        Control::ConnectionRow(connection) => connections::row(connection, cx).into_any_element(),
        // Two options: a token each, the chosen one inverse.
        // A short choice: a segmented control, the chosen segment lit.
        Control::Tokens { id, options, pick } => controls::segments(false)
            .children(
                options
                    .into_iter()
                    .enumerate()
                    .map(|(at, (label, selected))| {
                        let pick = pick.clone();
                        controls::segment((id, at), label, selected, false, cx).on_click(
                            move |_, _, cx| {
                                cx.stop_propagation();
                                pick(at, cx);
                            },
                        )
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
        } => controls::chooser(id, &title, selected.clone(), Default::default(), cx)
            .debug_selector(move || id.into())
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
                                components::kit_row(
                                    ferrite_ui::menu::row_face(&item, false, false)
                                        .debug_selector(move || format!("{id}-option-{at}")),
                                )
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
            components::faded_button(id, none, none, none, rgb(INK_STRONG).into(), cx)
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
                let ink = if live { INK_STRONG } else { TEXT_FAINT };
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
                        .child(components::form_label(value.clone(), INK_STRONG)),
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
        // A read-only fact (About): its value at the right in `INK_SUBTLE`
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
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    act(window, cx);
                })
                .into_any_element(),
            None => fact_value(&title, value).truncate().into_any_element(),
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
                    .group_hover(COPY_GROUP, |style| style.text_color(rgb(INK_SUBTLE))),
                )
                .into_any_element()
        }
    }
}

/// What a chooser's `CLI default` row says about itself.
const CLI_DEFAULT_NOTE: &str = "follows the CLI";

/// An action on a row (`Copy`, `Restart`, `Add`, `Remove`): the quiet
/// bordered button — `INK_STRONG` on nothing in a `FLOAT_EDGE` ring, radius
/// `R_CONTROL`, `paint::HOVER` under the pointer. Settings has no filled
/// button; what changes state is a switch or a segment.
fn quiet_button(id: impl Into<gpui::ElementId>, label: SharedString, cx: &App) -> Button {
    components::faded_button(
        id,
        gpui::rgba(TRANSPARENT).into(),
        paint::HOVER.into(),
        paint::PRESS.into(),
        rgb(INK_STRONG).into(),
        cx,
    )
    .tab_stop(true)
    .h(px(SETTINGS_BUTTON_H))
    .px(px(SETTINGS_SEGMENT_PAD_X))
    .rounded(px(R_CONTROL))
    .border_1()
    .border_color(paint::FLOAT_EDGE)
    .child(components::form_label(label, INK_STRONG))
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
        .text_color(rgb(INK_SUBTLE))
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
        assert_eq!(style.background, Some(paint::FLOAT.into()));
        assert_eq!(style.border_color, Some(paint::FLOAT_EDGE.into()));
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

    #[test]
    fn ssh_search_finds_the_connections_page() {
        use ferrite_ui::frame::settings::page;
        let pages = vec![page(
            PageKey::Connections,
            vec![Group::new(Some("Remote environments")).rows([Row::new(
                "Add environment",
                "SSH host alias username port",
                vec![],
                Control::Fact { value: "".into() },
            )])],
        )];
        let found = shown(&pages, PageKey::About, "SSH");
        assert_eq!(found.hit, vec![PageKey::Connections]);
        assert_eq!(
            found.groups[0].0.as_deref(),
            Some("Connections · Remote environments")
        );
    }

    #[gpui::test]
    fn a_busy_connection_form_disables_submit_but_keeps_cancel_live(cx: &mut gpui::TestAppContext) {
        use ferrite_ui::frame::settings::{ConnectionFields, ConnectionForm};
        use std::cell::Cell;
        struct Host {
            fields: ConnectionFields,
            submitted: Rc<Cell<usize>>,
            cancelled: Rc<Cell<usize>>,
        }
        impl Render for Host {
            fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let submitted = self.submitted.clone();
                let cancelled = self.cancelled.clone();
                connections::form(
                    ConnectionForm::Editing {
                        fields: self.fields.clone(),
                        signing_in: None,
                        mode: ferrite_ui::frame::settings::ConnectionMode::Ssh,
                        focus: cx.focus_handle(),
                        suggestions: None,
                        choose_mode: Rc::new(|_, _, _| {}),
                        open_suggestions: Rc::new(|_, _| {}),
                        choose_host: Rc::new(|_, _, _| {}),
                        progress: Some("Connecting…".into()),
                        error: None,
                        warning: None,
                        verification: None,
                        submit: Rc::new(move |_, _| submitted.set(submitted.get() + 1)),
                        cancel: Rc::new(move |_, _| cancelled.set(cancelled.get() + 1)),
                    },
                    window,
                    cx,
                )
            }
        }
        cx.update(ferrite_pane::pane::init_components);
        let submitted = Rc::new(Cell::new(0));
        let cancelled = Rc::new(Cell::new(0));
        let (_, cx) = cx.add_window_view({
            let submitted = submitted.clone();
            let cancelled = cancelled.clone();
            |window, cx| {
                let host = cx.new(|cx| Host {
                    fields: ConnectionFields {
                        remote_host: cx.new(|cx| InputState::new(window, cx)),
                        pairing_code: cx.new(|cx| InputState::new(window, cx)),
                        host: cx.new(|cx| InputState::new(window, cx)),
                        username: cx.new(|cx| InputState::new(window, cx)),
                        port: cx.new(|cx| InputState::new(window, cx)),
                    },
                    submitted,
                    cancelled,
                });
                gpui::component::Root::new(host, window, cx)
            }
        });
        cx.run_until_parked();
        let hit = cx.debug_bounds("connection-submit").unwrap().center();
        cx.simulate_click(hit, gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(submitted.get(), 0);
        let hit = cx.debug_bounds("connection-close").unwrap().center();
        cx.simulate_click(hit, gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(cancelled.get(), 1);
    }

    #[gpui::test]
    fn compact_connection_form_keeps_fields_errors_and_submit_visible(
        cx: &mut gpui::TestAppContext,
    ) {
        use ferrite_ui::frame::settings::{ConnectionFields, ConnectionForm, ConnectionMode};
        use std::cell::Cell;
        struct Host {
            fields: ConnectionFields,
            mode: Rc<Cell<ConnectionMode>>,
            failed: Rc<Cell<bool>>,
        }
        impl Render for Host {
            fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                connections::form(
                    ConnectionForm::Editing {
                        fields: self.fields.clone(),
                        signing_in: None,
                        mode: self.mode.get(),
                        focus: cx.focus_handle(),
                        suggestions: None,
                        choose_mode: Rc::new(|_, _, _| {}),
                        open_suggestions: Rc::new(|_, _| {}),
                        choose_host: Rc::new(|_, _, _| {}),
                        progress: None,
                        error: self.failed.get().then(|| {
                            "The server could not be reached. Check the host and try again.".into()
                        }),
                        warning: None,
                        verification: None,
                        submit: Rc::new(|_, _| {}),
                        cancel: Rc::new(|_, _| {}),
                    },
                    window,
                    cx,
                )
            }
        }
        cx.update(ferrite_pane::pane::init_components);
        let mode = Rc::new(Cell::new(ConnectionMode::Ssh));
        let failed = Rc::new(Cell::new(false));
        let (root, cx) = cx.add_window_view({
            let mode = mode.clone();
            let failed = failed.clone();
            |window, cx| {
                let host = cx.new(|cx| Host {
                    fields: ConnectionFields {
                        remote_host: cx.new(|cx| InputState::new(window, cx)),
                        pairing_code: cx.new(|cx| InputState::new(window, cx)),
                        host: cx.new(|cx| InputState::new(window, cx)),
                        username: cx.new(|cx| InputState::new(window, cx)),
                        port: cx.new(|cx| InputState::new(window, cx)),
                    },
                    mode,
                    failed,
                });
                gpui::component::Root::new(host, window, cx)
            }
        });
        let host = root.read_with(cx, |root, _| {
            root.view().clone().downcast::<Host>().unwrap()
        });
        cx.simulate_resize(gpui::size(px(1470.), px(923.)));
        cx.run_until_parked();
        assert_eq!(
            cx.debug_bounds("connection-dialog").unwrap().size.width,
            px(768.)
        );
        assert_eq!(
            cx.debug_bounds("connection-mode-ssh").unwrap().size.height,
            px(96.)
        );
        cx.simulate_resize(gpui::size(px(640.), px(420.)));
        for selected in [ConnectionMode::Ssh, ConnectionMode::Remote] {
            mode.set(selected);
            for has_error in [false, true] {
                failed.set(has_error);
                host.update(cx, |_, cx| cx.notify());
                cx.run_until_parked();
                let dialog = cx.debug_bounds("connection-dialog").unwrap();
                let submit = cx.debug_bounds("connection-submit").unwrap();
                assert!(dialog.top() >= px(0.) && dialog.bottom() <= px(420.));
                assert!(submit.top() >= dialog.top() && submit.bottom() < dialog.bottom());
                assert_eq!(
                    cx.debug_bounds("connection-mode-ssh").unwrap().size.height,
                    px(44.)
                );
                let fields = match selected {
                    ConnectionMode::Ssh => {
                        &["connection-host", "connection-username", "connection-port"][..]
                    }
                    ConnectionMode::Remote => {
                        &["connection-remote-host", "connection-pairing-code"][..]
                    }
                };
                for selector in fields {
                    let bounds = cx.debug_bounds(selector).unwrap();
                    assert!(
                        bounds.top() >= dialog.top() && bounds.bottom() < submit.top(),
                        "{selector}: {bounds:?}"
                    );
                }
                let close = cx.debug_bounds("connection-close").unwrap();
                assert!(close.top() >= dialog.top() && close.bottom() < submit.top());
                if has_error {
                    let error = cx.debug_bounds("connection-error").unwrap();
                    assert!(error.top() >= dialog.top() && error.bottom() < submit.top());
                }
            }
        }
    }

    #[gpui::test]
    fn host_verification_keeps_the_fingerprint_bounded_and_continues_on_enter(
        cx: &mut gpui::TestAppContext,
    ) {
        use ferrite_ui::frame::settings::{
            ConnectionFields, ConnectionForm, ConnectionVerification,
        };
        use std::cell::Cell;
        struct Host {
            fields: ConnectionFields,
            focus: gpui::FocusHandle,
            submitted: Rc<Cell<usize>>,
        }
        impl Render for Host {
            fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let submitted = self.submitted.clone();
                connections::form(
                    ConnectionForm::Editing {
                        fields: self.fields.clone(),
                        signing_in: None,
                        mode: ferrite_ui::frame::settings::ConnectionMode::Ssh,
                        focus: cx.focus_handle(),
                        suggestions: None,
                        choose_mode: Rc::new(|_, _, _| {}),
                        open_suggestions: Rc::new(|_, _| {}),
                        choose_host: Rc::new(|_, _, _| {}),
                        progress: Some("Connecting…".into()),
                        error: None,
                        warning: None,
                        verification: Some(ConnectionVerification {
                            target: "developer@example.com".into(),
                            prompt: format!(
                                "{}\nSHA256:exact-fingerprint",
                                "Host information\n".repeat(200)
                            )
                            .into(),
                            response: None,
                            focus: Some(self.focus.clone()),
                            expires: "Expires in 120 seconds".into(),
                            submit: Rc::new(move |_, _| submitted.set(submitted.get() + 1)),
                        }),
                        submit: Rc::new(|_, _| {}),
                        cancel: Rc::new(|_, _| {}),
                    },
                    window,
                    cx,
                )
            }
        }
        cx.update(ferrite_pane::pane::init_components);
        let submitted = Rc::new(Cell::new(0));
        let focus = cx.update(|cx| cx.focus_handle());
        let (_, cx) = cx.add_window_view({
            let submitted = submitted.clone();
            let focus = focus.clone();
            |window, cx| {
                let host = cx.new(|cx| Host {
                    fields: ConnectionFields {
                        remote_host: cx.new(|cx| InputState::new(window, cx)),
                        pairing_code: cx.new(|cx| InputState::new(window, cx)),
                        host: cx.new(|cx| InputState::new(window, cx)),
                        username: cx.new(|cx| InputState::new(window, cx)),
                        port: cx.new(|cx| InputState::new(window, cx)),
                    },
                    focus,
                    submitted,
                });
                gpui::component::Root::new(host, window, cx)
            }
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.focus(&focus, cx));
        assert!(cx.debug_bounds("connection-prompt").unwrap().size.height <= px(96.));
        assert!(cx.debug_bounds("connection-verify-submit").is_some());
        assert!(cx.debug_bounds("connection-response").is_none());
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(submitted.get(), 1);
    }

    /// The chosen segment of a short choice is lit — the selected fill —
    /// and the rest lie on no ground at all.
    #[gpui::test]
    fn a_chosen_segment_is_lit_and_the_rest_lie_flat(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            ferrite_pane::pane::init_components(cx);
            let mut on = controls::segment(("c", 0), "Claude", true, false, cx);
            let mut off = controls::segment(("c", 1), "Codex", false, false, cx);
            assert_eq!(on.style().background, Some(paint::FLOAT_SEL.into()));
            assert_eq!(off.style().background, None);
        });
    }

    /// A CLI with nothing to update reads as its version and path. A path
    /// too long for the row is cut, and the label and its hint keep the
    /// width they have beside a short one.
    #[gpui::test]
    fn a_long_value_is_cut_and_its_label_keeps_its_width(cx: &mut gpui::TestAppContext) {
        const ROW_W: f32 = 480.;
        struct Host;
        impl Render for Host {
            fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let cli = |title: &'static str, id: &'static str, value: &'static str| {
                    Row::new(
                        title,
                        "Not checked for updates yet",
                        Vec::new(),
                        Control::Action {
                            id,
                            value: value.into(),
                            button: None,
                            act: Rc::new(|_, _| {}),
                        },
                    )
                };
                let short = cli("Short", "short", "0.1.0");
                let long = cli(
                    "Long",
                    "long",
                    "0.169.1 \u{b7} /home/someone/.nvm/versions/node/v25.9.0/lib/node_modules/bin/codex",
                );
                div()
                    .w(px(ROW_W))
                    .child(setting_row(&short, window, cx))
                    .child(setting_row(&long, window, cx))
            }
        }
        cx.update(ferrite_pane::pane::init_components);
        let (_, cx) = cx.add_window_view(|_, _| Host);
        cx.run_until_parked();
        let bounds = |cx: &mut gpui::VisualTestContext, selector: &'static str| {
            cx.debug_bounds(selector).unwrap_or_else(|| panic!("{selector} is drawn"))
        };
        let short_label = bounds(cx, "settings-label-Short");
        let long_label = bounds(cx, "settings-label-Long");
        let value = bounds(cx, "settings-fact-Long");
        assert_eq!(long_label.size, short_label.size, "the label and its hint are whole");
        assert!(value.left() >= long_label.right(), "the value starts after the label");
        assert!(value.right() <= px(ROW_W), "the value ends inside the row");
    }

    /// A choice from a list shows its current value; its menu lists every
    /// option with the current one checked, and picking one makes it the
    /// value shown and the one checked.
    #[gpui::test]
    fn a_chooser_shows_its_value_and_its_menu_picks_another(cx: &mut gpui::TestAppContext) {
        use std::cell::Cell;
        const OPTIONS: [&str; 2] = ["Opus", "Sonnet"];
        struct Host {
            current: Rc<Cell<usize>>,
        }
        impl Render for Host {
            fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let now = self.current.get();
                let current = self.current.clone();
                let row = Row::new(
                    "Model",
                    "",
                    Vec::new(),
                    Control::Chooser {
                        id: "model",
                        selected: OPTIONS[now].into(),
                        options: OPTIONS
                            .iter()
                            .enumerate()
                            .map(|(at, label)| (SharedString::from(*label), at == now))
                            .collect(),
                        pick: Rc::new(move |at, _| current.set(at)),
                    },
                );
                div().size_full().child(control(&row, window, cx))
            }
        }
        cx.update(ferrite_pane::pane::init_components);
        let current = Rc::new(Cell::new(1));
        let (host, cx) = cx.add_window_view({
            let current = current.clone();
            |_, _| Host { current }
        });
        cx.run_until_parked();
        let sonnet_shown = cx.debug_bounds("chooser-value").expect("its value");
        let chooser = cx.debug_bounds("model").expect("the chooser");
        assert!(chooser.contains(&sonnet_shown.center()));

        let checked = |cx: &mut gpui::VisualTestContext| {
            let rows = [
                cx.debug_bounds("model-option-0")
                    .expect("the menu lists Opus"),
                cx.debug_bounds("model-option-1").expect("and Sonnet"),
            ];
            let check = cx.debug_bounds("float-check").expect("one is checked");
            let lit: Vec<usize> = (0..2)
                .filter(|at| rows[*at].contains(&check.center()))
                .collect();
            (rows, lit)
        };
        cx.simulate_click(chooser.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        let (rows, lit) = checked(cx);
        assert_eq!(lit, [1], "Sonnet, the current one, is checked");

        cx.simulate_click(rows[0].center(), gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(current.get(), 0, "picking Opus picks it");
        host.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        let opus_shown = cx.debug_bounds("chooser-value").expect("its value");
        assert!(
            opus_shown.size.width < sonnet_shown.size.width,
            "the shorter Opus is shown: {opus_shown:?} after {sonnet_shown:?}"
        );
        let chooser = cx.debug_bounds("model").expect("the chooser");
        cx.simulate_click(chooser.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(checked(cx).1, [0], "now Opus is checked");
    }
}
