//! What the actions share: the stages an agent holds a slot in, and an action's answer.
use crate::store::files::SLOT_STAGES;
use crate::store::tasks::Task;

/// The stages in which an agent is on a task: queued, its attempts at work, or a slot held.
pub fn agent_stages() -> Vec<&'static str> { let mut v = vec!["queued", "attempts"]; v.extend_from_slice(SLOT_STAGES); v }
pub fn is_agent_stage(s: &str) -> bool { agent_stages().contains(&s) }
/// What an action answers: the task as it now is, and one line that starts with its id.
#[derive(Clone, Debug)]
pub struct Said { pub task: Task, pub msg: String }
pub fn say(task: Task, msg: impl Into<String>) -> Said { Said { task, msg: msg.into() } }
