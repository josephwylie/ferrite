//! The Coordinator (#224, ADR 0037): one owned Thread per Project, in its
//! main checkout, for directing its Factory in conversation. Ferrite runs
//! no agent loop of its own: the Coordinator is an ordinary Provider
//! Thread, started on the operator's first word with a briefing that
//! teaches Ferrite's skill. It reads the board and drafts Workflow changes
//! freely; the four consequential acts (starting a Task, saving a Workflow
//! version, turning automatic starts on, redirecting a Task) are
//! [`Proposal`]s, stored here and drawn as cards, that change nothing
//! until the operator accepts one, which runs the act as theirs.

use serde::{Deserialize, Serialize};

use super::{
    Actor, Agent, Answer, Board, Catalog, Command, Definition, Effect, Override, Refusal,
    RefusalCode, State, Task, TaskId, ThenReturn, Turn, WorkflowId,
};
use crate::workspace::registry::ProjectId;
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
            } => format!("Save {} as version {version}", workflow_name(board, workflow)),
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
                let title = instructions.lines().next().unwrap_or_default().trim();
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
                if instructions.trim() != title {
                    lines.push(line("Instructions", instructions.trim().to_string()));
                }
                lines
            }
            Act::Save { draft, .. } => {
                let agents: Vec<&str> = draft.steps.iter().map(|step| step.name.as_str()).collect();
                vec![
                    line("Agents", agents.join(" \u{2192} ")),
                    line("Starts", draft.start.kind.label().to_string()),
                    line("Then", "New tasks use it; running tasks keep theirs. Saving starts nothing.".into()),
                ]
            }
            Act::Automation { workflow } => {
                let Some(found) = board.workflow(workflow) else {
                    return vec![line("Workflow", "A deleted workflow".into())];
                };
                let mut lines = vec![line("Workflow", found.draft.name.clone())];
                if let Some(latest) = found.latest() {
                    let start = &latest.definition.start;
                    lines[0].1 = format!("{} \u{b7} version {}", latest.definition.name, latest.version);
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

fn unknown_proposal() -> Refusal {
    Refusal::new(RefusalCode::UnknownProposal, "no proposal has that id")
}

/// How the Coordinator raises a proposal.
const PROPOSE: &str = "\"${FERRITE_BIN:-ferrite}\" proposal raise";
const F: &str = "\"${FERRITE_BIN:-ferrite}\"";

/// What the Coordinator is told first, before the operator's words.
pub fn briefing(project: ProjectId) -> String {
    let project = project.get();
    format!(
        "You are the Coordinator of this Project's Ferrite Factory (Project {project}). The \
         operator talks with you about priorities and requirements and hands work to the \
         Factory through you. Ferrite's skill teaches its commands: read it with `{F} --skill`.\n\n\
         ## Read and draft freely\n\n\
         Read the board, a Task or a Workflow whenever it helps:\n\n    \
         {F} task list --project {project}\n    {F} task get t-1\n    \
         {F} workflow list --project {project}\n    {F} workflow get w-1\n\n\
         Draft Workflow changes when the operator asks; a draft runs nothing:\n\n    \
         {F} workflow create --project {project}\n    {F} step add w-1\n    \
         {F} step edit w-1 s-1 --instructions \"…\"\n    {F} step move w-1 s-2 0\n    \
         {F} workflow approval w-1 --by human\n    {F} workflow start w-1 '{{\"type\":\"manual\"}}'\n\n\
         ## Propose; never act\n\n\
         You never act without the operator. Four acts are proposals, shown to the operator \
         as a card they accept or dismiss; nothing happens until they accept:\n\n    \
         {PROPOSE} start --instructions \"<what to do; first line is the title>\" [--workflow w-1]\n    \
         {PROPOSE} save --workflow w-1\n    \
         {PROPOSE} automation --workflow w-1\n    \
         {PROPOSE} redirect --task t-1 --to s-3 [--skip s-2] [--then me] [--note \"…\"]\n\n\
         A one-off handoff (`start` without `--workflow`) needs no Workflow. Never run \
         `task start`, `workflow save`, `workflow automation`, `task redirect`, or answer, \
         approve, rework, stop or delete a Task yourself. Talking starts nothing: propose \
         only what the operator wants, say what the card will do, and let them decide. \
         `{F} proposal list --project {project}` shows what was accepted or dismissed.\n\n\
         The operator says:\n\n"
    )
}

/// What the Coordinator is told of a Task a route sent to it.
pub(super) fn routed(task: &Task, from: &str, text: &str) -> String {
    let said = if text.trim().is_empty() {
        String::new()
    } else {
        format!(" It said: {}", text.trim())
    };
    format!(
        "Factory task {id} (\"{title}\") was sent to you: {from} routed it to the \
         Coordinator.{said}\n\nIt waits in Needs human. Read it with `{F} task get {id}`, \
         talk it over with the operator when they come, and propose where it goes next \
         (`{PROPOSE} redirect --task {id} …`). Do nothing else with it.",
        id = task.id,
        title = task.title,
    )
}

impl State {
    fn coordinator_mut(&mut self, project: ProjectId) -> &mut Coordinator {
        if let Some(at) = self.coordinators.iter().position(|c| c.project == project) {
            return &mut self.coordinators[at];
        }
        self.coordinators.push(Coordinator {
            project,
            thread: None,
            agent: None,
            starting: false,
            waiting: Vec::new(),
            error: None,
        });
        self.coordinators.last_mut().expect("just pushed")
    }

    /// Tell `project`'s Coordinator `text`, starting it (with its
    /// briefing) when it has no Thread.
    pub(super) fn tell_coordinator(
        &mut self,
        project: ProjectId,
        text: String,
        effects: &mut Vec<Effect>,
    ) {
        let coordinator = self.coordinator_mut(project);
        if let Some(thread) = coordinator.thread {
            effects.push(Effect::Prompt { thread, text });
        } else if coordinator.starting {
            coordinator.waiting.push(text);
        } else {
            coordinator.starting = true;
            coordinator.error = None;
            effects.push(Effect::StartCoordinator {
                project,
                agent: coordinator.agent.clone(),
                prompt: format!("{}{text}", briefing(project)),
            });
        }
    }

    /// Tasks routed to the Coordinator since the last step: it is told.
    pub(super) fn step_coordinators(&mut self, effects: &mut Vec<Effect>) {
        let told: Vec<(ProjectId, String)> = self
            .tasks
            .iter_mut()
            .filter_map(|task| Some((task.project, task.oversight.consult.take()?)))
            .collect();
        for (project, text) in told {
            self.tell_coordinator(project, text, effects);
        }
    }

    /// The project a Coordinator's Thread directs.
    fn coordinator_of(&self, thread: ThreadId) -> Result<ProjectId, Refusal> {
        self.coordinators
            .iter()
            .find(|c| c.thread == Some(thread))
            .map(|c| c.project)
            .ok_or_else(|| {
                Refusal::new(
                    RefusalCode::NotACoordinator,
                    "this Thread is no Project's Coordinator",
                )
            })
    }

    /// The command accepting `act` runs, as the operator's.
    fn act_command(act: &Act, project: ProjectId, agent: Agent, catalog: Catalog) -> Command {
        match act.clone() {
            Act::Start {
                instructions,
                workflow,
            } => Command::Start {
                project,
                instructions,
                agent,
                request: None,
                workflow,
            },
            Act::Save { workflow, .. } => Command::SaveWorkflow { workflow, catalog },
            Act::Automation { workflow } => Command::Automation {
                workflow,
                turn: Turn::On,
            },
            Act::Redirect { task, direction } => Command::Redirect { task, direction },
        }
    }

    /// `act` is `project`'s and would happen now, as the card says.
    fn check_act(
        &self,
        act: &Act,
        project: ProjectId,
        catalog: &Catalog,
        now_ms: i64,
    ) -> Result<(), Refusal> {
        let invalid = |message: &str| Refusal::new(RefusalCode::Invalid, message);
        let workflow_of = |id: &str| {
            self.workflows
                .iter()
                .find(|found| found.id == id)
                .ok_or_else(|| Refusal::new(RefusalCode::UnknownWorkflow, "no Workflow has that id"))
                .and_then(|found| {
                    (found.project == project)
                        .then_some(found)
                        .ok_or_else(|| invalid("that Workflow is another Project's"))
                })
        };
        match act {
            Act::Start { workflow, .. } => {
                if let Some(id) = workflow {
                    workflow_of(id)?;
                }
            }
            Act::Save {
                workflow, draft, ..
            } => {
                let found = workflow_of(workflow)?;
                if !found.unsaved() {
                    return Err(invalid("the draft has no changes to save"));
                }
                if found.draft != *draft {
                    return Err(Refusal::new(
                        RefusalCode::NotNow,
                        "the draft changed after this was proposed: ask for it again",
                    ));
                }
            }
            Act::Automation { workflow } => {
                if workflow_of(workflow)?.automation.switch == super::Switch::On {
                    return Err(invalid("its automatic starts are already on"));
                }
            }
            Act::Redirect { task, .. } => {
                let found = self
                    .tasks
                    .iter()
                    .find(|found| found.id == *task)
                    .ok_or_else(super::unknown_task)?;
                if found.project != project {
                    return Err(invalid("that Task is another Project's"));
                }
            }
        }
        // The act itself, tried on a copy: what it would refuse, it
        // refuses now.
        let mut trial = self.clone();
        let agent = Agent {
            provider: crate::store::Provider::Claude,
            model: None,
            effort: None,
        };
        trial.command(
            Self::act_command(act, project, agent, catalog.clone()),
            now_ms,
            &mut Vec::new(),
        )?;
        Ok(())
    }

    fn proposal_mut(&mut self, id: &str) -> Result<&mut Proposal, Refusal> {
        self.proposals
            .iter_mut()
            .find(|proposal| proposal.id == id)
            .ok_or_else(unknown_proposal)
    }

    /// A Coordinator command.
    pub(super) fn coordinate(
        &mut self,
        command: Command,
        now_ms: i64,
        effects: &mut Vec<Effect>,
    ) -> Result<Answer, Refusal> {
        match command {
            Command::CoordinatorSay { project, text } => {
                let text = text.trim().to_string();
                if text.is_empty() {
                    return Err(Refusal::new(RefusalCode::Invalid, "say something"));
                }
                self.tell_coordinator(project, text, effects);
                Ok(Answer::Changed(true))
            }
            Command::CoordinatorStarted { project, thread } => {
                let coordinator = self.coordinator_mut(project);
                if coordinator.thread == Some(thread) {
                    return Ok(Answer::Changed(false));
                }
                coordinator.thread = Some(thread);
                coordinator.starting = false;
                coordinator.error = None;
                for text in std::mem::take(&mut coordinator.waiting) {
                    effects.push(Effect::Prompt { thread, text });
                }
                Ok(Answer::Changed(true))
            }
            Command::CoordinatorFailed { project, why } => {
                let coordinator = self.coordinator_mut(project);
                coordinator.thread = None;
                coordinator.starting = false;
                coordinator.waiting.clear();
                coordinator.error = Some(why);
                Ok(Answer::Changed(true))
            }
            Command::CoordinatorAgent { project, agent } => {
                let coordinator = self.coordinator_mut(project);
                if coordinator.agent.as_ref() == Some(&agent) {
                    return Ok(Answer::Changed(false));
                }
                coordinator.agent = Some(agent.clone());
                if let Some(thread) = coordinator.thread {
                    effects.push(Effect::Tune { thread, agent });
                }
                Ok(Answer::Changed(true))
            }
            Command::Propose {
                coordinator,
                act,
                catalog,
            } => {
                let project = self.coordinator_of(coordinator)?;
                if let Act::Start { instructions, .. } = &act {
                    if instructions.trim().is_empty() {
                        return Err(Refusal::new(RefusalCode::Invalid, "a Task needs instructions"));
                    }
                }
                if let Some(same) = self.proposals.iter().find(|proposal| {
                    proposal.status == ProposalStatus::Pending
                        && proposal.project == project
                        && proposal.act == act
                }) {
                    return Ok(Answer::Proposal(same.id.clone()));
                }
                self.check_act(&act, project, &catalog, now_ms)?;
                let id = format!("p-{}", self.next_proposal);
                self.next_proposal += 1;
                self.proposals.push(Proposal {
                    id: id.clone(),
                    project,
                    act,
                    status: ProposalStatus::Pending,
                    raised_ms: now_ms,
                    task: None,
                });
                Ok(Answer::Proposal(id))
            }
            Command::AcceptProposal {
                proposal,
                agent,
                catalog,
            } => {
                let found = self.proposal_mut(&proposal)?.clone();
                match found.status {
                    ProposalStatus::Accepted => return Ok(Answer::Changed(false)),
                    ProposalStatus::Dismissed => {
                        return Err(Refusal::new(
                            RefusalCode::NotNow,
                            "the proposal was dismissed",
                        ))
                    }
                    ProposalStatus::Pending => {}
                }
                self.check_act(&found.act, found.project, &catalog, now_ms)?;
                let answer = self.command(
                    Self::act_command(&found.act, found.project, agent, catalog),
                    now_ms,
                    effects,
                )?;
                let made = match &answer {
                    Answer::Task(task) => Some(task.clone()),
                    _ => None,
                };
                if let Some(task) = made
                    .as_ref()
                    .and_then(|id| self.tasks.iter_mut().find(|task| task.id == *id))
                {
                    task.note(now_ms, Actor::Ferrite, "The Coordinator proposed it");
                }
                let accepted = self.proposal_mut(&proposal)?;
                accepted.status = ProposalStatus::Accepted;
                accepted.task = made;
                Ok(answer)
            }
            Command::DismissProposal { proposal } => {
                let found = self.proposal_mut(&proposal)?;
                match found.status {
                    ProposalStatus::Pending => {
                        found.status = ProposalStatus::Dismissed;
                        Ok(Answer::Changed(true))
                    }
                    ProposalStatus::Dismissed => Ok(Answer::Changed(false)),
                    ProposalStatus::Accepted => Err(Refusal::new(
                        RefusalCode::NotNow,
                        "the proposal was already accepted",
                    )),
                }
            }
            _ => unreachable!("only the Coordinator's commands"),
        }
    }
}
