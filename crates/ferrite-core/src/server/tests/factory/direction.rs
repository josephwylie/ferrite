//! Where a Task needs the operator through the Server (#222): a Worker's
//! question and its answer, a Provider Decision answered from the Task,
//! agent and human approval, rework, and one Task's redirect, across a
//! restart where that is the point.

use super::*;

/// The Server's CLI, refused nothing: its answer's `result`.
fn ok(harness: &Harness, method: &str, params: serde_json::Value) -> serde_json::Value {
    let (status, said, refused) = cli(&harness.api, method, params);
    assert_eq!((status, refused.len()), (0, 0), "{method}: {refused:?}");
    said[0]["result"].clone()
}

/// A Workflow of Steps named and instructed as given, the last routing to
/// Ready to approve, saved: its id and its Step ids.
fn workflow(harness: &Harness, project: &str, steps: &[(&str, &str)]) -> (String, Vec<String>) {
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
    for (at, ((name, instructions), id)) in steps.iter().zip(&ids).enumerate() {
        let route = if at + 1 == steps.len() {
            "approval"
        } else {
            "next"
        };
        ok(
            harness,
            "step.edit",
            serde_json::json!({
                "workflow": workflow, "step": id, "name": name,
                "instructions": instructions, "on_finished": route,
            }),
        );
    }
    ok(
        harness,
        "workflow.approval",
        serde_json::json!({ "workflow": workflow, "rework_to": format!("step:{}", ids[0]) }),
    );
    let saved = ok(
        harness,
        "workflow.save",
        serde_json::json!({ "workflow": workflow }),
    );
    assert_eq!(saved["version"], 1, "{saved}");
    (workflow, ids)
}

fn run(harness: &Harness, repo: &Path, workflow: &str, instructions: &str) -> String {
    let said = ok(
        harness,
        "task.start",
        serde_json::json!({ "instructions": instructions, "cwd": repo, "workflow": workflow }),
    );
    said["task"]["id"].as_str().unwrap().to_string()
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

/// The Task's Worker once it is another than `was`.
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

/// AC (case 5): a Worker's question moves its Task to Needs human and
/// starts nothing after it; it waits across a restart; the answer from
/// the CLI resumes the asker with the question and the answer, and a
/// second answer starts nothing; the next Step is told both, and the
/// result waits in Ready to approve until approved.
#[cfg(unix)]
#[test]
fn a_question_waits_across_a_restart_and_the_answer_resumes_the_asker() {
    let (harness, project, repo) = factory_project("factory-question");
    let (workflow, _) = workflow(
        &harness,
        &project,
        &[
            ("Builder", "Build the flag"),
            ("Reviewer", "Review the flag"),
        ],
    );
    let mut caller = call(&harness);
    let task = run(&harness, &repo, &workflow, "Add a --fast flag");
    let _agent = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let builder = worker_of(&mut caller, &task);
    let asked = ok_in(
        &harness,
        builder,
        "report.question",
        serde_json::json!({ "text": "Should --fast default to on?" }),
    );
    assert_eq!(asked["changed"], true, "{asked}");
    let held = task_until(&mut caller, &task, |task| task["status"] == "needs_human");
    assert_eq!(
        held["question"]["text"], "Should --fast default to on?",
        "{held}"
    );
    assert!(
        harness.sessions.try_recv().is_err(),
        "nothing after it starts"
    );

    let mut control = attach(&harness.address);
    control.command(222, Op::Stop { when: When::Now });
    assert_eq!(harness.server.join().unwrap(), 0);
    gone(&harness.address);
    gone(&harness.api);
    let harness = serve_in(harness.dir.clone(), None);
    let mut caller = call(&harness);
    let held = task_until(&mut caller, &task, |_| true);
    assert_eq!(held["status"], "needs_human", "{held}");
    assert_eq!(held["question"]["text"], "Should --fast default to on?");

    let answered = ok(
        &harness,
        "task.answer",
        serde_json::json!({ "task": task, "text": "Off; opt in only" }),
    );
    assert_eq!(answered["changed"], true, "{answered}");
    let read = told(&mut caller, builder, "Off; opt in only");
    assert!(read.contains("Should --fast default to on?"), "{read}");
    let again = ok(
        &harness,
        "task.answer",
        serde_json::json!({ "task": task, "text": "Off; opt in only" }),
    );
    assert_eq!(again["changed"], false, "a duplicate answer starts nothing");

    report(
        &harness,
        builder,
        "finished",
        "Added --fast, off by default",
    );
    let reviewer = next_worker(&mut caller, &task, builder);
    let read = told(&mut caller, reviewer, "Review the flag");
    assert!(read.contains("Off; opt in only"), "{read}");
    report(&harness, reviewer, "finished", "The flag works");
    let ready = task_until(&mut caller, &task, |task| task["status"] == "approval");
    assert_eq!(ready["result"], "The flag works", "{ready}");
    let approved = ok(
        &harness,
        "task.approve",
        serde_json::json!({ "task": task }),
    );
    assert_eq!(approved["task"]["status"], "completed", "{approved}");
}

/// AC (case 15): the windows are alerted when a Task needs the operator,
/// is ready to approve, stops or completes, and never as it moves from
/// one Step to the next.
#[test]
fn the_windows_are_alerted_on_the_four_task_events_and_not_between_steps() {
    use crate::factory::Status;
    let (harness, project, repo) = factory_project("factory-alerts");
    let mut window = cockpit(&harness);
    let (workflow, _) = workflow(
        &harness,
        &project,
        &[("Builder", "Build it"), ("Reviewer", "Review it")],
    );
    let mut caller = call(&harness);
    let shipped = run(&harness, &repo, &workflow, "Add a --fast flag");
    let builder = worker_of(&mut caller, &shipped);
    report(&harness, builder, "finished", "Built");
    let reviewer = next_worker(&mut caller, &shipped, builder);
    report(&harness, reviewer, "finished", "Reviewed");
    task_until(&mut caller, &shipped, |task| task["status"] == "approval");
    ok(
        &harness,
        "task.approve",
        serde_json::json!({ "task": shipped }),
    );

    let asked = ok(
        &harness,
        "task.start",
        serde_json::json!({ "instructions": "Tidy the README", "cwd": repo }),
    )["task"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let worker = worker_of(&mut caller, &asked);
    ok_in(
        &harness,
        worker,
        "report.question",
        serde_json::json!({ "text": "Which README?" }),
    );
    task_until(&mut caller, &asked, |task| task["status"] == "needs_human");
    ok(&harness, "task.stop", serde_json::json!({ "task": asked }));

    let status = |window: &crate::cockpit::Cockpit, task: &str| {
        window.factory().task(task).map(|task| task.status)
    };
    pump_until(&mut window, |window| {
        status(window, &shipped) == Some(Status::Completed)
            && status(window, &asked) == Some(Status::Stopped)
    });
    let of = |alerts: &[crate::cockpit::Asked], task: &str| -> Vec<Status> {
        alerts
            .iter()
            .filter_map(|alert| match alert {
                crate::cockpit::Asked::TaskAlert {
                    task: alerted,
                    status,
                    ..
                } if alerted == task => Some(*status),
                _ => None,
            })
            .collect()
    };
    // The alerts arrive in order, so the last one brings all before it.
    let alerts = pump_until_asked(&mut window, |alerts| {
        of(alerts, &asked).last() == Some(&Status::Stopped)
    });
    assert_eq!(of(&alerts, &shipped), [Status::Approval, Status::Completed]);
    assert_eq!(of(&alerts, &asked), [Status::NeedsHuman, Status::Stopped]);
}

/// AC (cases 8, 3): agent approval without a deciding Step or
/// instructions is refused; with them, the deciding Step is told the
/// instructions in its run and decides in it; its rejection carries its
/// reasons to the rework recipient; its approval completes the Task.
#[test]
fn an_agent_decides_approval_with_its_instructions_and_a_rejection_reworks() {
    let (harness, project, repo) = factory_project("factory-agent-approval");
    let (workflow, steps) = workflow(
        &harness,
        &project,
        &[
            ("Builder", "Build the flag"),
            ("Reviewer", "Review the flag"),
        ],
    );
    let mut caller = call(&harness);
    ok(
        &harness,
        "workflow.approval",
        serde_json::json!({ "workflow": workflow, "by": "agent" }),
    );
    let (_, _, refused) = cli(
        &harness.api,
        "workflow.save",
        serde_json::json!({ "workflow": workflow }),
    );
    let message = refused[0]["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("choose the agent that decides"),
        "{refused:?}"
    );
    assert!(
        message.contains("write the decision instructions"),
        "{refused:?}"
    );
    ok(
        &harness,
        "workflow.approval",
        serde_json::json!({
            "workflow": workflow, "step": steps[1],
            "instructions": "Approve only when a test covers the flag",
        }),
    );
    let saved = ok(
        &harness,
        "workflow.save",
        serde_json::json!({ "workflow": workflow }),
    );
    assert_eq!(saved["version"], 2, "{saved}");

    let task = run(&harness, &repo, &workflow, "Add a --fast flag");
    let building = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let builder = worker_of(&mut caller, &task);
    report(&harness, builder, "finished", "Added --fast");
    // Its turn over as the Server sees it, so the rework back to it is a
    // turn of its own: a prompt to an agent still mid-turn waits in the
    // provider's queue, which this one never runs.
    turn_ends(&building);
    caller.answer("w", "agent.wait", serde_json::json!({ "thread": builder }));
    let reviewing = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let reviewer = next_worker(&mut caller, &task, builder);
    let read = told(
        &mut caller,
        reviewer,
        "Approve only when a test covers the flag",
    );
    assert!(read.contains("outcome `approved`"), "{read}");

    report(&harness, reviewer, "rejected", "No test covers --fast");
    turn_ends(&reviewing);
    caller.answer("w", "agent.wait", serde_json::json!({ "thread": reviewer }));
    let back = task_until(&mut caller, &task, |task| {
        task["worker"] == serde_json::json!(builder)
    });
    assert_eq!(back["status"], "working", "{back}");
    told(&mut caller, builder, "No test covers --fast");

    report(&harness, builder, "finished", "Added a test for --fast");
    told(&mut caller, reviewer, "Added a test for --fast");
    let decided = report(&harness, reviewer, "approved", "The test covers it");
    assert_eq!(decided["task"]["status"], "completed", "{decided}");
}

/// AC (cases 6, 7): rework from Ready to approve needs feedback and
/// reaches the chosen recipient; a redirect sends one Task to a Step,
/// skipping another, returning it to the operator; the run it superseded
/// is parked and its late report, after a restart, cannot revive the old
/// route; the saved Workflow is unchanged.
#[cfg(unix)]
#[test]
fn rework_and_a_redirect_reach_their_steps_and_survive_a_restart() {
    let (harness, project, repo) = factory_project("factory-redirect");
    let (workflow, steps) = workflow(
        &harness,
        &project,
        &[
            ("Builder", "Build the flag"),
            ("Reviewer", "Review the flag"),
            ("Checker", "Run the checks"),
        ],
    );
    let saved = ok(
        &harness,
        "workflow.get",
        serde_json::json!({ "workflow": workflow }),
    );
    let mut caller = call(&harness);
    let task = run(&harness, &repo, &workflow, "Add a --fast flag");
    let builder = worker_of(&mut caller, &task);

    let directed = ok(
        &harness,
        "task.redirect",
        serde_json::json!({
            "task": task, "to": steps[2], "skip": [steps[1]], "then": "me",
            "note": "Only the docs changed",
        }),
    );
    assert_eq!(directed["changed"], true, "{directed}");
    let checker = next_worker(&mut caller, &task, builder);
    told(&mut caller, checker, "Only the docs changed");
    let parked = caller.answer("p", "thread.get", serde_json::json!({ "thread": builder }));
    assert_eq!(
        parked["error"]["code"], "unknown_thread",
        "superseded: {parked}"
    );

    let mut control = attach(&harness.address);
    control.command(222, Op::Stop { when: When::Now });
    assert_eq!(harness.server.join().unwrap(), 0);
    gone(&harness.address);
    gone(&harness.api);
    let harness = serve_in(harness.dir.clone(), None);
    let mut caller = call(&harness);

    let late = report(&harness, builder, "finished", "Late build");
    assert_eq!(late["changed"], false, "{late}");
    let held = task_until(&mut caller, &task, |_| true);
    assert_eq!(held["step"], serde_json::json!(steps[2]), "{held}");
    assert_eq!(held["override"]["skip"], serde_json::json!([steps[1]]));

    report(&harness, checker, "finished", "Docs checks pass");
    let ready = task_until(&mut caller, &task, |task| task["status"] == "approval");
    assert_eq!(ready["result"], "Docs checks pass", "{ready}");

    let (_, _, refused) = cli(
        &harness.api,
        "task.rework",
        serde_json::json!({ "task": task, "feedback": " " }),
    );
    assert_eq!(
        refused[0]["error"]["code"], "invalid_request",
        "{refused:?}"
    );
    ok(
        &harness,
        "task.rework",
        serde_json::json!({
            "task": task, "feedback": "Mention --fast in the README",
            "to": format!("step:{}", steps[1]),
        }),
    );
    let reviewer = next_worker(&mut caller, &task, checker);
    let read = told(&mut caller, reviewer, "Mention --fast in the README");
    assert!(
        read.contains("Docs checks pass"),
        "earlier findings: {read}"
    );

    let now = ok(
        &harness,
        "workflow.get",
        serde_json::json!({ "workflow": workflow }),
    );
    assert_eq!(
        now["workflow"], saved["workflow"],
        "the saved Workflow is unchanged"
    );
}

/// AC (case 12): a Provider Decision in a Worker's Session shows on the
/// board as Needs human (the window told) and never in the Inbox; once
/// answered the Task works again.
#[test]
fn a_workers_provider_decision_needs_the_operator_in_the_task_not_the_inbox() {
    let (harness, project, repo) = factory_project("factory-permission");
    let (workflow, _) = workflow(&harness, &project, &[("Builder", "Build the flag")]);
    let mut window = cockpit(&harness);
    let mut caller = call(&harness);
    let task = run(&harness, &repo, &workflow, "Add a --fast flag");
    let agent = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let builder = worker_of(&mut caller, &task);
    agent.send(decision()).unwrap();
    let held = task_until(&mut caller, &task, |task| task["status"] == "needs_human");
    assert_eq!(held["permission"], "ferrite-perm.txt", "{held}");
    let project_id: crate::workspace::registry::ProjectId =
        serde_json::from_value(serde_json::json!(project.parse::<u64>().unwrap())).unwrap();
    pump_until(&mut window, |window| {
        window.factory().needs_you(project_id) == 1 && window.blocked().len() == 1
    });
    assert!(window.needs_you().is_empty(), "never in the Inbox");
    pump_until_asked(&mut window, |asked| {
        asked.iter().any(|asked| {
            matches!(
                asked,
                crate::cockpit::Asked::TaskAlert {
                    status: crate::factory::Status::NeedsHuman,
                    ..
                }
            )
        })
    });

    let answered = caller.answer(
        "a",
        "decision.answer",
        serde_json::json!({
            "thread": builder, "decision": "perm_01",
            "answer": { "kind": "allow", "input": {} },
        }),
    );
    assert!(answered.get("result").is_some(), "{answered}");
    let back = task_until(&mut caller, &task, |task| task["status"] == "working");
    assert!(back.get("permission").is_none(), "{back}");
}
