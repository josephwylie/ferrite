//! Schema 9's owned representation. Live Activity types deliberately lack serde.

use super::{Outcome, PersistedProgress, PersistedToolResult};
use crate::activity::{
    ActivityEvent, AgentInfo, AgentKey, AgentStatus, ExecutionEvent, Subject, TranscriptCoverage,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum PersistedActivity {
    Discovered {
        key: String,
        parent: Option<PersistedSubject>,
        name: Option<String>,
        description: Option<String>,
        agent_kind: Option<String>,
        coverage: Coverage,
    },
    Status {
        key: String,
        state: Status,
    },
    Content {
        key: String,
        id: Option<String>,
        event: Execution,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    HistoryContent {
        key: String,
        id: Option<String>,
        event: Execution,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    Coverage {
        key: String,
        coverage: Coverage,
    },
    Detached {
        key: String,
    },
    MainContent {
        id: Option<String>,
        event: Execution,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    BackgroundTurnEnded {
        outcome: Outcome,
        cost_usd: Option<f64>,
    },
    Alias {
        from: String,
        to: String,
    },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum PersistedSubject {
    Main,
    Subagent { key: String },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Coverage {
    Unavailable,
    ToolActivity,
    Live,
    Partial,
    Complete,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Status {
    Unknown,
    Pending,
    Working,
    Waiting,
    Idle,
    Paused,
    Interrupted,
    Failed,
    Shutdown,
    NotFound,
    NotLoaded,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Execution {
    Progress {
        event: PersistedProgress,
    },
    ContentBoundary,
    ReasoningSummaryPart {
        item_id: String,
        summary_index: u64,
        text: String,
        snapshot: bool,
    },
    ToolOutputDelta {
        id: String,
        text: String,
    },
    TextDelta {
        text: String,
    },
    ThinkingDelta {
        text: String,
    },
    ReasoningSummaryDelta {
        text: String,
        summary_index: u64,
    },
    Text {
        text: String,
    },
    Thinking {
        text: String,
    },
    TextSnapshot {
        text: String,
    },
    ThinkingSnapshot {
        text: String,
    },
    Prompt {
        text: String,
    },
    Notice {
        text: String,
    },
    ToolStarted {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolCompleted {
        id: String,
        output: String,
        is_error: bool,
        result: PersistedToolResult,
    },
    TurnEnded {
        outcome: Outcome,
        cost_usd: Option<f64>,
    },
    TokenUsage {
        total_tokens: u64,
        input_tokens: u64,
        cached_input_tokens: u64,
        output_tokens: u64,
        reasoning_output_tokens: u64,
        context_window: Option<u64>,
    },
}

impl PersistedSubject {
    fn from_live(subject: &Subject) -> Self {
        match subject {
            Subject::Main => Self::Main,
            Subject::Subagent(key) => Self::Subagent {
                key: key.as_str().into(),
            },
        }
    }
    fn live(&self) -> Subject {
        match self {
            Self::Main => Subject::Main,
            Self::Subagent { key } => Subject::Subagent(AgentKey::from_stored(key.clone())),
        }
    }
}

impl Coverage {
    fn from_live(coverage: TranscriptCoverage) -> Self {
        match coverage {
            TranscriptCoverage::Unavailable => Self::Unavailable,
            TranscriptCoverage::ToolActivity => Self::ToolActivity,
            TranscriptCoverage::Live => Self::Live,
            TranscriptCoverage::Partial => Self::Partial,
            TranscriptCoverage::Complete => Self::Complete,
        }
    }
    fn live(&self) -> TranscriptCoverage {
        match self {
            Self::Unavailable => TranscriptCoverage::Unavailable,
            Self::ToolActivity => TranscriptCoverage::ToolActivity,
            Self::Live => TranscriptCoverage::Live,
            Self::Partial => TranscriptCoverage::Partial,
            Self::Complete => TranscriptCoverage::Complete,
        }
    }
}

impl Status {
    fn from_live(state: AgentStatus) -> Self {
        match state {
            AgentStatus::Unknown => Self::Unknown,
            AgentStatus::Pending => Self::Pending,
            AgentStatus::Working => Self::Working,
            AgentStatus::Waiting => Self::Waiting,
            AgentStatus::Idle => Self::Idle,
            AgentStatus::Paused => Self::Paused,
            AgentStatus::Interrupted => Self::Interrupted,
            AgentStatus::Failed => Self::Failed,
            AgentStatus::Shutdown => Self::Shutdown,
            AgentStatus::NotFound => Self::NotFound,
            AgentStatus::NotLoaded => Self::NotLoaded,
        }
    }
    fn live(&self) -> AgentStatus {
        match self {
            Self::Unknown => AgentStatus::Unknown,
            Self::Pending => AgentStatus::Pending,
            Self::Working => AgentStatus::Working,
            Self::Waiting => AgentStatus::Waiting,
            Self::Idle => AgentStatus::Idle,
            Self::Paused => AgentStatus::Paused,
            Self::Interrupted => AgentStatus::Interrupted,
            Self::Failed => AgentStatus::Failed,
            Self::Shutdown => AgentStatus::Shutdown,
            Self::NotFound => AgentStatus::NotFound,
            Self::NotLoaded => AgentStatus::NotLoaded,
        }
    }
}

impl Outcome {
    pub(super) fn from_live(outcome: &crate::TurnOutcome) -> Self {
        match outcome {
            crate::TurnOutcome::Completed => Self::Completed,
            crate::TurnOutcome::Interrupted => Self::Interrupted,
            crate::TurnOutcome::Error(error) => Self::Error(error.clone()),
        }
    }
    pub(super) fn live(&self) -> crate::TurnOutcome {
        match self {
            Self::Completed => crate::TurnOutcome::Completed,
            Self::Interrupted => crate::TurnOutcome::Interrupted,
            Self::Error(error) => crate::TurnOutcome::Error(error.clone()),
        }
    }
}

impl Execution {
    fn from_live(event: &ExecutionEvent) -> Self {
        match event {
            ExecutionEvent::Progress { event } => Self::Progress {
                event: PersistedProgress::from_live(event),
            },
            ExecutionEvent::ContentBoundary => Self::ContentBoundary,
            ExecutionEvent::ReasoningSummaryPart {
                item_id,
                summary_index,
                text,
                snapshot,
            } => Self::ReasoningSummaryPart {
                item_id: item_id.clone(),
                summary_index: *summary_index,
                text: text.clone(),
                snapshot: *snapshot,
            },
            ExecutionEvent::ToolOutputDelta { id, text } => Self::ToolOutputDelta {
                id: id.clone(),
                text: text.clone(),
            },
            ExecutionEvent::TextDelta { text } => Self::TextDelta { text: text.clone() },
            ExecutionEvent::ThinkingDelta { text } => Self::ThinkingDelta { text: text.clone() },
            ExecutionEvent::ReasoningSummaryDelta {
                text,
                summary_index,
            } => Self::ReasoningSummaryDelta {
                text: text.clone(),
                summary_index: *summary_index,
            },
            ExecutionEvent::Text { text } => Self::Text { text: text.clone() },
            ExecutionEvent::Thinking { text } => Self::Thinking { text: text.clone() },
            ExecutionEvent::TextSnapshot { text } => Self::TextSnapshot { text: text.clone() },
            ExecutionEvent::ThinkingSnapshot { text } => {
                Self::ThinkingSnapshot { text: text.clone() }
            }
            ExecutionEvent::Prompt { text } => Self::Prompt { text: text.clone() },
            ExecutionEvent::Notice { text } => Self::Notice { text: text.clone() },
            ExecutionEvent::ToolStarted { id, name, input } => Self::ToolStarted {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            },
            ExecutionEvent::ToolCompleted {
                id,
                output,
                is_error,
                result,
            } => Self::ToolCompleted {
                id: id.clone(),
                output: output.clone(),
                is_error: *is_error,
                result: PersistedToolResult::from_live(result),
            },
            ExecutionEvent::TurnEnded { outcome, cost_usd } => Self::TurnEnded {
                outcome: Outcome::from_live(outcome),
                cost_usd: *cost_usd,
            },
            ExecutionEvent::TokenUsage {
                total_tokens,
                input_tokens,
                cached_input_tokens,
                output_tokens,
                reasoning_output_tokens,
                context_window,
            } => Self::TokenUsage {
                total_tokens: *total_tokens,
                input_tokens: *input_tokens,
                cached_input_tokens: *cached_input_tokens,
                output_tokens: *output_tokens,
                reasoning_output_tokens: *reasoning_output_tokens,
                context_window: *context_window,
            },
        }
    }
    pub(super) fn live(&self) -> ExecutionEvent {
        match self {
            Self::Progress { event } => ExecutionEvent::Progress {
                event: event.live(),
            },
            Self::ContentBoundary => ExecutionEvent::ContentBoundary,
            Self::ReasoningSummaryPart {
                item_id,
                summary_index,
                text,
                snapshot,
            } => ExecutionEvent::ReasoningSummaryPart {
                item_id: item_id.clone(),
                summary_index: *summary_index,
                text: text.clone(),
                snapshot: *snapshot,
            },
            Self::ToolOutputDelta { id, text } => ExecutionEvent::ToolOutputDelta {
                id: id.clone(),
                text: text.clone(),
            },
            Self::TextDelta { text } => ExecutionEvent::TextDelta { text: text.clone() },
            Self::ThinkingDelta { text } => ExecutionEvent::ThinkingDelta { text: text.clone() },
            Self::ReasoningSummaryDelta {
                text,
                summary_index,
            } => ExecutionEvent::ReasoningSummaryDelta {
                text: text.clone(),
                summary_index: *summary_index,
            },
            Self::Text { text } => ExecutionEvent::Text { text: text.clone() },
            Self::Thinking { text } => ExecutionEvent::Thinking { text: text.clone() },
            Self::TextSnapshot { text } => ExecutionEvent::TextSnapshot { text: text.clone() },
            Self::ThinkingSnapshot { text } => {
                ExecutionEvent::ThinkingSnapshot { text: text.clone() }
            }
            Self::Prompt { text } => ExecutionEvent::Prompt { text: text.clone() },
            Self::Notice { text } => ExecutionEvent::Notice { text: text.clone() },
            Self::ToolStarted { id, name, input } => ExecutionEvent::ToolStarted {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            },
            Self::ToolCompleted {
                id,
                output,
                is_error,
                result,
            } => ExecutionEvent::ToolCompleted {
                id: id.clone(),
                output: output.clone(),
                is_error: *is_error,
                result: result.live(),
            },
            Self::TurnEnded { outcome, cost_usd } => ExecutionEvent::TurnEnded {
                outcome: outcome.live(),
                cost_usd: *cost_usd,
            },
            Self::TokenUsage {
                total_tokens,
                input_tokens,
                cached_input_tokens,
                output_tokens,
                reasoning_output_tokens,
                context_window,
            } => ExecutionEvent::TokenUsage {
                total_tokens: *total_tokens,
                input_tokens: *input_tokens,
                cached_input_tokens: *cached_input_tokens,
                output_tokens: *output_tokens,
                reasoning_output_tokens: *reasoning_output_tokens,
                context_window: *context_window,
            },
        }
    }
}

impl PersistedActivity {
    pub(super) fn from_live(event: &ActivityEvent, duration: Option<Duration>) -> Option<Self> {
        let duration_ms =
            duration.map(|duration| duration.as_millis().min(u64::MAX as u128) as u64);
        Some(match event {
            ActivityEvent::Discovered(info) => Self::Discovered {
                key: info.key.as_str().into(),
                parent: info.parent.as_ref().map(PersistedSubject::from_live),
                name: info.name.clone(),
                description: info.description.clone(),
                agent_kind: info.kind.clone(),
                coverage: Coverage::from_live(info.coverage),
            },
            ActivityEvent::Status { key, state } => Self::Status {
                key: key.as_str().into(),
                state: Status::from_live(*state),
            },
            ActivityEvent::Content { key, id, event } => Self::Content {
                key: key.as_str().into(),
                id: id.clone(),
                event: Execution::from_live(event),
                duration_ms: matches!(event, ExecutionEvent::ToolCompleted { .. })
                    .then_some(duration_ms)
                    .flatten(),
            },
            ActivityEvent::HistoryContent { key, id, event } => Self::HistoryContent {
                key: key.as_str().into(),
                id: id.clone(),
                event: Execution::from_live(event),
                duration_ms: matches!(event, ExecutionEvent::ToolCompleted { .. })
                    .then_some(duration_ms)
                    .flatten(),
            },
            ActivityEvent::Coverage { key, coverage } => Self::Coverage {
                key: key.as_str().into(),
                coverage: Coverage::from_live(*coverage),
            },
            ActivityEvent::Detached { key } => Self::Detached {
                key: key.as_str().into(),
            },
            ActivityEvent::MainContent { id, event } => Self::MainContent {
                id: id.clone(),
                event: Execution::from_live(event),
                duration_ms: matches!(event, ExecutionEvent::ToolCompleted { .. })
                    .then_some(duration_ms)
                    .flatten(),
            },
            ActivityEvent::BackgroundTurnEnded { outcome, cost_usd } => Self::BackgroundTurnEnded {
                outcome: Outcome::from_live(outcome),
                cost_usd: *cost_usd,
            },
            ActivityEvent::Alias { from, to } => Self::Alias {
                from: from.as_str().into(),
                to: to.as_str().into(),
            },
            // Replayed request IDs cannot authorize a new Session. Cancellation
            // only retires one of those live handles and is equally ephemeral.
            ActivityEvent::Decision { .. }
            | ActivityEvent::DecisionCancelled { .. }
            | ActivityEvent::DecisionReply { .. } => {
                return None;
            }
        })
    }
    pub(super) fn live(&self) -> ActivityEvent {
        match self {
            Self::Discovered {
                key,
                parent,
                name,
                description,
                agent_kind,
                coverage,
            } => ActivityEvent::Discovered(AgentInfo {
                key: AgentKey::from_stored(key.clone()),
                parent: parent.as_ref().map(PersistedSubject::live),
                name: name.clone(),
                description: description.clone(),
                kind: agent_kind.clone(),
                coverage: coverage.live(),
            }),
            Self::Status { key, state } => ActivityEvent::Status {
                key: AgentKey::from_stored(key.clone()),
                state: state.live(),
            },
            Self::Content { key, id, event, .. } => ActivityEvent::Content {
                key: AgentKey::from_stored(key.clone()),
                id: id.clone(),
                event: event.live(),
            },
            Self::HistoryContent { key, id, event, .. } => ActivityEvent::HistoryContent {
                key: AgentKey::from_stored(key.clone()),
                id: id.clone(),
                event: event.live(),
            },
            Self::Coverage { key, coverage } => ActivityEvent::Coverage {
                key: AgentKey::from_stored(key.clone()),
                coverage: coverage.live(),
            },
            Self::Detached { key } => ActivityEvent::Detached {
                key: AgentKey::from_stored(key.clone()),
            },
            Self::MainContent { id, event, .. } => ActivityEvent::MainContent {
                id: id.clone(),
                event: event.live(),
            },
            Self::BackgroundTurnEnded { outcome, cost_usd } => ActivityEvent::BackgroundTurnEnded {
                outcome: outcome.live(),
                cost_usd: *cost_usd,
            },
            Self::Alias { from, to } => ActivityEvent::Alias {
                from: AgentKey::from_stored(from.clone()),
                to: AgentKey::from_stored(to.clone()),
            },
        }
    }
    pub(super) fn is_boundary(&self) -> bool {
        matches!(
            self,
            Self::Status {
                state: Status::Idle
                    | Status::Interrupted
                    | Status::Failed
                    | Status::Shutdown
                    | Status::NotFound,
                ..
            } | Self::Content {
                event: Execution::TurnEnded { .. },
                ..
            } | Self::HistoryContent {
                event: Execution::TurnEnded { .. },
                ..
            } | Self::MainContent {
                event: Execution::TurnEnded { .. },
                ..
            } | Self::BackgroundTurnEnded { .. }
        )
    }
    pub(super) fn tool_duration(&self) -> Option<(Subject, String, Duration)> {
        match self {
            Self::Content {
                key,
                event: Execution::ToolCompleted { id, .. },
                duration_ms: Some(ms),
                ..
            }
            | Self::HistoryContent {
                key,
                event: Execution::ToolCompleted { id, .. },
                duration_ms: Some(ms),
                ..
            } => Some((
                Subject::Subagent(AgentKey::from_stored(key.clone())),
                id.clone(),
                Duration::from_millis(*ms),
            )),
            Self::MainContent {
                event: Execution::ToolCompleted { id, .. },
                duration_ms: Some(ms),
                ..
            } => Some((Subject::Main, id.clone(), Duration::from_millis(*ms))),
            _ => None,
        }
    }
}

#[derive(Default)]
pub(super) struct Aliases(std::collections::HashMap<String, String>);

impl Aliases {
    pub(super) fn new(records: &[super::Record]) -> Self {
        Self(
            records
                .iter()
                .filter_map(|record| match record {
                    super::Record::Activity {
                        observation: PersistedActivity::Alias { from, to },
                    } => Some((from.clone(), to.clone())),
                    _ => None,
                })
                .collect(),
        )
    }

    fn resolve(&self, start: &str) -> String {
        let mut key = start;
        let mut seen = std::collections::HashSet::new();
        while seen.insert(key) {
            let Some(next) = self.0.get(key) else {
                break;
            };
            key = next;
        }
        key.to_string()
    }

    pub(super) fn subject(&self, subject: Subject) -> Subject {
        match subject {
            Subject::Main => Subject::Main,
            Subject::Subagent(key) => {
                Subject::Subagent(AgentKey::from_stored(self.resolve(key.as_str())))
            }
        }
    }
}

/// Reads a single stable inode twice: aliases first, then the bounded child
/// projection. The checkpoint limits both scans before JSON decoding.
/// Regular Thread loading deliberately keeps its existing snapshot behavior.
pub(super) fn read_agent_inputs(
    mut file: std::fs::File,
    thread: crate::ThreadId,
    requested: &AgentKey,
    through: u64,
    limits: crate::activity::ActivityLimits,
) -> Result<Vec<crate::activity::ActivityInput>, super::LoadError> {
    use std::io::Seek;
    if file.metadata()?.len() < through {
        return Err(super::LoadError::Corrupt {
            detail: format!("thread {thread} is shorter than its history checkpoint"),
        });
    }
    let mut aliases = Aliases::default();
    for record in RecordReader::new(&mut file, thread, through)? {
        if let super::Record::Activity {
            observation: PersistedActivity::Alias { from, to },
        } = record?
        {
            aliases.0.insert(from, to);
        }
    }
    file.rewind()?;
    project_agent(
        RecordReader::new(&mut file, thread, through)?,
        aliases,
        requested,
        limits,
    )
}

/// A crash may leave a torn UTF-8/JSON tail. Match Store::load by stopping at
/// the first unreadable body record; malformed/future headers still fail.
/// Peak temporary data is one encoded line and one decoded record. The line
/// itself is not capped, preserving old logs and unusually large tool output.
struct RecordReader<'a> {
    reader: std::io::BufReader<std::io::Take<&'a mut std::fs::File>>,
    line: Vec<u8>,
    ended: bool,
}

struct EncodedSize(usize);
impl std::io::Write for EncodedSize {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn encoded_size(observation: &PersistedActivity) -> usize {
    let mut size = EncodedSize(0);
    match serde_json::to_writer(&mut size, observation) {
        Ok(()) => size.0,
        Err(_) => usize::MAX,
    }
}
impl<'a> RecordReader<'a> {
    fn new(
        file: &'a mut std::fs::File,
        thread: crate::ThreadId,
        through: u64,
    ) -> Result<Self, super::LoadError> {
        use std::io::{BufRead, Read};
        let mut reader = std::io::BufReader::new(file.take(through));
        let mut line = Vec::new();
        reader.read_until(b'\n', &mut line)?;
        let header: super::Header =
            serde_json::from_slice(&line).map_err(|_| super::LoadError::Corrupt {
                detail: format!("thread {thread} has no readable header"),
            })?;
        if header.schema > super::SCHEMA_VERSION {
            return Err(super::LoadError::FutureSchema {
                found: header.schema,
                supported: super::SCHEMA_VERSION,
            });
        }
        Ok(Self {
            reader,
            line,
            ended: false,
        })
    }
}
impl Iterator for RecordReader<'_> {
    type Item = Result<super::Record, super::LoadError>;
    fn next(&mut self) -> Option<Self::Item> {
        use std::io::BufRead;
        if self.ended {
            return None;
        }
        self.line.clear();
        match self.reader.read_until(b'\n', &mut self.line) {
            Ok(0) => {
                self.ended = true;
                None
            }
            Ok(_) => match serde_json::from_slice(&self.line) {
                Ok(record) => Some(Ok(record)),
                Err(_) => {
                    self.ended = true;
                    None
                }
            },
            Err(error) => {
                self.ended = true;
                Some(Err(error.into()))
            }
        }
    }
}

#[cfg(test)]
pub(super) fn agent_inputs(
    records: &[super::Record],
    requested: &AgentKey,
    limits: crate::activity::ActivityLimits,
) -> Vec<crate::activity::ActivityInput> {
    project_agent(
        records.iter().map(Ok),
        Aliases::new(records),
        requested,
        limits,
    )
    .expect("infallible record slice")
}

fn project_agent<R: std::borrow::Borrow<super::Record>>(
    records: impl IntoIterator<Item = Result<R, super::LoadError>>,
    aliases: Aliases,
    requested: &AgentKey,
    limits: crate::activity::ActivityLimits,
) -> Result<Vec<crate::activity::ActivityInput>, super::LoadError> {
    use crate::activity::ActivityInput;
    use std::collections::{HashMap, VecDeque};

    let canonical = aliases.resolve(requested.as_str());
    let key = AgentKey::from_stored(canonical.clone());
    let mut info = AgentInfo::new(key.clone());
    let mut coverage = TranscriptCoverage::Unavailable;
    let mut seen = false;
    let mut detached = false;
    let mut omitted = false;
    let mut bytes = 0usize;
    let mut last_status = AgentStatus::Unknown;
    let mut content: VecDeque<(ActivityEvent, usize, Option<(String, Duration)>)> = VecDeque::new();
    for record in records {
        let record = record?;
        let super::Record::Activity { observation } = record.borrow() else {
            continue;
        };
        let observed_key = match observation {
            PersistedActivity::Discovered { key, .. }
            | PersistedActivity::Content { key, .. }
            | PersistedActivity::HistoryContent { key, .. }
            | PersistedActivity::Status { key, .. }
            | PersistedActivity::Coverage { key, .. }
            | PersistedActivity::Detached { key } => key,
            _ => continue,
        };
        if aliases.resolve(observed_key) != canonical {
            continue;
        }
        seen = true;
        let size = encoded_size(observation);
        let historical_content = match observation {
            PersistedActivity::Discovered { .. } => {
                let ActivityEvent::Discovered(discovered) = observation.live() else {
                    unreachable!()
                };
                if discovered.parent.is_some() {
                    info.parent = discovered.parent.map(|parent| aliases.subject(parent));
                    detached = false;
                }
                if discovered.name.is_some() {
                    info.name = discovered.name;
                }
                if discovered.description.is_some() {
                    info.description = discovered.description;
                }
                if discovered.kind.is_some() {
                    info.kind = discovered.kind;
                }
                if discovered.coverage != TranscriptCoverage::Unavailable {
                    coverage = discovered.coverage;
                }
                None
            }
            PersistedActivity::Coverage {
                coverage: value, ..
            } => {
                coverage = value.live();
                None
            }
            PersistedActivity::Detached { .. } => {
                detached = true;
                None
            }
            PersistedActivity::Status { state, .. } => {
                let previous = last_status;
                last_status = state.live();
                // Replaying Status against the current live status can omit
                // an old interruption/failure row. Preserve the historical
                // boundary as content, without resurrecting lifecycle state.
                let outcome = if previous == last_status {
                    None
                } else {
                    match last_status {
                        AgentStatus::Idle => Some(crate::TurnOutcome::Completed),
                        AgentStatus::Interrupted | AgentStatus::Shutdown => {
                            Some(crate::TurnOutcome::Interrupted)
                        }
                        AgentStatus::Failed => {
                            Some(crate::TurnOutcome::Error("Subagent failed".into()))
                        }
                        _ => None,
                    }
                };
                outcome.map(|outcome| {
                    (
                        None,
                        ExecutionEvent::TurnEnded {
                            outcome,
                            cost_usd: None,
                        },
                    )
                })
            }
            PersistedActivity::Content { id, event, .. }
            | PersistedActivity::HistoryContent { id, event, .. } => {
                if matches!(observation, PersistedActivity::Content { .. }) {
                    match event {
                        Execution::TextDelta { .. }
                        | Execution::ThinkingDelta { .. }
                        | Execution::ReasoningSummaryDelta { .. }
                        | Execution::ReasoningSummaryPart {
                            snapshot: false, ..
                        }
                        | Execution::ToolOutputDelta { .. }
                        | Execution::Text { .. }
                        | Execution::Thinking { .. }
                        | Execution::TextSnapshot { .. }
                        | Execution::ThinkingSnapshot { .. }
                        | Execution::Prompt { .. }
                        | Execution::ToolStarted { .. } => last_status = AgentStatus::Working,
                        Execution::TurnEnded { outcome, .. } => {
                            last_status = match outcome {
                                Outcome::Completed => AgentStatus::Idle,
                                Outcome::Interrupted => AgentStatus::Interrupted,
                                Outcome::Error(_) => AgentStatus::Failed,
                            }
                        }
                        _ => {}
                    }
                }
                if coverage == TranscriptCoverage::Unavailable {
                    coverage = TranscriptCoverage::Live;
                }
                if size > limits.content_bytes_per_subject {
                    omitted = true;
                    continue;
                }
                Some((id.clone(), event.live()))
            }
            _ => None,
        };
        let Some((id, event)) = historical_content else {
            continue;
        };
        // Serialization cost is a conservative proxy for retained strings/JSON.
        // A single oversized record stays only on disk.
        if size > limits.content_bytes_per_subject {
            omitted = true;
            continue;
        }
        let timing = observation
            .tool_duration()
            .map(|(_, id, duration)| (id, duration));
        content.push_back((
            ActivityEvent::HistoryContent {
                key: key.clone(),
                id,
                event,
            },
            size,
            timing,
        ));
        bytes = bytes.saturating_add(size);
        while bytes > limits.content_bytes_per_subject
            || content.len() > limits.blocks_per_subject.saturating_mul(8).max(1)
        {
            if let Some((_, removed, _)) = content.pop_front() {
                bytes = bytes.saturating_sub(removed);
                omitted = true;
            }
        }
    }
    if !seen {
        return Ok(Vec::new());
    }
    if omitted {
        coverage = TranscriptCoverage::Partial;
    }
    info.coverage = coverage;
    let subject = Subject::Subagent(key.clone());
    let mut inputs = vec![
        ActivityInput::Retain(subject.clone()),
        ActivityInput::ReplayEvent(ActivityEvent::Discovered(info)),
    ];
    let mut timings = HashMap::new();
    for (event, _, duration) in content {
        inputs.push(ActivityInput::ReplayEvent(event));
        if let Some((id, duration)) = duration {
            timings.insert(id, duration);
        }
    }
    inputs.push(ActivityInput::ReplayEvent(ActivityEvent::Coverage {
        key: key.clone(),
        coverage,
    }));
    inputs.push(ActivityInput::RestoreTimings { subject, timings });
    if detached {
        inputs.push(ActivityInput::ReplayEvent(ActivityEvent::Detached { key }));
    }
    Ok(inputs)
}
