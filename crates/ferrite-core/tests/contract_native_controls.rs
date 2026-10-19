#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use ferrite_core::activity::ActivityEvent;
use ferrite_core::{ControlKind, SessionControl, SessionEvent};
use serde_json::json;
use support::*;

#[test]
fn context_refresh_uses_native_summary_control_and_updates_shared_usage() {
    let mut r = Replay::new("claude", vec![]);
    r.drain();
    assert!(r.session.supports_control(ControlKind::RefreshContext));
    r.session.control(SessionControl::RefreshContext).unwrap();
    let request = r.wait_host(|v| v["request"]["subtype"] == "get_context_usage");
    assert_eq!(
        request["request"]["detail"], "summary",
        "refresh must not initiate extra per-category token-count calls"
    );
    let mut usage = false;
    let mut details = false;
    for _ in 0..8 {
        let e = r
            .session
            .events()
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        match e {
            SessionEvent::ContextUsage {
                total_tokens: 12000,
                context_window: Some(180000),
            } => usage = true,
            SessionEvent::ContextDetails { details: d } => {
                assert_eq!(d.usable_window, Some(150000));
                assert_eq!(d.auto_compact_threshold, Some(140000));
                assert_eq!(d.categories[0].name, "Messages");
                details = true;
            }
            _ => {}
        }
        if usage && details {
            return;
        }
    }
    panic!("native context must reach shared meter and details");
}

#[test]
fn mcp_status_is_normalized_for_the_shared_ui() {
    let mut r = Replay::new("claude", vec![]);
    r.drain();
    r.session.control(SessionControl::RefreshMcp).unwrap();
    for _ in 0..8 {
        let e = r
            .session
            .events()
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        if let SessionEvent::McpServers { servers } = e {
            assert_eq!(servers.len(), 1);
            assert_eq!(servers[0].name, "search");
            assert_eq!(servers[0].status, ferrite_core::McpStatus::NeedsAuth);
            assert_eq!(servers[0].error.as_deref(), Some("Sign in to search"));
            return;
        }
    }
    panic!("native MCP state was not exposed");
}

#[test]
fn native_task_mcp_and_permission_controls_keep_exact_wire_handles() {
    let mut r = Replay::new("claude", vec![]);
    r.drain();
    for (control, expected) in [
        (
            SessionControl::StopTask {
                id: "task:1".into(),
            },
            json!({"subtype":"stop_task","task_id":"task:1"}),
        ),
        (
            SessionControl::BackgroundTasks,
            json!({"subtype":"background_tasks"}),
        ),
        (
            SessionControl::ReconnectMcp {
                server: "search".into(),
            },
            json!({"subtype":"mcp_reconnect","serverName":"search"}),
        ),
        (
            SessionControl::SetPermissionMode {
                mode: "plan".into(),
            },
            json!({"subtype":"set_permission_mode","mode":"plan"}),
        ),
    ] {
        r.session.control(control).unwrap();
        let request = r.wait_host(|v| v["request"]["subtype"] == expected["subtype"]);
        assert_eq!(request["request"], expected);
    }
}

#[test]
fn reconnect_never_displays_previous_sessions_live_mcp_status() {
    let mut t = ferrite_core::transcript::Transcript::default();
    t.apply(ferrite_core::transcript::Input::Event(
        SessionEvent::McpServers {
            servers: vec![ferrite_core::McpServer {
                name: "stale".into(),
                status: ferrite_core::McpStatus::Connected,
                error: None,
            }],
        },
    ));
    t.apply(ferrite_core::transcript::Input::Revived);
    assert!(
        t.mcp_servers().is_empty(),
        "live connection state cannot be inherited by a new Session"
    );
}

#[test]
fn adapters_supply_permission_choices_for_the_shared_ui() {
    for (provider, expected) in [
        // Bypass only for a Session launched in it: the CLI refuses to
        // switch into it otherwise.
        (
            "claude",
            vec!["default", "acceptEdits", "plan", "dontAsk", "auto"],
        ),
        ("codex", vec!["untrusted", "on-request", "never"]),
    ] {
        let r = Replay::new(provider, vec![]);
        r.drain();
        let choices = r.session.permission_modes();
        assert_eq!(
            choices.iter().map(|c| c.value.as_str()).collect::<Vec<_>>(),
            expected
        );
        assert!(choices.iter().all(|c| !c.label.is_empty()));
    }
}

/// Without `perTaskStopAffordance` the CLI's interrupt kills every
/// background agent and workflow along with Main's turn (2.1.299).
#[test]
fn claude_handshake_declares_per_task_stops_so_escape_spares_background_agents() {
    let mut r = Replay::new("claude", vec![]);
    r.drain();
    let initialize = r.wait_host(|v| v["request"]["subtype"] == "initialize");
    assert_eq!(initialize["request"]["perTaskStopAffordance"], true);
    r.session.interrupt().unwrap();
    let interrupt = r.wait_host(|v| v["request"]["subtype"] == "interrupt");
    assert_eq!(interrupt["request"], json!({"subtype": "interrupt"}));
}

#[test]
fn stopping_a_claude_subagent_stops_its_task_alone() {
    let mut r = Replay::new(
        "claude",
        vec![
            json!({"type":"assistant","message":{"id":"m1","role":"assistant","content":[
                {"type":"tool_use","id":"spawn","name":"Agent","input":{"prompt":"wait","run_in_background":true}}]}}),
            json!({"type":"system","subtype":"task_started","task_id":"agent-task","tool_use_id":"spawn","task_type":"local_agent","description":"wait"}),
            json!({"type":"system","subtype":"task_started","task_id":"shell-task","tool_use_id":"shell","task_type":"local_bash","is_backgrounded":true,"description":"sleep"}),
        ],
    );
    let key = r
        .drain()
        .into_iter()
        .find_map(|event| match event {
            SessionEvent::Activity(ActivityEvent::Discovered(info)) => Some(info.key),
            _ => None,
        })
        .expect("the background agent is a Subagent");
    r.session.stop_subagent(&key).unwrap();
    let stop = r.wait_host(|v| v["request"]["subtype"] == "stop_task");
    assert_eq!(
        stop["request"],
        json!({"subtype": "stop_task", "task_id": "agent-task"})
    );
    assert!(
        r.session
            .stop_subagent(&ferrite_core::activity::AgentKey::from_stored(
                "unknown".into()
            ))
            .is_err(),
        "a Subagent with no task is never guessed at"
    );
}
