//! The Pane at any size, and the wall cards the nav reads.
#[allow(unused_imports)]
use super::*;

/// A Pane has one drawing at any size: in the smallest window, the nav
/// and the panel beside it squeeze it under what was once a transcript's
/// floor, and it still reads the transcript and types in the Composer.
#[gpui::test]
fn the_smallest_window_still_draws_the_transcript_and_composer(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("small-window", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    super::hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(
        px(ferrite_ui::theme::WINDOW_MIN_W),
        px(ferrite_ui::theme::WINDOW_MIN_H),
    ));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "A small window still reads.\n\n".into(),
        })
        .unwrap();
    tick(cx);
    let rect = cx.update(|window, cx| view.read(cx).pane_rects(window, cx)[0].1);
    assert!(
        rect.w < 300.0 || rect.h < 360.0,
        "the premise: the cell is under the old 300 by 360 transcript floor: {rect:?}"
    );
    assert!(cx.debug_bounds("composer-block").is_some(), "the Composer");
    assert!(cx.debug_bounds("transcript-answer").is_some(), "the transcript");
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
        view.read_with(cx, |view, cx| !view.cockpit.read(cx).thread(thread).unwrap().busy()),
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
    view.update(cx, |view, cx| view.aim_launch(&repo, cx));
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
    let thread = view.read_with(cx, |view, cx| {
        let thread = view.panes[view.focused(cx)].thread().expect("locked");
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
