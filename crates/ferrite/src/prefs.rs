//! Ferrite's sheets, and the Settings sheet built on them. Values and
//! persistence remain owned by the cockpit (`CockpitView::change_settings`);
//! this module only lays settings out, searches them and draws their
//! controls. The Settings sheet's rules live in `theme.rs` (`Settings
//! sheet`, WP-E).
//!
//! **The sheet recipe** (Settings, the Project editor, the image preview):
//! a `VEIL` over the Cockpit; the sheet is the float at sheet size —
//! `paint::FLOAT`, a 1px `paint::LINE2` edge, square, the float shadow; a
//! `MODAL_HEAD_H` head (the title in `TEXT_STRONG` at `W_STRONG` and the one
//! close control, `×` with the tooltip `Close esc`) over a `paint::LINE`; a
//! scrolling body; a footer pinned under a `paint::LINE`. Inside: one face
//! on the grid, rows and rules, words for controls.

use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, App, Div, Entity, SharedString, Window};

use gpui::component::button::Button;
use gpui::component::input::{Input, InputState};
use gpui::component::menu::{DropdownMenu, PopupMenuItem};
use gpui::component::{Disableable, Selectable, Sizable};

use crate::pointer::Pointer;
use std::rc::Rc;

use crate::components::{self, MenuItem};
use crate::icons::{self, icon};
use crate::theme::*;

// ------------------------------------------------------------ the sheet

/// The dim veil over the Cockpit while a sheet is up: a press on it
/// closes the sheet (the cockpit wires that). It covers selectable
/// transcript text, so it owns the neutral cursor outside the sheet too.
pub fn veil() -> Div {
    components::veil().cursor_default()
}

/// A modal sheet, `width` × `height` at most `MODAL_VIEWPORT_FRACTION` of
/// the window: the float at sheet size. Its definite height is what lets
/// the body scroll.
pub fn sheet(width: f32, height: f32) -> Div {
    sheet_frame(width)
        .h(px(height))
        .max_h(gpui::relative(MODAL_VIEWPORT_FRACTION))
}

/// `sheet` sized to its content, up to `max_height`: a short form draws a
/// short sheet; past the cap its body scrolls.
pub fn sheet_fit(width: f32, max_height: f32) -> Div {
    sheet_frame(width).max_h(px(max_height))
}

fn sheet_frame(width: f32) -> Div {
    components::text_ui()
        .flex()
        .flex_col()
        .w(px(width))
        .max_w(gpui::relative(MODAL_VIEWPORT_FRACTION))
        .overflow_hidden()
        .rounded(px(R_PANE))
        .bg(paint::FLOAT)
        .border_1()
        .border_color(paint::LINE2)
        .shadow(components::elevation(components::Elevation::Sheet))
}

/// The Settings sheet: the sheet recipe at `SETTINGS_W` × `SETTINGS_H`
/// (capped to the window).
pub fn settings_sheet() -> Div {
    sheet(SETTINGS_W, SETTINGS_H)
}

/// A sheet's head: its title, then the close button, over a `paint::LINE`.
/// The close button's tooltip names `esc`; no keycap repeats it.
pub fn sheet_head(title: impl Into<SharedString>, close: impl IntoElement) -> Div {
    head_row(title, close)
        .border_b_1()
        .border_color(paint::LINE)
}

/// The Settings head: `settings`, its close control, the same rule.
pub fn settings_head(close: impl IntoElement) -> Div {
    sheet_head("settings", close)
}

fn head_row(title: impl Into<SharedString>, close: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(MODAL_GAP))
        .h(px(MODAL_HEAD_H))
        .pl(px(MODAL_PAD))
        .pr(px(SPACE_1))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .font_weight(W_STRONG)
                .text_color(rgb(TEXT_STRONG))
                .child(title.into()),
        )
        // The kit button's own tooltip takes only a string; the chord
        // suffix rides a wrapper, as on every keyed control.
        .child(
            div()
                .id("sheet-close-tip")
                .flex_shrink_0()
                .tooltip(crate::menu::action_tooltip("Close", "cockpit::Interrupt"))
                .child(close),
        )
}

/// A sheet's close button, the one close control on every sheet: the close
/// glyph in a 28px square, `paint::HOVER` under the pointer. `sheet_head`
/// gives it its tooltip, `Close` with `esc` (esc closes it too); `label` is
/// the longer name a screen reader hears.
pub fn sheet_close(id: &'static str, label: &'static str, cx: &App) -> Button {
    components::form_button(id, cx)
        .debug_selector(move || id.into())
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .accessibility_label(label)
        .child(icon(icons::CLOSE, ICON_BUTTON_GLYPH, TEXT_MUTED))
}

/// The pinned footer under a `paint::LINE`: secondary actions left, the
/// completing action right. It never scrolls with the body.
pub fn sheet_footer(left: impl IntoElement, right: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap(px(MODAL_GAP))
        .px(px(MODAL_PAD))
        .py(px(MODAL_GAP))
        .border_t_1()
        .border_color(paint::LINE)
        .child(left)
        .child(right)
}

// ------------------------------------------------------------ Settings

/// The Settings pages, in sidebar order. `About` is pinned to the
/// sidebar's foot, apart from the settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageKey {
    #[default]
    NewThreads,
    Permissions,
    Behaviour,
    About,
}

impl PageKey {
    pub const ALL: [PageKey; 4] = [
        PageKey::NewThreads,
        PageKey::Permissions,
        PageKey::Behaviour,
        PageKey::About,
    ];

    pub fn title(self) -> &'static str {
        match self {
            PageKey::NewThreads => "New threads",
            PageKey::Permissions => "Permissions",
            PageKey::Behaviour => "Behaviour",
            PageKey::About => "About",
        }
    }

    /// The sidebar row's selector, `settings-page-<slug>`.
    pub fn selector(self) -> &'static str {
        match self {
            PageKey::NewThreads => "settings-page-new-threads",
            PageKey::Permissions => "settings-page-permissions",
            PageKey::Behaviour => "settings-page-behaviour",
            PageKey::About => "settings-page-about",
        }
    }

    /// The page `delta` rows away in sidebar order, held at either end.
    pub fn step(self, delta: isize) -> PageKey {
        let at = PageKey::ALL
            .iter()
            .position(|page| *page == self)
            .unwrap_or(0) as isize;
        let to = (at + delta).clamp(0, PageKey::ALL.len() as isize - 1);
        PageKey::ALL[to as usize]
    }
}

type Control = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// One setting: its label, an optional one-line hint, the words search
/// also finds it by (every option label), and its control.
#[derive(Clone)]
pub struct Row {
    title: SharedString,
    hint: SharedString,
    words: Vec<SharedString>,
    control: Control,
    /// A fact's value gives way (wraps) rather than hold its width.
    fact: bool,
}

impl Row {
    fn new(
        title: impl Into<SharedString>,
        hint: impl Into<SharedString>,
        words: Vec<SharedString>,
        control: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            hint: hint.into(),
            words,
            control: Rc::new(control),
            fact: false,
        }
    }

    fn fact(mut self) -> Self {
        self.fact = true;
        self
    }

    /// Case-insensitive substring on the label, the hint, the option
    /// labels, and the group and page it sits in (`claude` finds every
    /// Claude row).
    fn matches(&self, query: &str, page: &str, group: Option<&str>) -> bool {
        let query = query.to_lowercase();
        [self.title.as_ref(), self.hint.as_ref(), page]
            .into_iter()
            .chain(group)
            .chain(self.words.iter().map(|word| word.as_ref()))
            .any(|text| text.to_lowercase().contains(&query))
    }
}

/// A group of rows under a quiet label; a provider group's label leads
/// with its logomark in brand colour.
#[derive(Clone)]
pub struct Group {
    title: Option<&'static str>,
    mark: Option<(&'static str, u32)>,
    rows: Vec<Row>,
}

impl Group {
    pub fn new(title: Option<&'static str>) -> Self {
        Self {
            title,
            mark: None,
            rows: Vec::new(),
        }
    }

    pub fn mark(mut self, path: &'static str, ink: u32) -> Self {
        self.mark = Some((path, ink));
        self
    }

    pub fn rows(mut self, rows: impl IntoIterator<Item = Row>) -> Self {
        self.rows.extend(rows);
        self
    }
}

pub struct Page {
    key: PageKey,
    groups: Vec<Group>,
}

pub fn page(key: PageKey, groups: Vec<Group>) -> Page {
    Page { key, groups }
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
/// results, split by a `paint::LINE`.
pub fn body(
    pages: Vec<Page>,
    selected: PageKey,
    search: &Entity<InputState>,
    select: impl Fn(PageKey, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let query = search.read(cx).value().trim().to_string();
    let shown = shown(&pages, selected, &query);
    let searching = !query.is_empty();
    let select = Rc::new(select);
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
        .border_color(paint::LINE)
        .pb(px(HALF_ROW))
        .child(search_field(search, window, cx))
        .child(
            div().flex().flex_col().pt(px(HALF_ROW)).children(
                [
                    PageKey::NewThreads,
                    PageKey::Permissions,
                    PageKey::Behaviour,
                ]
                .map(|key| nav_row(key, cx)),
            ),
        )
        .child(div().flex_1())
        .child(nav_row(PageKey::About, cx));

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
                            .text_color(rgb(TEXT_MUTED))
                            .child("no matching settings"),
                    )
                })
                .children(
                    shown
                        .groups
                        .iter()
                        .map(|(label, group)| group_rows(label.clone(), group, window, cx)),
                ),
        );

    div()
        .flex()
        .flex_1()
        .min_h_0()
        .child(sidebar)
        .child(content)
}

/// The search line atop the sidebar: the accent `❯` in its gutter, then the
/// field, over a `paint::LINE` — the palette's own input line.
fn search_field(search: &Entity<InputState>, window: &Window, cx: &App) -> Div {
    let focused = gpui::Focusable::focus_handle(search.read(cx), cx).is_focused(window);
    components::focused(
        div()
            .debug_selector(|| "settings-search".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(ROW + 2.0 * SPACE_1))
            .px(px(CH))
            .border_b_1()
            .border_color(paint::LINE)
            .child(crate::menu::gutter(true))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(search).appearance(false).small().cleanable(true)),
            ),
        focused,
    )
}

/// A sidebar row: the page's name on the grid. The page shown is
/// `paint::SELECTION` with the accent `❯` and `TEXT_STRONG`; the rest `TEXT`,
/// `paint::HOVER` under the pointer. While a search runs, a page with no
/// match reads `TEXT_MUTED`.
fn nav_row(key: PageKey, selected: bool, hit: bool, cx: &App) -> Button {
    let (rest, hover): (gpui::Hsla, gpui::Hsla) = if selected {
        (paint::SELECTION.into(), paint::SELECTION_HOVER.into())
    } else {
        (gpui::rgba(TRANSPARENT).into(), paint::HOVER.into())
    };
    let ink = match (selected, hit) {
        (true, _) => TEXT_STRONG,
        (false, true) => TEXT,
        (false, false) => TEXT_MUTED,
    };
    components::faded_button(
        key.selector(),
        rest,
        hover,
        paint::PRESS.into(),
        rgb(ink).into(),
        cx,
    )
    .selected(selected)
    .debug_selector(move || key.selector().into())
    .accessibility_label(key.title())
    .w_full()
    .h(px(SETTINGS_NAV_ROW_H))
    .px(px(CH))
    .child(
        div()
            .flex()
            .flex_1()
            .items_center()
            .child(crate::menu::gutter(selected))
            .child(components::form_label(key.title(), ink)),
    )
}

/// A group: its label (and a provider's mark) as a `TEXT_MUTED` row, then
/// its rows, each over a `paint::LINE` rule.
fn group_rows(
    label: Option<SharedString>,
    group: &Group,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let rows: Vec<AnyElement> = group
        .rows
        .iter()
        .map(|row| {
            setting_row(row, window, cx)
                .border_t_1()
                .border_color(paint::LINE)
                .into_any_element()
        })
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
                .text_color(rgb(TEXT_MUTED))
                .children(group.mark.map(|(path, ink)| icon(path, STATUS_LOGO, ink)))
                .child(section_words(&label))
        }))
        .children(rows)
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
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(2.0 * CH))
        .min_h(px(if hinted {
            SETTINGS_ROW_HINT_H
        } else {
            SETTINGS_ROW_H
        }))
        .px(px(SETTINGS_ROW_PAD_X))
        .py(px(HALF_ROW / 2.0))
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .map(|text| {
                    if row.fact {
                        text.flex_shrink_0().w(px(SETTINGS_FACT_KEY_W))
                    } else {
                        text.flex_1()
                    }
                })
                .child(
                    components::text_ui()
                        .truncate()
                        .text_color(rgb(TEXT))
                        .child(row.title.clone()),
                )
                .when(hinted, |text| text.child(hint(row.hint.clone()))),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .map(|control| {
                    if row.fact {
                        control.flex_1().min_w_0()
                    } else {
                        control.flex_shrink_0()
                    }
                })
                .child((row.control)(window, cx)),
        )
}

/// A row's hint: `TEXT_MUTED`, one line. One that opens with a key table's
/// chord (`cmd-B toggles it any time`) draws the chord as keys (`⌘B`,
/// `components::key_combo`): the face has no command glyph.
fn hint(detail: SharedString) -> Div {
    let line = components::text_ui()
        .min_w_0()
        .truncate()
        .text_color(rgb(TEXT_MUTED));
    match detail.split_once(' ') {
        Some((chord, rest)) if chord.starts_with("cmd-") => line
            .flex()
            .items_center()
            .gap(px(CH))
            .child(components::key_combo(chord, TEXT_MUTED))
            .child(SharedString::from(rest.to_string())),
        _ => line.child(detail),
    }
}

/// Two options are a pair of tokens, the chosen one inverse; more are a
/// chooser (the value and a chevron that opens a menu).
pub fn choices<T: Clone + 'static>(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    options: Vec<(SharedString, bool, T)>,
    change: impl Fn(T, &mut App) + 'static,
) -> Row {
    // Long ladders are values to choose, not a second row of navigation.
    if options.len() > 2 {
        return chooser(id, title, detail, options, change);
    }
    let change = Rc::new(change);
    let keywords: Vec<_> = options.iter().map(|(label, _, _)| label.clone()).collect();
    Row::new(title, detail, keywords, move |_, cx| {
        div()
            .flex()
            .items_center()
            .children(
                options
                    .iter()
                    .enumerate()
                    .map(|(at, (label, selected, value))| {
                        let value = value.clone();
                        let change = change.clone();
                        chip((id, at), label.clone(), *selected, cx).on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            change(value.clone(), cx);
                        })
                    }),
            )
            .into_any_element()
    })
}

/// A choice from a list: the current value and a chevron, as wide as its
/// value (at most `SETTINGS_MENU_MAX_W`), `paint::HOVER` under the pointer.
/// The menu keeps every value, the standing one checked; search also
/// indexes the hidden labels.
pub fn chooser<T: Clone + 'static>(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    options: Vec<(SharedString, bool, T)>,
    change: impl Fn(T, &mut App) + 'static,
) -> Row {
    let change = Rc::new(change);
    let keywords: Vec<_> = options.iter().map(|(label, _, _)| label.clone()).collect();
    let selected = options
        .iter()
        .find(|(_, selected, _)| *selected)
        .map(|(label, _, _)| label.clone())
        .unwrap_or_else(|| "choose".into());
    Row::new(title, detail, keywords, move |_, cx| {
        let options = options.clone();
        let change = change.clone();
        components::form_button(id, cx)
            .debug_selector(move || id.into())
            .accessibility_label(format!("{title}: {selected}"))
            .h(px(SETTINGS_CONTROL_H))
            .max_w(px(SETTINGS_MENU_MAX_W))
            .px(px(CH))
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
                    .child(icon(icons::CHEVRON_DOWN, ICON_CHEVRON, TEXT_MUTED).flex_shrink_0()),
            )
            .dropdown_menu_with_anchor(gpui::Anchor::TopRight, move |menu, _, _| {
                options.iter().fold(
                    menu.min_w(px(CHOICE_MENU_MIN_W))
                        .max_w(px(CHOICE_MENU_MAX_W))
                        .max_h(px(MENU_MAX_H))
                        .scrollable(true),
                    |menu, (label, selected, value)| {
                        let value = value.clone();
                        let change = change.clone();
                        // The CLI's own default says whose choice it is.
                        let mut item = MenuItem::new(label.clone()).checked(*selected);
                        if label.as_ref() == CLI_DEFAULT {
                            item = item.detail(CLI_DEFAULT_NOTE);
                        }
                        menu.item(
                            PopupMenuItem::element(move |_, _| {
                                components::kit_row(crate::menu::row_face(&item, false, false))
                            })
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                change(value.clone(), cx);
                            }),
                        )
                    },
                )
            })
            .into_any_element()
    })
}

/// The unset value of every setting a CLI decides for itself.
pub const CLI_DEFAULT: &str = "CLI default";
/// What a chooser's `CLI default` row says about itself.
const CLI_DEFAULT_NOTE: &str = "follows the CLI";

/// Keep the effective selected value represented even when it is an alias or
/// absent from the current catalog. No choice is silently made for the user.
pub fn model_options(
    catalog: Vec<ferrite_core::ModelInfo>,
    chosen: Option<&str>,
) -> Vec<(SharedString, bool, Option<String>)> {
    let chosen = chosen.filter(|value| *value != "default");
    let mut represented = chosen.is_none();
    let mut options = vec![(CLI_DEFAULT.into(), represented, None)];
    for model in catalog.into_iter().filter(|model| model.value != "default") {
        let selected = !represented && chosen.is_some_and(|chosen| model.is(chosen));
        represented |= selected;
        options.push((model.display.into(), selected, Some(model.value)));
    }
    if !represented {
        let chosen = chosen.expect("the default always has a row");
        options.push((chosen.to_string().into(), true, Some(chosen.to_string())));
    }
    options
}

/// The on/off token's word and ink: `on` in the accent at `W_STRONG`, `off`
/// in `TEXT_MUTED`.
fn toggle_face(checked: bool) -> (&'static str, u32) {
    if checked {
        ("on", ACCENT)
    } else {
        ("off", TEXT_MUTED)
    }
}

/// An on/off setting: one word on the row — `on` in the accent, `off` dim —
/// that flips the setting on a click, Enter or Space (a tab stop with the
/// focus outline), `paint::HOVER` under the pointer. No iOS switch.
pub fn toggle(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    checked: bool,
    change: impl Fn(bool, &mut App) + 'static,
) -> Row {
    let change = Rc::new(change);
    Row::new(title, detail, Vec::new(), move |_, cx| {
        let change = change.clone();
        let (word, ink) = toggle_face(checked);
        components::form_button(id, cx)
            .selected(checked)
            .debug_selector(move || id.into())
            .accessibility_label(format!("{title}: {word}"))
            .h(px(SETTINGS_CONTROL_H))
            .px(px(CH))
            .child(
                components::text_ui()
                    .font_weight(if checked { W_STRONG } else { W_BODY })
                    .text_color(rgb(ink))
                    .child(word),
            )
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                change(!checked, cx);
            })
            .into_any_element()
    })
}

/// One option of a short ladder (a two-way choice, a stepper's ends): a
/// token one row high with a cell of padding. Chosen, it is inverse —
/// `ACCENT_STRONG` with `ON_ACCENT` at `W_STRONG`; the rest `TEXT_MUTED`,
/// `paint::HOVER` under the pointer. A tab stop.
pub fn chip(id: (&'static str, usize), label: SharedString, selected: bool, cx: &App) -> Button {
    let (rest, hover, ink): (gpui::Hsla, gpui::Hsla, u32) = if selected {
        (
            rgb(ACCENT_STRONG).into(),
            rgb(PRIMARY_HOVER).into(),
            ON_ACCENT,
        )
    } else {
        (
            gpui::rgba(TRANSPARENT).into(),
            paint::HOVER.into(),
            TEXT_MUTED,
        )
    };
    components::faded_button(id, rest, hover, paint::PRESS.into(), rgb(ink).into(), cx)
        .tab_stop(true)
        .selected(selected)
        .toggled(selected)
        .debug_selector(move || format!("{}-{}", id.0, id.1))
        .h(px(SETTINGS_CONTROL_H))
        .px(px(TOKEN_PAD_X))
        .child(
            components::text_ui()
                .font_weight(if selected { W_STRONG } else { W_BODY })
                .text_color(rgb(ink))
                .child(label),
        )
}

/// A size stepper: `‹ 14px ›` — the ends are tokens (a tab stop each), the
/// value tabular between them in its fixed slot. An end with nowhere to go
/// is `TEXT_FAINT` and takes no press. The keys that do the same ride the
/// hint.
pub fn stepper(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    value: SharedString,
    can_down: bool,
    can_up: bool,
    step: impl Fn(i32, &mut App) + 'static,
) -> Row {
    let step = Rc::new(step);
    Row::new(title, detail, vec![value.clone()], move |_, cx| {
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
    })
}

/// A read-only fact (About): its key at the left, its value at the right
/// in `TEXT_MUTED` (versions are machine text), wrapping anywhere rather
/// than cut. Searchable by both.
pub fn fact(title: &'static str, value: SharedString) -> Row {
    let words = vec![value.clone()];
    Row::new(title, "", words, move |_, _| {
        fact_value(title, value.clone()).into_any_element()
    })
    .fact()
}

/// A row that says where something stands (its hint) and, when there is
/// something to do about it, offers the one button that does it: the
/// primary face, one row high, hugging its label. With nothing to do, the
/// row reads its `value` as a fact does.
pub fn action(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    value: SharedString,
    button: Option<SharedString>,
    act: impl Fn(&mut App) + 'static,
) -> Row {
    let act = Rc::new(act);
    let words = std::iter::once(value.clone())
        .chain(button.iter().cloned())
        .collect();
    Row::new(title, detail, words, move |_, cx| match button.clone() {
        Some(label) => {
            let act = act.clone();
            components::primary_button(id, false, cx)
                .debug_selector(move || id.into())
                .h(px(SETTINGS_CONTROL_H))
                .px(px(CH))
                .child(components::form_label(label, ON_ACCENT))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    act(cx);
                })
                .into_any_element()
        }
        None => fact_value(title, value.clone()).into_any_element(),
    })
}

/// A path fact: shown from `~` when it lies under the home directory, and
/// copied whole on a click. The copy mark sits after it in structure ink,
/// brightening under the pointer, and turns to a check once `copied`.
pub fn path_fact(
    title: &'static str,
    path: String,
    copied: bool,
    on_copy: impl Fn(&mut App) + 'static,
) -> Row {
    let shown: SharedString = home_relative(&path, std::env::var("HOME").ok().as_deref()).into();
    let words = vec![shown.clone(), path.clone().into()];
    let on_copy = Rc::new(on_copy);
    Row::new(title, "", words, move |_, _| {
        let path = path.clone();
        let on_copy = on_copy.clone();
        let id = gpui::ElementId::from(SharedString::from(format!("settings-copy-{title}")));
        let key = crate::pointer::hover_key(&id);
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
            .tooltip(crate::menu::tooltip(if copied {
                "Copied"
            } else {
                "Copy path"
            }))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(path.clone()));
                on_copy(cx);
            })
            .child(fact_value(title, shown.clone()))
            .child(
                icon(
                    if copied { icons::CHECK } else { icons::COPY },
                    ICON_CHEVRON,
                    if copied { RUNNING } else { TEXT_FAINT },
                )
                .flex_shrink_0()
                .group_hover(COPY_GROUP, |style| style.text_color(rgb(TEXT_MUTED))),
            )
            .into_any_element()
    })
    .fact()
}

/// A path fact's hover reaches its copy mark through this group.
const COPY_GROUP: &str = "settings-copy";

fn fact_value(title: &'static str, value: SharedString) -> Div {
    components::text_ui()
        .debug_selector(move || format!("settings-fact-{title}"))
        .min_w_0()
        .text_right()
        .font_family(FONT_CODE)
        .font_weight(W_BODY)
        .text_color(rgb(TEXT_MUTED))
        .child(value)
}

/// `path` from `~` when it lies under `home`.
fn home_relative(path: &str, home: Option<&str>) -> String {
    match home.filter(|home| home.len() > 1) {
        Some(home) => match path.strip_prefix(home.trim_end_matches('/')) {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
            _ => path.to_string(),
        },
        None => path.to_string(),
    }
}

/// The nav chrome's gear: the door to this panel. Ground and glyph blend
/// to their hover faces over the one 150ms blend (`TEXT_MUTED` → `TEXT`, as
/// the collapse button does). `gear` hangs its tooltip, `Settings ⌘,`.
pub fn gear_button(cx: &App) -> Button {
    let id = gpui::ElementId::from("settings-gear");
    let key = crate::pointer::hover_key(&id);
    let glyph = crate::motion::hover_blend(&key, rgb(TEXT_MUTED).into(), rgb(TEXT).into());
    components::faded_button(
        id,
        gpui::rgba(TRANSPARENT).into(),
        paint::HOVER.into(),
        paint::PRESS.into(),
        rgb(TEXT_MUTED).into(),
        cx,
    )
    .debug_selector(|| "settings-gear".into())
    .w(px(ICON_BUTTON))
    .h(px(ICON_BUTTON))
    .p_0()
    .accessibility_label("Settings")
    .child(icon(icons::GEAR, ICON_BUTTON_GLYPH, TEXT_MUTED).text_color(glyph))
}

/// The gear with its tooltip, `Settings ⌘,` (a kit button's own tooltip is
/// plain text, so the key rides a wrapper).
pub fn gear(button: Button, id: &'static str) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex_shrink_0()
        .tooltip(crate::menu::action_tooltip(
            "Settings",
            "cockpit::OpenSettings",
        ))
        .child(button)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_choices_keep_default_alias_and_custom_values_selected() {
        use ferrite_core::{providers::models::fallback, store::Provider};
        for provider in [Provider::Claude, Provider::Codex] {
            let options = model_options(fallback(provider), None);
            assert_eq!(options.iter().filter(|(_, on, _)| *on).count(), 1);
            assert_eq!(options[0], ("CLI default".into(), true, None));
        }
        let options = model_options(fallback(Provider::Claude), Some("claude-sonnet-5"));
        let selected: Vec<_> = options.iter().filter(|(_, on, _)| *on).collect();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].2.as_deref(), Some("sonnet"));
        let options = model_options(fallback(Provider::Codex), Some("private-model"));
        assert_eq!(
            options.last().unwrap(),
            &("private-model".into(), true, Some("private-model".into()))
        );
        assert_eq!(options.iter().filter(|(_, on, _)| *on).count(), 1);
    }

    /// Terminal-native (WP-E): a sheet is the float at sheet size — the
    /// float ground, the `LINE2` edge, square, the one float shadow — over
    /// the veil.
    #[test]
    fn the_sheet_is_the_float_over_the_veil() {
        let mut drawn = sheet(SETTINGS_W, SETTINGS_H);
        let style = drawn.style();
        assert_eq!(style.background, Some(paint::FLOAT.into()));
        assert_eq!(style.border_color, Some(paint::LINE2.into()));
        assert_eq!(style.box_shadow, Some(components::float_shadow()));
        assert_eq!(style.corner_radii.top_left, Some(px(0.).into()), "square");
        assert_eq!(veil().style().background, Some(gpui::rgba(VEIL).into()));
    }

    /// Settings is the same recipe, and every sheet's head stands over a
    /// rule.
    #[test]
    fn the_settings_sheet_is_the_sheet_recipe() {
        let mut drawn = settings_sheet();
        let style = drawn.style();
        assert_eq!(style.background, Some(paint::FLOAT.into()));
        assert_eq!(style.border_color, Some(paint::LINE2.into()));
        assert_eq!(style.box_shadow, Some(components::float_shadow()));
        for head in [settings_head(div()), sheet_head("Project", div())] {
            let mut head = head;
            assert_eq!(head.style().border_widths.bottom, Some(px(1.).into()));
        }
    }

    #[test]
    fn rows_sit_on_the_grid_and_the_sheet_fits_its_pages() {
        assert_eq!(SETTINGS_NAV_ROW_H, ROW);
        assert_eq!(SETTINGS_CONTROL_H, ROW);
        // A label and its hint are two rows; each row keeps a quarter row
        // of air above and below.
        assert_eq!(SETTINGS_ROW_H, ROW + HALF_ROW);
        assert_eq!(SETTINGS_ROW_HINT_H, 2.0 * ROW + HALF_ROW);
        // Checked at compile time: the rhythm and the size can never drift.
        const _: () = assert!(SETTINGS_CONTROL_H <= SETTINGS_ROW_H - 2.0 * SPACE_1);
        const _: () = assert!(
            SETTINGS_W <= 800.0 && SETTINGS_H <= 600.0,
            "sized to content"
        );
    }

    #[test]
    fn pages_step_in_sidebar_order_and_hold_at_the_ends() {
        assert_eq!(PageKey::NewThreads.step(1), PageKey::Permissions);
        assert_eq!(PageKey::Behaviour.step(1), PageKey::About);
        assert_eq!(PageKey::About.step(1), PageKey::About);
        assert_eq!(PageKey::NewThreads.step(-1), PageKey::NewThreads);
        assert_eq!(PageKey::About.step(-3), PageKey::NewThreads);
        assert_eq!(PageKey::default(), PageKey::NewThreads);
    }

    /// Search spans every page: a row matches on its label, its hint, an
    /// option label, or the group and page it sits in; each result card is
    /// labelled `Page · Group`, and only pages with a hit stay lit.
    #[test]
    fn search_filters_every_page_and_labels_each_result() {
        let row = |title: &'static str, hint: &'static str, words: &[&'static str]| {
            Row::new(
                title,
                hint,
                words.iter().map(|word| SharedString::from(*word)).collect(),
                |_, _| div().into_any_element(),
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
    fn a_path_under_home_reads_from_tilde() {
        let home = Some("/Users/op");
        assert_eq!(
            home_relative("/Users/op/Library/ferrite/threads", home),
            "~/Library/ferrite/threads"
        );
        assert_eq!(home_relative("/Users/op", home), "~");
        assert_eq!(
            home_relative("/Users/operator/x", home),
            "/Users/operator/x"
        );
        assert_eq!(home_relative("/tmp/x", home), "/tmp/x");
        assert_eq!(home_relative("/tmp/x", None), "/tmp/x");
        assert_eq!(home_relative("/x", Some("/")), "/x");
    }

    /// Terminal-native (WP-E): an on/off setting is a word, `on` in the
    /// accent and `off` dim — no iOS switch.
    #[test]
    fn a_toggle_reads_on_in_the_accent_and_off_dim() {
        assert_eq!(toggle_face(true), ("on", ACCENT));
        assert_eq!(toggle_face(false), ("off", TEXT_MUTED));
    }

    /// The chosen token of a short ladder is inverse — the accent ground
    /// with dark ink — and the rest lie on no ground at all.
    #[gpui::test]
    fn a_chosen_token_is_inverse_and_the_rest_lie_flat(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            crate::theme::init_components(cx);
            let mut on = chip(("c", 0), "Claude".into(), true, cx);
            let mut off = chip(("c", 1), "Codex".into(), false, cx);
            assert_eq!(on.style().background, Some(rgb(ACCENT_STRONG).into()));
            assert_eq!(off.style().background, None);
        });
    }
}
