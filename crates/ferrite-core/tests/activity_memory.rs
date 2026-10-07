use std::time::{Duration, Instant};

use ferrite_core::activity::{
    Activity, ActivityEvent, ActivityInput, ActivityLimits, ActivityUpdate, AgentInfo, AgentKey,
    AgentStatus, ExecutionEvent, Subject, ToolTiming,
};
use ferrite_core::store::Provider;
use ferrite_core::ToolResult;

fn activity() -> Activity {
    let mut activity = Activity::new(ActivityLimits {
        max_children: 1,
        blocks_per_subject: 4,
        ..ActivityLimits::default()
    });
    activity.apply(ActivityInput::Connect { generation: 1 });
    activity
}

fn observe(activity: &mut Activity, event: ActivityEvent) -> ActivityUpdate {
    let update = activity.apply(ActivityInput::Observe {
        generation: 1,
        event,
        at: Instant::now(),
    });
    assert!(!update.rejected);
    assert_eq!(
        update.accepted.len(),
        1,
        "cache eviction must not lose durable facts"
    );
    update
}

fn child(activity: &mut Activity, name: &str) -> AgentKey {
    let key = AgentKey::new(Provider::Codex, "root", name);
    let mut info = AgentInfo::new(key.clone());
    info.parent = Some(Subject::Main);
    observe(activity, ActivityEvent::Discovered(info));
    key
}

fn content(activity: &mut Activity, key: &AgentKey, event: ExecutionEvent) -> ActivityUpdate {
    observe(
        activity,
        ActivityEvent::Content {
            key: key.clone(),
            id: None,
            event,
        },
    )
}

fn started(id: &str) -> ExecutionEvent {
    ExecutionEvent::ToolStarted {
        id: id.into(),
        name: "Bash".into(),
        input: serde_json::Value::Null,
    }
}

fn completed(id: &str, duration_ms: Option<u64>) -> ExecutionEvent {
    ExecutionEvent::ToolCompleted {
        id: id.into(),
        output: String::new(),
        is_error: false,
        result: ToolResult::Command {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms,
        },
    }
}

fn evict_by_selecting_another_child(activity: &mut Activity) {
    let next = child(activity, "next");
    activity.apply(ActivityInput::Retain(Subject::Subagent(next)));
}

#[test]
fn unretained_children_do_not_accumulate_completed_tool_timings() {
    let mut activity = activity();
    let evicted = child(&mut activity, "evicted");
    evict_by_selecting_another_child(&mut activity);
    let never_retained = child(&mut activity, "never-retained");

    for key in [evicted, never_retained] {
        for index in 0..1000 {
            let id = format!("tool-{index}");
            content(&mut activity, &key, started(&id));
            let update = content(&mut activity, &key, completed(&id, Some(42)));
            assert_eq!(
                update.completed_tool_duration,
                Some(Duration::from_millis(42))
            );
        }
        let view = activity.view().subject(&Subject::Subagent(key)).unwrap();
        assert!(!view.retained());
        assert!(view.transcript().blocks().is_empty());
        assert!(
            view.timings().is_empty(),
            "unretained child kept {} completed timings",
            view.timings().len()
        );
        assert_eq!(
            view.status(),
            AgentStatus::Working,
            "live activity still advances"
        );
        assert!(view.fresh());
    }
}

#[test]
fn eviction_releases_completed_timings_and_keeps_inflight_tools_until_completion() {
    for duration_ms in [None, Some(42)] {
        let mut activity = activity();
        let key = child(&mut activity, "working");
        let subject = Subject::Subagent(key.clone());
        content(&mut activity, &key, started("finished"));
        content(&mut activity, &key, completed("finished", Some(10)));
        content(&mut activity, &key, started("inflight"));
        assert_eq!(
            activity.view().subject(&subject).unwrap().timings().len(),
            2
        );

        evict_by_selecting_another_child(&mut activity);
        let view = activity.view().subject(&subject).unwrap();
        assert!(!view.retained());
        assert_eq!(
            view.timings().len(),
            1,
            "completed timings belong to the evicted history"
        );
        assert!(matches!(view.timings()["inflight"], ToolTiming::Running(_)));

        let update = content(&mut activity, &key, completed("inflight", duration_ms));
        assert!(update.completed_tool_duration.is_some());
        if let Some(duration_ms) = duration_ms {
            assert_eq!(
                update.completed_tool_duration,
                Some(Duration::from_millis(duration_ms))
            );
        }
        assert!(activity
            .view()
            .subject(&subject)
            .unwrap()
            .timings()
            .is_empty());
    }
}

#[test]
fn only_accepted_live_completions_publish_a_duration_for_persistence() {
    let mut activity = activity();
    let key = child(&mut activity, "history");
    let event = ActivityEvent::Content {
        key: key.clone(),
        id: Some("completion".into()),
        event: completed("native", Some(42)),
    };
    let observe = |event, generation| ActivityInput::Observe {
        generation,
        event,
        at: Instant::now(),
    };
    let update = activity.apply(observe(event.clone(), 1));
    assert_eq!(
        update.completed_tool_duration,
        Some(Duration::from_millis(42))
    );

    let duplicate = activity.apply(observe(event.clone(), 1));
    assert!(duplicate.rejected);
    assert!(duplicate.completed_tool_duration.is_none());
    let obsolete = activity.apply(observe(event, 2));
    assert!(obsolete.rejected);
    assert!(obsolete.completed_tool_duration.is_none());

    let historical = ActivityEvent::HistoryContent {
        key: key.clone(),
        id: Some("historical-completion".into()),
        event: completed("historical", Some(17)),
    };
    let history = activity.apply(observe(historical, 1));
    assert!(!history.rejected);
    assert!(history.completed_tool_duration.is_none());
    let replay = activity.apply(ActivityInput::ReplayEvent(ActivityEvent::Content {
        key: key.clone(),
        id: Some("replayed-completion".into()),
        event: completed("replayed", Some(23)),
    }));
    assert!(!replay.rejected);
    assert!(replay.completed_tool_duration.is_none());
    let clockless = content(&mut activity, &key, completed("unobserved", None));
    assert!(clockless.completed_tool_duration.is_none());
}

#[test]
fn evicted_inflight_timings_are_released_when_a_turn_or_session_ends() {
    for disconnect in [false, true] {
        let mut activity = activity();
        let key = child(&mut activity, "working");
        let subject = Subject::Subagent(key.clone());
        content(&mut activity, &key, started("inflight"));
        evict_by_selecting_another_child(&mut activity);
        assert!(matches!(
            activity.view().subject(&subject).unwrap().timings()["inflight"],
            ToolTiming::Running(_)
        ));

        if disconnect {
            activity.apply(ActivityInput::Disconnect);
        } else {
            content(
                &mut activity,
                &key,
                ExecutionEvent::TurnEnded {
                    outcome: ferrite_core::TurnOutcome::Completed,
                    cost_usd: None,
                },
            );
        }
        let view = activity.view().subject(&subject).unwrap();
        assert!(view.timings().is_empty());
        assert!(!view.busy());
    }
}

#[test]
fn historical_timings_are_cached_only_after_the_subject_is_retained() {
    let mut activity = activity();
    let key = child(&mut activity, "history");
    let subject = Subject::Subagent(key);
    evict_by_selecting_another_child(&mut activity);
    let restore = || ActivityInput::RestoreTimings {
        subject: subject.clone(),
        timings: [("finished".into(), Duration::from_millis(42))].into(),
    };
    activity.apply(restore());
    assert!(activity
        .view()
        .subject(&subject)
        .unwrap()
        .timings()
        .is_empty());

    activity.apply(ActivityInput::Retain(subject.clone()));
    activity.apply(restore());
    assert!(
        matches!(activity.view().subject(&subject).unwrap().timings()["finished"], ToolTiming::Done(elapsed) if elapsed == Duration::from_millis(42))
    );
}

/// One turn of a long Main Thread: a prompt, an answer streamed in deltas,
/// a tool call and its result, and the turn's end. Answers how many bytes
/// of content it streamed.
fn stream_turn(activity: &mut Activity, turn: usize) -> usize {
    use ferrite_core::transcript::Input;
    use ferrite_core::{SessionEvent, TurnOutcome};
    let mut main = |input: Input| {
        activity.apply(ActivityInput::Main {
            input,
            at: Instant::now(),
        });
    };
    let mut streamed = 0;
    main(Input::Prompt(format!("prompt {turn}")));
    for delta in 0..8 {
        let text = format!("{}{turn}.{delta}\n\n", "streamed answer words ".repeat(12));
        streamed += text.len();
        main(Input::Event(SessionEvent::TextDelta { text }));
    }
    let call = format!("call-{turn}");
    main(Input::Event(SessionEvent::ToolStarted {
        id: call.clone(),
        name: "Bash".into(),
        input: serde_json::json!({ "command": "cargo test" }),
    }));
    let output = "test result: ok\n".repeat(16);
    streamed += output.len();
    main(Input::Event(SessionEvent::ToolCompleted {
        id: call,
        output,
        is_error: false,
        result: ToolResult::Opaque,
    }));
    main(Input::Event(SessionEvent::TurnEnded {
        outcome: TurnOutcome::Completed,
        cost_usd: None,
    }));
    streamed
}

/// A long Main Thread at its retention cap: every append trims the oldest
/// history, and that trim must evict the trimmed Blocks from the front of
/// the Transcript rather than rebuild it from every retained record. A
/// rebuild is a new history generation: it re-keys the Pane's native text
/// and resyncs its whole transcript, once per streamed delta.
#[test]
fn streaming_at_the_retention_cap_evicts_history_without_rebuilding_it() {
    use ferrite_core::transcript::Body;
    const CAP: usize = 64 * 1024;
    let mut activity = Activity::new(ActivityLimits {
        content_bytes_per_subject: CAP,
        ..ActivityLimits::default()
    });
    activity.apply(ActivityInput::Connect { generation: 1 });
    let mut turn = 0;
    let mut streamed = 0;
    // Fill past the cap, then measure a whole MiB streamed beyond it.
    while streamed < 2 * CAP {
        streamed += stream_turn(&mut activity, turn);
        turn += 1;
    }
    let before = activity.view().main().revision();
    let at_cap = streamed;
    while streamed < at_cap + 1024 * 1024 {
        streamed += stream_turn(&mut activity, turn);
        turn += 1;
    }
    let rebuilds = activity.view().main().revision() - before;
    assert_eq!(
        rebuilds, 0,
        "{rebuilds} history rebuilds while streaming 1 MiB at the cap"
    );
    // The cap still bounds the projection: the oldest turns are gone and
    // the newest is whole.
    let blocks = activity.view().main().transcript().blocks();
    assert!(!blocks
        .iter()
        .any(|block| matches!(&block.body, Body::Prompt(line) if line == "prompt 0")));
    let newest = format!("prompt {}", turn - 1);
    assert!(blocks
        .iter()
        .any(|block| matches!(&block.body, Body::Prompt(line) if *line == newest)));
    let retained: usize = blocks
        .iter()
        .map(|block| block.markdown.as_ref().map_or(0, String::len))
        .sum();
    assert!(retained <= CAP, "retained {retained} bytes of answer");
}
