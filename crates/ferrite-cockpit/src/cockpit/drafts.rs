//! What is unsent outlives its Pane and the window: a Thread's draft and a
//! new Pane's, kept as they are written (`ferrite_core::drafts`) and put
//! back when the Pane opens.

use super::*;
use ferrite_core::draft_content::DraftContent;
use ferrite_core::drafts::Drafts;
use ferrite_core::roster::DraftId;

/// How long after the last keystroke the drafts are written: a burst of
/// typing is one write.
const SAVE_AFTER: Duration = Duration::from_secs(1);

pub(super) struct Unsent {
    drafts: Drafts,
    /// A write is already waiting on `SAVE_AFTER`.
    saving: bool,
    /// The draft a walk through prompt history started from.
    walk: Option<Walk>,
}

/// History hands back as text the draft it was given, so the typed parts of
/// that draft are kept here to come back when the walk reaches it again.
struct Walk {
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
        }
    }

    /// A history step is about to replace the Composer's `content`, whose
    /// text form is `draft`. Mid-walk, that is a prompt the walk showed and
    /// the draft it began from stays.
    pub(super) fn step_from(&mut self, draft: String, content: DraftContent) {
        if self
            .walk
            .as_ref()
            .is_some_and(|walk| walk.shown.as_deref() == Some(draft.as_str()))
        {
            return;
        }
        self.walk = Some(Walk {
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
        let content = pane.composer.read(cx).content();
        match (identity, pane.draft()) {
            (PaneIdentity::Thread(thread), _) => self.unsent.drafts.keep_thread(thread, content),
            (PaneIdentity::Draft(id), Some(draft)) => {
                self.unsent.drafts.keep_fresh(id, &draft.binding, content)
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

    /// `thread`'s Pane just opened: its Composer takes what it was left
    /// with.
    pub(super) fn restore_draft(
        &mut self,
        thread: ThreadId,
        composer: &Entity<Composer>,
        cx: &mut Context<Self>,
    ) {
        if let Some(content) = self.unsent.drafts.thread(thread).cloned() {
            composer.update(cx, |composer, cx| composer.load(content, cx));
        }
    }

    /// The launch's own Panes are up: reopen the new Panes the last launch
    /// left unsent, each aimed where it was, and leave the focus where the
    /// launch put it.
    pub(super) fn reopen_unsent_drafts(&mut self, cx: &mut Context<Self>) {
        let kept = self.unsent.drafts.take_fresh();
        if kept.is_empty() {
            return;
        }
        let focused = self.cockpit.roster().focused();
        for fresh in kept {
            let id = self.spawn_draft_pane(fresh.binding, DraftPlacement::Loose, cx);
            if let Some(pane) = self
                .panes
                .iter()
                .find(|pane| pane.identity.draft() == Some(id))
            {
                pane.composer
                    .update(cx, |composer, cx| composer.load(fresh.content, cx));
            }
        }
        if let Some(focused) = focused {
            self.cockpit.focus(focused);
            self.sync_panes(cx);
        }
    }

    /// The first send made a Thread of draft `id`: whatever its Composer
    /// still holds goes to the Thread's draft, and the new Pane's is gone.
    pub(super) fn promote_draft(&mut self, id: DraftId, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(pane) = self
            .panes
            .iter()
            .find(|pane| pane.identity == PaneIdentity::Thread(thread))
        else {
            return;
        };
        let content = pane.composer.read(cx).content();
        if self.unsent.drafts.promote(id, thread, content) {
            self.save_drafts_soon(cx);
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
            eprintln!("ferrite: drafts were not saved: {error}");
        }
    }
}
