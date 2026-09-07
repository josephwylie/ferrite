#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use std::time::Duration;
use support::*;

#[test]
fn native_file_search_preserves_ranking_paths_and_directory_identity() {
    for provider in ["claude", "codex"] {
        let mut replay = Replay::new(provider, vec![]);
        replay.drain();
        let reply = replay.session.search_files("src").unwrap();
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
            let frame = replay.wait_host(|v| v["request"]["subtype"] == "file_suggestions");
            assert_eq!(frame["request"]["query"], "src");
        }
        replay
            .session
            .search_files("next")
            .unwrap()
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap();
    }
}
