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
    pub(super) defaults: Option<ServerDefaults>,
    pub(super) defaults_error: Option<SharedString>,
    pub(super) defaults_asked: Option<Answer<ServerDefaults>>,
    versions: Option<Answer<Done>>,
    check: Option<Answer<Done>>,
    updates: Vec<(Provider, Answer<Done>)>,
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

    /// Read the machine's defaults, unless a read is out.
    pub(super) fn ask_server_defaults(&mut self) {
        if self.on_machine() && self.environment.defaults_asked.is_none() {
            self.send_defaults(None);
        }
    }

    fn send_defaults(&mut self, set: Option<ServerDefaults>) {
        match self.cockpit.server_defaults(set) {
            Ok(answer) => {
                self.environment.defaults_asked = Some(answer);
                self.environment.defaults_error = None;
            }
            Err(error) => self.environment.defaults_error = Some(error.to_string().into()),
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

    /// Change one default where it lives. On a machine the Server writes it
    /// and rereads its file, and Settings shows that reread.
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
        self.send_defaults(Some(defaults));
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
            settings::Group::new(None).rows([applies]),
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
        group.rows([
            applies,
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
                        let _ = view.update(cx, |view, cx| view.step_factory_limit(delta, cx));
                    }
                },
            ),
        ])
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

    /// What the machine answered since the pump last looked.
    pub(super) fn poll_environment(&mut self, cx: &mut Context<Self>) {
        let mut heard = false;
        if let Some(answer) = take_answer(&mut self.environment.defaults_asked) {
            heard = true;
            match answer {
                Ok(defaults) => {
                    self.environment.defaults = Some(defaults);
                    self.environment.defaults_error = None;
                }
                Err(error) => self.environment.defaults_error = Some(error.to_string().into()),
            }
        }
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
                self.cli_updates.checked(
                    provider,
                    status.ok_or_else(|| std::io::Error::other("the machine did not answer")),
                    auto,
                );
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
    let answer = match slot.as_ref()?.try_recv() {
        Ok(answer) => answer,
        Err(TryRecvError::Empty) => return None,
        Err(error) => Err(std::io::Error::other(error)),
    };
    *slot = None;
    Some(answer)
}
