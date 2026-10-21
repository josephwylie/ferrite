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
use ferrite_core::workspace::Listing;
use gpui::App;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

/// An edit to the saved machines list.
type SavedChange = Box<dyn Fn(&mut Vec<Machine>)>;

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

    /// The window is on a machine: the paths it shows are the machine's,
    /// so no git or folder of this computer answers for them — the Server
    /// does, or nothing.
    pub(super) fn on_machine(&self) -> bool {
        self.cockpit.machine().is_some()
    }

    /// The window is on a machine whose Server is not there: what it shows
    /// stays, dimmed, and takes no input until it is back.
    pub(super) fn machine_lost(&self) -> bool {
        self.on_machine() && matches!(self.cockpit.server(), Some(ServerState::Detached { .. }))
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
            Some((Landing::Alert(Alert { thread, .. }), _)) => self.land_on_thread(thread, cx),
            None => {}
        }
    }

    /// The bell's rows for the machines off screen: their live requests,
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
            requests.extend(
                away.notifications
                    .decisions()
                    .take(50)
                    .filter_map(|notice| {
                        away_row(&away, &RowTarget::Decision(notice.id.clone()), now)
                    }),
            );
            finished.extend(away.notifications.notices().take(50).filter_map(|notice| {
                Some((
                    notice.at,
                    away_row(&away, &RowTarget::Notice(notice.id), now)?,
                ))
            }));
        }
        (requests, finished)
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
        let act = |change: SavedChange| -> prefs::Act {
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

impl CockpitView {
    /// Ask the machine's Server what `project`'s repo holds for the
    /// workspace popover (`list_repo`): git there, never here (ADR 0026).
    /// An older Server cannot say, and the popover offers the fresh starts
    /// alone.
    pub(super) fn ask_machine_repo(&mut self, project: ProjectId) {
        self.asking_repo = self
            .cockpit
            .list_repo(project)
            .ok()
            .map(|answer| (project, answer));
    }

    /// Take the machine's answer to `ask_machine_repo`, once it lands: the
    /// popover's rows, and the branch its checkout is on, which no git
    /// here could name — `main` when it cannot say, as for a local Project.
    /// With nothing on its way, ask about a draft's Project whose branch
    /// the chip has no answer for yet.
    pub(super) fn poll_machine_repo(&mut self, cx: &mut Context<Self>) {
        let Some((project, answer)) = &self.asking_repo else {
            self.ask_drafted_checkout();
            return;
        };
        let project = *project;
        let listing = match answer.try_recv() {
            Err(TryRecvError::Empty) => return,
            Ok(listing) => listing.unwrap_or_default(),
            Err(TryRecvError::Disconnected) => RepoListing::default(),
        };
        let branch = listing.checked_out.as_deref().unwrap_or(MAIN_BRANCH);
        self.checked_out
            .borrow_mut()
            .insert(project, SharedString::from(branch.to_string()));
        self.repo_listing = Some((project, listing));
        self.asking_repo = None;
        self.sync_workspace_rows(cx);
        cx.notify();
    }

    /// A draft's workspace chip names its checkout's branch, which on a
    /// machine only its Server can read: ask about the first drafted
    /// Project it has no answer for.
    fn ask_drafted_checkout(&mut self) {
        if !self.on_machine() || self.machine_lost() {
            return;
        }
        let unknown = {
            let known = self.checked_out.borrow();
            self.panes
                .iter()
                .filter_map(PaneView::draft)
                .map(|draft| draft.binding.project())
                .find(|project| !known.contains_key(project))
        };
        if let Some(project) = unknown {
            self.ask_machine_repo(project);
        }
    }
}

/// The Project card's path field on a machine (ADR 0026), where no folder
/// dialog can browse: what is typed completes against the directory it
/// names there, listed by the machine's Server (`list_dir`).
pub(super) struct RemotePath {
    pub(super) field: Entity<Composer>,
    /// The listing the completions come from, of the directory as typed:
    /// empty when it could not be read, which is not asked again.
    listed: Option<(String, Listing)>,
    /// A listing asked for and not answered yet, of the directory as typed.
    asking: Option<(String, Receiver<std::io::Result<Listing>>)>,
}

/// Where the field starts: the machine's home, which its Server spells out.
const REMOTE_HOME: &str = "~/";

/// How many completions the card lists under the field.
const REMOTE_COMPLETIONS: usize = 8;

/// `path` as the machine's Server spelled it out in the listing in hand —
/// of it, or of the directory it is in — so `~` stages as the machine's
/// home, named for its leaf, and one directory typed two ways stages
/// once. As typed otherwise: the Server spells `~` out when it registers.
fn spelled_out(remote: &RemotePath, path: &str) -> PathBuf {
    let (directory, leaf) = split_typed(path);
    match &remote.listed {
        Some((listed, listing)) if listed.trim_end_matches('/') == path => listing.path.clone(),
        Some((listed, listing)) if listed == directory => listing.path.join(leaf),
        _ => path.into(),
    }
}

/// A typed path's directory, to list, and the start of the name in it, to
/// complete: `~/src/fe` is `~/src/` and `fe`.
fn split_typed(typed: &str) -> (&str, &str) {
    match typed.rfind('/') {
        Some(at) => typed.split_at(at + 1),
        None => ("", typed),
    }
}

impl CockpitView {
    /// The path field a new card gets on a machine; none on this computer,
    /// whose card browses with the system's folder dialog.
    pub(super) fn remote_path(&mut self, cx: &mut Context<Self>) -> Option<RemotePath> {
        if !self.on_machine() {
            return None;
        }
        let field = cx.new(Composer::new);
        field.update(cx, |field, cx| {
            field.set_role(crate::composer::Role::Field(None), cx);
            field.set(REMOTE_HOME.into(), cx);
        });
        cx.subscribe(&field, |view, _, _: &Edited, cx| view.list_remote_path(cx))
            .detach();
        Some(RemotePath {
            field,
            listed: None,
            asking: None,
        })
    }

    /// The open card's path field, on a machine.
    fn open_remote_path(&self) -> Option<&RemotePath> {
        self.project_editor.as_ref()?.remote.as_ref()
    }

    fn open_remote_path_mut(&mut self) -> Option<&mut RemotePath> {
        self.project_editor.as_mut()?.remote.as_mut()
    }

    /// List the directory the open card's field names — on opening, and
    /// each time it moves — unless that listing is in hand or on its way.
    pub(super) fn list_remote_path(&mut self, cx: &mut Context<Self>) {
        let Some(remote) = self.open_remote_path() else {
            return;
        };
        let typed = remote.field.read(cx).text().to_string();
        let (directory, _) = split_typed(&typed);
        let known = |held: Option<&String>| held.is_some_and(|held| held == directory);
        if known(remote.listed.as_ref().map(|(at, _)| at))
            || known(remote.asking.as_ref().map(|(at, _)| at))
        {
            return;
        }
        let asking = if directory.is_empty() {
            None
        } else {
            self.cockpit
                .list_dir(std::path::Path::new(directory))
                .ok()
                .map(|answer| (directory.to_string(), answer))
        };
        if let Some(remote) = self.open_remote_path_mut() {
            remote.asking = asking;
        }
        cx.notify();
    }

    /// Take the machine's answer to the field's listing, once it lands. A
    /// directory that cannot be read lists nothing; one whose answer the
    /// lost connection took is asked again at the next keystroke.
    pub(super) fn poll_remote_path(&mut self, cx: &mut Context<Self>) {
        let Some(remote) = self.open_remote_path_mut() else {
            return;
        };
        let Some((directory, answer)) = &remote.asking else {
            return;
        };
        let listing = match answer.try_recv() {
            Err(TryRecvError::Empty) => return,
            Ok(Ok(listing)) => Some(listing),
            Ok(Err(_)) => Some(Listing {
                path: directory.into(),
                entries: Vec::new(),
            }),
            Err(TryRecvError::Disconnected) => None,
        };
        if let Some(listing) = listing {
            remote.listed = Some((directory.clone(), listing));
        }
        remote.asking = None;
        cx.notify();
    }

    /// The directories in the listing whose names start as the field's
    /// last name does; a dot directory only once a dot is typed.
    pub(super) fn remote_completions(&self, cx: &App) -> Vec<String> {
        let Some(remote) = self.open_remote_path() else {
            return Vec::new();
        };
        let typed = remote.field.read(cx).text();
        let (directory, start) = split_typed(typed);
        let Some((_, listing)) = remote
            .listed
            .as_ref()
            .filter(|(listed, _)| listed == directory)
        else {
            return Vec::new();
        };
        listing
            .entries
            .iter()
            .filter(|entry| {
                entry.directory
                    && entry.name.starts_with(start)
                    && (start.starts_with('.') || !entry.name.starts_with('.'))
            })
            .take(REMOTE_COMPLETIONS)
            .map(|entry| entry.name.clone())
            .collect()
    }

    /// Complete the field's last name to `name`, a directory: the field
    /// then lists what is in it.
    pub(super) fn complete_remote_path(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(remote) = self.open_remote_path() else {
            return;
        };
        let field = remote.field.clone();
        let typed = field.read(cx).text().to_string();
        let (directory, _) = split_typed(&typed);
        let completed = format!("{directory}{name}/");
        field.update(cx, |field, cx| field.set(completed, cx));
    }

    /// Tab in the path field: complete it to the first directory that
    /// fits. False when the field does not hold the keyboard.
    pub(super) fn tab_remote_path(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if !self.remote_path_focused(window, cx) {
            return false;
        }
        if let Some(first) = self.remote_completions(cx).into_iter().next() {
            self.complete_remote_path(&first, cx);
        }
        true
    }

    /// ↵ in the path field: the directory it names joins the card, and the
    /// field empties, so the next ↵ confirms the card. False when the
    /// field does not hold the keyboard or holds nothing.
    pub(super) fn submit_remote_path(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        self.remote_path_focused(window, cx) && self.add_remote_path(cx)
    }

    /// Add the directory the field names to the card, and empty the field.
    fn add_remote_path(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(remote) = self.open_remote_path() else {
            return false;
        };
        let field = remote.field.clone();
        let typed = field.read(cx).text().trim().to_string();
        let path = match typed.trim_end_matches('/') {
            "" if typed.starts_with('/') => "/",
            "" => return false,
            path => path,
        };
        let path = spelled_out(remote, path);
        self.adopt_editor_directory(path, cx);
        field.update(cx, |field, cx| field.set(String::new(), cx));
        true
    }

    fn remote_path_focused(&self, window: &Window, cx: &App) -> bool {
        self.open_remote_path()
            .is_some_and(|remote| remote.field.read(cx).focus_handle(cx).is_focused(window))
    }

    /// The path field and, under it, the directories that complete it —
    /// each a press away.
    pub(super) fn remote_path_element(
        &self,
        remote: &RemotePath,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let machine = self
            .cockpit
            .machine()
            .map_or_else(String::new, |bridge| bridge.name().to_string());
        let focused = remote.field.read(cx).focus_handle(cx).is_focused(window);
        let completions =
            self.remote_completions(cx)
                .into_iter()
                .enumerate()
                .map(|(index, name)| {
                    project_editor::path_completion(index, &name).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.complete_remote_path(&name, cx);
                        }),
                    )
                });
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(project_editor::path_field(
                &machine,
                remote.field.clone(),
                focused,
            ))
            .child(project_editor::directory_list(completions.collect()))
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
/// `here` (once attached), or the machine off screen it is on. `None`
/// while that machine has not been heard from (`Bell::ring`).
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
