#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use ferrite_core::{SessionEvent, TurnOutcome};
use serde_json::json;
use support::*;
#[test]
fn claude_native_context_report_owns_the_meter_without_model_name_guessing() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"assistant","uuid":"context","session_id":"root","parent_tool_use_id":null,"context_usage":{"model":"custom-model","total_tokens":32100,"raw_max_tokens":180000,"percentage":18,"categories":[],"mcp_tools":[],"memory_files":[],"agents":[]},"message":{"id":"context","model":"custom-model","content":[{"type":"text","text":"Context"}],"usage":{"input_tokens":3,"output_tokens":1}}}),
        ],
    );
    let events = r.drain();
    let usage = events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::TokenUsage {
                total_tokens,
                context_window,
                ..
            } => Some((*total_tokens, *context_window)),
            _ => None,
        })
        .last();
    assert_eq!(usage, Some((32100, Some(180000))));
}
#[test]
fn claude_result_failure_preserves_all_native_error_details() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"result","subtype":"error_during_execution","is_error":true,"errors":["Plugin failed to load","Authentication expired"],"session_id":"root","uuid":"result"}),
        ],
    );
    let events = r.drain();
    let error = events
        .iter()
        .find_map(|e| match e {
            SessionEvent::TurnEnded {
                outcome: TurnOutcome::Error(s),
                ..
            } => Some(s),
            _ => None,
        })
        .unwrap();
    assert!(
        error.contains("Plugin failed to load") && error.contains("Authentication expired"),
        "native details lost: {error}"
    );
}
#[test]
fn claude_command_catalog_refresh_replaces_stale_commands() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"system","subtype":"commands_changed","session_id":"root","uuid":"commands","commands":[{"name":"review","description":"Review the changes","argumentHint":"[scope]"}]}),
        ],
    );
    let events = r.drain();
    assert!(events.iter().any(|e|matches!(e,SessionEvent::Commands{commands} if commands.len()==1 && commands[0].name=="review")));
}
