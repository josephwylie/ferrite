//! Claude Sessions driven against stub CLIs: no network, no real `claude`.
//!
//! The stubs are shell scripts, so these are Unix-only; the parser and version
//! pin they exercise are covered platform-independently by unit tests.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use ferrite_core::providers::{ClaudeConfig, ClaudeSession, SpawnError};
use ferrite_core::{SessionEvent, TurnOutcome};

const VERSION_CASE: &str = "case \"$1\" in --version) echo '2.1.243 (Claude Code)'; exit 0;; esac";

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude-hello-2.1.243.jsonl")
}

/// An executable stub `claude` in a per-process temp dir.
fn stub(name: &str, script: &str) -> String {
    let dir = std::env::temp_dir().join(format!("ferrite-claude-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path.display().to_string()
}

fn config(program: String) -> ClaudeConfig {
    ClaudeConfig {
        program,
        ..Default::default()
    }
}

/// Events up to and including the first turn end or close, or whatever
/// arrived before the deadline.
fn drain(events: &Receiver<SessionEvent>) -> Vec<SessionEvent> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut drained = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let Ok(event) = events.recv_timeout(left) else {
            return drained;
        };
        let last = matches!(
            event,
            SessionEvent::TurnEnded { .. } | SessionEvent::Closed { .. }
        );
        drained.push(event);
        if last {
            return drained;
        }
    }
}

fn spawn_failure(program: String) -> SpawnError {
    match ClaudeSession::spawn(config(program)) {
        Err(e) => e,
        Ok(_) => panic!("expected spawn to fail"),
    }
}

#[test]
fn a_missing_cli_is_named_in_the_error() {
    let program = "/nonexistent/ferrite/claude".to_string();
    match spawn_failure(program.clone()) {
        SpawnError::CliNotFound { program: named } => assert_eq!(named, program),
        other => panic!("expected CliNotFound, got {other:?}"),
    }
}

#[test]
fn a_cli_below_the_pin_is_refused_before_any_conversation() {
    match spawn_failure(stub("claude-old", "echo '2.1.223 (Claude Code)'")) {
        SpawnError::CliVersionUnmet { found, required } => {
            assert_eq!(found, "2.1.223");
            assert_eq!(required, "2.1.224");
        }
        other => panic!("expected CliVersionUnmet, got {other:?}"),
    }
}

/// The pin is a minimum, not an exclusive bound: the exact pinned release is
/// the one Ferrite is developed against and must spawn.
#[test]
fn a_cli_exactly_at_the_pin_is_accepted() {
    let program = stub(
        "claude-at-pin",
        "case \"$1\" in --version) echo '2.1.224 (Claude Code)'; exit 0;; esac\nexit 0",
    );
    ClaudeSession::spawn(config(program)).expect("the pinned version must spawn");
}

#[test]
fn an_unreadable_version_banner_is_its_own_error() {
    match spawn_failure(stub("claude-mute", "echo 'Claude Code'")) {
        SpawnError::VersionCheckFailed { detail } => assert!(
            detail.contains("Claude Code"),
            "detail should quote what the CLI printed: {detail}"
        ),
        other => panic!("expected VersionCheckFailed, got {other:?}"),
    }
}

#[test]
fn a_failing_version_check_is_not_mistaken_for_an_old_cli() {
    match spawn_failure(stub("claude-broken", "exit 7")) {
        SpawnError::VersionCheckFailed { detail } => assert!(
            detail.contains("exit status: 7"),
            "detail should carry the status: {detail}"
        ),
        other => panic!("expected VersionCheckFailed, got {other:?}"),
    }
}

/// The whole path: real process, real pipes, real reader thread — the same
/// stream the committed 2.1.243 capture describes.
#[test]
fn the_reader_thread_delivers_the_captured_stream() {
    let program = stub(
        "claude-stream",
        &format!(
            "{VERSION_CASE}\ncat '{}'\nexec sleep 30",
            fixture().display()
        ),
    );
    let session = ClaudeSession::spawn(config(program)).unwrap();
    let events = drain(session.events());

    assert_eq!(events.len(), 10, "unexpected stream: {events:?}");
    let inits: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Init { session_id, model } => Some((session_id, model)),
            _ => None,
        })
        .collect();
    assert_eq!(inits.len(), 1);
    assert!(!inits[0].0.is_empty());
    assert_eq!(inits[0].1, "claude-haiku-4-5-20251001");

    let text: String = events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "hello ferrite");
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::ThinkingDelta { .. })));
    assert_eq!(
        events.last(),
        Some(&SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(0.03798),
        })
    );

    // The stub is still alive and holding the pipe: dropping must kill it
    // rather than wait for it.
    let dropped_at = Instant::now();
    drop(session);
    assert!(
        dropped_at.elapsed() < Duration::from_secs(5),
        "drop hung for {:?}",
        dropped_at.elapsed()
    );
}

#[test]
fn stdout_eof_closes_the_session_with_the_exit_status() {
    let program = stub("claude-quits", &format!("{VERSION_CASE}\nexit 0"));
    let session = ClaudeSession::spawn(config(program)).unwrap();
    match drain(session.events()).as_slice() {
        [SessionEvent::Closed { reason }] => {
            assert!(reason.contains("exit status: 0"), "reason: {reason}")
        }
        other => panic!("expected a lone Closed, got {other:?}"),
    }
}

#[test]
fn an_abnormal_exit_explains_itself_with_stderr() {
    let program = stub(
        "claude-crashes",
        &format!("{VERSION_CASE}\necho 'fatal: no auth token' >&2\nexit 3"),
    );
    let session = ClaudeSession::spawn(config(program)).unwrap();
    match drain(session.events()).as_slice() {
        [SessionEvent::Closed { reason }] => {
            assert!(reason.contains("exit status: 3"), "reason: {reason}");
            assert!(reason.contains("fatal: no auth token"), "reason: {reason}");
        }
        other => panic!("expected a lone Closed, got {other:?}"),
    }
}

/// What the CLI is asked to be, and what it is told — recorded so a change to
/// either is deliberate.
#[test]
fn the_session_speaks_the_pinned_command_line_and_protocol() {
    let log = log_path("argv.log");
    let program = stub(
        "claude-echoes",
        &format!(
            "{VERSION_CASE}\necho \"$@\" > '{}'\ncat >> '{}'",
            log.display(),
            log.display()
        ),
    );
    let mut session = ClaudeSession::spawn(ClaudeConfig {
        program,
        cwd: Some(std::env::temp_dir()),
        model: Some("haiku".into()),
    })
    .unwrap();
    session.send("hi").unwrap();
    session.interrupt().unwrap();
    session.interrupt().unwrap();

    let recorded = read_lines(&log, 4);
    drop(session);
    let sent: Vec<serde_json::Value> = recorded[1..]
        .iter()
        .map(|line| serde_json::from_str(line).expect("every line is one JSON object"))
        .collect();

    assert_eq!(
        recorded[0],
        "-p --input-format stream-json --output-format stream-json \
         --include-partial-messages --verbose --model haiku"
    );
    assert_eq!(
        sent[0],
        serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": [{"type": "text", "text": "hi"}]},
        })
    );
    assert_eq!(
        sent[1],
        serde_json::json!({
            "type": "control_request",
            "request_id": "req_1",
            "request": {"subtype": "interrupt"},
        })
    );
    assert_eq!(sent[2]["request_id"], "req_2");
}

/// Ferrite must not smuggle in a model the operator did not choose: with no
/// override the CLI is left to its own configured default.
#[test]
fn no_model_is_passed_when_the_config_names_none() {
    let log = log_path("argv-default.log");
    let program = stub(
        "claude-echoes-default",
        &format!(
            "{VERSION_CASE}\necho \"$@\" > '{}'\nexec cat > /dev/null",
            log.display()
        ),
    );
    let session = ClaudeSession::spawn(config(program)).unwrap();
    let recorded = read_lines(&log, 1);
    drop(session);
    assert_eq!(
        recorded[0],
        "-p --input-format stream-json --output-format stream-json \
         --include-partial-messages --verbose"
    );
}

/// The Thread's workspace binding is exactly the CLI's working directory —
/// the agent has to be editing that checkout and no other.
#[test]
fn the_cli_runs_in_the_configured_workspace() {
    let workspace = std::env::temp_dir().join(format!("ferrite-workspace-{}", std::process::id()));
    fs::create_dir_all(&workspace).unwrap();
    let log = log_path("cwd.log");
    let program = stub(
        "claude-pwd",
        &format!(
            "{VERSION_CASE}\npwd -P > '{}'\nexec cat > /dev/null",
            log.display()
        ),
    );
    let session = ClaudeSession::spawn(ClaudeConfig {
        program,
        cwd: Some(workspace.clone()),
        model: None,
    })
    .unwrap();
    let recorded = read_lines(&log, 1);
    drop(session);
    assert_eq!(
        recorded[0],
        fs::canonicalize(&workspace).unwrap().display().to_string()
    );
}

/// A crash is explained by the end of stderr, not all of it: a chatty CLI must
/// not grow a Session's memory for as long as it runs.
#[test]
fn only_the_tail_of_a_noisy_stderr_is_kept() {
    let program = stub(
        "claude-noisy",
        &format!(
            "{VERSION_CASE}\ni=1\nwhile [ $i -le 100 ]; do echo \"noise $i\" >&2; i=$((i+1)); done\nexit 3"
        ),
    );
    let session = ClaudeSession::spawn(config(program)).unwrap();
    match drain(session.events()).as_slice() {
        [SessionEvent::Closed { reason }] => {
            let tail = reason.split_once("stderr: ").expect("reason: {reason}").1;
            assert_eq!(tail.lines().count(), 20, "unbounded stderr: {tail}");
            assert!(tail.contains("noise 100"), "newest line missing: {tail}");
            assert!(tail.contains("noise 81"), "tail is the wrong 20: {tail}");
            assert!(!tail.contains("noise 80"), "kept too much: {tail}");
        }
        other => panic!("expected a lone Closed, got {other:?}"),
    }
}

/// One mangled byte costs one line, never the Session: the CLI's stream is not
/// guaranteed to be well-formed UTF-8 and Ferrite keeps reading regardless.
#[test]
fn a_line_of_invalid_utf8_does_not_end_the_session() {
    let payload = log_path("mangled.jsonl");
    let mut bytes = vec![0xff, 0xfe, b'\n'];
    bytes.extend_from_slice(
        br#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"still here"}}}"#,
    );
    bytes.push(b'\n');
    bytes.extend_from_slice(br#"{"type":"result","is_error":false,"terminal_reason":"completed"}"#);
    bytes.push(b'\n');
    fs::write(&payload, &bytes).unwrap();

    let program = stub(
        "claude-mangles",
        &format!(
            "{VERSION_CASE}\ncat '{}'\nexec cat > /dev/null",
            payload.display()
        ),
    );
    let session = ClaudeSession::spawn(config(program)).unwrap();
    let events = drain(session.events());
    assert_eq!(
        events,
        vec![
            SessionEvent::TextDelta {
                text: "still here".into()
            },
            SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            },
        ]
    );
}

/// A Session that outruns its reader must stall the CLI, not lose its output:
/// the channel is bounded, so a slow frame is backpressure and never a hole in
/// the transcript.
#[test]
fn a_slow_consumer_stalls_the_cli_instead_of_losing_events() {
    const DELTAS: usize = 3000;

    let payload = log_path("flood.jsonl");
    let mut lines = String::new();
    for n in 0..DELTAS {
        lines.push_str(&format!(
            r#"{{"type":"stream_event","event":{{"type":"content_block_delta","delta":{{"type":"text_delta","text":"{n} "}}}}}}"#
        ));
        lines.push('\n');
    }
    lines.push_str(r#"{"type":"result","is_error":false,"terminal_reason":"completed"}"#);
    lines.push('\n');
    fs::write(&payload, &lines).unwrap();

    let program = stub(
        "claude-floods",
        &format!(
            "{VERSION_CASE}\ncat '{}'\nexec cat > /dev/null",
            payload.display()
        ),
    );
    let session = ClaudeSession::spawn(config(program)).unwrap();
    // Long enough for the reader to fill the channel and park on it.
    std::thread::sleep(Duration::from_millis(300));

    let events = drain(session.events());
    let text: String = events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let expected: String = (0..DELTAS).map(|n| format!("{n} ")).collect();
    assert_eq!(text.len(), expected.len(), "text was truncated");
    assert_eq!(text, expected, "deltas arrived out of order or with holes");
    assert!(matches!(
        events.last(),
        Some(SessionEvent::TurnEnded { .. })
    ));
}

fn log_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrite-claude-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// Wait for the stub to have written `wanted` lines; the CLI end of a pipe
/// gets there when it gets there.
fn read_lines(path: &std::path::Path, wanted: usize) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let lines: Vec<String> = fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect();
        if lines.len() >= wanted {
            return lines;
        }
        assert!(
            Instant::now() < deadline,
            "stub logged only {lines:?} of {wanted} lines"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
