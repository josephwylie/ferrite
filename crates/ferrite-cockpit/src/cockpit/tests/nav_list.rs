//! The nav: thread rows, the Activity view and the parked list.
#[allow(unused_imports)]
use super::*;

/// A Thread row is one quiet line (§5.1.5): the title on its Project's
/// label (Claude's one left edge), 30 high, nothing under it and no
/// provider logo.
#[gpui::test]
fn a_thread_row_is_one_quiet_line(cx: &mut TestAppContext) {
    let (core, _) = cockpit("nav-age-align", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    cx.run_until_parked();
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

    let id = |name: &str| -> &'static str { format!("{name}-{}", thread.get()).leak() };
    let row = cx.debug_bounds(id("nav-thread")).expect("the row");
    let title = cx.debug_bounds(id("nav-title")).expect("its title");
    assert_eq!(row.size.height, px(ferrite_ui::theme::NAV_ROW_H));
    assert!(
        (title.left() - (row.left() + px(ferrite_ui::theme::NAV_ROW_PAD_X))).abs() < px(0.5),
        "the title on its Project's label: {title:?} in {row:?}"
    );
    assert!(
        cx.debug_bounds(id("nav-mark")).is_none(),
        "no provider logo on a nav row"
    );
}

/// C8: while Threads wait, their own rows say so — the yellow mark and
/// what each waits for — and their Group's row rolls it up; there is
/// no second strip of them. ⌘D walks them oldest request first, and —
/// the nav being open — no system toast card shows.
#[gpui::test]
fn two_decisions_mark_their_rows_and_their_group_and_no_toast(cx: &mut TestAppContext) {
    use gpui::component::WindowExt as _;
    let (mut core, fake) = cockpit("needs-you-rows", 3);
    let threads = core.threads();
    let group = group_all(&mut core);
    core.enter_group(group).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);

    // The third Thread asks first, then the first.
    fake.streams.borrow()[2].send(decision("perm_3")).unwrap();
    tick(cx);
    fake.streams.borrow()[0].send(decision("perm_1")).unwrap();
    tick(cx);

    view.read_with(cx, |view, _| {
        let state = view.nav_state();
        let block = &state.groups[0];
        assert_eq!(
            nav::roll_up(&block.members),
            Some(nav::RowStatus::NeedsYou),
            "the Group rolls up"
        );
        for at in [0, 2] {
            let row = view.thread_row(threads[at]);
            assert_eq!(row.status, nav::RowStatus::NeedsYou);
            assert_eq!(
                row.tail,
                nav::NavTail::Waiting(ferrite_ui::theme::words::APPROVAL),
                "the word says what it waits for"
            );
        }
        assert_ne!(view.thread_row(threads[1]).status, nav::RowStatus::NeedsYou);
    });
    assert!(
        cx.debug_bounds("nav-needs-you").is_none(),
        "no strip: the rows are the list"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.cockpit.answer_target()),
        Some(threads[2]),
        "⌘D answers the request that has waited longest"
    );
    assert!(
        cx.debug_bounds("nav-needs-you").is_none(),
        "no strip restates the waiting rows"
    );
    cx.update(|window, cx| {
        assert_eq!(
            window.notifications(cx).len(),
            0,
            "the rows are the queue: no toast card"
        )
    });
}

/// §5.1.2: the Inbox (⌘I; the bell's Activity view took its row) stands
/// in the tree's place — the blocked Threads oldest-first as their
/// two-line rows, then `Recent`. Esc and ⌘I again return to the tree, and
/// ⌘D still walks the queue.
#[gpui::test]
fn the_needs_you_view_lists_the_queue_in_the_trees_place(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("needs-you-view", 3);
    let threads = core.threads();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    fake.streams.borrow()[2].send(decision("perm_3")).unwrap();
    tick(cx);
    fake.streams.borrow()[0].send(decision("perm_1")).unwrap();
    tick(cx);
    assert!(cx.debug_bounds("nav-view-needs-you-list").is_none());
    assert!(
        cx.debug_bounds("nav-view-needs-you").is_none(),
        "no Inbox row: the bell is the door"
    );

    cx.simulate_keystrokes("cmd-i");
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.needs_you_shown()));
    assert!(cx.debug_bounds("nav-view-needs-you-list").is_some());
    assert!(
        cx.debug_bounds("nav-project-section-0").is_none(),
        "in the tree's place"
    );
    let first = cx.debug_bounds("notice-row-0").expect("the oldest request");
    let second = cx.debug_bounds("notice-row-1").expect("then the next");
    assert!(first.top() < second.top());
    let row = |thread: ThreadId| -> &'static str { format!("nav-thread-{}", thread.get()).leak() };
    let oldest = cx.debug_bounds(row(threads[2])).expect("its row");
    assert!(first.contains(&oldest.center()), "oldest first");
    assert_eq!(oldest.size.height, px(ferrite_ui::theme::NAV_ROW_H));
    // The queue stays flat: no Project heading, every row on the Notices'
    // column, so its order is the queue's.
    assert!(cx.debug_bounds("nav-view-project-0").is_none());
    let title = cx
        .debug_bounds(format!("nav-title-{}", threads[2].get()).leak())
        .expect("its title");
    assert!(
        (title.left() - oldest.left() - px(ferrite_ui::theme::NAV_TITLE_X)).abs() < px(0.5),
        "on the Notices' title column"
    );
    assert!(cx
        .debug_bounds(row(threads[0]))
        .is_some_and(|next| second.contains(&next.center())));
    assert_eq!(
        view.read_with(cx, |view, _| view.cockpit.answer_target()),
        Some(threads[2]),
        "⌘D answers the request that has waited longest"
    );

    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!view.read_with(cx, |view, _| view.needs_you_shown()), "esc");
    assert!(cx.debug_bounds("nav-project-section-0").is_some());
    assert!(
        cx.debug_bounds("nav-needs-you").is_none(),
        "the tree has no strip"
    );
    cx.simulate_keystrokes("cmd-i");
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.needs_you_shown()));
    cx.simulate_keystrokes("cmd-i");
    tick(cx);
    assert!(
        !view.read_with(cx, |view, _| view.needs_you_shown()),
        "⌘I again returns to the tree"
    );
}

/// N-3: rows follow creation order, independent of activity. A Thread
/// moving from quiet to working and on to done keeps its row index —
/// with the pointer nowhere near the column, so nothing is merely held.
#[gpui::test]
fn a_thread_keeps_its_row_through_working_and_done(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("nav-stable-order", 3);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let order = |view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.nav_state()
                .ordered_solos()
                .iter()
                .map(|row| row.thread)
                .collect::<Vec<_>>()
        })
    };
    let before = order(&view, cx);
    let mut created = threads.clone();
    created.sort();
    assert_eq!(before, created, "creation order");
    let last = *before.last().unwrap();
    let stream = threads.iter().position(|thread| *thread == last).unwrap();
    assert!(!view.read_with(cx, |view, _| view.nav_hovered));

    fake.streams.borrow()[stream]
        .send(SessionEvent::TextDelta {
            text: "thinking".into(),
        })
        .unwrap();
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.thread_row(last).status),
        nav::RowStatus::Working,
        "the premise: the bottom row started work"
    );
    assert_eq!(order(&view, cx), before, "working moves no row");

    fake.streams.borrow()[stream]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    assert_ne!(
        view.read_with(cx, |view, _| view.thread_row(last).status),
        nav::RowStatus::Working,
        "the premise: the turn is over"
    );
    assert_eq!(order(&view, cx), before, "nor does finishing");
}

/// The Activity view (the bell, Codex's): `Priority` holds what needs the
/// operator, oldest request first, then what works, in creation order; a
/// finish moves a Thread on to `Today`. The bell shows it in the tree's
/// place, filled, and again gives the tree back; a row says its title and
/// its mark, and a press lands on its Thread. The tree carries no status
/// sections of its own.
#[gpui::test]
fn the_activity_view_lists_priority_then_today(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("activity-view", 5);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    assert!(activity(&view, cx).0.is_empty(), "nothing in flight");
    let send = |index: usize, event: SessionEvent| {
        fake.streams.borrow()[index].send(event).unwrap();
    };
    let working = || SessionEvent::TextDelta {
        text: "thinking".into(),
    };
    send(4, working());
    send(1, working());
    tick(cx);
    assert_eq!(activity(&view, cx).0, vec![threads[1], threads[4]]);
    send(2, working());
    tick(cx);
    send(
        2,
        SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        },
    );
    tick(cx);
    send(3, decision("perm-a"));
    tick(cx);
    send(1, decision("perm-b"));
    tick(cx);
    let (priority, today) = activity(&view, cx);
    assert_eq!(priority, vec![threads[3], threads[1], threads[4]]);
    assert!(today.contains(&threads[2]), "a finish moves on to Today");
    assert!(today.iter().all(|thread| !priority.contains(thread)));

    // The tree says no section of status; the bell swaps it for the view.
    assert!(cx.debug_bounds("nav-activity-list").is_none());
    assert!(cx.debug_bounds("nav-projects-heading").is_some());
    let bell = cx.debug_bounds("nav-activity").expect("the bell");
    cx.simulate_click(bell.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.nav_view),
        nav::NavView::Activity
    );
    assert!(
        cx.debug_bounds("nav-projects-heading").is_none(),
        "in the tree's place"
    );
    let at = |cx: &mut gpui::VisualTestContext, key: String| {
        cx.debug_bounds(key.clone().leak())
            .unwrap_or_else(|| panic!("{key} drawn"))
    };
    let (head, later) = (
        at(cx, "nav-activity-priority".into()),
        at(cx, "nav-activity-today".into()),
    );
    assert!(head.bottom() <= later.top(), "Priority over Today");
    let running = at(cx, format!("nav-activity-row-{}", threads[4].get()));
    let mark = at(cx, format!("nav-activity-state-{}", threads[4].get()));
    assert!(mark.left() > running.center().x, "its spinner at the right");
    assert!(
        running.size.height > px(ferrite_ui::theme::NAV_ROW_H),
        "title, then the Project"
    );

    // A press lands on the Thread.
    let done = at(cx, format!("nav-activity-row-{}", threads[2].get()));
    cx.simulate_click(done.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.cockpit.roster().focused_thread()),
        Some(threads[2])
    );

    // The bell again: the tree.
    let bell = cx.debug_bounds("nav-activity").expect("the bell");
    cx.simulate_click(bell.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.nav_view),
        nav::NavView::Tree
    );
}

/// C8 under `sort: recent`: the tree never re-sorts under the pointer.
/// While the pointer is inside the nav the rows hold the order they
/// were drawn in; leaving re-ranks them.
#[gpui::test]
fn sort_recent_holds_still_under_the_pointer(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-hold-order", 3);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.update(cx, |view, cx| {
        view.set_thread_order(ThreadListOrder::Recent, cx);
        assert_eq!(
            view.prefs.settings.thread_list_order,
            ThreadListOrder::Recent
        );
        let drawn: Vec<ThreadId> = view
            .nav_state()
            .ordered_solos()
            .iter()
            .map(|row| row.thread)
            .collect();
        let held = vec![drawn[2], drawn[0], drawn[1]];
        // The pointer is inside, and the last drawn order differs from
        // what recency would say: it is kept.
        view.nav_hovered = true;
        *view.nav_snapshot.borrow_mut() = Some(NavSnapshot {
            rows: held.clone(),
            focused: view.cockpit.roster().focused_thread(),
            ranks: Default::default(),
        });
        let order: Vec<ThreadId> = view
            .nav_state()
            .ordered_solos()
            .iter()
            .map(|row| row.thread)
            .collect();
        assert_eq!(order, held, "nothing moves under the pointer");
        view.nav_hovered = false;
        let order: Vec<ThreadId> = view
            .nav_state()
            .ordered_solos()
            .iter()
            .map(|row| row.thread)
            .collect();
        assert_eq!(order, drawn, "leaving re-ranks");
    });
}

/// Rule 2.7.7: a window too narrow for the column beside a board folds
/// the nav — shown, never saved — and unfolds only past the threshold
/// plus its hysteresis. cmd-B overrides it.
#[gpui::test]
fn a_narrow_window_folds_the_nav_without_saving_it(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{NAV_AUTO_RAIL_BOARD_W, NAV_AUTO_RAIL_HYSTERESIS, NAV_WIDTH};
    let (core, _fake) = cockpit("nav-auto-rail", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    let saved = view.read_with(cx, |view, _| view.prefs.settings.nav_collapsed);
    // A Solo Pane leaves no gutter at the window's edge (theme rule 2).
    let threshold = NAV_WIDTH + NAV_AUTO_RAIL_BOARD_W;
    cx.simulate_resize(gpui::size(px(720.), px(900.)));
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.nav_railed()),
        "720px folds"
    );
    assert!(
        (cx.debug_bounds("nav-column").unwrap().size.width - px(nav::FOLDED_WIDTH)).abs() < px(0.5),
        "the column is folded to the rail"
    );
    assert!(
        cx.debug_bounds("nav-rail-collapse").is_some(),
        "the rail keeps the sidebar toggle at its top"
    );
    assert_eq!(
        view.read_with(cx, |view, _| (
            view.nav_collapsed,
            view.prefs.settings.nav_collapsed
        )),
        (false, saved),
        "nothing is saved"
    );
    // Just past the threshold, the hysteresis holds the rail.
    cx.simulate_resize(gpui::size(px(threshold + 1.), px(900.)));
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.nav_railed()), "held");
    cx.simulate_resize(gpui::size(
        px(threshold + NAV_AUTO_RAIL_HYSTERESIS + 1.),
        px(900.),
    ));
    tick(cx);
    assert!(!view.read_with(cx, |view, _| view.nav_railed()), "restored");
    // Narrow again; cmd-B holds the column open.
    cx.simulate_resize(gpui::size(px(720.), px(900.)));
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.nav_railed()));
    view.update(cx, |view, cx| view.toggle_nav_now(cx));
    tick(cx);
    assert!(
        !view.read_with(cx, |view, _| view.nav_railed()),
        "cmd-B wins"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.prefs.settings.nav_collapsed),
        saved
    );
}

/// ⌘1…⌘9 (R5) focus the shown board's Panes in head order — reading
/// order — whatever needs answering; a digit past the board does
/// nothing, and in Solo ⌘1 is the Solo Pane.
#[gpui::test]
fn command_digits_focus_the_boards_panes_in_head_order(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("board-digits", 3);
    let group = group_all(&mut core);
    core.enter_group(group).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    fake.streams.borrow()[1].send(decision("perm_2")).unwrap();
    tick(cx);
    let order: Vec<ThreadId> = cx.update(|window, cx| {
        let view = view.read(cx);
        view.board_order(window)
            .into_iter()
            .map(|index| view.panes[index].thread().unwrap())
            .collect()
    });
    assert_eq!(order.len(), 3);
    cx.simulate_keystrokes("cmd-3");
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.focused_thread()),
        Some(order[2])
    );
    cx.simulate_keystrokes("cmd-1");
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.focused_thread()),
        Some(order[0]),
        "the waiting Thread does not jump the order"
    );
    cx.simulate_keystrokes("cmd-9");
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.focused_thread()),
        Some(order[0]),
        "a digit past the board does nothing"
    );
}

/// The one row grammar under a Project heading (N-2): the title then
/// the word on one line, and none of the marks the redesign dropped —
/// no subagent count, no provider logo, no Project filter, order door,
/// new-thread door or pencil above the tree (N-1).
#[gpui::test]
fn a_project_section_draws_one_row_grammar_and_no_filter_row(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("nav-thread-order", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let key = ferrite_core::activity::AgentKey::new(Provider::Claude, "nav", "child");
    let mut info = ferrite_core::activity::AgentInfo::new(key);
    info.parent = Some(ferrite_core::activity::Subject::Main);
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(
            ferrite_core::activity::ActivityEvent::Discovered(info),
        ))
        .unwrap();
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    assert_eq!(
        view.read_with(cx, |view, _| view
            .facts
            .get(thread)
            .map_or(0, |facts| facts.subagents)),
        1,
        "the premise: the Thread runs a subagent"
    );

    let title_id: &'static str = format!("nav-title-{}", thread.get()).leak();
    assert!(cx.debug_bounds(title_id).is_some(), "the one row grammar");
    for gone in [
        format!("nav-subagents-{}", thread.get()),
        format!("nav-mark-{}", thread.get()),
        format!("nav-group-membership-{}", thread.get()),
        "nav-filter".to_string(),
        "thread-list-order".to_string(),
        "add-thread".to_string(),
        "project-edit".to_string(),
        "nav-project-add-0".to_string(),
    ] {
        assert!(
            cx.debug_bounds(gone.clone().leak()).is_none(),
            "{gone} is gone from the nav"
        );
    }
    let heading = cx
        .debug_bounds("nav-project-section-0")
        .expect("the Project heading");
    let row = cx
        .debug_bounds(format!("nav-thread-{}", thread.get()).leak())
        .unwrap();
    assert!(heading.bottom() <= row.origin.y, "the row hangs under it");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.prefs.settings.thread_list_order,
            ThreadListOrder::Created,
            "creation order by default"
        );
        assert_eq!(view.nav_state().sections.len(), 1);
    });
}

/// A member row opens its whole Group on that Thread, its parked
/// members revived.
#[gpui::test]
fn a_member_row_opens_the_whole_group(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("member-row-group-open", 2);
    let threads = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    core.park(threads[0]).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    let target: &'static str = format!("nav-thread-{}", threads[1].get()).leak();
    let row = cx
        .debug_bounds(target)
        .expect("the member hangs under its Group");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    cx.run_until_parked();

    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.roster().view(), View::Group(group));
        assert_eq!(
            view.visible_indices().len(),
            2,
            "every Group Pane is visible"
        );
        assert_eq!(view.panes.len(), 2, "every Group Pane has been restored");
        assert_eq!(view.focused_thread(), Some(threads[1]));
    });
}

/// A new Thread in a named Project (the palette's command; the heading
/// has no `+` now): one draft, bound to that Project whatever the nav
/// filter or the focused Pane happen to say.
#[gpui::test]
fn a_new_thread_in_a_project_opens_a_draft_there(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("project-heading-plus", 1);
    let elsewhere = core
        .register_project(&std::env::temp_dir().canonicalize().unwrap())
        .unwrap();
    let heading = core.project_id(core.threads()[0]).expect("a bound Thread");
    assert_ne!(heading, elsewhere);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| {
        // The fallback names the *other* Project, so a draft that
        // ignored the request would be caught.
        view.launch_project = Some(elsewhere);
        view.open_draft_in_project(heading, cx);
    });
    tick(cx);

    view.read_with(cx, |view, _| {
        let draft = view
            .panes
            .iter()
            .find_map(|pane| pane.draft())
            .expect("the command opened a draft");
        assert_eq!(draft.binding.project(), heading);
        assert_eq!(
            view.launch_project,
            Some(heading),
            "the next draft starts where this one was asked for"
        );
    });
}

/// N-2: one section per Project — its loose rows first, then its
/// Groups — every Thread drawn exactly once, and nothing in creation
/// order moves because a Group was made.
#[gpui::test]
fn a_project_lists_its_loose_rows_then_its_groups(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("nav-interleave", 4);
    let mut ids = core.threads();
    ids.sort();
    let group = core
        .apply_group(GroupChange::Create {
            first: ids[0],
            second: ids[1],
        })
        .unwrap()
        .group
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.read_with(cx, |view, _| {
        let state = view.nav_state();
        assert_eq!(state.sections.len(), 1, "one Project");
        let section = &state.sections[0];
        assert_eq!(section.solos.len(), 2);
        assert_eq!(section.groups.len(), 1);
        assert_eq!(state.groups[section.groups[0]].id, group);
        let rows: Vec<ThreadId> = state.ordered_rows().iter().map(|row| row.thread).collect();
        assert_eq!(
            rows,
            vec![ids[2], ids[3], ids[0], ids[1]],
            "the loose rows, then the Group's members, each in creation order"
        );
    });
}

#[gpui::test]
fn dragging_filtered_members_uses_the_full_group_order(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("filtered-group-order", 2);
    let repo = repo_in(&scratch("filtered-order-other"));
    let project = core.register_project(&repo).unwrap();
    let third = core
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: repo.clone(),
            },
        )
        .unwrap();
    let fourth = core
        .open(Provider::Codex, WorkspaceChoice::Main { checkout: repo })
        .unwrap();
    let ids = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: ids[0],
            second: ids[1],
        })
        .unwrap()
        .group
        .unwrap();
    for thread in [third, fourth] {
        core.apply_group(GroupChange::Join {
            thread,
            group,
            index: None,
        })
        .unwrap();
    }
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    view.update(cx, |view, cx| {
        view.nav_filter = Some(project);
        view.enter_group(group, cx);
    });
    tick(cx);
    drag_nav(cx, "nav-thread-4", "nav-thread-3");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.groups().get(group).unwrap().members,
            [ids[0], ids[1], fourth, third]
        );
    });
}

/// The ongoing tier holds still. Two Threads streaming at once keep
/// trading the "most recent" crown as their deltas land; before this,
/// each update re-raced them and the rows swapped under the operator's
/// cursor. Now every ongoing row shares one rank, so the stable sort
/// leaves them exactly where they were.
#[gpui::test]
fn ongoing_threads_hold_their_place_while_they_stream(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("nav-ongoing-stable", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    // Both Threads start a turn, oldest first, so they are both Working.
    for index in [0, 1] {
        fake.streams.borrow()[index]
            .send(SessionEvent::TextDelta {
                text: "thinking".into(),
            })
            .unwrap();
    }
    tick(cx);

    let before = view.read_with(cx, |view, _| {
        let state = view.nav_state();
        assert!(
            state
                .solos
                .iter()
                .all(|row| row.status == nav::RowStatus::Working),
            "both Threads are mid-turn"
        );
        state
            .ordered_solos()
            .iter()
            .map(|row| row.thread)
            .collect::<Vec<_>>()
    });
    assert_eq!(before.len(), 2);

    // The one at the bottom speaks again — repeatedly. Its recency now
    // beats the other's, and the old sort would have lifted it.
    for _ in 0..3 {
        fake.streams.borrow()[1]
            .send(SessionEvent::TextDelta {
                text: "more".into(),
            })
            .unwrap();
        tick(cx);
    }

    view.read_with(cx, |view, _| {
        let after: Vec<ThreadId> = view
            .nav_state()
            .ordered_solos()
            .iter()
            .map(|row| row.thread)
            .collect();
        assert_eq!(
            after, before,
            "an ongoing Thread's own updates must not move it"
        );
    });
}

/// #21 AC1, amended: the tree lists every **open** Thread — in creation
/// order (N-3). A parked solo is not a tree row any more: it waits in
/// the Parked section at the foot of the column, folded shut.
#[gpui::test]
fn the_nav_lists_open_threads_in_the_tree_and_parked_ones_below(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-order", 3);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-w", CloseThread, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let (grid_order, parked_thread) = view.read_with(cx, |view, _| {
        (
            view.panes
                .iter()
                .filter_map(|pane| pane.thread())
                .collect::<Vec<_>>(),
            view.panes[1].thread().unwrap(),
        )
    });

    view.update(cx, |view, _| view.focus_pane(1));
    cx.simulate_keystrokes("cmd-w");

    view.read_with(cx, |view, _| {
        let state = view.nav_state();
        assert!(state.groups.is_empty(), "no Group claims these Threads");
        let ordered = state.ordered_solos();
        let rows: Vec<ThreadId> = ordered.iter().map(|row| row.thread).collect();
        let mut expected: Vec<ThreadId> = grid_order
            .iter()
            .copied()
            .filter(|thread| *thread != parked_thread)
            .collect();
        // The nav's default order: creation order, over the Threads
        // that are open.
        expected.sort();
        assert_eq!(rows, expected, "creation order");
        assert!(
            !rows.contains(&parked_thread),
            "a parked Thread is not a tree row"
        );
        assert!(
            state
                .ordered_rows()
                .iter()
                .all(|row| row.thread != parked_thread),
            "nor is it in the sequence the rail folds to"
        );
        // An untitled Thread with no prompt is `New thread`, never its
        // number (rule 2.11), in the row and the head alike.
        assert_eq!(
            ordered[0].name.as_ref(),
            crate::facts::NEW_THREAD,
            "rows say what the Pane head says"
        );
        assert_eq!(view.facts.name(expected[0]), ordered[0].name);
        assert!(
            ordered
                .iter()
                .all(|row| matches!(row.tail, nav::NavTail::Age(_))),
            "every quiet row's tail is how long since it was used"
        );
        let parked: Vec<ThreadId> = state.parked.iter().map(|row| row.thread).collect();
        assert_eq!(
            parked,
            vec![parked_thread],
            "the parked Thread waits in the Parked section"
        );
        assert_eq!(
            view.nav_view,
            nav::NavView::Tree,
            "the tree until the operator asks for Parked"
        );
        assert_eq!(state.parked[0].status, nav::RowStatus::Parked);
        assert!(matches!(state.parked[0].tail, nav::NavTail::Age(_)));
    });
}

/// A parked Group member keeps its seat in its Group, and opening the
/// Group revives it there. The Parked section counts it with the rest
/// (the prototype's `parked 3`), but the menu that clears the section
/// deletes only the solos — it can never dissolve a Group behind the
/// operator's back.
#[gpui::test]
fn a_parked_group_member_stays_under_its_group(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("parked-member", 3);
    let threads = core.threads();
    core.apply_group(GroupChange::Create {
        first: threads[0],
        second: threads[1],
    })
    .unwrap();
    core.park(threads[0]).unwrap();
    core.park(threads[2]).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.read_with(cx, |view, _| {
        let state = view.nav_state();
        assert_eq!(state.groups.len(), 1);
        let members: Vec<ThreadId> = state.groups[0]
            .members
            .iter()
            .map(|row| row.thread)
            .collect();
        assert_eq!(
            members,
            vec![threads[0], threads[1]],
            "the parked member keeps its seat in the Group"
        );
        assert_eq!(state.groups[0].members[0].status, nav::RowStatus::Parked);
        let parked: Vec<ThreadId> = state.parked.iter().map(|row| row.thread).collect();
        assert_eq!(
            parked,
            vec![threads[0], threads[2]],
            "the section counts every parked Thread, the member too"
        );
        assert_eq!(
            view.deletable_parked(),
            vec![threads[2]],
            "but the menu deletes only the parked solo"
        );
    });
}

/// The Parked view (§5.1.2): its row's press shows the parked Threads
/// in the tree's place and a second press the tree again, and a right
/// press opens its menu. Its delete arms on the first press and, on the
/// second, erases every Thread the view lists — and only those: the
/// open Threads stay, and with nothing left the count goes.
#[gpui::test]
fn the_parked_view_shows_its_rows_and_its_menu_deletes_them(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("parked-fold", 4);
    let threads = core.threads();
    core.park(threads[0]).unwrap();
    core.park(threads[1]).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    let first_parked: &'static str = format!("nav-thread-{}", threads[0].get()).leak();
    let live: &'static str = format!("nav-thread-{}", threads[2].get()).leak();
    let door = cx
        .debug_bounds("nav-view-parked")
        .expect("the Parked view's row");
    assert!(cx.debug_bounds("nav-view-parked-count").is_some());
    assert!(
        cx.debug_bounds(first_parked).is_none(),
        "the tree draws no parked row"
    );
    assert!(
        cx.debug_bounds(live).is_some(),
        "an open Thread is a tree row"
    );

    cx.simulate_click(door.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.nav_view, nav::NavView::Parked, "the press shows it")
    });
    assert!(
        cx.debug_bounds(first_parked).is_some(),
        "the parked rows are drawn"
    );
    assert!(cx.debug_bounds(live).is_none(), "in the tree's place");
    cx.simulate_click(door.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.nav_view,
            nav::NavView::Tree,
            "a second press: the tree"
        )
    });

    cx.simulate_mouse_down(
        door.center(),
        gpui::MouseButton::Right,
        gpui::Modifiers::none(),
    );
    cx.run_until_parked();
    let delete = view.read_with(cx, |view, _| {
        let menu = view
            .context_menu
            .as_ref()
            .expect("a right press opens the view's menu");
        assert_eq!(menu.target, MenuTarget::Parked);
        let labels: Vec<&str> = menu
            .rows
            .iter()
            .flatten()
            .map(|(item, _)| item.label.as_ref())
            .collect();
        assert_eq!(labels, vec!["Show parked threads", "Delete parked threads"]);
        menu.rows
            .iter()
            .position(|row| matches!(row, Some((_, MenuVerb::DeleteAllParked))))
            .unwrap()
    });
    view.update_in(cx, |view, window, cx| {
        view.press_menu_row(delete, window, cx);
        let menu = view.context_menu.as_ref().expect("armed, still up");
        assert_eq!(menu.armed, Some(delete), "the first press only arms");
        assert_eq!(view.facts.parked().len(), 2, "and deletes nothing");
        view.press_menu_row(delete, window, cx);
        assert!(
            view.context_menu.is_none(),
            "the second press runs and closes"
        );
        assert!(
            view.facts.parked().is_empty(),
            "every parked Thread is gone from the nav"
        );
        assert!(
            view.cockpit.parked().unwrap().is_empty(),
            "and from the store"
        );
        assert_eq!(
            view.cockpit.threads().len(),
            2,
            "the open Threads are untouched"
        );
        assert!(view.group_error.is_none(), "nothing refused");
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("nav-view-parked-count").is_none(),
        "nothing parked, no count"
    );
}

/// The single-Thread "confirm before deleting" setting is about one
/// Thread. The bulk delete always asks: with the setting off, one
/// press still only arms.
#[gpui::test]
fn deleting_every_parked_thread_always_asks_first(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("parked-confirm", 2);
    let threads = core.threads();
    core.park(threads[0]).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.update_in(cx, |view, window, cx| {
        view.prefs.settings.confirm_delete = false;
        view.open_context_menu(MenuTarget::Parked, gpui::point(px(100.), px(600.)), cx);
        let delete = view
            .context_menu
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .position(|row| matches!(row, Some((_, MenuVerb::DeleteAllParked))))
            .unwrap();
        view.press_menu_row(delete, window, cx);
        let menu = view
            .context_menu
            .as_ref()
            .expect("still up: one press only arms, whatever the setting says");
        assert_eq!(menu.armed, Some(delete));
        assert_eq!(
            view.facts.parked(),
            &[threads[0]],
            "nothing was deleted yet"
        );
        view.press_menu_row(delete, window, cx);
        assert!(view.context_menu.is_none());
        assert!(view.facts.parked().is_empty());
    });
}

/// The Parked view scrolls in the tree's place: forty parked Threads,
/// the last below the column's foot until the list is scrolled.
#[gpui::test]
fn a_long_parked_list_scrolls_in_the_trees_place(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("parked-cap", 41);
    let threads = core.threads();
    for thread in &threads[1..] {
        core.park(*thread).unwrap();
    }
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(600.)));
    view.update(cx, |view, cx| {
        view.show_nav_view(nav::NavView::Parked, None, cx);
    });
    tick(cx);

    let list = cx.debug_bounds("nav-view-parked-list").expect("shown");
    let last: &'static str = format!("nav-thread-{}", threads[40].get()).leak();
    let before = cx.debug_bounds(last).expect("drawn, clipped");
    assert!(
        before.origin.y > px(600.),
        "the last parked row starts below the window: {before:?}"
    );
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: list.origin + gpui::point(px(40.), px(40.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-40_000.))),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::default(),
    });
    cx.run_until_parked();
    let after = cx.debug_bounds(last).unwrap();
    assert!(
        after.center().y < px(600.),
        "scrolling the list exposes the last parked row: {after:?}"
    );
}

/// #21 AC2: clicking a running nav row lands the operator on that Pane —
/// through `focus_pane`, so a fullscreened cockpit re-aims to the
/// clicked Thread instead of going stale on the one they left.
#[gpui::test]
fn clicking_a_running_nav_row_focuses_its_pane_and_reaims_fullscreen(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-click", 2);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-shift-enter", ToggleFullscreen, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.read_with(cx, |view, _| assert_eq!(view.focused(), 0));

    // Each Pane's row, where the nav drew it.
    let row_at = |view: &Entity<CockpitView>, pane: usize, cx: &mut gpui::VisualTestContext| {
        let thread = view.read_with(cx, |view, _| view.panes[pane].thread().unwrap());
        cx.debug_bounds(format!("nav-thread-{}", thread.get()).leak())
            .expect("every open Thread has a row")
            .center()
    };
    let (second, first) = (row_at(&view, 1, cx), row_at(&view, 0, cx));
    cx.simulate_click(second, gpui::Modifiers::none());
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused(), 1, "the click moved focus to the row's Pane");
    });

    cx.simulate_keystrokes("cmd-shift-enter");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.roster().fullscreen(),
            Some(PaneIdentity::Thread(view.panes[1].thread().unwrap()))
        );
    });
    // And back to the other Pane's row.
    cx.simulate_click(first, gpui::Modifiers::none());
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused(), 0, "the nav still answers while fullscreen");
        assert_eq!(
            view.cockpit.roster().fullscreen(),
            Some(PaneIdentity::Thread(view.panes[0].thread().unwrap())),
            "and the fullscreen re-aims with focus — the one door"
        );
    });
}

/// #21 AC2: clicking a parked nav row revives that Thread — a Pane,
/// focus, and the park order forgetting it so cmd-o cannot revive it a
/// second time. The row lives in the Parked view, so that is shown
/// first.
#[gpui::test]
fn clicking_a_parked_nav_row_revives_that_thread(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-revive", 2);
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-w", CloseThread, None),
            KeyBinding::new("cmd-o", ReopenThread, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let parked = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    cx.simulate_keystrokes("cmd-w");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 1);
        assert_eq!(view.facts.parked().len(), 1, "the parked Thread got a row");
    });

    let door = cx
        .debug_bounds("nav-view-parked")
        .expect("the Parked view's row");
    cx.simulate_click(door.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let row_id: &'static str = format!("nav-thread-{}", parked.get()).leak();
    let row = cx
        .debug_bounds(row_id)
        .expect("shown, the parked Thread's row is drawn");
    cx.simulate_click(row.center(), gpui::Modifiers::none());

    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 2, "the revived Thread got a Pane");
        assert_eq!(
            view.panes[1].thread().unwrap(),
            parked,
            "and it is the same Thread"
        );
        assert_eq!(view.focused(), 1, "focus followed the revival");
        assert!(view.facts.parked().is_empty(), "its nav row moved up");
    });

    // cmd-o must not bring back a Thread the nav already revived.
    cx.simulate_keystrokes("cmd-o");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 2, "nothing was left parked to reopen");
    });
}

/// A pending Decision changes a row's mark (SPEC §6), never the rows
/// themselves: the same Threads under the same Projects, in the column
/// and in the collapsed rail alike.
#[gpui::test]
fn a_pending_decision_moves_no_row_in_the_nav(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("nav-amber", 2);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-b", ToggleNav, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let before = view.read_with(cx, |view, _| describe_nav(&view.nav_state()));

    fake.streams.borrow()[1].send(decision("perm_02")).unwrap();
    tick(cx);

    view.read_with(cx, |view, _| {
        assert_eq!(
            describe_nav(&view.nav_state()),
            before,
            "a pending Decision moves no row in navigation"
        );
    });

    // And the rail says no more than the column did.
    cx.simulate_keystrokes("cmd-b");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let state = view.nav_state();
        assert!(view.nav_railed());
        assert_eq!(describe_nav(&state), before);
    });
}

/// Every nav row's name and Project, as one comparable list.
fn describe_nav(state: &nav::NavState) -> Vec<String> {
    state
        .ordered_rows()
        .into_iter()
        .map(|row| format!("{}|{:?}", row.name, row.project))
        .collect()
}

/// #21: the nav's width is part of the zoom input — cmd-b folding it
/// away hands width back, so a Pane that could not hold a transcript
/// beside the full nav can without it. cmd-b again takes the width back.
#[gpui::test]
fn cmd_b_collapses_the_nav_and_the_cells_grow_a_level(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-toggle", 1);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-b", ToggleNav, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    hold_nav_open(&view, cx);
    // Sized so the Transcript threshold sits between the two nav
    // widths: a wall tile beside the full column, Transcript without it.
    cx.simulate_resize(gpui::size(px(550.), px(700.)));
    tick(cx);
    let expanded = cx.update(|window, cx| view.read(cx).level_now(window));
    assert_eq!(
        expanded,
        Level::Wall,
        "the premise: the full nav costs this cell its transcript"
    );

    cx.simulate_keystrokes("cmd-b");
    let collapsed = cx.update(|window, cx| view.read(cx).level_now(window));
    assert_eq!(collapsed, Level::Transcript, "folding hands width back");
    tick(cx);

    cx.simulate_keystrokes("cmd-b");
    let reopened = cx.update(|window, cx| view.read(cx).level_now(window));
    assert_eq!(reopened, Level::Wall, "cmd-b toggles back");
}

/// The Threads in creation order.
fn in_order(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<ThreadId> {
    view.read_with(cx, |view, _| {
        let mut threads = view.cockpit.threads();
        threads.sort();
        threads
    })
}

/// Does the focused Pane's Composer hold the keyboard?
fn composer_has_keys(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> bool {
    view.update_in(cx, |view, window, cx| {
        let pane = &view.panes[view.focused()];
        pane.composer.read(cx).focus_handle(cx).is_focused(window)
    })
}

/// A press on a tree row opens its Thread and leaves the keyboard with its
/// Composer: the tree takes the keys only from ⌘⇧E, so typing after
/// picking a Thread lands in it.
#[gpui::test]
fn a_press_on_a_row_leaves_the_keys_with_the_composer(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (core, _fake) = cockpit("nav-press-keys", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    tick(cx);
    let threads = in_order(&view, cx);
    let other = threads
        .into_iter()
        .find(|thread| view.read_with(cx, |view, _| view.focused_thread()) != Some(*thread))
        .expect("a Thread not shown");
    let row = cx
        .debug_bounds(format!("nav-thread-{}", other.get()).leak())
        .expect("its tree row");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.focused_thread()),
        Some(other)
    );
    assert!(
        !view.update_in(cx, |view, window, _| view.nav_view_focus.is_focused(window)),
        "the tree did not take the keys"
    );
    assert!(composer_has_keys(&view, cx), "the Composer types");
    cx.simulate_input("hi");
    tick(cx);
    assert!(view.read_with(cx, |view, cx| {
        view.panes[view.focused()]
            .composer
            .read(cx)
            .text()
            .ends_with("hi")
    }));
}

/// ⌘⇧E hands the tree the keys and puts its cursor on a row; however the
/// keys leave, the cursor and its hover face go with them.
#[gpui::test]
fn the_cursor_clears_when_the_tree_lets_go(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (core, _fake) = cockpit("nav-cursor-clears", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    tick(cx);
    cx.simulate_keystrokes("cmd-shift-e down");
    tick(cx);
    let key = view
        .read_with(cx, |view, _| view.nav_cursor.clone())
        .expect("a cursor")
        .key;
    // The keys go to the board without Esc.
    view.update_in(cx, |view, window, cx| {
        let pane = &view.panes[view.focused()];
        window.focus(&pane.composer.read(cx).focus_handle(cx), cx);
    });
    tick(cx);
    leap(cx, Duration::from_millis(500));
    assert!(view.read_with(cx, |view, _| view.nav_cursor.is_none()));
    assert_eq!(
        cx.update(|_, _| ferrite_ui::motion::hover_t(&key)),
        0.0,
        "its row no longer wears the hover face"
    );
}

/// ← on a loose Thread moves the cursor to its Project's heading and folds
/// the Project, as an outline's ← goes to the parent.
#[gpui::test]
fn left_on_a_loose_row_folds_its_project(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (core, _fake) = cockpit("nav-left-folds", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    tick(cx);
    // ⌘⇧E starts on the row the board shows: a loose, idle Thread.
    cx.simulate_keystrokes("cmd-shift-e");
    tick(cx);
    let start = view.read_with(cx, |view, _| view.nav_cursor.clone().map(|c| c.item));
    assert!(
        matches!(start, Some(nav::NavItem::Thread(_, None))),
        "{start:?}"
    );
    cx.simulate_keystrokes("left");
    tick(cx);
    let project = view
        .read_with(cx, |view, _| view.nav_state().sections[0].project)
        .expect("the fixture's Project");
    assert_eq!(
        view.read_with(cx, |view, _| view.nav_cursor.clone().map(|c| c.item)),
        Some(nav::NavItem::Heading(NavFold::Project(project)))
    );
    assert!(view.read_with(cx, |view, _| view.nav_folded(NavFold::Project(project))));
}

/// The tree fades at whichever edge has rows past it, never slicing a row
/// against the head or the foot: the foot while rows remain below, the top
/// once it has scrolled, and neither edge with nothing past it.
#[gpui::test]
fn the_tree_fades_at_the_edges_rows_pass(cx: &mut TestAppContext) {
    let (core, _) = cockpit("nav-edge-fades", 24);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    // Every row drawn: past `Show more`.
    view.update(cx, |view, _| {
        let project = view.nav_state().sections[0].project;
        view.nav_more.insert(project);
    });
    cx.simulate_resize(gpui::size(px(1000.), px(320.)));
    cx.run_until_parked();
    let shows = |cx: &mut gpui::VisualTestContext, id: &'static str| cx.debug_bounds(id).is_some();
    assert!(shows(cx, "nav-fade-foot"), "rows below: the foot fades");
    assert!(!shows(cx, "nav-fade-top"), "at the top: nothing above");
    let wheel = |cx: &mut gpui::VisualTestContext, y: f32| {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(px(100.), px(200.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(y))),
            ..Default::default()
        });
        cx.run_until_parked();
    };
    wheel(cx, -40.);
    assert!(shows(cx, "nav-fade-top"), "scrolled: the top fades");
    assert!(shows(cx, "nav-fade-foot"));
    wheel(cx, -40_000.);
    assert!(shows(cx, "nav-fade-top"));
    assert!(!shows(cx, "nav-fade-foot"), "at the end: nothing below");
}

/// Claude's `Pinned`: the row menu's `Pin` lists a loose Thread under the
/// `Pinned` heading, above the Projects, and nowhere else; the pin is
/// saved with the settings; `Unpin` puts the row back in its Project. The
/// heading folds its rows away.
#[gpui::test]
fn a_pinned_thread_heads_the_tree_until_unpinned(cx: &mut TestAppContext) {
    let (core, _) = cockpit("nav-pinned", 3);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let menu = |view: &CockpitView| -> Vec<String> {
        view.context_rows(MenuTarget::Thread(threads[2]))
            .into_iter()
            .flatten()
            .map(|(item, _)| item.label.to_string())
            .collect()
    };
    assert_eq!(view.read_with(cx, |view, _| menu(view))[0], "Pin");
    assert!(
        cx.debug_bounds("nav-pinned-heading").is_none(),
        "nothing pinned"
    );

    view.update_in(cx, |view, window, cx| {
        view.run_menu_verb(MenuTarget::Thread(threads[2]), MenuVerb::Pin, window, cx)
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.prefs.settings.pinned_threads, vec![threads[2]]);
        let state = view.nav_state();
        assert_eq!(
            state
                .pinned
                .iter()
                .map(|row| row.thread)
                .collect::<Vec<_>>(),
            vec![threads[2]]
        );
        assert!(
            state.solos.iter().all(|row| row.thread != threads[2]),
            "listed once"
        );
        assert_eq!(menu(view)[0], "Unpin");
    });
    let heading = cx.debug_bounds("nav-pinned-heading").expect("Pinned");
    let row = cx
        .debug_bounds(format!("nav-thread-{}", threads[2].get()).leak())
        .expect("its row");
    let projects = cx.debug_bounds("nav-projects-heading").expect("Projects");
    assert!(heading.bottom() <= row.top() && row.bottom() <= projects.top());

    cx.simulate_click(heading.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx
        .debug_bounds(format!("nav-thread-{}", threads[2].get()).leak())
        .is_none());

    view.update_in(cx, |view, window, cx| {
        view.run_menu_verb(MenuTarget::Thread(threads[2]), MenuVerb::Unpin, window, cx)
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.prefs.settings.pinned_threads.is_empty());
        assert!(view.nav_state().pinned.is_empty());
    });
    assert!(cx.debug_bounds("nav-pinned-heading").is_none());
}

/// Codex's `Show more`: past five loose rows a Project draws the first
/// five (and the selected row) over `Show more`, which draws them all and
/// becomes `Show less`.
#[gpui::test]
fn a_long_project_shows_five_rows_then_show_more(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("nav-show-more", 7);
    let threads = core.threads();
    core.focus_thread(threads[0]);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    tick(cx);
    let drawn = |cx: &mut gpui::VisualTestContext| -> usize {
        threads
            .iter()
            .filter(|thread| {
                cx.debug_bounds(format!("nav-thread-{}", thread.get()).leak())
                    .is_some()
            })
            .count()
    };
    assert_eq!(drawn(cx), nav::SHOWN_ROWS);
    let more = cx.debug_bounds("nav-show-more-0").expect("Show more");
    cx.simulate_click(more.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(drawn(cx), threads.len());
    assert_eq!(
        view.read_with(cx, |view, _| view.nav_state().sections[0].more),
        Some(nav::ShowMore::Less)
    );
    let less = cx.debug_bounds("nav-show-more-0").expect("Show less");
    cx.simulate_click(less.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(drawn(cx), nav::SHOWN_ROWS);
}

/// A Project's fold is saved with the settings, and its heading's caret
/// says which way it is.
#[gpui::test]
fn a_project_fold_is_saved(cx: &mut TestAppContext) {
    let (core, _) = cockpit("nav-fold-saved", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    tick(cx);
    assert!(cx.debug_bounds("nav-project-section-0-chevron").is_some());
    let project = view
        .read_with(cx, |view, _| view.nav_state().sections[0].project)
        .expect("the fixture's Project");
    let heading = cx.debug_bounds("nav-project-section-0").unwrap();
    cx.simulate_click(heading.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.prefs.settings.folded_projects, vec![project]);
        assert!(view.nav_state().sections[0].folded);
    });
}

/// Codex's header: search beside the bell, opening the palette; the
/// views no longer list `Search` or `Inbox`.
#[gpui::test]
fn search_sits_in_the_nav_head(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (core, _) = cockpit("nav-search-head", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let search = cx.debug_bounds("nav-search").expect("the search button");
    let bell = cx.debug_bounds("nav-activity").expect("the bell");
    assert!(search.bottom() <= px(ferrite_ui::theme::WIN_CHROME_H));
    assert!(bell.right() <= search.left(), "after the bell");
    assert!(cx.debug_bounds("nav-view-search").is_none());
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.floats.palette.is_some()));
}
