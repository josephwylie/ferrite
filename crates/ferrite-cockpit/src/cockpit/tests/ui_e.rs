//! WP-E's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// FL-18 and C8: a request from a Thread off the focused Pane rings, but
/// with the nav open its row is the queue and no toast draws; with the nav
/// folded it toasts as one float top right — in the panel's content column
/// under the window's head, or with no panel `TOAST_W` wide under the Pane
/// heads — never over a Composer.
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
    use ferrite_ui::theme::{
        PANEL_PAD_L, PANEL_PAD_R, PANE_HEAD_H, TOAST_BOX_RIGHT, TOAST_TOP, TOAST_W, WIN_CHROME_H,
    };
    let window = cx.update(|window, _| window.viewport_size());
    let panel = view.read_with(cx, |view, _| view.panel_column_w());
    // Over the panel's content column under the window's head, or with no
    // panel `TOAST_W` wide under the Pane heads.
    let (width, right, top) = if panel > 0. {
        (
            TOAST_W.min(panel - PANEL_PAD_L - PANEL_PAD_R),
            PANEL_PAD_R,
            WIN_CHROME_H + TOAST_TOP,
        )
    } else {
        (
            TOAST_W,
            TOAST_BOX_RIGHT,
            WIN_CHROME_H + PANE_HEAD_H + TOAST_TOP,
        )
    };
    assert!(
        (toast.size.width - px(width)).abs() <= px(1.),
        "{width} wide: {toast:?}"
    );
    assert!(
        (toast.right() - (window.width - px(right))).abs() <= px(1.),
        "{right} in from the window's edge: {toast:?}"
    );
    assert!(
        (toast.top() - px(top)).abs() <= px(1.),
        "{top} from the window's top: {toast:?}"
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
    ring(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.needs_you.toasts().len(), 1, "one toast, off the board")
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
        // Clear of it: under it, or beside it over the panel's column.
        assert!(
            !head.intersects(&toast),
            "a toast {toast:?} would cover the Pane head at {head:?}"
        );
    }

    // The Thread lands on the board: its toast goes.
    view.update(cx, |view, cx| view.land_on_thread(threads[2], cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.needs_you.toasts().is_empty(),
            "a Thread on the board has no toast"
        )
    });
    assert!(cx.debug_bounds(selector).is_none());
}
