//! Session import: a vendor's on-disk session file becomes a Thread whose
//! next prompt continues that conversation.
//!
//! The fixtures are real session files from fresh throwaway sessions —
//! `~/.claude/projects/<slug>/<session>.jsonl` and
//! `~/.codex/sessions/<date>/rollout-*.jsonl` as the vendors actually write
//! them, redacted by `fixtures/redact_import.py`. They are the import
//! contract: a vendor release that changes the on-disk shape breaks these
//! tests loudly.

use std::fs;
use std::path::PathBuf;

use ferrite_core::import::{candidates, import, ImportError};
use ferrite_core::store::{Provider, Store};
use ferrite_core::transcript::{Input, Transcript};
use ferrite_core::{SessionEvent, TurnOutcome};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}"))
}

/// A fresh per-test store directory.
fn scratch(name: &str) -> Store {
    let dir = std::env::temp_dir().join(format!("ferrite-import-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    Store::open(&dir).unwrap()
}

/// The imported claude fixture: a three-turn `claude` 2.1.241 session
/// (codeword, recall, a Bash tool run).
const CLAUDE_SESSION_ID: &str = "e7699e43-9435-449e-b952-7df6cc3d0386";

#[test]
fn a_claude_session_file_imports_into_a_thread_that_can_resume_it() {
    let store = scratch("claude-resume");
    let thread = import(&store, &fixture("import-claude-session-2.1.241.jsonl")).unwrap();

    let snapshot = store.load(thread).unwrap();
    assert_eq!(snapshot.provider(), Provider::Claude);
    let project = snapshot
        .project_id()
        .expect("imports durably name a project");
    let registry = ferrite_core::workspace::registry::Registry::open(store.dir()).unwrap();
    assert_eq!(
        registry.project(project).unwrap().root,
        PathBuf::from("/workspace")
    );
    // The whole point: the file's own session id is what the next Session
    // resumes from — this is the value the live probe hands to
    // `ClaudeConfig::resume` and gets the conversation back.
    assert_eq!(snapshot.resume_target(), Some(CLAUDE_SESSION_ID));
}

#[test]
fn the_imported_claude_history_replays_as_the_conversation() {
    let store = scratch("claude-history");
    let thread = import(&store, &fixture("import-claude-session-2.1.241.jsonl")).unwrap();
    let inputs = store.load(thread).unwrap().inputs();

    // The operator's three prompts, in order and word for word.
    let prompts: Vec<&str> = inputs
        .iter()
        .filter_map(|input| match input {
            Input::Prompt(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        prompts,
        [
            "Remember the codeword: ferrite-import-claude. Reply with exactly: saved",
            "What is the codeword? Reply with the codeword only.",
            "Run the shell command `echo ferrite-import-tool-ok` with Bash and tell me its output.",
        ]
    );

    // The Session identity the Thread announces.
    assert!(inputs.contains(&Input::Event(SessionEvent::Init {
        session_id: CLAUDE_SESSION_ID.into(),
        model: "claude-haiku-4-5-20251001".into(),
    })));

    // This capture's four thinking blocks are all the empty strings the CLI
    // persists for redacted thinking: none of them is an empty thought.
    assert!(
        !inputs
            .iter()
            .any(|input| matches!(input, Input::Event(SessionEvent::ThinkingDelta { .. }))),
        "empty thinking imported as thoughts: {inputs:?}"
    );

    // The assistant's words, per turn.
    let text: Vec<&str> = inputs
        .iter()
        .filter_map(|input| match input {
            Input::Event(SessionEvent::TextDelta { text }) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        text,
        [
            "saved",
            "ferrite-import-claude",
            "Output: `ferrite-import-tool-ok`"
        ]
    );

    // The tool run, whole: the call and its result agree on the id.
    let started: Vec<_> = inputs
        .iter()
        .filter_map(|input| match input {
            Input::Event(SessionEvent::ToolStarted { id, name, input }) => Some((id, name, input)),
            _ => None,
        })
        .collect();
    assert_eq!(started.len(), 1, "expected one tool call: {inputs:?}");
    let (tool_id, name, input) = started[0];
    assert_eq!(name, "Bash");
    assert_eq!(input["command"], "echo ferrite-import-tool-ok");
    assert!(
        inputs.iter().any(|event| matches!(
            event,
            Input::Event(SessionEvent::ToolCompleted { id, output, is_error: false, .. })
                if id == tool_id && output == "ferrite-import-tool-ok"
        )),
        "no completion for {tool_id}: {inputs:?}"
    );

    // The session file marks no turns, so a new prompt closes the one before
    // it and the file's end closes the last: three completed turns, costs
    // unknown (the file records none).
    let ends: Vec<_> = inputs
        .iter()
        .filter_map(|input| match input {
            Input::Event(SessionEvent::TurnEnded { outcome, cost_usd }) => {
                Some((outcome, *cost_usd))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        ends,
        [
            (&TurnOutcome::Completed, None),
            (&TurnOutcome::Completed, None),
            (&TurnOutcome::Completed, None),
        ]
    );
}

/// The imported Thread must read as at rest, not mid-stream: the transcript
/// replay of the history ends Idle, exactly like a Thread revived from
/// Ferrite's own log.
#[test]
fn an_imported_thread_replays_to_an_idle_transcript() {
    let store = scratch("claude-idle");
    let thread = import(&store, &fixture("import-claude-session-2.1.241.jsonl")).unwrap();

    let mut transcript = Transcript::default();
    for input in store.load(thread).unwrap().inputs() {
        transcript.apply(input);
    }
    assert_eq!(
        transcript.status(),
        ferrite_core::transcript::Status::Idle,
        "an imported session is at rest"
    );
    assert_eq!(transcript.session_id(), Some(CLAUDE_SESSION_ID));
}

/// The imported codex fixture: a `codex` 0.149.1 rollout with a completed
/// codeword turn (reasoning summary included) and a second turn the shutdown
/// aborted — closed by the vendor's own explicit `turn_aborted` line, so the
/// file does not end mid-turn (the truncation test below covers that case).
const CODEX_THREAD_ID: &str = "01a03825-ffbc-7241-aaea-8785dae41248";

#[test]
fn a_codex_rollout_imports_into_a_thread_that_can_resume_it() {
    let store = scratch("codex-resume");
    let thread = import(&store, &fixture("import-codex-rollout-0.149.1.jsonl")).unwrap();

    let snapshot = store.load(thread).unwrap();
    assert_eq!(snapshot.provider(), Provider::Codex);
    // The rollout's thread id is what `CodexConfig::resume` sends in
    // thread/resume — proven live by the resume probes.
    assert_eq!(snapshot.resume_target(), Some(CODEX_THREAD_ID));
}

#[test]
fn the_imported_codex_history_replays_as_the_conversation() {
    let store = scratch("codex-history");
    let thread = import(&store, &fixture("import-codex-rollout-0.149.1.jsonl")).unwrap();
    let inputs = store.load(thread).unwrap().inputs();

    assert!(inputs.contains(&Input::Event(SessionEvent::Init {
        session_id: CODEX_THREAD_ID.into(),
        model: "gpt-5.4-mini".into(),
    })));
    let prompts = inputs
        .iter()
        .filter(|input| matches!(input, Input::Prompt(_)))
        .count();
    assert_eq!(prompts, 2);

    // Codex's own concepts arrive typed: the reasoning summary the rollout
    // recorded, and token accounting against the context window.
    assert!(
        inputs.contains(&Input::Event(SessionEvent::ReasoningSummaryDelta {
            text: "**Confirming exact response \"saved\"**".into(),
            summary_index: 0,
        }))
    );
    assert!(inputs.contains(&Input::Event(SessionEvent::TokenUsage {
        total_tokens: 15044,
        input_tokens: 15003,
        cached_input_tokens: 4480,
        output_tokens: 41,
        reasoning_output_tokens: 34,
        context_window: Some(258400),
    })));
    assert!(inputs.contains(&Input::Event(SessionEvent::TextDelta {
        text: "saved".into()
    })));

    // Turn one completed in so many words (task_complete); the file's second
    // turn ends with the vendor's own turn_aborted — an interrupted turn,
    // not an invented completion.
    let ends: Vec<&TurnOutcome> = inputs
        .iter()
        .filter_map(|input| match input {
            Input::Event(SessionEvent::TurnEnded { outcome, .. }) => Some(outcome),
            _ => None,
        })
        .collect();
    assert_eq!(ends, [&TurnOutcome::Completed, &TurnOutcome::Interrupted]);
}

#[test]
fn imported_context_usage_is_the_latest_window_not_lifetime_spend() {
    let store = scratch("codex-current-context");
    let original = fs::read_to_string(fixture("import-codex-rollout-0.149.1.jsonl")).unwrap();
    let lines: Vec<String> = original
        .lines()
        .map(|line| {
            let mut value: serde_json::Value = serde_json::from_str(line).unwrap();
            if value["payload"]["type"] == "token_count" {
                value["payload"]["info"]["last_token_usage"]["total_tokens"] = 31000.into();
                value["payload"]["info"]["total_token_usage"]["total_tokens"] = 900000.into();
            }
            value.to_string()
        })
        .collect();
    let path = store.dir().join("context-rollout.jsonl");
    fs::write(&path, lines.join("\n")).unwrap();
    let thread = import(&store, &path).unwrap();
    let mut transcript = Transcript::default();
    for input in store.load(thread).unwrap().inputs() {
        transcript.apply(input);
    }
    let usage = transcript.usage().unwrap();
    assert_eq!(usage.total_tokens, 31000);
    assert_eq!(usage.context_window, Some(258400));
}

/// The tool-bearing rollout: an exec_command round trip imports as the tool
/// events the wire would have carried.
#[test]
fn an_imported_codex_command_arrives_as_a_tool_round_trip() {
    let store = scratch("codex-tool");
    let thread = import(&store, &fixture("import-codex-rollout-tool-0.149.1.jsonl")).unwrap();
    let inputs = store.load(thread).unwrap().inputs();

    let started: Vec<_> = inputs
        .iter()
        .filter_map(|input| match input {
            Input::Event(SessionEvent::ToolStarted { id, name, input }) => Some((id, name, input)),
            _ => None,
        })
        .collect();
    assert_eq!(started.len(), 1, "expected one command: {inputs:?}");
    let (tool_id, name, input) = started[0];
    assert_eq!(name, "exec_command");
    assert_eq!(input["cmd"], "echo ferrite-tool-ok");
    assert!(
        inputs.iter().any(|event| matches!(
            event,
            Input::Event(SessionEvent::ToolCompleted { id, output, is_error: false, .. })
                if id == tool_id && output.contains("ferrite-tool-ok")
        )),
        "no completion for {tool_id}: {inputs:?}"
    );
    assert!(inputs.contains(&Input::Event(SessionEvent::TurnEnded {
        outcome: TurnOutcome::Completed,
        cost_usd: None,
    })));
}

/// The second claude fixture: the same session driven the way harnesses
/// (Ferrite included) drive it — stream-json input, so the file records the
/// prompt as an array of content blocks — plus a custom slash command turn,
/// whose command file the CLI injects as an `isMeta` user line.
const STREAM_SESSION: &str = "import-claude-stream-session-2.1.241.jsonl";
const STREAM_SESSION_ID: &str = "e9118136-315b-45f1-a419-560e93c20f3e";

/// A prompt sent as content blocks is still the operator speaking: the
/// array-form user line imports as the prompt text, word for word.
#[test]
fn an_array_content_prompt_imports_as_the_prompt_text() {
    let store = scratch("array-prompt");
    let thread = import(&store, &fixture(STREAM_SESSION)).unwrap();
    let snapshot = store.load(thread).unwrap();
    assert_eq!(snapshot.resume_target(), Some(STREAM_SESSION_ID));

    let inputs = snapshot.inputs();
    assert_eq!(
        inputs.iter().find_map(|input| match input {
            Input::Prompt(text) => Some(text.as_str()),
            _ => None,
        }),
        Some("Remember the codeword: ferrite-import-meta. Reply with exactly: saved"),
        "history: {inputs:?}"
    );
    // This capture also carries real (non-empty) thinking; it must survive.
    assert!(
        inputs
            .iter()
            .any(|input| matches!(input, Input::Event(SessionEvent::ThinkingDelta { .. }))),
        "the thinking went missing: {inputs:?}"
    );
}

/// The CLI marks its own injected lines as meta — a slash command's file
/// content arrives as an `isMeta` user line. That was never the operator
/// speaking: the import keeps the operator's own command invocation and the
/// agent's reply, and nothing else pretends to be a prompt.
#[test]
fn an_injected_meta_line_is_not_the_operator_speaking() {
    let store = scratch("meta-line");
    let thread = import(&store, &fixture(STREAM_SESSION)).unwrap();
    let inputs = store.load(thread).unwrap().inputs();

    let prompts: Vec<&str> = inputs
        .iter()
        .filter_map(|input| match input {
            Input::Prompt(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        prompts,
        [
            "Remember the codeword: ferrite-import-meta. Reply with exactly: saved",
            "<command-message>ferrite-meta</command-message>\n<command-name>/ferrite-meta</command-name>",
        ],
        "the injected command content must not appear as a prompt"
    );
    // The turn the command started still completed with its reply.
    assert!(inputs.contains(&Input::Event(SessionEvent::TextDelta {
        text: "meta-ok".into()
    })));
    let ends = inputs
        .iter()
        .filter(|input| matches!(input, Input::Event(SessionEvent::TurnEnded { .. })))
        .count();
    assert_eq!(ends, 2, "history: {inputs:?}");
}

/// Detection is not trust: a file that opens with a genuine marker line and
/// then collapses into garbage exercises the parser interiors, which must
/// refuse it — a Thread with no conversation would be nothing to continue —
/// without panicking and without leaving a Thread behind.
#[test]
fn a_valid_marker_followed_by_junk_is_refused_by_the_parser_itself() {
    let store = scratch("marker-junk");
    let dir = std::env::temp_dir().join(format!("ferrite-import-marker-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();

    // Real marker lines, lifted from the committed fixtures; junk after.
    let claude_marker = r#"{"type":"queue-operation","operation":"dequeue","sessionId":"e7699e43-9435-449e-b952-7df6cc3d0386","timestamp":"2026-08-25T09:00:00.000Z"}"#;
    let codex_marker = r#"{"timestamp":"2026-08-25T08:30:09.949Z","type":"session_meta","payload":{"id":"01a03825-ffbc-7241-aaea-8785dae41248","timestamp":"2026-08-25T08:30:09.949Z","cwd":"/workspace","originator":"ferrite","cli_version":"0.149.1","source":"vscode"}}"#;
    for (name, marker) in [("claude", claude_marker), ("codex", codex_marker)] {
        let path = dir.join(format!("{name}-then-junk.jsonl"));
        let mut bytes = format!("{marker}\n").into_bytes();
        bytes.extend_from_slice(b"{\"type\":\"resp");
        bytes.extend_from_slice(&[0xff, 0xfe, 0x00, b'\n']);
        bytes.extend_from_slice(b"complete garbage, not json\n");
        fs::write(&path, &bytes).unwrap();

        match import(&store, &path) {
            Err(ImportError::Unrecognized { detail }) => assert!(
                !detail.is_empty(),
                "{name}: the reason must be sayable to the operator"
            ),
            other => panic!("{name}: expected Unrecognized, got {other:?}"),
        }
    }
    assert_eq!(store.thread_ids().unwrap(), vec![]);
}

/// A subagent's transcript is a separate file whose every line is marked
/// `isSidechain:true` — and stamped with the PARENT session's id. Adopting
/// one would make a Thread that claims the parent conversation as its resume
/// target while showing the subagent's exchange: refused instead, as a file
/// with no operator conversation in it. (On 2.1.241 the main session file
/// carries no sidechain lines — they live under `<session>/subagents/` — but
/// the skip also covers any vintage that interleaves them.)
#[test]
fn a_subagent_transcript_is_refused_not_adopted_as_the_parent_session() {
    let store = scratch("subagent");
    match import(&store, &fixture("import-claude-subagent-2.1.241.jsonl")) {
        Err(ImportError::Unrecognized { detail }) => assert!(
            detail.contains("no conversation"),
            "the reason should say what is missing: {detail}"
        ),
        other => panic!("expected Unrecognized, got {other:?}"),
    }
    assert_eq!(store.thread_ids().unwrap(), vec![]);
}

/// A CLI killed while a tool ran: the file ends on the `tool_use` with no
/// result. That turn never finished — it closes Interrupted, not as an
/// invented completion. The case is the committed fixture truncated at the
/// tool call, the same style as the store's torn-tail tests.
#[test]
fn a_file_ending_inside_a_tool_call_imports_as_an_interrupted_turn() {
    let store = scratch("mid-tool");
    let bytes = fs::read(fixture("import-claude-session-2.1.241.jsonl")).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    // The exact block marker, not the bare word: assistant usage metadata
    // mentions server_tool_use on unrelated lines.
    let mut kept = String::new();
    for line in text.lines() {
        kept.push_str(line);
        kept.push('\n');
        if line.contains(r#""type":"tool_use""#) {
            break;
        }
    }
    assert!(
        kept.contains(r#""type":"tool_use""#),
        "the fixture lost its tool call"
    );

    let dir = std::env::temp_dir().join(format!("ferrite-import-midtool-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mid-tool.jsonl");
    fs::write(&path, kept).unwrap();

    let thread = import(&store, &path).unwrap();
    let inputs = store.load(thread).unwrap().inputs();
    let ends: Vec<&TurnOutcome> = inputs
        .iter()
        .filter_map(|input| match input {
            Input::Event(SessionEvent::TurnEnded { outcome, .. }) => Some(outcome),
            _ => None,
        })
        .collect();
    assert_eq!(
        ends,
        [
            &TurnOutcome::Completed,
            &TurnOutcome::Completed,
            &TurnOutcome::Interrupted
        ],
        "history: {inputs:?}"
    );
}

/// A rollout genuinely cut off inside a turn — no `turn_aborted`, no
/// `task_complete`, the file just stops. This is what proves the codex
/// parser's end-of-file rule; the committed fixture's own aborted turn is
/// closed by an explicit line and never reaches it.
#[test]
fn a_codex_rollout_cut_off_inside_a_turn_closes_it_as_interrupted() {
    let store = scratch("codex-cut");
    let bytes = fs::read(fixture("import-codex-rollout-0.149.1.jsonl")).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let kept: String = text
        .lines()
        .take_while(|line| !line.contains("turn_aborted"))
        .flat_map(|line| [line, "\n"])
        .collect();
    assert!(
        kept.contains("task_complete"),
        "the cut must keep the completed first turn"
    );

    let dir = std::env::temp_dir().join(format!("ferrite-import-codexcut-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cut.jsonl");
    fs::write(&path, kept).unwrap();

    let thread = import(&store, &path).unwrap();
    let inputs = store.load(thread).unwrap().inputs();
    let ends: Vec<&TurnOutcome> = inputs
        .iter()
        .filter_map(|input| match input {
            Input::Event(SessionEvent::TurnEnded { outcome, .. }) => Some(outcome),
            _ => None,
        })
        .collect();
    assert_eq!(
        ends,
        [&TurnOutcome::Completed, &TurnOutcome::Interrupted],
        "history: {inputs:?}"
    );
}

/// A session killed before its reply: the file ends on the prompt. The
/// imported turn is honestly interrupted — never a Thread stuck rendering
/// "streaming" for a process that died months ago. (Line shapes are the real
/// ones, cut down to the case.)
#[test]
fn a_file_ending_on_an_unanswered_prompt_imports_as_an_interrupted_turn() {
    let store = scratch("cut-off");
    let dir = std::env::temp_dir().join(format!("ferrite-import-cutoff-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cut-off.jsonl");
    fs::write(
        &path,
        concat!(
            r#"{"type":"user","sessionId":"cut-off-4f2a","cwd":"/workspace","message":{"role":"user","content":"first question"}}"#,
            "\n",
            r#"{"type":"assistant","sessionId":"cut-off-4f2a","message":{"model":"claude-haiku-4-5","content":[{"type":"text","text":"first answer"}]}}"#,
            "\n",
            r#"{"type":"user","sessionId":"cut-off-4f2a","cwd":"/workspace","message":{"role":"user","content":"second question, never answered"}}"#,
            "\n",
        ),
    )
    .unwrap();

    let thread = import(&store, &path).unwrap();
    let inputs = store.load(thread).unwrap().inputs();
    let ends: Vec<&TurnOutcome> = inputs
        .iter()
        .filter_map(|input| match input {
            Input::Event(SessionEvent::TurnEnded { outcome, .. }) => Some(outcome),
            _ => None,
        })
        .collect();
    assert_eq!(
        ends,
        [&TurnOutcome::Completed, &TurnOutcome::Interrupted],
        "history: {inputs:?}"
    );

    let mut transcript = Transcript::default();
    for input in inputs {
        transcript.apply(input);
    }
    assert_eq!(transcript.status(), ferrite_core::transcript::Status::Idle);
}

#[test]
fn a_missing_file_is_an_io_error_with_the_os_reason() {
    let store = scratch("missing");
    match import(&store, &fixture("no-such-session.jsonl")) {
        Err(ImportError::Io(e)) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
        other => panic!("expected Io(NotFound), got {other:?}"),
    }
}

/// Foreign and damaged files are refused with a readable reason, never a
/// crash — including Ferrite's own Thread logs, which are JSONL too.
#[test]
fn foreign_files_are_refused_with_a_readable_reason() {
    let store = scratch("foreign");
    let dir = std::env::temp_dir().join(format!("ferrite-import-foreign-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();

    for (name, contents) in [
        ("empty.jsonl", "".to_string()),
        (
            "prose.txt",
            "not a session file at all\njust words\n".to_string(),
        ),
        (
            "ferrite-log.jsonl",
            concat!(
                r#"{"schema":2,"provider":"claude"}"#,
                "\n",
                r#"{"type":"init","session_id":"x","model":"m"}"#,
                "\n"
            )
            .to_string(),
        ),
        (
            "unrelated.json",
            r#"{"widgets":[1,2,3],"name":"config"}"#.to_string(),
        ),
    ] {
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        match import(&store, &path) {
            Err(ImportError::Unrecognized { detail }) => assert!(
                !detail.is_empty(),
                "{name}: the reason must be sayable to the operator"
            ),
            other => panic!("{name}: expected Unrecognized, got {other:?}"),
        }
    }
    // Nothing half-imported: refused files must not leave Threads behind.
    assert_eq!(store.thread_ids().unwrap(), vec![]);
}

/// A session file that names no session cannot be continued — the one thing
/// an import exists to do — so it is refused, not half-adopted.
#[test]
fn a_session_file_without_a_session_id_is_refused() {
    let store = scratch("no-id");
    let dir = std::env::temp_dir().join(format!("ferrite-import-noid-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("headless.jsonl");
    fs::write(
        &path,
        concat!(
            r#"{"type":"user","message":{"role":"user","content":"hello"}}"#,
            "\n"
        ),
    )
    .unwrap();

    match import(&store, &path) {
        Err(ImportError::Unrecognized { detail }) => {
            assert!(!detail.is_empty(), "detail: {detail}")
        }
        other => panic!("expected Unrecognized, got {other:?}"),
    }
}

/// Junk of any shape is an error, never a panic: arbitrary bytes, torn JSON,
/// binary noise. Plain LCG-driven bytes — the point is many varied inputs,
/// not randomness.
#[test]
fn arbitrary_junk_never_panics_the_importer() {
    let store = scratch("junk");
    let dir = std::env::temp_dir().join(format!("ferrite-import-junk-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();

    let mut state: u64 = 0x5eed;
    let mut next = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        state
    };
    for round in 0..64 {
        let len = (next() % 512) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| (next() >> 24) as u8).collect();
        let path = dir.join(format!("junk-{round}"));
        fs::write(&path, &bytes).unwrap();
        match import(&store, &path) {
            Err(_) => {}
            Ok(id) => panic!("round {round}: junk imported as thread {id}"),
        }
    }
    assert_eq!(store.thread_ids().unwrap(), vec![]);
}

/// A session file `age_secs` old: written, then stamped with the mtime
/// discovery orders by.
fn write_session_file(path: &std::path::Path, contents: &str, age_secs: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    let modified = std::time::SystemTime::now() - std::time::Duration::from_secs(age_secs);
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
}

fn session_roots(base: &std::path::Path) -> Vec<(Provider, PathBuf)> {
    vec![
        (Provider::Claude, base.join("claude-projects")),
        (Provider::Codex, base.join("codex-sessions")),
    ]
}

/// Discovery is a bounded, ordered walk: both roots, `.jsonl` only,
/// newest first, capped — and a missing root lists nothing rather than
/// erroring.
#[test]
fn session_file_discovery_walks_both_roots_newest_first_and_capped() {
    let base =
        std::env::temp_dir().join(format!("ferrite-import-discovery-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let roots = session_roots(&base);
    write_session_file(
        &roots[0].1.join("-workspace-alpha").join("old.jsonl"),
        "x\n",
        3600,
    );
    write_session_file(
        &roots[0].1.join("-workspace-beta").join("new.jsonl"),
        "x\n",
        10,
    );
    write_session_file(
        &roots[1].1.join("2026").join("08").join("rollout-mid.jsonl"),
        "x\n",
        600,
    );
    // Not a session file shape: ignored by extension.
    write_session_file(
        &roots[0].1.join("-workspace-alpha").join("notes.txt"),
        "x\n",
        5,
    );

    let all = candidates(&roots, 8);
    let names: Vec<String> = all
        .iter()
        .map(|candidate| {
            candidate
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(names, ["new.jsonl", "rollout-mid.jsonl", "old.jsonl"]);
    assert_eq!(
        all.iter()
            .map(|candidate| candidate.provider)
            .collect::<Vec<_>>(),
        [Provider::Claude, Provider::Codex, Provider::Claude]
    );

    let capped = candidates(&roots, 2);
    assert_eq!(capped.len(), 2, "the cap holds");
    assert_eq!(
        capped[0].path.file_name().unwrap().to_string_lossy(),
        "new.jsonl"
    );

    let missing = session_roots(&base.join("nowhere"));
    assert!(candidates(&missing, 8).is_empty());
}
