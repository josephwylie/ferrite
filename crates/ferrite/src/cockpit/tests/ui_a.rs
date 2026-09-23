//! WP-A's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// A copy sweep crosses a disclosed 40-line output and takes the prose
/// before it, the whole output and the prose after it (ADR 0006's logical
/// selection membership). Output stays inline below the byte cap for this:
/// the scrolling viewport owns its own selection.
#[gpui::test]
fn a_sweep_across_long_disclosed_output_copies_prose_output_and_prose(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("sweep-long-output", 1);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(1400.)));
    let output = (1..=40)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let stream = fake.streams.borrow();
    stream[0]
        .send(SessionEvent::TextDelta {
            text: "before\n\n".into(),
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::ToolStarted {
            id: "sweep".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "seq 40" }),
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::ToolCompleted {
            id: "sweep".into(),
            output: output.clone(),
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
    for disclosure in [
        pane::DisclosureId::Group("sweep".into()),
        pane::DisclosureId::Tool("sweep".into()),
    ] {
        let control = view.read_with(cx, |view, _| {
            view.panes[0]
                .tool_bounds(disclosure.clone())
                .unwrap()
                .center()
        });
        cx.simulate_click(control, gpui::Modifiers::none());
        tick(cx);
    }

    let from = caret(&view, cx, 0, 0);
    let mut to = caret(&view, cx, 2, "after".len() - 1);
    to.x += px(40.);
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    let copied = clipboard(cx).unwrap_or_default();
    assert!(copied.starts_with("before"), "{copied:?}");
    assert!(
        copied.contains(&output),
        "the whole output copies: {copied:?}"
    );
    assert!(copied.ends_with("after"), "{copied:?}");
    assert!(
        !copied.contains('$') && !copied.contains('⎿'),
        "the `$` and the elbows are chrome: {copied:?}"
    );
}

/// UI-14 in the new grammar: an answer that opens with a heading centres
/// its Ferrite mark on the heading's own line box, not the prose line's.
#[gpui::test]
fn the_answer_mark_centres_on_a_leading_headings_line_box(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("answer-mark-heading", 1);
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "# Heading one\n\nBody text under it.\n\n".into(),
        })
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    tick(cx);
    let mark = debug_bounds(cx, "answer-mark".into()).unwrap();
    let answer = debug_bounds(cx, "transcript-answer".into()).unwrap();
    let heading = view.read_with(cx, |view, cx| {
        let thread = view.panes[0].thread().unwrap();
        let block = &view.cockpit.thread(thread).unwrap().transcript().blocks()[0];
        let id = format!(
            "markdown-{}-{:?}",
            view.panes[0].text_namespace(),
            block.markdown_run.unwrap_or(block.id)
        );
        crate::rich::testing::bounds(&id, 0, cx).unwrap()
    });
    let line = crate::rich::heading_line_height(1, crate::theme::FS_PROSE);
    assert!(
        (mark.top() + mark.size.height / 2. - (answer.top() + px(line / 2.))).abs() <= px(1.),
        "the mark centres on the heading's {line}px line box: {mark:?} / {answer:?}"
    );
    assert!(
        (heading.top() - answer.top()).abs() <= px(1.),
        "the leading heading carries no top margin to throw the mark off: {heading:?}"
    );
}
