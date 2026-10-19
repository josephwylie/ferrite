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
            (send.left() >= island.left()
                && send.top() >= island.top()
                && send.right() <= island.right()
                && send.bottom() <= island.bottom()),
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
    // Every call is its own row: two calls, two Tab stops.
    cx.simulate_keystrokes("tab enter");
    tick(cx);
    for keys in ["", "tab", "shift-tab"] {
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
        assert!(!view.panes[0].tool_expanded(pane::DisclosureId::Tool("visible-a".into())))
    });
    assert_eq!(fake.sent.borrow().as_slice(), ["Inspect output"]);
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
    // The Decision is the transcript's tail: a Composer grown to its full
    // height leaves the body shorter, and the form stays in the transcript,
    // reached by scrolling — there is no fullscreen door.
    let draft = "Keep the entered values.\nShow the recovery action.\nRetain the selected option.\nRestore the form afterward.\nKeep the focus where it was.\nName the expired session.\nOffer to sign in again.\nThen resume the form.";
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set(draft.into(), cx))
    });
    tick(cx);
    assert!(cx.debug_bounds("question-island").is_some());
    assert!(cx.debug_bounds("question-expand").is_none());
    assert_eq!(composer_text(&view, cx), draft);
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set(String::new(), cx))
    });
    tick(cx);
    assert!(cx.debug_bounds("question-island").is_some());
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
    let choice = cx.debug_bounds("question-choice-0-0").unwrap();
    cx.simulate_click(choice.center(), gpui::Modifiers::none());
    tick(cx);
    let send = bounds(cx, format!("request-submit-{}-{serial}", thread.get()));
    cx.simulate_click(send.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        matches!(&fake.answered.borrow().last().unwrap().1,DecisionAnswer::Questions{answers} if answers[0].picks==[0]),
        "the selected answer survives fallback and inline restoration"
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

/// The keyboard walks a group as the pointer sees it: Tab lands on the
/// summary, a shut group's calls are no stops, Enter opens it, and then
/// Tab steps into its calls.
#[gpui::test]
fn tab_walks_a_group_and_enter_opens_it(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("polish-group-keys", 1);
    let thread = core.threads()[0];
    core.send(thread, "Inspect output".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    for id in ["keys-a", "keys-b"] {
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
    let group = pane::DisclosureId::Group("keys-a".into());
    cx.simulate_keystrokes("tab");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.panes[0].targeted_tool(), Some(&group));
    });
    cx.simulate_keystrokes("tab");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.panes[0].targeted_tool(),
            None,
            "a shut group's calls are no stops"
        );
    });
    cx.simulate_keystrokes("tab");
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.panes[0].tool_expanded(group.clone()), "Enter opens it");
    });
    assert!(cx.debug_bounds("tool-row-keys-b").is_some());
    cx.simulate_keystrokes("tab");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.panes[0].targeted_tool(),
            Some(&pane::DisclosureId::Tool("keys-a".into())),
            "open, Tab steps into its calls"
        );
    });
}
