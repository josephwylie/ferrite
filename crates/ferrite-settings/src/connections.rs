//! Connections (ADR 0041): the Settings page that adds an environment,
//! and retries, disconnects and removes the saved ones. It lives with the
//! Settings sheet, so what it holds (the editor and the attempt it runs, a
//! Remove waiting for its confirm, the last error and warning) goes when
//! the sheet does. What it shows of the window it reads through a
//! `Source` (`ConnectionsInputs`); what changes the saved environments, or
//! goes to one, it says as a `ConnectionsEvent`, which the window saves
//! and answers (`answered`, `refused`, `added`, `removed`, `warn`); what
//! it asks of the sheet, as a `PageEvent`.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use ferrite_core::machine::{self, auth, remote, setup, Machine};
use ferrite_core::tree::Side;
use ferrite_ui::frame::settings::{self, ConnectionMode};
use ferrite_ui::source::Source;
use gpui::component::input::{InputEvent, InputState};
use gpui::prelude::*;
use gpui::{
    px, App, Context, Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, SharedString,
    Window,
};

pub struct ConnectionsView {
    /// The environments as the window has them.
    inputs: Source<ConnectionsInputs, ()>,
    editor: Option<Editor>,
    // Not one state with the editor: a Remove can wait while an
    // environment is being added, and an error said while the editor is
    // open shows once it closes.
    /// The saved environment whose Remove waits for its confirm.
    removing: Option<String>,
    /// The saved environment whose projects folder is being changed, and
    /// the field it is typed in.
    projects: Option<(String, Entity<InputState>)>,
    warning: Option<String>,
    error: Option<String>,
    generation: u64,
}

/// What Connections asks of the window.
#[derive(Clone, Debug, PartialEq)]
pub enum ConnectionsEvent {
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
    UseSignIns(String, Side),
    /// Remove a saved environment, as confirmed.
    Remove { machine: Machine, unsaved: bool },
    /// Copy Projects into this folder on the saved environment `target`.
    ProjectsDir { target: String, dir: String },
    /// A setup or pairing finished: save `machine` in place of the
    /// environment it set up again, if it did.
    Ready {
        machine: Machine,
        replacing: Option<String>,
    },
}

impl EventEmitter<ConnectionsEvent> for ConnectionsView {}

/// What Connections asks of the sheet it is a page of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageEvent {
    /// An edit began: show the Connections page whole.
    Editing,
    /// The editor was cancelled: the sheet takes the keyboard back.
    Cancelled,
}

impl EventEmitter<PageEvent> for ConnectionsView {}

/// What Connections draws from, as the window has it now.
#[derive(Clone, Default)]
pub struct ConnectionsInputs {
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

/// How the window reaches an environment, as its row says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reached {
    /// Switched off: the window holds no server for it.
    Off,
    /// Its server is attached, or runs in this process.
    Online,
    /// Its server is not attached: `reason` says why, and `attention`
    /// what the operator must do before trying again helps.
    Detached {
        reason: String,
        attention: Option<machine::Failure>,
    },
}

/// One environment of the machines list.
#[derive(Clone)]
pub struct Listed {
    /// Its place in the window's machines list.
    pub index: usize,
    pub reached: Reached,
    pub machine: Machine,
    /// Named for this run (`--remote`), but not saved in Settings.
    pub unsaved: bool,
    /// Settings' defaults and CLIs are about it now.
    pub editing: bool,
    /// Where its setup sync stands; `None` when the window syncs none.
    pub sync: Option<Sync>,
}

/// Where an environment's setup sync stands, as its row says it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sync {
    /// Why this window syncs no setup: the switch is drawn off, and does
    /// nothing.
    pub refused: bool,
    /// It stopped short of removing much: it can be made to go on.
    pub stopped: bool,
    /// Two sign-ins wait for the operator to say whose to keep.
    pub sign_ins: bool,
    pub failed: bool,
    /// What the row says of it.
    pub line: String,
}

pub struct Editor {
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
    /// Connections, reading the environments through `inputs`.
    pub fn new(inputs: Source<ConnectionsInputs, ()>) -> Self {
        Self {
            inputs,
            editor: None,
            removing: None,
            projects: None,
            warning: None,
            error: None,
            generation: 0,
        }
    }

    /// Never called while the window is being updated: it reads it.
    /// None once the window has gone, or has no environments to say.
    fn inputs(&self, cx: &App) -> Option<ConnectionsInputs> {
        self.inputs.read(&(), cx)
    }

    /// The editor, opened by a press of the page's own: on `machine`, as
    /// the window says it. With no window, none opens.
    fn edit_here(&mut self, machine: Option<Machine>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ConnectionsInputs { dir, .. }) = self.inputs(cx) {
            self.edit(machine, &dir, window, cx);
        }
    }

    /// An environment is being added, or set up again.
    pub fn editing(&self) -> bool {
        self.editor.is_some()
    }

    /// The editor goes, and the attempt it runs with it.
    pub fn close_editor(&mut self) {
        self.editor = None;
    }

    pub fn holds_focus(&self, window: &Window, cx: &App) -> bool {
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

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        self.editor = None;
        cx.emit(PageEvent::Cancelled);
        cx.notify();
    }

    /// The editor, open on `machine` to set it up again, or on a new
    /// environment. `dir` is where the window's settings live, which
    /// spells a link's target; it is given, so the window can open the
    /// editor while it is being updated.
    pub fn edit(
        &mut self,
        machine: Option<Machine>,
        dir: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(PageEvent::Editing);
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
            .map(|machine| machine.display_target(dir))
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

    /// The environments the window saves; none once it has gone.
    fn saved(&self, cx: &App) -> Vec<Machine> {
        self.inputs(cx).map(|inputs| inputs.saved).unwrap_or_default()
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
            .and_then(|at| self.hosts_matching(&self.saved(cx), cx).get(at).cloned())
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
        let hosts = self.hosts_matching(&self.saved(cx), cx);
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
        let Some(ConnectionsInputs { dir, saved, .. }) = self.inputs(cx) else {
            // No window to save a setup in: the edit ends, and no attempt
            // starts.
            if self.editor.take().is_some() {
                cx.notify();
            }
            return;
        };
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

    /// Take what the attempt in flight said. The window's pump calls it
    /// each frame, so it reads nothing of the window.
    pub fn poll(&mut self, cx: &mut Context<Self>) {
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
            // The window's answer to `Ready` ends the attempt; nothing
            // after it is taken.
            if ready {
                return;
            }
        }
    }

    pub fn setup_event(&mut self, event: setup::Event, cx: &mut Context<Self>) {
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

    /// Change `machine`'s projects folder in its row, starting from the
    /// one it has.
    fn edit_projects_dir(&mut self, machine: &Machine, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(machine::PROJECTS_DIR));
        input.update(cx, |input, cx| {
            input.set_value(machine.projects_dir.clone(), window, cx);
            input.focus(window, cx);
        });
        cx.subscribe_in(&input, window, |view, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                view.save_projects_dir(cx);
            }
        })
        .detach();
        self.projects = Some((machine.target.clone(), input));
        cx.notify();
    }

    /// Keep the folder typed, or the default for none.
    fn save_projects_dir(&mut self, cx: &mut Context<Self>) {
        let Some((target, input)) = self.projects.take() else {
            return;
        };
        let typed = input.read(cx).value().trim().to_string();
        let dir = if typed.is_empty() { machine::PROJECTS_DIR.to_string() } else { typed };
        cx.emit(ConnectionsEvent::ProjectsDir { target, dir });
        cx.notify();
    }

    /// The window's answer to a change asked of it: the error it met,
    /// if any.
    pub fn answered(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        self.error = error;
        cx.notify();
    }

    /// The setup finished, but is not saved: why. The editor stays, to
    /// try again.
    pub fn refused(&mut self, error: String, cx: &mut Context<Self>) {
        if let Some(editor) = &mut self.editor {
            editor.stage = Stage::Failed(error);
        }
        cx.notify();
    }

    /// The window saved the finished setup, or could not.
    pub fn added(&mut self, saved: Result<(), String>, cx: &mut Context<Self>) {
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

    /// The window's answer to a confirmed Remove.
    pub fn removed(&mut self, saved: Result<(), String>, cx: &mut Context<Self>) {
        self.editor = None;
        if saved.is_ok() {
            self.removing = None;
        }
        self.error = saved.err();
        cx.notify();
    }

    /// What went wrong after a change was saved, said under the
    /// environments.
    pub fn warn(&mut self, warning: String, cx: &mut Context<Self>) {
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
                open: action(cx, |view, window, cx| view.edit_here(None, window, cx)),
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

    /// The Connections page's groups, for the Settings sheet: none once
    /// the window has gone.
    pub fn groups(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<settings::Group> {
        let Some(ConnectionsInputs {
            dir,
            saved,
            local_enabled,
            switchable,
            listed,
        }) = self.inputs(cx)
        else {
            return Vec::new();
        };
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
            projects: None,
        });
        let form = self.form(&saved, window, cx);
        let mut remote = vec![settings::Row::new(
            "Add environment",
            "SSH host alias username port",
            vec![],
            settings::Control::ConnectionForm(form),
        )];
        for Listed {
            index,
            reached,
            machine,
            unsaved,
            editing,
            sync,
        } in listed
        {
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
            if machine.enabled && reached != Reached::Online {
                let machine = machine.clone();
                let attention = match &reached {
                    Reached::Detached { attention, .. } => attention.as_ref(),
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
                        view.edit_here(Some(machine.clone()), window, cx);
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
                        view.edit_here(Some(saving.clone()), window, cx);
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
                            view.edit_here(Some(reconnect.clone()), window, cx);
                            view.submit(window, cx);
                            return;
                        }
                        view.editor = None;
                        cx.emit(ConnectionsEvent::Toggle(toggled.clone()));
                        cx.notify();
                    }),
                });
            }
            let folder = machine.clone();
            actions.push(settings::ConnectionAction {
                id: format!("connection-projects-{index}").into(),
                label: "Projects folder…".into(),
                enabled: self.editor.is_none(),
                act: action(cx, move |view, window, cx| view.edit_projects_dir(&folder, window, cx)),
            });
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
            let connected = reached == Reached::Online;
            let (status, detail) = match reached {
                Reached::Off => ("Disconnected", None),
                Reached::Online => ("Connected", None),
                Reached::Detached { reason, attention } => (
                    if attention.is_some() {
                        "Needs attention"
                    } else {
                        "Connecting…"
                    },
                    Some(reason.into()),
                ),
            };
            let sync = sync.map(|status| {
                let toggled = target.clone();
                let press = |name: &str, label: &str, event: ConnectionsEvent| settings::ConnectionAction {
                    id: format!("connection-sync-{name}-{index}").into(),
                    label: label.to_string().into(),
                    enabled: true,
                    act: action(cx, move |_, _, cx| cx.emit(event.clone())),
                };
                let actions = match &status {
                    Sync { stopped: true, .. } => vec![press("anyway", "Sync anyway", ConnectionsEvent::SyncAnyway(target.clone()))],
                    Sync { sign_ins: true, .. } => vec![
                        press("here", "Use this computer's", ConnectionsEvent::UseSignIns(target.clone(), Side::Here)),
                        press("there", &format!("Use {}'s", machine.name), ConnectionsEvent::UseSignIns(target.clone(), Side::There)),
                    ],
                    _ => Vec::new(),
                };
                // A window that syncs no setup draws the switch off, and
                // it does nothing.
                let refused = status.refused;
                settings::ConnectionSync {
                    on: machine.sync_setup && !refused,
                    toggle: settings::ConnectionAction {
                        id: format!("connection-sync-{index}").into(),
                        label: "Sync my setup".into(),
                        enabled: !unsaved && !refused,
                        act: action(cx, move |_, _, cx| cx.emit(ConnectionsEvent::ToggleSync(toggled.clone()))),
                    },
                    actions,
                    failed: status.failed,
                    status: status.line.into(),
                }
            });
            let projects = self
                .projects
                .as_ref()
                .filter(|(editing, _)| *editing == target)
                .map(|(_, input)| settings::ProjectsFolder {
                    input: input.clone(),
                    save: action(cx, |view, _, cx| view.save_projects_dir(cx)),
                    cancel: action(cx, |view, _, cx| {
                        view.projects = None;
                        cx.notify();
                    }),
                });
            let display_target = machine.display_target(&dir);
            remote.push(connection_row(settings::ConnectionRow {
                name: machine.name.into(),
                target: display_target.into(),
                status: status.into(),
                connected,
                enabled: machine.enabled,
                detail,
                actions,
                removal,
                sync,
                projects,
            }));
        }
        let mut group = settings::Group::new(Some("Environments")).rows(remote);
        if let Some(warning) = &self.warning {
            group = group.note(warning.clone());
        }
        vec![settings::Group::new(Some("This app")).rows([local]), group]
    }
}

/// Why a setup is not saved: its target is saved already.
pub const ALREADY_SAVED: &str = "This environment is already saved. Reconnect it below.";

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
pub fn connection_exists(machine: &Machine, original: Option<&str>, saved: &[Machine]) -> bool {
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

    use ferrite_ui::testing::{click, mount, record, shows};
    use gpui::{deferred, div, TestAppContext, VisualTestContext};

    /// The Settings sheet's part in it: the Connections page, drawn as the
    /// app draws the sheet (`ferrite_frame`), with the connection form over
    /// it as the sheet host lays it.
    struct Host {
        page: Entity<ConnectionsView>,
        search: Entity<InputState>,
        focus: FocusHandle,
    }

    impl Render for Host {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let groups = self.page.update(cx, |page, cx| page.groups(window, cx));
            let sheet = ferrite_ui::frame::SettingsSheet {
                pages: vec![settings::page(settings::PageKey::Connections, groups)],
                selected: settings::PageKey::Connections,
                search: self.search.clone(),
                focus: self.focus.clone(),
                select: Rc::new(|_, _, _| {}),
                step: Box::new(|_, _, _| {}),
                close: Box::new(|_, _| {}),
            };
            let drawn = ferrite_ui::frame::active().settings(sheet, window, cx);
            div()
                .size_full()
                .relative()
                .child(ferrite_ui::sheet::veil().child(drawn.card))
                .children(drawn.above.map(|above| deferred(above).with_priority(4)))
        }
    }

    /// What the window keeps of its environments, as a test keeps it.
    struct Kept(ConnectionsInputs);

    /// The app's frame, as it launches with it. Its key table names the
    /// window's actions, which are not here: the page's own keys are its
    /// fields' (the toolkit's) and the editor's, which it takes itself.
    fn launch(_: &mut TestAppContext) {
        ferrite_ui::frame::install(ferrite_frame::Frame { menus: Vec::new });
    }

    /// The Connections page in a window of its own, reading `inputs` as
    /// the window keeps them.
    fn open(
        inputs: ConnectionsInputs,
        cx: &mut TestAppContext,
    ) -> (Entity<ConnectionsView>, Entity<Kept>, &mut VisualTestContext) {
        launch(cx);
        let kept = cx.new(|_| Kept(inputs));
        let source = Source::of(&kept, |kept: &Kept, _: &(), _| Some(kept.0.clone()));
        let (host, cx) = mount(cx, move |window, cx| Host {
            page: cx.new(|_| ConnectionsView::new(source)),
            search: cx.new(|cx| InputState::new(window, cx).placeholder("Search")),
            focus: cx.focus_handle(),
        });
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        cx.run_until_parked();
        let page = host.read_with(cx, |host, _| host.page.clone());
        (page, kept, cx)
    }

    fn settle(cx: &mut VisualTestContext) {
        cx.run_until_parked();
    }

    fn groups(page: &Entity<ConnectionsView>, cx: &mut VisualTestContext) -> Vec<settings::Group> {
        page.update_in(cx, |page, window, cx| page.groups(window, cx))
    }

    fn fields(page: &Entity<ConnectionsView>, cx: &mut VisualTestContext) -> settings::ConnectionFields {
        groups(page, cx)
            .into_iter()
            .flat_map(|group| group.rows)
            .find_map(|row| match row.control {
                settings::Control::ConnectionForm(settings::ConnectionForm::Editing { fields, .. }) => Some(fields),
                _ => None,
            })
            .expect("the connection editor is open")
    }

    /// The environment rows, as the page lists them.
    fn rows(page: &Entity<ConnectionsView>, cx: &mut VisualTestContext) -> Vec<settings::ConnectionRow> {
        groups(page, cx)
            .into_iter()
            .flat_map(|group| group.rows)
            .filter_map(|row| match row.control {
                settings::Control::ConnectionRow(row) => Some(row),
                _ => None,
            })
            .collect()
    }

    /// The row button `id`, as its menu offers it.
    fn verb(page: &Entity<ConnectionsView>, id: &str, cx: &mut VisualTestContext) -> settings::ConnectionAction {
        rows(page, cx)
            .into_iter()
            .flat_map(|row| row.actions.into_iter().chain(row.sync.map(|sync| sync.toggle)))
            .find(|action| action.id.as_ref() == id)
            .unwrap_or_else(|| panic!("no {id}"))
    }

    fn press_verb(page: &Entity<ConnectionsView>, id: &str, cx: &mut VisualTestContext) {
        let act = verb(page, id, cx).act;
        cx.update(|window, cx| act(window, cx));
        settle(cx);
    }

    fn closed_error(page: &Entity<ConnectionsView>, cx: &mut VisualTestContext) -> Option<SharedString> {
        groups(page, cx)
            .into_iter()
            .flat_map(|group| group.rows)
            .find_map(|row| match row.control {
                settings::Control::ConnectionForm(settings::ConnectionForm::Closed { error, .. }) => error,
                _ => None,
            })
    }

    fn box_machine() -> Machine {
        Machine { name: "box".into(), ..Machine::new("me@box") }
    }

    /// One saved environment, `box`, listed at `index` 1, reached so.
    fn listed(reached: Reached) -> Listed {
        Listed {
            index: 1,
            reached,
            machine: box_machine(),
            unsaved: false,
            editing: false,
            sync: None,
        }
    }

    fn with(listed: Vec<Listed>) -> ConnectionsInputs {
        ConnectionsInputs {
            saved: listed.iter().map(|listed| listed.machine.clone()).collect(),
            local_enabled: true,
            switchable: true,
            listed,
            ..ConnectionsInputs::default()
        }
    }

    /// Add environment opens on a link: its host field has the keyboard,
    /// Tab and Shift-Tab keep to the editor, a refused pairing is said on
    /// it until SSH is chosen, and the editor opens afresh after its close;
    /// Escape cancels it and gives the sheet the keyboard back.
    #[gpui::test]
    fn the_editor_opens_on_a_link_keeps_tab_in_it_and_opens_afresh(cx: &mut TestAppContext) {
        let (page, _kept, cx) = open(ConnectionsInputs::default(), cx);
        let heard = record(cx, &page, |event: &PageEvent| *event);
        click(cx, "connection-add");
        assert_eq!(heard.take(), [PageEvent::Editing]);
        assert!(shows(cx, "connection-remote-host"));
        assert!(shows(cx, "connection-pairing-code"));
        assert!(!shows(cx, "connection-host"));
        let first = fields(&page, cx);
        cx.update(|window, cx| assert!(first.remote_host.read(cx).focus_handle(cx).is_focused(window)));
        let mut visited = Vec::new();
        for _ in 0..12 {
            cx.simulate_keystrokes("tab");
            settle(cx);
            let focused = cx.update(|window, cx| {
                assert!(page.read(cx).holds_focus(window, cx));
                window.focused(cx).unwrap()
            });
            if !visited.contains(&focused) {
                visited.push(focused);
            }
        }
        assert!(visited.len() >= 6, "Tab reaches the fields, modes, close, and submit");
        for _ in 0..12 {
            cx.simulate_keystrokes("shift-tab");
            settle(cx);
            cx.update(|window, cx| assert!(page.read(cx).holds_focus(window, cx)));
        }
        cx.update(|window, cx| {
            first.remote_host.update(cx, |field, cx| field.set_value("backend.example.com", window, cx));
            first.pairing_code.update(cx, |field, cx| field.set_value("", window, cx));
        });
        cx.simulate_keystrokes("enter");
        settle(cx);
        assert!(shows(cx, "connection-error"), "↵ submits");
        click(cx, "connection-mode-ssh");
        assert!(!shows(cx, "connection-error"));
        click(cx, "connection-mode-remote");
        click(cx, "connection-submit");
        assert!(shows(cx, "connection-error"), "and so does its button");
        click(cx, "connection-close");
        assert!(!page.read_with(cx, |page, _| page.editing()));
        assert_eq!(heard.take(), [PageEvent::Cancelled]);

        click(cx, "connection-add");
        let again = fields(&page, cx);
        assert_ne!(first.remote_host, again.remote_host);
        assert!(shows(cx, "connection-remote-host"));
        assert!(!shows(cx, "connection-host"));
        page.read_with(cx, |_, cx| {
            assert!(again.remote_host.read(cx).value().is_empty());
            assert!(again.pairing_code.read(cx).value().is_empty());
        });
        cx.simulate_keystrokes("escape");
        settle(cx);
        assert!(!page.read_with(cx, |page, _| page.editing()));
        assert_eq!(heard.take(), [PageEvent::Editing, PageEvent::Cancelled]);
    }

    /// While text is being composed in a field, ↵ submits nothing and
    /// Escape closes nothing; once composed, Escape closes the editor.
    #[gpui::test]
    fn composing_text_neither_submits_nor_closes(cx: &mut TestAppContext) {
        let (page, _kept, cx) = open(ConnectionsInputs::default(), cx);
        page.update_in(cx, |page, window, cx| page.edit(None, Path::new(""), window, cx));
        settle(cx);
        let fields = fields(&page, cx);
        cx.update(|window, cx| {
            fields.remote_host.update(cx, |input, cx| {
                input.focus(window, cx);
                input.replace_and_mark_text_in_range(None, "仮", None, window, cx);
            });
        });
        for key in ["enter", "escape"] {
            cx.simulate_keystrokes(key);
            settle(cx);
            assert!(shows(cx, "connection-dialog"), "{key}");
            assert!(!shows(cx, "connection-error"), "{key}");
        }
        cx.update(|window, cx| fields.remote_host.update(cx, |input, cx| input.unmark_text(window, cx)));
        cx.simulate_keystrokes("escape");
        settle(cx);
        assert!(!shows(cx, "connection-dialog"));
    }

    /// A pairing URL typed a character at a time stays in the host field
    /// as typed; it still holds the host and the code.
    #[gpui::test]
    fn a_pairing_url_typed_keeps_its_prefix(cx: &mut TestAppContext) {
        let (page, _kept, cx) = open(ConnectionsInputs::default(), cx);
        page.update_in(cx, |page, window, cx| page.edit(None, Path::new(""), window, cx));
        settle(cx);
        let fields = fields(&page, cx);
        let url = "https://backend.example.com/pair#code=0123456789abcdef0123456789abcdef&server=fedcba9876543210fedcba9876543210";
        let mut typed = String::new();
        for character in url.chars() {
            typed.push(character);
            cx.simulate_input(&character.to_string());
            settle(cx);
            page.read_with(cx, |_, cx| {
                assert_eq!(fields.remote_host.read(cx).value().as_ref(), typed.as_str());
                assert!(fields.pairing_code.read(cx).value().is_empty());
            });
        }
        page.read_with(cx, |_, cx| {
            let parsed = remote::PairingFields::parse(
                fields.remote_host.read(cx).value().as_ref(),
                fields.pairing_code.read(cx).value().as_ref(),
            )
            .unwrap();
            assert_eq!(parsed.host, "https://backend.example.com");
            assert_eq!(parsed.pairing_code, "0123456789abcdef0123456789abcdef");
        });
    }

    /// A pairing URL pasted over the host fills the host and the code.
    #[gpui::test]
    fn a_pairing_url_pasted_fills_host_and_code(cx: &mut TestAppContext) {
        let (page, _kept, cx) = open(ConnectionsInputs::default(), cx);
        page.update_in(cx, |page, window, cx| page.edit(None, Path::new(""), window, cx));
        settle(cx);
        let fields = fields(&page, cx);
        cx.simulate_input("https://backend.example.com");
        settle(cx);
        cx.simulate_keystrokes(&ferrite_ui::keymap::PLATFORM.chord("a"));
        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                "https://backend.example.com/pair#code=0123456789abcdef0123456789abcdef&server=fedcba9876543210fedcba9876543210".into(),
            ))
        });
        cx.simulate_keystrokes(&ferrite_ui::keymap::PLATFORM.chord("v"));
        settle(cx);
        page.read_with(cx, |_, cx| {
            assert_eq!(fields.remote_host.read(cx).value().as_ref(), "https://backend.example.com");
            assert_eq!(fields.pairing_code.read(cx).value().as_ref(), "0123456789abcdef0123456789abcdef");
        });
    }

    /// An SSH setup refused keeps what was typed, inside the sheet; a
    /// failed one keeps the same fields; Escape cancels the editor. A
    /// finished setup is asked of the window once, and once the window has
    /// saved it the editor is gone.
    #[gpui::test]
    fn a_failed_setup_keeps_what_was_typed_and_a_ready_one_is_asked_once(cx: &mut TestAppContext) {
        use ferrite_core::machine::setup::Event;
        let (page, _kept, cx) = open(ConnectionsInputs::default(), cx);
        cx.simulate_resize(gpui::size(px(800.), px(600.)));
        settle(cx);
        let heard = record(cx, &page, ConnectionsEvent::clone);
        click(cx, "connection-add");
        click(cx, "connection-mode-ssh");
        let typed = fields(&page, cx);
        cx.update(|window, cx| {
            typed.host.update(cx, |field, cx| field.set_value("-bad", window, cx));
            typed.username.update(cx, |field, cx| field.set_value("developer", window, cx));
            typed.port.update(cx, |field, cx| field.set_value("2222", window, cx));
        });
        cx.simulate_keystrokes("enter");
        settle(cx);
        assert!(shows(cx, "connection-error"));
        let card = cx.debug_bounds("connection-dialog").unwrap();
        for selector in ["connection-host", "connection-username", "connection-port", "connection-submit", "connection-close"] {
            let bounds = cx.debug_bounds(selector).expect(selector);
            assert!(bounds.left() >= card.left() && bounds.right() <= card.right(), "{selector} fits the small sheet");
        }
        page.read_with(cx, |_, cx| {
            assert_eq!(typed.host.read(cx).value().as_ref(), "-bad");
            assert_eq!(typed.username.read(cx).value().as_ref(), "developer");
            assert_eq!(typed.port.read(cx).value().as_ref(), "2222");
        });
        page.update(cx, |page, cx| page.setup_event(Event::Failed("Connection refused. Check the host and port.".into()), cx));
        settle(cx);
        assert_eq!(fields(&page, cx).host, typed.host, "a failure keeps the same fields");
        cx.simulate_keystrokes("escape");
        settle(cx);
        assert!(!page.read_with(cx, |page, _| page.editing()), "Escape cancels the editor");
        assert!(shows(cx, "connection-add"));
        assert_eq!(heard.take(), []);

        click(cx, "connection-add");
        let ready = Machine::new("ready.example");
        page.update(cx, |page, cx| page.setup_event(Event::Ready(ready.clone()), cx));
        assert_eq!(heard.take(), [ConnectionsEvent::Ready { machine: ready.clone(), replacing: None }]);
        page.update(cx, |page, cx| page.added(Ok(()), cx));
        page.update(cx, |page, cx| page.setup_event(Event::Ready(ready), cx));
        assert!(!page.read_with(cx, |page, _| page.editing()));
        assert_eq!(heard.take(), [], "saved: there is no editor to say it again");
    }

    /// The window refusing a finished setup keeps the editor, saying why;
    /// failing to save it, too. Set up again, a saved one is asked of the
    /// window in place of itself, and the setup's warning stays after.
    #[gpui::test]
    fn the_windows_answer_to_a_setup_is_said_on_the_page(cx: &mut TestAppContext) {
        use ferrite_core::machine::setup::Event;
        let saved = Machine { enabled: false, ..box_machine() };
        let (page, _kept, cx) = open(with(vec![Listed { machine: saved.clone(), ..listed(Reached::Off) }]), cx);
        let heard = record(cx, &page, ConnectionsEvent::clone);
        page.update_in(cx, |page, window, cx| page.edit(Some(saved.clone()), Path::new(""), window, cx));
        settle(cx);
        let ready = Machine { enabled: true, ..saved.clone() };
        page.update(cx, |page, cx| {
            page.setup_event(Event::Warning("Host key added.".into()), cx);
            page.setup_event(Event::Ready(ready.clone()), cx);
        });
        assert_eq!(heard.take(), [ConnectionsEvent::Ready { machine: ready.clone(), replacing: Some(saved.target.clone()) }]);
        let error = |cx: &mut VisualTestContext| {
            cx.update(|window, _| window.debug_text("connection-error")).unwrap_or_default().join(" ")
        };
        page.update(cx, |page, cx| page.refused(ALREADY_SAVED.into(), cx));
        settle(cx);
        assert!(page.read_with(cx, |page, _| page.editing()), "refused: the editor stays");
        assert!(error(cx).contains("already saved"), "{}", error(cx));
        page.update(cx, |page, cx| page.added(Ok(()), cx));
        settle(cx);
        assert!(!page.read_with(cx, |page, _| page.editing()));
        let note = groups(&page, cx).into_iter().find(|group| group.title == Some("Environments")).and_then(|group| group.note);
        assert_eq!(note.as_deref(), Some("Host key added."), "the setup's warning stays");

        page.update_in(cx, |page, window, cx| page.edit(Some(saved.clone()), Path::new(""), window, cx));
        page.update(cx, |page, cx| page.setup_event(Event::Ready(ready), cx));
        page.update(cx, |page, cx| page.added(Err("Environment not saved: denied. Try again.".into()), cx));
        settle(cx);
        assert!(page.read_with(cx, |page, _| page.editing()), "a failed save keeps the editor");
        assert!(error(cx).contains("not saved"), "{}", error(cx));
    }

    /// Each row says how its environment is reached, and offers what can
    /// be done about it: Retry while it connects, Sign in to a session
    /// the machine refused, Set up where Ferrite is not installed; Open
    /// asks the window, until Settings is about it already.
    #[gpui::test]
    fn each_row_says_how_its_environment_is_reached(cx: &mut TestAppContext) {
        let detached = |attention| Reached::Detached { reason: "no route".into(), attention };
        let cases = [
            (Reached::Online, "Connected", None),
            (detached(None), "Connecting…", Some("Retry")),
            (detached(Some(machine::Failure::Revoked)), "Needs attention", Some("Sign in")),
            (detached(Some(machine::Failure::NotInstalled)), "Needs attention", Some("Set up")),
        ];
        for (reached, status, retry) in cases {
            let connected = reached == Reached::Online;
            let (page, _kept, cx) = open(with(vec![listed(reached.clone())]), cx);
            let row = rows(&page, cx).pop().expect("box's row");
            assert_eq!(row.status.as_ref(), status);
            assert_eq!(row.detail.is_some(), !connected, "{status}: the reason is said");
            assert_eq!(row.connected, connected);
            let label = row.actions.iter().find(|action| action.id.as_ref() == "connection-retry-1").map(|action| action.label.to_string());
            assert_eq!(label.as_deref(), retry, "{status}");
        }
        let (page, kept, cx) = open(with(vec![listed(Reached::Off)]), cx);
        let heard = record(cx, &page, ConnectionsEvent::clone);
        let row = rows(&page, cx).pop().unwrap();
        assert_eq!(row.status.as_ref(), "Disconnected");
        assert!(verb(&page, "connection-open-1", cx).enabled);
        press_verb(&page, "connection-open-1", cx);
        assert_eq!(heard.take(), [ConnectionsEvent::Open(1)]);
        kept.update(cx, |kept, cx| {
            kept.0.listed[0].editing = true;
            cx.notify();
        });
        assert!(!verb(&page, "connection-open-1", cx).enabled, "Settings is about it already");
    }

    /// Disconnect is asked of the window, and closes the editor; this
    /// app's own switch too, while the window can let it go.
    #[gpui::test]
    fn a_switch_is_asked_of_the_window(cx: &mut TestAppContext) {
        let (page, kept, cx) = open(with(vec![listed(Reached::Online)]), cx);
        let heard = record(cx, &page, ConnectionsEvent::clone);
        assert_eq!(verb(&page, "connection-toggle-1", cx).label.as_ref(), "Disconnect");
        page.update_in(cx, |page, window, cx| page.edit(None, Path::new(""), window, cx));
        press_verb(&page, "connection-toggle-1", cx);
        assert!(!page.read_with(cx, |page, _| page.editing()));
        assert_eq!(heard.take(), [ConnectionsEvent::Toggle("me@box".into())]);
        let local = rows(&page, cx).remove(0);
        assert_eq!(local.name.as_ref(), "Local environment");
        assert!(local.actions[0].enabled);
        press_verb(&page, "connection-toggle-local", cx);
        assert_eq!(heard.take(), [ConnectionsEvent::ToggleLocal]);
        kept.update(cx, |kept, cx| {
            kept.0.switchable = false;
            cx.notify();
        });
        assert!(!rows(&page, cx).remove(0).actions[0].enabled, "no machines list: nothing to turn off");
        page.update(cx, |page, cx| page.answered(Some("Settings cannot be saved: bad".into()), cx));
        assert_eq!(closed_error(&page, cx).as_deref(), Some("Settings cannot be saved: bad"));
        page.update(cx, |page, cx| page.answered(None, cx));
        assert_eq!(closed_error(&page, cx), None);
        page.update(cx, |page, cx| page.warn("Your draft on box is put away until box is on again.".into(), cx));
        let note = groups(&page, cx).into_iter().find(|group| group.title == Some("Environments")).and_then(|group| group.note);
        assert_eq!(note.as_deref(), Some("Your draft on box is put away until box is on again."));
    }

    /// Remove asks first: its confirm asks the window, its cancel takes it
    /// back; the window's answer ends it, or says why not.
    #[gpui::test]
    fn remove_asks_first(cx: &mut TestAppContext) {
        let (page, _kept, cx) = open(with(vec![Listed { unsaved: true, ..listed(Reached::Online) }]), cx);
        let heard = record(cx, &page, ConnectionsEvent::clone);
        let removal = |cx: &mut VisualTestContext| rows(&page, cx).pop().unwrap().removal;
        assert!(removal(cx).is_none());
        press_verb(&page, "connection-remove-1", cx);
        let (_, cancel) = removal(cx).expect("it asks");
        cx.update(|window, cx| cancel(window, cx));
        assert!(removal(cx).is_none(), "taken back");
        assert_eq!(heard.take(), []);
        press_verb(&page, "connection-remove-1", cx);
        let (confirm, _) = removal(cx).unwrap();
        cx.update(|window, cx| confirm(window, cx));
        assert_eq!(heard.take(), [ConnectionsEvent::Remove { machine: box_machine(), unsaved: true }]);
        page.update(cx, |page, cx| page.removed(Err("files unsaved".into()), cx));
        assert!(removal(cx).is_some(), "refused: still asking");
        assert_eq!(closed_error(&page, cx).as_deref(), Some("files unsaved"));
        page.update(cx, |page, cx| page.removed(Ok(()), cx));
        assert!(removal(cx).is_none());
        assert_eq!(closed_error(&page, cx), None);
    }

    /// The editor offers the hosts found in the SSH config, less those
    /// saved: ↑/↓ walk them, Escape closes them first, typing opens them
    /// again, and ↵, a press or ⌘N takes one, which is looked up, and
    /// said on the editor when it cannot be.
    #[gpui::test]
    fn the_hosts_found_are_offered_as_the_host_is_typed(cx: &mut TestAppContext) {
        let saved = Machine::from_ssh_fields("dev03", "", "").unwrap();
        let (page, _kept, cx) = open(ConnectionsInputs { saved: vec![saved], ..ConnectionsInputs::default() }, cx);
        click(cx, "connection-add");
        click(cx, "connection-mode-ssh");
        let offer = |cx: &mut VisualTestContext| {
            page.update(cx, |page, cx| {
                let editor = page.editor.as_mut().unwrap();
                // In place of the machine's own config, and of the
                // reading of it still in flight.
                editor.discovery = Discovery {
                    hosts: vec![host("-bad", "10.0.0.9"), host("devbox", "10.0.0.2"), host("dev03", "10.0.0.3")],
                    generation: editor.discovery.generation.wrapping_add(1),
                    pending: false,
                    error: None,
                };
                editor.suggestions_open = true;
                cx.notify();
            });
            settle(cx);
        };
        let highlighted = |cx: &mut VisualTestContext| page.read_with(cx, |page, _| page.editor.as_ref().unwrap().highlighted);
        offer(cx);
        assert!(shows(cx, "connection-host-suggestion-0"));
        assert!(shows(cx, "connection-host-suggestion-1"));
        assert!(!shows(cx, "connection-host-suggestion-2"), "dev03 is saved");
        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("down");
        assert_eq!(highlighted(cx), Some(0), "down wraps");
        cx.simulate_keystrokes("up");
        assert_eq!(highlighted(cx), Some(1), "up wraps");
        cx.simulate_keystrokes("escape");
        settle(cx);
        assert!(!shows(cx, "connection-host-suggestions"), "Escape closes the hosts first");
        assert!(page.read_with(cx, |page, _| page.editing()));
        click(cx, "connection-host");
        assert!(shows(cx, "connection-host-suggestions"), "a press on the host offers them");
        let typing = fields(&page, cx);
        cx.update(|window, cx| typing.username.update(cx, |field, cx| field.focus(window, cx)));
        settle(cx);
        assert!(!shows(cx, "connection-host-suggestions"), "they go with the host's focus");
        cx.update(|window, cx| typing.host.update(cx, |field, cx| field.focus(window, cx)));
        settle(cx);
        assert!(shows(cx, "connection-host-suggestions"), "and come back with it");
        cx.simulate_keystrokes("escape");
        settle(cx);
        cx.simulate_input("-");
        settle(cx);
        assert!(shows(cx, "connection-host-suggestion-0"), "typing offers them again");
        assert!(!shows(cx, "connection-host-suggestion-1"), "only -bad has a -");
        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("enter");
        settle(cx);
        let error = cx.update(|window, _| window.debug_text("connection-error")).expect("it cannot be looked up").join(" ");
        assert!(!error.is_empty());
        let fields = fields(&page, cx);
        page.read_with(cx, |_, cx| assert_eq!(fields.host.read(cx).value().as_ref(), "-bad"));

        for take in ["press", "chord"] {
            click(cx, "connection-close");
            click(cx, "connection-add");
            click(cx, "connection-mode-ssh");
            offer(cx);
            match take {
                "press" => click(cx, "connection-host-suggestion-0"),
                _ => cx.simulate_keystrokes(&ferrite_ui::keymap::PLATFORM.chord("1")),
            }
            settle(cx);
            assert!(shows(cx, "connection-error"), "{take}: looked up");
            let fields = self::fields(&page, cx);
            page.read_with(cx, |_, cx| assert_eq!(fields.host.read(cx).value().as_ref(), "-bad", "{take}"));
        }
    }

    /// Retry, Save and Reconnect set the environment up again at once;
    /// Sign in opens its pairing, waiting for a code.
    #[gpui::test]
    fn retry_save_and_reconnect_set_it_up_again(cx: &mut TestAppContext) {
        // An SSH target `ssh` refuses before anything runs: the setup
        // fails in the editor, so it was started.
        let bad = Machine { name: "bad".into(), ..Machine::new("-bad") };
        let detached = Reached::Detached { reason: "no route".into(), attention: None };
        let cases = [
            ("connection-retry-1", Listed { machine: bad.clone(), ..listed(detached) }),
            ("connection-save-1", Listed { machine: bad.clone(), unsaved: true, ..listed(Reached::Online) }),
            ("connection-toggle-1", Listed { machine: Machine { enabled: false, ..bad.clone() }, ..listed(Reached::Off) }),
        ];
        for (id, row) in cases {
            let (page, _kept, cx) = open(with(vec![row]), cx);
            press_verb(&page, id, cx);
            assert!(page.read_with(cx, |page, _| page.editing()), "{id}");
            assert!(shows(cx, "connection-error"), "{id}: set up at once");
        }
        let link = Machine { name: "box".into(), ..Machine::new("ferrite-link:00112233445566778899aabbccddeeff") };
        let revoked = Reached::Detached { reason: "revoked".into(), attention: Some(machine::Failure::Revoked) };
        let (page, _kept, cx) = open(with(vec![Listed { machine: link, ..listed(revoked) }]), cx);
        press_verb(&page, "connection-retry-1", cx);
        let dialog = cx.update(|window, _| window.debug_text("connection-dialog")).expect("its pairing");
        assert!(dialog.iter().any(|line| line == "Sign in to box"), "{dialog:?}");
        assert!(!shows(cx, "connection-error"), "it waits for a code");
    }

    /// Projects folder… edits the folder in the row: the one typed, or the
    /// default for none, is asked of the window; its cancel keeps it.
    #[gpui::test]
    fn the_projects_folder_is_typed_in_its_row(cx: &mut TestAppContext) {
        let (page, _kept, cx) = open(with(vec![listed(Reached::Online)]), cx);
        let heard = record(cx, &page, ConnectionsEvent::clone);
        let folder = |cx: &mut VisualTestContext| rows(&page, cx).pop().unwrap().projects;
        press_verb(&page, "connection-projects-1", cx);
        let input = folder(cx).expect("its field").input;
        page.read_with(cx, |_, cx| assert_eq!(input.read(cx).value().as_ref(), box_machine().projects_dir.as_str()));
        cx.update(|window, cx| input.update(cx, |input, cx| input.set_value("  ~/work  ", window, cx)));
        let save = folder(cx).unwrap().save;
        cx.update(|window, cx| save(window, cx));
        assert_eq!(heard.take(), [ConnectionsEvent::ProjectsDir { target: "me@box".into(), dir: "~/work".into() }]);
        assert!(folder(cx).is_none());
        press_verb(&page, "connection-projects-1", cx);
        let input = folder(cx).unwrap().input;
        cx.update(|window, cx| input.update(cx, |input, cx| input.set_value("", window, cx)));
        let save = folder(cx).unwrap().save;
        cx.update(|window, cx| save(window, cx));
        assert_eq!(heard.take(), [ConnectionsEvent::ProjectsDir { target: "me@box".into(), dir: machine::PROJECTS_DIR.into() }]);
        press_verb(&page, "connection-projects-1", cx);
        assert!(shows(cx, "connection-projects-dir"));
        cx.simulate_input("~/enter");
        cx.simulate_keystrokes("enter");
        settle(cx);
        let typed = format!("{}~/enter", box_machine().projects_dir);
        assert_eq!(heard.take(), [ConnectionsEvent::ProjectsDir { target: "me@box".into(), dir: typed }], "↵ keeps it too");
        press_verb(&page, "connection-projects-1", cx);
        let cancel = folder(cx).unwrap().cancel;
        cx.update(|window, cx| cancel(window, cx));
        assert!(folder(cx).is_none());
        assert_eq!(heard.take(), []);
    }

    /// Sync my setup says where the sync stands; its switch is asked of the
    /// window, and so are going on past a stop and whose sign-ins to keep.
    /// A window that syncs no setup draws the switch off, taking no press.
    #[gpui::test]
    fn sync_my_setup_says_where_it_stands(cx: &mut TestAppContext) {
        let syncing = |sync: Sync| Listed { sync: Some(sync), machine: Machine { sync_setup: true, ..box_machine() }, ..listed(Reached::Online) };
        let (page, kept, cx) = open(with(vec![syncing(Sync { line: "In sync".into(), ..Sync::default() })]), cx);
        let heard = record(cx, &page, ConnectionsEvent::clone);
        let sync = |cx: &mut VisualTestContext| rows(&page, cx).pop().unwrap().sync.expect("its sync");
        let shown = sync(cx);
        assert!(shown.on && shown.toggle.enabled && !shown.failed && shown.actions.is_empty());
        assert_eq!(shown.status.as_ref(), "In sync");
        press_verb(&page, "connection-sync-1", cx);
        assert_eq!(heard.take(), [ConnectionsEvent::ToggleSync("me@box".into())]);

        let set = |sync: Sync, cx: &mut VisualTestContext| {
            kept.update(cx, |kept, cx| {
                kept.0.listed[0] = syncing(sync);
                cx.notify();
            })
        };
        set(Sync { stopped: true, failed: true, line: "Stopped: it removes much".into(), ..Sync::default() }, cx);
        let shown = sync(cx);
        assert!(shown.failed);
        let anyway = shown.actions.into_iter().map(|action| (action.id.to_string(), action.label.to_string(), action.act)).collect::<Vec<_>>();
        assert_eq!(anyway.iter().map(|(id, label, _)| (id.as_str(), label.as_str())).collect::<Vec<_>>(), [("connection-sync-anyway-1", "Sync anyway")]);
        cx.update(|window, cx| (anyway[0].2)(window, cx));
        assert_eq!(heard.take(), [ConnectionsEvent::SyncAnyway("me@box".into())]);

        set(Sync { sign_ins: true, line: "Signed in differently".into(), ..Sync::default() }, cx);
        let verbs = sync(cx).actions;
        assert_eq!(verbs.iter().map(|action| action.label.to_string()).collect::<Vec<_>>(), ["Use this computer's", "Use box's"]);
        for action in verbs {
            cx.update(|window, cx| (action.act)(window, cx));
        }
        assert_eq!(heard.take(), [ConnectionsEvent::UseSignIns("me@box".into(), Side::Here), ConnectionsEvent::UseSignIns("me@box".into(), Side::There)]);

        set(Sync { refused: true, line: "This window syncs no setup".into(), ..Sync::default() }, cx);
        let shown = sync(cx);
        assert!(!shown.on && !shown.toggle.enabled, "refused: drawn off, taking no press");
    }

    /// The page draws the environments as the window has them now. Once
    /// the window has gone it draws nothing and opens no editor, and an
    /// editor open then ends at its submit, starting no setup.
    #[gpui::test]
    fn the_page_reads_the_window(cx: &mut TestAppContext) {
        let (page, kept, cx) = open(with(vec![]), cx);
        assert_eq!(rows(&page, cx).len(), 1, "this app's own");
        kept.update(cx, |kept, cx| {
            kept.0.listed.push(listed(Reached::Online));
            cx.notify();
        });
        settle(cx);
        assert_eq!(rows(&page, cx).len(), 2);
        assert!(cx.update(|window, _| window.debug_text("settings-card")).unwrap().join(" ").contains("box"), "the window's notify redraws it");
        page.update_in(cx, |page, window, cx| page.edit(None, Path::new(""), window, cx));
        let typed = fields(&page, cx);
        cx.update(|window, cx| {
            typed.remote_host.update(cx, |field, cx| field.set_value("https://backend.example.com", window, cx));
            typed.pairing_code.update(cx, |field, cx| field.set_value("0123456789abcdef0123456789abcdef", window, cx));
        });
        let gone = kept.downgrade();
        drop(kept);
        cx.run_until_parked();
        assert!(gone.upgrade().is_none());
        assert!(groups(&page, cx).is_empty(), "no window: nothing drawn");
        page.update_in(cx, |page, window, cx| page.submit(window, cx));
        assert!(!page.read_with(cx, |page, _| page.editing()), "no window to save it in: the edit ends, and no setup starts");
        page.update_in(cx, |page, window, cx| page.edit_here(None, window, cx));
        assert!(!page.read_with(cx, |page, _| page.editing()), "and none opens");
    }
}
