//! A Thread's unsent line outlives its Pane and the window.
#[allow(unused_imports)]
use super::*;
use ferrite_core::drafts::Drafts;

/// A scratch directory for the window's preferences, so no test writes
/// where the operator's own files are.
fn prefs_in(name: &str) -> Preferences {
    let dir = scratch(name);
    std::fs::create_dir_all(&dir).unwrap();
    Preferences {
        dir,
        ..Preferences::ephemeral()
    }
}

/// What is typed is written once the typing pauses, and again when the
/// window quits, for the next launch to find.
#[gpui::test]
fn the_line_typed_is_written_for_the_next_launch(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("drafts-written", 1);
    let thread = core.threads()[0];
    let prefs = prefs_in("drafts-written-prefs");
    let dir = prefs.dir.clone();
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    cx.simulate_input("half a thought");
    assert_eq!(Drafts::load(&dir).get(thread), None, "not on every key");
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(Drafts::load(&dir).get(thread), Some("half a thought"));

    // Sending empties the line, and the draft goes with it.
    view.update(cx, |view, cx| {
        let composer = view.panes[0].composer.clone();
        composer.update(cx, |composer, cx| composer.take(cx));
    });
    cx.run_until_parked();
    view.update(cx, |view, _| view.save_drafts());
    assert_eq!(Drafts::load(&dir).get(thread), None);
}

/// A Thread's Pane opens with the line it was left with.
#[gpui::test]
fn a_pane_opens_with_the_line_it_was_left_with(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("drafts-restored", 1);
    let thread = core.threads()[0];
    let prefs = prefs_in("drafts-restored-prefs");
    let mut drafts = Drafts::default();
    drafts.set(thread, "where I left off");
    drafts.save(&prefs.dir).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes[0].composer.read(cx).text(), "where I left off");
    });
}
