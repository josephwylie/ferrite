//! The window's Server (ADR 0025): the band that says there is none, the
//! restart onto this window's newer build, and Settings › Server — its
//! version, uptime and live Sessions, "Start at login", Restart and Stop.
//! A window on an Engine of its own (`--demo`, `--load`) has none of it.

use super::*;
use ferrite_core::cockpit::ServerState;
use ferrite_core::protocol::When;
use ferrite_core::server::agent;
use gpui::component::notification::{Notification, NotificationType};
use gpui::component::WindowExt as _;
use gpui::PromptLevel;

/// The no-Server band, so attaching again takes it down.
struct Detached;
/// The band saying the Server will update.
struct Updating;

/// What the window has said about its Server this run.
#[derive(Default)]
pub(crate) struct ServerBands {
    detached: bool,
    updating: bool,
    /// The restart onto this build is asked for once per run.
    restart_asked: bool,
}

impl CockpitView {
    /// Say when there is no Server, and ask an older one running this
    /// window's own executable to restart onto it once its agents are idle.
    pub(super) fn present_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.cockpit.server() {
            None => {}
            Some(ServerState::Detached { reason }) => {
                if std::mem::take(&mut self.server_bands.updating) {
                    window.remove_notification::<Updating>(cx);
                }
                if self.server_bands.detached {
                    return;
                }
                self.server_bands.detached = true;
                let view = cx.entity().downgrade();
                window.push_notification(
                    Notification::new()
                        .id::<Detached>()
                        .title("No Ferrite server")
                        .message(format!(
                            "{reason}. Your agents run in it; Ferrite keeps trying. \
                             Click to try now."
                        ))
                        .with_type(NotificationType::Warning)
                        .autohide(false)
                        .on_click(move |_, _, cx| {
                            view.update(cx, |view, cx| {
                                view.cockpit.retry_server();
                                view.server_bands.detached = false;
                                cx.notify();
                            })
                            .ok();
                        }),
                    cx,
                );
            }
            Some(ServerState::Attached { server, .. }) => {
                if std::mem::take(&mut self.server_bands.detached) {
                    window.remove_notification::<Detached>(cx);
                }
                if self.server_bands.restart_asked || !self.server_is_older(&server) {
                    return;
                }
                self.server_bands.restart_asked = true;
                if let Err(error) = self.cockpit.restart_server(When::Idle) {
                    ferrite_core::log!("the server was not asked to update: {error}");
                    return;
                }
                self.server_bands.updating = true;
                let view = cx.entity().downgrade();
                window.push_notification(
                    Notification::new()
                        .id::<Updating>()
                        .title("Ferrite's server will update when your agents are idle")
                        .message("Click to restart it now.")
                        .with_type(NotificationType::Info)
                        .autohide(false)
                        .on_click(move |_, window, cx| {
                            view.update(cx, |view, cx| view.restart_server(window, cx))
                                .ok();
                        }),
                    cx,
                );
            }
        }
    }

    /// Whether the Server is an older build of this very executable: one
    /// running any other is never asked, since it restarts into itself.
    fn server_is_older(&self, server: &ferrite_core::protocol::ServerInfo) -> bool {
        std::env::current_exe().is_ok_and(|executable| {
            ferrite_core::server::wants_restart(crate::ferrite_update::VERSION, &executable, server)
        })
    }

    /// Restart the Server now, asking first while an agent works or a
    /// Decision waits: the turn is cut off, and reads interrupted after.
    pub(super) fn restart_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.server_act("Restart", window, cx, |view| {
            view.cockpit.restart_server(When::Now)
        });
    }

    /// Stop the Server, asking first as a restart does. Its open Threads
    /// resume when a window starts it again.
    fn stop_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.server_act("Stop", window, cx, |view| {
            view.cockpit.stop_server(When::Now)
        });
    }

    fn server_act(
        &mut self,
        verb: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
        act: impl FnOnce(&mut Self) -> std::io::Result<()> + 'static,
    ) {
        let done = move |view: &mut Self, result: std::io::Result<()>| {
            if let Err(error) = result {
                view.group_error = Some(format!("the server did not {verb}: {error}").into());
            }
        };
        let Some(busy) = self.in_flight() else {
            let result = act(self);
            done(self, result);
            cx.notify();
            return;
        };
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("{verb} Ferrite's server?"),
            Some(&format!(
                "{busy}. The server going cuts their turns off; their Threads resume."
            )),
            &[verb, "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |view, cx| {
            if answer.await == Ok(0) {
                let _ = view.update(cx, |view, cx| {
                    let result = act(view);
                    done(view, result);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Windows' installer cannot replace a running executable, and the
    /// Server runs this one: an update stops it first (ADR 0025). Its
    /// agents are idle by then, and the relaunched window starts the new
    /// Server, which resumes their Threads.
    pub(super) fn release_executable(&mut self) {
        if cfg!(windows) && self.cockpit.server().is_some() {
            if let Err(error) = self.cockpit.stop_server(When::Now) {
                ferrite_core::log!("the server was not stopped for the update: {error}");
            }
        }
    }

    /// Settings › Server, for a window attached to one.
    pub(super) fn server_rows(&self, window: &Window, cx: &Context<Self>) -> Vec<prefs::Row> {
        let Some(state) = self.cockpit.server() else {
            return Vec::new();
        };
        let status: SharedString = match state {
            ServerState::Attached { server, since } => {
                let live = self.cockpit.live_sessions(Provider::Claude)
                    + self.cockpit.live_sessions(Provider::Codex);
                let sessions = match live {
                    1 => "1 live session".to_string(),
                    n => format!("{n} live sessions"),
                };
                let mut status = format!(
                    "{} · up {} · {sessions}",
                    server.version,
                    uptime(since.elapsed())
                );
                if std::env::current_exe().is_ok_and(|exe| exe != server.executable) {
                    status.push_str(&format!(" · runs {}", server.executable.display()));
                }
                status.into()
            }
            ServerState::Detached { reason } => format!("not running: {reason}").into(),
        };
        let handle = window.window_handle();
        let act = |view: gpui::WeakEntity<Self>,
                   run: fn(&mut Self, &mut Window, &mut Context<Self>)| {
            move |cx: &mut gpui::App| {
                let view = view.clone();
                let _ = handle.update(cx, |_, window, cx| {
                    view.update(cx, |view, cx| run(view, window, cx))
                });
            }
        };
        let view = cx.entity().downgrade();
        vec![
            prefs::fact("Server", status),
            prefs::toggle(
                "settings-start-at-login",
                "Start at login",
                "Your agents' server starts when you log in, so their Threads resume \
                 without a window.",
                self.prefs.settings.start_at_login,
                {
                    let view = view.clone();
                    move |on, cx| {
                        let _ = view.update(cx, |view, cx| {
                            view.change_settings(|settings| settings.start_at_login = on, cx);
                            if let Err(error) = std::env::current_exe()
                                .and_then(|exe| agent::start_at_login(&exe, on))
                            {
                                view.group_error =
                                    Some(format!("start at login not changed: {error}").into());
                            }
                        });
                    }
                },
            ),
            prefs::action(
                "settings-server-restart",
                "Restart",
                "Restart the server and resume every open Thread",
                SharedString::default(),
                Some("Restart".into()),
                act(view.clone(), Self::restart_server),
            ),
            prefs::action(
                "settings-server-stop",
                "Stop",
                "Stop the server and its agents; a window starts it again",
                SharedString::default(),
                Some("Stop".into()),
                act(view, Self::stop_server),
            ),
        ]
    }
}

/// How long the Server has run, as Settings says it: `42s`, `5m`, `3h 12m`,
/// `2d 4h`.
fn uptime(up: Duration) -> String {
    let seconds = up.as_secs();
    let (days, hours, minutes) = (seconds / 86_400, seconds / 3_600 % 24, seconds / 60 % 60);
    match (days, hours, minutes) {
        (0, 0, 0) => format!("{seconds}s"),
        (0, 0, minutes) => format!("{minutes}m"),
        (0, hours, minutes) => format!("{hours}h {minutes}m"),
        (days, hours, _) => format!("{days}d {hours}h"),
    }
}

#[cfg(test)]
mod tests {
    use super::uptime;
    use std::time::Duration;

    #[test]
    fn uptime_reads_in_its_two_largest_units() {
        assert_eq!(uptime(Duration::from_secs(42)), "42s");
        assert_eq!(uptime(Duration::from_secs(5 * 60 + 9)), "5m");
        assert_eq!(uptime(Duration::from_secs(3 * 3_600 + 12 * 60)), "3h 12m");
        assert_eq!(uptime(Duration::from_secs(2 * 86_400 + 4 * 3_600)), "2d 4h");
    }
}
