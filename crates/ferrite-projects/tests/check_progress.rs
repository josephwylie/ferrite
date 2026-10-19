//! The board's own check as a task shows it, worked out: what a check may write, the true count while it runs, the
//! wait for its turn, an answer known at once, a check with no count, and standing still said plainly.
use serde_json::{json, Value};
use sw::check_progress::{check_shown, ran_for, read_report, start_check, step_check, still_after, KNOWN_LINE};
use sw::json::*;

const MIN: i64 = 60000;
const T0: i64 = 1_000_000_000_000;
type S = (String, Option<i64>, Option<i64>);
fn sh(c: &Value, now: i64) -> S { let s = check_shown(c, now); (s.msg, s.progress, s.eta) }
fn said(msg: &str, progress: Option<i64>, eta: Option<i64>) -> S { (msg.into(), progress, eta) }
fn step(c: &Value, r: Value, now: i64) -> Value { step_check(c, Some(&r), now) }
/// A check that has run for `spent`, with what it last reported.
fn running(report: Value, spent: i64, usual: i64, idle: i64) -> S {
    let mut r = json!({ "state": "running" }); for (k, v) in report.as_object().cloned().unwrap() { r[k] = v; }
    let mut c = step(&start_check(T0, usual), r, T0);
    c["movedAt"] = json!(T0 + spent - idle);
    sh(&c, T0 + spent)
}

#[test]
fn what_a_check_may_write_a_json_line_or_plain_words_anything_else_is_not_a_count() {
    assert_eq!(read_report(r#"{"done":31,"total":47,"unit":"tests"}"#), Some(json!({ "state": "running", "done": 31, "total": 47, "before": 0, "unit": "tests" })));
    assert_eq!(read_report(r#"{"done":31,"total":47,"before":40}"#), Some(json!({ "state": "running", "done": 31, "total": 47, "before": 31, "unit": "" }))); // never more than are finished
    assert_eq!(read_report("{\"state\":\"running\",\"done\":3,\"total\":10}\n"), Some(json!({ "state": "running", "done": 3, "total": 10, "before": 0, "unit": "" })));
    assert_eq!(read_report("31/47"), Some(json!({ "state": "running", "done": 31, "total": 47, "before": 0, "unit": "" })));
    assert_eq!(read_report("31 of 47 tests\n"), Some(json!({ "state": "running", "done": 31, "total": 47, "before": 0, "unit": "tests" })));
    assert_eq!(read_report(r#"{"done":50,"total":47}"#), Some(json!({ "state": "running", "done": 47, "total": 47, "before": 0, "unit": "" }))); // never past the end
    assert_eq!(read_report(r#"{"state":"waiting","ahead":2}"#), Some(json!({ "state": "waiting", "ahead": 2 })));
    assert_eq!(read_report(r#"{"state":"known"}"#), Some(json!({ "state": "known" })));
    assert_eq!(read_report(r#"{"state":"running"}"#), Some(json!({ "state": "running" })));
    for junk in ["", "half", r#"{"done":3}"#, r#"{"done":3,"total":0}"#, r#"{"state":"odd","done":1,"total":2}"#, "[1,2]", r#"{"done":"x","total":4}"#, r#"{"done":1,"tot"#] { assert_eq!(read_report(junk), None, "{junk}"); }
}

#[test]
fn while_it_runs_the_bar_is_the_true_count_and_the_minutes_left_come_from_full_runs_here() {
    assert_eq!(running(json!({ "done": 31, "total": 47, "unit": "tests" }), 2 * MIN, 5 * MIN, 0), said("Running the check · tests 31 of 47", Some(65), Some(3)));
    assert_eq!(running(json!({ "done": 0, "total": 47, "unit": "tests" }), 0, 5 * MIN, 0), said("Running the check · tests 0 of 47", Some(0), Some(5)));
    assert_eq!(running(json!({ "done": 3, "total": 10, "unit": "" }), 30000, 40000, 0), said("Running the check · 3 of 10", Some(30), Some(1))); // never under a minute while work is left
    // not 100% before the last one is finished; finished, nothing is left
    assert_eq!(running(json!({ "done": 199, "total": 200 }), 0, MIN, 0).1, Some(99));
    assert_eq!(running(json!({ "done": 47, "total": 47, "unit": "tests" }), MIN, 5 * MIN, 0), said("Running the check · tests 47 of 47", Some(100), Some(0)));
    // past the usual time, or with no run timed yet: at the pace of this run; too little done to tell: no minutes
    assert_eq!(running(json!({ "done": 30, "total": 40 }), 6 * MIN, 5 * MIN, 0).2, Some(2));
    assert_eq!(running(json!({ "done": 10, "total": 40 }), 2 * MIN, 0, 0).2, Some(6));
    assert_eq!(running(json!({ "done": 1, "total": 40 }), 2 * MIN, 0, 0).2, None);
    // a run that found most files passed already has only the rest to do
    let c = step(&start_check(T0, 10 * MIN), json!({ "state": "running", "done": 40, "total": 50, "before": 40, "unit": "tests" }), T0);
    assert_eq!(c.i("from"), 40);
    assert_eq!(sh(&step(&c, json!({ "state": "running", "done": 45, "total": 50, "before": 40, "unit": "tests" }), T0 + MIN), T0 + MIN), said("Running the check · tests 45 of 50", Some(90), Some(1)));
}

#[test]
fn while_it_waits_its_turn_it_says_how_many_are_ahead_with_minutes_and_no_bar() {
    let c = step(&start_check(T0, 2 * MIN), json!({ "state": "waiting", "ahead": 2 }), T0 + 1000);
    assert_eq!(sh(&c, T0 + 1000), said("Waiting for its turn · 2 ahead", None, Some(6))); // two runs ahead and its own
    assert_eq!(sh(&c, T0 + 1000 + MIN).2, Some(5)); // the time since the line last moved comes off
    let c = step(&c, json!({ "state": "waiting", "ahead": 1 }), T0 + 3 * MIN);
    assert_eq!(sh(&c, T0 + 3 * MIN), said("Waiting for its turn · 1 ahead", None, Some(4)));
    assert_eq!(sh(&step(&start_check(T0, 0), json!({ "state": "waiting", "ahead": 1 }), T0), T0), said("Waiting for its turn · 1 ahead", None, None)); // no run timed yet
    // its turn comes: the run's own time starts then
    let c = step(&c, json!({ "state": "running", "done": 0, "total": 10, "unit": "tests" }), T0 + 4 * MIN);
    assert_eq!(c.i("runAt"), T0 + 4 * MIN);
    assert_eq!(sh(&c, T0 + 5 * MIN), said("Running the check · tests 0 of 10", Some(0), Some(1)));
    assert_eq!(ran_for(&step(&c, json!({ "state": "running", "done": 10, "total": 10, "unit": "tests" }), T0 + 6 * MIN), T0 + 6 * MIN), Some(2 * MIN)); // the wait is not part of a run
}

#[test]
fn an_answer_known_at_once_has_no_bar_and_a_check_with_no_count_shows_its_time_against_last_time() {
    assert_eq!(sh(&step(&start_check(T0, MIN), json!({ "state": "known" }), T0), T0), said(KNOWN_LINE, None, None));
    assert_eq!(ran_for(&step(&start_check(T0, MIN), json!({ "state": "known" }), T0), T0 + 5), None); // and is not timed as a run
    let plain = |spent: i64, usual: i64| sh(&start_check(T0, usual), T0 + spent);
    assert_eq!(plain(20000, 0), said("Running the check · under 1m", None, None));
    assert_eq!(plain(7 * MIN + 5000, 0), said("Running the check · 7m", None, None));
    assert_eq!(plain(2 * MIN, 5 * MIN), said("Running the check · 2m of about 5m", None, Some(3)));
    assert_eq!(plain(6 * MIN, 5 * MIN), said("Running the check · 6m, usually 5m", None, None)); // past it: no minutes made up
    assert_eq!(ran_for(&start_check(T0, 0), T0 + 90000), Some(90000));
    // a run that had part of its work done already is not what a full run takes
    assert_eq!(ran_for(&step(&start_check(T0, 0), json!({ "state": "running", "done": 5, "total": 9, "before": 4 }), T0), T0 + 1000), None);
    assert_eq!(ran_for(&step(&start_check(T0, 0), json!({ "state": "running", "done": 5, "total": 9 }), T0), T0 + 1000), Some(1000)); // a first report that comes late is still a full run
}

#[test]
fn standing_still_far_longer_than_it_should_is_said_plainly() {
    assert_eq!((still_after(0), still_after(30000), still_after(4 * MIN)), (10 * MIN, 3 * MIN, 12 * MIN));
    assert_eq!(running(json!({ "done": 31, "total": 47, "unit": "tests" }), 14 * MIN, 4 * MIN, 13 * MIN), said("The check stands still · nothing finished for 13m · tests 31 of 47", Some(65), None));
    // every file finished and the command still runs long after: said too, with no "0m left" that never comes true
    assert_eq!(running(json!({ "done": 47, "total": 47, "unit": "tests" }), 20 * MIN, 5 * MIN, 16 * MIN), said("The check has not ended · all finished 16m ago · tests 47 of 47", Some(100), None));
    assert!(running(json!({ "done": 31, "total": 47, "unit": "tests" }), 14 * MIN, 4 * MIN, 11 * MIN).0.starts_with("Running the check")); // slow is not still
    let c = step(&start_check(T0, MIN), json!({ "state": "waiting", "ahead": 2 }), T0);
    assert_eq!(sh(&c, T0 + 4 * MIN), said("Waiting for its turn unusually long · nothing moved for 4m · 2 ahead", None, None));
    assert_eq!(sh(&step(&c, json!({ "state": "waiting", "ahead": 1 }), T0 + 4 * MIN), T0 + 4 * MIN), said("Waiting for its turn · 1 ahead", None, Some(2))); // it moved
    assert_eq!(sh(&start_check(T0, 2 * MIN), T0 + 13 * MIN), said("The check runs far longer than usual · 13m, usually 2m", None, None));
    // something unreadable changes nothing
    let c = step(&start_check(T0, 0), json!({ "state": "running", "done": 2, "total": 4 }), T0);
    assert_eq!(step_check(&c, read_report("half a li").as_ref(), T0 + MIN), c);
}
