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
/// own edges — the head at the Pane's left and top, its title 23.4px (a
/// cell of padding and the 2-cell dot column) in — and every Pane root is
/// the `PANE_GROUP` hover group.
#[gpui::test]
fn a_panes_content_starts_at_its_own_edges(cx: &mut TestAppContext) {
    let (view, _fake, cx, _group) = board_of("frame-edge-overlay", 4, cx);
    let rects = cx.update(|window, cx| view.read(cx).pane_rects(window));
    assert_eq!(rects.len(), 4);
    for (index, rect) in rects {
        let key = view.read_with(cx, |view, _| view.panes[index].thread().unwrap().get());
        let head = bounds(cx, format!("pane-head-{key}"));
        assert!(
            (head.left() - px(rect.x)).abs() <= px(0.5)
                && (head.top() - px(rect.y)).abs() <= px(0.5),
            "the head starts at the Pane's corner: {head:?} / {rect:?}"
        );
        assert_eq!(head.size.height, px(crate::theme::PANE_HEAD_H));
        let title = bounds(cx, format!("pane-head-title-{key}"));
        assert!(
            (title.left() - px(rect.x + 3.0 * crate::theme::CH)).abs() <= px(0.5),
            "the title 23.4px in: {title:?} / {rect:?}"
        );
        let root = bounds(cx, format!("pane-root-{key}"));
        assert!((root.left() - px(rect.x)).abs() <= px(0.5));
    }
    assert_eq!(pane::PANE_GROUP, "pane");
}

/// F-2: a waiting Pane wears the one attention edge whether it holds focus
/// or not — no focus ring inside it — and a calm focused Pane the accent.
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

/// F-7: a quick answer is 22px outside (a line and its two borders) and as
/// wide as its words and a cell of padding each side — `1 allow` is 72px —
/// its first box two cells in from the tile's edge, its row half a row
/// above the tile's foot.
#[gpui::test]
fn a_quick_answer_box_is_one_line_and_its_borders(cx: &mut TestAppContext) {
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
    let width = 7.0 * crate::theme::CH + 2.0 * crate::theme::QUICK_ANSWER_PAD_X + 2.0;
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
/// the parked tile — and ⏎ on it wakes it on a fresh Session, in place.
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
/// left edge is the column's width this frame and the seam, and the Pane
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
                let column = view.nav_column_now(cx);
                let width = view.pane_rects(window)[0].1.w;
                (left, column, width)
            });
            assert!(
                (left - (column + crate::theme::CHROME_SEAM_W)).abs() < 0.01,
                "{count}: the board starts at the column + 1: {left} / {column}"
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

/// F-9: the bottom bar always shows `1 solo` and a tab per Group, open or
/// not; a press on a Group's tab enters it, and on `1 solo` with no loose
/// Thread it shows the empty board.
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
        "no loose Thread: `1 solo` is the empty board"
    );
}

/// F-14: a reader opens beside the Solo Pane at a line — half and half —
/// and closing it gives the Pane its board back.
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
    assert!(
        (half - (whole - crate::theme::BOARD_SEAM) / 2.0).abs() <= 1.0,
        "half and half: {half} of {whole}"
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
