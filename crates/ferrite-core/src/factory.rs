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
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::Provider;
use crate::workspace::registry::ProjectId;
use crate::ThreadId;

mod direct;
mod pr;
mod starts;
mod workflow;

pub use direct::{Answered, Feedback, FeedbackKind, Override, Oversight, Question, ThenReturn};
pub use pr::{
    evidence_line, short, Delivery, Evidence, EvidenceKind, MergeOutcome, MergeRoutes, PullRequest,
    Revision,
};
pub use starts::{when_label, zone_label};
pub use starts::{
    Automation, GitHubItem, GitHubQuery, GitHubRead, GitHubSource, Missed, Overlap, Recurrence,
    Schedule, Source, Start, StartKind, Switch, Trigger, Turn, INTERVAL_HOURS, POLL_MS, WEEKDAYS,
};
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
}

impl Actor {
    pub fn word(self) -> &'static str {
        match self {
            Actor::Ferrite => "Ferrite",
            Actor::Agent => "Agent",
            Actor::Operator => "You",
            Actor::Tool => "Tool",
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
}

impl Board {
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

    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Task(TaskId),
    Workflow(WorkflowId),
    Step(StepId),
    /// The version a save made, or the latest when nothing changed.
    Version(u32),
    /// Whether the command changed anything: a repeat does not.
    Changed(bool),
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
    /// Toast, chime and system notification in the windows.
    Notify {
        task: TaskId,
        project: ProjectId,
        status: Status,
        title: String,
    },
    /// Read a GitHub start's matches with the operator's `gh`, off the
    /// pump; answered with [`Command::GitHubRead`].
    ReadGitHub {
        workflow: WorkflowId,
        version: u32,
        query: GitHubQuery,
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
}

fn one() -> u64 {
    1
}

pub struct Factory {
    state: State,
    /// Where it is written; `None` keeps it in memory (tests).
    path: Option<PathBuf>,
    revision: u64,
    /// Effects decided by commands, handed out by the next step.
    pending: Vec<Effect>,
    /// How many Tasks may be Working at once (Settings; 3 by default).
    limit: usize,
    /// Each GitHub start's reads, kept only while the Server runs.
    polls: BTreeMap<WorkflowId, starts::Poll>,
}

impl Factory {
    pub fn in_memory() -> Self {
        Self {
            state: State {
                version: SCHEMA_VERSION,
                next: 1,
                tasks: Vec::new(),
                next_workflow: 1,
                workflows: Vec::new(),
            },
            path: None,
            revision: 0,
            pending: Vec::new(),
            limit: DEFAULT_LIMIT,
            polls: BTreeMap::new(),
        }
    }

    /// The Factory written at `path`, or a fresh one when there is none.
    /// One written by a newer Ferrite is refused rather than overwritten.
    pub fn load(path: &Path) -> io::Result<Self> {
        let mut factory = Self::in_memory();
        match std::fs::read(path) {
            Ok(bytes) => {
                let state: State = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                if state.version > SCHEMA_VERSION {
                    return Err(io::Error::other(format!(
                        "{} is from a newer Ferrite (version {})",
                        path.display(),
                        state.version
                    )));
                }
                factory.state = state;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        factory.path = Some(path.to_path_buf());
        Ok(factory)
    }

    /// One change, whoever asks. Its effects come out of the next step.
    pub fn command(&mut self, command: Command, now_ms: i64) -> Result<Answer, Refusal> {
        self.heard(&command);
        self.change(|state, effects| state.command(command, now_ms, effects))
    }

    /// One pass: what to do now, given the time and what each Worker's
    /// Thread is doing. A Worker the Server does not see (no open Thread)
    /// is left as it is.
    pub fn step(&mut self, now_ms: i64, workers: &BTreeMap<ThreadId, Seen>) -> Vec<Effect> {
        // A step that cannot be written decides nothing; the next pass
        // tries again.
        let limit = self.limit;
        let _ = self.change(|state, effects| {
            state.step_starts(now_ms, effects);
            state.step(now_ms, workers, limit, effects);
            Ok(())
        });
        self.poll(now_ms);
        std::mem::take(&mut self.pending)
    }

    /// Apply `change` to a copy, write the copy, and only then take it and
    /// its effects: a change that cannot be written is not made.
    fn change<T>(
        &mut self,
        change: impl FnOnce(&mut State, &mut Vec<Effect>) -> Result<T, Refusal>,
    ) -> Result<T, Refusal> {
        let mut next = self.state.clone();
        let mut effects = Vec::new();
        let answer = change(&mut next, &mut effects)?;
        if next != self.state {
            if let Some(path) = &self.path {
                let bytes = serde_json::to_vec_pretty(&next)
                    .map_err(|error| Refusal::new(RefusalCode::Failed, error.to_string()))?;
                crate::store::replace_file(path, &bytes).map_err(|error| {
                    Refusal::new(
                        RefusalCode::Failed,
                        format!("the Factory could not be saved: {error}"),
                    )
                })?;
            }
            self.state = next;
            self.revision += 1;
        }
        self.pending.extend(effects);
        Ok(answer)
    }

    pub fn board(&self) -> Board {
        Board {
            tasks: self.state.tasks.clone(),
            workflows: self.state.workflows.clone(),
        }
    }

    pub fn workflow(&self, id: &str) -> Option<&Workflow> {
        self.state
            .workflows
            .iter()
            .find(|workflow| workflow.id == id)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// How many Tasks may be Working at once.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Set from Settings; at least one.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.max(1);
    }

    pub fn task(&self, id: &str) -> Option<&Task> {
        self.state.tasks.iter().find(|task| task.id == id)
    }

    /// The Task `thread` is (or was) a Worker of.
    pub fn task_of(&self, thread: ThreadId) -> Option<&Task> {
        self.state.tasks.iter().find(|task| task.had_worker(thread))
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
        self.worker == Some(thread)
            || self.workers.iter().any(|each| each.thread == thread)
            || self.oversight.superseded.contains(&thread)
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

    /// A Workflow authoring command.
    fn author(&mut self, command: Command, now_ms: i64) -> Result<Answer, Refusal> {
        match command {
            Command::CreateWorkflow {
                project,
                template,
                agent,
            } => {
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
            Command::RenameWorkflow { workflow, name } => {
                let draft = &mut self.workflow_mut(&workflow)?.draft;
                let changed = draft.name != name;
                draft.name = name;
                Ok(Answer::Changed(changed))
            }
            Command::AddStep {
                workflow,
                after,
                agent,
            } => {
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
            Command::EditStep {
                workflow,
                step,
                change,
            } => {
                let step = self.draft_step(&workflow, &step)?;
                let before = step.clone();
                change.apply(step);
                Ok(Answer::Changed(*step != before))
            }
            Command::MoveStep { workflow, step, to } => {
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
            Command::DeleteStep { workflow, step } => {
                let steps = &mut self.workflow_mut(&workflow)?.draft.steps;
                let at = steps
                    .iter()
                    .position(|each| each.id == step)
                    .ok_or_else(unknown_step)?;
                steps.remove(at);
                Ok(Answer::Changed(true))
            }
            Command::EditApproval { workflow, change } => {
                let approval = &mut self.workflow_mut(&workflow)?.draft.approval;
                let before = approval.clone();
                change.apply(approval);
                Ok(Answer::Changed(*approval != before))
            }
            Command::SaveWorkflow { workflow, catalog } => {
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
            _ => unreachable!("only authoring commands"),
        }
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
            } => {
                if let Some(task) = request.as_ref().and_then(|request| {
                    self.tasks
                        .iter()
                        .find(|task| task.request.as_ref() == Some(request))
                }) {
                    return Ok(Answer::Task(task.id.clone()));
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
                                Refusal::new(
                                    RefusalCode::UnknownWorkflow,
                                    "no Workflow has that id",
                                )
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
                Ok(Answer::Task(id))
            }
            Command::WorkerStarted {
                task,
                thread,
                worktree,
            } => {
                let task = self.task_mut(&task)?;
                if task.worker == Some(thread) {
                    return Ok(Answer::Changed(false));
                }
                task.worker = Some(thread);
                if task.worktree.is_none() {
                    task.worktree = worktree;
                }
                if let Some(step) = task.step.clone() {
                    task.workers.retain(|each| each.step != step);
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
                let task = self.worker_task(worker)?;
                // A Worker no longer at its Task's Step (one Step runs at
                // a time) reports into the past.
                if !task.running() || task.worker != Some(worker) {
                    return Ok(Answer::Changed(false));
                }
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
                        task.reason = None;
                        task.enter(Status::Completed, effects);
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
                let task = self.worker_task(worker)?;
                task.note(now_ms, Actor::Operator, text);
                // Said in the Worker, it is the answer to its question.
                task.oversight.question = None;
                if matches!(task.status, Status::NeedsHuman | Status::Stopped)
                    && task.oversight.permission.is_none()
                {
                    task.reason = None;
                    task.enter(Status::Working, effects);
                }
                task.watch.rebase = true;
                Ok(Answer::Changed(true))
            }
            Command::Stop { task } => {
                let task = self.task_mut(&task)?;
                if !matches!(
                    task.status,
                    Status::Queued | Status::Working | Status::NeedsHuman
                ) {
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
                match task.worker {
                    Some(_) => {
                        task.enter(Status::Working, effects);
                        task.watch.prompt = Some(resume_prompt(task));
                    }
                    // It never had a Worker: the next step starts one.
                    None => task.enter(Status::Queued, effects),
                }
                Ok(Answer::Changed(true))
            }
            Command::EditStart { .. }
            | Command::Automation { .. }
            | Command::Backlog { .. }
            | Command::GitHubRead { .. }
            | Command::Submit { .. } => self.automate(command, now_ms, effects),
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
                let task = self.tasks.remove(at);
                let mut parked = Vec::new();
                for thread in task
                    .worker
                    .into_iter()
                    .chain(task.workers.iter().map(|each| each.thread))
                {
                    if !parked.contains(&thread) {
                        parked.push(thread);
                        effects.push(Effect::Park { thread });
                    }
                }
                if let Some(path) = task.worktree {
                    effects.push(Effect::RemoveWorktree { path });
                }
                Ok(Answer::Changed(true))
            }
            Command::Ask { worker, text } => {
                let task = self.worker_task(worker)?;
                direct::ask(task, worker, text, now_ms, effects)
            }
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
            Command::ReportRevision { worker, head, pr } => {
                pr::reported_revision(self.worker_task(worker)?, worker, head, pr, now_ms)
            }
            Command::ReportEvidence { worker, text } => {
                pr::reported_evidence(self.worker_task(worker)?, worker, text, now_ms)
            }
            Command::WorktreeTidied { task, kept } => {
                pr::tidied(self.task_mut(&task)?, kept, now_ms)
            }
            authoring => self.author(authoring, now_ms),
        }
    }

    fn step(
        &mut self,
        now_ms: i64,
        workers: &BTreeMap<ThreadId, Seen>,
        limit: usize,
        effects: &mut Vec<Effect>,
    ) {
        let mut working = self
            .tasks
            .iter()
            .filter(|task| task.status == Status::Working)
            .count();
        for task in &mut self.tasks {
            match task.status {
                // The excess waits until a Task leaves Working.
                Status::Queued if working >= limit => {}
                Status::Queued => {
                    working += 1;
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
                Status::Working | Status::NeedsHuman => {
                    let Some(seen) = task.worker.and_then(|thread| workers.get(&thread)) else {
                        continue;
                    };
                    if direct::permission(
                        task,
                        seen.decision.as_deref(),
                        seen.turns,
                        now_ms,
                        effects,
                    ) || task.status != Status::Working
                    {
                        continue;
                    }
                    at_rest_rule(task, seen, now_ms, effects);
                }
                _ => {}
            }
        }
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
    task.oversight.deciding = deciding;
    task.enter(Status::Working, effects);
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
            effects.push(Effect::Park { thread: old });
            task.workers.retain(|each| each.step != step.id);
            start_step(task, step, now_ms, effects);
        }
        None => start_step(task, step, now_ms, effects),
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
/// Worker is never asked.
fn at_rest_rule(task: &mut Task, seen: &Seen, now_ms: i64, effects: &mut Vec<Effect>) {
    let Some(thread) = task.worker else {
        return;
    };
    if task.watch.at_rest && seen.busy {
        return;
    }
    if let Some(text) = task.watch.prompt.take() {
        task.watch.at_rest = false;
        effects.push(Effect::Prompt { thread, text });
        task.watch.baseline = seen.turns;
        task.watch.nudged_ms = None;
        return;
    }
    if std::mem::take(&mut task.watch.rebase) {
        task.watch.baseline = seen.turns;
        return;
    }
    if seen.busy {
        return;
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
                text: nudge_prompt(),
            });
            task.watch.baseline = seen.turns;
            task.watch.nudged_ms = Some(now_ms);
        }
        Some(nudged) if ended || now_ms - nudged >= NUDGE_GRACE_MS => {
            task.note(
                now_ms,
                Actor::Ferrite,
                "The agent stopped again without reporting",
            );
            task.reason = Some("The agent stopped without reporting an outcome.".into());
            task.enter(Status::NeedsHuman, effects);
        }
        _ => {}
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
        TaskAction::Start { .. } | TaskAction::Delete { .. } | TaskAction::Unknown => return None,
    })
}

/// `Claude · claude-opus-4`, as a card names a Worker's.
pub fn agent_label(agent: &Agent) -> String {
    let provider = match agent.provider {
        Provider::Claude => "Claude",
        Provider::Codex => "Codex",
    };
    match &agent.model {
        Some(model) => format!("{provider} · {model}"),
        None => provider.to_string(),
    }
}

/// How a Worker reports: the `ferrite` its Session was given.
const REPORT: &str = "\"${FERRITE_BIN:-ferrite}\" report outcome";
/// How a Worker reports its PR's head.
const REVISION: &str = "\"${FERRITE_BIN:-ferrite}\" report revision";
/// How a Worker reports what a command or tool showed.
const EVIDENCE: &str = "\"${FERRITE_BIN:-ferrite}\" report evidence";
/// How a Worker asks the operator.
const QUESTION: &str = "\"${FERRITE_BIN:-ferrite}\" report question";

fn first_prompt(task: &Task) -> String {
    format!(
        "You are working on Ferrite Factory task {id}, in the task's own worktree on branch \
         `{branch}`.\n\n{instructions}\n{pr}\nWhen you are done, report the outcome with one \
         command:\n\n    {REPORT} finished \"<one line: what you did>\"\n\nIf you cannot \
         finish, report what you tried:\n\n    {REPORT} unable \"<what you tried and what \
         stopped you>\"\n\n{ASKING}\n\nThe outcome is read only from that command, never \
         from your messages.",
        id = task.id,
        branch = task.branch,
        instructions = task.instructions,
        pr = if task.delivery.pr.is_some() {
            pr::context(task)
        } else {
            String::new()
        },
    )
}

const REPORTING: &str = "When you are done, report the outcome with one command:\n\n    \
     \"${FERRITE_BIN:-ferrite}\" report outcome finished \"<one line: what you did>\"\n\nIf you \
     cannot finish, report what you tried:\n\n    \"${FERRITE_BIN:-ferrite}\" report outcome \
     unable \"<what you tried and what stopped you>\"\n\nThe outcome is read only from that \
     command, never from your messages.";

const ASKING: &str = "If you need the operator to decide something before you can go on, ask \
     them and end your turn; the task waits for their answer:\n\n    \
     \"${FERRITE_BIN:-ferrite}\" report question \"<your question>\"";

/// How the current run is to report: its outcome, or (deciding an
/// approval) its decision.
fn reporting(task: &Task, step: &Step) -> String {
    if task.oversight.deciding {
        format!(
            "{}\nIf you cannot decide, report what stopped you and the operator decides:\n\n    \
             {REPORT} unable \"<what stopped you>\"",
            direct::decision_section(task)
        )
    } else if task.decides_in_own_run(step) {
        format!(
            "When your work is done, you also decide its approval.\n\n{}\nIf you cannot \
             finish, report what you tried:\n\n    {REPORT} unable \"<what you tried>\"",
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
    let reporting = match task.current_step() {
        Some(step) => reporting(task, step),
        None => format!(
            "When you are done, report the outcome with `{REPORT} finished \"<what you did>\"`, \
             or `{REPORT} unable \"<what you tried>\"`."
        ),
    };
    format!(
        "The operator answered your question.\n\nYou asked: {question}\n\nTheir answer: \
         {answer}\n\nContinue with your instructions in light of that.\n\n{reporting}"
    )
}

fn nudge_prompt() -> String {
    format!(
        "Your turn ended without reporting the task's outcome. Report it now with \
         `{REPORT} finished \"<what you did>\"`, or `{REPORT} unable \"<what you tried>\"`."
    )
}

fn resume_prompt(task: &Task) -> String {
    // A merge Step reports its result, after reconciling an unconfirmed
    // one (#223).
    if let Some(step) = task.current_step().filter(|step| step.outcome_routes.is_some()) {
        return format!(
            "The operator resumed this task. Continue where you left off.\n{}\n{}",
            pr::reconcile(task, step),
            reporting(task, step)
        );
    }
    format!(
        "The operator resumed this task. Continue where you left off, and report the \
         outcome with `{REPORT} finished …` or `{REPORT} unable …` when you are done."
    )
}

#[cfg(test)]
mod tests;
