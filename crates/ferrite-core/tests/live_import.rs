//! Live import probes: a real CLI session's file, imported, resumed,
//! remembered. Ignored by default because they cost money, need auth, and
//! talk to vendor services.
//!
//! Run deliberately, after changing anything about the import parsers:
//! `cargo test -p ferrite-core --test live_import -- --ignored --nocapture`
//!
//! Each probe is the whole acceptance path: make a throwaway session with
//! the real CLI, find the session file the vendor wrote for it, import that
//! file into a fresh store, then spawn a new Session from the imported
//! Thread's resume target and ask for the codeword the throwaway session was
//! told — the answer only exists in the vendor's own history.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use ferrite_core::import::import;
use ferrite_core::providers::{ClaudeConfig, ClaudeSession, CodexConfig, CodexSession};
use ferrite_core::store::{Provider, Store};
use ferrite_core::{SessionEvent, TurnOutcome};

/// Generous: a real turn crosses the network and may be rate limited.
const TURN_TIMEOUT: Duration = Duration::from_secs(180);

fn scratch_store(name: &str) -> Store {
    let dir =
        std::env::temp_dir().join(format!("ferrite-live-import-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    Store::open(&dir).unwrap()
}

/// Collect until the turn ends, returning what the agent said.
fn await_turn_end(events: &Receiver<SessionEvent>) -> (TurnOutcome, String) {
    let deadline = Instant::now() + TURN_TIMEOUT;
    let mut text = String::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(SessionEvent::TextDelta { text: delta }) => text.push_str(&delta),
            Ok(SessionEvent::TurnEnded { outcome, .. }) => return (outcome, text),
            Ok(SessionEvent::Closed { reason }) => panic!("session closed mid-turn: {reason}"),
            Ok(_) => {}
            Err(e) => panic!("no turn end within {TURN_TIMEOUT:?}: {e}"),
        }
    }
}

fn await_init(events: &Receiver<SessionEvent>) -> String {
    let deadline = Instant::now() + TURN_TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(SessionEvent::Init { session_id, .. }) => return session_id,
            Ok(_) => {}
            Err(e) => panic!("no Init: {e}"),
        }
    }
}

/// The session file the vendor wrote for `session_id`, found by walking
/// `root` — the layouts differ (per-project slugs vs per-date directories),
/// but the filename carries the id in both.
fn find_session_file(root: &Path, session_id: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_session_file(&path, session_id) {
                return Some(found);
            }
        } else if path
            .file_name()
            .map(|name| name.to_string_lossy().contains(session_id))
            .unwrap_or(false)
        {
            return Some(path);
        }
    }
    None
}

/// Wait for the vendor to have written the session file: it flushes on its
/// own schedule, not Ferrite's.
fn settled_session_file(root: &Path, session_id: &str) -> PathBuf {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(path) = find_session_file(root, session_id) {
            return path;
        }
        assert!(
            Instant::now() < deadline,
            "no session file for {session_id} under {root:?}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn home(join: &str) -> PathBuf {
    // Windows spells the home directory USERPROFILE, not HOME.
    let base = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .expect("a home directory is set");
    PathBuf::from(base).join(join)
}

/// The whole claude acceptance path: session → file → import → resumed
/// Thread that remembers.
#[test]
#[ignore = "spawns the real claude CLI"]
fn an_imported_claude_session_continues_on_the_next_prompt() {
    let cwd = std::env::temp_dir().join("ferrite-live-import-claude");
    std::fs::create_dir_all(&cwd).unwrap();
    let config = ClaudeConfig {
        program: std::env::var("FERRITE_CLAUDE_BIN").unwrap_or_else(|_| "claude".into()),
        cwd: Some(cwd),
        model: Some("haiku".into()),
        ..Default::default()
    };

    // The throwaway session the operator is imagined to have had.
    let mut session = ClaudeSession::spawn(config.clone()).unwrap();
    session
        .send("Remember the codeword: ferrite-import-live-claude. Reply with exactly: saved")
        .unwrap();
    let session_id = await_init(session.events());
    let (outcome, _) = await_turn_end(session.events());
    assert_eq!(outcome, TurnOutcome::Completed);
    drop(session);

    let file = settled_session_file(&home(".claude/projects"), &session_id);
    let store = scratch_store("claude");
    let thread = import(&store, &file).unwrap();
    let snapshot = store.load(thread).unwrap();
    assert_eq!(snapshot.provider(), Provider::Claude);
    assert_eq!(snapshot.resume_target(), Some(session_id.as_str()));

    // The imported Thread's next prompt, through the provider's own resume.
    let mut revived = ClaudeSession::spawn(ClaudeConfig {
        resume: snapshot.resume_target().map(str::to_string),
        ..config
    })
    .unwrap();
    revived
        .send("What is the codeword? Reply with the codeword only.")
        .unwrap();
    let (outcome, text) = await_turn_end(revived.events());
    assert_eq!(outcome, TurnOutcome::Completed);
    assert!(
        text.contains("ferrite-import-live-claude"),
        "the imported session forgot: {text:?}"
    );
}

/// The same acceptance path for codex, through its rollout file.
#[test]
#[ignore = "spawns the real codex CLI"]
fn an_imported_codex_session_continues_on_the_next_prompt() {
    let config = CodexConfig {
        program: std::env::var("FERRITE_CODEX_BIN").unwrap_or_else(|_| "codex".into()),
        cwd: Some(std::env::temp_dir()),
        model: Some("gpt-5.4-mini".into()),
        effort: None,
        approval_policy: Some("never".into()),
        sandbox: Some("read-only".into()),
        resume: None,
    };

    let mut session = CodexSession::spawn(config.clone()).unwrap();
    let session_id = await_init(session.events());
    session
        .send("Remember the codeword: ferrite-import-live-codex. Reply with exactly: saved")
        .unwrap();
    let (outcome, _) = await_turn_end(session.events());
    assert_eq!(outcome, TurnOutcome::Completed);
    drop(session);

    let file = settled_session_file(&home(".codex/sessions"), &session_id);
    let store = scratch_store("codex");
    let thread = import(&store, &file).unwrap();
    let snapshot = store.load(thread).unwrap();
    assert_eq!(snapshot.provider(), Provider::Codex);
    assert_eq!(snapshot.resume_target(), Some(session_id.as_str()));

    let mut revived = CodexSession::spawn(CodexConfig {
        resume: snapshot.resume_target().map(str::to_string),
        ..config
    })
    .unwrap();
    revived
        .send("What is the codeword? Reply with the codeword only.")
        .unwrap();
    let (outcome, text) = await_turn_end(revived.events());
    assert_eq!(outcome, TurnOutcome::Completed);
    assert!(
        text.contains("ferrite-import-live-codex"),
        "the imported session forgot: {text:?}"
    );
}
