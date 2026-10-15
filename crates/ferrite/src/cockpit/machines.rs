//! The window's machines (ADR 0026): their headings and their Threads in
//! the nav, the window going to another one, what a machine that went
//! looks like, what the bell says of the machines off screen, and Settings
//! › Machines. A window with only this computer draws none of it.

use std::rc::Rc;

use super::*;
use crate::machines::{Connection, MachineRow, Machines, OffScreen};
use crate::notifications::{Alert, Detail, State};
use ferrite_core::cockpit::ServerState;
use ferrite_core::machine::{self, Machine};
use ferrite_core::notifications::Notifications;
use gpui::App;

/// What a lost machine still shows (ADR 0026): `element` dimmed under a
/// cover, `id`, that takes every press until the machine is back.
pub(super) fn held(element: Div, id: &'static str) -> Div {
    element
        .relative()
        .opacity(crate::theme::MACHINE_LOST_OPACITY)
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

    /// The window is on a machine whose Server is not there: what it shows
    /// stays, dimmed, and takes no input until it is back.
    pub(super) fn machine_lost(&self) -> bool {
        self.cockpit.machine().is_some()
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

    /// A machine's heading: its press goes to the machine, or folds the
    /// one the window is on; its triangle folds either.
    pub(super) fn machine_heading(&self, row: &MachineRow, cx: &mut Context<Self>) -> AnyElement {
        let (index, selected) = (row.index, row.selected);
        let fold = nav::machine_fold(index, row.folded).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                view.fold_machine(index, cx);
            }),
        );
        nav::machine_heading(row, fold)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    if selected {
                        view.fold_machine(index, cx);
                    } else {
                        cx.emit(GoToMachine { index, open: None });
                    }
                }),
            )
            .into_any_element()
    }

    /// Every machine the window is not on: its heading and, unfolded, the
    /// Threads it has open. A press on either goes to the machine.
    pub(super) fn machines_off_screen(
        &self,
        rows: &[MachineRow],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut drawn = Vec::new();
        for row in rows.iter().filter(|row| !row.selected) {
            drawn.push(self.machine_heading(row, cx));
            if row.folded {
                continue;
            }
            let index = row.index;
            let lost = row.connection != Connection::Online;
            drawn.extend(row.threads.iter().map(|thread| {
                nav::machine_thread_row(index, thread, lost)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, _: &MouseDownEvent, _, cx| {
                            cx.emit(GoToMachine { index, open: None });
                        }),
                    )
                    .into_any_element()
            }));
        }
        drawn
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
                index,
                open: Some(Alert {
                    away: None,
                    ..alert
                }),
            });
        }
    }

    /// Open what the window came to this machine for, once its Server is
    /// there: the alert while it still stands, else its Thread while it is
    /// open.
    pub(super) fn land(&mut self, cx: &mut Context<Self>) {
        let cockpit = &self.cockpit;
        let Some(alert) = self.landing.take_if(|_| {
            cockpit
                .server()
                .is_none_or(|server| matches!(server, ServerState::Attached { .. }))
        }) else {
            return;
        };
        if alert.target.wants(self.cockpit.notifications()) {
            self.notice_verb(alert.open_verb(), cx);
        } else if self.cockpit.thread(alert.thread).is_some() {
            self.land_on_thread(alert.thread, cx);
        }
    }

    /// The bell's rows for the machines off screen: their live requests,
    /// then their finished turns, each named by its machine.
    pub(super) fn away_rows(&self, cx: &App) -> (Vec<NoticeRow>, Vec<NoticeRow>) {
        let now = ferrite_core::clock::system_time();
        let (mut requests, mut finished) = (Vec::new(), Vec::new());
        let Some(machines) = &self.machines else {
            return (requests, finished);
        };
        for away in machines.read(cx).off_screen() {
            requests.extend(
                away.notifications
                    .decisions()
                    .take(50)
                    .filter_map(|notice| {
                        away_row(&away, &RowTarget::Decision(notice.id.clone()), now)
                    }),
            );
            finished.extend(
                away.notifications
                    .notices()
                    .take(50)
                    .filter_map(|notice| away_row(&away, &RowTarget::Notice(notice.id), now)),
            );
        }
        (requests, finished)
    }

    /// The unread live requests on the machines off screen: what the
    /// bell's badge adds to the ones here.
    pub(super) fn away_unread_requests(&self, cx: &App) -> usize {
        self.machines.as_ref().map_or(0, |machines| {
            machines
                .read(cx)
                .off_screen()
                .map(|away| away.notifications.unread_requests())
                .sum()
        })
    }

    fn fold_machine(&mut self, index: usize, cx: &mut Context<Self>) {
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
            self.group_error = Some(format!("cannot sign in: {error}").into());
            cx.notify();
        }
    }

    /// Follow Settings' machines. Turning off the one the window is on
    /// puts the window on this computer: a turned-off machine is never
    /// connected to.
    pub(super) fn machines_changed(&mut self, cx: &mut Context<Self>) {
        let saved = self.prefs.settings.machines.clone();
        let Some(machines) = &self.machines else {
            return;
        };
        let turned_off = machines.update(cx, |machines, cx| {
            cx.notify();
            machines.settings_changed(&saved)
        });
        if turned_off {
            cx.emit(GoToMachine {
                index: 0,
                open: None,
            });
        }
    }

    /// Settings › Machines: each machine — sign in, on or off, remove (or
    /// save, for the `--remote` one) — and a field to add one by its ssh
    /// target. `None` for a window on an Engine of its own.
    pub(super) fn machine_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<prefs::Group> {
        let machines = self.machines.clone()?;
        let field = self
            .machine_field
            .get_or_insert_with(|| {
                cx.new(|cx| {
                    gpui::component::input::InputState::new(window, cx).placeholder("me@host")
                })
            })
            .clone();
        let view = cx.entity().downgrade();
        let act = |change: Box<dyn Fn(&mut Vec<Machine>)>| -> prefs::Act {
            let view = view.clone();
            Rc::new(move |cx: &mut gpui::App| {
                let _ = view.update(cx, |view, cx| {
                    view.change_settings(|settings| change(&mut settings.machines), cx);
                });
            })
        };
        let listed: Vec<(MachineRow, Machine)> = {
            let machines = machines.read(cx);
            machines
                .rows(self.cockpit.server())
                .into_iter()
                .filter_map(|row| Some((row.clone(), machines.machine(row.index)?.clone())))
                .collect()
        };
        let unsaved = machines.read(cx).unsaved().cloned();
        let mut shown = Vec::new();
        for (row, machine) in listed {
            let target = machine.target.clone();
            let mut acts: Vec<(SharedString, prefs::Act)> = Vec::new();
            if row.connection == Connection::Attention {
                let index = row.index;
                let view = view.clone();
                acts.push((
                    "sign in".into(),
                    Rc::new(move |cx: &mut gpui::App| {
                        let _ = view.update(cx, |view, cx| {
                            let signed = view.machines.as_ref().map(|machines| {
                                machines.update(cx, |machines, _| machines.sign_in(index))
                            });
                            if let Some(Err(error)) = signed {
                                view.group_error = Some(format!("cannot sign in: {error}").into());
                            }
                            cx.notify();
                        });
                    }),
                ));
            }
            if unsaved.as_ref() == Some(&machine) {
                let saving = machine.clone();
                acts.push((
                    "save".into(),
                    act(Box::new(move |saved| saved.push(saving.clone()))),
                ));
            } else {
                let (flip, word) = if machine.enabled {
                    (false, crate::theme::words::OFF)
                } else {
                    (true, "on")
                };
                let flipped = target.clone();
                acts.push((
                    format!("turn {word}").into(),
                    act(Box::new(move |saved| {
                        for machine in saved.iter_mut().filter(|m| m.target == flipped) {
                            machine.enabled = flip;
                        }
                    })),
                ));
                let removed = target.clone();
                acts.push((
                    "remove".into(),
                    act(Box::new(move |saved| saved.retain(|m| m.target != removed))),
                ));
            }
            shown.push(prefs::named(
                "settings-machine",
                row.index,
                row.name.clone().into(),
                connection_hint(&target, row.connection).into(),
                acts,
            ));
        }
        let adding = view.clone();
        let typed = field.clone();
        shown.push(prefs::entry(
            "settings-machine-add",
            "Add a machine",
            "Its ssh target. It needs `ferrited` on its PATH; ssh signs in as it \
             always does, and Ferrite keeps no password.",
            field,
            "add",
            move |window, cx| {
                let target = typed.read(cx).value().trim().to_string();
                let _ = adding.update(cx, |view, cx| match machine::check_target(&target) {
                    Ok(()) => {
                        view.change_settings(
                            |settings| {
                                if !settings.machines.iter().any(|m| m.target == target) {
                                    settings.machines.push(Machine::new(&target));
                                }
                            },
                            cx,
                        );
                        view.group_error = None;
                    }
                    Err(error) => view.group_error = Some(error.into()),
                });
                typed.update(cx, |field, cx| field.set_value("", window, cx));
            },
        ));
        Some(prefs::Group::new(Some("Machines")).rows(shown))
    }
}

/// A machine's hint in Settings: its target and how it is connected.
fn connection_hint(target: &str, connection: Connection) -> String {
    let state = match connection {
        Connection::Online => "connected",
        Connection::Connecting => crate::theme::words::CONNECTING,
        Connection::Attention => crate::theme::words::NEEDS_YOU,
        Connection::Disabled => crate::theme::words::OFF,
    };
    format!("{target} · {state}")
}

/// Whether an alert's record still wants the operator, on its machine:
/// `here`, or the machine off screen it is on while that is heard from.
pub(super) fn wanted(alert: &Alert, here: &Notifications, machines: Option<&Machines>) -> bool {
    match &alert.away {
        None => alert.target.wants(here),
        Some(key) => machines
            .and_then(|machines| machines.away(key))
            .is_some_and(|away| alert.target.wants(away.notifications)),
    }
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
            let notice = away.notifications.decision(id)?;
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
        when: crate::facts::age_label(at, now),
        read,
        folded: Vec::new(),
    })
}
