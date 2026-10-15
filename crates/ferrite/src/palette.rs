//! The command palette (⌘K, the prototype's `#palette`): every Thread, the
//! Projects and Groups to jump to, then every command, filtered as the
//! operator types (FL-1…FL-5). Scoped, it is also ⌘G's Group list and the
//! session import picker.
//!
//! The view holds what it shows — the rows, the filter's survivors and the
//! cursor — and draws it in the float grammar: the input row (`❯`, the query
//! in `TEXT_STRONG`, the block caret, the status filter's name at the
//! right), section rows (`threads`, `projects`, `groups`, `commands`), rows
//! (the cursor's `❯` on its `FLOAT_SEL` bar, a still state mark, the name
//! with its matches in the accent at `W_STRONG`, the context muted after
//! it, the key at the right), and the footer — the cursor row's detail (a
//! Thread's working directory) where it has one, else the keys. The
//! cockpit builds the rows (`cockpit::palette`) and runs them: every key
//! the palette answers is a `palette::*` action bound in its `Palette`
//! context, handled at the cockpit's root.
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
use crate::components;
use crate::composer::{Composer, Edited, Role};
use crate::nav::RowStatus;
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::*;

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
            Section::Threads => "threads",
            Section::Projects => "projects",
            Section::Groups => "groups",
            Section::Commands => "commands",
            Section::Sessions => "sessions",
        }
    }
}

/// The status filter's name, at the input row's right while it is on.
fn filter_word(filter: Attention) -> &'static str {
    use crate::theme::words;
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
    /// Start the Project's coordinator, or land on it (ADR 0028).
    Coordinate(ProjectId),
    FilterAll,
    FilterProject(ProjectId),
    SortCreated,
    SortRecent,
    SortAttention,
    NewProject,
    EditProject(ProjectId),
    Settings,
    ToggleSidebar,
    FullscreenPane,
    TextLarger,
    TextSmaller,
    TextReset,
    ShowThinking,
    HideThinking,
    SmartFilteringOn,
    SmartFilteringOff,
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
    /// One of Projects' commands (ADR 0030).
    Projects(crate::projects::Command),
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
    pub act: Act,
}

impl Row {
    /// How far after the name its context stands: four cells for a Thread
    /// (and a Project or Group), two for a command's description.
    fn context_gap(&self) -> f32 {
        match self.section {
            Section::Commands => FLOAT_DETAIL_GAP,
            Section::Threads | Section::Projects | Section::Groups | Section::Sessions => {
                PALETTE_CONTEXT_GAP
            }
        }
    }

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
        self.shown = filter(&self.rows, &query, self.scope);
        self.cursor = 0;
        self.scroll.scroll_to_item(0);
        cx.notify();
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

/// The rows the line keeps — a status filter's, or the search's — with
/// their names' lit ranges.
fn filter(rows: &[Row], query: &str, scope: PaletteScope) -> Vec<Shown> {
    let (query, status) = match status_filter(scope, query) {
        Some(status) => ("", status),
        None => (query, None),
    };
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.passes(status))
        .filter_map(|(index, row)| {
            crate::fuzzy::palette_match(query, &row.name, &row.context)
                .map(|lit| Shown { row: index, lit })
        })
        .collect()
}

/// The palette's footer, word for word (the prototype's `.ffoot`).
pub(crate) const FOOTER: &str =
    "\u{2191}\u{2193} select \u{b7} \u{23ce} open \u{b7} \u{21e5} preview in a pane \u{b7} esc";
/// ⌘G's footer: a Group has no pane to preview in.
pub(crate) const GROUPS_FOOTER: &str = "\u{2191}\u{2193} select \u{b7} \u{23ce} open \u{b7} esc";
/// The import picker's footer: ⏎ imports the session and resumes it.
pub(crate) const SESSIONS_FOOTER: &str =
    "\u{2191}\u{2193} select \u{b7} \u{23ce} import \u{b7} esc";
const NO_MATCHES: &str = "no matches";

impl Render for Palette {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input = input_row("palette-input", self.input.clone()).children(
            status_filter(self.scope, &self.query(cx))
                .flatten()
                .map(|filter| {
                    div()
                        .debug_selector(|| "palette-filter".into())
                        .flex_shrink_0()
                        .ml(px(FLOAT_DETAIL_GAP))
                        .text_color(rgb(TEXT_MUTED))
                        .child(filter_word(filter))
                }),
        );
        let mut list = div()
            .id("palette-rows")
            .flex()
            .flex_col()
            .min_h_0()
            .max_h(px(PALETTE_MAX_H))
            .overflow_y_scroll()
            .track_scroll(&self.scroll);
        let mut section = None;
        for (at, shown) in self.shown.iter().enumerate() {
            let row = &self.rows[shown.row];
            if section != Some(row.section) {
                section = Some(row.section);
                list = list.child(
                    crate::menu::section(row.section.title(), None, None)
                        .debug_selector(move || format!("palette-section-{}", at)),
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
            list = list.child(crate::menu::note(note).mt(px(FLOAT_SECTION_GAP)));
        }
        let footer = match (
            self.selected().and_then(|row| row.detail.clone()),
            self.scope,
        ) {
            (Some(detail), _) => detail,
            (None, PaletteScope::All) => FOOTER.into(),
            (None, PaletteScope::Groups) => GROUPS_FOOTER.into(),
            (None, PaletteScope::Sessions) => SESSIONS_FOOTER.into(),
        };
        crate::menu::float()
            .debug_selector(|| "palette".into())
            .w(px(PALETTE_BOX_W))
            // A press inside never reaches the veil or the board under it.
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(input)
            .child(list)
            .child(crate::menu::footer_line(footer))
    }
}

/// A float's input row (the palette's, the shortcuts sheet's filter): `❯`
/// in its 2-cell gutter (the accent, the typed prompt mark), the line in
/// `TEXT_STRONG` with the block caret, a row and 12px tall over the float's
/// rule.
pub(crate) fn input_row(selector: &'static str, input: Entity<Composer>) -> gpui::Div {
    div()
        .debug_selector(move || selector.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(PALETTE_INPUT_H))
        .px(px(FLOAT_PAD_X))
        .border_b_1()
        .border_color(FLOAT_RULE)
        // Centred in the 31px over its rule, the line falls half a
        // pixel off the grid: set where the browser sets it.
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .w(px(FLOAT_GUTTER))
                .child(components::prompt_mark(ACCENT)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(FONT_CODE)
                .line_height(px(LH_UI))
                .text_color(rgb(TEXT_STRONG))
                .child(input),
        )
}

/// One palette row (the prototype's `.frow`): the gutter (`❯` on the
/// cursor), the 2-cell dot column, the name (matches lit), the context
/// muted after it, the key at the right.
fn row_element(
    at: usize,
    row: &Row,
    lit: &[Range<usize>],
    cursor: bool,
) -> gpui::Stateful<gpui::Div> {
    let id = gpui::ElementId::from(("palette-row", at));
    let key = crate::pointer::hover_key(&id);
    let ink = if cursor { TEXT_STRONG } else { TEXT };
    let name =
        StyledText::new(row.name.clone()).with_highlights(components::match_highlights(lit, false));
    let element = components::text_ui()
        .id(id)
        .debug_selector(move || format!("palette-row-{at}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .min_w_0()
        .h(px(FLOAT_ROW_H))
        .pl(px(FLOAT_PAD_X))
        .pr(px(PALETTE_ROW_PAD_R))
        .whitespace_nowrap()
        .text_color(rgb(ink))
        .child(crate::menu::gutter(cursor))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .w(px(FLOAT_GUTTER))
                .when_some(row.status, |cell, status| cell.child(status.face().dot())),
        )
        .child(div().flex_shrink_0().min_w_0().truncate().child(name))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .ml(px(row.context_gap()))
                .truncate()
                .text_color(rgb(TEXT_MUTED))
                .child(row.context.clone()),
        )
        .when_some(row.keys.clone(), |element, keys| {
            element.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .ml(px(FLOAT_DETAIL_GAP))
                    .child(components::key_combo(&keys, TEXT_MUTED)),
            )
        });
    if cursor {
        element.float_cursor().press_float()
    } else {
        element.hover_float(key).press_float()
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
        let shown = filter(&rows, "par", PaletteScope::All);
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
        assert_eq!(filter(&rows, "", PaletteScope::All).len(), rows.len());
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
        let kept = |query| names(&rows, &filter(&rows, query, PaletteScope::All));
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
        let kept = |query| names(&rows, &filter(&rows, query, PaletteScope::All));
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
        let shown = filter(&rows, "d", PaletteScope::Sessions);
        assert_eq!(names(&rows, &shown), ["deploy script"]);
    }
}
