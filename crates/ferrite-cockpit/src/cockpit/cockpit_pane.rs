//! One Pane as the Cockpit holds it (`CockpitPane`): its view, an entity
//! of its own, and what the Cockpit keeps beside it.

use ferrite_core::roster::PaneIdentity;
use ferrite_core::ThreadKey;
use ferrite_pane::composer::Composer;
use ferrite_pane::pane::{self, PaneView};
use gpui::{AppContext as _, Entity};

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
}

impl CockpitPane {
    pub(super) fn new(view: PaneView, cx: &mut gpui::App) -> Self {
        Self {
            identity: view.identity,
            view: cx.new(|_| view),
            review: beside::ReviewSlot::new(cx),
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
