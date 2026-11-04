#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use ferrite_core::progress::StepStatus;
use serde_json::json;
use support::*;

#[test]
fn proposed_tool_input_does_not_create_or_complete_a_native_plan() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"assistant","uuid":"create","message":{"id":"msg","content":[{"type":"tool_use","id":"tool","name":"TaskCreate","input":{"subject":"Not accepted"}}]}}),
        ],
    );
    assert!(fold(r.drain()).view().main().transcript().todos().is_none());
}

#[test]
fn native_task_get_status_is_not_mistaken_for_pending_creation() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"user","uuid":"task","tool_use_result":{"task":{"id":"17","subject":"Implemented","status":"completed","description":"Done","blocks":[],"blockedBy":[]}},"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"get","content":"Done"}]}}),
        ],
    );
    let a = fold(r.drain());
    let p = a.view().main().transcript().progress();
    assert_eq!(p.plan.len(), 1);
    assert_eq!(p.plan[0].status, StepStatus::Completed);
}

#[test]
fn authoritative_native_task_lists_replace_the_plan_and_keep_task_ids() {
    let receipt = |id: &str, result: serde_json::Value| json!({"type":"user","uuid":id,"tool_use_result":result,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":id,"content":"OK"}]}});
    let r = Replay::new(
        "claude",
        vec![
            receipt("old", json!({"task":{"id":"old","subject":"Remove me"}})),
            receipt(
                "list",
                json!({"tasks":[{"id":"b","subject":"Keep me","status":"in_progress","blockedBy":[]}]}),
            ),
            receipt(
                "update",
                json!({"success":true,"taskId":"b","updatedFields":["status"],"statusChange":{"from":"in_progress","to":"completed"}}),
            ),
        ],
    );
    let a = fold(r.drain());
    let p = a.view().main().transcript().progress();
    assert_eq!(p.plan.len(), 1);
    assert_eq!(p.plan[0].text, "Keep me");
    assert_eq!(p.plan[0].status, StepStatus::Completed);
}

#[test]
fn native_todo_receipt_replaces_the_previous_todo_plan() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"user","uuid":"todos","tool_use_result":{"oldTodos":[{"content":"Old","status":"pending","activeForm":"Old"}],"newTodos":[{"content":"Current","status":"in_progress","activeForm":"Working"}]},"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"todo","content":"Updated"}]}}),
        ],
    );
    let a = fold(r.drain());
    let p = a.view().main().transcript().progress();
    assert_eq!(p.plan.len(), 1);
    assert_eq!(p.plan[0].text, "Current");
    assert_eq!(p.plan[0].status, StepStatus::InProgress);
}

/// A captured fixture's frames (`tests/fixtures`), one JSON value a line.
fn fixture(name: &str) -> Vec<serde_json::Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Each distinct plan Main's transcript held, folding `events` in order.
fn plans(events: Vec<ferrite_core::SessionEvent>) -> Vec<Vec<(String, StepStatus)>> {
    use ferrite_core::activity::{Activity, ActivityInput};
    let mut activity = Activity::default();
    activity.apply(ActivityInput::Connect { generation: 1 });
    let mut plans: Vec<Vec<(String, StepStatus)>> = Vec::new();
    for event in events {
        let at = std::time::Instant::now();
        activity.apply(match event {
            ferrite_core::SessionEvent::Activity(event) => ActivityInput::Observe {
                generation: 1,
                event,
                at,
            },
            event => ActivityInput::Main {
                input: ferrite_core::transcript::Input::Event(event),
                at,
            },
        });
        let plan: Vec<_> = activity
            .view()
            .main()
            .transcript()
            .progress()
            .plan
            .iter()
            .map(|step| (step.text.clone(), step.status))
            .collect();
        if !plan.is_empty() && plans.last() != Some(&plan) {
            plans.push(plan);
        }
    }
    plans
}

fn plan(steps: &[(&str, StepStatus)]) -> Vec<(String, StepStatus)> {
    steps
        .iter()
        .map(|(text, status)| ((*text).into(), *status))
        .collect()
}

/// Real `claude` 2.1.318 (Opus 5.5) on a four-step task in Ferrite: no
/// native checklist tool is offered, so it plans with Ferrite's
/// `update_plan` — a plan at its first action, `advance` as each step
/// finishes, `done` for the last — and the plan card's steps move with it.
#[test]
fn claude_plans_with_update_plan_and_the_steps_move_as_it_works() {
    use StepStatus::*;
    let frames = fixture("claude-update-plan-2.1.318.jsonl");
    let init = frames
        .iter()
        .find(|frame| frame["subtype"] == "init")
        .unwrap();
    let tools = init["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool == "mcp__ferrite__update_plan"));
    assert!(!tools
        .iter()
        .any(|tool| ["TaskCreate", "TodoWrite"].contains(&tool.as_str().unwrap())));

    let r = Replay::new("claude", frames);
    let events = r.drain();
    // Every call was accepted, its answer says where the plan stands, and
    // its words are the plan's.
    let answers = r.wait_host(|line| {
        line["response"]["response"]["mcp_response"]["result"]["content"][0]["text"]
            == "Plan shown (4 steps)."
    });
    assert_eq!(
        answers["response"]["response"]["mcp_response"]["result"]["isError"],
        false
    );
    let step = |names: [StepStatus; 4]| {
        plan(&[
            ("Write calc.py", names[0]),
            ("Write test_calc.py", names[1]),
            ("Run tests", names[2]),
            ("Write README.md", names[3]),
        ])
    };
    assert_eq!(
        plans(events),
        [
            step([InProgress, Pending, Pending, Pending]),
            step([Completed, InProgress, Pending, Pending]),
            step([Completed, Completed, InProgress, Pending]),
            step([Completed, Completed, Completed, InProgress]),
            step([Completed, Completed, Completed, Completed]),
        ]
    );
}

/// Real `codex` 0.166.0 on the same task, its own `update_plan` enabled and
/// the planning guidance given at thread start: `turn/plan/updated` moves
/// the same plan model.
#[test]
fn codex_plans_with_its_own_update_plan_and_the_steps_move_as_it_works() {
    use StepStatus::*;
    let r = Replay::new("codex", fixture("codex-update-plan-0.166.0.jsonl"));
    let step = |names: [StepStatus; 3]| {
        plan(&[
            ("Create module and tests", names[0]),
            ("Run unittest tests", names[1]),
            ("Write README", names[2]),
        ])
    };
    assert_eq!(
        plans(r.drain()),
        [
            step([InProgress, Pending, Pending]),
            step([Completed, InProgress, Pending]),
            step([Completed, Completed, InProgress]),
            step([Completed, Completed, Completed]),
        ]
    );
}

/// The frames of one `update_plan` (or `wrap_up`) call as the CLI streams
/// it: the call, the in-band request the server answers, and its receipt.
fn call(
    id: &str,
    tool: &str,
    input: serde_json::Value,
    receipt: serde_json::Value,
) -> Vec<serde_json::Value> {
    vec![
        json!({"type":"assistant","parent_tool_use_id":null,"message":{"id":format!("m-{id}"),"role":"assistant","content":[{"type":"tool_use","id":id,"name":format!("mcp__ferrite__{tool}"),"input":input}]}}),
        json!({"type":"control_request","request_id":format!("r-{id}"),"request":{"subtype":"mcp_message","server_name":"ferrite","message":{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":tool,"arguments":input,"_meta":{"claudecode/toolUseId":id}}}}}),
        receipt,
    ]
}

fn receipt(id: &str, parent: Option<&str>, is_error: bool) -> serde_json::Value {
    json!({"type":"user","parent_tool_use_id":parent,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":id,"is_error":is_error,"content":"ok"}]}})
}

fn first_plan() -> serde_json::Value {
    json!({"scope":"first_task","items":[{"content":"One","status":"in_progress"},{"content":"Two","status":"pending"}]})
}

/// §12: children do not update Main's plan, and a call the CLI failed
/// moves nothing.
#[test]
fn a_subagents_or_failed_plan_call_leaves_mains_plan_alone() {
    use StepStatus::*;
    let mut frames = call(
        "p1",
        "update_plan",
        first_plan(),
        receipt("p1", None, false),
    );
    frames.extend(call(
        "p2",
        "update_plan",
        json!({"advance":true}),
        receipt("p2", Some("agent"), false),
    ));
    frames.extend(call(
        "p3",
        "update_plan",
        json!({"advance":true}),
        receipt("p3", None, true),
    ));
    let r = Replay::new("claude", frames);
    assert_eq!(
        plans(r.drain()),
        [plan(&[("One", InProgress), ("Two", Pending)])]
    );
}

/// A call the server refuses is answered as an error, telling the agent
/// what to fix: a step number past the plan it set.
#[test]
fn a_step_past_the_plan_is_refused_to_the_agent() {
    let mut frames = call(
        "p1",
        "update_plan",
        first_plan(),
        receipt("p1", None, false),
    );
    frames.extend(call(
        "p2",
        "update_plan",
        json!({"done":[3]}),
        receipt("p2", None, true),
    ));
    let r = Replay::new("claude", frames);
    r.drain();
    let refusal = r.wait_host(|line| line["response"]["request_id"] == "r-p2");
    let result = &refusal["response"]["response"]["mcp_response"]["result"];
    assert_eq!(result["isError"], true);
    assert!(result["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("1 to 2"));
}

/// §12–13: the wrap-up of a turn that moved the plan settles its unfinished
/// steps; a blocked wrap-up is not the end of the work and settles nothing;
/// and a step change after settling applies to the plan as the agent left
/// it, not as the settling marked it.
#[test]
fn a_wrap_up_settles_the_plan_it_closes_and_keeps_what_the_agent_said() {
    use StepStatus::*;
    let blocked = {
        let mut frames = call(
            "p1",
            "update_plan",
            first_plan(),
            receipt("p1", None, false),
        );
        frames.extend(call(
            "w1",
            "wrap_up",
            json!({"recap":"Stuck.","blocked":true}),
            receipt("w1", None, false),
        ));
        plans(Replay::new("claude", frames).drain())
    };
    assert_eq!(blocked, [plan(&[("One", InProgress), ("Two", Pending)])]);

    let mut frames = call(
        "p1",
        "update_plan",
        first_plan(),
        receipt("p1", None, false),
    );
    frames.extend(call(
        "w1",
        "wrap_up",
        json!({"recap":"Done."}),
        receipt("w1", None, false),
    ));
    frames.extend(call(
        "p2",
        "update_plan",
        json!({"advance":true}),
        receipt("p2", None, false),
    ));
    assert_eq!(
        plans(Replay::new("claude", frames).drain()),
        [
            plan(&[("One", InProgress), ("Two", Pending)]),
            plan(&[("One", Completed), ("Two", Completed)]),
            plan(&[("One", Completed), ("Two", InProgress)]),
        ]
    );
}

/// A wrap-up in a turn that never touched the plan leaves an earlier plan
/// as it stood — the turn that made it may have been cut short.
#[test]
fn a_later_turns_wrap_up_leaves_an_earlier_plan_alone() {
    use StepStatus::*;
    let mut frames = call(
        "p1",
        "update_plan",
        first_plan(),
        receipt("p1", None, false),
    );
    frames.push(json!({"type":"result","subtype":"success","parent_tool_use_id":null,"is_error":false,"result":"","session_id":"s"}));
    frames.extend(call(
        "w1",
        "wrap_up",
        json!({"recap":"Answered."}),
        receipt("w1", None, false),
    ));
    assert_eq!(
        plans(Replay::new("claude", frames).drain()),
        [plan(&[("One", InProgress), ("Two", Pending)])]
    );
}
