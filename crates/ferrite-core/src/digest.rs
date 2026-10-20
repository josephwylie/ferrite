//! What every window reads of a Thread it does not draw (ADR 0032). The
//! Server folds it from the Thread's Activity and sends it to every window;
//! only a Thread drawn at Transcript level is mirrored whole.

use std::collections::HashMap;
use std::time::Duration;

use crate::activity::ToolTiming;
use crate::docview::{is_test_run, passed_count, Instruments, Tests};
use crate::transcript::{Block, Body, Span, Status, ToolBlock, ToolState, Transcript};
use crate::{Decision, TurnOutcome};

/// The agent's and a tool call's bullet: Geist Mono's own `●`.
pub const BULLET: &str = "\u{25cf}";
/// The result elbow: Geist Mono's own `└`.
pub const ELBOW: &str = "\u{2514}";
/// A turn or an agent failed.
pub const FAILED: &str = "failed";

/// The wall tile's folded reading — rebuilt only when the Thread changed,
/// never per frame (the wall budget: 24 tiles × 60fps must not walk Blocks
/// or format strings). Words only: the tile draws them.
#[derive(Clone, Debug, Default, PartialEq)]
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
    /// The transcript's last lines as the tile prints them, oldest first
    /// (the tile shows the last `WALL_LINES` rows), a pending Decision's
    /// lines last.
    pub lines: Vec<WallLine>,
}

/// One line of a wall tile: one `TEXT_MUTED` run, its glyph included —
/// `● Update(canvas.rs) +31 −9`, `  └ 212/357 tests`, `◆ Bash wants to
/// run`, `  $ gh issue close 212`, `? Which fix do you want?`.
#[derive(Clone, Debug, PartialEq)]
pub struct WallLine {
    pub text: String,
    /// Agent prose: wider than the tile it wraps once, to a continuation
    /// at col 2 (`  from the spike notes`); every other line truncates.
    pub wraps: bool,
}

impl WallLine {
    fn line(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            wraps: false,
        }
    }
}

/// How many Blocks a wall card reaches back for — more than any tile can
/// show.
const WALL_TAIL_BLOCKS: usize = 16;

/// `◆`, typed: the tile's lines are one run each, glyph included (F-6).
const DIAMOND_GLYPH: &str = "\u{25c6}";

impl WallCard {
    /// Fold one Thread's wall reading, with its calls' clocks: a settled
    /// call's line carries its time (`● Bash(cargo test) 1m01s`). The
    /// activity phrase stays a status word — naming the running tool would
    /// put `Instruments::of` on every tile every rebuild during streaming.
    pub fn of(
        transcript: &Transcript,
        decision: Option<&Decision>,
        timings: Option<&HashMap<String, ToolTiming>>,
    ) -> Self {
        let working = transcript
            .progress()
            .caption()
            .unwrap_or_else(|| "Working".into());
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
            lines: wall_lines(tail, decision, timings),
        }
    }
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

/// A transcript tail as the tile prints it, oldest first (F-6): agent
/// prose behind `●` (it may wrap once); every call behind `●` with its
/// trail inline after one space (`+31 −9`, a settled time, `ok` for a
/// passed test run); under a test run its progress (`  └ 212/357 tests`)
/// or, settled red, its result (`  └ 357 passed; 2 failed`); a failed
/// call's reason; a notice behind `◆`. Prompts, reasoning, code, meta and
/// a completed turn's end leave no line (the state word says `done`). A
/// pending approval drops the call it gates and closes the list with
/// `◆ Bash wants to run` and `  $ <command>`; a question with `? <text>`.
fn wall_lines(
    tail: &[Block],
    decision: Option<&Decision>,
    timings: Option<&HashMap<String, ToolTiming>>,
) -> Vec<WallLine> {
    let first_line = |text: &str| -> String {
        text.lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let gated = decision
        .map(|decision| decision.tool_use_id.as_str())
        .filter(|id| !id.is_empty());
    let mut lines = Vec::new();
    for block in tail {
        match &block.body {
            Body::Paragraph { .. } | Body::Bullet { .. } | Body::Heading { .. } => {
                let Some(text) = tail_text(&block.body, false) else {
                    continue;
                };
                // Two source lines at most: a line break in the prose is
                // where its continuation starts.
                let mut source = text.lines().filter(|line| !line.trim().is_empty());
                let head = source.next().unwrap_or_default().trim().to_string();
                let text = match source.next() {
                    Some(next) => format!("{BULLET} {head}\n{}", next.trim()),
                    None => format!("{BULLET} {head}"),
                };
                lines.push(WallLine {
                    text: text.into(),
                    wraps: true,
                });
            }
            Body::Notice(text)
                if decision.is_none() && !crate::transcript::is_opened_notice(text) =>
            {
                let text = first_line(notice_text(text.trim(), false));
                if !text.is_empty() {
                    lines.push(WallLine::line(format!("{DIAMOND_GLYPH} {text}")));
                }
            }
            Body::TurnEnd(end) if !end.completed() => {
                lines.push(WallLine::line(format!("  {ELBOW} {}", end.text())));
            }
            Body::Tool(tool) => {
                if gated == Some(tool.call.as_str()) {
                    continue;
                }
                lines.push(WallLine::line(wall_call_line(tool, timings)));
                if let Some(result) = wall_result_line(tool) {
                    lines.push(WallLine::line(format!("  {ELBOW} {result}")));
                }
            }
            _ => {}
        }
    }
    if let Some(decision) = decision {
        lines.extend(wall_decision_lines(decision));
    }
    lines
}

/// `● Name(args) trail`: the call as the transcript names it, its trail
/// one space after it — the change (`+31 −9`), `ok` for a passed test run,
/// else a settled time from one second up. A running call has no trail:
/// the state line's clock is ticking.
fn wall_call_line(tool: &ToolBlock, timings: Option<&HashMap<String, ToolTiming>>) -> String {
    let label = tool.label();
    let trail = match &tool.state {
        ToolState::Running => None,
        ToolState::Ok if !tool.diffs.is_empty() => {
            let added: usize = tool.diffs.iter().map(|diff| diff.added).sum();
            let removed: usize = tool.diffs.iter().map(|diff| diff.removed).sum();
            Some(format!("+{added} \u{2212}{removed}"))
        }
        ToolState::Ok if is_test_run(tool) => Some("ok".to_string()),
        _ => match timings.and_then(|map| map.get(&tool.call)) {
            Some(ToolTiming::Done(total))
                if total.as_millis() >= crate::progress::DURATION_MIN_MS =>
            {
                Some(crate::progress::live_seconds(*total))
            }
            _ => None,
        },
    };
    match trail {
        Some(trail) => format!("{BULLET} {label} {trail}"),
        None => format!("{BULLET} {label}"),
    }
}

/// What hangs under a call on its tile: a running test run's progress
/// (`212/357 tests`, `ToolBlock::progress`), a red run's result (`357
/// passed; 2 failed`), a failed call's reason. A passed run says `ok` on
/// its own line, and nothing else hangs.
fn wall_result_line(tool: &ToolBlock) -> Option<String> {
    match &tool.state {
        ToolState::Running => tool.progress.as_ref().map(|progress| {
            if progress.known() {
                format!("{}/{} tests", progress.done, progress.total)
            } else {
                format!("{} tests", progress.done)
            }
        }),
        ToolState::Failed(message) => {
            if is_test_run(tool) {
                if let Some(summary) = test_summary(tool) {
                    return Some(summary);
                }
            }
            let reason = message
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(str::trim)
                .unwrap_or_default();
            Some(if reason.is_empty() {
                FAILED.to_string()
            } else {
                format!("{} \u{b7} {reason}", FAILED)
            })
        }
        _ => None,
    }
}

/// A red test run's own count line, `357 passed; 2 failed`, read from the
/// last line of its output that reports both.
fn test_summary(tool: &ToolBlock) -> Option<String> {
    let failure = match &tool.state {
        ToolState::Failed(message) => Some(message.as_str()),
        _ => None,
    };
    let texts = [
        failure,
        tool.result_line.as_deref(),
        tool.output.as_ref().map(|output| output.text.as_str()),
    ];
    texts.into_iter().flatten().find_map(|text| {
        text.lines().rev().find_map(|line| {
            let passed = passed_count(line)?;
            let failed = count_before(line, "failed")?;
            Some(format!("{passed} passed; {failed} failed"))
        })
    })
}

/// The number standing directly before `word` (`2 failed` → 2).
fn count_before(line: &str, word: &str) -> Option<usize> {
    let lower = line.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    tokens
        .windows(2)
        .find_map(|pair| (pair[1] == word).then(|| pair[0].parse().ok()).flatten())
}

/// A pending Decision's lines at the foot of its tile: an approval names
/// what it would run (`◆ Bash wants to run`, then `  $ <command>`), else
/// what it asks; a question is `? <its text>`.
fn wall_decision_lines(decision: &Decision) -> Vec<WallLine> {
    if let Some(questions) = questions_of(decision) {
        let text = questions
            .first()
            .map(|question| question.question.trim().to_string())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| crate::questions::summary(questions));
        let text = text.lines().next().unwrap_or_default().trim().to_string();
        return vec![WallLine::line(format!("? {text}"))];
    }
    let tool = crate::transcript::display_tool_name(&decision.tool_name).to_string();
    let command = decision
        .input
        .get("command")
        .and_then(serde_json::Value::as_str)
        .filter(|command| !command.trim().is_empty());
    match (tool.is_empty(), command) {
        (false, Some(command)) => {
            let command = crate::providers::shell::unwrap_shell(command).to_string();
            let command = command
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or_default()
                .trim()
                .to_string();
            vec![
                WallLine::line(format!("{DIAMOND_GLYPH} {tool} wants to run")),
                WallLine::line(format!("  $ {command}")),
            ]
        }
        (false, None) => {
            let mut lines = vec![WallLine::line(format!(
                "{DIAMOND_GLYPH} {tool} wants approval"
            ))];
            let description = decision.description.trim();
            if !description.is_empty() {
                let description = description.lines().next().unwrap_or_default().trim();
                lines.push(WallLine::line(format!("  {description}")));
            }
            lines
        }
        (true, _) => vec![WallLine::line(format!(
            "{DIAMOND_GLYPH} {}",
            decision_subject(decision)
        ))],
    }
}

/// A Block's visible text as one line of words — what a wall tile's prose
/// line reads (`wall_lines`), spelled once.
pub fn tail_text(body: &Body, docked: bool) -> Option<String> {
    let prose = |spans: &[Span]| -> String {
        spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>()
            .trim()
            .to_string()
    };
    let text = match body {
        Body::Prompt(text) => text.trim().to_string(),
        Body::Paragraph { spans } | Body::Bullet { spans } | Body::Heading { spans, .. } => {
            prose(spans)
        }
        Body::Code { language, .. } => format!("```{}", language.as_deref().unwrap_or("")),
        Body::Tool(tool) => tool.label(),
        Body::Notice(text) => notice_text(text.trim(), docked).to_string(),
        Body::Meta(text) => text.clone(),
        Body::TurnEnd(end) if end.outcome == TurnOutcome::Completed => return None,
        Body::TurnEnd(end) => end.text(),
        Body::Thinking(text) => crate::progress::headline(text).to_string(),
        Body::Visual(visual) => format!("Visual  {}", visual.title.trim()),
    };
    (!text.is_empty()).then_some(text)
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

/// A notice as the L2 tail reads it: whole, or only its lead phrase (before
/// the first ` · `) while the Decision it announces is docked below it.
pub fn notice_text(text: &str, docked: bool) -> &str {
    match text.split_once(" \u{b7} ") {
        Some((lead, _)) if docked => lead,
        _ => text,
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
