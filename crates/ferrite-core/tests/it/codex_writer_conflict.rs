//! A resume that meets the thread's previous writer still alive — the
//! app-server of a Ferrite that is quitting — waits it out instead of failing
//! the Session; any other refusal fails at once.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use ferrite_core::providers::{CodexConfig, CodexSession};

static NEXT_STUB: AtomicU64 = AtomicU64::new(0);

/// A stub app-server that refuses `thread/resume` with the given error for
/// the first `refusals` processes spawned against it, counting processes in
/// a file because every retry is a fresh process.
struct Stub {
    dir: PathBuf,
}

impl Stub {
    fn new(refusals: u32, message: &str) -> Self {
        let dir = loop {
            let id = NEXT_STUB.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("ferrite-codex-writer-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => break path,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create stub: {error}"),
            }
        };
        let counter = dir.join("spawns");
        let script = format!(
            r#"#!/bin/sh
case "$1" in --version) echo 'codex-cli 0.153.4'; exit 0;; esac
n=$(cat '{counter}' 2>/dev/null || echo 0)
n=$((n + 1))
echo "$n" > '{counter}'
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*) echo '{{"id":1,"result":{{}}}}' ;;
    *'"method":"skills/list"'*) echo '{{"id":3,"result":{{"data":[]}}}}' ;;
    *'"method":"thread/resume"'*)
      if [ "$n" -le {refusals} ]; then
        echo '{{"id":2,"error":{{"code":-32600,"message":"{message}"}}}}'
      else
        echo '{{"id":2,"result":{{"thread":{{"id":"resumed"}},"model":"stub"}}}}'
      fi ;;
  esac
done
"#,
            counter = counter.display(),
        );
        let program = dir.join("codex");
        fs::write(&program, script).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }

    fn config(&self) -> CodexConfig {
        CodexConfig {
            program: self.dir.join("codex").display().to_string(),
            resume: Some("resumed".into()),
            ..Default::default()
        }
    }

    fn spawns(&self) -> u32 {
        fs::read_to_string(self.dir.join("spawns"))
            .map(|n| n.trim().parse().unwrap())
            .unwrap_or(0)
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

const CONFLICT: &str = "thread resumed already has an active writer";

#[test]
fn a_resume_waits_out_the_previous_writer() {
    let stub = Stub::new(2, CONFLICT);
    let session = CodexSession::spawn(stub.config()).expect("the third server answers");
    assert_eq!(stub.spawns(), 3, "one fresh server per try");
    drop(session);
}

#[test]
fn any_other_refusal_fails_at_once() {
    let stub = Stub::new(1, "thread resumed not found");
    let error = match CodexSession::spawn(stub.config()) {
        Ok(_) => panic!("the refusal stands"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("not found"), "{error}");
    assert_eq!(stub.spawns(), 1);
}
