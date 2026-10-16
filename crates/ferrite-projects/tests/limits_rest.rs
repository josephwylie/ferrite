//! limit.time, what the first port left out: the row says nothing until the run nears its limit, a stop is "needs you"
//! in the row and herdr's sidebar, nothing is reviewed while stopped, a raised limit counts from the time the run had
//! taken, and continue by the command, a reply and "take it as it is" by the command each carry on.
use crate::common;
use common::*;
use serde_json::json;
use sw::json::*;
use sw::limits::run::ran_for;
use sw::store::runs::run_began;

fn at_work(w: &World, settings: serde_json::Value) -> String {
    w.script(json!({ "worker": vec![json!({ "status": "working" }); 6], "reviewer": [{ "verdict": { "meets": true, "text": "Looked at it.", "findings": [] } }] }));
    let id = w.add("Long work");
    delegate_with(&id, settings);
    assert_eq!(settle(&id).s("stage"), "working");
    sw::store::marks::set_token(&id, "agent", "tok", 0).unwrap();
    id
}
fn tick(w: &World, now: i64) { let carry = |id: &str| { let _ = sw::actions::limits::carry_on(id, true); }; sw::limits::tick::limits_tick(&sw::flow::live::board_for(&w.socket), now, Some(&carry)); }
fn reviewers(w: &World) -> usize { let r = regex::Regex::new(r"-r(-\d+)?$").unwrap(); w.state().arr("panes").iter().filter(|p| r.is_match(p.at("agent").s("name"))).count() }
fn said(argv: &[&str]) -> String { let a = sw::cli::flags::parse(&argv.iter().map(|s| s.to_string()).collect::<Vec<_>>()); sw::cli::tasks::command(&a.pos[0], &a).map(|(_, s)| s).unwrap_or_else(|e| panic!("{argv:?}: {}", e.msg)) }
fn began(id: &str) -> i64 { run_began(&task(id)) }

#[test]
fn limit_time_a_stop_is_needs_you_nothing_is_reviewed_meanwhile_and_every_way_on_carries_on() {
    let w = world_with(Opts { repo: true, settings: json!({ "merge.by": "itself" }), ..Default::default() });
    let id = at_work(&w, json!({ "limit.time": "2h" }));
    let tree = std::path::PathBuf::from(task(&id).s("worktree"));
    std::fs::write(tree.join("half.txt"), "half done\n").unwrap();
    let t0 = began(&id);
    assert!(row(&id).at("limits").at("near").is_null(), "far from its limit the row says nothing of it");
    tick(&w, t0 + 140 * 60000);
    let r = row(&id);
    assert_eq!(r.s("stage"), "stopped");
    // under "needs you", in the row and in herdr's sidebar
    assert_eq!(sw::notify::event_of(&r), "needs you");
    let e = sw::flow::sidebar_entry::entry_of(&r, "", sw::clock::now());
    assert!(e.stands.contains("needs you"), "{e:?}");
    assert_eq!(e.line, ""); // the row's sentence is the board's, not the sidebar's
    // it never fires again by itself, and nothing is checked, reviewed or merged meanwhile
    let at = task(&id).at("limitStop").at("at").clone();
    for m in [141, 200, 900] { tick(&w, t0 + m * 60000); }
    settle(&id); sw::housekeeping::housekeeping(&w.socket, sw::housekeeping::Force::Yes);
    assert_eq!((task(&id).t("stage"), task(&id).at("limitStop").at("at").clone(), reviewers(&w)), ("stopped".into(), at, 0));
    // a higher limit carries on by itself, from the time the run had taken
    sw::store::settings::change::set_value("limit.time", &json!("3h"), &sw::store::settings::lookup::Scope::task(&task(&id))).unwrap();
    tick(&w, sw::clock::now() + 5000);
    assert_eq!(task(&id).s("stage"), "working");
    let t1 = began(&id);
    assert_eq!(ran_for(&task(&id), t1 + 60000), 141 * 60000);
    tick(&w, t1 + 30 * 60000); assert_eq!(task(&id).s("stage"), "working", "2h50m of 3h");
    tick(&w, t1 + 45 * 60000); assert_eq!(task(&id).at("limitStop").s("limit"), "3h");
    // "continue" (the key, `retry`, or a reply): the run has its limit once more
    assert!(sw::actions::finish_line::retry(&id).unwrap().msg.contains("told the agent to continue"));
    tick(&w, began(&id) + 4 * 3600000); assert_eq!(task(&id).s("stage"), "stopped");
    assert!(said(&["retry", &id]).contains("told the agent to continue"));
    assert_eq!(task(&id).s("stage"), "working");
    assert!(prompts(&w, task(&id).at("link").s("pane")).last().unwrap().contains("The human said to carry on"));
    assert_eq!(ran_for(&task(&id), began(&id) + 1000), 1000);
    tick(&w, began(&id) + 5 * 3600000); assert_eq!(task(&id).s("stage"), "stopped");
    sw::actions::talk::tell(&id, "Carry on, but only the first part.", false).unwrap();
    assert_eq!((task(&id).t("stage"), task(&id).at("limitStop").is_null()), ("working".into(), true));
    // "take it as it is": through the check and the review like any finished turn
    tick(&w, began(&id) + 6 * 3600000); assert_eq!(task(&id).s("stage"), "stopped");
    let s = said(&["finish", &id]);
    assert!(s.contains("taking the work as it is (after 6h)"), "{s}");
    let jobs: Vec<String> = sw::runner::JOBS.with(|j| j.borrow().iter().map(|x| x.0.clone()).collect());
    assert_eq!((task(&id).t("stage"), task(&id).at("limitStop").is_null(), jobs), ("checking".into(), true, vec!["pipeline".to_string()]));
    assert_eq!(read(&tree.join("half.txt")), "half done\n");
    sw::runner::JOBS.with(|j| j.borrow_mut().clear());
}
