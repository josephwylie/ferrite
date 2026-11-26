//! What is unsent outlives its Pane and the window: a Thread's draft and a
//! new Pane's, kept as they are written (`ferrite_core::drafts`) and put
//! back when the Pane opens.

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
