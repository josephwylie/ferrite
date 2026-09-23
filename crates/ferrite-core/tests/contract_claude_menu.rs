//! The Claude `/` menu after the handshake: MCP prompts join once their
//! servers have settled, named the way they are typed.
#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use ferrite_core::SessionEvent;
use support::*;

#[test]
fn claude_mcp_prompts_join_the_menu_once_the_servers_settle() {
    let r = Replay::new("claude", vec![]);
    r.drain();
    // The settle: one status poll (the stub's one server is past pending),
    // then initialize again — on Ferrite's own ids, not the Session's.
    let status = r.wait_host(|v| v["request"]["subtype"] == "mcp_status");
    assert_eq!(status["request_id"], "ferrite_mcp_status_1");
    let again =
        r.wait_host(|v| v["request"]["subtype"] == "initialize" && v["request_id"] != "req_1");
    assert_eq!(again["request_id"], "ferrite_mcp_init_2");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let commands = loop {
        assert!(
            std::time::Instant::now() < deadline,
            "the refreshed menu must be announced"
        );
        if let Ok(SessionEvent::Commands { commands }) = r
            .session
            .events()
            .recv_timeout(std::time::Duration::from_millis(200))
        {
            break commands;
        }
    };
    let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["compact", "mcp__search__summarize"]);
    assert_eq!(
        commands[1].description, "search:summarize (MCP) · Summarize the results",
        "the CLI's display name leads the description"
    );
    assert!(
        commands.iter().all(|c| c.path.is_none()),
        "Claude commands are dispatched on their text"
    );
}
