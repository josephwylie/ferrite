//! Schema 9's owned representation. Live Activity types deliberately lack serde.

use super::{Outcome, PersistedProgress, PersistedToolResult};
use crate::activity::{
    ActivityEvent, AgentInfo, AgentKey, AgentStatus, ExecutionEvent, Subject, TranscriptCoverage,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum PersistedActivity {
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
    CompletionObservation {
        subject: PersistedSubject,
        elapsed_ms: u64,
        completed_at: String,
        /// The turn's token counts; absent from older logs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_tokens: Option<u64>,
    },
    Alias {
        from: String,
        to: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum PersistedSubject {
    Main,
    Subagent { key: String },
}

impl PersistedSubject {
    /// The child's stored key, or `None` for Main.
    pub(crate) fn key(&self) -> Option<&str> {
        match self {
            Self::Main => None,
            Self::Subagent { key } => Some(key),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Coverage {
    Unavailable,
    ToolActivity,
    Live,
    Partial,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Execution {
    FileChanges {
        id: String,
        edits: Vec<StoredFileEdit>,
    },
    TurnDiff {
        turn_id: String,
        diff: String,
    },
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
    Retract {
        ids: Vec<String>,
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
    ContextUsage {
        total_tokens: u64,
        context_window: Option<u64>,
    },
    UsageDetails {
        details: crate::UsageDetails,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct StoredFileEdit {
    path: String,
    hunks: Vec<StoredHunk>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct StoredHunk {
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
    lines: Vec<String>,
    /// The section its header named; absent from older logs (none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    section: Option<String>,
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
    pub(crate) fn live(&self) -> TranscriptCoverage {
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
    pub(crate) fn live(&self) -> AgentStatus {
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
    pub(crate) fn live(&self) -> crate::TurnOutcome {
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
            ExecutionEvent::FileChanges { id, edits } => Self::FileChanges {
                id: id.clone(),
                edits: edits
                    .iter()
                    .map(|e| StoredFileEdit {
                        path: e.path.clone(),
                        hunks: e
                            .hunks
                            .iter()
                            .map(|h| StoredHunk {
                                old_start: h.old_start,
                                old_lines: h.old_lines,
                                new_start: h.new_start,
                                new_lines: h.new_lines,
                                lines: h.lines.clone(),
                                section: h.section.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            },
            ExecutionEvent::TurnDiff { turn_id, diff } => Self::TurnDiff {
                turn_id: turn_id.clone(),
                diff: diff.clone(),
            },
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
            ExecutionEvent::Retract { ids } => Self::Retract { ids: ids.clone() },
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
            ExecutionEvent::ContextUsage {
                total_tokens,
                context_window,
            } => Self::ContextUsage {
                total_tokens: *total_tokens,
                context_window: *context_window,
            },
            ExecutionEvent::UsageDetails { details } => Self::UsageDetails {
                details: details.clone(),
            },
        }
    }
    pub(crate) fn live(&self) -> ExecutionEvent {
        match self {
            Self::FileChanges { id, edits } => ExecutionEvent::FileChanges {
                id: id.clone(),
                edits: edits
                    .iter()
                    .map(|e| crate::FileEdit {
                        path: e.path.clone(),
                        hunks: e
                            .hunks
                            .iter()
                            .map(|h| crate::Hunk {
                                old_start: h.old_start,
                                old_lines: h.old_lines,
                                new_start: h.new_start,
                                new_lines: h.new_lines,
                                lines: h.lines.clone(),
                                section: h.section.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            },
            Self::TurnDiff { turn_id, diff } => ExecutionEvent::TurnDiff {
                turn_id: turn_id.clone(),
                diff: diff.clone(),
            },
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
            Self::Retract { ids } => ExecutionEvent::Retract { ids: ids.clone() },
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
            Self::ContextUsage {
                total_tokens,
                context_window,
            } => ExecutionEvent::ContextUsage {
                total_tokens: *total_tokens,
                context_window: *context_window,
            },
            Self::UsageDetails { details } => ExecutionEvent::UsageDetails {
                details: details.clone(),
            },
        }
    }
}

impl PersistedActivity {
    pub(crate) fn from_live(event: &ActivityEvent, duration: Option<Duration>) -> Option<Self> {
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
            ActivityEvent::CompletionObservation {
                subject,
                elapsed_ms,
                completed_at,
                input_tokens,
                output_tokens,
            } => Self::CompletionObservation {
                subject: PersistedSubject::from_live(subject),
                elapsed_ms: *elapsed_ms,
                completed_at: completed_at.clone(),
                input_tokens: *input_tokens,
                output_tokens: *output_tokens,
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
    pub(crate) fn live(&self) -> ActivityEvent {
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
            Self::CompletionObservation {
                subject,
                elapsed_ms,
                completed_at,
                input_tokens,
                output_tokens,
            } => ActivityEvent::CompletionObservation {
                subject: subject.live(),
                elapsed_ms: *elapsed_ms,
                completed_at: completed_at.clone(),
                input_tokens: *input_tokens,
                output_tokens: *output_tokens,
            },
            Self::Alias { from, to } => ActivityEvent::Alias {
                from: AgentKey::from_stored(from.clone()),
                to: AgentKey::from_stored(to.clone()),
            },
        }
    }
    /// Every child key this observation names.
    pub(crate) fn children(&self) -> impl Iterator<Item = &str> {
        let keys: [Option<&str>; 2] = match self {
            Self::Discovered { key, .. }
            | Self::Status { key, .. }
            | Self::Content { key, .. }
            | Self::HistoryContent { key, .. }
            | Self::Coverage { key, .. }
            | Self::Detached { key } => [Some(key), None],
            Self::Alias { from, to } => [Some(from), Some(to)],
            Self::CompletionObservation { subject, .. } => [subject.key(), None],
            Self::MainContent { .. } | Self::BackgroundTurnEnded { .. } => [None, None],
        };
        keys.into_iter().flatten()
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
                | Self::CompletionObservation { .. }
        )
    }
    pub(crate) fn tool_duration(&self) -> Option<(Subject, String, Duration)> {
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

