//! A Delegate (ADR 0047): a Thread one Thread's agent started on another
//! linked machine to do one piece of work. This Server follows it until it
//! ends, then wakes the starting Thread's agent with its result. It is T3
//! Code's remote delegated task (`apps/server/src/peer/RemoteDelegation.ts`
//! and `delegatedCompletionWakeDetail` in `orchestration-v2/Orchestrator.ts`
//! at 0fd48228), with its statuses and its words.
//!
//! The record lives on the parent Thread's header ([`Delegates`]), restated
//! whole on each change, so a restarted Server follows each open one again
//! from the record alone. Nothing here does I/O: the Server's follower asks
//! the peer, and says here what it heard.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{json, Value};

use crate::server::ServerId;
use crate::wake::{Outcome, Wake};
use crate::ThreadId;

/// The most of its results a wake carries in full, in bytes (T3's
/// `DELEGATED_RESULT_INLINE_BYTES`). A result past it is left for
/// `task_status` to read.
pub const RESULT_INLINE_BYTES: usize = 16_000;

/// What a start tells the agent to do next (T3's running-task `next`).
pub const NEXT: &str =
    "Completion wakes this thread. Do not wait on it or poll it. Continue other work, and end your turn when none is left.";

/// A Thread's Delegates, in the order they started: open and ended alike,
/// so an id is never given twice and a repeated launch finds its own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Delegates(Vec<Delegate>);

/// One Delegate, as its parent's Server knows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delegate {
    pub id: DelegateId,
    /// The peer's Server. Each call checks the machine answers as it, so a
    /// machine pointed elsewhere ends the Delegate instead of following
    /// another Server's Thread of the same number (G-A3).
    pub server: ServerId,
    /// The machine's saved name here when it started, for words.
    pub machine: String,
    /// Its Thread there, by that Server's id for it.
    pub thread: ThreadId,
    /// The launch key there, `delegate:<server>:<parent>:<key>` (G-A9).
    pub request: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub task: String,
    pub status: Status,
    /// Once ended: its final reply, a failed turn's error, or why it ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// Why following it, or stopping it there, is held up: the machine
    /// asleep, a sign-in, in the failure's own words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// A stop there is owed: one was asked for (a cancel, its parent
    /// stopped or parked) and has not reached it yet. Kept across restarts
    /// until one does (T3's `stoppedThere: false`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub stop_owed: bool,
    /// Its ending reached its parent's agent: a wake carried it, the agent
    /// read it or cancelled it, or its parent stopped. An ended Delegate
    /// not told holds a wake.
    #[serde(default, skip_serializing_if = "is_false")]
    pub told: bool,
}

/// T3's delegated task statuses, less `queued`: a Delegate is recorded
/// once its Thread there exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Its Thread there exists, and has not been looked at since.
    Starting,
    Running,
    /// Its Thread there waits on a Decision.
    Waiting,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl Status {
    pub fn ended(self) -> bool {
        matches!(
            self,
            Status::Completed | Status::Failed | Status::Cancelled | Status::Interrupted
        )
    }

    pub fn word(self) -> &'static str {
        match self {
            Status::Starting => "starting",
            Status::Running => "running",
            Status::Waiting => "waiting",
            Status::Completed => "completed",
            Status::Failed => "failed",
            Status::Cancelled => "cancelled",
            Status::Interrupted => "interrupted",
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// A Delegate's name on its parent: `d1`, `d2`, never given twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DelegateId(u32);

impl DelegateId {
    pub const fn new(n: u32) -> Self {
        Self(n)
    }
}

impl fmt::Display for DelegateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "d{}", self.0)
    }
}

impl FromStr for DelegateId {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.strip_prefix('d')
            .and_then(|n| n.parse().ok())
            .map(Self)
            .ok_or_else(|| format!("`{text}` is not a Delegate id, which reads `d1`"))
    }
}

/// A string `d<n>`, as the API takes it.
impl schemars::JsonSchema for DelegateId {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DelegateId".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^d[0-9]+$",
            "examples": ["d1"],
        })
    }
}

impl Serialize for DelegateId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for DelegateId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// What a launch there answered, for the record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    pub server: ServerId,
    pub machine: String,
    pub thread: ThreadId,
    pub request: String,
    pub title: Option<String>,
    pub task: String,
}

/// What a cancel did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cancel {
    /// It was open: it is cancelled here, and its stop there is owed (T3's
    /// `cancel_requested`).
    Requested,
    /// It had ended: its result stands, and it is stopped there once more
    /// at best.
    Ended(Status),
}

impl Delegates {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Delegate> {
        self.0.iter()
    }

    pub fn get(&self, id: DelegateId) -> Option<&Delegate> {
        self.0.iter().find(|delegate| delegate.id == id)
    }

    /// The Delegate a launch with `request` recorded.
    pub fn by_request(&self, request: &str) -> Option<&Delegate> {
        self.0.iter().find(|delegate| delegate.request == request)
    }

    pub fn open(&self) -> impl Iterator<Item = &Delegate> {
        self.0.iter().filter(|delegate| !delegate.status.ended())
    }

    /// Whether its parent still waits on one: one is open, or one ended
    /// and its wake is owed. While it does the parent counts as working,
    /// and its turn as not ended (T3's `untilDelegatedWorkSettled`).
    pub fn holding(&self) -> bool {
        self.0
            .iter()
            .any(|delegate| !delegate.status.ended() || !delegate.told)
    }

    /// The ended ones whose wake is owed, oldest first.
    pub fn untold(&self) -> Vec<DelegateId> {
        self.0
            .iter()
            .filter(|delegate| delegate.status.ended() && !delegate.told)
            .map(|delegate| delegate.id)
            .collect()
    }

    /// The ones a stop is owed there.
    pub fn owed_stops(&self) -> impl Iterator<Item = &Delegate> {
        self.0.iter().filter(|delegate| delegate.stop_owed)
    }

    /// Record a launch, under the next id.
    pub fn start(&mut self, started: Started) -> &Delegate {
        let id = DelegateId(self.0.iter().map(|delegate| delegate.id.0).max().unwrap_or(0) + 1);
        self.0.push(Delegate {
            id,
            server: started.server,
            machine: started.machine,
            thread: started.thread,
            request: started.request,
            title: started.title,
            task: started.task,
            status: Status::Starting,
            result: None,
            problem: None,
            stop_owed: false,
            told: false,
        });
        self.0.last().expect("pushed")
    }

    /// What following an open one saw there: its status while it runs,
    /// and why following is held up. Answers whether anything changed.
    pub fn seen(&mut self, id: DelegateId, status: Status, problem: Option<String>) -> bool {
        debug_assert!(!status.ended(), "an ending is `end`'s");
        let Some(delegate) = self.open_mut(id) else {
            return false;
        };
        let changed = delegate.status != status || delegate.problem != problem;
        delegate.status = status;
        delegate.problem = problem;
        changed
    }

    /// End an open one with its result. One that ended here first, by a
    /// cancel or its parent's stop, keeps its own. Answers whether it
    /// ended now.
    pub fn end(&mut self, id: DelegateId, status: Status, result: String) -> bool {
        debug_assert!(status.ended(), "an end is ended");
        let Some(delegate) = self.open_mut(id) else {
            return false;
        };
        delegate.status = status;
        delegate.result = Some(result);
        delegate.problem = None;
        true
    }

    /// Cancel one as its parent's agent asks (T3's `task_cancel`): an open
    /// one ends cancelled, with `reason`; an ended one keeps its result.
    /// Either way the agent asked, so no wake is owed, and a stop there is.
    pub fn cancel(&mut self, id: DelegateId, reason: Option<&str>) -> Option<Cancel> {
        let delegate = self.0.iter_mut().find(|delegate| delegate.id == id)?;
        let done = match delegate.status.ended() {
            true => Cancel::Ended(delegate.status),
            false => {
                delegate.status = Status::Cancelled;
                delegate.result = Some(match reason {
                    None => "Cancelled.".into(),
                    Some(reason) => format!("Cancelled: {reason}"),
                });
                delegate.problem = None;
                Cancel::Requested
            }
        };
        delegate.stop_owed = true;
        delegate.told = true;
        Some(done)
    }

    /// Its parent stopped or was parked (T3's `stopDelegatedTasks`): each
    /// open one ends interrupted, and each one, ended or not, is stopped
    /// there, since its Thread there may have run on. No wake is owed for
    /// any. Answers whether there were any.
    pub fn stop_with_parent(&mut self) -> bool {
        for delegate in &mut self.0 {
            if !delegate.status.ended() {
                delegate.status = Status::Interrupted;
                delegate.result = Some("Stopped with its parent.".into());
                delegate.problem = None;
            }
            delegate.stop_owed = true;
            delegate.told = true;
        }
        !self.0.is_empty()
    }

    /// A stop reached it there, or nothing is left there to stop.
    pub fn stopped(&mut self, id: DelegateId) -> bool {
        match self.0.iter_mut().find(|delegate| delegate.id == id) {
            Some(delegate) if delegate.stop_owed => {
                delegate.stop_owed = false;
                delegate.problem = None;
                true
            }
            _ => false,
        }
    }

    /// A stop owed there did not get there, for `problem`.
    pub fn unstopped(&mut self, id: DelegateId, problem: String) -> bool {
        match self.0.iter_mut().find(|delegate| delegate.id == id) {
            Some(delegate) if delegate.stop_owed && delegate.problem.as_ref() != Some(&problem) => {
                delegate.problem = Some(problem);
                true
            }
            _ => false,
        }
    }

    /// Its ending reached the parent's agent. Answers whether that was news.
    pub fn tell(&mut self, ids: &[DelegateId]) -> bool {
        let mut changed = false;
        for delegate in &mut self.0 {
            if ids.contains(&delegate.id) && delegate.status.ended() && !delegate.told {
                delegate.told = true;
                changed = true;
            }
        }
        changed
    }

    /// The wake for `ids`' endings, in T3's words: each one's label and
    /// result, the results in full while they fit [`RESULT_INLINE_BYTES`]
    /// between them, and a pointer to `task_status` for the rest. `None`
    /// when none of them has ended.
    pub fn wake(&self, ids: &[DelegateId]) -> Option<Wake> {
        let ended: Vec<&Delegate> = ids
            .iter()
            .filter_map(|id| self.get(*id))
            .filter(|delegate| delegate.status.ended())
            .collect();
        let mut left = RESULT_INLINE_BYTES;
        let text = ended
            .iter()
            .map(|delegate| {
                let label = delegate.label();
                if let Some(result) = &delegate.result {
                    let detail = format!("{label}:\n{result}");
                    if detail.len() <= left {
                        left -= detail.len();
                        return detail;
                    }
                }
                format!(
                    "{label}. Use task_status with taskId {} to read the result.",
                    delegate.id
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let summary = match ended.as_slice() {
            [] => return None,
            [one] => format!("Delegated task \"{}\" {}", one.name(), one.status),
            many => format!("{} delegated tasks ended", many.len()),
        };
        let outcome = if ended.iter().any(|delegate| delegate.status == Status::Failed) {
            Outcome::Failed
        } else if ended.iter().all(|delegate| delegate.status == Status::Completed) {
            Outcome::Completed
        } else {
            Outcome::Updated
        };
        Some(Wake {
            text,
            outcome,
            summary,
        })
    }

    fn open_mut(&mut self, id: DelegateId) -> Option<&mut Delegate> {
        self.0
            .iter_mut()
            .find(|delegate| delegate.id == id && !delegate.status.ended())
    }
}

impl Delegate {
    /// `Delegated task d1 (Fix the tests) [completed]`, as T3 labels a task
    /// in a wake.
    fn label(&self) -> String {
        let title = self
            .title
            .as_ref()
            .map(|title| format!(" ({title})"))
            .unwrap_or_default();
        format!("Delegated task {}{title} [{}]", self.id, self.status)
    }

    /// Its title, else its id.
    fn name(&self) -> String {
        self.title.clone().unwrap_or_else(|| self.id.to_string())
    }

    /// Its result as a read shows it: a stop still owed says it may still
    /// run there, and why (T3's cancel result).
    pub fn result_text(&self) -> Option<String> {
        let result = self.result.as_ref()?;
        Some(match (self.stop_owed, &self.problem) {
            (true, Some(problem)) => format!(
                "{result} It could not be stopped on {} and may still run there: {problem}",
                self.machine
            ),
            _ => result.clone(),
        })
    }

    /// The API's answer for it, on `parent`.
    pub fn info(&self, parent: ThreadId) -> Value {
        json!({
            "delegate": self.id,
            "parent": parent,
            "machine": self.machine,
            "server": self.server,
            "thread": self.thread,
            "title": self.title,
            "status": self.status,
            "result": self.result_text(),
            "problem": self.problem,
            "stop_owed": self.stop_owed,
        })
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests;
