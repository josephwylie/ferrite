//! Automatic starts through the Server (#225, ADR 0037): a schedule on a
//! passed-in clock, GitHub starts against a scripted reader in place of
//! `gh`, a script's submissions from the CLI, and the Settings limit on
//! Tasks working at once.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicI64, Ordering};

use super::*;
use crate::factory::{GitHubItem, GitHubQuery, GitHubRead};
use crate::server::github::GitHub;

/// GitHub as the test scripts it: each read takes the next answer (an
/// unchanged poll once they run out) and is recorded.
#[derive(Default)]
struct Scripted {
    answers: Mutex<VecDeque<Result<GitHubRead, String>>>,
    asked: Mutex<Vec<GitHubQuery>>,
}

impl Scripted {
    fn will(&self, answer: Result<GitHubRead, String>) {
        self.answers.lock().unwrap().push_back(answer);
    }

    fn reads(&self) -> usize {
        self.asked.lock().unwrap().len()
    }
}

impl GitHub for Scripted {
    fn matches(&self, query: &GitHubQuery) -> Result<GitHubRead, String> {
        self.asked.lock().unwrap().push(query.clone());
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(GitHubRead::Unchanged))
    }

    fn repositories(&self) -> Result<Vec<String>, String> {
        Ok(vec!["acme/app".into(), "acme/site".into()])
    }

    fn labels(&self, repository: &str) -> Result<Vec<String>, String> {
        match repository {
            "acme/app" => Ok(vec!["bug".into(), "ready-for-agent".into()]),
            _ => Err(format!("{repository}: not found")),
        }
    }
}

/// A Server on a clock and a GitHub the test holds, with a Project.
struct World {
    harness: Harness,
    project: String,
    repo: PathBuf,
    clock: Arc<AtomicI64>,
    github: Arc<Scripted>,
}

/// Midnight, host-local, on Wednesday 2026-11-11.
fn midnight() -> i64 {
    use chrono::TimeZone;
    chrono::Local
        .with_ymd_and_hms(2026, 11, 11, 0, 0, 0)
        .earliest()
        .unwrap()
        .timestamp_millis()
}

const HOUR: i64 = 3_600_000;

fn world(name: &str) -> World {
    let clock = Arc::new(AtomicI64::new(midnight()));
    let github = Arc::new(Scripted::default());
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
        .unwrap_or_else(|| panic!("{opened}"))
        .to_string();
    World {
        harness,
        project,
        repo,
        clock,
        github,
    }
}

impl World {
    fn at(&self, ms: i64) {
        self.clock.store(ms, Ordering::SeqCst);
    }

    /// One CLI call that must succeed: its result.
    fn ok(&self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let (status, said, refused) = cli(&self.harness.api, method, params);
        assert_eq!((status, refused.len()), (0, 0), "{method}: {refused:?}");
        said[0]["result"].clone()
    }

    /// One CLI call that must be refused: its error.
    fn refused(&self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let (status, _, refused) = cli(&self.harness.api, method, params);
        assert_eq!(status, 1, "{method} was not refused");
        refused[0]["error"].clone()
    }

    /// A one-Step Workflow that starts as `start` says, saved.
    fn saved(&self, start: serde_json::Value) -> String {
        let made = self.ok(
            "workflow.create",
            serde_json::json!({ "project": self.project, "name": "Intake" }),
        );
        let workflow = made["workflow"]["id"].as_str().unwrap().to_string();
        let step = made["workflow"]["draft"]["steps"][0]["id"].clone();
        self.ok(
            "step.edit",
            serde_json::json!({ "workflow": workflow, "step": step, "instructions": "Do it" }),
        );
        self.ok(
            "workflow.start",
            serde_json::json!({ "workflow": workflow, "start": start }),
        );
        let saved = self.ok("workflow.save", serde_json::json!({ "workflow": workflow }));
        assert_eq!(saved["version"], 1, "{saved}");
        workflow
    }

    fn tasks(&self) -> Vec<serde_json::Value> {
        let listed = self.ok("task.list", serde_json::json!({ "project": self.project }));
        listed["tasks"].as_array().cloned().unwrap_or_default()
    }

    /// The Tasks once `done` holds of them, for at most five seconds.
    fn tasks_until(&self, done: impl Fn(&[serde_json::Value]) -> bool) -> Vec<serde_json::Value> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let tasks = self.tasks();
            if done(&tasks) {
                return tasks;
            }
            assert!(Instant::now() < deadline, "timed out: {tasks:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn automation(&self, workflow: &str) -> serde_json::Value {
        self.ok("workflow.get", serde_json::json!({ "workflow": workflow }))["workflow"]
            ["automation"]
            .clone()
    }

    /// Let the Server make a few passes.
    fn settle(&self) {
        std::thread::sleep(Duration::from_millis(600));
    }
}

/// AC: a schedule saved starts nothing until its switch is on; then a
/// run due on the passed-in clock starts one Task with the schedule's
/// instructions and a Worker; paused, the next one starts nothing.
#[test]
fn a_schedule_starts_a_task_at_its_time_only_once_turned_on() {
    let world = world("starts-schedule");
    let workflow = world.saved(serde_json::json!({
        "type": "schedule",
        "schedule": {
            "recurrence": "interval", "interval_hours": 1,
            "instructions": "Triage new issues since the last run",
        },
    }));
    world.at(midnight() + HOUR);
    world.settle();
    assert!(world.tasks().is_empty(), "saving starts nothing");
    assert_eq!(world.automation(&workflow), serde_json::Value::Null);

    let on = world.ok(
        "workflow.automation",
        serde_json::json!({ "workflow": workflow, "turn": "on" }),
    );
    assert_eq!(on["changed"], true, "{on}");
    assert_eq!(on["workflow"]["automation"]["switch"], "on", "{on}");
    assert_eq!(
        on["workflow"]["automation"]["due_ms"],
        midnight() + 2 * HOUR,
        "{on}"
    );
    world.at(midnight() + 2 * HOUR);
    let tasks = world.tasks_until(|tasks| tasks.len() == 1);
    assert_eq!(
        tasks[0]["instructions"],
        "Triage new issues since the last run"
    );
    assert_eq!(tasks[0]["source"]["kind"], "schedule", "{:?}", tasks[0]);
    world
        .harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .expect("its Worker started");

    world.ok(
        "workflow.automation",
        serde_json::json!({ "workflow": workflow, "turn": "pause" }),
    );
    world.at(midnight() + 3 * HOUR);
    world.settle();
    assert_eq!(world.tasks().len(), 1, "paused starts nothing");
}

fn issue(number: u64, title: &str) -> GitHubItem {
    GitHubItem {
        number,
        title: title.into(),
        body: format!("Steps to reproduce {number}"),
        url: format!("https://github.com/acme/app/issues/{number}"),
    }
}

fn matching(items: Vec<GitHubItem>) -> Result<GitHubRead, String> {
    Ok(GitHubRead::Matches {
        etag: Some("W/\"1\"".into()),
        items,
    })
}

/// AC: a GitHub start missing its label is refused; turned on, it is read
/// off the pump through the reader about once a minute, conditionally;
/// each matching issue starts one Task with its title, description,
/// extra instructions and link, and the same issue again starts nothing;
/// a failed read is said in the Workflow, never a Task, and is retried
/// at the next check. A backlog found at the first read waits for the
/// operator's word.
#[test]
fn github_issues_start_one_task_each_and_a_failed_read_is_reported() {
    let world = world("starts-github");
    let made = world.ok(
        "workflow.create",
        serde_json::json!({ "project": world.project }),
    );
    let unlabelled = made["workflow"]["id"].as_str().unwrap().to_string();
    let step = made["workflow"]["draft"]["steps"][0]["id"].clone();
    world.ok(
        "step.edit",
        serde_json::json!({ "workflow": unlabelled, "step": step, "instructions": "Do it" }),
    );
    let problems = world.ok(
        "workflow.start",
        serde_json::json!({ "workflow": unlabelled, "start": {
            "type": "github_issue", "github": { "repository": "acme/app" },
        }}),
    )["problems"]
        .clone();
    assert_eq!(
        problems,
        serde_json::json!([{ "message": "Choose the label to check for." }])
    );
    let refused = world.refused(
        "workflow.save",
        serde_json::json!({ "workflow": unlabelled }),
    );
    assert!(
        refused["message"].as_str().unwrap().contains("label"),
        "{refused}"
    );

    let workflow = world.saved(serde_json::json!({
        "type": "github_issue",
        "github": {
            "repository": "acme/app", "trigger": "labelled", "label": "ready-for-agent",
            "extra_instructions": "Open a PR that closes the issue.",
        },
    }));
    world.settle();
    assert_eq!(world.github.reads(), 0, "off reads nothing");

    world.github.will(matching(vec![issue(3, "Old crash")]));
    world.ok(
        "workflow.automation",
        serde_json::json!({ "workflow": workflow, "turn": "on" }),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while world.automation(&workflow)["backlog"].is_null() {
        assert!(Instant::now() < deadline, "no backlog");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        world.tasks().is_empty(),
        "the backlog waits for the operator"
    );
    world.ok(
        "workflow.backlog",
        serde_json::json!({ "workflow": workflow, "take": false }),
    );

    world.github.will(matching(vec![
        issue(3, "Old crash"),
        issue(7, "Login fails"),
    ]));
    world.at(midnight() + POLL);
    let tasks = world.tasks_until(|tasks| tasks.len() == 1);
    let task = &tasks[0];
    assert_eq!(task["title"], "Login fails");
    for expected in [
        "Login fails",
        "Steps to reproduce 7",
        "Open a PR that closes the issue.",
        "https://github.com/acme/app/issues/7",
    ] {
        assert!(
            task["instructions"].as_str().unwrap().contains(expected),
            "{expected}: {task}"
        );
    }
    assert_eq!(
        task["source"]["url"],
        "https://github.com/acme/app/issues/7"
    );
    assert_eq!(world.github.reads(), 2);
    let asked = world.github.asked.lock().unwrap().clone();
    assert_eq!(asked[0].etag, None);
    assert_eq!(asked[1].etag.as_deref(), Some("W/\"1\""), "conditional");

    world.github.will(matching(vec![issue(7, "Login fails")]));
    world.github.will(Err("gh: HTTP 502".into()));
    world.at(midnight() + 2 * POLL);
    while world.github.reads() < 3 {
        std::thread::sleep(Duration::from_millis(10));
    }
    world.at(midnight() + 3 * POLL);
    let deadline = Instant::now() + Duration::from_secs(5);
    while world.automation(&workflow)["error"].is_null() {
        assert!(Instant::now() < deadline, "the failure was not said");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(world.automation(&workflow)["error"], "gh: HTTP 502");
    assert_eq!(world.tasks().len(), 1, "seen again, or failed: no new Task");

    world.at(midnight() + 4 * POLL);
    while world.github.reads() < 5 {
        std::thread::sleep(Duration::from_millis(10));
    }
    world.settle();
    assert_eq!(
        world.automation(&workflow)["error"],
        serde_json::Value::Null,
        "retried and cleared"
    );
}

const POLL: i64 = crate::factory::POLL_MS;

/// AC: the pickers list what the operator's login reaches, through the
/// Server's reader, never typed.
#[test]
fn a_window_lists_repositories_and_labels_from_the_login() {
    let world = world("starts-lists");
    let mut window = cockpit(&world.harness);
    let names = |window: &mut crate::cockpit::Cockpit, ask| {
        let answer = window.github_names(ask).unwrap();
        let landed = std::cell::RefCell::new(None);
        pump_until(window, |_| {
            if let Ok(read) = answer.try_recv() {
                *landed.borrow_mut() = Some(read);
            }
            landed.borrow().is_some()
        });
        landed.into_inner().unwrap()
    };
    assert_eq!(
        names(&mut window, crate::protocol::GitHubAsk::Repositories).unwrap(),
        ["acme/app", "acme/site"]
    );
    assert_eq!(
        names(
            &mut window,
            crate::protocol::GitHubAsk::Labels {
                repository: "acme/app".into()
            }
        )
        .unwrap(),
        ["bug", "ready-for-agent"]
    );
    assert!(names(
        &mut window,
        crate::protocol::GitHubAsk::Labels {
            repository: "acme/site".into()
        }
    )
    .is_err());
}

/// AC: a script submits from the CLI; a submission needs instructions
/// and an identity and a live script start, and a repeated identity
/// returns the existing Task.
#[test]
fn a_script_submission_starts_one_task_per_identity() {
    let world = world("starts-script");
    let workflow = world.saved(serde_json::json!({ "type": "external" }));
    let submission = |identity: &str, instructions: &str| {
        serde_json::json!({
            "workflow": workflow, "identity": identity, "instructions": instructions,
        })
    };
    let off = world.refused("task.submit", submission("JIRA-12", "Fix the export"));
    assert_eq!(off["code"], "not_now", "{off}");
    world.ok(
        "workflow.automation",
        serde_json::json!({ "workflow": workflow, "turn": "on" }),
    );
    let blank = world.refused("task.submit", submission("", "Fix the export"));
    assert_eq!(blank["code"], "invalid_request", "{blank}");
    let first = world.ok("task.submit", submission("JIRA-12", "Fix the export"));
    assert_eq!(first["duplicate"], false, "{first}");
    assert_eq!(first["task"]["source"]["identity"], "JIRA-12");
    let again = world.ok("task.submit", submission("JIRA-12", "Fix the export"));
    assert_eq!(again["duplicate"], true, "{again}");
    assert_eq!(again["task"]["id"], first["task"]["id"]);
    assert_eq!(world.tasks().len(), 1);
}

/// AC: Settings bound how many Tasks work at once; the excess waits in
/// Queued and is released as a Task leaves Working.
#[test]
fn the_settings_limit_queues_the_excess_and_releases_it() {
    let world = world("starts-limit");
    let settings = crate::settings::Settings {
        factory_tasks_at_once: 1,
        ..Default::default()
    };
    settings.save(&world.harness.dir).unwrap();
    let mut window = cockpit(&world.harness);
    window.settings_changed(&settings);
    std::thread::sleep(Duration::from_millis(300));
    for instructions in ["First", "Second"] {
        world.ok(
            "task.start",
            serde_json::json!({ "instructions": instructions, "cwd": world.repo }),
        );
    }
    let tasks = world.tasks_until(|tasks| tasks[0]["worker"].is_u64());
    world.settle();
    let tasks_now = world.tasks();
    assert_eq!(tasks_now[0]["status"], "working", "{tasks:?}");
    assert_eq!(tasks_now[1]["status"], "queued", "{tasks_now:?}");
    let first = tasks_now[0]["id"].clone();
    world.ok("task.stop", serde_json::json!({ "task": first }));
    let released = world.tasks_until(|tasks| tasks[1]["status"] == "working");
    assert_eq!(released[0]["status"], "stopped");
}

impl World {
    /// Stop the Server; while it is down, move the clock to `at_ms` and
    /// script GitHub's next answers with `will`; then serve its Store
    /// again on the same clock and GitHub: a restart.
    fn restart(self, at_ms: i64, will: Vec<Result<GitHubRead, String>>) -> World {
        let mut control = attach(&self.harness.address);
        drop(control.command(226, Op::Stop { when: When::Now }));
        let World {
            harness,
            project,
            repo,
            clock,
            github,
        } = self;
        assert_eq!(harness.server.join().unwrap(), 0);
        gone(&harness.address);
        gone(&harness.api);
        clock.store(at_ms, Ordering::SeqCst);
        for answer in will {
            github.will(answer);
        }
        let (time, reader) = (Arc::clone(&clock), Arc::clone(&github));
        let harness = serve_configured(
            harness.dir.clone(),
            None,
            Tuning::default(),
            Box::new(move |server: &mut Server| {
                server.clock = Arc::new(move || time.load(Ordering::SeqCst));
                server.github = reader;
            }),
        );
        World {
            harness,
            project,
            repo,
            clock,
            github,
        }
    }
}

/// AC: a schedule's run that fell due while the Server was down, and a
/// GitHub match that arrived then, each start one Task after the restart,
/// once; what was seen before it is not started again; the switches and
/// the schedule's state are intact.
#[cfg(unix)]
#[test]
fn starts_that_arrived_during_a_restart_are_handled_once() {
    let world = world("starts-restart");
    let schedule = world.saved(serde_json::json!({
        "type": "schedule",
        "schedule": {
            "recurrence": "interval", "interval_hours": 1,
            "instructions": "Triage new issues since the last run",
        },
    }));
    let github = world.saved(serde_json::json!({
        "type": "github_issue",
        "github": { "repository": "acme/app", "trigger": "labelled", "label": "ready-for-agent" },
    }));
    world.ok(
        "workflow.automation",
        serde_json::json!({ "workflow": schedule, "turn": "on" }),
    );
    world.github.will(matching(vec![issue(3, "Old crash")]));
    world.ok(
        "workflow.automation",
        serde_json::json!({ "workflow": github, "turn": "on" }),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while world.automation(&github)["backlog"].is_null() {
        assert!(Instant::now() < deadline, "no backlog");
        std::thread::sleep(Duration::from_millis(10));
    }
    world.ok(
        "workflow.backlog",
        serde_json::json!({ "workflow": github, "take": false }),
    );

    // Down across the schedule's run; a new issue is labelled meanwhile.
    let world = world.restart(
        midnight() + HOUR + 30_000,
        vec![matching(vec![issue(3, "Old crash"), issue(7, "Login fails")])],
    );
    assert_eq!(world.automation(&schedule)["switch"], "on");
    assert_eq!(world.automation(&github)["switch"], "on");
    let tasks = world.tasks_until(|tasks| tasks.len() == 2);
    let mut titles: Vec<String> = tasks
        .iter()
        .map(|task| task["title"].as_str().unwrap().to_string())
        .collect();
    titles.sort();
    assert_eq!(
        titles,
        vec!["Login fails", "Triage new issues since the last run"]
    );
    world.settle();
    assert_eq!(world.tasks().len(), 2, "once");

    // And once more across a second restart: nothing is started again.
    let world = world.restart(
        midnight() + HOUR + 60_000,
        vec![matching(vec![issue(3, "Old crash"), issue(7, "Login fails")])],
    );
    world.settle();
    assert_eq!(world.tasks().len(), 2, "nothing again");
    assert_eq!(
        world.automation(&schedule)["due_ms"],
        midnight() + 2 * HOUR,
        "the schedule's next run is kept"
    );
}
