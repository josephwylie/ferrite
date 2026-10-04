//! The terminal-native nav's parity behaviour: non-exclusive Groups (R1,
//! N-17), the real folds (N-4), the selection following the board (N-5),
//! the one seam (N-8), and the content fade and titlebar cross-fade on
//! their own clocks (N-15, N-16).

use super::*;

fn sorted_threads(core: &Cockpit) -> Vec<ThreadId> {
    let mut threads = core.threads();
    threads.sort();
    threads
}

/// N-17 / R1: a Thread in Perf sweep (its home) and included in
/// Everything is listed under both Groups, is no loose row, and keeps one
/// Pane whichever Group the board shows. Its first Group is Perf sweep.
#[gpui::test]
fn a_thread_in_two_groups_is_listed_under_both_and_keeps_one_pane(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("nav-two-groups", 4);
    let threads = sorted_threads(&core);
    let perf = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    let everything = core
        .apply_group(GroupChange::Create {
            first: threads[2],
            second: threads[3],
        })
        .unwrap()
        .group
        .unwrap();
    core.apply_group(GroupChange::Include {
        thread: threads[0],
        group: everything,
        index: Some(0),
    })
    .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.update(cx, |view, cx| {
        let state = view.nav_state();
        let listed: Vec<GroupId> = state
            .groups
            .iter()
            .filter(|group| group.members.iter().any(|row| row.thread == threads[0]))
            .map(|group| group.id)
            .collect();
        assert_eq!(listed, [perf, everything], "listed under every Group");
        assert!(
            state.solos.iter().all(|row| row.thread != threads[0]),
            "its home is Perf sweep, so it is no loose row"
        );
        assert_eq!(
            view.cockpit.groups().of(threads[0]).map(|group| group.id),
            Some(perf),
            "its first Group in creation order"
        );
        let panes_of = |view: &CockpitView| {
            view.panes
                .iter()
                .filter(|pane| pane.thread() == Some(threads[0]))
                .count()
        };
        view.enter_group(everything, cx);
        assert_eq!(panes_of(view), 1, "one Pane on Everything's board");
        assert_eq!(view.visible_indices().len(), 3);
        view.enter_group(perf, cx);
        assert_eq!(panes_of(view), 1, "and the same one on Perf sweep's");
        assert_eq!(view.visible_indices().len(), 2);
    });
}

/// R1, the prototype's zeron rows: a Thread only ever *included* in a
/// Group has no home, so it is still a loose row under its Project — and
/// it is listed under the Group too.
#[gpui::test]
fn a_guest_only_thread_is_still_a_loose_row(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("nav-guest-loose", 4);
    let threads = sorted_threads(&core);
    let everything = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    for thread in [threads[2], threads[3]] {
        core.apply_group(GroupChange::Include {
            thread,
            group: everything,
            index: None,
        })
        .unwrap();
    }
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.read_with(cx, |view, _| {
        let state = view.nav_state();
        let loose: Vec<ThreadId> = state.ordered_solos().iter().map(|row| row.thread).collect();
        assert_eq!(loose, [threads[2], threads[3]], "the guests stay loose");
        assert_eq!(state.groups.len(), 1);
        assert_eq!(
            state.groups[0].members.len(),
            4,
            "Everything holds all four"
        );
    });
}

/// N-5: the one selection is the row of what the board shows — the Group
/// row on its Group's board (none of its members), the Thread's own row in
/// Solo — and carries the `❯`.
#[gpui::test]
fn the_selection_follows_what_the_board_shows(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("nav-selection", 3);
    let threads = sorted_threads(&core);
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.update(cx, |view, cx| {
        view.enter_group(group, cx);
        let state = view.nav_state();
        assert!(state.groups[0].selected, "the Group row is selected");
        assert!(
            state.groups[0].members.iter().all(|row| !row.selected),
            "its members carry no selection"
        );
        assert!(state.solos.iter().all(|row| !row.selected));

        view.focus_thread(threads[2], cx);
        assert_eq!(view.cockpit.roster().view(), View::Solo);
        let state = view.nav_state();
        assert!(!state.groups[0].selected);
        let selected: Vec<ThreadId> = state
            .solos
            .iter()
            .filter(|row| row.selected)
            .map(|row| row.thread)
            .collect();
        assert_eq!(selected, [threads[2]], "Solo selects the Thread's row");
    });
}

/// N-4: the triangles fold for real. A Group's triangle hides its members
/// and keeps its count, and the press stops there — the Group does not
/// open. Opening a folded Group leaves it folded, and a view switch keeps
/// the fold. A Project heading folds everything under it.
#[gpui::test]
fn the_triangles_fold_for_real_and_the_folds_hold(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("nav-folds", 3);
    let threads = sorted_threads(&core);
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let row = |thread: ThreadId| -> &'static str { format!("nav-thread-{}", thread.get()).leak() };
    assert!(
        cx.debug_bounds(row(threads[0])).is_some(),
        "open: members drawn"
    );
    let before = view.read_with(cx, |view, _| view.cockpit.roster().view());

    let fold: &'static str = format!("nav-group-fold-{}", group.get()).leak();
    let triangle = cx.debug_bounds(fold).expect("the Group's triangle");
    cx.simulate_click(triangle.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.nav_folds.contains(&NavFold::Group(group)));
        assert_eq!(
            view.cockpit.roster().view(),
            before,
            "the triangle folds; it does not open the Group"
        );
        let state = view.nav_state();
        assert!(state.groups[0].folded);
        assert_eq!(state.groups[0].members.len(), 2, "the count is unchanged");
    });
    assert!(cx.debug_bounds(row(threads[0])).is_none(), "members hidden");
    assert!(cx.debug_bounds(row(threads[1])).is_none());

    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        let state = view.nav_state();
        assert!(
            state.groups[0].folded,
            "opening a folded Group keeps it folded"
        );
        assert!(state.groups[0].selected);
    });
    view.update(cx, |view, cx| view.focus_thread(threads[2], cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.roster().view(), View::Solo);
        assert!(view.nav_state().groups[0].folded, "a view switch keeps it");
    });

    let heading = cx
        .debug_bounds("nav-project-section-0")
        .expect("the Project heading");
    cx.simulate_click(heading.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let project = view.read_with(cx, |view, _| {
        let state = view.nav_state();
        assert!(state.sections[0].folded, "the heading folds its Project");
        assert!(state.ordered_rows().is_empty(), "nothing under it is drawn");
        state.sections[0].project.expect("a named Project")
    });
    assert!(cx.debug_bounds(row(threads[2])).is_none());
    let group_row: &'static str = format!("nav-group-{}", group.get()).leak();
    assert!(cx.debug_bounds(group_row).is_none());
    assert!(
        cx.debug_bounds("nav-project-section-0").is_some(),
        "the heading stays: ▸ and its name"
    );

    view.update(cx, |view, cx| {
        view.set_nav_fold(NavFold::Project(project), false, cx);
        view.set_nav_fold(NavFold::Group(group), false, cx);
    });
    tick(cx);
    assert!(cx.debug_bounds(row(threads[0])).is_some(), "unfolded again");
    assert!(cx.debug_bounds(row(threads[2])).is_some());
}

/// N-8: the nav's one seam is its own 1px column right of the column, the
/// board starting after it; nothing else draws a seam, and folded at rest
/// the seam sits at x = 0 in the plane's colour.
#[gpui::test]
fn one_seam_column_sits_right_of_the_nav(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-one-seam", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let column = cx.debug_bounds("nav-column").expect("the column");
    let seam = cx.debug_bounds("nav-seam").expect("the seam");
    assert!(
        (seam.left() - column.right()).abs() < px(0.5),
        "{seam:?} {column:?}"
    );
    assert_eq!(seam.size.width, px(crate::theme::CHROME_SEAM_W));
    assert!(
        cx.debug_bounds("chrome-seam").is_none(),
        "one seam, not two"
    );
    view.read_with(cx, |view, _| {
        assert!(
            (view.board_left() - f32::from(seam.right())).abs() < 0.5,
            "the board starts after it"
        );
    });

    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    tick(cx);
    let seam = cx.debug_bounds("nav-seam").expect("the seam");
    assert!(
        seam.left() < px(0.5),
        "folded, the seam is at x = 0: {seam:?}"
    );
}

/// N-15 / N-16: folding fades the content 1 → 0 over 150ms (CSS `ease`)
/// while the width still rides its 200ms, and opening fades it 0 → 1 the
/// same way; the titlebar cell cross-fades from the chrome to the plane
/// over 200ms as one colour. Test-clock samples.
#[gpui::test]
fn the_content_fades_on_its_own_clock_while_the_width_rides(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let (core, _fake) = cockpit("nav-content-fade", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let sample = |view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let now = cx.background_executor().now();
            let reduced = crate::motion::reduced_motion(cx);
            (
                view.nav_content_opacity(now, reduced),
                view.nav_band_t(now, reduced),
                view.nav_column_now(cx),
            )
        })
    };
    let (fade, band, width) = sample(&view, cx);
    assert_eq!((fade, band, width), (1.0, 0.0, nav::WIDTH), "at rest, open");

    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    cx.executor().advance_clock(Duration::from_millis(75));
    let (fade, band, width) = sample(&view, cx);
    let eased = crate::motion::EASE.eval(0.5);
    assert!(
        (fade - (1.0 - eased)).abs() < 0.02,
        "folding, half-way through its 150ms on CSS ease: {fade}"
    );
    assert!(band > 0.0 && band < 1.0, "the cell mid cross-fade: {band}");
    assert!(width > nav::FOLDED_WIDTH && width < nav::WIDTH, "{width}");

    cx.executor().advance_clock(Duration::from_millis(100));
    let (fade, band, width) = sample(&view, cx);
    assert_eq!(fade, 0.0, "the content is gone at 150ms…");
    assert!(
        width > nav::FOLDED_WIDTH,
        "…while the width still rides: {width}"
    );
    assert!(band < 1.0, "and the cell is still cross-fading: {band}");

    cx.executor().advance_clock(Duration::from_millis(100));
    let (_, band, width) = sample(&view, cx);
    assert_eq!((band, width), (1.0, nav::FOLDED_WIDTH), "folded at rest");

    view.update(cx, |view, cx| view.set_nav_collapsed(false, cx));
    cx.executor().advance_clock(Duration::from_millis(75));
    let (fade, band, _) = sample(&view, cx);
    assert!(
        (fade - eased).abs() < 0.02,
        "opening, 0 → 1 on the same clock: {fade}"
    );
    assert!(band > 0.0 && band < 1.0, "{band}");

    cx.executor().advance_clock(Duration::from_millis(200));
    let (fade, band, width) = sample(&view, cx);
    assert_eq!((fade, band, width), (1.0, 0.0, nav::WIDTH), "open at rest");
}
