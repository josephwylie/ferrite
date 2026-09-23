//! WP-C's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// Solo has no head (C2): the titlebar carries the Thread on one line — the
/// checkout, the tasks meter and the PR/CI chip ride it left to right, inside
/// the band, and nothing spills past the window.
#[gpui::test]
fn the_solo_thread_rides_the_titlebar_in_one_row(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("head-one-row", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    view.update(cx, |view, cx| {
        view.facts
            .set_branches(vec![(thread, Some(branch_status_with_checks()))]);
        cx.notify();
    });
    for (id, status) in [
        ("1", ferrite_core::progress::StepStatus::Completed),
        ("2", ferrite_core::progress::StepStatus::Pending),
    ] {
        fake.streams.borrow()[0]
            .send(SessionEvent::Progress {
                event: ferrite_core::progress::ProgressEvent::Task {
                    id: id.into(),
                    subject: format!("step {id}"),
                    status: Some(status),
                    deleted: false,
                },
            })
            .unwrap();
    }
    tick(cx);
    assert!(cx.debug_bounds("pane-head-1").is_none(), "Solo has no head");
    let band = cx
        .debug_bounds("titlebar-thread")
        .expect("the titlebar Thread");
    assert!(band.bottom() <= px(crate::theme::WIN_CHROME_H));
    let branch = cx.debug_bounds("project-branch-0").expect("the checkout");
    let meter = cx.debug_bounds("tasks-meter-1").expect("the tasks meter");
    let ci = cx.debug_bounds("ci-mark-1").expect("the PR/CI chip");
    let window = cx.update(|window, _| window.viewport_size().width);
    for (name, part) in [("checkout", branch), ("meter", meter), ("ci", ci)] {
        assert!(
            part.top() >= px(0.) && part.bottom() <= px(crate::theme::WIN_CHROME_H),
            "the {name} rides the titlebar row: {part:?} / {band:?}"
        );
        assert!(
            part.right() <= window,
            "the {name} stays inside the window: {part:?}"
        );
    }
    assert!(branch.right() < meter.left() && meter.right() < ci.left());
}

/// Closing the last Pane leaves a board that says how to start, and the
/// first new Thread takes its place.
#[gpui::test]
fn the_empty_cockpit_says_how_to_start(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("empty-board", 1);
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-w", CloseThread, None),
            KeyBinding::new("cmd-n", NewThread, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    assert!(cx.debug_bounds("empty-board").is_none());
    cx.simulate_keystrokes("cmd-w");
    tick(cx);
    view.read_with(cx, |view, _| assert!(view.panes.is_empty()));
    assert!(cx.debug_bounds("empty-board").is_some());
    cx.simulate_keystrokes("cmd-n");
    tick(cx);
    assert!(cx.debug_bounds("empty-board").is_none());
}

/// The empty board's keys come from the platform's own key table.
#[test]
fn the_empty_board_spells_keys_from_the_key_table() {
    let primary = match crate::keymap::PLATFORM {
        crate::keymap::Platform::Mac => "cmd",
        crate::keymap::Platform::Windows => "ctrl",
    };
    assert_eq!(
        CockpitView::key_label("cockpit::NewThread").as_deref(),
        Some(format!("{primary} N").as_str())
    );
    assert_eq!(
        CockpitView::key_label("cockpit::NewWorktreeThread").as_deref(),
        Some(format!("{primary} shift N").as_str())
    );
}

// ---------------------------------------------------------------- P4 board

/// A board of `count` Threads in one Group, entered, at the app size.
fn board<'a>(
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
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    (view, fake, cx, group)
}

/// One Level per board (rule 2.3.5): fewer Panes never draw at a lower tier
/// than more in the same window — 4 on the default grid are a 2×2 at L1 —
/// and a dragged 5-over-7 tree keeps every Pane at one Level.
#[gpui::test]
fn fewer_panes_never_draw_a_lower_level_and_a_board_never_mixes_tiers(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("board-levels", 13);
    let threads = core.threads().to_vec();
    let four = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    for thread in &threads[2..4] {
        core.apply_group(GroupChange::Join {
            thread: *thread,
            group: four,
            index: None,
        })
        .unwrap();
    }
    let nine = core
        .apply_group(GroupChange::Create {
            first: threads[4],
            second: threads[5],
        })
        .unwrap()
        .group
        .unwrap();
    for thread in &threads[6..13] {
        core.apply_group(GroupChange::Join {
            thread: *thread,
            group: nine,
            index: None,
        })
        .unwrap();
    }
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    let level = |group, cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| view.enter_group(group, cx));
        tick(cx);
        cx.update(|window, cx| {
            let view = view.read(cx);
            let levels: Vec<_> = view
                .pane_rects(window)
                .into_iter()
                .map(|(index, _)| view.level_of(index, window))
                .collect();
            assert!(
                levels.windows(2).all(|pair| pair[0] == pair[1]),
                "one Level per board: {levels:?}"
            );
            view.board_level(window)
        })
    };
    let at_four = level(four, cx);
    let at_nine = level(nine, cx);
    assert_eq!(
        at_four,
        Level::Transcript,
        "4 on the default grid are 2×2 at L1"
    );
    assert!(at_four >= at_nine, "{at_four:?} < {at_nine:?}");
    let rects = cx.update(|window, cx| view.read(cx).pane_rects(window));
    let columns: std::collections::BTreeSet<i32> = rects
        .iter()
        .map(|(_, rect)| rect.x.round() as i32)
        .collect();
    assert_eq!(columns.len(), 3, "9 are 3×3 with aligned seams: {rects:?}");
}

/// The operator's own tree survives a window resize — only a reset puts
/// the Group back on the default grid — and every Pane of a dragged 5-over-7
/// board still draws at the one Level its smallest cell allows.
#[gpui::test]
fn a_dragged_tree_survives_a_resize_and_draws_one_level(cx: &mut TestAppContext) {
    use ferrite_core::layout::{Axis, Node};
    let (view, _fake, cx, group) = board("board-dragged", 12, cx);
    let threads: Vec<ThreadId> = view.read_with(cx, |view, _| {
        view.panes.iter().filter_map(|pane| pane.thread()).collect()
    });
    let chain = |ids: &[ThreadId]| {
        let mut nodes: Vec<Node> = ids.iter().map(|id| Node::Leaf(*id)).collect();
        let mut node = nodes.pop().unwrap();
        let mut count = 1;
        while let Some(head) = nodes.pop() {
            count += 1;
            node = Node::Split {
                axis: Axis::Row,
                ratio: 1.0 / count as f32,
                first: Box::new(head),
                second: Box::new(node),
            };
        }
        node
    };
    let dragged = Tree {
        root: Some(Node::Split {
            axis: Axis::Column,
            ratio: 0.5,
            first: Box::new(chain(&threads[..5])),
            second: Box::new(chain(&threads[5..])),
        }),
    };
    view.update(cx, |view, cx| {
        view.cockpit
            .set_group_layout(group, dragged.clone())
            .unwrap();
        cx.notify();
    });
    tick(cx);
    for (width, height) in [(1440., 900.), (1100., 760.), (1800., 1100.)] {
        cx.simulate_resize(gpui::size(px(width), px(height)));
        tick(cx);
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.group_layout(group),
                Some(dragged.clone()),
                "the drag is kept"
            );
        });
        cx.update(|window, cx| {
            let view = view.read(cx);
            let rects = view.pane_rects(window);
            let mut rows = std::collections::BTreeMap::<i32, usize>::new();
            for (_, rect) in &rects {
                *rows.entry(rect.y.round() as i32).or_default() += 1;
            }
            assert_eq!(
                rows.values().copied().collect::<Vec<_>>(),
                [5, 7],
                "still five over seven: {rects:?}"
            );
            let level = view.board_level(window);
            for (index, _) in rects {
                assert_eq!(view.level_of(index, window), level);
            }
        });
    }
    // A reset forgets the drag: the default grid again.
    view.update(cx, |view, cx| {
        view.cockpit.reset_group_layout(group).unwrap();
        cx.notify();
    });
    tick(cx);
    let rects = cx.update(|window, cx| view.read(cx).pane_rects(window));
    let widths: std::collections::BTreeSet<i32> = rects
        .iter()
        .map(|(_, rect)| rect.w.round() as i32)
        .collect();
    assert_eq!(
        widths.len(),
        1,
        "the default grid has equal cells: {rects:?}"
    );
}

/// Stepping focus across a 3×3 board moves nothing: every cell's body keeps
/// its bounds (the grid Composer is one fixed line, focused or not), and
/// only the focused cell's line is live — one caret on the board.
#[gpui::test]
fn stepping_focus_across_nine_cells_moves_no_body(cx: &mut TestAppContext) {
    let (view, _fake, cx, _group) = board("board-cmd-bracket", 9, cx);
    let keys: Vec<(u64, SharedString)> = view.read_with(cx, |view, _| {
        view.panes
            .iter()
            .map(|pane| (pane.thread().unwrap().get(), pane.text_namespace()))
            .collect()
    });
    let bodies = |cx: &mut gpui::VisualTestContext| -> Vec<gpui::Bounds<gpui::Pixels>> {
        keys.iter()
            .map(|(key, _)| bounds(cx, format!("pane-body-{key}")))
            .collect()
    };
    let live = |cx: &mut gpui::VisualTestContext| -> usize {
        keys.iter()
            .filter(|(_, namespace)| {
                debug_bounds(cx, format!("composer-live-{namespace}")).is_some()
            })
            .count()
    };
    let before = bodies(cx);
    assert_eq!(live(cx), 1, "one live line on the board");
    for _ in 0..9 {
        cx.simulate_keystrokes("cmd-]");
        tick(cx);
        assert_eq!(bodies(cx), before, "focus moved a body");
        assert_eq!(live(cx), 1, "exactly one caret-bearing line");
    }
}

/// Nothing paints under a Group head: the body begins at the head's rule
/// and every transcript row sits below it.
#[gpui::test]
fn nothing_paints_under_a_group_head(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("board-head-clip", 2);
    let thread = core.threads()[0];
    core.send(thread, "Explain the board".into());
    let group = group_all(&mut core);
    core.enter_group(group).unwrap();
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "One regression in the board.\n\nFixed it.".into(),
        })
        .unwrap();
    tick(cx);
    let key = thread.get();
    let head = bounds(cx, format!("pane-head-{key}"));
    let body = bounds(cx, format!("pane-body-{key}"));
    assert_eq!(head.size.height, px(crate::theme::PANE_HEAD_H));
    assert!(body.top() >= head.bottom(), "{body:?} under {head:?}");
    for row in ["transcript-prompt", "transcript-answer"] {
        let row = cx.debug_bounds(row).expect("the row paints");
        assert!(
            row.top() >= head.bottom(),
            "a row {row:?} intersects the head {head:?}"
        );
    }
}

/// The Composer's `❯` sits on the transcript's mark column: at L1 over the
/// prompt row's `❯`, at L2 over the tail's (±1px) — whose text starts at C1,
/// one gutter past it, in the prose face's small size.
#[gpui::test]
fn the_composer_mark_shares_the_transcript_mark_axis_at_l1_and_l2(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("board-mark-axis", 1);
    let thread = core.threads()[0];
    core.send(thread, "Line the marks up".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    super::hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "They line up.".into(),
        })
        .unwrap();
    tick(cx);
    let mark = cx.debug_bounds("composer-mark").unwrap();
    let prompt = cx.debug_bounds("transcript-prompt").unwrap();
    assert!(
        (mark.left() - prompt.left()).abs() <= px(1.),
        "L1: {mark:?} / {prompt:?}"
    );

    cx.simulate_resize(gpui::size(px(560.), px(700.)));
    tick(cx);
    assert_eq!(
        cx.update(|window, cx| view.read(cx).level_now(window)),
        Level::Instruments
    );
    let (namespace, prompt_id, answer_id) = view.read_with(cx, |view, _| {
        let blocks = view.cockpit.thread(thread).unwrap().transcript().blocks();
        (view.panes[0].text_namespace(), blocks[0].id, blocks[1].id)
    });
    let mark = cx.debug_bounds("composer-mark").unwrap();
    let row = bounds(cx, format!("l2-tail-row-{namespace}-{prompt_id:?}"));
    assert!(
        (mark.left() - row.left()).abs() <= px(1.),
        "L2: {mark:?} / {row:?}"
    );
    let text = bounds(cx, format!("l2-tail-text-{namespace}-{answer_id:?}"));
    assert!(
        (text.left() - (row.left() + px(crate::theme::GUTTER_W))).abs() <= px(0.5),
        "tail text at C1: {text:?} / {row:?}"
    );
    let rect = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1);
    assert!(
        (row.left() - px(rect.x + 1. + crate::theme::PANE_PAD_X)).abs() <= px(0.5),
        "the tail's marks sit at PANE_PAD_X: {row:?} / {rect:?}"
    );
    // Whole lines only, never half a line under the rule.
    let prose = bounds(cx, format!("l2-tail-row-{namespace}-{answer_id:?}"));
    assert_eq!(
        (f32::from(prose.size.height) / crate::theme::LH_PROSE_SM).fract(),
        0.,
        "{prose:?}"
    );
}

/// The expand and approve keys still reach a board cell once the head's
/// chips are gone: `y` answers the focused cell's approval and the expand
/// key fills the board with it.
#[gpui::test]
fn the_expand_and_approve_keys_still_work_without_head_chips(cx: &mut TestAppContext) {
    let (view, fake, cx, _group) = board("board-keys", 4, cx);
    fake.streams.borrow()[0]
        .send(decision("board-perm"))
        .unwrap();
    tick(cx);
    let key = view.read_with(cx, |view, _| view.panes[0].thread().unwrap().get());
    assert!(debug_bounds(cx, format!("head-slot-{key}")).is_some());
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    tick(cx);
    cx.simulate_keystrokes("cmd-f");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.roster().fullscreen(),
            Some(view.panes[0].identity),
            "the expand key fills the board"
        );
    });
    cx.simulate_keystrokes("y");
    tick(cx);
    assert_eq!(fake.answered.borrow().len(), 1, "y answered the approval");
}

/// Unread breathes on the head dot, on the shared pulse clock: N unread
/// Panes cost one ~30fps tick (never a display frame each), and under
/// reduced motion they hold still and lease nothing.
#[gpui::test]
fn unread_breathing_rides_the_pulse_clock_and_rests_under_reduced_motion(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let (view, fake, cx, _group) = board("board-unread", 3, cx);
    for stream in 1..3 {
        for event in [
            SessionEvent::TextDelta {
                text: "Done here.".into(),
            },
            SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            },
        ] {
            fake.streams.borrow()[stream].send(event).unwrap();
        }
    }
    tick(cx);
    let unread = view.read_with(cx, |view, _| {
        view.panes
            .iter()
            .filter_map(|pane| pane.thread())
            .filter(|thread| view.cockpit.notifications().attention(*thread))
            .count()
    });
    assert!(unread >= 2, "two Threads finished out of sight");
    assert!(cx.debug_bounds("breathing-dot").is_some());
    // The completion toasts arrive on their own springs; let them rest.
    let mut settled = false;
    for _ in 0..64 {
        let frames = cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        if frames == 0 {
            settled = true;
            break;
        }
        cx.executor().advance_clock(Duration::from_millis(16));
    }
    assert!(settled, "the window stops asking for display frames");
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    assert!(
        !cx.update(|_, cx| crate::motion::pulse_parked(cx)),
        "the breath leases the shared clock"
    );
    assert!(
        crate::theme::MOTION_PULSE_TICK_MS >= 33,
        "the clock ticks at most 1000/33 times a second"
    );
    assert_eq!(
        cx.update(|window, cx| window.simulate_next_frame(cx)),
        0,
        "breathing never asks the display for every frame"
    );

    cx.update(|_, cx| cx.set_reduce_motion(true));
    tick(cx);
    cx.executor().advance_clock(Duration::from_millis(
        crate::theme::MOTION_PULSE_LEASE_MS + 2 * crate::theme::MOTION_PULSE_TICK_MS,
    ));
    cx.run_until_parked();
    assert!(
        cx.update(|_, cx| crate::motion::pulse_parked(cx)),
        "reduced motion holds the breath still and leases nothing"
    );
    assert_eq!(cx.update(|window, cx| window.simulate_next_frame(cx)), 0);
}
