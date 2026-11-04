//! The title-bar tools (Claude's, `claude-files-changes.md`): the five doors
//! at the head row's right end — `Show session rail`, `Terminal`, `Changes`
//! (⇧⌘D), `Files` (⇧⌘F) and `⋯` — which replace the old Finder door. The
//! one in use wears a pressed ground.
//!
//! The rail is the panel's sections (`panel`), so its door is the panel's
//! toggle. The other three are the **dock**, the right column's other
//! half: beside the rail (or alone) it holds, each opened on its own door,
//!
//! - Files and Changes, one column — Files at the top, Changes under it,
//!   or either alone (`files`, `changes`);
//! - the Terminal, a column of its own to the right of that (`terminal`).
//!
//! The dock is per Thread: a Thread's tools stay as they were left when
//! focus moves away and back. Opening a tool gives the column to it — a
//! side chat, an agent or the Background tasks pane that held the column
//! closes — and a door pressed again puts its tool away.

use std::collections::HashMap;

use super::*;
use ferrite_ui::{icons, theme};

/// One of the title bar's tool doors that has a surface behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tool {
    /// The panel's sections.
    Rail,
    Terminal,
    Changes,
    Files,
}

/// What one Thread has open in the dock.
#[derive(Clone, Copy, Default)]
struct Dock {
    files: bool,
    changes: bool,
    terminal: bool,
}

impl Dock {
    fn is_open(self) -> bool {
        self.files || self.changes || self.terminal
    }
}

/// Every Thread's dock.
#[derive(Default)]
pub(crate) struct Tools {
    docks: HashMap<ThreadId, Dock>,
}

impl Tools {
    fn dock(&self, thread: ThreadId) -> Dock {
        self.docks.get(&thread).copied().unwrap_or_default()
    }
}

gpui::actions!(cockpit, [ToggleTerminal, ToggleChanges, ToggleFiles]);

impl CockpitView {
    /// Whether the Thread's dock has anything open.
    pub(super) fn dock_shown(&self, thread: Option<ThreadId>) -> bool {
        thread.is_some_and(|thread| self.tools.dock(thread).is_open())
    }

    /// Whether the dock's Files or Changes column is open for `thread`.
    pub(super) fn files_column_shown(&self, thread: ThreadId) -> bool {
        let dock = self.tools.dock(thread);
        dock.files || dock.changes
    }

    /// Whether the dock's Terminal column is open for `thread`.
    pub(super) fn terminal_column_shown(&self, thread: ThreadId) -> bool {
        self.tools.dock(thread).terminal
    }

    /// Whether `tool`'s surface is up for the focused Thread: the door's
    /// pressed ground.
    pub(super) fn tool_pressed(&self, tool: Tool) -> bool {
        let Some(thread) = self.focused_thread() else {
            return false;
        };
        let dock = self.tools.dock(thread);
        match tool {
            Tool::Rail => self.panel_pane().is_some() && self.prefs.settings.panel_shown,
            Tool::Terminal => dock.terminal,
            Tool::Changes => dock.changes,
            Tool::Files => dock.files,
        }
    }

    /// A tool's door, its chord, ⇧⌘D or ⇧⌘F: open its surface for the
    /// focused Thread, or put it away.
    pub(super) fn press_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        if tool == Tool::Rail {
            self.toggle_panel_now(cx);
            return;
        }
        let dock = self.tools.docks.entry(thread).or_default();
        let opening = match tool {
            Tool::Terminal => &mut dock.terminal,
            Tool::Changes => &mut dock.changes,
            Tool::Files => &mut dock.files,
            Tool::Rail => unreachable!("the rail is the panel's"),
        };
        *opening = !*opening;
        let opened = *opening;
        if !self.tools.dock(thread).is_open() {
            self.tools.docks.remove(&thread);
            self.panel.expanded = false;
        }
        if opened {
            self.give_column_to_tools(thread, cx);
        }
        cx.notify();
    }

    /// Close whatever else held the right column over `thread`'s dock.
    fn give_column_to_tools(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.close_side_chat(thread, cx);
        self.close_agent_drill(thread, cx);
        if self.panel.background == Some(thread) {
            self.panel.background = None;
        }
    }

    pub(super) fn toggle_terminal(&mut self, _: &ToggleTerminal, _: &mut Window, cx: &mut Context<Self>) {
        self.press_tool(Tool::Terminal, cx);
    }

    pub(super) fn toggle_changes(&mut self, _: &ToggleChanges, _: &mut Window, cx: &mut Context<Self>) {
        self.press_tool(Tool::Changes, cx);
    }

    pub(super) fn toggle_files(&mut self, _: &ToggleFiles, _: &mut Window, cx: &mut Context<Self>) {
        self.press_tool(Tool::Files, cx);
    }

    /// The dock's columns as one row, `width` wide: Files/Changes taking
    /// what the Terminal leaves.
    pub(super) fn dock_element(&self, thread: ThreadId, cx: &mut Context<Self>) -> Div {
        let mut row = div()
            .debug_selector(|| "dock".into())
            .flex()
            .flex_1()
            .min_w_0()
            .h_full();
        if self.files_column_shown(thread) {
            row = row.child(self.files_column(thread, cx));
        }
        if self.terminal_column_shown(thread) {
            row = row.child(self.terminal_column(thread, cx));
        }
        row
    }

    fn files_column(&self, thread: ThreadId, cx: &mut Context<Self>) -> Div {
        let dock = self.tools.dock(thread);
        let mut column = dock_column("dock-files", true);
        if dock.files {
            column = column.child(dock_head("Files", "dock-files-close", Tool::Files, cx));
        }
        if dock.changes {
            column = column.child(dock_head("Changes", "dock-changes-close", Tool::Changes, cx));
        }
        column
    }

    fn terminal_column(&self, _thread: ThreadId, cx: &mut Context<Self>) -> Div {
        dock_column("dock-terminal", false)
            .w(px(theme::TERMINAL_W))
            .child(dock_head("Terminal", "dock-terminal-close", Tool::Terminal, cx))
    }
}

/// A column of the dock: a vertical seam at its left edge, under the head
/// row's own, and its pieces stacked.
fn dock_column(selector: &'static str, grows: bool) -> Div {
    div()
        .debug_selector(move || selector.into())
        .flex()
        .flex_col()
        .h_full()
        .min_w_0()
        .border_l(px(theme::HAIRLINE_W))
        .border_color(theme::paint::LINE)
        .when(grows, |column| column.flex_1())
        .when(!grows, |column| column.flex_shrink_0())
}

/// A dock piece's head: its name, and its close at the right.
fn dock_head(
    title: &'static str,
    close_key: &'static str,
    tool: Tool,
    cx: &mut Context<CockpitView>,
) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::SPACE_1))
        .h(px(theme::WIN_CHROME_H))
        .px(px(theme::PANEL_PAD_L))
        .text_size(px(theme::FS_LABEL))
        .text_color(rgb(theme::INK_SUBTLE))
        .child(div().flex_1().min_w_0().truncate().child(title))
        .child(
            ferrite_pane::preview::head_door(close_key, close_key.into(), icons::CLOSE)
                .tooltip(ferrite_ui::menu::tooltip("Close"))
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.press_tool(tool, cx);
                })),
        )
}
