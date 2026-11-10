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
    AgentPromptParams, ReportOutcomeParams, ReportedOutcome, TaskListParams, TaskParams,
    TaskStartParams,
};
use crate::api::{ApiError, Code};
use crate::engine::{Launch, ProviderChoice};
use crate::factory::{self, Agent, Answer, Command, Effect, Outcome, Seen};
use crate::protocol::{Done, ServerMessage, TaskAction, WireError};
use crate::settings::Settings;
use crate::workspace::registry::ProjectId;
use crate::workspace::{self, WorkspaceBinding, WorkspaceChoice};
use crate::{Attention, ThreadId, ThreadOwner};

/// Wall-clock milliseconds, as the Factory's activity is stamped.
fn now_ms() -> i64 {
    crate::clock::now().timestamp_millis()
}

/// The Factory kept in the Store's directory, or one in memory (said in
/// the log) when the file there cannot be read: it is never overwritten.
pub(super) fn load(store_dir: &std::path::Path) -> factory::Factory {
    let path = store_dir.join(factory::FILE_NAME);
    factory::Factory::load(&path).unwrap_or_else(|error| {
        crate::log!(
            "server: the Factory at {} could not be read, so nothing is saved: {error}",
            path.display()
        );
        factory::Factory::in_memory()
    })
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
    /// One pass of the Factory, and its effects carried out. What a
    /// deleted Task's worktree removal left behind is said back.
    pub(super) fn step_factory(&mut self) -> Vec<String> {
        let mut kept = Vec::new();
        // Carrying out an effect may answer with a command (a Worker
        // started), whose effects the next round carries out.
        for _ in 0..4 {
            let workers = self.factory_workers();
            let effects = self.factory.step(now_ms(), &workers);
            if effects.is_empty() {
                break;
            }
            for effect in effects {
                if let Some(note) = self.carry_out(effect) {
                    kept.push(note);
                }
            }
        }
        kept
    }

    /// What each open Worker's Thread is doing.
    fn factory_workers(&self) -> BTreeMap<ThreadId, Seen> {
        self.factory
            .board()
            .tasks
            .iter()
            .filter_map(|task| task.worker)
            .filter_map(|thread| {
                let view = self.engine.thread(thread)?;
                let attention = self.engine.attention(thread)?;
                let busy = !matches!(attention, Attention::Idle | Attention::Done)
                    || self.engine.holds_prompt(thread)
                    || view.starting();
                let turns = self.engine.completion_seq(thread).unwrap_or(0);
                Some((thread, Seen { busy, turns }))
            })
            .collect()
    }

    /// One command, and at once what it set off.
    fn factory_command(&mut self, command: Command) -> Result<(Answer, Vec<String>), ApiError> {
        let answer = self.factory.command(command, now_ms())?;
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
                prompt,
            } => {
                let command = match self.start_worker(&task, project, agent, branch, prompt) {
                    Ok((thread, worktree)) => Command::WorkerStarted {
                        task,
                        thread,
                        worktree,
                    },
                    Err(why) => Command::WorkerFailed { task, why },
                };
                if let Err(refusal) = self.factory.command(command, now_ms()) {
                    crate::log!("server: the Factory did not take a Worker's start: {refusal}");
                }
            }
            Effect::Prompt { thread, text } => {
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
            Effect::RemoveWorktree { path } => return self.remove_task_worktree(&path),
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

    /// Open the Task's Worker in a new worktree of its Project on its
    /// branch, owned by it, titled as it, and give it its first prompt.
    fn start_worker(
        &mut self,
        task: &str,
        project: ProjectId,
        agent: Agent,
        branch: String,
        prompt: String,
    ) -> Result<(ThreadId, Option<std::path::PathBuf>), String> {
        let repo = self
            .engine
            .registry()
            .project(project)
            .map(|project| project.root.clone())
            .ok_or("its Project is gone")?;
        let title = self.factory.task(task).map(|task| task.title.clone());
        let launch = Launch {
            project: Some(project),
            owner: Some(ThreadOwner::Task(task.to_string())),
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
                WorkspaceChoice::NamedWorktree {
                    repo,
                    branch,
                    base: None,
                },
                launch,
            )
            .map_err(|error| error.to_string())?;
        if let Some(title) = title {
            let _ = self.execute(crate::protocol::Op::Rename { thread, title });
        }
        let worktree = match self.engine.thread(thread).and_then(|view| view.workspace()) {
            Some(WorkspaceBinding::Worktree { path, .. }) => Some(path.clone()),
            _ => None,
        };
        self.prompt(&AgentPromptParams {
            thread,
            text: prompt,
            wait: false,
            timeout_ms: None,
            until: Vec::new(),
        })
        .map_err(|error| error.message)?;
        Ok((thread, worktree))
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
            ServerMessage::Factory {
                revision,
                board: self.factory.board(),
            }
        })
    }

    /// A window's Factory command.
    pub(super) fn task_command(&mut self, action: &TaskAction) -> Result<Option<Done>, WireError> {
        let wire = |error: ApiError| WireError {
            code: crate::protocol::ErrorCode::Failed,
            message: error.message,
        };
        match action.clone() {
            TaskAction::Start {
                project,
                instructions,
                request,
            } => {
                let start = Command::Start {
                    project,
                    instructions,
                    agent: self.factory_agent.clone(),
                    request,
                };
                match self.factory_command(start).map_err(wire)? {
                    (Answer::Task(task), _) => Ok(Some(Done::Task { task })),
                    (Answer::Changed(changed), _) => Ok(Some(Done::Changed { changed })),
                }
            }
            TaskAction::Stop { task } => self.task_changed(Command::Stop { task }).map_err(wire),
            TaskAction::Resume { task } => {
                self.task_changed(Command::Resume { task }).map_err(wire)
            }
            TaskAction::Delete { task } => {
                let (_, kept) = self
                    .factory_command(Command::Delete { task })
                    .map_err(wire)?;
                Ok(Some(Done::Deleted {
                    kept: (!kept.is_empty()).then(|| kept.join(" ")),
                }))
            }
            TaskAction::Unknown => Err(WireError {
                code: crate::protocol::ErrorCode::Unsupported,
                message: "this server does not know that Factory command".into(),
            }),
        }
    }

    fn task_changed(&mut self, command: Command) -> Result<Option<Done>, ApiError> {
        match self.factory_command(command)? {
            (Answer::Changed(changed), _) => Ok(Some(Done::Changed { changed })),
            (Answer::Task(task), _) => Ok(Some(Done::Task { task })),
        }
    }

    // ------------------------------------------------------------ the API

    fn task_json(&self, task: &str) -> Result<Value, ApiError> {
        self.factory
            .task(task)
            .map(|task| json!(task))
            .ok_or_else(|| ApiError::new(Code::UnknownTask, "no Task has that id"))
    }

    pub(super) fn api_task_start(&mut self, params: TaskStartParams) -> Result<Value, ApiError> {
        let project = match (&params.project, &params.cwd) {
            (Some(project), _) => self.project(project)?,
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
                    })?
            }
            (None, None) => {
                return Err(ApiError::new(
                    Code::InvalidRequest,
                    "name the Project, or a directory in it",
                ))
            }
        };
        let start = Command::Start {
            project,
            instructions: params.instructions,
            agent: self.factory_agent.clone(),
            request: None,
        };
        match self.factory_command(start)? {
            (Answer::Task(task), _) => Ok(json!({ "task": self.task_json(&task)? })),
            (Answer::Changed(_), _) => Err(ApiError::new(Code::Failed, "no Task was made")),
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
            .filter(|task| project.is_none_or(|project| task.project == project))
            .collect();
        Ok(json!({ "tasks": tasks }))
    }

    pub(super) fn api_task_get(&self, params: TaskParams) -> Result<Value, ApiError> {
        Ok(json!({ "task": self.task_json(&params.task)? }))
    }

    pub(super) fn api_task_change(&mut self, command: Command) -> Result<Value, ApiError> {
        let task = match &command {
            Command::Stop { task } | Command::Resume { task } => task.clone(),
            _ => unreachable!("only stop and resume change a Task in place"),
        };
        let (answer, _) = self.factory_command(command)?;
        let changed = answer == Answer::Changed(true);
        Ok(json!({ "task": self.task_json(&task)?, "changed": changed }))
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
        let outcome = match params.outcome {
            ReportedOutcome::Finished => Outcome::Finished {
                summary: params.summary,
            },
            ReportedOutcome::Unable => Outcome::Unable {
                tried: params.summary,
            },
        };
        let (answer, _) = self.factory_command(Command::Report {
            worker: params.worker,
            outcome,
        })?;
        let task = self
            .factory
            .task_of(params.worker)
            .map(|task| task.id.clone())
            .unwrap_or_default();
        Ok(json!({
            "task": self.task_json(&task)?,
            "changed": answer == Answer::Changed(true),
        }))
    }
}
