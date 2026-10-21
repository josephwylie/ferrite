//! A pull request merges only at the commit that was reviewed (a commit pushed from elsewhere stops it, also the
//! remembered merge), and one that GitHub only queued is not taken for merged until GitHub says so, or closed.
use crate::common;
use common::*;
use serde_json::{json, Value};
use std::path::Path;
use sw::actions::finish_line;
use sw::housekeeping::poll_prs;
use sw::json::*;

fn pr_world() -> World { world_with(Opts { repo: true, origin: true, settings: json!({ "review.when": "never" }), ..Default::default() }) }
fn view(head: &str, state: &str) -> Value {
    let check = if state == "PENDING" { json!({ "name": "build", "state": state }) } else { json!({ "name": "build", "conclusion": state }) };
    json!({ "state": "OPEN", "headRefOid": head, "statusCheckRollup": [check], "comments": [], "reviews": [], "author": { "login": "me" } })
}
fn plain(head: &str, state: &str) -> Value { json!({ "state": state, "headRefOid": head, "statusCheckRollup": [], "comments": [], "reviews": [], "author": { "login": "me" } }) }
fn in_sidebar(w: &World, t: &sw::json::Obj) -> bool { w.state().arr("workspaces").iter().any(|x| x.s("workspace_id") == t.at("link").s("workspace")) }

#[test]
fn a_pull_request_only_merges_at_the_commit_that_was_reviewed_a_commit_pushed_from_elsewhere_stops_it_also_the_auto_merge() {
    let w = pr_world();
    let id = to_approval(&w, "With a PR");
    let t = task(&id);
    let reviewed = git(Path::new(t.s("worktree")), &["rev-parse", "HEAD"]);
    let other = "1".repeat(40);
    let branch = t.t("branch");
    // GitHub says the pull request's head is another commit, and so does the branch at the remote: said plainly, nothing merges
    let pushed = |sha: &str| { git(&w.root, &["push", "-q", "--force", "origin", &format!("{sha}:refs/heads/{branch}")]); };
    w.gh_says(json!({ "head": other, "view": { "state": "OPEN", "headRefOid": other } }));
    pushed("main");
    finish_line::approve(&id, None, false).unwrap();
    let t = settle(&id);
    pushed(&reviewed);
    assert_eq!(t.s("stage"), "approval"); assert_eq!(t.at("mergeFailed"), &json!(true)); assert!(!t.b("autoMerge"));
    assert_eq!(t.at("failure").s("title"), "Pull request has commits that were not reviewed"); assert!(t.at("failure").s("detail").contains(&other));
    assert!(!gh_log(&w).contains("pr merge"));
    // it arrives between that look and the merge: GitHub itself refuses (the look itself is first answered late, with a
    // commit the branch at the remote does not stand at: asked again, not taken for a commit nobody reviewed)
    w.gh_says(json!({ "head": other, "late": { "head": other, "times": 1 } }));
    let looks = || gh_log(&w).lines().filter(|l| *l == "pr view 7 --json headRefOid").count();
    let looked = looks();
    finish_line::approve(&id, None, false).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval"); assert!(!t.b("autoMerge")); assert_eq!(t.at("failure").s("title"), "Pull request could not merge");
    assert!(t.at("failure").s("detail").contains("Head branch was modified"), "{}", t.at("failure"));
    assert_eq!(looks(), looked + 2);
    assert!(gh_log(&w).contains(&format!("pr merge 7 --squash --match-head-commit {reviewed}\n")), "{}", gh_log(&w));
    assert!(!regex::Regex::new("--auto|--delete-branch").unwrap().is_match(&gh_log(&w)));
    assert!(Path::new(t.s("worktree")).exists()); assert!(!t.b("tidy"));
    // checks still running: remembered. Someone pushes to the pull request meanwhile: the approval is withdrawn
    w.gh_says(json!({ "head": reviewed, "mergeFail": "checks pending", "view": view(&reviewed, "PENDING") }));
    finish_line::approve(&id, None, false).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "merging"); assert_eq!(t.at("autoMerge"), &json!(true));
    // GitHub naming another commit while the branch at the remote is still the reviewed one is only a late answer (it
    // was asked right after the board's own push): nothing is withdrawn, the next poll asks again
    w.gh_says(json!({ "head": other, "view": view(&other, "SUCCESS") }));
    poll_prs(); settle(&id);
    let t = task(&id);
    assert_eq!((t.t("stage"), t.at("autoMerge").clone(), t.b("failure")), ("merging".to_string(), json!(true), false));
    pushed("main"); // someone really pushed to it
    poll_prs(); settle(&id);
    let t = task(&id);
    assert_eq!(t.s("stage"), "approval"); assert!(!t.b("autoMerge") && !t.b("mergeIntent")); assert_eq!(t.at("failure").s("title"), "Pull request has commits that were not reviewed");
    pushed(&reviewed);
    // remembered again, the checks pass on the reviewed commit: the board merges it itself, the remote branch goes, clean-up follows
    w.gh_says(json!({ "head": reviewed, "mergeFail": "checks pending", "view": view(&reviewed, "PENDING") }));
    finish_line::approve(&id, None, false).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "merging"); assert_eq!(t.at("autoMerge"), &json!(true));
    w.gh_says(json!({ "head": reviewed, "view": view(&reviewed, "SUCCESS") }));
    poll_prs();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "done", "{}", t.at("failure")); assert!(t.b("cleaned") && !t.b("autoMerge"));
    assert_eq!(git(&w.root, &["ls-remote", "--heads", "origin", &branch]), "", "deleted at the remote, by the lease on the merged commit");
}

#[test]
fn a_pull_request_that_github_only_queued_is_not_taken_for_merged_nothing_is_cleaned_up_until_github_says_so() {
    let w = pr_world();
    let id = to_approval(&w, "With a PR");
    let head = task(&id).t("prHead");
    // a repo with a merge queue: gh pr merge answers fine, the pull request stays open
    w.gh_says(json!({ "queue": true, "view": plain(&head, "OPEN") }));
    finish_line::approve(&id, None, false).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "merging"); assert_eq!(t.at("autoMerge"), &json!(true)); assert!(!t.b("prMerged") && !t.b("cleaned") && !t.b("tidy"));
    assert!(t.s("msg").contains("GitHub merges it when"), "{}", t.s("msg"));
    assert!(Path::new(t.s("worktree")).exists()); assert!(in_sidebar(&w, &t));
    assert_eq!(git(&w.root, &["ls-remote", "--heads", "origin", t.s("branch")]).split('\t').next().unwrap(), head, "the remote branch is still there");
    poll_prs();
    assert_eq!(task(&id).s("stage"), "merging", "still watched, still waiting");
    // the queue merges it: now it is done and cleaned up
    w.gh_says(json!({ "view": plain(&head, "MERGED") }));
    poll_prs();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "done"); assert!(t.b("prMerged") && t.b("cleaned") && !t.b("autoMerge"));
    assert_eq!(w.state().arr("workspaces").len(), 0); assert_eq!(git(&w.root, &["ls-remote", "--heads", "origin", t.s("branch")]), "");
}

#[test]
fn a_queued_pull_request_that_is_closed_without_merging_stops_waiting_and_says_so() {
    let w = pr_world();
    let id = to_approval(&w, "With a PR");
    let head = task(&id).t("prHead");
    w.gh_says(json!({ "queue": true, "view": plain(&head, "OPEN") }));
    finish_line::approve(&id, None, false).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "merging"); assert_eq!(t.at("autoMerge"), &json!(true));
    w.gh_says(json!({ "view": plain(&head, "CLOSED") }));
    poll_prs();
    let t = task(&id);
    assert_eq!(t.s("stage"), "approval"); assert!(!t.b("autoMerge") && !t.b("mergeIntent") && !t.b("prMerged")); assert_eq!(t.at("mergeFailed"), &json!(true));
    assert_eq!(t.at("failure").s("title"), "Pull request was closed without merging"); assert_eq!(t.s("msg"), t.at("failure").s("title"));
    assert_eq!(row(&id).s("state"), "blocked");
    assert!(Path::new(t.s("worktree")).exists()); assert!(in_sidebar(&w, &t), "nothing was cleaned up");
    poll_prs();
    assert_eq!(task(&id).s("stage"), "approval");
}
