//! limit.tokens and what the board does by itself: a review's tokens are the task's too, and at the limit findings
//! do not go back and ready work does not merge, until the limit is higher; reviewed work with nothing to land stops too.
use crate::common;
use common::*;
use serde_json::{json, Value};
use std::path::Path;
use sw::json::*;
use sw::limits::run::used_of;
use sw::store::settings::{change::set_value, lookup::Scope};

fn line(file: &Path, o: Value) { use std::io::Write; if let Some(d) = file.parent() { std::fs::create_dir_all(d).unwrap(); } std::fs::OpenOptions::new().create(true).append(true).open(file).unwrap().write_all(format!("{o}\n").as_bytes()).unwrap(); }
fn prompt(file: &Path, at: i64) { line(file, json!({ "type": "user", "uuid": format!("u{at}"), "timestamp": sw::clock::iso(at), "message": { "role": "user", "content": "Go on." } })); }
fn reply(file: &Path, at: i64, tokens: i64) { line(file, json!({ "type": "assistant", "uuid": format!("a{at}"), "timestamp": sw::clock::iso(at), "message": { "id": format!("msg_{at}"), "role": "assistant", "stop_reason": "end_turn", "usage": { "input_tokens": 10, "cache_creation_input_tokens": 20, "cache_read_input_tokens": tokens - 40, "output_tokens": 10 }, "content": [{ "type": "text", "text": "Done." }] } })); }
fn pane(w: &World, id: &str, status: &str) {
    let p = task(id).at("link").t("pane");
    w.patch(|s| { let seq = s.i("seq") + 1; s["seq"] = json!(seq); for x in s["panes"].as_array_mut().unwrap() { if x.s("pane_id") == p { x["agent"]["status"] = json!(status); x["agent"]["seq"] = json!(seq); x["agent"]["completion"] = json!(seq); } } });
    on_pane(w, &p);
}
fn reported(id: &str, file: &Path) { sw::turn::turn_ended(id, "agent", "tok", 0, &[json!({ "session_id": "s1", "transcript_path": file.display().to_string() }).to_string()]); }
fn tick(w: &World, now: i64) { let c = |id: &str| { let _ = sw::actions::limits::carry_on(id, true); }; sw::limits::tick::limits_tick(&sw::flow::live::board_for(&w.socket), now, Some(&c)); }
/// A reviewer's turn: the review ends having used the tokens of its recorded ending, with these findings.
fn review(fixture: &str, findings: Value) -> Value { json!({ "verdict": { "meets": true, "text": "Looked at it.", "findings": findings }, "ends": fixture }) }
fn at_work(w: &World, settings: Value, reviewer: Vec<Value>) -> String {
    w.script(json!({ "worker": vec![json!({ "status": "working" }); 6], "reviewer": reviewer }));
    let id = w.add("Long work");
    delegate_with(&id, settings);
    assert_eq!(settle(&id).s("stage"), "working");
    sw::store::marks::set_token(&id, "agent", "tok", 0).unwrap();
    id
}
fn n_prompts(w: &World, id: &str) -> usize { prompts(w, task(id).at("link").s("pane")).len() }

#[test]
fn findings_do_not_go_back_and_ready_work_does_not_merge_until_the_limit_is_higher() {
    let w = world_with(Opts { repo: true, settings: json!({ "merge.by": "itself", "reviewer.agent": "claude" }), ..Default::default() });
    let bad = json!([{ "severity": "major", "subject": "Wrong", "where": "f.txt", "proof": "ran it" }]);
    let id = at_work(&w, json!({ "limit.tokens": "350k" }), vec![review("claude-review-60k-made", bad), review("claude-review-60k-again-made", json!([]))]);
    let file = w.base.join("conversation.jsonl");
    let ends = |tokens: i64, body: &str| { std::fs::write(Path::new(task(&id).s("worktree")).join("f.txt"), body).unwrap(); let at = sw::clock::now(); prompt(&file, at - 1000); reply(&file, at, tokens); pane(&w, &id, "done"); reported(&id, &file); };
    // The agent's turn ends under the limit, so its work is checked and reviewed. The reviewer's tokens are the
    // task's too, and take it over: the findings wait with the task instead of going back by themselves.
    ends(300000, "v1\n");
    let r = settle(&id);
    let ls = r.at("limitStop");
    assert_eq!((r.t("stage"), r.t("msg"), ls.t("role"), ls.at("review").clone(), ls.f("used"), r.at("spent").f("total")), ("stopped".into(), "stopped at 350k tokens".into(), "reviewer".into(), json!(true), 360000.0, 360000.0), "{}", r.at("failure"));
    assert_eq!((r.at("review").at("pass"), row(&id).t("state")), (&json!(false), "blocked".into()));
    let n = n_prompts(&w, &id);
    tick(&w, sw::clock::now() + 5000); settle(&id);
    assert_eq!((task(&id).t("stage"), n_prompts(&w, &id)), ("stopped".into(), n));
    // a higher limit carries on by itself: the review goes to the agent, and the count goes on as it was
    set_value("limit.tokens", &json!("500k"), &Scope::task(&task(&id))).unwrap();
    tick(&w, sw::clock::now() + 10000);
    let r = task(&id);
    assert_eq!((r.t("stage"), r.at("limitStop").is_null(), n_prompts(&w, &id), used_of(&r)), ("working".into(), true, n + 1, 360000.0));
    assert!(prompts(&w, r.at("link").s("pane")).last().unwrap().contains("Wrong"));
    // Its repair passes the review, and that review takes the task over its limit again. Also with a merge the board
    // was already on its way to: nothing merges, and the task is not ordinary approval either. It is stopped, under
    // "needs you", with what was reviewed kept.
    ends(100000, "v2\n");
    sw::store::tasks::patch(&id, |t| { t.put("mergeIntent", json!({ "approvedAt": sw::clock::now(), "repairs": 1, "verifiedHead": null, "auto": true })); }).unwrap();
    let r = settle(&id);
    assert_eq!((r.t("stage"), r.t("msg"), r.at("limitStop").at("ready").clone(), r.at("mergeIntent").is_null(), r.at("spent").f("total"), r.at("review").at("pass").clone()), ("stopped".into(), "stopped at 500k tokens".into(), json!(true), true, 520000.0, json!(true)), "{} {}", r.at("failure"), r.s("msg"));
    assert_eq!((sw::notify::event_of(&r), row(&id).t("state")), ("needs you", "blocked".into()));
    sw::housekeeping::housekeeping(&w.socket, sw::housekeeping::Force::Yes); settle(&id);
    assert_eq!(task(&id).s("stage"), "stopped");
    assert!(!w.root.join("f.txt").exists());
    // a higher limit carries on by itself: the work is put up as it was reviewed, and now it may merge by itself
    set_value("limit.tokens", &json!("none"), &Scope::task(&task(&id))).unwrap();
    tick(&w, sw::clock::now() + 20000);
    let r = task(&id);
    assert_eq!((r.t("stage"), r.at("limitStop").is_null()), ("approval".into(), true));
    assert!(r.s("msg").starts_with("Approve to merge? Review: Pass"), "{}", r.s("msg"));
    assert!(sw::finish::auto_merge::green(&r));
    sw::runner::JOBS.with(|j| j.borrow_mut().clear());
}

#[test]
fn reviewed_work_with_nothing_to_land_stops_at_the_limit_too_and_carrying_on_ends_it() {
    let w = world_with(Opts { repo: true, settings: json!({ "merge.by": "itself", "reviewer.agent": "claude" }), ..Default::default() });
    let plain = at_work(&w, json!({ "limit.tokens": "1k", "review.when": "always" }), vec![review("claude-review-1510-made", json!([]))]);
    let said = w.base.join("plain.jsonl");
    let now = sw::clock::now();
    prompt(&said, now - 1000); reply(&said, now, 500); pane(&w, &plain, "done"); reported(&plain, &said);
    let r = settle(&plain);
    assert_eq!((r.t("stage"), r.t("msg"), r.at("limitStop").at("ready").clone(), r.at("spent").f("total"), sw::notify::event_of(&r)), ("stopped".into(), "stopped at 1k tokens".into(), json!(true), 2010.0, "needs you"), "{} {}", r.at("failure"), r.s("msg"));
    assert!(sw::actions::limits::carry_on(&plain, false).unwrap().msg.contains("it is done"));
    let r = task(&plain);
    assert_eq!((r.t("stage"), r.at("limitStop").is_null(), r.s("msg").contains("Pass")), ("done".into(), true, true));
    sw::runner::JOBS.with(|j| j.borrow_mut().clear());
}
