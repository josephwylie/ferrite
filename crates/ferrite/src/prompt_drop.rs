//! A pane-wide native file target. GPUI owns hit testing and drag lifetime;
//! Composer owns pending files. The cockpit only activates the dropped pane.

use crate::{composer::Composer, theme};
use gpui::prelude::*;
use gpui::{div, px, rgb, App, Div, Entity, ExternalPaths, Focusable, Window};

pub fn target(
    content: Div,
    composer: Entity<Composer>,
    activate: impl Fn(&mut Window, &mut App) + 'static,
) -> Div {
    content
        // GPUI Kit 0.6 translates native file events into mouse moves without
        // changing keyboard modality; its drop hitboxes then reject the drag.
        // Normalize the hovered pane's first move through the public input API.
        .on_drag_move(|event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
            if window.last_input_was_keyboard() && event.bounds.contains(&event.event.position) {
                let event = event.event.clone();
                window.defer(cx, move |window, cx| {
                    window.dispatch_event(gpui::PlatformInput::MouseMove(event), cx);
                });
            }
        })
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
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .rounded(px(theme::R_SURFACE))
                .bg(rgb(theme::RAISED))
                .opacity(0.)
                .drag_over::<ExternalPaths>(|style, _, _, _| style.opacity(0.96))
                .font_family(theme::FONT_UI)
                .text_size(px(theme::FS_MD))
                .text_color(rgb(theme::TEXT_STRONG))
                .child("Drop files to add to prompt"),
        )
}
