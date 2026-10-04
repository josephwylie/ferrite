//! The command palette (⌘K, the prototype's `#palette`): every Thread, then
//! every command, filtered as the operator types (FL-1…FL-5).
//!
//! The view holds what it shows — the rows, the filter's survivors and the
//! cursor — and draws it in the float grammar: the input row (`❯`, the query
//! in `TEXT_STRONG`, the block caret), section rows (`threads`, `commands`,
//! or `groups` for ⌘G), rows (the cursor's `❯` on its `FLOAT_SEL` bar, a
//! still state dot, the name with its matches in the accent at `W_STRONG`,
//! the context muted after it, the key at the right), and the footer. The
//! cockpit builds the rows (`cockpit::palette`) and runs them: every key
//! the palette answers is a `palette::*` action bound in its `Palette`
//! context, handled at the cockpit's root.

use std::ops::Range;

use ferrite_core::groups::GroupId;
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::ThreadId;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, Context, Entity, IntoElement, ScrollHandle, SharedString, StyledText,
    Subscription, Window,
};

use crate::components;
use crate::composer::{Composer, Edited, Role};
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
        CompareWithMain,
        // Its own keys, in the `Palette` context.
        SelectNext,
        SelectPrevious,
        Confirm,
        Preview,
        Dismiss,
    ]
);

/// What the palette lists: everything (⌘K), or the Groups alone (⌘G).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaletteScope {
    All,
    Groups,
}

/// A row's section, in the order the palette draws them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Threads,
    Groups,
    Commands,
}

impl Section {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Section::Threads => "threads",
            Section::Groups => "groups",
            Section::Commands => "commands",
        }
    }
}

/// A Thread row's still state dot (never a spinner): `●` in the state's ink,
/// `○` faint for a parked Thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dot {
    Working,
    NeedsYou,
    Failing,
    Done,
    Parked,
}

impl Dot {
    pub(crate) fn glyph(self) -> &'static str {
        match self {
            Dot::Parked => "\u{25cb}",
            _ => "\u{25cf}",
        }
    }

    pub(crate) fn ink(self) -> u32 {
        match self {
            Dot::Working => RUNNING,
            Dot::NeedsYou => ATTENTION,
            Dot::Failing => BLOCKED,
            Dot::Done => TEXT_MUTED,
            Dot::Parked => TEXT_FAINT,
        }
    }
}

/// Every command the palette offers (FL-4), in its table's order. Each runs
/// what its key or its old chrome ran; a row hides while its act is
/// unavailable (`cockpit::palette::command_rows`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    ParkThread,
    ShowParked,
    CompareWithMain,
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
    NewProject,
    EditProject(ProjectId),
    Settings,
    ToggleSidebar,
    FullscreenPane,
    TextLarger,
    TextSmaller,
    TextReset,
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
    ShowPlan,
    UpdateClis,
    DiscardDraft,
}

/// What a row does when it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    /// Focus the Thread, or revive it; ⇥ opens it beside the current Pane.
    Thread(ThreadId),
    /// Enter the Group.
    Group(GroupId),
    Command(Command),
}

/// One palette row, ready to draw and to run.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    pub section: Section,
    pub name: SharedString,
    /// The muted words after the name: a Thread's `perf sweep · working`,
    /// a command's description. Part of what the query matches.
    pub context: SharedString,
    /// A Thread's state dot; `None` holds the 2-cell column empty.
    pub dot: Option<Dot>,
    /// The key that does the same, as the key table spells it (`cmd-1`).
    pub keys: Option<String>,
    pub act: Act,
}

impl Row {
    /// How far after the name its context stands: four cells for a Thread
    /// (and a Group), two for a command's description.
    fn context_gap(&self) -> f32 {
        match self.section {
            Section::Commands => FLOAT_DETAIL_GAP,
            Section::Threads | Section::Groups => PALETTE_CONTEXT_GAP,
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
            input,
            scroll: ScrollHandle::new(),
            _edited: edited,
        };
        palette.refilter(cx);
        palette
    }

    /// What the line holds now.
    pub(crate) fn query(&self, cx: &gpui::App) -> String {
        self.input.read(cx).text().to_string()
    }

    /// Filter the rows by the line (`fuzzy::palette_match`): survivors keep
    /// their order, never re-ranked; the cursor returns to the first.
    pub(crate) fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx);
        self.shown = filter(&self.rows, &query);
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

/// The rows the query keeps, with their names' lit ranges.
fn filter(rows: &[Row], query: &str) -> Vec<Shown> {
    rows.iter()
        .enumerate()
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

impl Render for Palette {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The input row: `❯` in its 2-cell gutter (the accent, the drawn
        // prompt mark), the query in `TEXT_STRONG` with the block caret,
        // a row and 12px tall over the float's rule.
        let input = div()
            .debug_selector(|| "palette-input".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(PALETTE_INPUT_H))
            .px(px(FLOAT_PAD_X))
            .border_b_1()
            .border_color(FLOAT_RULE)
            // Centred in the 31px over its rule, the line falls half a
            // pixel off the grid: set where the browser sets it.
            .child(components::css_line(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .w(px(FLOAT_GUTTER))
                    .child(components::prompt_mark(ACCENT)),
            ))
            .child(components::css_line(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_family(FONT_CODE)
                    .line_height(px(LH_UI))
                    .text_color(rgb(TEXT_STRONG))
                    .child(self.input.clone()),
            ));
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
            list = list.child(crate::menu::note("no matches").mt(px(FLOAT_SECTION_GAP)));
        }
        let footer = match self.scope {
            PaletteScope::All => FOOTER,
            PaletteScope::Groups => GROUPS_FOOTER,
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
                .when_some(row.dot, |cell, dot| {
                    cell.text_color(rgb(dot.ink())).child(dot.glyph())
                }),
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
        Row {
            section: Section::Threads,
            name: name.to_string().into(),
            context: context.to_string().into(),
            dot: Some(Dot::Working),
            keys: None,
            act: Act::Thread(ThreadId::new(id)),
        }
    }

    fn command(name: &str, context: &str, command: Command) -> Row {
        Row {
            section: Section::Commands,
            name: name.to_string().into(),
            context: context.to_string().into(),
            dot: None,
            keys: None,
            act: Act::Command(command),
        }
    }

    /// The filter keeps the table's order and lights only the names.
    #[test]
    fn par_keeps_the_prototypes_rows_in_order() {
        let rows = vec![
            thread("Nav rows jitter on stream start", "ferrite \u{b7} done", 1),
            thread("Perf: layout cache", "perf sweep \u{b7} working", 2),
            thread("Close stale issues", "perf sweep \u{b7} needs you", 3),
            thread("Parked \u{b7} Release 0.5.0 notes", "ferrite \u{b7} 2h", 4),
            thread("Port onboarding flow", "zeron \u{b7} working", 5),
            command("park thread", "", Command::ParkThread),
            command("show parked", "", Command::ShowParked),
            command(
                "compare with main",
                "open the diff reader",
                Command::CompareWithMain,
            ),
            command("open a group", "", Command::OpenGroup),
        ];
        let shown = filter(&rows, "par");
        let names: Vec<&str> = shown
            .iter()
            .map(|shown| rows[shown.row].name.as_ref())
            .collect();
        assert_eq!(
            names,
            [
                "Perf: layout cache",
                "Parked \u{b7} Release 0.5.0 notes",
                "Port onboarding flow",
                "park thread",
                "show parked",
                "compare with main",
            ]
        );
        assert_eq!(filter(&rows, "").len(), rows.len());
    }

    #[test]
    fn the_dots_are_the_states_inks() {
        assert_eq!(Dot::Working.ink(), RUNNING);
        assert_eq!(Dot::NeedsYou.ink(), ATTENTION);
        assert_eq!(Dot::Failing.ink(), BLOCKED);
        assert_eq!(Dot::Done.ink(), TEXT_MUTED);
        assert_eq!(
            (Dot::Parked.ink(), Dot::Parked.glyph()),
            (TEXT_FAINT, "\u{25cb}")
        );
        assert_eq!(Dot::Done.glyph(), "\u{25cf}");
    }
}
