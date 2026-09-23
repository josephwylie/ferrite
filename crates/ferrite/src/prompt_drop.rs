//! A pane-wide native file target. GPUI owns hit testing and drag lifetime;
//! Composer owns pending files. The cockpit only activates the dropped pane
//! and remembers which Pane the files hover, so the Pane can draw its drop
//! sheet beneath the Composer that will receive them.

use crate::{composer::Composer, theme};
use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, App, Div, Entity, ExternalPaths, Focusable, Window};

pub fn target(
    content: Div,
    composer: Entity<Composer>,
    activate: impl Fn(&mut Window, &mut App) + 'static,
    hover: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> Div {
    content
        .on_drag_move(
            move |event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                // GPUI hands every drag move to every listener: whether the files
                // are over *this* Pane is the pointer against its bounds.
                let over = event.bounds.contains(&event.event.position);
                // GPUI Kit 0.6 translates native file events into mouse moves
                // without changing keyboard modality; its drop hitboxes then
                // reject the drag. Normalize the hovered pane's first move
                // through the public input API.
                if window.last_input_was_keyboard() && over {
                    let event = event.event.clone();
                    window.defer(cx, move |window, cx| {
                        window.dispatch_event(gpui::PlatformInput::MouseMove(event), cx);
                    });
                }
                hover(over, window, cx);
            },
        )
        .on_drop(move |files: &ExternalPaths, window, cx| {
            if files.paths().is_empty() {
                return;
            }
            activate(window, cx);
            composer.update(cx, |composer, cx| {
                composer.add_files(files.paths(), cx);
                composer.focus_handle(cx).focus(window, cx);
            });
            cx.stop_propagation();
        })
}

/// The sheet a Pane wears while files hover it: the whole Pane, raised, with
/// the accent's non-focus edge like a Pane drop's wash, saying what a
/// release does. The Pane lays it just before its Composer, so the Composer
/// paints above the sheet, edged in the same accent: that is where the
/// files land.
pub fn sheet() -> Div {
    div()
        .debug_selector(|| "prompt-drop-sheet".into())
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .rounded(px(theme::R_PANE))
        .bg(rgb(theme::RAISED))
        .border_1()
        .border_color(rgba(theme::ACCENT_EDGE))
        .opacity(0.96)
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(theme::TEXT_STRONG))
        .child("Drop files to add to prompt")
}
