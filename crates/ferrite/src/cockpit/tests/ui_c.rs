//! WP-C's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// Solo has no head (C2): the titlebar carries the Thread on one line —
/// `project / title · state · branch` (F-10) — inside the band; the plan's
/// meter and the PR/CI chip are palette commands now (F-11), not chrome.
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
        .debug_bounds("titlebar-location")
        .expect("the titlebar's location");
    let title = cx.debug_bounds("titlebar-title").expect("the title");
    let window = cx.update(|window, _| window.viewport_size().width);
    for (name, part) in [("location", band), ("title", title)] {
        assert!(
            part.top() >= px(0.) && part.bottom() <= px(crate::theme::WIN_CHROME_H),
            "the {name} rides the titlebar row: {part:?}"
        );
        assert!(part.right() <= window, "the {name} stays inside the window");
    }
    assert!(
        cx.debug_bounds("tasks-meter-1").is_none(),
        "no meter in the chrome"
    );
    assert!(
        cx.debug_bounds("ci-mark-1").is_none(),
        "no PR/CI chip in the chrome"
    );
    let location = view.read_with(cx, |view, _| view.titlebar_location(None));
    let crate::titlebar::Location::Thread { branch, .. } = location else {
        panic!("a Solo Thread's location");
    };
    assert_eq!(branch.as_deref(), Some("feat/thread-pane-header"));
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
/// as glyphs: `⌘N`, `⌘⇧N`, `⌘O`, `⌘G`, `⌘K` (rule 2.11.4, F-13).
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
    assert_eq!(spelled("palette::OpenGroups"), Some(format!("{glyph}G")));
    assert_eq!(spelled("palette::Toggle"), Some(format!("{glyph}K")));
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

/// Stepping focus across a 3×3 board (FL-11): an unfocused Pane is flush —
/// its body runs down to its band, which ends 7px over the Pane's foot —
/// and only the focused Pane carries the status line, so its body alone is
/// shorter, by exactly the status line and its gap. No body's top ever
/// moves, and only the focused cell's line is live: one caret on the board.
#[gpui::test]
fn stepping_focus_across_nine_cells_moves_no_body(cx: &mut TestAppContext) {
    let (view, _fake, cx, _group) = board("board-cmd-bracket", 9, cx);
    // Cells large enough to read at L1 (R12: at least 300 × 360).
    cx.simulate_resize(gpui::size(px(2000.), px(1300.)));
    tick(cx);
    let keys: Vec<(u64, SharedString)> = view.read_with(cx, |view, _| {
        view.panes
            .iter()
            .map(|pane| (pane.thread().unwrap().get(), pane.text_namespace()))
            .collect()
    });
    let bodies = |cx: &mut gpui::VisualTestContext| -> Vec<(u64, gpui::Bounds<gpui::Pixels>)> {
        keys.iter()
            .map(|(key, _)| (*key, bounds(cx, format!("pane-body-{key}"))))
            .collect()
    };
    let live = |cx: &mut gpui::VisualTestContext| -> usize {
        keys.iter()
            .filter(|(_, namespace)| {
                debug_bounds(cx, format!("composer-live-{namespace}")).is_some()
            })
            .count()
    };
    let status = px(crate::theme::COMPOSER_STATUS_GAP + crate::theme::COMPOSER_STATUS_H);
    let tops: Vec<gpui::Pixels> = bodies(cx).iter().map(|(_, body)| body.top()).collect();
    for _ in 0..9 {
        let focused = view.read_with(cx, |view, _| view.focused_thread().unwrap().get());
        let now = bodies(cx);
        assert_eq!(
            now.iter().map(|(_, body)| body.top()).collect::<Vec<_>>(),
            tops,
            "focus moved a body's top"
        );
        let rest: Vec<gpui::Pixels> = now
            .iter()
            .filter(|(key, _)| *key != focused)
            .map(|(_, body)| body.size.height)
            .collect();
        let flush = rest[0];
        assert!(
            rest.iter().all(|height| (*height - flush).abs() <= px(1.)),
            "every unfocused body runs to its band: {rest:?}"
        );
        let mine = now
            .iter()
            .find(|(key, _)| *key == focused)
            .map(|(_, body)| body.size.height)
            .unwrap();
        assert!(
            ((flush - mine) - status).abs() <= px(1.),
            "only the focused body is shorter, by the status line: {mine:?} vs {flush:?}"
        );
        assert_eq!(live(cx), 1, "exactly one caret-bearing line");
        cx.simulate_keystrokes("cmd-]");
        tick(cx);
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

/// Terminal-native (WP-A, WP-D): the Composer's `❯` hangs in the
/// prototype's 2-cell gutter, `COMPOSER_PAD_L` in from the Pane's own edge
/// (the edge is an overlay, F-1), on the same axis as the transcript's
/// prompt `❯`.
#[gpui::test]
fn the_composer_mark_shares_the_transcript_mark_axis(cx: &mut TestAppContext) {
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
    // The Composer's `❯` hangs `COMPOSER_PAD_L` in from the Pane's edge
    // (`.comp{padding-left:2ch}`), and the transcript's `❯` two chrome
    // cells in from the body's left edge (`TX_PAD_L`): one axis.
    let composer = cx.debug_bounds("composer-mark").unwrap();
    let rect = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1);
    assert!(
        (composer.left() - px(rect.x + crate::theme::COMPOSER_PAD_L)).abs() <= px(1.),
        "L1: {composer:?} / {rect:?}"
    );
    let mark = cx.debug_bounds("prompt-mark").unwrap();
    let body = cx.debug_bounds("transcript-prompt").unwrap();
    assert!(
        (mark.left() - (body.left() + px(crate::theme::TX_PAD_L))).abs() <= px(1.),
        "L1: {mark:?} / {body:?}"
    );
    assert!(
        (mark.left() - composer.left()).abs() <= px(1.),
        "the transcript's ❯ {mark:?} and the Composer's {composer:?} share an axis"
    );
    let _ = thread;
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
    // The head names the Thread and nothing else (theme rule 7): the
    // approval reads at the Pane's foot, never as a head word.
    assert!(debug_bounds(cx, format!("pane-head-{key}")).is_some());
    assert!(debug_bounds(cx, format!("head-slot-{key}")).is_none());
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

/// Terminal-native (theme rule 6: no pulsing dots): unread reads as a
/// bright title on a still dot, never a breath, so N unread Panes ask the
/// display for no frames once the toasts rest, and under reduced motion
/// nothing leases the shared pulse clock. (Before the redesign this pinned
/// the unread breath; the assertion on the breathing dot now inverts.)
#[gpui::test]
fn unread_panes_hold_still_and_rest_under_reduced_motion(cx: &mut TestAppContext) {
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
    assert!(
        cx.debug_bounds("breathing-dot").is_none(),
        "unread never breathes"
    );
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
    assert_eq!(
        cx.update(|window, cx| window.simulate_next_frame(cx)),
        0,
        "unread Panes never ask the display for a frame"
    );

    cx.update(|_, cx| cx.set_reduce_motion(true));
    tick(cx);
    cx.executor().advance_clock(Duration::from_millis(
        crate::theme::MOTION_PULSE_LEASE_MS + 2 * crate::theme::MOTION_PULSE_TICK_MS,
    ));
    cx.run_until_parked();
    assert!(
        cx.update(|_, cx| crate::motion::pulse_parked(cx)),
        "reduced motion holds every loop still and leases nothing"
    );
    assert_eq!(cx.update(|window, cx| window.simulate_next_frame(cx)), 0);
}

/// F-7: at the wall every waiting tile shows its quick answers, the digit
/// dim on each (`1 allow` `2 always` `3 deny`); with the focused tile not
/// waiting, `1` answers the tile that has waited longest, and focus stays
/// where the operator left it.
#[gpui::test]
fn the_wall_digits_answer_the_oldest_waiting_tile(cx: &mut TestAppContext) {
    let (view, fake, cx, _group) = board("board-answer-target", 12, cx);
    assert_eq!(
        cx.update(|window, cx| view.read(cx).level_now(window)),
        Level::Wall
    );
    fake.streams.borrow()[1]
        .send(decision("first-wait"))
        .unwrap();
    tick(cx);
    fake.streams.borrow()[2]
        .send(decision("second-wait"))
        .unwrap();
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    tick(cx);
    let waiting = view.read_with(cx, |view, _| {
        [
            view.panes[1].thread().unwrap(),
            view.panes[2].thread().unwrap(),
        ]
    });
    for thread in waiting {
        for word in ["allow", "always", "deny"] {
            let id: &'static str = format!("wall-answer-{}-{word}", thread.get()).leak();
            assert!(
                cx.debug_bounds(id).is_some(),
                "every waiting tile offers {word}"
            );
        }
    }
    cx.simulate_keystrokes("1");
    tick(cx);
    assert!(
        matches!(
            fake.answered.borrow().last(),
            Some((id, DecisionAnswer::Allow { .. })) if id == "first-wait"
        ),
        "1 allowed the tile that waited longest: {:?}",
        fake.answered.borrow()
    );
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused(), 0, "and answering did not move the operator")
    });
}

/// Rule 2.8.7: at the group9 geometry a waiting L1 cell's Decision claims
/// its natural height — `n Deny` sits whole inside the block.
#[gpui::test]
fn a_group_cell_decision_never_clips_its_deny_row(cx: &mut TestAppContext) {
    // 4 on the board: 9 are wall tiles since R12.
    let (view, fake, cx, _group) = board("board-deny-whole", 4, cx);
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
        (deny.left() >= island.left()
            && deny.top() >= island.top()
            && deny.right() <= island.right()
            && deny.bottom() <= island.bottom()),
        "{deny:?} inside {island:?}"
    );
    // Terminal-native: an option is one grid row.
    assert!(deny.size.height >= px(crate::theme::LH_UI));
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

/// A nav title truncates against what its row leaves it. At the nav's own
/// width, a Group member on a long worktree branch that runs five
/// subagents is one line: its tree glyph, its dot, its title pinned to the
/// row's text width less its cells and its word, and the word ending on the
/// row's text edge. The branch and the count are the tooltip's, never marks
/// on the line, so a long branch can never squeeze the title.
#[gpui::test]
fn a_nav_title_takes_what_its_cells_and_word_leave(cx: &mut TestAppContext) {
    use crate::theme::{CH, NAV_CELL, NAV_LINE, NAV_PAD_X, NAV_TEXT_W, NAV_WORD_GAP};
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
    for thread in &threads {
        let id = thread.get();
        let row = view.read_with(cx, |view, _| view.thread_row(*thread));
        let (subagents, branch) = view.read_with(cx, |view, _| {
            let facts = view.facts.get(*thread);
            (
                facts.map_or(0, |facts| facts.subagents),
                facts.and_then(|facts| facts.off_default_branch()),
            )
        });
        assert_eq!(subagents, 5);
        assert_eq!(branch.as_deref(), Some(long_branch));
        let title = bounds(cx, format!("nav-title-{id}"));
        let word = bounds(cx, format!("nav-since-{id}"));
        let whole_row = bounds(cx, format!("nav-thread-{id}"));
        assert_eq!(whole_row.size.height, px(NAV_LINE), "one line");
        // A member: its tree glyph, its dot, then the title.
        let cells = 3.0;
        let text = row
            .tail
            .text()
            .map_or(0.0, |text| text.chars().count() as f32 * CH + NAV_WORD_GAP);
        let expected = NAV_TEXT_W - cells * NAV_CELL - text;
        assert!(
            (title.size.width - px(expected)).abs() < px(0.5),
            "the title takes what its cells and word leave: {title:?}, {expected}"
        );
        assert!(
            (title.left() - (whole_row.left() + px(NAV_PAD_X + cells * NAV_CELL))).abs() < px(0.5),
            "the title starts after the tree glyph and the dot"
        );
        assert!(title.right() <= word.left() + px(0.5));
        assert!(
            (word.right() - (whole_row.right() - px(NAV_PAD_X))).abs() < px(0.5),
            "the word ends on the row's text edge"
        );
        for gone in ["nav-branch", "nav-subagents", "nav-mark"] {
            assert!(
                debug_bounds(cx, format!("{gone}-{id}")).is_none(),
                "{gone} is the tooltip's, not the line's"
            );
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

/// F-10: the Solo titlebar says `needs you` once — the Thread's own state
/// word — and counts nobody (the count is a board's); a board's titlebar
/// counts who on it needs you: `1 needs you`, `2 need you`.
#[gpui::test]
fn the_solo_titlebar_says_needs_you_once(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("needs-you-once", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    fake.streams.borrow()[0].send(question("once")).unwrap();
    tick(cx);
    let words: Vec<String> = view.read_with(cx, |view, _| {
        crate::titlebar::Title {
            location: view.titlebar_location(None),
        }
        .words()
        .into_iter()
        .map(|(word, _)| word.to_string())
        .collect()
    });
    assert!(
        words.iter().any(|word| word.starts_with("needs you")),
        "the Thread's state word says needs you: {words:?}"
    );
    assert!(
        !words.iter().any(|word| word.ends_with("need you")),
        "and no count says it again: {words:?}"
    );
}

#[gpui::test]
fn a_board_titlebar_keeps_its_need_you_count(cx: &mut TestAppContext) {
    let (view, fake, cx, _group) = board("need-you-count", 2, cx);
    fake.streams.borrow()[1].send(decision("count")).unwrap();
    tick(cx);
    let words = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            crate::titlebar::Title {
                location: view.titlebar_location(None),
            }
            .words()
            .into_iter()
            .map(|(word, _)| word.to_string())
            .collect::<Vec<_>>()
        })
    };
    assert!(
        words(&view, cx).contains(&"1 needs you".to_string()),
        "one waiting reads singular: {:?}",
        words(&view, cx)
    );
    fake.streams.borrow()[0].send(decision("count-2")).unwrap();
    tick(cx);
    assert!(
        words(&view, cx).contains(&"2 need you".to_string()),
        "two read plural: {:?}",
        words(&view, cx)
    );
}

/// F-2: one attention edge. On a board every waiting cell draws
/// `ATTENTION_EDGE` (ochre at 75%) on all four sides — the one it would
/// answer included, so no cell wears full ink — and in Solo no state
/// recolours the edge.
#[gpui::test]
fn every_waiting_cell_wears_one_attention_edge_and_solo_none(cx: &mut TestAppContext) {
    assert_eq!(
        crate::theme::ATTENTION_EDGE,
        (crate::theme::ATTENTION << 8) | 0xbf,
        "the waiting edge is ATTENTION itself, at 75%"
    );
    let (view, fake, cx, _group) = board("edge-answer-target", 4, cx);
    fake.streams.borrow()[1].send(decision("edge-1")).unwrap();
    fake.streams.borrow()[2].send(decision("edge-2")).unwrap();
    tick(cx);
    let waiting = view.read_with(cx, |view, _| {
        [
            view.panes[1].thread().unwrap(),
            view.panes[2].thread().unwrap(),
        ]
    });
    for thread in waiting {
        assert!(
            cx.debug_bounds(format!("pane-waiting-edge-{}", thread.get()).leak())
                .is_some(),
            "every waiting cell wears the edge"
        );
        assert!(
            cx.debug_bounds(format!("pane-answer-edge-{}", thread.get()).leak())
                .is_none(),
            "and none wears full ink"
        );
    }

    let (core, fake) = cockpit("edge-solo", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    fake.streams.borrow()[0]
        .send(decision("edge-solo"))
        .unwrap();
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    assert!(
        cx.debug_bounds(format!("pane-waiting-edge-{}", thread.get()).leak())
            .is_none(),
        "Solo draws no waiting edge"
    );
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
