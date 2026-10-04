//! WP-D's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// Terminal-native (WP-D): the Composer's input band spans the Pane's
/// width — no reading column, no box — its `❯` hanging in the 2-cell gutter
/// `COMPOSER_PAD_L` in, its editor starting two cells after that; under it
/// the status line, then the stack's own air to the Pane's foot.
#[gpui::test]
fn the_composer_block_shares_the_reading_column(cx: &mut TestAppContext) {
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
        // Inside the Pane's 1px border, edge to edge.
        assert!(
            (block.left() - px(pane.x + 1.)).abs() <= px(1.)
                && (block.right() - px(pane.x + pane.w - 1.)).abs() <= px(1.),
            "{block:?} / {pane:?}"
        );
        assert!(
            (editor.left()
                - block.left()
                - px(crate::theme::COMPOSER_PAD_L + crate::theme::COMPOSER_GUTTER))
            .abs()
                <= px(0.5),
            "{editor:?} / {block:?}"
        );
        let meta = cx.debug_bounds("composer-meta").expect("the status line");
        assert!(meta.top() >= block.bottom());
        assert!(
            px(pane.y + pane.h) - meta.bottom() >= px(crate::theme::COMPOSER_STATUS_PAD_B),
            "the status line stands clear of the Pane's foot"
        );
    }
}
