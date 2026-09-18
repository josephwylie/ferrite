use super::*;

#[gpui::test]
fn composer_followup_stays_inside_the_editor_beside_send(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("composer-followup-width", 1);
    let thread = core.threads()[0];
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Updated the parser.".into(),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    core.pump();
    let suggestion = "Run the parser tests and check that the existing fixtures still pass";
    core.deliver_suggestion(thread, suggestion.into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(760.), px(700.)));
    tick(cx);
    let ghost = cx.debug_bounds("prompt-placeholder").unwrap();
    let editor = cx.debug_bounds("focused-prompt-editor").unwrap();
    let send = bounds(
        cx,
        format!("composer-send-{:?}", PaneIdentity::Thread(thread)),
    );
    assert!(ghost.right() <= editor.right());
    assert!(ghost.right() < send.left());
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes[0].composer.read(cx).text(), suggestion);
    });
    assert!(fake.sent.borrow().is_empty());
    assert!(cx.debug_bounds("prompt-placeholder").is_none());
}

#[gpui::test]
fn composer_pointer_actions_target_their_own_pane(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("composer-pointer-panes", 2);
    let group = group_all(&mut core);
    core.enter_group(group).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let threads = view.read_with(cx, |view, _| {
        view.panes
            .iter()
            .map(|pane| pane.thread().unwrap())
            .collect::<Vec<_>>()
    });
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    tick(cx);
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.set("keep this draft".into(), cx)
        });
        view.panes[1].composer.update(cx, |composer, cx| {
            composer.set("send from pane two".into(), cx)
        });
        cx.notify();
    });
    cx.run_until_parked();
    let origin = cx.debug_bounds("prompt-editor").unwrap().left();
    let send = bounds(
        cx,
        format!("composer-send-{:?}", PaneIdentity::Thread(threads[1])),
    );
    cx.simulate_click(send.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(fake.sent.borrow().as_slice(), ["send from pane two"]);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(), Some(threads[1]));
        assert_eq!(view.panes[0].composer.read(cx).text(), "keep this draft");
        assert!(view.panes[1].composer.read(cx).is_empty());
    });
    assert_eq!(
        cx.debug_bounds("focused-prompt-editor").unwrap().left(),
        origin,
        "gaining focus must keep the editor's text origin fixed"
    );

    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    tick(cx);
    let stop = bounds(
        cx,
        format!("composer-stop-{:?}", PaneIdentity::Thread(threads[1])),
    );
    cx.simulate_click(stop.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(*fake.interrupts.borrow(), 1);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(), Some(threads[1]));
        assert_eq!(view.panes[0].composer.read(cx).text(), "keep this draft");
    });

    // At instrument size, actions share the metadata row so they do not
    // narrow every line of a draft or force extra wrapped rows.
    cx.simulate_resize(gpui::size(px(860.), px(500.)));
    tick(cx);
    let editor = cx.debug_bounds("focused-prompt-editor").unwrap();
    let send = bounds(
        cx,
        format!("composer-send-{:?}", PaneIdentity::Thread(threads[1])),
    );
    assert!(editor.right() >= send.right() - px(1.));
    assert!(editor.bottom() <= send.top());
}

#[gpui::test]
fn composer_pointer_send_preserves_busy_queue_behavior(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("composer-pointer-queue", 1);
    let thread = core.threads()[0];
    core.send(thread, "first".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    tick(cx);
    cx.simulate_input("follow up");
    cx.run_until_parked();
    let send = bounds(
        cx,
        format!("composer-send-{:?}", PaneIdentity::Thread(thread)),
    );
    cx.simulate_click(send.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(fake.sent.borrow().as_slice(), ["first"]);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.cockpit.thread(thread).unwrap().queued_all(),
            ["follow up"]
        );
        assert!(view.panes[0].composer.read(cx).is_empty());
    });
    // A disabled empty Send must not invoke Enter's separate unqueue path.
    cx.simulate_click(send.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.cockpit.thread(thread).unwrap().queued_all(),
            ["follow up"]
        );
        assert!(view.panes[0].composer.read(cx).is_empty());
    });
    let stop = bounds(
        cx,
        format!("composer-stop-{:?}", PaneIdentity::Thread(thread)),
    );
    cx.simulate_click(stop.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(*fake.interrupts.borrow(), 1);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.thread(thread).unwrap().queued_all(),
            ["follow up"],
            "the pointer Stop preserves existing interrupt/queue semantics"
        );
    });
}

#[gpui::test]
fn composer_busy_tuning_choices_explain_and_preserve_selection(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("composer-busy-tuning", 1);
    let thread = core.threads()[0];
    core.set_model(thread, Some("sonnet".into())).unwrap();
    core.set_effort(thread, Some("high".into())).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.update(cx, |view, cx| {
        view.open_provider_picker(thread, cx);
        let pick = view
            .popover
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .position(|row| !row.inert && !row.active)
            .unwrap();
        // A click from the menu's previous frame also observes current busy state.
        view.cockpit.send(thread, "work".into());
        view.cockpit.apply_input(
            thread,
            ferrite_core::transcript::Input::Event(SessionEvent::RunState {
                state: ferrite_core::RunState::Running,
            }),
        );
        view.pick(pick, cx);
        let menu = view.popover.as_ref().expect("explanation stays open");
        assert_eq!(menu.rows[0].name, TUNING_BUSY_HINT);
        assert!(menu.rows.iter().all(|row| row.inert));
        assert!(menu.rows.iter().any(|row| row.active));
        assert_eq!(view.cockpit.thread(thread).unwrap().model(), Some("sonnet"));
        view.open_effort_picker(thread, cx);
        let menu = view.popover.as_ref().unwrap();
        assert_eq!(menu.rows[0].name, TUNING_BUSY_HINT);
        assert!(menu.rows.iter().all(|row| row.inert));
        assert_eq!(
            menu.rows.iter().find(|row| row.active).unwrap().name,
            "High"
        );
    });
    fake.streams
        .borrow()
        .last()
        .unwrap()
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        let menu = view
            .popover
            .as_ref()
            .expect("finishing keeps the picker open");
        assert_ne!(menu.rows[0].name, TUNING_BUSY_HINT);
        assert!(menu.rows.iter().any(|row| !row.inert));
        assert_eq!(
            menu.rows.iter().find(|row| row.active).unwrap().name,
            "High"
        );
    });
}

#[gpui::test]
fn composer_pointer_action_does_not_confirm_another_surface(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("composer-modal-guard", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.update_in(cx, |view, window, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set("must stay".into(), cx));
        let identity = view.panes[0].identity;
        view.settings_open = true;
        view.composer_action(identity, false, window, cx);
        assert!(view.settings_open);
        view.settings_open = false;
        // The Solo view shows only the focused Thread, so pane zero is hidden.
        view.focus_pane(1);
        view.composer_action(identity, false, window, cx);
        assert_eq!(view.panes[0].composer.read(cx).text(), "must stay");
        assert!(fake.sent.borrow().is_empty());
    });
}

/// Draft growth follows each actual split's height, including semantic-zoom
/// boundaries; the full draft survives shrinking and returns when expanded.
#[gpui::test]
fn multiline_drafts_keep_context_visible_across_group_sizes(cx: &mut TestAppContext) {
    let draft = "One\nTwo\nThree\nFour\nFive\nSix\nSeven\nEight\nNine\nTen";
    for (count, width, height) in [
        (1, 640., 500.),
        (2, 860., 500.),
        (4, 860., 500.),
        (4, 1200., 800.),
        (6, 1200., 800.),
        (6, 860., 500.),
    ] {
        let (mut core, _) = cockpit(&format!("composer-sized-{count}-{width}"), count);
        if count > 1 {
            let group = group_all(&mut core);
            core.enter_group(group).unwrap();
        }
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(width), px(height)));
        view.update(cx, |view, cx| {
            view.focus_pane(0);
            view.panes[0].composer.update(cx, |composer, cx| composer.set(draft.into(), cx));
            cx.notify();
        });
        tick(cx);
        let (rect, level, thread) = cx.update(|window, cx| {
            let view = view.read(cx);
            (
                view.pane_rects(window).into_iter().find(|(index, _)| *index == 0).unwrap().1,
                view.level_of(0, window),
                view.panes[0].thread().unwrap(),
            )
        });
        if level != Level::Wall {
            let editor = cx.debug_bounds("focused-prompt-editor").unwrap();
            let send = bounds(cx, format!("composer-send-{:?}", PaneIdentity::Thread(thread)));
            assert!(
                editor.top() >= px(rect.y + rect.h * (1. - crate::theme::COMPOSER_MAX_PANE_FRACTION)),
                "typing preserves the majority of the Pane for context: {editor:?} / {rect:?}"
            );
            assert!(editor.bottom() <= px(rect.y + rect.h));
            assert!(send.top() >= px(rect.y) && send.bottom() <= px(rect.y + rect.h));
        }
        view.read_with(cx, |view, cx| assert_eq!(view.panes[0].composer.read(cx).text(), draft));
        view.update(cx, |view, cx| {
            view.cockpit.toggle_fullscreen();
            cx.notify();
        });
        cx.simulate_resize(gpui::size(px(1200.), px(900.)));
        tick(cx);
        let expanded = cx.debug_bounds("focused-prompt-editor").unwrap();
        assert_eq!(expanded.size.height, px(crate::theme::COMPOSER_ROW_H * crate::composer::MAX_ROWS as f32));
        view.read_with(cx, |view, cx| assert_eq!(view.panes[0].composer.read(cx).text(), draft));
    }
}

/// Queue growth stays inside the Composer, with older prompts still reachable
/// by native scrolling; cancellation and take-back keep their keyboard semantics.
#[gpui::test]
fn compact_queue_scrolls_without_covering_context_or_composer_actions(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("composer-bounded-queue", 4);
    let thread = core.threads()[0];
    let other = core.threads()[1];
    core.send(thread, "Working".into());
    core.send(other, "Other work".into());
    for index in 0..8 {
        assert!(core.queue(thread, format!("Queued follow-up {index}")));
        assert!(core.queue(other, format!("Other follow-up {index}")));
    }
    let group = group_all(&mut core);
    core.enter_group(group).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let (namespace, other_namespace) = view.read_with(cx, |view, _| {
        (view.panes[0].text_namespace(), view.panes[1].text_namespace())
    });
    for (width, height) in [(860., 500.), (1000., 520.)] {
        cx.simulate_resize(gpui::size(px(width), px(height)));
        tick(cx);
        let queue = bounds(cx, format!("composer-queue-{namespace}"));
        let other_queue = bounds(cx, format!("composer-queue-{other_namespace}"));
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: queue.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(1000.))),
            ..Default::default()
        });
        tick(cx);
        let latest = bounds(cx, format!("queue-row-{namespace}-0"));
        let other_latest = bounds(cx, format!("queue-row-{other_namespace}-0"));
        let editor = cx.debug_bounds("focused-prompt-editor").unwrap();
        let send = bounds(cx, format!("composer-send-{:?}", PaneIdentity::Thread(thread)));
        assert!(queue.size.height <= px(crate::theme::CELL_HEADER_H + 1.));
        assert!(latest.top() >= queue.top() && latest.bottom() <= queue.bottom());
        assert!(queue.bottom() <= editor.top());
        assert!(editor.bottom() <= send.top() || (editor.top() - send.top()).abs() <= px(1.));
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: queue.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-1000.))),
            ..Default::default()
        });
        tick(cx);
        let oldest = bounds(cx, format!("queue-row-{namespace}-7"));
        assert!(
            oldest.top() >= queue.top() - px(1.) && oldest.bottom() <= queue.bottom() + px(1.),
            "oldest {oldest:?} must fit queue {queue:?}"
        );
        let unaffected = bounds(cx, format!("queue-row-{other_namespace}-0"));
        assert_eq!(unaffected, other_latest, "another Pane keeps its own scroll position");
        assert!(unaffected.top() >= other_queue.top() && unaffected.bottom() <= other_queue.bottom());
    }
    cx.simulate_keystrokes("backspace");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.panes[0].composer.read(cx).is_empty());
        assert_eq!(view.cockpit.thread(thread).unwrap().queued_all().len(), 7);
        assert_eq!(view.cockpit.thread(thread).unwrap().queued(), Some("Queued follow-up 6"));
        assert_eq!(view.cockpit.thread(other).unwrap().queued_all().len(), 8);
    });
    cx.simulate_keystrokes("enter");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes[0].composer.read(cx).text(), "Queued follow-up 6");
        assert_eq!(view.cockpit.thread(thread).unwrap().queued_all().len(), 6);
        assert_eq!(view.cockpit.thread(other).unwrap().queued_all().len(), 8);
    });
    assert_eq!(fake.sent.borrow().as_slice(), ["Working", "Other work"]);
}
