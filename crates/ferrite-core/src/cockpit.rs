//! The Cockpit's headless state: what the operator is on the hook for, per
//! Thread.
//!
//! The pump's beginnings: Threads, their pending Decisions, and prompts held
//! back while a turn runs. No process and no window — a Thread's events are
//! fed in, and what the operator must answer comes out.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::mpsc::Receiver;

use crate::providers::Session;
use crate::store::{LoadError, Provider, Store, ThreadWriter};
use crate::transcript::{BlockId, Input, Lexer, Transcript};
use crate::workspace::{self, WorkspaceBinding, WorkspaceChoice};
use crate::{Decision, DecisionAnswer, SessionEvent, ThreadId};

/// How a Session is started. Injected so the cockpit can be driven with
/// scripted Sessions in tests — nothing below this line spawns a process.
pub trait Spawner {
    /// `resume` is the provider-native id of a Thread being revived, which the
    /// provider reloads its own history from. `cwd` is the Thread's workspace
    /// binding resolved to the directory the Session works in; `None` only
    /// for a Thread from before bindings were recorded.
    fn spawn(
        &mut self,
        provider: Provider,
        resume: Option<&str>,
        cwd: Option<&Path>,
    ) -> io::Result<Box<dyn Session>>;
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
    pending: Option<Decision>,
    /// A prompt the operator wrote while the turn was still running.
    queued: Option<String>,
    busy: bool,
    /// The provider-native id a replacement Session resumes from — the latest
    /// the provider announced. Held here rather than read back from the log,
    /// which has not necessarily flushed when the watchdog acts.
    resume: Option<String>,
    /// The checkout this Thread works in — what the Pane's chrome shows and
    /// what every replacement Session spawns into. `None` only for a Thread
    /// from before bindings were recorded.
    workspace: Option<WorkspaceBinding>,
}

impl Thread {
    /// A Thread ready to serve: its own Lexer, a fresh Transcript, a live
    /// Session. Revival replays history into the Transcript afterwards.
    fn fresh(
        session: Box<dyn Session>,
        writer: ThreadWriter,
        provider: Provider,
        resume: Option<String>,
        workspace: Option<WorkspaceBinding>,
    ) -> Self {
        let (lexer, highlights) = Lexer::new();
        Self {
            transcript: Transcript::new(std::sync::Arc::new(lexer)),
            highlights,
            session: Some(session),
            writer,
            provider,
            pending: None,
            queued: None,
            busy: false,
            resume,
            workspace,
        }
    }
}

pub struct Cockpit {
    threads: BTreeMap<ThreadId, Thread>,
    store: Store,
    spawner: Box<dyn Spawner>,
    sampler: Option<Box<dyn RssSampler>>,
    /// Bytes one Session may hold before the watchdog replaces it.
    limit: u64,
}

impl Cockpit {
    pub fn new(store: Store, spawner: Box<dyn Spawner>) -> Self {
        Self {
            threads: BTreeMap::new(),
            store,
            spawner,
            sampler: None,
            limit: u64::MAX,
        }
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
            let cwd = thread
                .workspace
                .as_ref()
                .map(|binding| binding.cwd().to_path_buf());
            // Drop the old Session before asking for a new one: the leaking
            // process must not outlive its replacement.
            thread.session = None;
            let spawned = self
                .spawner
                .spawn(thread.provider, resume.as_deref(), cwd.as_deref());
            let note = match spawned {
                Ok(session) => {
                    thread.session = Some(session);
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
    /// serving it. A worktree choice creates the worktree here, on demand —
    /// and a worktree that cannot be created takes the half-born Thread
    /// with it rather than leaving a binding that points at nothing.
    pub fn open(&mut self, provider: Provider, workspace: WorkspaceChoice) -> io::Result<ThreadId> {
        let (id, writer, binding) = self.store.create(provider, workspace)?;
        if let Err(e) = ensure_workspace(&binding, id) {
            let _ = self.store.delete(id);
            return Err(io::Error::other(e));
        }
        let session = self.spawner.spawn(provider, None, Some(binding.cwd()))?;
        self.threads.insert(
            id,
            Thread::fresh(session, writer, provider, None, Some(binding)),
        );
        Ok(id)
    }

    /// The checkout an open Thread works in — what the Pane's chrome shows.
    pub fn workspace(&self, thread: ThreadId) -> Option<&WorkspaceBinding> {
        self.threads.get(&thread)?.workspace.as_ref()
    }

    /// Delete a Thread for good: its log, its directory, and — when it was
    /// bound to a worktree — the worktree itself, but only a clean one
    /// ("clean" exactly as `git status` defines it). A dirty worktree
    /// refuses the whole deletion before anything is touched: uncommitted
    /// agent work must never vanish with a keystroke. The worktree's branch
    /// is never deleted — it is what keeps the Thread's commits reachable
    /// after the tree is gone.
    pub fn delete(&mut self, thread: ThreadId) -> Result<(), DeleteError> {
        let snapshot = self.store.load(thread).map_err(DeleteError::Load)?;
        if let Some(WorkspaceBinding::Worktree { repo, path }) = snapshot.workspace() {
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
                workspace::remove_worktree(&repo, &path).map_err(DeleteError::Git)?;
            }
        }
        self.threads.remove(&thread);
        self.store.delete(thread).map_err(DeleteError::Io)
    }

    /// Close a Pane: the Session ends, the log is flushed, and the Thread
    /// keeps nothing in memory until it is opened again.
    pub fn park(&mut self, thread: ThreadId) -> io::Result<()> {
        let Some(mut state) = self.threads.remove(&thread) else {
            return Ok(());
        };
        state.session = None;
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
            ensure_workspace(binding, thread).map_err(|e| LoadError::Io(io::Error::other(e)))?;
        }
        let cwd = workspace
            .as_ref()
            .map(|binding| binding.cwd().to_path_buf());
        let session = self
            .spawner
            .spawn(provider, snapshot.resume_target(), cwd.as_deref())
            .map_err(LoadError::Io)?;
        let writer = self.store.writer(thread)?;

        let resume = snapshot.resume_target().map(|target| target.to_string());
        let mut state = Thread::fresh(session, writer, provider, resume, workspace);
        for input in snapshot.inputs() {
            state.transcript.apply(input);
        }
        state.transcript.apply(Input::Revived);

        self.threads.insert(thread, state);
        Ok(())
    }

    /// Send a prompt now: on the wire, in the transcript, in the log.
    pub fn send(&mut self, thread: ThreadId, text: String) {
        let Some(state) = self.threads.get_mut(&thread) else {
            return;
        };
        match &mut state.session {
            Some(session) => {
                if let Err(e) = session.send(&text) {
                    state
                        .transcript
                        .apply(Input::Notice(format!("send failed: {e}")));
                    return;
                }
                let _ = state.writer.record_prompt(&text);
                state.transcript.apply(Input::Prompt(text));
            }
            None => {
                state.transcript.apply(Input::Prompt(text));
                state
                    .transcript
                    .apply(Input::Notice("no session — this Thread is parked".into()));
            }
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

    pub fn transcript(&self, thread: ThreadId) -> Option<&Transcript> {
        Some(&self.threads.get(&thread)?.transcript)
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

    /// Which backend serves this Thread.
    pub fn provider(&self, thread: ThreadId) -> Option<Provider> {
        Some(self.threads.get(&thread)?.provider)
    }

    pub fn threads(&self) -> Vec<ThreadId> {
        self.threads.keys().copied().collect()
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
                let _ = thread.writer.record_event(&event);
                if let Wake::Send(held) = fold(thread, &event) {
                    release = Some(held);
                }
                let applied = thread.transcript.apply(Input::Event(event));
                update.dirty.extend(applied.dirty);
                update.evicted.extend(applied.evicted);
            }
            for answer in answers {
                update.dirty.extend(thread.transcript.apply(answer).dirty);
            }
            if let Some(held) = release {
                if let Some(session) = &mut thread.session {
                    let _ = session.send(&held);
                    let _ = thread.writer.record_prompt(&held);
                    let applied = thread.transcript.apply(Input::Prompt(held));
                    update.dirty.extend(applied.dirty);
                }
            }
            frame.push(update);
        }
        frame
    }

    /// Is a turn running? A prompt written now has to wait for it.
    pub fn busy(&self, thread: ThreadId) -> bool {
        self.threads.get(&thread).is_some_and(|state| state.busy)
    }

    /// Hold a prompt written mid-turn. It stays visible and editable until the
    /// turn ends, which is when it is sent.
    pub fn queue(&mut self, thread: ThreadId, text: String) {
        if let Some(state) = self.threads.get_mut(&thread) {
            state.queued = Some(text);
        }
    }

    /// Take a held prompt back into the Composer, so a typo written mid-turn
    /// is fixable before it goes out.
    pub fn unqueue(&mut self, thread: ThreadId) -> Option<String> {
        self.threads.get_mut(&thread)?.queued.take()
    }

    pub fn queued(&self, thread: ThreadId) -> Option<&str> {
        self.threads.get(&thread)?.queued.as_deref()
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

    /// What this Thread is blocked on, if anything.
    pub fn pending(&self, thread: ThreadId) -> Option<&Decision> {
        self.threads.get(&thread)?.pending.as_ref()
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

fn megabytes(bytes: u64) -> String {
    format!("{} MB", bytes / (1024 * 1024))
}

/// The workspace a binding names, made real: a worktree is created (or
/// recreated) on the Thread's own branch; the main checkout is the
/// operator's and is touched by nothing.
fn ensure_workspace(
    binding: &WorkspaceBinding,
    thread: ThreadId,
) -> Result<(), workspace::GitError> {
    if let WorkspaceBinding::Worktree { repo, path } = binding {
        workspace::ensure_worktree(repo, path, &branch_name(thread))?;
    }
    Ok(())
}

/// The branch a Thread's worktree lives on. Named for the Thread so the
/// operator can read `git branch` and know whose work each one is.
fn branch_name(thread: ThreadId) -> String {
    format!("ferrite/thread-{thread}")
}

/// The bookkeeping half of a fold: what the operator is on the hook for.
fn fold(state: &mut Thread, event: &SessionEvent) -> Wake {
    let mut wake = Wake::Nothing;
    match event {
        SessionEvent::TextDelta { .. }
        | SessionEvent::ThinkingDelta { .. }
        | SessionEvent::ReasoningSummaryDelta { .. }
        | SessionEvent::ToolStarted { .. } => state.busy = true,
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
    }

    impl crate::providers::Session for Scripted {
        fn events(&self) -> &Receiver<SessionEvent> {
            &self.rx
        }

        fn send(&mut self, text: &str) -> std::io::Result<()> {
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
        resumed: Rc<RefCell<Vec<Option<String>>>>,
        cwds: Rc<RefCell<Vec<Option<std::path::PathBuf>>>>,
    }

    impl Spawner for Fake {
        fn spawn(
            &mut self,
            _provider: Provider,
            resume: Option<&str>,
            cwd: Option<&Path>,
        ) -> std::io::Result<Box<dyn crate::providers::Session>> {
            let (tx, rx) = mpsc::channel();
            self.streams.borrow_mut().push(tx);
            self.resumed
                .borrow_mut()
                .push(resume.map(|target| target.to_string()));
            self.cwds
                .borrow_mut()
                .push(cwd.map(|path| path.to_path_buf()));
            Ok(Box::new(Scripted {
                rx,
                sent: self.sent.clone(),
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
                WorkspaceChoice::Worktree { repo: repo.clone() },
            )
            .unwrap();

        let Some(WorkspaceBinding::Worktree { path, repo: bound }) =
            cockpit.workspace(thread).cloned()
        else {
            panic!(
                "the Pane must know its binding: {:?}",
                cockpit.workspace(thread)
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
            cockpit.workspace(thread),
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
                WorkspaceChoice::Worktree { repo: repo.clone() },
            )
            .unwrap();
        let binding = cockpit.workspace(thread).cloned().unwrap();
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

        assert_eq!(relaunched.workspace(thread), Some(&binding));
        assert!(
            binding.cwd().join(".git").exists(),
            "the worktree must come back on demand"
        );
        assert_eq!(
            relaunched_fake.cwds.borrow().last().unwrap().as_deref(),
            Some(binding.cwd())
        );
    }

    /// AC: the worktree is removed when the Thread is deleted, if clean —
    /// and a dirty one refuses the whole deletion, destroying nothing.
    #[test]
    fn deleting_a_thread_removes_a_clean_worktree_and_refuses_a_dirty_one() {
        let root = scratch("delete-worktree");
        let (mut cockpit, _fake) = cockpit("delete-worktree-store");
        let repo = init_repo(&root);
        let thread = cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::Worktree { repo: repo.clone() },
            )
            .unwrap();
        let path = cockpit.workspace(thread).unwrap().cwd().to_path_buf();

        // The agent left uncommitted work: deletion must refuse whole.
        std::fs::write(path.join("wip.txt"), "uncommitted\n").unwrap();
        match cockpit.delete(thread) {
            Err(DeleteError::DirtyWorktree { path: dirty }) => assert_eq!(dirty, path),
            other => panic!("a dirty worktree must refuse deletion: {other:?}"),
        }
        assert_eq!(
            std::fs::read_to_string(path.join("wip.txt")).unwrap(),
            "uncommitted\n",
            "refusal must destroy nothing"
        );
        assert!(
            cockpit.workspace(thread).is_some(),
            "the Thread must survive its own refused deletion"
        );

        // Cleaned up, the deletion goes through: worktree gone, Thread gone,
        // branch kept (a clean tree can still hold unmerged commits).
        std::fs::remove_file(path.join("wip.txt")).unwrap();
        cockpit.delete(thread).unwrap();
        assert!(!path.exists(), "the worktree must be removed");
        assert!(cockpit.threads().is_empty());
        assert_eq!(cockpit.parked().unwrap(), vec![]);
        let branches =
            crate::workspace::git_for_tests(&repo, &["branch", "--list", &branch_name(thread)]);
        assert!(
            branches.contains(&branch_name(thread)),
            "the branch must survive: {branches:?}"
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
                WorkspaceChoice::Worktree { repo: repo.clone() },
            )
            .unwrap();
        let two = cockpit
            .open(
                Provider::Codex,
                WorkspaceChoice::Worktree { repo: repo.clone() },
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
        assert_eq!(cockpit.transcript(one).unwrap().blocks().len(), 1);
        assert!(cockpit.transcript(two).unwrap().blocks().is_empty());
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
        let echoed = cockpit.transcript(thread).unwrap().blocks().last().cloned();
        assert!(
            matches!(echoed.map(|b| b.body), Some(Body::Prompt(line)) if line == "run the tests")
        );
        // And durably: a restart must not lose what the operator asked for.
        cockpit.park(thread).unwrap();
        cockpit.revive(thread).unwrap();
        assert!(cockpit
            .transcript(thread)
            .unwrap()
            .blocks()
            .iter()
            .any(|block| matches!(&block.body, Body::Prompt(line) if line == "run the tests")));
    }

    #[test]
    fn answering_through_the_cockpit_clears_the_card_and_records_who_did_it() {
        let (mut cockpit, fake) = cockpit("respond");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        fake.streams.borrow()[0]
            .send(decision("perm_01", "Write"))
            .unwrap();
        cockpit.pump();
        let pending = cockpit.pending(thread).cloned().unwrap();

        cockpit.respond(
            thread,
            &pending,
            crate::DecisionAnswer::Allow {
                input: pending.input.clone(),
            },
        );

        assert_eq!(cockpit.pending(thread), None);
        let last = cockpit.transcript(thread).unwrap().blocks().last().cloned();
        assert!(matches!(last.map(|b| b.body), Some(Body::Meta(line)) if line == "allowed Write"));

        // A second press answers nothing twice.
        cockpit.respond(
            thread,
            &pending,
            crate::DecisionAnswer::Allow {
                input: pending.input.clone(),
            },
        );
        let blocks = cockpit.transcript(thread).unwrap().blocks();
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
            .transcript(thread)
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

        let blocks = cockpit.transcript(thread).unwrap().blocks();
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
        assert!(cockpit.transcript(thread).is_none());
        assert!(cockpit.threads().is_empty());

        cockpit.revive(thread).unwrap();

        let blocks = cockpit.transcript(thread).unwrap().blocks();
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

    #[test]
    fn a_decision_is_pending_against_the_thread_that_raised_it() {
        let (mut cockpit, fake) = cockpit("pending");
        let one = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let two = cockpit.open(Provider::Claude, main_choice()).unwrap();

        fake.streams.borrow()[0]
            .send(decision("perm_01", "Write"))
            .unwrap();
        cockpit.pump();

        assert_eq!(cockpit.pending(one).unwrap().tool_name, "Write");
        assert_eq!(cockpit.pending(two), None);
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
        assert_eq!(cockpit.pending(thread), None);

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

        assert_eq!(cockpit.pending(thread), None);
        assert!(cockpit.blocked().is_empty());
        assert!(!cockpit.answer(thread, "perm_01"));
    }

    #[test]
    fn a_prompt_typed_during_a_turn_is_sent_when_the_turn_ends() {
        let (mut cockpit, fake) = cockpit("queued");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        fake.streams.borrow()[0].send(text("working")).unwrap();
        cockpit.pump();
        assert!(cockpit.busy(thread));

        cockpit.queue(thread, "and then run the tests".into());
        assert_eq!(cockpit.queued(thread), Some("and then run the tests"));
        assert!(fake.sent.borrow().is_empty(), "nothing goes out mid-turn");

        fake.streams.borrow()[0].send(ended()).unwrap();
        cockpit.pump();

        assert_eq!(fake.sent.borrow().as_slice(), ["and then run the tests"]);
        assert_eq!(cockpit.queued(thread), None);
        assert!(!cockpit.busy(thread));
    }

    #[test]
    fn a_held_prompt_can_be_taken_back_for_editing() {
        let (mut cockpit, _fake) = cockpit("unqueue");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        cockpit.queue(thread, "run the tets".into());

        let back = cockpit.unqueue(thread);

        assert_eq!(back.as_deref(), Some("run the tets"));
        assert_eq!(cockpit.queued(thread), None);
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
}
