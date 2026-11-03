//! WP-D's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// SPEC §5.5: Solo's Composer is a box framing the reading column (at
/// most `COMPOSER_MAX_W`, the column and 16 either side, centred,
/// `COMPOSER_INSET` from the card's sides at least), its line after the
/// `+`, the 41 footer under it; the bar carries no status line.
#[gpui::test]
fn the_composer_box_stands_inside_the_card(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("composer-reading-column", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    spoken(&fake, 0);
    for (width, height) in [(1600., 900.), (700., 700.)] {
        cx.simulate_resize(gpui::size(px(width), px(height)));
        tick(cx);
        let block = cx.debug_bounds("composer-block").unwrap();
        let editor = cx
            .debug_bounds("focused-prompt-editor")
            .or_else(|| cx.debug_bounds("prompt-editor"))
            .unwrap();
        let pane = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1);
        // The reading column and its outset: at most `COMPOSER_MAX_W`,
        // centred in the card, never nearer its sides than `COMPOSER_INSET`.
        let inset = ferrite_ui::theme::COMPOSER_INSET;
        let w = (pane.w - 2. * inset).min(ferrite_ui::theme::COMPOSER_MAX_W);
        let left = pane.x + (pane.w - w) / 2.;
        assert!(
            (block.left() - px(left)).abs() <= px(1.) && (block.size.width - px(w)).abs() <= px(1.),
            "{block:?} / {pane:?}"
        );
        // The footer (model and effort, mode) stands under the box, all
        // there is between it and the card's foot.
        let foot = ferrite_ui::theme::COMPOSER_FOOTER_H;
        assert!(
            (block.bottom() - px(pane.y + pane.h - foot)).abs() <= px(1.),
            "{block:?} / {pane:?}"
        );
        // The line starts after the `+`.
        let lead = ferrite_ui::theme::COMPOSER_ROW_PAD
            + ferrite_ui::theme::COMPOSER_ATTACH
            + ferrite_ui::theme::COMPOSER_ROW_GAP;
        assert!(
            (editor.left() - block.left() - px(lead)).abs() <= px(0.5),
            "{editor:?} / {block:?}"
        );
        assert!(cx.debug_bounds("composer-meta").is_none(), "no status line");
    }
}
