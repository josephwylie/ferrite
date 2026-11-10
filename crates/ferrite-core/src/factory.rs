//! The Factory (ADR 0037): each Project's delegated work, headless. It
//! knows nothing of the Server or a Provider's process. Its interface is
//! three parts:
//!
//! - [`Factory::command`]: every change, whoever asks (a window, a Worker,
//!   a script), through one door, answered or refused.
//! - [`Factory::step`]: once per Server pass, given the time and what each
//!   Worker's Thread is doing, it answers with [`Effect`]s for the Server
//!   to carry out. It reads no file and touches no agent while deciding.
//! - [`Factory::board`]: what the windows draw, with a [`Factory::revision`]
//!   that moves whenever it changes.
//!
//! Persist, then act: every change is written beside the Store before the
//! effects that depend on it are handed out, and a change that cannot be
//! written is not made. A repeated command or report changes nothing.
//!
//! This is the one-off slice (#220): a Task is a single Step run by one
//! Worker on the operator's default Provider and model.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::Provider;
use crate::workspace::registry::ProjectId;
use crate::ThreadId;

/// A Task's id: `t-<n>`, never given twice by one Server. A Worker's
/// owner names it (`{"task": "t-1"}`).
pub type TaskId = String;

/// Where the Factory keeps itself in the Store's directory.
pub const FILE_NAME: &str = "factory.json";
const SCHEMA_VERSION: u32 = 1;

/// How long a Worker prompted to report may stay at rest without working
/// again before its Task needs the operator.
pub const NUDGE_GRACE_MS: i64 = 60_000;

/// The board's columns, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    Working,
    NeedsHuman,
    /// Ready to approve (no one-off Task reaches it).
    Approval,
    /// Stopped or failed.
    Stopped,
    Completed,
}

impl Status {
    pub const ALL: [Status; 6] = [
        Status::Queued,
        Status::Working,
        Status::NeedsHuman,
        Status::Approval,
        Status::Stopped,
        Status::Completed,
    ];

    /// The board's column heading.
    pub fn column(self) -> &'static str {
        match self {
            Status::Queued => "Queued",
            Status::Working => "Working",
            Status::NeedsHuman => "Needs human",
            Status::Approval => "Ready to approve",
            Status::Stopped => "Stopped or failed",
            Status::Completed => "Completed",
        }
    }

    /// What a Task in it is called, alone.
    pub fn word(self) -> &'static str {
        match self {
            Status::Stopped => "Stopped",
            status => status.column(),
        }
    }

    /// The operator is wanted: the sidebar counts these.
    pub fn needs_you(self) -> bool {
        matches!(self, Status::NeedsHuman | Status::Approval)
    }

    /// Delete is offered.
    pub fn deletable(self) -> bool {
        matches!(self, Status::Queued | Status::Stopped | Status::Completed)
    }
}

/// What runs a Step: a Provider, its model and Effort.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub provider: Provider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

/// Who did what, in a Task's activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    Ferrite,
    Agent,
    Operator,
}

impl Actor {
    pub fn word(self) -> &'static str {
        match self {
            Actor::Ferrite => "Ferrite",
            Actor::Agent => "Agent",
            Actor::Operator => "You",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Wall-clock milliseconds since the Unix epoch.
    pub at_ms: i64,
    pub by: Actor,
    pub text: String,
}

/// One piece of delegated work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub project: ProjectId,
    /// The instructions' first line.
    pub title: String,
    pub instructions: String,
    pub status: Status,
    /// What its Worker runs on.
    pub agent: Agent,
    /// Its own branch, in its own worktree.
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<ThreadId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<PathBuf>,
    /// What the Worker reported it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// Why it needs the operator, or why it stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub activity: Vec<Entry>,
    pub created_ms: i64,
    /// The start request it answers, so a repeat is the same Task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    /// The at-rest rule's memory of the current turn.
    #[serde(default, skip_serializing_if = "Watch::is_fresh")]
    pub watch: Watch,
}

/// What the at-rest rule remembers of a working Worker.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watch {
    /// How many turns the Worker had ended ([`Seen::turns`]) when it was
    /// last prompted: one more is the turn that prompt began, ended.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub baseline: u64,
    /// When it was asked to report, if it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nudged_ms: Option<i64>,
    /// A prompt for the next step to send, where its baseline is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Someone else prompted it (the operator): take the baseline afresh.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rebase: bool,
}

impl Watch {
    fn is_fresh(&self) -> bool {
        *self == Self::default()
    }
}

fn is_zero(count: &u64) -> bool {
    *count == 0
}

/// What the windows draw: every Project's Tasks, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    pub tasks: Vec<Task>,
}

impl Board {
    pub fn of(&self, project: ProjectId) -> impl Iterator<Item = &Task> {
        self.tasks
            .iter()
            .filter(move |task| task.project == project)
    }

    /// How many of `project`'s Tasks need the operator.
    pub fn needs_you(&self, project: ProjectId) -> usize {
        self.of(project)
            .filter(|task| task.status.needs_you())
            .count()
    }

    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }
}

/// What a Worker reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Finished { summary: String },
    Unable { tried: String },
}

/// Every change, whoever asks.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// A one-off Task: one Worker on `agent`, with `instructions`.
    /// `request`, when given, makes a repeat the same Task.
    Start {
        project: ProjectId,
        instructions: String,
        agent: Agent,
        request: Option<String>,
    },
    /// The Server started the Worker a [`Effect::StartWorker`] asked for.
    WorkerStarted {
        task: TaskId,
        thread: ThreadId,
        worktree: Option<PathBuf>,
    },
    /// The Server could not.
    WorkerFailed {
        task: TaskId,
        why: String,
    },
    /// A Worker reports, known by its own Thread.
    Report {
        worker: ThreadId,
        outcome: Outcome,
    },
    /// The operator said something to a Worker in its Task.
    OperatorSaid {
        worker: ThreadId,
        text: String,
    },
    Stop {
        task: TaskId,
    },
    Resume {
        task: TaskId,
    },
    Delete {
        task: TaskId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Task(TaskId),
    /// Whether the command changed anything: a repeat does not.
    Changed(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalCode {
    UnknownTask,
    /// The Thread is no Task's Worker.
    NotAWorker,
    /// Not in the Task's current state.
    NotNow,
    Invalid,
    /// The change could not be written; nothing changed.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: RefusalCode,
    pub message: String,
}

impl Refusal {
    fn new(code: RefusalCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// What the Server tells the step of one Worker's Thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    /// Not at rest: Main, a Subagent or a background task working, a
    /// prompt held for its starting Session, or a Decision waiting.
    pub busy: bool,
    /// A count that grows each time one of its turns ends (its
    /// `completion_seq`; 0 before its first).
    pub turns: u64,
}

/// What the Server carries out, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Start an owned Thread for `task` on `agent`, in a new worktree on
    /// `branch`, and send it `prompt`.
    StartWorker {
        task: TaskId,
        project: ProjectId,
        agent: Agent,
        branch: String,
        prompt: String,
    },
    Prompt {
        thread: ThreadId,
        text: String,
    },
    Interrupt {
        thread: ThreadId,
    },
    Park {
        thread: ThreadId,
    },
    /// Remove a deleted Task's worktree, only if its checkout is clean;
    /// its branch stays.
    RemoveWorktree {
        path: PathBuf,
    },
    /// Toast, chime and system notification in the windows.
    Notify {
        task: TaskId,
        project: ProjectId,
        status: Status,
        title: String,
    },
}

/// What is written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct State {
    version: u32,
    /// The next Task's number.
    next: u64,
    tasks: Vec<Task>,
}

pub struct Factory {
    state: State,
    /// Where it is written; `None` keeps it in memory (tests).
    path: Option<PathBuf>,
    revision: u64,
    /// Effects decided by commands, handed out by the next step.
    pending: Vec<Effect>,
}

impl Factory {
    pub fn in_memory() -> Self {
        Self {
            state: State {
                version: SCHEMA_VERSION,
                next: 1,
                tasks: Vec::new(),
            },
            path: None,
            revision: 0,
            pending: Vec::new(),
        }
    }

    /// The Factory written at `path`, or a fresh one when there is none.
    /// One written by a newer Ferrite is refused rather than overwritten.
    pub fn load(path: &Path) -> io::Result<Self> {
        let mut factory = Self::in_memory();
        match std::fs::read(path) {
            Ok(bytes) => {
                let state: State = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                if state.version > SCHEMA_VERSION {
                    return Err(io::Error::other(format!(
                        "{} is from a newer Ferrite (version {})",
                        path.display(),
                        state.version
                    )));
                }
                factory.state = state;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        factory.path = Some(path.to_path_buf());
        Ok(factory)
    }

    /// One change, whoever asks. Its effects come out of the next step.
    pub fn command(&mut self, command: Command, now_ms: i64) -> Result<Answer, Refusal> {
        self.change(|state, effects| state.command(command, now_ms, effects))
    }

    /// One pass: what to do now, given the time and what each Worker's
    /// Thread is doing. A Worker the Server does not see (no open Thread)
    /// is left as it is.
    pub fn step(&mut self, now_ms: i64, workers: &BTreeMap<ThreadId, Seen>) -> Vec<Effect> {
        // A step that cannot be written decides nothing; the next pass
        // tries again.
        let _ = self.change(|state, effects| {
            state.step(now_ms, workers, effects);
            Ok(())
        });
        std::mem::take(&mut self.pending)
    }

    /// Apply `change` to a copy, write the copy, and only then take it and
    /// its effects: a change that cannot be written is not made.
    fn change<T>(
        &mut self,
        change: impl FnOnce(&mut State, &mut Vec<Effect>) -> Result<T, Refusal>,
    ) -> Result<T, Refusal> {
        let mut next = self.state.clone();
        let mut effects = Vec::new();
        let answer = change(&mut next, &mut effects)?;
        if next != self.state {
            if let Some(path) = &self.path {
                let bytes = serde_json::to_vec_pretty(&next)
                    .map_err(|error| Refusal::new(RefusalCode::Failed, error.to_string()))?;
                crate::store::replace_file(path, &bytes).map_err(|error| {
                    Refusal::new(
                        RefusalCode::Failed,
                        format!("the Factory could not be saved: {error}"),
                    )
                })?;
            }
            self.state = next;
            self.revision += 1;
        }
        self.pending.extend(effects);
        Ok(answer)
    }

    pub fn board(&self) -> Board {
        Board {
            tasks: self.state.tasks.clone(),
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn task(&self, id: &str) -> Option<&Task> {
        self.state.tasks.iter().find(|task| task.id == id)
    }

    /// The Task `thread` is the Worker of.
    pub fn task_of(&self, thread: ThreadId) -> Option<&Task> {
        self.state
            .tasks
            .iter()
            .find(|task| task.worker == Some(thread))
    }
}

fn unknown_task() -> Refusal {
    Refusal::new(RefusalCode::UnknownTask, "no Task has that id")
}

fn not_a_worker() -> Refusal {
    Refusal::new(RefusalCode::NotAWorker, "this Thread is no Task's Worker")
}

impl Task {
    fn note(&mut self, at_ms: i64, by: Actor, text: impl Into<String>) {
        self.activity.push(Entry {
            at_ms,
            by,
            text: text.into(),
        });
    }

    /// Move to `status`, telling the operator when it is one they hear of.
    fn enter(&mut self, status: Status, effects: &mut Vec<Effect>) {
        self.status = status;
        self.watch = Watch::default();
        let title = match status {
            Status::NeedsHuman => "Task needs you",
            Status::Approval => "Task ready to approve",
            Status::Stopped => "Task stopped",
            Status::Completed => "Task completed",
            Status::Queued | Status::Working => return,
        };
        effects.push(Effect::Notify {
            task: self.id.clone(),
            project: self.project,
            status,
            title: format!("{title}: {}", self.title),
        });
    }
}

impl State {
    fn task_mut(&mut self, id: &str) -> Result<&mut Task, Refusal> {
        self.tasks
            .iter_mut()
            .find(|task| task.id == id)
            .ok_or_else(unknown_task)
    }

    fn worker_task(&mut self, worker: ThreadId) -> Result<&mut Task, Refusal> {
        self.tasks
            .iter_mut()
            .find(|task| task.worker == Some(worker))
            .ok_or_else(not_a_worker)
    }

    fn command(
        &mut self,
        command: Command,
        now_ms: i64,
        effects: &mut Vec<Effect>,
    ) -> Result<Answer, Refusal> {
        match command {
            Command::Start {
                project,
                instructions,
                agent,
                request,
            } => {
                if let Some(task) = request.as_ref().and_then(|request| {
                    self.tasks
                        .iter()
                        .find(|task| task.request.as_ref() == Some(request))
                }) {
                    return Ok(Answer::Task(task.id.clone()));
                }
                let instructions = instructions.trim().to_string();
                let Some(title) = instructions.lines().next().map(str::trim) else {
                    return Err(Refusal::new(
                        RefusalCode::Invalid,
                        "a Task needs instructions",
                    ));
                };
                let id = format!("t-{}", self.next);
                self.next += 1;
                let mut task = Task {
                    title: title.to_string(),
                    branch: format!("factory/{id}"),
                    id: id.clone(),
                    project,
                    instructions,
                    status: Status::Queued,
                    agent,
                    worker: None,
                    worktree: None,
                    result: None,
                    reason: None,
                    activity: Vec::new(),
                    created_ms: now_ms,
                    request,
                    watch: Watch::default(),
                };
                task.note(now_ms, Actor::Operator, "Started the task");
                self.tasks.push(task);
                Ok(Answer::Task(id))
            }
            Command::WorkerStarted {
                task,
                thread,
                worktree,
            } => {
                let task = self.task_mut(&task)?;
                if task.worker == Some(thread) {
                    return Ok(Answer::Changed(false));
                }
                task.worker = Some(thread);
                task.worktree = worktree;
                Ok(Answer::Changed(true))
            }
            Command::WorkerFailed { task, why } => {
                let task = self.task_mut(&task)?;
                if task.status != Status::Working || task.worker.is_some() {
                    return Ok(Answer::Changed(false));
                }
                task.note(
                    now_ms,
                    Actor::Ferrite,
                    format!("Could not start the agent: {why}"),
                );
                task.reason = Some(format!("The agent could not start: {why}"));
                task.enter(Status::Stopped, effects);
                Ok(Answer::Changed(true))
            }
            Command::Report { worker, outcome } => {
                let task = self.worker_task(worker)?;
                if task.status != Status::Working {
                    return Ok(Answer::Changed(false));
                }
                match outcome {
                    Outcome::Finished { summary } => {
                        task.note(now_ms, Actor::Agent, format!("Finished: {summary}"));
                        task.result = Some(summary);
                        task.reason = None;
                        task.enter(Status::Completed, effects);
                    }
                    Outcome::Unable { tried } => {
                        task.note(now_ms, Actor::Agent, format!("Unable to finish: {tried}"));
                        task.reason = Some(tried);
                        task.enter(Status::NeedsHuman, effects);
                    }
                }
                Ok(Answer::Changed(true))
            }
            Command::OperatorSaid { worker, text } => {
                let task = self.worker_task(worker)?;
                task.note(now_ms, Actor::Operator, text);
                if matches!(task.status, Status::NeedsHuman | Status::Stopped) {
                    task.reason = None;
                    task.enter(Status::Working, effects);
                }
                task.watch.rebase = true;
                Ok(Answer::Changed(true))
            }
            Command::Stop { task } => {
                let task = self.task_mut(&task)?;
                if !matches!(
                    task.status,
                    Status::Queued | Status::Working | Status::NeedsHuman
                ) {
                    return Ok(Answer::Changed(false));
                }
                task.note(now_ms, Actor::Operator, "Stopped the task");
                task.reason = Some("Stopped by you".into());
                if let Some(thread) = task.worker {
                    effects.push(Effect::Interrupt { thread });
                }
                task.enter(Status::Stopped, effects);
                Ok(Answer::Changed(true))
            }
            Command::Resume { task } => {
                let task = self.task_mut(&task)?;
                if task.status != Status::Stopped {
                    return Ok(Answer::Changed(false));
                }
                task.note(now_ms, Actor::Operator, "Resumed the task");
                task.reason = None;
                match task.worker {
                    Some(_) => {
                        task.enter(Status::Working, effects);
                        task.watch.prompt = Some(resume_prompt());
                    }
                    // It never had a Worker: the next step starts one.
                    None => task.enter(Status::Queued, effects),
                }
                Ok(Answer::Changed(true))
            }
            Command::Delete { task } => {
                let at = self
                    .tasks
                    .iter()
                    .position(|candidate| candidate.id == task)
                    .ok_or_else(unknown_task)?;
                let status = self.tasks[at].status;
                if !status.deletable() {
                    return Err(Refusal::new(
                        RefusalCode::NotNow,
                        format!(
                            "a Task that is {} cannot be deleted: stop it first",
                            status.word().to_lowercase()
                        ),
                    ));
                }
                let task = self.tasks.remove(at);
                if let Some(thread) = task.worker {
                    effects.push(Effect::Park { thread });
                }
                if let Some(path) = task.worktree {
                    effects.push(Effect::RemoveWorktree { path });
                }
                Ok(Answer::Changed(true))
            }
        }
    }

    fn step(&mut self, now_ms: i64, workers: &BTreeMap<ThreadId, Seen>, effects: &mut Vec<Effect>) {
        for task in &mut self.tasks {
            match task.status {
                Status::Queued => {
                    task.note(
                        now_ms,
                        Actor::Ferrite,
                        format!("Started an agent on {}", agent_label(&task.agent)),
                    );
                    effects.push(Effect::StartWorker {
                        task: task.id.clone(),
                        project: task.project,
                        agent: task.agent.clone(),
                        branch: task.branch.clone(),
                        prompt: first_prompt(task),
                    });
                    task.enter(Status::Working, effects);
                }
                Status::Working => {
                    let Some(seen) = task.worker.and_then(|thread| workers.get(&thread)) else {
                        continue;
                    };
                    at_rest_rule(task, *seen, now_ms, effects);
                }
                _ => {}
            }
        }
    }
}

/// A Worker is at rest when its turn has ended and nothing of its Session
/// still runs. At rest without a report once the turn it was prompted
/// for has ended, it is asked once to report; at rest again after that
/// turn (or still, after a grace), its Task needs the operator. A busy
/// Worker is never asked.
fn at_rest_rule(task: &mut Task, seen: Seen, now_ms: i64, effects: &mut Vec<Effect>) {
    let Some(thread) = task.worker else {
        return;
    };
    if let Some(text) = task.watch.prompt.take() {
        effects.push(Effect::Prompt { thread, text });
        task.watch.baseline = seen.turns;
        task.watch.nudged_ms = None;
        return;
    }
    if std::mem::take(&mut task.watch.rebase) {
        task.watch.baseline = seen.turns;
        return;
    }
    if seen.busy {
        return;
    }
    let ended = seen.turns > task.watch.baseline;
    match task.watch.nudged_ms {
        None if ended => {
            task.note(
                now_ms,
                Actor::Ferrite,
                "The agent stopped without reporting; asked it to report",
            );
            effects.push(Effect::Prompt {
                thread,
                text: nudge_prompt(),
            });
            task.watch.baseline = seen.turns;
            task.watch.nudged_ms = Some(now_ms);
        }
        Some(nudged) if ended || now_ms - nudged >= NUDGE_GRACE_MS => {
            task.note(
                now_ms,
                Actor::Ferrite,
                "The agent stopped again without reporting",
            );
            task.reason = Some("The agent stopped without reporting an outcome.".into());
            task.enter(Status::NeedsHuman, effects);
        }
        _ => {}
    }
}

/// `Claude · claude-opus-4`, as a card names a Worker's.
pub fn agent_label(agent: &Agent) -> String {
    let provider = match agent.provider {
        Provider::Claude => "Claude",
        Provider::Codex => "Codex",
    };
    match &agent.model {
        Some(model) => format!("{provider} · {model}"),
        None => provider.to_string(),
    }
}

/// How a Worker reports: the `ferrite` its Session was given.
const REPORT: &str = "\"${FERRITE_BIN:-ferrite}\" report outcome";

fn first_prompt(task: &Task) -> String {
    format!(
        "You are working on Ferrite Factory task {id}, in the task's own worktree on branch \
         `{branch}`.\n\n{instructions}\n\nWhen you are done, report the outcome with one \
         command:\n\n    {REPORT} finished \"<one line: what you did>\"\n\nIf you cannot \
         finish, report what you tried:\n\n    {REPORT} unable \"<what you tried and what \
         stopped you>\"\n\nThe outcome is read only from that command, never from your \
         messages.",
        id = task.id,
        branch = task.branch,
        instructions = task.instructions,
    )
}

fn nudge_prompt() -> String {
    format!(
        "Your turn ended without reporting the task's outcome. Report it now with \
         `{REPORT} finished \"<what you did>\"`, or `{REPORT} unable \"<what you tried>\"`."
    )
}

fn resume_prompt() -> String {
    format!(
        "The operator resumed this task. Continue where you left off, and report the \
         outcome with `{REPORT} finished …` or `{REPORT} unable …` when you are done."
    )
}

#[cfg(test)]
mod tests;
