//! Event-sourced Thread log: what makes a Thread outlive every process.
//!
//! Files live under a directory the caller passes — one subdirectory per
//! Thread, one JSONL log inside. The persisted schema is the store's own,
//! versioned from day one and converted internally from `SessionEvent`:
//! live-model churn is never a data migration. Writers buffer in memory and
//! flush on boundary marks (turn end, close) or a timeout — a durable write
//! per delta is impossible by interface shape.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::workspace::registry::ProjectId;
use crate::workspace::WorkspaceBinding;
use crate::SessionEvent;
use crate::{transcript::Input, ThreadId};

mod activity;
use activity::{Execution as PersistedExecution, PersistedActivity};
mod base;
use base::{Base, Tracker};
mod worker;
use worker::Worker;
#[cfg(test)]
pub(crate) use worker::lock as worker_lock;
#[cfg(test)]
mod activity_tests;
#[cfg(test)]
pub(crate) mod compat_tests;
#[cfg(test)]
mod durability_tests;
#[cfg(test)]
mod revive_tests;

/// The schema this store writes. Every log names the schema it was written
/// at in its header line; `load` accepts this version and every version
/// before it, and refuses anything newer — a log from Ferrite's future must
/// fail loudly, not half-read.
///
/// History:
/// - **1** — header + event records converted from the Session stream.
/// - **2** — `prompt` records (the operator's own lines are history too) and
///   the structured `result` on `tool_completed` (a diff card cannot be
///   redrawn from prose). A v1 log loads with no prompts and every result
///   `Opaque` — exactly what v1 recorded, nothing invented.
/// - **3** — the workspace binding in the header (the checkout a Thread
///   works in must survive a restart). A v1/v2 log loads with no binding —
///   those Threads never recorded where they worked.
/// - **4** — the session project root in the header (#24): the git repo
///   inside the binding where the Thread's work happens. A v1–v3 log loads
///   with none — those Threads work in the binding itself, which is also
///   what `None` means today.
/// - **5** — the chosen model in the header (#25): what a pre-first-prompt
///   pick recorded, verbatim as the provider announced it. A v1–v4 log
///   loads with none — the provider's default, which is also what absent
///   means.
/// - **6** — the registered project in the header (#29): the registry id of
///   the project the Thread's CWD choice named. A v1–v5 log loads with none
///   — the resolved binding paths stay the durable truth either way, so a
///   Thread without one loses nothing but a grouping key.
/// - **7** — an optional operator-chosen Thread title. Older Threads keep
///   their generated `thread-NN` label until renamed.
/// - **8** — the chosen reasoning effort in the header, beside the model,
///   and the `handover` record: a provider switch after the first prompt,
///   which tells a revive that the last Init belongs to a provider no
///   longer serving. A v1–v7 log loads with no effort — the provider's
///   default, which is also what absent means — and no handovers.
/// - **9** — attributed subagent facts, kept separate from Main records.
///   Old logs remain Main-only; live Decision handles are never persisted.
/// - **10** — native progress, identified summary sections, and live tool output
///   in both Main and attributed execution records.
/// - **11** — durable locally observed completion time and elapsed duration.
/// - **12** — a prompt's observed send time (`prompt_observation`, right
///   after its prompt), a turn's token counts on its completion
///   observation, and a hunk's section. A v1–v11 log loads with none of
///   them: its prompts draw no time and its stamps no tokens.
/// - **13** — header facts change by appending, never by rewriting the log
///   (ADR 0009): a `facts` record restates them all after a change, a
///   `handover` carries the facts it switched to, and the store's own
///   `mark` restates them every megabyte or so, so a peek reads the tail
///   instead of the whole log. The newest record carrying facts wins. A
///   v1–v12 log's header is its only facts record; its handovers carry
///   none, their effect already in the header rewritten with them.
const SCHEMA_VERSION: u32 = 13;

/// How far apart the store's marks sit, at least: what a peek or a reopen
/// reads back from the end of a log to find its facts.
const MARK_SPACING: u64 = 1024 * 1024;

/// How far `peek_first_prompt` reads before giving up: the first prompt
/// is normally the second line, and a log whose first prompt sits past
/// this much preamble is named by its number instead.
const FIRST_PROMPT_SCAN: usize = 64 * 1024;

/// Presence means the Thread had an open Pane when Ferrite last ran. Session
/// processes are deliberately not durable, but the operator's open/parked
/// choice is.
const OPEN_MARKER: &str = ".open";
const OPEN_STATE_MARKER: &str = ".open-state-v1";

/// The store's single-writer claim (ADR 0009): the process holding an
/// exclusive lock on this file is the only one that changes any log.
const CLAIM: &str = ".lock";

/// What a process that found the claim taken may still do, in its own words.
const READ_ONLY: &str = "another Ferrite has this store open, so this window can read \
     its Threads but not open, create or change them";

/// Which agent backend serves this Thread — persisted so a restart knows
/// which provider to revive the Thread on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Codex,
}

impl std::fmt::Display for Provider {
    /// The provider's name as a Notice says it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Provider::Claude => "Claude",
            Provider::Codex => "Codex",
        })
    }
}

/// Loading one Thread failed. Errors are per-Thread by design: one damaged
/// log must never take the rest of the store down with it.
#[derive(Debug)]
pub enum LoadError {
    /// The log was written by a newer Ferrite than this one. Refused whole:
    /// half-reading a schema from the future would show the operator a
    /// Thread that quietly is not theirs. The operator upgrades Ferrite —
    /// the log is fine.
    FutureSchema {
        found: u32,
        supported: u32,
    },
    /// The log's own header could not be read — the file was damaged at or
    /// before the first line, which only a crash inside `create` can leave
    /// behind. Everything after the header heals silently; the header is the
    /// one line with nothing before it to recover to.
    Corrupt {
        detail: String,
    },
    Io(io::Error),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::FutureSchema { found, supported } => write!(
                f,
                "thread log is schema {found}, newer than the supported {supported}; \
                 upgrade Ferrite"
            ),
            LoadError::Corrupt { detail } => write!(f, "thread log corrupt: {detail}"),
            LoadError::Io(e) => write!(f, "io error loading thread: {e}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<io::Error> for LoadError {
    fn from(e: io::Error) -> Self {
        LoadError::Io(e)
    }
}

/// The first line of every log: what wrote it, for what provider, working
/// where.
#[derive(Serialize, Deserialize)]
struct Header {
    schema: u32,
    provider: Provider,
    /// Schema 3+; a v1/v2 header loads as `None` — those Threads never
    /// recorded a binding.
    #[serde(default)]
    workspace: Option<PersistedBinding>,
    /// Schema 4+; the git repo inside the binding where work happens.
    /// `None` — and every v1–v3 header — means work in the binding itself.
    #[serde(default)]
    session_project_root: Option<PathBuf>,
    /// Schema 5+; the model this Thread chose before its first prompt.
    /// `None` — and every v1–v4 header — means the provider's default.
    #[serde(default)]
    model: Option<String>,
    /// Schema 6+; the registry id of the project this Thread's CWD choice
    /// named (#29). `None` — and every v1–v5 header — means unregistered:
    /// the binding's resolved paths still say where work happens.
    #[serde(default)]
    project_id: Option<ProjectId>,
    /// Schema 7+; absent means the generated `thread-NN` label.
    #[serde(default)]
    title: Option<String>,
    /// Schema 8+; the reasoning effort this Thread chose. `None` — and
    /// every v1–v7 header — means the provider's default.
    #[serde(default)]
    effort: Option<String>,
}

impl Header {
    fn facts(&self) -> Facts {
        Facts {
            provider: self.provider,
            workspace: self.workspace.clone(),
            session_project_root: self.session_project_root.clone(),
            model: self.model.clone(),
            project_id: self.project_id,
            title: self.title.clone(),
            effort: self.effort.clone(),
        }
    }

    fn of(facts: Facts) -> Self {
        Header {
            schema: SCHEMA_VERSION,
            provider: facts.provider,
            workspace: facts.workspace,
            session_project_root: facts.session_project_root,
            model: facts.model,
            project_id: facts.project_id,
            title: facts.title,
            effort: facts.effort,
        }
    }
}

/// A Thread's header facts (schema 13): what its first line says, and what
/// every record that changes them restates in full — never a delta, so the
/// newest one alone is the truth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Facts {
    provider: Provider,
    #[serde(default)]
    workspace: Option<PersistedBinding>,
    #[serde(default)]
    session_project_root: Option<PathBuf>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    project_id: Option<ProjectId>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    effort: Option<String>,
}

impl Facts {
    fn meta(self) -> ThreadMeta {
        ThreadMeta {
            provider: self.provider,
            workspace: self.workspace.as_ref().map(PersistedBinding::live),
            session_project_root: self.session_project_root,
            model: self.model,
            project_id: self.project_id,
            title: self.title,
            effort: self.effort,
            summary: None,
        }
    }
}

/// What a parked Thread's nav row counts (schema 13, in every mark): read
/// off the log's tail instead of replaying the whole log.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct Summary {
    /// Prompts since the last conversation reset: the wall's `11 turns`.
    turns: u64,
    /// Whether any prompt was ever sent, which locks the provider choice.
    prompted: bool,
    /// The subagents the live Activity knew when the mark was written;
    /// absent when no Activity has told the writer yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subagents: Option<u64>,
}

impl Summary {
    fn public(&self) -> ThreadSummary {
        ThreadSummary {
            turns: self.turns as usize,
            prompted: self.prompted,
            subagents: self.subagents.map(|count| count as usize),
        }
    }

    /// Count `record` in.
    fn observe(&mut self, record: &Record) {
        match record {
            Record::Prompt { .. } => {
                self.turns += 1;
                self.prompted = true;
            }
            Record::ConversationReset { .. } => self.turns = 0,
            _ => {}
        }
    }
}

/// What a parked Thread's row says without replaying its log (ADR 0009).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSummary {
    /// Prompts since the last conversation reset.
    pub turns: usize,
    /// Whether the operator ever sent a prompt (the provider lock).
    pub prompted: bool,
    /// The subagents the Thread knew when it was last written, when known.
    pub subagents: Option<usize>,
}

/// The persisted form of a Thread's workspace binding, mirroring
/// `workspace::WorkspaceBinding` shape for shape — but the store's own type,
/// so the live vocabulary can change without rewriting anyone's history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PersistedBinding {
    Main { checkout: PathBuf },
    Worktree { repo: PathBuf, path: PathBuf },
}

impl PersistedBinding {
    fn from_live(binding: &WorkspaceBinding) -> Self {
        match binding {
            WorkspaceBinding::Main { checkout } => PersistedBinding::Main {
                checkout: checkout.clone(),
            },
            WorkspaceBinding::Worktree { repo, path } => PersistedBinding::Worktree {
                repo: repo.clone(),
                path: path.clone(),
            },
        }
    }

    fn live(&self) -> WorkspaceBinding {
        match self {
            PersistedBinding::Main { checkout } => WorkspaceBinding::Main {
                checkout: checkout.clone(),
            },
            PersistedBinding::Worktree { repo, path } => WorkspaceBinding::Worktree {
                repo: repo.clone(),
                path: path.clone(),
            },
        }
    }
}

/// One line of the log body: the persisted schema, owned by the store.
/// Converted from `SessionEvent`, never `SessionEvent` itself — the live
/// event vocabulary may grow any day, and this one changes only with a
/// schema bump. Text deltas are coalesced into one record per run: the log
/// stores what was said, not how the wire chopped it.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Record {
    SummaryPart {
        item_id: String,
        summary_index: u64,
        text: String,
        snapshot: bool,
    },
    Progress {
        event: PersistedProgress,
    },
    ToolOutput {
        id: String,
        text: String,
    },
    FileChanges {
        id: String,
        edits: Vec<PersistedFileEdit>,
    },
    TurnDiff {
        turn_id: String,
        diff: String,
    },
    ContentBoundary,
    /// Schema 9: a durable attributed fact, never a pending request handle.
    Activity {
        observation: PersistedActivity,
    },
    Init {
        session_id: String,
        model: String,
    },
    ModelChanged {
        model: String,
    },
    ConversationReset {
        session_id: String,
    },
    /// A line the operator sent (schema 2+).
    Prompt {
        text: String,
    },
    Text {
        text: String,
    },
    Thinking {
        text: String,
    },
    ReasoningSummary {
        text: String,
        summary_index: u64,
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
    ContextDetails {
        details: crate::ContextDetails,
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
        /// Schema 2+; a v1 record loads as `Opaque`, which is also what v1
        /// wrote by never recording one.
        #[serde(default)]
        result: PersistedToolResult,
        /// How long the call ran, as the cockpit clocked it live. Absent
        /// from any log written before the clock was persisted, and absent
        /// for a call whose start this cockpit never saw — the row then
        /// draws no duration, exactly as it did.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    TurnEnded {
        outcome: Outcome,
        cost_usd: Option<f64>,
    },
    RunState {
        state: StoredRunState,
    },
    CompletionObservation {
        elapsed_ms: u64,
        completed_at: String,
        /// The turn's token counts; absent from older logs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_tokens: Option<u64>,
    },
    /// When the prompt before it was sent (`7:31 pm`), recorded right after
    /// it. A log from before it was kept has none, and its prompts draw no
    /// time.
    PromptObservation {
        sent_at: String,
    },
    Closed {
        reason: String,
    },
    /// The Thread moved to another provider after its first prompt (schema
    /// 8+). Everything before this line was said to `from`; the Session
    /// that follows is `to`'s, started fresh, so the Init before this line
    /// is nobody's to resume and the conversation before it travels as
    /// context in the next prompt instead.
    Handover {
        from: Provider,
        to: Provider,
        model: Option<String>,
        /// Schema 13: the facts the switch commits, in the same line as the
        /// switch itself. Absent from older logs, whose header rewrite
        /// already holds them.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        facts: Option<Facts>,
    },
    /// Schema 13: the header facts after a change, restated whole.
    Facts { facts: Facts },
    /// Schema 13: a bookmark the store writes for itself. It restates the
    /// facts at this point, so a peek or a reopen reads back from the end
    /// to the newest one instead of the whole log, and certifies that every
    /// line before it was readable when it was written. Derived: a reader
    /// that ignores marks reads exactly the same Thread.
    Mark {
        facts: Facts,
        /// The offset of the mark before this one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prev: Option<u64>,
        /// The parked row's counts at this point.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<Summary>,
        /// Where a revive may start reading, and what it replays first.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<Base>,
    },
}

impl Record {
    /// The facts this record restates, when it carries them.
    fn facts(&self) -> Option<&Facts> {
        match self {
            Record::Facts { facts } | Record::Mark { facts, .. } => Some(facts),
            Record::Handover { facts, .. } => facts.as_ref(),
            _ => None,
        }
    }

    /// The store's own bookkeeping, folded into the facts when a log is
    /// read and never part of the conversation.
    fn is_bookkeeping(&self) -> bool {
        matches!(self, Record::Facts { .. } | Record::Mark { .. })
    }
}

/// The Notice a provider switch leaves in the transcript, live and on
/// replay alike.
pub(crate) fn handover_notice(to: Provider, model: Option<&str>) -> String {
    let label = crate::providers::models::label(model.unwrap_or("default"), &[]);
    format!("continued on {to} · {label} — the earlier conversation is handed over as context")
}

/// How a persisted turn ended: `"completed"`, `"interrupted"`, or
/// `{"error":"…"}`.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Completed,
    Interrupted,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredRunState {
    Running,
    RequiresAction,
    Idle,
}
impl StoredRunState {
    fn from_live(state: crate::RunState) -> Self {
        match state {
            crate::RunState::Running => Self::Running,
            crate::RunState::RequiresAction => Self::RequiresAction,
            crate::RunState::Idle => Self::Idle,
        }
    }
    fn live(self) -> crate::RunState {
        match self {
            Self::Running => crate::RunState::Running,
            Self::RequiresAction => crate::RunState::RequiresAction,
            Self::Idle => crate::RunState::Idle,
        }
    }
}

/// The structured half of a persisted tool result, mirroring
/// `crate::ToolResult` shape for shape — but its own type, so the live model
/// can change without rewriting anyone's history.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PersistedToolResult {
    #[default]
    Opaque,
    Command {
        stdout: String,
        stderr: String,
        exit_code: Option<i64>,
        duration_ms: Option<u64>,
    },
    Structured {
        value: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    FileEdit {
        path: String,
        hunks: Vec<PersistedHunk>,
    },
    FileEdits {
        edits: Vec<PersistedFileEdit>,
    },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct PersistedFileEdit {
    path: String,
    hunks: Vec<PersistedHunk>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct PersistedHunk {
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
    lines: Vec<String>,
    /// The section its header named; absent from older logs (none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    section: Option<String>,
}

impl PersistedToolResult {
    fn from_live(result: &crate::ToolResult) -> Self {
        match result {
            crate::ToolResult::Opaque => PersistedToolResult::Opaque,
            crate::ToolResult::Command {
                stdout,
                stderr,
                exit_code,
                duration_ms,
            } => PersistedToolResult::Command {
                stdout: stdout.clone(),
                stderr: stderr.clone(),
                exit_code: *exit_code,
                duration_ms: *duration_ms,
            },
            crate::ToolResult::Structured { value, duration_ms } => {
                PersistedToolResult::Structured {
                    value: value.clone(),
                    duration_ms: *duration_ms,
                }
            }
            crate::ToolResult::FileEdit { path, hunks } => PersistedToolResult::FileEdit {
                path: path.clone(),
                hunks: hunks
                    .iter()
                    .map(|hunk| PersistedHunk {
                        old_start: hunk.old_start,
                        old_lines: hunk.old_lines,
                        new_start: hunk.new_start,
                        new_lines: hunk.new_lines,
                        lines: hunk.lines.clone(),
                        section: hunk.section.clone(),
                    })
                    .collect(),
            },
            crate::ToolResult::FileEdits { edits } => PersistedToolResult::FileEdits {
                edits: edits
                    .iter()
                    .map(|edit| PersistedFileEdit {
                        path: edit.path.clone(),
                        hunks: edit
                            .hunks
                            .iter()
                            .map(|hunk| PersistedHunk {
                                old_start: hunk.old_start,
                                old_lines: hunk.old_lines,
                                new_start: hunk.new_start,
                                new_lines: hunk.new_lines,
                                lines: hunk.lines.clone(),
                                section: hunk.section.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            },
        }
    }

    fn live(&self) -> crate::ToolResult {
        match self {
            PersistedToolResult::Opaque => crate::ToolResult::Opaque,
            PersistedToolResult::Command {
                stdout,
                stderr,
                exit_code,
                duration_ms,
            } => crate::ToolResult::Command {
                stdout: stdout.clone(),
                stderr: stderr.clone(),
                exit_code: *exit_code,
                duration_ms: *duration_ms,
            },
            PersistedToolResult::Structured { value, duration_ms } => {
                crate::ToolResult::Structured {
                    value: value.clone(),
                    duration_ms: *duration_ms,
                }
            }
            PersistedToolResult::FileEdit { path, hunks } => crate::ToolResult::FileEdit {
                path: path.clone(),
                hunks: hunks
                    .iter()
                    .map(|hunk| crate::Hunk {
                        old_start: hunk.old_start,
                        old_lines: hunk.old_lines,
                        new_start: hunk.new_start,
                        new_lines: hunk.new_lines,
                        lines: hunk.lines.clone(),
                        section: hunk.section.clone(),
                    })
                    .collect(),
            },
            PersistedToolResult::FileEdits { edits } => crate::ToolResult::FileEdits {
                edits: edits
                    .iter()
                    .map(|edit| crate::FileEdit {
                        path: edit.path.clone(),
                        hunks: edit
                            .hunks
                            .iter()
                            .map(|hunk| crate::Hunk {
                                old_start: hunk.old_start,
                                old_lines: hunk.old_lines,
                                new_start: hunk.new_start,
                                new_lines: hunk.new_lines,
                                lines: hunk.lines.clone(),
                                section: hunk.section.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PersistedProgress {
    Phase {
        phase: StoredPhase,
        detail: String,
    },
    Tool {
        id: String,
        message: String,
        elapsed_ms: Option<u64>,
    },
    Plan {
        steps: Vec<StoredStep>,
        explanation: String,
    },
    Task {
        id: String,
        subject: String,
        status: Option<StoredStepStatus>,
        deleted: bool,
    },
    TasksSnapshot {
        tasks: Vec<StoredPlanTask>,
    },
    Background {
        id: String,
        label: String,
        status: StoredTaskStatus,
        detail: String,
    },
    BackgroundSnapshot {
        tasks: Vec<StoredBackgroundTask>,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredBackgroundTask {
    id: String,
    label: String,
    status: StoredTaskStatus,
    detail: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredStep {
    text: String,
    status: StoredStepStatus,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredPlanTask {
    id: String,
    text: String,
    status: StoredStepStatus,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredPhase {
    Working,
    Thinking,
    Answering,
    Compacting,
    Retrying,
    Waiting,
}
impl StoredPhase {
    fn from_live(value: crate::progress::Phase) -> Self {
        match value {
            crate::progress::Phase::Working => Self::Working,
            crate::progress::Phase::Thinking => Self::Thinking,
            crate::progress::Phase::Answering => Self::Answering,
            crate::progress::Phase::Compacting => Self::Compacting,
            crate::progress::Phase::Retrying => Self::Retrying,
            crate::progress::Phase::Waiting => Self::Waiting,
        }
    }
    fn live(self) -> crate::progress::Phase {
        match self {
            Self::Working => crate::progress::Phase::Working,
            Self::Thinking => crate::progress::Phase::Thinking,
            Self::Answering => crate::progress::Phase::Answering,
            Self::Compacting => crate::progress::Phase::Compacting,
            Self::Retrying => crate::progress::Phase::Retrying,
            Self::Waiting => crate::progress::Phase::Waiting,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredStepStatus {
    Pending,
    InProgress,
    Completed,
}
impl StoredStepStatus {
    fn from_live(value: crate::progress::StepStatus) -> Self {
        match value {
            crate::progress::StepStatus::Pending => Self::Pending,
            crate::progress::StepStatus::InProgress => Self::InProgress,
            crate::progress::StepStatus::Completed => Self::Completed,
        }
    }
    fn live(self) -> crate::progress::StepStatus {
        match self {
            Self::Pending => crate::progress::StepStatus::Pending,
            Self::InProgress => crate::progress::StepStatus::InProgress,
            Self::Completed => crate::progress::StepStatus::Completed,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredTaskStatus {
    Working,
    Completed,
    Failed,
    Stopped,
    Unknown,
}
impl StoredTaskStatus {
    fn from_live(value: crate::progress::TaskStatus) -> Self {
        match value {
            crate::progress::TaskStatus::Working => Self::Working,
            crate::progress::TaskStatus::Completed => Self::Completed,
            crate::progress::TaskStatus::Failed => Self::Failed,
            crate::progress::TaskStatus::Stopped => Self::Stopped,
            crate::progress::TaskStatus::Unknown => Self::Unknown,
        }
    }
    fn live(self) -> crate::progress::TaskStatus {
        match self {
            Self::Working => crate::progress::TaskStatus::Working,
            Self::Completed => crate::progress::TaskStatus::Completed,
            Self::Failed => crate::progress::TaskStatus::Failed,
            Self::Stopped => crate::progress::TaskStatus::Stopped,
            Self::Unknown => crate::progress::TaskStatus::Unknown,
        }
    }
}
impl PersistedProgress {
    fn coalesce(&mut self, next: &Self) -> bool {
        match (self, next) {
            (
                Self::Tool {
                    id,
                    message,
                    elapsed_ms,
                },
                Self::Tool {
                    id: next,
                    message: more,
                    elapsed_ms: elapsed,
                },
            ) if id == next => {
                if !more.is_empty() {
                    *message = more.clone();
                }
                if let Some(elapsed) = elapsed {
                    *elapsed_ms = Some(elapsed_ms.unwrap_or(0).max(*elapsed));
                }
                true
            }
            _ => false,
        }
    }
    fn from_live(event: &crate::progress::ProgressEvent) -> Self {
        use crate::progress::ProgressEvent as E;
        match event {
            E::Phase { phase, detail } => Self::Phase {
                phase: StoredPhase::from_live(*phase),
                detail: detail.clone(),
            },
            E::Tool {
                id,
                message,
                elapsed_ms,
            } => Self::Tool {
                id: id.clone(),
                message: message.clone(),
                elapsed_ms: *elapsed_ms,
            },
            E::Plan { steps, explanation } => Self::Plan {
                steps: steps
                    .iter()
                    .map(|step| StoredStep {
                        text: step.text.clone(),
                        status: StoredStepStatus::from_live(step.status),
                    })
                    .collect(),
                explanation: explanation.clone(),
            },
            E::Task {
                id,
                subject,
                status,
                deleted,
            } => Self::Task {
                id: id.clone(),
                subject: subject.clone(),
                status: status.map(StoredStepStatus::from_live),
                deleted: *deleted,
            },
            E::TasksSnapshot { tasks } => Self::TasksSnapshot {
                tasks: tasks
                    .iter()
                    .map(|task| StoredPlanTask {
                        id: task.id.clone(),
                        text: task.text.clone(),
                        status: StoredStepStatus::from_live(task.status),
                    })
                    .collect(),
            },
            E::Background {
                id,
                label,
                status,
                detail,
            } => Self::Background {
                id: id.clone(),
                label: label.clone(),
                status: StoredTaskStatus::from_live(*status),
                detail: detail.clone(),
            },
            E::BackgroundSnapshot { tasks } => Self::BackgroundSnapshot {
                tasks: tasks
                    .iter()
                    .map(|task| StoredBackgroundTask {
                        id: task.id.clone(),
                        label: task.label.clone(),
                        status: StoredTaskStatus::from_live(task.status),
                        detail: task.detail.clone(),
                    })
                    .collect(),
            },
        }
    }
    fn live(&self) -> crate::progress::ProgressEvent {
        use crate::progress::ProgressEvent as E;
        match self {
            Self::Phase { phase, detail } => E::Phase {
                phase: phase.live(),
                detail: detail.clone(),
            },
            Self::Tool {
                id,
                message,
                elapsed_ms,
            } => E::Tool {
                id: id.clone(),
                message: message.clone(),
                elapsed_ms: *elapsed_ms,
            },
            Self::Plan { steps, explanation } => E::Plan {
                steps: steps
                    .iter()
                    .map(|step| crate::progress::PlanStep {
                        text: step.text.clone(),
                        status: step.status.live(),
                    })
                    .collect(),
                explanation: explanation.clone(),
            },
            Self::Task {
                id,
                subject,
                status,
                deleted,
            } => E::Task {
                id: id.clone(),
                subject: subject.clone(),
                status: status.map(StoredStepStatus::live),
                deleted: *deleted,
            },
            Self::TasksSnapshot { tasks } => E::TasksSnapshot {
                tasks: tasks
                    .iter()
                    .map(|task| crate::progress::PlanTask {
                        id: task.id.clone(),
                        text: task.text.clone(),
                        status: task.status.live(),
                    })
                    .collect(),
            },
            Self::Background {
                id,
                label,
                status,
                detail,
            } => E::Background {
                id: id.clone(),
                label: label.clone(),
                status: status.live(),
                detail: detail.clone(),
            },
            Self::BackgroundSnapshot { tasks } => E::BackgroundSnapshot {
                tasks: tasks
                    .iter()
                    .map(|task| crate::progress::BackgroundTask {
                        id: task.id.clone(),
                        label: task.label.clone(),
                        status: task.status.live(),
                        detail: task.detail.clone(),
                    })
                    .collect(),
            },
        }
    }
}

impl Record {
    /// The persisted form of one live event, or `None` for events that are
    /// Session state rather than durable history (a pending Decision dies
    /// with its Session; there is nothing to replay it into). `duration` is
    /// the wall clock the cockpit measured for a settled tool call — the one
    /// thing in the record no event carries.
    fn from_event(event: &SessionEvent, duration: Option<std::time::Duration>) -> Option<Record> {
        Some(match event {
            SessionEvent::ReasoningSummaryPart {
                item_id,
                summary_index,
                text,
                snapshot,
            } => Record::SummaryPart {
                item_id: item_id.clone(),
                summary_index: *summary_index,
                text: text.clone(),
                snapshot: *snapshot,
            },
            SessionEvent::Progress { event } => Record::Progress {
                event: PersistedProgress::from_live(event),
            },
            SessionEvent::ToolOutputDelta { id, text } => Record::ToolOutput {
                id: id.clone(),
                text: text.clone(),
            },
            SessionEvent::FileChanges { id, edits } => Record::FileChanges {
                id: id.clone(),
                edits: edits
                    .iter()
                    .map(|edit| PersistedFileEdit {
                        path: edit.path.clone(),
                        hunks: edit
                            .hunks
                            .iter()
                            .map(|hunk| PersistedHunk {
                                old_start: hunk.old_start,
                                old_lines: hunk.old_lines,
                                new_start: hunk.new_start,
                                new_lines: hunk.new_lines,
                                lines: hunk.lines.clone(),
                                section: hunk.section.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            },
            SessionEvent::TurnDiff { turn_id, diff } => Record::TurnDiff {
                turn_id: turn_id.clone(),
                diff: diff.clone(),
            },
            SessionEvent::ContentBoundary => Record::ContentBoundary,
            SessionEvent::Activity(observation) => Record::Activity {
                observation: PersistedActivity::from_live(observation, duration)?,
            },
            SessionEvent::Init { session_id, model } => Record::Init {
                session_id: session_id.clone(),
                model: model.clone(),
            },
            SessionEvent::ModelChanged { model } => Record::ModelChanged {
                model: model.clone(),
            },
            SessionEvent::ConversationReset { session_id } => Record::ConversationReset {
                session_id: session_id.clone(),
            },
            SessionEvent::TextDelta { text } => Record::Text { text: text.clone() },
            SessionEvent::ThinkingDelta { text } => Record::Thinking { text: text.clone() },
            SessionEvent::ToolStarted { id, name, input } => Record::ToolStarted {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            },
            SessionEvent::ToolCompleted {
                id,
                output,
                is_error,
                result,
            } => Record::ToolCompleted {
                id: id.clone(),
                output: output.clone(),
                is_error: *is_error,
                result: PersistedToolResult::from_live(result),
                duration_ms: duration.map(|total| total.as_millis() as u64),
            },
            SessionEvent::TurnEnded { outcome, cost_usd } => Record::TurnEnded {
                outcome: match outcome {
                    crate::TurnOutcome::Completed => Outcome::Completed,
                    crate::TurnOutcome::Interrupted => Outcome::Interrupted,
                    crate::TurnOutcome::Error(message) => Outcome::Error(message.clone()),
                },
                cost_usd: *cost_usd,
            },
            SessionEvent::RunState { state } => Record::RunState {
                state: StoredRunState::from_live(*state),
            },
            SessionEvent::Closed { reason } => Record::Closed {
                reason: reason.clone(),
            },
            SessionEvent::ReasoningSummaryDelta {
                text,
                summary_index,
            } => Record::ReasoningSummary {
                text: text.clone(),
                summary_index: *summary_index,
            },
            SessionEvent::TokenUsage {
                total_tokens,
                input_tokens,
                cached_input_tokens,
                output_tokens,
                reasoning_output_tokens,
                context_window,
            } => Record::TokenUsage {
                total_tokens: *total_tokens,
                input_tokens: *input_tokens,
                cached_input_tokens: *cached_input_tokens,
                output_tokens: *output_tokens,
                reasoning_output_tokens: *reasoning_output_tokens,
                context_window: *context_window,
            },
            SessionEvent::ContextUsage {
                total_tokens,
                context_window,
            } => Record::ContextUsage {
                total_tokens: *total_tokens,
                context_window: *context_window,
            },
            SessionEvent::UsageDetails { details } => Record::UsageDetails {
                details: details.clone(),
            },
            SessionEvent::ContextDetails { details } => Record::ContextDetails {
                details: details.clone(),
            },
            SessionEvent::DecisionRequested { .. } => return None,
            // A streaming input is superseded by the start that follows it.
            SessionEvent::ToolDraft { .. } => return None,
            // The command menu, the permission mode and the model menu are
            // the live Session's, like a Decision: a replay has no Session
            // to serve them and the next one announces its own.
            SessionEvent::Commands { .. } => return None,
            SessionEvent::PermissionMode { .. } => return None,
            SessionEvent::Models { .. } | SessionEvent::Queue(_) => return None,
            SessionEvent::RateLimits { .. } => return None,
            SessionEvent::McpServers { .. } => return None,
            SessionEvent::McpAuthorization { .. } => return None,
        })
    }

    /// Replay this record as the transcript input it stands for.
    fn input(&self) -> Input {
        match self {
            Record::SummaryPart {
                item_id,
                summary_index,
                text,
                snapshot,
            } => Input::Event(SessionEvent::ReasoningSummaryPart {
                item_id: item_id.clone(),
                summary_index: *summary_index,
                text: text.clone(),
                snapshot: *snapshot,
            }),
            Record::Progress { event } => Input::Event(SessionEvent::Progress {
                event: event.live(),
            }),
            Record::ToolOutput { id, text } => Input::Event(SessionEvent::ToolOutputDelta {
                id: id.clone(),
                text: text.clone(),
            }),
            Record::FileChanges { id, edits } => Input::Event(SessionEvent::FileChanges {
                id: id.clone(),
                edits: edits
                    .iter()
                    .map(|edit| crate::FileEdit {
                        path: edit.path.clone(),
                        hunks: edit
                            .hunks
                            .iter()
                            .map(|hunk| crate::Hunk {
                                old_start: hunk.old_start,
                                old_lines: hunk.old_lines,
                                new_start: hunk.new_start,
                                new_lines: hunk.new_lines,
                                lines: hunk.lines.clone(),
                                section: hunk.section.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            }),
            Record::TurnDiff { turn_id, diff } => Input::Event(SessionEvent::TurnDiff {
                turn_id: turn_id.clone(),
                diff: diff.clone(),
            }),
            Record::ContentBoundary => Input::Event(SessionEvent::ContentBoundary),
            Record::Activity { observation } => {
                Input::Event(SessionEvent::Activity(observation.live()))
            }
            Record::Init { session_id, model } => Input::Event(SessionEvent::Init {
                session_id: session_id.clone(),
                model: model.clone(),
            }),
            Record::ModelChanged { model } => Input::Event(SessionEvent::ModelChanged {
                model: model.clone(),
            }),
            Record::ConversationReset { session_id } => {
                Input::Event(SessionEvent::ConversationReset {
                    session_id: session_id.clone(),
                })
            }
            Record::Prompt { text } => Input::Prompt(text.clone()),
            Record::Text { text } => Input::Event(SessionEvent::TextDelta { text: text.clone() }),
            Record::Thinking { text } => {
                Input::Event(SessionEvent::ThinkingDelta { text: text.clone() })
            }
            Record::ToolStarted { id, name, input } => Input::Event(SessionEvent::ToolStarted {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            }),
            Record::ToolCompleted {
                id,
                output,
                is_error,
                result,
                // The clock is not transcript vocabulary — it replays
                // through `tool_durations`, into the cockpit's own timings.
                duration_ms: _,
            } => Input::Event(SessionEvent::ToolCompleted {
                id: id.clone(),
                output: output.clone(),
                is_error: *is_error,
                result: result.live(),
            }),
            Record::TurnEnded { outcome, cost_usd } => Input::Event(SessionEvent::TurnEnded {
                outcome: match outcome {
                    Outcome::Completed => crate::TurnOutcome::Completed,
                    Outcome::Interrupted => crate::TurnOutcome::Interrupted,
                    Outcome::Error(message) => crate::TurnOutcome::Error(message.clone()),
                },
                cost_usd: *cost_usd,
            }),
            Record::RunState { state } => Input::Event(SessionEvent::RunState {
                state: state.live(),
            }),
            Record::CompletionObservation {
                elapsed_ms,
                completed_at,
                input_tokens,
                output_tokens,
            } => Input::CompletionObservation {
                elapsed_ms: *elapsed_ms,
                completed_at: completed_at.clone(),
                input_tokens: *input_tokens,
                output_tokens: *output_tokens,
            },
            Record::PromptObservation { sent_at } => Input::PromptObservation {
                sent_at: sent_at.clone(),
            },
            Record::ReasoningSummary {
                text,
                summary_index,
            } => Input::Event(SessionEvent::ReasoningSummaryDelta {
                text: text.clone(),
                summary_index: *summary_index,
            }),
            Record::TokenUsage {
                total_tokens,
                input_tokens,
                cached_input_tokens,
                output_tokens,
                reasoning_output_tokens,
                context_window,
            } => Input::Event(SessionEvent::TokenUsage {
                total_tokens: *total_tokens,
                input_tokens: *input_tokens,
                cached_input_tokens: *cached_input_tokens,
                output_tokens: *output_tokens,
                reasoning_output_tokens: *reasoning_output_tokens,
                context_window: *context_window,
            }),
            Record::ContextUsage {
                total_tokens,
                context_window,
            } => Input::Event(SessionEvent::ContextUsage {
                total_tokens: *total_tokens,
                context_window: *context_window,
            }),
            Record::UsageDetails { details } => Input::Event(SessionEvent::UsageDetails {
                details: details.clone(),
            }),
            Record::ContextDetails { details } => Input::Event(SessionEvent::ContextDetails {
                details: details.clone(),
            }),
            Record::Closed { reason } => Input::Event(SessionEvent::Closed {
                reason: reason.clone(),
            }),
            Record::Handover { to, model, .. } => {
                Input::Notice(handover_notice(*to, model.as_deref()))
            }
            Record::Facts { .. } | Record::Mark { .. } => {
                unreachable!("the store's bookkeeping is folded when a log is read")
            }
        }
    }

    /// Whether the log is consistent here — the transcript's boundary marks,
    /// seen from the store's side of the conversion. A handover is one too:
    /// the header rewrite that follows it must find it on disk.
    fn is_boundary(&self) -> bool {
        matches!(self, Record::Activity { observation } if observation.is_boundary())
            || matches!(
                self,
                Record::TurnEnded { .. }
                    | Record::CompletionObservation { .. }
                    | Record::Closed { .. }
                    | Record::Handover { .. }
            )
    }

    /// Extend this record with a later delta of the same kind, if the two
    /// coalesce. A run of deltas is one record; anything else keeps its line.
    fn coalesce(&mut self, next: &Record) -> bool {
        match (self, next) {
            (
                Record::SummaryPart {
                    item_id,
                    summary_index,
                    text,
                    snapshot: false,
                },
                Record::SummaryPart {
                    item_id: next,
                    summary_index: part,
                    text: more,
                    snapshot: false,
                },
            ) if item_id == next && summary_index == part => {
                text.push_str(more);
                true
            }

            (
                Record::ToolOutput { id, text },
                Record::ToolOutput {
                    id: next,
                    text: more,
                },
            ) if id == next => {
                text.push_str(more);
                true
            }
            (
                Record::Progress {
                    event: event @ PersistedProgress::Tool { .. },
                },
                Record::Progress {
                    event: next @ PersistedProgress::Tool { .. },
                },
            ) => event.coalesce(next),
            (Record::Text { text }, Record::Text { text: more }) => {
                text.push_str(more);
                true
            }
            (Record::Thinking { text }, Record::Thinking { text: more }) => {
                text.push_str(more);
                true
            }
            (
                Record::ReasoningSummary {
                    text,
                    summary_index,
                },
                Record::ReasoningSummary {
                    text: more,
                    summary_index: part,
                },
            ) if summary_index == part => {
                text.push_str(more);
                true
            }
            _ => false,
        }
    }
}

/// How long a writer lets buffered records sit before an append makes them
/// durable anyway. Bounds what a crash can cost during a long-streaming turn;
/// short turns never reach it, flushing on their boundary instead.
const DEFAULT_FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// The longest a written record waits for a full sync (ADR 0009). Barriers
/// at every boundary already survive a crash of Ferrite or the OS; this
/// bounds what a power cut can cost.
const FULL_SYNC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// How far a sync reaches (ADR 0009).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyncLevel {
    /// The log's writes so far reach the drive before any that follow:
    /// they survive a crash of Ferrite or the OS, and a power cut can lose
    /// only a suffix, never leave a hole. Cheap enough for every boundary.
    Barrier,
    /// The drive's own cache too (F_FULLFSYNC on macOS): the writes survive
    /// a power cut. Park, create, rewrite, and at least every 30 s.
    Full,
}

/// Sync a file behind a barrier: its writes reach the drive before any
/// that follow (ADR 0009). For files beside the logs, like the queue.
pub(crate) fn sync_ordered(file: &File) -> io::Result<()> {
    sync_file(file, SyncLevel::Barrier)
}

/// Sync one file to `level`. Plain `fsync` is never used on macOS: it does
/// not order writes, so a power cut could persist a later block without an
/// earlier one.
fn sync_file(file: &File, level: SyncLevel) -> io::Result<()> {
    match level {
        SyncLevel::Full => file.sync_data(),
        SyncLevel::Barrier => barrier(file),
    }
}

#[cfg(target_os = "macos")]
fn barrier(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    const F_BARRIERFSYNC: std::ffi::c_int = 85;
    extern "C" {
        fn fcntl(fd: std::ffi::c_int, cmd: std::ffi::c_int, ...) -> std::ffi::c_int;
    }
    // SAFETY: F_BARRIERFSYNC takes no argument and only reads the
    // descriptor, which `file` keeps open for the call.
    if unsafe { fcntl(file.as_raw_fd(), F_BARRIERFSYNC) } == -1 {
        // A filesystem without barriers still gets its writes to disk.
        return file.sync_data();
    }
    Ok(())
}

/// Elsewhere the data sync orders writes already (`fdatasync`,
/// `FlushFileBuffers`).
#[cfg(not(target_os = "macos"))]
fn barrier(file: &File) -> io::Result<()> {
    file.sync_data()
}

/// A directory of Thread logs.
#[derive(Clone)]
pub struct Store {
    dir: PathBuf,
    flush_interval: std::time::Duration,
    full_sync_interval: std::time::Duration,
    /// How far apart marks sit, at least.
    mark_spacing: u64,
    /// The Main cost a replay base must have after it: twice what Activity
    /// retains of Main, so its window lies wholly after the base.
    base_cost: u64,
    shared: std::sync::Arc<Shared>,
    #[cfg(test)]
    fail_create: bool,
    #[cfg(test)]
    fail_delete: bool,
}

/// What every handle this process opens on one store directory shares.
/// Two Ferrites on one store (a dev build beside the installed app) must
/// never both write: one could cut the other's records off a log or strand
/// its appends behind a rename. The first to open holds the claim; any other
/// reads and refuses to write.
struct Shared {
    /// Why this process may not write here, or `None` when it may.
    read_only: Option<String>,
    /// The derived summary cache (`Store::summary`), loaded on first use.
    summaries: std::sync::Mutex<Option<BTreeMap<u64, CachedSummary>>>,
    /// The store's worker, started with its first writer.
    worker: std::sync::OnceLock<std::sync::Arc<Worker>>,
    /// What the worker synced and marked, in order, for tests.
    #[cfg(test)]
    journal: std::sync::Mutex<Vec<(&'static str, u64)>>,
    /// Every byte read off a log, so tests can bound what a read costs.
    #[cfg(test)]
    read_bytes: std::sync::atomic::AtomicU64,
    /// Syncs so far, barrier then full, so tests can see which one ran.
    #[cfg(test)]
    syncs: [std::sync::atomic::AtomicU64; 2],
    #[cfg(test)]
    faults: Faults,
}

impl Shared {
    /// Sync `file` to `level`, counted for tests.
    fn sync(&self, file: &File, level: SyncLevel) -> io::Result<()> {
        sync_file(file, level)?;
        self.synced(level);
        Ok(())
    }

    fn synced(&self, _level: SyncLevel) {
        #[cfg(test)]
        self.syncs[_level as usize].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// The store's worker, started on first use.
    fn worker(self: &std::sync::Arc<Self>) -> std::sync::Arc<Worker> {
        self.worker
            .get_or_init(|| Worker::start(std::sync::Arc::downgrade(self)))
            .clone()
    }

    /// A sync the worker runs: where a test can slow or fail it.
    fn sync_in_worker(&self, file: &File, level: SyncLevel) -> io::Result<()> {
        #[cfg(test)]
        {
            let delay = *worker::lock(&self.faults.sync_delay);
            std::thread::sleep(delay);
            if self
                .faults
                .fail_sync
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                return Err(io::Error::other("injected sync failure"));
            }
        }
        self.sync(file, level)
    }

    #[cfg(test)]
    fn journal(&self, what: &'static str, at: u64) {
        worker::lock(&self.journal).push((what, at));
    }
}

/// Failures a test can switch on for every handle on one store.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct Faults {
    /// Fail replacing a log at this step.
    pub(crate) replace_fails_at: std::sync::Mutex<Option<ReplaceStep>>,
    /// Fail an amendment or handover after its line is written, before
    /// it is synced.
    pub(crate) fail_commit: std::sync::atomic::AtomicBool,
    /// And then fail taking it back off the log.
    pub(crate) fail_rollback: std::sync::atomic::AtomicBool,
    /// Slow every sync the worker runs by this much.
    pub(crate) sync_delay: std::sync::Mutex<std::time::Duration>,
    /// Fail every sync the worker runs.
    pub(crate) fail_sync: std::sync::atomic::AtomicBool,
}

/// The steps of putting a rewritten log in place, for fault injection.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReplaceStep {
    /// Mid-write of the new file: a crash leaves half of it behind.
    Write,
    /// Syncing the new file.
    Sync,
    /// Renaming it over the log.
    Rename,
    /// Making the rename durable.
    Durable,
}

#[cfg(test)]
impl Faults {
    pub(crate) fn fail_replace_at(&self, step: Option<ReplaceStep>) {
        *self
            .replace_fails_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = step;
    }
}

/// Take the claim on `dir` for this process, or say why another holds it.
/// A claim once taken is kept until the process exits: if it lapsed when
/// the last handle dropped, a reopen racing that drop (or a second Ferrite)
/// could find it taken, or take it, in between.
fn claim(dir: &Path, key: &DirKey) -> io::Result<Option<String>> {
    use std::sync::{Mutex, PoisonError};
    static CLAIMED: Mutex<BTreeMap<DirKey, File>> = Mutex::new(BTreeMap::new());
    let mut claimed = CLAIMED.lock().unwrap_or_else(PoisonError::into_inner);
    if claimed.contains_key(key) {
        return Ok(None);
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(CLAIM))?;
    match lock.try_lock() {
        Ok(()) => {
            claimed.insert(key.clone(), lock);
            Ok(None)
        }
        Err(fs::TryLockError::WouldBlock) => Ok(Some(READ_ONLY.to_string())),
        // A filesystem without locks cannot hold the claim; this process
        // writes as Ferrite always did there.
        Err(fs::TryLockError::Error(error)) if error.kind() == io::ErrorKind::Unsupported => {
            Ok(None)
        }
        Err(fs::TryLockError::Error(error)) => Err(error),
    }
}

impl Shared {
    fn new(read_only: Option<String>) -> Self {
        Self {
            read_only,
            summaries: std::sync::Mutex::new(None),
            worker: std::sync::OnceLock::new(),
            #[cfg(test)]
            journal: Default::default(),
            #[cfg(test)]
            read_bytes: Default::default(),
            #[cfg(test)]
            syncs: Default::default(),
            #[cfg(test)]
            faults: Faults::default(),
        }
    }

    /// The one `Shared` for `dir` in this process, keyed by what the
    /// directory is rather than how it is spelled.
    fn of(dir: &Path) -> io::Result<std::sync::Arc<Self>> {
        use std::sync::{Arc, PoisonError, Weak};
        let key = dir_key(dir)?;
        let mut open = OPEN.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(shared) = open.get(&key).and_then(Weak::upgrade) {
            return Ok(shared);
        }
        open.retain(|_, shared| shared.strong_count() > 0);
        let shared = Arc::new(Self::new(claim(dir, &key)?));
        open.insert(key, Arc::downgrade(&shared));
        Ok(shared)
    }
}

/// Every store this process has open, by what its directory is.
static OPEN: std::sync::Mutex<BTreeMap<DirKey, std::sync::Weak<Shared>>> =
    std::sync::Mutex::new(BTreeMap::new());

/// What a panic must not lose: every record any open store has accepted
/// and not yet written goes into its log now, on the panicking thread. A
/// writer that thread holds is left as it is. Install with
/// `install_panic_rescue`.
pub fn rescue_after_panic() {
    rescue(None);
}

/// `rescue_after_panic` before whatever panic hook was installed.
pub fn install_panic_rescue() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        rescue_after_panic();
        previous(info);
    }));
}

/// Write what every writer of the stores in `only` (all, for `None`) holds.
fn rescue(only: Option<&Path>) {
    let only = only.and_then(|dir| dir_key(dir).ok());
    let stores: Vec<_> = worker::lock(&OPEN)
        .iter()
        .filter(|(key, _)| only.as_ref().is_none_or(|only| only == *key))
        .filter_map(|(_, shared)| shared.upgrade())
        .collect();
    for shared in stores {
        let Some(worker) = shared.worker.get() else {
            continue;
        };
        let writers = worker.queue().writers();
        for writer in writers {
            if let Ok(mut state) = writer.try_lock() {
                let _ = state.flush();
            }
        }
    }
}

#[cfg(unix)]
type DirKey = (u64, u64);

/// A directory's device and inode: the same for every path that names it.
#[cfg(unix)]
fn dir_key(dir: &Path) -> io::Result<DirKey> {
    use std::os::unix::fs::MetadataExt;
    let meta = fs::metadata(dir)?;
    Ok((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
type DirKey = PathBuf;

#[cfg(not(unix))]
fn dir_key(dir: &Path) -> io::Result<DirKey> {
    fs::canonicalize(dir)
}

/// The derived summary cache beside the logs: counts for pre-13 logs that
/// would otherwise be replayed whole for every parked row.
const SUMMARIES: &str = "summaries.json";

/// One cached count, valid while its log is unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedSummary {
    stamp: (u64, u128),
    summary: Summary,
}

/// What identifies a log's contents cheaply: its length and modification
/// time in nanoseconds.
fn log_stamp(path: &Path) -> Option<(u64, u128)> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some((meta.len(), modified))
}

/// A name for a file written beside a log and renamed over it. Never a
/// shared name: two writers must not truncate each other's temp file.
fn temp_beside(path: &Path) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("{name}.tmp-{}-{n}", std::process::id()))
}

/// Rename `from` over `to` and make the rename itself survive a crash: the
/// directory is synced after it.
#[cfg(not(windows))]
fn rename_over(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

/// Windows has no directory sync; the move itself is written through.
#[cfg(windows)]
fn rename_over(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<u16>>()
    };
    let (from, to) = (wide(from), wide(to));
    // SAFETY: both paths are NUL-terminated wide strings alive for the call.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Make a rename inside `dir` durable: sync the directory itself.
#[cfg(unix)]
fn sync_dir(dir: &Path) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    extern "C" {
        fn fsync(fd: std::ffi::c_int) -> std::ffi::c_int;
    }
    let dir = File::open(dir)?;
    // SAFETY: fsync only reads the descriptor, which `dir` keeps open.
    if unsafe { fsync(dir.as_raw_fd()) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Windows renames write through (`rename_over`); there is nothing to sync.
#[cfg(not(unix))]
fn sync_dir(_: &Path) -> io::Result<()> {
    Ok(())
}

/// A log opened for reading. Every read of a log goes through one, so
/// tests can count what a peek or a revive actually pulls off the disk.
struct LogRead {
    file: File,
    #[cfg(test)]
    shared: std::sync::Arc<Shared>,
}

impl Read for LogRead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.file.read(buf)?;
        #[cfg(test)]
        self.shared
            .read_bytes
            .fetch_add(read as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(read)
    }
}

impl Seek for LogRead {
    fn seek(&mut self, to: io::SeekFrom) -> io::Result<u64> {
        self.file.seek(to)
    }
}

/// A log's lines read from its end backwards (`Store::backwards`).
struct Backwards {
    file: LogRead,
    floor: u64,
    /// Where the bytes in `pending` begin in the file.
    start: u64,
    /// Bytes read but not yet yielded: the tail of an unfinished line.
    pending: Vec<u8>,
    done: bool,
}

impl Iterator for Backwards {
    type Item = (u64, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        const CHUNK: u64 = 64 * 1024;
        if self.done {
            return None;
        }
        loop {
            if let Some(at) = self.pending.iter().rposition(|byte| *byte == b'\n') {
                let line = self.pending.split_off(at + 1);
                self.pending.pop(); // the newline before it
                return Some((self.start + at as u64 + 1, line));
            }
            if self.start <= self.floor {
                self.done = true;
                return Some((self.floor, std::mem::take(&mut self.pending)));
            }
            // Read the chunk before what is pending; a line longer than any
            // chunk just takes more of them.
            let from = self.start.saturating_sub(CHUNK).max(self.floor);
            let mut chunk = vec![0; (self.start - from) as usize];
            let read = self
                .file
                .seek(io::SeekFrom::Start(from))
                .and_then(|_| self.file.read_exact(&mut chunk));
            if read.is_err() {
                self.done = true;
                return None;
            }
            chunk.extend_from_slice(&self.pending);
            self.pending = chunk;
            self.start = from;
        }
    }
}

impl Store {
    /// Bind a store to `dir`, creating it if it does not exist.
    pub fn open(dir: impl AsRef<Path>) -> io::Result<Self> {
        Self::with_flush_interval(dir, DEFAULT_FLUSH_INTERVAL)
    }

    /// `open`, with the writers' flush interval chosen by the caller.
    pub fn with_flush_interval(
        dir: impl AsRef<Path>,
        flush_interval: std::time::Duration,
    ) -> io::Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        let shared = Shared::of(&dir)?;
        Ok(Self {
            dir,
            flush_interval,
            full_sync_interval: FULL_SYNC_INTERVAL,
            mark_spacing: MARK_SPACING,
            base_cost: 2 * crate::activity::ActivityLimits::default().content_bytes_per_subject as u64,
            shared,
            #[cfg(test)]
            fail_create: false,
            #[cfg(test)]
            fail_delete: false,
        })
    }

    /// Why this process may only read the store — another Ferrite holds its
    /// claim — or `None` when it may write.
    pub fn read_only(&self) -> Option<&str> {
        self.shared.read_only.as_deref()
    }

    /// Every change to a log starts here: a read-only store refuses it.
    fn writable(&self) -> io::Result<()> {
        match &self.shared.read_only {
            Some(reason) => Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                reason.clone(),
            )),
            None => Ok(()),
        }
    }

    /// Bytes read off logs so far, by this store and every handle sharing it.
    #[cfg(test)]
    pub(crate) fn bytes_read(&self) -> u64 {
        self.shared
            .read_bytes
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(crate) fn faults(&self) -> &Faults {
        &self.shared.faults
    }

    /// Syncs run so far on this store: (barriers, full syncs).
    #[cfg(test)]
    pub(crate) fn syncs(&self) -> (u64, u64) {
        let count = |level: SyncLevel| {
            self.shared.syncs[level as usize].load(std::sync::atomic::Ordering::Relaxed)
        };
        (count(SyncLevel::Barrier), count(SyncLevel::Full))
    }

    /// Marks this far apart at least, and replay bases sized for an
    /// Activity with these limits: a small world for a test.
    #[cfg(test)]
    pub(crate) fn scaled(mut self, mark_spacing: u64, limits: crate::activity::ActivityLimits) -> Self {
        self.mark_spacing = mark_spacing;
        self.base_cost = 2 * limits.content_bytes_per_subject as u64;
        self
    }

    /// The writers' longest wait for a full sync, shortened for a test.
    #[cfg(test)]
    pub(crate) fn full_sync_every(mut self, interval: std::time::Duration) -> Self {
        self.full_sync_interval = interval;
        self
    }

    /// A writer appending to `file`, the open log of Thread `id`, which ends
    /// as `tail` says.
    fn writer_on(&self, id: ThreadId, file: File, tail: Tail) -> ThreadWriter {
        let state = std::sync::Arc::new(std::sync::Mutex::new(self.state_on(id, file, tail)));
        self.shared.worker().follow(&state);
        ThreadWriter { state }
    }

    fn state_on(&self, id: ThreadId, file: File, tail: Tail) -> WriterState {
        WriterState {
            file,
            path: self.log_path(id),
            buffer: Vec::new(),
            flush_interval: self.flush_interval,
            buffered_since: None,
            pending_flush: None,
            full_sync_interval: self.full_sync_interval,
            fully_synced: std::time::Instant::now(),
            facts: tail.facts,
            len: tail.len,
            last_mark: tail.last_mark,
            mark_len: tail.mark_len,
            broken: None,
            summary: tail.summary,
            mark_spacing: self.mark_spacing,
            synced: tail.len,
            full_synced: tail.len,
            tracker: tail.tracker,
            inherited: tail.base,
            failure: None,
            parking: false,
            park_marked: false,
            unmark: None,
            shared: self.shared.clone(),
        }
    }

    /// Wait until a parked Thread's log is as its park left it: the open
    /// marker gone and the final mark written. A failed sync there is this
    /// read's error — what the park could not save, a revive must not hide.
    fn settle(&self, id: ThreadId) -> io::Result<()> {
        let Some(worker) = self.shared.worker.get() else {
            return Ok(());
        };
        let path = self.log_path(id);
        loop {
            let mut pending = false;
            for writer in worker.held() {
                let state = worker::lock(&writer);
                if state.path != path {
                    continue;
                }
                if state.parking && (state.unmark.is_some() || !state.park_marked) {
                    if let Some(failure) = &state.failure {
                        return Err(io::Error::other(format!(
                            "history could not be saved: {failure}"
                        )));
                    }
                    pending = true;
                }
            }
            if !pending {
                return Ok(());
            }
            worker.wait_pass(std::time::Duration::from_millis(50));
        }
    }

    /// The process is quitting (E1): write every record any writer of this
    /// store has accepted — always, however long the disk takes — then end
    /// each log in a mark and sync it through the drive's cache, waiting at
    /// most `deadline` for those syncs. Open markers stay: the next launch
    /// reopens what was open. A writer that cannot write keeps what it holds
    /// in `log.pending-<pid>-<n>.jsonl` beside its log, which the next
    /// revive reports.
    pub fn quit(&self, deadline: std::time::Duration) {
        let Some(worker) = self.shared.worker.get() else {
            return;
        };
        let writers = worker.queue().writers();
        for writer in &writers {
            let mut state = worker::lock(writer);
            if state.flush().is_err() {
                state.keep_pending();
                continue;
            }
            if !state.parking {
                state.park(false);
            }
        }
        worker.poke();
        let until = std::time::Instant::now() + deadline;
        while std::time::Instant::now() < until
            && writers
                .iter()
                .any(|writer| !worker::lock(writer).finished())
        {
            worker.wait_pass(std::time::Duration::from_millis(10));
        }
    }

    /// Records a writer could not write before Ferrite last quit, kept
    /// beside this Thread's log (`quit`).
    pub fn pending_records(&self, id: ThreadId) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(self.dir.join(id.to_string())) else {
            return Vec::new();
        };
        let mut pending: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("log.pending-"))
            })
            .collect();
        pending.sort();
        pending
    }

    /// Write what the writers of this store hold, as a panic would.
    #[cfg(test)]
    pub(crate) fn rescue(&self) {
        rescue(Some(&self.dir));
    }

    /// Wait until every park's open marker is gone: what the open state
    /// says must be what the operator last did.
    fn settle_markers(&self) {
        let Some(worker) = self.shared.worker.get() else {
            return;
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while worker
            .held()
            .iter()
            .any(|writer| worker::lock(writer).unmark.is_some())
            && std::time::Instant::now() < deadline
        {
            worker.wait_pass(std::time::Duration::from_millis(20));
        }
    }

    /// Wait until the worker has synced everything written so far.
    #[cfg(test)]
    pub(crate) fn settle_all(&self) {
        let Some(worker) = self.shared.worker.get() else {
            return;
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            let held_done = worker.held().iter().all(|writer| worker::lock(writer).finished());
            let live = worker.queue().live_writers();
            let live_done = live.iter().all(|writer| {
                let state = worker::lock(writer);
                state.synced >= state.len && !state.mark_due()
            });
            if held_done && live_done {
                return;
            }
            worker.wait_pass(std::time::Duration::from_millis(20));
        }
        panic!("the store's worker did not settle");
    }

    /// What the worker synced and marked, in order.
    #[cfg(test)]
    pub(crate) fn journal(&self) -> Vec<(&'static str, u64)> {
        worker::lock(&self.shared.journal).clone()
    }

    /// Open one Thread's log for reading.
    fn read_log(&self, id: ThreadId) -> io::Result<LogRead> {
        Ok(LogRead {
            file: File::open(self.log_path(id))?,
            #[cfg(test)]
            shared: self.shared.clone(),
        })
    }

    /// One Thread's whole log, as bytes.
    fn read_whole_log(&self, id: ThreadId) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.read_log(id)?.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    #[cfg(test)]
    pub(crate) fn refuse_create(mut self) -> Self {
        self.fail_create = true;
        self
    }

    #[cfg(test)]
    pub(crate) fn refuse_delete(mut self) -> Self {
        self.fail_delete = true;
        self
    }

    /// Mint a new Thread from an already-resolved workspace binding. The
    /// Thread is durable before this returns: a crash immediately after
    /// still shows it. The store runs no git and places no paths — worktree
    /// placement is the registry's (#29) — it only writes the resolved
    /// truth down.
    pub fn create(
        &self,
        provider: Provider,
        project: Option<ProjectId>,
        binding: WorkspaceBinding,
    ) -> io::Result<(ThreadId, ThreadWriter)> {
        self.create_with_model(provider, None, project, binding)
    }

    pub(crate) fn create_with_model(
        &self,
        provider: Provider,
        model: Option<String>,
        project: Option<ProjectId>,
        binding: WorkspaceBinding,
    ) -> io::Result<(ThreadId, ThreadWriter)> {
        #[cfg(test)]
        if self.fail_create {
            return Err(io::Error::other("stub refused Thread creation"));
        }
        self.writable()?;
        let mut next = self.thread_ids()?.last().map_or(1, |id| id.get() + 1);
        loop {
            match fs::create_dir(self.dir.join(next.to_string())) {
                Ok(()) => break,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => next += 1,
                Err(e) => return Err(e),
            }
        }
        let id = ThreadId::new(next);

        let mut file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(self.log_path(id))?;
        let header = Header {
            schema: SCHEMA_VERSION,
            provider,
            workspace: Some(PersistedBinding::from_live(&binding)),
            // Work starts in the binding itself; a root is picked later.
            session_project_root: None,
            // And on the provider's default model; a choice is picked later.
            model,
            project_id: project,
            title: None,
            // And on its default effort.
            effort: None,
        };
        let written = line(&header)?;
        file.write_all(written.as_bytes())?;
        self.shared.sync(&file, SyncLevel::Full)?;
        let tail = Tail {
            facts: header.facts(),
            len: written.len() as u64,
            last_mark: None,
            mark_len: 0,
            summary: Some(Summary::default()),
            tracker: Some(Tracker::new(self.base_cost)),
            base: None,
        };
        Ok((id, self.writer_on(id, file, tail)))
    }

    /// Where the store keeps its files — what the registry binds to, so the
    /// registry file and the central worktree layout live beside the Thread
    /// logs (#29).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Remove one Thread entirely: its log, its directory, everything in it.
    /// The caller settles the worktree's fate first — anything still under
    /// the Thread's directory goes with it.
    pub fn delete(&self, id: ThreadId) -> io::Result<()> {
        #[cfg(test)]
        if self.fail_delete {
            return Err(io::Error::other("stub refused Thread deletion"));
        }
        self.writable()?;
        // A parking Thread's writer still holds its log open; whatever it
        // had left to sync goes with the Thread.
        if let Some(worker) = self.shared.worker.get() {
            worker.forget(&self.log_path(id));
        }
        fs::remove_dir_all(self.dir.join(id.to_string()))
    }

    /// Every Thread in the store, sorted by creation.
    pub fn thread_ids(&self) -> io::Result<Vec<ThreadId>> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            if let Ok(number) = entry.file_name().to_string_lossy().parse::<u64>() {
                ids.push(ThreadId::new(number));
            }
        }
        ids.sort_unstable();
        Ok(ids)
    }

    /// Remember that the operator has this Thread open. The tiny sidecar is
    /// separate from history because opening a Pane is cockpit state, not a
    /// conversation event.
    pub fn mark_open(&self, id: ThreadId) -> io::Result<()> {
        self.writable()?;
        self.shared.sync(
            &File::create(self.dir.join(OPEN_STATE_MARKER))?,
            SyncLevel::Barrier,
        )?;
        self.shared.sync(
            &File::create(self.dir.join(id.to_string()).join(OPEN_MARKER))?,
            SyncLevel::Barrier,
        )
    }

    /// Remember an explicit park. Missing markers are also the legacy format,
    /// in which Threads were all considered parked on startup.
    pub fn mark_parked(&self, id: ThreadId) -> io::Result<()> {
        self.writable()?;
        self.shared.sync(
            &File::create(self.dir.join(OPEN_STATE_MARKER))?,
            SyncLevel::Barrier,
        )?;
        match fs::remove_file(self.dir.join(id.to_string()).join(OPEN_MARKER)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Threads whose Panes were open when the previous process ended.
    pub fn open_threads(&self) -> io::Result<Vec<ThreadId>> {
        self.settle_markers();
        Ok(self
            .thread_ids()?
            .into_iter()
            .filter(|id| self.dir.join(id.to_string()).join(OPEN_MARKER).is_file())
            .collect())
    }

    pub fn tracks_open_state(&self) -> bool {
        self.settle_markers();
        self.dir.join(OPEN_STATE_MARKER).is_file()
    }

    /// Header-only read of one Thread: what wrote it, for what provider,
    /// working where — without replaying its log. This exists for render
    /// paths (#21's nav lists parked Threads): `read_until` pulls buffered
    /// chunks only up to the first newline, so the records after the header
    /// are never read off the disk, and a huge log peeks at the same cost
    /// as an empty one.
    /// When a Thread was last written to — its log's modification time.
    /// The log is appended on every prompt, every stream and every act, so
    /// its mtime *is* "last used", and it costs one `stat` rather than a
    /// replay. A log that cannot be stat'd has no time; a row with no time
    /// simply says nothing, and sorts last.
    pub fn last_used(&self, id: ThreadId) -> Option<SystemTime> {
        fs::metadata(self.log_path(id))
            .and_then(|meta| meta.modified())
            .ok()
    }

    pub fn peek(&self, id: ThreadId) -> Result<ThreadMeta, LoadError> {
        use std::io::BufRead;
        self.settle(id)?;
        let mut first = Vec::new();
        io::BufReader::new(self.read_log(id)?).read_until(b'\n', &mut first)?;
        let header: Header = serde_json::from_slice(&first).map_err(|_| LoadError::Corrupt {
            detail: format!("thread {id} has no readable header"),
        })?;
        if header.schema > SCHEMA_VERSION {
            return Err(LoadError::FutureSchema {
                found: header.schema,
                supported: SCHEMA_VERSION,
            });
        }
        if header.schema < 13 {
            // Before amendments, the header was the only place facts lived.
            return Ok(header.facts().meta());
        }
        // Back from the end to the newest mark: the newest facts on the way,
        // and the prompts and resets after the mark, which its counts miss.
        let mut facts = None;
        let mut after = Summary::default();
        let mut reset = false;
        let mut marked = None;
        for (_, line) in self.backwards(id, first.len() as u64)? {
            let kinds = [
                &br#"{"type":"prompt","#[..],
                br#"{"type":"conversation_reset","#,
                br#"{"type":"facts","#,
                br#"{"type":"mark","#,
                br#"{"type":"handover","#,
            ];
            if !kinds.iter().any(|kind| line.starts_with(kind)) {
                continue;
            }
            // A crash's fragment may start like a record; it is not one.
            let Ok(record) = serde_json::from_slice::<Record>(&line) else {
                continue;
            };
            if facts.is_none() {
                facts = record.facts().cloned();
            }
            match record {
                Record::Prompt { .. } => {
                    after.prompted = true;
                    if !reset {
                        after.turns += 1;
                    }
                }
                Record::ConversationReset { .. } => reset = true,
                Record::Mark { summary, .. } => {
                    marked = Some(summary);
                    break;
                }
                _ => {}
            }
        }
        let summary = match marked {
            // Read to the header: the counts are exact.
            None => Some(after),
            Some(None) => None,
            Some(Some(mark)) => Some(Summary {
                turns: if reset { after.turns } else { mark.turns + after.turns },
                prompted: mark.prompted || after.prompted,
                subagents: mark.subagents,
            }),
        };
        let mut meta = facts.unwrap_or_else(|| header.facts()).meta();
        meta.summary = summary.as_ref().map(Summary::public);
        Ok(meta)
    }

    /// A parked Thread's counts without replaying it: off its tail
    /// (`peek`) when the log is schema 13; otherwise from a derived cache
    /// beside the logs, filled once per log by `count` over a full load —
    /// call this off the UI thread. The cache is pure acceleration: delete
    /// it and the next call counts again.
    pub fn summary(
        &self,
        id: ThreadId,
        count_subagents: impl FnOnce(&ThreadSnapshot) -> usize,
    ) -> Result<ThreadSummary, LoadError> {
        let meta = self.peek(id)?;
        if let Some(summary) = meta.summary.filter(|summary| summary.subagents.is_some()) {
            return Ok(summary);
        }
        if let Some(cached) = self.cached_summary(id) {
            return Ok(cached.public());
        }
        let snapshot = self.load(id)?;
        let summary = Summary {
            turns: snapshot.prompt_texts().len() as u64,
            prompted: snapshot
                .records
                .iter()
                .any(|record| matches!(record, Record::Prompt { .. })),
            subagents: Some(count_subagents(&snapshot) as u64),
        };
        self.remember_summary(id, &summary);
        Ok(summary.public())
    }

    /// Whether a prompt was ever sent on this Thread — what locks its
    /// provider — without a full load when the log can say so cheaply.
    pub fn prompted(&self, id: ThreadId) -> Result<bool, LoadError> {
        if let Some(summary) = self.peek(id)?.summary {
            return Ok(summary.prompted);
        }
        if let Some(cached) = self.cached_summary(id) {
            return Ok(cached.prompted);
        }
        if self.peek_first_prompt(id)?.is_some() {
            return Ok(true);
        }
        if fs::metadata(self.log_path(id))?.len() <= FIRST_PROMPT_SCAN as u64 {
            return Ok(false);
        }
        Ok(self
            .load(id)?
            .records
            .iter()
            .any(|record| matches!(record, Record::Prompt { .. })))
    }

    /// The derived summary of a pre-13 log, if one was counted for it as it
    /// is now (its length and modification time).
    fn cached_summary(&self, id: ThreadId) -> Option<Summary> {
        let stamp = log_stamp(&self.log_path(id))?;
        let mut cache = self.summaries();
        cache
            .as_mut()?
            .get(&id.get())
            .filter(|cached| cached.stamp == stamp)
            .map(|cached| cached.summary.clone())
    }

    fn remember_summary(&self, id: ThreadId, summary: &Summary) {
        let Some(stamp) = log_stamp(&self.log_path(id)) else {
            return;
        };
        let mut cache = self.summaries();
        let Some(entries) = cache.as_mut() else {
            return;
        };
        entries.insert(
            id.get(),
            CachedSummary {
                stamp,
                summary: summary.clone(),
            },
        );
        if self.writable().is_err() {
            return;
        }
        // A cache: written whole and renamed, never synced; a lost one is
        // counted again.
        let path = self.dir.join(SUMMARIES);
        let tmp = temp_beside(&path);
        let written = serde_json::to_vec(&*entries)
            .map_err(io::Error::other)
            .and_then(|bytes| fs::write(&tmp, bytes))
            .and_then(|()| fs::rename(&tmp, &path));
        if written.is_err() {
            let _ = fs::remove_file(&tmp);
        }
    }

    /// The summary cache, loaded on first use.
    fn summaries(&self) -> std::sync::MutexGuard<'_, Option<BTreeMap<u64, CachedSummary>>> {
        let mut cache = self
            .shared
            .summaries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if cache.is_none() {
            *cache = Some(
                fs::read(self.dir.join(SUMMARIES))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                    .unwrap_or_default(),
            );
        }
        cache
    }

    /// One log's lines from its end back to `floor` (the end of its header),
    /// newest first, each with its offset. What a peek or a reopen reads
    /// instead of the whole log: it stops as soon as its caller does.
    fn backwards(&self, id: ThreadId, floor: u64) -> io::Result<Backwards> {
        let mut file = self.read_log(id)?;
        let len = file.file.metadata()?.len();
        let mut end = len;
        if len > floor {
            // A final newline ends the last line; it does not start one.
            let mut last = [0u8];
            file.file.seek(io::SeekFrom::Start(len - 1))?;
            file.read_exact(&mut last)?;
            if last[0] == b'\n' {
                end = len - 1;
            }
        }
        Ok(Backwards {
            file,
            floor,
            start: end.max(floor),
            pending: Vec::new(),
            done: len <= floor,
        })
    }

    /// The first prompt the operator sent, without loading the log: the
    /// records are read only until the first `prompt`, and never past
    /// `FIRST_PROMPT_SCAN` bytes — a huge log with no early prompt costs
    /// that much and no more. `None` when no prompt was found in reach.
    pub fn peek_first_prompt(&self, id: ThreadId) -> Result<Option<String>, LoadError> {
        use std::io::BufRead;
        let mut reader = io::BufReader::new(self.read_log(id)?);
        let mut line = Vec::new();
        let mut read = 0usize;
        // The header first; it is not a record.
        reader.read_until(b'\n', &mut line)?;
        loop {
            line.clear();
            let n = reader.read_until(b'\n', &mut line)?;
            if n == 0 {
                return Ok(None);
            }
            read += n;
            if let Ok(serde_json::Value::Object(record)) =
                serde_json::from_slice::<serde_json::Value>(&line)
            {
                if record.get("type").and_then(serde_json::Value::as_str) == Some("prompt") {
                    return Ok(record
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string));
                }
            }
            if read > FIRST_PROMPT_SCAN {
                return Ok(None);
            }
        }
    }

    /// Load one Thread's snapshot: its history and resume metadata.
    pub fn load(&self, id: ThreadId) -> Result<ThreadSnapshot, LoadError> {
        // Bytes, not a String: a crash can tear the tail mid-character, and
        // a loader that insists the whole file is UTF-8 would lose the
        // Thread over its last three bytes.
        self.settle(id)?;
        Ok(parse(id, &self.read_whole_log(id)?)?.snapshot)
    }

    /// Record where inside the binding this Thread's work happens — or
    /// `None` to work in the binding itself. The header is the log's first
    /// line, so the change rewrites the log whole (written beside, renamed
    /// over — the same crash safety as any upgrade). `writer` is the
    /// Thread's open writer, if one exists: the rename would leave its
    /// handle on the replaced inode, where appends vanish silently — so it
    /// is flushed before the rewrite reads the log and swapped onto the new
    /// file after. The swap happens only once the rename has succeeded: on
    /// any error the caller's writer is untouched and still valid.
    #[cfg(test)]
    pub(crate) fn set_session_project_root(
        &self,
        id: ThreadId,
        root: Option<PathBuf>,
        writer: Option<&mut ThreadWriter>,
    ) -> Result<(), LoadError> {
        self.amend(id, writer, |facts| facts.session_project_root = root)
    }

    /// Record which provider serves this Thread, and the model and effort
    /// it chose — `None` for the provider's default of either (#25). The
    /// same header rewrite as `set_session_project_root`, with the same
    /// writer contract: the Thread's open writer, if one exists, is flushed
    /// before and swapped onto the new file after, and on any error it is
    /// untouched and still valid on the old one.
    pub fn set_provider(
        &self,
        id: ThreadId,
        provider: Provider,
        model: Option<String>,
        effort: Option<String>,
        writer: Option<&mut ThreadWriter>,
    ) -> Result<(), LoadError> {
        self.amend(id, writer, |facts| {
            facts.provider = provider;
            facts.model = model;
            facts.effort = effort;
        })
    }

    /// Record the checkout this Thread now works in — the binding moving
    /// after the agent it follows (`workspace::follow`). The same header
    /// rewrite as `set_provider`, with the same writer contract: the
    /// Thread's open writer, if one exists, is flushed before and swapped
    /// onto the new file after, and on any error it is untouched and still
    /// valid on the old one. Every path that later reads the binding —
    /// revive, respawn, the header — sees the new one from here.
    pub fn set_workspace(
        &self,
        id: ThreadId,
        binding: &WorkspaceBinding,
        writer: Option<&mut ThreadWriter>,
    ) -> Result<(), LoadError> {
        self.amend(id, writer, |facts| {
            facts.workspace = Some(PersistedBinding::from_live(binding));
        })
    }

    /// Read every exchange and commit the switch in one call. The cockpit
    /// does the two apart (`read_exchanges`, `commit_handover`), so a long
    /// log is read off the UI thread.
    #[cfg(test)]
    pub(crate) fn hand_over(
        &self,
        id: ThreadId,
        provider: Provider,
        model: Option<String>,
        writer: &mut ThreadWriter,
    ) -> Result<Handover, LoadError> {
        self.writable()?;
        writer.flush()?;
        // Every exchange the log holds, read whole (ADR 0009, invariant 3).
        let exchanges = self.load(id)?.exchanges_so_far();
        self.commit_handover(provider, model, writer, exchanges)
    }

    /// The exchanges a provider switch made now carries, read whole from the
    /// log as it stands (invariant 3): here, for a small log; on a thread of
    /// its own for a large one, so no caller waits on it. Everything the
    /// writer holds must be written first.
    pub(crate) fn read_exchanges(&self, id: ThreadId) -> ExchangeRead {
        let store = self.clone();
        let read = move || store.load(id).map(|snapshot| snapshot.exchanges_so_far());
        let size = fs::metadata(self.log_path(id)).map_or(0, |meta| meta.len());
        if size <= INLINE_EXCHANGE_READ {
            return ExchangeRead::Ready(Some(read()));
        }
        let (send, receive) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("ferrite-handover".into())
            .spawn(move || {
                let _ = send.send(read());
            });
        match spawned {
            Ok(_) => ExchangeRead::Pending(receive),
            Err(error) => ExchangeRead::Ready(Some(Err(LoadError::Io(error)))),
        }
    }

    /// Commit a provider switch: one line, carrying the facts it switches
    /// to, so the switch and its facts land together or not at all. Answers
    /// the Handover the next prompt owes, with `exchanges` read before it.
    pub(crate) fn commit_handover(
        &self,
        provider: Provider,
        model: Option<String>,
        writer: &mut ThreadWriter,
        exchanges: Vec<(String, String)>,
    ) -> Result<Handover, LoadError> {
        self.writable()?;
        let mut writer = writer.lock();
        let from = writer.facts.provider;
        let mut facts = writer.facts.clone();
        facts.provider = provider;
        facts.model = model.clone();
        facts.effort = None;
        writer.commit(Record::Handover {
            from,
            to: provider,
            model,
            facts: Some(facts),
        })?;
        Ok(Handover {
            from,
            exchanges,
            delivered: false,
        })
    }

    /// Record the model and effort this Thread runs with, leaving its
    /// provider as the log has it: a caller's stale view of the provider
    /// (a switch committed a moment ago) can never be written back over it.
    pub fn set_tuning(
        &self,
        id: ThreadId,
        model: Option<String>,
        effort: Option<String>,
        writer: Option<&mut ThreadWriter>,
    ) -> Result<(), LoadError> {
        self.amend(id, writer, |facts| {
            facts.model = model;
            facts.effort = effort;
        })
    }

    /// Change a Thread's facts by `change` (a delta, applied to the facts
    /// the writer holds — never a whole restatement from the caller) and
    /// append the result. Through the Thread's open writer when it has
    /// one; otherwise through one opened for the purpose, which leaves the
    /// parked log synced through the drive's cache.
    fn amend(
        &self,
        id: ThreadId,
        writer: Option<&mut ThreadWriter>,
        change: impl FnOnce(&mut Facts),
    ) -> Result<(), LoadError> {
        self.writable()?;
        match writer {
            Some(writer) => writer.lock().amend(change)?,
            None => {
                let writer = self.writer(id)?;
                writer.lock().amend(change)?;
                // Parked: the worker marks the log and syncs it fully.
                writer.close();
            }
        }
        Ok(())
    }

    pub fn set_title(
        &self,
        id: ThreadId,
        title: String,
        writer: Option<&mut ThreadWriter>,
    ) -> Result<(), LoadError> {
        self.amend(id, writer, |facts| facts.title = Some(title))
    }

    /// Reopen one Thread's log for appending — how a revived Thread's next
    /// turns reach the same history after a restart.
    ///
    /// A log written at an older schema is upgraded whole before the first
    /// append: new records under an old header would make the file a lie,
    /// and a reader of that older schema would stop dead at the first record
    /// it cannot know.
    ///
    /// A log a crash left torn is repaired first (`repair`).
    pub fn writer(&self, id: ThreadId) -> Result<ThreadWriter, LoadError> {
        use std::io::BufRead;
        self.writable()?;
        self.settle(id)?;
        self.sweep_temps(id);
        let mut first = Vec::new();
        io::BufReader::new(self.read_log(id)?).read_until(b'\n', &mut first)?;
        let schema = serde_json::from_slice::<Header>(&first)
            .map(|header| header.schema)
            .unwrap_or(0);
        if schema > SCHEMA_VERSION {
            return Err(LoadError::FutureSchema {
                found: schema,
                supported: SCHEMA_VERSION,
            });
        }
        if schema < SCHEMA_VERSION {
            let bytes = self.read_whole_log(id)?;
            let parsed = parse(id, &bytes)?;
            return self.upgrade(id, &bytes, parsed);
        }
        self.reopen(id, &first)
    }

    /// Reopen a current log for appending, reading only its tail: from the
    /// newest mark that parses — which vouches for every line before it —
    /// to the end. Every line in that stretch must parse; the first that
    /// does not, and everything after it, is repaired away (`repair`).
    fn reopen(&self, id: ThreadId, first: &[u8]) -> Result<ThreadWriter, LoadError> {
        let header_end = first.len() as u64;
        let header: Header = serde_json::from_slice(first).map_err(|_| LoadError::Corrupt {
            detail: format!("thread {id} has no readable header"),
        })?;
        let mark = self.backwards(id, header_end)?.find_map(|(at, line)| {
            if !line.starts_with(br#"{"type":"mark""#) {
                return None;
            }
            match serde_json::from_slice::<Record>(&line) {
                Ok(Record::Mark {
                    facts,
                    summary,
                    base,
                    ..
                }) => Some((at, line.len() as u64 + 1, facts, summary, base)),
                _ => None,
            }
        });
        let (from, mut facts, mut summary) = match &mark {
            Some((at, _, facts, summary, _)) => (*at, facts.clone(), summary.clone()),
            // Read from the header on, so the counts are exact.
            None => (header_end, header.facts(), Some(Summary::default())),
        };
        // From the header on, the whole log passes through: follow it.
        let mut tracker = mark.is_none().then(|| Tracker::new(self.base_cost));
        let mut tail = Vec::new();
        let mut file = self.read_log(id)?;
        file.seek(io::SeekFrom::Start(from))?;
        file.read_to_end(&mut tail)?;
        let mut readable = 0;
        while readable < tail.len() {
            let end = tail[readable..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(tail.len(), |at| readable + at);
            let Ok(record) = serde_json::from_slice::<Record>(&tail[readable..end]) else {
                break;
            };
            if let Some(restated) = record.facts() {
                facts = restated.clone();
            }
            if let Some(summary) = summary.as_mut() {
                summary.observe(&record);
            }
            if let Some(tracker) = tracker.as_mut() {
                tracker.observe(from + readable as u64, &record);
            }
            readable = (end + 1).min(tail.len());
        }
        self.repair_tail(id, from, &tail, readable)?;
        let file = OpenOptions::new().append(true).open(self.log_path(id))?;
        let len = file.metadata()?.len();
        let (last_mark, mark_len, base) = match mark {
            Some((at, mark_len, _, _, base)) => (Some(at), mark_len as usize, base),
            None => (None, 0, None),
        };
        Ok(self.writer_on(
            id,
            file,
            Tail {
                facts,
                len,
                last_mark,
                mark_len,
                summary,
                tracker,
                base,
            },
        ))
    }

    /// Upgrade an older log to the current schema before anything is
    /// appended (`rewrite_from`), ending it with a mark.
    fn upgrade(&self, id: ThreadId, bytes: &[u8], parsed: Parsed) -> Result<ThreadWriter, LoadError> {
        let facts = parsed.snapshot.facts();
        let mut summary = Summary::default();
        for record in &parsed.snapshot.records {
            summary.observe(record);
        }
        // A parked row counted this log's subagents once already: keep it.
        summary.subagents = self
            .cached_summary(id)
            .and_then(|cached| cached.subagents);
        // Follow every record where it will sit in the upgraded log — the
        // same bytes after a new header line — so the upgrade's mark names
        // a base, and a long Thread's first revive is already bounded.
        let old_header = parsed.offsets.first().copied().unwrap_or(parsed.readable as u64);
        let new_header = line(&Header::of(facts.clone()))?.len() as u64;
        let mut tracker = Tracker::new(self.base_cost);
        for (at, record) in parsed.offsets.iter().zip(&parsed.snapshot.records) {
            tracker.observe(at - old_header + new_header, record);
        }
        let base = tracker.base();
        let mark = |summary: &Summary| Record::Mark {
            facts: facts.clone(),
            prev: None,
            summary: Some(summary.clone()),
            base: base.clone(),
        };
        let marked = line(&mark(&summary))?;
        let (file, _) = self.rewrite_from(id, bytes, parsed, |_| {}, Some(mark(&summary)))?;
        let len = file.metadata()?.len();
        Ok(self.writer_on(
            id,
            file,
            Tail {
                facts,
                len,
                last_mark: Some(len - marked.len() as u64),
                mark_len: marked.len(),
                summary: Some(summary),
                tracker: Some(tracker),
                base,
            },
        ))
    }

    /// Remove temp files a crash left beside one Thread's log. Only this
    /// process writes the store (its claim), and it is not mid-rewrite of a
    /// Thread it is opening, so every one of them is stale.
    fn sweep_temps(&self, id: ThreadId) {
        let Ok(entries) = fs::read_dir(self.dir.join(id.to_string())) else {
            return;
        };
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with("log.jsonl.tmp-")
            {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    /// Keep every byte past `readable` in `log.damaged-<readable>.jsonl`,
    /// synced, before anything cuts it off the log.
    fn keep_damage(&self, id: ThreadId, at: u64, damaged: &[u8]) -> io::Result<()> {
        let kept = self
            .log_path(id)
            .with_file_name(format!("log.damaged-{at}.jsonl"));
        let mut copy = File::create(&kept)?;
        copy.write_all(damaged)?;
        self.shared.sync(&copy, SyncLevel::Full)
    }

    /// Make the log end in whole, readable records before anything is
    /// appended: an append straight after a crash's fragment would fuse
    /// with it into one unreadable line, where every reader stops, hiding
    /// every turn after the crash.
    ///
    /// Nothing is rewritten. A last record missing only its newline gets
    /// it. Anything unreadable is cut off in place — but only once every
    /// byte about to go is kept in `log.damaged-<offset>.jsonl` and synced:
    /// a crash's fragment was never a record, but damage inside the log
    /// may hide records after it, and those must never be lost. If keeping
    /// them fails, nothing is cut and the log stays closed to appends.
    ///
    /// `tail` is the log from byte `from` on; `readable` is where, within
    /// it, the readable records end.
    fn repair_tail(&self, id: ThreadId, from: u64, tail: &[u8], readable: usize) -> io::Result<()> {
        let path = self.log_path(id);
        if readable == tail.len() {
            if !tail.is_empty() && !tail.ends_with(b"\n") {
                let mut log = OpenOptions::new().append(true).open(&path)?;
                log.write_all(b"\n")?;
                self.shared.sync(&log, SyncLevel::Full)?;
            }
            return Ok(());
        }
        self.keep_damage(id, from + readable as u64, &tail[readable..])?;
        // Not the append handle: Windows truncates only through a handle
        // opened for writing.
        let log = OpenOptions::new().write(true).open(&path)?;
        log.set_len(from + readable as u64)?;
        self.shared.sync(&log, SyncLevel::Full)
    }

    /// Reopen a parked Thread for a revive: its writer, and what to replay,
    /// read in time proportional to what Activity retains rather than to
    /// the log (ADR 0009). The newest mark names a base — a turn start far
    /// enough back that Activity's window lies wholly after it — and the
    /// records before it that a replay from there needs first; those are
    /// read by offset, then the log from the base on. Children known from
    /// before the base come back as identity, status and outcome, their
    /// content left on disk for the ADR-0002 loader to restore when shown.
    /// Without a usable base the whole log is read, as before.
    ///
    /// A switch whose carry never went out still gets every exchange, read
    /// whole (invariant 3).
    pub fn revive(&self, id: ThreadId) -> Result<Revival, LoadError> {
        let handle = self.writer(id)?;
        let mut writer = handle.lock();
        let bounded = match writer.inherited.clone() {
            Some(base) => self.read_from(id, &base, writer.len)?,
            None => None,
        };
        let (records, tracker, evict) = match bounded {
            Some(read) => read,
            None => {
                let parsed = parse(id, &self.read_whole_log(id)?)?;
                let mut tracker = Tracker::new(self.base_cost);
                for (at, record) in parsed.offsets.iter().zip(&parsed.snapshot.records) {
                    tracker.observe(*at, record);
                }
                (parsed.snapshot.records, tracker, Vec::new())
            }
        };
        // From here the writer follows the log, so its marks name fresh bases.
        writer.tracker = Some(tracker);
        let facts = writer.facts.clone();
        let snapshot = ThreadSnapshot {
            id,
            provider: facts.provider,
            schema: SCHEMA_VERSION,
            workspace: facts.workspace,
            session_project_root: facts.session_project_root,
            model: facts.model,
            project_id: facts.project_id,
            title: facts.title,
            effort: facts.effort,
            records,
        };
        let owed = match snapshot.last_handover() {
            Some(handover) if !handover.delivered => self.load(id)?.last_handover(),
            _ => None,
        };
        let prompted = writer.summary.as_ref().is_some_and(|summary| summary.prompted)
            || snapshot
                .records
                .iter()
                .any(|record| matches!(record, Record::Prompt { .. }));
        drop(writer);
        Ok(Revival {
            snapshot,
            evict,
            owed,
            prompted,
            writer: Some(handle),
        })
    }

    /// The records a replay from `base` needs: the carried ones, read by
    /// offset, then the log from the base to `end`. `None` when the base
    /// cannot be trusted (an offset that is not a whole record), and the
    /// caller reads the whole log instead.
    #[allow(clippy::type_complexity)]
    fn read_from(
        &self,
        id: ThreadId,
        base: &Base,
        end: u64,
    ) -> Result<Option<(Vec<Record>, Tracker, Vec<String>)>, LoadError> {
        use std::io::BufRead;
        // Small records mostly, read in log order: skip ahead within what
        // is buffered rather than seek and refill for each one.
        let mut file = io::BufReader::with_capacity(1024, self.read_log(id)?);
        let mut position = 0;
        let go_to = |file: &mut io::BufReader<LogRead>, position: u64, at: u64| {
            if at >= position && at - position <= file.buffer().len() as u64 {
                file.seek_relative((at - position) as i64)
            } else {
                file.seek(io::SeekFrom::Start(at)).map(|_| ())
            }
        };
        let mut tracker = Tracker::new(self.base_cost);
        let mut records = Vec::new();
        let mut evict = Vec::new();
        let mut line = Vec::new();
        for at in &base.carry {
            line.clear();
            go_to(&mut file, position, *at)?;
            position = *at + file.read_until(b'\n', &mut line)? as u64;
            let Ok(record) = serde_json::from_slice::<Record>(&line) else {
                return Ok(None);
            };
            if record.is_bookkeeping() || *at >= base.at {
                return Ok(None);
            }
            if let Record::Activity { observation } = &record {
                evict.extend(observation.children().map(str::to_string));
            }
            tracker.seed(*at, &record);
            records.push(record);
        }
        tracker.idle();
        go_to(&mut file, position, base.at)?;
        let mut at = base.at;
        while at < end {
            line.clear();
            let read = file.read_until(b'\n', &mut line)?;
            if read == 0 {
                break;
            }
            let Ok(record) = serde_json::from_slice::<Record>(&line) else {
                return Ok(None);
            };
            if at == base.at && !matches!(record, Record::Prompt { .. }) {
                return Ok(None);
            }
            tracker.observe(at, &record);
            if !record.is_bookkeeping() {
                records.push(record);
            }
            at += read as u64;
        }
        evict.sort_unstable();
        evict.dedup();
        Ok(Some((records, tracker, evict)))
    }

    /// Recover one child's recent persisted content for an off-thread cache
    /// reload. No current status or actionable request is restored. The return
    /// value is bounded by Activity's default rendering budget; omitted local
    /// history is explicitly Partial. Two scans retain the alias graph and a
    /// bounded target projection, plus one line/decoded record at a time.
    /// A single externally supplied line can still be arbitrarily large.
    /// Call this on a worker, never from the paint path.
    pub fn agent_inputs(
        &self,
        thread: ThreadId,
        key: &crate::activity::AgentKey,
    ) -> Result<Vec<crate::activity::ActivityInput>, LoadError> {
        let file = File::open(self.log_path(thread))?;
        let through = file.metadata()?.len();
        activity::read_agent_inputs(
            file,
            thread,
            key,
            through,
            crate::activity::ActivityLimits::default(),
        )
    }

    /// Read only the complete prefix returned by `ThreadWriter::checkpoint`.
    /// Later appends belong to the caller's live-event buffer, so streamed
    /// deltas are replayed exactly once when an evicted cache is restored.
    /// The caller invalidates this work on a header rewrite or Thread change.
    pub fn agent_inputs_at(
        &self,
        thread: ThreadId,
        key: &crate::activity::AgentKey,
        through: u64,
    ) -> Result<Vec<crate::activity::ActivityInput>, LoadError> {
        activity::read_agent_inputs(
            File::open(self.log_path(thread))?,
            thread,
            key,
            through,
            crate::activity::ActivityLimits::default(),
        )
    }

    /// Replace a Thread's log: a header at the current schema saying what
    /// `amend` leaves in its facts, then every record it already holds,
    /// byte for byte — never decoded and encoded again, so nothing an old
    /// writer wrote is normalised away — then `extra`. Anything unreadable
    /// past the records is kept beside the log first (`keep_damage`).
    /// Answers an append handle on the new log, and what it now holds.
    /// Only an upgrade rewrites a log; every other change appends.
    fn rewrite_from(
        &self,
        id: ThreadId,
        bytes: &[u8],
        parsed: Parsed,
        amend: impl FnOnce(&mut ThreadSnapshot),
        extra: Option<Record>,
    ) -> Result<(File, ThreadSnapshot), LoadError> {
        let Parsed {
            mut snapshot,
            readable,
            ..
        } = parsed;
        amend(&mut snapshot);
        let header_end = bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |at| at + 1)
            .min(readable);
        let mut contents = line(&Header::of(snapshot.facts()))?.into_bytes();
        contents.extend_from_slice(&bytes[header_end..readable]);
        if !contents.ends_with(b"\n") {
            // The last record lost only its newline to a crash.
            contents.push(b'\n');
        }
        if let Some(record) = extra {
            contents.extend_from_slice(line(&record)?.as_bytes());
            if !record.is_bookkeeping() {
                snapshot.records.push(record);
            }
        }
        if readable < bytes.len() {
            self.keep_damage(id, readable as u64, &bytes[readable..])?;
        }
        let file = self.replace(id, &contents)?;
        snapshot.schema = SCHEMA_VERSION;
        Ok((file, snapshot))
    }

    /// Put `contents` in place of a Thread's log so that a crash at any
    /// moment leaves one whole log or the other: written beside under a
    /// name of its own, synced through the drive's cache, renamed over, and
    /// the rename itself made durable. Answers an append handle taken on
    /// the new file before the rename, so every failure here happens while
    /// the original log (and every handle on it) is still the real one.
    fn replace(&self, id: ThreadId, contents: &[u8]) -> io::Result<File> {
        let path = self.log_path(id);
        let tmp = temp_beside(&path);
        let mut file = File::create(&tmp)?;
        #[cfg(test)]
        if self.injected(ReplaceStep::Write) {
            // A crash mid-write: half the file, and the temp left behind.
            file.write_all(&contents[..contents.len() / 2])?;
            return Err(io::Error::other("injected failure while writing"));
        }
        let handle = (|| -> io::Result<File> {
            file.write_all(contents)?;
            #[cfg(test)]
            if self.injected(ReplaceStep::Sync) {
                return Err(io::Error::other("injected failure while syncing"));
            }
            self.shared.sync(&file, SyncLevel::Full)?;
            let handle = OpenOptions::new().append(true).open(&tmp)?;
            #[cfg(test)]
            if self.injected(ReplaceStep::Rename) {
                return Err(io::Error::other("injected failure while renaming"));
            }
            rename_over(&tmp, &path)?;
            Ok(handle)
        })()
        .inspect_err(|_: &io::Error| {
            let _ = fs::remove_file(&tmp);
        })?;
        #[cfg(test)]
        if self.injected(ReplaceStep::Durable) {
            return Err(io::Error::other(
                "injected failure making the rename durable",
            ));
        }
        sync_dir(self.dir.join(id.to_string()).as_path())?;
        Ok(handle)
    }

    #[cfg(test)]
    fn injected(&self, step: ReplaceStep) -> bool {
        *self
            .shared
            .faults
            .replace_fails_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            == Some(step)
    }

    fn log_path(&self, id: ThreadId) -> PathBuf {
        self.dir.join(id.to_string()).join("log.jsonl")
    }
}

/// One Thread's header facts, read without its history — what a nav row can
/// say about a parked Thread (#21). Everything here is the log's first line.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadMeta {
    pub provider: Provider,
    /// `None` for a log from before schema 3, which never recorded one.
    pub workspace: Option<WorkspaceBinding>,
    /// `None` — including every pre-v4 log — means work in the binding
    /// itself.
    pub session_project_root: Option<PathBuf>,
    /// `None` — including every pre-v5 log — means the provider's default
    /// model.
    pub model: Option<String>,
    /// `None` — including every pre-v6 log — means unregistered (#29): the
    /// binding's resolved paths still say where work happens.
    pub project_id: Option<ProjectId>,
    pub title: Option<String>,
    /// `None` — including every pre-v8 log — means the provider's default
    /// reasoning effort.
    pub effort: Option<String>,
    /// The counts a parked row shows, read off the log's tail; `None` for
    /// a log from before schema 13 (see `Store::summary`).
    pub summary: Option<ThreadSummary>,
}

/// What a revive replays and resumes from (`Store::revive`): the Thread's
/// facts and the records a replay needs — the whole log, or a bounded read
/// of it — plus its writer.
pub struct Revival {
    snapshot: ThreadSnapshot,
    /// Children known from before the replay base: evicted once the replay
    /// is done, their content restored from disk when shown.
    evict: Vec<String>,
    owed: Option<Handover>,
    prompted: bool,
    writer: Option<ThreadWriter>,
}

impl Revival {
    /// The facts and the records read; recall, the resume target and the
    /// handover state read exactly as from a full load.
    pub fn snapshot(&self) -> &ThreadSnapshot {
        &self.snapshot
    }

    /// The switch whose carry never went out, with every exchange before it.
    pub(crate) fn owed_handover(&self) -> Option<&Handover> {
        self.owed.as_ref()
    }

    /// Whether a prompt was ever sent: what locks the provider choice.
    pub fn prompted(&self) -> bool {
        self.prompted
    }

    /// The replay, then the eviction of every child known from before the
    /// replay base — after it, so later content cannot land on a child
    /// already evicted.
    pub fn activity_inputs(&self) -> Vec<crate::activity::ActivityInput> {
        let mut inputs = self.snapshot.activity_inputs();
        inputs.extend(self.evict.iter().map(|key| {
            crate::activity::ActivityInput::Evict(crate::activity::Subject::Subagent(
                crate::activity::AgentKey::from_stored(key.clone()),
            ))
        }));
        inputs
    }

    /// The Thread's writer, for amendments before the revive completes.
    pub fn writer(&mut self) -> &mut ThreadWriter {
        self.writer.as_mut().expect("taken once, last")
    }

    /// Take the writer for the revived Thread.
    pub fn take_writer(&mut self) -> ThreadWriter {
        self.writer.take().expect("taken once")
    }
}

/// One Thread as loaded from disk: everything a restart needs.
pub struct ThreadSnapshot {
    pub id: ThreadId,
    provider: Provider,
    /// The schema the log on disk declares — what tells `writer` an old log
    /// needs upgrading before anything lands after it.
    schema: u32,
    workspace: Option<PersistedBinding>,
    session_project_root: Option<PathBuf>,
    model: Option<String>,
    project_id: Option<ProjectId>,
    title: Option<String>,
    effort: Option<String>,
    records: Vec<Record>,
}

/// How large a log may be for a provider switch to read its exchanges on
/// the caller's thread: a few milliseconds of parsing at most.
const INLINE_EXCHANGE_READ: u64 = 2 * 1024 * 1024;

/// A provider switch's exchanges, being read (`Store::read_exchanges`).
pub(crate) enum ExchangeRead {
    Ready(Option<Result<Vec<(String, String)>, LoadError>>),
    Pending(std::sync::mpsc::Receiver<Result<Vec<(String, String)>, LoadError>>),
}

impl ExchangeRead {
    /// The exchanges once read — taken, so asked once — or `None` while
    /// the read is still running.
    pub(crate) fn poll(&mut self) -> Option<Result<Vec<(String, String)>, LoadError>> {
        match self {
            Self::Ready(read) => read.take(),
            Self::Pending(receive) => match receive.try_recv() {
                Ok(read) => Some(read),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(LoadError::Io(
                    io::Error::other("the read of the switch's exchanges stopped"),
                ))),
            },
        }
    }
}

/// The last provider switch a log records, and what the next prompt on
/// the new provider owes it.
pub(crate) struct Handover {
    /// The provider the conversation before the switch was held with.
    pub from: Provider,
    /// Every prompt before the switch with the answer text that followed
    /// it — the material of the carry digest.
    pub exchanges: Vec<(String, String)>,
    /// Whether a prompt has gone out since the switch: the carry travelled
    /// with it, and a revive must not send it again.
    pub delivered: bool,
}

/// The Main-only answer projection used for Handover. An authoritative item
/// replaces its own accumulated deltas, not the whole answer or another actor.
#[derive(Default)]
struct AnswerText {
    parts: Vec<String>,
    ids: std::collections::HashMap<String, usize>,
    settled: std::collections::HashSet<String>,
    retracted: std::collections::HashSet<String>,
}

impl AnswerText {
    fn observe(&mut self, id: Option<&str>, event: &PersistedExecution) {
        if let PersistedExecution::Retract { ids } = event {
            for id in ids {
                self.retracted.insert(id.clone());
                if let Some(index) = self.ids.get(id) {
                    self.parts[*index].clear();
                }
            }
            return;
        }
        let (text, complete) = match event {
            PersistedExecution::TextDelta { text } => (text, false),
            PersistedExecution::Text { text } | PersistedExecution::TextSnapshot { text } => {
                (text, true)
            }
            _ => return,
        };
        let Some(id) = id else {
            self.parts.push(text.clone());
            return;
        };
        if self.retracted.contains(id) {
            return;
        }
        if matches!(event, PersistedExecution::Text { .. }) && self.settled.contains(id) {
            return;
        }
        let index = *self.ids.entry(id.into()).or_insert_with(|| {
            self.parts.push(String::new());
            self.parts.len() - 1
        });
        if complete {
            self.parts[index] = text.clone();
            if matches!(event, PersistedExecution::Text { .. }) {
                self.settled.insert(id.into());
            }
        } else {
            self.parts[index].push_str(text);
        }
    }
}

impl ThreadSnapshot {
    fn facts(&self) -> Facts {
        Facts {
            provider: self.provider,
            workspace: self.workspace.clone(),
            session_project_root: self.session_project_root.clone(),
            model: self.model.clone(),
            project_id: self.project_id,
            title: self.title.clone(),
            effort: self.effort.clone(),
        }
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    /// The checkout this Thread works in. `None` for a log from before
    /// schema 3, which never recorded one.
    pub fn workspace(&self) -> Option<WorkspaceBinding> {
        self.workspace.as_ref().map(PersistedBinding::live)
    }

    /// The git repo inside the binding where this Thread's work happens.
    /// `None` — including every log from before schema 4 — means work in
    /// the binding itself.
    pub fn session_project_root(&self) -> Option<PathBuf> {
        self.session_project_root.clone()
    }

    /// The model this Thread chose before its first prompt. `None` —
    /// including every log from before schema 5 — means the provider's
    /// default.
    pub fn model(&self) -> Option<String> {
        self.model.clone()
    }

    /// The registered project this Thread's CWD choice named (#29). `None`
    /// — including every log from before schema 6 — means unregistered.
    pub fn project_id(&self) -> Option<ProjectId> {
        self.project_id
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// The reasoning effort this Thread chose. `None` — including every
    /// log from before schema 8 — means the provider's default.
    pub fn effort(&self) -> Option<String> {
        self.effort.clone()
    }

    /// The provider-native id the next Session resumes with — the latest the
    /// provider announced, so a provider that renames its session on resume
    /// still resumes from the newest name. `None` before any Session spoke,
    /// and `None` while a handover is undelivered. An Init can arrive before
    /// the first prompt creates any provider-side conversation to resume.
    pub fn resume_target(&self) -> Option<&str> {
        if self
            .last_handover()
            .is_some_and(|handover| !handover.delivered)
        {
            return None;
        }
        self.records.iter().rev().find_map(|record| match record {
            Record::Init { session_id, .. } => Some(Some(session_id.as_str())),
            Record::ConversationReset { session_id } => Some(Some(session_id.as_str())),
            Record::Handover { .. } => Some(None),
            _ => None,
        })?
    }

    /// The last provider switch in this log, with the conversation before
    /// it as prompt/answer pairs (answer = the assistant text that followed
    /// the prompt, tool runs and reasoning left out). `None` for a Thread
    /// that never switched.
    pub(crate) fn last_handover(&self) -> Option<Handover> {
        let at = self
            .records
            .iter()
            .rposition(|record| matches!(record, Record::Handover { .. }))?;
        if self.records[at + 1..]
            .iter()
            .any(|record| matches!(record, Record::ConversationReset { .. }))
        {
            return None;
        }
        let Record::Handover { from, .. } = &self.records[at] else {
            unreachable!("rposition matched a handover");
        };
        Some(Handover {
            from: *from,
            exchanges: self.exchanges_before(at),
            delivered: self.records[at + 1..]
                .iter()
                .any(|record| matches!(record, Record::Prompt { .. })),
        })
    }

    /// Every prompt since the last reset with the answer that followed it:
    /// what a provider switch made now carries.
    pub(crate) fn exchanges_so_far(&self) -> Vec<(String, String)> {
        self.exchanges_before(self.records.len())
    }

    /// Every prompt between the last reset before record `at` and `at`,
    /// with the answer text that followed it (tool runs and reasoning left
    /// out).
    fn exchanges_before(&self, at: usize) -> Vec<(String, String)> {
        let mut exchanges: Vec<(String, AnswerText)> = Vec::new();
        let after_reset = self.records[..at]
            .iter()
            .rposition(|record| matches!(record, Record::ConversationReset { .. }))
            .map(|index| index + 1)
            .unwrap_or(0);
        for record in &self.records[after_reset..at] {
            match record {
                Record::Prompt { text } => exchanges.push((text.clone(), AnswerText::default())),
                Record::Text { text } => {
                    if let Some((_, answer)) = exchanges.last_mut() {
                        answer.parts.push(text.clone());
                    }
                }
                Record::Activity {
                    observation: PersistedActivity::MainContent { id, event, .. },
                } => {
                    if matches!(event, PersistedExecution::Retract { .. }) {
                        for (_, answer) in &mut exchanges {
                            answer.observe(id.as_deref(), event);
                        }
                    } else if let Some((_, answer)) = exchanges.last_mut() {
                        answer.observe(id.as_deref(), event);
                    }
                }
                _ => {}
            }
        }
        exchanges
            .into_iter()
            .map(|(prompt, answer)| (prompt, answer.parts.concat()))
            .collect()
    }

    /// Legacy Main-only inputs. Child facts and identity-bearing Main content
    /// require `activity_inputs`; flattening them here would duplicate snapshots
    /// or attribute child prompts to the operator.
    pub fn inputs(&self) -> Vec<Input> {
        self.records
            .iter()
            .filter_map(|record| match record {
                Record::Activity {
                    observation: PersistedActivity::BackgroundTurnEnded { outcome, cost_usd },
                } => Some(Input::Event(SessionEvent::TurnEnded {
                    outcome: outcome.live(),
                    cost_usd: *cost_usd,
                })),
                Record::Activity { .. } => None,
                _ => Some(record.input()),
            })
            .collect()
    }

    /// Full chronological replay through Activity's historical Interface.
    /// Request handles were never recorded; replay cannot restore them. Tool
    /// durations follow content and alias restoration, scoped by their subject.
    pub fn activity_inputs(&self) -> Vec<crate::activity::ActivityInput> {
        use crate::activity::{ActivityInput, Subject};
        let mut inputs: Vec<_> = self
            .records
            .iter()
            .map(|record| match record {
                Record::Activity { observation } => ActivityInput::ReplayEvent(observation.live()),
                _ => ActivityInput::Replay(record.input()),
            })
            .collect();
        let mut timings: std::collections::BTreeMap<
            Subject,
            std::collections::HashMap<String, std::time::Duration>,
        > = std::collections::BTreeMap::new();
        let aliases = activity::Aliases::new(&self.records);
        for record in &self.records {
            let timing = match record {
                Record::Activity { observation } => observation.tool_duration(),
                Record::ToolCompleted {
                    id,
                    duration_ms: Some(ms),
                    ..
                } => Some((
                    Subject::Main,
                    id.clone(),
                    std::time::Duration::from_millis(*ms),
                )),
                _ => None,
            };
            if let Some((subject, id, duration)) = timing {
                timings
                    .entry(aliases.subject(subject))
                    .or_default()
                    .insert(id, duration);
            }
        }
        inputs.extend(
            timings
                .into_iter()
                .map(|(subject, timings)| ActivityInput::RestoreTimings { subject, timings }),
        );
        inputs
    }

    /// Every settled tool call's wall clock, keyed by the provider's call
    /// id — what the cockpit clocked when the call actually ran. Replayed
    /// into the cockpit's timings so a revived Thread's rows still carry
    /// the durations they were drawn with; calls logged before the clock
    /// was persisted are simply absent.
    #[cfg(test)]
    pub(crate) fn tool_durations(&self) -> Vec<(String, std::time::Duration)> {
        self.records
            .iter()
            .filter_map(|record| match record {
                Record::ToolCompleted {
                    id,
                    duration_ms: Some(ms),
                    ..
                } => Some((id.clone(), std::time::Duration::from_millis(*ms))),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn prompt_texts(&self) -> Vec<String> {
        let after_reset = self
            .records
            .iter()
            .rposition(|record| matches!(record, Record::ConversationReset { .. }))
            .map(|index| index + 1)
            .unwrap_or(0);
        self.records[after_reset..]
            .iter()
            .filter_map(|record| match record {
                Record::Prompt { text } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }
}

impl ThreadWriter {
    fn lock(&self) -> std::sync::MutexGuard<'_, WriterState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The writer's state, for tests that reach past its interface.
    #[cfg(test)]
    fn state(&self) -> std::sync::MutexGuard<'_, WriterState> {
        self.lock()
    }

    /// Make every later write fail, for a test: the descriptor becomes one
    /// opened for reading.
    #[cfg(test)]
    pub(crate) fn break_writes(&self) {
        let mut state = self.lock();
        state.file = File::open(&state.path).expect("the log opens for reading");
    }

    /// Buffer one Session event, converted to the persisted schema. Flushes
    /// when the event is a boundary (turn end, close). `duration` is the
    /// wall clock a settled tool call took, where the caller measured one.
    pub fn record_event(
        &mut self,
        event: &SessionEvent,
        duration: Option<std::time::Duration>,
    ) -> io::Result<()> {
        self.lock().record_event(event, duration)
    }

    /// Persist the values observed at a live completion.
    pub fn record_completion(
        &mut self,
        subject: &crate::activity::Subject,
        observed: &CompletionFacts,
    ) -> io::Result<()> {
        self.lock().record_completion(subject, observed)
    }

    /// Tell the log how many subagents the Thread's Activity knows, for the
    /// next mark: what a parked row shows without replaying the log.
    pub fn note_subagents(&mut self, count: usize) {
        self.lock().note_subagents(count);
    }

    /// Buffer one line the operator sent.
    pub fn record_prompt(&mut self, text: &str) -> io::Result<()> {
        self.lock().record_prompt(text)
    }

    /// Buffer when the prompt just recorded was sent (`7:31 pm`).
    pub fn record_prompt_observation(&mut self, sent_at: &str) -> io::Result<()> {
        self.lock().record_prompt_observation(sent_at)
    }

    /// Record a provider switch the old way: a handover carrying no facts.
    pub fn record_handover(
        &mut self,
        from: Provider,
        to: Provider,
        model: Option<String>,
    ) -> io::Result<()> {
        self.lock().record_handover(from, to, model)
    }

    /// Everything buffered, written.
    pub fn flush(&mut self) -> io::Result<()> {
        self.lock().flush()
    }

    /// Everything buffered and written, through the drive's own cache.
    pub fn flush_fully(&mut self) -> io::Result<()> {
        self.lock().flush_fully()
    }

    /// Flush all accepted facts and freeze the readable byte boundary for
    /// an asynchronous child-cache reload.
    pub fn checkpoint(&mut self) -> io::Result<u64> {
        self.lock().checkpoint()
    }

    /// Why the worker could not sync this log, until it can: the pump holds
    /// the Session's events while this says anything, so nothing more is
    /// accepted than the disk can keep.
    pub fn failure(&self) -> Option<String> {
        self.lock().failure.clone()
    }

    /// The Thread is parked: hand the writer to the worker, which removes
    /// the open marker, ends the log in a mark and syncs it through the
    /// drive's cache. Returns at once; everything buffered must already be
    /// written (`flush`). A sync that fails is retried, and a revive waits
    /// for it (`Store::settle`).
    pub fn park(self) {
        self.release(true);
    }

    /// Done with the log, its Thread still parked: the worker ends it in a
    /// mark and syncs it through the drive's cache. Returns at once.
    pub fn close(self) {
        self.release(false);
    }

    fn release(self, parked: bool) {
        let worker = {
            let mut state = self.lock();
            state.park(parked);
            state.shared.worker()
        };
        worker.hold(self.state.clone());
    }
}

impl Drop for ThreadWriter {
    /// A writer let go of with writes not yet synced: the worker finishes
    /// them.
    fn drop(&mut self) {
        if std::sync::Arc::strong_count(&self.state) > 1 {
            return;
        }
        let worker = {
            let state = self.lock();
            if state.synced >= state.len || state.parking {
                return;
            }
            state.shared.worker()
        };
        worker.hold(self.state.clone());
    }
}

/// What a live turn's end observed: its elapsed, the stamp's clock, and its
/// token counts (`None` where the provider reported none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionFacts {
    pub elapsed_ms: u64,
    pub completed_at: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// Appends one Thread's records. Buffered: nothing reaches the disk until a
/// boundary (turn end, close), a timeout, or an explicit `flush`. A handle:
/// the store's worker shares what it appends to, to sync it (ADR 0009).
pub struct ThreadWriter {
    state: std::sync::Arc<std::sync::Mutex<WriterState>>,
}

/// One Thread's open log and everything buffered for it.
struct WriterState {
    file: File,
    buffer: Vec<Record>,
    flush_interval: std::time::Duration,
    /// When the oldest unflushed record was buffered; `None` while empty.
    buffered_since: Option<std::time::Instant>,
    /// An unsuccessful append retains both its records and exact byte offset.
    /// A retry writes only bytes not accepted yet and retries a failed sync.
    pending_flush: Option<PendingFlush>,
    /// The longest written records wait for a full sync.
    full_sync_interval: std::time::Duration,
    /// When the log last had one.
    fully_synced: std::time::Instant,
    path: PathBuf,
    /// The Thread's facts as this log now has them: what every amendment
    /// changes and every mark restates.
    facts: Facts,
    /// The log's length, as far as this writer has written it.
    len: u64,
    /// Where the newest mark starts, and how long it is.
    last_mark: Option<u64>,
    mark_len: usize,
    /// Why this writer may no longer append: a failed amendment could not
    /// be taken back off the log. Reopening the Thread repairs it.
    broken: Option<String>,
    /// The parked row's counts as of every record accepted; `None` while
    /// unknown (a log whose last mark predates them).
    summary: Option<Summary>,
    mark_spacing: u64,
    /// See `Tail`.
    tracker: Option<Tracker>,
    inherited: Option<Base>,
    /// How much of the log the worker has synced behind a barrier, and
    /// through the drive's cache.
    synced: u64,
    full_synced: u64,
    /// Why the worker's last sync failed, until one succeeds.
    failure: Option<String>,
    /// Parked: the worker marks and fully syncs the log, then lets it go.
    parking: bool,
    park_marked: bool,
    /// The open marker a park removes, and the store's open-state marker.
    unmark: Option<(PathBuf, PathBuf)>,
    shared: std::sync::Arc<Shared>,
}

/// How a log ends, for the writer that appends to it.
struct Tail {
    facts: Facts,
    len: u64,
    last_mark: Option<u64>,
    mark_len: usize,
    summary: Option<Summary>,
    /// Following the log record by record, when the writer has read (or
    /// written) all of it that a base needs; `None` after a tail-only
    /// reopen, whose marks keep naming the newest base it found.
    tracker: Option<Tracker>,
    /// The newest base a mark named.
    base: Option<Base>,
}

struct PendingFlush {
    bytes: Vec<u8>,
    written: usize,
    records: usize,
}

/// The private durability seam lets failures exercise the same retry path as
/// a file, including partial writes followed by errors and failed syncs.
trait DurableWrite: Write {
    fn sync(&self, level: SyncLevel) -> io::Result<()>;
}

impl DurableWrite for File {
    fn sync(&self, level: SyncLevel) -> io::Result<()> {
        sync_file(self, level)
    }
}

fn flush_records(
    file: &mut impl DurableWrite,
    buffer: &mut Vec<Record>,
    pending: &mut Option<PendingFlush>,
    level: Option<SyncLevel>,
    encoded: &mut dyn FnMut(&Record, usize),
) -> io::Result<()> {
    while !buffer.is_empty() {
        if pending.is_none() {
            let mut bytes = Vec::new();
            for record in buffer.iter() {
                let line = line(record)?;
                encoded(record, line.len());
                bytes.extend_from_slice(line.as_bytes());
            }
            *pending = Some(PendingFlush {
                bytes,
                written: 0,
                records: buffer.len(),
            });
        }
        let append = pending.as_mut().expect("prepared above");
        while append.written < append.bytes.len() {
            match file.write(&append.bytes[append.written..]) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero)),
                Ok(written) => append.written += written,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        if let Some(level) = level {
            file.sync(level)?;
        }
        buffer.drain(..append.records);
        *pending = None;
    }
    Ok(())
}

impl WriterState {
    /// Buffer one Session event, converted to the persisted schema. Flushes
    /// internally when the event is a boundary (turn end, close).
    /// `duration` is the wall clock a settled tool call took, where the
    /// caller measured one; `None` everywhere else.
    pub fn record_event(
        &mut self,
        event: &SessionEvent,
        duration: Option<std::time::Duration>,
    ) -> io::Result<()> {
        let Some(record) = Record::from_event(event, duration) else {
            return Ok(());
        };
        self.push(record)
    }

    /// Persist the values observed at a live completion. These are Ferrite
    /// facts, separate from the provider event that ended the turn.
    pub fn record_completion(
        &mut self,
        subject: &crate::activity::Subject,
        observed: &CompletionFacts,
    ) -> io::Result<()> {
        let record = match subject {
            crate::activity::Subject::Main => Record::CompletionObservation {
                elapsed_ms: observed.elapsed_ms,
                completed_at: observed.completed_at.clone(),
                input_tokens: observed.input_tokens,
                output_tokens: observed.output_tokens,
            },
            crate::activity::Subject::Subagent(_) => Record::Activity {
                observation: PersistedActivity::from_live(
                    &crate::activity::ActivityEvent::CompletionObservation {
                        subject: subject.clone(),
                        elapsed_ms: observed.elapsed_ms,
                        completed_at: observed.completed_at.clone(),
                        input_tokens: observed.input_tokens,
                        output_tokens: observed.output_tokens,
                    },
                    None,
                )
                .expect("completion observation is durable"),
            },
        };
        self.push(record)
    }

    /// Tell the log how many subagents the Thread's Activity knows, for the
    /// next mark: what a parked row shows without replaying the log.
    pub fn note_subagents(&mut self, count: usize) {
        if let Some(summary) = self.summary.as_mut() {
            summary.subagents = Some(count as u64);
        }
    }

    /// Buffer one line the operator sent. Not a Session event: the prompt is
    /// Ferrite's own act, and no provider will ever echo it back.
    pub fn record_prompt(&mut self, text: &str) -> io::Result<()> {
        self.push(Record::Prompt { text: text.into() })
    }

    /// Buffer when the prompt just recorded was sent (`7:31 pm`), so replay
    /// draws the time it was sent rather than none.
    pub fn record_prompt_observation(&mut self, sent_at: &str) -> io::Result<()> {
        self.push(Record::PromptObservation {
            sent_at: sent_at.into(),
        })
    }

    /// Record a provider switch after the first prompt: a boundary, so it
    /// is on disk before the header rewrite that follows reads the log.
    pub fn record_handover(
        &mut self,
        from: Provider,
        to: Provider,
        model: Option<String>,
    ) -> io::Result<()> {
        self.push(Record::Handover {
            from,
            to,
            model,
            facts: None,
        })
    }

    /// Everything buffered, written: in the page cache, where every reader
    /// sees it and a crash of Ferrite cannot lose it. The worker syncs it,
    /// behind a barrier at once and through the drive's cache within
    /// `FULL_SYNC_INTERVAL` — never this caller (ADR 0009).
    pub fn flush(&mut self) -> io::Result<()> {
        self.usable()?;
        let wrote = !self.buffer.is_empty();
        if wrote && self.synced == self.len && self.mark_due() {
            // Everything before is synced already: a mark may vouch for it
            // here, ahead of what this flush adds.
            self.mark()?;
        }
        // Each record as it is encoded, where it will start in the log.
        let mut at = self.len
            + self
                .pending_flush
                .as_ref()
                .map_or(0, |append| append.bytes.len() as u64);
        let tracker = &mut self.tracker;
        flush_records(
            &mut self.file,
            &mut self.buffer,
            &mut self.pending_flush,
            None,
            &mut |record, len| {
                if let Some(tracker) = tracker.as_mut() {
                    tracker.observe(at, record);
                }
                at += len as u64;
            },
        )?;
        self.buffered_since = None;
        if wrote {
            self.len = self.file.metadata()?.len();
            self.shared.worker().poke();
        }
        Ok(())
    }

    /// Everything buffered, written and synced through the drive's own
    /// cache, here and now, ending in a mark: what an import or a test waits
    /// for. A park hands its writer to the worker instead (`park`).
    pub fn flush_fully(&mut self) -> io::Result<()> {
        self.flush()?;
        if self.synced < self.len {
            // A mark vouches for what precedes it: that first.
            self.shared.sync(&self.file, SyncLevel::Barrier)?;
            self.synced = self.len;
        }
        if self.since_mark() > 0 {
            self.mark()?;
        }
        if self.full_synced < self.len {
            self.shared.sync(&self.file, SyncLevel::Full)?;
            self.synced = self.len;
            self.full_synced = self.len;
            self.fully_synced = std::time::Instant::now();
        }
        Ok(())
    }

    /// Whether a mark is due: the bytes since the last one.
    fn mark_due(&self) -> bool {
        self.since_mark() >= self.mark_spacing.max(16 * self.mark_len as u64)
    }

    /// Bytes written since the newest mark (or the header).
    fn since_mark(&self) -> u64 {
        let marked = self
            .last_mark
            .map_or(0, |at| at + self.mark_len as u64);
        self.len.saturating_sub(marked)
    }

    /// Append a mark restating the facts as they are now.
    fn mark(&mut self) -> io::Result<()> {
        // A fresh base when one is far enough back; else the newest one a
        // mark named, still valid, only further back.
        if let Some(base) = self.tracker.as_mut().and_then(Tracker::base) {
            self.inherited = Some(base);
        }
        let mark = line(&Record::Mark {
            facts: self.facts.clone(),
            prev: self.last_mark,
            summary: self.summary.clone(),
            base: self.inherited.clone(),
        })?;
        let at = self.len;
        self.append_now(mark.as_bytes())?;
        self.last_mark = Some(at);
        self.mark_len = mark.len();
        #[cfg(test)]
        self.shared.journal("mark", at);
        Ok(())
    }

    /// Change the Thread's facts by `change` and append the result: an
    /// amendment (`Store::amend`).
    fn amend(&mut self, change: impl FnOnce(&mut Facts)) -> io::Result<()> {
        let mut facts = self.facts.clone();
        change(&mut facts);
        if facts == self.facts {
            return Ok(());
        }
        self.commit(Record::Facts { facts })
    }

    /// Append one record now, outside the buffer: an amendment or a handover
    /// its caller waits on. Everything buffered goes first.
    /// On failure the line is taken back off the log, so a change reported
    /// as failed never lands later; if even that fails, the writer refuses
    /// every append until the Thread is reopened (and repaired).
    fn commit(&mut self, record: Record) -> io::Result<()> {
        self.flush()?;
        let written = line(&record)?;
        let at = self.len;
        self.append_now(written.as_bytes())?;
        // Written; the worker syncs it like any other line.
        self.shared.worker().poke();
        if let Some(tracker) = self.tracker.as_mut() {
            tracker.observe(at, &record);
        }
        if let Some(facts) = record.facts() {
            self.facts = facts.clone();
        }
        Ok(())
    }

    /// Write `bytes` at the end of the log now, or not at all.
    fn append_now(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.usable()?;
        let before = self.len;
        let written = (|| {
            self.file.write_all(bytes)?;
            #[cfg(test)]
            if self
                .shared
                .faults
                .fail_commit
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                return Err(io::Error::other("injected failure after the write"));
            }
            Ok(())
        })();
        if let Err(error) = written {
            let rolled_back = (|| {
                #[cfg(test)]
                if self
                    .shared
                    .faults
                    .fail_rollback
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    return Err(io::Error::other("injected failure rolling back"));
                }
                // Not the append handle: Windows truncates only through a
                // handle opened for writing.
                let log = OpenOptions::new().write(true).open(&self.path)?;
                log.set_len(before)?;
                sync_file(&log, SyncLevel::Barrier)
            })();
            if let Err(rollback) = rolled_back {
                self.broken = Some(format!(
                    "{error}; taking it back off the log failed too: {rollback}"
                ));
            }
            return Err(error);
        }
        self.len = before + bytes.len() as u64;
        Ok(())
    }

    /// Refuse to append while broken (`commit`).
    fn usable(&self) -> io::Result<()> {
        match &self.broken {
            Some(why) => Err(io::Error::other(format!(
                "history is closed until the Thread is reopened: {why}"
            ))),
            None => Ok(()),
        }
    }

    /// Flush all accepted facts and freeze the readable byte boundary for
    /// an asynchronous child-cache reload. Failure retains the same retry
    /// contract as `flush`; the caller must not resubmit buffered events.
    pub fn checkpoint(&mut self) -> io::Result<u64> {
        self.flush()?;
        Ok(self.file.metadata()?.len())
    }

    fn push(&mut self, record: Record) -> io::Result<()> {
        if let Some(summary) = self.summary.as_mut() {
            summary.observe(&record);
        }
        // Records already encoded for a retry are immutable until committed.
        let frozen = self
            .pending_flush
            .as_ref()
            .map_or(0, |append| append.records);
        let coalesced = self.buffer.len() > frozen
            && self
                .buffer
                .last_mut()
                .is_some_and(|last| last.coalesce(&record));
        let flush_now = record.is_boundary();
        if !coalesced {
            self.buffer.push(record);
        }
        let since = *self
            .buffered_since
            .get_or_insert_with(std::time::Instant::now);
        if flush_now || since.elapsed() >= self.flush_interval {
            self.flush()?;
        }
        Ok(())
    }
}

/// A log read as far as it is readable.
struct Parsed {
    snapshot: ThreadSnapshot,
    /// Where each of `snapshot.records` starts in the log.
    offsets: Vec<u64>,
    /// Where the readable log ends: after the last record that parsed, and
    /// its newline when it has one. Anything past it is a crash's fragment
    /// or damage.
    readable: usize,
}

/// Read a log's bytes: the header, then every record up to the first line
/// that does not parse — a crash tears at most the final line, so the
/// first unreadable line is where the log ends.
fn parse(id: ThreadId, bytes: &[u8]) -> Result<Parsed, LoadError> {
    let line_at = |start: usize| {
        let end = bytes[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |at| start + at);
        (&bytes[start..end], (end + 1).min(bytes.len()))
    };
    let (first, mut readable) = line_at(0);
    let header: Header = serde_json::from_slice(first).map_err(|_| LoadError::Corrupt {
        detail: format!("thread {id} has no readable header"),
    })?;
    if header.schema > SCHEMA_VERSION {
        return Err(LoadError::FutureSchema {
            found: header.schema,
            supported: SCHEMA_VERSION,
        });
    }
    let mut facts = header.facts();
    let mut records = Vec::new();
    let mut offsets = Vec::new();
    while readable < bytes.len() {
        let (body_line, next) = line_at(readable);
        let Ok(record) = serde_json::from_slice::<Record>(body_line) else {
            break;
        };
        if let Some(restated) = record.facts() {
            facts = restated.clone();
        }
        if !record.is_bookkeeping() {
            offsets.push(readable as u64);
            records.push(record);
        }
        readable = next;
    }
    Ok(Parsed {
        snapshot: ThreadSnapshot {
            id,
            provider: facts.provider,
            schema: header.schema,
            workspace: facts.workspace,
            session_project_root: facts.session_project_root,
            model: facts.model,
            project_id: facts.project_id,
            title: facts.title,
            effort: facts.effort,
            records,
        },
        offsets,
        readable,
    })
}

/// One record as one JSONL line.
fn line<T: Serialize>(record: &T) -> io::Result<String> {
    let mut text = serde_json::to_string(record).map_err(io::Error::other)?;
    text.push('\n');
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Transcript;
    use crate::{ToolResult, TurnOutcome};

    /// A fresh per-test scratch directory.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrite-store-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// The binding for tests that are not about bindings.
    fn main_choice() -> WorkspaceBinding {
        WorkspaceBinding::Main {
            checkout: std::env::temp_dir(),
        }
    }

    /// A Thread's workspace binding is part of what a restart restores:
    /// both shapes round-trip exactly as the caller resolved them — since
    /// #29 the registry places worktrees, and the store only writes the
    /// resolved truth down. The project id rides the same header.
    #[test]
    fn a_thread_s_workspace_binding_survives_reopening_the_store() {
        let dir = scratch("binding");
        let store = Store::open(&dir).unwrap();
        let main_binding = WorkspaceBinding::Main {
            checkout: "/repos/project".into(),
        };
        let (main_id, _writer) = store
            .create(Provider::Claude, None, main_binding.clone())
            .unwrap();
        let wt_binding = WorkspaceBinding::Worktree {
            repo: "/repos/project".into(),
            path: "/store/worktrees/project-4f2a1c9e7b30/ferrite-wt-1".into(),
        };
        let project = registry_id(&dir, "project-a");
        let (wt_id, _writer) = store
            .create(Provider::Codex, Some(project), wt_binding.clone())
            .unwrap();

        // The fake restart: nothing survives but the directory.
        let store = Store::open(&dir).unwrap();
        assert_eq!(store.load(main_id).unwrap().workspace(), Some(main_binding));
        assert_eq!(store.load(main_id).unwrap().project_id(), None);
        assert_eq!(store.load(wt_id).unwrap().workspace(), Some(wt_binding));
        assert_eq!(store.load(wt_id).unwrap().project_id(), Some(project));
        assert_eq!(store.peek(wt_id).unwrap().project_id, Some(project));
    }

    /// A real ProjectId for header tests — minted by the registry, which is
    /// the only place one comes from.
    fn registry_id(dir: &Path, name: &str) -> ProjectId {
        let root = dir.join(name);
        fs::create_dir_all(&root).unwrap();
        crate::workspace::registry::Registry::open(dir)
            .unwrap()
            .register(&root)
            .unwrap()
    }

    /// Deletion is per-Thread and total: the log directory goes, the other
    /// Threads stay.
    #[test]
    fn a_deleted_thread_is_gone_and_its_neighbours_are_not() {
        let dir = scratch("delete");
        let store = Store::open(&dir).unwrap();
        let (first, _writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        let (second, _writer) = store.create(Provider::Codex, None, main_choice()).unwrap();

        store.delete(first).unwrap();

        assert_eq!(store.thread_ids().unwrap(), vec![second]);
        assert!(store.load(first).is_err(), "a deleted Thread must not load");
        assert!(store.load(second).is_ok());
    }

    /// #21: the nav's parked rows come from `peek`, which must answer from
    /// the header line alone. The body here is a megabyte of bytes that are
    /// not records at all — a peek that read or parsed past the first line
    /// would choke on them, and a `load` in a render path is exactly the
    /// whole-log replay the nav exists to avoid.
    #[test]
    fn peek_reads_the_header_line_and_never_the_records() {
        let dir = scratch("peek");
        let store = Store::open(&dir).unwrap();
        let (id, writer) = store
            .create(
                Provider::Codex,
                None,
                WorkspaceBinding::Main {
                    checkout: "/repos/project".into(),
                },
            )
            .unwrap();
        drop(writer);
        store
            .set_session_project_root(id, Some("/repos/project/api".into()), None)
            .unwrap();
        let mut log = OpenOptions::new()
            .append(true)
            .open(dir.join(id.to_string()).join("log.jsonl"))
            .unwrap();
        writeln!(log, "{}", "x".repeat(1024 * 1024)).unwrap();

        let meta = store.peek(id).unwrap();

        assert_eq!(meta.provider, Provider::Codex);
        assert_eq!(
            meta.workspace,
            Some(WorkspaceBinding::Main {
                checkout: "/repos/project".into(),
            })
        );
        assert_eq!(
            meta.session_project_root,
            Some(PathBuf::from("/repos/project/api"))
        );
    }

    /// A log from Ferrite's future refuses a peek exactly as it refuses a
    /// load: a nav row half-read from an unknown schema would claim a
    /// Thread that cannot actually be revived.
    #[test]
    fn peek_refuses_a_future_schema_like_load_does() {
        let dir = scratch("peek-future");
        let store = Store::open(&dir).unwrap();
        let (id, writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        drop(writer);
        let path = dir.join(id.to_string()).join("log.jsonl");
        let log = fs::read_to_string(&path).unwrap();
        fs::write(
            &path,
            log.replace(&format!("\"schema\":{SCHEMA_VERSION}"), "\"schema\":99"),
        )
        .unwrap();

        assert!(matches!(
            store.peek(id),
            Err(LoadError::FutureSchema { found: 99, .. })
        ));
    }

    /// The frozen contract for schema 2, byte for byte what its writer
    /// produced: prompts and structured results, but no workspace binding.
    /// Logs like this exist on disks; they must load forever.
    const V2_LOG: &str = concat!(
        r#"{"schema":2,"provider":"claude"}"#,
        "\n",
        r#"{"type":"init","session_id":"v2-era-4f2a","model":"claude-haiku-4-5"}"#,
        "\n",
        r#"{"type":"prompt","text":"fix the typo"}"#,
        "\n",
        r#"{"type":"text","text":"done"}"#,
        "\n",
        r#"{"type":"turn_ended","outcome":"completed","cost_usd":null}"#,
        "\n",
    );

    /// AC (schema story): loading a log written at schema v2 succeeds after
    /// the bump to v3 — with no binding, exactly what v2 recorded.
    #[test]
    fn a_log_written_at_schema_v2_still_loads_after_the_bump() {
        let dir = scratch("v2");
        plant_log(&dir, "9", V2_LOG);

        let thread = Store::open(&dir).unwrap().load(ThreadId::new(9)).unwrap();
        assert_eq!(thread.provider(), Provider::Claude);
        assert_eq!(thread.workspace(), None);
        assert_eq!(thread.resume_target(), Some("v2-era-4f2a"));
        assert_eq!(
            thread.inputs(),
            vec![
                Input::Event(SessionEvent::Init {
                    session_id: "v2-era-4f2a".into(),
                    model: "claude-haiku-4-5".into(),
                }),
                Input::Prompt("fix the typo".into()),
                Input::Event(SessionEvent::TextDelta {
                    text: "done".into(),
                }),
                Input::Event(SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: None,
                }),
            ]
        );
    }

    /// The frozen contract for schema 3, byte for byte what its writer
    /// produced: the workspace binding in the header, but no session project
    /// root. Logs like this exist on disks; they must load forever.
    const V3_LOG: &str = concat!(
        r#"{"schema":3,"provider":"claude","workspace":{"kind":"main","checkout":"/repos/project"}}"#,
        "\n",
        r#"{"type":"init","session_id":"v3-era-4f2a","model":"claude-haiku-4-5"}"#,
        "\n",
        r#"{"type":"turn_ended","outcome":"completed","cost_usd":null}"#,
        "\n",
    );

    /// AC (schema story): loading a log written at schema v3 succeeds after
    /// the bump to v4 — binding intact, and no session project root, exactly
    /// what v3 recorded: work happens in the binding itself.
    #[test]
    fn a_log_written_at_schema_v3_still_loads_after_the_bump() {
        let dir = scratch("v3");
        plant_log(&dir, "11", V3_LOG);

        let thread = Store::open(&dir).unwrap().load(ThreadId::new(11)).unwrap();
        assert_eq!(thread.provider(), Provider::Claude);
        assert_eq!(
            thread.workspace(),
            Some(WorkspaceBinding::Main {
                checkout: "/repos/project".into(),
            })
        );
        assert_eq!(thread.session_project_root(), None);
        assert_eq!(thread.resume_target(), Some("v3-era-4f2a"));
    }

    /// AC (#24): the session project root survives restart. Setting it
    /// rewrites the header; the Thread's open writer is passed through and
    /// comes back on the new log — the rename leaves the old handle on the
    /// replaced inode, where appends would vanish. Clearing the root hands
    /// back None, today's work-in-the-binding behavior.
    #[test]
    fn a_thread_s_session_project_root_survives_reopening_the_store() {
        let dir = scratch("session-root");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        assert_eq!(store.load(id).unwrap().session_project_root(), None);

        store
            .set_session_project_root(
                id,
                Some("/repos/project/apps/web".into()),
                Some(&mut writer),
            )
            .unwrap();
        // The writer rode the rewrite: this append must land where loads
        // look, not on the renamed-over inode.
        writer.record_prompt("after the pick").unwrap();
        writer.flush().unwrap();

        // The fake restart: nothing survives but the directory.
        let reopened = Store::open(&dir).unwrap();
        let thread = reopened.load(id).unwrap();
        assert_eq!(
            thread.session_project_root(),
            Some("/repos/project/apps/web".into())
        );
        assert!(
            thread
                .inputs()
                .contains(&Input::Prompt("after the pick".into())),
            "the swapped writer's append is history: {:?}",
            thread.inputs()
        );
        // The header declares the schema that wrote it.
        let log = fs::read_to_string(dir.join(id.to_string()).join("log.jsonl")).unwrap();
        assert!(
            log.lines()
                .next()
                .unwrap()
                .contains(&format!("\"schema\":{SCHEMA_VERSION}")),
            "header: {log}"
        );

        // Cleared — no writer open this time — the Thread works in the
        // binding again.
        reopened.set_session_project_root(id, None, None).unwrap();
        assert_eq!(reopened.load(id).unwrap().session_project_root(), None);
    }

    /// The frozen contract for schema 4, byte for byte what its writer
    /// produced: the session project root in the header, but no model.
    /// Logs like this exist on disks; they must load forever.
    const V4_LOG: &str = concat!(
        r#"{"schema":4,"provider":"claude","workspace":{"kind":"main","checkout":"/repos/project"},"session_project_root":"/repos/project/api"}"#,
        "\n",
        r#"{"type":"init","session_id":"v4-era-4f2a","model":"claude-haiku-4-5"}"#,
        "\n",
        r#"{"type":"turn_ended","outcome":"completed","cost_usd":null}"#,
        "\n",
    );

    /// AC (schema story): loading a log written at schema v4 succeeds after
    /// the bump to v5 — root intact, and no model, exactly what v4
    /// recorded: the provider's default.
    #[test]
    fn a_log_written_at_schema_v4_still_loads_after_the_bump() {
        let dir = scratch("v4");
        plant_log(&dir, "13", V4_LOG);

        let thread = Store::open(&dir).unwrap().load(ThreadId::new(13)).unwrap();
        assert_eq!(thread.provider(), Provider::Claude);
        assert_eq!(
            thread.session_project_root(),
            Some("/repos/project/api".into())
        );
        assert_eq!(thread.model(), None);
        assert_eq!(thread.resume_target(), Some("v4-era-4f2a"));
    }

    /// The frozen contract for schema 5, byte for byte what its writer
    /// produced: the chosen model in the header, but no project id. Logs
    /// like this exist on disks; they must load forever.
    const V5_LOG: &str = concat!(
        r#"{"schema":5,"provider":"codex","workspace":{"kind":"worktree","repo":"/repos/project","path":"/store/7/worktree"},"session_project_root":null,"model":"gpt-5.4-mini"}"#,
        "\n",
        r#"{"type":"init","session_id":"v5-era-4f2a","model":"gpt-5.4-mini"}"#,
        "\n",
        r#"{"type":"turn_ended","outcome":"completed","cost_usd":null}"#,
        "\n",
    );

    /// AC (schema story): loading a log written at schema v5 succeeds after
    /// the bump to v6 — binding and model intact, and no project id,
    /// exactly what v5 recorded: those Threads registered nothing.
    #[test]
    fn a_log_written_at_schema_v5_still_loads_after_the_bump() {
        let dir = scratch("v5");
        plant_log(&dir, "17", V5_LOG);

        let thread = Store::open(&dir).unwrap().load(ThreadId::new(17)).unwrap();
        assert_eq!(thread.provider(), Provider::Codex);
        assert_eq!(thread.model(), Some("gpt-5.4-mini".into()));
        assert_eq!(thread.project_id(), None);
        assert_eq!(
            thread.workspace(),
            Some(WorkspaceBinding::Worktree {
                repo: "/repos/project".into(),
                path: "/store/7/worktree".into(),
            })
        );
        assert_eq!(thread.resume_target(), Some("v5-era-4f2a"));
    }

    /// The frozen contract for schema 7, byte for byte what its writer
    /// produced: the title in the header, but no effort and no handovers.
    const V7_LOG: &str = concat!(
        r#"{"schema":7,"provider":"claude","workspace":{"kind":"main","checkout":"/repos/project"},"session_project_root":null,"model":"sonnet","project_id":null,"title":"Old title"}"#,
        "\n",
        r#"{"type":"init","session_id":"v7-era-4f2a","model":"claude-sonnet-5"}"#,
        "\n",
        r#"{"type":"prompt","text":"hello"}"#,
        "\n",
        r#"{"type":"turn_ended","outcome":"completed","cost_usd":null}"#,
        "\n",
    );

    /// AC (schema story): loading a log written at schema v7 succeeds after
    /// the bump to v8 — title and model intact, no effort (the provider's
    /// default) and no handover, exactly what v7 recorded.
    #[test]
    fn a_log_written_at_schema_v7_still_loads_after_the_bump() {
        let dir = scratch("v7");
        plant_log(&dir, "19", V7_LOG);

        let store = Store::open(&dir).unwrap();
        let thread = store.load(ThreadId::new(19)).unwrap();
        assert_eq!(thread.provider(), Provider::Claude);
        assert_eq!(thread.model(), Some("sonnet".into()));
        assert_eq!(thread.title(), Some("Old title"));
        assert_eq!(thread.effort(), None);
        assert!(thread.last_handover().is_none());
        assert_eq!(thread.resume_target(), Some("v7-era-4f2a"));
        assert_eq!(store.peek(ThreadId::new(19)).unwrap().effort, None);
    }

    /// AC (#25): the provider and model choice survives restart. Setting it
    /// rewrites the header; the Thread's open writer rides the rewrite the
    /// same way the root setter's does. Clearing the model hands back None,
    /// the provider's default.
    #[test]
    fn a_thread_s_provider_and_model_survive_reopening_the_store() {
        let dir = scratch("provider-model");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        assert_eq!(store.load(id).unwrap().model(), None);

        store
            .set_provider(
                id,
                Provider::Codex,
                Some("gpt-5.4-mini".into()),
                Some("high".into()),
                Some(&mut writer),
            )
            .unwrap();
        // The writer rode the rewrite: this append must land where loads
        // look, not on the renamed-over inode.
        writer.record_prompt("after the pick").unwrap();
        writer.flush().unwrap();

        // The fake restart: nothing survives but the directory.
        let reopened = Store::open(&dir).unwrap();
        let thread = reopened.load(id).unwrap();
        assert_eq!(thread.provider(), Provider::Codex);
        assert_eq!(thread.model(), Some("gpt-5.4-mini".into()));
        assert_eq!(thread.effort(), Some("high".into()));
        assert!(
            thread
                .inputs()
                .contains(&Input::Prompt("after the pick".into())),
            "the swapped writer's append is history: {:?}",
            thread.inputs()
        );
        let meta = reopened.peek(id).unwrap();
        assert_eq!(meta.provider, Provider::Codex);
        assert_eq!(meta.model, Some("gpt-5.4-mini".into()));
        assert_eq!(meta.effort, Some("high".into()));

        // Back on the provider's defaults — no writer open this time.
        reopened
            .set_provider(id, Provider::Codex, None, None, None)
            .unwrap();
        assert_eq!(reopened.load(id).unwrap().model(), None);
        assert_eq!(reopened.load(id).unwrap().effort(), None);
        assert_eq!(reopened.peek(id).unwrap().effort, None);
    }

    /// A handover is durable history: it replays as its Notice, it hides
    /// the old provider's Init from the resume target until the new one
    /// speaks, and it carries the conversation before it as exchanges —
    /// marked delivered once a prompt follows it.
    #[test]
    fn contract_late_retraction_is_excluded_from_all_handover_exchanges() {
        let dir = scratch("late-retraction-carry");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        writer.record_prompt("first").unwrap();
        writer
            .record_event(
                &SessionEvent::Activity(crate::activity::ActivityEvent::MainContent {
                    id: Some("old".into()),
                    event: crate::activity::ExecutionEvent::Text {
                        text: "Must not carry".into(),
                    },
                }),
                None,
            )
            .unwrap();
        writer.record_prompt("second").unwrap();
        writer
            .record_event(
                &SessionEvent::Activity(crate::activity::ActivityEvent::MainContent {
                    id: None,
                    event: crate::activity::ExecutionEvent::Retract {
                        ids: vec!["old".into()],
                    },
                }),
                None,
            )
            .unwrap();
        writer
            .record_handover(Provider::Claude, Provider::Codex, None)
            .unwrap();
        writer.flush().unwrap();
        let handover = store.load(id).unwrap().last_handover().unwrap();
        assert_eq!(
            handover.exchanges,
            [
                ("first".into(), String::new()),
                ("second".into(), String::new())
            ]
        );
    }

    #[test]
    fn a_handover_is_replayed_and_shadows_the_old_providers_init() {
        let dir = scratch("handover");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        writer
            .record_event(
                &SessionEvent::Init {
                    session_id: "claude-sess".into(),
                    model: "claude-opus-5".into(),
                },
                None,
            )
            .unwrap();
        writer.record_prompt("first question").unwrap();
        writer
            .record_event(
                &SessionEvent::TextDelta {
                    text: "first ".into(),
                },
                None,
            )
            .unwrap();
        writer
            .record_event(
                &SessionEvent::TextDelta {
                    text: "answer".into(),
                },
                None,
            )
            .unwrap();
        writer
            .record_event(
                &SessionEvent::ToolCompleted {
                    id: "t1".into(),
                    output: "tool noise".into(),
                    is_error: false,
                    result: crate::ToolResult::Opaque,
                },
                None,
            )
            .unwrap();
        writer.record_prompt("second question").unwrap();
        writer
            .record_handover(
                Provider::Claude,
                Provider::Codex,
                Some("gpt-5.6-sol".into()),
            )
            .unwrap();
        assert!(!dir.join("1").join("log.jsonl.tmp").exists());
        assert_eq!(
            store.load(id).unwrap().resume_target(),
            None,
            "a boundary: on disk already"
        );
        store
            .set_provider(
                id,
                Provider::Codex,
                Some("gpt-5.6-sol".into()),
                None,
                Some(&mut writer),
            )
            .unwrap();

        let thread = store.load(id).unwrap();
        assert_eq!(thread.provider(), Provider::Codex);
        assert_eq!(
            thread.resume_target(),
            None,
            "the Init before the switch belongs to Claude"
        );
        let handover = thread.last_handover().expect("recorded");
        assert_eq!(handover.from, Provider::Claude);
        assert_eq!(
            handover.exchanges,
            [
                ("first question".to_string(), "first answer".to_string()),
                ("second question".to_string(), String::new()),
            ]
        );
        assert!(!handover.delivered);
        assert!(thread.inputs().contains(&Input::Notice(
            "continued on Codex · GPT-5.6 Sol — the earlier conversation is handed over as context"
                .into()
        )));

        // Init alone does not mean the new provider has saved a conversation.
        // Recover logs stranded by the old effort-change restart too.
        writer
            .record_event(
                &SessionEvent::Init {
                    session_id: "empty-codex-thread".into(),
                    model: "gpt-5.6-sol".into(),
                },
                None,
            )
            .unwrap();
        writer
            .record_event(
                &SessionEvent::Closed {
                    reason: "no rollout found for thread id empty-codex-thread".into(),
                },
                None,
            )
            .unwrap();
        writer.flush().unwrap();
        let thread = Store::open(&dir).unwrap().load(id).unwrap();
        assert_eq!(
            thread.resume_target(),
            None,
            "the new provider has received no prompt"
        );
        assert!(!thread.last_handover().unwrap().delivered);

        // The new provider speaks and a prompt goes out: resume is its own
        // id again and the carry is spent.
        writer.record_prompt("third question").unwrap();
        writer
            .record_event(
                &SessionEvent::Init {
                    session_id: "codex-thread".into(),
                    model: "gpt-5.6-sol".into(),
                },
                None,
            )
            .unwrap();
        writer.flush().unwrap();
        let thread = Store::open(&dir).unwrap().load(id).unwrap();
        assert_eq!(thread.resume_target(), Some("codex-thread"));
        assert!(thread.last_handover().unwrap().delivered);
        assert!(store.create(Provider::Claude, None, main_choice()).is_ok());
        assert!(Store::open(&dir)
            .unwrap()
            .load(ThreadId::new(2))
            .unwrap()
            .last_handover()
            .is_none());
    }

    /// A realistic Claude-shaped turn: identity, thinking, markdown streamed
    /// in ragged deltas, a tool run, a paid ending.
    fn claude_turn() -> Vec<SessionEvent> {
        vec![
            SessionEvent::Init {
                session_id: "4f2a1c9e-7b30".into(),
                model: "claude-haiku-4-5".into(),
            },
            SessionEvent::ThinkingDelta {
                text: "weighing ".into(),
            },
            SessionEvent::ThinkingDelta {
                text: "options".into(),
            },
            SessionEvent::TextDelta {
                text: "## Plan\nfirst ".into(),
            },
            SessionEvent::TextDelta {
                text: "step\n\n".into(),
            },
            SessionEvent::ToolStarted {
                id: "toolu_1".into(),
                name: "Bash".into(),
                input: serde_json::json!({ "command": "cargo test" }),
            },
            SessionEvent::ToolCompleted {
                id: "toolu_1".into(),
                output: "42 passed".into(),
                is_error: false,
                result: ToolResult::Opaque,
            },
            SessionEvent::TextDelta {
                text: "done".into(),
            },
            SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: Some(0.02),
            },
        ]
    }

    /// Replay a snapshot's history into a fresh Transcript.
    fn restore(thread: &ThreadSnapshot) -> Transcript {
        let mut transcript = Transcript::default();
        for input in thread.inputs() {
            transcript.apply(input);
        }
        transcript
    }

    /// A deterministic stream of non-boundary Session events, `seed`-shaped.
    /// Plain LCG: the point is many varied interleavings, not randomness.
    fn arbitrary_mid_turn_events(seed: u64, count: usize) -> Vec<SessionEvent> {
        let mut state = seed;
        let mut next = move || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            state
        };
        (0..count)
            .map(|n| match next() % 6 {
                0 => SessionEvent::TextDelta {
                    text: format!("delta {n} "),
                },
                1 => SessionEvent::ThinkingDelta {
                    text: format!("thought {n} "),
                },
                2 => SessionEvent::ReasoningSummaryDelta {
                    text: format!("summary {n} "),
                    summary_index: next() % 3,
                },
                3 => SessionEvent::ToolStarted {
                    id: format!("toolu_{n}"),
                    name: "Bash".into(),
                    input: serde_json::json!({ "command": format!("step {n}") }),
                },
                4 => SessionEvent::ToolCompleted {
                    id: format!("toolu_{n}"),
                    output: format!("out {n}"),
                    is_error: n % 2 == 0,
                    result: ToolResult::Opaque,
                },
                _ => SessionEvent::TokenUsage {
                    total_tokens: next() % 10_000,
                    input_tokens: 1,
                    cached_input_tokens: 2,
                    output_tokens: 3,
                    reasoning_output_tokens: 4,
                    context_window: None,
                },
            })
            .collect()
    }

    /// AC: no durable write ever occurs per delta. The property, over many
    /// arbitrary mid-turn streams: appending any non-boundary event leaves
    /// the file untouched, and the boundary that ends the turn is the single
    /// moment the file grows.
    #[test]
    fn no_durable_write_ever_occurs_per_delta() {
        let dir = scratch("no-per-delta");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        let log = dir.join(id.to_string()).join("log.jsonl");

        for seed in 0..32u64 {
            let start = fs::metadata(&log).unwrap().len();
            let count = (seed % 40 + 5) as usize;
            for event in arbitrary_mid_turn_events(seed, count) {
                writer.record_event(&event, None).unwrap();
                assert_eq!(
                    fs::metadata(&log).unwrap().len(),
                    start,
                    "a durable write occurred mid-turn (seed {seed})"
                );
            }
            writer
                .record_event(
                    &SessionEvent::TurnEnded {
                        outcome: TurnOutcome::Completed,
                        cost_usd: None,
                    },
                    None,
                )
                .unwrap();
            assert!(
                fs::metadata(&log).unwrap().len() > start,
                "the boundary did not flush (seed {seed})"
            );
        }
    }

    /// The third flush trigger: a turn that streams for a long time hits the
    /// interval and its tail becomes durable mid-turn — still never one
    /// write per delta.
    #[test]
    fn a_long_turn_flushes_on_the_interval_not_per_delta() {
        let dir = scratch("interval");
        let store = Store::with_flush_interval(&dir, std::time::Duration::from_millis(50)).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        let log = dir.join(id.to_string()).join("log.jsonl");
        let header_len = fs::metadata(&log).unwrap().len();

        writer
            .record_event(
                &SessionEvent::TextDelta {
                    text: "mid-turn ".into(),
                },
                None,
            )
            .unwrap();
        assert_eq!(
            fs::metadata(&log).unwrap().len(),
            header_len,
            "the first delta must only buffer"
        );

        std::thread::sleep(std::time::Duration::from_millis(80));
        writer
            .record_event(
                &SessionEvent::TextDelta {
                    text: "still going".into(),
                },
                None,
            )
            .unwrap();
        assert!(
            fs::metadata(&log).unwrap().len() > header_len,
            "the interval passed and nothing became durable"
        );
    }

    /// AC: a crash mid-turn loses at most the unflushed tail, never the
    /// Thread. The property, at every possible tear: chop the log at any
    /// byte — mid-line, mid-record, mid-UTF-8-character — and the Thread
    /// still loads, with some prefix of the full history. Only a tear inside
    /// the header (a crash inside `create` itself) may fail the load.
    #[test]
    fn a_crash_torn_tail_never_loses_the_thread() {
        let dir = scratch("torn");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        writer.record_prompt("try the café fix").unwrap();
        for event in claude_turn() {
            writer.record_event(&event, None).unwrap();
        }
        // Multi-byte characters on the final line: a tear can land inside
        // one, and a loader that insists on whole-file UTF-8 dies there.
        writer
            .record_event(
                &SessionEvent::TextDelta {
                    text: "naïve café ☕ résumé".into(),
                },
                None,
            )
            .unwrap();
        writer
            .record_event(
                &SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: None,
                },
                None,
            )
            .unwrap();
        drop(writer);

        let log = dir.join(id.to_string()).join("log.jsonl");
        let bytes = fs::read(&log).unwrap();
        let header_len = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
        let full = store.load(id).unwrap().inputs();

        for cut in 0..=bytes.len() {
            fs::write(&log, &bytes[..cut]).unwrap();
            let recovered = match store.load(id) {
                Ok(thread) => thread,
                // Refusal is allowed only for a tear inside `create`'s own
                // header write; once the header's newline is durable, a
                // failed load is a lost Thread.
                Err(e) => {
                    assert!(cut < header_len, "the Thread was lost at cut {cut}: {e}");
                    continue;
                }
            };
            assert_eq!(recovered.provider(), Provider::Claude);
            let inputs = recovered.inputs();
            assert!(
                full.starts_with(&inputs),
                "cut {cut} recovered something the full log never held: {inputs:?}"
            );
        }
    }

    /// Write a raw log for one thread, bypassing the writer — how tests plant
    /// logs from other schema eras and torn files.
    fn plant_log(dir: &Path, thread: &str, contents: &str) {
        let thread_dir = dir.join(thread);
        fs::create_dir_all(&thread_dir).unwrap();
        fs::write(thread_dir.join("log.jsonl"), contents).unwrap();
    }

    /// The frozen contract for schema 1, byte for byte what its writer
    /// produced: no `prompt` records, no `result` on `tool_completed`. Logs
    /// like this exist on disks; they must load forever.
    const V1_LOG: &str = concat!(
        r#"{"schema":1,"provider":"claude"}"#,
        "\n",
        r#"{"type":"init","session_id":"legacy-4f2a","model":"claude-haiku-4-5"}"#,
        "\n",
        r#"{"type":"text","text":"running the suite\n\n"}"#,
        "\n",
        r#"{"type":"tool_started","id":"toolu_9","name":"Bash","input":{"command":"cargo test"}}"#,
        "\n",
        r#"{"type":"tool_completed","id":"toolu_9","output":"42 passed","is_error":false}"#,
        "\n",
        r#"{"type":"turn_ended","outcome":"completed","cost_usd":0.01}"#,
        "\n",
    );

    /// AC: loading a log written at schema v1 succeeds after the bump to v2.
    /// v1 recorded no prompts and no structured results, so the load carries
    /// exactly that — nothing lost, nothing invented.
    #[test]
    fn a_log_written_at_schema_v1_still_loads_after_the_bump() {
        let dir = scratch("v1");
        plant_log(&dir, "7", V1_LOG);

        let store = Store::open(&dir).unwrap();
        let thread = store.load(ThreadId::new(7)).unwrap();
        assert_eq!(thread.provider(), Provider::Claude);
        assert_eq!(thread.resume_target(), Some("legacy-4f2a"));
        assert_eq!(
            thread.inputs(),
            vec![
                Input::Event(SessionEvent::Init {
                    session_id: "legacy-4f2a".into(),
                    model: "claude-haiku-4-5".into(),
                }),
                Input::Event(SessionEvent::TextDelta {
                    text: "running the suite\n\n".into(),
                }),
                Input::Event(SessionEvent::ToolStarted {
                    id: "toolu_9".into(),
                    name: "Bash".into(),
                    input: serde_json::json!({ "command": "cargo test" }),
                }),
                Input::Event(SessionEvent::ToolCompleted {
                    id: "toolu_9".into(),
                    output: "42 passed".into(),
                    is_error: false,
                    result: ToolResult::Opaque,
                }),
                Input::Event(SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: Some(0.01),
                }),
            ]
        );
    }

    /// A resumed old Thread gets new turns. Appending schema-2 records under
    /// a schema-1 header would make the file a lie — a v1 reader would stop
    /// at the first record it cannot know and silently lose everything after
    /// it. So reopening a v1 Thread for writing upgrades the whole log to
    /// the current schema first.
    #[test]
    fn appending_to_a_v1_thread_upgrades_its_log_first() {
        let dir = scratch("v1-append");
        plant_log(&dir, "7", V1_LOG);
        let store = Store::open(&dir).unwrap();

        let mut writer = store.writer(ThreadId::new(7)).unwrap();
        writer.record_prompt("continue where you left off").unwrap();
        writer
            .record_event(
                &SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: None,
                },
                None,
            )
            .unwrap();
        drop(writer);

        let log = dir.join("7").join("log.jsonl");
        let first_line = fs::read_to_string(&log)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_string();
        assert!(
            first_line.contains(&format!("\"schema\":{SCHEMA_VERSION}")),
            "the log still declares the old schema: {first_line}"
        );

        let thread = store.load(ThreadId::new(7)).unwrap();
        let inputs = thread.inputs();
        assert_eq!(inputs.len(), 7, "history: {inputs:?}");
        assert_eq!(
            inputs[0],
            Input::Event(SessionEvent::Init {
                session_id: "legacy-4f2a".into(),
                model: "claude-haiku-4-5".into(),
            }),
            "the v1 history must survive the upgrade"
        );
        assert_eq!(
            inputs[5],
            Input::Prompt("continue where you left off".into())
        );
    }

    /// The turn after the crash: a torn final line must not swallow what
    /// comes next. Appending straight after the tear would concatenate the
    /// first new record onto the fragment — one unreadable line where the
    /// loader stops, hiding every turn after the crash. Reopening for write
    /// clears the tear first.
    #[test]
    fn appending_after_a_crash_never_hides_the_new_turn() {
        let dir = scratch("torn-append");
        plant_log(
            &dir,
            "4",
            concat!(
                r#"{"schema":2,"provider":"codex"}"#,
                "\n",
                r#"{"type":"init","session_id":"0199-thread","model":"gpt-5.4-mini"}"#,
                "\n",
                r#"{"type":"turn_ended","outcome":"completed","cost_usd":null}"#,
                "\n",
                // The crash: a record torn mid-write, no newline.
                r#"{"type":"text","te"#,
            ),
        );
        let store = Store::open(&dir).unwrap();

        let mut writer = store.writer(ThreadId::new(4)).unwrap();
        writer.record_prompt("are you still there").unwrap();
        writer
            .record_event(
                &SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: None,
                },
                None,
            )
            .unwrap();
        drop(writer);

        let inputs = store.load(ThreadId::new(4)).unwrap().inputs();
        assert_eq!(
            inputs,
            vec![
                Input::Event(SessionEvent::Init {
                    session_id: "0199-thread".into(),
                    model: "gpt-5.4-mini".into(),
                }),
                Input::Event(SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: None,
                }),
                Input::Prompt("are you still there".into()),
                Input::Event(SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: None,
                }),
            ]
        );
    }

    /// The version gate is what the header line is for: a log from a newer
    /// Ferrite is refused whole, never half-read into a lie.
    #[test]
    fn a_log_from_ferrites_future_is_refused_not_half_read() {
        let dir = scratch("future");
        plant_log(
            &dir,
            "3",
            concat!(
                r#"{"schema":999,"provider":"claude"}"#,
                "\n",
                r#"{"type":"init","session_id":"from-the-future","model":"m"}"#,
                "\n",
            ),
        );

        let store = Store::open(&dir).unwrap();
        match store.load(ThreadId::new(3)) {
            Err(LoadError::FutureSchema { found, supported }) => {
                assert_eq!(found, 999);
                assert_eq!(supported, SCHEMA_VERSION);
            }
            Ok(_) => panic!("a future schema must not load"),
            Err(other) => panic!("expected FutureSchema, got {other:?}"),
        }
    }

    /// What the operator typed and what the agent changed are both history:
    /// the restored Pane must show the prompt echo and the diff card, not
    /// just the prose between them.
    #[test]
    fn the_operator_prompt_and_the_diff_card_survive_the_round_trip() {
        let dir = scratch("prompt-diff");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();

        let mut live = Transcript::default();
        writer.record_prompt("fix the typo").unwrap();
        live.apply(Input::Prompt("fix the typo".into()));
        for event in [
            SessionEvent::TextDelta {
                text: "Editing now.\n\n".into(),
            },
            SessionEvent::ToolStarted {
                id: "toolu_edit".into(),
                name: "Edit".into(),
                input: serde_json::json!({ "file_path": "/workspace/x.txt" }),
            },
            SessionEvent::ToolCompleted {
                id: "toolu_edit".into(),
                output: "applied".into(),
                is_error: false,
                result: ToolResult::FileEdit {
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
            },
            SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            },
        ] {
            writer.record_event(&event, None).unwrap();
            live.apply(Input::Event(event));
        }
        drop(writer);

        let thread = Store::open(&dir).unwrap().load(id).unwrap();
        let restored = restore(&thread);
        assert_eq!(restored.blocks(), live.blocks());
    }

    /// CT-3/CT-24: a prompt's send time and a turn's token counts are live
    /// observations kept beside the prompt and the turn's end; reopening
    /// the store replays them exactly, and a log without them draws none.
    #[test]
    fn a_prompts_send_time_and_a_turns_tokens_survive_reopening() {
        let dir = scratch("prompt-time");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        writer.record_prompt("apply it").unwrap();
        writer.record_prompt_observation("7:31 pm").unwrap();
        writer
            .record_event(
                &SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: None,
                },
                None,
            )
            .unwrap();
        writer
            .record_completion(
                &crate::activity::Subject::Main,
                &CompletionFacts {
                    elapsed_ms: 41_000,
                    completed_at: "7:32 pm".into(),
                    input_tokens: Some(3_200),
                    output_tokens: Some(1_100),
                },
            )
            .unwrap();
        writer.record_prompt("older log, no time").unwrap();
        writer.flush().unwrap();
        drop(writer);

        let restored = restore(&Store::open(&dir).unwrap().load(id).unwrap());
        let prompts: Vec<_> = restored
            .blocks()
            .iter()
            .filter(|block| matches!(block.body, crate::transcript::Body::Prompt(_)))
            .map(|block| block.sent_at.clone())
            .collect();
        assert_eq!(prompts, [Some("7:31 pm".to_string()), None]);
        let stamp = restored
            .blocks()
            .iter()
            .find_map(|block| match &block.body {
                crate::transcript::Body::TurnEnd(end) => Some(end.text()),
                _ => None,
            })
            .expect("the turn's stamp");
        assert_eq!(
            stamp,
            "Worked for 41s \u{b7} 7:32 pm \u{b7} \u{2191} 3.2k \u{2193} 1.1k"
        );
    }

    /// Codex's own concepts survive the round trip: reasoning summaries keep
    /// their part structure, token accounting keeps its numbers, and the
    /// provider-native thread id is the resume target. Asserted on `inputs`
    /// rather than Blocks because nothing renders reasoning summaries yet —
    /// the store must not lose what a Pane does not yet draw.
    #[test]
    fn a_codex_turn_keeps_its_reasoning_and_token_accounting() {
        let dir = scratch("codex");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Codex, None, main_choice()).unwrap();

        let usage = SessionEvent::TokenUsage {
            total_tokens: 900,
            input_tokens: 600,
            cached_input_tokens: 100,
            output_tokens: 300,
            reasoning_output_tokens: 120,
            context_window: Some(272_000),
        };
        for event in [
            SessionEvent::Init {
                session_id: "0199a1b2-thread".into(),
                model: "gpt-5.4-mini".into(),
            },
            SessionEvent::ReasoningSummaryDelta {
                text: "planning ".into(),
                summary_index: 0,
            },
            SessionEvent::ReasoningSummaryDelta {
                text: "the fix".into(),
                summary_index: 0,
            },
            SessionEvent::ReasoningSummaryDelta {
                text: "running tests".into(),
                summary_index: 1,
            },
            usage.clone(),
            SessionEvent::TurnEnded {
                outcome: TurnOutcome::Interrupted,
                cost_usd: None,
            },
            SessionEvent::Closed {
                reason: "codex app-server exited: exit status: 0".into(),
            },
        ] {
            writer.record_event(&event, None).unwrap();
        }
        drop(writer);

        let thread = Store::open(&dir).unwrap().load(id).unwrap();
        assert_eq!(thread.provider(), Provider::Codex);
        assert_eq!(thread.resume_target(), Some("0199a1b2-thread"));
        assert_eq!(
            thread.inputs(),
            vec![
                Input::Event(SessionEvent::Init {
                    session_id: "0199a1b2-thread".into(),
                    model: "gpt-5.4-mini".into(),
                }),
                Input::Event(SessionEvent::ReasoningSummaryDelta {
                    text: "planning the fix".into(),
                    summary_index: 0,
                }),
                Input::Event(SessionEvent::ReasoningSummaryDelta {
                    text: "running tests".into(),
                    summary_index: 1,
                }),
                Input::Event(usage),
                Input::Event(SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Interrupted,
                    cost_usd: None,
                }),
                Input::Event(SessionEvent::Closed {
                    reason: "codex app-server exited: exit status: 0".into(),
                }),
            ]
        );
    }

    #[test]
    fn a_flushed_turn_replays_identically_after_reopening() {
        let dir = scratch("roundtrip");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();

        let mut live = Transcript::default();
        for event in claude_turn() {
            writer.record_event(&event, None).unwrap();
            live.apply(Input::Event(event));
        }
        drop(writer);
        drop(store);

        let thread = Store::open(&dir).unwrap().load(id).unwrap();
        assert_eq!(thread.resume_target(), Some("4f2a1c9e-7b30"));

        let restored = restore(&thread);
        assert_eq!(restored.blocks(), live.blocks());
        assert_eq!(restored.session_id(), live.session_id());
        assert_eq!(restored.model(), live.model());
        assert_eq!(restored.last_cost(), live.last_cost());
        assert_eq!(restored.turn_outcome(), live.turn_outcome());
        assert_eq!(restored.turn_completed(), live.turn_completed());
        assert_eq!(restored.status(), live.status());
    }

    #[test]
    fn persisted_turn_outcomes_and_next_prompts_restore_the_completion_reading() {
        let dir = scratch("turn-outcomes");
        let store = Store::open(&dir).unwrap();
        for cost_usd in [None, Some(0.038)] {
            for outcome in [
                TurnOutcome::Completed,
                TurnOutcome::Interrupted,
                TurnOutcome::Error("model overloaded".into()),
            ] {
                let (id, mut writer) = store.create(Provider::Codex, None, main_choice()).unwrap();
                writer.record_prompt("go").unwrap();
                writer
                    .record_event(
                        &SessionEvent::TurnEnded {
                            outcome: outcome.clone(),
                            cost_usd,
                        },
                        None,
                    )
                    .unwrap();

                let reopened = Store::open(&dir).unwrap();
                let mut restored = restore(&reopened.load(id).unwrap());
                restored.apply(Input::Revived);
                assert_eq!(restored.turn_outcome(), Some(&outcome));
                assert_eq!(restored.last_cost(), cost_usd);
                assert_eq!(restored.turn_completed(), outcome == TurnOutcome::Completed);

                writer.record_prompt("next turn").unwrap();
                writer.flush().unwrap();
                let mut active = restore(&reopened.load(id).unwrap());
                active.apply(Input::Revived);
                assert_eq!(active.turn_outcome(), None);
                assert!(!active.turn_completed());
                assert_eq!(active.status(), crate::transcript::Status::Idle);
                assert!(
                    active.turn_elapsed().is_none(),
                    "replayed activity has no live clock"
                );
            }
        }
    }

    #[test]
    fn a_created_thread_survives_reopening_the_store() {
        let dir = scratch("reopen");
        {
            let store = Store::open(&dir).unwrap();
            let (id, _writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
            assert_eq!(id.to_string(), "1");
        }

        // The fake restart: nothing survives but the directory.
        let store = Store::open(&dir).unwrap();
        let ids = store.thread_ids().unwrap();
        assert_eq!(ids.len(), 1);
        let thread = store.load(ids[0]).unwrap();
        assert_eq!(thread.id, ids[0]);
        assert_eq!(thread.provider(), Provider::Claude);
        assert_eq!(thread.resume_target(), None);
        assert!(thread.inputs().is_empty());
    }
}
