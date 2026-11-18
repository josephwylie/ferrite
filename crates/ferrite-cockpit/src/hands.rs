//! What the driven window's hands send it (ADR 0038), kept apart from the
//! headless platform so the window tests send the very same thing
//! `ferrite --headless` does.

use ferrite_ui::theme::{WINDOW_MIN_H, WINDOW_MIN_W};
use gpui::{
    App, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    PlatformInput, Point, Size, Window,
};

/// The moves a drag makes with the button held, so what follows the
/// pointer sees it travel rather than jump.
const DRAG_STEPS: usize = 8;

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

/// A left-button drag from `from` to `to`: the pointer onto `from`, the
/// button down, `DRAG_STEPS` moves with it held (the last onto `to`), and
/// the button up there.
pub fn drag(from: Point<Pixels>, to: Point<Pixels>) -> Vec<PlatformInput> {
    let button = MouseButton::Left;
    let modifiers = Modifiers::default();
    let moved = |position, pressed_button| {
        PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button,
            modifiers,
        })
    };
    let mut events = vec![
        moved(from, None),
        PlatformInput::MouseDown(MouseDownEvent {
            button,
            position: from,
            modifiers,
            click_count: 1,
            first_mouse: false,
        }),
    ];
    events.extend((1..=DRAG_STEPS).map(|step| {
        let t = step as f32 / DRAG_STEPS as f32;
        let at = gpui::point(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        moved(at, Some(button))
    }));
    events.push(PlatformInput::MouseUp(MouseUpEvent {
        button,
        position: to,
        modifiers,
        click_count: 1,
    }));
    events
}
