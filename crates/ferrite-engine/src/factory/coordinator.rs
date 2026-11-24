//! The Coordinator (#224, ADR 0037): one owned Thread per Project, in its
//! main checkout, for directing its Factory in conversation. Ferrite runs
//! no agent loop of its own: the Coordinator is an ordinary Provider
//! Thread, started on the operator's first word with a briefing that
//! teaches Ferrite's skill. It reads the board and drafts Workflow changes
//! freely; the four consequential acts (starting a Task, saving a Workflow
//! version, turning automatic starts on, redirecting a Task) are
//! [`Proposal`]s, stored here and drawn as cards, that change nothing
//! until the operator accepts one, which runs the act as theirs.

use super::{Answer, Command, Effect, Refusal, RefusalCode, State, Transitions};
use ferrite_core::factory_board::{
    Act, Actor, Agent, Catalog, Coordinator, Proposal, ProposalId, ProposalStatus, Task, Turn,
};
use ferrite_core::project::ProjectId;
use ferrite_core::ThreadId;

pub(super) fn unknown_proposal() -> Refusal {
    Refusal::new(RefusalCode::UnknownProposal, "no proposal has that id")
}

/// How the Coordinator raises a proposal.
const PROPOSE: &str = "\"${FERRITE_BIN:-ferrite}\" proposal raise";
const F: &str = "\"${FERRITE_BIN:-ferrite}\"";

/// What the Coordinator is told: the instructions of its Session, never a
/// message it shows as the operator's.
pub fn briefing(project: ProjectId) -> String {
    let project = project.get();
    format!(
        "You are the Coordinator of this Project's Ferrite Factory (Project {project}). The \
         operator talks with you about priorities and requirements and hands work to the \
         Factory through you. Ferrite's skill teaches its commands: read it with `{F} --skill`.\n\n\
         ## Read and draft freely\n\n\
         Read the board, a Task or a Workflow whenever it helps, with your own tools, which \
         need no permission: `task_list`, `task_get`, `workflow_list`, `workflow_get` and \
         `proposal_list` (what was accepted or dismissed). Run the commands only if those \
         tools are not offered to you:\n\n    \
         {F} task list --project {project}\n    {F} task get t-1\n    \
         {F} workflow list --project {project}\n    {F} workflow get w-1\n    \
         {F} proposal list --project {project}\n\n\
         Draft Workflow changes when the operator asks; a draft runs nothing:\n\n    \
         {F} workflow create --project {project}\n    {F} step add w-1\n    \
         {F} step edit w-1 s-1 --instructions \"…\"\n    {F} step move w-1 s-2 0\n    \
         {F} workflow approval w-1 --by human\n    {F} workflow start w-1 '{{\"type\":\"manual\"}}'\n\n\
         ## Propose; never act\n\n\
         You never act without the operator. Four acts are proposals, shown to the operator \
         as a card they accept or dismiss; nothing happens until they accept. Raise one with \
         your own `proposal_raise` tool, which needs no permission: its `act` and the flags \
         below as its fields. Run the command only if that tool is not offered to you:\n\n    \
         {PROPOSE} start --instructions \"<what to do; first line is the title>\" [--workflow w-1]\n    \
         {PROPOSE} save --workflow w-1\n    \
         {PROPOSE} automation --workflow w-1\n    \
         {PROPOSE} redirect --task t-1 --to s-3 [--skip s-2] [--then me] [--note \"…\"]\n\n\
         A one-off handoff (`start` without `--workflow`) needs no Workflow. Never run \
         `task start`, `workflow save`, `workflow automation`, `task redirect`, or answer, \
         approve, rework, stop or delete a Task yourself. Talking starts nothing: propose \
         only what the operator wants, say what the card will do, and let them decide."
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
         Coordinator.{said}\n\nIt waits in Needs human. Read it with your `task_get` tool \
         (or `{F} task get {id}`), talk it over with the operator when they come, and \
         propose where it goes next \
         (`{PROPOSE} redirect --task {id} …`). Do nothing else with it.",
        id = task.id,
        title = task.title,
    )
}

/// A window's ask of a Coordinator as the command it is: an accepted
/// one-off on `agent`, a save checked against `catalog` (whose skills are
/// the window's). `None` for an ask this Ferrite does not know.
pub fn coordinator_command(
    action: ferrite_core::protocol::CoordinatorAction,
    agent: &Agent,
    mut catalog: Catalog,
) -> Option<Command> {
    use ferrite_core::protocol::CoordinatorAction;
    Some(match action {
        CoordinatorAction::Say { project, text } => Command::CoordinatorSay { project, text },
        CoordinatorAction::Agent { project, agent } => Command::CoordinatorAgent { project, agent },
        CoordinatorAction::Accept { proposal, skills } => {
            catalog.skills = skills;
            Command::AcceptProposal {
                proposal,
                agent: agent.clone(),
                catalog,
            }
        }
        CoordinatorAction::Dismiss { proposal } => Command::DismissProposal { proposal },
        CoordinatorAction::Unknown => return None,
    })
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

    /// Tell `project`'s Coordinator `text`, starting it when it has no
    /// Thread (its Session carries its briefing).
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
                prompt: text,
            });
        }
    }

    /// Tasks routed to the Coordinator since the last step: it is told.
    pub(super) fn step_coordinators(&mut self, effects: &mut Vec<Effect>) -> bool {
        let told: Vec<(ProjectId, String)> = self
            .tasks
            .iter_mut()
            .filter_map(|task| Some((task.project, task.oversight.consult.take()?)))
            .collect();
        let changed = !told.is_empty();
        for (project, text) in told {
            self.tell_coordinator(project, text, effects);
        }
        changed
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
                pr: None,
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
                .ok_or_else(|| {
                    Refusal::new(RefusalCode::UnknownWorkflow, "no Workflow has that id")
                })
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
                if workflow_of(workflow)?.automation.switch == ferrite_core::factory_board::Switch::On {
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
            provider: ferrite_core::provider::Provider::Claude,
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

    pub(super) fn coordinator_say(
        &mut self,
        project: ProjectId,
        text: String,
        effects: &mut Vec<Effect>,
    ) -> Result<Answer, Refusal> {
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err(Refusal::new(RefusalCode::Invalid, "say something"));
        }
        self.tell_coordinator(project, text, effects);
        Ok(Answer::Changed(true))
    }

    pub(super) fn coordinator_started(
        &mut self,
        project: ProjectId,
        thread: ThreadId,
        effects: &mut Vec<Effect>,
    ) -> Result<Answer, Refusal> {
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

    pub(super) fn coordinator_failed(
        &mut self,
        project: ProjectId,
        why: String,
    ) -> Result<Answer, Refusal> {
        let coordinator = self.coordinator_mut(project);
        coordinator.thread = None;
        coordinator.starting = false;
        coordinator.waiting.clear();
        coordinator.error = Some(why);
        Ok(Answer::Changed(true))
    }

    pub(super) fn coordinator_agent(
        &mut self,
        project: ProjectId,
        agent: Agent,
        effects: &mut Vec<Effect>,
    ) -> Result<Answer, Refusal> {
        let coordinator = self.coordinator_mut(project);
        if coordinator.agent.as_ref() == Some(&agent) {
            return Ok(Answer::Changed(false));
        }
        coordinator.agent = Some(agent.clone());
        if let Some(thread) = coordinator.thread {
            effects.push(Effect::Tune { thread });
        }
        Ok(Answer::Changed(true))
    }

    pub(super) fn propose(
        &mut self,
        coordinator: ThreadId,
        act: Act,
        catalog: Catalog,
        now_ms: i64,
    ) -> Result<Answer, Refusal> {
        let project = self.coordinator_of(coordinator)?;
        if let Act::Start { instructions, .. } = &act {
            if instructions.trim().is_empty() {
                return Err(Refusal::new(
                    RefusalCode::Invalid,
                    "a Task needs instructions",
                ));
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

    pub(super) fn accept_proposal(
        &mut self,
        proposal: ProposalId,
        agent: Agent,
        catalog: Catalog,
        now_ms: i64,
        effects: &mut Vec<Effect>,
    ) -> Result<Answer, Refusal> {
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
        if let Act::Redirect { task, direction } = &found.act {
            if let Some(task) = self.tasks.iter_mut().find(|each| each.id == *task) {
                let to = task
                    .workflow
                    .as_ref()
                    .and_then(|run| run.definition.step(&direction.to))
                    .map_or_else(|| direction.to.clone(), |step| step.name.clone());
                task.note(
                    now_ms,
                    Actor::Coordinator,
                    format!("Proposed sending this task to {to}"),
                );
            }
        }
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
            task.note(now_ms, Actor::Coordinator, "Proposed this task");
            task.note(
                now_ms,
                Actor::Operator,
                "Accepted the Coordinator's proposal",
            );
        }
        let accepted = self.proposal_mut(&proposal)?;
        accepted.status = ProposalStatus::Accepted;
        accepted.task = made;
        Ok(answer)
    }

    pub(super) fn dismiss_proposal(&mut self, proposal: ProposalId) -> Result<Answer, Refusal> {
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
}
