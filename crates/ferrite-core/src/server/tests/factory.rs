//! The Factory through the Server (#220, ADR 0037): a one-off Task
//! started by the CLI or a window runs a Worker in its own worktree, is
//! reported finished or unable, is stopped, resumed and deleted, and the
//! windows hear what needs the operator. No window is needed for any of
//! it but the alerts.

use super::*;

/// A Project on a fresh repository: the harness, its Project's id and the
/// repository.
fn factory_project(name: &str) -> (Harness, String, PathBuf) {
    let harness = start(name);
    let repo = repo_in(&harness.dir);
    let opened = call(&harness).answer("p", "project.open", serde_json::json!({ "cwd": repo }));
    let project = opened["result"]["project"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("{opened}"))
        .to_string();
    (harness, project, repo)
}

/// Ask until `done` holds of the Task's JSON, for at most five seconds.
fn task_until(
    caller: &mut Caller,
    task: &str,
    done: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let got = caller.answer("g", "task.get", serde_json::json!({ "task": task }));
        let task = got["result"]["task"].clone();
        if done(&task) {
            return task;
        }
        assert!(Instant::now() < deadline, "timed out: {got}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A Task's Worker once it has one.
fn worker_of(caller: &mut Caller, task: &str) -> ThreadId {
    let task = task_until(caller, task, |task| task["worker"].is_u64());
    ThreadId::new(task["worker"].as_u64().unwrap())
}

/// What the agent of `thread` was told, as `agent.read` gives it.
fn read(caller: &mut Caller, thread: ThreadId) -> String {
    let read = caller.answer("r", "agent.read", serde_json::json!({ "thread": thread }));
    read["result"]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn started(said: &[serde_json::Value]) -> String {
    said[0]["result"]["task"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("{said:?}"))
        .to_string()
}

/// AC: a fresh Factory is empty and starts nothing; `task start` from the
/// CLI makes a Task, its worktree and branch, and one Worker owned by it
/// with its instructions; the Worker's report (naming only its own
/// Thread) completes it, and the same report again changes nothing. No
/// window is attached throughout.
#[test]
fn a_task_from_the_cli_runs_one_worker_in_its_own_worktree_with_no_window() {
    let (harness, project, repo) = factory_project("factory-cli");
    let mut caller = call(&harness);
    let listed = caller.answer("1", "task.list", serde_json::json!({ "project": project }));
    assert_eq!(listed["result"]["tasks"], serde_json::json!([]), "{listed}");
    assert!(harness.sessions.try_recv().is_err(), "nothing started");

    let (status, said, refused) = cli(
        &harness.api,
        "task.start",
        serde_json::json!({
            "instructions": "Fix the flaky test\nIt fails one run in ten.",
            "cwd": repo,
        }),
    );
    assert_eq!((status, refused.len()), (0, 0), "{refused:?}");
    let task = started(&said);
    assert_eq!(said[0]["result"]["task"]["title"], "Fix the flaky test");
    let _agent = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .expect("a Worker started");
    let worker = worker_of(&mut caller, &task);

    let info = caller.answer("2", "thread.get", serde_json::json!({ "thread": worker }));
    let info = &info["result"]["thread"];
    assert_eq!(info["owner"], serde_json::json!({ "task": task }), "{info}");
    assert_eq!(info["branch"], format!("factory/{task}"), "{info}");
    let tree = PathBuf::from(info["worktree"].as_str().expect("its own worktree"));
    assert!(tree.join("file.txt").exists(), "{info}");
    assert_eq!(
        task_until(&mut caller, &task, |_| true)["worktree"],
        serde_json::json!(tree)
    );
    let told = read(&mut caller, worker);
    assert!(told.contains("It fails one run in ten."), "{told}");
    assert!(told.contains("report outcome finished"), "{told}");

    let report = || {
        cli(
            &harness.api,
            "report.outcome",
            serde_json::json!({
                "outcome": "finished",
                "summary": "Fixed the race in setup",
                "worker": worker,
            }),
        )
    };
    let (status, said, refused) = report();
    assert_eq!((status, refused.len()), (0, 0), "{refused:?}");
    assert_eq!(said[0]["result"]["changed"], true, "{said:?}");
    let done = task_until(&mut caller, &task, |task| task["status"] == "completed");
    assert_eq!(done["result"], "Fixed the race in setup");
    let (_, said, _) = report();
    assert_eq!(
        said[0]["result"]["changed"], false,
        "a repeat changes nothing"
    );
    assert!(harness.sessions.try_recv().is_err(), "and starts nothing");

    let (_, _, refused) = cli(
        &harness.api,
        "report.outcome",
        serde_json::json!({ "outcome": "finished", "summary": "x", "worker": 999 }),
    );
    assert_eq!(refused[0]["error"]["code"], "not_a_worker", "{refused:?}");
}

/// AC: a Worker at rest without a report is prompted once; with still no
/// report its Task needs the operator, and the window is told (toast,
/// chime and system notification) and counts it; an unable report moves
/// a Task there with what was tried. Nothing reaches the Inbox.
#[test]
fn a_worker_that_never_reports_is_prompted_once_then_needs_the_operator() {
    let (harness, project, repo) = factory_project("factory-at-rest");
    let mut window = cockpit(&harness);
    let mut caller = call(&harness);
    let task = caller.answer(
        "1",
        "task.start",
        serde_json::json!({ "instructions": "Tidy the README", "project": project, "cwd": repo }),
    )["result"]["task"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let agent = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let worker = worker_of(&mut caller, &task);

    turn_ends(&agent);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !read(&mut caller, worker).contains("without reporting") {
        assert!(Instant::now() < deadline, "never asked to report");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        task_until(&mut caller, &task, |_| true)["status"],
        "working"
    );
    turn_ends(&agent);
    let stuck = task_until(&mut caller, &task, |task| task["status"] == "needs_human");
    assert_eq!(
        read(&mut caller, worker)
            .matches("without reporting")
            .count(),
        1,
        "asked once"
    );
    assert!(stuck["reason"].as_str().is_some(), "{stuck}");

    let project_id: crate::workspace::registry::ProjectId =
        serde_json::from_value(serde_json::json!(project.parse::<u64>().unwrap())).unwrap();
    pump_until(&mut window, |window| {
        window.factory().needs_you(project_id) == 1
    });
    let asked = window.take_asked();
    assert!(
        asked.iter().any(|asked| matches!(
            asked,
            crate::cockpit::Asked::TaskAlert {
                status: crate::factory::Status::NeedsHuman,
                ..
            }
        )),
        "{asked:?}"
    );
    assert!(window.needs_you().is_empty(), "nothing in the Inbox");
}

/// AC: Stop interrupts the Worker and stops the Task; Resume prompts it
/// on; what the operator says to the Worker from a window is recorded on
/// the Task; Delete is refused while it works, and removes its worktree
/// only when clean, saying so otherwise, and never its branch.
#[test]
fn a_task_is_stopped_resumed_and_deleted_leaving_dirty_work_alone() {
    use crate::workspace::git_for_tests as git;
    let (harness, project, repo) = factory_project("factory-stop");
    let mut window = attach(&harness.address);
    let mut caller = call(&harness);
    let start = |window: &mut Window, id: u64, instructions: &str| {
        let reply = window.command(
            id,
            Op::Task {
                action: protocol::TaskAction::Start {
                    project: serde_json::from_value(serde_json::json!(project
                        .parse::<u64>()
                        .unwrap()))
                    .unwrap(),
                    instructions: instructions.into(),
                    request: Some(format!("ask-{id}")),
                    workflow: None,
                    pr: None,
                },
            },
        );
        match reply {
            ServerMessage::Reply {
                ok: Some(Done::Task { task }),
                ..
            } => task,
            reply => panic!("{reply:?}"),
        }
    };
    let task = start(&mut window, 10, "Rename the config key");
    assert_eq!(
        start(&mut window, 10, "Rename the config key"),
        task,
        "one Task per ask"
    );
    let agent = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let worker = worker_of(&mut caller, &task);

    let refused = caller.answer("1", "task.delete", serde_json::json!({ "task": task }));
    assert_eq!(refused["error"]["code"], "not_now", "{refused}");

    // The operator steers the Worker from its Task: recorded on the Task.
    window.command(
        11,
        Op::Send {
            thread: worker,
            text: "Keep the old key as an alias".into(),
        },
    );
    let said = task_until(&mut caller, &task, |task| {
        task["activity"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["text"] == "Keep the old key as an alias")
    });
    let entry = said["activity"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["text"] == "Keep the old key as an alias")
        .unwrap()
        .clone();
    assert_eq!(entry["by"], "operator");

    let stopped = caller.answer("2", "task.stop", serde_json::json!({ "task": task }));
    assert_eq!(stopped["result"]["task"]["status"], "stopped", "{stopped}");
    let again = caller.answer("3", "task.stop", serde_json::json!({ "task": task }));
    assert_eq!(again["result"]["changed"], false, "{again}");
    // The interrupt lands: the turn ends.
    turn_ends(&agent);
    caller.answer("w", "agent.wait", serde_json::json!({ "thread": worker }));

    let resumed = caller.answer("4", "task.resume", serde_json::json!({ "task": task }));
    assert_eq!(resumed["result"]["task"]["status"], "working", "{resumed}");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !read(&mut caller, worker).contains("resumed this task") {
        assert!(Instant::now() < deadline, "never told to carry on");
        std::thread::sleep(Duration::from_millis(10));
    }
    turn_ends(&agent);
    caller.answer("5", "task.stop", serde_json::json!({ "task": task }));

    // Dirty: deleted, the worktree left and said so, its branch kept.
    let tree = PathBuf::from(
        task_until(&mut caller, &task, |_| true)["worktree"]
            .as_str()
            .unwrap(),
    );
    std::fs::write(tree.join("notes.txt"), "mine\n").unwrap();
    let deleted = caller.answer("6", "task.delete", serde_json::json!({ "task": task }));
    let kept = deleted["result"]["kept"]
        .as_str()
        .unwrap_or_else(|| panic!("{deleted}"));
    assert!(kept.contains("uncommitted"), "{kept}");
    assert!(tree.join("notes.txt").exists());
    assert!(
        !git(&repo, &["branch", "--list", &format!("factory/{task}")])
            .trim()
            .is_empty()
    );
    let gone = caller.answer("7", "task.get", serde_json::json!({ "task": task }));
    assert_eq!(gone["error"]["code"], "unknown_task", "{gone}");
    let parked = caller.answer("8", "thread.get", serde_json::json!({ "thread": worker }));
    assert_eq!(
        parked["error"]["code"], "unknown_thread",
        "the Worker is parked"
    );

    // Clean: its worktree goes, its branch stays.
    let clean = start(&mut window, 20, "Bump the version");
    worker_of(&mut caller, &clean);
    let _ = harness.sessions.recv_timeout(Duration::from_secs(5));
    caller.answer("9", "task.stop", serde_json::json!({ "task": clean }));
    let tree = PathBuf::from(
        task_until(&mut caller, &clean, |_| true)["worktree"]
            .as_str()
            .unwrap(),
    );
    let deleted = caller.answer("10", "task.delete", serde_json::json!({ "task": clean }));
    assert!(deleted["result"].get("kept").is_none(), "{deleted}");
    assert!(!tree.exists());
    assert!(
        !git(&repo, &["branch", "--list", &format!("factory/{clean}")])
            .trim()
            .is_empty()
    );
}

/// AC (#221): a Workflow authored wholly from the CLI, with custom Step
/// names, is saved as version 1 and that starts nothing; a Task on it
/// runs its Steps one at a time, each Worker on its own Step's Provider,
/// model and Effort, told its instructions and skills, the Task's request
/// and the earlier Steps' results, in the Task's one worktree. Editing
/// and saving version 2 while it runs changes nothing for it, and the
/// last Step's plain (non-PR) result completes it.
#[test]
fn a_workflow_task_runs_each_step_with_its_own_agent_and_context() {
    let (harness, project, repo) = factory_project("factory-workflow");
    let mut caller = call(&harness);
    let author = |method: &str, params: serde_json::Value| {
        let (status, said, refused) = cli(&harness.api, method, params);
        assert_eq!((status, refused.len()), (0, 0), "{method}: {refused:?}");
        said[0]["result"].clone()
    };
    let made = author(
        "workflow.create",
        serde_json::json!({ "project": project, "name": "Release notes" }),
    );
    let workflow = made["workflow"]["id"].as_str().unwrap().to_string();
    assert_eq!(made["workflow"]["draft"]["name"], "Release notes", "{made}");
    let scout = made["workflow"]["draft"]["steps"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    author(
        "step.edit",
        serde_json::json!({
            "workflow": workflow, "step": scout, "name": "Scout",
            "instructions": "Map the code", "on_finished": "next",
        }),
    );
    let added = author("step.add", serde_json::json!({ "workflow": workflow }));
    let writer = added["step"].as_str().unwrap().to_string();
    let edited = author(
        "step.edit",
        serde_json::json!({
            "workflow": workflow, "step": writer, "name": "Writer",
            "instructions": "Write the changelog", "skills": ["changelog"],
            "provider": "codex", "model": "gpt-test", "effort": "high",
            "on_finished": "complete",
        }),
    );
    assert_eq!(edited["problems"], serde_json::json!([]), "{edited}");
    let saved = author("workflow.save", serde_json::json!({ "workflow": workflow }));
    assert_eq!(saved["version"], 1, "{saved}");
    assert!(
        harness.sessions.try_recv().is_err(),
        "saving starts nothing"
    );

    let (status, said, refused) = cli(
        &harness.api,
        "task.start",
        serde_json::json!({
            "instructions": "Release notes for 2.0", "cwd": repo, "workflow": workflow,
        }),
    );
    assert_eq!((status, refused.len()), (0, 0), "{refused:?}");
    let task = started(&said);
    let _first = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .expect("Scout's Worker");
    let scouting = worker_of(&mut caller, &task);
    let told = read(&mut caller, scouting);
    assert!(told.contains("Map the code"), "{told}");
    assert!(told.contains("Release notes for 2.0"), "{told}");
    assert!(harness.sessions.try_recv().is_err(), "one Step at a time");

    author(
        "step.edit",
        serde_json::json!({
            "workflow": workflow, "step": writer, "instructions": "Write it twice",
        }),
    );
    let again = author("workflow.save", serde_json::json!({ "workflow": workflow }));
    assert_eq!(again["version"], 2, "{again}");

    let (status, _, refused) = cli(
        &harness.api,
        "report.outcome",
        serde_json::json!({
            "outcome": "finished", "summary": "Found three modules", "worker": scouting,
        }),
    );
    assert_eq!((status, refused.len()), (0, 0), "{refused:?}");
    let _second = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .expect("Writer's Worker");
    let moved = task_until(&mut caller, &task, |task| {
        task["worker"].is_u64() && task["worker"] != serde_json::json!(scouting)
    });
    assert_eq!(moved["status"], "working", "{moved}");
    assert_eq!(moved["workflow"]["version"], 1, "it keeps its version");
    let writing = ThreadId::new(moved["worker"].as_u64().unwrap());
    let told = read(&mut caller, writing);
    for expected in [
        "Write the changelog",
        "changelog",
        "Release notes for 2.0",
        "Found three modules",
        "Writer",
    ] {
        assert!(told.contains(expected), "{expected}: {told}");
    }
    assert!(!told.contains("twice"), "version 1's instructions: {told}");
    let launches = harness.launches.lock().unwrap().clone();
    assert_eq!(
        launches.last(),
        Some(&(
            Provider::Codex,
            Some("gpt-test".into()),
            Some("high".into())
        )),
        "{launches:?}"
    );
    let tree = |thread: ThreadId, caller: &mut Caller| {
        caller.answer("t", "thread.get", serde_json::json!({ "thread": thread }))["result"]
            ["thread"]["worktree"]
            .clone()
    };
    assert_eq!(
        tree(writing, &mut caller),
        tree(scouting, &mut caller),
        "one shared worktree"
    );

    cli(
        &harness.api,
        "report.outcome",
        serde_json::json!({
            "outcome": "finished", "summary": "Wrote CHANGELOG.md", "worker": writing,
        }),
    );
    let done = task_until(&mut caller, &task, |task| task["status"] == "completed");
    assert_eq!(done["result"], "Wrote CHANGELOG.md");
    assert!(harness.sessions.try_recv().is_err(), "nothing more started");
}

mod direction;
mod merging;
mod recovery;
