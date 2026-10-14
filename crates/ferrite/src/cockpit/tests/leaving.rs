use super::*;

/// Nothing in flight: leaving goes at once. A working agent: the window
/// asks first, and Cancel keeps everything as it was.
#[gpui::test]
fn closing_while_an_agent_works_asks_first(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("leaving-asks", 1);
    let thread = core.threads()[0];
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.read_with(cx, |view, _| assert_eq!(view.leaving_warning(), None));
    assert!(view.update_in(cx, |view, window, cx| view.close_requested(window, cx)));
    assert!(!cx.has_pending_prompt(), "nothing to interrupt, nothing asked");

    view.update(cx, |view, _| view.cockpit.send(thread, "Run the checks".into()));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Running them.".into(),
        })
        .unwrap();
    tick(cx);
    let warning = view.read_with(cx, |view, _| view.leaving_warning());
    assert_eq!(
        warning.as_deref(),
        Some("1 agent is working. Leaving stops them; their Threads reopen next launch.")
    );
    assert!(!view.update_in(cx, |view, window, cx| view.close_requested(window, cx)));
    assert!(cx.has_pending_prompt());
    // A second ⌘Q while the question is up asks nothing more.
    view.update_in(cx, |view, window, cx| view.request_quit(window, cx));

    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    view.read_with(cx, |view, _| {
        assert!(!view.leave_asked, "the next ⌘Q asks again");
        assert_eq!(view.cockpit.threads(), vec![thread], "the Thread stays live");
    });
}

/// A Thread waiting on a Decision is counted as waiting, not working.
#[gpui::test]
fn a_pending_decision_is_named_as_waiting(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("leaving-decision", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    fake.streams.borrow()[0].send(decision("d1")).unwrap();
    tick(cx);

    let warning = view.read_with(cx, |view, _| view.leaving_warning()).unwrap();
    assert!(
        warning.starts_with("1 agent is waiting on a Decision."),
        "{warning}"
    );
}
