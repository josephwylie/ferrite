use std::cell::RefCell;

use super::*;

/// What a call asked, and the answers it got, in order.
struct Server {
    asked: RefCell<Vec<Method>>,
    answers: RefCell<Vec<Result<Value, String>>>,
}

impl Server {
    fn answering(answers: Vec<Result<Value, String>>) -> Self {
        Self {
            asked: RefCell::default(),
            answers: RefCell::new(answers),
        }
    }

    fn call(&self, tool: &str, arguments: Value) -> (Value, bool) {
        let ask = |method| {
            self.asked.borrow_mut().push(method);
            self.answers.borrow_mut().remove(0)
        };
        let answer = super::call(tool, &arguments, ThreadId::new(4), &ask);
        let text = serde_json::from_str(&answer.text).unwrap_or(Value::String(answer.text));
        (text, answer.is_error)
    }
}

fn info(status: &str, result: Option<&str>) -> Value {
    json!({
        "delegate": "d1", "parent": 4, "machine": "pc", "server": "srv-pc", "thread": 9,
        "title": "Login test", "provider": "claude", "model": "opus", "status": status,
        "result": result, "problem": null, "stop_owed": false,
    })
}

/// T3's five names, each with T3's required parameters.
#[test]
fn the_tools_are_t3s() {
    let definitions = definitions();
    let names: Vec<&str> = definitions.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, NAMES);
    let schema = |name: &str| {
        definitions
            .iter()
            .find(|tool| tool.name == name)
            .unwrap()
            .input_schema
            .clone()
    };
    assert_eq!(schema("delegate_task")["required"], json!(["task", "target"]));
    assert_eq!(
        schema("delegate_task")["properties"]["target"]["required"],
        json!(["machineId"])
    );
    assert_eq!(schema("task_status")["required"], json!(["taskId"]));
    assert_eq!(schema("task_cancel")["required"], json!(["taskId"]));
    let delegate = &definitions[1].description;
    assert!(delegate.starts_with("Start one child agent on a linked machine with only `task`."));
    assert!(!delegate.contains("isolation"), "no worktree isolation");
}

/// `delegate_task` is one `delegate.start` from the Session's Thread, in
/// T3's parameters: the role said first, the effort from either form of
/// `options`, the mode by T3's word; its answer is T3's task with `next`.
#[test]
fn delegate_task_starts_a_delegate_and_says_not_to_poll() {
    let server = Server::answering(vec![Ok(json!({ "delegate": info("starting", None), "next": NEXT }))]);
    let (answer, error) = server.call(
        "delegate_task",
        json!({
            "task": "Fix the flaky login test.",
            "target": r#"{"machineId":"pc","providerInstanceId":"claude","model":"opus","options":[{"id":"effort","value":"high"}],"projectId":"p-1"}"#,
            "title": "Login test",
            "role": "test",
            "clientRequestId": "k-1",
            "runtimeMode": "auto-accept-edits",
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
            effort: Some("high".into()),
            mode: Some(RuntimeMode::AutoAcceptEdits),
        }
    );
    assert_eq!(
        answer,
        json!({
            "taskId": "d1",
            "remoteChild": { "machineId": "pc", "threadId": 9, "label": "pc" },
            "status": "starting",
            "workState": "working",
            "providerInstanceId": "claude",
            "model": "opus",
            "summary": null,
            "next": NEXT,
        })
    );
}

/// What Ferrite cannot honour is refused before the Server hears it,
/// never taken and dropped.
#[test]
fn what_a_delegate_cannot_be_is_refused_without_asking() {
    let server = Server::answering(Vec::new());
    for (arguments, said) in [
        (json!({ "task": "x", "target": {} }), NO_MACHINE.to_string()),
        (json!({ "task": "x" }), NO_MACHINE.to_string()),
        (
            json!({ "task": "x", "target": { "machineId": "pc", "options": { "fast": true } } }),
            "Unknown option fast: orchestrator_capabilities lists each model's options.".into(),
        ),
        (
            json!({ "task": "x", "target": { "machineId": "pc" }, "runtimeMode": "yolo" }),
            "Unknown runtimeMode yolo.".into(),
        ),
        (
            json!({ "task": " ", "target": { "machineId": "pc" } }),
            "Pass task: what the child agent is to do.".into(),
        ),
    ] {
        assert_eq!(server.call("delegate_task", arguments), (Value::String(said), true));
    }
    assert!(server.asked.borrow().is_empty());
}

/// An ended Delegate reads as T3's finished task, with no `next`; a
/// held-up one says why; a cancel answers T3's status.
#[test]
fn task_status_and_task_cancel_answer_t3s_shapes() {
    let mut held = info("running", None);
    held["problem"] = "pc did not answer: no route to host".into();
    let server = Server::answering(vec![
        Ok(json!({ "delegate": info("completed", Some("All green.")) })),
        Ok(json!({ "delegate": held })),
        Ok(json!({ "delegate": "d1", "status": "cancel_requested" })),
    ]);
    let (done, _) = server.call("task_status", json!({ "taskId": "d1" }));
    assert_eq!(
        (&done["status"], &done["workState"], &done["summary"], done.get("next")),
        (&"completed".into(), &"result_available".into(), &"All green.".into(), None)
    );
    let (running, _) = server.call("task_status", json!({ "taskId": "d1" }));
    assert_eq!(running["problem"], "pc did not answer: no route to host");
    assert_eq!(running["next"], NEXT);
    let (cancelled, _) = server.call("task_cancel", json!({ "taskId": "d1", "reason": "done" }));
    assert_eq!(cancelled, json!({ "taskId": "d1", "status": "cancel_requested" }));
    let asked = server.asked.borrow();
    assert!(matches!(&asked[0], Method::DelegateGet(DelegateParams { thread: None, .. })));
    assert!(matches!(
        &asked[2],
        Method::DelegateCancel(DelegateCancelParams { reason: Some(reason), .. }) if reason == "done"
    ));
    assert_eq!(
        Server::answering(Vec::new()).call("task_status", json!({ "taskId": "1" })).1,
        true,
        "an id reads `d1`"
    );
}

/// `machine_list` reaches every machine and lists each as T3's link, with
/// what to do for one not listed.
#[test]
fn machine_list_is_t3s_links() {
    let server = Server::answering(vec![Ok(json!({ "machines": [
        { "id": "pc", "label": "pc", "target": "me@pc", "status": "reachable" },
        { "id": "vps", "label": "vps", "target": "vps", "status": "sign_in", "problem": "vps needs a sign-in: the user signs in to it from a Ferrite window." },
    ] }))]);
    let (listed, _) = server.call("machine_list", json!({}));
    assert_eq!(
        listed,
        json!({
            "machines": [
                { "machineId": "pc", "label": "pc", "status": "reachable", "lastError": null },
                { "machineId": "vps", "label": "vps", "status": "sign_in", "lastError": "vps needs a sign-in: the user signs in to it from a Ferrite window." },
            ],
            "notListed": NOT_LISTED,
        })
    );
    assert_eq!(
        *server.asked.borrow(),
        vec![Method::MachineList(MachineListParams { reach: true })]
    );
}

/// `orchestrator_capabilities` with a machine lists what that machine
/// runs, in T3's shape, and what this Thread would hand on.
#[test]
fn capabilities_are_the_machines_with_this_threads_inheritance() {
    let server = Server::answering(vec![
        Ok(json!({ "thread": { "thread": 4, "provider": "codex", "model": "gpt-5.5", "effort": "high" } })),
        Ok(json!({ "providers": [{
            "provider": "claude",
            "models": [
                { "model": "opus", "label": "Opus", "efforts": ["low", "high"], "default_effort": "high" },
                { "model": "haiku", "label": "haiku", "efforts": [] },
            ],
            "modes": ["approval_required", "full_access"],
            "mode": "approval_required",
        }], "default_provider": "claude" })),
    ]);
    let (capabilities, _) = server.call("orchestrator_capabilities", json!({ "machineId": "pc" }));
    assert_eq!(
        capabilities,
        json!({
            "parentThreadId": 4,
            "inheritedProviderInstanceId": "codex",
            "inheritedModel": "gpt-5.5",
            "inheritedOptions": [{ "id": "effort", "value": "high" }],
            "providers": [{
                "providerInstanceId": "claude",
                "displayName": "Claude",
                "models": [
                    { "id": "opus", "label": "Opus", "options": [{ "id": "effort", "values": ["low", "high"], "default": "high" }] },
                    { "id": "haiku" },
                ],
                "runtimeModes": ["approval-required", "full-access"],
                "canRunChildTask": true,
                "canRunCrossProviderChildTask": true,
                "constraints": [],
            }],
        })
    );
    assert_eq!(
        server.asked.borrow()[1],
        Method::ProviderList(ProviderListParams {
            machine: Some("pc".into())
        })
    );
}

/// A refusal reaches the agent in the Server's words.
#[test]
fn a_refusal_is_the_servers_words() {
    let off = "Refused (linked_machines_off): Linked machines is off on this machine.";
    let server = Server::answering(vec![Err(off.into())]);
    assert_eq!(
        server.call("machine_list", json!({})),
        (Value::String(off.into()), true)
    );
}
