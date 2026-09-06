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
