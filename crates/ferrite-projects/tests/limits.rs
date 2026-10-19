//! Limits on time and tokens (limit.time, limit.tokens): a run that reaches its limit is stopped once, its work kept
//! and nothing merged ("stopped at its 2h limit"); continue, a higher limit, a reply or "take it as it is" carry on.
use crate::common;
use common::*;
use serde_json::json;
use sw::json::*;

fn at_work(w: &World, settings: serde_json::Value) -> String {
    w.script(json!({ "worker": [{ "status": "working" }, { "status": "working" }, { "status": "working" }, { "status": "working" }, { "status": "working" }, { "status": "working" }], "reviewer": [{ "verdict": { "meets": true, "text": "Looked at it.", "findings": [] } }] }));
    let id = w.add("Long work");
    delegate_with(&id, settings);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "working");
    sw::store::marks::set_token(&id, "agent", "tok", 0).unwrap();
    id
}
fn tick(w: &World, now: i64) { let carry = |id: &str| { let _ = sw::actions::limits::carry_on(id, true); }; sw::limits::tick::limits_tick(&sw::flow::live::board_for(&w.socket), now, Some(&carry)); }
fn pane(w: &World, id: &str, status: &str) { set_status(w, task(id).at("link").s("pane"), status); }

#[test]
fn limit_time_stops_a_run_once_keeps_its_work_and_continue_or_a_higher_limit_carry_on() {
    let w = world_with(Opts { repo: true, settings: json!({ "merge.by": "itself" }), ..Default::default() });
    let id = at_work(&w, json!({ "limit.time": "2h" }));
    std::fs::write(std::path::Path::new(task(&id).s("worktree")).join("half.txt"), "half done\n").unwrap();
    let t0 = sw::store::runs::run_began(&task(&id));
    assert_eq!(sw::limits::shown::limits_of(&task(&id), t0 + 30 * 60000), Some(json!({ "time": { "limit": "2h", "used": 30 * 60000 } })));
    assert_eq!(sw::limits::shown::limits_of(&task(&id), t0 + 100 * 60000).unwrap().s("near"), "1h40m of its 2h");
    tick(&w, t0 + 119 * 60000); assert_eq!(task(&id).s("stage"), "working");
    // an agent that shows a dialog waits for you: that wait is not stopped
    pane(&w, &id, "blocked"); tick(&w, t0 + 130 * 60000); assert_eq!(task(&id).s("stage"), "working"); pane(&w, &id, "working");
    tick(&w, t0 + 140 * 60000);
    let r = row(&id);
    assert_eq!((r.t("stage"), r.t("state"), r.t("note"), r.at("limitStop").t("what"), r.at("limitStop").t("limit"), r.at("limitStop").t("role")),
        ("stopped".into(), "blocked".into(), "stopped at its 2h limit".into(), "time".into(), "2h".into(), "agent".into()));
    assert!(!agent_of(&w, r.at("link").s("pane")).arr("keys").is_empty(), "its turn was interrupted");
    assert_eq!(read(&std::path::Path::new(r.s("worktree")).join("half.txt")), "half done\n");
    assert_eq!(sw::actions::finish_line::details(&id).unwrap().at("limits").at("stopped").s("line"), "stopped at its 2h limit");
    // it never fires again by itself, and nothing is checked, reviewed or merged meanwhile
    let n = prompts(&w, r.at("link").s("pane")).len();
    for m in [141, 200, 900] { tick(&w, t0 + m * 60000); }
    settle(&id);
    assert_eq!((task(&id).t("stage"), prompts(&w, r.at("link").s("pane")).len()), ("stopped".into(), n));
    // a higher limit carries on by itself, from the time the run had taken
    sw::store::settings::change::set_value("limit.time", &json!("3h"), &sw::store::settings::lookup::Scope::task(&task(&id))).unwrap();
    tick(&w, sw::clock::now() + 5000);
    let r = task(&id);
    assert_eq!((r.t("stage"), r.at("limitStop").is_null(), prompts(&w, r.at("link").s("pane")).len()), ("working".into(), true, n + 1));
    assert!(regex::Regex::new(r"reached its limit \(2h for one run\)[^\n]*The limit was raised: continue the task where you left off\.").unwrap().is_match(prompts(&w, r.at("link").s("pane")).last().unwrap()));
    // "continue": the run has its limit once more
    tick(&w, sw::store::runs::run_began(&task(&id)) + 4 * 3600000); assert_eq!(task(&id).s("stage"), "stopped");
    assert!(sw::actions::finish_line::retry(&id).unwrap().msg.contains("told the agent to continue"));
    assert_eq!(task(&id).s("stage"), "working");
    assert!(prompts(&w, r.at("link").s("pane")).last().unwrap().contains("The human said to carry on"));
    // "take it as it is": through the check and the review like any finished turn
    tick(&w, sw::store::runs::run_began(&task(&id)) + 6 * 3600000); assert_eq!(task(&id).s("stage"), "stopped");
    let said = sw::actions::finish_line::finish_anyway(&id).unwrap().msg;
    assert!(said.contains("taking the work as it is (after 6h)"), "{said}");
    assert_eq!(task(&id).s("stage"), "checking");
    sw::runner::JOBS.with(|j| j.borrow_mut().clear());
}
