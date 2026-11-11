//! The Coordinator through the Server (#224): a scripted Coordinator
//! Session started on the operator's first word, owned by the Factory in
//! the main checkout, on the settings the Factory holds; its four
//! proposals change nothing until a window or the CLI accepts them, and
//! nothing when dismissed; a Task routed to it is told to it.

use super::*;

/// The Server's CLI, refused nothing: its answer's `result`.
fn ok(harness: &Harness, method: &str, params: serde_json::Value) -> serde_json::Value {
    let (status, said, refused) = cli(&harness.api, method, params);
    assert_eq!((status, refused.len()), (0, 0), "{method}: {refused:?}");
    said[0]["result"].clone()
}

fn project_id(project: &str) -> crate::workspace::registry::ProjectId {
    serde_json::from_value(serde_json::json!(project.parse::<u64>().unwrap())).unwrap()
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

/// The Coordinator started by the CLI's first word: its Thread and the
/// scripted Session playing it.
fn coordinator(harness: &Harness, project: &str) -> (ThreadId, Sender<SessionEvent>) {
    let said = ok(
        harness,
        "coordinator.say",
        serde_json::json!({ "project": project, "text": "Hello" }),
    );
    let thread = said["coordinator"]["thread"]
        .as_u64()
        .unwrap_or_else(|| panic!("{said}"));
    let agent = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .expect("the Coordinator started");
    (ThreadId::new(thread), agent)
}

/// A Workflow "Ship it" of Steps named as given, the last completing,
/// saved: its id and Step ids.
fn workflow(harness: &Harness, project: &str, steps: &[&str]) -> (String, Vec<String>) {
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
    for (at, (name, id)) in steps.iter().zip(&ids).enumerate() {
        let route = if at + 1 == steps.len() {
            "complete"
        } else {
            "next"
        };
        ok(
            harness,
            "step.edit",
            serde_json::json!({
                "workflow": workflow, "step": id, "name": name,
                "instructions": format!("Do {name}"), "on_finished": route,
            }),
        );
    }
    let saved = ok(
        harness,
        "workflow.save",
        serde_json::json!({ "workflow": workflow }),
    );
    assert_eq!(saved["version"], 1, "{saved}");
    (workflow, ids)
}

fn raise(harness: &Harness, coordinator: ThreadId, mut params: serde_json::Value) -> String {
    params["coordinator"] = serde_json::json!(coordinator);
    let raised = ok(harness, "proposal.raise", params);
    assert_eq!(raised["proposal"]["status"], "pending", "{raised}");
    raised["proposal"]["id"].as_str().unwrap().to_string()
}

fn window_says(window: &mut Window, id: u64, action: protocol::CoordinatorAction) -> Done {
    match window.command(id, Op::Coordinator { action }) {
        ServerMessage::Reply { ok: Some(done), .. } => done,
        reply => panic!("{reply:?}"),
    }
}

/// AC: nothing runs until the operator speaks; their first word starts
/// one Coordinator, owned by the Factory, in the main checkout, on the
/// Provider, model and effort set in the Factory, briefed; later words
/// reach the same Thread; it is not the operator's own (no Inbox); a
/// change of settings reaches its Session for its next turn.
#[test]
fn the_coordinator_starts_on_the_first_word_in_the_main_checkout_on_its_settings() {
    let (harness, project, repo) = factory_project("coordinator-start");
    let mut window = attach(&harness.address);
    let mut caller = call(&harness);
    ok(
        &harness,
        "coordinator.set",
        serde_json::json!({
            "project": project, "provider": "codex", "model": "gpt-coordinator", "effort": "high",
        }),
    );
    assert!(harness.sessions.try_recv().is_err(), "nothing runs yet");

    let done = window_says(
        &mut window,
        1,
        protocol::CoordinatorAction::Say {
            project: project_id(&project),
            text: "What is left on the board?".into(),
        },
    );
    assert_eq!(done, Done::Changed { changed: true });
    let agent = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .expect("the Coordinator started");
    let listed = ok(
        &harness,
        "proposal.list",
        serde_json::json!({ "project": project }),
    );
    assert_eq!(listed["proposals"], serde_json::json!([]));
    let board = caller.answer("t", "task.list", serde_json::json!({}));
    assert_eq!(
        board["result"]["tasks"],
        serde_json::json!([]),
        "talking starts nothing"
    );
    let threads = caller.answer("l", "thread.list", serde_json::json!({}));
    let coordinator = threads["result"]["threads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|info| info["owner"] == serde_json::json!({ "factory": project }))
        .and_then(|info| info["thread"].as_u64())
        .map(ThreadId::new)
        .unwrap_or_else(|| panic!("{threads}"));

    let info = caller.answer(
        "i",
        "thread.get",
        serde_json::json!({ "thread": coordinator }),
    );
    let info = &info["result"]["thread"];
    assert_eq!(
        info["owner"],
        serde_json::json!({ "factory": project }),
        "{info}"
    );
    assert!(info["worktree"].is_null(), "the main checkout: {info}");
    assert_eq!(
        harness.launches.lock().unwrap().last().cloned(),
        Some((
            Provider::Codex,
            Some("gpt-coordinator".to_string()),
            Some("high".to_string())
        ))
    );
    let first = told(&mut caller, coordinator, "What is left on the board?");
    assert!(first.contains("Coordinator of this Project"), "{first}");
    assert!(first.contains("proposal raise"), "{first}");
    turn_ends(&agent);
    ok(
        &harness,
        "coordinator.say",
        serde_json::json!({ "cwd": repo, "text": "And the docs?" }),
    );
    told(&mut caller, coordinator, "And the docs?");
    assert!(harness.sessions.try_recv().is_err(), "one Coordinator");

    // A change of Provider and model reaches its Session at its next turn.
    turn_ends(&agent);
    ok(
        &harness,
        "coordinator.set",
        serde_json::json!({ "project": project, "provider": "claude", "model": "claude-coordinator" }),
    );
    ok(
        &harness,
        "coordinator.say",
        serde_json::json!({ "project": project, "text": "Now?" }),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !harness.launches.lock().unwrap().iter().any(|launch| {
        launch.0 == Provider::Claude && launch.1.as_deref() == Some("claude-coordinator")
    }) {
        assert!(
            Instant::now() < deadline,
            "never tuned: {:?}",
            harness.launches
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    let mut cockpit_window = cockpit(&harness);
    pump_until(&mut cockpit_window, |window| {
        window
            .factory()
            .coordinator(project_id(&project))
            .is_some_and(|c| c.thread == Some(coordinator))
    });
    assert!(!cockpit_window.operators(coordinator));
    assert!(cockpit_window.needs_you().is_empty());
}

/// AC: each of the four acts raised by the scripted Coordinator is stored
/// and changes nothing; dismissed it changes nothing; accepted (from a
/// window or the CLI) it runs as the operator's, once. A one-off handoff
/// needs no Workflow; only a Coordinator proposes.
#[test]
fn each_proposal_waits_for_the_operator_and_runs_once_accepted() {
    let (harness, project, repo) = factory_project("coordinator-proposals");
    let mut window = attach(&harness.address);
    let mut caller = call(&harness);
    let (coordinator, _agent) = coordinator(&harness, &project);
    let (built, steps) = workflow(&harness, &project, &["Build", "Check"]);
    let (scripts, _) = workflow(&harness, &project, &["Run"]);
    ok(
        &harness,
        "workflow.start",
        serde_json::json!({ "workflow": scripts, "start": { "type": "external" } }),
    );
    ok(
        &harness,
        "workflow.save",
        serde_json::json!({ "workflow": scripts }),
    );
    let running = ok(
        &harness,
        "task.start",
        serde_json::json!({ "instructions": "Running", "cwd": repo, "workflow": built }),
    )["task"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _worker_agent = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let worker = worker_of(&mut caller, &running);
    ok(
        &harness,
        "step.edit",
        serde_json::json!({ "workflow": built, "step": steps[1], "instructions": "Check twice" }),
    );

    let refused = cli(
        &harness.api,
        "proposal.raise",
        serde_json::json!({ "act": "start", "instructions": "x", "coordinator": worker }),
    )
    .2;
    assert_eq!(
        refused[0]["error"]["code"], "not_a_coordinator",
        "{refused:?}"
    );

    let one_off = raise(
        &harness,
        coordinator,
        serde_json::json!({ "act": "start", "instructions": "Fix the toolbar overflow" }),
    );
    let save = raise(
        &harness,
        coordinator,
        serde_json::json!({ "act": "save", "workflow": built }),
    );
    let automation = raise(
        &harness,
        coordinator,
        serde_json::json!({ "act": "automation", "workflow": scripts }),
    );
    let redirect = raise(
        &harness,
        coordinator,
        serde_json::json!({ "act": "redirect", "task": running, "to": steps[1], "then": "me" }),
    );
    let listed = ok(
        &harness,
        "proposal.list",
        serde_json::json!({ "project": project }),
    );
    assert_eq!(listed["proposals"].as_array().unwrap().len(), 4, "{listed}");
    let will = listed["proposals"][3]["will"].to_string();
    assert!(will.contains("Send to: Check"), "{will}");

    // Nothing happened.
    let unchanged = |caller: &mut Caller| {
        let tasks = caller.answer("t", "task.list", serde_json::json!({}));
        assert_eq!(
            tasks["result"]["tasks"].as_array().unwrap().len(),
            1,
            "{tasks}"
        );
        assert_eq!(tasks["result"]["tasks"][0]["step"], steps[0]);
        let made = ok(
            &harness,
            "workflow.get",
            serde_json::json!({ "workflow": built }),
        );
        assert_eq!(made["workflow"]["versions"].as_array().unwrap().len(), 1);
        let made = ok(
            &harness,
            "workflow.get",
            serde_json::json!({ "workflow": scripts }),
        );
        assert!(made["workflow"].get("automation").is_none(), "{made}");
    };
    unchanged(&mut caller);
    assert!(harness.sessions.try_recv().is_err(), "nothing started");

    // Dismissed, each changes nothing.
    for (at, proposal) in [&one_off, &save, &automation, &redirect]
        .into_iter()
        .enumerate()
    {
        let done = window_says(
            &mut window,
            10 + at as u64,
            protocol::CoordinatorAction::Dismiss {
                proposal: proposal.clone(),
            },
        );
        assert_eq!(done, Done::Changed { changed: true });
    }
    unchanged(&mut caller);
    assert!(harness.sessions.try_recv().is_err(), "nothing started");

    // Raised again and accepted: each runs, once.
    let one_off = raise(
        &harness,
        coordinator,
        serde_json::json!({ "act": "start", "instructions": "Fix the toolbar overflow" }),
    );
    let accept = |window: &mut Window, id: u64, proposal: &str| {
        window_says(
            window,
            id,
            protocol::CoordinatorAction::Accept {
                proposal: proposal.into(),
                skills: Vec::new(),
            },
        )
    };
    let Done::Task { task } = accept(&mut window, 20, &one_off) else {
        panic!("no task");
    };
    harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .expect("its Worker started");
    let made = task_until(&mut caller, &task, |task| task["worker"].is_u64());
    assert!(made.get("workflow").is_none(), "one-off: {made}");
    assert_eq!(
        accept(&mut window, 21, &one_off),
        Done::Changed { changed: false }
    );
    assert!(harness.sessions.try_recv().is_err(), "accepted once");

    let save = raise(
        &harness,
        coordinator,
        serde_json::json!({ "act": "save", "workflow": built }),
    );
    let accepted = ok(
        &harness,
        "proposal.accept",
        serde_json::json!({ "proposal": save }),
    );
    assert_eq!(accepted["proposal"]["status"], "accepted", "{accepted}");
    let made = ok(
        &harness,
        "workflow.get",
        serde_json::json!({ "workflow": built }),
    );
    assert_eq!(made["workflow"]["versions"].as_array().unwrap().len(), 2);

    let automation = raise(
        &harness,
        coordinator,
        serde_json::json!({ "act": "automation", "workflow": scripts }),
    );
    ok(
        &harness,
        "proposal.accept",
        serde_json::json!({ "proposal": automation }),
    );
    let made = ok(
        &harness,
        "workflow.get",
        serde_json::json!({ "workflow": scripts }),
    );
    assert_eq!(made["workflow"]["automation"]["switch"], "on", "{made}");

    let redirect = raise(
        &harness,
        coordinator,
        serde_json::json!({ "act": "redirect", "task": running, "to": steps[1], "then": "me" }),
    );
    accept(&mut window, 22, &redirect);
    let moved = task_until(&mut caller, &running, |task| task["step"] == steps[1]);
    assert_eq!(moved["to_me"], true, "{moved}");
}

/// A Task routed to the Coordinator waits in Needs human, and the
/// Coordinator (started for it) is told the Task and what was reported.
#[test]
fn a_task_routed_to_the_coordinator_is_told_to_it() {
    let (harness, project, repo) = factory_project("coordinator-route");
    let mut caller = call(&harness);
    let (built, steps) = workflow(&harness, &project, &["Build"]);
    ok(
        &harness,
        "step.edit",
        serde_json::json!({ "workflow": built, "step": steps[0], "on_finished": "coordinator", "on_unable": "complete" }),
    );
    ok(
        &harness,
        "workflow.save",
        serde_json::json!({ "workflow": built }),
    );
    let task = ok(
        &harness,
        "task.start",
        serde_json::json!({ "instructions": "Build it", "cwd": repo, "workflow": built }),
    )["task"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _worker_agent = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let worker = worker_of(&mut caller, &task);
    ok(
        &harness,
        "report.outcome",
        serde_json::json!({ "outcome": "finished", "summary": "Built; the API is unclear", "worker": worker }),
    );
    let held = task_until(&mut caller, &task, |task| task["status"] == "needs_human");
    assert!(
        held["reason"].as_str().unwrap().contains("Coordinator"),
        "{held}"
    );
    harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .expect("the Coordinator started for it");
    let listed = ok(
        &harness,
        "coordinator.say",
        serde_json::json!({ "project": project, "text": "Hi" }),
    );
    let coordinator = ThreadId::new(listed["coordinator"]["thread"].as_u64().unwrap());
    let read = told(&mut caller, coordinator, &task);
    assert!(read.contains("Built; the API is unclear"), "{read}");
}

/// One CLI call run from inside `thread`, as its Session's `ferrite`
/// makes it (`FERRITE_THREAD_ID`).
fn cli_from(
    harness: &Harness,
    thread: ThreadId,
    method: &str,
    params: serde_json::Value,
) -> (i32, Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let list = crate::api::Method::OPERATIONS
        .iter()
        .find(|op| op.name == "task.list")
        .unwrap();
    let context = crate::api::cli::Context {
        thread: Some(thread.to_string()),
        cwd: None,
    };
    let parsed = crate::api::cli::parse(list, &[], &context).unwrap();
    assert_eq!(parsed.caller, Some(thread), "the CLI names where it runs");
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

/// G1, spec: the Coordinator only proposes. A call from its Thread that
/// would act (start a Task, save a version, turn starts on, change a
/// Task or its settings) is refused, pointing at `proposal raise`, and
/// nothing happens; it may still read and raise, and the same act raised
/// and accepted by the operator runs.
#[test]
fn the_coordinator_cannot_act_only_propose() {
    let (harness, project, repo) = factory_project("coordinator-cannot-act");
    let (coordinator, _agent) = coordinator(&harness, &project);
    let (built, _) = workflow(&harness, &project, &["Build"]);
    let acts = [
        (
            "task.start",
            serde_json::json!({ "instructions": "Sneak it in", "cwd": repo }),
        ),
        ("workflow.save", serde_json::json!({ "workflow": built })),
        (
            "workflow.automation",
            serde_json::json!({ "workflow": built, "turn": "on" }),
        ),
        ("task.stop", serde_json::json!({ "task": "t-1" })),
        (
            "coordinator.set",
            serde_json::json!({ "project": project, "provider": "codex" }),
        ),
        ("proposal.accept", serde_json::json!({ "proposal": "p-1" })),
    ];
    for (method, params) in acts {
        let (status, _, refused) = cli_from(&harness, coordinator, method, params);
        assert_eq!(status, 1, "{method}");
        assert_eq!(
            refused[0]["error"]["code"], "propose_instead",
            "{method}: {refused:?}"
        );
        let message = refused[0]["error"]["message"].as_str().unwrap();
        assert!(message.contains("proposal raise"), "{message}");
    }
    assert_eq!(
        ok(&harness, "task.list", serde_json::json!({}))["tasks"],
        serde_json::json!([]),
        "no Task exists"
    );
    let (status, _, _) = cli_from(&harness, coordinator, "task.list", serde_json::json!({}));
    assert_eq!(status, 0, "it reads");

    let proposal = raise(
        &harness,
        coordinator,
        serde_json::json!({ "act": "start", "instructions": "Sneak it in" }),
    );
    let accepted = ok(
        &harness,
        "proposal.accept",
        serde_json::json!({ "proposal": proposal }),
    );
    assert_eq!(accepted["proposal"]["status"], "accepted", "{accepted}");
    assert_eq!(
        ok(&harness, "task.list", serde_json::json!({}))["tasks"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
