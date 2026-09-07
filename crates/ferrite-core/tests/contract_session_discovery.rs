#![cfg(unix)]
use ferrite_core::providers::codex_sessions;
use serde_json::json;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn native_session_discovery_paginates_without_creating_conversations() {
    let dir = std::env::temp_dir().join(format!("ferrite-native-discovery-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let program = dir.join("codex");
    let log = dir.join("requests");
    let pages = [
        json!({"id":1,"result":{}}),
        json!({"id":2,"result":{"data":[{"id":"native-first","name":"Investigate startup","preview":"first prompt","cwd":"/workspace/a","path":"/sessions/first.jsonl","updatedAt":1000,"createdAt":900}],"nextCursor":"page-two"}}),
        json!({"id":3,"result":{"data":[{"id":"native-second","name":null,"preview":"Fix search","cwd":"/workspace/b","path":"/sessions/second.jsonl","updatedAt":800,"createdAt":700}],"nextCursor":null}}),
    ];
    let script = format!("#!/bin/sh\ncase \"$1\" in --version) echo 'codex-cli 0.153.4'; exit 0;; esac\nwhile IFS= read -r request; do\nprintf '%s\\n' \"$request\" >> '{}'\ncase \"$request\" in\n*'\"id\":1'*) echo '{}' ;;\n*'\"id\":2'*) echo '{}' ;;\n*'\"id\":3'*) echo '{}' ;;\nesac\ndone\n", log.display(), pages[0], pages[1], pages[2]);
    fs::write(&program, script).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    let rows = codex_sessions(program.to_str().unwrap(), 20).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].title.as_deref(), Some("Investigate startup"));
    assert_eq!(rows[1].title.as_deref(), Some("Fix search"));
    assert_eq!(rows[0].session_id.as_deref(), Some("native-first"));
    assert_eq!(
        rows[0].cwd.as_deref(),
        Some(std::path::Path::new("/workspace/a"))
    );
    assert_eq!(rows[0].path, std::path::Path::new("/sessions/first.jsonl"));
    let requests: Vec<serde_json::Value> = fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(requests.iter().all(|r| matches!(
        r["method"].as_str(),
        Some("initialize" | "initialized" | "thread/list")
    )));
    let lists: Vec<_> = requests
        .iter()
        .filter(|r| r["method"] == "thread/list")
        .collect();
    assert_eq!(lists.len(), 2);
    assert_eq!(lists[1]["params"]["cursor"], "page-two");
    assert_eq!(lists[0]["params"]["sortKey"], "updated_at");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn claude_store_discovery_uses_native_titles_and_excludes_subagents() {
    let dir = std::env::temp_dir().join(format!("ferrite-claude-discovery-{}", std::process::id()));
    fs::create_dir_all(dir.join("project/subagents")).unwrap();
    let body = concat!(
        "{\"type\":\"user\",\"sessionId\":\"main-session\",\"cwd\":\"/workspace/ferrite\",\"message\":{\"role\":\"user\",\"content\":\"Fix startup\"}}\n",
        "{\"type\":\"custom-title\",\"sessionId\":\"main-session\",\"customTitle\":\"Startup investigation\"}\n"
    );
    fs::write(dir.join("project/main-session.jsonl"), body).unwrap();
    fs::write(dir.join("project/subagents/agent-child.jsonl"), body).unwrap();
    let rows = ferrite_core::providers::claude_sessions(&dir, 20).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title.as_deref(), Some("Startup investigation"));
    assert_eq!(rows[0].session_id.as_deref(), Some("main-session"));
    assert_eq!(
        rows[0].cwd.as_deref(),
        Some(std::path::Path::new("/workspace/ferrite"))
    );
    fs::remove_dir_all(dir).unwrap();
}
