//! The task panel has one form for every part, in every state and at every width: a headline on its rule, facts in one
//! column, nothing boxed and nothing cut; what each state shows under Status; the reviews as a history; evidence
//! wrapped whole; the place of a task's own setting wrapped with its value; another machine's settings; and the inks.
use crate::common;
use common::board::{open, Board};
use common::*;
use serde_json::{json, Value};
use sw::board::keys::key_name;
use sw::json::*;
use sw::shown::ink::{Ink, Inked};
use sw::store::settings::change::{set_value, unset_value};
use sw::store::settings::lookup::Scope;
use sw::text::width as wd;

fn has(text: &str, re: &str) -> bool { regex::Regex::new(re).unwrap().is_match(text) }
fn with(base: &Value, more: Value) -> Obj { let mut b = base.clone(); for (k, v) in more.as_object().unwrap() { if v.is_null() { b.as_object_mut().unwrap().remove(k); } else { b[k] = v.clone(); } } obj(b) }
/// The panel of a row as the board draws it: what the loader reads for a row (its settings, its conversation) first,
/// as the board's loader does for the selected task (the panel itself never reads a file).
fn inked(b: &mut Board, r: &Obj, width: usize, full: bool) -> Vec<Inked> { let mut r = r.clone(); if !r.has("snap") { let snap = sw::board::data::snap(&r); r.put("snap", snap); } sw::board::view::panel::panel(&mut b.s, &b.theme, &r, width, full, 0) }
fn lines(b: &mut Board, r: &Obj, width: usize, full: bool) -> Vec<String> { inked(b, r, width, full).iter().map(|l| l.text()).collect() }
fn label(l: &str) -> String { l.chars().skip(1).take(13).collect::<String>().trim_end().to_string() }
fn finding(severity: &str, subject: &str, more: Value) -> Value { let mut f = json!({ "id": "F1", "severity": severity, "subject": subject, "where": "src/store/titles.js:14", "proof": "" }); for (k, v) in more.as_object().unwrap() { f[k] = v.clone(); } f }
const LABELS: [&str; 29] = ["State", "Progress", "Now", "On it", "Change", "Task", "Next step", "Details", "History", "Review 1", "Review 2", "Review 3", "Evidence", "Findings", "Checked", "Unverified", "Project", "Worktree", "Branch", "Agent", "Plan first", "Reviewer", "Review when", "Send back", "Check", "Merge", "Workspace", "Also", "Task id"];

#[test]
fn one_form_for_every_part_in_every_state_and_at_every_width() {
    let w = world();
    let id = sw::actions::filing::add(&w.root, "Max length for task titles", sw::actions::filing::AddOpts { description: "A task title should have a maximum length. A longer one is cut between words.".into(), ..Default::default() }).unwrap().t("id");
    let mut s = open(&w.root, 34);
    let now = sw::clock::now();
    let reviews = json!([
        { "round": 1, "at": now - 7200000, "meets": false, "gate": "major", "pass": false, "text": "A title is cut in the middle of a word.", "findings": [finding("critical", "A title is cut in the middle of a word", json!({})), finding("minor", "The limit is written in two places", json!({})), finding("major", "The old menu flickers", json!({ "preexisting": true, "task": "proj/7" }))], "checks": ["Ran the check."], "limitations": ["Not tried on another machine."], "head": "1111111aaaa" },
        { "round": 2, "at": now - 3600000, "error": "the reviewer finished without a verdict" },
        { "round": 3, "at": now - 60000, "meets": true, "gate": "major", "pass": true, "text": "The change meets the task. A title at the limit is kept whole, and a longer one is cut between words with the rest moved into the description.", "findings": [], "checks": ["Ran the whole check."], "limitations": [], "head": "abcdef12345", "artifacts": [{ "caption": "Open review", "link": "https://example.test/review/1/" }] }]);
    let mut me = Value::Object(task(&id).into_iter().collect()); me["owner"] = json!("me");
    let base = Value::Object(with(&me, json!({ "agent": "claude", "app": "claude", "at": now - 60000, "stat": " 19 files changed, 340 insertions(+), 27 deletions(-)", "pr": "https://github.com/a/b/pull/60", "reviewRound": 3, "reviews": reviews, "review": reviews[2], "said": "Titles longer than 80 characters are cut between words." })).into_iter().collect());
    let states: Vec<(&str, Obj)> = vec![
        ("to do", with(&me, json!({ "state": "idle", "at": now }))),
        ("working", with(&base, json!({ "state": "working", "stage": "working", "progress": 60, "eta": 11, "msg": "Found the cause; now checking every caller of the resize handler for the same mistake", "reviews": [], "review": null, "reviewRound": 0 }))),
        ("reviewing", with(&base, json!({ "state": "working", "stage": "reviewing", "progress": 35, "eta": 6, "msg": "Filing tasks with titles at the limit." }))),
        ("waiting for approval", with(&base, json!({ "state": "blocked", "kind": "permission", "stage": "approval", "prState": "passing", "msg": "Approve to merge? Review: Pass. Done." }))),
        ("failed", with(&base, json!({ "state": "error", "stage": "failed", "prState": "failing", "failure": { "stage": "merge", "phase": "sync", "title": "Conflict with main", "detail": "Conflicting files:\nsrc/board/view/row.js", "agentCanFix": true, "next": "Resolve the conflict." } }))),
        ("done", with(&base, json!({ "state": "done", "stage": "done", "prState": "merged", "prMerged": true }))),
    ];
    let st = |name: &str| states.iter().find(|(n, _)| *n == name).unwrap().1.clone();
    for (name, r) in &states { for (width, full) in [(38, false), (46, false), (58, false), (70, false), (44, true), (80, true), (100, true)] {
        let ls = lines(&mut s, r, width, full);
        let at = format!("{name}, {width} columns{}", if full { ", full screen" } else { "" });
        assert!(ls.iter().all(|l| wd(l) <= width), "{at}: nothing runs over");
        assert!(!ls.iter().any(|l| has(l, "[╭╮╰╯│▎]")), "{at}: no box and no bar");
        assert!(!ls.iter().any(|l| l.ends_with('…') && !l.starts_with(" ↗")), "{at}: nothing is cut with an ellipsis\n{}", ls.join("\n"));
        // A headline is the only line with a rule, and the rule runs to the edge. An empty line stands before each one.
        let heads: Vec<usize> = (0..ls.len()).filter(|n| has(&ls[*n], " ─+$")).collect();
        assert!(heads.len() >= 2 && heads.iter().all(|n| has(&ls[*n], "^ [A-Z][^─]* ─+$") && wd(&ls[*n]) == width - 1 && ls[n - 1].is_empty()), "{at}: every headline on its rule, after an empty line\n{}", ls.join("\n"));
        assert!(!(1..ls.len()).any(|n| ls[n].is_empty() && ls[n - 1].is_empty()), "{at}: never two empty lines");
        // Every fact: its label, then its value in the one column all values start in.
        let facts: Vec<&String> = ls.iter().filter(|l| !has(l, " ─+$") && LABELS.iter().any(|k| l.starts_with(&format!(" {k} ")))).collect();
        assert!(facts.len() >= if full { 8 } else { 1 } && facts.iter().all(|l| has(l, r"^ .{12} \S") && LABELS.contains(&label(l).as_str())), "{at}: every value starts in the same column\n{}", ls.join("\n"));
        for k in if full { vec!["State", "Project", "Agent", "Merge", "Task id"] } else { vec!["State"] } { assert!(facts.iter().any(|l| l.starts_with(&format!(" {k} "))), "{at}: {k}"); }
    } }
    // What each state needs you to see first is under Status, in the same order in every state.
    let mut side = |name: &str| lines(&mut s, &st(name), 70, false);
    let labels = |l: Vec<String>| { let from = l.iter().position(|x| x.starts_with(" Status ")).unwrap() + 1; let end = l.iter().skip(3).position(|x| x.is_empty()).unwrap() + 3; l[from..end].iter().filter(|x| has(x, r"^ \S")).map(|x| label(x)).collect::<Vec<_>>() };
    assert_eq!(labels(side("to do")), vec!["State"]); assert_eq!(labels(side("working")), vec!["State", "Progress", "Now", "On it", "Change"]);
    assert_eq!(labels(side("reviewing")), vec!["State", "Progress", "Now", "Change"]); assert_eq!(labels(side("done")), vec!["State", "On it", "Change"]);
    let t = side("waiting for approval").join("\n"); assert!(has(&t, r"\n State {8}◆ ready to approve · 3rd review · 1m ago\n On it {8}claude\n Change {7}PR#60 · 19 files, \+340 −27\n"), "{t}");
    let t = side("done").join("\n"); assert!(has(&t, r"\n Change {7}PR#60 merged · 19 files, \+340 −27\n"), "{t}");
    // What needs you is a part of its own right under Status, and the reviews read as a history.
    let t = side("failed").join("\n");
    assert!(has(&t, &format!(r"\n\n Went wrong ─+\n Conflict with main\n The agent can fix this: {} sends it\.\n Next step {{4}}Resolve the conflict\.\n Details {{6}}Conflicting files:\n[ ▌] {{13}}src/board/view/row\.js\n\n Description ─", regex::escape(&key_name("sendBack")))), "{t}");
    let t = side("done").join("\n");
    assert!(has(&t, r"\n Reviews · 3 ─+\n History {6}fail → no verdict → pass\n Review 3 {5}✓ Pass · 1m ago\n[ ▌] {13}The change meets the task\. A title at the limit is kept\n[ ▌] {13}whole, and a longer one is cut between words with the\n[ ▌] {13}rest moved into the description\.\n Evidence {5}↗ Open review\n"), "{t}");
    // Full screen every review is there, the newest first, with what it found by how bad it is.
    let whole = lines(&mut s, &st("done"), 100, true).join("\n");
    assert!(has(&whole, r"\n Review 1 {5}✗ Fail · 1 critical, 1 minor, 1 pre-existing · 2h ago · at 1111111\n[ ▌] {13}A title is cut in the middle of a word\.\n Findings {5}critical {2}A title is cut in the middle of a word \(src/store/titles\.js:14\)\n[ ▌] {13}minor {5}The limit is written in two places \(src/store/titles\.js:14\) · not required\n[ ▌] {13}major {5}The old menu flickers \(src/store/titles\.js:14\) · pre-existing, task proj/7\n Checked {6}• Ran the check\.\n Unverified {3}• Not tried on another machine\.\n"), "{whole}");
    let at = |x: &str| whole.find(x).unwrap();
    assert!(at("\n Review 3 ") < at("\n Review 2 ") && at("\n Review 2 ") < at("\n Review 1 "));
    // Nothing of the evidence is cut on the full screen: a long name, what it is and its address are wrapped under
    // themselves. Beside the list its name is wrapped too, and the last line says where the rest is.
    let piece = json!({ "caption": "A recording of every title length from seventy to ninety characters being filed", "detail": "Play it from the start: the first minute is the board opening, then one task is filed for each length.", "link": "https://example.test/proj/1/recordings/titles-from-seventy-to-ninety-characters.mp4" });
    let mut review = reviews[2].clone(); review["artifacts"] = json!([piece]);
    let long = with(&Value::Object(st("done").into_iter().collect()), json!({ "review": review }));
    for width in [40, 60, 100] {
        let page = lines(&mut s, &long, width, true);
        let from = page.iter().position(|l| l.starts_with(" Evidence ─")).unwrap();
        let end = page.iter().skip(from).position(|l| l.is_empty()).map(|n| n + from).unwrap_or(page.len());
        let text = page[from + 1..end].iter().map(|l| regex::Regex::new(r"^ (↗ | {2})").unwrap().replace(l, "").to_string()).collect::<Vec<_>>().join(" ");
        assert!(page.iter().all(|l| wd(l) <= width && !l.ends_with('…')), "evidence at {width} columns: nothing runs over, nothing is cut");
        assert_eq!(text.replace(' ', ""), format!("{} {} {}", piece.s("caption"), piece.s("detail"), piece.s("link")).replace(' ', ""), "evidence at {width} columns: all of it");
    }
    let beside = lines(&mut s, &long, 46, false);
    assert!(has(&beside.join("\n"), r"\n Evidence {5}↗ A recording of every title\n[ ▌] {15}length from seventy to ninety\n[ ▌] {15}characters being filed\n"), "{}", beside.join("\n"));
    assert_eq!(beside.last().unwrap(), &format!(" {} shows the whole task", key_name("panel")));
    // A review whose summary is shortened beside the list says where the rest is, also when nothing else is short.
    let wordy = with(&me, json!({ "description": "", "agent": "claude", "app": "claude", "state": "done", "stage": "done", "at": now, "reviewRound": 1, "reviews": null, "review": { "score": 4, "recommend": true, "text": vec!["word"; 120].join(" ") } }));
    let ls = lines(&mut s, &wordy, 60, false);
    assert!(ls.iter().any(|l| l.ends_with(" …")), "the summary is shortened"); assert_eq!(ls.last().unwrap(), &format!(" {} shows the whole task", key_name("panel")));
    let fine = with(&Value::Object(wordy.into_iter().collect()), json!({ "review": { "score": 4, "recommend": true, "text": "Fine." } }));
    assert!(!lines(&mut s, &fine, 60, false).join("\n").contains("shows the whole task"));
    // The `*` of a task's own setting is wrapped with its value: it never runs over the edge.
    set_value("reviewer.agent", &json!("claude"), &Scope::task(&task(&id))).unwrap(); set_value("reviewer.fast", &json!("on"), &Scope::task(&task(&id))).unwrap();
    for width in [40, 56, 70, 71, 72, 100] {
        let mut mine = Value::Object(task(&id).into_iter().collect()); mine["owner"] = json!("me");
        let page = lines(&mut s, &with(&mine, json!({ "state": "idle", "at": now })), width, true);
        let from = page.iter().position(|l| l.starts_with(" Reviewer ")).unwrap();
        let end = page.iter().enumerate().position(|(n, l)| n > from && has(l, r"^ \S")).unwrap();
        let row = &page[from..end];
        assert!(page.iter().all(|l| wd(l) <= width), "{width} columns: no setting runs over");
        assert!(row.last().unwrap().ends_with(" *"), "{width} columns: the mark is there: {row:?}");
        // (A dot between two facts is left out where the second starts a line: read without the dots.)
        let said = row.iter().map(|l| l.chars().skip(14).collect::<String>().trim().to_string()).collect::<Vec<_>>().join(" ").replace(" ·", "");
        assert_eq!(said, "claude default model * default effort no fast mode in claude *", "{width} columns: the whole value");
        assert!(page[end..].iter().any(|l| l == " * this task only"), "{width} columns: the line that says what * means: {page:?}");
    }
    // One of a pair set and the other not: both are said, each with its value, a `*` on this task's own. The same
    // holds for what is moot: a reviewer's own choices where nothing is reviewed, a planner where no plan is asked for.
    let pair = w.add("A pair");
    let set = |k: &str, v: &str| { set_value(k, &json!(v), &Scope::task(&task(&pair))).unwrap(); };
    let unset = |k: &str| { unset_value(k, &Scope::task(&task(&pair))).unwrap(); };
    let mut whole = || { let mut p = Value::Object(task(&pair).into_iter().collect()); p["owner"] = json!("me"); lines(&mut s, &with(&p, json!({ "state": "idle", "at": now })), 100, true).join("\n") };
    set("worker.fallback", "codex"); set("reviewer.fallback", "none"); set("limit.time", "2h"); set("limit.tokens", "none");
    let t = whole();
    assert!(has(&t, r"\n Takes over {3}codex · when the agent cannot go on \*\n") && has(&t, r"\n Takes over {3}nobody · when the reviewer cannot go on \*\n"), "{t}");
    assert!(has(&t, r"\n Limits {7}2h a run \* · no token limit \*\n") && !has(&t, "no limits|no fallback"), "{t}");
    set("limit.time", "none"); unset("limit.tokens"); unset("worker.fallback");
    let t = whole();
    assert!(has(&t, r"\n Limits {7}no time limit \* · no token limit\n") && has(&t, r"\n Also {9}one attempt · not research only · no fallback \*\n"), "{t}");
    set("review.when", "never"); set("reviewer.effort", "high"); set("plan.agent", "codex");
    let t = whole();
    assert!(has(&t, r"\n Plan first {3}off · would be written by codex \*\n") && has(&t, r"\n Reviewer {5}off \* · high effort \*\n"), "{t}");
    // Every work setting this task has an answer of its own for is marked somewhere in the part, and the line under
    // the rows says what the mark means.
    assert_eq!(regex::Regex::new(r"\S \*").unwrap().find_iter(&t).count(), 6, "{t}"); assert!(t.contains("\n * this task only\n"), "{t}");
    // Another machine's task: the same rows, from the settings that machine sent; only that task's own are marked.
    let sent: Vec<Value> = sw::store::settings::show::rows(&Scope::task(&task(&id))).iter().map(|x| { let mut j = x.json(); if x.name == "review.rounds" { j["value"] = json!(1); j["place"] = json!("you"); j["from"] = json!("you"); } else if x.name == "check.command" { j["value"] = json!("npm test"); j["place"] = json!("file"); j["from"] = json!("the repo's file"); } j }).collect();
    let far = with(&me, json!({ "id": format!("vm-a:{id}"), "rid": id, "host": "vm-a", "state": "idle", "at": now, "snap": { "folder": "~/far", "settings": sent, "canWorktree": true } }));
    let t = lines(&mut s, &far, 70, false).join("\n");
    assert!(has(&t, r"\n Settings ─+\n Machine {6}vm-a\n Agent {8}claude · default model · default effort\n"), "{t}");
    assert!(has(&t, r"\n Send back {4}major or worse · 1 round max\n Check {8}npm test · 15m at most\n"), "{t}");
    assert!(has(&t, r"\n Reviewer {5}claude · default model \* · default effort\n {14}no fast mode in claude \*\n"), "{t}");
    // A result and a severity are said in words and marks; the colour only repeats them. A headline that needs you is
    // in its state's colour. Merged, how its reviews went is quiet like its pull request; not merged, they keep their colours.
    let pieces = |ls: Vec<Inked>| ls.into_iter().flat_map(|l| l.0.into_iter()).collect::<Vec<_>>();
    let merged = pieces(inked(&mut s, &st("done"), 100, true));
    let open_one = pieces(inked(&mut s, &with(&Value::Object(st("done").into_iter().collect()), json!({ "state": "blocked", "kind": "permission", "stage": "approval", "prState": "passing", "prMerged": false })), 100, true));
    let is = |ps: &[sw::shown::ink::Piece], start: &str, c: Ink, quiet: bool| ps.iter().any(|p| p.t.starts_with(start) && p.c == c && p.quiet == quiet);
    assert!(is(&open_one, "✓", Ink::Green, false) && is(&open_one, "✗", Ink::Red, false) && is(&merged, "critical  ", Ink::Red, false) && is(&merged, "minor     ", Ink::Yellow, false), "marks and severities in their colours");
    let quiet = |p: &&sw::shown::ink::Piece| p.quiet || p.c == Ink::Muted;
    assert!(merged.iter().filter(|p| p.t.starts_with('✓') || p.t.starts_with('✗')).all(|p| quiet(&p)) && merged.iter().any(|p| p.t.starts_with('✓')), "merged: the marks of its reviews are quiet: {:?}", merged.iter().filter(|p| p.t.starts_with('✓') || p.t.starts_with('✗')).collect::<Vec<_>>());
    let failed = pieces(inked(&mut s, &st("failed"), 70, false));
    assert!(failed.iter().any(|p| p.t == "Went wrong" && p.c == Ink::Red && p.bold), "what went wrong has a red headline");
}
