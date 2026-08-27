//! The workspace registry (#29): the deep module behind the CWD selector.
//!
//! Projects are *registered* root-path entities — an explicit list, never a
//! filesystem scan — and each project carries its own worktree registry.
//! This module hides the persistence file and its atomic rewrite, path
//! canonicalization and dedup, the central worktree layout, branch minting,
//! and the adoption conflict check against `git worktree list`. Callers
//! speak ids and verbs; render code never holds a repo path of its own.
//!
//! The registry is a menu, not history: the Thread headers keep the resolved
//! paths as the durable truth, so a registry file lost by hand orphans
//! nothing — the menu just starts empty and regrows as Threads bind roots.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::GitError;

/// An opaque handle to a registered project. Minted by `register`, carried
/// by Thread headers and the selector's rows — never a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(u64);

/// One registered project: a root the operator (or a Thread binding) named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: ProjectId,
    /// The root's leaf name — what a selector row says.
    pub title: String,
    /// The canonicalized root path.
    pub root: PathBuf,
    /// The high-water mark `mint_branch` counts from. Never decremented —
    /// a removed worktree's branch survives removal, so its number must
    /// never be dealt to a "new worktree" again.
    minted: u64,
}

/// One registered worktree of one project, labeled by its branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub project: ProjectId,
    pub branch: String,
    pub path: PathBuf,
}

/// The registered projects and their worktrees, persisted in one file under
/// the store directory (beside the Thread logs, never inside a repo).
#[derive(Clone)]
pub struct Registry {
    dir: PathBuf,
    projects: Vec<Project>,
    worktrees: BTreeMap<ProjectId, Vec<WorktreeEntry>>,
    next_project: u64,
}

/// The registry file's schema, versioned from day one like the Thread log's.
const SCHEMA_VERSION: u32 = 1;

const FILE_NAME: &str = "registry.json";

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
    minted: u64,
}

#[derive(Serialize, Deserialize)]
struct PersistedWorktree {
    project: ProjectId,
    branch: String,
    path: PathBuf,
}

impl Registry {
    /// Bind a registry to `dir`, loading the registry file if one is there.
    /// A missing file starts empty. Every other read or decode failure is
    /// returned: silently treating protected, damaged, or newer data as an
    /// empty registry would let the next mutation overwrite it.
    pub fn open(dir: &Path) -> io::Result<Registry> {
        let mut registry = Registry {
            dir: dir.to_path_buf(),
            projects: Vec::new(),
            worktrees: BTreeMap::new(),
            next_project: 1,
        };
        let bytes = match fs::read(dir.join(FILE_NAME)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(registry),
            Err(error) => return Err(error),
        };
        let persisted = serde_json::from_slice::<PersistedRegistry>(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if persisted.schema != SCHEMA_VERSION {
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
            .map(|project| project.root.as_path())
            .collect();
        let max_id = persisted
            .projects
            .iter()
            .map(|project| project.id.0)
            .max()
            .unwrap_or(0);
        if ids.len() != persisted.projects.len()
            || roots.len() != persisted.projects.len()
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
                minted: project.minted,
            })
            .collect();
        for worktree in persisted.worktrees {
            registry
                .worktrees
                .entry(worktree.project)
                .or_default()
                .push(WorktreeEntry {
                    project: worktree.project,
                    branch: worktree.branch,
                    path: worktree.path,
                });
        }
        Ok(registry)
    }

    /// Register a project root, idempotently: the path is canonicalized, and
    /// a root already registered answers its existing id — two spellings of
    /// one directory can never become two projects. Durable before this
    /// returns.
    pub fn register(&mut self, root: &Path) -> io::Result<ProjectId> {
        let root = fs::canonicalize(root)?;
        self.register_resolved(root)
    }

    /// Imported history may name a checkout that no longer exists on this
    /// machine. Preserve and register that durable CWD instead of rejecting
    /// otherwise readable history; interactive registration remains strict.
    pub(crate) fn register_recorded(&mut self, root: &Path) -> io::Result<ProjectId> {
        let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        self.register_resolved(root)
    }

    fn register_resolved(&mut self, root: PathBuf) -> io::Result<ProjectId> {
        if let Some(existing) = self.projects.iter().find(|project| project.root == root) {
            return Ok(existing.id);
        }
        let mut next = self.clone();
        let id = ProjectId(next.next_project);
        next.next_project += 1;
        next.projects.push(Project {
            id,
            title: leaf(&root),
            root,
            minted: 0,
        });
        next.persist()?;
        *self = next;
        Ok(id)
    }

    /// Every registered project, in registration order.
    pub fn projects(&self) -> &[Project] {
        &self.projects
    }

    pub fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.iter().find(|project| project.id == id)
    }

    /// The registered worktrees of one project — the workspace chip's rows.
    /// Scoped by construction: no verb answers a cross-project list.
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

    /// Deal the next worktree branch for a project: `ferrite/wt-N` off the
    /// project's own high-water counter. Never a reused number — a removed
    /// worktree's branch survives removal (it keeps the commits reachable),
    /// and a "new worktree" that silently checked out an old branch's work
    /// would not be new. The bump is memory-only until `register_worktree`
    /// persists it, so a refused creation leaks nothing.
    /// Reserve the next branch and its central path as one durable lifecycle
    /// operation. Callers cannot split counter minting, placement, and menu
    /// registration into a half-transaction.
    pub fn reserve_worktree(&mut self, project: ProjectId) -> io::Result<WorktreeEntry> {
        let mut next = self.clone();
        let registered = next
            .projects
            .iter_mut()
            .find(|entry| entry.id == project)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "project is not registered"))?;
        registered.minted += 1;
        let branch = format!("ferrite/wt-{}", registered.minted);
        let path = next.place(project, &branch);
        let entry = WorktreeEntry {
            project,
            branch,
            path,
        };
        next.worktrees
            .entry(project)
            .or_default()
            .push(entry.clone());
        next.persist()?;
        *self = next;
        Ok(entry)
    }

    /// Where a worktree of `project` on `branch` lives: the central layout
    /// `<dir>/worktrees/<repoName>-<hash12>/<branch-dashed>` — outside every
    /// repo, keyed by branch so the tree outlives any one Thread, the
    /// 12-hex path hash keeping same-named repos apart.
    pub fn place(&self, project: ProjectId, branch: &str) -> PathBuf {
        let (title, root) = self
            .project(project)
            .map(|entry| (entry.title.as_str(), entry.root.as_path()))
            .unwrap_or(("unknown", Path::new("")));
        self.dir
            .join("worktrees")
            .join(format!("{title}-{}", hash12(root)))
            .join(branch.replace('/', "-"))
    }

    /// Record a worktree of `project`, idempotently by path. Durable before
    /// this returns — the minted counter rides the same write.
    pub fn register_worktree(
        &mut self,
        project: ProjectId,
        branch: &str,
        path: &Path,
    ) -> io::Result<()> {
        if self
            .worktrees
            .get(&project)
            .is_some_and(|entries| entries.iter().any(|entry| entry.path == path))
        {
            return Ok(());
        }
        let mut next = self.clone();
        next.worktrees
            .entry(project)
            .or_default()
            .push(WorktreeEntry {
                project,
                branch: branch.to_string(),
                path: path.to_path_buf(),
            });
        next.persist()?;
        *self = next;
        Ok(())
    }

    /// Forget the worktree at `path`, whoever's project it was. The tree
    /// and its branch are git's business — this only trims the menu.
    pub fn remove_worktree(&mut self, path: &Path) -> io::Result<()> {
        let mut next = self.clone();
        let mut removed = false;
        for entries in next.worktrees.values_mut() {
            let before = entries.len();
            entries.retain(|entry| entry.path != path);
            removed |= entries.len() != before;
        }
        if removed {
            next.persist()?;
            *self = next;
        }
        Ok(())
    }

    /// Written beside and renamed over, so a crash mid-write leaves the old
    /// file whole — the store's own rewrite discipline.
    fn persist(&self) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let persisted = PersistedRegistry {
            schema: SCHEMA_VERSION,
            next_project: self.next_project,
            projects: self
                .projects
                .iter()
                .map(|project| PersistedProject {
                    id: project.id,
                    title: project.title.clone(),
                    root: project.root.clone(),
                    minted: project.minted,
                })
                .collect(),
            worktrees: self
                .worktrees
                .values()
                .flatten()
                .map(|entry| PersistedWorktree {
                    project: entry.project,
                    branch: entry.branch.clone(),
                    path: entry.path.clone(),
                })
                .collect(),
        };
        let path = self.dir.join(FILE_NAME);
        let tmp = path.with_extension("json.tmp");
        fs::write(
            &tmp,
            serde_json::to_vec(&persisted).map_err(io::Error::other)?,
        )?;
        fs::rename(&tmp, &path)
    }
}

/// The adoption conflict check (#29): before a Thread spawns into an
/// existing worktree, git itself must agree the path is a live worktree of
/// `repo` — a directory merely squatting where a worktree once was, or a
/// worktree of some other repo, refuses adoption instead of hosting a
/// Session under a lie.
pub fn adoption_check(repo: &Path, path: &Path) -> Result<(), GitError> {
    let registered = super::worktree_paths(repo)?;
    // Compared canonicalized: git prints resolved paths, and the registry
    // stores what it was handed.
    let canonical = fs::canonicalize(path).map_err(GitError::Io)?;
    if registered
        .iter()
        .any(|worktree| fs::canonicalize(worktree).ok().as_deref() == Some(&canonical))
    {
        return Ok(());
    }
    Err(GitError::Git {
        detail: format!(
            "{} is not a worktree of {} (git worktree list does not name it)",
            path.display(),
            repo.display()
        ),
    })
}

/// A root's leaf name, for selector rows.
fn leaf(root: &Path) -> String {
    root.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string())
}

/// The first 12 hex digits of a stable 64-bit FNV-1a over the path's bytes.
/// Not `DefaultHasher`, whose output may change across Rust releases — the
/// central layout must place the same repo at the same path forever.
fn hash12(path: &Path) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")[..12].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh scratch directory for one test's registry and repos.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-registry-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// An initialised repo with one committed file, under `root`.
    fn init_repo(root: &Path, name: &str) -> PathBuf {
        let repo = root.join(name);
        fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| crate::workspace::git_for_tests(&repo, args);
        git(&["init", "-q", "-b", "main"]);
        fs::write(repo.join("file.txt"), "base\n").unwrap();
        git(&["add", "file.txt"]);
        git(&[
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "user.name=test",
            "commit",
            "-qm",
            "base",
        ]);
        repo
    }

    /// Registration is idempotent across spellings: the canonicalized root
    /// is the identity, so `repo` and `repo/.` are one project, not two.
    #[test]
    fn registering_two_spellings_of_one_root_yields_one_project() {
        let dir = scratch("dedup");
        let repo = dir.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let mut registry = Registry::open(&dir.join("store")).unwrap();

        let first = registry.register(&repo).unwrap();
        let second = registry.register(&repo.join(".")).unwrap();

        assert_eq!(first, second);
        assert_eq!(registry.projects().len(), 1);
        assert_eq!(registry.projects()[0].title, "repo");
        // A root that does not exist cannot be registered — the menu never
        // offers a directory a Session could not spawn into.
        assert!(registry.register(&dir.join("nowhere")).is_err());
    }

    /// The registry survives reopening: projects, worktrees, and the minted
    /// counter all come back from the file.
    #[test]
    fn the_registry_survives_reopening_its_store() {
        let dir = scratch("reopen");
        let repo = dir.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let store = dir.join("store");
        let mut registry = Registry::open(&store).unwrap();
        let project = registry.register(&repo).unwrap();
        let reserved = registry.reserve_worktree(project).unwrap();
        let branch = reserved.branch;
        let path = reserved.path;
        drop(registry);

        let mut reopened = Registry::open(&store).unwrap();

        assert_eq!(reopened.projects().len(), 1);
        assert_eq!(reopened.project(project).unwrap().title, "repo");
        assert_eq!(
            reopened.worktrees(project),
            [WorktreeEntry {
                project,
                branch: branch.clone(),
                path: path.clone(),
            }]
        );
        assert_eq!(reopened.branch_for(&path), Some(branch.as_str()));
        // The counter persisted with the entry: the next mint moves on.
        assert_eq!(
            reopened.reserve_worktree(project).unwrap().branch,
            "ferrite/wt-2"
        );
        // And a registry file lost by hand is an empty menu, not a failure.
        fs::remove_file(store.join(FILE_NAME)).unwrap();
        assert!(Registry::open(&store).unwrap().projects().is_empty());
    }

    /// The central layout: under the store's `worktrees/`, one directory per
    /// repo — leaf name plus a 12-hex path hash, which is what keeps two
    /// same-named repos apart — one subdirectory per branch, dashed.
    #[test]
    fn placement_follows_the_central_layout_and_separates_same_named_repos() {
        let dir = scratch("place");
        let one = dir.join("teams").join("alpha").join("app");
        let two = dir.join("teams").join("beta").join("app");
        fs::create_dir_all(&one).unwrap();
        fs::create_dir_all(&two).unwrap();
        let store = dir.join("store");
        let mut registry = Registry::open(&store).unwrap();
        let first = registry.register(&one).unwrap();
        let second = registry.register(&two).unwrap();

        let placed_one = registry.place(first, "ferrite/wt-1");
        let placed_two = registry.place(second, "ferrite/wt-1");

        assert_ne!(placed_one, placed_two, "same-named repos must not collide");
        assert_eq!(placed_one.file_name().unwrap(), "ferrite-wt-1");
        let parent = placed_one.parent().unwrap();
        let leaf = parent.file_name().unwrap().to_string_lossy().into_owned();
        assert!(leaf.starts_with("app-"), "leaf: {leaf}");
        assert_eq!(leaf.len(), "app-".len() + 12, "12-hex path hash: {leaf}");
        assert_eq!(parent.parent().unwrap(), store.join("worktrees"));
        // Placement is a pure read: the same ask answers the same path.
        assert_eq!(placed_one, registry.place(first, "ferrite/wt-1"));
    }

    /// Minted branches count up and never reuse a number, even after the
    /// worktree they named is removed — the branch survives removal, and a
    /// "new worktree" must never silently check out an old branch's work.
    #[test]
    fn minted_branches_never_reuse_a_removed_worktree_s_number() {
        let dir = scratch("mint");
        let repo = dir.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let mut registry = Registry::open(&dir.join("store")).unwrap();
        let project = registry.register(&repo).unwrap();

        let first = registry.reserve_worktree(project).unwrap();
        assert_eq!(first.branch, "ferrite/wt-1");
        let path = first.path;
        registry.remove_worktree(&path).unwrap();
        assert!(registry.worktrees(project).is_empty());

        assert_eq!(
            registry.reserve_worktree(project).unwrap().branch,
            "ferrite/wt-2"
        );
    }

    /// The worktree list is scoped per project by construction: each
    /// project answers its own entries and nothing else's.
    #[test]
    fn worktrees_are_scoped_to_their_own_project() {
        let dir = scratch("scoped");
        let one = dir.join("one");
        let two = dir.join("two");
        fs::create_dir_all(&one).unwrap();
        fs::create_dir_all(&two).unwrap();
        let mut registry = Registry::open(&dir.join("store")).unwrap();
        let first = registry.register(&one).unwrap();
        let second = registry.register(&two).unwrap();
        registry.reserve_worktree(first).unwrap();

        assert_eq!(registry.worktrees(first).len(), 1);
        assert!(registry.worktrees(second).is_empty());
    }

    #[test]
    fn a_failed_persist_rolls_memory_back_and_retry_is_real() {
        let dir = scratch("persist-rollback");
        let first = dir.join("first");
        let second = dir.join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let store = dir.join("store");
        let mut registry = Registry::open(&store).unwrap();
        registry.register(&first).unwrap();

        let parked = dir.join("parked-store");
        fs::rename(&store, &parked).unwrap();
        fs::write(&store, "not a directory").unwrap();
        assert!(registry.register(&second).is_err());
        assert_eq!(registry.projects().len(), 1, "memory rolled back");

        fs::remove_file(&store).unwrap();
        fs::rename(&parked, &store).unwrap();
        registry.register(&second).unwrap();
        assert_eq!(registry.projects().len(), 2);
        assert_eq!(Registry::open(&store).unwrap().projects().len(), 2);
    }

    #[test]
    fn future_schema_is_refused_and_never_overwritten() {
        let dir = scratch("future-schema");
        let store = dir.join("store");
        fs::create_dir_all(&store).unwrap();
        let path = store.join(FILE_NAME);
        let future = br#"{"schema":99,"next_project":1,"projects":[],"worktrees":[]}"#;
        fs::write(&path, future).unwrap();

        let error = Registry::open(&store).err().expect("future data refuses");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(&path).unwrap(), future);
    }

    /// The adoption conflict check against real git: a worktree git lists
    /// passes; a plain directory squatting at a path, and a worktree of a
    /// different repo, both refuse.
    #[test]
    fn adoption_accepts_a_live_worktree_and_refuses_impostors() {
        let dir = scratch("adopt");
        let repo = init_repo(&dir, "repo");
        let other = init_repo(&dir, "other");
        let tree = dir
            .join("store")
            .join("worktrees")
            .join("repo-x")
            .join("wt");
        crate::workspace::ensure_worktree(&repo, &tree, "ferrite/wt-1").unwrap();

        adoption_check(&repo, &tree).unwrap();

        // A directory that merely exists is not a worktree.
        let squatter = dir.join("squatter");
        fs::create_dir_all(&squatter).unwrap();
        assert!(adoption_check(&repo, &squatter).is_err());

        // A worktree of some other repo refuses too.
        let foreign = dir.join("foreign-wt");
        crate::workspace::ensure_worktree(&other, &foreign, "ferrite/wt-1").unwrap();
        assert!(adoption_check(&repo, &foreign).is_err());

        // And a path that no longer exists cannot be adopted as-is.
        fs::remove_dir_all(&tree).unwrap();
        assert!(adoption_check(&repo, &tree).is_err());
    }
}
