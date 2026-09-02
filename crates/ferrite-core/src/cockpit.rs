//! The Cockpit's headless state: what the operator is on the hook for, per
//! Thread.
//!
//! The pump's beginnings: Threads, their pending Decisions, and prompts held
//! back while a turn runs. No process and no window — a Thread's events are
//! fed in, and what the operator must answer comes out.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use crate::groups::{Applied, ApplyError, Drag, DropTarget, GroupChange, GroupId, Groups, Plan};
pub use crate::prompt_history::HistoryDirection;
use crate::prompt_history::PromptHistory;
use crate::providers::Session;
use crate::roster::{DraftId, DraftScope, Layout, PaneIdentity, Roster, View};
use crate::store::{LoadError, Provider, Store, ThreadWriter};
use crate::transcript::{BlockId, Input, Lexer, Transcript, Update};
use crate::workspace::registry::{self, ProjectId, Registry};
use crate::workspace::{self, WorkspaceBinding, WorkspaceChoice};
use crate::{Decision, DecisionAnswer, SessionEvent, ThreadId};

/// Everything one spawn needs, in one struct: every path that starts a
/// Session (open, revive, send-respawn, sweep) reads the Thread's stored
/// choice through it, so a new fact travels to all of them at once (#25).
pub struct SpawnRequest<'a> {
    pub provider: Provider,
    /// The Thread's chosen model, verbatim as the provider announced it.
    /// `None` is the provider's own default.
    pub model: Option<&'a str>,
    /// The provider-native id of a Thread being revived, which the provider
    /// reloads its own history from.
    pub resume: Option<&'a str>,
    /// The Thread's workspace binding resolved to the directory the Session
    /// works in; `None` only for a Thread from before bindings were
    /// recorded.
    pub cwd: Option<&'a Path>,
}

/// How a Session is started. Injected so the cockpit can be driven with
/// scripted Sessions in tests — nothing below this line spawns a process.
pub trait Spawner {
    fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn Session>>;
}

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
}

/// What one fold left for the caller to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Wake {
    Nothing,
    /// The turn ended and a prompt was waiting behind it — send this now.
    Send(String),
}

/// A provider and the model to serve it with — what the #25 picker picks,
/// whole. `None` is the provider's own default, which is also what every
/// Thread starts on.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderChoice {
    pub provider: Provider,
    pub model: Option<String>,
}

/// Re-aiming a Thread's provider failed — and changed nothing: the old
/// Session keeps serving and the header on disk is untouched.
#[derive(Debug)]
pub enum ProvisionError {
    /// The first prompt has gone out; nothing re-aims after it (#25, #29).
    Locked,
    /// The new provider's CLI would not spawn. The words are the
    /// provider's own.
    Spawn(io::Error),
    /// The durable header rewrite failed.
    Store(LoadError),
}

impl std::fmt::Display for ProvisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProvisionError::Locked => {
                write!(f, "the first prompt was sent; the provider is fixed")
            }
            ProvisionError::Spawn(e) => write!(f, "{e}"),
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
                write!(f, "could not revive Thread {thread}: {error}")
            }
            Self::Rollback { thread, error } => {
                write!(f, "could not roll back revived Thread {thread}: {error}")
            }
        }
    }
}

impl std::error::Error for ReviveGroupError {}

struct Thread {
    transcript: Transcript,
    /// Highlighting answers for this Thread's own Blocks. Per Thread because
    /// BlockIds are per Transcript: one shared channel could not say which
    /// Pane an answer belonged to.
    highlights: Receiver<Input>,
    /// The live Session, or None for a parked Thread.
    session: Option<Box<dyn Session>>,
    writer: ThreadWriter,
    provider: Provider,
    /// The model this Thread chose before its first prompt (#25), verbatim
    /// as the provider announced it. `None` — most Threads — is the
    /// provider's default. Every respawn reads it, so the choice holds for
    /// the Thread's whole life.
    model: Option<String>,
    title: Option<String>,
    pending: Option<Decision>,
    /// A prompt the operator wrote while the turn was still running.
    queued: Option<String>,
    prompt_history: PromptHistory,
    busy: bool,
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
    /// Armed whenever a Session is constructed or attached (open, revive,
    /// send-respawn, sweep-respawn); taken by the first prompt that goes
    /// out, which is the one that carries the hidden session-context
    /// preface when a root is set.
    preface_pending: bool,
    /// The live Session's command menu (#23) — what the Composer's `/`
    /// popover lists. Announced by the Session itself at start; empty until
    /// it speaks, and Session state only: a parked Thread keeps none.
    commands: Vec<crate::SessionCommand>,
    /// The models the live Session's install offers (#25) — the provider
    /// picker's model rows. Announced like the command menu, empty until
    /// the Session speaks, and Session state exactly like it: never a
    /// static list, gone with the Session.
    models: Vec<String>,
    /// The lock every pre-prompt control reads (#25, #29): armed by the
    /// first operator prompt that goes out, and on revive when the replayed
    /// history holds one. Never disarmed — nothing re-aims a Thread the
    /// operator has spoken in.
    first_prompt_sent: bool,
    /// The live Session's permission mode (#23) — the meta row's mode chip,
    /// in the provider's own word. Display-only, and Session state exactly
    /// like the menu: None until announced, gone with the Session.
    permission_mode: Option<String>,
    /// Each tool call's wall clock: stamped when this cockpit ingested its
    /// events, and restored from the log on revive — the transcript's folds
    /// stay clockless either way. Keyed by the provider's call id, the
    /// `ToolBlock.call` a row renders from.
    timings: HashMap<String, ToolTiming>,
}

impl Thread {
    /// A Thread ready to serve: its own Lexer, a fresh Transcript, a live
    /// Session. Revival replays history into the Transcript afterwards.
    fn fresh(
        session: Box<dyn Session>,
        writer: ThreadWriter,
        provider: Provider,
        model: Option<String>,
        resume: Option<String>,
        workspace: Option<WorkspaceBinding>,
        session_project_root: Option<PathBuf>,
    ) -> Self {
        let (lexer, highlights) = Lexer::new();
        Self {
            transcript: Transcript::new(std::sync::Arc::new(lexer)),
            highlights,
            session: Some(session),
            writer,
            provider,
            model,
            title: None,
            pending: None,
            queued: None,
            prompt_history: PromptHistory::new(Vec::new()),
            busy: false,
            resume,
            workspace,
            session_project_root,
            preface_pending: true,
            commands: Vec::new(),
            models: Vec::new(),
            first_prompt_sent: false,
            permission_mode: None,
            timings: HashMap::new(),
        }
    }
}

/// One tool call's clock reading, stamped at ingestion.
#[derive(Debug, Clone, Copy)]
pub enum ToolTiming {
    /// Still in flight — the clock is ticking from here.
    Running(Instant),
    /// Settled, with the whole run measured.
    Done(Duration),
}

impl ToolTiming {
    /// Time on the clock — still growing for a running call.
    pub fn elapsed(&self) -> Duration {
        match self {
            ToolTiming::Running(since) => since.elapsed(),
            ToolTiming::Done(total) => *total,
        }
    }
}

pub struct Cockpit {
    threads: BTreeMap<ThreadId, Thread>,
    store: Store,
    /// The workspace registry (#29): registered projects and their
    /// worktrees, living beside the Thread logs. The selector reads it; the
    /// bootstrap places worktrees through it.
    registry: Registry,
    groups: Groups,
    /// What is on screen (#28): the roster of Panes, focus, view, fullscreen
    /// and park order. Read through `roster()`; changed only by the acts
    /// below, which keep it showing exactly the open Threads plus drafts.
    roster: Roster,
    spawner: Box<dyn Spawner>,
    sampler: Option<Box<dyn RssSampler>>,
    /// Bytes one Session may hold before the watchdog replaces it.
    limit: u64,
    #[cfg(test)]
    refuse_park: std::collections::HashSet<ThreadId>,
}

impl Cockpit {
    pub fn try_new(store: Store, spawner: Box<dyn Spawner>) -> io::Result<Self> {
        let registry = Registry::open(store.dir())?;
        let groups = Groups::load(store.dir())?;
        Ok(Self {
            threads: BTreeMap::new(),
            store,
            registry,
            groups,
            roster: Roster::default(),
            spawner,
            sampler: None,
            limit: u64::MAX,
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

    /// A Group change (#28), and what it does to the view: the Group the
    /// operator was looking at may just have dissolved under them. Its
    /// survivor is the one Thread still on screen, so that is where they
    /// land — never the Thread that left, and never a blank Cockpit.
    pub fn apply_group(&mut self, change: GroupChange) -> Result<Applied, ApplyError> {
        let applied = self.groups.apply(change)?;
        if let View::Group(active) = self.roster.view() {
            if let Some(dissolved) = applied
                .dissolved
                .iter()
                .find(|dissolved| dissolved.group == active)
            {
                self.roster.set_view(View::Solo);
                self.roster.focus(PaneIdentity::Thread(dissolved.survivor));
            }
        }
        self.roster.heal_focus(&self.groups);
        Ok(applied)
    }

    pub fn plan_group_drop(&self, drag: Drag, target: DropTarget) -> Plan {
        self.groups.preview_drop(drag, target)
    }

    /// Register a project root (idempotent, canonicalized) — the selector's
    /// type-a-path row and the launch seed both land here.
    pub fn register_project(&mut self, root: &Path) -> io::Result<ProjectId> {
        self.registry.register(root)
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
                Some((*id, sampler.sample(*id, session.pid())?))
            })
            .filter(|(_, rss)| *rss > self.limit)
            .collect();

        let mut restarts = Vec::new();
        for (id, rss) in over {
            let Some(thread) = self.threads.get_mut(&id) else {
                continue;
            };
            let resume = thread.resume.clone();
            let cwd = workspace::effective_cwd(
                thread.session_project_root.as_deref(),
                thread.workspace.as_ref(),
            )
            .map(Path::to_path_buf);
            // Drop the old Session before asking for a new one: the leaking
            // process must not outlive its replacement.
            thread.session = None;
            let spawned = self.spawner.spawn(SpawnRequest {
                provider: thread.provider,
                model: thread.model.as_deref(),
                resume: resume.as_deref(),
                cwd: cwd.as_deref(),
            });
            let note = match spawned {
                Ok(session) => {
                    thread.session = Some(session);
                    // A fresh Session knows nothing: its first prompt must
                    // carry the session-context preface again.
                    thread.preface_pending = true;
                    format!("restarted — the Session had grown to {}", megabytes(rss))
                }
                Err(e) => format!("restart failed after {}: {e}", megabytes(rss)),
            };
            thread.transcript.apply(Input::Notice(note));
            restarts.push(Restart { thread: id, rss });
        }
        restarts
    }

    /// Start a Thread: a durable log, a workspace to work in, and a Session
    /// serving it. Called at first send (#29) — the draft Pane's bootstrap —
    /// so a failure anywhere leaves NO Thread behind: a worktree that cannot
    /// be created or adopted, and a Session that will not spawn, both take
    /// the half-born Thread with them and the Pane stays draft.
    ///
    /// The choice's repo registers as a project (idempotent — this is how
    /// the registry grows as Threads bind roots); a new worktree is placed
    /// by the registry's central layout on a branch it mints; an existing
    /// worktree passes the adoption conflict check and skips creation.
    pub fn open(&mut self, provider: Provider, workspace: WorkspaceChoice) -> io::Result<ThreadId> {
        self.open_choice(
            ProviderChoice {
                provider,
                model: None,
            },
            workspace,
        )
    }

    fn open_choice(
        &mut self,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
    ) -> io::Result<ThreadId> {
        let ProviderChoice { provider, model } = choice;
        let root = match &workspace {
            WorkspaceChoice::Main { checkout } => checkout,
            WorkspaceChoice::NewWorktree { repo } => repo,
            WorkspaceChoice::ExistingWorktree { repo, .. } => repo,
        };
        let project = self.registry.register(root)?;
        let (binding, fresh_branch) = match workspace {
            WorkspaceChoice::Main { checkout } => (WorkspaceBinding::Main { checkout }, None),
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
            WorkspaceChoice::ExistingWorktree { repo, path } => {
                (WorkspaceBinding::Worktree { repo, path }, None)
            }
        };
        let created =
            self.store
                .create_with_model(provider, model.clone(), Some(project), binding.clone());
        let (id, writer) = match created {
            Ok(created) => created,
            Err(error) => {
                if fresh_branch.is_some() {
                    self.registry.remove_worktree(binding.cwd())?;
                }
                return Err(error);
            }
        };
        if let Err(e) = ensure_workspace(&self.registry, &binding, id) {
            if fresh_branch.is_some() {
                let _ = self.registry.remove_worktree(binding.cwd());
            }
            let _ = self.store.delete(id);
            return Err(io::Error::other(e));
        }
        let session = match self.spawner.spawn(SpawnRequest {
            provider,
            model: model.as_deref(),
            resume: None,
            cwd: workspace::effective_cwd(None, Some(&binding)),
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
            Thread::fresh(session, writer, provider, model, None, Some(binding), None),
        );
        self.roster.insert_thread(id);
        Ok(id)
    }

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
        let id = self.open_choice(choice, workspace)?;
        if let Some(binding) = self.thread(id).and_then(|open| open.workspace()) {
            let notice = format!("opened in {}", binding.cwd().display());
            self.threads
                .get_mut(&id)
                .expect("open inserted the Thread")
                .transcript
                .apply(Input::Notice(notice));
        }
        let delivered = deliver(
            self.threads.get_mut(&id).expect("open inserted the Thread"),
            prompt.to_string(),
        );
        if let Err(error) = delivered {
            self.threads.remove(&id);
            self.roster.remove_thread(id);
            self.store.delete(id)?;
            return Err(error);
        }
        Ok(id)
    }

    /// Bootstrap a draft into an existing group as one operator transaction.
    /// Expected group failures happen before a Thread or prompt is created;
    /// a later persistence failure rolls the new Thread back.
    pub fn bootstrap_in_group(
        &mut self,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
        group: GroupId,
    ) -> io::Result<ThreadId> {
        let root = match &workspace {
            WorkspaceChoice::Main { checkout } => checkout,
            WorkspaceChoice::NewWorktree { repo } => repo,
            WorkspaceChoice::ExistingWorktree { repo, .. } => repo,
        };
        let project = self.registry.register(root)?;
        self.groups
            .validate_join_project(group, Some(project))
            .map_err(io::Error::other)?;

        let id = self.open_choice(choice, workspace)?;
        if let Err(error) = self.groups.apply(GroupChange::Join {
            thread: id,
            group,
            index: None,
        }) {
            self.threads.remove(&id);
            self.roster.remove_thread(id);
            self.store.delete(id)?;
            return Err(io::Error::other(error));
        }
        if let Some(binding) = self.thread(id).and_then(|open| open.workspace()) {
            let notice = format!("opened in {}", binding.cwd().display());
            self.threads
                .get_mut(&id)
                .expect("open inserted the Thread")
                .transcript
                .apply(Input::Notice(notice));
        }
        if let Err(error) = deliver(
            self.threads.get_mut(&id).expect("open inserted the Thread"),
            prompt.to_string(),
        ) {
            // Membership is removed durably before the Thread it references.
            self.groups
                .apply(GroupChange::Leave { thread: id })
                .map_err(io::Error::other)?;
            self.threads.remove(&id);
            self.roster.remove_thread(id);
            self.store.delete(id)?;
            return Err(error);
        }
        Ok(id)
    }

    // Legacy #24 fixtures exercise persisted `session_project_root` loading.
    // No production writer exists after the #29 lock.
    #[cfg(test)]
    fn set_session_project_root(
        &mut self,
        thread: ThreadId,
        root: Option<PathBuf>,
    ) -> Result<(), LoadError> {
        match self.threads.get_mut(&thread) {
            Some(state) => {
                state.session = None;
                state.pending = None;
                state.busy = false;
                self.store.set_session_project_root(
                    thread,
                    root.clone(),
                    Some(&mut state.writer),
                )?;
                state.session_project_root = root;
            }
            None => self.store.set_session_project_root(thread, root, None)?,
        }
        if let Some(held) = self
            .threads
            .get_mut(&thread)
            .and_then(|state| state.queued.take())
        {
            self.send(thread, held);
        }
        Ok(())
    }

    /// Re-aim a Thread onto a provider (and optionally a model) before its
    /// first prompt (#25). Refused whole once `first_prompt_sent` — nothing
    /// re-aims a Thread the operator has spoken in. Spawn-new-first, swap
    /// second: a CLI that fails to spawn leaves the old Session serving and
    /// the header untouched, and the error carries the provider's words.
    /// The swap is a fresh Transcript — nothing operator-authored exists
    /// pre-lock, so the old Init and model must not linger — and an eager
    /// respawn: the new Provider's commands and models arrive while the
    /// operator is still choosing. Durable before anything in memory
    /// changes, so a parked-never-prompted Thread revives onto its choice.
    /// Works on a parked Thread too: only the store is touched.
    pub fn set_provider(
        &mut self,
        thread: ThreadId,
        choice: ProviderChoice,
    ) -> Result<(), ProvisionError> {
        let Some(state) = self.threads.get(&thread) else {
            // Parked: the lock reads the log the way a revive would.
            let snapshot = self.store.load(thread).map_err(ProvisionError::Store)?;
            if history_locks(&snapshot.inputs()) {
                return Err(ProvisionError::Locked);
            }
            return self
                .store
                .set_provider(thread, choice.provider, choice.model, None)
                .map_err(ProvisionError::Store);
        };
        if state.first_prompt_sent {
            return Err(ProvisionError::Locked);
        }
        if state.provider == choice.provider && state.model == choice.model {
            return Ok(());
        }
        let cwd = workspace::effective_cwd(
            state.session_project_root.as_deref(),
            state.workspace.as_ref(),
        )
        .map(Path::to_path_buf);
        // No resume: pre-lock there is no conversation for the new
        // provider to reload, and the old provider's id means nothing to it.
        let session = self
            .spawner
            .spawn(SpawnRequest {
                provider: choice.provider,
                model: choice.model.as_deref(),
                resume: None,
                cwd: cwd.as_deref(),
            })
            .map_err(ProvisionError::Spawn)?;
        let state = self.threads.get_mut(&thread).expect("checked above");
        // Durable before the swap: on a refused rewrite the new Session is
        // dropped and the old one keeps serving under the old header.
        self.store
            .set_provider(
                thread,
                choice.provider,
                choice.model.clone(),
                Some(&mut state.writer),
            )
            .map_err(ProvisionError::Store)?;
        // The swap. Session-scoped state dies with the old Session; the
        // fresh Transcript drops its Init, so the footer relabels only when
        // the new Provider speaks. A queued prompt is the operator's own
        // and stays held.
        let (lexer, highlights) = Lexer::new();
        state.transcript = Transcript::new(std::sync::Arc::new(lexer));
        state.highlights = highlights;
        state.session = Some(session);
        state.provider = choice.provider;
        state.model = choice.model;
        state.pending = None;
        state.busy = false;
        state.resume = None;
        state.preface_pending = true;
        state.commands.clear();
        state.models.clear();
        state.permission_mode = None;
        state.timings.clear();
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
        let snapshot = self.store.load(thread).map_err(DeleteError::Load)?;
        let groups_before = self.groups.clone();
        let grouped = self.groups.of(thread).is_some();
        if grouped {
            self.groups
                .apply(GroupChange::Leave { thread })
                .map_err(|error| DeleteError::Io(io::Error::other(error)))?;
        }

        let deleted = (|| {
            if let (None, Some(WorkspaceBinding::Worktree { repo, path })) =
                (snapshot.project_id(), snapshot.workspace())
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
                    self.roster.remove_thread(thread);
                    workspace::remove_worktree(&repo, &path).map_err(DeleteError::Git)?;
                }
                // The menu forgets the removed tree; the branch stays git's.
                self.registry
                    .remove_worktree(&path)
                    .map_err(DeleteError::Io)?;
            }
            self.threads.remove(&thread);
            self.roster.remove_thread(thread);
            self.store.delete(thread).map_err(DeleteError::Io)
        })();

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
        let Some(mut state) = self.threads.remove(&thread) else {
            return Ok(());
        };
        self.roster.remove_thread(thread);
        state.session = None;
        #[cfg(test)]
        if self.refuse_park.contains(&thread) {
            return Err(io::Error::other("stub refused to park"));
        }
        state.writer.flush()
    }

    /// Reopen a parked Thread: its history is replayed from the log into a
    /// fresh Transcript, and the new Session is told where to resume.
    pub fn revive(&mut self, thread: ThreadId) -> Result<(), LoadError> {
        let snapshot = self.store.load(thread)?;
        let provider = snapshot.provider();
        let workspace = snapshot.workspace();
        // On demand also means back on demand: a worktree deleted while the
        // Thread was parked is recreated on its own branch before anything
        // spawns into it.
        if let Some(binding) = &workspace {
            ensure_workspace(&self.registry, binding, thread)
                .map_err(|e| LoadError::Io(io::Error::other(e)))?;
        }
        let session_project_root = snapshot.session_project_root();
        let cwd = workspace::effective_cwd(session_project_root.as_deref(), workspace.as_ref())
            .map(Path::to_path_buf);
        let model = snapshot.model();
        let title = snapshot.title().map(str::to_string);
        let session = self
            .spawner
            .spawn(SpawnRequest {
                provider,
                model: model.as_deref(),
                resume: snapshot.resume_target(),
                cwd: cwd.as_deref(),
            })
            .map_err(LoadError::Io)?;
        let writer = self.store.writer(thread)?;

        let resume = snapshot.resume_target().map(|target| target.to_string());
        let mut state = Thread::fresh(
            session,
            writer,
            provider,
            model,
            resume,
            workspace,
            session_project_root,
        );
        state.title = title;
        state.prompt_history = PromptHistory::new(snapshot.prompt_texts());
        // The clocks come back with the history: a settled call's duration
        // was measured when it ran and written down, so the replayed rows
        // draw the same durations they drew before the restart.
        state.timings = snapshot
            .tool_durations()
            .into_iter()
            .map(|(id, total)| (id, ToolTiming::Done(total)))
            .collect();
        let inputs = snapshot.inputs();
        // The lock arms with the history (#25): a replayed operator prompt
        // is a first prompt already sent.
        state.first_prompt_sent = history_locks(&inputs);
        for input in inputs {
            state.transcript.apply(input);
        }
        state.transcript.apply(Input::Revived);

        self.threads.insert(thread, state);
        self.roster.insert_thread(thread);
        Ok(())
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
        crate::import::import_registered(&self.store, &mut self.registry, path)
    }

    /// Send a prompt now: on the wire, in the transcript, in the log. A
    /// Thread whose Session was ended under it — a changed session project
    /// root, a failed watchdog restart — respawns here through the same
    /// resume path a revive uses, which re-arms the preface.
    pub fn send(&mut self, thread: ThreadId, text: String) {
        let Some(state) = self.threads.get_mut(&thread) else {
            return;
        };
        state.prompt_history.reset();
        // Guarded before anything spawns: a refused send must not leave a
        // fresh provider process behind. `deliver` guards again for the
        // sends that never pass through here.
        if let Some(refusal) = vanished_root_refusal(state) {
            state.transcript.apply(Input::Notice(refusal));
            return;
        }
        if state.session.is_none() {
            let resume = state.resume.clone();
            let cwd = workspace::effective_cwd(
                state.session_project_root.as_deref(),
                state.workspace.as_ref(),
            )
            .map(Path::to_path_buf);
            match self.spawner.spawn(SpawnRequest {
                provider: state.provider,
                model: state.model.as_deref(),
                resume: resume.as_deref(),
                cwd: cwd.as_deref(),
            }) {
                Ok(session) => {
                    state.session = Some(session);
                    state.preface_pending = true;
                }
                Err(e) => {
                    state
                        .transcript
                        .apply(Input::Notice(format!("send failed: {e}")));
                    return;
                }
            }
        }
        if let Err(error) = deliver(state, text) {
            state
                .transcript
                .apply(Input::Notice(format!("send failed: {error}")));
        }
    }

    /// Stop the running turn.
    pub fn interrupt(&mut self, thread: ThreadId) {
        let Some(state) = self.threads.get_mut(&thread) else {
            return;
        };
        if let Some(session) = &mut state.session {
            if let Err(e) = session.interrupt() {
                state
                    .transcript
                    .apply(Input::Notice(format!("interrupt failed: {e}")));
            }
        }
    }

    /// Answer a Decision, if it is still the one this Thread waits on.
    pub fn respond(&mut self, thread: ThreadId, decision: &Decision, answer: DecisionAnswer) {
        if !self.answer(thread, &decision.id) {
            return;
        }
        let allowed = !matches!(answer, DecisionAnswer::Deny { .. });
        let Some(state) = self.threads.get_mut(&thread) else {
            return;
        };
        if let Some(session) = &mut state.session {
            if let Err(e) = session.respond_to_decision(&decision.id, answer) {
                state
                    .transcript
                    .apply(Input::Notice(format!("answer failed: {e}")));
            }
        }
        state.transcript.apply(Input::Answered {
            allowed,
            tool_name: decision.tool_name.clone(),
        });
    }

    /// Something Ferrite itself needs to say in a Pane.
    pub fn apply_input(&mut self, thread: ThreadId, input: Input) {
        if let Some(state) = self.threads.get_mut(&thread) {
            state.transcript.apply(input);
        }
    }

    /// One open Thread, read through one handle: everything a Pane, a nav
    /// row or a menu asks about a Thread this Cockpit holds live. `None` is
    /// the one "is it open?" answer; every read after it is infallible and
    /// borrows the Cockpit, not the handle.
    pub fn thread(&self, thread: ThreadId) -> Option<ThreadView<'_>> {
        self.threads.get(&thread).map(|state| ThreadView { state })
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

    /// Header-only facts about a Thread the store holds — what a nav row
    /// says about a parked Thread (#21). One header line off disk, never a
    /// log replay; still I/O, so callers cache it rather than peek per
    /// frame.
    pub fn peek(&self, thread: ThreadId) -> Result<crate::store::ThreadMeta, LoadError> {
        self.store.peek(thread)
    }

    pub fn threads(&self) -> Vec<ThreadId> {
        self.threads.keys().copied().collect()
    }

    /// The durable operator title, whether this Thread is live or parked.
    pub fn thread_title(&self, thread: ThreadId) -> Result<Option<String>, LoadError> {
        if let Some(state) = self.threads.get(&thread) {
            return Ok(state.title.clone());
        }
        self.store.peek(thread).map(|meta| meta.title)
    }

    pub fn rename_thread(&mut self, thread: ThreadId, title: &str) -> io::Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Thread title cannot be blank",
            ));
        }
        let mut state = self.threads.get_mut(&thread);
        self.store
            .set_title(
                thread,
                title.to_string(),
                state.as_mut().map(|state| &mut state.writer),
            )
            .map_err(io::Error::other)?;
        if let Some(state) = state {
            state.title = Some(title.to_string());
        }
        Ok(())
    }

    /// One frame: drain every live Session, fold what arrived, and write it
    /// down. What comes back is only the Panes that actually changed.
    pub fn pump(&mut self) -> Vec<PaneUpdate> {
        let mut frame = Vec::new();
        for (id, thread) in &mut self.threads {
            let Some(session) = &thread.session else {
                continue;
            };
            let events: Vec<SessionEvent> = session.events().try_iter().collect();
            let answers: Vec<Input> = thread.highlights.try_iter().collect();
            if events.is_empty() && answers.is_empty() {
                continue;
            }
            let mut update = PaneUpdate {
                thread: *id,
                dirty: Vec::new(),
                evicted: Vec::new(),
            };
            let mut release = None;
            for event in events {
                // Folded first, then written: the fold is what stops a tool
                // call's clock, and the record carries that reading so a
                // revived Thread keeps its durations.
                if let Wake::Send(held) = fold(thread, &event) {
                    release = Some(held);
                }
                let duration = settled_duration(thread, &event);
                let _ = thread.writer.record_event(&event, duration);
                let applied = thread.transcript.apply(Input::Event(event));
                update.dirty.extend(applied.dirty);
                update.evicted.extend(applied.evicted);
            }
            for answer in answers {
                update.dirty.extend(thread.transcript.apply(answer).dirty);
            }
            if let Some(held) = release {
                if thread.session.is_some() {
                    // Through `deliver`, like any prompt: the preface and
                    // the vanished-root guard apply to held prompts too.
                    if let Ok(sent) = deliver(thread, held) {
                        update.dirty.extend(sent.dirty);
                    }
                }
            }
            frame.push(update);
        }
        frame
    }

    /// Hold a prompt written mid-turn. It stays visible and editable until the
    /// turn ends, which is when it is sent.
    pub fn queue(&mut self, thread: ThreadId, text: String) {
        if let Some(state) = self.threads.get_mut(&thread) {
            state.prompt_history.reset();
            state.queued = Some(text);
        }
    }

    /// Take a held prompt back into the Composer, so a typo written mid-turn
    /// is fixable before it goes out.
    pub fn unqueue(&mut self, thread: ThreadId) -> Option<String> {
        let state = self.threads.get_mut(&thread)?;
        state.prompt_history.reset();
        state.queued.take()
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
        if state
            .pending
            .as_ref()
            .is_none_or(|pending| pending.id != id)
        {
            return false;
        }
        state.pending = None;
        true
    }

    /// Models actually announced by live Sessions of this provider, stable
    /// and deduplicated for a draft that has no Session of its own.
    pub fn announced_models(&self, provider: Provider) -> Vec<String> {
        let mut models = Vec::new();
        for thread in self
            .threads
            .values()
            .filter(|open| open.provider == provider)
        {
            for model in &thread.models {
                if !models.contains(model) {
                    models.push(model.clone());
                }
            }
        }
        models
    }

    /// The next Thread waiting on the operator, after `from`, wrapping. One
    /// key held down walks every Decision in the cockpit and stops nowhere
    /// else.
    pub fn next_blocked(&self, from: Option<ThreadId>) -> Option<ThreadId> {
        let blocked = self.blocked();
        match from {
            Some(after) => blocked
                .iter()
                .find(|id| **id > after)
                .copied()
                .or_else(|| blocked.first().copied()),
            None => blocked.first().copied(),
        }
    }

    /// Threads waiting on the operator — what the wall badges.
    pub fn blocked(&self) -> Vec<ThreadId> {
        self.threads
            .iter()
            .filter(|(_, state)| state.pending.is_some())
            .map(|(id, _)| *id)
            .collect()
    }
}

/// An open Thread's facts, read through `Cockpit::thread`. Every accessor
/// hands back a borrow of the Cockpit itself (`'a`), so a caller keeps the
/// facts it read after the handle is gone.
#[derive(Clone, Copy)]
pub struct ThreadView<'a> {
    state: &'a Thread,
}

impl<'a> ThreadView<'a> {
    pub fn transcript(&self) -> &'a Transcript {
        &self.state.transcript
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

    /// The durable operator title, as this open Thread holds it.
    pub fn title(&self) -> Option<&'a str> {
        self.state.title.as_deref()
    }

    /// What this Thread is blocked on, if anything.
    pub fn pending(&self) -> Option<&'a Decision> {
        self.state.pending.as_ref()
    }

    /// A prompt held back while the turn runs.
    pub fn queued(&self) -> Option<&'a str> {
        self.state.queued.as_deref()
    }

    /// Is a turn running? A prompt written now has to wait for it.
    pub fn busy(&self) -> bool {
        self.state.busy
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
        &self.state.timings
    }

    /// The live Session's command menu (#23): what `/` offers in this
    /// Thread's Composer. Empty until the Session announces one — never a
    /// static list.
    pub fn commands(&self) -> &'a [crate::SessionCommand] {
        &self.state.commands
    }

    /// The models the live Session's install offers (#25): the provider
    /// picker's model rows. Empty until the Session announces a list —
    /// never a static one.
    pub fn models(&self) -> &'a [String] {
        &self.state.models
    }

    /// The live Session's permission mode (#23): the meta row's mode chip.
    /// None until the Session announces one — a chip is never invented.
    pub fn permission_mode(&self) -> Option<&'a str> {
        self.state.permission_mode.as_deref()
    }
}

/// What `Cockpit::close` can refuse: the park that ends a Solo Thread's
/// Session — the Pane is gone either way — or the Group change that takes
/// a member out of the view.
#[derive(Debug)]
pub enum CloseError {
    Park(io::Error),
    Group(ApplyError),
}

impl std::fmt::Display for CloseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CloseError::Park(error) => write!(f, "did not park cleanly: {error}"),
            CloseError::Group(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CloseError {}

/// A draft's first send, done: the Thread it became, and whether the
/// leave the draft was holding open (see `close`) applied.
#[derive(Debug)]
pub struct Bootstrapped {
    pub thread: ThreadId,
    /// The deferred leave the Groups module refused, if it did — the Thread
    /// is live and in its Group regardless.
    pub refused_leave: Option<ApplyError>,
}

/// A session file adopted through `adopt_into` (#11): durable the moment
/// this exists, and open unless `not_opened` says why not.
#[derive(Debug)]
pub struct Adopted {
    pub thread: ThreadId,
    /// The imported Thread would not open: it is durable and parked, exactly
    /// like a launch-time import that would not open.
    pub not_opened: Option<LoadError>,
    /// The blank Thread the door was opened from stayed open: its deletion
    /// was refused.
    pub blank_kept: Option<DeleteError>,
}

/// The operator's acts on what is on screen (#28): every rule about Solo,
/// Group, focus, fullscreen and park order runs here, against the roster,
/// with the reviving, parking and Group changes it takes. The window only
/// mirrors the roster and paints.
impl Cockpit {
    /// What is on screen: read-only. Every change is one of the acts below.
    pub fn roster(&self) -> &Roster {
        &self.roster
    }

    /// The Panes on screen, in order — see `Roster::visible`.
    pub fn visible(&self) -> Vec<PaneIdentity> {
        self.roster.visible(&self.groups)
    }

    /// The grid the visible Panes lay out on — see `Roster::layout`.
    pub fn layout(&self) -> Layout {
        self.roster.layout(&self.groups)
    }

    /// The one door to focus: every move — keys, clicks, nav rows — lands
    /// here, so fullscreen re-aims with focus. False for a Pane that is not
    /// open.
    pub fn focus(&mut self, identity: PaneIdentity) -> bool {
        self.roster.focus(identity)
    }

    /// A running nav row's click (#21): land on that Thread's Pane, in the
    /// view that shows it — its Group, or Solo.
    pub fn focus_thread(&mut self, thread: ThreadId) -> bool {
        let identity = PaneIdentity::Thread(thread);
        if self.roster.index_of(identity).is_none() {
            return false;
        }
        self.roster.set_view(
            self.groups
                .of(thread)
                .map_or(View::Solo, |group| View::Group(group.id)),
        );
        self.roster.focus(identity)
    }

    /// cmd-] / cmd-[: walk the visible Panes, wrapping.
    pub fn step_focus(&mut self, delta: isize) {
        self.roster.step(delta, &self.groups);
    }

    /// cmd-f (#20): the focused Pane takes the whole cockpit; cmd-f again
    /// restores the grid.
    pub fn toggle_fullscreen(&mut self) {
        self.roster.toggle_fullscreen();
    }

    /// Jump to the next Thread waiting on the operator — the whole point of
    /// a Group you cannot read all of at once. The Thread landed on, if any.
    pub fn next_decision(&mut self) -> Option<ThreadId> {
        let next = self.next_blocked(self.roster.focused_thread())?;
        self.focus_thread(next).then_some(next)
    }

    /// Open a Group (#28): every member gets a Pane, parked ones included.
    /// A Group *is* its membership on screen, so entering one that has a
    /// parked member and showing the rest would be showing a different
    /// Group. Focus stays put when the operator was already on a member,
    /// so entering from one of its own rows does not jump them.
    pub fn enter_group(&mut self, group: GroupId) -> Result<(), ReviveGroupError> {
        let members = self.revive_group(group)?;
        let retain = self
            .roster
            .focused_thread()
            .filter(|thread| members.contains(thread));
        for thread in &members {
            self.roster.note_revived(*thread);
        }
        self.roster.set_view(View::Group(group));
        if let Some(thread) = retain.or_else(|| members.first().copied()) {
            self.roster.focus(PaneIdentity::Thread(thread));
        }
        Ok(())
    }

    /// Revive one parked Thread into the view that shows it (#21): a Pane,
    /// focus, and the park order forgetting it — cmd-o must not revive it a
    /// second time. The shared tail of cmd-o and a parked nav row's click.
    pub fn reopen(&mut self, thread: ThreadId) -> Result<(), LoadError> {
        self.revive(thread)?;
        self.roster.set_view(
            self.groups
                .of(thread)
                .map_or(View::Solo, |group| View::Group(group.id)),
        );
        self.roster.note_revived(thread);
        self.roster.focus(PaneIdentity::Thread(thread));
        Ok(())
    }

    /// cmd-o (#17): reopen the Thread parked most recently — the one the
    /// operator just closed, which is the one they want back. The order is
    /// remembered only for this launch: once it is drained — Threads parked
    /// before a relaunch are never in it — the newest-created parked Thread
    /// is next (accepted v1 behavior). A Thread whose revive fails keeps its
    /// park but loses its slot in the order: cmd-o moves on rather than
    /// jamming on it. None with nothing parked at all.
    pub fn reopen_last(&mut self) -> Option<(ThreadId, Result<(), LoadError>)> {
        let thread = self
            .roster
            .pop_park_order()
            .or_else(|| self.parked().unwrap_or_default().last().copied())?;
        Some((thread, self.reopen(thread)))
    }

    /// cmd-t (#29): a draft Pane in the current view's scope — a Group's
    /// pending member, or a loose Solo draft. Nothing durable until its
    /// first send; it takes focus.
    pub fn open_draft(&mut self) -> DraftId {
        let group = match self.roster.view() {
            View::Group(group) => Some(group),
            View::Solo => None,
        };
        self.roster.open_draft(DraftScope {
            group,
            pending_leave: None,
        })
    }

    /// The first send (#29): bootstrap the Thread — create, worktree, spawn
    /// — and only then let the prompt go; the Thread takes the draft's own
    /// slot, and joins the Group the draft was pending in. The leave the
    /// draft was holding open (see `close`) applies now, and only now: the
    /// new member is already in, so the pair never dissolves. On any
    /// failure nothing is half-born: no Thread, the Pane stays draft, and
    /// the prompt stays with the caller.
    pub fn bootstrap_draft(
        &mut self,
        draft: DraftId,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
    ) -> io::Result<Bootstrapped> {
        let scope = self
            .roster
            .draft_scope(draft)
            .ok_or_else(|| io::Error::other("no such draft"))?;
        let thread = match scope.group {
            Some(group) => self.bootstrap_in_group(choice, workspace, prompt, group)?,
            None => self.bootstrap(choice, workspace, prompt)?,
        };
        self.roster.draft_became(draft, thread);
        let refused_leave = scope
            .pending_leave
            .and_then(|leaving| self.apply_group(GroupChange::Leave { thread: leaving }).err());
        Ok(Bootstrapped {
            thread,
            refused_leave,
        })
    }

    /// Discard a draft Pane (#29): nothing durable dies with it. A leave it
    /// was holding open applies now — nothing came to replace the leaving
    /// Thread, so the pair dissolves exactly as closing that Pane would
    /// have done without a draft in the way.
    pub fn discard_draft(&mut self, draft: DraftId) -> Result<(), ApplyError> {
        let Some(scope) = self.roster.remove_draft(draft) else {
            return Ok(());
        };
        if let (Some(_), Some(leaving)) = (scope.group, scope.pending_leave) {
            self.apply_group(GroupChange::Leave { thread: leaving })?;
        }
        Ok(())
    }

    /// Close a Pane — cmd-w. A draft is discarded. A Thread is taken out of
    /// whatever the operator is looking at: in Solo that is a park, plain —
    /// the Session ends, the log stays, and reopening revives it. In a
    /// Group it is a Leave, and the Thread stays open — and the survivor
    /// the operator lands on is the Thread that took this one's ordinal,
    /// not the first member: closing the middle Pane of three should leave
    /// the pointer where it was, the way closing a browser tab does.
    ///
    /// The exception is a pair with a draft pending in it. A Group needs
    /// two members, so leaving a pair dissolves it — which would tear down
    /// the very Group the draft is waiting to join. The leave is therefore
    /// *deferred* onto the draft, and the Group stays whole and visible
    /// until the draft either sends (`bootstrap_draft` applies the leave;
    /// membership never dips below two) or is discarded (the leave applies
    /// then, and the pair dissolves as it always would).
    pub fn close(&mut self, identity: PaneIdentity) -> Result<(), CloseError> {
        match identity {
            PaneIdentity::Draft(draft) => self.discard_draft(draft).map_err(CloseError::Group),
            PaneIdentity::Thread(thread) => self.close_thread(thread),
        }
    }

    fn close_thread(&mut self, thread: ThreadId) -> Result<(), CloseError> {
        if let View::Group(group) = self.roster.view() {
            let members = self
                .groups
                .get(group)
                .map(|group| group.members.clone())
                .unwrap_or_default();
            let ordinal = members
                .iter()
                .position(|member| *member == thread)
                .unwrap_or(0);
            if members.len() == 2 {
                if let Some(draft) = self.roster.pending_draft(group) {
                    self.roster.defer_leave(draft, thread);
                    let survivor = members[usize::from(members[0] == thread)];
                    self.roster.focus(PaneIdentity::Thread(survivor));
                    return Ok(());
                }
            }
            let applied = self
                .apply_group(GroupChange::Leave { thread })
                .map_err(CloseError::Group)?;
            // A dissolved Group already landed on its survivor.
            if !applied.dissolved.iter().any(|item| item.group == group) {
                if let Some(group) = self.groups.get(group) {
                    let next = group.members[ordinal.min(group.members.len() - 1)];
                    self.roster.focus(PaneIdentity::Thread(next));
                }
            }
            return Ok(());
        }
        // Solo: park. Parked even on a flush error — the Session is gone
        // either way, so cmd-o should still bring this Thread back first —
        // and the clamped survivor takes focus and, while fullscreen, the
        // screen (#20).
        let re_aim = self.roster.fullscreen() == Some(PaneIdentity::Thread(thread));
        let parked = self.park(thread);
        self.roster.note_parked(thread);
        if re_aim {
            self.roster.fullscreen_focused();
        }
        parked.map_err(CloseError::Park)
    }

    /// A nav drag's drop (#28): the plan the Groups module makes of it,
    /// applied in the View the drag started from — the row's own mouse-down
    /// fires before the drag does, so by the time the drop lands the view
    /// is wherever the *press* took the operator, not where they picked the
    /// row up. Dragging a member out of a Group is the pointer's spelling
    /// of closing its Pane, so it takes the same door — which defers the
    /// leave when a draft is pending in a pair, and keeps the
    /// ordinal-preserving focus. A refused or empty plan changes nothing.
    pub fn drop(&mut self, drag: Drag, origin: View, target: DropTarget) -> Result<(), ApplyError> {
        self.roster.set_view(origin);
        let outcome = match self.groups.preview_drop(drag, target) {
            Plan::Change(GroupChange::Leave { thread })
                if matches!(drag, Drag::Thread { group: Some(_), .. }) =>
            {
                let Drag::Thread {
                    group: Some(group), ..
                } = drag
                else {
                    unreachable!("the guard above matched this exact shape");
                };
                self.roster.set_view(View::Group(group));
                let closed = self.close_thread(thread).map_err(|error| match error {
                    CloseError::Group(error) => error,
                    CloseError::Park(error) => error.into(),
                });
                // Out of the Group for real — either the leave applied, or
                // it is parked on a draft. Either way the dragged Thread is
                // no longer a member, so the operator follows it to Solo.
                if self.roster.pending_leave(group) == Some(thread)
                    || self.groups.of(thread).is_none()
                {
                    self.roster.set_view(View::Solo);
                    self.roster.focus(PaneIdentity::Thread(thread));
                }
                closed
            }
            Plan::Change(change) => self.apply_group(change).map(|_| ()),
            Plan::Refused(_) | Plan::Nothing => Ok(()),
        };
        self.roster.heal_focus(&self.groups);
        outcome
    }

    /// Adopt a CLI session file (#11) in place of the Pane it was picked
    /// from: a draft becomes the imported Thread; a still-blank Thread
    /// yields its slot and is deleted — clean exactly while it is blank,
    /// which the picker's own invariant guarantees and this re-checks.
    /// Import creates the Thread; revive opens it — the same
    /// replay-and-resume any parked Thread gets — and it takes focus. A
    /// refusal is the import module's, unchanged, and no Thread was
    /// created.
    pub fn adopt_into(
        &mut self,
        from: PaneIdentity,
        path: &Path,
    ) -> Result<Adopted, crate::import::ImportError> {
        let thread = self.import(path)?;
        let mut adopted = Adopted {
            thread,
            not_opened: None,
            blank_kept: None,
        };
        match self.revive(thread) {
            Ok(()) => {
                match from {
                    PaneIdentity::Draft(draft) => {
                        self.roster.draft_became(draft, thread);
                    }
                    PaneIdentity::Thread(blank) => {
                        if self
                            .thread(blank)
                            .is_some_and(|open| open.transcript().offers_import())
                        {
                            adopted.blank_kept = self.delete(blank).err();
                        }
                    }
                }
                self.roster.focus(PaneIdentity::Thread(thread));
            }
            Err(error) => adopted.not_opened = Some(error),
        }
        Ok(adopted)
    }

    /// The nav's parked rows (#21), in stable, append-only order: Threads
    /// parked before this launch keep creation order, and this launch's
    /// parks append below in park order — a fresh park lands at the bottom
    /// of the section instead of re-sorting it.
    pub fn parked_in_order(&self) -> io::Result<Vec<ThreadId>> {
        let parked = self.parked()?;
        let order = self.roster.park_order();
        Ok(parked
            .iter()
            .filter(|thread| !order.contains(thread))
            .copied()
            .chain(order.iter().filter(|thread| parked.contains(thread)).copied())
            .collect())
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{} MB", bytes / (1024 * 1024))
}

/// The first-prompt lock, read off a replayed history (#25, #29): an
/// operator prompt in the log is a first prompt already sent. The one rule
/// for every Thread that is not live — a revive arming its state, and a
/// parked `set_provider` judging the log directly.
fn history_locks(inputs: &[Input]) -> bool {
    inputs.iter().any(|input| matches!(input, Input::Prompt(_)))
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
fn deliver(state: &mut Thread, text: String) -> io::Result<Update> {
    if let Some(refusal) = vanished_root_refusal(state) {
        return Err(io::Error::new(io::ErrorKind::NotFound, refusal));
    }
    let Some(session) = &mut state.session else {
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
    session.send(prefaced.as_deref().unwrap_or(&text))?;
    // The Session's first prompt has gone out; every later one is bare.
    state.preface_pending = false;
    // And the Thread's first locks its provider for good (#25, #29).
    state.first_prompt_sent = true;
    let _ = state.writer.record_prompt(&text);
    state.prompt_history.append(text.clone());
    Ok(state.transcript.apply(Input::Prompt(text)))
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
    // Anything already standing at the path must BE the worktree — a
    // directory that merely exists there (squatting where one was, or was
    // never) must not be silently rebuilt over or spawned into.
    if path.exists() {
        return registry::adoption_check(repo, path);
    }
    let branch = registry
        .branch_for(path)
        .map(str::to_string)
        .unwrap_or_else(|| branch_name(thread));
    workspace::ensure_worktree(repo, path, &branch)
}

/// The branch a Thread's worktree lives on. Named for the Thread so the
/// operator can read `git branch` and know whose work each one is.
fn branch_name(thread: ThreadId) -> String {
    format!("ferrite/thread-{thread}")
}

/// The clock reading this event settled, for the log to keep. Only a
/// completed tool call has one, and only where the fold just stopped it —
/// a call whose start this cockpit never saw stays clockless.
fn settled_duration(state: &Thread, event: &SessionEvent) -> Option<Duration> {
    let SessionEvent::ToolCompleted { id, .. } = event else {
        return None;
    };
    match state.timings.get(id) {
        Some(ToolTiming::Done(total)) => Some(*total),
        _ => None,
    }
}

/// The bookkeeping half of a fold: what the operator is on the hook for.
fn fold(state: &mut Thread, event: &SessionEvent) -> Wake {
    let mut wake = Wake::Nothing;
    match event {
        SessionEvent::TextDelta { .. }
        | SessionEvent::ThinkingDelta { .. }
        | SessionEvent::ReasoningSummaryDelta { .. } => state.busy = true,
        // The call's clock starts and stops at ingestion — the only clock
        // durations ever get (transcript folds keep none).
        SessionEvent::ToolStarted { id, .. } => {
            state.busy = true;
            state
                .timings
                .insert(id.clone(), ToolTiming::Running(Instant::now()));
        }
        SessionEvent::ToolCompleted { id, .. } => {
            if let Some(ToolTiming::Running(since)) = state.timings.get(id) {
                let total = since.elapsed();
                state.timings.insert(id.clone(), ToolTiming::Done(total));
            }
        }
        // A turn that ends takes its Decision with it: the provider is no
        // longer waiting, so an answer would go nowhere. Anything the operator
        // wrote behind the turn goes out now.
        SessionEvent::TurnEnded { .. } | SessionEvent::Closed { .. } => {
            state.pending = None;
            state.busy = false;
            if let Some(held) = state.queued.take() {
                wake = Wake::Send(held);
            }
        }
        SessionEvent::DecisionRequested { decision } => {
            state.pending = Some(decision.clone());
        }
        // The Session announced its command menu; the Composer's `/` popover
        // reads it from here.
        SessionEvent::Commands { commands } => {
            state.commands = commands.clone();
        }
        // And its model menu — the provider picker's rows (#25).
        SessionEvent::Models { models } => {
            state.models = models.clone();
        }
        // And its permission mode — the meta row's chip.
        SessionEvent::PermissionMode { mode } => {
            state.permission_mode = Some(mode.clone());
        }
        // Kept for the watchdog: a replacement Session resumes from the newest
        // id the provider gave, even one it renamed mid-Thread.
        SessionEvent::Init { session_id, .. } => {
            state.resume = Some(session_id.clone());
        }
        _ => {}
    }
    wake
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::mpsc::{self, Sender};

    use crate::store::{Provider, Store};
    use crate::transcript::{Body, Class};

    /// A fresh per-test scratch directory.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-cockpit-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A Session with no process behind it: the test pushes the events a
    /// provider would have streamed, and reads back what Ferrite sent.
    struct Scripted {
        rx: Receiver<SessionEvent>,
        sent: Rc<RefCell<Vec<String>>>,
        fail_send: Rc<RefCell<bool>>,
    }

    impl crate::providers::Session for Scripted {
        fn events(&self) -> &Receiver<SessionEvent> {
            &self.rx
        }

        fn send(&mut self, text: &str) -> std::io::Result<()> {
            if *self.fail_send.borrow() {
                return Err(io::Error::other("stub refused first prompt"));
            }
            self.sent.borrow_mut().push(text.to_string());
            Ok(())
        }

        fn interrupt(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        fn respond_to_decision(
            &mut self,
            _id: &str,
            _answer: crate::DecisionAnswer,
        ) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Hands out Scripted Sessions and keeps the handles the test drives them
    /// with. Nothing here spawns a process.
    #[derive(Clone, Default)]
    struct Fake {
        streams: Rc<RefCell<Vec<Sender<SessionEvent>>>>,
        sent: Rc<RefCell<Vec<String>>>,
        providers: Rc<RefCell<Vec<Provider>>>,
        models: Rc<RefCell<Vec<Option<String>>>>,
        resumed: Rc<RefCell<Vec<Option<String>>>>,
        cwds: Rc<RefCell<Vec<Option<std::path::PathBuf>>>>,
        /// Every spawn call, successes and refusals alike.
        attempts: Rc<RefCell<usize>>,
        /// While set, spawn refuses — how a test makes a restart fail.
        fail: Rc<RefCell<bool>>,
        fail_at: Rc<RefCell<Option<usize>>>,
        fail_send: Rc<RefCell<bool>>,
    }

    impl Fake {
        /// Every spawn's choice, in call order.
        fn spawn_pairs(&self) -> Vec<ProviderChoice> {
            self.providers
                .borrow()
                .iter()
                .zip(self.models.borrow().iter())
                .map(|(provider, model)| ProviderChoice {
                    provider: *provider,
                    model: model.clone(),
                })
                .collect()
        }
    }

    impl Spawner for Fake {
        fn spawn(
            &mut self,
            request: SpawnRequest,
        ) -> std::io::Result<Box<dyn crate::providers::Session>> {
            *self.attempts.borrow_mut() += 1;
            let attempt = *self.attempts.borrow();
            if *self.fail.borrow() || *self.fail_at.borrow() == Some(attempt) {
                return Err(std::io::Error::other("stub refused to spawn"));
            }
            let (tx, rx) = mpsc::channel();
            self.streams.borrow_mut().push(tx);
            self.providers.borrow_mut().push(request.provider);
            self.models
                .borrow_mut()
                .push(request.model.map(|model| model.to_string()));
            self.resumed
                .borrow_mut()
                .push(request.resume.map(|target| target.to_string()));
            self.cwds
                .borrow_mut()
                .push(request.cwd.map(|path| path.to_path_buf()));
            Ok(Box::new(Scripted {
                rx,
                sent: self.sent.clone(),
                fail_send: self.fail_send.clone(),
            }))
        }
    }

    /// The binding for tests that are not about bindings.
    fn main_choice() -> WorkspaceChoice {
        WorkspaceChoice::Main {
            checkout: std::env::temp_dir(),
        }
    }

    fn cockpit(name: &str) -> (Cockpit, Fake) {
        let fake = Fake::default();
        let store = Store::open(scratch(name)).unwrap();
        (Cockpit::new(store, Box::new(fake.clone())), fake)
    }

    /// An initialised repo with one committed file, for binding tests.
    /// Always under a scratch directory — never near a real checkout.
    fn init_repo(root: &Path) -> std::path::PathBuf {
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| crate::workspace::git_for_tests(&repo, args);
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("file.txt"), "base\n").unwrap();
        git(&["add", "file.txt"]);
        git(&[
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "user.name=test",
            "commit",
            "-qm",
            "base",
        ]);
        repo
    }

    /// The whole binding flow at the cockpit's own seam: the choice becomes
    /// a worktree on disk, the Session spawns inside it, and the Pane's
    /// chrome can say so.
    #[test]
    fn opening_with_a_worktree_choice_creates_it_and_spawns_the_session_inside() {
        let root = scratch("worktree-open");
        let (mut cockpit, fake) = cockpit("worktree-open-store");
        let repo = init_repo(&root);

        let thread = cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::NewWorktree { repo: repo.clone() },
            )
            .unwrap();

        let Some(WorkspaceBinding::Worktree { path, repo: bound }) =
            cockpit.thread(thread).and_then(|open| open.workspace()).cloned()
        else {
            panic!(
                "the Pane must know its binding: {:?}",
                cockpit.thread(thread).and_then(|open| open.workspace())
            );
        };
        assert_eq!(bound, repo);
        assert!(path.join(".git").exists(), "no worktree at {path:?}");
        assert_eq!(
            fake.cwds.borrow().last().unwrap().as_deref(),
            Some(path.as_path()),
            "the Session must spawn inside the worktree"
        );
    }

    /// #29: opening registers the choice's repo as a project (the registry
    /// grows as Threads bind roots), a new worktree lands in the central
    /// layout on a registry-minted branch, and the Thread header carries
    /// the project id.
    #[test]
    fn opening_registers_the_project_and_places_the_worktree_centrally() {
        let root = scratch("central-open");
        let dir = scratch("central-open-store");
        let repo = init_repo(&root);
        let fake = Fake::default();
        let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));

        let thread = cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::NewWorktree { repo: repo.clone() },
            )
            .unwrap();

        let projects = cockpit.registry().projects();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].title, "repo");
        let project = projects[0].id;
        assert_eq!(cockpit.peek(thread).unwrap().project_id, Some(project));

        let entries = cockpit.registry().worktrees(project);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].branch, "ferrite/wt-1");
        let path = cockpit.thread(thread).and_then(|open| open.workspace()).unwrap().cwd().to_path_buf();
        assert_eq!(entries[0].path, path);
        // The central layout: under the store's worktrees/, never inside
        // the repo or a per-Thread store directory.
        assert!(
            path.starts_with(dir.join("worktrees")),
            "central layout: {path:?}"
        );
        assert_eq!(path.file_name().unwrap(), "ferrite-wt-1");
    }

    /// #29: choosing an existing worktree adopts the standing tree — no
    /// creation, no new branch — and the Session spawns into it.
    #[test]
    fn opening_with_an_existing_worktree_adopts_the_standing_tree() {
        let root = scratch("adopt-open");
        let (mut cockpit, fake) = cockpit("adopt-open-store");
        let repo = init_repo(&root);
        let first = cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::NewWorktree { repo: repo.clone() },
            )
            .unwrap();
        let path = cockpit.thread(first).and_then(|open| open.workspace()).unwrap().cwd().to_path_buf();
        // The first Thread is gone; its worktree outlives it as the
        // adoptable row. Deleting would remove the tree, so park instead.
        cockpit.park(first).unwrap();

        let second = cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::ExistingWorktree {
                    repo: repo.clone(),
                    path: path.clone(),
                },
            )
            .unwrap();

        assert_eq!(
            cockpit.thread(second).and_then(|open| open.workspace()),
            Some(&WorkspaceBinding::Worktree {
                repo: repo.clone(),
                path: path.clone(),
            })
        );
        assert_eq!(
            fake.cwds.borrow().last().unwrap().as_deref(),
            Some(path.as_path())
        );
        // Adoption minted nothing: one worktree entry, one branch.
        let project = cockpit.registry().projects()[0].id;
        assert_eq!(cockpit.registry().worktrees(project).len(), 1);
        let branches = crate::workspace::git_for_tests(&repo, &["branch", "--list", "ferrite/*"]);
        assert_eq!(branches.matches("ferrite/").count(), 1, "{branches:?}");
    }

    /// #29: the adoption conflict check — a directory that git does not
    /// call a worktree of the repo refuses the whole open, and no Thread
    /// is left behind.
    #[test]
    fn adopting_a_directory_that_is_not_a_worktree_refuses_and_leaves_no_thread() {
        let root = scratch("adopt-refused");
        let (mut cockpit, _fake) = cockpit("adopt-refused-store");
        let repo = init_repo(&root);
        let squatter = root.join("squatter");
        std::fs::create_dir_all(&squatter).unwrap();

        let refused = cockpit.open(
            Provider::Claude,
            WorkspaceChoice::ExistingWorktree {
                repo: repo.clone(),
                path: squatter.clone(),
            },
        );

        assert!(refused.is_err(), "a squatting directory must refuse");
        assert!(cockpit.threads().is_empty());
        assert_eq!(cockpit.parked().unwrap(), vec![], "no half-born Thread");
    }

    /// #29's bootstrap failure contract: a Session that will not spawn
    /// takes the half-born Thread with it — the draft Pane stays draft and
    /// nothing claims to exist. The worktree just created stays, real and
    /// registered: it is adoptable, not half-born.
    #[test]
    fn a_failed_spawn_at_bootstrap_leaves_no_thread_behind() {
        let root = scratch("bootstrap-fails");
        let (mut cockpit, fake) = cockpit("bootstrap-fails-store");
        let repo = init_repo(&root);
        *fake.fail.borrow_mut() = true;

        let refused = cockpit.open(
            Provider::Claude,
            WorkspaceChoice::NewWorktree { repo: repo.clone() },
        );

        assert!(refused.is_err());
        assert!(cockpit.threads().is_empty());
        assert_eq!(cockpit.parked().unwrap(), vec![], "no half-born Thread");
        let project = cockpit.registry().projects()[0].id;
        assert_eq!(
            cockpit.registry().worktrees(project).len(),
            1,
            "the created worktree survives as an adoptable entry"
        );
    }

    #[test]
    fn a_failed_thread_create_removes_the_reserved_worktree() {
        let root = scratch("create-fails-after-reserve");
        let repo = init_repo(&root);
        let store = Store::open(scratch("create-fails-after-reserve-store"))
            .unwrap()
            .refuse_create();
        let mut cockpit = Cockpit::new(store, Box::new(Fake::default()));

        let refused = cockpit.open(Provider::Claude, WorkspaceChoice::NewWorktree { repo });

        assert!(refused.is_err());
        assert!(cockpit.threads().is_empty());
        let project = cockpit.registry().projects()[0].id;
        assert!(cockpit.registry().worktrees(project).is_empty());
    }

    #[test]
    fn a_failed_first_send_leaves_no_thread_or_log() {
        let root = scratch("first-send-fails");
        let (mut cockpit, fake) = cockpit("first-send-fails-store");
        let repo = init_repo(&root);
        *fake.fail_send.borrow_mut() = true;

        let refused = cockpit.bootstrap(
            ProviderChoice {
                provider: Provider::Codex,
                model: Some("announced-model".into()),
            },
            WorkspaceChoice::Main { checkout: repo },
            "exact drafted prompt",
        );

        assert!(refused.is_err());
        assert!(cockpit.threads().is_empty());
        assert!(cockpit.parked().unwrap().is_empty(), "the log was removed");
        assert!(fake.sent.borrow().is_empty());
    }

    #[test]
    fn grouped_bootstrap_persists_membership_before_sending() {
        let root = scratch("group-bootstrap-persist");
        let repo = init_repo(&root);
        let store_dir = scratch("group-bootstrap-persist-store");
        let fake = Fake::default();
        let mut cockpit = Cockpit::new(Store::open(&store_dir).unwrap(), Box::new(fake.clone()));
        let seed = cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::Main {
                    checkout: repo.clone(),
                },
            )
            .unwrap();
        let second = cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::Main {
                    checkout: repo.clone(),
                },
            )
            .unwrap();
        let group = cockpit
            .apply_group(GroupChange::Create {
                first: seed,
                second,
            })
            .unwrap()
            .group
            .unwrap();
        let groups_path = store_dir.join("groups.json");
        std::fs::remove_file(&groups_path).unwrap();
        std::fs::create_dir(&groups_path).unwrap();

        let refused = cockpit.bootstrap_in_group(
            ProviderChoice {
                provider: Provider::Claude,
                model: None,
            },
            WorkspaceChoice::Main { checkout: repo },
            "must not be sent",
            group,
        );

        assert!(refused.is_err());
        assert_eq!(fake.sent.borrow().as_slice(), &[] as &[String]);
        assert_eq!(cockpit.threads(), vec![seed, second]);
        assert_eq!(
            cockpit.groups().get(group).unwrap().members,
            vec![seed, second]
        );
        assert!(cockpit.parked().unwrap().is_empty());
    }

    #[test]
    fn opening_on_the_main_checkout_spawns_the_session_there() {
        let root = scratch("main-open");
        let (mut cockpit, fake) = cockpit("main-open-store");
        let repo = init_repo(&root);

        let thread = cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::Main {
                    checkout: repo.clone(),
                },
            )
            .unwrap();

        assert_eq!(
            cockpit.thread(thread).and_then(|open| open.workspace()),
            Some(&WorkspaceBinding::Main {
                checkout: repo.clone(),
            })
        );
        assert_eq!(
            fake.cwds.borrow().last().unwrap().as_deref(),
            Some(repo.as_path())
        );
    }

    /// AC: the binding is restored on relaunch — and the worktree comes back
    /// on demand even when someone deleted it by hand while Ferrite was off.
    #[test]
    fn a_relaunch_restores_the_binding_and_spawns_where_it_points() {
        let root = scratch("relaunch-binding");
        let dir = scratch("relaunch-binding-store");
        let repo = init_repo(&root);
        let fake = Fake::default();
        let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake.clone()));
        let thread = cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::NewWorktree { repo: repo.clone() },
            )
            .unwrap();
        let binding = cockpit.thread(thread).and_then(|open| open.workspace()).cloned().unwrap();
        cockpit.park(thread).unwrap();
        drop(cockpit);
        // While Ferrite is off, the worktree vanishes by hand.
        std::fs::remove_dir_all(binding.cwd()).unwrap();

        // A new run of Ferrite over the same store.
        let relaunched_fake = Fake::default();
        let mut relaunched = Cockpit::new(
            Store::open(&dir).unwrap(),
            Box::new(relaunched_fake.clone()),
        );
        relaunched.revive(thread).unwrap();

        assert_eq!(relaunched.thread(thread).and_then(|open| open.workspace()), Some(&binding));
        assert!(
            binding.cwd().join(".git").exists(),
            "the worktree must come back on demand"
        );
        assert_eq!(
            relaunched_fake.cwds.borrow().last().unwrap().as_deref(),
            Some(binding.cwd())
        );
    }

    /// Registered central worktrees outlive every individual Thread. Two
    /// Threads may share one adoption, so deleting either cannot inspect or
    /// remove the checkout beneath the other.
    #[test]
    fn deleting_a_thread_preserves_a_shared_registered_worktree() {
        let root = scratch("delete-worktree");
        let (mut cockpit, _fake) = cockpit("delete-worktree-store");
        let repo = init_repo(&root);
        let first = cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::NewWorktree { repo: repo.clone() },
            )
            .unwrap();
        let path = cockpit.thread(first).and_then(|open| open.workspace()).unwrap().cwd().to_path_buf();
        let second = cockpit
            .open(
                Provider::Codex,
                WorkspaceChoice::ExistingWorktree {
                    repo: repo.clone(),
                    path: path.clone(),
                },
            )
            .unwrap();
        std::fs::write(path.join("wip.txt"), "uncommitted\n").unwrap();
        cockpit.delete(first).unwrap();
        assert_eq!(
            std::fs::read_to_string(path.join("wip.txt")).unwrap(),
            "uncommitted\n",
            "the registered checkout survives"
        );
        assert!(
            cockpit.thread(second).and_then(|open| open.workspace()).is_some(),
            "the other Thread still uses the checkout"
        );
        assert_eq!(
            cockpit
                .registry()
                .worktrees(cockpit.registry().projects()[0].id)
                .len(),
            1
        );
    }

    /// Deleting a main-bound Thread deletes the Thread — the operator's
    /// checkout is not Ferrite's to touch.
    #[test]
    fn deleting_a_main_bound_thread_leaves_the_checkout_alone() {
        let root = scratch("delete-main");
        let (mut cockpit, _fake) = cockpit("delete-main-store");
        let repo = init_repo(&root);
        let thread = cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::Main {
                    checkout: repo.clone(),
                },
            )
            .unwrap();
        // The checkout is even dirty — irrelevant: it is not a worktree
        // Ferrite owns.
        std::fs::write(repo.join("operator.txt"), "the operator's own\n").unwrap();

        cockpit.delete(thread).unwrap();

        assert!(cockpit.threads().is_empty());
        assert_eq!(cockpit.parked().unwrap(), vec![]);
        assert_eq!(
            std::fs::read_to_string(repo.join("operator.txt")).unwrap(),
            "the operator's own\n"
        );
    }

    #[test]
    fn deleting_a_grouped_thread_removes_membership_before_the_log() {
        let root = scratch("delete-grouped");
        let repo = init_repo(&root);
        let (mut cockpit, _fake) = cockpit("delete-grouped-store");
        let one = cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::Main {
                    checkout: repo.clone(),
                },
            )
            .unwrap();
        let two = cockpit
            .open(Provider::Claude, WorkspaceChoice::Main { checkout: repo })
            .unwrap();
        let group = cockpit
            .apply_group(GroupChange::Create {
                first: one,
                second: two,
            })
            .unwrap()
            .group
            .unwrap();

        cockpit.delete(one).unwrap();

        assert!(cockpit.groups().get(group).is_none());
        assert_eq!(cockpit.threads(), vec![two]);
        assert_eq!(cockpit.store.thread_ids().unwrap(), vec![two]);
    }

    #[test]
    fn failed_delete_restores_the_exact_group_snapshot() {
        let store = Store::open(scratch("delete-group-rollback"))
            .unwrap()
            .refuse_delete();
        let mut cockpit = Cockpit::new(store, Box::new(Fake::default()));
        let threads: Vec<_> = (0..4)
            .map(|_| cockpit.open(Provider::Claude, main_choice()).unwrap())
            .collect();
        let first = cockpit
            .apply_group(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        let second = cockpit
            .apply_group(GroupChange::Create {
                first: threads[2],
                second: threads[3],
            })
            .unwrap()
            .group
            .unwrap();
        cockpit
            .apply_group(GroupChange::Rename {
                group: first,
                title: "exact title".into(),
            })
            .unwrap();
        cockpit
            .apply_group(GroupChange::MoveGroup {
                group: second,
                index: 0,
            })
            .unwrap();
        let before: Vec<_> = cockpit.groups().iter().cloned().collect();

        assert!(cockpit.delete(threads[0]).is_err());
        assert_eq!(cockpit.groups().iter().cloned().collect::<Vec<_>>(), before);
    }

    #[test]
    fn revive_group_is_ordered_and_rolls_back_when_a_later_member_fails() {
        let (mut cockpit, fake) = cockpit("revive-group-atomic");
        let first = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let second = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let third = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let group = cockpit
            .apply_group(GroupChange::Create { first, second })
            .unwrap()
            .group
            .unwrap();
        cockpit
            .apply_group(GroupChange::Join {
                thread: third,
                group,
                index: None,
            })
            .unwrap();
        cockpit.park(first).unwrap();
        cockpit.park(second).unwrap();
        cockpit.park(third).unwrap();
        cockpit.refuse_park.insert(first);
        *fake.fail_at.borrow_mut() = Some(*fake.attempts.borrow() + 3);

        assert!(matches!(
            cockpit.revive_group(group),
            Err(ReviveGroupError::Rollback { thread, .. }) if thread == first
        ));
        assert!(
            cockpit.threads().is_empty(),
            "the first revival rolled back"
        );
        assert_eq!(
            cockpit.groups().get(group).unwrap().members,
            [first, second, third]
        );

        cockpit.refuse_park.clear();
        *fake.fail_at.borrow_mut() = None;
        assert_eq!(cockpit.revive_group(group).unwrap(), [first, second, third]);
        assert_eq!(cockpit.threads(), [first, second, third]);
    }

    #[test]
    fn thread_title_is_durable_and_blank_rename_is_refused() {
        let dir = scratch("thread-title");
        let fake = Fake::default();
        let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake.clone()));
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();

        cockpit.rename_thread(thread, "  Parser work  ").unwrap();
        assert_eq!(
            cockpit.thread_title(thread).unwrap().as_deref(),
            Some("Parser work")
        );
        assert!(cockpit.rename_thread(thread, "   ").is_err());
        cockpit.park(thread).unwrap();
        assert_eq!(
            cockpit.thread_title(thread).unwrap().as_deref(),
            Some("Parser work"),
            "the one title seam reads parked Threads too"
        );
        drop(cockpit);

        let mut relaunched = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));
        relaunched.revive(thread).unwrap();
        assert_eq!(
            relaunched.thread_title(thread).unwrap().as_deref(),
            Some("Parser work")
        );
    }

    /// AC: two Threads on the same repo in separate worktrees cannot touch
    /// each other's tree — through the cockpit's own seam, in the cwds it
    /// hands the Sessions.
    #[test]
    fn two_threads_on_one_repo_work_in_isolated_worktrees() {
        let root = scratch("two-threads");
        let (mut cockpit, fake) = cockpit("two-threads-store");
        let repo = init_repo(&root);

        let one = cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::NewWorktree { repo: repo.clone() },
            )
            .unwrap();
        let two = cockpit
            .open(
                Provider::Codex,
                WorkspaceChoice::NewWorktree { repo: repo.clone() },
            )
            .unwrap();

        let cwds = fake.cwds.borrow();
        let cwd_one = cwds[0].clone().expect("thread one has a cwd");
        let cwd_two = cwds[1].clone().expect("thread two has a cwd");
        drop(cwds);
        assert_ne!(cwd_one, cwd_two, "two Threads must not share a tree");
        assert_ne!(one, two);

        // Each "agent" writes in its own workspace; neither sees the other.
        std::fs::write(cwd_one.join("only-one.txt"), "one\n").unwrap();
        std::fs::write(cwd_two.join("file.txt"), "two edited\n").unwrap();
        let status_one = crate::workspace::git_for_tests(&cwd_one, &["status", "--porcelain"]);
        let status_two = crate::workspace::git_for_tests(&cwd_two, &["status", "--porcelain"]);
        assert!(status_one.contains("only-one.txt"), "one: {status_one}");
        assert!(!status_one.contains("file.txt"), "one: {status_one}");
        assert!(status_two.contains("file.txt"), "two: {status_two}");
        assert!(!status_two.contains("only-one.txt"), "two: {status_two}");
        assert!(!cwd_two.join("only-one.txt").exists());
        assert_eq!(
            std::fs::read_to_string(cwd_one.join("file.txt")).unwrap(),
            "base\n"
        );
    }

    fn text(s: &str) -> SessionEvent {
        SessionEvent::TextDelta { text: s.into() }
    }

    #[test]
    fn each_thread_folds_only_its_own_events() {
        let (mut cockpit, fake) = cockpit("own-events");
        let one = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let two = cockpit.open(Provider::Claude, main_choice()).unwrap();

        fake.streams.borrow()[0].send(text("only for one")).unwrap();

        let updates = cockpit.pump();

        // Coalesced: a Thread that produced nothing is not in the frame.
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].thread, one);
        assert_eq!(updates[0].dirty.len(), 1);
        assert_eq!(cockpit.thread(one).unwrap().transcript().blocks().len(), 1);
        assert!(cockpit.thread(two).unwrap().transcript().blocks().is_empty());
    }

    /// #22: durations are stamped at ingestion — a call runs on a live
    /// clock until its completion fixes the total, and a call the cockpit
    /// never saw live has none. No sleeps: monotonic clocks never run
    /// backwards, so the invariants hold without waiting on a scheduler.
    #[test]
    fn tool_calls_are_clocked_at_ingestion() {
        let (mut cockpit, fake) = cockpit("timings");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();

        fake.streams.borrow()[0]
            .send(SessionEvent::ToolStarted {
                id: "t1".into(),
                name: "Bash".into(),
                input: serde_json::json!({ "command": "cargo check" }),
            })
            .unwrap();
        cockpit.pump();
        let running = cockpit.thread(thread).map(|open| open.tool_timings()).unwrap()["t1"];
        assert!(matches!(running, ToolTiming::Running(_)));
        let first = running.elapsed();
        assert!(
            running.elapsed() >= first,
            "a running call's clock never runs backwards"
        );

        fake.streams.borrow()[0]
            .send(SessionEvent::ToolCompleted {
                id: "t1".into(),
                output: String::new(),
                is_error: false,
                result: crate::ToolResult::Opaque,
            })
            .unwrap();
        cockpit.pump();
        let done = cockpit.thread(thread).map(|open| open.tool_timings()).unwrap()["t1"];
        let ToolTiming::Done(total) = done else {
            panic!("a settled call fixes its total: {done:?}");
        };
        assert!(
            total >= first,
            "the total covers at least what had already elapsed"
        );
        assert_eq!(done.elapsed(), done.elapsed(), "a settled call is fixed");

        // A completion the cockpit never saw start has no clock to read.
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolCompleted {
                id: "ghost".into(),
                output: String::new(),
                is_error: false,
                result: crate::ToolResult::Opaque,
            })
            .unwrap();
        cockpit.pump();
        assert!(!cockpit.thread(thread).map(|open| open.tool_timings()).unwrap().contains_key("ghost"));
    }

    fn decision(id: &str, tool: &str) -> SessionEvent {
        SessionEvent::DecisionRequested {
            decision: Decision {
                id: id.into(),
                tool_use_id: format!("toolu_{id}"),
                tool_name: tool.into(),
                description: "ferrite-perm.txt".into(),
                input: serde_json::json!({ "file_path": "ferrite-perm.txt" }),
                suggestions: vec![],
            },
        }
    }

    fn ended() -> SessionEvent {
        SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        }
    }

    /// Reports whatever the test tells it to.
    struct Meter(Rc<RefCell<u64>>);

    impl RssSampler for Meter {
        fn sample(&mut self, _thread: ThreadId, _pid: Option<u32>) -> Option<u64> {
            Some(*self.0.borrow())
        }
    }

    #[test]
    fn a_sent_prompt_reaches_the_provider_the_transcript_and_the_log() {
        let (mut cockpit, fake) = cockpit("send");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();

        cockpit.send(thread, "run the tests".into());

        assert_eq!(fake.sent.borrow().as_slice(), ["run the tests"]);
        let echoed = cockpit.thread(thread).unwrap().transcript().blocks().last().cloned();
        assert!(
            matches!(echoed.map(|b| b.body), Some(Body::Prompt(line)) if line == "run the tests")
        );
        // And durably: a restart must not lose what the operator asked for.
        cockpit.park(thread).unwrap();
        cockpit.revive(thread).unwrap();
        assert!(cockpit
            .thread(thread).map(|open| open.transcript())
            .unwrap()
            .blocks()
            .iter()
            .any(|block| matches!(&block.body, Body::Prompt(line) if line == "run the tests")));
    }

    #[test]
    fn prompt_history_is_per_thread_and_rebuilt_from_the_log() {
        let (mut cockpit, _) = cockpit("prompt-history-core");
        let first = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let second = cockpit.open(Provider::Claude, main_choice()).unwrap();
        cockpit.send(first, "one".into());
        cockpit.send(first, "two".into());
        cockpit.send(second, "other".into());

        assert_eq!(
            cockpit.recall_prompt(first, HistoryDirection::Older, "draft"),
            Some("two".into())
        );
        assert_eq!(
            cockpit.recall_prompt(second, HistoryDirection::Older, "second draft"),
            Some("other".into())
        );

        cockpit.park(first).unwrap();
        cockpit.revive(first).unwrap();
        assert_eq!(
            cockpit.recall_prompt(first, HistoryDirection::Older, "revived draft"),
            Some("two".into())
        );
        assert_eq!(
            cockpit.recall_prompt(first, HistoryDirection::Older, "ignored edit"),
            Some("one".into())
        );
    }

    #[test]
    fn only_successful_delivery_joins_history_and_queued_delivery_waits_for_turn_end() {
        let (mut cockpit, fake) = cockpit("prompt-history-delivery");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        cockpit.send(thread, "sent".into());
        *fake.fail_send.borrow_mut() = true;
        cockpit.send(thread, "refused".into());
        *fake.fail_send.borrow_mut() = false;
        assert_eq!(
            cockpit.recall_prompt(thread, HistoryDirection::Older, "draft"),
            Some("sent".into())
        );

        cockpit.queue(thread, "taken back".into());
        assert_eq!(cockpit.unqueue(thread).as_deref(), Some("taken back"));
        assert_eq!(
            cockpit.recall_prompt(thread, HistoryDirection::Older, "after unqueue"),
            Some("sent".into()),
            "unqueue resets traversal and never appends"
        );

        fake.streams.borrow()[0].send(text("working")).unwrap();
        cockpit.pump();
        cockpit.queue(thread, "held".into());
        assert_eq!(
            cockpit.recall_prompt(thread, HistoryDirection::Older, "while held"),
            Some("sent".into()),
            "queueing resets traversal but does not append"
        );
        fake.streams.borrow()[0].send(ended()).unwrap();
        cockpit.pump();
        assert_eq!(
            cockpit.recall_prompt(thread, HistoryDirection::Older, "after release"),
            Some("held".into()),
            "turn-end delivery appends through the single delivery chokepoint"
        );
    }

    #[test]
    fn a_new_cockpit_recovers_prompt_history_from_the_store() {
        let dir = scratch("prompt-history-relaunch");
        let fake = Fake::default();
        let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake.clone()));
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        cockpit.send(thread, "before relaunch".into());
        cockpit.park(thread).unwrap();
        drop(cockpit);

        let mut relaunched = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));
        relaunched.revive(thread).unwrap();
        assert_eq!(
            relaunched.recall_prompt(thread, HistoryDirection::Older, "draft"),
            Some("before relaunch".into())
        );
    }

    #[test]
    fn bootstrap_delivery_is_immediately_recallable() {
        let (mut cockpit, _) = cockpit("prompt-history-bootstrap");
        let thread = cockpit
            .bootstrap(
                ProviderChoice {
                    provider: Provider::Claude,
                    model: None,
                },
                main_choice(),
                "first prompt",
            )
            .unwrap();

        assert_eq!(
            cockpit.recall_prompt(thread, HistoryDirection::Older, "draft"),
            Some("first prompt".into())
        );
    }

    /// The exact wire preface for one binding + root pair (#24) — what the
    /// provider sees and nothing the operator ever does.
    fn preface(binding: &Path, root: &Path) -> String {
        format!(
            "<ferrite-session-context>\n\
             Agent workspace root: {}\n\
             Session project root: {}\n\
             Run edits, commands, tests, builds, and Git operations from the \
             session project root. Use the agent workspace root only for \
             broader workspace context.\n\
             </ferrite-session-context>\n",
            binding.display(),
            root.display()
        )
    }

    /// A directory that exists, for tests that pick it as the root.
    fn existing_root(name: &str) -> std::path::PathBuf {
        let root = scratch(name);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// AC (#24): the first prompt after a Session start goes to the provider
    /// wearing the hidden session-context block; the second carries nothing
    /// extra. The transcript and the log keep the operator's raw text only —
    /// displayed ≠ sent.
    #[test]
    fn the_first_prompt_of_a_session_carries_the_hidden_context_and_the_second_does_not() {
        let dir = scratch("preface-store");
        let fake = Fake::default();
        let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake.clone()));
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        assert_eq!(cockpit.thread(thread).and_then(|open| open.session_project_root()), None);
        let root = existing_root("preface-root");
        cockpit
            .set_session_project_root(thread, Some(root.clone()))
            .unwrap();
        assert_eq!(cockpit.thread(thread).and_then(|open| open.session_project_root()), Some(root.as_path()));

        cockpit.send(thread, "run the tests".into());
        cockpit.send(thread, "and the lints".into());

        let binding = std::env::temp_dir(); // main_choice()'s checkout
        assert_eq!(
            fake.sent.borrow().as_slice(),
            [
                format!("{}run the tests", preface(&binding, &root)),
                "and the lints".to_string(),
            ]
        );
        // Displayed ≠ sent: the Pane echoes the raw prompt...
        let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
        assert!(blocks
            .iter()
            .any(|b| matches!(&b.body, Body::Prompt(line) if line == "run the tests")));
        assert!(
            !format!("{blocks:?}").contains("ferrite-session-context"),
            "the preface must never reach the transcript"
        );
        // ...and the log stores it raw: the preface exists only on the wire.
        cockpit.park(thread).unwrap();
        let inputs = Store::open(&dir).unwrap().load(thread).unwrap().inputs();
        assert!(inputs.contains(&Input::Prompt("run the tests".into())));
        assert!(
            !format!("{inputs:?}").contains("ferrite-session-context"),
            "the preface must never reach the log"
        );
    }

    /// AC (#24): changing the selection on a live Thread ends its Session —
    /// no mid-session mutation of the root — and the next prompt respawns
    /// through the resume path with the preface re-armed, naming the new
    /// root. The respawn's cwd is the chain's head (#29): the picked root.
    #[test]
    fn changing_the_root_ends_the_session_and_the_next_prompt_respawns_with_the_preface() {
        let (mut cockpit, fake) = cockpit("root-change");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let first_root = existing_root("root-change-first");
        let second_root = existing_root("root-change-second");
        cockpit
            .set_session_project_root(thread, Some(first_root.clone()))
            .unwrap();
        cockpit.send(thread, "one".into());
        // The provider names the session; a later respawn must resume it.
        fake.streams
            .borrow()
            .last()
            .unwrap()
            .send(SessionEvent::Init {
                session_id: "sess-1".into(),
                model: "claude-haiku-4-5".into(),
            })
            .unwrap();
        cockpit.pump();
        cockpit.send(thread, "two".into());
        let spawns = fake.streams.borrow().len();

        cockpit
            .set_session_project_root(thread, Some(second_root.clone()))
            .unwrap();
        // The Session ended; nothing respawns until the operator speaks.
        assert_eq!(fake.streams.borrow().len(), spawns);
        cockpit.send(thread, "three".into());

        assert_eq!(fake.streams.borrow().len(), spawns + 1);
        assert_eq!(
            fake.resumed.borrow().last().unwrap().as_deref(),
            Some("sess-1"),
            "the respawn goes through the resume path"
        );
        let binding = std::env::temp_dir();
        assert_eq!(
            fake.cwds.borrow().last().unwrap().as_deref(),
            Some(second_root.as_path()),
            "the respawn's cwd is the chain's head (#29): the picked root"
        );
        assert_eq!(
            fake.sent.borrow().as_slice(),
            [
                format!("{}one", preface(&binding, &first_root)),
                "two".to_string(),
                format!("{}three", preface(&binding, &second_root)),
            ]
        );
    }

    /// AC (#24): the root survives restart, and a revive is a Session start
    /// like any other — its first prompt carries the preface again.
    #[test]
    fn a_revived_thread_keeps_its_root_and_prefaces_its_first_prompt_again() {
        let (mut cockpit, fake) = cockpit("root-revive");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let root = existing_root("root-revive-root");
        cockpit
            .set_session_project_root(thread, Some(root.clone()))
            .unwrap();
        cockpit.send(thread, "before the park".into());

        cockpit.park(thread).unwrap();
        cockpit.revive(thread).unwrap();

        assert_eq!(cockpit.thread(thread).and_then(|open| open.session_project_root()), Some(root.as_path()));
        cockpit.send(thread, "after the revive".into());
        let binding = std::env::temp_dir();
        assert_eq!(
            fake.sent.borrow().last().unwrap(),
            &format!("{}after the revive", preface(&binding, &root))
        );
    }

    /// AC (#24): a watchdog respawn is a Session start too — the
    /// replacement's first prompt carries the preface again.
    #[test]
    fn a_sweep_respawned_session_prefaces_its_next_prompt() {
        let (mut cockpit, fake) = cockpit("root-sweep");
        let rss = Rc::new(RefCell::new(4 * 1024 * 1024 * 1024));
        cockpit.watch_memory(Box::new(Meter(rss)), 1024 * 1024 * 1024);
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let root = existing_root("root-sweep-root");
        cockpit
            .set_session_project_root(thread, Some(root.clone()))
            .unwrap();
        cockpit.send(thread, "one".into()); // consumes the first preface

        assert_eq!(cockpit.sweep().len(), 1, "the leak replaces the Session");
        cockpit.send(thread, "two".into());

        let binding = std::env::temp_dir();
        assert_eq!(
            fake.sent.borrow().last().unwrap(),
            &format!("{}two", preface(&binding, &root))
        );
    }

    /// AC (#24): a root gone from disk refuses the send, naming the path and
    /// the remedy — never a silent fallback to working in the binding.
    /// Nothing reaches the wire, the transcript's history, or the log.
    #[test]
    fn a_send_with_a_vanished_root_is_refused_naming_the_path_and_the_remedy() {
        let (mut cockpit, fake) = cockpit("root-vanished");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let root = existing_root("root-vanished-root");
        cockpit
            .set_session_project_root(thread, Some(root.clone()))
            .unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        cockpit.send(thread, "into the void".into());

        assert!(
            fake.sent.borrow().is_empty(),
            "nothing may reach the provider"
        );
        let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
        let last = blocks.last().unwrap();
        let Body::Notice(line) = &last.body else {
            panic!("the refusal must be a readable notice: {:?}", last.body);
        };
        assert!(
            line.contains(&root.display().to_string()),
            "the path is named: {line}"
        );
        assert!(line.contains("re-pick"), "the remedy is named: {line}");
        assert!(
            !blocks.iter().any(|b| matches!(&b.body, Body::Prompt(_))),
            "a refused prompt is not history"
        );
    }

    /// A prompt queued behind a running turn must not strand when the
    /// operator changes the root: no turn will ever end to release it, the
    /// Session being gone. It goes out at once as the new Session's first
    /// prompt, wearing the new preface — what the operator picked it for.
    #[test]
    fn changing_the_root_releases_a_queued_prompt_into_the_new_session() {
        let (mut cockpit, fake) = cockpit("root-queued");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        fake.streams.borrow()[0].send(text("working")).unwrap();
        cockpit.pump();
        assert!(cockpit.thread(thread).is_some_and(|open| open.busy()));
        cockpit.queue(thread, "and then the tests".into());
        let root = existing_root("root-queued-root");

        cockpit
            .set_session_project_root(thread, Some(root.clone()))
            .unwrap();

        assert_eq!(cockpit.thread(thread).and_then(|open| open.queued()), None, "the held prompt went out");
        assert_eq!(fake.streams.borrow().len(), 2, "a fresh Session took it");
        let binding = std::env::temp_dir();
        assert_eq!(
            fake.sent.borrow().as_slice(),
            [format!("{}and then the tests", preface(&binding, &root))]
        );
        // Raw in the Pane: displayed ≠ sent.
        let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
        assert!(blocks
            .iter()
            .any(|b| matches!(&b.body, Body::Prompt(line) if line == "and then the tests")));
    }

    /// A held prompt released by a turn's end is a prompt like any other:
    /// on a fresh Session (the watchdog replaced it mid-wait) its wire text
    /// carries the preface — once — and the Pane keeps the raw text.
    #[test]
    fn a_queued_prompt_released_on_a_fresh_session_carries_the_preface_once() {
        let (mut cockpit, fake) = cockpit("root-queued-release");
        let rss = Rc::new(RefCell::new(0u64));
        cockpit.watch_memory(Box::new(Meter(rss.clone())), 1024);
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let root = existing_root("root-queued-release-root");
        cockpit
            .set_session_project_root(thread, Some(root.clone()))
            .unwrap();
        cockpit.send(thread, "one".into()); // consumes the first preface
        fake.streams.borrow()[1].send(text("working")).unwrap();
        cockpit.pump();
        cockpit.queue(thread, "held".into());
        *rss.borrow_mut() = 4096; // over the limit: the watchdog acts
        assert_eq!(cockpit.sweep().len(), 1, "a fresh Session, preface armed");
        fake.streams.borrow()[2].send(ended()).unwrap();

        cockpit.pump(); // the turn ends; the held prompt is released

        let binding = std::env::temp_dir();
        assert_eq!(
            fake.sent.borrow().last().unwrap(),
            &format!("{}held", preface(&binding, &root))
        );
        cockpit.send(thread, "next".into());
        assert_eq!(
            fake.sent.borrow().last().unwrap(),
            "next",
            "the preface rides once per Session"
        );
        let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
        assert!(
            !format!("{blocks:?}").contains("ferrite-session-context"),
            "the preface must never reach the transcript"
        );
    }

    /// After a failed watchdog restart the Thread sits in the cockpit with
    /// no Session. The next prompt makes exactly one spawn attempt; a
    /// failure is one readable Notice on the send-failure surface — not a
    /// loop, and not a prompt in the history that never went anywhere.
    #[test]
    fn a_send_after_a_failed_restart_attempts_one_spawn_and_reports_the_failure() {
        let (mut cockpit, fake) = cockpit("respawn-fails");
        let rss = Rc::new(RefCell::new(u64::MAX));
        cockpit.watch_memory(Box::new(Meter(rss)), 1024);
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        *fake.fail.borrow_mut() = true;
        assert_eq!(cockpit.sweep().len(), 1, "the restart fails; no Session");
        let attempts = *fake.attempts.borrow();

        cockpit.send(thread, "hello".into());

        assert_eq!(
            *fake.attempts.borrow(),
            attempts + 1,
            "exactly one spawn attempt per send"
        );
        assert!(fake.sent.borrow().is_empty());
        let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
        let Body::Notice(line) = &blocks.last().unwrap().body else {
            panic!("the failure must be a Notice: {:?}", blocks.last());
        };
        assert!(line.starts_with("send failed:"), "{line}");
        assert!(
            !blocks.iter().any(|b| matches!(&b.body, Body::Prompt(_))),
            "an unsent prompt is not history"
        );
    }

    #[test]
    fn answering_through_the_cockpit_clears_the_card_and_records_who_did_it() {
        let (mut cockpit, fake) = cockpit("respond");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        fake.streams.borrow()[0]
            .send(decision("perm_01", "Write"))
            .unwrap();
        cockpit.pump();
        let pending = cockpit.thread(thread).and_then(|open| open.pending()).cloned().unwrap();

        cockpit.respond(
            thread,
            &pending,
            crate::DecisionAnswer::Allow {
                input: pending.input.clone(),
            },
        );

        assert_eq!(cockpit.thread(thread).and_then(|open| open.pending()), None);
        let last = cockpit.thread(thread).unwrap().transcript().blocks().last().cloned();
        assert!(matches!(last.map(|b| b.body), Some(Body::Meta(line)) if line == "allowed Write"));

        // A second press answers nothing twice.
        cockpit.respond(
            thread,
            &pending,
            crate::DecisionAnswer::Allow {
                input: pending.input.clone(),
            },
        );
        let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
        assert_eq!(
            blocks
                .iter()
                .filter(|b| matches!(&b.body, Body::Meta(line) if line == "allowed Write"))
                .count(),
            1
        );
    }

    #[test]
    fn a_code_fence_comes_back_highlighted_without_the_app_routing_anything() {
        let (mut cockpit, fake) = cockpit("highlight");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();

        fake.streams.borrow()[0]
            .send(text("```rust\nfn main() {}\n```\n\n"))
            .unwrap();
        cockpit.pump();
        // The lexer answers on its own channel; the next frame picks it up.
        let update = cockpit.pump();

        let code = cockpit
            .thread(thread).map(|open| open.transcript())
            .unwrap()
            .blocks()
            .iter()
            .find(|block| matches!(block.body, Body::Code { .. }))
            .cloned()
            .expect("a code block");
        assert!(update.iter().any(|pane| pane.dirty.contains(&code.id)));
        let Body::Code { tokens, .. } = &code.body else {
            unreachable!()
        };
        assert!(
            tokens
                .as_deref()
                .is_some_and(|tokens| tokens.iter().any(|t| t.class == Class::Keyword)),
            "the cockpit owns highlighting: {tokens:?}"
        );
    }

    #[test]
    fn jumping_to_the_next_decision_wraps_around_the_cockpit() {
        let (mut cockpit, fake) = cockpit("next-decision");
        let one = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let two = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let three = cockpit.open(Provider::Claude, main_choice()).unwrap();
        assert_eq!(cockpit.next_blocked(None), None);

        fake.streams.borrow()[0]
            .send(decision("perm_01", "Write"))
            .unwrap();
        fake.streams.borrow()[2]
            .send(decision("perm_03", "Bash"))
            .unwrap();
        cockpit.pump();

        // From nowhere, the first blocked Thread; from one, the next one on.
        assert_eq!(cockpit.next_blocked(None), Some(one));
        assert_eq!(cockpit.next_blocked(Some(one)), Some(three));
        // Past the last, back to the first — the operator keeps pressing one
        // key until the cockpit is quiet.
        assert_eq!(cockpit.next_blocked(Some(three)), Some(one));
        // A Thread with nothing pending is still a valid place to jump from.
        assert_eq!(cockpit.next_blocked(Some(two)), Some(three));
    }

    #[test]
    fn a_leaking_session_is_restarted_where_the_operator_can_see_it() {
        let (mut cockpit, fake) = cockpit("watchdog");
        let rss = Rc::new(RefCell::new(200 * 1024 * 1024));
        cockpit.watch_memory(Box::new(Meter(rss.clone())), 1024 * 1024 * 1024);
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::Init {
                session_id: "sess-1".into(),
                model: "claude-haiku-4-5".into(),
            })
            .unwrap();
        fake.streams.borrow()[0].send(text("work so far")).unwrap();
        cockpit.pump();

        // Comfortably under: nothing happens.
        assert!(cockpit.sweep().is_empty());

        *rss.borrow_mut() = 4 * 1024 * 1024 * 1024;
        let restarted = cockpit.sweep();

        assert_eq!(restarted.len(), 1);
        assert_eq!(restarted[0].thread, thread);
        assert_eq!(restarted[0].rss, 4 * 1024 * 1024 * 1024);
        // A second Session, told where to pick up — and where to work: the
        // replacement spawns into the same binding the first Session had.
        assert_eq!(fake.resumed.borrow().len(), 2);
        assert_eq!(
            fake.resumed.borrow().last().unwrap().as_deref(),
            Some("sess-1")
        );
        let cwds = fake.cwds.borrow();
        assert_eq!(cwds.last().unwrap(), &cwds[0]);

        let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
        let last = blocks.last().unwrap();
        assert!(
            matches!(&last.body, Body::Notice(line) if line.contains("restarted")),
            "the Pane must say it happened: {:?}",
            last.body
        );
        // The scrollback is not collateral damage.
        assert!(blocks.iter().any(|block| matches!(
            &block.body,
            Body::Paragraph { spans } if spans.iter().any(|s| s.text == "work so far")
        )));
    }

    #[test]
    fn a_restart_finds_the_threads_it_left_behind() {
        let dir = scratch("restart");
        let fake = Fake::default();
        let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake.clone()));
        let first = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let second = cockpit.open(Provider::Claude, main_choice()).unwrap();
        cockpit.park(first).unwrap();
        cockpit.park(second).unwrap();
        drop(cockpit);

        // A new run of Ferrite over the same store.
        let mut restarted = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));

        assert_eq!(restarted.parked().unwrap(), vec![first, second]);
        restarted.revive(second).unwrap();
        assert_eq!(restarted.threads(), vec![second]);
        // What is open is no longer parked.
        assert_eq!(restarted.parked().unwrap(), vec![first]);
    }

    #[test]
    fn a_parked_thread_revives_with_its_history_and_says_so() {
        let (mut cockpit, fake) = cockpit("revive");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::Init {
                session_id: "sess-1".into(),
                model: "claude-haiku-4-5".into(),
            })
            .unwrap();
        fake.streams.borrow()[0]
            .send(text("an answer from before"))
            .unwrap();
        fake.streams.borrow()[0].send(ended()).unwrap();
        cockpit.pump();

        cockpit.park(thread).unwrap();
        // A parked Thread costs memory nothing: no Session, no transcript.
        assert!(cockpit.thread(thread).is_none());
        assert!(cockpit.threads().is_empty());

        cockpit.revive(thread).unwrap();

        let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
        let text_of = |body: &Body| match body {
            Body::Paragraph { spans } => spans.iter().map(|s| s.text.clone()).collect::<String>(),
            Body::Meta(line) => line.clone(),
            _ => String::new(),
        };
        assert!(
            blocks
                .iter()
                .any(|b| text_of(&b.body) == "an answer from before"),
            "history should come back: {blocks:?}"
        );
        // Honest about what it is: a new Session wearing an old Thread.
        assert_eq!(
            text_of(&blocks.last().unwrap().body),
            "revived — new Session, history from the log"
        );
        // And the provider is told where to pick up.
        assert_eq!(
            fake.resumed.borrow().last().unwrap().as_deref(),
            Some("sess-1")
        );
    }

    /// #11: the running cockpit's own import door. The session file is
    /// adopted into this cockpit's store — durable and parked when the call
    /// returns — and `revive` opens it like any parked Thread, history
    /// replayed and the new Session told where to resume.
    #[test]
    fn a_session_file_imports_through_the_cockpits_own_door() {
        let (mut cockpit, fake) = cockpit("import-door");
        let dir = scratch("import-door-file");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("adopted.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"type":"user","sessionId":"door-9c1d","cwd":"/workspace","message":{"role":"user","content":"first question"}}"#,
                "\n",
                r#"{"type":"assistant","sessionId":"door-9c1d","message":{"model":"claude-haiku-4-5","content":[{"type":"text","text":"first answer"}]}}"#,
                "\n",
            ),
        )
        .unwrap();

        let thread = cockpit.import(&path).unwrap();

        assert!(
            cockpit.parked().unwrap().contains(&thread),
            "the imported Thread is durable and parked, ready to revive"
        );
        cockpit.revive(thread).unwrap();
        assert_eq!(
            cockpit.recall_prompt(thread, HistoryDirection::Older, "draft"),
            Some("first question".into()),
            "imported prompt records participate in recall"
        );
        let transcript = cockpit.thread(thread).unwrap().transcript();
        assert_eq!(transcript.session_id(), Some("door-9c1d"));
        assert!(
            transcript
                .blocks()
                .iter()
                .any(|block| matches!(&block.body, Body::Prompt(text) if text == "first question")),
            "the conversation replays: {:?}",
            transcript.blocks()
        );
        // The whole point of adoption: the new Session resumes the file's
        // own session id.
        assert_eq!(
            fake.resumed.borrow().last().unwrap().as_deref(),
            Some("door-9c1d")
        );
    }

    /// A file that is not a session file is refused by the import module —
    /// through this door, with the same readable error and no Thread.
    #[test]
    fn the_import_door_passes_a_refusal_through_unchanged() {
        let (mut cockpit, _fake) = cockpit("import-door-refused");
        let dir = scratch("import-door-refused-file");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("junk.jsonl");
        std::fs::write(&path, "not a session file\n").unwrap();

        let refused = cockpit.import(&path);
        assert!(
            matches!(
                refused,
                Err(crate::import::ImportError::Unrecognized { .. })
            ),
            "got {refused:?}"
        );
        assert!(cockpit.parked().unwrap().is_empty(), "no Thread was left");
    }

    #[test]
    fn a_decision_is_pending_against_the_thread_that_raised_it() {
        let (mut cockpit, fake) = cockpit("pending");
        let one = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let two = cockpit.open(Provider::Claude, main_choice()).unwrap();

        fake.streams.borrow()[0]
            .send(decision("perm_01", "Write"))
            .unwrap();
        cockpit.pump();

        assert_eq!(cockpit.thread(one).and_then(|open| open.pending()).unwrap().tool_name, "Write");
        assert_eq!(cockpit.thread(two).and_then(|open| open.pending()), None);
        assert_eq!(cockpit.blocked(), vec![one]);
    }

    #[test]
    fn answering_a_decision_that_is_no_longer_pending_is_refused_not_forwarded() {
        let (mut cockpit, fake) = cockpit("stale");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        fake.streams.borrow()[0]
            .send(decision("perm_01", "Write"))
            .unwrap();
        cockpit.pump();

        assert!(cockpit.answer(thread, "perm_01"));
        assert_eq!(cockpit.thread(thread).and_then(|open| open.pending()), None);

        // A second keystroke on a card already answered, and an answer to a
        // request this Thread never had: neither may reach the provider.
        assert!(!cockpit.answer(thread, "perm_01"));
        assert!(!cockpit.answer(thread, "perm_99"));
    }

    #[test]
    fn a_turn_that_ends_takes_its_unanswered_decision_with_it() {
        let (mut cockpit, fake) = cockpit("moot");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        fake.streams.borrow()[0]
            .send(decision("perm_01", "Write"))
            .unwrap();
        cockpit.pump();

        // The turn was interrupted, or the provider gave up waiting: the
        // request is gone, so the card must not linger over the Composer.
        fake.streams.borrow()[0].send(ended()).unwrap();
        cockpit.pump();

        assert_eq!(cockpit.thread(thread).and_then(|open| open.pending()), None);
        assert!(cockpit.blocked().is_empty());
        assert!(!cockpit.answer(thread, "perm_01"));
    }

    #[test]
    fn a_prompt_typed_during_a_turn_is_sent_when_the_turn_ends() {
        let (mut cockpit, fake) = cockpit("queued");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        fake.streams.borrow()[0].send(text("working")).unwrap();
        cockpit.pump();
        assert!(cockpit.thread(thread).is_some_and(|open| open.busy()));

        cockpit.queue(thread, "and then run the tests".into());
        assert_eq!(cockpit.thread(thread).and_then(|open| open.queued()), Some("and then run the tests"));
        assert!(fake.sent.borrow().is_empty(), "nothing goes out mid-turn");

        fake.streams.borrow()[0].send(ended()).unwrap();
        cockpit.pump();

        assert_eq!(fake.sent.borrow().as_slice(), ["and then run the tests"]);
        assert_eq!(cockpit.thread(thread).and_then(|open| open.queued()), None);
        assert!(!cockpit.thread(thread).is_some_and(|open| open.busy()));
    }

    #[test]
    fn a_held_prompt_can_be_taken_back_for_editing() {
        let (mut cockpit, _fake) = cockpit("unqueue");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        cockpit.queue(thread, "run the tets".into());

        let back = cockpit.unqueue(thread);

        assert_eq!(back.as_deref(), Some("run the tets"));
        assert_eq!(cockpit.thread(thread).and_then(|open| open.queued()), None);
        assert_eq!(cockpit.unqueue(thread), None);
    }

    #[test]
    fn a_turn_ending_with_nothing_held_sends_nothing() {
        let (mut cockpit, fake) = cockpit("nothing-held");
        cockpit.open(Provider::Claude, main_choice()).unwrap();

        fake.streams.borrow()[0].send(ended()).unwrap();
        cockpit.pump();

        assert!(fake.sent.borrow().is_empty());
    }

    /// #23: the Session's announced command menu becomes the Thread's — the
    /// `/` popover's source — without touching the transcript or the log; a
    /// revived Thread starts with none until its own Session speaks.
    #[test]
    fn a_commands_event_becomes_the_thread_menu_and_never_the_history() {
        let (mut cockpit, fake) = cockpit("commands");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        assert!(cockpit.thread(thread).map(|open| open.commands()).unwrap_or_default().is_empty(), "nothing is static");

        let menu = vec![crate::SessionCommand {
            name: "compact".into(),
            description: "summarize context".into(),
            path: None,
        }];
        fake.streams.borrow()[0]
            .send(SessionEvent::Commands {
                commands: menu.clone(),
            })
            .unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::PermissionMode {
                mode: "acceptEdits".into(),
            })
            .unwrap();
        cockpit.pump();

        assert_eq!(cockpit.thread(thread).map(|open| open.commands()).unwrap_or_default(), menu.as_slice());
        assert_eq!(cockpit.thread(thread).and_then(|open| open.permission_mode()), Some("acceptEdits"));
        assert!(
            cockpit.thread(thread).unwrap().transcript().blocks().is_empty(),
            "a menu is not conversation"
        );

        // Session state only: the log replays nothing, so a revived Thread
        // waits for its own Session's announcement.
        cockpit.park(thread).unwrap();
        cockpit.revive(thread).unwrap();
        assert!(cockpit.thread(thread).map(|open| open.commands()).unwrap_or_default().is_empty());
        assert_eq!(cockpit.thread(thread).and_then(|open| open.permission_mode()), None);
    }

    /// #25: the Session's announced model list becomes the Thread's — the
    /// provider picker's rows — on the same lane as the command menu:
    /// never the history, never the log, gone with the Session.
    #[test]
    fn a_models_event_becomes_the_thread_menu_and_never_the_history() {
        let (mut cockpit, fake) = cockpit("models");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        assert!(cockpit.thread(thread).map(|open| open.models()).unwrap_or_default().is_empty(), "nothing is static");

        fake.streams.borrow()[0]
            .send(SessionEvent::Models {
                models: vec!["sonnet".into(), "opus".into(), "haiku".into()],
            })
            .unwrap();
        cockpit.pump();

        assert_eq!(cockpit.thread(thread).map(|open| open.models()).unwrap_or_default(), ["sonnet", "opus", "haiku"]);
        assert!(
            cockpit.thread(thread).unwrap().transcript().blocks().is_empty(),
            "a model list is not conversation"
        );

        cockpit.park(thread).unwrap();
        cockpit.revive(thread).unwrap();
        assert!(cockpit.thread(thread).map(|open| open.models()).unwrap_or_default().is_empty());
    }

    /// AC (#25): choosing a provider before the first prompt replaces the
    /// Session on the spot — eager, so the new provider's menus arrive
    /// while the operator is still choosing — on a fresh Transcript, with
    /// the choice durable in the header. The first send goes to that live
    /// Session; nothing respawns for it.
    #[test]
    fn choosing_a_provider_pre_lock_respawns_eagerly_and_the_first_send_uses_it() {
        let (mut cockpit, fake) = cockpit("provider-pick");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        // The old Session had already spoken; none of it may linger.
        fake.streams.borrow()[0]
            .send(SessionEvent::Init {
                session_id: "sess-old".into(),
                model: "claude-haiku-4-5".into(),
            })
            .unwrap();
        fake.streams.borrow()[0].send(text("old provider")).unwrap();
        cockpit.pump();

        cockpit
            .set_provider(
                thread,
                ProviderChoice {
                    provider: Provider::Codex,
                    model: Some("gpt-5.4-mini".into()),
                },
            )
            .unwrap();

        assert_eq!(fake.streams.borrow().len(), 2, "the respawn is eager");
        assert_eq!(
            fake.spawn_pairs().last().unwrap(),
            &ProviderChoice {
                provider: Provider::Codex,
                model: Some("gpt-5.4-mini".into()),
            }
        );
        assert_eq!(
            fake.resumed.borrow().last().unwrap(),
            &None,
            "the old provider's id means nothing to the new one"
        );
        assert_eq!(cockpit.thread(thread).map(|open| open.provider()), Some(Provider::Codex));
        assert_eq!(cockpit.thread(thread).and_then(|open| open.model()), Some("gpt-5.4-mini"));
        assert!(
            cockpit.thread(thread).unwrap().transcript().blocks().is_empty(),
            "the old Init and prose must not linger"
        );
        // Durable: a crash right now still revives onto the choice.
        let meta = cockpit.peek(thread).unwrap();
        assert_eq!(meta.provider, Provider::Codex);
        assert_eq!(meta.model, Some("gpt-5.4-mini".into()));

        cockpit.send(thread, "first prompt".into());
        assert_eq!(fake.streams.borrow().len(), 2, "no respawn for the send");
        assert_eq!(fake.sent.borrow().as_slice(), ["first prompt"]);
    }

    /// AC (#25): the first prompt locks the Thread. `send` arms the one
    /// predicate, and the setter refuses whole — no spawn attempt, no
    /// header rewrite.
    #[test]
    fn the_first_prompt_locks_the_provider_for_good() {
        let (mut cockpit, fake) = cockpit("provider-lock");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        assert!(!cockpit.thread(thread).is_some_and(|open| open.first_prompt_sent()));

        cockpit.send(thread, "hello".into());
        assert!(cockpit.thread(thread).is_some_and(|open| open.first_prompt_sent()));
        let attempts = *fake.attempts.borrow();

        let refused = cockpit.set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Codex,
                model: None,
            },
        );
        assert!(
            matches!(refused, Err(ProvisionError::Locked)),
            "{refused:?}"
        );
        assert_eq!(*fake.attempts.borrow(), attempts, "no spawn was tried");
        assert_eq!(cockpit.thread(thread).map(|open| open.provider()), Some(Provider::Claude));
        assert_eq!(cockpit.peek(thread).unwrap().provider, Provider::Claude);
    }

    /// The lock arms from replayed history too: a revived Thread whose log
    /// holds an operator prompt is as locked as the live one was — and so
    /// is a parked Thread, judged straight off its log.
    #[test]
    fn a_thread_with_a_prompt_in_its_history_is_locked_on_revive_and_parked() {
        let (mut cockpit, _fake) = cockpit("provider-lock-history");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        cockpit.send(thread, "hello".into());
        cockpit.park(thread).unwrap();

        // Parked: the store-only path reads the log for the lock.
        assert!(matches!(
            cockpit.set_provider(
                thread,
                ProviderChoice {
                    provider: Provider::Codex,
                    model: None,
                },
            ),
            Err(ProvisionError::Locked)
        ));

        cockpit.revive(thread).unwrap();
        assert!(cockpit.thread(thread).is_some_and(|open| open.first_prompt_sent()), "history armed the lock");
        assert!(matches!(
            cockpit.set_provider(
                thread,
                ProviderChoice {
                    provider: Provider::Codex,
                    model: None,
                },
            ),
            Err(ProvisionError::Locked)
        ));
    }

    /// AC (#25): a parked-never-prompted Thread revives onto its chosen
    /// provider and model — the durable header is what every later spawn
    /// reads.
    #[test]
    fn a_park_then_revive_keeps_the_chosen_provider_and_model() {
        let (mut cockpit, fake) = cockpit("provider-revive");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        cockpit
            .set_provider(
                thread,
                ProviderChoice {
                    provider: Provider::Codex,
                    model: Some("gpt-5.4-mini".into()),
                },
            )
            .unwrap();

        cockpit.park(thread).unwrap();
        cockpit.revive(thread).unwrap();

        assert_eq!(
            fake.spawn_pairs().last().unwrap(),
            &ProviderChoice {
                provider: Provider::Codex,
                model: Some("gpt-5.4-mini".into()),
            }
        );
        assert_eq!(cockpit.thread(thread).map(|open| open.provider()), Some(Provider::Codex));
        assert_eq!(cockpit.thread(thread).and_then(|open| open.model()), Some("gpt-5.4-mini"));
        assert!(!cockpit.thread(thread).is_some_and(|open| open.first_prompt_sent()), "still unlocked");
    }

    /// AC (#25): a CLI that fails to spawn refuses the whole switch — the
    /// old Session keeps serving, the header is untouched, and the error
    /// carries the provider's words.
    #[test]
    fn a_failed_provider_spawn_leaves_the_old_session_serving() {
        let (mut cockpit, fake) = cockpit("provider-spawn-fails");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        *fake.fail.borrow_mut() = true;

        let refused = cockpit.set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Codex,
                model: None,
            },
        );

        let Err(ProvisionError::Spawn(e)) = refused else {
            panic!("expected the spawn refusal: {refused:?}");
        };
        assert!(e.to_string().contains("stub refused to spawn"));
        assert_eq!(cockpit.thread(thread).map(|open| open.provider()), Some(Provider::Claude));
        assert_eq!(cockpit.peek(thread).unwrap().provider, Provider::Claude);
        // The old Session still serves: the next send spawns nothing new.
        *fake.fail.borrow_mut() = false;
        cockpit.send(thread, "still here".into());
        assert_eq!(fake.sent.borrow().as_slice(), ["still here"]);
        assert_eq!(fake.streams.borrow().len(), 1);
    }

    /// Re-picking what the Thread already is changes nothing: no teardown,
    /// no respawn, no rewrite — the common path must stay friction-free.
    #[test]
    fn re_picking_the_current_provider_and_model_is_a_no_op() {
        let (mut cockpit, fake) = cockpit("provider-noop");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        fake.streams.borrow()[0]
            .send(text("still serving"))
            .unwrap();
        cockpit.pump();

        cockpit
            .set_provider(
                thread,
                ProviderChoice {
                    provider: Provider::Claude,
                    model: None,
                },
            )
            .unwrap();

        assert_eq!(fake.streams.borrow().len(), 1, "nothing respawned");
        assert!(
            !cockpit.thread(thread).unwrap().transcript().blocks().is_empty(),
            "and nothing was torn down"
        );
    }

    // ------------------------------------------- what is on screen (#28)

    /// `n` open Threads on the main checkout, in open order.
    fn opened(cockpit: &mut Cockpit, n: usize) -> Vec<ThreadId> {
        (0..n)
            .map(|_| cockpit.open(Provider::Claude, main_choice()).unwrap())
            .collect()
    }

    fn pair(cockpit: &mut Cockpit, first: ThreadId, second: ThreadId) -> GroupId {
        cockpit
            .apply_group(GroupChange::Create { first, second })
            .unwrap()
            .group
            .unwrap()
    }

    #[test]
    fn open_threads_are_the_roster_and_solo_shows_the_focused_one() {
        let (mut cockpit, _) = cockpit("roster-solo");
        let threads = opened(&mut cockpit, 3);
        let panes: Vec<ThreadId> = cockpit
            .roster()
            .panes()
            .iter()
            .filter_map(|pane| pane.thread())
            .collect();
        assert_eq!(panes, threads, "every open Thread has a Pane, in open order");
        assert_eq!(cockpit.visible(), [PaneIdentity::Thread(threads[0])]);
        assert!(cockpit.focus(PaneIdentity::Thread(threads[2])));
        assert_eq!(cockpit.visible(), [PaneIdentity::Thread(threads[2])]);
        assert_eq!(cockpit.layout().columns, 1);
        cockpit.park(threads[2]).unwrap();
        assert_eq!(cockpit.roster().panes().len(), 2, "a parked Thread has no Pane");
        assert_eq!(
            cockpit.roster().focused_thread(),
            Some(threads[1]),
            "focus clamps onto a survivor"
        );
    }

    #[test]
    fn closing_in_solo_parks_and_closing_in_a_group_leaves_onto_the_ordinal_survivor() {
        let (mut cockpit, _) = cockpit("roster-close");
        let threads = opened(&mut cockpit, 3);
        let group = pair(&mut cockpit, threads[0], threads[1]);
        cockpit
            .apply_group(GroupChange::Join {
                thread: threads[2],
                group,
                index: None,
            })
            .unwrap();
        cockpit.enter_group(group).unwrap();
        assert_eq!(cockpit.layout().columns, 2);
        cockpit.focus(PaneIdentity::Thread(threads[1]));

        cockpit.close(PaneIdentity::Thread(threads[1])).unwrap();
        assert_eq!(cockpit.threads().len(), 3, "leaving never parks");
        assert_eq!(cockpit.roster().view(), View::Group(group));
        assert_eq!(
            cockpit.roster().focused_thread(),
            Some(threads[2]),
            "the Thread that took the closed one's ordinal"
        );

        cockpit.close(PaneIdentity::Thread(threads[2])).unwrap();
        assert_eq!(cockpit.roster().view(), View::Solo, "a pair losing a member dissolves");
        assert_eq!(cockpit.roster().focused_thread(), Some(threads[0]), "onto the survivor");
        assert!(cockpit.groups().get(group).is_none());

        cockpit.close(PaneIdentity::Thread(threads[0])).unwrap();
        assert!(cockpit.parked().unwrap().contains(&threads[0]), "a Solo close parks");
        assert_eq!(cockpit.roster().park_order(), [threads[0]]);
        assert_eq!(cockpit.roster().panes().len(), 2);
    }

    #[test]
    fn a_pending_draft_defers_a_pairs_leave_until_it_sends_or_is_discarded() {
        let (mut cockpit, _) = cockpit("roster-defer");
        let threads = opened(&mut cockpit, 4);
        let sending = pair(&mut cockpit, threads[0], threads[1]);
        cockpit.enter_group(sending).unwrap();
        let draft = cockpit.open_draft();
        assert_eq!(cockpit.roster().draft_scope(draft).unwrap().group, Some(sending));
        assert_eq!(cockpit.visible().len(), 3, "the pending draft shows");

        cockpit.close(PaneIdentity::Thread(threads[0])).unwrap();
        assert_eq!(
            cockpit.groups().get(sending).unwrap().members,
            threads[..2],
            "the pair stands while the draft is pending"
        );
        assert_eq!(cockpit.roster().pending_leave(sending), Some(threads[0]));
        assert_eq!(
            cockpit.visible(),
            [PaneIdentity::Thread(threads[1]), PaneIdentity::Draft(draft)],
            "the leaving member is already gone from view"
        );
        // The operator writes in the draft, so it holds focus when it sends.
        cockpit.focus(PaneIdentity::Draft(draft));
        let done = cockpit
            .bootstrap_draft(
                draft,
                ProviderChoice {
                    provider: Provider::Claude,
                    model: None,
                },
                main_choice(),
                "join",
            )
            .unwrap();
        assert!(done.refused_leave.is_none());
        assert_eq!(
            cockpit.groups().get(sending).unwrap().members,
            [threads[1], done.thread],
            "the deferred leave applied once the new member was in"
        );
        assert_eq!(
            cockpit.roster().focused_thread(),
            Some(done.thread),
            "the Thread took the draft's own slot"
        );
        assert_eq!(cockpit.roster().draft_scope(draft), None);

        // The other way out: discarding the draft dissolves the pair as a
        // plain close would have.
        let closing = pair(&mut cockpit, threads[2], threads[3]);
        cockpit.enter_group(closing).unwrap();
        let draft = cockpit.open_draft();
        cockpit.close(PaneIdentity::Thread(threads[2])).unwrap();
        assert_eq!(cockpit.groups().get(closing).unwrap().members.len(), 2);
        cockpit.close(PaneIdentity::Draft(draft)).unwrap();
        assert!(cockpit.groups().get(closing).is_none());
        assert_eq!(cockpit.roster().view(), View::Solo);
        assert_eq!(cockpit.roster().focused_thread(), Some(threads[3]));
    }

    #[test]
    fn entering_a_group_revives_parked_members_and_keeps_focus_on_a_member() {
        let (mut cockpit, _) = cockpit("roster-enter");
        let threads = opened(&mut cockpit, 3);
        let group = pair(&mut cockpit, threads[1], threads[2]);
        cockpit.close(PaneIdentity::Thread(threads[2])).unwrap();
        assert!(cockpit.parked().unwrap().contains(&threads[2]));
        cockpit.focus(PaneIdentity::Thread(threads[1]));

        cockpit.enter_group(group).unwrap();
        assert_eq!(cockpit.roster().view(), View::Group(group));
        assert_eq!(
            cockpit.roster().focused_thread(),
            Some(threads[1]),
            "focus stays on the member the operator was on"
        );
        assert!(cockpit.thread(threads[2]).is_some(), "the parked member revived");
        assert!(cockpit.roster().park_order().is_empty(), "and cmd-o forgets it");

        assert!(cockpit.focus_thread(threads[0]));
        assert_eq!(cockpit.roster().view(), View::Solo, "a loose Thread shows Solo");
        cockpit.enter_group(group).unwrap();
        assert_eq!(
            cockpit.roster().focused_thread(),
            Some(threads[1]),
            "entering from outside lands on the first member"
        );
    }

    #[test]
    fn reopen_walks_this_launches_park_order_then_creation_order() {
        let (mut cockpit, _) = cockpit("roster-reopen");
        let threads = opened(&mut cockpit, 3);
        // Parked before this launch, as far as the roster can know.
        cockpit.park(threads[2]).unwrap();
        cockpit.close(PaneIdentity::Thread(threads[0])).unwrap();
        cockpit.close(PaneIdentity::Thread(threads[1])).unwrap();
        assert!(cockpit.roster().panes().is_empty());

        let (first, opened) = cockpit.reopen_last().unwrap();
        opened.unwrap();
        assert_eq!(first, threads[1], "the last park comes back first");
        assert_eq!(cockpit.reopen_last().unwrap().0, threads[0]);
        assert_eq!(
            cockpit.reopen_last().unwrap().0,
            threads[2],
            "the order drained: creation order"
        );
        assert!(cockpit.reopen_last().is_none());
        assert_eq!(cockpit.roster().focused_thread(), Some(threads[2]), "reopen focuses");
    }

    #[test]
    fn fullscreen_follows_focus_and_survives_a_close_but_not_an_external_park() {
        let (mut cockpit, _) = cockpit("roster-fullscreen");
        let threads = opened(&mut cockpit, 3);
        cockpit.toggle_fullscreen();
        assert_eq!(cockpit.roster().fullscreen(), Some(PaneIdentity::Thread(threads[0])));
        cockpit.step_focus(1);
        assert_eq!(
            cockpit.roster().fullscreen(),
            Some(PaneIdentity::Thread(threads[0])),
            "Solo has one visible Pane to step through"
        );
        cockpit.focus(PaneIdentity::Thread(threads[1]));
        assert_eq!(cockpit.roster().fullscreen(), Some(PaneIdentity::Thread(threads[1])));
        cockpit.close(PaneIdentity::Thread(threads[1])).unwrap();
        assert_eq!(
            cockpit.roster().fullscreen(),
            Some(PaneIdentity::Thread(threads[2])),
            "the survivor fills the screen, like the next tab"
        );
        cockpit.park(threads[2]).unwrap();
        assert_eq!(
            cockpit.roster().fullscreen(),
            None,
            "parked under the roster: back to the grid"
        );
    }

    #[test]
    fn a_drop_out_of_a_pair_tracks_its_origin_and_lands_on_the_survivor() {
        let (mut cockpit, _) = cockpit("roster-drop");
        let threads = opened(&mut cockpit, 3);
        let group = pair(&mut cockpit, threads[0], threads[1]);
        cockpit.enter_group(group).unwrap();
        cockpit
            .drop(
                Drag::Thread {
                    thread: threads[0],
                    group: Some(group),
                },
                View::Group(group),
                DropTarget::ThreadRow {
                    thread: threads[2],
                    group: None,
                    index: 0,
                },
            )
            .unwrap();
        assert_eq!(cockpit.roster().view(), View::Solo);
        assert_eq!(cockpit.roster().focused_thread(), Some(threads[1]));
        assert!(cockpit.groups().get(group).is_none());
    }

    #[test]
    fn the_next_decision_lands_in_the_view_that_shows_the_waiting_thread() {
        let (mut cockpit, fake) = cockpit("roster-decision");
        let threads = opened(&mut cockpit, 3);
        let group = pair(&mut cockpit, threads[1], threads[2]);
        fake.streams.borrow()[2]
            .send(decision("perm", "Write"))
            .unwrap();
        cockpit.pump();
        assert_eq!(cockpit.next_decision(), Some(threads[2]));
        assert_eq!(cockpit.roster().view(), View::Group(group));
        assert_eq!(cockpit.roster().focused_thread(), Some(threads[2]));
    }
}
