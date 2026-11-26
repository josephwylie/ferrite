//! The Cockpit's cached parts: its nav (`NavView`, ADR 0041) and each Pane,
//! each drawn by a view of its own and rebuilt only when something it shows
//! changes.
//!
//! A Pane's view (`ferrite_pane::cell::CellView`) reads what its cell draws
//! of the window through `Source` (`cell_inputs`, and the details card's
//! sections) and draws the cell in its own context, holding no handle to
//! the Cockpit. What a press in the cell asks of the window it emits as a
//! `PaneEvent` naming the Pane, which the Cockpit hears
//! (`on_pane_event`); the nav draws only what it last read of the Cockpit
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
    div, prelude::*, px, AnyElement, AnyView, Context, Div, Entity, StyleRefinement, Window,
};

use super::popups::Popup;
use super::surfaces::SurfaceAction;
use super::{CockpitView, MenuTarget, Submit};
use ferrite_pane::cell::{
    band_trigger, CellAsk, CellInputs, CellSlot, CellView, ControlsInputs, DetailsAsk, Doors, DraftInputs, KindInputs,
    PaneAsk, PaneEvent, QueuedAct, ThreadInputs, HERO_TRIGGER,
};
use ferrite_core::ThreadKey;
use ferrite_ui::components::MenuDoor;
use ferrite_ui::source::Source;
use gpui::{Focusable, SharedString};

/// A part of the Cockpit drawn as a cached view of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Part {
    /// The nav's rows: the tree and the parked fold.
    Nav,
    /// One Pane's cell, wherever the board lays it out.
    Pane(PaneIdentity),
}

/// The parts' views and what each was last mounted with, and what
/// notifies the frame around them alone.
pub(crate) struct Parts {
    /// Whether the nav and the Panes are cached parts: with the loops
    /// overlay, over a cached Cockpit.
    pub on: bool,
    views: HashMap<PaneIdentity, Entity<CellView>>,
    /// What `notify_frame` notifies with parts on: the Cockpit reads it
    /// every render (`render_cockpit`), the parts never do.
    frame_tick: Entity<FrameTick>,
}

impl Parts {
    /// No parts yet, drawn inline.
    pub(crate) fn new(cx: &mut gpui::App) -> Self {
        Self {
            on: false,
            views: HashMap::new(),
            frame_tick: cx.new(|_| FrameTick),
        }
    }

    /// The entity whose notify redraws only the frame.
    pub(crate) fn frame_tick(&self) -> &Entity<FrameTick> {
        &self.frame_tick
    }

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

    /// `pane`'s view, made the first time it is drawn: the Cockpit hears
    /// what its cell asks for as long as it lives.
    fn part_view(&self, pane: PaneIdentity, window: &Window, cx: &mut Context<Self>) -> Entity<CellView> {
        if let Some(view) = self.parts.borrow().views.get(&pane) {
            return view.clone();
        }
        let root = cx.entity();
        let inputs = Source::of(&root, |root: &Self, ask: &CellAsk, cx| root.cell_inputs(ask, cx));
        let details = Source::of(&root, |root: &Self, (thread, subject, pages): &DetailsAsk, cx| {
            Some(root.details_content(*thread, subject, *pages, cx))
        });
        let model = self.cockpit.clone();
        let view = cx.new(|_| CellView::new(pane, model, inputs, details));
        cx.subscribe_in(&view, window, Self::on_pane_event).detach();
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
    pub(super) fn pane_slot(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> Div {
        self.cell_slot(index, window, cx).draw()
    }

    /// Pane `index`'s cell as a slot lays it out, its view made the first
    /// time.
    pub(super) fn cell_slot(&self, index: usize, window: &Window, cx: &mut Context<Self>) -> CellSlot {
        let view = self.part_view(self.panes[index].identity, window, cx);
        CellSlot::new(view, self.parts.borrow().on)
    }

    /// Pane `index`'s cell as a slot lays it out, once its view is made:
    /// what a read of the root may hand a feature that lays it out.
    pub(super) fn made_cell_slot(&self, index: usize) -> Option<CellSlot> {
        let parts = self.parts.borrow();
        let view = parts.views.get(&self.panes[index].identity)?.clone();
        Some(CellSlot::new(view, parts.on))
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


    /// What Pane `event.pane`'s cell asks (`PaneAsk`): each the act its
    /// press made when the cell drew with the Cockpit's own listeners, run
    /// in the same effect cycle as the press, before the next frame. A
    /// Pane gone since the press asks nothing of its own.
    fn on_pane_event(
        &mut self,
        _: &Entity<CellView>,
        event: &PaneEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let identity = event.pane;
        // Native files and board slots speak for their drag, which may
        // have outlived the Pane under it.
        match event.ask.clone() {
            PaneAsk::FilesOver(over) => {
                if self.slot_drag.files_moved(identity, over) {
                    cx.notify();
                }
                return;
            }
            PaneAsk::FilesDropped => self.slot_drag.files_dropped(),
            PaneAsk::SlotDragged { source, at, bounds } => {
                return self.slot_dragged(source, super::pane_leaf(identity), at, bounds, cx);
            }
            PaneAsk::SlotDropped { source } => {
                return self.drop_pane(source, super::pane_leaf(identity), cx);
            }
            _ => {}
        }
        let Some(index) = self.index_of(identity) else {
            return;
        };
        let composer = self.panes[index].read(cx).composer.clone();
        match event.ask.clone() {
            PaneAsk::Pressed => self.pointer_down(index, cx),
            PaneAsk::FilesDropped => {
                self.focus_pane(index, cx);
                self.panes[index].update_draft(cx, |draft| draft.band_focus = None);
                self.close_composer_menu(cx);
                cx.notify();
            }
            PaneAsk::BandMenu { chip, from_hero } => {
                self.focus_pane(index, cx);
                self.open_band_menu(chip, from_hero, cx);
            }
            PaneAsk::Send => {
                self.focus_pane(index, cx);
                // A press on the circle sends, wherever tab left the ring.
                self.panes[index].update_draft(cx, |draft| draft.band_focus = None);
                window.focus(&composer.read(cx).focus_handle(cx), cx);
                self.submit(&Submit, window, cx);
            }
            PaneAsk::Stop => {
                self.focus_pane(index, cx);
                window.focus(&composer.read(cx).focus_handle(cx), cx);
                if let Some(thread) = identity.thread() {
                    self.cockpit.update(cx, |cockpit, _| cockpit.interrupt(thread));
                    self.facts.acted(self.cockpit.read(cx), thread);
                    cx.notify();
                }
            }
            PaneAsk::AttachMenu(true) => {
                self.focus_pane(index, cx);
                let focus = cx.focus_handle();
                self.show_popup(Popup::Attach { pane: identity, focus }, cx);
            }
            PaneAsk::AttachMenu(false) => {
                if self.attach_menu_focus(identity).is_some() {
                    self.close_popup(cx);
                }
            }
            PaneAsk::AttachPick(at) => {
                match at {
                    0 => self.attach_files(index, false, cx),
                    1 => self.attach_files(index, true, cx),
                    _ => composer.update(cx, |composer, cx| composer.insert("/", cx)),
                }
                cx.notify();
            }
            PaneAsk::PluginCommand(token) => {
                composer.update(cx, |composer, cx| {
                    composer.insert(&format!("{token} "), cx);
                    composer.stage_chip(SharedString::from(token), cx);
                });
                cx.notify();
            }
            PaneAsk::ModelPicker => {
                self.focus_pane(index, cx);
                if self.model_picker_open(identity) {
                    self.close_popup(cx);
                } else {
                    self.open_model_picker(identity, super::model_picker::Column::List, cx);
                }
            }
            ask => {
                if let Some(thread) = identity.thread() {
                    self.on_thread_pane_ask(index, thread, ask, window, cx);
                }
            }
        }
    }

    /// What Thread Pane `index`'s cell asks for `thread`: what only a
    /// Thread's cell draws.
    fn on_thread_pane_ask(
        &mut self,
        index: usize,
        thread: ThreadKey,
        ask: PaneAsk,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ask {
            PaneAsk::ContextMenu { at, copied } => {
                self.native_copy = copied;
                self.focus_pane(index, cx);
                self.open_context_menu(MenuTarget::Pane(thread), at, cx);
            }
            PaneAsk::StartFresh => self.start_fresh_conversation(index, thread, window, cx),
            PaneAsk::Queued { id, act } => {
                self.focus_pane(index, cx);
                let focus = self.panes[index].read(cx).composer.read(cx).focus_handle(cx);
                window.focus(&focus, cx);
                self.cockpit.update(cx, |cockpit, _| match act {
                    QueuedAct::SendNow => cockpit.send_queued_now(thread, &id),
                    QueuedAct::Edit => cockpit.edit_queued_item(thread, &id),
                    QueuedAct::Remove => cockpit.remove_queued_item(thread, &id, false),
                });
                self.facts.acted(self.cockpit.read(cx), thread);
                cx.notify();
            }
            PaneAsk::Connector { generation, act } => {
                match act {
                    Some(act) => self.run_session_control(thread, generation, act, cx),
                    None => self.open_panel(thread, cx),
                }
                cx.notify();
            }
            PaneAsk::ModeMenu { generation } => {
                self.focus_pane(index, cx);
                self.toggle_mode_menu(thread, generation, cx);
            }
            PaneAsk::DetailsMenu { at } => {
                self.open_context_menu(MenuTarget::Details(thread), at, cx);
            }
            PaneAsk::CloseDetails => self.close_details(thread, window, cx),
            PaneAsk::Surface(open) => self.upsert_surface(thread, open.into(), cx),
            PaneAsk::CreatePr => self.create_pr(thread, cx),
            PaneAsk::StopWatching { url } => self.watch_pr(thread, url, false, cx),
            PaneAsk::PreviousAgents => {
                self.apply_surface(thread, SurfaceAction::TogglePreviousAgents);
                cx.notify();
            }
            PaneAsk::MorePreviousAgents => {
                self.panes[index].read(cx).details.show_more();
                cx.notify();
            }
            PaneAsk::Subject { subject, parent: true } => {
                self.select_subject(thread, subject, window, cx);
            }
            PaneAsk::Subject { subject, parent: false } => self.open_subagent(thread, subject, cx),
            PaneAsk::Elsewhere {
                server,
                thread: there,
                delegator,
            } => match delegator {
                true => self.open_delegator(&server, there, cx),
                false => self.open_elsewhere(&server, there, cx),
            },
            PaneAsk::CancelDelegate(delegate) => self.cancel_delegate(thread, delegate, cx),
            // A draft's or a drag's, heard above.
            PaneAsk::Pressed
            | PaneAsk::FilesOver(_)
            | PaneAsk::FilesDropped
            | PaneAsk::SlotDragged { .. }
            | PaneAsk::SlotDropped { .. }
            | PaneAsk::BandMenu { .. }
            | PaneAsk::Send
            | PaneAsk::Stop
            | PaneAsk::AttachMenu(_)
            | PaneAsk::AttachPick(_)
            | PaneAsk::PluginCommand(_)
            | PaneAsk::ModelPicker => {}
        }
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
                effort_default: provider
                    .and_then(|provider| self.prefs.settings.effort_for(provider))
                    .map(|effort| effort.to_string()),
            }
        });
        Some(CellInputs {
            index,
            identity,
            view: (*self.panes[index]).clone(),
            focused,
            lost,
            sheet_up: self.sheet_up(cx),
            drop_target: self.slot_drag.hovered_by_files(self.panes[index].identity, cx),
            composer_menu: drawn
                .then(|| self.composer_menu_over(index, cx))
                .flatten()
                .map(AnyView::from),
            doors: self.cell_doors(identity, &kind, controls.is_some()),
            controls,
            kind,
        })
    }

    /// The doors of the menus Pane `identity`'s cell opens, for what it
    /// draws: a draft's band chips and hero word, the model chip of a Main
    /// Composer's `controls`, and a Thread's mode chip.
    fn cell_doors(&self, identity: PaneIdentity, kind: &KindInputs, controls: bool) -> Doors {
        use ferrite_pane::pane::BandChip;
        let triggers = self.popups.triggers();
        let door = |up: bool, key: gpui::SharedString| MenuDoor::new(up, triggers.record(key));
        let band = self
            .band_menu()
            .filter(|menu| menu.pane == identity)
            .map(|menu| (menu.kind.chip, menu.kind.from_hero));
        let draft = matches!(kind, KindInputs::Draft(_));
        let thread = matches!(kind, KindInputs::Thread(Some(_)));
        Doors {
            bands: match draft {
                true => [BandChip::Project, BandChip::Workspace, BandChip::Environment]
                    .into_iter()
                    .map(|chip| (chip, door(band == Some((chip, false)), band_trigger(chip))))
                    .collect(),
                false => Vec::new(),
            },
            hero: draft.then(|| door(band == Some((BandChip::Project, true)), HERO_TRIGGER.into())),
            model: controls.then(|| {
                door(self.model_picker_open(identity), ferrite_pane::cell::model_chip_trigger(identity))
            }),
            mode: identity.thread().filter(|_| thread).map(|thread| {
                door(
                    self.mode_menu().is_some_and(|menu| menu.thread == thread),
                    ferrite_pane::cell::mode_chip_trigger(thread),
                )
            }),
        }
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
        // The slot's width as the board laid it out this frame (a dragged
        // seam included), else the Pane's rect's: what the meter says the
        // cell was drawn at.
        #[cfg(test)]
        super::drawn::laid_out(
            self.panes[index].identity,
            self.slot_drag
                .size(super::pane_leaf(self.panes[index].identity))
                .map(|(w, _)| w)
                .or_else(|| rect.map(|rect| rect.w))
                .unwrap_or(board.w),
        );
        ThreadInputs {
            plane: self.on_plane(thread, cx),
            facts: self
                .facts
                .get(thread)
                .map(|facts| (facts.project_label.clone(), facts.branch.clone())),
            decisions,
            details_inline_open: self.details_inline_open(index),
            details_menu_up: self
                .context_menu()
                .is_some_and(|menu| menu.target == super::MenuTarget::Details(thread)),
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
        let tick = {
            let parts = self.parts.borrow();
            parts.on.then(|| parts.frame_tick.clone())
        };
        match tick {
            Some(tick) => tick.update(cx, |_, cx| cx.notify()),
            None => cx.notify(),
        }
    }

    /// The entity whose notify redraws only the frame: the Cockpit reads it
    /// every render (`render_cockpit`), the parts never do.
    pub(super) fn frame_tick_id(&self) -> gpui::EntityId {
        self.parts.borrow().frame_tick.entity_id()
    }
}

/// What `notify_frame` notifies: no state, read by the Cockpit's render.
pub(crate) struct FrameTick;
