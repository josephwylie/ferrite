//! A bounded revive shows what a full replay shows (ADR 0009): the same
//! Main transcript and runtime, the same children (those known from before
//! the replay base evicted, their content left on disk), the same resume
//! target, recall and provider lock — from a read that stops at the base.

use super::compat_tests::{render_activity, scratch};
use super::*;
use crate::activity::{
    Activity, ActivityEvent, ActivityInput, ActivityLimits, AgentInfo, AgentKey, AgentStatus,
    ExecutionEvent, Subject,
};
use crate::progress::{BackgroundTask, Phase, PlanStep, PlanTask, ProgressEvent, StepStatus, TaskStatus};

/// A small world: a window of 4 KB, so a few hundred KB of log is long.
fn limits() -> ActivityLimits {
    ActivityLimits {
        max_children: 3,
        blocks_per_subject: 40,
        content_bytes_per_subject: 4096,
        dedup_ids_per_subject: 256,
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn text(&mut self, max: u64) -> String {
        let words = ["log", "mark", "base", "turn", "carry", "retain", "café", "naïve", "\n", "**bold**"];
        (0..1 + self.below(max / 4))
            .map(|_| words[self.below(words.len() as u64) as usize])
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A seeded long Thread written through the store: turns of text and
/// tools, usage cards, progress, model and session changes, child agents
/// with aliases, queued prompts, resets, parks and a provider switch.
fn write_thread(store: &Store, seed: u64, turns: usize) -> ThreadId {
    let binding = WorkspaceBinding::Main {
        checkout: "/fixture/repo".into(),
    };
    let (id, mut writer) = store.create(Provider::Claude, None, binding).unwrap();
    let mut rng = Rng(seed);
    let mut children: Vec<AgentKey> = Vec::new();
    let mut session = 0;
    let event = |writer: &mut ThreadWriter, event: SessionEvent| {
        let duration = matches!(event, SessionEvent::ToolCompleted { .. })
            .then(|| std::time::Duration::from_millis(1234));
        writer.record_event(&event, duration).unwrap();
    };
    event(
        &mut writer,
        SessionEvent::Init {
            session_id: "s-0".into(),
            model: "m-0".into(),
        },
    );
    let switch_at = (seed % 3 == 0).then(|| turns / 2);
    for turn in 0..turns {
        writer
            .record_prompt(&format!("turn {turn} {}", rng.text(40)))
            .unwrap();
        for step in 0..1 + rng.below(6) {
            let tool = format!("t-{turn}-{step}");
            match rng.below(16) {
                0..=3 => event(&mut writer, SessionEvent::TextDelta { text: rng.text(400) }),
                4 => event(&mut writer, SessionEvent::ThinkingDelta { text: rng.text(200) }),
                5 => {
                    event(
                        &mut writer,
                        SessionEvent::ToolStarted {
                            id: tool.clone(),
                            name: "Bash".into(),
                            input: serde_json::json!({ "command": rng.text(20) }),
                        },
                    );
                    event(
                        &mut writer,
                        SessionEvent::ToolOutputDelta {
                            id: tool.clone(),
                            text: rng.text(100),
                        },
                    );
                    event(
                        &mut writer,
                        SessionEvent::ToolCompleted {
                            id: tool,
                            output: rng.text(100),
                            is_error: rng.below(5) == 0,
                            result: crate::ToolResult::Opaque,
                        },
                    );
                }
                6 => event(
                    &mut writer,
                    match rng.below(4) {
                        0 => SessionEvent::TokenUsage {
                            total_tokens: rng.next(),
                            input_tokens: rng.below(1000),
                            cached_input_tokens: rng.below(1000),
                            output_tokens: rng.below(1000),
                            reasoning_output_tokens: rng.below(100),
                            context_window: Some(200_000),
                        },
                        1 => SessionEvent::ContextUsage {
                            total_tokens: rng.below(100_000),
                            context_window: Some(200_000),
                        },
                        2 => SessionEvent::UsageDetails {
                            details: crate::UsageDetails {
                                scope: crate::UsageScope::Turn,
                                input_tokens: rng.below(1000),
                                cached_input_tokens: 0,
                                output_tokens: rng.below(1000),
                                reasoning_output_tokens: 0,
                            },
                        },
                        _ => SessionEvent::ContextDetails {
                            details: crate::ContextDetails {
                                usable_window: Some(rng.below(100_000)),
                                auto_compact_threshold: None,
                                is_auto_compact_enabled: Some(true),
                                categories: vec![],
                            },
                        },
                    },
                ),
                7 => event(
                    &mut writer,
                    SessionEvent::Progress {
                        event: match rng.below(6) {
                            0 => ProgressEvent::Plan {
                                steps: vec![PlanStep {
                                    text: rng.text(20),
                                    status: StepStatus::InProgress,
                                }],
                                explanation: String::new(),
                            },
                            1 => ProgressEvent::Task {
                                id: format!("task-{}", rng.below(4)),
                                subject: rng.text(12),
                                status: Some(StepStatus::Completed),
                                deleted: false,
                            },
                            2 => ProgressEvent::TasksSnapshot {
                                tasks: vec![PlanTask {
                                    id: "task-0".into(),
                                    text: rng.text(12),
                                    status: StepStatus::Pending,
                                }],
                            },
                            3 => ProgressEvent::Background {
                                id: format!("bg-{}", rng.below(3)),
                                label: rng.text(12),
                                status: TaskStatus::Working,
                                detail: String::new(),
                            },
                            4 => ProgressEvent::BackgroundSnapshot {
                                tasks: vec![BackgroundTask {
                                    id: "bg-0".into(),
                                    label: rng.text(12),
                                    status: TaskStatus::Completed,
                                    detail: String::new(),
                                }],
                            },
                            _ => ProgressEvent::Phase {
                                phase: Phase::Thinking,
                                detail: String::new(),
                            },
                        },
                    },
                ),
                8 | 9 => {
                    let activity = if children.len() < 6 && rng.below(3) == 0 {
                        let key = AgentKey::new(Provider::Claude, "root", &format!("agent-{}", children.len()));
                        children.push(key.clone());
                        let mut info = AgentInfo::new(key);
                        info.parent = Some(Subject::Main);
                        info.name = Some(rng.text(8));
                        ActivityEvent::Discovered(info)
                    } else if let Some(key) = children.get(rng.below(children.len().max(1) as u64) as usize) {
                        match rng.below(3) {
                            0 => ActivityEvent::Status {
                                key: key.clone(),
                                state: [AgentStatus::Working, AgentStatus::Idle][rng.below(2) as usize],
                            },
                            1 => ActivityEvent::Alias {
                                from: AgentKey::new(Provider::Claude, "root", &format!("toolu-{turn}-{step}")),
                                to: key.clone(),
                            },
                            _ => ActivityEvent::Content {
                                key: key.clone(),
                                id: Some(format!("c-{turn}-{step}")),
                                event: ExecutionEvent::Text { text: rng.text(200) },
                            },
                        }
                    } else {
                        continue;
                    };
                    event(&mut writer, SessionEvent::Activity(activity));
                }
                10 => event(&mut writer, SessionEvent::ModelChanged { model: format!("m-{}", rng.below(5)) }),
                11 => writer.record_prompt(&format!("queued {turn}")).unwrap(),
                12 => event(&mut writer, SessionEvent::RunState { state: crate::RunState::Running }),
                13 => event(
                    &mut writer,
                    SessionEvent::TurnDiff {
                        turn_id: format!("turn-{turn}"),
                        diff: rng.text(60),
                    },
                ),
                _ => event(&mut writer, SessionEvent::TextDelta { text: rng.text(100) }),
            }
        }
        event(
            &mut writer,
            SessionEvent::TurnEnded {
                outcome: [crate::TurnOutcome::Completed, crate::TurnOutcome::Interrupted]
                    [rng.below(2) as usize]
                    .clone(),
                cost_usd: Some(0.01),
            },
        );
        writer.note_subagents(children.len());
        match rng.below(30) {
            0 => event(
                &mut writer,
                SessionEvent::ConversationReset {
                    session_id: format!("reset-{turn}"),
                },
            ),
            1 | 2 => {
                session += 1;
                event(
                    &mut writer,
                    SessionEvent::Init {
                        session_id: format!("s-{session}"),
                        model: format!("m-{session}"),
                    },
                );
            }
            3..=6 => writer.flush_fully().unwrap(),
            _ => {}
        }
        if switch_at == Some(turn) {
            store
                .hand_over(id, Provider::Codex, Some("gpt".into()), &mut writer)
                .unwrap();
        }
    }
    writer.flush_fully().unwrap();
    id
}

fn replayed(inputs: Vec<ActivityInput>) -> Activity {
    let mut activity = Activity::new(limits());
    activity.apply(ActivityInput::Disconnect);
    for input in inputs {
        activity.apply(input);
    }
    activity
}

/// What an operator sees of an Activity, children known from before the
/// base reduced to what their eviction keeps: identity and status.
fn seen(activity: &Activity, evicted: &[&str]) -> String {
    let mut out = String::new();
    // A tool's time shows only on its block: timings of blocks long since
    // trimmed are invisible, and only a full replay restores them.
    let visible: Vec<String> = activity
        .view()
        .main()
        .transcript()
        .blocks()
        .iter()
        .filter_map(|block| match &block.body {
            crate::transcript::Body::Tool(tool) => Some(format!("{}=", tool.call)),
            _ => None,
        })
        .collect();
    for line in render_activity(activity).lines() {
        if line.starts_with("child ") {
            break;
        }
        if line.starts_with("timings=") {
            let mut timings: Vec<String> = activity
                .view()
                .main()
                .tool_timings()
                .iter()
                .map(|(id, timing)| format!("{id}={timing:?}"))
                .filter(|timing| visible.iter().any(|call| timing.starts_with(call.as_str())))
                .collect();
            timings.sort();
            out.push_str(&format!("visible timings={timings:?}\n"));
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    for child in activity.view().children() {
        let key = child.key().as_str();
        out.push_str(&format!("child {key} {:?} status={:?}\n", child.info(), child.status()));
        if !evicted.contains(&key) {
            out.push_str(&format!("  coverage={:?} retained={}\n", child.coverage(), child.retained()));
            for block in child.transcript().blocks() {
                out.push_str(&format!("    {:?}\n", block.body));
            }
        }
    }
    out
}

/// The property: for seeded long Threads, the bounded revive and a full
/// replay show the same Thread, and the bounded one replays far less.
#[test]
fn a_bounded_revive_shows_what_a_full_replay_shows() {
    let mut bounded_reads = 0;
    for seed in 0..24u64 {
        let dir = scratch(&format!("revive-equivalence-{seed}"));
        let store = Store::open(&dir).unwrap().scaled(16 * 1024, limits());
        let id = write_thread(&store, seed, 300 + (seed as usize % 5) * 100);

        let full = store.load(id).unwrap();
        let revival = store.revive(id).unwrap();
        if revival.snapshot().records.len() < full.records.len() / 2 {
            bounded_reads += 1;
        }

        let evicted: Vec<&str> = revival.evict.iter().map(String::as_str).collect();
        let expected = seen(&replayed(full.activity_inputs()), &evicted);
        let actual = seen(&replayed(revival.activity_inputs()), &evicted);
        if expected != actual {
            let diverged = expected
                .lines()
                .zip(actual.lines())
                .position(|(a, b)| a != b);
            panic!(
                "seed {seed}: the bounded revive shows a different Thread at line {diverged:?}\n\
                 --- full\n{expected}\n--- bounded\n{actual}"
            );
        }
        let snapshot = revival.snapshot();
        assert_eq!(snapshot.resume_target(), full.resume_target(), "seed {seed}");
        assert_eq!(snapshot.prompt_texts(), full.prompt_texts(), "seed {seed}");
        assert_eq!(
            revival.prompted(),
            full.records.iter().any(|record| matches!(record, Record::Prompt { .. })),
            "seed {seed}"
        );
        assert_eq!(snapshot.provider(), full.provider(), "seed {seed}");
        assert_eq!(snapshot.model(), full.model(), "seed {seed}");
        assert_eq!(
            revival.owed_handover().map(|h| (h.from, h.exchanges.clone())),
            full.last_handover()
                .filter(|h| !h.delivered)
                .map(|h| (h.from, h.exchanges)),
            "seed {seed}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
    assert!(
        bounded_reads >= 20,
        "only {bounded_reads} of 24 revives replayed less than half the log"
    );
}

/// A long Thread shaped like real ones — big turns, few prompts — revives
/// from its tail: a few windows' worth of the log, not all of it.
#[test]
fn a_long_thread_revives_from_its_tail() {
    let dir = scratch("revive-tail");
    let store = Store::open(&dir).unwrap().scaled(64 * 1024, limits());
    let binding = WorkspaceBinding::Main {
        checkout: "/fixture/repo".into(),
    };
    let (id, mut writer) = store.create(Provider::Claude, None, binding).unwrap();
    for turn in 0..400 {
        writer.record_prompt(&format!("turn {turn}")).unwrap();
        writer
            .record_event(&SessionEvent::TextDelta { text: "w".repeat(10_000) }, None)
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
    writer.flush_fully().unwrap();
    drop(writer);
    let size = fs::metadata(dir.join(id.to_string()).join("log.jsonl"))
        .unwrap()
        .len();
    let read = store.bytes_read();
    let revival = store.revive(id).unwrap();
    let read = store.bytes_read() - read;
    assert!(size > 4_000_000);
    assert!(read < 512 * 1024, "the revive read {read} of {size} bytes");
    assert_eq!(revival.snapshot().prompt_texts().len(), 400, "recall is whole");
    let _ = fs::remove_dir_all(&dir);
}

/// The property on a real log: set `LAB_LOG` to a log to copy (an APFS
/// clone). Its first revive upgrades it, and the upgrade's mark names the
/// base the bounded read starts from.
#[test]
#[ignore = "needs LAB_LOG, a copy of a real Thread log; run in release"]
fn a_real_log_revives_bounded_and_identical() {
    use std::time::Instant;
    let Ok(source) = std::env::var("LAB_LOG") else {
        return;
    };
    let dir = scratch("lab-revive");
    fs::create_dir_all(dir.join("3")).unwrap();
    fs::copy(&source, dir.join("3").join("log.jsonl")).unwrap();
    let store = Store::open(&dir).unwrap();
    let id = ThreadId::new(3);
    let ms = |since: Instant| since.elapsed().as_secs_f64() * 1000.0;

    let started = Instant::now();
    drop(store.revive(id).unwrap());
    eprintln!("LAB first revive (upgrade + bounded read) ms={:.1}", ms(started));
    let started = Instant::now();
    let read = store.bytes_read();
    let revival = store.revive(id).unwrap();
    eprintln!(
        "LAB revive read ms={:.1} bytes={} records={}",
        ms(started),
        store.bytes_read() - read,
        revival.snapshot().records.len()
    );
    let started = Instant::now();
    let bounded = {
        let mut activity = Activity::default();
        activity.apply(ActivityInput::Disconnect);
        for input in revival.activity_inputs() {
            activity.apply(input);
        }
        activity
    };
    eprintln!("LAB bounded replay ms={:.1}", ms(started));
    let started = Instant::now();
    let full = {
        let mut activity = Activity::default();
        activity.apply(ActivityInput::Disconnect);
        for input in store.load(id).unwrap().activity_inputs() {
            activity.apply(input);
        }
        activity
    };
    eprintln!("LAB full load + replay ms={:.1}", ms(started));
    let evicted: Vec<&str> = revival.evict.iter().map(String::as_str).collect();
    assert_eq!(seen(&bounded, &evicted), seen(&full, &evicted));
    let _ = fs::remove_dir_all(&dir);
}
