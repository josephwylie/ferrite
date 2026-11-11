//! Recovery and the Archive through the Server (#226): a restart mid-Step
//! and mid-approval, a scripted Session that dies, another Project's
//! Factory left alone, an unreadable store, and Clear.

use super::*;

/// The Server's CLI, refused nothing: its answer's `result`.
fn ok(harness: &Harness, method: &str, params: serde_json::Value) -> serde_json::Value {
    let (status, said, refused) = cli(&harness.api, method, params);
    assert_eq!((status, refused.len()), (0, 0), "{method}: {refused:?}");
    said[0]["result"].clone()
}

fn one_off(harness: &Harness, repo: &Path, instructions: &str) -> String {
    ok(
        harness,
        "task.start",
        serde_json::json!({ "instructions": instructions, "cwd": repo }),
    )["task"]["id"]
        .as_str()
        .unwrap()
        .to_string()
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

/// Stop the Server as `ferrite server stop` does and serve its Store
/// again: a restart.
fn restart(harness: Harness) -> Harness {
    let mut control = attach(&harness.address);
    control.command(226, Op::Stop { when: When::Now });
    assert_eq!(harness.server.join().unwrap(), 0);
    gone(&harness.address);
    gone(&harness.api);
    serve_in(harness.dir.clone(), None)
}

fn said(task: &serde_json::Value) -> Vec<(String, String)> {
    task["activity"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["by"].as_str().unwrap().to_string(),
                entry["text"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// AC (case 16, "a scripted Session that dies"): a Worker's Session that
/// ends mid-Step, with no restart, is revived once on the same Thread and
/// told so, said in the activity as Ferrite's, apart from an agent's
/// unable report; when it dies again the Task stops with the reason, for
/// the operator to resume. Another Project's Task works on untouched,
/// and completes.
#[test]
fn a_session_that_dies_is_revived_once_then_stops_and_another_project_is_untouched() {
    let (harness, _, repo) = factory_project("factory-dies");
    let other_repo = repo_in(&harness.dir.join("other"));
    ok(
        &harness,
        "project.open",
        serde_json::json!({ "cwd": other_repo }),
    );
    let mut caller = call(&harness);
    let task = one_off(&harness, &repo, "Fix the flaky test");
    let agent = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let worker = worker_of(&mut caller, &task);
    let other = one_off(&harness, &other_repo, "Bump the version");
    let other_agent = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let other_worker = worker_of(&mut caller, &other);

    exit(agent);
    let revived = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .expect("the same Thread on a new Session");
    let read = told(&mut caller, worker, "ended unexpectedly");
    assert!(read.contains("report outcome finished"), "{read}");
    let held = task_until(&mut caller, &task, |task| {
        said(task)
            .iter()
            .any(|(_, text)| text.contains("Session interrupted"))
    });
    assert_eq!(held["status"], "working", "{held}");
    assert_eq!(held["worker"], serde_json::json!(worker), "same Thread");
    let (by, text) = said(&held)
        .into_iter()
        .find(|(_, text)| text.contains("Session interrupted"))
        .unwrap();
    assert_eq!(by, "ferrite", "Ferrite's word, not the agent's");
    assert!(text.contains("ended unexpectedly"), "{text}");
    assert!(!text.contains("Unable"), "{text}");

    exit(revived);
    let stopped = task_until(&mut caller, &task, |task| task["status"] == "stopped");
    let reason = stopped["reason"].as_str().unwrap();
    assert!(reason.contains("revived once"), "{reason}");
    assert!(
        harness.sessions.try_recv().is_err(),
        "nothing is started again unasked"
    );

    let mid = task_until(&mut caller, &other, |_| true);
    assert_eq!(mid["status"], "working", "{mid}");
    assert!(!read_contains(&mut caller, other_worker, "interrupted"));
    ok(
        &harness,
        "report.outcome",
        serde_json::json!({ "outcome": "finished", "summary": "Bumped", "worker": other_worker }),
    );
    let done = task_until(&mut caller, &other, |task| task["status"] == "completed");
    assert_eq!(done["result"], "Bumped");
    drop(other_agent);

    ok(&harness, "task.resume", serde_json::json!({ "task": task }));
    let _resumed = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .expect("Resume starts its Session");
    told(&mut caller, worker, "resumed this task");
}

fn read_contains(caller: &mut Caller, thread: ThreadId, text: &str) -> bool {
    read(caller, thread).contains(text)
}

/// A one-Step Workflow that routes to the operator's approval, saved.
fn approved_workflow(harness: &Harness, project: &str) -> String {
    let made = ok(
        harness,
        "workflow.create",
        serde_json::json!({ "project": project, "name": "Reviewed" }),
    );
    let workflow = made["workflow"]["id"].as_str().unwrap().to_string();
    let step = made["workflow"]["draft"]["steps"][0]["id"].clone();
    ok(
        harness,
        "step.edit",
        serde_json::json!({
            "workflow": workflow, "step": step, "name": "Builder",
            "instructions": "Build it", "on_finished": "approval",
        }),
    );
    ok(
        harness,
        "workflow.approval",
        serde_json::json!({ "workflow": workflow, "rework_to": format!("step:{}", step.as_str().unwrap()) }),
    );
    ok(
        harness,
        "workflow.save",
        serde_json::json!({ "workflow": workflow }),
    );
    workflow
}

/// AC (case 16): after a restart mid-Step the working Task's Worker is
/// revived once, on the same Thread, and told Ferrite restarted; a Task
/// Ready to approve is still waiting with nothing started, and its
/// approval completes it; the revived Worker failing again stops its Task.
/// Clear then moves the Completed Task to the Archive, kept whole, and
/// deleting the Task deletes its Worker.
#[cfg(unix)]
#[test]
fn a_restart_mid_step_and_mid_approval_revives_once_and_waits() {
    let (harness, project, repo) = factory_project("factory-restart");
    let workflow = approved_workflow(&harness, &project);
    let mut caller = call(&harness);
    let waiting = ok(
        &harness,
        "task.start",
        serde_json::json!({ "instructions": "Add a --fast flag", "cwd": repo, "workflow": workflow }),
    )["task"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let building = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let builder = worker_of(&mut caller, &waiting);
    ok(
        &harness,
        "report.outcome",
        serde_json::json!({ "outcome": "finished", "summary": "Added --fast", "worker": builder }),
    );
    turn_ends(&building);
    task_until(&mut caller, &waiting, |task| task["status"] == "approval");
    let working = one_off(&harness, &repo, "Fix the flaky test");
    let _agent = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let worker = worker_of(&mut caller, &working);

    let harness = restart(harness);
    let mut caller = call(&harness);
    let revived = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .expect("the working Worker, revived");
    told(&mut caller, worker, "Ferrite restarted");
    let still = task_until(&mut caller, &working, |_| true);
    assert_eq!(still["status"], "working", "{still}");
    assert_eq!(still["worker"], serde_json::json!(worker));
    let held = task_until(&mut caller, &waiting, |_| true);
    assert_eq!(held["status"], "approval", "{held}");
    assert_eq!(held["result"], "Added --fast");
    assert!(
        harness.sessions.try_recv().is_err(),
        "nothing else started early"
    );

    exit(revived);
    let stopped = task_until(&mut caller, &working, |task| task["status"] == "stopped");
    assert!(
        stopped["reason"].as_str().unwrap().contains("revived once"),
        "{stopped}"
    );

    let approved = ok(
        &harness,
        "task.approve",
        serde_json::json!({ "task": waiting }),
    );
    assert_eq!(approved["task"]["status"], "completed", "{approved}");
    let cleared = ok(&harness, "task.clear", serde_json::json!({ "cwd": repo }));
    assert_eq!(cleared["changed"], true, "{cleared}");
    let archived = task_until(&mut caller, &waiting, |task| task["archived_ms"].is_i64());
    assert_eq!(archived["status"], "completed");
    assert!(!said(&archived).is_empty(), "kept whole: {archived}");
    assert_eq!(
        archived["workers"][0]["thread"],
        serde_json::json!(builder),
        "{archived}"
    );

    let parked = caller.answer("g", "thread.get", serde_json::json!({ "thread": builder }));
    assert_eq!(
        parked["error"]["code"], "unknown_thread",
        "parked: {parked}"
    );
    let log = harness.dir.join("threads").join(builder.get().to_string());
    assert!(log.exists(), "parked, not deleted: {}", log.display());
    ok(
        &harness,
        "task.delete",
        serde_json::json!({ "task": waiting }),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while log.exists() {
        assert!(Instant::now() < deadline, "deleted with its Task");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// AC: a Factory store that cannot be read is said plainly, refuses to
/// start anything over it, and is left as it was.
#[cfg(unix)]
#[test]
fn an_unreadable_factory_store_is_said_and_never_started_afresh() {
    let (harness, _, repo) = factory_project("factory-unreadable");
    one_off(&harness, &repo, "Fix the flaky test");
    let _agent = harness.sessions.recv_timeout(Duration::from_secs(5));
    let mut control = attach(&harness.address);
    control.command(226, Op::Stop { when: When::Now });
    assert_eq!(harness.server.join().unwrap(), 0);
    gone(&harness.address);
    gone(&harness.api);
    let file = harness.dir.join("threads").join(crate::factory::FILE_NAME);
    let mut torn = std::fs::read(&file).unwrap();
    torn.truncate(torn.len() / 2);
    std::fs::write(&file, &torn).unwrap();

    let harness = serve_in(harness.dir.clone(), None);
    let listed = ok(&harness, "task.list", serde_json::json!({}));
    assert!(
        listed["trouble"]
            .as_str()
            .is_some_and(|trouble| trouble.contains("could not be read")),
        "{listed}"
    );
    let (status, _, refused) = cli(
        &harness.api,
        "task.start",
        serde_json::json!({ "instructions": "Another", "cwd": repo }),
    );
    assert_eq!(status, 1);
    let message = refused[0]["error"]["message"].as_str().unwrap();
    assert!(message.contains("could not be read"), "{message}");
    assert_eq!(std::fs::read(&file).unwrap(), torn, "never written over");
}
