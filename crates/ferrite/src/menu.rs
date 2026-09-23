//! The context menu: what a right-click on a Thread, a Group, a Project
//! or a Pane offers. Drawing only, like `nav.rs` — the cockpit decides the
//! rows and runs the verbs. It is the one floating surface
//! (`components::floating_surface`), anchored at the pointer, its rows the
//! one menu row (`components::menu_row`), its groups split by a hairline.
//!
//! A destructive verb never runs on one press: its row arms on the first
//! (the label becomes the confirmation, on the blocked wash) and runs on
//! the second. Anything else pressed disarms it.

use gpui::prelude::*;
use gpui::{div, px, rgb, Div, SharedString, Stateful};

use crate::components::{self, MenuItem};
use crate::icons;
use crate::theme::*;

/// One row of the menu: the shared menu row's content. Its `shortcut` is the
/// key that does the same thing (`cmd-F`), drawn here so the command key can
/// be a glyph box.
pub type Item = MenuItem;

/// The floating shell: at least `MENU_W`, and as wide as its longest verb
/// beside its shortcut, so no verb is ever cut. The caller positions it
/// (`anchored`, which keeps it inside the window).
pub fn shell() -> Div {
    components::floating_surface().min_w(px(MENU_W))
}

/// The line between two groups of rows.
pub fn gap() -> Div {
    components::menu_separator()
}

/// One row, its shortcut hard right. `armed` is a destructive row on its
/// second press — the confirmation, on the blocked wash.
pub fn row(index: usize, item: &Item, armed: bool) -> Stateful<Div> {
    let keys = item.shortcut.clone();
    let face = MenuItem {
        shortcut: None,
        ..item.clone()
    };
    let ink = components::row_inks(item, false, armed).shortcut;
    components::menu_row(("context-menu-row", index), &face, false, armed)
        .when_some(keys.filter(|_| !armed), |row, keys| {
            row.child(shortcut(&keys, ink))
        })
}

/// A shortcut in the menu's trailing column: mono `FS_SM`. A `cmd-` prefix
/// draws the command key as a glyph box, because Geist Mono has no `⌘`.
fn shortcut(keys: &SharedString, ink: u32) -> Div {
    let drawn = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .text_size(px(FS_SM))
        .text_color(rgb(ink));
    match keys.strip_prefix("cmd-") {
        Some(key) => drawn
            .child(icons::icon(icons::COMMAND, MENU_KEY_GLYPH, ink))
            .child(key.to_string()),
        None => drawn.child(keys.clone()),
    }
}

/// A tooltip in the floating vocabulary: mono `FS_SM`, 8px × 4px, at most
/// `TOOLTIP_MAX_W` wide (a long path wraps), the float shadow over the kit's
/// raised ground and strong hairline edge.
pub fn tooltip(
    text: impl Into<SharedString>,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |window, cx| {
        gpui::component::tooltip::Tooltip::new(text.clone())
            .font_family(FONT_MONO)
            .text_size(px(FS_SM))
            .line_height(px(LH_META))
            .px(px(TOOLTIP_PAD_X))
            .py(px(TOOLTIP_PAD_Y))
            .max_w(px(TOOLTIP_MAX_W))
            .shadow(components::float_shadow())
            .build(window, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{deferred, rgba, Context, CursorStyle, Render};
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
        let live = Item::new("Rename").shortcut("↵");
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
        assert_eq!(drawn.style().min_size.width, Some(px(MENU_W).into()));
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
        assert_eq!(
            components::row_inks(&delete, false, false).label,
            BLOCKED,
            "a destructive verb wears the blocked ink before it arms"
        );
    }
}
