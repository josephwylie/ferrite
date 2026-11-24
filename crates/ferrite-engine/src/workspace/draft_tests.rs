//! A Draft's binding resolved against a real registry and its worktrees:
//! `ferrite_core::draft`'s tests, here because the registry is the engine's.

use std::fs;
use std::path::PathBuf;

use std::io;

use ferrite_core::checkout::WorkspaceChoice;
use ferrite_core::draft::{DraftBinding, DraftTarget};
use ferrite_core::provider::ProviderChoice;
use ferrite_core::{ModelInfo, ProjectKey};
use ferrite_core::prompt_files::mention_files;
use ferrite_core::provider::Provider;
use crate::workspace::registry::Registry;
use crate::workspace::{ensure_worktree, git_for_tests};

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

    /// `draft` resolved against the Project the registry lists for it.
    fn resolve(&self, draft: &DraftBinding) -> io::Result<WorkspaceChoice> {
        draft.resolve(self.registry.project(draft.project().project()), true)
    }

    fn worktree(&mut self, project: ProjectKey) -> ferrite_core::project::WorktreeEntry {
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
            fixture.resolve(&draft).unwrap().source_root(),
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
    let main = fixture.resolve(&draft).unwrap();
    assert_eq!(
        main,
        WorkspaceChoice::Main {
            checkout: repo.clone()
        }
    );
    assert!(mention_files(main.source_root(), 100).contains(&"main-only.txt".into()));

    draft.choose_target(DraftTarget::New);
    let new = fixture.resolve(&draft).unwrap();
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
    let existing = fixture.resolve(&draft).unwrap();
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
        fixture.resolve(&draft).unwrap().source_root(),
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
    let error = fixture.resolve(&draft).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(error.to_string().contains(&tree.branch));
    assert_eq!(draft.target(), &target);
    fixture.registry.remove_project(fixture.one.project()).unwrap();
    let replacement = fixture.registry.register(&repo).unwrap();
    assert_ne!(replacement, fixture.one.project());
    for target in [DraftTarget::Main, DraftTarget::New, target] {
        draft.choose_target(target.clone());
        let error = fixture.resolve(&draft).unwrap_err();
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
