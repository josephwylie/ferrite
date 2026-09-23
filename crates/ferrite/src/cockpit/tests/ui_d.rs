//! WP-D's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// The Composer is a raised block in the reading column: at most
/// `READING_MAX_W` wide, centred in a wide Pane, its editor starting at C1
/// (`BOX_INSET_X` + `GUTTER_W`) so its `❯` shares the transcript's axis.
#[gpui::test]
fn the_composer_block_shares_the_reading_column(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("composer-reading-column", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    tick(cx);
    let block = cx.debug_bounds("composer-block").unwrap();
    let editor = cx
        .debug_bounds("focused-prompt-editor")
        .or_else(|| cx.debug_bounds("prompt-editor"))
        .unwrap();
    let pane = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1);
    assert_eq!(block.size.width, px(crate::theme::READING_MAX_W));
    let centre = px(pane.x + pane.w / 2.);
    assert!(
        (block.center().x - centre).abs() <= px(1.),
        "{block:?} / {pane:?}"
    );
    assert_eq!(
        editor.left() - block.left(),
        px(crate::theme::BOX_INSET_X + crate::theme::GUTTER_W)
    );
    assert!(
        px(pane.y + pane.h) - block.bottom() >= px(crate::theme::COMPOSER_INSET_B),
        "the block floats clear of the Pane's bottom edge"
    );

    // A narrow Pane gives the block the full column less the Pane padding.
    cx.simulate_resize(gpui::size(px(700.), px(700.)));
    tick(cx);
    let block = cx.debug_bounds("composer-block").unwrap();
    let pane = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1);
    assert!(block.size.width < px(crate::theme::READING_MAX_W));
    assert!(
        (block.left() - px(pane.x + crate::theme::PANE_PAD_X)).abs() <= px(1.),
        "{block:?} / {pane:?}"
    );
}
