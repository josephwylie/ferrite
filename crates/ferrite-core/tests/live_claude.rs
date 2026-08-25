//! Probes against the real `claude` CLI: ignored by default because they cost
//! money, need auth, and talk to a vendor service.
//!
//! Run deliberately, after changing anything about the wire:
//! `cargo test -p ferrite-core --test live_claude -- --ignored --nocapture`
//! Set FERRITE_CLAUDE_BIN to point at a specific install.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use ferrite_core::providers::{ClaudeConfig, ClaudeSession};
use ferrite_core::{Decision, DecisionAnswer, SessionEvent, TurnOutcome};

/// Generous: a real turn crosses the network and may be rate limited.
const TURN_TIMEOUT: Duration = Duration::from_secs(180);

fn live_config() -> ClaudeConfig {
    ClaudeConfig {
        program: std::env::var("FERRITE_CLAUDE_BIN").unwrap_or_else(|_| "claude".into()),
        cwd: Some(std::env::temp_dir()),
        model: Some("haiku".into()),
        permission_mode: None,
    }
}

/// A Session that will actually ask before it acts. Without pinning the mode
/// these probes pass vacuously on a machine configured to bypass permissions.
fn gated_config() -> ClaudeConfig {
    ClaudeConfig {
        permission_mode: Some("default".into()),
        ..live_config()
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

/// Feature detection has to work against the real CLI, not only against a
/// replayed capture: this is what proves the handshake fits inside spawn.
#[test]
#[ignore = "spawns the real claude CLI"]
fn the_real_cli_answers_the_capability_handshake() {
    let session = ClaudeSession::spawn(live_config()).unwrap();
    let capabilities = session.capabilities();
    println!("{capabilities:?}");

    assert!(
        !capabilities.permission_mode.is_empty(),
        "the handshake did not report a permission mode"
    );
    assert!(
        capabilities.models.iter().any(|model| model == "haiku"),
        "models: {:?}",
        capabilities.models
    );
}

/// Drive one turn, answering the first Decision it raises. Returns how the turn
/// ended and every tool result it produced.
fn turn_answering(
    session: &mut ClaudeSession,
    prompt: &str,
    answer: fn(&SessionEvent) -> DecisionAnswer,
) -> (TurnOutcome, Vec<(String, bool)>) {
    session.send(prompt).unwrap();
    let deadline = Instant::now() + TURN_TIMEOUT;
    let mut tools = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match session.events().recv_timeout(left) {
            Ok(event @ SessionEvent::DecisionRequested { .. }) => {
                println!("{event:?}");
                let SessionEvent::DecisionRequested {
                    decision: Decision { id, .. },
                } = &event
                else {
                    unreachable!()
                };
                session
                    .respond_to_decision(&id.clone(), answer(&event))
                    .unwrap();
            }
            Ok(SessionEvent::ToolCompleted {
                output, is_error, ..
            }) => tools.push((output, is_error)),
            Ok(SessionEvent::TurnEnded { outcome, .. }) => return (outcome, tools),
            Ok(SessionEvent::Closed { reason }) => panic!("session closed mid-turn: {reason}"),
            Ok(_) => {}
            Err(e) => panic!("no turn end within {TURN_TIMEOUT:?}: {e}"),
        }
    }
}

const WRITE_PROMPT: &str =
    "Create a file named ferrite-live-perm.txt containing exactly the word ok, \
     using the Write tool. Then say done.";

fn live_artifact() -> std::path::PathBuf {
    std::env::temp_dir().join("ferrite-live-perm.txt")
}

/// The control response Ferrite sends for "allow" has to be the one the CLI
/// acts on: after this, the tool has really run.
#[test]
#[ignore = "spawns the real claude CLI"]
fn allowing_a_decision_runs_the_tool() {
    let _ = std::fs::remove_file(live_artifact());
    let mut session = ClaudeSession::spawn(gated_config()).unwrap();
    assert_ne!(
        session.capabilities().permission_mode,
        "bypassPermissions",
        "a Session that never asks cannot prove anything about Decisions"
    );

    let (outcome, tools) = turn_answering(&mut session, WRITE_PROMPT, |event| {
        let SessionEvent::DecisionRequested {
            decision: Decision { input, .. },
        } = event
        else {
            unreachable!()
        };
        DecisionAnswer::Allow {
            input: input.clone(),
        }
    });

    assert_eq!(outcome, TurnOutcome::Completed);
    assert!(
        tools.iter().any(|(_, is_error)| !is_error),
        "no tool succeeded: {tools:?}"
    );
    assert!(
        live_artifact().exists(),
        "allow did not actually run the tool"
    );
    let _ = std::fs::remove_file(live_artifact());
}

/// Denying must refuse the tool without killing the Session: the model
/// gets the operator's reason back and finishes the turn talking about it.
#[test]
#[ignore = "spawns the real claude CLI"]
fn denying_a_decision_leaves_the_turn_running() {
    let _ = std::fs::remove_file(live_artifact());
    let mut session = ClaudeSession::spawn(gated_config()).unwrap();

    let (outcome, tools) = turn_answering(&mut session, WRITE_PROMPT, |_| DecisionAnswer::Deny {
        message: "Ferrite operator denied this tool".into(),
    });

    assert_eq!(outcome, TurnOutcome::Completed, "the turn should survive");
    assert!(
        tools
            .iter()
            .any(|(output, is_error)| *is_error && output.contains("Ferrite operator denied")),
        "the denial did not reach the model: {tools:?}"
    );
    assert!(!live_artifact().exists(), "deny still let the tool run");
}
