//! Every API operation, declared once (after Herdr's `define_operations!`):
//! its wire name, its parameters, which of them the CLI takes in order,
//! and the line `ferrite help` says. The wire enum, the catalog the CLI
//! parses by, and the JSON Schema (`ferrite --schema`) all come from that
//! one declaration. Names are append-only, as the client contract's are:
//! never renamed or removed; a new parameter is optional.
//!
//! A refusal's `code` is one of these, stable as the names are:
//!
//! - `invalid_request`, `unknown_method`: the line, or its parameters.
//! - `unknown_thread`, `project_not_found`, `unknown_decision`: no such
//!   open Thread (parked, for `thread.revive`), Project, or pending
//!   Decision on the Thread.
//! - `agent_blocked`: a Decision waits; nothing was sent.
//! - `agent_not_ready`: `thread.start --ready` found the agent blocked, or
//!   not up within its time; the refusal names the Thread, which stays.
//! - `agent_prompt_stalled`, `timeout`: a wait gave up; nothing was undone.
//! - `thread_closed`: the Thread closed while a wait ran.
//! - `worktree_not_found`: no linked worktree is at that path.
//! - `dirty_worktree_requires_force`: the worktree holds changes; nothing
//!   was removed (there is no force).
//! - `already_answered`, `invalid_answer`: a Decision's answer.
//! - `unknown_task`: no Task has that id.
//! - `unknown_workflow`, `unknown_step`: no Workflow, or no Step in it,
//!   has that id.
//! - `not_a_worker`: `report` from a Thread that is no Task's Worker.
//! - `not_now`: the Task's state does not allow it (delete a working
//!   Task: stop it first).
//! - `unknown_proposal`: no proposal has that id.
//! - `not_a_coordinator`: `proposal raise` from a Thread that is no
//!   Project's Coordinator.
//! - `failed`: anything else, in its own words.
//! - `server_unavailable`: the CLI's own, no Server answered.
//!
//! Thread ids are the Store's, so they hold across a Server's restart
//! (ADR 0029); a Thread's `attention_seq` and `completion_seq` are one
//! count for every Thread of the Server's `boot`, never reset by a park or
//! a revive, and start over with another `boot` when it restarts.

use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::protocol::{WireAnswer, WireSound};
use crate::store::Provider;
use crate::{Attention, ThreadId};

/// One operation as the CLI and `ferrite help` know it.
pub struct Operation {
    /// The wire name, `group.verb`; the CLI spells it `ferrite group verb`.
    pub name: &'static str,
    /// The parameters the CLI takes without a flag, in order.
    pub positional: &'static [&'static str],
    pub summary: &'static str,
    /// The JSON Schema of its parameters.
    pub params: fn() -> schemars::Schema,
}

impl Operation {
    /// `thread.start` → (`thread`, `start`).
    pub fn words(&self) -> (&'static str, &'static str) {
        self.name
            .split_once('.')
            .expect("an operation is group.verb")
    }
}

macro_rules! operations {
    ($( $variant:ident($params:ty) => $name:literal, [$($positional:literal),*], $summary:literal; )*) => {
        /// What a request asks for, and with what.
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
        #[serde(tag = "method", content = "params")]
        pub enum Method {
            $(#[doc = $summary] #[serde(rename = $name)] $variant($params),)*
        }

        impl Method {
            pub const OPERATIONS: &'static [Operation] = &[
                $(Operation {
                    name: $name,
                    positional: &[$($positional),*],
                    summary: $summary,
                    params: || schemars::schema_for!($params),
                },)*
            ];

            pub fn name(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $name,)*
                }
            }
        }
    };
}

operations! {
    ThreadStart(ThreadStartParams) => "thread.start", [],
        "Start a Thread: a new agent Session working in a directory.";
    ThreadList(Empty) => "thread.list", [],
        "List the open Threads and their Attention.";
    ThreadGet(ThreadParams) => "thread.get", ["thread"],
        "One open Thread and its Attention.";
    ThreadFocus(ThreadParams) => "thread.focus", ["thread"],
        "Land every Ferrite window on a Thread.";
    ThreadRename(ThreadRenameParams) => "thread.rename", ["thread", "title"],
        "Give a Thread a title.";
    ThreadClose(ThreadParams) => "thread.close", ["thread"],
        "Park a Thread: its Session ends, its history stays.";
    AgentPrompt(AgentPromptParams) => "agent.prompt", ["thread", "text"],
        "Send a Thread's agent a prompt; with wait, until it settles.";
    AgentWait(AgentWaitParams) => "agent.wait", ["thread"],
        "Wait until a Thread's agent settles, or reaches one Attention.";
    AgentRead(AgentReadParams) => "agent.read", ["thread"],
        "The tail of a Thread's transcript, as plain text.";
    AgentInterrupt(ThreadParams) => "agent.interrupt", ["thread"],
        "Stop a Thread's running turn, as the operator's Esc does.";
    DecisionList(DecisionListParams) => "decision.list", [],
        "The pending Decisions, of every Thread or of one.";
    DecisionAnswer(DecisionAnswerParams) => "decision.answer", ["thread", "decision"],
        "Answer a pending Decision. Only on the operator's word.";
    ProjectList(Empty) => "project.list", [],
        "List the Projects, how many open Threads each has, and the focused one.";
    ProjectGet(ProjectParams) => "project.get", ["project"],
        "One Project.";
    ProjectOpen(ProjectOpenParams) => "project.open", [],
        "The Project holding a directory, registered if there is none.";
    ProjectRename(ProjectRenameParams) => "project.rename", ["project", "label"],
        "Give a Project a label.";
    ProjectClose(ProjectParams) => "project.close", ["project"],
        "Park every open Thread of a Project.";
    WorktreeCreate(WorktreeCreateParams) => "worktree.create", [],
        "Make a new worktree of a repository on a new branch, for a Thread to start in.";
    WorktreeRemove(WorktreeRemoveParams) => "worktree.remove", ["path"],
        "Remove a clean worktree, parking the Threads in it; its branch stays. Never forced.";
    MachineList(Empty) => "machine.list", [],
        "The saved machines another Server is asked on (--machine).";
    EventsSubscribe(EventsSubscribeParams) => "events.subscribe", [],
        "What the Threads are now, then every change as it happens.";
    NotificationShow(NotificationShowParams) => "notification.show", ["title"],
        "Show a toast in every Ferrite window.";
    ProjectFocus(ProjectParams) => "project.focus", ["project"],
        "Land every Ferrite window on a Project's first open Thread.";
    ThreadRevive(ThreadReviveParams) => "thread.revive", ["thread"],
        "Start a parked Thread's agent again in place, keeping its id.";
    TaskStart(TaskStartParams) => "task.start", ["instructions"],
        "Hand a Project's Factory a one-off Task: one agent in its own worktree.";
    TaskList(TaskListParams) => "task.list", [],
        "The Factory's Tasks, of every Project or of one.";
    TaskGet(TaskParams) => "task.get", ["task"],
        "One Task: its state, activity, Worker and worktree.";
    TaskStop(TaskParams) => "task.stop", ["task"],
        "Stop a Task: its Worker is interrupted.";
    TaskResume(TaskParams) => "task.resume", ["task"],
        "Resume a stopped Task: its Worker carries on.";
    TaskDelete(TaskParams) => "task.delete", ["task"],
        "Delete a Queued, Stopped or Completed Task; its worktree only if clean.";
    ReportOutcome(ReportOutcomeParams) => "report.outcome", ["outcome", "summary"],
        "A Worker reports its Task finished, or unable to finish and what it tried.";
    WorkflowCreate(WorkflowCreateParams) => "workflow.create", [],
        "Make a Project's Workflow, Blank or from the issue_to_reviewed_pr template.";
    WorkflowList(TaskListParams) => "workflow.list", [],
        "The Factory's Workflows, of every Project or of one.";
    WorkflowGet(WorkflowParams) => "workflow.get", ["workflow"],
        "One Workflow: its draft, saved versions and the draft's problems.";
    WorkflowRename(WorkflowRenameParams) => "workflow.rename", ["workflow", "name"],
        "Rename a Workflow's draft.";
    WorkflowSave(WorkflowParams) => "workflow.save", ["workflow"],
        "Save a Workflow's draft as its next version. Starts nothing.";
    StepAdd(StepAddParams) => "step.add", ["workflow"],
        "Add a Step to a Workflow's draft, after a Step or last.";
    StepEdit(StepEditParams) => "step.edit", ["workflow", "step"],
        "Change a Step: name, instructions, skills, provider, model, effort, routes, fresh.";
    StepMove(StepMoveParams) => "step.move", ["workflow", "step", "to"],
        "Move a Step to a place in the outline (0 is first).";
    StepDelete(StepParams) => "step.delete", ["workflow", "step"],
        "Delete a Step; routes to it stay, flagged.";
    ReportQuestion(ReportQuestionParams) => "report.question", ["text"],
        "A Worker asks the operator; its Task waits in Needs human for the answer.";
    TaskAnswer(TaskAnswerParams) => "task.answer", ["task", "text"],
        "Answer a Task's question: the agent that asked continues with it.";
    TaskApprove(TaskParams) => "task.approve", ["task"],
        "Approve a Task that is Ready to approve: what follows approval runs.";
    TaskRework(TaskReworkParams) => "task.rework", ["task", "feedback"],
        "Send a Task back with feedback, to the Workflow's rework recipient or one chosen.";
    TaskRedirect(TaskRedirectParams) => "task.redirect", ["task", "to"],
        "Send one Task to a Step, skipping Steps, then its routes or back to you. The Workflow is unchanged.";
    WorkflowApproval(WorkflowApprovalParams) => "workflow.approval", ["workflow"],
        "Change who approves a Workflow's draft: you or an agent, its instructions, what follows, where rework goes.";
    WorkflowStart(WorkflowStartParams) => "workflow.start", ["workflow", "start"],
        "Set how a Workflow's draft starts Tasks: manual, schedule, github_issue, github_pr or external.";
    WorkflowAutomation(WorkflowAutomationParams) => "workflow.automation", ["workflow", "turn"],
        "Turn a Workflow's automatic starts on, pause, resume or off. Saving never turns them on.";
    WorkflowBacklog(WorkflowBacklogParams) => "workflow.backlog", ["workflow", "take"],
        "Take, or pass over, what already matched when a GitHub start was turned on.";
    TaskSubmit(TaskSubmitParams) => "task.submit", ["workflow", "identity", "instructions"],
        "Submit a Task to a Workflow that starts from your own script; a repeated identity returns its Task.";
    ReportRevision(ReportRevisionParams) => "report.revision", ["head"],
        "A Worker reports its Task's PR head (and which PR, the first time): each new head is a revision.";
    ReportEvidence(ReportEvidenceParams) => "report.evidence", ["text"],
        "A Worker keeps what a check or tool showed as evidence, about the PR's current revision.";
    ProposalRaise(ProposalRaiseParams) => "proposal.raise", ["act"],
        "The Coordinator proposes starting a Task, saving a version, automatic starts or a redirect; nothing happens until the user accepts.";
    ProposalList(TaskListParams) => "proposal.list", [],
        "The Coordinators' proposals, pending, accepted or dismissed, of every Project or of one.";
    ProposalAccept(ProposalParams) => "proposal.accept", ["proposal"],
        "Accept a proposal: its act runs as the user's. Only on the user's word.";
    ProposalDismiss(ProposalParams) => "proposal.dismiss", ["proposal"],
        "Dismiss a proposal: nothing changes.";
    CoordinatorSay(CoordinatorSayParams) => "coordinator.say", ["text"],
        "Say something to a Project's Coordinator, starting it on first use.";
    CoordinatorSet(CoordinatorSetParams) => "coordinator.set", ["provider"],
        "Choose the Coordinator's provider, model and effort, from its next turn.";
    TaskClear(TaskClearParams) => "task.clear", [],
        "Move a Project's Completed Tasks to its Archive now, off the board; they are kept whole.";
}

/// No parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Empty {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThreadStartParams {
    /// The directory the agent works in. The CLI sends its own when none
    /// is given, except with `--machine`.
    pub cwd: PathBuf,
    /// The operator's default Provider when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<Provider>,
    /// Work in a fresh worktree of `cwd`'s repository instead. A `cwd`
    /// that is a linked worktree already is worked in as one, either way.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub worktree: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Land every window on it once started.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub focus: bool,
    /// Extra arguments for the provider's CLI, written as for its terminal
    /// UI, after Ferrite's own, for every Session of that provider the
    /// Thread has, revived ones too. Codex's become its app-server's
    /// settings: `-m`/`--model`, `-c model_reasoning_effort=…`,
    /// `-a`/`--ask-for-approval`, `-s`/`--sandbox` and
    /// `--dangerously-bypass-approvals-and-sandbox` (never ask, full
    /// access); the rest (`-c`, `--enable`, `--disable`) pass through.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// With `worktree`: the new worktree's branch, which must not exist;
    /// one Ferrite names when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// With `branch`: what it starts from; the repository's HEAD when
    /// absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Open it in this Project (`project.list`'s `id`): `cwd` must lie in
    /// one of the Project's directories (or be a linked worktree of one),
    /// and the Thread works in `cwd` itself; with `worktree`, the fresh
    /// worktree is of the directory holding `cwd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Answer only once the agent is up and ready for a prompt (idle or
    /// done); `agent_not_ready` when it is blocked or not up in time.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ready: bool,
    /// With `ready`: how long to give it, from the request; 20 s when
    /// absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// The provider's own id of a conversation to carry on
    /// (`provider_session`), rather than a new one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
    /// Set in the agent's environment, under Ferrite's own, for every
    /// Session of that provider the Thread has, revived ones too.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// A program the provider's CLI is run through, and its arguments, for
    /// every Session of that provider: `bwrap --ro-bind <tree> <tree> --`
    /// runs it where `<tree>` cannot be written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub launcher: Vec<String>,
    /// Whose the Thread is, when not the operator's: `{"task": "<id>"}`
    /// for a Worker, `{"factory": "<project>"}` for a Project's
    /// Coordinator. An owned Thread is left out of the window's lists and
    /// alerts; the API lists it, naming its owner. Kept for its life.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<Owner>,
}

/// Whose a Thread is when not the operator's (ADR 0037), as the API
/// spells it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Owner {
    /// A Worker: the Task it runs a Step for, by the Task's id.
    Task(String),
    /// A Coordinator: the Project whose Factory it directs, by
    /// `project.list`'s `id`.
    Factory(String),
}

/// A parked Thread started again (Herdr's `agent start` in the pane whose
/// agent was ended): what it starts with replaces what it was started
/// with, and it carries on `resume`'s conversation, or a new one under its
/// history when there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThreadReviveParams {
    pub thread: ThreadId,
    /// The Provider these launch overrides target. If the Thread has
    /// switched since, resume its current conversation with its saved
    /// launch instead. This does not switch the Thread's Provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_provider: Option<Provider>,
    /// As `thread.start`'s.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// As `thread.start`'s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
    /// As `thread.start`'s.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// As `thread.start`'s.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub launcher: Vec<String>,
    /// As `thread.start`'s.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ready: bool,
    /// As `thread.start`'s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThreadParams {
    pub thread: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThreadRenameParams {
    pub thread: ThreadId,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentPromptParams {
    pub thread: ThreadId,
    pub text: String,
    /// Answer once the agent was seen working and then settled (idle,
    /// done or blocked), not as soon as the prompt is sent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wait: bool,
    /// Give up `timeout` after this long, counted from the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// Answer as soon as the agent is seen in one of these after the
    /// prompt, not once it settles (`working` and `blocked` confirm the
    /// prompt was taken). Implies `wait`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub until: Vec<Attention>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentWaitParams {
    pub thread: ThreadId,
    /// The Attention to wait for; settled (idle, done or blocked) when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<Attention>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentReadParams {
    pub thread: ThreadId,
    /// How many of the last lines; 80 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DecisionListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<ThreadId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DecisionAnswerParams {
    pub thread: ThreadId,
    /// The Decision's `id`, as `decision.list` gives it.
    pub decision: String,
    pub answer: WireAnswer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EventsSubscribeParams {
    /// Only these Threads' events; every Thread's when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub threads: Vec<ThreadId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NotificationShowParams {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// The chime the windows ring with it; none when absent.
    #[serde(default, skip_serializing_if = "WireSound::is_none")]
    pub sound: WireSound,
    /// Left up until dismissed, rather than fading.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub important: bool,
}

/// A Project, by the `id` `project.list` gives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectParams {
    pub project: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectOpenParams {
    /// A directory of the Project. The CLI sends its own when none is
    /// given, except with `--machine`.
    pub cwd: PathBuf,
    /// The label a Project registered by this request starts with; one
    /// already there keeps its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Land every window on the Project's first open Thread, if it has one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub focus: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRenameParams {
    pub project: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorktreeCreateParams {
    /// A checkout of the repository. The CLI sends its own when none is
    /// given, except with `--machine`.
    pub cwd: PathBuf,
    /// The new worktree's branch, which must not exist yet.
    pub branch: String,
    /// What the branch starts from; the repository's HEAD when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorktreeRemoveParams {
    /// The linked worktree's directory.
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskStartParams {
    /// What the Worker is to do; its first line is the Task's title.
    pub instructions: String,
    /// The Project whose Factory takes it (`project.list`'s `id`); else
    /// the Project holding `cwd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The CLI sends its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    /// Run it through this Workflow's latest saved version (`w-1`);
    /// else it is one-off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<String>,
    /// Its pull request, which every Step works on:
    /// `owner/repository#number` or its URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskClearParams {
    /// The Project whose Completed Tasks go (`project.list`'s `id`); else
    /// the Project holding `cwd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The CLI sends its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskListParams {
    /// Only this Project's (`project.list`'s `id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskParams {
    /// The Task's id, as `task.list` gives it (`t-1`).
    pub task: String,
}

/// What a Worker reports: `finished` (`summary` says what it did) or
/// `unable` to finish (`summary` says what it tried); an agent deciding
/// an approval reports `approved` (what it verified) or `rejected` (its
/// reasons); a merge Step reports `merged` (only once confirmed),
/// `conflict`, `pending`, `denied`, `head_changed` or `unknown`, with
/// what showed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReportedOutcome {
    Finished,
    Unable,
    Approved,
    Rejected,
    Merged,
    Conflict,
    Pending,
    Denied,
    HeadChanged,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReportOutcomeParams {
    pub outcome: ReportedOutcome,
    pub summary: String,
    /// The reporting Worker's own Thread: the CLI sends
    /// `FERRITE_THREAD_ID`, so its Task is known from where it runs.
    pub worker: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReportRevisionParams {
    /// The PR's head commit hash.
    pub head: String,
    /// Which PR (`owner/repository#number` or its URL): needed the first
    /// time, when the Task has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<String>,
    /// The reporting Worker's own Thread (the CLI sends
    /// `FERRITE_THREAD_ID`).
    pub worker: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReportEvidenceParams {
    /// What a command or tool showed: `<the command>: <what it showed>`.
    pub text: String,
    /// The reporting Worker's own Thread (the CLI sends
    /// `FERRITE_THREAD_ID`).
    pub worker: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReportQuestionParams {
    /// What the Worker needs the operator to decide.
    pub text: String,
    /// The asking Worker's own Thread (the CLI sends `FERRITE_THREAD_ID`).
    pub worker: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskAnswerParams {
    pub task: String,
    pub text: String,
    /// The question's id (`task.get`'s `question.id`): an answer to an
    /// earlier one changes nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskReworkParams {
    pub task: String,
    /// What should change; every later Step is told it.
    pub feedback: String,
    /// `step:<id>` or `coordinator`; else the Workflow's rework recipient.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskRedirectParams {
    pub task: String,
    /// The Step that takes it (`s-2`).
    pub to: String,
    /// Steps passed over for this Task, as if they had finished.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skip: Vec<String>,
    /// `routes` (the default) or `me`: return it to you to approve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub then: Option<crate::factory::ThenReturn>,
    /// Context for the agent that takes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Each change given is made: `by` (`human` or `agent`), the deciding
/// `step`, its `instructions`, `after` (`complete` or `step:<id>`) and
/// `rework_to` (`step:<id>` or `coordinator`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowApprovalParams {
    pub workflow: String,
    #[serde(flatten)]
    pub change: crate::factory::ApprovalChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowCreateParams {
    /// What it starts from: `blank` (the default) or
    /// `issue_to_reviewed_pr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<crate::factory::Template>,
    /// Its name; the template's when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The Project (`project.list`'s `id`); else the Project holding `cwd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The CLI sends its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowParams {
    /// The Workflow's id, as `workflow.list` gives it (`w-1`).
    pub workflow: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowRenameParams {
    pub workflow: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StepAddParams {
    pub workflow: String,
    /// The Step it goes after (`s-1`); else last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StepParams {
    pub workflow: String,
    /// The Step's id (`s-1`).
    pub step: String,
}

/// Each change given is made; an empty model or effort is the
/// Provider's default. Routes: `next`, `step:<id>`, `complete`, `human`
/// or `coordinator`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StepEditParams {
    pub workflow: String,
    pub step: String,
    #[serde(flatten)]
    pub change: crate::factory::StepChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StepMoveParams {
    pub workflow: String,
    pub step: String,
    /// Its place in the outline, 0 first.
    pub to: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowStartParams {
    pub workflow: String,
    /// `{"type": "schedule", "schedule": {...}, "github": {...}}`: every
    /// source's settings, kept while another is chosen.
    pub start: crate::factory::Start,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowAutomationParams {
    pub workflow: String,
    pub turn: crate::factory::Turn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowBacklogParams {
    pub workflow: String,
    /// Start a Task for each; else they are passed over.
    pub take: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskSubmitParams {
    /// A Workflow whose saved version starts from your own script.
    pub workflow: String,
    /// The submission's own id in your tracker: the same one again is the
    /// same Task.
    pub identity: String,
    /// What the Task is to do; its first line is its title.
    pub instructions: String,
}

/// Which of the four acts a proposal is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProposedAct {
    Start,
    Save,
    Automation,
    Redirect,
}

/// `start` takes `instructions` and maybe `workflow`; `save` and
/// `automation` a `workflow`; `redirect` a `task`, the Step it goes `to`,
/// and maybe `skip`, `then` and `note`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProposalRaiseParams {
    pub act: ProposedAct,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skip: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub then: Option<crate::factory::ThenReturn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The proposing Coordinator's own Thread (the CLI sends
    /// `FERRITE_THREAD_ID`).
    pub coordinator: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProposalParams {
    /// The proposal's id, as `proposal.list` gives it (`p-1`).
    pub proposal: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CoordinatorSayParams {
    pub text: String,
    /// The Project (`project.list`'s `id`); else the Project holding `cwd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The CLI sends its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CoordinatorSetParams {
    pub provider: crate::store::Provider,
    /// The provider's default when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The operator's default when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The CLI sends its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
}
