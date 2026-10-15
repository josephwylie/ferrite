//! The API's requests (ADR 0027), answered on the pump thread. Each one
//! that changes a Thread is the window's own command (`Engine::execute`),
//! so an agent and the operator act through the same door.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::Server;
use crate::api::schema::{
    AgentPromptParams, AgentReadParams, AgentWaitParams, DecisionAnswerParams, ThreadStartParams,
};
use crate::api::{
    self, thread_info, thread_result, unknown_thread, ApiError, CallerId, Code, Method, Request,
    ThreadInfo, Until, Wait,
};
use crate::engine::Launch;
use crate::protocol::{Done, Op, ProjectAction, ServerMessage};
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

    fn api_request(&mut self, caller: CallerId, Request { id, method }: Request) {
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
                        self.api.wait(&self.engine, caller, id, thread, wait);
                        return;
                    }
                    (Ok(thread), None) => thread_result(&self.engine, thread),
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
                self.execute(Op::Park { thread })?;
                Ok(json!({ "thread": thread }))
            }),
            Method::AgentPrompt(params) => match self.prompt(&params) {
                Ok(()) if params.wait || !params.until.is_empty() => {
                    let until = match params.until.is_empty() {
                        true => Until::Settled,
                        false => Until::AnyOf(params.until),
                    };
                    let wait = Wait::after_prompt_until(until, timeout(params.timeout_ms), now);
                    self.api.wait(&self.engine, caller, id, params.thread, wait);
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
                    self.api.wait(&self.engine, caller, id, thread, wait);
                    return;
                }
                None => Err(unknown_thread()),
            },
            Method::AgentRead(params) => self.read(params),
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
            Method::DecisionAnswer(params) => self.answer(caller, params),
            Method::ProjectList(_) => Ok(json!({ "projects": api::project_infos(&self.engine) })),
            Method::ProjectGet(params) => self
                .project(&params.project)
                .and_then(|project| self.project_result(project)),
            Method::ProjectOpen(params) => self.open_project(params),
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
            Method::NavFocus(params) => self.focus_nav(params),
            Method::EventsSubscribe(params) => {
                self.api.subscribe(&self.engine, caller, id, params.threads);
                return;
            }
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
        self.api.reply(caller, id, result);
    }

    pub(super) fn execute(&mut self, op: Op) -> Result<Option<Done>, ApiError> {
        self.engine.execute(&op).map_err(ApiError::from)
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
                let cwd = std::fs::canonicalize(&params.cwd)
                    .map_err(|error| ApiError::new(Code::Failed, error.to_string()))?;
                let directory = self.directory_in(project, &cwd)?;
                // As Herdr's tab starts in its cwd, the Thread works in
                // `cwd` itself; a fresh worktree is of its directory.
                let checkout = if params.worktree { directory } else { cwd };
                (checkout, Some(project))
            }
            None => (params.cwd, None),
        };
        let workspace = workspace_choice(checkout, params.worktree, params.branch, params.base)?;
        let launch = Launch {
            args: params.args,
            resume: params.resume,
            project,
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

    /// Herdr's rule: nothing is sent to an agent a Decision blocks, since
    /// the prompt would read as that Decision's answer or be lost behind
    /// it. A prompt to a Session still starting is held until it is up, as
    /// a send to a stopped one is; only one is held, so a second is
    /// refused rather than replace the first. A prompt to a working agent
    /// waits its turn in the provider's queue, as the Composer's does.
    pub(super) fn prompt(&mut self, params: &AgentPromptParams) -> Result<(), ApiError> {
        let (thread, text) = (params.thread, params.text.clone());
        let view = self.engine.thread(thread).ok_or_else(unknown_thread)?;
        if view.pending().is_some() {
            return Err(ApiError::new(
                Code::AgentBlocked,
                "a Decision waits on this Thread; nothing was sent",
            ));
        }
        let op = if view.starting() {
            if self.engine.holds_prompt(thread) {
                return Err(ApiError::new(
                    Code::AgentStarting,
                    "this Thread's Session is starting and already holds a prompt; \
                     nothing was sent: retry once the Thread is working",
                ));
            }
            Op::Send { thread, text }
        } else if view.needs_queue() {
            Op::Queue { thread, text }
        } else {
            Op::Send { thread, text }
        };
        self.execute(op).map(|_| ())
    }

    fn read(&self, params: AgentReadParams) -> Result<Value, ApiError> {
        let view = self
            .engine
            .thread(params.thread)
            .ok_or_else(unknown_thread)?;
        let text = crate::transcript::plain_text(view.transcript().blocks());
        let lines: Vec<&str> = text.lines().collect();
        let from = lines
            .len()
            .saturating_sub(params.lines.unwrap_or(READ_LINES));
        Ok(json!({ "thread": params.thread, "text": lines[from..].join("\n") }))
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
