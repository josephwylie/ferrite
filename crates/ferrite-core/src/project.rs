//! A registered Project as every window names and draws it: its id, its
//! title and its roots. The registry that mints and keeps them is the
//! Server's (`workspace::registry`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// An opaque handle to a registered project. Minted by `register`, carried
/// by Thread headers and the selector's rows — never a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(pub(crate) u64);

impl ProjectId {
    pub fn get(self) -> u64 {
        self.0
    }
}

/// One registered project: a root the operator (or a Thread binding) named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: ProjectId,
    /// What a selector row says: the root's leaf name until the operator
    /// `rename_project`s it.
    pub title: String,
    /// The canonicalized root path.
    pub root: PathBuf,
    /// Other canonicalized roots made available to Sessions in this Project.
    /// `root` remains the primary working directory and git-worktree source.
    pub additional_roots: Vec<PathBuf>,
    /// The high-water mark `mint_branch` counts from. Never decremented —
    /// a removed worktree's branch survives removal, so its number must
    /// never be dealt to a "new worktree" again.
    pub(crate) minted: u64,
}

impl Project {
    /// Primary first, then additional roots in the operator's order.
    pub fn directories(&self) -> impl Iterator<Item = &Path> {
        std::iter::once(self.root.as_path())
            .chain(self.additional_roots.iter().map(PathBuf::as_path))
    }
}
