//! The nav: thread rows, the Activity view and the parked list.
#[allow(unused_imports)]
use super::*;

fn sorted_threads(core: &Cockpit) -> Vec<ThreadId> {
    let mut threads = core.threads();
    threads.sort();
    threads
}

/// A Thread's row is its card (sidebar.md §3): a 78px box with its
/// checkout line — 8 + 20, 4, 20, 2, 16 + 8 — its content 10px in, and a
/// card with nothing for line 3 hugs its two lines (60). Cards pitch at
/// their 2px item padding.
#[gpui::test]
fn a_thread_card_is_three_lines_in_a_fixed_box(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{NAV_CARD_H, NAV_CARD_H_SHORT, NAV_ROW_PAD_X};
    let (core, _) = cockpit("nav-card-box", 2);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    without_checkouts(&view, cx);
    view.update(cx, |view, cx| {
        view.facts.set_branches(vec![(
            threads[0],
            Some(ferrite_core::workspace::BranchStatus {
                branch: Some("t3/improve-sidebar-architecture".into()),
                ..Default::default()
            }),
        )]);
        view.notify_part(parts::Part::Nav, cx);
    });
    tick(cx);
    let at = |cx: &mut gpui::VisualTestContext, name: &str, thread: ThreadId| {
        let key: &'static str = format!("{name}-{}", thread.get()).leak();
        cx.debug_bounds(key).unwrap_or_else(|| panic!("{key} drawn"))
    };
    let card = at(cx, "nav-thread", threads[0]);
    let line1 = at(cx, "nav-card-line1", threads[0]);
    let line2 = at(cx, "nav-card-line2", threads[0]);
    let line3 = at(cx, "nav-card-line3", threads[0]);
    let title = at(cx, "nav-title", threads[0]);
    assert_eq!(card.size.height, px(NAV_CARD_H));
    assert_eq!(line1.top() - card.top(), px(8.0), "8px over line 1");
    assert_eq!(line1.size.height, px(20.0));
    assert_eq!(line2.top() - line1.bottom(), px(4.0));
    assert_eq!(line2.size.height, px(20.0));
    assert_eq!(line3.top() - line2.bottom(), px(2.0));
    assert_eq!(line3.size.height, px(16.0));
    assert_eq!(card.bottom() - line3.bottom(), px(8.0), "8px under line 3");
    assert!(
        (title.left() - (card.left() + px(NAV_ROW_PAD_X))).abs() < px(0.5),
        "the title 10px in: {title:?} in {card:?}"
    );
    let bare = at(cx, "nav-thread", threads[1]);
    assert_eq!(bare.size.height, px(NAV_CARD_H_SHORT), "no checkout: it hugs");
    let key: &'static str = format!("nav-card-line3-{}", threads[1].get()).leak();
    assert!(cx.debug_bounds(key).is_none());
    let (upper, lower) = if card.top() < bare.top() {
        (card, bare)
    } else {
        (bare, card)
    };
    assert!(
        lower.top() - upper.bottom() >= px(4.0),
        "2px item padding above and below each card"
    );
}

/// Under the pointer a card's status gives way to its strip — Snooze and
/// Park — at once; Snooze opens the Snooze menu, and the strip stays up
/// while that menu is open; Park parks the Thread.
#[gpui::test]
fn under_the_pointer_the_status_gives_way_to_snooze_and_park(cx: &mut TestAppContext) {
    let (core, _) = cockpit("nav-card-strip", 2);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = threads[0];
    let key = |name: &str| -> &'static str { format!("{name}-{}", thread.get()).leak() };
    assert!(cx.debug_bounds(key("nav-card-park")).is_none(), "no strip at rest");
    let card = cx.debug_bounds(key("nav-thread")).expect("the card");
    cx.simulate_mouse_move(card.center(), None, gpui::Modifiers::none());
    tick(cx);
    let snooze = cx.debug_bounds(key("nav-card-snooze")).expect("Snooze stands");
    assert!(cx.debug_bounds(key("nav-card-park")).is_some(), "and Park");
    assert!(
        cx.debug_bounds(key("nav-state")).is_none(),
        "in place of the status"
    );
    assert!(nav::testing::drawn_card(thread).is_some_and(|card| card.strip));

    cx.simulate_click(snooze.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.context_menu.as_ref().map(|menu| menu.target)),
        Some(MenuTarget::Snooze(thread)),
        "the Snooze menu"
    );
    // The pointer leaves for the menu: the strip holds.
    cx.simulate_mouse_move(gpui::point(px(900.), px(650.)), None, gpui::Modifiers::none());
    tick(cx);
    assert!(
        cx.debug_bounds(key("nav-card-park")).is_some(),
        "the strip stays up while its menu is open"
    );
    view.update(cx, |view, cx| {
        view.context_menu = None;
        cx.notify();
    });
    tick(cx);
    assert!(cx.debug_bounds(key("nav-card-park")).is_none());

    let card = cx.debug_bounds(key("nav-thread")).expect("the card");
    cx.simulate_mouse_move(card.center(), None, gpui::Modifiers::none());
    tick(cx);
    let park = cx.debug_bounds(key("nav-card-park")).expect("Park stands");
    cx.simulate_click(park.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.cockpit.thread(thread).is_none()),
        "Park parks the Thread"
    );
}

/// The cards the nav draws, top to bottom.
fn cards_top_down(
    cx: &mut gpui::VisualTestContext,
    threads: &[ThreadId],
) -> Vec<(ThreadId, gpui::Bounds<Pixels>)> {
    let mut cards: Vec<_> = threads
        .iter()
        .filter_map(|thread| {
            let key: &'static str = format!("nav-thread-{}", thread.get()).leak();
            cx.debug_bounds(key).map(|bounds| (*thread, bounds))
        })
        .collect();
    cards.sort_by(|a, b| a.1.top().partial_cmp(&b.1.top()).unwrap());
    cards
}

/// Press-and-sweep: a press on one card's Park, dragged across two more
/// cards while held, picks each card it crosses and parks all three on
/// release; the card it never reached stays.
#[gpui::test]
fn a_park_press_swept_across_three_cards_parks_all_three(cx: &mut TestAppContext) {
    let (core, _) = cockpit("nav-card-sweep", 4);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    tick(cx);
    let cards = cards_top_down(cx, &threads);
    assert_eq!(cards.len(), 4, "every card drawn");
    let (first, a) = cards[0];
    cx.simulate_mouse_move(a.center(), None, gpui::Modifiers::none());
    tick(cx);
    let park: &'static str = format!("nav-card-park-{}", first.get()).leak();
    let park = cx.debug_bounds(park).expect("Park under the pointer");
    cx.simulate_mouse_down(park.center(), MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    for (thread, card) in &cards[1..3] {
        cx.simulate_mouse_move(card.center(), Some(MouseButton::Left), gpui::Modifiers::none());
        tick(cx);
        assert!(
            view.read_with(cx, |view, _| view.thread_row(*thread).picked),
            "a swept card wears the picked fill"
        );
    }
    let (_, last) = cards[2];
    cx.simulate_mouse_up(last.center(), MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    let open = |view: &CockpitView, thread: ThreadId| view.cockpit.thread(thread).is_some();
    for (thread, _) in &cards[..3] {
        assert!(!view.read_with(cx, |view, _| open(view, *thread)), "swept and parked");
    }
    assert!(
        view.read_with(cx, |view, _| open(view, cards[3].0)),
        "the card the sweep never reached stays"
    );
}

/// ⌘-click picks a card without opening it and ⇧-click extends the pick
/// over the cards between; a right click on a picked card offers
/// `Park (3)`, which parks them all.
#[gpui::test]
fn cmd_and_shift_click_pick_cards_and_park_takes_the_pick(cx: &mut TestAppContext) {
    let (core, _) = cockpit("nav-card-pick", 4);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    tick(cx);
    let cards = cards_top_down(cx, &threads);
    let shown = view.read_with(cx, |view, _| view.focused_thread());
    cx.simulate_click(cards[0].1.center(), gpui::Modifiers::command());
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.focused_thread()),
        shown,
        "a picking press does not open the card"
    );
    cx.simulate_click(cards[2].1.center(), gpui::Modifiers::shift());
    tick(cx);
    let picked: Vec<ThreadId> = cards[..3].iter().map(|(thread, _)| *thread).collect();
    assert_eq!(view.read_with(cx, |view, _| view.nav_picked.clone()), picked);
    assert!(view.read_with(cx, |view, _| view.thread_row(picked[1]).picked));

    cx.simulate_mouse_down(cards[1].1.center(), MouseButton::Right, gpui::Modifiers::none());
    tick(cx);
    let park = view.read_with(cx, |view, _| {
        view.context_rows(MenuTarget::Thread(picked[1]))
            .into_iter()
            .flatten()
            .find(|(_, verb)| *verb == MenuVerb::Park)
            .map(|(item, _)| item.label.to_string())
    });
    assert_eq!(park.as_deref(), Some("Park (3)"));
    view.update_in(cx, |view, window, cx| {
        view.run_menu_verb(MenuTarget::Thread(picked[1]), MenuVerb::Park, window, cx)
    });
    tick(cx);
    for thread in &picked {
        assert!(view.read_with(cx, |view, _| view.cockpit.thread(*thread).is_none()));
    }
    assert!(view.read_with(cx, |view, _| view.cockpit.thread(cards[3].0).is_some()));
}

/// A working card says how long its run has worked — `42s`, `12m`,
/// `1h 5m` — from the turn's start, in the working blue; a working card
/// not shown recedes to 70% and comes up to full under the pointer over
/// 150ms.
#[gpui::test]
fn a_working_card_counts_its_run_and_recedes(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{NAV_CARD_RECEDED_OPACITY, NAV_WORKING_INK};
    ferrite_ui::motion::testing::drive();
    let clock = hold_wall_clock();
    let (mut core, _) = cockpit("nav-card-working", 2);
    let threads = sorted_threads(&core);
    core.send(threads[0], "work".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    view.update(cx, |view, cx| view.focus_thread(threads[1], cx));
    tick(cx);
    let label = |view: &CockpitView| {
        let row = view.thread_row(threads[0]);
        nav::status_label(
            &row,
            ferrite_core::clock::instant(),
            ferrite_core::clock::system_time(),
        )
        .expect("a label")
    };
    let started = view.read_with(cx, |view, _| view.thread_row(threads[0]).working_since);
    assert!(started.is_some(), "the run's start is on the row");
    let first = view.read_with(cx, |view, _| label(view));
    assert_eq!(first.ink, NAV_WORKING_INK);
    for (by, text) in [
        (42, "Working 42s"),
        (12 * 60 - 42, "Working 12m"),
        (65 * 60 - 12 * 60, "Working 1h 5m"),
    ] {
        clock.advance(Duration::from_secs(by));
        assert_eq!(view.read_with(cx, |view, _| label(view)).text, text);
    }

    view.update(cx, |view, cx| view.set_shelf_open(nav::Shelf::Working, true, cx));
    tick(cx);
    let drawn = nav::testing::drawn_card(threads[0]).expect("drawn");
    assert!(drawn.receded);
    assert_eq!(drawn.opacity, NAV_CARD_RECEDED_OPACITY, "70% at rest");
    let key: &'static str = format!("nav-thread-{}", threads[0].get()).leak();
    let card = cx.debug_bounds(key).expect("the card");
    cx.simulate_mouse_move(card.center(), None, gpui::Modifiers::none());
    cx.run_until_parked();
    let rising = nav::testing::drawn_card(threads[0]).unwrap().opacity;
    assert!(rising < 1.0, "it fades, not snaps: {rising}");
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert_eq!(
        nav::testing::drawn_card(threads[0]).unwrap().opacity,
        1.0,
        "full under the pointer after 150ms"
    );
    assert!(
        !nav::testing::drawn_card(threads[1]).unwrap().receded,
        "the shown Thread never recedes"
    );
}

/// C8 and ADR 0040: while Threads wait, their own cards say so — what
/// each waits for — and head the list, oldest request first; there is no
/// second strip of them. ⌘D walks them in the same order, and — the nav
/// being open — no system toast card shows.
#[gpui::test]
fn two_decisions_head_the_list_and_no_toast(cx: &mut TestAppContext) {
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
        let first: Vec<ThreadId> = state.active.iter().take(2).map(|row| row.thread).collect();
        assert_eq!(
            first,
            [threads[2], threads[0]],
            "the requests head the list, oldest first"
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

/// Rule 2.7.7: a window too narrow for the column beside a board folds
/// the nav — shown, never saved — and unfolds only past the threshold
/// plus its hysteresis. cmd-B overrides it.
#[gpui::test]
fn a_narrow_window_folds_the_nav_without_saving_it(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{NAV_AUTO_FOLD_BOARD_W, NAV_AUTO_FOLD_HYSTERESIS, NAV_WIDTH};
    let (core, _fake) = cockpit("nav-auto-rail", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    let saved = view.read_with(cx, |view, _| view.prefs.settings.nav_collapsed);
    // A Solo Pane leaves no gutter at the window's edge (theme rule 2).
    let threshold = NAV_WIDTH + NAV_AUTO_FOLD_BOARD_W;
    cx.simulate_resize(gpui::size(px(720.), px(900.)));
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.nav_folded()),
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
    assert!(view.read_with(cx, |view, _| view.nav_folded()), "held");
    cx.simulate_resize(gpui::size(
        px(threshold + NAV_AUTO_FOLD_HYSTERESIS + 1.),
        px(900.),
    ));
    tick(cx);
    assert!(!view.read_with(cx, |view, _| view.nav_folded()), "restored");
    // Narrow again; cmd-B holds the column open.
    cx.simulate_resize(gpui::size(px(720.), px(900.)));
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.nav_folded()));
    view.update(cx, |view, cx| view.toggle_nav_now(cx));
    tick(cx);
    assert!(
        !view.read_with(cx, |view, _| view.nav_folded()),
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

/// The Parked shelf (ADR 0040): folded at first, its header counts its
/// Threads; a press opens it and another folds it, and a right press
/// opens its menu. Its delete arms on the first press and, on the second,
/// erases every Thread the shelf lists — and only those: the open Threads
/// stay, and with nothing left the shelf goes.
#[gpui::test]
fn the_parked_shelf_shows_its_rows_and_its_menu_deletes_them(cx: &mut TestAppContext) {
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
        .debug_bounds("nav-shelf-parked")
        .expect("the Parked shelf's header");
    assert!(cx.debug_bounds("nav-shelf-parked-count").is_some());
    assert!(
        cx.debug_bounds(first_parked).is_none(),
        "folded, the shelf draws no parked row"
    );
    assert!(cx.debug_bounds(live).is_some(), "an open Thread is a card");

    cx.simulate_click(door.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.shelf_open(nav::Shelf::Parked), "the press opens it")
    });
    assert!(
        cx.debug_bounds(first_parked).is_some(),
        "the parked rows are drawn"
    );
    assert!(cx.debug_bounds(live).is_some(), "under the cards");
    let door = cx.debug_bounds("nav-shelf-parked").unwrap();
    cx.simulate_click(door.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.shelf_open(nav::Shelf::Parked), "a second press folds it")
    });
    let door = cx.debug_bounds("nav-shelf-parked").unwrap();

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
            .expect("a right press opens the shelf's menu");
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
        cx.debug_bounds("nav-shelf-parked").is_none(),
        "nothing parked, no shelf"
    );
}

/// ⌘I (ADR 0040): the list takes the keys on its first card — the
/// request that has waited longest — and Esc, or ⌘I again, gives them
/// back. ⌘D walks the same queue.
#[gpui::test]
fn cmd_i_hands_the_list_the_keys_on_its_first_card(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("nav-cmd-i", 3);
    let threads = core.threads();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    fake.streams.borrow()[2].send(decision("perm_3")).unwrap();
    tick(cx);
    fake.streams.borrow()[0].send(decision("perm_1")).unwrap();
    tick(cx);
    let held = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.update_in(cx, |view, window, _| {
            view.nav_keys_held && view.nav_focus.is_focused(window)
        })
    };
    let row = |thread: ThreadId| -> &'static str { format!("nav-thread-{}", thread.get()).leak() };
    let oldest = cx.debug_bounds(row(threads[2])).expect("its card");
    let next = cx.debug_bounds(row(threads[0])).expect("its card");
    assert!(oldest.top() < next.top(), "oldest first");

    cx.simulate_keystrokes("cmd-i");
    tick(cx);
    assert!(held(&view, cx));
    assert_eq!(
        view.read_with(cx, |view, _| view.nav_cursor.clone().map(|cursor| cursor.item)),
        Some(nav::NavItem::Thread(threads[2])),
        "on the first card"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.cockpit.answer_target()),
        Some(threads[2]),
        "⌘D answers the request that has waited longest"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!held(&view, cx), "esc");
    cx.simulate_keystrokes("cmd-i");
    tick(cx);
    assert!(held(&view, cx));
    cx.simulate_keystrokes("cmd-i");
    tick(cx);
    assert!(!held(&view, cx), "⌘I again gives the keys back");
}

/// A Thread that starts work leaves the Active cards for the Working
/// shelf, newest send first, and comes back when its turn ends.
#[gpui::test]
fn a_working_thread_moves_to_the_working_shelf_and_back(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("nav-working-shelf", 3);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let sections = |view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            let state = view.nav_state();
            let ids = |rows: &[nav::ThreadRow]| rows.iter().map(|row| row.thread).collect::<Vec<_>>();
            (ids(&state.active), ids(&state.working))
        })
    };
    let (active, working) = sections(&view, cx);
    assert_eq!(active.len(), 3);
    assert!(working.is_empty());
    let busy = threads[1];

    fake.streams.borrow()[1]
        .send(SessionEvent::TextDelta {
            text: "thinking".into(),
        })
        .unwrap();
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.thread_row(busy).status),
        nav::RowStatus::Working,
        "the premise: it started work"
    );
    let (active, working) = sections(&view, cx);
    assert_eq!(working, [busy], "on the Working shelf");
    assert!(!active.contains(&busy), "and nowhere else");

    fake.streams.borrow()[1]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    let (active, working) = sections(&view, cx);
    assert!(working.is_empty());
    assert!(active.contains(&busy), "back among the cards");
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
/// second time. The row lives in the Parked shelf, so that is opened
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
        .debug_bounds("nav-shelf-parked")
        .expect("the Parked shelf's header");
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
        !view.update_in(cx, |view, window, _| view.nav_focus.is_focused(window)),
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

/// F2 on the keyboard cursor's card renames its Thread in place.
#[gpui::test]
fn f2_renames_the_cursors_card(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (core, _fake) = cockpit("nav-f2-rename", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    tick(cx);
    cx.simulate_keystrokes("cmd-shift-e");
    tick(cx);
    let Some(nav::NavItem::Thread(thread)) =
        view.read_with(cx, |view, _| view.nav_cursor.clone().map(|c| c.item))
    else {
        panic!("the cursor on a card");
    };
    cx.simulate_keystrokes("f2");
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.renaming_thread(thread)),
        "the card's title is the rename field"
    );
}

/// The list fades at whichever edge has rows past it, 12px deep, never
/// slicing a row against the head or the foot: the foot while rows remain
/// below, the top once it has scrolled, and neither edge with nothing past
/// it.
#[gpui::test]
fn the_list_fades_at_the_edges_rows_pass(cx: &mut TestAppContext) {
    let (core, _) = cockpit("nav-edge-fades", 24);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(320.)));
    cx.run_until_parked();
    let shows = |cx: &mut gpui::VisualTestContext, id: &'static str| cx.debug_bounds(id).is_some();
    assert!(shows(cx, "nav-fade-foot"), "rows below: the foot fades");
    assert_eq!(
        cx.debug_bounds("nav-fade-foot").unwrap().size.height,
        px(ferrite_ui::theme::NAV_EDGE_FADE)
    );
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

/// Claude's `Pinned`: the row menu's `Pin` puts a Thread's card at the
/// top of the list, over the others, and nowhere else; the pin is saved
/// with the settings; `Unpin` puts the card back among them.
#[gpui::test]
fn a_pinned_thread_heads_the_list_until_unpinned(cx: &mut TestAppContext) {
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
    // Claude's order: where it opens, then its pin.
    assert_eq!(
        view.read_with(cx, |view, _| menu(view))[..2],
        ["Open in", "Pin"]
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
            state.active.iter().all(|row| row.thread != threads[2]),
            "listed once"
        );
        assert_eq!(menu(view)[1], "Unpin");
    });
    let card = |cx: &mut gpui::VisualTestContext, thread: ThreadId| {
        cx.debug_bounds(format!("nav-thread-{}", thread.get()).leak())
            .expect("its card")
    };
    let pinned = card(cx, threads[2]);
    for other in &threads[..2] {
        assert!(pinned.bottom() <= card(cx, *other).top(), "at the top");
    }

    view.update_in(cx, |view, window, cx| {
        view.run_menu_verb(MenuTarget::Thread(threads[2]), MenuVerb::Unpin, window, cx)
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.prefs.settings.pinned_threads.is_empty());
        assert!(view.nav_state().pinned.is_empty());
    });
}

