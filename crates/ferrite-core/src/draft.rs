//! A draft's choices before its first send. Selection rules and their
//! resolution against the chosen Project live here; Pane focus/errors and
//! Thread creation do not.

use std::io;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::checkout::WorkspaceChoice;
use crate::models::efforts_for;
use crate::project::{Project, ProjectId};
use crate::provider::ProviderChoice;
use crate::{MachineKey, ModelInfo, ProjectKey};

/// The workspace choice within the selected Project: the checkout as it
/// stands, one of the repo's other branches checked out there, a fresh
/// branch, one of the repo's existing worktrees, or a fresh worktree. An
/// existing worktree carries the path git lists it at and the branch the
/// row and chip call it by; a new one has no path until bootstrap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DraftTarget {
    Main,
    Branch { name: String },
    NewBranch,
    Existing { branch: String, path: PathBuf },
    New,
}

/// The pending binding, without a Thread or Session. Callers can read the
/// choices but change them through the rules that keep them consistent.
/// Its Project names its machine: a first send starts the Thread there.
#[derive(Debug, Clone, PartialEq)]
pub struct DraftBinding {
    provider: ProviderChoice,
    effort: Option<String>,
    project: ProjectKey,
    target: DraftTarget,
}

/// A binding as it is saved across runs: its Project by the machine's
/// saved name and its id there. One saved before Projects were saved
/// with their machine names none, and is this computer's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedBinding {
    provider: ProviderChoice,
    effort: Option<String>,
    #[serde(default)]
    machine: MachineKey,
    project: ProjectId,
    target: DraftTarget,
}

impl SavedBinding {
    /// The machine its Project is on, as saved files name it.
    pub fn machine(&self) -> &MachineKey {
        &self.machine
    }

    pub fn project(&self) -> ProjectId {
        self.project
    }

    /// The binding again, its Project found as `project` (the saved
    /// machine's, held now).
    pub fn bind(self, project: ProjectKey) -> DraftBinding {
        DraftBinding {
            provider: self.provider,
            effort: self.effort,
            project,
            target: self.target,
        }
    }
}

impl DraftBinding {
    pub fn new(provider: ProviderChoice, project: ProjectKey, target: DraftTarget) -> Self {
        Self {
            provider,
            effort: None,
            project,
            target,
        }
    }

    pub fn provider(&self) -> &ProviderChoice {
        &self.provider
    }

    /// None follows Settings at first send.
    pub fn effort(&self) -> Option<&str> {
        self.effort.as_deref()
    }

    pub fn project(&self) -> ProjectKey {
        self.project
    }

    /// This binding as it is saved, its Project's machine saved as
    /// `machine`.
    pub fn saved(&self, machine: MachineKey) -> SavedBinding {
        SavedBinding {
            provider: self.provider.clone(),
            effort: self.effort.clone(),
            machine,
            project: self.project.project(),
            target: self.target.clone(),
        }
    }

    pub fn target(&self) -> &DraftTarget {
        &self.target
    }

    /// Preserve the chosen Effort only when the new Provider/model takes
    /// it. The Provider's announced catalog supplies compatibility.
    pub fn choose_provider(&mut self, provider: ProviderChoice, announced: &[ModelInfo]) {
        let ladder = efforts_for(provider.provider, provider.model.as_deref(), announced);
        if self
            .effort
            .as_ref()
            .is_some_and(|effort| !ladder.contains(effort))
        {
            self.effort = None;
        }
        self.provider = provider;
    }

    pub fn choose_effort(&mut self, effort: Option<String>) {
        self.effort = effort;
    }

    /// A different Project starts on its main checkout; re-picking the
    /// standing Project preserves its workspace choice.
    pub fn choose_project(&mut self, project: ProjectKey) {
        if self.project != project {
            self.choose_checkout(project);
        }
    }

    /// Choosing a folder or typed path explicitly aims at its checkout,
    /// including when that Project was already selected.
    pub fn choose_checkout(&mut self, project: ProjectKey) {
        self.project = project;
        self.target = DraftTarget::Main;
    }

    pub fn choose_target(&mut self, target: DraftTarget) {
        self.target = target;
    }

    /// Interpret the choice once for both first send and file completion.
    /// This only reads `project`, the Project as its machine lists it now
    /// (none once it is no longer registered), and, when its paths are this
    /// computer's (`here`), asks whether a chosen worktree's directory
    /// still stands: bootstrap still owns worktree creation and the git
    /// adoption check, and on a machine (ADR 0026) its Server asks. Stale
    /// choices fail without selecting a different checkout or mutating the
    /// draft.
    pub fn resolve(&self, project: Option<&Project>, here: bool) -> io::Result<WorkspaceChoice> {
        let project = project.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "the chosen project is no longer registered — re-pick it",
            )
        })?;
        let repo = project.root.clone();
        Ok(match &self.target {
            DraftTarget::Main => WorkspaceChoice::Main { checkout: repo },
            DraftTarget::Branch { name } => WorkspaceChoice::Branch {
                checkout: repo,
                branch: name.clone(),
            },
            DraftTarget::NewBranch => WorkspaceChoice::NewBranch { checkout: repo },
            DraftTarget::New => WorkspaceChoice::NewWorktree { repo },
            DraftTarget::Existing { branch, path } => {
                if here && !path.is_dir() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!(
                            "worktree {branch} is gone from {} — re-pick the workspace",
                            path.display()
                        ),
                    ));
                }
                WorkspaceChoice::ExistingWorktree {
                    repo,
                    path: path.clone(),
                }
            }
        })
    }
}

