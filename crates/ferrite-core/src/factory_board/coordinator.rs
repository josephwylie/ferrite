//! A Project's Coordinator (#224, ADR 0037) and its proposals, as the
//! board holds them. Briefing it and running a proposal the operator
//! accepts are the Factory's (`factory::coordinator`).

use serde::{Deserialize, Serialize};

use super::{Agent, Board, Definition, Override, TaskId, ThenReturn, WorkflowId};
use crate::project::ProjectId;
use crate::ThreadId;

/// A proposal's id: `p-<n>`, never given twice by one Server.
pub type ProposalId = String;

/// A Project's Coordinator, as the Factory keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coordinator {
    pub project: ProjectId,
    /// Its Thread, once started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<ThreadId>,
    /// The Provider, model and effort chosen for it; the operator's
    /// default when none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<Agent>,
    /// Its Thread is being started.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub starting: bool,
    /// What was said while it started, told once it has.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waiting: Vec<String>,
    /// Why it could not start, last time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One of the four acts the Coordinator may only propose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Act {
    /// Start a Task: one-off, or on a Workflow's latest saved version.
    Start {
        instructions: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workflow: Option<WorkflowId>,
    },
    /// Save this draft of a Workflow as `version`.
    Save {
        workflow: WorkflowId,
        version: u32,
        draft: Definition,
    },
    /// Turn a Workflow's automatic starts on.
    Automation { workflow: WorkflowId },
    /// One Task's direction.
    Redirect { task: TaskId, direction: Override },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Pending,
    Accepted,
    Dismissed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub id: ProposalId,
    pub project: ProjectId,
    pub act: Act,
    pub status: ProposalStatus,
    pub raised_ms: i64,
    /// The Task an accepted start made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskId>,
}

impl Proposal {
    /// The card's title.
    pub fn title(&self, board: &Board) -> String {
        match &self.act {
            Act::Start { .. } => "Hand off a task".into(),
            Act::Save {
                workflow, version, ..
            } => format!(
                "Save {} as version {version}",
                workflow_name(board, workflow)
            ),
            Act::Automation { .. } => "Turn on automatic starts".into(),
            Act::Redirect { task, .. } => format!("Direct task {task} \u{b7} this task only"),
        }
    }

    /// Its button.
    pub fn action(&self) -> &'static str {
        match self.act {
            Act::Start { .. } => "Start task",
            Act::Save { .. } => "Save version",
            Act::Automation { .. } => "Turn on",
            Act::Redirect { .. } => "Apply to this task",
        }
    }

    /// Exactly what accepting it does, as (label, value) lines.
    pub fn lines(&self, board: &Board) -> Vec<(String, String)> {
        let line = |label: &str, value: String| (label.to_string(), value);
        match &self.act {
            Act::Start {
                instructions,
                workflow,
            } => {
                let (title, rest) = instructions
                    .trim()
                    .split_once('\n')
                    .unwrap_or((instructions.trim(), ""));
                let title = title.trim();
                let process = match workflow {
                    None => "One-off handoff".to_string(),
                    Some(id) => match board.workflow(id) {
                        Some(found) => match found.latest() {
                            Some(latest) => format!(
                                "{} \u{b7} version {}",
                                latest.definition.name, latest.version
                            ),
                            None => format!("{} \u{b7} no saved version", found.draft.name),
                        },
                        None => "A deleted workflow".into(),
                    },
                };
                let mut lines = vec![line("Task", title.to_string()), line("Process", process)];
                if !rest.trim().is_empty() {
                    lines.push(line("Instructions", rest.trim().to_string()));
                }
                lines
            }
            Act::Save { draft, .. } => {
                let agents: Vec<&str> = draft.steps.iter().map(|step| step.name.as_str()).collect();
                vec![
                    line("Agents", agents.join(" \u{2192} ")),
                    line("Starts", draft.start.kind.label().to_string()),
                    line(
                        "Then",
                        "New tasks use it; running tasks keep theirs. Saving starts nothing."
                            .into(),
                    ),
                ]
            }
            Act::Automation { workflow } => {
                let Some(found) = board.workflow(workflow) else {
                    return vec![line("Workflow", "A deleted workflow".into())];
                };
                let mut lines = vec![line("Workflow", found.draft.name.clone())];
                if let Some(latest) = found.latest() {
                    let start = &latest.definition.start;
                    lines[0].1 = format!(
                        "{} \u{b7} version {}",
                        latest.definition.name, latest.version
                    );
                    lines.push(line("Starts", start.kind.label().to_string()));
                    let detail = match start.kind {
                        super::StartKind::Schedule => Some(start.schedule.describe()),
                        kind if kind.is_github() => Some(start.github.describe(kind)),
                        _ => None,
                    };
                    lines.extend(detail.map(|detail| line("When", detail)));
                }
                lines
            }
            Act::Redirect { task, direction } => {
                let found = board.task(task);
                let step_name = |id: &str| {
                    found
                        .and_then(|task| task.workflow.as_ref())
                        .and_then(|run| run.definition.step(id))
                        .map_or_else(|| id.to_string(), |step| step.name.clone())
                };
                let mut lines = vec![];
                if let Some(found) = found {
                    lines.push(line("Task", found.title.clone()));
                }
                lines.push(line("Send to", step_name(&direction.to)));
                let skipped: Vec<String> = direction.skip.iter().map(|id| step_name(id)).collect();
                lines.push(line(
                    "Skip",
                    if skipped.is_empty() {
                        "Nothing".into()
                    } else {
                        skipped.join(", ")
                    },
                ));
                lines.push(line(
                    "Then",
                    match direction.then {
                        ThenReturn::Me => "Return it to you to approve".into(),
                        ThenReturn::Routes => "Follow the workflow's routes".into(),
                    },
                ));
                if !direction.note.trim().is_empty() {
                    lines.push(line("Feedback", direction.note.trim().to_string()));
                }
                lines
            }
        }
    }
}

fn workflow_name(board: &Board, id: &str) -> String {
    board
        .workflow(id)
        .map_or_else(|| id.to_string(), |found| found.draft.name.clone())
}
