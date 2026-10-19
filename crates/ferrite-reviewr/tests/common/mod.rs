//! A real git repo for integration tests; each binary uses a subset.
#![allow(dead_code, unreachable_pub)]

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

pub struct Repo {
    dir: TempDir,
}

impl Repo {
    /// A fresh repo on branch `main` with an identity configured.
    pub fn init() -> Self {
        let repo = Self { dir: TempDir::new().expect("tempdir") };
        repo.git(&["init", "-q", "-b", "main"]);
        // Pin `init.defaultBranch` so the machine's global config never steers a test.
        repo.git(&["config", "init.defaultBranch", "no-such-default"]);
        repo
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn path_buf(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    /// [`Self::git`] with extra environment, such as a pinned committer date.
    pub fn git_env(&self, args: &[&str], env: &[(&str, &str)]) -> String {
        let out = Command::new("git")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@herdr.test")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@herdr.test")
            .envs(env.iter().copied())
            .arg("-C")
            .arg(self.path())
            .args(args)
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Run `git -C <repo> <args>`, asserting success, returning stdout.
    pub fn git(&self, args: &[&str]) -> String {
        self.git_env(args, &[])
    }

    /// Fake `origin/<name>` at `rev` as the remote default, with no real remote.
    pub fn set_origin_default(&self, name: &str, rev: &str) {
        let oid = self.git(&["rev-parse", rev]).trim().to_string();
        self.git(&["update-ref", &format!("refs/remotes/origin/{name}"), &oid]);
        self.git(&[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            &format!("refs/remotes/origin/{name}"),
        ]);
    }

    /// Record `content` as a blob and point `git_ref` at it, bypassing `write_base_pick`.
    fn plant_blob(&self, git_ref: &str, content: &str) {
        let path = self.path().join("plant-blob");
        std::fs::write(&path, content).unwrap();
        let blob = self.git(&["hash-object", "-w", path.to_str().unwrap()]).trim().to_string();
        self.git(&["update-ref", git_ref, &blob]);
        std::fs::remove_file(&path).unwrap();
    }

    /// Write `content` to the pick ref verbatim, as only a foreign writer could.
    pub fn write_raw_base_pick(&self, content: &str) {
        self.plant_blob("refs/worktree/reviewr/base-pick", content);
    }

    /// A leftover clone-wide pick from before the worktree-private cutover.
    pub fn plant_legacy_base_pick(&self, content: &str) {
        self.plant_blob("refs/reviewr/base-pick", content);
    }

    /// The path-hashed last-turn ref an old binary left, before refs went worktree-private.
    pub fn plant_legacy_turn_base(&self, sha: &str) {
        let key = legacy_worktree_key(self.path());
        self.git(&["update-ref", &format!("refs/reviewr/turn-base/{key}"), sha]);
    }

    /// A linked worktree of this clone on a new branch. Lives as long as the returned value.
    pub fn add_worktree(&self, branch: &str) -> LinkedWorktree {
        let keep = TempDir::new().expect("tempdir");
        let path = keep.path().join("wt");
        self.git(&["worktree", "add", "-q", "-b", branch, path.to_str().unwrap()]);
        LinkedWorktree { _keep: keep, path }
    }

    pub fn write(&self, rel: &str, contents: &str) {
        let path = self.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, contents).expect("write");
    }

    pub fn remove(&self, rel: &str) {
        std::fs::remove_file(self.path().join(rel)).expect("remove");
    }

    /// Stage everything and commit.
    pub fn commit_all(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }
}

/// A linked worktree created by [`Repo::add_worktree`]. The directory is deleted when dropped.
pub struct LinkedWorktree {
    _keep: TempDir,
    path: PathBuf,
}

impl LinkedWorktree {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn legacy_worktree_key(repo: &Path) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in repo.to_string_lossy().bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// A minimal open-PR snapshot to override per test: `PrSnapshot { .., ..pr_snapshot() }`.
pub fn pr_snapshot() -> ferrite_reviewr::forge::PrSnapshot {
    use ferrite_reviewr::forge::{Merge, PrSnapshot, PrState, Sync};
    PrSnapshot {
        number: 1,
        title: "t".into(),
        body: String::new(),
        url: "u".into(),
        state: PrState::Open,
        is_draft: false,
        head_ref: "feature".into(),
        head_is_fork: false,
        head_oid: String::new(),
        base_ref: "main".into(),
        merge: Merge::Clean,
        sync: Sync::InSync,
        checks: Vec::new(),
        comments: Vec::new(),
        comments_truncated: false,
        checks_truncated: false,
    }
}

/// A minimal PR comment to override per test.
pub fn comment() -> ferrite_reviewr::forge::Comment {
    use ferrite_reviewr::forge::{Comment, CommentKind};
    Comment {
        kind: CommentKind::Comment,
        author: "ann".into(),
        author_is_bot: false,
        anchor: "comment".into(),
        place: None,
        body: "b".into(),
        snippet: None,
        created_at: "2026-06-27T10:00:00Z".into(),
        is_resolved: false,
        is_outdated: false,
        replies: Vec::new(),
    }
}
