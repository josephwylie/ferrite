//! Settings against the machine each value belongs to. The window's own
//! preferences (text size, sounds, the sidebar) live in this computer's
//! `settings.json`. A Server's defaults — how its Sessions ask for
//! permission, its Factory's limit, its follow-up suggestions — and its
//! provider CLIs belong to the machine the Server runs on. Settings edits
//! one machine's at a time, its page's own state (`Environment::machine`):
//! this computer's from its file, another machine's through its Server
//! (`Op::ServerDefaults`, `Op::Cli`), showing the file's own answer, never
//! this computer's.

use super::*;
use ferrite_core::protocol::{CliAction, CliBinary, Done};
use ferrite_core::settings::ServerDefaults;
use std::sync::mpsc::{Receiver, TryRecvError};

type Answer<T> = Receiver<std::io::Result<T>>;

/// The machine Settings edits, what the window has asked of it and not yet
/// heard, and the defaults it last heard.
#[derive(Default)]
pub(crate) struct Environment {
    /// The machine Settings' defaults and CLIs are about, as Connections
    /// opened it; `None` is this computer (`CockpitView::settings_machine`).
    machine: Option<MachineId>,
    /// The machine's defaults as its own `settings.json` last said, kept
    /// only for a machine other than this computer: this computer's are the
    /// window's.
    /// Edits show at once; the Server's reread replaces them when the last
    /// write lands.
    pub(super) defaults: Option<ServerDefaults>,
    /// Why the machine's defaults could not be read, or an edit written.
    pub(super) defaults_error: Option<DefaultsTrouble>,
    defaults_asked: Vec<DefaultsAsk>,
    versions: Option<Answer<Done>>,
    check: Option<Answer<Done>>,
    updates: Vec<(Provider, Answer<Done>)>,
}

/// What went wrong with the machine's defaults: a read that never
/// landed, or an edit the machine refused. A read that lands clears only
/// the first, so a refused edit stays shown beside the values read back.
pub(super) enum DefaultsTrouble {
    Unread(SharedString),
    Unsaved(SharedString),
}

/// A read or a write of the machine's defaults, out and not yet answered.
struct DefaultsAsk {
    answer: Answer<ServerDefaults>,
    write: bool,
}

/// The CLIs' line in Settings › About: the copy that answers, and where.
fn cli_line(binary: Option<&CliBinary>) -> SharedString {
    match binary {
        Some(found) => format!("{} · {}", found.version, found.path.display()).into(),
        None => "not found on PATH or in the usual install directories".into(),
    }
}

impl CockpitView {
    /// The machine Settings' defaults and CLIs are about: the one
    /// Connections opened, while the window holds it; else this computer;
    /// else the first machine the window holds.
    pub(super) fn settings_machine(&self, cx: &gpui::App) -> Option<MachineId> {
        let cockpit = self.cockpit.read(cx);
        self.environment
            .machine
            .filter(|machine| cockpit.machines().any(|held| held == *machine))
            .or(cockpit.start_machine())
    }

    /// Have Settings edit `machine`'s defaults and CLIs: what it heard of
    /// the one before is forgotten, and `machine` asked anew.
    pub(super) fn edit_machine_settings(&mut self, machine: MachineId, cx: &mut Context<Self>) {
        if self.settings_machine(cx) == Some(machine) {
            return;
        }
        self.environment = Environment {
            machine: Some(machine),
            ..Environment::default()
        };
        self.cli_versions = None;
        self.ask_server_defaults(cx);
        self.probe_cli_versions(cx);
        cx.notify();
    }

    /// Where the Sessions Settings is about run, as Settings says it.
    pub(super) fn machine_place(&self, cx: &gpui::App) -> SharedString {
        let Some(machine) = self.settings_machine(cx) else {
            return "No environment selected".into();
        };
        self.place_of(machine, cx)
    }

    /// What Settings' pages about a machine say atop them, as T3 Code's
    /// say the environment they apply to: `Applying settings on box`.
    /// With no machine held there is none to name.
    pub(super) fn settings_scope(&self, cx: &gpui::App) -> Option<SharedString> {
        self.settings_machine(cx)?;
        Some(format!("Applying settings on {}", self.machine_place(cx)).into())
    }

    /// Whether Settings is about a machine other than this computer, or
    /// none: with no machine held, this computer's files stand in for
    /// nothing.
    fn settings_elsewhere(&self, cx: &gpui::App) -> bool {
        self.settings_machine(cx)
            .is_none_or(|machine| self.elsewhere(machine, cx))
    }

    /// What the machine's failure reads as (`machines::said`).
    fn said(&self, error: &std::io::Error, cx: &gpui::App) -> SharedString {
        super::machines::said(error, &self.machine_place(cx)).into()
    }

    /// Read the machine's defaults, unless a read is out.
    pub(super) fn ask_server_defaults(&mut self, cx: &mut gpui::App) {
        if self.settings_elsewhere(cx) && self.environment.defaults_asked.is_empty() {
            self.send_defaults(None, cx);
        }
    }

    /// Send a read, or with `set` a write, of the machine's defaults.
    /// Whether it went out.
    fn send_defaults(&mut self, set: Option<ServerDefaults>, cx: &mut gpui::App) -> bool {
        let write = set.is_some();
        let Some(machine) = self.settings_machine(cx) else {
            return false;
        };
        match self
            .cockpit
            .update(cx, |cockpit, _| cockpit.server_defaults(machine, set))
        {
            Ok(answer) => {
                self.environment
                    .defaults_asked
                    .push(DefaultsAsk { answer, write });
                true
            }
            Err(error) => {
                self.defaults_failed(write, self.said(&error, cx));
                false
            }
        }
    }

    /// The defaults Settings shows: this computer's from its own file, or
    /// the machine's once it has said them.
    pub(super) fn shown_defaults(&self, cx: &gpui::App) -> Option<ServerDefaults> {
        if self.settings_elsewhere(cx) {
            self.environment.defaults.clone()
        } else {
            Some(self.prefs.settings.server_defaults())
        }
    }

    /// How many Factory Tasks the machine works at once, when known.
    pub(super) fn factory_limit(&self, cx: &gpui::App) -> Option<usize> {
        self.shown_defaults(cx)
            .map(|defaults| defaults.factory_tasks_at_once)
    }

    /// Change one default where it lives. On a machine Settings shows the
    /// edit at once, the Server writes it and rereads its file, and the
    /// last reread is what Settings keeps.
    pub(super) fn change_defaults(
        &mut self,
        change: impl FnOnce(&mut ServerDefaults),
        cx: &mut Context<Self>,
    ) {
        if !self.settings_elsewhere(cx) {
            self.change_settings(
                |settings| {
                    let mut defaults = settings.server_defaults();
                    change(&mut defaults);
                    settings.set_server_defaults(defaults);
                },
                cx,
            );
            return;
        }
        let Some(mut defaults) = self.environment.defaults.clone() else {
            return;
        };
        change(&mut defaults);
        if self.send_defaults(Some(defaults.clone()), cx) {
            self.environment.defaults = Some(defaults);
        }
        cx.notify();
    }

    /// What Settings says in place of the machine's defaults until it has
    /// said them.
    pub(super) fn defaults_unread(&self, cx: &gpui::App) -> SharedString {
        if self.settings_machine(cx).is_none() {
            return "Enable or add an environment in Connections".into();
        }
        match &self.environment.defaults_error {
            Some(DefaultsTrouble::Unread(error)) => format!("Could not read them: {error}").into(),
            _ => format!("Reading from {}…", self.machine_place(cx)).into(),
        }
    }

    /// The machine's refusal of an edit, said beside the values it kept.
    pub(super) fn defaults_unsaved(&self, cx: &gpui::App) -> Option<SharedString> {
        let Some(DefaultsTrouble::Unsaved(error)) = &self.environment.defaults_error else {
            return None;
        };
        Some(format!("Not saved on {}: {error}", self.machine_place(cx)).into())
    }

    /// The CLIs of the machine Settings is about, asked `action`.
    fn ask_clis(&mut self, action: CliAction, cx: &mut Context<Self>) -> std::io::Result<Answer<Done>> {
        let machine = self
            .settings_machine(cx)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotConnected, "No environment selected"))?;
        self.cockpit.update(cx, |cockpit, _| cockpit.cli(machine, action))
    }

    /// Ask the machine's CLIs their versions.
    pub(super) fn probe_cli_versions(&mut self, cx: &mut Context<Self>) {
        match self.ask_clis(CliAction::Versions, cx) {
            Ok(answer) => self.environment.versions = Some(answer),
            Err(error) => {
                ferrite_core::log!("could not ask the CLIs' versions: {error}");
                self.cli_versions = None;
            }
        }
        cx.notify();
    }

    /// Ask the machine to check its CLIs against the registry.
    pub(super) fn check_clis(&mut self, cx: &mut Context<Self>) {
        match self.ask_clis(CliAction::Check, cx) {
            Ok(answer) => self.environment.check = Some(answer),
            Err(error) => {
                let auto = self.prefs.settings.auto_update_clis;
                for provider in Provider::ALL {
                    let error = std::io::Error::other(error.to_string());
                    self.cli_updates.checked(provider, Err(error), auto);
                }
            }
        }
        cx.notify();
    }

    /// Have the machine install `provider`'s newest CLI.
    pub(super) fn install_cli(&mut self, provider: Provider, cx: &mut Context<Self>) {
        match self.ask_clis(CliAction::Update { provider }, cx) {
            Ok(answer) => self.environment.updates.push((provider, answer)),
            Err(error) => self.cli_updates.updated(provider, Err(error.to_string())),
        }
        cx.notify();
    }

    /// Keeps why a read or a write of the defaults failed. A failed reread
    /// behind rows already shown is only logged: the rows still stand.
    fn defaults_failed(&mut self, write: bool, error: SharedString) {
        self.environment.defaults_error = if write {
            Some(DefaultsTrouble::Unsaved(error))
        } else if self.environment.defaults.is_none() {
            Some(DefaultsTrouble::Unread(error))
        } else {
            ferrite_core::log!("could not reread the machine's defaults: {error}");
            return;
        };
    }

    /// The defaults replies that landed, in the order they were asked. A
    /// reply keeps the edits still in flight, so only the last one replaces
    /// what Settings shows; a write that failed is read back, so the rows
    /// show what the machine kept.
    fn poll_defaults(&mut self, cx: &mut gpui::App) -> bool {
        let mut landed = Vec::new();
        let mut waiting = Vec::new();
        for ask in std::mem::take(&mut self.environment.defaults_asked) {
            if !waiting.is_empty() {
                waiting.push(ask);
                continue;
            }
            match try_answer(&ask.answer) {
                None => waiting.push(ask),
                Some(answer) => landed.push((ask.write, answer)),
            }
        }
        self.environment.defaults_asked = waiting;
        let heard = !landed.is_empty();
        let mut unsaved = false;
        for (write, answer) in landed {
            match answer {
                Ok(defaults) => {
                    if write
                        || matches!(
                            self.environment.defaults_error,
                            Some(DefaultsTrouble::Unread(_))
                        )
                    {
                        self.environment.defaults_error = None;
                    }
                    if self.environment.defaults_asked.is_empty() {
                        self.environment.defaults = Some(defaults);
                    }
                }
                Err(error) => {
                    unsaved |= write;
                    self.defaults_failed(write, self.said(&error, cx));
                }
            }
        }
        if unsaved && self.environment.defaults_asked.is_empty() {
            self.send_defaults(None, cx);
        }
        heard
    }

    /// What the machine answered since the pump last looked.
    pub(super) fn poll_environment(&mut self, cx: &mut Context<Self>) {
        let mut heard = false;
        heard |= self.poll_defaults(cx);
        if let Some(answer) = take_answer(&mut self.environment.versions) {
            heard = true;
            self.cli_versions = Some(match answer {
                Ok(Done::Clis { clis }) => {
                    let line = |provider| {
                        cli_line(
                            clis.iter()
                                .find(|cli| cli.provider == provider)
                                .and_then(|cli| cli.binary.as_ref()),
                        )
                    };
                    (line(Provider::Claude), line(Provider::Codex))
                }
                Err(error) => {
                    let said = self.said(&error, cx);
                    (said.clone(), said)
                }
                _ => ("unavailable".into(), "unavailable".into()),
            });
        }
        if let Some(answer) = take_answer(&mut self.environment.check) {
            heard = true;
            let auto = self.prefs.settings.auto_update_clis;
            let mut reports = match answer {
                Ok(Done::Clis { clis }) => clis,
                _ => Vec::new(),
            };
            for provider in Provider::ALL {
                let status = reports
                    .iter_mut()
                    .find(|cli| cli.provider == provider)
                    .and_then(|cli| cli.status.take());
                let found = match status {
                    None => Err("the machine did not answer"),
                    Some(status) => status.known().ok_or(
                        "the machine's ferrited reported a finding this Ferrite does not know",
                    ),
                };
                self.cli_updates
                    .checked(provider, found.map_err(std::io::Error::other), auto);
            }
            self.auto_update_clis(cx);
        }
        let mut landed = Vec::new();
        self.environment
            .updates
            .retain_mut(|(provider, answer)| match answer.try_recv() {
                Err(TryRecvError::Empty) => true,
                Ok(done) => {
                    landed.push((*provider, done));
                    false
                }
                Err(error) => {
                    landed.push((*provider, Err(std::io::Error::other(error))));
                    false
                }
            });
        for (provider, answer) in landed {
            heard = true;
            match answer {
                Ok(Done::CliUpdated { binary, .. }) => {
                    // Open Threads restart on the new CLI as each goes idle,
                    // and the pickers ask for its menu now.
                    if let Some(machine) = self.settings_machine(cx) {
                        self.cockpit
                            .update(cx, |cockpit, _| cockpit.cli_replaced(machine, provider));
                    }
                    self.cli_updates.updated(provider, Ok(binary.version));
                    self.probe_cli_versions(cx);
                }
                Ok(_) => self
                    .cli_updates
                    .updated(provider, Err("the machine answered something else".into())),
                Err(error) => self.cli_updates.updated(provider, Err(error.to_string())),
            }
        }
        if heard {
            cx.notify();
        }
    }
}

/// A reply that arrived, taking its channel; a hung-up one reads as an
/// error.
fn take_answer<T>(slot: &mut Option<Answer<T>>) -> Option<std::io::Result<T>> {
    let answer = try_answer(slot.as_ref()?)?;
    *slot = None;
    Some(answer)
}

fn try_answer<T>(answer: &Answer<T>) -> Option<std::io::Result<T>> {
    match answer.try_recv() {
        Ok(answer) => Some(answer),
        Err(TryRecvError::Empty) => None,
        Err(error) => Some(Err(std::io::Error::other(error))),
    }
}
