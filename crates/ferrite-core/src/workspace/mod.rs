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

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub mod registry;

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
/// in a fresh worktree Ferrite will create, or in a worktree it already
/// registered (#29 — the adoptable row). A new worktree's path is not the
/// operator's to choose — the registry places it, which is what turns a
/// choice into a `WorkspaceBinding`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceChoice {
    Main { checkout: PathBuf },
    NewWorktree { repo: PathBuf },
    ExistingWorktree { repo: PathBuf, path: PathBuf },
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

/// Directory names the repository scan never enters or reports: dependency
/// and build output trees, and git's own metadata (SwarmDeck's discovery
/// skip list, #24 — minus `.worktrees`, which its review restored to the
/// scan: worktree nests are exactly where linked worktrees live, and the
/// operator's ask was a worktree selector).
const SCAN_SKIP: [&str; 5] = ["node_modules", ".git", "dist", "target", "build"];

/// How deep below the root the scan looks: a repo more than four directory
/// levels down is not offered (SwarmDeck's depth, #24).
const SCAN_DEPTH: usize = 4;

/// Every directory up to four levels below `root` that is itself a git
/// checkout — holds a `.git` DIRECTORY (a repository) or a `.git` FILE (a
/// linked worktree; #24 review — the operator's worktrees are roots work
/// lands in). `root` itself is never listed: it is the binding, already on
/// offer. Directories named in `SCAN_SKIP` are skipped wholesale, and the
/// order is deterministic (sorted by path). The scan never leaves `root`,
/// so only checkouts INSIDE the binding can ever be offered.
pub fn nested_repositories(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    scan(root, 0, &mut found);
    found.sort();
    found
}

fn scan(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let git = dir.join(".git");
    if depth > 0 && (git.is_dir() || git.is_file()) {
        found.push(dir.to_path_buf());
    }
    if depth == SCAN_DEPTH {
        return;
    }
    // An unreadable directory hides only itself — the scan is a menu, not
    // an audit.
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        // `file_type` over `path().is_dir()`: it does not follow symlinks,
        // so a link cannot loop the walk or reach outside the root.
        let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
        let name = entry.file_name();
        if !is_dir || SCAN_SKIP.iter().any(|skip| name == *skip) {
            continue;
        }
        scan(&entry.path(), depth + 1, found);
    }
}

/// The files and directories under `root` the Composer's `@` menu
/// completes over (#23): relative paths with `/` separators, a directory
/// with a trailing `/`, breadth-first — everything at the top before
/// anything nested, so a deep tree cannot bury the root's own files under
/// the cap — each directory read in name order, dotfiles and the noise
/// directories of `MENTION_SKIP` skipped wholesale, symlinks never
/// followed, and the whole answer capped at `cap` so a monorepo cannot
/// stall a keystroke. A root that cannot be read is an empty menu, not an
/// error — the walk is a menu, not an audit.
pub fn mention_files(root: &Path, cap: usize) -> Vec<String> {
    let mut found = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back((root.to_path_buf(), String::new()));
    while let Some((dir, prefix)) = queue.pop_front() {
        if found.len() >= cap {
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if found.len() >= cap {
                break;
            }
            // `file_type` over `path().is_dir()`: it does not follow
            // symlinks, so a link cannot loop the walk or reach outside.
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                // A path the menu could not insert as text is not offered.
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let relative = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            if kind.is_dir() {
                if MENTION_SKIP.contains(&name) {
                    continue;
                }
                found.push(format!("{relative}/"));
                queue.push_back((entry.path(), relative));
            } else if kind.is_file() {
                found.push(relative);
            }
        }
    }
    found
}

/// Directories the `@` menu never lists: build output, dependencies,
/// caches — bulk nobody mentions on purpose.
const MENTION_SKIP: [&str; 12] = [
    "node_modules",
    "dist",
    "target",
    "build",
    "out",
    "coverage",
    "vendor",
    "venv",
    "__pycache__",
    "DerivedData",
    "Pods",
    "tmp",
];

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

/// The checkout the tree at `cwd` is on (#29's display-only header): the
/// branch by name, or the short commit id for a detached HEAD. `None` where
/// `cwd` is not a git checkout at all — a header has nothing honest to say
/// there. Callers cache the answer on a stated cadence; nothing renders
/// through this per frame.
pub fn checkout_branch(cwd: &Path) -> Option<String> {
    let branch = git(cwd, &["branch", "--show-current"]).ok()?;
    let branch = branch.trim();
    if !branch.is_empty() {
        return Some(branch.to_string());
    }
    // Detached HEAD: `--show-current` answers nothing; the short commit id
    // is the honest name of what is checked out.
    let head = git(cwd, &["rev-parse", "--short", "HEAD"]).ok()?;
    let head = head.trim();
    (!head.is_empty()).then(|| head.to_string())
}

/// The local branches of `repo`, in git's own ref order.
pub fn branches(repo: &Path) -> Result<Vec<String>, GitError> {
    let listed = git(
        repo,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )?;
    Ok(listed.lines().map(str::to_string).collect())
}

/// The worktree paths git itself registers for `repo` — the first line of
/// each `git worktree list --porcelain` stanza, main checkout included.
/// The adoption conflict check's ground truth (#29).
pub(crate) fn worktree_paths(repo: &Path) -> Result<Vec<PathBuf>, GitError> {
    let listed = git(repo, &["worktree", "list", "--porcelain"])?;
    Ok(listed
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect())
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

    /// Discovery (#24): a repo is a directory holding a `.git` DIRECTORY or
    /// a `.git` FILE (a linked worktree) — found down to four levels below
    /// the root, in deterministic order, with the noise directories skipped
    /// wholesale. The root itself is never listed (it is the binding,
    /// already on offer), and a repo may hold nested repos of its own.
    #[test]
    fn nested_repositories_scans_four_levels_and_skips_the_noise() {
        let root = scratch("discovery");
        let plant = |relative: &str| {
            fs::create_dir_all(root.join(relative).join(".git")).unwrap();
        };
        // The root is itself a repo — and still not listed.
        fs::create_dir_all(root.join(".git")).unwrap();
        plant("alpha"); // depth 1
        plant("alpha/vendor/lib"); // depth 3, nested inside a found repo
        plant("beta/c/d/deep"); // depth 4 — the last level scanned
        plant("beta/c/d/deep/deeper"); // depth 5 — beyond the scan
        for noise in [
            "node_modules/x",
            "dist/z",
            "target/w",
            "build/v",
            ".git/modules/sub",
        ] {
            plant(noise);
        }
        // A linked worktree marks itself with a `.git` FILE: a root work
        // can land in, exactly like a full repo (#24 review).
        fs::create_dir_all(root.join("linked")).unwrap();
        fs::write(root.join("linked").join(".git"), "gitdir: elsewhere\n").unwrap();

        assert_eq!(
            nested_repositories(&root),
            vec![
                root.join("alpha"),
                root.join("alpha/vendor/lib"),
                root.join("beta/c/d/deep"),
                root.join("linked"),
            ]
        );
    }

    /// Discovery (#24 review): linked git worktrees — directories holding a
    /// `.git` FILE — are what the operator's "worktree selector" is for.
    /// They are found bare at depth 1 and inside a `.worktrees/` nest alike
    /// (`.worktrees` is deliberately NOT on the skip list: it is exactly
    /// where worktree nests live). Only worktrees INSIDE the binding can
    /// ever appear — the scan never leaves `root`, so Ferrite's own
    /// store-placed worktrees, siblings of the binding, stay correctly
    /// impossible.
    #[test]
    fn linked_worktrees_inside_the_binding_are_discovered() {
        let root = scratch("discovery-worktrees");
        let link = |relative: &str| {
            let dir = root.join(relative);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(".git"), "gitdir: elsewhere\n").unwrap();
        };
        link("checkout"); // depth 1, bare
        link(".worktrees/T3-code"); // the worktree-nest convention

        assert_eq!(
            nested_repositories(&root),
            vec![root.join(".worktrees/T3-code"), root.join("checkout")]
        );
    }

    /// The Composer's `@` menu source (#23): files relative with `/`, the
    /// scan's skip list honoured, deterministic order, and the cap holding.
    #[test]
    fn mention_files_walks_with_the_scan_skip_list_and_caps_the_answer() {
        let root = scratch("mention-files");
        let file = |relative: &str| {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "x\n").unwrap();
        };
        file("README.md");
        file("src/lib.rs");
        file("src/nested/deep.rs");
        for noise in [
            "node_modules/pkg/index.js",
            ".git/HEAD",
            "dist/out.js",
            "target/debug/bin",
            "build/o.txt",
        ] {
            file(noise);
        }

        // Breadth-first: the root's own entries first, directories with
        // a trailing slash, dotfiles and noise never.
        assert_eq!(
            mention_files(&root, 100),
            [
                "README.md",
                "src/",
                "src/lib.rs",
                "src/nested/",
                "src/nested/deep.rs"
            ]
        );
        // The cap bounds the walk — a monorepo cannot stall a keystroke.
        assert_eq!(mention_files(&root, 2), ["README.md", "src/"]);
        // An unreadable or missing root is an empty menu, not an error.
        assert!(mention_files(&root.join("nowhere"), 100).is_empty());
    }

    /// #29: the display-only header's source — the branch by name, the
    /// short id for a detached HEAD, and nothing at all outside a checkout.
    #[test]
    fn checkout_branch_names_the_branch_the_detached_head_or_nothing() {
        let root = scratch("branch-of");
        let repo = init_repo(&root);
        assert_eq!(checkout_branch(&repo).as_deref(), Some("main"));

        let tree = root.join("store").join("wt");
        ensure_worktree(&repo, &tree, "ferrite/wt-1").unwrap();
        assert_eq!(checkout_branch(&tree).as_deref(), Some("ferrite/wt-1"));

        // Detached: the short commit id is what is honestly checked out.
        let head = git(&repo, &["rev-parse", "--short", "HEAD"]).unwrap();
        git(&tree, &["checkout", "-q", "--detach"]).unwrap();
        assert_eq!(checkout_branch(&tree).as_deref(), Some(head.trim()));

        // Not a checkout at all: nothing to say.
        let bare = root.join("bare-dir");
        fs::create_dir_all(&bare).unwrap();
        assert_eq!(checkout_branch(&bare), None);
    }

    #[test]
    fn branches_lists_the_local_branches() {
        let root = scratch("branches");
        let repo = init_repo(&root);
        git(&repo, &["branch", "feature"]).unwrap();

        assert_eq!(branches(&repo).unwrap(), ["feature", "main"]);
    }

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
