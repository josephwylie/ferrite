//! One Pane as the Cockpit holds it (`CockpitPane`): its view, an entity
//! of its own, and what the Cockpit keeps beside it.

use std::collections::HashSet;

use ferrite_core::activity::{AgentKey, Subject};
use ferrite_core::roster::PaneIdentity;
use ferrite_core::ThreadKey;
use ferrite_pane::composer::Composer;
use ferrite_pane::pane::{self, PaneView};
use gpui::{AppContext as _, Entity, EntityId};

use super::beside;

/// One Pane as the Cockpit holds it: its view, and what the Cockpit keeps
/// beside it. Kept with the Pane, so a draft's first send — which makes the
/// Pane a Thread's — keeps it as it was.
///
/// The view is an entity of its own (ADR 0041), so a view other than the
/// root can hold the Pane and read it. Nothing notifies it: whatever
/// changes a Pane redraws what shows it, as before (`notify_part`, or the
/// root's notify). Read it with `read(cx)`, change it with `update`.
pub(crate) struct CockpitPane {
    /// Which Pane of the roster this is, as its view says: kept here too,
    /// so finding a Pane by it needs no read (`index_of`, `pane_for`).
    /// Only `new` and `adopt_thread` set it, and they set both.
    pub(crate) identity: PaneIdentity,
    view: Entity<PaneView>,
    /// The Review beside this Pane on the board (`beside.rs`).
    pub(crate) review: beside::ReviewSlot,
    /// The Pane's transcripts whose events the Cockpit hears. Each is
    /// subscribed to once, the first time it is drawn, and forgotten when
    /// the Pane lets it go; they all go with the Pane.
    heard: HashSet<EntityId>,
}

impl CockpitPane {
    pub(super) fn new(view: PaneView, cx: &mut gpui::App) -> Self {
        Self {
            identity: view.identity,
            view: cx.new(|_| view),
            review: beside::ReviewSlot::new(cx),
            heard: HashSet::new(),
        }
    }

    /// The Thread this Pane shows, or None while it is still a draft.
    pub(crate) fn thread(&self) -> Option<ThreadKey> {
        self.identity.thread()
    }

    /// The Pane's Composer, to change or to hand on with `cx`.
    pub(crate) fn composer(&self, cx: &gpui::App) -> Entity<Composer> {
        self.view.read(cx).composer.clone()
    }

    /// The Pane's preview, to change or to hand on with `cx`.
    pub(crate) fn preview(&self, cx: &gpui::App) -> ferrite_pane::preview::Preview {
        self.view.read(cx).preview.clone()
    }

    /// Change the draft's choices, while it is one; `None` once it is a
    /// Thread's.
    pub(crate) fn update_draft<R>(
        &self,
        cx: &mut gpui::App,
        change: impl FnOnce(&mut pane::DraftBinding) -> R,
    ) -> Option<R> {
        self.view.update(cx, |view, _| view.draft_mut().map(change))
    }

    /// Whether `transcript` is new to the Cockpit, to be subscribed to
    /// now: each is heard once.
    pub(super) fn hear(&mut self, transcript: EntityId) -> bool {
        self.heard.insert(transcript)
    }

    /// Core evicted `subject`'s transcript: the Pane lets it go, and it is
    /// no longer heard.
    pub(super) fn release_transcript(&mut self, subject: &Subject, cx: &mut gpui::App) {
        if let Some(transcript) = self.view.update(cx, |view, _| view.release_transcript(subject)) {
            self.heard.remove(&transcript.entity_id());
        }
    }

    /// The provider said `from` is `to`: the Pane keeps one Subject of
    /// them, and a transcript it lets go is no longer heard.
    pub(super) fn redirect_subject(&mut self, from: &AgentKey, to: &AgentKey, cx: &mut gpui::App) {
        if let Some(dropped) = self.view.update(cx, |view, _| view.redirect_subject(from, to)) {
            self.heard.remove(&dropped.entity_id());
        }
    }

    /// Whether the Cockpit hears `transcript`'s events.
    #[cfg(test)]
    pub(crate) fn hears(&self, transcript: EntityId) -> bool {
        self.heard.contains(&transcript)
    }

    /// The draft's first send made a Thread of it (`PaneView::adopt_thread`).
    pub(super) fn adopt_thread(&mut self, thread: ThreadKey, cx: &mut gpui::App) {
        self.view.update(cx, |view, _| view.adopt_thread(thread));
        self.identity = PaneIdentity::Thread(thread);
    }
}

impl std::ops::Deref for CockpitPane {
    type Target = Entity<PaneView>;
    fn deref(&self) -> &Entity<PaneView> {
        &self.view
    }
}
