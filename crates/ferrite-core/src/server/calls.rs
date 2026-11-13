//! The API's requests (ADR 0027), answered on the pump thread. Each one
//! that changes a Thread is the window's own command (`Engine::execute`),
//! so an agent and the operator act through the same door.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::Server;
use crate::api::schema::Callers;
use crate::api::schema::{
    AgentPromptParams, AgentReadParams, AgentWaitParams, DecisionAnswerParams, ThreadReviveParams,
    ThreadStartParams,
};
use crate::api::{
    self, thread_info, thread_result, unknown_thread, ApiError, CallerId, Code, Method, Request,
    ThreadInfo, Until, Wait,
};
use crate::drive::Action;
use crate::engine::{Launch, ReadBack};
use crate::protocol::{Done, Op, ProjectAction, ServerMessage};
use crate::store::LoadError;
use crate::workspace::{self, WorkspaceChoice};
use crate::ThreadId;

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
                let ready = params.ready.then(|| {
                    let within = timeout(params.timeout_ms).unwrap_or(api::READY);
                    Wait::until(Until::Ready, Some(within), now)
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
            Method::ThreadGet(params) => thread_result(&self.engine, params.thread),
            Method::ThreadFocus(params) => {
                let result = thread_result(&self.engine, params.thread);
                if result.is_ok() {
                    self.say(ServerMessage::Focus {
                        thread: params.thread,
                    });
                }
                result
            }
            Method::ThreadRename(params) => self
                .execute(Op::Rename {
                    thread: params.thread,
                    title: params.title,
                })
                .and_then(|_| thread_result(&self.engine, params.thread)),
            Method::ThreadClose(params) => self.open_thread(params.thread).and_then(|thread| {
                self.park(thread)?;
                Ok(json!({ "thread": thread }))
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
                until,
                timeout_ms,
            }) => match self.engine.thread(thread) {
                Some(_) => {
                    let until = until.map_or(Until::Settled, Until::Attention);
                    let wait = Wait::until(until, timeout(timeout_ms), now);
                    self.api.wait(&self.engine, caller_id, id, thread, wait);
                    return;
                }
                None => Err(unknown_thread()),
            },
            Method::AgentRead(params) => match self.engine.read_back_later(params.thread) {
                Some(read_back) => {
                    self.read_back_off_pump(caller_id, id, params, read_back);
                    return;
                }
                None => self.read(&params),
            },
            Method::AgentInterrupt(params) => self.open_thread(params.thread).and_then(|thread| {
                self.execute(Op::Interrupt { thread })?;
                thread_result(&self.engine, thread)
            }),
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
            Method::MachineList(_) => self.machines(),
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
                crate::factory::Command::Stop { task: params.task },
            ),
            Method::TaskResume(params) => self.api_task_command(
                &params.task.clone(),
                crate::factory::Command::Resume { task: params.task },
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
                crate::factory::Command::RenameWorkflow {
                    workflow: params.workflow,
                    name: params.name,
                },
            ),
            Method::WorkflowSave(params) => self.api_workflow_save(params.workflow),
            Method::StepAdd(params) => self.api_author(
                params.workflow.clone(),
                crate::factory::Command::AddStep {
                    workflow: params.workflow,
                    after: params.after,
                    agent: self.factory_agent.clone(),
                },
            ),
            Method::StepEdit(params) => self.api_author(
                params.workflow.clone(),
                crate::factory::Command::EditStep {
                    workflow: params.workflow,
                    step: params.step,
                    change: params.change,
                },
            ),
            Method::StepMove(params) => self.api_author(
                params.workflow.clone(),
                crate::factory::Command::MoveStep {
                    workflow: params.workflow,
                    step: params.step,
                    to: params.to,
                },
            ),
            Method::StepDelete(params) => self.api_author(
                params.workflow.clone(),
                crate::factory::Command::DeleteStep {
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
                    crate::factory::Command::ReportRevision {
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
                    crate::factory::Command::ReportEvidence {
                        worker,
                        text: params.text,
                    },
                )
            }),
            Method::TaskAnswer(params) => {
                let task = params.task.clone();
                self.api_task_command(
                    &task,
                    crate::factory::Command::AnswerQuestion {
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
                    crate::factory::Command::Approve { task: params.task },
                )
            }
            Method::TaskRework(params) => self.api_task_rework(params),
            Method::TaskRedirect(params) => self.api_task_redirect(params),
            Method::WorkflowApproval(params) => self.api_author(
                params.workflow.clone(),
                crate::factory::Command::EditApproval {
                    workflow: params.workflow,
                    change: params.change,
                },
            ),
            Method::WorkflowStart(params) => self.api_automate(
                params.workflow.clone(),
                crate::factory::Command::EditStart {
                    workflow: params.workflow,
                    start: params.start,
                },
            ),
            Method::WorkflowAutomation(params) => self.api_automate(
                params.workflow.clone(),
                crate::factory::Command::Automation {
                    workflow: params.workflow,
                    turn: params.turn,
                },
            ),
            Method::WorkflowBacklog(params) => self.api_automate(
                params.workflow.clone(),
                crate::factory::Command::Backlog {
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
            Method::WindowShot(shot) => return self.drive(caller_id, id, Action::Shot(shot)),
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

    /// Park a Thread as the window's park does (`Cockpit::park_thread`),
    /// with the same two commands: a Group member leaves its Group first,
    /// so no window's board keeps a seat for a Thread that went away.
    /// The leave reaches the windows before the park, as each of a window's
    /// commands does: published together, the Roster that closes the Thread
    /// goes out ahead of the Groups it left, and a window between the two
    /// holds a closed Thread in its Group, its focus still on it.
    pub(super) fn park(&mut self, thread: ThreadId) -> Result<(), ApiError> {
        if self.engine.groups().of(thread).is_some() {
            self.execute(Op::Group {
                change: (&crate::groups::GroupChange::Leave { thread }).into(),
            })?;
            self.broadcast();
        }
        self.execute(Op::Park { thread }).map(drop)
    }

    /// How many windows hear what the Server says to them all.
    fn windows(&self) -> usize {
        self.clients
            .values()
            .filter(|client| client.welcomed)
            .count()
    }

    /// An open Thread, or `unknown_thread`.
    fn open_thread(&self, thread: ThreadId) -> Result<ThreadId, ApiError> {
        self.engine
            .thread(thread)
            .map(|_| thread)
            .ok_or_else(unknown_thread)
    }

    /// Open a Thread as a window's `open` does, its CLI given `args`, then
    /// title it and land the windows on it as asked.
    fn start_thread(&mut self, params: ThreadStartParams) -> Result<ThreadId, ApiError> {
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
            Some(api::schema::Owner::Task(task)) => Some(crate::ThreadOwner::Task(task)),
            Some(api::schema::Owner::Factory(project)) => {
                Some(crate::ThreadOwner::Factory(self.project(&project)?))
            }
        };
        let workspace = workspace_choice(checkout, params.worktree, params.branch, params.base)?;
        let launch = Launch {
            launch_provider: params.provider,
            args: params.args,
            env: params.env.into_iter().collect(),
            launcher: params.launcher,
            resume: params.resume,
            project,
            owner,
        };
        let thread = self
            .engine
            .open_with(
                params.provider.unwrap_or(self.default_provider),
                workspace,
                launch,
            )
            .map_err(|error| ApiError::new(Code::Failed, error.to_string()))?;
        if let Some(title) = params.title {
            self.execute(Op::Rename { thread, title })?;
        }
        if params.focus {
            self.say(ServerMessage::Focus { thread });
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
            project: None,
            owner: None,
        };
        self.engine
            .revive_with(thread, launch)
            .map_err(|error| ApiError::new(Code::Failed, error.to_string()))
    }

    /// Herdr's rule: nothing is sent to an agent a Decision blocks, since
    /// the prompt would read as that Decision's answer or be lost behind
    /// it. A prompt to a Session still starting is held until it is up, as
    /// a send to a stopped one is, and any after it wait behind it. A
    /// prompt to a working agent waits its turn in the provider's queue, as
    /// the Composer's does.
    pub(super) fn prompt(&mut self, params: &AgentPromptParams) -> Result<(), ApiError> {
        let (thread, text) = (params.thread, params.text.clone());
        let view = self.engine.thread(thread).ok_or_else(unknown_thread)?;
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
            .ok_or_else(unknown_thread)?;
        Ok(read_answer(params, view.transcript()))
    }

    /// A let-go Thread's transcript, read back from its log on a thread of
    /// its own: the pump goes on serving every window and Session, and the
    /// Thread stays let go.
    fn read_back_off_pump(
        &mut self,
        caller: CallerId,
        request: String,
        params: AgentReadParams,
        read_back: std::io::Result<ReadBack>,
    ) {
        let (answer, answered) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            crate::spawn::serve_the_operator();
            let read = read_back
                .map_err(LoadError::Io)
                .and_then(ReadBack::activity)
                .map(|activity| read_answer(&params, activity.view().main().transcript()))
                .map_err(|error| {
                    ApiError::new(
                        Code::Failed,
                        format!("the Thread's log could not be read back: {error}"),
                    )
                });
            let _ = answer.send(read);
        });
        self.api_reads.push((caller, request, answered));
    }

    /// Reply to each read off the pump that finished.
    pub(super) fn answer_api_reads(&mut self) {
        let mut answered = Vec::new();
        self.api_reads
            .retain(|(caller, request, answer)| match answer.try_recv() {
                Ok(read) => {
                    answered.push((*caller, request.clone(), read));
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let ended = ApiError::new(Code::Failed, "the read ended");
                    answered.push((*caller, request.clone(), Err(ended)));
                    false
                }
            });
        for (caller, request, read) in answered {
            self.api.reply(caller, request, read);
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

/// `agent.read`'s answer: the last lines of `transcript` as plain text.
fn read_answer(params: &AgentReadParams, transcript: &crate::transcript::Transcript) -> Value {
    let text = crate::transcript::plain_text(transcript.blocks());
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
