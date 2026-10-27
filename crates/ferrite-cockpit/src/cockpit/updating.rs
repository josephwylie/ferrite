//! Ferrite's own update in the window (`crate::ferrite_update`, ADR 0023):
//! the checks and the download off the UI thread, the restart offered —
//! now when no agent is working, else once they all are idle — the
//! Settings rows, and What's New on the first launch of a new release.

use super::*;
use crate::ferrite_update::{State, CHECK_EVERY, FIRST_CHECK_AFTER, IDLE_POLL, VERSION};
use ferrite_core::self_update;
use gpui::component::notification::{Notification, NotificationType};
use gpui::component::WindowExt as _;

/// Look for a newer Ferrite shortly after launch and every few hours
/// after, while the operator lets it.
pub(super) fn schedule_checks(cx: &mut Context<CockpitView>) {
    cx.spawn(async move |this, cx| {
        cx.background_executor().timer(FIRST_CHECK_AFTER).await;
        loop {
            let checked = this.update(cx, |view, cx| {
                if view.prefs.settings.check_for_updates {
                    view.check_for_ferrite_update(cx);
                }
            });
            if checked.is_err() {
                break;
            }
            cx.background_executor().timer(CHECK_EVERY).await;
        }
    })
    .detach();
}

/// Quit, and arrange for Ferrite to open again: that launch installs
/// what is staged. A relaunch that cannot be arranged still quits — the
/// operator's next launch installs it just the same.
fn restart(cx: &mut gpui::App) {
    if let Err(error) = self_update::relaunch_after_exit() {
        ferrite_core::log!("Ferrite will not reopen by itself: {error}");
    }
    cx.quit();
}

impl CockpitView {
    /// Ask GitHub for a newer Ferrite and download what it finds, unless a
    /// check is out or a release already waits.
    pub(crate) fn check_for_ferrite_update(&mut self, cx: &mut Context<Self>) {
        if !self.ferrite_update.begin_check() {
            return;
        }
        cx.notify();
        let dir = self.prefs.dir.clone();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async { self_update::check(VERSION) })
                .await;
            let release = this.update(cx, |view, cx| {
                cx.notify();
                view.ferrite_update.checked(found)
            });
            let Ok(Some(release)) = release else {
                return;
            };
            let staged = cx
                .background_executor()
                .spawn(async move { self_update::download(&dir, &release) })
                .await;
            this.update(cx, |view, cx| {
                view.ferrite_update.downloaded(staged);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Restart onto the ready release: at once when no agent is working
    /// or waiting, otherwise as soon as none is. Nothing is cut off.
    pub(crate) fn restart_to_update(&mut self, cx: &mut Context<Self>) {
        if !self.ferrite_update.restart_offered() {
            return;
        }
        if self.in_flight().is_none() {
            self.release_executable();
            restart(cx);
            return;
        }
        if !self.ferrite_update.restart_when_idle() {
            return;
        }
        cx.notify();
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(IDLE_POLL).await;
            let idle = this.update(cx, |view, cx| {
                let idle = view.in_flight().is_none();
                if idle {
                    view.release_executable();
                    restart(cx);
                }
                idle
            });
            if !matches!(idle, Ok(false)) {
                break;
            }
        })
        .detach();
    }

    /// Settings' update button: check, restart when idle, or — once
    /// already waiting — restart now, asking first as ⌘Q does. A release
    /// handed over gets its installer opened again.
    fn press_ferrite_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.ferrite_update.state() {
            State::Unchecked | State::UpToDate | State::Failed { .. } => {
                self.check_for_ferrite_update(cx)
            }
            State::HandedOver { .. } => {
                if let Err(error) = self_update::reopen(&self.prefs.dir) {
                    self.ferrite_update.reopen_failed(error);
                    cx.notify();
                }
            }
            State::Ready {
                when_idle: false, ..
            } => self.restart_to_update(cx),
            State::Ready {
                when_idle: true, ..
            } => {
                // On Windows the update stops the Server too, so it asks
                // even when the window's going alone would end nothing.
                let warning = if cfg!(windows) {
                    self.in_flight()
                        .map(|what| format!("{what}. The update stops them."))
                } else {
                    self.leaving_warning()
                };
                let view = cx.entity().downgrade();
                let leave = move |_: &mut Window, cx: &mut gpui::App| {
                    view.update(cx, |view, _| view.release_executable()).ok();
                    restart(cx);
                };
                if self.confirm_leaving("Restart", warning, window, cx, leave) {
                    self.release_executable();
                    restart(cx);
                }
            }
            State::Checking | State::Downloading { .. } => {}
        }
    }

    /// Settings › About's rows for Ferrite's own update: where it stands,
    /// with the one button that moves it on, and the automatic checks.
    /// None in a build that does not update itself.
    pub(super) fn ferrite_update_rows(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> Vec<prefs::Row> {
        if !self.prefs.ferrite_updates {
            return Vec::new();
        }
        let busy = self.in_flight();
        let (detail, button) = self.ferrite_update.describe(busy.as_deref());
        let view = cx.entity().downgrade();
        let handle = window.window_handle();
        vec![
            prefs::action(
                "settings-update-ferrite",
                "Updates",
                detail,
                SharedString::default(),
                button,
                move |cx| {
                    let view = view.clone();
                    let _ = handle.update(cx, |_, window, cx| {
                        view.update(cx, |view, cx| view.press_ferrite_update(window, cx))
                    });
                },
            ),
            prefs::toggle(
                "settings-check-for-updates",
                "Check for updates automatically",
                "Look for a newer Ferrite every few hours and download it. It installs \
                 when Ferrite restarts, which waits for your agents.",
                self.prefs.settings.check_for_updates,
                self.setting_change(cx, |s, v| s.check_for_updates = v),
            ),
        ]
    }

    /// The release just downloaded, offered once as a toast, and the
    /// notes of the release this launch installed. Like the CLI updater's
    /// toasts, they wait for render, the one place with a Window in hand.
    pub(super) fn present_ferrite_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(staged) = self.prefs.whats_new.take() {
            let title = format!("What's new in Ferrite {}", staged.version);
            let notes = if staged.notes.trim().is_empty() {
                "This release came with no notes.".to_string()
            } else {
                staged.notes
            };
            window.open_dialog(cx, move |dialog, window, _| {
                crate::rich::reading_dialog(
                    dialog,
                    "whats-new",
                    title.clone(),
                    gpui::base::text::TextView::markdown("whats-new-text", notes.clone())
                        .style(crate::rich::style(window.rem_size())),
                )
            });
        }
        let Some(version) = self.ferrite_update.take_offer() else {
            return;
        };
        let view = cx.entity().downgrade();
        window.push_notification(
            Notification::new()
                .title(format!("Ferrite {version} is ready"))
                .message(
                    "Click to restart once no agent is working. Otherwise it installs \
                     the next time Ferrite opens.",
                )
                .with_type(NotificationType::Info)
                .autohide(false)
                .on_click(move |_, _, cx| {
                    view.update(cx, |view, cx| view.restart_to_update(cx)).ok();
                }),
            cx,
        );
        cx.notify();
    }
}
