#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use serde_json::json;
use support::*;
#[test]
fn claude_renames_the_live_native_session() {
    let mut r = Replay::new("claude", vec![]);
    r.drain();
    r.session.set_name("Provider parity").unwrap();
    let frame = r.wait_host(|v| {
        v["type"] == "control_request" && v["request"]["subtype"] == "rename_session"
    });
    assert_eq!(
        frame["request"],
        json!({"subtype":"rename_session","title":"Provider parity"})
    );
}

#[test]
fn codex_model_change_applies_to_the_next_turn_on_the_same_thread() {
    let mut r = Replay::new("codex", vec![]);
    r.drain();
    let pid = r.session.pid();
    r.session.set_model(Some("next-model")).unwrap();
    r.session.send("Hello").unwrap();
    let request = r.wait_host(|v| v["method"] == "turn/start");
    assert_eq!(request["params"]["model"], "next-model");
    assert_eq!(request["params"]["threadId"], "root");
    assert_eq!(r.session.pid(), pid);
}
#[test]
fn claude_model_change_waits_for_native_acceptance_and_reports_rejection() {
    let mut r = Replay::new("claude", vec![]);
    r.drain();
    let pid = r.session.pid();
    r.session.set_model(Some("next-model")).unwrap();
    let request = r.wait_host(|v| v["request"]["subtype"] == "set_model");
    assert_eq!(request["request"]["model"], "next-model");
    assert!(r
        .session
        .set_model(Some("rejected-model"))
        .unwrap_err()
        .to_string()
        .contains("model unavailable"));
    assert_eq!(r.session.pid(), pid);
}
