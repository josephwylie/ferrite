//! The tools behind three of the Panel's Surfaces (ADR 0039): Files
//! (`files`, the tree and its editor), Changes (`changes`, the diff) and the
//! Terminal (`terminal`, one shell a Surface). Each is per Thread: a
//! Thread's tools stay as they were left when focus moves away and back.
//! Their chords, ⇧⌘F, ⇧⌘D and ⌘J, open their Surface through the Panel's
//! one door (`upsert_surface`); pressed again they bring the same tab
//! forward. The Terminal keeps its shells in a view of its own
//! (`TerminalsView`, ADR 0041); the others' state is still `Tools`, here.

use std::collections::{HashMap, HashSet};

mod changes;
mod editor;
mod files;
mod save;
mod terminal;
mod tree;
mod working_set;

use super::surfaces::SurfaceId;
use super::*;

/// Every Thread's tools.
#[derive(Default)]
pub(crate) struct Tools {
    files: HashMap<ThreadId, files::Files>,
    changes: HashMap<changes::Diff, changes::Changes>,
    /// Threads whose agent worked since the last sweep, so their checkout
    /// may have changed (`follow_workspace`).
    worked: HashSet<ThreadId>,
}

gpui::actions!(cockpit, [ToggleTerminal, ToggleChanges, ToggleFiles, ToggleFileTree, GoToFile]);

#[cfg(test)]
pub(crate) use editor::AUTO_SAVE_AFTER;
pub(crate) use changes::{Diff, Sub as ChangesSub, Verb as ChangesVerb};
pub(crate) use editor::Editor;
#[cfg(test)]
pub(crate) use save::SaveState;
pub(crate) use files::{Sub as FilesSub, Verb as FilesVerb};
#[cfg(test)]
pub(crate) use tree::Status as FileStatus;
pub(crate) use terminal::{Sub as TerminalSub, TerminalsView, Verb as TerminalVerb};
pub(crate) use working_set::WorkingSets;

impl CockpitView {
    /// Note that the agent of each of these Threads worked this frame.
    pub(super) fn note_work(&mut self, frame: &[ferrite_core::cockpit::PaneUpdate]) {
        self.tools.worked.extend(frame.iter().map(|update| update.thread));
    }

    /// What the agents of the Threads that worked since the last call may
    /// have changed shows: the open Changes read again, and each Files
    /// buffer that holds no edits of its own takes its file's new text.
    pub(super) fn follow_workspace(&mut self, cx: &mut Context<Self>) {
        for thread in std::mem::take(&mut self.tools.worked) {
            if self.tools.changes.contains_key(&Diff::Changes(thread)) {
                self.reread_changes(Diff::Changes(thread), cx);
            }
            for editor in self.files_editors(thread) {
                editor.update(cx, |editor, cx| editor.follow_disk(cx));
            }
        }
    }

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
                self.terminals.update(cx, |terminals, _| terminals.want_focus());
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

#[cfg(test)]
impl CockpitView {
    /// How many of `thread`'s tools are held: its Files, and each diff read
    /// for it.
    pub(super) fn tools_held_for_test(&self, thread: ThreadId) -> usize {
        let of_thread = |diff: &Diff| match diff {
            Diff::Changes(of) => *of == thread,
            Diff::PullRequest(target) => target.source == panel::pr_read::PrSource::Thread(thread),
        };
        usize::from(self.tools.files.contains_key(&thread))
            + self.tools.changes.keys().filter(|diff| of_thread(diff)).count()
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
