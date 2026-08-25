//! Workspace binding: the checkout a Thread works in — a per-Thread worktree
//! Ferrite creates, or the main checkout — chosen at Thread creation
//! (CONTEXT.md). This module holds the binding vocabulary and the `git
//! worktree` operations behind the worktree half, spoken to the operator's
//! own `git` via `std::process::Command`.
//!
//! Probed against real git in a scratch repo before anything here was
//! written; every behavior a function leans on is named where it is used.
//!
//! Known leaving: an operator who deletes the store directory by hand takes
//! every worktree in it along, and the repo keeps their stale registrations
//! until `git worktree prune` (which `ensure_worktree` runs before creating,
//! so Ferrite's own paths self-heal). Documented, not built for.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The checkout a Thread works in. Persisted by the store (as its own
/// schema), resolved to a Session's working directory by the cockpit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceBinding {
    /// The repo's main checkout — the directory itself, shared with the
    /// operator and any other Thread bound the same way.
    Main { checkout: PathBuf },
    /// A dedicated worktree of `repo`, created by Ferrite for this Thread.
    Worktree { repo: PathBuf, path: PathBuf },
}

impl WorkspaceBinding {
    /// Where this Thread's Session runs.
    pub fn cwd(&self) -> &Path {
        match self {
            WorkspaceBinding::Main { checkout } => checkout,
            WorkspaceBinding::Worktree { path, .. } => path,
        }
    }
}

/// What the operator picks at Thread creation: work in the main checkout,
/// or in a dedicated worktree Ferrite will create. The worktree's own path
/// is not the operator's to choose — the store places it, which is what
/// turns a choice into a `WorkspaceBinding`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceChoice {
    Main { checkout: PathBuf },
    Worktree { repo: PathBuf },
}

/// A git operation failed, or could not be run at all.
#[derive(Debug)]
pub enum GitError {
    /// git ran and refused; `detail` is the command and its stderr.
    Git {
        detail: String,
    },
    Io(io::Error),
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitError::Git { detail } => write!(f, "{detail}"),
            GitError::Io(e) => write!(f, "could not run git: {e}"),
        }
    }
}

impl std::error::Error for GitError {}

/// Make sure the worktree at `path` exists, on its own branch named
/// `branch` — creating either or both on demand.
pub fn ensure_worktree(repo: &Path, path: &Path, branch: &str) -> Result<(), GitError> {
    if path.join(".git").exists() {
        return Ok(());
    }
    // A worktree deleted by hand leaves a stale registration that keeps its
    // branch "checked out" (probed: re-adding the branch fails until a
    // prune). Pruning first makes recreation just work.
    git(repo, &["worktree", "prune"])?;
    let path = path_str(path)?;
    if git(repo, &["branch", "--list", branch])?.trim().is_empty() {
        // Probed: a bare `worktree add` would mint a branch named after the
        // leaf directory — every Thread's would collide on "worktree".
        git(repo, &["worktree", "add", "-b", branch, path])?;
    } else {
        // The Thread's branch already exists (a recreated worktree): check
        // it out rather than failing on `-b`.
        git(repo, &["worktree", "add", path, branch])?;
    }
    Ok(())
}

/// Command arguments are strings; a path that is not UTF-8 cannot be handed
/// to `git` through this seam.
fn path_str(path: &Path) -> Result<&str, GitError> {
    path.to_str().ok_or_else(|| {
        GitError::Io(io::Error::other(format!(
            "path is not valid UTF-8: {}",
            path.display()
        )))
    })
}

/// Whether the tree at `cwd` is clean exactly as `git status` defines it:
/// nothing modified, nothing untracked — an empty porcelain listing.
pub fn is_clean(cwd: &Path) -> Result<bool, GitError> {
    Ok(git(cwd, &["status", "--porcelain"])?.trim().is_empty())
}

/// Remove the worktree at `path`. Never forced: the caller decides what to
/// do about a dirty tree, and git itself refuses one as a second guard. The
/// worktree's branch survives removal — a clean tree can still hold
/// unmerged commits, and the branch is what keeps them reachable.
pub fn remove_worktree(repo: &Path, path: &Path) -> Result<(), GitError> {
    git(repo, &["worktree", "remove", path_str(path)?])?;
    Ok(())
}

/// Test-only: run one git command and unwrap — the shared plumbing the
/// cockpit's binding tests drive their scratch repos with.
#[cfg(test)]
pub(crate) fn git_for_tests(repo: &Path, args: &[&str]) -> String {
    git(repo, args).unwrap()
}

/// Run one git command against `repo`, answering its stdout.
fn git(repo: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(GitError::Io)?;
    if !output.status.success() {
        return Err(GitError::Git {
            detail: format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A fresh scratch directory holding nothing but this test's repo and
    /// worktrees — never anywhere near a real checkout.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-workspace-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// An initialised repo with one committed file, under `root`.
    fn init_repo(root: &Path) -> PathBuf {
        let repo = root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]).unwrap();
        fs::write(repo.join("file.txt"), "base\n").unwrap();
        git(&repo, &["add", "file.txt"]).unwrap();
        git(
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
        )
        .unwrap();
        repo
    }

    fn branch_of(tree: &Path) -> String {
        git(tree, &["branch", "--show-current"])
            .unwrap()
            .trim()
            .to_string()
    }

    /// The isolation the binding buys: two Threads on one repo, each in its
    /// own worktree, each seeing only its own changes — and the main
    /// checkout seeing neither.
    #[test]
    fn two_worktrees_of_one_repo_cannot_touch_each_other() {
        let root = scratch("isolated");
        let repo = init_repo(&root);
        let one = root.join("store").join("1").join("worktree");
        let two = root.join("store").join("2").join("worktree");
        ensure_worktree(&repo, &one, "ferrite/thread-1").unwrap();
        ensure_worktree(&repo, &two, "ferrite/thread-2").unwrap();

        fs::write(one.join("only-one.txt"), "one wrote this\n").unwrap();
        fs::write(two.join("file.txt"), "two edited this\n").unwrap();

        // Each tree is dirty with exactly its own change.
        let status_one = git(&one, &["status", "--porcelain"]).unwrap();
        let status_two = git(&two, &["status", "--porcelain"]).unwrap();
        assert!(status_one.contains("only-one.txt"), "one: {status_one}");
        assert!(!status_one.contains("file.txt"), "one: {status_one}");
        assert!(status_two.contains("file.txt"), "two: {status_two}");
        assert!(!status_two.contains("only-one.txt"), "two: {status_two}");

        // Neither tree holds the other's work, and main saw nothing.
        assert!(!two.join("only-one.txt").exists());
        assert_eq!(fs::read_to_string(one.join("file.txt")).unwrap(), "base\n");
        assert_eq!(fs::read_to_string(repo.join("file.txt")).unwrap(), "base\n");
        assert!(is_clean(&repo).unwrap());
    }

    /// "Clean" is git's own word, nothing else: an empty porcelain listing.
    /// Untracked counts as dirty — `git worktree remove` refuses it too.
    #[test]
    fn clean_means_exactly_what_git_status_says() {
        let root = scratch("clean");
        let repo = init_repo(&root);
        let tree = root.join("store").join("1").join("worktree");
        ensure_worktree(&repo, &tree, "ferrite/thread-1").unwrap();
        assert!(is_clean(&tree).unwrap());

        fs::write(tree.join("scratch.txt"), "untracked\n").unwrap();
        assert!(!is_clean(&tree).unwrap(), "untracked is dirty");
        fs::remove_file(tree.join("scratch.txt")).unwrap();
        assert!(is_clean(&tree).unwrap());

        fs::write(tree.join("file.txt"), "modified\n").unwrap();
        assert!(!is_clean(&tree).unwrap(), "modified is dirty");
    }

    /// A branch serves one worktree at a time: asking for a second worktree
    /// on a branch that is already checked out fails loudly — and leaves no
    /// directory and no registration behind, so the refused path is not
    /// poisoned for its own later use. Probed, now pinned.
    #[test]
    fn a_branch_already_checked_out_refuses_a_second_worktree_cleanly() {
        let root = scratch("collision");
        let repo = init_repo(&root);
        let first = root.join("store").join("1").join("worktree");
        ensure_worktree(&repo, &first, "ferrite/thread-1").unwrap();

        let second = root.join("store").join("9").join("worktree");
        match ensure_worktree(&repo, &second, "ferrite/thread-1") {
            Err(GitError::Git { detail }) => {
                assert!(detail.contains("worktree add"), "detail: {detail}")
            }
            other => panic!("a checked-out branch must refuse a second tree: {other:?}"),
        }
        assert!(!second.exists(), "the failed add left a directory behind");
        assert_eq!(branch_of(&first), "ferrite/thread-1");

        // No registration was left either: the same path works at once with
        // its own branch.
        ensure_worktree(&repo, &second, "ferrite/thread-9").unwrap();
        assert_eq!(branch_of(&second), "ferrite/thread-9");
    }

    /// A brand-new repo with no commits still gets a worktree: git infers an
    /// orphan branch. Probed on this machine's git, now pinned — an older
    /// git without the inference fails this loudly, which is the alarm
    /// working.
    #[test]
    fn a_repo_with_no_commits_still_gets_a_worktree() {
        let root = scratch("unborn");
        let repo = root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]).unwrap();

        let tree = root.join("store").join("1").join("worktree");
        ensure_worktree(&repo, &tree, "ferrite/thread-1").unwrap();

        assert!(tree.join(".git").exists());
        assert_eq!(branch_of(&tree), "ferrite/thread-1");
    }

    /// Removing a clean worktree deletes the tree and nothing else: the
    /// branch survives, because a clean tree can still hold unmerged
    /// commits and the branch is what keeps them reachable.
    #[test]
    fn removing_a_clean_worktree_keeps_its_branch() {
        let root = scratch("remove");
        let repo = init_repo(&root);
        let tree = root.join("store").join("1").join("worktree");
        ensure_worktree(&repo, &tree, "ferrite/thread-1").unwrap();

        remove_worktree(&repo, &tree).unwrap();

        assert!(!tree.exists());
        let branches = git(&repo, &["branch", "--list", "ferrite/thread-1"]).unwrap();
        assert!(
            branches.contains("ferrite/thread-1"),
            "the branch must survive: {branches:?}"
        );
    }

    /// The never-force guarantee: a dirty tree is refused by git itself, and
    /// the work is still there afterwards. Callers gate on `is_clean`; this
    /// is the second lock on the same door.
    #[test]
    fn removing_a_dirty_worktree_is_refused_and_loses_nothing() {
        let root = scratch("remove-dirty");
        let repo = init_repo(&root);
        let tree = root.join("store").join("1").join("worktree");
        ensure_worktree(&repo, &tree, "ferrite/thread-1").unwrap();
        fs::write(tree.join("wip.txt"), "uncommitted work\n").unwrap();

        match remove_worktree(&repo, &tree) {
            Err(GitError::Git { detail }) => {
                assert!(detail.contains("worktree remove"), "detail: {detail}")
            }
            other => panic!("a dirty tree must refuse removal: {other:?}"),
        }
        assert_eq!(
            fs::read_to_string(tree.join("wip.txt")).unwrap(),
            "uncommitted work\n"
        );
    }

    /// A worktree deleted by hand comes back on demand — same path, same
    /// branch, prior commits intact. Probed: without a prune, the stale
    /// registration blocks the branch.
    #[test]
    fn a_hand_deleted_worktree_is_recreated_on_its_own_branch() {
        let root = scratch("recreate");
        let repo = init_repo(&root);
        let tree = root.join("store").join("1").join("worktree");
        ensure_worktree(&repo, &tree, "ferrite/thread-1").unwrap();
        fs::write(tree.join("file.txt"), "committed by the thread\n").unwrap();
        git(
            &tree,
            &[
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "user.name=test",
                "commit",
                "-aqm",
                "thread work",
            ],
        )
        .unwrap();
        fs::remove_dir_all(&tree).unwrap();

        ensure_worktree(&repo, &tree, "ferrite/thread-1").unwrap();

        assert_eq!(branch_of(&tree), "ferrite/thread-1");
        assert_eq!(
            fs::read_to_string(tree.join("file.txt")).unwrap(),
            "committed by the thread\n",
            "the branch's commits must come back with it"
        );
    }

    #[test]
    fn a_worktree_is_created_on_demand_with_its_own_branch() {
        let root = scratch("create");
        let repo = init_repo(&root);
        let path = root.join("store").join("1").join("worktree");

        ensure_worktree(&repo, &path, "ferrite/thread-1").unwrap();

        // A real checkout of the repo's content, on the Thread's own branch,
        // leaving the main checkout where it was.
        assert_eq!(fs::read_to_string(path.join("file.txt")).unwrap(), "base\n");
        assert_eq!(branch_of(&path), "ferrite/thread-1");
        assert_eq!(branch_of(&repo), "main");
    }
}
