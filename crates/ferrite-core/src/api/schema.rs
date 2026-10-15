//! Every API operation, declared once (after Herdr's `define_operations!`):
//! its wire name, its parameters, which of them the CLI takes in order,
//! and the line `ferrite help` says. The wire enum, the catalog the CLI
//! parses by, and the JSON Schema (`ferrite --schema`) all come from that
//! one declaration. Names are append-only, as the client contract's are:
//! never renamed or removed; a new parameter is optional.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::protocol::WireAnswer;
use crate::store::Provider;
use crate::{Attention, ThreadId};

/// One operation as the CLI and `ferrite help` know it.
pub struct Operation {
    /// The wire name, `group.verb`; the CLI spells it `ferrite group verb`.
    pub name: &'static str,
    /// The parameters the CLI takes without a flag, in order.
    pub positional: &'static [&'static str],
    pub summary: &'static str,
    /// The JSON Schema of its parameters.
    pub params: fn() -> schemars::Schema,
}

impl Operation {
    /// `thread.start` → (`thread`, `start`).
    pub fn words(&self) -> (&'static str, &'static str) {
        self.name
            .split_once('.')
            .expect("an operation is group.verb")
    }
}

macro_rules! operations {
    ($( $variant:ident($params:ty) => $name:literal, [$($positional:literal),*], $summary:literal; )*) => {
        /// What a request asks for, and with what.
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
        #[serde(tag = "method", content = "params")]
        pub enum Method {
            $(#[doc = $summary] #[serde(rename = $name)] $variant($params),)*
        }

        impl Method {
            pub const OPERATIONS: &'static [Operation] = &[
                $(Operation {
                    name: $name,
                    positional: &[$($positional),*],
                    summary: $summary,
                    params: || schemars::schema_for!($params),
                },)*
            ];

            pub fn name(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $name,)*
                }
            }
        }
    };
}

operations! {
    ThreadStart(ThreadStartParams) => "thread.start", [],
        "Start a Thread: a new agent Session working in a directory.";
    ThreadList(Empty) => "thread.list", [],
        "List the open Threads and their Attention.";
    ThreadGet(ThreadParams) => "thread.get", ["thread"],
        "One open Thread and its Attention.";
    ThreadFocus(ThreadParams) => "thread.focus", ["thread"],
        "Land every Ferrite window on a Thread.";
    ThreadRename(ThreadRenameParams) => "thread.rename", ["thread", "title"],
        "Give a Thread a title.";
    AgentPrompt(AgentPromptParams) => "agent.prompt", ["thread", "text"],
        "Send a Thread's agent a prompt; with wait, until it settles.";
    AgentWait(AgentWaitParams) => "agent.wait", ["thread"],
        "Wait until a Thread's agent settles, or reaches one Attention.";
    AgentRead(AgentReadParams) => "agent.read", ["thread"],
        "The tail of a Thread's transcript, as plain text.";
    DecisionList(DecisionListParams) => "decision.list", [],
        "The pending Decisions, of every Thread or of one.";
    DecisionAnswer(DecisionAnswerParams) => "decision.answer", ["thread", "decision"],
        "Answer a pending Decision. Only on the operator's word.";
    EventsSubscribe(EventsSubscribeParams) => "events.subscribe", [],
        "What the Threads are now, then every change as it happens.";
    NotificationShow(NotificationShowParams) => "notification.show", ["title"],
        "Show a toast in every Ferrite window.";
    CoordinatorStart(CoordinatorStartParams) => "coordinator.start", [],
        "Start the coordinator of the Project in a directory, or answer the one it has; either way the brief given is recorded.";
    CoordinatorGet(ThreadParams) => "coordinator.get", ["thread"],
        "A coordinator's brief and its workers, what needs the operator first.";
    CoordinatorUpdate(CoordinatorUpdateParams) => "coordinator.update", ["thread"],
        "Change a coordinator's goal, standing instructions or PR follow-up.";
    WorkerStart(WorkerStartParams) => "worker.start", ["thread", "task"],
        "Start a coordinator's worker on one task in a new worktree. Only on the operator's go-ahead.";
}

/// No parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Empty {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThreadStartParams {
    /// The directory the agent works in. The CLI sends its own when none
    /// is given.
    pub cwd: PathBuf,
    /// The operator's default Provider when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<Provider>,
    /// Work in a fresh worktree of `cwd`'s repository instead.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub worktree: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThreadParams {
    pub thread: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThreadRenameParams {
    pub thread: ThreadId,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentPromptParams {
    pub thread: ThreadId,
    pub text: String,
    /// Answer once the agent was seen working and then settled (idle,
    /// done or blocked), not as soon as the prompt is sent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wait: bool,
    /// Give up `timeout` after this long, counted from the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentWaitParams {
    pub thread: ThreadId,
    /// The Attention to wait for; settled (idle, done or blocked) when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<Attention>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentReadParams {
    pub thread: ThreadId,
    /// How many of the last lines; 80 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DecisionListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<ThreadId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DecisionAnswerParams {
    pub thread: ThreadId,
    /// The Decision's `id`, as `decision.list` gives it.
    pub decision: String,
    pub answer: WireAnswer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EventsSubscribeParams {
    /// Only these Threads' events; every Thread's when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub threads: Vec<ThreadId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NotificationShowParams {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CoordinatorStartParams {
    /// A directory of the Project. The CLI sends its own when none is
    /// given.
    pub cwd: PathBuf,
    /// The operator's default Provider when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<Provider>,
    /// What the Project is for: every worker is briefed with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    /// What every worker must always do or never do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Send failing checks and review remarks on a worker's PR back to it.
    /// Absent leaves a coordinator the Project has as it is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub follow_prs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CoordinatorUpdateParams {
    /// The coordinator.
    pub thread: ThreadId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_prs: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkerStartParams {
    /// The coordinator the worker reports to.
    pub thread: ThreadId,
    /// The work, complete on its own: the worker knows only its brief.
    pub task: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The operator's default Provider when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<Provider>,
}
