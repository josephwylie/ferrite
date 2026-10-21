//! Suggested tasks are gone: what old tasks still carry of them is saved once and taken off, the old commands say so
//! in a plain sentence, and a task that still carries some reads as any other.
use crate::common;
use common::board::open;
use common::*;
use serde_json::json;
use sw::json::*;
use sw::store::tasks::{update, Change};

fn has(text: &str, re: &str) -> bool { regex::Regex::new(re).unwrap().is_match(text) }
fn put(id: &str, f: impl FnOnce(&mut Obj)) { update(id, |t| { f(t); Ok(Change::Write) }).unwrap(); }

#[test]
fn a_task_saved_with_suggestions_loads_and_shows_as_any_other_and_the_open_ones_are_written_down_once_and_taken_off() {
    let w = world();
    let (one, two, three) = (w.add("Add a column"), w.add("Old work"), w.add("Nothing open"));
    put(&one, |t| t.put("suggestions", json!([
        { "id": "S1", "title": "The cleanup retry test fails under load", "description": "Where: test/cleanup-retry.test.js:52\nWhat the reviewer saw: 2 of 5 runs failed.", "by": "reviewer", "state": "open", "at": 1790812800000i64 },
        { "id": "S2", "title": "Dismissed long ago", "by": "agent", "state": "dismissed" },
        { "id": "S3", "title": "Dates are parsed as text", "where": "src/dates.js", "by": "agent", "state": "open" }])));
    put(&two, |t| { t.put("suggestions", json!([{ "id": "S1", "title": "Speed up the export", "description": "It takes a minute.", "by": "agent", "state": "open" }])); t.put("archivedAt", sw::clock::now()); });
    put(&three, |t| t.put("suggestions", json!([{ "id": "S1", "title": "Made a task", "by": "agent", "state": "accepted", "task": "proj/9" }])));
    // Before anything is taken off: the board, the panel and the commands read such a task, and say nothing of them.
    let mut s = open(&w.root, 40); s.s.sel = one.clone(); s.load();
    let txt = s.screen(120).join("\n");
    assert!(txt.contains("Add a column") && !has(&txt, "[Ss]uggest"), "{txt}");
    // ctrl+n was no key of the board then; now it moves to the next attempt, and on a task done once it does nothing.
    s.press(&["ctrl+n"]); assert!(s.s.ask.is_none()); assert!(!has(&s.screen(120).join("\n"), "[Ss]uggest"));
    assert!(sw::actions::finish_line::details(&one).unwrap().at("suggestions").is_null());
    // The pass writes the open ones down, each with its task, its title, what it said and who made it.
    let file = sw::store::old_suggestions::old_suggestions_file();
    assert_eq!(file, sw::store::files::reports_dir().join("old-suggestions.md")); assert!(!file.exists());
    sw::housekeeping::housekeeping(&w.socket, sw::housekeeping::Force::Yes);
    let page = read(&file);
    assert!(has(&page, r"^# Suggested tasks that were still open\n"), "{page}");
    assert!(has(&page, r"\n## The cleanup retry test fails under load\n\nOn proj/1 \(Add a column\) · from the reviewer · 2026-10-01\n\nWhere: test/cleanup-retry\.test\.js:52\nWhat the reviewer saw: 2 of 5 runs failed\.\n"), "{page}");
    assert!(has(&page, r"\n## Dates are parsed as text\n\nOn proj/1 \(Add a column\) · from the agent\n\nWhere: src/dates\.js\n"), "{page}");
    assert!(has(&page, r"\n## Speed up the export\n\nOn proj/2 \(Old work\) · from the agent\n\nIt takes a minute\.\n"), "an archived task's too: {page}");
    assert!(!has(&page, "Dismissed long ago|Made a task")); assert_eq!(page.lines().filter(|l| l.starts_with("## ")).count(), 3);
    for t in sw::store::tasks::list(sw::store::tasks::ListOpts { include_archived: true, ..Default::default() }) { assert!(!t.contains_key("suggestions"), "{}", t.s("id")); }
    // Once: a later pass writes nothing again, and reads no task for it.
    put(&one, |t| t.put("suggestions", json!([{ "id": "S9", "title": "Sent by an older version", "by": "agent", "state": "open" }])));
    sw::housekeeping::housekeeping(&w.socket, sw::housekeeping::Force::Yes);
    assert_eq!(read(&file), page); assert_eq!(sw::store::old_suggestions::save_old_suggestions(), 0);
}

#[test]
fn the_commands_that_are_gone_fail_with_a_plain_sentence_that_says_so_and_the_help_names_none_of_them() {
    let w = world();
    let t = w.add("Add a column");
    let args: [&[&str]; 4] = [&["suggest", "Fix the flaky test", "--from", &t], &["suggestions"], &["accept", &t, "S1"], &["dismiss", &t]];
    for a in args {
        let argv: Vec<String> = a.iter().map(|x| x.to_string()).collect();
        let e = sw::cli::tasks::command(a[0], &sw::cli::flags::parse(&argv)).err().unwrap_or_else(|| panic!("{a:?} was not refused"));
        assert_eq!(e.msg, format!("There is no \"{}\" command any more: suggested tasks were removed from the board. Mention work you came across outside your task in a short \"Also noticed\" list in your closing note or your report, and carry on with your own task.", a[0]));
    }
    assert_eq!(sw::store::tasks::list(Default::default()).iter().map(|x| x.t("id")).collect::<Vec<_>>(), vec![t]);
    assert!(!has(sw::cli::help::HELP, "suggest|accept <id>|dismiss"));
}

#[test]
fn agents_reviewers_and_the_coordinator_are_told_the_same_plain_thing_work_found_outside_the_task_is_mentioned_never_a_task() {
    let w = world();
    let t = task(&w.add("Add a column"));
    let told = sw::delegate::rules::rules(&t);
    assert!(told.contains("Work you merely come across is a line in your closing note or report, never a task."), "{told}");
    assert!(!has(&told, "herdr-projects suggest|accept|dismiss"));
    let change = json!({ "head": "abc" });
    let brief = sw::finish::review_brief::review_brief(&t, &sw::finish::review_brief::Brief { text: "done", change: Some(&change), to_land: true, ch: &Obj::new(), earlier: None, check: "" });
    assert!(brief.contains("such a finding is a note for the human, it never sends the work back and nothing else is done with it"), "{brief}");
    assert!(brief.contains("one that was there before this change is marked `preexisting`: it is a note for the human in your review"), "{brief}");
    assert!(!has(&brief, "suggest|becomes a task"));
    let coordinator = sw::coordinator::rules::rules("board");
    assert!(coordinator.contains("they mention it in a short \"Also noticed\" list in their closing note or report (a reviewer: as a note in the review)"), "{coordinator}");
    let skill = sw::extras::skill_text::skill_text("herdr-projects");
    assert!(skill.contains("mention it in a short \"Also noticed\" list"), "{skill}");
    for words in [&coordinator, &skill] { assert!(!has(words, "suggest|accept <id>|dismiss")); }
    // A reviewer on an older version's brief may still write a `title`: it is read past.
    let f = sw::gate::read_findings(&json!([{ "severity": "minor", "subject": "x", "title": "y" }]));
    let mut keys: Vec<&String> = f[0].keys().collect(); keys.sort();
    assert_eq!(keys, vec!["id", "preexisting", "proof", "severity", "subject", "where"]);
}
