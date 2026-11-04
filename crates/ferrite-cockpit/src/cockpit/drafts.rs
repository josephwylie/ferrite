//! A Thread's unsent line outlives its Pane and the window: kept as it is
//! typed (`ferrite_core::drafts`), put back when the Thread's Pane opens.

use super::*;
use ferrite_core::drafts::Drafts;

/// How long after the last keystroke the drafts are written: a burst of
/// typing is one write.
const SAVE_AFTER: Duration = Duration::from_secs(1);

pub(super) struct Unsent {
    drafts: Drafts,
    /// A write is already waiting on `SAVE_AFTER`.
    saving: bool,
}

impl Unsent {
    pub(super) fn load(dir: &std::path::Path) -> Self {
        Self {
            drafts: Drafts::load(dir),
            saving: false,
        }
    }
}

impl CockpitView {
    /// The Composer of `thread` changed: keep what it holds.
    pub(super) fn keep_draft(
        &mut self,
        thread: ThreadId,
        composer: &Entity<Composer>,
        cx: &mut Context<Self>,
    ) {
        let text = composer.read(cx).prompt();
        if !self.unsent.drafts.set(thread, &text) || self.unsent.saving {
            return;
        }
        self.unsent.saving = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_AFTER).await;
            this.update(cx, |view, _| view.save_drafts()).ok();
        })
        .detach();
    }

    /// `thread`'s Pane just opened: its Composer takes the line it was left
    /// with.
    pub(super) fn restore_draft(
        &mut self,
        thread: ThreadId,
        composer: &Entity<Composer>,
        cx: &mut Context<Self>,
    ) {
        if let Some(text) = self.unsent.drafts.get(thread) {
            let text = text.to_owned();
            composer.update(cx, |composer, cx| composer.set(text, cx));
        }
    }

    /// Write the drafts now (the wait over, or the window quitting).
    pub(super) fn save_drafts(&mut self) {
        self.unsent.saving = false;
        if let Err(error) = self.unsent.drafts.save(&self.prefs.dir) {
            eprintln!("ferrite: drafts were not saved: {error}");
        }
    }
}
