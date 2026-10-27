//! The command palette (⌘K, the prototype's `#palette`): every Thread, the
//! Projects and Groups to jump to, then every command, filtered as the
//! operator types (FL-1…FL-5). Scoped, it is also ⌘G's Group list and the
//! session import picker.
//!
//! The view holds what it shows — the rows, the filter's survivors and the
//! cursor — and draws it as the command-search dialog (SPEC §5.8, theme's
//! `PALETTE_*`): the input row (the `search` icon, the query in Geist
//! `TEXT_STRONG`, `esc` or the status filter's name at the right), section
//! headings (`Threads`, `Projects`, `Groups`, `Commands`, or `Sessions`),
//! rows (a state mark or the command's icon, the name with its matches in
//! `TEXT_STRONG` at `W_STRONG`, the context and the chord at the right, the
//! cursor row `SELECTED` with its enter mark), and the footer — the cursor
//! row's detail (a Thread's working directory) where it has one, else the
//! keys, and the result count. The cockpit builds the rows
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
            Section::Commands => "Commands",
            Section::Sessions => "Sessions",
        }
    }
}

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
    /// One of Projects' commands (ADR 0031).
    Projects(crate::projects::Command),
}

impl Command {
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

/// The palette's footer, word for word (`command-search.png`'s).
pub(crate) const FOOTER: &str =
    "\u{2191}\u{2193} select \u{b7} \u{23ce} open \u{b7} \u{21e5} preview in a pane \u{b7} esc";
/// ⌘G's footer: a Group has no pane to preview in.
pub(crate) const GROUPS_FOOTER: &str = "\u{2191}\u{2193} select \u{b7} \u{23ce} open \u{b7} esc";
/// The import picker's footer: ⏎ imports the session and resumes it.
pub(crate) const SESSIONS_FOOTER: &str =
    "\u{2191}\u{2193} select \u{b7} \u{23ce} import \u{b7} esc";
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
        let trailing = match status_filter(self.scope, &self.query(cx)).flatten() {
            Some(filter) => div()
                .debug_selector(|| "palette-filter".into())
                .text_color(rgb(filter_ink(filter)))
                .child(filter_word(filter)),
            None => div().text_color(rgb(TEXT_DIM)).child("esc"),
        };
        let input = input_row("palette-input", self.input.clone()).child(
            trailing
                .flex_shrink_0()
                .text_size(px(FS_META))
                .whitespace_nowrap(),
        );
        let mut list = div()
            .id("palette-rows")
            .flex()
            .flex_col()
            .min_h_0()
            .max_h(px(PALETTE_MAX_H))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px(px(PALETTE_LIST_PAD))
            .pt(px(PALETTE_LIST_PAD))
            .pb(px(PALETTE_LIST_PAD_B));
        let mut section = None;
        for (at, shown) in self.shown.iter().enumerate() {
            let row = &self.rows[shown.row];
            if section != Some(row.section) {
                let first = section.is_none();
                section = Some(row.section);
                list = list.child(
                    section_heading(row.section.title(), first)
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
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(PALETTE_ROW_H))
                    .px(px(PALETTE_ROW_PAD_X))
                    .text_color(rgb(TEXT_DIM))
                    .child(note),
            );
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
        // It comes in as every sheet does (`dialog-in`), over its veil
        // darkening in (`cockpit::palette`); it closes at once.
        ferrite_ui::motion::dialog_in_casting(
            "palette-in",
            dialog()
                .debug_selector(|| "palette".into())
                // A press inside never reaches the veil or the board under it.
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(input)
                .child(list)
                .child(footer_row(footer, results_word(self.shown.len()))),
            components::dialog_shadow,
        )
    }
}

/// The status filter's word in its state's ink: hue for the two states
/// that have one (needs you, working), `TEXT_DIM` for idle and done (done
/// is a shape, not a hue).
fn filter_ink(filter: Attention) -> u32 {
    match filter {
        Attention::Blocked => ATTENTION,
        Attention::Working => RUNNING,
        Attention::Idle | Attention::Done => TEXT_DIM,
    }
}

/// The dialog's shell (SPEC §5.8): `PALETTE_W` on `PALETTE_GROUND`,
/// radius `R_PALETTE`, clipped to it, the 1px `HAIRLINE_STRONG` ring drawn
/// *outside* the box (so it keeps its full 640 inside) and the deep shadow
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
        .rounded(px(R_PALETTE))
        .bg(PALETTE_GROUND)
        .shadow(components::dialog_shadow())
}

/// The dialog's input row (the palette's, the shortcuts sheet's filter):
/// `PALETTE_INPUT_H` over a `HAIRLINE`, the `search` icon in `TEXT_DIM`,
/// then the line in Geist `FS_PALETTE` `TEXT_STRONG`. The caller adds
/// what stands at its right.
pub(crate) fn input_row(selector: &'static str, input: Entity<Composer>) -> gpui::Div {
    div()
        .debug_selector(move || selector.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(PALETTE_GAP))
        // The row, then its hairline under it (gpui's border sits inside
        // the box).
        .h(px(PALETTE_INPUT_H + 1.0))
        .px(px(PALETTE_PAD_X))
        .border_b_1()
        .border_color(gpui::rgba(HAIRLINE))
        .child(ferrite_ui::icons::icon(
            ferrite_ui::icons::SEARCH,
            PALETTE_SEARCH_ICON,
            TEXT_DIM,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(FONT_UI)
                .text_size(px(FS_PALETTE))
                .line_height(px(LH_UI))
                .text_color(rgb(TEXT_STRONG))
                .child(input),
        )
}

/// A section heading: its word in 12/500 `TEXT_DIM`, set on the heading's
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
        .text_color(rgb(TEXT_DIM))
        .whitespace_nowrap()
        .child(title)
}

/// The footer: `PALETTE_FOOT_H` under a `HAIRLINE`, the cursor row's cwd or
/// the keys at the left (cut with an ellipsis), the result count at the
/// right, all 12 `TEXT_DIM`.
pub(crate) fn footer_row(left: SharedString, right: String) -> gpui::Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(SPACE_4))
        // Its hairline over it, then the row.
        .h(px(PALETTE_FOOT_H + 1.0))
        .px(px(PALETTE_PAD_X))
        .border_t_1()
        .border_color(gpui::rgba(HAIRLINE))
        .text_size(px(FS_META))
        .text_color(rgb(TEXT_DIM))
        .whitespace_nowrap()
        .child(div().flex_1().min_w_0().truncate().child(left))
        .child(
            div()
                .flex_shrink_0()
                .debug_selector(|| "palette-results".into())
                .child(right),
        )
}

/// A row's mark: a Thread's (or a Project's or Group's roll-up) state
/// mark, else the command's icon, else nothing.
fn mark(row: &Row) -> Option<gpui::AnyElement> {
    match (row.status, &row.act) {
        (Some(status), _) => Some(crate::nav::mark(status, false)),
        (None, Act::Command(command)) => command.icon().map(|icon| {
            ferrite_ui::icons::icon(icon, PALETTE_MARK_ICON, TEXT_DIM).into_any_element()
        }),
        _ => None,
    }
}

/// One palette row (SPEC §5.8): `PALETTE_ROW_H`, radius `R_CONTROL`, its
/// parts `PALETTE_GAP` apart — the mark column, the name (`TEXT`, its
/// matches 600 `TEXT_STRONG`), the context at the right in 12 `TEXT_DIM`,
/// the chord after it as plain 12 `TEXT_DIM` text, then the enter mark's
/// column: `corner-down-left` on the cursor row (`SELECTED`, its name all
/// `TEXT_STRONG`), held empty on a row with no chord so the contexts line
/// up.
fn row_element(
    at: usize,
    row: &Row,
    lit: &[Range<usize>],
    cursor: bool,
) -> gpui::Stateful<gpui::Div> {
    let id = gpui::ElementId::from(("palette-row", at));
    let key = ferrite_ui::pointer::hover_key(&id);
    let ink = if cursor { TEXT_STRONG } else { TEXT };
    let name =
        StyledText::new(row.name.clone()).with_highlights(components::match_highlights(lit, false));
    let meta = || div().text_size(px(FS_META)).text_color(rgb(TEXT_DIM));
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
        .text_color(rgb(ink))
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
        .when_some(row.keys.clone(), |element, keys| {
            element.child(
                meta().flex_shrink_0().child(
                    components::key_combo(&keys, TEXT_DIM)
                        .font_family(FONT_UI)
                        .text_size(px(FS_META)),
                ),
            )
        })
        .when(cursor || row.keys.is_none(), |element| {
            element.child(div().flex().flex_shrink_0().w(px(PALETTE_ENTER_ICON)).when(
                cursor,
                |cell| {
                    cell.child(ferrite_ui::icons::icon(
                        ferrite_ui::icons::CORNER_DOWN_LEFT,
                        PALETTE_ENTER_ICON,
                        TEXT_DIM,
                    ))
                },
            ))
        });
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

    /// The footer counts what survives, in the singular for one.
    #[test]
    fn the_footer_counts_the_results() {
        assert_eq!(results_word(0), "0 results");
        assert_eq!(results_word(1), "1 result");
        assert_eq!(results_word(5), "5 results");
    }

    /// The headings are sentence case, `Commands` (never `Actions`), and
    /// the mockup's commands lead with its icons.
    #[test]
    fn headings_and_command_icons_follow_the_mockup() {
        assert_eq!(Section::Threads.title(), "Threads");
        assert_eq!(Section::Commands.title(), "Commands");
        assert_eq!(Command::NewThread.icon(), Some(ferrite_ui::icons::PLUS));
        assert_eq!(
            Command::NewWorktreeThread.icon(),
            Some(ferrite_ui::icons::BRANCH)
        );
        assert_eq!(Command::OpenGroup.icon(), Some(ferrite_ui::icons::BOARD));
        assert_eq!(Command::SortCreated.icon(), None);
    }
}
