//! One machine as a window holds it (ADR 0045): the link to its Engine and
//! everything that Engine's messages built — each open Thread's mirror,
//! the shared state's copies, the parked rows, what it was told of the
//! window and what the window streams from it. A Cockpit holds one per
//! machine, by `MachineId`, and names every Thread of one by a key that
//! carries its id, so two machines' Threads of the same number never meet.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use super::{mirror, parked, remote, shared, Ask, Asked, Said, Unread};
use ferrite_core::clock::ServerClock;
use ferrite_core::machine::Bridge;
use ferrite_core::pane_update::PaneUpdate;
use ferrite_core::protocol::{
    CliAction, Done, GitHubAsk, Op, ReviewPullRequest, ReviewPullRequests, ServerMessage, WireError,
    WirePrFilter,
};
use ferrite_core::provider::Provider;
use ferrite_core::roster::{BootstrapResult, Bootstrapped, DraftId, Roster};
use ferrite_core::{MachineId, MachineKey, ProjectKey, ThreadId, ThreadKey};
use ferrite_reviewr::ferrite::{Bases, FileSides, Read};
use ferrite_reviewr::git::{CommitRow, WorktreeEntry};
use ferrite_reviewr::search::SearchResults;

/// How long a Thread off screen and at rest stays streamed whole: as long
/// as the Server keeps its Session (ADR 0032). One at work stays however
/// long it works, drawn here or not.
const UNWATCH_AFTER: Duration = ferrite_core::digest::HELD_AT_REST;

pub(super) struct HeldMachine {
    /// What every key of this machine's Threads carries.
    pub(super) id: MachineId,
    /// What saved files name this machine.
    pub(super) saved_as: MachineKey,
    pub(super) link: Box<dyn Link>,
    /// The Server said something since the window last asked
    /// (`take_arrived`).
    pub(super) arrived: bool,
    /// What a parked Thread says: the rows its Server published.
    pub(super) parked: parked::Parked,
    /// The open Threads, as the Roster last followed them (`sync_roster`).
    synced: Vec<ThreadId>,
    /// Each open Thread as the Engine's messages built it.
    pub(super) mirrors: mirror::Mirrors,
    /// What every window shares, as the Engine's messages left it.
    pub(super) shared: shared::Shared,
    /// What the Engine was last told of this window (`tell_window`); `None`
    /// until told, and again after every attach: a Server forgets a window
    /// that left.
    told: Option<Told>,
    /// What its Server was last told is seen on each visible Thread, so
    /// the same unread things are told once (`acknowledge_seen`); emptied
    /// with `told` at every attach.
    pub(super) seen_told: BTreeMap<ThreadId, Vec<Unread>>,
    /// What this window draws of this machine's Threads at Transcript
    /// level (`draws_transcripts`). `None` for an Engine in this process
    /// until its window first says: until then it streams every open
    /// Thread.
    pub(super) drawn: Option<Drawn>,
    /// The `window.*` actions this machine's Server handed the window and
    /// it has not done: only a `ferrite --headless` one is (ADR 0038).
    pub(super) drives: Vec<(u64, ferrite_core::drive::Action)>,
}

impl HeldMachine {
    /// `limits` are what its Engine's Activity retains, so its mirrors
    /// retain the same.
    pub(super) fn new(
        link: Box<dyn Link>,
        limits: ferrite_core::activity::ActivityLimits,
        saved_as: MachineKey,
    ) -> Self {
        Self {
            id: MachineId::mint(),
            saved_as,
            link,
            arrived: false,
            parked: parked::Parked::default(),
            synced: Vec::new(),
            mirrors: mirror::Mirrors::retaining(limits),
            shared: shared::Shared::new(),
            told: None,
            seen_told: BTreeMap::new(),
            drawn: None,
            drives: Vec::new(),
        }
    }

    /// `thread` of this machine.
    pub(super) fn key(&self, thread: ThreadId) -> ThreadKey {
        ThreadKey::new(self.id, thread)
    }

    pub(super) fn project_key(&self, project: ferrite_core::project::ProjectId) -> ProjectKey {
        ProjectKey::new(self.id, project)
    }

    /// `server_ms`, milliseconds since the epoch on this machine's clock, as
    /// a time on this computer's (F-A4, T3 Code's `toServerClock`): two
    /// machines' times compare as they happened, however far apart their
    /// clocks are.
    pub(super) fn wall(&self, server_ms: i64) -> SystemTime {
        let local_ms = self.link.clock().local_ms(server_ms);
        SystemTime::UNIX_EPOCH + Duration::from_millis(local_ms.max(0) as u64)
    }

    /// Whether this machine is this computer, reached: a path it names is
    /// a path on this disk. A machine's is not (ADR 0026).
    pub(super) fn local(&self) -> bool {
        self.link.bridge().is_none()
    }

    /// What its Engine said since the last take.
    pub(super) fn take_arrivals(&mut self) -> Vec<remote::Arrival> {
        self.link.poll()
    }

    /// Take in what arrived: the mirrors, the copies, what the window is
    /// to hear, and which Threads to stream. The first sends that settled
    /// are the Cockpit's to complete, in its Roster.
    pub(super) fn arrive(
        &mut self,
        arrived: Vec<remote::Arrival>,
        said: &mut Said,
    ) -> Vec<BootstrapResult> {
        self.arrived |= !arrived.is_empty();
        let open_before = self.shared.open.clone();
        let attached = arrived
            .iter()
            .any(|arrival| matches!(arrival, remote::Arrival::Attached { .. }));
        let boot = self.boot();
        let mut settled = Vec::new();
        for arrival in arrived {
            match arrival {
                remote::Arrival::Message(message) => self.message(message, boot, said),
                remote::Arrival::Attached { snapshot, new_boot } => {
                    self.told = None;
                    self.seen_told.clear();
                    if new_boot {
                        self.mirrors = mirror::Mirrors::retaining(self.mirrors.limits());
                    }
                    self.shared.attached(&snapshot);
                    self.parked.attached(&snapshot);
                    self.mirrors.retain(&self.shared.open);
                    self.mirrors.restore_decisions(&snapshot.decisions);
                    for state in snapshot.threads {
                        self.mirrors.receive(
                            ServerMessage::Thread {
                                revision: snapshot.revision,
                                state,
                            },
                            boot,
                        );
                    }
                    self.mirrors.attached(snapshot.digests, boot);
                }
                remote::Arrival::Started(result) => settled.push(result),
                remote::Arrival::Answered => {}
            }
        }
        // What a Server's snapshot lists is not new.
        if !attached {
            let now = ferrite_core::clock::instant();
            if let Some(drawn) = &mut self.drawn {
                for &thread in &self.shared.open {
                    if !open_before.contains(&thread) {
                        drawn.hold(thread, now);
                    }
                }
            }
        }
        self.follow_attention();
        settled
    }

    /// The Engine's clock zero on this side.
    fn boot(&self) -> Instant {
        self.link.boot()
    }

    fn message(&mut self, message: ServerMessage, boot: Instant, said: &mut Said) {
        if let ServerMessage::Drive { request, action } = message {
            self.drives.push((request, action));
            return;
        }
        if let ServerMessage::Browser { thread, .. }
        | ServerMessage::BrowserRequest { thread, .. } = &message
        {
            said.browser.push((self.key(*thread), message));
            return;
        }
        if let ServerMessage::AllowanceWarning {
            provider,
            window,
            threshold,
            used_fraction,
        } = &message
        {
            if let Ok(window) = (*window).try_into() {
                said.allowance_warnings
                    .push(ferrite_core::allowance::AllowanceWarning {
                        provider: *provider,
                        window,
                        threshold: *threshold,
                        used_fraction: *used_fraction,
                    });
            }
        }
        match &message {
            ServerMessage::Focus { thread, fork_of } => said.asked.push(Asked::Focus {
                thread: self.key(*thread),
                fork_of: fork_of.map(|fork_of| self.key(fork_of)),
            }),
            ServerMessage::Notification {
                title,
                body,
                sound,
                important,
            } => said.asked.push(Asked::Notification {
                title: title.clone(),
                body: body.clone(),
                sound: sound.sound(),
                important: *important,
            }),
            ServerMessage::TaskAlert {
                project,
                task,
                status,
                title,
                detail,
            } => said.asked.push(Asked::TaskAlert {
                project: self.project_key(*project),
                task: task.clone(),
                status: *status,
                title: title.clone(),
                detail: detail.clone(),
            }),
            _ => {}
        }
        self.shared.receive(&message);
        self.parked.receive(&message);
        self.mirrors.receive(message, boot);
    }

    /// The Roster follows this machine's open Threads, whoever changed
    /// them — this window, another, or an API caller. A Thread that opened
    /// gets a Pane at the end (a Thread a Draft is starting takes the
    /// Draft's slot when it settles instead); one that closed loses its
    /// Pane.
    pub(super) fn sync_roster(&mut self, roster: &mut Roster) {
        let open = self.shared.open.clone();
        for gone in self.synced.iter().filter(|thread| !open.contains(thread)) {
            roster.remove_thread(self.key(*gone));
        }
        for thread in open.iter().filter(|thread| !self.synced.contains(thread)) {
            if !self.mirrors.for_draft(*thread) {
                roster.insert_thread(self.key(*thread));
            }
        }
        self.synced = open;
    }

    /// What changed for each of this machine's Panes since the last take.
    pub(super) fn take_updates(&mut self) -> impl Iterator<Item = PaneUpdate<ThreadKey>> + '_ {
        let id = self.id;
        self.mirrors
            .take_updates()
            .into_iter()
            .map(move |update| PaneUpdate {
                thread: ThreadKey::new(id, update.thread),
                dirty: update.dirty,
                evicted: update.evicted,
                subjects: update.subjects,
                activity_changed: update.activity_changed,
                redirects: update.redirects,
            })
    }

    /// The Threads this window streams whole from this machine: those it
    /// draws at Transcript level, and on this computer those drawn or at
    /// work within `UNWATCH_AFTER`. Another machine is a round trip and a
    /// link away, so its Threads stream whole only while drawn (F-A2):
    /// their status, Decisions and Notices arrive all the same. Until the
    /// window says what it draws, or while its Engine sends no digests to
    /// read the rest from, every open one.
    fn watched(&self) -> Vec<ThreadId> {
        let open = &self.shared.open;
        let Some(drawn) = self.drawn.as_ref().filter(|_| self.mirrors.digested()) else {
            return open.clone();
        };
        let now = ferrite_core::clock::instant();
        let lingers = self.local();
        open.iter()
            .copied()
            .filter(|thread| {
                if lingers {
                    drawn.holds(*thread, now)
                } else {
                    drawn.shows(*thread)
                }
            })
            .collect()
    }

    /// Stream what the window draws, and let the rest go.
    pub(super) fn follow_attention(&mut self) {
        if let Some(drawn) = &mut self.drawn {
            let now = ferrite_core::clock::instant();
            for thread in self.mirrors.at_work() {
                drawn.hold(thread, now);
            }
        }
        let watched = self.watched();
        if let Some(drawn) = &mut self.drawn {
            drawn.retain(&self.shared.open);
        }
        self.link.stream(&watched, &mut self.mirrors);
    }

    /// Tell the Engine what changed of what only this window knows of this
    /// machine's Threads: the one its focused Pane shows while it has the
    /// operator's focus, and its Composers holding unsent text.
    pub(super) fn tell_window(
        &mut self,
        focused: Option<ThreadKey>,
        composing: &BTreeSet<ThreadKey>,
    ) {
        let mine = |key: ThreadKey| (key.machine() == self.id).then(|| key.thread());
        let now = Told {
            focused: focused.and_then(mine),
            composing: composing.iter().copied().filter_map(mine).collect(),
        };
        let before = self.told.replace(now.clone());
        let mut said = Vec::new();
        if before.as_ref().map(|told| told.focused) != Some(now.focused) {
            said.push(Op::Focused {
                thread: now.focused,
            });
        }
        let composed = before.map(|told| told.composing).unwrap_or_default();
        for (threads, unsent) in [
            (now.composing.difference(&composed), true),
            (composed.difference(&now.composing), false),
        ] {
            said.extend(threads.map(|thread| Op::Composing {
                thread: *thread,
                unsent,
            }));
        }
        for op in said {
            self.link.tell(op);
        }
    }

    /// Every Thread its Server has, open or parked, owned ones included.
    pub(super) fn known(&self) -> impl Iterator<Item = ThreadId> + '_ {
        self.mirrors
            .threads()
            .into_iter()
            .chain(self.parked.thread_ids())
    }

    /// Its Server, for what only a Server does.
    pub(super) fn remote(&mut self) -> io::Result<&mut remote::Remote> {
        self.link.remote().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Unsupported, "this window runs its agents itself")
        })
    }

    /// A first send that settled, as the window names its Thread.
    pub(super) fn keyed(&self, result: BootstrapResult) -> BootstrapResult<ThreadKey> {
        BootstrapResult {
            draft: result.draft,
            prompt: result.prompt,
            result: result.result.map(|done| Bootstrapped {
                thread: self.key(done.thread),
            }),
        }
    }
}

/// Where a machine's Engine runs, as its window reaches it (ADR 0049): in
/// its Server, over the socket (`remote::Remote`); or, for the tests and
/// the demo only, in this process (`in_process`). Every act the window
/// takes of a machine goes through it, so the window's code is the same
/// for both; what only a Server does is its `remote`.
pub(super) trait Link {
    /// What its Engine said since the last take.
    fn poll(&mut self) -> Vec<remote::Arrival>;

    /// The Engine's clock zero on this side.
    fn boot(&self) -> Instant;

    /// Its machine's clock, as this computer's reads it.
    fn clock(&self) -> ServerClock;

    /// The bridge its machine is reached through; `None` for this computer.
    fn bridge(&self) -> Option<&Arc<Bridge>>;

    /// Whether what it is told reaches its Engine now.
    fn reaches_server(&self) -> bool;

    /// Whether its Engine names the Thread a fork that opened was made
    /// from (`Capability::ForkOf`).
    fn names_fork_owners(&self) -> bool;

    /// Whether an act is carried out by the time it returns, so what it
    /// changed is the window's to take in at once (`Cockpit::settle`). A
    /// Server's arrives as its messages do.
    fn answers_now(&self) -> bool {
        false
    }

    /// One act, its reply waited for.
    fn execute(&mut self, op: Op) -> Executed;

    /// One act whose reply nobody reads.
    fn tell(&mut self, op: Op);

    /// One act whose reply goes to `answer` once it comes, or that it did
    /// not come in time; at once when it cannot go.
    fn ask_act(&mut self, op: Op, answer: remote::Answer);

    /// Stream exactly `watched` into `mirrors`, each from where its mirror
    /// left off; the rest stop.
    fn stream(&mut self, watched: &[ThreadId], mirrors: &mut mirror::Mirrors);

    /// A Draft's first send: the Thread it became when it started at once,
    /// else `None`, and it settles in `poll`.
    fn bootstrap(&mut self, draft: DraftId, prompt: &str, op: Op) -> io::Result<Option<ThreadId>>;

    fn draft_starting(&self, draft: DraftId) -> bool;

    /// Take back a first send still starting. True when one was.
    fn cancel_bootstrap(&mut self, draft: DraftId) -> io::Result<bool>;

    /// Settings changed on disk.
    fn settings_changed(&mut self, settings: &ferrite_core::settings::Settings);

    /// One frame of an Engine the window pumps. A Server pumps its own.
    fn pump(&mut self) {}

    /// Replace each Session grown past the memory limit, in an Engine the
    /// window sweeps. A Server sweeps its own.
    fn sweep(&mut self) -> Vec<ferrite_core::memory::Restart> {
        Vec::new()
    }

    /// The window is going: Sessions that would go with it end. A
    /// Server's keep running.
    fn halt_sessions(&mut self) {}

    /// One Factory ask (ADR 0037).
    fn factory(&mut self, op: Op) -> io::Result<Option<Done>>;

    /// The provider sessions its machine has to import, at most `cap`,
    /// found by its machine's Server (ADR 0049).
    fn list_sessions(&mut self, cap: usize) -> Ask<ferrite_core::session_file::Found>;

    /// The commands `provider`'s CLI on its machine finds in `cwd` there.
    fn list_commands(
        &mut self,
        provider: Provider,
        cwd: &Path,
    ) -> Ask<Vec<ferrite_core::SessionCommand>>;

    /// Main's whole transcript as plain text, read back from the log by
    /// its machine's Server.
    fn read_transcript(&mut self, thread: ThreadId) -> Ask<String>;

    fn search_files(
        &mut self,
        thread: ThreadId,
        query: &str,
    ) -> io::Result<Receiver<io::Result<Vec<ferrite_core::prompt_files::FileSuggestion>>>>;

    fn upload_attachment(&self, path: PathBuf) -> io::Result<ferrite_core::attachment_transfer::Upload>;

    fn read_attachment(&self, path: PathBuf) -> io::Result<Receiver<io::Result<Vec<u8>>>>;

    /// A browser annotation's screenshot, cached here for display.
    fn annotation_image(
        &mut self,
        thread: ThreadId,
        annotation: ferrite_core::browser_annotations::Annotation,
    ) -> Ask<PathBuf>;

    fn changes(
        &mut self,
        request: ferrite_core::changes::Request,
    ) -> io::Result<Receiver<io::Result<ferrite_core::changes::Snapshot>>>;

    fn set_file_staged(&mut self, request: ferrite_core::changes::Request) -> io::Result<Receiver<io::Result<()>>>;

    fn file_operation(
        &mut self,
        request: ferrite_core::file_operations::Request,
    ) -> io::Result<Receiver<io::Result<ferrite_core::file_operations::Reply>>>;

    fn repositories(&mut self) -> io::Result<Receiver<io::Result<ferrite_core::protocol::Repositories>>>;

    fn review_diff(&mut self, path: &Path, read: &Read) -> io::Result<Receiver<io::Result<Vec<FileSides>>>>;

    fn review_commits(&mut self, path: &Path) -> io::Result<Receiver<io::Result<Vec<CommitRow>>>>;

    fn search_repo(&mut self, path: &Path, query: &str) -> io::Result<Receiver<io::Result<SearchResults>>>;

    fn read_review_file(&mut self, path: &Path, file: &str) -> io::Result<Receiver<io::Result<String>>>;

    fn review_pull_request_diff(
        &mut self,
        path: &Path,
        number: u64,
    ) -> io::Result<Receiver<io::Result<Vec<FileSides>>>>;

    fn review_pull_request(
        &mut self,
        path: &Path,
        number: Option<u64>,
    ) -> io::Result<Receiver<io::Result<ReviewPullRequest>>>;

    fn list_pull_requests(
        &mut self,
        path: &Path,
        filter: WirePrFilter,
    ) -> io::Result<Receiver<io::Result<ReviewPullRequests>>>;

    fn review_files(&mut self, path: &Path) -> io::Result<Receiver<io::Result<Vec<WorktreeEntry>>>>;

    fn review_bases(&mut self, path: &Path) -> io::Result<Receiver<io::Result<Bases>>>;

    fn pick_review_base(&mut self, path: &Path, base: &str) -> io::Result<Receiver<io::Result<()>>>;

    fn pull_request(
        &mut self,
        path: &Path,
    ) -> io::Result<Receiver<io::Result<Option<ferrite_core::pull_request::PullRequest>>>>;

    /// A `watch_pr`: whether the PR is watched once its Engine has it.
    fn watch_pr(&mut self, op: Op) -> io::Result<Receiver<io::Result<bool>>>;

    /// A `cancel_delegate`: answered once it is cancelled.
    fn cancel_delegate(&mut self, op: Op) -> io::Result<Receiver<io::Result<()>>>;

    /// What a GitHub start's picker lists (#225).
    fn github_names(&mut self, ask: GitHubAsk) -> io::Result<Receiver<io::Result<Vec<String>>>>;

    /// What its machine has of the provider CLIs, or the update it ran.
    fn cli(&mut self, action: CliAction) -> io::Result<Receiver<io::Result<Done>>>;

    /// Its Server, when it is one.
    fn remote(&mut self) -> Option<&mut remote::Remote> {
        None
    }

    fn remote_ref(&self) -> Option<&remote::Remote> {
        None
    }

    /// Its Engine in this process, when it is one: what the tests and the
    /// demo reach into.
    #[cfg(any(test, feature = "test-support"))]
    fn in_process(&mut self) -> Option<&mut super::in_process::InProcess> {
        None
    }

    #[cfg(any(test, feature = "test-support"))]
    fn in_process_ref(&self) -> Option<&super::in_process::InProcess> {
        None
    }
}

/// What a command came to, as its reply says.
pub(super) type Executed = Result<Option<Done>, WireError>;

/// The Threads a window draws of one machine at Transcript level, and
/// since when each other it holds has been off screen and at rest.
#[derive(Default)]
pub(super) struct Drawn {
    now: BTreeSet<ThreadId>,
    left: HashMap<ThreadId, Instant>,
}

impl Drawn {
    pub(super) fn draws(&mut self, threads: BTreeSet<ThreadId>, at: Instant) {
        for &gone in self.now.difference(&threads) {
            self.left.insert(gone, at);
        }
        for thread in &threads {
            self.left.remove(thread);
        }
        self.now = threads;
    }

    /// Hold `thread` as if it had just left the screen: one that opened
    /// while the window is up, which the operator is about to look at, or
    /// one at work, whose time at rest starts again.
    fn hold(&mut self, thread: ThreadId, at: Instant) {
        if !self.now.contains(&thread) {
            self.left.insert(thread, at);
        }
    }

    /// Whether `thread` is drawn now.
    fn shows(&self, thread: ThreadId) -> bool {
        self.now.contains(&thread)
    }

    /// Whether `thread` is drawn, or left the screen within `UNWATCH_AFTER`.
    fn holds(&self, thread: ThreadId, at: Instant) -> bool {
        self.now.contains(&thread)
            || self
                .left
                .get(&thread)
                .is_some_and(|left| at.saturating_duration_since(*left) < UNWATCH_AFTER)
    }

    fn retain(&mut self, open: &[ThreadId]) {
        self.now.retain(|thread| open.contains(thread));
        self.left.retain(|thread, _| open.contains(thread));
    }
}

/// What only a window knows and the API reports (ADR 0030): the Thread it
/// shows the operator, and its Composers holding unsent text — of one
/// machine, as that machine's Server is told.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Told {
    focused: Option<ThreadId>,
    composing: BTreeSet<ThreadId>,
}
