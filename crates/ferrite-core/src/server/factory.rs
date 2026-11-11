//! The Server's side of the Factory (ADR 0037). It steps the module once
//! a pass with what each Worker's Thread is doing, and carries out the
//! effects through the same doors as any caller's act: a Worker is opened
//! as `thread.start` opens one, prompted as `agent.prompt` prompts, parked
//! as `thread.close` parks and its worktree removed as `worktree.remove`
//! removes one. Its commands come from the API, the windows and the
//! Workers' reports, one door each.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::Server;
use crate::api::schema::{
    AgentPromptParams, ReportOutcomeParams, ReportQuestionParams, ReportedOutcome, TaskClearParams,
    TaskListParams, TaskParams, TaskRedirectParams, TaskReworkParams, TaskStartParams,
    TaskSubmitParams, WorkflowCreateParams,
};
use crate::api::{ApiError, Code};
use crate::engine::{Launch, ProviderChoice};
use crate::factory::{self, Agent, Answer, Command, Effect, Outcome, Seen};
use crate::protocol::{Done, ServerMessage, TaskAction, WireError, WorkflowAction};
use crate::settings::Settings;
use crate::workspace::registry::ProjectId;
use crate::workspace::{self, WorkspaceBinding, WorkspaceChoice};
use crate::{Attention, ThreadId, ThreadOwner};

/// The Factory kept in the Store's directory. One whose file cannot be
/// read says so on the board and in every refusal, and does nothing: it
/// is never started afresh over the Tasks it holds, nor written (#226).
pub(super) fn load(store_dir: &std::path::Path) -> factory::Factory {
    let factory = factory::Factory::open(store_dir);
    let board = factory.changes();
    for trouble in board.trouble.iter().chain(board.troubles.values()) {
        crate::log!("server: {trouble}");
    }
    factory
}

/// A Factory refusal as a window's error.
pub(super) fn wire_error(error: ApiError) -> WireError {
    WireError {
        code: crate::protocol::ErrorCode::Failed,
        message: error.message,
    }
}

pub(super) use crate::factory::done;

/// A Provider Decision as a Task says it: the Provider's own summary,
/// else the tool it gates.
fn decision_line(decision: &crate::protocol::WireDecision) -> String {
    let said = decision.description.trim();
    if said.is_empty() {
        decision.tool_name.clone()
    } else {
        said.to_string()
    }
}

/// A one-off Task's Worker: the operator's default Provider, its model
/// and Effort.
pub(super) fn default_agent(settings: &Settings) -> Agent {
    let provider = settings.default_provider;
    let (model, effort) = match provider {
        crate::store::Provider::Claude => (&settings.claude_model, &settings.claude_effort),
        crate::store::Provider::Codex => (&settings.codex_model, &settings.codex_effort),
    };
    Agent {
        provider,
        model: model.clone(),
        effort: effort.clone(),
    }
}

impl Server {
    /// The Factory's time now: its one clock, which tests script.
    pub(super) fn now_ms(&self) -> i64 {
        (self.clock)()
    }

    /// One pass of the Factory, and its effects carried out. What a
    /// deleted Task's worktree removal left behind is said back.
    pub(super) fn step_factory(&mut self) -> Vec<String> {
        let mut kept = Vec::new();
        for heard in self.github_answers() {
            if let Err(refusal) = self.factory.command(heard, self.now_ms()) {
                crate::log!("server: the Factory did not take a GitHub read: {refusal}");
            }
        }
        // Carrying out an effect may answer with a command (a Worker
        // started), whose effects the next round carries out.
        for _ in 0..4 {
            let workers = self.factory_workers();
            let effects = self.factory.step(self.now_ms(), &workers);
            if effects.is_empty() {
                break;
            }
            for effect in effects {
                if let Some(note) = self.carry_out(effect) {
                    kept.push(note);
                }
            }
        }
        self.tune_coordinators();
        kept
    }

    /// What each open Worker's Thread is doing.
    fn factory_workers(&self) -> BTreeMap<ThreadId, Seen> {
        let decisions = self.engine.unanswered_decisions();
        self.factory
            .watched()
            .filter_map(|thread| {
                let view = self.engine.thread(thread)?;
                self.engine.attention(thread)?;
                let busy = !self.at_rest(thread);
                let turns = self.engine.completion_seq(thread).unwrap_or(0);
                // A Provider Decision in its Session is the Task's to
                // answer (#222), never the Inbox's.
                let decision = decisions
                    .iter()
                    .find(|pending| pending.thread == thread)
                    .map(|pending| decision_line(&pending.decision));
                // Ended at rest, a Session leaves its Activity connected
                // until the next starts; one that closed on its own, or a
                // Thread the restart reopened, has neither (#226).
                let lost = !busy && !view.has_session() && !view.activity().connected();
                Some((
                    thread,
                    Seen {
                        busy,
                        turns,
                        decision,
                        lost,
                    },
                ))
            })
            .collect()
    }

    /// One command, and at once what it set off.
    pub(super) fn factory_command(
        &mut self,
        command: Command,
    ) -> Result<(Answer, Vec<String>), ApiError> {
        let answer = self.factory.command(command, self.now_ms())?;
        let kept = self.step_factory();
        Ok((answer, kept))
    }

    fn carry_out(&mut self, effect: Effect) -> Option<String> {
        match effect {
            Effect::StartWorker {
                task,
                project,
                agent,
                branch,
                worktree,
                prompt,
            } => {
                let command =
                    match self.start_worker(&task, project, agent, branch, worktree, prompt) {
                        Ok((thread, worktree)) => Command::WorkerStarted {
                            task,
                            thread,
                            worktree,
                        },
                        Err(why) => Command::WorkerFailed { task, why },
                    };
                if let Err(refusal) = self.factory.command(command, self.now_ms()) {
                    crate::log!("server: the Factory did not take a Worker's start: {refusal}");
                }
            }
            Effect::Prompt { thread, text } => {
                if !self.wake_coordinator(thread, &text) {
                    return None;
                }
                let prompt = AgentPromptParams {
                    thread,
                    text,
                    wait: false,
                    timeout_ms: None,
                    until: Vec::new(),
                };
                if let Err(error) = self.prompt(&prompt) {
                    crate::log!("server: a Worker could not be prompted: {}", error.message);
                }
            }
            Effect::Interrupt { thread } => {
                if self.engine.thread(thread).is_some() {
                    if let Err(error) = self.execute(crate::protocol::Op::Interrupt { thread }) {
                        crate::log!(
                            "server: a Worker could not be interrupted: {}",
                            error.message
                        );
                    }
                }
            }
            Effect::Park { thread } => {
                if self.engine.thread(thread).is_some() {
                    if let Err(error) = self.park(thread) {
                        crate::log!("server: a Worker could not be parked: {}", error.message);
                    }
                }
            }
            Effect::DeleteThread { thread } => {
                if self.engine.groups().of(thread).is_some() {
                    let _ = self.execute(crate::protocol::Op::Group {
                        change: (&crate::groups::GroupChange::Leave { thread }).into(),
                    });
                }
                if let Err(error) = self.execute(crate::protocol::Op::Delete { thread }) {
                    crate::log!("server: a Worker could not be deleted: {}", error.message);
                }
            }
            Effect::RemoveWorktree { path } => return self.remove_task_worktree(&path),
            Effect::ReadGitHub {
                workflow,
                version,
                query,
            } => self.read_github(workflow, version, query),
            Effect::Tidy { task, path } => {
                let kept = self.remove_task_worktree(&path);
                let tidied = Command::WorktreeTidied { task, kept };
                if let Err(refusal) = self.factory.command(tidied, self.now_ms()) {
                    crate::log!("server: the Factory did not take a worktree's removal: {refusal}");
                }
            }
            Effect::StartCoordinator {
                project,
                agent,
                prompt,
            } => self.start_coordinator(project, agent, prompt),
            Effect::Tune { thread } => self.retune(thread),
            Effect::Notify {
                task,
                project,
                status,
                title,
            } => self.say(ServerMessage::TaskAlert {
                project,
                task,
                status,
                title,
            }),
        }
        None
    }

    /// Open the Task's Worker in its worktree (a new one of its Project on
    /// its branch, when it has none yet), owned by it, titled as it and
    /// its Step, and give it its first prompt.
    fn start_worker(
        &mut self,
        task: &str,
        project: ProjectId,
        agent: Agent,
        branch: String,
        worktree: Option<std::path::PathBuf>,
        prompt: String,
    ) -> Result<(ThreadId, Option<std::path::PathBuf>), String> {
        let repo = self
            .engine
            .registry()
            .project(project)
            .map(|project| project.root.clone())
            .ok_or("its Project is gone")?;
        let title = self
            .factory
            .task(task)
            .map(|task| match task.current_step() {
                Some(step) => format!("{} \u{b7} {}", task.title, step.name),
                None => task.title.clone(),
            });
        let workspace = match worktree.filter(|path| path.exists()) {
            Some(path) => WorkspaceChoice::ExistingWorktree { repo, path },
            None => WorkspaceChoice::NamedWorktree {
                repo,
                branch,
                base: None,
            },
        };
        let launch = Launch {
            project: Some(project),
            owner: Some(ThreadOwner::Task(task.to_string())),
            ..Default::default()
        };
        let thread = self.open_owned(agent, workspace, launch, title, prompt)?;
        let worktree = match self.engine.thread(thread).and_then(|view| view.workspace()) {
            Some(WorkspaceBinding::Worktree { path, .. }) => Some(path.clone()),
            _ => None,
        };
        Ok((thread, worktree))
    }

    /// Open a Thread the Factory owns on `agent`'s Provider, model and
    /// Effort, title it, and give it its first prompt: a Worker or a
    /// Coordinator.
    pub(super) fn open_owned(
        &mut self,
        agent: Agent,
        workspace: WorkspaceChoice,
        launch: Launch,
        title: Option<String>,
        prompt: String,
    ) -> Result<ThreadId, String> {
        let thread = self
            .engine
            .open_choice(
                ProviderChoice {
                    provider: agent.provider,
                    model: agent.model,
                },
                workspace,
                agent.effort,
                launch,
            )
            .map_err(|error| error.to_string())?;
        if let Some(title) = title {
            let _ = self.execute(crate::protocol::Op::Rename { thread, title });
        }
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

    /// At rest: its turn over, nothing of its Session running or held.
    pub(super) fn at_rest(&self, thread: ThreadId) -> bool {
        matches!(
            self.engine.attention(thread),
            Some(Attention::Idle | Attention::Done)
        ) && !self.engine.holds_prompt(thread)
            && self
                .engine
                .thread(thread)
                .is_some_and(|view| !view.starting())
    }

    /// A deleted Task's worktree goes only when its checkout is clean;
    /// otherwise it stays, and why is said. Its branch always stays.
    fn remove_task_worktree(&mut self, path: &std::path::Path) -> Option<String> {
        if !path.exists() {
            return None;
        }
        match workspace::is_clean(path) {
            Ok(true) => match self.remove_worktree(path) {
                Ok(_) => None,
                Err(error) => Some(format!(
                    "The worktree {} was left in place: {}",
                    path.display(),
                    error.message
                )),
            },
            Ok(false) => Some(format!(
                "The worktree {} has uncommitted changes, so it was left in place.",
                path.display()
            )),
            Err(error) => Some(format!(
                "The worktree {} was left in place: {error}",
                path.display()
            )),
        }
    }

    /// What the operator says to a Worker from a window is its Task's.
    pub(super) fn heard_operator(&mut self, thread: ThreadId, text: &str) {
        if self.factory.task_of(thread).is_none() {
            return;
        }
        let said = Command::OperatorSaid {
            worker: thread,
            text: text.to_string(),
        };
        if let Err(error) = self.factory_command(said) {
            crate::log!(
                "server: the Factory did not take what was said: {}",
                error.message
            );
        }
    }

    /// The board, to every window, when it changed.
    pub(super) fn factory_changes(&mut self) -> Option<ServerMessage> {
        let revision = self.factory.revision();
        (self.told_factory != Some(revision)).then(|| {
            self.told_factory = Some(revision);
            // The Archive goes only when it changed: a window keeps the
            // one it has (ADR 0025).
            let archive = self.factory.archive_revision();
            let board = if self.told_archive == Some(archive) {
                self.factory.changes()
            } else {
                self.told_archive = Some(archive);
                self.factory.board()
            };
            ServerMessage::Factory { revision, board }
        })
    }

    /// A window's Factory command.
    pub(super) fn task_command(&mut self, action: &TaskAction) -> Result<Option<Done>, WireError> {
        match action.clone() {
            TaskAction::Start {
                project,
                instructions,
                request,
                workflow,
                pr,
            } => {
                let start = Command::Start {
                    project,
                    instructions,
                    agent: self.factory_agent.clone(),
                    request,
                    workflow,
                    pr,
                };
                let (answer, _) = self.factory_command(start).map_err(wire_error)?;
                Ok(Some(done(answer)))
            }
            TaskAction::Delete { task } => {
                let (_, kept) = self
                    .factory_command(Command::Delete { task })
                    .map_err(wire_error)?;
                Ok(Some(Done::Deleted {
                    kept: (!kept.is_empty()).then(|| kept.join(" ")),
                }))
            }
            other => match factory::task_command(other) {
                Some(command) => self.task_changed(command).map_err(wire_error),
                None => Err(WireError {
                    code: crate::protocol::ErrorCode::Unsupported,
                    message: "this server does not know that Factory command".into(),
                }),
            },
        }
    }

    fn task_changed(&mut self, command: Command) -> Result<Option<Done>, ApiError> {
        let (answer, _) = self.factory_command(command)?;
        Ok(Some(done(answer)))
    }

    /// What is known installed, for a save's check: each Provider's
    /// models as its agents announced them, and the skills a window found.
    pub(super) fn catalog(
        &self,
        skills: Vec<(crate::store::Provider, Vec<String>)>,
    ) -> factory::Catalog {
        factory::Catalog {
            models: crate::store::Provider::ALL
                .into_iter()
                .map(|provider| (provider, self.engine.announced_models(provider)))
                .collect(),
            skills,
        }
    }

    /// A window's Workflow authoring command.
    pub(super) fn workflow_command(
        &mut self,
        action: &WorkflowAction,
    ) -> Result<Option<Done>, WireError> {
        let models = self.catalog(Vec::new()).models;
        let Some(command) = factory::workflow_command(action.clone(), &self.factory_agent, models)
        else {
            return Err(WireError {
                code: crate::protocol::ErrorCode::Unsupported,
                message: "this server does not know that Workflow command".into(),
            });
        };
        let (answer, _) = self.factory_command(command).map_err(wire_error)?;
        Ok(Some(done(answer)))
    }

    // ------------------------------------------------------------ the API

    fn task_json(&self, task: &str) -> Result<Value, ApiError> {
        self.factory
            .task(task)
            .map(|task| json!(task))
            .ok_or_else(|| ApiError::new(Code::UnknownTask, "no Task has that id"))
    }

    pub(super) fn api_task_start(&mut self, params: TaskStartParams) -> Result<Value, ApiError> {
        let project = self.project_of(params.project.as_deref(), params.cwd.as_deref())?;
        let start = Command::Start {
            project,
            instructions: params.instructions,
            agent: self.factory_agent.clone(),
            request: None,
            workflow: params.workflow,
            pr: params.pr,
        };
        match self.factory_command(start)? {
            (Answer::Task(task), _) => Ok(json!({ "task": self.task_json(&task)? })),
            _ => Err(ApiError::new(Code::Failed, "no Task was made")),
        }
    }

    pub(super) fn api_workflow_create(
        &mut self,
        params: WorkflowCreateParams,
    ) -> Result<Value, ApiError> {
        let project = self.project_of(params.project.as_deref(), params.cwd.as_deref())?;
        let create = Command::CreateWorkflow {
            project,
            template: params.template.unwrap_or(factory::Template::Blank),
            agent: self.factory_agent.clone(),
        };
        let (Answer::Workflow(workflow), _) = self.factory_command(create)? else {
            return Err(ApiError::new(Code::Failed, "no Workflow was made"));
        };
        if let Some(name) = params.name {
            self.factory_command(Command::RenameWorkflow {
                workflow: workflow.clone(),
                name,
            })?;
        }
        self.api_workflow_answer(&workflow, None)
    }

    pub(super) fn api_workflow_list(&self, params: TaskListParams) -> Result<Value, ApiError> {
        let project = params
            .project
            .as_deref()
            .map(|project| self.project(project))
            .transpose()?;
        let board = self.factory.board();
        let workflows: Vec<&factory::Workflow> = board
            .workflows
            .iter()
            .filter(|workflow| project.is_none_or(|project| workflow.project == project))
            .collect();
        Ok(json!({ "workflows": workflows }))
    }

    /// A Workflow and its draft's problems, as far as the Server knows
    /// what is installed; `extra` joins the answer.
    pub(super) fn api_workflow_answer(
        &self,
        workflow: &str,
        extra: Option<(&str, Value)>,
    ) -> Result<Value, ApiError> {
        let found = self
            .factory
            .workflow(workflow)
            .ok_or_else(|| ApiError::new(Code::UnknownWorkflow, "no Workflow has that id"))?;
        let problems = found.draft.problems(&self.catalog(Vec::new()));
        let mut answer = json!({ "workflow": found, "problems": problems });
        if let Some((key, value)) = extra {
            answer[key] = value;
        }
        Ok(answer)
    }

    /// An authoring command from the API, answered with the Workflow.
    pub(super) fn api_author(
        &mut self,
        workflow: String,
        command: Command,
    ) -> Result<Value, ApiError> {
        let (answer, _) = self.factory_command(command)?;
        let extra = match answer {
            Answer::Step(step) => Some(("step", json!(step))),
            Answer::Changed(changed) => Some(("changed", json!(changed))),
            _ => None,
        };
        self.api_workflow_answer(&workflow, extra)
    }

    pub(super) fn api_workflow_save(
        &mut self,
        workflow: String,
        skills: Vec<(crate::store::Provider, Vec<String>)>,
    ) -> Result<Value, ApiError> {
        let save = Command::SaveWorkflow {
            workflow: workflow.clone(),
            catalog: self.catalog(skills),
        };
        let (answer, _) = self.factory_command(save)?;
        let version = match answer {
            Answer::Version(version) => version,
            _ => 0,
        };
        self.api_workflow_answer(&workflow, Some(("version", json!(version))))
    }

    /// An automatic starts' command from the API, answered with the
    /// Workflow and whether it changed.
    pub(super) fn api_automate(
        &mut self,
        workflow: String,
        command: Command,
    ) -> Result<Value, ApiError> {
        let (answer, _) = self.factory_command(command)?;
        let changed = answer == Answer::Changed(true);
        self.api_workflow_answer(&workflow, Some(("changed", json!(changed))))
    }

    /// A script's submission: its Task, new or the one its identity
    /// already started.
    pub(super) fn api_task_submit(&mut self, params: TaskSubmitParams) -> Result<Value, ApiError> {
        let submit = Command::Submit {
            workflow: params.workflow,
            identity: params.identity,
            instructions: params.instructions,
        };
        match self.factory_command(submit)? {
            (Answer::Task(task), _) => {
                Ok(json!({ "task": self.task_json(&task)?, "duplicate": false }))
            }
            (Answer::Duplicate(task), _) => {
                Ok(json!({ "task": self.task_json(&task)?, "duplicate": true }))
            }
            _ => Err(ApiError::new(Code::Failed, "no Task was made")),
        }
    }

    /// The Project named, or the one holding `cwd`.
    pub(super) fn project_of(
        &self,
        project: Option<&str>,
        cwd: Option<&std::path::Path>,
    ) -> Result<ProjectId, ApiError> {
        match (project, cwd) {
            (Some(project), _) => self.project(project),
            (None, Some(cwd)) => {
                let cwd = dunce::canonicalize(cwd)
                    .map_err(|error| ApiError::new(Code::Failed, error.to_string()))?;
                let registry = self.engine.registry();
                registry
                    .containing(&cwd)
                    .or_else(|| {
                        let checkout =
                            workspace::checkout_of(&cwd).filter(|checkout| checkout.linked)?;
                        registry.containing(&checkout.repo)
                    })
                    .ok_or_else(|| {
                        ApiError::new(
                            Code::ProjectNotFound,
                            "no Project holds that directory: open one with `project open`",
                        )
                    })
            }
            (None, None) => Err(ApiError::new(
                Code::InvalidRequest,
                "name the Project, or a directory in it",
            )),
        }
    }

    pub(super) fn api_task_list(&self, params: TaskListParams) -> Result<Value, ApiError> {
        let project = params
            .project
            .as_deref()
            .map(|project| self.project(project))
            .transpose()?;
        let board = self.factory.board();
        let tasks: Vec<&factory::Task> = board
            .tasks
            .iter()
            .chain(board.archive.iter().flatten())
            .filter(|task| project.is_none_or(|project| task.project == project))
            .collect();
        let mut answer = json!({ "tasks": tasks });
        let trouble = match project {
            Some(project) => board.trouble_of(project),
            None => board
                .trouble
                .as_deref()
                .or_else(|| board.troubles.values().next().map(String::as_str)),
        };
        if let Some(trouble) = trouble {
            answer["trouble"] = json!(trouble);
        }
        Ok(answer)
    }

    /// Clear: the Project's Completed Tasks go to its Archive (#226).
    pub(super) fn api_task_clear(&mut self, params: TaskClearParams) -> Result<Value, ApiError> {
        let project = self.project_of(params.project.as_deref(), params.cwd.as_deref())?;
        let (answer, _) = self.factory_command(Command::Clear { project })?;
        Ok(json!({ "changed": answer == Answer::Changed(true) }))
    }

    pub(super) fn api_task_get(&self, params: TaskParams) -> Result<Value, ApiError> {
        Ok(json!({ "task": self.task_json(&params.task)? }))
    }

    pub(super) fn api_task_delete(&mut self, params: TaskParams) -> Result<Value, ApiError> {
        let (_, kept) = self.factory_command(Command::Delete {
            task: params.task.clone(),
        })?;
        let mut answer = json!({ "deleted": params.task });
        if !kept.is_empty() {
            answer["kept"] = json!(kept.join(" "));
        }
        Ok(answer)
    }

    pub(super) fn api_report(&mut self, params: ReportOutcomeParams) -> Result<Value, ApiError> {
        let summary = params.summary;
        let outcome = match params.outcome {
            ReportedOutcome::Finished => Outcome::Finished { summary },
            ReportedOutcome::Unable => Outcome::Unable { tried: summary },
            ReportedOutcome::Approved => Outcome::Approved { summary },
            ReportedOutcome::Rejected => Outcome::Rejected { reasons: summary },
            merge => Outcome::Merge {
                result: match merge {
                    ReportedOutcome::Merged => factory::MergeOutcome::Merged,
                    ReportedOutcome::Conflict => factory::MergeOutcome::Conflict,
                    ReportedOutcome::Pending => factory::MergeOutcome::Pending,
                    ReportedOutcome::Denied => factory::MergeOutcome::Denied,
                    ReportedOutcome::HeadChanged => factory::MergeOutcome::HeadChanged,
                    _ => factory::MergeOutcome::Unknown,
                },
                summary,
            },
        };
        self.api_worker_said(
            params.worker,
            Command::Report {
                worker: params.worker,
                outcome,
            },
        )
    }

    pub(super) fn api_ask(&mut self, params: ReportQuestionParams) -> Result<Value, ApiError> {
        self.api_worker_said(
            params.worker,
            Command::Ask {
                worker: params.worker,
                text: params.text,
            },
        )
    }

    /// A Worker's report or question, answered with its Task.
    pub(super) fn api_worker_said(
        &mut self,
        worker: ThreadId,
        command: Command,
    ) -> Result<Value, ApiError> {
        let (answer, _) = self.factory_command(command)?;
        let task = self
            .factory
            .task_of(worker)
            .map(|task| task.id.clone())
            .unwrap_or_default();
        Ok(json!({
            "task": self.task_json(&task)?,
            "changed": answer == Answer::Changed(true),
        }))
    }

    /// An operator's command on one Task, answered with the Task.
    pub(super) fn api_task_command(
        &mut self,
        task: &str,
        command: Command,
    ) -> Result<Value, ApiError> {
        let (answer, _) = self.factory_command(command)?;
        let changed = answer == Answer::Changed(true);
        Ok(json!({ "task": self.task_json(task)?, "changed": changed }))
    }

    pub(super) fn api_task_rework(&mut self, params: TaskReworkParams) -> Result<Value, ApiError> {
        let to = params
            .to
            .as_deref()
            .map(str::parse::<factory::Destination>)
            .transpose()
            .map_err(|why| ApiError::new(Code::InvalidRequest, why))?;
        let task = params.task.clone();
        self.api_task_command(
            &task,
            Command::Rework {
                task: params.task,
                feedback: params.feedback,
                to,
            },
        )
    }

    pub(super) fn api_task_redirect(
        &mut self,
        params: TaskRedirectParams,
    ) -> Result<Value, ApiError> {
        let task = params.task.clone();
        self.api_task_command(
            &task,
            Command::Redirect {
                task: params.task,
                direction: factory::Override {
                    to: params.to,
                    skip: params.skip,
                    then: params.then.unwrap_or_default(),
                    note: params.note.unwrap_or_default(),
                },
            },
        )
    }
}
