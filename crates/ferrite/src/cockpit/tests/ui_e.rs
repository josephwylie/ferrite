//! WP-E's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// The one toast (C8, only ever shown with the nav on the rail) stands
/// BottomRight above the Composer, one at a time, whichever way the nav is
/// folded: there is no bottom-left card any more.
#[gpui::test]
fn the_toast_stands_bottom_right_above_the_composer(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("toast-side", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let side = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| {
            let toasts = &gpui::component::Theme::global(cx).notification;
            (
                toasts.placement,
                toasts.margins.bottom,
                toasts.width,
                toasts.max_items,
            )
        })
    };
    let expected = (
        gpui::Anchor::BottomRight,
        px(crate::theme::TOAST_ABOVE_COMPOSER),
        px(crate::theme::NAV_WIDTH - 2. * crate::theme::SPACE_2),
        1,
    );
    assert_eq!(side(cx), expected);
    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    tick(cx);
    assert_eq!(side(cx), expected);
    let (_, bottom, _, _) = side(cx);
    let editor = cx.debug_bounds("focused-prompt-editor").unwrap();
    let window_h = cx.update(|window, _| window.viewport_size().height);
    assert!(
        window_h - bottom <= editor.origin.y,
        "the toast's foot clears the Composer's line"
    );
    view.update(cx, |view, cx| view.set_nav_collapsed(false, cx));
    tick(cx);
    assert_eq!(side(cx), expected);
}

/// At the app size with the nav on the rail, an off-board Thread's finish
/// stands as one toast, BottomRight above the Composer: it never covers a
/// Pane's head or the focused Composer, and it goes once its Thread lands
/// on the board.
#[gpui::test]
fn the_rails_one_toast_clears_every_pane_head_and_the_composer(cx: &mut TestAppContext) {
    use gpui::component::WindowExt as _;
    // A board of two, and a third Thread off it.
    let (mut core, fake) = cockpit("toast-geometry", 3);
    let threads = core.threads();
    core.apply_group(GroupChange::Create {
        first: threads[0],
        second: threads[1],
    })
    .unwrap();
    let group = core.groups().of(threads[0]).unwrap().id;
    core.send(threads[2], "task".into());
    core.pump();
    core.enter_group(group).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    tick(cx);
    fake.streams.borrow()[2]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    std::thread::sleep(Duration::from_millis(600));
    for _ in 0..8 {
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        assert_eq!(
            window.notifications(cx).len(),
            1,
            "one toast, off the board"
        )
    });
    let selector: &'static str = format!("toast-{}", threads[2].get()).leak();
    let toast = cx.debug_bounds(selector).expect("the toast is up");
    let window = cx.update(|window, _| window.viewport_size());
    assert!(
        toast.right() <= window.width - px(crate::theme::GRID_PAD),
        "BottomRight, inside the board's padding: {toast:?}"
    );
    assert!(
        toast.bottom() <= window.height - px(crate::theme::TOAST_ABOVE_COMPOSER),
        "above the Composer: {toast:?}"
    );
    for thread in &threads[..2] {
        let head: &'static str = format!("pane-head-{}", thread.get()).leak();
        let head = cx.debug_bounds(head).expect("the board shows a Pane head");
        assert!(
            head.bottom() <= toast.top(),
            "a toast would cover the Pane head at {head:?}"
        );
    }
    let editor = cx.debug_bounds("focused-prompt-editor").unwrap();
    assert!(
        toast.bottom() <= editor.origin.y,
        "a toast would cover the Composer"
    );

    // The Thread lands on the board: its toast goes.
    view.update(cx, |view, cx| view.land_on_thread(threads[2], cx));
    tick(cx);
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(
            window.notifications(cx).is_empty(),
            "a Thread on the board has no toast"
        )
    });
}
