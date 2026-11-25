//! The board with nothing on it (F-13): the home. One calm column, centred
//! a touch above the middle: the greeting (`components::greeting`), the
//! provider CLIs Ferrite found, the ways to start as rows with their keys,
//! and the four Threads used last.
//!
//! In the chrome's grammar (theme `EMPTY_*`, SPEC §2, §5.8): sentence-case
//! labels, an icon column, keys read from the key table in `INK_SUBTLE`, the
//! selection a rounded `SELECTED` row and the rest the row hover.
//!
//! The rows are a list with its own focus and key context (`EmptyBoard`):
//! `empty_board::Next` / `Previous` move the selection, `Run` does what the
//! row says (the floats package binds ↑ ↓ ⏎). A press does the same.
//!
//! Its state lives in a view of its own (ADR 0041), `EmptyBoardView`: the
//! row selected and the list's focus, which its own keys and presses
//! write. The window holds it for its life, so the selection is where it
//! was left at the next visit. What it draws of the window it reads
//! through a `Source<EmptyBoardInputs>` the window makes over itself; what
//! a row asks it says as an `EmptyBoardEvent`. Whether it shows, the ways
//! to start, the Threads used last and the CLI probe whose versions it
//! names stay the window's.

use ferrite_core::ThreadKey;
use ferrite_pane::mark::{mark, RowStatus};
use ferrite_ui::pointer::{Pointer, PointerPressed};
use ferrite_ui::source::Source;
use gpui::prelude::*;
use gpui::{
    actions, div, px, rgb, AnyElement, ClickEvent, Context, Div, EventEmitter, FocusHandle,
    SharedString, Stateful, Window,
};

actions!(empty_board, [Next, Previous, Run]);

/// The key context the list holds focus under.
pub const KEY_CONTEXT: &str = "EmptyBoard";

/// One way to start: the action's name in the key table (its chord), its
/// icon, what the row says (the palette's words for the same command), and
/// what it dispatches.
pub struct StartRow {
    action: &'static str,
    icon: &'static str,
    verb: &'static str,
    run: Box<dyn gpui::Action>,
}

impl StartRow {
    pub fn new(
        action: &'static str,
        icon: &'static str,
        verb: &'static str,
        run: Box<dyn gpui::Action>,
    ) -> Self {
        Self {
            action,
            icon,
            verb,
            run,
        }
    }
}

/// A Thread used lately, as its nav row says it (its mark, name and
/// Project), how long since, and whether a Pane shows it.
pub struct RecentRow {
    pub thread: ThreadKey,
    pub status: RowStatus,
    pub name: SharedString,
    pub project: Option<SharedString>,
    pub age: Option<SharedString>,
    pub open: bool,
}

/// What the empty board draws from the window.
pub struct EmptyBoardInputs {
    /// The CLIs' versions, once probed: (claude, codex).
    pub cli_versions: Option<(SharedString, SharedString)>,
    /// The ways to start, in order: only those that would do something
    /// now (a parked Thread to reopen), as the palette offers them.
    pub rows: Vec<StartRow>,
    /// R13: the four Threads used last, open or parked.
    pub recent: Vec<RecentRow>,
}

/// What the empty board's rows ask of the window.
pub enum EmptyBoardEvent {
    /// A way to start: the action its row dispatches.
    Run(Box<dyn gpui::Action>),
    /// A recent Thread a Pane shows: focus it.
    Focus(ThreadKey),
    /// A recent Thread no Pane shows: bring it back.
    Revive(ThreadKey),
}

/// The empty board: its selected row and its list's focus.
pub struct EmptyBoardView {
    inputs: Source<EmptyBoardInputs>,
    /// The way to start that ⏎ runs.
    cursor: usize,
    /// The list's own focus, which the root's focus pump hands the
    /// keyboard while the empty board shows.
    focus: FocusHandle,
}

impl EventEmitter<EmptyBoardEvent> for EmptyBoardView {}

/// A home row's frame: `EMPTY_ROW_H`, the icon column, the label filling
/// the row, `trail` at the right.
fn home_row(
    id: impl Into<gpui::ElementId>,
    mark: AnyElement,
    label: Div,
    trail: Div,
) -> Stateful<Div> {
    use ferrite_ui::theme::*;
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(EMPTY_ROW_GAP))
        .w_full()
        .min_w_0()
        .h(px(EMPTY_ROW_H))
        .px(px(EMPTY_ROW_PAD_X))
        .rounded(px(R_CONTROL))
        .whitespace_nowrap()
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .w(px(EMPTY_ICON_W))
                .child(mark),
        )
        .child(label.flex_1().min_w_0().truncate())
        .child(
            trail
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(SPACE_3))
                .text_size(px(FS_META))
                .text_color(rgb(INK_SUBTLE)),
        )
}

/// `claude 2.1.316 · codex 0.166.0`: the provider CLIs found, each led
/// by its logomark, one dim `FS_META` line. Nothing until the probe
/// lands, and the plain fact when it found neither.
fn cli_facts(cli_versions: Option<&(SharedString, SharedString)>) -> Option<Div> {
    use ferrite_ui::theme::*;
    let (claude, codex) = cli_versions?;
    let version = |found: &SharedString| -> Option<SharedString> {
        (!found.starts_with("not found"))
            .then(|| {
                found
                    .split(" \u{b7} ")
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .filter(|version| !version.is_empty())
            .map(SharedString::from)
    };
    let found: Vec<(&'static str, u32, &'static str, SharedString)> = [
        (
            ferrite_ui::icons::CLAUDE,
            PROVIDER_CLAUDE,
            "claude",
            version(claude),
        ),
        (
            ferrite_ui::icons::CODEX,
            PROVIDER_CODEX,
            "codex",
            version(codex),
        ),
    ]
    .into_iter()
    .filter_map(|(glyph, ink, name, version)| version.map(|v| (glyph, ink, name, v)))
    .collect();
    let line = div()
        .debug_selector(|| "empty-board-facts".into())
        .flex()
        .items_center()
        .gap(px(SPACE_2))
        .whitespace_nowrap()
        .text_size(px(FS_META))
        .line_height(px(LH_META))
        .text_color(rgb(INK_SUBTLE));
    if found.is_empty() {
        return Some(line.child("No provider CLI found"));
    }
    Some(
        found
            .into_iter()
            .enumerate()
            .fold(line, |line, (at, (glyph, ink, name, version))| {
                line.when(at > 0, |line| line.child("\u{b7}")).child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(SPACE_1))
                        .child(
                            ferrite_ui::icons::icon(glyph, BAR_MARK, ink)
                                .mt(px(-STATUS_LOGO_FOOT)),
                        )
                        .child(SharedString::from(format!("{name} {version}"))),
                )
            }),
    )
}

/// A bound action's first keystroke as a shortcut spells it, from this
/// platform's key table: `cmd-shift-n` → `cmd-shift-N`, which
/// `components::key_combo` draws tight with glyph modifiers, `⌘⇧N`.
pub fn key_label(action: &str) -> Option<SharedString> {
    ferrite_ui::components::bound_chord(action).map(SharedString::from)
}

impl EmptyBoardView {
    pub fn new(inputs: Source<EmptyBoardInputs>, cx: &mut Context<Self>) -> Self {
        Self {
            inputs,
            cursor: 0,
            focus: cx.focus_handle(),
        }
    }

    /// The way to start that ⏎ runs.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The list's own focus, which the window hands the keyboard while the
    /// empty board shows.
    pub fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(inputs) = self.inputs.read(&(), cx) else {
            return;
        };
        let count = inputs.rows.len() as isize;
        let at = (self.cursor as isize).min(count - 1);
        let next = (at + delta).rem_euclid(count) as usize;
        if next != self.cursor {
            self.cursor = next;
            cx.notify();
        }
    }

    fn run(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(inputs) = self.inputs.read(&(), cx) else {
            return;
        };
        let rows = inputs.rows;
        let at = at.min(rows.len() - 1);
        let Some(row) = rows.into_iter().nth(at) else {
            return;
        };
        self.cursor = at;
        cx.emit(EmptyBoardEvent::Run(row.run));
        cx.notify();
    }
}

impl Render for EmptyBoardView {
    /// The board with no Pane on it (see the module doc).
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use ferrite_ui::theme::*;
        let Some(EmptyBoardInputs {
            cli_versions,
            rows,
            recent,
        }) = self.inputs.read(&(), cx)
        else {
            return div();
        };
        let head = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(EMPTY_FACTS_GAP))
            .child(ferrite_ui::components::greeting())
            .children(cli_facts(cli_versions.as_ref()));

        let mut actions = div()
            .debug_selector(|| "empty-board-commands".into())
            .track_focus(&self.focus)
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(|board, _: &Next, _, cx| board.step(1, cx)))
            .on_action(cx.listener(|board, _: &Previous, _, cx| board.step(-1, cx)))
            .on_action(cx.listener(|board, _: &Run, _, cx| board.run(board.cursor, cx)))
            .flex()
            .flex_col()
            .w_full();
        // The rows offered can shrink under the cursor (the last parked
        // Thread reopened): it holds on the last.
        let cursor = self.cursor.min(rows.len() - 1);
        for (at, row) in rows.into_iter().enumerate() {
            let StartRow {
                action,
                icon: glyph,
                verb,
                ..
            } = row;
            let selected = at == cursor;
            let keys = key_label(action);
            let row = home_row(
                ("empty-board-command", at),
                ferrite_ui::icons::icon(glyph, NAV_ICON, INK_STRONG).into_any_element(),
                div().text_color(rgb(INK_STRONG)).child(verb),
                div().children(keys.map(|keys| {
                    ferrite_ui::components::key_combo(&keys, INK_SUBTLE)
                        .font_family(FONT_UI)
                        .text_size(px(FS_META))
                })),
            )
            .debug_selector(move || format!("empty-board-command-{at}"));
            let row = if selected {
                row.bg(paint::SELECTED).cursor_pointer()
            } else {
                row.hover_row(format!("empty-board-command-{at}"))
                    .press_row()
            };
            actions = actions.child(row.on_click(
                cx.listener(move |board, _: &ClickEvent, _, cx| board.run(at, cx)),
            ));
        }

        let any_recent = !recent.is_empty();
        let recent_rows = recent.into_iter().map(
            |RecentRow {
                 thread,
                 status,
                 name,
                 project,
                 age,
                 open,
             }| {
                home_row(
                    ("empty-board-recent", thread.thread().get() as usize),
                    mark(status, false),
                    div().text_color(rgb(INK_STRONG)).child(name),
                    div()
                        .children(project)
                        .children(age.filter(|age| !age.is_empty())),
                )
                .debug_selector(move || format!("empty-board-recent-{}", thread.thread().get()))
                .hover_row(format!("empty-board-recent-{}", thread.thread().get()))
                .press_row()
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.emit(if open {
                        EmptyBoardEvent::Focus(thread)
                    } else {
                        EmptyBoardEvent::Revive(thread)
                    });
                }))
            },
        );
        let recent_list = any_recent.then(|| {
            div()
                .debug_selector(|| "empty-board-recent".into())
                .flex()
                .flex_col()
                .w_full()
                // A section heading, as the nav's and the palette's, on the
                // labels' edge.
                .child(
                    div()
                        .pl(px(EMPTY_ROW_PAD_X + EMPTY_ICON_W + EMPTY_ROW_GAP))
                        .pb(px(EMPTY_HEADING_PAD_B))
                        .text_size(px(FS_META))
                        .line_height(px(LH_META))
                        .font_weight(W_LABEL)
                        .text_color(rgb(INK_SUBTLE))
                        .child("Recent"),
                )
                .children(recent_rows)
        });

        let column = div()
            .flex()
            .flex_col()
            .items_center()
            .flex_shrink_0()
            .w_full()
            .max_w(px(TX_COLUMN_MAX_W))
            .gap(px(EMPTY_SECTION_GAP))
            .child(head)
            .child(actions)
            .children(recent_list);
        div()
            .debug_selector(|| "empty-board".into())
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .min_w_0()
            .overflow_hidden()
            .bg(paint::CARD)
            .p(px(EMPTY_PAD))
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .text_color(rgb(INK_STRONG))
            // Centred, a touch above the middle (`EMPTY_LIFT`), and pinned
            // to the top once it no longer fits.
            .child(div().flex_1().min_h_0())
            .child(column)
            .child(div().flex_1().min_h_0().pb(px(EMPTY_LIFT)))
    }
}

#[cfg(test)]
mod tests;
