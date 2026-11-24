//! A Thread's log, line by line (ADR 0009): the schema every Thread is
//! written in, on disk and on the wire (ADR 0025). The persisted schema is
//! the log's own, versioned from day one and converted from `SessionEvent`:
//! live-model churn is never a data migration. Reading and writing a log's
//! file is the Store's.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::checkout::WorkspaceBinding;
use crate::header::{ThreadMeta, ThreadSummary};
use crate::project::ProjectId;
use crate::provider::Provider;
use crate::transcript::Input;
use crate::SessionEvent;

pub mod activity;
use activity::PersistedActivity;

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
/// - **14** — `tool_summary` records (a provider's summary of the calls it
///   names). A v1–v13 log loads with no summaries, which is what it
///   recorded.
/// - **15** — rewinds: a prompt's
///   observation carries the id it was sent under; a `rewind` record cuts
///   the conversation back to before the prompt it names, replay dropping
///   that prompt and everything after it; a `resume_at` record points the
///   next Session at an earlier entry of the provider's conversation. A
///   v1–v14 log loads with no ids, so none of its prompts can be rewound
///   to, and no rewinds.
/// - **16** — a `background` tool result: the call launched work that runs
///   on, whose outcome a later result for the same call brings (HD64). A
///   v1–v15 log has none, so its background launches read as settled.
/// - **17** — an `image` tool result: the call read a picture (the bytes
///   are not kept; the row draws the file its call names). A v1–v16 log
///   has none, so its image reads stay plain rows.
/// - **18** — a `tool_denied` record: the provider refused a call by its
///   permission mode (HD22). A v1–v17 log has none, so its refused calls
///   read as failures.
/// - **19** — a `wake` record: a prompt the Server sent the agent on its
///   own (`crate::wake`), such as a watched PR's news. A v1–v18 log has
///   none.
pub const SCHEMA_VERSION: u32 = 19;

/// The first line of every log: what wrote it, for what provider, working
/// where.
#[derive(Serialize, Deserialize)]
pub struct Header {
    pub schema: u32,
    pub provider: Provider,
    /// Schema 3+; a v1/v2 header loads as `None` — those Threads never
    /// recorded a binding.
    #[serde(default)]
    pub workspace: Option<PersistedBinding>,
    /// Schema 4+; the git repo inside the binding where work happens.
    /// `None` — and every v1–v3 header — means work in the binding itself.
    #[serde(default)]
    pub session_project_root: Option<PathBuf>,
    /// Schema 5+; the model this Thread chose before its first prompt.
    /// `None` — and every v1–v4 header — means the provider's default.
    #[serde(default)]
    pub model: Option<String>,
    /// Schema 6+; the registry id of the project this Thread's CWD choice
    /// named (#29). `None` — and every v1–v5 header — means unregistered:
    /// the binding's resolved paths still say where work happens.
    #[serde(default)]
    pub project_id: Option<ProjectId>,
    /// Schema 7+; absent means the generated `thread-NN` label.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snooze: Option<crate::snooze::Snooze>,
    /// Schema 8+; the reasoning effort this Thread chose. `None` — and
    /// every v1–v7 header — means the provider's default.
    #[serde(default)]
    pub effort: Option<String>,
    /// Whose the Thread is when not the operator's (ADR 0037). Absent —
    /// every Thread before it, and the operator's own — means theirs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<crate::ThreadOwner>,
    /// The Thread on another machine that delegated it. Absent
    /// — every Thread before it, and one started here — means none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegated_from: Option<crate::DelegatedFrom>,
    /// The key of the `thread.start` that made it, which the same key
    /// finds again rather than starting another.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    /// The most its agent may do without asking. Absent means uncapped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap: Option<crate::runtime_mode::RuntimeMode>,
    /// The PRs the Thread's agent watches (ADR 0046). Absent means none.
    #[serde(default, skip_serializing_if = "crate::pull_request::WatchFact::is_empty")]
    pub pr_watch: crate::pull_request::WatchFact,
    /// The Threads it started on other machines (ADR 0047). Absent means none.
    #[serde(default, skip_serializing_if = "crate::delegate::Delegates::is_empty")]
    pub delegates: crate::delegate::Delegates,
}

impl Header {
    pub fn facts(&self) -> Facts {
        Facts {
            provider: self.provider,
            workspace: self.workspace.clone(),
            session_project_root: self.session_project_root.clone(),
            model: self.model.clone(),
            project_id: self.project_id,
            title: self.title.clone(),
            snooze: self.snooze,
            effort: self.effort.clone(),
            owner: self.owner.clone(),
            delegated_from: self.delegated_from.clone(),
            request: self.request.clone(),
            cap: self.cap,
            pr_watch: self.pr_watch.clone(),
            delegates: self.delegates.clone(),
        }
    }

    pub fn of(facts: Facts) -> Self {
        Header {
            schema: SCHEMA_VERSION,
            provider: facts.provider,
            workspace: facts.workspace,
            session_project_root: facts.session_project_root,
            model: facts.model,
            project_id: facts.project_id,
            title: facts.title,
            snooze: facts.snooze,
            effort: facts.effort,
            owner: facts.owner,
            delegated_from: facts.delegated_from,
            request: facts.request,
            cap: facts.cap,
            pr_watch: facts.pr_watch,
            delegates: facts.delegates,
        }
    }
}

/// A Thread's header facts (schema 13): what its first line says, and what
/// every record that changes them restates in full — never a delta, so the
/// newest one alone is the truth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Facts {
    pub provider: Provider,
    #[serde(default)]
    pub workspace: Option<PersistedBinding>,
    #[serde(default)]
    pub session_project_root: Option<PathBuf>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub project_id: Option<ProjectId>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snooze: Option<crate::snooze::Snooze>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<crate::ThreadOwner>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegated_from: Option<crate::DelegatedFrom>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap: Option<crate::runtime_mode::RuntimeMode>,
    #[serde(default, skip_serializing_if = "crate::pull_request::WatchFact::is_empty")]
    pub pr_watch: crate::pull_request::WatchFact,
    #[serde(default, skip_serializing_if = "crate::delegate::Delegates::is_empty")]
    pub delegates: crate::delegate::Delegates,
}

impl Facts {
    pub fn meta(self) -> ThreadMeta {
        ThreadMeta {
            provider: self.provider,
            workspace: self.workspace.as_ref().map(PersistedBinding::live),
            session_project_root: self.session_project_root,
            model: self.model,
            project_id: self.project_id,
            title: self.title,
            snooze: self.snooze,
            effort: self.effort,
            owner: self.owner,
            delegated_from: self.delegated_from,
            request: self.request,
            cap: self.cap,
            pr_watch: self.pr_watch,
            delegates: self.delegates,
            summary: None,
        }
    }
}

/// What a parked Thread's nav row counts (schema 13, in every mark): read
/// off the log's tail instead of replaying the whole log.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    /// Prompts since the last conversation reset: the wall's `11 turns`.
    pub turns: u64,
    /// Whether any prompt was ever sent, which locks the provider choice.
    pub prompted: bool,
    /// The subagents the live Activity knew when the mark was written;
    /// absent when no Activity has told the writer yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagents: Option<u64>,
}

impl Summary {
    pub fn public(&self) -> ThreadSummary {
        ThreadSummary {
            turns: self.turns as usize,
            prompted: self.prompted,
            subagents: self.subagents.map(|count| count as usize),
        }
    }

    /// Count `record` in.
    pub fn observe(&mut self, record: &Record) {
        match record {
            Record::Prompt { .. } => {
                self.turns += 1;
                self.prompted = true;
            }
            // A wake starts a turn, but it is not the operator prompting.
            Record::Wake { .. } => self.turns += 1,
            Record::ConversationReset { .. } => self.turns = 0,
            Record::Rewind { dropped, .. } => self.turns = self.turns.saturating_sub(*dropped),
            _ => {}
        }
    }
}

/// A replay base, as a mark names it: the offset of a turn start, and the
/// offsets of the records before it a replay from there needs first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Base {
    pub at: u64,
    pub carry: Vec<u64>,
}

/// The persisted form of a Thread's workspace binding, mirroring
/// `checkout::WorkspaceBinding` shape for shape — but the store's own type,
/// so the live vocabulary can change without rewriting anyone's history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PersistedBinding {
    Main { checkout: PathBuf },
    Worktree { repo: PathBuf, path: PathBuf },
}

impl PersistedBinding {
    pub fn from_live(binding: &WorkspaceBinding) -> Self {
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

    pub fn live(&self) -> WorkspaceBinding {
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Record {
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
    /// Schema 19: a prompt the Server sent on its own (`crate::wake`).
    Wake {
        text: String,
        summary: String,
        outcome: crate::wake::Outcome,
        #[serde(default, skip_serializing_if = "crate::wake::Source::is_pr_watch")]
        source: crate::wake::Source,
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
    /// Schema 14: the provider's summary of the calls it names.
    ToolSummary {
        ids: Vec<String>,
        summary: String,
    },
    /// Schema 18: the provider refused this call by its permission mode.
    ToolDenied {
        id: String,
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
        /// Schema 15: the id the prompt was sent under, which a rewind
        /// names it by. Absent from older logs and for a prompt whose
        /// provider took no id: neither can be rewound to.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt_id: Option<String>,
    },
    /// The wall clock, cut to its minute, before the first event of each
    /// minute a Thread was busy: what the transcript stamps the work after
    /// it with. Absent from older logs, whose work draws no time.
    Clock {
        at: String,
    },
    /// Schema 15: the conversation went back to before the prompt whose
    /// observation carries `prompt`. Replay drops that prompt and every
    /// line after it up to here; the log itself stays append-only.
    /// `dropped` is how many prompts went, for the turn count.
    Rewind {
        prompt: String,
        dropped: u64,
    },
    /// Schema 15: the conversation the next Session resumes, when it is
    /// not the one the last Init named (`ResumePoint`): the provider's
    /// `session`, at its entry `at` rather than at its end, forked into a
    /// conversation of its own when `fork`.
    ResumeAt {
        session: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        fork: bool,
        /// Schema 16: the prompt a rewind's cut drops; absent before.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        drops: Option<String>,
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
    Facts {
        facts: Facts,
    },
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
    pub fn facts(&self) -> Option<&Facts> {
        match self {
            Record::Facts { facts } | Record::Mark { facts, .. } => Some(facts),
            Record::Handover { facts, .. } => facts.as_ref(),
            _ => None,
        }
    }

    /// The store's own bookkeeping, folded into the facts when a log is
    /// read and never part of the conversation.
    pub fn is_bookkeeping(&self) -> bool {
        matches!(self, Record::Facts { .. } | Record::Mark { .. })
    }
}

/// One log line as the client protocol carries it (ADR 0025): everything
/// durable crosses the wire in the log's own schema, so the Server never
/// re-encodes history and a client reads exactly what a reopen would.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LogRecord(Record);

impl LogRecord {
    /// The line a live transcript input is logged as, or `None` for what
    /// the log never keeps. `duration` is as `ThreadWriter::record_event`'s.
    pub fn of(input: &Input, duration: Option<std::time::Duration>) -> Option<Self> {
        Some(Self(match input {
            Input::Event(event) => Record::from_event(event, duration)?,
            Input::Prompt(text) => Record::Prompt { text: text.clone() },
            Input::Wake(wake) => Record::Wake {
                text: wake.text.clone(),
                summary: wake.summary.clone(),
                outcome: wake.outcome,
                source: wake.source,
            },
            Input::PromptObservation { sent_at, prompt_id } => Record::PromptObservation {
                sent_at: sent_at.clone(),
                prompt_id: prompt_id.clone(),
            },
            Input::Rewind { prompt, dropped } => Record::Rewind {
                prompt: prompt.clone(),
                dropped: *dropped,
            },
            Input::ClockObservation { at } => Record::Clock { at: at.clone() },
            Input::CompletionObservation {
                elapsed_ms,
                completed_at,
                input_tokens,
                output_tokens,
            } => Record::CompletionObservation {
                elapsed_ms: *elapsed_ms,
                completed_at: completed_at.clone(),
                input_tokens: *input_tokens,
                output_tokens: *output_tokens,
            },
            Input::Notice(_)
            | Input::Answered { .. }
            | Input::Revived
            | Input::Highlighted { .. } => return None,
        }))
    }

    /// Extend this line with the next when the log would keep the two as
    /// one (a run of deltas): what lets a waiting client take one line.
    pub fn coalesce(&mut self, next: &LogRecord) -> bool {
        self.0.coalesce(&next.0)
    }

    /// What this line replays as; `None` for the store's own bookkeeping
    /// (facts and marks), which a reader skips.
    pub fn input(&self) -> Option<Input> {
        match &self.0 {
            Record::Facts { .. } | Record::Mark { .. } | Record::ResumeAt { .. } => None,
            record => Some(record.input()),
        }
    }
}

/// The Notice a provider switch leaves in the transcript, live and on
/// replay alike.
pub fn handover_notice(to: Provider, model: Option<&str>) -> String {
    let label = crate::models::label(model.unwrap_or("default"), &[]);
    format!("continued on {to} · {label} — the earlier conversation is handed over as context")
}

/// How a persisted turn ended: `"completed"`, `"interrupted"`, or
/// `{"error":"…"}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Completed,
    Interrupted,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredRunState {
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PersistedToolResult {
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
    /// Schema 16.
    Background {
        task: String,
    },
    /// Schema 17.
    Image,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedFileEdit {
    path: String,
    hunks: Vec<PersistedHunk>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedHunk {
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
            crate::ToolResult::Background { task } => {
                PersistedToolResult::Background { task: task.clone() }
            }
            crate::ToolResult::Image => PersistedToolResult::Image,
            // An Activity's own record, never a provider's event.
            crate::ToolResult::Preview { .. } => {
                debug_assert!(false, "a row's preview is never logged");
                PersistedToolResult::Opaque
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
            PersistedToolResult::Background { task } => {
                crate::ToolResult::Background { task: task.clone() }
            }
            PersistedToolResult::Image => crate::ToolResult::Image,
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
pub enum PersistedProgress {
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
    PlanSteps {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        done: Vec<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<usize>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        advance: bool,
    },
    PlanSettled,
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
    Compacted {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        failure: Option<String>,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredBackgroundTask {
    id: String,
    label: String,
    status: StoredTaskStatus,
    detail: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredStep {
    text: String,
    status: StoredStepStatus,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredPlanTask {
    id: String,
    text: String,
    status: StoredStepStatus,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredPhase {
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
pub enum StoredStepStatus {
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
pub enum StoredTaskStatus {
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
            E::PlanSteps(steps) => Self::PlanSteps {
                done: steps.done.clone(),
                start: steps.start,
                advance: steps.advance,
            },
            E::PlanSettled => Self::PlanSettled,
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
            E::Compacted(compacted) => Self::Compacted {
                before: compacted.before,
                after: compacted.after,
                failure: compacted.failure.clone(),
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
            Self::PlanSteps {
                done,
                start,
                advance,
            } => E::PlanSteps(crate::update_plan::Steps {
                done: done.clone(),
                start: *start,
                advance: *advance,
            }),
            Self::PlanSettled => E::PlanSettled,
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
            Self::Compacted {
                before,
                after,
                failure,
            } => E::Compacted(crate::transcript::Compacted {
                before: *before,
                after: *after,
                failure: failure.clone(),
            }),
        }
    }
}

impl Record {
    /// The persisted form of one live event, or `None` for events that are
    /// Session state rather than durable history (a pending Decision dies
    /// with its Session; there is nothing to replay it into). `duration` is
    /// the wall clock the cockpit measured for a settled tool call — the one
    /// thing in the record no event carries.
    pub fn from_event(event: &SessionEvent, duration: Option<std::time::Duration>) -> Option<Record> {
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
            SessionEvent::ToolSummary { ids, summary } => Record::ToolSummary {
                ids: ids.clone(),
                summary: summary.clone(),
            },
            SessionEvent::ToolDenied { id } => Record::ToolDenied { id: id.clone() },
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
            // What the Session says of itself is not history: the Thread
            // keeps the last of it beside the log (`engine::announced`).
            SessionEvent::Commands { .. }
            | SessionEvent::PermissionMode { .. }
            | SessionEvent::Models { .. }
            | SessionEvent::McpServers { .. } => return None,
            SessionEvent::Queue(_) => return None,
            SessionEvent::RateLimits { .. } => return None,
            SessionEvent::McpAuthorization { .. } => return None,
            // An estimate for the live line; the turn's usage is its record.
            SessionEvent::ThinkingTokens { .. } => return None,
            // Every Session says it again at its handshake.
            SessionEvent::AccountPlan { .. }
            | SessionEvent::Handshake { .. }
            | SessionEvent::HookRan { .. } => return None,
            // The provider's late no to a control: live, not history.
            SessionEvent::ControlRefused { .. } => return None,
            SessionEvent::ScheduledWork { .. } => return None,
        })
    }

    /// Replay this record as the transcript input it stands for.
    pub fn input(&self) -> Input {
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
            Record::Wake {
                text,
                summary,
                outcome,
                source,
            } => Input::Wake(crate::wake::Wake {
                text: text.clone(),
                summary: summary.clone(),
                outcome: *outcome,
                source: *source,
            }),
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
            Record::ToolSummary { ids, summary } => Input::Event(SessionEvent::ToolSummary {
                ids: ids.clone(),
                summary: summary.clone(),
            }),
            Record::ToolDenied { id } => Input::Event(SessionEvent::ToolDenied { id: id.clone() }),
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
            Record::PromptObservation { sent_at, prompt_id } => Input::PromptObservation {
                sent_at: sent_at.clone(),
                prompt_id: prompt_id.clone(),
            },
            Record::Rewind { prompt, dropped } => Input::Rewind {
                prompt: prompt.clone(),
                dropped: *dropped,
            },
            Record::Clock { at } => Input::ClockObservation { at: at.clone() },
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
            Record::ResumeAt { .. } => {
                unreachable!("a resume point is the next Session's, never replayed")
            }
        }
    }

    /// Whether the log is consistent here — the transcript's boundary marks,
    /// seen from the store's side of the conversion. A handover is one too:
    /// the header rewrite that follows it must find it on disk.
    pub fn is_boundary(&self) -> bool {
        matches!(self, Record::Activity { observation } if observation.is_boundary())
            || matches!(
                self,
                Record::TurnEnded { .. }
                    | Record::CompletionObservation { .. }
                    | Record::Closed { .. }
                    | Record::Handover { .. }
                    | Record::Rewind { .. }
                    | Record::ResumeAt { .. }
            )
    }

    /// Extend this record with a later delta of the same kind, if the two
    /// coalesce. A run of deltas is one record; anything else keeps its line.
    pub fn coalesce(&mut self, next: &Record) -> bool {
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

