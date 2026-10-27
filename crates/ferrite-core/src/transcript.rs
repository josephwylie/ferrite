//! Folds SessionEvents into stable-identity Blocks.
//!
//! Events in, Blocks out — no window, no timers. Every apply reports exactly
//! which Blocks changed. That is the seam the wall will repaint through; the
//! single Pane shipping today still redraws whole and drops the report.

use std::sync::Arc;

use crate::progress::{Phase, Progress, ProgressEvent, StepStatus};
use crate::{Hunk, RateLimitWindow, SessionEvent, ToolResult, TurnOutcome};

mod highlight;
mod markdown;
pub use highlight::{language_for_path, tokens as highlight_tokens, Lexer};
use markdown::OpenSection;

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
    /// An agent's `show_visual` page (see `crate::visual`): a tool call the
    /// Pane draws as the page itself, live while its input streams.
    Visual(crate::visual::Visual),
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
    /// An unmodelled provider result, as its bounded, selectable JSON
    /// preview for disclosure: never the payload itself, which only the log
    /// keeps (ADR 0032).
    pub structured_preview: Option<ToolOutput>,
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

/// A running test suite's count, folded from its streamed output. `total`
/// is 0 while the runner's announcement is unknown: Codex's stream drops a
/// command's first moments (0.160 sends no delta for what printed before
/// its first poll), so a fast `cargo test` can stream its result lines
/// without the `running 342 tests` that counted them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TestProgress {
    pub done: u32,
    pub total: u32,
}

impl TestProgress {
    /// Whether the runner said how many tests it holds.
    pub fn known(&self) -> bool {
        self.total > 0
    }
}

impl ToolBlock {
    /// The call line as it reads: `Name(args)`, the name as Claude Code
    /// prints it (`Update`, `Bash`), a shell command out of its `bash -lc
    /// '…'` wrapper.
    pub fn label(&self) -> String {
        let name = display_tool_name(&self.name);
        if self.summary.is_empty() {
            name.to_string()
        } else {
            format!("{name}({})", self.summary_line())
        }
    }

    /// The call line's argument. It never lays out hard line breaks: the
    /// original command stays in the ToolBlock and becomes selectable in
    /// the disclosed details.
    fn summary_line(&self) -> std::borrow::Cow<'_, str> {
        // A command reads as the command (`Bash(cargo test)`), as Claude
        // Code prints it, whatever purpose the model wrote beside it
        // (Claude's Bash `description`: "Run cargo test to identify failing
        // tests").
        let summary = if crate::docview::is_command_run(&self.name) && !self.summary.is_empty() {
            crate::providers::shell::unwrap_shell(&self.summary)
        } else {
            std::borrow::Cow::Borrowed(self.title.as_deref().unwrap_or(&self.summary))
        };
        if summary.contains(['\n', '\r']) {
            let first = summary
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("");
            format!("{} \u{2026}", first.trim()).into()
        } else {
            summary
        }
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

/// The change an edit approval asks to make, drawn as the diff it would
/// leave (Claude's `Edit`, `MultiEdit` and `Write` carry the text itself,
/// before the call runs; its row's `structuredPatch` comes only after).
/// Each replacement is one hunk: its old text against its new, line by
/// line, placed where the old text stands in the file today. `path` reads
/// relative to `workspace` when it is inside it. `None` for any other tool,
/// or an input that names no change.
pub fn proposed_diff(
    tool_name: &str,
    input: &serde_json::Value,
    workspace: Option<&std::path::Path>,
) -> Option<Diff> {
    let path = input.get("file_path").and_then(serde_json::Value::as_str)?;
    let text = |value: &serde_json::Value, key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let full = match workspace {
        Some(root) if !std::path::Path::new(path).is_absolute() => root.join(path),
        _ => std::path::PathBuf::from(path),
    };
    let current = std::fs::read_to_string(&full).ok();
    let replacements: Vec<(String, String)> = match tool_name {
        "Edit" => vec![(text(input, "old_string")?, text(input, "new_string")?)],
        "MultiEdit" => input
            .get("edits")?
            .as_array()?
            .iter()
            .filter_map(|edit| Some((text(edit, "old_string")?, text(edit, "new_string")?)))
            .collect(),
        "Write" => vec![(current.clone().unwrap_or_default(), text(input, "content")?)],
        _ => return None,
    };
    if replacements.is_empty() {
        return None;
    }
    let hunks = replacements
        .iter()
        .map(|(old, new)| {
            // Where the old text starts in the file, counted in lines.
            let start = current
                .as_deref()
                .filter(|_| !old.is_empty())
                .and_then(|file| file.find(old.as_str()))
                .and_then(|at| {
                    current
                        .as_deref()
                        .map(|file| file[..at].matches('\n').count())
                })
                .map_or(1, |before| before as u32 + 1);
            let lines = line_diff(old, new);
            let count =
                |mark: char| lines.iter().filter(|line| !line.starts_with(mark)).count() as u32;
            Hunk {
                old_start: start,
                old_lines: count('+'),
                new_start: start,
                new_lines: count('-'),
                section: hunk_section(&lines),
                lines,
            }
        })
        .collect();
    let shown = workspace
        .and_then(|root| full.strip_prefix(root).ok())
        .map(|inside| inside.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned());
    Some(Diff::new(shown, hunks))
}

/// `old` against `new`, line by line, each line marked as a hunk's are
/// (`' '` kept, `'-'` removed, `'+'` added): the longest common run kept,
/// removals before the additions that replace them. Past a few hundred
/// lines a side the whole of `old` is removed and `new` added.
pub fn line_diff(old: &str, new: &str) -> Vec<String> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let (n, m) = (a.len(), b.len());
    if n.saturating_mul(m) > 250_000 {
        return a
            .iter()
            .map(|line| format!("-{line}"))
            .chain(b.iter().map(|line| format!("+{line}")))
            .collect();
    }
    // `keep[i][j]`: the longest common run of `a[i..]` and `b[j..]`.
    let width = m + 1;
    let mut keep = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            keep[i * width + j] = if a[i] == b[j] {
                keep[(i + 1) * width + j + 1] + 1
            } else {
                keep[(i + 1) * width + j].max(keep[i * width + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut lines = Vec::with_capacity(n.max(m));
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            lines.push(format!(" {}", a[i]));
            i += 1;
            j += 1;
        } else if i < n && (j == m || keep[(i + 1) * width + j] >= keep[i * width + j + 1]) {
            lines.push(format!("-{}", a[i]));
            i += 1;
        } else {
            lines.push(format!("+{}", b[j]));
            j += 1;
        }
    }
    lines
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
    /// This Thread's history was replayed from the log: it reopened.
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
    /// The answer section still growing, and its Block.
    section: OpenSection,
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
    /// The thought still growing at the tail, and how far its heading has
    /// been read for the live status.
    thought: Option<(BlockId, crate::progress::HeadingScan)>,
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
        if self.total > 0 {
            return Some(TestProgress {
                done: self.done.min(self.total),
                total: self.total,
            });
        }
        // Results with no announcement: the count, against no total.
        (self.done > 0).then_some(TestProgress {
            done: self.done,
            total: 0,
        })
    }
}

/// The record an answered request leaves (`allowed Bash`, `denied Edit`):
/// history and search keep it; the transcript draws the call's own row
/// instead.
pub fn is_decision_record(text: &str) -> bool {
    [ANSWER_ALLOWED, ANSWER_DENIED].iter().any(|verb| {
        text.strip_prefix(verb)
            .is_some_and(|rest| rest.starts_with(' '))
    })
}

const ANSWER_ALLOWED: &str = "allowed";
const ANSWER_DENIED: &str = "denied";

/// The note a first send leaves of where the Thread works (`opened in
/// ~/ferrite`): the banner heading the transcript says it, so no row does.
pub fn is_opened_notice(text: &str) -> bool {
    text.starts_with(OPENED_IN)
}

/// The words `is_opened_notice` reads, as the first send writes them.
pub const OPENED_IN: &str = "opened in ";

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
    /// The most of an open line kept: its latest bytes.
    const KEPT: usize = 2048;

    fn push(&mut self, text: &str) -> Option<String> {
        let mut segments = text.split(['\n', '\r']).peekable();
        while let Some(segment) = segments.next() {
            self.extend(segment);
            if segments.peek().is_some() {
                let line = crate::progress::one_line(&self.line, RESULT_CHARS);
                if !line.is_empty() {
                    self.last = line;
                }
                self.line.clear();
            }
        }
        let line = crate::progress::one_line(&self.line, RESULT_CHARS);
        let line = if line.is_empty() { &self.last } else { &line };
        (!line.is_empty()).then(|| line.clone())
    }

    /// Add to the open line, keeping its latest `KEPT` bytes: one copy of
    /// what is kept, never one per character past it.
    fn extend(&mut self, segment: &str) {
        let segment = tail_of(segment, Self::KEPT);
        self.line.push_str(segment);
        if self.line.len() > Self::KEPT {
            let start = self.line.len() - tail_of(&self.line, Self::KEPT).len();
            self.line.drain(..start);
        }
    }
}

/// The last `bytes` of `text` at most, from a character boundary.
fn tail_of(text: &str, bytes: usize) -> &str {
    let mut start = text.len().saturating_sub(bytes);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
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
            section: OpenSection::default(),
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
            thought: None,
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

    /// When the running turn started; None between turns.
    pub fn turn_started(&self) -> Option<std::time::Instant> {
        self.turn_started
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
            self.forget_evicted();
        }
        update
    }

    /// Drop every Block up to `through`, oldest first: the history its
    /// owner trimmed by its own limit. Ids, Markdown runs and the
    /// presentation of what remains are untouched, exactly as for the
    /// oldest Blocks falling away; prose still streaming into a dropped
    /// Block begins a new one, as a replay of what remains would.
    pub(crate) fn evict_through(&mut self, through: BlockId) -> Update {
        let end = self.blocks.partition_point(|block| block.id <= through);
        if end == 0 {
            return Update::default();
        }
        let evicted = self.blocks.drain(..end).map(|block| block.id).collect();
        if self.section.block().is_some_and(|open| open <= through) {
            self.section.clear();
        }
        if self.latest_reasoning_part.is_some_and(|id| id <= through) {
            self.latest_reasoning_part = None;
        }
        if self.blocks.is_empty() {
            self.thinking_open = false;
        }
        self.forget_evicted();
        self.advance_revision();
        Update {
            evicted,
            ..Update::default()
        }
    }

    /// The Block a reasoning summary part streams into, while it lives.
    pub(crate) fn reasoning_part(&self, item_id: &str, summary_index: u64) -> Option<BlockId> {
        self.reasoning_parts
            .get(&(item_id.to_owned(), summary_index))
            .copied()
    }

    /// Release what only evicted Blocks needed. One pass over the Blocks
    /// left, however many calls are remembered.
    fn forget_evicted(&mut self) {
        let first = self.blocks.first().map(|block| block.id);
        self.reasoning_parts
            .retain(|_, id| first.is_some_and(|first| *id >= first));
        let mut calls = std::collections::HashSet::new();
        let mut running = std::collections::HashSet::new();
        for block in &self.blocks {
            if let Body::Tool(tool) = &block.body {
                calls.insert(tool.call.as_str());
                if tool.state == ToolState::Running {
                    running.insert(tool.call.as_str());
                }
            }
        }
        self.output_tails
            .retain(|id, _| running.contains(id.as_str()));
        self.test_counts
            .retain(|id, _| running.contains(id.as_str()));
        self.settled_at.retain(|id, _| calls.contains(id.as_str()));
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
                        if crate::progress::decision_wait(phase, &detail) {
                            return Update::default();
                        }
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
                if let Some(block) = self.section.block() {
                    self.ask_to_highlight(block);
                }
                self.section.clear();
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
                let verb = if allowed {
                    ANSWER_ALLOWED
                } else {
                    ANSWER_DENIED
                };
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
                self.turn_started = None;
                let mut dirty = self.retire_tools();
                // The log ended inside a turn (a crash, a kill, a restart):
                // that turn will never end, so it reads as interrupted
                // rather than idle with no outcome. Nothing is logged.
                if matches!(self.status, Status::Streaming | Status::Blocked) {
                    self.status = Status::Idle;
                    self.progress.end_turn();
                    self.latest_reasoning_part = None;
                    self.turn_outcome = Some(TurnOutcome::Interrupted);
                    dirty.push(self.push(Body::TurnEnd(TurnEnd {
                        outcome: TurnOutcome::Interrupted,
                        elapsed_ms: None,
                        completed_at: None,
                        input_tokens: None,
                        output_tokens: None,
                    })));
                }
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
                ..
            }) if self.visual_index(&id).is_some() => {
                self.progress.finish_tool(&id);
                Update {
                    dirty: self.settle_visual(&id, &output, is_error),
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
                self.output_tails.remove(&id);
                self.test_counts.remove(&id);
                let name = self.tool(&id).map(|tool| tool.name.clone());
                // A failure already carries its message in the state; a
                // success keeps its summary (`412 lines`, `6 matches in 2
                // files`) or its first output line for the `└` row.
                let row_line = |payload: Option<&serde_json::Value>| {
                    (!is_error)
                        .then(|| {
                            name.as_deref()
                                .and_then(|name| result_summary(name, payload, &output))
                                .or_else(|| result_line(&output))
                        })
                        .flatten()
                };
                // A payload Ferrite does not model is kept only as its bounded
                // preview, made once here (ADR 0032); a record's `Preview`
                // brings the preview and row line it settled with.
                let (diffs, structured_preview, result_line) = match result {
                    ToolResult::Preview {
                        preview,
                        result_line,
                        ..
                    } => (Vec::new(), preview, result_line),
                    ToolResult::FileEdit { path, hunks } => {
                        (vec![Diff::new(path, hunks)], None, row_line(None))
                    }
                    ToolResult::FileEdits { edits } => (
                        edits
                            .into_iter()
                            .map(|edit| Diff::new(edit.path, edit.hunks))
                            .collect(),
                        None,
                        row_line(None),
                    ),
                    ToolResult::Structured { value, .. } => (
                        Vec::new(),
                        serde_json::to_string_pretty(&value)
                            .ok()
                            .and_then(|text| retained_output(&text)),
                        row_line(Some(&value)),
                    ),
                    _ => (Vec::new(), None, row_line(None)),
                };
                let output = retained_output(&output);
                Update {
                    dirty: self
                        .settle_tool(&id, state, diffs, structured_preview, result_line, output)
                        .into_iter()
                        .collect(),
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::ToolDraft { id, name, input }) => {
                if !crate::visual::is_tool(&name) {
                    return Update::default();
                }
                self.status = Status::Streaming;
                self.progress.phase(Phase::Working);
                Update {
                    dirty: self
                        .draw_visual(&id, &input, crate::visual::Status::Drawing)
                        .into_iter()
                        .collect(),
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::ToolStarted { id, name, input })
                if crate::visual::is_tool(&name) =>
            {
                self.status = Status::Streaming;
                self.progress.phase(Phase::Working);
                Update {
                    dirty: self
                        .draw_visual(&id, &input, crate::visual::Status::Checking)
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
                // Codex's patch item names its edit up front: its row (and
                // an approval it waits on) shows the diff it would make.
                let diffs = if name == "fileChange" {
                    crate::providers::file_change_diffs(&input)
                        .into_iter()
                        .map(|(path, hunks)| Diff::new(path, hunks))
                        .collect()
                } else {
                    Vec::new()
                };
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
                    diffs,
                    structured_preview: None,
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
            Input::Event(
                SessionEvent::Models { .. }
                | SessionEvent::Queue(_)
                | SessionEvent::ScheduledWork { .. },
            ) => Update::default(),
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
        let last_id = &mut self.last_id;
        let grown = self
            .section
            .grow(text, &mut self.blocks, || next_id(last_id));
        // A closed section is worth highlighting even if it closed unchanged.
        for id in grown.closed {
            self.ask_to_highlight(id);
        }
        grown.dirty
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
                Body::Visual(visual)
                    if matches!(
                        visual.status,
                        crate::visual::Status::Drawing | crate::visual::Status::Checking
                    ) =>
                {
                    visual.status = crate::visual::Status::Failed(VISUAL_UNFINISHED.into());
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
            let id = *id;
            let scan = match &mut self.thought {
                Some((owner, scan)) if *owner == id => scan,
                slot => &mut slot.insert((id, Default::default())).1,
            };
            self.progress.thought_heading(scan.read(thought));
            self.section.clear();
            return id;
        }
        let id = self.push(Body::Thinking(text.to_string()));
        let mut scan = crate::progress::HeadingScan::default();
        self.progress.thought_heading(scan.read(text));
        self.thought = Some((id, scan));
        self.thinking_open = true;
        id
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
        structured_preview: Option<ToolOutput>,
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
            && tool.structured_preview == structured_preview
            && tool.result_line == result_line
            && tool.output == output
        {
            return None;
        }
        let was_running = tool.state == ToolState::Running;
        tool.state = state;
        tool.diffs = diffs;
        tool.structured_preview = structured_preview;
        if result_line.is_some() || !was_running {
            tool.result_line = result_line;
        }
        if output.is_some() || !was_running {
            tool.output = output;
        }
        Some(block.id)
    }

    /// The row of call `call`, newest first.
    pub fn tool(&self, call: &str) -> Option<&ToolBlock> {
        self.blocks
            .iter()
            .rev()
            .find_map(|block| match &block.body {
                Body::Tool(tool) if tool.call == call => Some(tool),
                _ => None,
            })
    }

    /// Where the visual for call `id` is, newest first.
    fn visual_index(&self, id: &str) -> Option<usize> {
        self.blocks
            .iter()
            .rposition(|block| matches!(&block.body, Body::Visual(visual) if visual.id == id))
    }

    /// A `show_visual` call's input, as far as it has arrived (`Drawing`) or
    /// whole (`Checking`), into its Block — begun by the first of them. A
    /// visual that has already settled is not drawn back: a completed
    /// message repeating the call changes nothing.
    fn draw_visual(
        &mut self,
        id: &str,
        input: &serde_json::Value,
        status: crate::visual::Status,
    ) -> Option<BlockId> {
        let next = crate::visual::Visual::from_input(id, input, status);
        let Some(index) = self.visual_index(id) else {
            return Some(self.push(Body::Visual(next)));
        };
        let block = &mut self.blocks[index];
        let Body::Visual(visual) = &mut block.body else {
            return None;
        };
        let open = matches!(
            visual.status,
            crate::visual::Status::Drawing | crate::visual::Status::Checking
        );
        if !open || *visual == next {
            return None;
        }
        *visual = next;
        Some(block.id)
    }

    /// The call's result: shown, or why not. A shown visual that asked to
    /// replace the one before it folds that one away.
    fn settle_visual(&mut self, id: &str, output: &str, is_error: bool) -> Vec<BlockId> {
        let mut dirty = Vec::new();
        let Some(index) = self.visual_index(id) else {
            return dirty;
        };
        let Body::Visual(visual) = &mut self.blocks[index].body else {
            return dirty;
        };
        let status = if is_error {
            crate::visual::Status::Failed(trim(output, ERROR_CHARS))
        } else {
            crate::visual::Status::Shown
        };
        let replaces = !is_error && visual.replace;
        if visual.status != status {
            visual.status = status;
            dirty.push(self.blocks[index].id);
        }
        if replaces {
            let previous = self.blocks[..index]
                .iter_mut()
                .rev()
                .find_map(|block| match &mut block.body {
                    Body::Visual(visual) if !visual.replaced => Some((block.id, visual)),
                    _ => None,
                });
            if let Some((block, visual)) = previous {
                visual.replaced = true;
                dirty.push(block);
            }
        }
        dirty
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
        self.section.clear();
        id
    }

    fn mint(&mut self) -> BlockId {
        next_id(&mut self.last_id)
    }

    fn advance_revision(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }
}

fn next_id(last_id: &mut u64) -> BlockId {
    *last_id += 1;
    BlockId(*last_id)
}

/// How much of a tool failure a row carries; the model got all of it.
const ERROR_CHARS: usize = 200;

/// Why a visual that never got its result failed: its turn ended (or its
/// Session closed) while it was still drawing or being checked.
pub const VISUAL_UNFINISHED: &str = "not shown: the turn ended before it finished";

/// How much of a tool's output its `⎿` continuation row carries: one line
/// the row cuts by width, never wrapping.
const RESULT_CHARS: usize = crate::progress::ROW_CHARS;

pub(crate) const TOOL_OUTPUT_BYTES: usize = 64 * 1024;

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
    // A patch (Codex's `fileChange`) names its files in `changes`: the
    // first, and how many more.
    if let Some(changes) = input.get("changes").and_then(|v| v.as_array()) {
        let paths: Vec<&str> = changes
            .iter()
            .filter_map(|change| change.get("path")?.as_str())
            .collect();
        match paths.as_slice() {
            [] => {}
            [one] => return (*one).to_string(),
            [first, rest @ ..] => return format!("{first} +{}", rest.len()),
        }
    }
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

/// A transcript as plain text, the way the operator would paste it
/// somewhere else and the API's `agent.read` hands it to an agent: prompts
/// quoted, prose flat, code fenced, tool rows as their one line plus the
/// result line indented under it. Thinking stays out — it is not the
/// conversation.
pub fn plain_text(blocks: &[Block]) -> String {
    let flat = |spans: &[Span]| -> String { spans.iter().map(|span| span.text.as_str()).collect() };
    let mut out = String::new();
    for block in blocks {
        let piece = match &block.body {
            Body::Prompt(text) => format!("> {text}"),
            Body::Paragraph { spans } => flat(spans),
            Body::Heading { spans, .. } => format!("## {}", flat(spans)),
            Body::Bullet { spans } => format!("• {}", flat(spans)),
            Body::Code {
                language, source, ..
            } => format!(
                "```{}\n{source}\n```",
                language.as_deref().unwrap_or_default()
            ),
            Body::Tool(tool) => {
                let mut line = format!("● {}", tool.name);
                if !tool.summary.is_empty() {
                    line.push_str(&format!(" ({})", tool.summary));
                }
                // The elbow is painted, never text: the result hangs at
                // its column, as a sweep across the row copies it.
                if let Some(result) = &tool.result_line {
                    line.push_str(&format!("\n    {result}"));
                }
                line
            }
            Body::Thinking(_) => continue,
            Body::Notice(text) | Body::Meta(text) => text.clone(),
            Body::TurnEnd(end) => end.text(),
            // The page is a picture; its words are its title and caption.
            Body::Visual(visual) => format!("● Visual ({})", visual.text().replace('\n', " · ")),
        };
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&piece);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::markdown::spans;
    use super::*;

    use crate::Decision;

    /// A command's output tail reads its latest line, however long the
    /// output, keeping the last 2 KB of a line with no end yet.
    #[test]
    fn an_output_tail_takes_a_long_line_in_its_stride() {
        let mut tail = OutputTail::default();
        let long = "x".repeat(4 * 1024 * 1024);
        assert!(tail.push(&long).is_some());
        assert_eq!(tail.line.len(), 2048);
        assert_eq!(
            tail.push("tail end\nnext").as_deref(),
            Some("next"),
            "the open line after the newline"
        );
        assert_eq!(tail.push("\n").as_deref(), Some("next"));
        assert_eq!(tail.last, "next");
        assert_eq!(tail.push("caf\u{e9}").as_deref(), Some("caf\u{e9}"));

        // Cut at a character, never inside one; a carriage return starts
        // the line over.
        let mut tail = OutputTail::default();
        tail.push(&"\u{e9}".repeat(2000));
        assert!(
            (2047..=2048).contains(&tail.line.len()),
            "{}",
            tail.line.len()
        );
        assert_eq!(tail.push("\rnew").as_deref(), Some("new"));
    }

    /// Codex's patch item names its file and its edit before it runs (and
    /// before an approval it waits on is answered): `Update(NOTES.md)` and
    /// the diff, as 0.160 sends it.
    #[test]
    fn a_codex_patch_names_its_file_and_edit_as_it_starts() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "exec-1".into(),
            name: "fileChange".into(),
            input: serde_json::json!({
                "type": "fileChange", "id": "exec-1", "status": "inProgress",
                "changes": [{ "path": "/repo/NOTES.md", "kind": { "type": "add" }, "diff": "hello\n" }],
            }),
        }));
        let Body::Tool(tool) = &transcript.blocks()[0].body else {
            panic!("a tool row");
        };
        assert_eq!(tool.summary, "/repo/NOTES.md");
        assert_eq!(tool.diffs.len(), 1);
        assert_eq!((tool.diffs[0].added, tool.diffs[0].removed), (1, 0));
    }

    /// An edit approval reads as the diff it would leave, placed where its
    /// old text stands in the file.
    #[test]
    fn an_edit_approval_reads_as_its_diff() {
        assert_eq!(
            line_diff("a\nb\nc", "a\nB\nc\nd"),
            [" a", "-b", "+B", " c", "+d"]
        );
        let root = std::env::temp_dir().join(format!("ferrite-proposed-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/stats.rs"),
            "one\ntwo\n    let mid = 1;\nfour\n",
        )
        .unwrap();
        let input = serde_json::json!({
            "file_path": root.join("src/stats.rs").display().to_string(),
            "old_string": "    let mid = 1;",
            "new_string": "    let len = 2;\n    let mid = len / 2;",
        });
        let diff = proposed_diff("Edit", &input, Some(&root)).unwrap();
        assert_eq!(diff.path, "src/stats.rs");
        assert_eq!((diff.added, diff.removed), (2, 1));
        assert_eq!(diff.hunks[0].new_start, 3, "the old text stands on line 3");
        assert!(proposed_diff("Bash", &input, Some(&root)).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

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
        // Codex's stream lost the announcement: the count, total unknown.
        let mut cut = TestCounter::default();
        cut.push("test case_005 ... ok\ntest case_003 ... ok\n");
        let cut = cut.progress().unwrap();
        assert_eq!((cut.done, cut.known()), (2, false));
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
            Body::Visual(visual) => visual.text(),
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

    /// ADR 0032: which Session serves a Thread is not the operator's
    /// concern, so a revive adds nothing to a settled transcript.
    #[test]
    fn a_revived_thread_reads_as_it_was_left() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("from before the restart".into()));
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        }));
        let before = transcript.blocks().to_vec();

        let update = transcript.apply(Input::Revived);

        assert!(update.dirty.is_empty());
        assert_eq!(transcript.blocks(), before.as_slice());
    }

    #[test]
    fn a_turn_the_log_left_open_revives_as_interrupted() {
        for blocked in [false, true] {
            let mut transcript = Transcript::default();
            transcript.apply(Input::Prompt("cut off mid-turn".into()));
            transcript.apply(text("half a thou"));
            if blocked {
                transcript.apply(Input::Event(SessionEvent::RunState {
                    state: crate::RunState::RequiresAction,
                }));
            }

            let update = transcript.apply(Input::Revived);

            assert_eq!(transcript.status(), Status::Idle);
            assert_eq!(
                transcript.turn_outcome(),
                Some(&crate::TurnOutcome::Interrupted)
            );
            let last = transcript.blocks().last().unwrap();
            assert_eq!(body_text(last), "interrupted");
            assert_eq!(update.dirty, vec![last.id]);
        }
    }

    #[test]
    fn a_settled_thread_revives_without_an_interruption() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("done before the restart".into()));
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        }));

        transcript.apply(Input::Revived);

        assert_eq!(
            transcript.turn_outcome(),
            Some(&crate::TurnOutcome::Completed)
        );
        assert!(transcript
            .blocks()
            .iter()
            .all(|block| !matches!(block.body, Body::TurnEnd(_))));
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

    fn thinking(text: &str) -> Input {
        Input::Event(SessionEvent::ThinkingDelta { text: text.into() })
    }

    /// The live status reads a thought's first bold heading, however the
    /// stream splits it: the marks, the words between them, or a heading
    /// arriving late in a long thought.
    #[test]
    fn a_streamed_thought_finds_its_heading_wherever_the_deltas_split() {
        let caption = |deltas: &[&str]| {
            let mut transcript = Transcript::default();
            for delta in deltas {
                transcript.apply(thinking(delta));
            }
            transcript.progress().caption()
        };
        let heading = Some("Planning the fix".to_string());
        assert_eq!(caption(&["**Planning the fix** then more"]), heading);
        assert_eq!(caption(&["*", "*Planning", " the fix*", "* more"]), heading);
        assert_eq!(
            caption(&["no heading yet ", "**Planning the fix**"]),
            heading
        );
        assert_eq!(caption(&["**Planning the fix**", " **Later**"]), heading);
        assert_eq!(
            caption(&["界**", "Planning the fix", "**"]),
            heading,
            "a mark right after a multi-byte character"
        );
        assert_eq!(
            caption(&["***Planning the fix**"]),
            Some("*Planning the fix".into())
        );
        assert_eq!(
            caption(&["** **", " **Later**"]),
            Some("Working".into()),
            "a blank heading names none, and no later one stands in"
        );
        assert_eq!(
            caption(&["an open **mark ", "never closed"]),
            Some("Working".into())
        );
        // A new thought reads its own heading.
        let mut transcript = Transcript::default();
        transcript.apply(thinking("**First**"));
        transcript.apply(text("answer\n\n"));
        transcript.apply(thinking("plain words, ending with **Second**"));
        assert_eq!(transcript.progress().caption(), Some("Second".into()));
    }

    /// Claude streams a thought in thousands of small deltas. Each delta
    /// must cost what it adds, not a re-read of the whole thought so far.
    #[test]
    fn a_long_thought_streams_in_time_linear_in_its_length() {
        let mut transcript = Transcript::default();
        let delta = format!("{} ", "w".repeat(127));
        let started = std::time::Instant::now();
        for _ in 0..16_000 {
            transcript.apply(thinking(&delta));
        }
        transcript.apply(thinking("**Heading at last**"));
        let elapsed = started.elapsed();
        assert_eq!(
            transcript.progress().caption(),
            Some("Heading at last".into())
        );
        assert!(
            elapsed < std::time::Duration::from_millis(1500),
            "a 2 MB thought in 16,000 deltas took {elapsed:?}"
        );
    }

    /// An answer streams the same way: one long paragraph, styled as it
    /// goes, costs each delta what it adds. It folds to the Block the whole
    /// text folds to at once.
    #[test]
    fn a_long_paragraph_streams_in_time_linear_in_its_length() {
        let mut transcript = Transcript::default();
        let delta = format!("{} `c` **b** [l](u) ", "w".repeat(108));
        let budget = std::time::Duration::from_millis(1500);
        let started = std::time::Instant::now();
        for sent in 1..=8_000 {
            transcript.apply(text(&delta));
            assert!(
                started.elapsed() < budget,
                "{sent} deltas of one paragraph took {:?}",
                started.elapsed()
            );
        }
        let mut whole = Transcript::default();
        whole.apply(text(&delta.repeat(8_000)));
        assert_eq!(transcript.blocks().len(), 1);
        assert_eq!(transcript.blocks()[0].body, whole.blocks()[0].body);
        assert_eq!(transcript.blocks()[0].markdown, whole.blocks()[0].markdown);
    }

    /// A long fenced block streams the same way: each delta costs what it
    /// adds, and the block folds to what the whole text folds to, open and
    /// closed.
    #[test]
    fn a_long_code_block_streams_in_time_linear_in_its_length() {
        let mut transcript = Transcript::default();
        transcript.apply(text("```rust\n"));
        let delta = format!(
            "    let w = {}; // `c` **b**\n{} ",
            "w".repeat(60),
            "w".repeat(40)
        );
        let budget = std::time::Duration::from_millis(1500);
        let started = std::time::Instant::now();
        for sent in 1..=8_000 {
            transcript.apply(text(&delta));
            assert!(
                started.elapsed() < budget,
                "{sent} deltas of one code block took {:?}",
                started.elapsed()
            );
        }
        let fold = |t: &Transcript| -> Vec<(Body, Option<String>)> {
            t.blocks()
                .iter()
                .map(|b| (b.body.clone(), b.markdown.clone()))
                .collect()
        };
        let mut whole = Transcript::default();
        whole.apply(text(&format!("```rust\n{}", delta.repeat(8_000))));
        assert_eq!(fold(&transcript), fold(&whole));
        transcript.apply(text("\n```\nafter"));
        whole.apply(text("\n```\nafter"));
        assert_eq!(fold(&transcript), fold(&whole));
    }

    /// A marker that never closes — a lone backtick, `**`, an index's `[` —
    /// does not hold the rest of its paragraph to be read again on every
    /// delta.
    #[test]
    fn a_marker_that_never_closes_streams_in_time_linear_in_its_length() {
        for opener in ["a lone ` tick", "a lone ** star", "array[i] reads"] {
            let mut transcript = Transcript::default();
            transcript.apply(text(opener));
            let delta = format!(" {}", "w".repeat(125));
            let budget = std::time::Duration::from_millis(1500);
            let started = std::time::Instant::now();
            for sent in 1..=8_000 {
                transcript.apply(text(&delta));
                assert!(
                    started.elapsed() < budget,
                    "{sent} deltas after {opener:?} took {:?}",
                    started.elapsed()
                );
            }
            let mut whole = Transcript::default();
            whole.apply(text(&format!("{opener}{}", delta.repeat(8_000))));
            assert_eq!(transcript.blocks()[0].body, whole.blocks()[0].body);
        }
    }

    /// Nor do markers back to back, one space between them: the space is
    /// where the prose before them settles.
    #[test]
    fn markers_back_to_back_stream_in_time_linear_in_their_length() {
        for delta in ["`a` ", "**b** ", "[l](u) "] {
            let mut transcript = Transcript::default();
            let budget = std::time::Duration::from_millis(1500);
            let started = std::time::Instant::now();
            for sent in 1..=8_000 {
                transcript.apply(text(delta));
                assert!(
                    started.elapsed() < budget,
                    "{sent} deltas of {delta:?} took {:?}",
                    started.elapsed()
                );
            }
            let mut whole = Transcript::default();
            whole.apply(text(&delta.repeat(8_000)));
            assert_eq!(transcript.blocks()[0].body, whole.blocks()[0].body);
        }
    }

    /// A link however long its target — a signed URL, a data URL — is a
    /// link, and streams like the rest.
    #[test]
    fn a_link_with_a_long_url_streams_in_time_linear_in_its_length() {
        let link = format!("see [the file](https://x.y/{}) and ", "u".repeat(4096));
        let mut transcript = Transcript::default();
        let budget = std::time::Duration::from_millis(1500);
        let started = std::time::Instant::now();
        for (sent, delta) in link.repeat(100).as_bytes().chunks(32).enumerate() {
            transcript.apply(text(std::str::from_utf8(delta).unwrap()));
            assert!(
                started.elapsed() < budget,
                "{sent} deltas of long links took {:?}",
                started.elapsed()
            );
        }
        let mut whole = Transcript::default();
        whole.apply(text(&link.repeat(100)));
        assert_eq!(transcript.blocks()[0].body, whole.blocks()[0].body);
        let Body::Paragraph { spans } = &transcript.blocks()[0].body else {
            panic!("a paragraph");
        };
        let links: Vec<_> = spans
            .iter()
            .filter(|span| span.style == Style::Link)
            .collect();
        assert_eq!(links.len(), 100);
        assert!(links.iter().all(|span| span.text == "the file"));
    }

    /// However the provider splits an answer, every delta leaves the Blocks
    /// the text so far folds to in one piece: markers that close late, or
    /// never; sections that open as one kind and turn out another.
    #[test]
    fn an_answer_folds_the_same_however_its_deltas_split() {
        let answers = [
            "plain words that run on and on without a mark",
            "a `code` span, **bold `with code` inside**, and [a link](https://x.y) end",
            "an unclosed `tick runs on and on, then closes` later",
            "an unclosed **star runs on and on, then closes** later",
            "a [bracket runs on and on](then a target) later",
            "array[i] and [x] then words then ](late) closes it",
            "a``b and ****, then *one* star, ** spaced ** and `` `` done",
            "line one\nline two `spans\nlines` **and\nbold** too\n\nnext paragraph",
            "  leading spaces\n\n# A heading `x`\n- a bullet **b**\n* star bullet\n## \nafter",
            "```rust\nfn main() {}\n\n```\nprose after code\n```\nopen code",
            "caf\u{e9} \u{1f600} [\u{e9}](u) **\u{e9}** `\u{e9}` \u{e9}",
            "trailing spaces   \n  \n\n\n- \n-x\n#x\n``\n```",
            "crlf\r\nline\r\n\r\npara",
            "a lone * then *more* and [a] [b](c) `d` **e `f** g` h [i](j k",
            "```rust\nfn a() {}\r\n    indented\r\n\r\n\nlast\r``\nmore `x`\n``` \nafter",
            "```\n\n\nx\n\r\n```",
            "`a` `b` **c** **d** [e](f) [g](h)`i` j `k`l ``m`` *n*`o`",
        ];
        // Markers whose closer comes far on, or never, with more markers
        // waiting between: each closes however far it has to look.
        let far = "w ".repeat(700);
        let long = "w".repeat(1500);
        let edges = [
            format!("a lone ` tick {far} then `x` y"),
            format!("a lone ** star {far} then **b** y"),
            format!("array[i] {far} then [l](u) and ](late) y"),
            format!("a ` b **c** {far} d"),
            format!("a ` b {far} **c `d` [e {far} f` g** h"),
            format!("`{long}` then `x` done"),
            format!("**{long}** then **b** done"),
            format!("[l](https://x.y/{long}) then [m](u) done"),
            format!("[{far}](u) a [](x) [b [c {far} ](d) e) f"),
        ];
        for answer in answers
            .iter()
            .copied()
            .chain(edges.iter().map(String::as_str))
        {
            let chars: Vec<char> = answer.chars().collect();
            for size in [1, 2, 3, 5, 8, 13] {
                let mut transcript = Transcript::default();
                let mut sent = String::new();
                for chunk in chars.chunks(size) {
                    let chunk: String = chunk.iter().collect();
                    sent.push_str(&chunk);
                    transcript.apply(text(&chunk));
                    let mut whole = Transcript::default();
                    whole.apply(text(&sent));
                    let fold = |t: &Transcript| -> Vec<(Body, Option<String>)> {
                        t.blocks()
                            .iter()
                            .map(|b| (b.body.clone(), b.markdown.clone()))
                            .collect()
                    };
                    assert_eq!(fold(&transcript), fold(&whole), "{sent:?} in {size}s");
                }
            }
        }
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

    mod visuals {
        use super::super::*;
        use crate::visual::{Status as Shown, CLAUDE_NAME, CODEX_NAME};
        use serde_json::json;

        fn draft(t: &mut Transcript, id: &str, input: serde_json::Value) -> Update {
            t.apply(Input::Event(SessionEvent::ToolDraft {
                id: id.into(),
                name: CLAUDE_NAME.into(),
                input,
            }))
        }

        fn start(t: &mut Transcript, id: &str, input: serde_json::Value) -> Update {
            t.apply(Input::Event(SessionEvent::ToolStarted {
                id: id.into(),
                name: CLAUDE_NAME.into(),
                input,
            }))
        }

        fn complete(t: &mut Transcript, id: &str, output: &str, is_error: bool) -> Update {
            t.apply(Input::Event(SessionEvent::ToolCompleted {
                id: id.into(),
                output: output.into(),
                is_error,
                result: ToolResult::Opaque,
            }))
        }

        fn visuals(t: &Transcript) -> Vec<(BlockId, crate::visual::Visual)> {
            t.blocks()
                .iter()
                .filter_map(|block| match &block.body {
                    Body::Visual(visual) => Some((block.id, visual.clone())),
                    _ => None,
                })
                .collect()
        }

        #[test]
        fn a_visual_draws_checks_and_shows_in_one_row() {
            let mut t = Transcript::default();
            t.apply(Input::Prompt("show me".into()));
            let first = draft(&mut t, "v1", json!({"title": "Sal"}));
            let [(row, visual)] = visuals(&t).try_into().unwrap();
            assert_eq!(first.dirty, vec![row]);
            assert_eq!(visual.status, Shown::Drawing);
            assert_eq!(visual.title, "Sal");

            let grown = draft(&mut t, "v1", json!({"title": "Sales", "html": "<p>"}));
            assert_eq!(grown.dirty, vec![row]);
            let again = draft(&mut t, "v1", json!({"title": "Sales", "html": "<p>"}));
            assert!(again.dirty.is_empty(), "an unchanged draft changes nothing");

            let full = json!({"title": "Sales", "caption": "Q3", "html": "<p>done</p>"});
            assert_eq!(start(&mut t, "v1", full.clone()).dirty, vec![row]);
            let [(_, visual)] = visuals(&t).try_into().unwrap();
            assert_eq!(visual.status, Shown::Checking);
            assert_eq!(visual.caption.as_deref(), Some("Q3"));
            assert_eq!(visual.html, "<p>done</p>");

            assert_eq!(
                complete(&mut t, "v1", "Shown.\n[image]", false).dirty,
                vec![row]
            );
            // A completed message repeating the call draws nothing back.
            assert!(start(&mut t, "v1", full).dirty.is_empty());
            let [(_, visual)] = visuals(&t).try_into().unwrap();
            assert_eq!(visual.status, Shown::Shown);
            assert!(
                !t.blocks()
                    .iter()
                    .any(|block| matches!(block.body, Body::Tool(_))),
                "never a tool row as well"
            );
        }

        #[test]
        fn a_codex_call_arrives_whole_and_starts_checking() {
            let mut t = Transcript::default();
            t.apply(Input::Event(SessionEvent::ToolStarted {
                id: "call_1".into(),
                name: CODEX_NAME.into(),
                input: json!({"title": "Map", "html": "<svg/>"}),
            }));
            let [(_, visual)] = visuals(&t).try_into().unwrap();
            assert_eq!(visual.status, Shown::Checking);
            assert_eq!(visual.id, "call_1");
        }

        #[test]
        fn a_failed_call_says_why() {
            let mut t = Transcript::default();
            start(&mut t, "v1", json!({"title": "T", "html": "<p>"}));
            complete(
                &mut t,
                "v1",
                "The visual could not be rendered: engine crashed",
                true,
            );
            let [(_, visual)] = visuals(&t).try_into().unwrap();
            assert_eq!(
                visual.status,
                Shown::Failed("The visual could not be rendered: engine crashed".into())
            );
        }

        #[test]
        fn a_shown_replacement_folds_the_previous_visual_away() {
            let mut t = Transcript::default();
            start(&mut t, "v1", json!({"title": "A", "html": "<p>1</p>"}));
            complete(&mut t, "v1", "ok", false);
            start(&mut t, "other", json!({"title": "B", "html": "<p>b</p>"}));
            complete(&mut t, "other", "ok", false);
            // A replacement that fails replaces nothing.
            start(
                &mut t,
                "v2",
                json!({"title": "B2", "html": "<p>", "replace": true}),
            );
            complete(&mut t, "v2", "bad", true);
            assert!(visuals(&t).iter().all(|(_, visual)| !visual.replaced));
            // One that shows replaces the visual before it, not older ones.
            start(
                &mut t,
                "v3",
                json!({"title": "B3", "html": "<p>3</p>", "replace": true}),
            );
            let update = complete(&mut t, "v3", "ok", false);
            let rows = visuals(&t);
            let replaced: Vec<&str> = rows
                .iter()
                .filter(|(_, visual)| visual.replaced)
                .map(|(_, visual)| visual.id.as_str())
                .collect();
            assert_eq!(replaced, vec!["v2"]);
            assert_eq!(update.dirty.len(), 2, "the new row and the one it replaced");
            // The next replacement skips what is already replaced.
            start(
                &mut t,
                "v4",
                json!({"title": "B4", "html": "<p>4</p>", "replace": true}),
            );
            complete(&mut t, "v4", "ok", false);
            let replaced: Vec<String> = visuals(&t)
                .into_iter()
                .filter(|(_, visual)| visual.replaced)
                .map(|(_, visual)| visual.id)
                .collect();
            assert_eq!(replaced, vec!["v2".to_string(), "v3".to_string()]);
        }

        #[test]
        fn a_turn_that_ends_first_leaves_the_visual_failed() {
            let mut t = Transcript::default();
            draft(&mut t, "v1", json!({"title": "T", "html": "<p"}));
            t.apply(Input::Event(SessionEvent::TurnEnded {
                outcome: TurnOutcome::Interrupted,
                cost_usd: None,
            }));
            let [(_, visual)] = visuals(&t).try_into().unwrap();
            assert_eq!(visual.status, Shown::Failed(VISUAL_UNFINISHED.into()));
            // A result arriving after that is still honoured.
            complete(&mut t, "v1", "ok", false);
            let [(_, visual)] = visuals(&t).try_into().unwrap();
            assert_eq!(visual.status, Shown::Shown);
        }

        #[test]
        fn other_tools_never_draft() {
            let mut t = Transcript::default();
            let update = t.apply(Input::Event(SessionEvent::ToolDraft {
                id: "b".into(),
                name: "Bash".into(),
                input: json!({"command": "ls"}),
            }));
            assert!(update.dirty.is_empty());
            assert!(t.blocks().is_empty());
        }
    }
}
