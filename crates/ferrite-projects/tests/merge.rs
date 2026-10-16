//! Merging an approved task into your checkout: unsaved files there stop it and nothing is forced; a commit that
//! reaches the branch during the merge is neither merged unseen nor deleted as merged.
use crate::common;
use common::*;
use serde_json::json;
use std::path::Path;
use sw::actions::finish_line;
use sw::json::*;

#[test]
fn unsaved_files_in_your_checkout_stop_the_merge_and_nothing_is_forced() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.when": "never" }), ..Default::default() });
    let id = to_approval(&w, "Work");
    std::fs::write(w.root.join("mine.txt"), "unsaved\n").unwrap();
    finish_line::approve(&id, None, false).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval");
    assert_eq!(t.at("failure").s("title"), "Merge needs a clean checkout");
    assert_eq!(t.at("failure").at("agentCanFix"), &json!(false));
    let e = finish_line::send_failure(&id).unwrap_err().msg;
    assert!(e.contains("Commit, stash or move"), "{e}");
    assert!(w.root.join("mine.txt").exists());
}

#[test]
fn a_commit_that_reaches_the_branch_while_it_is_being_merged_is_neither_merged_unseen_nor_deleted_as_merged() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.when": "never" }), ..Default::default() });
    let id = to_approval(&w, "Work");
    let t = task(&id);
    let tree = Path::new(t.s("worktree"));
    let reviewed = git(tree, &["rev-parse", "HEAD"]);
    // your checkout's pre-commit hook stands in for the agent: it commits to the task's branch in the middle of the merge
    let hook = w.root.join(".git/hooks/pre-commit");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(&hook, format!("#!/bin/sh\n[ -n \"$SW_LATE\" ] && exit 0\n(unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_PREFIX; cd \"{}\" && echo late > late.txt && SW_LATE=1 git add -A && SW_LATE=1 git commit -qm late)\n", tree.display())).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    finish_line::approve(&id, None, false).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "done", "{}", t.at("failure"));
    assert!(regex::Regex::new(r"^Merged into main\. Kept: .*new work arrived during clean-up").unwrap().is_match(t.s("msg")), "{}", t.s("msg"));
    assert_eq!(read(&w.root.join("f.txt")), "v1\n");
    assert!(!w.root.join("late.txt").exists(), "what was reviewed landed, nothing else");
    assert!(tree.join("late.txt").exists());
    assert_eq!(git(&w.root, &["log", "-1", "--format=%s", t.s("branch")]), "late");
    assert_eq!(git(&w.root, &["rev-parse", &format!("{}~1", t.s("branch"))]), reviewed);
    assert!(!w.state().arr("workspaces").iter().any(|x| x.s("workspace_id") == t.at("link").s("workspace")));
}
