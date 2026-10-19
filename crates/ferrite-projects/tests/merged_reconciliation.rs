//! Merged deliveries cannot become new work merely because someone sends housekeeping words.
use crate::common::*;
use sw::json::*;
use serde_json::Value;
use sw::store::tasks::patch;

#[test]
fn cleanup_message_preserves_completed_delivery() {
    let w = world();
    let id = w.add("Completed component");
    patch(&id, |t| {
        t.put("stage", "done"); t.put("cleaned", true);
        t.put("pr", "https://github.com/test/proj/pull/73"); t.put("prMerged", true);
        t.put("review", json!({"meets": true, "findings": [], "pass": true, "head": "original"}));
    }).unwrap();
    let before = task(&id);
    assert!(sw::actions::talk::reply(&id, "Remove the merged worktree", "coordinator").is_err());
    assert_eq!(task(&id), before);
    assert!(sw::runner::JOBS.with(|j| j.borrow().is_empty()));
}

fn cli(args: &[&str]) -> sw::err::Res<(Value, String)> {
    let a = sw::cli::flags::parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    sw::cli::flags::refuse_unknown(args[0], &a)?;
    sw::cli::tasks::command(args[0], &a)
}
const PR: &str = "https://github.com/test/proj/pull/73";
struct Delivery { id: String, reviewed: String, merged: String, tree: std::path::PathBuf, github: Value }
fn delivery(w: &World) -> Delivery {
    let id = w.add("Merged component");
    let tree = w.base.join("delivery");
    git(&w.root, &["worktree", "add", "-b", "work/component", tree.to_str().unwrap()]);
    std::fs::write(tree.join("component.txt"), "original source work\n").unwrap();
    git(&tree, &["add", "."]); git(&tree, &["commit", "-qm", "reviewed component"]);
    let reviewed = git(&tree, &["rev-parse", "HEAD"]);
    git(&w.root, &["merge", "--squash", "work/component"]);
    git(&w.root, &["commit", "-qm", "Merge PR 73"]);
    let merged = git(&w.root, &["rev-parse", "HEAD"]);
    let bare = w.base.join("remote.git");
    git(&w.root, &["clone", "--bare", w.root.to_str().unwrap(), bare.to_str().unwrap()]);
    // The transport stays entirely local; GitHub itself is answered by the fixture.
    git(&w.root, &["remote", "add", "origin", "https://github.com/test/proj"]);
    git(&w.root, &["config", &format!("url.{}.insteadOf", bare.display()), "https://github.com/test/proj"]);
    git(&tree, &["reset", "--hard", &merged]);
    let review = json!({"round":1, "at": 1000, "head":reviewed, "meets":true, "findings":[], "gate":"major", "pass":true, "checks":["component checked"], "limitations":["integration separate"]});
    patch(&id, |t| {
        t.put("stage", "stopped"); t.put("branch", "work/component"); t.put("worktree", tree.display().to_string());
        t.put("link", json!({"socket":w.socket,"pane":"gone-worker","reviewer":{"pane":"gone-reviewer"}}));
        t.put("last", json!({"status":"done","seq":1,"comp":1,"term":"test","at":1000}));
        t.put("reviews", json!([review])); t.put("reviewRound", 2); t.put("turnOpen", false);
        t.put("mergeCommit", merged.clone());
    }).unwrap();
    let github = json!({"url":PR,"state":"MERGED","headRefName":"work/component","headRefOid":reviewed,"baseRefName":"main","mergeCommit":{"oid":merged},"mergedAt":"2026-10-19T05:10:41Z","isCrossRepository":false});
    w.gh_says(json!({"view":github}));
    Delivery { id, reviewed, merged, tree, github }
}
fn recover(d: &Delivery, check: bool) -> sw::err::Res<(Value, String)> {
    let mut args = vec!["reconcile-merged", &d.id, "--pr", PR, "--review-head", &d.reviewed];
    if check { args.push("--check"); }
    cli(&args)
}
#[test]
fn recovery_restores_original_pass_and_pr_without_a_new_review_or_delivery() {
    let w = world(); let d = delivery(&w);
    let dependent = w.add("Dependent integration");
    patch(&dependent, |t| { t.put("stage", "after"); t.put("after", json!([d.id])); }).unwrap();
    let queued = task(&dependent);
    patch(&d.id, |t| t.put("tidy", json!({"deleteBranch":"merged","tip":d.merged}))).unwrap();
    let before = task(&d.id);
    assert!(recover(&d, true).unwrap().0.b("verified"));
    assert_eq!(task(&d.id), before, "preflight writes no task state");
    let (result, _) = recover(&d, false).unwrap();
    let restored = task(&d.id);
    assert!(!restored.b("tidy"), "do not schedule cleanup as a side effect of recovery");
    assert_eq!(restored.s("stage"), "done"); assert_eq!(restored.s("pr"), PR);
    assert_eq!(restored.s("mergeCommit"), d.merged); assert_eq!(restored.s("reviewedHead"), d.reviewed);
    assert_eq!(restored.at("reviews"), before.at("reviews")); assert_eq!(restored.i("reviewRound"), 2);
    assert_eq!(restored.at("review"), &before.arr("reviews")[0]);
    assert_eq!(sw::store::reviews::current_review(&restored).unwrap().at("pass"), &json!(true));
    assert_eq!(task(&dependent), queued, "no dependency overrides or incidental runners");
    assert_eq!(read(&d.tree.join("component.txt")), "original source work\n");
    let evidence: Value = serde_json::from_str(&read(std::path::Path::new(result.s("evidence")))).unwrap();
    assert_eq!(evidence.at("task"), &Value::Object(before)); assert_eq!(evidence.at("github"), &d.github);
    assert!(sw::runner::JOBS.with(|j| j.borrow().is_empty()));
    assert!(w.gh.log.borrow().iter().all(|s| s.starts_with("pr view ")));
    recover(&d, true).unwrap(); // repeated verification keeps the original pass
}
#[test]
fn recovery_rejects_dirty_untracked_and_novel_unmerged_work_without_mutation() {
    let w = world(); let d = delivery(&w); let before = task(&d.id);
    std::fs::write(d.tree.join("component.txt"), "dirty").unwrap();
    assert!(recover(&d, false).unwrap_err().msg.contains("dirty")); assert_eq!(task(&d.id), before);
    git(&d.tree, &["checkout", "--", "component.txt"]);
    std::fs::write(d.tree.join("new.txt"), "untracked").unwrap();
    assert!(recover(&d, false).unwrap_err().msg.contains("untracked")); assert_eq!(task(&d.id), before);
    git(&d.tree, &["add", "."]); git(&d.tree, &["commit", "-qm", "new work"]);
    assert!(recover(&d, false).unwrap_err().msg.contains("unmerged")); assert_eq!(task(&d.id), before);
    assert_eq!(read(&d.tree.join("new.txt")), "untracked");
    assert!(sw::runner::JOBS.with(|j| j.borrow().is_empty()));
}
#[test]
fn recovery_refuses_mismatched_ambiguous_and_active_cases() {
    let w = world(); let d = delivery(&w);
    for (field, value) in [("state",json!("OPEN")), ("headRefName",json!("work/other")), ("headRefOid",json!("f".repeat(40))), ("isCrossRepository",json!(true))] {
        let mut v = d.github.clone(); v[field] = value; w.gh_says(json!({"view":v}));
        let before = task(&d.id); assert!(recover(&d, false).is_err(), "{field}"); assert_eq!(task(&d.id), before);
    }
    w.gh_says(json!({"view":d.github}));
    for (key, value) in [("pr",json!("https://github.com/test/proj/pull/74")), ("turnOpen",json!(true)), ("runner",Value::Object(sw::store::files::marker())), ("mergeCommit",json!("f".repeat(40))), ("tidy",json!({"tip":"wrong","deleteBranch":"merged"}))] {
        let original = task(&d.id).at(key).clone();
        patch(&d.id, |t| t.put(key, value.clone())).unwrap();
        let before = task(&d.id); assert!(recover(&d, false).is_err(), "{key}"); assert_eq!(task(&d.id), before);
        patch(&d.id, |t| t.put(key, original)).unwrap();
    }
    for pane in ["gone-worker", "gone-reviewer"] {
        for status in ["working", "blocked", "unknown"] {
            w.patch(|s| s["panes"] = json!([{"pane_id":pane,"agent":{"kind":"codex","status":status}}]));
            let before = task(&d.id); assert!(recover(&d, false).unwrap_err().msg.contains("active or its state is unknown")); assert_eq!(task(&d.id), before);
        }
        w.patch(|s| s["panes"] = json!([{"pane_id":pane,"tool":true,"agent":{"kind":"codex","status":"done"}}]));
        let before = task(&d.id); assert!(recover(&d, false).unwrap_err().msg.contains("running work")); assert_eq!(task(&d.id), before);
    }
    w.patch(|s| s["panes"] = json!([]));
    patch(&d.id, |t| { let mut history = t.arr("reviews").to_vec(); history.push(json!({"head":d.reviewed,"at":2000,"meets":false,"findings":[],"pass":false})); t.put("reviews",history); }).unwrap();
    let before = task(&d.id); assert!(recover(&d, false).is_err()); assert_eq!(task(&d.id), before);
}
#[test]
fn explicit_follow_up_keeps_delivery_and_uses_normal_work_and_review_gates() {
    let w = world(); let d = delivery(&w);
    recover(&d, false).unwrap();
    // Completed cleanup: a subsequent explicit new delivery starts from the normal delegation path.
    patch(&d.id, |t| { t.put("cleaned",true); t.put("link",json!({"socket":w.socket})); }).unwrap();
    let before = task(&d.id);
    let args = ["tell", &d.id, "--follow-up", "Implement the requested new component"];
    cli(&args).unwrap();
    let after = task(&d.id);
    assert!(["queued", "starting"].contains(&after.s("stage")));
    let kept = &after.arr("deliveries")[0];
    for key in ["pr", "mergeCommit", "review", "reviews", "reviewedHead", "reviewRound", "branch"] { assert_eq!(kept.at(key),before.at(key),"{key}"); }
    assert!(!after.b("pr")); assert!(!after.b("mergeCommit")); assert!(!after.b("review"));
    assert_eq!(after.at("reviews"),before.at("reviews")); assert_eq!(after.at("settings"),before.at("settings"));
    assert_eq!(sw::runner::JOBS.with(|j| j.borrow().iter().map(|j| j.0.clone()).collect::<Vec<_>>()), vec!["delegate"]);
}

#[test]
fn recovery_refuses_unavailable_session_missing_review_and_changed_task() {
    let w = world(); let d = delivery(&w);
    let original = task(&d.id);
    patch(&d.id, |t| t.obj_mut("link").put("socket", "missing-session")).unwrap();
    // Fake sessions answer regardless of socket, so model a failed session API explicitly.
    sw::herdr::cli::set_fake(None);
    let before = task(&d.id); assert!(recover(&d, false).unwrap_err().msg.contains("session unavailable")); assert_eq!(task(&d.id), before);
    sw::herdr::cli::set_fake(Some(w.fake.clone()));
    patch(&d.id, |t| { *t = original.clone(); t.put("reviews",json!([])); }).unwrap();
    let before = task(&d.id); assert!(recover(&d, false).unwrap_err().msg.contains("review history")); assert_eq!(task(&d.id), before);
    patch(&d.id, |t| *t = original.clone()).unwrap();
    let id = d.id.clone(); let response = d.github.clone();
    sw::flow::git::set_fake_gh(Some(std::rc::Rc::new(move |_| {
        patch(&id, |t| t.obj_mut("last").put("at", 2000)).unwrap();
        sw::flow::git::GhOut { status:0, stdout:response.to_string(), stderr:String::new() }
    })));
    recover(&d, true).unwrap(); // a keeper polling the same finished turn must not prevent recovery
    assert_eq!(task(&d.id).at("last").i("at"), 2000);
    let id = d.id.clone(); let response = d.github.clone();
    sw::flow::git::set_fake_gh(Some(std::rc::Rc::new(move |_| {
        patch(&id, |t| t.put("title", "Changed while verifying")).unwrap();
        sw::flow::git::GhOut { status:0, stdout:response.to_string(), stderr:String::new() }
    })));
    assert!(recover(&d, false).unwrap_err().msg.contains("changed during verification"));
    assert_eq!(task(&d.id).s("stage"), "stopped"); assert_eq!(task(&d.id).s("title"), "Changed while verifying");
}
