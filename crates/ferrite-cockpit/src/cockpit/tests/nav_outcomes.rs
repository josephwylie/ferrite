//! Nav acts on several Threads say how each one went: a failed Park or
//! reopen is named in the list, and Undo puts a Thread back in its Group.

use super::*;

fn sorted_threads(core: &Cockpit) -> Vec<ThreadId> {
    let mut threads = core.threads();
    threads.sort();
    threads
}

fn notice(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, _| view.group_error.as_ref().map(|error| error.to_string()))
}

/// A Park of two where one cannot park (its Group's file cannot be
/// written): the other parks, the failed one is named and stays picked, and
/// ⌘Z takes back only the one.
#[gpui::test]
fn a_bulk_park_names_the_thread_that_failed(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    bind_production_keys(cx);
    let (mut core, _fake) = cockpit("nav-park-fails", 3);
    let threads = sorted_threads(&core);
    core.apply_group(GroupChange::Create { first: threads[0], second: threads[1] }).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    tick(cx);
    let store = std::env::temp_dir().join(format!("ferrite-view-{}-nav-park-fails", std::process::id()));
    let writable = std::fs::metadata(&store).unwrap().permissions();
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o555)).unwrap();
    let both = [threads[0], threads[2]];
    view.update(cx, |view, cx| {
        view.nav_picked = both.to_vec();
        view.park_listed(&both, cx);
    });
    std::fs::set_permissions(&store, writable).unwrap();
    tick(cx);

    let said = notice(&view, cx).expect("the failure is reported");
    assert!(said.starts_with("Couldn't park "), "{said}");
    assert_eq!(said.matches(';').count(), 0, "one failure, one entry: {said}");
    assert!(cx.debug_bounds("nav-notice").is_some(), "drawn in the list");
    view.read_with(cx, |view, _| {
        assert_eq!(view.nav_picked, vec![threads[0]], "the failed one stays picked");
        assert_eq!(view.facts.parked(), vec![threads[2]]);
    });

    cx.simulate_keystrokes("cmd-shift-e cmd-z");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.facts.parked().is_empty(), "Undo revived the one that parked");
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

/// Undo of a Park puts the Thread back in the Group it left, at its place;
/// a Group the Park dissolved is made again with its title and order.
#[gpui::test]
fn undo_restores_the_groups_a_park_left(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake) = cockpit("nav-undo-groups", 3);
    let threads = sorted_threads(&core);
    let group = group_all(&mut core);
    core.apply_group(GroupChange::Rename { group, title: "Trio".into() }).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    tick(cx);
    let members = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.cockpit
                .groups()
                .iter()
                .map(|group| (group.title.clone(), group.members.clone()))
                .collect::<Vec<_>>()
        })
    };
    let before = members(&view, cx);
    assert_eq!(before, vec![("Trio".to_string(), threads.clone())]);

    view.update(cx, |view, cx| view.park_listed(&[threads[0]], cx));
    tick(cx);
    assert_eq!(members(&view, cx), vec![("Trio".to_string(), vec![threads[1], threads[2]])]);
    cx.simulate_keystrokes("cmd-shift-e cmd-z");
    tick(cx);
    assert_eq!(members(&view, cx), before, "back at its place in the Group");

    view.update(cx, |view, cx| view.park_listed(&[threads[0], threads[1]], cx));
    tick(cx);
    assert!(members(&view, cx).is_empty(), "the Group dissolved with the second Park");
    cx.simulate_keystrokes("cmd-shift-e cmd-z");
    tick(cx);
    assert_eq!(members(&view, cx), before, "made again, whole, with its title");
    assert!(notice(&view, cx).is_none());
}
