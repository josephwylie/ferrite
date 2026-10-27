//! The checks a PR counts and the card that opens them.
#[allow(unused_imports)]
use super::*;

/// F-11: the checks card is the palette's `checks` command — it names
/// every run behind the PR's rollup, grouped by its workflow, and escape
/// closes it.
#[gpui::test]
fn the_checks_command_opens_the_checks_it_counts(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("ci-card", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    view.update(cx, |view, cx| {
        view.facts.pr_answered(thread, Some(pr_with_checks()));
        cx.notify();
    });
    tick(cx);
    assert!(
        cx.debug_bounds("ci-mark-1").is_none(),
        "no mark in the chrome"
    );
    assert!(
        cx.debug_bounds("context-checks-card").is_none(),
        "the card is closed until it is asked for"
    );
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_checks(window, cx)));
    tick(cx);
    assert!(cx.debug_bounds("context-checks-card").is_some());
    for row in ["check-row-0", "check-row-1", "check-row-2"] {
        assert!(
            cx.debug_bounds(row).is_some(),
            "every run behind the rollup gets a row"
        );
    }
    view.read_with(cx, |view, _| assert!(view.context_checks.is_some()));
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.context_checks.is_none()));
}

/// A PR whose rollup is empty claims nothing about CI, so the checks
/// command opens nothing — the card would have had nothing to list.
#[gpui::test]
fn a_pr_without_checks_opens_no_checks_card(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("ci-no-checks", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    view.update(cx, |view, cx| {
        let mut pr = pr_with_checks();
        pr.checks = None;
        pr.runs.clear();
        view.facts.pr_answered(thread, Some(pr));
        cx.notify();
    });
    tick(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_checks(window, cx)));
    view.read_with(cx, |view, _| assert!(view.context_checks.is_none()));
}
