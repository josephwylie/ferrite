//! The tools behind three of the Panel's Surfaces (ADR 0039): Files
//! (`files`, the tree and its editor), Changes (`changes`, the diff) and the
//! Terminal (`terminal`, one shell a Surface). Each is per Thread: a
//! Thread's tools stay as they were left when focus moves away and back.
//! Their chords, ⇧⌘F, ⇧⌘D and ⌘J, open their Surface through the Panel's
//! one door (`upsert_surface`); pressed again they bring the same tab
//! forward. The Terminal keeps its shells, Changes its diffs and Files
//! its trees and editors in views of their own (`TerminalsView`,
//! `ChangesView`, `FilesView`, ADR 0041). The first two are
//! `ferrite-tools`', and `changes` and `terminal` here are the window's
//! side of them; Files is the Cockpit's, as it registers its tab stops in
//! the Panel's `PanelFocus` while it draws.

use std::collections::HashSet;

use ferrite_core::ThreadKey;

mod changes;
mod editor;
mod files;
pub(in crate::cockpit) mod remote_files;
mod save;
mod terminal;
mod tree;
mod working_set;

use super::surfaces::SurfaceId;
use super::*;

/// What the window notes for its tools.
#[derive(Default)]
pub(crate) struct Tools {
    /// Threads whose agent worked since the last sweep, so their checkout
    /// may have changed (`follow_workspace`).
    worked: HashSet<ThreadKey>,
}

gpui::actions!(cockpit, [ToggleTerminal, ToggleChanges, ToggleFiles, ToggleFileTree, GoToFile]);

#[cfg(test)]
pub(crate) use editor::AUTO_SAVE_AFTER;
pub(crate) use editor::Editor;
#[cfg(test)]
pub(crate) use save::SaveState;
pub(crate) use files::{FilesView, Sub as FilesSub, Verb as FilesVerb};
#[cfg(test)]
pub(crate) use tree::Status as FileStatus;
pub(crate) use working_set::WorkingSets;

impl CockpitView {
    /// Note that the agent of each of these Threads worked this frame.
    pub(super) fn note_work(&mut self, frame: &[ferrite_client::cockpit::PaneUpdate]) {
        self.tools
            .worked
            .extend(frame.iter().map(|update| update.thread));
    }

    /// What the agents of the Threads that worked since the last call may
    /// have changed shows: the open Changes read again, and each Files
    /// buffer that holds no edits of its own takes its file's new text.
    pub(super) fn follow_workspace(&mut self, cx: &mut Context<Self>) {
        for thread in std::mem::take(&mut self.tools.worked) {
            let diff = ferrite_tools::changes::Diff::Changes(thread);
            if self.changes.read(cx).holds(diff) {
                self.read_diff(diff, true, cx);
            }
            self.files.update(cx, |files, cx| files.follow_disk(thread, cx));
        }
    }

    /// ⌘J: the focused Thread's last shell brought forward, or its first
    /// started.
    pub(super) fn toggle_terminal(&mut self, _: &ToggleTerminal, _: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread(cx) else {
            return;
        };
        if self.opener_unavailable(thread, panel::Opener::Terminal, cx).is_some() {
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
        if let Some(thread) = self.focused_thread(cx) {
            self.run_opener(thread, opener, cx);
        }
    }

    pub(super) fn toggle_file_tree_action(&mut self, _: &ToggleFileTree, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(thread) = self.focused_thread(cx) {
            self.files
                .update(cx, |files, cx| files.run_verb(thread, FilesVerb::ToggleTree, cx));
        }
    }

    pub(super) fn go_to_file_action(&mut self, _: &GoToFile, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(thread) = self.focused_thread(cx) {
            self.go_to_file(thread, window, cx);
        }
    }

    /// Every Thread `keep` refuses takes its tools with it: its Files and
    /// each diff read for it.
    pub(super) fn forget_tools(&mut self, keep: impl Fn(ThreadKey) -> bool, cx: &mut gpui::App) {
        self.files.update(cx, |files, cx| files.forget(|thread| keep(thread), cx));
        self.changes.update(cx, |changes, _| changes.forget(&keep));
        self.tools.worked.retain(|thread| keep(*thread));
    }
}

#[cfg(test)]
impl CockpitView {
    /// How many of `thread`'s tools are held: its Files, and each diff read
    /// for it.
    pub(super) fn tools_held_for_test(&self, thread: ThreadKey, cx: &gpui::App) -> usize {
        usize::from(self.files.read(cx).holds(thread)) + self.changes.read(cx).held(thread)
    }
}
