//! Repros of the Server-side review findings on #238 (server #1-#6).
//! Each test asserts what the intent requires.

use super::*;

/// One CLI command line run from inside `thread` (or from no Thread),
/// parsed by the real CLI parser and sent to the Server: exit status,
/// stdout lines, stderr lines.
fn cli_line(
    harness: &Harness,
    thread: Option<ThreadId>,
    cwd: Option<&Path>,
    op: &str,
    args: &[&str],
) -> (
    i32,
    Vec<serde_json::Value>,
    Vec<serde_json::Value>,
    crate::api::Request,
) {
    let operation = crate::api::Method::OPERATIONS
        .iter()
        .find(|each| each.name == op)
        .unwrap();
    let context = crate::api::cli::Context {
        thread: thread.map(|thread| thread.to_string()),
        cwd: cwd.map(Path::to_path_buf),
    };
    let request = crate::api::cli::parse(operation, args, &context).unwrap();
    let ((mut out, said), (mut err, refused)) = (printed(), printed());
    let status = crate::api::cli::call(&request, &harness.api, &mut out, &mut err);
    drop((out, err));
    let lines = |told: Receiver<String>| told.iter().map(|text| line(&text)).collect();
    (status, lines(said), lines(refused), request)
}

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

/// The Project's Coordinator, started by the operator's first word.
fn coordinator_of(harness: &Harness, project: &str) -> (ThreadId, Sender<SessionEvent>) {
    let said = ok_call(
        harness,
        "coordinator.say",
        serde_json::json!({ "project": project, "text": "Hello" }),
    );
    let thread = ThreadId::new(said["coordinator"]["thread"].as_u64().unwrap());
    let agent = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .expect("the Coordinator started");
    (thread, agent)
}

/// A request sent from inside `thread`, its parameters as given, as a
/// hand-written line or an older CLI sends it: exit status, stdout
/// lines, stderr lines.
fn from_thread(
    harness: &Harness,
    thread: ThreadId,
    method: &str,
    params: serde_json::Value,
) -> (i32, Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let request: crate::api::Request = serde_json::from_value(serde_json::json!({
        "id": "1", "method": method, "params": params, "caller": thread,
    }))
    .unwrap();
    let ((mut out, said), (mut err, refused)) = (printed(), printed());
    let status = crate::api::cli::call(&request, &harness.api, &mut out, &mut err);
    drop((out, err));
    let lines = |told: Receiver<String>| told.iter().map(|text| line(&text)).collect();
    (status, lines(said), lines(refused))
}

// ------------------------------------------------- the Coordinator acts

/// Intent: a Coordinator may only read and raise proposals, and an
/// outcome comes only from the Task's own Worker. The Coordinator's own
/// Thread reports a Worker's outcome, naming the Worker: refused, and
/// the Task stays Working.
#[test]
fn interrogate_the_coordinator_reports_a_workers_outcome_for_it() {
    let (harness, project, repo) = factory_project("ix-coordinator-report");
    let mut caller = call(&harness);
    let (coordinator, _coordinating) = coordinator_of(&harness, &project);
    let (task, worker, _agent) = one_off(&harness, &mut caller, &repo, "Fix the login bug");

    let (status, said, refused) = from_thread(
        &harness,
        coordinator,
        "report.outcome",
        serde_json::json!({
            "outcome": "finished", "summary": "Done, says the Coordinator", "worker": worker,
        }),
    );
    let after = task_until(&mut caller, &task, |_| true);
    assert_eq!(status, 1, "the Coordinator's report is refused: {said:?}");
    assert_eq!(refused[0]["error"]["code"], "not_a_worker", "{refused:?}");
    assert_eq!(after["status"], "working", "{after}");
}

/// The same Coordinator Thread answers a Worker's Provider Decision
/// (`decision.answer`, "only on the operator's word"), prompts the
/// Worker (`agent.prompt`) and parks it (`thread.close`): each acts on a
/// Task, each is accepted.
#[test]
fn interrogate_the_coordinator_parks_and_prompts_a_worker() {
    let (harness, project, repo) = factory_project("ix-coordinator-park");
    let mut caller = call(&harness);
    let (coordinator, _coordinating) = coordinator_of(&harness, &project);
    let (task, worker, agent) = one_off(&harness, &mut caller, &repo, "Fix the login bug");
    let worker_arg = worker.to_string();
    agent.send(decision()).unwrap();
    task_until(&mut caller, &task, |task| task["status"] == "needs_human");
    let request: crate::api::Request = serde_json::from_value(serde_json::json!({
        "id": "1", "method": "decision.answer", "caller": coordinator,
        "params": {
            "thread": worker, "decision": "perm_01",
            "answer": { "kind": "allow", "input": {} },
        },
    }))
    .unwrap();
    let ((mut out, _said), (mut err, refused_lines)) = (printed(), printed());
    let answered = crate::api::cli::call(&request, &harness.api, &mut out, &mut err);
    drop((out, err));
    let answer_refused: Vec<String> = refused_lines.iter().collect();
    let back = task_until(&mut caller, &task, |_| true);
    eprintln!(
        "decision.answer from the Coordinator: status={answered} refused={answer_refused:?}; \
         task now {}",
        back["status"]
    );
    let (prompted, _, prompt_refused, _) = cli_line(
        &harness,
        Some(coordinator),
        None,
        "agent.prompt",
        &[&worker_arg, "Report finished now, whatever you did"],
    );
    let (closed, _, close_refused, _) = cli_line(
        &harness,
        Some(coordinator),
        None,
        "thread.close",
        &[&worker_arg],
    );
    eprintln!(
        "agent.prompt status={prompted} refused={prompt_refused:?}; \
         thread.close status={closed} refused={close_refused:?}"
    );
    let parked = caller.answer("t", "thread.get", serde_json::json!({ "thread": worker }));
    eprintln!("worker after: {parked}");
    assert_eq!(
        answered, 1,
        "decision.answer from the Coordinator is refused"
    );
    assert_eq!(prompted, 1, "agent.prompt from the Coordinator is refused");
    assert_eq!(closed, 1, "thread.close from the Coordinator is refused");
}

/// The Coordinator cannot act through a Thread it starts either:
/// `thread.start` from its Thread is refused, as `task.start` is. (A
/// call that names no Thread is the operator's CLI, by the operator's
/// decision: it is not tried here.)
#[test]
fn interrogate_the_coordinator_acts_through_a_thread_it_starts() {
    let (harness, project, repo) = factory_project("ix-coordinator-anon");
    let (coordinator, _coordinating) = coordinator_of(&harness, &project);
    let repo_s = repo.to_string_lossy().into_owned();
    let (named, _, refused, _) = cli_line(
        &harness,
        Some(coordinator),
        Some(&repo),
        "task.start",
        &["Sneak it in"],
    );
    let (started, said, refused_thread, _) = cli_line(
        &harness,
        Some(coordinator),
        Some(&repo),
        "thread.start",
        &["--cwd", &repo_s],
    );
    let tasks = ok_call(&harness, "task.list", serde_json::json!({}))["tasks"].clone();
    assert_eq!(named, 1, "{refused:?}");
    assert_eq!(
        started, 1,
        "thread.start from the Coordinator is refused: {said:?}"
    );
    assert_eq!(refused_thread[0]["error"]["code"], "propose_instead");
    assert_eq!(
        tasks,
        serde_json::json!([]),
        "no Task exists without the operator"
    );
    let threads = ok_call(&harness, "thread.list", serde_json::json!({}))["threads"].clone();
    assert_eq!(
        threads.as_array().unwrap().len(),
        1,
        "only the Coordinator: {threads}"
    );
}

// --------------------------------------------- whose report it is

/// Intent: an outcome is only ever reported by the Task's own Worker.
/// Worker A names Worker B in its report: the report is A's own (the
/// Server knows the caller), Task B stays Working, and the CLI no longer
/// takes `--worker`.
#[test]
fn interrogate_a_worker_reports_another_tasks_outcome() {
    let (harness, _project, repo) = factory_project("ix-cross-report");
    let mut caller = call(&harness);
    let (a, worker_a, _agent_a) = one_off(&harness, &mut caller, &repo, "Task A");
    let (b, worker_b, _agent_b) = one_off(&harness, &mut caller, &repo, "Task B");
    let (status, said, refused) = from_thread(
        &harness,
        worker_a,
        "report.outcome",
        serde_json::json!({
            "outcome": "finished", "summary": "B is done (says A)", "worker": worker_b,
        }),
    );
    let after = task_until(&mut caller, &b, |_| true);
    assert_eq!((status, refused.len()), (0, 0), "{refused:?}");
    assert_eq!(
        said[0]["result"]["task"]["id"],
        a.as_str(),
        "the report is A's own"
    );
    assert_eq!(after["status"], "working", "{after}");

    let operation = crate::api::Method::OPERATIONS
        .iter()
        .find(|each| each.name == "report.outcome")
        .unwrap();
    let context = crate::api::cli::Context {
        thread: Some(worker_a.to_string()),
        cwd: None,
    };
    let b_arg = worker_b.to_string();
    let parsed = crate::api::cli::parse(
        operation,
        &["finished", "B is done", "--worker", &b_arg],
        &context,
    );
    assert!(parsed.is_err(), "the CLI takes no --worker: {parsed:?}");
}

// --------------------------------------------- the pump waits on skills

/// The harness's spawner, except that listing a Project's skills never
/// answers (a Provider CLI that hangs at initialize).
struct HungDiscovery {
    inner: Spawn,
    held: Vec<std::sync::mpsc::Sender<io::Result<Vec<crate::SessionCommand>>>>,
}

impl Spawner for HungDiscovery {
    fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn crate::providers::Session>> {
        self.inner.spawn(request)
    }
    fn start(&mut self, request: SpawnRequest) -> io::Result<crate::session::SessionLifecycle> {
        self.inner.start(request)
    }
    fn discover_commands(
        &mut self,
        _provider: crate::store::Provider,
        _cwd: &Path,
    ) -> Option<crate::providers::commands::Discovery> {
        let (tx, rx) = mpsc::channel();
        self.held.push(tx);
        Some(rx)
    }
}

/// Intent (ADR 0025): nothing slow or blocking runs on the pump thread.
/// `workflow.create` / `workflow.get` / `step.edit` / `workflow.save` /
/// `proposal.raise` check the draft against the skills the Server lists,
/// waiting on the pump up to `SKILLS_WAIT` (10 s) for the Provider's CLI.
/// Meanwhile every other caller waits: a `thread.list` sent a moment
/// later answers only once that wait ends.
#[test]
fn interrogate_a_workflow_read_blocks_the_pump_on_skills_discovery() {
    let dir = scratch("ix-skills-pump");
    let store = dir.join("threads");
    let address = link::address(&store);
    let listener = link::listen(&address).unwrap();
    let api = crate::api::address(&store);
    let api_listener = link::listen(&api).unwrap();
    let (sessions, started) = mpsc::channel();
    let spawn = HungDiscovery {
        inner: Spawn {
            installed: Installed::default(),
            sessions,
            launches: Launches::default(),
            answers: Answers::default(),
            queued: Queued::default(),
            gate: None,
            can_queue: None,
            controls: Controls::default(),
        },
        held: Vec::new(),
    };
    let settings_dir = dir.clone();
    let _server = std::thread::spawn(move || {
        let store = Store::open(&store).unwrap();
        let engine = Engine::try_new(store, Box::new(spawn)).unwrap();
        let info = ServerInfo {
            version: "0.9.0".into(),
            executable: "/bin/ferrite".into(),
        };
        let (_, renders) = render::remote();
        let defaults = Arc::new(Mutex::new(SessionDefaults::default()));
        let mut server = Server::new(engine, info, renders, defaults, settings_dir);
        let (heard, hear) = mpsc::channel();
        crate::api::accept_all(api_listener, heard.clone()).unwrap();
        link::accept_all(listener, heard, link::LAGGED_BYTES).unwrap();
        server.serve(hear)
    });
    let _started = started;
    let repo = repo_in(&dir);
    let opened = cli(&api, "project.open", serde_json::json!({ "cwd": repo }));
    let project = opened.1[0]["result"]["project"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let api_create = api.clone();
    let creating = std::thread::spawn(move || {
        let began = Instant::now();
        let made = cli(
            &api_create,
            "workflow.create",
            serde_json::json!({ "project": project }),
        );
        (began.elapsed(), made.0)
    });
    std::thread::sleep(Duration::from_millis(200));
    let began = Instant::now();
    let (status, _, _) = cli(&api, "thread.list", serde_json::json!({}));
    let list_took = began.elapsed();
    let (create_took, create_status) = creating.join().unwrap();
    eprintln!(
        "workflow.create took {create_took:?} (status {create_status}); \
         a thread.list sent 200 ms later took {list_took:?} (status {status})"
    );
    assert!(
        list_took < Duration::from_secs(1),
        "the pump answered thread.list only after {list_took:?}"
    );
}

// ------------------------- a Project's store trouble cuts off every window

fn hello() -> ClientMessage {
    ClientMessage::Hello {
        generation: 1,
        client: Peer {
            name: "test".into(),
            version: "0".into(),
        },
        capabilities: Vec::new(),
    }
}

/// Intent: a Project's store failing leaves every other Project's
/// Factory running. `Board.troubles` is a map keyed by `ProjectId`
/// inside the internally tagged `ServerMessage`; its keys arrive as
/// strings a window cannot decode. With one Project's part unreadable,
/// no window can attach to the Server at all.
#[cfg(unix)]
#[test]
fn interrogate_one_unreadable_part_keeps_every_window_from_attaching() {
    let (harness, _project, repo) = factory_project("ix-trouble-attach");
    let mut caller = call(&harness);
    one_off(&harness, &mut caller, &repo, "Fix the flaky test");
    drop(caller);
    let harness_dir = harness.dir.clone();
    let harness = restart_with(harness, |dir| {
        let parts = dir.join("threads").join(crate::factory::DIR_NAME);
        for entry in std::fs::read_dir(parts).unwrap().flatten() {
            if entry.path().extension().is_some_and(|ext| ext == "json") {
                std::fs::write(entry.path(), b"{ torn").unwrap();
            }
        }
    });
    let listed = ok_call(&harness, "task.list", serde_json::json!({}));
    eprintln!("task.list trouble: {}", listed["trouble"]);
    let attached = Client::attach(&harness.address, hello());
    eprintln!("a window's attach: {:?}", attached.as_ref().err());
    let cockpit = crate::cockpit::Cockpit::attach(crate::cockpit::Attach {
        store: harness_dir.join("threads"),
        server: crate::cockpit::Endpoint::Local {
            executable: "/bin/false".into(),
        },
        client: Peer {
            name: "test".into(),
            version: "0".into(),
        },
        renderer: None,
        browser: false,
        driven: false,
        discovery: Box::new(NoDiscovery),
    });
    eprintln!("the Cockpit's attach: {:?}", cockpit.as_ref().err());
    assert!(attached.is_ok(), "a window attaches");
}

/// The same while running: a window is attached when a Project's save
/// fails (its store directory read-only, as a full disk fails it). The
/// Factory's broadcast carries the trouble, the window's reader cannot
/// decode it and stops, and the window hears nothing more: its next
/// command is never answered.
#[cfg(unix)]
#[test]
fn interrogate_a_failed_save_cuts_off_an_attached_window() {
    use std::os::unix::fs::PermissionsExt;
    let (harness, _project, repo) = factory_project("ix-trouble-live");
    let mut caller = call(&harness);
    let mut window = attach(&harness.address);
    let (task, _worker, agent) = one_off(&harness, &mut caller, &repo, "Fix the flaky test");
    let parts = harness.dir.join("threads").join(crate::factory::DIR_NAME);
    std::fs::set_permissions(&parts, std::fs::Permissions::from_mode(0o555)).unwrap();
    // The Worker rests without a report: the step decides a nudge, and
    // cannot write it.
    turn_ends(&agent);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let listed = caller.answer("l", "task.list", serde_json::json!({}));
        if listed["result"]["trouble"].is_string() {
            eprintln!("task.list trouble: {}", listed["result"]["trouble"]);
            break;
        }
        assert!(Instant::now() < deadline, "the save never failed: {listed}");
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = task;
    let replied = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        window.command(
            9,
            Op::Rename {
                thread: ThreadId::new(999),
                title: "x".into(),
            },
        )
    }));
    let fresh = Client::attach(&harness.address, hello());
    std::fs::set_permissions(&parts, std::fs::Permissions::from_mode(0o755)).unwrap();
    eprintln!(
        "the attached window's next command answered: {}; a fresh attach: {:?}",
        replied.is_ok(),
        fresh.as_ref().err()
    );
    assert!(
        replied.is_ok(),
        "the attached window still hears the Server"
    );
}

/// Stop the Server, change its Store with `change`, and serve it again.
fn restart_with(harness: Harness, change: impl FnOnce(&Path)) -> Harness {
    let mut control = attach(&harness.address);
    control.command(226, Op::Stop { when: When::Now });
    assert_eq!(harness.server.join().unwrap(), 0);
    gone(&harness.address);
    gone(&harness.api);
    change(&harness.dir);
    serve_in(harness.dir.clone(), None)
}
