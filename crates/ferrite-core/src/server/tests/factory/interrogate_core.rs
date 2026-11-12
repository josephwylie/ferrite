//! Interrogate pass (#238), the Core owner's share: Server-side repros of
//! Factory defects whose cause is in the Factory module. Each test asserts
//! what the intent requires.

use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use super::*;
use crate::factory::{GitHubItem, GitHubQuery, GitHubRead};
use crate::server::github::GitHub;

fn ok_call(harness: &Harness, method: &str, params: serde_json::Value) -> serde_json::Value {
    let (status, said, refused) = cli(&harness.api, method, params);
    assert_eq!((status, refused.len()), (0, 0), "{method}: {refused:?}");
    said[0]["result"].clone()
}

/// A one-off Task started from the CLI: its id, Worker and scripted
/// Session.
fn one_off(
    harness: &Harness,
    caller: &mut Caller,
    repo: &Path,
    instructions: &str,
) -> (String, ThreadId, Sender<SessionEvent>) {
    let started = ok_call(
        harness,
        "task.start",
        serde_json::json!({ "instructions": instructions, "cwd": repo }),
    );
    let task = started["task"]["id"].as_str().unwrap().to_string();
    let agent = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .expect("a Worker started");
    let worker = worker_of(caller, &task);
    (task, worker, agent)
}

/// GitHub as a conditional server: an unchanged list answers 304 to its
/// own tag. A read can be held at a gate.
#[derive(Default)]
struct Conditional {
    current: Mutex<(String, Vec<GitHubItem>)>,
    gate: Mutex<Option<Receiver<()>>>,
    reads: AtomicUsize,
    asked: Mutex<Vec<Option<String>>>,
}

impl GitHub for Conditional {
    fn matches(&self, query: &GitHubQuery) -> Result<GitHubRead, String> {
        self.asked.lock().unwrap().push(query.etag.clone());
        self.reads.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = self.gate.lock().unwrap().take() {
            let _ = gate.recv();
        }
        let (etag, items) = self.current.lock().unwrap().clone();
        if query.etag.as_deref() == Some(etag.as_str()) {
            return Ok(GitHubRead::Unchanged);
        }
        Ok(GitHubRead::Matches {
            etag: Some(etag),
            items,
        })
    }
    fn repositories(&self) -> Result<Vec<String>, String> {
        Ok(vec!["acme/app".into()])
    }
    fn labels(&self, _repository: &str) -> Result<Vec<String>, String> {
        Ok(vec!["ready".into()])
    }
    fn head(&self, repository: &str, number: u64) -> Result<String, String> {
        Err(format!("{repository}#{number}: not found"))
    }
}

fn gh_issue(number: u64, title: &str) -> GitHubItem {
    GitHubItem {
        number,
        title: title.into(),
        body: String::new(),
        url: format!("https://github.com/acme/app/issues/{number}"),
    }
}

/// A Server on a test clock and GitHub, with a Project and its repo.
fn clocked(name: &str, github: Arc<Conditional>) -> (Harness, String, PathBuf, Arc<AtomicI64>) {
    let clock = Arc::new(AtomicI64::new(1_800_000_000_000));
    let (time, reader) = (Arc::clone(&clock), Arc::clone(&github));
    let harness = serve_configured(
        scratch(name),
        None,
        Tuning::default(),
        Box::new(move |server: &mut Server| {
            server.clock = Arc::new(move || time.load(Ordering::SeqCst));
            server.github = reader;
        }),
    );
    let repo = repo_in(&harness.dir);
    let opened = call(&harness).answer("p", "project.open", serde_json::json!({ "cwd": repo }));
    let project = opened["result"]["project"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    (harness, project, repo, clock)
}

fn saved_workflow(harness: &Harness, project: &str, start: serde_json::Value) -> String {
    let made = ok_call(
        harness,
        "workflow.create",
        serde_json::json!({ "project": project, "name": "Intake" }),
    );
    let workflow = made["workflow"]["id"].as_str().unwrap().to_string();
    let step = made["workflow"]["draft"]["steps"][0]["id"].clone();
    ok_call(
        harness,
        "step.edit",
        serde_json::json!({ "workflow": workflow, "step": step, "instructions": "Do it" }),
    );
    ok_call(
        harness,
        "workflow.start",
        serde_json::json!({ "workflow": workflow, "start": start }),
    );
    let saved = ok_call(
        harness,
        "workflow.save",
        serde_json::json!({ "workflow": workflow }),
    );
    assert_eq!(saved["version"], 1, "{saved}");
    workflow
}

fn wait_for(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Intent: each matching issue starts one Task. A read that comes back
/// while its start is paused is dropped, but its tag is kept first
/// (`Factory::heard` runs before the command is applied): resumed, the
/// next read is conditional on that tag, GitHub answers 304, and the
/// issue that arrived in the dropped read never starts a Task.
#[test]
fn interrogate_a_github_read_dropped_while_paused_loses_its_issue() {
    let github = Arc::new(Conditional::default());
    *github.current.lock().unwrap() = ("\"1\"".into(), vec![gh_issue(3, "Old crash")]);
    let (harness, project, _repo, clock) = clocked("ix-gh-pause", Arc::clone(&github));
    let workflow = saved_workflow(
        &harness,
        &project,
        serde_json::json!({
            "type": "github_issue",
            "github": { "repository": "acme/app", "trigger": "labelled", "label": "ready" },
        }),
    );
    ok_call(
        &harness,
        "workflow.automation",
        serde_json::json!({ "workflow": workflow, "turn": "on" }),
    );
    let automation = |harness: &Harness| {
        ok_call(
            harness,
            "workflow.get",
            serde_json::json!({ "workflow": workflow }),
        )["workflow"]["automation"]
            .clone()
    };
    wait_for("the backlog", || !automation(&harness)["backlog"].is_null());
    ok_call(
        &harness,
        "workflow.backlog",
        serde_json::json!({ "workflow": workflow, "take": false }),
    );

    // Issue 7 is labelled; the next read is slow, and the operator
    // pauses the start while it is out.
    let (release, gate) = mpsc::channel();
    *github.gate.lock().unwrap() = Some(gate);
    *github.current.lock().unwrap() = (
        "\"2\"".into(),
        vec![gh_issue(3, "Old crash"), gh_issue(7, "Login fails")],
    );
    let t0 = clock.load(Ordering::SeqCst);
    clock.store(t0 + crate::factory::POLL_MS, Ordering::SeqCst);
    wait_for("the second read", || {
        github.reads.load(Ordering::SeqCst) == 2
    });
    ok_call(
        &harness,
        "workflow.automation",
        serde_json::json!({ "workflow": workflow, "turn": "pause" }),
    );
    release.send(()).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    ok_call(
        &harness,
        "workflow.automation",
        serde_json::json!({ "workflow": workflow, "turn": "resume" }),
    );
    clock.store(t0 + 3 * crate::factory::POLL_MS, Ordering::SeqCst);
    wait_for("the read after resuming", || {
        github.reads.load(Ordering::SeqCst) == 3
    });
    clock.store(t0 + 5 * crate::factory::POLL_MS, Ordering::SeqCst);
    wait_for("another read", || github.reads.load(Ordering::SeqCst) == 4);
    std::thread::sleep(Duration::from_millis(500));
    let tasks = ok_call(
        &harness,
        "task.list",
        serde_json::json!({ "project": project }),
    )["tasks"]
        .clone();
    eprintln!(
        "tags asked: {:?}; tasks: {}",
        github.asked.lock().unwrap(),
        tasks.as_array().unwrap().len()
    );
    assert_eq!(
        tasks.as_array().unwrap().len(),
        1,
        "issue 7 starts one Task once resumed: {tasks}"
    );
}

// ------------------------------------------------ a schedule goes silent

/// Intent: a schedule runs at its times; a missed run is skipped or run
/// once. A clock that jumps a week ahead and is corrected back (a wrong
/// RTC at boot, then NTP) leaves `due_ms` a week out: the hourly schedule
/// starts nothing for a week.
#[test]
fn interrogate_a_clock_corrected_back_silences_a_schedule() {
    let github = Arc::new(Conditional::default());
    let (harness, project, _repo, clock) = clocked("ix-clock-back", github);
    let workflow = saved_workflow(
        &harness,
        &project,
        serde_json::json!({
            "type": "schedule",
            "schedule": {
                "recurrence": "interval", "interval_hours": 1,
                "instructions": "Hourly triage", "missed": "skip",
            },
        }),
    );
    const HOUR: i64 = 3_600_000;
    let t0 = clock.load(Ordering::SeqCst);
    ok_call(
        &harness,
        "workflow.automation",
        serde_json::json!({ "workflow": workflow, "turn": "on" }),
    );
    let due = |harness: &Harness| {
        ok_call(
            harness,
            "workflow.get",
            serde_json::json!({ "workflow": workflow }),
        )["workflow"]["automation"]["due_ms"]
            .as_i64()
    };
    let first = due(&harness).expect("a due time");
    eprintln!("due after turning on: {first} (now {t0})");
    // The clock jumps a week ahead for a moment ...
    clock.store(t0 + 7 * 24 * HOUR, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(300));
    eprintln!("due during the jump: {:?}", due(&harness));
    // ... and is corrected; three hours pass, each run seen on time (a
    // run seen late is missed, and this schedule skips a missed run).
    for hours in 0..3 {
        clock.store(first + hours * HOUR + 1_000, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(300));
    }
    wait_for("a run after the correction", || {
        ok_call(
            &harness,
            "task.list",
            serde_json::json!({ "project": project }),
        )["tasks"]
            .as_array()
            .is_some_and(|tasks| !tasks.is_empty())
    });
    let tasks = ok_call(
        &harness,
        "task.list",
        serde_json::json!({ "project": project }),
    )["tasks"]
        .clone();
    eprintln!(
        "after the correction and three on-time hours: due {:?}, tasks {}",
        due(&harness),
        tasks.as_array().unwrap().len()
    );
    assert!(
        !tasks.as_array().unwrap().is_empty(),
        "the hourly schedule runs again once the clock is right"
    );
}

// ------------------------------------- a parked Worker is never missed

/// Intent: a lost Worker is revived once, then the Task is Stopped. The
/// operator's `project.close` ("Park every open Thread of a Project")
/// parks a working Task's Worker too; a parked Thread is left out of
/// what the step sees, so the Task stays Working for good: never
/// revived, never nudged, never stopped, holding a place in the limit.
#[test]
fn interrogate_a_worker_parked_by_project_close_leaves_its_task_working_forever() {
    let (harness, project, repo) = factory_project("ix-parked-worker");
    let mut caller = call(&harness);
    let (task, worker, _agent) = one_off(&harness, &mut caller, &repo, "Fix the login bug");
    let closed = ok_call(
        &harness,
        "project.close",
        serde_json::json!({ "project": project }),
    );
    eprintln!("project.close parked {}", closed["parked"]);
    std::thread::sleep(Duration::from_secs(3));
    let after = task_until(&mut caller, &task, |_| true);
    let thread = caller.answer("t", "thread.get", serde_json::json!({ "thread": worker }));
    eprintln!(
        "3 s later: task status {} reason {}; worker thread.get: {}",
        after["status"], after["reason"], thread["error"]["code"]
    );
    assert_eq!(
        after["status"], "stopped",
        "a Task whose Worker was parked does not stay Working: {after}"
    );
    let resumed = ok_call(&harness, "task.resume", serde_json::json!({ "task": task }));
    eprintln!("resumed: {}", resumed["task"]["status"]);
    let reopened = task_until(&mut caller, &task, |task| task["status"] == "working");
    assert_eq!(
        reopened["worker"],
        serde_json::json!(worker),
        "Resume reopens its Worker"
    );
    let thread = caller.answer("t", "thread.get", serde_json::json!({ "thread": worker }));
    assert!(
        thread["error"].is_null(),
        "the Worker is open again: {thread}"
    );
}

// ------------------------- a Project's store trouble cuts off every window

/// Ask the Server at `address` to stop, as `ferrite server stop` does,
/// without reading anything it says back.
fn stop_raw(address: &Path) {
    let mut stream = link::connect(address).unwrap();
    link::send(
        &mut stream,
        &ClientMessage::Hello {
            generation: 1,
            client: Peer {
                name: "test".into(),
                version: "0".into(),
            },
            capabilities: Vec::new(),
        },
    )
    .unwrap();
    link::send(
        &mut stream,
        &ClientMessage::Command(crate::protocol::Command {
            id: 1,
            op: Op::Stop { when: When::Now },
        }),
    )
    .unwrap();
}

/// Intent: one Project's store failing does not harm another's. With
/// Project A's part unreadable at load, the ids it used are not counted:
/// Project B's next Task is `t-1` again. Once A's file is readable again,
/// two Tasks share the id `t-1`, and `task.get t-1` reaches only one.
#[cfg(unix)]
#[test]
fn interrogate_an_unreadable_part_lets_another_project_reuse_its_ids() {
    let (harness, project_a, repo_a) = factory_project("ix-ids");
    let mut caller = call(&harness);
    let (a_task, _worker, _agent) = one_off(&harness, &mut caller, &repo_a, "A's task");
    // A second Project, its own repository.
    let repo_b = harness.dir.join("other");
    std::fs::create_dir_all(&repo_b).unwrap();
    {
        use crate::workspace::git_for_tests as git;
        git(&repo_b, &["init", "-q", "-b", "main"]);
        std::fs::write(repo_b.join("b.txt"), "b\n").unwrap();
        git(&repo_b, &["add", "b.txt"]);
        git(
            &repo_b,
            &[
                "-c",
                "user.email=t@example.invalid",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "b",
            ],
        );
    }
    let opened = caller.answer("p", "project.open", serde_json::json!({ "cwd": repo_b }));
    let project_b = opened["result"]["project"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    drop(caller);

    let part = harness
        .dir
        .join("threads")
        .join(crate::factory::DIR_NAME)
        .join(format!("{project_a}.json"));
    let kept = std::fs::read(&part).unwrap();
    std::fs::write(&part, b"{ not json").unwrap();
    let harness = restart(harness);
    let mut caller = call(&harness);
    let started = ok_call(
        &harness,
        "task.start",
        serde_json::json!({ "instructions": "B's task", "cwd": repo_b }),
    );
    let b_task = started["task"]["id"].as_str().unwrap().to_string();
    eprintln!("A's task was {a_task}; with A unreadable, B's new task is {b_task}");

    std::fs::write(&part, kept).unwrap();
    // No window can attach to this Server (see the trouble repro below),
    // so it is stopped over a raw link.
    stop_raw(&harness.address);
    assert_eq!(harness.server.join().unwrap(), 0);
    gone(&harness.address);
    gone(&harness.api);
    let harness = serve_in(harness.dir.clone(), None);
    let listed = ok_call(&harness, "task.list", serde_json::json!({}));
    let ids: Vec<(String, String)> = listed["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| {
            (
                task["id"].as_str().unwrap().to_string(),
                task["project"].to_string(),
            )
        })
        .collect();
    let got = call(&harness).answer("g", "task.get", serde_json::json!({ "task": "t-1" }));
    eprintln!(
        "after A is readable again: tasks {ids:?}; task.get t-1 -> project {} ({} / {})",
        got["result"]["task"]["project"], project_a, project_b
    );
    let _ = &mut caller;
    assert_ne!(a_task, b_task, "an id is never given twice");
}
