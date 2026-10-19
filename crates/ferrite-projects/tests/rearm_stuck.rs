//! A task an older version left stuck re-armed (working, turn open, no runner, completion handled): handed to the
//! human, never finished by itself; and a real new turn after "finish anyway" is never taken for one.
use crate::common;
use common::*;
use serde_json::{json, Value};
use std::path::Path;
use sw::herdr::Herdr;
use sw::hook::{apply, reconcile};
use sw::json::*;
use sw::store::reviews::current_review;
use sw::store::tasks::patch;

fn api(w: &World) -> Herdr { Herdr::new(Some(w.socket.clone())) }
fn with(a: &Value, f: Value) -> Value { let mut a = a.clone(); for (k, v) in f.as_object().cloned().unwrap() { a[k] = v; } a }
fn jobs() -> Vec<String> { sw::runner::JOBS.with(|j| j.borrow().iter().map(|x| x.0.clone()).collect()) }
fn herdr_says(w: &World, pane: &str, f: impl Fn(&mut Value)) { w.patch(|s| { let seq = s.i("seq") + 1; s["seq"] = json!(seq); for p in s["panes"].as_array_mut().unwrap() { if p.s("pane_id") == pane { p["agent"]["seq"] = json!(seq); f(p); } } }); }
fn status(s: &str) -> impl Fn(&mut Value) + '_ { move |p: &mut Value| p["agent"]["status"] = json!(s) }

#[test]
fn a_task_already_stuck_re_armed_is_handed_to_the_human_never_finished_by_itself() {
    let w = world();
    let id = to_approval(&w, "Work");
    // the state an earlier version left behind, without the fields this one writes
    patch(&id, |t| { let h = t.obj_mut("handled"); h.del("seq"); h.del("term"); let at = h.i("at"); t.del("terms"); t.del("last"); t.del("turnBy"); t.put("stage", "working"); t.put("turnOpen", true); t.put("armedAt", at + 11); t.put("since", at + 11); t.put("msg", ""); t.obj_mut("review").put("stale", true); t.put("requireApproval", true); }).unwrap();
    let t = task(&id);
    let pane = t.at("link").t("pane");
    let tree = Path::new(t.s("worktree")).to_path_buf();
    assert_eq!(row(&id).s("state"), "working");
    // a `working` taken before that turn completed arrives late, more than once: it says nothing about this turn
    let stale = with(&api(&w).agent_get(&pane).unwrap().unwrap(), json!({ "agent_status": "working", "state_change_seq": t.at("handled").i("comp") - 1 }));
    apply(&id, &stale, &api(&w)); apply(&id, &stale, &api(&w));
    assert!(!task(&id).b("turnBy")); assert_eq!(task(&id).at("armedAt"), t.at("armedAt")); assert_eq!(task(&id).s("stage"), "working");
    // a reply sent from the board looks like this for a moment: nothing happens at once
    on_pane(&w, &pane);
    assert_eq!((task(&id).t("stage"), jobs().len()), ("working".into(), 0));
    unsafe { std::env::set_var("HERDR_PROJECTS_REOPEN_MS", "0") };
    // the runner asks herdr itself: while herdr says the agent works, a late "done" from a replaced terminal finishes nothing
    let real = api(&w).agent_get(&pane).unwrap().unwrap();
    std::fs::write(tree.join("half.txt"), "not finished\n").unwrap();
    herdr_says(&w, &pane, |p| { p["terminal_id"] = json!("term_c"); p["agent"]["status"] = json!("working"); });
    apply(&id, &with(&real, json!({ "terminal_id": "term_b" })), &api(&w));
    assert_eq!((jobs().len(), task(&id).t("stage")), (0, "working".into()));
    assert_ne!(task(&id).arr("terms").last().map(text), Some("term_b".into()));
    // nor does a late "done" of the pane's own terminal: the runner sees the agent working and leaves
    patch(&id, |t| { t.del("terms"); t.del("last"); t.obj_mut("handled").put("term", "term_c"); }).unwrap();
    apply(&id, &with(&real, json!({ "terminal_id": "term_c" })), &api(&w));
    assert_eq!(jobs(), vec!["recover"]);
    sw::runner::drain_jobs(&|job, id, extra| { sw::jobs::run_job(job, id, extra); });
    let t = task(&id);
    assert_eq!(t.s("stage"), "working"); assert!(!t.b("runner")); assert!(git(&tree, &["status", "--porcelain"]).contains("half.txt"));
    assert_eq!(t.s("turnBy"), "typed"); // herdr has now seen it work: a real turn
    // the stuck task as it was, its agent really at rest: now the finish line runs again
    patch(&id, |t| { t.put("turnBy", Value::Null); }).unwrap();
    std::fs::remove_file(tree.join("half.txt")).unwrap();
    herdr_says(&w, &pane, status("done"));
    reconcile(&task(&id), &api(&w));
    assert_eq!(jobs(), vec!["recover"]);
    // handed to the human and nothing decided for them: unsaved work stays unsaved, nothing committed, checked or put up for approval
    std::fs::write(tree.join("half.txt"), "not finished\n").unwrap();
    let head = git(&tree, &["rev-parse", "HEAD"]);
    let t = settle(&id);
    assert_eq!((t.s("stage"), t.b("turnOpen"), t.b("runner"), row(&id).t("state")), ("nosummary", false, false, "error".into()));
    assert_eq!(git(&tree, &["rev-parse", "HEAD"]), head); assert!(git(&tree, &["status", "--porcelain"]).contains("half.txt"));
    assert!(current_review(&t).is_none());
    std::fs::remove_file(tree.join("half.txt")).unwrap();
    // "finish anyway" is the human saying it is done
    sw::actions::finish_line::finish_anyway(&id).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure")); assert!(!t.b("runner")); assert!(!current_review(&t).unwrap().has("stale"));
    // a real new turn is never taken for that, though the note and herdr's count are unchanged and herdr calls the agent idle a while
    let quiet = || { herdr_says(&w, &pane, status("idle")); on_pane(&w, &pane); (task(&id).t("stage"), jobs().len()) };
    std::fs::write(tree.join("half.txt"), "not finished\n").unwrap();
    herdr_says(&w, &pane, status("working"));
    on_pane(&w, &pane);
    assert_eq!((task(&id).t("turnBy"), task(&id).t("stage")), ("typed".into(), "working".into()));
    let rearm = |by: i64| { patch(&id, |t| { let at = t.at("handled").i("at") + by; t.put("armedAt", at); }).unwrap(); };
    rearm(11);
    assert_eq!(quiet(), ("working".into(), 0));
    patch(&id, |t| { t.put("stage", "approval"); t.put("turnOpen", false); t.put("turnBy", Value::Null); }).unwrap();
    w.script(json!({ "worker": [{ "note": null, "status": "working" }] }));
    sw::actions::talk::tell(&id, "one more thing", false).unwrap();
    assert_eq!(task(&id).s("turnBy"), "reply");
    rearm(11);
    assert_eq!(quiet(), ("working".into(), 0));
    assert!(git(&tree, &["status", "--porcelain"]).contains("half.txt"));
    std::fs::remove_file(tree.join("half.txt")).unwrap();
    patch(&id, |t| { t.put("turnBy", Value::Null); }).unwrap();
    // nor one armed long after the handled turn
    rearm(60000);
    assert_eq!(quiet(), ("working".into(), 0));
    // a turn really in progress is left alone: the agent has written a newer note than the handled one
    patch(&id, |t| { t.put("stage", "working"); t.put("turnOpen", true); }).unwrap();
    rearm(11);
    let note = sw::flow::task::note_file(&t);
    std::fs::File::options().write(true).open(&note).unwrap().set_modified(std::time::SystemTime::now() + std::time::Duration::from_millis(50)).unwrap();
    on_pane(&w, &pane);
    assert_eq!((task(&id).t("stage"), jobs().len()), ("working".into(), 0));
    unsafe { std::env::remove_var("HERDR_PROJECTS_REOPEN_MS") };
}
