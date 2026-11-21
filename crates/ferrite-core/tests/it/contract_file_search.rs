#![cfg(unix)]
use std::time::Duration;
use crate::support::*;

#[test]
fn native_file_search_preserves_ranking_paths_and_directory_identity() {
    for provider in ["claude", "codex"] {
        let mut replay = Replay::new(provider, vec![]);
        replay.drain();
        let (answer, reply) = ferrite_core::doorbell::sync_channel(1);
        replay.session.search_files("src", answer).unwrap();
        let rows = reply.recv_timeout(Duration::from_secs(3)).unwrap().unwrap();
        assert_eq!(rows[0].path, "src/zé.rs");
        assert!(!rows[0].is_directory);
        assert!(rows[1].is_directory);
        if provider == "codex" {
            assert_eq!(rows[0].matched, [5]);
            assert!(rows[1].matched.is_empty());
            let frame = replay.wait_host(|v| v["method"] == "fuzzyFileSearch");
            assert_eq!(frame["params"]["query"], "src");
            assert!(frame["params"]["roots"]
                .as_array()
                .is_some_and(|roots| !roots.is_empty()));
            assert!(frame["params"]["cancellationToken"]
                .as_str()
                .is_some_and(|s| !s.is_empty()));
        } else {
            assert_eq!(rows[2].path, "/external/file.md");
            // The search's own frame, not the one that built the index.
            let frame = replay.wait_host(|v| {
                v["request"]["subtype"] == "file_suggestions"
                    && v["request_id"] != "ferrite_file_index"
            });
            assert_eq!(frame["request"]["query"], "src");
        }
        let (answer, reply) = ferrite_core::doorbell::sync_channel(1);
        replay.session.search_files("next", answer).unwrap();
        reply.recv_timeout(Duration::from_secs(3)).unwrap().unwrap();
    }
}

/// The Claude CLI starts its file index at the first file search and answers
/// that one from the index as it stands, so a fresh process finds nothing.
/// The operator's first `@` on a Claude Session still lists the files.
#[test]
fn a_claude_sessions_first_file_search_lists_files() {
    let mut replay = Replay::new("claude", vec![]);
    replay.drain();
    let (answer, reply) = ferrite_core::doorbell::sync_channel(1);
    replay.session.search_files("src", answer).unwrap();
    let rows = reply.recv_timeout(Duration::from_secs(3)).unwrap().unwrap();
    let paths: Vec<_> = rows.iter().map(|row| row.path.as_str()).collect();
    assert_eq!(paths, ["src/zé.rs", "src/a/", "/external/file.md"]);
}
