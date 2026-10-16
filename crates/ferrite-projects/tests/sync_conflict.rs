//! A conflict with main caught before review, not after approval: sent to the agent by itself, its repairs capped,
//! caught again by the last sync after an approval, and kept in the record of conflicts.
use crate::common;
use common::*;
use serde_json::json;
use std::path::Path;
use sw::actions::{finish_line, talk};
use sw::json::*;
use sw::sync::conflict_log::{conflict_found, conflict_log, conflict_logged, conflict_repaired, conflicts};

fn holds(dir: &Path, sha: &str) -> bool { common::worktrees::git(dir, &["merge-base", "--is-ancestor", sha, "HEAD"]).is_ok() }
fn main_moves(w: &World, file: &str, body: &str) -> String { std::fs::write(w.root.join(file), body).unwrap(); git(&w.root, &["add", "-A"]); git(&w.root, &["commit", "-qm", &format!("main: {file}")]); git(&w.root, &["rev-parse", "HEAD"]) }
fn asking(w: &World, title: &str) -> String { let id = w.add(title); delegate(&id); assert_eq!(settle(&id).s("stage"), "asks"); id }
fn tree(t: &Obj) -> &Path { Path::new(t.s("worktree")) }
fn pane_prompts(w: &World, id: &str) -> Vec<String> { prompts(w, task(id).at("link").s("pane")) }
/// The agent's turn ends by hand: a closing note, and herdr says done.
fn end_turn(w: &World, t: &Obj, text: &str) {
    let note = sw::flow::task::note_file(t);
    std::fs::create_dir_all(note.parent().unwrap()).unwrap();
    std::fs::write(&note, text).unwrap();
    let later = std::time::SystemTime::now() + std::time::Duration::from_millis(2000);
    std::fs::File::options().write(true).open(&note).unwrap().set_modified(later).unwrap();
    let pane = t.at("link").t("pane");
    w.patch(|s| { let seq = s.i("seq") + 1; s["seq"] = json!(seq); for p in s["panes"].as_array_mut().unwrap() { if p.s("pane_id") == pane { p["agent"]["status"] = json!("done"); p["agent"]["seq"] = json!(seq); p["agent"]["completion"] = json!(seq); } } });
}

#[test]
fn a_conflict_with_main_goes_to_the_agent_by_itself_before_any_approval_and_the_repair_is_checked_and_reviewed() {
    let w = world();
    w.script(json!({ "worker": [{ "note": "Which one?" }, { "files": { "a.txt": "from the agent\n" }, "commit": "agent", "note": "Changed a.txt." }, { "note": null, "status": "working" }] }));
    let id = asking(&w, "Conflict");
    let target = main_moves(&w, "a.txt", "from main\n");
    talk::tell(&id, "The first one.", false).unwrap();
    let t = settle(&id);
    // rolled back, sent, and nothing to approve
    assert_eq!(t.s("stage"), "working"); assert_eq!(t.at("expectedWake").s("reason"), "conflict"); assert_eq!(t.i("syncRepairs"), 1);
    assert_eq!(t.at("failure").s("title"), "Merge conflict with main"); assert_eq!(t.at("failure").s("phase"), "sync"); assert!(t.at("failure").b("sentAt"));
    assert!(t.at("failure").s("detail").contains("Conflicting files:\na.txt"));
    assert_eq!(git(tree(&t), &["status", "--porcelain"]), ""); assert_eq!(read(&tree(&t).join("a.txt")), "from the agent\n");
    assert!(!holds(tree(&t), &target)); assert_eq!(git(&w.root, &["status", "--porcelain"]), "");
    let said = pane_prompts(&w, &id).last().cloned().unwrap();
    assert!(said.contains("conflicts with main") && said.contains("Conflicting files:\n- a.txt") && said.contains(&format!("Target commit: {target}")), "{said}");
    assert!(said.contains("resolve the conflicts, commit, and finish again") && !said.contains("already approved"), "{said}");
    assert!(!t.b("review") && !t.b("mergeIntent"));
    let e = sw::flow::sidebar::entry_now(&t, sw::clock::now());
    assert_eq!((row(&id).t("state"), row(&id).t("note"), e.place, e.line), ("working".into(), "Resolving a conflict with main".into(), "agent · working".into(), String::new()), "what the board shows, and where herdr's sidebar says it stands");
    assert!(finish_line::approve(&id, None, false).unwrap_err().msg.contains("not waiting for an approval"));
    // the agent resolves it in its own worktree and finishes again
    let _ = common::worktrees::git(tree(&t), &["merge", "-q", &target]);
    std::fs::write(tree(&t).join("a.txt"), "merged\n").unwrap(); git(tree(&t), &["add", "-A"]); git(tree(&t), &["commit", "-qm", "resolve"]);
    end_turn(&w, &t, "Resolved the conflict.");
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure"));
    assert!(t.at("failure").is_null()); assert_eq!(t.i("syncRepairs"), 0); assert_eq!(t.at("review").at("pass"), &json!(true)); assert!(holds(tree(&t), &target));
    // the conflict left one line in the record: the task, the files, the minutes, and that a review followed
    let kept = conflicts();
    assert_eq!(kept.len(), 1);
    assert_eq!((kept[0].t("task"), kept[0].at("files").clone(), kept[0].i("repairs"), kept[0].at("review").clone()), (id.clone(), json!(["a.txt"]), 1, json!(true)));
    assert!(kept[0].f("minutes") >= 0.0 && chrono::DateTime::parse_from_rfc3339(kept[0].s("at")).is_ok()); assert!(t.at("conflict").is_null());
    assert_eq!(conflict_log(), sw::store::files::data_dir().join("conflicts.jsonl"));
    finish_line::approve(&id, None, false).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "done", "{}", t.at("failure"));
    assert_eq!(read(&w.root.join("a.txt")), "merged\n");
}

#[test]
fn conflict_repairs_are_capped_after_rounds_tries_in_a_row_the_human_is_asked_with_the_details_kept() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.rounds": 1 }), ..Default::default() });
    // the "repair" changes the file again without bringing main in
    w.script(json!({ "worker": [{ "note": "Which one?" }, { "files": { "a.txt": "from the agent\n" }, "commit": "agent", "note": "Changed a.txt." }, { "files": { "a.txt": "still mine\n" }, "commit": "again", "note": "Resolved." }] }));
    let id = asking(&w, "Capped");
    main_moves(&w, "a.txt", "from main\n");
    talk::tell(&id, "The first one.", false).unwrap();
    let mut t = settle(&id);
    for _ in 0..2 { if t.s("stage") != "asks" { t = settle(&id); } }
    assert_eq!((t.s("stage"), t.s("kind")), ("asks", "question"));
    assert!(t.s("msg").contains("could not resolve the conflict with main after 1 repairs"), "{}", t.s("msg"));
    assert_eq!(t.at("failure").s("title"), "Merge conflict with main"); assert!(t.at("failure").at("sentAt").is_null());
    assert_eq!(pane_prompts(&w, &id).len(), 3, "its task, your answer, one repair: not a second one");
    assert_eq!(git(tree(&t), &["status", "--porcelain"]), ""); assert_eq!(read(&tree(&t).join("a.txt")), "still mine\n");
    assert_eq!((row(&id).t("state"), row(&id).t("kind")), ("blocked".into(), "question".into()));
    assert!(finish_line::approve(&id, None, false).unwrap_err().msg.contains("not waiting for an approval"));
}

#[test]
fn after_an_approval_the_sync_is_the_last_safety_net_a_conflict_there_goes_to_the_agent_and_the_approved_merge_is_retried() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.when": "never", "workspace.cleanup": false }), ..Default::default() });
    w.script(json!({ "worker": [{ "files": { "a.txt": "from the agent\n" }, "commit": "agent", "note": "Changed a.txt." }] }));
    let id = w.add("Late");
    delegate(&id);
    assert_eq!(settle(&id).s("stage"), "approval");
    let target = main_moves(&w, "a.txt", "from main\n"); // main moves and you approve before the board has looked
    w.script(json!({ "worker": [{ "merge": target, "files": { "a.txt": "merged\n" }, "commit": "resolve", "note": "Resolved." }] }));
    finish_line::approve(&id, None, false).unwrap();
    let mut t = settle(&id);
    for _ in 0..2 { if t.s("stage") != "done" { t = settle(&id); } }
    assert_eq!(t.s("stage"), "done", "{} {} {}", t.s("stage"), t.s("msg"), t.at("failure"));
    let p = pane_prompts(&w, &id);
    assert!(regex::Regex::new(r"Conflicting files:\n- a\.txt[\s\S]*already approved this merge").unwrap().is_match(p.last().unwrap()), "{}", p.last().unwrap());
    assert_eq!(p.len(), 2);
    assert_eq!(read(&w.root.join("a.txt")), "merged\n");
}

#[test]
fn the_record_of_conflicts_a_repair_that_conflicts_again_is_one_conflict_the_minutes_run_to_the_repair_and_nothing_is_written_before_it() {
    let _w = world();
    let mut t = obj(json!({ "id": "proj/7" }));
    let t0 = chrono::DateTime::parse_from_rfc3339("2026-10-13T10:00:00Z").unwrap().timestamp_millis();
    let at = |ms: i64| sw::clock::set(Some(ms));
    assert_eq!(conflict_logged(&mut t, true), None, "no conflict: no line");
    at(t0); conflict_found(&mut t, "a.js\nb.js");
    assert_eq!(conflict_logged(&mut t, true), None, "found, not repaired yet: no line");
    at(t0 + 5 * 60000); conflict_found(&mut t, "b.js\nc.js"); // sent back a second time
    at(t0 + 12 * 60000); conflict_repaired(&mut t); at(t0 + 50 * 60000); conflict_repaired(&mut t); // the first time it is up to date is the repair
    assert_eq!(conflict_logged(&mut t, false), Some(json!({ "at": "2026-10-13T10:00:00.000Z", "task": "proj/7", "files": ["a.js", "b.js", "c.js"], "minutes": 12.0, "repairs": 2, "review": false })));
    assert!(t.at("conflict").is_null()); assert_eq!(conflict_logged(&mut t, false), None);
    at(t0 + 60 * 60000); conflict_found(&mut t, "d.js"); at(t0 + 60 * 60000 + 90000); conflict_repaired(&mut t); conflict_logged(&mut t, true);
    let all: Vec<_> = conflicts().iter().map(|c| (c.t("task"), strings(c.at("files")).join(","), c.f("minutes"), c.i("repairs"), c.b("review"))).collect();
    assert_eq!(all, vec![("proj/7".into(), "a.js,b.js,c.js".into(), 12.0, 2, false), ("proj/7".into(), "d.js".into(), 1.5, 1, true)]);
}
