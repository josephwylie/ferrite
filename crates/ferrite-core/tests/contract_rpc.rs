#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use ferrite_core::{SessionEvent, TurnOutcome};
use support::*;

#[test]
fn rejected_codex_start_settles_the_optimistic_turn_with_native_error() {
    let mut r = Replay::new("codex", vec![]);
    r.drain();
    r.session.send("reject-start").unwrap();
    loop {
        match r
            .session
            .events()
            .recv_timeout(std::time::Duration::from_secs(3))
            .expect("rejected native start must not leave the turn working forever")
        {
            SessionEvent::TurnEnded {
                outcome: TurnOutcome::Error(error),
                ..
            } => {
                assert_eq!(error, "Native start rejected");
                break;
            }
            SessionEvent::Closed { reason } => panic!("session unexpectedly closed: {reason}"),
            _ => {}
        }
    }
}

#[test]
fn codex_interrupt_uses_the_acknowledged_turn_when_notification_is_late() {
    let mut r = Replay::new("codex", vec![]);
    r.drain();
    r.session.send("accept-without-notification").unwrap();
    r.session.interrupt().unwrap();
    let request = r.wait_host(|v| v["method"] == "turn/interrupt");
    assert_eq!(request["params"]["threadId"], "root");
    assert_eq!(request["params"]["turnId"], "native-turn");
}
