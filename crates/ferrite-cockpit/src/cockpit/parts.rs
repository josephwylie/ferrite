//! The Cockpit's cached parts: its nav (`NavView`, ADR 0041) and each Pane,
//! each drawn by a view of its own and rebuilt only when something it shows
//! changes.
//!
//! A Pane's view (`PartView`) draws by calling back into the Cockpit
//! (`render_part`), so every element and listener in it is the Cockpit's
//! own, as before; the nav draws only what it last read of the Cockpit
//! (`NavView::look`), which it reads again as the Cockpit notifies or the
//! client's model says something `Arrived`, so it redraws only when that
//! moved. Each is mounted
//! cached and tracking what it reads (vendored gpui,
//! `ViewElement::tracking_reads`). That makes the Cockpit's own notify the
//! conservative case: every Pane's part read the Cockpit, so a
//! change anywhere in its state rebuilds them all, exactly as when they
//! were drawn inline. The
//! hot paths are precise instead (`notify_part`, `notify_frame`): a stream
//! delta rebuilds its Pane and the nav, a keystroke its Pane, a working
//! clock the Panes and nav rows that show it, a hover the part it is in.
//! The frame around the parts (titlebar, bottom bar, floats) is rebuilt
//! whenever any part is, without rebuilding the others.
//!
//! Under `FERRITE_LOOPS_OVERLAY=0` (no cached Cockpit) the Panes are drawn
//! inline and the nav's view is mounted uncached.

use std::collections::HashMap;

use ferrite_core::roster::PaneIdentity;
use gpui::{
    div, prelude::*, px, AnyElement, AnyView, Context, Div, Entity, StyleRefinement, WeakEntity,
    Window,
};

use super::CockpitView;

/// A part of the Cockpit drawn as a cached view of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Part {
    /// The nav's rows: the tree and the parked fold.
    Nav,
    /// One Pane's cell, wherever the board lays it out.
    Pane(PaneIdentity),
}

/// The view that draws one Pane's cell, by asking the Cockpit for it.
pub(crate) struct PartView {
    cockpit: WeakEntity<CockpitView>,
    pane: PaneIdentity,
}

impl Render for PartView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // This part's loops are laid out afresh for the loops overlay.
        ferrite_ui::loops_overlay::begin(window, cx.entity_id());
        let pane = self.pane;
        self.cockpit
            .update(cx, |view, cx| view.render_part(pane, window, cx))
            .unwrap_or_else(|_| div().into_any_element())
    }
}

/// The parts' views and what each was last mounted with.
#[derive(Default)]
pub(crate) struct Parts {
    /// Whether the nav and the Panes are cached parts: with the loops
    /// overlay, over a cached Cockpit.
    pub on: bool,
    views: HashMap<PaneIdentity, Entity<PartView>>,
}

impl Parts {
    /// The view drawing `pane`'s cell, once it has been mounted.
    pub(crate) fn view_id(&self, pane: PaneIdentity) -> Option<gpui::EntityId> {
        self.views.get(&pane).map(Entity::entity_id)
    }
}

impl CockpitView {
    /// Draw the nav and the Panes as cached parts (over a cached Cockpit),
    /// or inline. With parts, an animation the Cockpit draws itself (a
    /// float's entrance, a frame control's hover) redraws the frame around
    /// them, not every part (`motion::draw_frames_through`).
    pub(crate) fn set_parts(&mut self, on: bool, cx: &mut Context<Self>) {
        self.parts.borrow_mut().on = on;
        let view = cx.entity_id();
        ferrite_ui::motion::draw_frames_through(view, on.then(|| self.frame_tick_id()));
        if on {
            cx.on_release(move |_, _| ferrite_ui::motion::draw_frames_through(view, None))
                .detach();
        }
    }

    fn part_view(&self, pane: PaneIdentity, cx: &mut Context<Self>) -> Entity<PartView> {
        if let Some(view) = self.parts.borrow().views.get(&pane) {
            return view.clone();
        }
        let cockpit = cx.entity().downgrade();
        let view = cx.new(|_| PartView { cockpit, pane });
        self.parts.borrow_mut().views.insert(pane, view.clone());
        view
    }

    /// `frame`, holding a part's cached view: the frame is the box the
    /// part had in the Cockpit's layout when it was drawn inline, the view
    /// fills it.
    fn mount_part(frame: Div, view: AnyView) -> Div {
        frame.child(
            view.cached(StyleRefinement::default().size_full())
                .tracking_reads(),
        )
    }

    /// A Pane's cell on the board: its cached part, or the cell itself
    /// without parts.
    pub(super) fn pane_slot(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if !self.parts.borrow().on {
            return self.pane_cell(index, window, cx);
        }
        let identity = self.panes[index].identity;
        // The box `pane_content`'s cell has on the board.
        let frame = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0();
        let view = self.part_view(identity, cx);
        Self::mount_part(frame, view.into())
    }

    /// The nav's column of rows: its view, cached as a part with parts on.
    pub(super) fn nav_rows(&self) -> Div {
        let frame = div().flex_shrink_0().h_full().w(px(self.nav_expanded_width()));
        if !self.parts.borrow().on {
            return frame.child(self.nav_view.clone());
        }
        Self::mount_part(frame, self.nav_view.clone().into())
    }

    /// The toast float: its view, cached as a part with parts on, so a
    /// Cockpit redraw that moves nothing it draws leaves it. It takes no
    /// room: its toasts hang over the window from where they are anchored.
    pub(super) fn toast_float(&self) -> AnyElement {
        if !self.parts.borrow().on {
            return self.toasts.clone().into_any_element();
        }
        AnyView::from(self.toasts.clone())
            .cached(StyleRefinement::default().absolute().top_0().left_0())
            .tracking_reads()
            .into_any_element()
    }

    /// Forget the parts of Panes no longer open.
    pub(super) fn prune_parts(&self) {
        let mut parts = self.parts.borrow_mut();
        let open: Vec<PaneIdentity> = self.panes.iter().map(|pane| pane.identity).collect();
        parts.views.retain(|identity, _| open.contains(identity));
    }

    /// What a Pane's view draws.
    fn render_part(
        &mut self,
        pane: PaneIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.index_of(pane) {
            Some(index) => div()
                .size_full()
                .flex()
                .flex_col()
                .child(self.pane_cell(index, window, cx))
                .into_any_element(),
            None => div().into_any_element(),
        }
    }

    /// Redraw `part` and the frame around it — not the other parts. Without
    /// parts, the whole Cockpit. The nav looks again instead, and redraws
    /// if what it draws moved (`NavView::look`).
    pub(super) fn notify_part(&self, part: Part, cx: &mut Context<Self>) {
        let identity = match part {
            // The nav is always a view of its own.
            Part::Nav => return self.nav_view.update(cx, |nav, cx| nav.look_again(cx)),
            Part::Pane(identity) => identity,
        };
        let (on, view) = {
            let parts = self.parts.borrow();
            (parts.on, parts.views.get(&identity).cloned())
        };
        match view {
            Some(view) => view.update(cx, |_, cx| cx.notify()),
            // Never mounted: it renders afresh when it is.
            None if on => {}
            None => cx.notify(),
        }
    }

    /// Redraw the Pane showing `thread`, if one does.
    pub(super) fn notify_thread_pane(
        &self,
        thread: ferrite_core::ThreadKey,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.pane_for(thread) {
            self.notify_part(Part::Pane(self.panes[index].identity), cx);
        }
    }

    /// Redraw only the frame around the parts (the titlebar, the bottom
    /// bar, the floats). Without parts, the whole Cockpit.
    pub(super) fn notify_frame(&self, cx: &mut Context<Self>) {
        if self.parts.borrow().on {
            self.frame_tick.update(cx, |_, cx| cx.notify());
        } else {
            cx.notify();
        }
    }

    /// The entity whose notify redraws only the frame: the Cockpit reads it
    /// every render (`render_cockpit`), the parts never do.
    pub(super) fn frame_tick_id(&self) -> gpui::EntityId {
        self.frame_tick.entity_id()
    }
}

/// What `notify_frame` notifies: no state, read by the Cockpit's render.
pub(crate) struct FrameTick;
