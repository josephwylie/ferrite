//! The API's requests (ADR 0027), answered on the pump thread. Each one
//! that changes a Thread is the window's own command (`Engine::execute`),
//! so an agent and the operator act through the same door; `thread.stop`,
//! which no window asks, is the Engine's `stop`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::Server;
use ferrite_core::api::schema::Callers;
use ferrite_core::api::schema::{
    AgentPromptParams, AgentReadParams, AgentWaitParams, DecisionAnswerParams, ThreadReviveParams,
    ThreadStartParams,
};
use ferrite_core::api::{
    self, thread_info, thread_parked, thread_result, unknown_thread, ApiError, CallerId, Code,
    Method, Request, ThreadInfo, Until, Wait,
};
use ferrite_core::drive::Action;
use ferrite_core::engine::{Launch, ReadBack};
use ferrite_core::protocol::{Done, Op, ProjectAction, ServerMessage, WireAnswer};
use ferrite_core::runtime_mode::RuntimeMode;
use ferrite_core::settings::Settings;
use ferrite_core::store::{LoadError, Provider};
use ferrite_core::workspace::{self, WorkspaceChoice};
use ferrite_core::ThreadId;

mod projects;

/// How much of a transcript `agent.read` gives when not told.
const READ_LINES: usize = 80;

impl Server {
    pub(super) fn api_heard(&mut self, heard: api::Heard) {
        match heard {
            api::Heard::Joined(caller, answers) => self.api.join(caller, answers),
            api::Heard::Request(caller, request) => self.api_request(caller, request),
            api::Heard::Hungup(caller) => self.api.hang_up(caller),
        }
    }

    fn api_request(
        &mut self,
        caller_id: CallerId,
        Request {
            id,
            method,
            caller: from,
        }: Request,
    ) {
        let caller = self.caller(from);
        if let Err(refused) = caller.may(&method) {
            self.api.reply(caller_id, id, Err(refused));
            return;
        }
        // What changed before the request reaches the windows and the
        // subscribers first.
        self.broadcast();
        let now = Instant::now();
        let timeout = |ms: Option<u64>| ms.map(Duration::from_millis);
        let result = match method {
            Method::ThreadStart(params) => {
                // With a prompt, ready is up and at work on it: a first
                // turn that meets a Decision is an agent that is ready.
                let ready = params.ready.then(|| {
                    let within = Some(timeout(params.timeout_ms).unwrap_or(api::READY));
                    match params.prompt {
                        Some(_) => Wait::after_prompt_until(Until::Taken, within, now),
                        None => Wait::until(Until::Ready, within, now),
                    }
                });
                match (self.start_thread(params), ready) {
                    (Ok(thread), Some(wait)) => {
                        self.api.wait(&self.engine, caller_id, id, thread, wait);
                        return;
                    }
                    (Ok(thread), None) => thread_result(&self.engine, thread),
                    (Err(error), _) => Err(error),
                }
            }
            Method::ThreadRevive(params) => {
                let ready = params.ready.then(|| {
                    let within = timeout(params.timeout_ms).unwrap_or(api::READY);
                    Wait::until(Until::Ready, Some(within), now)
                });
                let thread = params.thread;
                match (self.revive_thread(params), ready) {
                    (Ok(()), Some(wait)) => {
                        self.api.wait(&self.engine, caller_id, id, thread, wait);
                        return;
                    }
                    (Ok(()), None) => thread_result(&self.engine, thread),
                    (Err(error), _) => Err(error),
                }
            }
            Method::ThreadList(_) => {
                let threads: Vec<ThreadInfo> = self
                    .engine
                    .open_threads()
                    .iter()
                    .filter_map(|thread| thread_info(&self.engine, *thread))
                    .collect();
                Ok(json!({ "threads": threads }))
            }
            Method::ThreadGet(params) => thread_result(&self.engine, params.thread)
                .map_err(|_| self.not_open(params.thread)),
            Method::ThreadFocus(params) => self.open_thread(params.thread).and_then(|thread| {
                self.say(ServerMessage::Focus {
                    thread,
                    fork_of: None,
                });
                thread_result(&self.engine, thread)
            }),
            Method::ThreadRename(params) => self.open_thread(params.thread).and_then(|thread| {
                self.execute(Op::Rename {
                    thread,
                    title: params.title,
                })?;
                thread_result(&self.engine, thread)
            }),
            // Closing a parked Thread again changes nothing, as stopping
            // one does (`stop_thread`).
            Method::ThreadClose(params) => self.is_open(params.thread).and_then(|open| {
                if open {
                    self.park(params.thread)?;
                }
                Ok(json!({ "thread": params.thread }))
            }),
            Method::AgentPrompt(params) => match self.prompt(&params) {
                Ok(()) if params.wait || !params.until.is_empty() => {
                    let until = match params.until.is_empty() {
                        true => Until::Settled,
                        false => Until::AnyOf(params.until),
                    };
                    let wait = Wait::after_prompt_until(until, timeout(params.timeout_ms), now);
                    self.api
                        .wait(&self.engine, caller_id, id, params.thread, wait);
                    return;
                }
                Ok(()) => thread_result(&self.engine, params.thread),
                Err(error) => Err(error),
            },
            Method::AgentWait(AgentWaitParams {
                thread,
                until: Some(_),
                turn_ended: true,
                ..
            }) => Err(ApiError::new(
                Code::InvalidRequest,
                "wait for an Attention or for the turn to end, not both",
            )
            .on(thread)),
            Method::AgentWait(AgentWaitParams {
                thread,
                until,
                turn_ended,
                timeout_ms,
            }) => match self.engine.thread(thread) {
                Some(_) => {
                    let until = match turn_ended {
                        true => Until::TurnEnded,
                        false => until.map_or(Until::Settled, Until::Attention),
                    };
                    let wait = Wait::until(until, timeout(timeout_ms), now);
                    self.api.wait(&self.engine, caller_id, id, thread, wait);
                    return;
                }
                None => Err(self.not_open(thread)),
            },
            Method::AgentRead(params) => match self.engine.read_back_later(params.thread) {
                Some(read_back) => {
                    self.read_back_off_pump(caller_id, id, read_back, move |transcript| {
                        read_answer(&params, transcript)
                    });
                    return;
                }
                None => self.read(&params),
            },
            Method::AgentResult(params) => {
                let thread = params.thread;
                let outcome = self.engine.ended_turn(thread);
                if let Some(read_back) = self.engine.read_back_later(thread) {
                    self.read_back_off_pump(caller_id, id, read_back, move |transcript| {
                        api::turn_answer(thread, outcome.as_ref(), transcript.turn_result().reply, false)
                    });
                    return;
                }
                match self.engine.thread(thread) {
                    Some(view) => {
                        let reply = view.transcript().turn_result().reply;
                        Ok(api::turn_answer(thread, outcome.as_ref(), reply, false))
                    }
                    None if self.is_parked(thread) => {
                        self.parked_result_off_pump(caller_id, id, thread);
                        return;
                    }
                    None => Err(unknown_thread()),
                }
            }
            Method::AgentInterrupt(params) => self.open_thread(params.thread).and_then(|thread| {
                self.execute(Op::Interrupt { thread })?;
                thread_result(&self.engine, thread)
            }),
            Method::ThreadStop(params) => self.stop_thread(params.thread),
            Method::DecisionList(params) => {
                let decisions: Vec<_> = self
                    .engine
                    .unanswered_decisions()
                    .into_iter()
                    .filter(|pending| params.thread.is_none_or(|thread| pending.thread == thread))
                    .collect();
                Ok(json!({ "decisions": decisions }))
            }
            Method::DecisionAnswer(params) => self.answer(caller_id, params),
            Method::ProjectList(_) => Ok(json!({ "projects": api::project_infos(&self.engine) })),
            Method::ProjectGet(params) => self
                .project(&params.project)
                .and_then(|project| self.project_result(project)),
            Method::ProjectOpen(params) => self.open_project(params),
            Method::ProjectFocus(params) => self.project(&params.project).and_then(|project| {
                let landed = self.focus_project(project);
                let mut answer = self.project_result(project)?;
                if let Some(thread) = landed {
                    answer["thread"] = json!(thread);
                }
                Ok(answer)
            }),
            Method::ProjectRename(params) => self.project(&params.project).and_then(|project| {
                self.execute(Op::Project {
                    action: ProjectAction::Rename {
                        project,
                        title: params.label,
                    },
                })?;
                self.project_result(project)
            }),
            Method::ProjectClose(params) => self
                .project(&params.project)
                .and_then(|project| self.close_project(project)),
            Method::WorktreeCreate(params) => self.create_worktree(params),
            Method::WorktreeRemove(params) => self.remove_worktree(&params.path),
            Method::MachineList(params) => match self.api_machine_list(caller_id, &id, &params) {
                Some(result) => result,
                None => return,
            },
            Method::IdentityGet(_) => Ok(json!({ "server_id": self.id })),
            Method::ProviderList(params) => match self.api_provider_list(caller_id, &id, params) {
                Some(result) => result,
                None => return,
            },
            Method::EventsSubscribe(params) => {
                self.api
                    .subscribe(&self.engine, caller_id, id, params.threads);
                return;
            }
            Method::TaskStart(params) => self.api_task_start(params),
            Method::TaskList(params) => self.api_task_list(params),
            Method::TaskGet(params) => self.api_task_get(params),
            Method::TaskStop(params) => self.api_task_command(
                &params.task.clone(),
                ferrite_core::factory::Command::Stop { task: params.task },
            ),
            Method::TaskResume(params) => self.api_task_command(
                &params.task.clone(),
                ferrite_core::factory::Command::Resume { task: params.task },
            ),
            Method::TaskDelete(params) => self.api_task_delete(params),
            Method::ReportOutcome(params) => caller
                .worker()
                .and_then(|worker| self.api_report(worker, params)),
            Method::WorkflowCreate(params) => self.api_workflow_create(params),
            Method::WorkflowList(params) => self.api_workflow_list(params),
            Method::WorkflowGet(params) => self.api_workflow_answer(&params.workflow, None),
            Method::WorkflowRename(params) => self.api_author(
                params.workflow.clone(),
                ferrite_core::factory::Command::RenameWorkflow {
                    workflow: params.workflow,
                    name: params.name,
                },
            ),
            Method::WorkflowSave(params) => self.api_workflow_save(params.workflow),
            Method::StepAdd(params) => self.api_author(
                params.workflow.clone(),
                ferrite_core::factory::Command::AddStep {
                    workflow: params.workflow,
                    after: params.after,
                    agent: self.factory_agent.clone(),
                },
            ),
            Method::StepEdit(params) => self.api_author(
                params.workflow.clone(),
                ferrite_core::factory::Command::EditStep {
                    workflow: params.workflow,
                    step: params.step,
                    change: params.change,
                },
            ),
            Method::StepMove(params) => self.api_author(
                params.workflow.clone(),
                ferrite_core::factory::Command::MoveStep {
                    workflow: params.workflow,
                    step: params.step,
                    to: params.to,
                },
            ),
            Method::StepDelete(params) => self.api_author(
                params.workflow.clone(),
                ferrite_core::factory::Command::DeleteStep {
                    workflow: params.workflow,
                    step: params.step,
                },
            ),
            Method::ReportQuestion(params) => caller
                .worker()
                .and_then(|worker| self.api_ask(worker, params)),
            Method::ReportRevision(params) => caller.worker().and_then(|worker| {
                self.api_worker_said(
                    worker,
                    ferrite_core::factory::Command::ReportRevision {
                        worker,
                        head: params.head,
                        pr: params.pr,
                    },
                )
            }),
            Method::TaskClear(params) => self.api_task_clear(params),
            Method::ReportEvidence(params) => caller.worker().and_then(|worker| {
                self.api_worker_said(
                    worker,
                    ferrite_core::factory::Command::ReportEvidence {
                        worker,
                        text: params.text,
                    },
                )
            }),
            Method::TaskAnswer(params) => {
                let task = params.task.clone();
                self.api_task_command(
                    &task,
                    ferrite_core::factory::Command::AnswerQuestion {
                        task: params.task,
                        text: params.text,
                        question: params.question,
                    },
                )
            }
            Method::TaskApprove(params) => {
                let task = params.task.clone();
                self.api_task_command(
                    &task,
                    ferrite_core::factory::Command::Approve { task: params.task },
                )
            }
            Method::TaskRework(params) => self.api_task_rework(params),
            Method::TaskRedirect(params) => self.api_task_redirect(params),
            Method::WorkflowApproval(params) => self.api_author(
                params.workflow.clone(),
                ferrite_core::factory::Command::EditApproval {
                    workflow: params.workflow,
                    change: params.change,
                },
            ),
            Method::WorkflowStart(params) => self.api_automate(
                params.workflow.clone(),
                ferrite_core::factory::Command::EditStart {
                    workflow: params.workflow,
                    start: params.start,
                },
            ),
            Method::WorkflowAutomation(params) => self.api_automate(
                params.workflow.clone(),
                ferrite_core::factory::Command::Automation {
                    workflow: params.workflow,
                    turn: params.turn,
                },
            ),
            Method::WorkflowBacklog(params) => self.api_automate(
                params.workflow.clone(),
                ferrite_core::factory::Command::Backlog {
                    workflow: params.workflow,
                    take: params.take,
                },
            ),
            Method::TaskSubmit(params) => self.api_task_submit(params),
            Method::ProposalRaise(params) => caller
                .coordinator()
                .and_then(|coordinator| self.api_proposal_raise(coordinator, params)),
            Method::ProposalList(params) => self.api_proposal_list(params),
            Method::ProposalAccept(params) => self.api_proposal_answer(params.proposal, true),
            Method::ProposalDismiss(params) => self.api_proposal_answer(params.proposal, false),
            Method::CoordinatorSay(params) => self.api_coordinator_say(params),
            Method::CoordinatorSet(params) => self.api_coordinator_set(params),
            Method::WindowKey(keys) => return self.drive(caller_id, id, Action::Key(keys)),
            Method::WindowType(text) => return self.drive(caller_id, id, Action::Type(text)),
            Method::WindowClick(click) => return self.drive(caller_id, id, Action::Click(click)),
            Method::WindowFind(find) => return self.drive(caller_id, id, Action::Find(find)),
            Method::WindowAttach(files) => return self.drive(caller_id, id, Action::Attach(files)),
            Method::WindowShot(shot) => return self.drive(caller_id, id, Action::Shot(shot)),
            Method::WindowResize(size) => return self.drive(caller_id, id, Action::Resize(size)),
            Method::WindowDrag(drag) => return self.drive(caller_id, id, Action::Drag(drag)),
            Method::WindowText(find) => return self.drive(caller_id, id, Action::Read(find)),
            Method::WindowFocus(_) => return self.drive(caller_id, id, Action::Focus),
            Method::PrWatch(params) => match self.api_pr_watch(caller_id, &id, from, params) {
                Some(result) => result,
                None => return,
            },
            Method::PrUnwatch(params) => self.api_pr_unwatch(from, &params),
            Method::DelegateStart(params) => {
                match self.api_delegate_start(caller_id, &id, from, params) {
                    Some(result) => result,
                    None => return,
                }
            }
            Method::DelegateGet(params) => self.api_delegate_get(from, &params),
            Method::DelegateList(params) => self.api_delegate_list(from, &params),
            Method::DelegateCancel(params) => self.api_delegate_cancel(from, &params),
            Method::NotificationShow(params) => {
                self.say(ServerMessage::Notification {
                    title: params.title,
                    body: params.body,
                    sound: params.sound,
                    important: params.important,
                });
                Ok(json!({ "windows": self.windows() }))
            }
        };
        // What the request changed reaches the windows before its answer.
        self.broadcast();
        self.api.reply(caller_id, id, result);
    }

    /// Who a request comes from: the Thread it names, as the Factory knows
    /// that Thread.
    fn caller(&self, from: Option<ThreadId>) -> Caller {
        match from {
            None => Caller::Operator,
            Some(thread) if self.factory.coordinates(thread) => Caller::Coordinator(thread),
            Some(thread) if self.factory.task_of(thread).is_some() => Caller::Worker(thread),
            Some(_) => Caller::Thread,
        }
    }

    pub(super) fn execute(&mut self, op: Op) -> Result<Option<Done>, ApiError> {
        self.engine.execute(&op).map_err(ApiError::from)
    }

    /// Park a Thread as the window's park does (`Cockpit::park_thread`).
    pub(super) fn park(&mut self, thread: ThreadId) -> Result<(), ApiError> {
        self.execute(Op::Park { thread }).map(drop)
    }

    /// How many windows hear what the Server says to them all.
    fn windows(&self) -> usize {
        self.clients
            .values()
            .filter(|client| client.welcomed)
            .count()
    }

    /// An open Thread, or why not (`not_open`).
    pub(super) fn open_thread(&self, thread: ThreadId) -> Result<ThreadId, ApiError> {
        self.engine
            .thread(thread)
            .map(|_| thread)
            .ok_or_else(|| self.not_open(thread))
    }

    /// Whether `thread` is open (`true`) or parked (`false`); `unknown_thread`
    /// for an id the Store does not hold.
    pub(super) fn is_open(&self, thread: ThreadId) -> Result<bool, ApiError> {
        if self.engine.thread(thread).is_some() {
            return Ok(true);
        }
        match self.is_parked(thread) {
            true => Ok(false),
            false => Err(unknown_thread()),
        }
    }

    /// The refusal for a Thread that is not open: `thread_parked` while
    /// the Store holds it, `unknown_thread` once it is gone.
    fn not_open(&self, thread: ThreadId) -> ApiError {
        match self.is_parked(thread) {
            true => thread_parked(thread),
            false => unknown_thread(),
        }
    }

    /// Whether the Store holds `thread`, parked.
    fn is_parked(&self, thread: ThreadId) -> bool {
        self.engine
            .parked()
            .is_ok_and(|parked| parked.contains(&thread))
    }

    /// Stop everything `thread` runs (`Engine::stop`). Stopping it again
    /// changes nothing, and a parked Thread runs nothing: both are already
    /// stopped. Only a Thread the Store does not hold is unknown.
    fn stop_thread(&mut self, thread: ThreadId) -> Result<Value, ApiError> {
        if self.is_open(thread)? {
            self.engine.stop(thread);
        }
        Ok(json!({ "thread": thread }))
    }

    /// Open a Thread as a window's `open` does, its CLI given `args` and
    /// capped at `mode`, then title it, prompt it and land the windows on
    /// it as asked; or answer with the Thread an earlier start with the
    /// same `request` key opened, starting and sending nothing.
    fn start_thread(&mut self, params: ThreadStartParams) -> Result<ThreadId, ApiError> {
        if params.delegated_from.is_some() && !self.takes_delegates()? {
            return Err(ApiError::new(
                Code::LinkedMachinesOff,
                "this machine takes no Thread from another machine's: turn on Settings › \
                 Labs › Linked machines here",
            ));
        }
        if let Some(request) = &params.request {
            let started = self
                .engine
                .started_by(request)
                .map_err(|error| ApiError::new(Code::Failed, error.to_string()))?;
            if let Some(thread) = started {
                return self.open_thread(thread).map_err(|_| thread_parked(thread));
            }
        }
        let provider = params.provider.unwrap_or(self.default_provider);
        let env: Vec<(String, String)> = params.env.into_iter().collect();
        if params.mode.is_some() {
            refuse_widening(provider, &params.args, &env, &params.launcher)?;
        }
        let (checkout, project) = match &params.project {
            Some(project) => {
                let project = self.project(project)?;
                let cwd = dunce::canonicalize(&params.cwd)
                    .map_err(|error| ApiError::new(Code::Failed, error.to_string()))?;
                let directory = self.directory_in(project, &cwd)?;
                // As Herdr's tab starts in its cwd, the Thread works in
                // `cwd` itself; a fresh worktree is of its directory.
                let checkout = if params.worktree { directory } else { cwd };
                (checkout, Some(project))
            }
            None => (params.cwd, None),
        };
        let owner = match params.owner {
            None => None,
            Some(api::schema::Owner::Task(task)) => Some(ferrite_core::ThreadOwner::Task(task)),
            Some(api::schema::Owner::Factory(project)) => {
                Some(ferrite_core::ThreadOwner::Factory(self.project(&project)?))
            }
        };
        let workspace = workspace_choice(checkout, params.worktree, params.branch, params.base)?;
        let launch = Launch {
            launch_provider: params.provider,
            args: params.args,
            env,
            launcher: params.launcher,
            resume: params.resume,
            project,
            owner,
            delegated_from: params.delegated_from,
            request: params.request,
            cap: params.mode,
        };
        let thread = self
            .engine
            .open_with(provider, workspace, launch)
            .map_err(|error| ApiError::new(Code::Failed, error.to_string()))?;
        if let Some(title) = params.title {
            self.execute(Op::Rename { thread, title })?;
        }
        if let Some(text) = params.prompt {
            let prompt = AgentPromptParams {
                thread,
                text,
                wait: false,
                timeout_ms: None,
                until: Vec::new(),
            };
            self.prompt(&prompt).map_err(|error| error.on(thread))?;
        }
        if params.focus {
            self.say(ServerMessage::Focus {
                thread,
                fork_of: None,
            });
        }
        Ok(thread)
    }

    /// Start a parked Thread's agent again in place, as `params` says. An
    /// open Thread is refused: it is closed first (`thread.close`), as
    /// Herdr's pane is quit before its agent is started again.
    fn revive_thread(&mut self, params: ThreadReviveParams) -> Result<(), ApiError> {
        let thread = params.thread;
        if self.engine.thread(thread).is_some() {
            return Err(ApiError::new(
                Code::InvalidRequest,
                "this Thread is open: close it first (thread.close)",
            ));
        }
        let parked = self
            .engine
            .parked()
            .map_err(|error| ApiError::new(Code::Failed, error.to_string()))?;
        if !parked.contains(&thread) {
            return Err(ApiError::new(
                Code::UnknownThread,
                "no parked Thread has that id",
            ));
        }
        let launch = Launch {
            launch_provider: params.launch_provider,
            args: params.args,
            env: params.env.into_iter().collect(),
            launcher: params.launcher,
            resume: params.resume,
            ..Launch::default()
        };
        // A revive runs the Thread's own Provider (`Engine::reopen`).
        if let Some((provider, _)) = self.engine.cap(thread) {
            refuse_widening(provider, &launch.args, &launch.env, &launch.launcher)?;
        }
        self.engine
            .revive_with(thread, launch)
            .map_err(|error| ApiError::new(Code::Failed, error.to_string()))
    }

    /// Whether this machine takes a Thread from another machine's: its
    /// Settings › Labs › Linked machines, as its `settings.json` says now.
    pub(super) fn takes_delegates(&self) -> Result<bool, ApiError> {
        Settings::load(&self.settings_dir)
            .map(|settings| settings.labs.linked_machines)
            .map_err(|error| ApiError::new(Code::Failed, error.to_string()))
    }

    /// What this machine runs Threads on, as `provider.list` answers
    /// (T3 Code's `orchestrator_capabilities`): each provider's models and
    /// their efforts, the modes a Thread may be capped at, and the mode
    /// its settings start one at, which no cap broadens.
    pub(crate) fn providers(&self) -> Value {
        let defaults = self.defaults.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let providers: Vec<Value> = Provider::ALL
            .into_iter()
            .map(|provider| {
                let models: Vec<Value> = self
                    .engine
                    .model_catalog(provider)
                    .into_iter()
                    .map(|model| {
                        json!({
                            "model": model.value,
                            "label": model.display,
                            "efforts": model.efforts,
                            "default_effort": model.default_effort,
                        })
                    })
                    .collect();
                json!({
                    "provider": provider,
                    "models": models,
                    "modes": RuntimeMode::ALL,
                    "mode": RuntimeMode::of_defaults(provider, &defaults),
                })
            })
            .collect();
        json!({ "providers": providers, "default_provider": self.default_provider })
    }

    /// Herdr's rule: nothing is sent to an agent a Decision blocks, since
    /// the prompt would read as that Decision's answer or be lost behind
    /// it. A prompt to a Session still starting is held until it is up, as
    /// a send to a stopped one is, and any after it wait behind it. A
    /// prompt to a working agent waits its turn in the provider's queue, as
    /// the Composer's does.
    pub(super) fn prompt(&mut self, params: &AgentPromptParams) -> Result<(), ApiError> {
        let (thread, text) = (params.thread, params.text.clone());
        let view = self
            .engine
            .thread(thread)
            .ok_or_else(|| self.not_open(thread))?;
        if view.pending().is_some() {
            return Err(ApiError::new(
                Code::AgentBlocked,
                "a Decision waits on this Thread; nothing was sent",
            ));
        }
        let op = if view.needs_queue() && !view.starting() {
            Op::Queue { thread, text }
        } else {
            Op::Send { thread, text }
        };
        self.execute(op).map(|_| ())
    }

    /// A held Thread's transcript, read where it is.
    fn read(&self, params: &AgentReadParams) -> Result<Value, ApiError> {
        let view = self
            .engine
            .thread(params.thread)
            .ok_or_else(|| self.not_open(params.thread))?;
        Ok(read_answer(params, view.transcript()))
    }

    /// A parked Thread's latest turn (`agent.result`), replayed whole from
    /// its log on a thread of its own.
    fn parked_result_off_pump(&mut self, caller: CallerId, request: String, thread: ThreadId) {
        let store = self.engine.browser_store().clone();
        let (answer, answered) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            ferrite_core::spawn::serve_the_operator();
            let reply = match store.turn_result(thread) {
                Ok(turn) => Ok(api::turn_answer(thread, turn.outcome.as_ref(), turn.reply, true)),
                Err(error) => Err(ApiError::new(
                    Code::Failed,
                    format!("the Thread's log could not be read: {error}"),
                )),
            };
            let _ = answer.send((reply, None));
        });
        self.api_reads.push((caller, request, answered));
    }

    /// A let-go Thread's transcript, read back from its log on a thread of
    /// its own while the pump goes on serving every window and Session,
    /// and answered as `answer` reads it.
    fn read_back_off_pump(
        &mut self,
        caller: CallerId,
        request: String,
        read_back: std::io::Result<ReadBack>,
        answer_with: impl FnOnce(&ferrite_core::transcript::Transcript) -> Value + Send + 'static,
    ) {
        let (answer, answered) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            ferrite_core::spawn::serve_the_operator();
            let read = read_back.map_err(LoadError::Io).and_then(ReadBack::read);
            let reply = match &read {
                Ok(read) => Ok(answer_with(read.view().main().transcript())),
                Err(error) => Err(ApiError::new(
                    Code::Failed,
                    format!("the Thread's log could not be read back: {error}"),
                )),
            };
            let _ = answer.send((reply, read.ok()));
        });
        self.api_reads.push((caller, request, answered));
    }

    /// Reply to each read off the pump that finished, and hold what it read.
    pub(super) fn answer_api_reads(&mut self) {
        let mut answered = Vec::new();
        self.api_reads
            .retain(|(caller, request, answer)| match answer.try_recv() {
                Ok((reply, read)) => {
                    answered.push((*caller, request.clone(), reply, read));
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let ended = ApiError::new(Code::Failed, "the read ended");
                    answered.push((*caller, request.clone(), Err(ended), None));
                    false
                }
            });
        for (caller, request, reply, read) in answered {
            if let Some(read) = read {
                self.engine.adopt(read);
            }
            self.api.reply(caller, request, reply);
        }
    }

    /// A Decision is named by the id the provider gave it, as
    /// `decision.list` shows it; the answer is the window's own `answer`,
    /// first valid answer wins.
    fn answer(
        &mut self,
        caller: CallerId,
        params: DecisionAnswerParams,
    ) -> Result<Value, ApiError> {
        let thread: ThreadId = params.thread;
        if self.engine.cap(thread).is_some()
            && matches!(
                params.answer,
                WireAnswer::AllowAlways { .. } | WireAnswer::Choose { .. }
            )
        {
            return Err(ApiError::new(
                Code::WidensCap,
                "a capped Thread's Decision is allowed once or refused over the API: an answer \
                 that remembers a choice would widen what its agent does without asking, and is \
                 the operator's own, from the window",
            ));
        }
        let handle = self
            .engine
            .pending_decisions()
            .into_iter()
            .find(|pending| pending.thread == thread && pending.decision.id == params.decision)
            .map(|pending| pending.handle)
            .ok_or_else(|| {
                ApiError::new(
                    Code::UnknownDecision,
                    "no pending Decision on that Thread has that id",
                )
            })?;
        self.execute(Op::Answer {
            thread,
            handle: handle.clone(),
            answer: params.answer,
        })?;
        self.answered.insert((thread, handle), caller);
        thread_result(&self.engine, thread)
    }
}

/// Refused `widens_cap`, naming it, when a capped launch on `provider`
/// carries what would set or widen how far its agent goes: under a cap
/// the Server alone sets that.
fn refuse_widening(
    provider: Provider,
    args: &[String],
    env: &[(String, String)],
    launcher: &[String],
) -> Result<(), ApiError> {
    match ferrite_core::runtime_mode::widening(provider, args, env, launcher) {
        Some(widening) => Err(ApiError::new(Code::WidensCap, widening.to_string())),
        None => Ok(()),
    }
}

/// `agent.read`'s answer: the last lines of `transcript` as plain text.
fn read_answer(params: &AgentReadParams, transcript: &ferrite_core::transcript::Transcript) -> Value {
    let text = ferrite_core::transcript::plain_text(transcript.blocks());
    let lines: Vec<&str> = text.lines().collect();
    let from = lines
        .len()
        .saturating_sub(params.lines.unwrap_or(READ_LINES));
    json!({ "thread": params.thread, "text": lines[from..].join("\n") })
}

/// Who a request comes from. A call that names no Thread is the
/// operator's CLI.
#[derive(Debug, Clone, Copy)]
enum Caller {
    Operator,
    /// An ordinary Thread: the operator's agent.
    Thread,
    /// A Thread that is, or was, a Task's Worker.
    Worker(ThreadId),
    Coordinator(ThreadId),
}

impl Caller {
    /// Refused unless `method`'s callers ([`Callers`]) take this one.
    fn may(self, method: &Method) -> Result<(), ApiError> {
        match method.callers() {
            Callers::Anyone => Ok(()),
            Callers::Operator => match self {
                Caller::Coordinator(_) => Err(ApiError::new(
                    Code::ProposeInstead,
                    format!(
                        "the Coordinator does not {}: raise it as a proposal for the user to \
                         accept, with `\"${{FERRITE_BIN:-ferrite}}\" proposal raise`",
                        method.name()
                    ),
                )),
                _ => Ok(()),
            },
            Callers::Worker => self.worker().map(drop),
            Callers::Coordinator => self.coordinator().map(drop),
        }
    }

    /// The Worker that reports: the Thread the call runs in, never one it
    /// names.
    fn worker(self) -> Result<ThreadId, ApiError> {
        match self {
            Caller::Worker(thread) => Ok(thread),
            _ => Err(ApiError::new(
                Code::NotAWorker,
                "a report is a Task's Worker's own: this call runs in no Worker's Thread",
            )),
        }
    }

    fn coordinator(self) -> Result<ThreadId, ApiError> {
        match self {
            Caller::Coordinator(thread) => Ok(thread),
            _ => Err(ApiError::new(
                Code::NotACoordinator,
                "a proposal is a Coordinator's own: this call runs in no Coordinator's Thread",
            )),
        }
    }
}

/// Where a started Thread works: a linked worktree `cwd` is, opened
/// rather than made (Herdr's `worktree open`, or a tab in a worktree
/// `worktree.create` made); else `cwd` itself, or with `worktree` a new
/// worktree of its repository, on `branch` from `base` when named.
fn workspace_choice(
    cwd: PathBuf,
    worktree: bool,
    branch: Option<String>,
    base: Option<String>,
) -> Result<WorkspaceChoice, ApiError> {
    let invalid = |message: &str| Err(ApiError::new(Code::InvalidRequest, message));
    if base.is_some() && branch.is_none() {
        return invalid("base goes with branch");
    }
    if let Some(checkout) = workspace::checkout_of(&cwd).filter(|checkout| checkout.linked) {
        if branch.is_some() {
            return invalid("cwd is a worktree already: it is opened, so it takes no branch");
        }
        return Ok(WorkspaceChoice::ExistingWorktree {
            repo: checkout.repo,
            path: cwd,
        });
    }
    Ok(match (worktree, branch) {
        (false, Some(_)) => return invalid("branch goes with worktree"),
        (false, None) => WorkspaceChoice::Main { checkout: cwd },
        (true, Some(branch)) => WorkspaceChoice::NamedWorktree {
            repo: cwd,
            branch,
            base,
        },
        (true, None) => WorkspaceChoice::NewWorktree { repo: cwd },
    })
}
