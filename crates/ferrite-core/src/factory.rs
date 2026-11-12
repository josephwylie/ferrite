//! The Factory (ADR 0037): each Project's delegated work, headless. It
//! knows nothing of the Server or a Provider's process. Its interface is
//! three parts:
//!
//! - [`Factory::command`]: every change, whoever asks (a window, a Worker,
//!   a script), through one door, answered or refused.
//! - [`Factory::step`]: once per Server pass, given the time and what each
//!   Worker's Thread is doing, it answers with [`Effect`]s for the Server
//!   to carry out. It reads no file and touches no agent while deciding.
//! - [`Factory::board`]: what the windows draw, with a [`Factory::revision`]
//!   that moves whenever it changes.
//!
//! Persist, then act: every change is written beside the Store before the
//! effects that depend on it are handed out, and a change that cannot be
//! written is not made. A repeated command or report changes nothing.
//!
//! A Task is one-off (a single Worker on the operator's default Provider
//! and model, #220) or runs through the Steps of one saved version of a
//! [`Workflow`] (#221), one Step at a time, each Step's Worker in the
//! Task's one worktree.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::store::Provider;
use crate::workspace::registry::ProjectId;
use crate::ThreadId;

mod archive;
mod coordinator;
mod direct;
mod pr;
mod recovery;
mod starts;
mod store;
mod workflow;

pub use archive::ARCHIVE_AFTER_MS;
pub use coordinator::{
    briefing, coordinator_command, Act, Coordinator, Proposal, ProposalId, ProposalStatus,
};
pub use direct::{Answered, Feedback, FeedbackKind, Override, Oversight, Question, ThenReturn};
pub use pr::{
    evidence_line, short, Delivery, Evidence, EvidenceKind, MergeOutcome, MergeRoutes, PullRequest,
    Revision, TIDY_AGAIN_MS,
};
pub use recovery::Keeping;
pub use starts::{when_label, zone_label};
pub use starts::{
    Automation, GitHubItem, GitHubQuery, GitHubRead, GitHubSource, Missed, Overlap, Recurrence,
    Schedule, Source, Start, StartKind, Switch, Trigger, Turn, INTERVAL_HOURS, POLL_MS, WEEKDAYS,
};
pub use store::{Factory, DIR_NAME};
pub use workflow::{
    Approval, ApprovalChange, Approver, Catalog, Definition, Destination, Problem, Resolved,
    RouteKind, Step, StepChange, StepId, Template, Version, Workflow, WorkflowId,
};

/// A Task's id: `t-<n>`, never given twice by one Server. A Worker's
/// owner names it (`{"task": "t-1"}`).
pub type TaskId = String;

/// Where the Factory keeps itself in the Store's directory.
pub const FILE_NAME: &str = "factory.json";
const SCHEMA_VERSION: u32 = 1;

/// How many Tasks work at once unless Settings say otherwise.
pub const DEFAULT_LIMIT: usize = 3;

/// How many Tasks Settings may let work at once: one rule, here, for the
/// Settings stepper and the Server alike.
pub const LIMIT_RANGE: std::ops::RangeInclusive<usize> = 1..=12;

/// `limit` as the Factory takes it: within [`LIMIT_RANGE`].
pub fn clamp_limit(limit: usize) -> usize {
    limit.clamp(*LIMIT_RANGE.start(), *LIMIT_RANGE.end())
}

/// How long a Worker prompted to report may stay at rest without working
/// again before its Task needs the operator.
pub const NUDGE_GRACE_MS: i64 = 60_000;

/// The board's columns, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    Working,
    NeedsHuman,
    /// Ready to approve (no one-off Task reaches it).
    Approval,
    /// Stopped or failed.
    Stopped,
    Completed,
}

impl Status {
    pub const ALL: [Status; 6] = [
        Status::Queued,
        Status::Working,
        Status::NeedsHuman,
        Status::Approval,
        Status::Stopped,
        Status::Completed,
    ];

    /// The board's column heading.
    pub fn column(self) -> &'static str {
        match self {
            Status::Queued => "Queued",
            Status::Working => "Working",
            Status::NeedsHuman => "Needs human",
            Status::Approval => "Ready to approve",
            Status::Stopped => "Stopped or failed",
            Status::Completed => "Completed",
        }
    }

    /// What a Task in it is called, alone.
    pub fn word(self) -> &'static str {
        match self {
            Status::Stopped => "Stopped",
            status => status.column(),
        }
    }

    /// The operator is wanted: the sidebar counts these.
    pub fn needs_you(self) -> bool {
        matches!(self, Status::NeedsHuman | Status::Approval)
    }

    /// Stop is offered (story 23).
    pub fn stoppable(self) -> bool {
        matches!(
            self,
            Status::Queued | Status::Working | Status::NeedsHuman | Status::Approval
        )
    }

    /// Delete is offered.
    pub fn deletable(self) -> bool {
        matches!(self, Status::Queued | Status::Stopped | Status::Completed)
    }
}

/// What runs a Step: a Provider, its model and Effort.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub provider: Provider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

/// Who did what, in a Task's activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    Ferrite,
    Agent,
    Operator,
    /// A command or tool's result, as an agent reported it (#223).
    Tool,
    /// The Project's Coordinator: what it proposed (#224).
    Coordinator,
}

impl Actor {
    pub fn word(self) -> &'static str {
        match self {
            Actor::Ferrite => "Ferrite",
            Actor::Agent => "Agent",
            Actor::Operator => "You",
            Actor::Tool => "Tool",
            Actor::Coordinator => "Coordinator",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Wall-clock milliseconds since the Unix epoch.
    pub at_ms: i64,
    pub by: Actor,
    pub text: String,
    /// The Task's PR revision when it happened (#223).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u32>,
}

/// One piece of delegated work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub project: ProjectId,
    /// The instructions' first line.
    pub title: String,
    pub instructions: String,
    pub status: Status,
    /// What its Worker runs on.
    pub agent: Agent,
    /// Its own branch, in its own worktree.
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<ThreadId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<PathBuf>,
    /// What the Worker reported it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// Why it needs the operator, or why it stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub activity: Vec<Entry>,
    pub created_ms: i64,
    /// The start request it answers, so a repeat is the same Task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    /// The at-rest rule's memory of the current turn.
    #[serde(default, skip_serializing_if = "Watch::is_fresh")]
    pub watch: Watch,
    /// The Workflow version it runs, kept whole; none for a one-off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<Run>,
    /// The Step it is at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<StepId>,
    /// What each Step reported, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub results: Vec<StepResult>,
    /// Each visited Step's Worker, which a return to it continues.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workers: Vec<StepWorker>,
    /// Questions, answers, feedback, approval and this Task's direction.
    #[serde(flatten)]
    pub oversight: Oversight,
    /// What started it without the operator (#225), if anything did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// Its pull request, evidence and worktree's fate (#223).
    #[serde(flatten)]
    pub delivery: Delivery,
    /// What restarts and time did to it (#226): its Worker revived, when
    /// it completed and when it was archived.
    #[serde(flatten)]
    pub keeping: Keeping,
}

/// The version of a Workflow a Task keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: WorkflowId,
    pub version: u32,
    pub definition: Definition,
}

/// What one Step of a Task reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepResult {
    pub step: StepId,
    /// The Step's name when it reported.
    pub name: String,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepWorker {
    pub step: StepId,
    pub thread: ThreadId,
}

/// What the at-rest rule remembers of a working Worker.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watch {
    /// How many turns the Worker had ended ([`Seen::turns`]) when it was
    /// last prompted: one more is the turn that prompt began, ended.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub baseline: u64,
    /// When it was asked to report, if it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nudged_ms: Option<i64>,
    /// A prompt for the next step to send, where its baseline is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Someone else prompted it (the operator): take the baseline afresh.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rebase: bool,
    /// Hold `prompt` until the Worker is at rest: it reported mid-turn
    /// and its turn is still running (#223, a merge Step kept waiting).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub at_rest: bool,
}

impl Watch {
    fn is_fresh(&self) -> bool {
        *self == Self::default()
    }
}

fn is_zero(count: &u64) -> bool {
    *count == 0
}

/// What the windows draw: every Project's Tasks, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    pub tasks: Vec<Task>,
    /// Every Project's Workflows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workflows: Vec<Workflow>,
    /// Why the Factory's store could not be read, when it could not: the
    /// Factory then does nothing until it can be (#226).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trouble: Option<String>,
    /// Why one Project's part of the store could not be read or written:
    /// that Project's Factory waits, every other one runs.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        with = "troubles_as_list"
    )]
    pub troubles: BTreeMap<ProjectId, String>,
    /// Every Project's archived Tasks, most recently archived last; none
    /// when a window is told only what changed and the Archive did not
    /// (it keeps the one it has).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<Vec<Task>>,
    /// Every Project's Coordinator that has been spoken to (#224).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coordinators: Vec<Coordinator>,
    /// Every Coordinator's proposals, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposals: Vec<Proposal>,
}

impl Board {
    pub fn coordinator(&self, project: ProjectId) -> Option<&Coordinator> {
        self.coordinators
            .iter()
            .find(|coordinator| coordinator.project == project)
    }

    pub fn proposal(&self, id: &str) -> Option<&Proposal> {
        self.proposals.iter().find(|proposal| proposal.id == id)
    }

    pub fn proposals_of(&self, project: ProjectId) -> impl Iterator<Item = &Proposal> {
        self.proposals
            .iter()
            .filter(move |proposal| proposal.project == project)
    }

    pub fn of(&self, project: ProjectId) -> impl Iterator<Item = &Task> {
        self.tasks
            .iter()
            .filter(move |task| task.project == project)
    }

    /// How many of `project`'s Tasks need the operator.
    pub fn needs_you(&self, project: ProjectId) -> usize {
        self.of(project)
            .filter(|task| task.status.needs_you())
            .count()
    }

    /// A Task on the board or in the Archive.
    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks
            .iter()
            .chain(self.archive.iter().flatten())
            .find(|task| task.id == id)
    }

    /// Why `project`'s Factory does nothing, if it does not.
    pub fn trouble_of(&self, project: ProjectId) -> Option<&str> {
        self.trouble
            .as_deref()
            .or_else(|| self.troubles.get(&project).map(String::as_str))
    }

    pub fn workflows_of(&self, project: ProjectId) -> impl Iterator<Item = &Workflow> {
        self.workflows
            .iter()
            .filter(move |workflow| workflow.project == project)
    }

    pub fn workflow(&self, id: &str) -> Option<&Workflow> {
        self.workflows.iter().find(|workflow| workflow.id == id)
    }
}

/// `troubles` on the wire: `[{"project": 1, "trouble": "…"}]`. As a map
/// its keys would go out as strings, which a Board inside a tagged
/// `ServerMessage` cannot read back as Project ids.
mod troubles_as_list {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::workspace::registry::ProjectId;

    #[derive(Serialize, Deserialize)]
    struct Trouble<T> {
        project: ProjectId,
        trouble: T,
    }

    pub fn serialize<S: Serializer>(
        troubles: &BTreeMap<ProjectId, String>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(troubles.iter().map(|(project, trouble)| Trouble {
            project: *project,
            trouble,
        }))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<ProjectId, String>, D::Error> {
        let troubles = Vec::<Trouble<String>>::deserialize(deserializer)?;
        Ok(troubles
            .into_iter()
            .map(|each| (each.project, each.trouble))
            .collect())
    }
}

/// What a Worker reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Finished {
        summary: String,
    },
    Unable {
        tried: String,
    },
    /// The deciding agent approves.
    Approved {
        summary: String,
    },
    /// The deciding agent does not, and why.
    Rejected {
        reasons: String,
    },
    /// A merge Step's result, and what showed it (#223).
    Merge {
        result: MergeOutcome,
        summary: String,
    },
}

/// Every change, whoever asks.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// A Task with `instructions`: one-off (one Worker on `agent`), or on
    /// `workflow`'s latest saved version. `request`, when given, makes a
    /// repeat the same Task.
    Start {
        project: ProjectId,
        instructions: String,
        agent: Agent,
        request: Option<String>,
        workflow: Option<WorkflowId>,
        /// Its pull request: `owner/repository#number` or a URL.
        pr: Option<String>,
    },
    /// A new Workflow from `template`, its Steps on `agent`.
    CreateWorkflow {
        project: ProjectId,
        template: Template,
        agent: Agent,
    },
    RenameWorkflow {
        workflow: WorkflowId,
        name: String,
    },
    /// A new Step on `agent`, after `after` (else last).
    AddStep {
        workflow: WorkflowId,
        after: Option<StepId>,
        agent: Agent,
    },
    EditStep {
        workflow: WorkflowId,
        step: StepId,
        change: StepChange,
    },
    /// Move a Step to index `to` of the outline.
    MoveStep {
        workflow: WorkflowId,
        step: StepId,
        to: usize,
    },
    /// Delete a Step; routes to it stay, flagged.
    DeleteStep {
        workflow: WorkflowId,
        step: StepId,
    },
    /// Save the draft as the next version, if it has no problems against
    /// `catalog`. Starts nothing.
    SaveWorkflow {
        workflow: WorkflowId,
        catalog: Catalog,
    },
    /// The Server started the Worker a [`Effect::StartWorker`] asked for.
    WorkerStarted {
        task: TaskId,
        thread: ThreadId,
        worktree: Option<PathBuf>,
    },
    /// The Server could not.
    WorkerFailed {
        task: TaskId,
        why: String,
    },
    /// A Worker reports, known by its own Thread.
    Report {
        worker: ThreadId,
        outcome: Outcome,
    },
    /// The operator said something to a Worker in its Task.
    OperatorSaid {
        worker: ThreadId,
        text: String,
    },
    Stop {
        task: TaskId,
    },
    Resume {
        task: TaskId,
    },
    Delete {
        task: TaskId,
    },
    /// A Worker asks the operator, known by its own Thread.
    Ask {
        worker: ThreadId,
        text: String,
    },
    /// The operator answers the question waiting (that one, when
    /// `question` names it).
    AnswerQuestion {
        task: TaskId,
        text: String,
        question: Option<u64>,
    },
    /// The operator approves what is Ready to approve.
    Approve {
        task: TaskId,
    },
    /// Rework with `feedback`, to `to` (the Workflow's recipient when
    /// none).
    Rework {
        task: TaskId,
        feedback: String,
        to: Option<Destination>,
    },
    /// One Task's direction; the saved Workflow is unchanged.
    Redirect {
        task: TaskId,
        direction: Override,
    },
    EditApproval {
        workflow: WorkflowId,
        change: ApprovalChange,
    },
    /// Set how a Workflow's draft starts its Tasks (#225).
    EditStart {
        workflow: WorkflowId,
        start: Start,
    },
    /// Turn a Workflow's automatic starts on, pause, resume or off.
    Automation {
        workflow: WorkflowId,
        turn: Turn,
    },
    /// Whether to take what already matched when a GitHub start was
    /// turned on; otherwise it is passed over.
    Backlog {
        workflow: WorkflowId,
        take: bool,
    },
    /// What a [`Effect::ReadGitHub`] found, or why it failed.
    GitHubRead {
        workflow: WorkflowId,
        version: u32,
        result: Result<GitHubRead, String>,
    },
    /// A script's Task for a Workflow that starts from one: a repeated
    /// `identity` is the Task it already started.
    Submit {
        workflow: WorkflowId,
        identity: String,
        instructions: String,
    },
    /// A Worker reports its Task's PR head (and which PR, the first
    /// time), known by its own Thread.
    ReportRevision {
        worker: ThreadId,
        head: String,
        pr: Option<String>,
    },
    /// A Worker reports what a command or tool showed.
    ReportEvidence {
        worker: ThreadId,
        text: String,
    },
    /// The Server tried to remove a merged Task's worktree: removed, or
    /// kept and why.
    WorktreeTidied {
        task: TaskId,
        kept: Option<String>,
    },
    /// What a [`Effect::ReadHead`] found: the PR's head commit, or why it
    /// could not be read.
    GitHubHead {
        task: TaskId,
        head: Result<String, String>,
    },
    /// The Completed column's Clear: `project`'s Completed Tasks go to the
    /// Archive now (#226).
    Clear {
        project: ProjectId,
    },
    /// The operator speaks to `project`'s Coordinator: it starts, with
    /// its briefing, if it has not (#224).
    CoordinatorSay {
        project: ProjectId,
        text: String,
    },
    /// The Server started the Coordinator a [`Effect::StartCoordinator`]
    /// asked for.
    CoordinatorStarted {
        project: ProjectId,
        thread: ThreadId,
    },
    /// It could not.
    CoordinatorFailed {
        project: ProjectId,
        why: String,
    },
    /// The Coordinator's Provider, model and effort, from its next turn.
    CoordinatorAgent {
        project: ProjectId,
        agent: Agent,
    },
    /// A Coordinator, known by its own Thread, proposes `act`; the save's
    /// check is against `catalog`.
    Propose {
        coordinator: ThreadId,
        act: Act,
        catalog: Catalog,
    },
    /// The operator accepts a proposal: its act runs as theirs (a Task on
    /// `agent` when one-off).
    AcceptProposal {
        proposal: ProposalId,
        agent: Agent,
        catalog: Catalog,
    },
    DismissProposal {
        proposal: ProposalId,
    },
}

impl Command {
    /// It reports what the Server already did: a failed write cannot
    /// refuse it, so it is taken and written later (Persist, then act).
    fn reports_effect(&self) -> bool {
        matches!(
            self,
            Command::WorkerStarted { .. }
                | Command::WorkerFailed { .. }
                | Command::WorktreeTidied { .. }
                | Command::GitHubRead { .. }
                | Command::GitHubHead { .. }
                | Command::CoordinatorStarted { .. }
                | Command::CoordinatorFailed { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Task(TaskId),
    /// The Task a repeated submission already started (#225).
    Duplicate(TaskId),
    Workflow(WorkflowId),
    Step(StepId),
    /// The version a save made, or the latest when nothing changed.
    Version(u32),
    /// Whether the command changed anything: a repeat does not.
    Changed(bool),
    /// The proposal raised, or the same one already waiting.
    Proposal(ProposalId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalCode {
    UnknownTask,
    UnknownWorkflow,
    UnknownStep,
    /// The Thread is no Task's Worker.
    NotAWorker,
    /// Not in the Task's current state.
    NotNow,
    Invalid,
    /// The change could not be written; nothing changed.
    Failed,
    UnknownProposal,
    /// The Thread is no Project's Coordinator.
    NotACoordinator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: RefusalCode,
    pub message: String,
}

impl Refusal {
    fn new(code: RefusalCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// What the Server tells the step of one Worker's Thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    /// Not at rest: Main, a Subagent or a background task working, a
    /// prompt held for its starting Session, or a Decision waiting.
    pub busy: bool,
    /// A count that grows each time one of its turns ends (its
    /// `completion_seq`; 0 before its first).
    pub turns: u64,
    /// A Provider Decision waiting in its Session, as it reads.
    pub decision: Option<String>,
    /// No Session serves it and none ended at rest: its Session closed on
    /// its own, or the Server restarted, and nothing starts one (#226).
    pub lost: bool,
    /// Its Thread is not open: someone parked it (`project.close`,
    /// `thread.close`) or it is gone.
    pub closed: bool,
}

/// What the Server carries out, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Start an owned Thread for `task` on `agent`, in its `worktree`, or
    /// a new one on `branch` when it has none yet, and send it `prompt`.
    StartWorker {
        task: TaskId,
        project: ProjectId,
        agent: Agent,
        branch: String,
        worktree: Option<PathBuf>,
        prompt: String,
    },
    Prompt {
        thread: ThreadId,
        text: String,
    },
    Interrupt {
        thread: ThreadId,
    },
    Park {
        thread: ThreadId,
    },
    /// Delete a deleted Task's Worker for good, open or parked (#226).
    DeleteThread {
        thread: ThreadId,
    },
    /// Remove a deleted Task's worktree, only if its checkout is clean;
    /// its branch stays.
    RemoveWorktree {
        path: PathBuf,
    },
    /// Remove a completed Task's worktree now its PR is merged, only if
    /// its checkout is clean (its branch stays), answered with
    /// [`Command::WorktreeTidied`].
    Tidy {
        task: TaskId,
        path: PathBuf,
    },
    /// Read the head commit of `task`'s pull request with the operator's
    /// `gh`, off the pump; answered with [`Command::GitHubHead`] (#223).
    ReadHead {
        task: TaskId,
        repository: String,
        number: u64,
    },
    /// Toast, chime and system notification in the windows.
    Notify {
        task: TaskId,
        project: ProjectId,
        status: Status,
        title: String,
        detail: Option<String>,
    },
    /// Read a GitHub start's matches with the operator's `gh`, off the
    /// pump; answered with [`Command::GitHubRead`].
    ReadGitHub {
        workflow: WorkflowId,
        version: u32,
        query: GitHubQuery,
    },
    /// Start `project`'s Coordinator, owned by its Factory, in its main
    /// checkout, on `agent` (the operator's default when none), and send
    /// it `prompt`; answered with [`Command::CoordinatorStarted`].
    StartCoordinator {
        project: ProjectId,
        agent: Option<Agent>,
        prompt: String,
    },
    /// Bring an open Coordinator's Provider, model and effort to its
    /// settings, from its next turn.
    Tune {
        thread: ThreadId,
    },
}

/// What is written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct State {
    version: u32,
    /// The next Task's number.
    next: u64,
    tasks: Vec<Task>,
    /// The next Workflow's number.
    #[serde(default = "one")]
    next_workflow: u64,
    #[serde(default)]
    workflows: Vec<Workflow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    coordinators: Vec<Coordinator>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    proposals: Vec<Proposal>,
    /// The next proposal's number.
    #[serde(default = "one")]
    next_proposal: u64,
}

fn one() -> u64 {
    1
}

/// A deleted Task's Workers go with it, and only with it (#226); its
/// worktree is removed if its checkout is clean.
fn deleted(task: Task, effects: &mut Vec<Effect>) {
    for thread in task.every_worker() {
        effects.push(Effect::DeleteThread { thread });
    }
    if let Some(path) = task.worktree {
        effects.push(Effect::RemoveWorktree { path });
    }
}

fn unknown_step() -> Refusal {
    Refusal::new(
        RefusalCode::UnknownStep,
        "the workflow has no Step with that id",
    )
}

fn unknown_task() -> Refusal {
    Refusal::new(RefusalCode::UnknownTask, "no Task has that id")
}

fn not_a_worker() -> Refusal {
    Refusal::new(RefusalCode::NotAWorker, "this Thread is no Task's Worker")
}

impl Task {
    /// `thread` is one of its Workers, current or not.
    pub fn had_worker(&self, thread: ThreadId) -> bool {
        self.every_worker().contains(&thread)
    }

    /// What the state it is in says: a result to approve or completed,
    /// or why it needs the operator or stopped.
    pub fn outcome(&self) -> Option<&str> {
        match self.status {
            Status::Completed | Status::Approval => self.result.as_deref(),
            Status::NeedsHuman | Status::Stopped => self.reason.as_deref(),
            Status::Queued | Status::Working => None,
        }
    }

    /// The Step it is at, in the version it keeps.
    pub fn current_step(&self) -> Option<&Step> {
        self.workflow
            .as_ref()?
            .definition
            .step(self.step.as_deref()?)
    }

    fn note(&mut self, at_ms: i64, by: Actor, text: impl Into<String>) {
        let revision = self.revision_now();
        self.activity.push(Entry {
            at_ms,
            by,
            text: text.into(),
            revision,
        });
    }

    /// Back to work, the one way into Working besides the step letting a
    /// Queued Task in. A Task whose Worker's run still goes holds its
    /// place in the limit ([`Task::running`]: Working, or waiting on a
    /// Provider permission) and keeps it; any other waits its turn in
    /// Queued, its Worker and what it is to be told kept.
    fn to_work(&mut self, effects: &mut Vec<Effect>) {
        let status = match self.running() {
            true => Status::Working,
            false => Status::Queued,
        };
        self.enter(status, effects);
    }

    /// The one way into Completed. A Workflow that merges completes only
    /// on a merge its merge Step reported; otherwise the operator is
    /// wanted.
    fn complete(&mut self, effects: &mut Vec<Effect>) {
        let merges = self
            .workflow
            .as_ref()
            .is_some_and(|run| run.definition.merges());
        if merges && !self.delivery.pr.as_ref().is_some_and(|pr| pr.merged) {
            self.reason = Some(
                "Only a confirmed merge completes this workflow, and none was reported: merge it \
                 yourself, or redirect the task to its merge agent."
                    .into(),
            );
            self.enter(Status::NeedsHuman, effects);
            return;
        }
        self.reason = None;
        self.enter(Status::Completed, effects);
    }

    /// Move to `status`, telling the operator when it is one they hear of.
    fn enter(&mut self, status: Status, effects: &mut Vec<Effect>) {
        self.status = status;
        self.watch = Watch::default();
        let title = match status {
            Status::NeedsHuman => "Task needs you",
            Status::Approval => "Task ready to approve",
            Status::Stopped => "Task stopped",
            Status::Completed => "Task completed",
            Status::Queued | Status::Working => return,
        };
        effects.push(Effect::Notify {
            task: self.id.clone(),
            project: self.project,
            status,
            title: format!("{title}: {}", self.title),
            detail: self.outcome().map(str::to_string),
        });
        pr::tidy(self, effects);
    }
}

impl State {
    fn task_mut(&mut self, id: &str) -> Result<&mut Task, Refusal> {
        self.tasks
            .iter_mut()
            .find(|task| task.id == id)
            .ok_or_else(unknown_task)
    }

    fn worker_task(&mut self, worker: ThreadId) -> Result<&mut Task, Refusal> {
        self.tasks
            .iter_mut()
            .find(|task| task.had_worker(worker))
            .ok_or_else(not_a_worker)
    }

    /// The Task `worker`'s report is for, when it counts: it is the
    /// Task's current Worker and its run still goes. Any other Worker's
    /// (superseded, replaced, at another Step, or after the Task stopped)
    /// is dropped before anything is touched.
    fn reporting(&mut self, worker: ThreadId) -> Result<Option<&mut Task>, Refusal> {
        let task = self.worker_task(worker)?;
        Ok((task.running() && task.worker == Some(worker)).then_some(task))
    }

    fn workflow_mut(&mut self, id: &str) -> Result<&mut Workflow, Refusal> {
        self.workflows
            .iter_mut()
            .find(|workflow| workflow.id == id)
            .ok_or_else(|| Refusal::new(RefusalCode::UnknownWorkflow, "no Workflow has that id"))
    }

    fn draft_step(&mut self, workflow: &str, step: &str) -> Result<&mut Step, Refusal> {
        self.workflow_mut(workflow)?
            .draft
            .step_mut(step)
            .ok_or_else(unknown_step)
    }

    pub(super) fn create_workflow(
        &mut self,
        project: ProjectId,
        template: Template,
        agent: Agent,
    ) -> Result<Answer, Refusal> {
        let id = format!("w-{}", self.next_workflow);
        self.next_workflow += 1;
        let mut made = Workflow {
            id: id.clone(),
            project,
            draft: Definition {
                name: String::new(),
                steps: Vec::new(),
                approval: Approval::default(),
                start: Start::default(),
            },
            versions: Vec::new(),
            next_step: 1,
            automation: Automation::default(),
        };
        workflow::from_template(template, &mut made, &agent);
        self.workflows.push(made);
        Ok(Answer::Workflow(id))
    }

    pub(super) fn rename_workflow(
        &mut self,
        workflow: WorkflowId,
        name: String,
    ) -> Result<Answer, Refusal> {
        let draft = &mut self.workflow_mut(&workflow)?.draft;
        let changed = draft.name != name;
        draft.name = name;
        Ok(Answer::Changed(changed))
    }

    pub(super) fn add_step(
        &mut self,
        workflow: WorkflowId,
        after: Option<StepId>,
        agent: Agent,
    ) -> Result<Answer, Refusal> {
        let made = self.workflow_mut(&workflow)?;
        let at = match &after {
            Some(after) => {
                made.draft
                    .steps
                    .iter()
                    .position(|step| step.id == *after)
                    .ok_or_else(unknown_step)?
                    + 1
            }
            None => made.draft.steps.len(),
        };
        let id = made.new_step_id();
        made.draft
            .steps
            .insert(at, workflow::new_step(id.clone(), "New agent", agent));
        Ok(Answer::Step(id))
    }

    pub(super) fn edit_step(
        &mut self,
        workflow: WorkflowId,
        step: StepId,
        change: StepChange,
    ) -> Result<Answer, Refusal> {
        let step = self.draft_step(&workflow, &step)?;
        let before = step.clone();
        change.apply(step);
        Ok(Answer::Changed(*step != before))
    }

    pub(super) fn move_step(
        &mut self,
        workflow: WorkflowId,
        step: StepId,
        to: usize,
    ) -> Result<Answer, Refusal> {
        let steps = &mut self.workflow_mut(&workflow)?.draft.steps;
        let from = steps
            .iter()
            .position(|each| each.id == step)
            .ok_or_else(unknown_step)?;
        let to = to.min(steps.len() - 1);
        let moved = steps.remove(from);
        steps.insert(to, moved);
        Ok(Answer::Changed(from != to))
    }

    pub(super) fn delete_step(
        &mut self,
        workflow: WorkflowId,
        step: StepId,
    ) -> Result<Answer, Refusal> {
        let steps = &mut self.workflow_mut(&workflow)?.draft.steps;
        let at = steps
            .iter()
            .position(|each| each.id == step)
            .ok_or_else(unknown_step)?;
        steps.remove(at);
        Ok(Answer::Changed(true))
    }

    pub(super) fn edit_approval(
        &mut self,
        workflow: WorkflowId,
        change: ApprovalChange,
    ) -> Result<Answer, Refusal> {
        let approval = &mut self.workflow_mut(&workflow)?.draft.approval;
        let before = approval.clone();
        change.apply(approval);
        Ok(Answer::Changed(*approval != before))
    }

    pub(super) fn save_workflow(
        &mut self,
        workflow: WorkflowId,
        catalog: Catalog,
        now_ms: i64,
    ) -> Result<Answer, Refusal> {
        let made = self.workflow_mut(&workflow)?;
        let problems = made.draft.problems(&catalog);
        if !problems.is_empty() {
            let listed: Vec<String> = problems
                .into_iter()
                .map(|problem| problem.message)
                .collect();
            return Err(Refusal::new(
                RefusalCode::Invalid,
                format!("the workflow cannot be saved: {}", listed.join(" ")),
            ));
        }
        if !made.unsaved() {
            return Ok(Answer::Version(made.latest().map_or(0, |v| v.version)));
        }
        // A new start is a new setup: it is never live until the
        // operator turns it on.
        if made
            .latest()
            .is_some_and(|latest| latest.definition.start != made.draft.start)
        {
            made.automation.turn_off();
        }
        let version = made.latest().map_or(1, |latest| latest.version + 1);
        made.versions.push(Version {
            version,
            saved_ms: now_ms,
            definition: made.draft.clone(),
        });
        Ok(Answer::Version(version))
    }

    /// A Task with `instructions`, Queued, or the one `request` already
    /// started.
    #[allow(clippy::too_many_arguments)]
    fn start(
        &mut self,
        project: ProjectId,
        instructions: String,
        agent: Agent,
        request: Option<String>,
        workflow: Option<WorkflowId>,
        pr: Option<String>,
        now_ms: i64,
    ) -> Result<TaskId, Refusal> {
        if let Some(task) = request.as_ref().and_then(|request| {
            self.tasks
                .iter()
                .find(|task| task.request.as_ref() == Some(request))
        }) {
            return Ok(task.id.clone());
        }
        let instructions = instructions.trim().to_string();
        let Some(title) = instructions.lines().next().map(str::trim) else {
            return Err(Refusal::new(
                RefusalCode::Invalid,
                "a Task needs instructions",
            ));
        };
        let pr = pr::given(pr)?;
        let run = match workflow {
            None => None,
            Some(id) => {
                let made = self
                    .workflows
                    .iter()
                    .find(|made| made.id == id)
                    .ok_or_else(|| {
                        Refusal::new(RefusalCode::UnknownWorkflow, "no Workflow has that id")
                    })?;
                if made.project != project {
                    return Err(Refusal::new(
                        RefusalCode::Invalid,
                        "that Workflow is another Project's",
                    ));
                }
                let version = made.latest().ok_or_else(|| {
                    Refusal::new(
                        RefusalCode::Invalid,
                        "the workflow has no saved version: save it first",
                    )
                })?;
                Some(Run {
                    id: made.id.clone(),
                    version: version.version,
                    definition: version.definition.clone(),
                })
            }
        };
        let first = run
            .as_ref()
            .and_then(|run| run.definition.steps.first().cloned());
        let id = format!("t-{}", self.next);
        self.next += 1;
        let mut task = Task {
            title: title.to_string(),
            branch: format!("factory/{id}"),
            id: id.clone(),
            project,
            instructions,
            status: Status::Queued,
            agent: first.as_ref().map_or(agent, |step| step.agent.clone()),
            worker: None,
            worktree: None,
            result: None,
            reason: None,
            activity: Vec::new(),
            created_ms: now_ms,
            request,
            watch: Watch::default(),
            step: first.map(|step| step.id),
            workflow: run,
            results: Vec::new(),
            workers: Vec::new(),
            oversight: Oversight::default(),
            source: None,
            delivery: Delivery {
                pr,
                ..Delivery::default()
            },
            keeping: Keeping::default(),
        };
        let started = match &task.workflow {
            Some(run) => format!(
                "Started the task on {} (version {})",
                run.definition.name, run.version
            ),
            None => "Started the task".into(),
        };
        let started = match &task.delivery.pr {
            Some(pr) => format!("{started}, on {}", pr.label()),
            None => started,
        };
        task.note(now_ms, Actor::Operator, started);
        self.tasks.push(task);
        Ok(id)
    }

    fn command(
        &mut self,
        command: Command,
        now_ms: i64,
        effects: &mut Vec<Effect>,
    ) -> Result<Answer, Refusal> {
        match command {
            Command::Start {
                project,
                instructions,
                agent,
                request,
                workflow,
                pr,
            } => self
                .start(project, instructions, agent, request, workflow, pr, now_ms)
                .map(Answer::Task),
            Command::WorkerStarted {
                task,
                thread,
                worktree,
            } => {
                let task = self.task_mut(&task)?;
                if task.worker == Some(thread) {
                    return Ok(Answer::Changed(false));
                }
                // A Worker this one replaces stays in the Task's history.
                let step = task.step.clone();
                let replaced: Vec<ThreadId> = task
                    .worker
                    .into_iter()
                    .chain(
                        task.workers
                            .iter()
                            .filter(|each| Some(&each.step) == step.as_ref())
                            .map(|each| each.thread),
                    )
                    .collect();
                for old in replaced {
                    task.retire(old, effects);
                }
                task.worker = Some(thread);
                if task.worktree.is_none() {
                    task.worktree = worktree;
                }
                if let Some(step) = step {
                    task.workers.push(StepWorker { step, thread });
                }
                Ok(Answer::Changed(true))
            }
            Command::WorkerFailed { task, why } => {
                let task = self.task_mut(&task)?;
                if task.status != Status::Working || task.worker.is_some() {
                    return Ok(Answer::Changed(false));
                }
                task.note(
                    now_ms,
                    Actor::Ferrite,
                    format!("Could not start the agent: {why}"),
                );
                task.reason = Some(format!("The agent could not start: {why}"));
                task.enter(Status::Stopped, effects);
                Ok(Answer::Changed(true))
            }
            Command::Report { worker, outcome } => {
                let Some(task) = self.reporting(worker)? else {
                    return Ok(Answer::Changed(false));
                };
                if task.workflow.is_some() {
                    direct::reported(task, outcome, now_ms, effects)?;
                    return Ok(Answer::Changed(true));
                }
                task.oversight.permission = None;
                match outcome {
                    Outcome::Finished { summary } => {
                        task.note(now_ms, Actor::Agent, format!("Finished: {summary}"));
                        task.evidence(
                            EvidenceKind::Conclusion,
                            "The agent",
                            format!("Finished: {summary}"),
                            now_ms,
                        );
                        task.result = Some(summary);
                        task.complete(effects);
                    }
                    Outcome::Unable { tried } => {
                        task.note(now_ms, Actor::Agent, format!("Unable to finish: {tried}"));
                        task.evidence(
                            EvidenceKind::Conclusion,
                            "The agent",
                            format!("Could not finish: {tried}"),
                            now_ms,
                        );
                        task.reason = Some(tried);
                        task.enter(Status::NeedsHuman, effects);
                    }
                    Outcome::Approved { .. } | Outcome::Rejected { .. } | Outcome::Merge { .. } => {
                        return Err(Refusal::new(
                            RefusalCode::Invalid,
                            "this agent is not deciding an approval: report finished or unable",
                        ))
                    }
                }
                Ok(Answer::Changed(true))
            }
            Command::OperatorSaid { worker, text } => {
                // Recorded, so later Steps are told; nothing more. Resume
                // and Answer are the operator's commands for the rest.
                direct::operator_said(self.worker_task(worker)?, worker, text, now_ms);
                Ok(Answer::Changed(true))
            }
            Command::Stop { task } => {
                let task = self.task_mut(&task)?;
                if !task.status.stoppable() {
                    return Ok(Answer::Changed(false));
                }
                task.note(now_ms, Actor::Operator, "Stopped the task");
                task.reason = Some("Stopped by you".into());
                task.oversight.question = None;
                task.oversight.permission = None;
                if let Some(thread) = task.worker {
                    effects.push(Effect::Interrupt { thread });
                }
                task.enter(Status::Stopped, effects);
                Ok(Answer::Changed(true))
            }
            Command::Resume { task } => {
                let task = self.task_mut(&task)?;
                if task.status != Status::Stopped {
                    return Ok(Answer::Changed(false));
                }
                task.note(now_ms, Actor::Operator, "Resumed the task");
                task.reason = None;
                task.keeping.revived_ms = None;
                task.to_work(effects);
                // With no Worker yet, the step starts one when it is let in.
                if task.worker.is_some() {
                    task.watch.prompt = Some(resume_prompt(task));
                }
                Ok(Answer::Changed(true))
            }
            Command::Delete { task } => {
                let at = self
                    .tasks
                    .iter()
                    .position(|candidate| candidate.id == task)
                    .ok_or_else(unknown_task)?;
                let status = self.tasks[at].status;
                if !status.deletable() {
                    return Err(Refusal::new(
                        RefusalCode::NotNow,
                        format!(
                            "a Task that is {} cannot be deleted: stop it first",
                            status.word().to_lowercase()
                        ),
                    ));
                }
                deleted(self.tasks.remove(at), effects);
                Ok(Answer::Changed(true))
            }
            Command::Ask { worker, text } => match self.reporting(worker)? {
                Some(task) => direct::ask(task, worker, text, now_ms, effects),
                None => Ok(Answer::Changed(false)),
            },
            Command::AnswerQuestion {
                task,
                text,
                question,
            } => direct::answer(self.task_mut(&task)?, text, question, now_ms, effects),
            Command::Approve { task } => {
                direct::approve_command(self.task_mut(&task)?, now_ms, effects)
            }
            Command::Rework { task, feedback, to } => {
                direct::rework(self.task_mut(&task)?, feedback, to, now_ms, effects)
            }
            Command::Redirect { task, direction } => {
                direct::redirect(self.task_mut(&task)?, direction, now_ms, effects)
            }
            Command::ReportRevision { worker, head, pr } => match self.reporting(worker)? {
                Some(task) => pr::reported_revision(task, head, pr, now_ms),
                None => Ok(Answer::Changed(false)),
            },
            Command::ReportEvidence { worker, text } => match self.reporting(worker)? {
                Some(task) => pr::reported_evidence(task, text, now_ms),
                None => Ok(Answer::Changed(false)),
            },
            Command::WorktreeTidied { task, kept } => {
                pr::tidied(self.task_mut(&task)?, kept, now_ms)
            }
            Command::Clear { project } => Ok(self.clear(project, now_ms)),
            Command::GitHubHead { task, head } => {
                Ok(pr::read_head(self.task_mut(&task)?, head, now_ms))
            }
            Command::CreateWorkflow {
                project,
                template,
                agent,
            } => self.create_workflow(project, template, agent),
            Command::RenameWorkflow { workflow, name } => self.rename_workflow(workflow, name),
            Command::AddStep {
                workflow,
                after,
                agent,
            } => self.add_step(workflow, after, agent),
            Command::EditStep {
                workflow,
                step,
                change,
            } => self.edit_step(workflow, step, change),
            Command::MoveStep { workflow, step, to } => self.move_step(workflow, step, to),
            Command::DeleteStep { workflow, step } => self.delete_step(workflow, step),
            Command::EditApproval { workflow, change } => self.edit_approval(workflow, change),
            Command::SaveWorkflow { workflow, catalog } => {
                self.save_workflow(workflow, catalog, now_ms)
            }
            Command::EditStart { workflow, start } => self.edit_start(workflow, start),
            Command::Automation { workflow, turn } => self.automation(workflow, turn, now_ms),
            Command::Backlog { workflow, take } => self.backlog(workflow, take, now_ms),
            Command::GitHubRead {
                workflow,
                version,
                result,
            } => self.github_read(workflow, version, result, now_ms),
            Command::Submit {
                workflow,
                identity,
                instructions,
            } => self.submit(workflow, identity, instructions, now_ms),
            Command::CoordinatorSay { project, text } => {
                self.coordinator_say(project, text, effects)
            }
            Command::CoordinatorStarted { project, thread } => {
                self.coordinator_started(project, thread, effects)
            }
            Command::CoordinatorFailed { project, why } => self.coordinator_failed(project, why),
            Command::CoordinatorAgent { project, agent } => {
                self.coordinator_agent(project, agent, effects)
            }
            Command::Propose {
                coordinator,
                act,
                catalog,
            } => self.propose(coordinator, act, catalog, now_ms),
            Command::AcceptProposal {
                proposal,
                agent,
                catalog,
            } => self.accept_proposal(proposal, agent, catalog, now_ms, effects),
            Command::DismissProposal { proposal } => self.dismiss_proposal(proposal),
        }
    }

    /// The first half of a pass of this Project's part, in place, before
    /// the limit is shared out: what the restart caught, and what the
    /// schedules start. Whether it changed.
    fn pass_starts(
        &mut self,
        now_ms: i64,
        unstarted: &std::collections::BTreeSet<TaskId>,
        effects: &mut Vec<Effect>,
    ) -> bool {
        let changed = recovery::unstarted(self, unstarted, now_ms, effects);
        self.step_starts(now_ms) || changed
    }

    /// The rest of the pass: the Tasks let in start, and every Worker is
    /// looked at. Whether it changed.
    fn pass(
        &mut self,
        now_ms: i64,
        workers: &BTreeMap<ThreadId, Seen>,
        admit: &std::collections::BTreeSet<TaskId>,
        restarted: &std::collections::BTreeSet<ThreadId>,
        effects: &mut Vec<Effect>,
    ) -> bool {
        let mut changed = self.admit(admit, now_ms, effects);
        changed |= self.step(now_ms, workers, restarted, effects);
        changed |= self.step_archive(now_ms, workers, effects);
        changed |= self.step_coordinators(effects);
        changed
    }

    /// Let the Queued Tasks the limit has room for into Working: a new one
    /// starts its Worker; one back to work keeps its Worker, which is told
    /// what it was to be told.
    fn admit(
        &mut self,
        admit: &std::collections::BTreeSet<TaskId>,
        now_ms: i64,
        effects: &mut Vec<Effect>,
    ) -> bool {
        let mut changed = false;
        for task in &mut self.tasks {
            if task.status != Status::Queued || !admit.contains(&task.id) {
                continue;
            }
            changed = true;
            pr::read_when_started(task, effects);
            if task.worker.is_some() {
                let prompt = task.watch.prompt.take();
                task.enter(Status::Working, effects);
                task.watch.prompt = prompt;
                continue;
            }
            match task.current_step().cloned() {
                Some(step) => start_step(task, &step, now_ms, effects),
                None => {
                    task.note(
                        now_ms,
                        Actor::Ferrite,
                        format!("Started an agent on {}", agent_label(&task.agent)),
                    );
                    effects.push(Effect::StartWorker {
                        task: task.id.clone(),
                        project: task.project,
                        agent: task.agent.clone(),
                        branch: task.branch.clone(),
                        worktree: task.worktree.clone(),
                        prompt: first_prompt(task),
                    });
                }
            }
            task.enter(Status::Working, effects);
        }
        changed
    }

    /// Each working Worker the Server sees: lost, asking permission, or
    /// at rest. Whether any Task changed.
    fn step(
        &mut self,
        now_ms: i64,
        workers: &BTreeMap<ThreadId, Seen>,
        restarted: &std::collections::BTreeSet<ThreadId>,
        effects: &mut Vec<Effect>,
    ) -> bool {
        let mut changed = false;
        for task in &mut self.tasks {
            if !matches!(task.status, Status::Working | Status::NeedsHuman) {
                continue;
            }
            let Some(seen) = task.worker.and_then(|thread| workers.get(&thread)) else {
                continue;
            };
            let cause = match task
                .worker
                .is_some_and(|thread| restarted.contains(&thread))
            {
                true => recovery::Cause::Restart,
                false => recovery::Cause::Ended,
            };
            if recovery::closed(task, seen, now_ms, effects)
                || recovery::lost(task, seen, cause, now_ms, effects)
                || direct::permission(task, seen.decision.as_deref(), seen.turns, now_ms, effects)
            {
                changed = true;
                continue;
            }
            if task.status == Status::Working {
                changed |= at_rest_rule(task, seen, now_ms, effects);
            }
        }
        changed
    }
}

/// Move the Task to `step` and run it: its earlier Worker continues
/// (unless the Step starts fresh every time), else a new one starts.
/// `deciding`, the run decides an approval.
fn enter_step(
    task: &mut Task,
    step: &Step,
    deciding: bool,
    now_ms: i64,
    effects: &mut Vec<Effect>,
) {
    task.step = Some(step.id.clone());
    task.agent = step.agent.clone();
    task.reason = None;
    task.keeping.revived_ms = None;
    task.oversight.deciding = deciding;
    task.to_work(effects);
    let earlier = task
        .workers
        .iter()
        .find(|each| each.step == step.id)
        .map(|each| each.thread);
    match earlier {
        Some(thread) if !step.fresh => {
            task.note(
                now_ms,
                Actor::Ferrite,
                format!("Back to {}: its agent continues", step.name),
            );
            task.worker = Some(thread);
            task.watch.prompt = Some(return_prompt(task, step));
        }
        Some(old) => {
            task.retire(old, effects);
            begin_step(task, step, now_ms, effects);
        }
        None => begin_step(task, step, now_ms, effects),
    }
}

/// A new Worker for `step` now, or, while the Task waits its turn, when
/// the step lets it in.
fn begin_step(task: &mut Task, step: &Step, now_ms: i64, effects: &mut Vec<Effect>) {
    match task.status {
        Status::Queued => task.worker = None,
        _ => start_step(task, step, now_ms, effects),
    }
}

/// Start a new Worker for `step` in the Task's worktree (or a new one).
fn start_step(task: &mut Task, step: &Step, now_ms: i64, effects: &mut Vec<Effect>) {
    task.worker = None;
    task.note(
        now_ms,
        Actor::Ferrite,
        format!("Started {} on {}", step.name, agent_label(&step.agent)),
    );
    effects.push(Effect::StartWorker {
        task: task.id.clone(),
        project: task.project,
        agent: step.agent.clone(),
        branch: task.branch.clone(),
        worktree: task.worktree.clone(),
        prompt: step_prompt(task, step),
    });
}

/// A Worker is at rest when its turn has ended and nothing of its Session
/// still runs. At rest without a report once the turn it was prompted
/// for has ended, it is asked once to report; at rest again after that
/// turn (or still, after a grace), its Task needs the operator. A busy
/// Worker is never asked. Whether the Task changed.
fn at_rest_rule(task: &mut Task, seen: &Seen, now_ms: i64, effects: &mut Vec<Effect>) -> bool {
    let Some(thread) = task.worker else {
        return false;
    };
    if task.watch.at_rest && seen.busy {
        return false;
    }
    if let Some(text) = task.watch.prompt.take() {
        task.watch.at_rest = false;
        effects.push(Effect::Prompt { thread, text });
        task.watch.baseline = seen.turns;
        task.watch.nudged_ms = None;
        return true;
    }
    if std::mem::take(&mut task.watch.rebase) {
        task.watch.baseline = seen.turns;
        return true;
    }
    if seen.busy {
        return false;
    }
    let ended = seen.turns > task.watch.baseline;
    match task.watch.nudged_ms {
        None if ended => {
            task.note(
                now_ms,
                Actor::Ferrite,
                "The agent stopped without reporting; asked it to report",
            );
            effects.push(Effect::Prompt {
                thread,
                text: nudge_prompt(task),
            });
            task.watch.baseline = seen.turns;
            task.watch.nudged_ms = Some(now_ms);
            true
        }
        Some(nudged) if ended || now_ms - nudged >= NUDGE_GRACE_MS => {
            task.note(
                now_ms,
                Actor::Ferrite,
                "The agent stopped again without reporting",
            );
            task.reason = Some("The agent stopped without reporting an outcome.".into());
            task.enter(Status::NeedsHuman, effects);
            true
        }
        _ => false,
    }
}

/// A Factory answer as a window's reply.
pub fn done(answer: Answer) -> crate::protocol::Done {
    use crate::protocol::Done;
    match answer {
        Answer::Task(task) | Answer::Duplicate(task) => Done::Task { task },
        Answer::Changed(changed) => Done::Changed { changed },
        Answer::Workflow(workflow) => Done::Workflow { workflow },
        Answer::Step(step) => Done::Step { step },
        Answer::Version(version) => Done::Version { version },
        Answer::Proposal(proposal) => Done::Proposal { proposal },
    }
}

/// A window's authoring ask as the command it is: new Workflows and
/// Steps on `agent`; a save checked against `models` and the skills the
/// window found. `None` for an ask this Ferrite does not know.
pub fn workflow_command(
    action: crate::protocol::WorkflowAction,
    agent: &Agent,
    models: Vec<(Provider, Vec<crate::ModelInfo>)>,
) -> Option<Command> {
    use crate::protocol::WorkflowAction;
    Some(match action {
        WorkflowAction::Create { project, template } => Command::CreateWorkflow {
            project,
            template,
            agent: agent.clone(),
        },
        WorkflowAction::Rename { workflow, name } => Command::RenameWorkflow { workflow, name },
        WorkflowAction::AddStep { workflow, after } => Command::AddStep {
            workflow,
            after,
            agent: agent.clone(),
        },
        WorkflowAction::EditStep {
            workflow,
            step,
            change,
        } => Command::EditStep {
            workflow,
            step,
            change,
        },
        WorkflowAction::MoveStep { workflow, step, to } => Command::MoveStep { workflow, step, to },
        WorkflowAction::DeleteStep { workflow, step } => Command::DeleteStep { workflow, step },
        WorkflowAction::Save { workflow, skills } => Command::SaveWorkflow {
            workflow,
            catalog: Catalog { models, skills },
        },
        WorkflowAction::EditApproval { workflow, change } => {
            Command::EditApproval { workflow, change }
        }
        WorkflowAction::EditStart { workflow, start } => Command::EditStart { workflow, start },
        WorkflowAction::Automation { workflow, turn } => Command::Automation { workflow, turn },
        WorkflowAction::Backlog { workflow, take } => Command::Backlog { workflow, take },
        WorkflowAction::Unknown => return None,
    })
}

/// A window's ask of one Task as the command it is; `None` for one that
/// is not of one Task in place (start, delete) or that this Ferrite does
/// not know.
pub fn task_command(action: crate::protocol::TaskAction) -> Option<Command> {
    use crate::protocol::TaskAction;
    Some(match action {
        TaskAction::Stop { task } => Command::Stop { task },
        TaskAction::Resume { task } => Command::Resume { task },
        TaskAction::Answer {
            task,
            text,
            question,
        } => Command::AnswerQuestion {
            task,
            text,
            question,
        },
        TaskAction::Approve { task } => Command::Approve { task },
        TaskAction::Rework { task, feedback, to } => Command::Rework { task, feedback, to },
        TaskAction::Redirect { task, direction } => Command::Redirect { task, direction },
        TaskAction::Clear { project } => Command::Clear { project },
        TaskAction::Start { .. } | TaskAction::Delete { .. } | TaskAction::Unknown => return None,
    })
}

/// `Claude · claude-opus-4`, as a card names a Worker's.
pub fn agent_label(agent: &Agent) -> String {
    let provider = agent.provider;
    match &agent.model {
        Some(model) => format!("{provider} · {model}"),
        None => provider.to_string(),
    }
}

/// How a Worker is told to report, once: with its own tools, which need no
/// permission, and by the command only when they are not offered. Every
/// other text names a report by its tool and its outcome, never by command.
const TOOLS: &str = "Report with Ferrite's own tools, which need no permission: \
     `report_outcome` (`outcome`, `summary`), `report_question` (`text`), `report_revision` \
     (`head`, `pr`) and `report_evidence` (`text`). Only these reports count, never your \
     messages. Run the same report as a command only if the tools are not offered to you: \
     `\"${FERRITE_BIN:-ferrite}\" report outcome <outcome> \"<summary>\"`, and likewise \
     `report question`, `report revision` and `report evidence`.";

const REPORTING: &str = "When you are done, call `report_outcome` with outcome `finished` and one \
     line: what you did. If you cannot finish, call it with outcome `unable` and what you tried \
     and what stopped you.";

const ASKING: &str = "If you need the operator to decide something before you can go on, ask \
     them with `report_question` and end your turn; the task waits for their answer.";

fn first_prompt(task: &Task) -> String {
    format!(
        "You are working on Ferrite Factory task {id}, in the task's own worktree on branch \
         `{branch}`.\n\n{instructions}\n{pr}\n{reporting}",
        id = task.id,
        branch = task.branch,
        instructions = task.instructions,
        pr = if task.delivery.pr.is_some() {
            pr::context(task)
        } else {
            String::new()
        },
        reporting = one_off_reporting(),
    )
}

fn one_off_reporting() -> String {
    format!("{TOOLS}\n\n{REPORTING}\n\n{ASKING}")
}

/// How the current run is to report: its outcome, or (deciding an
/// approval) its decision.
fn reporting(task: &Task, step: &Step) -> String {
    format!("{TOOLS}\n\n{}", reporting_sections(task, step))
}

fn reporting_sections(task: &Task, step: &Step) -> String {
    if task.oversight.deciding {
        format!(
            "{}\nIf you cannot decide, call `report_outcome` with outcome `unable` and what \
             stopped you, and the operator decides.",
            direct::decision_section(task)
        )
    } else if task.decides_in_own_run(step) {
        format!(
            "When your work is done, you also decide its approval.\n\n{}\nIf you cannot \
             finish, call `report_outcome` with outcome `unable` and what you tried.",
            direct::decision_section(task)
        )
    } else if step.outcome_routes.is_some() {
        format!("{}\n{ASKING}", pr::merge_reporting(task))
    } else {
        format!("{REPORTING}\n\n{ASKING}")
    }
}

fn said(result: &StepResult) -> String {
    match &result.outcome {
        Outcome::Finished { summary } => format!("- {} finished: {summary}", result.name),
        Outcome::Unable { tried } => format!("- {} could not finish: {tried}", result.name),
        Outcome::Approved { summary } => format!("- {} approved: {summary}", result.name),
        Outcome::Rejected { reasons } => format!("- {} did not approve: {reasons}", result.name),
        Outcome::Merge {
            result: merge,
            summary,
        } => format!(
            "- {} reported {}: {summary}",
            result.name,
            merge.label().to_lowercase()
        ),
    }
}

/// A Step's Worker's first prompt: who it is, its instructions and
/// skills, the Task's request, what the Steps before it reported, the
/// operator's answers and feedback, and how to report.
fn step_prompt(task: &Task, step: &Step) -> String {
    let mut prompt = format!(
        "You are the \"{name}\" agent on Ferrite Factory task {id}, working in the task's own \
         worktree on branch `{branch}`, which the task's other agents share.\n\n\
         ## Your instructions\n\n{instructions}\n",
        name = step.name,
        id = task.id,
        branch = task.branch,
        instructions = step.instructions.trim(),
    );
    if !step.skills.is_empty() {
        prompt.push_str(&format!(
            "\n## Required skills\n\nUse these skills: {}\n",
            step.skills.join(", ")
        ));
    }
    prompt.push_str(&format!("\n## The task\n\n{}\n", task.instructions));
    if !task.results.is_empty() {
        let earlier: Vec<String> = task.results.iter().map(said).collect();
        prompt.push_str(&format!("\n## Earlier results\n\n{}\n", earlier.join("\n")));
    }
    prompt.push_str(&pr::context(task));
    prompt.push_str(&direct::said_since(task, 0, true));
    prompt.push_str(&pr::reconcile(task, step));
    prompt.push('\n');
    prompt.push_str(&reporting(task, step));
    prompt
}

/// What a continued Worker is told when the Task returns to its Step:
/// what the other Steps reported and the feedback given since it last
/// reported.
fn return_prompt(task: &Task, step: &Step) -> String {
    let last_own = task
        .results
        .iter()
        .rposition(|result| result.step == step.id)
        .map_or(0, |at| at + 1);
    let news: Vec<String> = task.results[last_own..].iter().map(said).collect();
    let news = if news.is_empty() {
        "- Nothing new was reported.".to_string()
    } else {
        news.join("\n")
    };
    format!(
        "The task is back with you, \"{name}\". Since you last reported:\n\n{news}\n{said}{pr}\n\
         Continue with your instructions in light of that.\n\n{reporting}",
        name = step.name,
        said = direct::said_since(task, last_own, false),
        pr = if task.delivery.pr.is_some() {
            format!("{}{}", pr::context(task), pr::reconcile(task, step))
        } else {
            String::new()
        },
        reporting = reporting(task, step),
    )
}

/// What the asker is told once the operator answers.
fn answer_prompt(task: &Task, question: &str, answer: &str) -> String {
    let reporting = reporting_now(task);
    format!(
        "The operator answered your question.\n\nYou asked: {question}\n\nTheir answer: \
         {answer}\n\nContinue with your instructions in light of that.\n\n{reporting}"
    )
}

/// How the Task's current run reports, whatever it is: a Step's outcome,
/// a merge result or an approval decision, or a one-off's outcome.
fn reporting_now(task: &Task) -> String {
    match task.current_step() {
        Some(step) => reporting(task, step),
        None => one_off_reporting(),
    }
}

fn nudge_prompt(task: &Task) -> String {
    format!(
        "Your turn ended without reporting the task's outcome. Report it now.\n\n{}",
        reporting_now(task)
    )
}

/// A merge Step is told to reconcile an unconfirmed result first (#223).
fn resume_prompt(task: &Task) -> String {
    let reconcile = task
        .current_step()
        .map(|step| pr::reconcile(task, step))
        .unwrap_or_default();
    format!(
        "The operator resumed this task. Continue where you left off.\n{reconcile}\n{}",
        reporting_now(task)
    )
}

#[cfg(test)]
mod tests;
