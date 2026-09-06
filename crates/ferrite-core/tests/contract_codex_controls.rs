#![cfg(unix)]
#[allow(dead_code)]
#[path="support/provider_contract.rs"]mod support;
use support::*;
use ferrite_core::{SessionEvent,SessionControl,ControlKind,McpStatus};

#[test]
fn codex_mcp_status_refresh_uses_native_thread_scope() {
    let mut r=Replay::new("codex",vec![]);r.drain();
    assert!(r.session.supports_control(ControlKind::RefreshMcp));
    assert!(!r.session.supports_control(ControlKind::ReconnectMcp),"config reload is not per-server reconnect");
    r.session.control(SessionControl::RefreshMcp).unwrap();
    let request=r.wait_host(|v|v["method"]=="mcpServerStatus/list");assert_eq!(request["params"]["threadId"],"root");
    loop {if let SessionEvent::McpServers{servers}=r.session.events().recv_timeout(std::time::Duration::from_secs(3)).unwrap(){assert_eq!(servers[0].name,"search");assert_eq!(servers[0].status,McpStatus::NeedsAuth);break;}}
}

#[test]
fn codex_mcp_login_provides_a_shared_url_without_opening_it() {
    let mut r=Replay::new("codex",vec![]);r.drain();
    r.session.control(SessionControl::LoginMcp{server:"search".into()}).unwrap();
    let request=r.wait_host(|v|v["method"]=="mcpServer/oauth/login");assert_eq!(request["params"]["name"],"search");assert_eq!(request["params"]["threadId"],"root");
    loop {if let SessionEvent::McpAuthorization{server,url}=r.session.events().recv_timeout(std::time::Duration::from_secs(3)).unwrap(){assert_eq!(server,"search");assert_eq!(url.as_deref(),Some("https://example.com/authorize"));break;}}
}

#[test]
fn codex_native_permission_policy_and_reload_controls_are_distinct() {
    let mut r=Replay::new("codex",vec![]);r.drain();
    r.session.control(SessionControl::SetPermissionMode{mode:"on-request".into()}).unwrap();
    let request=r.wait_host(|v|v["method"]=="thread/settings/update");assert_eq!(request["params"],serde_json::json!({"threadId":"root","approvalPolicy":"on-request"}));
    let init=r.wait_host(|v|v["method"]=="initialize");assert_eq!(init["params"]["capabilities"]["experimentalApi"],true,"native settings API requires the experimental opt-in");
    r.session.control(SessionControl::ReloadMcp).unwrap();
    let reload=r.wait_host(|v|v["method"]=="config/mcpServer/reload");assert!(reload.get("params").is_none());
}
