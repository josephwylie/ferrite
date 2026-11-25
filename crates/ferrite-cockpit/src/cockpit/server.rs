//! The window's Servers (ADR 0025): the band that says a machine's is not
//! there, one per machine; and this computer's own — the restart onto this
//! window's newer build, and Settings › Server, its version, uptime and
//! live Sessions, "Start at login", Restart and Stop. A window on an
//! Engine of its own (`--demo`, `--load`) has none of it.

use super::*;
use ferrite_client::cockpit::{ServerRestart, ServerState};
use ferrite_core::machine::Failure;
use ferrite_core::protocol::When;
use ferrite_core::server::agent;
use ferrite_settings::sheet::ServerInputs;
use gpui::component::notification::{Notification, NotificationType};
use gpui::component::WindowExt as _;
use gpui::PromptLevel;

/// How long an update waits for the Server to end: past the Store's
/// final syncs.
const RELEASE_WAIT: Duration = Duration::from_secs(5);

/// A machine's no-Server band, so attaching again takes it down: one per
/// machine (`band`).
struct Detached;

/// What a no-Server band says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gone {
    /// This computer's Server does not answer.
    NoServer,
    /// A machine went.
    Lost,
    SignIn,
    /// A paired link's session ended there: only a new pairing undoes it.
    Revoked,
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

/// What the window has said about its Servers this run.
#[derive(Default)]
pub(crate) struct ServerBands {
    /// Each machine's no-Server band up, and what it says.
    detached: std::collections::HashMap<MachineId, Gone>,
    /// The machines whose Server answered once: one that goes is lost,
    /// not still being reached.
    attached: std::collections::HashSet<MachineId>,
    /// This computer's offer to restart onto this build is up.
    updating: bool,
    /// The restart onto this build is asked for once per run.
    restart_asked: bool,
    /// Each machine's Server state, and this computer's restart, as the
    /// pump last saw them.
    seen: Vec<(MachineId, Option<ServerState>)>,
    restart_seen: Option<ServerRestart>,
    /// The restart the window last said something about.
    restart: Option<ServerRestart>,
}

#[cfg(test)]
impl ServerBands {
    /// What `machine`'s no-Server band says, while it is up.
    pub(crate) fn detached(&self, machine: MachineId) -> Option<Gone> {
        self.detached.get(&machine).copied()
    }
}

/// `machine`'s no-Server band's key among the window's notifications.
fn band(machine: MachineId) -> gpui::ElementId {
    ("server-band", machine.get() as usize).into()
}

impl CockpitView {
    /// This computer as the window holds it: what Settings › Server, a
    /// restart and an update act on.
    fn home(&self, cx: &gpui::App) -> Option<MachineId> {
        self.cockpit.read(cx).this_computer()
    }

    /// This computer's Server's state; `None` for an Engine of the
    /// window's own, or this computer turned off.
    fn home_server(&self, cx: &gpui::App) -> Option<ServerState> {
        self.home(cx)
            .and_then(|home| self.cockpit.read(cx).server(home))
    }

    /// Whether a machine's Server state moved since the pump last looked:
    /// an attach that fails or succeeds streams nothing, and the bands and
    /// the nav's glyph are drawn from the state alone.
    pub(super) fn server_moved(&mut self, cx: &gpui::App) -> bool {
        let cockpit = self.cockpit.read(cx);
        let now: Vec<(MachineId, Option<ServerState>)> = cockpit
            .machines()
            .map(|machine| (machine, cockpit.server(machine)))
            .collect();
        // A restart that times out moves no state but its own.
        let restart = self
            .home(cx)
            .and_then(|home| self.cockpit.read(cx).server_restart(home).cloned());
        if self.server_bands.seen == now && self.server_bands.restart_seen == restart {
            return false;
        }
        self.server_bands.seen = now;
        self.server_bands.restart_seen = restart;
        true
    }

    /// Say, for each machine, when its Server is not there; and offer to
    /// restart this computer's when it runs an older build from this
    /// window's executable. A band of a machine the window let go goes
    /// with it.
    pub(super) fn present_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let machines: Vec<MachineId> = self.cockpit.read(cx).machines().collect();
        let gone: Vec<MachineId> = self
            .server_bands
            .detached
            .keys()
            .filter(|machine| !machines.contains(machine))
            .copied()
            .collect();
        for machine in gone {
            self.server_bands.detached.remove(&machine);
            window.remove_notification1::<Detached>(band(machine), cx);
        }
        self.server_bands
            .attached
            .retain(|machine| machines.contains(machine));
        for machine in machines {
            self.present_machine(machine, window, cx);
        }
    }

    fn present_machine(&mut self, machine: MachineId, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .cockpit
            .read(cx)
            .bridge(machine)
            .map(|bridge| bridge.name().to_string());
        let local = name.is_none();
        match self.cockpit.read(cx).server(machine) {
            None => {}
            // Down mid-restart: the restart's own notice says so.
            Some(ServerState::Detached { .. })
                if local
                    && matches!(
                        self.cockpit.read(cx).server_restart(machine),
                        Some(ServerRestart::Restarting)
                    ) => {}
            Some(ServerState::Detached { reason, attention }) => {
                if local {
                    self.server_bands.restart_asked = false;
                    if std::mem::take(&mut self.server_bands.updating) {
                        window.remove_notification::<Updating>(cx);
                    }
                }
                let gone = match (&name, attention) {
                    (None, _) => Gone::NoServer,
                    (Some(_), Some(Failure::SignIn)) => Gone::SignIn,
                    (Some(_), Some(Failure::Revoked)) => Gone::Revoked,
                    (Some(_), Some(_)) => Gone::NotInstalled,
                    (Some(_), None) if self.server_bands.attached.contains(&machine) => Gone::Lost,
                    // Being reached for the first time: the nav's glyph says so.
                    (Some(_), None) => return,
                };
                if self.server_bands.detached.get(&machine) == Some(&gone) {
                    return;
                }
                if self.server_bands.detached.insert(machine, gone).is_some() {
                    window.remove_notification1::<Detached>(band(machine), cx);
                }
                let name = name.unwrap_or_default();
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
                    Gone::Revoked => (
                        format!("Sign in to {name}"),
                        "This machine no longer takes this session. Click to pair it \
                         again; Ferrite connects once you have."
                            .to_string(),
                    ),
                    Gone::NotInstalled => (
                        format!("{name} has no ferrited"),
                        format!(
                            "{reason}. Install it there (cargo install --path \
                             crates/ferrite-server --bin ferrited); Ferrite keeps trying."
                        ),
                    ),
                };
                let view = cx.entity().downgrade();
                window.push_notification(
                    Notification::new()
                        .id1::<Detached>(band(machine))
                        .title(title)
                        .message(message)
                        .with_type(NotificationType::Warning)
                        .autohide(false)
                        .on_click(move |_, window, cx| {
                            view.update(cx, |view, cx| {
                                if matches!(gone, Gone::SignIn | Gone::Revoked) {
                                    view.sign_in(machine, window, cx);
                                } else {
                                    view.cockpit
                                        .update(cx, |cockpit, _| cockpit.retry_server(machine));
                                }
                                view.server_bands.detached.remove(&machine);
                                cx.notify();
                            })
                            .ok();
                        }),
                    cx,
                );
            }
            Some(ServerState::Attached { server, since, .. }) => {
                self.server_bands.attached.insert(machine);
                if self.server_bands.detached.remove(&machine).is_some() {
                    window.remove_notification1::<Detached>(band(machine), cx);
                }
                if !local {
                    return;
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
        let restart = self
            .home(cx)
            .and_then(|home| self.cockpit.read(cx).server_restart(home).cloned());
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
            ServerRestart::Restarted { version } if self.server_build(cx) == Some(Build::Older) => (
                NotificationType::Warning,
                "Ferrite’s server restarted on an older build".to_string(),
                format!("It runs {version}, not the installed build. Click to restart again."),
                true,
                true,
            ),
            ServerRestart::Restarted { version } => (
                NotificationType::Success,
                "Ferrite’s server restarted".to_string(),
                match self.server_build(cx) {
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
                            if matches!(view.home_server(cx), Some(ServerState::Attached { .. })) {
                                view.restart_server(window, cx);
                            } else if let Some(home) = view.home(cx) {
                                view.cockpit.update(cx, |cockpit, _| cockpit.retry_server(home));
                                cx.notify();
                            }
                        })
                        .ok();
                    })
                }),
            cx,
        );
    }

    /// Where this computer's Server stands, as Settings › Server's
    /// `Status` row says it: its build and uptime and whether it is the
    /// installed one, restarting, or not running. (About does not repeat
    /// it: one place per fact.)
    fn server_standing(&self, cx: &gpui::App) -> String {
        let Some(home) = self.home(cx) else {
            return "This computer is turned off in Settings › Connections".to_string();
        };
        if matches!(
            self.cockpit.read(cx).server_restart(home),
            Some(ServerRestart::Restarting)
        ) {
            return "restarting…".to_string();
        }
        match self.cockpit.read(cx).server(home) {
            None => "This window runs without a separate server".to_string(),
            Some(ServerState::Detached { reason, .. }) => format!("not running \u{b7} {reason}"),
            Some(ServerState::Attached { server, since, .. }) => {
                let up = format!("{} · up {}", server.version, uptime(since.elapsed()));
                match self.server_build(cx) {
                    Some(Build::Older) => {
                        format!("{up} · an older build: restart to use the installed one")
                    }
                    Some(Build::Installed) => format!("{up} · the installed build"),
                    _ => up,
                }
            }
        }
    }

    /// Which build this computer's attached Server runs, as far as this
    /// window can tell: `None` unattached.
    fn server_build(&self, cx: &gpui::App) -> Option<Build> {
        let Some(ServerState::Attached { server, since, .. }) = self.home_server(cx) else {
            return None;
        };
        let ours = std::env::current_exe().is_ok_and(|exe| exe == server.executable);
        Some(if !ours {
            Build::Other
        } else if self.server_is_older(&server, since) {
            Build::Older
        } else {
            Build::Installed
        })
    }

    /// Whether this computer's Server is an older build of this very
    /// executable: one running any other is never asked, since it restarts
    /// into itself.
    fn server_is_older(
        &self,
        server: &ferrite_core::protocol::ServerInfo,
        since: std::time::Instant,
    ) -> bool {
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

    /// Restart this computer's Server now, asking first while an agent
    /// works or a Decision waits: the turn is cut off, and reads
    /// interrupted after.
    pub(super) fn restart_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A refused restart is the restart's own notice (`present_restart`),
        // not a second line beside it.
        self.server_act("Restart", window, cx, |cockpit, home| {
            let _ = cockpit.restart_server(home, When::Now);
            Ok(())
        });
    }

    /// Stop this computer's Server, asking first as a restart does. Its
    /// open Threads resume when a window starts it again.
    pub(super) fn stop_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.server_act("Stop", window, cx, |cockpit, home| {
            cockpit.stop_server(home, When::Now)
        });
    }

    fn server_act(
        &mut self,
        verb: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
        act: impl FnOnce(&mut Cockpit, MachineId) -> std::io::Result<()> + 'static,
    ) {
        let Some(home) = self.home(cx) else {
            return;
        };
        let done = move |view: &mut Self, result: std::io::Result<()>| {
            if let Err(error) = result {
                view.nav_notice.problem(format!("the server did not {verb}: {error}"));
            }
        };
        let Some(busy) = self.in_flight(|machine| machine == home, cx) else {
            let result = self.cockpit.update(cx, |cockpit, _| act(cockpit, home));
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
                    let result = view.cockpit.update(cx, |cockpit, _| act(cockpit, home));
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
    pub(super) fn release_executable(&mut self, cx: &mut gpui::App) {
        let Some(home) = self.home(cx) else {
            return;
        };
        if cfg!(windows) && self.cockpit.read(cx).server(home).is_some() {
            if let Err(error) = self
                .cockpit
                .update(cx, |cockpit, _| cockpit.stop_server_and_wait(home, RELEASE_WAIT))
            {
                ferrite_core::log!("the server was not stopped for the update: {error}");
            }
        }
    }

    /// Settings › Server: this computer's, for a window attached to it.
    pub(super) fn server_inputs(&self, cx: &gpui::App) -> ServerInputs {
        let (Some(home), Some(state)) = (self.home(cx), self.home_server(cx)) else {
            return ServerInputs::None(self.server_standing(cx).into());
        };
        let mut status = self.server_standing(cx);
        if let ServerState::Attached { server, .. } = state {
            let live: usize = Provider::ALL
                .into_iter()
                .map(|provider| self.cockpit.read(cx).live_sessions(home, provider))
                .sum();
            status.push_str(&match live {
                1 => " · 1 live session".to_string(),
                n => format!(" · {n} live sessions"),
            });
            if std::env::current_exe().is_ok_and(|exe| exe != server.executable) {
                status.push_str(&format!(" · runs {}", server.executable.display()));
            }
        }
        ServerInputs::Home(status.into())
    }

    /// Settings › Server's Start at login: saved, and the LaunchAgent
    /// installed or removed.
    pub(super) fn set_start_at_login(&mut self, on: bool, cx: &mut Context<Self>) {
        self.change_settings(|settings| settings.start_at_login = on, cx);
        if let Err(error) = std::env::current_exe().and_then(|exe| agent::start_at_login(&exe, on)) {
            self.nav_notice
                .problem(format!("start at login not changed: {error}"));
        }
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
