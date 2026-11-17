//! Thread context has separate inline and popover visibility. Wide canvases
//! reserve a conversation lane beside the persistent card; narrow canvases
//! expose the same card through its title bar control.

mod content;

use super::*;
use ferrite_pane::details::{Density, Standing};
use ferrite_ui::motion;
use ferrite_ui::theme::*;

/// The corner the card hangs from, which it grows and shrinks about.
const DETAILS_ORIGIN: motion::ScaleFrom = (1.0, 0.0);

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
        let Some((thread, pane)) = self.focused_details() else {
            return;
        };
        if self.details_inline(pane) {
            self.apply_surface(thread, surfaces::SurfaceAction::ToggleDetails);
            cx.notify();
            return;
        }
        if self.panes[pane].details.is_open() {
            self.close_details(thread, window, cx);
            return;
        }
        let card = &self.panes[pane].details;
        card.set_open(true);
        window.focus(&card.focus, cx);
        self.refresh_context(thread);
        cx.notify();
    }

    pub(super) fn dismiss_details(
        &mut self,
        _: &CloseDetails,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((thread, _)) = self.focused_details() {
            self.close_details(thread, window, cx);
        }
    }

    /// Shut `thread`'s card; the keyboard goes back to its Composer if the
    /// card held it, and stays where the press put it otherwise.
    pub(super) fn close_details(
        &mut self,
        thread: ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.panes.iter().find(|pane| pane.thread() == Some(thread)) else {
            return;
        };
        if !pane.details.is_open() {
            return;
        }
        pane.details.set_open(false);
        if pane.details.focus.contains_focused(window, cx) {
            window.focus(&pane.composer.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// Inline context requires a readable conversation lane and 160px of usable height.
    pub(super) fn details_inline(&self, index: usize) -> bool {
        self.panes[index].details.canvas.get().is_some_and(|card| {
            let current = f32::from(card.size.width);
            let fit = self.panes[index].details.fit_width.get().unwrap_or(current);
            fit >= 280. + 12. + 32. + 48. + 640.
                && card.size.height >= px(160. + 24. + self.panes[index].details.top_inset.get())
        })
    }

    pub(super) fn details_lane_width(&self, index: usize) -> f32 {
        if self.details_inline(index) && self.details_inline_open(index) {
            let width = self.panes[index]
                .details
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
        let previous_rest = self.nav_measure_rest.replace(true);
        let targets = self.pane_rects(window);
        self.nav_measure_rest.set(previous_rest);
        let current_rects = self.pane_rects(window);
        for (index, target) in targets {
            let pane = &self.panes[index];
            let current = current_rects
                .iter()
                .find(|(pane, _)| *pane == index)
                .map_or(target.w, |(_, rect)| rect.w);
            let width = current.min(target.w);
            let fit = if self.columns.moving() {
                pane.details
                    .fit_width
                    .get()
                    .map_or(width, |held| held.min(width))
            } else {
                width
            };
            pane.details.fit_width.set(Some(fit));
        }
        let focused = self.focused();
        let stale: Vec<ThreadId> = self
            .panes
            .iter()
            .enumerate()
            .filter(|(index, pane)| *index != focused && pane.details.is_open())
            .filter_map(|(_, pane)| pane.thread())
            .collect();
        for thread in stale {
            self.close_details(thread, window, cx);
        }
    }

    fn focused_details(&self) -> Option<(ThreadId, usize)> {
        let index = self.focused();
        let thread = self.panes.get(index)?.thread()?;
        Some((thread, index))
    }

    pub(super) fn details_open(&self) -> bool {
        self.focused_details().is_some_and(|(_, index)| {
            if self.details_inline(index) {
                self.details_inline_open(index)
            } else {
                self.panes[index].details.is_open()
            }
        })
    }

    fn details_inline_open(&self, index: usize) -> bool {
        self.panes[index]
            .thread()
            .and_then(|thread| self.panel.surfaces.get(thread))
            .is_none_or(|state| state.details_inline_open)
    }

    pub(super) fn details_hold_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.panes
            .get(self.focused())
            .is_some_and(|pane| pane.details.focus.contains_focused(window, cx))
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
        let canvas = pane.details.canvas.get()?;
        let find_open = pane
            .transcript()
            .is_some_and(|transcript| transcript.read(cx).find_open());
        pane.details.top_inset.set(if find_open {
            2.0 * FLOAT_OFFSET + FLOAT_ROW_H - DETAILS_GAP
        } else {
            0.
        });
        let inline = self.details_inline(index);
        if inline {
            pane.details.set_open(false);
        }
        let standing = if inline {
            if !self.details_inline_open(index) {
                return None;
            }
            Standing::Open
        } else {
            pane.details.standing(
                motion::DIALOG_OUT,
                cx.background_executor().now(),
                motion::reduced_motion(cx),
            )
        };
        if standing == Standing::Hidden {
            return None;
        }
        let top = if find_open {
            2.0 * FLOAT_OFFSET + FLOAT_ROW_H
        } else {
            DETAILS_GAP
        };
        let available = (f32::from(canvas.size.height) - top - DETAILS_GAP).max(0.);
        let content = self.details_content(thread);
        let width = (f32::from(canvas.size.width) - 2. * DETAILS_GAP).clamp(0., DETAILS_W);
        let measured = pane.details.measurements.clone();
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
        let scroll = pane.details.scroll.clone();
        let body = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .id(("details-body", thread.get() as usize))
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
            .id(("details-card", thread.get() as usize))
            .debug_selector(|| "details-card".into())
            .key_context("ThreadDetails")
            .track_focus(&pane.details.focus)
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
                        .is_some_and(|pane| pane.details.on_opener(event.position));
                    let choosing_items = view
                        .context_menu
                        .as_ref()
                        .is_some_and(|menu| menu.target == MenuTarget::Details(thread));
                    if !inline && !on_toggle && !choosing_items {
                        view.close_details(thread, window, cx);
                    }
                }),
            )
            .child(body);
        let card = match standing {
            Standing::Leaving(left) => {
                motion::float_out(left, motion::DIALOG_OUT, card, DETAILS_ORIGIN).into_any_element()
            }
            _ if inline => card.into_any_element(),
            _ => motion::dialog_in_casting(
                ("details-card-in", thread.get() as usize),
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

    pub(super) fn details_toggle(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let card = self.focused_details().map(|(_, index)| index);
        let door = crate::nav::icon_button(
            "details-toggle",
            ferrite_ui::icons::DETAILS,
            ICON_BUTTON_GLYPH,
            ICON_BUTTON,
            R_CONTROL,
        )
        .occlude()
        .when(self.details_open(), |door| door.bg(paint::SELECTED))
        .tooltip(ferrite_ui::menu::action_tooltip(
            "Toggle thread details",
            "cockpit::ToggleDetails",
        ))
        .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
            cx.stop_propagation();
            view.toggle_details_now(window, cx);
        }));
        let Some(index) = card else {
            return door;
        };
        let opener = self.panes[index].details.opener();
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
