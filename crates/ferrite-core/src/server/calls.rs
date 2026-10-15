//! The API's requests (ADR 0026), answered on the pump thread. Each one
//! that changes a Thread is the window's own command (`Engine::execute`),
//! so an agent and the operator act through the same door.

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
use crate::protocol::{Done, Op, ServerMessage, WireChoice};
use crate::ThreadId;

/// How much of a transcript `agent.read` gives when not told.
const READ_LINES: usize = 80;

impl Server {
    pub(super) fn api_heard(&mut self, heard: api::Heard) {
        match heard {
            api::Heard::Joined(caller, answers) => self.api.join(caller, answers),
            api::Heard::Request(caller, request) => self.api_request(caller, request),
            api::Heard::Gone(caller) => self.api.leave(caller),
        }
    }

    fn api_request(&mut self, caller: CallerId, Request { id, method }: Request) {
        // What changed before the request reaches the windows and the
        // subscribers first.
        self.broadcast();
        let now = Instant::now();
        let timeout = |ms: Option<u64>| ms.map(Duration::from_millis);
        let result = match method {
            Method::ThreadStart(params) => self.start_thread(params),
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
            Method::AgentPrompt(params) => match self.prompt(&params) {
                Ok(()) if params.wait => {
                    let wait = Wait::after_prompt(timeout(params.timeout_ms), now);
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
            Method::DecisionList(params) => {
                let decisions: Vec<_> = self
                    .engine
                    .pending_decisions()
                    .into_iter()
                    .filter(|pending| params.thread.is_none_or(|thread| pending.thread == thread))
                    .collect();
                Ok(json!({ "decisions": decisions }))
            }
            Method::DecisionAnswer(params) => self.answer(caller, params),
            Method::EventsSubscribe(params) => {
                self.api.subscribe(&self.engine, caller, id, params.threads);
                return;
            }
            Method::NotificationShow(params) => {
                let windows = self
                    .clients
                    .values()
                    .filter(|client| client.welcomed)
                    .count();
                self.say(ServerMessage::Notification {
                    title: params.title,
                    body: params.body,
                });
                Ok(json!({ "windows": windows }))
            }
        };
        // What the request changed reaches the windows before its answer.
        self.broadcast();
        self.api.reply(caller, id, result);
    }

    fn execute(&mut self, op: Op) -> Result<Option<Done>, ApiError> {
        self.engine.execute(&op).map_err(ApiError::from)
    }

    /// Open a Thread as a window's `open` does, then title it.
    fn start_thread(&mut self, params: ThreadStartParams) -> Result<Value, ApiError> {
        let workspace = if params.worktree {
            WireChoice::NewWorktree { repo: params.cwd }
        } else {
            WireChoice::Main {
                checkout: params.cwd,
            }
        };
        let Some(Done::Thread { thread }) = self.execute(Op::Open {
            provider: params.provider.unwrap_or(self.default_provider),
            workspace,
        })?
        else {
            return Err(ApiError::new(Code::Failed, "no Thread was opened"));
        };
        if let Some(title) = params.title {
            self.execute(Op::Rename { thread, title })?;
        }
        thread_result(&self.engine, thread)
    }

    /// Herdr's rule: nothing is sent to an agent a Decision blocks, since
    /// the prompt would read as that Decision's answer or be lost behind
    /// it. A prompt to a Session still starting is held until it is up, as
    /// a send to a stopped one is; only one is held, so a second is
    /// refused rather than replace the first. A prompt to a working agent
    /// waits its turn in the provider's queue, as the Composer's does.
    fn prompt(&mut self, params: &AgentPromptParams) -> Result<(), ApiError> {
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
