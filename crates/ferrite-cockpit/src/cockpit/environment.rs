//! Settings against the machine each value belongs to. The window's own
//! preferences (text size, sounds, the sidebar) live in this computer's
//! `settings.json`. A Server's defaults — how its Sessions ask for
//! permission, its Factory's limit, its follow-up suggestions — and its
//! provider CLIs belong to the machine the Server runs on, so a window on a
//! machine reads and writes those through the Server (`Op::ServerDefaults`,
//! `Op::Cli`) and shows the file's own answer, never this computer's.

use super::*;
use ferrite_core::protocol::{CliAction, CliBinary, Done};
use ferrite_core::settings::ServerDefaults;
use std::sync::mpsc::{Receiver, TryRecvError};

type Answer<T> = Receiver<std::io::Result<T>>;

/// What the window has asked of its machine and not yet heard, and the
/// defaults it last heard.
#[derive(Default)]
pub(crate) struct Environment {
    /// The machine's defaults as its own `settings.json` last said, kept
    /// only for a window on a machine: this computer's are the window's.
    /// Edits show at once; the Server's reread replaces them when the last
    /// write lands.
    pub(super) defaults: Option<ServerDefaults>,
    /// Why the machine's defaults could not be read, or an edit written.
    pub(super) defaults_error: Option<SharedString>,
    defaults_asked: Vec<DefaultsAsk>,
    versions: Option<Answer<Done>>,
    check: Option<Answer<Done>>,
    updates: Vec<(Provider, Answer<Done>)>,
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
    /// Where the window's Sessions run, as Settings says it.
    pub(super) fn machine_place(&self) -> SharedString {
        match self.cockpit.machine() {
            Some(machine) => machine.name().to_string().into(),
            None => "this computer".into(),
        }
    }

    /// What a machine's failure reads as: a Server from before the command
    /// is told to update, not shown its refusal.
    fn said(&self, error: &std::io::Error) -> SharedString {
        match error.kind() {
            std::io::ErrorKind::Unsupported => {
                format!("update ferrited on {}", self.machine_place()).into()
            }
            _ => error.to_string().into(),
        }
    }

    /// Read the machine's defaults, unless a read is out.
    pub(super) fn ask_server_defaults(&mut self) {
        if self.on_machine() && self.environment.defaults_asked.is_empty() {
            self.send_defaults(None);
        }
    }

    /// Send a read, or with `set` a write, of the machine's defaults.
    /// Whether it went out.
    fn send_defaults(&mut self, set: Option<ServerDefaults>) -> bool {
        let write = set.is_some();
        match self.cockpit.server_defaults(set) {
            Ok(answer) => {
                self.environment
                    .defaults_asked
                    .push(DefaultsAsk { answer, write });
                true
            }
            Err(error) => {
                self.environment.defaults_error = Some(self.said(&error));
                false
            }
        }
    }

    /// The defaults Settings shows: this computer's from its own file, or
    /// the machine's once it has said them.
    pub(super) fn shown_defaults(&self) -> Option<ServerDefaults> {
        if self.on_machine() {
            self.environment.defaults.clone()
        } else {
            Some(self.prefs.settings.server_defaults())
        }
    }

    /// How many Factory Tasks the machine works at once, when known.
    pub(super) fn factory_limit(&self) -> Option<usize> {
        self.shown_defaults()
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
        if !self.on_machine() {
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
        if self.send_defaults(Some(defaults.clone())) {
            self.environment.defaults = Some(defaults);
        }
        cx.notify();
    }

    fn default_change<T: 'static>(
        &self,
        cx: &Context<Self>,
        write: impl Fn(&mut ServerDefaults, T) + 'static,
    ) -> impl Fn(T, &mut gpui::App) + 'static {
        let view = cx.entity().downgrade();
        move |value, cx| {
            let _ = view.update(cx, |view, cx| {
                view.change_defaults(|defaults| write(defaults, value), cx);
            });
        }
    }

    /// What stands in for the machine's rows until it has said them.
    fn defaults_unread(&self) -> settings::Row {
        let status: SharedString = match &self.environment.defaults_error {
            Some(error) => format!("Could not read them: {error}").into(),
            None => format!("Reading from {}…", self.machine_place()).into(),
        };
        settings::fact("Defaults", status)
    }

    /// The machine's refusal of an edit, shown beside the values it kept.
    fn defaults_unsaved(&self) -> Option<settings::Row> {
        let error = self.environment.defaults_error.as_ref()?;
        Some(settings::fact(
            "Defaults",
            format!("Not saved on {}: {error}", self.machine_place()).into(),
        ))
    }

    /// Settings › Permissions: how each provider's Sessions ask before
    /// acting, as the machine they run on says.
    pub(super) fn permission_groups(&self, cx: &Context<Self>) -> Vec<settings::Group> {
        let applies = settings::fact("Applies to", self.machine_place());
        let Some(defaults) = self.shown_defaults() else {
            return vec![settings::Group::new(None).rows([applies, self.defaults_unread()])];
        };
        let modes = |options: &[(&str, Option<&str>)], selected: Option<&str>| {
            options
                .iter()
                .map(|(label, value)| {
                    (
                        SharedString::from(label.to_string()),
                        selected == *value,
                        value.map(str::to_string),
                    )
                })
                .collect()
        };
        let claude = vec![settings::choices(
            "settings-claude-mode",
            "Permissions",
            "When Claude asks before acting",
            modes(
                &[
                    (settings::CLI_DEFAULT, None),
                    ("Ask", Some("default")),
                    ("Accept edits", Some("acceptEdits")),
                    ("Plan", Some("plan")),
                    ("Bypass permissions", Some("bypassPermissions")),
                ],
                defaults.claude_permission_mode.as_deref(),
            ),
            self.default_change(cx, |d, v| d.claude_permission_mode = v),
        )];
        let codex = vec![
            settings::choices(
                "settings-codex-approval",
                "Approvals",
                "When Codex asks before acting",
                modes(
                    &[
                        ("On request", Some("on-request")),
                        ("Untrusted", Some("untrusted")),
                        ("Never", Some("never")),
                    ],
                    Some(&defaults.codex_approval_policy),
                ),
                self.default_change(cx, |d, v: Option<String>| {
                    d.codex_approval_policy = v.unwrap_or_else(|| "on-request".into())
                }),
            ),
            settings::choices(
                "settings-codex-sandbox",
                "Sandbox",
                "What Codex may touch",
                modes(
                    &[
                        (settings::CLI_DEFAULT, None),
                        ("Read only", Some("read-only")),
                        ("Workspace write", Some("workspace-write")),
                        ("Full access", Some("danger-full-access")),
                    ],
                    defaults.codex_sandbox.as_deref(),
                ),
                self.default_change(cx, |d, v| d.codex_sandbox = v),
            ),
        ];
        vec![
            settings::Group::new(None)
                .rows(std::iter::once(applies).chain(self.defaults_unsaved())),
            provider_group(Provider::Claude).rows(claude),
            provider_group(Provider::Codex).rows(codex),
        ]
    }

    /// Settings › Behaviour's group of what the machine does on its own
    /// account: follow-up suggestions and its Factory's limit.
    pub(super) fn machine_group(&self, cx: &Context<Self>) -> settings::Group {
        let applies = settings::fact("Applies to", self.machine_place());
        let group = settings::Group::new(Some("Machine defaults"));
        let Some(defaults) = self.shown_defaults() else {
            return group.rows([applies, self.defaults_unread()]);
        };
        let at_once = defaults.factory_tasks_at_once;
        group.rows(
            std::iter::once(applies)
                .chain(self.defaults_unsaved())
                .chain([
                    settings::toggle(
                        "settings-placeholder-suggestions",
                        "Suggest follow-up prompts",
                        "A likely next prompt in an empty composer · tab takes it",
                        defaults.placeholder_suggestions,
                        self.default_change(cx, |d, v| d.placeholder_suggestions = v),
                    ),
                    settings::stepper(
                        "settings-factory-limit",
                        "Tasks working at once",
                        "In the Factory · the rest wait in Queued",
                        at_once.to_string().into(),
                        at_once > *ferrite_core::factory::LIMIT_RANGE.start(),
                        at_once < *ferrite_core::factory::LIMIT_RANGE.end(),
                        {
                            let view = cx.entity().downgrade();
                            move |delta, cx| {
                                let _ =
                                    view.update(cx, |view, cx| view.step_factory_limit(delta, cx));
                            }
                        },
                    ),
                ]),
        )
    }

    /// Ask the machine's CLIs their versions.
    pub(super) fn probe_cli_versions(&mut self, cx: &mut Context<Self>) {
        match self.cockpit.cli(CliAction::Versions) {
            Ok(answer) => self.environment.versions = Some(answer),
            Err(error) => {
                ferrite_core::log!("could not ask the CLIs' versions: {error}");
                self.cli_versions = Some(("unavailable".into(), "unavailable".into()));
            }
        }
        cx.notify();
    }

    /// Ask the machine to check its CLIs against the registry.
    pub(super) fn check_clis(&mut self, cx: &mut Context<Self>) {
        match self.cockpit.cli(CliAction::Check) {
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
        match self.cockpit.cli(CliAction::Update { provider }) {
            Ok(answer) => self.environment.updates.push((provider, answer)),
            Err(error) => self.cli_updates.updated(provider, Err(error.to_string())),
        }
        cx.notify();
    }

    /// The defaults replies that landed, in the order they were asked. A
    /// reply keeps the edits still in flight, so only the last one replaces
    /// what Settings shows; a write that failed is read back, so the rows
    /// show what the machine kept.
    fn poll_defaults(&mut self) -> bool {
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
                    if write {
                        self.environment.defaults_error = None;
                    }
                    if self.environment.defaults_asked.is_empty() {
                        self.environment.defaults = Some(defaults);
                    }
                }
                Err(error) if write => {
                    self.environment.defaults_error = Some(self.said(&error));
                    unsaved = true;
                }
                Err(error) if self.environment.defaults.is_none() => {
                    self.environment.defaults_error = Some(self.said(&error));
                }
                Err(error) => {
                    ferrite_core::log!("could not reread the machine's defaults: {error}")
                }
            }
        }
        if unsaved && self.environment.defaults_asked.is_empty() {
            self.send_defaults(None);
        }
        heard
    }

    /// What the machine answered since the pump last looked.
    pub(super) fn poll_environment(&mut self, cx: &mut Context<Self>) {
        let mut heard = false;
        heard |= self.poll_defaults();
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
                    let said = self.said(&error);
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
                    self.cockpit.cli_replaced(provider);
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
