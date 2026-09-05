//! A draft's choices before its first send. Selection rules and registry
//! resolution live here; Pane focus/errors and Thread creation do not.

use std::io;

use crate::cockpit::ProviderChoice;
use crate::providers::models::efforts_for;
use crate::workspace::registry::{ProjectId, Registry};
use crate::workspace::WorkspaceChoice;
use crate::ModelInfo;

/// The workspace choice within the selected Project. An existing worktree
/// is named by its registered branch; a new one has no path until bootstrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftTarget {
    Main,
    Existing { branch: String },
    New,
}

/// The pending binding, without a Thread or Session. Callers can read the
/// choices but change them through the rules that keep them consistent.
#[derive(Debug, Clone)]
pub struct DraftBinding {
    provider: ProviderChoice,
    effort: Option<String>,
    project: ProjectId,
    target: DraftTarget,
}

impl DraftBinding {
    pub fn new(provider: ProviderChoice, project: ProjectId, target: DraftTarget) -> Self {
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

    pub fn project(&self) -> ProjectId {
        self.project
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
    pub fn choose_project(&mut self, project: ProjectId) {
        if self.project != project {
            self.choose_checkout(project);
        }
    }

    /// Choosing a folder or typed path explicitly aims at its checkout,
    /// including when that Project was already selected.
    pub fn choose_checkout(&mut self, project: ProjectId) {
        self.project = project;
        self.target = DraftTarget::Main;
    }

    pub fn choose_target(&mut self, target: DraftTarget) {
        self.target = target;
    }

    /// Interpret the choice once for both first send and file completion.
    /// This only reads the registry: bootstrap still owns worktree creation
    /// and git adoption checks. Stale choices fail without selecting a
    /// different checkout or mutating the draft.
    pub fn resolve(&self, registry: &Registry) -> io::Result<WorkspaceChoice> {
        let project = registry.project(self.project).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "the chosen project is no longer registered — re-pick it",
            )
        })?;
        let repo = project.root.clone();
        Ok(match &self.target {
            DraftTarget::Main => WorkspaceChoice::Main { checkout: repo },
            DraftTarget::New => WorkspaceChoice::NewWorktree { repo },
            DraftTarget::Existing { branch } => {
                let worktree = registry
                    .worktrees(self.project)
                    .iter()
                    .find(|entry| entry.branch == *branch)
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::NotFound,
                            format!(
                                "worktree {branch} is no longer registered — re-pick the workspace"
                            ),
                        )
                    })?;
                WorkspaceChoice::ExistingWorktree {
                    repo,
                    path: worktree.path.clone(),
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
        one: ProjectId,
        two: ProjectId,
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
                registry.register(&repo).unwrap()
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

        fn worktree(&mut self, project: ProjectId) -> crate::workspace::registry::WorktreeEntry {
            let entry = self.registry.reserve_worktree(project).unwrap();
            ensure_worktree(
                &self.registry.project(project).unwrap().root,
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
                draft.resolve(&fixture.registry).unwrap().source_root(),
                fixture.registry.project(fixture.two).unwrap().root,
            );
        }
    }

    #[test]
    fn main_new_and_existing_choices_share_their_source_files_with_resolution() {
        let mut fixture = Fixture::new("resolve");
        let tree = fixture.worktree(fixture.one);
        // Each Project mints the same branch name; resolution must stay
        // scoped to the selected Project, never the first matching branch.
        let other_tree = fixture.worktree(fixture.two);
        assert_eq!(tree.branch, other_tree.branch);
        let repo = fixture.registry.project(fixture.one).unwrap().root.clone();
        fs::write(repo.join("main-only.txt"), "main").unwrap();
        fs::write(tree.path.join("tree-only.txt"), "tree").unwrap();

        let mut draft = fixture.draft(DraftTarget::Main);
        let main = draft.resolve(&fixture.registry).unwrap();
        assert_eq!(
            main,
            WorkspaceChoice::Main {
                checkout: repo.clone()
            }
        );
        assert!(mention_files(main.source_root(), 100).contains(&"main-only.txt".into()));

        draft.choose_target(DraftTarget::New);
        let new = draft.resolve(&fixture.registry).unwrap();
        assert_eq!(new, WorkspaceChoice::NewWorktree { repo: repo.clone() });
        assert_eq!(new.source_root(), main.source_root());
        assert_eq!(
            fixture.registry.worktrees(fixture.one).len(),
            1,
            "resolution creates nothing"
        );

        draft.choose_target(DraftTarget::Existing {
            branch: tree.branch.clone(),
        });
        let existing = draft.resolve(&fixture.registry).unwrap();
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
            branch: tree.branch,
        });
        assert_eq!(
            draft.resolve(&fixture.registry).unwrap().source_root(),
            other_tree.path
        );
    }

    #[test]
    fn stale_choices_refuse_resolution_without_falling_back_or_changing_the_draft() {
        let mut fixture = Fixture::new("stale");
        let tree = fixture.worktree(fixture.one);
        let target = DraftTarget::Existing {
            branch: tree.branch.clone(),
        };
        let mut draft = fixture.draft(target.clone());
        fixture.registry.remove_worktree(&tree.path).unwrap();
        let error = draft.resolve(&fixture.registry).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(error.to_string().contains(&tree.branch));
        assert_eq!(draft.target(), &target);
        let repo = fixture.registry.project(fixture.one).unwrap().root.clone();
        fixture.registry.remove_project(fixture.one).unwrap();
        let replacement = fixture.registry.register(&repo).unwrap();
        assert_ne!(replacement, fixture.one);
        for target in [DraftTarget::Main, DraftTarget::New, target] {
            draft.choose_target(target.clone());
            let error = draft.resolve(&fixture.registry).unwrap_err();
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
