use super::*;
use crate::cockpit::model_picker::Picked;

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
    view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| cockpit.park_thread(threads[0]).now().unwrap());
        assert_eq!(
            view.panes[view.focused(cx)].thread(),
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
        assert_eq!(view.panes[view.focused(cx)].thread(), Some(threads[2]));
    });
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(" pasted".into())));
    cx.dispatch_action(Paste);
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.panes[view.focused(cx)].composer.read(cx).text(),
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

/// While a turn runs, ⏎ queues the line behind it, and esc interrupts and
/// keeps both the new line and the queue.
#[gpui::test]
fn composer_escape_stops_a_running_turn_with_text_in_the_line(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("composer-escape-queue", 1);
    let thread = core.threads()[0];
    let _ = core.send(thread, "first".into());
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
            view.cockpit
                .read(cx)
                .thread(thread)
                .unwrap()
                .queue()
                .iter()
                .map(|item| item.text.as_str())
                .collect::<Vec<_>>(),
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
            view.cockpit
                .read(cx)
                .thread(thread)
                .unwrap()
                .queue()
                .iter()
                .map(|item| item.text.as_str())
                .collect::<Vec<_>>(),
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
    core.set_model(thread, Some("sonnet".into())).now().unwrap();
    core.set_effort(thread, Some("high".into())).now().unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.update_in(cx, |view, window, cx| {
        view.open_provider_picker(thread, cx);
        let pick = view
            .model_picker_view(cx)
            .unwrap()
            .rows
            .iter()
            .position(|row| !row.inert && !row.active)
            .unwrap();
        // A click from the menu's previous frame also observes current busy state.
        let _ = view.cockpit.update(cx, |cockpit, _| cockpit.send(thread, "work".into()));
        view.cockpit.update(cx, |cockpit, _| cockpit.apply_input(
            thread,
            ferrite_core::transcript::Input::Event(SessionEvent::RunState {
                state: ferrite_core::RunState::Running,
            }),
        ));
        let _ = window;
        view.pick_in_model_picker(Picked::Model(pick), cx);
        let picker = view.model_picker_view(cx).expect("explanation stays open");
        assert_eq!(picker.hold, Some(TUNING_BUSY_HINT));
        assert!(picker.rows.iter().all(|row| row.inert));
        assert!(picker.rows.iter().any(|row| row.active));
        assert_eq!(view.cockpit.read(cx).thread(thread).unwrap().model(), Some("sonnet"));
        view.open_effort_picker(thread, cx);
        let picker = view.model_picker_view(cx).unwrap();
        assert_eq!(picker.hold, Some(TUNING_BUSY_HINT));
        // SPEC §5.5: each busy choice is disabled, the reason its tooltip.
        assert!(!picker.efforts.is_empty());
        assert!(picker.efforts.iter().all(|rung| rung.inert));
        assert_eq!(
            picker.efforts.iter().find(|rung| rung.active).unwrap().name,
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
    view.read_with(cx, |view, cx| {
        let picker = view
            .model_picker_view(cx)
            .expect("finishing keeps the picker open");
        assert_eq!(picker.hold, None);
        assert!(picker.rows.iter().any(|row| !row.inert));
        assert_eq!(
            picker.efforts.iter().find(|rung| rung.active).unwrap().name,
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
        assert_eq!(view.cockpit.read(cx).thread(thread).unwrap().effort(), None);
        view.open_effort_picker(thread, cx);
        let picker = view.model_picker_view(cx).unwrap();
        assert_eq!(picker.hold, Some(TUNING_PENDING_HINT));
        assert!(picker.efforts.iter().all(|rung| rung.inert));
    });
    fake.effort_answers
        .borrow_mut()
        .as_mut()
        .unwrap()
        .remove(0)
        .send(Ok(()))
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.cockpit.read(cx).thread(thread).unwrap().effort(), Some("high"));
        let picker = view
            .model_picker_view(cx)
            .expect("the answer keeps the picker open");
        assert_eq!(picker.hold, None);
        assert!(picker.efforts.iter().any(|rung| !rung.inert));
        assert_eq!(
            picker.efforts.iter().find(|rung| rung.active).unwrap().name,
            "High"
        );
    });
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
            .debug_bounds("composer-footer")
            .expect("the Solo box draws its footer");
        let chip = |name: &str, cx: &mut gpui::VisualTestContext| {
            let chip = debug_bounds(cx, format!("{name}-{}", thread.thread().get()))
                .unwrap_or_else(|| panic!("the {name} chip (live: {live})"));
            assert!(toolbar.contains(&chip.center()), "{name} rides the footer");
            chip
        };
        // The mode: its menu is the Session's own list.
        let mode = chip("mode-picker", cx);
        cx.simulate_click(mode.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        tick(cx);
        view.read_with(cx, |view, _| {
            assert!(view.mode_menu().is_some(), "the mode chip opens its menu");
        });
        let item = cx
            .debug_bounds("mode-segment-0")
            .expect("the mode menu is drawn");
        assert!(item.bottom() <= mode.top(), "{item:?} rests above {mode:?}");
        view.update(cx, |view, cx| {
            view.close_popup(cx);
        });
        tick(cx);
        // The model and its effort are one chip: it opens the model
        // picker, the effort its Reasoning column.
        let at = chip("model-picker", cx);
        cx.simulate_click(at.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        tick(cx);
        view.read_with(cx, |view, _| {
            assert!(view.model_picker().is_some(), "the chip opens its picker");
        });
        let menu = cx.debug_bounds("model-picker").expect("the picker is drawn");
        assert!(menu.bottom() <= at.top(), "{menu:?} above {at:?}");
        assert!(
            cx.debug_bounds("model-picker-reasoning").is_some(),
            "the effort rides the model picker as its column"
        );
        view.update(cx, |view, cx| {
            view.close_popup(cx);
        });
        tick(cx);
    }
}

/// SPEC §5.5: however narrow the box, send (once there is text, Claude's:
/// an empty box shows none) stands whole inside it, its row's padding from
/// the box's end, and under the box the model word never runs under the
/// mode's.
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
    tick(cx);
    assert!(
        cx.debug_bounds("composer-send").is_none(),
        "no send in an empty box"
    );
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set("Typed".into(), cx));
    });
    for width in [1200., 900., 720.] {
        cx.simulate_resize(gpui::size(px(width), px(800.)));
        tick(cx);
        let field = cx.debug_bounds("composer-block").expect("the box");
        let send = cx.debug_bounds("composer-send").expect("the send circle");
        assert!(
            send.left() >= field.left()
                && send.right() <= field.right() - px(ferrite_ui::theme::COMPOSER_ROW_PAD - 0.5),
            "at {width}px send stands inside the box: {send:?} / {field:?}"
        );
        let mode = debug_bounds(cx, format!("mode-picker-{}", thread.thread().get())).expect("mode");
        let model = debug_bounds(cx, format!("model-picker-{}", thread.thread().get())).expect("model");
        assert!(
            model.right() <= mode.left(),
            "at {width}px the model never runs under the mode: {mode:?} / {model:?}"
        );
    }
}

/// In a narrow box the model's label cuts before the permission mode's
/// word does: the mode chip keeps its whole width while the model chip has
/// any label left to give. (A long model name: Claude's own, `Opus 5.5`,
/// is too short to have to cut before the box goes compact.)
#[gpui::test]
fn the_model_gives_way_before_the_permission_mode(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("toolbar-priority", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    for event in [
        SessionEvent::Init {
            session_id: "priority".into(),
            model: "research-preview-with-a-long-model-name".into(),
        },
        SessionEvent::PermissionMode {
            mode: "acceptEdits".into(),
        },
    ] {
        fake.streams.borrow()[0].send(event).unwrap();
    }
    cx.simulate_resize(gpui::size(px(1600.), px(800.)));
    tick(cx);
    let roomy = debug_bounds(cx, format!("mode-picker-{}", thread.thread().get())).expect("mode");
    let roomy_model = debug_bounds(cx, format!("model-picker-{}", thread.thread().get())).expect("model");
    // Narrow until the model has had to give some of its label. Its env
    // words now stand over the box (`env_strip`), so the footer is roomy
    // and the model starts to cut only in a narrow window.
    let mut width = 1600.;
    loop {
        width -= 20.;
        cx.simulate_resize(gpui::size(px(width), px(800.)));
        tick(cx);
        let model = debug_bounds(cx, format!("model-picker-{}", thread.thread().get())).expect("model");
        if model.size.width < roomy_model.size.width - px(1.) || width < 500. {
            break;
        }
    }
    let mode = debug_bounds(cx, format!("mode-picker-{}", thread.thread().get())).unwrap();
    assert!(width >= 500., "the model chip gave way at some width");
    assert!(
        (mode.size.width - roomy.size.width).abs() <= px(1.),
        "at {width}px the mode keeps its word while the model cuts: {mode:?} / {roomy:?}"
    );
}

/// SPEC §5.5: the queue strip says what waits — how many, the top of the
/// pile — and its remove button drops the top, as ⌫ does. The send circle
/// sends what is typed; with a turn running and nothing typed it is `stop`,
/// and interrupts.
#[gpui::test]
fn the_queued_row_drops_and_the_send_circle_sends_or_stops(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("toolbar-send", 1);
    let thread = core.threads()[0];
    let _ = core.send(thread, "Working".into());
    assert!(core.queue(thread, "First follow-up".into()).now().unwrap());
    assert!(core.queue(thread, "Second follow-up".into()).now().unwrap());
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
        cx.debug_bounds("queued-count-2").is_some(),
        "the strip counts both"
    );
    // A row each, the one to go first on top; each row's close drops its
    // own message, not the latest.
    let first = cx.debug_bounds("queued-1").expect("the first queued");
    let second = cx.debug_bounds("queued-0").expect("the latest");
    assert!(first.bottom() <= second.top(), "in the order they go");
    let close = cx
        .debug_bounds("queued-close-1")
        .expect("the first row's close button");
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.cockpit
                .read(cx)
                .thread(thread)
                .unwrap()
                .queue()
                .iter()
                .map(|item| item.text.as_str())
                .collect::<Vec<_>>(),
            ["Second follow-up"],
            "its own row's close drops the first, and only it"
        );
    });
    assert!(cx.debug_bounds("queued-count-1").is_some());
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

/// Claude's queue strip: a card of its own over the box (44 high, 8 above
/// it), whose edit takes the latest back into the Composer as ↑ does and
/// whose send now stops the running turn so the queue goes out, as Escape
/// does.
#[gpui::test]
fn the_queue_strip_edits_and_sends_now(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("queue-strip-acts", 1);
    let thread = core.threads()[0];
    let _ = core.send(thread, "Working".into());
    assert!(core.queue(thread, "Held follow-up".into()).now().unwrap());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "working".into(),
        })
        .unwrap();
    tick(cx);
    let namespace = view.read_with(cx, |view, _| view.panes[0].text_namespace());
    let strip = cx
        .debug_bounds(format!("composer-queue-{namespace}").leak())
        .expect("the strip");
    let block = cx.debug_bounds("composer-block").expect("the box");
    assert_eq!(strip.size.height, px(ferrite_ui::theme::QUEUE_CARD_H));
    assert_eq!(
        block.top() - strip.bottom(),
        px(ferrite_ui::theme::QUEUE_CARD_GAP),
        "its own card, over the box"
    );
    assert_eq!(
        (strip.left(), strip.size.width),
        (block.left(), block.size.width)
    );

    let send_now = cx
        .debug_bounds("queued-send-now-0")
        .expect("send now while the turn runs");
    cx.simulate_click(send_now.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(*fake.interrupts.borrow(), 1, "send now is Escape's act");

    let edit = cx.debug_bounds("queued-edit-0").expect("edit");
    cx.simulate_click(edit.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.cockpit.read(cx).thread(thread).unwrap().queue().is_empty());
        assert_eq!(view.panes[0].composer.read(cx).text(), "Held follow-up");
    });
}

/// From the moment a prompt is sent — before the provider's first event —
/// the send circle is stop (`turn_running`), so a way to stop the turn is
/// in view while the working line already says the turn runs.
#[gpui::test]
fn a_sent_prompt_shows_stop_before_its_first_event(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("stop-on-send", 1);
    let thread = core.threads()[0];
    let _ = core.send(thread, "Go".into());
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    assert!(
        cx.debug_bounds("composer-stop").is_some(),
        "stop stands from the send"
    );
}

/// T3's primary actions: while a turn runs with something typed, stop (a
/// 28 circle) stands beside send (a 32 circle), so the operator can steer
/// or interrupt from the same row. Each does its own act.
#[gpui::test]
fn a_running_turn_with_text_shows_stop_beside_send(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("stop-beside-send", 1);
    let thread = core.threads()[0];
    let _ = core.send(thread, "Go".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    assert!(cx.debug_bounds("composer-stop").is_some(), "stop alone");
    assert!(cx.debug_bounds("composer-send").is_none(), "no send when empty");
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set("Also this".into(), cx));
    });
    tick(cx);
    let stop = cx.debug_bounds("composer-stop").expect("stop stays");
    let send = cx.debug_bounds("composer-send").expect("send joins it");
    assert!(stop.right() <= send.left(), "stop before send: {stop:?} / {send:?}");
    assert_eq!(stop.size.width, px(28.));
    assert_eq!(send.size.width, px(32.));
    cx.simulate_click(stop.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(*fake.interrupts.borrow(), 1, "stop interrupts");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes[0].composer.read(cx).text(), "Also this", "and keeps the text");
    });
}

/// T3's model trigger: the provider's mark leads the chip, then the model,
/// then the effort.
#[gpui::test]
fn the_model_chip_leads_with_its_providers_mark(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("model-chip-mark", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let chip = debug_bounds(cx, format!("model-picker-{}", thread.thread().get())).expect("the chip");
    let mark = debug_bounds(
        cx,
        format!("model-chip-{:?}-mark", PaneIdentity::Thread(thread)),
    )
    .expect("the provider's mark");
    assert!(chip.contains(&mark.center()), "{mark:?} in {chip:?}");
    assert_eq!(mark.size.width, px(ferrite_ui::theme::MODEL_PICKER_MARK));
    assert!(
        mark.left() - chip.left() <= px(ferrite_ui::theme::FOOTER_CHIP_PAD_X + 1.),
        "the mark leads: {mark:?} / {chip:?}"
    );
}

/// The effort chip's door, `/effort`, and the palette's `effort` open the
/// model picker with the keyboard in its Reasoning column, its cursor on
/// the level in force; the model chip opens it on the list.
#[gpui::test]
fn effort_opens_the_picker_on_its_reasoning_column(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("effort-opens-reasoning", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    cx.simulate_input("/effort");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    tick(cx);
    let column = |view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let picker = view.model_picker_view(cx).expect("the picker is open");
            (
                picker.column,
                picker.efforts.iter().position(|rung| rung.active),
                picker.effort_cursor,
            )
        })
    };
    let (at, active, cursor) = column(&view, cx);
    assert_eq!(at, Column::Reasoning, "/effort lands in the column");
    assert_eq!(Some(cursor), active, "on the level in force");
    assert!(cx.debug_bounds("model-picker-reasoning").is_some());
    view.update(cx, |view, cx| {
        view.close_popup(cx);
    });
    tick(cx);
    let chip = debug_bounds(cx, format!("model-picker-{}", thread.thread().get())).expect("the chip");
    cx.simulate_click(chip.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(column(&view, cx).0, Column::List, "the chip opens on the list");
}
