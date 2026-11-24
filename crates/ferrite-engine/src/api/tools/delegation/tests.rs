use std::cell::RefCell;

use super::*;

/// What a call asked, and the answers it got, in order; the last answer
/// is given again to any later ask. Who this machine is (`machine.list`
/// without `reach`) is answered `srv-here` and not kept.
struct Server {
    asked: RefCell<Vec<Method>>,
    answers: RefCell<Vec<Result<Value, Refusal>>>,
}

impl Server {
    fn answering(answers: Vec<Result<Value, Refusal>>) -> Self {
        Self {
            asked: RefCell::default(),
            answers: RefCell::new(answers),
        }
    }

    fn call(&self, tool: &str, arguments: Value) -> (Value, bool) {
        let ask = |method| {
            if method == Method::MachineList(MachineListParams::default()) {
                return Ok(json!({ "server_id": "srv-here", "machines": [] }));
            }
            self.asked.borrow_mut().push(method);
            let mut answers = self.answers.borrow_mut();
            match answers.len() {
                1 => answers[0].clone(),
                _ => answers.remove(0),
            }
        };
        let answer = super::call(tool, &arguments, ThreadId::new(4), &ask);
        let text = serde_json::from_str(&answer.text).unwrap_or(Value::String(answer.text));
        (text, answer.is_error)
    }
}

fn refusal(code: &str, message: &str) -> Refusal {
    Refusal {
        code: code.into(),
        message: message.into(),
    }
}

fn failure(code: &str, message: &str) -> (Value, bool) {
    (json!({ "code": code, "message": message }), true)
}

fn info(status: &str, result: Option<&str>) -> Value {
    json!({
        "delegate": "d1", "parent": 4, "machine": "pc", "server": "srv-pc", "thread": 9,
        "title": "Login test", "provider": "claude", "model": "opus", "status": status,
        "result": result, "problem": null, "stop_owed": false,
    })
}

fn schema(name: &str) -> Value {
    definitions()
        .into_iter()
        .find(|tool| tool.name == name)
        .unwrap()
        .input_schema
}

/// T3's six names, words and required parameters, with a machine where T3
/// says an environment.
#[test]
fn the_tools_are_t3s() {
    let definitions = definitions();
    let names: Vec<&str> = definitions.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, NAMES);
    assert_eq!(schema("delegate_task")["required"], json!(["task", "target"]));
    assert_eq!(
        schema("delegate_task")["properties"]["target"]["required"],
        json!(["machineId"])
    );
    assert_eq!(schema("task_status")["required"], json!(["taskId"]));
    assert_eq!(schema("task_cancel")["required"], json!(["taskId"]));
    let described = |name: &str| {
        definitions
            .iter()
            .find(|tool| tool.name == name)
            .map(|tool| tool.description.clone())
            .unwrap()
    };
    assert_eq!(
        described("delegate_task"),
        "Start one child agent with only `task`. No parent history. Separate checkout or PR per child: `isolation:'worktree'`. Keep `taskId` for `task_status` and `task_cancel`. No secrets in `task`."
    );
    assert_eq!(
        described("orchestrator_capabilities"),
        "List providers, models, options, and runtime modes for `delegate_task`. Short list. `search` finds others. Any catalog model works."
    );
    assert!(described("task_status").contains("working, waiting_for_children, result_available"));
    assert_eq!(described("project_list"), "List projects. Pages can shift.");
    for definition in &definitions {
        let words = format!("{} {}", definition.description, definition.input_schema);
        assert!(!words.contains("nvironment") && !words.contains("T3"), "{words}");
    }
}

/// `delegate_task` is one `delegate.start` from the Session's Thread, in
/// T3's parameters: the role said first, the effort from either form of
/// `options`, the mode by T3's word; its answer is T3's remote task with
/// `next`.
#[test]
fn delegate_task_starts_a_delegate_and_says_not_to_poll() {
    let server = Server::answering(vec![Ok(json!({ "delegate": info("starting", None), "next": NEXT }))]);
    let (answer, error) = server.call(
        "delegate_task",
        json!({
            "task": "Fix the flaky login test.",
            "target": { "machineId": "pc", "providerInstanceId": "claude", "model": "opus", "options": [{ "id": "effort", "value": "high" }], "projectId": "p-1" },
            "title": "Login test",
            "role": "test",
            "isolation": "shared",
            "mode": "async",
            "timeoutMs": 5,
            "clientRequestId": "k-1",
            "runtimeMode": "auto-accept-edits",
            "interactionMode": "default",
        }),
    );
    assert!(!error, "{answer}");
    let Method::DelegateStart(start) = server.asked.borrow_mut().remove(0) else {
        panic!("a delegate.start");
    };
    assert_eq!(
        start,
        DelegateStartParams {
            thread: None,
            machine: "pc".into(),
            task: "Act as the test sub-agent for this task.\n\nFix the flaky login test.".into(),
            checkout: None,
            project: Some("p-1".into()),
            title: Some("Login test".into()),
            provider: Some(Provider::Claude),
            request: Some("k-1".into()),
            model: Some("opus".into()),
            effort: Some(Some("high".into())),
            mode: Some(RuntimeMode::AutoAcceptEdits),
        }
    );
    assert!(server.asked.borrow().is_empty(), "async: no wait");
    assert_eq!(
        answer,
        json!({
            "taskId": "d1",
            "childThreadId": null,
            "remoteChild": { "machineId": "pc", "threadId": 9, "label": "pc" },
            "status": "starting",
            "workState": "working",
            "hasPendingChildRuns": false,
            "latestTerminalStatus": null,
            "latestTerminalSummary": null,
            "providerInstanceId": "claude",
            "model": "opus",
            "executedModel": null,
            "summary": null,
            "hasPendingQuestion": false,
            "pendingApproval": null,
            "waitTimedOut": false,
            "next": NEXT,
        })
    );
}

/// T3's `options`: given values override what is inherited, nothing given
/// inherits, and `{}` or `[]` clears.
#[test]
fn options_override_inherit_or_clear_the_effort() {
    let started = || Ok(json!({ "delegate": info("starting", None) }));
    for (options, effort) in [
        (json!({ "effort": "max" }), Some(Some("max".to_string()))),
        (Value::Null, None),
        (json!({}), Some(None)),
        (json!([]), Some(None)),
    ] {
        let server = Server::answering(vec![started()]);
        let mut target = json!({ "machineId": "pc" });
        if !options.is_null() {
            target["options"] = options.clone();
        }
        server.call("delegate_task", json!({ "task": "x", "target": target }));
        let Method::DelegateStart(start) = server.asked.borrow_mut().remove(0) else {
            panic!("a delegate.start");
        };
        assert_eq!(start.effort, effort, "{options}");
    }
}

/// `mode: "wait"` reads the Delegate until it ends and answers its result;
/// past `timeoutMs` it answers T3's timed-out wait, the child still
/// running.
#[test]
fn a_wait_answers_the_result_or_that_it_timed_out() {
    let start = json!({ "task": "x", "target": { "machineId": "pc" }, "mode": "wait" });
    let server = Server::answering(vec![
        Ok(json!({ "delegate": info("starting", None) })),
        Ok(json!({ "delegate": info("completed", Some("All green.")) })),
    ]);
    let (done, error) = server.call("delegate_task", start.clone());
    assert!(!error, "{done}");
    assert_eq!(
        (&done["status"], &done["summary"], &done["latestTerminalSummary"], &done["waitTimedOut"]),
        (&"completed".into(), &"All green.".into(), &"All green.".into(), &false.into())
    );
    assert_eq!(done.get("next"), None);
    assert!(matches!(server.asked.borrow()[1], Method::DelegateGet(_)));

    let mut timed = start;
    timed["timeoutMs"] = 1.into();
    let server = Server::answering(vec![Ok(json!({ "delegate": info("running", None) }))]);
    let (running, _) = server.call("delegate_task", timed);
    assert_eq!(
        (&running["status"], &running["waitTimedOut"], &running["next"]),
        (&"running".into(), &true.into(), &NEXT.into())
    );
}

/// `driverKind` names the provider there whose driver it is: a Ferrite
/// provider is its own one instance. One there has none of, or one that
/// contradicts `providerInstanceId`, is refused in T3's words.
#[test]
fn a_driver_kind_names_its_provider_there() {
    let listed = || Ok(json!({ "providers": [{ "provider": "claude" }, { "provider": "codex" }] }));
    let started = || Ok(json!({ "delegate": info("starting", None) }));
    let server = Server::answering(vec![listed(), started()]);
    let target = json!({ "machineId": "pc", "driverKind": "codex", "model": "gpt-5.5" });
    server.call("delegate_task", json!({ "task": "x", "target": target }));
    let asked = server.asked.borrow();
    assert_eq!(
        asked[0],
        Method::ProviderList(MachineParams {
            machine: Some("pc".into())
        })
    );
    assert!(matches!(&asked[1], Method::DelegateStart(start) if start.provider == Some(Provider::Codex)));
    drop(asked);

    let server = Server::answering(vec![listed()]);
    let target = json!({ "machineId": "pc", "driverKind": "claudeAgent" });
    assert_eq!(
        server.call("delegate_task", json!({ "task": "x", "target": target })),
        failure(
            "provider_unavailable",
            "No instance of driver claudeAgent there can run a child task. orchestrator_capabilities with this machineId lists its providers."
        )
    );
    let server = Server::answering(vec![listed()]);
    let target = json!({ "machineId": "pc", "driverKind": "codex", "providerInstanceId": "claude" });
    assert_eq!(
        server.call("delegate_task", json!({ "task": "x", "target": target })),
        failure(
            "invalid_request",
            "Provider instance claude there uses driver claude, not codex."
        )
    );
}

/// What Ferrite cannot do there is refused before the Server hears it,
/// each in T3's manner, never taken and dropped.
#[test]
fn what_a_delegate_cannot_be_is_refused_without_asking() {
    let server = Server::answering(Vec::new());
    let there = json!({ "machineId": "pc" });
    for (arguments, said) in [
        (json!({ "task": "x", "target": {} }), failure("target_required", NO_MACHINE)),
        (json!({ "task": "x" }), failure("target_required", NO_MACHINE)),
        // `machine_list`'s own `machineId` names this machine, as T3's does.
        (
            json!({ "task": "x", "target": { "machineId": "srv-here" } }),
            failure("target_required", NO_MACHINE),
        ),
        (
            json!({ "task": "x", "target": there, "isolation": "worktree" }),
            failure("invalid_request", NO_WORKTREE),
        ),
        (
            json!({ "task": "x", "target": there, "interactionMode": "plan" }),
            failure("invalid_request", NO_PLAN),
        ),
        (
            json!({ "task": "x", "target": { "machineId": "pc", "options": [{ "id": "fast", "value": true }] } }),
            failure(
                "invalid_request",
                "Unknown option fast: orchestrator_capabilities lists each model's options.",
            ),
        ),
        (
            json!({ "task": " ", "target": there }),
            failure("invalid_request", "Pass task: what the child agent is to do."),
        ),
    ] {
        assert_eq!(server.call("delegate_task", arguments.clone()), said, "{arguments}");
    }
    assert!(server.asked.borrow().is_empty());
}

/// Each Server refusal reaches the agent with T3's code, and each API
/// name in it as these tools name it: `widens_cap` in T3's mode words.
#[test]
fn a_server_refusal_is_said_in_these_tools_names() {
    for ((code, message), said) in [
        (
            ("widens_cap", "Child runtime mode full_access is broader than parent mode auto_accept_edits."),
            failure(
                "runtime_mode_escalation_denied",
                "Child runtime mode full-access is broader than parent mode auto-accept-edits.",
            ),
        ),
        (
            ("widens_cap", "Child runtime mode auto is broader than parent mode approval_required."),
            failure(
                "runtime_mode_escalation_denied",
                "Child runtime mode auto is broader than parent mode approval-required.",
            ),
        ),
        (
            ("project_not_found", "No project there has this repository (github.com/acme/app). Pass `project`; `project.list` with this `machine` lists them. To bring the repository there, clone it on pc and open it as a Project there."),
            failure(
                "invalid_request",
                "No project there has this repository (github.com/acme/app). Pass target.projectId; project_list with this machineId lists them. To bring the repository there, clone it on pc and open it as a Project there.",
            ),
        ),
        (
            ("target_required", "Pass `project`: this Thread's project has no repository to match there."),
            failure(
                "target_required",
                "Pass target.projectId: this Thread's project has no repository to match there.",
            ),
        ),
        (
            ("target_required", "Pass `model` from `provider.list` with this `machine`."),
            failure(
                "target_required",
                "Pass target.model from orchestrator_capabilities with this machineId.",
            ),
        ),
        (
            ("invalid_request", "This machine is not linked to vps. `machine.list` lists the linked ones."),
            failure("invalid_request", "This machine is not linked to vps. machine_list lists the linked ones."),
        ),
        (
            ("linked_machines_off", "Linked machines is off on this machine."),
            failure("capability_denied", "Linked machines is off on this machine."),
        ),
        (
            ("unknown_delegate", "Delegated task d7 does not belong to thread 4."),
            failure("task_not_found", "Delegated task d7 does not belong to thread 4."),
        ),
        (
            ("failed", "pc did not answer at wss://pc:7878."),
            failure("orchestration_error", "pc did not answer at wss://pc:7878."),
        ),
    ] {
        let server = Server::answering(vec![Err(refusal(code, message))]);
        let start = json!({ "task": "x", "target": { "machineId": "pc" } });
        assert_eq!(server.call("delegate_task", start), said);
    }
}

/// An ended Delegate reads as T3's finished task, with no `next`; a
/// held-up one says why; an id this Thread never gave is T3's
/// `task_not_found`; a cancel answers T3's status.
#[test]
fn task_status_and_task_cancel_answer_t3s_shapes() {
    let mut held = info("running", None);
    held["problem"] = "pc did not answer at wss://pc:7878.".into();
    let server = Server::answering(vec![
        Ok(json!({ "delegate": info("completed", Some("All green.")) })),
        Ok(json!({ "delegate": held })),
        Ok(json!({ "delegate": "d1", "status": "cancel_requested" })),
    ]);
    let (done, _) = server.call("task_status", json!({ "taskId": "d1" }));
    assert_eq!(
        (&done["status"], &done["workState"], &done["summary"], &done["latestTerminalStatus"]),
        (&"completed".into(), &"result_available".into(), &"All green.".into(), &"completed".into())
    );
    assert_eq!(done.get("next"), None);
    let (running, _) = server.call("task_status", json!({ "taskId": "d1" }));
    assert_eq!(running["problem"], "pc did not answer at wss://pc:7878.");
    assert_eq!(running["next"], NEXT);
    let cancel = json!({ "taskId": "d1", "reason": "done", "clientRequestId": "c-1" });
    let (cancelled, _) = server.call("task_cancel", cancel);
    assert_eq!(cancelled, json!({ "taskId": "d1", "status": "cancel_requested" }));
    let asked = server.asked.borrow();
    assert!(matches!(&asked[0], Method::DelegateGet(DelegateParams { thread: None, .. })));
    assert!(matches!(
        &asked[2],
        Method::DelegateCancel(DelegateCancelParams { reason: Some(reason), .. }) if reason == "done"
    ));
    for tool in ["task_status", "task_cancel"] {
        assert_eq!(
            Server::answering(Vec::new()).call(tool, json!({ "taskId": "task-1" })),
            failure("task_not_found", "Delegated task task-1 does not belong to thread 4.")
        );
    }
}

/// `machine_list` reaches every machine and lists each as T3's link, with
/// this machine's id, each link's access and end, and what to do for one
/// not listed.
#[test]
fn machine_list_is_t3s_links() {
    let server = Server::answering(vec![Ok(json!({ "server_id": "srv-here", "machines": [
        { "id": "pc", "label": "pc", "target": "wss://pc:7878", "expires_at": "2027-02-20T10:00:00Z", "status": "reachable" },
        { "id": "old", "label": "old", "target": "wss://old:7878", "expires_at": "2026-01-01T00:00:00Z", "status": "expired", "problem": "The link to old expired." },
    ] }))]);
    let (listed, _) = server.call("machine_list", json!({}));
    assert_eq!(
        listed,
        json!({
            "machineId": "srv-here",
            "links": [
                { "machineId": "pc", "label": "pc", "status": "reachable", "access": "full-access", "expiresAt": "2027-02-20T10:00:00Z", "lastError": null },
                { "machineId": "old", "label": "old", "status": "expired", "access": "full-access", "expiresAt": "2026-01-01T00:00:00Z", "lastError": "The link to old expired." },
            ],
            "notListed": NOT_LISTED,
        })
    );
    assert_eq!(
        *server.asked.borrow(),
        vec![Method::MachineList(MachineListParams { reach: true })]
    );
}

/// `project_list` is T3's: here or there, a page at a time from `cursor`,
/// each Project with the repository a start matches it by.
#[test]
fn project_list_pages_the_projects_there() {
    let project = |id: &str| {
        json!({ "id": id, "label": format!("app {id}"), "focused": false, "thread_count": 0,
            "directory": format!("/src/{id}"), "worktree": { "repository": "github.com/acme/app" } })
    };
    let listed = json!({ "projects": [project("1"), project("2"), project("3")] });
    let server = Server::answering(vec![Ok(listed)]);
    let (page, _) = server.call("project_list", json!({ "machineId": "pc", "cursor": 1, "limit": 1 }));
    assert_eq!(
        page,
        json!({
            "projects": [{ "id": "2", "title": "app 2", "workspaceRoot": "/src/2", "repositoryIdentity": { "canonicalKey": "github.com/acme/app" } }],
            "nextCursor": 2,
        })
    );
    let (last, _) = server.call("project_list", json!({ "cursor": 2, "limit": 500 }));
    assert_eq!(last["nextCursor"], Value::Null);
    server.call("project_list", json!({ "machineId": "srv-here" }));
    assert_eq!(
        *server.asked.borrow(),
        vec![
            Method::ProjectList(MachineParams {
                machine: Some("pc".into())
            }),
            Method::ProjectList(MachineParams::default()),
            Method::ProjectList(MachineParams::default()),
        ]
    );
    assert_eq!(
        server.call("project_list", json!({ "limit": 0 })),
        failure("invalid_request", "Pass limit: a whole number from 1.")
    );
}

fn providers() -> Value {
    json!({ "providers": [{
        "provider": "claude",
        "models": [
            { "model": "opus", "label": "Opus", "efforts": ["low", "high"], "default_effort": "high" },
            { "model": "haiku", "label": "Haiku", "efforts": [] },
        ],
        "modes": ["approval_required", "full_access"],
        "mode": "approval_required",
    }, {
        "provider": "codex",
        "models": (1..=10).map(|n| json!({ "model": format!("gpt-5.{n}"), "label": format!("GPT-5.{n}"), "efforts": [] })).collect::<Vec<_>>(),
        "modes": ["auto"],
        "mode": "auto",
    }], "default_provider": "claude" })
}

fn defaults() -> Value {
    json!({ "thread": 4, "provider": "codex", "model": "gpt-5.5", "effort": "high", "mode": "auto_accept_edits" })
}

/// Here, `orchestrator_capabilities` lists what this machine runs and
/// what this Thread hands on, the model being the one its Session runs;
/// with a machine, that machine's, which knows no parent (T3 asks the
/// machine itself). Both say this Thread's modes and Ferrite's features.
#[test]
fn capabilities_are_t3s_here_and_there() {
    let server = Server::answering(vec![Ok(defaults()), Ok(providers())]);
    let (here, _) = server.call("orchestrator_capabilities", json!({}));
    assert_eq!(
        (&here["parentThreadId"], &here["inheritedProviderInstanceId"], &here["inheritedModel"]),
        (&4.into(), &"codex".into(), &"gpt-5.5".into())
    );
    assert_eq!(here["inheritedOptions"], json!([{ "id": "effort", "value": "high" }]));
    assert_eq!(
        (&here["runtimeMode"], &here["interactionMode"]),
        (&"auto-accept-edits".into(), &"default".into())
    );
    assert_eq!(here["features"]["asyncPolling"], true);
    assert_eq!(here["features"]["scheduledTasks"], false);
    assert_eq!(
        here["providers"][0],
        json!({
            "providerInstanceId": "claude",
            "driverKind": "claude",
            "displayName": "Claude",
            "models": [
                { "id": "opus", "options": [{ "id": "effort", "values": ["low", "high"], "default": "high" }] },
                { "id": "haiku" },
            ],
            "runtimeModes": ["approval-required", "full-access"],
            "canRunChildTask": true,
            "canRunCrossProviderChildTask": true,
            "constraints": [],
        })
    );
    assert_eq!(here["providers"][1]["models"].as_array().unwrap().len(), 10, "every model");
    assert_eq!(here["providers"][1].get("omittedModels"), None);
    assert_eq!(
        server.asked.borrow().as_slice(),
        [
            Method::DelegateDefaults(ThreadParams { thread: ThreadId::new(4) }),
            Method::ProviderList(MachineParams::default()),
        ]
    );

    let server = Server::answering(vec![Ok(defaults()), Ok(providers())]);
    let (there, _) = server.call("orchestrator_capabilities", json!({ "machineId": "pc" }));
    assert_eq!(
        (&there["parentThreadId"], &there["inheritedModel"], &there["inheritedOptions"]),
        (&Value::Null, &Value::Null, &json!([]))
    );
    assert_eq!(there["runtimeMode"], "auto-accept-edits");
    assert_eq!(
        server.asked.borrow()[1],
        Method::ProviderList(MachineParams {
            machine: Some("pc".into())
        })
    );
}

/// T3's `search`: a word of a provider's names names it, not a model;
/// the rest match model ids and labels, at most eight a provider, the
/// others counted in `omittedModels`.
#[test]
fn a_search_lists_the_models_it_matches() {
    let search = |words: &str| {
        let server = Server::answering(vec![Ok(defaults()), Ok(providers())]);
        server.call("orchestrator_capabilities", json!({ "search": words })).0
    };
    let ids = |capabilities: &Value, provider: usize| -> Vec<String> {
        capabilities["providers"][provider]["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|model| model["id"].as_str().unwrap().to_string())
            .collect()
    };
    let gpt = search("gpt");
    assert!(ids(&gpt, 0).is_empty());
    assert_eq!(ids(&gpt, 1).len(), 8);
    assert_eq!(gpt["providers"][1]["omittedModels"], 2);
    let opus = search("Claude OPUS");
    assert_eq!(ids(&opus, 0), ["opus"]);
    assert!(ids(&opus, 1).is_empty(), "codex has no opus");
    let codex = search("codex");
    assert_eq!(ids(&codex, 1).len(), 10, "the provider's own name lists it all");
    let server = Server::answering(Vec::new());
    assert_eq!(
        server.call("orchestrator_capabilities", json!({ "search": "   " })),
        failure("invalid_request", "Pass search: 1 to 200 characters.")
    );
}

/// The orchestration block names only tools Ferrite offers, and none of
/// T3's own.
#[test]
fn the_instructions_name_only_these_tools() {
    for name in NAMES.iter().filter(|name| **name != "project_list") {
        assert!(INSTRUCTIONS.contains(&format!("`{name}`")), "{name}");
    }
    for t3 in ["t3_", "environment", "isolation", "create_threads", "request_secret"] {
        assert!(!INSTRUCTIONS.contains(t3), "{t3}");
    }
}
