//! Background work reporting back to the row that launched it (HD64).
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use ferrite_core::activity::{Activity, ActivityInput};
use ferrite_core::providers::{ClaudeConfig, ClaudeSession};
use ferrite_core::transcript::{Body, Input, ToolBlock, ToolState, Transcript};
use ferrite_core::{SessionEvent, ToolResult, TurnOutcome};

const PRELUDE: &str = concat!(
    "case \"$1\" in --version) echo '2.1.243 (Claude Code)'; exit 0;; esac\n",
    r#"echo '{"type":"control_response","response":{"subtype":"success","request_id":"req_1","response":{}}}'"#,
);

fn fixture(name: &str) -> PathBuf {
    // Cargo runs a crate's tests in the crate's own directory.
    std::path::absolute(format!("tests/fixtures/claude-{name}.jsonl")).unwrap()
}

fn stub(name: &str, script: &str) -> String {
    let dir = std::env::temp_dir().join(format!("ferrite-claude-bg-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path.display().to_string()
}

/// Every event the capture produces: it holds three results (the launching
/// turn and one per handback), so draining stops at a quiet pipe.
fn drain_all(events: &Receiver<SessionEvent>) -> Vec<SessionEvent> {
    let mut drained = Vec::new();
    while let Ok(event) = events.recv_timeout(Duration::from_millis(1500)) {
        drained.push(event);
    }
    drained
}

fn replay(name: &str) -> Vec<SessionEvent> {
    let program = stub(
        &format!("claude-replay-{name}"),
        &format!(
            "{PRELUDE}\ncat '{}'\nexec cat > /dev/null",
            fixture(name).display()
        ),
    );
    let session = ClaudeSession::spawn(ClaudeConfig {
        program,
        ..Default::default()
    })
    .unwrap();
    drain_all(session.events())
}

fn fold(events: &[SessionEvent]) -> Activity {
    let mut activity = Activity::default();
    activity.apply(ActivityInput::Connect { generation: 1 });
    for event in events.iter().cloned() {
        let at = Instant::now();
        activity.apply(match event {
            SessionEvent::Activity(event) => ActivityInput::Observe {
                generation: 1,
                event,
                at,
            },
            event => ActivityInput::Main {
                input: Input::Event(event),
                at,
            },
        });
    }
    activity
}

fn notices(activity: &Activity) -> Vec<String> {
    activity
        .view()
        .main()
        .transcript()
        .blocks()
        .iter()
        .filter_map(|block| match &block.body {
            Body::Notice(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn tool(activity: &Activity, name: &str) -> ToolBlock {
    activity
        .view()
        .main()
        .transcript()
        .blocks()
        .iter()
        .find_map(|block| match &block.body {
            Body::Tool(tool) if tool.name == name => Some(tool.clone()),
            _ => None,
        })
        .expect("the call has a row")
}

/// Claude's real capture (2.1.318): a backgrounded Bash call and a
/// background agent, each launched in one turn and handed back later. A
/// launch's result only acknowledges the launch, so neither row settles
/// there: each stays running through the turn's end, and settles when its
/// task's notification names its call — leaving a muted line where it
/// completed, in Claude's words.
#[test]
fn background_work_settles_its_own_row_when_its_task_reports_back() {
    let events = replay("background-2.1.318");
    let activity = fold(&events);
    let transcript = activity.view().main().transcript();

    let bash = tool(&activity, "Bash");
    assert_eq!(bash.state, ToolState::Ok, "{bash:?}");
    assert!(
        !bash
            .result_line
            .as_deref()
            .unwrap_or("")
            .contains("running in background"),
        "the launch acknowledgement is not the call's result: {bash:?}"
    );
    let agent = tool(&activity, "Agent");
    assert_eq!(agent.state, ToolState::Ok);

    let calls: Vec<_> = transcript.background_calls().collect();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls.iter().all(|call| !call.in_background()));
    assert_eq!(
        calls
            .iter()
            .map(|call| call.background.as_ref().unwrap().task.as_str())
            .collect::<Vec<_>>(),
        ["bhv9eh2do", "a8691430d2a53580d"],
        "in row order: the Bash call was made first"
    );

    assert_eq!(
        notices(&activity),
        [
            "Background task completed \u{b7} Agent \"Run sleep and echo\" \u{b7} took 8s",
            "Background command completed \u{b7} Sleep 6 seconds then echo bg-done",
        ],
        "one line per handback, and no generic agent count"
    );
    // Each line sits where its task completed: after the launching turn's
    // answer, ahead of the turns the handbacks started (both notifications
    // land before either handback turn answers, capture lines 77 and 82).
    let order: Vec<&str> = transcript
        .blocks()
        .iter()
        .filter_map(|block| match &block.body {
            Body::Notice(_) => Some("notice"),
            Body::Paragraph { .. } => Some("prose"),
            _ => None,
        })
        .collect();
    assert_eq!(order, ["prose", "notice", "notice", "prose", "prose"]);
}

/// The model of it, without a provider: launched, the call runs on past
/// its turn's end (a turn end retires only foreground calls), and its
/// handback settles it. A Session that closes first takes the work with it.
#[test]
fn a_background_call_outlives_its_turn_but_not_its_session() {
    let mut transcript = Transcript::default();
    transcript.apply(Input::Prompt("go".into()));
    transcript.apply(Input::Event(SessionEvent::ToolStarted {
        id: "call".into(),
        name: "Bash".into(),
        input: serde_json::json!({"command": "sleep 9", "description": "Sleep nine"}),
    }));
    transcript.apply(Input::Event(SessionEvent::ToolCompleted {
        id: "call".into(),
        output: "Command running in background with ID: b1.".into(),
        is_error: false,
        result: ToolResult::Background { task: "b1".into() },
    }));
    transcript.apply(Input::Event(SessionEvent::TurnEnded {
        outcome: TurnOutcome::Completed,
        cost_usd: None,
    }));
    assert_eq!(transcript.tool("call").unwrap().state, ToolState::Running);
    assert!(transcript.running_in_background("call"));

    transcript.apply(Input::Event(SessionEvent::ToolCompleted {
        id: "call".into(),
        output: "nine\n".into(),
        is_error: false,
        result: ToolResult::Command {
            stdout: "nine\n".into(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: None,
        },
    }));
    let call = transcript.tool("call").unwrap();
    assert_eq!(call.state, ToolState::Ok);
    assert_eq!(call.output.as_ref().map(|out| out.text.as_str()), Some("nine\n"));
    assert!(!transcript.running_in_background("call"));
    assert!(matches!(
        &transcript.blocks().last().unwrap().body,
        Body::Notice(text) if text == "Background command completed \u{b7} Sleep nine"
    ));

    let mut closing = Transcript::default();
    closing.apply(Input::Event(SessionEvent::ToolStarted {
        id: "call".into(),
        name: "Bash".into(),
        input: serde_json::json!({"command": "sleep 9"}),
    }));
    closing.apply(Input::Event(SessionEvent::ToolCompleted {
        id: "call".into(),
        output: String::new(),
        is_error: false,
        result: ToolResult::Background { task: "b1".into() },
    }));
    closing.apply(Input::Event(SessionEvent::Closed {
        reason: "closed".into(),
    }));
    assert_eq!(closing.tool("call").unwrap().state, ToolState::Unavailable);
}

/// A failed command says so, with the code the CLI reported.
#[test]
fn a_failed_background_command_says_it_failed() {
    let mut transcript = Transcript::default();
    transcript.apply(Input::Event(SessionEvent::ToolStarted {
        id: "call".into(),
        name: "Bash".into(),
        input: serde_json::json!({"command": "false", "description": "Fail on purpose"}),
    }));
    transcript.apply(Input::Event(SessionEvent::ToolCompleted {
        id: "call".into(),
        output: String::new(),
        is_error: false,
        result: ToolResult::Background { task: "b1".into() },
    }));
    transcript.apply(Input::Event(SessionEvent::ToolCompleted {
        id: "call".into(),
        output: "Background command \"Fail on purpose\" failed with exit code 1".into(),
        is_error: true,
        result: ToolResult::Command {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(1),
            duration_ms: None,
        },
    }));
    assert!(matches!(transcript.tool("call").unwrap().state, ToolState::Failed(_)));
    assert!(matches!(
        &transcript.blocks().last().unwrap().body,
        Body::Notice(text) if text == "Background command failed \u{b7} Fail on purpose"
    ));
}
