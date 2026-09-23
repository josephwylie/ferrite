//! WP-E's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// Toasts stack at the foot of the nav; with the nav collapsed they move
/// BottomRight, clear of the Composer, and come back when it reopens.
#[gpui::test]
fn toasts_follow_the_nav_to_the_side_with_ground_to_spare(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("toast-side", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let side = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| {
            let toasts = &gpui::component::Theme::global(cx).notification;
            (toasts.placement, toasts.margins.bottom, toasts.width)
        })
    };
    assert_eq!(
        side(cx),
        (
            gpui::Anchor::BottomLeft,
            px(crate::theme::GRID_PAD),
            px(crate::theme::NAV_WIDTH - 2. * crate::theme::SPACE_2)
        )
    );
    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    tick(cx);
    let (anchor, bottom, _) = side(cx);
    assert_eq!(anchor, gpui::Anchor::BottomRight);
    assert_eq!(bottom, px(crate::theme::TOAST_ABOVE_COMPOSER));
    let editor = cx.debug_bounds("focused-prompt-editor").unwrap();
    let window_h = cx.update(|window, _| window.viewport_size().height);
    assert!(
        window_h - bottom <= editor.origin.y,
        "the stack's foot clears the Composer's line"
    );
    view.update(cx, |view, cx| view.set_nav_collapsed(false, cx));
    tick(cx);
    assert_eq!(side(cx).0, gpui::Anchor::BottomLeft);
}

/// At the app size with the nav open, the toast stack's column (the kit's
/// own placement values) lies wholly on the nav's ground: it never covers a
/// Pane's head or the focused Composer.
#[gpui::test]
fn the_toast_column_clears_every_pane_head_and_the_composer(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("toast-geometry", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let (placement, margins, width) = cx.update(|_, cx| {
        let toasts = &gpui::component::Theme::global(cx).notification;
        (toasts.placement, toasts.margins.clone(), toasts.width)
    });
    assert_eq!(placement, gpui::Anchor::BottomLeft);
    let right = margins.left + width;
    assert!(
        right <= px(crate::theme::NAV_WIDTH),
        "the column stays on the nav"
    );
    let threads: Vec<_> = view.read_with(cx, |view, _| {
        view.panes.iter().filter_map(|pane| pane.thread()).collect()
    });
    let heads: Vec<_> = threads
        .iter()
        .filter_map(|thread| {
            cx.debug_bounds(Box::leak(
                format!("pane-head-{}", thread.get()).into_boxed_str(),
            ))
        })
        .collect();
    assert!(!heads.is_empty(), "the board shows a Pane head");
    for head in heads {
        assert!(
            head.origin.x >= right,
            "a toast would cover the Pane head at {head:?}"
        );
    }
    let editor = cx.debug_bounds("focused-prompt-editor").unwrap();
    assert!(editor.origin.x >= right, "a toast would cover the Composer");
}
