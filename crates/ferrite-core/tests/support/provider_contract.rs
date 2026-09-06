//! Provider protocol replay through the production Session and Activity interfaces.
use ferrite_core::{
    activity::{Activity, ActivityEvent, ActivityInput, ExecutionEvent},
    providers::{ClaudeConfig, ClaudeSession, CodexConfig, CodexSession, Session},
    transcript::{Body, Input},
    SessionEvent,
};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

pub struct Replay {
    pub session: Box<dyn Session>,
    directory: PathBuf,
}
impl Replay {
    pub fn new(provider: &str, frames: Vec<Value>) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "ferrite-contract-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let mut all = if provider == "claude" {
            vec![
                json!({"type":"control_response","response":{"subtype":"success","request_id":"req_1","response":{}}}),
            ]
        } else {
            vec![
                json!({"id":1,"result":{"userAgent":"fixture"}}),
                json!({"id":2,"result":{"thread":{"id":"root"},"model":"fixture","approvalPolicy":"on-request","sandbox":{"type":"readOnly"}}}),
            ]
        };
        all.extend(frames);
        all.push(if provider == "claude" { json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"__CONTRACT_END__"}}}) } else { json!({"method":"item/agentMessage/delta","params":{"threadId":"root","delta":"__CONTRACT_END__"}}) });
        fs::write(
            directory.join("frames"),
            all.iter().map(|v| format!("{v}\n")).collect::<String>(),
        )
        .unwrap();
        let quote =
            |s: &std::path::Path| format!("'{}'", s.display().to_string().replace('\'', "'\\''"));
        let program = directory.join("provider");
        let version = if provider == "claude" {
            "2.1.263 (Claude Code)"
        } else {
            "codex-cli 0.153.4"
        };
        fs::write(&program,format!("#!/bin/sh\ncase \"$1\" in --version) echo '{version}'; exit 0;; esac\ncat {}\nexec cat > {}\n",quote(&directory.join("frames")),quote(&directory.join("host")))).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        let program = program.display().to_string();
        let session: Box<dyn Session> = if provider == "claude" {
            Box::new(
                ClaudeSession::spawn(ClaudeConfig {
                    program,
                    ..Default::default()
                })
                .unwrap(),
            )
        } else {
            Box::new(
                CodexSession::spawn(CodexConfig {
                    program,
                    ..Default::default()
                })
                .unwrap(),
            )
        };
        Self { session, directory }
    }
    pub fn drain(&self) -> Vec<SessionEvent> {
        let mut out = vec![];
        loop {
            let event = self
                .session
                .events()
                .recv_timeout(Duration::from_secs(5))
                .expect("protocol replay must finish");
            let end = match &event {
                SessionEvent::TextDelta { text } => text == "__CONTRACT_END__",
                SessionEvent::Activity(ActivityEvent::MainContent {
                    event: ExecutionEvent::TextDelta { text },
                    ..
                }) => text == "__CONTRACT_END__",
                _ => false,
            };
            if end {
                return out;
            }
            assert!(
                !matches!(event, SessionEvent::Closed { .. }),
                "fixture closed early: {event:?}"
            );
            out.push(event);
        }
    }
    pub fn wait_host(&self, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(v) = fs::read_to_string(self.directory.join("host"))
                .unwrap_or_default()
                .lines()
                .filter_map(|s| serde_json::from_str::<Value>(s).ok())
                .find(&predicate)
            {
                return v;
            }
            assert!(
                Instant::now() < deadline,
                "expected native request or reply absent"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for Replay {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
pub fn fold(events: Vec<SessionEvent>) -> Activity {
    let mut a = Activity::default();
    a.apply(ActivityInput::Connect { generation: 1 });
    for event in events {
        let at = Instant::now();
        a.apply(match event {
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
    a
}
pub fn prose(a: &Activity) -> Vec<String> {
    a.view()
        .main()
        .transcript()
        .blocks()
        .iter()
        .filter_map(|b| match &b.body {
            Body::Paragraph { spans } | Body::Heading { spans, .. } | Body::Bullet { spans } => {
                Some(spans.iter().map(|s| s.text.as_str()).collect())
            }
            _ => None,
        })
        .collect()
}
pub fn reasoning(a: &Activity) -> Vec<String> {
    a.view()
        .main()
        .transcript()
        .blocks()
        .iter()
        .filter_map(|b| match &b.body {
            Body::Thinking(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}
pub fn decisions(events: &[SessionEvent]) -> Vec<ferrite_core::Decision> {
    events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::DecisionRequested { decision }
            | SessionEvent::Activity(ActivityEvent::Decision { decision, .. }) => {
                Some(decision.clone())
            }
            _ => None,
        })
        .collect()
}
