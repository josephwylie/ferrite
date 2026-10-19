//! The gate's rules, what the first port left out: a reviewer's own findings decide without a line of the board's, old
//! faults never send work back, a claim with a blank proof, the cut of a very long list, the words for the agent and
//! the pull request, and a review from before the gate keeping its own words and rule.
use serde_json::{json, Value};
use sw::gate as g;
use sw::json::*;

fn finding(severity: &str, subject: &str, extra: Value) -> Value { let mut f = json!({ "severity": severity, "subject": subject, "where": "f.txt", "proof": "ran it" }); for (k, v) in extra.as_object().cloned().unwrap_or_default() { f[k] = v; } f }
fn review_of(meets: bool, findings: Vec<Value>, gate: &str, earlier: Option<Vec<Value>>) -> Value {
    let (list, left) = g::settle(Some(meets), &g::read_findings(&Value::Array(findings)), earlier.as_deref(), &[]);
    let mut rv = obj(json!({ "meets": meets, "text": "T.", "findings": list, "left": left }));
    g::stamp(&mut rv, gate);
    Value::Object(rv)
}
fn re(r: &str, s: &str) -> bool { regex::Regex::new(r).unwrap().is_match(s) }
fn none() -> Value { json!({}) }
fn n(rv: &Value, f: impl Fn(&Value) -> bool) -> usize { rv.arr("findings").iter().filter(|x| f(x)).count() }

#[test]
fn the_reviewers_findings_decide_old_faults_never_send_work_back_and_a_long_list_keeps_what_matters() {
    // where the reviewer gave findings, those and their severities decide: the board adds no line of its own beside them
    let below = review_of(false, vec![finding("minor", "A name is unclear", none())], "major", None);
    assert_eq!(below.arr("findings").iter().map(|f| (f.t("severity"), f.b("board"))).collect::<Vec<_>>(), vec![("minor".to_string(), false)]);
    let real = review_of(false, vec![finding("major", "Crashes on an empty file", none())], "major", None);
    assert_eq!(real.arr("findings").iter().map(|f| f.t("subject")).collect::<Vec<_>>(), vec!["Crashes on an empty file"]);
    assert_eq!(g::result(&real, ""), "Fail · 1 major");
    // a round whose findings are all faults from before the task never sends the work back, whatever `meets` says
    let all_old = review_of(false, vec![finding("critical", "The old importer drops rows", json!({ "preexisting": true })), finding("major", "An old page is wrong", json!({ "preexisting": true }))], "major", None);
    assert_eq!(n(&all_old, |f| f.b("board")), 0);
    let later_old = review_of(false, vec![finding("major", "The old importer drops rows", json!({ "preexisting": true }))], "major", Some(vec![json!({ "id": "F1", "severity": "major", "subject": "Fixed since", "where": "f.txt", "proof": "ran it" })]));
    assert_eq!((later_old.at("pass"), g::sendable(&later_old)), (&json!(true), false));
    // a critical finding whose proof is only blank is not taken for a shown one
    assert_eq!(review_of(true, vec![finding("critical", "Might lose data", json!({ "proof": " " }))], "major", None).at("pass"), &json!(false));
    // the cut comes after the rules: critical claims nobody showed and old faults go before a proven critical one does
    let mut many: Vec<Value> = (0..80).map(|i| finding("critical", &format!("A claim {i}"), json!({ "proof": "" }))).collect();
    many.extend((0..80).map(|i| finding("critical", &format!("An old fault {i}"), json!({ "preexisting": true }))));
    many.push(finding("critical", "Deletes the data", none())); many.push(finding("minor", "A nit", none()));
    let buried = review_of(true, many, "critical", None);
    assert_eq!((buried.arr("findings").len(), buried.at("pass")), (162, &json!(false)));
    assert!(buried.arr("findings").iter().any(|f| f.s("subject") == "Deletes the data"));
    assert_eq!(n(&buried, |f| f.s("severity") == "major"), 80);
    // what was there before the task is never cut, however long the list: each one is a note the human should see
    let mut olds: Vec<Value> = (0..130).map(|i| finding("minor", &format!("An old fault {i}"), json!({ "preexisting": true }))).collect();
    olds.extend((0..120).map(|i| finding("trivial", &format!("Note {i}"), none())));
    olds.push(finding("critical", "Deletes the data", none()));
    let olds = review_of(true, olds, "major", None);
    assert_eq!((n(&olds, |f| f.b("preexisting")), n(&olds, |f| !f.b("preexisting")), olds.i("left"), olds.at("pass")), (130, 100, 21, &json!(false)));
    let mut nits: Vec<Value> = (0..20).map(|i| finding("minor", &format!("Nit {i}"), none())).collect();
    nits.push(finding("critical", "Deletes the data", none()));
    assert_eq!(review_of(true, nits, "major", None).at("pass"), &json!(false));
}

#[test]
fn the_words_for_the_agent_and_the_pull_request_and_a_review_from_before_the_gate() {
    // required and optional: what the agent is told, and what the pull request says
    let mut rv = review_of(true, vec![finding("major", "Crashes on an empty file", none()), finding("minor", "A name is unclear", none()), finding("major", "The old importer drops rows", json!({ "preexisting": true }))], "major", None);
    rv["findings"][2]["task"] = json!("proj/9");
    let msg = g::review_message(&rv, false);
    assert!(re(r"Result: Fail · 1 major, 1 minor, 1 pre-existing\.\n\nT\.\n\nRequired: fix each of these\.\n- \[major\] Crashes on an empty file \(f\.txt\)\n  Proof: ran it\n\nOptional: [^\n]*\n- \[minor\] A name is unclear \(f\.txt\)\n  Proof: ran it\n\nNot yours to fix: [^\n]*\n- \[major\] The old importer drops rows \(f\.txt\) → task proj/9\n\nFix what is required", &msg), "{msg}");
    let pr = g::pr_comment(&rv, 2);
    assert!(re(r"^\*\*Review 2 · Fail · 1 major, 1 minor, 1 pre-existing\*\*[\s\S]*\*\*Required\*\*\n- \*\*major\*\*: Crashes[\s\S]*\*\*Not required \(below the gate\)\*\*\n- \*\*minor\*\*: A name is unclear[\s\S]*\*\*Not required \(there before this change\)\*\*\n- \*\*major\*\*: The old importer[\s\S]*Task proj/9\.", &pr), "{pr}");
    // by hand, a review that passed still has something to send: then nothing in it is optional
    let mixed = vec![finding("minor", "A name is unclear", none()), finding("minor", "A comment is stale", none()), finding("trivial", "A note", none())];
    assert!(re(r"Required: fix each of these\.\n- \[minor\] A name is unclear", &g::review_message(&review_of(true, mixed, "major", None), true)));
    // a review from before the gate keeps its own words and its own rule (4 or 5, and not "don't merge")
    let before = [json!({ "score": 4, "recommend": true, "text": "Fine." }), json!({ "score": 4, "recommend": false, "text": "No." }), json!({ "score": 3, "recommend": true, "text": "Hm." }), json!({ "score": 5, "text": "Good." })];
    assert_eq!(before.iter().map(|r| (g::passes(r), g::result(r, "merge"), g::sentence(r, "merge"))).collect::<Vec<_>>(), vec![
        (true, "4/5, says merge".to_string(), "Reviewed 4/5, says merge.".to_string()), (false, "4/5, says don't merge".into(), "Reviewed 4/5, says don't merge.".into()),
        (false, "3/5, says merge".into(), "Reviewed 3/5, says merge.".into()), (true, "5/5".into(), "Reviewed 5/5.".into())]);
    let old = g::review_message(&json!({ "score": 3, "text": "Hm.", "problems": ["a.js: null"] }), false);
    assert!(re(r"rated it 3 out of 5\.\n\nHm\.\n\nProblems it found:\n- a\.js: null", &old), "{old}");
    // The board's review words come off a task's message, old and new alike. (The old test kept "Approve to merge? "
    // with a capture group; strip_sentence takes the whole sentence off, the question with it.)
    for m in ["Approve to merge? Reviewed 4/5, says merge. Did it.", "Approve to merge? Review: Pass · 2 minor, 1 pre-existing. Did it.", "Approve to merge? Review: Fail · 1 critical. Did it.", "Approve to merge? Review: Pass. Did it."] {
        assert_eq!(g::strip_sentence(m), "Did it.", "{m}"); assert!(g::has_sentence(m));
    }
}

#[test]
fn the_board_shows_a_message_without_its_review_words_and_keeps_the_question_as_the_old_board_did() {
    // The old board's reviewNote: `msg.replace(SENTENCE, "$1")`, the "Approve to merge? " stays; a task not at an agent keeps its message whole.
    for m in ["Approve to merge? Reviewed 4/5, says merge. Did it.", "Approve to merge? Review: Pass · 2 minor, 1 pre-existing. Did it.", "Approve to merge? Review: Fail · 1 critical. Did it.", "Approve to merge? Review: Pass. Did it."] {
        let r = obj(json!({ "agent": "claude", "stage": "approval", "msg": m }));
        assert_eq!(sw::shown::review_words::review_note(&r), "Approve to merge? Did it.", "{m}");
        assert_eq!(sw::shown::review_words::review_note(&obj(json!({ "stage": "todo", "msg": m }))), m);
    }
}
