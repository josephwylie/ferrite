//! Pointer work in the transcript: focus, scroll and text selection.
#[allow(unused_imports)]
use super::*;

/// #15 AC1: a click lands the operator on a Pane — the focus ring moves,
/// and the keyboard follows it into that Pane's Composer.
#[gpui::test]
fn clicking_a_pane_focuses_it_and_the_keyboard_follows(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("click-focus", 2);
    let group = group_all(&mut core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    // Two Panes side by side, each big enough to hold a Composer even
    // with the 208px nav (#21) taken off the left.
    cx.simulate_resize(gpui::size(px(1800.), px(600.)));
    tick(cx);
    view.read_with(cx, |view, _| assert_eq!(view.focused(), 0));

    cx.simulate_click(gpui::point(px(1200.), px(300.)), gpui::Modifiers::none());

    view.read_with(cx, |view, _| {
        assert_eq!(view.focused(), 1, "the click moved the focus ring");
    });

    cx.simulate_input("hi");

    let (left, right) = view.update(cx, |view, cx| {
        (
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.take(cx)),
            view.panes[1]
                .composer
                .update(cx, |composer, cx| composer.take(cx)),
        )
    });
    assert_eq!(right, "hi", "typing lands in the clicked Pane");
    assert_eq!(left, "", "and nowhere else");
}

/// #15 AC4: wheel-scrolling into history detaches from the tail — new
/// Blocks must not yank the reader down — and scrolling back to the
/// bottom reattaches tail-follow.
#[gpui::test]
fn wheel_scroll_detaches_from_the_tail_and_scrolling_back_reattaches(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("scroll-detach", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let say = |line: usize| {
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: format!("history line {line:03}\n\n"),
            })
            .unwrap();
    };
    for line in 0..80 {
        say(line);
    }
    tick(cx);
    let (offset, max) = view.read_with(cx, |view, cx| {
        let transcript = view.panes[0].transcript().unwrap();
        let scroll = transcript.read(cx).scroll();
        (scroll.offset().y, scroll.max_offset().y)
    });
    assert!(max > px(0.), "the transcript must overflow for this test");
    assert!(
        offset + max <= TAIL_SLACK,
        "streaming keeps the tail: {offset:?} against {max:?}"
    );

    // One wheel gesture up: the operator is reading history now.
    let wheel = |cx: &mut gpui::VisualTestContext, dy: f32| {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(px(500.), px(350.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(dy))),
            modifiers: gpui::Modifiers::none(),
            touch_phase: gpui::TouchPhase::default(),
        });
    };
    wheel(cx, 120.);
    let held = view.read_with(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .read(cx)
            .scroll()
            .offset()
            .y
    });
    assert!(held > offset, "wheel up must move the view: {held:?}");

    for line in 80..100 {
        say(line);
    }
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.panes[0]
                .transcript()
                .unwrap()
                .read(cx)
                .scroll()
                .offset()
                .y,
            held,
            "new Blocks must not yank a reader down"
        );
    });

    // Back to the bottom: the tail is theirs again.
    wheel(cx, -100000.);
    for line in 100..110 {
        say(line);
    }
    tick(cx);
    view.read_with(cx, |view, cx| {
        let transcript = view.panes[0].transcript().unwrap();
        let scroll = transcript.read(cx).scroll();
        let gap = scroll.max_offset().y + scroll.offset().y;
        assert!(
            gap <= TAIL_SLACK,
            "scrolling to the bottom reattaches the tail: {gap:?}"
        );
    });
}

/// A Thread opens on its tail. A fresh ScrollHandle sits at the top, so
/// a reopened Thread with history landed on its oldest line; it must
/// land on its newest, and keep following the tail from there.
#[gpui::test]
fn a_reopened_thread_opens_at_the_bottom_and_keeps_following_the_tail(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("reopen-at-tail", 2);
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-w", CloseThread, None),
            KeyBinding::new("cmd-o", ReopenThread, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    // The reopened Thread's agent starts when the operator writes to it
    // (ADR 0032); it then streams on the newest Sender, not the one it
    // was parked with.
    let say = |line: usize| {
        fake.streams
            .borrow()
            .last()
            .expect("a spawned Session")
            .send(SessionEvent::TextDelta {
                text: format!("history line {line:03}\n\n"),
            })
            .unwrap();
    };
    // Stream into the second-spawned Session: that is `panes[0]`'s only
    // if spawn order matches grid order, so pick the Pane by its stream
    // instead — the last spawn is the last Thread created.
    for line in 0..80 {
        say(line);
    }
    tick(cx);
    let closed = view.read_with(cx, |view, _| {
        view.panes
            .iter()
            .filter_map(|pane| pane.thread())
            .max()
            .unwrap()
    });
    let at = view.read_with(cx, |view, _| {
        view.panes
            .iter()
            .position(|pane| pane.thread() == Some(closed))
            .unwrap()
    });
    view.update(cx, |view, _| view.focus_pane(at));
    cx.simulate_keystrokes("cmd-w");
    view.read_with(cx, |view, _| assert_eq!(view.panes.len(), 1));

    cx.simulate_keystrokes("cmd-o");
    tick(cx);
    let gap = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let pane = view
                .panes
                .iter()
                .find(|pane| pane.thread() == Some(closed))
                .expect("the reopened Pane");
            let transcript = pane.transcript().unwrap();
            let scroll = transcript.read(cx).scroll();
            (
                scroll.max_offset().y,
                scroll.max_offset().y + scroll.offset().y,
            )
        })
    };
    let (max, at_open) = gap(cx);
    assert!(max > px(0.), "the transcript must overflow for this test");
    assert!(
        at_open <= TAIL_SLACK,
        "a reopened Thread opens at its bottom, not its top: {at_open:?} of {max:?}"
    );

    // Streaming into the reopened Pane keeps it on the tail.
    view.update(cx, |view, _| view.cockpit.set_composing(closed, true));
    tick(cx);
    for line in 80..100 {
        say(line);
    }
    tick(cx);
    let (later_max, after_stream) = gap(cx);
    assert!(later_max > max, "more history grew the transcript");
    assert!(
        after_stream <= TAIL_SLACK,
        "new Blocks keep a tail-riding reader at the bottom: {after_stream:?}"
    );
}

/// #15 AC2 at character grain (#27): a mid-word press sweeps exact
/// characters across Blocks, and cmd-c puts exactly the highlighted
/// text on the clipboard — endpoint rows cut at their characters, the
/// middle row whole, rows joined with honest newlines. A plain click
/// selects nothing and leaves the clipboard alone.
#[gpui::test]
fn a_drag_selects_characters_and_the_copy_key_takes_exactly_them(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("select-copy", 1);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "alpha\n\nbravo  two   three\t界 e\u{301}\u{a0}fin\n\ncharlie\n\n".into(),
        })
        .unwrap();
    tick(cx);
    // "al|pha" down to "char|lie".
    let from = caret(&view, cx, 0, 2);
    let to = caret(&view, cx, 2, 4);

    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(
        clipboard(cx).as_deref(),
        Some("pha\nbravo  two   three\t界 e\u{301}\u{a0}fin\nchar")
    );

    // A plain click clears the selection; copying then changes nothing.
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("kept".into())));
    cx.simulate_click(from, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some("kept"));
}

/// A native window resize can consume the pointer release after its
/// initial press reached the window. Once the pointer reports that no
/// button is held, transcript selection must not keep following it.
#[gpui::test]
fn resizing_the_window_cannot_leave_text_selection_dragging(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("resize-selection", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "alpha\n\nbravo\n\ncharlie\n\n".into(),
        })
        .unwrap();
    tick(cx);

    let over_text = caret(&view, cx, 1, 3);
    let resize_edge = gpui::point(px(999.), over_text.y);
    cx.simulate_mouse_down(
        resize_edge,
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    cx.simulate_resize(gpui::size(px(900.), px(700.)));
    cx.simulate_mouse_move(over_text, None, gpui::Modifiers::none());

    cx.update(|window, cx| {
        assert!(
            !gpui::base::TextSelection::has_selection(window, cx),
            "a lost resize release must not turn later pointer motion into selection"
        );
    });
}

#[gpui::test]
fn a_plain_transcript_click_does_not_move_the_viewport(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("click-keeps-scroll", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: (0..80)
                .map(|line| format!("history line {line:03}\n\n"))
                .collect(),
        })
        .unwrap();
    tick(cx);

    let clicked = view.update(cx, |view, cx| {
        let pane = &mut view.panes[0];
        let transcript = pane.transcript().unwrap();
        transcript.update(cx, |transcript, _| {
            transcript.scroll().pause_following_tail();
            let offset = gpui::point(px(0.), -transcript.scroll().max_offset().y / 2.);
            transcript.scroll().set_offset(offset);
        });
        cx.notify();
        let bounds = transcript.read(cx).scroll().bounds();
        gpui::point(bounds.center().x, bounds.bottom() - px(2.))
    });
    cx.run_until_parked();
    let before = view.read_with(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .read(cx)
            .scroll()
            .offset()
    });

    let titlebar = gpui::point(px(500.), px(ferrite_ui::theme::WIN_CHROME_H / 2.));
    cx.simulate_mouse_down(titlebar, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(titlebar, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_click(clicked, gpui::Modifiers::none());
    cx.simulate_mouse_move(clicked, MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();

    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.panes[0]
                .transcript()
                .unwrap()
                .read(cx)
                .scroll()
                .offset(),
            before
        );
    });
}

/// Native selection: double-click takes a word; triple-click a paragraph.
#[gpui::test]
fn double_click_selects_the_word_and_triple_click_the_line(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("select-clicks", 1);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "make it fast\n\nkeep it honest\n\n".into(),
        })
        .unwrap();
    tick(cx);

    // Double-click lands mid "make" and takes the whole word.
    let on_make = caret(&view, cx, 0, 2);
    cx.simulate_click(on_make, gpui::Modifiers::none());
    press(cx, on_make, 2);
    cx.simulate_mouse_up(on_make, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some("make"));

    // Triple-click takes the rendered run whole.
    let on_honest = caret(&view, cx, 1, 10);
    press(cx, on_honest, 3);
    cx.simulate_mouse_up(on_honest, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some("keep it honest"));
}

/// #27 fix round: chrome is not selectable ground. A double-click in
/// the Composer region selects nothing — a press anchors only inside
/// the transcript body — and a drag that leaves through the bottom
/// edge clamps into the body, selecting to the last row instead of
/// freezing short or grabbing chrome.
#[gpui::test]
fn presses_on_chrome_select_nothing_and_a_drag_out_the_bottom_clamps(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("select-chrome", 1);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "alpha\n\nbravo\n\ncharlie\n\n".into(),
        })
        .unwrap();
    tick(cx);
    let body = view.read_with(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .read(cx)
            .scroll()
            .bounds()
    });

    // A double-click below the body — the Composer region — must not
    // light up a word in the nearest transcript row.
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("kept".into())));
    let on_chrome = gpui::point(body.center().x, body.bottom() + px(20.));
    cx.simulate_click(on_chrome, gpui::Modifiers::none());
    press(cx, on_chrome, 2);
    cx.simulate_mouse_up(on_chrome, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some("kept"));

    // A drag that exits through the bottom edge keeps extending —
    // moves ride the root — and clamps to the body's boundary: the
    // sweep reaches the end of the last row, never the Composer.
    let from = caret(&view, cx, 0, 2);
    let out = gpui::point(body.right() - px(10.), body.bottom() + px(60.));
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(out, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(out, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some("pha\nbravo\ncharlie"));
}

/// #27: copy is what you see. A sweep across a shut tool row takes its
/// call line as one line — never its icon, its code chip's markup, a
/// duration, or the output it folds away.
#[gpui::test]
fn a_sweep_across_a_tool_row_copies_its_text_and_never_its_chrome(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("select-tool-row", 1);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let stream = fake.streams.borrow();
    stream[0]
        .send(SessionEvent::TextDelta {
            text: "before\n\n".into(),
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::ToolStarted {
            id: "toolu_9".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "echo hi" }),
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::ToolCompleted {
            id: "toolu_9".into(),
            output: "done".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::TextDelta {
            text: "after\n\n".into(),
        })
        .unwrap();
    drop(stream);
    tick(cx);

    let from = caret(&view, cx, 0, 0);
    let mut to = caret(&view, cx, 2, "after".len() - 1);
    to.x += px(40.);
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(
        clipboard(cx).as_deref(),
        Some("before\n\nRan echo hi\n\nafter"),
        "copy the call line as it reads; omit the icon and the trail"
    );
}

/// A selection's Reply pill is offered once a drag has selected a passage,
/// in the focused transcript.
fn offered(view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> bool {
    view.read_with(cx, |view, cx| {
        view.panes[0].transcript().unwrap().read(cx).reply_offered()
    })
}

/// Drag from block `from`'s byte to block `to`'s, and let go.
fn drag(
    view: &gpui::Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
    from: (usize, usize),
    to: (usize, usize),
) -> (gpui::Point<gpui::Pixels>, gpui::Point<gpui::Pixels>) {
    let from = caret(view, cx, from.0, from.1);
    let to = caret(view, cx, to.0, to.1);
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    (from, to)
}

/// A selection's Reply (Claude's): a drag offers the pill centred over the
/// selection; a press on it adds the passage as a Quote row over the
/// Composer (the draft's words untouched), clears the selection and hands
/// the Composer the keyboard. Quotes stack, each row's × removes it, and
/// the sent prompt carries them ahead of its words as block quotes, its
/// bubble drawing the same rows.
#[gpui::test]
fn a_selections_reply_quotes_it_over_the_draft_and_sends_it(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("selection-reply", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "alpha\n\nbravo\n\ncharlie\n\n".into(),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set("Look:".into(), cx));
    });

    // A plain click selects nothing and offers nothing.
    let at = caret(&view, cx, 0, 2);
    cx.simulate_click(at, gpui::Modifiers::none());
    tick(cx);
    assert!(!offered(&view, cx));

    // "al|ph|a" on one line: the pill centres over the run, above it.
    let (from, to) = drag(&view, cx, (0, 2), (0, 4));
    assert!(offered(&view, cx), "the drag offers its Reply");
    let pill = cx.debug_bounds("selection-reply").expect("the pill draws");
    assert!(
        (pill.center().x - (from.x + to.x) / 2.).abs() < px(1.),
        "centred over the selection: {pill:?} against {from:?}..{to:?}"
    );
    assert!(
        pill.bottom() < from.y,
        "above it: {pill:?} against {from:?}"
    );

    cx.simulate_click(pill.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(!offered(&view, cx));
    cx.update(|window, cx| {
        assert!(!gpui::base::TextSelection::has_selection(window, cx));
        let composer = view.read(cx).panes[0].composer.clone();
        assert!(
            composer
                .read(cx)
                .focus_target(window, cx)
                .is_focused(window),
            "the Composer takes the keyboard"
        );
    });

    // A second passage, across lines, stacks under the first.
    drag(&view, cx, (1, 3), (2, 4));
    let pill = cx.debug_bounds("selection-reply").expect("the pill draws");
    cx.simulate_click(pill.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        let composer = view.panes[0].composer.read(cx);
        assert_eq!(composer.text(), "Look:", "the draft's words stay as typed");
        assert_eq!(composer.quotes(), ["ph", "vo\nchar"]);
    });
    assert!(cx.debug_bounds("quote-row-0").is_some());
    assert!(cx.debug_bounds("quote-row-1").is_some());

    // Each row's × takes that quote alone.
    let remove = cx
        .debug_bounds("remove-quote-0")
        .expect("a draft's row removes");
    cx.simulate_click(remove.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes[0].composer.read(cx).quotes(), ["vo\nchar"]);
    });

    // The send carries the quote ahead of the words, as block quotes (the
    // ×'s press handed the line the keyboard back).
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        fake.sent.borrow().last().map(String::as_str),
        Some("> vo\n> char\n\nLook:")
    );
    view.read_with(cx, |view, cx| {
        let composer = view.panes[0].composer.read(cx);
        assert!(composer.quotes().is_empty() && composer.text().is_empty());
    });
    assert!(
        cx.debug_bounds("quote-row-0").is_some(),
        "the sent prompt draws its Quote row"
    );
    assert!(cx.debug_bounds("remove-quote-0").is_none(), "with no ×");
}

/// The pill goes when its selection does (a press away), and fades out
/// on a scroll. There is no esc:
/// Claude's has none, and esc keeps its Interrupt.
#[gpui::test]
fn a_selections_reply_goes_with_its_selection_and_on_a_scroll(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("selection-reply-dismiss", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    for line in 0..60 {
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: format!("line {line:03}\n\n"),
            })
            .unwrap();
    }
    tick(cx);

    drag(&view, cx, (58, 1), (59, 4));
    assert!(offered(&view, cx));
    let away = caret(&view, cx, 57, 2);
    cx.simulate_click(away, gpui::Modifiers::none());
    tick(cx);
    assert!(
        !offered(&view, cx),
        "a press away clears the selection and the pill"
    );

    drag(&view, cx, (58, 1), (59, 4));
    assert!(offered(&view, cx));
    let wheel = |cx: &mut gpui::VisualTestContext, dy: f32| {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(px(500.), px(350.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(dy))),
            modifiers: gpui::Modifiers::none(),
            touch_phase: gpui::TouchPhase::default(),
        });
        tick(cx);
    };
    // A scroll fades it out, then it goes (at once here: the suite runs
    // under reduced motion, `motion::live`).
    wheel(cx, 120.);
    cx.executor()
        .advance_clock(ferrite_ui::motion::FADE_QUICK.duration() * 2);
    tick(cx);
    assert!(!offered(&view, cx), "a scroll dismisses it");
    assert!(cx.debug_bounds("selection-reply").is_none());
}

/// PageUp and PageDown step the transcript a page from an empty Composer
/// line; Home and End reach its top and its live tail.
#[gpui::test]
fn page_keys_scroll_the_transcript_from_an_empty_line(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("page-keys", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: (0..80)
                .map(|line| format!("history line {line:03}\n\n"))
                .collect(),
        })
        .unwrap();
    tick(cx);
    let scroll = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let transcript = view.panes[0].transcript().unwrap().read(cx);
            (
                transcript.scroll().offset().y,
                transcript.scroll().is_following_tail(),
            )
        })
    };
    let (tail, following) = scroll(cx);
    assert!(
        following && tail < px(0.),
        "at the tail of a long transcript"
    );
    cx.simulate_keystrokes("pageup");
    cx.run_until_parked();
    let (up, following) = scroll(cx);
    assert!(up > tail && !following, "a page up: {tail:?} → {up:?}");
    cx.simulate_keystrokes("pagedown");
    cx.run_until_parked();
    assert_eq!(scroll(cx).0, tail, "and a page back down");
    cx.simulate_keystrokes("home");
    cx.run_until_parked();
    assert_eq!(scroll(cx).0, px(0.));
    cx.simulate_keystrokes("end");
    cx.run_until_parked();
    assert!(scroll(cx).1, "End follows the tail again");
}
