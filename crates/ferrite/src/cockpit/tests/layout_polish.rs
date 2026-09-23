//! Layout acceptance at the everyday Group sizes used in the UX audit.
use super::*;

#[gpui::test]
fn group_question_stays_between_its_thread_header_and_growing_composer(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("polish-question-containment", 4);
    let group = group_all(&mut core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    view.update(cx, |view, cx| {
        view.enter_group(group, cx);
        view.focus_pane(0);
        cx.notify();
    });
    fake.streams.borrow()[0]
        .send(question("bounded-question"))
        .unwrap();
    tick(cx);
    for draft in ["", "First line\nSecond line\nThird line", ""] {
        view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.set(draft.into(), cx))
        });
        tick(cx);
        let pane = cx.update(|window, cx| {
            view.read(cx)
                .pane_rects(window)
                .into_iter()
                .find(|(index, _)| *index == 0)
                .unwrap()
                .1
        });
        let island = cx.debug_bounds("question-island").unwrap();
        let composer = cx.debug_bounds("focused-prompt-editor").unwrap();
        assert!(
            island.top() >= px(pane.y + crate::theme::PANE_HEAD_H),
            "Question must leave its Thread identity visible: {island:?} / {pane:?}"
        );
        assert!(island.left() >= px(pane.x) && island.right() <= px(pane.x + pane.w));
        assert!(
            island.bottom() <= composer.top(),
            "Question and Composer may not overlap"
        );
        assert!(
            island.size.height > px(80.),
            "Question must remain usable: {island:?} / {composer:?}"
        );
        let (thread, serial) = view.read_with(cx, |view, _| {
            let thread = view.panes[0].thread().unwrap();
            (
                thread,
                view.cockpit
                    .thread(thread)
                    .unwrap()
                    .activity()
                    .pending_decisions()[0]
                    .handle
                    .serial,
            )
        });
        let content = cx.debug_bounds("question-scroll-content").unwrap();
        assert!(
            content.size.height >= px(crate::theme::LH_UI),
            "the question has a real scroll viewport: {content:?}"
        );
        assert!(content.top() >= island.top());
        let send = bounds(cx, format!("request-submit-{}-{serial}", thread.get()));
        assert!(
            send.top() - content.top() >= px(crate::theme::LH_UI),
            "content keeps a usable viewport above the fixed actions"
        );
        assert!(
            island.contains(&send.origin) && island.contains(&send.bottom_right()),
            "the fixed answer row must stay inside the complete island: {send:?} / {island:?}"
        );
    }
    let choice = cx.debug_bounds("question-choice-0-0").unwrap();
    cx.simulate_click(choice.center(), gpui::Modifiers::none());
    tick(cx);
    let (thread, serial) = view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        (
            thread,
            view.cockpit
                .thread(thread)
                .unwrap()
                .activity()
                .pending_decisions()[0]
                .handle
                .serial,
        )
    });
    let send = bounds(cx, format!("request-submit-{}-{serial}", thread.get()));
    cx.simulate_click(send.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        fake.answered.borrow().len(),
        1,
        "the bounded question remains operable in Group"
    );
}

#[gpui::test]
fn compact_group_paints_complete_latest_rows_after_composer_growth(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("polish-l2-whole-rows", 4);
    let thread = core.threads()[0];
    core.send(thread, "An older prompt that must never be a sliver".into());
    let group = group_all(&mut core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| {
        view.enter_group(group, cx);
        view.focus_pane(0);
        cx.notify();
    });
    for line in 0..16 {
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: format!("Meaningful update {line}.\n\n"),
            })
            .unwrap();
    }
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    for (width, height, draft) in [
        (860., 500., ""),
        (860., 500., "First line\nSecond line\nThird line"),
        (900., 560., ""),
    ] {
        cx.simulate_resize(gpui::size(px(width), px(height)));
        view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.set(draft.into(), cx))
        });
        tick(cx);
        let (namespace, ids) = view.read_with(cx, |view, _| {
            (
                view.panes[0].text_namespace(),
                view.cockpit
                    .thread(thread)
                    .unwrap()
                    .transcript()
                    .blocks()
                    .iter()
                    // What the tail draws: a completed turn leaves no row
                    // (the head's `done` says it).
                    .filter(|block| pane::tail_text(&block.body, false).is_some())
                    .map(|block| block.id)
                    .collect::<Vec<_>>(),
            )
        });
        let tail = bounds(cx, format!("l2-tail-{namespace}"));
        let mut painted = Vec::new();
        for id in &ids {
            if let Some(row) = debug_bounds(cx, format!("l2-tail-row-{namespace}-{id:?}")) {
                assert!(
                    row.top() >= tail.top() && row.bottom() <= tail.bottom(),
                    "a whole semantic row must fit the remaining slot: {row:?} / {tail:?}"
                );
                assert!(
                    row.size.height >= px(crate::theme::LH_META - 1.),
                    "glyph lines must not shrink: {row:?}"
                );
                painted.push(*id);
            }
        }
        assert_eq!(
            painted.last(),
            ids.last(),
            "the newest meaningful update has priority"
        );
        assert!(
            painted.len() < ids.len(),
            "older rows should be omitted whole"
        );
        assert!(tail.bottom() <= cx.debug_bounds("focused-prompt-editor").unwrap().top());
    }
}

#[gpui::test]
fn keyboard_disclosure_target_is_visible_across_three_targets_and_reverse(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("polish-disclosure-target", 1);
    let thread = core.threads()[0];
    core.send(thread, "Inspect output".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    for id in ["visible-a", "visible-b"] {
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolStarted {
                id: id.into(),
                name: "Bash".into(),
                input: serde_json::json!({"command":"echo result"}),
            })
            .unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolCompleted {
                id: id.into(),
                output: "result".into(),
                is_error: false,
                result: ferrite_core::ToolResult::Opaque,
            })
            .unwrap();
    }
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    cx.simulate_keystrokes("tab enter");
    tick(cx);
    for keys in ["", "tab", "tab", "shift-tab", "shift-tab"] {
        if !keys.is_empty() {
            cx.simulate_keystrokes(keys);
        }
        tick(cx);
        let target = cx
            .debug_bounds("tool-disclosure-keyboard-target")
            .expect("keyboard target paints a visible header outline");
        assert!(
            target.size.width > px(100.) && target.size.height >= px(16.),
            "target must paint the full header, not only the chevron: {target:?}"
        );
    }
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(!view.panes[0].tool_expanded(pane::DisclosureId::Group("visible-a".into())))
    });
    assert_eq!(fake.sent.borrow().as_slice(), ["Inspect output"]);
}

/// Compact Questions name themselves in the fixed head's slot and open the
/// retained full form with the expand key; a tall draft must never cover
/// that head or clear a selected answer.
#[gpui::test]
fn compact_group_question_expands_and_retains_answer_and_draft(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("polish-l2-question-expand", 4);
    let group = group_all(&mut core);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(860.), px(500.)));
    view.update(cx, |view, cx| {
        view.enter_group(group, cx);
        view.focus_pane(0);
        cx.notify();
    });
    for (iteration, draft) in [
        "",
        "First line\nSecond line\nThird line",
        "One\nTwo\nThree\nFour\nFive\nSix\nSeven\nEight",
    ]
    .into_iter()
    .enumerate()
    {
        view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.set(draft.into(), cx))
        });
        fake.streams.borrow()[0]
            .send(question(&format!("compact-question-{iteration}")))
            .unwrap();
        tick(cx);
        let (pane, level) = cx.update(|window, cx| {
            let view = view.read(cx);
            (
                view.pane_rects(window)
                    .into_iter()
                    .find(|(index, _)| *index == 0)
                    .unwrap()
                    .1,
                view.level_of(0, window),
            )
        });
        assert_eq!(level, Level::Instruments);
        assert!(
            cx.debug_bounds("question-island").is_none(),
            "L2 must not paint an unusable compressed form"
        );
        // The fixed head's slot says what the cell needs and jumps to it;
        // the expand key opens the full form.
        let slot = cx
            .debug_bounds("head-slot-1")
            .expect("a compact Question names itself in the head's slot");
        assert!(slot.left() >= px(pane.x) && slot.right() <= px(pane.x + pane.w));
        assert!(
            slot.top() >= px(pane.y)
                && slot.bottom() <= px(pane.y + crate::theme::PANE_HEAD_H + 1.),
            "the slot remains in the fixed head even with an eight-line draft: {slot:?} / {pane:?}"
        );
        assert_eq!(composer_text(&view, cx), draft);
        if iteration != 1 {
            // A press on `needs you` runs the ⌘D jump: it lands on the
            // Pane that asks.
            cx.simulate_click(slot.center(), gpui::Modifiers::none());
            tick(cx);
            view.read_with(cx, |view, _| assert_eq!(view.focused(), 0));
        }
        cx.simulate_keystrokes("cmd-f");
        tick(cx);
        cx.update(|window, cx| {
            let view = view.read(cx);
            assert_eq!(view.level_of(0, window), Level::Transcript);
            assert_eq!(
                view.cockpit.roster().fullscreen(),
                Some(view.panes[0].identity)
            );
        });
        let choice = cx
            .debug_bounds("question-choice-0-0")
            .unwrap_or_else(|| panic!("expanded form has choices with draft {iteration}"));
        assert!(
            cx.debug_bounds("pane-head-1").is_none(),
            "fullscreen has no head: the titlebar carries the Thread"
        );
        let answering_editor = cx.debug_bounds("focused-prompt-editor").unwrap();
        let content = cx.debug_bounds("question-scroll-content").unwrap();
        let island = cx.debug_bounds("question-island").unwrap();
        assert!(island.bottom() <= answering_editor.top());
        let viewport = cx.debug_bounds("question-viewport").unwrap();
        assert!(
            choice.top() >= content.top() && choice.bottom() <= viewport.bottom(),
            "a complete option remains above the fixed footer: {choice:?} / {viewport:?}"
        );
        if iteration == 2 {
            assert!(answering_editor.size.height <= px(crate::theme::COMPOSER_ROW_H * 2. + 1.));
        }
        cx.simulate_click(choice.center(), gpui::Modifiers::none());
        tick(cx);
        cx.simulate_keystrokes("cmd-f");
        tick(cx);
        assert_eq!(composer_text(&view, cx), draft);
        if iteration == 2 {
            let restored_editor = cx.debug_bounds("focused-prompt-editor").unwrap();
            assert!(
                restored_editor.size.height
                    <= px(pane.h * crate::theme::COMPOSER_MAX_PANE_FRACTION),
                "returning to a compact Pane keeps the draft viewport bounded"
            );
        }
        assert!(
            cx.debug_bounds("question-expand").is_some(),
            "returning to Group says the question answers expanded"
        );
        cx.simulate_keystrokes("cmd-f");
        tick(cx);
        let (thread, serial) = view.read_with(cx, |view, _| {
            let thread = view.panes[0].thread().unwrap();
            (
                thread,
                view.cockpit
                    .thread(thread)
                    .unwrap()
                    .activity()
                    .pending_decisions()[0]
                    .handle
                    .serial,
            )
        });
        let send = bounds(cx, format!("request-submit-{}-{serial}", thread.get()));
        let island = cx.debug_bounds("question-island").unwrap();
        assert!(island.contains(&send.origin) && island.contains(&send.bottom_right()));
        cx.simulate_click(send.center(), gpui::Modifiers::none());
        tick(cx);
        assert_eq!(fake.answered.borrow().len(), iteration + 1);
        assert!(
            matches!(&fake.answered.borrow().last().unwrap().1, DecisionAnswer::Questions { answers } if answers[0].picks == [0]),
            "selected answer survives collapse and re-expansion"
        );
        assert_eq!(
            composer_text(&view, cx),
            draft,
            "answering must preserve the complete Composer draft"
        );
        if iteration == 2 {
            let restored_editor = cx.debug_bounds("focused-prompt-editor").unwrap();
            assert!(
                restored_editor.size.height > answering_editor.size.height,
                "answering restores the full draft viewport"
            );
        }
        cx.simulate_keystrokes("cmd-f");
        tick(cx);
    }
}

#[gpui::test]
fn group_question_uses_measured_space_and_restores_inline_form(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("polish-question-adaptive", 4);
    let group = group_all(&mut core);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    view.update(cx, |view, cx| {
        view.enter_group(group, cx);
        view.focus_pane(0);
        cx.notify();
    });
    let SessionEvent::DecisionRequested { mut decision } = question("adaptive") else {
        unreachable!()
    };
    let ferrite_core::DecisionKind::Questions(questions) = &mut decision.kind else {
        unreachable!()
    };
    questions[0].question = "How should an expired session affect an unfinished form?".into();
    questions[0].options[0].label = "Keep the draft (Recommended)".into();
    questions[0].options[0].description =
        "Return to the form after sign-in with the entered values preserved.".into();
    questions[0].options[1].description =
        "Discard the previous values and show an empty form after sign-in.".into();
    fake.streams.borrow()[0]
        .send(SessionEvent::DecisionRequested { decision })
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("question-island").is_some());
    assert!(
        cx.debug_bounds("question-expand").is_none(),
        "normal Group space keeps the inline form"
    );
    let first = cx.debug_bounds("question-choice-0-0").unwrap();
    let viewport = cx.debug_bounds("question-viewport").unwrap();
    assert!(first.top() >= viewport.top() && first.bottom() <= viewport.bottom());
    for dy in [-40., 100.] {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: first.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(dy))),
            ..Default::default()
        });
        tick(cx);
        assert!(cx.debug_bounds("question-island").is_some());
        assert!(
            cx.debug_bounds("question-expand").is_none(),
            "scroll position must not affect the fit decision"
        );
    }
    let draft = "Keep the entered values.\nShow the recovery action.\nRetain the selected option.\nRestore the form afterward.";
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set(draft.into(), cx))
    });
    tick(cx);
    assert!(
        cx.debug_bounds("question-island").is_none(),
        "insufficient Group body space uses the expansion action"
    );
    assert!(cx.debug_bounds("question-expand").is_some());
    cx.simulate_keystrokes("cmd-f");
    tick(cx);
    assert_eq!(composer_text(&view, cx), draft);
    let first = cx.debug_bounds("question-choice-0-0").unwrap();
    cx.simulate_click(first.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_keystrokes("cmd-f");
    tick(cx);
    assert!(cx.debug_bounds("question-expand").is_some());
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set(String::new(), cx))
    });
    tick(cx);
    assert!(
        cx.debug_bounds("question-island").is_some(),
        "shrinking the Composer restores the inline form"
    );
    assert!(cx.debug_bounds("question-expand").is_none());
    // Repeat geometry changes to catch stale collapsed state and feedback loops.
    for height in [1000., 800., 1000., 800.] {
        cx.simulate_resize(gpui::size(px(1200.), px(height)));
        tick(cx);
        assert!(cx.debug_bounds("question-island").is_some());
        assert!(cx.debug_bounds("question-expand").is_none());
    }
    let (thread, serial) = view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        (
            thread,
            view.cockpit
                .thread(thread)
                .unwrap()
                .activity()
                .pending_decisions()[0]
                .handle
                .serial,
        )
    });
    let send = bounds(cx, format!("request-submit-{}-{serial}", thread.get()));
    cx.simulate_click(send.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        matches!(&fake.answered.borrow().last().unwrap().1,DecisionAnswer::Questions{answers} if answers[0].picks==[0]),
        "the selected answer survives fallback and inline restoration"
    );
}

/// The compact status owns only the current live reasoning headline. Older
/// observations and completed thinking remain available in the transcript.
#[gpui::test]
fn compact_live_reasoning_appears_once_and_returns_to_history(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("polish-l2-live-reasoning", 4);
    let thread = core.threads()[0];
    core.send(thread, "Review the flow".into());
    let group = group_all(&mut core);
    core.enter_group(group).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    // A 2×2 board of instrument cells: the default grid takes 1×4 at the
    // transcript Level when the window is tall enough for it.
    cx.simulate_resize(gpui::size(px(860.), px(700.)));
    for (item, text) in [
        ("earlier", "The earlier observation is still relevant"),
        ("current", "Checking the remaining interactions"),
    ] {
        fake.streams.borrow()[0]
            .send(SessionEvent::ReasoningSummaryPart {
                item_id: item.into(),
                summary_index: 0,
                text: text.into(),
                snapshot: false,
            })
            .unwrap();
    }
    tick(cx);
    let (namespace, earlier, current) = view.read_with(cx, |view, _| {
        let thoughts = view
            .cockpit
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .filter(|block| matches!(block.body, Body::Thinking(_)))
            .map(|block| block.id)
            .collect::<Vec<_>>();
        (view.panes[0].text_namespace(), thoughts[0], thoughts[1])
    });
    let row = |id| format!("l2-tail-row-{namespace}-{id:?}");
    assert!(
        debug_bounds(cx, row(earlier)).is_some(),
        "older reasoning remains visible"
    );
    assert!(
        debug_bounds(cx, row(current)).is_none(),
        "live reasoning has one presentation"
    );
    assert!(cx
        .debug_bounds("progress-caption-Checking the remaining interactions")
        .is_some());
    fake.streams.borrow()[0]
        .send(SessionEvent::Progress {
            event: ferrite_core::progress::ProgressEvent::Phase {
                phase: ferrite_core::progress::Phase::Compacting,
                detail: String::new(),
            },
        })
        .unwrap();
    tick(cx);
    assert!(
        debug_bounds(cx, row(current)).is_some(),
        "a different live caption does not hide reasoning history"
    );
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    assert!(debug_bounds(cx, row(earlier)).is_some());
    assert!(
        debug_bounds(cx, row(current)).is_some(),
        "completed reasoning is retained"
    );
}

/// A pointer click only opens or closes a disclosure: it draws no keyboard
/// ring on the row and leaves the Composer holding the keyboard. The ring
/// is Tab's alone, and Tab still draws it after a click.
#[gpui::test]
fn clicking_a_disclosure_opens_it_without_a_keyboard_ring(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("polish-disclosure-click", 1);
    let thread = core.threads()[0];
    core.send(thread, "Inspect output".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    for id in ["clicked-a", "clicked-b"] {
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolStarted {
                id: id.into(),
                name: "Bash".into(),
                input: serde_json::json!({"command":"echo result"}),
            })
            .unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolCompleted {
                id: id.into(),
                output: "result".into(),
                is_error: false,
                result: ferrite_core::ToolResult::Opaque,
            })
            .unwrap();
    }
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    let group = pane::DisclosureId::Group("clicked-a".into());
    let at = view.read_with(cx, |view, _| {
        view.panes[0].tool_bounds(group.clone()).unwrap().center()
    });
    cx.simulate_click(at, gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.panes[0].tool_expanded(group.clone()),
            "the click opens the group"
        );
        assert!(
            !view.panes[0].has_tool_target(),
            "a click sets no keyboard target"
        );
    });
    assert!(
        cx.debug_bounds("tool-disclosure-keyboard-target").is_none(),
        "a click paints no keyboard ring"
    );
    cx.update(|window, cx| {
        let pane = &view.read(cx).panes[0];
        assert!(pane.composer.focus_handle(cx).is_focused(window));
    });
    cx.simulate_click(at, gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(
            !view.panes[0].tool_expanded(group.clone()),
            "a second click closes it"
        );
    });
    cx.simulate_keystrokes("tab");
    tick(cx);
    assert!(
        cx.debug_bounds("tool-disclosure-keyboard-target").is_some(),
        "Tab still draws the ring"
    );
}
