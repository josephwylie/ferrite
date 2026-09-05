use std::time::{Duration, Instant};

use ferrite_core::activity::{
    Activity, ActivityEvent, ActivityInput, ActivityLimits, AgentInfo, AgentKey, AgentStatus,
    ExecutionEvent, Subject, TranscriptCoverage,
};
use ferrite_core::store::Provider;
use ferrite_core::transcript::{Body, Input};
use ferrite_core::{Decision, SessionEvent, ToolResult, TurnOutcome};

fn key(native: &str) -> AgentKey {
    AgentKey::new(Provider::Claude, "root", native)
}
fn connected() -> Activity {
    let mut activity = Activity::default();
    activity.apply(ActivityInput::Connect { generation: 7 });
    activity
}
fn observe(
    activity: &mut Activity,
    event: ActivityEvent,
) -> ferrite_core::activity::ActivityUpdate {
    activity.apply(ActivityInput::Observe {
        generation: 7,
        event,
        at: Instant::now(),
    })
}
fn discover(activity: &mut Activity, native: &str) -> AgentKey {
    let key = key(native);
    let mut info = AgentInfo::new(key.clone());
    info.parent = Some(Subject::Main);
    info.coverage = TranscriptCoverage::Live;
    observe(activity, ActivityEvent::Discovered(info));
    key
}
fn content(
    activity: &mut Activity,
    key: &AgentKey,
    id: &str,
    event: ExecutionEvent,
) -> ferrite_core::activity::ActivityUpdate {
    observe(
        activity,
        ActivityEvent::Content {
            key: key.clone(),
            id: Some(id.into()),
            event,
        },
    )
}
fn text(activity: &Activity, subject: &Subject) -> String {
    activity
        .view()
        .subject(subject)
        .unwrap()
        .transcript()
        .blocks()
        .iter()
        .filter_map(|block| match &block.body {
            Body::Paragraph { spans } | Body::Heading { spans, .. } | Body::Bullet { spans } => {
                Some(
                    spans
                        .iter()
                        .map(|span| span.text.as_str())
                        .collect::<String>(),
                )
            }
            Body::Thinking(text) | Body::Prompt(text) | Body::Notice(text) => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn decision(id: &str) -> Decision {
    Decision {
        id: id.into(),
        tool_use_id: "shared-tool".into(),
        tool_name: "Bash".into(),
        description: "Allow command".into(),
        input: serde_json::Value::Null,
        suggestions: vec![],
    }
}

#[test]
fn interleaving_child_content_and_completion_never_ends_main() {
    let mut activity = connected();
    let a = discover(&mut activity, "a");
    let b = discover(&mut activity, "b");
    activity.apply(ActivityInput::Main {
        input: Input::Event(SessionEvent::TextDelta {
            text: "MAIN".into(),
        }),
        at: Instant::now(),
    });
    content(
        &mut activity,
        &a,
        "frame",
        ExecutionEvent::Text {
            text: "CHILD A".into(),
        },
    );
    content(
        &mut activity,
        &b,
        "frame",
        ExecutionEvent::Text {
            text: "CHILD B".into(),
        },
    );
    let ended = content(
        &mut activity,
        &a,
        "turn",
        ExecutionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        },
    );
    assert!(!ended.main_turn_ended);
    assert!(activity.view().main().busy());
    assert_eq!(text(&activity, &Subject::Main), "MAIN");
    assert_eq!(text(&activity, &Subject::Subagent(a)), "CHILD A");
    assert_eq!(text(&activity, &Subject::Subagent(b)), "CHILD B");
}

#[test]
fn complete_frames_deduplicate_by_delivery_not_shared_message_id() {
    let mut activity = connected();
    let a = discover(&mut activity, "a");
    content(
        &mut activity,
        &a,
        "uuid-1:0",
        ExecutionEvent::Text {
            text: "first block".into(),
        },
    );
    content(
        &mut activity,
        &a,
        "uuid-2:0",
        ExecutionEvent::Text {
            text: "second block".into(),
        },
    );
    let duplicate = content(
        &mut activity,
        &a,
        "uuid-1:0",
        ExecutionEvent::Text {
            text: "first block".into(),
        },
    );
    assert!(duplicate.accepted.is_empty());
    assert_eq!(
        text(&activity, &Subject::Subagent(a)),
        "first block\nsecond block"
    );
}

#[test]
fn completed_item_reconciles_deltas_and_preserves_other_items() {
    let mut activity = connected();
    let a = discover(&mut activity, "a");
    content(
        &mut activity,
        &a,
        "item-1",
        ExecutionEvent::TextDelta { text: "par".into() },
    );
    content(
        &mut activity,
        &a,
        "item-1",
        ExecutionEvent::TextDelta {
            text: "tial".into(),
        },
    );
    content(
        &mut activity,
        &a,
        "item-2",
        ExecutionEvent::Text {
            text: "other item".into(),
        },
    );
    content(
        &mut activity,
        &a,
        "item-1",
        ExecutionEvent::TextSnapshot {
            text: "authoritative\n\n".into(),
        },
    );
    let rendered = text(&activity, &Subject::Subagent(a));
    assert_eq!(rendered, "authoritative\nother item");
    assert!(!rendered.contains("partial"));
}

#[test]
fn summary_deltas_and_thinking_snapshot_share_one_item() {
    let mut activity = connected();
    let a = discover(&mut activity, "a");
    content(
        &mut activity,
        &a,
        "reasoning",
        ExecutionEvent::ReasoningSummaryDelta {
            text: "early".into(),
            summary_index: 0,
        },
    );
    content(
        &mut activity,
        &a,
        "reasoning",
        ExecutionEvent::ThinkingSnapshot {
            text: "finished".into(),
        },
    );
    assert_eq!(text(&activity, &Subject::Subagent(a)), "finished");
}

#[test]
fn working_order_is_stable_and_completed_identity_can_work_again() {
    let mut activity = connected();
    let a = discover(&mut activity, "a");
    let b = discover(&mut activity, "b");
    observe(
        &mut activity,
        ActivityEvent::Status {
            key: b.clone(),
            state: AgentStatus::Working,
        },
    );
    assert_eq!(activity.view().children()[0].key(), &b);
    observe(
        &mut activity,
        ActivityEvent::Status {
            key: b.clone(),
            state: AgentStatus::Idle,
        },
    );
    assert_eq!(activity.view().children()[0].key(), &a);
    observe(
        &mut activity,
        ActivityEvent::Status {
            key: b.clone(),
            state: AgentStatus::Working,
        },
    );
    assert_eq!(activity.view().children()[0].key(), &b);
    assert_eq!(
        activity
            .view()
            .subject(&Subject::Subagent(b))
            .unwrap()
            .last_outcome(),
        Some(&TurnOutcome::Completed)
    );
}

#[test]
fn decisions_keep_request_identity_through_owner_enrichment_and_session_replacement() {
    let mut activity = connected();
    let a = discover(&mut activity, "a");
    observe(
        &mut activity,
        ActivityEvent::Decision {
            subject: None,
            decision: decision("request"),
        },
    );
    let handle = activity.view().decisions()[0].handle.clone();
    observe(
        &mut activity,
        ActivityEvent::Decision {
            subject: Some(Subject::Subagent(a.clone())),
            decision: decision("request"),
        },
    );
    assert_eq!(activity.view().decisions().len(), 1);
    assert_eq!(activity.view().decisions()[0].handle, handle);
    assert_eq!(
        activity.view().decisions()[0].subject,
        Some(Subject::Subagent(a))
    );
    activity.apply(ActivityInput::Connect { generation: 8 });
    activity.apply(ActivityInput::Observe {
        generation: 8,
        event: ActivityEvent::Decision {
            subject: Some(Subject::Main),
            decision: decision("request"),
        },
        at: Instant::now(),
    });
    let result = activity.apply(ActivityInput::Answered {
        handle,
        allowed: true,
        at: Instant::now(),
    });
    assert!(result.rejected);
    assert_eq!(activity.view().decisions().len(), 1);
}

#[test]
fn another_child_ending_or_answering_cannot_clear_a_pending_request() {
    let mut activity = connected();
    let a = discover(&mut activity, "a");
    let b = discover(&mut activity, "b");
    for (key, id) in [(&a, "a-request"), (&b, "b-request")] {
        observe(
            &mut activity,
            ActivityEvent::Decision {
                subject: Some(Subject::Subagent(key.clone())),
                decision: decision(id),
            },
        );
    }
    let b_handle = activity.view().decisions()[1].handle.clone();
    activity.apply(ActivityInput::Answered {
        handle: b_handle,
        allowed: false,
        at: Instant::now(),
    });
    observe(
        &mut activity,
        ActivityEvent::Status {
            key: b,
            state: AgentStatus::Idle,
        },
    );
    assert_eq!(activity.view().decisions().len(), 1);
    assert_eq!(activity.view().decisions()[0].decision.id, "a-request");
}

#[test]
fn replay_has_no_actionable_decisions_or_running_indicators() {
    let a = key("a");
    let mut info = AgentInfo::new(a.clone());
    info.parent = Some(Subject::Main);
    let mut activity = Activity::default();
    activity.apply(ActivityInput::ReplayEvent(ActivityEvent::Discovered(info)));
    activity.apply(ActivityInput::ReplayEvent(ActivityEvent::Status {
        key: a.clone(),
        state: AgentStatus::Working,
    }));
    activity.apply(ActivityInput::ReplayEvent(ActivityEvent::Decision {
        subject: Some(Subject::Subagent(a)),
        decision: decision("old"),
    }));
    assert!(activity.view().decisions().is_empty());
    assert_eq!(activity.view().working_descendants(), 0);
    assert!(!activity.view().children()[0].fresh());
}

#[test]
fn history_reads_cannot_change_live_waiting_status_or_timing() {
    let mut activity = connected();
    let a = discover(&mut activity, "a");
    content(
        &mut activity,
        &a,
        "live-tool-start",
        ExecutionEvent::ToolStarted {
            id: "live-tool".into(),
            name: "Bash".into(),
            input: serde_json::Value::Null,
        },
    );
    observe(
        &mut activity,
        ActivityEvent::Decision {
            subject: Some(Subject::Subagent(a.clone())),
            decision: decision("pending"),
        },
    );
    observe(
        &mut activity,
        ActivityEvent::HistoryContent {
            key: a.clone(),
            id: Some("old-message".into()),
            event: ExecutionEvent::TextSnapshot {
                text: "old history".into(),
            },
        },
    );
    let view = activity.view().subject(&Subject::Subagent(a)).unwrap();
    assert_eq!(view.status(), AgentStatus::Waiting);
    assert_eq!(
        view.transcript().status(),
        ferrite_core::transcript::Status::Blocked
    );
    assert!(view.fresh());
    assert!(matches!(
        view.timings()["live-tool"],
        ferrite_core::activity::ToolTiming::Running(_)
    ));
    assert_eq!(activity.view().decisions().len(), 1);
}

#[test]
fn transcript_cache_limit_retains_metadata_decisions_and_durable_facts() {
    let mut activity = Activity::new(ActivityLimits {
        max_children: 1,
        ..ActivityLimits::default()
    });
    activity.apply(ActivityInput::Connect { generation: 7 });
    let a = discover(&mut activity, "a");
    let b = discover(&mut activity, "b");
    content(
        &mut activity,
        &a,
        "a-frame",
        ExecutionEvent::Text {
            text: "first".into(),
        },
    );
    let stored = content(
        &mut activity,
        &b,
        "b-frame",
        ExecutionEvent::Text {
            text: "second".into(),
        },
    );
    assert_eq!(stored.accepted.len(), 1);
    assert_eq!(activity.view().children().len(), 2);
    assert!(
        !activity
            .view()
            .subject(&Subject::Subagent(b.clone()))
            .unwrap()
            .retained()
    );
    observe(
        &mut activity,
        ActivityEvent::Decision {
            subject: Some(Subject::Subagent(b.clone())),
            decision: decision("pending"),
        },
    );
    activity.apply(ActivityInput::Retain(Subject::Subagent(b.clone())));
    activity.apply(ActivityInput::ReplayEvent(ActivityEvent::HistoryContent {
        key: b.clone(),
        id: Some("b-frame".into()),
        event: ExecutionEvent::Text {
            text: "second".into(),
        },
    }));
    assert_eq!(text(&activity, &Subject::Subagent(b)), "second");
    assert!(
        !activity
            .view()
            .subject(&Subject::Subagent(a))
            .unwrap()
            .retained()
    );
    assert_eq!(activity.view().decisions().len(), 1);
}

#[test]
fn oversized_render_content_is_bounded_and_marks_partial_coverage() {
    let mut activity = Activity::new(ActivityLimits {
        content_bytes_per_subject: 32,
        ..ActivityLimits::default()
    });
    activity.apply(ActivityInput::Connect { generation: 7 });
    let a = discover(&mut activity, "a");
    let update = content(
        &mut activity,
        &a,
        "frame",
        ExecutionEvent::Text {
            text: "界".repeat(1000),
        },
    );
    assert_eq!(update.accepted.len(), 1);
    assert!(text(&activity, &Subject::Subagent(a.clone())).len() <= 32);
    observe(
        &mut activity,
        ActivityEvent::Coverage {
            key: a.clone(),
            coverage: TranscriptCoverage::Complete,
        },
    );
    assert_eq!(
        activity
            .view()
            .subject(&Subject::Subagent(a))
            .unwrap()
            .coverage(),
        TranscriptCoverage::Partial
    );
}

#[test]
fn highlights_are_scoped_even_when_children_reuse_block_ids() {
    let mut activity = connected();
    let a = discover(&mut activity, "a");
    let b = discover(&mut activity, "b");
    for (key, source) in [(&a, "fn alpha() {}"), (&b, "fn beta() {}")] {
        content(
            &mut activity,
            key,
            "frame",
            ExecutionEvent::Text {
                text: format!("```rust\n{source}\n```\n"),
            },
        );
    }
    activity.apply(ActivityInput::DrainHighlights);
    for (key, expected) in [(a, "alpha"), (b, "beta")] {
        let view = activity.view().subject(&Subject::Subagent(key)).unwrap();
        let code = view
            .transcript()
            .blocks()
            .iter()
            .find_map(|block| {
                if let Body::Code {
                    tokens: Some(tokens),
                    ..
                } = &block.body
                {
                    Some(
                        tokens
                            .iter()
                            .map(|token| token.text.as_str())
                            .collect::<String>(),
                    )
                } else {
                    None
                }
            })
            .expect("highlighted child code");
        assert!(code.contains(expected));
    }
}

#[test]
fn descendant_discovery_waits_for_verified_parent_and_rejects_cycles() {
    let mut activity = connected();
    let child = key("child");
    let parent = key("parent");
    let mut info = AgentInfo::new(child.clone());
    info.parent = Some(Subject::Subagent(parent.clone()));
    observe(&mut activity, ActivityEvent::Discovered(info));
    assert!(activity.view().children().is_empty());
    discover(&mut activity, "parent");
    assert_eq!(activity.view().children().len(), 2);
    let mut cyclic = AgentInfo::new(parent);
    cyclic.parent = Some(Subject::Subagent(child));
    assert!(observe(&mut activity, ActivityEvent::Discovered(cyclic)).rejected);
}

#[test]
fn same_tool_id_is_timed_separately_per_subject() {
    let mut activity = connected();
    let a = discover(&mut activity, "a");
    let b = discover(&mut activity, "b");
    let now = Instant::now();
    for key in [&a, &b] {
        activity.apply(ActivityInput::Observe {
            generation: 7,
            at: now,
            event: ActivityEvent::Content {
                key: key.clone(),
                id: Some("start".into()),
                event: ExecutionEvent::ToolStarted {
                    id: "shared".into(),
                    name: "Bash".into(),
                    input: serde_json::Value::Null,
                },
            },
        });
    }
    activity.apply(ActivityInput::Observe {
        generation: 7,
        at: now + Duration::from_secs(2),
        event: ActivityEvent::Content {
            key: a.clone(),
            id: Some("end".into()),
            event: ExecutionEvent::ToolCompleted {
                id: "shared".into(),
                output: String::new(),
                is_error: false,
                result: ToolResult::Opaque,
            },
        },
    });
    assert!(
        matches!(activity.view().subject(&Subject::Subagent(a)).unwrap().timings()["shared"], ferrite_core::activity::ToolTiming::Done(total) if total == Duration::from_secs(2))
    );
    assert!(matches!(
        activity
            .view()
            .subject(&Subject::Subagent(b))
            .unwrap()
            .timings()["shared"],
        ferrite_core::activity::ToolTiming::Running(_)
    ));
}

#[test]
fn cancelling_one_request_keeps_other_attention_without_inventing_completion() {
    let mut activity = connected();
    let a = discover(&mut activity, "a");
    let subject = Subject::Subagent(a);
    for id in ["first", "second"] {
        observe(
            &mut activity,
            ActivityEvent::Decision {
                subject: Some(subject.clone()),
                decision: decision(id),
            },
        );
    }
    observe(
        &mut activity,
        ActivityEvent::DecisionCancelled { id: "first".into() },
    );
    assert_eq!(
        activity.view().subject(&subject).unwrap().status(),
        AgentStatus::Waiting
    );
    assert_eq!(
        activity
            .view()
            .subject(&subject)
            .unwrap()
            .transcript()
            .status(),
        ferrite_core::transcript::Status::Blocked
    );
    observe(
        &mut activity,
        ActivityEvent::DecisionCancelled {
            id: "second".into(),
        },
    );
    let view = activity.view().subject(&subject).unwrap();
    assert_eq!(view.status(), AgentStatus::Unknown);
    assert_ne!(
        view.transcript().status(),
        ferrite_core::transcript::Status::Blocked
    );
    assert!(view.last_outcome().is_none());
}

#[test]
fn background_completion_settles_autonomous_work_but_preserves_operator_turn() {
    let mut activity = connected();
    activity.apply(ActivityInput::Main {
        input: Input::Prompt("operator".into()),
        at: Instant::now(),
    });
    activity.apply(ActivityInput::Main {
        input: Input::Event(SessionEvent::TextDelta {
            text: "foreground".into(),
        }),
        at: Instant::now(),
    });
    let background = ActivityEvent::BackgroundTurnEnded {
        outcome: TurnOutcome::Completed,
        cost_usd: None,
    };
    let update = observe(&mut activity, background.clone());
    assert!(!update.main_turn_ended);
    assert!(activity.view().main().busy());
    activity.apply(ActivityInput::Main {
        input: Input::Event(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        }),
        at: Instant::now(),
    });
    activity.apply(ActivityInput::Main {
        input: Input::Event(SessionEvent::TextDelta {
            text: "autonomous".into(),
        }),
        at: Instant::now(),
    });
    assert!(activity.view().main().busy());
    assert!(!observe(&mut activity, background).main_turn_ended);
    assert!(!activity.view().main().busy());
    assert_eq!(
        activity.view().main().transcript().status(),
        ferrite_core::transcript::Status::Idle
    );
}

#[test]
fn alias_join_preserves_latest_runtime_and_complete_frame_identity() {
    let mut activity = connected();
    let canonical = discover(&mut activity, "canonical");
    let provisional = discover(&mut activity, "provisional");
    content(
        &mut activity,
        &provisional,
        "delivery",
        ExecutionEvent::Text {
            text: "observed once".into(),
        },
    );
    let update = observe(
        &mut activity,
        ActivityEvent::Alias {
            from: provisional.clone(),
            to: canonical.clone(),
        },
    );
    assert!(!update.rejected);
    assert_eq!(activity.view().children().len(), 1);
    assert_eq!(
        activity
            .view()
            .canonical_subject(&Subject::Subagent(provisional)),
        Subject::Subagent(canonical.clone())
    );
    assert_eq!(
        activity
            .view()
            .subject(&Subject::Subagent(canonical.clone()))
            .unwrap()
            .status(),
        AgentStatus::Working
    );
    assert!(
        content(
            &mut activity,
            &canonical,
            "delivery",
            ExecutionEvent::Text {
                text: "observed once".into()
            }
        )
        .rejected
    );
    assert_eq!(
        text(&activity, &Subject::Subagent(canonical)),
        "observed once"
    );
}

#[test]
fn alias_cannot_join_an_ancestor_and_descendant() {
    let mut activity = connected();
    let ancestor = discover(&mut activity, "ancestor");
    let child = key("child");
    let mut info = AgentInfo::new(child.clone());
    info.parent = Some(Subject::Subagent(ancestor.clone()));
    observe(&mut activity, ActivityEvent::Discovered(info));
    let update = observe(
        &mut activity,
        ActivityEvent::Alias {
            from: ancestor,
            to: child,
        },
    );
    assert!(update.rejected);
    assert!(update.accepted.is_empty());
    assert_eq!(activity.view().children().len(), 2);
}

#[test]
fn cold_aliases_are_preserved_independently_of_transcript_cache_limit() {
    let mut activity = Activity::new(ActivityLimits {
        max_children: 1,
        ..ActivityLimits::default()
    });
    activity.apply(ActivityInput::Connect { generation: 7 });
    let canonical = discover(&mut activity, "canonical");
    for index in 0..32 {
        let provisional = key(&format!("task-{index}"));
        assert!(
            !observe(
                &mut activity,
                ActivityEvent::Alias {
                    from: provisional.clone(),
                    to: canonical.clone()
                }
            )
            .rejected
        );
        assert_eq!(
            activity
                .view()
                .canonical_subject(&Subject::Subagent(provisional)),
            Subject::Subagent(canonical.clone())
        );
    }
}

#[test]
fn cache_retry_discards_projection_but_preserves_live_request_and_runtime() {
    let mut activity = connected();
    let a = discover(&mut activity, "a");
    let subject = Subject::Subagent(a.clone());
    content(
        &mut activity,
        &a,
        "frame",
        ExecutionEvent::Text {
            text: "one copy".into(),
        },
    );
    observe(
        &mut activity,
        ActivityEvent::Decision {
            subject: Some(subject.clone()),
            decision: decision("live"),
        },
    );
    let handle = activity.view().decisions()[0].handle.clone();
    let revision = activity.view().subject(&subject).unwrap().revision();
    activity.apply(ActivityInput::Evict(subject.clone()));
    assert!(text(&activity, &subject).is_empty());
    let view = activity.view().subject(&subject).unwrap();
    assert!(!view.retained());
    assert!(view.revision() > revision);
    assert_eq!(view.status(), AgentStatus::Waiting);
    assert_eq!(
        view.transcript().status(),
        ferrite_core::transcript::Status::Blocked
    );
    activity.apply(ActivityInput::Retain(subject.clone()));
    activity.apply(ActivityInput::ReplayEvent(ActivityEvent::HistoryContent {
        key: a,
        id: Some("frame".into()),
        event: ExecutionEvent::Text {
            text: "one copy".into(),
        },
    }));
    assert_eq!(text(&activity, &subject), "one copy");
    assert_eq!(activity.view().decisions()[0].handle, handle);
    assert_eq!(
        activity.view().subject(&subject).unwrap().status(),
        AgentStatus::Waiting
    );
}

#[test]
fn frozen_history_duration_cannot_replace_a_newer_live_completed_tool() {
    let mut activity = connected();
    let a = discover(&mut activity, "a");
    let now = Instant::now();
    for (at, id, event) in [
        (
            now,
            "start",
            ExecutionEvent::ToolStarted {
                id: "reused".into(),
                name: "Bash".into(),
                input: serde_json::Value::Null,
            },
        ),
        (
            now + Duration::from_secs(3),
            "end",
            ExecutionEvent::ToolCompleted {
                id: "reused".into(),
                output: String::new(),
                is_error: false,
                result: ToolResult::Opaque,
            },
        ),
    ] {
        activity.apply(ActivityInput::Observe {
            generation: 7,
            at,
            event: ActivityEvent::Content {
                key: a.clone(),
                id: Some(id.into()),
                event,
            },
        });
    }
    let subject = Subject::Subagent(a);
    activity.apply(ActivityInput::RestoreTimings {
        subject: subject.clone(),
        timings: [
            ("reused".into(), Duration::from_secs(1)),
            ("older".into(), Duration::from_secs(2)),
        ]
        .into(),
    });
    let timings = activity.view().subject(&subject).unwrap().timings();
    assert!(
        matches!(timings["reused"], ferrite_core::activity::ToolTiming::Done(total) if total == Duration::from_secs(3))
    );
    assert!(
        matches!(timings["older"], ferrite_core::activity::ToolTiming::Done(total) if total == Duration::from_secs(2))
    );
}
