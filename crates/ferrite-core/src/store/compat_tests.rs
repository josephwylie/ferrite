//! What a log means must never depend on which Ferrite last touched it.
//! Real-shaped logs from older schemas load to one pinned observable —
//! what a revive would show — before and after anything reopens them, and
//! the four agent-context invariants (ADR 0009) are each tested by name.

use super::*;
use crate::activity::{Activity, ActivityInput};
use std::fmt::Write as _;

/// A fresh per-test scratch directory.
pub(crate) fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrite-compat-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// Write a raw log for one Thread, bypassing every writer.
pub(crate) fn plant(dir: &Path, thread: u64, contents: &str) {
    let thread_dir = dir.join(thread.to_string());
    fs::create_dir_all(&thread_dir).unwrap();
    fs::write(thread_dir.join("log.jsonl"), contents).unwrap();
}

/// FNV-1a: a stable fingerprint for pinning long observables.
pub(crate) fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

/// Everything an operator could see of one Activity: every Subject's
/// blocks, runtime and identity. Time-free, so it pins exactly.
pub(crate) fn render_activity(activity: &Activity) -> String {
    let view = activity.view();
    let main = view.main();
    let transcript = main.transcript();
    let mut out = String::new();
    let mut timings: Vec<String> = main
        .tool_timings()
        .iter()
        .map(|(id, timing)| format!("{id}={timing:?}"))
        .collect();
    timings.sort();
    writeln!(
        out,
        "main status={:?} coverage={:?} transcript={:?} model={:?} session={:?}",
        main.status(),
        main.coverage(),
        transcript.status(),
        transcript.model(),
        transcript.session_id(),
    )
    .unwrap();
    writeln!(
        out,
        "usage={:?} usage_details={:?} context_details={:?}",
        transcript.usage(),
        transcript.usage_details(),
        transcript.context_details(),
    )
    .unwrap();
    writeln!(out, "progress={:?}", transcript.progress()).unwrap();
    writeln!(out, "timings={timings:?}").unwrap();
    for block in transcript.blocks() {
        writeln!(out, "  {:?}", block.body).unwrap();
    }
    for child in view.children() {
        writeln!(
            out,
            "child {} {:?} status={:?} coverage={:?} retained={}",
            child.key().as_str(),
            child.info(),
            child.status(),
            child.coverage(),
            child.retained(),
        )
        .unwrap();
        for block in child.transcript().blocks() {
            writeln!(out, "    {:?}", block.body).unwrap();
        }
    }
    out
}

/// What a revive of this Thread would read and show, from a full load:
/// the header facts, the resume target, the owed handover, prompt recall,
/// the legacy inputs, and the replayed Activity.
pub(crate) fn observed(store: &Store, id: ThreadId) -> String {
    let snapshot = store.load(id).expect("the log loads");
    let mut out = String::new();
    writeln!(
        out,
        "facts {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
        snapshot.provider(),
        snapshot.workspace(),
        snapshot.session_project_root(),
        snapshot.model(),
        snapshot.project_id(),
        snapshot.title(),
        snapshot.effort(),
    )
    .unwrap();
    let meta = store.peek(id).expect("the header peeks");
    writeln!(
        out,
        "peek {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
        meta.provider,
        meta.workspace,
        meta.session_project_root,
        meta.model,
        meta.project_id,
        meta.title,
        meta.effort,
    )
    .unwrap();
    writeln!(out, "first prompt {:?}", store.peek_first_prompt(id).unwrap()).unwrap();
    writeln!(out, "resume {:?}", snapshot.resume_target()).unwrap();
    let handover = snapshot
        .last_handover()
        .map(|handover| (handover.from, handover.exchanges, handover.delivered));
    writeln!(out, "handover {handover:?}").unwrap();
    writeln!(out, "recall {:?}", snapshot.prompt_texts()).unwrap();
    writeln!(out, "inputs {:?}", snapshot.inputs()).unwrap();
    let mut activity = Activity::default();
    activity.apply(ActivityInput::Disconnect);
    for input in snapshot.activity_inputs() {
        activity.apply(input);
    }
    out.push_str(&render_activity(&activity));
    out
}

/// Only the conversation, not the header: what an amendment must leave alone.
pub(crate) fn history_of(observed: &str) -> String {
    observed
        .lines()
        .filter(|line| !line.starts_with("facts ") && !line.starts_with("peek "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Schema 1, byte for byte what its writer produced: no prompts, no
/// structured results.
const V1: &str = concat!(
    r#"{"schema":1,"provider":"claude"}"#,
    "\n",
    r#"{"type":"init","session_id":"legacy-4f2a","model":"claude-haiku-4-5"}"#,
    "\n",
    r#"{"type":"text","text":"running the suite\n\n"}"#,
    "\n",
    r#"{"type":"tool_started","id":"toolu_9","name":"Bash","input":{"command":"cargo test"}}"#,
    "\n",
    r#"{"type":"tool_completed","id":"toolu_9","output":"42 passed","is_error":false}"#,
    "\n",
    r#"{"type":"turn_ended","outcome":"completed","cost_usd":0.01}"#,
    "\n",
);

/// Schema 8 after a provider switch: the header was rewritten to Codex,
/// and later picked a model the handover record never named.
const V8_HANDOVER: &str = concat!(
    r#"{"schema":8,"provider":"codex","workspace":{"kind":"main","checkout":"/fixture/repo"},"session_project_root":null,"model":"gpt-5.4","project_id":null,"title":"Port the parser","effort":null}"#,
    "\n",
    r#"{"type":"prompt","text":"port the parser to nom"}"#,
    "\n",
    r#"{"type":"init","session_id":"claude-sess-1","model":"claude-opus-4-1"}"#,
    "\n",
    r#"{"type":"text","text":"Starting with the lexer."}"#,
    "\n",
    r#"{"type":"tool_started","id":"toolu_1","name":"Edit","input":{"file_path":"src/lex.rs"}}"#,
    "\n",
    r#"{"type":"tool_completed","id":"toolu_1","output":"ok","is_error":false,"result":{"kind":"file_edit","path":"src/lex.rs","hunks":[{"old_start":1,"old_lines":1,"new_start":1,"new_lines":2,"lines":["-a","+b","+c"]}]},"duration_ms":1500}"#,
    "\n",
    r#"{"type":"turn_ended","outcome":"completed","cost_usd":0.42}"#,
    "\n",
    r#"{"type":"handover","from":"claude","to":"codex","model":null}"#,
    "\n",
    r#"{"type":"prompt","text":"continue on codex"}"#,
    "\n",
    r#"{"type":"init","session_id":"codex-thread-1","model":"gpt-5.4"}"#,
    "\n",
    r#"{"type":"reasoning_summary","text":"Plan the parser","summary_index":0}"#,
    "\n",
    r#"{"type":"text","text":"Parser ported."}"#,
    "\n",
    r#"{"type":"token_usage","total_tokens":1200,"input_tokens":1000,"cached_input_tokens":200,"output_tokens":200,"reasoning_output_tokens":50,"context_window":272000}"#,
    "\n",
    r#"{"type":"turn_ended","outcome":"completed","cost_usd":null}"#,
    "\n",
);

/// Schema 11 in the shape real long Threads have: attributed Main content,
/// a subagent with an alias, background work, usage cards, a reset.
const V11_ACTIVITY: &str = concat!(
    r#"{"schema":11,"provider":"claude","workspace":{"kind":"worktree","repo":"/fixture/repo","path":"/fixture/repo/.worktrees/cash-week"},"session_project_root":null,"model":"claude-fable-5-1[1m]","project_id":14,"title":"Switch to worktree 165","effort":"xhigh"}"#,
    "\n",
    r#"{"type":"prompt","text":"start fresh please"}"#,
    "\n",
    r#"{"type":"init","session_id":"2cd1-a","model":"claude-fable-5-1"}"#,
    "\n",
    r#"{"type":"text","text":"Sure."}"#,
    "\n",
    r#"{"type":"prompt","text":"now clear and start over"}"#,
    "\n",
    r#"{"type":"conversation_reset","session_id":"2cd1-b"}"#,
    "\n",
    r#"{"type":"prompt","text":"Can you cd into the 165 worktree?"}"#,
    "\n",
    r#"{"type":"progress","event":{"kind":"phase","phase":"working","detail":""}}"#,
    "\n",
    r#"{"type":"activity","observation":{"kind":"main_content","id":"msg_1","event":{"kind":"thinking_delta","text":"Let me look."}}}"#,
    "\n",
    r#"{"type":"activity","observation":{"kind":"main_content","id":"msg_1","event":{"kind":"text_delta","text":"Switching now."}}}"#,
    "\n",
    r#"{"type":"activity","observation":{"kind":"main_content","id":"toolu_1","event":{"kind":"tool_started","id":"toolu_1","name":"Bash","input":{"command":"git worktree list","description":"List worktrees"}}}}"#,
    "\n",
    r#"{"type":"activity","observation":{"kind":"main_content","id":"toolu_1","event":{"kind":"tool_completed","id":"toolu_1","output":"/fixture/repo  abc [main]","is_error":false,"result":{"kind":"command","stdout":"/fixture/repo  abc [main]","stderr":"","exit_code":null,"duration_ms":null}},"duration_ms":412}}"#,
    "\n",
    r#"{"type":"activity","observation":{"kind":"discovered","key":"[\"claude\",\"2cd1-a\",\"agent-7\"]","parent":null,"name":"explorer","description":"Find the worktree","agent_kind":"Explore","coverage":"unavailable"}}"#,
    "\n",
    r#"{"type":"activity","observation":{"kind":"status","key":"[\"claude\",\"2cd1-a\",\"agent-7\"]","state":"working"}}"#,
    "\n",
    r#"{"type":"activity","observation":{"kind":"content","key":"[\"claude\",\"2cd1-a\",\"agent-7\"]","id":"m-c1","event":{"kind":"text","text":"Found it under .worktrees."}}}"#,
    "\n",
    r#"{"type":"activity","observation":{"kind":"alias","from":"[\"claude\",\"2cd1-a\",\"toolu_2\"]","to":"[\"claude\",\"2cd1-a\",\"agent-7\"]"}}"#,
    "\n",
    r#"{"type":"activity","observation":{"kind":"status","key":"[\"claude\",\"2cd1-a\",\"agent-7\"]","state":"idle"}}"#,
    "\n",
    r#"{"type":"activity","observation":{"kind":"main_content","id":"msg_2","event":{"kind":"text","text":"Switching now. Done."}}}"#,
    "\n",
    r#"{"type":"token_usage","total_tokens":4567,"input_tokens":4000,"cached_input_tokens":3000,"output_tokens":567,"reasoning_output_tokens":0,"context_window":null}"#,
    "\n",
    r#"{"type":"usage_details","details":{"scope":"turn","input_tokens":4000,"cached_input_tokens":3000,"output_tokens":567,"reasoning_output_tokens":0}}"#,
    "\n",
    r#"{"type":"content_boundary"}"#,
    "\n",
    r#"{"type":"turn_ended","outcome":"completed","cost_usd":0.12}"#,
    "\n",
    r#"{"type":"completion_observation","elapsed_ms":8123,"completed_at":"7:31 pm"}"#,
    "\n",
    r#"{"type":"context_usage","total_tokens":4567,"context_window":1000000}"#,
    "\n",
    r#"{"type":"context_details","details":{"usable_window":1000000,"auto_compact_threshold":900000,"is_auto_compact_enabled":true,"categories":[{"name":"Messages","tokens":4567}]}}"#,
    "\n",
    r#"{"type":"activity","observation":{"kind":"background_turn_ended","outcome":"completed","cost_usd":0.01}}"#,
    "\n",
    r#"{"type":"progress","event":{"kind":"task","id":"1","subject":"List worktrees","status":null,"deleted":false}}"#,
    "\n",
    r#"{"type":"progress","event":{"kind":"background_snapshot","tasks":[{"id":"bg1","label":"cargo test","status":"working","detail":""}]}}"#,
    "\n",
    r#"{"type":"prompt","text":"fresh start"}"#,
    "\n",
    r#"{"type":"activity","observation":{"kind":"main_content","id":"msg_3","event":{"kind":"text_snapshot","text":"Fresh."}}}"#,
    "\n",
    r#"{"type":"turn_ended","outcome":"interrupted","cost_usd":null}"#,
    "\n",
    r#"{"type":"closed","reason":"session ended"}"#,
    "\n",
);

/// Schema 12 with its send times, token counts and hunk sections — and a
/// crash's torn final line.
const V12_TORN: &str = concat!(
    r#"{"schema":12,"provider":"codex","workspace":{"kind":"main","checkout":"/fixture/repo"},"session_project_root":"/fixture/repo/api","model":null,"project_id":null,"title":null,"effort":"high"}"#,
    "\n",
    r#"{"type":"prompt","text":"add a test"}"#,
    "\n",
    r#"{"type":"prompt_observation","sent_at":"9:02 am"}"#,
    "\n",
    r#"{"type":"init","session_id":"0199-codex","model":"gpt-5.4-mini"}"#,
    "\n",
    r#"{"type":"summary_part","item_id":"rs_1","summary_index":0,"text":"Thinking about tests","snapshot":false}"#,
    "\n",
    r#"{"type":"file_changes","id":"fc_1","edits":[{"path":"tests/a.rs","hunks":[{"old_start":0,"old_lines":0,"new_start":1,"new_lines":1,"lines":["+fn a() {}"],"section":"mod tests"}]}]}"#,
    "\n",
    r#"{"type":"tool_output","id":"call_1","text":"running 1 test\n"}"#,
    "\n",
    r#"{"type":"turn_diff","turn_id":"turn_1","diff":"diff --git a/tests/a.rs b/tests/a.rs\n"}"#,
    "\n",
    r#"{"type":"turn_ended","outcome":{"error":"rate limited"},"cost_usd":null}"#,
    "\n",
    r#"{"type":"completion_observation","elapsed_ms":3000,"completed_at":"9:03 am","input_tokens":900,"output_tokens":120}"#,
    "\n",
    r#"{"type":"run_state","state":"idle"}"#,
    "\n",
    r#"{"type":"text","te"#,
);

/// Each fixture: its log, how many records it must load (every whole line
/// after the header), and the fingerprint of what it shows.
pub(crate) const FIXTURES: &[(&str, &str, usize, u64)] = &[
    ("v1", V1, 5, 9959750768830046166),
    ("v8-handover", V8_HANDOVER, 13, 17518645202087855028),
    ("v11-activity", V11_ACTIVITY, 31, 13604963610014101423),
    ("v12-torn", V12_TORN, 10, 5782667316263972688),
];

/// Old logs load to exactly what they always showed — and still do after a
/// writer reopens them, which is when any repair or upgrade happens.
#[test]
fn old_format_fixtures_load_identically_before_and_after_reopening() {
    for (name, log, records, pinned) in FIXTURES.iter().rev() {
        let dir = scratch(&format!("fixture-{name}"));
        plant(&dir, 5, log);
        let store = Store::open(&dir).unwrap();
        let id = ThreadId::new(5);
        assert_eq!(
            store.load(id).unwrap().records.len(),
            *records,
            "{name}: a fixture line did not parse"
        );
        let before = observed(&store, id);
        if fingerprint(&before) != *pinned {
            eprintln!("{name} fingerprint {}", fingerprint(&before));
        }
        assert_eq!(
            fingerprint(&before),
            *pinned,
            "{name} no longer loads as it did:\n{before}"
        );
        drop(store.writer(id).unwrap());
        let after = observed(&Store::open(&dir).unwrap(), id);
        assert_eq!(before, after, "{name}: reopening changed what it shows");
    }
}

/// Invariant 1 (ADR 0009): every event logged today is still logged, in
/// order, byte for byte. Records the store adds for itself (`facts`,
/// `mark`) are allowed between them; nothing else is.
#[test]
fn invariant_every_logged_event_is_still_logged_in_order() {
    use crate::activity::{ActivityEvent, AgentKey, ExecutionEvent, Subject};
    let dir = scratch("invariant-logged");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    let child = AgentKey::new(Provider::Claude, "sess-1", "agent-1");
    let events = |writer: &mut ThreadWriter, round: usize| {
        writer.record_prompt(&format!("prompt {round}")).unwrap();
        writer.record_prompt_observation("7:31 pm").unwrap();
        for event in [
            SessionEvent::Init {
                session_id: format!("sess-{round}"),
                model: "claude-opus-5".into(),
            },
            SessionEvent::TextDelta { text: "par".into() },
            SessionEvent::TextDelta { text: "tial".into() },
            SessionEvent::ToolStarted {
                id: format!("toolu_{round}"),
                name: "Bash".into(),
                input: serde_json::json!({ "command": "ls" }),
            },
            SessionEvent::ToolOutputDelta {
                id: format!("toolu_{round}"),
                text: "a\n".into(),
            },
            SessionEvent::ToolOutputDelta {
                id: format!("toolu_{round}"),
                text: "b\n".into(),
            },
            SessionEvent::ToolCompleted {
                id: format!("toolu_{round}"),
                output: "a\nb\n".into(),
                is_error: false,
                result: crate::ToolResult::Opaque,
            },
            SessionEvent::Activity(ActivityEvent::Content {
                key: child.clone(),
                id: Some("m1".into()),
                event: ExecutionEvent::Text {
                    text: "child says".into(),
                },
            }),
            SessionEvent::TurnEnded {
                outcome: crate::TurnOutcome::Completed,
                cost_usd: Some(0.5),
            },
        ] {
            let duration = matches!(event, SessionEvent::ToolCompleted { .. })
                .then(|| std::time::Duration::from_millis(1500));
            writer.record_event(&event, duration).unwrap();
        }
        writer
            .record_completion(
                &Subject::Main,
                &CompletionFacts {
                    elapsed_ms: 900,
                    completed_at: "7:32 pm".into(),
                    input_tokens: Some(10),
                    output_tokens: Some(5),
                },
            )
            .unwrap();
    };
    events(&mut writer, 1);
    drop(writer);
    // A revive in between: the next Session's records follow the last.
    let mut writer = store.writer(id).unwrap();
    events(&mut writer, 2);
    writer
        .record_event(
            &SessionEvent::Closed {
                reason: "parked".into(),
            },
            None,
        )
        .unwrap();
    writer.flush().unwrap();

    let round = |n: usize| {
        vec![
            format!(r#"{{"type":"prompt","text":"prompt {n}"}}"#),
            r#"{"type":"prompt_observation","sent_at":"7:31 pm"}"#.to_string(),
            format!(r#"{{"type":"init","session_id":"sess-{n}","model":"claude-opus-5"}}"#),
            r#"{"type":"text","text":"partial"}"#.to_string(),
            format!(
                r#"{{"type":"tool_started","id":"toolu_{n}","name":"Bash","input":{{"command":"ls"}}}}"#
            ),
            format!(r#"{{"type":"tool_output","id":"toolu_{n}","text":"a\nb\n"}}"#),
            format!(
                r#"{{"type":"tool_completed","id":"toolu_{n}","output":"a\nb\n","is_error":false,"result":{{"kind":"opaque"}},"duration_ms":1500}}"#
            ),
            r#"{"type":"activity","observation":{"kind":"content","key":"[\"claude\",\"sess-1\",\"agent-1\"]","id":"m1","event":{"kind":"text","text":"child says"}}}"#.to_string(),
            r#"{"type":"turn_ended","outcome":"completed","cost_usd":0.5}"#.to_string(),
            r#"{"type":"completion_observation","elapsed_ms":900,"completed_at":"7:32 pm","input_tokens":10,"output_tokens":5}"#.to_string(),
        ]
    };
    let mut expected = round(1);
    expected.extend(round(2));
    expected.push(r#"{"type":"closed","reason":"parked"}"#.to_string());

    let logged: Vec<String> = fs::read_to_string(dir.join(id.to_string()).join("log.jsonl"))
        .unwrap()
        .lines()
        .skip(1)
        .filter(|line| {
            !line.starts_with(r#"{"type":"facts""#) && !line.starts_with(r#"{"type":"mark""#)
        })
        .map(str::to_string)
        .collect();
    assert_eq!(logged, expected);
}

/// Invariant 3 (ADR 0009): a hand-over receives every exchange the log
/// holds since the last reset, however long the Thread — not a window.
#[test]
fn invariant_hand_over_receives_every_exchange() {
    let dir = scratch("invariant-exchanges");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    // Before the reset: not part of the conversation being handed over.
    writer.record_prompt("forgotten").unwrap();
    writer
        .record_event(
            &SessionEvent::ConversationReset {
                session_id: "sess-2".into(),
            },
            None,
        )
        .unwrap();
    let answer = |n: usize| format!("answer {n} {}", "x".repeat(20_000));
    for n in 0..300 {
        writer.record_prompt(&format!("question {n}")).unwrap();
        writer
            .record_event(&SessionEvent::TextDelta { text: answer(n) }, None)
            .unwrap();
        writer
            .record_event(
                &SessionEvent::TurnEnded {
                    outcome: crate::TurnOutcome::Completed,
                    cost_usd: None,
                },
                None,
            )
            .unwrap();
    }
    drop(writer);
    // Reopened as a revive would, so any bounded read is in play.
    let mut writer = store.writer(id).unwrap();
    let handover = store
        .hand_over(id, Provider::Codex, None, &mut writer)
        .unwrap();

    assert_eq!(handover.from, Provider::Claude);
    let expected: Vec<(String, String)> = (0..300)
        .map(|n| (format!("question {n}"), answer(n)))
        .collect();
    assert_eq!(handover.exchanges.len(), expected.len());
    assert!(handover.exchanges == expected, "an exchange was lost or changed");    let _ = fs::remove_dir_all(&dir);
}

/// The bytes of every record a log holds, as written: what no rewrite may
/// change.
pub(crate) fn record_bytes(log: &[u8]) -> Vec<u8> {
    let readable = parse(ThreadId::new(0), log).unwrap().readable;
    let header_end = log.iter().position(|byte| *byte == b'\n').unwrap() + 1;
    log[header_end.min(readable)..readable].to_vec()
}

/// Invariant 4 (ADR 0009): nothing the store does to an existing log loses
/// what it held. Every reopen, repair, upgrade and amendment leaves every
/// earlier record readable, in order — and byte for byte as its writer
/// wrote it: no record is ever decoded and encoded again.
#[test]
fn invariant_no_existing_log_is_rewritten_lossily() {
    for (name, log, _, _) in FIXTURES {
        let dir = scratch(&format!("invariant-lossless-{name}"));
        plant(&dir, 5, log);
        let store = Store::open(&dir).unwrap();
        let id = ThreadId::new(5);
        let before = history_of(&observed(&store, id));
        let records = record_bytes(log.as_bytes());

        let mut writer = store.writer(id).unwrap();
        store
            .set_title(id, "renamed".into(), Some(&mut writer))
            .unwrap();
        store
            .set_workspace(
                id,
                &WorkspaceBinding::Main {
                    checkout: "/fixture/elsewhere".into(),
                },
                Some(&mut writer),
            )
            .unwrap();
        let meta = store.peek(id).unwrap();
        store
            .set_provider(id, meta.provider, Some("m".into()), None, Some(&mut writer))
            .unwrap();
        drop(writer);

        let after = Store::open(&dir).unwrap();
        assert_eq!(
            history_of(&observed(&after, id)),
            before,
            "{name}: the conversation changed"
        );
        let meta = after.peek(id).unwrap();
        assert_eq!(meta.title.as_deref(), Some("renamed"), "{name}");
        assert_eq!(meta.model.as_deref(), Some("m"), "{name}");
        let now = fs::read(dir.join("5").join("log.jsonl")).unwrap();
        assert!(
            record_bytes(&now).starts_with(&records),
            "{name}: a record's bytes changed"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

/// A peek pulls the header off the disk and nothing after it, however
/// long the log — counted, not inferred.
#[test]
fn a_peek_reads_only_the_start_of_a_long_log() {
    let dir = scratch("peek-bytes");
    plant(&dir, 5, V11_ACTIVITY);
    let mut log = OpenOptions::new()
        .append(true)
        .open(dir.join("5").join("log.jsonl"))
        .unwrap();
    for n in 0..2000 {
        writeln!(log, r#"{{"type":"text","text":"line {n} {}"}}"#, "y".repeat(1000)).unwrap();
    }
    let store = Store::open(&dir).unwrap();
    let start = store.bytes_read();
    store.peek(ThreadId::new(5)).unwrap();
    assert!(
        store.bytes_read() - start <= 64 * 1024,
        "peek read {} bytes of a {} byte log",
        store.bytes_read() - start,
        fs::metadata(dir.join("5").join("log.jsonl")).unwrap().len()
    );
}

/// A real log, copied (never touched in place): set `LAB_LOG` to a log and,
/// to compare against an earlier run, `LAB_OBSERVED` to its fingerprint.
/// The fingerprint of what it shows must survive a revive's reopen and an
/// amendment unchanged.
#[test]
#[ignore = "needs LAB_LOG, a copy of a real Thread log"]
fn a_real_log_loads_identically_before_and_after_reopening() {
    let Ok(source) = std::env::var("LAB_LOG") else {
        return;
    };
    let dir = scratch("lab-real-log");
    fs::create_dir_all(dir.join("3")).unwrap();
    fs::copy(&source, dir.join("3").join("log.jsonl")).unwrap();
    let store = Store::open(&dir).unwrap();
    let id = ThreadId::new(3);
    let before = observed(&store, id);
    eprintln!("LAB_OBSERVED={}", fingerprint(&before));
    if let Ok(pinned) = std::env::var("LAB_OBSERVED") {
        assert_eq!(fingerprint(&before).to_string(), pinned);
    }
    let mut writer = store.writer(id).unwrap();
    store
        .set_title(id, "lab title".into(), Some(&mut writer))
        .unwrap();
    drop(writer);
    let after = observed(&Store::open(&dir).unwrap(), id);
    assert_eq!(history_of(&before), history_of(&after));
    let _ = fs::remove_dir_all(&dir);
}

fn main() -> WorkspaceBinding {
    WorkspaceBinding::Main {
        checkout: "/fixture/repo".into(),
    }
}
