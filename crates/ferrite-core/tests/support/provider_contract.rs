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
        let mut script = format!(
            "#!/bin/sh\ncase \"$1\" in --version) echo '{version}'; exit 0;; esac\ncat {}\n",
            quote(&directory.join("frames"))
        );
        script.push_str(&format!(
            "skill_reads=0\nwhile IFS= read -r frame; do\nprintf '%s\\n' \"$frame\" >> {}\n",
            quote(&directory.join("host"))
        ));
        script.push_str(r#"case "$frame" in
*'"method":"model/list"'*)
request=$(printf '%s' "$frame" | sed -n 's/.*"id":\([^,}]*\).*/\1/p')
case "$frame" in
*'"cursor":"second"'*) printf '{"id":%s,"result":{"data":[{"id":"native-second","model":"native-second","displayName":"Second"}],"nextCursor":null}}\n' "$request";;
*) printf '{"id":%s,"result":{"data":[{"id":"native-first","model":"native-first","displayName":"First"}],"nextCursor":"second"}}\n' "$request";;
esac;;
*'"method":"skills/list"'*)
request=$(printf '%s' "$frame" | sed -n 's/.*"id":\([^,}]*\).*/\1/p')
skill_reads=$((skill_reads + 1))
if [ "$skill_reads" -eq 1 ]; then
printf '{"id":%s,"result":{"data":[{"cwd":"/workspace","skills":[{"name":"review","description":"Review changes","path":"/workspace/review/SKILL.md","enabled":true}],"errors":[]}]}}\n' "$request"
else
printf '{"id":%s,"result":{"data":[{"cwd":"/workspace","skills":[],"errors":[]}]}}\n' "$request"
fi;;
*'"method":"turn/start"'*)
request=$(printf '%s' "$frame" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
case "$frame" in
*'reject-start'*) printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32000,"message":"Native start rejected"}}\n' "$request";;
*'accept-without-notification'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"turn":{"id":"native-turn","status":"inProgress","items":[]}}}\n' "$request";;
esac;;
*'"subtype":"get_context_usage"'*)
request=$(printf '%s' "$frame" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
printf '{"type":"control_response","response":{"subtype":"success","request_id":"%s","response":{"totalTokens":12000,"rawMaxTokens":180000,"maxTokens":150000,"model":"fixture","autoCompactThreshold":140000,"isAutoCompactEnabled":true,"categories":[{"name":"Messages","tokens":12000,"color":"blue"}]}}}\n' "$request";;
*'"subtype":"mcp_status"'*)
request=$(printf '%s' "$frame" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
printf '{"type":"control_response","response":{"subtype":"success","request_id":"%s","response":{"mcpServers":[{"name":"search","status":"needs-auth","error":"Sign in to search"}]}}}\n' "$request";;
*'"subtype":"stop_task"'*|*'"subtype":"background_tasks"'*|*'"subtype":"mcp_reconnect"'*|*'"subtype":"set_permission_mode"'*)
request=$(printf '%s' "$frame" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
printf '{"type":"control_response","response":{"subtype":"success","request_id":"%s","response":{}}}\n' "$request";;
*'"subtype":"set_model"'*)
request=$(printf '%s' "$frame" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
case "$frame" in
*'rejected-model'*) printf '{"type":"control_response","response":{"subtype":"error","request_id":"%s","error":"model unavailable"}}\n' "$request";;
*) printf '{"type":"control_response","response":{"subtype":"success","request_id":"%s","response":{}}}\n' "$request";;
esac;;
esac
done
"#);
        fs::write(&program, script).unwrap();
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
