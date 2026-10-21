//! The Engine (ADR 0025): everything that keeps running without a window.
//! The Store and its claim, every Session and its writer, the prompt
//! queue, pending Decisions, Notices, Groups, the registry, and every
//! policy — Handover, the memory watchdog, ending idle Sessions, the titler,
//! suggestions, deferred finish. The Server owns one; a window reads it
//! through its client, `cockpit::Cockpit`, and changes it only by its acts.
//!
//! What it shares of the Roster is which Threads are open, in the order
//! they opened (`open_threads`). Focus, the View, fullscreen and Drafts are
//! each window's own.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

pub use crate::activity::ToolTiming;
use crate::activity::{
    Activity, ActivityEvent, ActivityInput, ActivityUpdate, ActivityView, AgentKey, DecisionHandle,
    PendingDecision, Subject,
};
use crate::digest::Digest;
use crate::groups::{Applied, ApplyError, Drag, DropTarget, GroupChange, GroupId, Groups, Plan};
use crate::notifications::{DecisionNoticeId, Frame, NoticeId, Notifications};
pub use crate::prompt_history::HistoryDirection;
use crate::prompt_history::PromptHistory;
use crate::protocol::{self, Outgoing, ServerMessage, SharedRoster};
use crate::providers::{Session, SettingAck};
use crate::roster::{DraftId, DraftScope};
use crate::session::SessionLifecycle;
use crate::store::{LoadError, Provider, Store, ThreadWriter};
use crate::suggest::Suggestion;
use crate::transcript::{BlockId, Input, Transcript, Update};
use crate::workspace::follow::{self, Follow, Move};
use crate::workspace::registry::{self, ProjectId, Registry};
use crate::workspace::{self, WorkspaceBinding, WorkspaceChoice};
use crate::{Decision, DecisionAnswer, ModelInfo, SessionEvent, ThreadId};

mod announced;
mod commands;
mod history;
mod launch_args;
mod queue;
mod windows;

pub use windows::WindowId;

/// How a Thread's first Session starts, beyond its provider and checkout
/// (`thread.start`, ADR 0030), or a parked one's next (`thread.revive`,
/// ADR 0031).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Launch {
    /// The Provider these overrides were written for. On revival after a
    /// handover, foreign overrides leave the current conversation intact.
    pub launch_provider: Option<Provider>,
    /// Extra arguments for the provider's CLI, after Ferrite's own, for
    /// every Session of that provider while the Thread is open.
    pub args: Vec<String>,
    /// Set in the agent's environment, under Ferrite's own, for every
    /// Session of that provider.
    pub env: Vec<(String, String)>,
    /// A program the provider's CLI is run through, and its arguments
    /// (`bwrap … --`): the CLI and its arguments follow them.
    pub launcher: Vec<String>,
    /// The provider's own id of a conversation to carry on.
    pub resume: Option<String>,
    /// The Project the Thread joins when its checkout is a directory inside
    /// one rather than one of its roots (`thread.start --project --cwd`, as
    /// Herdr starts a tab in its cwd); `None` is the checkout's own Project.
    pub project: Option<ProjectId>,
}

/// Everything one spawn needs, in one struct: every path that starts a
/// Session (open, revive, send-respawn, sweep) reads the Thread's stored
/// choice through it, so a new fact travels to all of them at once (#25).
pub struct SpawnRequest<'a> {
    /// The Thread it serves: told to the agent (`FERRITE_THREAD_ID`), so it
    /// can name its own Thread to the API (ADR 0027).
    pub thread: ThreadId,
    pub provider: Provider,
    /// The Thread's chosen model, verbatim as the provider announced it.
    /// `None` is the provider's own default.
    pub model: Option<&'a str>,
    /// The Thread's chosen reasoning effort, in the provider's own word.
    /// `None` is the operator's default for the provider (Settings), which
    /// the spawner applies — or the provider's own when there is none.
    pub effort: Option<&'a str>,
    /// The provider-native id of a Thread being revived, which the provider
    /// reloads its own history from.
    pub resume: Option<&'a str>,
    /// The permission mode the Thread's last agent ran in, in the
    /// provider's own word (ADR 0032). `None` is the operator's default for
    /// the provider (Settings).
    pub permission_mode: Option<&'a str>,
    /// The Thread's title, for the provider's own session list. `None`
    /// until the operator names the Thread.
    pub name: Option<&'a str>,
    /// The Thread's workspace binding resolved to the directory the Session
    /// works in; `None` only for a Thread from before bindings were
    /// recorded.
    pub cwd: Option<&'a Path>,
    /// Other roots in the same Project. Providers expose these alongside
    /// `cwd`; they never change which directory the Session starts in.
    pub additional_directories: Vec<PathBuf>,
    /// What the Thread was started with beyond Ferrite's own
    /// (`thread.start`).
    pub launch: AgentLaunch<'a>,
}

/// What a Session starts with beyond Ferrite's own (`thread.start --args
/// --env --launcher`, ADR 0030, ADR 0031).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentLaunch<'a> {
    /// Extra arguments for the provider's CLI, after Ferrite's own: what
    /// the operator's safety settings add, a model flag.
    pub args: &'a [String],
    /// Set in the agent's environment, under Ferrite's own.
    pub env: &'a [(String, String)],
    /// The program the CLI is run through, and its arguments.
    pub launcher: &'a [String],
}

fn project_additional_directories(
    registry: &Registry,
    project: Option<ProjectId>,
    cwd: Option<&Path>,
) -> Vec<PathBuf> {
    let cwd = cwd.map(|cwd| std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf()));
    project
        .and_then(|project| registry.project(project))
        .into_iter()
        .flat_map(|project| project.directories())
        .filter(|directory| !cwd.as_deref().is_some_and(|cwd| cwd.starts_with(directory)))
        .map(Path::to_path_buf)
        .collect()
}

/// How a Session is started. Injected so the cockpit can be driven with
/// scripted Sessions in tests — nothing below this line spawns a process.
pub trait Spawner {
    fn discover_sessions(
        &mut self,
        _roots: Vec<(Provider, PathBuf)>,
        _cap: usize,
    ) -> Option<Receiver<io::Result<Vec<crate::import::Candidate>>>> {
        None
    }

    /// Metadata only: discover the selected workspace's effective commands.
    fn discover_commands(
        &mut self,
        _provider: Provider,
        _cwd: &std::path::Path,
    ) -> Option<crate::providers::commands::Discovery> {
        None
    }

    /// Discover provider menus off-thread, without creating a Session or
    /// sending a prompt. Unsupported adapters keep the fallback catalog.
    fn discover_models(&mut self) -> Option<Receiver<(Provider, Vec<ModelInfo>)>> {
        None
    }

    /// Optional background prediction. Scripted/demo adapters do nothing;
    /// production delegates to the provider one-shot module.
    fn suggest(&mut self, _request: crate::suggest::Request, _replies: Sender<Suggestion>) {}

    fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn Session>>;

    /// A ready adapter completes synchronously; production overrides this
    /// to start off-thread. Only readiness, never queue admission, is success.
    fn start(&mut self, request: SpawnRequest) -> io::Result<SessionLifecycle> {
        self.spawn(request).map(SessionLifecycle::ready)
    }

    /// `start`, once `prepare` has run (ADR 0018). A ready adapter runs it
    /// here, on the caller's thread; production runs it first on the
    /// thread it starts the CLI on, so the caller waits for neither.
    fn start_after(
        &mut self,
        prepare: Preparation,
        request: SpawnRequest,
    ) -> io::Result<SessionLifecycle> {
        prepare()?;
        self.start(request)
    }
}

/// Work a Session's start waits on and its caller should not: a launch's
/// stagger, and the git calls that make a revived Thread's worktree real.
pub type Preparation = Box<dyn FnOnce() -> io::Result<()> + Send>;

/// Resident memory per Session, injected. The cockpit never shells out for
/// this itself: a test drives the watchdog by saying what the number is.
pub trait RssSampler {
    /// Bytes resident for the Session serving `thread`, or None when it
    /// cannot be measured. `pid` is the Session's process where it has one.
    fn sample(&mut self, thread: ThreadId, pid: Option<u32>) -> Option<u64>;
}

/// A Session the watchdog replaced, for the operator and the app's log.
#[derive(Debug, Clone, PartialEq)]
pub struct Restart {
    pub thread: ThreadId,
    /// What it had grown to when the watchdog acted.
    pub rss: u64,
}

/// What one frame changed for one Pane. Threads that produced nothing are
/// absent: a quiet Pane costs the renderer nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneUpdate {
    pub thread: ThreadId,
    /// Exactly the Blocks whose content moved.
    pub dirty: Vec<BlockId>,
    /// Blocks that fell off the far end and no longer exist.
    pub evicted: Vec<BlockId>,
    /// Child changes never enter the Main-only compatibility lists above.
    pub subjects: Vec<SubjectUpdate>,
    pub activity_changed: bool,
    pub redirects: Vec<(AgentKey, AgentKey)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubjectUpdate {
    pub subject: Subject,
    pub dirty: Vec<BlockId>,
    pub evicted: Vec<BlockId>,
    pub content_changed: bool,
}

impl PaneUpdate {
    pub(crate) fn new(thread: ThreadId) -> Self {
        Self {
            thread,
            dirty: Vec::new(),
            evicted: Vec::new(),
            subjects: Vec::new(),
            activity_changed: false,
            redirects: Vec::new(),
        }
    }

    pub(crate) fn absorb(&mut self, applied: ActivityUpdate, content_changed: bool) {
        self.activity_changed |=
            !applied.changed.is_empty() || applied.attention_changed || applied.order_changed;
        self.redirects.extend(applied.redirects);
        for subject in applied.changed {
            if !self.subjects.iter().any(|change| change.subject == subject) {
                self.subjects.push(SubjectUpdate {
                    subject,
                    dirty: Vec::new(),
                    evicted: Vec::new(),
                    content_changed: false,
                });
            }
        }
        for (subject, blocks) in applied.blocks {
            if subject == Subject::Main {
                self.dirty.extend(blocks.dirty.iter().copied());
                self.evicted.extend(blocks.evicted.iter().copied());
            }
            let index = self
                .subjects
                .iter()
                .position(|change| change.subject == subject)
                .unwrap_or_else(|| {
                    self.subjects.push(SubjectUpdate {
                        subject,
                        dirty: Vec::new(),
                        evicted: Vec::new(),
                        content_changed: false,
                    });
                    self.subjects.len() - 1
                });
            let change = &mut self.subjects[index];
            change.content_changed |=
                content_changed && (!blocks.dirty.is_empty() || !blocks.evicted.is_empty());
            change.dirty.extend(blocks.dirty);
            change.evicted.extend(blocks.evicted);
        }
    }
}

/// A provider and the model to serve it with — what the #25 picker picks,
/// whole. `None` is the provider's own default, which is also what every
/// Thread starts on.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderChoice {
    pub provider: Provider,
    pub model: Option<String>,
}

/// Re-aiming a Thread's provider, model or effort failed — and changed
/// nothing: the old Session keeps serving and the header on disk is
/// untouched.
#[derive(Debug)]
pub enum ProvisionError {
    /// The first prompt has gone out and the Thread is parked: a switch
    /// needs the live conversation to hand over, so it waits for a revive.
    /// A live Thread is never locked — its provider switches with the
    /// earlier conversation carried as context (#25, #29).
    Locked,
    /// A turn is running: changing the Session now would cut it short.
    Busy,
    /// The last model or effort change waits on the provider's answer.
    Tuning,
    /// The new provider's CLI would not spawn. The words are the
    /// provider's own.
    Spawn(io::Error),
    /// The live Session refused an in-place setting change.
    Tune(io::Error),
    /// The durable header rewrite failed.
    Store(LoadError),
}

impl std::fmt::Display for ProvisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProvisionError::Locked => {
                write!(
                    f,
                    "the first prompt was sent; reopen the thread to switch its provider"
                )
            }
            ProvisionError::Busy => write!(f, "a turn is running; change the model when it ends"),
            ProvisionError::Tuning => write!(f, "the last change is waiting on the provider"),
            ProvisionError::Spawn(e) => write!(f, "{e}"),
            ProvisionError::Tune(e) => write!(f, "{e}"),
            ProvisionError::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ProvisionError {}

/// Deleting a Thread failed — and destroyed nothing.
#[derive(Debug)]
pub enum DeleteError {
    /// The Thread's worktree holds uncommitted work ("clean" exactly as
    /// `git status` defines it). Deleting would destroy that work, so the
    /// operator commits, stashes, or discards it first, then deletes.
    DirtyWorktree {
        path: std::path::PathBuf,
    },
    /// The Thread could not be loaded to learn its binding.
    Load(LoadError),
    /// A git operation failed.
    Git(workspace::GitError),
    Io(io::Error),
}

impl std::fmt::Display for DeleteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeleteError::DirtyWorktree { path } => write!(
                f,
                "worktree at {} has uncommitted work; commit, stash or discard it first",
                path.display()
            ),
            DeleteError::Load(e) => write!(f, "could not load the thread: {e}"),
            DeleteError::Git(e) => write!(f, "{e}"),
            DeleteError::Io(e) => write!(f, "io error deleting thread: {e}"),
        }
    }
}

impl std::error::Error for DeleteError {}

#[derive(Debug)]
pub enum ReviveGroupError {
    MissingGroup,
    Member { thread: ThreadId, error: LoadError },
    Rollback { thread: ThreadId, error: io::Error },
}

impl std::fmt::Display for ReviveGroupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingGroup => write!(f, "group no longer exists"),
            Self::Member { thread, error } => {
                write!(f, "could not revive thread {thread}: {error}")
            }
            Self::Rollback { thread, error } => {
                write!(f, "could not roll back revived thread {thread}: {error}")
            }
        }
    }
}

impl std::error::Error for ReviveGroupError {}

enum ReplacementKind {
    Fresh,
    Handover,
    Retune,
}

/// What waits for a Thread's Session to take it (ADR 0032).
#[derive(Default)]
struct Awaiting {
    /// Prompts the Session cannot take yet, oldest first: it is starting,
    /// or it cannot queue yet (Claude, before its first turn starts). Each
    /// goes out as a turn when none runs, or into the provider's queue once
    /// it takes them; anything written meanwhile waits behind them.
    prompts: VecDeque<String>,
    /// Controls asked while it starts, in order: run once it is up.
    controls: Vec<crate::SessionControl>,
    /// The file search asked while it starts, the latest only: asked of it
    /// once it is up.
    search: Option<(String, crate::providers::FileSearchReply)>,
}

impl Awaiting {
    /// Whether anything besides prompts waits.
    fn asks(&self) -> bool {
        !self.controls.is_empty() || self.search.is_some()
    }
}

struct Replacement {
    /// None for a Thread with no Session (ADR 0032): the change starts no
    /// agent, and the next start is made with it.
    session: Option<SessionLifecycle>,
    provider: Provider,
    model: Option<String>,
    effort: Option<String>,
    kind: ReplacementKind,
    /// A handover's exchanges, once their read has started: the commit
    /// waits for them, and the old Session's events wait with it.
    exchanges: Option<crate::store::ExchangeRead>,
}

/// A model or effort change the live Session has asked its provider for.
/// The Thread's choice and its header move only once the provider accepts
/// it (`settle_tuning`).
struct Tuning {
    change: Tune,
    ack: SettingAck,
    asked: Instant,
}

enum Tune {
    Model(Option<String>),
    Effort(Option<String>),
}

impl Tune {
    /// What the change is to, as the transcript names it.
    fn noun(&self) -> &'static str {
        match self {
            Tune::Model(_) => "model",
            Tune::Effort(_) => "effort",
        }
    }
}

/// How long a provider has to accept a setting change before it counts as
/// refused.
const TUNE_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a Session may stay at rest before it ends (ADR 0032).
const END_AFTER: Duration = Duration::from_secs(10 * 60);

struct PendingBootstrap {
    prompt: String,
    group: Option<GroupId>,
    new_group_with: Option<ThreadId>,
    draft: Option<DraftId>,
}

/// A deferred first send settled by `pump`. The draft and submitted text
/// identify the original Pane even if the operator has since moved focus.
pub struct BootstrapResult {
    pub draft: Option<DraftId>,
    pub prompt: String,
    pub result: io::Result<Bootstrapped>,
}

/// A Thread's Activity, and every input applied to it since the Engine
/// last published: what its clients' mirrors apply, in the same order.
/// Each input's instant is put on the millisecond grid of the Engine's boot,
/// the precision the wire carries, so a mirror applies exactly it.
struct Journaled {
    activity: Activity,
    boot: Instant,
    applied: Vec<ActivityInput>,
    /// The Activity was replaced since the last publish: mirrors start over.
    reset: bool,
}

impl Journaled {
    /// A new Activity: its mirrors start from nothing.
    fn new(boot: Instant) -> Self {
        Self {
            activity: Activity::default(),
            boot,
            applied: Vec::new(),
            reset: true,
        }
    }

    fn apply(&mut self, mut input: ActivityInput) -> ActivityUpdate {
        if let ActivityInput::Main { at, .. }
        | ActivityInput::Observe { at, .. }
        | ActivityInput::Answered { at, .. } = &mut input
        {
            *at = crate::protocol::instant_at(crate::protocol::ms_since(*at, self.boot), self.boot);
        }
        let update = self.activity.apply(input.clone());
        // The highlighter is each side's own: a mirror drains its own.
        if !matches!(input, ActivityInput::DrainHighlights) {
            self.applied.push(input);
        }
        update
    }

    fn view(&self) -> ActivityView<'_> {
        self.activity.view()
    }

    /// Forget the Activity without telling the mirrors: a Thread let go to
    /// its digest (`Engine::let_go_unused`).
    fn forget(&mut self) {
        self.activity = Activity::default();
        self.applied.clear();
        self.reset = false;
    }

    /// What was applied since the last publish is not news: an Activity
    /// read back as every window was already told it (`Engine::hold`).
    fn settle(&mut self) {
        self.applied.clear();
    }

    /// A fresh Activity, for a Session that starts the conversation over.
    fn replace(&mut self) {
        self.activity = Activity::default();
        self.applied.clear();
        self.reset = true;
    }
}

struct Thread {
    activity: Journaled,
    generation: u64,
    store_error: Option<String>,
    history: HashMap<AgentKey, history::Pending>,
    history_errors: HashMap<AgentKey, String>,
    /// The live Session, or None while the Thread is not in use (ADR 0032).
    session: Option<SessionLifecycle>,
    replacement: Option<Replacement>,
    writer: ThreadWriter,
    provider: Provider,
    /// The model this Thread chose before its first prompt (#25), verbatim
    /// as the provider announced it. `None` — most Threads — is the
    /// provider's default. Every respawn reads it, so the choice holds for
    /// the Thread's whole life.
    model: Option<String>,
    /// The reasoning effort this Thread chose, in the provider's own word.
    /// `None` is the operator's default for the provider. Read by every
    /// respawn, like the model.
    effort: Option<String>,
    title: Option<String>,
    /// What waits for the Session to take it.
    awaiting: Awaiting,
    /// The Session was started for use (ADR 0032) rather than asked for:
    /// it shows as starting only once a prompt waits for it.
    woken: bool,
    /// Since when its Session has been at rest, or last used at rest;
    /// `None` while it is not at rest.
    settled_at: Option<Instant>,
    /// Its Session said it holds work it will start itself
    /// (`SessionEvent::ScheduledWork`).
    scheduled_work: bool,
    /// Whether the Engine holds its Activity. One with no Session that no
    /// window watches is let go to its digest, and read back from its log
    /// when used (ADR 0032).
    held: bool,
    /// When it was last read back: it is not let go again within
    /// `HOLD_AT_LEAST`.
    read_back_at: Option<Instant>,
    native_queue: queue::Queue,
    /// An interrupt was sent while prompts were mirrored: when the turn
    /// ends, everything held goes out as one prompt — Escape means "stop,
    /// and do what I queued now". Claude's CLI does this itself; Codex
    /// drops un-consumed steers on interrupt, so Ferrite resends them.
    resend_held_on_turn_end: bool,
    prompt_history: PromptHistory,
    /// The provider-native id a replacement Session resumes from — the latest
    /// the provider announced. Held here rather than read back from the log,
    /// which has not necessarily flushed when the watchdog acts.
    resume: Option<String>,
    /// The checkout this Thread works in — what the Pane's chrome shows and
    /// what every replacement Session spawns into. `None` only for a Thread
    /// from before bindings were recorded.
    workspace: Option<WorkspaceBinding>,
    /// The git repo inside the binding where work should happen (#24).
    /// `None` — today's behavior — means work in the binding itself. Never
    /// changes under a live Session: the setter ends the Session first.
    session_project_root: Option<PathBuf>,
    /// What Main has been seen doing that bears on where it works: the
    /// binding follows it (`workspace::follow`).
    follow: Follow,
    /// Armed whenever a Session is constructed or attached (open, revive,
    /// send-respawn, sweep-respawn); taken by the first prompt that goes
    /// out, which is the one that carries the hidden session-context
    /// preface when a root is set.
    preface_pending: bool,
    /// The earlier conversation, digested, owed to a Provider this Thread
    /// switched to after its first prompt: the next prompt out carries it
    /// once, ahead of the operator's text. Set by the switch (and by a
    /// revive that finds the switch's carry undelivered), taken by the
    /// first prompt that goes out.
    carry: Option<String>,
    /// What its agent last said of itself (ADR 0032): the command menu
    /// (#23), models (#25), permission mode and modes, controls and MCP
    /// servers. Offered while the Thread has no Session as well as while it
    /// has one; forgotten when it changes Provider.
    announced: announced::Announced,
    /// The lock every pre-prompt control reads (#25, #29): armed by the
    /// first operator prompt that goes out, and on revive when the replayed
    /// history holds one. Never disarmed — nothing re-aims a Thread the
    /// operator has spoken in.
    first_prompt_sent: bool,
    /// The follow-up predicted for this Thread's last response (`suggest`),
    /// or None while nothing has been predicted — before the first turn ends,
    /// for the ~seconds a prediction is in flight, and whenever the one that
    /// came back was refused. Never persisted: it describes a response, and
    /// a later completed turn requests a fresh prediction.
    suggestion: Option<String>,
    suggestion_revision: u64,
    /// A model or effort change waiting on the provider. Session state: a
    /// replacement Session drops it, since its answer would be the old
    /// Session's.
    tuning: Option<Tuning>,
    /// The extra CLI arguments, environment and launcher the Thread was
    /// started with, and the Provider they were written for
    /// (`thread.start`). Every Session of that Provider starts with them;
    /// saved, so a revive does too (`launch_args`).
    agent_args: Option<launch_args::LaunchArgs>,
}

impl Thread {
    /// What a Session of `provider` starts with beyond Ferrite's own: what
    /// the Thread was started with, for its Provider only.
    fn agent_launch(&self, provider: Provider) -> AgentLaunch<'_> {
        launch_args::for_provider(self.agent_args.as_ref(), provider)
    }

    /// A Thread ready to serve: its own Lexer, a fresh Transcript, and its
    /// Session when it starts with one. Revival replays history into the
    /// Transcript afterwards.
    fn fresh(
        boot: Instant,
        session: Option<SessionLifecycle>,
        writer: ThreadWriter,
        provider: Provider,
        model: Option<String>,
        effort: Option<String>,
        resume: Option<String>,
        workspace: Option<WorkspaceBinding>,
        session_project_root: Option<PathBuf>,
        dir: PathBuf,
    ) -> Self {
        let generation = next_generation();
        let mut activity = Journaled::new(boot);
        activity.apply(ActivityInput::Connect { generation });
        let mut state = Self {
            activity,
            generation,
            store_error: None,
            history: HashMap::new(),
            history_errors: HashMap::new(),
            session,
            replacement: None,
            writer,
            provider,
            model,
            effort,
            title: None,
            awaiting: Awaiting::default(),
            woken: false,
            settled_at: None,
            scheduled_work: false,
            held: true,
            read_back_at: None,
            native_queue: queue::Queue::open(dir.join("queue.json")),
            resend_held_on_turn_end: false,
            prompt_history: PromptHistory::new(Vec::new()),
            resume,
            workspace,
            session_project_root,
            follow: Follow::default(),
            preface_pending: true,
            carry: None,
            announced: announced::Announced::open(dir.join("announced.json")),
            first_prompt_sent: false,
            suggestion: None,
            suggestion_revision: 0,
            tuning: None,
            agent_args: None,
        };
        if let Some(notice) = state.native_queue.take_unreadable() {
            state.apply(Input::Notice(notice));
        }
        state
    }
}

// A park/revive creates a new Activity; generation must still distinguish old UI handles.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
fn next_generation() -> u64 {
    NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
}

/// What every window shares, as the Engine last told its clients.
#[derive(Default)]
struct PublishedShared {
    groups: Option<serde_json::Value>,
    registry: Option<serde_json::Value>,
    notices: Option<Vec<protocol::WireNotice>>,
    providers: Option<Vec<protocol::ProviderState>>,
    parked: Option<Vec<protocol::ParkedThread>>,
}

/// The `parked` message (ADR 0026): what a window on a machine lists of
/// the Threads not open. Read only while such a window is attached, and
/// off the pump thread: a row's reads wait for a park's final mark, and a
/// log from before schema 13 is replayed once for its counts.
#[derive(Default)]
struct ParkedListing {
    /// A window that lists them is attached (`list_parked`).
    wanted: bool,
    /// Something may have changed a row since the last read began.
    stale: bool,
    reading: Option<Receiver<Vec<protocol::ParkedThread>>>,
}

/// `now`, when it is not what was last published: it is from here on.
fn changed<T: Clone + PartialEq>(published: &mut Option<T>, now: T) -> Option<T> {
    if published.as_ref() == Some(&now) {
        return None;
    }
    *published = Some(now.clone());
    Some(now)
}

/// What a Thread's clients were last told.
#[derive(Default)]
struct Published {
    /// The Thread's last input this boot, counted across every reset and
    /// reopen, so a `seq` names one moment of one Activity.
    seq: u64,
    state: Option<protocol::ThreadState>,
    digest: Option<Digest>,
    /// When the digest last refolded: a stream refolds it once per
    /// `DIGEST_REFOLD`.
    digest_at: Option<Instant>,
    /// Inputs applied since the digest last refolded.
    digest_owed: bool,
}

/// How long a Thread read back from its log is held before it may be let
/// go again, so a Thread read often reads its log at most once a minute.
const HOLD_AT_LEAST: Duration = Duration::from_secs(60);

/// How often a streaming Thread's digest may refold: four times a second.
/// The fold walks every Block the Thread holds (and its Subagents', for the
/// changed files) while the pump drains a stream up to 125 times a second;
/// a Wall card a quarter second behind is still ahead of the eye.
const DIGEST_REFOLD: Duration = Duration::from_millis(250);

impl Thread {
    /// Everything a window reads of this Thread that is not its Activity.
    /// What a window reads of this Thread beyond its Activity. Whether a
    /// Draft is starting it is the Engine's to say (`Engine::thread_state`).
    fn wire_state(&self, id: ThreadId) -> protocol::ThreadState {
        let announced = &self.announced;
        protocol::ThreadState {
            thread: id,
            generation: self.generation,
            provider: self.provider,
            model: self.model.clone(),
            effort: self.effort.clone(),
            title: self.title.clone(),
            workspace: self.workspace.as_ref().map(Into::into),
            session_project_root: self.session_project_root.clone(),
            first_prompt_sent: self.first_prompt_sent,
            has_prompt_history: self.prompt_history.has_entries(),
            starting: self.starting(),
            working: self.busy(),
            tuning: self.tuning.is_some(),
            native_queue_pending: self.native_queue.pending(),
            queued: self
                .awaiting
                .prompts
                .iter()
                .rev()
                .cloned()
                .chain(
                    self.native_queue
                        .items
                        .iter()
                        .rev()
                        .map(|item| item.text.clone()),
                )
                .collect(),
            suggestion: self.suggestion.clone(),
            commands: announced.commands().iter().map(Into::into).collect(),
            models: announced.models().to_vec(),
            permission_mode: announced.permission_mode().map(str::to_string),
            permission_modes: announced
                .permission_modes()
                .iter()
                .map(|mode| protocol::WirePermissionMode {
                    value: mode.value.clone(),
                    label: mode.label.clone(),
                })
                .collect(),
            controls: announced.controls().iter().map(Into::into).collect(),
            store_error: self.store_error.clone(),
            for_draft: false,
            recovered: self
                .native_queue
                .recovered
                .iter()
                .map(|(text, prepend)| protocol::WireRecovered {
                    text: text.clone(),
                    prepend: *prepend,
                })
                .collect(),
            wants_listing: self.follow.wants_listing(),
            history_errors: {
                let mut errors: Vec<_> = self
                    .history_errors
                    .iter()
                    .map(|(key, error)| protocol::WireHistoryError {
                        agent: key.as_str().to_string(),
                        error: error.clone(),
                    })
                    .collect();
                errors.sort_by(|a, b| a.agent.cmp(&b.agent));
                errors
            },
        }
    }

    fn apply(&mut self, input: Input) -> Update {
        let applied = self.activity.apply(ActivityInput::Main {
            input,
            at: crate::clock::instant(),
        });
        let mut update = Update::default();
        for (subject, changed) in applied.blocks {
            if subject == Subject::Main {
                update.dirty.extend(changed.dirty);
                update.evicted.extend(changed.evicted);
                update.boundary = changed.boundary.or(update.boundary);
            }
        }
        update
    }

    fn transcript(&self) -> &Transcript {
        self.activity.view().main().transcript()
    }
    fn busy(&self) -> bool {
        self.activity.view().main().busy()
    }

    /// Whether a window shows the Session starting: a replacement, or a
    /// start it was asked for. A wake shows only once a prompt waits on it.
    fn starting(&self) -> bool {
        self.replacement.is_some()
            || self
                .session
                .as_ref()
                .is_some_and(SessionLifecycle::is_starting)
                && (!self.woken || !self.awaiting.prompts.is_empty())
    }

    /// End a Session started unseen that nothing waits on yet: nothing of
    /// it reached a window, so the generation stands. Answers whether it
    /// did.
    fn drop_unseen_start(&mut self) -> bool {
        let unseen = self.woken
            && self.replacement.is_none()
            && self
                .session
                .as_ref()
                .is_some_and(SessionLifecycle::is_starting)
            && self.awaiting.prompts.is_empty()
            && !self.awaiting.asks();
        if unseen {
            self.end_session();
        }
        unseen
    }

    /// The Session, once it can take what is asked of it: started, and not
    /// being replaced.
    fn ready_session(&mut self) -> Option<&mut (dyn Session + 'static)> {
        if self.replacement.is_some() {
            return None;
        }
        self.session
            .as_mut()
            .and_then(SessionLifecycle::session_mut)
    }

    /// Its agent could not start: prompts that waited for it go back to the
    /// Composer; the controls and file search that did are dropped.
    fn give_up_awaiting(&mut self) {
        let waited = std::mem::take(&mut self.awaiting);
        let prompts = waited.prompts.into_iter().map(|text| (text, false));
        self.native_queue.recovered.extend(prompts);
    }

    /// A prompt that could not be delivered goes back to the Composer, so
    /// the operator can send it again; nothing retries it unasked.
    fn undelivered(&mut self, text: String, error: io::Error) -> Update {
        self.native_queue.recovered.push((text, false));
        self.apply(Input::Notice(format!("send failed: {error}")))
    }

    /// Whether a prompt sent now would wait behind a turn: one runs, or
    /// the provider still holds or takes queued prompts.
    fn turn_ahead(&self) -> bool {
        self.busy() || self.activity.view().main_operator_turn() || self.native_queue.pending()
    }

    /// Whether work runs behind Main: its Subagents, or its transcript's
    /// background tasks.
    fn background_work(&self) -> bool {
        self.activity.view().working_descendants() > 0
            || self.transcript().progress().working_background() > 0
    }

    /// At rest: no turn, no starting or replacement Session, no change
    /// waiting on the provider, no working Subagent or background task, no
    /// Decision and nothing in the provider's queue. What a Session may be
    /// replaced at without cutting anything off.
    fn settled(&self) -> bool {
        let view = self.activity.view();
        !self
            .session
            .as_ref()
            .is_some_and(SessionLifecycle::is_starting)
            && self.replacement.is_none()
            && self.tuning.is_none()
            && !self.busy()
            && !view.main_operator_turn()
            && !self.background_work()
            && view.decisions().is_empty()
            && !self.native_queue.pending()
    }

    /// Its Activity as its log says it, with no Session running: what the
    /// last one announced still stands (ADR 0032).
    fn read_back(&mut self, inputs: Vec<ActivityInput>) {
        self.activity.apply(ActivityInput::Disconnect);
        for input in inputs {
            self.activity.apply(input);
        }
        self.apply(Input::Revived);
        let servers = self.announced.mcp_servers().to_vec();
        if !servers.is_empty() {
            self.apply(Input::Event(SessionEvent::McpServers { servers }));
        }
    }

    /// `session` serves it from now on. A fresh Session knows nothing:
    /// its first prompt carries the session-context preface again, and it
    /// has scheduled nothing yet.
    fn begin_session(&mut self, session: SessionLifecycle) {
        self.session = Some(session);
        self.preface_pending = true;
        self.scheduled_work = false;
    }

    /// Its Session ends; the Thread keeps everything else.
    fn end_session(&mut self) {
        self.session = None;
    }

    /// At rest, and nothing waits for a Session to take it: what the
    /// Server may stop at, and what a Session may end at.
    fn at_rest(&self) -> bool {
        self.settled() && self.awaiting.prompts.is_empty()
    }

    /// Asked of its Session: it is in use, so its time at rest counts
    /// again from now.
    fn used(&mut self) {
        self.settled_at = Some(crate::clock::instant());
    }

    /// Whether its Session has outlived its use at `now` (ADR 0032): at
    /// rest for `END_AFTER`, or at all on a `stale` CLI, and holding no
    /// work it will start itself. Starts the count on the first call at
    /// rest.
    fn outlived_use(&mut self, now: Instant, stale: bool) -> bool {
        if self.session.is_none() || self.scheduled_work || !self.at_rest() {
            self.settled_at = None;
            return false;
        }
        let since = *self.settled_at.get_or_insert(now);
        stale || now.saturating_duration_since(since) >= END_AFTER
    }

    /// Whether the model and effort can change now: not mid-turn, not
    /// while the Session starts or is being replaced, and not while the
    /// last change waits on the provider.
    fn tunable(&self) -> Result<(), ProvisionError> {
        if self.tuning.is_some() {
            return Err(ProvisionError::Tuning);
        }
        if self.busy()
            || self.replacement.is_some()
            || self
                .session
                .as_ref()
                .is_some_and(SessionLifecycle::is_starting)
        {
            return Err(ProvisionError::Busy);
        }
        Ok(())
    }
    fn invalidate_suggestion(&mut self) {
        self.suggestion = None;
        self.suggestion_revision = self.suggestion_revision.wrapping_add(1);
    }

    fn replace_generation(&mut self) {
        if let Some(notice) = self
            .native_queue
            .disconnect(self.provider == Provider::Codex)
        {
            self.apply(Input::Notice(notice));
        }
        if let Some(tuning) = self.tuning.take() {
            self.apply(Input::Notice(format!(
                "{} unchanged: the session restarted before accepting it",
                tuning.change.noun()
            )));
        }
        self.invalidate_suggestion();
        self.history.clear();
        self.history_errors.clear();
        self.generation = next_generation();
        self.activity.apply(ActivityInput::Connect {
            generation: self.generation,
        });
    }

    fn report_store_error(&mut self, error: io::Error) {
        let message = error.to_string();
        if self.store_error.as_ref() != Some(&message) {
            self.apply(Input::Notice(format!(
                "history could not be saved: {message}"
            )));
        }
        self.store_error = Some(message);
    }
}

pub struct Engine {
    threads: BTreeMap<ThreadId, Thread>,
    store: Store,
    /// The workspace registry (#29): registered projects and their
    /// worktrees, living beside the Thread logs. The selector reads it; the
    /// bootstrap places worktrees through it.
    registry: Registry,
    groups: Groups,
    /// The open Threads, in the order they opened: the shared half of the
    /// Roster, which each window follows (`open_threads`).
    open: Vec<ThreadId>,
    /// When this Engine started: the zero of every `at_ms` it publishes.
    boot: Instant,
    /// This boot's identity (`boot_id`).
    boot_id: String,
    /// What the Engine has told its clients and not yet handed over
    /// (`take_messages`), and the state each Thread's last `thread`
    /// message carried.
    outbox: Vec<ServerMessage>,
    published: BTreeMap<ThreadId, Published>,
    published_open: Vec<ThreadId>,
    published_shared: PublishedShared,
    parked_listing: ParkedListing,
    revision: u64,
    spawner: Box<dyn Spawner>,
    model_discovery: Option<Receiver<(Provider, Vec<ModelInfo>)>>,
    model_cache: crate::providers::models::ModelCache,
    /// The last subscription windows each provider reported — an account
    /// fact the transcript log deliberately does not replay.
    limit_cache: crate::providers::limits::LimitCache,
    /// The use, in percent, at which a window rising past it warns the
    /// operator (`Settings::allowance_warnings`); and the warnings due.
    allowance_thresholds: Vec<u8>,
    allowance_warnings: Vec<crate::providers::limits::AllowanceWarning>,
    models_changed: bool,
    bootstraps: HashMap<ThreadId, PendingBootstrap>,
    bootstrap_results: Vec<BootstrapResult>,
    history_loader: Option<history::Loader>,
    /// What every Thread has to tell the operator once they look away:
    /// the Notices, and the per-Thread deferrals behind them.
    notifications: Notifications,
    /// The Subject the window has actually selected in each open Thread.
    /// Main is implicit until the renderer records a child selection.
    visible_subjects: BTreeMap<ThreadId, Subject>,
    /// The Threads a Server's windows watch; `None` with no Server telling
    /// (`set_watched`): every Thread is then held.
    watched: Option<BTreeSet<ThreadId>>,
    sampler: Option<Box<dyn RssSampler>>,
    /// Bytes one Session may hold before the watchdog replaces it.
    limit: u64,
    /// Where finished follow-up predictions come back. One channel for every
    /// Thread: the reply names the Thread and the generation it was asked
    /// for, so a Pane that has since been parked or re-aimed drops it.
    suggestions: (Sender<Suggestion>, Receiver<Suggestion>),
    /// Whether completed turns may ask for and show predicted follow-ups.
    suggestions_enabled: bool,
    /// Threads whose Session runs a CLI since replaced on disk. Each is
    /// restarted on its own conversation at the first frame it is idle.
    stale_sessions: BTreeSet<ThreadId>,
    /// What the windows said of themselves, for the API (ADR 0030).
    windows: windows::Windows,
    /// Each open Thread's Attention changes, counted for the API.
    attention_count: windows::AttentionCount,
    /// The live Sessions as reviewr's turn tracking last heard of them,
    /// and this Engine's feed into it.
    turn_agents: Vec<ferrite_reviewr::ferrite::Agent>,
    turns: ferrite_reviewr::ferrite::TurnSource,
    /// The worktrees removed from disk and not yet handed over
    /// (`take_removed_worktrees`).
    removed_worktrees: Vec<PathBuf>,
    #[cfg(test)]
    refuse_park: std::collections::HashSet<ThreadId>,
}

/// A boot's identity, unlike every other boot's.
fn boot_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}-{:x}", std::process::id(), nanos)
}

/// How long quitting waits for the drive to take every log's final sync.
/// The writes themselves are always waited for.
const QUIT_SYNC_DEADLINE: Duration = Duration::from_millis(500);

impl Engine {
    pub fn try_new(store: Store, mut spawner: Box<dyn Spawner>) -> io::Result<Self> {
        let registry = Registry::open(store.dir())?;
        let groups = Groups::load(store.dir())?;
        let model_discovery = spawner.discover_models();
        let model_cache = crate::providers::models::ModelCache::load(store.dir());
        let limit_cache = crate::providers::limits::LimitCache::load(store.dir());
        Ok(Self {
            threads: BTreeMap::new(),
            store,
            registry,
            groups,
            open: Vec::new(),
            boot: crate::clock::instant(),
            boot_id: boot_id(),
            outbox: Vec::new(),
            published: BTreeMap::new(),
            published_open: Vec::new(),
            published_shared: PublishedShared::default(),
            parked_listing: ParkedListing::default(),
            revision: 0,
            spawner,
            model_discovery,
            model_cache,
            limit_cache,
            allowance_thresholds: Vec::new(),
            allowance_warnings: Vec::new(),
            models_changed: false,
            bootstraps: HashMap::new(),
            bootstrap_results: Vec::new(),
            history_loader: None,
            notifications: Notifications::default(),
            visible_subjects: BTreeMap::new(),
            watched: None,
            sampler: None,
            limit: u64::MAX,
            suggestions: channel(),
            suggestions_enabled: true,
            stale_sessions: BTreeSet::new(),
            windows: Default::default(),
            attention_count: Default::default(),
            turn_agents: Vec::new(),
            turns: Default::default(),
            removed_worktrees: Vec::new(),
            #[cfg(test)]
            refuse_park: std::collections::HashSet::new(),
        })
    }

    /// Test and embedding convenience. The application uses `try_new` so a
    /// protected or incompatible registry is reported instead of replaced.
    pub fn new(store: Store, spawner: Box<dyn Spawner>) -> Self {
        Self::try_new(store, spawner).expect("the workspace registry opens")
    }

    /// The workspace registry, for the selector's rows (#29). Read-only:
    /// registration goes through `register_project`, and worktree entries
    /// are the bootstrap's own bookkeeping.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn groups(&self) -> &Groups {
        &self.groups
    }

    /// A Group change (#28). What it does to each window's view is the
    /// window's (`Cockpit::apply_group`).
    pub fn apply_group(&mut self, change: GroupChange) -> Result<Applied, ApplyError> {
        self.groups.apply(change)
    }

    pub fn plan_group_drop(&self, drag: Drag, target: DropTarget) -> Plan {
        self.groups.preview_drop(drag, target)
    }

    /// Register a project root (idempotent, canonicalized) — the selector's
    /// type-a-path row and the launch seed both land here.
    pub fn register_project(&mut self, root: &Path) -> io::Result<ProjectId> {
        self.registry.register(root)
    }

    pub fn register_project_directories(&mut self, roots: &[PathBuf]) -> io::Result<ProjectId> {
        self.registry.register_directories(roots)
    }

    pub fn add_project_directories(
        &mut self,
        project: ProjectId,
        roots: &[PathBuf],
    ) -> io::Result<()> {
        self.registry.add_directories(project, roots)
    }

    pub fn replace_project_directory(
        &mut self,
        project: ProjectId,
        index: usize,
        root: &Path,
    ) -> io::Result<()> {
        self.registry.replace_directory(project, index, root)
    }

    pub fn remove_project_directory(&mut self, project: ProjectId, index: usize) -> io::Result<()> {
        self.registry.remove_directory(project, index)
    }

    /// A Group's Pane layout, reconciled to its members; None for a Group
    /// that no longer exists.
    pub fn group_layout(
        &self,
        group: GroupId,
        bounds: crate::layout::Rect,
    ) -> Option<crate::layout::Tree> {
        self.groups.layout(group, bounds)
    }

    /// Put a Group back on the default grid, forgetting the arrangement the
    /// operator dragged, swapped or split.
    pub fn reset_group_layout(&mut self, group: GroupId) -> Result<(), ApplyError> {
        self.groups.reset_layout(group)
    }

    /// Persist a Group's Pane layout — a seam dragged, a Pane dropped.
    pub fn set_group_layout(
        &mut self,
        group: GroupId,
        tree: crate::layout::Tree,
    ) -> Result<(), ApplyError> {
        self.groups.set_layout(group, tree)
    }

    /// Forget a registered Project. Refused while any Thread, open or
    /// parked, records it — the Threads would lose their name and their
    /// filter — so removal is for a root that nothing works in any more.
    pub fn remove_project(&mut self, project: ProjectId) -> io::Result<()> {
        let recorded = self
            .store
            .thread_ids()?
            .into_iter()
            .any(|thread| self.project_id(thread) == Some(project));
        if recorded {
            return Err(io::Error::other("Threads still record this project"));
        }
        self.registry.remove_project(project)
    }

    /// Retitle a registered Project.
    pub fn rename_project(&mut self, project: ProjectId, title: &str) -> io::Result<()> {
        self.registry.rename_project(project, title)
    }

    /// Watch Session memory. Off until asked for: the operator's budget is
    /// theirs to set, and a cockpit with no sampler never restarts anything.
    pub fn watch_memory(&mut self, sampler: Box<dyn RssSampler>, limit: u64) {
        self.sampler = Some(sampler);
        self.limit = limit;
    }

    /// Replace any Session that has grown past the limit. The Thread keeps its
    /// transcript and its log; only the process is new, and the Pane says so.
    pub fn sweep(&mut self) -> Vec<Restart> {
        let Some(sampler) = &mut self.sampler else {
            return Vec::new();
        };
        let over: Vec<(ThreadId, u64)> = self
            .threads
            .iter()
            .filter_map(|(id, thread)| {
                let session = thread.session.as_ref()?;
                Some((*id, sampler.sample(*id, session.session()?.pid())?))
            })
            .filter(|(_, rss)| *rss > self.limit)
            .collect();

        let mut restarts = Vec::new();
        for (id, rss) in over {
            let note = match self.respawn(id) {
                None => continue,
                Some(Ok(())) => {
                    format!("restarted — the Session had grown to {}", megabytes(rss))
                }
                Some(Err(e)) => format!("restart failed after {}: {e}", megabytes(rss)),
            };
            crate::log!("thread {id}: session {note}");
            if let Some(thread) = self.threads.get_mut(&id) {
                thread.apply(Input::Notice(note));
            }
            restarts.push(Restart { thread: id, rss });
        }
        restarts
    }

    /// Replace `id`'s Session with a fresh one on the same conversation.
    /// `None` when there is no such Thread.
    fn respawn(&mut self, id: ThreadId) -> Option<io::Result<()>> {
        self.stale_sessions.remove(&id);
        self.visible_subjects.remove(&id);
        let project = self.project_id(id);
        let thread = self.threads.get_mut(&id)?;
        let resume = thread.resume.clone();
        let cwd = workspace::effective_cwd(
            thread.session_project_root.as_deref(),
            thread.workspace.as_ref(),
        )
        .map(Path::to_path_buf);
        // Drop the old Session before asking for a new one: the old
        // process must not outlive its replacement.
        thread.end_session();
        thread.replacement = None;
        thread.replace_generation();
        let spawned = self.spawner.start_after(
            placement(
                &self.registry,
                self.store.dir(),
                thread.workspace.as_ref(),
                id,
            ),
            SpawnRequest {
                thread: id,
                provider: thread.provider,
                model: thread.model.as_deref(),
                effort: thread.effort.as_deref(),
                resume: resume.as_deref(),
                permission_mode: thread.announced.permission_mode(),
                cwd: cwd.as_deref(),
                name: thread.title.as_deref(),
                additional_directories: project_additional_directories(
                    &self.registry,
                    project,
                    cwd.as_deref(),
                ),
                launch: thread.agent_launch(thread.provider),
            },
        );
        Some(spawned.map(|session| thread.begin_session(session)))
    }

    /// The provider's CLI was replaced on disk: every Session of
    /// `provider` ends once it is settled, so the next one runs the new
    /// release — see `end_unused_sessions`.
    pub fn end_stale_sessions(&mut self, provider: Provider) {
        let live = self.threads.iter().filter(|(_, state)| {
            state.provider == provider && (state.session.is_some() || state.replacement.is_some())
        });
        self.stale_sessions.extend(live.map(|(id, _)| *id));
    }

    /// End each Session its Thread has no use for (ADR 0032). Nothing
    /// shows it: the Thread keeps its transcript, status and generation,
    /// and its next use starts a Session on the same conversation.
    fn end_unused_sessions(&mut self) {
        let now = crate::clock::instant();
        for (id, thread) in &mut self.threads {
            if thread.outlived_use(now, self.stale_sessions.contains(id)) {
                crate::log!("thread {id}: session ended, settled");
                thread.end_session();
                self.stale_sessions.remove(id);
            }
        }
        // A stale Session parked, deleted or already ended needs nothing:
        // the next one starts on the new CLI anyway.
        let threads = &self.threads;
        self.stale_sessions.retain(|id| {
            threads
                .get(id)
                .is_some_and(|state| state.session.is_some() || state.replacement.is_some())
        });
    }

    /// Start a Thread: a durable log, a workspace to work in, and a Session
    /// serving it. Called at first send (#29) — the draft Pane's bootstrap —
    /// through `bootstrap_draft`, which rolls back any failed first send.
    /// Direct callers receive a provisional id during startup; a later
    /// failure leaves its open Thread showing the error for retry.
    ///
    /// The choice's repo registers as a project (idempotent — this is how
    /// the registry grows as Threads bind roots); a new worktree is placed
    /// by the registry's central layout on a branch it mints; an existing
    /// worktree passes the adoption conflict check and skips creation.
    pub fn open(&mut self, provider: Provider, workspace: WorkspaceChoice) -> io::Result<ThreadId> {
        self.open_with(provider, workspace, Launch::default())
    }

    /// `open`, its first Session started as `launch` says
    /// (`thread.start --args --resume`).
    pub fn open_with(
        &mut self,
        provider: Provider,
        workspace: WorkspaceChoice,
        launch: Launch,
    ) -> io::Result<ThreadId> {
        self.open_choice(
            ProviderChoice {
                provider,
                model: None,
            },
            workspace,
            None,
            launch,
        )
    }

    /// Make a worktree of `repo` on the new `branch`, from `base` (the
    /// repo's HEAD when none), placed and registered as the registry's own
    /// are; its Project and path. Git refuses a branch that already
    /// exists.
    pub fn create_worktree(
        &mut self,
        repo: &Path,
        branch: &str,
        base: Option<&str>,
    ) -> io::Result<(ProjectId, PathBuf)> {
        let project = self.registry.register(repo)?;
        let path = self.registry.place(project, branch);
        workspace::add_worktree(repo, &path, branch, base).map_err(io::Error::other)?;
        self.registry.register_worktree(project, branch, &path)?;
        Ok((project, path))
    }

    /// `effort` is the Thread's own reasoning level from its first breath
    /// (a draft's chip); None is the operator's default for the provider.
    fn open_choice(
        &mut self,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        effort: Option<String>,
        launch: Launch,
    ) -> io::Result<ThreadId> {
        let ProviderChoice { provider, model } = choice;
        let first_args = launch_args::LaunchArgs::of(provider, &launch);
        let agent_args = first_args.clone().and_then(launch_args::LaunchArgs::later);
        let Launch {
            resume, project, ..
        } = launch;
        let root =
            match &workspace {
                WorkspaceChoice::Main { checkout } => checkout,
                WorkspaceChoice::Branch { checkout, .. }
                | WorkspaceChoice::NewBranch { checkout } => checkout,
                WorkspaceChoice::NewWorktree { repo }
                | WorkspaceChoice::NamedWorktree { repo, .. } => repo,
                WorkspaceChoice::ExistingWorktree { repo, .. } => repo,
            };
        let project = match project {
            Some(project) => project,
            None => self.registry.register(root)?,
        };
        match &workspace {
            WorkspaceChoice::Branch { checkout, branch } => {
                workspace::checkout_existing_branch(checkout, branch).map_err(io::Error::other)?;
            }
            WorkspaceChoice::NewBranch { checkout } => {
                let branch = self.registry.reserve_branch(project)?;
                workspace::checkout_new_branch(checkout, &branch).map_err(io::Error::other)?;
            }
            _ => {}
        }
        let (binding, fresh_branch) = match workspace {
            WorkspaceChoice::Main { checkout } => (WorkspaceBinding::Main { checkout }, None),
            WorkspaceChoice::Branch { checkout, .. } | WorkspaceChoice::NewBranch { checkout } => {
                (WorkspaceBinding::Main { checkout }, None)
            }
            WorkspaceChoice::NewWorktree { repo } => {
                let entry = self.registry.reserve_worktree(project)?;
                (
                    WorkspaceBinding::Worktree {
                        repo,
                        path: entry.path,
                    },
                    Some(entry.branch),
                )
            }
            WorkspaceChoice::NamedWorktree { repo, branch, base } => {
                let (_, path) = self.create_worktree(&repo, &branch, base.as_deref())?;
                (WorkspaceBinding::Worktree { repo, path }, Some(branch))
            }
            WorkspaceChoice::ExistingWorktree { repo, path } => {
                (WorkspaceBinding::Worktree { repo, path }, None)
            }
        };
        let created =
            self.store
                .create_with_model(provider, model.clone(), Some(project), binding.clone());
        let (id, mut writer) = match created {
            Ok(created) => created,
            Err(error) => {
                if fresh_branch.is_some() {
                    self.registry.remove_worktree(binding.cwd())?;
                }
                return Err(error);
            }
        };
        if let Err(error) = self.store.mark_open(id) {
            let _ = self.store.delete(id);
            return Err(error);
        }
        if let Err(e) = ensure_workspace(&self.registry, &binding, id) {
            if fresh_branch.is_some() {
                let _ = self.registry.remove_worktree(binding.cwd());
            }
            let _ = self.store.delete(id);
            return Err(io::Error::other(e));
        }
        // The chosen effort goes into the header beside the model, so a
        // revive spawns on it too.
        if effort.is_some() {
            if let Err(e) =
                self.store
                    .set_tuning(id, model.clone(), effort.clone(), Some(&mut writer))
            {
                let _ = self.store.delete(id);
                return Err(io::Error::other(e.to_string()));
            }
        }
        if agent_args.is_some() {
            let path = launch_args::path(self.store.dir(), id);
            if let Err(e) = launch_args::save(&path, agent_args.as_ref()) {
                let _ = self.store.delete(id);
                return Err(e);
            }
        }
        let session = match self.spawner.start(SpawnRequest {
            thread: id,
            provider,
            model: model.as_deref(),
            effort: effort.as_deref(),
            resume: resume.as_deref(),
            permission_mode: None,
            cwd: workspace::effective_cwd(None, Some(&binding)),
            name: None,
            additional_directories: project_additional_directories(
                &self.registry,
                Some(project),
                Some(binding.cwd()),
            ),
            launch: launch_args::for_provider(first_args.as_ref(), provider),
        }) {
            Ok(session) => session,
            // The bootstrap's failure contract: no Thread. The worktree, if
            // one was just created, stays — real, registered, adoptable.
            Err(e) => {
                let _ = self.store.delete(id);
                return Err(e);
            }
        };
        self.threads.insert(
            id,
            Thread::fresh(
                self.boot,
                Some(session),
                writer,
                provider,
                model,
                effort,
                resume,
                Some(binding),
                None,
                self.store.dir().join(id.to_string()),
            ),
        );
        if let Some(state) = self.threads.get_mut(&id) {
            state.agent_args = agent_args;
        }
        self.note_open(id);
        Ok(id)
    }

    /// Start the first send. The returned id is provisional while startup
    /// is pending; `take_bootstrap_results` reports its delivery or rollback.
    /// Ready adapters complete before this returns.
    ///
    /// Turn one draft into a live, locked Thread. The Thread log and Session
    /// are implementation details of this transaction: if the first prompt
    /// cannot reach the Session, both are removed and the caller still owns
    /// the exact prompt and draft choices.
    pub fn bootstrap(
        &mut self,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
    ) -> io::Result<ThreadId> {
        self.bootstrap_with(choice, workspace, prompt, None)
    }

    /// `bootstrap` with the Thread's own effort from the start.
    fn bootstrap_with(
        &mut self,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
        effort: Option<String>,
    ) -> io::Result<ThreadId> {
        self.begin_bootstrap(choice, workspace, prompt, effort, None, None)
    }

    pub fn bootstrap_in_group(
        &mut self,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
        group: GroupId,
    ) -> io::Result<ThreadId> {
        self.bootstrap_in_group_with(choice, workspace, prompt, group, None)
    }

    fn bootstrap_in_group_with(
        &mut self,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
        group: GroupId,
        effort: Option<String>,
    ) -> io::Result<ThreadId> {
        self.groups.validate_join(group).map_err(io::Error::other)?;
        self.begin_bootstrap(choice, workspace, prompt, effort, Some(group), None)
    }

    fn begin_bootstrap(
        &mut self,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
        effort: Option<String>,
        group: Option<GroupId>,
        new_group_with: Option<ThreadId>,
    ) -> io::Result<ThreadId> {
        let id = self.open_choice(choice, workspace, effort, Launch::default())?;
        let pending = PendingBootstrap {
            prompt: prompt.to_string(),
            group,
            new_group_with,
            draft: None,
        };
        if self.threads[&id]
            .session
            .as_ref()
            .is_some_and(SessionLifecycle::is_starting)
        {
            self.bootstraps.insert(id, pending);
        } else if let Err(error) = self.finish_bootstrap(id, &pending) {
            self.rollback_bootstrap(id)?;
            return Err(error);
        }
        Ok(id)
    }

    /// Membership is durable before delivery; readiness precedes both.
    fn finish_bootstrap(&mut self, id: ThreadId, pending: &PendingBootstrap) -> io::Result<()> {
        if let Some(group) = pending.group {
            self.groups
                .apply(GroupChange::Join {
                    thread: id,
                    group,
                    index: None,
                })
                .map_err(io::Error::other)?;
        }
        if let Some(first) = pending.new_group_with {
            if self.groups.of(first).is_some() {
                return Err(io::Error::other(
                    "the original Thread joined a Group before the draft was sent",
                ));
            }
            self.groups
                .apply(GroupChange::Create { first, second: id })
                .map_err(io::Error::other)?;
        }
        let state = self.threads.get_mut(&id).expect("pending Thread exists");
        if let Some(binding) = &state.workspace {
            state.apply(Input::Notice(format!(
                "{}{}",
                crate::transcript::OPENED_IN,
                binding.cwd().display()
            )));
        }
        deliver(state, pending.prompt.clone(), self.suggestions_enabled)?;
        Ok(())
    }

    fn rollback_bootstrap(&mut self, id: ThreadId) -> io::Result<()> {
        // Roll back membership before its durable Thread; if persistence
        // refuses, preserve the Thread so the Group never points to nothing.
        if self.groups.of(id).is_some() {
            self.groups
                .apply(GroupChange::Leave { thread: id })
                .map_err(io::Error::other)?;
        }
        self.threads.remove(&id);
        self.open.retain(|open| *open != id);
        self.store.delete(id)
    }

    // Legacy #24 fixtures exercise persisted `session_project_root` loading.
    // No production writer exists after the #29 lock.
    #[cfg(test)]
    pub(crate) fn set_session_project_root(
        &mut self,
        thread: ThreadId,
        root: Option<PathBuf>,
    ) -> Result<(), LoadError> {
        self.visible_subjects.remove(&thread);
        match self.threads.get_mut(&thread) {
            Some(state) => {
                state.end_session();
                state.replacement = None;
                state.replace_generation();
                self.store.set_session_project_root(
                    thread,
                    root.clone(),
                    Some(&mut state.writer),
                )?;
                state.session_project_root = root;
            }
            None => self.store.set_session_project_root(thread, root, None)?,
        }
        let awaiting = self
            .threads
            .get_mut(&thread)
            .map(|state| std::mem::take(&mut state.awaiting.prompts))
            .unwrap_or_default();
        for text in awaiting {
            self.send(thread, text);
        }
        Ok(())
    }

    /// Re-aim a Thread onto a provider (and optionally a model) (#25).
    /// `Ok` accepts the request; `ThreadView::starting` reports a deferred
    /// replacement. A later refusal is a Notice, leaving the prior choices
    /// and Session intact. Ready adapters still settle before returning.
    /// Spawn-new-first, swap second: a CLI that fails to spawn leaves the
    /// old Session serving and the header untouched, and the error carries
    /// the provider's words. Durable before anything in memory changes, so
    /// a parked-never-prompted Thread revives onto its choice.
    ///
    /// Before the first prompt the swap is a fresh Transcript — nothing
    /// operator-authored exists pre-lock, so the old Init and model must
    /// not linger — and, when an agent runs, an eager respawn: the new
    /// Provider's commands and models arrive while the operator is still
    /// choosing. Works on a parked Thread too: only the store is touched.
    /// A Thread with no agent running starts none (ADR 0032): its next
    /// start is on the new Provider.
    ///
    /// After the first prompt the Transcript and history stay: the new
    /// Provider starts fresh (the old one's id means nothing to it) and the
    /// next prompt carries the earlier conversation, digested, ahead of the
    /// operator's text — once. The switch is a `handover` record in the
    /// log, so a revive knows the last Init is nobody's to resume and
    /// whether the carry still owes. Mid-turn the switch is refused rather
    /// than cutting the turn; a parked Thread that has spoken waits for its
    /// revive (`Locked`), because the carry needs the live conversation.
    /// The effort resets to the new Provider's default: the old one's word
    /// may mean nothing to it.
    pub fn set_provider(
        &mut self,
        thread: ThreadId,
        choice: ProviderChoice,
    ) -> Result<(), ProvisionError> {
        self.past_unseen_start(thread, |engine| engine.change_provider(thread, choice))
    }

    fn change_provider(
        &mut self,
        thread: ThreadId,
        choice: ProviderChoice,
    ) -> Result<(), ProvisionError> {
        self.parked_listing.stale = true;
        let Some(state) = self.threads.get(&thread) else {
            // Parked: the lock is whether a prompt was ever sent, which the
            // log's tail says without a full load.
            if self.store.prompted(thread).map_err(ProvisionError::Store)? {
                return Err(ProvisionError::Locked);
            }
            return self
                .store
                .set_provider(thread, choice.provider, choice.model, None, None)
                .map_err(ProvisionError::Store);
        };
        if state.provider == choice.provider && state.model == choice.model {
            self.threads.get_mut(&thread).expect("checked").replacement = None;
            return Ok(());
        }
        if state.first_prompt_sent {
            return self.hand_over(thread, choice);
        }
        // The effort is a word of the provider's: it stays across a model
        // change and resets across a provider change.
        let effort = if state.provider == choice.provider {
            state.effort.clone()
        } else {
            None
        };
        self.provision(thread, choice.provider, choice.model, effort)
    }

    fn provision(
        &mut self,
        thread: ThreadId,
        provider: Provider,
        model: Option<String>,
        effort: Option<String>,
    ) -> Result<(), ProvisionError> {
        self.replace_session(thread, provider, model, effort, ReplacementKind::Fresh)
    }

    fn hand_over(
        &mut self,
        thread: ThreadId,
        choice: ProviderChoice,
    ) -> Result<(), ProvisionError> {
        self.replace_session(
            thread,
            choice.provider,
            choice.model,
            None,
            ReplacementKind::Handover,
        )
    }

    /// The old Session and durable choices stay authoritative until startup
    /// succeeds. All replacement paths share this transaction. A Thread
    /// with no Session gets no new one: the change is recorded for its next.
    fn replace_session(
        &mut self,
        thread: ThreadId,
        provider: Provider,
        model: Option<String>,
        effort: Option<String>,
        kind: ReplacementKind,
    ) -> Result<(), ProvisionError> {
        let project = self.project_id(thread);
        let state = self.threads.get(&thread).expect("checked by caller");
        if state.tuning.is_some() {
            return Err(ProvisionError::Tuning);
        }
        if state.native_queue.pending()
            || (state.busy() && !matches!(kind, ReplacementKind::Fresh))
            || state.replacement.is_some()
            || self.bootstraps.contains_key(&thread)
        {
            return Err(ProvisionError::Busy);
        }
        let cwd = workspace::effective_cwd(
            state.session_project_root.as_deref(),
            state.workspace.as_ref(),
        );
        let session = if state.session.is_none() {
            None
        } else {
            let resume = match kind {
                ReplacementKind::Retune => {
                    Some(state.resume.as_deref().ok_or(ProvisionError::Locked)?)
                }
                _ => None,
            };
            let started = self.spawner.start_after(
                placement(
                    &self.registry,
                    self.store.dir(),
                    state.workspace.as_ref(),
                    thread,
                ),
                SpawnRequest {
                    thread,
                    provider,
                    model: model.as_deref(),
                    effort: effort.as_deref(),
                    resume,
                    // Another Provider's word means nothing to the next.
                    permission_mode: (provider == state.provider)
                        .then(|| state.announced.permission_mode())
                        .flatten(),
                    cwd,
                    name: state.title.as_deref(),
                    additional_directories: project_additional_directories(
                        &self.registry,
                        project,
                        cwd,
                    ),
                    launch: state.agent_launch(provider),
                },
            );
            Some(started.map_err(ProvisionError::Spawn)?)
        };
        let mut replacement = Replacement {
            session,
            provider,
            model,
            effort,
            kind,
            exchanges: None,
        };
        if replacement
            .session
            .as_ref()
            .is_some_and(SessionLifecycle::is_starting)
        {
            self.threads.get_mut(&thread).expect("checked").replacement = Some(replacement);
            return Ok(());
        }
        match self.carry_read(thread, &mut replacement)? {
            Some(exchanges) => self.commit_replacement(thread, replacement, exchanges),
            None => {
                self.threads.get_mut(&thread).expect("checked").replacement = Some(replacement);
                Ok(())
            }
        }
    }

    /// A handover's exchanges, read whole from the log before it commits
    /// (invariant 3): `Some` once read — at once for a small log — and
    /// `None` while a large one is read off the UI thread. Other
    /// replacements carry nothing and are ready now.
    fn carry_read(
        &mut self,
        thread: ThreadId,
        replacement: &mut Replacement,
    ) -> Result<Option<Vec<(String, String)>>, ProvisionError> {
        if !matches!(replacement.kind, ReplacementKind::Handover) {
            return Ok(Some(Vec::new()));
        }
        if replacement.exchanges.is_none() {
            // Everything the old Session said is in the log before it is read.
            let state = self.threads.get_mut(&thread).expect("checked");
            state
                .writer
                .flush()
                .map_err(|error| ProvisionError::Store(LoadError::Io(error)))?;
            replacement.exchanges = Some(self.store.read_exchanges(thread));
        }
        match replacement.exchanges.as_mut().expect("started").poll() {
            Some(read) => read.map(Some).map_err(ProvisionError::Store),
            None => Ok(None),
        }
    }

    fn commit_replacement(
        &mut self,
        thread: ThreadId,
        replacement: Replacement,
        exchanges: Vec<(String, String)>,
    ) -> Result<(), ProvisionError> {
        self.visible_subjects.remove(&thread);
        let Replacement {
            session,
            provider,
            model,
            effort,
            kind,
            ..
        } = replacement;
        let state = self.threads.get_mut(&thread).expect("checked");
        let handover = if matches!(kind, ReplacementKind::Handover) {
            Some(
                self.store
                    .commit_handover(provider, model.clone(), &mut state.writer, exchanges)
                    .map_err(ProvisionError::Store)?,
            )
        } else {
            self.store
                .set_provider(
                    thread,
                    provider,
                    model.clone(),
                    effort.clone(),
                    Some(&mut state.writer),
                )
                .map_err(ProvisionError::Store)?;
            None
        };
        match kind {
            ReplacementKind::Fresh => {
                state.activity.replace();
                state.resume = None;
                state.carry = None;
            }
            ReplacementKind::Handover => {
                let handover = handover.expect("committed with header");
                state.carry = Some(carry_digest(handover.from, &handover.exchanges));
                state.resume = None;
                state.apply(Input::Notice(crate::store::handover_notice(
                    provider,
                    model.as_deref(),
                )));
            }
            ReplacementKind::Retune => {
                if state.model != model {
                    let label = crate::providers::models::label(
                        model.as_deref().unwrap_or("default"),
                        state.announced.models(),
                    );
                    state.apply(Input::Notice(format!("model changed to {label}")));
                }
                if state.effort != effort {
                    state.apply(Input::Notice(format!(
                        "effort changed to {}",
                        effort.as_deref().unwrap_or("default")
                    )));
                }
            }
        }
        if state.session.is_some() {
            let why = match kind {
                ReplacementKind::Fresh => "replaced by a fresh conversation".to_string(),
                ReplacementKind::Handover => format!("handed over to {provider}"),
                ReplacementKind::Retune => "replaced to change model or effort".to_string(),
            };
            crate::log!("thread {thread}: session ended: {why}");
        }
        if state.provider != provider {
            state.announced.forget();
        }
        match session {
            Some(session) => state.begin_session(session),
            None => state.end_session(),
        }
        state.provider = provider;
        state.model = model;
        state.effort = effort;
        state.replace_generation();
        Ok(())
    }

    pub fn project_id(&self, thread: ThreadId) -> Option<ProjectId> {
        self.try_project_id(thread).ok().flatten()
    }

    pub fn try_project_id(&self, thread: ThreadId) -> Result<Option<ProjectId>, LoadError> {
        self.store.peek(thread).map(|meta| meta.project_id)
    }

    /// Delete a Thread for good: its log and directory. Registered project
    /// worktrees survive as durable, adoptable workspace inventory. Only a
    /// legacy unregistered worktree binding is removed, and then only when
    /// clean (`git status` defines clean); its branch is never deleted.
    pub fn delete(&mut self, thread: ThreadId) -> Result<(), DeleteError> {
        self.parked_listing.stale = true;
        if self.bootstraps.contains_key(&thread) {
            return self.park(thread).map_err(DeleteError::Io);
        }
        let meta = self.store.peek(thread).map_err(DeleteError::Load)?;
        let groups_before = self.groups.clone();
        let grouped = self.groups.of(thread).is_some();
        let live = self
            .threads
            .get(&thread)
            .is_some_and(|state| state.session.is_some());
        if grouped {
            self.groups
                .apply(GroupChange::Leave { thread })
                .map_err(|error| DeleteError::Io(io::Error::other(error)))?;
        }

        let deleted = (|| {
            if let (None, Some(WorkspaceBinding::Worktree { repo, path })) =
                (meta.project_id, meta.workspace)
            {
                // A worktree already gone by hand leaves nothing to check or
                // remove; the log's deletion below is all that is left to do.
                if path.exists() {
                    if !workspace::is_clean(&path).map_err(DeleteError::Git)? {
                        return Err(DeleteError::DirtyWorktree { path });
                    }
                    // The Session dies before its worktree goes: a live process
                    // whose cwd is the directory being removed holds it open on
                    // Windows, and the dirty check above already ruled out any
                    // work that Session had in flight.
                    self.threads.remove(&thread);
                    self.open.retain(|open| *open != thread);
                    workspace::remove_worktree(&repo, &path).map_err(DeleteError::Git)?;
                    self.removed_worktrees.push(path.clone());
                }
                // The menu forgets the removed tree; the branch stays git's.
                self.registry
                    .remove_worktree(&path)
                    .map_err(DeleteError::Io)?;
            }
            self.threads.remove(&thread);
            self.visible_subjects.remove(&thread);
            self.open.retain(|open| *open != thread);
            self.notifications.forget(thread);
            self.store.delete(thread).map_err(DeleteError::Io)
        })();
        // A refused delete may still have ended the Session (see above).
        if live && !self.threads.contains_key(&thread) {
            crate::log!("thread {thread}: session ended: deleted");
        }

        if let Err(error) = deleted {
            if grouped {
                self.groups
                    .restore_snapshot(groups_before)
                    .map_err(|restore| {
                        DeleteError::Io(io::Error::other(format!(
                            "{error}; restoring group membership also failed: {restore}"
                        )))
                    })?;
            }
            return Err(error);
        }
        Ok(())
    }

    /// Close a Pane: the Session ends, the log is flushed, and the Thread
    /// keeps nothing in memory until it is opened again.
    pub fn park(&mut self, thread: ThreadId) -> io::Result<()> {
        if let Some(pending) = self.bootstraps.remove(&thread) {
            self.rollback_bootstrap(thread)?;
            self.bootstrap_results.push(BootstrapResult {
                draft: pending.draft,
                prompt: pending.prompt,
                result: Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Session startup cancelled",
                )),
            });
            return Ok(());
        }
        let Some(state) = self.threads.get_mut(&thread) else {
            // A parked Group member's tile (`enter_group`) has a Pane and no
            // Session: parking it again only takes the Pane away.
            self.open.retain(|open| *open != thread);
            return Ok(());
        };
        // The parked row's subagent count, from the Activity that knows it.
        state
            .writer
            .note_subagents(state.activity.view().children().len());
        // A failed write keeps the live owner and its retry buffer reachable.
        if let Err(error) = state.writer.flush() {
            state.report_store_error(io::Error::new(error.kind(), error.to_string()));
            return Err(error);
        }
        let mut state = self.threads.remove(&thread).expect("checked");
        self.count_closed(thread);
        if state.session.is_some() {
            crate::log!("thread {thread}: session ended: parked");
        }
        // The rest — the open marker, a final mark, a sync through the
        // drive's cache — is the store's worker's; a revive waits for it.
        state.writer.park();
        self.notifications.disconnect(thread);
        self.visible_subjects.remove(&thread);
        self.open.retain(|open| *open != thread);
        state.session = None;
        state.replacement = None;
        #[cfg(test)]
        if self.refuse_park.contains(&thread) {
            return Err(io::Error::other("stub refused to park"));
        }
        Ok(())
    }

    /// Reopen a parked Thread: its history is replayed from the log into a
    /// fresh Transcript, with nothing running in it (ADR 0032). Its Session
    /// starts when the Thread is used, told where to resume.
    pub fn revive(&mut self, thread: ThreadId) -> Result<(), LoadError> {
        self.reopen(thread, None).map(drop)
    }

    /// Reopen a parked Thread as `launch` says (`thread.revive`, ADR 0031):
    /// its arguments, environment and launcher replace the saved ones, and
    /// its Session carries on `launch.resume`'s conversation, or starts a
    /// new one under the Thread's history when there is none. The Thread
    /// keeps its id, as a Herdr pane does when its agent is started again.
    /// Its Session starts now; one that cannot parks the Thread again.
    pub fn revive_with(&mut self, thread: ThreadId, launch: Launch) -> Result<(), LoadError> {
        let first_args = self.reopen(thread, Some(launch))?;
        if let Err(error) = self.start_session(thread, first_args.as_ref()) {
            self.park(thread)?;
            return Err(LoadError::Io(error));
        }
        Ok(())
    }

    /// Open a parked Thread with nothing running in it, answering the
    /// arguments its first Session starts with.
    fn reopen(
        &mut self,
        thread: ThreadId,
        launch: Option<Launch>,
    ) -> Result<Option<launch_args::LaunchArgs>, LoadError> {
        // Reopened while open, a Thread would be replaced whole: its live
        // Session killed mid-turn, the records its log had not taken yet
        // lost, and a second CLI started on the conversation.
        if self.threads.contains_key(&thread) {
            return Err(LoadError::Io(io::Error::other("the Thread is open")));
        }
        // Bounded: what Activity retains, not the whole log (ADR 0009).
        let mut revival = self.store.revive(thread)?;
        let snapshot = revival.snapshot();
        let provider = snapshot.provider();
        // The board may still remember the Provider from before a handover.
        // Its flags, launcher and resume id belong to that Provider alone;
        // keep the Thread's current conversation and saved launch instead.
        let launch = launch.filter(|launch| {
            launch
                .launch_provider
                .is_none_or(|target| target == provider)
        });
        let mut workspace = snapshot.workspace();
        let session_project_root = snapshot.session_project_root();
        let model = snapshot.model();
        let effort = snapshot.effort();
        let title = snapshot.title().map(str::to_string);
        let resume = match &launch {
            Some(launch) => launch.resume.clone(),
            None => snapshot.resume_target().map(str::to_string),
        };
        // A worktree the agent made and the Thread followed into is the
        // agent's, not Ferrite's: gone while parked, the Thread goes back
        // to main rather than rebuilding somebody else's tree. Ferrite's
        // own (registered, or pre-registry under the store) are recreated
        // when its Session starts, as ever.
        if let Some(WorkspaceBinding::Worktree { repo, path }) = &workspace {
            if !path.exists()
                && self.registry.branch_for(path).is_none()
                && !path.starts_with(self.store.dir())
            {
                let main = WorkspaceBinding::Main {
                    checkout: repo.clone(),
                };
                self.store
                    .set_workspace(thread, &main, Some(revival.writer()))?;
                workspace = Some(main);
            }
        }
        let saved = launch_args::path(self.store.dir(), thread);
        // A new launch's own arguments start this Session; every later one
        // starts with what is saved of them.
        let (first_args, agent_args, unreadable_args) = match &launch {
            Some(launch) => {
                let first_args = launch_args::LaunchArgs::of(provider, launch);
                let agent_args = first_args.clone().and_then(launch_args::LaunchArgs::later);
                launch_args::save(&saved, agent_args.as_ref())?;
                (first_args, agent_args, None)
            }
            None => match launch_args::load(&saved) {
                Ok(saved) => (saved.clone(), saved, None),
                Err(error) => (None, None, Some(error)),
            },
        };
        let writer = revival.take_writer();

        let mut state = Thread::fresh(
            self.boot,
            None,
            writer,
            provider,
            model,
            effort,
            resume,
            workspace,
            session_project_root,
            self.store.dir().join(thread.to_string()),
        );
        state.title = title;
        state.agent_args = agent_args;
        if let Some(error) = unreadable_args {
            state.apply(Input::Notice(format!(
                "the launch arguments saved for this Thread could not be read ({error}); \
                 its agent started without them"
            )));
        }
        // A switch whose carry never went out (parked before the next
        // prompt) still owes it: the new Provider has heard nothing yet.
        if let Some(handover) = revival.owed_handover() {
            state.carry = Some(carry_digest(handover.from, &handover.exchanges));
        }
        state.prompt_history = PromptHistory::new(revival.snapshot().prompt_texts());
        state.first_prompt_sent = revival.prompted();
        state.read_back(revival.activity_inputs());
        for kept in self.store.pending_records(thread) {
            state.apply(Input::Notice(format!(
                "history from before Ferrite last quit could not be saved in the log; \
                 it is kept in {}",
                kept.display()
            )));
        }
        if let Some(notice) = state.native_queue.disconnect(provider == Provider::Codex) {
            state.apply(Input::Notice(notice));
        }
        state.activity.apply(ActivityInput::Connect {
            generation: state.generation,
        });

        let owes_delivery = state.native_queue.holds_unsent();
        self.store.mark_open(thread)?;
        self.threads.insert(thread, state);
        self.note_open(thread);
        // Queued prompts the provider keeps are sent only by a running agent.
        if owes_delivery {
            self.start_or_say(thread, "the queued prompts could not be sent");
        }
        Ok(first_args)
    }

    /// Open every member of one persisted Group or leave the live set unchanged.
    pub fn revive_group(&mut self, group: GroupId) -> Result<Vec<ThreadId>, ReviveGroupError> {
        let members = self
            .groups
            .get(group)
            .ok_or(ReviveGroupError::MissingGroup)?
            .members
            .clone();
        let mut revived = Vec::new();
        for thread in &members {
            if self.threads.contains_key(thread) {
                continue;
            }
            if let Err(error) = self.revive(*thread) {
                let mut rollback_failure = None;
                for opened in revived.into_iter().rev() {
                    if let Err(error) = self.park(opened) {
                        rollback_failure.get_or_insert((opened, error));
                    }
                }
                if let Some((thread, error)) = rollback_failure {
                    return Err(ReviveGroupError::Rollback { thread, error });
                }
                return Err(ReviveGroupError::Member {
                    thread: *thread,
                    error,
                });
            }
            revived.push(*thread);
        }
        Ok(members)
    }

    /// Adopt a CLI session file as a Thread of this cockpit (#11) — the
    /// import module's work, against this cockpit's own store. The Thread
    /// is durable and parked when this returns; `revive` opens it like any
    /// other, history replayed and the Session resuming the file's own
    /// session id. A refusal is the import module's, unchanged: readable,
    /// and no Thread was created.
    pub fn import(&mut self, path: &Path) -> Result<ThreadId, crate::import::ImportError> {
        self.parked_listing.stale = true;
        crate::import::import_registered(&self.store, &mut self.registry, path)
    }

    /// Send a prompt now: on the wire, in the transcript, in the log. A
    /// Thread with no Session starts one here through the same resume path
    /// a revive uses, which re-arms the preface. One that closed starts
    /// afresh on Main; one that ended unseen (ADR 0032) keeps what its
    /// window shows.
    pub fn send(&mut self, thread: ThreadId, text: String) {
        if self.bootstraps.contains_key(&thread) {
            return;
        }
        if self
            .threads
            .get(&thread)
            .is_some_and(|state| state.session.is_none() && !state.activity.view().connected())
        {
            self.visible_subjects.remove(&thread);
        }
        let Some(state) = self.threads.get_mut(&thread) else {
            return;
        };
        state.prompt_history.reset();
        // Guarded before anything spawns: a refused send must not leave a
        // fresh provider process behind. `deliver` guards again for the
        // sends that never pass through here.
        if let Some(refusal) = vanished_root_refusal(state) {
            state.apply(Input::Notice(refusal));
            return;
        }
        if !self.start_or_say(thread, "send failed") {
            return;
        }
        let state = self.threads.get_mut(&thread).expect("checked");
        if state.ready_session().is_none() || !state.awaiting.prompts.is_empty() {
            state.awaiting.prompts.push_back(text);
            return;
        }
        if let Err(error) = deliver(state, text.clone(), self.suggestions_enabled) {
            state.undelivered(text, error);
        }
    }

    /// `start_session` for something the operator asked of the Thread: a
    /// start that fails says so on it, `what` naming what failed.
    fn start_or_say(&mut self, thread: ThreadId, what: &str) -> bool {
        let Err(error) = self.start_session(thread, None) else {
            return true;
        };
        if let Some(state) = self.threads.get_mut(&thread) {
            state.apply(Input::Notice(format!("{what}: {error}")));
        }
        false
    }

    /// Start an open Thread's Session where its conversation left off, the
    /// one way any does (ADR 0032), unless it has one running or starting.
    /// `first`: arguments for this start only; otherwise the saved ones.
    /// Refused while its session project root is gone. A worktree deleted
    /// while the Thread was parked is recreated on its own branch before
    /// anything spawns into it; the git calls never hold the caller, they
    /// run where the spawner starts the CLI (`Spawner::start_after`).
    fn start_session(
        &mut self,
        thread: ThreadId,
        first: Option<&launch_args::LaunchArgs>,
    ) -> io::Result<()> {
        self.hold(thread);
        let project = self.project_id(thread);
        let Some(state) = self.threads.get_mut(&thread) else {
            return Ok(());
        };
        if state.session.is_some() || state.replacement.is_some() {
            return Ok(());
        }
        if let Some(refusal) = vanished_root_refusal(state) {
            return Err(io::Error::new(io::ErrorKind::NotFound, refusal));
        }
        // After a close, or with no Session yet, this start begins a new
        // generation. After one ended unseen, nothing a window holds is
        // invalidated: the start is unseen too (ADR 0032).
        if !state.activity.view().connected() {
            state.replace_generation();
        }
        let cwd = workspace::effective_cwd(
            state.session_project_root.as_deref(),
            state.workspace.as_ref(),
        )
        .map(Path::to_path_buf);
        let additional_directories =
            project_additional_directories(&self.registry, project, cwd.as_deref());
        let place = placement(
            &self.registry,
            self.store.dir(),
            state.workspace.as_ref(),
            thread,
        );
        let session = self.spawner.start_after(
            place,
            SpawnRequest {
                thread,
                provider: state.provider,
                model: state.model.as_deref(),
                effort: state.effort.as_deref(),
                resume: state.resume.as_deref(),
                permission_mode: state.announced.permission_mode(),
                cwd: cwd.as_deref(),
                name: state.title.as_deref(),
                additional_directories,
                launch: match first {
                    Some(args) => launch_args::for_provider(Some(args), state.provider),
                    None => state.agent_launch(state.provider),
                },
            },
        )?;
        state.begin_session(session);
        state.woken = true;
        Ok(())
    }

    /// Stop the running turn.
    pub fn interrupt(&mut self, thread: ThreadId) {
        if self.bootstraps.contains_key(&thread) {
            let _ = self.park(thread);
            return;
        }
        let Some(state) = self.threads.get_mut(&thread) else {
            return;
        };
        // A start the operator sees (prompts wait on it) is what Escape
        // stops: what waited goes back to the Composer. An unseen one is
        // not theirs to stop.
        if state.starting()
            && state
                .session
                .as_ref()
                .is_some_and(SessionLifecycle::is_starting)
        {
            state.end_session();
            state.replace_generation();
            state.give_up_awaiting();
            self.visible_subjects.remove(&thread);
        }
        let state = self.threads.get_mut(&thread).expect("checked");
        state.replacement = None;
        if let Some(session) = state
            .session
            .as_mut()
            .and_then(SessionLifecycle::session_mut)
        {
            match session.interrupt() {
                Ok(()) => {
                    // Escape with prompts held means "stop, then do these":
                    // Claude's CLI runs its surviving queue itself; Codex
                    // discards un-consumed steers, so the resend is ours.
                    state.resend_held_on_turn_end =
                        state.provider == Provider::Codex && state.native_queue.pending();
                }
                Err(e) => {
                    state.apply(Input::Notice(format!("interrupt failed: {e}")));
                }
            }
        }
    }

    /// Compatibility entry point. The app uses generation-scoped handles via
    /// `respond_decision`; this accepts only a still-pending identical request.
    pub fn respond(&mut self, thread: ThreadId, decision: &Decision, answer: DecisionAnswer) {
        let handle = self.threads.get(&thread).and_then(|state| {
            state
                .activity
                .view()
                .decisions()
                .iter()
                .find(|pending| pending.decision == *decision)
                .map(|pending| pending.handle.clone())
        });
        if let Some(handle) = handle {
            let _ = self.respond_decision(thread, &handle, answer);
        }
    }

    /// Answer exactly the request the operator saw. A tab change cannot
    /// redirect it, and an old Session's handle cannot match its replacement.
    /// A failed write retains the Decision so the operator can retry.
    pub fn respond_decision(
        &mut self,
        thread: ThreadId,
        handle: &DecisionHandle,
        answer: DecisionAnswer,
    ) -> io::Result<bool> {
        let Some(state) = self.threads.get_mut(&thread) else {
            return Ok(false);
        };
        let Some(pending) = state
            .activity
            .view()
            .decisions()
            .iter()
            .find(|pending| pending.handle == *handle)
            .cloned()
        else {
            return Ok(false);
        };
        if handle.generation != state.generation {
            return Ok(false);
        }
        if pending.submitting {
            return Ok(false);
        }
        let allowed = !matches!(answer, DecisionAnswer::Deny { .. });
        let Some(session) = state
            .session
            .as_mut()
            .and_then(SessionLifecycle::session_mut)
        else {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Session is not available",
            ));
        };
        if let Err(error) = session.respond_to_decision(&pending.decision.id, answer) {
            state.apply(Input::Notice(format!("answer failed: {error}")));
            return Err(error);
        }
        if !pending.decision.blocks_execution() {
            state.activity.apply(ActivityInput::AnswerSubmitted {
                handle: handle.clone(),
            });
            return Ok(true);
        }
        state.activity.apply(ActivityInput::Answered {
            handle: handle.clone(),
            allowed,
            at: crate::clock::instant(),
        });
        if let Err(error) = state.writer.flush() {
            state.report_store_error(error);
        }
        Ok(true)
    }

    /// Something Ferrite itself needs to say in a Pane.
    pub fn apply_input(&mut self, thread: ThreadId, input: Input) {
        if let Some(state) = self.threads.get_mut(&thread) {
            state.apply(input);
        }
    }

    /// One open Thread, read through one handle: everything a Pane, a nav
    /// row or a menu asks about a Thread this Cockpit holds live. `None` is
    /// the one "is it open?" answer; every read after it is infallible and
    /// borrows the Cockpit, not the handle.
    pub fn thread(&self, thread: ThreadId) -> Option<ThreadView<'_>> {
        self.threads.get(&thread).map(|state| ThreadView { state })
    }

    pub fn discover_sessions(
        &mut self,
        roots: Vec<(Provider, PathBuf)>,
        cap: usize,
    ) -> Option<Receiver<io::Result<Vec<crate::import::Candidate>>>> {
        self.spawner.discover_sessions(roots, cap)
    }

    /// Ask the Thread's agent for files matching `query`, starting it if it
    /// is not running (ADR 0032): asked once it is up.
    pub fn search_files(
        &mut self,
        thread: ThreadId,
        query: &str,
    ) -> io::Result<Receiver<io::Result<Vec<crate::providers::FileSuggestion>>>> {
        let state = self.wake_for(thread)?;
        let (reply, answer) = std::sync::mpsc::sync_channel(1);
        match state.ready_session() {
            Some(session) => session.search_files(query, reply)?,
            None => state.awaiting.search = Some((query.to_string(), reply)),
        }
        Ok(answer)
    }

    /// Route a provider-native control to the Thread's agent, starting it if
    /// it is not running (ADR 0032): refused at once if the agent last said
    /// it does not offer it, else run once it is up, a failure then said on
    /// the Thread. Controls never replace a Session and never alter
    /// Activity.
    pub fn control(&mut self, thread: ThreadId, action: crate::SessionControl) -> io::Result<()> {
        if self
            .threads
            .get(&thread)
            .is_some_and(|state| !state.announced.supports(control_kind(&action)))
        {
            return Err(unsupported_control());
        }
        let state = self.wake_for(thread)?;
        match state.ready_session() {
            Some(session) => run_control(session, action),
            None => {
                state.awaiting.controls.push(action);
                Ok(())
            }
        }
    }

    /// An open Thread whose agent is running or starting, for asking it
    /// something. The ask is use: its time at rest counts again.
    fn wake_for(&mut self, thread: ThreadId) -> io::Result<&mut Thread> {
        if !self.threads.contains_key(&thread) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Thread is not open",
            ));
        }
        self.start_session(thread, None)?;
        let state = self.threads.get_mut(&thread).expect("checked");
        state.used();
        Ok(state)
    }

    /// What was asked of `thread`'s agent while it started, besides
    /// prompts, goes to it once it is up.
    fn release_asks(&mut self, thread: ThreadId) {
        let state = self.threads.get_mut(&thread).expect("checked by caller");
        if !state.awaiting.asks() || state.ready_session().is_none() {
            return;
        }
        let controls = std::mem::take(&mut state.awaiting.controls);
        let search = state.awaiting.search.take();
        let session = state.ready_session().expect("checked");
        let failed: Vec<io::Error> = controls
            .into_iter()
            .filter_map(|action| run_control(session, action).err())
            .collect();
        if let Some((query, reply)) = search {
            let unanswered = reply.clone();
            if let Err(error) = session.search_files(&query, reply) {
                let _ = unanswered.try_send(Err(error));
            }
        }
        for error in failed {
            state.apply(Input::Notice(format!("control failed: {error}")));
        }
    }

    /// Stop one Subagent of `thread`; Main's turn and the others run on.
    /// A failure is said in the transcript, as an interrupt's is.
    pub fn stop_subagent(&mut self, thread: ThreadId, key: &AgentKey) {
        let Some(state) = self.threads.get_mut(&thread) else {
            return;
        };
        let Some(session) = state
            .session
            .as_mut()
            .and_then(SessionLifecycle::session_mut)
        else {
            return;
        };
        if let Err(e) = session.stop_subagent(key) {
            state.apply(Input::Notice(format!("stop failed: {e}")));
        }
    }

    /// Threads the store holds that no Pane is showing — what a restart finds,
    /// and what "reopen" reopens.
    pub fn parked(&self) -> io::Result<Vec<ThreadId>> {
        Ok(self
            .store
            .thread_ids()?
            .into_iter()
            .filter(|id| !self.threads.contains_key(id))
            .collect())
    }

    /// A parked Thread's binding, read from its log's header; `None` for an
    /// open Thread (its view has it) or one the Store cannot read.
    pub fn parked_workspace(&self, thread: ThreadId) -> Option<WorkspaceBinding> {
        if self.threads.contains_key(&thread) {
            return None;
        }
        self.store.peek(thread).ok()?.workspace
    }

    /// A parked Thread whose worktree was removed for good
    /// (`worktree.remove`) is bound to its repo's main checkout, so a
    /// revive does not rebuild the tree on demand, and the worktree menu
    /// forgets the tree. Refused for an open Thread: its Session works
    /// there.
    pub fn retire_worktree(&mut self, thread: ThreadId) -> io::Result<()> {
        if self.threads.contains_key(&thread) {
            return Err(io::Error::other("the Thread is open"));
        }
        let meta = self.store.peek(thread).map_err(io::Error::other)?;
        let Some(WorkspaceBinding::Worktree { repo, path }) = meta.workspace else {
            return Ok(());
        };
        self.parked_listing.stale = true;
        let main = WorkspaceBinding::Main { checkout: repo };
        self.store
            .set_workspace(thread, &main, None)
            .map_err(io::Error::other)?;
        self.registry.remove_worktree(&path)
    }

    /// Every Thread, open or parked, working in the worktree at `path`,
    /// however the path is spelled: what `worktree.remove` parks and
    /// retires. Read while the tree is still on disk.
    pub fn threads_in_worktree(&self, path: &Path) -> io::Result<Vec<ThreadId>> {
        let here = |binding: Option<&WorkspaceBinding>| match binding {
            Some(WorkspaceBinding::Worktree { path: bound, .. }) => {
                workspace::same_dir(bound, path)
            }
            _ => false,
        };
        let mut threads: Vec<ThreadId> = self
            .threads
            .iter()
            .filter(|(_, state)| here(state.workspace.as_ref()))
            .map(|(thread, _)| *thread)
            .collect();
        for thread in self.parked()? {
            if here(self.parked_workspace(thread).as_ref()) {
                threads.push(thread);
            }
        }
        Ok(threads)
    }

    /// The registry forgets the worktree at `path`, removed for good with
    /// no Thread bound to it (`retire_worktree` forgets the others').
    pub fn forget_worktree(&mut self, path: &Path) -> io::Result<()> {
        self.removed_worktrees.push(path.to_owned());
        self.registry.remove_worktree(path)
    }

    /// The worktrees removed from disk since the last call: by
    /// `worktree.remove` (`forget_worktree`) or with a deleted Thread.
    pub fn take_removed_worktrees(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.removed_worktrees)
    }

    /// Whether some Thread's Main just finished making a worktree and is
    /// waiting on a fresh listing to name it — the driver's cue to list
    /// now rather than on its next tick.
    pub fn wants_worktree_listing(&self) -> bool {
        self.threads
            .values()
            .any(|state| state.follow.wants_listing())
    }

    /// Git's worktree list for a Thread's repo (`workspace::worktree_paths`
    /// of the binding's `repo`), as the driver read it off the frame — the
    /// cockpit never runs git in its pump — and `taken_at`, the driver's
    /// clock from before it asked git, so a listing that was already being
    /// read when a creation finished is not mistaken for one that could
    /// name it. Answers whether the Thread's
    /// binding moved: into a worktree its agent made or has been working
    /// in, or back to main when a worktree the agent adopted is gone. A
    /// worktree Ferrite minted is never abandoned for vanishing — revive
    /// recreates those on their branch.
    pub fn worktrees_listed(
        &mut self,
        thread: ThreadId,
        listed: Vec<PathBuf>,
        taken_at: Instant,
    ) -> bool {
        let Some(state) = self.threads.get_mut(&thread) else {
            return false;
        };
        let adopted = match &state.workspace {
            Some(WorkspaceBinding::Worktree { path, .. })
                if self.registry.branch_for(path).is_none() =>
            {
                Some(path.clone())
            }
            _ => None,
        };
        let Some(moved) = state
            .follow
            .observe_listing(listed, taken_at, adopted.as_deref())
        else {
            return false;
        };
        apply_move(&self.store, thread, state, moved)
    }

    pub fn threads(&self) -> Vec<ThreadId> {
        self.threads.keys().copied().collect()
    }

    /// The process is quitting: end every Session now, replacements and
    /// pending startups included, so no provider process outlives Ferrite.
    /// A Codex app-server left running holds its thread's writer lock until
    /// it notices its stdin is gone; the next launch's resume would meet it.
    /// Threads stay as they are — their logs are durable — and the Cockpit
    /// is not used again.
    pub fn halt_sessions(&mut self) {
        let live = self
            .threads
            .values()
            .filter(|state| state.session.is_some())
            .count();
        crate::log!("quitting: {live} sessions ended");
        self.bootstraps.clear();
        for state in self.threads.values_mut() {
            state.replacement = None;
            state.end_session();
        }
        // reviewr's private index copies leave each checkout's git dir.
        ferrite_reviewr::git::end_sessions();
        // Every record accepted is written before Ferrite goes (E1); only
        // the final syncs are bounded.
        self.store.quit(QUIT_SYNC_DEADLINE);
    }

    pub fn rename_thread(&mut self, thread: ThreadId, title: &str) -> io::Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Thread title cannot be blank",
            ));
        }
        self.parked_listing.stale = true;
        let mut state = self.threads.get_mut(&thread);
        if let Some(state) = state.as_mut() {
            state.history.clear();
        }
        self.store
            .set_title(
                thread,
                title.to_string(),
                state.as_mut().map(|state| &mut state.writer),
            )
            .map_err(io::Error::other)?;
        if let Some(state) = state {
            state.title = Some(title.to_string());
            // The live Session's own list follows, where its wire allows;
            // a refusal is cosmetic and the next spawn carries the title
            // anyway.
            if let Some(session) = state
                .session
                .as_mut()
                .and_then(SessionLifecycle::session_mut)
            {
                let _ = session.set_name(title);
            }
        }
        Ok(())
    }

    /// Settle startup transactions before reading provider events. Background
    /// workers only construct Sessions; all delivery and durable changes
    /// happen here, under the same owner as synchronous adapters.
    fn advance_startups(&mut self) -> Vec<PaneUpdate> {
        let mut changed = Vec::new();
        let ids: Vec<_> = self.threads.keys().copied().collect();
        for id in ids {
            let replacement = self
                .threads
                .get_mut(&id)
                .and_then(|state| state.replacement.take());
            if let Some(mut replacement) = replacement {
                // A handover whose exchanges are being read was ready already.
                let ready = if replacement.exchanges.is_some() {
                    Ok(true)
                } else {
                    replacement
                        .session
                        .as_mut()
                        .map_or(Ok(true), SessionLifecycle::poll)
                };
                let result = match ready {
                    Ok(false) => {
                        self.threads.get_mut(&id).expect("exists").replacement = Some(replacement);
                        continue;
                    }
                    Ok(true) => {
                        if replacement.exchanges.is_none() {
                            if let (Some(title), Some(session)) = (
                                self.threads[&id].title.as_deref(),
                                replacement
                                    .session
                                    .as_mut()
                                    .and_then(SessionLifecycle::session_mut),
                            ) {
                                let _ = session.set_name(title);
                            }
                        }
                        match self.carry_read(id, &mut replacement) {
                            Ok(Some(exchanges)) => {
                                self.commit_replacement(id, replacement, exchanges)
                            }
                            Ok(None) => {
                                self.threads.get_mut(&id).expect("exists").replacement =
                                    Some(replacement);
                                continue;
                            }
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(ProvisionError::Spawn(error)),
                };
                if let Err(error) = result {
                    crate::log!("thread {id}: session change failed: {error}");
                    self.threads
                        .get_mut(&id)
                        .expect("exists")
                        .apply(Input::Notice(format!("Session change failed: {error}")));
                }
                changed.push(id);
            }
            let state = self.threads.get_mut(&id).expect("exists");
            let starting = state
                .session
                .as_ref()
                .is_some_and(SessionLifecycle::is_starting);
            if starting {
                let result = state.session.as_mut().expect("exists").poll();
                if matches!(result, Ok(false)) {
                    continue;
                }
                if let Some(pending) = self.bootstraps.remove(&id) {
                    let delivered = result.and_then(|_| self.finish_bootstrap(id, &pending));
                    let result = match delivered {
                        // A draft's own slot and deferred leave are its
                        // window's: `Cockpit::pump` completes them.
                        Ok(()) => Ok(Bootstrapped {
                            thread: id,
                            refused_leave: None,
                            applied_leave: false,
                        }),
                        Err(error) => match self.rollback_bootstrap(id) {
                            Ok(()) => Err(error),
                            Err(rollback) => Err(io::Error::other(format!(
                                "{error}; rollback failed: {rollback}"
                            ))),
                        },
                    };
                    self.bootstrap_results.push(BootstrapResult {
                        draft: pending.draft,
                        prompt: pending.prompt,
                        result,
                    });
                    changed.push(id);
                    continue;
                }
                let state = self.threads.get_mut(&id).expect("exists");
                match result {
                    Ok(_) => {
                        if let (Some(title), Some(session)) = (
                            &state.title,
                            state
                                .session
                                .as_mut()
                                .and_then(SessionLifecycle::session_mut),
                        ) {
                            let _ = session.set_name(title);
                        }
                    }
                    Err(error) => {
                        state.end_session();
                        state.replace_generation();
                        crate::log!("thread {id}: session could not start: {error}");
                        state.apply(Input::Notice(format!("could not start: {error}")));
                        state.give_up_awaiting();
                    }
                }
                changed.push(id);
            }
            let Some(state) = self.threads.get_mut(&id) else {
                continue;
            };
            if let Some(session) = state.session.as_ref().and_then(SessionLifecycle::session) {
                state.announced.ask(session);
            }
            if self.serve_awaiting(id) {
                changed.push(id);
            }
        }
        changed.sort_unstable();
        changed.dedup();
        changed
            .into_iter()
            .map(|thread| PaneUpdate {
                thread,
                dirty: self
                    .threads
                    .get(&thread)
                    .map(|state| {
                        state
                            .transcript()
                            .blocks()
                            .iter()
                            .map(|block| block.id)
                            .collect()
                    })
                    .unwrap_or_default(),
                evicted: Vec::new(),
                subjects: Vec::new(),
                activity_changed: true,
                redirects: Vec::new(),
            })
            .collect()
    }

    /// One frame: drain every live Session, fold what arrived, and write it
    /// down. What comes back is only the Panes that actually changed.
    pub fn pump(&mut self) -> Vec<PaneUpdate> {
        self.poll_model_discovery();
        // Predictions that finished between frames. Collected before the
        // Thread walk so a landing suggestion repaints in the same frame.
        let landed = self.poll_suggestions();
        let mut frame = self.advance_startups();
        // Threads whose Main turn ended this frame, and which therefore owe
        // themselves a fresh prediction. Gathered here and asked after the
        // walk: the walk holds `threads` mutably.
        let mut ended: Vec<ThreadId> = Vec::new();
        frame.extend(self.advance_history());
        let mut history_reloads = Vec::new();
        for (id, thread) in &mut self.threads {
            let mut update = PaneUpdate::new(*id);
            // Fold answers from the previous frame before asking the Lexer
            // for new work; highlighted Blocks remain an explicit next-frame wake.
            let highlighted = thread.activity.apply(ActivityInput::DrainHighlights);
            update.absorb(highlighted, false);
            // A log the worker cannot sync holds the Session's events too:
            // nothing more is accepted than the disk can keep.
            if let Some(failure) = thread.writer.failure() {
                thread.report_store_error(io::Error::other(failure));
                update.activity_changed = true;
                frame.push(update);
                continue;
            }
            // Backpressure on failed persistence: retain the writer's buffer
            // and let the bounded provider channel fill rather than lose history.
            if thread.store_error.is_some() {
                match thread.writer.flush() {
                    Ok(()) => {
                        thread.store_error = None;
                        update.activity_changed = true;
                    }
                    Err(error) => {
                        thread.report_store_error(error);
                        update.activity_changed = true;
                        frame.push(update);
                        continue;
                    }
                }
            }
            if thread.history_backpressure() {
                if update.activity_changed {
                    frame.push(update);
                }
                continue;
            }
            // A handover reading its exchanges: the old Session's events wait,
            // so none lands between what is read and the switch's line.
            if thread
                .replacement
                .as_ref()
                .is_some_and(|replacement| replacement.exchanges.is_some())
            {
                if update.activity_changed {
                    frame.push(update);
                }
                continue;
            }
            let mut closed = false;
            let mut settled = false;
            let mut turn_ended = false;
            for _ in 0..256 {
                if thread.store_error.is_some() || thread.history_backpressure() {
                    break;
                }
                let Some(event) = thread
                    .session
                    .as_ref()
                    .and_then(SessionLifecycle::session)
                    .and_then(|session| session.events().try_recv().ok())
                else {
                    break;
                };
                if let SessionEvent::Queue(event) = event {
                    let change = thread.native_queue.observe(event);
                    if let Some(text) = change.prompt {
                        if let Err(error) = thread.writer.record_prompt(&text) {
                            thread.report_store_error(error);
                        }
                        thread.prompt_history.append(text.clone());
                        if change.historical {
                            update.absorb(
                                thread
                                    .activity
                                    .apply(ActivityInput::Replay(Input::Prompt(text))),
                                true,
                            );
                        } else {
                            thread.invalidate_suggestion();
                            let sent_at = crate::clock::now_label();
                            if let Err(error) = thread.writer.record_prompt_observation(&sent_at) {
                                thread.report_store_error(error);
                            }
                            let applied = thread.apply(Input::Prompt(text));
                            update.dirty.extend(applied.dirty);
                            let stamped = thread.apply(Input::PromptObservation { sent_at });
                            update.dirty.extend(stamped.dirty);
                        }
                    }
                    if let Some(notice) = change.notice {
                        update
                            .dirty
                            .extend(thread.apply(Input::Notice(notice)).dirty);
                    }
                    update.activity_changed = true;
                    continue;
                }
                if let SessionEvent::RateLimits { five_hour, weekly } = &event {
                    let before = self.limit_cache.get(thread.provider);
                    let changed = self.limit_cache.remember(
                        thread.provider,
                        crate::transcript::RateLimits {
                            five_hour: *five_hour,
                            weekly: *weekly,
                        },
                    );
                    if changed {
                        self.allowance_warnings
                            .extend(crate::providers::limits::warnings(
                                thread.provider,
                                before,
                                self.limit_cache.get(thread.provider),
                                &self.allowance_thresholds,
                            ));
                    }
                }
                if let SessionEvent::Models { models } = &event {
                    self.model_cache.remember(thread.provider, models);
                    // Refresh this Session's effort menu even when its catalog
                    // matches the saved provider menu. Child Activity payloads
                    // cannot announce models or alter Main's provider metadata.
                    self.models_changed |= !models.is_empty();
                }
                let alias_needs_reload = match &event {
                    SessionEvent::Activity(ActivityEvent::Alias { from, to }) => {
                        let view = thread.activity.view();
                        [from, to].iter().any(|key| {
                            let subject = Subject::Subagent((*key).clone());
                            view.subject(&subject)
                                .is_some_and(|subject| subject.alias_needs_history())
                                || thread.history.keys().any(|pending| {
                                    view.canonical_subject(&Subject::Subagent(pending.clone()))
                                        == view.canonical_subject(&subject)
                                })
                        })
                    }
                    _ => false,
                };
                let completion = completion_observation(thread, &event);
                fold(thread, &event);
                if let Some(moved) = follow_move(thread, &event) {
                    if apply_move(&self.store, *id, thread, moved) {
                        update.activity_changed = true;
                    }
                }
                let content_changed = event_changes_content(&event);
                let applied = match &event {
                    SessionEvent::Activity(event) => {
                        thread.activity.apply(ActivityInput::Observe {
                            generation: thread.generation,
                            event: event.clone(),
                            at: crate::clock::instant(),
                        })
                    }
                    _ => thread.activity.apply(ActivityInput::Main {
                        input: Input::Event(event.clone()),
                        at: crate::clock::instant(),
                    }),
                };
                turn_ended |= applied.main_turn_ended;
                settled |= applied.main_settled;
                if matches!(&event, SessionEvent::Activity(_)) {
                    for accepted in &applied.accepted {
                        if !matches!(accepted, ActivityEvent::Status { .. })
                            || !applied.blocks.is_empty()
                        {
                            thread.buffer_history(accepted, applied.completed_tool_duration);
                        }
                        let event = SessionEvent::Activity(accepted.clone());
                        let duration = applied.completed_tool_duration;
                        if let Err(error) = thread.writer.record_event(&event, duration) {
                            thread.report_store_error(error);
                        }
                    }
                } else {
                    let duration = applied.completed_tool_duration;
                    if let Err(error) = thread.writer.record_event(&event, duration) {
                        thread.report_store_error(error);
                    }
                }
                // An agent blocked on the operator may wait for hours:
                // everything it did before it asked is on disk meanwhile.
                if changes_decisions(&event) {
                    if let Err(error) = thread.writer.flush() {
                        thread.report_store_error(error);
                    }
                }
                let completion_accepted = match &event {
                    SessionEvent::Activity(event) => {
                        applied.accepted.iter().any(|accepted| accepted == event)
                    }
                    _ => applied.main_turn_ended,
                };
                if completion_accepted {
                    if let Some((subject, observed_facts)) = completion {
                        let observation = ActivityEvent::CompletionObservation {
                            subject: subject.clone(),
                            elapsed_ms: observed_facts.elapsed_ms,
                            completed_at: observed_facts.completed_at.clone(),
                            input_tokens: observed_facts.input_tokens,
                            output_tokens: observed_facts.output_tokens,
                        };
                        if matches!(&subject, Subject::Subagent(_)) {
                            thread.buffer_history(&observation, None);
                        }
                        let observed = match subject {
                            Subject::Main => thread.activity.apply(ActivityInput::Main {
                                input: Input::CompletionObservation {
                                    elapsed_ms: observed_facts.elapsed_ms,
                                    completed_at: observed_facts.completed_at.clone(),
                                    input_tokens: observed_facts.input_tokens,
                                    output_tokens: observed_facts.output_tokens,
                                },
                                at: crate::clock::instant(),
                            }),
                            Subject::Subagent(_) => thread.activity.apply(ActivityInput::Observe {
                                generation: thread.generation,
                                event: observation,
                                at: crate::clock::instant(),
                            }),
                        };
                        if let Err(error) =
                            thread.writer.record_completion(&subject, &observed_facts)
                        {
                            thread.report_store_error(error);
                        }
                        update.absorb(observed, true);
                    }
                }
                if alias_needs_reload {
                    for (_, canonical) in &applied.redirects {
                        let subject = Subject::Subagent(canonical.clone());
                        let view = thread.activity.view();
                        thread.history.retain(|key, _| {
                            view.canonical_subject(&Subject::Subagent(key.clone())) != subject
                        });
                        thread.history_errors.remove(canonical);
                        update.absorb(
                            thread.activity.apply(ActivityInput::Evict(subject.clone())),
                            true,
                        );
                        history_reloads.push((*id, subject));
                    }
                }
                if let SessionEvent::Closed { reason } = &event {
                    crate::log!("thread {id}: session ended: {reason}");
                    closed = true;
                }
                update.absorb(applied, content_changed);
            }
            if turn_ended && std::mem::take(&mut thread.resend_held_on_turn_end) {
                if let Some(applied) = resend_held(thread, self.suggestions_enabled) {
                    update.dirty.extend(applied.dirty);
                    update.activity_changed = true;
                }
            }
            if closed {
                thread.resend_held_on_turn_end = false;
                if let Some(notice) = thread
                    .native_queue
                    .disconnect(thread.provider == Provider::Codex)
                {
                    update
                        .dirty
                        .extend(thread.apply(Input::Notice(notice)).dirty);
                }
                thread.end_session();
                thread.history.clear();
                thread.activity.apply(ActivityInput::Disconnect);
                self.visible_subjects.remove(id);
                update.activity_changed = true;
            }
            // Every frame, every Thread it holds: the deferral's grace is a
            // clock only the pump ticks.
            if thread.held {
                let born = self.notifications.observe(
                    *id,
                    Frame {
                        activity: thread.activity.view(),
                        settled,
                        resumed: thread.native_queue.pending(),
                    },
                    crate::clock::instant(),
                );
                update.activity_changed |= born.is_some();
            }
            if turn_ended && thread.provider == Provider::Claude {
                thread.invalidate_suggestion();
            }
            if let Some(session) = thread
                .session
                .as_mut()
                .and_then(SessionLifecycle::session_mut)
            {
                if let Some(text) = session.take_suggestion() {
                    if self.suggestions_enabled
                        && !thread.busy()
                        && !thread.native_queue.pending()
                        && !thread.activity.view().main_operator_turn()
                        && thread.transcript().status() == crate::transcript::Status::Idle
                    {
                        thread.suggestion = Some(text);
                        update.activity_changed = true;
                    }
                }
            }
            if turn_ended {
                ended.push(*id);
                thread
                    .writer
                    .note_subagents(thread.activity.view().children().len());
            }
            if update.activity_changed || !update.dirty.is_empty() || !update.subjects.is_empty() {
                frame.push(update);
            }
        }
        // After the walk: whatever the provider said before its answer to a
        // setting change is in the transcript ahead of the change's line.
        frame.extend(self.settle_tuning());
        // After the walk: a Thread settles on the frame its last event lands.
        self.end_unused_sessions();
        // A landed prediction changes only the Composer's idle line, so it
        // owes its Pane a repaint and nothing else.
        for id in landed {
            if !frame.iter().any(|update| update.thread == id) {
                let mut update = PaneUpdate::new(id);
                update.activity_changed = true;
                frame.push(update);
            }
        }
        for id in ended {
            self.request_suggestion(id);
        }
        self.publish_turn_agents();
        for (thread, subject) in history_reloads {
            if let Err(error) = self.ensure_subject_history(thread, &subject) {
                if let (Some(state), Subject::Subagent(key)) =
                    (self.threads.get_mut(&thread), subject)
                {
                    state.history_errors.insert(key, error.to_string());
                }
            }
        }
        frame
    }

    /// Tell reviewr's turn tracking where each live Session works and
    /// whether it is working, when that changed: a review's last turn is
    /// the checkout since the latest turn that changed it began (ADR 0021).
    fn publish_turn_agents(&mut self) {
        use crate::transcript::Status as Turn;
        use ferrite_reviewr::turn::Status;
        let agents: Vec<_> = self
            .threads
            .iter()
            // A Session ended unseen, or let go to its digest, is still the
            // Thread's agent (ADR 0032).
            .filter(|(_, state)| {
                state.session.is_some() || !state.held || state.activity.view().connected()
            })
            .filter_map(|(id, state)| {
                let cwd = crate::workspace::effective_cwd(
                    state.session_project_root.as_deref(),
                    state.workspace.as_ref(),
                )?;
                let status = match state.transcript().status() {
                    Turn::Idle => Status::Idle,
                    Turn::Streaming => Status::Working,
                    Turn::Blocked => Status::Blocked,
                    Turn::Closed => Status::Done,
                };
                Some(ferrite_reviewr::ferrite::Agent {
                    thread: id.to_string(),
                    cwd: cwd.to_path_buf(),
                    status,
                })
            })
            .collect();
        if agents != self.turn_agents {
            self.turn_agents = agents.clone();
            self.turns.agents_changed(agents, crate::clock::instant());
        }
    }

    /// Submit immediately; the provider owns admission and execution.
    /// False leaves the caller's draft intact; no local fallback is installed.
    pub fn queue(&mut self, thread: ThreadId, text: String) -> bool {
        let Some(state) = self.threads.get_mut(&thread) else {
            return false;
        };
        state.prompt_history.reset();
        if let Some(refusal) = vanished_root_refusal(state) {
            state.apply(Input::Notice(refusal));
            return false;
        }
        if !self.start_or_say(thread, "queue failed") {
            return false;
        }
        let state = self.threads.get_mut(&thread).expect("checked");
        let ready = state
            .session
            .as_mut()
            .and_then(SessionLifecycle::session_mut)
            .is_some();
        if !ready || !state.awaiting.prompts.is_empty() {
            state.awaiting.prompts.push_back(text);
            state.invalidate_suggestion();
            return true;
        }
        match self.enqueue(thread, &text) {
            Enqueued::Taken => true,
            Enqueued::NotYet(error) => {
                let state = self.threads.get_mut(&thread).expect("checked");
                state.apply(Input::Notice(format!("native queue failed: {error}")));
                false
            }
            Enqueued::Failed => false,
        }
    }

    /// Hand `text` to the provider's queue.
    fn enqueue(&mut self, thread: ThreadId, text: &str) -> Enqueued {
        let state = self.threads.get_mut(&thread).expect("checked by caller");
        let id = &format!(
            "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
            std::process::id(),
            state.generation & 0xffff,
            (state.generation >> 16) & 0xfff,
            next_generation() & 0xfff,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                & 0xffffffffffff
        );
        if let Err(error) = state.native_queue.prepare(id, text) {
            state.native_queue.refused(id);
            state.apply(Input::Notice(format!(
                "queue metadata could not be saved: {error}"
            )));
            return Enqueued::Failed;
        }
        let result = state
            .session
            .as_mut()
            .and_then(SessionLifecycle::session_mut)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "Session is still starting; retry when ready",
                )
            })
            .and_then(|session| {
                session.set_suggestions_enabled(self.suggestions_enabled)?;
                session.enqueue(id, text)
            });
        if let Err(error) = result {
            if matches!(
                error.kind(),
                io::ErrorKind::Unsupported | io::ErrorKind::NotConnected
            ) {
                state.native_queue.refused(id);
                return Enqueued::NotYet(error);
            }
            // A partial pipe write might have reached the provider. Keep its
            // correlation metadata and never turn uncertainty into a resend.
            state.apply(Input::Notice(format!(
                "native queue delivery is uncertain; not retried: {error}\n{text}"
            )));
        }
        state.invalidate_suggestion();
        Enqueued::Taken
    }

    /// Each frame, what waits for `thread`'s Session goes to it once it can
    /// take it. Answers whether any prompt went out, or back to the
    /// Composer.
    fn serve_awaiting(&mut self, thread: ThreadId) -> bool {
        if self.start_for_awaiting(thread) {
            return true;
        }
        self.release_asks(thread);
        self.release_prompts(thread)
    }

    /// Prompts sent while a change was recorded for no Session start the
    /// next; if it cannot start they go back to the Composer, and the
    /// answer is that they did.
    fn start_for_awaiting(&mut self, thread: ThreadId) -> bool {
        let state = self.threads.get_mut(&thread).expect("checked by caller");
        if state.session.is_some()
            || state.replacement.is_some()
            || state.awaiting.prompts.is_empty()
            || self.start_or_say(thread, "send failed")
        {
            return false;
        }
        let state = self.threads.get_mut(&thread).expect("checked by caller");
        state.give_up_awaiting();
        true
    }

    /// Prompts that waited for `thread`'s Session go out once it can take
    /// them, oldest first: as a turn when none runs or waits, else into
    /// the provider's queue. One the provider cannot queue yet waits for
    /// the next frame, and everything behind it with it. Answers whether
    /// any went out.
    fn release_prompts(&mut self, thread: ThreadId) -> bool {
        let mut released = false;
        loop {
            let state = self.threads.get_mut(&thread).expect("checked by caller");
            if state.ready_session().is_none() {
                return released;
            }
            let Some(text) = state.awaiting.prompts.pop_front() else {
                return released;
            };
            if !state.turn_ahead() {
                if let Err(error) = deliver(state, text.clone(), self.suggestions_enabled) {
                    state.undelivered(text, error);
                }
                released = true;
                continue;
            }
            match self.enqueue(thread, &text) {
                Enqueued::Taken => released = true,
                // Its draft is gone: it goes back to the Composer.
                Enqueued::Failed => {
                    let state = self.threads.get_mut(&thread).expect("checked");
                    state.native_queue.recovered.push((text, false));
                    released = true;
                }
                Enqueued::NotYet(_) => {
                    let state = self.threads.get_mut(&thread).expect("checked");
                    state.awaiting.prompts.push_front(text);
                    return released;
                }
            }
        }
    }

    /// Empty-Composer Enter requests cancellation; retrieval arrives via the pump.
    pub fn unqueue(&mut self, thread: ThreadId) -> Option<String> {
        if let Some(state) = self.threads.get_mut(&thread) {
            if let Some(text) = state.awaiting.prompts.pop_back() {
                return Some(text);
            }
        }
        self.cancel_queued(thread, true);
        None
    }

    pub fn cancel_queued(&mut self, thread: ThreadId, restore: bool) {
        self.cancel_queue(thread, restore, false);
    }

    /// Up edits the visible queued prompt ahead of the current draft.
    pub fn edit_queued(&mut self, thread: ThreadId) {
        self.cancel_queue(thread, true, true);
    }

    fn cancel_queue(&mut self, thread: ThreadId, restore: bool, prepend: bool) {
        let Some(state) = self.threads.get_mut(&thread) else {
            return;
        };
        state.prompt_history.reset();
        // Input still waiting for the Session never reached the provider
        // and can be taken back locally.
        if let Some(text) = state.awaiting.prompts.pop_back() {
            if restore {
                state.native_queue.recovered.push((text, prepend));
            }
            return;
        }
        let Some(id) = state.native_queue.cancellation(restore, prepend) else {
            return;
        };
        let result = state
            .session
            .as_mut()
            .and_then(SessionLifecycle::session_mut)
            .ok_or_else(|| io::Error::other("no live Session to cancel queued input"))
            .and_then(|session| session.cancel_queued(&id));
        if let Err(error) = result {
            state.native_queue.cancel_failed();
            state.apply(Input::Notice(format!(
                "native cancellation failed: {error}"
            )));
        }
    }

    pub fn take_retrieved_prompt(&mut self, thread: ThreadId) -> Option<(String, bool)> {
        let state = self.threads.get_mut(&thread)?;
        if state.native_queue.recovered.is_empty() {
            None
        } else {
            Some(state.native_queue.recovered.remove(0))
        }
    }

    pub fn recall_prompt(
        &mut self,
        thread: ThreadId,
        direction: HistoryDirection,
        current_draft: &str,
    ) -> Option<String> {
        self.threads
            .get_mut(&thread)?
            .prompt_history
            .recall(direction, current_draft)
    }

    /// Take the pending Decision, if `id` is really what this Thread is
    /// waiting on. False means the answer is stale — the request was already
    /// answered, or the turn ended under it — and must not reach the provider,
    /// which would either ignore it or, worse, apply it to the next request.
    pub fn answer(&mut self, thread: ThreadId, id: &str) -> bool {
        let Some(state) = self.threads.get_mut(&thread) else {
            return false;
        };
        let Some(handle) = state
            .activity
            .view()
            .decisions()
            .iter()
            .find(|pending| pending.decision.id == id)
            .map(|pending| pending.handle.clone())
        else {
            return false;
        };
        state.activity.apply(ActivityInput::Observe {
            generation: state.generation,
            event: ActivityEvent::DecisionCancelled {
                id: handle.request_id,
            },
            at: crate::clock::instant(),
        });
        if let Err(error) = state.writer.flush() {
            state.report_store_error(error);
        }
        true
    }

    /// Fold in whatever predictions finished since the last frame.
    ///
    /// A reply is dropped unless it still names a live Thread at the
    /// generation it was asked for: a park, revive or provider switch since
    /// the ask means it is answering a conversation that is gone.
    fn poll_suggestions(&mut self) -> Vec<ThreadId> {
        let mut landed = Vec::new();
        while let Ok(reply) = self.suggestions.1.try_recv() {
            if !self.suggestions_enabled {
                continue;
            }
            let Some(thread) = self.threads.get_mut(&reply.thread) else {
                continue;
            };
            if thread.generation != reply.generation
                || thread.suggestion_revision != reply.revision
                || thread.native_queue.pending()
            {
                continue;
            }
            thread.suggestion = Some(reply.text);
            landed.push(reply.thread);
        }
        landed
    }

    /// Ask for the follow-up to this Thread's just-finished response.
    ///
    /// Called exactly once per finished turn. The old prediction is dropped
    /// first: it described the previous response, and showing it beside a new
    /// one would be worse than the generic line the Thread falls back to for
    /// the seconds the ask takes.
    fn request_suggestion(&mut self, id: ThreadId) {
        let Some(thread) = self.threads.get_mut(&id) else {
            return;
        };
        if thread.provider == Provider::Claude {
            return;
        }
        thread.invalidate_suggestion();
        if !self.suggestions_enabled || thread.native_queue.pending() {
            return;
        }
        let Some(context) = crate::suggest::context(thread.transcript()) else {
            return;
        };
        let request = crate::suggest::Request {
            thread: id,
            generation: thread.generation,
            revision: thread.suggestion_revision,
            provider: thread.provider,
            context,
        };
        self.spawner.suggest(request, self.suggestions.0.clone());
    }

    /// Land a prediction as if the predictor had just answered, stamped with
    /// the Thread's current generation.
    ///
    /// The renderer's seam for anything that produces a follow-up without
    /// going through a provider run — today, the tests that cover the
    /// Composer's accept key end to end.
    pub fn deliver_suggestion(&self, thread: ThreadId, text: String) {
        let Some(state) = self.threads.get(&thread) else {
            return;
        };
        let _ = self.suggestions.0.send(Suggestion {
            thread,
            generation: state.generation,
            revision: state.suggestion_revision,
            text,
        });
    }

    /// Enable or disable predicted Composer follow-ups. Disabling is
    /// immediate: visible predictions are cleared and late replies are
    /// discarded by `poll_suggestions`.
    pub fn set_suggestions_enabled(&mut self, enabled: bool) {
        self.suggestions_enabled = enabled;
        for thread in self.threads.values_mut() {
            if !enabled {
                thread.invalidate_suggestion();
            }
            if let Some(session) = thread
                .session
                .as_mut()
                .and_then(SessionLifecycle::session_mut)
            {
                let _ = session.set_suggestions_enabled(enabled);
            }
        }
    }

    fn poll_model_discovery(&mut self) {
        loop {
            let Some(receiver) = &self.model_discovery else {
                return;
            };
            match receiver.try_recv() {
                Ok((provider, models)) => {
                    self.models_changed |= self.model_cache.remember(provider, &models);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.model_discovery = None;
                    return;
                }
            }
        }
    }

    /// A provider menu changed, including before there is a Pane Session
    /// to repaint. The view also refreshes any already-open model picker.
    pub fn take_models_changed(&mut self) -> bool {
        std::mem::take(&mut self.models_changed)
    }

    /// Ask the provider menus again, as at startup — after a CLI upgrade,
    /// whose new release may announce models the old one did not.
    pub fn rediscover_models(&mut self) {
        self.model_discovery = self.spawner.discover_models();
    }

    /// How many Threads on `provider` hold a Session, running, starting or
    /// being replaced. A CLI upgrade waits for none, so it never swaps a
    /// binary out from under a live Session (Windows refuses to, mid-run).
    pub fn live_sessions(&self, provider: Provider) -> usize {
        self.threads
            .values()
            .filter(|state| {
                state.provider == provider
                    && (state.session.is_some() || state.replacement.is_some())
            })
            .count()
    }

    /// The last usable provider announcement, including one saved in an
    /// earlier launch. Fresh discovery or Session announcements replace it.
    pub fn announced_models(&self, provider: Provider) -> Vec<ModelInfo> {
        self.model_cache.get(provider).to_vec()
    }

    /// The subscription windows in force for `provider` — the live
    /// Session's latest report, else the one a previous Session or launch
    /// left behind, and unknown once a remembered window has rolled over.
    /// Account-wide, so every Thread on the provider reads the same meter.
    pub fn account_limits(&self, provider: Provider) -> crate::transcript::RateLimits {
        self.limit_cache.get(provider)
    }

    /// The use, in percent, at which a rising window warns the operator.
    /// Empty turns the warnings off.
    pub fn set_allowance_thresholds(&mut self, thresholds: Vec<u8>) {
        self.allowance_thresholds = thresholds;
    }

    /// Request workspace commands without creating a Thread or Session.
    pub fn discover_commands(
        &mut self,
        provider: Provider,
        cwd: &std::path::Path,
    ) -> Option<crate::providers::commands::Discovery> {
        self.spawner.discover_commands(provider, cwd)
    }

    /// The provider's announced model rows, else the fallback catalog.
    pub fn model_catalog(&self, provider: Provider) -> Vec<ModelInfo> {
        crate::providers::models::catalog(provider, &self.announced_models(provider))
    }

    /// Change an idle Thread's model through its live native Session control.
    /// Unsupported controls fall back to resuming the same conversation;
    /// provider refusals preserve the current Session and durable choice.
    /// A provider that answers later holds the change pending
    /// (`settle_tuning`): nothing moves until it accepts.
    pub fn set_model(
        &mut self,
        thread: ThreadId,
        model: Option<String>,
    ) -> Result<(), ProvisionError> {
        self.past_unseen_start(thread, |engine| engine.change_model(thread, model))
    }

    fn change_model(
        &mut self,
        thread: ThreadId,
        model: Option<String>,
    ) -> Result<(), ProvisionError> {
        let effort = self.tuning(thread)?.1;
        let Some(state) = self.threads.get_mut(&thread) else {
            return self
                .store
                .set_tuning(thread, model, effort, None)
                .map_err(ProvisionError::Store);
        };
        state.tunable()?;
        if state.model == model && model.is_some() {
            return Ok(());
        }
        let native = state
            .session
            .as_mut()
            .and_then(SessionLifecycle::session_mut)
            .map(|session| session.set_model(model.as_deref()));
        match native {
            Some(Ok(ack)) => self.await_tuning(thread, Tune::Model(model), ack),
            Some(Err(error)) if error.kind() != io::ErrorKind::Unsupported => {
                Err(ProvisionError::Tune(error))
            }
            None | Some(Err(_)) => self.retune(thread, model, effort),
        }
    }

    /// Change the next turn's effort on the existing Session. A turn or
    /// Session startup/replacement must finish first; refusal preserves its
    /// submitted choices and prompts. No Session is replaced by this action.
    /// `None` is the operator's default for the provider.
    pub fn set_effort(
        &mut self,
        thread: ThreadId,
        effort: Option<String>,
    ) -> Result<(), ProvisionError> {
        self.past_unseen_start(thread, |engine| engine.change_effort(thread, effort))
    }

    fn change_effort(
        &mut self,
        thread: ThreadId,
        effort: Option<String>,
    ) -> Result<(), ProvisionError> {
        let Some(state) = self.threads.get_mut(&thread) else {
            let meta = self.store.peek(thread).map_err(ProvisionError::Store)?;
            return self
                .store
                .set_tuning(thread, meta.model, effort, None)
                .map_err(ProvisionError::Store);
        };
        state.tunable()?;
        if state.effort == effort && effort.is_some() {
            return Ok(());
        }
        match state
            .session
            .as_mut()
            .and_then(SessionLifecycle::session_mut)
        {
            Some(session) => {
                let ack = session
                    .set_effort(effort.as_deref())
                    .map_err(ProvisionError::Tune)?;
                self.await_tuning(thread, Tune::Effort(effort), ack)
            }
            None => self.commit_tuning(thread, Tune::Effort(effort)),
        }
    }

    /// Commit `change` once the provider accepts it: now, when it already
    /// has, else on the pump that finds its answer.
    fn await_tuning(
        &mut self,
        thread: ThreadId,
        change: Tune,
        ack: SettingAck,
    ) -> Result<(), ProvisionError> {
        match ack.try_recv() {
            Ok(answer) => {
                answer.map_err(ProvisionError::Tune)?;
                self.commit_tuning(thread, change)
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                let state = self.threads.get_mut(&thread).expect("checked by caller");
                state.tuning = Some(Tuning {
                    change,
                    ack,
                    asked: crate::clock::instant(),
                });
                Ok(())
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err(ProvisionError::Tune(
                io::Error::other(format!("{} change was not acknowledged", change.noun())),
            )),
        }
    }

    /// The provider took `change`: the Thread's choice and header follow,
    /// and the transcript says so. A header that cannot be written puts
    /// the Session back as it was.
    fn commit_tuning(&mut self, thread: ThreadId, change: Tune) -> Result<(), ProvisionError> {
        let state = self.threads.get_mut(&thread).expect("checked by caller");
        let (model, effort) = match &change {
            Tune::Model(model) => (model.clone(), state.effort.clone()),
            // Re-selecting Default reapplies the latest Settings even when
            // the durable choice was already None; the header stays.
            Tune::Effort(effort) if *effort == state.effort => return Ok(()),
            Tune::Effort(effort) => (state.model.clone(), effort.clone()),
        };
        state.history.clear();
        if let Err(error) = self.store.set_tuning(
            thread,
            model.clone(),
            effort.clone(),
            Some(&mut state.writer),
        ) {
            if let Some(session) = state
                .session
                .as_mut()
                .and_then(SessionLifecycle::session_mut)
            {
                // Put back, unanswered: the header still names the old.
                let _ = match change {
                    Tune::Model(_) => session.set_model(state.model.as_deref()),
                    Tune::Effort(_) => session.set_effort(state.effort.as_deref()),
                };
            }
            return Err(ProvisionError::Store(error));
        }
        let notice = match change {
            Tune::Model(_) => format!(
                "model changed to {}",
                crate::providers::models::label(
                    model.as_deref().unwrap_or("default"),
                    state.announced.models()
                )
            ),
            Tune::Effort(_) => format!(
                "effort changed to {}",
                effort.as_deref().unwrap_or("default")
            ),
        };
        state.apply(Input::Notice(notice));
        state.model = model;
        state.effort = effort;
        Ok(())
    }

    /// Settle the setting changes their providers have answered, or have
    /// left unanswered too long: committed, or refused in the transcript.
    fn settle_tuning(&mut self) -> Vec<PaneUpdate> {
        let waiting: Vec<ThreadId> = self
            .threads
            .iter()
            .filter(|(_, state)| state.tuning.is_some())
            .map(|(id, _)| *id)
            .collect();
        let mut settled = Vec::new();
        for id in waiting {
            let state = self.threads.get_mut(&id).expect("listed above");
            let tuning = state.tuning.as_ref().expect("listed above");
            let answer = match tuning.ack.try_recv() {
                Ok(answer) => answer,
                Err(std::sync::mpsc::TryRecvError::Empty)
                    if crate::clock::instant().duration_since(tuning.asked) < TUNE_TIMEOUT =>
                {
                    continue;
                }
                Err(_) => Err(io::Error::other(format!(
                    "{} change was not acknowledged",
                    tuning.change.noun()
                ))),
            };
            let change = state.tuning.take().expect("listed above").change;
            let noun = change.noun();
            let result = answer
                .map_err(ProvisionError::Tune)
                .and_then(|()| self.commit_tuning(id, change));
            let state = self.threads.get_mut(&id).expect("listed above");
            if let Err(error) = result {
                state.apply(Input::Notice(format!("{noun} unchanged: {error}")));
            }
            let mut update = PaneUpdate::new(id);
            update.activity_changed = true;
            settled.push(update);
        }
        settled
    }

    /// Make `change` to `thread` as if the agent it started unseen (ADR
    /// 0032), and nothing yet waits on, had not started: a change is
    /// recorded for the next agent rather than refused while one starts.
    /// The next starts at once if the operator is still writing.
    fn past_unseen_start(
        &mut self,
        thread: ThreadId,
        change: impl FnOnce(&mut Self) -> Result<(), ProvisionError>,
    ) -> Result<(), ProvisionError> {
        let dropped = self
            .threads
            .get_mut(&thread)
            .is_some_and(Thread::drop_unseen_start);
        let changed = change(self);
        if dropped && self.composing(thread) {
            self.wake(thread);
        }
        changed
    }

    /// A Thread's current model and effort, live or off the header.
    fn tuning(&self, thread: ThreadId) -> Result<(Option<String>, Option<String>), ProvisionError> {
        match self.threads.get(&thread) {
            Some(state) => Ok((state.model.clone(), state.effort.clone())),
            None => {
                let meta = self.store.peek(thread).map_err(ProvisionError::Store)?;
                Ok((meta.model, meta.effort))
            }
        }
    }

    /// Model changes: the header always; pre-lock the
    /// eager respawn; post-lock a respawn resuming the same conversation,
    /// refused mid-turn (`Busy`) and when the Session never named itself
    /// (`Locked` — nothing to resume). With no agent running, no respawn:
    /// the next start takes the change. The transcript says what changed.
    fn retune(
        &mut self,
        thread: ThreadId,
        model: Option<String>,
        effort: Option<String>,
    ) -> Result<(), ProvisionError> {
        let Some(state) = self.threads.get(&thread) else {
            return self
                .store
                .set_tuning(thread, model, effort, None)
                .map_err(ProvisionError::Store);
        };
        if state.model == model && state.effort == effort {
            self.threads.get_mut(&thread).expect("checked").replacement = None;
            return Ok(());
        }
        if !state.first_prompt_sent {
            let provider = state.provider;
            return self.provision(thread, provider, model, effort);
        }
        let provider = state.provider;
        self.replace_session(thread, provider, model, effort, ReplacementKind::Retune)
    }
}

impl Engine {
    /// Every Notice, and each Thread's deferral behind it — read-only.
    pub fn notifications(&self) -> &Notifications {
        &self.notifications
    }

    /// How long Main may sit idle after its last child settles before a
    /// deferred finish is called: a provider that never resumes Main on
    /// its own is given this much to prove it. Tests set it to zero.
    pub fn set_notification_grace(&mut self, grace: Duration) {
        self.notifications = Notifications::with_grace(grace);
    }

    /// The operator opened a Notice: it is read. Its Thread, or None when
    /// it is gone; landing on it is the window's (`Cockpit::open_notice`).
    pub fn open_notice(&mut self, id: NoticeId) -> Option<ThreadId> {
        self.notifications.open(id)
    }

    pub fn dismiss_notice(&mut self, id: NoticeId) -> bool {
        self.notifications.dismiss(id)
    }

    /// Open one live Decision attention record, and show the Subject that
    /// asked. Its handle remains resolved through Activity, so opening it
    /// cannot complete a turn or release a queued prompt.
    pub fn open_decision_notice(&mut self, id: &DecisionNoticeId) -> Option<ThreadId> {
        let subject = self
            .notifications
            .decision(id)?
            .subject
            .clone()
            .unwrap_or(Subject::Main);
        let thread = self.notifications.open_decision(id)?;
        self.set_visible_subject(thread, subject);
        Some(thread)
    }

    pub fn dismiss_decision_notice(&mut self, id: &DecisionNoticeId) -> bool {
        self.notifications.dismiss_decision(id)
    }

    pub fn clear_notices(&mut self) {
        self.notifications.clear();
    }

    /// The bell's `mark all read`: every Notice and live request read,
    /// none removed (`Notifications::mark_all_read`).
    pub fn mark_notices_read(&mut self) {
        self.notifications.mark_all_read();
    }

    /// Fixtures only (the parity scenes): date a Thread's notification
    /// records (`Notifications::fixture_backdate`).
    pub fn fixture_backdate_notices(&mut self, thread: ThreadId, at: std::time::SystemTime) {
        self.notifications.fixture_backdate(thread, at);
    }

    /// Fixtures only (the parity scenes): a Notice for `thread`, born now
    /// (`Notifications::fixture_push`).
    pub fn fixture_notice(&mut self, thread: ThreadId, outcome: crate::TurnOutcome) {
        self.notifications.fixture_push(thread, outcome);
    }

    /// A focused window shows these Threads: each one's Notices are read,
    /// and so are the live Decisions of the Subject its Pane shows. True
    /// when anything was read.
    pub fn acknowledge_seen(&mut self, shown: &[ThreadId]) -> bool {
        let mut read = false;
        for &thread in shown {
            let subject = self
                .visible_subjects
                .get(&thread)
                .map(|subject| {
                    self.threads
                        .get(&thread)
                        .map(|state| state.activity.view().canonical_subject(subject))
                        .unwrap_or(Subject::Main)
                })
                .filter(|subject| {
                    self.threads
                        .get(&thread)
                        .is_some_and(|state| state.activity.view().subject(subject).is_some())
                })
                .unwrap_or(Subject::Main);
            if subject == Subject::Main {
                self.visible_subjects.remove(&thread);
            }
            read |= self.notifications.acknowledge_subject(thread, &subject);
        }
        read
    }

    /// Whether every Thread is at rest and nothing waits to be sent: what
    /// the Server may stop or restart at without cutting an agent off.
    pub fn idle(&self) -> bool {
        self.bootstraps.is_empty() && self.threads.values().all(Thread::at_rest)
    }

    /// An open Thread's Attention, as the Server knows it (ADR 0015): a
    /// Decision waiting, any work running or in the provider's queue, an
    /// unseen Notice, else idle. A prompt not yet answered counts as work —
    /// sent before the provider streams a byte, the provider's queued one,
    /// or one held for a starting Session: until it runs, the agent is not
    /// at rest.
    pub fn attention(&self, thread: ThreadId) -> Option<crate::Attention> {
        use crate::Attention;
        let view = self.thread(thread)?;
        Some(if view.pending().is_some() {
            Attention::Blocked
        } else if view.working()
            || view.activity().main_operator_turn()
            || view.queue_pending()
            || view.background_work()
            || (view.starting() && self.holds_prompt(thread))
        {
            Attention::Working
        } else if self.notifications.attention(thread) {
            Attention::Done
        } else {
            Attention::Idle
        })
    }

    /// Everything a window draws before any transcript (ADR 0025).
    pub fn snapshot(&mut self) -> protocol::Snapshot {
        self.publish();
        protocol::Snapshot {
            boot_id: self.boot_id.clone(),
            revision: self.revision,
            threads: self
                .threads
                .iter()
                .map(|(&id, _)| self.thread_state(id))
                .collect(),
            roster: SharedRoster {
                open: self.open.clone(),
                parked: self.parked().unwrap_or_default(),
            },
            decisions: self.pending_decisions(),
            groups: self.published_shared.groups.clone(),
            registry: self.published_shared.registry.clone(),
            notices: self.published_shared.notices.clone(),
            providers: self.published_shared.providers.clone(),
            parked: self.published_shared.parked.clone(),
            digests: Some(
                self.threads
                    .keys()
                    .filter_map(|&thread| {
                        let digest = self.published.get(&thread)?.digest.clone()?;
                        Some(protocol::ThreadDigest { thread, digest })
                    })
                    .collect(),
            ),
        }
    }

    /// Every open Thread's pending Decisions.
    pub fn pending_decisions(&self) -> Vec<protocol::PendingEntry> {
        self.decisions_where(|_| true)
    }

    /// The pending Decisions still waiting for an answer: one whose answer
    /// is being submitted (an async question's) is answered already.
    pub fn unanswered_decisions(&self) -> Vec<protocol::PendingEntry> {
        self.decisions_where(|pending| !pending.submitting)
    }

    fn decisions_where(
        &self,
        wanted: impl Fn(&PendingDecision) -> bool + Copy,
    ) -> Vec<protocol::PendingEntry> {
        self.threads
            .iter()
            .flat_map(|(&thread, state)| {
                state
                    .activity
                    .view()
                    .decisions()
                    .iter()
                    .filter(move |pending| wanted(pending))
                    .map(move |pending| protocol::PendingEntry {
                        thread,
                        subject: pending.subject.as_ref().map(Into::into),
                        handle: (&pending.handle).into(),
                        decision: (&pending.decision).into(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn thread_state(&self, id: ThreadId) -> protocol::ThreadState {
        let mut state = self.threads[&id].wire_state(id);
        state.for_draft = self.starting_for_draft(id);
        state
    }

    /// What a window shows of each provider.
    fn provider_states(&self) -> Vec<protocol::ProviderState> {
        [Provider::Claude, Provider::Codex]
            .into_iter()
            .map(|provider| protocol::ProviderState {
                provider,
                live_sessions: self.live_sessions(provider),
                models: self.announced_models(provider),
                limits: Some(self.account_limits(provider).into()),
            })
            .collect()
    }

    /// A Thread's history for a watcher that has none of it: its log as a
    /// reopen would replay it, read later off the pump thread up to a
    /// checkpoint taken now, then the live state the log never holds —
    /// the Session's connection, pending Decisions, MCP state, limits.
    /// Everything published so far is in it; inputs continue after `seq`.
    pub fn history(&mut self, thread: ThreadId) -> Option<History> {
        use protocol::{Live, WireInput};
        // Its watcher holds it from here, before the Server next says so.
        if let Some(watched) = &mut self.watched {
            watched.insert(thread);
        }
        self.hold(thread);
        self.publish();
        let state = self.threads.get_mut(&thread)?;
        let through = match state.writer.checkpoint() {
            Ok(through) => through,
            Err(error) => {
                state.report_store_error(error);
                return None;
            }
        };
        let log = match self.store.log_at(thread, &state.writer, through) {
            Ok(log) => log,
            Err(error) => {
                crate::log!("thread {thread}: its log could not be opened: {error}");
                return None;
            }
        };
        let state = &self.threads[&thread];
        let view = state.activity.view();
        let transcript = view.main().transcript();
        let mut live = vec![WireInput::Live(Live::Connect {
            generation: state.generation,
        })];
        live.extend(view.decisions().iter().map(|pending| {
            WireInput::Live(match &pending.subject {
                Some(Subject::Main) => Live::DecisionRequested {
                    decision: (&pending.decision).into(),
                },
                subject => Live::Decision {
                    subject: subject.as_ref().map(Into::into),
                    decision: (&pending.decision).into(),
                },
            })
        }));
        live.push(WireInput::Live(Live::McpServers {
            servers: transcript.mcp_servers().iter().map(Into::into).collect(),
        }));
        live.push(WireInput::Live(Live::RateLimits {
            limits: transcript.rate_limits().into(),
        }));
        Some(History {
            thread,
            log,
            live,
            seq: self
                .published
                .get(&thread)
                .map_or(0, |published| published.seq),
        })
    }

    /// When this Engine started: the zero of the `at_ms` it publishes.
    pub fn boot(&self) -> Instant {
        self.boot
    }

    /// This boot's identity: a resume names it, and the API's Attention
    /// counts are of it. A Server that restarted has another, so no client
    /// resumes into a stream that is not there, nor reads a count as one
    /// it has seen.
    pub fn boot_id(&self) -> &str {
        &self.boot_id
    }

    /// The open Threads, in the order they opened.
    pub fn open_threads(&self) -> &[ThreadId] {
        &self.open
    }

    /// Everything the Engine has to tell its clients since it was last
    /// asked, in order (ADR 0025): each open Thread's applied inputs, its
    /// state when that changed, and the open Threads when they changed.
    pub fn take_messages(&mut self) -> Vec<ServerMessage> {
        self.publish();
        std::mem::take(&mut self.outbox)
    }

    fn publish(&mut self) {
        self.count_attention();
        self.forget_closed_composing();
        let now = protocol::ms_since(crate::clock::instant(), self.boot);
        for (&id, thread) in &mut self.threads {
            let published = self.published.entry(id).or_default();
            if std::mem::take(&mut thread.activity.reset) {
                // The reset takes a `seq` of its own, so a client that saw
                // it is told apart from one that did not.
                published.seq += 1;
                self.outbox.push(ServerMessage::History {
                    thread: id,
                    after: 0,
                    inputs: Vec::new(),
                });
                self.outbox.push(ServerMessage::HistoryEnd {
                    thread: id,
                    seq: published.seq,
                    live: Vec::new(),
                });
            }
            let applied = std::mem::take(&mut thread.activity.applied);
            published.digest_owed |= !applied.is_empty();
            // Taken, not drained: a revive's replay grows the journal to its
            // whole history, and a drained Vec keeps that capacity for good.
            for input in applied {
                let at_ms = match &input {
                    ActivityInput::Main { at, .. }
                    | ActivityInput::Observe { at, .. }
                    | ActivityInput::Answered { at, .. } => protocol::ms_since(*at, self.boot),
                    _ => now,
                };
                match protocol::outgoing(&input, None) {
                    Outgoing::Input(input) => {
                        published.seq += 1;
                        self.outbox.push(ServerMessage::Input {
                            thread: id,
                            seq: published.seq,
                            at_ms,
                            input,
                        });
                    }
                    Outgoing::ToolDraft {
                        id: tool,
                        name,
                        input,
                    } => self.outbox.push(ServerMessage::ToolDraft {
                        thread: id,
                        id: tool,
                        name,
                        input,
                    }),
                    Outgoing::Local => {}
                }
            }
        }
        self.publish_digests();
        self.let_go_unused();
        let states: Vec<_> = self
            .threads
            .iter()
            .map(|(&id, _)| self.thread_state(id))
            .collect();
        for state in states {
            let published = self.published.entry(state.thread).or_default();
            if published.state.as_ref() != Some(&state) {
                self.revision += 1;
                published.state = Some(state.clone());
                self.outbox.push(ServerMessage::Thread {
                    revision: self.revision,
                    state,
                });
            }
        }
        if self.open != self.published_open {
            // A Thread that closed keeps counting where it left off, so no
            // `seq` of its earlier life names one of a later life.
            for (id, published) in &mut self.published {
                if !self.threads.contains_key(id) {
                    published.state = None;
                    published.digest = None;
                    published.digest_at = None;
                }
            }
            self.published_open = self.open.clone();
            self.parked_listing.stale = true;
            self.revision += 1;
            self.outbox.push(ServerMessage::Roster {
                revision: self.revision,
                roster: SharedRoster {
                    open: self.open.clone(),
                    parked: self.parked().unwrap_or_default(),
                },
            });
        }
        let shared = &mut self.published_shared;
        if let Some(groups) = changed(&mut shared.groups, self.groups.to_wire()) {
            self.revision += 1;
            self.outbox.push(ServerMessage::Groups {
                revision: self.revision,
                groups,
            });
        }
        if let Some(registry) = changed(&mut shared.registry, self.registry.to_wire()) {
            self.revision += 1;
            self.outbox.push(ServerMessage::Registry {
                revision: self.revision,
                registry,
            });
        }
        if let Some(notices) = changed(&mut shared.notices, self.notifications.to_wire()) {
            self.revision += 1;
            self.outbox.push(ServerMessage::Notices {
                revision: self.revision,
                notices,
            });
        }
        for warning in self.allowance_warnings.drain(..) {
            self.outbox.push(ServerMessage::AllowanceWarning {
                provider: warning.provider,
                window: warning.window.into(),
                threshold: warning.threshold,
                used_fraction: warning.used_fraction,
            });
        }
        let providers = self.provider_states();
        if let Some(providers) = changed(&mut self.published_shared.providers, providers) {
            self.revision += 1;
            self.outbox.push(ServerMessage::Providers {
                revision: self.revision,
                providers,
            });
        }
        self.publish_parked();
    }

    /// Let go of the Activity of each Thread with no Session that no window
    /// watches, once at rest and its digest says so: that is all anyone
    /// reads of it until it is used (`hold`). Only while a Server says what
    /// its windows watch (`set_watched`).
    fn let_go_unused(&mut self) {
        let Some(watched) = &self.watched else {
            return;
        };
        let now = crate::clock::instant();
        for (id, thread) in &mut self.threads {
            let current = self
                .published
                .get(id)
                .is_some_and(|published| published.digest.is_some() && !published.digest_owed);
            if thread.held
                && current
                && !watched.contains(id)
                && thread.session.is_none()
                && thread.replacement.is_none()
                && thread.at_rest()
                && thread
                    .read_back_at
                    .is_none_or(|at| now.saturating_duration_since(at) >= HOLD_AT_LEAST)
            {
                thread.activity.forget();
                thread.history.clear();
                thread.held = false;
            }
        }
    }

    /// What a Server's windows watch now. A Thread newly watched is held
    /// again; one nobody watches may be let go (`let_go_unused`).
    pub fn set_watched(&mut self, watched: BTreeSet<ThreadId>) {
        for &thread in &watched {
            self.hold(thread);
        }
        self.watched = Some(watched);
    }

    /// Read a let-go Thread's Activity back from its log, as a revive does
    /// but on its live writer, its Session's generation kept: nothing a
    /// window holds of it changes.
    pub(crate) fn hold(&mut self, thread: ThreadId) {
        let Some(state) = self.threads.get_mut(&thread).filter(|state| !state.held) else {
            return;
        };
        let replay = state
            .writer
            .checkpoint()
            .map_err(LoadError::Io)
            .and_then(|through| {
                self.store
                    .log_at(thread, &state.writer, through)
                    .map_err(LoadError::Io)
            })
            .and_then(crate::store::LogAt::load);
        let replay = match replay {
            Ok(replay) => replay,
            Err(error) => {
                // Held as it is: its digest and notifications go on.
                crate::log!("thread {thread}: its log could not be read back: {error:?}");
                state.held = true;
                return;
            }
        };
        state.read_back(replay.activity_inputs());
        state.activity.apply(ActivityInput::Connect {
            generation: state.generation,
        });
        state.activity.settle();
        state.held = true;
        state.read_back_at = Some(crate::clock::instant());
    }

    /// Refold each digest its Thread's inputs left owing: at once when Main
    /// started or ended a turn, a Decision came or went, or the windows
    /// show another Subject; else once its `DIGEST_REFOLD` is up, the last
    /// thing streamed included.
    fn publish_digests(&mut self) {
        let now = crate::clock::instant();
        for (&id, thread) in self.threads.iter().filter(|(_, thread)| thread.held) {
            let published = self.published.entry(id).or_default();
            let shown = self.visible_subjects.get(&id);
            let current = published
                .digest
                .as_ref()
                .is_some_and(|digest| digest.shows(shown));
            if !published.digest_owed && current {
                continue;
            }
            let activity = thread.activity.view();
            let due = published
                .digest
                .as_ref()
                .is_none_or(|digest| digest.outdated_by(activity, shown))
                || published
                    .digest_at
                    .is_none_or(|at| now.saturating_duration_since(at) >= DIGEST_REFOLD);
            if !due {
                continue;
            }
            published.digest_owed = false;
            // A Thread's first digest starts no throttle: what streams next
            // lands at once.
            published.digest_at = published.digest.is_some().then_some(now);
            let digest = Digest::of(id, activity, shown, self.boot);
            if published.digest.as_ref() != Some(&digest) {
                published.digest = Some(digest.clone());
                self.outbox
                    .push(ServerMessage::Digest { thread: id, digest });
            }
        }
    }

    /// Whether a window that lists this Server's parked Threads is attached
    /// (`Capability::ParkedThreads`): only then are their rows read. A
    /// window on this computer reads the Store itself.
    pub fn list_parked(&mut self, wanted: bool) {
        if wanted == self.parked_listing.wanted {
            return;
        }
        self.parked_listing = ParkedListing {
            wanted,
            stale: true,
            reading: None,
        };
        self.published_shared.parked = None;
    }

    /// Publish the rows a read brought back, unless something changed
    /// since it began; then start the next read if one is due.
    fn publish_parked(&mut self) {
        if let Some(reading) = &self.parked_listing.reading {
            match reading.try_recv() {
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Ok(rows) if !self.parked_listing.stale => {
                    if let Some(threads) = changed(&mut self.published_shared.parked, rows) {
                        self.revision += 1;
                        self.outbox.push(ServerMessage::Parked {
                            revision: self.revision,
                            threads,
                        });
                    }
                }
                // Out of date, or the read died: the next one starts below.
                _ => self.parked_listing.stale = true,
            }
            self.parked_listing.reading = None;
        }
        if !self.parked_listing.wanted || !std::mem::take(&mut self.parked_listing.stale) {
            return;
        }
        let store = self.store.clone();
        let threads = self.parked().unwrap_or_default();
        let (rows, reading) = channel();
        let read = std::thread::Builder::new()
            .name("ferrite-parked-rows".into())
            .spawn(move || {
                let _ = rows.send(parked_rows(&store, threads));
            });
        match read {
            Ok(_) => self.parked_listing.reading = Some(reading),
            Err(error) => crate::log!("the parked Threads could not be read: {error}"),
        }
    }

    fn note_open(&mut self, thread: ThreadId) {
        if !self.open.contains(&thread) {
            self.open.push(thread);
        }
    }

    /// Record the Subject the renderer is actually showing. Invalid or stale
    /// children fall back to Main, so a replacement Session cannot suppress a
    /// later Main request with an old selection.
    pub fn set_visible_subject(&mut self, thread: ThreadId, subject: Subject) {
        let subject = self
            .threads
            .get(&thread)
            .map(|state| state.activity.view().canonical_subject(&subject))
            .filter(|subject| {
                self.threads
                    .get(&thread)
                    .is_some_and(|state| state.activity.view().subject(subject).is_some())
            })
            .unwrap_or(Subject::Main);
        if subject == Subject::Main {
            self.visible_subjects.remove(&thread);
        } else {
            self.visible_subjects.insert(thread, subject);
        }
    }

    /// A Draft's first send, in the scope its window gave it: the Thread
    /// it became, or `None` while startup is pending — the result then
    /// arrives through `take_bootstrap_results`, tagged with `draft`.
    pub fn bootstrap_draft(
        &mut self,
        draft: DraftId,
        scope: DraftScope,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
        effort: Option<String>,
    ) -> io::Result<Option<ThreadId>> {
        let thread = match scope.group {
            Some(group) => {
                self.bootstrap_in_group_with(choice, workspace, prompt, group, effort)?
            }
            None => self.begin_bootstrap(
                choice,
                workspace,
                prompt,
                effort,
                None,
                scope.new_group_with,
            )?,
        };
        match self.bootstraps.get_mut(&thread) {
            Some(pending) => {
                pending.draft = Some(draft);
                Ok(None)
            }
            None => Ok(Some(thread)),
        }
    }

    /// Cancel a submitted startup before changing a draft's choices. The
    /// draft slot and Composer remain; cancellation emits no completion.
    pub fn cancel_draft_start(&mut self, draft: DraftId) -> io::Result<bool> {
        let Some(thread) = self
            .bootstraps
            .iter()
            .find_map(|(id, pending)| (pending.draft == Some(draft)).then_some(*id))
        else {
            return Ok(false);
        };
        self.bootstraps.remove(&thread);
        self.rollback_bootstrap(thread)?;
        Ok(true)
    }

    /// Whether a prompt waits on `thread`'s Session: one written while it
    /// starts or before it can queue, or a new Thread's first prompt.
    pub fn holds_prompt(&self, thread: ThreadId) -> bool {
        self.bootstraps.contains_key(&thread)
            || self
                .threads
                .get(&thread)
                .is_some_and(|state| !state.awaiting.prompts.is_empty())
    }

    /// Whether `thread` is a Draft's first send still starting: its Pane is
    /// the Draft's until it settles.
    pub fn starting_for_draft(&self, thread: ThreadId) -> bool {
        self.bootstraps
            .get(&thread)
            .is_some_and(|pending| pending.draft.is_some())
    }

    pub fn draft_starting(&self, draft: DraftId) -> bool {
        self.bootstraps
            .values()
            .any(|pending| pending.draft == Some(draft))
    }

    pub fn take_bootstrap_results(&mut self) -> Vec<BootstrapResult> {
        std::mem::take(&mut self.bootstrap_results)
    }
}

/// How much of a history one `history` message carries, encoded: a log of
/// any size crosses the link in pieces no reader refuses.
const HISTORY_CHUNK: usize = 1 << 20;

/// A watched Thread rebuilt from its log (`history` … `history_end`),
/// read and sent off the pump thread (`Engine::history`).
pub struct History {
    thread: ThreadId,
    log: crate::store::LogAt,
    live: Vec<protocol::WireInput>,
    seq: u64,
}

impl History {
    /// The last `seq` this history covers: the Thread's inputs continue after it.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Read the log and hand `send` its `history` pieces, then
    /// `history_end`. A log that cannot be read leaves the history empty:
    /// the watcher still gets the live state and what follows.
    pub fn send(self, send: &mut dyn FnMut(ServerMessage) -> io::Result<()>) -> io::Result<()> {
        let thread = self.thread;
        let replay = match self.log.load() {
            Ok(replay) => replay.activity_inputs(),
            Err(error) => {
                crate::log!("thread {thread}: its history could not be read: {error:?}");
                Vec::new()
            }
        };
        let inputs = std::iter::once(ActivityInput::Disconnect)
            .chain(replay)
            .filter_map(|input| match protocol::outgoing(&input, None) {
                Outgoing::Input(input) => Some(input),
                _ => None,
            });
        let mut chunk = Vec::new();
        let mut bytes = 0;
        let mut after = 0;
        for input in inputs {
            bytes += protocol::encode(&input).len();
            chunk.push(input);
            if bytes >= HISTORY_CHUNK {
                let inputs = std::mem::take(&mut chunk);
                let sent = inputs.len() as u64;
                send(ServerMessage::History {
                    thread,
                    after,
                    inputs,
                })?;
                after += sent;
                bytes = 0;
            }
        }
        if after == 0 || !chunk.is_empty() {
            send(ServerMessage::History {
                thread,
                after,
                inputs: chunk,
            })?;
        }
        send(ServerMessage::HistoryEnd {
            thread,
            seq: self.seq,
            live: self.live,
        })
    }
}

/// An open Thread's facts, read through `Engine::thread`. Every accessor
/// hands back a borrow of the Engine itself (`'a`), so a caller keeps the
/// facts it read after the handle is gone.
#[derive(Clone, Copy)]
pub struct ThreadView<'a> {
    state: &'a Thread,
}

impl<'a> ThreadView<'a> {
    /// Changes whenever this Thread receives a replacement Session.
    pub fn generation(&self) -> u64 {
        self.state.generation
    }

    /// Whether its agent offers the control `kind`, as it last said
    /// (ADR 0032): running it starts the agent if it is not.
    pub fn supports_control(&self, kind: crate::ControlKind) -> bool {
        self.state.announced.supports(kind)
    }

    pub fn permission_modes(&self) -> Vec<crate::PermissionModeChoice> {
        self.state.announced.permission_modes().to_vec()
    }
    pub fn activity(&self) -> ActivityView<'a> {
        self.state.activity.view()
    }

    pub fn store_error(&self) -> Option<&'a str> {
        self.state.store_error.as_deref()
    }

    pub fn transcript(&self) -> &'a Transcript {
        self.state.transcript()
    }

    /// Which backend serves this Thread.
    pub fn provider(&self) -> Provider {
        self.state.provider
    }

    /// The model this Thread chose before its first prompt (#25) — the ✓ in
    /// the provider picker. `None` is the provider's default.
    pub fn model(&self) -> Option<&'a str> {
        self.state.model.as_deref()
    }

    /// The reasoning effort this Thread chose, in the provider's own word.
    /// `None` is the operator's default for the provider.
    pub fn effort(&self) -> Option<&'a str> {
        self.state.effort.as_deref()
    }

    /// The durable operator title, as this open Thread holds it.
    pub fn title(&self) -> Option<&'a str> {
        self.state.title.as_deref()
    }

    /// The follow-up predicted for this Thread's last response, if one has
    /// landed and survived the filter. What the Composer's idle line offers
    /// and what Tab accepts.
    pub fn suggestion(&self) -> Option<&'a str> {
        self.state.suggestion.as_deref()
    }

    /// What this Thread is blocked on, if anything.
    pub fn pending(&self) -> Option<&'a Decision> {
        self.state
            .activity
            .view()
            .decisions()
            .first()
            .map(|pending| &pending.decision)
    }

    /// The prompt held back while the turn runs — the latest, which is
    /// the one `⌫ unqueue` and Up act on.
    pub fn queued(&self) -> Option<&'a str> {
        self.state
            .awaiting
            .prompts
            .back()
            .map(String::as_str)
            .or(self
                .state
                .native_queue
                .items
                .last()
                .map(|item| item.text.as_str()))
    }

    /// Every prompt held back, newest first: the latest sits on top of the
    /// pile and what was said before it reads underneath.
    pub fn queued_all(&self) -> Vec<&'a str> {
        self.state
            .awaiting
            .prompts
            .iter()
            .rev()
            .map(String::as_str)
            .chain(
                self.state
                    .native_queue
                    .items
                    .iter()
                    .rev()
                    .map(|item| item.text.as_str()),
            )
            .collect()
    }

    /// A replacement is pending, or a start that shows: its choices and
    /// first prompt have not committed yet. Ready adapters never enter this
    /// state, and a wake shows only once a prompt waits on it (ADR 0032).
    pub fn starting(&self) -> bool {
        self.state.starting()
    }

    /// Is a turn running, or a start that shows? A prompt written now has
    /// to wait for it.
    pub fn busy(&self) -> bool {
        self.state.busy() || self.starting()
    }

    /// Is the agent mid-turn? Unlike `busy`, a Session still starting
    /// (a launch's revival, a model or provider change) is not working.
    pub fn working(&self) -> bool {
        self.state.busy()
    }

    /// Whether work runs behind Main: its Subagents, or its transcript's
    /// background tasks.
    pub fn background_work(&self) -> bool {
        self.state.background_work()
    }

    /// Whether a model or effort change waits on the provider's answer:
    /// the pickers hold until it lands.
    pub fn tuning(&self) -> bool {
        self.state.tuning.is_some()
    }

    pub fn needs_queue(&self) -> bool {
        self.starting() || self.state.turn_ahead()
    }

    /// Whether the provider holds a queued prompt, or is still taking one.
    pub fn queue_pending(&self) -> bool {
        self.state.native_queue.pending()
    }

    /// How many prompts wait in the provider's own queue.
    pub fn provider_queue_len(&self) -> usize {
        self.state.native_queue.items.len()
    }

    /// The provider's own id for the conversation, which it resumes from:
    /// the latest its Session announced.
    pub fn provider_session(&self) -> Option<&'a str> {
        self.state.resume.as_deref()
    }

    /// The checkout this Thread works in — what the Pane's chrome shows.
    pub fn workspace(&self) -> Option<&'a WorkspaceBinding> {
        self.state.workspace.as_ref()
    }

    /// Where inside the binding its work happens (#24). `None` means the
    /// binding itself — today's behavior, and every Thread's starting point.
    pub fn session_project_root(&self) -> Option<&'a Path> {
        self.state.session_project_root.as_deref()
    }

    /// Whether the first operator prompt has gone out — the one lock every
    /// pre-prompt control reads (#25, #29). Armed by `send`, and on revive
    /// or import when the replayed history contains an operator prompt.
    pub fn first_prompt_sent(&self) -> bool {
        self.state.first_prompt_sent
    }

    pub fn has_prompt_history(&self) -> bool {
        self.state.prompt_history.has_entries()
    }

    /// The wall clocks of this Thread's tool calls, keyed by call id — the
    /// calls the Cockpit ingested live, plus the ones its log recorded a
    /// duration for. Never a guess: a call with no measured clock is absent.
    pub fn tool_timings(&self) -> &'a HashMap<String, ToolTiming> {
        self.state.activity.view().main().tool_timings()
    }

    /// Its agent's command menu (#23), as it last said: what `/` offers in
    /// this Thread's Composer. Empty until an agent announces one — never a
    /// static list.
    pub fn commands(&self) -> &'a [crate::SessionCommand] {
        self.state.announced.commands()
    }

    /// The models its agent's install offers (#25), as it last said: the
    /// provider picker's model rows. Empty until an agent announces a list
    /// — never a static one.
    pub fn models(&self) -> &'a [ModelInfo] {
        self.state.announced.models()
    }

    /// Its agent's permission mode (#23), as it last said: the meta row's
    /// mode chip. None until an agent announces one — a chip is never
    /// invented.
    pub fn permission_mode(&self) -> Option<&'a str> {
        self.state.announced.permission_mode()
    }
}

/// A draft's first send, done: the Thread it became, and whether the
/// leave the draft was holding open (see `close`) applied.
#[derive(Debug)]
pub struct Bootstrapped {
    pub thread: ThreadId,
    /// The deferred leave the Groups module refused, if it did — the Thread
    /// is live and in its Group regardless.
    pub refused_leave: Option<ApplyError>,
    /// A deferred Group leave applied with this first send.
    pub applied_leave: bool,
}

fn megabytes(bytes: u64) -> String {
    format!("{} MB", bytes / (1024 * 1024))
}

/// A Thread's name from its first prompt: the first non-blank line,
/// whitespace collapsed, cut to `TITLE_CHARS` characters with an ellipsis.
/// A prompt that is only whitespace names nothing.
pub fn title_from_prompt(text: &str) -> String {
    const TITLE_CHARS: usize = 48;
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let collapsed: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = collapsed.chars();
    let cut: String = chars.by_ref().take(TITLE_CHARS).collect();
    if chars.next().is_some() {
        format!("{}…", cut.trim_end())
    } else {
        cut
    }
}

/// What became of a prompt handed to the provider's queue.
enum Enqueued {
    /// The provider took it, or may have: never resent.
    Taken,
    /// It cannot queue yet; nothing was sent.
    NotYet(io::Error),
    /// Ferrite could not record it, and said so; nothing was sent.
    Failed,
}

/// The #24 guard: a session project root that no longer exists on disk
/// refuses the send — readably, naming the path and the remedy — never a
/// silent fallback to working in the binding.
fn vanished_root_refusal(state: &Thread) -> Option<String> {
    let root = state.session_project_root.as_ref()?;
    if root.exists() {
        return None;
    }
    Some(format!(
        "send refused: session project root {} no longer exists — re-pick it",
        root.display()
    ))
}

/// One operator prompt onto the wire, into the log, onto the Pane. The
/// Session's first prompt is prefaced — on the wire only — with the hidden
/// session-context block when a root is set: the transcript and the log
/// carry the operator's raw text, displayed ≠ sent. Answers what the
/// transcript changed.
fn deliver(state: &mut Thread, text: String, suggestions_enabled: bool) -> io::Result<Update> {
    if let Some(refusal) = vanished_root_refusal(state) {
        return Err(io::Error::new(io::ErrorKind::NotFound, refusal));
    }
    let Some(session) = state
        .session
        .as_mut()
        .and_then(SessionLifecycle::session_mut)
    else {
        return Err(io::Error::new(io::ErrorKind::NotConnected, "no Session"));
    };
    let prefaced = match (&state.session_project_root, &state.workspace) {
        (Some(root), Some(binding)) if state.preface_pending => Some(format!(
            "<ferrite-session-context>\n\
             Agent workspace root: {}\n\
             Session project root: {}\n\
             Run edits, commands, tests, builds, and Git operations from the \
             session project root. Use the agent workspace root only for \
             broader workspace context.\n\
             </ferrite-session-context>\n\
             {}",
            binding.cwd().display(),
            root.display(),
            text
        )),
        _ => None,
    };
    // A Provider switched to mid-Thread is owed the earlier conversation:
    // it rides between the preface and the operator's text, this once.
    let mut wire = prefaced.unwrap_or_else(|| text.clone());
    if let Some(carry) = &state.carry {
        let operator = wire.len() - text.len();
        wire.insert_str(operator, &format!("{carry}\n\n"));
    }
    session.set_suggestions_enabled(suggestions_enabled)?;
    session.send(&wire)?;
    state.invalidate_suggestion();
    state.carry = None;
    // The Session's first prompt has gone out; every later one is bare.
    state.preface_pending = false;
    // And the Thread's first locks its provider for good (#25, #29).
    state.first_prompt_sent = true;
    // When it went out, kept beside it so replay draws the same `7:31 pm`.
    let sent_at = crate::clock::now_label();
    if let Err(error) = state.writer.record_prompt(&text) {
        state.report_store_error(error);
    }
    if let Err(error) = state.writer.record_prompt_observation(&sent_at) {
        state.report_store_error(error);
    }
    state.prompt_history.append(text.clone());
    let mut update = state.apply(Input::Prompt(text));
    update
        .dirty
        .extend(state.apply(Input::PromptObservation { sent_at }).dirty);
    Ok(update)
}

/// Everything the mirror held goes back to the provider as one prompt,
/// oldest first — what Escape promised. Each native id is cancelled first
/// so the provider's own queue cannot run it a second time; a steer has no
/// handle to cancel, and that refusal is not news.
fn resend_held(state: &mut Thread, suggestions_enabled: bool) -> Option<Update> {
    let held = state.native_queue.flush();
    if held.is_empty() {
        return None;
    }
    if let Some(session) = state
        .session
        .as_mut()
        .and_then(SessionLifecycle::session_mut)
    {
        for item in &held {
            let _ = session.cancel_queued(&item.id);
        }
    }
    let text = held
        .iter()
        .map(|item| item.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    match deliver(state, text.clone(), suggestions_enabled) {
        Ok(applied) => Some(applied),
        Err(error) => Some(state.undelivered(text, error)),
    }
}

/// How much of the earlier conversation a Provider switch hands over:
/// the last `CARRY_PAIRS` prompt/answer pairs, within `CARRY_CHARS`
/// characters — enough to continue, never the whole log.
const CARRY_PAIRS: usize = 12;
const CARRY_CHARS: usize = 6_000;

/// The earlier conversation as the next Provider reads it: the newest
/// pairs that fit the budget, oldest first, each `User:`/`Assistant:`
/// pair as it was said (an answer that never came is left out). One pair
/// too long for the whole budget keeps its head and says so.
fn carry_digest(from: Provider, exchanges: &[(String, String)]) -> String {
    let mut entries: Vec<String> = exchanges
        .iter()
        .rev()
        .take(CARRY_PAIRS)
        .map(|(prompt, answer)| {
            let mut entry = format!("User: {}", prompt.trim());
            let answer = answer.trim();
            if !answer.is_empty() {
                entry.push_str("\nAssistant: ");
                entry.push_str(answer);
            }
            entry
        })
        .collect();
    let mut total: usize = entries.iter().map(|entry| entry.len() + 2).sum();
    while entries.len() > 1 && total > CARRY_CHARS {
        let dropped = entries.pop().expect("more than one");
        total -= dropped.len() + 2;
    }
    if let Some(only) = entries.first_mut() {
        if only.len() > CARRY_CHARS {
            let mut cut = CARRY_CHARS;
            while !only.is_char_boundary(cut) {
                cut -= 1;
            }
            only.truncate(cut);
            only.push('…');
        }
    }
    entries.reverse();
    format!(
        "Earlier in this Thread (on {from}):\n\n{}",
        entries.join("\n\n")
    )
}

/// The workspace a binding names, made real; the main checkout is the
/// operator's and is touched by nothing. A missing worktree is created —
/// or recreated after a hand-deletion — on its registered branch (#29's
/// central layout), falling back to the Thread-named branch for worktrees
/// from before the registry. A tree already standing passes the adoption
/// conflict check instead: git itself must call it a worktree of this
/// repo, or a Session would spawn into a directory that only looks like
/// one.
fn ensure_workspace(
    registry: &Registry,
    binding: &WorkspaceBinding,
    thread: ThreadId,
) -> Result<(), workspace::GitError> {
    let WorkspaceBinding::Worktree { repo, path } = binding else {
        return Ok(());
    };
    place_worktree(repo, path, &worktree_branch(registry, path, thread))
}

/// The branch a Thread's worktree at `path` is (re)made on.
fn worktree_branch(registry: &Registry, path: &Path, thread: ThreadId) -> String {
    registry
        .branch_for(path)
        .map(str::to_string)
        .unwrap_or_else(|| branch_name(thread))
}

/// What every start of an existing Thread's Session waits on (ADR 0018):
/// its worktree checked if standing, or remade if missing and Ferrite's own
/// (registered, or pre-registry under the store). A revive's start and any
/// later one alike, so a worktree a revival could not place is tried again
/// on the next send rather than spawned into while missing. The branch is
/// looked up here; the git calls run wherever the spawner runs the
/// preparation.
fn placement(
    registry: &Registry,
    store_dir: &Path,
    workspace: Option<&WorkspaceBinding>,
    thread: ThreadId,
) -> Preparation {
    let worktree = match workspace {
        Some(WorkspaceBinding::Worktree { repo, path })
            if path.exists()
                || registry.branch_for(path).is_some()
                || path.starts_with(store_dir) =>
        {
            Some((
                repo.clone(),
                path.clone(),
                worktree_branch(registry, path, thread),
            ))
        }
        _ => None,
    };
    Box::new(move || match worktree {
        Some((repo, path, branch)) => {
            place_worktree(&repo, &path, &branch).map_err(io::Error::other)
        }
        None => Ok(()),
    })
}

/// `ensure_workspace`'s git half, which needs no registry: it can run off
/// the UI thread (`placement`).
fn place_worktree(repo: &Path, path: &Path, branch: &str) -> Result<(), workspace::GitError> {
    // Anything already standing at the path must BE the worktree — a
    // directory that merely exists there (squatting where one was, or was
    // never) must not be silently rebuilt over or spawned into.
    if path.exists() {
        return registry::adoption_check(repo, path);
    }
    workspace::ensure_worktree(repo, path, branch)
}

/// The branch a Thread's worktree lives on. Named for the Thread so the
/// operator can read `git branch` and know whose work each one is.
fn branch_name(thread: ThreadId) -> String {
    format!("ferrite/thread-{thread}")
}

/// A Decision asked, answered or withdrawn: a point the log is flushed at.
fn changes_decisions(event: &SessionEvent) -> bool {
    matches!(
        event,
        SessionEvent::DecisionRequested { .. }
            | SessionEvent::Activity(
                ActivityEvent::Decision { .. }
                    | ActivityEvent::DecisionReply { .. }
                    | ActivityEvent::DecisionCancelled { .. }
            )
    )
}

pub(crate) fn event_changes_content(event: &SessionEvent) -> bool {
    use crate::activity::ExecutionEvent;
    match event {
        SessionEvent::Activity(
            ActivityEvent::Content { event, .. }
            | ActivityEvent::HistoryContent { event, .. }
            | ActivityEvent::MainContent { event, .. },
        ) => !matches!(event, ExecutionEvent::TokenUsage { .. }),
        SessionEvent::Activity(ActivityEvent::Status { .. }) => true,
        SessionEvent::Activity(_)
        | SessionEvent::TokenUsage { .. }
        | SessionEvent::Commands { .. }
        | SessionEvent::Models { .. }
        | SessionEvent::PermissionMode { .. }
        | SessionEvent::ContextDetails { .. }
        | SessionEvent::McpServers { .. }
        | SessionEvent::McpAuthorization { .. }
        | SessionEvent::ScheduledWork { .. } => false,
        _ => true,
    }
}

/// Run `action` on `session`, if it offers it.
fn run_control(session: &mut dyn Session, action: crate::SessionControl) -> io::Result<()> {
    if !session.supports_control(control_kind(&action)) {
        return Err(unsupported_control());
    }
    session.control(action)
}

fn unsupported_control() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "Session does not support that control",
    )
}

fn control_kind(action: &crate::SessionControl) -> crate::ControlKind {
    match action {
        crate::SessionControl::RefreshContext => crate::ControlKind::RefreshContext,
        crate::SessionControl::RefreshMcp => crate::ControlKind::RefreshMcp,
        crate::SessionControl::ReconnectMcp { .. } => crate::ControlKind::ReconnectMcp,
        crate::SessionControl::LoginMcp { .. } => crate::ControlKind::LoginMcp,
        crate::SessionControl::ReloadMcp => crate::ControlKind::ReloadMcp,
        crate::SessionControl::StopTask { .. } => crate::ControlKind::StopTask,
        crate::SessionControl::BackgroundTasks => crate::ControlKind::BackgroundTasks,
        crate::SessionControl::SetPermissionMode { .. } => crate::ControlKind::SetPermissionMode,
    }
}

fn completion_observation(
    thread: &Thread,
    event: &SessionEvent,
) -> Option<(Subject, crate::store::CompletionFacts)> {
    use crate::activity::ExecutionEvent;
    // Every outcome is timed: a completed turn's stamp, and the elapsed an
    // interrupted or failed turn's row reports.
    let subject = match event {
        SessionEvent::TurnEnded { .. } => Subject::Main,
        SessionEvent::Activity(ActivityEvent::Content {
            key,
            event: ExecutionEvent::TurnEnded { .. },
            ..
        }) => Subject::Subagent(key.clone()),
        SessionEvent::Activity(ActivityEvent::MainContent {
            event: ExecutionEvent::TurnEnded { .. },
            ..
        }) => Subject::Main,
        SessionEvent::Activity(ActivityEvent::BackgroundTurnEnded { .. }) => Subject::Main,
        _ => return None,
    };
    let view = thread.activity.view();
    let transcript = view.subject(&subject)?.transcript();
    let elapsed = transcript.turn_elapsed()?;
    // The turn's token counts, as its usage reports summed them; a turn
    // that reported none keeps none rather than a guessed zero.
    let (input, output) = (
        transcript.turn_input_tokens(),
        transcript.turn_output_tokens(),
    );
    let reported = input > 0 || output > 0;
    Some((
        subject,
        crate::store::CompletionFacts {
            elapsed_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
            completed_at: crate::clock::now_label(),
            input_tokens: reported.then_some(input),
            output_tokens: reported.then_some(output),
        },
    ))
}

/// Main's tool calls, as `follow` wants to see them: a start with its
/// input, or a finish with its verdict. Subagents never move the Thread —
/// they may well be working somewhere else on purpose.
fn follow_move(state: &mut Thread, event: &SessionEvent) -> Option<Move> {
    use crate::activity::ExecutionEvent;
    let binding = state.workspace.as_ref()?;
    let current = workspace::effective_cwd(state.session_project_root.as_deref(), Some(binding))?
        .to_path_buf();
    match event {
        SessionEvent::ToolStarted { id, name, input }
        | SessionEvent::Activity(ActivityEvent::MainContent {
            event: ExecutionEvent::ToolStarted { id, name, input },
            ..
        }) => state.follow.observe_started(id, name, input, &current),
        SessionEvent::ToolCompleted { id, is_error, .. }
        | SessionEvent::Activity(ActivityEvent::MainContent {
            event: ExecutionEvent::ToolCompleted { id, is_error, .. },
            ..
        }) => state.follow.observe_completed(id, *is_error),
        _ => None,
    }
}

/// Rebind a live Thread where `follow` says its agent went. The header is
/// rewritten first, so a Thread parked a moment later revives there too;
/// a refused rewrite leaves the binding as it was — the log and the live
/// state never disagree. The live Session is not restarted: the agent
/// already works there by its own paths, and the next spawn (revive,
/// watchdog restart) lands in the new cwd through the one cwd chain.
fn apply_move(store: &Store, id: ThreadId, state: &mut Thread, moved: Move) -> bool {
    let Some(binding) = state.workspace.clone() else {
        return false;
    };
    let repo = binding.repo().to_path_buf();
    let next = match moved {
        Move::ToMain => WorkspaceBinding::Main { checkout: repo },
        Move::Into(path) if follow::canonical(&path) == follow::canonical(&repo) => {
            WorkspaceBinding::Main { checkout: repo }
        }
        Move::Into(path) => WorkspaceBinding::Worktree { repo, path },
    };
    if next == binding {
        return false;
    }
    // The header rewrite moves every byte after it: a child-history read
    // in flight holds a checkpoint into the old log and would land short
    // (ADR 0002 — header changes invalidate obsolete loads).
    state.history.clear();
    if let Err(error) = store.set_workspace(id, &next, Some(&mut state.writer)) {
        state.report_store_error(match error {
            LoadError::Io(error) => error,
            other => io::Error::other(other.to_string()),
        });
        return false;
    }
    state.workspace = Some(next);
    state.follow.reset();
    true
}

/// Only Session-level metadata belongs here; execution state lives in Activity.
fn fold(state: &mut Thread, event: &SessionEvent) {
    match event {
        SessionEvent::Commands { .. }
        | SessionEvent::Models { .. }
        | SessionEvent::PermissionMode { .. }
        | SessionEvent::McpServers { .. } => state.announced.hear(event),
        SessionEvent::Init { session_id, .. } => state.resume = Some(session_id.clone()),
        SessionEvent::ScheduledWork { pending } => state.scheduled_work = *pending,
        SessionEvent::ConversationReset { session_id } => {
            state.resume = Some(session_id.clone());
            state.history.clear();
            state.history_errors.clear();
            state.carry = None;
            state.prompt_history = PromptHistory::new(Vec::new());
            state.invalidate_suggestion();
        }
        _ => {}
    }
}

/// Reads a Thread's log without the Cockpit: `Cockpit::log_reader`. It
/// never writes, so a copy may replay parked logs off the UI thread. On a
/// machine it reads what the machine's Server counted (ADR 0026).
#[derive(Clone)]
pub struct LogReader {
    logs: Logs,
}

#[derive(Clone)]
enum Logs {
    Store(Store),
    Published(Vec<protocol::ParkedThread>),
}

impl LogReader {
    pub(crate) fn new(store: Store) -> Self {
        Self {
            logs: Logs::Store(store),
        }
    }

    pub(crate) fn published(threads: Vec<protocol::ParkedThread>) -> Self {
        Self {
            logs: Logs::Published(threads),
        }
    }

    /// The subagents a parked Thread's durable activity knows — a replay of
    /// its whole log, so never on the UI thread for more than one Thread.
    /// Read off the log's tail for a schema-13 log; a log from before is
    /// replayed once and remembered (`Store::summary`).
    pub fn subagent_count(&self, thread: ThreadId) -> Result<usize, LoadError> {
        match &self.logs {
            Logs::Store(store) => Ok(store
                .summary(thread, replayed_subagents)?
                .subagents
                .unwrap_or_default()),
            Logs::Published(threads) => published(threads, thread, |row| row.subagents),
        }
    }

    /// How many turns a parked Thread's log holds — its prompts since the
    /// last conversation reset: the wall's parked tile reads `11 turns`.
    /// Off the log's tail, or remembered like `subagent_count`.
    pub fn turn_count(&self, thread: ThreadId) -> Result<usize, LoadError> {
        match &self.logs {
            Logs::Store(store) => Ok(store.summary(thread, replayed_subagents)?.turns),
            Logs::Published(threads) => published(threads, thread, |row| row.turns),
        }
    }
}

/// A count the machine's Server published for `thread`, when it could.
fn published(
    threads: &[protocol::ParkedThread],
    thread: ThreadId,
    count: impl Fn(&protocol::ParkedThread) -> Option<usize>,
) -> Result<usize, LoadError> {
    threads
        .iter()
        .find(|row| row.thread == thread)
        .and_then(count)
        .ok_or_else(|| {
            LoadError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!("thread {thread}: the machine did not say"),
            ))
        })
}

/// Each of `threads`' row, as a window on a machine lists it: a header
/// line, the log's tail and a `stat` each.
fn parked_rows(store: &Store, threads: Vec<ThreadId>) -> Vec<protocol::ParkedThread> {
    threads
        .into_iter()
        .map(|thread| {
            let last_used_ms = store
                .last_used(thread)
                .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|since| since.as_millis() as u64);
            let Ok(meta) = store.peek(thread) else {
                return protocol::ParkedThread {
                    thread,
                    provider: None,
                    title: None,
                    prompt_title: None,
                    workspace: None,
                    session_project_root: None,
                    project: None,
                    last_used_ms,
                    turns: None,
                    subagents: None,
                };
            };
            let prompt_title = match meta.title {
                Some(_) => None,
                None => store
                    .peek_first_prompt(thread)
                    .ok()
                    .flatten()
                    .map(|text| title_from_prompt(&text)),
            };
            let summary = store.summary(thread, replayed_subagents).ok();
            protocol::ParkedThread {
                thread,
                provider: Some(meta.provider),
                title: meta.title,
                prompt_title,
                workspace: meta.workspace.as_ref().map(Into::into),
                session_project_root: meta.session_project_root,
                project: meta.project_id,
                last_used_ms,
                turns: summary.as_ref().map(|summary| summary.turns),
                subagents: summary.and_then(|summary| summary.subagents),
            }
        })
        .collect()
}

/// The subagents a full replay of a snapshot knows: what a pre-13 log's
/// parked row is counted from, once.
fn replayed_subagents(snapshot: &crate::store::ThreadSnapshot) -> usize {
    let mut activity = Activity::default();
    for input in snapshot.activity_inputs() {
        activity.apply(input);
    }
    activity.view().children().len()
}

#[cfg(test)]
#[path = "cockpit/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "cockpit/lifecycle_tests.rs"]
mod lifecycle_tests;
