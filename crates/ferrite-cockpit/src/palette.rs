//! The command palette (⌘K, the prototype's `#palette`): every Thread, the
//! Projects and Groups to jump to, then every command, filtered as the
//! operator types (FL-1…FL-5). Scoped, it is also ⌘G's Group list and the
//! session import picker.
//!
//! The view holds what it shows — the rows, the type tab, the filter's
//! survivors and the cursor — and draws it as the Claude app's search
//! (theme's `PALETTE_*`): the input row (the `search` icon, the query or
//! its `Search` placeholder, the status filter's name at the right while
//! one is on), ⌘K's type tabs (`All`, then each type it holds; ← → on an
//! empty line), section headings, rows (a state mark or the command's
//! icon, the name with its matches lit, the context and the chord at the
//! right, the cursor row `SELECTED`), and the footer — the keys' hints
//! (`Close esc`, `Change type ← →`, `Open beside ⇥`) and the cursor row's
//! working directory or the result count. At rest — `All`, nothing typed
//! — it lists `Recents` (the Threads used last) and a few `Actions` that
//! start work; a query or a tab reaches every row. The cockpit builds the rows
//! (`cockpit::palette`) and runs them: every key the palette answers is a
//! `palette::*` action bound in its `Palette` context, handled at the
//! cockpit's root.
//!
//! **Status filters.** A line of one letter is a filter, not a search: `b`
//! `w` `i` `d` keep only the rows whose state is blocked, working, idle or
//! done (`cockpit::Attention`; a Project or Group by its roll-up), and `a`
//! keeps them all. Commands have no state and stay. The next letter turns
//! the line into a search, so no key is ever lost to a filter: `debug`
//! finds an idle Thread named so although `d` alone keeps the done ones.

use std::ops::Range;

use ferrite_core::groups::GroupId;
use ferrite_core::roster::PaneIdentity;
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::ThreadId;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, Context, Entity, IntoElement, ScrollHandle, SharedString, StyledText,
    Subscription, Window,
};

use crate::cockpit::Attention;
use crate::nav::RowStatus;
use ferrite_pane::composer::{Composer, Edited, Role};
use ferrite_ui::components;
use ferrite_ui::pointer::{Pointer, PointerPressed};
use ferrite_ui::theme::*;

gpui::actions!(
    palette,
    [
        // ⌘K: the palette, open or shut.
        Toggle,
        // ⌘G: the palette scoped to Groups.
        OpenGroups,
        // ⌘⇧P / ⌘⇧D: the palette's two keyed commands.
        ShowParked,
        ReviewBranch,
        // Its own keys, in the `Palette` context.
        SelectNext,
        SelectPrevious,
        Confirm,
        Preview,
        Dismiss,
        // ← / → on an empty line: the next or previous type tab.
        NextType,
        PreviousType,
    ]
);

/// What the palette lists: everything (⌘K), the Groups alone (⌘G), or the
/// CLI sessions there are to import (`import a session`, `/import`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaletteScope {
    All,
    Groups,
    Sessions,
}

/// A row's section, in the order the palette draws them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Threads,
    Projects,
    Groups,
    Commands,
    Sessions,
}

impl Section {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Section::Threads => "Threads",
            Section::Projects => "Projects",
            Section::Groups => "Groups",
            Section::Commands => "Actions",
            Section::Sessions => "Sessions",
        }
    }
}

/// The ⌘K palette's type tabs: `All`, or one section alone. A tab shows
/// only where the palette holds rows of its section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tab {
    All,
    Only(Section),
}

impl Tab {
    fn title(self) -> &'static str {
        match self {
            Tab::All => "All",
            Tab::Only(section) => section.title(),
        }
    }

    fn keeps(self, section: Section) -> bool {
        match self {
            Tab::All => true,
            Tab::Only(only) => only == section,
        }
    }
}

/// How many Threads `All` lists as `Recents` before a query.
pub(crate) const RECENTS_MAX: usize = 8;

/// The status filter's name, at the input row's right while it is on.
fn filter_word(filter: Attention) -> &'static str {
    use ferrite_ui::theme::words;
    match filter {
        Attention::Blocked => words::BLOCKED,
        Attention::Working => words::WORKING,
        Attention::Idle => words::IDLE,
        Attention::Done => words::DONE,
    }
}

/// Every command the palette offers (FL-4), in its table's order. Each runs
/// what its key or its old chrome ran; a row hides while its act is
/// unavailable (`cockpit::palette::command_rows`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    ParkThread,
    ShowParked,
    ReviewBranch,
    ReviewChanges,
    NewThread,
    NewWorktreeThread,
    NewThreadInGroup,
    NewGroup,
    OpenGroup,
    ReopenLast,
    NewThreadIn(ProjectId),
    FilterAll,
    FilterProject(ProjectId),
    SortCreated,
    SortRecent,
    SortAttention,
    NewProject,
    EditProject(ProjectId),
    Settings,
    ToggleSidebar,
    TogglePanel,
    FullscreenPane,
    TextLarger,
    TextSmaller,
    TextReset,
    ShowThinking,
    HideThinking,
    Notifications,
    Shortcuts,
    NextRequest,
    CopyPrompt,
    ResendPrompt,
    Model,
    Effort,
    PermissionMode,
    RefreshMcp,
    ReconnectMcp,
    BackgroundTasks,
    ShowChanges,
    ShowChecks,
    RefreshPr,
    OpenPr,
    ShowPlan,
    UpdateClis,
    UpdateFerrite,
    DiscardDraft,
    ImportSession,
    ResetKeys,
    /// The operator's own command, by its place in Settings `commands`.
    User(usize),
}

impl Command {
    /// The few acts `All` offers as `Actions` before a query (the Claude
    /// app's `New session`): starting work. The rest wait for a query or
    /// the `Actions` tab.
    fn primary(&self) -> bool {
        matches!(
            self,
            Command::NewThread | Command::NewWorktreeThread | Command::ImportSession
        )
    }

    /// The icon in the row's mark column, where the icon family has one
    /// that says the act (`command-search.png`: `plus`, `branch`, `board`);
    /// the rest hold the column empty.
    fn icon(&self) -> Option<&'static str> {
        use ferrite_ui::icons::*;
        Some(match self {
            Command::NewThread
            | Command::NewThreadIn(_)
            | Command::NewThreadInGroup
            | Command::NewGroup
            | Command::NewProject => PLUS,
            Command::NewWorktreeThread | Command::ReviewBranch => BRANCH,
            Command::OpenGroup => BOARD,
            Command::ParkThread | Command::ShowParked => ARCHIVE,
            Command::Settings => GEAR,
            Command::ToggleSidebar => SIDEBAR,
            Command::TogglePanel => PANEL_RIGHT,
            Command::FullscreenPane => MAXIMIZE,
            Command::Notifications => BELL,
            Command::CopyPrompt => COPY,
            Command::ResendPrompt => RESEND,
            Command::PermissionMode => SHIELD,
            Command::ShowChanges | Command::ReviewChanges => FILE,
            Command::RefreshPr | Command::RefreshMcp | Command::ReconnectMcp => REFRESH,
            Command::OpenPr => EXTERNAL,
            _ => return None,
        })
    }
}

/// What a row does when it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    /// Focus the Thread, or revive it; ⇥ opens it beside the current Pane.
    Thread(ThreadId),
    /// Enter the Group.
    Group(GroupId),
    /// Land on the Project's Thread that most needs the operator.
    Project(ProjectId),
    Command(Command),
    /// Import the session file and resume it: in place of `into` (a blank
    /// Pane the picker was opened for), else alongside.
    Adopt {
        path: std::path::PathBuf,
        into: Option<PaneIdentity>,
    },
}

/// One palette row, ready to draw and to run.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    pub section: Section,
    pub name: SharedString,
    /// The muted words after the name: a Thread's `perf sweep · working`,
    /// a command's description. Part of what the query matches.
    pub context: SharedString,
    /// A Thread's state, or a Project's or Group's roll-up: its still mark
    /// (`RowStatus::face`) and what the status filters read. `None` holds
    /// the 2-cell column empty and passes every filter.
    pub status: Option<RowStatus>,
    /// The footer's line while the cursor is on the row: a Thread's working
    /// directory.
    pub detail: Option<SharedString>,
    /// The key that does the same, as the key table spells it (`cmd-1`).
    pub keys: Option<String>,
    /// When the operator last used it (a Thread): `All`'s `Recents` order.
    pub used: Option<std::time::SystemTime>,
    pub act: Act,
}

impl Row {
    /// Whether the status filter keeps the row: a row with no state always.
    fn passes(&self, filter: Option<Attention>) -> bool {
        match (filter, self.status) {
            (Some(filter), Some(status)) => status.attention() == filter,
            _ => true,
        }
    }
}

/// A surviving row: its index into the rows, and the name's lit ranges.
#[derive(Clone, Debug, PartialEq)]
struct Shown {
    row: usize,
    lit: Vec<Range<usize>>,
}

/// The palette: its scope, its rows and the filter's survivors, the cursor
/// over them, and the input line.
pub(crate) struct Palette {
    pub scope: PaletteScope,
    /// The type tab (⌘K only; the other scopes are one type already).
    pub tab: Tab,
    rows: Vec<Row>,
    shown: Vec<Shown>,
    cursor: usize,
    /// What the list says while it holds no rows at all — a list the query
    /// emptied says `no matches`.
    empty: SharedString,
    pub input: Entity<Composer>,
    scroll: ScrollHandle,
    _edited: Subscription,
}

impl Palette {
    /// A palette over `rows` (already in the order it draws them), its line
    /// seeded with `query`.
    pub(crate) fn new(
        scope: PaletteScope,
        rows: Vec<Row>,
        query: &str,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = query.to_string();
        let input = cx.new(|cx| {
            let mut line = Composer::new(cx);
            line.set_role(Role::Field(Some("Palette")), cx);
            line.set_visible_row_limit(1, cx);
            if !query.is_empty() {
                line.set(query, cx);
            }
            line
        });
        let edited = cx.subscribe(&input, |palette: &mut Self, _, _: &Edited, cx| {
            palette.refilter(cx);
        });
        let mut palette = Self {
            scope,
            tab: Tab::All,
            rows,
            shown: Vec::new(),
            cursor: 0,
            empty: NO_MATCHES.into(),
            input,
            scroll: ScrollHandle::new(),
            _edited: edited,
        };
        palette.refilter(cx);
        palette
    }

    /// Replace the rows (a list that arrives after the palette opened) and
    /// what an empty list says; the query stands and filters them.
    pub(crate) fn set_rows(&mut self, rows: Vec<Row>, empty: SharedString, cx: &mut Context<Self>) {
        self.rows = rows;
        self.empty = empty;
        self.refilter(cx);
    }

    /// What the line holds now.
    pub(crate) fn query(&self, cx: &gpui::App) -> String {
        self.input.read(cx).text().to_string()
    }

    /// Filter the rows by the line (`fuzzy::palette_match`, or a status
    /// filter's letter): survivors keep their order, never re-ranked; the
    /// cursor returns to the first.
    pub(crate) fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx);
        self.shown = filter(&self.rows, &query, self.scope, self.tab);
        self.cursor = 0;
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    /// The type tabs offered: `All`, then each section the rows hold, in
    /// draw order. None outside ⌘K.
    pub(crate) fn tabs(&self) -> Vec<Tab> {
        if self.scope != PaletteScope::All {
            return Vec::new();
        }
        let mut tabs = vec![Tab::All];
        for row in &self.rows {
            let tab = Tab::Only(row.section);
            if !tabs.contains(&tab) {
                tabs.push(tab);
            }
        }
        tabs
    }

    /// Show `tab`'s rows; the query stands and filters them.
    pub(crate) fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if tab != self.tab {
            self.tab = tab;
            self.refilter(cx);
        }
    }

    /// ← / →: the next or previous tab, wrapping. With text on the line
    /// the arrows move the caret instead, as a field's do.
    fn step_tab(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let tabs = self.tabs();
        if tabs.is_empty() || !self.query(cx).is_empty() {
            let caret: Box<dyn gpui::Action> = if delta < 0 {
                Box::new(ferrite_pane::composer::Left)
            } else {
                Box::new(ferrite_pane::composer::Right)
            };
            window.dispatch_action(caret, cx);
            return;
        }
        let at = tabs.iter().position(|tab| *tab == self.tab).unwrap_or(0) as isize;
        let next = tabs[(at + delta).rem_euclid(tabs.len() as isize) as usize];
        self.set_tab(next, cx);
    }

    /// ↑ / ↓: the cursor steps over the survivors, wrapping.
    pub(crate) fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.shown.is_empty() {
            return;
        }
        let count = self.shown.len() as isize;
        self.cursor = (self.cursor as isize + delta).rem_euclid(count) as usize;
        self.scroll.scroll_to_item(self.element_index(self.cursor));
        cx.notify();
    }

    /// Put the cursor on survivor `at` (a click).
    pub(crate) fn set_cursor(&mut self, at: usize, cx: &mut Context<Self>) {
        if at < self.shown.len() && at != self.cursor {
            self.cursor = at;
            cx.notify();
        }
    }

    /// The row under the cursor, if anything survives.
    pub(crate) fn selected(&self) -> Option<&Row> {
        self.shown
            .get(self.cursor)
            .map(|shown| &self.rows[shown.row])
    }

    /// The surviving rows, in order — what the tests read.
    #[cfg(test)]
    pub(crate) fn survivors(&self) -> Vec<&Row> {
        self.shown
            .iter()
            .map(|shown| &self.rows[shown.row])
            .collect()
    }

    /// What the list says while it holds no rows — what the tests read.
    #[cfg(test)]
    pub(crate) fn empty_note(&self) -> &str {
        &self.empty
    }

    /// The lit ranges of survivor `at`'s name — what the tests read.
    #[cfg(test)]
    pub(crate) fn lit(&self, at: usize) -> Option<&[Range<usize>]> {
        self.shown.get(at).map(|shown| shown.lit.as_slice())
    }

    /// Where survivor `at` stands among the list's children: each section
    /// row counts too.
    fn element_index(&self, at: usize) -> usize {
        let mut index = 0;
        let mut section = None;
        for (position, shown) in self.shown.iter().enumerate() {
            let row_section = self.rows[shown.row].section;
            if section != Some(row_section) {
                section = Some(row_section);
                index += 1;
            }
            if position == at {
                return index;
            }
            index += 1;
        }
        index
    }
}

/// The status filter a one-letter line asks for: `Some(None)` for `a`,
/// every state. Any other line is a search, and so is every line in the
/// import picker, whose sessions have no state.
fn status_filter(scope: PaletteScope, query: &str) -> Option<Option<Attention>> {
    if scope == PaletteScope::Sessions {
        return None;
    }
    match query {
        "b" => Some(Some(Attention::Blocked)),
        "w" => Some(Some(Attention::Working)),
        "i" => Some(Some(Attention::Idle)),
        "d" => Some(Some(Attention::Done)),
        "a" => Some(None),
        _ => None,
    }
}

/// Whether the palette shows its resting `All` view: ⌘K, the `All` tab,
/// nothing typed — `Recents` and a few `Actions`, as the Claude app's
/// search opens.
fn resting(scope: PaletteScope, tab: Tab, query: &str) -> bool {
    scope == PaletteScope::All && tab == Tab::All && query.is_empty()
}

/// The rows the line keeps — the tab's, then a status filter's or the
/// search's — with their names' lit ranges. At rest `All` keeps the
/// `RECENTS_MAX` Threads used last, newest first, then the primary
/// commands.
fn filter(rows: &[Row], query: &str, scope: PaletteScope, tab: Tab) -> Vec<Shown> {
    let whole = |index: usize| Shown {
        row: index,
        lit: Vec::new(),
    };
    if resting(scope, tab, query) {
        let mut recents: Vec<usize> = (0..rows.len())
            .filter(|index| rows[*index].section == Section::Threads)
            .collect();
        // Stable: Threads never used keep their tree order, last.
        recents.sort_by_key(|index| std::cmp::Reverse(rows[*index].used));
        recents.truncate(RECENTS_MAX);
        let actions = (0..rows.len()).filter(
            |index| matches!(&rows[*index].act, Act::Command(command) if command.primary()),
        );
        return recents.into_iter().chain(actions).map(whole).collect();
    }
    let (query, status) = match status_filter(scope, query) {
        Some(status) => ("", status),
        None => (query, None),
    };
    rows.iter()
        .enumerate()
        .filter(|(_, row)| tab.keeps(row.section))
        .filter(|(_, row)| row.passes(status))
        .filter_map(|(index, row)| {
            crate::fuzzy::palette_match(query, &row.name, &row.context)
                .map(|lit| Shown { row: index, lit })
        })
        .collect()
}

pub(crate) const NO_MATCHES: &str = "No matches";

/// The footer's right: how many rows survive.
fn results_word(count: usize) -> String {
    match count {
        1 => "1 result".to_string(),
        count => format!("{count} results"),
    }
}

impl Render for Palette {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.query(cx);
        let rest = resting(self.scope, self.tab, &query);
        let tabs = self.tabs();
        // The tabs carry the list's hairline when there are any.
        let mut input = input_row(
            "palette-input",
            self.input.clone(),
            query.is_empty().then_some("Search"),
            tabs.is_empty(),
        );
        if let Some(filter) = status_filter(self.scope, &query).flatten() {
            input = input.child(
                div()
                    .debug_selector(|| "palette-filter".into())
                    .flex_shrink_0()
                    .text_size(px(FS_META))
                    .whitespace_nowrap()
                    .text_color(rgb(filter_ink(filter)))
                    .child(filter_word(filter)),
            );
        }
        let tab_row = (!tabs.is_empty()).then(|| {
            tabs.iter().enumerate().fold(
                div()
                    .debug_selector(|| "palette-tabs".into())
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(PALETTE_TAB_GAP))
                    .h(px(PALETTE_TABS_H + 1.0))
                    .px(px(PALETTE_PAD_X))
                    // The pills hang at the row's top, the air under them.
                    .pb(px(PALETTE_TABS_H - PALETTE_TAB_H))
                    .border_b_1()
                    .border_color(paint::FLOAT_EDGE),
                |row, (at, tab)| {
                    let tab = *tab;
                    row.child(self.tab_element(at, tab, cx))
                },
            )
        });
        let mut list = div()
            .id("palette-rows")
            .flex()
            .flex_col()
            .min_h_0()
            .max_h(px(PALETTE_MAX_H))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p(px(PALETTE_LIST_PAD));
        let mut section = None;
        for (at, shown) in self.shown.iter().enumerate() {
            let row = &self.rows[shown.row];
            if section != Some(row.section) {
                let first = section.is_none();
                section = Some(row.section);
                // At rest the Threads are the ones used last.
                let title = match row.section {
                    Section::Threads if rest => "Recents",
                    section => section.title(),
                };
                list = list.child(
                    section_heading(title, first)
                        .debug_selector(move || format!("palette-section-{at}")),
                );
            }
            list = list.child(
                row_element(at, row, &shown.lit, at == self.cursor).on_click(cx.listener(
                    move |palette, _: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        palette.set_cursor(at, cx);
                        window.dispatch_action(Box::new(Confirm), cx);
                    },
                )),
            );
        }
        if self.shown.is_empty() {
            let note = if self.rows.is_empty() {
                self.empty.clone()
            } else {
                NO_MATCHES.into()
            };
            list = list.child(empty_note(note));
        }
        // The footer's hints: what each key does here, as the Claude
        // app's search says them; the cursor row's directory at the right.
        let mut hints: Vec<(&'static str, &'static str)> = vec![("Close", "esc")];
        if !tabs.is_empty() {
            hints.push(("Change type", "\u{2190} \u{2192}"));
        }
        match self.scope {
            PaletteScope::All => hints.push(("Open beside", "\u{21e5}")),
            PaletteScope::Groups => {}
            PaletteScope::Sessions => hints.push(("Import", "\u{23ce}")),
        }
        let detail = self
            .selected()
            .and_then(|row| row.detail.clone())
            .unwrap_or_else(|| SharedString::from(results_word(self.shown.len())));
        // It comes in as every sheet does (`dialog-in`), over its veil
        // darkening in (`cockpit::palette`); it closes at once.
        ferrite_ui::motion::dialog_in_casting(
            "palette-in",
            dialog()
                .debug_selector(|| "palette".into())
                // A press inside never reaches the veil or the board under it.
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_action(
                    cx.listener(|palette, _: &NextType, window, cx| {
                        palette.step_tab(1, window, cx)
                    }),
                )
                .on_action(cx.listener(|palette, _: &PreviousType, window, cx| {
                    palette.step_tab(-1, window, cx)
                }))
                .child(input)
                .children(tab_row)
                .child(list)
                .child(footer_row(hint_line(&hints), detail)),
            components::dialog_shadow,
        )
    }
}

impl Palette {
    /// One type tab: its word on a `PALETTE_TAB_H` pill, the chosen one
    /// `SELECTED`; a press picks it.
    fn tab_element(
        &self,
        at: usize,
        tab: Tab,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let chosen = tab == self.tab;
        let id = gpui::ElementId::from(("palette-tab", at));
        let key = ferrite_ui::pointer::hover_key(&id);
        let pill = div()
            .id(id)
            .debug_selector(move || format!("palette-tab-{at}"))
            .flex()
            .items_center()
            .h(px(PALETTE_TAB_H))
            .px(px(PALETTE_TAB_PAD_X))
            .rounded(px(R_CONTROL))
            .whitespace_nowrap()
            .text_color(rgb(if chosen { INK_STRONG } else { INK_SUBTLE }))
            .when(chosen, |pill| pill.font_weight(W_LABEL))
            .child(tab.title())
            .on_click(cx.listener(move |palette, _: &gpui::ClickEvent, _, cx| {
                cx.stop_propagation();
                palette.set_tab(tab, cx);
            }));
        if chosen {
            pill.bg(paint::FLOAT_SEL)
        } else {
            pill.hover_palette(key).press_float()
        }
    }
}

/// The footer's hints: each word in `INK_SUBTLE`, its key after it on a
/// cap (`components::key_cap`, `INK_STRONG` edged `FLOAT_EDGE`, Claude's),
/// `PALETTE_HINT_GAP` apart.
fn hint_line(hints: &[(&'static str, &'static str)]) -> gpui::Div {
    hints.iter().fold(
        div()
            .debug_selector(|| "palette-hints".into())
            .flex()
            .items_center()
            .gap(px(PALETTE_HINT_GAP)),
        |line, (word, key)| {
            line.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .child(*word)
                    .child(components::key_cap(
                        *key,
                        rgb(INK_STRONG),
                        paint::FLOAT_EDGE,
                    )),
            )
        },
    )
}

/// What a dialog's list says when it has nothing to show (the palette's,
/// the shortcuts sheet's): its note, `INK_SUBTLE`, centred in
/// `PALETTE_EMPTY_H`.
pub(crate) fn empty_note(note: impl Into<SharedString>) -> gpui::Div {
    div()
        .debug_selector(|| "palette-empty".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(px(PALETTE_EMPTY_H))
        .text_color(rgb(INK_SUBTLE))
        .child(note.into())
}

/// The status filter's word in its state's ink: hue for the two states
/// that have one (needs you, working), `INK_SUBTLE` for idle and done (done
/// is a shape, not a hue).
fn filter_ink(filter: Attention) -> u32 {
    match filter {
        Attention::Blocked => ATTENTION,
        Attention::Working => RUNNING,
        Attention::Idle | Attention::Done => INK_SUBTLE,
    }
}

/// The dialog's shell (SPEC §5.8): `PALETTE_W` on `paint::FLOAT`,
/// radius `R_FLOAT`, clipped to it, the 1px `FLOAT_EDGE` ring drawn
/// *outside* the box (so it keeps its full `PALETTE_W` inside) and the deep shadow
/// under it (`components::dialog_shadow`). The
/// shortcuts sheet wears it too.
pub(crate) fn dialog() -> gpui::Div {
    components::text_ui()
        .cursor_default()
        .occlude()
        .flex()
        .flex_col()
        .w(px(PALETTE_W))
        .overflow_hidden()
        .rounded(px(R_FLOAT))
        .bg(paint::FLOAT)
        .shadow(components::dialog_shadow())
}

/// The dialog's input row (the palette's, the shortcuts sheet's filter):
/// `PALETTE_INPUT_H`, the `search` icon in `INK_SUBTLE`, then the line in
/// `FS_UI` `INK_STRONG`, its `placeholder` in `PLACEHOLDER_INK` while it is
/// empty; over a `FLOAT_EDGE` rule when `rule` (⌘K's tabs carry it instead). The
/// caller adds what stands at its right.
pub(crate) fn input_row(
    selector: &'static str,
    input: Entity<Composer>,
    placeholder: Option<&'static str>,
    rule: bool,
) -> gpui::Div {
    div()
        .debug_selector(move || selector.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(PALETTE_GAP))
        .px(px(PALETTE_PAD_X))
        // The row, then its hairline under it (gpui's border sits inside
        // the box).
        .map(|row| {
            if rule {
                row.h(px(PALETTE_INPUT_H + 1.0))
                    .border_b_1()
                    .border_color(paint::FLOAT_EDGE)
            } else {
                row.h(px(PALETTE_INPUT_H))
            }
        })
        .child(ferrite_ui::icons::icon(
            ferrite_ui::icons::SEARCH,
            PALETTE_SEARCH_ICON,
            INK_SUBTLE,
        ))
        .child(
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .font_family(FONT_UI)
                .text_size(px(FS_UI))
                .line_height(px(LH_UI))
                .text_color(rgb(INK_STRONG))
                // Behind the line, so the caret stands at its start.
                .children(placeholder.map(|words| {
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .text_color(rgb(PLACEHOLDER_INK))
                        .child(words)
                }))
                .child(input),
        )
}

/// A section heading: its word in 12/500 `INK_SUBTLE`, set on the heading's
/// foot; a heading after the first stands `PALETTE_SECTION_GAP` taller.
pub(crate) fn section_heading(title: &'static str, first: bool) -> gpui::Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_end()
        .h(px(PALETTE_SECTION_H))
        .when(!first, |heading| heading.mt(px(PALETTE_SECTION_GAP)))
        .px(px(PALETTE_ROW_PAD_X))
        .pb(px(PALETTE_SECTION_PAD_B))
        .text_size(px(FS_META))
        .font_weight(W_LABEL)
        .text_color(rgb(INK_SUBTLE))
        .whitespace_nowrap()
        .child(title)
}

/// The footer: `PALETTE_FOOT_H` under a `FLOAT_EDGE` rule, the key hints at the
/// left, and at the right the cursor row's directory or the result count,
/// cut from its start where it runs long; all 12 `INK_SUBTLE`.
pub(crate) fn footer_row(left: impl IntoElement, right: SharedString) -> gpui::Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(SPACE_4))
        // Its hairline over it, then the row.
        .h(px(PALETTE_FOOT_H + 1.0))
        .px(px(PALETTE_PAD_X))
        .border_t_1()
        .border_color(paint::FLOAT_EDGE)
        .text_size(px(FS_META))
        .text_color(rgb(INK_SUBTLE))
        .whitespace_nowrap()
        .child(div().flex_shrink_0().child(left))
        .child(
            div()
                .debug_selector(|| "palette-results".into())
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis_start()
                .text_right()
                .child(right),
        )
}

/// A row's mark: a Thread's (or a Project's or Group's roll-up) state
/// mark, else the command's icon, else nothing.
fn mark(row: &Row) -> Option<gpui::AnyElement> {
    match (row.status, &row.act) {
        (Some(status), _) => Some(crate::nav::mark(status, false)),
        (None, Act::Command(command)) => command.icon().map(|icon| {
            ferrite_ui::icons::icon(icon, PALETTE_MARK_ICON, INK_STRONG).into_any_element()
        }),
        _ => None,
    }
}

/// One palette row (SPEC §5.8): `PALETTE_ROW_H`, radius `R_CONTROL`, its
/// parts `PALETTE_GAP` apart — the mark column, the name (`INK_STRONG`, its
/// matches 600), the context at the right in 12 `INK_SUBTLE`,
/// then the key column, `PALETTE_KEYS_W` on every row so the contexts end
/// on one edge: the chord as plain 12 `INK_SUBTLE` text, or, on a chordless
/// cursor row (`SELECTED`, its name all `INK_STRONG`), `corner-down-left`.
fn row_element(
    at: usize,
    row: &Row,
    lit: &[Range<usize>],
    cursor: bool,
) -> gpui::Stateful<gpui::Div> {
    let id = gpui::ElementId::from(("palette-row", at));
    let key = ferrite_ui::pointer::hover_key(&id);
    let name =
        StyledText::new(row.name.clone()).with_highlights(components::match_highlights(lit, false));
    let meta = || div().text_size(px(FS_META)).text_color(rgb(INK_SUBTLE));
    let element = div()
        .id(id)
        .debug_selector(move || format!("palette-row-{at}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(PALETTE_GAP))
        .min_w_0()
        .h(px(PALETTE_ROW_H))
        .px(px(PALETTE_ROW_PAD_X))
        .rounded(px(R_CONTROL))
        .whitespace_nowrap()
        .text_color(rgb(INK_STRONG))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .w(px(PALETTE_MARK_W))
                .children(mark(row)),
        )
        .child(div().flex_1().min_w_0().truncate().child(name))
        .when(!row.context.is_empty(), |element| {
            element.child(meta().min_w_0().truncate().child(row.context.clone()))
        })
        .child(
            meta()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_end()
                .min_w(px(PALETTE_KEYS_W))
                .map(|cell| match row.keys.clone() {
                    Some(keys) => cell.child(
                        components::key_combo(&keys, INK_SUBTLE)
                            .font_family(FONT_UI)
                            .text_size(px(FS_META)),
                    ),
                    None if cursor => cell.child(ferrite_ui::icons::icon(
                        ferrite_ui::icons::CORNER_DOWN_LEFT,
                        PALETTE_ENTER_ICON,
                        INK_SUBTLE,
                    )),
                    None => cell,
                }),
        );
    if cursor {
        element.palette_cursor().press_float()
    } else {
        element.hover_palette(key).press_float()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(name: &str, context: &str, id: u64) -> Row {
        state_thread(name, context, id, RowStatus::Working)
    }

    fn state_thread(name: &str, context: &str, id: u64, status: RowStatus) -> Row {
        Row {
            section: Section::Threads,
            name: name.to_string().into(),
            context: context.to_string().into(),
            status: Some(status),
            detail: None,
            keys: None,
            used: None,
            act: Act::Thread(ThreadId::new(id)),
        }
    }

    fn command(name: &str, context: &str, command: Command) -> Row {
        Row {
            section: Section::Commands,
            name: name.to_string().into(),
            context: context.to_string().into(),
            status: None,
            detail: None,
            keys: None,
            used: None,
            act: Act::Command(command),
        }
    }

    fn session(name: &str, context: &str) -> Row {
        Row {
            section: Section::Sessions,
            name: name.to_string().into(),
            context: context.to_string().into(),
            status: None,
            detail: None,
            keys: None,
            used: None,
            act: Act::Adopt {
                path: name.into(),
                into: None,
            },
        }
    }

    fn names<'a>(rows: &'a [Row], shown: &[Shown]) -> Vec<&'a str> {
        shown
            .iter()
            .map(|shown| rows[shown.row].name.as_ref())
            .collect()
    }

    /// The filter keeps the table's order and lights only the names.
    #[test]
    fn par_keeps_the_prototypes_rows_in_order() {
        let rows = vec![
            thread("Nav rows jitter on stream start", "ferrite \u{b7} done", 1),
            thread("Perf: layout cache", "perf sweep \u{b7} working", 2),
            thread("Close stale issues", "perf sweep \u{b7} question", 3),
            thread("Parked \u{b7} Release 0.5.0 notes", "ferrite \u{b7} 2h", 4),
            thread("Port onboarding flow", "zeron \u{b7} working", 5),
            command("park thread", "", Command::ParkThread),
            command("show parked", "", Command::ShowParked),
            command(
                "review branch",
                "against the default branch",
                Command::ReviewBranch,
            ),
            command("open a group", "", Command::OpenGroup),
        ];
        let shown = filter(&rows, "par", PaletteScope::All, Tab::All);
        assert_eq!(
            names(&rows, &shown),
            [
                "Perf: layout cache",
                "Parked \u{b7} Release 0.5.0 notes",
                "Port onboarding flow",
                "park thread",
                "show parked",
            ]
        );
        // Under a tab the same query keeps that type alone.
        let commands = filter(
            &rows,
            "par",
            PaletteScope::All,
            Tab::Only(Section::Commands),
        );
        assert_eq!(names(&rows, &commands), ["park thread", "show parked"]);
    }

    /// At rest (`All`, nothing typed) the palette opens as the Claude app's
    /// search does: the Threads used last as `Recents`, newest first and at
    /// most `RECENTS_MAX`, then the few commands that start work. Every
    /// other row waits for a query or its tab.
    #[test]
    fn at_rest_all_lists_recents_then_actions() {
        let used = |thread: &mut Row, ago: u64| {
            thread.used = Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000 - ago));
        };
        let mut rows: Vec<Row> = (1..=10)
            .map(|id| thread(&format!("thread {id}"), "", id))
            .collect();
        used(&mut rows[9], 1);
        used(&mut rows[0], 5);
        rows.push(command("park thread", "", Command::ParkThread));
        rows.push(command("new thread", "", Command::NewThread));
        let shown = names(&rows, &filter(&rows, "", PaletteScope::All, Tab::All));
        assert_eq!(shown.len(), RECENTS_MAX + 1);
        assert_eq!(&shown[..2], ["thread 10", "thread 1"], "newest first");
        assert_eq!(shown.last(), Some(&"new thread"), "only what starts work");
        let threads = filter(&rows, "", PaletteScope::All, Tab::Only(Section::Threads));
        assert_eq!(threads.len(), 10, "a tab lists its type whole");
    }

    /// A one-letter line keeps the rows in its one of the four states —
    /// failed reads as idle, background work as working — and every
    /// command, which has no state; `a` keeps every row.
    #[test]
    fn a_status_letter_keeps_its_states_rows_and_every_command() {
        let rows = vec![
            state_thread("waits", "", 1, RowStatus::NeedsYou),
            state_thread("broke", "", 2, RowStatus::Failed),
            state_thread("runs", "", 3, RowStatus::Working),
            state_thread("behind", "", 4, RowStatus::Background),
            state_thread("finished", "", 5, RowStatus::Done),
            state_thread("quiet", "", 6, RowStatus::Idle),
            state_thread("shelved", "", 7, RowStatus::Parked),
            command("settings", "", Command::Settings),
        ];
        let kept = |query| names(&rows, &filter(&rows, query, PaletteScope::All, Tab::All));
        assert_eq!(kept("b"), ["waits", "settings"]);
        assert_eq!(kept("w"), ["runs", "behind", "settings"]);
        assert_eq!(kept("d"), ["finished", "settings"]);
        assert_eq!(kept("i"), ["broke", "quiet", "shelved", "settings"]);
        assert_eq!(kept("a").len(), rows.len());
    }

    /// The letter after a status letter makes the line a search, whatever
    /// the state: no name is hidden by the filter its first letter names.
    #[test]
    fn a_second_letter_searches_every_state() {
        let rows = vec![
            state_thread("debug crash", "", 1, RowStatus::Idle),
            state_thread("wire up", "", 2, RowStatus::Done),
        ];
        let kept = |query| names(&rows, &filter(&rows, query, PaletteScope::All, Tab::All));
        assert_eq!(kept("d"), ["wire up"]);
        assert_eq!(kept("de"), ["debug crash"]);
        assert_eq!(kept("wi"), ["wire up"]);
    }

    /// In the import picker a status letter is a search: a session has no
    /// state for it to filter by.
    #[test]
    fn a_status_letter_searches_the_import_picker() {
        let rows = vec![
            session("deploy script", "ferrite"),
            session("fix the nav", "zeron"),
        ];
        let shown = filter(&rows, "d", PaletteScope::Sessions, Tab::All);
        assert_eq!(names(&rows, &shown), ["deploy script"]);
    }

    /// The footer counts what survives, in the singular for one.
    #[test]
    fn the_footer_counts_the_results() {
        assert_eq!(results_word(0), "0 results");
        assert_eq!(results_word(1), "1 result");
        assert_eq!(results_word(5), "5 results");
    }

    /// The headings are sentence case, commands are `Actions` (the Claude
    /// app's word), and the mockup's commands lead with its icons.
    #[test]
    fn headings_and_command_icons_follow_the_mockup() {
        assert_eq!(Section::Threads.title(), "Threads");
        assert_eq!(Section::Commands.title(), "Actions");
        assert_eq!(Command::NewThread.icon(), Some(ferrite_ui::icons::PLUS));
        assert_eq!(
            Command::NewWorktreeThread.icon(),
            Some(ferrite_ui::icons::BRANCH)
        );
        assert_eq!(Command::OpenGroup.icon(), Some(ferrite_ui::icons::BOARD));
        assert_eq!(Command::SortCreated.icon(), None);
    }
}
