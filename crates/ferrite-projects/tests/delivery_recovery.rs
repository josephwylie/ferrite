//! Concrete stalled finish-line states, isolated from the installed board.
use crate::common::*;
use crate::pull_requests_checks::{pr_world, running, passed, merged, pane_prompts};
use sw::json::*;
use sw::store::tasks::{update, Change};
use sw::housekeeping::{housekeeping, poll_prs, Force};
use serde_json::json;

#[test]
fn historical_ci_failure_resumes_authorized_exact_head_into_pending_checks() {
    let w = pr_world(json!({"review.rounds": 0}));
    let id = to_approval(&w, "FRT-1 replacement head");
    let head = task(&id).t("prHead");
    update(&id, |t| {
        t.put("review", json!({"head": head, "score": 5, "recommend": true}));
        t.put("mergeIntent", json!({"auto": true, "verifiedHead": head, "approvedAt": 1}));
        t.put("prFailure", json!({"remoteKind": "checks", "head": "9bd6b6a", "title": "Old CI failure"}));
        t.put("prState", "failing");
        Ok(Change::Write)
    }).unwrap();
    sw::store::settings::change::set_value("merge.by", &json!("itself"), &sw::store::settings::lookup::Scope::task(&task(&id))).unwrap();
    let before = pane_prompts(&w, &id);
    let mut pending = running(&head);
    pending["statusCheckRollup"].as_array_mut().unwrap().push(json!({"__typename": "CheckRun", "name": "lint", "status": "COMPLETED", "conclusion": "SUCCESS"}));
    w.gh_says(json!({"view": pending}));
    poll_prs(); poll_prs();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "merging", "{t:?}");
    assert!(t.b("checksWait"));
    assert_eq!(t.s("msg"), "Waiting for GitHub checks · 1 of 2 passed");
    assert_eq!(row(&id).s("state"), "working");
    assert!(!merged(&w));
    assert_eq!(pane_prompts(&w, &id), before);
    let mut all_passed = passed(&head);
    all_passed["statusCheckRollup"].as_array_mut().unwrap().push(json!({"__typename": "CheckRun", "name": "lint", "status": "COMPLETED", "conclusion": "SUCCESS"}));
    w.gh_says(json!({"view": all_passed}));
    poll_prs();
    assert_eq!(settle(&id).s("stage"), "done");
}

#[test]
fn timed_out_push_already_on_remote_continues_without_source_turn() {
    let w = pr_world(json!({}));
    let id = to_approval(&w, "Peek62 lost push response");
    let original = task(&id);
    let tree = std::path::Path::new(original.s("worktree"));
    let head = git(tree, &["rev-parse", "HEAD"]);
    assert!(git(tree, &["ls-remote", "origin", &format!("refs/heads/{}", original.s("branch"))]).starts_with(&head));
    assert!(git(tree, &["status", "--porcelain"]).is_empty());
    update(&id, |t| {
        t.put("stage", "failed"); t.put("pr", Value::Null); t.put("prHead", Value::Null);
        t.put("review", Value::Null); t.put("reviewedHead", Value::Null);
        t.put("failure", json!({"stage": "push", "title": "Could not push the branch", "detail": "herdr did not answer in time"}));
        Ok(Change::Write)
    }).unwrap();
    let transport = PushTransport::new(&w, false);
    let before = pane_prompts(&w, &id);
    housekeeping(&w.socket, Force::Yes); housekeeping(&w.socket, Force::Yes);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval", "{t:?}");
    assert_eq!(transport.pushes(), 0, "already on remote: do not push again");
    assert_eq!(t.s("reviewedHead"), head);
    assert_eq!(git(tree, &["rev-parse", "HEAD"]), head);
    assert_eq!(pane_prompts(&w, &id), before);
    assert_eq!(t.at("summary"), original.at("summary"));
    assert!(!merged(&w));
}

// A real git wrapper at the transport boundary: can lose a successful push's response, or time out before it.
// All repositories, commands and counters belong to this test's temporary world.
struct PushTransport { path: std::ffi::OsString, count: std::path::PathBuf }
impl PushTransport {
    fn new(w: &World, finish: bool) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::var_os("PATH").unwrap();
        let dir = w.base.join("transport"); std::fs::create_dir_all(&dir).unwrap();
        let count = dir.join("pushes");
        let git = std::process::Command::new("which").arg("git").output().unwrap();
        let real = String::from_utf8(git.stdout).unwrap().trim().to_string();
        let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
        let body = format!("#!/bin/sh\nif [ \"$1\" = push ]; then\n echo push >> {}\n {}\n echo 'herdr did not answer in time' >&2\n exit 1\nfi\nexec {} \"$@\"\n", quote(&count.display().to_string()), if finish { format!("{} \"$@\" || exit $?", quote(&real)) } else { ":".into() }, quote(&real));
        let fetch = format!("if [ \"$1\" = fetch ] && [ -f {} ]; then\n cat {} >&2\n exit 1\nfi\n", quote(&dir.join("fetch-error").display().to_string()), quote(&dir.join("fetch-error").display().to_string()));
        let body = body.replacen("#!/bin/sh\n", &format!("#!/bin/sh\n{fetch}"), 1);
        std::fs::write(dir.join("git"), body).unwrap();
        std::fs::set_permissions(dir.join("git"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut paths = vec![dir]; paths.extend(std::env::split_paths(&path));
        unsafe { std::env::set_var("PATH", std::env::join_paths(paths).unwrap()); }
        Self { path, count }
    }
    fn fetch_error(&self, error: Option<&str>) {
        let path = self.count.parent().unwrap().join("fetch-error");
        if let Some(error) = error { std::fs::write(path, error).unwrap(); } else { std::fs::remove_file(path).unwrap(); }
    }
    fn pushes(&self) -> usize { read(&self.count).lines().count() }
}
impl Drop for PushTransport { fn drop(&mut self) { unsafe { std::env::set_var("PATH", &self.path); } } }

fn lost_push(id: &str) {
    update(id, |t| {
        t.put("stage", "failed");
        t.put("failure", json!({"stage": "push", "title": "Could not push the branch", "detail": "herdr did not answer in time"}));
        Ok(Change::Write)
    }).unwrap();
}
fn recover(w: &World, id: &str) {
    sw::finish::delivery::recover(&[task(id)], &w.socket);
    sw::finish::delivery::recover(&[task(id)], &w.socket);
}

#[test]
fn push_timeouts_reconcile_success_and_back_off_to_an_explicit_limit() {
    for completes in [true, false] {
        let w = pr_world(json!({}));
        let id = to_approval(&w, "Push transport");
        let t = task(&id);
        let before = pane_prompts(&w, &id);
        git(std::path::Path::new(t.s("worktree")), &["push", "origin", &format!(":{}", t.s("branch"))]);
        lost_push(&id);
        let transport = PushTransport::new(&w, completes);
        recover(&w, &id);
        assert_eq!(sw::runner::JOBS.with(|j| j.borrow().iter().filter(|(job, who, _)| job == "pipeline" && who == &id).count()), 1);
        let mut t = settle(&id);
        if completes {
            assert_eq!(t.s("stage"), "approval", "{}", t.at("failure"));
            assert_eq!(transport.pushes(), 1);
            assert!(!t.b("deliveryRetry"));
        } else {
            for (i, delay) in [5_000, 15_000, 45_000].iter().enumerate() {
                assert_eq!(t.s("stage"), "pushing", "{}", t.at("failure"));
                assert_eq!(t.at("deliveryRetry").i("tries"), i as i64 + 1);
                assert_eq!(row(&id).s("state"), "working");
                let until = t.at("deliveryRetry").i("until");
                assert!(until - sw::clock::now() <= *delay);
                sw::clock::set(Some(until - 1)); recover(&w, &id); settle(&id);
                assert_eq!(transport.pushes(), i + 1, "no early or duplicate push");
                sw::clock::set(Some(until)); recover(&w, &id); t = settle(&id);
            }
            assert_eq!(t.s("stage"), "failed");
            assert!(t.at("deliveryRetry").b("exhausted"));
            assert!(t.at("failure").s("detail").contains("retries exhausted"));
            assert_eq!(transport.pushes(), 4);
            recover(&w, &id); settle(&id);
            assert_eq!(transport.pushes(), 4);
        }
        assert_eq!(pane_prompts(&w, &id), before);
        assert!(!merged(&w));
    }
}

#[test]
fn recovery_preserves_review_ci_and_human_decision_gates() {
    let w = pr_world(json!({"review.rounds": 0}));
    let id = to_approval(&w, "Delivery gates");
    let head = task(&id).t("prHead");
    update(&id, |t| {
        t.put("mergeIntent", json!({"verifiedHead": head, "approvedAt": 1}));
        t.put("review", json!({"head": head, "score": 5, "recommend": true, "stale": true}));
        Ok(Change::Write)
    }).unwrap();
    w.gh_says(json!({"view": passed(&head)}));
    poll_prs(); assert_eq!(settle(&id).s("stage"), "approval"); assert!(!merged(&w));
    update(&id, |t| { t.obj_mut("review").put("stale", false); Ok(Change::Write) }).unwrap();
    w.gh_says(json!({"view": crate::pull_requests_checks::failed(&head)}));
    poll_prs(); let t = settle(&id);
    assert_eq!(t.s("stage"), "approval"); assert_eq!(t.at("prFailure").s("head"), head); assert!(!merged(&w));
    let before = pane_prompts(&w, &id);
    for failure in [json!({"stage": "push", "detail": "Authentication failed"}), json!({"stage": "push", "detail": "herdr did not answer in time", "auth": true})] {
        update(&id, |t| { t.put("stage", "failed"); t.put("failure", failure); Ok(Change::Write) }).unwrap();
        recover(&w, &id); assert_eq!(settle(&id).s("stage"), "failed");
    }
    // A human decision or completed task never becomes eligible merely because old push evidence remains.
    for stage in ["asks", "done"] {
        update(&id, |t| { t.put("stage", stage); t.put("failure", json!({"stage": "push", "detail": "herdr did not answer in time"})); Ok(Change::Write) }).unwrap();
        let before_task = task(&id); recover(&w, &id); assert_eq!(task(&id), before_task);
    }
    assert_eq!(pane_prompts(&w, &id), before);
}

#[test]
fn fetch_timeout_during_push_recovery_resumes_when_transport_returns() {
    let w = pr_world(json!({}));
    let id = to_approval(&w, "Fetch during push recovery");
    let original = task(&id);
    let tree = std::path::Path::new(original.s("worktree"));
    let head = git(tree, &["rev-parse", "HEAD"]);
    assert!(git(tree, &["ls-remote", "origin", &format!("refs/heads/{}", original.s("branch"))]).starts_with(&head));
    let before = pane_prompts(&w, &id);
    lost_push(&id);
    let transport = PushTransport::new(&w, false);
    transport.fetch_error(Some("herdr did not answer in time"));
    recover(&w, &id); let waiting = settle(&id);
    assert_eq!(waiting.s("stage"), "pushing");
    assert_eq!(waiting.at("deliveryRetry").i("tries"), 1);
    assert!(waiting.s("msg").starts_with("Fetch timed out"));
    sw::clock::set(Some(waiting.at("deliveryRetry").i("until") - 1));
    recover(&w, &id);
    assert!(sw::runner::JOBS.with(|j| j.borrow().is_empty()), "no early or duplicate attempt");
    transport.fetch_error(None);
    sw::clock::set(Some(sw::clock::now() + 120_000));
    recover(&w, &id); housekeeping(&w.socket, Force::Yes);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure"));
    assert_eq!(t.s("reviewedHead"), head);
    assert_eq!(transport.pushes(), 0);
    assert_eq!(pane_prompts(&w, &id), before);
    assert_eq!(t.at("summary"), original.at("summary"));
}

#[test]
fn recovery_fetch_shares_push_budget_and_keeps_permanent_failures_visible() {
    let w = pr_world(json!({}));
    let id = to_approval(&w, "Fetch retry budget");
    let original = task(&id);
    let before = pane_prompts(&w, &id);
    // The first timeout is a push, the remaining ones are fetches: changing the failing step cannot reset the budget.
    git(std::path::Path::new(original.s("worktree")), &["push", "origin", &format!(":{}", original.s("branch"))]);
    lost_push(&id);
    let transport = PushTransport::new(&w, false);
    recover(&w, &id); let mut t = settle(&id);
    transport.fetch_error(Some("herdr did not answer in time"));
    for tries in 1..=3 {
        assert_eq!(t.at("deliveryRetry").i("tries"), tries);
        assert_eq!(t.s("stage"), "pushing");
        sw::clock::set(Some(t.at("deliveryRetry").i("until")));
        recover(&w, &id); t = settle(&id);
    }
    assert_eq!(t.s("stage"), "failed");
    assert!(t.at("deliveryRetry").b("exhausted"));
    assert_eq!(t.at("failure").s("title"), "Delivery transport retries exhausted");
    transport.fetch_error(None);
    sw::clock::set(Some(sw::clock::now() + 120_000));
    recover(&w, &id); assert_eq!(settle(&id).s("stage"), "failed");
    assert_eq!(transport.pushes(), 1);
    // Auth and unknown fetch errors are not transient merely because this is delivery recovery.
    for error in ["Authentication failed", "fatal: couldn't find remote ref main"] {
        update(&id, |t| { t.put("deliveryRetry", Value::Null); Ok(Change::Write) }).unwrap();
        lost_push(&id); transport.fetch_error(Some(error));
        recover(&w, &id); let t = settle(&id);
        assert_eq!(t.s("stage"), "failed"); assert_eq!(t.at("failure").s("phase"), "fetch");
        assert_eq!(t.at("deliveryRetry").i("tries"), 0);
        assert_eq!(t.at("failure").b("auth"), error == "Authentication failed");
        transport.fetch_error(None);
        recover(&w, &id); assert_eq!(settle(&id).s("stage"), "failed");
    }
    // A fetch timeout saved by the previous CLI is recognized only within an existing delivery recovery.
    update(&id, |t| {
        t.put("failure", json!({"stage": "merge", "phase": "fetch", "detail": "herdr did not answer in time"}));
        Ok(Change::Write)
    }).unwrap();
    recover(&w, &id);
    assert_eq!(task(&id).s("stage"), "checking");
    assert_eq!(sw::runner::JOBS.with(|j| j.borrow().iter().filter(|(job, who, _)| job == "pipeline" && who == &id).count()), 1);
    settle(&id);
    assert_eq!(pane_prompts(&w, &id), before);
    assert!(!merged(&w));
}
