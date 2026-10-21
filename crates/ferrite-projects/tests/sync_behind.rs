//! A task that could not be brought up to date with main is held as behind main: no approval until the sync really
//! happened, and nothing merged into it, or by it, while your checkout is on another branch.
use crate::common;
use common::*;
use serde_json::json;
use std::path::Path;
use sw::actions::{finish_line, talk};
use sw::json::*;
use sw::store::tasks::{update, Change};

fn holds(dir: &Path, sha: &str) -> bool { common::worktrees::git(dir, &["merge-base", "--is-ancestor", sha, "HEAD"]).is_ok() }
fn main_moves(w: &World, file: &str, body: &str) -> String { std::fs::write(w.root.join(file), body).unwrap(); git(&w.root, &["add", "-A"]); git(&w.root, &["commit", "-qm", &format!("main: {file}")]); git(&w.root, &["rev-parse", "HEAD"]) }
fn tree(t: &Obj) -> &Path { Path::new(t.s("worktree")) }
fn sync_jobs() -> usize { sw::runner::JOBS.with(|j| j.borrow().iter().filter(|(job, _, _)| job == "sync").count()) }
fn housekeep(w: &World) { sw::runner::JOBS.with(|j| j.borrow_mut().clear()); sw::housekeeping::housekeeping(&w.socket, sw::housekeeping::Force::Yes); }
fn behind(id: &str) { let e = finish_line::approve(id, None, false).unwrap_err().msg; assert!(e.contains("behind main and cannot be approved yet"), "{e}"); }
fn place(t: &Obj) -> String { sw::flow::sidebar::entry_now(t, sw::clock::now()).place }

#[test]
fn a_task_that_could_not_be_brought_up_to_date_is_held_as_behind_main_no_approval_until_the_sync_really_happened() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.when": "never" }), ..Default::default() });
    w.script(json!({ "worker": [{ "files": { "f.txt": "v1\n" }, "commit": "v1", "note": "Did v1." }] }));
    let id = w.add("Behind");
    delegate(&id);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval"); assert_eq!(row(&id).at("asReview"), &json!(true));
    let (seen, head) = (t.at("syncSeen").clone(), git(tree(&t), &["rev-parse", "HEAD"]));
    let target = main_moves(&w, "g.txt", "from main\n");
    git(tree(&t), &["checkout", "-q", "-b", "elsewhere"]); // not at rest on its own branch: nothing is merged here
    housekeep(&w);
    assert_eq!(sync_jobs(), 1);
    sw::runner::drain_jobs(&|job, id, extra| { sw::jobs::run_job(job, id, extra); });
    let t = task(&id);
    assert_eq!(t.s("stage"), "approval"); assert_eq!(t.at("syncSeen"), &seen, "the skipped attempt is not marked as seen"); assert!(t.i("syncRetryAt") > sw::clock::now());
    assert_eq!(git(tree(&t), &["rev-parse", "HEAD"]), head);
    assert!(t.at("syncBlocked").s("why").contains("not on its own branch"), "{}", t.at("syncBlocked")); assert!(t.s("msg").starts_with("Behind main"), "{}", t.s("msg"));
    assert_eq!((row(&id).at("asReview").clone(), place(&t)), (json!(false), "behind main".to_string()));
    behind(&id);
    assert_eq!(task(&id).s("stage"), "approval");
    // back on its branch, and main has not moved since: the retry comes, and only then can it be approved
    git(tree(&t), &["checkout", "-q", t.s("branch")]);
    update(&id, |t| { t.put("syncRetryAt", sw::clock::now() - 1); Ok(Change::Write) }).unwrap();
    housekeep(&w);
    assert_eq!(sync_jobs(), 1);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure")); assert!(t.at("syncBlocked").is_null()); assert!(t.s("msg").starts_with("Approve to merge?"), "{}", t.s("msg"));
    assert!(holds(tree(&t), &target)); assert_eq!(row(&id).at("asReview"), &json!(true));
    finish_line::approve(&id, None, false).unwrap();
    assert_eq!(settle(&id).s("stage"), "done");
}

#[test]
fn your_checkout_on_another_branch_nothing_is_merged_into_a_waiting_task_and_it_cannot_be_approved_until_you_are_back() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.when": "never" }), ..Default::default() });
    let kick = |id: &str| { update(id, |t| { t.put("syncRetryAt", 0); Ok(Change::Write) }).unwrap(); housekeep(&w); settle(id) };
    w.script(json!({ "worker": [{ "files": { "a.txt": "from the agent\n" }, "commit": "agent", "note": "Changed a.txt." }, { "files": { "f.txt": "more\n" }, "commit": "more", "note": "Added f.txt." }] }));
    let id = w.add("Elsewhere");
    delegate(&id);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval"); assert_eq!(t.s("syncBranch"), "main");
    let head = git(tree(&t), &["rev-parse", "HEAD"]);
    // you check out a side branch that conflicts with the task
    git(&w.root, &["checkout", "-q", "-b", "side"]);
    let side = main_moves(&w, "a.txt", "from the side branch\n");
    let t = kick(&id);
    assert_eq!(t.s("stage"), "approval"); assert_eq!(git(tree(&t), &["rev-parse", "HEAD"]), head); assert!(!holds(tree(&t), &side));
    assert!(t.at("syncBlocked").s("why").contains("your checkout is on side, not on main"), "{}", t.at("syncBlocked")); assert!(t.s("msg").starts_with("Behind main"));
    assert_eq!((row(&id).at("asReview").clone(), place(&t)), (json!(false), "behind main".to_string()));
    behind(&id);
    // a new turn of the agent does not make it approvable either: it was not checked against where it would land
    talk::tell(&id, "Add f.txt too.", false).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval"); assert!(t.b("syncBlocked")); assert!(!holds(tree(&t), &side));
    behind(&id);
    // back on main, which has not moved: the hold is lifted and it lands
    git(&w.root, &["checkout", "-q", "main"]);
    let t = kick(&id);
    assert_eq!(t.s("stage"), "approval"); assert!(t.at("syncBlocked").is_null()); assert!(t.s("msg").starts_with("Approve to merge?"), "{}", t.s("msg")); assert_eq!(row(&id).at("asReview"), &json!(true));
    // another branch at the very same commit is still another place to land: held as soon as the board looks
    git(&w.root, &["checkout", "-q", "-b", "twin"]);
    let t = kick(&id);
    assert!(t.at("syncBlocked").s("why").contains("your checkout is on twin, not on main"), "{}", t.at("syncBlocked"));
    behind(&id);
    // and an approval given before the board has looked merges nothing there
    update(&id, |t| { t.put("syncBlocked", serde_json::Value::Null); Ok(Change::Write) }).unwrap();
    let twin = git(&w.root, &["rev-parse", "HEAD"]);
    finish_line::approve(&id, None, false).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval"); assert_eq!(t.at("mergeFailed"), &json!(true)); assert_eq!(t.at("failure").s("title"), "Your checkout is on another branch"); assert!(t.at("mergeIntent").is_null());
    assert_eq!(git(&w.root, &["rev-parse", "HEAD"]), twin); assert_eq!(git(&w.root, &["status", "--porcelain"]), "");
    let e = finish_line::send_failure(&id).unwrap_err().msg;
    assert!(e.contains("Switch your checkout back to main"), "{e}");
    // back on main: approve again, and it lands there
    git(&w.root, &["checkout", "-q", "main"]);
    finish_line::approve(&id, None, false).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "done", "{}", t.at("failure"));
    assert_eq!(git(&w.root, &["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(read(&w.root.join("a.txt")), "from the agent\n");
}
