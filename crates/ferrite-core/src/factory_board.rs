//! The Factory's board (ADR 0037) as the Server publishes it and every
//! window draws it: each Project's Tasks, its Workflows, its Coordinator
//! and the Coordinator's proposals, with every fact they hold. The
//! Factory that changes them, and when, is the Server's (`factory`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::project::ProjectId;
use crate::provider::Provider;
use crate::ThreadId;

mod archive;
mod coordinator;
mod direct;
mod pr;
mod recovery;
mod starts;
mod workflow;

pub use coordinator::{Act, Coordinator, Proposal, ProposalId, ProposalStatus};
pub(crate) use direct::Said;
pub use direct::{Answered, Feedback, FeedbackKind, Override, Oversight, Question, ThenReturn};
pub use pr::{
    short, Delivery, Evidence, EvidenceKind, MergeOutcome, MergeRoutes, PullRequest, Revision,
};
pub use recovery::Keeping;
pub use starts::{when_label, zone_label};
pub use starts::{
    Automation, GitHubItem, GitHubQuery, GitHubRead, GitHubSource, Missed, Overlap, Recurrence,
    Schedule, Source, Start, StartKind, Switch, Trigger, Turn, INTERVAL_HOURS, WEEKDAYS,
};
pub use workflow::{
    Approval, ApprovalChange, Approver, Catalog, Definition, Destination, Problem, Resolved,
    RouteKind, Step, StepChange, StepId, Template, Version, Workflow, WorkflowId,
};

/// A Task's id: `t-<n>`, never given twice by one Server. A Worker's
/// owner names it (`{"task": "t-1"}`).
pub type TaskId = String;

/// How many Tasks work at once unless Settings say otherwise.
pub const DEFAULT_LIMIT: usize = 3;

/// How many Tasks Settings may let work at once: one rule, here, for the
/// Settings stepper and the Server alike.
pub const LIMIT_RANGE: std::ops::RangeInclusive<usize> = 1..=12;

/// `limit` as the Factory takes it: within [`LIMIT_RANGE`].
pub fn clamp_limit(limit: usize) -> usize {
    limit.clamp(*LIMIT_RANGE.start(), *LIMIT_RANGE.end())
}

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
    /// How many turns the Worker had ended (`factory::Seen::turns`) when it was
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

    use crate::project::ProjectId;

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

/// `Claude · claude-opus-4`, as a card names a Worker's.
pub fn agent_label(agent: &Agent) -> String {
    let provider = agent.provider;
    match &agent.model {
        Some(model) => format!("{provider} · {model}"),
        None => provider.to_string(),
    }
}
