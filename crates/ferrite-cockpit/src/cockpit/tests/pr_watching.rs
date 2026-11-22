//! A watched PR in the details card (ADR 0046): T3's eye beside its checks
//! while the Server watches it, a press on it that only stops the watch,
//! and the palette's `Stop watching PR`.
use super::*;
use crate::cockpit::surfaces::SurfaceId;
use ferrite_core::pr_watch::{Target, WatchFact};

fn palette(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.palette_command_rows(window, cx)
                .into_iter()
                .map(|row| row.name.to_string())
                .collect()
        })
    })
}

#[gpui::test]
fn the_eye_shows_while_the_pr_is_watched_and_its_press_stops_the_watch(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("pr-watch-eye", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| view.pr_landed(thread, Some(pr_with_checks()), cx));
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    assert!(cx.debug_bounds("details-card").is_some(), "the card is open");
    assert!(
        cx.debug_bounds("details-pr-watch").is_none(),
        "nobody watches the PR yet"
    );
    assert!(!palette(&view, cx).contains(&"Stop watching PR".to_string()));

    let mut fact = WatchFact::default();
    let target = Target::of_url("https://github.com/o/r/pull/48").unwrap();
    fact.watch(target, None, Some(true), 0);
    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.fixture_pr_watch(thread, fact))
    });
    until(cx, |cx| cx.debug_bounds("details-pr-watch").is_some());
    assert!(palette(&view, cx).contains(&"Stop watching PR".to_string()));
    assert!(
        !palette(&view, cx).contains(&"Watch PR for changes".to_string()),
        "no Server runs a watcher for this window: none can start"
    );

    let eye = cx.debug_bounds("details-pr-watch").unwrap();
    cx.simulate_click(eye.center(), gpui::Modifiers::none());
    until(cx, |cx| cx.debug_bounds("details-pr-watch").is_none());
    assert!(!palette(&view, cx).contains(&"Stop watching PR".to_string()));
    assert_ne!(
        panel_tabs(&view, thread, cx).1,
        Some(SurfaceId::PullRequest(48)),
        "the press only stopped the watch: the PR did not open"
    );
}
