//! The checkout a Thread works in, as a window shows it and the protocol
//! carries it: its Workspace binding (CONTEXT.md), what the operator picks
//! at Thread creation, how far its branch has drifted, and what a repo
//! holds for the workspace chip. The `git` that reads them is the
//! Server's (`workspace`).

use std::path::{Path, PathBuf};

/// The checkout a Thread works in. Persisted by the store (as its own
/// schema), resolved to a Session's working directory by the cockpit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceBinding {
    /// The repo's main checkout — the directory itself, shared with the
    /// operator and any other Thread bound the same way.
    Main { checkout: PathBuf },
    /// A dedicated worktree of `repo` — created by Ferrite for this Thread,
    /// or one the Thread's own agent made and moved into (`follow`).
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

    /// The repository this binding is a checkout of — the main checkout
    /// itself, or the repo a worktree was added from. What `git worktree
    /// list` is asked about.
    pub fn repo(&self) -> &Path {
        match self {
            WorkspaceBinding::Main { checkout } => checkout,
            WorkspaceBinding::Worktree { repo, .. } => repo,
        }
    }
}

/// What the operator picks at Thread creation: work in the main checkout,
/// in a fresh worktree Ferrite will create, or in a worktree it already
/// registered (#29 — the adoptable row). A new worktree's path is not the
/// operator's to choose — the registry places it, which is what turns a
/// choice into a `WorkspaceBinding`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceChoice {
    Main {
        checkout: PathBuf,
    },
    Branch {
        checkout: PathBuf,
        branch: String,
    },
    NewBranch {
        checkout: PathBuf,
    },
    NewWorktree {
        repo: PathBuf,
    },
    /// A fresh worktree on a new branch the caller names, made from `base`
    /// (the repo's HEAD when none): the API's `thread.start --branch`.
    NamedWorktree {
        repo: PathBuf,
        branch: String,
        base: Option<String>,
    },
    ExistingWorktree {
        repo: PathBuf,
        path: PathBuf,
    },
}

impl WorkspaceChoice {
    /// The source files available before Thread creation. A new worktree
    /// starts from its repo; an existing one exposes its own checkout.
    pub fn source_root(&self) -> &Path {
        match self {
            Self::Main { checkout } => checkout,
            Self::Branch { checkout, .. } | Self::NewBranch { checkout } => checkout,
            Self::NewWorktree { repo } | Self::NamedWorktree { repo, .. } => repo,
            Self::ExistingWorktree { path, .. } => path,
        }
    }
}

/// The one cwd chain every spawn reads (#29): `session_project_root ??
/// worktree_path ?? project_root` — the last two being what `cwd` already
/// collapses a binding to. Open, revive, send-respawn, sweep and re-aim all
/// answer their `SpawnRequest.cwd` here, so a new fact about where Sessions
/// run travels to every path at once.
pub fn effective_cwd<'a>(
    session_project_root: Option<&'a Path>,
    workspace: Option<&'a WorkspaceBinding>,
) -> Option<&'a Path> {
    session_project_root.or_else(|| workspace.map(WorkspaceBinding::cwd))
}

/// How far a checkout has drifted — the Pane header's second line (#29)
/// and the nav row's details. Every field is `Option`-ish in spirit: a tree
/// with no upstream reports no ahead/behind. Nothing here is invented. Its
/// PR is read apart, on its own cadence (`pull_request`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchStatus {
    /// The branch by name, or a short commit id on a detached HEAD.
    pub branch: Option<String>,
    /// The upstream this branch tracks, when it has one.
    pub upstream: Option<String>,
    /// Commits ahead of / behind the upstream. Both zero without one.
    pub ahead: u32,
    pub behind: u32,
    /// Working-tree entries `git status --porcelain` lists — modified,
    /// staged and untracked alike. Zero is a clean tree.
    pub dirty: u32,
    /// The lines the checkout's work moves against its base: the Changes
    /// row's reading. `None` when git could not count them.
    pub changes: Option<BranchDiff>,
}

/// What a checkout's work adds and removes, and what that is counted
/// against: the same changeset the Panel's Changes Surface reads for the
/// whole branch, so the two never disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchDiff {
    pub against: DiffBase,
    pub added: u32,
    pub removed: u32,
}

/// What a [`BranchDiff`] counts against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffBase {
    /// The base the branch was cut from, as the ref that was compared
    /// (`origin/main`, or `main` with no remote): everything since the
    /// merge base, committed or not, untracked files included.
    Branch(String),
    /// No base: a detached HEAD, or no base that resolves or shares
    /// history. The uncommitted work against `HEAD` alone.
    Uncommitted,
}

/// One worktree as `git worktree list --porcelain` names it: the path git
/// prints (symlinks resolved) and the branch it has checked out — none
/// when its HEAD is detached, or for a bare repository's own stanza.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedWorktree {
    pub path: PathBuf,
    pub branch: Option<String>,
}

/// What a repo holds for the workspace chip: the branch its main checkout
/// is on, its other worktrees, and its local branches — each in git's own
/// order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoListing {
    /// None when the checkout's HEAD is detached.
    pub checked_out: Option<String>,
    pub worktrees: Vec<ListedWorktree>,
    pub branches: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #29: the one cwd chain — session_project_root ?? worktree_path ??
    /// project_root — pure and total over every binding shape.
    #[test]
    fn the_effective_cwd_chain_falls_back_in_order() {
        let root = PathBuf::from("/repos/project/api");
        let main = WorkspaceBinding::Main {
            checkout: "/repos/project".into(),
        };
        let tree = WorkspaceBinding::Worktree {
            repo: "/repos/project".into(),
            path: "/store/worktrees/project-abc/wt".into(),
        };
        assert_eq!(
            effective_cwd(Some(&root), Some(&main)),
            Some(root.as_path()),
            "a session project root outranks the binding"
        );
        assert_eq!(
            effective_cwd(None, Some(&tree)).unwrap(),
            Path::new("/store/worktrees/project-abc/wt")
        );
        assert_eq!(
            effective_cwd(None, Some(&main)).unwrap(),
            Path::new("/repos/project")
        );
        assert_eq!(effective_cwd(None, None), None);
    }
}
