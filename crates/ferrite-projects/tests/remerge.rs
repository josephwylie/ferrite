//! Work that passed its review and then only took main in is not reviewed again: the tests run on the merged result and
//! the review stands. Anything beyond the merge is reviewed as ever.
use crate::common;
use common::*;
use serde_json::json;
use std::path::Path;
use sw::json::*;
use sw::store::reviews::review_history;

fn holds(dir: &str, sha: &str) -> bool { common::worktrees::git(Path::new(dir), &["merge-base", "--is-ancestor", sha, "HEAD"]).is_ok() }
fn big(first: &str, last: &str) -> String { let mut v = vec![first.to_string()]; v.extend((2..20).map(|i| format!("line {i}"))); v.push(last.into()); v.join("\n") + "\n" }
fn main_moves(w: &World, files: &[(&str, &str)]) -> String { for (f, t) in files { std::fs::write(w.root.join(f), t).unwrap(); } git(&w.root, &["add", "-A"]); git(&w.root, &["commit", "-qm", "main moves"]); git(&w.root, &["rev-parse", "HEAD"]) }
fn resync(w: &World, id: &str) -> Obj { sw::housekeeping::housekeeping(&w.socket, sw::housekeeping::Force::Yes); let mut t = settle(id); for _ in 0..2 { t = settle(id); } t }
fn head(t: &Obj) -> String { git(Path::new(t.s("worktree")), &["rev-parse", "HEAD"]) }
/// A world whose main has big.txt and a check, and one task that edited big.txt's first line and passed review 1.
fn passed() -> (World, String, String) {
    let w = world_with(Opts { repo: true, repo_settings: Some(json!({ "check.command": "! test -f broken.txt" })), ..Default::default() });
    std::fs::write(w.root.join("big.txt"), big("line 1", "line 20")).unwrap(); git(&w.root, &["add", "-A"]); git(&w.root, &["commit", "-qm", "big"]);
    w.script(json!({ "worker": [{ "files": { "big.txt": big("TASK", "line 20") }, "commit": "task", "note": "Changed the first line." }] }));
    let id = w.add("Edit big");
    delegate(&id);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure")); assert_eq!(t.i("reviewRound"), 1); assert_eq!(t.at("review").at("pass"), &json!(true));
    assert_eq!(review_history(&t)[0].at("head"), t.at("reviewedHead"), "the commit the passing review looked at is on record");
    let r = t.t("reviewedHead");
    (w, id, r)
}

#[test]
fn main_merged_cleanly_into_the_tasks_own_file_after_a_passing_review_the_tests_run_no_new_review() {
    let (w, id, reviewed) = passed();
    let main = main_moves(&w, &[("big.txt", &big("line 1", "MAIN"))]);
    let t = resync(&w, &id);
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure")); assert!(holds(t.s("worktree"), &main));
    assert_eq!(t.i("reviewRound"), 1); assert_eq!(review_history(&t).len(), 1, "the tag keeps showing the review that passed");
    assert!(t.at("review").b("keptAt")); assert_eq!(t.at("review").at("pass"), &json!(true)); assert_eq!(t.t("reviewedHead"), head(&t));
    let k = t.at("review").at("kept");
    assert_eq!((k.t("from"), k.t("head"), k.i("merges"), k.at("resolved").clone(), k.t("check")), (reviewed.clone(), t.t("reviewedHead"), 1, json!([]), "! test -f broken.txt".to_string()));
    assert!(t.s("msg").starts_with("Approve to merge? Review: Pass. Re-merged with main, tests pass, review 1 still stands. "), "{}", t.s("msg"));
    let e = review_history(&t)[0].clone();
    assert_eq!(e.t("head"), reviewed); assert_eq!(e.arr("kept").len(), 1); assert_eq!(e.at("kept")[0].t("head"), t.t("reviewedHead")); assert!(e.at("kept")[0].i("at") > 0);
    assert_eq!(sw::actions::finish_line::details(&id).unwrap().at("reviews")[0].arr("kept").len(), 1);
    assert_eq!(prompts(&w, t.at("link").s("pane")).len(), 1, "the agent heard nothing");
    // main moves again: still measured from the commit the reviewer really saw, and written down again
    main_moves(&w, &[("big.txt", &big("line 1", "MAIN AGAIN"))]);
    let u = resync(&w, &id);
    assert_eq!(u.s("stage"), "approval"); assert_eq!(u.i("reviewRound"), 1); assert_eq!(u.at("review").at("kept").i("merges"), 2); assert_eq!(u.at("review").at("kept").t("from"), reviewed);
    assert_eq!(review_history(&u)[0].arr("kept").len(), 2);
    sw::actions::finish_line::approve(&id, None, false).unwrap();
    assert_eq!(settle(&id).s("stage"), "done");
    assert_eq!(read(&w.root.join("big.txt")), big("TASK", "MAIN AGAIN"));
}

#[test]
fn a_conflict_the_agent_resolved_in_the_merge_and_nothing_more_the_tests_run_the_review_stands() {
    let (w, id, reviewed) = passed();
    let main = main_moves(&w, &[("big.txt", &big("MAIN", "line 20"))]);
    w.script(json!({ "worker": [{ "merge": main, "files": { "big.txt": big("TASK and MAIN", "line 20") }, "commit": "resolve", "note": "Resolved." }] }));
    let t = resync(&w, &id);
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure")); assert!(holds(t.s("worktree"), &main));
    let last = prompts(&w, t.at("link").s("pane")).last().cloned().unwrap_or_default();
    assert!(last.contains("already passed its review: change only the conflicting files"), "{last}");
    assert_eq!(t.i("reviewRound"), 1); assert_eq!(review_history(&t).len(), 1); assert!(t.at("review").b("keptAt")); assert!(!t.at("review").b("stale"));
    assert_eq!((t.at("review").at("kept").t("from"), t.at("review").at("kept").at("resolved").clone()), (reviewed, json!(["big.txt"])));
    assert!(t.s("msg").contains("Re-merged with main (conflicts resolved in 1 file), tests pass, review 1 still stands."), "{}", t.s("msg"));
}

#[test]
fn more_than_the_merge_needed_is_reviewed_again_a_file_that_did_not_conflict_or_a_commit_on_top() {
    for case in 0..2 {
        let (w, id, _) = passed();
        let main = main_moves(&w, &[("big.txt", &big("MAIN", "line 20"))]);
        let step = if case == 0 { json!({ "merge": main, "files": { "big.txt": big("TASK and MAIN", "line 20"), "extra.txt": "new work\n" }, "commit": "resolve", "note": "Resolved." }) }
            else { json!({ "merge": main, "files": { "big.txt": big("TASK and MAIN", "line 20") }, "commit": "resolve", "note": null, "status": "working" }) };
        w.script(json!({ "worker": [step] }));
        let mut t = resync(&w, &id);
        if t.s("stage") == "working" {
            // the second case: the merge, then new work in a commit of its own
            let tree = Path::new(t.s("worktree"));
            std::fs::write(tree.join("extra.txt"), "new work\n").unwrap(); git(tree, &["add", "-A"]); git(tree, &["commit", "-qm", "more"]);
            let note = sw::flow::task::note_file(&t);
            std::fs::create_dir_all(note.parent().unwrap()).unwrap();
            std::fs::write(&note, "Resolved, and more.").unwrap();
            // stamped later than anything the board does next: its clock stands still from here, however slow the machine
            let at = sw::clock::now(); sw::clock::set(Some(at));
            std::fs::File::options().write(true).open(&note).unwrap().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_millis(at as u64 + 2000)).unwrap();
            let pane = t.at("link").t("pane");
            w.patch(|s| { let seq = s.i("seq") + 1; s["seq"] = json!(seq); for p in s["panes"].as_array_mut().unwrap() { if p.s("pane_id") == pane { p["agent"]["status"] = json!("done"); p["agent"]["seq"] = json!(seq); p["agent"]["completion"] = json!(seq); } } });
            t = settle(&id); for _ in 0..2 { t = settle(&id); }
        }
        assert_eq!(t.s("stage"), "approval", "case {case}: {}", t.at("failure"));
        assert_eq!(t.i("reviewRound"), 2, "case {case}"); assert!(!t.at("review").b("keptAt")); assert_eq!(review_history(&t).len(), 2); assert!(!review_history(&t)[0].b("kept"));
    }
}

#[test]
fn tests_that_fail_on_the_re_merged_branch_go_back_to_the_agent_as_a_failed_check_does_its_fix_is_reviewed() {
    let (w, id, _) = passed();
    main_moves(&w, &[("big.txt", &big("line 1", "MAIN")), ("broken.txt", "breaks the check\n")]);
    w.script(json!({ "worker": [{ "note": null, "status": "working" }] }));
    let t = resync(&w, &id);
    assert_eq!(t.s("stage"), "working"); assert_eq!(t.at("failure").s("title"), "Check failed"); assert_eq!(t.i("reviewRound"), 1);
    let last = prompts(&w, t.at("link").s("pane")).last().cloned().unwrap_or_default();
    assert!(last.starts_with("The check failed, so this is not done yet. The branch was only re-merged with main since its review passed"), "{last}");
    assert!(!review_history(&t)[0].b("kept"), "nothing stood: the tests did not pass");
}

#[test]
fn a_review_that_did_not_pass_is_never_the_one_that_stands() {
    let w = world_with(Opts { repo: true, repo_settings: Some(json!({ "check.command": "true" })), settings: json!({ "review.rounds": 0 }), ..Default::default() });
    std::fs::write(w.root.join("big.txt"), big("line 1", "line 20")).unwrap(); git(&w.root, &["add", "-A"]); git(&w.root, &["commit", "-qm", "big"]);
    w.script(json!({ "worker": [{ "files": { "big.txt": big("TASK", "line 20") }, "commit": "task", "note": "Done." }], "reviewer": [{ "verdict": { "meets": false, "findings": [], "text": "Not what was asked." } }] }));
    let id = w.add("Failing");
    delegate(&id);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval"); assert_eq!(t.at("review").at("pass"), &json!(false)); assert!(sw::store::reviews::passed_review(&t).is_none());
    main_moves(&w, &[("big.txt", &big("line 1", "MAIN"))]);
    let t = resync(&w, &id);
    assert_eq!(t.s("stage"), "approval"); assert_eq!(t.i("reviewRound"), 2); assert!(!t.at("review").b("keptAt"));
}

#[test]
fn with_no_check_a_merge_into_the_tasks_own_files_is_reviewed_again_other_files_only_the_review_stands_and_says_so() {
    let w = world();
    std::fs::write(w.root.join("big.txt"), big("line 1", "line 20")).unwrap(); git(&w.root, &["add", "-A"]); git(&w.root, &["commit", "-qm", "big"]);
    w.script(json!({ "worker": [{ "files": { "big.txt": big("TASK", "line 20") }, "commit": "task", "note": "Done." }] }));
    let id = w.add("No check");
    delegate(&id);
    assert_eq!(settle(&id).s("stage"), "approval");
    main_moves(&w, &[("other.txt", "x\n")]);
    let t = resync(&w, &id);
    assert_eq!(t.i("reviewRound"), 1); assert_eq!(t.at("review").at("kept").s("check"), "");
    assert!(t.s("msg").contains("Re-merged with main, no check is set, review 1 still stands."), "{}", t.s("msg"));
    main_moves(&w, &[("big.txt", &big("line 1", "MAIN"))]);
    let t = resync(&w, &id);
    assert_eq!(t.s("stage"), "approval"); assert_eq!(t.i("reviewRound"), 2); assert!(!t.at("review").b("keptAt"));
}

#[test]
fn what_the_task_says_when_its_review_stood() {
    let _w = world_with(Opts::default());
    assert_eq!(sw::store::reviews::kept_words(&json!({ "check": "npm test", "resolved": [] }), 2), "re-merged with main, tests pass, review 2 still stands");
}

/// The rule itself, on plain git: what counts as "only the merge of main".
#[test]
fn merge_only_merges_of_main_and_their_conflict_resolutions_pass_anything_else_does_not() {
    let w = world_with(Opts::default());
    let dir = w.base.join("m"); std::fs::create_dir_all(&dir).unwrap();
    let d = dir.display().to_string();
    let write = |f: &str, t: &str| std::fs::write(dir.join(f), t).unwrap();
    let g = |a: &[&str]| { let _ = common::worktrees::git(&dir, a); };
    let commit = |m: &str| { git(&dir, &["add", "-A"]); git(&dir, &["commit", "-qm", m]); git(&dir, &["rev-parse", "HEAD"]) };
    let mo = |r: &str, m: &str| sw::sync::merge_only(&d, r, m);
    git(&dir, &["init", "-q", "-b", "main"]);
    write("a.txt", "one\n"); write("b.txt", "one\n"); write("c.txt", "one\n"); commit("init");
    git(&dir, &["checkout", "-q", "-b", "task"]); write("a.txt", "task\n");
    let reviewed = commit("task");
    git(&dir, &["checkout", "-q", "main"]); write("a.txt", "main\n"); write("b.txt", "main\n");
    let main = commit("main");
    let fresh = |name: &str| { git(&dir, &["checkout", "-q", "-B", name, &reviewed]); };
    let merge = |resolve: &dyn Fn()| { g(&["merge", "-q", "--no-edit", &main]); resolve(); commit("merge") };
    assert_eq!(mo(&reviewed, &main).at("ok"), &json!(false), "nothing merged yet");
    assert!(mo("", &main).s("why").contains("not known"));
    fresh("resolved"); merge(&|| write("a.txt", "both\n"));
    let m = mo(&reviewed, &main);
    assert_eq!((m.at("ok").clone(), m.i("merges"), m.at("resolved").clone()), (json!(true), 1, json!(["a.txt"])));
    fresh("evil"); merge(&|| { write("a.txt", "both\n"); write("c.txt", "sneaked in\n"); });
    let m = mo(&reviewed, &main);
    assert_eq!(m.at("ok"), &json!(false)); assert!(m.s("why").contains("changes more than its conflicts needed (c.txt)"), "{}", m.s("why"));
    fresh("ontop"); merge(&|| write("a.txt", "both\n")); write("c.txt", "more\n"); commit("more");
    assert!(mo(&reviewed, &main).s("why").contains("is new work, not a merge of main"));
    // another branch merged in is not main
    git(&dir, &["checkout", "-q", "-B", "side", &reviewed]); write("c.txt", "side\n"); let side = commit("side");
    fresh("other"); git(&dir, &["merge", "-q", "--no-ff", "--no-edit", &side]);
    assert!(mo(&reviewed, &main).s("why").contains("merges something that is not main"));
    // rebuilt on main: the reviewed commit is gone from the branch
    fresh("rebased");
    if common::worktrees::git(&dir, &["rebase", "-q", &main]).is_err() { write("a.txt", "both\n"); git(&dir, &["add", "-A"]); git(&dir, &["-c", "core.editor=true", "rebase", "--continue"]); }
    assert!(mo(&reviewed, &main).s("why").contains("no longer holds the reviewed commit"));
    // merged the other way round (main first): not the branch's own line
    git(&dir, &["checkout", "-q", "-B", "backwards", &main]); g(&["merge", "-q", "--no-edit", &reviewed]); write("a.txt", "both\n"); commit("merge");
    assert_eq!(mo(&reviewed, &main).at("ok"), &json!(false));
    // a clean merge with nothing to resolve
    git(&dir, &["checkout", "-q", "-B", "task2", &format!("{main}~1")]); write("c.txt", "task2\n"); let r2 = commit("task2");
    git(&dir, &["merge", "-q", "--no-edit", &main]);
    let m = mo(&r2, &main);
    assert_eq!((m.at("ok").clone(), m.i("merges"), m.at("resolved").clone()), (json!(true), 1, json!([])));
}
