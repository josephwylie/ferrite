//! WP-C's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// Solo has no Pane head: the view head in the head row names the Thread
/// (SPEC §5.2), as the nav's row does. A plan draws no meter in the
/// chrome (its task list carries it); the PR/CI chip is not chrome (F-11).
#[gpui::test]
fn the_solo_thread_is_named_by_the_view_head_and_meters_no_plan(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("head-one-row", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let thread = view.read_with(cx, |view, cx| view.cockpit.read(cx).threads()[0]);
    view.update(cx, |view, cx| {
        view.facts.pr_answered(thread, Some(pr_with_checks()));
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
    assert!(cx.debug_bounds("titlebar-location").is_none());
    let row: &'static str = format!("nav-thread-{}", thread.thread().get()).leak();
    assert!(cx.debug_bounds(row).is_some(), "the nav's row names it");
    let head = cx
        .debug_bounds("view-title")
        .expect("the view head names it");
    assert!(
        head.bottom() <= px(ferrite_ui::theme::WIN_CHROME_H),
        "in the head row"
    );
    // The plan reads in the task list alone, and Main wears no strip.
    assert!(cx.debug_bounds("tasks-meter-1").is_none(), "no plan meter");
    assert!(cx.debug_bounds("pane-subagent-1").is_none(), "no strip");
    assert!(
        cx.debug_bounds("ci-mark-1").is_none(),
        "no PR/CI chip in the chrome"
    );
}

// ---------------------------------------------------------------- P4 board

/// The transcript column sits `tx_inset_at` in from the Pane's edges
/// (Solo's 48px at this width): the answer's row starting on its left edge, the prompt's bubble
/// ending on its right.
#[gpui::test]
fn the_prompt_bubble_keeps_the_transcript_column(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("board-mark-axis", 1);
    let thread = core.threads()[0];
    let _ = core.send(thread, "Line the marks up".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    super::hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "They line up.".into(),
        })
        .unwrap();
    tick(cx);
    let rect = cx.update(|window, cx| view.read(cx).pane_rects(window, cx)[0].1);
    let inset = ferrite_ui::theme::tx_inset_at(rect.w, true);
    let column = px(rect.x + inset);
    let column_end = px(rect.x + rect.w - inset);
    let bubble = cx.debug_bounds("transcript-prompt").unwrap();
    assert!(
        (bubble.right() - column_end).abs() <= px(1.),
        "the bubble {bubble:?} ends on the column {column_end:?}"
    );
    let answer = cx.debug_bounds("transcript-answer").unwrap();
    assert!(
        (answer.left() - column).abs() <= px(1.),
        "the answer {answer:?} starts on the column {column:?}"
    );
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
    let (thread, serial) = view.read_with(cx, |view, cx| {
        let thread = view.panes[0].thread().unwrap();
        let serial = view.cockpit.read(cx).thread(thread).unwrap().pending_decisions()[0]
            .handle
            .serial;
        (thread, serial)
    });
    let send = bounds(cx, format!("request-submit-{}-{serial}", thread.thread().get()));
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

/// A card's title takes its whole line. At the nav's own width, a Thread
/// on a long worktree branch that runs five subagents
/// is a 78px card: its title pinned to the card's text width, its branch
/// cut in its middle on line 3 inside the card, so a long branch can
/// never squeeze the title. The subagent count is never on the card.
#[gpui::test]
fn a_card_title_takes_its_line_and_the_branch_its_own(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{NAV_CARD_H, NAV_ROW_PAD_X};
    let (core, fake) = cockpit("nav-title-first", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
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
            view.cockpit.update(cx, |cockpit, _| cockpit.rename_thread(*thread, title).now().unwrap());
            view.facts.renamed(view.cockpit.read(cx), *thread);
        }
        view.facts.set_branches(
            threads
                .iter()
                .map(|thread| {
                    (
                        *thread,
                        Some(ferrite_core::checkout::BranchStatus {
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
        ferrite_nav::COLUMN_WIDTH,
        "the nav at its default width"
    );
    for thread in &threads {
        let id = thread.thread().get();
        let title = bounds(cx, format!("nav-title-{id}"));
        let card = bounds(cx, format!("nav-thread-{id}"));
        assert_eq!(card.size.height, px(NAV_CARD_H), "three lines");
        // One flat list (ADR 0040): every card's title starts at its pad
        // and takes the rest of the line.
        let pad = NAV_ROW_PAD_X;
        let expected = card.size.width - px(2.0 * pad);
        assert!(
            (title.size.width - expected).abs() < px(0.5),
            "the title takes its line: {title:?}, {expected:?}"
        );
        assert!(
            (title.left() - (card.left() + px(pad))).abs() < px(0.5),
            "the title starts at the card's pad"
        );
        let branch = bounds(cx, format!("nav-card-branch-{id}"));
        assert!(
            branch.right() <= card.right() - px(NAV_ROW_PAD_X) + px(0.5),
            "the branch is cut inside the card: {branch:?}"
        );
        assert!(branch.top() >= title.bottom(), "on line 3");
        for gone in ["nav-subagents", "nav-mark"] {
            assert!(
                debug_bounds(cx, format!("{gone}-{id}")).is_none(),
                "{gone} is the tooltip's, not the line's"
            );
        }
    }
}

/// Fix 2: a Thread whose turn ended in an error reads `failed` in the nav,
/// and its dot is `BLOCKED` there, in the rail and in the Pane head — never
/// the idle grey beside a red word. The failure is a finish: once seen it
/// says nothing and wears no dot, as a seen `done` does.
#[gpui::test]
fn a_failed_threads_dot_is_blocked_until_the_failure_is_seen(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("failed-dot", 1);
    let (view, cx) = add_unfocused_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
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
    view.read_with(cx, |view, cx| {
        let thread = view.cockpit.read(cx).threads()[0];
        let row = view.thread_row(thread, cx);
        assert_eq!(
            row.tail,
            nav::NavTail::Failed,
            "unseen, the tail says failed"
        );
        assert_eq!(
            row.status.ink(),
            ferrite_ui::theme::BLOCKED,
            "and the nav and rail mark say it too"
        );
        assert_eq!(
            pane::thread_word(view.cockpit.read(cx).thread(thread).unwrap(), None, true),
            Some(pane::HeadSlot::Failed),
            "the Pane's word agrees"
        );
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        let thread = view.cockpit.read(cx).threads()[0];
        assert!(!view.cockpit.read(cx).notifications(view.cockpit.read(cx).first_machine()).attention(thread.thread()), "seen");
        let row = view.thread_row(thread, cx);
        assert_ne!(row.tail, nav::NavTail::Failed, "seen, it says nothing");
        assert_eq!(row.status, nav::RowStatus::Idle, "and wears no dot");
        assert_eq!(
            pane::thread_word(view.cockpit.read(cx).thread(thread).unwrap(), None, false),
            None,
            "nor does the Pane"
        );
    });
}
