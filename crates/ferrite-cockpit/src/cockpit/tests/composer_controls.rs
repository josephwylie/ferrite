use super::*;

/// A background task can park a Thread without going through this window's
/// close action. The surviving Pane must keep the keyboard and the same draft
/// even when that removal changes its index in the core roster.
#[gpui::test]
fn composer_survives_background_thread_removal(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("composer-background-close", 3);
    let threads = core.threads().to_vec();
    core.focus_thread(threads[2]);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    tick(cx);
    cx.simulate_input("keep this draft");
    tick(cx);

    // Exercise the core mutation seen from the Server, without the UI close
    // action (which already synchronizes its mirror).
    view.update(cx, |view, _| {
        view.cockpit.park_thread(threads[0]).unwrap();
        assert_eq!(
            view.panes[view.focused()].thread(),
            Some(threads[2]),
            "focus follows identity even before the next UI pump"
        );
    });
    tick(cx);
    let editor = cx
        .debug_bounds("focused-prompt-editor")
        .expect("the visible editor still owns focus after another Thread parks");
    cx.simulate_click(editor.center(), gpui::Modifiers::none());
    cx.simulate_keystrokes("end");
    cx.simulate_input(" typed");
    tick(cx);
    view.read_with(cx, |view, cx| {
        let pane = &view.panes[view.pane_for(threads[2]).unwrap()];
        assert_eq!(pane.composer.read(cx).text(), "keep this draft typed");
        assert_eq!(view.panes[view.focused()].thread(), Some(threads[2]));
    });
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(" pasted".into())));
    cx.dispatch_action(Paste);
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.panes[view.focused()].composer.read(cx).text(),
            "keep this draft typed pasted"
        );
    });
    assert!(
        cx.debug_bounds("prompt-placeholder").is_none(),
        "pasted text must be visible in the editor"
    );
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        fake.sent.borrow().as_slice(),
        ["keep this draft typed pasted"]
    );
}

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
    // Terminal-native (WP-D): no send control shares the row; the ghost
    // stays inside the editor, its accept hint whole.
    assert!(ghost.right() <= editor.right());
    assert!(cx.debug_bounds("prompt-placeholder-hint").is_some());
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes[0].composer.read(cx).text(), suggestion);
    });
    assert!(fake.sent.borrow().is_empty());
    assert!(cx.debug_bounds("prompt-placeholder").is_none());
}

/// FL-10 / FL-11: no verb is drawn — ⏎ sends and esc interrupts, as in a
/// terminal — and the keys act on the focused Pane only; an unfocused board
/// cell draws no status line and keeps its draft. Focus never moves the
/// editor's text origin.
#[gpui::test]
fn composer_keys_target_the_focused_pane_only(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("composer-keys-panes", 2);
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
    tick(cx);
    let origin = cx.debug_bounds("prompt-editor").unwrap().left();
    assert!(
        cx.debug_bounds(format!("composer-send-{:?}", PaneIdentity::Thread(threads[1])).leak())
            .is_none(),
        "no verb hint is drawn"
    );
    view.update_in(cx, |view, window, cx| {
        view.focus_pane(1);
        let focus = view.panes[1].composer.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    });
    tick(cx);
    assert_eq!(
        cx.debug_bounds("focused-prompt-editor").unwrap().left(),
        origin,
        "gaining focus must keep the editor's text origin fixed"
    );
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(fake.sent.borrow().as_slice(), ["send from pane two"]);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(), Some(threads[1]));
        assert_eq!(view.panes[0].composer.read(cx).text(), "keep this draft");
        assert!(view.panes[1].composer.read(cx).is_empty());
    });
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(*fake.interrupts.borrow(), 1);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(), Some(threads[1]));
        assert_eq!(view.panes[0].composer.read(cx).text(), "keep this draft");
    });
}

/// While a turn runs, ⏎ queues the line behind it, and esc interrupts and
/// keeps both the new line and the queue.
#[gpui::test]
fn composer_escape_stops_a_running_turn_with_text_in_the_line(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("composer-escape-queue", 1);
    let thread = core.threads()[0];
    core.send(thread, "first".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    tick(cx);
    cx.simulate_input("follow up");
    cx.run_until_parked();
    // Enter queues the line, as it always has.
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(fake.sent.borrow().as_slice(), ["first"]);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.cockpit.thread(thread).unwrap().queued_all(),
            ["follow up"]
        );
        assert!(view.panes[0].composer.read(cx).is_empty());
    });

    // With a new line typed, esc interrupts and keeps it.
    cx.simulate_input("and then this");
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(*fake.interrupts.borrow(), 1);
    assert_eq!(
        fake.sent.borrow().as_slice(),
        ["first"],
        "esc sends nothing"
    );
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.cockpit.thread(thread).unwrap().queued_all(),
            ["follow up"],
            "the interrupt keeps the queue"
        );
        assert_eq!(
            view.panes[0].composer.read(cx).text(),
            "and then this",
            "the line survives the interrupt"
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
    view.update_in(cx, |view, window, cx| {
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
        view.pick(pick, window, cx);
        let menu = view.popover.as_ref().expect("explanation stays open");
        assert_eq!(menu.hold, Some(TUNING_BUSY_HINT));
        assert!(menu.rows.iter().all(|row| row.inert));
        assert!(menu.rows.iter().any(|row| row.active));
        assert_eq!(view.cockpit.thread(thread).unwrap().model(), Some("sonnet"));
        view.open_effort_picker(thread, cx);
        let menu = view.popover.as_ref().unwrap();
        assert_eq!(menu.hold, Some(TUNING_BUSY_HINT));
        assert!(menu.rows.iter().all(|row| row.inert));
        // SPEC §5.5: each busy choice is disabled with the reason in its
        // tooltip; no inert note row says it inside the menu.
        let drawn = view.choice_menu(
            0,
            Kind::Effort,
            gpui::component::button::Button::new("effort-trigger"),
            cx,
        );
        assert!(!drawn.choices.is_empty());
        for choice in &drawn.choices {
            assert!(choice.disabled, "{} is disabled", choice.label);
            assert_eq!(choice.why.as_deref(), Some(TUNING_BUSY_HINT));
            assert_ne!(choice.label.as_ref(), TUNING_BUSY_HINT);
        }
        let menu = view.popover.as_ref().unwrap();
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
        assert_eq!(menu.hold, None);
        assert!(menu.rows.iter().any(|row| !row.inert));
        assert_eq!(
            menu.rows.iter().find(|row| row.active).unwrap().name,
            "High"
        );
    });
}

/// An effort change waiting on the provider holds the pickers, saying so,
/// and lets go of them when the provider answers.
#[gpui::test]
fn a_tuning_change_waiting_on_the_provider_holds_the_pickers(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("composer-pending-tuning", 1);
    let thread = core.threads()[0];
    *fake.effort_answers.borrow_mut() = Some(Vec::new());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.update(cx, |view, cx| {
        view.pick_effort(thread, Some("high".into()), cx);
        assert_eq!(view.cockpit.thread(thread).unwrap().effort(), None);
        view.open_effort_picker(thread, cx);
        let menu = view.popover.as_ref().unwrap();
        assert_eq!(menu.hold, Some(TUNING_PENDING_HINT));
        assert!(menu.rows.iter().all(|row| row.inert));
    });
    fake.effort_answers
        .borrow_mut()
        .as_mut()
        .unwrap()
        .remove(0)
        .send(Ok(()))
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.thread(thread).unwrap().effort(), Some("high"));
        let menu = view
            .popover
            .as_ref()
            .expect("the answer keeps the picker open");
        assert_eq!(menu.hold, None);
        assert!(menu.rows.iter().any(|row| !row.inert));
        assert_eq!(
            menu.rows.iter().find(|row| row.active).unwrap().name,
            "High"
        );
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
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.set(draft.into(), cx));
            cx.notify();
        });
        tick(cx);
        let (rect, level, thread) = cx.update(|window, cx| {
            let view = view.read(cx);
            (
                view.pane_rects(window)
                    .into_iter()
                    .find(|(index, _)| *index == 0)
                    .unwrap()
                    .1,
                view.level_of(0, window),
                view.panes[0].thread().unwrap(),
            )
        });
        if level != Level::Wall {
            let editor = cx.debug_bounds("focused-prompt-editor").unwrap();
            assert!(
                editor.top()
                    >= px(rect.y + rect.h * (1. - ferrite_ui::theme::COMPOSER_MAX_PANE_FRACTION)),
                "typing preserves the majority of the Pane for context: {editor:?} / {rect:?}"
            );
            assert!(editor.bottom() <= px(rect.y + rect.h));
            // The verb hint rides the status line, which an L2 cell does not
            // draw (terminal-native, WP-D); where it shows, it is in the Pane.
            if let Some(send) =
                cx.debug_bounds(format!("composer-send-{:?}", PaneIdentity::Thread(thread)).leak())
            {
                assert!(send.top() >= px(rect.y) && send.bottom() <= px(rect.y + rect.h));
            }
        }
        view.read_with(cx, |view, cx| {
            assert_eq!(view.panes[0].composer.read(cx).text(), draft)
        });
        view.update(cx, |view, cx| {
            view.cockpit.toggle_fullscreen();
            cx.notify();
        });
        cx.simulate_resize(gpui::size(px(1200.), px(900.)));
        tick(cx);
        let expanded = cx.debug_bounds("focused-prompt-editor").unwrap();
        assert_eq!(
            expanded.size.height,
            px(ferrite_ui::theme::COMPOSER_ROW_H * ferrite_pane::composer::MAX_ROWS as f32)
        );
        view.read_with(cx, |view, cx| {
            assert_eq!(view.panes[0].composer.read(cx).text(), draft)
        });
    }
}

/// A board Pane's compact Composer is input only (SPEC §5.9): no queue
/// rows on any cell — an unfocused one says `8 queued` in its box — the
/// live status stays above the box, and cancellation and take-back keep
/// their keyboard semantics.
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
        (
            view.panes[0].text_namespace(),
            view.panes[1].text_namespace(),
        )
    });
    // Reproduce the actual compact surface: checkout metadata, a passed-test
    // badge, live reasoning and elapsed time, queue, and an eight-line draft.
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: "queue-test-run".into(),
            name: "Bash".into(),
            input: serde_json::json!({"command": "cargo test --lib"}),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolCompleted {
            id: "queue-test-run".into(),
            output: "test result: ok. 24 passed; 0 failed; 0 ignored".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::ReasoningSummaryDelta {
            text: "Checking the remaining interactions".into(),
            summary_index: 0,
        })
        .unwrap();
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.set("One\nTwo\nThree\nFour\nFive\nSix\nSeven\nEight".into(), cx);
        });
    });
    // The smallest 2×2 boards whose cells still read as transcripts (R12:
    // 300 wide, 360 tall, under the 46px head row and over the 30px bar and
    // the frame); anything smaller is the wall, with no Composer. The panel
    // is off, so the board is the window less the nav.
    hide_panel(&view, cx);
    for (width, height) in [(900., 812.), (1000., 830.)] {
        cx.simulate_resize(gpui::size(px(width), px(height)));
        tick(cx);
        assert!(
            cx.debug_bounds(format!("composer-queue-{namespace}").leak())
                .is_some(),
            "the focused box draws its queued row"
        );
        assert!(
            cx.debug_bounds(format!("composer-queue-{other_namespace}").leak())
                .is_none(),
            "an unfocused box draws none"
        );
        let other_cell = cx.update(|window, cx| {
            view.read(cx)
                .pane_rects(window)
                .into_iter()
                .find(|(at, _)| *at == 1)
                .unwrap()
                .1
        });
        let facts = cx
            .debug_bounds("composer-flat-facts")
            .expect("the unfocused box counts the queue");
        assert!(
            facts.left() >= px(other_cell.x) && facts.right() <= px(other_cell.x + other_cell.w),
            "{facts:?} rides the unfocused cell {other_cell:?}"
        );
        assert_eq!(ferrite_pane::pane::flat_facts_text(0, 8), "8 queued");
        let editor = cx.debug_bounds("focused-prompt-editor").unwrap();
        let cell = cx.update(|window, cx| {
            view.read(cx)
                .pane_rects(window)
                .into_iter()
                .find(|(at, _)| *at == 0)
                .unwrap()
                .1
        });
        assert!(
            editor.bottom() <= px(cell.y + cell.h),
            "the eight-line draft stays inside its cell: {editor:?} / {cell:?}"
        );
        if width == 900. {
            let progress = bounds(cx, format!("transcript-progress-{namespace}"));
            assert!(
                progress.bottom() <= editor.top(),
                "the complete live status stays above the box: {progress:?} / {editor:?}"
            );
            assert!(progress.size.height <= px(ferrite_ui::theme::COMPOSER_ROW_H));
        }
    }
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set(String::new(), cx));
    });
    tick(cx);
    cx.simulate_keystrokes("backspace");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.panes[0].composer.read(cx).is_empty());
        assert_eq!(view.cockpit.thread(thread).unwrap().queued_all().len(), 7);
        assert_eq!(
            view.cockpit.thread(thread).unwrap().queued(),
            Some("Queued follow-up 6")
        );
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

/// SPEC §5.5: the Solo Composer's toolbar holds every picker the bar's
/// status line did. Each chip opens its own picker from a click — with a
/// live Session, and again once that Session has closed, from the cached
/// answers (ADR 0032) — and each menu rests above its chip.
#[gpui::test]
fn the_toolbar_chips_open_their_pickers_with_and_without_a_session(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("toolbar-pickers", 1);
    bind_production_keys(cx);
    *fake.native_controls.borrow_mut() = true;
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0]
        .send(SessionEvent::PermissionMode {
            mode: "native-mode".into(),
        })
        .unwrap();
    tick(cx);
    assert!(
        cx.debug_bounds("composer-meta").is_none(),
        "the bar carries no status line"
    );
    for live in [true, false] {
        if !live {
            fake.streams.borrow()[0]
                .send(SessionEvent::Closed {
                    reason: "settled".into(),
                })
                .unwrap();
            tick(cx);
        }
        let toolbar = cx
            .debug_bounds("composer-toolbar")
            .expect("the Solo box draws its toolbar");
        let chip = |name: &str, cx: &mut gpui::VisualTestContext| {
            let chip = debug_bounds(cx, format!("{name}-{}", thread.get()))
                .unwrap_or_else(|| panic!("the {name} chip (live: {live})"));
            assert!(toolbar.contains(&chip.center()), "{name} rides the toolbar");
            chip
        };
        // The mode: its menu is the Session's own list.
        let mode = chip("mode-picker", cx);
        cx.simulate_click(mode.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        tick(cx);
        view.read_with(cx, |view, _| {
            assert!(view.mode_picker.is_some(), "the mode chip opens its menu");
        });
        let item = cx
            .debug_bounds("choice-row-0")
            .expect("the mode menu is drawn");
        assert!(item.bottom() <= mode.top(), "{item:?} rests above {mode:?}");
        view.update(cx, |view, cx| {
            view.mode_picker = None;
            cx.notify();
        });
        tick(cx);
        // The model, then the effort: each opens its part of the picker.
        for (name, effort) in [("model-picker", false), ("effort-picker", true)] {
            let at = chip(name, cx);
            cx.simulate_click(at.center(), gpui::Modifiers::none());
            cx.run_until_parked();
            tick(cx);
            view.read_with(cx, |view, _| {
                let open = view.popover.as_ref().expect("the chip opens its picker");
                assert_eq!(open.kind.picker_slot(), Some((effort, false)), "{name}");
            });
            let row = cx.debug_bounds("choice-menu").expect("the picker is drawn");
            assert!(row.bottom() <= at.top(), "{name}: {row:?} above {at:?}");
            view.update(cx, |view, cx| {
                view.popover = None;
                cx.notify();
            });
            tick(cx);
        }
    }
}

/// SPEC §5.5: however narrow the box, send stands whole inside it, its
/// toolbar padding from the box's end — the chips give way, never the
/// circle — and the mode chip, the one that gives way first, never runs
/// under the model chip.
#[gpui::test]
fn the_send_circle_stays_inside_a_narrow_box(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("toolbar-narrow", 1);
    *fake.native_controls.borrow_mut() = true;
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0]
        .send(SessionEvent::PermissionMode {
            mode: "a-long-native-permission-mode".into(),
        })
        .unwrap();
    for width in [1200., 900., 720.] {
        cx.simulate_resize(gpui::size(px(width), px(800.)));
        tick(cx);
        let field = cx.debug_bounds("composer-block").expect("the box");
        let send = cx.debug_bounds("composer-send").expect("the send circle");
        assert!(
            send.left() >= field.left()
                && send.right()
                    <= field.right() - px(ferrite_ui::theme::COMPOSER_TOOLBAR_PAD_X - 0.5),
            "at {width}px send stands inside the box: {send:?} / {field:?}"
        );
        let mode = debug_bounds(cx, format!("mode-picker-{}", thread.get())).expect("mode");
        let model = debug_bounds(cx, format!("model-picker-{}", thread.get())).expect("model");
        assert!(
            mode.right() <= model.left(),
            "at {width}px the mode gives way before the model: {mode:?} / {model:?}"
        );
    }
}

/// SPEC §5.5: the queued row says what waits — the top of the pile, `+N`
/// for the rest — and its close button drops it, as ⌫ does. The send circle
/// sends what is typed; with a turn running and nothing typed it is `stop`,
/// and interrupts.
#[gpui::test]
fn the_queued_row_drops_and_the_send_circle_sends_or_stops(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("toolbar-send", 1);
    let thread = core.threads()[0];
    core.send(thread, "Working".into());
    assert!(core.queue(thread, "First follow-up".into()));
    assert!(core.queue(thread, "Second follow-up".into()));
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "working".into(),
        })
        .unwrap();
    tick(cx);
    assert!(
        cx.debug_bounds("queued-more").is_some(),
        "+1 counts the rest"
    );
    let close = cx
        .debug_bounds("queued-close")
        .expect("the row's close button");
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.thread(thread).unwrap().queued_all(),
            ["First follow-up"],
            "the close button drops the top of the pile"
        );
    });
    assert!(cx.debug_bounds("queued-more").is_none());
    // A running turn and an empty line: the circle stops it.
    let stop = cx
        .debug_bounds("composer-stop")
        .expect("stop while working");
    cx.simulate_click(stop.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(*fake.interrupts.borrow(), 1, "stop is Esc's act");
    // Typed text turns it back into send, which is Enter's act.
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set("Typed".into(), cx));
    });
    tick(cx);
    let send = cx.debug_bounds("composer-send").expect("send with text");
    cx.simulate_click(send.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.panes[0].composer.read(cx).is_empty(), "sent");
    });
}

/// SPEC §5.9 item 5: the focused Group Pane's compact box carries Solo's
/// queued row above its input, growing only while something is queued;
/// ↑ takes the prompt back and ⌫ on its empty line drops it. An unfocused Pane says it in its
/// placeholder instead.
#[gpui::test]
fn the_focused_group_pane_shows_its_queue_and_backspace_drops_it(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("group-queue-row", 2);
    let threads = core.threads().to_vec();
    let group = group_all(&mut core);
    core.enter_group(group).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    let focus = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext, thread| {
        view.update(cx, |view, cx| {
            let index = view.pane_for(thread).unwrap();
            view.focus_pane(index);
            cx.notify();
        });
        tick(cx);
    };
    focus(&view, cx, threads[0]);
    let queued = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.cockpit
                .thread(threads[0])
                .and_then(|open| open.queued().map(str::to_string))
        })
    };
    // The focused Pane's live input line.
    let focused_line =
        |cx: &mut gpui::VisualTestContext| cx.debug_bounds("focused-prompt-editor").expect("live");
    let before = focused_line(cx);
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "working".into(),
        })
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("queued-0").is_none(), "nothing queued yet");
    cx.simulate_input("also this");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(queued(&view, cx).as_deref(), Some("also this"));
    let row = cx.debug_bounds("queued-0").expect("the queued row");
    let after = focused_line(cx);
    assert!(row.bottom() <= after.top(), "above the input");
    assert_eq!(
        after.top() - before.top(),
        px(0.),
        "the box grows upward, the input stays put"
    );
    assert!(cx.debug_bounds("queued-close").is_some(), "with its close");

    // Unfocused, the Pane counts it in its placeholder; focused again, the
    // row is back.
    focus(&view, cx, threads[1]);
    assert!(
        cx.debug_bounds("queued-0").is_none(),
        "the focused Pane's only"
    );
    assert!(cx.debug_bounds("composer-flat-facts").is_some());
    focus(&view, cx, threads[0]);
    assert!(cx.debug_bounds("queued-0").is_some());

    // ↑ takes the prompt back to edit, as in Solo; ⏎ queues it again.
    cx.simulate_keystrokes("up");
    tick(cx);
    assert_eq!(queued(&view, cx), None, "↑ takes it back");
    let pane = view.read_with(cx, |view, _| view.pane_for(threads[0]).unwrap());
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes[pane].composer.read(cx).text(), "also this");
    });
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(queued(&view, cx).as_deref(), Some("also this"));

    cx.simulate_keystrokes("backspace");
    tick(cx);
    assert_eq!(queued(&view, cx), None, "⌫ on the empty line drops it");
    assert!(cx.debug_bounds("queued-0").is_none(), "and the row goes");
}
