//! An attempt's change as a page of its own: the whole diff as text, kept in the attempt's folder on the board and
//! served like a review's evidence, so each attempt in the task panel has a link to what it changed.
use crate::json::*;
use crate::store::files::artifacts_dir;
use crate::store::repos::git_slow;
use std::path::Path;

pub const DIFF_FILE: &str = "diff.txt";
/// What the attempt remembers ({ file, link }), or None when there is nothing to show.
pub fn keep_diff(t: &Obj, range: &str) -> Option<Value> {
    if !t.b("worktree") || range.is_empty() { return None; }
    let text = git_slow(Path::new(t.s("worktree")), &["diff", "--stat", "--patch", range], 120000).ok()?;
    if text.trim().is_empty() { return None; }
    let file = artifacts_dir(t.s("id")).join(DIFF_FILE);
    std::fs::create_dir_all(file.parent()?).ok()?;
    std::fs::write(&file, format!("Attempt {} of {}: {}\nBranch {}, {range}\n\n{text}\n", t.s("letter"), t.s("attemptOf"), t.s("title"), t.s("branch"))).ok()?;
    let root = crate::review_page::artifact_base(t.s("id"));
    Some(json!({ "file": file.display().to_string(), "link": if root.is_empty() { file.display().to_string() } else { format!("{root}/{DIFF_FILE}") } }))
}
