use ferrite_core::{
    activity::{
        Activity, ActivityEvent, ActivityInput, AgentInfo, AgentKey, ExecutionEvent, Subject,
    },
    docview::Instruments,
    progress::{Phase, PlanStep, ProgressEvent, StepStatus},
    store::{Provider, Store},
    transcript::{Body, Input, Transcript},
    workspace::WorkspaceBinding,
    SessionEvent, ToolResult, TurnOutcome,
};
use serde_json::json;
use std::time::Instant;

fn tool(id: &str) -> SessionEvent {
    SessionEvent::ToolStarted {
        id: id.into(),
        name: "Bash".into(),
        input: json!({"command":"cargo test"}),
    }
}

#[test]
fn latest_output_survives_the_disclosed_prefix_limit_and_split_lines() {
    let mut t = Transcript::default();
    t.apply(Input::Event(tool("tests")));
    for text in ["x".repeat(70_000), "\n12 tests ".into(), "passed\n".into()] {
        t.apply(Input::Event(SessionEvent::ToolOutputDelta {
            id: "tests".into(),
            text,
        }));
    }
    let tool = t
        .blocks()
        .iter()
        .find_map(|block| {
            if let Body::Tool(tool) = &block.body {
                Some(tool)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(tool.result_line.as_deref(), Some("12 tests passed"));
    assert_eq!(tool.output.as_ref().unwrap().text, "x".repeat(64 * 1024));
    assert!(tool.output.as_ref().unwrap().omitted_bytes > 0);
}

#[test]
fn unavailable_latest_test_run_clears_the_previous_verdict() {
    let mut t = Transcript::default();
    t.apply(Input::Event(tool("first")));
    t.apply(Input::Event(SessionEvent::ToolCompleted {
        id: "first".into(),
        output: "12 tests passed".into(),
        is_error: false,
        result: ToolResult::Opaque,
    }));
    assert!(Instruments::of(&t).tests.is_some());
    t.apply(Input::Event(tool("second")));
    t.apply(Input::Event(SessionEvent::TurnEnded {
        outcome: TurnOutcome::Interrupted,
        cost_usd: None,
    }));
    assert!(Instruments::of(&t).tests.is_none());
}

#[test]
fn repeated_native_wait_detail_is_compared_in_its_display_form() {
    let mut t = Transcript::default();
    let detail = format!("\u{1b}[31m Retry\n {}\u{1b}[0m", "é".repeat(600));
    for _ in 0..3 {
        t.apply(Input::Event(SessionEvent::Progress {
            event: ProgressEvent::Phase {
                phase: Phase::Retrying,
                detail: detail.clone(),
            },
        }));
    }
    assert_eq!(t.blocks().len(), 1);
}

#[test]
fn persisted_tool_ticks_preserve_nonempty_message_and_monotonic_elapsed_time() {
    let dir = TestDir::new();
    let store = Store::open(dir.path()).unwrap();
    let (id, mut writer) = store
        .create(
            Provider::Claude,
            None,
            WorkspaceBinding::Main {
                checkout: dir.path().into(),
            },
        )
        .unwrap();
    let mut live = Transcript::default();
    for (message, elapsed_ms) in [("Reading", Some(1000)), ("", Some(500)), ("", None)] {
        let event = SessionEvent::Progress {
            event: ProgressEvent::Tool {
                id: "tool".into(),
                message: message.into(),
                elapsed_ms,
            },
        };
        live.apply(Input::Event(event.clone()));
        writer.record_event(&event, None).unwrap();
    }
    writer.flush().unwrap();
    let mut restored = Transcript::default();
    for input in store.load(id).unwrap().inputs() {
        restored.apply(input);
    }
    assert_eq!(
        restored.progress().tool("tool"),
        live.progress().tool("tool")
    );
}

fn observed(activity: &mut Activity, event: SessionEvent) {
    let input = match event {
        SessionEvent::Activity(event) => ActivityInput::Observe {
            generation: 1,
            event,
            at: Instant::now(),
        },
        event => ActivityInput::Main {
            input: Input::Event(event),
            at: Instant::now(),
        },
    };
    activity.apply(input);
}

#[test]
fn scoped_native_progress_roundtrips_without_leaking_to_main_or_restarting_clocks() {
    let dir = TestDir::new();
    let store = Store::open(dir.path()).unwrap();
    let (thread, mut writer) = store
        .create(
            Provider::Codex,
            None,
            WorkspaceBinding::Main {
                checkout: dir.path().into(),
            },
        )
        .unwrap();
    let key = AgentKey::new(Provider::Codex, "root", "child");
    let mut activity = Activity::default();
    activity.apply(ActivityInput::Connect { generation: 1 });
    let mut info = AgentInfo::new(key.clone());
    info.parent = Some(Subject::Main);
    let discovered = SessionEvent::Activity(ActivityEvent::Discovered(info));
    writer.record_event(&discovered, None).unwrap();
    observed(&mut activity, discovered);
    let events = vec![
        ExecutionEvent::ReasoningSummaryPart {
            item_id: "heading".into(),
            summary_index: 0,
            text: "**Checking child".into(),
            snapshot: false,
        },
        ExecutionEvent::ReasoningSummaryPart {
            item_id: "heading".into(),
            summary_index: 0,
            text: "**Checking child scope**".into(),
            snapshot: true,
        },
        ExecutionEvent::ContentBoundary,
        ExecutionEvent::from_session(&tool("tool")).unwrap(),
        ExecutionEvent::ToolOutputDelta {
            id: "tool".into(),
            text: "12 tests passed\n".into(),
        },
        ExecutionEvent::Progress {
            event: ProgressEvent::Tool {
                id: "tool".into(),
                message: "Reading".into(),
                elapsed_ms: Some(1000),
            },
        },
        ExecutionEvent::Progress {
            event: ProgressEvent::Plan {
                steps: vec![PlanStep {
                    text: "Check child".into(),
                    status: StepStatus::InProgress,
                }],
                explanation: "Native plan".into(),
            },
        },
        // This snapshot rebuild used to erase native progress added between text deltas.
        ExecutionEvent::TextDelta {
            text: "Partial".into(),
        },
        ExecutionEvent::TextSnapshot {
            text: "Partial answer".into(),
        },
    ];
    for event in events {
        let event = SessionEvent::Activity(ActivityEvent::Content {
            key: key.clone(),
            id: Some("message".into()),
            event,
        });
        writer.record_event(&event, None).unwrap();
        observed(&mut activity, event);
    }
    let child = activity
        .view()
        .subject(&Subject::Subagent(key.clone()))
        .unwrap();
    assert_eq!(
        child.transcript().progress().caption().as_deref(),
        Some("Checking child scope")
    );
    assert_eq!(child.transcript().current_task(), Some("Check child"));
    assert_eq!(
        child
            .transcript()
            .progress()
            .tool("tool")
            .unwrap()
            .elapsed_ms,
        Some(1000)
    );
    assert!(activity
        .view()
        .main()
        .transcript()
        .progress()
        .caption()
        .is_none());
    let event = SessionEvent::Activity(ActivityEvent::Content {
        key: key.clone(),
        id: Some("turn".into()),
        event: ExecutionEvent::TurnEnded {
            outcome: TurnOutcome::Interrupted,
            cost_usd: None,
        },
    });
    writer.record_event(&event, None).unwrap();
    observed(&mut activity, event);
    writer.flush().unwrap();
    let mut restored = Activity::default();
    for input in store.load(thread).unwrap().activity_inputs() {
        restored.apply(input);
    }
    let child = restored.view().subject(&Subject::Subagent(key)).unwrap();
    assert!(child.transcript().progress().caption().is_none());
    assert!(child.transcript().turn_elapsed().is_none());
    assert_eq!(child.transcript().current_task(), Some("Check child"));
    assert_eq!(Instruments::of(child.transcript()).running, 0);
    assert_eq!(child.transcript().blocks().iter().filter(|block| matches!(&block.body, Body::Thinking(text) if text == "**Checking child scope**")).count(), 1);
    assert!(restored.view().main().transcript().blocks().is_empty());
}

struct TestDir(std::path::PathBuf);
impl TestDir {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "ferrite-progress-lifecycle-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_completed_reasoning_snapshot_updates_history_without_restarting_work() {
    let mut t = Transcript::default();
    t.apply(Input::Event(SessionEvent::TurnEnded {
        outcome: TurnOutcome::Completed,
        cost_usd: None,
    }));
    t.apply(Input::Event(SessionEvent::ReasoningSummaryPart {
        item_id: "late".into(),
        summary_index: 0,
        text: "**Checking files**".into(),
        snapshot: true,
    }));
    assert!(t.progress().caption().is_none());
    assert!(t.turn_elapsed().is_none());
    assert!(t.turn_completed());
    assert!(t
        .blocks()
        .iter()
        .any(|block| matches!(&block.body, Body::Thinking(text) if text == "**Checking files**")));
}

#[test]
fn progress_before_prose_starts_the_shared_view_and_closed_tools_cannot_restart_it() {
    let mut activity = Activity::default();
    activity.apply(ActivityInput::Connect { generation: 1 });
    let key = AgentKey::new(Provider::Codex, "root", "child");
    observed(
        &mut activity,
        SessionEvent::Activity(ActivityEvent::Discovered(AgentInfo::new(key.clone()))),
    );
    for event in [
        ExecutionEvent::Progress {
            event: ProgressEvent::Phase {
                phase: Phase::Compacting,
                detail: String::new(),
            },
        },
        ExecutionEvent::from_session(&tool("cmd")).unwrap(),
    ] {
        observed(
            &mut activity,
            SessionEvent::Activity(ActivityEvent::Content {
                key: key.clone(),
                id: None,
                event,
            }),
        );
    }
    let subject = Subject::Subagent(key.clone());
    assert!(activity
        .view()
        .subject(&subject)
        .unwrap()
        .transcript()
        .turn_elapsed()
        .is_some());
    observed(
        &mut activity,
        SessionEvent::Activity(ActivityEvent::Content {
            key: key.clone(),
            id: None,
            event: ExecutionEvent::TurnEnded {
                outcome: TurnOutcome::Interrupted,
                cost_usd: None,
            },
        }),
    );
    observed(
        &mut activity,
        SessionEvent::Activity(ActivityEvent::Content {
            key,
            id: None,
            event: ExecutionEvent::ToolOutputDelta {
                id: "cmd".into(),
                text: "late".into(),
            },
        }),
    );
    let child = activity.view().subject(&subject).unwrap();
    assert!(!child.busy());
    assert!(child.transcript().turn_elapsed().is_none());
    assert!(child.transcript().progress().caption().is_none());
}
