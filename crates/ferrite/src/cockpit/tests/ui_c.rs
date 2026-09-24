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

/// The empty board's keys come from the platform's own key table and read
/// as glyphs: `⌘N`, `⌘⇧N`, `⌘O` (rule 2.11.4).
#[test]
fn the_empty_board_spells_keys_from_the_key_table() {
    let (primary, glyph) = match crate::keymap::PLATFORM {
        crate::keymap::Platform::Mac => ("cmd", "\u{2318}"),
        crate::keymap::Platform::Windows => ("ctrl", "\u{2303}"),
    };
    let spelled = |action: &str| {
        CockpitView::key_label(action).map(|keys| crate::components::key_glyphs(&keys))
    };
    assert_eq!(
        CockpitView::key_label("cockpit::NewThread").as_deref(),
        Some(format!("{primary}-N").as_str())
    );
    assert_eq!(spelled("cockpit::NewThread"), Some(format!("{glyph}N")));
    assert_eq!(
        CockpitView::key_label("cockpit::NewWorktreeThread").as_deref(),
        Some(format!("{primary}-shift-N").as_str())
    );
    assert_eq!(
        spelled("cockpit::NewWorktreeThread"),
        Some(format!("{glyph}\u{21e7}N"))
    );
    assert_eq!(spelled("cockpit::ReopenThread"), Some(format!("{glyph}O")));
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

/// C6: with two cells waiting on a board, only the cell `y` would answer
/// shows the keys, and `y` answers exactly that cell.
#[gpui::test]
fn only_the_answer_target_cell_shows_its_keys_and_y_answers_it(cx: &mut TestAppContext) {
    let (view, fake, cx, _group) = board("board-answer-target", 12, cx);
    assert_eq!(
        cx.update(|window, cx| view.read(cx).level_now(window)),
        Level::Instruments
    );
    fake.streams.borrow()[1]
        .send(decision("first-wait"))
        .unwrap();
    fake.streams.borrow()[2]
        .send(decision("second-wait"))
        .unwrap();
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    tick(cx);
    let (target, keyed) = view.update_in(cx, |view, window, cx| {
        let target = view.key_target().expect("a Thread waits");
        let keyed: Vec<_> = (0..view.panes.len())
            .filter(|index| {
                view.decide_keycaps(*index, Level::Instruments, window, cx)
                    .is_some()
            })
            .filter_map(|index| view.panes[index].thread())
            .collect();
        (target, keyed)
    });
    assert_eq!(keyed, [target], "the keys show on the target's cell alone");
    let target_rect = view.update_in(cx, |view, window, _| {
        let index = view.pane_for(target).unwrap();
        view.pane_rects(window)
            .into_iter()
            .find(|(at, _)| *at == index)
            .unwrap()
            .1
    });
    let deny = cx.debug_bounds("decision-deny").expect("the target's n");
    assert!(
        deny.left() >= px(target_rect.x)
            && deny.right() <= px(target_rect.x + target_rect.w)
            && deny.top() >= px(target_rect.y)
            && deny.bottom() <= px(target_rect.y + target_rect.h),
        "{deny:?} sits in the target cell {target_rect:?}"
    );
    let expected = view.read_with(cx, |view, _| {
        view.cockpit
            .thread(target)
            .unwrap()
            .pending()
            .unwrap()
            .id
            .clone()
    });
    // ⌘D lands on the answer target; its keys stay on that one cell.
    cx.simulate_keystrokes("cmd-d");
    tick(cx);
    let keyed = view.update_in(cx, |view, window, cx| {
        (0..view.panes.len())
            .filter(|index| {
                view.decide_keycaps(*index, Level::Instruments, window, cx)
                    .is_some()
            })
            .filter_map(|index| view.panes[index].thread())
            .collect::<Vec<_>>()
    });
    assert_eq!(keyed, [target], "focus on the target keeps one keyed cell");
    cx.simulate_keystrokes("y");
    tick(cx);
    assert!(
        matches!(
            fake.answered.borrow().last(),
            Some((id, DecisionAnswer::Allow { .. })) if *id == expected
        ),
        "y answered the cell that showed it: {:?}",
        fake.answered.borrow()
    );
    assert_eq!(fake.answered.borrow().len(), 1);
}

/// Rule 2.8.7: at the group9 geometry a waiting L1 cell's Decision claims
/// its natural height — `n Deny` sits whole inside the block.
#[gpui::test]
fn a_group_cell_decision_never_clips_its_deny_row(cx: &mut TestAppContext) {
    let (view, fake, cx, _group) = board("board-deny-whole", 9, cx);
    assert_eq!(
        cx.update(|window, cx| view.read(cx).level_now(window)),
        Level::Transcript
    );
    fake.streams.borrow()[1]
        .send(decision("whole-deny"))
        .unwrap();
    tick(cx);
    let island = cx.debug_bounds("question-island").expect("the Decision");
    let deny = cx.debug_bounds("decision-deny").expect("its deny row");
    assert!(
        island.contains(&deny.origin) && island.contains(&deny.bottom_right()),
        "{deny:?} inside {island:?}"
    );
    assert!(deny.size.height >= px(crate::theme::MENU_ROW_H));
}

/// Rule 2.8.6: the digit one past a question's options arms its one answer
/// line — typing lands there, not in the Composer — and Send carries it.
#[gpui::test]
fn the_next_digit_arms_the_own_answer_line(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("own-answer-digit", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(900.)));
    fake.streams.borrow()[0].send(question("armed")).unwrap();
    tick(cx);
    view.update_in(cx, |view, window, cx| {
        let focus = view.panes[0].composer.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    });
    tick(cx);
    cx.simulate_keystrokes("3");
    tick(cx);
    cx.simulate_input("mine");
    tick(cx);
    assert_eq!(composer_text(&view, cx), "", "the Composer keeps its line");
    let (thread, serial) = view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        let serial = view
            .cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .pending_decisions()[0]
            .handle
            .serial;
        (thread, serial)
    });
    let send = bounds(cx, format!("request-submit-{}-{serial}", thread.get()));
    cx.simulate_click(send.center(), gpui::Modifiers::none());
    tick(cx);
    let answered = fake.answered.borrow();
    let Some((_, DecisionAnswer::Questions { answers })) = answered.last() else {
        panic!("the question was answered: {answered:?}")
    };
    assert!(answers[0].picks.is_empty());
    assert_eq!(answers[0].other.as_deref(), Some("mine"));
}

// ------------------------------------------------------ operator rulings

/// Discover `count` subagents under a Thread's Main.
fn subagents(fake: &Fake, stream: usize, count: usize) {
    for n in 0..count {
        let key = ferrite_core::activity::AgentKey::new(
            Provider::Claude,
            "title-first",
            &format!("child-{stream}-{n}"),
        );
        let mut info = ferrite_core::activity::AgentInfo::new(key);
        info.parent = Some(ferrite_core::activity::Subject::Main);
        fake.streams.borrow()[stream]
            .send(SessionEvent::Activity(
                ferrite_core::activity::ActivityEvent::Discovered(info),
            ))
            .unwrap();
    }
}

/// The nav title comes first. At the nav's own width, a Group member on a
/// long worktree branch that runs five subagents keeps its title at its
/// floor or whole, the branch gives way first, and the count gives way
/// before the title drops under its floor. A short title keeps its own
/// width, and then the branch and the count both fit beside it. The word
/// or age and the provider mark are never squeezed.
#[gpui::test]
fn a_nav_title_keeps_its_floor_before_the_branch_and_the_count(cx: &mut TestAppContext) {
    use crate::theme::{NAV_TITLE_FLOOR, PROVIDER_MARK};
    let (view, fake, cx, _group) = board("nav-title-first", 2, cx);
    subagents(&fake, 0, 5);
    subagents(&fake, 1, 5);
    tick(cx);
    let threads = view.read_with(cx, |view, _| {
        [
            view.panes[0].thread().unwrap(),
            view.panes[1].thread().unwrap(),
        ]
    });
    let long_branch = "worktree-pay-api-migration-cleanup-and-retry";
    view.update(cx, |view, cx| {
        for (thread, title) in threads.iter().zip([
            "Switch the payment worker to the new settlement queue",
            "Fix",
        ]) {
            view.cockpit.rename_thread(*thread, title).unwrap();
            view.facts.renamed(&view.cockpit, *thread);
        }
        view.facts.set_branches(
            threads
                .iter()
                .map(|thread| {
                    (
                        *thread,
                        Some(ferrite_core::workspace::BranchStatus {
                            branch: Some(long_branch.into()),
                            ..Default::default()
                        }),
                    )
                })
                .collect(),
        );
        cx.notify();
    });
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.nav_width()),
        crate::nav::WIDTH,
        "the nav at its default width"
    );
    let on_line = |cx: &mut gpui::VisualTestContext, id: String, fit: gpui::Bounds<Pixels>| {
        debug_bounds(cx, id)
            .filter(|bounds| bounds.top() < fit.bottom() && bounds.size.width > px(0.))
    };
    for (index, thread) in threads.iter().enumerate() {
        let id = thread.get();
        let row = view.read_with(cx, |view, _| view.thread_row(*thread));
        assert_eq!(row.subagents, 5);
        assert_eq!(row.branch.as_deref(), Some(long_branch));
        let fit = bounds(cx, format!("nav-title-fit-{id}"));
        let title = bounds(cx, format!("nav-title-{id}"));
        let branch = on_line(cx, format!("nav-branch-{id}"), fit);
        let count = on_line(cx, format!("nav-subagents-{id}"), fit);
        let mark = bounds(cx, format!("nav-mark-{id}"));
        let tail = bounds(cx, format!("nav-since-{id}"));
        let whole_row = bounds(cx, format!("nav-thread-{id}"));
        assert!(title.left() >= fit.left() && title.right() <= fit.right() + px(0.5));
        assert!(
            tail.left() >= fit.right() && mark.left() >= tail.right(),
            "the tail and the mark sit after the title's line"
        );
        assert!(mark.right() <= whole_row.right(), "the mark is whole");
        assert_eq!(mark.size.width, px(PROVIDER_MARK), "the mark never shrinks");
        assert!(tail.size.width >= px(crate::theme::NAV_TAIL_MIN_W));
        if index == 0 {
            assert!(
                title.size.width >= px(NAV_TITLE_FLOOR),
                "a long title keeps its floor: {title:?}"
            );
            if let Some(branch) = branch {
                assert!(branch.right() <= fit.right(), "the branch truncates");
            }
            if let Some(count) = count {
                assert!(count.left() >= title.right(), "{count:?} after {title:?}");
            }
        } else {
            assert!(
                title.size.width < px(NAV_TITLE_FLOOR),
                "a short title keeps its own width: {title:?}"
            );
            let branch = branch.expect("beside a short title the branch fits");
            assert!(branch.left() >= title.right() && branch.right() <= fit.right());
            assert!(
                branch.size.width > px(crate::theme::NAV_BRANCH_MIN_W),
                "the branch takes what the title leaves: {branch:?}"
            );
            let count = count.expect("beside a short title the count fits");
            assert!(count.left() >= branch.right() && count.right() <= fit.right());
        }
    }
}

/// Fix 2: a Thread whose turn ended in an error reads `failed` in the nav,
/// and its dot is `BLOCKED` there, in the rail and in the titlebar — never
/// the idle grey beside a red word.
#[gpui::test]
fn a_failed_threads_dot_is_blocked_wherever_its_word_says_failed(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("failed-dot", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    for event in [
        SessionEvent::TextDelta {
            text: "Running the pump test.".into(),
        },
        SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Error("API Error: 529 overloaded".into()),
            cost_usd: None,
        },
    ] {
        fake.streams.borrow()[0].send(event).unwrap();
    }
    tick(cx);
    view.read_with(cx, |view, _| {
        let thread = view.cockpit.threads()[0];
        let row = view.thread_row(thread);
        assert_eq!(row.tail, nav::NavTail::Failed, "the tail says failed");
        assert_eq!(
            thread_status(row.status.wall(), row.unread).ink,
            crate::theme::BLOCKED,
            "and the nav and rail dot say it too"
        );
        let (face, word) = pane::thread_face(view.cockpit.thread(thread).unwrap(), None, false);
        assert_eq!(word, Some(pane::HeadSlot::Failed));
        assert_eq!(face.ink, crate::theme::BLOCKED, "the titlebar's dot agrees");
    });
}

/// Fix 3: the Solo titlebar says `needs you` once. The Thread's own state
/// word (`needs you · question`, the ⌘D door) stands, and the band's
/// `· N need you` count does not repeat it; on a board, where no Thread
/// rides the band, the count is still there.
#[gpui::test]
fn the_solo_titlebar_says_needs_you_once(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("needs-you-once", 1);
    bind_production_keys(cx);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    fake.streams.borrow()[0].send(question("once")).unwrap();
    tick(cx);
    assert!(
        cx.debug_bounds("titlebar-needs-you").is_some(),
        "the Thread's state word says needs you"
    );
    assert!(
        cx.debug_bounds("titlebar-need-you").is_none(),
        "and the count does not say it again"
    );
}

#[gpui::test]
fn a_board_titlebar_keeps_its_need_you_count(cx: &mut TestAppContext) {
    let (_view, fake, cx, _group) = board("need-you-count", 2, cx);
    fake.streams.borrow()[1].send(decision("count")).unwrap();
    tick(cx);
    assert!(cx.debug_bounds("titlebar-need-you").is_some());
    assert!(cx.debug_bounds("titlebar-needs-you").is_none());
}

/// Q6 (the operator's ruling): on a board a waiting cell's edge is
/// `ATTENTION_EDGE`, ochre at 35%, and the single answer-target cell alone
/// wears full `ATTENTION`; in Solo no state recolours the edge — the docked
/// Decision carries it.
#[gpui::test]
fn one_answer_target_wears_full_ink_and_solo_wears_no_state_edge(cx: &mut TestAppContext) {
    assert_eq!(
        crate::theme::ATTENTION_EDGE,
        (crate::theme::ATTENTION << 8) | 0x59,
        "the waiting edge is ATTENTION itself"
    );
    assert_eq!((0.35_f32 * 255.).round() as u32, 0x59, "at 35% alpha");
    let (view, fake, cx, _group) = board("edge-answer-target", 4, cx);
    fake.streams.borrow()[1].send(decision("edge-1")).unwrap();
    fake.streams.borrow()[2].send(decision("edge-2")).unwrap();
    tick(cx);
    let (target, waiting) = view.read_with(cx, |view, _| {
        (
            view.key_target().expect("a Thread waits"),
            [
                view.panes[1].thread().unwrap(),
                view.panes[2].thread().unwrap(),
            ],
        )
    });
    for thread in waiting {
        let full = cx
            .debug_bounds(format!("pane-answer-edge-{}", thread.get()).leak())
            .is_some();
        let alpha = cx
            .debug_bounds(format!("pane-waiting-edge-{}", thread.get()).leak())
            .is_some();
        assert_eq!(full, thread == target, "only the answer target is full ink");
        assert_eq!(alpha, thread != target, "every other waiting cell is alpha");
    }

    let (core, fake) = cockpit("edge-solo", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    fake.streams.borrow()[0]
        .send(decision("edge-solo"))
        .unwrap();
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    for edge in ["pane-answer-edge", "pane-waiting-edge"] {
        assert!(
            cx.debug_bounds(format!("{edge}-{}", thread.get()).leak())
                .is_none(),
            "Solo draws no {edge}"
        );
    }
}

/// Fix 5: the attachment shelf belongs to the live Composer. A file
/// attached in cell A stays in A's draft while B holds focus; A's flat
/// line draws no chip and says `1 attachment` after its `❯` instead. Focus
/// back on A brings the chip back, and the file still sends.
#[gpui::test]
fn an_unfocused_cell_counts_its_attachments_on_the_flat_line(cx: &mut TestAppContext) {
    assert_eq!(pane::flat_facts_text(1, 0), "1 attachment");
    assert_eq!(pane::flat_facts_text(3, 0), "3 attachments");
    assert_eq!(pane::flat_facts_text(0, 2), "2 queued");
    assert_eq!(pane::flat_facts_text(1, 2), "1 attachment \u{b7} 2 queued");
    assert_eq!(pane::flat_facts_text(0, 0), "");

    let (view, fake, cx, _group) = board("flat-attachments", 4, cx);
    assert_eq!(
        cx.update(|window, cx| view.read(cx).level_now(window)),
        Level::Transcript
    );
    let file = std::path::PathBuf::from("/tmp/CleanShot 2026-09-24 at 10.12.03.png");
    let focus_composer =
        |view: &Entity<CockpitView>, index: usize, cx: &mut gpui::VisualTestContext| {
            view.update_in(cx, |view, window, cx| {
                view.focus_pane(index);
                let focus = view.panes[index].composer.read(cx).focus_handle(cx);
                window.focus(&focus, cx);
                cx.notify();
            });
            tick(cx);
        };
    focus_composer(&view, 0, cx);
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.add_files(std::slice::from_ref(&file), cx)
        });
    });
    tick(cx);
    assert!(cx.debug_bounds("pending-attachment-tray").is_some());
    assert!(cx.debug_bounds("composer-flat-facts").is_none());

    focus_composer(&view, 1, cx);
    let cell = cx.update(|window, cx| {
        view.read(cx)
            .pane_rects(window)
            .into_iter()
            .find(|(at, _)| *at == 0)
            .unwrap()
            .1
    });
    assert!(
        cx.debug_bounds("pending-attachment-tray").is_none(),
        "no chip shelf over an unfocused cell's flat line"
    );
    let facts = cx
        .debug_bounds("composer-flat-facts")
        .expect("the flat line counts the attachment");
    assert!(
        facts.left() >= px(cell.x)
            && facts.right() <= px(cell.x + cell.w)
            && facts.bottom() <= px(cell.y + cell.h),
        "{facts:?} rides cell A's flat line {cell:?}"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).file_count()),
        1,
        "focus moves never touch the draft"
    );

    focus_composer(&view, 0, cx);
    assert!(
        cx.debug_bounds("pending-attachment-tray").is_some(),
        "focus brings the chip back"
    );
    assert!(cx.debug_bounds("composer-flat-facts").is_none());
    cx.simulate_input("look at this");
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    let sent = fake.sent.borrow().clone();
    assert!(
        sent.iter().any(
            |prompt| prompt.contains("look at this") && prompt.contains("CleanShot 2026-09-24")
        ),
        "the attachment still sends: {sent:?}"
    );
}
