//! Every review a task had, in its panel: how many, a history of how each went, the latest under its own name beside
//! the list, every one full screen (the newest first, with what it found by how bad it is), nothing dropped on a narrow
//! full screen, a short side panel that keeps where the task stands and the latest review, and a review that still
//! stood after main was merged in.
use crate::common;
use crate::review_kit;
use common::*;
use review_kit::*;
use serde_json::{json, Value};
use sw::board::keys::key_name;
use sw::json::*;

fn spiral(v: &V) -> Obj {
    let now = v.now;
    with(&with(&v.pr(), json!({ "reviewRound": 2 })), json!({ "reviewRound": 3, "review": { "score": 4, "recommend": true, "text": "Fine now.", "stale": false }, "reviews": [
        { "round": 1, "at": now - 7200000, "score": 2, "recommend": false, "text": "Crashes on start.", "problems": ["game.js: null plane."], "checks": ["Ran it."], "limitations": [], "head": "1111111aaaa" },
        { "round": 2, "at": now - 3600000, "error": "the reviewer finished without a verdict" },
        { "round": 3, "at": now - 60000, "score": 4, "recommend": true, "text": "Fine now.", "problems": [], "checks": ["Ran the tests: all pass."], "limitations": ["Not tried on a phone."], "head": "abcdef12345" }] }))
}
fn tagged() -> Value { json!({ "meets": true, "gate": "major", "pass": true, "text": "Works.", "findings": [{ "id": "F1", "severity": "minor", "subject": "A name is unclear", "where": "game.js", "proof": "" }, { "id": "F2", "severity": "minor", "subject": "A comment is stale", "where": "sky.js", "proof": "" }] }) }
fn failing() -> Value { json!({ "meets": false, "gate": "major", "pass": false, "text": "No fix.", "left": 1, "findings": [{ "id": "F1", "severity": "critical", "subject": "The plane never takes off", "where": "game.js", "proof": "ran it" }, { "id": "F2", "severity": "major", "subject": "Crashes without a sky", "where": "sky.js", "proof": "ran it" }, { "id": "F3", "severity": "minor", "subject": "A name is unclear", "where": "game.js", "proof": "" }, { "id": "F4", "severity": "major", "subject": "The old menu flickers", "where": "menu.js", "proof": "", "preexisting": true, "task": "proj/7" }] }) }
fn merged(a: Value, b: Value) -> Value { let mut o = a; for (k, x) in b.as_object().unwrap() { o[k] = x.clone(); } o }

#[test]
fn every_review_the_task_had_on_both_shapes_of_the_panel() {
    let _w = world();
    let mut v = V::new();
    let spiral = spiral(&v);
    m(&v.page(&spiral, false), r"\n Reviews · 3 ─+\n History {6}2/5 no → no verdict → 4/5 yes\n Review 3 {5}✓ 4/5, says merge · 1m ago\n[ ▌] {13}Fine now\.\n");
    no(&v.page(&spiral, false), r"Crashes on start|the newest first|Review [12] ");
    m(&v.page(&spiral, false), &format!(r"\n\n {} shows the whole task$", regex::escape(&key_name("panel")))); // beside the list: only the latest, and where the rest is
    let whole = v.page(&spiral, true);
    m(&whole, r"\n Reviews · 3 ─+\n History {6}2/5 no → no verdict → 4/5 yes\n\n Review 3 {5}✓ 4/5, says merge · 1m ago · at abcdef1\n[ ▌] {13}Fine now\.\n Checked {6}• Ran the tests: all pass\.\n Unverified {3}• Not tried on a phone\.\n\n Review 2 {5}– No verdict · 1h ago\n[ ▌] {13}the reviewer finished without a verdict\n\n Review 1 {5}✗ 2/5, says don't merge · 2h ago · at 1111111\n[ ▌] {13}Crashes on start\.\n Problems {5}• game\.js: null plane\.\n Checked {6}• Ran it\.\n");
    let at = |s: &str| whole.find(s).unwrap_or_else(|| panic!("{s:?} not in:\n{whole}"));
    assert!(at("◆ ready to approve") < at("Reviews · 3 ") && at("\n Review 3 ") < at("\n Review 1 ") && at("\n Review 1 ") < at("\n Settings ") && at("\n Settings ") < at("\n Merge "), "where it stands, every review, then the settings");
    // a narrow full screen drops nothing of a review: a label is a line of its own there, and its value is under it
    let thin = v.panel(&spiral, 24, true, 0).join("\n");
    m(&thin, r"\n Review 3\n[ ▌] {2}✓ 4/5, says merge\n[ ▌] {2}1m ago · at abcdef1\n[ ▌] {2}Fine now\.\n Checked\n[ ▌] {2}• Ran the tests: all\n[ ▌] {4}pass\.\n");
    m(&thin, r"\n Review 2\n[ ▌] {2}– No verdict\n[ ▌] {2}1h ago\n"); m(&thin, r"\n Review 1\n[ ▌] {2}✗ 2/5, says don't\n[ ▌] {2}merge\n[ ▌] {2}2h ago · at 1111111\n");
    assert!(thin.split('\n').all(|l| sw::text::width(l) <= 24 && !l.ends_with('…')), "nothing runs over, nothing is cut:\n{thin}");
    // a side panel too short for the settings still has where the task stands and the latest review, as its one row;
    // with no room for both, the change goes before the review's row
    let short = with(&spiral, json!({ "worktree": "", "description": "Long. ".repeat(200) }));
    assert_eq!(v.panel(&short, 46, false, 12), vec![" #1 Build a flying game", "", " Status ─────────────────────────────────────", " State        ◆ ready to approve · 3rd review", "              0s ago", " On it        claude", " Change       PR#12", "", " Reviews · 3 ────────────────────────────────", " Review 3     ✓ 4/5, says merge · 1m ago", "              Fine now.", " alt+t shows the whole task"]);
    assert_eq!(v.panel(&short, 46, false, 8), vec![" #1 Build a flying game", " Status ─────────────────────────────────────", " State        ◆ ready to approve · 3rd review", "              0s ago", " Change       PR#12", " Review 3     ✓ 4/5, says merge", "", " alt+t shows the whole task"]);
    assert_eq!(v.panel(&short, 46, false, 4), vec![" #1 Build a flying game", " State        ◆ ready to approve · 3rd review", "              0s ago", " alt+t shows the whole task"]);
    // many reviews in a narrow panel: the history is wrapped, and every one of them is there
    let many: Vec<Value> = (1..=12).map(|n| json!({ "round": n, "score": 2, "recommend": false, "text": "Still broken." })).collect();
    m(&v.panel(&with(&spiral, json!({ "reviews": many })), 44, false, 0).join("\n"), r"\n Reviews · 12 ─+\n History {6}2/5 no → 2/5 no → 2/5 no\n[ ▌] {13}→ 2/5 no → 2/5 no → 2/5 no\n[ ▌] {13}→ 2/5 no → 2/5 no → 2/5 no\n[ ▌] {13}→ 2/5 no → 2/5 no → 2/5 no\n Review 12 {4}✗ 2/5, says don't merge\n");
}

#[test]
fn a_gated_review_in_the_history_and_one_that_still_stood_after_main_was_merged_in() {
    let _w = world();
    let mut v = V::new();
    let now = v.now;
    let gated = with(&v.pr(), json!({ "msg": "Approve to merge? Review: Pass · 2 minor. Built **Skylark**.", "review": tagged() }));
    let lp = with(&gated, json!({ "reviewRound": 3, "reviews": [merged(failing(), json!({ "round": 1, "at": now - 7200000, "head": "1111111aaaa" })), { "round": 2, "at": now - 3600000, "score": 4, "recommend": true, "text": "An old one.", "problems": ["game.js: old."] }, merged(tagged(), json!({ "round": 3, "at": now - 60000, "head": "abcdef12345" }))] }));
    m(&v.page(&lp, false), r"\n Reviews · 3 ─+\n History {6}fail → 4/5 yes → pass\n Review 3 {5}✓ Pass · 2 minor · 1m ago\n[ ▌] {13}Works\.\n");
    no(&v.page(&lp, false), r"Skylark[^\n]*Review:|Review: Pass"); // the board's own words about the review are not said twice
    m(&v.page(&lp, true), r"\n Review 3 {5}✓ Pass · 2 minor · 1m ago · at abcdef1\n[ ▌] {13}Works\.\n Findings {5}minor {5}A name is unclear \(game\.js\) · not required\n[ ▌] {13}minor {5}A comment is stale \(sky\.js\) · not required\n\n Review 2 {5}✓ 4/5, says merge · 1h ago\n[ ▌] {13}An old one\.\n Problems {5}• game\.js: old\.\n\n Review 1 {5}✗ Fail · 1 critical, 1 major, 1 minor, 1 pre-existing · 2h ago\n[ ▌] {13}at 1111111\n[ ▌] {13}No fix\.\n Findings {5}critical {2}The plane never takes off \(game\.js\)\n[ ▌] {13}major {5}Crashes without a sky \(sky\.js\)\n[ ▌] {13}minor {5}A name is unclear \(game\.js\) · not required\n[ ▌] {13}major {5}The old menu flickers \(menu\.js\) · pre-existing, task\n[ ▌] {23}proj/7\n[ ▌] {13}1 smaller finding noticed late, left out\n");
    // a review that still stood after main was merged in says so under its result, and the task is still on that review
    let mut reviews = lp.at("reviews").as_array().unwrap().clone();
    reviews[2]["kept"] = json!([{ "at": now - 3600000, "check": "npm test", "resolved": [] }, { "at": now - 30000, "check": "npm test", "resolved": ["game.js"] }]);
    let stood = with(&lp, json!({ "reviews": reviews }));
    m(&v.page(&stood, false), r"\n Review 3 {5}✓ Pass · 2 minor · 1m ago\n[ ▌] {13}Re-merged with main \(conflicts resolved in 1 file\), tests pass,\n[ ▌] {13}review 3 still stands · 30s ago · 2 times\n[ ▌] {13}Works\.\n");
    m(&v.row(&stood, 120), r" R#3 PR#12 [^\n]*\n[ ▌] {5}Pass · 2 minor$");
}
