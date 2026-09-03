//! Codex Sessions driven against stub app-servers: no network, no real
//! `codex`.
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

use ferrite_core::providers::{CodexCapabilities, CodexConfig, CodexSession, CodexSpawnError};
use ferrite_core::{Decision, DecisionAnswer, SessionEvent, ToolResult, TurnOutcome};

const VERSION_CASE: &str = "case \"$1\" in --version) echo 'codex-cli 0.149.1'; exit 0;; esac";

/// What every stub has to do before it can pretend to be the app-server:
/// answer `--version`, then answer spawn's initialize and thread/start
/// requests the way a real server does — spawn refuses to hand back a
/// Session without both. Stubs that are *about* the handshake build on
/// `VERSION_CASE` instead and answer (or withhold) it themselves.
const PRELUDE: &str = concat!(
    "case \"$1\" in --version) echo 'codex-cli 0.149.1'; exit 0;; esac\n",
    r#"echo '{"id":1,"result":{"userAgent":"stub"}}'"#,
    "\n",
    r#"echo '{"id":2,"result":{"thread":{"id":"stub-thread"},"model":"stub-model","modelProvider":"stub","approvalPolicy":"on-request","sandbox":{"type":"readOnly"}}}'"#,
);

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/codex-{name}.jsonl"))
}

/// An executable stub `codex` in a per-process temp dir.
fn stub(name: &str, script: &str) -> String {
    let dir = std::env::temp_dir().join(format!("ferrite-codex-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path.display().to_string()
}

fn config(program: String) -> CodexConfig {
    CodexConfig {
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
/// reader thread. The fixture's own recorded responses answer the session's
/// handshake, because the capture driver numbers requests the way the
/// session does. Every capture is driven through a real process in this file
/// — the turnless `initialize` capture through the handshake test, every
/// turn-bearing one through this replay — not only through the parser's own
/// tests.
fn replay(name: &str) -> Vec<SessionEvent> {
    replay_with(name, CodexConfig::default())
}

fn replay_with(name: &str, template: CodexConfig) -> Vec<SessionEvent> {
    let program = stub(
        &format!("codex-replay-{name}"),
        &format!(
            "{VERSION_CASE}\ncat '{}'\nexec cat > /dev/null",
            fixture(name).display()
        ),
    );
    let session = CodexSession::spawn(CodexConfig {
        program,
        ..template
    })
    .unwrap();
    drain(session.events())
}

fn spawn_failure(config: CodexConfig) -> CodexSpawnError {
    match CodexSession::spawn(config) {
        Err(e) => e,
        Ok(_) => panic!("expected spawn to fail"),
    }
}

fn spawn_failure_of(program: String) -> CodexSpawnError {
    spawn_failure(config(program))
}

#[test]
fn a_missing_cli_is_named_in_the_error() {
    let program = "/nonexistent/ferrite/codex".to_string();
    match spawn_failure_of(program.clone()) {
        CodexSpawnError::CliNotFound { program: named } => assert_eq!(named, program),
        other => panic!("expected CliNotFound, got {other:?}"),
    }
}

#[test]
fn a_cli_below_the_pin_is_refused_before_any_session() {
    match spawn_failure_of(stub("codex-old", "echo 'codex-cli 0.149.0'")) {
        CodexSpawnError::CliVersionUnmet { found, required } => {
            assert_eq!(found, "0.149.0");
            assert_eq!(required, "0.149.1");
        }
        other => panic!("expected CliVersionUnmet, got {other:?}"),
    }
}

/// The floor is inclusive: the exact pinned release is the one Ferrite is
/// developed against and must spawn.
#[test]
fn a_cli_exactly_at_the_pin_is_accepted() {
    let program = stub("codex-at-pin", &format!("{PRELUDE}\nexec cat > /dev/null"));
    CodexSession::spawn(config(program)).expect("the pinned version must spawn");
}

/// A new major is a new protocol until someone proves otherwise. Refusing at
/// spawn is the whole point: a 1.x CLI that silently changed the wire would
/// otherwise fail somewhere deep in a turn, where the cause is invisible.
#[test]
fn a_cli_at_the_next_major_is_refused_rather_than_trusted() {
    match spawn_failure_of(stub("codex-future", "echo 'codex-cli 1.0.0'")) {
        CodexSpawnError::CliVersionUnsupported {
            found,
            supported_below,
        } => {
            assert_eq!(found, "1.0.0");
            assert_eq!(supported_below, "1.0.0");
        }
        other => panic!("expected CliVersionUnsupported, got {other:?}"),
    }
}

/// The ceiling is exclusive, so the whole 0.x line stays supported: only a
/// major bump is treated as an unknown protocol.
#[test]
fn the_last_release_below_the_next_major_is_accepted() {
    let program = stub(
        "codex-late-zero",
        &format!(
            "case \"$1\" in --version) echo 'codex-cli 0.999.999'; exit 0;; esac\n{}",
            PRELUDE_BODY
        ),
    );
    CodexSession::spawn(config(program)).expect("0.x must keep spawning");
}

/// The handshake lines of `PRELUDE` without its version case, for stubs that
/// answer `--version` differently.
const PRELUDE_BODY: &str = concat!(
    r#"echo '{"id":1,"result":{"userAgent":"stub"}}'"#,
    "\n",
    r#"echo '{"id":2,"result":{"thread":{"id":"stub-thread"},"model":"stub-model","modelProvider":"stub","approvalPolicy":"on-request","sandbox":{"type":"readOnly"}}}'"#,
    "\n",
    "exec cat > /dev/null",
);

/// What a stub that intends to EXIT must do first: consume the host's
/// handshake writes. Spawn is still writing `initialized` and `thread/start`
/// when a stub that never reads quits — the pipe breaks mid-handshake and the
/// test races the scheduler (it lost that race on a slow CI runner).
const DRAIN_HANDSHAKE: &str =
    r#"while read -r line; do case "$line" in *thread/start*) break;; esac; done"#;

#[test]
fn an_unreadable_version_banner_is_its_own_error() {
    match spawn_failure_of(stub("codex-mute", "echo 'codex-cli'")) {
        CodexSpawnError::VersionCheckFailed { detail } => assert!(
            detail.contains("codex-cli"),
            "detail should quote what the CLI printed: {detail}"
        ),
        other => panic!("expected VersionCheckFailed, got {other:?}"),
    }
}

#[test]
fn a_failing_version_check_is_not_mistaken_for_an_old_cli() {
    match spawn_failure_of(stub("codex-broken", "exit 7")) {
        CodexSpawnError::VersionCheckFailed { detail } => assert!(
            detail.contains("exit status: 7"),
            "detail should carry the status: {detail}"
        ),
        other => panic!("expected VersionCheckFailed, got {other:?}"),
    }
}

/// The whole path: real process, real pipes, real reader thread — the same
/// stream the committed 0.149.1 capture describes, with the Session's own
/// Init in front of it.
#[test]
fn the_reader_thread_delivers_the_captured_stream() {
    let program = stub(
        "codex-stream",
        &format!(
            "{VERSION_CASE}\ncat '{}'\nexec sleep 30",
            fixture("hello-0.149.1").display()
        ),
    );
    let session = CodexSession::spawn(config(program)).unwrap();
    let events = drain(session.events());

    assert_eq!(events.len(), 7, "unexpected stream: {events:?}");
    let SessionEvent::Init { session_id, model } = &events[0] else {
        panic!("the Session must announce itself first: {events:?}");
    };
    assert!(!session_id.is_empty());
    assert_eq!(model, "gpt-5.4-mini");

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
        .any(|e| matches!(e, SessionEvent::ReasoningSummaryDelta { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::TokenUsage { .. })));
    assert_eq!(
        events.last(),
        Some(&SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
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

/// A command run, whole: the server settles the command, runs it, reports the
/// merged output. Replayed from the committed `tool` capture.
#[test]
fn a_command_run_arrives_as_a_start_and_a_completion() {
    let events = replay("tool-0.149.1");

    let started: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::ToolStarted { id, name, input } => Some((id, name, input)),
            _ => None,
        })
        .collect();
    assert_eq!(started.len(), 1, "expected one tool start: {events:?}");
    let (id, name, input) = started[0];
    assert_eq!(name, "commandExecution");
    assert_eq!(input["command"], "/bin/zsh -lc 'echo ferrite-tool-ok'");

    assert!(
        events.contains(&SessionEvent::ToolCompleted {
            id: id.clone(),
            output: "ferrite-tool-ok\n".into(),
            is_error: false,
            result: ToolResult::Opaque,
        }),
        "no completion matching {id}: {events:?}"
    );
}

/// A Decision: the server stops and asks whether a command may run. Replayed
/// from the committed `approval-allow` capture, which recorded the real
/// JSON-RPC approval request.
#[test]
fn an_approval_request_arrives_as_a_decision_naming_its_tool_call() {
    let events = replay("approval-allow-0.149.1");

    let decisions: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, SessionEvent::DecisionRequested { .. }))
        .collect();
    assert_eq!(decisions.len(), 1, "expected one Decision: {events:?}");
    let SessionEvent::DecisionRequested {
        decision:
            Decision {
                id,
                tool_use_id,
                tool_name,
                description,
                input,
                suggestions,
            },
    } = decisions[0]
    else {
        unreachable!()
    };

    assert!(!id.is_empty(), "a Decision must be answerable");
    assert_eq!(tool_name, "commandExecution");
    assert_eq!(
        description,
        "/bin/zsh -lc \"printf 'ok' > ferrite-perm.txt\""
    );
    assert_eq!(input["itemId"], tool_use_id.as_str());
    assert!(!suggestions.is_empty(), "0.149.1 offers standing answers");

    // The Decision names the tool card it blocks, so a Pane can render it in
    // place instead of as a free-floating prompt.
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::ToolStarted { id, .. } if id == tool_use_id
        )),
        "no ToolStarted for {tool_use_id}: {events:?}"
    );
}

/// The other Decision shape, whole-path: a patch approval gates a fileChange
/// item whose changes live on the tool card, not in the approval params.
/// Replayed from the committed `approval-patch` capture.
#[test]
fn a_patch_approval_arrives_as_a_decision_on_the_file_change() {
    let events = replay("approval-patch-0.149.1");

    let SessionEvent::DecisionRequested {
        decision:
            Decision {
                tool_use_id,
                tool_name,
                ..
            },
    } = events
        .iter()
        .find(|e| matches!(e, SessionEvent::DecisionRequested { .. }))
        .expect("a Decision")
    else {
        unreachable!()
    };
    assert_eq!(tool_name, "fileChange");

    let SessionEvent::ToolStarted { input, .. } = events
        .iter()
        .find(|e| matches!(e, SessionEvent::ToolStarted { id, .. } if id == tool_use_id))
        .expect("the gated fileChange item")
    else {
        unreachable!()
    };
    assert_eq!(input["changes"][0]["path"], "/workspace/ferrite-patch.txt");
}

/// An interrupted capture ends the way the server said it did, through the
/// whole path. Replayed from the committed `interrupt` capture.
#[test]
fn an_interrupted_capture_replays_as_an_interrupted_turn() {
    let events = replay("interrupt-0.149.1");
    assert_eq!(
        events.last(),
        Some(&SessionEvent::TurnEnded {
            outcome: TurnOutcome::Interrupted,
            cost_usd: None,
        })
    );
}

/// Answering a Decision has to put the exact bytes on the wire that the real
/// server accepted, so the assertion is the recorded host side of the capture
/// itself: whatever `respond_to_decision` writes must match what the live
/// capture proved works (accept → the tool ran; decline → the turn survived).
/// `tag` keeps each caller's stub and log its own — tests run in parallel.
fn answering(fixture_name: &str, tag: &str, answer: impl Fn(&Decision) -> DecisionAnswer) -> Value {
    let log = log_path(&format!("{tag}-answer.log"));
    let _ = fs::remove_file(&log);
    let program = stub(
        &format!("codex-decides-{tag}"),
        &format!(
            "{VERSION_CASE}\ncat '{}'\ncat >> '{}'",
            fixture(fixture_name).display(),
            log.display()
        ),
    );
    let mut session = CodexSession::spawn(config(program)).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match session.events().recv_timeout(left) {
            Ok(SessionEvent::DecisionRequested { decision }) => {
                session
                    .respond_to_decision(&decision.id, answer(&decision))
                    .unwrap();
                break;
            }
            Ok(_) => {}
            Err(e) => panic!("no Decision arrived: {e}"),
        }
    }

    // The first five lines are spawn's own handshake and its skills/list
    // and model/list requests (#23, #25); the answer follows.
    let written = read_lines(&log, 6);
    drop(session);
    serde_json::from_str(&written[5]).expect("one JSON object per line")
}

/// What the recording sent back for the same Decision.
fn recorded_answer(fixture_name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/codex-{fixture_name}.host.jsonl"));
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|line| line.get("result").is_some())
        .expect("the capture answered a Decision")
}

/// The standing answer, echoed back exactly as the request offered it. The
/// capture behind this test answered one command that way and watched the
/// identical command run again in the same turn without a second approval.
#[test]
fn adopting_a_standing_answer_writes_the_amendment_the_server_accepted() {
    let sent = answering("approval-always-0.149.1", "always", |decision| {
        DecisionAnswer::AllowAlways {
            input: Value::Null,
            suggestion: decision
                .suggestions
                .iter()
                .find(|offered| offered.is_object())
                .cloned()
                .expect("the request offers a standing answer"),
        }
    });
    assert_eq!(sent, recorded_answer("approval-always-0.149.1"));
}

#[test]
fn allowing_a_decision_writes_what_the_server_accepted() {
    let sent = answering("approval-allow-0.149.1", "allow", |_| {
        DecisionAnswer::Allow { input: Value::Null }
    });
    assert_eq!(sent, recorded_answer("approval-allow-0.149.1"));
}

/// The documented capability gap, pinned: Codex cannot run a tool with
/// edited input — its accept is bare — so an Allow carrying edits must put
/// the same recorded bytes on the wire, not smuggle the edit into a shape
/// the server never accepted.
#[test]
fn an_allow_with_edited_input_still_writes_the_bare_accept() {
    let sent = answering("approval-allow-0.149.1", "allow-edited", |_| {
        DecisionAnswer::Allow {
            input: serde_json::json!({"command": "echo edited-by-operator"}),
        }
    });
    assert_eq!(sent, recorded_answer("approval-allow-0.149.1"));
}

#[test]
fn denying_a_decision_writes_what_the_server_accepted() {
    let sent = answering("approval-deny-0.149.1", "deny", |_| DecisionAnswer::Deny {
        // Dropped by design: the codex wire's decline carries no message.
        message: "Ferrite operator denied this tool".into(),
    });
    assert_eq!(sent, recorded_answer("approval-deny-0.149.1"));
}

/// A failed turn has to say what failed, in the server's own words. Replayed
/// from the committed `error` capture, where an unauthenticated server could
/// not reach the API.
#[test]
fn a_failed_turn_ends_with_the_reason_the_server_gave() {
    let events = replay("error-0.149.1");
    let Some(SessionEvent::TurnEnded {
        outcome: TurnOutcome::Error(reason),
        cost_usd: None,
    }) = events.last()
    else {
        panic!("expected a failed turn: {events:?}");
    };
    assert!(reason.contains("401 Unauthorized"), "reason: {reason}");
}

/// Feature detection happens in the thread/start response, so a Session knows
/// what the server can do before the operator is offered anything. Replayed
/// from the committed `initialize` capture — the real responses, verbatim.
#[test]
fn spawn_completes_the_capability_handshake() {
    let program = stub(
        "codex-handshakes",
        &format!(
            "{VERSION_CASE}\ncat '{}'\nexec cat > /dev/null",
            fixture("initialize-0.149.1").display()
        ),
    );
    let session = CodexSession::spawn(config(program)).unwrap();

    assert_eq!(
        session.capabilities(),
        &CodexCapabilities {
            model: "gpt-5.4-mini".into(),
            model_provider: "openai".into(),
            approval_policy: "on-request".into(),
            sandbox: "workspaceWrite".into(),
            reasoning_effort: Some("xhigh".into()),
        }
    );
}

/// #23: spawn asks for the `/` menu (skills/list) and the reader announces
/// the answer on the event stream — then a leading `/name` in a prompt rides
/// the wire as the typed `{"type":"skill"}` item with the args as text,
/// never as slash text the model would read as prose.
#[test]
fn a_listed_skill_is_sent_as_the_typed_item_never_as_slash_text() {
    let log = log_path("skill-send.log");
    let _ = fs::remove_file(&log);
    // The PRELUDE answers the handshake; the committed skills fixture
    // answers the menu request it provokes.
    let program = stub(
        "codex-skills",
        &format!(
            "{PRELUDE}\ncat '{}'\ncat >> '{}'",
            fixture("skills-0.149.1").display(),
            log.display()
        ),
    );
    let mut session = CodexSession::spawn(config(program)).unwrap();

    // The menu is announced before anything else can stream.
    let deadline = Instant::now() + Duration::from_secs(10);
    let commands = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match session.events().recv_timeout(left) {
            Ok(SessionEvent::Commands { commands }) => break commands,
            Ok(SessionEvent::Init { .. }) => continue,
            Ok(other) => panic!("unexpected event before the menu: {other:?}"),
            Err(e) => panic!("no menu arrived: {e}"),
        }
    };
    let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "probe-codex-skill",
            "probe-body",
            "browser:control-in-app-browser"
        ],
        "enabled skills only, in the server's order"
    );

    session.send("/probe-body follow the skill").unwrap();

    // Five handshake lines (initialize, initialized, thread/start,
    // skills/list, model/list), then the turn.
    let recorded = read_lines(&log, 6);
    drop(session);
    let skills_request: Value = serde_json::from_str(&recorded[3]).unwrap();
    assert_eq!(skills_request["method"], "skills/list");
    let turn: Value = serde_json::from_str(&recorded[5]).unwrap();
    assert_eq!(turn["method"], "turn/start");
    assert_eq!(
        turn["params"]["input"],
        serde_json::json!([
            {
                "type": "skill",
                "name": "probe-body",
                "path": "/workspace/.codex/skills/probe-body/SKILL.md",
            },
            {"type": "text", "text": "follow the skill"},
        ])
    );
}

/// #23: an `@path` token naming a real file under the thread's cwd rides as
/// a `{"type":"mention"}` item beside the verbatim text.
#[test]
fn a_mentioned_file_rides_as_a_mention_item_beside_the_text() {
    let workspace = log_path("mention-workspace");
    let _ = fs::remove_dir_all(&workspace);
    fs::create_dir_all(&workspace).unwrap();
    fs::write(workspace.join("notes.txt"), "MAGIC-WORD: zanzibar77\n").unwrap();

    let log = log_path("mention-send.log");
    let _ = fs::remove_file(&log);
    let program = stub(
        "codex-mentions",
        &format!("{PRELUDE}\ncat >> '{}'", log.display()),
    );
    let mut session = CodexSession::spawn(CodexConfig {
        program,
        cwd: Some(workspace.clone()),
        ..Default::default()
    })
    .unwrap();

    session
        .send("what is the magic word in @notes.txt ?")
        .unwrap();

    let recorded = read_lines(&log, 6);
    drop(session);
    let turn: Value = serde_json::from_str(&recorded[5]).unwrap();
    assert_eq!(
        turn["params"]["input"],
        serde_json::json!([
            {
                "type": "mention",
                "name": "notes.txt",
                "path": workspace.join("notes.txt").display().to_string(),
            },
            {"type": "text", "text": "what is the magic word in @notes.txt ?"},
        ])
    );
}

/// Resume is spawn with history: the session *asks* for the recorded thread
/// (the outbound thread/resume is asserted, not assumed), announces itself
/// with the resumed identity, and the model answers from a conversation this
/// process never had. Replayed from the committed `resume` capture.
#[test]
fn a_resumed_session_answers_from_the_previous_process_history() {
    let host = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/codex-resume-0.149.1.host.jsonl"),
    )
    .unwrap();
    let resumed_thread = host
        .lines()
        .find_map(|line| {
            let value: Value = serde_json::from_str(line).ok()?;
            if value.get("method")?.as_str()? != "thread/resume" {
                return None;
            }
            Some(value["params"]["threadId"].as_str()?.to_string())
        })
        .expect("the capture resumed a thread");

    let log = log_path("resume.log");
    let _ = fs::remove_file(&log);
    let program = stub(
        "codex-replay-resume-logged",
        &format!(
            "{VERSION_CASE}\ncat '{}'\ncat >> '{}'",
            fixture("resume-0.149.1").display(),
            log.display()
        ),
    );
    let session = CodexSession::spawn(CodexConfig {
        program,
        resume: Some(resumed_thread.clone()),
        ..Default::default()
    })
    .unwrap();
    let events = drain(session.events());

    // What Ferrite wrote: the second request must be a resume of exactly the
    // recorded thread — a session that quietly started a fresh thread would
    // replay this fixture identically otherwise.
    let recorded = read_lines(&log, 3);
    drop(session);
    let request: Value = serde_json::from_str(&recorded[2]).unwrap();
    assert_eq!(request["method"], "thread/resume");
    assert_eq!(
        request["params"]["threadId"].as_str(),
        Some(resumed_thread.as_str())
    );

    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::Init { session_id, .. } if *session_id == resumed_thread
        )),
        "the Session did not announce the resumed thread: {events:?}"
    );
    let text: String = events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "ferrite-resume-ok");
}

/// A server that answers nothing is a failed spawn, not a mute Session: a
/// Codex Session without a thread id could never say anything. The stub
/// holds stdout open and says nothing, so this rides out the whole handshake
/// budget.
#[test]
fn a_silent_server_fails_the_spawn_within_the_budget() {
    let program = stub("codex-mum", &format!("{VERSION_CASE}\nexec sleep 30"));
    match spawn_failure(config(program)) {
        CodexSpawnError::HandshakeFailed { detail } => assert!(
            detail.contains("no initialize response"),
            "detail should name the missing response: {detail}"
        ),
        other => panic!("expected HandshakeFailed, got {other:?}"),
    }
}

/// The server's own refusal reaches the operator in the server's own words.
#[test]
fn a_refused_thread_start_fails_the_spawn_with_the_server_s_reason() {
    let program = stub(
        "codex-refuses",
        &format!(
            "{VERSION_CASE}\n{}\n{}\nexec cat > /dev/null",
            r#"echo '{"id":1,"result":{}}'"#,
            r#"echo '{"id":2,"error":{"code":-32600,"message":"no such model"}}'"#
        ),
    );
    match spawn_failure(config(program)) {
        CodexSpawnError::HandshakeFailed { detail } => {
            assert!(detail.contains("no such model"), "detail: {detail}")
        }
        other => panic!("expected HandshakeFailed, got {other:?}"),
    }
}

/// A server that dies mid-handshake explains itself with its stderr, not a
/// timeout.
#[test]
fn a_server_that_dies_in_the_handshake_explains_itself() {
    let program = stub(
        "codex-dies",
        &format!("{VERSION_CASE}\necho 'fatal: bad config' >&2\nexit 3"),
    );
    match spawn_failure(config(program)) {
        CodexSpawnError::HandshakeFailed { detail } => {
            assert!(detail.contains("closed"), "detail: {detail}");
            assert!(detail.contains("fatal: bad config"), "detail: {detail}");
        }
        other => panic!("expected HandshakeFailed, got {other:?}"),
    }
}

/// What the server is asked to be, and what it is told — recorded so a change
/// to either is deliberate.
#[test]
fn the_session_speaks_the_pinned_command_line_and_protocol() {
    let log = log_path("argv.log");
    let program = stub(
        "codex-echoes",
        &format!(
            "{VERSION_CASE}\necho \"$@\" > '{}'\n{}\n{}\n{}\ncat >> '{}'",
            log.display(),
            r#"echo '{"id":1,"result":{}}'"#,
            // A turn already running when the handshake completes: what gives
            // interrupt a turn id to name without a race in the test.
            r#"echo '{"method":"turn/started","params":{"threadId":"stub-thread","turn":{"id":"stub-turn","items":[],"status":"inProgress"}}}'"#,
            r#"echo '{"id":2,"result":{"thread":{"id":"stub-thread"},"model":"stub-model"}}'"#,
            log.display()
        ),
    );
    let mut session = CodexSession::spawn(CodexConfig {
        program,
        cwd: Some(std::env::temp_dir()),
        model: Some("gpt-5.4-mini".into()),
        effort: Some("high".into()),
        approval_policy: Some("on-request".into()),
        sandbox: Some("read-only".into()),
        resume: None,
    })
    .unwrap();
    session.send("hi").unwrap();
    session.interrupt().unwrap();
    session.set_name("CI flake").unwrap();

    let recorded = read_lines(&log, 9);
    drop(session);
    assert_eq!(recorded[0], "app-server");
    let sent: Vec<Value> = recorded[1..]
        .iter()
        .map(|line| serde_json::from_str(line).expect("every line is one JSON object"))
        .collect();

    assert_eq!(sent[0]["method"], "initialize");
    assert_eq!(sent[0]["id"], 1);
    assert_eq!(sent[0]["params"]["clientInfo"]["name"], "ferrite");
    assert_eq!(
        sent[1],
        serde_json::json!({"jsonrpc": "2.0", "method": "initialized"})
    );
    assert_eq!(
        sent[2],
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "thread/start",
            "params": {
                "cwd": std::env::temp_dir().display().to_string(),
                "model": "gpt-5.4-mini",
                "approvalPolicy": "on-request",
                "sandbox": "read-only",
                "config": {"model_reasoning_effort": "high"},
            },
        })
    );
    // The `/` menu is asked for as soon as the thread is up (#23), and
    // the model menu right after it (#25).
    assert_eq!(
        sent[3],
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "skills/list",
            "params": {"cwds": [std::env::temp_dir().display().to_string()]},
        })
    );
    assert_eq!(
        sent[4],
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "model/list",
            "params": {},
        })
    );
    assert_eq!(
        sent[5],
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "turn/start",
            "params": {
                "threadId": "stub-thread",
                "input": [{"type": "text", "text": "hi"}],
            },
        })
    );
    // The interrupt names the turn the stub announced.
    assert_eq!(
        sent[6],
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "turn/interrupt",
            "params": {"threadId": "stub-thread", "turnId": "stub-turn"},
        })
    );
    // A rename names the thread server-side.
    assert_eq!(
        sent[7],
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "thread/name/set",
            "params": {"threadId": "stub-thread", "name": "CI flake"},
        })
    );
}

/// #25: spawn asks for the model menu (model/list) and the reader announces
/// the answer on the event stream — the picker's rows, with each model's
/// own effort ladder, hidden rows left out.
#[test]
fn the_model_list_is_announced_on_the_event_stream() {
    // The PRELUDE answers the handshake; the committed capture answers the
    // model/list request it provokes (the skills/list goes unanswered,
    // which must not hold the models back).
    let program = stub(
        "codex-models",
        &format!(
            "{PRELUDE}\ncat '{}'\nexec cat > /dev/null",
            fixture("models-0.144.4").display()
        ),
    );
    let session = CodexSession::spawn(config(program)).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    let models = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match session.events().recv_timeout(left) {
            Ok(SessionEvent::Models { models }) => break models,
            Ok(SessionEvent::Init { .. }) => continue,
            Ok(other) => panic!("unexpected event before the menu: {other:?}"),
            Err(e) => panic!("no menu arrived: {e}"),
        }
    };
    let values: Vec<&str> = models.iter().map(|row| row.value.as_str()).collect();
    assert_eq!(
        values,
        [
            "gpt-5.6-sol",
            "gpt-5.6-terra",
            "gpt-5.6-luna",
            "gpt-5.5",
            "gpt-5.4",
            "gpt-5.4-mini",
            "gpt-5.3-codex-spark",
        ]
    );
    assert_eq!(models[0].display, "GPT-5.6 Sol");
    assert_eq!(
        models[0].efforts,
        ["low", "medium", "high", "xhigh", "max", "ultra"]
    );
    assert_eq!(models[0].default_effort.as_deref(), Some("low"));
}

/// A resume carries the effort the same way a start does.
#[test]
fn a_resume_passes_the_effort_in_its_config() {
    let log = log_path("resume-effort.log");
    let _ = fs::remove_file(&log);
    let program = stub(
        "codex-resume-effort",
        &format!("{PRELUDE}\ncat >> '{}'", log.display()),
    );
    let session = CodexSession::spawn(CodexConfig {
        program,
        effort: Some("xhigh".into()),
        resume: Some("stub-thread".into()),
        ..Default::default()
    })
    .unwrap();
    let recorded = read_lines(&log, 3);
    drop(session);
    let resume: Value = serde_json::from_str(&recorded[2]).unwrap();
    assert_eq!(resume["method"], "thread/resume");
    assert_eq!(
        resume["params"],
        serde_json::json!({
            "threadId": "stub-thread",
            "config": {"model_reasoning_effort": "xhigh"},
        })
    );
}

/// Ferrite must not smuggle in a model, posture or sandbox the operator did
/// not choose: with no override the server keeps its own configuration.
#[test]
fn nothing_is_passed_when_the_config_names_nothing() {
    let log = log_path("argv-default.log");
    let program = stub(
        "codex-echoes-default",
        &format!("{PRELUDE}\ncat >> '{}'", log.display()),
    );
    let session = CodexSession::spawn(config(program)).unwrap();
    let recorded = read_lines(&log, 3);
    drop(session);
    let thread_start: Value = serde_json::from_str(&recorded[2]).unwrap();
    assert_eq!(thread_start["method"], "thread/start");
    assert_eq!(thread_start["params"], serde_json::json!({}));
}

/// A finished turn is no longer interruptible: once turn/completed has
/// arrived there is nothing running to name, so a late interrupt is the same
/// documented no-op as an idle one — never a request naming a dead turn.
#[test]
fn interrupting_after_the_turn_completed_writes_nothing() {
    let log = log_path("interrupt-late.log");
    let _ = fs::remove_file(&log);
    let program = stub(
        "codex-turn-done",
        &format!(
            "{VERSION_CASE}\n{}\n{}\n{}\n{}\ncat >> '{}'",
            r#"echo '{"id":1,"result":{}}'"#,
            // A whole turn passes before the handshake finishes, so by the
            // time spawn returns the reader has both seen and outlived it.
            r#"echo '{"method":"turn/started","params":{"threadId":"stub-thread","turn":{"id":"stub-turn","items":[],"status":"inProgress"}}}'"#,
            r#"echo '{"method":"turn/completed","params":{"threadId":"stub-thread","turn":{"id":"stub-turn","items":[],"status":"completed"}}}'"#,
            r#"echo '{"id":2,"result":{"thread":{"id":"stub-thread"},"model":"stub-model"}}'"#,
            log.display()
        ),
    );
    let mut session = CodexSession::spawn(config(program)).unwrap();
    session.interrupt().unwrap();
    session.send("hi").unwrap();

    // The send proves the write path works; no interrupt line precedes it.
    let recorded = read_lines(&log, 5);
    drop(session);
    assert!(
        !recorded.iter().any(|line| line.contains("turn/interrupt")),
        "an interrupt for a finished turn reached the wire: {recorded:?}"
    );
}

/// Interrupting before any turn has started has nothing to name: a no-op,
/// never a malformed request.
#[test]
fn interrupting_before_any_turn_writes_nothing() {
    let log = log_path("interrupt-idle.log");
    let _ = fs::remove_file(&log);
    let program = stub(
        "codex-idle",
        &format!("{PRELUDE}\ncat >> '{}'", log.display()),
    );
    let mut session = CodexSession::spawn(config(program)).unwrap();
    session.interrupt().unwrap();
    session.send("hi").unwrap();

    // The send proves the write path works; only the handshake, the menu
    // request and the turn are in the log, no interrupt line.
    let recorded = read_lines(&log, 5);
    drop(session);
    assert!(
        !recorded.iter().any(|line| line.contains("turn/interrupt")),
        "an idle interrupt reached the wire: {recorded:?}"
    );
}

#[test]
fn stdout_eof_closes_the_session_with_the_exit_status() {
    let program = stub(
        "codex-quits",
        &format!("{PRELUDE}\n{DRAIN_HANDSHAKE}\nexit 0"),
    );
    let session = CodexSession::spawn(config(program)).unwrap();
    match drain(session.events()).as_slice() {
        [SessionEvent::Init { .. }, SessionEvent::Closed { reason }] => {
            assert!(reason.contains("exit status: 0"), "reason: {reason}")
        }
        other => panic!("expected Init then Closed, got {other:?}"),
    }
}

#[test]
fn an_abnormal_exit_explains_itself_with_stderr() {
    let program = stub(
        "codex-crashes",
        &format!("{PRELUDE}\n{DRAIN_HANDSHAKE}\necho 'fatal: no auth token' >&2\nexit 3"),
    );
    let session = CodexSession::spawn(config(program)).unwrap();
    match drain(session.events()).as_slice() {
        [SessionEvent::Init { .. }, SessionEvent::Closed { reason }] => {
            assert!(reason.contains("exit status: 3"), "reason: {reason}");
            assert!(reason.contains("fatal: no auth token"), "reason: {reason}");
        }
        other => panic!("expected Init then Closed, got {other:?}"),
    }
}

/// A crash is explained by the end of stderr, not all of it: a chatty server
/// must not grow a Session's memory for as long as it runs.
#[test]
fn only_the_tail_of_a_noisy_stderr_is_kept() {
    let program = stub(
        "codex-noisy",
        &format!(
            "{PRELUDE}\n{DRAIN_HANDSHAKE}\ni=1\nwhile [ $i -le 100 ]; do echo \"noise $i\" >&2; i=$((i+1)); done\nexit 3"
        ),
    );
    let session = CodexSession::spawn(config(program)).unwrap();
    match drain(session.events()).as_slice() {
        [SessionEvent::Init { .. }, SessionEvent::Closed { reason }] => {
            let tail = reason
                .split_once("stderr: ")
                .unwrap_or_else(|| panic!("no stderr tail in reason: {reason}"))
                .1;
            assert_eq!(tail.lines().count(), 20, "unbounded stderr: {tail}");
            assert!(tail.contains("noise 100"), "newest line missing: {tail}");
            assert!(tail.contains("noise 81"), "tail is the wrong 20: {tail}");
            assert!(!tail.contains("noise 80"), "kept too much: {tail}");
        }
        other => panic!("expected Init then Closed, got {other:?}"),
    }
}

/// One mangled byte costs one line, never the Session: the server's stream is
/// not guaranteed to be well-formed UTF-8 and Ferrite keeps reading
/// regardless.
#[test]
fn a_line_of_invalid_utf8_does_not_end_the_session() {
    let payload = log_path("mangled.jsonl");
    let mut bytes = vec![0xff, 0xfe, b'\n'];
    bytes.extend_from_slice(
        br#"{"method":"item/agentMessage/delta","params":{"threadId":"t","turnId":"u","itemId":"m","delta":"still here"}}"#,
    );
    bytes.push(b'\n');
    bytes.extend_from_slice(
        br#"{"method":"turn/completed","params":{"threadId":"t","turn":{"id":"u","items":[],"status":"completed"}}}"#,
    );
    bytes.push(b'\n');
    fs::write(&payload, &bytes).unwrap();

    let program = stub(
        "codex-mangles",
        &format!(
            "{PRELUDE}\ncat '{}'\nexec cat > /dev/null",
            payload.display()
        ),
    );
    let session = CodexSession::spawn(config(program)).unwrap();
    let events = drain(session.events());
    assert_eq!(
        events,
        vec![
            SessionEvent::Init {
                session_id: "stub-thread".into(),
                model: "stub-model".into(),
            },
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

/// A Session that outruns its reader must stall the server, not lose its
/// output: the channel is bounded, so a slow frame is backpressure and never
/// a hole in the transcript.
#[test]
fn a_slow_consumer_stalls_the_server_instead_of_losing_events() {
    const DELTAS: usize = 3000;

    let payload = log_path("flood.jsonl");
    let mut lines = String::new();
    for n in 0..DELTAS {
        lines.push_str(&format!(
            r#"{{"method":"item/agentMessage/delta","params":{{"threadId":"t","turnId":"u","itemId":"m","delta":"{n} "}}}}"#
        ));
        lines.push('\n');
    }
    lines.push_str(
        r#"{"method":"turn/completed","params":{"threadId":"t","turn":{"id":"u","items":[],"status":"completed"}}}"#,
    );
    lines.push('\n');
    fs::write(&payload, &lines).unwrap();

    let program = stub(
        "codex-floods",
        &format!(
            "{PRELUDE}\ncat '{}'\nexec cat > /dev/null",
            payload.display()
        ),
    );
    let session = CodexSession::spawn(config(program)).unwrap();
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
    let dir = std::env::temp_dir().join(format!("ferrite-codex-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// Wait for the stub to have written `wanted` lines; the server end of a pipe
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
