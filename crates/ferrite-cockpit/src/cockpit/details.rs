//! Thread context has separate inline and popover visibility. Wide canvases
//! reserve a conversation lane beside the persistent card; narrow canvases
//! expose the same card through its title bar control.

mod content;
mod lineage;

use super::*;
use ferrite_pane::details::{Density, Standing};
use ferrite_ui::motion;
use ferrite_ui::theme::*;

/// The corner the card hangs from, which it grows and shrinks about.
const DETAILS_ORIGIN: motion::ScaleFrom = (1.0, 0.0);

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
        self.panes[index].read(cx).details.canvas.get().is_some_and(|card| {
            let current = f32::from(card.size.width);
            let fit = self.panes[index].read(cx).details.fit_width.get().unwrap_or(current);
            fit >= 280. + 12. + 32. + 48. + 640.
                && card.size.height >= px(160. + 24. + self.panes[index].read(cx).details.top_inset.get())
        })
    }

    pub(super) fn details_lane_width(&self, index: usize, cx: &gpui::App) -> f32 {
        if self.details_inline(index, cx) && self.details_inline_open(index) {
            let width = self.panes[index]
                .read(cx).details
                .canvas
                .get()
                .map_or(0., |card| f32::from(card.size.width));
            let lane_right = width - DETAILS_W - DETAILS_GAP - 32.;
            let chat_width = TX_COLUMN_MAX_W.min(lane_right - 48.);
            let left = ((width - chat_width) / 2.)
                .min(lane_right - chat_width)
                .max(48.);
            (width - 2. * left - chat_width).max(0.)
        } else {
            0.
        }
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

    fn details_inline_open(&self, index: usize) -> bool {
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

    /// Draw the thread's context using the canvas placement and available density.
    pub(super) fn details_card(
        &self,
        index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pane = self.panes.get(index)?;
        let thread = pane.thread()?;
        let canvas = pane.read(cx).details.canvas.get()?;
        let find_open = pane
            .read(cx).transcript()
            .is_some_and(|transcript| transcript.read(cx).find_open());
        pane.read(cx).details.top_inset.set(if find_open {
            2.0 * FLOAT_OFFSET + FLOAT_ROW_H - DETAILS_GAP
        } else {
            0.
        });
        let inline = self.details_inline(index, cx);
        if inline {
            pane.read(cx).details.set_open(false);
        }
        let standing = if inline {
            if self.details_inline_open(index) {
                Standing::Open
            } else {
                Standing::Hidden
            }
        } else {
            pane.read(cx).details.standing(
                motion::DIALOG_OUT,
                cx.background_executor().now(),
                motion::reduced_motion(cx),
            )
        };
        if standing == Standing::Hidden {
            pane.read(cx).details.drawn.set(None);
            return None;
        }
        let top = if find_open {
            2.0 * FLOAT_OFFSET + FLOAT_ROW_H
        } else {
            DETAILS_GAP
        };
        let available = (f32::from(canvas.size.height) - top - DETAILS_GAP).max(0.);
        let content =
            self.details_content(thread, &pane.read(cx).selected, pane.read(cx).details.previous_pages(), cx);
        let width = (f32::from(canvas.size.width) - 2. * DETAILS_GAP).clamp(0., DETAILS_W);
        let measured = pane.read(cx).details.measurements.clone();
        measured.borrow_mut().prepare(
            content.key(thread, width, Density::Full),
            content.key(thread, width, Density::Compact),
        );
        let density = measured.borrow().density(available);
        let key = content.key(thread, width, density);
        let scrolling_height = content.scrolling_height(density);
        let sections = ferrite_ui::components::on_bounds(
            div()
                .relative()
                .debug_selector(move || {
                    format!(
                        "details-density-{}",
                        match density {
                            Density::Full => "full",
                            Density::Compact => "compact",
                            Density::Essential => "essential",
                        }
                    )
                })
                .child(content.render(thread, density, cx)),
            move |bounds, _, cx| {
                let height = (f32::from(bounds.size.height) - scrolling_height + 2.).max(0.);
                if measured.borrow_mut().record(density, &key, height) {
                    cx.refresh_windows();
                }
            },
        );
        let scroll = pane.read(cx).details.scroll.clone();
        let body = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .id(("details-body", thread.thread().get() as usize))
                    .debug_selector(|| "details-body".into())
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&scroll)
                    .text_size(px(13.))
                    .font_weight(W_LABEL)
                    .child(sections),
            )
            .child(ferrite_ui::components::scrollbar_layer().child(
                ferrite_ui::components::scrollbar_bar("details-scrollbar", &scroll),
            ));
        let card = ferrite_ui::menu::float()
            .id(("details-card", thread.thread().get() as usize))
            .debug_selector(|| "details-card".into())
            .key_context("ThreadDetails")
            .track_focus(&pane.read(cx).details.focus)
            .flex()
            .flex_col()
            .w(px(DETAILS_W))
            .rounded(px(16.))
            .bg(rgb(0x111111))
            .border_color(rgba(0xffffff1a))
            .max_w_full()
            .max_h_full()
            .min_w_0()
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_context_menu(MenuTarget::Details(thread), event.position, cx);
                }),
            )
            .on_mouse_down_out(
                cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                    let on_toggle = view
                        .panes
                        .iter()
                        .find(|pane| pane.thread() == Some(thread))
                        .is_some_and(|pane| pane.read(cx).details.on_opener(event.position));
                    let choosing_items = view
                        .context_menu()
                        .is_some_and(|menu| menu.target == MenuTarget::Details(thread));
                    if !inline && !on_toggle && !choosing_items {
                        view.close_details(thread, window, cx);
                    }
                }),
            )
            .child(body);
        let drawn = pane.read(cx).details.drawn.clone();
        // The measure is inside the card's 1px edge.
        let card = ferrite_ui::components::on_bounds(card, move |bounds, _, _| {
            drawn.set(Some(bounds.dilate(px(1.))))
        });
        let card = match standing {
            Standing::Leaving(left) => {
                motion::float_out(left, motion::DIALOG_OUT, card, DETAILS_ORIGIN).into_any_element()
            }
            _ if inline => card.into_any_element(),
            _ => motion::dialog_in_casting(
                ("details-card-in", thread.thread().get() as usize),
                card,
                ferrite_ui::components::float_shadow,
                DETAILS_ORIGIN,
            )
            .into_any_element(),
        };
        Some(
            div()
                .absolute()
                .top(px(top))
                .bottom(px(DETAILS_GAP))
                .left(px(DETAILS_GAP))
                .right(px(DETAILS_GAP))
                .flex()
                .justify_end()
                .items_start()
                .child(card)
                .into_any_element(),
        )
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
