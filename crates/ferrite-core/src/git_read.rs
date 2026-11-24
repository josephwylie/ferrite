//! A checkout as git reads it, for a window and a Server alike: its
//! branch, its drift and work against its base, the repo's default
//! branch, branches and worktrees, and the stamp that says whether any
//! of it moved. Each read runs the operator's own `git` without its
//! optional locks, never on a UI thread. Making and removing worktrees
//! is the Server's (`workspace`).

use crate::checkout::{BranchDiff, BranchStatus, DiffBase, ListedWorktree, RepoListing};
use crate::spawn::NoConsoleWindow;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

mod stamp;

pub use stamp::{checkout_stamp, head_of};

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
    let checkout = canonical(root);
    let (main, worktrees): (Vec<_>, Vec<_>) = worktrees(root)
        .unwrap_or_default()
        .into_iter()
        .filter(|worktree| worktree.path.is_dir())
        .partition(|worktree| canonical(&worktree.path) == checkout);
    RepoListing {
        checked_out: main.into_iter().find_map(|main| main.branch),
        worktrees,
        branches: branches(root).unwrap_or_default(),
    }
}

/// The path as git would print it: symlinks resolved. A path that does
/// not exist yet resolves through its nearest standing ancestor, so a file
/// about to be written still names its checkout.
pub fn canonical(path: &Path) -> PathBuf {
    if let Ok(resolved) = dunce::canonicalize(path) {
        return resolved;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(leaf)) => canonical(parent).join(leaf),
        _ => path.to_path_buf(),
    }
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
pub fn git(repo: &Path, args: &[&str]) -> Result<String, GitError> {
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
}
