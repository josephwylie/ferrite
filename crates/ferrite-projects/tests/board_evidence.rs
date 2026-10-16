//! The task details of a reviewed task with evidence: every review, and every piece of evidence with what it is and
//! its link in full; beside the list the latest review, the first piece, and where the rest is.
use crate::common;
use common::board::open;
use common::*;
use serde_json::json;
use sw::board::keys::key_name;
use sw::json::*;

fn has(text: &str, re: &str) -> bool { regex::Regex::new(re).unwrap().is_match(text) }

#[test]
fn every_review_and_every_piece_of_evidence_shows() {
    let w = world();
    let id = sw::actions::filing::add(&w.root, "Build a flying game", sw::actions::filing::AddOpts { description: "A small game.".into(), ..Default::default() }).unwrap().t("id");
    let now = sw::clock::now();
    sw::store::tasks::update(&id, |t| {
        t.put("reviews", json!([
            { "round": 1, "at": now - 7200000, "score": 2, "recommend": false, "text": "Crashes on start.", "problems": ["game.js: null plane."], "checks": ["Ran it."] },
            { "round": 2, "at": now - 3600000, "error": "the reviewer finished without a verdict" },
            { "round": 3, "at": now - 60000, "score": 4, "recommend": true, "text": "Fine now.", "checks": ["Ran the tests: all pass."] }]));
        t.put("reviewRound", 3);
        t.put("review", json!({ "score": 4, "recommend": true, "text": "Fine now.", "artifacts": [
            { "file": "takeoff.png", "caption": "The plane takes off", "detail": "A screenshot of the first level.", "link": "https://example.test/proj/1/takeoff.png" },
            { "file": "tests.txt", "caption": "The test run", "link": "https://example.test/proj/1/tests.txt" }] }));
        Ok(sw::store::tasks::Change::Write)
    }).unwrap();
    let mut s = open(&w.root, 80);
    s.s.sel = id.clone(); s.load();
    let text = s.screen(120).join("\n");
    assert!(text.contains("Reviews · 3 ─") && text.contains("↗ The plane takes off") && text.contains(&format!("… 1 more: {}", key_name("panel"))), "{text}");
    s.press(&["alt+t"]);
    let text = s.screen(120).join("\n");
    assert!(!has(&text, "view error|could not be shown"));
    assert!(text.starts_with(" Task details  PRJ-1 \"Build a flying game\""));
    assert!(has(&text, r"\n Reviews · 3 ─+ *\n History      2/5 no → no verdict → 4/5 yes *\n"), "{text}");
    for n in 1..=3 { assert!(has(&text, &format!(r"\n Review {n}     [✓✗–] ")), "review {n}: {text}"); }
    assert!(has(&text, r"\n Evidence ─+ *\n ↗ The plane takes off *\n[ ▌]  A screenshot of the first level\. *\n[ ▌]  https://example\.test/proj/1/takeoff\.png *\n *\n ↗ The test run *\n[ ▌]  https://example\.test/proj/1/tests\.txt *\n"), "{text}");
    assert!(text.find("\n Evidence ").unwrap() < text.find("\n Settings ").unwrap());
    s.press(&["esc"]); assert!(!s.s.full);
}
