//! Public Session regression for the interleaving observed in the Codex
//! subagent captures. A real stub process and reader thread; no provider calls.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use ferrite_core::providers::{CodexConfig, CodexSession, Session};
use ferrite_core::SessionEvent;
use serde_json::{json, Value};

static NEXT_STUB_ID: AtomicU64 = AtomicU64::new(0);

struct Stub {
    directory: PathBuf,
}

impl Stub {
    fn new() -> Self {
        loop {
            let id = NEXT_STUB_ID.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir().join(format!(
                "ferrite-codex-subagent-interrupt-{}-{id}",
                std::process::id()
            ));
            // Clock resolution cannot give parallel tests distinct ownership.
            // Never share an existing directory, including a stale PID's files.
            match fs::create_dir(&directory) {
                Ok(()) => return Self { directory },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("could not create stub directory: {error}"),
            }
        }
    }

    fn spawn(&self, before_handshake: &str) -> CodexSession {
        let script = format!(
            r#"#!/bin/sh
case "$1" in --version) echo 'codex-cli 0.153.4'; exit 0;; esac
{before_handshake}
while IFS= read -r line; do
  printf '%s\n' "$line" >> '{log}'
  case "$line" in
    *'"method":"initialize"'*) echo '{{"id":1,"result":{{}}}}' ;;
    *'"method":"thread/start"'*)
      echo '{{"id":2,"result":{{"thread":{{"id":"main"}},"model":"stub-model"}}}}' ;;
    *'"text":"phase-1"'*)
      {main_started}
      {alpha_started}
      {beta_started}
      {one} ;;
    *'"text":"phase-2"'*)
      {alpha_completed}
      {two} ;;
    *'"text":"phase-3"'*)
      {beta_completed}
      {stale_completed}
      {unattributed_started}
      {unattributed_completed}
      {three} ;;
    *'"text":"phase-4"'*)
      {main_completed}
      {four} ;;
    *'"text":"flush"'*) {flushed} ;;
  esac
done
"#,
            log = self.directory.join("host.jsonl").display(),
            main_started = lifecycle("turn/started", Some("main"), "main-turn"),
            alpha_started = lifecycle("turn/started", Some("alpha"), "alpha-turn"),
            beta_started = lifecycle("turn/started", Some("beta"), "beta-turn"),
            alpha_completed = lifecycle("turn/completed", Some("alpha"), "alpha-turn"),
            beta_completed = lifecycle("turn/completed", Some("beta"), "beta-turn"),
            stale_completed = lifecycle("turn/completed", Some("main"), "previous-main-turn"),
            unattributed_started = lifecycle("turn/started", None, "unknown-turn"),
            unattributed_completed = lifecycle("turn/completed", None, "unknown-turn"),
            main_completed = lifecycle("turn/completed", Some("main"), "main-turn"),
            one = marker("one"),
            two = marker("two"),
            three = marker("three"),
            four = marker("four"),
            flushed = marker("flushed"),
        );
        let program = self.directory.join("codex");
        fs::write(&program, script).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        CodexSession::spawn(CodexConfig {
            program: program.display().to_string(),
            ..Default::default()
        })
        .unwrap()
    }

    fn interrupts(&self) -> Vec<Value> {
        fs::read_to_string(self.directory.join("host.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|frame| frame["method"] == "turn/interrupt")
            .map(|frame| frame["params"].clone())
            .collect()
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn lifecycle(method: &str, thread: Option<&str>, turn: &str) -> String {
    let status = if method == "turn/started" {
        "inProgress"
    } else {
        "completed"
    };
    let mut params = json!({"turn": {"id": turn, "status": status, "items": []}});
    if let Some(thread) = thread {
        params["threadId"] = json!(thread);
    }
    format!("echo '{}'", json!({"method": method, "params": params}))
}

fn marker_token(text: &str) -> u64 {
    text.bytes().map(u64::from).sum()
}
fn marker(text: &str) -> String {
    // Usage is valid after turn completion; a late content delta is stale
    // work, so it cannot serve as the reader's synchronization checkpoint.
    format!(
        "echo '{}'",
        json!({"method":"thread/tokenUsage/updated","params":{
            "threadId":"main", "tokenUsage":{"last":{"totalTokens":marker_token(text)},"total":{"totalTokens":marker_token(text),"inputTokens":0,"cachedInputTokens":0,"outputTokens":0,"reasoningOutputTokens":0},"modelContextWindow":200000}
        }})
    )
}

fn wait_for(session: &dyn Session, marker: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let event = session
            .events()
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_else(|e| panic!("no checkpoint {marker}: {e}"));
        if matches!(event, SessionEvent::TokenUsage { total_tokens, .. } if total_tokens == marker_token(marker))
        {
            return;
        }
        assert!(
            !matches!(event, SessionEvent::Closed { .. }),
            "stub closed before {marker}"
        );
    }
}

#[test]
fn child_lifecycle_cannot_replace_or_retire_mains_interrupt_target() {
    let stub = Stub::new();
    let mut session: Box<dyn Session> = Box::new(stub.spawn(""));
    session.send("phase-1").unwrap();
    for (checkpoint, next) in [("one", "phase-2"), ("two", "phase-3"), ("three", "phase-4")] {
        wait_for(session.as_ref(), checkpoint);
        session.interrupt().unwrap();
        session.send(next).unwrap();
    }
    wait_for(session.as_ref(), "four");
    session.interrupt().unwrap(); // Matching Main completion makes this a no-op.
    session.send("flush").unwrap();
    wait_for(session.as_ref(), "flushed"); // All preceding writes reached the stub.
    assert_eq!(
        stub.interrupts(),
        vec![json!({"threadId": "main", "turnId": "main-turn"}); 3]
    );
}

#[test]
fn handshake_selects_main_from_interleaved_early_turns() {
    let stub = Stub::new();
    let early = [
        lifecycle("turn/started", Some("main"), "early-main-turn"),
        lifecycle("turn/started", Some("alpha"), "early-alpha-turn"),
        lifecycle("turn/completed", Some("alpha"), "early-alpha-turn"),
        lifecycle("turn/started", Some("beta"), "early-beta-turn"),
    ]
    .join("\n");
    let mut session: Box<dyn Session> = Box::new(stub.spawn(&early));
    session.interrupt().unwrap();
    session.send("flush").unwrap();
    wait_for(session.as_ref(), "flushed");
    assert_eq!(
        stub.interrupts(),
        vec![json!({"threadId": "main", "turnId": "early-main-turn"})]
    );
}

#[test]
fn overflowing_early_candidates_never_falls_back_to_a_child() {
    let stub = Stub::new();
    let mut early: Vec<String> = (0..128)
        .map(|index| {
            lifecycle(
                "turn/started",
                Some(&format!("child-{index}")),
                &format!("child-turn-{index}"),
            )
        })
        .collect();
    early.push(lifecycle("turn/started", Some("main"), "early-main-turn"));
    let mut session: Box<dyn Session> = Box::new(stub.spawn(&early.join("\n")));
    session.interrupt().unwrap(); // No retained Main evidence: no guessed target.
    session.send("phase-1").unwrap();
    wait_for(session.as_ref(), "one");
    session.interrupt().unwrap(); // New scoped evidence restores the target.
    session.send("flush").unwrap();
    wait_for(session.as_ref(), "flushed");
    assert_eq!(
        stub.interrupts(),
        vec![json!({"threadId": "main", "turnId": "main-turn"})]
    );
}
