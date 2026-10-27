//! Shortcuts that open or jump to threads.
#[allow(unused_imports)]
use super::*;

/// AC3: one key walks to whoever is waiting, wherever they are in the grid.
#[gpui::test]
fn one_key_jumps_to_the_pane_that_needs_answering(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("jump", 4);
    let group = group_all(&mut core);
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-d", NextDecision, None),
            KeyBinding::new("cmd-]", NextPane, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    fake.streams.borrow()[2].send(decision("perm_03")).unwrap();
    tick(cx);
    view.read_with(cx, |view, _| assert_eq!(view.focused(), 0));

    cx.simulate_keystrokes("cmd-d");

    view.read_with(cx, |view, _| {
        assert_eq!(view.focused(), 2, "focus should land on the blocked Pane");
    });

    // And plain cycling still walks the grid in order.
    cx.simulate_keystrokes("cmd-]");
    view.read_with(cx, |view, _| assert_eq!(view.focused(), 3));
}

/// #20: cmd-t is the browser-tab spelling of a new Thread, and cmd-n —
/// the original — still works beside it. Both keys ride the same
/// cockpit::NewThread; the keymap table carries both rows.
#[gpui::test]
fn cmd_t_opens_a_new_thread_and_cmd_n_still_does(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("cmd-t", 1);
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-t", NewThread, None),
            KeyBinding::new("cmd-n", NewThread, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    cx.simulate_keystrokes("cmd-t");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 2, "cmd-t opened a draft");
        assert!(view.panes[view.focused()].draft().is_some());
    });

    // cmd-n reaches the same action; with that draft still up it
    // re-focuses it rather than stacking a second.
    cx.simulate_keystrokes("cmd-n");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 2, "and cmd-n reuses the standing draft");
        assert!(view.panes[view.focused()].draft().is_some());
    });
}

/// cmd-t on a Thread opens the new draft in that Thread's Project, not
/// wherever the last draft ran: the operator is in that Project now.
#[gpui::test]
fn cmd_t_from_a_solo_thread_starts_in_its_project(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("cmd-t-project", 1);
    let elsewhere = core
        .register_project(&std::env::temp_dir().canonicalize().unwrap())
        .unwrap();
    let here = core.project_id(core.threads()[0]).expect("a bound Thread");
    assert_ne!(here, elsewhere);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-t", NewThread, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.launch_project = Some(elsewhere));
    tick(cx);

    cx.simulate_keystrokes("cmd-t");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.roster().view(), View::Solo);
        let draft = view.panes[view.focused()]
            .draft()
            .expect("cmd-t opened a draft");
        assert_eq!(draft.binding.project(), here);
    });
}

/// In a Group that spans Projects, cmd-t follows the focused member's
/// Project, not the Group's first member or the last draft's.
#[gpui::test]
fn cmd_t_from_a_grouped_thread_starts_in_its_project(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("cmd-t-group-project", 1);
    let first = core.threads()[0];
    let home = core.project_id(first).expect("a bound Thread");
    let repo = repo_in(&scratch("cmd-t-group-other"));
    let other = core.register_project(&repo).unwrap();
    let second = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: repo })
        .unwrap();
    let group = core
        .apply_group(GroupChange::Create { first, second })
        .unwrap()
        .group
        .unwrap();
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-t", NewThread, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| {
        view.launch_project = Some(home);
        view.enter_group(group, cx);
        view.cockpit.focus(PaneIdentity::Thread(second));
    });
    tick(cx);

    cx.simulate_keystrokes("cmd-t");
    view.read_with(cx, |view, _| {
        let draft = view.panes[view.focused()]
            .draft()
            .expect("cmd-t opened a draft");
        assert_eq!(draft.binding.project(), other);
    });
}

/// The Thread cmd-t is pressed on outranks the nav filter: the operator
/// is in that Thread's Project, whatever the nav is narrowed to.
#[gpui::test]
fn cmd_t_from_a_thread_outranks_the_nav_filter(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("cmd-t-over-filter", 1);
    let here = core.project_id(core.threads()[0]).expect("a bound Thread");
    let filtered = core
        .register_project(&std::env::temp_dir().canonicalize().unwrap())
        .unwrap();
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-t", NewThread, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.nav_filter = Some(filtered));
    tick(cx);

    cx.simulate_keystrokes("cmd-t");
    view.read_with(cx, |view, _| {
        let draft = view.panes[view.focused()]
            .draft()
            .expect("cmd-t opened a draft");
        assert_eq!(draft.binding.project(), here);
    });
}

/// A draft left standing in another Project follows cmd-t to the
/// Thread the operator is in: the one it was pressed on, or, pressed on
/// the draft itself, the Thread focus last left.
#[gpui::test]
fn cmd_t_from_a_thread_brings_the_standing_draft_to_its_project(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("cmd-t-standing-draft", 1);
    let thread = core.threads()[0];
    let here = core.project_id(thread).expect("a bound Thread");
    let elsewhere = core
        .register_project(&std::env::temp_dir().canonicalize().unwrap())
        .unwrap();
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-t", NewThread, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.open_draft_in_project(elsewhere, cx));
    tick(cx);
    let project = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.panes[view.focused()]
                .draft()
                .expect("the draft is focused")
                .binding
                .project()
        })
    };

    cx.simulate_keystrokes("cmd-t");
    assert_eq!(
        project(&view, cx),
        here,
        "pressed on the draft, it follows the Thread focus last left"
    );

    view.update(cx, |view, cx| {
        view.open_draft_in_project(elsewhere, cx);
        view.cockpit.focus(PaneIdentity::Thread(thread));
    });
    tick(cx);
    cx.simulate_keystrokes("cmd-t");
    assert_eq!(
        project(&view, cx),
        here,
        "pressed on the Thread, it follows"
    );
}
