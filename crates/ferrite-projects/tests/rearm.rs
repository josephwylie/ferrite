//! Typing in the agent's own terminal re-arms the pipeline (approval revoked, review stale, checked again once you
//! are done with the chat), and the readings that must not: a late `working` of the turn already handled, one from the
//! terminal herdr had before it restarted, one an older version saved, and a task an older version left stuck re-armed.
//! A closing note is handled once.
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
fn get(w: &World, pane: &str) -> Value { api(w).agent_get(pane).unwrap().unwrap() }
fn with(a: &Value, f: Value) -> Value { let mut a = a.clone(); for (k, v) in f.as_object().cloned().unwrap() { a[k] = v; } a }
fn jobs() -> Vec<String> { sw::runner::JOBS.with(|j| j.borrow().iter().map(|x| x.0.clone()).collect()) }
/// The closing note written now, and stamped a little later than now. The board's clock stands still from here, so
/// nothing it does next (a turn handled, re-armed) is later than the note, however slow the machine.
fn note(t: &Obj, text: Option<&str>, later_ms: u64) {
    let at = sw::clock::now(); sw::clock::set(Some(at));
    let p = sw::flow::task::note_file(t);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    if let Some(text) = text { std::fs::write(&p, text).unwrap(); }
    std::fs::File::options().write(true).open(&p).unwrap().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_millis(at as u64 + later_ms)).unwrap();
}
/// herdr sees the pane's agent at `status`; `done` also counts a finished turn.
fn herdr_says(w: &World, pane: &str, status: &str, complete: bool, by: i64) {
    w.patch(|s| { let seq = s.i("seq") + by; s["seq"] = json!(seq); for p in s["panes"].as_array_mut().unwrap() { if p.s("pane_id") == pane { p["agent"]["status"] = json!(status); p["agent"]["seq"] = json!(seq); if complete { p["agent"]["completion"] = json!(seq); } } } });
}
fn new_terminal(w: &World, pane: &str, term: &str) { w.patch(|s| { for p in s["panes"].as_array_mut().unwrap() { if p.s("pane_id") == pane { p["terminal_id"] = json!(term); } } }); }
fn head(t: &Obj) -> String { git(Path::new(t.s("worktree")), &["rev-parse", "HEAD"]) }
/// A turn typed in the agent's own pane that changed the code waits for you to be done with the chat: the quiet time
/// passes, and its one review runs.
fn quiet_passes(w: &World, id: &str) -> Obj {
    let t = task(id);
    assert!(sw::flow::chat::held(&t), "waits for you: {} {}", t.s("stage"), t.at("chat"));
    sw::clock::advance(61_000);
    sw::housekeeping::housekeeping(&w.socket, sw::housekeeping::Force::Yes);
    settle(id)
}

#[test]
fn typing_in_the_agents_own_terminal_re_arms_the_pipeline_then_it_is_checked_again() {
    let w = world();
    let id = to_approval(&w, "Work");
    let t = task(&id);
    let pane = t.at("link").t("pane");
    w.script(json!({ "worker": [{ "files": { "f.txt": "v2\n" }, "commit": "v2", "note": null, "status": "working" }] }));
    api(&w).agent_prompt(&pane, "make it v2").unwrap();
    on_pane(&w, &pane);
    let t = task(&id);
    assert_eq!((t.s("stage"), t.at("review").b("stale"), t.b("requireApproval"), t.b("turnOpen")), ("working", true, true, true));
    assert_eq!(row(&id).s("state"), "working");
    note(&t, Some("Did v2."), 50);
    herdr_says(&w, &pane, "done", true, 1);
    settle(&id);
    let t = quiet_passes(&w, &id);
    assert_eq!((t.s("stage"), t.s("summary"), t.at("review").b("stale")), ("approval", "Did v2.", false));
    assert_eq!(t.s("reviewedHead"), head(&t));
}

#[test]
fn a_closing_note_is_handled_once_its_time_reads_back_the_same_from_the_task_s_file() {
    let w = world();
    let id = to_approval(&w, "Work");
    let pane = task(&id).at("link").t("pane");
    w.script(json!({ "worker": [{ "files": { "f.txt": "v2\n" }, "commit": "v2", "note": null, "status": "working" }] }));
    api(&w).agent_prompt(&pane, "make it v2").unwrap();
    on_pane(&w, &pane);
    // stamped at a time whose last digit a quick reading of the saved task gets wrong (one in nine is; it failed the
    // check on GitHub): 2100-01-01, later than anything the board armed
    let t = task(&id);
    note(&t, Some("Did v2."), 0);
    std::fs::File::options().write(true).open(sw::flow::task::note_file(&t)).unwrap().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_nanos(4102444800123456700)).unwrap();
    herdr_says(&w, &pane, "done", true, 1);
    let t = settle(&id);
    assert_eq!(t.at("handled").f("note"), sw::flow::task::note_stat(&t).unwrap().0, "the note handled is the note there");
    let armed = t.at("armedAt").clone();
    // later readings of the same rest do not take that note for a new turn
    herdr_says(&w, &pane, "done", false, 1); on_pane(&w, &pane);
    settle(&id);
    let t = quiet_passes(&w, &id);
    assert_eq!((t.s("stage"), t.s("summary"), t.at("review").b("stale"), t.at("armedAt")), ("approval", "Did v2.", false, &armed));
    assert_eq!(t.s("reviewedHead"), head(&t));
}

#[test]
fn a_working_reading_from_the_turn_already_handled_does_not_re_arm_it() {
    let w = world();
    w.script(json!({ "worker": [{ "files": { "f.txt": "v1\n" }, "commit": "v1", "note": null, "status": "working" }] }));
    let id = w.add("Work");
    delegate(&id);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "working");
    let pane = t.at("link").t("pane");
    let late = get(&w, &pane);
    assert_eq!(late.s("agent_status"), "working");
    // meanwhile the turn ends and the finish line handles it
    note(&t, Some("Did v1."), 50);
    herdr_says(&w, &pane, "done", true, 1);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure"));
    let armed = t.at("armedAt").clone();
    let same = || { let t = task(&id); assert_eq!((t.s("stage"), t.b("turnOpen"), t.at("armedAt"), t.b("requireApproval")), ("approval", false, &armed, false)); };
    apply(&id, &late, &api(&w)); same();
    apply(&id, &with(&late, json!({ "state_change_seq": t.at("handled").at("seq") })), &api(&w)); same();
    // however late it arrives
    patch(&id, |t| { let at = t.at("handled").i("at") - 3600000; t.obj_mut("handled").put("at", at); }).unwrap();
    apply(&id, &late, &api(&w)); same();
    on_pane(&w, &pane); same();
    // herdr restarted: the same or a lower number in a new terminal is a new turn, the human typing
    let t = task(&id);
    new_terminal(&w, &pane, "term_after_restart");
    apply(&id, &with(&late, json!({ "terminal_id": "term_after_restart", "state_change_seq": t.at("handled").at("seq") })), &api(&w));
    let t = task(&id);
    assert_eq!((t.s("stage"), t.b("turnOpen"), t.b("requireApproval")), ("working", true, true));
    if t.b("review") { assert!(t.at("review").b("stale")); }
}

#[test]
fn a_working_newer_than_the_handled_turn_or_lower_right_after_a_restart_is_the_human_typing() {
    let w = world();
    let id = to_approval(&w, "Work");
    let t = task(&id);
    let pane = t.at("link").t("pane");
    let now = get(&w, &pane);
    assert_eq!((t.at("handled").at("seq"), t.at("handled").at("term")), (now.at("state_change_seq"), now.at("terminal_id")));
    // a terminal herdr does not have in that pane is no restart: the reading is dropped
    apply(&id, &with(&now, json!({ "agent_status": "working", "terminal_id": "term_nowhere", "state_change_seq": 1 })), &api(&w));
    assert_eq!(task(&id).s("stage"), "approval");
    new_terminal(&w, &pane, "term_after_restart");
    apply(&id, &with(&now, json!({ "agent_status": "working", "terminal_id": "term_after_restart", "state_change_seq": 1 })), &api(&w));
    let t = task(&id);
    assert_eq!((t.s("stage"), t.b("turnOpen"), t.b("requireApproval")), ("working", true, true));
    // in the same herdr run a higher number re-arms as before
    w.script(json!({})); // to_approval scripts its one commit only when nothing is scripted
    let id2 = to_approval(&w, "More");
    let t = task(&id2);
    let b = get(&w, t.at("link").s("pane"));
    apply(&id2, &with(&b, json!({ "agent_status": "working", "state_change_seq": t.at("handled").i("seq") + 1 })), &api(&w));
    let t = task(&id2);
    assert_eq!((t.s("stage"), t.b("turnOpen"), t.b("requireApproval")), ("working", true, true));
}

#[test]
fn a_late_working_from_the_terminal_before_a_restart_does_not_re_open_work_finished_in_the_new_one() {
    let w = world();
    let id = to_approval(&w, "Work");
    let t = task(&id);
    let pane = t.at("link").t("pane");
    let old = with(&get(&w, &pane), json!({ "agent_status": "working" }));
    // herdr restarts: a new terminal, the counter starts over. The human types there
    w.patch(|s| { s["seq"] = json!(0); for p in s["panes"].as_array_mut().unwrap() { if p.s("pane_id") == pane { p["terminal_id"] = json!("term_after_restart"); p["agent"]["seq"] = json!(0); p["agent"].as_object_mut().unwrap().remove("completion"); } } });
    w.script(json!({ "worker": [{ "files": { "f.txt": "v2\n" }, "commit": "v2", "note": null, "status": "working" }] }));
    api(&w).agent_prompt(&pane, "make it v2").unwrap();
    on_pane(&w, &pane);
    assert_eq!(task(&id).s("stage"), "working");
    // a late "done" from the old terminal finishes nothing, however long ago the task was armed
    unsafe { std::env::set_var("HERDR_PROJECTS_REOPEN_MS", "0") };
    apply(&id, &with(&old, json!({ "agent_status": "done" })), &api(&w));
    unsafe { std::env::remove_var("HERDR_PROJECTS_REOPEN_MS") };
    assert_eq!((task(&id).t("stage"), task(&id).at("last").t("term"), jobs().len()), ("working".into(), "term_after_restart".into(), 0));
    note(&t, Some("Did v2."), 50);
    herdr_says(&w, &pane, "done", true, 1);
    settle(&id);
    let t = quiet_passes(&w, &id);
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure"));
    assert_eq!(t.at("handled").s("term"), "term_after_restart");
    assert!(old.i("state_change_seq") > t.at("handled").i("seq"));
    let fresh = current_review(&t).cloned();
    // the reading taken before the restart arrives now
    apply(&id, &old, &api(&w));
    let t = task(&id);
    assert_eq!((t.s("stage"), t.b("turnOpen")), ("approval", false));
    assert_eq!(current_review(&t).cloned(), fresh);
    // typing in the new terminal still re-arms
    apply(&id, &with(&get(&w, &pane), json!({ "agent_status": "working", "state_change_seq": t.at("handled").i("seq") + 1 })), &api(&w));
    assert_eq!(task(&id).s("stage"), "working");
}

#[test]
fn an_approval_saved_by_an_older_version_is_not_re_opened_by_a_late_working_either() {
    let w = world();
    let id = to_approval(&w, "Work");
    // as the older version saved it: no reading kept with the handled turn, no terminals, no mark on the turn
    patch(&id, |t| { let h = t.obj_mut("handled"); h.del("seq"); h.del("term"); if t.at("last").is_object() { t.obj_mut("last").del("term"); } t.del("terms"); t.del("turnBy"); }).unwrap();
    let t = task(&id);
    let pane = t.at("link").t("pane");
    let now = get(&w, &pane);
    let review = current_review(&t).cloned();
    assert!(review.is_some());
    // its turn was handled through the agent's own hook while herdr still said "working": that very `working` arrives again
    let s0 = now.i("state_change_seq") - 1;
    let before = t.at("handled").i("comp") - 5;
    let at = t.at("handled").i("at");
    for last in [json!({ "status": "working", "seq": s0, "comp": before, "at": at - 50 }), json!({ "status": "done", "seq": s0 + 1, "comp": s0 + 1, "at": at + 50 })] {
        let l = last.clone();
        patch(&id, move |t| { t.obj_mut("handled").put("comp", before); t.put("last", l); }).unwrap();
        apply(&id, &with(&now, json!({ "agent_status": "working", "state_change_seq": s0, "completion_seq": before })), &api(&w));
        let t = task(&id);
        assert_eq!(t.s("stage"), "approval", "{last}"); assert!(!t.b("turnOpen")); assert_eq!(current_review(&t).cloned(), review);
    }
    let comp = now.at("completion_seq").clone();
    let seq = now.at("state_change_seq").clone();
    patch(&id, |t| { t.obj_mut("handled").put("comp", comp.clone()); t.put("last", json!({ "status": "done", "seq": seq, "comp": comp, "at": sw::clock::now() })); }).unwrap();
    let t = task(&id);
    for seq in [t.at("handled").i("comp") - 1, t.at("handled").i("comp")] { apply(&id, &with(&now, json!({ "agent_status": "working", "state_change_seq": seq })), &api(&w)); }
    for _ in 0..3 { reconcile(&task(&id), &api(&w)); }
    let t = task(&id);
    assert_eq!(t.s("stage"), "approval"); assert!(!t.b("turnOpen")); assert!(!t.b("turnBy")); assert_eq!(current_review(&t).cloned(), review); assert_eq!(jobs().len(), 0);
    // the human typing is a later reading: that re-arms
    apply(&id, &with(&now, json!({ "agent_status": "working", "state_change_seq": t.at("handled").i("comp") + 1 })), &api(&w));
    let t = task(&id);
    assert_eq!((t.s("stage"), t.s("turnBy"), t.at("review").b("stale")), ("working", "typed", true));
}

#[test]
fn a_turn_nobody_saw_start_still_counts_a_new_commit_behind_an_approval_is_checked_again() {
    let w = world();
    let id = to_approval(&w, "Work");
    let t = task(&id);
    let tree = Path::new(t.s("worktree"));
    std::fs::write(tree.join("f.txt"), "sneaky\n").unwrap();
    git(tree, &["commit", "-qam", "sneaky"]);
    note(&t, Some("Changed it again."), 2000);
    herdr_says(&w, t.at("link").s("pane"), "done", true, 2);
    settle(&id);
    let t = quiet_passes(&w, &id);
    assert_eq!((t.s("stage"), t.s("summary")), ("approval", "Changed it again."));
    assert_eq!(t.s("reviewedHead"), head(&t));
}
