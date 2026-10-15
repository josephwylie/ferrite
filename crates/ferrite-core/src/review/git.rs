//! The git scopes of a review, read where the checkout is: by the Server
//! that owns the Thread (`review_diff`, `list_commits`, ADR 0021), or by
//! this process when it runs its agents itself. A diff is git's unified
//! text, bounded by `MAX_DIFF`, which the window parses into file edits.

use std::path::Path;
use std::process::Command;

use crate::spawn::NoConsoleWindow;
use crate::{FileEdit, Hunk};

/// The most diff text a review reads: past it the review says so rather
/// than drawing (or carrying over a link) a diff no one can read.
pub const MAX_DIFF: usize = 8 << 20;

/// The most commits the commits scope lists, newest first.
pub const MAX_COMMITS: usize = 500;

/// What git reads for a review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Diff {
    /// The checkout against its last commit, untracked files included.
    Uncommitted,
    /// The branch against the repo's default branch.
    Branch,
    /// One commit, by its full id.
    Commit(String),
}

/// One of the branch's commits, as the commits scope lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub id: String,
    pub subject: String,
}

/// `diff` in `workspace`, or why it cannot be read. The branch is read
/// against the repo's default branch (`workspace::default_branch`).
pub fn read(workspace: &Path, diff: &Diff) -> Result<String, String> {
    repo(workspace)?;
    match diff {
        Diff::Uncommitted => uncommitted(workspace),
        Diff::Branch => branch(workspace, &crate::workspace::default_branch(workspace)),
        Diff::Commit(id) => commit(workspace, id),
    }
}

/// The branch's commits since the repo's default branch, newest first (at
/// most `MAX_COMMITS`), against `origin/<base>` when only the remote has
/// the branch.
pub fn commits(workspace: &Path) -> Result<Vec<Commit>, String> {
    repo(workspace)?;
    let base = crate::workspace::default_branch(workspace);
    let log = |base: &str| {
        git(
            workspace,
            &[
                "log",
                "--no-color",
                &format!("--max-count={MAX_COMMITS}"),
                "--format=%H%x1f%s",
                &format!("{base}..HEAD"),
            ],
        )
    };
    let log = log(&base).or_else(|error| log(&format!("origin/{base}")).map_err(|_| error))?;
    Ok(log
        .lines()
        .filter_map(|line| line.split_once('\x1f'))
        .map(|(id, subject)| Commit {
            id: id.to_string(),
            subject: subject.to_string(),
        })
        .collect())
}

/// Whether `workspace` is in a repo, saying so in git's words when not:
/// `git diff` outside one would only suggest `--no-index`.
fn repo(workspace: &Path) -> Result<(), String> {
    git(workspace, &["rev-parse", "--git-dir"]).map(drop)
}

/// `git diff --no-color base...HEAD` in `workspace`, against
/// `origin/<base>` when only the remote has the branch.
fn branch(workspace: &Path, base: &str) -> Result<String, String> {
    let diff = |base: &str| {
        git(
            workspace,
            &["diff", "--no-color", &format!("{base}...HEAD")],
        )
    };
    let diff = diff(base).or_else(|error| diff(&format!("origin/{base}")).map_err(|_| error))?;
    bounded(diff)
}

/// What commit `id` changed — a merge against its first parent. Only a
/// commit id is read, never an option or a range.
fn commit(workspace: &Path, id: &str) -> Result<String, String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("{id:?} is not a commit id"));
    }
    bounded(git(
        workspace,
        &[
            "show",
            "--no-color",
            "--format=",
            "--diff-merges=first-parent",
            id,
        ],
    )?)
}

/// The checkout against its last commit, staged or not, and each untracked
/// file as all-new (`git diff --no-index /dev/null <file>`), stopping as
/// soon as the text passes `MAX_DIFF`.
fn uncommitted(workspace: &Path) -> Result<String, String> {
    let mut diff = bounded(git(workspace, &["diff", "--no-color", "HEAD"])?)?;
    let untracked = git(
        workspace,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    for path in untracked.split('\0').filter(|path| !path.is_empty()) {
        diff.push_str(&git(
            workspace,
            &["diff", "--no-color", "--no-index", "--", "/dev/null", path],
        )?);
        diff = bounded(diff)?;
    }
    Ok(diff)
}

fn bounded(diff: String) -> Result<String, String> {
    if diff.len() > MAX_DIFF {
        return Err(format!(
            "more than {} MB of changes: too large to review here",
            MAX_DIFF >> 20
        ));
    }
    Ok(diff)
}

/// What a git read in `workspace` printed, or the first line of why git
/// refused. Exit 1 still counts: `diff --no-index` says "they differ" so.
fn git(workspace: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(args)
        // A read, never a lock on the index an agent may be writing.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .no_console_window()
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => "git is not installed".to_string(),
            _ => format!("git: {error}"),
        })?;
    if matches!(out.status.code(), Some(0 | 1)) {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    Err(stderr
        .lines()
        .next()
        .unwrap_or("git failed")
        .trim()
        .to_string())
}

/// git's multi-file unified diff into file edits: one per `diff --git`
/// section (named by its `+++ b/` path, else its `--- a/` path for a
/// deletion), its hunks as git printed them. A file with no hunks (a
/// binary one) is left out.
pub fn parse(diff: &str) -> Vec<FileEdit> {
    let mut edits: Vec<FileEdit> = Vec::new();
    let mut old_path: Option<String> = None;
    let mut hunk: Option<Hunk> = None;
    let flush = |edits: &mut Vec<FileEdit>, hunk: &mut Option<Hunk>| {
        if let (Some(done), Some(edit)) = (hunk.take(), edits.last_mut()) {
            edit.hunks.push(done);
        }
    };
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            flush(&mut edits, &mut hunk);
            old_path = None;
            continue;
        }
        // git ends a path holding a space with a tab on these two lines.
        if let Some(path) = line
            .strip_prefix("--- ")
            .map(|path| path.trim_end_matches('\t'))
        {
            if hunk.is_none() {
                old_path = path.strip_prefix("a/").map(str::to_string);
                continue;
            }
        }
        if let Some(path) = line
            .strip_prefix("+++ ")
            .map(|path| path.trim_end_matches('\t'))
        {
            if hunk.is_none() {
                flush(&mut edits, &mut hunk);
                let path = path
                    .strip_prefix("b/")
                    .map(str::to_string)
                    .or_else(|| old_path.clone())
                    .unwrap_or_else(|| path.to_string());
                edits.push(FileEdit {
                    path,
                    hunks: Vec::new(),
                });
                continue;
            }
        }
        if let Some((old_start, old_lines, new_start, new_lines, section)) = hunk_header(line) {
            flush(&mut edits, &mut hunk);
            hunk = Some(Hunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                section,
                lines: Vec::new(),
            });
            continue;
        }
        if let Some(open) = hunk.as_mut() {
            if matches!(line.as_bytes().first(), Some(b' ' | b'+' | b'-')) {
                open.lines.push(line.to_string());
            }
        }
    }
    flush(&mut edits, &mut hunk);
    edits.retain(|edit| !edit.hunks.is_empty());
    edits
}

/// A hunk header's ranges and the section git names after it:
/// `@@ -88,6 +88,9 @@ fn draw` → (88, 6, 88, 9, `fn draw`).
fn hunk_header(line: &str) -> Option<(u32, u32, u32, u32, Option<String>)> {
    let (old, rest) = line.strip_prefix("@@ -")?.split_once(" +")?;
    let (new, section) = rest.split_once(" @@")?;
    let section = Some(section.trim())
        .filter(|section| !section.is_empty())
        .map(str::to_string);
    let range = |range: &str| -> Option<(u32, u32)> {
        let (start, count) = range.split_once(',').unwrap_or((range, "1"));
        Some((start.parse().ok()?, count.parse().ok()?))
    };
    let (old_start, old_lines) = range(old)?;
    let (new_start, new_lines) = range(new)?;
    Some((old_start, old_lines, new_start, new_lines, section))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "\
diff --git a/crates/ferrite/src/nav.rs b/crates/ferrite/src/nav.rs
index 1111111..2222222 100644
--- a/crates/ferrite/src/nav.rs
+++ b/crates/ferrite/src/nav.rs
@@ -208,7 +208,8 @@ fn thread_row
     let facts = self.facts.get(&t.id);
-    let mut row = div();
+    let live = t.session.is_some();
+    let mut row = div().h(px(ROW_H));
     row
diff --git a/old.rs b/old.rs
deleted file mode 100644
--- a/old.rs
+++ /dev/null
@@ -1,2 +0,0 @@
-fn gone() {}
-
";

    /// A scratch repo named `name`, one commit on `main`.
    fn repo(name: &str) -> (std::path::PathBuf, impl Fn(&[&str])) {
        let dir = std::env::temp_dir().join(format!("ferrite-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let at = dir.clone();
        let git = move |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&at)
                .output()
                .expect("git");
            assert!(out.status.success(), "git {args:?}: {out:?}");
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "operator@example.invalid"]);
        git(&["config", "user.name", "operator"]);
        std::fs::write(dir.join("kept.rs"), "one\n").unwrap();
        std::fs::write(dir.join(".gitignore"), "target\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "root"]);
        (dir, git)
    }

    /// git's multi-file diff parses into one edit per file — a deletion
    /// named by its old path.
    #[test]
    fn git_diff_parses_into_file_edits() {
        let edits = parse(DIFF);
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].path, "crates/ferrite/src/nav.rs");
        assert_eq!(edits[0].hunks.len(), 1);
        let hunk = &edits[0].hunks[0];
        assert_eq!(
            (
                hunk.old_start,
                hunk.old_lines,
                hunk.new_start,
                hunk.new_lines
            ),
            (208, 7, 208, 8)
        );
        assert_eq!(hunk.lines.len(), 5);
        assert_eq!(edits[1].path, "old.rs");
        assert!(parse("").is_empty());
    }

    /// The uncommitted scope holds edits to tracked files and every
    /// untracked file as all-new; ignored files stay out.
    #[test]
    fn uncommitted_includes_untracked_files() {
        let (dir, _git) = repo("review-uncommitted");
        std::fs::write(dir.join("kept.rs"), "two\n").unwrap();
        std::fs::write(dir.join("new file.rs"), "fresh\n").unwrap();
        std::fs::write(dir.join("target"), "built\n").unwrap();
        let edits = parse(&read(&dir, &Diff::Uncommitted).unwrap());
        let files: Vec<(&str, &[String])> = edits
            .iter()
            .map(|edit| (edit.path.as_str(), edit.hunks[0].lines.as_slice()))
            .collect();
        assert_eq!(
            files,
            [
                ("kept.rs", &["-one".to_string(), "+two".to_string()][..]),
                ("new file.rs", &["+fresh".to_string()][..]),
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The branch scope reads against the repo's default branch, falls
    /// back to the remote's when the checkout has no local one, and says
    /// why when neither resolves.
    #[test]
    fn the_branch_reads_against_the_remote_base_when_only_it_exists() {
        let (dir, git) = repo("review-branch");
        git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        git(&["checkout", "-q", "-b", "dev"]);
        git(&["branch", "-q", "-D", "main"]);
        std::fs::write(dir.join("kept.rs"), "two\n").unwrap();
        git(&["commit", "-q", "-am", "two"]);
        let edits = parse(&read(&dir, &Diff::Branch).unwrap());
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].hunks[0].lines, ["-one", "+two"]);
        assert!(branch(&dir, "trunk").is_err_and(|error| !error.is_empty()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The commits scope lists the branch's own commits, newest first, and
    /// reads each one's diff alone; anything but a commit id is refused.
    #[test]
    fn the_branchs_commits_list_newest_first_and_read_one_at_a_time() {
        let (dir, git) = repo("review-commits");
        git(&["checkout", "-q", "-b", "dev"]);
        std::fs::write(dir.join("kept.rs"), "two\n").unwrap();
        git(&["commit", "-q", "-am", "two"]);
        std::fs::write(dir.join("kept.rs"), "three\n").unwrap();
        git(&["commit", "-q", "-am", "three"]);
        let listed = commits(&dir).unwrap();
        let subjects: Vec<&str> = listed.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["three", "two"]);
        let edits = parse(&read(&dir, &Diff::Commit(listed[1].id.clone())).unwrap());
        assert_eq!(edits[0].hunks[0].lines, ["-one", "+two"]);
        assert!(read(&dir, &Diff::Commit("--all".into())).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A checkout that is no repo says git's reason; a diff past
    /// `MAX_DIFF` says it is too large rather than arriving.
    #[test]
    fn what_cannot_be_reviewed_says_why() {
        let elsewhere =
            std::env::temp_dir().join(format!("ferrite-no-repo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&elsewhere);
        std::fs::create_dir_all(&elsewhere).unwrap();
        let refused = read(&elsewhere, &Diff::Uncommitted).unwrap_err();
        assert!(refused.contains("not a git repository"), "{refused}");
        std::fs::remove_dir_all(&elsewhere).unwrap();

        let (dir, _git) = repo("review-large");
        std::fs::write(dir.join("huge.txt"), "line\n".repeat(MAX_DIFF / 4)).unwrap();
        let refused = read(&dir, &Diff::Uncommitted).unwrap_err();
        assert!(refused.contains("too large"), "{refused}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
