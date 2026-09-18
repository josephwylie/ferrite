#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use ferrite_core::{ControlKind, McpStatus, SessionControl, SessionEvent};
use support::*;

#[test]
fn codex_mcp_status_refresh_uses_native_thread_scope() {
    let mut r = Replay::new("codex", vec![]);
    r.drain();
    assert!(r.session.supports_control(ControlKind::RefreshMcp));
    assert!(
        !r.session.supports_control(ControlKind::ReconnectMcp),
        "config reload is not per-server reconnect"
    );
    r.session.control(SessionControl::RefreshMcp).unwrap();
    let request = r.wait_host(|v| v["method"] == "mcpServerStatus/list");
    assert_eq!(request["params"]["threadId"], "root");
    loop {
        if let SessionEvent::McpServers { servers } = r
            .session
            .events()
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap()
        {
            assert_eq!(servers[0].name, "search");
            assert_eq!(servers[0].status, McpStatus::NeedsAuth);
            break;
        }
    }
}

#[test]
fn codex_mcp_login_provides_a_shared_url_without_opening_it() {
    let mut r = Replay::new("codex", vec![]);
    r.drain();
    r.session
        .control(SessionControl::LoginMcp {
            server: "search".into(),
        })
        .unwrap();
    let request = r.wait_host(|v| v["method"] == "mcpServer/oauth/login");
    assert_eq!(request["params"]["name"], "search");
    assert_eq!(request["params"]["threadId"], "root");
    loop {
        if let SessionEvent::McpAuthorization { server, url } = r
            .session
            .events()
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap()
        {
            assert_eq!(server, "search");
            assert_eq!(url.as_deref(), Some("https://example.com/authorize"));
            break;
        }
    }
}

#[test]
fn codex_native_permission_policy_and_reload_controls_are_distinct() {
    let mut r = Replay::new("codex", vec![]);
    r.drain();
    r.session
        .control(SessionControl::SetPermissionMode {
            mode: "on-request".into(),
        })
        .unwrap();
    let request = r.wait_host(|v| v["method"] == "thread/settings/update");
    assert_eq!(
        request["params"],
        serde_json::json!({"threadId":"root","approvalPolicy":"on-request"})
    );
    let init = r.wait_host(|v| v["method"] == "initialize");
    assert_eq!(
        init["params"]["capabilities"]["experimentalApi"], true,
        "native settings API requires the experimental opt-in"
    );
    r.session.control(SessionControl::ReloadMcp).unwrap();
    let reload = r.wait_host(|v| v["method"] == "config/mcpServer/reload");
    assert!(reload.get("params").is_none());
}

#[test]
fn codex_stop_task_terminates_the_background_terminal_by_process_id() {
    let mut r = Replay::new("codex", vec![]);
    r.drain();
    assert!(r.session.supports_control(ControlKind::StopTask));
    assert!(
        !r.session.supports_control(ControlKind::BackgroundTasks),
        "Codex has no verb for sending everything to the background"
    );
    r.session
        .control(SessionControl::StopTask { id: "63014".into() })
        .unwrap();
    let request = r.wait_host(|v| v["method"] == "thread/backgroundTerminals/terminate");
    assert_eq!(
        request["params"],
        serde_json::json!({"threadId":"root","processId":"63014"})
    );
    // A process the server no longer has is said so, as a notice — never
    // as an error that ends the turn.
    r.session
        .control(SessionControl::StopTask { id: "gone".into() })
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut noticed = false;
    while std::time::Instant::now() < deadline && !noticed {
        if let Ok(SessionEvent::Activity(ferrite_core::activity::ActivityEvent::MainContent {
            event: ferrite_core::activity::ExecutionEvent::Notice { text },
            ..
        })) = r
            .session
            .events()
            .recv_timeout(std::time::Duration::from_millis(200))
        {
            noticed = text.contains("gone");
        }
    }
    assert!(noticed, "a missing process is reported as a notice");
}

#[test]
fn codex_background_terminals_are_read_off_the_item_stream() {
    use ferrite_core::progress::{BackgroundTask, ProgressEvent, TaskStatus};
    use serde_json::json;
    let exec = |status: &str, exit: serde_json::Value| {
        json!({
            "type": "commandExecution",
            "id": "exec-1",
            "command": "/bin/zsh -lc 'sleep 600'",
            "cwd": "/workspace",
            "processId": "63014",
            "source": "unifiedExecStartup",
            "status": status,
            "commandActions": [{"type": "unknown", "command": "sleep 600"}],
            "aggregatedOutput": null,
            "exitCode": exit,
            "durationMs": null
        })
    };
    let r = Replay::new(
        "codex",
        vec![
            json!({"method":"turn/started","params":{"threadId":"root","turn":{"id":"t1","status":"inProgress"}}}),
            json!({"method":"item/started","params":{"threadId":"root","turnId":"t1","item":exec("inProgress", json!(null))}}),
            // The model moved on while the process runs: that is what
            // "backgrounded" means for Codex.
            json!({"method":"item/started","params":{"threadId":"root","turnId":"t1","item":{"type":"agentMessage","id":"msg-1","text":"started"}}}),
            json!({"method":"item/completed","params":{"threadId":"root","turnId":"t1","item":{"type":"agentMessage","id":"msg-1","text":"started"}}}),
            json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"t1","status":"completed"}}}),
            // The process ended (here: terminated) after the turn.
            json!({"method":"item/completed","params":{"threadId":"root","turnId":"t1","item":exec("failed", json!(-1))}}),
        ],
    );
    let snapshots: Vec<Vec<BackgroundTask>> = r
        .drain()
        .into_iter()
        .filter_map(|event| match event {
            SessionEvent::Progress {
                event: ProgressEvent::BackgroundSnapshot { tasks },
            } => Some(tasks),
            _ => None,
        })
        .collect();
    assert_eq!(
        snapshots,
        vec![
            vec![BackgroundTask {
                id: "63014".into(),
                label: "sleep 600".into(),
                status: TaskStatus::Working,
                detail: "shell".into(),
            }],
            vec![],
        ],
        "one snapshot when the terminal is backgrounded, one when it ends"
    );
}
