//! Probes against the real `claude` CLI: ignored by default because they cost
//! money, need auth, and talk to a vendor service.
//!
//! Run deliberately, after changing anything about the wire:
//! `cargo test -p ferrite-core --test live_claude -- --ignored --nocapture`
//! Set FERRITE_CLAUDE_BIN to point at a specific install.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use ferrite_core::providers::{ClaudeConfig, ClaudeSession};
use ferrite_core::{SessionEvent, TurnOutcome};

/// Generous: a real turn crosses the network and may be rate limited.
const TURN_TIMEOUT: Duration = Duration::from_secs(180);

fn live_config() -> ClaudeConfig {
    ClaudeConfig {
        program: std::env::var("FERRITE_CLAUDE_BIN").unwrap_or_else(|_| "claude".into()),
        cwd: Some(std::env::temp_dir()),
        model: Some("haiku".into()),
    }
}

/// Collect until the turn ends, echoing the stream so a probe run is readable.
fn await_turn_end(events: &Receiver<SessionEvent>) -> (TurnOutcome, String) {
    let deadline = Instant::now() + TURN_TIMEOUT;
    let mut text = String::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(SessionEvent::TextDelta { text: delta }) => text.push_str(&delta),
            Ok(SessionEvent::TurnEnded { outcome, cost_usd }) => {
                println!("turn ended: {outcome:?} cost={cost_usd:?} text={text:?}");
                return (outcome, text);
            }
            Ok(SessionEvent::Closed { reason }) => panic!("session closed mid-turn: {reason}"),
            Ok(other) => println!("{other:?}"),
            Err(e) => panic!("no turn end within {TURN_TIMEOUT:?}: {e}"),
        }
    }
}

/// The claim the whole design rests on: one process, stdin held open, many
/// turns.
#[test]
#[ignore = "spawns the real claude CLI"]
fn a_session_serves_more_than_one_turn() {
    let mut session = ClaudeSession::spawn(live_config()).unwrap();

    session.send("Say exactly: one").unwrap();
    let (first, text) = await_turn_end(session.events());
    assert_eq!(first, TurnOutcome::Completed);
    assert!(text.contains("one"), "first turn said {text:?}");

    session.send("Say exactly: two").unwrap();
    let (second, text) = await_turn_end(session.events());
    assert_eq!(second, TurnOutcome::Completed);
    assert!(text.contains("two"), "second turn said {text:?}");
}

/// Interrupting mid-stream. The CLI reports this as an error result
/// (`is_error: true`, `subtype: "error_during_execution"`) and only
/// `terminal_reason: "aborted_streaming"` distinguishes it from a failure —
/// this test is what keeps that mapping honest.
#[test]
#[ignore = "spawns the real claude CLI"]
fn an_interrupted_turn_ends_as_interrupted() {
    let mut session = ClaudeSession::spawn(live_config()).unwrap();
    session
        .send("Count slowly from 1 to 200, one number per line. Do not stop early.")
        .unwrap();

    let deadline = Instant::now() + TURN_TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match session.events().recv_timeout(left) {
            Ok(SessionEvent::TextDelta { .. }) => break,
            Ok(SessionEvent::Closed { reason }) => panic!("session closed early: {reason}"),
            Ok(_) => {}
            Err(e) => panic!("no text within {TURN_TIMEOUT:?}: {e}"),
        }
    }
    session.interrupt().unwrap();

    let (outcome, _) = await_turn_end(session.events());
    assert_eq!(outcome, TurnOutcome::Interrupted);
}
