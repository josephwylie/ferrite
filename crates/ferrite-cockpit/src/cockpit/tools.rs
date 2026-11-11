//! The tools behind three of the Panel's Surfaces (ADR 0039): Files
//! (`files`, the tree and its editor), Changes (`changes`, the diff) and the
//! Terminal (`terminal`, one shell a Surface). Each is per Thread: a
//! Thread's tools stay as they were left when focus moves away and back.
//! Their chords, ⇧⌘F, ⇧⌘D and ⌘J, open their Surface through the Panel's
//! one door (`upsert_surface`); pressed again they bring the same tab
//! forward.

use std::collections::HashMap;

mod changes;
mod editor;
mod files;
mod terminal;
mod tree;

use super::surfaces::SurfaceId;
use super::*;

/// Every Thread's tools.
#[derive(Default)]
pub(crate) struct Tools {
    files: HashMap<ThreadId, files::Files>,
    changes: HashMap<ThreadId, changes::Changes>,
    terminals: HashMap<ThreadId, terminal::Terminals>,
    terminal_prefs: terminal::Prefs,
    /// The shown shell is to take the keyboard (`tend_terminal`).
    terminal_focus_wanted: bool,
    /// Whether the shell's right-click found a selection.
    terminal_selected: bool,
}

gpui::actions!(cockpit, [ToggleTerminal, ToggleChanges, ToggleFiles, ToggleFileTree, GoToFile]);

#[cfg(test)]
pub(crate) use editor::AUTO_SAVE_AFTER;
pub(crate) use changes::{Sub as ChangesSub, Verb as ChangesVerb};
pub(crate) use files::{Sub as FilesSub, Verb as FilesVerb};
pub(crate) use terminal::{Sub as TerminalSub, Verb as TerminalVerb};

impl CockpitView {
    /// ⌘J: the focused Thread's last shell brought forward, or its first
    /// started.
    pub(super) fn toggle_terminal(&mut self, _: &ToggleTerminal, _: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        if self.opener_unavailable(thread, panel::Opener::Terminal).is_some() {
            return;
        }
        let open = self
            .panel
            .surfaces
            .get(thread)
            .and_then(|state| {
                state
                    .tabs
                    .iter()
                    .rev()
                    .find(|id| matches!(id, SurfaceId::Terminal(_)))
                    .cloned()
            });
        match open {
            Some(id) => {
                self.upsert_surface(thread, id, cx);
                self.tools.terminal_focus_wanted = true;
            }
            None => self.new_terminal_surface(thread, cx),
        }
    }

    /// ⇧⌘D.
    pub(super) fn toggle_changes(&mut self, _: &ToggleChanges, _: &mut Window, cx: &mut Context<Self>) {
        self.open_tool_surface(panel::Opener::Changes, cx);
    }

    /// ⇧⌘F.
    pub(super) fn toggle_files(&mut self, _: &ToggleFiles, _: &mut Window, cx: &mut Context<Self>) {
        self.open_tool_surface(panel::Opener::Files, cx);
    }

    fn open_tool_surface(&mut self, opener: panel::Opener, cx: &mut Context<Self>) {
        if let Some(thread) = self.focused_thread() {
            self.run_opener(thread, opener, cx);
        }
    }

    pub(super) fn toggle_file_tree_action(&mut self, _: &ToggleFileTree, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_file_tree(cx);
    }

    pub(super) fn go_to_file_action(&mut self, _: &GoToFile, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(thread) = self.focused_thread() {
            self.go_to_file(thread, window, cx);
        }
    }
}

/// A Surface's own row of tools under the Panel's head: `SURFACE_BAR_H`
/// tall, a hairline under it.
pub(super) fn surface_bar() -> Div {
    use ferrite_ui::theme::*;
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(SPACE_1))
        .h(px(SURFACE_BAR_H))
        .px(px(SIDE_CHAT_INSET))
        .border_b(px(HAIRLINE_W))
        .border_color(paint::LINE)
}
