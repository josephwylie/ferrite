//! The Server's side of the Coordinator (#224): its Thread is opened as
//! `thread.start` opens one, in the Project's main checkout and owned by
//! the Factory; its settings reach its Session through the same doors the
//! Composer's pickers use; its proposals come from the API, and the
//! operator's accept or dismiss from a window or the CLI.

use serde_json::{json, Value};

use super::factory::done;
use super::Server;
use crate::api::schema::{
    CoordinatorSayParams, CoordinatorSetParams, ProposalRaiseParams, ProposedAct, TaskListParams,
};
use crate::api::{ApiError, Code};
use crate::engine::Launch;
use crate::factory::{Act, Agent, Answer, Command, Override};
use crate::protocol::{CoordinatorAction, Done, Op, WireError};
use crate::workspace::registry::ProjectId;
use crate::workspace::WorkspaceChoice;
use crate::{ThreadId, ThreadOwner};

impl Server {
    /// Open `project`'s Coordinator in its main checkout, owned by its
    /// Factory, and tell it `prompt`; the Factory hears how it went.
    pub(super) fn start_coordinator(
        &mut self,
        project: ProjectId,
        agent: Option<Agent>,
        prompt: String,
    ) {
        let agent = agent.unwrap_or_else(|| self.factory_agent.clone());
        let command = match self.open_coordinator(project, agent, prompt) {
            Ok(thread) => Command::CoordinatorStarted { project, thread },
            Err(why) => Command::CoordinatorFailed { project, why },
        };
        if let Err(refusal) = self.factory.command(command, self.now_ms()) {
            crate::log!("server: the Factory did not take a Coordinator's start: {refusal}");
        }
    }

    fn open_coordinator(
        &mut self,
        project: ProjectId,
        agent: Agent,
        prompt: String,
    ) -> Result<ThreadId, String> {
        let checkout = self
            .engine
            .registry()
            .project(project)
            .map(|project| project.root.clone())
            .ok_or("its Project is gone")?;
        let launch = Launch {
            project: Some(project),
            owner: Some(ThreadOwner::Factory(project)),
            ..Default::default()
        };
        self.open_owned(
            agent,
            WorkspaceChoice::Main { checkout },
            launch,
            Some("Coordinator".into()),
            prompt,
        )
    }

    /// A Coordinator's settings changed: its Session is tuned to them at
    /// its next rest.
    pub(super) fn retune(&mut self, thread: ThreadId) {
        self.coordinators_tuned.remove(&thread);
    }

    /// Each Coordinator at rest whose Session is not on its settings is
    /// tuned to them, through the doors the Composer's pickers use, so the
    /// change applies from its next turn. A refusal is not tried again
    /// until the settings change.
    pub(super) fn tune_coordinators(&mut self) {
        let wanted: Vec<(ThreadId, Agent)> = self
            .factory
            .coordinators()
            .filter_map(|coordinator| Some((coordinator.thread?, coordinator.agent.clone()?)))
            .collect();
        for (thread, agent) in wanted {
            if self.coordinators_tuned.get(&thread) == Some(&agent) {
                continue;
            }
            if !self.at_rest(thread) {
                continue;
            }
            let Some(view) = self.engine.thread(thread) else {
                continue;
            };
            let provider = view.provider() != agent.provider;
            let model = agent.model.is_some() && view.model() != agent.model.as_deref();
            let effort = agent.effort.is_some() && view.effort() != agent.effort.as_deref();
            let mut ops = Vec::new();
            if provider {
                ops.push(Op::SetProvider {
                    thread,
                    provider: agent.provider,
                    model: agent.model.clone(),
                });
            } else if model {
                ops.push(Op::SetModel {
                    thread,
                    model: agent.model.clone(),
                });
            }
            if effort {
                ops.push(Op::SetEffort {
                    thread,
                    effort: agent.effort.clone(),
                });
            }
            for op in ops {
                if let Err(error) = self.execute(op) {
                    crate::log!(
                        "server: the Coordinator could not be tuned: {}",
                        error.message
                    );
                }
            }
            self.coordinators_tuned.insert(thread, agent);
        }
    }

    /// Before a Coordinator is prompted: a parked one is revived; one
    /// that is gone is started afresh with `text` (false: nothing more to
    /// send).
    pub(super) fn wake_coordinator(&mut self, thread: ThreadId, text: &str) -> bool {
        if self.engine.thread(thread).is_some() {
            return true;
        }
        let Some(project) = self
            .factory
            .coordinators()
            .find(|coordinator| coordinator.thread == Some(thread))
            .map(|coordinator| coordinator.project)
        else {
            return true;
        };
        if self.execute(Op::Revive { thread }).is_ok() && self.engine.thread(thread).is_some() {
            return true;
        }
        let now = self.now_ms();
        let gone = Command::CoordinatorFailed {
            project,
            why: "its Thread is gone".into(),
        };
        let again = Command::CoordinatorSay {
            project,
            text: text.to_string(),
        };
        for command in [gone, again] {
            if let Err(refusal) = self.factory.command(command, now) {
                crate::log!("server: the Factory did not take a Coordinator's restart: {refusal}");
            }
        }
        false
    }

    /// A window's Coordinator command.
    pub(super) fn coordinator_command(
        &mut self,
        action: &CoordinatorAction,
    ) -> Result<Option<Done>, WireError> {
        let catalog = crate::factory::Catalog::default();
        let Some(mut command) =
            crate::factory::coordinator_command(action.clone(), &self.factory_agent, catalog)
        else {
            return Err(WireError {
                code: crate::protocol::ErrorCode::Unsupported,
                message: "this server does not know that Coordinator command".into(),
            });
        };
        if let Command::AcceptProposal {
            proposal, catalog, ..
        } = &mut command
        {
            let workflow = self.saved_by(proposal);
            let from_window = std::mem::take(&mut catalog.skills);
            *catalog = self.catalog(workflow.as_deref(), from_window);
        }
        let (answer, _) = self
            .factory_command(command)
            .map_err(super::factory::wire_error)?;
        Ok(Some(done(answer)))
    }

    // ------------------------------------------------------------ the API

    fn proposal_json(&self, proposal: &str) -> Result<Value, ApiError> {
        let board = self.factory.board();
        let found = board
            .proposal(proposal)
            .ok_or_else(|| ApiError::new(Code::UnknownProposal, "no proposal has that id"))?;
        let mut answer = json!(found);
        answer["title"] = json!(found.title(&board));
        answer["will"] = json!(found
            .lines(&board)
            .into_iter()
            .map(|(label, value)| format!("{label}: {value}"))
            .collect::<Vec<_>>());
        Ok(answer)
    }

    pub(super) fn api_proposal_raise(
        &mut self,
        params: ProposalRaiseParams,
    ) -> Result<Value, ApiError> {
        let needs = |what: Option<String>, name: &str| {
            what.ok_or_else(|| {
                ApiError::new(
                    Code::InvalidRequest,
                    format!("this proposal needs --{name}"),
                )
            })
        };
        let act = match params.act {
            ProposedAct::Start => Act::Start {
                instructions: needs(params.instructions, "instructions")?,
                workflow: params.workflow,
            },
            ProposedAct::Save => {
                let workflow = needs(params.workflow, "workflow")?;
                let found = self.factory.workflow(&workflow).ok_or_else(|| {
                    ApiError::new(Code::UnknownWorkflow, "no Workflow has that id")
                })?;
                Act::Save {
                    version: found.latest().map_or(1, |latest| latest.version + 1),
                    draft: found.draft.clone(),
                    workflow,
                }
            }
            ProposedAct::Automation => Act::Automation {
                workflow: needs(params.workflow, "workflow")?,
            },
            ProposedAct::Redirect => Act::Redirect {
                task: needs(params.task, "task")?,
                direction: Override {
                    to: needs(params.to, "to")?,
                    skip: params.skip,
                    then: params.then.unwrap_or_default(),
                    note: params.note.unwrap_or_default(),
                },
            },
        };
        let saves = match &act {
            Act::Save { workflow, .. } => Some(workflow.clone()),
            _ => None,
        };
        let propose = Command::Propose {
            coordinator: params.coordinator,
            act,
            catalog: self.catalog(saves.as_deref(), Vec::new()),
        };
        match self.factory_command(propose)? {
            (Answer::Proposal(proposal), _) => Ok(json!({
                "proposal": self.proposal_json(&proposal)?,
                "said": "Proposed. Nothing happens until the user accepts it in the Coordinator view.",
            })),
            _ => Err(ApiError::new(Code::Failed, "no proposal was raised")),
        }
    }

    pub(super) fn api_proposal_list(&self, params: TaskListParams) -> Result<Value, ApiError> {
        let project = params
            .project
            .as_deref()
            .map(|project| self.project(project))
            .transpose()?;
        let board = self.factory.board();
        let proposals = board
            .proposals
            .iter()
            .filter(|proposal| project.is_none_or(|project| proposal.project == project))
            .map(|proposal| self.proposal_json(&proposal.id))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({ "proposals": proposals }))
    }

    /// The Workflow `proposal` would save, if it is a save.
    fn saved_by(&self, proposal: &str) -> Option<String> {
        match &self.factory.proposal(proposal)?.act {
            Act::Save { workflow, .. } => Some(workflow.clone()),
            _ => None,
        }
    }

    /// Accept (its act runs as the operator's) or dismiss a proposal.
    pub(super) fn api_proposal_answer(
        &mut self,
        proposal: String,
        accept: bool,
    ) -> Result<Value, ApiError> {
        let command = if accept {
            let workflow = self.saved_by(&proposal);
            Command::AcceptProposal {
                proposal: proposal.clone(),
                agent: self.factory_agent.clone(),
                catalog: self.catalog(workflow.as_deref(), Vec::new()),
            }
        } else {
            Command::DismissProposal {
                proposal: proposal.clone(),
            }
        };
        let (answer, _) = self.factory_command(command)?;
        Ok(json!({
            "proposal": self.proposal_json(&proposal)?,
            "changed": answer != Answer::Changed(false),
        }))
    }

    fn coordinator_json(&self, project: ProjectId) -> Value {
        let board = self.factory.board();
        json!(board.coordinator(project))
    }

    pub(super) fn api_coordinator_say(
        &mut self,
        params: CoordinatorSayParams,
    ) -> Result<Value, ApiError> {
        let project = self.project_of(params.project.as_deref(), params.cwd.as_deref())?;
        self.factory_command(Command::CoordinatorSay {
            project,
            text: params.text,
        })?;
        Ok(json!({ "coordinator": self.coordinator_json(project) }))
    }

    pub(super) fn api_coordinator_set(
        &mut self,
        params: CoordinatorSetParams,
    ) -> Result<Value, ApiError> {
        let project = self.project_of(params.project.as_deref(), params.cwd.as_deref())?;
        let (answer, _) = self.factory_command(Command::CoordinatorAgent {
            project,
            agent: Agent {
                provider: params.provider,
                model: params.model.filter(|model| !model.is_empty()),
                effort: params.effort.filter(|effort| !effort.is_empty()),
            },
        })?;
        Ok(json!({
            "coordinator": self.coordinator_json(project),
            "changed": answer == Answer::Changed(true),
        }))
    }
}
