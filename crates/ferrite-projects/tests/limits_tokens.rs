//! limit.tokens on a task at work: counted when a turn ends, the stop before anything more is spent, your word that
//! gives the limit once more, a time limit read when a run starts, a review's tokens owed and counted later, a cut-off
//! turn at the limit, and limit.time stopping a review too.
use crate::common;
use common::*;
use serde_json::{json, Value};
use std::path::Path;
use sw::json::*;
use sw::limits::{reached::tokens_over, run::used_of};
use sw::store::runs::{run_began, run_time};
use sw::store::settings::{change::set_value, lookup::Scope};

fn line(file: &Path, o: Value) { use std::io::Write; if let Some(d) = file.parent() { std::fs::create_dir_all(d).unwrap(); } std::fs::OpenOptions::new().create(true).append(true).open(file).unwrap().write_all(format!("{o}\n").as_bytes()).unwrap(); }
fn prompt(file: &Path, at: i64) { line(file, json!({ "type": "user", "uuid": format!("u{at}"), "timestamp": sw::clock::iso(at), "message": { "role": "user", "content": "Go on." } })); }
fn reply(file: &Path, at: i64, tokens: i64, text: &str) { line(file, json!({ "type": "assistant", "uuid": format!("a{at}"), "timestamp": sw::clock::iso(at), "message": { "id": format!("msg_{at}"), "role": "assistant", "stop_reason": "end_turn", "usage": { "input_tokens": 10, "cache_creation_input_tokens": 20, "cache_read_input_tokens": tokens - 40, "output_tokens": 10 }, "content": [{ "type": "text", "text": text }] } })); }
/// The agent in the task's pane moves to `status`; `complete`: its turn completed, as herdr says it.
fn pane(w: &World, id: &str, status: &str, complete: bool) {
    let p = task(id).at("link").t("pane");
    w.patch(|s| { let seq = s.i("seq") + 1; s["seq"] = json!(seq); for x in s["panes"].as_array_mut().unwrap() { if x.s("pane_id") == p { x["agent"]["status"] = json!(status); x["agent"]["seq"] = json!(seq); if complete { x["agent"]["completion"] = json!(seq); } } } });
    on_pane(w, &p);
}
fn reported(id: &str, file: &Path) { sw::turn::turn_ended(id, "agent", "tok", 0, &[json!({ "session_id": "s1", "transcript_path": file.display().to_string() }).to_string()]); }
fn reviewers(w: &World) -> usize { let r = regex::Regex::new(r"-r(-\d+)?$").unwrap(); w.state().arr("panes").iter().filter(|p| r.is_match(p.at("agent").s("name"))).count() }
fn at_work(w: &World, settings: Value) -> String {
    w.script(json!({ "worker": vec![json!({ "status": "working" }); 6], "reviewer": [{ "verdict": { "meets": true, "text": "Looked at it.", "findings": [] } }] }));
    let id = w.add("Long work");
    delegate_with(&id, settings);
    assert_eq!(settle(&id).s("stage"), "working");
    sw::store::marks::set_token(&id, "agent", "tok", 0).unwrap();
    id
}
fn tick(w: &World, now: i64, carry: bool) { let c = |id: &str| { let _ = sw::actions::limits::carry_on(id, true); }; sw::limits::tick::limits_tick(&sw::flow::live::board_for(&w.socket), now, if carry { Some(&c) } else { None }); }
fn put(id: &str, k: &str, v: Value) { sw::store::tasks::patch(id, |t| { t.put(k, v); }).unwrap(); }

#[test]
fn limit_tokens_counted_when_a_turn_ends_stops_before_more_is_spent_and_your_word_gives_it_the_limit_once_more() {
    let w = world();
    let id = at_work(&w, json!({ "limit.tokens": "500k" }));
    std::fs::write(Path::new(task(&id).s("worktree")).join("f.txt"), "v1\n").unwrap();
    // while it runs the count is kept for the panel, and the row says it from near the limit on
    let file = w.base.join("conversation.jsonl");
    let now = sw::clock::now();
    prompt(&file, now - 5000); reply(&file, now - 4000, 410000, "Half way.");
    put(&id, "transcript", json!(file.display().to_string()));
    tick(&w, now + 60000, false);
    let r = row(&id);
    assert_eq!(r.at("spent").f("total"), 410000.0);
    let tk = r.at("limits").at("tokens");
    assert_eq!((tk.t("limit"), tk.f("used"), tk.t("kind"), r.at("limits").t("near")), ("500k".into(), 410000.0, "claude".into(), "410k of 500k tokens".into()));
    // the turn ends over the limit (one turn can pass it): stopped, nothing checked or reviewed
    let at = sw::clock::now();
    prompt(&file, at - 1000); reply(&file, at, 200000, "Done.");
    pane(&w, &id, "done", true); reported(&id, &file);
    let r = settle(&id);
    assert_eq!((r.t("stage"), r.t("msg"), r.at("limitStop").t("what"), r.at("limitStop").f("used"), reviewers(&w), r.i("reviewRound")), ("stopped".into(), "stopped at 500k tokens".into(), "tokens".into(), 610000.0, 0, 0), "{}", r.at("failure"));
    assert_eq!(row(&id).s("state"), "blocked");
    assert_eq!(read(&Path::new(r.s("worktree")).join("f.txt")), "v1\n");
    tick(&w, sw::clock::now() + 120000, true); settle(&id);
    assert_eq!(task(&id).s("stage"), "stopped", "not again by itself");
    // "continue": the count starts from where it stopped, so the next turn is not stopped at once
    sw::actions::limits::carry_on(&id, false).unwrap();
    let r = task(&id);
    assert_eq!((r.t("stage"), r.at("limitStop").is_null(), r.at("spent").f("base"), used_of(&r)), ("working".into(), true, 610000.0, 0.0));
    let at = sw::clock::now(); prompt(&file, at - 1000); reply(&file, at, 100000, "Done.");
    assert_eq!(tokens_over(&id, "agent"), None);
    assert_eq!(used_of(&task(&id)), 100000.0);
    assert_eq!(sw::actions::finish_line::details(&id).unwrap().at("limits").at("tokens").f("total"), 710000.0);
    reply(&file, at + 1, 400000, "Done.");
    assert_eq!(tokens_over(&id, "agent"), Some(("500k".to_string(), 500000.0)), "and at the limit again, it would stop again");

    // A time limit is read when a run starts: set while one is under way, it is the next run's, not this one's.
    // (Also in a run's first seconds, before anything looked at it: the change itself keeps what the run started with.)
    sw::store::tasks::patch(&id, |t| { t.del("runLimit"); }).unwrap();
    set_value("limit.time", &json!("1s"), &Scope::task(&task(&id))).unwrap();
    assert_eq!(task(&id).at("runLimit"), &json!({ "from": run_began(&task(&id)), "time": "none" }));
    tick(&w, run_began(&task(&id)) + 2000, true); assert_eq!(task(&id).s("stage"), "working");
    set_value("limit.time", &json!("60s"), &Scope::task(&task(&id))).unwrap();
    tick(&w, run_began(&task(&id)) + 3600000, true);
    assert_eq!((task(&id).t("stage"), run_time(&task(&id)), row(&id).at("limits").at("time").is_null()), ("working".into(), "none".into(), true));
    sw::actions::talk::tell(&id, "And one more thing.", false).unwrap();
    assert_eq!(run_time(&task(&id)), "60s", "a new run: now it holds");
    tick(&w, run_began(&task(&id)) + 61000, true);
    assert_eq!(task(&id).s("msg"), "stopped at its 60s limit");

    // limit.time holds for the reviewer too: a review that took its time is stopped, and carrying on reviews again
    sw::actions::talk::tell(&id, "Go on.", false).unwrap();
    let began = sw::clock::now();
    sw::store::tasks::patch(&id, |t| { t.put("stage", "reviewing"); t.put("reviewAt", began); }).unwrap(); // its review is under way
    assert_eq!(sw::limits::shown::limits_of(&task(&id), began + 60000).unwrap().at("time").i("used"), 60000);
    tick(&w, began + 31 * 60000, true);
    let r = row(&id);
    assert_eq!((r.t("stage"), r.t("state"), r.t("note"), r.at("limitStop").t("role"), reviewers(&w)), ("stopped".into(), "blocked".into(), "stopped at its 60s limit".into(), "reviewer".into(), 0));
    // carry on: checked and reviewed again, a fresh run (the pipeline is started for it)
    assert!(sw::actions::limits::carry_on(&id, false).unwrap().msg.contains("it is reviewed again"));
    let r = task(&id);
    let jobs: Vec<String> = sw::runner::JOBS.with(|j| j.borrow().iter().map(|x| x.0.clone()).collect());
    assert_eq!((r.t("stage"), r.at("limitStop").is_null(), jobs), ("checking".into(), true, vec!["pipeline".to_string()]));
    sw::runner::JOBS.with(|j| j.borrow_mut().clear());
}

#[test]
fn a_reviews_tokens_owed_before_a_limit_count_once_one_is_set_and_a_cut_off_turn_at_the_limit_stops() {
    let w = world();
    // A limit set after a review ended still counts that review: its conversation was written down, and is read now.
    let dir = std::path::PathBuf::from(std::env::var("CLAUDE_CONFIG_DIR").unwrap()).join("projects").join("x");
    let id = at_work(&w, json!({}));
    sw::store::tasks::patch(&id, |t| { t.put("reviewSession", "rev-1"); t.obj_mut("link").put("reviewer", json!({ "kind": "claude" })); }).unwrap();
    reply(&dir.join("rev-1.jsonl"), sw::clock::now(), 300000, "Done.");
    let cwd = sw::store::repos::cwd_of(&task(&id)).display().to_string();
    assert_eq!(sw::limits::spent::count(&id, "reviewer", 2000).unwrap().at("spent"), &json!({ "owed": [{ "kind": "claude", "session": "rev-1", "file": "", "cwd": cwd }] }));
    sw::store::tasks::patch(&id, |t| { t.put("reviewSession", Value::Null); t.obj_mut("link").put("reviewer", Value::Null); }).unwrap();
    set_value("limit.tokens", &json!("200k"), &Scope::task(&task(&id))).unwrap();
    assert_eq!(tokens_over(&id, "agent"), Some(("200k".to_string(), 300000.0)));
    assert!(!task(&id).at("spent").has("owed"));
    // A turn the provider cut off, on a task at its limit: it is not tried again by itself. It stops.
    let cut = w.base.join("cut.jsonl");
    let at = sw::clock::now();
    prompt(&cut, at - 1000); reply(&cut, at - 500, 50000, "Working on it.");
    line(&cut, json!({ "type": "assistant", "uuid": format!("e{at}"), "timestamp": sw::clock::iso(at), "isApiErrorMessage": true, "error": "overloaded_error", "apiErrorStatus": 529, "message": { "id": format!("err_{at}"), "role": "assistant", "content": [{ "type": "text", "text": "API Error: 529 Overloaded" }] } }));
    pane(&w, &id, "done", true); reported(&id, &cut);
    let r = settle(&id);
    assert_eq!((r.t("stage"), r.t("msg"), r.at("waiting").is_null(), r.at("limitStop").f("used")), ("stopped".into(), "stopped at 200k tokens".into(), true, 350000.0), "{}", r.at("failure"));
}
