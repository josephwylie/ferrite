//! Closing a pane parks its thread; reopening revives it.
#[allow(unused_imports)]
use super::*;

/// AC1 at the keyboard: closing a Pane parks its Thread — the Session
/// ends, the log stays, and the store still has it to reopen.
#[gpui::test]
fn closing_a_pane_parks_the_thread_rather_than_losing_it(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("park", 2);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-w", CloseThread, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let closed = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

    cx.simulate_keystrokes("cmd-w");

    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 1, "the Pane is gone");
        assert!(
            view.cockpit.thread(closed).is_none(),
            "and so is its memory"
        );
        assert!(
            view.cockpit.parked().unwrap().contains(&closed),
            "but the Thread is still there to reopen"
        );
    });
}

/// AC1's other half: a parked Thread comes back into the running cockpit,
/// with its history and the marker saying what it is.
#[gpui::test]
fn reopening_brings_a_parked_thread_back_with_its_history(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("reopen", 2);
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-w", CloseThread, None),
            KeyBinding::new("cmd-o", ReopenThread, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let closed = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    cx.simulate_keystrokes("cmd-w");
    view.read_with(cx, |view, _| assert_eq!(view.panes.len(), 1));

    cx.simulate_keystrokes("cmd-o");

    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 2, "the Pane is back");
        assert!(
            view.panes.iter().any(|pane| pane.thread() == Some(closed)),
            "and it is the same Thread, not a new one"
        );
        assert!(view.cockpit.thread(closed).is_some(), "with its history");
    });
}

/// #17: cmd-o follows park order, not creation order. Create A then B,
/// park B then A — the Thread that comes back is A, the one the operator
/// just closed.
#[gpui::test]
fn reopening_revives_the_thread_parked_most_recently(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("park-order", 2);
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-w", CloseThread, None),
            KeyBinding::new("cmd-o", ReopenThread, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let (a, b) = created(&view, cx);

    view.update(cx, |view, _| view.focus_pane(view.pane_for(b).unwrap()));
    cx.simulate_keystrokes("cmd-w"); // park B
    cx.simulate_keystrokes("cmd-w"); // then A — the most recent park
    cx.simulate_keystrokes("cmd-o");

    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 1);
        assert_eq!(
            view.panes[0].thread().unwrap(),
            a,
            "cmd-o must revive the just-parked {a}, not the newest-created {b}"
        );
    });
}

/// Reopening again keeps walking the park order backwards: park A then B,
/// and two cmd-o bring back B first, then A.
#[gpui::test]
fn reopening_again_walks_the_park_order_backwards(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("park-order-again", 2);
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-w", CloseThread, None),
            KeyBinding::new("cmd-o", ReopenThread, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let (a, b) = created(&view, cx);

    view.update(cx, |view, _| view.focus_pane(view.pane_for(a).unwrap()));
    cx.simulate_keystrokes("cmd-w"); // park A
    cx.simulate_keystrokes("cmd-w"); // then B

    cx.simulate_keystrokes("cmd-o");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 1);
        assert_eq!(
            view.panes[0].thread().unwrap(),
            b,
            "the last park comes back first"
        );
    });

    cx.simulate_keystrokes("cmd-o");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 2);
        assert!(
            view.panes.iter().any(|pane| pane.thread() == Some(a)),
            "and the one before it comes back next"
        );
    });
}

/// The park order is memory, not store: a Thread parked before this
/// launch is not in it. This launch's parks come back first, and only
/// then does cmd-o fall back to the newest-created parked Thread.
#[gpui::test]
fn reopening_falls_back_to_creation_order_for_threads_parked_before_launch(
    cx: &mut TestAppContext,
) {
    let fake = Fake::default();
    let store = Store::open(scratch("park-order-fallback")).unwrap();
    let mut core = Cockpit::new(store, Box::new(fake.clone()));
    let a = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    let b = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    // Parked before the view exists — a previous launch, as far as the
    // view can know.
    core.park(b).unwrap();
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-w", CloseThread, None),
            KeyBinding::new("cmd-o", ReopenThread, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.read_with(cx, |view, _| assert_eq!(view.panes.len(), 1));

    cx.simulate_keystrokes("cmd-w"); // park A — this launch's only park
    cx.simulate_keystrokes("cmd-o");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 1);
        assert_eq!(
            view.panes[0].thread().unwrap(),
            a,
            "the just-parked {a} outranks the newer-created {b}"
        );
    });

    cx.simulate_keystrokes("cmd-o"); // the order is drained: creation order
    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 2);
        assert!(
            view.panes.iter().any(|pane| pane.thread() == Some(b)),
            "the pre-launch park still comes back, by creation order"
        );
    });
}
