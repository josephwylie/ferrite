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
            content.size.height >= px(crate::theme::FS_MD * crate::theme::LINE_UI),
            "the question has a real scroll viewport: {content:?}"
        );
        assert!(content.top() >= island.top());
        let send = bounds(cx, format!("request-submit-{}-{serial}", thread.get()));
        assert!(
            send.top() - content.top() >= px(crate::theme::FS_MD * crate::theme::LINE_UI),
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
                    row.size.height >= px(crate::theme::FS_MONO * crate::theme::LINE_BODY - 1.),
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
