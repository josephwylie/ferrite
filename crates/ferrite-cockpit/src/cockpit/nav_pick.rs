//! Picking cards in the nav (T3 Code's sidebar selection): ⌘-click toggles
//! a card into the pick and ⇧-click extends it from the last one picked,
//! in the order the nav draws its cards. Picked cards wear
//! `NAV_CARD_SELECTED`, and the context menu's Park takes them all. A plain
//! press, or a right click on a card outside the pick, clears it.
//!
//! **Press-and-sweep.** A press on a card's Park arms a sweep from that
//! card; dragging the pointer across other cards while it is held picks
//! every card between the first and the one under the pointer, and the
//! release parks them all. Released back on the first card, it parks that
//! one alone.

use super::nav_view::{NavEvent, NavView};
use super::*;

/// A Park press held down: the card it started on, and every card drawn
/// from there to the one under the pointer now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ParkSweep {
    from: ThreadId,
    swept: Vec<ThreadId>,
}

impl NavView {
    /// The cards `from..=to` spans in the nav's last drawing order, either
    /// way round; just `from` when either is not drawn. Read between
    /// frames only: a render is redrawing the order.
    fn drawn_span(&self, from: ThreadId, to: ThreadId) -> Vec<ThreadId> {
        let drawn = self.drawn.borrow();
        let at = |thread| drawn.iter().position(|drawn| *drawn == thread);
        match (at(from), at(to)) {
            (Some(a), Some(b)) => drawn[a.min(b)..=a.max(b)].to_vec(),
            _ => vec![from],
        }
    }

    /// Whether `thread`'s card wears the picked fill.
    pub(super) fn card_picked(&self, thread: ThreadId) -> bool {
        self.picked.contains(&thread)
            || self
                .park_sweep
                .as_ref()
                .is_some_and(|sweep| sweep.swept.contains(&thread))
    }

    /// What a menu opened on `thread` acts on: the whole pick when the
    /// card is in it, else the Thread alone.
    pub(super) fn menu_threads(&self, thread: ThreadId) -> Vec<ThreadId> {
        if self.picked.len() > 1 && self.picked.contains(&thread) {
            self.picked.clone()
        } else {
            vec![thread]
        }
    }

    /// A left press on a card: ⌘ toggles it in the pick and ⇧ extends the
    /// pick to it, and either answers `true` (the press is the pick's, the
    /// card does not open). A plain press clears the pick and answers
    /// `false`: the card opens.
    pub(super) fn pick_card(
        &mut self,
        thread: ThreadId,
        modifiers: gpui::Modifiers,
        cx: &mut Context<Self>,
    ) -> bool {
        if modifiers.platform {
            match self.picked.iter().position(|picked| *picked == thread) {
                Some(at) => {
                    self.picked.remove(at);
                }
                None => self.picked.push(thread),
            }
        } else if modifiers.shift {
            let anchor = self
                .picked
                .last()
                .copied()
                .or_else(|| self.inputs().and_then(|inputs| inputs.shown))
                .unwrap_or(thread);
            self.picked = self.drawn_span(anchor, thread);
        } else {
            if !self.picked.is_empty() {
                self.picked.clear();
                cx.notify();
            }
            return false;
        }
        cx.notify();
        true
    }

    /// A right press on a card outside the pick lets the pick go: the menu
    /// is that card's alone.
    pub(super) fn pick_for_menu(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if !self.picked.contains(&thread) && !self.picked.is_empty() {
            self.picked.clear();
            cx.notify();
        }
    }

    /// A press on `thread`'s Park arms the sweep from its card.
    pub(super) fn arm_park_sweep(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.park_sweep = Some(ParkSweep {
            from: thread,
            swept: vec![thread],
        });
        cx.notify();
    }

    /// The pointer, held down, over `thread`'s card: the sweep reaches it.
    pub(super) fn sweep_over(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(from) = self.park_sweep.as_ref().map(|sweep| sweep.from) else {
            return;
        };
        let swept = self.drawn_span(from, thread);
        if let Some(sweep) = self
            .park_sweep
            .as_mut()
            .filter(|sweep| sweep.swept != swept)
        {
            sweep.swept = swept;
            cx.notify();
        }
    }

    /// The release: every card the sweep spans parks.
    pub(super) fn finish_park_sweep(&mut self, cx: &mut Context<Self>) {
        let Some(sweep) = self.park_sweep.take() else {
            return;
        };
        cx.emit(NavEvent::Park(sweep.swept));
        cx.notify();
    }

    /// A card's pointer wiring, at every place the nav lists one: it joins
    /// the drawing order picks and sweeps span, and a held Park sweeps
    /// across it.
    pub(super) fn pickable_card(
        &self,
        card: Stateful<Div>,
        thread: ThreadId,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.drawn.borrow_mut().push(thread);
        card.on_mouse_move(cx.listener(move |nav, event: &MouseMoveEvent, _, cx| {
            if event.pressed_button == Some(MouseButton::Left) {
                nav.sweep_over(thread, cx);
            }
        }))
    }

    /// The nav's list catches a sweep's release wherever it lands.
    pub(super) fn sweep_release(
        &self,
        list: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        list.capture_any_mouse_up(cx.listener(|nav, event: &MouseUpEvent, _, cx| {
            if event.button == MouseButton::Left {
                nav.finish_park_sweep(cx);
            }
        }))
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|nav, _: &MouseUpEvent, _, cx| nav.finish_park_sweep(cx)),
        )
    }
}
