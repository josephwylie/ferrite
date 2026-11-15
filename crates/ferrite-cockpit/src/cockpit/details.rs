//! The Thread details card (T3's `ThreadDetailsCard`, ADR 0039): the
//! Thread's lightweight context, a float over the top right of its
//! transcript, reachable whatever the right panel holds. It is the
//! panel's Overview (`panel_sections`) drawn compact, not a second source
//! of Thread context.
//!
//! Its visibility is the Pane's own (`ferrite_pane::details`): ⌘⇧B or its
//! title-bar toggle opens it and takes the keyboard; Escape or a press
//! outside it closes it and the Composer has the keyboard again. It opens
//! and leaves as a float does (`menu_in`, `float_out`), at `DETAILS_W`
//! and `DETAILS_GAP` from the edges, narrowing with a narrow Pane and
//! scrolling when the Thread has more to say than the Pane has height, and
//! hangs below the find bar while that is open.

use super::*;
use ferrite_pane::details::Standing;
use ferrite_ui::motion::{self, Opens};
use ferrite_ui::theme::*;

impl CockpitView {
    /// ⌘⇧B, the View menu, the palette and the title bar's toggle.
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
        let card = &self.panes[pane].details;
        card.set_open(true);
        window.focus(&card.focus, cx);
        self.refresh_context(thread);
        cx.notify();
    }

    /// Escape in the card.
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

    /// The focused Pane's Thread and index, when it can show a card.
    fn focused_details(&self) -> Option<(ThreadId, usize)> {
        let index = self.focused();
        let thread = self.panes.get(index)?.thread()?;
        Some((thread, index))
    }

    /// Whether the focused Pane's card is open (the toggle's pressed face).
    pub(super) fn details_open(&self) -> bool {
        self.focused_details()
            .is_some_and(|(_, index)| self.panes[index].details.is_open())
    }

    /// Whether a Pane's card holds the keyboard.
    pub(super) fn details_hold_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.panes
            .get(self.focused())
            .is_some_and(|pane| pane.details.focus.contains_focused(window, cx))
    }

    /// The card over `index`'s body: its Thread's name over the panel's
    /// sections, while it is open and while it leaves.
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
                    .child(self.panel_sections(thread, window, cx)),
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
            .child(body);
        let card = match standing {
            Standing::Leaving(left) => {
                motion::float_out(left, motion::DIALOG_OUT, card, (1.0, 0.0)).into_any_element()
            }
            _ => motion::menu_in("details-card-in", card, Opens::Down),
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

    /// The title bar's toggle for the card, pressed while it is open.
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
