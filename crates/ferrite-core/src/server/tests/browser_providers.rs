//! Real provider protocol adapters into the public Conversation browser channel.
use super::*;
use crate::providers::{ClaudeConfig, ClaudeSession};
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;

fn stub(dir: &Path, script: &str) -> String {
    let path = dir.join("provider");
    std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.display().to_string()
}

fn lines_until(path: &Path, done: impl Fn(&Value) -> bool) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let lines: Vec<Value> = std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        if lines.iter().any(&done) {
            return lines;
        }
        assert!(Instant::now() < deadline, "provider never answered: {lines:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn claude_browser_opens_the_bound_threads_tab_with_visuals_off() {
    let harness = start("claude-browser-provider");
    let mut window = attach(&harness.address);
    let (thread, _provider) = open(&mut window, &harness);
    let _browser_window = browser_test_window(&harness, thread);
    let host = harness.dir.join("claude-host.jsonl");
    let call = json!({"type":"control_request","request_id":"browser-open","request":{
        "subtype":"mcp_message","server_name":"ferrite","message":{
            "jsonrpc":"2.0","id":8,"method":"tools/call","params":{
                "name":"browser","arguments":{"kind":"action","action":{
                    "kind":"open","url":"https://example.com/provider-owned"
                }}
            }
        }
    }});
    let program = stub(&harness.dir, &format!(
        "case \"$1\" in --version) echo '2.1.292 (Claude Code)'; exit 0;; esac\n\
         echo '{{\"type\":\"control_response\",\"response\":{{\"subtype\":\"success\",\"request_id\":\"req_1\",\"response\":{{}}}}}}'\n\
         echo '{call}'\nexec cat > '{}'", host.display()));
    let session = ClaudeSession::spawn(ClaudeConfig {
        program,
        browser: Some(harness.browser.for_thread(thread)),
        ..Default::default()
    }).unwrap();
    let lines = lines_until(&host, |line| line["response"]["request_id"] == "browser-open");
    let answer = lines.iter().find(|line| line["response"]["request_id"] == "browser-open").unwrap();
    let result = &answer["response"]["response"]["mcp_response"]["result"];
    assert_eq!(result["isError"], false, "{answer}");
    let value: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(value["state"]["tabs"][0]["url"], "https://example.com/provider-owned");
    drop(session);
    window.command(99, Op::Stop { when: When::Now });
    assert_eq!(harness.server.join().unwrap(), 0);
}
