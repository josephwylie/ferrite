//! What opens beside the focused Pane: a reader on a file, the diff reader
//! against a base, or another Thread (the palette's ⇥).
//!
//! Phase 0 (shared interface; the frame package owns this module and its
//! real implementation): the shapes every package calls, so the palette's
//! commands compile and run before the frame lands. A reader opens through
//! the Pane's own preview; the compare and Thread slots wait for the frame.

use super::*;

/// What `open_beside` opens next to the focused Pane.
#[allow(dead_code)] // the frame package reads every field
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Beside {
    Reader {
        path: std::path::PathBuf,
        line: Option<u32>,
    },
    Compare {
        workspace: std::path::PathBuf,
        base: String,
    },
    Thread(ThreadId),
}

impl CockpitView {
    /// Open `beside` next to the focused Pane.
    pub(crate) fn open_beside(
        &mut self,
        beside: Beside,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Beside::Reader { path, .. } = beside {
            if let Some(pane) = self.panes.get(self.focused()) {
                let title = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                pane.preview.open_document(path, title, window, cx);
            }
        }
        cx.notify();
    }

    /// The focused Thread's PR checks (the palette's `show checks`).
    pub(crate) fn open_checks(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        let checks = self
            .facts
            .get(thread)
            .and_then(|facts| facts.status.as_ref())
            .and_then(|status| status.pr.as_ref())
            .is_some_and(|pr| pr.checks.is_some());
        if checks {
            self.context_checks = Some(thread);
            cx.notify();
        }
    }

    /// The focused Thread's plan (the palette's `show plan`).
    pub(crate) fn open_plan(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }

    /// Install every provider CLI with a newer release (the palette's
    /// `update CLIs`).
    pub(crate) fn install_cli_updates(&mut self, cx: &mut Context<Self>) {
        for provider in self.cli_updates.ready() {
            self.update_cli(provider, cx);
        }
    }
}
