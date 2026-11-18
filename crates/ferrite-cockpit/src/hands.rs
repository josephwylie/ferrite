//! What the driven window's hands send it (ADR 0038), kept apart from the
//! headless platform so the window tests send the very same thing
//! `ferrite --headless` does.

use ferrite_ui::theme::{WINDOW_MIN_H, WINDOW_MIN_W};
use gpui::{App, Pixels, Size, Window};

/// The window's content set to `size`, as a resize by its corner lands:
/// never under the least size the app's window gives its platform (a
/// smaller one stops there), then the platform's resize callback's own
/// step. The size it has now.
pub fn resize(window: &mut Window, cx: &mut App, size: Size<Pixels>) -> Size<Pixels> {
    window.resize(gpui::size(
        size.width.max(gpui::px(WINDOW_MIN_W)),
        size.height.max(gpui::px(WINDOW_MIN_H)),
    ));
    window.bounds_changed(cx);
    window.viewport_size()
}
