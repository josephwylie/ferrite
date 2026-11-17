//! Nav acts on several Threads say how each one went: a failed Park or
//! reopen is named in the list, and Undo takes back what parked.

use super::*;

fn sorted_threads(core: &Cockpit) -> Vec<ThreadId> {
    let mut threads = core.threads();
    threads.sort();
    threads
}

fn notice(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, _| view.nav_notice.text().map(|error| error.to_string()))
}

/// A Park of two where the Engine refuses one: the failed one is named in
/// the list and stays picked, the other parks, and ⌘Z takes back the one
/// that parked.
#[gpui::test]
fn a_bulk_park_of_loose_threads_names_the_thread_that_failed(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake) = cockpit("nav-park-refused", 3);
    let threads = sorted_threads(&core);
    core.refuse_park(threads[0]);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    tick(cx);
    let both = [threads[0], threads[2]];
    view.update(cx, |view, cx| {
        view.nav_picked = both.to_vec();
        view.park_listed(&both, cx);
    });
    tick(cx);

    let said = notice(&view, cx).expect("the failure is reported");
    let name = view.read_with(cx, |view, _| view.facts.name(threads[0]).to_string());
    assert!(said.starts_with(&format!("Couldn't park {name}: ")), "{said}");
    assert_eq!(said.matches(';').count(), 0, "one failure, one entry: {said}");
    assert!(cx.debug_bounds("nav-notice").is_some(), "drawn in the list");
    view.read_with(cx, |view, _| {
        assert_eq!(view.nav_picked, vec![threads[0]], "the failed one stays picked");
        assert!(view.facts.parked().contains(&threads[2]));
    });

    cx.simulate_keystrokes("cmd-shift-e cmd-z");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(
            !view.facts.parked().contains(&threads[2]),
            "Undo revived the one that parked"
        );
    });
}

/// Reviving a parked Thread whose log has gone bad says so, in the list,
/// and the Thread stays parked.
#[gpui::test]
fn a_failed_reopen_is_reported(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("nav-reopen-fails", 3);
    let threads = sorted_threads(&core);
    core.park(threads[0]).unwrap();
    std::fs::write(
        std::env::temp_dir()
            .join(format!("ferrite-view-{}-nav-reopen-fails", std::process::id()))
            .join(threads[0].to_string())
            .join("log.jsonl"),
        "not a log\n",
    )
    .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    view.update(cx, |view, cx| view.set_shelf_open(nav::Shelf::Parked, true, cx));
    tick(cx);

    let row_id: &'static str = format!("nav-thread-{}", threads[0].get()).leak();
    let row = cx.debug_bounds(row_id).expect("the parked row");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    tick(cx);
    let said = notice(&view, cx).expect("the failure is reported");
    assert!(said.starts_with("Couldn't reopen "), "{said}");
    assert!(cx.debug_bounds("nav-notice").is_some(), "drawn in the list");
    view.read_with(cx, |view, _| {
        assert!(view.facts.parked().contains(&threads[0]), "still parked");
    });
}

/// A refusal the list shows is in the log too, in the same words, so
/// Copy diagnostics carries it after the notice is gone.
#[gpui::test]
fn a_refused_park_reaches_the_log(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("nav-park-logged", 2);
    let threads = sorted_threads(&core);
    core.refuse_park(threads[0]);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let dir = view.read_with(cx, |view, _| view.prefs.dir.clone());
    ferrite_core::diagnostics::open(&dir);
    tick(cx);

    view.update(cx, |view, cx| view.park_listed(&[threads[0]], cx));
    tick(cx);
    let said = notice(&view, cx).expect("the failure is reported");
    view.update(cx, |view, cx| view.copy_diagnostics(cx));
    cx.run_until_parked();

    let report = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap();
    let log = &report[report.find("--- ferrite.log").unwrap()..];
    assert!(log.contains(&said), "{said:?} not in\n{report}");
}

/// ⌘O reopens a parked Thread the nav's Project scope hides: the scope
/// widens, and its row is in the list.
#[gpui::test]
fn opening_a_thread_outside_the_scope_widens_it(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake) = cockpit("nav-scope-reveal", 2);
    let threads = sorted_threads(&core);
    core.park(threads[1]).unwrap();
    let elsewhere = core
        .register_project(&std::env::temp_dir().canonicalize().unwrap())
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    view.update(cx, |view, cx| view.choose_nav_filter(Some(elsewhere), cx));
    tick(cx);
    let row_id: &'static str = format!("nav-thread-{}", threads[1].get()).leak();
    assert!(cx.debug_bounds(row_id).is_none(), "scoped out of the list");

    cx.simulate_keystrokes("cmd-o");
    tick(cx);
    view.read_with(cx, |view, _| assert_eq!(view.nav_filter, None, "all projects"));
    assert!(cx.debug_bounds(row_id).is_some(), "its row is in the list");
}
