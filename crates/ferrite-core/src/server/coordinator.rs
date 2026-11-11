//! The Server's side of the Coordinator (#224): its Thread is opened as
//! `thread.start` opens one, in the Project's main checkout and owned by
//! the Factory; its settings reach its Session through the same doors the
//! Composer's pickers use; its proposals come from the API, and the
//! operator's accept or dismiss from a window or the CLI.

use serde_json::{json, Value};

use super::factory::done;
use super::Server;
use crate::api::schema::{
    AgentPromptParams, CoordinatorSayParams, CoordinatorSetParams, ProposalRaiseParams,
    ProposedAct, TaskListParams,
};
use crate::api::{ApiError, Code};
use crate::engine::{Launch, ProviderChoice};
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
        if let Err(refusal) = self.factory.command(command, (self.clock)()) {
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
        let thread = self
            .engine
            .open_tuned(
                ProviderChoice {
                    provider: agent.provider,
                    model: agent.model,
                },
                agent.effort,
                WorkspaceChoice::Main { checkout },
                launch,
            )
            .map_err(|error| error.to_string())?;
        let _ = self.execute(Op::Rename {
            thread,
            title: "Coordinator".into(),
        });
        self.prompt(&AgentPromptParams {
            thread,
            text: prompt,
            wait: false,
            timeout_ms: None,
            until: Vec::new(),
        })
        .map_err(|error| error.message)?;
        Ok(thread)
    }

    /// An open Thread's Provider, model and effort, from its next turn.
    pub(super) fn tune(&mut self, thread: ThreadId, agent: Agent) {
        let Some(view) = self.engine.thread(thread) else {
            return;
        };
        let provider = view.provider();
        let mut ops = Vec::new();
        if provider != agent.provider {
            ops.push(Op::SetProvider {
                thread,
                provider: agent.provider,
                model: agent.model.clone(),
            });
        } else {
            ops.push(Op::SetModel {
                thread,
                model: agent.model.clone(),
            });
        }
        ops.push(Op::SetEffort {
            thread,
            effort: agent.effort,
        });
        for op in ops {
            if let Err(error) = self.execute(op) {
                crate::log!("server: the Coordinator could not be tuned: {}", error.message);
            }
        }
    }

    /// A window's Coordinator command.
    pub(super) fn coordinator_command(
        &mut self,
        action: &CoordinatorAction,
    ) -> Result<Option<Done>, WireError> {
        let wire = |error: ApiError| WireError {
            code: crate::protocol::ErrorCode::Failed,
            message: error.message,
        };
        let command = match action.clone() {
            CoordinatorAction::Say { project, text } => Command::CoordinatorSay { project, text },
            CoordinatorAction::Agent { project, agent } => {
                Command::CoordinatorAgent { project, agent }
            }
            CoordinatorAction::Accept { proposal, skills } => Command::AcceptProposal {
                proposal,
                agent: self.factory_agent.clone(),
                catalog: self.catalog(skills),
            },
            CoordinatorAction::Dismiss { proposal } => Command::DismissProposal { proposal },
            CoordinatorAction::Unknown => {
                return Err(WireError {
                    code: crate::protocol::ErrorCode::Unsupported,
                    message: "this server does not know that Coordinator command".into(),
                })
            }
        };
        let (answer, _) = self.factory_command(command).map_err(wire)?;
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
        let propose = Command::Propose {
            coordinator: params.coordinator,
            act,
            catalog: self.catalog(Vec::new()),
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

    /// Accept (its act runs as the operator's) or dismiss a proposal.
    pub(super) fn api_proposal_answer(
        &mut self,
        proposal: String,
        accept: bool,
    ) -> Result<Value, ApiError> {
        let command = if accept {
            Command::AcceptProposal {
                proposal: proposal.clone(),
                agent: self.factory_agent.clone(),
                catalog: self.catalog(Vec::new()),
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
