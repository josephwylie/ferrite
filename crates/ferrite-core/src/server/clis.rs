//! The provider CLIs of the machine a Server runs on. A window reads and
//! replaces them through its Server, so the copy it names, the update it
//! runs and the Sessions that restart after it are all that one machine's
//! (`Op::Cli`). Probes and installers run off the pump (ADR 0025).
//!
//! [`Clis`] is the seam: the Server's harness hands it a scripted machine.

use crate::protocol::{CliAction, CliBinary, CliReport, CliStatus, Done, WireError};
use crate::provider::Provider;
use crate::providers::{discover, update};

/// What the Server's machine has installed.
pub trait Clis: Send + Sync {
    /// The copy of `provider`'s CLI the machine runs, looked for afresh.
    fn binary(&self, provider: Provider) -> Option<CliBinary>;
    /// How that copy compares to the newest release. Reaches the registry.
    fn check(&self, provider: Provider) -> std::io::Result<update::Status>;
    /// Install `provider`'s newest release; the copy that answers after.
    fn update(&self, provider: Provider) -> Result<CliBinary, String>;
}

/// This machine's own `claude` and `codex`.
pub struct Host;

impl Clis for Host {
    fn binary(&self, provider: Provider) -> Option<CliBinary> {
        discover::rediscover();
        located(provider)
    }

    fn check(&self, provider: Provider) -> std::io::Result<update::Status> {
        update::check(provider)
    }

    fn update(&self, provider: Provider) -> Result<CliBinary, String> {
        match update::check(provider).map_err(|error| error.to_string())? {
            update::Status::Available {
                upgrade: Some(upgrade),
                ..
            } => {
                update::run(provider, &upgrade)?;
                located(provider)
                    .ok_or_else(|| "the upgrade finished, but no copy answers `--version`".into())
            }
            update::Status::Available { latest, .. } => Err(format!(
                "{latest} is available, but Ferrite cannot tell how this copy was installed"
            )),
            update::Status::UpToDate { current } => Err(format!("{current} is the latest release")),
            update::Status::Missing => Err("no copy is installed".into()),
        }
    }
}

fn located(provider: Provider) -> Option<CliBinary> {
    discover::located(provider).map(|found| CliBinary {
        version: found.version,
        path: found.path,
    })
}

/// One `cli` command, answered from `clis`: what the Server does off its
/// pump, and what a window running its own agents does for itself.
pub fn perform(clis: &dyn Clis, action: &CliAction) -> Result<Done, WireError> {
    match action {
        CliAction::Versions | CliAction::Check => {
            let check = *action == CliAction::Check;
            Ok(Done::Clis {
                clis: Provider::ALL
                    .map(|provider| CliReport {
                        provider,
                        binary: clis.binary(provider),
                        status: check
                            .then(|| {
                                clis.check(provider)
                                    .map(CliStatus::from)
                                    .map_err(|error| {
                                        crate::log!("cli check for {provider}: {error}")
                                    })
                                    .ok()
                            })
                            .flatten(),
                    })
                    .to_vec(),
            })
        }
        CliAction::Update { provider } => clis
            .update(*provider)
            .map(|binary| Done::CliUpdated {
                provider: *provider,
                binary,
            })
            .map_err(failed),
        CliAction::Unknown => Err(failed("the Server does not know this cli command")),
    }
}

fn failed(error: impl ToString) -> WireError {
    WireError {
        code: crate::protocol::ErrorCode::Failed,
        message: error.to_string(),
    }
}
