//! Layout acceptance from the UX audit: the keyboard and pointer on tool
//! disclosures.
use super::*;

#[gpui::test]
fn keyboard_disclosure_target_is_visible_across_three_targets_and_reverse(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("polish-disclosure-target", 1);
    let thread = core.threads()[0];
    let _ = core.send(thread, "Inspect output".into());
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
    // The turn's fold is the first stop: Enter opens it, and the target
    // then walks between it and the calls' group.
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

/// A pointer click only opens or closes a disclosure: it draws no keyboard
/// ring on the row and leaves the Composer holding the keyboard. The ring
/// is Tab's alone, and Tab still draws it after a click.
#[gpui::test]
fn clicking_a_disclosure_opens_it_without_a_keyboard_ring(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("polish-disclosure-click", 1);
    let thread = core.threads()[0];
    let _ = core.send(thread, "Inspect output".into());
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
    open_folds(&view, cx);
    let group = pane::DisclosureId::Group("clicked-a".into());
    let at = view.read_with(cx, |view, cx| {
        view.panes[0]
            .tool_bounds(group.clone(), cx)
            .unwrap()
            .center()
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
    // The transcript sits on its foot (SPEC §5.4): opening the group grew
    // it upward, so the second click finds the row where it now is.
    let at = view.read_with(cx, |view, cx| {
        view.panes[0]
            .tool_bounds(group.clone(), cx)
            .unwrap()
            .center()
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
    let _ = core.send(thread, "Inspect output".into());
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
    // The settled turn's fold is the first stop; Enter opens it, its work
    // in place under it.
    let prompt = view.read_with(cx, |view, cx| {
        view.cockpit.read(cx).thread(thread).unwrap().transcript().blocks()[0].id
    });
    let fold = pane::DisclosureId::Turn(prompt);
    cx.simulate_keystrokes("tab");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.panes[0].targeted_tool(), Some(&fold));
    });
    assert!(cx.debug_bounds("tool-group-keys-a").is_none(), "shut, the work folds away");
    cx.simulate_keystrokes("enter");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.panes[0].tool_expanded(fold.clone()), "Enter opens the fold");
    });
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
