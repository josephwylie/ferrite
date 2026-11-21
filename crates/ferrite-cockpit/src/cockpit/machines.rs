//! The window's machines (ADR 0026): their headings and their Threads in
//! the nav, the window going to another one, what a machine that went
//! looks like, what the Inbox says of the machines off screen, and Settings
//! › Machines. A window with only this computer draws none of it.

use super::*;
use crate::machines::{MachineRow, Machines, OffScreen};
use crate::notifications::{Alert, Detail, State};
use ferrite_core::cockpit::ServerState;
use ferrite_core::notifications::Notifications;
use gpui::App;

/// What a lost machine still shows (ADR 0026): `element` dimmed under a
/// cover, `id`, that takes every press until the machine is back.
pub(super) fn held(element: Div, id: &'static str) -> Div {
    element
        .relative()
        .opacity(ferrite_ui::theme::MACHINE_LOST_OPACITY)
        .child(
            div()
                .id(id)
                .debug_selector(move || id.into())
                .absolute()
                .inset_0()
                .occlude(),
        )
}

impl CockpitView {
    /// Every machine for the nav, when there is more than this computer.
    pub(super) fn machine_rows(&self, cx: &App) -> Option<Vec<MachineRow>> {
        let machines = self.machines.as_ref()?.read(cx);
        machines
            .listed()
            .then(|| machines.rows(self.cockpit.server()))
    }

    /// Remote and empty clients must never substitute local files or defaults.
    pub(super) fn on_machine(&self) -> bool {
        !self.cockpit.environment_available() || self.cockpit.machine().is_some()
    }

    /// The window is on a machine whose Server is not there: what it shows
    /// stays, dimmed, and takes no input until it is back.
    pub(super) fn machine_lost(&self) -> bool {
        self.cockpit.environment_available()
            && self.on_machine()
            && matches!(self.cockpit.server(), Some(ServerState::Detached { .. }))
    }

    /// Take what the machines off screen said.
    pub(super) fn poll_machines(&mut self, cx: &mut Context<Self>) {
        if let Some(machines) = &self.machines {
            machines.update(cx, |machines, cx| {
                if machines.poll() {
                    cx.notify();
                }
            });
        }
    }

    /// Go to the machine an alert is about and open it there (a toast's
    /// or a row's open, a system notification's click). The window builds
    /// a Cockpit on it, which opens the alert once attached (`land`).
    pub(super) fn go_to_alert(&mut self, alert: Alert, cx: &mut Context<Self>) {
        let index = alert.away.as_ref().and_then(|key| {
            self.machines
                .as_ref()
                .and_then(|machines| machines.read(cx).index(key))
        });
        if let Some(index) = index {
            cx.emit(GoToMachine {
                index: Some(index),
                open: Some(Landing::Alert(Alert {
                    away: None,
                    ..alert
                })),
            });
        }
    }

    /// Whether the Cockpit's Server answers (or it runs an Engine of its
    /// own): what it mirrors is the Server's.
    pub(super) fn attached(&self) -> bool {
        self.cockpit
            .server()
            .is_none_or(|server| matches!(server, ServerState::Attached { .. }))
    }

    /// Open what the window came to this machine for, once its Server is
    /// there: an alert while it still stands, else its Thread (revived
    /// when parked). A machine that does not answer by the landing's
    /// deadline opens nothing: the operator has moved on.
    pub(super) fn land(&mut self, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        let expired = self.landing.as_ref().is_some_and(|(_, until)| now > *until);
        if !expired && !self.attached() {
            return;
        }
        match self.landing.take().filter(|_| !expired) {
            Some((Landing::Alert(alert), _))
                if alert.target.wants(self.cockpit.notifications()) =>
            {
                self.notice_verb(alert.open_verb(), cx)
            }
            Some((Landing::Alert(alert), _)) => {
                self.notice_verb(Verb::OpenThread(alert.thread), cx);
            }
            None => {}
        }
    }

    /// The Inbox's rows for the machines off screen: their live requests,
    /// then their finished turns with when each finished, each named by
    /// its machine.
    pub(super) fn away_rows(
        &self,
        cx: &App,
    ) -> (Vec<NoticeRow>, Vec<(std::time::SystemTime, NoticeRow)>) {
        let now = ferrite_core::clock::system_time();
        let (mut requests, mut finished) = (Vec::new(), Vec::new());
        let Some(machines) = &self.machines else {
            return (requests, finished);
        };
        for away in machines.read(cx).off_screen() {
            let mut statuses = away.status.rows();
            statuses.sort_by_key(|status| {
                away.notifications
                    .first_request(status.thread)
                    .map(|notice| notice.seq)
                    .unwrap_or(u64::MAX)
            });
            for status in statuses {
                if away.status.snooze(status.thread).is_some() {
                    continue;
                }
                if status.waiting || away.notifications.first_request(status.thread).is_some() {
                    if let Some(notice) = away.notifications.first_request(status.thread) {
                        requests.extend(away_row(
                            &away,
                            &RowTarget::Decision(notice.id.clone()),
                            now,
                        ));
                        continue;
                    }
                }
                if !status.working || status.waiting {
                    if let Some(row) = away_row(&away, &RowTarget::Thread(status.thread), now) {
                        finished.push((now, row));
                    }
                }
            }
        }
        (requests, finished)
    }

    pub(super) fn fold_machine(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(machines) = &self.machines {
            machines.update(cx, |machines, cx| {
                machines.toggle_fold(index);
                cx.notify();
            });
        }
        cx.notify();
    }

    /// Sign in to the machine the window is on, in a terminal.
    pub(super) fn sign_in(&mut self, cx: &mut Context<Self>) {
        let Some(bridge) = self.cockpit.machine() else {
            return;
        };
        if let Err(error) = bridge.sign_in() {
            self.nav_notice.problem(format!("cannot sign in: {error}"));
            cx.notify();
        }
    }

    /// Move a disabled selection to the first enabled environment, or none.
    pub(super) fn machines_changed(&mut self, cx: &mut Context<Self>) {
        let saved = self.prefs.settings.machines.clone();
        let Some(machines) = &self.machines else {
            return;
        };
        let local_enabled = self.prefs.settings.local_environment_enabled;
        let turned_off = machines.update(cx, |machines, cx| {
            machines.set_local_enabled(local_enabled);
            cx.notify();
            machines.settings_changed(&saved)
        });
        if turned_off {
            cx.emit(GoToMachine {
                index: machines.read(cx).fallback(),
                open: None,
            });
        }
    }
}

impl CockpitView {
    /// Ask the machine's Server what `project`'s repo holds for the
    /// workspace popover (`list_repo`): git there, never here (ADR 0026).
    /// An older Server cannot say, and the popover offers the fresh starts
    /// alone.
    pub(super) fn ask_machine_repo(&mut self, project: ProjectId) {
        let reply = self.cockpit.list_repo(project).ok();
        self.checkouts.listen_machine(project, reply);
    }

    /// Take the machine's answer to `ask_machine_repo`, once it lands: the
    /// popover's rows, and the branch its checkout is on, which no git
    /// here could name — `main` when it cannot say, as for a local Project.
    /// With nothing on its way, ask about a draft's Project whose branch
    /// the chip has no answer for yet.
    pub(super) fn poll_machine_repo(&mut self, cx: &mut Context<Self>) {
        match self.checkouts.machine_listed() {
            None => self.ask_drafted_checkout(),
            Some(false) => {}
            Some(true) => {
                self.sync_workspace_rows(cx);
                cx.notify();
            }
        }
    }

    /// A draft's workspace chip names its checkout's branch, which on a
    /// machine only its Server can read: ask about the first drafted
    /// Project it has no answer for.
    fn ask_drafted_checkout(&mut self) {
        if !self.on_machine() || self.machine_lost() {
            return;
        }
        let unknown = self
            .panes
            .iter()
            .filter_map(|pane| pane.draft())
            .map(|draft| draft.binding.project())
            .find(|project| self.checkouts.branch(*project).is_none());
        if let Some(project) = unknown {
            self.ask_machine_repo(project);
        }
    }
}

/// Whether an alert's record still wants the operator, on its machine:
/// `here` (once attached), or the machine off screen it is on. `None`
/// while that machine has not been heard from (`NeedsYou::ring`).
pub(super) fn wanted(
    alert: &Alert,
    here: Option<&Notifications>,
    machines: Option<&Machines>,
) -> Option<bool> {
    let notifications = match &alert.away {
        None => here?,
        Some(key) => machines?.away(key)?.notifications,
    };
    Some(alert.target.wants(notifications))
}

/// A row about a machine off screen, worded from what its status link
/// hears and titled with the machine's name. It hears no transcript, so a
/// finish says only whether it failed.
pub(super) fn away_row(
    away: &OffScreen<'_>,
    target: &RowTarget,
    now: std::time::SystemTime,
) -> Option<NoticeRow> {
    let (thread, state, detail, at, read) = match target {
        RowTarget::Thread(thread) => {
            if let Some(notice) = away
                .notifications
                .notices()
                .find(|notice| notice.thread == *thread)
            {
                let mut row = away_row(away, &RowTarget::Notice(notice.id), now)?;
                row.target = target.clone();
                return Some(row);
            }
            (*thread, State::Idle, Detail::None, now, true)
        }
        RowTarget::Notice(id) => {
            let notice = away.notifications.get(*id)?;
            let (state, detail) = match &notice.outcome {
                ferrite_core::TurnOutcome::Error(error) => (
                    State::Failed,
                    Detail::Words(
                        ferrite_core::progress::one_line(error, ferrite_core::progress::ROW_CHARS)
                            .into(),
                    ),
                ),
                _ => (State::Done, Detail::None),
            };
            (notice.thread, state, detail, notice.at, notice.read)
        }
        RowTarget::Decision(id) => {
            let notice = away.notifications.decision(id).or_else(|| {
                away.notifications
                    .first_request(id.thread)
                    .filter(|notice| notice.id == *id)
            })?;
            let detail = away
                .status
                .decision(id.thread, &id.handle)
                .and_then(|decision| ferrite_core::Decision::try_from(decision).ok())
                .map(|decision| {
                    crate::notifications::request_detail(&decision, notice.kind, None, || None)
                })
                .unwrap_or(Detail::None);
            (
                id.thread,
                State::NeedsYou(notice.kind),
                detail,
                notice.at,
                notice.read,
            )
        }
    };
    let title = away
        .status
        .title(thread)
        .filter(|title| !title.is_empty())
        .unwrap_or(crate::facts::NEW_THREAD);
    Some(NoticeRow {
        away: Some(away.key.clone()),
        target: target.clone(),
        thread,
        title: crate::notifications::away_title(&away.name, title),
        state,
        detail,
        when: crate::facts::since_label(at, now),
        read,
    })
}
