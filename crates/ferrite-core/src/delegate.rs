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
use crate::store::Provider;
use crate::wake::{Outcome, Source, Wake};
use crate::ThreadId;

/// The most of its results a wake carries in full, in bytes (T3's
/// `DELEGATED_RESULT_INLINE_BYTES`). A result past it is left for
/// `task_status` to read.
pub const RESULT_INLINE_BYTES: usize = 16_000;

/// The longest label a wake's line gives a Delegate (T3's
/// `LABEL_MAX_LENGTH`).
const REPORT_LABEL_CHARS: usize = 80;

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
    /// The provider its Thread there runs on, and the model it was asked
    /// to (none: that provider's default there).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<Provider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
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
    /// When it was recorded and when it ended here, as stored stamps
    /// (`clock::now_stamp`): its row's clock (T3's `startedAt` and
    /// `completedAt`). A record from before them has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
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

/// What every window reads of a Thread's Delegates, as its digest carries
/// them (ADR 0047): each one, oldest first, and the Server's word on
/// whether its parent waits on them. Lineage draws the first, the Working
/// shelf and the Notice read the second.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delegating {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delegates: Vec<Delegated>,
    /// One is open, or one ended and its wake is owed
    /// ([`Delegates::holding`]).
    #[serde(default, skip_serializing_if = "is_false")]
    pub holds: bool,
}

/// The most of a Delegate's result its digest carries, in characters
/// (T3's `subagentDetailSummary` cut): its timeline row shows one line.
pub const RESULT_LINE_CHARS: usize = 280;

/// One Delegate, as a window draws it: no task, and its result only as far
/// as its row shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delegated {
    pub id: DelegateId,
    /// The peer's Server: the window finds the machine that answers as it.
    pub server: ServerId,
    /// The machine's saved name when it started.
    pub machine: String,
    /// Its Thread there.
    pub thread: ThreadId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<Provider>,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub stop_owed: bool,
    /// Its result as a read shows it, cut to [`RESULT_LINE_CHARS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
}

impl Delegating {
    pub fn of(delegates: &Delegates) -> Self {
        Self {
            delegates: delegates
                .iter()
                .map(|delegate| Delegated {
                    id: delegate.id,
                    server: delegate.server.clone(),
                    machine: delegate.machine.clone(),
                    thread: delegate.thread,
                    title: delegate.title.clone(),
                    provider: delegate.provider,
                    status: delegate.status,
                    problem: delegate.problem.clone(),
                    stop_owed: delegate.stop_owed,
                    result: delegate.result_text().map(|result| cut(&result, RESULT_LINE_CHARS, RESULT_LINE_CHARS)),
                    started_at: delegate.started_at.clone(),
                    ended_at: delegate.ended_at.clone(),
                })
                .collect(),
            holds: delegates.holding(),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl Delegated {
    /// What a stop still owed there says (T3's cancel result, without the
    /// result before it): `None` until a stop has failed to get there.
    pub fn unstopped(&self) -> Option<String> {
        let problem = self.problem.as_ref().filter(|_| self.stop_owed)?;
        Some(unstopped(&self.machine, problem))
    }
}

/// T3's words for a stop that has not reached the Delegate's Thread.
fn unstopped(machine: &str, problem: &str) -> String {
    format!("It could not be stopped on {machine} and may still run there: {problem}")
}

/// The Delegate a `delegate_task` call started, by the `taskId` its answer
/// names (T3 reads its child card's task from the same answer): none for
/// another call, or one that has not answered or failed.
pub fn started_by(tool: &crate::transcript::ToolBlock) -> Option<DelegateId> {
    if !crate::transcript::tools::starts_delegate(&tool.name) || tool.state != crate::transcript::ToolState::Ok {
        return None;
    }
    // The answer is T3's task as JSON, quoted again when a provider hands
    // it back inside its own (`\"taskId\":\"d1\"`).
    let answer = &tool.output.as_ref()?.text;
    let after = &answer[answer.find("taskId")? + "taskId".len()..];
    let id: String = after
        .trim_start_matches(['\\', '"', ':', ' '])
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect();
    id.parse().ok()
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
    pub provider: Provider,
    pub model: Option<String>,
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
            provider: Some(started.provider),
            model: started.model,
            status: Status::Starting,
            result: None,
            problem: None,
            stop_owed: false,
            told: false,
            started_at: Some(crate::clock::now_stamp()),
            ended_at: None,
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
        delegate.ended_at = Some(crate::clock::now_stamp());
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
                delegate.ended_at = Some(crate::clock::now_stamp());
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
                delegate.ended_at = Some(crate::clock::now_stamp());
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
        // T3's `delegatedCompletionNotification`: one verb for them all,
        // failed over stopped over finished, and each one's label.
        let verb = if ended.iter().any(|delegate| delegate.status == Status::Failed) {
            "failed"
        } else if ended.iter().any(|delegate| delegate.status != Status::Completed) {
            "stopped"
        } else {
            "finished"
        };
        let summary = match ended.as_slice() {
            [] => return None,
            [one] => format!("Delegated task \"{}\" {verb}", one.report_label()),
            many => format!(
                "{} delegated tasks {verb}: {}",
                many.len(),
                many.iter().map(|delegate| delegate.report_label()).collect::<Vec<_>>().join(", ")
            ),
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
            source: Source::Delegates,
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

    /// What a wake's line names it by (T3's `reportLabel`): the first line
    /// of its title, else of its task, cut to 80 characters.
    fn report_label(&self) -> String {
        let named = self.title.as_deref().map(str::trim).filter(|title| !title.is_empty());
        let line = named.unwrap_or(&self.task).trim().lines().next().unwrap_or_default().trim();
        cut(line, REPORT_LABEL_CHARS - 1, REPORT_LABEL_CHARS)
    }

    /// Its result as a read shows it: a stop still owed says it may still
    /// run there, and why (T3's cancel result).
    pub fn result_text(&self) -> Option<String> {
        let result = self.result.as_ref()?;
        Some(match (self.stop_owed, &self.problem) {
            (true, Some(problem)) => format!("{result} {}", unstopped(&self.machine, problem)),
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
            "provider": self.provider,
            "model": self.model,
            "status": self.status,
            "result": self.result_text(),
            "problem": self.problem,
            "stop_owed": self.stop_owed,
        })
    }
}

/// `text` as it is up to `limit` characters, else its first `keep` and
/// `…` (T3's cut labels).
fn cut(text: &str, keep: usize, limit: usize) -> String {
    match (text.char_indices().nth(limit), text.char_indices().nth(keep)) {
        (Some(_), Some((end, _))) => format!("{}\u{2026}", &text[..end]),
        _ => text.to_owned(),
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests;
