//! The Cockpit: one window's hold on the Engine (ADR 0025). What a window
//! keeps for itself — its Roster's focus, View and fullscreen, its Drafts,
//! whether it has the operator's focus — lives here; everything that must
//! keep running without a window is the Engine's, and the Cockpit changes
//! it only through the Engine's acts. The open Threads are shared: the
//! Roster follows the Engine's (`sync_roster`).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use crate::activity::{DecisionHandle, Subject};
pub use crate::engine::{
    title_from_prompt, BootstrapResult, Bootstrapped, DeleteError, Engine, HistoryDirection,
    LogReader, PaneUpdate, Preparation, ProviderChoice, ProvisionError, Restart, ReviveGroupError,
    RssSampler, SpawnRequest, Spawner, SubjectUpdate, ToolTiming,
};
use crate::groups::{Applied, ApplyError, Drag, DropTarget, GroupChange, GroupId, Groups, Plan};
use crate::notifications::{DecisionNoticeId, NoticeId, Notifications};
use crate::roster::{DraftId, DraftScope, PaneIdentity, Roster, View};
use crate::store::{LoadError, Provider, Store};
use crate::transcript::Input;
use crate::workspace::registry::{ProjectId, Registry};
use crate::workspace::WorkspaceChoice;
use crate::{Decision, DecisionAnswer, ModelInfo, ThreadId};
pub use mirror::ThreadView;

pub(crate) mod mirror;
mod shared;

pub struct Cockpit {
    pub(crate) engine: Engine,
    /// What is on screen (#28): the roster of Panes, focus, view, fullscreen
    /// and park order. Read through `roster()`; changed only by the acts
    /// below, which keep it showing exactly the open Threads plus drafts.
    pub(crate) roster: Roster,
    /// The Engine's open Threads as the Roster last followed them.
    synced: Vec<ThreadId>,
    /// Each open Thread as the Engine's messages built it: what `thread`
    /// reads and `pump` reports.
    mirrors: mirror::Mirrors,
    /// What every window shares, as the Engine's messages left it.
    shared: shared::Shared,
    bootstrap_results: Vec<BootstrapResult>,
    /// Whether the window has the operator's focus: what is on screen is
    /// seen only while it does (`acknowledge_seen`), so a finish or a
    /// request on screen stays unread until they come back (ADR 0017). True
    /// until the window says otherwise, so a headless Cockpit reads what it
    /// shows.
    window_active: bool,
}

impl Cockpit {
    pub fn try_new(store: Store, spawner: Box<dyn Spawner>) -> io::Result<Self> {
        Ok(Self {
            engine: Engine::try_new(store, spawner)?,
            roster: Roster::default(),
            synced: Vec::new(),
            mirrors: mirror::Mirrors::default(),
            shared: shared::Shared::default(),
            bootstrap_results: Vec::new(),
            window_active: true,
        })
    }

    /// Test and embedding convenience. The application uses `try_new` so a
    /// protected or incompatible registry is reported instead of replaced.
    pub fn new(store: Store, spawner: Box<dyn Spawner>) -> Self {
        Self::try_new(store, spawner).expect("the workspace registry opens")
    }

    /// The Roster follows the Engine's open Threads: a Thread that opened
    /// gets a Pane at the end, one that closed loses its own. A Thread a
    /// Draft is starting takes the Draft's slot when it settles instead.
    fn sync_roster(&mut self) {
        let open = self.engine.open_threads().to_vec();
        for gone in self.synced.iter().filter(|thread| !open.contains(thread)) {
            self.roster.remove_thread(*gone);
        }
        for thread in open.iter().filter(|thread| !self.synced.contains(thread)) {
            if !self.engine.starting_for_draft(*thread) {
                self.roster.insert_thread(*thread);
            }
        }
        self.synced = open;
    }

    /// Bring this window up to the Engine after one of its own acts. What
    /// the act changed is the window's to show, as it always was: only the
    /// pump reports Pane updates.
    fn settle(&mut self) {
        self.receive();
        self.mirrors.take_updates();
    }

    /// Bring this window up to the Engine: its Roster, then its mirrors,
    /// from the messages the Engine published. In debug builds each message
    /// crosses as its encoded line, so nothing escapes the contract.
    fn receive(&mut self) {
        self.sync_roster();
        let boot = self.engine.boot();
        for message in self.engine.take_messages() {
            #[cfg(debug_assertions)]
            let message = {
                let line = crate::protocol::encode(&message);
                crate::protocol::decode(&line[..line.len() - 1])
                    .expect("every message decodes as it encoded")
            };
            self.shared.receive(&message);
            self.mirrors.receive(message, boot);
        }
        #[cfg(test)]
        self.assert_mirrors_match();
    }

    /// One frame: the Engine's pump, then what changed for each Pane of
    /// this window, as its mirrors saw it.
    pub fn pump(&mut self) -> Vec<PaneUpdate> {
        // Fold highlighter answers from the previous frame first, as the
        // Engine does for its own.
        self.mirrors.drain_highlights();
        self.engine.pump();
        self.collect_bootstraps();
        self.receive();
        // What is on screen while the window has focus is seen. Here
        // rather than only at the focus doors, so a Thread that finishes in
        // view, or a focus moved by a close or a Group entry, reads the same.
        self.acknowledge_seen();
        self.mirrors.take_updates()
    }

    /// An open Thread's facts, as this window's mirror holds them.
    pub fn thread(&self, thread: ThreadId) -> Option<ThreadView<'_>> {
        self.mirrors.view(thread)
    }

    /// A Group change (#28), and what it does to the view: the Group the
    /// operator was looking at may just have dissolved under them. Its
    /// survivor is the one Thread still on screen, so that is where they
    /// land — never the Thread that left, and never a blank Cockpit.
    pub fn apply_group(&mut self, change: GroupChange) -> Result<Applied, ApplyError> {
        let applied = self.engine.apply_group(change);
        self.settle();
        let applied = applied?;
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
        self.roster.heal_focus(&self.shared.groups);
        Ok(applied)
    }

    /// Close a Pane: the Session ends, the log is flushed, and the Thread
    /// keeps nothing in memory until it is opened again. A parked Group
    /// member's tile (`enter_group`) has a Pane and no Session: parking it
    /// again only takes the Pane away.
    pub fn park(&mut self, thread: ThreadId) -> io::Result<()> {
        let parked = self.engine.park(thread);
        self.settle();
        if !self.engine.open_threads().contains(&thread) {
            self.roster.remove_thread(thread);
        }
        parked
    }

    pub fn ensure_subject_history(
        &mut self,
        thread: ThreadId,
        subject: &Subject,
    ) -> io::Result<bool> {
        let result = self.engine.ensure_subject_history(thread, subject);
        self.settle();
        result
    }

    pub fn subject_history_error(&self, thread: ThreadId, subject: &Subject) -> Option<&str> {
        self.engine.subject_history_error(thread, subject)
    }

    pub fn retry_subject_history(
        &mut self,
        thread: ThreadId,
        subject: &Subject,
    ) -> io::Result<bool> {
        let result = self.engine.retry_subject_history(thread, subject);
        self.settle();
        result
    }

    pub fn discover_sessions(
        &mut self,
        roots: Vec<(Provider, PathBuf)>,
        cap: usize,
    ) -> Option<Receiver<io::Result<Vec<crate::import::Candidate>>>> {
        let result = self.engine.discover_sessions(roots, cap);
        self.settle();
        result
    }

    /// The operator opened a Notice — from a toast or the bell: it is
    /// read, and they land on its Thread's Pane in the view that shows
    /// it, reviving a parked one. The Thread landed on, or None when it
    /// is gone.
    pub fn open_notice(&mut self, id: NoticeId) -> Option<ThreadId> {
        let thread = self.engine.open_notice(id)?;
        self.land_on(thread)
    }

    /// Open one live Decision attention record: its Subject is shown, and
    /// the operator lands on its Thread as `open_notice` does.
    pub fn open_decision_notice(&mut self, id: &DecisionNoticeId) -> Option<ThreadId> {
        let thread = self.engine.open_decision_notice(id)?;
        self.land_on(thread)
    }

    fn land_on(&mut self, thread: ThreadId) -> Option<ThreadId> {
        if !self.focus_thread(thread) {
            self.reopen(thread).ok()?;
        }
        self.acknowledge_seen();
        Some(thread)
    }

    /// Whether the operator is looking: the window has the system's focus.
    pub fn window_active(&self) -> bool {
        self.window_active
    }

    /// The window gained or lost the operator's focus. Gaining it, what
    /// is on screen is seen at once: true when that read anything.
    pub fn set_window_active(&mut self, active: bool) -> bool {
        if self.window_active == active {
            return false;
        }
        self.window_active = active;
        self.acknowledge_seen()
    }

    /// Everything on screen while the window has focus is seen: each
    /// visible Thread's Notices are read, and so are the live Decisions of
    /// the Subject its Pane shows. Fullscreen shows its one Pane. True when
    /// anything was read.
    fn acknowledge_seen(&mut self) -> bool {
        if !self.window_active {
            return false;
        }
        let shown: Vec<ThreadId> = match self.roster.fullscreen() {
            Some(fullscreen) => vec![fullscreen],
            None => self.visible(),
        }
        .into_iter()
        .filter_map(PaneIdentity::thread)
        .collect();
        self.engine.acknowledge_seen(&shown)
    }
}

/// Mirror parity (ADR 0025): after every message, each open Thread's
/// mirror reads exactly as the Engine's own Thread does. Every Cockpit
/// test checks it as it runs.
#[cfg(test)]
impl Cockpit {
    pub(crate) fn set_session_project_root(
        &mut self,
        thread: ThreadId,
        root: Option<PathBuf>,
    ) -> Result<(), LoadError> {
        let set = self.engine.set_session_project_root(thread, root);
        self.settle();
        set
    }

    fn assert_mirrors_match(&self) {
        assert_eq!(self.shared.groups.to_wire(), self.engine.groups().to_wire());
        assert_eq!(
            self.shared.registry.to_wire(),
            self.engine.registry().to_wire()
        );
        assert_eq!(
            self.shared.notifications.to_wire(),
            self.engine.notifications().to_wire()
        );
        for provider in [Provider::Claude, Provider::Codex] {
            assert_eq!(
                self.shared.announced_models(provider),
                self.engine.announced_models(provider)
            );
            assert_eq!(
                self.shared.live_sessions(provider),
                self.engine.live_sessions(provider)
            );
            assert_eq!(
                self.shared.account_limits(provider),
                self.engine.account_limits(provider)
            );
        }
        for &thread in self.engine.open_threads() {
            let engine = self.engine.thread(thread).expect("open in the Engine");
            let mirror = self
                .mirrors
                .view(thread)
                .unwrap_or_else(|| panic!("thread {thread} has no mirror"));
            assert_eq!(
                parity::seen!(mirror),
                parity::seen!(engine),
                "thread {thread}: the mirror drifted from the Engine"
            );
        }
    }
}

#[cfg(test)]
mod parity {
    use std::fmt::Write;

    use crate::activity::ActivityView;

    /// What a window could read of one Thread, as text.
    macro_rules! seen {
        ($view:expr) => {{
            let view = $view;
            format!(
                "generation={} provider={:?} model={:?} effort={:?} title={:?} \
                 suggestion={:?} pending={:?} queued={:?} starting={} busy={} working={} \
                 background={} tuning={} needs_queue={} workspace={:?} root={:?} \
                 first_prompt={} history={} commands={:?} models={:?} mode={:?} modes={:?} \
                 store_error={:?}\n{}",
                view.generation(),
                view.provider(),
                view.model(),
                view.effort(),
                view.title(),
                view.suggestion(),
                view.pending(),
                view.queued_all(),
                view.starting(),
                view.busy(),
                view.working(),
                view.background_work(),
                view.tuning(),
                view.needs_queue(),
                view.workspace(),
                view.session_project_root(),
                view.first_prompt_sent(),
                view.has_prompt_history(),
                view.commands(),
                view.models(),
                view.permission_mode(),
                view.permission_modes(),
                view.store_error(),
                crate::cockpit::parity::activity(view.activity()),
            )
        }};
    }
    pub(super) use seen;

    pub(super) fn activity(view: ActivityView<'_>) -> String {
        let main = view.main();
        let transcript = main.transcript();
        let mut out = String::new();
        let mut timings: Vec<String> = main
            .tool_timings()
            .iter()
            .map(|(id, timing)| format!("{id}={timing:?}"))
            .collect();
        timings.sort();
        writeln!(
            out,
            "main status={:?} transcript={:?} outcome={:?} model={:?} session={:?} \
             usage={:?} limits={:?} timings={timings:?}",
            main.status(),
            transcript.status(),
            transcript.turn_outcome(),
            transcript.model(),
            transcript.session_id(),
            transcript.usage(),
            transcript.rate_limits(),
        )
        .unwrap();
        for block in transcript.blocks() {
            writeln!(out, "  {:?} {:?}", block.id, block.body).unwrap();
        }
        writeln!(out, "decisions={:?}", view.decisions()).unwrap();
        for child in view.children() {
            writeln!(
                out,
                "child {} {:?} status={:?} coverage={:?} retained={}",
                child.key().as_str(),
                child.info(),
                child.status(),
                child.coverage(),
                child.retained(),
            )
            .unwrap();
            for block in child.transcript().blocks() {
                writeln!(out, "    {:?} {:?}", block.id, block.body).unwrap();
            }
        }
        out
    }
}

/// What the Engine answers, as this window asks it.
impl Cockpit {
    pub fn registry(&self) -> &Registry {
        &self.shared.registry
    }

    pub fn groups(&self) -> &Groups {
        &self.shared.groups
    }

    pub fn plan_group_drop(&self, drag: Drag, target: DropTarget) -> Plan {
        self.shared.groups.preview_drop(drag, target)
    }

    pub fn register_project(&mut self, root: &Path) -> io::Result<ProjectId> {
        let result = self.engine.register_project(root);
        self.settle();
        result
    }

    pub fn register_project_directories(&mut self, roots: &[PathBuf]) -> io::Result<ProjectId> {
        let result = self.engine.register_project_directories(roots);
        self.settle();
        result
    }

    pub fn add_project_directories(
        &mut self,
        project: ProjectId,
        roots: &[PathBuf],
    ) -> io::Result<()> {
        let result = self.engine.add_project_directories(project, roots);
        self.settle();
        result
    }

    pub fn replace_project_directory(
        &mut self,
        project: ProjectId,
        index: usize,
        root: &Path,
    ) -> io::Result<()> {
        let result = self.engine.replace_project_directory(project, index, root);
        self.settle();
        result
    }

    pub fn remove_project_directory(&mut self, project: ProjectId, index: usize) -> io::Result<()> {
        let result = self.engine.remove_project_directory(project, index);
        self.settle();
        result
    }

    pub fn group_layout(
        &self,
        group: GroupId,
        bounds: crate::layout::Rect,
    ) -> Option<crate::layout::Tree> {
        self.shared.groups.layout(group, bounds)
    }

    pub fn reset_group_layout(&mut self, group: GroupId) -> Result<(), ApplyError> {
        let result = self.engine.reset_group_layout(group);
        self.settle();
        result
    }

    pub fn set_group_layout(
        &mut self,
        group: GroupId,
        tree: crate::layout::Tree,
    ) -> Result<(), ApplyError> {
        let result = self.engine.set_group_layout(group, tree);
        self.settle();
        result
    }

    pub fn remove_project(&mut self, project: ProjectId) -> io::Result<()> {
        let result = self.engine.remove_project(project);
        self.settle();
        result
    }

    pub fn rename_project(&mut self, project: ProjectId, title: &str) -> io::Result<()> {
        let result = self.engine.rename_project(project, title);
        self.settle();
        result
    }

    pub fn watch_memory(&mut self, sampler: Box<dyn RssSampler>, limit: u64) {
        self.engine.watch_memory(sampler, limit);
        self.settle();
    }

    pub fn sweep(&mut self) -> Vec<Restart> {
        let result = self.engine.sweep();
        self.settle();
        result
    }

    pub fn restart_sessions(&mut self, provider: Provider) {
        self.engine.restart_sessions(provider);
        self.settle();
    }

    pub fn open(&mut self, provider: Provider, workspace: WorkspaceChoice) -> io::Result<ThreadId> {
        let result = self.engine.open(provider, workspace);
        self.settle();
        result
    }

    pub fn bootstrap(
        &mut self,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
    ) -> io::Result<ThreadId> {
        let result = self.engine.bootstrap(choice, workspace, prompt);
        self.settle();
        result
    }

    pub fn bootstrap_in_group(
        &mut self,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
        group: GroupId,
    ) -> io::Result<ThreadId> {
        let result = self
            .engine
            .bootstrap_in_group(choice, workspace, prompt, group);
        self.settle();
        result
    }

    pub fn set_provider(
        &mut self,
        thread: ThreadId,
        choice: ProviderChoice,
    ) -> Result<(), ProvisionError> {
        let result = self.engine.set_provider(thread, choice);
        self.settle();
        result
    }

    pub fn project_id(&self, thread: ThreadId) -> Option<ProjectId> {
        self.engine.project_id(thread)
    }

    pub fn try_project_id(&self, thread: ThreadId) -> Result<Option<ProjectId>, LoadError> {
        self.engine.try_project_id(thread)
    }

    pub fn delete(&mut self, thread: ThreadId) -> Result<(), DeleteError> {
        let result = self.engine.delete(thread);
        self.settle();
        result
    }

    pub fn revive(&mut self, thread: ThreadId) -> Result<(), LoadError> {
        let result = self.engine.revive(thread);
        self.settle();
        result
    }

    pub fn revive_after(&mut self, thread: ThreadId, delay: Duration) -> Result<(), LoadError> {
        let result = self.engine.revive_after(thread, delay);
        self.settle();
        result
    }

    pub fn revive_group(&mut self, group: GroupId) -> Result<Vec<ThreadId>, ReviveGroupError> {
        let result = self.engine.revive_group(group);
        self.settle();
        result
    }

    pub fn import(&mut self, path: &Path) -> Result<ThreadId, crate::import::ImportError> {
        let result = self.engine.import(path);
        self.settle();
        result
    }

    pub fn send(&mut self, thread: ThreadId, text: String) {
        self.engine.send(thread, text);
        self.settle();
    }

    pub fn interrupt(&mut self, thread: ThreadId) {
        self.engine.interrupt(thread);
        self.settle();
    }

    pub fn respond(&mut self, thread: ThreadId, decision: &Decision, answer: DecisionAnswer) {
        self.engine.respond(thread, decision, answer);
        self.settle();
    }

    pub fn respond_decision(
        &mut self,
        thread: ThreadId,
        handle: &DecisionHandle,
        answer: DecisionAnswer,
    ) -> io::Result<bool> {
        let result = self.engine.respond_decision(thread, handle, answer);
        self.settle();
        result
    }

    pub fn apply_input(&mut self, thread: ThreadId, input: Input) {
        self.engine.apply_input(thread, input);
        self.settle();
    }

    pub fn search_files(
        &mut self,
        thread: ThreadId,
        query: &str,
    ) -> io::Result<Receiver<io::Result<Vec<crate::providers::FileSuggestion>>>> {
        let result = self.engine.search_files(thread, query);
        self.settle();
        result
    }

    pub fn control(&mut self, thread: ThreadId, action: crate::SessionControl) -> io::Result<()> {
        let result = self.engine.control(thread, action);
        self.settle();
        result
    }

    pub fn parked(&self) -> io::Result<Vec<ThreadId>> {
        self.engine.parked()
    }

    pub fn previously_open(&self) -> io::Result<Vec<ThreadId>> {
        self.engine.previously_open()
    }

    pub fn tracks_open_state(&self) -> bool {
        self.engine.tracks_open_state()
    }

    pub fn store_read_only(&self) -> Option<&str> {
        self.engine.store_read_only()
    }

    pub fn wants_worktree_listing(&self) -> bool {
        self.engine.wants_worktree_listing()
    }

    pub fn worktrees_listed(
        &mut self,
        thread: ThreadId,
        listed: Vec<PathBuf>,
        taken_at: Instant,
    ) -> bool {
        let result = self.engine.worktrees_listed(thread, listed, taken_at);
        self.settle();
        result
    }

    pub fn peek(&self, thread: ThreadId) -> Result<crate::store::ThreadMeta, LoadError> {
        self.engine.peek(thread)
    }

    pub fn subagent_count(&self, thread: ThreadId) -> Result<usize, LoadError> {
        self.engine.subagent_count(thread)
    }

    pub fn log_reader(&self) -> LogReader {
        self.engine.log_reader()
    }

    pub fn last_used(&self, thread: ThreadId) -> Option<std::time::SystemTime> {
        self.engine.last_used(thread)
    }

    pub fn threads(&self) -> Vec<ThreadId> {
        self.engine.threads()
    }

    pub fn last_worked_project(&self) -> Option<ProjectId> {
        self.engine.last_worked_project()
    }

    pub fn halt_sessions(&mut self) {
        self.engine.halt_sessions();
        self.settle();
    }

    pub fn thread_title(&self, thread: ThreadId) -> Result<Option<String>, LoadError> {
        self.engine.thread_title(thread)
    }

    pub fn display_title(&self, thread: ThreadId, auto: bool) -> String {
        self.engine.display_title(thread, auto)
    }

    pub fn rename_thread(&mut self, thread: ThreadId, title: &str) -> io::Result<()> {
        let result = self.engine.rename_thread(thread, title);
        self.settle();
        result
    }

    pub fn queue(&mut self, thread: ThreadId, text: String) -> bool {
        let result = self.engine.queue(thread, text);
        self.settle();
        result
    }

    pub fn unqueue(&mut self, thread: ThreadId) -> Option<String> {
        let result = self.engine.unqueue(thread);
        self.settle();
        result
    }

    pub fn cancel_queued(&mut self, thread: ThreadId, restore: bool) {
        self.engine.cancel_queued(thread, restore);
        self.settle();
    }

    pub fn edit_queued(&mut self, thread: ThreadId) {
        self.engine.edit_queued(thread);
        self.settle();
    }

    pub fn take_retrieved_prompt(&mut self, thread: ThreadId) -> Option<(String, bool)> {
        let result = self.engine.take_retrieved_prompt(thread);
        self.settle();
        result
    }

    pub fn recall_prompt(
        &mut self,
        thread: ThreadId,
        direction: HistoryDirection,
        current_draft: &str,
    ) -> Option<String> {
        let result = self.engine.recall_prompt(thread, direction, current_draft);
        self.settle();
        result
    }

    pub fn answer(&mut self, thread: ThreadId, id: &str) -> bool {
        let result = self.engine.answer(thread, id);
        self.settle();
        result
    }

    pub fn deliver_suggestion(&self, thread: ThreadId, text: String) {
        self.engine.deliver_suggestion(thread, text)
    }

    pub fn set_suggestions_enabled(&mut self, enabled: bool) {
        self.engine.set_suggestions_enabled(enabled);
        self.settle();
    }

    pub fn take_models_changed(&mut self) -> bool {
        self.shared.take_models_changed() | self.mirrors.take_models_changed()
    }

    pub fn rediscover_models(&mut self) {
        self.engine.rediscover_models();
        self.settle();
    }

    pub fn live_sessions(&self, provider: Provider) -> usize {
        self.shared.live_sessions(provider)
    }

    pub fn announced_models(&self, provider: Provider) -> Vec<ModelInfo> {
        self.shared.announced_models(provider)
    }

    pub fn account_limits(&self, provider: Provider) -> crate::transcript::RateLimits {
        self.shared.account_limits(provider)
    }

    pub fn set_allowance_thresholds(&mut self, thresholds: Vec<u8>) {
        self.engine.set_allowance_thresholds(thresholds);
        self.settle();
    }

    pub fn take_allowance_warnings(&mut self) -> Vec<crate::providers::limits::AllowanceWarning> {
        let result = self.engine.take_allowance_warnings();
        self.settle();
        result
    }

    pub fn discover_commands(
        &mut self,
        provider: Provider,
        cwd: &std::path::Path,
    ) -> Option<crate::providers::commands::Discovery> {
        let result = self.engine.discover_commands(provider, cwd);
        self.settle();
        result
    }

    pub fn model_catalog(&self, provider: Provider) -> Vec<ModelInfo> {
        crate::providers::models::catalog(provider, &self.announced_models(provider))
    }

    pub fn set_model(
        &mut self,
        thread: ThreadId,
        model: Option<String>,
    ) -> Result<(), ProvisionError> {
        let result = self.engine.set_model(thread, model);
        self.settle();
        result
    }

    pub fn set_effort(
        &mut self,
        thread: ThreadId,
        effort: Option<String>,
    ) -> Result<(), ProvisionError> {
        let result = self.engine.set_effort(thread, effort);
        self.settle();
        result
    }

    pub fn next_blocked(&self, from: Option<ThreadId>) -> Option<ThreadId> {
        self.engine.next_blocked(from)
    }

    pub fn needs_you(&self) -> Vec<ThreadId> {
        self.engine.needs_you()
    }

    pub fn answer_target(&self) -> Option<ThreadId> {
        self.engine.answer_target()
    }

    pub fn blocked(&self) -> Vec<ThreadId> {
        self.engine.blocked()
    }

    pub fn notifications(&self) -> &Notifications {
        &self.shared.notifications
    }

    pub fn set_notification_grace(&mut self, grace: Duration) {
        self.engine.set_notification_grace(grace);
        self.settle();
    }

    pub fn dismiss_notice(&mut self, id: NoticeId) -> bool {
        let result = self.engine.dismiss_notice(id);
        self.settle();
        result
    }

    pub fn dismiss_decision_notice(&mut self, id: &DecisionNoticeId) -> bool {
        let result = self.engine.dismiss_decision_notice(id);
        self.settle();
        result
    }

    pub fn clear_notices(&mut self) {
        self.engine.clear_notices();
        self.settle();
    }

    pub fn mark_notices_read(&mut self) {
        self.engine.mark_notices_read();
        self.settle();
    }

    pub fn fixture_backdate_notices(&mut self, thread: ThreadId, at: std::time::SystemTime) {
        self.engine.fixture_backdate_notices(thread, at);
        self.settle();
    }

    pub fn fixture_notice(&mut self, thread: ThreadId, outcome: crate::TurnOutcome) {
        self.engine.fixture_notice(thread, outcome);
        self.settle();
    }

    pub fn draft_starting(&self, draft: DraftId) -> bool {
        self.engine.draft_starting(draft)
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
        self.roster.visible(&self.shared.groups)
    }

    /// The one door to focus: every move — keys, clicks, nav rows — lands
    /// here, so fullscreen re-aims with focus. False for a Pane that is not
    /// open.
    pub fn focus(&mut self, identity: PaneIdentity) -> bool {
        let landed = self.roster.focus(identity);
        self.acknowledge_seen();
        landed
    }

    /// A running nav row's click (#21): land on that Thread's Pane, in the
    /// view that shows it — its Group, or Solo.
    pub fn focus_thread(&mut self, thread: ThreadId) -> bool {
        let identity = PaneIdentity::Thread(thread);
        if self.roster.index_of(identity).is_none() {
            return false;
        }
        self.roster.set_view(self.view_showing(thread));
        let landed = self.roster.focus(identity);
        self.acknowledge_seen();
        landed
    }

    /// The view that shows `thread` (R1, Groups are non-exclusive): the
    /// Group on screen when it holds the Thread, else the Thread's home
    /// Group, else Solo — a Thread that is only a guest of Groups lives
    /// loose.
    fn view_showing(&self, thread: ThreadId) -> View {
        match self.roster.view() {
            View::Group(group)
                if self
                    .shared
                    .groups
                    .get(group)
                    .is_some_and(|group| group.members.contains(&thread)) =>
            {
                View::Group(group)
            }
            _ => self
                .shared
                .groups
                .home(thread)
                .map_or(View::Solo, |group| View::Group(group.id)),
        }
    }

    /// Record the Subject the renderer is actually showing. Invalid or stale
    /// children fall back to Main, so a replacement Session cannot suppress a
    /// later Main request with an old selection.
    pub fn set_visible_subject(&mut self, thread: ThreadId, subject: Subject) {
        self.engine.set_visible_subject(thread, subject);
        self.acknowledge_seen();
    }

    /// cmd-] / cmd-[: walk the visible Panes, wrapping.
    pub fn step_focus(&mut self, delta: isize) {
        self.roster.step(delta, &self.shared.groups);
    }

    /// ⌘⇧⏎ (#20): the focused Pane takes the whole cockpit; ⌘⇧⏎ again
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
    /// Group — but entering spends no Session on a Thread the operator put
    /// away: a parked member's Pane is its parked tile (`Parked 2h ago ·
    /// 11 turns`), and ⏎ on it wakes it (`wake`). Focus stays put when the
    /// operator was already on a member, so entering from one of its own
    /// rows does not jump them.
    pub fn enter_group(&mut self, group: GroupId) -> Result<(), ReviveGroupError> {
        let members = self
            .shared
            .groups
            .get(group)
            .ok_or(ReviveGroupError::MissingGroup)?
            .members
            .clone();
        let retain = self
            .roster
            .focused_thread()
            .filter(|thread| members.contains(thread));
        for thread in &members {
            // Open or parked, the member holds its slot on the board.
            self.roster.insert_thread(*thread);
        }
        self.roster.set_view(View::Group(group));
        if let Some(thread) = retain.or_else(|| members.first().copied()) {
            self.roster.focus(PaneIdentity::Thread(thread));
        }
        Ok(())
    }

    /// ⏎ on a parked Group member's tile: revive it on a fresh Session in
    /// place — its Pane keeps the slot the board gave it, takes focus, and
    /// the park order forgets it. An open Thread is left alone.
    pub fn wake(&mut self, thread: ThreadId) -> Result<(), LoadError> {
        if self.engine.thread(thread).is_some() {
            return Ok(());
        }
        self.revive(thread)?;
        self.roster.note_revived(thread);
        self.roster.focus(PaneIdentity::Thread(thread));
        Ok(())
    }

    /// Revive one parked Thread into the view that shows it (#21): a Pane,
    /// focus, and the park order forgetting it — cmd-o must not revive it a
    /// second time. The shared tail of cmd-o and a parked nav row's click.
    pub fn reopen(&mut self, thread: ThreadId) -> Result<(), LoadError> {
        self.revive(thread)?;
        self.roster.set_view(self.view_showing(thread));
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

    /// cmd-t (#29): open a loose Draft and switch the Cockpit to Solo, even
    /// when the action is invoked from inside a Group. Nothing durable exists
    /// until the first send; the Draft takes focus.
    pub fn open_draft(&mut self) -> DraftId {
        let draft = self.roster.open_draft(DraftScope::default());
        self.roster.set_view(View::Solo);
        draft
    }

    /// Open a Draft in the current view. Shortcuts use this path so invoking
    /// one from a Group preserves that context and the Thread joins the Group
    /// after its first send succeeds.
    pub fn open_draft_in_current_view(&mut self) -> DraftId {
        let group = match self.roster.view() {
            View::Group(group) => Some(group),
            View::Solo => None,
        };
        self.roster.open_draft(DraftScope {
            group,
            new_group_with: None,
            pending_leave: None,
        })
    }

    /// Open a focused draft beside one loose Thread as a provisional pair.
    /// Its first successful send creates the durable Group.
    pub fn open_draft_for_new_group(&mut self, first: ThreadId) -> DraftId {
        self.roster.open_draft(DraftScope {
            group: None,
            new_group_with: Some(first),
            pending_leave: None,
        })
    }

    /// Re-aim the standing draft the way `open_draft` would have aimed a
    /// fresh one: loose, and the Cockpit back to Solo.
    pub fn aim_draft_loose(&mut self, draft: DraftId) {
        self.roster.rescope_draft(draft, None, None);
        self.roster.set_view(View::Solo);
    }

    /// Re-aim the standing draft at the current view — the Group the
    /// operator is looking at, or nothing when they are Solo.
    pub fn aim_draft_at_current_view(&mut self, draft: DraftId) {
        let group = match self.roster.view() {
            View::Group(group) => Some(group),
            View::Solo => None,
        };
        self.roster.rescope_draft(draft, group, None);
    }

    /// Re-aim the standing draft beside one loose Thread as a provisional
    /// pair, as `open_draft_for_new_group` would have.
    pub fn aim_draft_at_new_group(&mut self, draft: DraftId, first: ThreadId) {
        self.roster.rescope_draft(draft, None, Some(first));
    }

    /// The first send (#29): bootstrap the Thread — create, worktree, spawn
    /// — and only then let the prompt go; the Thread takes the draft's own
    /// slot, and joins the Group the draft was pending in. The leave the
    /// draft was holding open (see `close`) applies now, and only now: the
    /// new member is already in, so the pair never dissolves. On any
    /// failure nothing is half-born: no Thread, the Pane stays draft, and
    /// the prompt stays with the caller. `None` means startup is pending;
    /// the draft stays in place until `take_bootstrap_results` reports it.
    pub fn bootstrap_draft(
        &mut self,
        draft: DraftId,
        choice: ProviderChoice,
        workspace: WorkspaceChoice,
        prompt: &str,
        effort: Option<String>,
    ) -> io::Result<Option<Bootstrapped>> {
        if self.draft_starting(draft) {
            return Ok(None);
        }
        let scope = self
            .roster
            .draft_scope(draft)
            .ok_or_else(|| io::Error::other("no such draft"))?;
        let started = self
            .engine
            .bootstrap_draft(draft, scope, choice, workspace, prompt, effort);
        self.settle();
        Ok(started?.map(|thread| self.complete_draft(draft, thread)))
    }

    fn complete_draft(&mut self, draft: DraftId, thread: ThreadId) -> Bootstrapped {
        let scope = self
            .roster
            .draft_scope(draft)
            .expect("draft retained during startup");
        self.roster.draft_became(draft, thread);
        if scope.new_group_with.is_some() {
            if let Some(group) = self.shared.groups.home(thread).map(|group| group.id) {
                self.roster.set_view(View::Group(group));
                self.roster.focus(PaneIdentity::Thread(thread));
            }
        }
        let refused_leave = scope.pending_leave.and_then(|leaving| {
            self.apply_group(GroupChange::Leave { thread: leaving })
                .err()
        });
        Bootstrapped {
            thread,
            applied_leave: scope.pending_leave.is_some() && refused_leave.is_none(),
            refused_leave,
        }
    }

    /// Cancel a submitted startup before changing a draft's choices. The
    /// draft slot and Composer remain; cancellation emits no completion.
    pub fn cancel_draft_start(&mut self, draft: DraftId) -> io::Result<bool> {
        let cancelled = self.engine.cancel_draft_start(draft);
        self.settle();
        cancelled
    }

    pub fn take_bootstrap_results(&mut self) -> Vec<BootstrapResult> {
        self.collect_bootstraps();
        std::mem::take(&mut self.bootstrap_results)
    }

    /// The Engine's settled first sends, each draft's completed in this
    /// window: its slot becomes the Thread and its deferred leave applies.
    fn collect_bootstraps(&mut self) {
        for mut settled in self.engine.take_bootstrap_results() {
            if let (Some(draft), Ok(done)) = (settled.draft, &settled.result) {
                let thread = done.thread;
                settled.result = Ok(self.complete_draft(draft, thread));
            }
            self.bootstrap_results.push(settled);
        }
    }

    /// Discard a draft Pane (#29): nothing durable dies with it. A leave it
    /// was holding open applies now — nothing came to replace the leaving
    /// Thread, so the pair dissolves exactly as closing that Pane would
    /// have done without a draft in the way.
    pub fn discard_draft(&mut self, draft: DraftId) -> Result<(), ApplyError> {
        self.cancel_draft_start(draft)
            .map_err(|error| ApplyError::Io(error.to_string()))?;
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
            PaneIdentity::Thread(thread) => self.park_thread(thread),
        }
    }

    /// Park a Thread in one act, wherever it sits — cmd-w and every "Park
    /// thread". A Group member leaves its Group first (onto the ordinal
    /// survivor when its Group is on screen, deferred onto a pending draft
    /// in a pair), then parks: it lands in the parked rows at once, never
    /// as a loose open Thread that needs a second park.
    pub fn park_thread(&mut self, thread: ThreadId) -> Result<(), CloseError> {
        if self.shared.groups.of(thread).is_some() {
            // Leave takes the Thread out of every Group; when the Group on
            // screen held it, its ordinal survivor takes focus.
            let on_screen = matches!(
                self.roster.view(),
                View::Group(group) if self
                    .shared
            .groups
                    .get(group)
                    .is_some_and(|group| group.members.contains(&thread))
            );
            if on_screen {
                self.close_thread(thread)?;
            } else {
                self.apply_group(GroupChange::Leave { thread })
                    .map_err(CloseError::Group)?;
            }
        }
        self.park_noted(thread)
    }

    /// Close a Thread's Pane in the view it is in: in a Group that is a
    /// leave (the drag-out door, which follows the Thread to Solo), in Solo
    /// a park. cmd-w goes through `park_thread`, which always parks.
    fn close_thread(&mut self, thread: ThreadId) -> Result<(), CloseError> {
        if let View::Group(group) = self.roster.view() {
            let members = self
                .shared
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
                if let Some(group) = self.shared.groups.get(group) {
                    let next = group.members[ordinal.min(group.members.len() - 1)];
                    self.roster.focus(PaneIdentity::Thread(next));
                }
            }
            return Ok(());
        }
        self.park_noted(thread)
    }

    /// Solo's park. Parked even on a flush error — the Session is gone
    /// either way, so cmd-o should still bring this Thread back first —
    /// and the clamped survivor takes focus and, while fullscreen, the
    /// screen (#20).
    fn park_noted(&mut self, thread: ThreadId) -> Result<(), CloseError> {
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
        let outcome = match self.shared.groups.preview_drop(drag, target) {
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
                    || self.shared.groups.of(thread).is_none()
                {
                    self.roster.set_view(View::Solo);
                    self.roster.focus(PaneIdentity::Thread(thread));
                }
                closed
            }
            Plan::Change(change) => self.apply_group(change).map(|_| ()),
            Plan::Refused(_) | Plan::Nothing => Ok(()),
        };
        self.roster.heal_focus(&self.shared.groups);
        outcome
    }

    /// Adopt a CLI session file (#11), in place of the Pane it was picked
    /// for or, with none, alongside the Panes shown: a draft becomes the
    /// imported Thread; a still-blank Thread yields its slot and is deleted
    /// — clean exactly while it is blank, which the picker's own invariant
    /// guarantees and this re-checks. Import creates the Thread; revive
    /// opens it — the same replay-and-resume any parked Thread gets — and
    /// it takes focus. A refusal is the import module's, unchanged, and no
    /// Thread was created.
    pub fn adopt_into(
        &mut self,
        into: Option<PaneIdentity>,
        path: &Path,
    ) -> Result<Adopted, crate::import::ImportError> {
        let thread = self.import(path)?;
        let mut adopted = Adopted {
            thread,
            not_opened: None,
            blank_kept: None,
        };
        let Some(into) = into else {
            adopted.not_opened = self.reopen(thread).err();
            return Ok(adopted);
        };
        match self.revive(thread) {
            Ok(()) => {
                match into {
                    PaneIdentity::Draft(draft) => {
                        self.cancel_draft_start(draft)
                            .map_err(crate::import::ImportError::Io)?;
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
            .chain(
                order
                    .iter()
                    .filter(|thread| parked.contains(thread))
                    .copied(),
            )
            .collect())
    }
}
