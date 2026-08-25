//! The store's whole promise, end to end: quit Ferrite, relaunch, and the
//! Thread is still there — history, provider identity, resume metadata — and
//! the next prompt continues the same provider session.
//!
//! The provider side runs against a stub CLI replaying the committed resume
//! capture, so this is headless; the same continuation is proven against the
//! real CLI by `live_claude::a_parked_thread_resumes_across_processes`.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use ferrite_core::providers::{ClaudeConfig, ClaudeSession};
use ferrite_core::store::{Provider, Store};
use ferrite_core::transcript::{Input, Transcript};
use ferrite_core::workspace::WorkspaceChoice;
use ferrite_core::{SessionEvent, ToolResult, TurnOutcome};

/// The session id the committed resume capture announces — the conversation
/// the stubbed CLI "continues".
const CAPTURED_SESSION: &str = "c05fd480-7620-4b3d-96cb-c6e77062d38a";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrite-store-it-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}"))
}

/// An executable stub `claude` in a per-process temp dir.
fn stub(name: &str, script: &str) -> String {
    let dir = std::env::temp_dir().join(format!("ferrite-store-it-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path.display().to_string()
}

/// Events up to and including the first turn end or close.
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

/// Wait for the stub to have written `wanted` lines.
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

/// The first Session's turn, as its events reached Ferrite.
fn first_turn() -> Vec<SessionEvent> {
    vec![
        SessionEvent::Init {
            session_id: CAPTURED_SESSION.into(),
            model: "claude-haiku-4-5-20251001".into(),
        },
        SessionEvent::TextDelta {
            text: "Saving the codeword.\n\n".into(),
        },
        SessionEvent::ToolStarted {
            id: "toolu_1".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "echo saved" }),
        },
        SessionEvent::ToolCompleted {
            id: "toolu_1".into(),
            output: "saved".into(),
            is_error: false,
            result: ToolResult::Command {
                stdout: "saved".into(),
                stderr: String::new(),
            },
        },
        SessionEvent::TextDelta {
            text: "saved".into(),
        },
        SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(0.0024),
        },
    ]
}

#[test]
fn a_restart_restores_the_thread_and_the_next_prompt_continues_the_session() {
    let dir = scratch("restart");

    // A Thread lives: one prompt, one turn, then the operator quits Ferrite.
    // `operator_saw` is what the Pane showed — the restored Pane must show
    // the same thing.
    let mut operator_saw = Transcript::default();
    let thread_id = {
        let store = Store::open(&dir).unwrap();
        let (id, mut writer, _) = store
            .create(
                Provider::Claude,
                WorkspaceChoice::Main {
                    checkout: std::env::temp_dir(),
                },
            )
            .unwrap();
        writer
            .record_prompt("Remember the codeword: ferrite-resume-ok")
            .unwrap();
        operator_saw.apply(Input::Prompt(
            "Remember the codeword: ferrite-resume-ok".into(),
        ));
        for event in first_turn() {
            writer.record_event(&event).unwrap();
            operator_saw.apply(Input::Event(event));
        }
        id
        // Store and writer drop here: the app is gone.
    };

    // Relaunch: nothing survives but the directory.
    let store = Store::open(&dir).unwrap();
    let ids = store.thread_ids().unwrap();
    assert_eq!(ids, vec![thread_id], "the Thread vanished with the process");
    let thread = store.load(thread_id).unwrap();
    assert_eq!(thread.provider(), Provider::Claude);
    assert_eq!(thread.resume_target(), Some(CAPTURED_SESSION));

    let mut restored = Transcript::default();
    for input in thread.inputs() {
        restored.apply(input);
    }
    assert_eq!(restored.blocks(), operator_saw.blocks());
    assert_eq!(restored.session_id(), operator_saw.session_id());
    assert_eq!(restored.model(), operator_saw.model());

    // The next prompt continues the same provider session: the resume target
    // the store kept is what the new Session is spawned with, and the stub
    // records the argv the real CLI would have received.
    let argv_log = std::env::temp_dir().join(format!(
        "ferrite-store-it-{}/resume-argv.log",
        std::process::id()
    ));
    let _ = fs::remove_file(&argv_log);
    let program = stub(
        "claude-continues",
        &format!(
            "case \"$1\" in --version) echo '2.1.243 (Claude Code)'; exit 0;; esac\n\
             echo \"$@\" > '{}'\n\
             echo '{{\"type\":\"control_response\",\"response\":{{\"subtype\":\"success\",\"request_id\":\"req_1\",\"response\":{{}}}}}}'\n\
             cat '{}'\nexec cat > /dev/null",
            argv_log.display(),
            fixture("claude-resume-2.1.243.jsonl").display()
        ),
    );
    let session = ClaudeSession::spawn(ClaudeConfig {
        program,
        resume: thread.resume_target().map(str::to_string),
        ..Default::default()
    })
    .unwrap();

    let argv = read_lines(&argv_log, 1);
    assert!(
        argv[0].ends_with(&format!("--resume {CAPTURED_SESSION}")),
        "the Session was not spawned against the stored resume target: {argv:?}"
    );

    // The continued turn reaches the same Thread's log through a reopened
    // writer, exactly as the pump will feed it.
    let mut writer = store.writer(thread_id).unwrap();
    writer
        .record_prompt("What is the codeword? Reply with the codeword only.")
        .unwrap();
    operator_saw.apply(Input::Prompt(
        "What is the codeword? Reply with the codeword only.".into(),
    ));
    let continued = drain(session.events());
    for event in &continued {
        writer.record_event(event).unwrap();
        operator_saw.apply(Input::Event(event.clone()));
    }
    drop(writer);

    // The capture answered from history this process never had, and its
    // init re-announced the same session id — the resume target is stable.
    let text: String = continued
        .iter()
        .filter_map(|e| match e {
            SessionEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "ferrite-resume-ok");

    // A second restart shows the whole life of the Thread, both turns.
    let thread = Store::open(&dir).unwrap().load(thread_id).unwrap();
    assert_eq!(thread.resume_target(), Some(CAPTURED_SESSION));
    let mut restored = Transcript::default();
    for input in thread.inputs() {
        restored.apply(input);
    }
    assert_eq!(restored.blocks(), operator_saw.blocks());
}
