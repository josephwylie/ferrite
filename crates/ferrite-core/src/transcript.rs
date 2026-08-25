//! Folds SessionEvents into stable-identity Blocks.
//!
//! Events in, Blocks out — no window, no timers. Every apply reports exactly
//! which Blocks changed. That is the seam the wall will repaint through; the
//! single Pane shipping today still redraws whole and drops the report.

use std::sync::Arc;

use crate::{Hunk, SessionEvent, ToolResult};

mod highlight;
pub use highlight::Lexer;

/// A Block's identity, stable for as long as the Block lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(u64);

/// One rendered unit of the transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub id: BlockId,
    pub body: Body,
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
}

/// A tool call as one collapsed row: what ran, on what, how it went.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolBlock {
    /// The provider's id for this call — what a later result quotes.
    pub call: String,
    pub name: String,
    /// One line naming what the call touched, for a row that never wraps.
    pub summary: String,
    pub state: ToolState,
    /// The patch this call applied, when it was a file edit. A row with one
    /// draws as a diff card; a row without stays a single line.
    pub diff: Option<Diff>,
    /// The first line of the tool's output, trimmed to a row — what the
    /// Pane's `⎿` continuation shows (DirectionDense). Errors carry their
    /// message in `state` instead; the rest of the output was the model's
    /// to read, never Ferrite's to keep.
    pub result_line: Option<String>,
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
        let lines = || hunks.iter().flat_map(|hunk| hunk.lines.iter());
        Self {
            added: lines().filter(|line| line.starts_with('+')).count(),
            removed: lines().filter(|line| line.starts_with('-')).count(),
            path,
            hunks,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolState {
    Running,
    Ok,
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

/// How much of the model's context a Thread has spent. Codex reports this and
/// never reports dollars; Claude reports dollars and not this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub total_tokens: u64,
    pub context_window: Option<u64>,
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
    usage: Option<Usage>,
    /// Which reasoning summary part the tail Block belongs to.
    summary_index: Option<u64>,
    /// The Thread's plan: how many steps it made, and which it has finished.
    /// Ids rather than a count, because a step can be completed twice.
    planned: usize,
    completed: std::collections::BTreeSet<String>,
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
    pub fn new(highlighter: Arc<dyn Highlighter>) -> Self {
        Self::with_capacity(highlighter, DEFAULT_CAPACITY)
    }

    pub fn with_capacity(highlighter: Arc<dyn Highlighter>, capacity: usize) -> Self {
        Self {
            blocks: Vec::new(),
            last_id: 0,
            open: None,
            source: String::new(),
            highlighter,
            capacity,
            status: Status::default(),
            model: None,
            session_id: None,
            last_cost: None,
            usage: None,
            summary_index: None,
            planned: 0,
            completed: std::collections::BTreeSet::new(),
        }
    }

    pub fn status(&self) -> Status {
        self.status
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

    pub fn usage(&self) -> Option<Usage> {
        self.usage
    }

    /// The Thread's plan, once it has made one.
    pub fn todos(&self) -> Option<Todos> {
        (self.planned > 0).then_some(Todos {
            // The CLI assigns task ids and TaskCreate never echoes them, so a
            // completion cannot be matched to the step it finished. Clamping
            // is the honest bound: a Pane may under-report, never overshoot.
            done: self.completed.len().min(self.planned),
            total: self.planned,
        })
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn apply(&mut self, input: Input) -> Update {
        let mut update = self.fold(input);
        update.evicted = self.evict();
        update
    }

    /// Exhaustive by construction: a new SessionEvent variant fails to compile
    /// here until someone decides what a Pane shows for it. That is the point
    /// of a superset event model — a wildcard would silently render nothing.
    fn fold(&mut self, input: Input) -> Update {
        match input {
            Input::Event(SessionEvent::TextDelta { text }) => {
                self.status = Status::Streaming;
                Update {
                    dirty: self.grow(&text),
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::ThinkingDelta { text }) => {
                self.status = Status::Streaming;
                self.summary_index = None;
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
                // The provider decides where its reasoning breaks; a new index
                // is a new paragraph, not a continuation of the last one.
                if self.summary_index != Some(summary_index) {
                    self.summary_index = Some(summary_index);
                    return Update {
                        dirty: vec![self.push(Body::Thinking(text))],
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
                ..
            }) => {
                self.usage = Some(Usage {
                    total_tokens,
                    context_window,
                });
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
            Input::Revived => Update {
                dirty: vec![self.push(Body::Meta(
                    "revived — new Session, history from the log".into(),
                ))],
                ..Update::default()
            },
            Input::Notice(line) => Update {
                dirty: vec![self.push(Body::Notice(line))],
                ..Update::default()
            },
            Input::Prompt(line) => {
                // A Closed session stays closed and a Blocked one stays
                // blocked: nothing is streaming in either.
                if let Status::Idle | Status::Streaming = self.status {
                    self.status = Status::Streaming;
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
                let state = if is_error {
                    ToolState::Failed(trim(&output, ERROR_CHARS))
                } else {
                    ToolState::Ok
                };
                let diff = match result {
                    ToolResult::FileEdit { path, hunks } => Some(Diff::new(path, hunks)),
                    _ => None,
                };
                // A failure already carries its message in the state; a
                // success keeps its first output line for the `⎿` row.
                let result_line = (!is_error).then(|| result_line(&output)).flatten();
                Update {
                    dirty: self
                        .settle_tool(&id, state, diff, result_line)
                        .into_iter()
                        .collect(),
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::ToolStarted { id, name, input }) => {
                self.plan(&name, &input);
                let block = self.push(Body::Tool(ToolBlock {
                    call: id,
                    summary: tool_summary(&input),
                    name,
                    state: ToolState::Running,
                    diff: None,
                    result_line: None,
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
            Input::Event(SessionEvent::TurnEnded { outcome, cost_usd }) => {
                self.status = Status::Idle;
                self.last_cost = cost_usd;
                let mut dirty = Vec::new();
                match outcome {
                    crate::TurnOutcome::Completed => {
                        if let Some(cost) = cost_usd {
                            dirty.push(self.push(Body::Meta(format!("${cost:.4}"))));
                        }
                    }
                    crate::TurnOutcome::Interrupted => {
                        dirty.push(self.push(Body::Meta("interrupted".into())))
                    }
                    crate::TurnOutcome::Error(message) => {
                        dirty.push(self.push(Body::Notice(message)))
                    }
                }
                Update {
                    dirty,
                    boundary: Some(Boundary::TurnEnded),
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::DecisionRequested { decision }) => {
                self.status = Status::Blocked;
                Update {
                    dirty: vec![self.push(Body::Notice(format!(
                        "decision needed: {} — {}",
                        decision.tool_name, decision.description
                    )))],
                    ..Update::default()
                }
            }
            Input::Event(SessionEvent::Closed { reason }) => {
                self.status = Status::Closed;
                Update {
                    dirty: vec![self.push(Body::Notice(reason))],
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
                // but it is not dirty — dirty means the body actually moved.
                let changed = self.write_open(body);
                let settled = changed.or(self.open);
                dirty.extend(changed);
                if let Some(id) = settled {
                    self.ask_to_highlight(id);
                }
            }
            self.open = None;
        }
        if let Some(body) = parse_section(&self.source) {
            dirty.extend(self.write_open(body));
        }
        dirty
    }

    /// Watch the Thread plan and tick its own work off. Nothing is stored but
    /// the counts: the plan's prose is already in the tool rows.
    fn plan(&mut self, name: &str, input: &serde_json::Value) {
        match name {
            "TaskCreate" => self.planned += 1,
            "TaskUpdate" if input.get("status").and_then(|s| s.as_str()) == Some("completed") => {
                if let Some(task) = input.get("taskId").and_then(|id| id.as_str()) {
                    self.completed.insert(task.to_string());
                }
            }
            _ => {}
        }
    }

    /// Thinking streams like prose but never shares a Block with the answer.
    fn grow_thinking(&mut self, text: &str) -> BlockId {
        if let Some(Block {
            id,
            body: Body::Thinking(thought),
        }) = self.blocks.last_mut()
        {
            thought.push_str(text);
            let id = *id;
            self.open = None;
            self.source.clear();
            return id;
        }
        self.push(Body::Thinking(text.to_string()))
    }

    /// Write a folded body into the open Block, creating it on first content.
    /// Reports the Block only when its body actually changed — a delta that
    /// adds nothing renderable (a lone newline) dirties nothing.
    fn write_open(&mut self, body: Body) -> Option<BlockId> {
        match self.open {
            Some(id) => {
                // The open Block is the tail by construction. Scanning for it
                // costs the whole transcript on every delta, which is what
                // decays a streaming cockpit from 120fps to 30.
                let block = match self.blocks.last_mut() {
                    Some(block) if block.id == id => block,
                    _ => self.blocks.iter_mut().find(|b| b.id == id)?,
                };
                if block.body == body {
                    return None;
                }
                block.body = body;
                Some(id)
            }
            None => {
                let id = self.mint();
                self.blocks.push(Block { id, body });
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
        diff: Option<Diff>,
        result_line: Option<String>,
    ) -> Option<BlockId> {
        let block = self
            .blocks
            .iter_mut()
            .find(|block| matches!(&block.body, Body::Tool(tool) if tool.call == call))?;
        let Body::Tool(tool) = &mut block.body else {
            return None;
        };
        if tool.state == state && tool.diff == diff && tool.result_line == result_line {
            return None;
        }
        tool.state = state;
        tool.diff = diff;
        tool.result_line = result_line;
        Some(block.id)
    }

    /// Append a Block that no further text can join.
    fn push(&mut self, body: Body) -> BlockId {
        let id = self.mint();
        self.blocks.push(Block { id, body });
        self.open = None;
        self.source.clear();
        id
    }

    fn mint(&mut self) -> BlockId {
        self.last_id += 1;
        BlockId(self.last_id)
    }
}

/// How much of a tool failure a row carries; the model got all of it.
const ERROR_CHARS: usize = 200;

/// How much of a tool's output its `⎿` continuation row carries — one line
/// that never wraps at transcript density.
const RESULT_CHARS: usize = 80;

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

/// The one line a collapsed tool row shows. Tool inputs are the vendor's own
/// schema, so this reads the few keys that name a subject and gives up
/// quietly on anything else rather than guessing.
fn tool_summary(input: &serde_json::Value) -> String {
    for key in ["command", "file_path", "path", "pattern", "url"] {
        if let Some(value) = input.get(key).and_then(|v| v.as_str()) {
            return value.to_string();
        }
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

/// Split a line on backticks: the odd runs are inline code.
fn spans(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    for (i, run) in text.split('`').enumerate() {
        if run.is_empty() {
            continue;
        }
        spans.push(Span {
            text: run.to_string(),
            style: if i % 2 == 1 {
                Style::Code
            } else {
                Style::Plain
            },
        });
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Decision;

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
    fn a_blank_line_starts_a_new_paragraph() {
        let mut transcript = Transcript::default();
        transcript.apply(text("first para"));

        // Deltas split words and newlines wherever the provider felt like it.
        transcript.apply(text("\n"));
        let update = transcript.apply(text("\nsecond para"));

        assert_eq!(transcript.blocks().len(), 2);
        assert_eq!(body_text(&transcript.blocks()[0]), "first para");
        assert_eq!(body_text(&transcript.blocks()[1]), "second para");
        assert_eq!(update.dirty, vec![transcript.blocks()[1].id]);
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

    #[test]
    fn a_paid_turn_leaves_its_cost_in_the_transcript() {
        let mut transcript = Transcript::default();
        transcript.apply(text("done"));

        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: Some(0.038),
        }));

        assert_eq!(transcript.last_cost(), Some(0.038));
        let last = transcript.blocks().last().unwrap();
        assert!(matches!(last.body, Body::Meta(_)));
        assert_eq!(body_text(last), "$0.0380");
    }

    #[test]
    fn a_decision_blocks_the_session_and_says_what_is_waiting() {
        let mut transcript = Transcript::default();

        transcript.apply(Input::Event(SessionEvent::DecisionRequested {
            decision: Decision {
                id: "perm_01".into(),
                tool_use_id: "toolu_01".into(),
                tool_name: "Write".into(),
                description: "ferrite-perm.txt".into(),
                input: serde_json::Value::Null,
                suggestions: vec![],
            },
        }));

        assert_eq!(transcript.status(), Status::Blocked);
        let last = transcript.blocks().last().unwrap();
        assert!(matches!(last.body, Body::Notice(_)));
        assert_eq!(body_text(last), "decision needed: Write — ferrite-perm.txt");
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

    /// The shapes are the committed `todo` capture's, not remembered ones:
    /// 2.1.243 has no TodoWrite — it plans with TaskCreate/TaskUpdate.
    #[test]
    fn a_planned_todo_list_is_counted_as_it_is_worked() {
        let mut transcript = Transcript::default();
        assert_eq!(transcript.todos(), None, "a Thread with no plan has none");

        for subject in ["init git", "add docs", "make dirs"] {
            transcript.apply(started(
                &format!("t{subject}"),
                "TaskCreate",
                serde_json::json!({ "subject": subject, "activeForm": subject }),
            ));
        }
        transcript.apply(started(
            "u1",
            "TaskUpdate",
            serde_json::json!({ "taskId": "1", "status": "completed" }),
        ));

        assert_eq!(transcript.todos(), Some(Todos { done: 1, total: 3 }));

        // A status that is not completion moves nothing.
        transcript.apply(started(
            "u2",
            "TaskUpdate",
            serde_json::json!({ "taskId": "2", "status": "in_progress" }),
        ));
        assert_eq!(transcript.todos(), Some(Todos { done: 1, total: 3 }));

        // And completing the same task twice is still one task done.
        transcript.apply(started(
            "u3",
            "TaskUpdate",
            serde_json::json!({ "taskId": "1", "status": "completed" }),
        ));
        assert_eq!(transcript.todos(), Some(Todos { done: 1, total: 3 }));
    }

    /// The CLI assigns task ids and never echoes them back on TaskCreate, so
    /// a completion cannot be matched to a creation. What can be promised is
    /// that the count never overshoots: "2/1 done" is nonsense on a Pane.
    #[test]
    fn finished_work_never_outruns_the_plan() {
        let mut transcript = Transcript::default();
        transcript.apply(started(
            "c1",
            "TaskCreate",
            serde_json::json!({ "subject": "the only step" }),
        ));
        for task in ["1", "2", "3"] {
            transcript.apply(started(
                &format!("u{task}"),
                "TaskUpdate",
                serde_json::json!({ "taskId": task, "status": "completed" }),
            ));
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
                }],
            },
        }));

        assert_eq!(update.dirty, vec![row]);
        match &transcript.blocks()[0].body {
            Body::Tool(tool) => {
                let diff = tool.diff.as_ref().expect("an edit carries a diff card");
                assert_eq!(diff.path, "/workspace/x.txt");
                assert_eq!((diff.added, diff.removed), (1, 1));
                assert_eq!(diff.hunks.len(), 1);
            }
            other => panic!("expected a tool row, got {other:?}"),
        }
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

        // An overlong line is cut to a row, marked.
        let mut long = Transcript::default();
        long.apply(started("toolu_2", "Bash", serde_json::Value::Null));
        long.apply(completed("toolu_2", &"x".repeat(500), false));
        let Body::Tool(tool) = &long.blocks()[0].body else {
            panic!("expected a tool row")
        };
        let line = tool.result_line.as_deref().unwrap();
        assert_eq!(line.chars().count(), 81);
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
        assert!(matches!(last.body, Body::Meta(_)));
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
        assert!(matches!(last.body, Body::Notice(_)));
        assert_eq!(body_text(last), "model overloaded");
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
