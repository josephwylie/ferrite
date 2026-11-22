//! ⌘{ / ⌘} and ⌘1…⌘9 walk the sidebar's Threads in the order it draws
//! them, one Thread on screen at a time.

use super::*;

/// The Threads the sidebar lists, top to bottom, as drawn.
fn listed(threads: &[ThreadKey], cx: &mut gpui::VisualTestContext) -> Vec<ThreadKey> {
    let mut rows: Vec<(Pixels, ThreadKey)> = threads
        .iter()
        .filter_map(|thread| {
            debug_bounds(cx, format!("nav-thread-{}", thread.thread().get()))
                .map(|row| (row.origin.y, *thread))
        })
        .collect();
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    rows.into_iter().map(|(_, thread)| thread).collect()
}

/// The Threads whose Pane is on screen.
fn shown(threads: &[ThreadKey], cx: &mut gpui::VisualTestContext) -> Vec<ThreadKey> {
    threads
        .iter()
        .copied()
        .filter(|thread| debug_bounds(cx, format!("pane-root-{}", thread.thread().get())).is_some())
        .collect()
}

fn three_threads<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, Vec<ThreadKey>, &'a mut gpui::VisualTestContext) {
    bind_production_keys(cx);
    let (core, _fake) = cockpit(name, 3);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    (view, threads, cx)
}

/// ⌘} shows the Thread listed under the one on screen, and wraps from the
/// last to the first; ⌘{ walks back up.
#[gpui::test]
fn cmd_brace_walks_the_sidebars_threads_in_its_order(cx: &mut TestAppContext) {
    let (view, threads, cx) = three_threads("thread-keys-walk", cx);
    let order = listed(&threads, cx);
    assert_eq!(order.len(), 3, "every Thread has a row");
    view.update(cx, |view, cx| view.focus_thread(order[0], cx));
    tick(cx);
    assert_eq!(shown(&threads, cx), [order[0]]);

    cx.simulate_keystrokes("cmd-}");
    tick(cx);
    assert_eq!(shown(&threads, cx), [order[1]], "the next row's Thread");
    cx.simulate_keystrokes("cmd-}");
    tick(cx);
    assert_eq!(shown(&threads, cx), [order[2]]);
    cx.simulate_keystrokes("cmd-}");
    tick(cx);
    assert_eq!(shown(&threads, cx), [order[0]], "the last wraps to the first");

    cx.simulate_keystrokes("cmd-{");
    tick(cx);
    assert_eq!(shown(&threads, cx), [order[2]], "and back up from the first");
    cx.simulate_keystrokes("cmd-{");
    tick(cx);
    assert_eq!(shown(&threads, cx), [order[1]]);
}

/// ⌘2 shows the sidebar's second Thread; a digit past the list leaves the
/// screen as it is.
#[gpui::test]
fn a_cmd_digit_shows_the_sidebars_thread_at_that_place(cx: &mut TestAppContext) {
    let (view, threads, cx) = three_threads("thread-keys-digits", cx);
    let order = listed(&threads, cx);
    view.update(cx, |view, cx| view.focus_thread(order[0], cx));
    tick(cx);

    cx.simulate_keystrokes("cmd-2");
    tick(cx);
    assert_eq!(shown(&threads, cx), [order[1]]);
    cx.simulate_keystrokes("cmd-3");
    tick(cx);
    assert_eq!(shown(&threads, cx), [order[2]]);
    cx.simulate_keystrokes("cmd-9");
    tick(cx);
    assert_eq!(shown(&threads, cx), [order[2]], "no ninth Thread: nothing moves");
}

/// A parked Thread's row is listed, but the walk steps over it: a key that
/// walks never spends a Session reviving one.
#[gpui::test]
fn the_walk_steps_over_a_parked_thread(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake) = cockpit("thread-keys-parked", 3);
    let threads = core.threads();
    core.park(threads[1]).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    view.update(cx, |view, cx| view.set_shelf_open(nav::Shelf::Parked, true, cx));
    tick(cx);
    assert!(
        debug_bounds(cx, format!("nav-thread-{}", threads[1].thread().get())).is_some(),
        "the parked row is listed"
    );
    let open = listed(&[threads[0], threads[2]], cx);
    view.update(cx, |view, cx| view.focus_thread(open[0], cx));
    tick(cx);

    for _ in 0..3 {
        cx.simulate_keystrokes("cmd-}");
        tick(cx);
        view.read_with(cx, |view, _| {
            assert!(view.facts.parked().contains(&threads[1]), "still parked");
        });
    }
    assert_eq!(shown(&threads, cx), [open[1]], "three steps over two Threads");
}
