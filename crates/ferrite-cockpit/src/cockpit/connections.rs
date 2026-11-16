use super::*;
use crate::machines::Connection;
use ferrite_core::cockpit::ServerState;
use ferrite_core::machine::{self, auth, setup, Machine};
use gpui::component::input::{InputEvent, InputState};
use std::rc::Rc;

#[derive(Default)]
pub(super) struct Connections {
    pub(super) editor: Option<Editor>,
    removing: Option<String>,
    warning: Option<String>,
    error: Option<String>,
}

pub(super) struct Editor {
    fields: settings::ConnectionFields,
    candidates: Vec<String>,
    original: Option<Machine>,
    stage: Stage,
    warning: Option<String>,
}

enum Stage {
    Editing,
    Adding {
        attempt: setup::Attempt,
        target: String,
        phase: setup::Phase,
        verification: Option<Verification>,
    },
    Failed(String),
}

struct Verification {
    question: auth::Question,
    input: VerificationInput,
}

enum VerificationInput {
    Pending,
    Confirm(FocusHandle),
    Secret(Entity<InputState>),
}

fn action(
    cx: &Context<CockpitView>,
    act: impl Fn(&mut CockpitView, &mut Window, &mut Context<CockpitView>) + 'static,
) -> settings::WindowAct {
    let view = cx.entity().downgrade();
    Rc::new(move |window, cx| {
        let _ = view.update(cx, |view, cx| act(view, window, cx));
    })
}

impl CockpitView {
    pub(super) fn connection_holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.connections.editor.as_ref().is_some_and(|editor| {
            [
                &editor.fields.host,
                &editor.fields.username,
                &editor.fields.port,
            ]
            .into_iter()
            .any(|field| field.read(cx).focus_handle(cx).is_focused(window))
                || match &editor.stage {
                    Stage::Adding {
                        verification: Some(verification),
                        ..
                    } => match &verification.input {
                        VerificationInput::Secret(field) => {
                            field.read(cx).focus_handle(cx).is_focused(window)
                        }
                        VerificationInput::Confirm(focus) => focus.contains_focused(window, cx),
                        VerificationInput::Pending => false,
                    },
                    _ => false,
                }
        })
    }

    fn save_connections(
        &mut self,
        change: impl FnOnce(&mut Vec<Machine>),
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some(error) = &self.prefs.unreadable {
            return Err(format!("Settings cannot be saved: {error}"));
        }
        let mut settings = self.prefs.settings.clone();
        change(&mut settings.machines);
        let selected = self.machines.as_ref().and_then(|machines| machines.read(cx).selected_key().0);
        let leaves = selected.is_some_and(|target| {
            self.prefs.settings.machines.iter().any(|machine| machine.target == target && machine.enabled)
                && !settings.machines.iter().any(|machine| machine.target == target && machine.enabled)
        });
        if leaves {
            if let Some(error) = self.files_leave_error(cx) {
                return Err(error.into());
            }
        }
        settings
            .save(&self.prefs.dir)
            .map_err(|error| format!("Environment not saved: {error}. Try again."))?;
        self.prefs.settings = settings;
        self.cockpit.settings_changed(&self.prefs.settings);
        self.machines_changed(cx);
        cx.notify();
        Ok(())
    }

    pub(super) fn cancel_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.connections.editor = None;
        window.focus(&self.settings_focus, cx);
        cx.notify();
    }

    pub(super) fn edit_connection(
        &mut self,
        machine: Option<Machine>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_page = settings::PageKey::Connections;
        if let Some(search) = &self.settings_search {
            search.update(cx, |search, cx| search.set_value("", window, cx));
        }
        let (host, username, port) = machine
            .as_ref()
            .map(Machine::ssh_fields)
            .unwrap_or_default();
        let mut field = |value: String, placeholder: &'static str| {
            let field = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
            field.update(cx, |field, cx| field.set_value(value, window, cx));
            cx.subscribe_in(&field, window, |view, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    view.submit_connection(window, cx);
                }
                cx.notify();
            })
            .detach();
            field
        };
        let fields = settings::ConnectionFields {
            host: field(host, "example.com or SSH alias"),
            username: field(username, "From SSH config"),
            port: field(port, "22"),
        };
        fields.host.update(cx, |field, cx| field.focus(window, cx));
        self.connections.editor = Some(Editor {
            fields,
            candidates: machine::hosts::candidates(),
            original: machine,
            stage: Stage::Editing,
            warning: None,
        });
        self.connections.warning = None;
        self.connections.error = None;
        self.connections.removing = None;
        cx.notify();
    }

    fn submit_connection(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.connections.editor.as_mut() else {
            return;
        };
        if matches!(editor.stage, Stage::Adding { .. }) {
            return;
        }
        let fields = &editor.fields;
        let result = Machine::from_ssh_fields(
            fields.host.read(cx).value().as_ref(),
            fields.username.read(cx).value().as_ref(),
            fields.port.read(cx).value().as_ref(),
        )
        .and_then(|mut machine| {
            if connection_exists(
                &machine,
                editor.original.as_ref(),
                &self.prefs.settings.machines,
            ) {
                return Err(
                    "This environment is already saved. Open it or reconnect below.".into(),
                );
            }
            if let Some(original) = &editor.original {
                if original.name != original.target {
                    machine.name = original.name.clone();
                }
            }
            let helper = std::env::current_exe().map_err(|error| error.to_string())?;
            let target = machine.target.clone();
            setup::Attempt::start(machine, self.prefs.dir.clone(), helper)
                .map(|attempt| (attempt, target))
                .map_err(|error| error.to_string())
        });
        editor.stage = match result {
            Ok((attempt, target)) => Stage::Adding {
                attempt,
                target,
                phase: setup::Phase::Connecting,
                verification: None,
            },
            Err(error) => Stage::Failed(error),
        };
        cx.notify();
    }

    fn answer_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.connections.editor.as_mut() else {
            return;
        };
        let Stage::Adding { verification, .. } = &mut editor.stage else {
            return;
        };
        let Some(verification) = verification.take() else {
            return;
        };
        let answer = match verification.question.kind {
            auth::PromptKind::ConfirmHost => "yes".to_string(),
            auth::PromptKind::Secret => match &verification.input {
                VerificationInput::Secret(field) => field.read(cx).value().to_string(),
                _ => return,
            },
        };
        if let VerificationInput::Secret(response) = verification.input {
            response.update(cx, |field, cx| field.set_value("", window, cx));
        }
        if let Err(error) = verification.question.respond(Some(answer)) {
            editor.stage = Stage::Failed(format!("SSH verification failed: {error}. Try again."));
        }
        window.focus(&self.settings_focus, cx);
        cx.notify();
    }

    pub(super) fn poll_connections(&mut self, cx: &mut Context<Self>) {
        loop {
            let Some(editor) = self.connections.editor.as_mut() else {
                return;
            };
            let Stage::Adding {
                attempt,
                verification,
                ..
            } = &mut editor.stage
            else {
                return;
            };
            if verification
                .as_ref()
                .is_some_and(|prompt| std::time::Instant::now() >= prompt.question.expires_at)
            {
                editor.stage = Stage::Failed("SSH verification expired. Try again.".into());
                cx.notify();
                return;
            }
            let Some(event) = attempt.poll() else { return };
            self.connection_event(event, cx);
        }
    }

    pub(super) fn connection_event(&mut self, event: setup::Event, cx: &mut Context<Self>) {
        let Some(editor) = self.connections.editor.as_mut() else {
            return;
        };
        match event {
            setup::Event::Progress(progress) => {
                if let Stage::Adding { phase, .. } = &mut editor.stage {
                    *phase = progress;
                }
            }
            setup::Event::Verify(question) => {
                if let Stage::Adding { verification, .. } = &mut editor.stage {
                    *verification = Some(Verification {
                        question,
                        input: VerificationInput::Pending,
                    });
                }
            }
            setup::Event::Warning(warning) => editor.warning = Some(warning),
            setup::Event::Failed(error) => editor.stage = Stage::Failed(error),
            setup::Event::Ready(machine) => {
                if connection_exists(
                    &machine,
                    editor.original.as_ref(),
                    &self.prefs.settings.machines,
                ) {
                    editor.stage = Stage::Failed(
                        "This environment is already saved. Open it or reconnect below.".into(),
                    );
                    cx.notify();
                    return;
                }
                let original = editor
                    .original
                    .as_ref()
                    .map(|machine| machine.target.clone());
                let warning = editor.warning.take();
                if let Err(error) = self.save_connections(
                    |saved| {
                        if let Some(existing) = saved
                            .iter_mut()
                            .find(|saved| original.as_ref() == Some(&saved.target))
                        {
                            *existing = machine.clone();
                        } else {
                            saved.push(machine.clone());
                        }
                    },
                    cx,
                ) {
                    self.connections
                        .editor
                        .as_mut()
                        .expect("active editor")
                        .stage = Stage::Failed(error);
                    cx.notify();
                    return;
                }
                self.connections.warning = warning;
                self.connections.editor = None;
                let selected = self.machines.as_ref().is_some_and(|machines| {
                    machines.read(cx).selected_key().0.as_deref() == Some(&machine.target)
                });
                if let Some(machines) = &self.machines {
                    machines.update(cx, |machines, cx| {
                        if let Some(index) = machines.named(&machine.target) {
                            machines.retry(index);
                        }
                        cx.notify();
                    });
                }
                if selected {
                    self.cockpit.retry_server();
                }
            }
        }
        cx.notify();
    }

    fn connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> settings::ConnectionForm {
        let submit = action(cx, Self::submit_connection);
        let cancel = action(cx, Self::cancel_connection);
        let verify = action(cx, Self::answer_connection);
        let view = cx.entity().downgrade();
        let choose_host = Rc::new(
            move |host: SharedString, window: &mut Window, cx: &mut gpui::App| {
                let _ = view.update(cx, |view, cx| {
                    if let Some(editor) = &view.connections.editor {
                        editor
                            .fields
                            .host
                            .update(cx, |field, cx| field.set_value(host.clone(), window, cx));
                        editor
                            .fields
                            .username
                            .update(cx, |field, cx| field.focus(window, cx));
                    }
                    cx.notify();
                });
            },
        );
        let Some(editor) = self.connections.editor.as_mut() else {
            return settings::ConnectionForm::Closed {
                open: action(cx, |view, window, cx| {
                    view.edit_connection(None, window, cx)
                }),
                error: self.connections.error.clone().map(Into::into),
            };
        };
        let mut prompt = None;
        let (progress, error) = match &mut editor.stage {
            Stage::Editing => (None, None),
            Stage::Failed(error) => (None, Some(error.clone().into())),
            Stage::Adding {
                phase,
                target,
                verification,
                ..
            } => {
                if let Some(verification) = verification {
                    if matches!(verification.input, VerificationInput::Pending)
                        && verification.question.kind == auth::PromptKind::Secret
                    {
                        let response = cx.new(|cx| {
                            InputState::new(window, cx)
                                .masked(true)
                                .placeholder("Password or passphrase")
                        });
                        cx.subscribe_in(
                            &response,
                            window,
                            |view, _, event: &InputEvent, window, cx| {
                                if matches!(event, InputEvent::PressEnter { .. }) {
                                    view.answer_connection(window, cx);
                                }
                            },
                        )
                        .detach();
                        response.update(cx, |field, cx| field.focus(window, cx));
                        verification.input = VerificationInput::Secret(response);
                    } else if matches!(verification.input, VerificationInput::Pending) {
                        let focus = cx.focus_handle();
                        window.focus(&focus, cx);
                        verification.input = VerificationInput::Confirm(focus);
                    }
                    let seconds = verification
                        .question
                        .expires_at
                        .saturating_duration_since(std::time::Instant::now())
                        .as_secs();
                    prompt = Some(settings::ConnectionVerification {
                        target: target.clone().into(),
                        prompt: verification.question.prompt.clone().into(),
                        response: match &verification.input {
                            VerificationInput::Secret(field) => Some(field.clone()),
                            _ => None,
                        },
                        focus: match &verification.input {
                            VerificationInput::Confirm(focus) => Some(focus.clone()),
                            _ => None,
                        },
                        expires: format!("Expires in {seconds} seconds").into(),
                        submit: verify,
                    });
                }
                (Some(phase.label().into()), None)
            }
        };
        let typed = editor
            .fields
            .host
            .read(cx)
            .value()
            .to_string()
            .to_lowercase();
        let suggestions = editor
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.to_lowercase().contains(&typed) && candidate.as_str() != typed
            })
            .take(4)
            .cloned()
            .map(Into::into)
            .collect();
        settings::ConnectionForm::Editing {
            fields: editor.fields.clone(),
            suggestions,
            choose_host,
            progress,
            error,
            warning: editor.warning.clone().map(Into::into),
            verification: prompt,
            submit,
            cancel,
        }
    }

    pub(super) fn connection_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<settings::Group> {
        let listed = self
            .machines
            .as_ref()
            .map(|machines| {
                let machines = machines.read(cx);
                machines
                    .rows(self.cockpit.server())
                    .into_iter()
                    .filter_map(|row| {
                        let machine = machines.machine(row.index)?.clone();
                        let state = machines.server_state(row.index, self.cockpit.server());
                        let unsaved = machines
                            .unsaved()
                            .is_some_and(|unsaved| unsaved.target == machine.target);
                        Some((row, machine, state, unsaved))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let local_actions = if self.on_machine() {
            vec![settings::ConnectionAction {
                id: "connection-open-0".into(),
                label: "Open".into(),
                enabled: true,
                act: action(cx, |_, _, cx| {
                    cx.emit(GoToMachine {
                        index: 0,
                        open: None,
                    })
                }),
            }]
        } else {
            Vec::new()
        };
        let local = connection_row(settings::ConnectionRow {
            name: "This computer".into(),
            target: "Local environment".into(),
            status: "Connected".into(),
            connected: true,
            detail: None,
            actions: local_actions,
            removal: None,
        });
        let form = self.connection_form(window, cx);
        let mut remote = vec![settings::Row::new(
            "Add environment",
            "SSH host alias username port",
            vec![],
            settings::Control::ConnectionForm(form),
        )];
        for (row, machine, state, unsaved) in listed {
            let index = row.index;
            let selected = row.selected;
            let target = machine.target.clone();
            let changing = self.connections.editor.as_ref().is_some_and(|editor| {
                matches!(editor.stage, Stage::Adding { .. })
                    && editor
                        .original
                        .as_ref()
                        .is_some_and(|original| original.target == target)
            });
            let mut actions = vec![settings::ConnectionAction {
                id: format!("connection-open-{index}").into(),
                label: "Open".into(),
                enabled: machine.enabled && !selected,
                act: action(cx, move |_, _, cx| {
                    cx.emit(GoToMachine { index, open: None })
                }),
            }];
            if machine.enabled && row.connection != Connection::Online {
                let machine = machine.clone();
                let label = match &state {
                    Some(ServerState::Detached {
                        attention: Some(machine::Failure::NotInstalled),
                        ..
                    }) => "Set up",
                    _ => "Retry",
                };
                actions.push(settings::ConnectionAction {
                    id: format!("connection-retry-{index}").into(),
                    label: label.into(),
                    enabled: self.connections.editor.is_none(),
                    act: action(cx, move |view, window, cx| {
                        view.edit_connection(Some(machine.clone()), window, cx);
                        view.submit_connection(window, cx);
                    }),
                });
            }
            let toggled = target.clone();
            let reconnect = machine.clone();
            if unsaved {
                let saving = machine.clone();
                actions.push(settings::ConnectionAction {
                    id: format!("connection-save-{index}").into(),
                    label: "Save".into(),
                    enabled: self.connections.editor.is_none(),
                    act: action(cx, move |view, window, cx| {
                        view.edit_connection(Some(saving.clone()), window, cx);
                        view.submit_connection(window, cx);
                    }),
                });
            } else {
                actions.push(settings::ConnectionAction {
                    id: format!("connection-toggle-{index}").into(),
                    label: if machine.enabled {
                        "Disconnect"
                    } else {
                        "Reconnect"
                    }
                    .into(),
                    enabled: !changing,
                    act: action(cx, move |view, window, cx| {
                        if !reconnect.enabled {
                            view.edit_connection(Some(reconnect.clone()), window, cx);
                            view.submit_connection(window, cx);
                            return;
                        }
                        view.connections.editor = None;
                        let result = view.save_connections(
                            |saved| {
                                for machine in
                                    saved.iter_mut().filter(|machine| machine.target == toggled)
                                {
                                    machine.enabled = !machine.enabled;
                                }
                            },
                            cx,
                        );
                        view.connections.error = result.err();
                        cx.notify();
                    }),
                });
            }
            let removing = target.clone();
            actions.push(settings::ConnectionAction {
                id: format!("connection-remove-{index}").into(),
                label: "Remove".into(),
                enabled: !changing,
                act: action(cx, move |view, _, cx| {
                    view.connections.removing = Some(removing.clone());
                    cx.notify();
                }),
            });
            let removal = (self.connections.removing.as_ref() == Some(&target)).then(|| {
                let confirm = action(cx, move |view, _, cx| {
                    let selected = view.machines.as_ref().is_some_and(|machines| {
                        machines.read(cx).selected_key().0.as_deref() == Some(&target)
                    });
                    if selected {
                        if let Some(error) = view.files_leave_error(cx) {
                            view.connections.error = Some(error.into());
                            cx.notify();
                            return;
                        }
                    }
                    view.connections.editor = None;
                    let result = view.save_connections(
                        |saved| saved.retain(|machine| machine.target != target),
                        cx,
                    );
                    if result.is_ok() {
                        view.connections.removing = None;
                        if unsaved {
                            let leave = view.machines.as_ref().is_some_and(|machines| {
                                machines.update(cx, |machines, cx| {
                                    let leave = machines.forget_unsaved(&target);
                                    cx.notify();
                                    leave
                                })
                            });
                            if leave {
                                cx.emit(GoToMachine {
                                    index: 0,
                                    open: None,
                                });
                            }
                        }
                    }
                    view.connections.error = result.err();
                    cx.notify();
                });
                let cancel = action(cx, |view, _, cx| {
                    view.connections.removing = None;
                    cx.notify();
                });
                (confirm, cancel)
            });
            let (status, detail) = match (row.connection, state) {
                (Connection::Disabled, _) => ("Disconnected", None),
                (Connection::Online, _) => ("Connected", None),
                (_, Some(ServerState::Detached { reason, attention })) => (
                    if attention.is_some() {
                        "Needs attention"
                    } else {
                        "Connecting…"
                    },
                    Some(reason.into()),
                ),
                _ => ("Connecting…", None),
            };
            remote.push(connection_row(settings::ConnectionRow {
                name: machine.name.into(),
                target: machine.target.into(),
                status: status.into(),
                connected: row.connection == Connection::Online,
                detail,
                actions,
                removal,
            }));
        }
        let mut group = settings::Group::new(Some("Remote environments")).rows(remote);
        if let Some(warning) = &self.connections.warning {
            group = group.note(warning.clone());
        }
        vec![settings::Group::new(None).rows([local]), group]
    }
}

fn connection_row(row: settings::ConnectionRow) -> settings::Row {
    settings::Row::new(
        row.name.clone(),
        row.target.clone(),
        vec![row.status.clone()],
        settings::Control::ConnectionRow(row),
    )
}

fn connection_exists(machine: &Machine, original: Option<&Machine>, saved: &[Machine]) -> bool {
    saved.iter().any(|saved| {
        saved.target == machine.target
            && original.is_none_or(|original| original.target != saved.target)
    })
}
