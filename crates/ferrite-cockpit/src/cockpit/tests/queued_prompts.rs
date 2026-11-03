//! Prompts queued behind a running turn.
#[allow(unused_imports)]
use super::*;

/// Two prompts sent while the turn runs: Solo's queue strip counts them
/// (`2 messages queued`) and shows the top of the pile — the latest, what
/// ↑ takes back and ⌫ drops (Claude's strip).
#[gpui::test]
fn queued_prompts_pile_up_with_the_latest_on_top(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("queued-pile", 1);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("enter", Submit, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "working".into(),
        })
        .unwrap();
    tick(cx);
    cx.simulate_input("first thing");
    cx.simulate_keystrokes("enter");
    tick(cx);
    cx.simulate_input("second thing");
    cx.simulate_keystrokes("enter");
    tick(cx);
    view.read_with(cx, |view, _| {
        let open = view.cockpit.thread(thread).unwrap();
        assert_eq!(open.queued_all(), ["second thing", "first thing"]);
        assert_eq!(open.queued(), Some("second thing"));
    });
    let top = cx
        .debug_bounds("queued-0")
        .expect("the latest prompt is drawn");
    let count = cx
        .debug_bounds("queued-count-2")
        .expect("the strip counts both");
    assert!(top.left() >= count.right(), "{top:?} follows {count:?}");
    assert!(cx.debug_bounds("queued-1").is_none(), "one row, not a pile");
}

/// The queued row's `⌫ unqueue` hint is a real key: Backspace on an
/// empty Composer line clears the held prompt, while with text on the
/// line it stays an editing key and the queue survives.
#[gpui::test]
fn backspace_on_an_empty_line_unqueues_the_held_prompt(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("unqueue-key", 1);
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("enter", Submit, None),
            KeyBinding::new("backspace", ferrite_pane::composer::Backspace, None),
            KeyBinding::new("up", HistoryOlder, Some("ComposerHistory")),
            KeyBinding::new("up", ferrite_pane::composer::Up, Some("Composer")),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    // A streaming turn makes the Session busy; the next prompt queues.
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "working".into(),
        })
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.cockpit.thread(thread).is_some_and(|open| open.busy()),
            "the premise: a turn in flight"
        );
    });
    cx.simulate_input("also this");
    cx.simulate_keystrokes("enter");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.thread(thread).and_then(|open| open.queued()),
            Some("also this")
        );
    });

    // With text on the line, Backspace edits; the queue is untouched.
    cx.simulate_input("dr");
    cx.simulate_keystrokes("backspace");
    view.read_with(cx, |view, cx| {
        assert!(
            !view.panes[0].composer.read(cx).is_empty(),
            "backspace with text is still an editing key"
        );
        assert_eq!(
            view.cockpit.thread(thread).and_then(|open| open.queued()),
            Some("also this")
        );
    });

    // Emptied, the next Backspace is the advertised ⌫ unqueue.
    cx.simulate_keystrokes("backspace");
    view.read_with(cx, |view, cx| {
        assert!(view.panes[0].composer.read(cx).is_empty());
        assert_eq!(
            view.cockpit.thread(thread).and_then(|open| open.queued()),
            Some("also this")
        );
    });
    cx.simulate_keystrokes("backspace");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.thread(thread).and_then(|open| open.queued()),
            None,
            "backspace on the empty line unqueues the held prompt"
        );
    });

    // Enter waits for cancellation and preserves edits/attachments made
    // while its receipt is in flight.
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.set(
                ferrite_core::prompt_files::compose(
                    "held",
                    &[std::path::PathBuf::from("/tmp/held.png")],
                ),
                cx,
            )
        });
    });
    cx.simulate_keystrokes("enter");
    tick(cx);
    cx.simulate_keystrokes("enter");
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.set(
                ferrite_core::prompt_files::compose(
                    "new draft",
                    &[std::path::PathBuf::from("/tmp/new.png")],
                ),
                cx,
            )
        });
    });
    tick(cx);
    view.read_with(cx, |view, cx| {
        let (text, files) =
            ferrite_core::prompt_files::split(view.panes[0].composer.read(cx).prompt());
        assert_eq!(text, "new draft\n\nheld");
        assert_eq!(
            files,
            [
                std::path::PathBuf::from("/tmp/new.png"),
                std::path::PathBuf::from("/tmp/held.png")
            ]
        );
    });

    // Up walks rows first, then edits the visible newest native prompt.
    view.update(cx, |view, cx| {
        view.cockpit.queue(thread, "earlier queued".into());
        view.cockpit.queue(thread, "edit queued".into());
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.set("draft\nsecond row".into(), cx)
        });
    });
    tick(cx);
    cx.simulate_keystrokes("up");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.cockpit.thread(thread).unwrap().queued(),
            Some("edit queued")
        );
        assert_eq!(view.panes[0].composer.read(cx).text(), "draft\nsecond row");
    });
    cx.simulate_keystrokes("up");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.cockpit.thread(thread).unwrap().queued(),
            Some("earlier queued")
        );
        assert_eq!(
            view.panes[0].composer.read(cx).text(),
            "edit queued\n\ndraft\nsecond row"
        );
    });
}
