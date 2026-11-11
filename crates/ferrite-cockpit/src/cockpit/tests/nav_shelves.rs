//! The nav's one list (ADR 0040): the shelves anchored at its foot, the
//! list's motion, the search row, the foot's undo notice and the
//! keyboard across the list.

use super::*;

fn sorted_threads(core: &Cockpit) -> Vec<ThreadId> {
    let mut threads = core.threads();
    threads.sort();
    threads
}

fn card(thread: ThreadId) -> &'static str {
    format!("nav-thread-{}", thread.get()).leak()
}

/// The shelves sit at the list's foot over the nav's foot, folded at
/// first, each header counting its Threads; a folded shelf still draws
/// the shown Thread's row; a press opens it, and the fold is kept in the
/// settings.
#[gpui::test]
fn the_shelves_are_anchored_over_the_foot_and_keep_their_fold(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("nav-shelves", 4);
    let threads = sorted_threads(&core);
    core.park(threads[0]).unwrap();
    core.park(threads[1]).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    let header = cx.debug_bounds("nav-shelf-parked").expect("the Parked shelf");
    assert_eq!(header.size.height, px(ferrite_ui::theme::NAV_SHELF_H));
    let foot = cx.debug_bounds("nav-foot").expect("the foot");
    assert!(
        foot.top() - header.bottom() < px(ferrite_ui::theme::NAV_SHELF_H),
        "anchored over the foot: {header:?} {foot:?}"
    );
    let live = cx.debug_bounds(card(threads[2])).expect("a card");
    assert!(live.bottom() < header.top(), "the cards stack from the top");
    assert!(
        cx.debug_bounds("nav-shelf-working").is_none(),
        "an empty shelf draws nothing"
    );
    assert!(cx.debug_bounds(card(threads[0])).is_none(), "folded at first");
    view.read_with(cx, |view, _| {
        assert!(view.prefs.settings.open_shelves.is_empty())
    });

    cx.simulate_click(header.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds(card(threads[0])).is_some(), "open");
    view.read_with(cx, |view, _| {
        assert_eq!(view.prefs.settings.open_shelves, [nav::Shelf::Parked]);
    });
}

/// A folded shelf keeps the shown Thread's row: a snoozed Thread on the
/// board stays in the list under its folded header.
#[gpui::test]
fn a_folded_shelf_still_shows_the_shown_threads_row(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-shelf-shown", 3);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    tick(cx);
    let (shown, other) = view.update(cx, |view, cx| {
        let shown = view.focused_thread().expect("a shown Thread");
        let other = view
            .cockpit
            .threads()
            .into_iter()
            .find(|thread| *thread != shown)
            .unwrap();
        let later = ferrite_core::clock::system_time() + Duration::from_secs(3600);
        view.snooze_thread(shown, Some(later), cx);
        view.snooze_thread(other, Some(later), cx);
        (shown, other)
    });
    tick(cx);
    assert!(cx.debug_bounds("nav-shelf-snoozed").is_some());
    assert!(cx.debug_bounds(card(shown)).is_some(), "the shown row stays");
    assert!(cx.debug_bounds(card(other)).is_none(), "the rest fold away");
}

/// The Parked shelf draws ten rows, then `Show 25 more`.
#[gpui::test]
fn the_parked_shelf_shows_ten_then_twenty_five_more(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("nav-parked-pages", 13);
    let threads = sorted_threads(&core);
    for thread in &threads[1..] {
        core.park(*thread).unwrap();
    }
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(2400.)));
    view.update(cx, |view, cx| {
        view.set_shelf_open(nav::Shelf::Parked, true, cx)
    });
    tick(cx);
    let drawn = |cx: &mut gpui::VisualTestContext| {
        threads[1..]
            .iter()
            .filter(|thread| cx.debug_bounds(card(**thread)).is_some())
            .count()
    };
    assert_eq!(drawn(cx), 10);
    let more = cx.debug_bounds("nav-parked-more").expect("Show 25 more");
    cx.simulate_click(more.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(drawn(cx), 12, "all twelve");
    assert!(cx.debug_bounds("nav-parked-more").is_none());
}

/// The list's motion (T3's FLIP): a card the list re-sorts slides from
/// where it was over 150ms — part way at 75ms — and one that leaves is
/// drawn as a ghost until its fade ends.
#[gpui::test]
fn a_resorted_card_slides_and_a_parked_one_leaves_a_ghost(cx: &mut TestAppContext) {
    ferrite_ui::motion::testing::drive();
    let (core, fake) = cockpit("nav-flip", 3);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    tick(cx);
    let mover = threads[2];
    let travel = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let now = cx.background_executor().now();
            view.nav_motion
                .borrow()
                .offset(super::super::nav_list::NavKey::Card(mover), now)
        })
    };
    leap(cx, Duration::from_millis(400));
    assert_eq!(travel(&view, cx), None, "at rest");

    fake.streams.borrow()[2].send(decision("perm_flip")).unwrap();
    tick(cx);
    let first = travel(&view, cx).expect("it travels");
    assert_eq!(
        view.read_with(cx, |view, _| view.nav_state().active[0].thread),
        mover,
        "a request lifts it to the top"
    );
    assert!(first > 0.0, "from below, where it was: {first}");
    step(cx, 40);
    let mid = travel(&view, cx).expect("still on its way at 75ms");
    assert!(0.0 < mid && mid < first, "part way: {first} then {mid}");
    leap(cx, Duration::from_millis(400));
    assert_eq!(travel(&view, cx), None, "landed");

    view.update(cx, |view, cx| view.park_listed(&[threads[0]], cx));
    tick(cx);
    view.read_with(cx, |view, cx| {
        let now = cx.background_executor().now();
        assert!(
            view.nav_motion.borrow().ghost_count(now) > 0,
            "its card leaves as a ghost"
        )
    });
    leap(cx, Duration::from_millis(400));
    view.read_with(cx, |view, cx| {
        let now = cx.background_executor().now();
        assert_eq!(view.nav_motion.borrow().ghost_count(now), 0, "and goes")
    });
}

/// Typing in the search row replaces the list with its results: a row
/// per title that holds the words, ↓ and ⏎ open one, and with no match
/// `No threads found`. Esc clears the field and brings the list back.
#[gpui::test]
fn the_search_row_replaces_the_list_with_its_results(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (core, _fake) = cockpit("nav-search", 3);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    tick(cx);
    let field = cx.debug_bounds("nav-search").expect("the field");
    assert_eq!(field.size.height, px(ferrite_ui::theme::NAV_SEARCH_H));
    assert!(cx.debug_bounds("nav-search-placeholder").is_some(), "Search");
    for button in ["nav-filter", "nav-add-project", "nav-new-thread"] {
        let bounds = cx.debug_bounds(button).expect(button);
        assert_eq!(bounds.size.height, px(ferrite_ui::theme::NAV_HEAD_BUTTON));
    }
    let name = "Needle in the nav";
    view.update(cx, |view, cx| {
        view.cockpit.rename_thread(threads[1], name).unwrap();
        view.facts.renamed(&view.cockpit, threads[1]);
        view.refresh_names();
        cx.notify();
    });
    tick(cx);

    cx.simulate_click(field.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input("needle");
    tick(cx);
    let typed = view.read_with(cx, |view, cx| view.nav_search.read(cx).text().to_string());
    assert!(cx.debug_bounds("nav-results").is_some(), "the results; typed {typed:?}");
    let result: &'static str = format!("nav-result-{}", threads[1].get()).leak();
    let row = cx.debug_bounds(result).expect("its row");
    assert_eq!(row.size.height, px(ferrite_ui::theme::NAV_RESULT_H));
    assert!(cx.debug_bounds(card(threads[0])).is_none(), "in the list's place");

    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.focused_thread()),
        Some(threads[1])
    );
    assert!(cx.debug_bounds("nav-results").is_none(), "cleared on open");

    cx.simulate_click(field.center(), gpui::Modifiers::none());
    cx.simulate_input("nothing-is-called-this");
    tick(cx);
    assert!(cx.debug_bounds("nav-no-results").is_some(), "No threads found");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(cx.debug_bounds("nav-no-results").is_none());
    assert!(cx.debug_bounds(card(threads[0])).is_some(), "the list is back");
}

/// The foot: Settings, Pull requests and Usage at 32px; a Park stands
/// the undo notice in the free middle, and ⌘Z (the list holding the
/// keys) revives what it parked.
#[gpui::test]
fn the_foot_offers_undo_for_a_park(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (core, _fake) = cockpit("nav-undo", 3);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    tick(cx);
    for button in ["nav-settings", "nav-pull-requests", "nav-usage"] {
        let bounds = cx.debug_bounds(button).expect(button);
        assert_eq!(bounds.size.height, px(ferrite_ui::theme::NAV_FOOT_BUTTON));
    }
    assert!(cx.debug_bounds("nav-undo-notice").is_none());
    view.update(cx, |view, cx| view.park_listed(&[threads[1]], cx));
    tick(cx);
    assert!(cx.debug_bounds("nav-undo-notice").is_some(), "Parked, Undo ⌘Z");
    view.read_with(cx, |view, _| {
        assert!(view.facts.parked().contains(&threads[1]))
    });

    cx.simulate_keystrokes("cmd-shift-e cmd-z");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.facts.parked().is_empty(), "⌘Z revived it");
        assert!(view.nav_undo.is_none());
    });
    assert!(cx.debug_bounds("nav-undo-notice").is_none());

    view.update(cx, |view, cx| view.park_listed(&[threads[1]], cx));
    tick(cx);
    leap(cx, Duration::from_millis(ferrite_ui::theme::NAV_UNDO_SHOWN_MS + 100));
    assert!(cx.debug_bounds("nav-undo-notice").is_none(), "it fades and goes");
}

/// The list's keyboard: Home and End reach its ends, ⏎ on a shelf's
/// header opens it, and ⌘W parks the cursor's Thread, the cursor going
/// with it into the Parked shelf.
#[gpui::test]
fn the_keyboard_walks_the_list_and_follows_a_parked_thread(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake) = cockpit("nav-keys-list", 3);
    let threads = sorted_threads(&core);
    core.park(threads[0]).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    tick(cx);
    let cursor = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.nav_cursor.clone().map(|cursor| cursor.item)
        })
    };
    cx.simulate_keystrokes("cmd-shift-e end");
    tick(cx);
    assert_eq!(
        cursor(&view, cx),
        Some(nav::NavItem::Shelf(nav::Shelf::Parked)),
        "End: the last row, the folded shelf's header"
    );
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.shelf_open(nav::Shelf::Parked)));
    cx.simulate_keystrokes("home");
    tick(cx);
    let first = cursor(&view, cx);
    assert!(matches!(first, Some(nav::NavItem::Thread(_))), "{first:?}");
    let Some(nav::NavItem::Thread(thread)) = first else {
        unreachable!()
    };
    cx.simulate_keystrokes("cmd-w");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.facts.parked().contains(&thread), "⌘W parked it");
        assert_eq!(
            view.nav_cursor.clone().map(|cursor| cursor.item),
            Some(nav::NavItem::Thread(thread)),
            "the cursor follows it"
        );
        assert!(view.nav_state().parked.iter().any(|row| row.thread == thread));
    });
    for key in ["shift-f10", "menu"] {
        cx.simulate_keystrokes(key);
        tick(cx);
        let target = view.read_with(cx, |view, _| view.context_menu.as_ref().map(|menu| menu.target));
        assert_eq!(target, Some(MenuTarget::Thread(thread)), "{key} opens the cursor's menu");
        view.update(cx, |view, cx| {
            view.context_menu = None;
            cx.notify();
        });
        tick(cx);
    }
}

/// The Filter menu lists every Project to filter by, and the Groups,
/// which open from there.
#[gpui::test]
fn the_filter_menu_scopes_a_project_and_opens_groups(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("nav-filter", 3);
    let group = group_all(&mut core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    tick(cx);
    let button = cx.debug_bounds("nav-filter").expect("Filter");
    cx.simulate_mouse_down(button.center(), gpui::MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    let (project, open_group) = view.read_with(cx, |view, _| {
        let menu = view.context_menu.as_ref().expect("its menu");
        assert_eq!(menu.target, MenuTarget::NavFilter);
        let verbs: Vec<MenuVerb> = menu.rows.iter().flatten().map(|(_, verb)| *verb).collect();
        assert_eq!(verbs[0], MenuVerb::FilterProject(None), "All projects");
        let project = verbs
            .iter()
            .find_map(|verb| match verb {
                MenuVerb::FilterProject(Some(project)) => Some(*project),
                _ => None,
            })
            .expect("a Project");
        (project, verbs.contains(&MenuVerb::OpenGroup(group)))
    });
    assert!(open_group, "the Group opens from here");
    view.update_in(cx, |view, window, cx| {
        view.run_menu_verb(MenuTarget::NavFilter, MenuVerb::FilterProject(Some(project)), window, cx);
        assert_eq!(view.nav_filter, Some(project));
        view.run_menu_verb(MenuTarget::NavFilter, MenuVerb::OpenGroup(group), window, cx);
        assert_eq!(view.cockpit.roster().view(), View::Group(group));
    });
}

/// #231's route reveal: when the shown Thread changes — from the board,
/// the palette, a toast — its card scrolls into view, not only when the
/// keyboard's cursor moves.
#[gpui::test]
fn the_shown_threads_card_scrolls_into_view(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-route-reveal", 24);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(420.)));
    tick(cx);
    let list = cx.debug_bounds("nav-list").expect("the list");
    let last = *threads.last().unwrap();
    let before = cx.debug_bounds(card(last)).expect("drawn, clipped");
    assert!(before.top() > list.bottom(), "the premise: below the fold");
    view.update(cx, |view, cx| view.focus_thread(last, cx));
    tick(cx);
    tick(cx);
    let after = cx.debug_bounds(card(last)).unwrap();
    assert!(
        after.top() >= list.top() && after.bottom() <= list.bottom() + px(1.0),
        "scrolled into view: {after:?} in {list:?}"
    );
}

/// A card under the pointer shows its detail to its right, top-aligned
/// 4px away and at most 320px wide, its pull request under a rule; with
/// its Snooze menu open it shows none.
#[gpui::test]
fn a_hovered_card_shows_its_detail_card(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-hover-card", 2);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    view.update(cx, |view, cx| {
        let pr = ferrite_core::workspace::PullRequest {
            number: 415,
            state: ferrite_core::workspace::PrState::Open,
            draft: false,
            url: None,
            title: Some("Thread cards in the nav".into()),
            checks: None,
            runs: Vec::new(),
        };
        view.facts.pr_answered(threads[0], Some(pr));
        cx.notify();
    });
    tick(cx);
    assert!(cx.debug_bounds("nav-hover-card").is_none(), "none at rest");

    let row = cx.debug_bounds(card(threads[0])).expect("its card");
    cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::none());
    tick(cx);
    let detail = cx.debug_bounds("nav-hover-card").expect("its detail");
    assert!(
        (detail.left() - row.right() - px(4.)).abs() <= px(0.5),
        "{detail:?} right of {row:?}"
    );
    assert!((detail.top() - row.top()).abs() <= px(0.5), "top-aligned");
    assert!(detail.size.width <= px(ferrite_ui::theme::NAV_HOVER_CARD_MAX_W));
    let prs = cx
        .debug_bounds("nav-hover-card-prs")
        .expect("its pull request");
    assert!(
        prs.bottom() <= detail.bottom(),
        "inside the card, at its foot"
    );

    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::Snooze(threads[0]), row.center(), cx)
    });
    tick(cx);
    assert!(
        cx.debug_bounds("nav-hover-card").is_none(),
        "not with its Snooze menu open"
    );
}

/// A parked slim row is 36px; under the pointer its age fades out and
/// Revive fades in over 150ms, not at once.
#[gpui::test]
fn a_slim_row_fades_its_age_into_revive(cx: &mut TestAppContext) {
    ferrite_ui::motion::testing::drive();
    let (mut core, _fake) = cockpit("nav-slim-hover", 2);
    let threads = sorted_threads(&core);
    core.park(threads[0]).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    view.update(cx, |view, cx| {
        view.set_shelf_open(nav::Shelf::Parked, true, cx)
    });
    tick(cx);
    let row = cx.debug_bounds(card(threads[0])).expect("its slim row");
    assert_eq!(row.size.height, px(ferrite_ui::theme::NAV_SLIM_H));
    assert_eq!(
        nav::testing::drawn_slim(threads[0]),
        Some(0.0),
        "its age at rest"
    );
    let revive: &'static str = format!("nav-revive-{}", threads[0].get()).leak();
    assert!(cx.debug_bounds(revive).is_none(), "no Revive at rest");

    cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::none());
    cx.run_until_parked();
    let rising = nav::testing::drawn_slim(threads[0]).unwrap();
    assert!(rising < 1.0, "it fades, not snaps: {rising}");
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert_eq!(
        nav::testing::drawn_slim(threads[0]),
        Some(1.0),
        "Revive after 150ms"
    );
    assert!(
        cx.debug_bounds(revive).is_some(),
        "Revive in the age's place"
    );
}
