//! Approval ID round-trips through the real Session transport and a local
//! stub process. No provider inference or network access.
#![cfg(unix)]

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ferrite_core::providers::{CodexConfig, CodexSession};
use ferrite_core::{DecisionAnswer, SessionEvent};
use serde_json::{json, Value};

struct Stub {
    directory: PathBuf,
    program: PathBuf,
    log: PathBuf,
}

impl Stub {
    fn new(ids: &[Value]) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "ferrite-codex-decision-ids-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let program = directory.join("codex");
        let stream = directory.join("provider.jsonl");
        let log = directory.join("host.jsonl");
        let mut frames = vec![
            json!({"id": 1, "result": {"userAgent": "stub"}}),
            json!({"id": 2, "result": {
                "thread": {"id": "stub-thread"}, "model": "stub-model",
                "approvalPolicy": "on-request", "sandbox": {"type": "readOnly"}
            }}),
        ];
        frames.extend(ids.iter().enumerate().map(|(index, id)| {
            json!({
                "id": id,
                "method": if index % 2 == 0 {
                    "item/commandExecution/requestApproval"
                } else {
                    "item/fileChange/requestApproval"
                },
                "params": {
                    "threadId": "stub-thread", "turnId": "stub-turn",
                    "itemId": format!("item-{index}")
                }
            })
        }));
        fs::write(
            &stream,
            frames
                .iter()
                .map(|frame| format!("{frame}\n"))
                .collect::<String>(),
        )
        .unwrap();
        fs::write(&program, format!(
            "#!/bin/sh\ncase \"$1\" in --version) echo 'codex-cli 0.149.1'; exit 0;; esac\ncat {}\nexec cat > {}\n",
            shell_path(&stream), shell_path(&log)
        )).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            directory,
            program,
            log,
        }
    }

    fn replies(&self) -> Vec<Value> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|frame| frame.get("result").is_some())
            .collect()
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn shell_path(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

#[test]
fn simultaneous_numeric_and_string_decisions_reply_with_their_original_ids() {
    let ids = vec![
        json!(0),
        json!("0"),
        json!(3),
        json!("3"),
        json!(4),
        json!("4"),
        json!(-1),
        json!("-1"),
        json!(i64::MAX),
        json!(""),
        json!("null"),
        json!("request \"quoted\"\n\\slash"),
        json!("\"0\""),
    ];
    let stub = Stub::new(&ids);
    let mut session = CodexSession::spawn(CodexConfig {
        program: stub.program.display().to_string(),
        ..Default::default()
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut decisions = Vec::new();
    while decisions.len() < ids.len() {
        if let SessionEvent::DecisionRequested { decision } = session
            .events()
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("all simultaneous Decisions must arrive")
        {
            decisions.push(decision);
        }
    }
    assert_eq!(
        decisions
            .iter()
            .map(|decision| &decision.id)
            .collect::<HashSet<_>>()
            .len(),
        ids.len(),
        "wire IDs of different types must remain distinct pending Decisions"
    );
    for invalid_handle in ["", "raw-id", "01", "null", "true", "[]", "{}"] {
        assert_eq!(
            session
                .respond_to_decision(invalid_handle, DecisionAnswer::Allow { input: Value::Null })
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidInput,
            "invalid handles must fail before writing any reply"
        );
    }
    for decision in decisions.iter().rev() {
        session
            .respond_to_decision(
                &decision.id,
                DecisionAnswer::Allow {
                    input: decision.input.clone(),
                },
            )
            .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let replies = loop {
        let replies = stub.replies();
        if replies.len() == ids.len() {
            break replies;
        }
        assert!(
            Instant::now() < deadline,
            "missing host replies: {replies:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        replies
            .iter()
            .map(|frame| frame["id"].clone())
            .collect::<Vec<_>>(),
        ids.into_iter().rev().collect::<Vec<_>>()
    );
    assert!(replies
        .iter()
        .all(|frame| frame["result"]["decision"] == "accept"));
}
