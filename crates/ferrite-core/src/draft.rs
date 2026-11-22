//! A draft's choices before its first send. Selection rules and registry
//! resolution live here; Pane focus/errors and Thread creation do not.

use std::io;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::engine::ProviderChoice;
use crate::providers::models::efforts_for;
use crate::workspace::registry::{ProjectId, Registry};
use crate::workspace::WorkspaceChoice;
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
    /// This only reads the registry of the Project's machine and, when its
    /// paths are this
    /// computer's (`here`), asks whether a chosen worktree's directory
    /// still stands: bootstrap still owns worktree creation and the git
    /// adoption check, and on a machine (ADR 0026) its Server asks. Stale
    /// choices fail without selecting a different checkout or mutating the
    /// draft.
    pub fn resolve(&self, registry: &Registry, here: bool) -> io::Result<WorkspaceChoice> {
        let project = registry.project(self.project.project()).ok_or_else(|| {
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::store::Provider;
    use crate::workspace::{ensure_worktree, git_for_tests, mention_files};

    struct Fixture {
        dir: PathBuf,
        registry: Registry,
        one: ProjectKey,
        two: ProjectKey,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("ferrite-draft-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            let mut registry = Registry::open(&dir.join("store")).unwrap();
            let mut register = |name: &str| {
                let repo = dir.join(name);
                fs::create_dir_all(&repo).unwrap();
                git_for_tests(&repo, &["init", "-q", "-b", "main"]);
                fs::write(repo.join("committed.txt"), name).unwrap();
                git_for_tests(&repo, &["add", "committed.txt"]);
                git_for_tests(
                    &repo,
                    &[
                        "-c",
                        "user.email=test@example.invalid",
                        "-c",
                        "user.name=test",
                        "commit",
                        "-qm",
                        "base",
                    ],
                );
                ProjectKey::fixture(0, registry.register(&repo).unwrap())
            };
            let one = register("one");
            let two = register("two");
            Self {
                dir,
                registry,
                one,
                two,
            }
        }

        fn draft(&self, target: DraftTarget) -> DraftBinding {
            DraftBinding::new(
                ProviderChoice {
                    provider: Provider::Claude,
                    model: None,
                },
                self.one,
                target,
            )
        }

        fn worktree(&mut self, project: ProjectKey) -> crate::workspace::registry::WorktreeEntry {
            let entry = self.registry.reserve_worktree(project.project()).unwrap();
            ensure_worktree(
                &self.registry.project(project.project()).unwrap().root,
                &entry.path,
                &entry.branch,
            )
            .unwrap();
            entry
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn changing_project_resets_workspace_but_reselecting_it_preserves_the_choice() {
        let mut fixture = Fixture::new("project-change");
        let tree = fixture.worktree(fixture.one);
        for target in [
            DraftTarget::New,
            DraftTarget::Existing {
                branch: tree.branch,
                path: tree.path,
            },
        ] {
            let mut draft = fixture.draft(target.clone());
            draft.choose_project(fixture.one);
            assert_eq!(draft.target(), &target);
            draft.choose_checkout(fixture.one);
            assert_eq!(draft.target(), &DraftTarget::Main);
            draft.choose_target(target);
            draft.choose_project(fixture.two);
            assert_eq!(draft.project(), fixture.two);
            assert_eq!(draft.target(), &DraftTarget::Main);
            assert_eq!(
                draft
                    .resolve(&fixture.registry, true)
                    .unwrap()
                    .source_root(),
                fixture.registry.project(fixture.two.project()).unwrap().root,
            );
        }
    }

    #[test]
    fn main_new_and_existing_choices_share_their_source_files_with_resolution() {
        let mut fixture = Fixture::new("resolve");
        let tree = fixture.worktree(fixture.one);
        // Each Project mints the same branch name; the choice carries the
        // path, so two Projects' same-named worktrees are never confused.
        let other_tree = fixture.worktree(fixture.two);
        assert_eq!(tree.branch, other_tree.branch);
        let repo = fixture.registry.project(fixture.one.project()).unwrap().root.clone();
        fs::write(repo.join("main-only.txt"), "main").unwrap();
        fs::write(tree.path.join("tree-only.txt"), "tree").unwrap();

        let mut draft = fixture.draft(DraftTarget::Main);
        let main = draft.resolve(&fixture.registry, true).unwrap();
        assert_eq!(
            main,
            WorkspaceChoice::Main {
                checkout: repo.clone()
            }
        );
        assert!(mention_files(main.source_root(), 100).contains(&"main-only.txt".into()));

        draft.choose_target(DraftTarget::New);
        let new = draft.resolve(&fixture.registry, true).unwrap();
        assert_eq!(new, WorkspaceChoice::NewWorktree { repo: repo.clone() });
        assert_eq!(new.source_root(), main.source_root());
        assert_eq!(
            fixture.registry.worktrees(fixture.one.project()).len(),
            1,
            "resolution creates nothing"
        );

        draft.choose_target(DraftTarget::Existing {
            branch: tree.branch.clone(),
            path: tree.path.clone(),
        });
        let existing = draft.resolve(&fixture.registry, true).unwrap();
        assert_eq!(
            existing,
            WorkspaceChoice::ExistingWorktree {
                repo,
                path: tree.path.clone()
            }
        );
        let files = mention_files(existing.source_root(), 100);
        assert!(files.contains(&"tree-only.txt".into()));
        assert!(!files.contains(&"main-only.txt".into()));
        draft.choose_project(fixture.two);
        draft.choose_target(DraftTarget::Existing {
            branch: other_tree.branch,
            path: other_tree.path.clone(),
        });
        assert_eq!(
            draft
                .resolve(&fixture.registry, true)
                .unwrap()
                .source_root(),
            other_tree.path
        );
    }

    #[test]
    fn stale_choices_refuse_resolution_without_falling_back_or_changing_the_draft() {
        let mut fixture = Fixture::new("stale");
        let tree = fixture.worktree(fixture.one);
        let target = DraftTarget::Existing {
            branch: tree.branch.clone(),
            path: tree.path.clone(),
        };
        let mut draft = fixture.draft(target.clone());
        let repo = fixture.registry.project(fixture.one.project()).unwrap().root.clone();
        // The worktree is gone from disk — git removed it — so the choice
        // names a directory that no longer stands.
        crate::workspace::remove_worktree(&repo, &tree.path).unwrap();
        let error = draft.resolve(&fixture.registry, true).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(error.to_string().contains(&tree.branch));
        assert_eq!(draft.target(), &target);
        fixture.registry.remove_project(fixture.one.project()).unwrap();
        let replacement = fixture.registry.register(&repo).unwrap();
        assert_ne!(replacement, fixture.one.project());
        for target in [DraftTarget::Main, DraftTarget::New, target] {
            draft.choose_target(target.clone());
            let error = draft.resolve(&fixture.registry, true).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::NotFound);
            assert!(error
                .to_string()
                .contains("project is no longer registered"));
            assert_eq!(draft.project(), fixture.one);
            assert_eq!(draft.target(), &target);
        }
    }

    #[test]
    fn changing_provider_or_model_keeps_only_compatible_effort() {
        let fixture = Fixture::new("effort");
        let mut draft = fixture.draft(DraftTarget::New);
        let choice = ProviderChoice {
            provider: Provider::Codex,
            model: Some("test-model".into()),
        };
        let mut model = ModelInfo::bare("test-model");
        model.efforts = vec!["low".into(), "high".into()];
        draft.choose_effort(Some("high".into()));
        draft.choose_provider(choice.clone(), &[model.clone()]);
        assert_eq!(draft.provider(), &choice);
        assert_eq!(draft.effort(), Some("high"));
        model.efforts = vec!["low".into()];
        draft.choose_provider(choice.clone(), &[model.clone()]);
        assert_eq!(draft.effort(), None);
        draft.choose_provider(choice, &[model]);
        assert_eq!(draft.effort(), None, "default stays default");
        assert_eq!(draft.target(), &DraftTarget::New);
    }
}
