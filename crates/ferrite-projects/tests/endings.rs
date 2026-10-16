//! Recorded endings of real agent conversations (tests/fixtures/endings): a provider error is read from the
//! conversation file and told apart as one that passes (tried again) or one waiting cannot fix (a missing model, a
//! login); a finished turn is still a finished turn.
use std::path::Path;
use sw::agents::provider_error::provider_error;
use sw::agents::read_turn::{read_turn, Turn};

fn read(kind: &str, name: &str) -> Turn { read_turn(kind, &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/endings").join(format!("{name}.jsonl")), 1 << 20).unwrap_or_else(|| panic!("{name}: no turn read")) }
fn re(s: &str) -> regex::Regex { regex::Regex::new(s).unwrap() }

#[test]
fn a_provider_error_is_read_and_told_apart_as_passing_or_not() {
    let passing = [("codex", "codex-at-capacity", "^Selected model is at capacity", "model is at capacity"), ("pi", "pi-socket-closed", "socket connection was closed", "connection dropped"),
        ("pi", "pi-overloaded", "currently overloaded", "provider is overloaded"), ("pi", "pi-gateway-502", r"API error \(502\)", "provider had a server error"), ("pi", "pi-stream-ended", "stream ended", "connection dropped"),
        ("claude", "claude-overloaded-made", "529", "provider is overloaded"), ("claude", "claude-rate-limit-made", "429", "model is rate limited")];
    for (kind, name, error, reason) in passing {
        let t = read(kind, name);
        assert!(re(error).is_match(&t.error), "{name}: {}", t.error); assert_eq!(t.stop, "error", "{name}");
        assert_eq!((t.passing, t.reason.as_str(), t.auth, t.finished), (true, reason, false, false), "{name}");
    }
    let cap = read("codex", "codex-at-capacity");
    assert_eq!((cap.said.as_str(), cap.id.as_str()), ("Selected model is at capacity. Please try a different model.", "01a13034-87eb-7913-81ef-e5142693d7bb"));
    for (kind, name, error) in [("codex", "codex-model-not-supported", "not supported"), ("pi", "pi-unknown-provider", "Unknown provider"), ("pi", "pi-flagged", "flagged"), ("claude", "claude-model-not-found", "may not exist")] {
        let t = read(kind, name);
        assert!(re(error).is_match(&t.error), "{name}: {}", t.error);
        assert_eq!((t.passing, t.reason.as_str(), t.auth), (false, "", false), "{name}");
    }
    let login = read("codex", "codex-unauthorized");
    assert!(login.error.contains("401 Unauthorized")); assert_eq!((login.passing, login.auth), (false, true));
    let done = read("codex", "codex-finished-made");
    assert_eq!((done.finished, done.error.as_str()), (true, ""));
    // the program's own code decides where it has one; an error nobody recognises is not retried
    let p = provider_error("boom", "server_overloaded", 0); assert_eq!((p.passing, p.reason.as_str(), p.auth), (true, "provider is overloaded", false));
    assert!(!provider_error("stream closed", "usage_limit_exceeded", 0).passing);
    assert!(!provider_error("You've hit your session limit · resets 3pm", "rate_limit", 429).passing);
    assert!(!provider_error("something odd happened", "", 0).passing);
    for (code, status) in [("rate_limit", 429), ("other", 0), ("", 0)] { let p = provider_error("Rate limit reached. Please try again in 2 seconds.", code, status); assert_eq!((p.passing, p.reason.as_str()), (true, "model is rate limited")); }
    assert!(provider_error("Rate limit reached for gpt-6.1 on requests per min.", "", 429).passing);
    assert!(!provider_error("Claude usage limit reached. Your limit will reset at 3pm.", "rate_limit", 429).passing);
    assert!(!provider_error("Weekly limit reached", "", 0).passing);
}
