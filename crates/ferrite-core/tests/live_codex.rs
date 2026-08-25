//! Probes against the real `codex` CLI: ignored by default because they cost
//! money, need auth, and talk to a vendor service.
//!
//! Run deliberately, after changing anything about the wire:
//! `cargo test -p ferrite-core --test live_codex -- --ignored --nocapture`
//! Set FERRITE_CODEX_BIN to point at a specific install.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use ferrite_core::providers::{CodexConfig, CodexSession};
use ferrite_core::{Decision, DecisionAnswer, SessionEvent, TurnOutcome};

/// Generous: a real turn crosses the network and may be rate limited.
const TURN_TIMEOUT: Duration = Duration::from_secs(180);

fn live_config() -> CodexConfig {
    CodexConfig {
        program: std::env::var("FERRITE_CODEX_BIN").unwrap_or_else(|_| "codex".into()),
        cwd: Some(std::env::temp_dir()),
        model: Some("gpt-5.4-mini".into()),
        approval_policy: Some("never".into()),
        sandbox: Some("read-only".into()),
        resume: None,
    }
}

/// A Session that will actually ask before it acts. Without pinning the
/// posture these probes pass vacuously on a machine configured to never ask.
fn gated_config() -> CodexConfig {
    CodexConfig {
        approval_policy: Some("on-request".into()),
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
#[ignore = "spawns the real codex CLI"]
fn a_session_serves_more_than_one_turn() {
    let mut session = CodexSession::spawn(live_config()).unwrap();

    session.send("Say exactly: one").unwrap();
    let (first, text) = await_turn_end(session.events());
    assert_eq!(first, TurnOutcome::Completed);
    assert!(text.contains("one"), "first turn said {text:?}");

    session.send("Say exactly: two").unwrap();
    let (second, text) = await_turn_end(session.events());
    assert_eq!(second, TurnOutcome::Completed);
    assert!(text.contains("two"), "second turn said {text:?}");
}

/// Interrupting mid-stream: the server completes the turn with status
/// "interrupted" — this test is what keeps that mapping honest.
#[test]
#[ignore = "spawns the real codex CLI"]
fn an_interrupted_turn_ends_as_interrupted() {
    let mut session = CodexSession::spawn(live_config()).unwrap();
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

/// Feature detection has to work against the real server, not only against a
/// replayed capture: this is what proves the two-step handshake fits inside
/// spawn.
#[test]
#[ignore = "spawns the real codex CLI"]
fn the_real_server_answers_the_capability_handshake() {
    let session = CodexSession::spawn(live_config()).unwrap();
    let capabilities = session.capabilities();
    println!("{capabilities:?}");

    assert_eq!(capabilities.approval_policy, "never");
    assert_eq!(capabilities.sandbox, "readOnly");
    assert_eq!(capabilities.model, "gpt-5.4-mini");
}

/// Drive one turn, answering the first Decision it raises. Returns how the
/// turn ended and every tool result it produced.
fn turn_answering(
    session: &mut CodexSession,
    prompt: &str,
    answer: fn() -> DecisionAnswer,
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
                session.respond_to_decision(&id.clone(), answer()).unwrap();
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

/// The allow and deny probes run in parallel in one temp dir, so each works
/// on its own file — a shared one would let the allowed write satisfy (or
/// spoil) the denied probe's assertions.
fn write_prompt(name: &str) -> String {
    format!("Create a file named {name} containing exactly the word ok. Then say done.")
}

fn live_artifact(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(name)
}

/// The decision Ferrite sends for "allow" has to be the one the server acts
/// on: after this, the tool has really run.
#[test]
#[ignore = "spawns the real codex CLI"]
fn allowing_a_decision_runs_the_tool() {
    let artifact = live_artifact("ferrite-live-allow.txt");
    let _ = std::fs::remove_file(&artifact);
    let mut session = CodexSession::spawn(gated_config()).unwrap();
    assert_ne!(
        session.capabilities().approval_policy,
        "never",
        "a Session that never asks cannot prove anything about Decisions"
    );

    let (outcome, tools) = turn_answering(
        &mut session,
        &write_prompt("ferrite-live-allow.txt"),
        || DecisionAnswer::Allow {
            input: serde_json::Value::Null,
        },
    );

    assert_eq!(outcome, TurnOutcome::Completed);
    assert!(
        tools.iter().any(|(_, is_error)| !is_error),
        "no tool succeeded: {tools:?}"
    );
    assert!(artifact.exists(), "allow did not actually run the tool");
    let _ = std::fs::remove_file(&artifact);
}

/// Declining must refuse the tool without killing the Session. Codex's
/// decline carries no message, so (observed live) the model may retry the
/// rejected write indefinitely — the probe cannot wait for the model to give
/// up. Instead it asserts the wire facts one at a time: the declined tool
/// comes back as a failed completion, the still-running turn interrupts
/// cleanly, and the Session then serves a fresh turn.
#[test]
#[ignore = "spawns the real codex CLI"]
fn denying_a_decision_leaves_the_session_running() {
    let artifact = live_artifact("ferrite-live-deny.txt");
    let _ = std::fs::remove_file(&artifact);
    let mut session = CodexSession::spawn(gated_config()).unwrap();
    session
        .send(&write_prompt("ferrite-live-deny.txt"))
        .unwrap();

    let deadline = Instant::now() + TURN_TIMEOUT;
    let mut declined = false;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match session.events().recv_timeout(left) {
            Ok(SessionEvent::DecisionRequested {
                decision: Decision { id, .. },
            }) if !declined => {
                declined = true;
                session
                    .respond_to_decision(
                        &id,
                        DecisionAnswer::Deny {
                            message: "Ferrite operator denied this tool".into(),
                        },
                    )
                    .unwrap();
            }
            Ok(SessionEvent::ToolCompleted { is_error, .. }) if declined => {
                assert!(is_error, "a declined tool must complete as a failure");
                break;
            }
            Ok(SessionEvent::Closed { reason }) => panic!("session closed mid-turn: {reason}"),
            Ok(_) => {}
            Err(e) => panic!("no declined completion within {TURN_TIMEOUT:?}: {e}"),
        }
    }
    assert!(!artifact.exists(), "deny still let the tool run");

    // End the model's retry loop ourselves, then prove the Session outlived
    // the decline.
    session.interrupt().unwrap();
    let (outcome, _) = await_turn_end(session.events());
    assert_eq!(outcome, TurnOutcome::Interrupted);

    session.send("Say exactly: alive").unwrap();
    let (outcome, text) = await_turn_end(session.events());
    assert_eq!(outcome, TurnOutcome::Completed);
    assert!(text.contains("alive"), "the next turn said {text:?}");
}

/// Resume across processes, for real: a Session plants a codeword and dies;
/// a second Session resumes the same thread id and the model answers from a
/// conversation the new process never had.
#[test]
#[ignore = "spawns the real codex CLI"]
fn a_parked_thread_resumes_across_processes() {
    let mut session = CodexSession::spawn(live_config()).unwrap();
    let thread_id = loop {
        match session.events().recv_timeout(TURN_TIMEOUT) {
            Ok(SessionEvent::Init { session_id, .. }) => break session_id,
            Ok(_) => {}
            Err(e) => panic!("no Init: {e}"),
        }
    };
    session
        .send("Remember the codeword: ferrite-live-resume. Reply with exactly: saved")
        .unwrap();
    let (outcome, _) = await_turn_end(session.events());
    assert_eq!(outcome, TurnOutcome::Completed);
    drop(session);

    let mut revived = CodexSession::spawn(CodexConfig {
        resume: Some(thread_id),
        ..live_config()
    })
    .unwrap();
    revived
        .send("What is the codeword? Reply with the codeword only.")
        .unwrap();
    let (outcome, text) = await_turn_end(revived.events());
    assert_eq!(outcome, TurnOutcome::Completed);
    assert!(
        text.contains("ferrite-live-resume"),
        "the resumed thread forgot: {text:?}"
    );
}
