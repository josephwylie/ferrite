//! The fallback agents, what the first ports left out: herdr's sidebar and the task's details name the agent that took
//! over, the panel's note says who was passed over when nobody is left, and after you step in the list starts again.
use crate::common;
use common::*;
use serde_json::json;
use sw::json::*;
use sw::store::handovers::{next_fallback, nobody_left, on_now};

fn has(text: &str, re: &str) -> bool { regex::Regex::new(re).unwrap().is_match(text) }

#[test]
fn the_agent_that_took_over_is_named_in_the_sidebar_and_the_details_and_the_panel_says_who_was_passed_over() {
    let w = cut_world("30,30", json!({ "worker.fallback": ["codex", "pi"] }));
    w.script(json!({ "worker": [{ "files": { "half.txt": "half\n" }, "commit": "half the work", "ends": "claude-usage-limit-made" }, { "status": "working" }] }));
    let id = w.add("Runs out");
    delegate(&id);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "working", "{}", t.at("failure"));
    // the sidebar's row names the agent that is on it now
    let e = sw::flow::sidebar::entry_now(&t, sw::clock::now());
    assert!(e.place.starts_with("agent codex · "), "{e:?}");
    // `details` keeps who did what, in the words the board says it
    let d = sw::actions::finish_line::details(&id).unwrap();
    assert_eq!(d.arr("handovers").iter().map(|h| (h.t("role"), h.t("from"), h.t("to"), h.t("words"))).collect::<Vec<_>>(),
        vec![("worker".to_string(), "claude".to_string(), "codex".to_string(), "codex took over from claude: out of allowance until 17:00".to_string())]);
    // with nobody left the panel's note says who was passed over
    sw::store::tasks::patch(&id, |t| { t.put("fallbacks", json!({ "worker": nobody_left("worker.fallback", &["pi is not installed here".to_string()]) })); }).unwrap();
    let mut b = common::board::open(&w.root, 70);
    b.press(&["alt+t"]);
    let panel = b.text();
    assert!(has(&panel, r" Note +worker\.fallback: nobody left to take over \(pi is not installed here\)"), "{panel}");
}

#[test]
fn after_you_step_in_the_list_is_gone_through_again_from_its_start() {
    let w = cut_world("30", json!({ "worker.fallback": "claude, codex, pi" }));
    w.script(json!({ "worker": [{ "ends": "claude-model-not-found" }, { "ends": "codex-unauthorized" }, { "ends": "pi-unknown-provider" }] }));
    let id = w.add("Nobody can");
    delegate(&id);
    assert_eq!(settle(&id).s("stage"), "failed");
    w.script(json!({ "worker": [{ "status": "working" }] }));
    sw::actions::talk::tell(&id, "try again", false).unwrap();
    let t = task(&id);
    let setting = sw::store::settings::lookup::setting("worker.fallback", Some(&t));
    let list = sw::store::settings::values::list_of(&setting);
    let tried: Vec<String> = t.arr("tried").iter().map(text).collect();
    let agent = text(&sw::store::settings::lookup::setting("worker.agent", Some(&t)));
    assert_eq!(next_fallback(&list, &on_now(&t, &agent), &tried, |_| String::new()).0, "claude");
}
