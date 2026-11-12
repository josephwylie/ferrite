//! Pull requests and merging through the Server (#223): a Task's PR from
//! the CLI, Workers reporting its revisions and evidence, a merge Step
//! whose scripted Session reports each result, and a merged Task's
//! worktree tidied (or kept, and why). No real PR is merged and no
//! remote is touched: the repository is a scratch one.

use super::*;

fn ok(harness: &Harness, method: &str, params: serde_json::Value) -> serde_json::Value {
    let (status, said, refused) = cli(&harness.api, method, params);
    assert_eq!((status, refused.len()), (0, 0), "{method}: {refused:?}");
    said[0]["result"].clone()
}

/// Until `thread` was told `text`.
fn told(caller: &mut Caller, thread: ThreadId, text: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let read = read(caller, thread);
        if read.contains(text) {
            return read;
        }
        assert!(Instant::now() < deadline, "never told {text:?}: {read}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn next_worker(caller: &mut Caller, task: &str, was: ThreadId) -> ThreadId {
    let moved = task_until(caller, task, |task| {
        task["worker"].is_u64() && task["worker"] != serde_json::json!(was)
    });
    ThreadId::new(moved["worker"].as_u64().unwrap())
}

fn report(harness: &Harness, worker: ThreadId, outcome: &str, summary: &str) -> serde_json::Value {
    ok_in(
        harness,
        worker,
        "report.outcome",
        serde_json::json!({ "outcome": outcome, "summary": summary }),
    )
}

/// Steps named and instructed as given, saved after `shape` set their
/// routes by id: the Workflow and its Step ids.
fn workflow(
    harness: &Harness,
    project: &str,
    steps: &[(&str, &str)],
    shape: impl Fn(&[String]) -> Vec<serde_json::Value>,
) -> (String, Vec<String>) {
    let made = ok(
        harness,
        "workflow.create",
        serde_json::json!({ "project": project, "name": "Ship it" }),
    );
    let workflow = made["workflow"]["id"].as_str().unwrap().to_string();
    let mut ids = vec![made["workflow"]["draft"]["steps"][0]["id"]
        .as_str()
        .unwrap()
        .to_string()];
    for _ in 1..steps.len() {
        let added = ok(
            harness,
            "step.add",
            serde_json::json!({ "workflow": workflow }),
        );
        ids.push(added["step"].as_str().unwrap().to_string());
    }
    for ((name, instructions), id) in steps.iter().zip(&ids) {
        ok(
            harness,
            "step.edit",
            serde_json::json!({
                "workflow": workflow, "step": id, "name": name, "instructions": instructions,
            }),
        );
    }
    let mut last = serde_json::Value::Null;
    for mut change in shape(&ids) {
        change["workflow"] = serde_json::json!(workflow);
        last = ok(harness, "step.edit", change);
    }
    assert_eq!(last["problems"], serde_json::json!([]), "{last}");
    let saved = ok(
        harness,
        "workflow.save",
        serde_json::json!({ "workflow": workflow }),
    );
    assert_eq!(saved["version"], 1, "{saved}");
    (workflow, ids)
}

const HEAD_1: &str = "1111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEAD_2: &str = "2222222bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// AC (cases 9, 17, 18): a Task started with a PR (a bad one refused)
/// tells every Step its PR and revision; the merge Step's scripted
/// Session reports pending (it keeps waiting), conflict (the resolver
/// runs and hands back with a new revision), unknown (it is reconciled
/// before any retry) and merged (the Task completes); the clean worktree
/// is then removed and its branch kept. Evidence keeps each kind apart
/// with its revision.
#[test]
fn a_merge_step_reports_each_result_and_a_merged_task_tidies_its_worktree() {
    let (harness, project, repo) = factory_project("factory-merge");
    let (workflow, steps) = workflow(
        &harness,
        &project,
        &[
            ("Build", "Build the flag"),
            ("Merge", "Merge the PR"),
            ("Resolve", "Resolve the conflicts"),
        ],
        |ids| {
            vec![
                serde_json::json!({ "step": ids[0], "on_finished": "next" }),
                serde_json::json!({
                    "step": ids[1], "by_merge_result": true,
                    "on_conflict": format!("step:{}", ids[2]),
                }),
                serde_json::json!({ "step": ids[2], "on_finished": format!("step:{}", ids[1]) }),
            ]
        },
    );
    let mut caller = call(&harness);
    let (_, _, refused) = cli(
        &harness.api,
        "task.start",
        serde_json::json!({
            "instructions": "Add a --fast flag", "cwd": repo, "workflow": workflow,
            "pr": "https://gitlab.com/acme/widgets/pull/12",
        }),
    );
    assert_eq!(refused.len(), 1, "a bad PR is refused");
    let started = ok(
        &harness,
        "task.start",
        serde_json::json!({
            "instructions": "Add a --fast flag", "cwd": repo, "workflow": workflow,
            "pr": "https://github.com/acme/widgets/pull/12",
        }),
    );
    let task = started["task"]["id"].as_str().unwrap().to_string();
    assert_eq!(started["task"]["pr"]["repository"], "acme/widgets");

    let _building = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let builder = worker_of(&mut caller, &task);
    told(&mut caller, builder, "acme/widgets#12");
    let reported = ok_in(
        &harness,
        builder,
        "report.revision",
        serde_json::json!({ "head": HEAD_1 }),
    );
    assert_eq!(reported["changed"], true, "{reported}");
    ok_in(
        &harness,
        builder,
        "report.evidence",
        serde_json::json!({ "text": "cargo test: 12 passed" }),
    );
    report(&harness, builder, "finished", "Opened the PR");

    let merging = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let merger = next_worker(&mut caller, &task, builder);
    let read = told(&mut caller, merger, "Merge the PR");
    assert!(
        read.contains("revision 1") && read.contains(HEAD_1),
        "{read}"
    );

    report(&harness, merger, "pending", "CI is running");
    turn_ends(&merging);
    told(&mut caller, merger, "still pending");
    let waiting = task_until(&mut caller, &task, |_| true);
    assert_eq!(waiting["status"], "working", "{waiting}");
    assert_eq!(waiting["worker"], serde_json::json!(merger));

    report(&harness, merger, "conflict", "Conflicts in src/flags.rs");
    turn_ends(&merging);
    let _resolving = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let resolver = next_worker(&mut caller, &task, merger);
    told(&mut caller, resolver, "Resolve the conflicts");
    ok_in(
        &harness,
        resolver,
        "report.revision",
        serde_json::json!({ "head": HEAD_2 }),
    );
    report(&harness, resolver, "finished", "Resolved and pushed");
    let back = task_until(&mut caller, &task, |task| {
        task["worker"] == serde_json::json!(merger)
    });
    assert_eq!(back["step"], serde_json::json!(steps[1]), "{back}");
    told(&mut caller, merger, "revision 2");

    report(&harness, merger, "unknown", "gh pr merge timed out");
    turn_ends(&merging);
    let held = task_until(&mut caller, &task, |task| task["status"] == "needs_human");
    assert!(held["pr"].get("merged").is_none(), "{held}");
    ok(
        &harness,
        "task.redirect",
        serde_json::json!({ "task": task, "to": steps[1] }),
    );
    let merger = next_worker(&mut caller, &task, merger);
    let read = told(&mut caller, merger, "## Reconcile first");
    assert!(read.contains("acme/widgets#12 is already merged"), "{read}");

    let worktree = PathBuf::from(held["worktree"].as_str().unwrap());
    assert!(worktree.exists());
    report(&harness, merger, "merged", "gh pr view shows MERGED");
    let done = task_until(&mut caller, &task, |task| {
        task["status"] == "completed" && task.get("worktree").is_none()
    });
    assert_eq!(done["pr"]["merged"], true, "{done}");
    assert!(!worktree.exists(), "the clean worktree is removed");
    assert!(
        !crate::workspace::git_for_tests(&repo, &["branch", "--list", "factory/t-1"])
            .trim()
            .is_empty(),
        "the branch stays"
    );

    let evidence: Vec<(String, String, serde_json::Value)> = done["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|each| {
            (
                each["kind"].as_str().unwrap().to_string(),
                each["by"].as_str().unwrap().to_string(),
                each["revision"].clone(),
            )
        })
        .collect();
    assert_eq!(
        evidence[0],
        ("tool".into(), "Build".into(), serde_json::json!(1))
    );
    assert_eq!(
        evidence[1],
        ("conclusion".into(), "Build".into(), serde_json::json!(1))
    );
    assert!(evidence.contains(&("decision".into(), "You".into(), serde_json::json!(2))));
    assert_eq!(
        evidence.last().unwrap(),
        &("conclusion".into(), "Merge".into(), serde_json::json!(2))
    );
}

/// AC (case 17): a merged Task whose checkout has uncommitted changes
/// keeps its worktree, never forced, and says why.
#[test]
fn a_merged_task_with_a_dirty_checkout_keeps_its_worktree_and_says_why() {
    let (harness, project, repo) = factory_project("factory-merge-dirty");
    let (workflow, _) = workflow(&harness, &project, &[("Merge", "Merge the PR")], |ids| {
        vec![serde_json::json!({ "step": ids[0], "by_merge_result": true })]
    });
    let mut caller = call(&harness);
    let started = ok(
        &harness,
        "task.start",
        serde_json::json!({
            "instructions": "Merge it", "cwd": repo, "workflow": workflow,
            "pr": "acme/widgets#3",
        }),
    );
    let task = started["task"]["id"].as_str().unwrap().to_string();
    let _merging = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let merger = worker_of(&mut caller, &task);
    let at = task_until(&mut caller, &task, |task| task["worktree"].is_string());
    let worktree = PathBuf::from(at["worktree"].as_str().unwrap());
    std::fs::write(worktree.join("notes.txt"), "unsaved").unwrap();

    report(&harness, merger, "merged", "gh pr view shows MERGED");
    let done = task_until(&mut caller, &task, |task| {
        task["status"] == "completed" && task["kept"].is_string()
    });
    assert!(
        done["kept"]
            .as_str()
            .unwrap()
            .contains("uncommitted changes"),
        "{done}"
    );
    assert!(worktree.join("notes.txt").exists(), "nothing was discarded");
}
