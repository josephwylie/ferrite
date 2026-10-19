//! Ferrite: explicit repair of a stopped, already-merged delivery. This never runs the finish pipeline,
//! starts an agent, changes review gates, or removes a workspace. All evidence is retained.
use crate::clock::now;
use crate::err::Res;
use crate::flow;
use crate::gate;
use crate::json::*;
use crate::runner::runner_alive;
use crate::store::files::dir_of;
use crate::store::inbox::pending_text;
use crate::store::marks::pending_events;
use crate::store::repos::git_out;
use crate::store::reviews::review_history;
use crate::store::tasks::{assert_unarchived, find, update, Change, Task};
use std::path::Path;

fn sha(s: &str) -> bool { s.len() == 40 && s.bytes().all(|c| c.is_ascii_hexdigit()) }
fn refuse_busy(t: &Task) -> Res<()> {
    assert_unarchived(t)?;
    if !["stopped", "done"].contains(&t.s("stage")) || t.b("turnOpen") || runner_alive(t)
        || t.b("autoMerge") || t.b("mergeIntent") || (t.b("tidy") && t.s("stage") != "stopped")
        || !t.arr("attempts").is_empty() || t.b("attemptOf")
        || !pending_events(t).is_empty() || !pending_text(t.s("id")).is_empty() {
        crate::fail!("Stop all work first: reconciliation requires a stopped or done task with no active runner, turn, cleanup, merge or pending messages/events.");
    }
    Ok(())
}
/// Unknown/unreachable sessions are not proof that a worker or reviewer has stopped.
fn quiet(t: &Task) -> Res<()> {
    let socket = t.at("link").s("socket");
    let session = flow::live::read_session(socket);
    let flow::live::Session::Up { agents, panes } = session else { crate::fail!("Cannot verify that the task's worker and reviewer are at rest: session unavailable."); };
    let api = flow::task::h(t);
    for pane in [t.at("link").s("pane"), t.at("link").at("reviewer").s("pane")] {
        if pane.is_empty() { continue; }
        if let Some(a) = agents.get(pane) {
            if !["idle", "done"].contains(&a.s("agent_status")) { crate::fail!("Task pane {pane} is active or its state is unknown."); }
        }
        if panes.contains_key(pane) && !api.pane_childless(pane) { crate::fail!("Task pane {pane} still has running work."); }
    }
    Ok(())
}
fn gh_json(root: &Path, args: &[&str]) -> Res<Value> {
    let r = flow::git::gh(args, root, 60000);
    if r.status != 0 { crate::fail!("Cannot verify GitHub evidence: {}", r.stderr.trim()); }
    Ok(serde_json::from_str(&r.stdout)?)
}
fn origin_repo(root: &Path) -> Res<String> {
    let remote = git_out(root, &["config", "--get", "remote.origin.url"])?;
    let repo = remote.strip_prefix("https://github.com/").or_else(|| remote.strip_prefix("git@github.com:"))
        .or_else(|| remote.strip_prefix("ssh://git@github.com/")).unwrap_or("").trim_end_matches(".git");
    let parts: Vec<_> = repo.split('/').collect();
    if parts.len() != 2 || parts.iter().any(|s| s.is_empty() || !s.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))) {
        crate::fail!("Reconciliation requires an unambiguous GitHub origin repository.");
    }
    Ok(repo.into())
}
fn review_of(t: &Task, head: &str, merged_at: i64) -> Res<Value> {
    let history = review_history(t);
    // A later review of different work, or a later failure, is not an abandoned cleanup-only round.
    let Some(last) = history.last() else { crate::fail!("No original review history exists."); };
    if last.s("head") != head || !gate::passes(last) || !gate::is_tagged(last)
        || !gate::split(last, None).required.is_empty() || !last.b("meets")
        || last.i("at") <= 0 || last.i("at") > merged_at {
        crate::fail!("The latest recorded review must be the original passing review of the exact merged PR head, before its merge.");
    }
    let current = t.at("review");
    if current.is_object() && (current.s("head") != head || !gate::passes(current)
        || ["meets", "findings", "gate", "checks", "limitations", "text"].iter().any(|k| current.at(k) != last.at(k))) {
        crate::fail!("Current review conflicts with the original merged delivery.");
    }
    let mut review = if current.is_object() { current.clone() } else { last.clone() };
    if let Some(o) = review.as_object_mut() { o.remove("stale"); }
    Ok(review)
}
fn local_head(t: &Task, root: &Path, merged: &str, reviewed: &str, base_head: &str) -> Res<String> {
    let tree = Path::new(t.s("worktree"));
    if t.s("worktree").is_empty() || !tree.is_dir() { crate::fail!("The task's worktree must exist so its branch and local work can be verified."); }
    let common = |p: &Path| -> Res<std::path::PathBuf> { Ok(std::fs::canonicalize(git_out(p, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?)?) };
    if common(tree)? != common(root)? || git_out(tree, &["symbolic-ref", "--short", "HEAD"])? != t.s("branch") {
        crate::fail!("Task worktree/repository/branch association does not match.");
    }
    if !git_out(tree, &["status", "--porcelain=v1", "--untracked-files=all", "--ignore-submodules=none"])?.is_empty() {
        crate::fail!("Task worktree has dirty or untracked work; nothing was reconciled.");
    }
    let head = git_out(tree, &["rev-parse", "HEAD"])?;
    if t.b("tidy") && (t.at("tidy").s("tip") != head || !["merged", "always"].contains(&t.at("tidy").s("deleteBranch"))) {
        crate::fail!("Stopped cleanup marker does not match the verified local delivery.");
    }
    git_out(root, &["merge-base", "--is-ancestor", merged, base_head])?;
    // A squash/rebase merge need not contain the original reviewed commit. Only that exact
    // reviewed commit with the identical merged tree is accepted as the exception.
    let original_tree = head == reviewed && git_out(root, &["rev-parse", &format!("{head}^{{tree}}")])?
        == git_out(root, &["rev-parse", &format!("{merged}^{{tree}}")])?;
    if !original_tree && git_out(root, &["merge-base", "--is-ancestor", &head, base_head]).is_err() {
        crate::fail!("Task branch contains new unmerged commits; nothing was reconciled.");
    }
    // A reset to unrelated old work is not a cleanup-only reopening.
    if head != reviewed && git_out(root, &["merge-base", "--is-ancestor", merged, &head]).is_err() {
        crate::fail!("Task branch does not contain the merged result.");
    }
    Ok(head)
}

/// The keeper refreshes the read timestamp even when the same finished turn stands still.
/// Only that timestamp may move; status, terminal, turn sequences and every other task field
/// must still match. The update preserves the newest observation instead of writing the snapshot back.
fn same_task(a: &Task, b: &Task) -> bool {
    let normalize = |t: &Task| {
        let mut t = t.clone();
        if t.at("last").is_object() { t.obj_mut("last").del("at"); }
        t
    };
    normalize(a) == normalize(b)
}

/// `check` performs every validation without changing the task. A real repair writes an exclusive
/// evidence file before the compare-and-swap under the normal task lock. No history is rewritten.
pub fn reconcile(id: &str, pr: &str, reviewed: &str, check: bool) -> Res<Value> {
    let before = find(id)?;
    refuse_busy(&before)?;
    if !sha(reviewed) || before.s("branch").is_empty() { crate::fail!("Supply the full original --review-head SHA; the task must retain its exact branch."); }
    let root = Path::new(before.s("root"));
    let repo = origin_repo(root)?;
    let prefix = format!("https://github.com/{repo}/pull/");
    let number = pr.strip_prefix(&prefix).unwrap_or("");
    if number.is_empty() || !number.bytes().all(|c| c.is_ascii_digit()) || number.starts_with('0') {
        crate::fail!("--pr must be the exact pull request URL in the task's origin repository.");
    }
    if before.b("pr") && before.s("pr") != pr { crate::fail!("Task already names a different pull request."); }
    quiet(&before)?;
    let github = gh_json(root, &["pr", "view", pr, "--repo", &repo, "--json", "url,state,headRefName,headRefOid,baseRefName,mergeCommit,mergedAt,isCrossRepository"])?;
    let merged = github.at("mergeCommit").s("oid");
    let base = github.s("baseRefName");
    if github.s("url") != pr || github.s("state") != "MERGED" || github.at("isCrossRepository") != &json!(false)
        || github.s("headRefName") != before.s("branch") || github.s("headRefOid") != reviewed || !sha(merged)
        || base.is_empty() || git_out(root, &["check-ref-format", &format!("refs/heads/{base}")]).is_err() {
        crate::fail!("GitHub must confirm MERGED, exact task branch and reviewed head, and a same-repository PR with a merge commit/base.");
    }
    let merged_at = chrono::DateTime::parse_from_rfc3339(github.s("mergedAt")).map_err(|_| crate::err::Error::new("Invalid GitHub merge time"))?.timestamp_millis();
    if before.b("mergeCommit") && before.s("mergeCommit") != merged { crate::fail!("Saved merge commit conflicts with GitHub."); }
    let review = review_of(&before, reviewed, merged_at)?;
    // Fetch the exact remote base snapshot, without changing a checkout, branch, or the
    // shared FETCH_HEAD (another task may be fetching concurrently in this repository).
    let base_ref = format!("refs/heads/{base}");
    let remote = git_out(root, &["ls-remote", "--exit-code", "--refs", "origin", &base_ref])?;
    let fields: Vec<_> = remote.split_whitespace().collect();
    if fields.len() != 2 || !sha(fields[0]) || fields[1] != base_ref { crate::fail!("Cannot establish the exact remote PR base head."); }
    let base_head = fields[0].to_string();
    git_out(root, &["fetch", "--no-tags", "--no-write-fetch-head", "origin", &base_head])?;
    let head = local_head(&before, root, merged, reviewed, &base_head)?;
    quiet(&before)?;
    let evidence = json!({ "task": before, "github": github, "localHead": head, "baseHead": base_head, "reviewHead": reviewed, "checkedAt": now() });
    if check {
        update(before.s("id"), |t| {
            if !same_task(t, &before) { crate::fail!("Task changed during verification; retry after it is at rest."); }
            refuse_busy(t)?;
            if local_head(t, root, merged, reviewed, &base_head)? != head { crate::fail!("Branch moved during verification."); }
            Ok(Change::Keep)
        })?;
        return Ok(json!({ "id": before.s("id"), "verified": true, "changed": false, "pr": pr, "head": head, "reviewHead": reviewed, "mergeCommit": merged }));
    }
    let dir = dir_of(before.s("id")).join("reconciliation");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(format!("{}-{}.json", now(), std::process::id()));
    use std::os::unix::fs::OpenOptionsExt;
    let mut out = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&file)?;
    use std::io::Write;
    out.write_all(serde_json::to_string_pretty(&evidence)?.as_bytes())?;
    out.sync_all()?;
    let restored = update(before.s("id"), |t| {
        if !same_task(t, &before) { crate::fail!("Task changed during verification; retry after it is at rest."); }
        refuse_busy(t)?;
        if local_head(t, root, merged, reviewed, &base_head)? != head { crate::fail!("Branch moved during verification."); }
        t.put("stage", "done"); t.put("pr", pr); t.put("prMerged", true); t.put("prState", "merged");
        t.put("prHead", reviewed); t.put("reviewedHead", reviewed); t.put("review", review.clone());
        t.put("doneWhy", format!("Already merged in #{number}"));
        t.put("mergeCommit", merged); t.put("mergedAt", merged_at); t.put("msg", format!("Merged: {pr} (original delivery reconciled)"));
        t.put("reconciled", json!({ "at": now(), "evidence": file, "head": head, "pr": pr, "reviewHead": reviewed }));
        for k in ["tidy", "failure", "prFailure", "waiting", "watch", "selfBlocked", "blocked", "expectedWake"] { t.put(k, Value::Null); }
        // Keep worktree, history, reviewRound and all source work. Cleanup is a separate normal operation.
        Ok(Change::Write)
    })?;
    Ok(json!({ "id": restored.s("id"), "verified": true, "changed": true, "stage": "done", "pr": pr, "review": review, "mergeCommit": merged, "evidence": file }))
}
