//! WP-E's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// FL-18: a request from a Thread off the focused Pane toasts with the nav
/// open, as one float 52 cells wide, its right edge two cells in from the
/// window's, its foot 36px over the bottom bar.
#[gpui::test]
fn a_request_toast_stands_two_cells_in_and_over_the_bottom_bar(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("toast-side", 2);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let off_focus = view.read_with(cx, |view, _| {
        threads
            .iter()
            .copied()
            .find(|thread| Some(*thread) != view.focused_thread())
            .unwrap()
    });
    let stream = threads
        .iter()
        .position(|thread| *thread == off_focus)
        .unwrap();
    fake.streams.borrow()[stream]
        .send(super::decision("toast-side"))
        .unwrap();
    tick(cx);
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.bell.toasts().len(),
            1,
            "the nav is open, and it toasts"
        );
    });
    let selector: &'static str = format!("toast-{}", off_focus.get()).leak();
    let toast = cx.debug_bounds(selector).expect("the toast is up");
    let window = cx.update(|window, _| window.viewport_size());
    assert!(
        (toast.size.width - px(crate::theme::TOAST_W)).abs() <= px(1.),
        "52 cells: {toast:?}"
    );
    assert!(
        (toast.right() - (window.width - px(2.0 * crate::theme::CH))).abs() <= px(1.),
        "two cells in from the window's edge: {toast:?}"
    );
    assert!(
        (toast.bottom()
            - (window.height - px(crate::theme::STATUS_BAR_H + crate::theme::TOAST_BOTTOM)))
        .abs()
            <= px(1.),
        "36px over the bottom bar: {toast:?}"
    );
}

/// A finished turn keeps its toast rule: with the nav folded, an off-board
/// Thread's finish toasts (only `⏎ open`), clear of every Pane head, and it
/// goes once its Thread lands on the board.
#[gpui::test]
fn the_rails_one_toast_clears_every_pane_head_and_goes_on_landing(cx: &mut TestAppContext) {
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
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.bell.toasts().len(), 1, "one toast, off the board")
    });
    let selector: &'static str = format!("toast-{}", threads[2].get()).leak();
    let toast = cx.debug_bounds(selector).expect("the toast is up");
    let open: &'static str = format!("toast-open-{}", threads[2].get()).leak();
    assert!(cx.debug_bounds(open).is_some(), "a finish offers `⏎ open`");
    let allow: &'static str = format!("toast-allow-{}", threads[2].get()).leak();
    assert!(cx.debug_bounds(allow).is_none(), "and no answers");
    for thread in &threads[..2] {
        let head: &'static str = format!("pane-head-{}", thread.get()).leak();
        let head = cx.debug_bounds(head).expect("the board shows a Pane head");
        assert!(
            head.bottom() <= toast.top(),
            "a toast would cover the Pane head at {head:?}"
        );
    }

    // The Thread lands on the board: its toast goes.
    view.update(cx, |view, cx| view.land_on_thread(threads[2], cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.bell.toasts().is_empty(),
            "a Thread on the board has no toast"
        )
    });
    assert!(cx.debug_bounds(selector).is_none());
}
