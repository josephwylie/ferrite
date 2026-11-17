//! WP-E's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// FL-18 and C8: a request from a Thread off the focused Pane rings, but
/// with the nav open its row is the queue and no toast draws; with the nav
/// folded it toasts as one float top right of the transcript's column,
/// `TOAST_W` wide under the Pane heads — never over a Composer or the
/// right panel.
#[gpui::test]
fn a_request_toast_stands_top_right_clear_of_the_composer(cx: &mut TestAppContext) {
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
    ring(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.needs_you.toasts().len(), 1, "it rings");
    });
    let selector: &'static str = format!("toast-{}", off_focus.get()).leak();
    assert!(
        cx.debug_bounds(selector).is_none(),
        "the nav is open: its row is the queue, nothing toasts"
    );
    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    ring(cx);
    let toast = cx
        .debug_bounds(selector)
        .expect("the nav folded: it toasts");
    use ferrite_ui::theme::{PANE_HEAD_H, TOAST_BOX_RIGHT, TOAST_TOP, TOAST_W, WIN_CHROME_H};
    let window = cx.update(|window, _| window.viewport_size());
    let panel = view.read_with(cx, |view, _| view.panel_column_w());
    assert!(panel > 0., "the premise: the panel shows");
    // On the transcript's column, under the Pane heads, `TOAST_BOX_RIGHT`
    // in from the panel's left: the panel is never covered.
    let right = panel + TOAST_BOX_RIGHT;
    let top = WIN_CHROME_H + PANE_HEAD_H + TOAST_TOP;
    assert!(
        (toast.size.width - px(TOAST_W)).abs() <= px(1.),
        "{TOAST_W} wide: {toast:?}"
    );
    assert!(
        (toast.right() - (window.width - px(right))).abs() <= px(1.),
        "{right} in from the window's edge: {toast:?}"
    );
    assert!(
        (toast.top() - px(top)).abs() <= px(1.),
        "{top} from the window's top: {toast:?}"
    );
    let shown = cx.debug_bounds("panel").expect("the panel");
    assert!(
        !shown.intersects(&toast),
        "{toast:?} covers the panel {shown:?}"
    );
    assert_eq!(
        cx.update(|_, cx| gpui::component::Theme::global(cx)
            .notification
            .margins
            .right),
        px(right),
        "the kit's toasts keep to the same column"
    );
}

/// A finished turn keeps its toast rule: with the nav folded, a Thread off
/// screen toasts its finish (only `⏎ open`), clear of the view's head, and
/// the toast goes once its Thread is on screen.
#[gpui::test]
fn the_rails_one_toast_clears_the_view_head_and_goes_on_landing(cx: &mut TestAppContext) {
    // One Thread on screen, and another off it.
    let (mut core, fake) = cockpit("toast-geometry", 3);
    let threads = core.threads();
    let _ = core.send(threads[2], "task".into());
    core.pump();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    view.update(cx, |view, cx| {
        view.focus_thread(threads[0], cx);
        view.set_nav_collapsed(true, cx);
    });
    tick(cx);
    fake.streams.borrow()[2]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    ring(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.needs_you.toasts().len(), 1, "one toast, off screen")
    });
    let selector: &'static str = format!("toast-{}", threads[2].get()).leak();
    let toast = cx.debug_bounds(selector).expect("the toast is up");
    let open: &'static str = format!("toast-open-{}", threads[2].get()).leak();
    assert!(cx.debug_bounds(open).is_some(), "a finish offers `⏎ open`");
    let allow: &'static str = format!("toast-allow-{}", threads[2].get()).leak();
    assert!(cx.debug_bounds(allow).is_none(), "and no answers");
    let head = cx.debug_bounds("view-head").expect("the view's head");
    // Clear of it: under it, or beside it over the panel's column.
    assert!(
        !head.intersects(&toast),
        "a toast {toast:?} would cover the view head at {head:?}"
    );

    // The Thread comes on screen: its toast goes.
    view.update(cx, |view, cx| view.land_on_thread(threads[2], cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.needs_you.toasts().is_empty(),
            "a Thread on screen has no toast"
        )
    });
    assert!(cx.debug_bounds(selector).is_none());
}
