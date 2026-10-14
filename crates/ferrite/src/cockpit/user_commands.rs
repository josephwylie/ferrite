//! The operator's own commands and keys (Settings `commands` and `keys`,
//! ADR 0022): a command runs for the focused Thread, in its working
//! directory, off the UI thread, and says how it went in a toast or shows
//! its output in the reader beside that Thread's Pane; a key the launch
//! refused is said once; `reset keys` puts the defaults back.

use super::*;
use crate::palette::{Act, Command, Row as PaletteRow, Section as PaletteSection};
use ferrite_core::user_commands::{Output, Ran, UserCommand};
use gpui::component::notification::{Notification, NotificationType};
use gpui::component::WindowExt as _;

/// How many of a command's last lines its toast says.
const TOAST_LINES: usize = 4;

/// What the window has still to say about the operator's commands and
/// keys: toasts wait for render, the one place with a Window in hand.
#[derive(Default)]
pub(super) struct Said {
    toasts: Vec<Notification>,
    keys_told: bool,
}

impl CockpitView {
    /// The palette's rows for the operator's commands, after the built-in
    /// ones: only while the focused Thread has a directory to run in.
    pub(super) fn user_command_rows(&self) -> Vec<PaletteRow> {
        let runnable = self
            .focused_thread()
            .is_some_and(|thread| self.thread_path(thread).is_some());
        if !runnable {
            return Vec::new();
        }
        self.prefs
            .settings
            .commands
            .iter()
            .enumerate()
            .map(|(index, command)| PaletteRow {
                section: PaletteSection::Commands,
                name: command.name.clone().into(),
                context: command.run.label().into(),
                dot: None,
                keys: None,
                act: Act::Command(Command::User(index)),
            })
            .collect()
    }

    /// Run the operator's command `index` for the focused Thread. Its
    /// output goes to the Pane it was run from, if that Pane is still open
    /// when it ends.
    pub(super) fn run_user_command(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(command) = self.prefs.settings.commands.get(index).cloned() else {
            return;
        };
        let Some(thread) = self.focused_thread() else {
            return;
        };
        let Some(cwd) = self.thread_path(thread) else {
            return;
        };
        let owner = PaneIdentity::Thread(thread);
        self.said.toasts.push(
            Notification::new()
                .title(format!("Running {}", command.name))
                .message(command.run.label())
                .autohide(true),
        );
        cx.notify();
        let task = cx.background_executor().spawn({
            let command = command.clone();
            let cwd = cwd.clone();
            async move { command.run(&cwd, thread) }
        });
        cx.spawn(async move |this, cx| {
            let ran = task.await;
            this.update(cx, |view, cx| {
                view.user_command_ran(&command, cwd, owner, ran, cx)
            })
            .ok();
        })
        .detach();
    }

    /// A command ended: its toast, or its output in the reader beside the
    /// Pane it was run from (and a toast too when it failed, or when that
    /// Pane has gone).
    fn user_command_ran(
        &mut self,
        command: &UserCommand,
        cwd: std::path::PathBuf,
        owner: PaneIdentity,
        ran: Ran,
        cx: &mut Context<Self>,
    ) {
        let pane = self.index_of(owner);
        if command.output == Output::Reader {
            if let Some(index) = pane {
                self.empty_shown = false;
                // Named as a log in the directory it ran in, so the
                // reader's chip reads LOG; nothing is written there.
                self.panes[index].preview.open_source(
                    cwd.join(format!("{}.log", command.name)),
                    command.name.clone(),
                    ran.output.clone(),
                );
                if ran.ok {
                    cx.notify();
                    return;
                }
            }
        }
        let (title, kind) = if ran.ok {
            (command.name.clone(), NotificationType::Success)
        } else {
            (format!("{} failed", command.name), NotificationType::Error)
        };
        let tail = ran.tail(TOAST_LINES);
        self.said.toasts.push(
            Notification::new()
                .title(title)
                .message(if tail.is_empty() {
                    "no output".into()
                } else {
                    tail
                })
                .with_type(kind)
                .autohide(ran.ok),
        );
        cx.notify();
    }

    /// Say what waits: the commands' toasts, and once, the keys in
    /// `settings.json` this launch refused (`keymap::with_user_keys`).
    pub(super) fn present_user_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.said.keys_told {
            self.said.keys_told = true;
            let (_, refused) =
                crate::keymap::with_user_keys(crate::keymap::PLATFORM, &self.prefs.settings.keys);
            if !refused.is_empty() {
                self.said.toasts.push(
                    Notification::new()
                        .title("Some keys in settings.json were not bound")
                        .message(refused.join("\n"))
                        .with_type(NotificationType::Warning)
                        .autohide(false),
                );
            }
        }
        if self.said.toasts.is_empty() {
            // Called every render: nothing to say, nothing to redraw.
            return;
        }
        for toast in std::mem::take(&mut self.said.toasts) {
            window.push_notification(toast, cx);
        }
        cx.notify();
    }

    /// `reset keys`: forget the operator's keys and bind the defaults.
    pub(super) fn reset_keys(&mut self, cx: &mut Context<Self>) {
        self.change_settings(|settings| settings.keys.clear(), cx);
        crate::bind_keys(crate::keymap::bindings(crate::keymap::PLATFORM), cx);
    }
}
