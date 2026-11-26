//! The Cockpit's cached parts: its nav (`NavView`, ADR 0041) and each Pane,
//! each drawn by a view of its own and rebuilt only when something it shows
//! changes.
//!
//! A Pane's view (`PartView`) reads what its cell draws of the window
//! through `Source` (`cell_inputs`), then draws the cell by calling back
//! into the Cockpit (`render_part`), whose listeners are still the
//! Cockpit's own; the nav draws only what it last read of the Cockpit
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
//! Under `FERRITE_LOOPS_OVERLAY=0` (no cached Cockpit) the Panes' and the
//! nav's views are mounted uncached.

use std::collections::HashMap;

use ferrite_core::roster::PaneIdentity;
use gpui::{
    div, prelude::*, px, AnyElement, AnyView, Context, Div, Entity, StyleRefinement, WeakEntity,
    Window,
};

use super::pane_cell::{
    CellAsk, CellInputs, ControlsInputs, DraftInputs, KindInputs, ThreadInputs,
};
use super::CockpitView;
use ferrite_ui::source::Source;

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
    /// What the cell draws of the window (`CockpitView::cell_inputs`).
    inputs: Source<CellInputs, CellAsk>,
}

impl Render for PartView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // This part's loops are laid out afresh for the loops overlay.
        ferrite_ui::loops_overlay::begin(window, cx.entity_id());
        let ask = CellAsk {
            pane: self.pane,
            viewport: window.viewport_size(),
        };
        let Some(inputs) = self.inputs.read(&ask, cx) else {
            return div().into_any_element();
        };
        self.cockpit
            .update(cx, |view, cx| view.render_part(inputs, window, cx))
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
        let root = cx.entity();
        let inputs = Source::of(&root, |root: &Self, ask: &CellAsk, cx| root.cell_inputs(ask, cx));
        let cockpit = root.downgrade();
        let view = cx.new(|_| PartView { cockpit, pane, inputs });
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

    /// A Pane's cell on the board: its part, cached with parts on.
    pub(super) fn pane_slot(&self, index: usize, _window: &mut Window, cx: &mut Context<Self>) -> Div {
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
        if !self.parts.borrow().on {
            return frame.child(view);
        }
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

    /// What a Pane's view draws: its cell, from `inputs`.
    fn render_part(&self, inputs: CellInputs, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(index) = self.index_of(inputs.identity) else {
            return div().into_any_element();
        };
        let pane: Entity<ferrite_pane::pane::PaneView> = (*self.panes[index]).clone();
        let model = self.cockpit.clone();
        let details = |thread, subject: &ferrite_core::activity::Subject, pages, cx: &gpui::App| {
            Some(self.details_content(thread, subject, pages, cx))
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(super::pane_cell::cell(inputs, &pane, &model, &details, window, cx))
            .into_any_element()
    }

    /// What Pane `ask.pane`'s cell draws of the window rather than of its
    /// Pane: read as the cell draws, under the conditions its draw read
    /// them, so the part redraws for what it drew and nothing else. None
    /// once the Pane is gone.
    pub(super) fn cell_inputs(&self, ask: &CellAsk, cx: &gpui::App) -> Option<CellInputs> {
        let index = self.index_of(ask.pane)?;
        let identity = self.panes[index].identity;
        let lost = self
            .pane_machine(index, cx)
            .filter(|machine| self.machine_lost(*machine, cx));
        let focused = index == self.focused(cx);
        let kind = match identity.thread() {
            None => KindInputs::Draft(self.draft_inputs(index, cx)),
            Some(thread) => KindInputs::Thread(
                self.cockpit
                    .read(cx)
                    .thread(thread)
                    .is_some()
                    .then(|| self.thread_inputs(index, thread, ask.viewport, cx)),
            ),
        };
        // A Thread the model has not got yet draws an empty shell.
        let drawn = !matches!(kind, KindInputs::Thread(None));
        let view = self.panes[index].read(cx);
        let controls = (drawn && view.is_main()).then(|| {
            let provider = match &kind {
                KindInputs::Draft(_) => view.draft().map(|draft| draft.binding.provider().provider),
                KindInputs::Thread(_) => identity
                    .thread()
                    .and_then(|thread| self.cockpit.read(cx).thread(thread))
                    .map(|open| open.provider()),
            };
            ControlsInputs {
                attach_focus: self.attach_menu_focus(identity),
                attach_place: match view.geometry.get().composer {
                    Some(field) => ferrite_ui::components::FloatPlace {
                        floor: f32::from(field.top()),
                        limit_right: f32::from(field.right()),
                    },
                    None => self.bar_place(cx),
                },
                usage_popover_held: self.usage_popover_held,
                picker_up: self.model_picker_open(identity),
                effort_default: provider
                    .and_then(|provider| self.prefs.settings.effort_for(provider))
                    .map(|effort| effort.to_string()),
            }
        });
        Some(CellInputs {
            index,
            identity,
            focused,
            lost,
            sheet_up: self.sheet_up(cx),
            drop_target: self.drop_target(index, cx),
            composer_menu: drawn.then(|| self.composer_menu_over(index, cx)).flatten(),
            triggers: self.popups.triggers(),
            controls,
            kind,
        })
    }

    /// What draft Pane `index`'s band and hero read of the window.
    fn draft_inputs(&self, index: usize, cx: &gpui::App) -> DraftInputs {
        let draft = self.panes[index]
            .read(cx)
            .draft()
            .expect("a Pane is a Thread or a draft");
        let project = self
            .cockpit
            .read(cx)
            .project(draft.binding.project())
            .map(|project| gpui::SharedString::from(project.title.clone()))
            .unwrap_or_else(|| "Project".into());
        let machine = draft.binding.project().machine();
        let environment = self.environment_chip_shown(index, cx).then(|| {
            if let Some(chip) = self.copy_chip(index, cx) {
                return chip;
            }
            let mark = if self.elsewhere(machine, cx) {
                ferrite_ui::icons::SERVER
            } else {
                ferrite_ui::icons::LAPTOP
            };
            (ferrite_pane::pane::ChipMark::Icon(mark), self.environment_name(machine, cx))
        });
        DraftInputs {
            project,
            machine: environment
                .is_none()
                .then(|| self.machine_label(machine, cx))
                .flatten(),
            environment,
            copy_live: self.waited_on(index, cx).map(|(_, copy)| copy.phase.live()),
            mode: self
                .prefs
                .settings
                .permission_mode_for(draft.binding.provider().provider)
                .to_string(),
        }
    }

    /// What Thread Pane `index`'s cell reads of the window, at a window of
    /// `viewport`'s size.
    fn thread_inputs(
        &self,
        index: usize,
        thread: ferrite_core::ThreadKey,
        viewport: gpui::Size<gpui::Pixels>,
        cx: &gpui::App,
    ) -> ThreadInputs {
        use ferrite_ui::theme;
        let board = self.board_rect(viewport, cx);
        let rect = self
            .pane_rects_in(board, cx)
            .into_iter()
            .find(|(at, _)| *at == index)
            .map(|(_, rect)| rect);
        // Below the short-Pane height a question body keeps two option
        // rows and scrolls, so its head and answer row stay in reach.
        let short = rect.is_some_and(|rect| rect.h < theme::DECISION_SHORT_PANE_H);
        let decisions = self.decision_cards(index, true, short, cx).map(|mut cards| {
            if let Some(rect) = rect {
                cards.inset = theme::tx_inset_at(rect.w, true);
            }
            cards
        });
        ThreadInputs {
            plane: self.on_plane(thread, cx),
            mode_menu_up: self.mode_menu().is_some_and(|menu| menu.thread == thread),
            facts: self
                .facts
                .get(thread)
                .map(|facts| (facts.project_label.clone(), facts.branch.clone())),
            decisions,
            details_inline_open: self.details_inline_open(index),
            // The slot's width as the board laid it out this frame (a
            // dragged seam included), else the Pane's rect's.
            #[cfg(test)]
            cell_width: self
                .slot_sizes
                .borrow()
                .get(&super::pane_leaf(self.panes[index].identity))
                .map(|(w, _)| *w)
                .or_else(|| rect.map(|rect| rect.w))
                .unwrap_or(board.w),
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
