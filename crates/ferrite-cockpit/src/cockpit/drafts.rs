//! What is unsent outlives its Pane and the window: a Thread's draft and a
//! new Pane's, kept as they are written (`ferrite_core::drafts`) and put
//! back when the Pane opens.
//!
//! A draft Pane opens here too (⌘T, ⌘⇧N, the palette's new thread in a
//! Project), and its first send starts its Thread (`bootstrap_draft`).

use super::*;
use ferrite_core::draft_content::DraftContent;
use ferrite_core::drafts::Drafts;
use ferrite_core::replies::Reply;
use ferrite_core::roster::DraftId;
use ferrite_core::ThreadKey;

/// How long after the last keystroke the drafts are written: a burst of
/// typing is one write.
const SAVE_AFTER: Duration = Duration::from_secs(1);

pub(super) struct Unsent {
    drafts: Drafts,
    /// A write is already waiting on `SAVE_AFTER`.
    saving: bool,
    /// The draft a walk through prompt history started from.
    walk: Option<Walk>,
    /// The new Panes that went with a machine the window let go, kept to
    /// reopen once it holds that machine again (`put_away_drafts`).
    away: Vec<DraftId>,
}

/// History hands back as text the draft it was given, so the typed parts of
/// that draft are kept here to come back when the walk reaches it again.
struct Walk {
    thread: ThreadKey,
    draft: String,
    content: DraftContent,
    /// The text the last step showed.
    shown: Option<String>,
}

impl Unsent {
    pub(super) fn load(dir: &std::path::Path) -> Self {
        Self {
            drafts: Drafts::load(dir),
            saving: false,
            walk: None,
            away: Vec::new(),
        }
    }

    /// A history step is about to replace the Composer's `content`, whose
    /// text form is `draft`. Mid-walk, that is a prompt the walk showed on
    /// this Thread and the draft it began from stays.
    pub(super) fn step_from(&mut self, thread: ThreadKey, draft: String, content: DraftContent) {
        if self.walk.as_ref().is_some_and(|walk| {
            walk.thread == thread && walk.shown.as_deref() == Some(draft.as_str())
        }) {
            return;
        }
        self.walk = Some(Walk {
            thread,
            draft,
            content,
            shown: None,
        });
    }

    /// The typed parts of `text`, which history just handed back.
    pub(super) fn stepped_to(&mut self, text: String) -> DraftContent {
        let Some(walk) = self.walk.as_mut() else {
            return DraftContent::from_prompt(text);
        };
        walk.shown = Some(text.clone());
        if walk.draft == text {
            walk.content.clone()
        } else {
            DraftContent::from_prompt(text)
        }
    }
}

impl CockpitView {
    /// The Composer of Pane `identity` changed: keep what it holds, and for
    /// a new Pane the choices it is aimed at.
    pub(super) fn keep_draft(&mut self, identity: PaneIdentity, cx: &mut Context<Self>) {
        if self.snapshot_draft(identity, cx) {
            self.save_drafts_soon(cx);
        }
    }

    /// `identity`'s Pane's draft as the store holds it now. Whether that
    /// changed anything.
    fn snapshot_draft(&mut self, identity: PaneIdentity, cx: &gpui::App) -> bool {
        let Some(pane) = self.panes.iter().find(|pane| pane.identity == identity) else {
            return false;
        };
        let content = pane.read(cx).composer.read(cx).content();
        match (identity, pane.read(cx).draft()) {
            (PaneIdentity::Thread(thread), _) => {
                let Some(thread) = self.cockpit.read(cx).saved(thread) else {
                    return false;
                };
                self.unsent.drafts.keep_thread(thread, content)
            }
            (PaneIdentity::Draft(id), Some(draft)) => {
                let cockpit = self.cockpit.read(cx);
                let Some(machine) = cockpit.saved_as(draft.binding.project().machine()) else {
                    return false;
                };
                let binding = draft.binding.saved(machine.clone());
                self.unsent.drafts.keep_fresh(id, &binding, content)
            }
            (PaneIdentity::Draft(_), None) => false,
        }
    }

    fn save_drafts_soon(&mut self, cx: &mut Context<Self>) {
        if self.unsent.saving {
            return;
        }
        self.unsent.saving = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_AFTER).await;
            this.update(cx, |view, cx| view.save_drafts(cx)).ok();
        })
        .detach();
    }

    /// `replies` went out on `thread`: their quotes keep the Blocks they
    /// were cited from, for the sent rows' Go to source.
    pub(super) fn keep_cited_sources(&mut self, thread: ThreadKey, replies: &[Reply], cx: &gpui::App) {
        let Some(saved) = self.cockpit.read(cx).saved(thread) else {
            return;
        };
        if self.cited_sources.record(&saved, replies) {
            if let Err(error) = self.cited_sources.save(&self.prefs.dir) {
                ferrite_core::log!("cited sources were not saved: {error}");
            }
        }
    }

    /// Delete each of `threads`, and with each the sources its quotes
    /// were cited from and, at the next frame, its Panel. `then` hears how
    /// each went once the Server has answered for all of them.
    pub(super) fn delete_threads(
        &mut self,
        threads: Vec<ThreadKey>,
        then: impl FnOnce(&mut Self, Vec<(ThreadKey, std::io::Result<()>)>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let asked = self.cockpit.update(cx, |cockpit, _| {
            threads.iter().map(|thread| cockpit.delete(*thread)).collect()
        });
        self.when_all_answered(
            asked,
            move |view, answers, cx| {
                let deleted: Vec<_> = threads.into_iter().zip(answers).collect();
                for (thread, _) in deleted.iter().filter(|(_, deleted)| deleted.is_ok()) {
                    view.panel.deleted.push(*thread);
                    let saved = view.cockpit.read(cx).saved(*thread);
                    if saved.is_some_and(|saved| view.cited_sources.forget(&saved)) {
                        if let Err(error) = view.cited_sources.save(&view.prefs.dir) {
                            ferrite_core::log!("cited sources were not saved: {error}");
                        }
                    }
                }
                then(view, deleted, cx);
            },
            cx,
        );
    }

    /// `thread`'s Pane just opened: its Composer takes what it was left
    /// with.
    pub(super) fn restore_draft(
        &mut self,
        thread: ThreadKey,
        composer: &Entity<Composer>,
        cx: &mut Context<Self>,
    ) {
        let saved = self.cockpit.read(cx).saved(thread);
        if let Some(content) = saved.and_then(|saved| self.unsent.drafts.thread(&saved).cloned()) {
            composer.update(cx, |composer, cx| composer.load(content, cx));
        }
    }

    /// The launch's own Panes are up: reopen the new Panes the last launch
    /// left unsent, each aimed where it was, and leave the focus where the
    /// launch put it. One whose machine the window does not hold has
    /// nowhere to open.
    pub(super) fn reopen_unsent_drafts(&mut self, cx: &mut Context<Self>) {
        let kept = self.unsent.drafts.take_fresh();
        if kept.is_empty() {
            return;
        }
        let focused = self.cockpit.read(cx).roster().focused();
        for fresh in kept {
            // One whose machine the window does not hold is dropped.
            let _ = self.reopen_draft(fresh, cx);
        }
        if let Some(focused) = focused {
            self.cockpit.update(cx, |cockpit, _| cockpit.focus(focused));
            self.sync_panes(cx);
        }
    }

    /// A new Pane as `fresh` kept it, aimed where it was, when the window
    /// holds that machine and Project: whether it did.
    fn reopen_draft(&mut self, fresh: ferrite_core::drafts::FreshDraft, cx: &mut Context<Self>) -> bool {
        let Some(project) = self
            .cockpit
            .read(cx)
            .resolve_project(fresh.binding.machine(), fresh.binding.project())
        else {
            return false;
        };
        let id = self.spawn_draft_pane(fresh.binding.bind(project), cx);
        if let Some(pane) = self
            .panes
            .iter()
            .find(|pane| pane.identity.draft() == Some(id))
        {
            pane.composer(cx)
                .update(cx, |composer, cx| composer.load(fresh.content, cx));
        }
        // Its binding is new to the window: the notify pass asks about its
        // checkout (`ask_draft_checkouts`), which a launch brings no other
        // notify for.
        cx.notify();
        true
    }

    /// The window let go of `gone`: each new Pane aimed at one of their
    /// Projects goes with its machine, as its Threads' Panes do, and is
    /// no draft to re-aim or send. Its words stay kept under that machine
    /// (the last `save_drafts` took them), to reopen when the window holds
    /// it again.
    pub(super) fn put_away_drafts(&mut self, gone: &[ferrite_core::MachineId], cx: &mut Context<Self>) {
        let away: Vec<DraftId> = self
            .panes
            .iter()
            .filter(|pane| {
                pane.read(cx).draft()
                    .is_some_and(|draft| gone.contains(&draft.binding.project().machine()))
            })
            .filter_map(|pane| pane.identity.draft())
            .collect();
        for draft in away {
            if let Err(error) = self.cockpit.update(cx, |cockpit, _| cockpit.discard_draft(draft)) {
                ferrite_core::log!("draft {draft:?} did not close cleanly: {error}");
            }
            self.unsent.away.push(draft);
        }
    }

    /// Machines let go are held again: each new Pane put away with one of
    /// them reopens, its words as they were.
    pub(super) fn reopen_put_away_drafts(&mut self, cx: &mut Context<Self>) {
        for draft in std::mem::take(&mut self.unsent.away) {
            // One that held no words has none to bring back.
            let Some(fresh) = self.unsent.drafts.fresh(draft).cloned() else {
                continue;
            };
            if self.reopen_draft(fresh, cx) {
                self.unsent.drafts.discard_fresh(draft);
            } else {
                self.unsent.away.push(draft);
            }
        }
    }

    /// Draft Pane `index` became `thread`'s: it keeps what the File
    /// Surface kept of its preview, and its Decision forms.
    pub(super) fn adopt_thread(&mut self, index: usize, thread: ThreadKey, cx: &mut Context<Self>) {
        let draft = self.panes[index].identity;
        self.panes[index].adopt_thread(thread, cx);
        self.panel.file_surface
            .update(cx, |files, _| files.adopt(draft, PaneIdentity::Thread(thread)));
        self.decisions
            .update(cx, |decisions, _| decisions.adopt(draft, PaneIdentity::Thread(thread)));
    }

    /// The first send made a Thread of draft `id`: whatever its Composer
    /// still holds goes to the Thread's draft, and the new Pane's is gone.
    pub(super) fn promote_draft(&mut self, id: DraftId, thread: ThreadKey, cx: &mut Context<Self>) {
        let Some(pane) = self
            .panes
            .iter()
            .find(|pane| pane.identity == PaneIdentity::Thread(thread))
        else {
            return;
        };
        let content = pane.read(cx).composer.read(cx).content();
        let Some(thread) = self.cockpit.read(cx).saved(thread) else {
            return;
        };
        if self.unsent.drafts.promote(id, thread, content) {
            self.save_drafts_soon(cx);
        }
    }

    /// New Pane `id` was closed unsent: what it held is not for the next
    /// launch, and the file says so now. A Thread's draft is not touched.
    pub(super) fn discard_draft(&mut self, id: DraftId, cx: &gpui::App) {
        if self.unsent.drafts.discard_fresh(id) {
            self.save_drafts(cx);
        }
    }

    /// Write the drafts now (the wait over, or the window quitting), the new
    /// Panes' choices as they stand.
    pub(super) fn save_drafts(&mut self, cx: &gpui::App) {
        self.unsent.saving = false;
        for identity in self
            .panes
            .iter()
            .filter(|pane| pane.identity.draft().is_some())
            .map(|pane| pane.identity)
            .collect::<Vec<_>>()
        {
            self.snapshot_draft(identity, cx);
        }
        if let Err(error) = self.unsent.drafts.save(&self.prefs.dir) {
            ferrite_core::log!("drafts were not saved: {error}");
        }
    }
}

impl CockpitView {
    /// The focused Pane's draft, to change — None on a Thread.
    /// Change the focused Pane's draft choices, while it is a draft.
    pub(super) fn update_focused_draft<R>(
        &self,
        cx: &mut gpui::App,
        change: impl FnOnce(&mut pane::DraftBinding) -> R,
    ) -> Option<R> {
        let focused = self.focused(cx);
        self.panes.get(focused)?.update_draft(cx, change)
    }

    /// cmd-t's answer (#29): a draft Pane — a Composer, the pre-prompt
    /// band, and nothing durable until the first send bootstraps a Thread.
    /// The provider and the project follow the Pane the operator is on;
    /// `target` is the caller's (cmd-shift-n drafts
    /// straight onto "new worktree").
    pub(super) fn open_draft(&mut self, target: DraftTarget, cx: &mut Context<Self>) {
        let provider = self
            .panes
            .get(self.focused(cx))
            .map(|pane| match pane.read(cx).draft() {
                Some(draft) => draft.binding.provider().clone(),
                None => {
                    let open = pane.thread().and_then(|thread| self.cockpit.read(cx).thread(thread));
                    ProviderChoice {
                        provider: open
                            .map_or(self.prefs.settings.default_provider, |open| open.provider()),
                        model: open.and_then(|open| open.model()).map(str::to_string),
                    }
                }
            })
            .unwrap_or_else(|| self.default_choice());
        self.open_draft_with_choice(target, provider, None, cx);
    }

    /// What a new Thread starts on when nothing on screen says otherwise:
    /// the settings' provider and its chosen model.
    pub(super) fn default_choice(&self) -> ProviderChoice {
        let provider = self.prefs.settings.default_provider;
        ProviderChoice {
            provider,
            model: self.prefs.settings.model_for(provider).map(str::to_string),
        }
    }

    pub(super) fn open_draft_with_provider(
        &mut self,
        target: DraftTarget,
        provider: Provider,
        cx: &mut Context<Self>,
    ) {
        self.open_draft_with_choice(
            target,
            ProviderChoice {
                provider,
                model: None,
            },
            None,
            cx,
        );
    }

    /// A new Thread in a named Project — the ⌘K palette's "new thread in
    /// <project>". It is a loose draft like every pointer-made one, and it
    /// moves the fallback Project, so the draft after it starts here too.
    pub(crate) fn open_draft_in_project(&mut self, project: ProjectKey, cx: &mut Context<Self>) {
        self.launch_project = Some(project);
        let provider = self.default_choice();
        self.open_draft_with_choice(
            DraftTarget::Main,
            provider,
            Some(project),
            cx,
        );
    }

    /// `chosen` is a Project the caller is naming outright, which outranks
    /// every guess below — the filter, the fallback.
    pub(super) fn open_draft_with_choice(
        &mut self,
        target: DraftTarget,
        provider: ProviderChoice,
        chosen: Option<ProjectKey>,
        cx: &mut Context<Self>,
    ) {
        if !self.cockpit.read(cx).environment_available() {
            return;
        }
        // The draft starts in the Project of the Thread the operator is in:
        // the focused one, else the one focus last left.
        let current = self
            .cockpit
            .read(cx)
            .roster()
            .current_thread()
            .and_then(|thread| self.cockpit.read(cx).project_of(thread));
        // A second press with a draft already up must not stack another:
        // focus the standing draft and re-aim it at the caller's target.
        // A draft mid-bootstrap is on its way out and does not count.
        if let Some(index) = self.panes.iter().position(|pane| {
            pane.identity
                .draft()
                .is_some_and(|draft| !self.cockpit.read(cx).draft_starting(draft))
        }) {
            // The standing draft is re-aimed at the Project as well: the
            // chip must not sit there naming the Project of an earlier
            // press while the operator is in another.
            let aim = chosen.or(current);
            let identity = self.panes[index].identity;
            self.panes[index].update_draft(cx, |draft| {
                draft.binding.choose_target(target);
                if let Some(aim) = aim {
                    draft.binding.choose_project(aim);
                    draft.error = None;
                }
            });
            self.cockpit.update(cx, |cockpit, _| cockpit.focus(identity));
            // The draft may be the Pane focused as the Factory or the PRs
            // opened, so focus never moves to leave them: give the board
            // back here.
            self.leave_destination();
            self.sync_panes(cx);
            cx.notify();
            return;
        }
        // A Project the caller named outright wins, then the current
        // Thread's. With no Thread to go by, a chosen nav filter names the
        // Project the operator is looking at, and only then does the draft
        // start where the last one ran.
        let project = chosen
            .or(current)
            .or(self.nav_filter)
            .or(self.launch_project);
        let Some(project) = project else {
            self.open_project_creator(cx);
            return;
        };
        let binding = ferrite_core::draft::DraftBinding::new(provider, project, target);
        self.spawn_draft_pane(binding, cx);
        cx.notify();
    }

    /// A draft Pane aimed at `binding`, focused. The one place a draft Pane
    /// is built, for a press of cmd-t and for a draft kept across a restart.
    pub(super) fn spawn_draft_pane(
        &mut self,
        binding: ferrite_core::draft::DraftBinding,
        cx: &mut Context<Self>,
    ) -> ferrite_core::roster::DraftId {
        let binding = pane::DraftBinding {
            binding,
            band_focus: None,
            error: None,
        };
        let machine = binding.binding.project().machine();
        let draft = self.cockpit.update(cx, |cockpit, _| cockpit.open_draft());
        let pane = CockpitPane::new(PaneView::new_draft(draft, binding, cx), cx);
        self.decisions
            .update(cx, |decisions, _| decisions.open_pane(pane.identity));
        pane.read(cx).preview.set_remote_files(self.elsewhere(machine, cx));
        self.aim_attachments(&pane.composer(cx), machine, cx);
        cx.subscribe(&pane.composer(cx), Self::composer_edited).detach();
        cx.subscribe(&pane.composer(cx), Self::composer_opens_source).detach();
            cx.subscribe(&pane.composer(cx), Self::composer_comment_opened).detach();
        self.panes.push(pane);
        self.sync_panes(cx);
        draft
    }

    /// Stop the submitted startup before Escape or a choice changes its draft.
    pub(super) fn cancel_focused_draft_start(&mut self, cx: &mut gpui::App) -> bool {
        let Some(id) = self
            .panes
            .get(self.focused(cx))
            .and_then(|pane| pane.identity.draft())
        else {
            return true;
        };
        match self.cockpit.update(cx, |cockpit, _| cockpit.cancel_draft_start(id)) {
            Ok(_) => true,
            Err(error) => {
                self.update_focused_draft(cx, |draft| draft.error = Some(error.to_string().into()));
                false
            }
        }
    }

    /// The first send (#29): resolve the draft's ids through the registry,
    /// then the core act — create, worktree, spawn, the prompt, and the
    /// Thread taking the draft's own slot; the band is gone for the life of
    /// the Thread. On any failure nothing is half-born: no Thread, the Pane
    /// stays draft, the error shows where the band is, and the prompt stays
    /// in the Composer.
    pub(super) fn bootstrap_draft(&mut self, cx: &mut Context<Self>) {
        let Some(pane) = self.panes.get(self.focused(cx)) else {
            return;
        };
        let (Some(draft), Some(id)) = (pane.read(cx).draft(), pane.identity.draft()) else {
            return;
        };
        let binding = draft.binding.clone();
        let composer = pane.read(cx).composer.clone();
        // A draft waiting on its copy runs where the copy goes, once it is in.
        if self.waited_on(self.focused(cx), cx).is_some() || !self.admit_files(self.focused(cx), cx) {
            return;
        }
        let text = composer.read(cx).prompt().trim().to_string();
        if text.is_empty() {
            return;
        }
        let provider = binding.provider().clone();
        let effort = binding.effort().map(str::to_owned);
        let machine = binding.project().machine();
        self.sync_setup_before_start(machine, cx);
        let resolved = binding
            .resolve(
                self.cockpit.read(cx).project(binding.project()),
                !self.elsewhere(machine, cx),
            )
            .map_err(|e| e.to_string());
        let opened = resolved.and_then(|choice| {
            self.cockpit
                .update(cx, |cockpit, _| {
                    cockpit.bootstrap_draft(machine, id, provider, choice, &text, effort.clone())
                })
                .map_err(|e| e.to_string())
        });
        self.finish_draft_start(id, &text, opened, cx);
        cx.notify();
    }

    pub(super) fn finish_draft_start(
        &mut self,
        id: ferrite_core::roster::DraftId,
        text: &str,
        opened: Result<Option<ferrite_client::cockpit::Bootstrapped>, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .panes
            .iter()
            .position(|pane| pane.identity.draft() == Some(id))
        else {
            return;
        };
        match opened {
            Ok(Some(done)) => {
                let composer = self.panes[index].read(cx).composer.clone();
                composer.update(cx, |composer, cx| {
                    // Edits made while startup ran belong to the operator.
                    if composer.prompt().trim() == text {
                        composer.take(cx);
                    }
                });
                self.adopt_thread(index, done.thread, cx);
                self.promote_draft(id, done.thread, cx);
                self.sync_panes(cx);
                self.thread_opened(done.thread, cx);
                self.refresh_names(cx);
                self.scroll_transcript_to_bottom(index, cx);
            }
            Ok(None) => {
                self.panes[index].update_draft(cx, |draft| draft.error = None);
            }
            Err(message) => {
                self.panes[index].update_draft(cx, |draft| draft.error = Some(SharedString::from(message)));
            }
        }
    }

    /// Test-only: aim the launch project at a scratch repo — production
    /// registers `here()` once at construction, which tests cannot sit in.
    #[cfg(test)]
    pub(super) fn aim_launch(&mut self, root: &std::path::Path, cx: &mut gpui::App) {
        self.launch_project = Some(
            self.cockpit
                .update(cx, |cockpit, _| {
                    let machine = cockpit.this_computer().expect("this computer is held");
                    cockpit.register_project(machine, root).now()
                })
                .expect("the scratch repo registers"),
        );
        let project = self.launch_project.expect("the test aimed it");
        for pane in &self.panes {
            pane.update_draft(cx, |draft| draft.binding.choose_project(project));
        }
    }

    /// cmd-shift-n: the same draft, aimed straight at "new worktree" —
    /// isolation from the operator's checkout is one chip already chosen.
    pub(super) fn new_worktree_thread(
        &mut self,
        _: &NewWorktreeThread,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_draft(DraftTarget::New, cx);
    }

    /// cmd-t / cmd-n (#29): a draft Pane, not a Thread. The band chooses;
    /// the first send bootstraps.
    pub(super) fn new_thread(&mut self, _: &NewThread, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_draft(DraftTarget::Main, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unsent() -> Unsent {
        Unsent::load(&std::env::temp_dir().join(format!("ferrite-walk-{}", std::process::id())))
    }

    /// A walk through one Thread's history never hands that Thread's draft
    /// parts to another Thread whose Composer happens to show the same text.
    #[test]
    fn a_walk_belongs_to_its_thread() {
        let (first, second) = (ThreadKey::fixture(0, 1), ThreadKey::fixture(0, 2));
        let mut unsent = unsent();
        let first_draft = DraftContent {
            text: "mine".into(),
            files: vec!["/tmp/a.png".into()],
            ..DraftContent::default()
        };
        unsent.step_from(first, "mine".into(), first_draft.clone());
        assert_eq!(unsent.stepped_to("older".into()).text, "older");

        let second_draft = DraftContent {
            text: "older".into(),
            ..DraftContent::default()
        };
        unsent.step_from(second, "older".into(), second_draft);
        assert_eq!(
            unsent.stepped_to("mine".into()),
            DraftContent::from_prompt("mine".into()),
            "the first Thread's files stay with it"
        );
    }

    /// Thread 3 here and Thread 3 on a machine are two Threads: a walk
    /// through one's history is not the other's.
    #[test]
    fn a_walk_keeps_two_machines_threads_apart() {
        let (here, there) = (ThreadKey::fixture(0, 3), ThreadKey::fixture(1, 3));
        let mut unsent = unsent();
        let mine = DraftContent {
            text: "mine".into(),
            files: vec!["/tmp/a.png".into()],
            ..DraftContent::default()
        };
        unsent.step_from(here, "mine".into(), mine);
        assert_eq!(unsent.stepped_to("older".into()).text, "older");

        unsent.step_from(there, "older".into(), DraftContent::from_prompt("older".into()));
        assert_eq!(
            unsent.stepped_to("mine".into()),
            DraftContent::from_prompt("mine".into()),
            "the walk here keeps its files from the Thread on the machine"
        );
    }
}
