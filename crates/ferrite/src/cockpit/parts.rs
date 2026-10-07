//! The Cockpit's cached parts: its nav and each Pane, each drawn by a view
//! of its own (`PartView`) and rebuilt only when something it shows
//! changes.
//!
//! A part's view draws by calling back into the Cockpit (`render_part`), so
//! every element and listener in it is the Cockpit's own, as before; it is
//! mounted cached and tracking what it reads (vendored gpui,
//! `ViewElement::tracking_reads`). That makes the Cockpit's own notify the
//! conservative case: every part read the Cockpit, so a change anywhere in
//! its state rebuilds them all, exactly as when they were drawn inline. The
//! hot paths are precise instead (`notify_part`, `notify_frame`): a stream
//! delta rebuilds its Pane and the nav, a keystroke its Pane, a working
//! clock the Panes and nav rows that show it, a hover the part it is in.
//! The frame around the parts (titlebar, bottom bar, floats) is rebuilt
//! whenever any part is, without rebuilding the others.
//!
//! Under `FERRITE_LOOPS_OVERLAY=0` (no cached Cockpit) the parts are drawn
//! inline, as before.

use std::collections::HashMap;

use ferrite_core::docview::Level;
use ferrite_core::roster::PaneIdentity;
use gpui::{
    div, prelude::*, px, AnyElement, AnyView, Context, Div, Entity, StyleRefinement, WeakEntity,
    Window,
};

use super::CockpitView;
use crate::nav;

/// A part of the Cockpit drawn as a cached view of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Part {
    /// The nav's rows: the needs-you strip, the tree and the parked fold.
    Nav,
    /// One Pane's cell, wherever the board lays it out.
    Pane(PaneIdentity),
}

/// The view that draws one part, by asking the Cockpit for it.
pub(crate) struct PartView {
    cockpit: WeakEntity<CockpitView>,
    part: Part,
}

impl Render for PartView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // This part's loops are laid out afresh for the loops overlay.
        crate::loops_overlay::begin(window, cx.entity_id());
        let part = self.part;
        self.cockpit
            .update(cx, |view, cx| view.render_part(part, window, cx))
            .unwrap_or_else(|_| div().into_any_element())
    }
}

/// The parts' views and what each was last mounted with.
#[derive(Default)]
pub(crate) struct Parts {
    /// Whether the nav and the Panes are cached parts: with the loops
    /// overlay, over a cached Cockpit.
    pub on: bool,
    views: HashMap<Part, Entity<PartView>>,
    /// The Level each Pane was last laid out at (the board's, one for all).
    levels: HashMap<PaneIdentity, Level>,
}

impl Parts {
    /// The view drawing `part`, once it has been mounted.
    pub(crate) fn view_id(&self, part: Part) -> Option<gpui::EntityId> {
        self.views.get(&part).map(Entity::entity_id)
    }
}

impl CockpitView {
    /// Draw the nav and the Panes as cached parts (over a cached Cockpit),
    /// or inline.
    pub(crate) fn set_parts(&mut self, on: bool) {
        self.parts.borrow_mut().on = on;
    }

    fn part_view(&self, part: Part, cx: &mut Context<Self>) -> Entity<PartView> {
        if let Some(view) = self.parts.borrow().views.get(&part) {
            return view.clone();
        }
        let cockpit = cx.entity().downgrade();
        let view = cx.new(|_| PartView { cockpit, part });
        self.parts.borrow_mut().views.insert(part, view.clone());
        view
    }

    /// `frame`, holding the part's cached view: the frame is the box the
    /// part had in the Cockpit's layout when it was drawn inline, the view
    /// fills it.
    fn mount_part(&self, part: Part, frame: Div, cx: &mut Context<Self>) -> Div {
        let view = self.part_view(part, cx);
        frame.child(
            AnyView::from(view)
                .cached(StyleRefinement::default().size_full())
                .tracking_reads(),
        )
    }

    /// A Pane's cell on the board, at `level`: its cached part, or the cell
    /// itself without parts.
    pub(super) fn pane_slot(
        &self,
        index: usize,
        level: Level,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        if !self.parts.borrow().on {
            return self.pane_cell(index, level, window, cx);
        }
        let identity = self.panes[index].identity;
        self.parts.borrow_mut().levels.insert(identity, level);
        // The box `pane_content`'s cell has on the board.
        let frame = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0();
        self.mount_part(Part::Pane(identity), frame, cx)
    }

    /// The nav's rows at the column's content width, at `opacity`: their
    /// cached part, or the rows themselves without parts.
    pub(super) fn nav_rows(&self, opacity: f32, cx: &mut Context<Self>) -> Div {
        if !self.parts.borrow().on {
            return self.nav_content(cx).opacity(opacity);
        }
        // The box `nav::content` has in the column.
        let frame = div()
            .flex_shrink_0()
            .h_full()
            .w(px(nav::WIDTH))
            .opacity(opacity);
        self.mount_part(Part::Nav, frame, cx)
    }

    /// Forget the parts of Panes no longer open.
    pub(super) fn prune_parts(&self) {
        let mut parts = self.parts.borrow_mut();
        let open: Vec<PaneIdentity> = self.panes.iter().map(|pane| pane.identity).collect();
        parts.views.retain(|part, _| match part {
            Part::Nav => true,
            Part::Pane(identity) => open.contains(identity),
        });
        parts.levels.retain(|identity, _| open.contains(identity));
    }

    /// What a part's view draws.
    fn render_part(
        &mut self,
        part: Part,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match part {
            Part::Nav => self.nav_content(cx).into_any_element(),
            Part::Pane(identity) => {
                let level = self.parts.borrow().levels.get(&identity).copied();
                match (self.index_of(identity), level) {
                    (Some(index), Some(level)) => div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .child(self.pane_cell(index, level, window, cx))
                        .into_any_element(),
                    _ => div().into_any_element(),
                }
            }
        }
    }

    /// Redraw `part` and the frame around it — not the other parts. Without
    /// parts, the whole Cockpit.
    pub(super) fn notify_part(&self, part: Part, cx: &mut Context<Self>) {
        let (on, view) = {
            let parts = self.parts.borrow();
            (parts.on, parts.views.get(&part).cloned())
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
        thread: ferrite_core::ThreadId,
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
