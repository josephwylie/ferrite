//! The window's Server (ADR 0025): the band that says there is none, the
//! restart onto this window's newer build, and Settings › Server — its
//! version, uptime and live Sessions, "Start at login", Restart and Stop.
//! A window on an Engine of its own (`--demo`, `--load`) has none of it.

use super::*;
use ferrite_core::cockpit::{ServerRestart, ServerState};
use ferrite_core::machine::Failure;
use ferrite_core::protocol::When;
use ferrite_core::server::agent;
use gpui::component::notification::{Notification, NotificationType};
use gpui::component::WindowExt as _;
use gpui::PromptLevel;

/// How long an update waits for the Server to end: past the Store's
/// final syncs.
const RELEASE_WAIT: Duration = Duration::from_secs(5);

/// The no-Server band, so attaching again takes it down.
struct Detached;

/// What the no-Server band says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gone {
    /// This computer's Server does not answer.
    NoServer,
    /// The machine the window is on went.
    Lost,
    SignIn,
    NotInstalled,
}
/// The offer to restart an older Server.
struct Updating;

/// A restart, as the window sees it through.
struct Restarting;

/// Which build a Server runs, as this window can tell it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Build {
    /// This window's own executable, as installed.
    Installed,
    /// An older build of this window's executable.
    Older,
    /// Another executable, or a machine's: not this window's to judge.
    Other,
}

/// What the window has said about its Server this run.
#[derive(Default)]
pub(crate) struct ServerBands {
    /// The no-Server band up, and what it says.
    detached: Option<Gone>,
    /// The Server answered once: a machine's that goes is lost, not still
    /// being reached.
    attached: bool,
    updating: bool,
    /// The restart onto this build is asked for once per run.
    restart_asked: bool,
    /// The Server's state, and its restart, as the pump last saw them.
    seen: Option<ServerState>,
    restart_seen: Option<ServerRestart>,
    /// The restart the window last said something about.
    restart: Option<ServerRestart>,
}

#[cfg(test)]
impl ServerBands {
    /// What the no-Server band says, while it is up.
    pub(crate) fn detached(&self) -> Option<Gone> {
        self.detached
    }
}

impl CockpitView {
    /// Whether the Server's state moved since the pump last looked: an
    /// attach that fails or succeeds streams nothing, and the bands and
    /// the nav's glyph are drawn from the state alone.
    pub(super) fn server_moved(&mut self) -> bool {
        let now = self.cockpit.server();
        // A restart that times out moves no state but its own.
        let restart = self.cockpit.server_restart().cloned();
        if self.server_bands.seen == now && self.server_bands.restart_seen == restart {
            return false;
        }
        self.server_bands.seen = now;
        self.server_bands.restart_seen = restart;
        true
    }

    /// Say when there is no Server, or offer to restart one running an
    /// older build from this window's executable.
    pub(super) fn present_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.cockpit.server() {
            None => {}
            // Down mid-restart: the restart's own notice says so.
            Some(ServerState::Detached { .. })
                if matches!(
                    self.cockpit.server_restart(),
                    Some(ServerRestart::Restarting)
                ) => {}
            Some(ServerState::Detached { reason, attention }) => {
                self.server_bands.restart_asked = false;
                if std::mem::take(&mut self.server_bands.updating) {
                    window.remove_notification::<Updating>(cx);
                }
                let machine = self
                    .cockpit
                    .machine()
                    .map(|bridge| bridge.name().to_string());
                let gone = match (&machine, attention) {
                    (None, _) => Gone::NoServer,
                    (Some(_), Some(Failure::SignIn)) => Gone::SignIn,
                    (Some(_), Some(_)) => Gone::NotInstalled,
                    (Some(_), None) if self.server_bands.attached => Gone::Lost,
                    // Being reached for the first time: the nav's glyph says so.
                    (Some(_), None) => return,
                };
                if self.server_bands.detached == Some(gone) {
                    return;
                }
                if self.server_bands.detached.replace(gone).is_some() {
                    window.remove_notification::<Detached>(cx);
                }
                let name = machine.unwrap_or_default();
                let (title, message) = match gone {
                    Gone::NoServer => (
                        "No Ferrite server".to_string(),
                        format!(
                            "{reason}. Your agents run in it; Ferrite keeps trying. \
                             Click to try now."
                        ),
                    ),
                    Gone::Lost => (
                        format!("{name} is not connected"),
                        format!(
                            "{reason}. Its agents keep running there; Ferrite reconnects. \
                             Click to try now."
                        ),
                    ),
                    Gone::SignIn => (
                        format!("Sign in to {name}"),
                        "ssh needs you to sign in. Click to sign in in a terminal; \
                         Ferrite connects once you have."
                            .to_string(),
                    ),
                    Gone::NotInstalled => (
                        format!("{name} has no ferrited"),
                        format!(
                            "{reason}. Install it there (cargo install --path \
                             crates/ferrite-core --bin ferrited); Ferrite keeps trying."
                        ),
                    ),
                };
                let view = cx.entity().downgrade();
                window.push_notification(
                    Notification::new()
                        .id::<Detached>()
                        .title(title)
                        .message(message)
                        .with_type(NotificationType::Warning)
                        .autohide(false)
                        .on_click(move |_, _, cx| {
                            view.update(cx, |view, cx| {
                                if gone == Gone::SignIn {
                                    view.sign_in(cx);
                                } else {
                                    view.cockpit.retry_server();
                                }
                                view.server_bands.detached = None;
                                cx.notify();
                            })
                            .ok();
                        }),
                    cx,
                );
            }
            Some(ServerState::Attached { server, since }) => {
                self.server_bands.attached = true;
                if self.server_bands.detached.take().is_some() {
                    window.remove_notification::<Detached>(cx);
                }
                if !self.server_is_older(&server, since) {
                    if std::mem::take(&mut self.server_bands.updating) {
                        window.remove_notification::<Updating>(cx);
                    }
                    self.server_bands.restart_asked = false;
                    return;
                }
                if self.server_bands.restart_asked {
                    return;
                }
                self.server_bands.restart_asked = true;
                self.server_bands.updating = true;
                let view = cx.entity().downgrade();
                window.push_notification(
                    Notification::new()
                        .id::<Updating>()
                        .title("Ferrite’s server is running an older build")
                        .message("Click to restart it on the installed build.")
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

    /// Say how a restart goes, as the window sees it through: restarting,
    /// then restarted on the new build, or why not. The offer to restart is
    /// taken down as soon as one is under way.
    pub(super) fn present_restart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let restart = self.cockpit.server_restart().cloned();
        if restart == self.server_bands.restart {
            return;
        }
        self.server_bands.restart = restart.clone();
        window.remove_notification::<Restarting>(cx);
        let Some(restart) = restart else {
            return;
        };
        if std::mem::take(&mut self.server_bands.updating) {
            window.remove_notification::<Updating>(cx);
        }
        // Each: what it is, what it says, whether it stays up, and whether a
        // click restarts (again).
        let (kind, title, message, stays, restarts) = match restart {
            ServerRestart::Restarting => (
                NotificationType::Info,
                "Restarting Ferrite’s server…".to_string(),
                "Your agents stop for a moment; their Threads resume.".to_string(),
                true,
                false,
            ),
            ServerRestart::Restarted { version } if self.server_build() == Some(Build::Older) => (
                NotificationType::Warning,
                "Ferrite’s server restarted on an older build".to_string(),
                format!("It runs {version}, not the installed build. Click to restart again."),
                true,
                true,
            ),
            ServerRestart::Restarted { version } => (
                NotificationType::Success,
                "Ferrite’s server restarted".to_string(),
                match self.server_build() {
                    Some(Build::Installed) => format!("It runs {version}, the installed build."),
                    _ => format!("It runs {version}."),
                },
                false,
                false,
            ),
            ServerRestart::Failed { reason } => (
                NotificationType::Error,
                "Ferrite’s server did not restart".to_string(),
                format!("{reason}. Click to try again."),
                true,
                true,
            ),
        };
        let view = cx.entity().downgrade();
        window.push_notification(
            Notification::new()
                .id::<Restarting>()
                .title(title)
                .message(message)
                .with_type(kind)
                .autohide(!stays)
                .when(restarts, |notice| {
                    // Down, there is nothing to restart: reach for it again.
                    notice.on_click(move |_, window, cx| {
                        view.update(cx, |view, cx| {
                            if matches!(view.cockpit.server(), Some(ServerState::Attached { .. })) {
                                view.restart_server(window, cx);
                            } else {
                                view.cockpit.retry_server();
                                cx.notify();
                            }
                        })
                        .ok();
                    })
                }),
            cx,
        );
    }

    /// Where the Server stands, in one wording for Settings › About and
    /// Settings › Server: its build and uptime and whether it is the
    /// installed one, restarting, or not running.
    fn server_standing(&self) -> String {
        if matches!(
            self.cockpit.server_restart(),
            Some(ServerRestart::Restarting)
        ) {
            return "restarting…".to_string();
        }
        match self.cockpit.server() {
            None => "This window runs without a separate server".to_string(),
            Some(ServerState::Detached { reason, .. }) => format!("not running \u{b7} {reason}"),
            Some(ServerState::Attached { server, since }) => {
                let up = format!("{} · up {}", server.version, uptime(since.elapsed()));
                match self.server_build() {
                    Some(Build::Older) => {
                        format!("{up} · an older build: restart to use the installed one")
                    }
                    Some(Build::Installed) => format!("{up} · the installed build"),
                    _ => up,
                }
            }
        }
    }

    /// Which build the attached Server runs, as far as this window can
    /// tell: `None` unattached.
    fn server_build(&self) -> Option<Build> {
        let Some(ServerState::Attached { server, since }) = self.cockpit.server() else {
            return None;
        };
        let ours = self.cockpit.machine().is_none()
            && std::env::current_exe().is_ok_and(|exe| exe == server.executable);
        Some(if !ours {
            Build::Other
        } else if self.server_is_older(&server, since) {
            Build::Older
        } else {
            Build::Installed
        })
    }

    /// Settings › About's Server row: where it stands, as it changes, and
    /// the restart onto this build when it is an older one.
    pub(super) fn server_status_row(&self, window: &Window, cx: &Context<Self>) -> prefs::Row {
        let handle = window.window_handle();
        let view = cx.entity().downgrade();
        prefs::action(
            "settings-about-server",
            "Server",
            self.server_standing(),
            SharedString::default(),
            (self.server_build() == Some(Build::Older)).then(|| "Restart".into()),
            move |cx| {
                let view = view.clone();
                let _ = handle.update(cx, |_, window, cx| {
                    view.update(cx, |view, cx| view.restart_server(window, cx))
                });
            },
        )
    }

    /// Take down the bands this view put up: a window going to another
    /// machine leaves them behind with it.
    pub(crate) fn take_server_bands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.server_bands.detached.take().is_some() {
            window.remove_notification::<Detached>(cx);
        }
        if std::mem::take(&mut self.server_bands.updating) {
            window.remove_notification::<Updating>(cx);
        }
        if self.server_bands.restart.take().is_some() {
            window.remove_notification::<Restarting>(cx);
        }
    }

    /// Whether the Server is an older build of this very executable: one
    /// running any other is never asked, since it restarts into itself.
    fn server_is_older(
        &self,
        server: &ferrite_core::protocol::ServerInfo,
        since: std::time::Instant,
    ) -> bool {
        if self.cockpit.machine().is_some() {
            return false;
        }
        std::env::current_exe().is_ok_and(|executable| {
            ferrite_core::server::wants_restart(crate::ferrite_update::VERSION, &executable, server)
                || server.version == crate::ferrite_update::VERSION
                    && std::time::SystemTime::now()
                        .checked_sub(since.elapsed())
                        .is_some_and(|started| {
                            ferrite_core::server::executable_replaced_since(
                                &executable,
                                server,
                                started,
                            )
                        })
        })
    }

    /// Restart the Server now, asking first while an agent works or a
    /// Decision waits: the turn is cut off, and reads interrupted after.
    pub(super) fn restart_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A refused restart is the restart's own notice (`present_restart`),
        // not a second line beside it.
        self.server_act("Restart", window, cx, |view| {
            let _ = view.cockpit.restart_server(When::Now);
            Ok(())
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
    /// Server runs this one: an update stops it first, and waits for it to
    /// end (ADR 0025). Its agents are idle by then, and the relaunched
    /// window starts the new Server, which resumes their Threads.
    pub(super) fn release_executable(&mut self) {
        if cfg!(windows) && self.cockpit.server().is_some() {
            if let Err(error) = self.cockpit.stop_server_and_wait(RELEASE_WAIT) {
                ferrite_core::log!("the server was not stopped for the update: {error}");
            }
        }
    }

    /// Settings › Server, for a window attached to one.
    pub(super) fn server_rows(&self, window: &Window, cx: &Context<Self>) -> Vec<prefs::Row> {
        let Some(state) = self.cockpit.server() else {
            // A sentence, so the row's words (as About says it), not a
            // fact's machine-text value.
            return vec![prefs::action(
                "settings-server-none",
                "Server",
                self.server_standing(),
                SharedString::default(),
                None,
                |_| {},
            )];
        };
        let mut status = self.server_standing();
        if let ServerState::Attached { server, .. } = state {
            let live = self.cockpit.live_sessions(Provider::Claude)
                + self.cockpit.live_sessions(Provider::Codex);
            status.push_str(&match live {
                1 => " · 1 live session".to_string(),
                n => format!(" · {n} live sessions"),
            });
            if std::env::current_exe().is_ok_and(|exe| exe != server.executable) {
                status.push_str(&format!(" · runs {}", server.executable.display()));
            }
        }
        let status: SharedString = status.into();
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
        let mut rows = vec![prefs::fact("Server", status)];
        // A development build never installs the LaunchAgent (main), and
        // must not point the operator's at itself. A machine's Server is
        // not this computer's to start.
        if !crate::titlebar::DEV && self.cockpit.machine().is_none() {
            rows.push(prefs::toggle(
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
            ));
        }
        rows.extend([
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
        ]);
        rows
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
