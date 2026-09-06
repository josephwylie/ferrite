#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use ferrite_core::{SessionEvent, TurnOutcome};
use serde_json::json;
use support::*;

#[test]
fn claude_background_snapshots_replace_tasks_and_exclude_ambient_work() {
    let first = json!({"type":"system","subtype":"background_tasks_changed","session_id":"root","uuid":"snapshot-1","tasks":[{"task_id":"work","task_type":"local_bash","description":"Build","ambient":false},{"task_id":"ambient","task_type":"local_bash","description":"Poll","ambient":true}]});
    let r = Replay::new("claude", vec![first.clone()]);
    let a = fold(r.drain());
    let tasks = a.view().main().transcript().progress().background();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, "work");
    assert_eq!(a.view().main().transcript().progress().working_background(), 1, "snapshot membership itself means live background activity");
    let r = Replay::new("claude", vec![first, json!({"type":"system","subtype":"background_tasks_changed","session_id":"root","uuid":"snapshot-2","tasks":[]})]);
    assert!(fold(r.drain()).view().main().transcript().progress().background().is_empty(), "empty native snapshot must remove stale tasks");
}

#[test]
fn claude_ambient_and_hidden_tasks_do_not_create_working_children() {
    for flag in ["ambient", "skip_transcript"] {
        let mut frame = json!({"type":"system","subtype":"task_started","session_id":"root","uuid":"task","task_id":"ambient","tool_use_id":"tool","task_type":"local_agent","description":"Maintenance"});
        frame[flag] = json!(true);
        let r = Replay::new("claude", vec![frame, json!({"type":"system","subtype":"task_progress","session_id":"root","uuid":"later","task_id":"ambient","description":"Still running"})]);
        let a = fold(r.drain());
        assert!(a.view().children().is_empty(), "excluded native tasks must not enter visible agent activity");
    }
}

#[test]
fn native_diagnostics_are_visible_without_ending_a_turn() {
    for (provider, frame, wanted) in [
        ("codex", json!({"method":"configWarning","params":{"summary":"Invalid plugin","details":"Update the plugin path","path":"config.toml"}}), "Update the plugin path"),
        ("codex", json!({"method":"deprecationNotice","params":{"summary":"Old setting","details":"Use the replacement setting"}}), "Use the replacement setting"),
        ("claude", json!({"type":"system","subtype":"informational","session_id":"root","uuid":"info","content":"Reconnect the server","level":"warning"}), "Reconnect the server"),
        ("claude", json!({"type":"auth_status","session_id":"root","uuid":"auth","isAuthenticating":false,"output":[],"error":"Sign in again"}), "Sign in again"),
    ] {
        let r = Replay::new(provider, vec![frame]);
        let events = r.drain();
        assert!(!events.iter().any(|e| matches!(e, SessionEvent::TurnEnded { .. })));
        let a = fold(events);
        assert!(a.view().main().transcript().blocks().iter().any(|b| matches!(&b.body,ferrite_core::transcript::Body::Notice(s) if s.contains(wanted))), "native diagnostic dropped: {wanted}");
    }
}

#[test]
fn codex_rerouted_model_updates_effective_model_without_resetting_conversation() {
    let r = Replay::new("codex", vec![json!({"method":"model/rerouted","params":{"threadId":"root","turnId":"turn","fromModel":"original","toModel":"fallback","reason":"highRiskCyberActivity"}})]);
    let a = fold(r.drain());
    assert_eq!(a.view().main().transcript().model(), Some("fallback"));
    assert_eq!(a.view().main().transcript().session_id(), Some("root"));
}

#[test]
fn codex_settings_notification_reads_the_native_nested_settings() {
    let r = Replay::new("codex", vec![json!({"method":"thread/settings/updated","params":{"threadId":"root","threadSettings":{"model":"native-model","approvalPolicy":"never"}}})]);
    let events = r.drain();
    assert!(events.iter().any(|e| matches!(e, SessionEvent::PermissionMode { mode } if mode == "never")));
    assert_eq!(fold(events).view().main().transcript().model(), Some("native-model"));
}
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

#[test]
fn claude_native_context_report_does_not_require_api_usage() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"assistant","uuid":"context","session_id":"root","parent_tool_use_id":null,"context_usage":{"model":"custom-model","total_tokens":32100,"raw_max_tokens":180000,"percentage":18,"categories":[],"mcp_tools":[],"memory_files":[],"agents":[]},"message":{"id":"context","content":[{"type":"text","text":"Context"}]}}),
        ],
    );
    let events = r.drain();
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::TokenUsage {
                total_tokens: 32100,
                context_window: Some(180000),
                ..
            }
        )),
        "native context report without API usage was dropped"
    );
}

#[test]
fn claude_flat_rate_limit_updates_preserve_the_other_window() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","rateLimitType":"five_hour","utilization":0.72,"resetsAt":1800000000}}),
            json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed","rateLimitType":"seven_day","utilization":0.21,"resetsAt":1800500000}}),
        ],
    );
    let a = fold(r.drain());
    let limits = a.view().main().transcript().rate_limits();
    assert!((limits.five_hour.unwrap().used_fraction - 0.72).abs() < 0.0001);
    assert!((limits.weekly.unwrap().used_fraction - 0.21).abs() < 0.0001);
}
#[test]
fn codex_thread_warning_is_visible_without_finishing_the_turn() {
    let r = Replay::new(
        "codex",
        vec![
            json!({"method":"warning","params":{"threadId":"root","message":"Configured plugin could not start"}}),
        ],
    );
    let events = r.drain();
    assert!(!events
        .iter()
        .any(|e| matches!(e, SessionEvent::TurnEnded { .. })));
    let a = fold(events);
    assert!(a.view().main().transcript().blocks().iter().any(|b|matches!(&b.body,ferrite_core::transcript::Body::Notice(s) if s.contains("Configured plugin could not start"))));
}
#[test]
fn claude_live_permission_mode_is_reflected_from_native_status() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"system","subtype":"status","session_id":"root","uuid":"mode","status":null,"permissionMode":"plan"}),
        ],
    );
    assert!(r
        .drain()
        .iter()
        .any(|e| matches!(e,SessionEvent::PermissionMode{mode} if mode=="plan")));
}

#[test]
fn claude_session_state_is_authoritative_without_inventing_turn_completion() {
    for (state, busy, status) in [
        ("running", true, ferrite_core::activity::AgentStatus::Working),
        ("requires_action", true, ferrite_core::activity::AgentStatus::Waiting),
        ("idle", false, ferrite_core::activity::AgentStatus::Idle),
    ] {
        let r = Replay::new("claude", vec![json!({"type":"system","subtype":"session_state_changed","state":state,"session_id":"root","uuid":"state"})]);
        let events = r.drain();
        assert!(!events.iter().any(|e| matches!(e, SessionEvent::TurnEnded { .. })), "a state snapshot cannot release queued prompts or announce a completed turn");
        let a = fold(events);
        assert_eq!(a.view().main().busy(), busy);
        assert_eq!(a.view().main().status(), status);
    }
}
