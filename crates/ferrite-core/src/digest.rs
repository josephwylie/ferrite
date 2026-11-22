//! What every window reads of a Thread it does not draw (ADR 0032). The
//! Server folds it from the Thread's Activity and sends it to every window;
//! only a Thread drawn at Transcript level is mirrored whole.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::activity::{ActivityView, Subject};
use crate::docview::{FileChange, Instruments, Tests};
use crate::progress::ROW_CHARS;
use crate::protocol::{PendingEntry, WireHandle, WireOutcome, WireStatus, WireSubject};
use crate::transcript::{Block, Body, Span, Status, Transcript};
use crate::{Decision, ThreadId, TurnOutcome};

/// A turn or an agent failed.
pub const FAILED: &str = "failed";

/// One Thread as every window reads it away from its Transcript: the nav
/// row, the Wall card, the bell and its toasts, Needs-you, the board.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Digest {
    /// Main's status, as its transcript last saw it.
    #[serde(default)]
    pub status: WireStatus,
    /// How Main's last turn ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<WireOutcome>,
    /// When Main's turn started, on the Server's clock (`ms_since` its boot).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_started_ms: Option<u64>,
    /// Main is mid-turn.
    #[serde(default)]
    pub working: bool,
    /// The operator's prompt is out and unanswered.
    #[serde(default)]
    pub operator_turn: bool,
    /// Subagents working.
    #[serde(default)]
    pub working_descendants: usize,
    /// Work runs behind Main: its Subagents, or its background tasks.
    #[serde(default)]
    pub background_work: bool,
    /// The Thread is at work as the Server keeps its Session for: a turn,
    /// queued or unanswered prompts, Decisions, Subagents, background tasks
    /// or work it will start itself (ADR 0032). A window holds its
    /// transcript as long. Only the Server says so.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub at_work: bool,
    /// Every pending Decision, in the order they are answered.
    #[serde(default)]
    pub pending: Vec<PendingEntry>,
    /// Main's Wall card.
    #[serde(default)]
    pub wall: WallCard,
    /// The card of the Subagent a window shows in its place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shown: Option<ShownWall>,
    /// What a Thread with no title is called: its first prompt, shortened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_title: Option<String>,
    /// Prompts Main holds.
    #[serde(default)]
    pub turns: usize,
    #[serde(default)]
    pub subagents: usize,
    /// Files edited anywhere in the Thread, its Subagents' included.
    #[serde(default)]
    pub changed_files: Vec<FileChange>,
    /// How Main's latest turn finished, in the bell's words.
    #[serde(default)]
    pub finish: Finish,
    /// The PRs its agent watches and whether a server wake waits for it
    /// (ADR 0046): the details card's eye reads it. A Server older than it
    /// sends none.
    #[serde(default, skip_serializing_if = "crate::pr_watch::Watching::is_empty")]
    pub watching: crate::pr_watch::Watching,
}

/// Main's turn as the state word reads it: what Main does, how its last
/// turn ended, and since when the running one works.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Turn {
    pub status: Status,
    pub outcome: Option<TurnOutcome>,
    pub started: Option<Instant>,
}

impl Turn {
    pub fn of(transcript: &Transcript) -> Self {
        Self {
            status: transcript.status(),
            outcome: transcript.turn_outcome().cloned(),
            started: transcript.turn_started(),
        }
    }

    /// At rest after a successful turn (`Transcript::turn_completed`).
    pub fn completed(&self) -> bool {
        self.status == Status::Idle && self.outcome == Some(TurnOutcome::Completed)
    }

    /// How long the running (or last) turn has worked.
    pub fn elapsed(&self) -> Option<Duration> {
        self.started
            .map(|started| crate::clock::instant().saturating_duration_since(started))
    }
}

/// A Subagent's Wall card, for a tile showing it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShownWall {
    pub subject: WireSubject,
    pub wall: WallCard,
}

/// What a finished turn left for the bell: the tests it left failing, else
/// its answer when that is one row of words after its last call.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Finish {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failing_tests: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worked_ms: Option<u64>,
}

impl Digest {
    /// Fold one Thread's digest. `shown` is the Subject the windows show
    /// for it; `boot` is the Server's clock zero.
    pub fn of(
        thread: ThreadId,
        activity: ActivityView<'_>,
        shown: Option<&Subject>,
        boot: Instant,
    ) -> Self {
        let main = activity.main();
        let transcript = main.transcript();
        let pending = activity.decisions();
        let working_descendants = activity.working_descendants();
        Self {
            status: transcript.status().into(),
            outcome: transcript.turn_outcome().map(Into::into),
            turn_started_ms: transcript
                .turn_started()
                .map(|at| crate::protocol::ms_since(at, boot)),
            working: main.busy(),
            operator_turn: activity.main_operator_turn(),
            working_descendants,
            background_work: activity.background_work(),
            // The Server's verdict, from its Thread (`Engine::publish_digests`).
            at_work: false,
            pending: pending
                .iter()
                .map(|pending| PendingEntry {
                    thread,
                    subject: pending.subject.as_ref().map(Into::into),
                    handle: (&pending.handle).into(),
                    decision: (&pending.decision).into(),
                })
                .collect(),
            wall: WallCard::of(transcript, pending.first().map(|pending| &pending.decision)),
            // A Subagent the Thread no longer holds has an empty card: the
            // digest still says which one it is, so it is not refolded for it.
            shown: shown
                .filter(|subject| **subject != Subject::Main)
                .map(|subject| ShownWall {
                    subject: subject.into(),
                    wall: activity
                        .subject(subject)
                        .map(|view| WallCard::of(view.transcript(), None))
                        .unwrap_or_default(),
                }),
            prompt_title: prompt_title(transcript),
            turns: transcript
                .blocks()
                .iter()
                .filter(|block| matches!(block.body, Body::Prompt(_)))
                .count(),
            subagents: activity.children().len(),
            changed_files: changed_files(activity),
            finish: Finish::of(transcript),
            watching: activity.watching().clone(),
        }
    }

    /// Whether this digest carries the card of the Subject the windows
    /// show (none for Main).
    pub fn shows(&self, shown: Option<&Subject>) -> bool {
        let shown = shown.filter(|subject| **subject != Subject::Main);
        self.shown.as_ref().map(|shown| &shown.subject) == shown.map(WireSubject::from).as_ref()
    }

    /// Whether `activity` moved past this digest in a way no window should
    /// wait a refold for: Main started or ended a turn or changed phase, a
    /// Decision came or went, a Subagent came or work behind Main started
    /// or stopped, a PR watch moved, or the windows show another Subject.
    pub fn outdated_by(&self, activity: ActivityView<'_>, shown: Option<&Subject>) -> bool {
        let main = activity.main();
        let transcript = main.transcript();
        let decisions = activity.decisions();
        self.status != transcript.status().into()
            || self.working != main.busy()
            || self.operator_turn != activity.main_operator_turn()
            || self.working_descendants != activity.working_descendants()
            || self.subagents != activity.children().len()
            || self.watching != *activity.watching()
            || self.wall.working != caption(transcript)
            || self.shown.as_ref().is_some_and(|card| {
                shown
                    .and_then(|subject| activity.subject(subject))
                    .is_some_and(|view| card.wall.working != caption(view.transcript()))
            })
            || self.pending.len() != decisions.len()
            || self
                .pending
                .iter()
                .zip(decisions)
                .any(|(sent, pending)| sent.handle != WireHandle::from(&pending.handle))
            || !self.shows(shown)
    }
}

impl Finish {
    /// Main's latest turn: from its last prompt on.
    fn of(transcript: &Transcript) -> Self {
        let blocks = transcript.blocks();
        let start = blocks
            .iter()
            .rposition(|block| matches!(block.body, Body::Prompt(_)))
            .unwrap_or(0);
        // A turn still running has not finished: the bell has nothing of it
        // to say yet, and its words stay out of the digest while they
        // stream (F-A2).
        if transcript.turn_started().is_some() {
            return Self::default();
        }
        let turn = &blocks[start..];
        // The turn's last test run, and what it said: a failing summary,
        // or nothing for a green run.
        let failing_tests = turn
            .iter()
            .rev()
            .find_map(|block| match &block.body {
                Body::Tool(tool) => {
                    let text = tool
                        .output
                        .as_ref()
                        .map(|output| output.text.as_str())
                        .or(tool.result_line.as_deref())?;
                    text.contains(" passed;").then(|| failing_tests(text))
                }
                _ => None,
            })
            .flatten();
        // The final answer: the turn's prose after its last call, when it
        // is a single paragraph.
        let last_tool = turn
            .iter()
            .rposition(|block| matches!(block.body, Body::Tool(_)))
            .map_or(0, |at| at + 1);
        let mut prose = turn[last_tool..].iter().filter(|block| {
            matches!(
                block.body,
                Body::Paragraph { .. }
                    | Body::Heading { .. }
                    | Body::Bullet { .. }
                    | Body::Code { .. }
            )
        });
        let answer = match (prose.next(), prose.next()) {
            (
                Some(Block {
                    body: Body::Paragraph { spans },
                    ..
                }),
                None,
            ) => one_row(spans),
            _ => None,
        };
        let worked_ms = turn.iter().rev().find_map(|block| match &block.body {
            Body::TurnEnd(end) => end.elapsed_ms,
            _ => None,
        });
        Self {
            failing_tests,
            answer,
            worked_ms,
        }
    }
}

/// A paragraph that is one row of words, trimmed: a toast words only that
/// (#173). `None` for a longer one, read no further than a row.
fn one_row(spans: &[Span]) -> Option<String> {
    let mut chars = spans.iter().flat_map(|span| span.text.chars());
    let row = row(&mut chars, ROW_CHARS);
    (!row.contains('\n') && chars.all(char::is_whitespace)).then_some(row)
}

/// The first `max` characters of `chars` after its leading whitespace,
/// trimmed: as much of the text as a row shows.
fn row(chars: &mut impl Iterator<Item = char>, max: usize) -> String {
    let mut row: String = chars
        .by_ref()
        .skip_while(|c| c.is_whitespace())
        .take(max)
        .collect();
    row.truncate(row.trim_end().len());
    row
}

/// A test run's summary from its output (`357 passed; 2 failed`): the last
/// line that counts both, cut to the two counts. `None` when the output
/// reports no failing run.
pub fn failing_tests(output: &str) -> Option<String> {
    for line in output.lines().rev() {
        let Some(passed_at) = line.find(" passed;") else {
            continue;
        };
        let passed: String = line[..passed_at]
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let rest = &line[passed_at + " passed;".len()..];
        let failed: String = rest
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if passed.is_empty() || failed.is_empty() {
            continue;
        }
        if failed == "0" || !rest.trim_start()[failed.len()..].starts_with(" failed") {
            return None;
        }
        return Some(format!("{passed} passed; {failed} failed"));
    }
    None
}

/// What a Thread with no title is called: its first prompt, shortened.
pub fn prompt_title(transcript: &Transcript) -> Option<String> {
    transcript
        .blocks()
        .iter()
        .find_map(|block| match &block.body {
            Body::Prompt(text) if block.wake.is_none() => Some(crate::engine::title_from_prompt(text)),
            _ => None,
        })
}

/// Files edited anywhere in the Thread, its Subagents' included, with their
/// rolled-up totals.
fn changed_files(activity: ActivityView<'_>) -> Vec<FileChange> {
    let mut changed = Vec::<FileChange>::new();
    let transcripts = std::iter::once(activity.main().transcript()).chain(
        activity
            .children()
            .into_iter()
            .map(|agent| agent.transcript()),
    );
    for transcript in transcripts {
        for file in Instruments::of(transcript).changed {
            match changed.iter_mut().find(|changed| changed.path == file.path) {
                Some(changed) => {
                    changed.added += file.added;
                    changed.removed += file.removed;
                }
                None => changed.push(file),
            }
        }
    }
    changed
}

/// The wall tile's folded reading — rebuilt only when the Thread changed,
/// never per frame (the wall budget: 24 tiles × 60fps must not walk Blocks
/// or format strings). Words only: the tile draws them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WallCard {
    /// The latest test run failed (from `Instruments`, the one O(blocks)
    /// read the wall needs).
    pub tests_failing: bool,
    /// The failing run's own count, where it reported one: the tile reads
    /// `failing 2`, else `failing`.
    pub failing_count: Option<usize>,
    /// The working signal's detail: the progress caption (`Thinking`,
    /// `Retrying · Server busy`), the working state word's tooltip.
    pub working: String,
    /// An alert tile's context: the Decision's subject, or the reason the
    /// Session closed — the `failed` state word's tooltip. Empty when
    /// neither applies.
    pub context: String,
    /// How long the last completed turn worked (`3m 12s`): `done · 3m 12s`.
    pub done_after: Option<String>,
    /// Generation 1's tile lines, a transcript's tail in words. This build
    /// sends none and reads none: a digest carries no transcript (F-A2).
    /// Kept for the frozen gen1 messages, which round-trip byte for byte.
    pub lines: Vec<WallLine>,
}

/// One of generation 1's tile lines (`WallCard::lines`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WallLine {
    pub text: String,
    pub wraps: bool,
}

/// How many Blocks a wall card reaches back for its last finished turn.
const WALL_TAIL_BLOCKS: usize = 16;

impl WallCard {
    /// Fold one Thread's wall reading. It holds none of the transcript's
    /// words: a window that does not draw the Thread is sent its digest
    /// alone (ADR 0032), and a word of the stream in it would refold it
    /// with every beat. The activity phrase stays a status word — naming
    /// the running tool would put `Instruments::of` on every tile every
    /// rebuild during streaming.
    pub fn of(transcript: &Transcript, decision: Option<&Decision>) -> Self {
        let working = caption(transcript);
        let context = match decision {
            Some(decision) => decision_subject(decision),
            // A closed Thread's context is the reason it closed — the last
            // Notice the fold pushed (#22 C14).
            None if transcript.status() == Status::Closed => transcript
                .blocks()
                .iter()
                .rev()
                .find_map(|block| match &block.body {
                    Body::Notice(reason) => Some(reason.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "closed".to_string()),
            None => String::new(),
        };
        let tests = Instruments::of(transcript).tests;
        let failing_count = match tests {
            Some(Tests::Failed { count }) => count,
            _ => None,
        };
        let blocks = transcript.blocks();
        let tail = &blocks[blocks.len().saturating_sub(WALL_TAIL_BLOCKS)..];
        let done_after = tail.iter().rev().find_map(|block| match &block.body {
            Body::TurnEnd(end) if end.outcome == TurnOutcome::Completed => end
                .elapsed_ms
                .map(|ms| worked_label(Duration::from_millis(ms))),
            _ => None,
        });
        Self {
            tests_failing: matches!(tests, Some(Tests::Failed { .. })),
            failing_count,
            working,
            context,
            done_after,
            lines: Vec::new(),
        }
    }
}

/// The working signal's detail: the progress caption, else `Working`.
fn caption(transcript: &Transcript) -> String {
    transcript
        .progress()
        .caption()
        .unwrap_or_else(|| "Working".into())
}

/// A finished turn's length as the tile says it: `41s`, `3m 12s`.
pub fn worked_label(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 60 {
        crate::progress::settled_duration_label(elapsed)
    } else {
        format!("{}m {}s", secs / 60, secs % 60)
    }
}

/// The Decision's subject as one line of words — `Bash · gh issue close
/// 212`, never a `Bash:` label: the tool, then what it would do (the
/// command itself when the input carries one, else the provider's
/// description); a question is its summary. The tool's name alone without
/// a description, else the honest unreadable fallback. Every surface that
/// names a Decision in words goes through here.
pub fn decision_subject(decision: &Decision) -> String {
    if let Some(questions) = questions_of(decision) {
        return crate::questions::summary(questions);
    }
    let command = decision
        .input
        .get("command")
        .and_then(serde_json::Value::as_str)
        .filter(|command| !command.trim().is_empty());
    let text = match command {
        Some(command) => command,
        None => decision.description.as_str(),
    };
    match (decision.tool_name.as_str(), text) {
        ("", "") => "unreadable permission request".to_string(),
        (tool, "") => tool.to_string(),
        ("", text) => text.to_string(),
        (tool, text) => format!("{tool} \u{b7} {text}"),
    }
}

fn questions_of(decision: &Decision) -> Option<&[crate::questions::Question]> {
    match &decision.kind {
        crate::DecisionKind::Questions(questions) => Some(questions),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
