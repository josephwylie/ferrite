//! A CLI session file a window offers to import (ADR 0022): which
//! vendor's root it was found under, and what it says of itself, and
//! where the vendors keep them, walked by the Server of the machine that
//! has them (`list_sessions`, ADR 0049). Adopting one is `import`'s.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::provider::Provider;

/// One session file discovery found: which vendor's root it was under, and
/// when the vendor last wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub provider: Provider,
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
    pub title: Option<String>,
    pub cwd: Option<PathBuf>,
    pub session_id: Option<String>,
}

/// What a machine's listing found: its sessions, newest first, and the
/// folders it looked in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub sessions: Vec<Candidate>,
    pub roots: Vec<PathBuf>,
}

/// Where the vendors write session files, per the layouts the import
/// fixtures capture: `~/.claude/projects/<slug>/<session>.jsonl` and
/// `~/.codex/sessions/<date dirs>/rollout-*.jsonl`. Windows spells the
/// home directory USERPROFILE, not HOME.
pub fn default_roots() -> Vec<(Provider, PathBuf)> {
    let home = PathBuf::from(
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".into()),
    );
    vec![
        (Provider::Claude, home.join(".claude").join("projects")),
        (Provider::Codex, home.join(".codex").join("sessions")),
    ]
}

/// Candidate session files under the vendors' roots: every `.jsonl` in
/// either tree — the layouts differ (per-project slugs vs per-date
/// directories), so the walk recurses instead of assuming a depth, the
/// same read the live import probes use — newest first, capped. A missing
/// root lists nothing: that vendor was simply never run here. Whether a
/// candidate really is an adoptable session stays the parser's verdict,
/// not the filename's.
pub fn candidates(roots: &[(Provider, PathBuf)], cap: usize) -> Vec<Candidate> {
    fn walk(provider: Provider, dir: &Path, into: &mut Vec<Candidate>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(provider, &path, into);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
            {
                into.push(Candidate {
                    provider,
                    title: None,
                    cwd: None,
                    session_id: None,
                    modified: std::fs::metadata(&path)
                        .and_then(|metadata| metadata.modified())
                        .ok(),
                    path,
                });
            }
        }
    }
    let mut found = Vec::new();
    for (provider, root) in roots {
        walk(*provider, root, &mut found);
    }
    // Newest first; a file with no readable mtime sorts oldest rather than
    // vanishing.
    found.sort_by_key(|candidate| std::cmp::Reverse(candidate.modified));
    found.truncate(cap);
    found
}
