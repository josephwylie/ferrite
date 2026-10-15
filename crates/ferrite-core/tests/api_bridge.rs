//! `ferrited api-bridge`, the binary itself (ADR 0027): the CLI's
//! `--machine` call carried over its stdio, as `ssh` would run it, to a
//! running `ferrited server`'s API and back.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ferrite_core::machine::{Dial, Tunnel};

fn ferrited(store: &Path, word: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ferrited"));
    command
        .arg(word)
        .env("FERRITE_STORE", store)
        // A terminal's launch: the login shell is not asked.
        .env("TERM", "dumb");
    command
}

#[test]
fn the_api_bridge_carries_a_cli_call_to_the_server() {
    let root = std::env::temp_dir().join(format!("ferrite-api-bridge-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let store = root.join("threads");
    std::fs::create_dir_all(&store).unwrap();
    let mut server = ferrited(&store, "server").spawn().expect("ferrited starts");
    let deadline = Instant::now() + Duration::from_secs(20);
    while ferrite_core::link::connect(&ferrite_core::api::address(&store)).is_err() {
        assert!(Instant::now() < deadline, "the API never listened");
        std::thread::sleep(Duration::from_millis(50));
    }

    let bridge_store = store.clone();
    let dial: Dial = Box::new(move || {
        let mut child = ferrited(&bridge_store, "api-bridge")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        Ok(Tunnel {
            from: Box::new(child.stdout.take().unwrap()),
            to: Box::new(child.stdin.take().unwrap()),
            ended: Box::new(move || {
                let _ = child.wait();
                None
            }),
        })
    });
    let request = serde_json::from_value(serde_json::json!({
        "id": "1", "method": "thread.list", "params": {}
    }))
    .unwrap();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = ferrite_core::api::cli::call_machine(&request, "box", &dial, &mut out, &mut err);
    let said = String::from_utf8_lossy(&out);
    assert_eq!(status, 0, "{}", String::from_utf8_lossy(&err));
    let answer: serde_json::Value = serde_json::from_str(said.trim()).unwrap();
    assert_eq!(answer["id"], "1");
    assert_eq!(
        answer["result"]["threads"],
        serde_json::json!([]),
        "{answer}"
    );

    let _ = server.kill();
    let _ = server.wait();
    let _ = std::fs::remove_dir_all(&root);
}
