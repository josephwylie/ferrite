//! Thread context has separate inline and popover visibility. Wide canvases
//! reserve a conversation lane beside the persistent card; narrow canvases
//! expose the same card through its title bar control.

pub(super) mod content;
mod lineage;

use super::*;
use ferrite_ui::theme::*;

/// How far left of the board's right edge (`board`) a toast moves so it is
/// on none of `cards` (each one's left and right), nearest the edge first.
fn toast_inset(mut cards: Vec<(f32, f32)>, board: f32) -> f32 {
    cards.sort_by(|a, b| b.1.total_cmp(&a.1));
    let home = board - TOAST_BOX_RIGHT;
    let mut right = home;
    for (left, card_right) in cards {
        if card_right > right - TOAST_W && left < right {
            right = left - TOAST_BOX_RIGHT;
        }
    }
    home - right
}

impl CockpitView {
    pub(super) fn toggle_details(
        &mut self,
        _: &ToggleDetails,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_details_now(window, cx);
    }

    pub(super) fn toggle_details_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((thread, pane)) = self.focused_details(cx) else {
            return;
        };
        if self.details_inline(pane, cx) {
            self.panes[pane].read(cx).details.reset_pages();
            self.apply_surface(thread, surfaces::SurfaceAction::ToggleDetails);
            cx.notify();
            return;
        }
        if self.panes[pane].read(cx).details.is_open() {
            self.close_details(thread, window, cx);
            return;
        }
        let card = &self.panes[pane].read(cx).details;
        card.set_open(true);
        let focus = card.focus.clone();
        window.focus(&focus, cx);
        self.refresh_context(thread, cx);
        cx.notify();
    }

    pub(super) fn dismiss_details(
        &mut self,
        _: &CloseDetails,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((thread, _)) = self.focused_details(cx) {
            self.close_details(thread, window, cx);
        }
    }

    /// Shut `thread`'s card; the keyboard goes back to its Composer if the
    /// card held it, and stays where the press put it otherwise.
    pub(super) fn close_details(
        &mut self,
        thread: ThreadKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.panes.iter().find(|pane| pane.thread() == Some(thread)) else {
            return;
        };
        if !pane.read(cx).details.is_open() {
            return;
        }
        pane.read(cx).details.set_open(false);
        if pane.read(cx).details.focus.contains_focused(window, cx) {
            window.focus(&pane.read(cx).composer.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// Inline context requires a readable conversation lane and 160px of usable height.
    pub(super) fn details_inline(&self, index: usize, cx: &gpui::App) -> bool {
        ferrite_pane::details::inline(&self.panes[index].read(cx).details)
    }

    /// A card open in a Pane that no longer holds the focus shuts: Escape
    /// only reaches the focused Pane's card.
    pub(super) fn tend_details(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let targets = self.columns.measured_at_rest(|| self.pane_rects(window, cx));
        let current_rects = self.pane_rects(window, cx);
        for (index, target) in targets {
            let pane = &self.panes[index];
            let current = current_rects
                .iter()
                .find(|(pane, _)| *pane == index)
                .map_or(target.w, |(_, rect)| rect.w);
            let width = current.min(target.w);
            let fit = if self.columns.moving() {
                pane.read(cx).details
                    .fit_width
                    .get()
                    .map_or(width, |held| held.min(width))
            } else {
                width
            };
            pane.read(cx).details.fit_width.set(Some(fit));
        }
        let focused = self.focused(cx);
        let stale: Vec<ThreadKey> = self
            .panes
            .iter()
            .enumerate()
            .filter(|(index, pane)| *index != focused && pane.read(cx).details.is_open())
            .filter_map(|(_, pane)| pane.thread())
            .collect();
        for thread in stale {
            self.close_details(thread, window, cx);
        }
    }

    fn focused_details(&self, cx: &gpui::App) -> Option<(ThreadKey, usize)> {
        let index = self.focused(cx);
        let thread = self.panes.get(index)?.thread()?;
        Some((thread, index))
    }

    pub(super) fn details_open(&self, cx: &gpui::App) -> bool {
        self.focused_details(cx)
            .is_some_and(|(_, index)| self.details_standing(index, cx))
    }

    /// Whether Pane `index` has a card to draw: only a Pane showing a
    /// Thread does.
    fn details_standing(&self, index: usize, cx: &gpui::App) -> bool {
        if self.panes[index].thread().is_none() {
            return false;
        }
        if self.details_inline(index, cx) {
            self.details_inline_open(index)
        } else {
            self.panes[index].read(cx).details.is_open()
        }
    }

    /// How much further in from the board's right edge a toast stands to
    /// clear the Details cards open on the board.
    pub(super) fn details_toast_inset(&self, window: &Window, cx: &gpui::App) -> f32 {
        let cards = self
            .pane_rects(window, cx)
            .into_iter()
            .filter(|(index, _)| self.details_standing(*index, cx))
            .map(|(_, pane)| {
                let right = pane.x + pane.w - DETAILS_GAP;
                let width = (pane.w - 2. * DETAILS_GAP).clamp(0., DETAILS_W);
                (right - width, right)
            })
            .collect();
        let board = self.board_bounds(window, cx);
        toast_inset(cards, board.x + board.w)
    }

    pub(super) fn details_inline_open(&self, index: usize) -> bool {
        self.panes[index]
            .thread()
            .and_then(|thread| self.panel.surfaces.get(thread))
            .is_none_or(|state| state.details_inline_open)
    }

    pub(super) fn details_hold_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.panes
            .get(self.focused(cx))
            .is_some_and(|pane| pane.read(cx).details.focus.contains_focused(window, cx))
    }

    pub(super) fn details_toggle(&self, cx: &mut Context<Self>) -> gpui::component::button::Button {
        use ferrite_ui::components::{bound_chord, icon_button_as, IconSize, IconState, Tip};
        let card = self.focused_details(cx).map(|(_, index)| index);
        let door = icon_button_as(
            "details-toggle",
            ferrite_ui::icons::DETAILS,
            IconSize::Standard,
            "Toggle thread details",
            IconState::Toggle(self.details_open(cx)),
            cx,
        )
        .occlude()
        .tip_with_key("Toggle thread details", bound_chord("cockpit::ToggleDetails"))
        .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
            cx.stop_propagation();
            view.toggle_details_now(window, cx);
        }));
        let Some(index) = card else {
            return door;
        };
        let opener = self.panes[index].read(cx).details.opener();
        ferrite_ui::components::on_bounds(door, move |bounds, _, _| opener.set(Some(bounds)))
    }

    pub(super) fn details_menu_rows(&self) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let prefs = &self.prefs.settings.context_card_preferences;
        let mut rows: Vec<_> = ferrite_core::settings::ContextCardItem::ALL
            .into_iter()
            .map(|item| {
                Some((
                    menu::Item::new(item.label()).checked(prefs.visible(item)),
                    MenuVerb::DetailsItem(item),
                ))
            })
            .collect();
        rows.push(None);
        rows.push(Some((
            menu::Item::new("Reset to default").disabled(prefs.is_default()),
            MenuVerb::DetailsReset,
        )));
        rows
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A toast stands left of the card in its way, and of the next one its
    /// move puts it on; a card it does not reach leaves it where it was.
    #[test]
    fn a_toast_stands_clear_of_the_details_cards() {
        let board = 1440.;
        assert_eq!(toast_inset(vec![], board), 0.);
        assert_eq!(toast_inset(vec![(100., 380.)], board), 0., "out of its way");
        let card = (1148., 1428.);
        assert_eq!(toast_inset(vec![card], board), 292.);
        assert_eq!(toast_inset(vec![(100., 380.), card], board), 292.);
        assert_eq!(toast_inset(vec![(700., 980.), card], board), 740.);
    }
}
