#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use ferrite_core::SessionEvent;
use serde_json::json;
use support::*;

#[test]
fn live_model_catalog_paginates_before_publishing() {
    let r = Replay::new("codex", vec![]);
    r.drain();
    loop {
        let event = r
            .session
            .events()
            .recv_timeout(std::time::Duration::from_secs(3))
            .expect("model pagination must complete");
        if let SessionEvent::Models { models } = event {
            assert_eq!(
                models.iter().map(|m| m.value.as_str()).collect::<Vec<_>>(),
                ["native-first", "native-second"]
            );
            break;
        }
    }
}

#[test]
fn skill_invalidation_during_initial_read_reloads_and_clears_stale_commands() {
    let r = Replay::new(
        "codex",
        vec![json!({"method":"skills/changed","params":{}})],
    );
    r.drain();
    let mut initial = false;
    loop {
        let event = r
            .session
            .events()
            .recv_timeout(std::time::Duration::from_secs(3))
            .expect("skill invalidation must re-read native catalog");
        if let SessionEvent::Commands { commands } = event {
            if commands.is_empty() {
                assert!(initial);
                break;
            }
            assert_eq!(commands[0].name, "review");
            initial = true;
        }
    }
}
