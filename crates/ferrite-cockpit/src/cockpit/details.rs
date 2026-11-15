//! The Thread details card (T3's `ThreadDetailsCard`, ADR 0039): the
//! Thread's key facts and its context at a glance, floating over the top
//! right of its transcript beside whatever the Panel holds. It draws the
//! Panel's own sections at `Depth::Summary`, so there is no second source
//! of Thread context, and one press opens the full Overview in the Panel.
//!
//! A Pane too narrow to float it (`DETAILS_MIN_PANE_W`) opens the Overview
//! in the Panel instead, unless the Panel already holds another tool.

use super::panel::Depth;
use super::surfaces::SurfaceId;
use super::*;
use ferrite_pane::details::Standing;
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
        if self.panes[pane].details.is_open() {
            self.close_details(thread, window, cx);
            return;
        }
        if !self.details_fit(pane) {
            self.open_panel(thread, cx);
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

    /// Whether Pane `index` floats the card. Only a narrow Pane whose Panel
    /// holds no other tool gives way to the Overview: the card's point is to
    /// sit beside Files, Browser or a PR, so it never replaces one.
    fn details_fit(&self, index: usize) -> bool {
        let wide = self.panes[index]
            .geometry
            .get()
            .card
            .is_none_or(|card| card.size.width >= px(DETAILS_MIN_PANE_W));
        wide || self
            .shown_surface(index)
            .is_some_and(|shown| shown != SurfaceId::Overview)
    }

    /// A card open in a Pane that no longer holds the focus shuts: Escape
    /// only reaches the focused Pane's card.
    pub(super) fn tend_details(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        self.focused_details()
            .is_some_and(|(_, index)| self.panes[index].details.is_open())
    }

    pub(super) fn details_hold_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.panes
            .get(self.focused())
            .is_some_and(|pane| pane.details.focus.contains_focused(window, cx))
    }

    /// The card over `index`'s body: its Thread's name over the panel's
    /// summary and the way to the full Overview, while it is open and
    /// while it leaves.
    pub(super) fn details_card(
        &self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pane = self.panes.get(index)?;
        let thread = pane.thread()?;
        let standing = pane.details.standing(
            motion::DIALOG_OUT,
            cx.background_executor().now(),
            motion::reduced_motion(cx),
        );
        if standing == Standing::Hidden {
            return None;
        }
        let find_open = pane
            .transcript()
            .is_some_and(|transcript| transcript.read(cx).find_open());
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
                    .px(px(FLOAT_PAD_X))
                    .pt(px(FLOAT_PAD))
                    .child(self.panel_sections(thread, Depth::Summary, window, cx)),
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
            .max_w_full()
            .max_h_full()
            .min_w_0()
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .on_mouse_down_out(
                cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                    let on_toggle = view
                        .panes
                        .iter()
                        .find(|pane| pane.thread() == Some(thread))
                        .is_some_and(|pane| pane.details.on_opener(event.position));
                    if !on_toggle {
                        view.close_details(thread, window, cx);
                    }
                }),
            )
            .child(
                ferrite_ui::menu::head(self.facts.name(thread).clone())
                    .debug_selector(|| "details-head".into())
                    .px(px(FLOAT_PAD_X)),
            )
            .child(body)
            .child(
                ferrite_ui::menu::footer_shell()
                    .id(("details-overview", thread.get() as usize))
                    .debug_selector(|| "details-overview".into())
                    .cursor_pointer()
                    .hover(|style| style.text_color(rgb(INK_STRONG)))
                    .child("Open overview")
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        view.close_details(thread, window, cx);
                        view.open_panel(thread, cx);
                    })),
            );
        let card = match standing {
            Standing::Leaving(left) => {
                motion::float_out(left, motion::DIALOG_OUT, card, DETAILS_ORIGIN).into_any_element()
            }
            _ => motion::dialog_in_casting(
                ("details-card-in", thread.get() as usize),
                card,
                ferrite_ui::components::float_shadow,
                DETAILS_ORIGIN,
            )
            .into_any_element(),
        };
        let top = if find_open {
            2.0 * FLOAT_OFFSET + FLOAT_ROW_H
        } else {
            DETAILS_GAP
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
}
