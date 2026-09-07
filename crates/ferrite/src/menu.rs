//! The context menu: what a right-click on a Thread, a Group, a Project
//! or a Pane offers. Drawing only, like `nav.rs` — the cockpit decides the
//! rows and runs the verbs. It floats on the menu ground with the same
//! two-layer shadow every popover wears, anchored at the pointer.
//!
//! A destructive verb never runs on one press: its row arms on the first
//! (the label becomes the confirmation, in the blocked ink) and runs on
//! the second. Anything else pressed disarms it.

use gpui::prelude::*;
use gpui::{div, point, px, rgb, rgba, BoxShadow, Div, SharedString, Stateful};

use crate::pointer::{Pointer, PointerPressed};
use crate::theme::{
    BLOCKED, BLOCKED_WASH, FS_MD, FS_MONO, MENU, MENU_PAD, MENU_ROW_H, R_CONTROL, R_MENU,
    SHADOW_FAR, SHADOW_FAR_BLUR, SHADOW_FAR_SPREAD, SHADOW_FAR_Y, SHADOW_NEAR, SHADOW_NEAR_BLUR,
    SHADOW_NEAR_Y, TEXT, TEXT_MUTED, TEXT_STRONG,
};

/// The menu's width: wide enough for `Confirm delete Thread` beside a
/// shortcut hint, narrow enough to sit inside a nav row's reach.
const WIDTH: f32 = 224.0;
/// The band between two groups of rows — space, never a line.
const GAP_H: f32 = 6.0;

/// One row of the menu.
pub struct Item {
    pub label: SharedString,
    /// The key that does the same thing, shown muted at the right edge.
    pub hint: Option<SharedString>,
    /// Arms before it runs, and wears the blocked ink.
    pub destructive: bool,
    /// Drawn muted, presses do nothing.
    pub disabled: bool,
}

impl Item {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            hint: None,
            destructive: false,
            disabled: false,
        }
    }

    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// The floating shell, at the menu ground with the float shadow and no
/// border. The caller positions it (`anchored`) and fills it with `row`s.
pub fn shell() -> Div {
    div()
        // This deferred surface sits over selectable transcript text. Own its
        // inert space so the covered text's I-beam cannot show through.
        .cursor_default()
        .occlude()
        .flex()
        .flex_col()
        .w(px(WIDTH))
        .p(px(MENU_PAD))
        .rounded(px(R_MENU))
        .bg(rgb(MENU))
        .shadow(vec![
            BoxShadow {
                inset: false,
                color: rgba(SHADOW_FAR).into(),
                offset: point(px(0.), px(SHADOW_FAR_Y)),
                blur_radius: px(SHADOW_FAR_BLUR),
                spread_radius: px(SHADOW_FAR_SPREAD),
            },
            BoxShadow {
                inset: false,
                color: rgba(SHADOW_NEAR).into(),
                offset: point(px(0.), px(SHADOW_NEAR_Y)),
                blur_radius: px(SHADOW_NEAR_BLUR),
                spread_radius: px(0.),
            },
        ])
}

/// The space between two groups of rows.
pub fn gap() -> Div {
    div().flex_shrink_0().h(px(GAP_H))
}

/// One row: the label, the hint hard right. `armed` is a destructive row
/// on its second press — the confirmation, on the blocked wash.
pub fn row(index: usize, item: &Item, armed: bool) -> Stateful<Div> {
    let ink = if item.disabled {
        TEXT_MUTED
    } else if item.destructive {
        BLOCKED
    } else {
        TEXT
    };
    let label: SharedString = if armed {
        SharedString::from(format!("Confirm: {}", item.label))
    } else {
        item.label.clone()
    };
    let mut row = div()
        .id(("context-menu-row", index))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap(px(12.))
        .h(px(MENU_ROW_H))
        .px(px(9.))
        .rounded(px(R_CONTROL))
        .text_size(px(FS_MD))
        .text_color(rgb(ink))
        .child(div().min_w_0().truncate().child(label));
    if let Some(hint) = &item.hint {
        row = row.child(
            div()
                .flex_shrink_0()
                .text_size(px(FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child(hint.clone()),
        );
    }
    if armed {
        row = row.bg(rgba(BLOCKED_WASH)).text_color(rgb(TEXT_STRONG));
    }
    if item.disabled {
        row
    } else {
        row.hover_row().press_row()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{deferred, Context, CursorStyle, Render};
    use std::{cell::Cell, rc::Rc};

    struct OcclusionHarness {
        card_hovered: Rc<Cell<bool>>,
        menu_hovered: Rc<Cell<bool>>,
    }

    impl Render for OcclusionHarness {
        fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            let card_hovered = self.card_hovered.clone();
            let menu_hovered = self.menu_hovered.clone();
            div()
                .relative()
                .size(px(300.))
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .on_mouse_move(move |_, _, _| card_hovered.set(true)),
                )
                .child(
                    deferred(
                        shell().absolute().top_0().left_0().child(
                            div()
                                .size(px(80.))
                                .on_mouse_move(move |_, _, _| menu_hovered.set(true)),
                        ),
                    )
                    .with_priority(2),
                )
        }
    }

    #[test]
    fn a_live_row_is_a_button_and_a_disabled_one_is_not() {
        let live = Item::new("Rename").hint("⏎");
        let mut drawn = row(0, &live, false);
        assert_eq!(drawn.style().mouse_cursor, Some(CursorStyle::PointingHand));
        let dead = Item::new("Reveal in Finder").disabled(true);
        let mut drawn = row(1, &dead, false);
        assert_eq!(drawn.style().mouse_cursor, None);
    }

    #[test]
    fn the_floating_shell_masks_the_cursor_beneath_it() {
        let mut drawn = shell();
        assert_eq!(drawn.style().mouse_cursor, Some(CursorStyle::Arrow));
    }

    #[gpui::test]
    fn the_context_menu_blocks_hover_on_the_card_beneath_it(cx: &mut gpui::TestAppContext) {
        let card_hovered = Rc::new(Cell::new(false));
        let menu_hovered = Rc::new(Cell::new(false));
        let (_, window) = cx.add_window_view({
            let card_hovered = card_hovered.clone();
            let menu_hovered = menu_hovered.clone();
            move |_, _| OcclusionHarness {
                card_hovered,
                menu_hovered,
            }
        });
        window.update(|window, cx| window.draw(cx).clear(cx));
        window.simulate_mouse_move(
            gpui::point(px(20.), px(20.)),
            None,
            gpui::Modifiers::default(),
        );

        assert!(menu_hovered.get(), "the pointer still reaches the menu");
        assert!(
            !card_hovered.get(),
            "the covered Thread card must not receive hover"
        );
    }

    #[test]
    fn an_armed_destructive_row_wears_the_wash() {
        let delete = Item::new("Delete Thread").destructive();
        let mut drawn = row(2, &delete, true);
        assert_eq!(drawn.style().background, Some(rgba(BLOCKED_WASH).into()));
        let mut calm = row(2, &delete, false);
        assert_eq!(calm.style().background, None);
    }
}
