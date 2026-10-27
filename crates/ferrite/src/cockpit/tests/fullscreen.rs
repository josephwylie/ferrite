//! Fullscreening a pane and what follows it.
#[allow(unused_imports)]
use super::*;

/// #20: ⌘⇧⏎ gives the focused Pane the whole cockpit at L1, and ⌘⇧⏎
/// again restores the grid. The proof of L1 is the keyboard: only a
/// Transcript-level Pane renders a Composer, so typing landing there is
/// the level made observable — and the focus snap holding in fullscreen.
#[gpui::test]
fn the_fullscreen_chord_fullscreens_the_focused_pane_and_toggles_back(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("fullscreen", 4);
    let group = group_all(&mut core);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-shift-enter", ToggleFullscreen, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    // Four Panes in this window are wall tiles (a 2×2 board of ~273px
    // columns beside the nav, under R12's 300×360): a tile has no
    // Composer, so typing lands nowhere.
    cx.simulate_resize(gpui::size(px(860.), px(500.)));
    tick(cx);
    cx.simulate_input("lost");
    let typed = view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.take(cx))
    });
    assert_eq!(typed, "", "a wall tile has no Composer to type into");

    cx.simulate_keystrokes("cmd-shift-enter");

    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.roster().fullscreen(),
            Some(PaneIdentity::Thread(view.panes[0].thread().unwrap())),
            "the chord fullscreens the focused Pane"
        );
    });
    // One Pane rendered, spanning the whole area right of the nav —
    // a 2-column cell would be under 300px here.
    let width = view.read_with(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .read(cx)
            .scroll()
            .bounds()
            .size
            .width
    });
    assert!(
        width > px(500.),
        "the fullscreened Pane takes the whole cockpit: {width:?}"
    );
    cx.simulate_input("hi");
    let typed = view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.take(cx))
    });
    assert_eq!(typed, "hi", "fullscreen renders at Transcript level");

    cx.simulate_keystrokes("cmd-shift-enter");

    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.roster().fullscreen(),
            None,
            "the chord again restores the grid"
        );
    });
    cx.simulate_input("still");
    let typed = view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.take(cx))
    });
    assert_eq!(typed, "", "back on the wall, the tiles take no typing");
}

#[gpui::test]
fn the_fullscreen_chord_reaches_a_draft_from_a_dense_grid(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("draft-fullscreen", 5);
    for thread in core.threads() {
        core.park(thread).unwrap();
    }
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-shift-enter", ToggleFullscreen, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| {
        for _ in 0..5 {
            view.open_draft(DraftTarget::Main, cx);
        }
    });
    cx.simulate_resize(gpui::size(px(500.), px(320.)));
    tick(cx);

    cx.simulate_keystrokes("cmd-shift-enter");
    cx.simulate_input("reachable");

    view.read_with(cx, |view, cx| {
        assert!(matches!(
            view.cockpit.roster().fullscreen(),
            Some(PaneIdentity::Draft(_))
        ));
        assert_eq!(
            view.panes[view.focused()].composer.read(cx).text(),
            "reachable"
        );
    });
}

/// #20: fullscreen is L1 *regardless* — a window too small for any cell
/// to earn Transcript still renders the fullscreened Pane at Transcript,
/// Composer and all. Size stops deciding; the mode does.
#[gpui::test]
fn fullscreen_forces_transcript_level_even_in_a_tiny_window(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("fullscreen-tiny", 1);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-shift-enter", ToggleFullscreen, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(240.), px(200.)));
    tick(cx);
    let natural = cx.update(|window, cx| view.read(cx).level_now(window));
    assert!(
        natural < Level::Transcript,
        "the premise: this window cannot earn L1 by size ({natural:?})"
    );

    cx.simulate_keystrokes("cmd-shift-enter");
    cx.simulate_input("hi");

    let typed = view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.take(cx))
    });
    assert_eq!(typed, "hi", "forced L1: the Composer holds the keyboard");
}

/// #20: cmd-] while fullscreen pages the fullscreen to the next Thread —
/// browser-tab muscle memory — rather than exiting, or going stale on
/// the Thread the operator just left.
#[gpui::test]
fn paging_while_fullscreen_moves_the_fullscreen_to_the_next_thread(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("fullscreen-page", 3);
    let group = group_all(&mut core);
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-shift-enter", ToggleFullscreen, None),
            KeyBinding::new("cmd-]", NextPane, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);
    cx.simulate_keystrokes("cmd-shift-enter");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.roster().fullscreen(),
            Some(PaneIdentity::Thread(view.panes[0].thread().unwrap()))
        );
    });

    cx.simulate_keystrokes("cmd-]");

    view.read_with(cx, |view, _| {
        assert_eq!(view.focused(), 1, "cmd-] still walks the Threads");
        assert_eq!(
            view.cockpit.roster().fullscreen(),
            Some(PaneIdentity::Thread(view.panes[1].thread().unwrap())),
            "and the next Thread is the fullscreened one now"
        );
    });
}

/// #20: cmd-w while fullscreen parks the fullscreened Thread and the
/// survivor fills the screen — closing a browser tab shows the next
/// tab, not an overview.
#[gpui::test]
fn closing_the_fullscreened_thread_fullscreens_the_survivor(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("fullscreen-close", 2);
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-shift-enter", ToggleFullscreen, None),
            KeyBinding::new("cmd-w", CloseThread, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    cx.simulate_keystrokes("cmd-shift-enter");
    let closed = view.read_with(cx, |view, _| {
        assert!(
            view.cockpit.roster().fullscreen().is_some(),
            "the premise: fullscreen is on"
        );
        view.panes[0].thread().unwrap()
    });

    cx.simulate_keystrokes("cmd-w");

    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 1, "the Pane is gone");
        assert_eq!(
            view.cockpit.roster().fullscreen(),
            Some(PaneIdentity::Thread(view.panes[0].thread().unwrap())),
            "the surviving Thread fills the screen, like the next tab"
        );
        assert!(
            view.cockpit.parked().unwrap().contains(&closed),
            "and cmd-w still parks, exactly as before"
        );
    });
}

/// #20: parking the last Thread while fullscreen has nothing left to
/// fullscreen — the cockpit falls back to the (empty) grid rather than
/// rendering a blank fullscreen.
#[gpui::test]
fn parking_the_last_fullscreened_thread_falls_back_to_the_grid(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("fullscreen-last", 1);
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-shift-enter", ToggleFullscreen, None),
            KeyBinding::new("cmd-w", CloseThread, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    cx.simulate_keystrokes("cmd-shift-enter");
    view.read_with(cx, |view, _| {
        assert!(
            view.cockpit.roster().fullscreen().is_some(),
            "the premise: fullscreen is on"
        );
    });

    cx.simulate_keystrokes("cmd-w");

    view.read_with(cx, |view, _| {
        assert!(view.panes.is_empty(), "the last Pane is gone");
        assert_eq!(
            view.cockpit.roster().fullscreen(),
            None,
            "and so is the fullscreen"
        );
    });
}

/// #20 edge: the fullscreened Thread parked by a path that knows nothing
/// about fullscreen (a future nav click, the watchdog). The next frame
/// falls back to the grid — never a blank cockpit, never fullscreen on a
/// Thread the operator did not pick.
#[gpui::test]
fn a_fullscreened_thread_parked_externally_falls_back_to_the_grid(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("fullscreen-external", 2);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-shift-enter", ToggleFullscreen, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    cx.simulate_keystrokes("cmd-shift-enter");
    let gone = view.read_with(cx, |view, _| {
        assert!(
            view.cockpit.roster().fullscreen().is_some(),
            "the premise: fullscreen is on"
        );
        view.panes[0].thread().unwrap()
    });

    // Park it the way code that never heard of fullscreen would.
    view.update(cx, |view, cx| {
        view.cockpit.park(gone).unwrap();
        view.sync_panes(cx);
        cx.notify();
    });
    cx.run_until_parked();

    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.roster().fullscreen(),
            None,
            "a fullscreened Thread that vanished falls back to the grid"
        );
        assert_eq!(view.panes.len(), 1, "with the surviving Thread on it");
    });
}
