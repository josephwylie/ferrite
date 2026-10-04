//! Folds SessionEvents into stable-identity Blocks.
//!
//! Events in, Blocks out — no window, no timers. Every apply reports exactly
//! which Blocks changed. That is the seam the wall will repaint through; the
//! single Pane shipping today still redraws whole and drops the report.

use std::sync::Arc;

use crate::progress::{Phase, Progress, ProgressEvent, StepStatus};
use crate::{Hunk, RateLimitWindow, SessionEvent, ToolResult, TurnOutcome};

mod highlight;
pub use highlight::{language_for_path, tokens as highlight_tokens, Lexer};

/// A Block's identity, stable for as long as the Block lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(u64);

impl BlockId {
    /// Render-only selection identity for transcript metadata that is not a
    /// transcript Block: the banner heading a transcript, whose title and
    /// facts select and copy like any row.
    pub const BANNER: Self = Self(u64::MAX - 1);
}

/// One rendered unit of the transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub id: BlockId,
    pub body: Body,
    /// Exact Markdown for answer sections. The renderer can join adjacent
    /// sections without losing links, nesting, HTML or list numbering.
    /// Provider events remain the durable source; replay rebuilds this cache.
    pub markdown: Option<String>,
    /// Original identity of adjacent Markdown sections. Carried by every
    /// section so history eviction cannot remount a streaming document.
    pub markdown_run: Option<BlockId>,
    /// When the operator sent a prompt, as its band prints it (`7:31 pm`):
    /// stamped by the `Input::PromptObservation` that follows the prompt,
    /// live and on replay alike. `None` for every other Block and for a
    /// prompt from a log written before the time was kept.
    pub sent_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Paragraph {
        spans: Vec<Span>,
    },
    Heading {
        level: u8,
        spans: Vec<Span>,
    },
    Bullet {
        spans: Vec<Span>,
    },
    Code {
        language: Option<String>,
        source: String,
        /// Filled in when the injected highlighter answers; None until then,
        /// so a Pane renders plain code immediately and never waits.
        tokens: Option<Vec<Token>>,
    },
    Tool(ToolBlock),
    /// A line the operator sent.
    Prompt(String),
    /// Extended thinking, kept apart from the answer.
    Thinking(String),
    /// Something Ferrite or the provider says out of band: a closed session,
    /// a Decision, a failure.
    Notice(String),
    /// Bookkeeping the operator glances at — a turn's cost.
    Meta(String),
    /// How a turn ended: its completion stamp, or the note that it was
    /// interrupted. Typed so the turn footer can be drawn and searched
    /// without reparsing its words; `TurnEnd::text` is what it says.
    TurnEnd(TurnEnd),
}

/// The row a finished turn leaves behind.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnEnd {
    pub outcome: TurnOutcome,
    /// Wall time from the prompt to the end, when observed.
    pub elapsed_ms: Option<u64>,
    /// The local clock when it completed, as the stamp shows it.
    pub completed_at: Option<String>,
    /// The turn's input and output tokens (`↑ 3.2k ↓ 1.1k`), as the
    /// provider's usage reports summed over the turn; `None` from a log
    /// written before they were kept, or a turn that reported none.
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

impl TurnEnd {
    /// What the row says, and what copy and search see: `Worked for 41s ·
    /// 7:32 pm · ↑ 3.2k ↓ 1.1k` (whole seconds, `<1s` under one, `3m 12s`
    /// past a minute), `interrupted · 4.1s`, `failed · 4.1s · <the
    /// provider's message>`. State words are always lowercase, even leading
    /// a row (`INTERRUPTED`/`FAILED`: the app's lexicon, `theme::words`,
    /// asserts they agree). A time or a count that was never observed (an
    /// older log) is left out rather than guessed.
    pub fn text(&self) -> String {
        let elapsed = self.elapsed_ms.map(std::time::Duration::from_millis);
        let mut parts: Vec<String> = Vec::new();
        match &self.outcome {
            TurnOutcome::Completed => {
                parts.push(match elapsed {
                    Some(elapsed) => format!(
                        "Worked for {}",
                        crate::progress::settled_duration_label(elapsed)
                    ),
                    None => "Worked".into(),
                });
                parts.extend(self.completed_at.clone().filter(|at| !at.is_empty()));
                if let (Some(input), Some(output)) = (self.input_tokens, self.output_tokens) {
                    parts.push(format!(
                        "\u{2191} {} \u{2193} {}",
                        crate::progress::token_label(input),
                        crate::progress::token_label(output)
                    ));
                }
            }
            TurnOutcome::Interrupted => {
                parts.push(Self::INTERRUPTED.into());
                parts.extend(elapsed.map(crate::progress::duration_label));
            }
            TurnOutcome::Error(message) => {
                parts.push(Self::FAILED.into());
                parts.extend(elapsed.map(crate::progress::duration_label));
                parts.extend((!message.is_empty()).then(|| message.clone()));
            }
        }
        parts.join(" \u{b7} ")
    }

    pub fn completed(&self) -> bool {
        self.outcome == TurnOutcome::Completed
    }

    /// The lead word of a turn the operator stopped.
    pub const INTERRUPTED: &'static str = "interrupted";
    /// The lead word of a turn that failed.
    pub const FAILED: &'static str = "failed";
}

/// A tool call as one row: what ran, on what, how it went, and the bounded
/// provider output an L1 Pane may disclose.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolBlock {
    /// The provider's id for this call — what a later result quotes.
    pub call: String,
    pub name: String,
    /// Provider-authored purpose when supplied; the input remains in `summary`.
    pub title: Option<String>,
    /// One line naming what the call touched, for a row that never wraps.
    pub summary: String,
    pub state: ToolState,
    /// Every file this call changed.
    pub diffs: Vec<Diff>,
    /// An unmodelled provider result, retained for disclosure.
    pub structured_result: Option<serde_json::Value>,
    /// The first line of the tool's output, trimmed to a row — what the
    /// Pane's `⎿` continuation shows (DirectionDense). Errors carry their
    /// message in `state` instead; disclosure reads `output`.
    pub result_line: Option<String>,
    /// Exact provider output retained for inline disclosure, bounded so one
    /// noisy call cannot dominate a many-Pane cockpit.
    pub output: Option<ToolOutput>,
    /// A test run's count while it streams (`212/357`): its total from the
    /// runner's own announcement (`running 357 tests`), and how many result
    /// lines have arrived. `None` for anything that is not a test run, or a
    /// run whose output never said how many tests it holds.
    pub progress: Option<TestProgress>,
}

/// A running test suite's count, folded from its streamed output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TestProgress {
    pub done: u32,
    pub total: u32,
}

impl ToolBlock {
    /// A bounded, selectable JSON preview for an otherwise unmodelled result.
    /// The original value remains on this block for persistence and replay.
    pub fn structured_output(&self) -> Option<ToolOutput> {
        self.structured_result
            .as_ref()
            .and_then(|value| serde_json::to_string_pretty(value).ok())
            .and_then(|text| retained_output(&text))
    }
}

/// The name a tool row shows, the way Claude Code prints it: a file edit is
/// `Update` (Claude's `Edit` and `MultiEdit`, Codex's `fileChange`), a Codex
/// shell call is `Bash`; `Write` and every other name stand as sent.
pub fn display_tool_name(name: &str) -> &str {
    match name {
        "Edit" | "MultiEdit" | "fileChange" => "Update",
        "commandExecution" => "Bash",
        other => other,
    }
}

/// A command's head: the words before its first option (`gh issue close
/// 212 --reason x` → `gh issue close 212`), the whole command when it has
/// none.
pub fn command_head(command: &str) -> &str {
    let command = command.trim();
    let mut at = 0;
    for word in command.split_whitespace() {
        let start = at + command[at..].find(word).unwrap_or(0);
        if word.starts_with('-') {
            return command[..start].trim_end();
        }
        at = start + word.len();
    }
    command
}

/// What `@@ … @@` names for a hunk, trimmed to the item's head the way the
/// transcript draws it: `fn thread_row(&self, …) -> Div {` → `fn
/// thread_row`, `impl Canvas {` → `impl Canvas`. `None` for an empty one.
pub fn section_head(line: &str) -> Option<String> {
    let line = line.trim();
    let cut = [" where ", "(", "{", "<", " = ", " =", ";"]
        .iter()
        .filter_map(|stop| line.find(stop))
        .min()
        .unwrap_or(line.len());
    let head = line[..cut].trim_end().trim_end_matches(':').trim_end();
    (!head.is_empty()).then(|| head.to_string())
}

/// git's default funcname rule over a hunk's own leading context: the
/// nearest context line above its first change that begins with a letter,
/// `_` or `$` (an unindented item: `fn …`, `impl …`, `def …`), as its
/// head (`section_head`). `None` when the context holds no such line.
pub fn hunk_section(lines: &[String]) -> Option<String> {
    let leading = lines
        .iter()
        .take_while(|line| !line.starts_with('+') && !line.starts_with('-'));
    leading
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|line| line.strip_prefix(' ').unwrap_or(line.as_str()))
        .find(|body| funcname_line(body))
        .and_then(section_head)
}

/// Whether a line starts an item by git's default funcname rule.
pub fn funcname_line(line: &str) -> bool {
    line.chars()
        .next()
        .is_some_and(|ch| ch.is_alphabetic() || ch == '_' || ch == '$')
}

/// What a Thread did to one file, folded from its tool rows' diffs: the
/// hover card's `modified in this thread · +9 −4 · 2 min ago`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStat {
    pub added: usize,
    pub removed: usize,
    /// The first changed line of the most recent edit, on the new side.
    pub last_change_line: Option<u32>,
    /// When that edit landed, where a live fold saw it.
    pub at: Option<std::time::SystemTime>,
}

/// What `blocks` did to `path` (`Transcript::file_stat` over a render
/// window): matched whole or by a path suffix in either direction, the added
/// and removed lines across every edit, the first changed line of the latest
/// edit, and when that edit settled (`settled_at`, by call). `None` when no
/// block changed the file.
pub fn file_stat_in(
    blocks: &[Block],
    settled_at: &std::collections::BTreeMap<String, std::time::SystemTime>,
    path: &str,
) -> Option<FileStat> {
    let mut stat: Option<FileStat> = None;
    for block in blocks {
        let Body::Tool(tool) = &block.body else {
            continue;
        };
        for diff in tool.diffs.iter().filter(|diff| same_file(&diff.path, path)) {
            let entry = stat.get_or_insert(FileStat {
                added: 0,
                removed: 0,
                last_change_line: None,
                at: None,
            });
            entry.added += diff.added;
            entry.removed += diff.removed;
            if let Some(line) = first_changed_line(diff) {
                entry.last_change_line = Some(line);
            }
            if let Some(at) = settled_at.get(&tool.call) {
                entry.at = Some(*at);
            }
        }
    }
    stat
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub text: String,
    pub omitted_bytes: usize,
}
#[derive(Debug, Clone, PartialEq)]
pub struct TurnDiff {
    pub turn_id: String,
    pub diff: String,
    pub omitted_bytes: usize,
}

/// A file edit, ready to draw red and green.
#[derive(Debug, Clone, PartialEq)]
pub struct Diff {
    pub path: String,
    pub hunks: Vec<Hunk>,
    pub added: usize,
    pub removed: usize,
}

impl Diff {
    fn new(path: String, hunks: Vec<Hunk>) -> Self {
        let (added, removed) = hunks.iter().fold((0, 0), |(added, removed), hunk| {
            let (a, r) = hunk_stat(hunk);
            (added + a, removed + r)
        });
        Self {
            added,
            removed,
            path,
            hunks,
        }
    }
}

/// A hunk's change size: its `+` and `-` lines, or what its header counts
/// when the lines a provider sent were cut short of it (`@@ -208,7
/// +208,11 @@` over four context lines is `+7 −3`, however many rows came).
fn hunk_stat(hunk: &Hunk) -> (usize, usize) {
    let count = |mark: char| {
        hunk.lines
            .iter()
            .filter(|line| line.starts_with(mark))
            .count()
    };
    let (added, removed, context) = (count('+'), count('-'), count(' '));
    // Only a change (lines both sides) reads its header: a pure insertion
    // or deletion whose header is rough keeps its own count.
    if added == 0 || removed == 0 {
        return (added, removed);
    }
    let from_header = |lines: u32| (lines as usize).saturating_sub(context);
    (
        added.max(from_header(hunk.new_lines)),
        removed.max(from_header(hunk.old_lines)),
    )
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolState {
    Running,
    Ok,
    /// The stream ended before the provider reported this call’s result.
    Unavailable,
    /// The provider handed the model a failure; the operator sees why.
    Failed(String),
}

/// A run of text with one inline style.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Plain,
    /// Backticked inline code.
    Code,
    /// `**emphasized**` prose.
    Bold,
    /// A markdown link's text. The target is dropped at the fold: v1
    /// renders links inert, and a URL nobody can follow is noise.
    Link,
}

/// A highlighted run of code.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub text: String,
    pub class: Class,
}

/// What a highlighter can say about a run — small on purpose: a Pane maps
/// these to colours, and a bigger vocabulary would be a theme, not a fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Plain,
    Keyword,
    Str,
    Comment,
    Number,
    /// A called name: `name(`, and Rust's `name!`.
    Function,
    /// A capitalised name.
    Type,
    /// ASCII punctuation and operators.
    Punct,
}

/// Syntax highlighting, injected. Ferrite never blocks a frame on it: the
/// implementation may answer whenever it likes, and its answer re-enters the
/// Transcript as an ordinary `Input::Highlighted`.
pub trait Highlighter: Send + Sync {
    fn request(&self, request: HighlightRequest);
}

#[derive(Debug, Clone, PartialEq)]
pub struct HighlightRequest {
    pub block: BlockId,
    pub language: Option<String>,
    pub source: String,
}

/// The highlighter a Transcript uses until one is injected: none at all.
struct Unhighlighted;

impl Highlighter for Unhighlighted {
    fn request(&self, _request: HighlightRequest) {}
}

/// What a Transcript folds: provider events, and answers to its own requests.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Event(SessionEvent),
    /// A line the operator just sent. Not a SessionEvent: it is Ferrite's own
    /// act, and the provider will never echo it back.
    Prompt(String),
    /// Something Ferrite itself needs to say — a send that failed, a session
    /// that was never spawned. Also not the provider's word.
    Notice(String),
    /// The operator answered a Decision. The provider will say what happens
    /// next; this is the record that they were the one who unblocked it.
    Answered {
        allowed: bool,
        tool_name: String,
    },
    /// This Thread's history was replayed from the log into a fresh Session.
    /// Never recorded — a log that replayed itself would grow one revival
    /// line per restart.
    Revived,
    /// A highlighter's answer, arriving whenever it is ready.
    Highlighted {
        block: BlockId,
        tokens: Vec<Token>,
    },
    /// A completion fact observed while the turn was live. Replay must use
    /// these stored values, never a fresh clock or wall time.
    CompletionObservation {
        elapsed_ms: u64,
        completed_at: String,
        /// The turn's token counts, as observed; `None` in older logs.
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
    },
    /// When the prompt just before it was sent (`7:31 pm`), observed live
    /// and recorded right after the prompt. Replay restores the stored
    /// value; a log from before it was kept draws no time.
    PromptObservation {
        sent_at: String,
    },
}

/// What one apply changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Update {
    /// Exactly the Blocks whose content changed.
    pub dirty: Vec<BlockId>,
    /// Blocks that fell off the far end of the transcript and no longer exist.
    pub evicted: Vec<BlockId>,
    /// A point the log is worth flushing at — never mid-delta.
    pub boundary: Option<Boundary>,
}

/// A Thread's own plan, as it works it. Counted off the tool calls the
/// provider already makes — `claude` 2.1.243 plans with TaskCreate and marks
/// work done with TaskUpdate, which is what the committed `todo` capture
/// shows. A provider that plans some other way simply has none of this.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Todos {
    pub done: usize,
    pub total: usize,
}

/// Latest provider-reported context occupancy and the model's window limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub total_tokens: u64,
    pub context_window: Option<u64>,
}

/// Latest live account windows. These are deliberately not restored from the
/// transcript log: a persisted percentage becomes misleading as soon as its
/// rolling window resets.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RateLimits {
    pub five_hour: Option<RateLimitWindow>,
    pub weekly: Option<RateLimitWindow>,
}

/// What the Session is doing, as the transcript last saw it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Streaming,
    /// Stopped on a Decision only the operator can answer.
    Blocked,
    Closed,
}

/// Somewhere the transcript is consistent enough to persist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boundary {
    TurnEnded,
    Closed,
}

pub struct Transcript {
    blocks: Vec<Block>,
    last_id: u64,
    /// Monotonic presentation version for cached transcript renderers.
    revision: u64,
    /// The Block still growing, and the raw markdown it was folded from.
    open: Option<BlockId>,
    source: String,
    highlighter: Arc<dyn Highlighter>,
    /// How many Blocks a Pane keeps before the oldest fall away.
    capacity: usize,
    status: Status,
    model: Option<String>,
    session_id: Option<String>,
    last_cost: Option<f64>,
    /// The current turn's result, cleared when another turn starts. Cost
    /// cannot stand in for this: Codex completes without reporting dollars.
    turn_outcome: Option<TurnOutcome>,
    turn_diff: Option<TurnDiff>,
    usage: Option<Usage>,
    usage_details: Option<crate::UsageDetails>,
    context_details: Option<crate::ContextDetails>,
    mcp_servers: Vec<crate::McpServer>,
    mcp_authorizations: std::collections::BTreeMap<String, String>,
    rate_limits: RateLimits,
    /// When the running turn began — the operator's prompt went out — for
    /// the working line's clock. None between turns.
    turn_started: Option<std::time::Instant>,
    /// Output tokens the running turn has produced, summed across the
    /// messages it streams. Adapters normalize current reports to a per-turn
    /// total; the smaller-report fallback supports older stored events.
    turn_output_tokens: u64,
    last_report: u64,
    /// Input tokens the running (or last) turn consumed, summed the same way
    /// as `turn_output_tokens` (a running total grows; a smaller report is a
    /// new message's own count).
    turn_input_tokens: u64,
    last_input_report: u64,
    /// When each edit settled, where a live fold saw it (`note_settled_at`):
    /// the hover card's `2 min ago`. Replay never stamps one.
    settled_at: std::collections::BTreeMap<String, std::time::SystemTime>,
    /// Which reasoning summary part the tail Block belongs to.
    summary_index: Option<u64>,
    progress: Progress,
    output_tails: std::collections::BTreeMap<String, OutputTail>,
    /// Each running test call's count, folded line by line from its stream.
    test_counts: std::collections::BTreeMap<String, TestCounter>,
    thinking_open: bool,
    latest_reasoning_part: Option<BlockId>,
    reasoning_parts: std::collections::BTreeMap<(String, u64), BlockId>,
}

/// A test runner's progress, read off its output one complete line at a
/// time: the total it announces and the result lines it prints.
///
/// - cargo: `running N tests` per test binary (summed), one `test <name> ...
///   ok|FAILED|ignored` per test.
/// - pytest: `collected N items`, then `path::test PASSED` (verbose) or a
///   row of `.FsExX` per file.
/// - jest/vitest: per-test `✓`/`✕` lines; `Tests: … N total` at the end.
/// - `go test -v`: each `=== RUN` announces one, each `--- PASS|FAIL|SKIP`
///   settles one.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TestCounter {
    partial: String,
    done: u32,
    total: u32,
}

impl TestCounter {
    fn push(&mut self, text: &str) {
        for ch in text.chars() {
            if matches!(ch, '\n' | '\r') {
                let line = std::mem::take(&mut self.partial);
                self.line(&line);
            } else if self.partial.len() < 4096 {
                self.partial.push(ch);
            }
        }
    }

    fn line(&mut self, line: &str) {
        let line = crate::progress::one_line(line, 4096);
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let count_before = |line: &str, word: &str| -> Option<u32> {
            let words: Vec<&str> = line.split_whitespace().collect();
            words.windows(2).find_map(|pair| {
                (pair[1].trim_end_matches([',', '.', ';']) == word)
                    .then(|| pair[0].parse().ok())
                    .flatten()
            })
        };
        // cargo
        if let Some(rest) = line.strip_prefix("running ") {
            if let Some(count) = rest
                .strip_suffix(" tests")
                .or_else(|| rest.strip_suffix(" test"))
                .and_then(|count| count.trim().parse::<u32>().ok())
            {
                self.total = self.total.saturating_add(count);
                return;
            }
        }
        if test_result_line(line).is_some() {
            self.done = self.done.saturating_add(1);
            return;
        }
        // pytest
        if let Some(rest) = line.strip_prefix("collected ") {
            if let Some(count) = rest
                .split_whitespace()
                .next()
                .and_then(|count| count.parse::<u32>().ok())
            {
                self.total = count;
                return;
            }
        }
        if line.contains("::")
            && [
                " PASSED", " FAILED", " SKIPPED", " ERROR", " XFAIL", " XPASS",
            ]
            .iter()
            .any(|word| line.contains(word))
        {
            self.done = self.done.saturating_add(1);
            return;
        }
        if let Some((path, marks)) = line.split_once(".py ") {
            let marks = marks.split('[').next().unwrap_or("").trim();
            if !path.contains(' ')
                && !marks.is_empty()
                && marks
                    .chars()
                    .all(|ch| matches!(ch, '.' | 'F' | 's' | 'E' | 'x' | 'X'))
            {
                self.done = self.done.saturating_add(marks.chars().count() as u32);
                return;
            }
        }
        // jest / vitest
        if let Some(rest) = line.strip_prefix("Tests:") {
            if let Some(total) = count_before(rest, "total") {
                self.total = total;
            }
            return;
        }
        if ["\u{2713} ", "\u{2715} ", "\u{221a} ", "\u{d7} "]
            .iter()
            .any(|mark| line.starts_with(mark))
        {
            self.done = self.done.saturating_add(1);
            return;
        }
        // go test -v
        if line.starts_with("=== RUN ") {
            self.total = self.total.saturating_add(1);
        } else if line.starts_with("--- PASS")
            || line.starts_with("--- FAIL")
            || line.starts_with("--- SKIP")
        {
            self.done = self.done.saturating_add(1);
        }
    }

    fn progress(&self) -> Option<TestProgress> {
        (self.total > 0).then_some(TestProgress {
            done: self.done.min(self.total),
            total: self.total,
        })
    }
}

/// One cargo test result line (`test nav::rows ... ok`), as `(name,
/// verdict)`: `ok`, `FAILED` or `ignored` (an `ignored, reason` counts as
/// ignored). Shared with the transcript's elbow (`test_run_summary`).
pub fn test_result_line(line: &str) -> Option<(&str, &str)> {
    let rest = line.trim().strip_prefix("test ")?;
    let (name, verdict) = rest.rsplit_once(" ... ")?;
    let verdict = verdict.trim();
    let verdict = match verdict {
        "ok" | "FAILED" | "ignored" => verdict,
        other if other.starts_with("ignored") => "ignored",
        _ => return None,
    };
    Some((name.trim(), verdict))
}

/// Keep the latest output line even after the disclosed prefix reaches its
/// byte limit. A line can span native chunks; carriage returns replace it.
#[derive(Clone, Default)]
struct OutputTail {
    line: String,
    last: String,
}
impl OutputTail {
    fn push(&mut self, text: &str) -> Option<String> {
        for ch in text.chars() {
            if matches!(ch, '\n' | '\r') {
                let line = crate::progress::one_line(&self.line, RESULT_CHARS);
                if !line.is_empty() {
                    self.last = line;
                }
                self.line.clear();
            } else {
                self.line.push(ch);
                if self.line.len() > 2048 {
                    let mut start = self.line.len() - 2048;
                    while !self.line.is_char_boundary(start) {
                        start += 1;
                    }
                    self.line.drain(..start);
                }
            }
        }
        let line = crate::progress::one_line(&self.line, RESULT_CHARS);
        let line = if line.is_empty() { &self.last } else { &line };
        (!line.is_empty()).then(|| line.clone())
    }
}

/// Runtime observations travel separately from replayable content. Activity
/// preserves these while rebuilding an item or loading older child history.
#[derive(Clone)]
pub(crate) struct Runtime {
    progress: Progress,
    output_tails: std::collections::BTreeMap<String, OutputTail>,
    running_calls: std::collections::BTreeSet<String>,
    status: Status,
    model: Option<String>,
    session_id: Option<String>,
    last_cost: Option<f64>,
    turn_outcome: Option<TurnOutcome>,
    usage: Option<Usage>,
    turn_started: Option<std::time::Instant>,
    turn_output_tokens: u64,
    last_report: u64,
    turn_input_tokens: u64,
    last_input_report: u64,
    settled_at: std::collections::BTreeMap<String, std::time::SystemTime>,
    test_counts: std::collections::BTreeMap<String, TestCounter>,
    turn_diff: Option<TurnDiff>,
    usage_details: Option<crate::UsageDetails>,
    context_details: Option<crate::ContextDetails>,
    mcp_servers: Vec<crate::McpServer>,
    mcp_authorizations: std::collections::BTreeMap<String, String>,
    rate_limits: RateLimits,
}

/// Blocks a long-running Thread keeps in memory. Generous enough that a Pane
/// streaming all day scrolls back through the Thread's own history rather
/// than a recent sliver of it.
const DEFAULT_CAPACITY: usize = 2000;

impl Default for Transcript {
    fn default() -> Self {
        Self::new(Arc::new(Unhighlighted))
    }
}

impl Transcript {
    pub fn turn_diff(&self) -> Option<&TurnDiff> {
        self.turn_diff.as_ref()
    }
    pub fn new(highlighter: Arc<dyn Highlighter>) -> Self {
        Self::with_capacity(highlighter, DEFAULT_CAPACITY)
    }

    pub fn with_capacity(highlighter: Arc<dyn Highlighter>, capacity: usize) -> Self {
        Self {
            blocks: Vec::new(),
            last_id: 0,
            revision: 0,
            open: None,
            source: String::new(),
            highlighter,
            capacity,
            status: Status::default(),
            model: None,
            session_id: None,
            last_cost: None,
            turn_outcome: None,
            turn_diff: None,
            usage: None,
            usage_details: None,
            context_details: None,
            mcp_servers: Vec::new(),
            mcp_authorizations: std::collections::BTreeMap::new(),
            rate_limits: RateLimits::default(),
            turn_started: None,
            turn_output_tokens: 0,
            last_report: 0,
            turn_input_tokens: 0,
            last_input_report: 0,
            settled_at: Default::default(),
            summary_index: None,
            progress: Progress::default(),
            output_tails: Default::default(),
            test_counts: Default::default(),
            thinking_open: false,
            latest_reasoning_part: None,
            reasoning_parts: Default::default(),
        }
    }

    pub fn status(&self) -> Status {
        self.status
    }

    /// Advances whenever transcript-owned presentation state may have changed.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn set_attention(&mut self, pending: bool, busy: bool) {
        if pending {
            self.status = Status::Blocked;
        } else if self.status == Status::Blocked {
            self.status = if busy {
                Status::Streaming
            } else {
                Status::Idle
            };
        }
        self.advance_revision();
    }

    pub(crate) fn clear_activity(&mut self) -> Update {
        if matches!(self.status, Status::Streaming | Status::Blocked) {
            self.status = Status::Idle;
        }
        self.turn_started = None;
        self.progress.disconnected();
        let update = Update {
            dirty: self.retire_tools(),
            ..Update::default()
        };
        self.advance_revision();
        update
    }

    pub(crate) fn runtime(&self) -> Runtime {
        Runtime {
            progress: self.progress.clone(),
            output_tails: self.output_tails.clone(),
            running_calls: self
                .blocks
                .iter()
                .filter_map(|block| match &block.body {
                    Body::Tool(tool) if tool.state == ToolState::Running => Some(tool.call.clone()),
                    _ => None,
                })
                .collect(),
            status: self.status,
            model: self.model.clone(),
            session_id: self.session_id.clone(),
            last_cost: self.last_cost,
            turn_outcome: self.turn_outcome.clone(),
            usage: self.usage,
            turn_started: self.turn_started,
            turn_output_tokens: self.turn_output_tokens,
            last_report: self.last_report,
            turn_input_tokens: self.turn_input_tokens,
            last_input_report: self.last_input_report,
            settled_at: self.settled_at.clone(),
            test_counts: self.test_counts.clone(),
            turn_diff: self.turn_diff.clone(),
            usage_details: self.usage_details.clone(),
            context_details: self.context_details.clone(),
            mcp_servers: self.mcp_servers.clone(),
            mcp_authorizations: self.mcp_authorizations.clone(),
            rate_limits: self.rate_limits,
        }
    }

    pub(crate) fn restore_runtime(&mut self, runtime: Runtime) {
        self.progress = runtime.progress;
        self.output_tails = runtime.output_tails;
        for block in &mut self.blocks {
            if let Body::Tool(tool) = &mut block.body {
                match tool.state {
                    ToolState::Running if !runtime.running_calls.contains(&tool.call) => {
                        tool.state = ToolState::Unavailable
                    }
                    ToolState::Unavailable if runtime.running_calls.contains(&tool.call) => {
                        tool.state = ToolState::Running
                    }
                    _ => {}
                }
            }
        }
        self.status = runtime.status;
        self.model = runtime.model;
        self.session_id = runtime.session_id;
        self.last_cost = runtime.last_cost;
        self.turn_outcome = runtime.turn_outcome;
        self.usage = runtime.usage;
        self.turn_started = runtime.turn_started;
        self.turn_output_tokens = runtime.turn_output_tokens;
        self.last_report = runtime.last_report;
        self.turn_input_tokens = runtime.turn_input_tokens;
        self.last_input_report = runtime.last_input_report;
        // An edit's time is a live observation the rebuilt content cannot
        // replay: keep what was seen.
        for (call, at) in runtime.settled_at {
            self.settled_at.entry(call).or_insert(at);
        }
        for (call, count) in runtime.test_counts {
            self.test_counts.entry(call).or_insert(count);
        }
        self.turn_diff = runtime.turn_diff;
        self.usage_details = runtime.usage_details;
        self.context_details = runtime.context_details;
        self.mcp_servers = runtime.mcp_servers;
        self.mcp_authorizations = runtime.mcp_authorizations;
        self.rate_limits = runtime.rate_limits;
        self.advance_revision();
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn last_cost(&self) -> Option<f64> {
        self.last_cost
    }

    /// The current turn's result; None before a turn ends or after new
    /// activity. Session closure and revival preserve the recorded result.
    pub fn turn_outcome(&self) -> Option<&TurnOutcome> {
        self.turn_outcome.as_ref()
    }

    /// At rest after a successful turn, independent of whether its provider
    /// reported a cost. A closed Session still needs attention.
    pub fn turn_completed(&self) -> bool {
        self.status == Status::Idle && self.turn_outcome == Some(TurnOutcome::Completed)
    }

    pub fn usage(&self) -> Option<Usage> {
        self.usage
    }

    pub fn usage_details(&self) -> Option<&crate::UsageDetails> {
        self.usage_details.as_ref()
    }

    pub fn context_details(&self) -> Option<&crate::ContextDetails> {
        self.context_details.as_ref()
    }

    pub fn mcp_servers(&self) -> &[crate::McpServer] {
        &self.mcp_servers
    }

    pub fn mcp_authorizations(&self) -> &std::collections::BTreeMap<String, String> {
        &self.mcp_authorizations
    }

    pub fn rate_limits(&self) -> RateLimits {
        self.rate_limits
    }

    /// How long the running turn has been going; None between turns. Read
    /// off `clock::instant`, so a fixture freezes it.
    pub fn turn_elapsed(&self) -> Option<std::time::Duration> {
        self.turn_started
            .map(|started| crate::clock::instant().saturating_duration_since(started))
    }

    /// Output tokens the running (or last) turn produced.
    pub fn turn_output_tokens(&self) -> u64 {
        self.turn_output_tokens
    }

    /// Input tokens the running (or last) turn consumed.
    pub fn turn_input_tokens(&self) -> u64 {
        self.turn_input_tokens
    }

    /// When the Thread's first prompt in this window was sent, as its band
    /// prints it: the banner's `started 7:18 pm`.
    pub fn started_at(&self) -> Option<&str> {
        self.blocks
            .iter()
            .find(|block| matches!(block.body, Body::Prompt(_)))
            .and_then(|block| block.sent_at.as_deref())
    }

    /// How many turns have ended in this window: the banner's `2 turns`.
    pub fn turn_count(&self) -> usize {
        self.blocks
            .iter()
            .filter(|block| matches!(block.body, Body::TurnEnd(_)))
            .count()
    }

    /// The latest completed turn's settled time: the banner's `41s working`.
    pub fn last_working_ms(&self) -> Option<u64> {
        self.blocks
            .iter()
            .rev()
            .find_map(|block| match &block.body {
                Body::TurnEnd(end) if end.completed() => end.elapsed_ms,
                _ => None,
            })
    }

    /// Record when an edit settled, observed live (`activity` calls this for
    /// a live completion only): the hover card's age.
    pub(crate) fn note_settled_at(&mut self, call: &str, at: std::time::SystemTime) {
        self.settled_at.insert(call.to_owned(), at);
    }

    /// What this Thread did to `path` (matched whole, or by a path suffix in
    /// either direction, so `crates/x.rs` meets `/repo/crates/x.rs`): its
    /// added and removed lines across every edit, the first changed line of
    /// the latest edit, and when that edit landed. `None` when the Thread
    /// never changed the file.
    pub fn file_stat(&self, path: &str) -> Option<FileStat> {
        file_stat_in(&self.blocks, &self.settled_at, path)
    }

    /// When each edit settled, where a live fold saw it, by call: what a
    /// render window's `file_stat_in` reads.
    pub fn settled_at(&self) -> &std::collections::BTreeMap<String, std::time::SystemTime> {
        &self.settled_at
    }

    /// The Thread's plan, once it has made one.
    pub fn progress(&self) -> &Progress {
        &self.progress
    }

    pub fn todos(&self) -> Option<Todos> {
        if self.progress.has_plan {
            return Some(Todos {
                done: self
                    .progress
                    .plan
                    .iter()
                    .filter(|step| step.status == StepStatus::Completed)
                    .count(),
                total: self.progress.plan.len(),
            });
        }
        None
    }

    /// The step the Thread works now, by the tasks strip's reading: the
    /// first not-yet-finished subject in creation order. The same
    /// compromise `todos()` documents — completions cannot be matched to
    /// their steps, so order stands in — and a step created without a
    /// subject names nothing.
    pub fn current_task(&self) -> Option<&str> {
        if self.progress.has_plan {
            return self.progress.current_step();
        }
        None
    }

    /// #11: whether this Thread still offers adopting a CLI session — no
    /// conversation yet (nothing here beyond Ferrite's own notices and
    /// bookkeeping) and at rest. One predicate for every surface that
    /// opens the door — the placeholder hint, the `/` menu's local entry,
    /// and the pick that closes the blank Thread — so no two can disagree.
    pub fn offers_import(&self) -> bool {
        self.status() == Status::Idle
            && self.blocks.iter().all(|block| {
                matches!(
                    block.body,
                    Body::Notice(_) | Body::Meta(_) | Body::TurnEnd(_)
                )
            })
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn apply(&mut self, input: Input) -> Update {
        let update = self.apply_inner(input);
        // Activity can resume without a prompt in this window (replay or a
        // revived Session). The previous turn's result must not follow it.
        if matches!(self.status, Status::Streaming | Status::Blocked) {
            self.turn_outcome = None;
        }
        // The turn's clock starts with the prompt — or, for a Thread revived
        // mid-turn with no prompt of this turn in its window, with the
        // first thing that streams.
        match self.status {
            Status::Streaming if self.turn_started.is_none() => {
                self.turn_started = Some(crate::clock::instant());
            }
            Status::Idle => self.turn_started = None,
            _ => {}
        }
        self.advance_revision();
        update
    }

    fn apply_inner(&mut self, input: Input) -> Update {
        let mut update = self.fold(input);
        update.evicted = self.evict();
        if !update.evicted.is_empty() {
            let first = self.blocks.first().map(|block| block.id);
            self.reasoning_parts
                .retain(|_, id| first.is_some_and(|first| *id >= first));
            self.output_tails.retain(|id, _| self.blocks.iter().any(|block| matches!(&block.body, Body::Tool(tool) if &tool.call == id && tool.state == ToolState::Running)));
            self.test_counts.retain(|id, _| self.blocks.iter().any(|block| matches!(&block.body, Body::Tool(tool) if &tool.call == id && tool.state == ToolState::Running)));
            self.settled_at.retain(|id, _| {
                self.blocks
                    .iter()
                    .any(|block| matches!(&block.body, Body::Tool(tool) if &tool.call == id))
            });
        }
        update
    }

    /// Exhaustive by construction: a new SessionEvent variant fails to compile
    /// here until someone decides what a Pane shows for it. That is the point
    /// of a superset event model — a wildcard would silently render nothing.
    fn fold(&mut self, input: Input) -> Update {
        match input {
            Input::CompletionObservation {
                elapsed_ms,
                completed_at,
                input_tokens,
                output_tokens,
            } => {
                // An interrupted or failed turn already left its row; the
                // observation times it. A completed turn ends without a row,
                // so the observation is its stamp.
                if let Some(block) = self.blocks.last_mut() {
                    if let Body::TurnEnd(end) = &mut block.body {
                        if !end.completed() && end.elapsed_ms.is_none() {
                            end.elapsed_ms = Some(elapsed_ms);
                            end.completed_at = Some(completed_at);
                            end.input_tokens = input_tokens;
                            end.output_tokens = output_tokens;
                            return Update {
                                dirty: vec![block.id],
                                ..Update::default()
                            };
                        }
                    }
                }
                let id = self.push(Body::TurnEnd(TurnEnd {
                    outcome: TurnOutcome::Completed,
                    elapsed_ms: Some(elapsed_ms),
                    completed_at: Some(completed_at),
                    input_tokens,
                    output_tokens,
                }));
                Update {
                    dirty: vec![id],
                    ..Update::default()
                }
            }
            Input::PromptObservation { sent_at } => {
                // The prompt it was recorded right after: the latest one.
                let Some(block) = self
                    .blocks
                    .iter_mut()
                    .rev()
                    .find(|block| matches!(block.body, Body::Prompt(_)))
                else {
                    return Update::default();
                };
                if block.sent_at.as_deref() == Some(sent_at.as_str()) {
                    return Update::default();
                }
                block.sent_at = Some(sent_at);
                Update {
                    dirty: vec![block.id],
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::ReasoningSummaryPart {
                item_id,
                summary_index,
                text,
                snapshot,
            }) => {
                if text.is_empty() && !snapshot {
                    return Update::default();
                }
                let key = (item_id, summary_index);
                let existing = self.reasoning_parts.get(&key).copied();
                let (id, changed) = if let Some(id) = existing {
                    let block = self
                        .blocks
                        .iter_mut()
                        .rev()
                        .find(|block| block.id == id)
                        .expect("pruned with blocks");
                    let Body::Thinking(thought) = &mut block.body else {
                        unreachable!()
                    };
                    if snapshot {
                        let changed = *thought != text;
                        *thought = text;
                        (id, changed)
                    } else {
                        thought.push_str(&text);
                        (id, true)
                    }
                } else {
                    if text.is_empty() {
                        return Update::default();
                    }
                    let id = self.push(Body::Thinking(text));
                    self.latest_reasoning_part = Some(id);
                    self.reasoning_parts.insert(key, id);
                    (id, true)
                };
                if changed
                    && self.latest_reasoning_part == Some(id)
                    && !(snapshot && (self.turn_outcome.is_some() || self.status == Status::Closed))
                {
                    if let Some(Block {
                        body: Body::Thinking(thought),
                        ..
                    }) = self.blocks.iter().rev().find(|block| block.id == id)
                    {
                        self.progress.summary(thought);
                    }
                    self.progress.phase(Phase::Thinking);
                    self.status = Status::Streaming;
                }
                Update {
                    dirty: if changed { vec![id] } else { vec![] },
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::Progress { event }) => {
                if let ProgressEvent::Tool { id, .. } = &event {
                    let settled = self.blocks.iter().any(|block| matches!(&block.body, Body::Tool(tool) if &tool.call == id && tool.state != ToolState::Running));
                    if settled || self.turn_outcome.is_some() || self.status == Status::Closed {
                        return Update::default();
                    }
                }
                if matches!(
                    &event,
                    ProgressEvent::Phase { .. } | ProgressEvent::Tool { .. }
                ) && self.status != Status::Blocked
                    && self.status != Status::Closed
                {
                    self.status = Status::Streaming;
                }
                let changed = match &event {
                    ProgressEvent::Phase { phase, detail } => {
                        self.progress.phase != Some(*phase)
                            || self.progress.detail != crate::progress::one_line(detail, 512)
                    }
                    _ => false,
                };
                self.progress.apply(&event);
                // Keep exceptional waits in history, but periodic ticks never
                // append rows or interrupt the Markdown currently streaming.
                let mut dirty = vec![];
                if changed {
                    if let ProgressEvent::Phase {
                        phase: phase @ (Phase::Retrying | Phase::Compacting | Phase::Waiting),
                        detail,
                    } = event
                    {
                        let detail = crate::progress::one_line(&detail, 512);
                        let text = if detail.is_empty() {
                            phase.label().to_string()
                        } else {
                            format!("{} — {detail}", phase.label())
                        };
                        dirty.push(self.push(Body::Notice(text)));
                    }
                }
                Update {
                    dirty,
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::ContentBoundary) => {
                if let Some(block) = self.open {
                    self.ask_to_highlight(block);
                }
                self.open = None;
                self.source.clear();
                self.summary_index = None;
                self.thinking_open = false;
                Update::default()
            }
            Input::Event(SessionEvent::ToolOutputDelta { id, text }) => {
                let Some(block) = self.blocks.iter_mut().rev().find(|block| matches!(&block.body, Body::Tool(tool) if tool.call == id && tool.state == ToolState::Running)) else { return Update::default(); };
                let Body::Tool(tool) = &mut block.body else {
                    unreachable!()
                };
                let output = tool.output.get_or_insert_with(|| ToolOutput {
                    text: String::new(),
                    omitted_bytes: 0,
                });
                let mut end = if output.omitted_bytes > 0 {
                    0
                } else {
                    text.len()
                        .min(TOOL_OUTPUT_BYTES.saturating_sub(output.text.len()))
                };
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                output.text.push_str(&text[..end]);
                output.omitted_bytes = output.omitted_bytes.saturating_add(text.len() - end);
                // A test run counts its results as they stream: the row's
                // `running 357 tests` bar and `212/357`.
                if crate::docview::is_test_run(tool) {
                    let counter = self.test_counts.entry(id.clone()).or_default();
                    counter.push(&text);
                    tool.progress = counter.progress();
                }
                // The latest complete/partial output line is visible while the
                // bounded full output stays available through disclosure.
                tool.result_line = self.output_tails.entry(id).or_default().push(&text);
                Update {
                    dirty: vec![block.id],
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::FileChanges { id, edits }) => {
                let Some(block) = self
                    .blocks
                    .iter_mut()
                    .find(|block| matches!(&block.body, Body::Tool(tool) if tool.call == id))
                else {
                    return Update::default();
                };
                let Body::Tool(tool) = &mut block.body else {
                    unreachable!()
                };
                tool.diffs = edits
                    .into_iter()
                    .map(|edit| Diff::new(edit.path, edit.hunks))
                    .collect();
                Update {
                    dirty: vec![block.id],
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::TurnDiff { turn_id, diff }) => {
                self.turn_diff = retained_output(&diff).map(|output| TurnDiff {
                    turn_id,
                    diff: output.text,
                    omitted_bytes: output.omitted_bytes,
                });
                Update::default()
            }
            // Activity owns attribution and feeds each subject's execution
            // into its own Transcript. Legacy callers cannot fold children
            // into Main by accidentally replaying an attributed observation.
            Input::Event(SessionEvent::Activity(_)) => Update::default(),
            Input::Event(SessionEvent::TextDelta { text }) => {
                self.progress.phase(Phase::Answering);
                self.status = Status::Streaming;
                Update {
                    dirty: self.grow(&text),
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::ThinkingDelta { text }) => {
                self.progress.phase(Phase::Thinking);
                self.status = Status::Streaming;
                self.summary_index = None;
                // Claude sends an empty thinking delta for every redacted
                // thinking block — a signature with no words. It is a sign
                // the model is working, not a paragraph: no Block, or the
                // transcript fills with blank gaps between tool rows.
                if text.is_empty() {
                    return Update::default();
                }
                Update {
                    dirty: vec![self.grow_thinking(&text)],
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::ReasoningSummaryDelta {
                text,
                summary_index,
            }) => {
                self.status = Status::Streaming;
                self.progress.phase(Phase::Thinking);
                // The provider decides where its reasoning breaks; a new index
                // is a new paragraph, not a continuation of the last one.
                if self.summary_index != Some(summary_index) {
                    self.summary_index = Some(summary_index);
                    return Update {
                        dirty: vec![{
                            self.thinking_open = false;
                            self.grow_thinking(&text)
                        }],
                        ..Update::default()
                    };
                }
                Update {
                    dirty: vec![self.grow_thinking(&text)],
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::TokenUsage {
                total_tokens,
                context_window,
                input_tokens,
                output_tokens,
                ..
            }) => {
                self.usage = Some(Usage {
                    total_tokens,
                    context_window,
                });
                // A report that grew continues the last message (a running
                // total); one that shrank is a new message's own count.
                if output_tokens >= self.last_report {
                    self.turn_output_tokens += output_tokens - self.last_report;
                } else {
                    self.turn_output_tokens += output_tokens;
                }
                self.last_report = output_tokens;
                // Input the same way: Claude's per-turn usage and Codex's
                // cumulative counters both land as the turn's own total.
                if input_tokens >= self.last_input_report {
                    self.turn_input_tokens += input_tokens - self.last_input_report;
                } else {
                    self.turn_input_tokens += input_tokens;
                }
                self.last_input_report = input_tokens;
                Update::default()
            }
            Input::Event(SessionEvent::ContextUsage {
                total_tokens,
                context_window,
            }) => {
                self.usage = Some(Usage {
                    total_tokens,
                    context_window,
                });
                Update::default()
            }
            Input::Event(SessionEvent::UsageDetails { details }) => {
                // A turn's own report (Claude's `result`) is the turn's
                // count, whatever the running reports summed to.
                if details.scope == crate::UsageScope::Turn {
                    self.turn_input_tokens = details.input_tokens;
                    if details.output_tokens > 0 {
                        self.turn_output_tokens = details.output_tokens;
                    }
                }
                self.usage_details = Some(details);
                Update::default()
            }
            Input::Event(SessionEvent::RateLimits { five_hour, weekly }) => {
                self.rate_limits = RateLimits { five_hour, weekly };
                Update::default()
            }
            Input::Event(SessionEvent::ContextDetails { details }) => {
                self.context_details = Some(details);
                Update::default()
            }
            Input::Answered { allowed, tool_name } => {
                self.status = Status::Streaming;
                let verb = if allowed { "allowed" } else { "denied" };
                Update {
                    dirty: vec![self.push(Body::Meta(format!("{verb} {tool_name}")))],
                    ..Update::default()
                }
            }
            Input::Revived => {
                self.mcp_servers.clear();
                self.mcp_authorizations.clear();
                self.progress.disconnected();
                self.mcp_servers.clear();
                if matches!(self.status, Status::Streaming | Status::Blocked) {
                    self.status = Status::Idle;
                }
                self.turn_started = None;
                let mut dirty = self.retire_tools();
                dirty.push(self.push(Body::Meta(
                    "revived — new Session, history from the log".into(),
                )));
                Update {
                    dirty,
                    ..Update::default()
                }
            }
            Input::Notice(line) => Update {
                dirty: vec![self.push(Body::Notice(line))],
                ..Update::default()
            },
            Input::Prompt(line) => {
                self.turn_diff = None;
                self.turn_outcome = None;
                self.progress.end_turn();
                self.latest_reasoning_part = None;
                self.progress.phase(Phase::Working);
                // A Closed session stays closed and a Blocked one stays
                // blocked: nothing is streaming in either.
                if let Status::Idle | Status::Streaming = self.status {
                    self.status = Status::Streaming;
                    self.turn_started = Some(crate::clock::instant());
                    self.turn_output_tokens = 0;
                    self.last_report = 0;
                    // The last input report carries over: Codex's counter
                    // is cumulative, so the turn's input is what it grows
                    // by from here.
                    self.turn_input_tokens = 0;
                }
                Update {
                    dirty: vec![self.push(Body::Prompt(line))],
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::ToolCompleted {
                id,
                output,
                is_error,
                result,
            }) => {
                self.progress.finish_tool(&id);
                let state = if is_error {
                    ToolState::Failed(trim(&output, ERROR_CHARS))
                } else {
                    ToolState::Ok
                };
                let (diffs, structured_result) = match result {
                    ToolResult::FileEdit { path, hunks } => (vec![Diff::new(path, hunks)], None),
                    ToolResult::FileEdits { edits } => (
                        edits
                            .into_iter()
                            .map(|edit| Diff::new(edit.path, edit.hunks))
                            .collect(),
                        None,
                    ),
                    ToolResult::Structured { value, .. } => (Vec::new(), Some(value)),
                    _ => (Vec::new(), None),
                };
                // A failure already carries its message in the state; a
                // success keeps its summary (`412 lines`, `6 matches in 2
                // files`) or its first output line for the `└` row.
                self.output_tails.remove(&id);
                self.test_counts.remove(&id);
                let name = self
                    .blocks
                    .iter()
                    .rev()
                    .find_map(|block| match &block.body {
                        Body::Tool(tool) if tool.call == id => Some(tool.name.clone()),
                        _ => None,
                    });
                let result_line = (!is_error)
                    .then(|| {
                        name.as_deref()
                            .and_then(|name| {
                                result_summary(name, structured_result.as_ref(), &output)
                            })
                            .or_else(|| result_line(&output))
                    })
                    .flatten();
                let output = retained_output(&output);
                Update {
                    dirty: self
                        .settle_tool(&id, state, diffs, structured_result, result_line, output)
                        .into_iter()
                        .collect(),
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::ToolStarted { id, name, input }) => {
                // A completed-message snapshot can repeat an already streamed
                // tool. Its native id owns one row and one lifecycle.
                if self
                    .blocks
                    .iter()
                    .any(|block| matches!(&block.body, Body::Tool(tool) if tool.call == id))
                {
                    return Update::default();
                }
                self.status = Status::Streaming;
                self.progress.phase(Phase::Working);
                let block = self.push(Body::Tool(ToolBlock {
                    call: id,
                    summary: tool_summary(&name, &input),
                    title: ["title", "description", "reason"]
                        .into_iter()
                        .find_map(|key| {
                            input
                                .get(key)
                                .and_then(serde_json::Value::as_str)
                                .map(str::trim)
                                .filter(|text| !text.is_empty())
                        })
                        .map(str::to_owned),
                    name,
                    state: ToolState::Running,
                    diffs: Vec::new(),
                    structured_result: None,
                    result_line: None,
                    output: None,
                    progress: None,
                }));
                Update {
                    dirty: vec![block],
                    ..Update::default()
                }
            }
            Input::Highlighted { block, tokens } => Update {
                dirty: self.highlight(block, tokens).into_iter().collect(),
                ..Update::default()
            },
            Input::Event(SessionEvent::Init { session_id, model }) => {
                self.session_id = Some(session_id);
                self.model = Some(model);
                Update::default()
            }
            Input::Event(SessionEvent::ModelChanged { model }) => {
                self.model = Some(model);
                Update::default()
            }
            Input::Event(SessionEvent::ConversationReset { .. }) => Update::default(),
            Input::Event(SessionEvent::TurnEnded { outcome, cost_usd }) => {
                self.progress.end_turn();
                self.latest_reasoning_part = None;
                self.status = Status::Idle;
                self.last_cost = cost_usd;
                self.turn_started = None;
                let mut dirty = self.retire_tools();
                match &outcome {
                    // The cost is kept (`last_cost`) but never rendered —
                    // no dollar value appears anywhere (#22 operator
                    // amendment); a completed turn ends without a row.
                    TurnOutcome::Completed => {}
                    TurnOutcome::Interrupted => dirty.push(self.push(Body::TurnEnd(TurnEnd {
                        outcome: TurnOutcome::Interrupted,
                        elapsed_ms: None,
                        completed_at: None,
                        input_tokens: None,
                        output_tokens: None,
                    }))),
                    TurnOutcome::Error(message) => dirty.push(self.push(Body::TurnEnd(TurnEnd {
                        outcome: TurnOutcome::Error(message.clone()),
                        elapsed_ms: None,
                        completed_at: None,
                        input_tokens: None,
                        output_tokens: None,
                    }))),
                }
                self.turn_outcome = Some(outcome);
                Update {
                    dirty,
                    boundary: Some(Boundary::TurnEnded),
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::RunState { state }) => {
                self.status = match state {
                    crate::RunState::Running => Status::Streaming,
                    crate::RunState::RequiresAction => Status::Blocked,
                    crate::RunState::Idle => Status::Idle,
                };
                Update::default()
            }
            Input::Event(SessionEvent::DecisionRequested { decision }) => {
                if decision.blocks_execution() {
                    // The Decision itself says what waits — it is the
                    // transcript's last row while it pends (the app's tail
                    // row) — so no notice repeats `Bash needs approval`.
                    self.status = Status::Blocked;
                    return Update::default();
                }
                Update {
                    dirty: vec![self.push(Body::Notice(decision.description))],
                    ..Update::default()
                }
            }
            // The Session's command menu, permission mode and model menu are
            // cockpit state for the Composer's chrome, not conversation: no
            // Block, nothing dirty.
            Input::Event(SessionEvent::Commands { .. }) => Update::default(),
            Input::Event(SessionEvent::PermissionMode { .. }) => Update::default(),
            Input::Event(SessionEvent::Models { .. } | SessionEvent::Queue(_)) => Update::default(),
            Input::Event(SessionEvent::McpServers { servers }) => {
                self.mcp_servers = servers;
                Update::default()
            }
            Input::Event(SessionEvent::McpAuthorization { server, url }) => {
                if let Some(url) = url {
                    self.mcp_authorizations.insert(server, url);
                } else {
                    self.mcp_authorizations.remove(&server);
                }
                Update::default()
            }
            Input::Event(SessionEvent::Closed { reason }) => {
                self.mcp_servers.clear();
                self.mcp_authorizations.clear();
                self.progress.disconnected();
                self.mcp_servers.clear();
                self.latest_reasoning_part = None;
                self.status = Status::Closed;
                let mut dirty = self.retire_tools();
                dirty.push(self.push(Body::Notice(reason)));
                Update {
                    dirty,
                    boundary: Some(Boundary::Closed),
                    ..Update::default()
                }
            }
        }
    }

    /// Text streams into the Block it belongs to, not a Block per delta.
    fn grow(&mut self, text: &str) -> Vec<BlockId> {
        let mut dirty = Vec::new();
        self.source.push_str(text);

        // Fold off every section that can no longer grow, then re-render the
        // remainder, which is still streaming.
        while let Some(used) = complete_section(&self.source) {
            let section = self.source[..used].to_string();
            self.source = self.source[used..].to_string();
            if let Some(body) = parse_section(&section) {
                // A section that settled unchanged is still worth highlighting,
                // but it is not dirty unless its body or retained Markdown moved.
                let changed = self.write_open(body, &section);
                let settled = changed.or(self.open);
                dirty.extend(changed);
                if let Some(id) = settled {
                    self.ask_to_highlight(id);
                }
            } else if let Some(block) = self.blocks.last_mut() {
                // Blank lines carry Markdown structure even though they do
                // not have a visual block of their own.
                if let Some(markdown) = &mut block.markdown {
                    markdown.push_str(&section);
                    dirty.push(block.id);
                }
            }
            self.open = None;
        }
        if let Some(body) = parse_section(&self.source) {
            let source = self.source.clone();
            dirty.extend(self.write_open(body, &source));
        }
        dirty
    }

    fn retire_tools(&mut self) -> Vec<BlockId> {
        self.output_tails.clear();
        self.test_counts.clear();
        self.blocks
            .iter_mut()
            .filter_map(|block| match &mut block.body {
                Body::Tool(tool) if tool.state == ToolState::Running => {
                    tool.state = ToolState::Unavailable;
                    Some(block.id)
                }
                _ => None,
            })
            .collect()
    }

    /// Thinking streams like prose but never shares a Block with the answer.
    fn grow_thinking(&mut self, text: &str) -> BlockId {
        if let Some(Block {
            id,
            body: Body::Thinking(thought),
            ..
        }) = self.blocks.last_mut().filter(|_| self.thinking_open)
        {
            thought.push_str(text);
            self.progress.summary(thought);
            let id = *id;
            self.open = None;
            self.source.clear();
            return id;
        }
        self.progress.summary(text);
        let id = self.push(Body::Thinking(text.to_string()));
        self.thinking_open = true;
        id
    }

    /// Write a folded body into the open Block, creating it on first content.
    /// Reports changes to either the folded body or the original Markdown,
    /// since whitespace can change rich-text structure without changing plain text.
    fn write_open(&mut self, body: Body, source: &str) -> Option<BlockId> {
        match self.open {
            Some(id) => {
                // The open Block is the tail by construction. Scanning for it
                // costs the whole transcript on every delta, which is what
                // decays a streaming cockpit from 120fps to 30.
                let block = match self.blocks.last_mut() {
                    Some(block) if block.id == id => block,
                    _ => self.blocks.iter_mut().find(|b| b.id == id)?,
                };
                if block.body == body && block.markdown.as_deref() == Some(source) {
                    return None;
                }
                block.body = body;
                block.markdown = Some(source.to_string());
                Some(id)
            }
            None => {
                let id = self.mint();
                let markdown_run = self
                    .blocks
                    .last()
                    .and_then(|block| block.markdown_run)
                    .unwrap_or(id);
                self.blocks.push(Block {
                    id,
                    body,
                    markdown: Some(source.to_string()),
                    markdown_run: Some(markdown_run),
                    sent_at: None,
                });
                self.open = Some(id);
                Some(id)
            }
        }
    }

    /// Drop the oldest Blocks once the Pane holds more than it keeps.
    fn evict(&mut self) -> Vec<BlockId> {
        if self.blocks.len() <= self.capacity {
            return Vec::new();
        }
        let over = self.blocks.len() - self.capacity;
        self.blocks.drain(..over).map(|block| block.id).collect()
    }

    /// A code Block stops changing the moment its fence closes — that is when
    /// highlighting it is worth doing.
    fn ask_to_highlight(&self, id: BlockId) {
        let Some(block) = self.blocks.iter().find(|block| block.id == id) else {
            return;
        };
        let Body::Code {
            language, source, ..
        } = &block.body
        else {
            return;
        };
        self.highlighter.request(HighlightRequest {
            block: id,
            language: language.clone(),
            source: source.clone(),
        });
    }

    fn highlight(&mut self, id: BlockId, answer: Vec<Token>) -> Option<BlockId> {
        let block = self.blocks.iter_mut().find(|block| block.id == id)?;
        let Body::Code { tokens, .. } = &mut block.body else {
            return None;
        };
        *tokens = Some(answer);
        Some(id)
    }

    /// A result lands on the row that started the call — which is rarely the
    /// tail by the time it arrives.
    fn settle_tool(
        &mut self,
        call: &str,
        state: ToolState,
        diffs: Vec<Diff>,
        structured_result: Option<serde_json::Value>,
        result_line: Option<String>,
        output: Option<ToolOutput>,
    ) -> Option<BlockId> {
        let block = self
            .blocks
            .iter_mut()
            .find(|block| matches!(&block.body, Body::Tool(tool) if tool.call == call))?;
        let Body::Tool(tool) = &mut block.body else {
            return None;
        };
        if tool.state == state
            && tool.diffs == diffs
            && tool.structured_result == structured_result
            && tool.result_line == result_line
            && tool.output == output
        {
            return None;
        }
        let was_running = tool.state == ToolState::Running;
        tool.state = state;
        tool.diffs = diffs;
        tool.structured_result = structured_result;
        if result_line.is_some() || !was_running {
            tool.result_line = result_line;
        }
        if output.is_some() || !was_running {
            tool.output = output;
        }
        Some(block.id)
    }

    /// Append a Block that no further text can join.
    fn push(&mut self, body: Body) -> BlockId {
        self.thinking_open = false;
        let id = self.mint();
        self.blocks.push(Block {
            id,
            body,
            markdown: None,
            markdown_run: None,
            sent_at: None,
        });
        self.open = None;
        self.source.clear();
        id
    }

    fn mint(&mut self) -> BlockId {
        self.last_id += 1;
        BlockId(self.last_id)
    }

    fn advance_revision(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }
}

/// How much of a tool failure a row carries; the model got all of it.
const ERROR_CHARS: usize = 200;

/// How much of a tool's output its `⎿` continuation row carries: one line
/// the row cuts by width, never wrapping.
const RESULT_CHARS: usize = crate::progress::ROW_CHARS;

const TOOL_OUTPUT_BYTES: usize = 64 * 1024;

fn retained_output(output: &str) -> Option<ToolOutput> {
    if output.trim().is_empty() {
        return None;
    }
    let mut end = output.len().min(TOOL_OUTPUT_BYTES);
    while !output.is_char_boundary(end) {
        end -= 1;
    }
    Some(ToolOutput {
        text: output[..end].to_string(),
        omitted_bytes: output.len() - end,
    })
}

/// Cut to `limit` characters, marking the cut.
fn trim(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit).chain(['…']).collect()
}

/// The one line a settled tool row keeps of its output — the first
/// non-blank line, trimmed. Whitespace-only output keeps nothing.
fn result_line(output: &str) -> Option<String> {
    let line = output.lines().find(|line| !line.trim().is_empty())?;
    Some(trim(line.trim_end(), RESULT_CHARS))
}

/// `1 line` / `412 lines`.
fn counted(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// What a read or a search found, as Claude Code's own rows summarize it:
/// a Read's `412 lines` (its result's `file.numLines`, else the lines it
/// printed); a Grep's `6 matches in 2 files` (content mode), `2 files`
/// (files-with-matches) or `6 matches` (count). `None` for any other tool,
/// which keeps its first output line.
fn result_summary(
    name: &str,
    structured: Option<&serde_json::Value>,
    output: &str,
) -> Option<String> {
    let number = |value: &serde_json::Value, key: &str| value.get(key).and_then(|v| v.as_u64());
    match name {
        "Read" | "read_file" => {
            let lines = structured
                .and_then(|value| value.get("file"))
                .and_then(|file| number(file, "numLines"))
                .or_else(|| {
                    // Whitespace alone says nothing worth a row.
                    let count = output.lines().count() as u64;
                    (count > 0 && !output.trim().is_empty()).then_some(count)
                })?;
            Some(counted(lines, "line", "lines"))
        }
        "Grep" => {
            let value = structured?;
            let filenames = value
                .get("filenames")
                .and_then(|names| names.as_array())
                .map(|names| names.len() as u64);
            let content = value.get("content").and_then(|content| content.as_str());
            let files = number(value, "numFiles")
                .filter(|files| *files > 0)
                .or(filenames.filter(|files| *files > 0))
                .or_else(|| {
                    // Content lines are `path:line:text`; distinct paths
                    // are the files that matched.
                    let paths: std::collections::BTreeSet<&str> = content?
                        .lines()
                        .filter_map(|line| line.split_once(':').map(|(path, _)| path))
                        .collect();
                    (!paths.is_empty()).then_some(paths.len() as u64)
                })
                .unwrap_or(0);
            match value.get("mode").and_then(|mode| mode.as_str()) {
                Some("files_with_matches") => Some(counted(files, "file", "files")),
                Some("count") => {
                    let matches = number(value, "numMatches").or_else(|| {
                        content.map(|content| {
                            content
                                .lines()
                                .filter_map(|line| line.rsplit_once(':'))
                                .filter_map(|(_, count)| count.trim().parse::<u64>().ok())
                                .sum()
                        })
                    })?;
                    Some(counted(matches, "match", "matches"))
                }
                Some("content") | None => {
                    let matches = number(value, "numMatches")
                        .or_else(|| number(value, "numLines"))
                        .or_else(|| {
                            content.map(|content| {
                                content
                                    .lines()
                                    .filter(|line| !line.trim().is_empty())
                                    .count() as u64
                            })
                        })?;
                    Some(format!(
                        "{} in {}",
                        counted(matches, "match", "matches"),
                        counted(files.max(u64::from(matches > 0)), "file", "files")
                    ))
                }
                Some(_) => None,
            }
        }
        _ => None,
    }
}

/// Whether two spellings of a path name one file: equal, or one is the
/// other's tail at a path boundary (`crates/x.rs` and `/repo/crates/x.rs`).
fn same_file(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim_start_matches("./"), b.trim_start_matches("./"));
    if a == b {
        return true;
    }
    let (long, short) = if a.len() > b.len() { (a, b) } else { (b, a) };
    !short.is_empty()
        && long.ends_with(short)
        && long.as_bytes()[long.len() - short.len() - 1] == b'/'
}

/// A diff's first changed line, on the new side: the line a hover card
/// opens on.
/// Where an edit landed, as the hover card opens on it (R10): its first
/// hunk's new-side start — `Updated nav.rs at 211`, the prototype's `:211`.
fn first_changed_line(diff: &Diff) -> Option<u32> {
    let hunk = diff.hunks.first()?;
    Some(hunk.new_start.max(1))
}

/// The one line a collapsed tool row shows. Tool inputs are the vendor's own
/// schema, so this reads the few keys that name a subject and gives up
/// quietly on anything else rather than guessing. A search names what it
/// looks for, then where: `Grep(status_line, crates/ferrite/src)`.
fn tool_summary(name: &str, input: &serde_json::Value) -> String {
    if matches!(name, "Grep" | "Glob") {
        if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
            return match input.get("path").and_then(|v| v.as_str()) {
                Some(path) if !path.is_empty() => format!("{pattern}, {path}"),
                _ => pattern.to_string(),
            };
        }
    }
    tool_subject(input)
}

fn tool_subject(input: &serde_json::Value) -> String {
    for key in [
        "command",
        "file_path",
        "path",
        "pattern",
        "url",
        "subject",
        "query",
        "description",
        "prompt",
    ] {
        if let Some(value) = input.get(key).and_then(|v| v.as_str()) {
            return value.to_string();
        }
    }
    if let Some(args) = input.get("arguments").filter(|value| value.is_object()) {
        return tool_subject(args);
    }
    String::new()
}

/// How many bytes of `source` form a section that can no longer grow — or
/// None while the leading section is still open to more text.
fn complete_section(source: &str) -> Option<usize> {
    let first_end = source.find('\n')?;
    let first = &source[..first_end];
    if fence(first).is_some() {
        // A fenced block runs to its closing fence, blank lines included.
        let mut pos = first_end + 1;
        loop {
            let end = pos + source[pos..].find('\n')?;
            if fence(&source[pos..end]).is_some() {
                return Some(end + 1);
            }
            pos = end + 1;
        }
    }
    if first.trim().is_empty() || heading(first).is_some() || bullet(first).is_some() {
        return Some(first_end + 1);
    }
    // A paragraph runs until a line that cannot join it.
    let mut pos = first_end + 1;
    loop {
        let end = pos + source[pos..].find('\n')?;
        let line = &source[pos..end];
        if line.trim().is_empty()
            || heading(line).is_some()
            || bullet(line).is_some()
            || fence(line).is_some()
        {
            return Some(pos);
        }
        pos = end + 1;
    }
}

/// Fold one section of markdown into a Block body. Blank sections have none.
fn parse_section(source: &str) -> Option<Body> {
    if let Some(body) = parse_code(source) {
        return Some(body);
    }
    let source = source.trim();
    if source.is_empty() {
        return None;
    }
    if let Some((level, rest)) = heading(source) {
        return Some(Body::Heading {
            level,
            spans: spans(rest),
        });
    }
    if let Some(item) = bullet(source) {
        return Some(Body::Bullet { spans: spans(item) });
    }
    Some(Body::Paragraph {
        spans: spans(source),
    })
}

/// A fenced block, complete or still streaming. Its source keeps every inner
/// line untouched — indentation is code, not decoration.
fn parse_code(source: &str) -> Option<Body> {
    let mut lines = source.lines();
    let info = fence(lines.next()?)?;
    let mut body = Vec::new();
    for line in lines {
        if fence(line).is_some() {
            break;
        }
        body.push(line);
    }
    Some(Body::Code {
        language: (!info.is_empty()).then(|| info.to_string()),
        source: body.join("\n"),
        tokens: None,
    })
}

/// ```` ```rust ```` -> "rust"; a bare ```` ``` ```` -> "".
fn fence(line: &str) -> Option<&str> {
    Some(line.trim_end().strip_prefix("```")?.trim())
}

/// `- one` or `* one` -> "one".
fn bullet(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))?;
    Some(rest.trim())
}

/// `## Plan` -> (2, "Plan").
fn heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.len() - line.trim_start_matches('#').len();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &line[hashes..];
    let trimmed = rest.strip_prefix(' ')?;
    Some((hashes as u8, trimmed.trim()))
}

/// Fold a line into spans. `**bold**` is found first, over the whole line
/// — a bold run may hold inline code (`**drop the `MARK` lines**`), and
/// splitting on backticks first left its markers literal. Backticked code
/// is then cut out of each run, bold or plain; the plain runs split once
/// more on `[text](url)`. A code span's own `**` never opens bold: the
/// scan steps over code whole.
fn spans(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut plain = 0;
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        if let Some(code) = rest.strip_prefix('`') {
            // Step over a closed code span; an unclosed backtick is text.
            at += match code.find('`') {
                Some(end) => end + 2,
                None => 1,
            };
            continue;
        }
        if let Some((body, used)) = bold_at(rest) {
            code_split(&text[plain..at], Style::Plain, &mut spans);
            code_split(body, Style::Bold, &mut spans);
            at += used;
            plain = at;
            continue;
        }
        at += rest.chars().next().map_or(1, char::len_utf8);
    }
    code_split(&text[plain..], Style::Plain, &mut spans);
    spans
}

/// Cut the backticked code out of one run: the odd pieces are Code, the
/// even ones wear `style` — and a plain piece still folds its links.
fn code_split(text: &str, style: Style, spans: &mut Vec<Span>) {
    for (i, run) in text.split('`').enumerate() {
        if run.is_empty() {
            continue;
        }
        if i % 2 == 1 {
            spans.push(Span {
                text: run.to_string(),
                style: Style::Code,
            });
        } else if style == Style::Plain {
            styled(run, spans);
        } else {
            spans.push(Span {
                text: run.to_string(),
                style,
            });
        }
    }
}

/// Fold one plain run into Plain/Bold/Link spans. An unclosed marker stays
/// literal text — prose is never eaten on a guess.
fn styled(text: &str, spans: &mut Vec<Span>) {
    fn push(spans: &mut Vec<Span>, text: &str, style: Style) {
        if !text.is_empty() {
            spans.push(Span {
                text: text.to_string(),
                style,
            });
        }
    }
    let mut plain = 0;
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let styled = bold_at(rest)
            .map(|(body, used)| (body, used, Style::Bold))
            .or_else(|| link_at(rest).map(|(body, used)| (body, used, Style::Link)));
        if let Some((body, used, style)) = styled {
            push(spans, &text[plain..at], Style::Plain);
            push(spans, body, style);
            at += used;
            plain = at;
            continue;
        }
        at += rest.chars().next().map_or(1, char::len_utf8);
    }
    push(spans, &text[plain..], Style::Plain);
}

/// `**bold** …` → ("bold", bytes consumed).
fn bold_at(rest: &str) -> Option<(&str, usize)> {
    let body = rest.strip_prefix("**")?;
    let end = body.find("**")?;
    (end > 0).then_some((&body[..end], end + 4))
}

/// `[text](url) …` → ("text", bytes consumed). Only the text survives.
fn link_at(rest: &str) -> Option<(&str, usize)> {
    let body = rest.strip_prefix('[')?;
    let close = body.find("](")?;
    let target = &body[close + 2..];
    let end = target.find(')')?;
    (close > 0).then_some((&body[..close], 1 + close + 2 + end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Decision;

    /// CT-10/11: every call is its own Block under its own name — adjacent
    /// calls are never folded into a summary — and the names read as Claude
    /// Code prints them.
    #[test]
    fn every_call_keeps_its_own_block_and_display_name() {
        let mut transcript = Transcript::default();
        transcript.apply(text("Checking the files."));
        for (id, name) in [
            ("read", "Read"),
            ("mcp", "mcp__docs__search"),
            ("edit", "Edit"),
            ("shell", "Bash"),
        ] {
            transcript.apply(started(
                id,
                name,
                serde_json::json!({"file_path": "src/main.rs"}),
            ));
        }
        let tools: Vec<_> = transcript
            .blocks()
            .iter()
            .filter_map(|block| match &block.body {
                Body::Tool(tool) => Some(tool.call.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(tools, ["read", "mcp", "edit", "shell"]);
        for (name, shown) in [
            ("Edit", "Update"),
            ("MultiEdit", "Update"),
            ("fileChange", "Update"),
            ("commandExecution", "Bash"),
            ("Write", "Write"),
            ("Read", "Read"),
            ("mcp__docs__search", "mcp__docs__search"),
        ] {
            assert_eq!(display_tool_name(name), shown, "{name}");
        }
    }

    /// CT-12: a search names its pattern, then where it looked.
    #[test]
    fn a_search_names_its_pattern_then_its_path() {
        for (name, input, summary) in [
            (
                "Grep",
                serde_json::json!({"pattern": "status_line", "path": "crates/ferrite/src"}),
                "status_line, crates/ferrite/src",
            ),
            (
                "Grep",
                serde_json::json!({"pattern": "status_line"}),
                "status_line",
            ),
            (
                "Glob",
                serde_json::json!({"pattern": "**/*.rs", "path": "crates"}),
                "**/*.rs, crates",
            ),
            (
                "Read",
                serde_json::json!({"file_path": "crates/ferrite/src/nav.rs"}),
                "crates/ferrite/src/nav.rs",
            ),
        ] {
            assert_eq!(tool_summary(name, &input), summary, "{name} {input}");
        }
    }

    /// CT-13: a Read reports its line count, a Grep its matches and files.
    #[test]
    fn reads_and_searches_summarize_what_they_found() {
        let read =
            serde_json::json!({"type": "text", "file": {"filePath": "/w/nav.rs", "numLines": 412}});
        assert_eq!(
            result_summary("Read", Some(&read), "ignored").as_deref(),
            Some("412 lines")
        );
        assert_eq!(
            result_summary("Read", None, "     1\u{2192}a\n     2\u{2192}b").as_deref(),
            Some("2 lines")
        );
        let content = serde_json::json!({"mode": "content", "numFiles": 2, "filenames": [], "content": "a.rs:1:x\na.rs:9:x\nb.rs:3:x\nb.rs:4:x\nb.rs:5:x\nb.rs:6:x", "numLines": 6});
        assert_eq!(
            result_summary("Grep", Some(&content), "").as_deref(),
            Some("6 matches in 2 files")
        );
        let counted_files =
            serde_json::json!({"mode": "content", "numFiles": 0, "content": "a.rs:1:x\nb.rs:3:x"});
        assert_eq!(
            result_summary("Grep", Some(&counted_files), "").as_deref(),
            Some("2 matches in 2 files")
        );
        let files = serde_json::json!({"mode": "files_with_matches", "filenames": ["a.rs", "b.rs"], "numFiles": 2});
        assert_eq!(
            result_summary("Grep", Some(&files), "").as_deref(),
            Some("2 files")
        );
        let count =
            serde_json::json!({"mode": "count", "numFiles": 2, "content": "a.rs:2\nb.rs:4"});
        assert_eq!(
            result_summary("Grep", Some(&count), "").as_deref(),
            Some("6 matches")
        );
        assert_eq!(result_summary("Bash", None, "anything"), None);

        // Through the fold: the row's `└` line.
        let mut transcript = Transcript::default();
        transcript.apply(started(
            "r",
            "Read",
            serde_json::json!({"file_path": "crates/ferrite/src/canvas.rs"}),
        ));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "r".into(),
            output: "     1\u{2192}fn main() {}".into(),
            is_error: false,
            result: crate::ToolResult::Structured {
                value: serde_json::json!({"type": "text", "file": {"numLines": 640}}),
                duration_ms: None,
            },
        }));
        let Body::Tool(tool) = &transcript.blocks()[0].body else {
            panic!("a tool row")
        };
        assert_eq!(tool.result_line.as_deref(), Some("640 lines"));
    }

    #[test]
    fn a_command_head_stops_at_its_first_option() {
        assert_eq!(
            command_head("gh issue close 212 --reason \"not planned\""),
            "gh issue close 212"
        );
        assert_eq!(command_head("cargo test -p ferrite nav::"), "cargo test");
        assert_eq!(command_head("  ls  "), "ls");
        assert_eq!(command_head("--help"), "");
    }

    /// CT-20: a hunk's section is its header's tail, trimmed to the item's
    /// head, or git's funcname rule over its leading context.
    #[test]
    fn a_hunk_names_its_enclosing_section() {
        assert_eq!(
            section_head("fn thread_row(&self, t: &ThreadRow, cx: &App) -> Div {").as_deref(),
            Some("fn thread_row")
        );
        assert_eq!(
            section_head("impl Canvas {").as_deref(),
            Some("impl Canvas")
        );
        assert_eq!(section_head("  ").as_deref(), None);
        let lines = |lines: &[&str]| {
            lines
                .iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            hunk_section(&lines(&[
                " fn thread_row(&self, t: &ThreadRow, cx: &App) -> Div {",
                "     let facts = self.facts.get(&t.id);",
                "-    let mut row = div();",
                "+    let live = t.session.is_some();",
            ]))
            .as_deref(),
            Some("fn thread_row")
        );
        // Indented context names no item; a change with no context neither.
        assert_eq!(
            hunk_section(&lines(&["     let facts = 1;", "-a", "+b"])),
            None
        );
        assert_eq!(
            hunk_section(&lines(&["-pub const RUNNING: u32 = 0x7fbf95;", "+x"])),
            None
        );
    }

    /// CT-24: the turn's stamp carries its tokens; a settled span past a
    /// minute reads `3m 12s`.
    #[test]
    fn the_turn_stamp_reads_its_time_and_tokens() {
        let end = TurnEnd {
            outcome: crate::TurnOutcome::Completed,
            elapsed_ms: Some(192_000),
            completed_at: Some("7:33 pm".into()),
            input_tokens: None,
            output_tokens: None,
        };
        assert_eq!(end.text(), "Worked for 3m 12s \u{b7} 7:33 pm");
        let end = TurnEnd {
            elapsed_ms: Some(41_000),
            completed_at: Some("7:32 pm".into()),
            input_tokens: Some(3_200),
            output_tokens: Some(1_100),
            ..end
        };
        assert_eq!(
            end.text(),
            "Worked for 41s \u{b7} 7:32 pm \u{b7} \u{2191} 3.2k \u{2193} 1.1k"
        );
        let end = TurnEnd {
            elapsed_ms: Some(12_000),
            completed_at: Some("7:19 pm".into()),
            input_tokens: Some(1_800),
            output_tokens: Some(620),
            ..end
        };
        assert_eq!(
            end.text(),
            "Worked for 12s \u{b7} 7:19 pm \u{b7} \u{2191} 1.8k \u{2193} 620"
        );
    }

    /// CT-3: a prompt's send time rides the observation recorded after it;
    /// a prompt with none draws no time, never a fresh clock.
    #[test]
    fn a_prompt_takes_its_send_time_from_the_observation() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("first".into()));
        assert_eq!(transcript.blocks()[0].sent_at, None);
        let update = transcript.apply(Input::PromptObservation {
            sent_at: "7:18 pm".into(),
        });
        assert_eq!(update.dirty, vec![transcript.blocks()[0].id]);
        assert_eq!(transcript.blocks()[0].sent_at.as_deref(), Some("7:18 pm"));
        assert_eq!(transcript.started_at(), Some("7:18 pm"));
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        }));
        transcript.apply(Input::CompletionObservation {
            elapsed_ms: 12_000,
            completed_at: "7:19 pm".into(),
            input_tokens: Some(1_800),
            output_tokens: Some(620),
        });
        transcript.apply(Input::Prompt("second".into()));
        transcript.apply(Input::PromptObservation {
            sent_at: "7:31 pm".into(),
        });
        let prompts: Vec<_> = transcript
            .blocks()
            .iter()
            .filter(|block| matches!(block.body, Body::Prompt(_)))
            .map(|block| block.sent_at.as_deref())
            .collect();
        assert_eq!(prompts, [Some("7:18 pm"), Some("7:31 pm")]);
        assert_eq!(transcript.started_at(), Some("7:18 pm"));
        assert_eq!(transcript.turn_count(), 1);
        assert_eq!(transcript.last_working_ms(), Some(12_000));
    }

    /// CT-25: a streamed cargo run counts its results against the total
    /// its binaries announce.
    #[test]
    fn a_streaming_test_run_counts_its_results() {
        let mut transcript = Transcript::default();
        transcript.apply(started(
            "t",
            "Bash",
            serde_json::json!({"command": "cargo test --workspace"}),
        ));
        let delta = |text: &str| {
            Input::Event(SessionEvent::ToolOutputDelta {
                id: "t".into(),
                text: text.into(),
            })
        };
        let progress = |transcript: &Transcript| match &transcript.blocks()[0].body {
            Body::Tool(tool) => tool.progress,
            _ => None,
        };
        transcript.apply(delta("   Compiling ferrite v0.4.0\n"));
        assert_eq!(progress(&transcript), None, "no total yet");
        transcript.apply(delta("\nrunning 300 tests\ntest a ... ok\ntest b ... FAI"));
        assert_eq!(
            progress(&transcript),
            Some(TestProgress {
                done: 1,
                total: 300
            })
        );
        transcript.apply(delta("LED\ntest c ... ignored, slow\n\nrunning 57 tests\n"));
        assert_eq!(
            progress(&transcript),
            Some(TestProgress {
                done: 3,
                total: 357
            })
        );
        // A command that is not a test run counts nothing.
        let mut other = Transcript::default();
        other.apply(started("t", "Bash", serde_json::json!({"command": "ls"})));
        other.apply(delta("running 3 tests\ntest a ... ok\n"));
        assert_eq!(progress(&other), None);

        let mut counter = TestCounter::default();
        counter.push(
            "collected 4 items\n\ntests/test_a.py ..F\ntests/test_b.py::test_x PASSED [100%]\n",
        );
        assert_eq!(counter.progress(), Some(TestProgress { done: 4, total: 4 }));
        let mut go = TestCounter::default();
        go.push("=== RUN   TestA\n--- PASS: TestA (0.00s)\n=== RUN   TestB\n");
        assert_eq!(go.progress(), Some(TestProgress { done: 1, total: 2 }));
    }

    /// CT-31: what a Thread did to a file, for the hover card.
    #[test]
    fn a_file_stat_folds_every_edit_to_one_file() {
        let mut transcript = Transcript::default();
        for (id, start, lines) in [
            ("e1", 208u32, vec![" fn a() {", "-old", "+new", "+more"]),
            ("e2", 210, vec!["     ctx", "+added"]),
        ] {
            transcript.apply(started(
                id,
                "Edit",
                serde_json::json!({"file_path": "/repo/crates/ferrite/src/nav.rs"}),
            ));
            transcript.note_settled_at(id, std::time::SystemTime::UNIX_EPOCH);
            transcript.apply(Input::Event(SessionEvent::ToolCompleted {
                id: id.into(),
                output: String::new(),
                is_error: false,
                result: crate::ToolResult::FileEdit {
                    path: "/repo/crates/ferrite/src/nav.rs".into(),
                    hunks: vec![crate::Hunk {
                        old_start: start,
                        old_lines: 1,
                        new_start: start,
                        new_lines: 1,
                        lines: lines.into_iter().map(String::from).collect(),
                        section: None,
                    }],
                },
            }));
        }
        let stat = transcript
            .file_stat("crates/ferrite/src/nav.rs")
            .expect("the file was changed");
        assert_eq!((stat.added, stat.removed), (3, 1));
        assert_eq!(stat.last_change_line, Some(210));
        assert_eq!(stat.at, Some(std::time::SystemTime::UNIX_EPOCH));
        assert_eq!(transcript.file_stat("crates/ferrite/src/facts.rs"), None);
        assert_eq!(transcript.file_stat("nav.rs").map(|s| s.added), Some(3));
        assert!(!same_file("/repo/xnav.rs", "nav.rs"));
    }

    fn started(id: &str, name: &str, input: serde_json::Value) -> Input {
        Input::Event(SessionEvent::ToolStarted {
            id: id.into(),
            name: name.into(),
            input,
        })
    }

    fn completed(id: &str, output: &str, is_error: bool) -> Input {
        Input::Event(SessionEvent::ToolCompleted {
            id: id.into(),
            output: output.into(),
            is_error,
            result: crate::ToolResult::Opaque,
        })
    }

    fn text(s: &str) -> Input {
        Input::Event(SessionEvent::TextDelta { text: s.into() })
    }

    fn body_text(block: &Block) -> String {
        match &block.body {
            Body::Paragraph { spans } | Body::Heading { spans, .. } | Body::Bullet { spans } => {
                spans.iter().map(|s| s.text.as_str()).collect()
            }
            Body::Code { source, .. } => source.clone(),
            Body::Tool(tool) => tool.summary.clone(),
            Body::Prompt(line) => line.clone(),
            Body::Thinking(thought) => thought.clone(),
            Body::Notice(text) | Body::Meta(text) => text.clone(),
            Body::TurnEnd(end) => end.text(),
        }
    }

    #[test]
    fn streamed_text_folds_into_one_growing_paragraph() {
        let mut transcript = Transcript::default();

        let first = transcript.apply(text("Reading "));
        let second = transcript.apply(text("the composer."));

        assert_eq!(transcript.blocks().len(), 1);
        assert_eq!(first.dirty.len(), 1);
        assert_eq!(first.dirty, second.dirty); // the same block grew
        assert_eq!(body_text(&transcript.blocks()[0]), "Reading the composer.");
    }

    #[test]
    fn presentation_revision_tracks_changes_without_reminting_blocks() {
        let mut transcript = Transcript::default();
        assert_eq!(transcript.revision(), 0);
        let _ = (transcript.blocks(), transcript.status(), transcript.usage());
        assert_eq!(transcript.revision(), 0);

        transcript.apply(text("Reading"));
        let id = transcript.blocks()[0].id;
        assert_eq!(transcript.revision(), 1);

        transcript.apply(Input::Event(SessionEvent::TokenUsage {
            total_tokens: 12,
            input_tokens: 8,
            cached_input_tokens: 0,
            output_tokens: 4,
            reasoning_output_tokens: 0,
            context_window: Some(128),
        }));

        assert_eq!(transcript.revision(), 2);
        assert_eq!(transcript.blocks()[0].id, id);
    }

    #[test]
    fn a_blank_line_starts_a_new_paragraph() {
        let mut transcript = Transcript::default();
        transcript.apply(text("first para"));

        // Deltas split words and newlines wherever the provider felt like it.
        transcript.apply(text("\n"));
        let update = transcript.apply(text("\nsecond para"));

        assert_eq!(transcript.blocks().len(), 2);
        assert_eq!(body_text(&transcript.blocks()[0]), "first para");
        assert_eq!(body_text(&transcript.blocks()[1]), "second para");
        assert_eq!(
            update.dirty,
            vec![transcript.blocks()[0].id, transcript.blocks()[1].id]
        );
    }

    #[test]
    fn a_heading_is_its_own_block_without_a_blank_line() {
        let mut transcript = Transcript::default();

        transcript.apply(text("## Plan\nfirst step"));

        assert_eq!(transcript.blocks().len(), 2);
        assert!(matches!(
            transcript.blocks()[0].body,
            Body::Heading { level: 2, .. }
        ));
        assert_eq!(body_text(&transcript.blocks()[0]), "Plan");
        assert_eq!(body_text(&transcript.blocks()[1]), "first step");
    }

    #[test]
    fn each_bullet_is_its_own_block_and_ends_the_paragraph_above_it() {
        let mut transcript = Transcript::default();

        transcript.apply(text("what I found:\n- one\n- two\nback to prose"));

        let bodies: Vec<&Body> = transcript.blocks().iter().map(|b| &b.body).collect();
        assert!(matches!(bodies[0], Body::Paragraph { .. }));
        assert!(matches!(bodies[1], Body::Bullet { .. }));
        assert!(matches!(bodies[2], Body::Bullet { .. }));
        assert!(matches!(bodies[3], Body::Paragraph { .. }));
        assert_eq!(body_text(&transcript.blocks()[1]), "one");
        assert_eq!(body_text(&transcript.blocks()[3]), "back to prose");
    }

    #[test]
    fn rich_markdown_source_survives_stream_chunk_boundaries() {
        let source = "# Result\n\n1. first\n   - nested **bold**\n2. second\n\n| Name | Value |\n| --- | --- |\n| café | [link](https://example.com) |\n\n```html\n<h2>Preview</h2>\n<p>Exact text</p>\n```\n\nlast\n\n";
        for chunks in [
            vec![source.to_string()],
            source.chars().map(|ch| ch.to_string()).collect(),
        ] {
            let mut transcript = Transcript::default();
            for chunk in chunks {
                transcript.apply(text(&chunk));
            }
            let restored: String = transcript
                .blocks()
                .iter()
                .filter_map(|block| block.markdown.as_deref())
                .collect();
            assert_eq!(
                restored, source,
                "rich rendering needs the original structure and URLs"
            );
        }
    }

    #[test]
    fn markdown_run_identity_survives_core_eviction_and_changes_at_boundaries() {
        let mut transcript = Transcript::with_capacity(Arc::new(Unhighlighted), 2);
        transcript.apply(text("first\n\n"));
        let original = transcript.blocks()[0].markdown_run;
        for index in 0..8 {
            transcript.apply(text(&format!("paragraph {index}\n\n")));
            assert!(transcript
                .blocks()
                .iter()
                .all(|block| block.markdown_run == original));
        }
        assert!(transcript
            .blocks()
            .iter()
            .all(|block| Some(block.id) != original));
        transcript.apply(Input::Prompt("next answer".into()));
        transcript.apply(text("new\n\n"));
        assert_ne!(transcript.blocks().last().unwrap().markdown_run, original);
    }

    #[test]
    fn a_fenced_block_becomes_code_not_prose() {
        let mut transcript = Transcript::default();

        transcript.apply(text(
            "run this:\n```rust\nfn main() {\n    ok();\n}\n```\nthen go",
        ));

        assert_eq!(transcript.blocks().len(), 3);
        assert!(matches!(
            transcript.blocks()[0].body,
            Body::Paragraph { .. }
        ));
        match &transcript.blocks()[1].body {
            Body::Code {
                language, source, ..
            } => {
                assert_eq!(language.as_deref(), Some("rust"));
                assert_eq!(source, "fn main() {\n    ok();\n}");
            }
            other => panic!("expected code, got {other:?}"),
        }
        assert!(matches!(
            transcript.blocks()[2].body,
            Body::Paragraph { .. }
        ));
    }

    #[test]
    fn inline_code_is_its_own_span() {
        let mut transcript = Transcript::default();

        transcript.apply(text("run `cargo test` now"));

        match &transcript.blocks()[0].body {
            Body::Paragraph { spans } => {
                assert_eq!(
                    spans,
                    &[
                        Span {
                            text: "run ".into(),
                            style: Style::Plain
                        },
                        Span {
                            text: "cargo test".into(),
                            style: Style::Code
                        },
                        Span {
                            text: " now".into(),
                            style: Style::Plain
                        },
                    ]
                );
            }
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }

    /// #22 C13: `**bold**` and `[text](url)` fold to their own styles; a
    /// link keeps only its text, and an unclosed marker stays literal.
    #[test]
    fn bold_and_links_are_their_own_spans() {
        let mut transcript = Transcript::default();
        transcript.apply(text(
            "keep **stable identities** per [the fold](https://example.com) — a ** stray stays",
        ));
        match &transcript.blocks()[0].body {
            Body::Paragraph { spans } => {
                assert_eq!(
                    spans,
                    &[
                        Span {
                            text: "keep ".into(),
                            style: Style::Plain
                        },
                        Span {
                            text: "stable identities".into(),
                            style: Style::Bold
                        },
                        Span {
                            text: " per ".into(),
                            style: Style::Plain
                        },
                        Span {
                            text: "the fold".into(),
                            style: Style::Link
                        },
                        Span {
                            text: " — a ** stray stays".into(),
                            style: Style::Plain
                        },
                    ]
                );
            }
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }

    #[test]
    fn a_tool_call_becomes_a_row_naming_what_it_touched() {
        let mut transcript = Transcript::default();

        transcript.apply(started(
            "toolu_1",
            "Read",
            serde_json::json!({ "file_path": "/workspace/CONTEXT.md" }),
        ));

        match &transcript.blocks()[0].body {
            Body::Tool(tool) => {
                assert_eq!(tool.name, "Read");
                assert_eq!(tool.summary, "/workspace/CONTEXT.md");
                assert_eq!(tool.state, ToolState::Running);
            }
            other => panic!("expected a tool row, got {other:?}"),
        }

        // A planning call's subject is a subject too — TaskCreate(land the
        // diff) reads like every other row, never a bare bold name.
        transcript.apply(started(
            "toolu_2",
            "TaskCreate",
            serde_json::json!({ "subject": "land the diff" }),
        ));
        match &transcript.blocks()[1].body {
            Body::Tool(tool) => assert_eq!(tool.summary, "land the diff"),
            other => panic!("expected a tool row, got {other:?}"),
        }
    }

    #[test]
    fn a_tool_result_mutates_its_own_row_long_after_the_tail_moved_on() {
        let mut transcript = Transcript::default();
        transcript.apply(started(
            "toolu_1",
            "Bash",
            serde_json::json!({ "command": "cargo test" }),
        ));
        transcript.apply(text("running the suite\n\nwhile that goes"));
        let row = transcript.blocks()[0].id;
        let tail = transcript.blocks().last().unwrap().id;
        assert_ne!(row, tail);

        let update = transcript.apply(completed("toolu_1", "42 passed", false));

        assert_eq!(update.dirty, vec![row]);
        match &transcript.blocks()[0].body {
            Body::Tool(tool) => assert_eq!(tool.state, ToolState::Ok),
            other => panic!("expected a tool row, got {other:?}"),
        }
        assert_eq!(transcript.blocks().last().unwrap().id, tail);
    }

    #[test]
    fn a_prompt_is_echoed_as_its_own_block_and_starts_the_turn() {
        let mut transcript = Transcript::default();
        transcript.apply(text("an earlier answer"));
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        }));
        assert_eq!(transcript.status(), Status::Idle);

        let update = transcript.apply(Input::Prompt("run the tests".into()));

        let echo = transcript.blocks().last().unwrap();
        assert_eq!(update.dirty, vec![echo.id]);
        assert!(matches!(echo.body, Body::Prompt(_)));
        assert_eq!(body_text(echo), "run the tests");
        // The turn is under way from here, not from the first delta.
        assert_eq!(transcript.status(), Status::Streaming);
    }

    #[test]
    fn init_names_the_session_the_header_shows() {
        let mut transcript = Transcript::default();
        assert_eq!(transcript.model(), None);

        transcript.apply(Input::Event(SessionEvent::Init {
            session_id: "4f2a1c9e-7b30".into(),
            model: "claude-sonnet-4-5".into(),
        }));

        assert_eq!(transcript.model(), Some("claude-sonnet-4-5"));
        assert_eq!(transcript.session_id(), Some("4f2a1c9e-7b30"));
        assert!(transcript.blocks().is_empty()); // identity is not content
    }

    #[test]
    fn a_closed_session_says_why_and_stops() {
        let mut transcript = Transcript::default();
        transcript.apply(text("mid-answer"));

        let update = transcript.apply(Input::Event(SessionEvent::Closed {
            reason: "claude CLI exited with code 1".into(),
        }));

        assert_eq!(transcript.status(), Status::Closed);
        assert_eq!(update.boundary, Some(Boundary::Closed));
        let last = transcript.blocks().last().unwrap();
        assert_eq!(update.dirty, vec![last.id]);
        assert!(matches!(last.body, Body::Notice(_)));
        assert_eq!(body_text(last), "claude CLI exited with code 1");
    }

    /// The cost is data, never a row: `last_cost` records it and no dollar
    /// value reaches the transcript (#22 operator amendment).
    #[test]
    fn a_paid_turn_records_its_cost_without_rendering_it() {
        let mut transcript = Transcript::default();
        transcript.apply(text("done"));
        let before = transcript.blocks().len();

        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: Some(0.038),
        }));

        assert_eq!(transcript.last_cost(), Some(0.038));
        assert_eq!(
            transcript.blocks().len(),
            before,
            "a completed turn ends without a row"
        );
    }

    #[test]
    fn turn_completion_reads_the_outcome_independently_of_cost() {
        for cost_usd in [None, Some(0.038)] {
            for outcome in [
                TurnOutcome::Completed,
                TurnOutcome::Interrupted,
                TurnOutcome::Error("model overloaded".into()),
            ] {
                let mut transcript = Transcript::default();
                assert_eq!(transcript.turn_outcome(), None);
                assert!(!transcript.turn_completed());
                transcript.apply(Input::Prompt("go".into()));
                let update = transcript.apply(Input::Event(SessionEvent::TurnEnded {
                    outcome: outcome.clone(),
                    cost_usd,
                }));

                assert_eq!(transcript.turn_outcome(), Some(&outcome));
                assert_eq!(
                    transcript.turn_completed(),
                    outcome == TurnOutcome::Completed,
                    "{outcome:?}, cost {cost_usd:?}"
                );
                assert_eq!(transcript.last_cost(), cost_usd);
                assert_eq!(transcript.status(), Status::Idle);
                assert_eq!(transcript.turn_elapsed(), None);
                assert_eq!(update.boundary, Some(Boundary::TurnEnded));

                transcript.apply(Input::Prompt("next turn".into()));
                assert_eq!(transcript.turn_outcome(), None);
                assert!(!transcript.turn_completed());
                assert_eq!(transcript.status(), Status::Streaming);
                assert!(transcript.turn_elapsed().is_some());
                assert_eq!(transcript.last_cost(), cost_usd, "cost remains history");
            }
        }
    }

    #[test]
    fn revived_activity_retires_the_previous_turns_outcome() {
        let activities = [
            text("more answer"),
            Input::Event(SessionEvent::ThinkingDelta { text: "".into() }),
            reasoning("more reasoning", 0),
            started(
                "toolu_1",
                "Read",
                serde_json::json!({ "path": "README.md" }),
            ),
            Input::Event(SessionEvent::DecisionRequested {
                decision: Decision {
                    delivery: Default::default(),
                    kind: Default::default(),
                    policy: Default::default(),
                    id: "perm_01".into(),
                    tool_use_id: "toolu_01".into(),
                    tool_name: "AskUserQuestion".into(),
                    description: "which approach?".into(),
                    input: serde_json::Value::Null,
                    suggestions: vec![],
                },
            }),
            Input::Answered {
                allowed: true,
                tool_name: "Read".into(),
            },
        ];
        for closed in [false, true] {
            for activity in &activities {
                let mut transcript = Transcript::default();
                transcript.apply(Input::Event(SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: Some(0.038),
                }));
                if closed {
                    transcript.apply(Input::Event(SessionEvent::Closed {
                        reason: "Session exited".into(),
                    }));
                }
                transcript.apply(Input::Revived);
                assert_eq!(transcript.turn_outcome(), Some(&TurnOutcome::Completed));
                assert_eq!(transcript.turn_completed(), !closed);

                transcript.apply(activity.clone());

                assert_eq!(transcript.turn_outcome(), None, "{activity:?}");
                assert!(!transcript.turn_completed(), "{activity:?}");
                assert_eq!(transcript.last_cost(), Some(0.038));
                if matches!(
                    activity,
                    Input::Event(SessionEvent::DecisionRequested { .. })
                ) {
                    assert_eq!(transcript.status(), Status::Blocked);
                } else {
                    assert_eq!(transcript.status(), Status::Streaming);
                    assert!(transcript.turn_elapsed().is_some());
                }
            }
        }
    }

    /// CT-40: a blocking Decision blocks the Session and leaves no notice —
    /// the Decision row is what says it waits, once.
    #[test]
    fn a_decision_blocks_the_session_without_a_second_notice() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("write the file".into()));

        let update = transcript.apply(Input::Event(SessionEvent::DecisionRequested {
            decision: Decision {
                delivery: Default::default(),
                kind: Default::default(),
                policy: Default::default(),
                id: "perm_01".into(),
                tool_use_id: "toolu_01".into(),
                tool_name: "Write".into(),
                description: "ferrite-perm.txt".into(),
                input: serde_json::Value::Null,
                suggestions: vec![],
            },
        }));

        assert_eq!(transcript.status(), Status::Blocked);
        assert!(update.dirty.is_empty());
        assert!(transcript
            .blocks()
            .iter()
            .all(|block| !matches!(block.body, Body::Notice(_))));
    }

    #[test]
    fn a_blocking_question_leaves_no_notice_either() {
        let questions = crate::questions::parse(&serde_json::json!({"questions": [{
            "question": "Which approach?",
            "header": "Approach",
            "options": [{"label": "A"}, {"label": "B"}]
        }]}))
        .unwrap();
        for kind in [
            crate::DecisionKind::Questions(questions),
            crate::DecisionKind::Approval,
        ] {
            let mut transcript = Transcript::default();
            transcript.apply(Input::Event(SessionEvent::DecisionRequested {
                decision: Decision {
                    delivery: Default::default(),
                    kind,
                    policy: Default::default(),
                    id: "q_01".into(),
                    tool_use_id: "toolu_02".into(),
                    tool_name: "Bash".into(),
                    description: String::new(),
                    input: serde_json::Value::Null,
                    suggestions: vec![],
                },
            }));
            assert!(transcript.blocks().is_empty());
            assert_eq!(transcript.status(), Status::Blocked);
        }
    }

    #[test]
    fn a_revived_thread_says_it_was_revived() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("from before the restart".into()));

        let update = transcript.apply(Input::Revived);

        // The history is real; the Session serving it is new, and the
        // transcript says so rather than pretending nothing happened.
        let last = transcript.blocks().last().unwrap();
        assert_eq!(update.dirty, vec![last.id]);
        assert!(matches!(last.body, Body::Meta(_)));
        assert_eq!(
            body_text(last),
            "revived — new Session, history from the log"
        );
    }

    #[test]
    fn ferrite_can_say_something_of_its_own() {
        let mut transcript = Transcript::default();

        let update = transcript.apply(Input::Notice("send failed: broken pipe".into()));

        let last = transcript.blocks().last().unwrap();
        assert_eq!(update.dirty, vec![last.id]);
        assert!(matches!(last.body, Body::Notice(_)));
        assert_eq!(body_text(last), "send failed: broken pipe");
    }

    fn reasoning(text: &str, summary_index: u64) -> Input {
        Input::Event(SessionEvent::ReasoningSummaryDelta {
            text: text.into(),
            summary_index,
        })
    }

    #[test]
    fn answering_a_decision_records_it_and_unblocks_the_status() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Event(SessionEvent::DecisionRequested {
            decision: Decision {
                delivery: Default::default(),
                kind: Default::default(),
                policy: Default::default(),
                id: "perm_01".into(),
                tool_use_id: "toolu_01".into(),
                tool_name: "Write".into(),
                description: "ferrite-perm.txt".into(),
                input: serde_json::Value::Null,
                suggestions: vec![],
            },
        }));
        assert_eq!(transcript.status(), Status::Blocked);

        let update = transcript.apply(Input::Answered {
            allowed: true,
            tool_name: "Write".into(),
        });

        // The turn runs again the moment the answer goes out.
        assert_eq!(transcript.status(), Status::Streaming);
        let last = transcript.blocks().last().unwrap();
        assert_eq!(update.dirty, vec![last.id]);
        assert!(matches!(last.body, Body::Meta(_)));
        assert_eq!(body_text(last), "allowed Write");
    }

    #[test]
    fn a_denied_decision_says_so() {
        let mut transcript = Transcript::default();

        transcript.apply(Input::Answered {
            allowed: false,
            tool_name: "Bash".into(),
        });

        assert_eq!(
            body_text(transcript.blocks().last().unwrap()),
            "denied Bash"
        );
    }

    #[test]
    fn a_reasoning_summary_breaks_where_the_provider_broke_it() {
        let mut transcript = Transcript::default();

        transcript.apply(reasoning("Considering ", 0));
        transcript.apply(reasoning("the options.", 0));
        transcript.apply(reasoning("Now checking the tests.", 1));

        assert_eq!(transcript.blocks().len(), 2);
        assert!(matches!(transcript.blocks()[0].body, Body::Thinking(_)));
        assert_eq!(
            body_text(&transcript.blocks()[0]),
            "Considering the options."
        );
        assert_eq!(
            body_text(&transcript.blocks()[1]),
            "Now checking the tests."
        );
    }

    fn native_task(id: &str, subject: &str, status: crate::progress::StepStatus) -> Input {
        Input::Event(SessionEvent::Progress {
            event: crate::progress::ProgressEvent::Task {
                id: id.into(),
                subject: subject.into(),
                status: Some(status),
                deleted: false,
            },
        })
    }

    #[test]
    fn a_planned_todo_list_is_counted_as_it_is_worked() {
        use crate::progress::StepStatus::*;
        let mut transcript = Transcript::default();
        assert_eq!(transcript.todos(), None);
        for (id, subject) in [("1", "init git"), ("2", "add docs"), ("3", "make dirs")] {
            transcript.apply(native_task(id, subject, Pending));
        }
        transcript.apply(native_task("1", "", Completed));
        assert_eq!(transcript.todos(), Some(Todos { done: 1, total: 3 }));
        transcript.apply(native_task("2", "", InProgress));
        assert_eq!(transcript.todos(), Some(Todos { done: 1, total: 3 }));
        transcript.apply(native_task("1", "", Completed));
        assert_eq!(transcript.todos(), Some(Todos { done: 1, total: 3 }));
    }

    #[test]
    fn the_current_task_is_the_first_unfinished_subject() {
        use crate::progress::StepStatus::*;
        let mut transcript = Transcript::default();
        assert_eq!(transcript.current_task(), None);
        for (id, subject) in [
            ("1", "read the recipe"),
            ("2", "run the suite"),
            ("3", "land the diff"),
        ] {
            transcript.apply(native_task(id, subject, Pending));
        }
        assert_eq!(transcript.current_task(), Some("read the recipe"));
        transcript.apply(native_task("1", "", Completed));
        assert_eq!(transcript.current_task(), Some("run the suite"));
        for id in ["2", "3"] {
            transcript.apply(native_task(id, "", Completed));
        }
        assert_eq!(transcript.current_task(), None);
    }

    #[test]
    fn finished_work_never_outruns_the_plan() {
        use crate::progress::StepStatus::*;
        let mut transcript = Transcript::default();
        transcript.apply(native_task("1", "the only step", Pending));
        for id in ["1", "2", "3"] {
            transcript.apply(native_task(id, "", Completed));
        }
        assert_eq!(transcript.todos(), Some(Todos { done: 1, total: 1 }));
    }

    #[test]
    fn token_usage_is_kept_for_the_status_line() {
        let mut transcript = Transcript::default();
        assert_eq!(transcript.usage(), None);

        transcript.apply(Input::Event(SessionEvent::TokenUsage {
            total_tokens: 12_400,
            input_tokens: 11_000,
            cached_input_tokens: 8_000,
            output_tokens: 1_400,
            reasoning_output_tokens: 900,
            context_window: Some(200_000),
        }));

        let usage = transcript
            .usage()
            .expect("usage after the provider reports");
        assert_eq!(usage.total_tokens, 12_400);
        assert_eq!(usage.context_window, Some(200_000));
    }

    #[test]
    fn live_rate_limits_are_kept_separately_from_persisted_token_usage() {
        let mut transcript = Transcript::default();
        let five_hour = RateLimitWindow {
            used_fraction: 0.52,
            resets_at: Some(11),
        };
        let weekly = RateLimitWindow {
            used_fraction: 0.08,
            resets_at: Some(22),
        };
        transcript.apply(Input::Event(SessionEvent::RateLimits {
            five_hour: Some(five_hour),
            weekly: Some(weekly),
        }));

        assert_eq!(
            transcript.rate_limits(),
            RateLimits {
                five_hour: Some(five_hour),
                weekly: Some(weekly),
            }
        );
        assert_eq!(transcript.usage(), None, "limits do not invent token usage");
    }

    #[test]
    fn thinking_never_joins_the_answer() {
        let mut transcript = Transcript::default();

        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "weighing ".into(),
        }));
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "options".into(),
        }));
        transcript.apply(text("Here is the answer"));

        assert_eq!(transcript.blocks().len(), 2);
        assert!(matches!(transcript.blocks()[0].body, Body::Thinking(_)));
        assert_eq!(body_text(&transcript.blocks()[0]), "weighing options");
        assert!(matches!(
            transcript.blocks()[1].body,
            Body::Paragraph { .. }
        ));
    }

    /// The working line's clock runs from the prompt to the turn's end,
    /// and its token count sums the turn's messages whether the provider
    /// reports each message's own count or a running total.
    #[test]
    fn a_turn_has_a_clock_and_a_token_count() {
        let mut transcript = Transcript::new(std::sync::Arc::new(Unhighlighted));
        assert_eq!(transcript.turn_elapsed(), None);
        transcript.apply(Input::Prompt("go".into()));
        assert!(transcript.turn_elapsed().is_some());
        let usage = |output| {
            Input::Event(SessionEvent::TokenUsage {
                total_tokens: 100,
                input_tokens: 90,
                cached_input_tokens: 0,
                output_tokens: output,
                reasoning_output_tokens: 0,
                context_window: None,
            })
        };
        // A running total: 10, then 25 — the turn has 25.
        transcript.apply(usage(10));
        transcript.apply(usage(25));
        assert_eq!(transcript.turn_output_tokens(), 25);
        // A new message's own count of 8 — the turn has 33.
        transcript.apply(usage(8));
        assert_eq!(transcript.turn_output_tokens(), 33);
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        }));
        assert_eq!(transcript.turn_elapsed(), None);
        assert_eq!(
            transcript.turn_output_tokens(),
            33,
            "the count outlives the turn"
        );
        transcript.apply(Input::Prompt("again".into()));
        assert_eq!(
            transcript.turn_output_tokens(),
            0,
            "a new turn starts at nothing"
        );

        // A revived Thread mid-turn: no prompt in the window, but the
        // first streamed word starts the clock.
        let mut revived = Transcript::new(std::sync::Arc::new(Unhighlighted));
        revived.apply(Input::Event(SessionEvent::TextDelta { text: "…".into() }));
        assert!(revived.turn_elapsed().is_some());
    }

    /// Bold that wraps inline code stays bold on both sides of it, and a
    /// `**` inside code never opens bold.
    #[test]
    fn bold_may_wrap_inline_code() {
        assert_eq!(
            spans("1. **Drop the `// MARK:` lines** in X."),
            vec![
                Span {
                    text: "1. ".into(),
                    style: Style::Plain
                },
                Span {
                    text: "Drop the ".into(),
                    style: Style::Bold
                },
                Span {
                    text: "// MARK:".into(),
                    style: Style::Code
                },
                Span {
                    text: " lines".into(),
                    style: Style::Bold
                },
                Span {
                    text: " in X.".into(),
                    style: Style::Plain
                },
            ]
        );
        assert_eq!(
            spans("use `**kwargs` here"),
            vec![
                Span {
                    text: "use ".into(),
                    style: Style::Plain
                },
                Span {
                    text: "**kwargs".into(),
                    style: Style::Code
                },
                Span {
                    text: " here".into(),
                    style: Style::Plain
                },
            ]
        );
    }

    /// Claude's redacted thinking arrives as empty deltas — one per
    /// signature-only block. They must not become Blocks: a run of tool
    /// calls with a blank paragraph between each read as gaps.
    #[test]
    fn an_empty_thinking_delta_makes_no_block() {
        let mut transcript = Transcript::new(std::sync::Arc::new(Unhighlighted));
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: String::new(),
        }));
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: String::new(),
        }));
        assert!(transcript.blocks().is_empty());
        assert_eq!(transcript.status(), Status::Streaming);
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "now words".into(),
        }));
        assert_eq!(transcript.blocks().len(), 1);
    }

    #[test]
    fn an_overlong_transcript_drops_its_oldest_blocks_and_says_which() {
        let mut transcript = Transcript::with_capacity(std::sync::Arc::new(Unhighlighted), 2);
        transcript.apply(text("one\n\ntwo\n\n"));
        let oldest = transcript.blocks()[0].id;
        assert_eq!(transcript.blocks().len(), 2);

        let update = transcript.apply(text("three\n\n"));

        assert_eq!(update.evicted, vec![oldest]);
        assert_eq!(transcript.blocks().len(), 2);
        assert_eq!(body_text(&transcript.blocks()[0]), "two");
    }

    /// The memory claim behind a cockpit left running all day: a Thread that
    /// never stops talking stops growing, and says which Blocks it dropped.
    #[test]
    fn a_thread_that_streams_forever_stops_growing() {
        let mut transcript = Transcript::with_capacity(std::sync::Arc::new(Unhighlighted), 50);
        let mut evicted = 0;

        for n in 0..500 {
            let update = transcript.apply(text(&format!("paragraph {n}\n\n")));
            evicted += update.evicted.len();
        }

        assert_eq!(transcript.blocks().len(), 50, "the cap is the whole point");
        assert_eq!(evicted, 450, "and every drop was reported, not silent");
        // What is left is the newest end of the Thread, not the oldest.
        assert_eq!(
            body_text(transcript.blocks().last().unwrap()),
            "paragraph 499"
        );
    }

    #[test]
    fn a_turn_ending_marks_a_boundary_and_streaming_text_does_not() {
        let mut transcript = Transcript::default();

        let streaming = transcript.apply(text("still going"));
        assert_eq!(streaming.boundary, None);

        let ended = transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: Some(0.01),
        }));

        assert_eq!(ended.boundary, Some(Boundary::TurnEnded));
    }

    #[test]
    fn a_file_edit_settles_its_row_into_a_diff_card_with_counts() {
        let mut transcript = Transcript::default();
        transcript.apply(started(
            "toolu_1",
            "Edit",
            serde_json::json!({ "file_path": "/workspace/x.txt" }),
        ));
        let row = transcript.blocks()[0].id;

        let update = transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "toolu_1".into(),
            output: "applied".into(),
            is_error: false,
            result: crate::ToolResult::FileEdit {
                path: "/workspace/x.txt".into(),
                hunks: vec![crate::Hunk {
                    old_start: 1,
                    old_lines: 3,
                    new_start: 1,
                    new_lines: 3,
                    lines: vec![
                        " alpha".into(),
                        "-bravo".into(),
                        "+delta".into(),
                        " charlie".into(),
                    ],
                    section: None,
                }],
            },
        }));

        assert_eq!(update.dirty, vec![row]);
        match &transcript.blocks()[0].body {
            Body::Tool(tool) => {
                let diff = tool.diffs.first().expect("an edit carries a diff card");
                assert_eq!(diff.path, "/workspace/x.txt");
                assert_eq!((diff.added, diff.removed), (1, 1));
                assert_eq!(diff.hunks.len(), 1);
            }
            other => panic!("expected a tool row, got {other:?}"),
        }
    }

    #[test]
    fn a_settled_tool_retains_exact_bounded_output_on_its_stable_row() {
        let mut transcript = Transcript::default();
        transcript.apply(started("before", "Read", serde_json::Value::Null));
        transcript.apply(started("toolu_1", "Edit", serde_json::Value::Null));
        let row = transcript.blocks()[1].id;
        transcript.apply(Input::Notice("later tail block".into()));
        let output = format!("first line\n{}éTAIL", "x".repeat(65_524));

        let update = transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "toolu_1".into(),
            output: output.clone(),
            is_error: false,
            result: crate::ToolResult::FileEdit {
                path: "/workspace/x.txt".into(),
                hunks: vec![crate::Hunk {
                    old_start: 1,
                    old_lines: 1,
                    new_start: 1,
                    new_lines: 1,
                    lines: vec!["-old".into(), "+new".into()],
                    section: None,
                }],
            },
        }));

        assert_eq!(update.dirty, vec![row]);
        let Body::Tool(tool) = &transcript.blocks()[1].body else {
            panic!("expected the settled tool row")
        };
        assert_eq!(tool.result_line.as_deref(), Some("first line"));
        let retained = tool.output.as_ref().expect("non-blank output is retained");
        assert_eq!(retained.text.as_bytes(), &output.as_bytes()[..65_535]);
        assert!(retained.text.is_char_boundary(retained.text.len()));
        assert_eq!(retained.omitted_bytes, output.len() - retained.text.len());
        let diff = tool.diffs.first().expect("structured diff stays folded");
        assert_eq!((diff.added, diff.removed), (1, 1));

        let failure = transcript.apply(completed("toolu_1", "line one\nline two", true));
        assert_eq!(failure.dirty, vec![row]);
        let Body::Tool(tool) = &transcript.blocks()[1].body else {
            panic!("expected the same tool row")
        };
        assert!(
            matches!(&tool.state, ToolState::Failed(message) if message == "line one\nline two")
        );
        assert_eq!(tool.result_line, None);
        assert_eq!(
            tool.output.as_ref().map(|output| output.text.as_str()),
            Some("line one\nline two")
        );
    }

    /// DirectionDense's `⎿` continuation: a settled tool keeps the first
    /// line of its output, trimmed to a row — folded here, never parsed by
    /// the Pane (#22).
    #[test]
    fn a_tool_result_keeps_its_first_line_for_the_continuation_row() {
        let mut transcript = Transcript::default();
        transcript.apply(started("toolu_1", "Bash", serde_json::Value::Null));

        transcript.apply(completed(
            "toolu_1",
            "\n  \nexit 0 · 3.1s\nand 400 more lines nobody keeps",
            false,
        ));

        let Body::Tool(tool) = &transcript.blocks()[0].body else {
            panic!("expected a tool row")
        };
        assert_eq!(
            tool.result_line.as_deref(),
            Some("exit 0 · 3.1s"),
            "the first non-blank line, without the rest"
        );

        // A line longer than a narrow row stays whole: the row cuts it by
        // width, not this fold.
        let mut wide = Transcript::default();
        wide.apply(started("toolu_3", "Bash", serde_json::Value::Null));
        wide.apply(completed("toolu_3", &"y".repeat(150), false));
        let Body::Tool(tool) = &wide.blocks()[0].body else {
            panic!("expected a tool row")
        };
        assert_eq!(tool.result_line.as_deref(), Some("y".repeat(150).as_str()));

        // Only a line past what any row can show is cut, marked.
        let mut long = Transcript::default();
        long.apply(started("toolu_2", "Bash", serde_json::Value::Null));
        long.apply(completed("toolu_2", &"x".repeat(500), false));
        let Body::Tool(tool) = &long.blocks()[0].body else {
            panic!("expected a tool row")
        };
        let line = tool.result_line.as_deref().unwrap();
        assert_eq!(line.chars().count(), RESULT_CHARS + 1);
        assert!(line.ends_with('…'));
    }

    /// The other halves of the fold: whitespace-only output keeps nothing,
    /// and a failure keeps its message in the state, not a second copy here.
    #[test]
    fn blank_or_failed_output_leaves_no_continuation_row() {
        let mut blank = Transcript::default();
        blank.apply(started("toolu_1", "Read", serde_json::Value::Null));
        blank.apply(completed("toolu_1", "  \n \n", false));
        let Body::Tool(tool) = &blank.blocks()[0].body else {
            panic!("expected a tool row")
        };
        assert_eq!(tool.result_line, None);
        assert_eq!(tool.output, None);

        let mut failed = Transcript::default();
        failed.apply(started("toolu_2", "Bash", serde_json::Value::Null));
        failed.apply(completed("toolu_2", "boom", true));
        let Body::Tool(tool) = &failed.blocks()[0].body else {
            panic!("expected a tool row")
        };
        assert_eq!(tool.result_line, None);
        assert!(matches!(&tool.state, ToolState::Failed(m) if m == "boom"));
    }

    #[test]
    fn a_failed_tool_carries_its_error_trimmed_to_a_row() {
        let mut transcript = Transcript::default();
        transcript.apply(started("toolu_1", "Bash", serde_json::Value::Null));

        transcript.apply(completed("toolu_1", &"x".repeat(500), true));

        let Body::Tool(tool) = &transcript.blocks()[0].body else {
            panic!("expected a tool row")
        };
        let ToolState::Failed(message) = &tool.state else {
            panic!("expected a failure, got {:?}", tool.state)
        };
        assert_eq!(message.chars().count(), 201);
        assert!(message.ends_with('…'));
    }

    #[test]
    fn an_interrupted_turn_says_so_and_carries_no_cost() {
        let mut transcript = Transcript::default();
        transcript.apply(text("half a thou"));

        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Interrupted,
            cost_usd: None,
        }));

        assert_eq!(transcript.status(), Status::Idle);
        assert_eq!(transcript.last_cost(), None);
        let last = transcript.blocks().last().unwrap();
        assert!(matches!(
            &last.body,
            Body::TurnEnd(TurnEnd {
                outcome: crate::TurnOutcome::Interrupted,
                ..
            })
        ));
        // No observation timed it here, so the row claims no elapsed.
        assert_eq!(body_text(last), "interrupted");
    }

    #[test]
    fn a_failed_turn_surfaces_the_providers_message() {
        let mut transcript = Transcript::default();

        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Error("model overloaded".into()),
            cost_usd: None,
        }));

        assert_eq!(transcript.status(), Status::Idle);
        let last = transcript.blocks().last().unwrap();
        assert!(matches!(
            &last.body,
            Body::TurnEnd(TurnEnd {
                outcome: crate::TurnOutcome::Error(_),
                ..
            })
        ));
        assert_eq!(body_text(last), "failed · model overloaded");
    }

    #[test]
    fn an_observation_times_the_interrupted_or_failed_turn_it_follows() {
        for (outcome, said) in [
            (crate::TurnOutcome::Interrupted, "interrupted · 4.1s"),
            (
                crate::TurnOutcome::Error("model overloaded".into()),
                "failed · 4.1s · model overloaded",
            ),
            (crate::TurnOutcome::Completed, "Worked for 4s · 8:53 pm"),
        ] {
            let mut transcript = Transcript::default();
            transcript.apply(Input::Prompt("go".into()));
            transcript.apply(Input::Event(SessionEvent::TurnEnded {
                outcome,
                cost_usd: None,
            }));
            let rows = transcript.blocks().len();
            let update = transcript.apply(Input::CompletionObservation {
                elapsed_ms: 4_100,
                completed_at: "8:53 pm".into(),
                input_tokens: None,
                output_tokens: None,
            });
            let ends: Vec<_> = transcript
                .blocks()
                .iter()
                .filter(|block| matches!(block.body, Body::TurnEnd(_)))
                .collect();
            assert_eq!(ends.len(), 1, "one row per turn end, never two");
            assert_eq!(body_text(ends[0]), said);
            assert_eq!(update.dirty, vec![ends[0].id]);
            assert!(transcript.blocks().len() <= rows + 1);
        }
    }

    #[test]
    fn a_sub_second_turn_worked_for_under_a_second() {
        let end = TurnEnd {
            outcome: crate::TurnOutcome::Completed,
            elapsed_ms: Some(400),
            completed_at: Some("8:53 pm".into()),
            input_tokens: None,
            output_tokens: None,
        };
        assert_eq!(end.text(), "Worked for <1s \u{b7} 8:53 pm");
        let end = TurnEnd {
            elapsed_ms: Some(18_900),
            completed_at: None,
            ..end
        };
        assert_eq!(end.text(), "Worked for 18s");
    }

    #[test]
    fn prompting_a_closed_or_blocked_session_never_shows_streaming() {
        let mut closed = Transcript::default();
        closed.apply(Input::Event(SessionEvent::Closed {
            reason: "claude CLI exited".into(),
        }));
        closed.apply(Input::Prompt("anyone there?".into()));
        assert_eq!(closed.status(), Status::Closed);

        let mut blocked = Transcript::default();
        blocked.apply(Input::Event(SessionEvent::DecisionRequested {
            decision: Decision {
                delivery: Default::default(),
                kind: Default::default(),
                policy: Default::default(),
                id: "perm_01".into(),
                tool_use_id: "toolu_01".into(),
                tool_name: "Write".into(),
                description: "ferrite-perm.txt".into(),
                input: serde_json::Value::Null,
                suggestions: vec![],
            },
        }));
        blocked.apply(Input::Prompt("go ahead".into()));
        // The Decision is still what the Session waits on, not this prompt.
        assert_eq!(blocked.status(), Status::Blocked);
    }

    #[test]
    fn a_rust_fence_comes_back_highlighted_through_the_apply_path() {
        let (lexer, answers) = Lexer::new();
        let mut transcript = Transcript::new(std::sync::Arc::new(lexer));

        transcript.apply(text("```rust\nfn main() { let x = 1; }\n```\n\n"));

        // The lexer answered on its own channel; a pump feeds that back in.
        let answer = answers.try_recv().expect("the lexer answered");
        let update = transcript.apply(answer);

        let code = transcript
            .blocks()
            .iter()
            .find(|block| matches!(block.body, Body::Code { .. }))
            .expect("a code block");
        assert_eq!(update.dirty, vec![code.id]);
        let Body::Code { tokens, .. } = &code.body else {
            unreachable!()
        };
        let tokens = tokens.as_deref().expect("tokens for a settled fence");
        assert!(
            tokens
                .iter()
                .any(|token| token.class == Class::Keyword && token.text == "fn"),
            "no keyword in {tokens:?}"
        );
        // The Pane maps tokens onto the source by length, so they must cover it.
        let covered: String = tokens.iter().map(|token| token.text.as_str()).collect();
        assert_eq!(covered, "fn main() { let x = 1; }");
    }

    #[derive(Default)]
    struct Recorder {
        seen: std::sync::Mutex<Vec<HighlightRequest>>,
    }

    impl Highlighter for Recorder {
        fn request(&self, request: HighlightRequest) {
            self.seen.lock().unwrap().push(request);
        }
    }

    #[test]
    fn a_settled_code_block_is_highlighted_through_the_same_apply_path() {
        let recorder = std::sync::Arc::new(Recorder::default());
        let mut transcript = Transcript::new(recorder.clone());

        transcript.apply(text("```rust\nfn main() {}\n```\nafter"));

        // The module asked the injected highlighter — it never highlights itself.
        let asked = recorder.seen.lock().unwrap().clone();
        let code = transcript.blocks()[0].id;
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].block, code);
        assert_eq!(asked[0].language.as_deref(), Some("rust"));
        assert_eq!(asked[0].source, "fn main() {}");

        // The answer arrives later, as an input like any other.
        let update = transcript.apply(Input::Highlighted {
            block: code,
            tokens: vec![Token {
                text: "fn".into(),
                class: Class::Keyword,
            }],
        });

        assert_eq!(update.dirty, vec![code]);
        match &transcript.blocks()[0].body {
            Body::Code { tokens, .. } => {
                assert_eq!(tokens.as_deref().unwrap()[0].class, Class::Keyword)
            }
            other => panic!("expected code, got {other:?}"),
        }
    }
}
