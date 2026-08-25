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

use serde_json::Value;

use ferrite_core::providers::{Capabilities, ClaudeConfig, ClaudeSession, SpawnError};
use ferrite_core::{DecisionAnswer, SessionEvent, TurnOutcome};

const VERSION_CASE: &str = "case \"$1\" in --version) echo '2.1.243 (Claude Code)'; exit 0;; esac";

/// What every stub has to do before it can pretend to be the CLI: answer
/// `--version`, then answer spawn's initialize control request the way a real
/// CLI does. A stub that stayed silent would make its test wait out the
/// handshake timeout — and would be lying about the protocol. Stubs that are
/// *about* the handshake build on `VERSION_CASE` instead and answer it
/// themselves.
const PRELUDE: &str = concat!(
    "case \"$1\" in --version) echo '2.1.243 (Claude Code)'; exit 0;; esac\n",
    r#"echo '{"type":"control_response","response":{"subtype":"success","request_id":"req_1","response":{}}}'"#,
);

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/claude-{name}.jsonl"))
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

/// The whole path for one committed capture: real process, real pipes, real
/// reader thread. Every fixture earns its keep here, not only in the parser's
/// own tests.
fn replay(name: &str) -> Vec<SessionEvent> {
    let program = stub(
        &format!("claude-replay-{name}"),
        &format!(
            "{PRELUDE}\ncat '{}'\nexec cat > /dev/null",
            fixture(name).display()
        ),
    );
    let session = ClaudeSession::spawn(config(program)).unwrap();
    drain(session.events())
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
fn a_cli_below_the_pin_is_refused_before_any_session() {
    match spawn_failure(stub("claude-old", "echo '2.1.223 (Claude Code)'")) {
        SpawnError::CliVersionUnmet { found, required } => {
            assert_eq!(found, "2.1.223");
            assert_eq!(required, "2.1.224");
        }
        other => panic!("expected CliVersionUnmet, got {other:?}"),
    }
}

/// The floor is inclusive: the exact pinned release is the one Ferrite is
/// developed against and must spawn.
#[test]
fn a_cli_exactly_at_the_pin_is_accepted() {
    let program = stub(
        "claude-at-pin",
        "case \"$1\" in --version) echo '2.1.224 (Claude Code)'; exit 0;; esac\nexit 0",
    );
    ClaudeSession::spawn(config(program)).expect("the pinned version must spawn");
}

/// A new major is a new protocol until someone proves otherwise. Refusing at
/// spawn is the whole point: a 3.x CLI that silently changed the wire would
/// otherwise fail somewhere deep in a turn, where the cause is invisible.
#[test]
fn a_cli_at_the_next_major_is_refused_rather_than_trusted() {
    match spawn_failure(stub("claude-future", "echo '3.0.0 (Claude Code)'")) {
        SpawnError::CliVersionUnsupported {
            found,
            supported_below,
        } => {
            assert_eq!(found, "3.0.0");
            assert_eq!(supported_below, "3.0.0");
        }
        other => panic!("expected CliVersionUnsupported, got {other:?}"),
    }
}

/// The ceiling is exclusive, so the whole 2.x line stays supported: only a
/// major bump is treated as an unknown protocol.
#[test]
fn the_last_release_below_the_next_major_is_accepted() {
    let program = stub(
        "claude-late-two",
        "case \"$1\" in --version) echo '2.99.99 (Claude Code)'; exit 0;; esac\nexit 0",
    );
    ClaudeSession::spawn(config(program)).expect("2.x must keep spawning");
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
            "{PRELUDE}\ncat '{}'\nexec sleep 30",
            fixture("hello-2.1.243").display()
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

/// A tool call, whole: the CLI settles the input, runs the tool, reports what
/// it produced. Replayed from the committed `tool` capture.
#[test]
fn a_tool_call_arrives_as_a_start_and_a_completion() {
    let events = replay("tool-2.1.243");

    let started: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::ToolStarted { id, name, input } => Some((id, name, input)),
            _ => None,
        })
        .collect();
    assert_eq!(started.len(), 1, "expected one tool start: {events:?}");
    let (id, name, input) = started[0];
    assert_eq!(name, "Bash");
    assert_eq!(input["command"], "echo ferrite-tool-ok");

    assert!(
        events.contains(&SessionEvent::ToolCompleted {
            id: id.clone(),
            output: "ferrite-tool-ok".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Command {
                stdout: "ferrite-tool-ok".into(),
                stderr: String::new(),
            },
        }),
        "no completion matching {id}: {events:?}"
    );
}

/// A Decision: the CLI stops and asks whether a tool may run. Replayed from
/// the committed `permission-allow` capture, which recorded the real
/// `can_use_tool` control request.
#[test]
fn a_permission_request_arrives_as_a_decision_naming_its_tool_call() {
    let events = replay("permission-allow-2.1.243");

    let decisions: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, SessionEvent::DecisionRequested { .. }))
        .collect();
    assert_eq!(decisions.len(), 1, "expected one Decision: {events:?}");
    let SessionEvent::DecisionRequested {
        id,
        tool_use_id,
        tool_name,
        description,
        input,
        suggestions,
    } = decisions[0]
    else {
        unreachable!()
    };

    assert!(!id.is_empty(), "a Decision must be answerable");
    assert_eq!(tool_name, "Write");
    assert_eq!(description, "ferrite-perm.txt");
    assert_eq!(input["content"], "ok");
    assert_eq!(suggestions[0]["mode"], "acceptEdits");

    // The Decision names the tool card it blocks, so a Pane can render it in
    // place instead of as a free-floating prompt.
    assert!(
        events.contains(&SessionEvent::ToolStarted {
            id: tool_use_id.clone(),
            name: "Write".into(),
            input: input.clone(),
        }),
        "no ToolStarted for {tool_use_id}: {events:?}"
    );
}

/// Answering a Decision has to put the exact bytes on the wire that the real
/// CLI accepted, so the assertion is the recorded host side of the capture
/// itself: whatever `respond_to_decision` writes must match what the live
/// probe proved works (allow → the tool ran; deny → the turn survived).
fn answering(fixture_name: &str, answer: impl Fn(&SessionEvent) -> DecisionAnswer) -> Value {
    let log = log_path(&format!("{fixture_name}-answer.log"));
    let _ = fs::remove_file(&log);
    let program = stub(
        &format!("claude-decides-{fixture_name}"),
        &format!(
            "{PRELUDE}\ncat '{}'\ncat >> '{}'",
            fixture(fixture_name).display(),
            log.display()
        ),
    );
    let mut session = ClaudeSession::spawn(config(program)).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match session.events().recv_timeout(left) {
            Ok(event @ SessionEvent::DecisionRequested { .. }) => {
                let SessionEvent::DecisionRequested { id, .. } = &event else {
                    unreachable!()
                };
                session.respond_to_decision(id, answer(&event)).unwrap();
                break;
            }
            Ok(_) => {}
            Err(e) => panic!("no Decision arrived: {e}"),
        }
    }

    // Line one is spawn's own initialize request; the answer follows it.
    let written = read_lines(&log, 2);
    drop(session);
    serde_json::from_str(&written[1]).expect("one JSON object per line")
}

/// What the recording sent back for the same Decision.
fn recorded_answer(fixture_name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/claude-{fixture_name}.host.jsonl"));
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|line| line["type"] == "control_response")
        .expect("the capture answered a Decision")
}

#[test]
fn allowing_a_decision_writes_what_the_cli_accepted() {
    let sent = answering("permission-allow-2.1.243", |event| {
        let SessionEvent::DecisionRequested { input, .. } = event else {
            unreachable!()
        };
        DecisionAnswer::Allow {
            input: input.clone(),
        }
    });
    assert_eq!(sent, recorded_answer("permission-allow-2.1.243"));
}

#[test]
fn denying_a_decision_carries_the_operator_s_reason_to_the_model() {
    let sent = answering("permission-deny-2.1.243", |_| DecisionAnswer::Deny {
        message: "Ferrite operator denied this tool".into(),
    });
    assert_eq!(sent, recorded_answer("permission-deny-2.1.243"));
}

/// A failed turn has to say what failed. Replayed from the committed `error`
/// capture, where the CLI could not reach the API: `subtype` says "success"
/// even so, and only `terminal_reason` classifies it.
#[test]
fn a_failed_turn_ends_with_the_reason_the_cli_gave() {
    let events = replay("error-2.1.243");
    assert_eq!(
        events.last(),
        Some(&SessionEvent::TurnEnded {
            outcome: TurnOutcome::Error("api_error: Not logged in · Please run /login".into()),
            cost_usd: Some(0.0),
        }),
        "stream: {events:?}"
    );
}

/// Feature detection happens at initialize, so a Session knows what the CLI
/// can do before the operator is offered anything. Replayed from the committed
/// `initialize` capture — the real control response, verbatim.
#[test]
fn spawn_completes_the_capability_handshake() {
    let program = stub(
        "claude-handshakes",
        &format!(
            "{VERSION_CASE}\ncat '{}'\nexec cat > /dev/null",
            fixture("initialize-2.1.243").display()
        ),
    );
    let session = ClaudeSession::spawn(config(program)).unwrap();
    let capabilities = session.capabilities();

    assert_eq!(capabilities.permission_mode, "bypassPermissions");
    assert!(
        capabilities.models.iter().any(|model| model == "haiku"),
        "models: {:?}",
        capabilities.models
    );
}

/// A CLI that answers nothing still yields a Session: an unknown capability is
/// reported as unknown, never guessed, and never a failure to spawn.
#[test]
fn a_silent_cli_leaves_its_capabilities_empty_rather_than_assumed() {
    let program = stub(
        "claude-mum",
        &format!("{VERSION_CASE}\nexec cat > /dev/null"),
    );
    let session = ClaudeSession::spawn(config(program)).unwrap();
    assert_eq!(session.capabilities(), &Capabilities::default());
}

#[test]
fn stdout_eof_closes_the_session_with_the_exit_status() {
    let program = stub("claude-quits", &format!("{PRELUDE}\nexit 0"));
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
        &format!("{PRELUDE}\necho 'fatal: no auth token' >&2\nexit 3"),
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
            "{PRELUDE}\necho \"$@\" > '{}'\ncat >> '{}'",
            log.display(),
            log.display()
        ),
    );
    let mut session = ClaudeSession::spawn(ClaudeConfig {
        program,
        cwd: Some(std::env::temp_dir()),
        model: Some("haiku".into()),
        permission_mode: Some("default".into()),
    })
    .unwrap();
    session.send("hi").unwrap();
    session.interrupt().unwrap();
    session.interrupt().unwrap();

    let recorded = read_lines(&log, 5);
    drop(session);
    let sent: Vec<serde_json::Value> = recorded[1..]
        .iter()
        .map(|line| serde_json::from_str(line).expect("every line is one JSON object"))
        .collect();

    // `--permission-prompt-tool stdio` is what makes the CLI ask over the
    // control protocol rather than deciding alone: without it a Decision never
    // reaches the operator, so it is not optional for a cockpit.
    assert_eq!(
        recorded[0],
        "-p --input-format stream-json --output-format stream-json \
         --include-partial-messages --verbose --permission-prompt-tool stdio \
         --model haiku --permission-mode default"
    );
    // Feature detection comes first, before a word of the Thread.
    assert_eq!(
        sent[0],
        serde_json::json!({
            "type": "control_request",
            "request_id": "req_1",
            "request": {"subtype": "initialize"},
        })
    );
    assert_eq!(
        sent[1],
        serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": [{"type": "text", "text": "hi"}]},
        })
    );
    assert_eq!(
        sent[2],
        serde_json::json!({
            "type": "control_request",
            "request_id": "req_2",
            "request": {"subtype": "interrupt"},
        })
    );
    assert_eq!(sent[3]["request_id"], "req_3");
}

/// Ferrite must not smuggle in a model or a permission posture the operator
/// did not choose: with no override the CLI keeps its own configuration.
#[test]
fn no_model_or_permission_mode_is_passed_when_the_config_names_none() {
    let log = log_path("argv-default.log");
    let program = stub(
        "claude-echoes-default",
        &format!(
            "{PRELUDE}\necho \"$@\" > '{}'\nexec cat > /dev/null",
            log.display()
        ),
    );
    let session = ClaudeSession::spawn(config(program)).unwrap();
    let recorded = read_lines(&log, 1);
    drop(session);
    assert_eq!(
        recorded[0],
        "-p --input-format stream-json --output-format stream-json \
         --include-partial-messages --verbose --permission-prompt-tool stdio"
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
            "{PRELUDE}\npwd -P > '{}'\nexec cat > /dev/null",
            log.display()
        ),
    );
    let session = ClaudeSession::spawn(ClaudeConfig {
        program,
        cwd: Some(workspace.clone()),
        ..Default::default()
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
            "{PRELUDE}\ni=1\nwhile [ $i -le 100 ]; do echo \"noise $i\" >&2; i=$((i+1)); done\nexit 3"
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
            "{PRELUDE}\ncat '{}'\nexec cat > /dev/null",
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
            "{PRELUDE}\ncat '{}'\nexec cat > /dev/null",
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
