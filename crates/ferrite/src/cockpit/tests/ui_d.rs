//! WP-D's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// SPEC §5.5: Solo's Composer is a box `COMPOSER_INSET` inside the card's
/// sides, its editor `COMPOSER_INPUT_PAD_X` in from the box's edge; the bar
/// carries no status line.
#[gpui::test]
fn the_composer_box_stands_inside_the_card(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("composer-reading-column", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    for (width, height) in [(1600., 900.), (700., 700.)] {
        cx.simulate_resize(gpui::size(px(width), px(height)));
        tick(cx);
        let block = cx.debug_bounds("composer-block").unwrap();
        let editor = cx
            .debug_bounds("focused-prompt-editor")
            .or_else(|| cx.debug_bounds("prompt-editor"))
            .unwrap();
        let pane = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1);
        let inset = crate::theme::COMPOSER_INSET;
        assert!(
            (block.left() - px(pane.x + inset)).abs() <= px(1.)
                && (block.right() - px(pane.x + pane.w - inset)).abs() <= px(1.),
            "{block:?} / {pane:?}"
        );
        assert!(
            (block.bottom() - px(pane.y + pane.h - inset)).abs() <= px(1.),
            "{block:?} / {pane:?}"
        );
        assert!(
            (editor.left() - block.left() - px(crate::theme::COMPOSER_INPUT_PAD_X)).abs()
                <= px(0.5),
            "{editor:?} / {block:?}"
        );
        assert!(cx.debug_bounds("composer-meta").is_none(), "no status line");
    }
}
