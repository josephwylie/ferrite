//! The provider CLIs kept current from inside the app.
//!
//! A new model reaches Ferrite only through a newer CLI, so the window
//! checks the registry now and then, installs a newer release itself once
//! no Session of that provider is running (or, with automatic updates off,
//! offers it in a toast and in Settings), and asks the provider menus again
//! when an upgrade lands. The checking and installing are
//! `ferrite_core::providers::update`; this is only the state each provider
//! is in and the words the window says about it.

use std::time::{Duration, Instant};

use ferrite_core::providers::update::{Status, Upgrade};
use ferrite_core::store::Provider;
use gpui::SharedString;

/// How long after launch the first check waits: startup is busy enough.
pub const FIRST_CHECK_AFTER: Duration = Duration::from_secs(20);
/// How often a pending automatic upgrade looks again for idle Sessions.
pub const TICK: Duration = Duration::from_secs(5 * 60);
/// How often the registry is asked. Releases come days apart.
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

pub fn name(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "Claude CLI",
        Provider::Codex => "Codex CLI",
    }
}

/// Where one provider's CLI stands.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Unchecked,
    Checking,
    Current(Status),
    Updating {
        latest: String,
    },
    Updated {
        version: String,
    },
    Failed {
        detail: String,
    },
}

/// Something the window should say once, as a toast.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Toast {
    /// A release to install; the toast's click installs it.
    Offer {
        provider: Provider,
        latest: String,
    },
    Updated {
        provider: Provider,
        version: String,
    },
    Failed {
        provider: Provider,
        detail: String,
    },
}

/// The top bar's update button, with its tooltip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Badge {
    /// Pressing it installs every ready release.
    Ready(String),
    /// Everything pressable is already installing; the button waits.
    Installing(String),
}

#[derive(Default)]
pub struct CliUpdates {
    claude: State,
    codex: State,
    checked: Option<Instant>,
    /// Releases already offered, so each is offered once per launch.
    /// (`Provider` is not hashable, and the list stays tiny.)
    offered: Vec<(Provider, String)>,
    toasts: Vec<Toast>,
}

impl CliUpdates {
    pub fn state(&self, provider: Provider) -> &State {
        match provider {
            Provider::Claude => &self.claude,
            Provider::Codex => &self.codex,
        }
    }

    fn state_mut(&mut self, provider: Provider) -> &mut State {
        match provider {
            Provider::Claude => &mut self.claude,
            Provider::Codex => &mut self.codex,
        }
    }

    /// Whether the registry is due to be asked again. Starts the check
    /// when it is: both providers read as checking until it answers.
    pub fn begin_check(&mut self, now: Instant) -> bool {
        let busy = [Provider::Claude, Provider::Codex]
            .into_iter()
            .any(|provider| {
                matches!(
                    self.state(provider),
                    State::Checking | State::Updating { .. }
                )
            });
        let due = self
            .checked
            .is_none_or(|checked| now.duration_since(checked) >= CHECK_EVERY);
        if busy || !due {
            return false;
        }
        self.checked = Some(now);
        self.claude = State::Checking;
        self.codex = State::Checking;
        true
    }

    /// A check's answer. A failed check (offline, registry down) keeps
    /// quiet and reads as unchecked; the next interval tries again.
    pub fn checked(&mut self, provider: Provider, result: std::io::Result<Status>, auto: bool) {
        match result {
            Ok(status) => {
                if let Status::Available { latest, .. } = &status {
                    let release = (provider, latest.clone());
                    let fresh = !self.offered.contains(&release);
                    if fresh {
                        self.offered.push(release);
                    }
                    if fresh && !auto {
                        self.toasts.push(Toast::Offer {
                            provider,
                            latest: latest.clone(),
                        });
                    }
                }
                *self.state_mut(provider) = State::Current(status);
            }
            Err(error) => {
                eprintln!(
                    "ferrite: could not check for a newer {}: {error}",
                    name(provider)
                );
                *self.state_mut(provider) = State::Unchecked;
            }
        }
    }

    /// The upgrade to run for `provider` now, if one is ready, marking it
    /// as running. `None` when there is nothing to install or Ferrite does
    /// not know how.
    pub fn begin_update(&mut self, provider: Provider) -> Option<Upgrade> {
        let State::Current(Status::Available {
            latest,
            upgrade: Some(upgrade),
            ..
        }) = self.state(provider)
        else {
            return None;
        };
        let upgrade = upgrade.clone();
        *self.state_mut(provider) = State::Updating {
            latest: latest.clone(),
        };
        Some(upgrade)
    }

    /// Providers with an upgrade ready that automatic updating may run.
    pub fn ready(&self) -> Vec<Provider> {
        [Provider::Claude, Provider::Codex]
            .into_iter()
            .filter(|provider| {
                matches!(
                    self.state(*provider),
                    State::Current(Status::Available {
                        upgrade: Some(_),
                        ..
                    })
                )
            })
            .collect()
    }

    /// What the top bar's update button stands for: the releases it would
    /// install, or those installing now. `None` hides the button — there
    /// is nothing an operator could press it for.
    pub fn badge(&self) -> Option<Badge> {
        let mut ready = Vec::new();
        let mut installing = Vec::new();
        for provider in [Provider::Claude, Provider::Codex] {
            match self.state(provider) {
                State::Current(Status::Available {
                    latest,
                    upgrade: Some(_),
                    ..
                }) => ready.push(format!("{} {latest}", name(provider))),
                State::Updating { latest } => {
                    installing.push(format!("{} {latest}", name(provider)))
                }
                _ => {}
            }
        }
        if !ready.is_empty() {
            Some(Badge::Ready(format!("Update {}", ready.join(" and "))))
        } else if !installing.is_empty() {
            Some(Badge::Installing(format!(
                "Installing {}…",
                installing.join(" and ")
            )))
        } else {
            None
        }
    }

    pub fn updated(&mut self, provider: Provider, result: Result<String, String>) {
        let (state, toast) = match result {
            Ok(version) => (
                State::Updated {
                    version: version.clone(),
                },
                Toast::Updated { provider, version },
            ),
            Err(detail) => (
                State::Failed {
                    detail: detail.clone(),
                },
                Toast::Failed { provider, detail },
            ),
        };
        *self.state_mut(provider) = state;
        self.toasts.push(toast);
    }

    pub fn take_toasts(&mut self) -> Vec<Toast> {
        std::mem::take(&mut self.toasts)
    }
}

/// Settings' line under a provider's version, and its button's label when
/// there is something to press. `live` is how many of that provider's
/// Sessions are running.
pub fn describe(
    provider: Provider,
    state: &State,
    auto: bool,
    live: usize,
) -> (SharedString, Option<SharedString>) {
    let cli = name(provider);
    let (text, button) = match state {
        State::Unchecked => ("Not checked for updates yet".to_string(), None),
        State::Checking => ("Checking for updates…".to_string(), None),
        State::Current(Status::Missing) => (format!("{cli} is not installed"), None),
        State::Current(Status::UpToDate { current }) => {
            (format!("{current} is the latest release"), None)
        }
        State::Current(Status::Available {
            latest,
            upgrade: None,
            ..
        }) => (
            format!(
                "{latest} is available. Ferrite can't tell how this copy was installed, \
                 so update it the way you installed it"
            ),
            None,
        ),
        State::Current(Status::Available {
            latest,
            upgrade: Some(upgrade),
            ..
        }) => {
            let when = if auto && live > 0 {
                format!(
                    " Installs by itself once no {} Thread is running ({live} open now).",
                    provider_word(provider)
                )
            } else {
                String::new()
            };
            (
                format!("{latest} is available via `{}`.{when}", upgrade.display()),
                Some(format!("Update to {latest}")),
            )
        }
        State::Updating { latest } => (format!("Installing {latest}…"), None),
        State::Updated { version } => (
            format!("Updated to {version}. Open Threads restart on it as each finishes its turn"),
            None,
        ),
        State::Failed { detail } => (format!("The update failed: {detail}"), None),
    };
    (text.into(), button.map(SharedString::from))
}

fn provider_word(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "Claude",
        Provider::Codex => "Codex",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn available(latest: &str) -> Status {
        Status::Available {
            current: "2.1.259".into(),
            latest: latest.into(),
            upgrade: Some(Upgrade {
                program: PathBuf::from("claude"),
                args: vec!["update".into()],
                path_prefix: None,
            }),
        }
    }

    #[test]
    fn the_registry_is_asked_once_per_interval() {
        let mut updates = CliUpdates::default();
        let start = Instant::now();
        assert!(updates.begin_check(start));
        assert!(!updates.begin_check(start), "a check is already out");
        updates.checked(Provider::Claude, Ok(Status::Missing), true);
        updates.checked(Provider::Codex, Ok(Status::Missing), true);
        assert!(!updates.begin_check(start + Duration::from_secs(60)));
        assert!(updates.begin_check(start + CHECK_EVERY));
    }

    /// With automatic updates off, each release is offered once, not at
    /// every check.
    #[test]
    fn a_release_is_offered_once() {
        let mut updates = CliUpdates::default();
        updates.checked(Provider::Claude, Ok(available("2.1.270")), false);
        updates.checked(Provider::Claude, Ok(available("2.1.270")), false);
        assert_eq!(
            updates.take_toasts(),
            vec![Toast::Offer {
                provider: Provider::Claude,
                latest: "2.1.270".into(),
            }]
        );
        updates.checked(Provider::Claude, Ok(available("2.1.271")), false);
        assert_eq!(updates.take_toasts().len(), 1);
    }

    #[test]
    fn automatic_updates_install_without_offering() {
        let mut updates = CliUpdates::default();
        updates.checked(Provider::Claude, Ok(available("2.1.270")), true);
        assert!(updates.take_toasts().is_empty());
        assert_eq!(updates.ready(), vec![Provider::Claude]);
        assert!(updates.begin_update(Provider::Claude).is_some());
        assert!(updates.ready().is_empty(), "an upgrade runs once");
        assert!(updates.begin_update(Provider::Claude).is_none());
        updates.updated(Provider::Claude, Ok("2.1.270".into()));
        assert_eq!(
            updates.state(Provider::Claude),
            &State::Updated {
                version: "2.1.270".into()
            }
        );
    }

    /// A failed upgrade is not retried until the next check finds the
    /// release again, so a broken installer does not run every tick.
    #[test]
    fn a_failed_upgrade_waits_for_the_next_check() {
        let mut updates = CliUpdates::default();
        updates.checked(Provider::Codex, Ok(available("0.160.0")), true);
        updates.begin_update(Provider::Codex);
        updates.updated(Provider::Codex, Err("EBUSY".into()));
        assert!(updates.ready().is_empty());
    }

    /// The button shows only while it can do something, or is doing it.
    #[test]
    fn the_badge_follows_what_can_be_installed() {
        let mut updates = CliUpdates::default();
        assert_eq!(updates.badge(), None);
        updates.checked(Provider::Claude, Ok(available("2.1.280")), true);
        assert_eq!(
            updates.badge(),
            Some(Badge::Ready("Update Claude CLI 2.1.280".into()))
        );
        updates.begin_update(Provider::Claude);
        assert_eq!(
            updates.badge(),
            Some(Badge::Installing("Installing Claude CLI 2.1.280…".into()))
        );
        updates.updated(Provider::Claude, Ok("2.1.280".into()));
        assert_eq!(updates.badge(), None);

        let unrunnable = Status::Available {
            current: "0.153.0".into(),
            latest: "0.160.0".into(),
            upgrade: None,
        };
        updates.checked(Provider::Codex, Ok(unrunnable), true);
        assert_eq!(
            updates.badge(),
            None,
            "nothing to press for a manual upgrade"
        );
    }

    #[test]
    fn an_upgrade_ferrite_cannot_run_is_described_not_offered() {
        let status = Status::Available {
            current: "0.153.0".into(),
            latest: "0.160.0".into(),
            upgrade: None,
        };
        let (_, button) = describe(Provider::Codex, &State::Current(status), true, 0);
        assert_eq!(button, None);
    }
}
