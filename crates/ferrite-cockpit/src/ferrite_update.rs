//! Ferrite kept current from inside the app (ADR 0023).
//!
//! The window looks for a newer release now and then (or when Settings
//! asks), downloads and verifies it, and then only offers: a restart now
//! if no agent is working, else one once they all are idle. Whatever the
//! operator chooses, the next launch installs it (`at_launch`) and the new
//! Ferrite opens on its release notes. The fetching, checking and
//! installing are `ferrite_core::self_update`; this is the state the
//! window is in and the words it says about it.

use std::path::Path;
use std::time::Duration;

use ferrite_core::self_update::{self, AtLaunch, Release, Staged};
use gpui::SharedString;

/// How long after launch the first check waits: startup is busy enough.
pub const FIRST_CHECK_AFTER: Duration = Duration::from_secs(30);
/// How often GitHub is asked. Releases come days apart.
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// How often a restart waiting for idle agents looks again.
pub const IDLE_POLL: Duration = Duration::from_secs(2);

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Whether this copy updates itself at all: a release build, on a
/// platform and in a place an installer can replace. A development build
/// never does, so a `cargo run` cannot take the installed app's update.
pub fn enabled() -> bool {
    !crate::titlebar::DEV && self_update::supported()
}

/// Whether this window is the one Ferrite that updates itself: it holds
/// the update claim from here until it quits.
pub fn claim(dir: &Path) -> bool {
    self_update::claim(dir).unwrap_or_else(|error| {
        ferrite_core::log!("the update claim could not be taken: {error}");
        false
    })
}

/// Act on the release the last run staged, before any Session starts. A
/// successful install does not return: the new Ferrite takes this
/// process's place. Answers the notes to show when this launch is the
/// release that was staged.
pub fn at_launch(dir: &Path) -> Option<Staged> {
    match self_update::at_launch(dir, VERSION) {
        AtLaunch::Nothing => None,
        AtLaunch::Installed(staged) => Some(staged),
        AtLaunch::Install(staged) => {
            ferrite_core::log!("installing Ferrite {}", staged.version);
            let Err(error) = self_update::install(dir, &staged);
            ferrite_core::log!("Ferrite {} did not install: {error}", staged.version);
            hand_over(dir, &staged);
            None
        }
        AtLaunch::Failed(staged) => {
            ferrite_core::log!("Ferrite {} did not install last launch", staged.version);
            hand_over(dir, &staged);
            None
        }
    }
}

fn hand_over(dir: &Path, staged: &Staged) {
    if let Err(error) = self_update::hand_over(dir, staged) {
        ferrite_core::log!(
            "could not open the Ferrite {} installer: {error}",
            staged.version
        );
    }
}

/// Where Ferrite's own update stands this run.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Unchecked,
    Checking,
    UpToDate,
    Downloading {
        version: String,
    },
    /// Downloaded and verified; installs at the next launch.
    Ready {
        version: String,
        /// The operator asked to restart once no agent is working.
        when_idle: bool,
    },
    /// Ferrite could not install it and gave the operator its installer.
    HandedOver {
        version: String,
    },
    /// The words say whether the check or the download failed.
    Failed {
        detail: String,
    },
}

#[derive(Default)]
pub struct FerriteUpdate {
    state: State,
    /// A release just made ready, to say once as a toast.
    offer: Option<String>,
}

impl FerriteUpdate {
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Whether a check may start now, starting it if so. Not while one is
    /// out or a download runs, and not once a release is ready: it is the
    /// one the next launch installs.
    pub fn begin_check(&mut self) -> bool {
        if matches!(
            self.state,
            State::Checking | State::Downloading { .. } | State::Ready { .. }
        ) {
            return false;
        }
        self.state = State::Checking;
        true
    }

    /// A check's answer: the release to download, if there is one.
    pub fn checked(&mut self, result: std::io::Result<Option<Release>>) -> Option<Release> {
        match result {
            Ok(None) => {
                self.state = State::UpToDate;
                None
            }
            Ok(Some(release)) => {
                self.state = State::Downloading {
                    version: release.version.clone(),
                };
                Some(release)
            }
            Err(error) => {
                ferrite_core::log!("could not check for a newer Ferrite: {error}");
                self.state = State::Failed {
                    detail: format!("Could not check for a newer Ferrite: {error}"),
                };
                None
            }
        }
    }

    pub fn downloaded(&mut self, result: std::io::Result<Staged>) {
        self.state = match result {
            Ok(staged) if staged.handed_over() => State::HandedOver {
                version: staged.version,
            },
            Ok(staged) => {
                self.offer = Some(staged.version.clone());
                State::Ready {
                    version: staged.version,
                    when_idle: false,
                }
            }
            Err(error) => {
                ferrite_core::log!("could not download the newer Ferrite: {error}");
                State::Failed {
                    detail: format!("The download failed: {error}"),
                }
            }
        };
    }

    /// Restart once no agent is working. `true` when this newly asked it,
    /// so the caller starts watching; `false` when nothing is ready or the
    /// watch is already on.
    pub fn restart_when_idle(&mut self) -> bool {
        match &mut self.state {
            State::Ready { when_idle, .. } if !*when_idle => {
                *when_idle = true;
                true
            }
            _ => false,
        }
    }

    /// Whether a restart onto a ready release is still to offer: not
    /// once one waits for idle agents.
    pub fn restart_offered(&self) -> bool {
        matches!(
            self.state,
            State::Ready {
                when_idle: false,
                ..
            }
        )
    }

    /// The handed-over installer would not open again.
    pub fn reopen_failed(&mut self, error: std::io::Error) {
        ferrite_core::log!("could not open the Ferrite installer again: {error}");
        self.state = State::Failed {
            detail: format!("Could not open the installer: {error}"),
        };
    }

    pub fn take_offer(&mut self) -> Option<String> {
        self.offer.take()
    }

    /// Settings' line for Ferrite's own update, and its button's label
    /// when there is something to press. `busy` is what restarting now
    /// would interrupt (`CockpitView::in_flight`).
    pub fn describe(&self, busy: Option<&str>) -> (SharedString, Option<SharedString>) {
        let (text, button) = match &self.state {
            State::Unchecked => ("Not checked yet".to_string(), Some("Check now")),
            State::Checking => ("Checking for a newer Ferrite…".to_string(), None),
            State::UpToDate => (
                format!("{VERSION} is the latest release"),
                Some("Check now"),
            ),
            State::Downloading { version } => (format!("Downloading {version}…"), None),
            State::Ready {
                version,
                when_idle: false,
            } => (
                format!(
                    "{version} is downloaded and verified. It installs when Ferrite next opens"
                ),
                Some("Restart to update"),
            ),
            State::Ready {
                version,
                when_idle: true,
            } => (
                match busy {
                    Some(busy) => format!(
                        "{version} installs once no agent is working or waiting ({busy} now)"
                    ),
                    None => format!("{version} installs now"),
                },
                Some("Restart now"),
            ),
            State::HandedOver { version } => (
                format!("{version} could not install itself. Run its installer to finish"),
                Some("Open installer"),
            ),
            State::Failed { detail } => (detail.clone(), Some("Try again")),
        };
        (text.into(), button.map(SharedString::from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: &str) -> Release {
        Release {
            version: version.into(),
            notes: String::new(),
            asset: self_update::Asset {
                name: "ferrite.dmg".into(),
                url: "https://example.invalid/ferrite.dmg".into(),
                sha256: "0".repeat(64),
            },
        }
    }

    fn ready(version: &str) -> FerriteUpdate {
        let mut update = FerriteUpdate::default();
        assert!(update.begin_check());
        let found = update.checked(Ok(Some(release(version))));
        assert_eq!(found, Some(release(version)));
        let staged: Staged = serde_json::from_value(serde_json::json!({
            "version": version,
            "notes": "",
            "installer": "ferrite.dmg",
        }))
        .unwrap();
        update.downloaded(Ok(staged));
        update
    }

    #[test]
    fn a_found_release_downloads_then_is_offered_once() {
        let mut update = ready("0.5.0");
        assert!(update.restart_offered());
        assert_eq!(update.take_offer().as_deref(), Some("0.5.0"));
        assert_eq!(update.take_offer(), None);
    }

    /// Once a release is ready nothing else is checked for: it is the
    /// one the next launch installs.
    #[test]
    fn checks_wait_while_one_is_out_or_a_release_is_ready() {
        let mut update = FerriteUpdate::default();
        assert!(update.begin_check());
        assert!(!update.begin_check(), "a check is already out");
        update.checked(Ok(None));
        assert_eq!(update.state(), &State::UpToDate);
        assert!(update.begin_check());
        assert!(!ready("0.5.0").begin_check());
    }

    #[test]
    fn a_failed_check_can_be_tried_again() {
        let mut update = FerriteUpdate::default();
        update.begin_check();
        update.checked(Err(std::io::Error::other("offline")));
        let (text, button) = update.describe(None);
        assert_eq!(
            text.as_ref(),
            "Could not check for a newer Ferrite: offline"
        );
        assert_eq!(button.as_deref(), Some("Try again"));
        assert!(update.begin_check());
    }

    /// Restarting is only ever offered: with agents working it waits for
    /// them, and says who it waits for.
    #[test]
    fn a_restart_can_wait_for_idle_agents() {
        let mut update = ready("0.5.0");
        assert_eq!(
            update.describe(None).1.as_deref(),
            Some("Restart to update")
        );
        assert!(update.restart_when_idle());
        assert!(!update.restart_when_idle(), "already waiting");
        assert!(!update.restart_offered(), "the palette offers it no more");
        assert_eq!(
            update.state(),
            &State::Ready {
                version: "0.5.0".into(),
                when_idle: true
            }
        );
        let (text, button) = update.describe(Some("1 agent is working"));
        assert_eq!(
            text.as_ref(),
            "0.5.0 installs once no agent is working or waiting (1 agent is working now)"
        );
        assert_eq!(button.as_deref(), Some("Restart now"));
    }

    /// A release Ferrite already handed over is neither offered nor
    /// waited on again; checks go on, for the release after it.
    #[test]
    fn a_handed_over_release_is_not_offered_again() {
        let mut update = FerriteUpdate::default();
        update.begin_check();
        update.checked(Ok(Some(release("0.5.0"))));
        let staged: Staged = serde_json::from_value(serde_json::json!({
            "version": "0.5.0",
            "notes": "",
            "installer": "ferrite.dmg",
            "progress": "handed_over",
        }))
        .unwrap();
        update.downloaded(Ok(staged));
        assert_eq!(
            update.state(),
            &State::HandedOver {
                version: "0.5.0".into()
            }
        );
        assert_eq!(update.take_offer(), None);
        assert!(!update.restart_when_idle());
        assert_eq!(update.describe(None).1.as_deref(), Some("Open installer"));
        assert!(update.begin_check());
    }

    #[test]
    fn nothing_waits_for_idle_without_a_release() {
        let mut update = FerriteUpdate::default();
        assert!(!update.restart_when_idle());
        assert_eq!(update.state(), &State::Unchecked);
    }
}
