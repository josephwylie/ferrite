use super::*;
use crate::activity::{
    ActivityEvent, ActivityInput, AgentInfo, AgentKey, AgentStatus, ExecutionEvent, Subject,
    TranscriptCoverage,
};
use crate::{Decision, ToolResult, TurnOutcome};
use std::cell::Cell;
use std::time::Duration;

fn store() -> (Store, ThreadId, ThreadWriter) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ferrite-store-activity-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let store = Store::open(dir).unwrap();
    let (id, writer) = store
        .create(
            Provider::Claude,
            None,
            crate::workspace::WorkspaceBinding::Main {
                checkout: PathBuf::from("/fixture"),
            },
        )
        .unwrap();
    (store, id, writer)
}

fn key(native: &str) -> AgentKey {
    AgentKey::new(Provider::Claude, "root", native)
}

fn content(agent: &str, id: Option<&str>, event: ExecutionEvent) -> ActivityEvent {
    ActivityEvent::Content {
        key: key(agent),
        id: id.map(str::to_owned),
        event,
    }
}

fn record(writer: &mut ThreadWriter, event: &ActivityEvent) {
    writer
        .record_event(&SessionEvent::Activity(event.clone()), None)
        .unwrap();
}

fn replay_events(snapshot: &ThreadSnapshot) -> Vec<ActivityEvent> {
    snapshot
        .activity_inputs()
        .into_iter()
        .filter_map(|input| match input {
            ActivityInput::ReplayEvent(event) => Some(event),
            _ => None,
        })
        .collect()
}

#[test]
fn attributed_facts_round_trip_with_native_identity_and_complete_payloads() {
    let (store, thread, mut writer) = store();
    let mut info = AgentInfo::new(key("child"));
    info.parent = Some(Subject::Subagent(key("parent")));
    info.name = Some("SDK name".into());
    info.description = Some("research".into());
    info.kind = Some("Explore".into());
    info.coverage = TranscriptCoverage::Partial;
    let mut expected = vec![ActivityEvent::Discovered(info)];
    for state in [
        AgentStatus::Unknown,
        AgentStatus::Pending,
        AgentStatus::Working,
        AgentStatus::Waiting,
        AgentStatus::Idle,
        AgentStatus::Paused,
        AgentStatus::Interrupted,
        AgentStatus::Failed,
        AgentStatus::Shutdown,
        AgentStatus::NotFound,
        AgentStatus::NotLoaded,
    ] {
        expected.push(ActivityEvent::Status {
            key: key("child"),
            state,
        });
    }
    for (index, execution) in [
        ExecutionEvent::TextDelta {
            text: "first".into(),
        },
        ExecutionEvent::ThinkingDelta {
            text: "thinking".into(),
        },
        ExecutionEvent::ReasoningSummaryDelta {
            text: "summary".into(),
            summary_index: 2,
        },
        ExecutionEvent::Text {
            text: "complete frame".into(),
        },
        ExecutionEvent::Thinking {
            text: "complete thought".into(),
        },
        ExecutionEvent::TextSnapshot {
            text: "authoritative item".into(),
        },
        ExecutionEvent::ThinkingSnapshot {
            text: "authoritative thought".into(),
        },
        ExecutionEvent::Prompt {
            text: "child input".into(),
        },
        ExecutionEvent::Notice {
            text: "provider notice".into(),
        },
        ExecutionEvent::ToolStarted {
            id: "tool".into(),
            name: "Bash".into(),
            input: serde_json::json!({"command":"printf fixture"}),
        },
        ExecutionEvent::ToolCompleted {
            id: "tool".into(),
            output: "fixture".into(),
            is_error: false,
            result: ToolResult::Command {
                stdout: "fixture".into(),
                stderr: String::new(),
            },
        },
        ExecutionEvent::TurnEnded {
            outcome: TurnOutcome::Interrupted,
            cost_usd: Some(0.12),
        },
        ExecutionEvent::TokenUsage {
            total_tokens: 12,
            input_tokens: 6,
            cached_input_tokens: 2,
            output_tokens: 4,
            reasoning_output_tokens: 1,
            context_window: Some(200_000),
        },
    ]
    .into_iter()
    .enumerate()
    {
        expected.push(content(
            "child",
            Some(&format!("turn:item:frame-{index}")),
            execution,
        ));
    }
    expected.extend([
        ActivityEvent::HistoryContent {
            key: key("child"),
            id: Some("history-frame".into()),
            event: ExecutionEvent::TextSnapshot {
                text: "older item".into(),
            },
        },
        ActivityEvent::Coverage {
            key: key("child"),
            coverage: TranscriptCoverage::Unavailable,
        },
        ActivityEvent::Detached { key: key("child") },
        ActivityEvent::Alias {
            from: key("provisional"),
            to: key("child"),
        },
        ActivityEvent::MainContent {
            id: Some("main-item".into()),
            event: ExecutionEvent::TextSnapshot {
                text: "main text".into(),
            },
        },
        ActivityEvent::BackgroundTurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        },
    ]);
    for event in &expected {
        record(&mut writer, event);
    }
    writer.flush().unwrap();
    let snapshot = store.load(thread).unwrap();
    assert_eq!(replay_events(&snapshot), expected);
    let bytes = fs::read_to_string(store.log_path(thread)).unwrap();
    assert!(bytes.starts_with("{\"schema\":9,"));
    assert!(bytes.contains("\"type\":\"activity\""));
    assert_eq!(
        snapshot.inputs().len(),
        1,
        "only autonomous Main completion is legacy Main input"
    );
}

#[test]
fn chronology_and_tool_durations_remain_scoped_across_main_and_children() {
    let (store, thread, mut writer) = store();
    writer.record_prompt("operator prompt").unwrap();
    writer
        .record_event(
            &SessionEvent::ToolCompleted {
                id: "same-tool".into(),
                output: "main".into(),
                is_error: false,
                result: ToolResult::Opaque,
            },
            Some(Duration::from_millis(11)),
        )
        .unwrap();
    for (agent, ms) in [("alpha", 22), ("beta", 33)] {
        let event = content(
            agent,
            Some("same-item"),
            ExecutionEvent::ToolCompleted {
                id: "same-tool".into(),
                output: agent.into(),
                is_error: false,
                result: ToolResult::Opaque,
            },
        );
        writer
            .record_event(
                &SessionEvent::Activity(event),
                Some(Duration::from_millis(ms)),
            )
            .unwrap();
    }
    writer.flush().unwrap();
    let snapshot = store.load(thread).unwrap();
    let inputs = snapshot.activity_inputs();
    assert!(
        matches!(&inputs[0], ActivityInput::Replay(Input::Prompt(prompt)) if prompt == "operator prompt")
    );
    assert!(
        matches!(&inputs[1], ActivityInput::Replay(Input::Event(SessionEvent::ToolCompleted { output, .. })) if output == "main")
    );
    assert!(
        matches!(&inputs[2], ActivityInput::ReplayEvent(ActivityEvent::Content { key: observed, .. }) if observed == &key("alpha"))
    );
    assert!(
        matches!(&inputs[3], ActivityInput::ReplayEvent(ActivityEvent::Content { key: observed, .. }) if observed == &key("beta"))
    );
    let timings: std::collections::BTreeMap<_, _> = inputs
        .into_iter()
        .filter_map(|input| match input {
            ActivityInput::RestoreTimings { subject, timings } => Some((subject, timings)),
            _ => None,
        })
        .collect();
    for (subject, ms) in [
        (Subject::Main, 11),
        (Subject::Subagent(key("alpha")), 22),
        (Subject::Subagent(key("beta")), 33),
    ] {
        assert_eq!(timings[&subject]["same-tool"], Duration::from_millis(ms));
    }
    assert_eq!(
        snapshot.tool_durations(),
        vec![("same-tool".into(), Duration::from_millis(11))]
    );
    assert_eq!(snapshot.prompt_texts(), vec!["operator prompt"]);
}

#[test]
fn pending_decisions_and_cancellations_never_enter_the_log_or_replay() {
    let (store, thread, mut writer) = store();
    for subject in [
        None,
        Some(Subject::Main),
        Some(Subject::Subagent(key("child"))),
    ] {
        record(
            &mut writer,
            &ActivityEvent::Decision {
                subject,
                decision: Decision {
                    id: "live-only-request-secret".into(),
                    tool_use_id: "call".into(),
                    tool_name: "Bash".into(),
                    description: "approve".into(),
                    input: serde_json::json!({"command":"fixture"}),
                    suggestions: vec![],
                },
            },
        );
    }
    record(
        &mut writer,
        &ActivityEvent::DecisionCancelled {
            id: "live-only-request-secret".into(),
        },
    );
    record(
        &mut writer,
        &ActivityEvent::Status {
            key: key("child"),
            state: AgentStatus::Working,
        },
    );
    writer.flush().unwrap();
    assert!(
        !fs::read_to_string(store.log_path(thread))
            .unwrap()
            .contains("live-only-request-secret")
    );
    assert_eq!(
        replay_events(&store.load(thread).unwrap()),
        vec![ActivityEvent::Status {
            key: key("child"),
            state: AgentStatus::Working
        }]
    );
}

#[test]
fn child_completion_flushes_after_main_idle_without_cross_actor_coalescing() {
    let (store, thread, mut writer) = store();
    writer
        .record_event(
            &SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            },
            None,
        )
        .unwrap();
    let idle_size = fs::metadata(store.log_path(thread)).unwrap().len();
    let facts = [
        content(
            "alpha",
            Some("same-item"),
            ExecutionEvent::TextDelta { text: "a".into() },
        ),
        content(
            "beta",
            Some("same-item"),
            ExecutionEvent::TextDelta { text: "b".into() },
        ),
        content(
            "alpha",
            Some("other-item"),
            ExecutionEvent::TextDelta { text: "c".into() },
        ),
    ];
    for fact in &facts {
        record(&mut writer, fact);
    }
    assert_eq!(
        fs::metadata(store.log_path(thread)).unwrap().len(),
        idle_size
    );
    record(
        &mut writer,
        &ActivityEvent::Status {
            key: key("alpha"),
            state: AgentStatus::Idle,
        },
    );
    let snapshot = store.load(thread).unwrap();
    assert_eq!(&replay_events(&snapshot)[..3], &facts);
    assert!(writer.buffer.is_empty());
}

#[test]
fn handover_excludes_child_prompts_and_text_and_reconciles_main_item_snapshots() {
    let (store, thread, mut writer) = store();
    writer
        .record_event(
            &SessionEvent::Init {
                session_id: "main-root".into(),
                model: "model".into(),
            },
            None,
        )
        .unwrap();
    writer.record_prompt("operator prompt").unwrap();
    for execution in [
        ExecutionEvent::TextDelta { text: "par".into() },
        ExecutionEvent::TextDelta {
            text: "tial".into(),
        },
        ExecutionEvent::TextSnapshot {
            text: "intermediate".into(),
        },
        ExecutionEvent::TextSnapshot {
            text: "final".into(),
        },
        ExecutionEvent::TextSnapshot {
            text: "final".into(),
        },
    ] {
        record(
            &mut writer,
            &ActivityEvent::MainContent {
                id: Some("main-item".into()),
                event: execution,
            },
        );
    }
    record(
        &mut writer,
        &content(
            "child",
            Some("prompt-frame"),
            ExecutionEvent::Prompt {
                text: "child prompt".into(),
            },
        ),
    );
    record(
        &mut writer,
        &content(
            "child",
            Some("main-item"),
            ExecutionEvent::Text {
                text: "child answer".into(),
            },
        ),
    );
    writer.flush().unwrap();
    assert_eq!(
        store.load(thread).unwrap().resume_target(),
        Some("main-root")
    );
    writer
        .record_handover(Provider::Claude, Provider::Codex, None)
        .unwrap();
    record(
        &mut writer,
        &content(
            "child",
            None,
            ExecutionEvent::Prompt {
                text: "late child".into(),
            },
        ),
    );
    record(
        &mut writer,
        &content(
            "child",
            None,
            ExecutionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            },
        ),
    );
    let snapshot = store.load(thread).unwrap();
    assert_eq!(snapshot.resume_target(), None);
    assert_eq!(snapshot.prompt_texts(), vec!["operator prompt"]);
    let handover = snapshot.last_handover().unwrap();
    assert_eq!(
        handover.exchanges,
        vec![("operator prompt".into(), "final".into())]
    );
    assert!(
        !handover.delivered,
        "child prompt cannot consume Main Handover"
    );
    assert!(
        !snapshot
            .inputs()
            .iter()
            .any(|input| matches!(input, Input::Event(SessionEvent::Activity(_))))
    );
}

#[test]
fn schema_eight_upgrade_retains_main_history_before_new_attributed_facts() {
    let (store, thread, writer) = store();
    drop(writer);
    let old = concat!(
        "{\"schema\":8,\"provider\":\"claude\",\"effort\":\"high\"}\n",
        "{\"type\":\"init\",\"session_id\":\"old-main\",\"model\":\"model\"}\n",
        "{\"type\":\"prompt\",\"text\":\"old prompt\"}\n",
        "{\"type\":\"text\",\"text\":\"old answer\"}\n",
        "{\"type\":\"turn_ended\",\"outcome\":\"completed\",\"cost_usd\":null}\n"
    );
    fs::write(store.log_path(thread), old).unwrap();
    let before = store.load(thread).unwrap();
    assert!(replay_events(&before).is_empty());
    let mut writer = store.writer(thread).unwrap();
    let event = content(
        "child",
        Some("uuid"),
        ExecutionEvent::Text {
            text: "new child".into(),
        },
    );
    record(&mut writer, &event);
    writer.flush().unwrap();
    let after = store.load(thread).unwrap();
    assert_eq!(after.inputs(), before.inputs());
    assert_eq!(after.resume_target(), Some("old-main"));
    assert_eq!(after.effort().as_deref(), Some("high"));
    assert_eq!(replay_events(&after), vec![event]);
    assert!(
        fs::read_to_string(store.log_path(thread))
            .unwrap()
            .starts_with("{\"schema\":9,")
    );
}

#[derive(Default)]
struct FaultySink {
    bytes: Vec<u8>,
    fail_after: Option<usize>,
    fail_sync: Cell<bool>,
}
impl Write for FaultySink {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if self.fail_after == Some(0) {
            return Err(io::Error::other("injected write failure"));
        }
        let written = self
            .fail_after
            .map_or(input.len(), |left| input.len().min(left));
        self.bytes.extend_from_slice(&input[..written]);
        if let Some(left) = &mut self.fail_after {
            *left -= written;
        }
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl DurableWrite for FaultySink {
    fn sync_data(&self) -> io::Result<()> {
        if self.fail_sync.get() {
            Err(io::Error::other("injected sync failure"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn partial_write_retry_preserves_buffer_and_never_duplicates_the_written_prefix() {
    let mut sink = FaultySink {
        fail_after: Some(7),
        ..Default::default()
    };
    let mut buffer = vec![Record::Text {
        text: "first".into(),
    }];
    let first = line(&buffer[0]).unwrap();
    let mut pending = None;
    assert!(flush_records(&mut sink, &mut buffer, &mut pending).is_err());
    assert_eq!(buffer.len(), 1);
    assert_eq!(pending.as_ref().unwrap().written, 7);
    buffer.push(Record::Text {
        text: "later".into(),
    });
    let second = line(&buffer[1]).unwrap();
    sink.fail_after = None;
    flush_records(&mut sink, &mut buffer, &mut pending).unwrap();
    assert_eq!(String::from_utf8(sink.bytes).unwrap(), first + &second);
    assert!(buffer.is_empty());
    assert!(pending.is_none());
}

#[test]
fn sync_failure_retries_sync_without_reappending_already_written_records() {
    let mut sink = FaultySink {
        fail_sync: Cell::new(true),
        ..Default::default()
    };
    let mut buffer = vec![Record::Text {
        text: "first".into(),
    }];
    let expected = line(&buffer[0]).unwrap();
    let mut pending = None;
    assert!(flush_records(&mut sink, &mut buffer, &mut pending).is_err());
    assert_eq!(buffer.len(), 1);
    assert_eq!(sink.bytes, expected.as_bytes());
    sink.fail_sync.set(false);
    flush_records(&mut sink, &mut buffer, &mut pending).unwrap();
    assert_eq!(sink.bytes, expected.as_bytes());
    assert!(buffer.is_empty());
}

#[test]
fn failed_writer_keeps_encoded_records_immutable_when_new_text_arrives() {
    let (store, thread, mut writer) = store();
    writer
        .record_event(
            &SessionEvent::TextDelta {
                text: "first".into(),
            },
            None,
        )
        .unwrap();
    writer.file = File::open(store.log_path(thread)).unwrap(); // Read-only descriptor fails append.
    assert!(writer.flush().is_err());
    assert!(writer.buffered_since.is_some());
    writer
        .record_event(
            &SessionEvent::TextDelta {
                text: "second".into(),
            },
            None,
        )
        .unwrap();
    assert_eq!(
        writer.buffer.len(),
        2,
        "frozen prefix must not coalesce with new text"
    );
    writer.file = OpenOptions::new()
        .append(true)
        .open(store.log_path(thread))
        .unwrap();
    writer.flush().unwrap();
    assert_eq!(
        store.load(thread).unwrap().inputs(),
        vec![
            Input::Event(SessionEvent::TextDelta {
                text: "first".into()
            }),
            Input::Event(SessionEvent::TextDelta {
                text: "second".into()
            }),
        ]
    );
}

fn history_content(inputs: Vec<ActivityInput>) -> Vec<(AgentKey, Option<String>, ExecutionEvent)> {
    inputs
        .into_iter()
        .filter_map(|input| match input {
            ActivityInput::ReplayEvent(ActivityEvent::HistoryContent { key, id, event }) => {
                Some((key, id, event))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn child_cache_checkpoint_excludes_later_appends_and_unrelated_actors() {
    let (store, thread, mut writer) = store();
    record(
        &mut writer,
        &content(
            "alpha",
            Some("stream"),
            ExecutionEvent::TextDelta {
                text: "before".into(),
            },
        ),
    );
    record(
        &mut writer,
        &content(
            "beta",
            Some("stream"),
            ExecutionEvent::TextDelta {
                text: "unrelated".into(),
            },
        ),
    );
    writer.record_prompt("Main only").unwrap();
    let checkpoint = writer.checkpoint().unwrap();
    assert!(
        writer.buffer.is_empty(),
        "checkpoint is durable before worker starts"
    );
    record(
        &mut writer,
        &content(
            "alpha",
            Some("stream"),
            ExecutionEvent::TextDelta {
                text: "after".into(),
            },
        ),
    );
    writer.flush().unwrap();
    assert_eq!(
        history_content(
            store
                .agent_inputs_at(thread, &key("alpha"), checkpoint)
                .unwrap()
        ),
        vec![(
            key("alpha"),
            Some("stream".into()),
            ExecutionEvent::TextDelta {
                text: "before".into()
            }
        ),]
    );
    assert_eq!(
        history_content(store.agent_inputs(thread, &key("alpha")).unwrap()).len(),
        2
    );
    assert!(
        store
            .agent_inputs_at(thread, &key("absent"), checkpoint)
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        store.agent_inputs_at(thread, &key("alpha"), u64::MAX),
        Err(LoadError::Corrupt { .. })
    ));
}

#[test]
fn child_cache_reload_resolves_aliases_and_restores_only_target_timings() {
    let (store, thread, mut writer) = store();
    let mut provisional = AgentInfo::new(key("provisional"));
    provisional.parent = Some(Subject::Main);
    provisional.name = Some("first SDK name".into());
    provisional.coverage = TranscriptCoverage::Complete;
    record(&mut writer, &ActivityEvent::Discovered(provisional));
    record(
        &mut writer,
        &content(
            "provisional",
            Some("frame"),
            ExecutionEvent::Text {
                text: "before alias".into(),
            },
        ),
    );
    record(
        &mut writer,
        &ActivityEvent::Alias {
            from: key("provisional"),
            to: key("canonical"),
        },
    );
    record(
        &mut writer,
        &ActivityEvent::Status {
            key: key("canonical"),
            state: AgentStatus::Working,
        },
    );
    for (agent, ms) in [("canonical", 31), ("other", 57)] {
        writer
            .record_event(
                &SessionEvent::Activity(content(
                    agent,
                    Some("call"),
                    ExecutionEvent::ToolCompleted {
                        id: "same-call".into(),
                        output: "done".into(),
                        is_error: false,
                        result: ToolResult::Opaque,
                    },
                )),
                Some(Duration::from_millis(ms)),
            )
            .unwrap();
    }
    let checkpoint = writer.checkpoint().unwrap();
    let inputs = store
        .agent_inputs_at(thread, &key("provisional"), checkpoint)
        .unwrap();
    assert!(
        matches!(&inputs[0], ActivityInput::Retain(Subject::Subagent(observed)) if observed == &key("canonical"))
    );
    assert!(inputs.iter().any(|input| matches!(input, ActivityInput::ReplayEvent(ActivityEvent::Discovered(info)) if info.name.as_deref() == Some("first SDK name") && info.key == key("canonical"))));
    assert!(inputs.iter().all(|input| !matches!(
        input,
        ActivityInput::ReplayEvent(ActivityEvent::Status { .. } | ActivityEvent::Decision { .. })
            | ActivityInput::Replay(_)
    )));
    assert!(inputs.iter().any(|input| matches!(input, ActivityInput::RestoreTimings { subject: Subject::Subagent(observed), timings } if observed == &key("canonical") && timings["same-call"] == Duration::from_millis(31))));
    assert!(
        history_content(inputs)
            .iter()
            .all(|(observed, _, _)| observed == &key("canonical"))
    );
}

#[test]
fn bounded_cache_reload_marks_omitted_local_history_partial() {
    let (store, thread, mut writer) = store();
    let mut info = AgentInfo::new(key("child"));
    info.coverage = TranscriptCoverage::Complete;
    record(&mut writer, &ActivityEvent::Discovered(info));
    for index in 0..12 {
        record(
            &mut writer,
            &content(
                "child",
                Some(&format!("frame-{index}")),
                ExecutionEvent::Text {
                    text: format!("record {index}"),
                },
            ),
        );
    }
    writer.flush().unwrap();
    let snapshot = store.load(thread).unwrap();
    let inputs = activity::agent_inputs(
        &snapshot.records,
        &key("child"),
        crate::activity::ActivityLimits {
            blocks_per_subject: 1,
            content_bytes_per_subject: 1024 * 1024,
            ..Default::default()
        },
    );
    assert!(inputs.iter().any(|input| matches!(
        input,
        ActivityInput::ReplayEvent(ActivityEvent::Coverage {
            coverage: TranscriptCoverage::Partial,
            ..
        })
    )));
    let content = history_content(inputs);
    assert_eq!(content.len(), 8);
    assert_eq!(content.first().unwrap().1.as_deref(), Some("frame-4"));
    assert_eq!(content.last().unwrap().1.as_deref(), Some("frame-11"));
}

#[test]
fn child_cache_reload_keeps_oversized_records_on_disk_and_allows_reattachment() {
    let (store, thread, mut writer) = store();
    record(&mut writer, &ActivityEvent::Detached { key: key("child") });
    let mut info = AgentInfo::new(key("child"));
    info.parent = Some(Subject::Main);
    record(&mut writer, &ActivityEvent::Discovered(info));
    record(
        &mut writer,
        &content(
            "child",
            Some("large-frame"),
            ExecutionEvent::Text {
                text: "x".repeat(4096),
            },
        ),
    );
    record(
        &mut writer,
        &content(
            "child",
            Some("recent-frame"),
            ExecutionEvent::Text {
                text: "recent".into(),
            },
        ),
    );
    writer.flush().unwrap();
    let snapshot = store.load(thread).unwrap();
    let inputs = activity::agent_inputs(
        &snapshot.records,
        &key("child"),
        crate::activity::ActivityLimits {
            content_bytes_per_subject: 1024,
            ..Default::default()
        },
    );
    assert!(!inputs.iter().any(|input| matches!(
        input,
        ActivityInput::ReplayEvent(ActivityEvent::Detached { .. })
    )));
    assert!(inputs.iter().any(|input| matches!(
        input,
        ActivityInput::ReplayEvent(ActivityEvent::Coverage {
            coverage: TranscriptCoverage::Partial,
            ..
        })
    )));
    let content = history_content(inputs);
    assert_eq!(content.len(), 1);
    assert_eq!(content[0].1.as_deref(), Some("recent-frame"));
    assert!(
        fs::read_to_string(store.log_path(thread))
            .unwrap()
            .contains(&"x".repeat(4096))
    );
}

#[test]
fn durable_replay_cannot_make_working_history_or_approvals_current() {
    let (store, thread, mut writer) = store();
    let mut info = AgentInfo::new(key("child"));
    info.parent = Some(Subject::Main);
    record(&mut writer, &ActivityEvent::Discovered(info));
    record(
        &mut writer,
        &ActivityEvent::Status {
            key: key("child"),
            state: AgentStatus::Working,
        },
    );
    record(
        &mut writer,
        &content(
            "child",
            Some("call"),
            ExecutionEvent::ToolStarted {
                id: "call".into(),
                name: "Bash".into(),
                input: serde_json::json!({"command":"fixture"}),
            },
        ),
    );
    writer.flush().unwrap();
    let mut activity = crate::activity::Activity::default();
    for input in store.load(thread).unwrap().activity_inputs() {
        let update = activity.apply(input);
        assert!(!update.main_turn_ended);
        assert!(update.accepted.is_empty());
    }
    let subject = Subject::Subagent(key("child"));
    let child = activity.view().subject(&subject).unwrap();
    assert!(!child.fresh());
    assert!(!child.busy());
    assert!(!child.transcript().blocks().is_empty());
    assert!(activity.view().pending_decisions().is_empty());
    assert!(activity.view().main().transcript().blocks().is_empty());
}

#[test]
fn replay_timings_use_latest_completion_after_alias_resolution() {
    let (store, thread, mut writer) = store();
    for (agent, ms) in [("z-provisional", 31), ("a-canonical", 57)] {
        writer
            .record_event(
                &SessionEvent::Activity(content(
                    agent,
                    Some("call"),
                    ExecutionEvent::ToolCompleted {
                        id: "reused-call".into(),
                        output: "done".into(),
                        is_error: false,
                        result: ToolResult::Opaque,
                    },
                )),
                Some(Duration::from_millis(ms)),
            )
            .unwrap();
    }
    record(
        &mut writer,
        &ActivityEvent::Alias {
            from: key("z-provisional"),
            to: key("a-canonical"),
        },
    );
    writer.flush().unwrap();
    let restorations: Vec<_> = store
        .load(thread)
        .unwrap()
        .activity_inputs()
        .into_iter()
        .filter_map(|input| match input {
            ActivityInput::RestoreTimings { subject, timings } => Some((subject, timings)),
            _ => None,
        })
        .collect();
    assert_eq!(restorations.len(), 1);
    assert_eq!(restorations[0].0, Subject::Subagent(key("a-canonical")));
    assert_eq!(restorations[0].1["reused-call"], Duration::from_millis(57));
}

#[test]
fn cache_restores_historical_failure_rows_without_replacing_matching_live_status() {
    let (store, thread, mut writer) = store();
    for state in [
        AgentStatus::Working,
        AgentStatus::Interrupted,
        AgentStatus::Interrupted,
        AgentStatus::Working,
        AgentStatus::Failed,
    ] {
        record(
            &mut writer,
            &ActivityEvent::Status {
                key: key("child"),
                state,
            },
        );
    }
    writer.flush().unwrap();
    let mut activity = crate::activity::Activity::new(crate::activity::ActivityLimits {
        max_children: 0,
        ..Default::default()
    });
    activity.apply(ActivityInput::Connect { generation: 7 });
    activity.apply(ActivityInput::Observe {
        generation: 7,
        at: std::time::Instant::now(),
        event: ActivityEvent::Status {
            key: key("child"),
            state: AgentStatus::Failed,
        },
    });
    let subject = Subject::Subagent(key("child"));
    assert!(!activity.view().subject(&subject).unwrap().retained());
    for input in store.agent_inputs(thread, &key("child")).unwrap() {
        let update = activity.apply(input);
        assert!(update.accepted.is_empty());
        assert!(!update.main_turn_ended);
    }
    let child = activity.view().subject(&subject).unwrap();
    assert!(child.fresh());
    assert_eq!(child.status(), AgentStatus::Failed);
    let rows: Vec<_> = child
        .transcript()
        .blocks()
        .iter()
        .filter_map(|block| match &block.body {
            crate::transcript::Body::Meta(text) | crate::transcript::Body::Notice(text) => {
                Some(text.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(rows, vec!["interrupted", "Subagent failed"]);
}

#[test]
fn streaming_child_reader_recovers_torn_tail_and_refuses_future_header() {
    let (store, thread, mut writer) = store();
    record(
        &mut writer,
        &content(
            "child",
            Some("before"),
            ExecutionEvent::Text {
                text: "survives".into(),
            },
        ),
    );
    let checkpoint = writer.checkpoint().unwrap();
    drop(writer);
    let mut file = OpenOptions::new()
        .append(true)
        .open(store.log_path(thread))
        .unwrap();
    file.write_all(b"{\"type\":\"activity\",\"observation\":\"")
        .unwrap();
    file.write_all(&[0xf0, 0x9f]).unwrap();
    drop(file);
    assert_eq!(
        history_content(store.agent_inputs(thread, &key("child")).unwrap()),
        history_content(
            store
                .agent_inputs_at(thread, &key("child"), checkpoint)
                .unwrap()
        )
    );
    let mut writer = store.writer(thread).unwrap(); // Same torn-tail repair and schema upgrade path.
    record(
        &mut writer,
        &content(
            "child",
            Some("after"),
            ExecutionEvent::Text {
                text: "also survives".into(),
            },
        ),
    );
    writer.flush().unwrap();
    assert_eq!(
        history_content(store.agent_inputs(thread, &key("child")).unwrap()).len(),
        2
    );
    drop(writer);
    fs::write(
        store.log_path(thread),
        "{\"schema\":10,\"provider\":\"claude\"}\n",
    )
    .unwrap();
    assert!(matches!(
        store.agent_inputs(thread, &key("child")),
        Err(LoadError::FutureSchema { found: 10, .. })
    ));
}

#[test]
fn streaming_reader_filters_large_unrelated_history_and_late_aliases_in_two_passes() {
    let (store, thread, mut writer) = store();
    // A late alias must still recover earlier records without retaining all
    // intervening Main/sibling history in the selected child's projection.
    record(
        &mut writer,
        &content(
            "provisional",
            Some("early"),
            ExecutionEvent::Text {
                text: "early child".into(),
            },
        ),
    );
    for index in 0..256 {
        record(
            &mut writer,
            &content(
                "unrelated",
                Some(&format!("other-{index}")),
                ExecutionEvent::Text {
                    text: "x".repeat(8192),
                },
            ),
        );
        if index % 8 == 0 {
            writer.flush().unwrap();
        }
    }
    record(
        &mut writer,
        &ActivityEvent::Alias {
            from: key("provisional"),
            to: key("canonical"),
        },
    );
    record(
        &mut writer,
        &content(
            "canonical",
            Some("late"),
            ExecutionEvent::Text {
                text: "late child".into(),
            },
        ),
    );
    let checkpoint = writer.checkpoint().unwrap();
    let inputs = activity::read_agent_inputs(
        File::open(store.log_path(thread)).unwrap(),
        thread,
        &key("canonical"),
        checkpoint,
        crate::activity::ActivityLimits {
            content_bytes_per_subject: 1024,
            ..Default::default()
        },
    )
    .unwrap();
    let content = history_content(inputs);
    assert_eq!(content.len(), 2);
    assert_eq!(content[0].0, key("canonical"));
    assert_eq!(content[0].1.as_deref(), Some("early"));
    assert_eq!(content[1].1.as_deref(), Some("late"));
    assert!(checkpoint > 2 * 1024 * 1024);
}

#[test]
fn schema_eight_child_scan_remains_empty_then_reads_appended_schema_nine() {
    let (store, thread, writer) = store();
    drop(writer);
    fs::write(
        store.log_path(thread),
        concat!(
            "{\"schema\":8,\"provider\":\"claude\"}\n",
            "{\"type\":\"prompt\",\"text\":\"Main only\"}\n",
            "{\"type\":\"text\",\"text\":\"old answer\"}\n"
        ),
    )
    .unwrap();
    assert!(
        store
            .agent_inputs(thread, &key("child"))
            .unwrap()
            .is_empty()
    );
    let mut writer = store.writer(thread).unwrap();
    record(
        &mut writer,
        &content(
            "child",
            Some("new"),
            ExecutionEvent::Text {
                text: "new child".into(),
            },
        ),
    );
    let checkpoint = writer.checkpoint().unwrap();
    assert_eq!(
        history_content(
            store
                .agent_inputs_at(thread, &key("child"), checkpoint)
                .unwrap()
        )
        .len(),
        1
    );
    assert_eq!(
        store.load(thread).unwrap().prompt_texts(),
        vec!["Main only"]
    );
}
