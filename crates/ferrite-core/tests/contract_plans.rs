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
