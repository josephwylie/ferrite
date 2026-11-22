//! What a Delegate's Thread there is, before it starts (T3 Code's
//! `delegate_task` with a target, `peer/RemoteDelegation.ts` at 0fd48228):
//! where it works, found by its parent's repository as T3 finds a linked
//! environment's project; the model and effort it runs, inherited from its
//! parent on the same provider; and how far it goes without asking, never
//! past its parent.
//!
//! Each refusal is T3's, naming a parameter by its API name in backticks
//! and a mode by its API word, which the agent tools say in their own
//! names (`api::tools`).

use std::path::PathBuf;

use serde_json::Value;

use ferrite_core::api::schema::{MachineParams, ThreadStartParams};
use ferrite_core::api::{ApiError, Code, Method};
use ferrite_core::runtime_mode::RuntimeMode;
use ferrite_core::store::Provider;
use ferrite_core::ThreadId;

use super::{refusal, Peer, PeerFailure, Peers};

/// Where the Delegate's Thread works there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Place {
    /// The directory the start named, in the Project it named, if any.
    Checkout(PathBuf, Option<String>),
    /// The directory of the Project there with this id.
    Project(String),
    /// The directory of the one Project there with this repository
    /// (`workspace::repository_key`).
    Repository(String),
}

impl Place {
    /// Where a start that named `checkout` and `project`, or neither, from
    /// a Thread in `repository`, works.
    pub fn of(
        checkout: Option<PathBuf>,
        project: Option<String>,
        repository: Option<String>,
    ) -> Result<Self, ApiError> {
        match (checkout, project, repository) {
            (Some(checkout), project, _) => Ok(Self::Checkout(checkout, project)),
            (None, Some(project), _) => Ok(Self::Project(project)),
            (None, None, Some(repository)) => Ok(Self::Repository(repository)),
            (None, None, None) => Err(ApiError::new(
                Code::TargetRequired,
                "Pass `project`: this Thread's project has no repository to match there.",
            )),
        }
    }
}

/// The mode a Delegate is capped at: the one asked, or its parent's
/// `reach`; never broader.
pub(super) fn mode(asked: Option<RuntimeMode>, reach: RuntimeMode) -> Result<RuntimeMode, ApiError> {
    match asked {
        Some(mode) if mode > reach => Err(ApiError::new(
            Code::WidensCap,
            format!(
                "Child runtime mode {} is broader than parent mode {}.",
                word(mode),
                word(reach)
            ),
        )),
        Some(mode) => Ok(mode),
        None => Ok(reach),
    }
}

fn word(mode: RuntimeMode) -> String {
    serde_json::to_value(mode)
        .ok()
        .and_then(|word| word.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// What a Thread runs on: its provider, model and effort (`None`: the
/// provider's default).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Runs {
    pub provider: Provider,
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// What the Delegate runs: what was asked, the rest inherited from its
/// `parent` as T3 does. On the parent's provider it inherits the model,
/// and on its model the effort too; another provider inherits neither, so
/// it names its model. An effort asked as `None` is none: the provider's
/// default there.
pub(super) fn runs(
    parent: &Runs,
    provider: Option<Provider>,
    model: Option<String>,
    effort: Option<Option<String>>,
) -> Result<Runs, ApiError> {
    let provider = provider.unwrap_or(parent.provider);
    let same = provider == parent.provider;
    let model = match model {
        Some(model) => Some(model),
        None if same => parent.model.clone(),
        None => {
            return Err(ApiError::new(
                Code::TargetRequired,
                "Pass `model` from `provider.list` with this `machine`.",
            ))
        }
    };
    let effort = match effort {
        Some(effort) => effort,
        None => (same && model == parent.model).then(|| parent.effort.clone()).flatten(),
    };
    Ok(Runs {
        provider,
        model,
        effort,
    })
}

impl Runs {
    /// The provider CLI's own arguments for the model and effort, as a
    /// capped start takes them (`runtime_mode::widening` passes both).
    pub fn args(&self) -> Vec<String> {
        let mut args = Vec::new();
        match self.provider {
            Provider::Claude => {
                if let Some(model) = &self.model {
                    args.extend(["--model".into(), model.clone()]);
                }
                if let Some(effort) = &self.effort {
                    args.extend(["--effort".into(), effort.clone()]);
                }
            }
            Provider::Codex => {
                if let Some(model) = &self.model {
                    args.extend(["-m".into(), model.clone()]);
                }
                if let Some(effort) = &self.effort {
                    args.extend(["-c".into(), format!("model_reasoning_effort={effort}")]);
                }
            }
        }
        args
    }
}

/// Start the Thread there at `place`: its Server, and its id there. A
/// repeated key names the Thread the first start made, parked since or
/// not.
pub(super) fn launch(
    peers: &Peers,
    machine: &str,
    place: Place,
    start: ThreadStartParams,
) -> Result<(Peer, ThreadId), ApiError> {
    let failed = |failure: PeerFailure| refusal(&failure, machine);
    let peer = peers.identify(machine).map_err(failed)?;
    let (cwd, project) = match place {
        Place::Checkout(cwd, project) => (cwd, project),
        Place::Project(id) => placed(
            peers,
            &peer,
            |project| project["id"] == *id,
            format!("No project there has the id {id}. `project.list` with this `machine` lists them."),
        )?,
        Place::Repository(key) => placed(
            peers,
            &peer,
            |project| project["worktree"]["repository"] == *key,
            format!(
                "No project there has this repository ({key}). Pass `project`; `project.list` with \
                 this `machine` lists them. To bring the repository there, clone it on {machine} \
                 and open it as a Project there."
            ),
        )?,
    };
    let start = ThreadStartParams {
        cwd,
        project,
        ..start
    };
    let thread = match peers.call(&peer.server, Method::ThreadStart(start)) {
        Ok(answer) => serde_json::from_value(answer["thread"]["thread"].clone()).map_err(|error| {
            failed(PeerFailure::Unsupported(format!(
                "a thread.start answer this Server cannot read: {error}"
            )))
        })?,
        Err(PeerFailure::Refused(ApiError {
            code: Code::ThreadParked,
            thread: Some(thread),
            ..
        })) => thread,
        Err(failure) => return Err(failed(failure)),
    };
    Ok((peer, thread))
}

/// The directory and id of the one Project there that is `wanted`, from
/// its `project.list`; `none` says there is none.
fn placed(
    peers: &Peers,
    peer: &Peer,
    wanted: impl Fn(&Value) -> bool,
    none: String,
) -> Result<(PathBuf, Option<String>), ApiError> {
    let machine = &peer.label;
    let listed = peers
        .call(&peer.server, Method::ProjectList(MachineParams::default()))
        .map_err(|failure| refusal(&failure, machine))?;
    let projects = listed["projects"].as_array().cloned().unwrap_or_default();
    let found: Vec<&Value> = projects.iter().filter(|project| wanted(project)).collect();
    let one = match found.as_slice() {
        [one] => one,
        [] => return Err(ApiError::new(Code::ProjectNotFound, none)),
        several => {
            let named: Vec<String> = several
                .iter()
                .map(|project| {
                    let field = |name: &str| project[name].as_str().unwrap_or_default().to_string();
                    format!("{} ({})", field("label"), field("id"))
                })
                .collect();
            return Err(ApiError::new(
                Code::InvalidRequest,
                format!(
                    "Several projects there have this repository: {}. Pass `project`.",
                    named.join(", ")
                ),
            ));
        }
    };
    let directory: PathBuf = serde_json::from_value(one["directory"].clone()).map_err(|_| {
        let old = "its `ferrited` lists no Project's directory: update it there";
        refusal(&PeerFailure::Unsupported(old.into()), machine)
    })?;
    Ok((directory, one["id"].as_str().map(str::to_string)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parent(provider: Provider, model: Option<&str>, effort: Option<&str>) -> Runs {
        Runs {
            provider,
            model: model.map(str::to_string),
            effort: effort.map(str::to_string),
        }
    }

    /// T3's inheritance: the model on the same provider, the effort on the
    /// same model, neither on another provider, which names its model; an
    /// effort asked as none is none (T3's `options: {}`).
    #[test]
    fn a_delegate_inherits_what_it_does_not_name_on_its_parents_provider() {
        let claude = parent(Provider::Claude, Some("opus"), Some("high"));
        let runs = |provider, model: Option<&str>, effort: Option<&str>| {
            super::runs(
                &claude,
                provider,
                model.map(str::to_string),
                effort.map(|effort| Some(effort.to_string())),
            )
        };
        assert_eq!(
            super::runs(&claude, None, None, Some(None)),
            Ok(parent(Provider::Claude, Some("opus"), None))
        );
        assert_eq!(runs(None, None, None), Ok(claude.clone()));
        assert_eq!(
            runs(None, Some("haiku"), None),
            Ok(parent(Provider::Claude, Some("haiku"), None))
        );
        assert_eq!(
            runs(None, None, Some("low")),
            Ok(parent(Provider::Claude, Some("opus"), Some("low")))
        );
        assert_eq!(
            runs(Some(Provider::Codex), Some("gpt-5.5"), None),
            Ok(parent(Provider::Codex, Some("gpt-5.5"), None))
        );
        let refused = runs(Some(Provider::Codex), None, None).unwrap_err();
        assert_eq!(refused.code, Code::TargetRequired);
        assert_eq!(refused.message, "Pass `model` from `provider.list` with this `machine`.");
    }

    /// Each provider's own spelling, which a capped start takes.
    #[test]
    fn the_model_and_effort_are_the_clis_own_arguments() {
        let claude = parent(Provider::Claude, Some("opus"), Some("high")).args();
        assert_eq!(claude, ["--model", "opus", "--effort", "high"]);
        let codex = parent(Provider::Codex, Some("gpt-5.5"), Some("xhigh")).args();
        assert_eq!(codex, ["-m", "gpt-5.5", "-c", "model_reasoning_effort=xhigh"]);
        for (provider, args) in [(Provider::Claude, claude), (Provider::Codex, codex)] {
            assert!(ferrite_core::runtime_mode::widening(provider, &args, &[], &[]).is_none());
        }
        assert!(parent(Provider::Codex, None, None).args().is_empty());
    }

    /// A mode asked no broader than the parent's is the cap; a broader one
    /// is refused in T3's words.
    #[test]
    fn a_delegate_goes_no_further_than_its_parent() {
        let reach = RuntimeMode::AutoAcceptEdits;
        assert_eq!(mode(None, reach), Ok(reach));
        assert_eq!(
            mode(Some(RuntimeMode::ApprovalRequired), reach),
            Ok(RuntimeMode::ApprovalRequired)
        );
        let refused = mode(Some(RuntimeMode::FullAccess), reach).unwrap_err();
        assert_eq!(refused.code, Code::WidensCap);
        assert_eq!(
            refused.message,
            "Child runtime mode full_access is broader than parent mode auto_accept_edits."
        );
    }
}
