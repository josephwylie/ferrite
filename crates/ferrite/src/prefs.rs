//! Ferrite's sheets, and the Settings sheet built on them. Values and
//! persistence remain owned by the cockpit (`CockpitView::change_settings`);
//! this module only lays settings out, searches them and draws their
//! controls. The Settings sheet's rules live in `theme.rs` (`Settings
//! sheet`).
//!
//! **The sheet recipe** (the Project editor, the image preview): a `VEIL`
//! over the Cockpit; the sheet `RAISED` with a `HAIRLINE_STRONG` edge,
//! `R_PANE` and the float shadow; a `MODAL_HEAD_H` head (the one `W_LABEL`
//! title and the one close control, `×` with the tooltip `Close esc`) over
//! a hairline; a scrolling body; a footer pinned under a hairline. Those
//! two rules mark where the body scrolls. Settings is the recipe's quieter
//! variant: a `PANE` sheet whose head needs no rule, because its cards are
//! what is raised.

use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, AnyElement, App, Div, Entity, SharedString, Window};

use gpui::component::button::Button;
use gpui::component::input::{Input, InputState};
use gpui::component::menu::{DropdownMenu, PopupMenuItem};
use gpui::component::{Selectable, Sizable};
use gpui_base::{Switch, SwitchThumb, SwitchTrack};

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
/// the window: `RAISED`, the strong hairline edge, `R_PANE`, the float
/// shadow, UI type. Its definite height is what lets the body scroll.
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
        .bg(rgb(RAISED))
        .border_1()
        .border_color(rgba(HAIRLINE_STRONG))
        .shadow(components::float_shadow())
}

/// The Settings sheet: `sheet` on `PANE`, one step under its `RAISED`
/// cards, at `SETTINGS_W` × `SETTINGS_H` (capped to the window).
pub fn settings_sheet() -> Div {
    sheet(SETTINGS_W, SETTINGS_H).bg(rgb(PANE))
}

/// A sheet's head: its title (the sheet's one `W_LABEL` line), then the
/// close button, over a hairline. The close button's tooltip names `esc`;
/// no keycap repeats it.
pub fn sheet_head(title: impl Into<SharedString>, close: impl IntoElement) -> Div {
    head_row(title, close)
        .border_b_1()
        .border_color(rgba(HAIRLINE))
}

/// `sheet_head` without its rule: the Settings head, set apart from the
/// sidebar and the page by space alone.
pub fn settings_head(close: impl IntoElement) -> Div {
    head_row("Settings", close)
}

fn head_row(title: impl Into<SharedString>, close: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(MODAL_GAP))
        .h(px(MODAL_HEAD_H))
        .pl(px(MODAL_PAD))
        .pr(px(SPACE_2))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .font_weight(W_LABEL)
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

/// A sheet's close button, the one close control on every sheet: the
/// close glyph in a 28px square that lifts to `FILL` on the raised sheet.
/// `sheet_head` gives it its tooltip, `Close` with `esc` as a mono suffix
/// (esc closes it too); `label` is the longer name a screen reader hears.
pub fn sheet_close(id: &'static str, label: &'static str, cx: &App) -> Button {
    components::form_button(id, cx)
        .debug_selector(move || id.into())
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .accessibility_label(label)
        .child(icon(icons::CLOSE, ICON_BUTTON_GLYPH, TEXT_MUTED))
}

/// The pinned footer under a hairline: secondary actions left, the
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
        .border_color(rgba(HAIRLINE))
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

    fn icon(self) -> &'static str {
        match self {
            PageKey::NewThreads => icons::NEW_THREAD,
            PageKey::Permissions => icons::SHIELD,
            PageKey::Behaviour => icons::SLIDERS,
            PageKey::About => icons::INFO,
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

/// A card of rows under a quiet label; a provider group's label leads
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
/// holds a query, every matching row of every page, each card labelled
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

/// The Settings body under its head: the sidebar (search, pages, `About`
/// at the foot) and the selected page, or the search's results.
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
        .pl(px(MODAL_PAD - SPACE_1))
        .pr(px(SPACE_2))
        .pb(px(MODAL_PAD - SPACE_1))
        .child(
            div()
                .px(px(SPACE_1))
                .child(search_field(search, window, cx)),
        )
        .child(
            div().flex().flex_col().pt(px(SPACE_3)).children(
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
        .pl(px(SPACE_4))
        .pr(px(SPACE_6))
        .pb(px(SPACE_6))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .h(px(SETTINGS_NAV_ROW_H))
                .pl(px(SETTINGS_ROW_PAD_X))
                .text_size(px(FS_PROSE))
                .line_height(px(LH_PROSE))
                .font_weight(W_LABEL)
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
                            .pl(px(SETTINGS_ROW_PAD_X))
                            .text_color(rgb(TEXT_MUTED))
                            .child("No matching settings"),
                    )
                })
                .children(
                    shown
                        .groups
                        .iter()
                        .map(|(label, group)| group_card(label.clone(), group, window, cx)),
                ),
        );

    div()
        .flex()
        .flex_1()
        .min_h_0()
        .child(sidebar)
        .child(content)
}

/// The search field atop the sidebar: a row's height, `RAISED` with a
/// `HAIRLINE` edge (a field one step up off the sheet), the magnifier in
/// `TEXT_MUTED`, and the focus outline while it holds the caret.
fn search_field(search: &Entity<InputState>, window: &Window, cx: &App) -> Div {
    let focused = gpui::Focusable::focus_handle(search.read(cx), cx).is_focused(window);
    components::focused(
        div()
            .debug_selector(|| "settings-search".into())
            .flex()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(SETTINGS_NAV_ROW_H))
            .pl(px(SPACE_2))
            .pr(px(SPACE_1))
            .rounded(px(R_CONTROL))
            .bg(rgb(RAISED))
            .border_1()
            .border_color(rgba(HAIRLINE))
            .child(icon(icons::SEARCH, SETTINGS_SEARCH_ICON, TEXT_MUTED).flex_shrink_0())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(search).appearance(false).small().cleanable(true)),
            ),
        focused,
    )
}

/// A sidebar row: the page's mark and label at the one list pitch. The
/// selected row is `FILL` with `TEXT_STRONG`; the rest hover to `HOVER`.
/// While a search runs, a page with no match reads `TEXT_MUTED`.
fn nav_row(key: PageKey, selected: bool, hit: bool, cx: &App) -> Button {
    let (rest, hover) = if selected {
        (FILL, FILL_HOVER)
    } else {
        (TRANSPARENT, HOVER)
    };
    let rest = if selected {
        rgb(rest).into()
    } else {
        rgba(rest).into()
    };
    let (ink, mark) = match (selected, hit) {
        (true, _) => (TEXT_STRONG, TEXT),
        (false, true) => (TEXT_2, TEXT_MUTED),
        (false, false) => (TEXT_MUTED, TEXT_MUTED),
    };
    components::faded_button(
        key.selector(),
        rest,
        rgb(hover).into(),
        rgb(PRESSED).into(),
        rgb(ink).into(),
        cx,
    )
    .selected(selected)
    .debug_selector(move || key.selector().into())
    .accessibility_label(key.title())
    .w_full()
    .h(px(SETTINGS_NAV_ROW_H))
    .px(px(SPACE_2))
    .child(
        div()
            .flex()
            .flex_1()
            .items_center()
            .gap(px(SETTINGS_NAV_ICON_GAP))
            .child(icon(key.icon(), SETTINGS_NAV_ICON, mark))
            .child(components::form_label(key.title(), ink)),
    )
}

/// A group: its label (and a provider's mark) above one card of rows split
/// by hairlines.
fn group_card(
    label: Option<SharedString>,
    group: &Group,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let rows: Vec<AnyElement> = group
        .rows
        .iter()
        .enumerate()
        .map(|(at, row)| {
            setting_row(row, window, cx)
                .when(at > 0, |row| row.border_t_1().border_color(rgba(HAIRLINE)))
                .into_any_element()
        })
        .collect();
    div()
        .flex()
        .flex_col()
        .gap(px(SETTINGS_LABEL_GAP))
        .children(label.map(|label| {
            components::text_meta()
                .flex()
                .items_center()
                .gap(px(SPACE_1_5))
                .pl(px(SETTINGS_ROW_PAD_X))
                .font_weight(W_LABEL)
                .children(
                    group
                        .mark
                        .map(|(path, ink)| icon(path, MENU_SECTION_ICON, ink)),
                )
                .child(section_words(&label))
        }))
        .child(
            div()
                .flex()
                .flex_col()
                .bg(rgb(RAISED))
                .rounded(px(R_BLOCK))
                .border_1()
                .border_color(rgba(HAIRLINE))
                .children(rows),
        )
}

/// `Page · Group`, the `·` in structure ink.
fn section_words(label: &str) -> Div {
    let mut line = div().flex().items_center().gap(px(SPACE_1));
    for (at, part) in label.split(" · ").enumerate() {
        if at > 0 {
            line = line.child(div().text_color(rgb(TEXT_FAINT)).child("·"));
        }
        line = line.child(SharedString::from(part.to_string()));
    }
    line
}

/// One setting row: label over hint at the left, the control right and
/// centred; 36px, or 44px with a hint.
fn setting_row(row: &Row, window: &mut Window, cx: &mut App) -> Div {
    let hinted = !row.hint.is_empty();
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(SPACE_4))
        .min_h(px(if hinted {
            SETTINGS_ROW_HINT_H
        } else {
            SETTINGS_ROW_H
        }))
        .px(px(SETTINGS_ROW_PAD_X))
        .py(px(SPACE_1))
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
                .child(components::text_ui().truncate().child(row.title.clone()))
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

/// A row's hint: `FS_SM` `TEXT_MUTED`, one line. One that opens with a key
/// table's chord (`cmd-B toggles it any time`) draws the chord as keys
/// (`⌘B`, `components::key_combo`): neither face has the command glyph.
fn hint(detail: SharedString) -> Div {
    let line = components::text_meta().min_w_0().truncate();
    match detail.split_once(' ') {
        Some((chord, rest)) if chord.starts_with("cmd-") => line
            .flex()
            .items_center()
            .gap(px(SPACE_1))
            .child(components::key_combo(chord, TEXT_MUTED))
            .child(SharedString::from(rest.to_string())),
        _ => line.child(detail),
    }
}

/// Two options are a segmented tray; more are a menu button (`chooser`).
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
            .gap(px(FORM_CHOICE_PAD))
            .p(px(FORM_CHOICE_PAD))
            .rounded(px(R_CONTROL))
            .bg(rgb(RAISED_2))
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

/// A choice from a list: a compact menu button showing the current value
/// and a chevron, as wide as its value (at most `SETTINGS_MENU_MAX_W`).
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
        .unwrap_or_else(|| "Choose an option".into());
    Row::new(title, detail, keywords, move |_, cx| {
        let options = options.clone();
        let change = change.clone();
        // A control on the raised card: `RAISED_2`, lifting to `FILL`
        // under the pointer over the one blend.
        components::faded_button(
            id,
            rgb(RAISED_2).into(),
            rgb(FILL).into(),
            rgb(FILL_HOVER).into(),
            rgb(TEXT).into(),
            cx,
        )
        .tab_stop(true)
        .debug_selector(move || id.into())
        .accessibility_label(format!("{title}: {selected}"))
        .h(px(SETTINGS_CONTROL_H))
        .max_w(px(SETTINGS_MENU_MAX_W))
        .pl(px(FORM_FIELD_PAD_X))
        .pr(px(SPACE_2))
        .child(
            div()
                .flex()
                .items_center()
                .min_w_0()
                .gap(px(SPACE_1_5))
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
                            components::kit_row(components::menu_row_content(&item, false, false))
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

/// An on/off setting: a switch. Custom rather than the kit's (whose tokens
/// do not reach it): the track `ACCENT_STRONG` when on and `FILL_HOVER`
/// when off, so both read on the raised card; the thumb `TEXT_STRONG`,
/// sprung.
pub fn toggle(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    checked: bool,
    change: impl Fn(bool, &mut App) + 'static,
) -> Row {
    let change = Rc::new(change);
    Row::new(title, detail, Vec::new(), move |window, cx| {
        let change = change.clone();
        // The thumb moves over 150ms on the standard curve when the pointer
        // flipped it, and lands at once on a keyboard toggle (no spring).
        let now = cx.background_executor().now();
        let spec = if pointer_pressed(id, now) {
            crate::motion::TURN
        } else {
            crate::motion::MotionSpec::new(0, crate::motion::EASE_STANDARD)
        };
        let travel = crate::motion::settle(
            (gpui::ElementId::from(id), "thumb"),
            if checked { 1. } else { 0. },
            spec,
            window,
            cx,
        );
        let thumb_x = px(SWITCH_TRAVEL * travel);
        div()
            .id(id)
            .debug_selector(move || id.into())
            .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                note_pointer_press(id, cx.background_executor().now());
            })
            .child(
                Switch::new(id)
                    .checked(checked)
                    .accessibility_label(title)
                    .p(px(SPACE_1))
                    .rounded(px(R_CONTROL))
                    .hover_raised(format!("switch-{id}"))
                    .focus_visible(components::control_focus)
                    .on_change(move |value, _, _, cx| {
                        cx.stop_propagation();
                        change(value, cx);
                    })
                    .child(
                        SwitchTrack::new((gpui::ElementId::from(id), "track"))
                            .checked(checked)
                            .w(px(SWITCH_W))
                            .h(px(SWITCH_H))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .border(px(SWITCH_INSET))
                            .border_color(rgba(TRANSPARENT))
                            .bg(rgb(switch_track(checked)))
                            .child(
                                SwitchThumb::new(checked)
                                    .rounded_full()
                                    .size(px(SWITCH_THUMB))
                                    .left(thumb_x)
                                    .bg(rgb(TEXT_STRONG)),
                            ),
                    ),
            )
            .into_any_element()
    })
}

thread_local! {
    /// The switches the pointer pressed last, and when: a flip that follows
    /// one within `POINTER_FLIP` is a pointer toggle, anything else is the
    /// keyboard's.
    static POINTER_PRESSES: std::cell::RefCell<std::collections::HashMap<&'static str, std::time::Instant>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// How long after a press its switch still counts as flipped by the pointer.
const POINTER_FLIP: std::time::Duration = std::time::Duration::from_millis(1_000);

fn note_pointer_press(id: &'static str, now: std::time::Instant) {
    POINTER_PRESSES.with(|presses| presses.borrow_mut().insert(id, now));
}

fn pointer_pressed(id: &'static str, now: std::time::Instant) -> bool {
    POINTER_PRESSES.with(|presses| {
        presses
            .borrow()
            .get(id)
            .is_some_and(|at| now.saturating_duration_since(*at) < POINTER_FLIP)
    })
}

/// The switch track: steel when on, two steps above the card when off.
fn switch_track(checked: bool) -> u32 {
    if checked {
        ACCENT_STRONG
    } else {
        FILL_HOVER
    }
}

/// One option of a segmented choice, inside the tray. Its 1px edge is always
/// in layout, so selection never moves a neighbour: selected is a neutral
/// `FILL` chip with a `HAIRLINE_STRONG` edge and `TEXT_STRONG`, the rest
/// `TEXT_2` on the tray, lifting to `FILL` under the pointer. `R_CHIP` is
/// the tray's `R_CONTROL` less its `FORM_CHOICE_PAD`: concentric.
pub fn chip(id: (&'static str, usize), label: SharedString, selected: bool, cx: &App) -> Button {
    let (ink, ground, edge) = components::choice_inks(selected);
    let hover = rgb(if selected { FILL_HOVER } else { FILL }).into();
    let rest = match ground {
        Some(ground) => rgb(ground).into(),
        None => rgba(TRANSPARENT).into(),
    };
    components::faded_button(id, rest, hover, rgb(FILL_HOVER).into(), rgb(ink).into(), cx)
        .tab_stop(true)
        .selected(selected)
        .toggled(selected)
        .debug_selector(move || format!("{}-{}", id.0, id.1))
        .h(px(SETTINGS_CONTROL_H - 2. * FORM_CHOICE_PAD))
        .px(px(FORM_CHIP_PAD_X + SPACE_0_5))
        .rounded(px(R_CHIP))
        .border_1()
        .border_color(rgba(edge))
        .child(components::form_label(label, ink))
}

/// A read-only fact (About): its key at the left, its value at the right
/// in Geist Mono `TEXT_2` (versions are machine text), wrapping anywhere
/// rather than cut. Searchable by both.
pub fn fact(title: &'static str, value: SharedString) -> Row {
    let words = vec![value.clone()];
    Row::new(title, "", words, move |_, _| {
        fact_value(title, value.clone()).into_any_element()
    })
    .fact()
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
            .gap(px(SPACE_1_5))
            .px(px(SPACE_1_5))
            .mr(px(-SPACE_1_5))
            .rounded(px(R_CONTROL))
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
                    if copied { TEXT_MUTED } else { TEXT_FAINT },
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
        .text_color(rgb(TEXT_2))
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
        rgba(TRANSPARENT).into(),
        rgb(HOVER).into(),
        rgb(PRESSED).into(),
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

    #[test]
    fn the_sheet_is_raised_edged_and_rounded_over_the_veil() {
        let mut drawn = sheet(SETTINGS_W, SETTINGS_H);
        let style = drawn.style();
        assert_eq!(style.background, Some(rgb(RAISED).into()));
        assert_eq!(style.border_color, Some(rgba(HAIRLINE_STRONG).into()));
        assert_eq!(style.box_shadow, Some(components::float_shadow()));
        assert_eq!(
            style.corner_radii.top_left,
            Some(px(R_PANE).into()),
            "a sheet is a Pane-sized surface"
        );
        assert_eq!(veil().style().background, Some(rgba(VEIL).into()));
    }

    /// Settings is the recipe's quieter variant: a `PANE` sheet under its
    /// `RAISED` cards, with the recipe's edge, radius and shadow.
    #[test]
    fn the_settings_sheet_sits_one_step_under_its_cards() {
        let mut drawn = settings_sheet();
        let style = drawn.style();
        assert_eq!(style.background, Some(rgb(PANE).into()));
        assert_eq!(style.border_color, Some(rgba(HAIRLINE_STRONG).into()));
        assert_eq!(style.box_shadow, Some(components::float_shadow()));
        assert_eq!(style.corner_radii.top_left, Some(px(R_PANE).into()));
        const _: () = assert!(PANE < RAISED, "the cards are raised off the sheet");
        assert_eq!(
            head_row("Settings", div()).style().border_widths.bottom,
            None,
            "the Settings head has no rule"
        );
        assert_eq!(
            sheet_head("Project", div()).style().border_widths.bottom,
            Some(px(1.).into()),
            "the scrolling sheets keep theirs"
        );
    }

    #[test]
    fn rows_keep_the_one_list_pitch_and_the_sheet_fits_its_pages() {
        assert_eq!(SETTINGS_NAV_ROW_H, NAV_ROW_H);
        assert_eq!(SETTINGS_ROW_H, 36.0);
        assert_eq!(SETTINGS_ROW_HINT_H, 44.0);
        // Checked at compile time: the rhythm and the size can never drift.
        const _: () = assert!(SETTINGS_GROUP_GAP >= 2.0 * SETTINGS_LABEL_GAP);
        const _: () = assert!(SETTINGS_CONTROL_H <= SETTINGS_ROW_H - 2.0 * SPACE_1);
        const _: () = assert!(SWITCH_H + 2.0 * SPACE_1 <= SETTINGS_ROW_H);
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

    #[test]
    fn the_switch_is_steel_when_on_and_reads_on_the_sheet_when_off() {
        assert_eq!(switch_track(true), ACCENT_STRONG);
        assert_eq!(switch_track(false), FILL_HOVER);
        assert_ne!(
            switch_track(false),
            RAISED,
            "off must not vanish on the card"
        );
        assert_ne!(switch_track(false), RAISED_2, "nor on a hovered row");
    }

    #[gpui::test]
    fn a_selected_choice_is_a_neutral_chip_and_every_choice_keeps_its_edge(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            crate::theme::init_components(cx);
            let mut on = chip(("c", 0), "Claude".into(), true, cx);
            let mut off = chip(("c", 1), "Codex".into(), false, cx);
            assert_eq!(on.style().background, Some(rgb(FILL).into()));
            assert_eq!(on.style().border_color, Some(rgba(HAIRLINE_STRONG).into()));
            assert_eq!(off.style().background, None);
            assert_eq!(off.style().border_color, Some(rgba(TRANSPARENT).into()));
            for chip in [&mut on, &mut off] {
                let widths = chip.style().border_widths.clone();
                assert_eq!(widths.left, Some(px(1.).into()));
            }
        });
    }
}
