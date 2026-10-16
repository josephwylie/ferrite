//! The review gate (review.mustFix, review.rounds): what counts as critical, what a gate lets through, how the result
//! reads, what a later round may still find, and a review that fails the gate going straight back up to its rounds.
use crate::common;
use common::*;
use serde_json::{json, Value};
use sw::gate as g;
use sw::json::*;

fn finding(severity: &str, subject: &str, extra: Value) -> Value { let mut f = json!({ "severity": severity, "subject": subject, "where": "f.txt", "proof": "ran it" }); for (k, v) in extra.as_object().cloned().unwrap_or_default() { f[k] = v; } f }
fn review_of(meets: bool, findings: Vec<Value>, gate: &str, earlier: Option<Vec<Value>>) -> Value {
    let found = g::read_findings(&Value::Array(findings));
    let (list, left) = g::settle(Some(meets), &found, earlier.as_deref(), &[]);
    let mut rv = obj(json!({ "meets": meets, "text": "T.", "findings": list, "left": left }));
    g::stamp(&mut rv, gate);
    Value::Object(rv)
}

#[test]
fn the_boards_rules_what_counts_as_critical_what_a_gate_lets_through_and_how_it_reads() {
    let mixed = || vec![finding("minor", "A name is unclear", json!({})), finding("minor", "A comment is stale", json!({})), finding("trivial", "A note", json!({}))];
    assert_eq!(["critical", "major", "minor", "trivial", "off"].map(|gt| review_of(true, mixed(), gt, None).at("pass").clone()), [json!(true), json!(true), json!(false), json!(false), json!(true)]);
    assert_eq!(g::result(&review_of(true, mixed(), "major", None), ""), "Pass · 2 minor, 1 trivial");
    assert_eq!(g::result(&review_of(true, vec![finding("critical", "Deletes the data", json!({})), finding("major", "Crashes on an empty file", json!({}))], "major", None), ""), "Fail · 1 critical, 1 major");
    assert_eq!(g::result(&review_of(true, vec![], "major", None), ""), "Pass");
    assert_eq!(review_of(true, vec![], "off", None).s("gate"), "major");
    // `meets: false` with nothing listed is a critical finding of the board's own, at every gate
    let unmet = review_of(false, vec![], "critical", None);
    assert_eq!(unmet.at("pass"), &json!(false));
    assert_eq!(unmet.arr("findings").iter().map(|f| (f.t("severity"), f.b("board"), f.t("subject"))).collect::<Vec<_>>(), vec![("critical".into(), true, "The change does not do what the task asked.".into())]);
    assert_eq!(g::result(&unmet, ""), "Fail · 1 critical");
    assert_eq!(review_of(false, vec![finding("critical", "Does nothing", json!({}))], "major", None).arr("findings").len(), 1, "already said: not said twice");
    let below = review_of(false, vec![finding("minor", "A name is unclear", json!({}))], "major", None);
    assert_eq!((below.at("pass"), g::result(&below, "")), (&json!(true), "Pass · 1 minor".to_string()));
    // a round whose findings are all faults from before the task never sends the work back
    let all_old = review_of(false, vec![finding("critical", "The old importer drops rows", json!({ "preexisting": true })), finding("major", "An old page is wrong", json!({ "preexisting": true }))], "major", None);
    assert_eq!((all_old.at("pass"), g::sendable(&all_old), g::result(&all_old, "")), (&json!(true), false, "Pass · 2 pre-existing".to_string()));
    // a critical finding with an empty proof is treated as major
    let claim = review_of(true, vec![finding("critical", "Might lose data", json!({ "proof": "" }))], "critical", None);
    assert_eq!((claim.arr("findings")[0].s("severity"), claim.at("pass")), ("major", &json!(true)));
    // a pre-existing finding never fails the gate, whatever its severity
    let old = review_of(true, vec![finding("critical", "The old importer drops rows", json!({ "preexisting": true }))], "trivial", None);
    assert_eq!((old.at("pass"), g::result(&old, ""), g::sendable(&old)), (&json!(true), "Pass · 1 pre-existing".to_string(), false));
    // a very long list loses its mildest findings, never a serious one
    let mut many: Vec<Value> = (0..150).map(|n| finding("trivial", &format!("Note {n}"), json!({}))).collect();
    many.push(finding("critical", "Deletes the data", json!({})));
    let long = review_of(true, many, "major", None);
    assert_eq!((long.arr("findings").len(), long.arr("findings").last().unwrap().s("severity"), long.at("pass"), long.i("left")), (100, "critical", &json!(false), 51));
    // a severity the board does not know is read as major, a finding without a subject is no finding
    assert_eq!(g::read_findings(&json!([{ "severity": "HIGH", "subject": "x" }, { "severity": "minor" }, "junk"])).iter().map(|f| f.t("severity")).collect::<Vec<_>>(), vec!["major"]);
    // a later round cannot grow the list with new minor or major ones; an open one comes back under its id
    let first = review_of(true, vec![finding("major", "Crashes", json!({}))], "major", None);
    let earlier = first.arr("findings").to_vec();
    let second = review_of(true, vec![finding("major", "Crashes still", json!({ "id": "F1" })), finding("minor", "New nit", json!({})), finding("critical", "New and proven", json!({}))], "major", Some(earlier));
    assert_eq!(second.arr("findings").iter().map(|f| (f.t("id"), f.t("severity"))).collect::<Vec<_>>(), vec![("F1".into(), "major".into()), ("F2".into(), "critical".into())]);
}

#[test]
fn a_review_that_fails_the_gate_goes_straight_back_up_to_its_rounds_then_waits_for_you() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.rounds": 2 }), ..Default::default() });
    let bad = |id: Option<&str>, subject: &str| { let mut f = json!({ "severity": "major", "subject": subject, "where": "f.txt", "proof": "cat f.txt" }); if let Some(id) = id { f["id"] = json!(id); } f };
    w.script(json!({ "worker": [{ "files": { "f.txt": "1\n" }, "commit": "1", "note": "v1" }, { "files": { "f.txt": "2\n" }, "commit": "2", "note": "v2" }, { "files": { "f.txt": "3\n" }, "commit": "3", "note": "v3" }],
        "reviewer": [{ "verdict": { "meets": true, "text": "Bad.", "findings": [bad(None, "f.txt holds the wrong number")] } }, { "verdict": { "meets": true, "text": "Better.", "findings": [bad(Some("F1"), "f.txt still holds the wrong number")] } }, { "verdict": { "meets": true, "text": "Still not it.", "findings": [bad(Some("F1"), "f.txt still holds the wrong number")] } }] }));
    assert_eq!(sw::store::settings::lookup::setting("review.mustFix", None), json!("major"));
    let id = w.add("Loop");
    delegate(&id);
    let mut t = settle(&id);
    for _ in 0..3 { if t.s("stage") != "approval" { t = settle(&id); } }
    assert_eq!((t.t("stage"), t.i("reviewRounds"), t.at("review").at("pass").clone()), ("approval".into(), 2, json!(false)), "the cap is reached with the gate still failing: it waits for you");
    assert!(t.s("msg").starts_with("Approve to merge? Review: Fail · 1 major."), "{}", t.s("msg"));
    assert_eq!(prompts(&w, t.at("link").s("pane")).len(), 3);
    assert_eq!(sw::store::reviews::review_round(&t), 3);
}

#[test]
fn stop_sets_the_stage_before_esc_and_a_late_working_event_cannot_undo_it() {
    let w = world();
    w.script(json!({ "worker": [{ "status": "working" }] }));
    let id = w.add("Long work");
    delegate(&id);
    assert_eq!(settle(&id).s("stage"), "working");
    sw::actions::talk::stop(&id).unwrap();
    let t = task(&id);
    assert_eq!(t.s("stage"), "stopped");
    assert_eq!(agent_of(&w, t.at("link").s("pane")).at("keys"), &json!(["esc"]));
    assert_eq!((row(&id).t("state"), row(&id).t("note")), ("idle".into(), "Stopped".into()));
    settle(&id);
    assert_eq!(task(&id).s("stage"), "stopped", "the idle that follows esc starts nothing");
    w.script(json!({ "worker": [{ "note": "Resumed and done." }] }));
    sw::actions::talk::tell(&id, "carry on", false).unwrap();
    assert_ne!(settle(&id).s("stage"), "stopped");
}
