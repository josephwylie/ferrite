//! What a Task holds of the operator's say (#222): a Worker's question
//! and its answer, feedback, what was said to its Workers, and one Task's
//! direction. How each comes and goes is the Factory's
//! (`factory::direct`).

use serde::{Deserialize, Serialize};

use super::{Actor, StepId};
use crate::ThreadId;

/// What a Task holds of the operator's say, and of who is waiting on it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Oversight {
    /// A Worker's question, waiting for the operator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<Question>,
    /// How many questions were asked: the next one's id.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub asked: u64,
    /// Every question answered, which later Steps are told.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<Answered>,
    /// Rework, rejection and redirect feedback, which every later Step is
    /// told.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feedback: Vec<Feedback>,
    /// What the operator said to the Task's Workers, which every later
    /// Step is told (story 21).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub said: Vec<Said>,
    /// The operator's direction for this Task only.
    #[serde(default, rename = "override", skip_serializing_if = "Option::is_none")]
    pub direction: Option<Override>,
    /// "Return it to me": the operator approves, whoever the Workflow
    /// names.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub to_me: bool,
    /// The current run decides an approval.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deciding: bool,
    /// A Provider Decision waiting in the Worker's Session, as it reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
    /// Workers whose run a redirect superseded: what they report is
    /// ignored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub superseded: Vec<ThreadId>,
    /// What the Coordinator is to be told of a Task routed to it (#224),
    /// until the next step tells it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consult: Option<String>,
}

fn is_zero(count: &u64) -> bool {
    *count == 0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub id: u64,
    pub text: String,
    /// Who asked: the Step's name, or "The agent" for a one-off.
    pub asker: String,
    pub worker: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answered {
    pub asker: String,
    pub question: String,
    pub answer: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackKind {
    /// The operator asked for rework.
    Rework,
    /// The deciding agent did not approve.
    Rejected,
    /// The operator's note with a redirect.
    Redirect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feedback {
    pub kind: FeedbackKind,
    pub by: Actor,
    /// Who it was sent to, as the operator reads it.
    pub to: String,
    pub text: String,
    /// How many results there were when it was given: a returning Worker
    /// is told what came after its own last one.
    pub after: usize,
}

/// Something the operator said to one of the Task's Workers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Said {
    /// The Step whose Worker it was said to, as the operator reads it.
    pub to: String,
    pub text: String,
    /// How many results there were when it was said, as for feedback.
    pub after: usize,
}

/// After a redirect, follow the Workflow's routes or return the Task to
/// the operator to approve.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ThenReturn {
    #[default]
    Routes,
    Me,
}

/// One Task's direction: never the saved Workflow's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Override {
    /// The Step it was sent to.
    pub to: StepId,
    /// Steps a route passes over, as if they had finished.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skip: Vec<StepId>,
    #[serde(default)]
    pub then: ThenReturn,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}
