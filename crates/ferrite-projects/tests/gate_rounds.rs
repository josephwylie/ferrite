//! The gate on real rounds: minor findings merge at the default gate, `meets: false` goes back, a pre-existing fault
//! is only a note, round two cannot grow the list, and review.rounds 0 is the manual mode.
use crate::common;
use common::*;
use serde_json::{json, Value};
use sw::gate as g;
use sw::json::*;
use sw::store::reviews::{review_history, review_round};

fn finding(severity: &str, subject: &str, extra: Value) -> Value { let mut f = json!({ "severity": severity, "subject": subject, "where": "f.txt", "proof": "ran it" }); for (k, v) in extra.as_object().cloned().unwrap_or_default() { f[k] = v; } f }
fn verdict(findings: Vec<Value>, extra: Value) -> Value { let mut v = json!({ "meets": true, "text": "Looked at it.", "findings": findings }); for (k, x) in extra.as_object().cloned().unwrap_or_default() { v[k] = x; } json!({ "verdict": v }) }
fn work(n: i64) -> Value { json!({ "files": { "f.txt": format!("{n}\n") }, "commit": format!("v{n}"), "note": format!("Did v{n}.") }) }
fn re(r: &str, s: &str) -> bool { regex::Regex::new(r).unwrap().is_match(s) }
fn settled(id: &str, want: &str) -> Obj { let mut t = settle(id); for _ in 0..3 { if t.s("stage") != want { t = settle(id); } } t }
fn pane_prompts(w: &World, t: &Obj) -> Vec<String> { prompts(w, t.at("link").s("pane")) }
fn ids() -> Vec<String> { sw::store::tasks::list(Default::default()).iter().map(|t| t.t("id")).collect() }

#[test]
fn a_review_with_only_minor_findings_merges_at_the_default_gate_without_another_round() {
    let w = world_with(Opts { repo: true, settings: json!({ "merge.by": "itself" }), ..Default::default() });
    w.script(json!({ "worker": [work(1)], "reviewer": [verdict(vec![finding("minor", "The name could be clearer", json!({})), finding("trivial", "A note", json!({}))], json!({}))] }));
    let id = w.add("Only nits");
    delegate(&id);
    let t = settled(&id, "done");
    assert_eq!(t.s("stage"), "done", "{}", t.at("failure"));
    assert_eq!(read(&w.root.join("f.txt")), "1\n", "merged");
    assert_eq!((review_round(&t), t.i("reviewRounds")), (1, 0));
    assert!(pane_prompts(&w, &t).len() <= 1);
    assert_eq!((t.at("review").at("pass"), t.at("review").s("gate"), g::result(t.at("review"), "")), (&json!(true), "major", "Pass · 1 minor, 1 trivial".to_string()));
}

#[test]
fn a_review_where_meets_is_false_goes_back_even_if_the_reviewer_listed_nothing_else() {
    let w = world();
    w.script(json!({ "worker": [work(1), work(2)], "reviewer": [verdict(vec![], json!({ "meets": false, "text": "Only a test was added, no fix." })), verdict(vec![], json!({}))] }));
    let id = w.add("Fix it");
    delegate(&id);
    let t = settled(&id, "approval");
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure"));
    assert_eq!((review_round(&t), t.i("reviewRounds")), (2, 1));
    let p = pane_prompts(&w, &t);
    assert_eq!(p.len(), 2);
    assert!(re(r"Result: Fail · 1 critical\.\n\nOnly a test was added, no fix\.\n\nRequired: fix each of these\.\n- \[critical\] The change does not do what the task asked\.", &p[1]), "{}", p[1]);
    assert_eq!(review_history(&t).iter().map(|e| (e.i("round"), e.at("meets").clone(), e.at("pass").clone(), e.arr("findings").len())).collect::<Vec<_>>(), vec![(1, json!(false), json!(false), 1), (2, json!(true), json!(true), 0)]);
    assert!(t.s("msg").starts_with("Approve to merge? Review: Pass. Did v2."), "{}", t.s("msg"));
}

#[test]
fn a_pre_existing_fault_is_a_note_in_the_review_no_task_no_extra_round_nothing_else_kept() {
    let w = world();
    let old = finding("critical", "The importer drops the last row", json!({ "where": "import.js", "proof": "ran it on main too", "preexisting": true }));
    w.script(json!({ "worker": [work(1)], "reviewer": [verdict(vec![old.clone()], json!({}))] }));
    let id = w.add("Add a column");
    delegate(&id);
    let t = settled(&id, "approval");
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure"));
    assert_eq!((review_round(&t), pane_prompts(&w, &t).len()), (1, 1), "no extra round");
    assert_eq!((t.at("review").at("pass"), g::result(t.at("review"), "")), (&json!(true), "Pass · 1 pre-existing".to_string()));
    assert_eq!(ids(), vec![id.clone()]);
    assert!(!t.has("suggestions"), "nothing was added, on the board or on the task");
    let mut keys: Vec<String> = t.at("review").arr("findings")[0].as_object().unwrap().keys().cloned().collect(); keys.sort();
    assert_eq!(keys, vec!["id", "preexisting", "proof", "severity", "subject", "where"]);
    assert!(sw::actions::finish_line::send_review(&id).unwrap_err().msg.contains("nothing for the agent to fix"));
    // a later round that fails names it again: the agent is told it is a note and not theirs, the next reviewer that it is known
    let mut again = old.clone(); again["id"] = json!("F1");
    w.script(json!({ "worker": [work(2), work(3)], "reviewer": [verdict(vec![finding("major", "f.txt holds the wrong number", json!({})), old.clone()], json!({})), verdict(vec![again, finding("minor", "The old exporter is slow", json!({ "where": "export.js", "preexisting": true }))], json!({}))] }));
    sw::actions::talk::tell(&id, "Change the number.", false).unwrap();
    let t = settled(&id, "approval");
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure"));
    assert_eq!((review_round(&t), t.at("review").at("pass")), (3, &json!(true)));
    assert_eq!(ids(), vec![id.clone()]);
    assert!(!t.has("suggestions"));
    let last = pane_prompts(&w, &t).last().cloned().unwrap();
    assert!(re(r"(?m)Required: fix each of these\.\n- \[major\] f\.txt holds the wrong number[\s\S]*Not yours to fix:[^\n]*They are notes for the human\. Leave them\.\n- \[critical\] The importer drops the last row \(import\.js\)$", &last), "{last}");
    let ch = Obj::new();
    let change = json!({ "head": "abc", "range": "a..b" });
    let brief = sw::finish::review_brief::review_brief(&t, &sw::finish::review_brief::Brief { text: "", change: Some(&change), to_land: true, ch: &ch, earlier: Some(t.at("review").arr("findings")), check: "" });
    assert!(brief.contains("\nAlready known to the human (they were there before this change): do not report these again, in these words or in others.\n- [critical] The importer drops the last row (import.js)\n- [minor] The old exporter is slow (export.js)\n"), "{brief}");
    // word for word the same finding is one that says the same thing about the same place, whatever its proof
    let with = |k: &str, v: &str| { let mut f = old.clone(); f[k] = json!(v); f };
    assert!(!g::same_fault(&old, &with("subject", "The importer drops the first row")));
    let mut other_proof = with("subject", "the importer drops the last row!"); other_proof["proof"] = json!("other");
    assert!(g::same_fault(&old, &other_proof));
    for (w_, same) in [("import-json.js", false), ("", false), ("Import.js", false), (" import.js ", true)] { assert_eq!(g::same_fault(&old, &with("where", w_)), same, "{w_:?}"); }
    assert!(g::same_fault(&old, &with("proof", "")));
    // a review an older version saved still says where its fault went; a suggestion it named says nothing any more
    assert_eq!(g::went_to(&with("task", "proj/2")), "task proj/2");
    let mut sug = old.clone(); sug["suggested"] = json!({ "on": id, "id": "S1" });
    assert_eq!(g::went_to(&sug), "");
}

#[test]
fn round_two_cannot_grow_the_list_new_minor_and_major_findings_are_left_out_an_open_one_comes_back() {
    let w = world();
    let mut still = finding("major", "Still crashes on an empty file", json!({})); still["id"] = json!("F1");
    w.script(json!({ "worker": [work(1), work(2), work(3)], "reviewer": [
        verdict(vec![finding("major", "Crashes on an empty file", json!({})), finding("minor", "A name is unclear", json!({}))], json!({})),
        verdict(vec![still, finding("major", "A brand new worry", json!({})), finding("minor", "A brand new nit", json!({}))], json!({})),
        verdict(vec![finding("major", "Yet another new worry", json!({})), finding("minor", "Yet another nit", json!({}))], json!({}))] }));
    let id = w.add("Handle files");
    delegate(&id);
    let t = settled(&id, "approval");
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure"));
    assert_eq!((review_round(&t), t.i("reviewRounds")), (3, 2));
    let hist: Vec<(Value, Vec<String>, i64)> = review_history(&t).iter().map(|e| (e.at("pass").clone(), e.arr("findings").iter().map(|f| format!("{} {}", f.s("id"), f.s("severity"))).collect(), e.i("left"))).collect();
    assert_eq!(hist, vec![(json!(false), vec!["F1 major".to_string(), "F2 minor".into()], 0), (json!(false), vec!["F1 major".into()], 2), (json!(true), vec![], 2)]);
    let p = pane_prompts(&w, &t);
    assert!(re(r"Required: fix each of these\.\n- \[major\] Crashes on an empty file \(f\.txt\)\n  Proof: ran it\n\nOptional: [^\n]*\n- \[minor\] A name is unclear", &p[1]), "{}", p[1]);
    assert!(re(r"Result: Fail · 1 major\.[\s\S]*- \[major\] Still crashes on an empty file", &p[2]) && !re(r"brand new|Optional", &p[2]), "{}", p[2]);
    assert_eq!(g::result(t.at("review"), ""), "Pass");
    // a reply from you starts over: the next review is a full one again, and its findings take new numbers
    w.script(json!({ "worker": [work(4)], "reviewer": [verdict(vec![finding("minor", "A late nit", json!({}))], json!({}))] }));
    sw::actions::talk::tell(&id, "One more thing.", false).unwrap();
    let again = settled(&id, "approval");
    assert_eq!(again.at("review").arr("findings").iter().map(|f| (f.t("id"), f.t("severity"))).collect::<Vec<_>>(), vec![("F3".to_string(), "minor".to_string())]);
    assert_eq!((again.at("review").at("pass"), again.at("review").i("left")), (&json!(true), 0));
}

#[test]
fn review_rounds_0_is_the_manual_mode_and_one_tasks_own_bar_and_rounds_send_even_a_minor_finding_back() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.rounds": 0, "merge.by": "itself" }), ..Default::default() });
    w.script(json!({ "worker": [work(1), work(2)], "reviewer": [verdict(vec![finding("critical", "Deletes the data", json!({}))], json!({})), verdict(vec![], json!({}))] }));
    let id = w.add("Manual");
    delegate(&id);
    let t = settled(&id, "approval");
    // it waits for you, with merging by itself on too: a failed review is never merged by itself
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure"));
    assert_eq!((review_round(&t), pane_prompts(&w, &t).len(), t.at("review").at("pass")), (1, 1, &json!(false)));
    assert!(t.s("msg").starts_with("Approve to merge? Review: Fail · 1 critical."), "{}", t.s("msg"));
    let e = sw::flow::sidebar::entry_now(&t, sw::clock::now());
    assert_eq!((e.place.as_str(), e.line.as_str()), ("review failed", ""));
    let refused = sw::actions::finish_line::approve(&id, None, false).unwrap_err().msg;
    assert!(re(r"was not merged: its last review failed \(1 critical\)[\s\S]*approve proj/1 --anyway", &refused), "{refused}");
    assert_eq!(task(&id).s("stage"), "approval", "a plain approve never merges it");
    assert!(sw::actions::finish_line::send_review(&id).unwrap().msg.contains("review sent"), "by hand, as before");
    assert_eq!(task(&id).at("requireApproval"), &json!(false), "the board's own words sent back are not you stepping in");
    let t = settled(&id, "done");
    assert_eq!(review_round(&t), 2);
    let last_pass = if t.at("review").is_object() { t.at("review").at("pass").clone() } else { review_history(&t).last().unwrap().at("pass").clone() };
    assert_eq!(last_pass, json!(true));
    assert_eq!(t.s("stage"), "done", "{}", t.at("failure"));
    // one task with a stricter gate of its own: a minor finding goes back by itself
    w.script(json!({ "worker": [{ "files": { "g.txt": "1\n" }, "commit": "g1", "note": "Did g1." }, { "files": { "g.txt": "2\n" }, "commit": "g2", "note": "Did g2." }], "reviewer": [verdict(vec![finding("minor", "A name is unclear", json!({ "where": "g.txt" }))], json!({})), verdict(vec![], json!({}))] }));
    let strict = w.add("Strict");
    delegate_with(&strict, json!({ "review.mustFix": "minor", "review.rounds": 3 }));
    let s = settled(&strict, "done");
    assert_eq!(review_history(&s).iter().map(|e| (e.t("gate"), e.at("pass").clone())).collect::<Vec<_>>(), vec![("minor".to_string(), json!(false)), ("minor".to_string(), json!(true))]);
    assert_eq!(s.i("reviewRounds"), 1);
}
