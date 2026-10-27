//! The wall: grid membership, levels and wall cards.
#[allow(unused_imports)]
use super::*;

/// AC4: the wall flags a Thread, and one key answers it from across the
/// room — the operator never focuses the Pane it belongs to.
#[gpui::test]
fn a_wall_flagged_decision_is_answered_without_focusing_its_pane(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("wall-answer", 24);
    let group = group_all(&mut core);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("y", Allow, Some("Wall"))]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    // A 24-member Group lays out on the default grid, which picks the
    // highest Level every cell can hold: at 1200×900 a 4×6 grid still
    // keeps ~217×133px instruments. 1060×770 beside the open column
    // (the flush board, theme WP-C) leaves no grid whose cells clear the
    // 200×120px instruments floor, which is the range this test is
    // about — held open, since folding the nav away (rule 2.7.7) would
    // lift its cells over the floor.
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1060.), px(770.)));
    view.update(cx, |view, _| {
        assert_eq!(view.panes.len(), 24);
    });
    cx.update(|window, cx| {
        assert_eq!(view.read(cx).level_now(window), Level::Wall);
    });
    fake.streams.borrow()[7].send(decision("perm_08")).unwrap();
    tick(cx);
    let flagged = view.read_with(cx, |view, _| view.panes[7].thread().unwrap());
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused(), 0, "focus stays where the operator left it");
        assert!(view
            .cockpit
            .thread(flagged)
            .and_then(|open| open.pending())
            .is_some());
    });

    cx.simulate_keystrokes("y");

    view.read_with(cx, |view, _| {
        assert!(
            view.cockpit
                .thread(flagged)
                .and_then(|open| open.pending())
                .is_none(),
            "the flagged Thread is the one that got answered"
        );
        assert_eq!(view.focused(), 0, "and answering did not move the operator");
    });
}

/// A Group's default grid is the aspect rule (`layout::grid_shape`), and
/// it is the *only* grid: with the global wall gone (#28) there is no
/// board-specific shape to follow and no six-column ceiling — a Group is
/// as wide as its own membership and the window make it. The app
/// window's board (1440×900 less the nav and chrome) is 1134×838;
/// `grid_shape` returns (columns, rows).
#[test]
fn the_grid_follows_the_groups_own_membership() {
    let board = layout::Rect {
        x: 0.0,
        y: 0.0,
        w: 1134.0,
        h: 838.0,
    };
    let shape = |count| layout::grid_shape(count, board);
    assert_eq!(shape(1), (1, 1));
    assert_eq!(shape(2), (2, 1));
    assert_eq!(shape(6), (3, 2));
    // Four is a plain 2×2 now — the prototype's tall-left 2×3 is gone.
    assert_eq!(shape(4), (2, 2));
    assert_eq!(shape(9), (3, 3));
    assert_eq!(shape(12), (4, 3));
    // Past the old wall's 24, and past its six columns: nothing clamps.
    assert_eq!(shape(24), (5, 5));
    assert_eq!(shape(48), (7, 7));
    assert_eq!(shape(100), (10, 10));
}

/// AC1: no mode switch — the same cockpit renders at a different altitude
/// when the window changes size.
#[gpui::test]
fn resizing_the_window_changes_every_panes_level(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("resize", 4);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.run_until_parked();

    let wide = cx.update(|window, cx| view.read(cx).level_now(window));
    // Narrow enough that the board under the folded nav is narrower
    // than a transcript (300px).
    cx.simulate_resize(gpui::size(gpui::px(300.), gpui::px(280.)));
    let narrow = cx.update(|window, cx| view.read(cx).level_now(window));

    assert!(
        narrow < wide,
        "a smaller window must fall to a lower level: {narrow:?} vs {wide:?}"
    );
}

/// What a Thread's wall card says right now: its alert context and the
/// text of its lines.
fn wall_text(
    view: &gpui::Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
    thread: ThreadId,
) -> (String, Vec<String>) {
    view.read_with(cx, |view, _| {
        let wall = &view.facts.get(thread).expect("an open Thread").wall;
        (
            wall.context.to_string(),
            wall.lines
                .iter()
                .map(|line| line.text.to_string())
                .collect(),
        )
    })
}

/// What the operator must see at once is never held for the throttle:
/// a Decision arriving, its answer, and the turn's end — even a turn
/// that starts and ends inside one quarter second.
#[gpui::test]
fn a_decision_or_a_turn_end_refolds_the_wall_card_at_once(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("wall-forced", 1);
    let thread = core.threads()[0];
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("y", Allow, Some("Decision"))]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    step(cx, 1_000);
    let send = |event: SessionEvent| fake.streams.borrow()[0].send(event).unwrap();
    let turn_ended = || SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Completed,
        cost_usd: None,
    };

    send(SessionEvent::TextDelta {
        text: "working\n\n".into(),
    });
    step(cx, 16);
    send(decision("perm_wall"));
    step(cx, 16);
    assert!(
        wall_text(&view, cx, thread).0.contains("ferrite-perm.txt"),
        "a Decision reaches the card at once: {:?}",
        wall_text(&view, cx, thread)
    );
    cx.simulate_keystrokes("y");
    assert!(
        !wall_text(&view, cx, thread).0.contains("ferrite-perm.txt"),
        "and so does its answer"
    );

    send(turn_ended());
    step(cx, 16);
    // An idle card was just refolded; inside its quarter second a turn
    // starts, streams and ends. The end is seen and refolds at once.
    send(SessionEvent::TextDelta {
        text: "a quick turn\n\n".into(),
    });
    step(cx, 16);
    send(turn_ended());
    step(cx, 16);
    assert!(
        view.read_with(cx, |view, _| !view.cockpit.thread(thread).unwrap().busy()),
        "the premise: the turn is over"
    );
    assert!(
        wall_text(&view, cx, thread)
            .1
            .iter()
            .any(|line| line.contains("a quick turn")),
        "the turn's end refolds the card at once: {:?}",
        wall_text(&view, cx, thread)
    );
}

/// #29: the header's binding slot is fed from the branch cache — the
/// actual git checkout of the Thread's cwd, read at bootstrap and on
/// refresh, so an agent that switches branches is reported honestly.
/// Display-only by construction: nothing here is a control.
#[gpui::test]
fn the_locked_pane_caches_the_actual_checkout_branch(cx: &mut TestAppContext) {
    let base = scratch("branch-cache");
    let repo = repo_in(&base);
    let fake = Fake::default();
    let store = Store::open(base.join("threads")).unwrap();
    let core = Cockpit::new(store, Box::new(fake));
    bind_band_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.aim_launch(&repo));
    tick(cx);

    cx.simulate_input("hello");
    cx.simulate_keystrokes("enter");

    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    let expected = git(&["branch", "--show-current"]);
    let thread = view.read_with(cx, |view, _| {
        let thread = view.panes[view.focused()].thread().expect("locked");
        assert_eq!(
            view.facts
                .get(thread)
                .and_then(|facts| facts.branch.as_ref())
                .map(|branch| branch.to_string()),
            Some(expected.clone()),
            "the bootstrap cached the checkout"
        );
        thread
    });

    // The agent moves the checkout while no Session event arrives. The
    // watchdog cadence still refreshes and repaints the header.
    git(&["checkout", "-q", "-b", "agent-moved"]);
    view.update(cx, |view, cx| {
        view.swept = cx.background_executor().now() - SWEEP_INTERVAL;
        view.pump(cx);
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.facts
                .get(thread)
                .and_then(|facts| facts.branch.as_ref())
                .map(|branch| branch.to_string()),
            Some(expected),
            "the pump returns before the background Git refresh"
        );
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.facts
                .get(thread)
                .and_then(|facts| facts.branch.as_ref())
                .map(|branch| branch.to_string()),
            Some("agent-moved".to_string()),
            "the slot follows the repo, not the binding"
        );
    });
}
