//! Groups: scoping a draft to a group, opening, renaming and dragging members.
#[allow(unused_imports)]
use super::*;

/// Tab on an empty Composer showing a prediction puts it in the line —
/// as editable text, unsent. A guess is not consent to start a turn, so
/// the operator still has to press ↵.
#[gpui::test]
fn tab_accepts_the_predicted_follow_up_without_sending_it(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("tab-accept-suggestion", 1);
    let thread = core.threads()[0];
    // A finished turn, so the Thread is idle with a response to follow up.
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Fixed the decoder.".into(),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    core.pump();
    core.deliver_suggestion(thread, "Run the tests".into());
    core.pump();

    bind_band_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.read_with(cx, |view, cx| {
        assert!(
            view.panes[view.focused()].composer.read(cx).is_empty(),
            "the line starts empty"
        );
    });

    cx.simulate_keystrokes("tab");

    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.panes[view.focused()].composer.read(cx).text(),
            "Run the tests",
            "tab puts the prediction in the line"
        );
        assert!(
            view.cockpit
                .thread(view.panes[view.focused()].thread().unwrap())
                .unwrap()
                .transcript()
                .blocks()
                .iter()
                .all(|block| !matches!(&block.body, Body::Prompt(_))),
            "nothing was sent: accepting is not submitting"
        );
    });

    // A second tab must not append it again — the line is no longer
    // empty, so tab is back to being the disclosure walk.
    cx.simulate_keystrokes("tab");
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.panes[view.focused()].composer.read(cx).text(),
            "Run the tests"
        );
    });
}

#[gpui::test]
fn group_scope_is_one_view_and_cmd_w_parks_the_focused_member_in_one_press(
    cx: &mut TestAppContext,
) {
    let (mut core, _fake) = cockpit("group-scope-close", 2);
    let threads = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-w", CloseThread, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));

    view.update(cx, |view, cx| view.enter_group(group, cx));
    view.read_with(cx, |view, _| {
        assert!(matches!(view.cockpit.roster().view(), View::Group(_)));
        assert_eq!(view.visible_indices().len(), 2);
    });
    cx.simulate_keystrokes("cmd-w");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.threads().len(), 1, "one press parks it");
        assert!(view.cockpit.parked().unwrap().contains(&threads[0]));
        assert!(view.cockpit.groups().of(threads[0]).is_none());
        assert_eq!(view.visible_indices().len(), 1);
        assert_eq!(view.cockpit.roster().focused_thread(), Some(threads[1]));
    });
}

#[gpui::test]
fn new_thread_shortcut_from_a_group_adds_the_thread_to_that_group(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("group-shortcut-draft", 2);
    let threads = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-t", NewThread, None),
            KeyBinding::new("enter", Submit, None),
        ])
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    cx.simulate_keystrokes("cmd-t");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.roster().view(), View::Group(group));
        assert_eq!(view.visible_indices().len(), 3, "the draft Pane is visible");
        assert!(view.panes[view.focused()].draft().is_some());
        assert_eq!(
            view.cockpit.groups().iter().next().unwrap().members.len(),
            2,
            "the Draft has no durable membership before sending"
        );
    });
    cx.simulate_input("build it");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.groups().iter().next().unwrap().members.len(),
            3,
            "the shortcut-created Thread joins the Group"
        );
        let thread = view.cockpit.roster().focused_thread().unwrap();
        assert_eq!(
            view.cockpit.groups().of(thread).map(|group| group.id),
            Some(group)
        );
        assert!(view.panes.iter().all(|pane| pane.draft().is_none()));
    });
}

/// A draft left standing from an earlier press must not swallow the
/// next one's placement: pressing add from a loose Thread still aims
/// the draft at a new Group with that Thread.
#[gpui::test]
fn a_standing_draft_is_re_aimed_at_the_new_group(cx: &mut TestAppContext) {
    let (core, _) = cockpit("re-aim-standing-draft", 1);
    let original = core.threads()[0];
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| {
        // A loose draft is up first, as an empty launch or an earlier
        // press would leave it.
        view.open_draft(DraftTarget::Main, cx);
        view.open_draft_with_placement(
            DraftTarget::Main,
            DraftPlacement::NewGroupWith(original),
            cx,
        );
        let draft = view.panes[view.focused()]
            .identity
            .draft()
            .expect("the standing draft kept focus");
        assert_eq!(
            view.panes.iter().filter(|p| p.draft().is_some()).count(),
            1,
            "still one draft"
        );
        assert_eq!(
            view.cockpit
                .roster()
                .draft_scope(draft)
                .unwrap()
                .new_group_with,
            Some(original),
            "the standing draft was re-aimed at the new Group"
        );
    });
}

#[gpui::test]
fn a_new_group_from_a_loose_thread_scopes_the_draft_to_a_new_group(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("titlebar-add-new-group", 1);
    let original = core.threads()[0];
    cx.update(|cx| cx.bind_keys([KeyBinding::new("enter", Submit, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    // The palette's `new group`.
    view.update(cx, |view, cx| {
        view.open_draft_with_placement(
            DraftTarget::Main,
            DraftPlacement::NewGroupWith(original),
            cx,
        )
    });
    cx.run_until_parked();

    view.read_with(cx, |view, _| {
        let draft = view.panes[view.focused()]
            .identity
            .draft()
            .expect("the button focused a draft");
        assert_eq!(
            view.cockpit
                .roster()
                .draft_scope(draft)
                .unwrap()
                .new_group_with,
            Some(original)
        );
        assert_eq!(
            view.cockpit.visible(),
            [PaneIdentity::Thread(original), PaneIdentity::Draft(draft)],
            "the loose Thread and its focused Draft are shown together"
        );
        assert_eq!(
            layout::grid_shape(view.cockpit.visible().len(), view.board.get()).0,
            2
        );
    });

    cx.simulate_input("work beside the original thread");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        let focused = view
            .cockpit
            .roster()
            .focused_thread()
            .expect("the sent Draft became the focused Thread");
        assert_ne!(focused, original);
        let group = view
            .cockpit
            .groups()
            .of(focused)
            .expect("the new Thread belongs to a Group");
        assert_eq!(group.members, [original, focused]);
        assert_eq!(view.cockpit.roster().view(), View::Group(group.id));
    });
}

#[gpui::test]
fn a_new_thread_in_a_group_keeps_the_draft_in_that_group(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("titlebar-add-in-group", 2);
    let threads = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);
    view.update(cx, |view, cx| {
        view.open_draft_with_placement(DraftTarget::Main, DraftPlacement::CurrentGroup, cx)
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let draft = view.cockpit.roster().focused().unwrap().draft().unwrap();
        let scope = view.cockpit.roster().draft_scope(draft).unwrap();
        assert_eq!(scope.group, Some(group));
        assert_eq!(scope.new_group_with, None);
        assert_eq!(view.cockpit.roster().view(), View::Group(group));
        assert_eq!(view.cockpit.visible().len(), 3);
    });
}

#[gpui::test]
fn a_draft_close_button_discards_it_and_restores_the_original_thread(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("draft-close-button", 1);
    let original = core.threads()[0];
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.update(cx, |view, cx| {
        view.open_draft_with_placement(
            DraftTarget::Main,
            DraftPlacement::NewGroupWith(original),
            cx,
        )
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        view.cockpit
            .roster()
            .focused()
            .and_then(PaneIdentity::draft)
            .expect("the new Draft is focused")
    });

    let close = cx
        .debug_bounds("discard-draft")
        .expect("the Draft close button is visible");
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    cx.run_until_parked();

    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.roster().panes(),
            [PaneIdentity::Thread(original)]
        );
        assert_eq!(view.cockpit.visible(), [PaneIdentity::Thread(original)]);
        assert_eq!(
            view.cockpit.roster().focused_thread(),
            Some(original),
            "discarding the Draft returns focus to the surviving Thread"
        );
        assert!(view.cockpit.groups().of(original).is_none());
    });
}

/// ADR 0014: the titlebar band says nothing — no location, no build
/// badge, no doors — and there is no bar at the window's foot
/// (components.md cut 9). The sidebar toggle and the Activity bell are the
/// nav head's (§5.1.1).
#[gpui::test]
fn the_titlebar_band_says_nothing_and_no_bar_is_left(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("titlebar-says-nothing", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    for gone in [
        "titlebar-location",
        "titlebar-commands",
        "titlebar-dev-badge",
        "nav-chrome",
        "bottom-bar",
        "notifications-bell",
        "settings-gear",
        "nav-activity",
    ] {
        assert!(cx.debug_bounds(gone).is_none(), "{gone} is gone");
    }
    let toggle = cx.debug_bounds("nav-collapse").expect("nav-collapse");
    assert!(
        toggle.bottom() <= px(ferrite_ui::theme::WIN_CHROME_H),
        "the toggle is the nav head's (§5.1.1): {toggle:?}"
    );
    let search = cx.debug_bounds("nav-search").expect("nav-search");
    assert!(
        search.top() >= px(ferrite_ui::theme::WIN_CHROME_H),
        "the search field sits in the row under the head: {search:?}"
    );
}

/// Pressing new-thread with a draft already up re-aims that draft
/// rather than stacking a second one.
#[gpui::test]
fn a_second_new_thread_reuses_the_standing_draft(cx: &mut TestAppContext) {
    let (core, _) = cockpit("second-draft", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| {
        view.open_draft(DraftTarget::Main, cx);
        let drafts = |view: &CockpitView| view.panes.iter().filter(|p| p.draft().is_some()).count();
        assert_eq!(drafts(view), 1);
        view.open_draft(DraftTarget::Main, cx);
        assert_eq!(drafts(view), 1);
        view.open_draft_in_current_view(DraftTarget::New, cx);
        assert_eq!(drafts(view), 1);
        assert!(view.panes[view.focused()].draft().is_some());
        assert_eq!(
            view.focused_draft_mut().unwrap().binding.target(),
            &DraftTarget::New
        );
    });
}

#[gpui::test]
fn multi_project_groups_keep_bindings_and_filter_only_navigation(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("multi-project-group", 2);
    let original = core.threads();
    let first_project = core.project_id(original[0]).unwrap();
    let repo = repo_in(&scratch("multi-project-other"));
    let second_project = core.register_project(&repo).unwrap();
    let group = core
        .apply_group(GroupChange::Create {
            first: original[0],
            second: original[1],
        })
        .unwrap()
        .group
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| {
        view.enter_group(group, cx);
        view.open_draft(DraftTarget::Main, cx);
        view.focused_draft_mut()
            .unwrap()
            .binding
            .choose_project(second_project);
        view.panes[view.focused()]
            .composer
            .update(cx, |composer, cx| {
                composer.set("join from another project".into(), cx)
            });
        view.bootstrap_draft(cx);
        let joined = view
            .focused_thread()
            .expect("different-Project Thread created");
        view.cockpit
            .apply_group(GroupChange::Join {
                thread: joined,
                group,
                index: None,
            })
            .unwrap();
        assert_eq!(
            view.cockpit.groups().get(group).unwrap().members,
            [original[0], original[1], joined]
        );
        assert_eq!(
            view.cockpit
                .thread(joined)
                .unwrap()
                .workspace()
                .unwrap()
                .cwd(),
            repo.canonicalize().unwrap()
        );
        assert_eq!(view.cockpit.project_id(original[0]), Some(first_project));
        assert_eq!(view.cockpit.project_id(joined), Some(second_project));
        view.enter_group(group, cx);
        let listed = |view: &CockpitView| -> Vec<ThreadId> {
            let mut threads: Vec<ThreadId> =
                view.nav_state().open_rows().map(|row| row.thread).collect();
            threads.sort();
            threads
        };
        let mut members = vec![original[0], original[1], joined];
        members.sort();
        assert_eq!(
            listed(view),
            members,
            "a cross-Project Group's members are each one card"
        );
        view.nav_filter = Some(second_project);
        assert_eq!(
            listed(view),
            [joined],
            "filtered, the list holds that Project's Threads alone"
        );
        assert_eq!(
            view.visible_indices().len(),
            3,
            "filter must not shrink the Cockpit"
        );
        view.cockpit.park(joined).unwrap();
        view.sync_panes(cx);
        view.enter_group(group, cx);
        assert_eq!(
            view.visible_indices().len(),
            3,
            "opening the Group lays every member, a parked one as its tile"
        );
        assert!(
            view.cockpit.thread(joined).is_none(),
            "no Session spent on it"
        );
        view.cockpit.wake(joined).unwrap();
        view.sync_panes(cx);
        assert_eq!(
            view.cockpit
                .thread(joined)
                .unwrap()
                .workspace()
                .unwrap()
                .cwd(),
            repo.canonicalize().unwrap()
        );
    });
}

#[gpui::test]
fn solo_group_draft_and_close_follow_the_persisted_view_state_machine(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("group-view-state", 3);
    let threads = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    core.apply_group(GroupChange::Join {
        thread: threads[2],
        group,
        index: None,
    })
    .unwrap();
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-w", CloseThread, None),
            KeyBinding::new("cmd-t", NewThread, None),
            KeyBinding::new("enter", Submit, None),
        ])
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));

    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.roster().view(), View::Solo);
        assert_eq!(view.visible_indices(), [0]);
    });
    view.update(cx, |view, cx| view.enter_group(group, cx));
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.roster().view(), View::Group(group));
        let visible: Vec<_> = view
            .visible_indices()
            .into_iter()
            .filter_map(|index| view.panes[index].thread())
            .collect();
        assert_eq!(visible, threads);
    });

    cx.simulate_keystrokes("cmd-t");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.roster().view(), View::Group(group));
        assert_eq!(view.visible_indices().len(), 4, "Draft joins the view");
        assert_eq!(view.cockpit.groups().get(group).unwrap().members, threads);
    });
    cx.simulate_input("build it");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.groups().get(group).unwrap().members.len(), 4);
        assert_eq!(view.cockpit.roster().view(), View::Group(group));
    });

    // cmd-w parks a member in one press: out of the Group, into the
    // parked rows, no loose open Thread left to park again.
    cx.simulate_keystrokes("cmd-w");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.roster().view(), View::Group(group));
        assert_eq!(view.visible_indices().len(), 3);
        assert_eq!(view.cockpit.threads().len(), 3, "one press parks");
        assert_eq!(view.cockpit.parked().unwrap().len(), 1);
    });
    cx.simulate_keystrokes("cmd-w cmd-w");
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.roster().view(), View::Solo);
        assert!(view.cockpit.groups().get(group).is_none());
        assert_eq!(view.visible_indices().len(), 1);
        assert_eq!(view.cockpit.threads().len(), 1, "only the survivor runs");
        assert_eq!(view.cockpit.parked().unwrap().len(), 3);
    });
}

#[gpui::test]
fn a_cards_title_renames_on_its_second_press_only(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("card-pointer-rename", 2);
    let threads = core.threads();
    core.rename_thread(threads[1], "Existing title").unwrap();
    cx.update(|cx| cx.bind_keys([KeyBinding::new("enter", Submit, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    cx.run_until_parked();
    let title: &'static str = format!("rename-thread-{}", threads[1].get()).leak();

    let at = cx.debug_bounds(title).expect("the card's title is rendered");
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    view.read_with(cx, |view, _| {
        assert!(
            view.rename.is_none(),
            "one click never opens the inline editor"
        );
        assert_eq!(view.focused_thread(), Some(threads[1]), "it opens the Thread");
    });
    cx.run_until_parked();
    let at = cx.debug_bounds(title).unwrap();
    press(cx, at.center(), 2);
    view.read_with(cx, |view, cx| {
        let editor = &view.rename.as_ref().expect("inline Thread editor").1;
        assert_eq!(editor.read(cx).text(), "Existing title");
    });
    view.update(cx, |view, cx| {
        let editor = view.rename.as_ref().unwrap().1.clone();
        editor.update(cx, |line, cx| line.set("Saved title".into(), cx));
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.rename.is_none());
        assert_eq!(
            view.cockpit.thread_title(threads[1]).unwrap().as_deref(),
            Some("Saved title")
        );
    });
}

/// A Group whose member rows run several viewports past the nav's bottom
/// still reaches its last member by scrolling.
#[gpui::test]
fn a_long_group_remains_reachable_through_nav_scroll(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("group-nav-scroll", 24);
    group_all(&mut core);
    let last = format!("nav-thread-{}", core.threads().last().unwrap().get());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(320.)));
    cx.run_until_parked();
    let before = debug_bounds(cx, last.clone()).unwrap();
    assert!(
        before.center().y > px(320.),
        "the last row starts below the viewport"
    );

    // Past the end and clamped there, rather than a measured distance a
    // change of row height would make stop short.
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: gpui::point(px(100.), px(200.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-40_000.))),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::default(),
    });
    cx.run_until_parked();
    let after = debug_bounds(cx, last).unwrap();
    assert!(
        after.center().y < px(320.),
        "scrolling exposes the final member: {after:?}"
    );
}

#[gpui::test]
fn inline_title_rename_disarms_the_thread_composer_history_context_and_hint(
    cx: &mut TestAppContext,
) {
    let (mut core, _) = cockpit("prompt-history-rename", 1);
    let thread = core.threads()[0];
    core.send(thread, "history".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.update(cx, |view, cx| {
        view.start_rename(RenameTarget::Thread(thread), cx)
    });
    tick(cx);
    view.read_with(cx, |view, _| assert!(view.rename.is_some()));
    cx.update(|window, cx| {
        assert!(
            !view
                .read(cx)
                .history_available(0, view.read(cx).level_now(window)),
            "the visible Thread Composer must not advertise or arm history while rename owns focus"
        )
    });
}
