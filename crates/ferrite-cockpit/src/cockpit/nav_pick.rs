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

use super::*;

/// A Park press held down: the card it started on, and every card drawn
/// from there to the one under the pointer now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ParkSweep {
    from: ThreadId,
    swept: Vec<ThreadId>,
}

impl CockpitView {
    /// The cards `from..=to` spans in the nav's last drawing order, either
    /// way round; just `from` when either is not drawn. Read between
    /// frames only: a render is redrawing the order.
    fn drawn_span(&self, from: ThreadId, to: ThreadId) -> Vec<ThreadId> {
        let drawn = self.nav_drawn.borrow();
        let at = |thread| drawn.iter().position(|drawn| *drawn == thread);
        match (at(from), at(to)) {
            (Some(a), Some(b)) => drawn[a.min(b)..=a.max(b)].to_vec(),
            _ => vec![from],
        }
    }

    /// Whether `thread`'s card wears the picked fill.
    pub(super) fn card_picked(&self, thread: ThreadId) -> bool {
        self.nav_picked.contains(&thread)
            || self
                .park_sweep
                .as_ref()
                .is_some_and(|sweep| sweep.swept.contains(&thread))
    }

    /// What a menu opened on `thread` acts on: the whole pick when the
    /// card is in it, else the Thread alone.
    pub(super) fn menu_threads(&self, thread: ThreadId) -> Vec<ThreadId> {
        if self.nav_picked.len() > 1 && self.nav_picked.contains(&thread) {
            self.nav_picked.clone()
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
            match self.nav_picked.iter().position(|picked| *picked == thread) {
                Some(at) => {
                    self.nav_picked.remove(at);
                }
                None => self.nav_picked.push(thread),
            }
        } else if modifiers.shift {
            let anchor = self
                .nav_picked
                .last()
                .copied()
                .or(self.focused_thread())
                .unwrap_or(thread);
            self.nav_picked = self.drawn_span(anchor, thread);
        } else {
            if !self.nav_picked.is_empty() {
                self.nav_picked.clear();
                self.notify_part(parts::Part::Nav, cx);
            }
            return false;
        }
        self.notify_part(parts::Part::Nav, cx);
        true
    }

    /// A right press on a card outside the pick lets the pick go: the menu
    /// is that card's alone.
    pub(super) fn pick_for_menu(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if !self.nav_picked.contains(&thread) && !self.nav_picked.is_empty() {
            self.nav_picked.clear();
            self.notify_part(parts::Part::Nav, cx);
        }
    }

    /// A press on `thread`'s Park arms the sweep from its card.
    pub(super) fn arm_park_sweep(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.park_sweep = Some(ParkSweep {
            from: thread,
            swept: vec![thread],
        });
        self.notify_part(parts::Part::Nav, cx);
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
            self.notify_part(parts::Part::Nav, cx);
        }
    }

    /// The release: every card the sweep spans parks.
    pub(super) fn finish_park_sweep(&mut self, cx: &mut Context<Self>) {
        let Some(sweep) = self.park_sweep.take() else {
            return;
        };
        let threads = sweep.swept;
        self.nav_picked.retain(|picked| !threads.contains(picked));
        self.park_listed(&threads, cx);
        self.notify_part(parts::Part::Nav, cx);
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
        self.nav_drawn.borrow_mut().push(thread);
        card.on_mouse_move(cx.listener(move |view, event: &MouseMoveEvent, _, cx| {
            if event.pressed_button == Some(MouseButton::Left) {
                view.sweep_over(thread, cx);
            }
        }))
    }

    /// The nav's list catches a sweep's release wherever it lands.
    pub(super) fn sweep_release(
        &self,
        list: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        list.capture_any_mouse_up(cx.listener(|view, event: &MouseUpEvent, _, cx| {
            if event.button == MouseButton::Left {
                view.finish_park_sweep(cx);
            }
        }))
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|view, _: &MouseUpEvent, _, cx| view.finish_park_sweep(cx)),
        )
    }
}
