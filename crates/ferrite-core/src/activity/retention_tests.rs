//! Trimming retained history at its limits evicts the oldest Blocks in
//! place. Whatever the stream and the limits, what remains must read as a
//! rebuild from the retained records reads: the same Blocks, in the same
//! Markdown runs, streaming on the same way.
//!
//! The streams leave out what a rebuild itself cannot reproduce from the
//! records, so no eviction could match it: progress phases (whether one
//! leaves a notice depends on the phase before it, which a rebuild keeps
//! from the live view), and a revival's notice, which is not a record.

use super::*;
use crate::transcript::{Block, Body};
use crate::FileEdit;

/// Deterministic xorshift, so a failing seed replays exactly. It also
/// keeps the calls in flight as a provider makes them: each call has its
/// own id, its start may be repeated by the snapshot that follows it, it
/// streams output and edits while it runs, and it settles once.
struct Rng(u64, u64, Vec<String>, Option<String>);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len() as u64) as usize]
    }
}

/// Prose in the pieces a provider streams it in: words, paragraph and
/// line breaks, fences, bullets and headings, split anywhere.
fn prose(rng: &mut Rng) -> String {
    const PIECES: &[&str] = &[
        "word ",
        "more words ",
        "a sentence. ",
        "\n\n",
        "\n",
        "```rust\n",
        "fn x() {}\n",
        "```\n",
        "- bullet\n",
        "# Heading\n",
        "**bold** ",
        "`code` ",
        "界",
        "par",
        "tial",
    ];
    (0..1 + rng.below(6)).map(|_| rng.pick(PIECES)).collect()
}

/// A new call; now and then its snapshot will repeat the start next.
fn start(rng: &mut Rng) -> String {
    rng.1 += 1;
    let id = format!("call-{}", rng.1);
    rng.2.push(id.clone());
    if rng.chance(20) {
        rng.3 = Some(id.clone());
    }
    id
}

/// A running call, for its output or edits; a fresh one if none runs.
fn call(rng: &mut Rng) -> String {
    if rng.2.is_empty() {
        return start(rng);
    }
    let at = rng.below(rng.2.len() as u64) as usize;
    rng.2[at].clone()
}

/// A running call settling, never to be heard from again.
fn settle(rng: &mut Rng) -> String {
    let id = call(rng);
    rng.2.retain(|open| *open != id);
    id
}

fn main_input(rng: &mut Rng) -> Input {
    if let Some(id) = rng.3.take() {
        return Input::Event(SessionEvent::ToolStarted {
            id,
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo test", "file_path": "src/a.rs" }),
        });
    }
    match rng.below(18) {
        0..=4 => Input::Event(SessionEvent::TextDelta { text: prose(rng) }),
        5 => Input::Event(SessionEvent::ThinkingDelta {
            text: if rng.chance(10) {
                String::new()
            } else {
                prose(rng)
            },
        }),
        6 => Input::Event(SessionEvent::ReasoningSummaryDelta {
            text: prose(rng),
            summary_index: rng.below(3),
        }),
        7 => Input::Event(SessionEvent::ReasoningSummaryPart {
            item_id: format!("item-{}", rng.below(3)),
            summary_index: rng.below(2),
            text: prose(rng),
            snapshot: rng.chance(30),
        }),
        8 => Input::Event(SessionEvent::ToolStarted {
            id: start(rng),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo test", "file_path": "src/a.rs" }),
        }),
        9 => Input::Event(SessionEvent::ToolOutputDelta {
            id: call(rng),
            text: prose(rng),
        }),
        10 => Input::Event(SessionEvent::ToolCompleted {
            id: settle(rng),
            output: prose(rng),
            is_error: rng.chance(20),
            result: ToolResult::Opaque,
        }),
        11 => Input::Event(SessionEvent::FileChanges {
            id: call(rng),
            edits: vec![FileEdit {
                path: "src/a.rs".into(),
                hunks: Vec::new(),
            }],
        }),
        12 => Input::Event(SessionEvent::TurnEnded {
            // The turn's end retires whatever still runs.
            outcome: {
                rng.2.clear();
                match rng.below(3) {
                    0 => TurnOutcome::Completed,
                    1 => TurnOutcome::Interrupted,
                    _ => TurnOutcome::Error("failed".into()),
                }
            },
            cost_usd: None,
        }),
        13 => Input::CompletionObservation {
            elapsed_ms: rng.below(10_000),
            completed_at: "7:31 pm".into(),
            input_tokens: Some(rng.below(1000)),
            output_tokens: None,
        },
        14 => {
            if rng.chance(50) {
                Input::Prompt(prose(rng))
            } else {
                Input::PromptObservation {
                    sent_at: format!("7:{:02} pm", rng.below(60)),
                }
            }
        }
        15 => match rng.below(3) {
            0 => Input::Notice(prose(rng)),
            1 => Input::Answered {
                allowed: rng.chance(50),
                tool_name: "Bash".into(),
            },
            _ => Input::Event(SessionEvent::ContentBoundary),
        },
        _ => Input::Event(SessionEvent::TokenUsage {
            total_tokens: rng.below(1000),
            input_tokens: rng.below(1000),
            cached_input_tokens: 0,
            output_tokens: rng.below(1000),
            reasoning_output_tokens: 0,
            context_window: None,
        }),
    }
}

/// A child's content: identified deliveries that stream, complete,
/// snapshot and retract — the record shapes Main's own events never take.
fn child_event(rng: &mut Rng) -> (Option<String>, ExecutionEvent) {
    let id = Some(format!("item-{}", rng.below(4)));
    let event = match rng.below(8) {
        0..=2 => ExecutionEvent::TextDelta { text: prose(rng) },
        3 => ExecutionEvent::Text { text: prose(rng) },
        4 => ExecutionEvent::TextSnapshot { text: prose(rng) },
        5 => ExecutionEvent::ThinkingSnapshot { text: prose(rng) },
        6 => ExecutionEvent::ToolStarted {
            id: {
                // Only Main's own stream repeats a start here.
                let id = start(rng);
                rng.3 = None;
                id
            },
            name: "Bash".into(),
            input: serde_json::Value::Null,
        },
        _ => {
            if rng.chance(15) {
                ExecutionEvent::Retract {
                    ids: vec![format!("item-{}", rng.below(4))],
                }
            } else {
                ExecutionEvent::ToolCompleted {
                    id: settle(rng),
                    output: prose(rng),
                    is_error: false,
                    result: ToolResult::Opaque,
                }
            }
        }
    };
    (id, event)
}

/// A Block as a reader sees it, with each Markdown run named by the index
/// of its first member: ids are minted afresh by a rebuild, runs are not
/// an identity a reader can see, only which sections join.
type Shape = (Body, Option<String>, Option<String>, Option<usize>);

fn shape(blocks: &[Block]) -> Vec<Shape> {
    blocks
        .iter()
        .map(|block| {
            let run = block.markdown_run.map(|run| {
                blocks
                    .iter()
                    .position(|other| other.markdown_run == Some(run))
                    .expect("its own run")
            });
            (
                block.body.clone(),
                block.markdown.clone(),
                block.sent_at.clone(),
                run,
            )
        })
        .collect()
}

/// What a rebuild would show: every retained record replayed into a fresh
/// Transcript, the live runtime restored over it.
fn rebuilt(state: &SubjectState, limits: ActivityLimits) -> Transcript {
    let mut transcript =
        Transcript::with_capacity(Arc::new(Lexer::new().0), limits.blocks_per_subject);
    for record in &state.records {
        transcript.apply(record.input.clone());
    }
    transcript.restore_runtime(state.transcript.runtime());
    transcript
}

fn check(state: &SubjectState, limits: ActivityLimits, seed: u64, step: usize, subject: &str) {
    let rebuilt = rebuilt(state, limits);
    assert_eq!(
        shape(state.transcript.blocks()),
        shape(rebuilt.blocks()),
        "{subject} diverged from its rebuild: seed {seed}, step {step}"
    );
}

#[test]
fn trimmed_history_reads_as_a_rebuild_from_the_retained_records() {
    let mut in_place = 0;
    let mut rebuilds = 0;
    for seed in 1..=100u64 {
        let mut rng = Rng(
            seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            0,
            Vec::new(),
            None,
        );
        let limits = ActivityLimits {
            blocks_per_subject: 1 + rng.below(40) as usize,
            content_bytes_per_subject: 40 + rng.below(3000) as usize,
            ..ActivityLimits::default()
        };
        let mut activity = Activity::new(limits);
        activity.apply(ActivityInput::Connect { generation: 1 });
        let key = AgentKey::new(Provider::Codex, "root", "child");
        let mut info = AgentInfo::new(key.clone());
        info.parent = Some(Subject::Main);
        activity.apply(ActivityInput::Observe {
            generation: 1,
            event: ActivityEvent::Discovered(info),
            at: crate::clock::instant(),
        });
        for step in 0..240 {
            let at = crate::clock::instant();
            let main = rng.3.is_some() || rng.chance(80);
            let generation = if main {
                activity.main.revision
            } else {
                activity.agents[&key].state.revision
            };
            let update = if main {
                activity.apply(ActivityInput::Main {
                    input: main_input(&mut rng),
                    at,
                })
            } else {
                let (id, event) = child_event(&mut rng);
                activity.apply(ActivityInput::Observe {
                    generation: 1,
                    event: ActivityEvent::Content {
                        key: key.clone(),
                        id,
                        event,
                    },
                    at,
                })
            };
            let state = if main {
                &activity.main
            } else {
                &activity.agents[&key].state
            };
            if state.revision != generation {
                rebuilds += 1;
            } else if update
                .blocks
                .iter()
                .any(|(_, blocks)| !blocks.evicted.is_empty())
            {
                in_place += 1;
            }
            check(&activity.main, limits, seed, step, "Main");
            check(&activity.agents[&key].state, limits, seed, step, "child");
        }
    }
    assert!(
        in_place > rebuilds,
        "trimming should mostly evict in place: {in_place} in place, {rebuilds} rebuilt"
    );
}

#[test]
fn a_record_streaming_into_a_trimmed_block_is_rebuilt_not_lost() {
    // The answer's paragraph opens in one record, another call's output
    // splits the stream, and the paragraph goes on in the next record.
    // Trimming the first record leaves only the second's words to show.
    let limits = ActivityLimits {
        content_bytes_per_subject: 300,
        ..ActivityLimits::default()
    };
    let mut activity = Activity::new(limits);
    let at = crate::clock::instant();
    let mut main = |input: Input| {
        activity.apply(ActivityInput::Main { input, at });
    };
    main(Input::Event(SessionEvent::TextDelta {
        text: "a".repeat(200),
    }));
    main(Input::Event(SessionEvent::ToolOutputDelta {
        id: "background".into(),
        text: "built".into(),
    }));
    main(Input::Event(SessionEvent::TextDelta {
        text: "b".repeat(150),
    }));
    let state = &activity.main;
    assert_eq!(state.revision, 1, "the split paragraph needs its replay");
    let Body::Paragraph { spans } = &state.transcript.blocks()[0].body else {
        panic!("one paragraph remains");
    };
    assert_eq!(spans[0].text, "b".repeat(150));
    check(state, limits, 0, 0, "Main");
}
