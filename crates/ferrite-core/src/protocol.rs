//! The client contract, generation 1 (ADR 0025): what a window and its
//! Server say to each other, one JSON object per line. No I/O here.
//!
//! The rules, frozen with generation 1:
//!
//! - Fields are never removed, renamed or retyped. New fields are optional
//!   (`#[serde(default)]`); unknown fields are ignored — nothing in this
//!   module denies them.
//! - Every enum that crosses the wire has an `Unknown` fallback. An unknown
//!   message is ignored; an unknown command is answered `unsupported`.
//! - Optional behaviour is negotiated by name in `capabilities`.
//! - A change these rules cannot carry is generation 2, offered beside 1.
//!
//! Durable history crosses in the Store's own schema ([`LogRecord`]): the
//! log's lines, not a second encoding of them. What the log never keeps —
//! Decisions, menus, connection state — crosses as the small [`Live`]
//! vocabulary. Fixtures for every message are in `protocol/fixtures/gen1`
//! and are append-only.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::activity::{ActivityEvent, ActivityInput, AgentKey, DecisionHandle, Subject};
use crate::groups::{GroupChange, GroupId};
use crate::store::{LogRecord, Provider};
use crate::transcript::Input;
use crate::workspace::registry::ProjectId;
use crate::workspace::{WorkspaceBinding, WorkspaceChoice};
use crate::{SessionEvent, ThreadId};
use ferrite_reviewr::ferrite::Read;
use ferrite_reviewr::model::CommitPick;

mod review;
pub use review::*;

/// The only generation this build speaks.
pub const GENERATION: u32 = 1;

// ------------------------------------------------------- client → server

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// The first line of every connection, and nothing before it.
    Hello {
        generation: u32,
        client: Peer,
        #[serde(default)]
        capabilities: Vec<Capability>,
    },
    /// Stream this Thread's inputs: from `resume` when this boot still holds
    /// it, otherwise from its history.
    Watch {
        thread: ThreadId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resume: Option<Resume>,
    },
    Unwatch {
        thread: ThreadId,
    },
    Command(Command),
    /// The answer to a `render`.
    Rendered {
        request: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        png: Option<WirePng>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

/// Who is speaking: a name and its version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// This client can draw Visuals for an agent.
    RenderVisual,
    /// This client lists the Server's parked Threads off its `parked`
    /// message: a window on a machine, whose logs stay there (ADR 0026).
    ParkedThreads,
    #[serde(other)]
    Unknown,
}

/// Where a reconnecting watcher left off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resume {
    pub boot_id: String,
    /// The last `seq` the client applied.
    pub after: u64,
}

/// A rendered Visual: base64 PNG and the size it was taken at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePng {
    pub base64: String,
    pub width: u32,
    pub height: u32,
}

/// One act that changes shared or durable state. Exactly one `reply`
/// answers each.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub id: u64,
    #[serde(flatten)]
    pub op: Op,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Open {
        provider: Provider,
        workspace: WireChoice,
    },
    /// A Draft's first prompt: the Thread is created by sending it.
    Bootstrap {
        provider: Provider,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        effort: Option<String>,
        workspace: WireChoice,
        prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group: Option<GroupId>,
        /// The loose Thread the new one makes a Group with once it starts.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_group_with: Option<ThreadId>,
    },
    /// Take back a `bootstrap` still starting: no Thread is left of it,
    /// and that command is answered by this one's reply.
    CancelBootstrap {
        request: u64,
    },
    Send {
        thread: ThreadId,
        text: String,
    },
    Queue {
        thread: ThreadId,
        text: String,
    },
    Unqueue {
        thread: ThreadId,
    },
    CancelQueued {
        thread: ThreadId,
        restore: bool,
        /// Up's edit: the prompt goes back ahead of the Composer's draft.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        edit: bool,
    },
    /// Hand a prompt the provider gave back to the Composer
    /// (`ThreadState::recovered`): it leaves the Thread.
    TakeRecovered {
        thread: ThreadId,
    },
    /// Up or Down through the Thread's sent prompts.
    Recall {
        thread: ThreadId,
        direction: WireDirection,
        draft: String,
    },
    Interrupt {
        thread: ThreadId,
    },
    /// Names the handle the client drew: a request id is unique only
    /// within one provider process.
    Answer {
        thread: ThreadId,
        handle: WireHandle,
        answer: WireAnswer,
    },
    Control {
        thread: ThreadId,
        control: WireControl,
    },
    SetModel {
        thread: ThreadId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    SetEffort {
        thread: ThreadId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        effort: Option<String>,
    },
    SetProvider {
        thread: ThreadId,
        provider: Provider,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    Rename {
        thread: ThreadId,
        title: String,
    },
    Park {
        thread: ThreadId,
    },
    Revive {
        thread: ThreadId,
    },
    /// Every member of a Group opened.
    ReviveGroup {
        group: GroupId,
    },
    Delete {
        thread: ThreadId,
    },
    Import {
        path: PathBuf,
    },
    Group {
        change: WireGroupChange,
    },
    /// A Group's arrangement; none puts it back on the default grid.
    Layout {
        group: GroupId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tree: Option<crate::layout::Tree>,
    },
    Project {
        action: ProjectAction,
    },
    Notice {
        action: NoticeAction,
    },
    /// Settings changed on disk: reread them for the next Session.
    SettingsChanged,
    /// A provider's CLI was replaced on disk: its Sessions restart on the
    /// new release as they idle, and its models are asked for again.
    CliReplaced {
        provider: Provider,
    },
    /// Git's worktree list for a Thread's repo, read `age_ms` before it was
    /// sent: a binding follows its agent into a worktree it made.
    WorktreesListed {
        thread: ThreadId,
        listed: Vec<PathBuf>,
        age_ms: u64,
    },
    /// Read an evicted Subagent's history back from the log; `retry`
    /// forgets a failed read first.
    LoadSubject {
        thread: ThreadId,
        subject: WireSubject,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        retry: bool,
    },
    /// The Subject a window's Pane shows: its Decisions are the ones read
    /// when the window is looked at.
    ShowSubject {
        thread: ThreadId,
        subject: WireSubject,
    },
    /// A line Ferrite itself says in a Thread's Pane.
    Say {
        thread: ThreadId,
        text: String,
    },
    SearchFiles {
        thread: ThreadId,
        query: String,
    },
    /// The Thread this window's focused Pane shows while the window has the
    /// operator's focus; none once it lost it (a Thread's `focused` in the
    /// API, ADR 0030). Only a Server hears it: it names the window.
    Focused {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread: Option<ThreadId>,
    },
    /// Whether this window's Composer for a Thread holds text not yet sent
    /// (a Thread's `draft` in the API). Only a Server hears it.
    Composing {
        thread: ThreadId,
        unsent: bool,
    },
    /// A directory's entries on the Server's computer, `~` its home: how a
    /// window on a machine browses the machine's folders (ADR 0026).
    /// Replied `listing`.
    ListDir {
        path: PathBuf,
    },
    /// What a Project's repo holds for a Draft's workspace chip, read by
    /// git on the Server's computer: how a window on a machine offers the
    /// machine's worktrees and branches (ADR 0026). Replied `repo_listing`.
    ListRepo {
        project: ProjectId,
    },
    /// A review's git scope of the checkout at `path`, read by git on the
    /// Server's computer (ADR 0021). Replied `diff`: what a window from
    /// before `review_read` reviews with.
    ReviewDiff {
        path: PathBuf,
        scope: WireScope,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<String>,
    },
    /// A review's scope of the checkout at `path`, read by the review core
    /// on the Server's computer: how a window reviews a Thread's checkout,
    /// on this computer or a machine (ADR 0021). Replied `review`.
    ReviewRead {
        path: PathBuf,
        scope: WireScope,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<String>,
        /// With `commit`, the oldest commit of a run of them: the scope is
        /// the run's changes together. Absent for one commit.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        since: Option<String>,
    },
    /// The branch's commits since its base at `path`, for a review's
    /// commits scope (ADR 0021). Replied `commits`.
    ListCommits {
        path: PathBuf,
    },
    /// Files and lines matching `query` in the checkout at `path`, from
    /// the Server's search index of it (ADR 0021). Replied `found`.
    SearchRepo {
        path: PathBuf,
        query: String,
    },
    /// The worktree file `file` (relative) of the checkout at `path`, for
    /// a review's file view (ADR 0021). Replied `text`.
    ReadFile {
        path: PathBuf,
        file: String,
    },
    /// The pull request of the branch checked out at `path`, whole, as its
    /// forge's CLI reads it for a review (ADR 0021). Replied
    /// `review_pull_request`.
    ReviewPullRequest {
        path: PathBuf,
    },
    /// Every file of the checkout at `path`, for a review's all-files
    /// list (ADR 0021). Replied `all_files`.
    ListFiles {
        path: PathBuf,
    },
    /// The branches a review's branch scope can read the checkout at
    /// `path` against, and the base it reads against now (ADR 0021).
    /// Replied `bases`.
    ListBases {
        path: PathBuf,
    },
    /// Make `base` (a branch, or any revision git resolves) the base the
    /// branch scope reads the checkout at `path` against: reviewr's
    /// per-worktree pick (ADR 0021). Replied `changed`.
    PickBase {
        path: PathBuf,
        base: String,
    },
    /// The PR of the branch checked out at `path`, read by `gh` on the
    /// Server's computer, where the checkout is: what a Thread row's PR
    /// label says. Replied `pull_request`.
    PullRequest {
        path: PathBuf,
    },
    Restart {
        when: When,
    },
    Stop {
        when: When,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireDirection {
    Older,
    Newer,
    #[serde(other)]
    Unknown,
}

/// What a `review_read` (or `review_diff`) reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireScope {
    Uncommitted,
    Branch,
    /// The command's `commit`.
    Commit,
    /// Since the checkout's snapshot when its latest turn began.
    LastTurn,
    #[serde(other)]
    Unknown,
}

impl Op {
    pub fn review_read(path: &Path, read: &Read) -> Op {
        let (scope, commit, since) = match read {
            Read::Uncommitted => (WireScope::Uncommitted, None, None),
            Read::Branch => (WireScope::Branch, None, None),
            Read::LastTurn => (WireScope::LastTurn, None, None),
            Read::Commits(pick) => (
                WireScope::Commit,
                Some(pick.newest.clone()),
                (!pick.is_single()).then(|| pick.oldest.clone()),
            ),
        };
        Op::ReviewRead {
            path: path.to_path_buf(),
            scope,
            commit,
            since,
        }
    }

    /// The `review_diff` a Server from before `review_read` reads `read`
    /// with, unless it cannot: a last turn, or a run of commits.
    pub fn review_diff(path: &Path, read: &Read) -> Option<Op> {
        let (scope, commit) = match read {
            Read::Uncommitted => (WireScope::Uncommitted, None),
            Read::Branch => (WireScope::Branch, None),
            Read::Commits(pick) if pick.is_single() => {
                (WireScope::Commit, Some(pick.newest.clone()))
            }
            Read::LastTurn | Read::Commits(_) => return None,
        };
        Some(Op::ReviewDiff {
            path: path.to_path_buf(),
            scope,
            commit,
        })
    }
}

impl WireScope {
    /// The read it names, unless this Server does not know it (or a
    /// commit scope names no commit id: never an option or a revision
    /// range). `since`, the oldest of a run of commits, must be an id too.
    pub fn read(self, commit: Option<String>, since: Option<String>) -> Option<Read> {
        let is_id = |id: &str| !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit());
        match (self, commit) {
            (WireScope::Uncommitted, _) => Some(Read::Uncommitted),
            (WireScope::Branch, _) => Some(Read::Branch),
            (WireScope::LastTurn, _) => Some(Read::LastTurn),
            (WireScope::Commit, Some(newest)) if is_id(&newest) => match since {
                None => Some(Read::Commits(CommitPick::single(&newest))),
                Some(oldest) if is_id(&oldest) => {
                    Some(Read::Commits(CommitPick { oldest, newest }))
                }
                Some(_) => None,
            },
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum When {
    /// Once no agent works, nothing is queued and no Decision waits.
    Idle,
    Now,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireChoice {
    Main {
        checkout: PathBuf,
    },
    Branch {
        checkout: PathBuf,
        branch: String,
    },
    NewBranch {
        checkout: PathBuf,
    },
    NewWorktree {
        repo: PathBuf,
    },
    ExistingWorktree {
        repo: PathBuf,
        path: PathBuf,
    },
    NamedWorktree {
        repo: PathBuf,
        branch: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireControl {
    RefreshContext,
    RefreshMcp,
    ReconnectMcp {
        server: String,
    },
    LoginMcp {
        server: String,
    },
    ReloadMcp,
    StopTask {
        id: String,
    },
    BackgroundTasks,
    SetPermissionMode {
        mode: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireGroupChange {
    Create {
        first: ThreadId,
        second: ThreadId,
    },
    Join {
        thread: ThreadId,
        group: GroupId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    Include {
        thread: ThreadId,
        group: GroupId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    Leave {
        thread: ThreadId,
    },
    ReorderMember {
        group: GroupId,
        thread: ThreadId,
        index: usize,
    },
    MoveGroup {
        group: GroupId,
        index: usize,
    },
    Rename {
        group: GroupId,
        title: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectAction {
    Register {
        roots: Vec<PathBuf>,
    },
    AddDirectories {
        project: ProjectId,
        roots: Vec<PathBuf>,
    },
    ReplaceDirectory {
        project: ProjectId,
        index: usize,
        root: PathBuf,
    },
    RemoveDirectory {
        project: ProjectId,
        index: usize,
    },
    Remove {
        project: ProjectId,
    },
    Rename {
        project: ProjectId,
        title: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NoticeAction {
    Open {
        notice: u64,
    },
    Dismiss {
        notice: u64,
    },
    OpenDecision {
        thread: ThreadId,
        handle: WireHandle,
    },
    DismissDecision {
        thread: ThreadId,
        handle: WireHandle,
    },
    /// Every Notice read, as the bell does when it opens.
    Read,
    Clear,
    /// A focused window has this Thread on screen.
    Seen {
        thread: ThreadId,
    },
    #[serde(other)]
    Unknown,
}

// ------------------------------------------------------- server → client

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        generation: u32,
        server: ServerInfo,
        boot_id: String,
        #[serde(default)]
        capabilities: Vec<Capability>,
        /// Set when the generations do not meet; the Server then closes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<WireError>,
        /// The Server's clock now, as `at_ms` counts it: a window offsets
        /// every `at_ms` by it, so working timers agree across windows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uptime_ms: Option<u64>,
    },
    Snapshot(Snapshot),
    /// A watched Thread's log, from its start or after `after`.
    History {
        thread: ThreadId,
        after: u64,
        inputs: Vec<WireInput>,
    },
    /// The history is complete up to `seq`; `live` rebuilds what the log
    /// never holds and is still in effect. Inputs continue after `seq`.
    HistoryEnd {
        thread: ThreadId,
        seq: u64,
        live: Vec<WireInput>,
    },
    /// One input the Server applied to a watched Thread, in order.
    Input {
        thread: ThreadId,
        seq: u64,
        /// The Server's monotonic clock since its boot.
        at_ms: u64,
        input: WireInput,
    },
    Decision {
        thread: ThreadId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subject: Option<WireSubject>,
        handle: WireHandle,
        decision: WireDecision,
    },
    DecisionClosed {
        thread: ThreadId,
        handle: WireHandle,
        by: ClosedBy,
    },
    Reply {
        id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ok: Option<Done>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<WireError>,
    },
    /// Everything a window reads of one Thread that is not its Activity.
    Thread {
        revision: u64,
        state: ThreadState,
    },
    /// A tool call's input as far as it has streamed; latest wins.
    ToolDraft {
        thread: ThreadId,
        id: String,
        name: String,
        input: serde_json::Value,
    },
    Roster {
        revision: u64,
        roster: SharedRoster,
    },
    Groups {
        revision: u64,
        groups: serde_json::Value,
    },
    Registry {
        revision: u64,
        registry: serde_json::Value,
    },
    Notices {
        revision: u64,
        notices: Vec<WireNotice>,
    },
    Providers {
        revision: u64,
        providers: Vec<ProviderState>,
    },
    /// The Threads not open, as the Store says them (ADR 0026): what a
    /// window on a machine lists, whose logs stay there.
    Parked {
        revision: u64,
        threads: Vec<ParkedThread>,
    },
    /// A provider's allowance rose past one of the operator's thresholds.
    AllowanceWarning {
        provider: Provider,
        window: WireLimitWindow,
        /// The highest threshold crossed, in percent.
        threshold: u8,
        used_fraction: f32,
    },
    /// A Visual to draw, for the one `render_visual` client chosen.
    Render {
        request: u64,
        id: String,
        title: String,
        html: String,
    },
    /// An API caller asked the windows to land on this Thread
    /// (`thread.focus`, ADR 0027).
    Focus {
        thread: ThreadId,
    },
    /// An API caller's toast (`notification.show`, ADR 0027).
    Notification {
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        #[serde(default, skip_serializing_if = "WireSound::is_none")]
        sound: WireSound,
        /// Left up until dismissed, rather than fading.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        important: bool,
    },
    /// An API caller asked every window's nav to show only this Projects
    /// project's block, by its slug; none shows them all again
    /// (`nav.focus`, ADR 0030).
    NavFocus {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
    /// The Server is about to restart: reconnect.
    Restarting {
        when: When,
    },
    Stopping,
    #[serde(other)]
    Unknown,
}

/// Which chime an API caller's toast rings: a request's (the operator is
/// needed) or a finish's, or none.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WireSound {
    #[default]
    None,
    Request,
    Done,
    #[serde(other)]
    #[schemars(skip)]
    Unknown,
}

impl WireSound {
    pub fn is_none(&self) -> bool {
        *self == WireSound::None
    }

    /// The chime to play; one this window does not know rings none.
    pub fn sound(self) -> Option<crate::sound::Sound> {
        match self {
            WireSound::Request => Some(crate::sound::Sound::NeedsYou),
            WireSound::Done => Some(crate::sound::Sound::Done),
            WireSound::None | WireSound::Unknown => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub version: String,
    /// The Server restarts into this path: a client asks for an upgrade
    /// only when it is its own executable.
    pub executable: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub boot_id: String,
    pub revision: u64,
    pub threads: Vec<ThreadState>,
    pub roster: SharedRoster,
    pub decisions: Vec<PendingEntry>,
    /// `groups.json`, as the Store writes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groups: Option<serde_json::Value>,
    /// `registry.json`, as the Store writes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notices: Option<Vec<WireNotice>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub providers: Option<Vec<ProviderState>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parked: Option<Vec<ParkedThread>>,
}

/// A Thread not open, in creation order: what its log's header and tail
/// say, read where the log is. `provider` is absent when the log could not
/// be read; the row then claims nothing but its age.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParkedThread {
    pub thread: ThreadId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<Provider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Its first prompt, shortened as `title_from_prompt` does: what a
    /// Thread with no title is called.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WireBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_project_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectId>,
    /// When its log was last written, in milliseconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_ms: Option<u64>,
    /// Prompts since its last conversation reset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turns: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagents: Option<usize>,
}

/// One pending Decision, as the snapshot lists them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingEntry {
    pub thread: ThreadId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<WireSubject>,
    pub handle: WireHandle,
    pub decision: WireDecision,
}

/// The shared half of the Roster: which Threads are open, in order, and
/// the order they were parked in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedRoster {
    pub open: Vec<ThreadId>,
    pub parked: Vec<ThreadId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireError {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    AlreadyAnswered,
    InvalidAnswer,
    UnknownThread,
    Unsupported,
    Restarting,
    Failed,
    UnsupportedGeneration,
    /// The client fell more than the reliable queue's bound behind.
    Lagged,
    #[serde(other)]
    Unknown,
}

/// What a successful command produced, when it produced anything.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Done {
    Thread {
        thread: ThreadId,
    },
    Project {
        project: ProjectId,
    },
    /// A prompt handed back to the Composer (`unqueue`), or the file a
    /// `read_file` read.
    Text {
        text: String,
    },
    Files {
        files: Vec<WireFile>,
    },
    /// Whether the act changed anything.
    Changed {
        changed: bool,
    },
    Threads {
        threads: Vec<ThreadId>,
    },
    /// A prompt handed back, and whether it goes ahead of the draft.
    Recovered {
        text: String,
        prepend: bool,
    },
    /// A Group change: the Group it made or changed, and the Groups it
    /// left with a single member, gone.
    Grouped {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group: Option<GroupId>,
        #[serde(default)]
        dissolved: Vec<WireDissolved>,
    },
    /// A `list_dir`'s directory, with `~` spelled out, and its entries in
    /// name order.
    Listing {
        path: PathBuf,
        entries: Vec<WireEntry>,
    },
    /// A `list_repo`'s main checkout's branch, its other worktrees, and
    /// its local branches, each in git's order.
    RepoListing {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checked_out: Option<String>,
        worktrees: Vec<WireWorktree>,
        branches: Vec<String>,
    },
    /// A `review_diff`'s changes as a git-style unified diff.
    Diff {
        text: String,
    },
    /// A `list_commits`' commits, newest first.
    Commits {
        commits: Vec<WireCommit>,
    },
    /// A `review_read`'s changed files, in path order, each with its sides.
    Review {
        files: Vec<WireReviewFile>,
    },
    /// A `search_repo`'s matches: paths first (`file_total` matched in
    /// all), then lines (`code_more` when the cap or time budget cut them).
    Found {
        files: Vec<WireFileHit>,
        code: Vec<WireCodeHit>,
        file_total: usize,
        #[serde(default)]
        code_more: bool,
    },
    /// A `review_pull_request`'s pull request, or the note in its place,
    /// `noun` the forge's word for one.
    ReviewPullRequest {
        noun: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pr: Option<WirePr>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// A `list_files`' entries in path order: every file, and each
    /// ignored directory whole.
    AllFiles {
        entries: Vec<WireTreeEntry>,
    },
    /// A `list_bases`' branches, the default first, then the newest; and
    /// the base the branch scope reads against now.
    Bases {
        bases: Vec<WireBase>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        picked: Option<String>,
    },
    /// A `pull_request`'s reading: `gh pr view`'s JSON as `gh` printed it
    /// (`workspace::pr_view`), or none when `gh` had nothing to say — no
    /// `gh`, not logged in, no PR for the branch.
    PullRequest {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        json: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireEntry {
    pub name: String,
    #[serde(default)]
    pub directory: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireCommit {
    pub id: String,
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// When it was committed, in Unix seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWorktree {
    pub path: PathBuf,
    /// None when its HEAD is detached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

impl From<crate::workspace::RepoListing> for Done {
    fn from(listing: crate::workspace::RepoListing) -> Self {
        Done::RepoListing {
            checked_out: listing.checked_out,
            worktrees: listing
                .worktrees
                .into_iter()
                .map(|worktree| WireWorktree {
                    path: worktree.path,
                    branch: worktree.branch,
                })
                .collect(),
            branches: listing.branches,
        }
    }
}

impl From<crate::workspace::Listing> for Done {
    fn from(listing: crate::workspace::Listing) -> Self {
        Done::Listing {
            path: listing.path,
            entries: listing
                .entries
                .into_iter()
                .map(|entry| WireEntry {
                    name: entry.name,
                    directory: entry.directory,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireDissolved {
    pub group: GroupId,
    pub survivor: ThreadId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireLimitWindow {
    FiveHour,
    Weekly,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireFile {
    pub path: String,
    pub is_directory: bool,
    #[serde(default)]
    pub matched: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClosedBy {
    Answered {
        client: u64,
    },
    Cancelled,
    SessionEnded,
    ServerRestart,
    #[serde(other)]
    Unknown,
}

/// A Thread's state beyond its Activity: header facts, flags, menus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadState {
    pub thread: ThreadId,
    pub generation: u64,
    pub provider: Provider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WireBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_project_root: Option<PathBuf>,
    pub first_prompt_sent: bool,
    pub has_prompt_history: bool,
    /// A Session is starting or being replaced.
    pub starting: bool,
    /// A turn runs.
    pub working: bool,
    /// A model or effort change waits on the provider.
    pub tuning: bool,
    /// The provider holds prompts of its own queue.
    pub native_queue_pending: bool,
    /// Prompts held back, newest first.
    #[serde(default)]
    pub queued: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    #[serde(default)]
    pub commands: Vec<WireCommand>,
    #[serde(default)]
    pub models: Vec<crate::ModelInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub permission_modes: Vec<WirePermissionMode>,
    #[serde(default)]
    pub controls: Vec<WireControlKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_error: Option<String>,
    /// A Draft's first send is starting this Thread: its Pane is the
    /// Draft's until it settles.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub for_draft: bool,
    /// Prompts the provider gave back, for the Composer (`take_recovered`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recovered: Vec<WireRecovered>,
    /// The binding waits on a fresh worktree listing to follow its agent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wants_listing: bool,
    /// Subagent histories that could not be read back from the log.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history_errors: Vec<WireHistoryError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireRecovered {
    pub text: String,
    pub prepend: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireHistoryError {
    pub agent: String,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireBinding {
    Main {
        checkout: PathBuf,
    },
    Worktree {
        repo: PathBuf,
        path: PathBuf,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireCommand {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePermissionMode {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireControlKind {
    RefreshContext,
    RefreshMcp,
    ReconnectMcp,
    LoginMcp,
    ReloadMcp,
    StopTask,
    BackgroundTasks,
    SetPermissionMode,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireNotice {
    /// Main finished a turn.
    Finished {
        id: u64,
        thread: ThreadId,
        outcome: WireOutcome,
        /// Milliseconds since the Unix epoch.
        at_ms: u64,
        read: bool,
    },
    /// A Decision asks for the operator.
    Request {
        thread: ThreadId,
        handle: WireHandle,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subject: Option<WireSubject>,
        question: bool,
        read: bool,
        at_ms: u64,
        seq: u64,
        /// Hidden from the bell until it resolves; it still orders the
        /// Needs-you queue.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        dismissed: bool,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireOutcome {
    Completed,
    Interrupted,
    Error {
        message: String,
    },
    #[serde(other)]
    Unknown,
}

/// What a window shows about one provider: live Sessions, the models it
/// announced, the account's limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderState {
    pub provider: Provider,
    pub live_sessions: usize,
    #[serde(default)]
    pub models: Vec<crate::ModelInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<WireRateLimits>,
}

// ------------------------------------------------------------- inputs

/// One `ActivityInput` the Server applied, as it crosses the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireInput {
    /// A live input the log keeps, as its line: applied live.
    Record(WireRecord),
    /// A line replayed from the log (a revive, a Subagent's history).
    Replay(WireRecord),
    /// A live input the log never keeps.
    Live(Live),
}

/// A log line, or one from a schema this client does not know: skipped,
/// and the window says it is older than its Server.
#[derive(Debug, Clone, PartialEq)]
pub enum WireRecord {
    Known(Box<LogRecord>),
    Unknown,
}

impl Serialize for WireRecord {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Known(record) => record.serialize(serializer),
            Self::Unknown => serializer.serialize_unit(),
        }
    }
}

impl<'de> Deserialize<'de> for WireRecord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(serde_json::from_value(value)
            .map_or(Self::Unknown, |record| Self::Known(Box::new(record))))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Live {
    /// Something Ferrite itself says in the transcript. `replayed`: read
    /// back from the log (a provider switch's line says it this way).
    Notice {
        text: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        replayed: bool,
    },
    /// The Thread's history was replayed into a fresh Session.
    Revived,
    Connect {
        generation: u64,
    },
    Disconnect,
    AnswerSubmitted {
        handle: WireHandle,
    },
    Answered {
        handle: WireHandle,
        allowed: bool,
    },
    /// Main's Session asks for the operator.
    DecisionRequested {
        decision: WireDecision,
    },
    /// An attributed Decision: a Subagent's, or the connection's own.
    Decision {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subject: Option<WireSubject>,
        decision: WireDecision,
    },
    DecisionReply {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    DecisionCancelled {
        id: String,
    },
    Commands {
        commands: Vec<WireCommand>,
    },
    PermissionMode {
        mode: String,
    },
    Models {
        models: Vec<crate::ModelInfo>,
    },
    RateLimits {
        limits: WireRateLimits,
    },
    McpServers {
        servers: Vec<WireMcpServer>,
    },
    McpAuthorization {
        server: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    /// Tool clocks a replay restores, in milliseconds by call id.
    RestoreTimings {
        subject: WireSubject,
        timings: BTreeMap<String, u64>,
    },
    Retain {
        subject: WireSubject,
    },
    Evict {
        subject: WireSubject,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct WireHandle {
    pub generation: u64,
    pub serial: u64,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireSubject {
    Main,
    Subagent {
        key: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireDecision {
    pub id: String,
    pub delivery: WireDelivery,
    pub kind: WireDecisionKind,
    pub policy: WirePolicy,
    pub tool_use_id: String,
    pub tool_name: String,
    pub description: String,
    pub input: serde_json::Value,
    #[serde(default)]
    pub suggestions: Vec<WireDecisionChoice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireDelivery {
    Blocking,
    Async,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireDecisionKind {
    Approval,
    Questions {
        questions: Vec<WireQuestion>,
    },
    Form {
        fields: Vec<WireFormField>,
    },
    External {
        url: String,
    },
    Unsupported {
        reason: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePolicy {
    pub allow: bool,
    pub deny: bool,
    pub prefer_deny: bool,
    pub interaction_required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireDecisionChoice {
    pub label: String,
    pub value: serde_json::Value,
    pub standing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireQuestion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub question: String,
    pub header: String,
    pub options: Vec<WireQuestionChoice>,
    pub multi_select: bool,
    pub secret: bool,
    pub allow_other: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireQuestionChoice {
    pub label: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireFormField {
    pub id: String,
    pub label: String,
    pub description: String,
    pub required: bool,
    pub kind: WireFieldKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireFieldKind {
    String {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_length: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_length: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
    },
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minimum: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<f64>,
    },
    Integer {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minimum: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<i64>,
    },
    Boolean {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<bool>,
    },
    Enum {
        options: Vec<WireFormChoice>,
        multi_select: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_items: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_items: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<serde_json::Value>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireFormChoice {
    pub value: String,
    pub label: String,
}

/// Also the API's `decision.answer` (ADR 0027), hence its JSON Schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireAnswer {
    Allow {
        input: serde_json::Value,
    },
    Deny {
        message: String,
    },
    AllowAlways {
        input: serde_json::Value,
        suggestion: serde_json::Value,
    },
    Choose {
        value: serde_json::Value,
    },
    Questions {
        answers: Vec<WireQuestionAnswer>,
    },
    Form {
        values: serde_json::Value,
    },
    Cancel,
    #[serde(other)]
    #[schemars(skip)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WireQuestionAnswer {
    pub picks: Vec<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WireRateLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour: Option<WireRateWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly: Option<WireRateWindow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WireRateWindow {
    pub used_fraction: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireMcpServer {
    pub name: String,
    pub status: WireMcpStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireMcpStatus {
    Connected,
    Connecting,
    NeedsAuth,
    Failed,
    Disabled,
    #[serde(other)]
    Unknown,
}

// --------------------------------------------------------------- clock

/// An instant as the wire carries it: milliseconds since `boot`, rounded
/// up, so a clock read from it never runs ahead of the one it stands for.
pub fn ms_since(at: std::time::Instant, boot: std::time::Instant) -> u64 {
    at.saturating_duration_since(boot)
        .as_nanos()
        .div_ceil(1_000_000) as u64
}

/// The instant `ms` milliseconds after `boot`.
pub fn instant_at(ms: u64, boot: std::time::Instant) -> std::time::Instant {
    boot + Duration::from_millis(ms)
}

// ------------------------------------------------------------- framing

/// One message as one line: its JSON and a newline.
pub fn encode<T: Serialize>(message: &T) -> Vec<u8> {
    let mut line = serde_json::to_vec(message).expect("protocol messages always encode");
    line.push(b'\n');
    line
}

/// One line, without its newline.
pub fn decode<'a, T: Deserialize<'a>>(line: &'a [u8]) -> serde_json::Result<T> {
    serde_json::from_slice(line)
}

/// The Server's answer to a `hello`: a welcome, refused when the client
/// speaks another generation.
pub fn welcome(
    generation: u32,
    server: ServerInfo,
    boot_id: String,
    uptime_ms: u64,
) -> ServerMessage {
    ServerMessage::Welcome {
        generation: GENERATION,
        server,
        boot_id,
        capabilities: Vec::new(),
        error: (generation != GENERATION).then(|| WireError {
            code: ErrorCode::UnsupportedGeneration,
            message: format!("this server speaks generation {GENERATION}, not {generation}"),
        }),
        uptime_ms: Some(uptime_ms),
    }
}

// --------------------------------------------------------- conversions

/// Why a wire value has no core meaning here: it names something this
/// build does not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unknown;

/// How an `ActivityInput` crosses: an input, the latest-wins tool draft,
/// or nothing (the highlighter's work is each side's own).
#[derive(Debug, Clone, PartialEq)]
pub enum Outgoing {
    Input(WireInput),
    ToolDraft {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    Local,
}

/// Encode one applied input. `duration` is the tool clock the Server
/// measured, as the log records it.
pub fn outgoing(input: &ActivityInput, duration: Option<Duration>) -> Outgoing {
    let live = |live| Outgoing::Input(WireInput::Live(live));
    match input {
        ActivityInput::Main { input, .. } => main_outgoing(input, duration),
        ActivityInput::Observe { event, .. } => {
            match LogRecord::of(
                &Input::Event(SessionEvent::Activity(event.clone())),
                duration,
            ) {
                Some(record) => {
                    Outgoing::Input(WireInput::Record(WireRecord::Known(Box::new(record))))
                }
                None => live_activity(event).map_or(Outgoing::Local, live),
            }
        }
        // A replay is of the log: a line of it, or what a line said.
        ActivityInput::Replay(Input::Notice(text)) => live(Live::Notice {
            text: text.clone(),
            replayed: true,
        }),
        ActivityInput::Replay(input) => LogRecord::of(input, None)
            .map_or(Outgoing::Local, |record| {
                Outgoing::Input(WireInput::Replay(WireRecord::Known(Box::new(record))))
            }),
        ActivityInput::ReplayEvent(event) => LogRecord::of(
            &Input::Event(SessionEvent::Activity(event.clone())),
            duration,
        )
        .map_or(Outgoing::Local, |record| {
            Outgoing::Input(WireInput::Replay(WireRecord::Known(Box::new(record))))
        }),
        ActivityInput::Connect { generation } => live(Live::Connect {
            generation: *generation,
        }),
        ActivityInput::Disconnect => live(Live::Disconnect),
        ActivityInput::AnswerSubmitted { handle } => live(Live::AnswerSubmitted {
            handle: handle.into(),
        }),
        ActivityInput::Answered {
            handle, allowed, ..
        } => live(Live::Answered {
            handle: handle.into(),
            allowed: *allowed,
        }),
        ActivityInput::RestoreTimings { subject, timings } => live(Live::RestoreTimings {
            subject: subject.into(),
            timings: timings
                .iter()
                .map(|(id, total)| (id.clone(), total.as_millis() as u64))
                .collect(),
        }),
        ActivityInput::Retain(subject) => live(Live::Retain {
            subject: subject.into(),
        }),
        ActivityInput::Evict(subject) => live(Live::Evict {
            subject: subject.into(),
        }),
        ActivityInput::DrainHighlights => Outgoing::Local,
    }
}

fn main_outgoing(input: &Input, duration: Option<Duration>) -> Outgoing {
    if let Some(record) = LogRecord::of(input, duration) {
        return Outgoing::Input(WireInput::Record(WireRecord::Known(Box::new(record))));
    }
    let live = match input {
        Input::Notice(text) => Live::Notice {
            text: text.clone(),
            replayed: false,
        },
        Input::Revived => Live::Revived,
        Input::Event(SessionEvent::ToolDraft { id, name, input }) => {
            return Outgoing::ToolDraft {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            }
        }
        Input::Event(SessionEvent::DecisionRequested { decision }) => Live::DecisionRequested {
            decision: decision.into(),
        },
        Input::Event(SessionEvent::Commands { commands }) => Live::Commands {
            commands: commands.iter().map(Into::into).collect(),
        },
        Input::Event(SessionEvent::PermissionMode { mode }) => {
            Live::PermissionMode { mode: mode.clone() }
        }
        Input::Event(SessionEvent::Models { models }) => Live::Models {
            models: models.clone(),
        },
        Input::Event(SessionEvent::RateLimits { five_hour, weekly }) => Live::RateLimits {
            limits: WireRateLimits::from(crate::transcript::RateLimits {
                five_hour: *five_hour,
                weekly: *weekly,
            }),
        },
        Input::Event(SessionEvent::McpServers { servers }) => Live::McpServers {
            servers: servers.iter().map(Into::into).collect(),
        },
        Input::Event(SessionEvent::McpAuthorization { server, url }) => Live::McpAuthorization {
            server: server.clone(),
            url: url.clone(),
        },
        // Queue acknowledgements are the Thread's state, not its Activity's;
        // the highlighter is each side's own; `Answered` is Activity's.
        Input::Event(SessionEvent::Queue(_))
        | Input::Highlighted { .. }
        | Input::Answered { .. } => return Outgoing::Local,
        Input::Event(_)
        | Input::Prompt(_)
        | Input::PromptObservation { .. }
        | Input::CompletionObservation { .. } => unreachable!("the log keeps {input:?}"),
    };
    Outgoing::Input(WireInput::Live(live))
}

fn live_activity(event: &ActivityEvent) -> Option<Live> {
    Some(match event {
        ActivityEvent::Decision { subject, decision } => Live::Decision {
            subject: subject.as_ref().map(Into::into),
            decision: decision.into(),
        },
        ActivityEvent::DecisionReply { id, error } => Live::DecisionReply {
            id: id.clone(),
            error: error.clone(),
        },
        ActivityEvent::DecisionCancelled { id } => Live::DecisionCancelled { id: id.clone() },
        _ => return None,
    })
}

/// Decode one input for a mirror. `generation` is the Session the input
/// belongs to; `at` is its instant on this side's clock. `None` means the
/// input names something this build does not know, and is skipped.
pub fn incoming(
    input: &WireInput,
    generation: u64,
    at: std::time::Instant,
) -> Option<ActivityInput> {
    Some(match input {
        WireInput::Record(WireRecord::Known(record)) => match record.input()? {
            Input::Event(SessionEvent::Activity(event)) => ActivityInput::Observe {
                generation,
                event,
                at,
            },
            input => ActivityInput::Main { input, at },
        },
        WireInput::Replay(WireRecord::Known(record)) => match record.input()? {
            Input::Event(SessionEvent::Activity(event)) => ActivityInput::ReplayEvent(event),
            input => ActivityInput::Replay(input),
        },
        WireInput::Record(WireRecord::Unknown) | WireInput::Replay(WireRecord::Unknown) => {
            return None
        }
        WireInput::Live(live) => {
            let main = |event| ActivityInput::Main {
                input: Input::Event(event),
                at,
            };
            let observe = |event| ActivityInput::Observe {
                generation,
                event,
                at,
            };
            match live {
                Live::Notice {
                    text,
                    replayed: true,
                } => ActivityInput::Replay(Input::Notice(text.clone())),
                Live::Notice { text, .. } => ActivityInput::Main {
                    input: Input::Notice(text.clone()),
                    at,
                },
                Live::Revived => ActivityInput::Main {
                    input: Input::Revived,
                    at,
                },
                Live::Connect { generation } => ActivityInput::Connect {
                    generation: *generation,
                },
                Live::Disconnect => ActivityInput::Disconnect,
                Live::AnswerSubmitted { handle } => ActivityInput::AnswerSubmitted {
                    handle: handle.into(),
                },
                Live::Answered { handle, allowed } => ActivityInput::Answered {
                    handle: handle.into(),
                    allowed: *allowed,
                    at,
                },
                Live::DecisionRequested { decision } => main(SessionEvent::DecisionRequested {
                    decision: decision.try_into().ok()?,
                }),
                Live::Decision { subject, decision } => observe(ActivityEvent::Decision {
                    subject: match subject {
                        Some(subject) => Some(subject.try_into().ok()?),
                        None => None,
                    },
                    decision: decision.try_into().ok()?,
                }),
                Live::DecisionReply { id, error } => observe(ActivityEvent::DecisionReply {
                    id: id.clone(),
                    error: error.clone(),
                }),
                Live::DecisionCancelled { id } => {
                    observe(ActivityEvent::DecisionCancelled { id: id.clone() })
                }
                Live::Commands { commands } => main(SessionEvent::Commands {
                    commands: commands.iter().map(Into::into).collect(),
                }),
                Live::PermissionMode { mode } => {
                    main(SessionEvent::PermissionMode { mode: mode.clone() })
                }
                Live::Models { models } => main(SessionEvent::Models {
                    models: models.clone(),
                }),
                Live::RateLimits { limits } => main(SessionEvent::RateLimits {
                    five_hour: limits.five_hour.map(Into::into),
                    weekly: limits.weekly.map(Into::into),
                }),
                Live::McpServers { servers } => main(SessionEvent::McpServers {
                    servers: servers.iter().map(Into::into).collect(),
                }),
                Live::McpAuthorization { server, url } => main(SessionEvent::McpAuthorization {
                    server: server.clone(),
                    url: url.clone(),
                }),
                Live::RestoreTimings { subject, timings } => ActivityInput::RestoreTimings {
                    subject: subject.try_into().ok()?,
                    timings: timings
                        .iter()
                        .map(|(id, ms)| (id.clone(), Duration::from_millis(*ms)))
                        .collect(),
                },
                Live::Retain { subject } => ActivityInput::Retain(subject.try_into().ok()?),
                Live::Evict { subject } => ActivityInput::Evict(subject.try_into().ok()?),
                Live::Unknown => return None,
            }
        }
    })
}

impl From<&DecisionHandle> for WireHandle {
    fn from(handle: &DecisionHandle) -> Self {
        Self {
            generation: handle.generation,
            serial: handle.serial,
            request_id: handle.request_id.clone(),
        }
    }
}

impl From<&WireHandle> for DecisionHandle {
    fn from(handle: &WireHandle) -> Self {
        Self {
            generation: handle.generation,
            serial: handle.serial,
            request_id: handle.request_id.clone(),
        }
    }
}

impl From<&Subject> for WireSubject {
    fn from(subject: &Subject) -> Self {
        match subject {
            Subject::Main => Self::Main,
            Subject::Subagent(key) => Self::Subagent {
                key: key.as_str().into(),
            },
        }
    }
}

impl TryFrom<&WireSubject> for Subject {
    type Error = Unknown;
    fn try_from(subject: &WireSubject) -> Result<Self, Unknown> {
        match subject {
            WireSubject::Main => Ok(Self::Main),
            WireSubject::Subagent { key } => Ok(Self::Subagent(AgentKey::from_stored(key.clone()))),
            WireSubject::Unknown => Err(Unknown),
        }
    }
}

impl From<&crate::Decision> for WireDecision {
    fn from(decision: &crate::Decision) -> Self {
        Self {
            id: decision.id.clone(),
            delivery: match decision.delivery {
                crate::DecisionDelivery::Blocking => WireDelivery::Blocking,
                crate::DecisionDelivery::Async => WireDelivery::Async,
            },
            kind: match &decision.kind {
                crate::DecisionKind::Approval => WireDecisionKind::Approval,
                crate::DecisionKind::Questions(questions) => WireDecisionKind::Questions {
                    questions: questions.iter().map(Into::into).collect(),
                },
                crate::DecisionKind::Form { fields } => WireDecisionKind::Form {
                    fields: fields.iter().map(Into::into).collect(),
                },
                crate::DecisionKind::External { url } => {
                    WireDecisionKind::External { url: url.clone() }
                }
                crate::DecisionKind::Unsupported { reason } => WireDecisionKind::Unsupported {
                    reason: reason.clone(),
                },
            },
            policy: WirePolicy {
                allow: decision.policy.allow,
                deny: decision.policy.deny,
                prefer_deny: decision.policy.prefer_deny,
                interaction_required: decision.policy.interaction_required,
            },
            tool_use_id: decision.tool_use_id.clone(),
            tool_name: decision.tool_name.clone(),
            description: decision.description.clone(),
            input: decision.input.clone(),
            suggestions: decision
                .suggestions
                .iter()
                .map(|choice| WireDecisionChoice {
                    label: choice.label.clone(),
                    value: choice.value.clone(),
                    standing: choice.standing,
                })
                .collect(),
        }
    }
}

impl TryFrom<&WireDecision> for crate::Decision {
    type Error = Unknown;
    fn try_from(decision: &WireDecision) -> Result<Self, Unknown> {
        Ok(Self {
            delivery: match decision.delivery {
                WireDelivery::Blocking => crate::DecisionDelivery::Blocking,
                WireDelivery::Async => crate::DecisionDelivery::Async,
                WireDelivery::Unknown => return Err(Unknown),
            },
            // A kind this window cannot draw is still a Decision it can
            // refuse: it shows as one it cannot render.
            kind: match &decision.kind {
                WireDecisionKind::Approval => crate::DecisionKind::Approval,
                WireDecisionKind::Questions { questions } => {
                    crate::DecisionKind::Questions(questions.iter().map(Into::into).collect())
                }
                WireDecisionKind::Form { fields } => crate::DecisionKind::Form {
                    fields: fields
                        .iter()
                        .map(TryInto::try_into)
                        .collect::<Result<_, _>>()?,
                },
                WireDecisionKind::External { url } => {
                    crate::DecisionKind::External { url: url.clone() }
                }
                WireDecisionKind::Unsupported { reason } => crate::DecisionKind::Unsupported {
                    reason: reason.clone(),
                },
                WireDecisionKind::Unknown => crate::DecisionKind::Unsupported {
                    reason: "this window is older than its server".into(),
                },
            },
            policy: crate::DecisionPolicy {
                allow: decision.policy.allow,
                deny: decision.policy.deny,
                prefer_deny: decision.policy.prefer_deny,
                interaction_required: decision.policy.interaction_required,
            },
            id: decision.id.clone(),
            tool_use_id: decision.tool_use_id.clone(),
            tool_name: decision.tool_name.clone(),
            description: decision.description.clone(),
            input: decision.input.clone(),
            suggestions: decision
                .suggestions
                .iter()
                .map(|choice| crate::DecisionChoice {
                    label: choice.label.clone(),
                    value: choice.value.clone(),
                    standing: choice.standing,
                })
                .collect(),
        })
    }
}

impl From<&crate::questions::Question> for WireQuestion {
    fn from(question: &crate::questions::Question) -> Self {
        Self {
            id: question.id.clone(),
            question: question.question.clone(),
            header: question.header.clone(),
            options: question
                .options
                .iter()
                .map(|choice| WireQuestionChoice {
                    label: choice.label.clone(),
                    description: choice.description.clone(),
                    preview: choice.preview.clone(),
                })
                .collect(),
            multi_select: question.multi_select,
            secret: question.secret,
            allow_other: question.allow_other,
        }
    }
}

impl From<&WireQuestion> for crate::questions::Question {
    fn from(question: &WireQuestion) -> Self {
        Self {
            id: question.id.clone(),
            question: question.question.clone(),
            header: question.header.clone(),
            options: question
                .options
                .iter()
                .map(|choice| crate::questions::Choice {
                    label: choice.label.clone(),
                    description: choice.description.clone(),
                    preview: choice.preview.clone(),
                })
                .collect(),
            multi_select: question.multi_select,
            secret: question.secret,
            allow_other: question.allow_other,
        }
    }
}

impl From<&crate::FormField> for WireFormField {
    fn from(field: &crate::FormField) -> Self {
        use crate::FormFieldKind as Kind;
        Self {
            id: field.id.clone(),
            label: field.label.clone(),
            description: field.description.clone(),
            required: field.required,
            kind: match &field.kind {
                Kind::String {
                    min_length,
                    max_length,
                    default,
                } => WireFieldKind::String {
                    min_length: *min_length,
                    max_length: *max_length,
                    default: default.clone(),
                },
                Kind::Number {
                    minimum,
                    maximum,
                    default,
                } => WireFieldKind::Number {
                    minimum: *minimum,
                    maximum: *maximum,
                    default: *default,
                },
                Kind::Integer {
                    minimum,
                    maximum,
                    default,
                } => WireFieldKind::Integer {
                    minimum: *minimum,
                    maximum: *maximum,
                    default: *default,
                },
                Kind::Boolean { default } => WireFieldKind::Boolean { default: *default },
                Kind::Enum {
                    options,
                    multi_select,
                    min_items,
                    max_items,
                    default,
                } => WireFieldKind::Enum {
                    options: options
                        .iter()
                        .map(|choice| WireFormChoice {
                            value: choice.value.clone(),
                            label: choice.label.clone(),
                        })
                        .collect(),
                    multi_select: *multi_select,
                    min_items: *min_items,
                    max_items: *max_items,
                    default: default.clone(),
                },
            },
        }
    }
}

impl TryFrom<&WireFormField> for crate::FormField {
    type Error = Unknown;
    fn try_from(field: &WireFormField) -> Result<Self, Unknown> {
        use crate::FormFieldKind as Kind;
        Ok(Self {
            id: field.id.clone(),
            label: field.label.clone(),
            description: field.description.clone(),
            required: field.required,
            kind: match &field.kind {
                WireFieldKind::String {
                    min_length,
                    max_length,
                    default,
                } => Kind::String {
                    min_length: *min_length,
                    max_length: *max_length,
                    default: default.clone(),
                },
                WireFieldKind::Number {
                    minimum,
                    maximum,
                    default,
                } => Kind::Number {
                    minimum: *minimum,
                    maximum: *maximum,
                    default: *default,
                },
                WireFieldKind::Integer {
                    minimum,
                    maximum,
                    default,
                } => Kind::Integer {
                    minimum: *minimum,
                    maximum: *maximum,
                    default: *default,
                },
                WireFieldKind::Boolean { default } => Kind::Boolean { default: *default },
                WireFieldKind::Enum {
                    options,
                    multi_select,
                    min_items,
                    max_items,
                    default,
                } => Kind::Enum {
                    options: options
                        .iter()
                        .map(|choice| crate::FormChoice {
                            value: choice.value.clone(),
                            label: choice.label.clone(),
                        })
                        .collect(),
                    multi_select: *multi_select,
                    min_items: *min_items,
                    max_items: *max_items,
                    default: default.clone(),
                },
                WireFieldKind::Unknown => return Err(Unknown),
            },
        })
    }
}

impl From<&crate::DecisionAnswer> for WireAnswer {
    fn from(answer: &crate::DecisionAnswer) -> Self {
        use crate::DecisionAnswer as Answer;
        match answer {
            Answer::Allow { input } => Self::Allow {
                input: input.clone(),
            },
            Answer::Deny { message } => Self::Deny {
                message: message.clone(),
            },
            Answer::AllowAlways { input, suggestion } => Self::AllowAlways {
                input: input.clone(),
                suggestion: suggestion.clone(),
            },
            Answer::Choose { value } => Self::Choose {
                value: value.clone(),
            },
            Answer::Questions { answers } => Self::Questions {
                answers: answers
                    .iter()
                    .map(|answer| WireQuestionAnswer {
                        picks: answer.picks.clone(),
                        other: answer.other.clone(),
                    })
                    .collect(),
            },
            Answer::Form { values } => Self::Form {
                values: values.clone(),
            },
            Answer::Cancel => Self::Cancel,
        }
    }
}

impl TryFrom<&WireAnswer> for crate::DecisionAnswer {
    type Error = Unknown;
    fn try_from(answer: &WireAnswer) -> Result<Self, Unknown> {
        Ok(match answer {
            WireAnswer::Allow { input } => Self::Allow {
                input: input.clone(),
            },
            WireAnswer::Deny { message } => Self::Deny {
                message: message.clone(),
            },
            WireAnswer::AllowAlways { input, suggestion } => Self::AllowAlways {
                input: input.clone(),
                suggestion: suggestion.clone(),
            },
            WireAnswer::Choose { value } => Self::Choose {
                value: value.clone(),
            },
            WireAnswer::Questions { answers } => Self::Questions {
                answers: answers
                    .iter()
                    .map(|answer| crate::questions::Answer {
                        picks: answer.picks.clone(),
                        other: answer.other.clone(),
                    })
                    .collect(),
            },
            WireAnswer::Form { values } => Self::Form {
                values: values.clone(),
            },
            WireAnswer::Cancel => Self::Cancel,
            WireAnswer::Unknown => return Err(Unknown),
        })
    }
}

impl From<&crate::SessionCommand> for WireCommand {
    fn from(command: &crate::SessionCommand) -> Self {
        Self {
            name: command.name.clone(),
            description: command.description.clone(),
            path: command.path.clone(),
        }
    }
}

impl From<&WireCommand> for crate::SessionCommand {
    fn from(command: &WireCommand) -> Self {
        Self {
            name: command.name.clone(),
            description: command.description.clone(),
            path: command.path.clone(),
        }
    }
}

impl From<crate::transcript::RateLimits> for WireRateLimits {
    fn from(limits: crate::transcript::RateLimits) -> Self {
        Self {
            five_hour: limits.five_hour.map(Into::into),
            weekly: limits.weekly.map(Into::into),
        }
    }
}

impl From<WireRateLimits> for crate::transcript::RateLimits {
    fn from(limits: WireRateLimits) -> Self {
        Self {
            five_hour: limits.five_hour.map(Into::into),
            weekly: limits.weekly.map(Into::into),
        }
    }
}

impl From<crate::RateLimitWindow> for WireRateWindow {
    fn from(window: crate::RateLimitWindow) -> Self {
        Self {
            used_fraction: window.used_fraction,
            resets_at: window.resets_at,
        }
    }
}

impl From<WireRateWindow> for crate::RateLimitWindow {
    fn from(window: WireRateWindow) -> Self {
        Self {
            used_fraction: window.used_fraction,
            resets_at: window.resets_at,
        }
    }
}

impl From<&crate::McpServer> for WireMcpServer {
    fn from(server: &crate::McpServer) -> Self {
        use crate::McpStatus as Status;
        Self {
            name: server.name.clone(),
            status: match server.status {
                Status::Connected => WireMcpStatus::Connected,
                Status::Connecting => WireMcpStatus::Connecting,
                Status::NeedsAuth => WireMcpStatus::NeedsAuth,
                Status::Failed => WireMcpStatus::Failed,
                Status::Disabled => WireMcpStatus::Disabled,
                Status::Unknown => WireMcpStatus::Unknown,
            },
            error: server.error.clone(),
        }
    }
}

impl From<&WireMcpServer> for crate::McpServer {
    fn from(server: &WireMcpServer) -> Self {
        use crate::McpStatus as Status;
        Self {
            name: server.name.clone(),
            // The provider's own `Unknown` and a status this build does not
            // know read the same.
            status: match server.status {
                WireMcpStatus::Connected => Status::Connected,
                WireMcpStatus::Connecting => Status::Connecting,
                WireMcpStatus::NeedsAuth => Status::NeedsAuth,
                WireMcpStatus::Failed => Status::Failed,
                WireMcpStatus::Disabled => Status::Disabled,
                WireMcpStatus::Unknown => Status::Unknown,
            },
            error: server.error.clone(),
        }
    }
}

impl From<&crate::ControlKind> for WireControlKind {
    fn from(kind: &crate::ControlKind) -> Self {
        use crate::ControlKind as Kind;
        match kind {
            Kind::RefreshContext => Self::RefreshContext,
            Kind::RefreshMcp => Self::RefreshMcp,
            Kind::ReconnectMcp => Self::ReconnectMcp,
            Kind::LoginMcp => Self::LoginMcp,
            Kind::ReloadMcp => Self::ReloadMcp,
            Kind::StopTask => Self::StopTask,
            Kind::BackgroundTasks => Self::BackgroundTasks,
            Kind::SetPermissionMode => Self::SetPermissionMode,
        }
    }
}

impl TryFrom<&WireControlKind> for crate::ControlKind {
    type Error = Unknown;
    fn try_from(kind: &WireControlKind) -> Result<Self, Unknown> {
        use WireControlKind as Kind;
        Ok(match kind {
            Kind::RefreshContext => Self::RefreshContext,
            Kind::RefreshMcp => Self::RefreshMcp,
            Kind::ReconnectMcp => Self::ReconnectMcp,
            Kind::LoginMcp => Self::LoginMcp,
            Kind::ReloadMcp => Self::ReloadMcp,
            Kind::StopTask => Self::StopTask,
            Kind::BackgroundTasks => Self::BackgroundTasks,
            Kind::SetPermissionMode => Self::SetPermissionMode,
            Kind::Unknown => return Err(Unknown),
        })
    }
}

impl From<&crate::SessionControl> for WireControl {
    fn from(control: &crate::SessionControl) -> Self {
        use crate::SessionControl as Control;
        match control {
            Control::RefreshContext => Self::RefreshContext,
            Control::RefreshMcp => Self::RefreshMcp,
            Control::ReconnectMcp { server } => Self::ReconnectMcp {
                server: server.clone(),
            },
            Control::LoginMcp { server } => Self::LoginMcp {
                server: server.clone(),
            },
            Control::ReloadMcp => Self::ReloadMcp,
            Control::StopTask { id } => Self::StopTask { id: id.clone() },
            Control::BackgroundTasks => Self::BackgroundTasks,
            Control::SetPermissionMode { mode } => Self::SetPermissionMode { mode: mode.clone() },
        }
    }
}

impl TryFrom<&WireControl> for crate::SessionControl {
    type Error = Unknown;
    fn try_from(control: &WireControl) -> Result<Self, Unknown> {
        Ok(match control {
            WireControl::RefreshContext => Self::RefreshContext,
            WireControl::RefreshMcp => Self::RefreshMcp,
            WireControl::ReconnectMcp { server } => Self::ReconnectMcp {
                server: server.clone(),
            },
            WireControl::LoginMcp { server } => Self::LoginMcp {
                server: server.clone(),
            },
            WireControl::ReloadMcp => Self::ReloadMcp,
            WireControl::StopTask { id } => Self::StopTask { id: id.clone() },
            WireControl::BackgroundTasks => Self::BackgroundTasks,
            WireControl::SetPermissionMode { mode } => {
                Self::SetPermissionMode { mode: mode.clone() }
            }
            WireControl::Unknown => return Err(Unknown),
        })
    }
}

impl From<&WorkspaceChoice> for WireChoice {
    fn from(choice: &WorkspaceChoice) -> Self {
        match choice {
            WorkspaceChoice::Main { checkout } => Self::Main {
                checkout: checkout.clone(),
            },
            WorkspaceChoice::Branch { checkout, branch } => Self::Branch {
                checkout: checkout.clone(),
                branch: branch.clone(),
            },
            WorkspaceChoice::NewBranch { checkout } => Self::NewBranch {
                checkout: checkout.clone(),
            },
            WorkspaceChoice::NewWorktree { repo } => Self::NewWorktree { repo: repo.clone() },
            WorkspaceChoice::NamedWorktree { repo, branch, base } => Self::NamedWorktree {
                repo: repo.clone(),
                branch: branch.clone(),
                base: base.clone(),
            },
            WorkspaceChoice::ExistingWorktree { repo, path } => Self::ExistingWorktree {
                repo: repo.clone(),
                path: path.clone(),
            },
        }
    }
}

impl TryFrom<&WireChoice> for WorkspaceChoice {
    type Error = Unknown;
    fn try_from(choice: &WireChoice) -> Result<Self, Unknown> {
        Ok(match choice {
            WireChoice::Main { checkout } => Self::Main {
                checkout: checkout.clone(),
            },
            WireChoice::Branch { checkout, branch } => Self::Branch {
                checkout: checkout.clone(),
                branch: branch.clone(),
            },
            WireChoice::NewBranch { checkout } => Self::NewBranch {
                checkout: checkout.clone(),
            },
            WireChoice::NewWorktree { repo } => Self::NewWorktree { repo: repo.clone() },
            WireChoice::NamedWorktree { repo, branch, base } => Self::NamedWorktree {
                repo: repo.clone(),
                branch: branch.clone(),
                base: base.clone(),
            },
            WireChoice::ExistingWorktree { repo, path } => Self::ExistingWorktree {
                repo: repo.clone(),
                path: path.clone(),
            },
            WireChoice::Unknown => return Err(Unknown),
        })
    }
}

impl From<&WorkspaceBinding> for WireBinding {
    fn from(binding: &WorkspaceBinding) -> Self {
        match binding {
            WorkspaceBinding::Main { checkout } => Self::Main {
                checkout: checkout.clone(),
            },
            WorkspaceBinding::Worktree { repo, path } => Self::Worktree {
                repo: repo.clone(),
                path: path.clone(),
            },
        }
    }
}

impl TryFrom<&WireBinding> for WorkspaceBinding {
    type Error = Unknown;
    fn try_from(binding: &WireBinding) -> Result<Self, Unknown> {
        Ok(match binding {
            WireBinding::Main { checkout } => Self::Main {
                checkout: checkout.clone(),
            },
            WireBinding::Worktree { repo, path } => Self::Worktree {
                repo: repo.clone(),
                path: path.clone(),
            },
            WireBinding::Unknown => return Err(Unknown),
        })
    }
}

impl From<&GroupChange> for WireGroupChange {
    fn from(change: &GroupChange) -> Self {
        match change.clone() {
            GroupChange::Create { first, second } => Self::Create { first, second },
            GroupChange::Join {
                thread,
                group,
                index,
            } => Self::Join {
                thread,
                group,
                index,
            },
            GroupChange::Include {
                thread,
                group,
                index,
            } => Self::Include {
                thread,
                group,
                index,
            },
            GroupChange::Leave { thread } => Self::Leave { thread },
            GroupChange::ReorderMember {
                group,
                thread,
                index,
            } => Self::ReorderMember {
                group,
                thread,
                index,
            },
            GroupChange::MoveGroup { group, index } => Self::MoveGroup { group, index },
            GroupChange::Rename { group, title } => Self::Rename { group, title },
        }
    }
}

impl TryFrom<&WireGroupChange> for GroupChange {
    type Error = Unknown;
    fn try_from(change: &WireGroupChange) -> Result<Self, Unknown> {
        Ok(match change.clone() {
            WireGroupChange::Create { first, second } => Self::Create { first, second },
            WireGroupChange::Join {
                thread,
                group,
                index,
            } => Self::Join {
                thread,
                group,
                index,
            },
            WireGroupChange::Include {
                thread,
                group,
                index,
            } => Self::Include {
                thread,
                group,
                index,
            },
            WireGroupChange::Leave { thread } => Self::Leave { thread },
            WireGroupChange::ReorderMember {
                group,
                thread,
                index,
            } => Self::ReorderMember {
                group,
                thread,
                index,
            },
            WireGroupChange::MoveGroup { group, index } => Self::MoveGroup { group, index },
            WireGroupChange::Rename { group, title } => Self::Rename { group, title },
            WireGroupChange::Unknown => return Err(Unknown),
        })
    }
}

impl From<crate::providers::limits::LimitWindow> for WireLimitWindow {
    fn from(window: crate::providers::limits::LimitWindow) -> Self {
        use crate::providers::limits::LimitWindow;
        match window {
            LimitWindow::FiveHour => Self::FiveHour,
            LimitWindow::Weekly => Self::Weekly,
        }
    }
}

impl TryFrom<WireLimitWindow> for crate::providers::limits::LimitWindow {
    type Error = Unknown;
    fn try_from(window: WireLimitWindow) -> Result<Self, Unknown> {
        Ok(match window {
            WireLimitWindow::FiveHour => Self::FiveHour,
            WireLimitWindow::Weekly => Self::Weekly,
            WireLimitWindow::Unknown => return Err(Unknown),
        })
    }
}

impl From<&crate::TurnOutcome> for WireOutcome {
    fn from(outcome: &crate::TurnOutcome) -> Self {
        match outcome {
            crate::TurnOutcome::Completed => Self::Completed,
            crate::TurnOutcome::Interrupted => Self::Interrupted,
            crate::TurnOutcome::Error(message) => Self::Error {
                message: message.clone(),
            },
        }
    }
}

impl TryFrom<&WireOutcome> for crate::TurnOutcome {
    type Error = Unknown;
    fn try_from(outcome: &WireOutcome) -> Result<Self, Unknown> {
        Ok(match outcome {
            WireOutcome::Completed => Self::Completed,
            WireOutcome::Interrupted => Self::Interrupted,
            WireOutcome::Error { message } => Self::Error(message.clone()),
            WireOutcome::Unknown => return Err(Unknown),
        })
    }
}

#[cfg(test)]
mod tests;
