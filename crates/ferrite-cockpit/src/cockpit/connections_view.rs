//! Connections as a view of its own (ADR 0041): the Settings page that
//! adds an environment, and retries, disconnects and removes the saved
//! ones. It lives with the Settings sheet, so what it holds (the editor
//! and the attempt it runs, a Remove waiting for its confirm, the last
//! error and warning) goes when the sheet does. What it shows of the
//! window it reads through `connections_inputs`; what changes the saved
//! environments, or goes to one, it says as a `ConnectionsEvent`, which
//! the Cockpit saves and answers; what it asks of the sheet, as a
//! `PageEvent`.

use super::*;
use crate::machines::{Connection, MachineRow};
use ferrite_client::cockpit::ServerState;
use ferrite_core::machine::{self, auth, remote, setup, Machine};
use gpui::component::input::{InputEvent, InputState};
use gpui::{App, EntityInputHandler, WeakEntity};
use settings::ConnectionMode;
use std::path::PathBuf;
use std::rc::Rc;

pub(crate) struct ConnectionsView {
    cockpit: WeakEntity<CockpitView>,
    editor: Option<Editor>,
    // Not one state with the editor: a Remove can wait while an
    // environment is being added, and an error said while the editor is
    // open shows once it closes.
    /// The saved environment whose Remove waits for its confirm.
    removing: Option<String>,
    warning: Option<String>,
    error: Option<String>,
    generation: u64,
}

/// What Connections asks of the window.
#[derive(Clone)]
pub(crate) enum ConnectionsEvent {
    /// Have Settings edit the defaults and CLIs of the environment at this
    /// index of the machines list.
    Open(usize),
    /// Turn this app's own environment on or off.
    ToggleLocal,
    /// Turn the saved environment with this target on or off.
    Toggle(String),
    /// Turn "Sync my setup" of the saved environment with this target on
    /// or off.
    ToggleSync(String),
    /// Sync the environment with this target though it removes much.
    SyncAnyway(String),
    /// Sync the environment with this target, each sign-in that differs
    /// taken from this side.
    UseSignIns(String, ferrite_core::tree::Side),
    /// Remove a saved environment, as confirmed.
    Remove { machine: Machine, unsaved: bool },
    /// A setup or pairing finished: save `machine` in place of the
    /// environment it set up again, if it did.
    Ready {
        machine: Machine,
        replacing: Option<String>,
    },
}

impl EventEmitter<ConnectionsEvent> for ConnectionsView {}

/// What Connections asks of the sheet it is a page of.
#[derive(Clone, Copy)]
pub(crate) enum PageEvent {
    /// An edit began: show the Connections page whole.
    Editing,
    /// The editor was cancelled: the sheet takes the keyboard back.
    Cancelled,
}

impl EventEmitter<PageEvent> for ConnectionsView {}

/// What Connections draws from, as the Cockpit has it now.
#[derive(Default)]
pub(crate) struct ConnectionsInputs {
    /// Where the settings live: a link's target is shown from it.
    pub dir: PathBuf,
    /// The environments Settings saves.
    pub saved: Vec<Machine>,
    pub local_enabled: bool,
    /// The window keeps a machines list, so this app's own environment
    /// can be turned off.
    pub switchable: bool,
    pub listed: Vec<Listed>,
}

/// One environment of the machines list.
pub(crate) struct Listed {
    pub row: MachineRow,
    pub machine: Machine,
    pub state: Option<ServerState>,
    /// Named for this run (`--remote`), but not saved in Settings.
    pub unsaved: bool,
    /// Settings' defaults and CLIs are about it now.
    pub editing: bool,
    /// Where its setup sync stands; `None` when the window syncs none.
    pub sync: Option<super::setup_sync::Status>,
}

pub(super) struct Editor {
    fields: settings::ConnectionFields,
    generation: u64,
    mode: ConnectionMode,
    focus: FocusHandle,
    _keys: gpui::Subscription,
    discovery: Discovery,
    suggestions_open: bool,
    highlighted: Option<usize>,
    suggestion_scroll: gpui::ScrollHandle,
    original: Option<Machine>,
    remote_host_value: String,
    pairing: Option<remote::PairingFields>,
    stage: Stage,
    warning: Option<String>,
}

#[derive(Default)]
struct Discovery {
    hosts: Vec<machine::hosts::Host>,
    generation: u64,
    pending: bool,
    error: Option<String>,
}

enum Attempt {
    Ssh(setup::Attempt),
    Remote(remote::PairAttempt),
}

impl Attempt {
    fn poll(&mut self) -> Option<setup::Event> {
        match self {
            Self::Ssh(attempt) => attempt.poll(),
            Self::Remote(attempt) => attempt.poll(),
        }
    }
}

enum Stage {
    Editing,
    Resolving,
    Adding {
        attempt: Attempt,
        target: String,
        phase: setup::Phase,
        verification: Option<Verification>,
    },
    Failed(String),
}

struct Verification {
    question: auth::Question,
    input: VerificationInput,
    focus: FocusHandle,
    remaining: u64,
}

enum VerificationInput {
    Pending,
    Confirm,
    Secret(Entity<InputState>),
}

fn action(
    cx: &Context<ConnectionsView>,
    act: impl Fn(&mut ConnectionsView, &mut Window, &mut Context<ConnectionsView>) + 'static,
) -> settings::WindowAct {
    let view = cx.entity().downgrade();
    Rc::new(move |window, cx| {
        let _ = view.update(cx, |view, cx| act(view, window, cx));
    })
}

impl ConnectionsView {
    pub(super) fn new(cockpit: WeakEntity<CockpitView>) -> Self {
        Self {
            cockpit,
            editor: None,
            removing: None,
            warning: None,
            error: None,
            generation: 0,
        }
    }

    /// Never called while the Cockpit is being updated: it reads it.
    fn inputs(&self, cx: &App) -> ConnectionsInputs {
        self.cockpit
            .read_with(cx, |cockpit, cx| cockpit.connections_inputs(cx))
            .unwrap_or_default()
    }

    /// An environment is being added, or set up again.
    pub(super) fn editing(&self) -> bool {
        self.editor.is_some()
    }

    /// The editor goes, and the attempt it runs with it.
    pub(super) fn close_editor(&mut self) {
        self.editor = None;
    }

    pub(super) fn holds_focus(&self, window: &Window, cx: &App) -> bool {
        self.editor.as_ref().is_some_and(|editor| {
            editor.focus.contains_focused(window, cx)
                || [&editor.fields.host, &editor.fields.remote_host, &editor.fields.pairing_code,
                    &editor.fields.username, &editor.fields.port].into_iter()
                    .any(|field| field.read(cx).focus_handle(cx).is_focused(window))
                || match &editor.stage {
                    Stage::Adding { verification: Some(verification), .. } => {
                        verification.focus.contains_focused(window, cx)
                            || matches!(&verification.input, VerificationInput::Secret(input) if input.read(cx).focus_handle(cx).is_focused(window))
                    }
                    _ => false,
                }
        })
    }

    fn is_composing(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(editor) = &self.editor else {
            return false;
        };
        let mut inputs = vec![
            &editor.fields.host,
            &editor.fields.remote_host,
            &editor.fields.pairing_code,
            &editor.fields.username,
            &editor.fields.port,
        ];
        if let Stage::Adding {
            verification:
                Some(Verification {
                    input: VerificationInput::Secret(input),
                    ..
                }),
            ..
        } = &editor.stage
        {
            inputs.push(input);
        }
        inputs.into_iter().any(|input| {
            input.read(cx).focus_handle(cx).is_focused(window)
                && input.update(cx, |input, cx| {
                    input.marked_text_range(window, cx).is_some()
                })
        })
    }

    pub(super) fn cancel(&mut self, cx: &mut Context<Self>) {
        self.editor = None;
        cx.emit(PageEvent::Cancelled);
        cx.notify();
    }

    pub(super) fn edit(
        &mut self,
        machine: Option<Machine>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(PageEvent::Editing);
        let dir = self.inputs(cx).dir;
        let mode = if machine.as_ref().is_some_and(|machine| !machine.is_link()) {
            ConnectionMode::Ssh
        } else {
            ConnectionMode::Remote
        };
        let (host, username, port) = machine
            .as_ref()
            .filter(|machine| !machine.is_link())
            .map(Machine::ssh_fields)
            .unwrap_or_default();
        let remote_host = machine
            .as_ref()
            .filter(|machine| machine.is_link())
            .map(|machine| machine.display_target(&dir))
            .unwrap_or_default();
        let mut field = |value: String, placeholder: &'static str| {
            let field = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
            field.update(cx, |field, cx| field.set_value(value, window, cx));
            cx.subscribe_in(
                &field,
                window,
                |view, input, event: &InputEvent, window, cx| {
                    match event {
                        InputEvent::PressEnter { .. } => view.enter(window, cx),
                        InputEvent::Change => view.input_changed(input, window, cx),
                        InputEvent::Focus => {
                            if let Some(editor) = &mut view.editor {
                                if input == &editor.fields.host {
                                    editor.suggestions_open = true;
                                }
                            }
                        }
                        InputEvent::Blur => {
                            if let Some(editor) = &mut view.editor {
                                if input == &editor.fields.host {
                                    editor.suggestions_open = false;
                                    editor.highlighted = None;
                                }
                            }
                        }
                    }
                    cx.notify();
                },
            )
            .detach();
            field
        };
        let fields = settings::ConnectionFields {
            remote_host: field(remote_host.clone(), "backend.example.com"),
            pairing_code: field(String::new(), "PAIRCODE"),
            host: field(host, "Search hosts or type devbox"),
            username: field(username, "root"),
            port: field(port, "22"),
        };
        // A saved link keeps its host: what it needs is a new code.
        let initial = if mode == ConnectionMode::Ssh {
            &fields.host
        } else if remote_host.is_empty() {
            &fields.remote_host
        } else {
            &fields.pairing_code
        };
        initial.update(cx, |field, cx| field.focus(window, cx));
        self.generation = self.generation.wrapping_add(1);
        let view = cx.entity().downgrade();
        let owner = window.window_handle();
        let keys = cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() == owner {
                let _ = view.update(cx, |view, cx| view.key_down(&event.keystroke, window, cx));
            }
        });
        self.editor = Some(Editor {
            fields,
            generation: self.generation,
            mode,
            focus: cx.focus_handle(),
            _keys: keys,
            discovery: Discovery::default(),
            suggestions_open: false,
            highlighted: None,
            suggestion_scroll: gpui::ScrollHandle::new(),
            original: machine,
            remote_host_value: remote_host,
            pairing: None,
            stage: Stage::Editing,
            warning: None,
        });
        self.warning = None;
        self.error = None;
        self.removing = None;
        if mode == ConnectionMode::Ssh {
            self.refresh_hosts(window, cx);
        }
        cx.notify();
    }

    fn input_changed(
        &mut self,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        if input == &editor.fields.host
            && !matches!(editor.stage, Stage::Adding { .. } | Stage::Resolving)
        {
            editor.highlighted = None;
            editor.suggestions_open = true;
            editor
                .suggestion_scroll
                .set_offset(gpui::point(px(0.), px(0.)));
        }
        if input == &editor.fields.remote_host {
            let value = input.read(cx).value().to_string();
            let previous = std::mem::replace(&mut editor.remote_host_value, value.clone());
            if let Some(fields) = remote::PairingFields::parse(&value, "")
                .ok()
                .filter(|_| inserted_multiple_characters(&previous, &value))
            {
                if fields.host != value {
                    editor.pairing = Some(fields.clone());
                    editor.remote_host_value = fields.host.clone();
                    editor
                        .fields
                        .remote_host
                        .update(cx, |input, cx| input.set_value(fields.host, window, cx));
                    editor.fields.pairing_code.update(cx, |input, cx| {
                        input.set_value(fields.pairing_code, window, cx)
                    });
                }
            }
        }
        if editor.pairing.as_ref().is_some_and(|pairing| {
            pairing.host != editor.fields.remote_host.read(cx).value().trim()
                || pairing.pairing_code != editor.fields.pairing_code.read(cx).value().trim()
        }) {
            editor.pairing = None;
        }
    }

    fn set_mode(&mut self, mode: ConnectionMode, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        if matches!(editor.stage, Stage::Adding { .. } | Stage::Resolving) {
            return;
        }
        editor.mode = mode;
        editor.stage = Stage::Editing;
        editor.warning = None;
        editor.suggestions_open = false;
        editor.highlighted = None;
        let field = match mode {
            ConnectionMode::Remote => &editor.fields.remote_host,
            ConnectionMode::Ssh => &editor.fields.host,
        };
        field.update(cx, |field, cx| field.focus(window, cx));
        if mode == ConnectionMode::Ssh {
            self.refresh_hosts(window, cx);
        }
        cx.notify();
    }

    fn refresh_hosts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        editor.discovery.generation = editor.discovery.generation.wrapping_add(1);
        editor.discovery.pending = true;
        editor.discovery.error = None;
        let session = editor.generation;
        let refresh = editor.discovery.generation;
        cx.spawn_in(window, async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async { machine::hosts::discover() })
                .await;
            let _ = view.update(cx, |view, cx| {
                let Some(editor) = &mut view.editor else {
                    return;
                };
                if editor.generation != session || editor.discovery.generation != refresh {
                    return;
                }
                editor.discovery.pending = false;
                match result {
                    Ok(hosts) => editor.discovery.hosts = hosts,
                    Err(error) => editor.discovery.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn choose_host(&mut self, value: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        if matches!(editor.stage, Stage::Adding { .. } | Stage::Resolving) {
            return;
        }
        editor.suggestions_open = false;
        editor.highlighted = None;
        editor
            .fields
            .host
            .update(cx, |field, cx| field.set_value(value.clone(), window, cx));
        editor.stage = Stage::Resolving;
        let generation = editor.generation;
        cx.spawn_in(window, async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { machine::hosts::resolve(&value) })
                .await;
            let _ = view.update_in(cx, |view, window, cx| {
                let Some(editor) = &mut view.editor else {
                    return;
                };
                if editor.generation != generation || !matches!(editor.stage, Stage::Resolving) {
                    return;
                }
                match result {
                    Ok(host) => {
                        editor.fields.username.update(cx, |field, cx| {
                            field.set_value(host.username.unwrap_or_default(), window, cx)
                        });
                        editor.fields.port.update(cx, |field, cx| {
                            field.set_value(
                                host.port.map(|port| port.to_string()).unwrap_or_default(),
                                window,
                                cx,
                            )
                        });
                        editor.stage = Stage::Editing;
                        view.submit(window, cx);
                    }
                    Err(error) => editor.stage = Stage::Failed(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// The discovered hosts the SSH host field matches, less those saved.
    fn hosts_matching(&self, saved: &[Machine], cx: &App) -> Vec<machine::hosts::Host> {
        let Some(editor) = &self.editor else {
            return Vec::new();
        };
        matching_hosts(
            &editor.discovery.hosts,
            editor.fields.host.read(cx).value().as_ref(),
            saved,
        )
    }

    fn enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_composing(window, cx) {
            return;
        }
        let highlighted = self.editor.as_ref().and_then(|editor| {
            (editor.mode == ConnectionMode::Ssh && editor.suggestions_open)
                .then_some(editor.highlighted)
                .flatten()
        });
        if let Some(host) = highlighted
            .and_then(|at| self.hosts_matching(&self.inputs(cx).saved, cx).get(at).cloned())
        {
            self.choose_host(host.alias.into(), window, cx);
        } else {
            if self.editor.as_ref().is_some_and(|editor| {
                editor.mode == ConnectionMode::Ssh
                    && editor.fields.host.read(cx).value().trim().is_empty()
            }) {
                return;
            }
            self.submit(window, cx);
        }
    }

    fn key_down(
        &mut self,
        keystroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = &self.editor else {
            return;
        };
        if self.is_composing(window, cx) {
            if matches!(keystroke.key.as_str(), "enter" | "escape" | "up" | "down") {
                cx.stop_propagation();
            }
            return;
        }
        let key = keystroke.key.as_str();
        let modifiers = keystroke.modifiers;
        if key == "tab" && !modifiers.platform && !modifiers.control && !modifiers.alt {
            cx.stop_propagation();
            let scope = match &editor.stage {
                Stage::Adding {
                    verification: Some(verification),
                    ..
                } => verification.focus.clone(),
                _ => editor.focus.clone(),
            };
            let mut visited = Vec::new();
            loop {
                if modifiers.shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
                let Some(focused) = window.focused(cx) else {
                    break;
                };
                if scope.contains_focused(window, cx) {
                    break;
                }
                if visited.contains(&focused) {
                    window.focus(&scope, cx);
                    break;
                }
                visited.push(focused);
            }
            return;
        }
        if key == "escape" && !modifiers.modified() {
            cx.stop_propagation();
            if let Some(editor) = &mut self.editor {
                if editor.suggestions_open
                    && editor.mode == ConnectionMode::Ssh
                    && (!editor.discovery.hosts.is_empty()
                        || editor.discovery.pending
                        || editor.discovery.error.is_some())
                {
                    editor.suggestions_open = false;
                    editor.highlighted = None;
                    cx.notify();
                    return;
                }
            }
            self.cancel(cx);
            return;
        }
        if editor.mode != ConnectionMode::Ssh
            || !editor
                .fields
                .host
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
            || matches!(editor.stage, Stage::Adding { .. } | Stage::Resolving)
        {
            return;
        }
        let hosts = self.hosts_matching(&self.inputs(cx).saved, cx);
        if modifiers == gpui::Modifiers::secondary_key() {
            if let Some(at) = key.parse::<usize>().ok().filter(|at| (1..=9).contains(at)) {
                if let Some(host) = hosts.get(at - 1) {
                    cx.stop_propagation();
                    self.choose_host(host.alias.clone().into(), window, cx);
                }
            }
            return;
        }
        if modifiers.modified() {
            return;
        }
        if key == "up" || key == "down" {
            if hosts.is_empty() {
                return;
            }
            cx.stop_propagation();
            let editor = self.editor.as_mut().unwrap();
            editor.suggestions_open = true;
            let at = match (editor.highlighted, key) {
                (Some(at), "down") => (at + 1) % hosts.len(),
                (Some(at), _) => (at + hosts.len() - 1) % hosts.len(),
                (None, "down") => 0,
                (None, _) => hosts.len() - 1,
            };
            editor.highlighted = Some(at);
            editor.suggestion_scroll.scroll_to_item(at);
            cx.notify();
        } else if key == "enter" {
            cx.stop_propagation();
            self.enter(window, cx);
        }
    }

    fn submit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let ConnectionsInputs { dir, saved, .. } = self.inputs(cx);
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if matches!(editor.stage, Stage::Adding { .. } | Stage::Resolving) {
            return;
        }
        editor.suggestions_open = false;
        let fields = &editor.fields;
        let result = match editor.mode {
            ConnectionMode::Remote => {
                let existing = editor.original.as_ref().filter(|machine| {
                    machine.is_link()
                        && fields.pairing_code.read(cx).value().is_empty()
                        && machine.display_target(&dir)
                            == fields.remote_host.read(cx).value().as_ref()
                });
                if let Some(machine) = existing {
                    let mut machine = machine.clone();
                    machine.enabled = true;
                    remote::PairAttempt::reconnect(machine.clone(), dir.clone())
                        .map(|attempt| (Attempt::Remote(attempt), machine.display_target(&dir)))
                        .map_err(|error| error.to_string())
                } else {
                    let host = fields.remote_host.read(cx).value();
                    let code = fields.pairing_code.read(cx).value();
                    let parsed = if host.trim().is_empty() {
                        Err("Enter a backend host.".into())
                    } else if let Some(pairing) = editor.pairing.as_ref().filter(|pairing| {
                        pairing.host == host.trim() && pairing.pairing_code == code.trim()
                    }) {
                        Ok(pairing.clone())
                    } else {
                        remote::PairingFields::parse(host.as_ref(), code.as_ref())
                    };
                    parsed.and_then(|fields| {
                        let target = fields.host.clone();
                        let again = editor
                            .original
                            .as_ref()
                            .filter(|machine| machine.is_link())
                            .map(|machine| machine.target.clone());
                        remote::PairAttempt::start(fields, again, dir.clone())
                            .map(|attempt| (Attempt::Remote(attempt), target))
                            .map_err(|error| error.to_string())
                    })
                }
            }
            ConnectionMode::Ssh => Machine::from_ssh_fields(
                fields.host.read(cx).value().as_ref(),
                fields.username.read(cx).value().as_ref(),
                fields.port.read(cx).value().as_ref(),
            )
            .and_then(|mut machine| {
                let original = editor.original.as_ref().map(|original| original.target.as_str());
                if connection_exists(&machine, original, &saved) {
                    return Err(ALREADY_SAVED.into());
                }
                if let Some(original) = &editor.original {
                    if original.name != original.target {
                        machine.name = original.name.clone();
                    }
                }
                let helper = std::env::current_exe().map_err(|error| error.to_string())?;
                let target = machine.target.clone();
                setup::Attempt::start(machine, dir.clone(), helper)
                    .map(|attempt| (Attempt::Ssh(attempt), target))
                    .map_err(|error| error.to_string())
            }),
        };
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

    fn answer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_composing(window, cx) {
            return;
        }
        let Some(editor) = self.editor.as_mut() else {
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
        window.focus(&editor.focus, cx);
        cx.notify();
    }

    /// Take what the attempt in flight said. The Cockpit's pump calls it
    /// each frame, so it reads nothing of the Cockpit.
    pub(super) fn poll(&mut self, cx: &mut Context<Self>) {
        loop {
            let Some(editor) = self.editor.as_mut() else {
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
            if let Some(verification) = verification {
                let remaining = verification
                    .question
                    .expires_at
                    .saturating_duration_since(std::time::Instant::now())
                    .as_secs();
                if remaining != verification.remaining {
                    verification.remaining = remaining;
                    cx.notify();
                }
            }
            let Some(event) = attempt.poll() else { return };
            let ready = matches!(event, setup::Event::Ready(_));
            self.setup_event(event, cx);
            // The Cockpit's answer to `Ready` ends the attempt; nothing
            // after it is taken.
            if ready {
                return;
            }
        }
    }

    pub(super) fn setup_event(&mut self, event: setup::Event, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
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
                        remaining: question
                            .expires_at
                            .saturating_duration_since(std::time::Instant::now())
                            .as_secs(),
                        question,
                        input: VerificationInput::Pending,
                        focus: cx.focus_handle(),
                    });
                }
            }
            setup::Event::Warning(warning) => editor.warning = Some(warning),
            setup::Event::Failed(error) => editor.stage = Stage::Failed(error),
            setup::Event::Ready(machine) => cx.emit(ConnectionsEvent::Ready {
                replacing: editor.original.as_ref().map(|original| original.target.clone()),
                machine,
            }),
        }
        cx.notify();
    }

    /// The Cockpit's answer to a change asked of it: the error it met,
    /// if any.
    pub(super) fn answered(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        self.error = error;
        cx.notify();
    }

    /// The setup finished, but is not saved: why. The editor stays, to
    /// try again.
    pub(super) fn refused(&mut self, error: String, cx: &mut Context<Self>) {
        if let Some(editor) = &mut self.editor {
            editor.stage = Stage::Failed(error);
        }
        cx.notify();
    }

    /// The Cockpit saved the finished setup, or could not.
    pub(super) fn added(&mut self, saved: Result<(), String>, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let warning = editor.warning.take();
        match saved {
            Err(error) => editor.stage = Stage::Failed(error),
            Ok(()) => {
                if let Stage::Adding {
                    attempt: Attempt::Remote(attempt),
                    ..
                } = &mut editor.stage
                {
                    attempt.commit();
                }
                self.warning = warning;
                self.editor = None;
            }
        }
        cx.notify();
    }

    /// The Cockpit's answer to a confirmed Remove.
    pub(super) fn removed(&mut self, saved: Result<(), String>, cx: &mut Context<Self>) {
        self.editor = None;
        if saved.is_ok() {
            self.removing = None;
        }
        self.error = saved.err();
        cx.notify();
    }

    /// What went wrong after a change was saved, said under the
    /// environments.
    pub(super) fn warn(&mut self, warning: String, cx: &mut Context<Self>) {
        self.warning = Some(warning);
        cx.notify();
    }

    fn form(
        &mut self,
        saved: &[Machine],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> settings::ConnectionForm {
        let submit = action(cx, Self::submit);
        let cancel = action(cx, |view, _, cx| view.cancel(cx));
        let verify = action(cx, Self::answer);
        let view = cx.entity().downgrade();
        let choose_host = Rc::new(
            move |host: SharedString, window: &mut Window, cx: &mut App| {
                let _ = view.update(cx, |view, cx| view.choose_host(host, window, cx));
            },
        );
        let view = cx.entity().downgrade();
        let choose_mode = Rc::new(move |mode, window: &mut Window, cx: &mut App| {
            let _ = view.update(cx, |view, cx| view.set_mode(mode, window, cx));
        });
        let open_suggestions = action(cx, |view, _, cx| {
            if let Some(editor) = &mut view.editor {
                editor.suggestions_open = true;
            }
            cx.notify();
        });
        let hosts = self.hosts_matching(saved, cx);
        let Some(editor) = self.editor.as_mut() else {
            return settings::ConnectionForm::Closed {
                open: action(cx, |view, window, cx| view.edit(None, window, cx)),
                error: self.error.clone().map(Into::into),
            };
        };
        let mut prompt = None;
        let (progress, error) = match &mut editor.stage {
            Stage::Editing => (None, None),
            Stage::Resolving => (Some("Resolving SSH host…".into()), None),
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
                                    view.answer(window, cx);
                                }
                            },
                        )
                        .detach();
                        response.update(cx, |field, cx| field.focus(window, cx));
                        verification.input = VerificationInput::Secret(response);
                    } else if matches!(verification.input, VerificationInput::Pending) {
                        window.focus(&verification.focus, cx);
                        verification.input = VerificationInput::Confirm;
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
                        focus: Some(verification.focus.clone()),
                        expires: format!("Expires in {}:{:02}", seconds / 60, seconds % 60).into(),
                        submit: verify,
                    });
                }
                (Some(phase.label().into()), None)
            }
        };
        let suggestions = (editor.mode == ConnectionMode::Ssh
            && editor.suggestions_open
            && progress.is_none()
            && (!editor.discovery.hosts.is_empty()
                || editor.discovery.pending
                || editor.discovery.error.is_some()))
        .then(|| settings::ConnectionSuggestions {
            message: if let Some(error) = &editor.discovery.error {
                Some(error.clone().into())
            } else if hosts.is_empty() {
                Some(if editor.discovery.pending {
                    "Loading hosts…".into()
                } else {
                    format!(
                        "No hosts match \"{}\".",
                        editor.fields.host.read(cx).value()
                    )
                    .into()
                })
            } else {
                None
            },
            rows: hosts
                .into_iter()
                .map(|host| settings::ConnectionSuggestion {
                    value: host.alias.clone().into(),
                    label: host.alias.clone().into(),
                    address: if host.hostname == host.alias {
                        "".into()
                    } else {
                        host.hostname.into()
                    },
                })
                .collect(),
            highlighted: editor.highlighted,
            scroll: editor.suggestion_scroll.clone(),
        });
        settings::ConnectionForm::Editing {
            fields: editor.fields.clone(),
            mode: editor.mode,
            signing_in: editor
                .original
                .as_ref()
                .filter(|machine| machine.is_link())
                .map(|machine| machine.name.clone().into()),
            focus: editor.focus.clone(),
            suggestions,
            choose_mode,
            open_suggestions,
            choose_host,
            progress,
            error,
            warning: editor.warning.clone().map(Into::into),
            verification: prompt,
            submit,
            cancel,
        }
    }

    /// The Connections page's groups, for the Settings sheet.
    pub(super) fn groups(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<settings::Group> {
        let ConnectionsInputs {
            dir,
            saved,
            local_enabled,
            switchable,
            listed,
        } = self.inputs(cx);
        let local = connection_row(settings::ConnectionRow {
            name: "Local environment".into(),
            target: "".into(),
            status: if local_enabled {
                "Run agents on this computer. Turn off to use Ferrite only with remote environments."
            } else {
                "Turned off. This client only uses remote environments. Running agents are unaffected."
            }.into(),
            connected: local_enabled,
            enabled: local_enabled,
            detail: None,
            actions: vec![settings::ConnectionAction {
                id: "connection-toggle-local".into(),
                label: "Local environment".into(),
                enabled: switchable,
                act: action(cx, |_, _, cx| cx.emit(ConnectionsEvent::ToggleLocal)),
            }],
            removal: None,
            sync: None,
        });
        let form = self.form(&saved, window, cx);
        let mut remote = vec![settings::Row::new(
            "Add environment",
            "SSH host alias username port",
            vec![],
            settings::Control::ConnectionForm(form),
        )];
        for Listed {
            row,
            machine,
            state,
            unsaved,
            editing,
            sync,
        } in listed
        {
            let index = row.index;
            let target = machine.target.clone();
            let changing = self.editor.as_ref().is_some_and(|editor| {
                matches!(editor.stage, Stage::Adding { .. })
                    && editor
                        .original
                        .as_ref()
                        .is_some_and(|original| original.target == target)
            });
            let mut actions = vec![settings::ConnectionAction {
                id: format!("connection-open-{index}").into(),
                label: "Open".into(),
                enabled: machine.enabled && !editing,
                act: action(cx, move |_, _, cx| cx.emit(ConnectionsEvent::Open(index))),
            }];
            if machine.enabled && row.connection != Connection::Online {
                let machine = machine.clone();
                let attention = match &state {
                    Some(ServerState::Detached { attention, .. }) => attention.as_ref(),
                    _ => None,
                };
                let revoked = attention == Some(&machine::Failure::Revoked);
                let label = match attention {
                    Some(machine::Failure::NotInstalled) => "Set up",
                    Some(machine::Failure::Revoked) => "Sign in",
                    _ => "Retry",
                };
                actions.push(settings::ConnectionAction {
                    id: format!("connection-retry-{index}").into(),
                    label: label.into(),
                    enabled: self.editor.is_none(),
                    act: action(cx, move |view, window, cx| {
                        view.edit(Some(machine.clone()), window, cx);
                        // Its session is gone: the editor waits for a code.
                        if !revoked {
                            view.submit(window, cx);
                        }
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
                    enabled: self.editor.is_none(),
                    act: action(cx, move |view, window, cx| {
                        view.edit(Some(saving.clone()), window, cx);
                        view.submit(window, cx);
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
                            view.edit(Some(reconnect.clone()), window, cx);
                            view.submit(window, cx);
                            return;
                        }
                        view.editor = None;
                        cx.emit(ConnectionsEvent::Toggle(toggled.clone()));
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
                    view.removing = Some(removing.clone());
                    cx.notify();
                }),
            });
            let removal = (self.removing.as_ref() == Some(&target)).then(|| {
                let machine = machine.clone();
                let confirm = action(cx, move |_, _, cx| {
                    cx.emit(ConnectionsEvent::Remove {
                        machine: machine.clone(),
                        unsaved,
                    })
                });
                let cancel = action(cx, |view, _, cx| {
                    view.removing = None;
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
            let sync = sync.map(|status| {
                use super::setup_sync::Status;
                use ferrite_core::tree::Side;
                let toggled = target.clone();
                let press = |name: &str, label: &str, event: ConnectionsEvent| settings::ConnectionAction {
                    id: format!("connection-sync-{name}-{index}").into(),
                    label: label.to_string().into(),
                    enabled: true,
                    act: action(cx, move |_, _, cx| cx.emit(event.clone())),
                };
                let actions = match &status {
                    Status::Stopped(_) => vec![press("anyway", "Sync anyway", ConnectionsEvent::SyncAnyway(target.clone()))],
                    status if status.sign_ins() => vec![
                        press("here", "Use this computer's", ConnectionsEvent::UseSignIns(target.clone(), Side::Here)),
                        press("there", &format!("Use {}'s", machine.name), ConnectionsEvent::UseSignIns(target.clone(), Side::There)),
                    ],
                    _ => Vec::new(),
                };
                // A window that syncs no setup draws the switch off, and
                // it does nothing.
                let refused = matches!(status, Status::Refused(_));
                settings::ConnectionSync {
                    on: machine.sync_setup && !refused,
                    toggle: settings::ConnectionAction {
                        id: format!("connection-sync-{index}").into(),
                        label: "Sync my setup".into(),
                        enabled: !unsaved && !refused,
                        act: action(cx, move |_, _, cx| cx.emit(ConnectionsEvent::ToggleSync(toggled.clone()))),
                    },
                    actions,
                    failed: status.failed(),
                    status: status.line(&machine.name).into(),
                }
            });
            let display_target = machine.display_target(&dir);
            remote.push(connection_row(settings::ConnectionRow {
                name: machine.name.into(),
                target: display_target.into(),
                status: status.into(),
                connected: row.connection == Connection::Online,
                enabled: machine.enabled,
                detail,
                actions,
                removal,
                sync,
            }));
        }
        let mut group = settings::Group::new(Some("Environments")).rows(remote);
        if let Some(warning) = &self.warning {
            group = group.note(warning.clone());
        }
        vec![settings::Group::new(Some("This app")).rows([local]), group]
    }
}

const ALREADY_SAVED: &str = "This environment is already saved. Reconnect it below.";

impl CockpitView {
    /// What Connections draws from. It never reads the view: the view
    /// reads it, from its own listeners and as the sheet draws.
    pub(super) fn connections_inputs(&self, cx: &App) -> ConnectionsInputs {
        let cockpit = self.cockpit.read(cx);
        let editing = self.settings_machine(cx);
        let listed = self
            .machines
            .as_ref()
            .map(|machines| {
                machines
                    .rows(cockpit)
                    .into_iter()
                    .filter_map(|row| {
                        let machine = machines.machine(row.index)?.clone();
                        let state = machines.server_state(row.index, cockpit);
                        let unsaved = machines
                            .unsaved()
                            .is_some_and(|unsaved| unsaved.target == machine.target);
                        let sync = self.setup_sync.read(cx).status(&machine.target);
                        Some(Listed {
                            editing: row.held.is_some() && row.held == editing,
                            row,
                            machine,
                            state,
                            unsaved,
                            sync,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        ConnectionsInputs {
            dir: self.prefs.dir.clone(),
            saved: self.prefs.settings.machines.clone(),
            local_enabled: self.prefs.settings.local_environment_enabled,
            switchable: self.machines.is_some(),
            listed,
        }
    }

    /// Connections, while Settings is up.
    pub(crate) fn connections(&self, cx: &App) -> Option<Entity<ConnectionsView>> {
        Some(self.settings_sheet(cx)?.read(cx).connections.clone())
    }

    pub(super) fn on_connections_event(
        &mut self,
        view: Entity<ConnectionsView>,
        event: &ConnectionsEvent,
        cx: &mut Context<Self>,
    ) {
        match event.clone() {
            ConnectionsEvent::Open(index) => {
                let held = self.machines.as_ref().and_then(|machines| machines.held(index));
                if let Some(machine) = held {
                    self.edit_machine_settings(machine, cx);
                }
            }
            ConnectionsEvent::ToggleLocal => {
                let error = self.toggle_local_environment(cx).err();
                view.update(cx, |view, cx| view.answered(error, cx));
            }
            ConnectionsEvent::Toggle(target) => {
                let saved = self.save_connections(
                    |saved| {
                        for machine in saved.iter_mut().filter(|machine| machine.target == target) {
                            machine.enabled = !machine.enabled;
                        }
                    },
                    cx,
                );
                view.update(cx, |view, cx| view.answered(saved.err(), cx));
            }
            ConnectionsEvent::ToggleSync(target) => {
                let saved = self.save_connections(
                    |saved| {
                        for machine in saved.iter_mut().filter(|machine| machine.target == target) {
                            machine.sync_setup = !machine.sync_setup;
                        }
                    },
                    cx,
                );
                self.tick_setup_sync(cx);
                view.update(cx, |view, cx| view.answered(saved.err(), cx));
            }
            ConnectionsEvent::SyncAnyway(target) => self.sync_setup_anyway(&target, cx),
            ConnectionsEvent::UseSignIns(target, side) => self.sync_setup_sign_ins(&target, side, cx),
            ConnectionsEvent::Remove { machine, unsaved } => {
                self.remove_connection(&view, machine, unsaved, cx)
            }
            ConnectionsEvent::Ready { machine, replacing } => {
                self.add_connection(&view, machine, replacing, cx)
            }
        }
        cx.notify();
    }

    /// Why the window cannot let the environment `target` names go now
    /// (`files_leave_error`); `None` when it is not held.
    fn leave_error(&self, target: &str, cx: &App) -> Option<&'static str> {
        let machines = self.machines.as_ref()?;
        let held = machines.held(machines.named(target)?)?;
        self.files_leave_error(held, cx)
    }

    /// Connections' own save: unlike `change_settings`, it saves nothing
    /// in memory that it could not write, and refuses to let go of an
    /// environment that has files left unsaved.
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
        let leaving = self.prefs.settings.machines.iter().filter(|machine| {
            machine.enabled
                && !settings
                    .machines
                    .iter()
                    .any(|kept| kept.target == machine.target && kept.enabled)
        });
        for machine in leaving {
            if let Some(error) = self.leave_error(&machine.target, cx) {
                return Err(error.into());
            }
        }
        settings
            .save(&self.prefs.dir)
            .map_err(|error| format!("Environment not saved: {error}. Try again."))?;
        self.prefs.settings = settings;
        self.cockpit.update(cx, |cockpit, _| cockpit.settings_changed(&self.prefs.settings));
        self.machines_changed(cx);
        cx.notify();
        Ok(())
    }

    fn add_connection(
        &mut self,
        view: &Entity<ConnectionsView>,
        machine: Machine,
        replacing: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if connection_exists(&machine, replacing.as_deref(), &self.prefs.settings.machines) {
            view.update(cx, |view, cx| view.refused(ALREADY_SAVED.into(), cx));
            return;
        }
        let saved = self.save_connections(
            |saved| {
                if let Some(existing) = saved
                    .iter_mut()
                    .find(|saved| replacing.as_ref() == Some(&saved.target))
                {
                    *existing = machine.clone();
                } else {
                    saved.push(machine.clone());
                }
            },
            cx,
        );
        let added = saved.is_ok();
        view.update(cx, |view, cx| view.added(saved, cx));
        if !added {
            return;
        }
        if let Some(machines) = &self.machines {
            if let Some(index) = machines.named(&machine.target) {
                self.cockpit
                    .update(cx, |cockpit, _| machines.retry(index, cockpit));
            }
        }
    }

    fn remove_connection(
        &mut self,
        view: &Entity<ConnectionsView>,
        machine: Machine,
        unsaved: bool,
        cx: &mut Context<Self>,
    ) {
        let target = machine.target.clone();
        if let Some(error) = self.leave_error(&target, cx) {
            view.update(cx, |view, cx| view.answered(Some(error.into()), cx));
            return;
        }
        let saved = self.save_connections(|saved| saved.retain(|saved| saved.target != target), cx);
        let removed = saved.is_ok();
        view.update(cx, |view, cx| view.removed(saved, cx));
        if !removed {
            return;
        }
        if machine.is_link() {
            let directory = self.prefs.dir.clone();
            let view = view.downgrade();
            cx.spawn(async move |_, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { remote::forget(&machine, &directory) })
                    .await;
                if let Err(error) = result {
                    let warning = format!(
                        "Environment removed, but its saved credential could not be deleted: {error}"
                    );
                    let _ = view.update(cx, |view, cx| view.warn(warning, cx));
                }
            })
            .detach();
        }
        if unsaved {
            let before: Vec<MachineId> = self.cockpit.read(cx).machines().collect();
            if let Some(machines) = &mut self.machines {
                self.cockpit
                    .update(cx, |cockpit, _| machines.forget_unsaved(&target, cockpit));
            }
            self.machines_let_go(&before, cx);
        }
    }

    /// Like `save_connections`, it keeps nothing it could not write.
    fn toggle_local_environment(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if let Some(error) = &self.prefs.unreadable {
            return Err(format!("Settings cannot be saved: {error}"));
        }
        let enabled = !self.prefs.settings.local_environment_enabled;
        if !enabled {
            if let Some(error) = self.leave_error("", cx) {
                return Err(error.into());
            }
        }
        let mut settings = self.prefs.settings.clone();
        settings.local_environment_enabled = enabled;
        settings
            .save(&self.prefs.dir)
            .map_err(|error| format!("Local environment not saved: {error}. Try again."))?;
        self.prefs.settings = settings;
        self.machines_changed(cx);
        Ok(())
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

fn inserted_multiple_characters(previous: &str, value: &str) -> bool {
    let prefix = previous
        .chars()
        .zip(value.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let previous: Vec<_> = previous.chars().skip(prefix).collect();
    let value: Vec<_> = value.chars().skip(prefix).collect();
    let suffix = previous
        .iter()
        .rev()
        .zip(value.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    value.len().saturating_sub(suffix) > 1
}

/// Another saved environment already has `machine`'s target; `original`
/// is the target of the one being set up again, if one is.
fn connection_exists(machine: &Machine, original: Option<&str>, saved: &[Machine]) -> bool {
    saved.iter().any(|saved| {
        saved.target == machine.target && original.is_none_or(|original| original != saved.target)
    })
}

fn matching_hosts(
    hosts: &[machine::hosts::Host],
    typed: &str,
    saved: &[Machine],
) -> Vec<machine::hosts::Host> {
    let typed = typed.trim().to_lowercase();
    let mut matches: Vec<_> = hosts
        .iter()
        .filter(|host| {
            (host.alias.to_lowercase().contains(&typed)
                || host.hostname.to_lowercase().contains(&typed))
                && !saved
                    .iter()
                    .any(|machine| machine.ssh_fields().0 == host.alias)
        })
        .cloned()
        .collect();
    matches.sort_by_key(|host| {
        (
            !host.alias.to_lowercase().starts_with(&typed),
            host.alias.to_lowercase(),
        )
    });
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(alias: &str, hostname: &str) -> machine::hosts::Host {
        machine::hosts::Host {
            alias: alias.into(),
            hostname: hostname.into(),
            username: None,
            port: None,
            source: "config".into(),
        }
    }

    #[test]
    fn ssh_autocomplete_prioritizes_alias_prefixes_then_matches_resolved_addresses() {
        let hosts = vec![
            host("my-dev", "other.example"),
            host("zebra", "dev.example"),
            host("Devbox", "10.0.0.2"),
            host("dev", "10.0.0.1"),
            host("unrelated", "10.0.0.3"),
        ];
        let names: Vec<_> = matching_hosts(&hosts, "DEV", &[])
            .into_iter()
            .map(|host| host.alias)
            .collect();
        assert_eq!(names, ["dev", "Devbox", "my-dev", "zebra"]);
        assert_eq!(matching_hosts(&hosts, "10.0.0.2", &[])[0].alias, "Devbox");
        assert!(matching_hosts(&hosts, "missing", &[]).is_empty());
    }

    #[test]
    fn ssh_autocomplete_omits_saved_aliases_and_keeps_every_other_match() {
        let hosts: Vec<_> = (0..12)
            .map(|at| host(&format!("dev{at:02}"), "example.com"))
            .collect();
        let saved = Machine::from_ssh_fields("dev03", "developer", "2222").unwrap();
        let found = matching_hosts(&hosts, "dev", &[saved]);
        assert_eq!(found.len(), 11);
        assert!(found.iter().all(|host| host.alias != "dev03"));
        assert_eq!(found.last().unwrap().alias, "dev11");
    }
}
