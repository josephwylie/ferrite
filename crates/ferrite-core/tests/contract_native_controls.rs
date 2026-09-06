#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
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
    assert_eq!(request["request"]["detail"], "summary", "refresh must not initiate extra per-category token-count calls");
    let mut usage = false;
    let mut details = false;
    for _ in 0..8 {
        let e = r.session.events().recv_timeout(std::time::Duration::from_secs(3)).unwrap();
        match e {
            SessionEvent::TokenUsage { total_tokens:12000, context_window:Some(180000), .. } => usage = true,
            SessionEvent::ContextDetails { details: d } => {
                assert_eq!(d.usable_window, Some(150000));
                assert_eq!(d.auto_compact_threshold, Some(140000));
                assert_eq!(d.categories[0].name, "Messages");
                details = true;
            },
            _ => {},
        }
        if usage && details { return; }
    }
    panic!("native context must reach shared meter and details");
}

#[test]
fn mcp_status_is_normalized_for_the_shared_ui() {
    let mut r = Replay::new("claude", vec![]);
    r.drain();
    r.session.control(SessionControl::RefreshMcp).unwrap();
    for _ in 0..8 {
        let e = r.session.events().recv_timeout(std::time::Duration::from_secs(3)).unwrap();
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
        (SessionControl::StopTask { id:"task:1".into() }, json!({"subtype":"stop_task","task_id":"task:1"})),
        (SessionControl::BackgroundTasks, json!({"subtype":"background_tasks"})),
        (SessionControl::ReconnectMcp { server:"search".into() }, json!({"subtype":"mcp_reconnect","serverName":"search"})),
        (SessionControl::SetPermissionMode { mode:"plan".into() }, json!({"subtype":"set_permission_mode","mode":"plan"})),
    ] {
        r.session.control(control).unwrap();
        let request = r.wait_host(|v| v["request"]["subtype"] == expected["subtype"]);
        assert_eq!(request["request"], expected);
    }
}

#[test]
fn reconnect_never_displays_previous_sessions_live_mcp_status() {
    let mut t=ferrite_core::transcript::Transcript::default();
    t.apply(ferrite_core::transcript::Input::Event(SessionEvent::McpServers {servers:vec![ferrite_core::McpServer{name:"stale".into(),status:ferrite_core::McpStatus::Connected,error:None}]}));
    t.apply(ferrite_core::transcript::Input::Revived);
    assert!(t.mcp_servers().is_empty(),"live connection state cannot be inherited by a new Session");
}
