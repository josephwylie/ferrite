//! Research only (research.only, --research): the agent changes nothing in the repo, writes a report in the task's
//! folder among the board's reports, is reviewed on that report, and the task ends as done with nothing to merge;
//! one that changed the repo is sent back and, after its rounds, waits for you saying why.
use crate::common;
use common::*;
use serde_json::json;
use sw::actions::filing;
use sw::json::*;

fn add_research(w: &World, title: &str) -> String { filing::add(&w.root, title, filing::AddOpts { research: Some(true), ..Default::default() }).unwrap().t("id") }
fn all_prompts(w: &World, t: &Obj) -> String { prompts(w, t.at("link").s("pane")).join("\n---\n") }

#[test]
fn a_research_task_that_changed_the_repo_is_sent_back_and_after_its_rounds_waits_for_you() {
    let w = world_with(Opts { repo: true, origin: true, settings: json!({ "review.rounds": 1, "review.when": "never" }), repo_settings: Some(json!({ "check.command": "exit 1" })) });
    w.script(json!({ "worker": [{ "files": { "docs/report.md": "found\n" }, "commit": "the report", "note": "Wrote the report." }, { "files": { "notes.txt": "more\n" }, "note": "Looked again." }] }));
    let id = add_research(&w, "Research the options");
    delegate(&id);
    let mut t = settle(&id);
    for _ in 0..2 { if t.s("stage") != "failed" { t = settle(&id); } }
    assert_eq!(t.s("stage"), "failed", "{}", t.s("msg"));
    assert_eq!(t.s("msg"), "Research only, but its agent changed the repo again after 1 round (3 changes in the repo): nothing was pushed, opened or merged");
    assert_eq!((t.at("failure").s("stage"), t.at("failure").at("agentCanFix")), ("research", &json!(true)));
    let d = t.at("failure").s("detail");
    assert!(d.contains("- docs/report.md") && d.contains("- notes.txt") && d.contains("- (1 commit added)"), "{d}");
    assert!(t.at("failure").s("next").contains("herdr-projects set research.only false --task proj/1) and delegates it again"));
    let said = all_prompts(&w, &t);
    assert_eq!(said.matches("This task is research only, and you changed the repo").count(), 1);
    assert!(said.contains(&format!("save the report in {}/ (outside the repo)", sw::store::files::reports_dir().join("proj").join("1").display())), "{said}");
    assert!(!t.b("pr"));
    assert_eq!(git(&w.root, &["ls-remote", "origin"]).lines().filter(|l| l.contains("work/")).count(), 0, "nothing pushed");
    let tree = std::path::Path::new(t.s("worktree"));
    assert!(tree.join("notes.txt").exists(), "left as the agent left it");
    // Every change undone, the step is tried again: done, with nothing to merge, and the line says no report was found.
    git(tree, &["reset", "-q", "--hard", t.s("baseSha")]);
    std::fs::remove_file(tree.join("notes.txt")).ok();
    assert_eq!(sw::research::changes_of(&t, tree), Some(vec![]));
    sw::actions::finish_line::retry(&id).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "done", "{}", t.at("failure"));
    assert!(t.s("msg").contains("Looked again. No report was found."), "{}", t.s("msg"));
}

#[test]
fn a_research_task_that_changed_nothing_is_done_with_its_report_kept_and_reviewed() {
    let w = world_with(Opts { repo: true, repo_settings: Some(json!({ "check.command": "exit 1" })), ..Default::default() });
    let page = "<!doctype html><title>Report</title><h1>What I found</h1>";
    let dir = sw::store::files::reports_dir().join("proj").join("1");
    std::fs::create_dir_all(&dir).unwrap(); std::fs::write(dir.join("index.html"), page).unwrap();
    w.script(json!({ "worker": [{ "note": "The build is slow because of X." }], "reviewer": [{ "verdict": { "meets": true, "text": "The report answers it.", "findings": [] }, "evidence": { "out.txt": "ran it\n" } }] }));
    let id = add_research(&w, "Why is the build slow");
    delegate(&id);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "done", "{}", t.at("failure"));
    assert!(t.s("msg").contains("The build is slow because of X.") && !t.s("msg").contains("No report"), "{}", t.s("msg"));
    assert_eq!(t.at("report").s("file"), "index.html");
    assert_eq!(t.at("report").s("link"), dir.join("index.html").display().to_string());
    assert_eq!(read(&sw::store::files::artifacts_dir(&id).join("report").join("index.html")), page);
    assert!(sw::store::files::artifacts_dir(&id).join("out.txt").exists(), "the evidence, beside it");
    let d = sw::actions::finish_line::details(&id).unwrap();
    assert_eq!((d.at("report"), d.at("research")), (t.at("report"), &json!(true)));
    assert_eq!(sw::store::reviews::review_history(&t).len(), 1);
    assert!(!t.b("pr"));
}

#[test]
fn every_agent_is_told_a_report_never_goes_into_the_repo_and_a_research_task_changes_nothing() {
    let w = world();
    w.script(json!({ "worker": [{ "status": "working" }] }));
    let id = add_research(&w, "Look into it");
    delegate(&id);
    let t = settle(&id);
    let a = agent_of(&w, t.at("link").s("pane"));
    let args: Vec<String> = a.arr("args").iter().map(text).collect();
    let rules = read(std::path::Path::new(&args[args.iter().position(|x| x == "--append-system-prompt-file").unwrap() + 1]));
    assert!(rules.contains("research only"), "{rules}");
    assert!(rules.contains("never") && rules.contains("report"), "{rules}");
    assert!(sw::research::is_research(&task(&id)));
}
