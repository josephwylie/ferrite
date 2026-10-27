//! The operator's own commands and keys (Settings `commands` and `keys`,
//! ADR 0024): a command runs for the focused Thread, in its working
//! directory, on a thread of its own, until it ends or its palette row
//! stops it, and says how it went in a toast or shows its output in that
//! Thread's preview, in the Panel; a key the launch refused is said
//! once; `reset keys` puts the defaults back.

use super::*;
use crate::palette::{Act, Command, Row as PaletteRow, Section as PaletteSection};
use ferrite_core::user_commands::{Output, Ran, UserCommand};
use gpui::component::notification::{Notification, NotificationType};
use gpui::component::WindowExt as _;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// How many of a command's last lines its toast says.
const TOAST_LINES: usize = 4;
/// How often the window looks for a running command's end.
pub(super) const ENDED_POLL: Duration = Duration::from_millis(100);

/// The operator's commands still running, and what the window has still
/// to say about them and the keys: toasts wait for render, the one place
/// with a Window in hand.
#[derive(Default)]
pub(super) struct UserCommands {
    /// Each running command's stop switch, by its place in `commands`.
    pub(super) running: HashMap<usize, Arc<AtomicBool>>,
    toasts: Vec<Notification>,
}

impl CockpitView {
    /// The palette's rows for the operator's commands, after the built-in
    /// ones: only while the focused Thread has a directory to run in. A
    /// running command's row stops it.
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
            .map(|(index, command)| {
                let running = self.user_commands.running.contains_key(&index);
                PaletteRow {
                    section: PaletteSection::Commands,
                    name: if running {
                        format!("stop {}", command.name).into()
                    } else {
                        command.name.clone().into()
                    },
                    context: command.run.label().into(),
                    status: None,
                    detail: None,
                    keys: None,
                    act: Act::Command(Command::User(index)),
                }
            })
            .collect()
    }

    /// Run the operator's command `index` for the focused Thread, or stop
    /// it if it is running. Its output goes to the Pane it was run from, if
    /// that Pane is still open when it ends.
    pub(super) fn run_user_command(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(stop) = self.user_commands.running.get(&index) {
            stop.store(true, Ordering::Relaxed);
            return;
        }
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
        self.user_commands.toasts.push(
            Notification::new()
                .title(format!("Running {}", command.name))
                .message(command.run.label())
                .autohide(true),
        );
        let stop = Arc::new(AtomicBool::new(false));
        self.user_commands.running.insert(index, stop.clone());
        cx.notify();
        // Its own thread, not an executor worker: a watcher may run all
        // session long. The window looks for its end now and then.
        let (done, ended) = ferrite_core::doorbell::channel();
        std::thread::spawn({
            let command = command.clone();
            let cwd = cwd.clone();
            move || {
                let _ = done.send(command.run(&cwd, thread, &stop));
            }
        });
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(ENDED_POLL).await;
            let ran = match ended.try_recv() {
                Ok(ran) => ran,
                Err(std::sync::mpsc::TryRecvError::Empty) => continue,
                // Its thread died without a word: still an end to say,
                // and the row freed to run it again.
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Ran {
                    ok: false,
                    output: "the command was lost before it could say how it went".into(),
                },
            };
            this.update(cx, |view, cx| {
                view.user_command_ran(index, &command, cwd, owner, ran, cx)
            })
            .ok();
            return;
        })
        .detach();
    }

    /// A command ended: its toast, or its output in the preview of the
    /// Pane it was run from (and a toast too when it failed or was
    /// stopped, or when that Pane has gone).
    fn user_command_ran(
        &mut self,
        index: usize,
        command: &UserCommand,
        cwd: std::path::PathBuf,
        owner: PaneIdentity,
        ran: Ran,
        cx: &mut Context<Self>,
    ) {
        let stopped = self
            .user_commands
            .running
            .remove(&index)
            .is_some_and(|stop| stop.load(Ordering::Relaxed));
        let pane = self.index_of(owner);
        if command.output == Output::Reader {
            if let Some(index) = pane {
                self.empty_shown = false;
                // Named as a log in the directory it ran in, so the
                // preview reads it as plain text; nothing is written there.
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
        } else if stopped {
            (format!("{} stopped", command.name), NotificationType::Info)
        } else {
            (format!("{} failed", command.name), NotificationType::Error)
        };
        let tail = ran.tail(TOAST_LINES);
        self.user_commands.toasts.push(
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
    /// `settings.json` the launch refused.
    pub(super) fn present_user_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let refused = std::mem::take(&mut self.prefs.keys_refused);
        if !refused.is_empty() {
            self.user_commands.toasts.push(
                Notification::new()
                    .title("Some keys in settings.json were not bound")
                    .message(refused.join("\n"))
                    .with_type(NotificationType::Warning)
                    .autohide(false),
            );
        }
        if self.user_commands.toasts.is_empty() {
            // Called every render: nothing to say, nothing to redraw.
            return;
        }
        for toast in std::mem::take(&mut self.user_commands.toasts) {
            window.push_notification(toast, cx);
        }
        cx.notify();
    }

    /// `reset keys`: forget the operator's keys and bind the defaults.
    pub(super) fn reset_keys(&mut self, cx: &mut Context<Self>) {
        self.change_settings(|settings| settings.keys.clear(), cx);
        crate::frame().bind_keys(
            ferrite_ui::keymap::bindings(ferrite_ui::keymap::PLATFORM),
            cx,
        );
    }
}
