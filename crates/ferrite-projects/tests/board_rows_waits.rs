//! A row where the board waits on its way (for GitHub's checks, its turn to merge, its check's turn) has the board's
//! own ◇, never a moon (only a moon says someone works), on the terminal board and the web Board alike, in the colour
//! it had; and a merge that waits for GitHub's checks says how far they are (`2 of 3 passed`), each check once, never
//! their names.
use crate::common;
use crate::pull_requests_checks::{merged, pr_world};
use common::board::open; // Ferrite: not `web` (ADR 0031)
use common::*;
use serde_json::{json, Value};
use sw::actions::finish_line;
use sw::board::theme::Theme;
use sw::board::view::fields::mark;
use sw::json::*;
use sw::land::checks::waiting_words;
use sw::shown::ink::Ink;
use sw::shown::states::{as_row, status_ink};

fn check(name: &str, status: &str, conclusion: &str, flow: &str) -> Value { json!({ "__typename": "CheckRun", "name": name, "status": status, "conclusion": conclusion, "workflowName": flow }) }
fn run(name: &str, status: &str, conclusion: &str) -> Value { let mut c = check(name, status, conclusion, name); c["run"] = json!(true); c }
fn names(n: &[&str]) -> Vec<String> { n.iter().map(|s| s.to_string()).collect() }
fn with(r: &Obj, more: Value) -> Obj { let mut o = r.clone(); for (k, v) in more.as_object().unwrap() { o.insert(k.clone(), v.clone()); } o }

#[test]
fn waiting_for_github_checks_says_how_far_they_are_each_check_once_whatever_its_case() {
    let done = check("lint", "COMPLETED", "SUCCESS", "CI");
    let three = [check("build", "IN_PROGRESS", "", "CI"), done.clone(), check("test", "COMPLETED", "SUCCESS", "CI"), check("Build", "QUEUED", "", "CI"), done.clone()];
    assert_eq!(waiting_words(&three, &names(&["build"]), &[]), "Waiting for GitHub checks · 2 of 3 passed");
    // A workflow `Check` and its job `check`, each run for the push and for the pull request: one check, not four.
    let check_check = [run("Check", "COMPLETED", "SUCCESS"), check("check", "COMPLETED", "SUCCESS", "Check"), run("Check", "IN_PROGRESS", ""), check("check", "IN_PROGRESS", "", "Check")];
    assert_eq!(waiting_words(&check_check, &names(&["Check"]), &[]), "Waiting for GitHub checks · 0 of 1 passed");
    // A workflow stands for its jobs until they are listed, and for the ones still to come once those listed passed.
    assert_eq!(waiting_words(&[], &[], &names(&["CI"])), "Waiting for GitHub checks · 0 of 1 passed");
    assert_eq!(waiting_words(&[run("CI", "IN_PROGRESS", ""), check("build", "IN_PROGRESS", "", "CI")], &names(&["CI", "build"]), &[]), "Waiting for GitHub checks · 0 of 1 passed");
    assert_eq!(waiting_words(&[done.clone()], &[], &names(&["CI"])), "Waiting for GitHub checks · 1 of 2 passed");
    // A workflow named as one of its jobs: the job that passed counts as passed, whatever the workflow does.
    let ci = [run("CI", "IN_PROGRESS", ""), check("CI", "COMPLETED", "SUCCESS", "CI"), check("lint", "IN_PROGRESS", "", "CI")];
    assert_eq!(waiting_words(&ci, &names(&["CI", "lint"]), &[]), "Waiting for GitHub checks · 1 of 2 passed");
    // Its listed jobs all passed and the workflow is not through: one passed, more to come.
    assert_eq!(waiting_words(&ci[..2], &names(&["CI"]), &[]), "Waiting for GitHub checks · 1 of 2 passed");
    // A check gone from the list is still waited for, and counted.
    assert_eq!(waiting_words(&[done], &names(&["deploy"]), &[]), "Waiting for GitHub checks · 1 of 2 passed");
    assert_eq!(waiting_words(&[], &[], &[]), "Waiting for GitHub checks to start");
}

#[test]
fn a_merge_waiting_for_github_checks_has_the_boards_own_mark_and_says_how_far_they_are_on_both_boards() {
    let w = pr_world(json!({}));
    let id = to_approval(&w, "Waits for GitHub");
    let head = task(&id).t("prHead");
    let listed = [check("build", "IN_PROGRESS", "", "CI"), check("lint", "COMPLETED", "SUCCESS", "CI"), check("test", "COMPLETED", "SUCCESS", "CI"), check("Build", "IN_PROGRESS", "", "CI")];
    w.gh_says(json!({ "view": { "state": "OPEN", "headRefOid": head, "statusCheckRollup": listed, "comments": [], "reviews": [], "author": { "login": "me" } } }));
    finish_line::approve(&id, None, false).unwrap();
    let t = settle(&id);
    assert_eq!((t.s("stage"), t.s("msg")), ("merging", "Waiting for GitHub checks · 2 of 3 passed"), "{}", t.at("failure")); assert!(!merged(&w));
    assert_eq!(finish_line::details(&id).unwrap().at("checksWaitingFor"), &json!(["build"]), "the names, each once, are in `details`");
    let r = as_row(&row(&id));
    let theme = Theme::default_dark(true);
    assert_eq!((mark(&theme, &r).text(), status_ink(&r)), ("◇".to_string(), Ink::Yellow), "nobody works on it: the board's own mark, no moon");
    let mut b = open(&w.root, 30);
    let lines = b.screen(120);
    let at = lines.iter().position(|l| l.contains("Waits for GitHub") && !l.contains("herdr-projects")).unwrap_or_else(|| panic!("{}", lines.join("\n")));
    assert!(lines[at].contains('◇') && !lines[at].contains('◐'), "{}", lines[at]);
    assert!(lines[at + 1].trim_start_matches([' ', '▌']).starts_with("Waiting for GitHub checks · 2 of 3 passed") && !lines[at + 1].contains("uild"), "{}", lines[at + 1]);
    // Ferrite: the web board's part is not taken (crates/web is not: ADR 0031).
    assert_eq!(mark(&Theme::web(), &r).text(), "◇");
    // Once the checks have passed the board merges: its own step still.
    let merging = with(&r, json!({ "autoMerge": false, "msg": "Merging" }));
    assert_eq!((mark(&theme, &merging).text(), mark(&Theme::web(), &merging).text()), ("◇".to_string(), "◇".to_string()));
}

#[test]
fn every_other_wait_of_the_board_has_its_own_mark_too_and_a_cut_off_turn_its_moon() {
    let theme = Theme::default_dark(true);
    let base = obj(json!({ "id": "x/1", "title": "Waits", "state": "working", "agent": true }));
    let merging = with(&base, json!({ "stage": "merging" }));
    assert_eq!(mark(&theme, &merging).text(), "◇", "the board merges it: its own step");
    for (wait, ink, what) in [
        (with(&merging, json!({ "lineAt": 5, "msg": "Waiting to merge: x/2 goes first" })), Ink::Yellow, "its turn to merge"),
        (with(&merging, json!({ "checksWait": { "since": 5 }, "autoMerge": true, "msg": "Waiting for GitHub checks · could not read them (no network)" })), Ink::Yellow, "GitHub's checks it cannot read just now"),
        // the board's own check, waiting or running, is in its light grey (docs/check-progress.md)
        (with(&base, json!({ "stage": "checking", "check": { "state": "waiting" }, "msg": "Waiting for its turn · 2 ahead" })), Ink::Check, "its check's turn"),
    ] {
        assert_eq!((mark(&theme, &wait).text(), mark(&Theme::web(), &wait).text(), status_ink(&wait)), ("◇".to_string(), "◇".to_string(), ink), "{what}");
    }
    // An agent whose turn was cut off, waiting to try again, is still the one on the task: its moon.
    let cut = with(&base, json!({ "stage": "reviewing", "reviewRound": 1, "waiting": { "who": "reviewer", "reason": "model is at capacity", "until": 9, "tries": 1 } }));
    assert_eq!(mark(&theme, &cut).text(), "◐");
}
