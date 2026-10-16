//! The reviewer's brief and the Coordinator's messages are held to what a task's agent is: an enter a starting program
//! swallowed is pressed again, never the words again; words typed into a turn the Coordinator was in the middle of get
//! their enter once it rests.
use crate::common;
use common::*;
use serde_json::{json, Value};
use sw::coordinator::{pump::tick, requests::send, state};
use sw::json::*;

fn waits(ready: i64, submit: i64) { unsafe { std::env::set_var("HERDR_PROJECTS_READY_MS", ready.to_string()); std::env::set_var("HERDR_PROJECTS_SUBMIT_MS", submit.to_string()); } }
fn pass(w: &World) { sw::housekeeping::housekeeping(&w.socket, sw::housekeeping::Force::Yes); }
fn drain() { flush(); sw::runner::drain_jobs(&|job, id, extra| { sw::jobs::run_job(job, id, extra); flush(); }); }
fn matching(w: &World, re: &str) -> Vec<String> { let r = regex::Regex::new(re).unwrap(); calls(w).into_iter().filter(|c| r.is_match(c)).collect() }
fn coord_world() -> World {
    let w = world_with(Opts { repo: true, settings: json!({ "coordinator.agent": "claude", "review.when": "never" }), ..Default::default() });
    let home = w.base.join("home-coord");
    std::fs::create_dir_all(&home).unwrap();
    unsafe { std::env::set_var("HERDR_PROJECTS_COORDINATOR_DIR", &home) };
    w
}
fn agents(w: &World) -> Vec<Value> { w.state().arr("panes").iter().filter(|p| p.at("agent").is_object()).map(|p| p.at("agent").clone()).collect() }
fn get(w: &World) -> Obj { state::get(&w.socket).unwrap_or_default() }
fn last_lines(a: &Value) -> Vec<String> { a.arr("prompts").iter().map(|p| text(p).rsplit('\n').next().unwrap_or("").to_string()).collect() }
fn the_coordinator(w: &World, f: impl FnOnce(&mut Value)) { w.patch(|s| { if let Some(p) = s["panes"].as_array_mut().unwrap().iter_mut().find(|p| p.at("agent").is_object()) { f(&mut p["agent"]); } }); }
fn ticks(w: &World, n: usize) { for _ in 0..n { let _ = std::fs::remove_file(sw::store::marks::stamp_file("coordinator-default")); tick(&w.socket); drain(); } }

#[test]
fn the_reviewers_brief_and_the_coordinators_messages_get_enter_again_never_the_words_again() {
    {
        let w = world_with(Opts { repo: true, settings: json!({ "reviewer.agent": "codex" }), ..Default::default() });
        waits(0, 0); // (a world clears the waits: set after it)
        w.patch(|s| s["slowStart"] = json!(99)); // every program started here swallows its first enter
        w.script(json!({ "worker": [{ "files": { "f.txt": "v1\n" }, "commit": "v1", "note": "Did v1." }] }));
        let id = w.add("Reviewed");
        delegate(&id);
        drain();
        pass(&w); // the worker's own enter
        let done = settle(&id);
        assert_eq!(done.s("stage"), "approval", "{}", done.at("failure"));
        let rpane = matching(&w, r"^agent start \S+-r ")[0].split(' ').nth(6).unwrap().to_string();
        assert_eq!(matching(&w, r"^agent prompt ").iter().filter(|c| c.split(' ').nth(2) == Some(&rpane)).count(), 1);
        assert_eq!(matching(&w, r"^agent send-keys ").iter().filter(|c| c.split(' ').nth(2) == Some(&rpane)).map(|c| c.rsplit(' ').next().unwrap().to_string()).collect::<Vec<_>>(), vec!["enter"]);
    }
    let cw = coord_world();
    waits(0, 0);
    cw.patch(|s| s["slowStart"] = json!(99));
    send(&cw.root, "first", false, "", &cw.socket).unwrap(); drain();
    let a = &agents(&cw)[0];
    assert_eq!(last_lines(a), vec!["first"]);
    assert_eq!(a.at("keys"), &json!(["enter"]));
    assert!(!get(&cw).b("error"));
    // Words that stay in its box whatever is pressed: said so, taken off the list, and not typed again.
    cw.patch(|s| { s["swallow"] = json!(1); s["deaf"] = json!(99); });
    the_coordinator(&cw, |a| a["status"] = json!("idle"));
    send(&cw.root, "second", false, "", &cw.socket).unwrap(); drain();
    assert!(get(&cw).s("error").contains("sits unsent in its box"), "{:?}", get(&cw));
    assert_eq!(get(&cw).arr("queue").len(), 0);
    assert_eq!(matching(&cw, r"^agent prompt").len(), 2);
}

#[test]
fn the_coordinator_words_typed_into_a_turn_it_was_in_the_middle_of_get_their_enter_once_it_rests() {
    let w = coord_world();
    waits(60000, 0);
    w.script(json!({ "coordinator": [{ "status": "working" }, { "status": "working" }] }));
    send(&w.root, "first", false, "", &w.socket).unwrap(); drain();
    assert_eq!(agents(&w)[0].s("status"), "working");
    w.patch(|s| s["swallow"] = json!(1));
    send(&w.root, "BUSY MESSAGE", false, "", &w.socket).unwrap(); drain();
    assert!(agents(&w)[0].s("box").contains("BUSY MESSAGE"));
    assert!(get(&w).at("typed").b("busy"));
    ticks(&w, 3);
    assert!(agents(&w)[0].at("keys").is_null(), "it works: nothing says yet whether the words went in");
    w.patch(|s| { let seq = s.i("seq") + 1; s["seq"] = json!(seq); });
    let seq = w.state().i("seq");
    the_coordinator(&w, |a| { a["status"] = json!("idle"); a["seq"] = json!(seq); a["completion"] = json!(seq); });
    ticks(&w, 4);
    let a = &agents(&w)[0];
    assert_eq!((a.at("keys"), a.at("box").is_null()), (&json!(["enter"]), true));
    assert_eq!(last_lines(a), vec!["first", "BUSY MESSAGE"]);
    assert!(!get(&w).b("typed") && !get(&w).b("error"), "{:?}", get(&w));
}
