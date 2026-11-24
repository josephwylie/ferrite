//! A registered Project as every window names and draws it: its id, its
//! title and its roots, and a machine's Projects with the worktrees Ferrite
//! made for them, as the registry and a window's copy of it hold them
//! (`Projects`). The registry that mints, changes and keeps them is the
//! Server's (`workspace::registry`).

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// An opaque handle to a registered project. Minted by `register`, carried
/// by Thread headers and the selector's rows — never a path. Its field is
/// public for the registry (ferrite-engine), which alone mints one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(pub u64);

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
    /// never be dealt to a "new worktree" again. Public for the registry,
    /// which alone advances it.
    pub minted: u64,
}

impl Project {
    /// Primary first, then additional roots in the operator's order.
    pub fn directories(&self) -> impl Iterator<Item = &Path> {
        std::iter::once(self.root.as_path())
            .chain(self.additional_roots.iter().map(PathBuf::as_path))
    }
}

/// One registered worktree of one project, labeled by its branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub project: ProjectId,
    /// The repository this worktree belongs to. Kept explicitly so changing
    /// a Project's primary root cannot retarget an existing worktree.
    pub repo: PathBuf,
    pub branch: String,
    pub path: PathBuf,
}

/// A machine's registered Projects and the worktrees Ferrite made for them:
/// what the registry holds, and a window's read-only copy of the Server's
/// (ADR 0025). Only the registry's verbs change it: its fields, `parse` and
/// `to_bytes` are public for them (ferrite-engine), not for a window.
#[derive(Clone)]
pub struct Projects {
    pub projects: Vec<Project>,
    pub worktrees: BTreeMap<ProjectId, Vec<WorktreeEntry>>,
    pub next_project: u64,
}

/// The registry file's schema, versioned from day one like the Thread log's.
const SCHEMA_VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct PersistedRegistry {
    schema: u32,
    next_project: u64,
    projects: Vec<PersistedProject>,
    worktrees: Vec<PersistedWorktree>,
}

#[derive(Serialize, Deserialize)]
struct PersistedProject {
    id: ProjectId,
    title: String,
    root: PathBuf,
    #[serde(default)]
    additional_roots: Vec<PathBuf>,
    minted: u64,
}

#[derive(Serialize, Deserialize)]
struct PersistedWorktree {
    project: ProjectId,
    #[serde(default)]
    repo: Option<PathBuf>,
    branch: String,
    path: PathBuf,
}

impl Projects {
    /// The registry as its file spells it: what a window's copy is built
    /// from (ADR 0025).
    pub fn to_wire(&self) -> serde_json::Value {
        serde_json::to_value(self.persisted()).expect("the registry serializes")
    }

    /// A window's read-only copy of the Server's registry. It is never
    /// written: every change is the Server's.
    pub fn from_wire(value: serde_json::Value) -> io::Result<Projects> {
        let persisted = serde_json::from_value::<PersistedRegistry>(value)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Projects::from_persisted(persisted)
    }

    /// No projects: also a window's copy before the Server's lands.
    pub fn empty() -> Projects {
        Projects {
            projects: Vec::new(),
            worktrees: BTreeMap::new(),
            next_project: 1,
        }
    }

    /// The registry file's bytes, read: what `workspace::registry` opens.
    pub fn parse(bytes: &[u8]) -> io::Result<Projects> {
        let persisted = serde_json::from_slice::<PersistedRegistry>(bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Projects::from_persisted(persisted)
    }

    /// The registry file's bytes, as `workspace::registry` writes them.
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        serde_json::to_vec(&self.persisted()).map_err(io::Error::other)
    }

    fn from_persisted(persisted: PersistedRegistry) -> io::Result<Projects> {
        let mut registry = Projects::empty();
        if !(1..=SCHEMA_VERSION).contains(&persisted.schema) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "registry schema {} is not supported (expected {SCHEMA_VERSION})",
                    persisted.schema
                ),
            ));
        }
        let ids: BTreeSet<ProjectId> = persisted
            .projects
            .iter()
            .map(|project| project.id)
            .collect();
        let roots: BTreeSet<&Path> = persisted
            .projects
            .iter()
            .flat_map(|project| {
                std::iter::once(project.root.as_path())
                    .chain(project.additional_roots.iter().map(PathBuf::as_path))
            })
            .collect();
        let root_count: usize = persisted
            .projects
            .iter()
            .map(|project| 1 + project.additional_roots.len())
            .sum();
        let max_id = persisted
            .projects
            .iter()
            .map(|project| project.id.0)
            .max()
            .unwrap_or(0);
        if ids.len() != persisted.projects.len()
            || roots.len() != root_count
            || persisted.next_project <= max_id
            || persisted
                .worktrees
                .iter()
                .any(|worktree| !ids.contains(&worktree.project))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "registry contains inconsistent project or worktree records",
            ));
        }
        registry.next_project = persisted.next_project;
        registry.projects = persisted
            .projects
            .into_iter()
            .map(|project| Project {
                id: project.id,
                title: project.title,
                root: project.root,
                additional_roots: project.additional_roots,
                minted: project.minted,
            })
            .collect();
        for worktree in persisted.worktrees {
            let repo = worktree.repo.unwrap_or_else(|| {
                registry
                    .project(worktree.project)
                    .map(|project| project.root.clone())
                    .unwrap_or_default()
            });
            registry
                .worktrees
                .entry(worktree.project)
                .or_default()
                .push(WorktreeEntry {
                    project: worktree.project,
                    repo,
                    branch: worktree.branch,
                    path: worktree.path,
                });
        }
        Ok(registry)
    }

    /// Every registered project, in registration order.
    pub fn projects(&self) -> &[Project] {
        &self.projects
    }

    pub fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.iter().find(|project| project.id == id)
    }

    /// The Project with a directory holding `path` (canonical), the nearest
    /// one where directories nest.
    pub fn containing(&self, path: &Path) -> Option<ProjectId> {
        self.projects
            .iter()
            .flat_map(|project| project.directories().map(move |root| (project.id, root)))
            .filter(|(_, root)| path.starts_with(root))
            .max_by_key(|(_, root)| root.components().count())
            .map(|(id, _)| id)
    }

    /// The worktrees Ferrite itself minted for one project — its record of
    /// what `reserve_worktree` placed, not the repo's worktrees: the
    /// workspace chip lists those from `git worktree list`, hand-made ones
    /// included. Scoped by construction: no verb answers a cross-project
    /// list.
    pub fn worktrees(&self, project: ProjectId) -> &[WorktreeEntry] {
        self.worktrees
            .get(&project)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// The branch a registered worktree lives on, found by its path — what
    /// a revive recreates a hand-deleted tree onto.
    pub fn branch_for(&self, path: &Path) -> Option<&str> {
        self.worktrees
            .values()
            .flatten()
            .find(|entry| entry.path == path)
            .map(|entry| entry.branch.as_str())
    }

    fn persisted(&self) -> PersistedRegistry {
        PersistedRegistry {
            schema: SCHEMA_VERSION,
            next_project: self.next_project,
            projects: self
                .projects
                .iter()
                .map(|project| PersistedProject {
                    id: project.id,
                    title: project.title.clone(),
                    root: project.root.clone(),
                    additional_roots: project.additional_roots.clone(),
                    minted: project.minted,
                })
                .collect(),
            worktrees: self
                .worktrees
                .values()
                .flatten()
                .map(|entry| PersistedWorktree {
                    project: entry.project,
                    repo: Some(entry.repo.clone()),
                    branch: entry.branch.clone(),
                    path: entry.path.clone(),
                })
                .collect(),
        }
    }
}
