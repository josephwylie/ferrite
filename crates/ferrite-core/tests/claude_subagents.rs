//! Real ClaudeSession pipes driven by reduced capture fixtures from a local stub.
//! No authenticated provider, model request, or captured tool is executed.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ferrite_core::activity::{ActivityEvent, AgentStatus, ExecutionEvent, Subject};
use ferrite_core::providers::{ClaudeConfig, ClaudeSession};
use ferrite_core::{DecisionAnswer, SessionEvent};
use serde_json::{json, Value};

const FINISHED: &str = "__CLAUDE_SUBAGENT_FIXTURE_END__";

struct Stub {
    directory: PathBuf,
}

impl Stub {
    fn new() -> Self {
        static NEXT_STUB: AtomicU64 = AtomicU64::new(1);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "ferrite-claude-subagent-test-{}-{nonce}-{}",
            std::process::id(),
            NEXT_STUB.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        Self { directory }
    }

    fn spawn(&self, relative_fixture: &str) -> ClaudeSession {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/subagents")
            .join(relative_fixture);
        let program = self.directory.join("claude");
        let marker = json!({"type":"stream_event", "parent_tool_use_id":null, "event":{
            "type":"content_block_delta", "delta":{"type":"text_delta", "text":FINISHED}
        }});
        fs::write(&program, format!(
            "#!/bin/sh\ncase \"$1\" in --version) echo '2.1.261 (Claude Code)'; exit 0;; esac\nprintf '%s\\n' \"$@\" > {}\necho '{{\"type\":\"control_response\",\"response\":{{\"subtype\":\"success\",\"request_id\":\"req_1\",\"response\":{{}}}}}}'\ncat {}\nprintf '%s\\n' '{}'\nexec cat > {}\n",
            quoted(&self.directory.join("argv.txt")), quoted(&fixture), marker,
            quoted(&self.directory.join("host.jsonl")),
        )).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        ClaudeSession::spawn(ClaudeConfig {
            program: program.display().to_string(),
            ..Default::default()
        })
        .unwrap()
    }

    fn replies(&self) -> Vec<Value> {
        fs::read_to_string(self.directory.join("host.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|value| value["type"] == "control_response")
            .collect()
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

fn drain(session: &ClaudeSession) -> Vec<SessionEvent> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut result = Vec::new();
    loop {
        let event = session
            .events()
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("complete captured stream");
        if matches!(&event, SessionEvent::TextDelta { text } if text == FINISHED) {
            return result;
        }
        assert!(
            !matches!(event, SessionEvent::Closed { .. }),
            "stub exited early"
        );
        result.push(event);
    }
}

#[test]
fn forwarding_is_enabled_only_through_the_scoped_reader() {
    let stub = Stub::new();
    let session = stub.spawn("claude-overlap-2.1.261.jsonl");
    let events = drain(&session);
    assert!(fs::read_to_string(stub.directory.join("argv.txt"))
        .unwrap()
        .lines()
        .any(|arg| arg == "--forward-subagent-text"));
    let texts: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::Activity(ActivityEvent::Content {
                key,
                event: ExecutionEvent::Text { text },
                ..
            }) => Some((key, text.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(texts.len(), 4);
    assert_eq!(
        texts.iter().map(|(_, text)| *text).collect::<Vec<_>>(),
        ["ALPHA_START", "BETA_START", "ALPHA_DONE", "BETA_DONE"]
    );
    assert_eq!(texts[0].0, texts[2].0);
    assert_eq!(texts[1].0, texts[3].0);
    assert_ne!(texts[0].0, texts[1].0);
    assert!(events
        .iter()
        .all(|event| !matches!(event, SessionEvent::ToolStarted { name, .. } if name == "Bash")));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                SessionEvent::Activity(ActivityEvent::Status {
                    state: AgentStatus::Idle,
                    ..
                })
            ))
            .count(),
        2
    );
    let fixture = include_str!("fixtures/subagents/claude-overlap-2.1.261.jsonl");
    let expected_main_usage = fixture
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|value| {
            (value["type"] == "assistant"
                && value["parent_tool_use_id"].is_null()
                && value["message"].get("usage").is_some())
                || value["type"] == "result"
        })
        .count();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, SessionEvent::TokenUsage { .. }))
            .count(),
        expected_main_usage
    );
}

#[test]
fn simultaneous_child_decisions_reply_through_the_original_host_handles() {
    let stub = Stub::new();
    let mut session = stub.spawn("claude-decisions-2.1.261.jsonl");
    let events = drain(&session);
    let decisions: Vec<_> = events
        .into_iter()
        .filter_map(|event| match event {
            SessionEvent::Activity(ActivityEvent::Decision {
                subject: Some(Subject::Subagent(key)),
                decision,
            }) => Some((key, decision)),
            _ => None,
        })
        .collect();
    assert_eq!(decisions.len(), 2);
    assert_ne!(decisions[0].0, decisions[1].0);
    for (_, decision) in decisions.iter().rev() {
        session
            .respond_to_decision(
                &decision.id,
                DecisionAnswer::Deny {
                    message: "fixture denied".into(),
                },
            )
            .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let replies = loop {
        let replies = stub.replies();
        if replies.len() == 2 {
            break replies;
        }
        assert!(Instant::now() < deadline, "missing Decision replies");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        replies
            .iter()
            .map(|value| value["response"]["request_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        decisions
            .iter()
            .rev()
            .map(|(_, decision)| decision.id.as_str())
            .collect::<Vec<_>>()
    );
    assert!(replies
        .iter()
        .all(|value| value["response"]["response"]["behavior"] == "deny"));
}

#[test]
fn background_notification_results_do_not_masquerade_as_operator_turn_ends() {
    let stub = Stub::new();
    let session = stub.spawn("claude-reuse-2.1.261.jsonl");
    let events = drain(&session);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, SessionEvent::TurnEnded { .. }))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                SessionEvent::Activity(ActivityEvent::BackgroundTurnEnded { .. })
            ))
            .count(),
        1
    );
}
