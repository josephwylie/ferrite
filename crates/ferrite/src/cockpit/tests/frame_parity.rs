//! The frame package's parity tests (F-1 … F-18): the Pane edge, the two
//! altitudes, the wall tile and its quick answers, parked members, the
//! sidebar ride, the empty board, the reader and what opens beside, and the
//! board's seams.
#[allow(unused_imports)]
use super::*;

/// A board of `count` Threads in one Group, entered, at the app's size.
fn board_of<'a>(
    name: &str,
    count: usize,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    Fake,
    &'a mut gpui::VisualTestContext,
    GroupId,
) {
    let (mut core, fake) = cockpit(name, count);
    let group = group_all(&mut core);
    core.enter_group(group).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    (view, fake, cx, group)
}

/// F-1: the Pane's edge is an overlay, so its content starts at the Pane's
/// own edges — the head at the Pane's left and top, its mark 14px in and
/// its title a gap after it (SPEC §5.9) — and every Pane root is the
/// `PANE_GROUP` hover group.
#[gpui::test]
fn a_panes_content_starts_at_its_own_edges(cx: &mut TestAppContext) {
    let (view, _fake, cx, _group) = board_of("frame-edge-overlay", 4, cx);
    let rects = cx.update(|window, cx| view.read(cx).pane_rects(window));
    assert_eq!(rects.len(), 4);
    for (index, rect) in rects {
        let key = view.read_with(cx, |view, _| view.panes[index].thread().unwrap().get());
        let head = bounds(cx, format!("pane-head-{key}"));
        // gpui snaps every edge to a device pixel, so a head can stand up
        // to a pixel off the unsnapped split.
        assert!(
            (head.left() - px(rect.x)).abs() < px(1.) && (head.top() - px(rect.y)).abs() < px(1.),
            "the head starts at the Pane's corner: {head:?} / {rect:?}"
        );
        // The 38px row and its hairline under it.
        assert_eq!(head.size.height, px(crate::theme::PANE_HEAD_H + 1.));
        let mark = bounds(cx, format!("pane-head-dot-{key}"));
        assert!(
            (mark.left() - px(rect.x + crate::theme::PANE_HEAD_PAD_L)).abs() < px(1.),
            "the mark 14px in: {mark:?} / {rect:?}"
        );
        let title = bounds(cx, format!("pane-head-title-{key}"));
        assert!(
            (title.left() - (mark.right() + px(crate::theme::PANE_HEAD_GAP))).abs() < px(1.),
            "the title a gap after the mark: {title:?} / {mark:?}"
        );
        let root = bounds(cx, format!("pane-root-{key}"));
        assert!((root.left() - px(rect.x)).abs() < px(1.));
    }
    assert_eq!(pane::PANE_GROUP, "pane");
}

/// F-2: a waiting Pane wears the one attention edge whether it holds focus
/// or not — no focus ring inside it — and a calm focused Pane `TEXT_STRONG`.
#[gpui::test]
fn a_focused_waiting_pane_wears_only_the_attention_edge(cx: &mut TestAppContext) {
    let (view, fake, cx, _group) = board_of("frame-one-edge", 4, cx);
    fake.streams.borrow()[1]
        .send(decision("edge-focused"))
        .unwrap();
    tick(cx);
    view.update(cx, |view, cx| {
        view.focus_pane(1);
        cx.notify();
    });
    tick(cx);
    let (waiting, calm) = view.read_with(cx, |view, _| {
        (
            view.panes[1].thread().unwrap().get(),
            view.panes[0].thread().unwrap().get(),
        )
    });
    assert!(debug_bounds(cx, format!("pane-waiting-edge-{waiting}")).is_some());
    assert!(
        debug_bounds(cx, format!("pane-focus-edge-{waiting}")).is_none(),
        "focus on a waiting Pane is its head ground and title, not a second ring"
    );
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    tick(cx);
    assert!(debug_bounds(cx, format!("pane-focus-edge-{calm}")).is_some());
    assert!(debug_bounds(cx, format!("pane-waiting-edge-{waiting}")).is_some());
}

/// F-5 (R12): at 1440×900, nav open or folded, every board from 2 to 24
/// Panes draws transcript Panes or wall tiles — never a third altitude: a
/// 2×2 is four transcripts, a 3×3 nine tiles with no Composer.
#[gpui::test]
fn every_board_draws_transcripts_or_wall_tiles(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("frame-two-altitudes", 24);
    let threads = core.threads();
    let group = group_all(&mut core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    for folded in [false, true] {
        view.update(cx, |view, cx| {
            view.nav_forced_open = !folded;
            view.set_nav_collapsed(folded, cx);
        });
        for count in 2..=24usize {
            view.update(cx, |view, cx| {
                // The Group shrinks to `count` members, in order.
                for thread in &threads[count..] {
                    let _ = view
                        .cockpit
                        .apply_group(GroupChange::Leave { thread: *thread });
                }
                for thread in &threads[..count] {
                    if view
                        .cockpit
                        .groups()
                        .get(group)
                        .is_some_and(|g| !g.members.contains(thread))
                    {
                        let _ = view.cockpit.apply_group(GroupChange::Join {
                            thread: *thread,
                            group,
                            index: None,
                        });
                    }
                }
                if view.cockpit.groups().get(group).is_some() {
                    view.enter_group(group, cx);
                }
            });
            tick(cx);
            let (level, cells) = cx.update(|window, cx| {
                let view = view.read(cx);
                (view.board_level(window), view.pane_rects(window))
            });
            for (_, rect) in &cells {
                let own = Level::for_cell(ferrite_core::docview::Cell::new(rect.w, rect.h));
                assert!(
                    own == level || level == Level::Wall,
                    "{count} folded={folded}: one Level per board"
                );
            }
            match level {
                Level::Transcript => assert!(cx.debug_bounds("wall-tile").is_none()),
                Level::Wall => assert!(cx.debug_bounds("wall-tile").is_some()),
            }
        }
    }
}

/// F-5: a 3×3 at the app's size is nine wall tiles and no Composer; a 2×2
/// is four transcripts.
#[gpui::test]
fn a_three_by_three_is_tiles_and_a_two_by_two_transcripts(cx: &mut TestAppContext) {
    let (nine, _fake, wall, _group) = board_of("frame-nine-tiles", 9, cx);
    assert_eq!(
        wall.update(|window, cx| nine.read(cx).level_now(window)),
        Level::Wall
    );
    assert!(wall.debug_bounds("wall-tile").is_some());
    assert!(
        wall.debug_bounds("composer-block").is_none(),
        "no Composer on a tile"
    );
    let (view, _fake, cx, _group) = board_of("frame-four-transcripts", 4, cx);
    assert_eq!(
        cx.update(|window, cx| view.read(cx).level_now(window)),
        Level::Transcript
    );
    assert!(cx.debug_bounds("wall-tile").is_none());
}

/// F-7: a quick answer is 22px tall and as wide as its words and a cell
/// of padding each side, edgeless; its first button two cells in from the
/// tile's edge, its row half a row above the tile's foot.
#[gpui::test]
fn a_quick_answer_is_22px_and_as_wide_as_its_words(cx: &mut TestAppContext) {
    let (view, fake, cx, _group) = board_of("frame-quick-box", 9, cx);
    fake.streams.borrow()[1]
        .send(decision("quick-box"))
        .unwrap();
    tick(cx);
    let (key, rect) = cx.update(|window, cx| {
        let view = view.read(cx);
        let key = view.panes[1].thread().unwrap().get();
        let rect = view
            .pane_rects(window)
            .into_iter()
            .find(|(at, _)| *at == 1)
            .unwrap()
            .1;
        (key, rect)
    });
    let allow = bounds(cx, format!("wall-answer-{key}-allow"));
    assert_eq!(allow.size.height, px(crate::theme::QUICK_ANSWER_H));
    let width = 7.0 * crate::theme::CH + 2.0 * crate::theme::QUICK_ANSWER_PAD_X;
    assert!(
        (allow.size.width - px(width)).abs() <= px(1.),
        "`1 allow` is {width}px: {allow:?}"
    );
    assert!(
        (allow.left() - px(rect.x + crate::theme::WALL_PAD_X)).abs() <= px(0.5),
        "the first box two cells in: {allow:?} / {rect:?}"
    );
    assert!(
        (px(rect.y + rect.h) - allow.bottom() - px(crate::theme::WALL_PAD_Y)).abs() <= px(0.5),
        "half a row above the foot: {allow:?} / {rect:?}"
    );
}

/// F-7: at the wall the digits answer — `1` allow, `2` allow for this
/// thread, `3` deny and steer — the focused waiting tile, without moving
/// focus.
#[gpui::test]
fn the_wall_digits_answer_allow_always_and_deny(cx: &mut TestAppContext) {
    for (action, id) in [
        (1usize, "wall-pick-1"),
        (2, "wall-pick-2"),
        (3, "wall-pick-3"),
    ] {
        let (view, fake, cx, _group) = board_of(id, 9, cx);
        let SessionEvent::DecisionRequested { mut decision } = decision(id) else {
            unreachable!()
        };
        decision.suggestions = vec![ferrite_core::DecisionChoice {
            label: "Always allow Write".into(),
            value: serde_json::json!({ "rule": "Write" }),
            standing: true,
        }];
        fake.streams.borrow()[1]
            .send(SessionEvent::DecisionRequested { decision })
            .unwrap();
        tick(cx);
        view.update(cx, |view, cx| {
            view.focus_pane(1);
            cx.notify();
        });
        tick(cx);
        cx.update(|window, cx| {
            let pick: Box<dyn gpui::Action> = match action {
                1 => Box::new(PickOption1),
                2 => Box::new(PickOption2),
                _ => Box::new(PickOption3),
            };
            window.dispatch_action(pick, cx);
        });
        tick(cx);
        let answered = fake.answered.borrow().clone();
        let last = answered
            .last()
            .map(|(answered, answer)| (answered.as_str(), answer));
        match action {
            1 => assert!(
                matches!(last, Some((answered, DecisionAnswer::Allow { .. })) if answered == id),
                "1 allows: {answered:?}"
            ),
            2 => assert!(
                matches!(
                    last,
                    Some((answered, DecisionAnswer::AllowAlways { .. } | DecisionAnswer::Allow { .. }))
                        if answered == id
                ),
                "2 allows for this thread: {answered:?}"
            ),
            _ => assert!(
                matches!(last, Some((answered, DecisionAnswer::Deny { .. })) if answered == id),
                "3 denies: {answered:?}"
            ),
        }
        view.read_with(cx, |view, _| assert_eq!(view.focused(), 1, "focus stays"));
    }
}

/// F-8: entering a Group spends no Session on a parked member — its Pane is
/// the parked tile — and ⏎ on it reopens it, in place.
#[gpui::test]
fn a_parked_member_is_its_tile_until_enter_wakes_it(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("frame-parked-member", 3);
    let threads = core.threads();
    let group = group_all(&mut core);
    core.park(threads[2]).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.cockpit.thread(threads[2]).is_none(), "still parked");
        assert!(
            view.pane_for(threads[2]).is_some(),
            "but it holds its slot on the board"
        );
    });
    view.update(cx, |view, cx| {
        let index = view.pane_for(threads[2]).unwrap();
        view.focus_pane(index);
        cx.notify();
    });
    tick(cx);
    cx.update(|window, cx| window.dispatch_action(Box::new(Submit), cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.cockpit.thread(threads[2]).is_some(), "⏎ woke it");
        assert_eq!(view.cockpit.roster().view(), View::Group(group));
        assert_eq!(view.focused_thread(), Some(threads[2]));
    });
}

/// F-12: the board rides cmd-B with the column, every frame: mid-ride its
/// left edge is the column's width this frame, and the Pane
/// widths change from frame to frame — in Solo and on a 4-up board.
#[gpui::test]
fn the_board_rides_the_sidebar_every_frame(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    for count in [1usize, 4] {
        let (mut core, _fake) = cockpit(&format!("frame-ride-{count}"), count);
        if count > 1 {
            let group = group_all(&mut core);
            core.enter_group(group).unwrap();
        }
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        hold_nav_open(&view, cx);
        cx.simulate_resize(gpui::size(px(1440.), px(900.)));
        tick(cx);
        view.update(cx, |view, cx| {
            view.nav_forced_open = false;
            view.set_nav_collapsed(true, cx);
        });
        let mut widths = Vec::new();
        for _ in 0..6 {
            cx.executor().advance_clock(Duration::from_millis(25));
            let (left, column, width) = view.update_in(cx, |view, window, cx| {
                view.nav_ride.set(view.nav_ride_now(cx));
                let left = view.board_bounds(window).x;
                let column = view.nav_ride_now(cx).unwrap_or_else(|| view.nav_width());
                let width = view.pane_rects(window)[0].1.w;
                (left, column, width)
            });
            assert!(
                (left - column).abs() < 0.01,
                "{count}: the board starts at the column: {left} / {column}"
            );
            widths.push(width);
        }
        assert!(
            widths.windows(2).all(|pair| pair[1] > pair[0]),
            "{count}: the Panes widen every frame of the fold: {widths:?}"
        );
    }
}

/// F-12: a held ride (`fixture_hold_nav`) sits on the `RESIZE` curve.
#[gpui::test]
fn a_held_ride_sits_on_the_resize_curve(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("frame-ride-held", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let column = view.update(cx, |view, cx| {
        view.fixture_hold_nav(Some(0.5));
        view.nav_ride_now(cx).unwrap()
    });
    let expected = crate::motion::lerp(
        nav::FOLDED_WIDTH,
        nav::WIDTH,
        crate::motion::RESIZE.progress(0.5),
    );
    assert!((column - expected).abs() < 0.01, "{column} / {expected}");
    view.update(cx, |view, cx| {
        view.fixture_hold_nav(None);
        assert_eq!(view.nav_ride_now(cx), None);
    });
}

/// F-13: the empty board's rows walk with `Next` / `Previous`, wrapping,
/// and its recent list stops at four.
#[gpui::test]
fn the_empty_board_walks_its_rows_and_lists_four_recent(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("frame-empty-board", 6);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    view.update(cx, |view, cx| view.show_empty_board(cx));
    tick(cx);
    assert!(cx.debug_bounds("empty-board").is_some());
    for at in 0..5 {
        assert!(debug_bounds(cx, format!("empty-board-command-{at}")).is_some());
    }
    let step = |action: Box<dyn gpui::Action>, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| window.dispatch_action(action, cx));
        tick(cx);
    };
    step(Box::new(empty_board::Next), cx);
    step(Box::new(empty_board::Next), cx);
    view.read_with(cx, |view, _| assert_eq!(view.empty_cursor, 2));
    step(Box::new(empty_board::Previous), cx);
    step(Box::new(empty_board::Previous), cx);
    step(Box::new(empty_board::Previous), cx);
    view.read_with(cx, |view, _| assert_eq!(view.empty_cursor, 4, "it wraps"));
    let recent = view.read_with(cx, |view, _| view.cockpit.threads().len());
    assert!(recent > crate::theme::EMPTY_RECENT_MAX);
    assert_eq!(crate::theme::EMPTY_RECENT_MAX, 4);
    // Landing on a Thread leaves the empty board.
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    view.update(cx, |view, cx| view.focus_thread(thread, cx));
    tick(cx);
    assert!(cx.debug_bounds("empty-board").is_none());
}

/// SPEC §5.7: the bottom bar always shows `1 Solo` and a tab per Group,
/// open or not; a press on a Group's tab enters it, and on `Solo` with no
/// loose Thread it shows the empty board.
#[gpui::test]
fn the_bottom_bar_has_solo_and_every_group(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("frame-bottom-bar", 2);
    let group = group_all(&mut core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    assert!(cx.debug_bounds("bottom-bar-tab-solo").is_some());
    let tab = bounds(cx, format!("bottom-bar-tab-{}", group.get()));
    cx.simulate_click(tab.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.roster().view(), View::Group(group))
    });
    let solo = bounds(cx, "bottom-bar-tab-solo".to_string());
    cx.simulate_click(solo.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        cx.debug_bounds("empty-board").is_some(),
        "no loose Thread: `Solo` is the empty board"
    );
}

/// SPEC §5.7: the `Solo` tab's roll-up dot speaks for the Thread Solo
/// shows while it is the view — not for whichever loose Thread was written
/// last, which may be another — so the current tab's dot never contradicts
/// the board under it.
#[gpui::test]
fn the_solo_tabs_dot_follows_the_thread_it_shows(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("frame-solo-dot", 2);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let working = threads[0];
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "thinking".into(),
        })
        .unwrap();
    tick(cx);
    let dot =
        |cx: &mut gpui::VisualTestContext| cx.debug_bounds("bottom-bar-tab-solo-dot").is_some();
    view.update(cx, |view, cx| view.focus_thread(working, cx));
    tick(cx);
    assert!(dot(cx), "Solo shows the working Thread: green");
    view.update(cx, |view, cx| view.focus_thread(threads[1], cx));
    tick(cx);
    assert!(!dot(cx), "Solo shows the idle one: no dot");
}

/// SPEC §5.7: the bar is 30 high and full width, 14px in at each end:
/// the views' tabs (22 high, centred: 4 over and 4 under) at its left, the
/// vitals at its right ending in the clock. Its hairline lies on the
/// frame's last row above it, as the mockups draw it. `N need you` shows
/// only while a Thread waits, and opens the nav's Needs-you view; free
/// disk only under 20 GB, and never while unknown.
#[gpui::test]
fn the_bar_holds_the_views_and_the_vitals(cx: &mut TestAppContext) {
    let (view, fake, cx, group) = board_of("frame-bar-vitals", 2, cx);
    let bar = cx.debug_bounds("bottom-bar").unwrap();
    let window = cx.update(|window, _| window.viewport_size());
    assert_eq!(bar.size.height, px(crate::theme::STATUS_BAR_H));
    assert_eq!(crate::theme::STATUS_BAR_H, 30.0);
    assert_eq!(
        (bar.left(), bar.right()),
        (px(0.), window.width),
        "full width"
    );
    assert_eq!(bar.bottom(), window.height);
    let tabs = cx.debug_bounds("bottom-bar-tabs").unwrap();
    assert_eq!(tabs.left(), bar.left() + px(14.));
    let solo = bounds(cx, "bottom-bar-tab-solo".to_string());
    let work = bounds(cx, format!("bottom-bar-tab-{}", group.get()));
    assert_eq!(solo.size.height, px(22.));
    assert_eq!(
        solo.top() - bar.top(),
        bar.bottom() - solo.bottom(),
        "the tabs centred in the bar"
    );
    let rule = cx
        .debug_bounds("bottom-bar-rule")
        .expect("the bar's hairline");
    assert_eq!(
        (rule.top(), rule.bottom(), rule.size.width),
        (bar.top() - px(1.), bar.top(), window.width),
        "on the row above the bar, full width"
    );
    assert_eq!(work.left() - solo.right(), px(2.), "tabs 2px apart");
    let vitals = cx.debug_bounds("bottom-bar-vitals").unwrap();
    let clock = cx.debug_bounds("bottom-bar-clock").unwrap();
    assert_eq!(vitals.right(), bar.right() - px(14.));
    assert_eq!(clock.right(), vitals.right(), "the clock ends the bar");

    assert!(
        cx.debug_bounds("bottom-bar-needs-you").is_none(),
        "none waits"
    );
    fake.streams.borrow()[0]
        .send(SessionEvent::DecisionRequested {
            decision: Decision {
                delivery: Default::default(),
                kind: Default::default(),
                policy: Default::default(),
                id: "req0".into(),
                tool_use_id: "toolu_req0".into(),
                tool_name: "Bash".into(),
                description: "ls".into(),
                input: serde_json::json!({ "command": "ls" }),
                suggestions: vec![],
            },
        })
        .unwrap();
    tick(cx);
    let needs = cx
        .debug_bounds("bottom-bar-needs-you")
        .expect("one waits: `1 need you`");
    let vitals = cx.debug_bounds("bottom-bar-vitals").unwrap();
    let clock = cx.debug_bounds("bottom-bar-clock").unwrap();
    assert!(needs.left() >= vitals.left() && needs.right() <= clock.left());
    cx.simulate_click(needs.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.needs_you_shown()),
        "the Needs-you view is up"
    );

    for (free, shown) in [
        (None, false),
        (Some(30_000_000_000), false),
        (Some(7_500_000_000), true),
    ] {
        view.update(cx, |view, cx| view.free_disk_landed(free, cx));
        tick(cx);
        assert_eq!(
            cx.debug_bounds("bottom-bar-disk").is_some(),
            shown,
            "{free:?} free"
        );
    }
}

/// F-14: a reader opens beside the Solo Pane at a line — half and half,
/// the panel giving way — and closing it gives the Pane its board back.
#[gpui::test]
fn a_reader_opens_beside_the_pane_and_closes_back(cx: &mut TestAppContext) {
    let dir = scratch("frame-reader");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("nav.rs");
    std::fs::write(
        &file,
        "fn one() {}\nfn two() {}\nfn three() {}\nfn four() {}\n",
    )
    .unwrap();
    let (core, _fake) = cockpit("frame-reader-view", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let whole = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1.w);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_beside(
                beside::Beside::Reader {
                    path: file.clone(),
                    line: Some(3),
                },
                window,
                cx,
            )
        })
    });
    tick(cx);
    let (line, half) = cx.update(|window, cx| {
        let view = view.read(cx);
        (
            view.panes[0].preview.document().and_then(|doc| doc.line),
            view.pane_rects(window)[0].1.w,
        )
    });
    assert_eq!(line, Some(3));
    // The panel stays beside it (SPEC §5.6): the reader halves the board
    // the Pane had.
    let board = whole;
    assert!(
        (half - (board - crate::theme::BOARD_SEAM) / 2.0).abs() <= 1.0,
        "half and half: {half} of {board}"
    );
    assert!(cx.debug_bounds("reader-head").is_some());
    view.update(cx, |view, cx| view.close_reader(0, cx));
    tick(cx);
    let back = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1.w);
    assert!(
        (back - whole).abs() <= 0.5,
        "the board as it was: {back} / {whole}"
    );
}

/// F-15: another Thread opens beside the Solo Pane as a second Pane, and
/// the pair holds while focus stays on one of the two.
#[gpui::test]
fn a_thread_opens_beside_the_solo_pane(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("frame-thread-beside", 2);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    view.update(cx, |view, cx| view.focus_thread(threads[0], cx));
    tick(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_beside(beside::Beside::Thread(threads[1]), window, cx)
        })
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.visible_indices().len(),
            2,
            "two Panes on the Solo board"
        );
        assert_eq!(view.focused_thread(), Some(threads[0]), "focus stays home");
    });
    let rects = cx.update(|window, cx| view.read(cx).pane_rects(window));
    assert_eq!(rects.len(), 2);
    assert!(rects[0].1.x < rects[1].1.x, "the second Pane on the right");
    // Focus moving to the companion keeps the pair.
    view.update(cx, |view, cx| {
        let index = view.pane_for(threads[1]).unwrap();
        view.focus_pane(index);
        cx.notify();
    });
    tick(cx);
    view.read_with(cx, |view, _| assert_eq!(view.visible_indices().len(), 2));
}

/// F-18: a 2×2's vertical seam is one line: dragging it moves both rows'
/// boundary together.
#[gpui::test]
fn dragging_the_vertical_seam_moves_both_rows(cx: &mut TestAppContext) {
    let (view, _fake, cx, _group) = board_of("frame-seam-drag", 4, cx);
    let before = cx.update(|window, cx| view.read(cx).pane_rects(window));
    let band = bounds(cx, "board-seam-0-band-0".to_string());
    cx.simulate_mouse_down(band.center(), MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(
        band.center() + gpui::point(px(120.), px(0.)),
        MouseButton::Left,
        gpui::Modifiers::none(),
    );
    cx.simulate_mouse_up(
        band.center() + gpui::point(px(120.), px(0.)),
        MouseButton::Left,
        gpui::Modifiers::none(),
    );
    tick(cx);
    let after = cx.update(|window, cx| view.read(cx).pane_rects(window));
    let width = |rects: &Vec<(usize, layout::Rect)>, at: usize| rects[at].1.w;
    assert!(
        (width(&after, 0) - width(&before, 0) - 120.0).abs() <= 1.0,
        "the top row's left Pane widens: {before:?} → {after:?}"
    );
    assert!(
        (width(&after, 2) - width(&after, 0)).abs() <= 0.5,
        "and the bottom row's moves with it: {after:?}"
    );
}

/// SPEC §4: Solo's card sits on the frame flush right of the nav, under
/// the 46px head row, `BOARD_PAD` short of the 30px bar and flush with the
/// panel's column (§5.6), which runs from the window's top to the bar —
/// the mockups' frame at their 1600×960.
#[gpui::test]
fn the_solo_card_sits_on_the_frame(cx: &mut TestAppContext) {
    use crate::theme::{BOARD_PAD, PANEL_W, STATUS_BAR_H, WIN_CHROME_H};
    let (core, _fake) = cockpit("frame-solo-card", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1600.), px(960.)));
    tick(cx);
    let rects = cx.update(|window, cx| view.read(cx).pane_rects(window));
    let card = rects[0].1;
    assert_eq!((card.x, card.y), (nav::WIDTH, WIN_CHROME_H));
    assert_eq!(card.x + card.w, 1600. - PANEL_W);
    assert_eq!(card.y + card.h, 960. - STATUS_BAR_H - BOARD_PAD);
    assert_eq!((card.x, card.y, card.w, card.h), (264., 46., 1032., 876.));
    let panel = cx.debug_bounds("panel").expect("the panel");
    assert_eq!(
        panel,
        gpui::Bounds::new(
            gpui::point(px(1296.), px(0.)),
            gpui::size(px(PANEL_W), px(960. - STATUS_BAR_H))
        )
    );
    let key = view.read_with(cx, |view, _| view.panes[0].thread().unwrap().get());
    let root = bounds(cx, format!("pane-root-{key}"));
    assert!((root.left() - px(card.x)).abs() < px(1.) && (root.top() - px(card.y)).abs() < px(1.));
    let bar = cx.debug_bounds("bottom-bar").expect("the bar");
    assert_eq!(bar.size.height, px(STATUS_BAR_H));
    assert_eq!(bar.size.width, px(1600.), "full width");
}

/// SPEC §5.9: a 2×2 Group is four equal cards `GRID_GAP` apart on the
/// frame, flush with the panel's column, its seams the gutters' centre lines: a 9px grab band and a
/// `SEAM_LINE_W` line, both centred in the 8px gutter.
#[gpui::test]
fn a_group_is_cards_an_eight_pixel_gutter_apart(cx: &mut TestAppContext) {
    use crate::theme::{BOARD_PAD, GRID_GAP, PANEL_W, SEAM_LINE_W, STATUS_BAR_H, WIN_CHROME_H};
    let (view, _fake, cx, _group) = board_of("frame-group-cards", 4, cx);
    cx.simulate_resize(gpui::size(px(1600.), px(960.)));
    tick(cx);
    let mut rects: Vec<layout::Rect> = cx
        .update(|window, cx| view.read(cx).pane_rects(window))
        .into_iter()
        .map(|(_, rect)| rect)
        .collect();
    rects.sort_by(|a, b| (a.y, a.x).partial_cmp(&(b.y, b.x)).unwrap());
    let near = |a: f32, b: f32| (a - b).abs() < 0.01;
    let [tl, tr, bl, br] = rects[..] else {
        panic!("four cards: {rects:?}")
    };
    assert!(near(tl.x, nav::WIDTH) && near(tl.y, WIN_CHROME_H), "{tl:?}");
    assert!(near(tr.x - (tl.x + tl.w), GRID_GAP), "{tl:?} {tr:?}");
    assert!(near(bl.y - (tl.y + tl.h), GRID_GAP), "{tl:?} {bl:?}");
    // The focused Pane's panel stands right of the board (SPEC §5.6).
    assert!(near(br.x + br.w, 1600. - PANEL_W), "{br:?}");
    assert!(near(br.y + br.h, 960. - STATUS_BAR_H - BOARD_PAD), "{br:?}");
    assert!(
        near(tl.w, tr.w) && near(tl.h, bl.h),
        "equal cards: {rects:?}"
    );
    let gutter = tl.x + tl.w + GRID_GAP / 2.;
    let band = bounds(cx, "board-seam-0-band-0".to_string());
    assert_eq!(band.size.width, px(9.));
    // gpui snaps the half-pixel band to the device pixel.
    assert!((band.center().x - px(gutter)).abs() <= px(0.5), "{band:?}");
    let line = bounds(cx, "board-seam-0".to_string());
    assert_eq!(line.size.width, px(SEAM_LINE_W));
    assert!((line.center().x - px(gutter)).abs() <= px(0.5), "{line:?}");
}

/// SPEC §5.9: a Group Pane's head says `done` only while the finish is
/// unseen — on screen while the window is away — and the seen rule clears
/// it the moment the window comes back.
#[gpui::test]
fn the_seen_rule_clears_a_heads_done(cx: &mut TestAppContext) {
    let (view, fake, cx, _group) = board_of("frame-head-done", 2, cx);
    let key = view.read_with(cx, |view, _| view.panes[1].thread().unwrap().get());
    let word = move |cx: &mut gpui::VisualTestContext| {
        debug_bounds(cx, format!("pane-head-word-{key}")).is_some()
    };
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.deactivate_window();
    fake.streams.borrow()[1]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    assert!(word(cx), "on the board, but the window is away: `done`");
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    tick(cx);
    assert!(!word(cx), "seen on return: an idle head says nothing");
}

/// SPEC §5.9: the quick answers work on a Group Pane's Decision box — the
/// digit on the Pane that holds the keyboard, a press on any Pane's — and
/// a Group Pane's box says no keys under it (Solo's does), so focus moves
/// nothing in it.
#[gpui::test]
fn a_group_panes_decision_box_answers_by_digit_and_press(cx: &mut TestAppContext) {
    let (view, fake, cx, _group) = board_of("frame-box-answers", 2, cx);
    fake.streams.borrow()[1]
        .send(decision("box-digit"))
        .unwrap();
    tick(cx);
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    tick(cx);
    assert!(
        cx.debug_bounds("question-island").is_some(),
        "the box shows"
    );
    assert!(
        cx.debug_bounds("decision-hints").is_none(),
        "a Group Pane's box says no keys"
    );
    view.update(cx, |view, cx| {
        view.focus_pane(1);
        cx.notify();
    });
    tick(cx);
    assert!(cx.debug_bounds("decision-hints").is_none());
    cx.simulate_keystrokes("1");
    tick(cx);
    assert!(
        matches!(
            fake.answered.borrow().last(),
            Some((id, DecisionAnswer::Allow { .. })) if id == "box-digit"
        ),
        "{:?}",
        fake.answered.borrow()
    );

    // A press answers a box on a Pane that does not hold the keyboard.
    fake.streams.borrow()[1]
        .send(decision("box-press"))
        .unwrap();
    tick(cx);
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    tick(cx);
    let (thread, serial) = view.read_with(cx, |view, _| {
        let request = view.shown_request(1).unwrap();
        (view.panes[1].thread().unwrap(), request.handle.serial)
    });
    let deny = bounds(cx, format!("request-deny-{}-{serial}", thread.get()));
    cx.simulate_click(deny.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        matches!(
            fake.answered.borrow().last(),
            Some((id, DecisionAnswer::Deny { .. })) if id == "box-press"
        ),
        "{:?}",
        fake.answered.borrow()
    );
}

/// SPEC §5.9: a turn that failed leaves the failure strip under it, and
/// its `Retry` sends the failed turn's prompt again.
#[gpui::test]
fn retry_re_sends_the_failed_turns_prompt(cx: &mut TestAppContext) {
    let (view, fake, cx, _group) = board_of("frame-retry", 2, cx);
    let thread = view.read_with(cx, |view, _| view.panes[1].thread().unwrap());
    view.update(cx, |view, cx| {
        view.cockpit.send(thread, "build the range sheet".into());
        cx.notify();
    });
    tick(cx);
    fake.streams.borrow()[1]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Error("boom".into()),
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    let sent = |fake: &Fake| {
        fake.sent
            .borrow()
            .iter()
            .filter(|prompt| prompt.contains("build the range sheet"))
            .count()
    };
    assert_eq!(sent(&fake), 1);
    let key = thread.get();
    assert!(
        debug_bounds(cx, format!("failure-strip-{key}")).is_some(),
        "the strip stands under the failed turn"
    );
    let retry = bounds(cx, format!("failure-retry-{key}"));
    cx.simulate_click(retry.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(sent(&fake), 2, "Retry sent the prompt again");
}

/// SPEC §5.4: a transcript shorter than its viewport sits at the bottom,
/// on the Composer, with the empty space above it — in a Group Pane and
/// in Solo alike.
#[gpui::test]
fn a_short_transcript_sits_at_the_bottom(cx: &mut TestAppContext) {
    for grouped in [true, false] {
        let (mut core, fake) = cockpit(
            if grouped {
                "frame-bottom-group"
            } else {
                "frame-bottom-solo"
            },
            2,
        );
        let thread = core.threads()[0];
        core.send(thread, "Explain the board".into());
        if grouped {
            let group = group_all(&mut core);
            core.enter_group(group).unwrap();
        }
        let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1440.), px(900.)));
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: "One regression in the board.".into(),
            })
            .unwrap();
        tick(cx);
        let key = thread.get();
        let body = bounds(cx, format!("pane-body-{key}"));
        let prompt = cx.debug_bounds("transcript-prompt").expect("the prompt");
        let answer = cx.debug_bounds("transcript-answer").expect("the answer");
        let pad = px(crate::theme::body_pad_b(!grouped));
        assert!(
            (body.bottom() - pad - answer.bottom()).abs() <= px(2.),
            "grouped {grouped}: the last row sits on the body's foot: {answer:?} / {body:?}"
        );
        assert!(
            prompt.top() - body.top() > px(100.),
            "grouped {grouped}: the empty space is above the rows: {prompt:?} / {body:?}"
        );
    }
}
