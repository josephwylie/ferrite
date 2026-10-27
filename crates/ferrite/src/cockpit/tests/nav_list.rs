//! The nav: thread rows, the attention list and the parked list.
#[allow(unused_imports)]
use super::*;

/// A Thread row is two lines (§5.1.5): the title at 35 on line 1 (one
/// tree level further in, under its Project), line 2 under it on the
/// same edge, 44 high together, and no provider logo.
#[gpui::test]
fn a_thread_row_is_two_lines_on_one_edge(cx: &mut TestAppContext) {
    let (core, _) = cockpit("nav-age-align", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    cx.run_until_parked();
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

    let id = |name: &str| -> &'static str { format!("{name}-{}", thread.get()).leak() };
    let row = cx.debug_bounds(id("nav-thread")).expect("the row");
    let title = cx.debug_bounds(id("nav-title")).expect("line 1");
    let two = cx.debug_bounds(id("nav-line2")).expect("line 2");
    assert_eq!(row.size.height, px(crate::theme::NAV_ROW2_H));
    assert!(
        (title.left()
            - (row.left() + px(crate::theme::NAV_TITLE_X + crate::theme::NAV_TREE_INDENT)))
        .abs()
            < px(0.5),
        "the title at 35, a level in: {title:?} in {row:?}"
    );
    assert!((two.left() - title.left()).abs() < px(0.5), "one edge");
    assert!(two.top() >= title.bottom() - px(0.5), "line 2 under line 1");
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
            nav::RowStatus::roll_up(&block.members),
            Some(nav::RowStatus::NeedsYou),
            "the Group rolls up"
        );
        for at in [0, 2] {
            let row = view.thread_row(threads[at]);
            assert_eq!(row.status, nav::RowStatus::NeedsYou);
            assert_eq!(
                row.tail,
                nav::NavTail::Waiting(crate::theme::words::APPROVAL),
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

/// §5.1.2: the Needs-you view stands in the tree's place —
/// the blocked Threads oldest-first as their two-line rows, then
/// `Recent` — counted by the view row's pill. Esc and a click on the
/// active view return to the tree, and ⌘D still walks the queue.
#[gpui::test]
fn the_needs_you_view_lists_the_queue_in_the_trees_place(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("needs-you-view", 3);
    let threads = core.threads();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    assert!(
        cx.debug_bounds("nav-needs-you-pill").is_none(),
        "nothing waits: no pill"
    );
    fake.streams.borrow()[2].send(decision("perm_3")).unwrap();
    tick(cx);
    fake.streams.borrow()[0].send(decision("perm_1")).unwrap();
    tick(cx);
    assert!(cx.debug_bounds("nav-needs-you-pill").is_some(), "2 wait");
    assert!(cx.debug_bounds("nav-view-needs-you-list").is_none());

    let door = cx.debug_bounds("nav-view-needs-you").expect("the view row");
    cx.simulate_click(door.center(), gpui::Modifiers::none());
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
    let oldest = cx.debug_bounds(row(threads[2])).expect("a two-line row");
    assert!(first.contains(&oldest.center()), "oldest first");
    assert_eq!(oldest.size.height, px(crate::theme::NAV_ROW2_H));
    // The queue stays flat: no Project heading, no guide, every row at
    // the root, so its order is the queue's.
    assert!(cx.debug_bounds("nav-view-project-0").is_none());
    assert!(cx
        .debug_bounds(format!("nav-guide-{}-0", threads[2].get()).leak())
        .is_none());
    let title = cx
        .debug_bounds(format!("nav-title-{}", threads[2].get()).leak())
        .expect("its title");
    assert!(
        (title.left() - oldest.left() - px(crate::theme::NAV_TITLE_X)).abs() < px(0.5),
        "at the root"
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
    cx.simulate_click(door.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.needs_you_shown()));
    cx.simulate_click(door.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        !view.read_with(cx, |view, _| view.needs_you_shown()),
        "a click on the active view returns to the tree"
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

/// The Attention list (§5.1.2): always on, no setting. Nothing in
/// flight and it is gone. Then `Needs you` (oldest request first), `Done`
/// (newest finish first) and `Working` (creation order), in that order
/// over `Projects`, each head with its count, each row one line; and a
/// Thread's own row stays in the tree throughout.
#[gpui::test]
fn the_attention_list_lists_needs_you_then_done_then_working(cx: &mut TestAppContext) {
    use nav::ListSection::{Done, NeedsYou, Working};
    let (core, fake) = cockpit("attention-list", 6);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    assert_eq!(listed(&view, cx), vec![], "nothing in flight");
    assert!(cx.debug_bounds("nav-list").is_none(), "and no list at all");

    // threads[0] is focused; the others are out of sight.
    assert_eq!(
        view.read_with(cx, |view, _| view.cockpit.roster().focused_thread()),
        Some(threads[0])
    );
    let send = |index: usize, event: SessionEvent| {
        fake.streams.borrow()[index].send(event).unwrap();
    };
    let finish = || SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Completed,
        cost_usd: None,
    };
    let working = || SessionEvent::TextDelta {
        text: "thinking".into(),
    };
    // Two working, the later-created first to start.
    send(5, working());
    send(1, working());
    tick(cx);
    assert_eq!(
        listed(&view, cx),
        vec![(Working, vec![threads[1], threads[5]])]
    );
    // Two finishing, threads[2] first; two asking, threads[4] first.
    send(2, working());
    send(3, working());
    tick(cx);
    send(2, finish());
    tick(cx);
    send(3, finish());
    send(4, decision("perm-a"));
    tick(cx);
    send(1, decision("perm-b"));
    tick(cx);
    assert_eq!(
        listed(&view, cx),
        vec![
            (NeedsYou, vec![threads[4], threads[1]]),
            (Done, vec![threads[3], threads[2]]),
            (Working, vec![threads[5]]),
        ],
    );

    let at = |cx: &mut gpui::VisualTestContext, key: String| {
        cx.debug_bounds(key.clone().leak())
            .unwrap_or_else(|| panic!("{key} drawn"))
    };
    let heads: Vec<_> = ["nav-list-needs-you", "nav-list-done", "nav-list-working"]
        .into_iter()
        .map(|key| at(cx, key.into()))
        .collect();
    assert!(heads[0].bottom() <= heads[1].top() && heads[1].bottom() <= heads[2].top());
    let parked = at(cx, "nav-view-parked".into());
    let projects = at(cx, "nav-projects-heading".into());
    assert!(parked.bottom() <= heads[0].top(), "under the views");
    assert!(
        at(cx, "nav-list".into()).bottom() <= projects.top(),
        "over Projects"
    );
    let count = cx.debug_bounds("nav-list-done-count").expect("its count");
    assert!(count.right() > heads[1].center().x, "at the head's right");
    // One line each, its word at the right; its own row stays below.
    for thread in [threads[4], threads[3], threads[5]] {
        let row = at(cx, format!("nav-list-row-{}", thread.get()));
        assert_eq!(row.size.height, px(crate::theme::NAV_ROW_H), "one line");
        assert!(at(cx, format!("nav-thread-{}", thread.get())).top() >= projects.bottom());
    }
    let word = at(cx, format!("nav-list-word-{}", threads[4].get()));
    let row = at(cx, format!("nav-list-row-{}", threads[4].get()));
    assert!(word.right() <= row.right() && word.left() > row.center().x);
    assert_eq!(
        view.read_with(cx, |view, _| nav::line_word(&view.thread_row(threads[4])))
            .map(|(word, _)| word),
        Some("approval".into()),
        "a request says what it waits for"
    );
}

/// Each section folds to its head and count, the fold held as the
/// nav's others are; the other sections stay open.
#[gpui::test]
fn an_attention_list_section_folds_to_its_head(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("attention-list-fold", 3);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    for stream in [1, 2] {
        fake.streams.borrow()[stream]
            .send(SessionEvent::TextDelta {
                text: "thinking".into(),
            })
            .unwrap();
    }
    tick(cx);
    let row = |cx: &mut gpui::VisualTestContext, thread: ThreadId| {
        cx.debug_bounds(format!("nav-list-row-{}", thread.get()).leak())
    };
    assert!(row(cx, threads[1]).is_some());
    let head = cx.debug_bounds("nav-list-working").expect("its head");
    cx.simulate_click(head.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(view.read_with(cx, |view, _| view
        .nav_folds
        .contains(&NavFold::List(nav::ListSection::Working))));
    assert!(row(cx, threads[1]).is_none() && row(cx, threads[2]).is_none());
    assert!(
        cx.debug_bounds("nav-list-working-count").is_some(),
        "the count stays"
    );
    // A request arriving opens a section of its own, unfolded.
    fake.streams.borrow()[1]
        .send(decision("perm-fold"))
        .unwrap();
    tick(cx);
    assert!(row(cx, threads[1]).is_some(), "under Needs you");
    assert!(row(cx, threads[2]).is_none(), "Working still folded");

    let head = cx.debug_bounds("nav-list-working").expect("its head");
    cx.simulate_click(head.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(row(cx, threads[2]).is_some(), "open again");
}

/// Done's exit rule (CONTEXT.md "Attention list"): opening a done
/// Thread from the list reads its Notice, yet its row stays under
/// `Done` while it is the focused Thread, and leaves once focus moves
/// to another — by a press on a tree row or by the keyboard (⌘]). One
/// that finishes while focused is held under Done the same way.
#[gpui::test]
fn a_done_thread_stays_in_done_until_focus_moves_on(cx: &mut TestAppContext) {
    use nav::ListSection::{Done, Working};
    let (mut core, fake) = cockpit("attention-list-done", 4);
    let threads = core.threads();
    // threads[2] shares a Group with threads[0], so ⌘] can walk from it.
    core.apply_group(GroupChange::Create {
        first: threads[0],
        second: threads[2],
    })
    .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    for stream in [1, 2] {
        fake.streams.borrow()[stream]
            .send(SessionEvent::TextDelta {
                text: "thinking".into(),
            })
            .unwrap();
    }
    tick(cx);
    for stream in [1, 2] {
        fake.streams.borrow()[stream]
            .send(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
        tick(cx);
    }
    assert_eq!(
        listed(&view, cx),
        vec![(Done, vec![threads[2], threads[1]])]
    );

    let row = cx
        .debug_bounds(format!("nav-list-row-{}", threads[1].get()).leak())
        .expect("its list row");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    tick(cx);
    let status = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext, at: usize| {
        view.read_with(cx, |view, _| view.thread_row(threads[at]).status)
    };
    assert_eq!(
        view.read_with(cx, |view, _| view.cockpit.roster().focused_thread()),
        Some(threads[1]),
        "the press lands on it"
    );
    assert_ne!(status(&view, cx, 1), nav::RowStatus::Done, "seen: read");
    assert_eq!(
        listed(&view, cx),
        vec![(Done, vec![threads[2], threads[1]])],
        "and still in Done, where the operator clicked it"
    );
    let selected = cx
        .debug_bounds(format!("nav-list-row-{}", threads[1].get()).leak())
        .expect("its list row, drawn");
    assert!(
        selected.contains(&row.center()),
        "not moved from under the pointer"
    );

    // A press on a tree row moves focus on: it leaves.
    let tree_row = cx
        .debug_bounds(format!("nav-thread-{}", threads[3].get()).leak())
        .expect("a tree row");
    cx.simulate_click(tree_row.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        listed(&view, cx),
        vec![(Done, vec![threads[2]])],
        "it leaves"
    );

    // Focused from the tree, the other one is held too, and the
    // keyboard's ⌘] moving on to its Group's other member lets it go.
    // (Entering the Group puts both members on screen: both are seen.)
    let tree_row = cx
        .debug_bounds(format!("nav-thread-{}", threads[2].get()).leak())
        .expect("a tree row");
    cx.simulate_click(tree_row.center(), gpui::Modifiers::none());
    tick(cx);
    assert_ne!(status(&view, cx, 2), nav::RowStatus::Done);
    assert_eq!(listed(&view, cx), vec![(Done, vec![threads[2]])], "held");
    view.update_in(cx, |view, window, cx| view.next_pane(&NextPane, window, cx));
    tick(cx);
    assert_ne!(
        view.read_with(cx, |view, _| view.cockpit.roster().focused_thread()),
        Some(threads[2])
    );
    assert_eq!(
        listed(&view, cx),
        vec![],
        "focus moved on: gone, and so is the list"
    );
    assert!(cx.debug_bounds("nav-list").is_none());

    // A Thread that finishes while the operator watches it is seen at
    // once, and held under Done, not Working: the finish is the news.
    let on = view
        .read_with(cx, |view, _| view.cockpit.roster().focused_thread())
        .unwrap();
    let stream = threads.iter().position(|thread| *thread == on).unwrap();
    let send = |event: SessionEvent| fake.streams.borrow()[stream].send(event).unwrap();
    send(SessionEvent::TextDelta {
        text: "thinking".into(),
    });
    tick(cx);
    assert_eq!(listed(&view, cx), vec![(Working, vec![on])]);
    send(SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Completed,
        cost_usd: None,
    });
    tick(cx);
    assert_ne!(
        view.read_with(cx, |view, _| view.thread_row(on).status),
        nav::RowStatus::Done,
        "seen at once"
    );
    assert_eq!(listed(&view, cx), vec![(Done, vec![on])], "held under Done");
    // Work again moves it back at once; an interrupt is no finish, so
    // with no Notice it leaves rather than being held.
    send(SessionEvent::TextDelta {
        text: "again".into(),
    });
    tick(cx);
    assert_eq!(listed(&view, cx), vec![(Working, vec![on])]);
    send(SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Interrupted,
        cost_usd: None,
    });
    tick(cx);
    assert_eq!(listed(&view, cx), vec![], "interrupted: not held");
    // Finished and held, it leaves when focus moves on.
    send(SessionEvent::TextDelta {
        text: "once more".into(),
    });
    tick(cx);
    send(SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Completed,
        cost_usd: None,
    });
    tick(cx);
    assert_eq!(listed(&view, cx), vec![(Done, vec![on])]);
    let tree_row = cx
        .debug_bounds(format!("nav-thread-{}", threads[3].get()).leak())
        .expect("a tree row");
    cx.simulate_click(tree_row.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(listed(&view, cx), vec![], "focus moved on: it leaves");
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
    use crate::theme::{BOARD_PAD, NAV_AUTO_RAIL_BOARD_W, NAV_AUTO_RAIL_HYSTERESIS, NAV_WIDTH};
    let (core, _fake) = cockpit("nav-auto-rail", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    let saved = view.read_with(cx, |view, _| view.prefs.settings.nav_collapsed);
    let threshold = NAV_WIDTH + BOARD_PAD + NAV_AUTO_RAIL_BOARD_W;
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

    let line_id: &'static str = format!("nav-line2-{}", thread.get()).leak();
    let line = cx.debug_bounds(line_id).expect("the row keeps its line 2");
    let title_id: &'static str = format!("nav-title-{}", thread.get()).leak();
    let title = cx.debug_bounds(title_id).expect("the one row grammar");
    assert!(title.bottom() <= line.top() + px(0.5));
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
    cx.simulate_resize(gpui::size(px(560.), px(700.)));
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
