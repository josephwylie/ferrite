//! Workspace binding: the checkout a Thread works in — a per-Thread worktree
//! Ferrite creates, or the main checkout — chosen at Thread creation and
//! following the agent after it (CONTEXT.md; `follow`). This module holds
//! the `git worktree` operations behind the worktree half, spoken to the
//! operator's own `git` via `std::process::Command`. The binding's types
//! are in the shared `checkout` module.
//!
//! Probed against real git in a scratch repo before anything here was
//! written; every behavior a function leans on is named where it is used.
//!
//! Known leaving: an operator who deletes the store directory by hand takes
//! every worktree in it along, and the repo keeps their stale registrations
//! until `git worktree prune` (which `ensure_worktree` runs before creating,
//! so Ferrite's own paths self-heal). Documented, not built for.

use crate::checkout::{BranchDiff, BranchStatus, DiffBase, ListedWorktree, RepoListing};
use crate::folder::{expand_home, DirEntry, Listing};
use crate::pull_request::{parse_pr_json, PullRequest};
use crate::spawn::NoConsoleWindow;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub mod follow;
pub mod registry;
mod repository;
mod stamp;

pub(crate) use repository::{project_roots, repositories};
pub use repository::{repository_key, repository_key_of};
pub use stamp::{checkout_stamp, head_of};

/// Move the shared project checkout to an existing branch selected before
/// the Thread starts. Git owns validation and refuses dirty/conflicting moves.
pub fn checkout_existing_branch(cwd: &Path, branch: &str) -> Result<(), GitError> {
    git(cwd, &["switch", branch]).map(|_| ())
}

/// Create and check out a fresh branch in the shared project checkout.
pub fn checkout_new_branch(cwd: &Path, branch: &str) -> Result<(), GitError> {
    git(cwd, &["switch", "-c", branch]).map(|_| ())
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

/// Read a directory on this computer: how a window holding a machine browses
/// the machine's folders through its Server (ADR 0026). A name that is not
/// UTF-8 is left out: it could not be typed back.
pub fn list_dir(path: &Path) -> io::Result<Listing> {
    let path = expand_home(path);
    let mut entries: Vec<DirEntry> = fs::read_dir(&path)?
        .flatten()
        .filter_map(|entry| {
            Some(DirEntry {
                directory: entry.path().is_dir(),
                name: entry.file_name().into_string().ok()?,
            })
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Listing { path, entries })
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

/// Where Ferrite places the worktrees it makes, inside their project
/// (`Registry::place`).
pub const WORKTREES: &str = ".worktrees";

/// A worktree placed inside its own repo (`<root>/.worktrees/…`) stays out
/// of the main checkout's `git status`: its top directory goes into the
/// repo's `info/exclude`, once — never into `.gitignore`, a file the repo
/// tracks. A worktree outside the repo needs nothing. The repo may be
/// spelled through a symlink its registered root was resolved from.
fn exclude_inside(repo: &Path, path: &Path) -> Result<(), GitError> {
    let inside = path.strip_prefix(repo).ok().map(Path::to_path_buf).or_else(|| {
        let canonical = dunce::canonicalize(repo).ok()?;
        path.strip_prefix(&canonical).ok().map(Path::to_path_buf)
    });
    let Some(top) = inside.and_then(|inside| {
        inside
            .components()
            .next()
            .map(|top| top.as_os_str().to_string_lossy().into_owned())
    }) else {
        return Ok(());
    };
    let pattern = format!("/{top}/");
    let common = git(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let exclude = Path::new(common.trim()).join("info").join("exclude");
    let mut text = fs::read_to_string(&exclude).unwrap_or_default();
    if text.lines().any(|line| line.trim() == pattern) {
        return Ok(());
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&pattern);
    text.push('\n');
    fs::create_dir_all(exclude.parent().expect("info/exclude has a parent"))
        .and_then(|()| fs::write(&exclude, text))
        .map_err(GitError::Io)
}

/// Make sure the worktree at `path` exists, on its own branch named
/// `branch` — creating either or both on demand.
pub fn ensure_worktree(repo: &Path, path: &Path, branch: &str) -> Result<(), GitError> {
    if path.join(".git").exists() {
        return Ok(());
    }
    let _placing = placing();
    // A worktree deleted by hand leaves a stale registration that keeps its
    // branch "checked out" (probed: re-adding the branch fails until a
    // prune). Pruning first makes recreation just work.
    git(repo, &["worktree", "prune"])?;
    exclude_inside(repo, path)?;
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

/// Starts place their worktrees on spawning threads at once (ADR 0018).
/// One at a time: a prune can remove another add's admin directory in the
/// instant before git marks it "initializing".
fn placing() -> std::sync::MutexGuard<'static, ()> {
    static PLACING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    PLACING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Add a worktree at `path` on a new branch `branch`, made from `base` (the
/// repo's HEAD when none). Git refuses a branch that already exists: a new
/// worktree never takes over another's work.
pub fn add_worktree(
    repo: &Path,
    path: &Path,
    branch: &str,
    base: Option<&str>,
) -> Result<(), GitError> {
    let _placing = placing();
    git(repo, &["worktree", "prune"])?;
    exclude_inside(repo, path)?;
    let mut args = vec!["worktree", "add", "-b", branch, path_str(path)?];
    args.extend(base);
    git(repo, &args).map(|_| ())
}

/// Which repository the checkout at `root` belongs to, read from its `.git`
/// without running git: a main checkout's is a directory; a linked
/// worktree's is a file naming its admin directory, which holds the
/// `commondir` it shares with the main checkout. `None` outside git. A
/// submodule's `.git` file has no `commondir`: it is a checkout of its own.
pub fn checkout_of(root: &Path) -> Option<Checkout> {
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some(Checkout {
            repo: root.to_path_buf(),
            linked: false,
        });
    }
    let pointer = fs::read_to_string(&dot_git).ok()?;
    let admin = root.join(pointer.trim().strip_prefix("gitdir:")?.trim());
    let Ok(common) = fs::read_to_string(admin.join("commondir")) else {
        return Some(Checkout {
            repo: root.to_path_buf(),
            linked: false,
        });
    };
    let common = dunce::canonicalize(admin.join(common.trim())).ok()?;
    Some(Checkout {
        repo: common.parent()?.to_path_buf(),
        linked: true,
    })
}

/// Whether `a` and `b` are one directory, however spelled (a symlink, a
/// trailing `.`); paths not on disk are compared as written.
pub fn same_dir(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (dunce::canonicalize(a), dunce::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

/// A checkout, as `checkout_of` reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    /// The repository's main checkout.
    pub repo: PathBuf,
    /// A linked worktree of `repo`, not the main checkout itself.
    pub linked: bool,
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

/// The branch a repo calls its default: what `origin/HEAD` points at, else
/// `main` when that branch exists, else `master` when it does, else `main`.
/// Each probe is a `git` call, so the UI reads it once per Project into its
/// facts cache and never from a frame.
pub fn default_branch(cwd: &Path) -> String {
    let origin = git(
        cwd,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .ok();
    pick_default_branch(origin.as_deref(), |name| {
        git(
            cwd,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{name}"),
            ],
        )
        .is_ok()
    })
}

/// `default_branch`'s ladder over what git answered: the remote's HEAD
/// (`origin/main` → `main`), else the first of `main` and `master` that
/// exists locally, else `main`, the name a fresh clone would use.
pub fn pick_default_branch(origin_head: Option<&str>, has_branch: impl Fn(&str) -> bool) -> String {
    if let Some(head) = origin_head.map(str::trim).filter(|head| !head.is_empty()) {
        return head.strip_prefix("origin/").unwrap_or(head).to_string();
    }
    ["main", "master"]
        .into_iter()
        .find(|name| has_branch(name))
        .unwrap_or("main")
        .to_string()
}

/// Count the checkout's work against its base, off the UI thread.
/// `detached` is `git status`'s word: a detached HEAD has no branch to
/// compare, only its uncommitted work.
pub fn branch_diff(cwd: &Path, detached: bool) -> Option<BranchDiff> {
    use ferrite_reviewr::git::{self as review, ResolvedBase};
    // The Changes Surface's own base chain: the operator's pick, else the
    // default branch, origin's before the local one.
    let base = (!detached)
        .then(|| review::resolve_base(cwd, None).ok()?.status.winner)
        .flatten()
        .and_then(|base| Some((review::merge_base(cwd, base.oid())?, base)));
    let (old, against) = match base {
        Some((merge_base, base)) => {
            let name = match base {
                ResolvedBase::Branch { name, oid } => {
                    let remote = format!("refs/remotes/origin/{name}^{{commit}}");
                    match git(cwd, &["rev-parse", "--verify", "--quiet", &remote]) {
                        Ok(tip) if tip.trim() == oid => format!("origin/{name}"),
                        _ => name,
                    }
                }
                ResolvedBase::Rev { spelling, .. } => spelling,
            };
            (merge_base, DiffBase::Branch(name))
        }
        None => (
            review::diff_base(review::head_oid(cwd)),
            DiffBase::Uncommitted,
        ),
    };
    let files = review::changed_from(cwd, &old).ok()?;
    Some(BranchDiff {
        against,
        added: files.iter().map(|file| file.additions).sum(),
        removed: files.iter().map(|file| file.deletions).sum(),
    })
}

/// Read the checkout's drift, off the UI thread. One `git status
/// --porcelain=v2 --branch` answers branch, upstream, ahead/behind and dirt
/// together; `branch_diff` then counts the work against its base. `None`
/// where `cwd` is not a checkout at all.
pub fn branch_status(cwd: &Path) -> Option<BranchStatus> {
    let listed = git(cwd, &["status", "--porcelain=v2", "--branch"]).ok()?;
    let mut status = BranchStatus::default();
    let mut detached = false;
    for line in listed.lines() {
        if let Some(head) = line.strip_prefix("# branch.head ") {
            let head = head.trim();
            detached = head == "(detached)";
            if !detached {
                status.branch = Some(head.to_string());
            }
        } else if let Some(upstream) = line.strip_prefix("# branch.upstream ") {
            status.upstream = Some(upstream.trim().to_string());
        } else if let Some(ab) = line.strip_prefix("# branch.ab ") {
            let mut counts = ab.split_whitespace();
            status.ahead = counts.next().and_then(parse_signed).unwrap_or(0);
            status.behind = counts.next().and_then(parse_signed).unwrap_or(0);
        } else if !line.starts_with('#') && !line.trim().is_empty() {
            status.dirty += 1;
        }
    }
    if status.branch.is_none() {
        // Detached HEAD: the short commit id is the honest name.
        status.branch = git(cwd, &["rev-parse", "--short", "HEAD"])
            .ok()
            .map(|head| head.trim().to_string())
            .filter(|head| !head.is_empty());
    }
    status.changes = branch_diff(cwd, detached);
    Some(status)
}

/// The files git tracks in the checkout `cwd` is in though an ignore rule
/// matches them (force-added, or committed into an ignored folder), from
/// the checkout's top: `git status` reports on them, but a walk that
/// honours the ignores never meets them, so `checkout_stamp` reads them by
/// name. Empty outside a checkout, or when git cannot say.
pub fn tracked_ignored(cwd: &Path) -> Vec<PathBuf> {
    let Some((top, _)) = stamp::checkout_dirs(cwd) else {
        return Vec::new();
    };
    ferrite_reviewr::git::tracked_ignored(top).into_iter().map(PathBuf::from).collect()
}

/// `+3` / `-0` as git writes them in porcelain v2.
fn parse_signed(token: &str) -> Option<u32> {
    token.trim_start_matches(['+', '-']).parse::<u32>().ok()
}

/// The branch's PR through `gh`, as `pr_view` reads it.
pub fn pull_request(cwd: &Path) -> Option<PullRequest> {
    parse_pr_json(&pr_view(cwd)?)
}

/// `gh pr view`'s JSON for the branch checked out at `cwd`: what a Server
/// answers a window with (`pull_request` on the wire), for the window to
/// read with `parse_pr_json`. Every failure — no `gh`, not logged in, not
/// a GitHub remote, no PR for this branch — is the same answer: nothing to
/// say about a PR. `gh` reads the checkout through `git`, so it is held to
/// the same no-optional-locks rule.
pub fn pr_view(cwd: &Path) -> Option<String> {
    gh(
        cwd,
        &[
            "pr",
            "view",
            "--json",
            "number,state,isDraft,url,title,statusCheckRollup,mergeable",
        ],
    )
}

/// Run one `gh` command in `cwd`, answering its stdout; `None` on every
/// failure.
fn gh(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = gh_output(cwd, args).ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Run one `gh` command in `cwd` as the PR reads do, answering all it
/// said: what a Factory's GitHub start reads through as well.
pub(crate) fn gh_output(cwd: &Path, args: &[&str]) -> io::Result<std::process::Output> {
    Command::new("gh")
        .args(args)
        .current_dir(cwd)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .no_console_window()
        .output()
}

/// The local branches of `repo`, in git's own ref order. The workspace
/// chip's `existing branch` rows.
pub fn branches(repo: &Path) -> Result<Vec<String>, GitError> {
    let listed = git(
        repo,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )?;
    Ok(listed.lines().map(str::to_string).collect())
}

/// Every worktree git itself registers for `repo`, main checkout first as
/// git lists it — one stanza each, its `worktree` line the path and its
/// `branch refs/heads/…` line the branch. Hand-made worktrees (a repo's
/// own `.worktrees/*`, say) are listed exactly like Ferrite's own: this is
/// git's answer, not the registry's, and the workspace chip's worktree
/// rows read it.
pub fn worktrees(repo: &Path) -> Result<Vec<ListedWorktree>, GitError> {
    let listed = git(repo, &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktrees(&listed))
}

/// The stanzas of `git worktree list --porcelain`: each opens with its
/// `worktree <path>` line; a `branch refs/heads/<name>` line follows on a
/// checked-out branch, `detached` on none. `HEAD`, `bare`, `locked` and
/// `prunable` lines are passed over — the path and branch are the facts
/// a row needs.
fn parse_worktrees(listed: &str) -> Vec<ListedWorktree> {
    let mut found: Vec<ListedWorktree> = Vec::new();
    for line in listed.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            found.push(ListedWorktree {
                path: PathBuf::from(path),
                branch: None,
            });
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            if let Some(last) = found.last_mut() {
                last.branch = Some(branch.to_string());
            }
        }
    }
    found
}

/// The worktree paths git itself registers for `repo` — the first line of
/// each `git worktree list --porcelain` stanza, main checkout included.
/// The adoption conflict check's ground truth (#29), and what a Thread's
/// `follow` is handed to learn where its agent may have moved.
pub fn worktree_paths(repo: &Path) -> Result<Vec<PathBuf>, GitError> {
    Ok(worktrees(repo)?
        .into_iter()
        .map(|worktree| worktree.path)
        .collect())
}

/// Read `root`'s repo for the workspace chip — two git calls, never on a
/// UI or pump thread; on a machine its Server reads it (`list_repo`, ADR
/// 0026). The main checkout is left out of the worktrees (it is the
/// `checked out` row already), as is any worktree whose directory is
/// gone: git still lists a prunable one, but it is no place to work. A
/// root git cannot answer for lists nothing, and the chip offers the
/// fresh starts alone.
pub fn repo_listing(root: &Path) -> RepoListing {
    let checkout = follow::canonical(root);
    let (main, worktrees): (Vec<_>, Vec<_>) = worktrees(root)
        .unwrap_or_default()
        .into_iter()
        .filter(|worktree| worktree.path.is_dir())
        .partition(|worktree| follow::canonical(&worktree.path) == checkout);
    RepoListing {
        checked_out: main.into_iter().find_map(|main| main.branch),
        worktrees,
        branches: branches(root).unwrap_or_default(),
    }
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
#[cfg(any(test, feature = "test-support"))]
pub fn git_for_tests(repo: &Path, args: &[&str]) -> String {
    git(repo, args).unwrap()
}

thread_local! {
    /// How many `git` processes this thread has started.
    static GIT_SPAWNS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// How many `git` processes the calling thread has started through this
/// module. A `git` call is milliseconds at best and seconds on a busy repo,
/// so the UI's own thread must start none: its tests hold it to zero across
/// a frame and a pump, and every read here belongs off that thread.
pub fn git_spawns_on_this_thread() -> u64 {
    GIT_SPAWNS.with(std::cell::Cell::get)
}

/// Run one git command against `repo`, answering its stdout.
///
/// Never with git's optional locks: the checkouts Ferrite reads are the
/// ones its agents are writing, and a read that refreshed the index on the
/// side would take `index.lock` from under the agent's own `git`.
pub(crate) fn git(repo: &Path, args: &[&str]) -> Result<String, GitError> {
    GIT_SPAWNS.with(|spawns| spawns.set(spawns.get() + 1));
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .no_console_window()
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
        git(&repo, &["config", "core.autocrlf", "false"]).unwrap();
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

    /// The porcelain listing read stanza by stanza: the path from the
    /// `worktree` line, the branch from `branch refs/heads/…`, none on a
    /// detached HEAD — and the other lines git prints passed over.
    #[test]
    fn the_worktree_listing_parses_paths_and_branches_per_stanza() {
        let listed = concat!(
            "worktree /srv/repo\n",
            "HEAD 0123456789abcdef0123456789abcdef01234567\n",
            "branch refs/heads/main\n",
            "\n",
            "worktree /srv/repo/.worktrees/feature\n",
            "HEAD 0123456789abcdef0123456789abcdef01234567\n",
            "branch refs/heads/feature/rows\n",
            "locked\n",
            "\n",
            "worktree /srv/elsewhere\n",
            "HEAD 0123456789abcdef0123456789abcdef01234567\n",
            "detached\n",
            "prunable gitdir file points to non-existent location\n",
            "\n",
        );
        assert_eq!(
            parse_worktrees(listed),
            vec![
                ListedWorktree {
                    path: PathBuf::from("/srv/repo"),
                    branch: Some("main".into()),
                },
                ListedWorktree {
                    path: PathBuf::from("/srv/repo/.worktrees/feature"),
                    branch: Some("feature/rows".into()),
                },
                ListedWorktree {
                    path: PathBuf::from("/srv/elsewhere"),
                    branch: None,
                },
            ]
        );
    }

    /// Against real git: the main checkout leads, every added worktree
    /// follows with its branch, and `worktree_paths` is the same listing
    /// with the branches dropped.
    #[test]
    fn worktrees_lists_the_main_checkout_and_every_added_tree_with_its_branch() {
        let root = scratch("listing");
        let repo = init_repo(&root);
        let tree = root.join("side");
        ensure_worktree(&repo, &tree, "feature/side").unwrap();

        let listed = worktrees(&repo).unwrap();
        assert_eq!(listed.len(), 2, "{listed:?}");
        assert_eq!(
            dunce::canonicalize(&listed[0].path).unwrap(),
            dunce::canonicalize(&repo).unwrap()
        );
        assert_eq!(listed[0].branch.as_deref(), Some("main"));
        assert_eq!(
            dunce::canonicalize(&listed[1].path).unwrap(),
            dunce::canonicalize(&tree).unwrap()
        );
        assert_eq!(listed[1].branch.as_deref(), Some("feature/side"));
        assert_eq!(
            worktree_paths(&repo).unwrap(),
            listed
                .into_iter()
                .map(|worktree| worktree.path)
                .collect::<Vec<_>>()
        );
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

    /// A worktree placed inside its repo (`.worktrees/`) leaves the main
    /// checkout clean: git learns to skip it from the repo's own
    /// `info/exclude`, once however many trees go there, and no tracked
    /// file (`.gitignore`) is touched.
    #[test]
    fn a_worktree_inside_its_repo_is_excluded_not_ignored() {
        let root = scratch("inside");
        let repo = init_repo(&root);
        let first = repo.join(WORKTREES).join("ferrite-wt-1");
        add_worktree(&repo, &first, "ferrite/wt-1", None).unwrap();
        ensure_worktree(&repo, &repo.join(WORKTREES).join("ferrite-wt-2"), "ferrite/wt-2")
            .unwrap();

        assert!(first.join(".git").exists());
        assert_eq!(git(&repo, &["status", "--porcelain"]).unwrap(), "");
        assert!(!repo.join(".gitignore").exists());
        let exclude = fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
        assert_eq!(
            exclude.lines().filter(|line| *line == "/.worktrees/").count(),
            1,
            "{exclude}"
        );
        // A worktree outside the repo writes nothing there.
        add_worktree(&repo, &root.join("elsewhere"), "elsewhere", None).unwrap();
        assert_eq!(fs::read_to_string(repo.join(".git/info/exclude")).unwrap(), exclude);
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
    fn the_default_branch_follows_origin_then_main_then_master() {
        assert_eq!(
            pick_default_branch(Some("origin/trunk\n"), |_| true),
            "trunk"
        );
        assert_eq!(pick_default_branch(Some("origin/main"), |_| false), "main");
        assert_eq!(pick_default_branch(None, |name| name == "main"), "main");
        assert_eq!(pick_default_branch(None, |name| name == "master"), "master");
        assert_eq!(pick_default_branch(Some(""), |_| true), "main");
        assert_eq!(pick_default_branch(None, |_| false), "main");
        let root = scratch("default-branch");
        let repo = init_repo(&root);
        assert_eq!(
            default_branch(&repo),
            "main",
            "no origin: the local main is the default"
        );
    }

    /// The Changes row's reading, from real git: a branch counts everything
    /// since it left its base — commits, edits not yet committed and
    /// untracked files — against the ref it compared (`origin/main` when a
    /// remote has it, else the local `main`); a detached HEAD counts its
    /// uncommitted work alone.
    #[test]
    fn branch_diff_counts_the_work_against_the_base_it_compared() {
        let commit = |tree: &Path, message: &str| {
            git(tree, &["add", "-A"]).unwrap();
            git(
                tree,
                &[
                    "-c",
                    "user.email=test@example.invalid",
                    "-c",
                    "user.name=test",
                    "commit",
                    "-qm",
                    message,
                ],
            )
            .unwrap();
        };
        let diff = |against: DiffBase, added: u32, removed: u32| {
            Some(BranchDiff {
                against,
                added,
                removed,
            })
        };
        let root = scratch("branch-diff");
        let origin = init_repo(&root);
        fs::write(origin.join("file.txt"), "one\ntwo\nthree\n").unwrap();
        commit(&origin, "three lines");
        let clone = root.join("clone");
        git(
            &root,
            &[
                "clone",
                "-q",
                "-c",
                "core.autocrlf=false",
                &origin.to_string_lossy(),
                &clone.to_string_lossy(),
            ],
        )
        .unwrap();
        git(&clone, &["checkout", "-q", "-b", "feat"]).unwrap();
        let vs_origin = || DiffBase::Branch("origin/main".into());

        assert_eq!(
            branch_diff(&clone, false),
            diff(vs_origin(), 0, 0),
            "no changes: the base is still named"
        );

        fs::write(clone.join("added.txt"), "a\nb\n").unwrap();
        commit(&clone, "insertions only");
        assert_eq!(branch_diff(&clone, false), diff(vs_origin(), 2, 0));

        fs::write(clone.join("file.txt"), "one\n").unwrap();
        commit(&clone, "deletions only");
        assert_eq!(branch_diff(&clone, false), diff(vs_origin(), 2, 2));

        fs::write(clone.join("file.txt"), "one\nuncommitted\n").unwrap();
        fs::write(clone.join("untracked.txt"), "u\n").unwrap();
        assert_eq!(
            branch_diff(&clone, false),
            diff(vs_origin(), 4, 2),
            "uncommitted edits and untracked files count with the branch"
        );

        git(&clone, &["checkout", "-q", "--detach"]).unwrap();
        assert_eq!(
            branch_diff(&clone, true),
            diff(DiffBase::Uncommitted, 2, 0),
            "detached: the uncommitted work against HEAD alone"
        );

        // No remote: the local default branch is the base.
        git(&origin, &["checkout", "-q", "-b", "local"]).unwrap();
        fs::write(origin.join("local.txt"), "l\n").unwrap();
        commit(&origin, "local");
        assert_eq!(
            branch_diff(&origin, false),
            diff(DiffBase::Branch("main".into()), 1, 0)
        );
    }

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

    /// Ferrite reads a checkout the agent is working in at the same moment,
    /// so reading it must never take the index lock: a status read that
    /// refreshed the index would race the agent's own `git add` into an
    /// `index.lock` refusal. A tracked file whose timestamp moved is what
    /// tempts a plain `git status` into rewriting the index.
    #[test]
    fn reading_a_checkouts_status_never_rewrites_its_index() {
        let root = scratch("optional-locks");
        let repo = init_repo(&root);
        let index = repo.join(".git").join("index");
        let before = fs::read(&index).unwrap();
        let touched = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
        fs::File::options()
            .write(true)
            .open(repo.join("file.txt"))
            .unwrap()
            .set_modified(touched)
            .unwrap();

        assert!(is_clean(&repo).unwrap(), "only the timestamp moved");
        assert_eq!(checkout_branch(&repo).as_deref(), Some("main"));

        assert!(
            fs::read(&index).unwrap() == before,
            "reading the checkout rewrote its index"
        );
    }

    /// A named worktree starts from the base it was given, on its own new
    /// branch; a branch that already exists is git's refusal, not a reuse.
    #[test]
    fn a_named_worktree_is_a_new_branch_from_its_base() {
        let root = scratch("named");
        let repo = init_repo(&root);
        let base = git(&repo, &["rev-parse", "HEAD"]).unwrap();
        fs::write(repo.join("file.txt"), "moved on\n").unwrap();
        git(
            &repo,
            &[
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "user.name=test",
                "commit",
                "-qam",
                "later",
            ],
        )
        .unwrap();
        let tree = root.join("tree");

        add_worktree(&repo, &tree, "feature/named", Some(base.trim())).unwrap();

        assert_eq!(branch_of(&tree), "feature/named");
        assert_eq!(fs::read_to_string(tree.join("file.txt")).unwrap(), "base\n");
        let again = add_worktree(&repo, &root.join("again"), "feature/named", None);
        assert!(again.is_err(), "an existing branch is never taken over");
    }

    /// A checkout's repository is read from its `.git` alone: the main
    /// checkout is its own, a linked worktree names the main checkout, and
    /// a plain directory is no checkout.
    #[test]
    fn a_checkout_names_its_repository_without_running_git() {
        let root = scratch("checkout-of");
        let repo = init_repo(&root);
        let tree = root.join("linked");
        ensure_worktree(&repo, &tree, "ferrite/linked").unwrap();

        assert_eq!(
            checkout_of(&repo),
            Some(Checkout {
                repo: repo.clone(),
                linked: false,
            })
        );
        assert_eq!(
            checkout_of(&tree),
            Some(Checkout {
                repo: dunce::canonicalize(&repo).unwrap(),
                linked: true,
            })
        );
        assert_eq!(checkout_of(&root), None);
    }
}
