//! The public Session transport: observed descendants stay inside their parent
//! connection, history reads remain read-only, and their Decisions retain IDs.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use ferrite_core::activity::{Activity, ActivityEvent, ActivityInput, AgentKey, Subject};
use ferrite_core::providers::{CodexConfig, CodexSession, Session};
use ferrite_core::store::Provider;
use ferrite_core::{DecisionAnswer, SessionEvent};
use serde_json::{json, Value};

static NEXT_STUB: AtomicU64 = AtomicU64::new(0);

struct Stub(PathBuf);
impl Stub {
    fn new() -> Self {
        loop {
            let id = NEXT_STUB.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("ferrite-codex-routing-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create stub: {error}"),
            }
        }
    }
    fn spawn(&self) -> Box<dyn Session> {
        let program = self.0.join("codex");
        let script=r#"#!/bin/sh
case "$1" in --version) echo 'codex-cli 0.153.4'; exit 0;; esac
reads=0
answers=0
while IFS= read -r line; do
  printf '%s\n' "$line" >> '__LOG__'
  case "$line" in
    *'"method":"initialize"'*) echo '{"id":1,"result":{}}' ;;
    *'"method":"thread/start"'*) echo '{"id":2,"result":{"thread":{"id":"main"},"model":"stub"}}' ;;
    *'"text":"launch"'*)
      echo '{"method":"turn/started","params":{"threadId":"main","turn":{"id":"main-turn"}}}'
      echo '{"method":"thread/started","params":{"thread":{"id":"unrelated","parentThreadId":null}}}'
      echo '{"method":"item/agentMessage/delta","params":{"threadId":"unrelated","turnId":"stray","itemId":"same","delta":"STRAY"}}'
      echo '{"method":"turn/completed","params":{"threadId":"unrelated","turn":{"id":"stray","status":"completed"}}}'
      echo '{"method":"turn/started","params":{"threadId":"alpha","turn":{"id":"turn"}}}'
      echo '{"method":"item/agentMessage/delta","params":{"threadId":"alpha","turnId":"turn","itemId":"same","delta":"ALPHA_"}}'
      echo '{"method":"item/completed","params":{"threadId":"main","turnId":"main-turn","item":{"type":"subAgentActivity","id":"spawn-a","kind":"started","agentThreadId":"alpha","agentPath":"/root/alpha"}}}'
      echo '{"method":"item/completed","params":{"threadId":"main","turnId":"main-turn","item":{"type":"subAgentActivity","id":"spawn-b","kind":"started","agentThreadId":"beta","agentPath":"/root/beta"}}}'
      echo '{"method":"item/agentMessage/delta","params":{"threadId":"alpha","turnId":"turn","itemId":"same","delta":"NEW"}}'
      echo '{"method":"item/completed","params":{"threadId":"alpha","turnId":"turn","item":{"type":"agentMessage","id":"same","text":"ALPHA_NEW"}}}'
      echo '{"method":"item/agentMessage/delta","params":{"threadId":"beta","turnId":"turn","itemId":"same","delta":"BETA"}}'
      echo '{"method":"item/completed","params":{"threadId":"beta","turnId":"turn","item":{"type":"agentMessage","id":"same","text":"BETA"}}}'
      echo '{"method":"item/completed","params":{"threadId":"alpha","turnId":"turn","item":{"type":"commandExecution","id":"tool","command":"synthetic-alpha","status":"completed","aggregatedOutput":"alpha-output"}}}'
      echo '{"method":"item/completed","params":{"threadId":"beta","turnId":"turn","item":{"type":"commandExecution","id":"tool","command":"synthetic-beta","status":"completed","aggregatedOutput":"beta-output"}}}'
      echo '{"method":"thread/tokenUsage/updated","params":{"threadId":"alpha","tokenUsage":{"last":{"totalTokens":10},"total":{"inputTokens":8,"outputTokens":2}}}}'
      echo '{"method":"turn/completed","params":{"threadId":"alpha","turn":{"id":"turn","status":"completed","itemsView":"summary","items":[{"type":"agentMessage","id":"same","text":"ALPHA_NEW"}]}}}'
      echo '{"method":"turn/completed","params":{"threadId":"beta","turn":{"id":"turn","status":"completed"}}}'
      echo '{"method":"turn/started","params":{"threadId":"main","thread":{"id":"alpha"},"turn":{"id":"ambiguous-turn"}}}' ;;
    *'"method":"thread/read"'*)
      case "$line" in
        *'"threadId":"alpha"'*)
          echo '{"id":"ferrite-agent-history-1","result":{"thread":{"id":"alpha","parentThreadId":"main","agentNickname":"Plato","status":{"type":"idle"},"turns":[{"id":"turn","status":"completed","itemsView":"full","items":[{"type":"agentMessage","id":"same","text":"STALE_ALPHA"}]}]}}}' ;;
        *'"threadId":"beta"'*)
          echo '{"id":"ferrite-agent-history-2","result":{"thread":{"id":"beta","parentThreadId":"main","agentNickname":"Euler","status":{"type":"idle"},"turns":[{"id":"turn","status":"completed","itemsView":"full","items":[{"type":"agentMessage","id":"same","text":"STALE_BETA"}]}]}}}' ;;
      esac
      reads=$((reads + 1))
      if [ "$reads" -eq 2 ]; then
        echo '{"id":0,"method":"item/commandExecution/requestApproval","params":{"threadId":"alpha","turnId":"turn-2","itemId":"approval-tool","command":"synthetic-alpha"}}'
        echo '{"id":"0","method":"item/commandExecution/requestApproval","params":{"threadId":"beta","turnId":"turn-2","itemId":"approval-tool","command":"synthetic-beta"}}'
        echo '{"method":"item/agentMessage/delta","params":{"threadId":"main","turnId":"main-turn","itemId":"checkpoint","delta":"READY"}}'
      fi ;;
    *'"result":'*)
      answers=$((answers + 1))
      if [ "$answers" -eq 2 ]; then
        echo '{"method":"serverRequest/resolved","params":{"threadId":"alpha","requestId":0}}'
        echo '{"method":"serverRequest/resolved","params":{"threadId":"beta","requestId":"0"}}'
        echo '{"method":"item/agentMessage/delta","params":{"threadId":"main","turnId":"main-turn","itemId":"checkpoint","delta":"ACK"}}'
      fi ;;
    *'"text":"finish"'*)
      echo '{"method":"turn/completed","params":{"threadId":"main","turn":{"id":"main-turn","status":"completed"}}}'
      echo '{"method":"thread/tokenUsage/updated","params":{"threadId":"main","tokenUsage":{"last":{"totalTokens":3331},"total":{"totalTokens":3331,"inputTokens":0,"cachedInputTokens":0,"outputTokens":0,"reasoningOutputTokens":0},"modelContextWindow":200000}}}' ;;
    *'"text":"flush"'*) echo '{"method":"thread/tokenUsage/updated","params":{"threadId":"main","tokenUsage":{"last":{"totalTokens":3332},"total":{"totalTokens":3332,"inputTokens":0,"cachedInputTokens":0,"outputTokens":0,"reasoningOutputTokens":0},"modelContextWindow":200000}}}' ;;
  esac
done
"#.replace("__LOG__",&self.0.join("host.jsonl").display().to_string());
        fs::write(&program, script).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        Box::new(
            CodexSession::spawn(CodexConfig {
                program: program.display().to_string(),
                ..Default::default()
            })
            .unwrap(),
        )
    }
}
impl Drop for Stub {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn collect_until(session: &dyn Session, events: &mut Vec<SessionEvent>, checkpoint: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let event = session
            .events()
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("stub checkpoint");
        // Transport barriers after completion must remain metadata: late prose
        // for a completed turn is correctly rejected by the production Router.
        let done = match &event {
            SessionEvent::TextDelta { text } => text == checkpoint,
            SessionEvent::TokenUsage { total_tokens, .. } => matches!(
                (checkpoint, *total_tokens),
                ("FINISHED", 3331) | ("FLUSHED", 3332)
            ),
            _ => false,
        };
        assert!(
            !matches!(event, SessionEvent::Closed { .. }),
            "stub unexpectedly closed: {event:?}"
        );
        events.push(event);
        if done {
            return;
        }
    }
}

#[test]
fn descendants_read_history_on_parent_connection_and_answer_the_exact_child_requests() {
    let stub = Stub::new();
    let mut session = stub.spawn();
    let mut events = Vec::new();
    session.send("launch").unwrap();
    collect_until(session.as_ref(), &mut events, "READY");
    let decisions: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::Activity(ActivityEvent::Decision {
                subject: Some(Subject::Subagent(key)),
                decision,
            }) => Some((key.clone(), decision.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(decisions.len(), 2);
    assert_ne!(decisions[0].0, decisions[1].0);
    for (_, decision) in decisions.iter().rev() {
        session
            .respond_to_decision(&decision.id, DecisionAnswer::Allow { input: Value::Null })
            .unwrap();
    }
    collect_until(session.as_ref(), &mut events, "ACK");
    session.interrupt().unwrap();
    session.send("finish").unwrap();
    collect_until(session.as_ref(), &mut events, "FINISHED");
    session.interrupt().unwrap();
    session.send("flush").unwrap();
    collect_until(session.as_ref(), &mut events, "FLUSHED");
    let frames: Vec<Value> = fs::read_to_string(stub.0.join("host.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(frames.iter().any(|frame| frame["method"] == "thread/read"));
    assert!(!frames
        .iter()
        .any(|frame| frame["method"] == "thread/resume"));
    let answers: Vec<_> = frames
        .iter()
        .filter(|frame| frame.get("result").is_some())
        .map(|frame| frame["id"].clone())
        .collect();
    assert_eq!(answers, vec![json!("0"), json!(0)]);
    let interrupts: Vec<_> = frames
        .iter()
        .filter(|frame| frame["method"] == "turn/interrupt")
        .map(|frame| frame["params"].clone())
        .collect();
    assert_eq!(
        interrupts,
        vec![json!({"threadId":"main","turnId":"main-turn"})]
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, SessionEvent::TurnEnded { .. }))
            .count(),
        1
    );
    assert!(!events.iter().any(|event| matches!(
        event,
        SessionEvent::ToolStarted { .. }
            | SessionEvent::ToolCompleted { .. }
            | SessionEvent::DecisionRequested { .. }
    )));
    let main: String = events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(main, "READYACK");
    assert_eq!(
        events
            .iter()
            .filter_map(|event| match event {
                SessionEvent::TokenUsage { total_tokens, .. } => Some(*total_tokens),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![3331, 3332],
        "only Main transport checkpoints, never child token usage"
    );
    let mut activity = Activity::default();
    activity.apply(ActivityInput::Connect { generation: 1 });
    for event in events {
        if let SessionEvent::Activity(event) = event {
            activity.apply(ActivityInput::Observe {
                generation: 1,
                event,
                at: Instant::now(),
            });
        }
    }
    assert!(activity.view().pending_decisions().is_empty());
    for (native, name, text) in [("alpha", "Plato", "ALPHA_NEW"), ("beta", "Euler", "BETA")] {
        let key = AgentKey::new(Provider::Codex, "main", native);
        let child = activity
            .view()
            .children()
            .into_iter()
            .find(|child| child.key() == &key)
            .unwrap();
        assert_eq!(child.info().name.as_deref(), Some(name));
        let rendered: String = child
            .transcript()
            .blocks()
            .iter()
            .filter_map(|block| block.markdown.as_deref())
            .collect();
        assert_eq!(
            rendered, text,
            "history and live frames must reconcile by child/turn/item"
        );
    }
}

#[test]
fn a_large_resumed_tree_does_not_block_session_handshake() {
    let stub = Stub::new();
    let program = stub.0.join("codex");
    let items: Vec<Value> = (0..1100)
        .map(|index| {
            json!({
                "type":"subAgentActivity","id":format!("spawn-{index}"),"kind":"started",
                "agentThreadId":format!("child-{index}")
            })
        })
        .collect();
    let response = json!({"id":2,"result":{"model":"stub","thread":{"id":"main",
        "turns":[{"id":"old-turn","items":items}]}}});
    let script = format!(
        r#"#!/bin/sh
case "$1" in --version) echo 'codex-cli 0.153.4'; exit 0;; esac
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*) echo '{{"id":1,"result":{{}}}}' ;;
    *'"method":"thread/resume"'*) echo '{response}' ;;
  esac
done
"#
    );
    fs::write(&program, script).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let session = CodexSession::spawn(CodexConfig {
        program: program.display().to_string(),
        resume: Some("main".into()),
        ..Default::default()
    })
    .expect("spawn must return before the bounded event channel is drained");
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut discovered = 0;
    while discovered < 1100 {
        let event = session
            .events()
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("all compact child summaries survive the read/cache capacity");
        if matches!(event, SessionEvent::Activity(ActivityEvent::Discovered(_))) {
            discovered += 1;
        }
    }
}
