//! How transcript rows draw: reasoning, tool output and the prompt echo.
#[allow(unused_imports)]
use super::*;

#[gpui::test]
fn short_reasoning_has_no_empty_disclosure(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("reasoning-short", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::ThinkingDelta {
            text: "Checking the files.".into(),
        })
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, cx| {
        let pane = &view.panes[0];
        let block = &view
            .cockpit
            .thread(pane.thread().unwrap())
            .unwrap()
            .transcript()
            .blocks()[0];
        assert!(
            pane.tool_bounds(pane::DisclosureId::Reasoning(block.id), cx)
                .is_none(),
            "a short summary must not offer an expansion that repeats itself"
        );
    });
}

#[gpui::test]
fn the_prompt_being_read_copies_and_resends_from_commands(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("sent-prompt-actions", 1);
    let thread = core.threads()[0];
    core.send(thread, "Run the focused checks".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);

    // The pointer over the bubble shows its send time and no actions.
    let prompt = cx.debug_bounds("transcript-prompt").unwrap();
    cx.simulate_mouse_move(prompt.center(), None, gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("prompt-action-copy").is_none());
    assert!(cx.debug_bounds("prompt-action-resend").is_none());

    assert!(view.update(cx, |view, cx| view.copy_reading_prompt(0, cx)));
    assert_eq!(clipboard(cx).as_deref(), Some("Run the focused checks"));
    assert!(view.update(cx, |view, cx| view.resend_reading_prompt(0, cx)));
    assert_eq!(
        fake.sent.borrow().as_slice(),
        ["Run the focused checks", "Run the focused checks"]
    );
}

#[gpui::test]
fn reasoning_discloses_body_without_repeating_its_heading(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("reasoning-details", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new_thinking(core, cx));
    hide_panel(&view, cx);
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let body = "The provider supplied these additional details.";
    fake.streams.borrow()[0]
        .send(SessionEvent::ReasoningSummaryPart {
            item_id: "reasoning-1".into(),
            summary_index: 0,
            text: format!("**Checking files**\n\n{body}"),
            snapshot: false,
        })
        .unwrap();
    tick(cx);
    open_live(cx);
    let id = view.read_with(cx, |view, _| {
        view.cockpit
            .thread(view.panes[0].thread().unwrap())
            .unwrap()
            .transcript()
            .blocks()[0]
            .id
    });
    let summary = cx.debug_bounds("reasoning-summary").unwrap();
    cx.simulate_click(summary.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        let text = view.selection.registered(view.panes[0].thread().unwrap());
        assert!(
            text.iter()
                .any(|(block, _, _, text)| *block == id && text == body),
            "expanded reasoning must contain the body without duplicating its heading: {text:?}"
        );
    });
    for width in [1000., 740.] {
        cx.simulate_resize(gpui::size(px(width), px(700.)));
        tick(cx);
        let summary = cx.debug_bounds("reasoning-summary").unwrap();
        let control = view.read_with(cx, |view, cx| {
            view.panes[0]
                .tool_bounds(pane::DisclosureId::Reasoning(id), cx)
                .unwrap()
        });
        assert!(
            summary.size.width < px(160.),
            "a short summary must size to its text: {summary:?}"
        );
        // The toggle leads the header, its summary after it.
        assert!(
            control.left() <= summary.left() + px(0.5),
            "the header's toggle leads its summary: {summary:?} / {control:?}"
        );
    }

    fake.streams.borrow()[0]
        .send(SessionEvent::ReasoningSummaryPart {
            item_id: "reasoning-1".into(),
            summary_index: 0,
            text: " More streamed detail.".into(),
            snapshot: false,
        })
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        let text = view.selection.registered(view.panes[0].thread().unwrap());
        assert!(
            text.iter().any(|(block, _, _, text)| *block == id
                && text == &format!("{body} More streamed detail.")),
            "streaming must preserve the open disclosure and append its body: {text:?}"
        );
    });
}

#[gpui::test]
fn transcript_details_wrap_to_the_pane(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("transcript-wrap", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new_thinking(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(1000.)));
    let text = "Inspecting the provider transcript and preserving all the details. ".repeat(8);
    fake.streams.borrow()[0]
        .send(SessionEvent::ThinkingDelta { text: text.clone() })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: "wrap-tool".into(),
            name: "Read".into(),
            input: serde_json::json!({"file_path": format!("/workspace/{}", "long_filename".repeat(40))}),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolCompleted {
            id: "wrap-tool".into(),
            output: text.clone(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        })
        .unwrap();
    tick(cx);
    open_live(cx);
    let reasoning = view.read_with(cx, |view, cx| {
        let pane = &view.panes[0];
        let thread = pane.thread().unwrap();
        assert!(
            !view
                .selection
                .registered(thread)
                .iter()
                .any(|(_, _, _, value)| value == &text),
            "reasoning starts collapsed"
        );
        let id = view
            .cockpit
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .find(|b| matches!(&b.body, Body::Thinking(_)))
            .unwrap()
            .id;
        pane.tool_bounds(pane::DisclosureId::Reasoning(id), cx)
            .unwrap()
            .center()
    });
    cx.simulate_click(reasoning, gpui::Modifiers::none());
    tick(cx);
    view.update(cx, |view, cx| {
        view.panes[0].toggle_tool(&pane::DisclosureId::Tool("wrap-tool".into()));
        cx.notify();
    });
    tick(cx);
    // Tall enough that every row stays in view (a row scrolled out keeps
    // its last layout).
    for width in [1000., 740.] {
        cx.simulate_resize(gpui::size(px(width), px(1600.)));
        tick(cx);
        view.read_with(cx, |view, cx| {
            let pane = &view.panes[0];
            let viewport = pane.transcript().unwrap().read(cx).scroll().bounds();
            let transcript = view
                .cockpit
                .thread(pane.thread().unwrap())
                .unwrap()
                .transcript();
            for block in transcript.blocks() {
                let ids = match &block.body {
                    Body::Thinking(_) => {
                        vec![format!("thinking-{}-{:?}", pane.text_namespace(), block.id)]
                    }
                    // Its call line is one Markdown run; its disclosed
                    // input and output the literals after it.
                    Body::Tool(_) => (0..=1)
                        .map(|ordinal| {
                            format!("literal-{}-{:?}-{ordinal}", pane.text_namespace(), block.id)
                        })
                        .collect(),
                    _ => continue,
                };
                for id in ids {
                    let bounds = ferrite_pane::rich::testing::bounds(&id, 0, cx).unwrap();
                    assert!(
                        bounds.right() <= viewport.right(),
                        "text exceeds pane: {id} {bounds:?} / {viewport:?}"
                    );
                    assert!(
                        bounds.size.height > px(30.),
                        "long text must wrap: {id} {bounds:?}"
                    );
                }
            }
        });
    }
}

/// The prompt echo reads at the answer's size at every reading size
/// (14/21, 16/24, 18/27): the turn's heading is set apart by its band,
/// its `❯` and its ink, never by being smaller than the answer under it.
/// Terminal-native: the band is the answer's line with half a line
/// above and below.
#[gpui::test]
fn the_prompt_echo_reads_at_the_answer_size_at_every_reading_size(cx: &mut TestAppContext) {
    use ferrite_core::settings::ReadingSize;
    let (mut core, fake) = cockpit("echo-reading-size", 1);
    let thread = core.threads()[0];
    core.send(thread, "Check the build".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "The build passed.".into(),
        })
        .unwrap();
    tick(cx);
    // Each step changes the size, so the transcript re-renders (a cached
    // view registers no debug bounds).
    for size in [
        ReadingSize::nearest(18),
        ReadingSize::nearest(16),
        ReadingSize::STANDARD,
    ] {
        view.update(cx, |view, cx| {
            view.prefs.settings.reading_size = size;
            cx.notify();
        });
        tick(cx);
        let prompt = cx.debug_bounds("transcript-prompt").unwrap();
        let line = ferrite_ui::theme::prose_line_height(ferrite_ui::theme::tx_size(size, true));
        assert_eq!(
            prompt.size.height,
            px(line + 2. * ferrite_ui::theme::PROMPT_BUBBLE_PAD_Y),
            "{size:?}: the echo sits on the answer's line in its bubble"
        );
        let echo = view.read_with(cx, |view, cx| {
            let pane = &view.panes[0];
            let block = view
                .cockpit
                .thread(thread)
                .unwrap()
                .transcript()
                .blocks()
                .iter()
                .find(|block| matches!(block.body, Body::Prompt(_)))
                .unwrap()
                .id;
            ferrite_pane::rich::testing::font_size(
                &format!("literal-{}-{block:?}", pane.text_namespace()),
                cx,
            )
            .unwrap()
        });
        assert_eq!(
            echo,
            px(ferrite_ui::theme::tx_size(size, true)),
            "{size:?}: the echo is set at the answer's size"
        );
    }
}

#[gpui::test]
fn transcript_rows_share_one_content_edge_and_the_gap_table(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("transcript-spacing", 1);
    let thread = core.threads()[0];
    core.send(thread, "Check the build".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    for index in 0..2 {
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolStarted {
                id: format!("spacing-{index}"),
                name: "commandExecution".into(),
                input: serde_json::json!({"command": "cargo check"}),
            })
            .unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolCompleted {
                id: format!("spacing-{index}"),
                output: "ok".into(),
                is_error: false,
                result: ferrite_core::ToolResult::Opaque,
            })
            .unwrap();
    }
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "The build passed.\n\n".into(),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    // The settled turn's fold heads it, shut, its work folded away; the
    // answer stands its padding under the fold's rule. A press opens it.
    assert!(cx.debug_bounds("turn-stamp").is_none(), "no stamp at the end");
    assert!(cx.debug_bounds("tool-group-spacing-0").is_none());
    let fold = cx.debug_bounds("turn-fold-words").expect("the turn's fold");
    cx.simulate_click(fold.center(), gpui::Modifiers::none());
    for width in [1000., 720.] {
        cx.simulate_resize(gpui::size(px(width), px(700.)));
        tick(cx);
        let prompt = cx.debug_bounds("transcript-prompt").unwrap();
        let fold = cx.debug_bounds("turn-fold").unwrap();
        let tools = cx.debug_bounds("tool-group-spacing-0").unwrap();
        let answer = cx.debug_bounds("transcript-answer").unwrap();
        // The fold stands the prompt step under the bubble, the opened work
        // its padding under the fold's rule; consecutive calls fold into
        // one summary line.
        let line = pane::Grid::pane(Default::default(), true).line;
        assert_eq!(
            fold.top() - prompt.bottom(),
            px(ferrite_ui::theme::tx_after_prompt(line) + ferrite_ui::theme::TURN_FOLD_PAD_T)
        );
        let rule = fold.bottom() + px(ferrite_ui::theme::TURN_FOLD_TEXT_B + 1.);
        assert_eq!(tools.top() - rule, px(ferrite_ui::theme::TURN_FOLD_PAD_B));
        assert!(
            cx.debug_bounds("tool-row-spacing-1").is_none(),
            "a shut group draws no call rows"
        );
        // The turn's last prose is its answer, the prose step under the
        // work.
        assert_eq!(
            answer.top() - tools.bottom(),
            px(ferrite_ui::theme::tx_work_prose(line))
        );
        // Its actions stand under it, the first icon's ink on the prose's
        // edge.
        let actions = cx.debug_bounds("answer-actions").unwrap();
        assert_eq!(
            actions.top() - answer.bottom(),
            px(ferrite_ui::theme::MESSAGE_ACTIONS_GAP)
        );
        use ferrite_ui::theme::{MESSAGE_ACTION, MESSAGE_ACTION_GLYPH};
        assert_eq!(
            actions.left(),
            answer.left() - px((MESSAGE_ACTION - MESSAGE_ACTION_GLYPH) / 2.)
        );
        // One content edge: the group's summary and the answer's prose
        // start on the content column (the prompt's bubble keeps to the
        // right).
        let tools_start = caret(&view, cx, 1, 0).x;
        let answer_start = caret(&view, cx, 3, 0).x;
        assert_eq!(answer_start, tools_start, "answer prose sits on C1");
        // The row's box is rounded to the pixel and its text keeps its
        // fraction (the browser's model, vendor/gpui-pre "Pixel
        // snapping"): the two agree to within a pixel.
        assert!(
            (tools_start - tools.left() - px(0.5)).abs() <= px(1.0),
            "the words on the row's left edge, plus the caret helper's half-pixel inset: {tools_start:?} {tools:?}"
        );
    }
}

#[gpui::test]
fn multiline_tool_input_is_compact_and_disclosable_while_running(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("multiline-tool-disclosure", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let command = "cat <<'EOF'\nlet answer = 42;\nEOF";
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: "multiline".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": command }),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Next file.\n\n".into(),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: "next".into(),
            name: "Read".into(),
            input: serde_json::json!({ "file_path": "next.rs" }),
        })
        .unwrap();
    tick(cx);

    let chevron = view.read_with(cx, |view, cx| {
        let runs = view.selection.registered(thread);
        assert!(runs
            .iter()
            .any(|(_, _, _, text)| text.contains("cat <<'EOF' …")));
        assert!(!runs
            .iter()
            .any(|(_, _, _, text)| text.contains("let answer")));
        let first = runs
            .iter()
            .find(|(_, _, _, text)| text.starts_with("Running "))
            .unwrap()
            .0;
        let height = |block| {
            ferrite_pane::rich::testing::bounds(
                &format!("thinking-{}-{block:?}", view.panes[0].text_namespace()),
                0,
                cx,
            )
            .unwrap()
            .size
            .height
        };
        assert!(
            height(first) < px(30.),
            "a collapsed script must stay one row tall"
        );
        view.panes[0].tool_bounds("multiline", cx).unwrap().center()
    });
    cx.simulate_click(chevron, gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.panes[0].tool_expanded("multiline"));
        assert!(view
            .selection
            .registered(thread)
            .iter()
            .any(|(_, _, _, text)| text == command));
    });

    // Subsequent events must not prune an input-only disclosure.
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "continuing".into(),
        })
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.panes[0].tool_expanded("multiline"))
    });

    let chevron = view.read_with(cx, |view, cx| {
        view.panes[0].tool_bounds("multiline", cx).unwrap().center()
    });
    cx.simulate_click(chevron, gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.panes[0].tool_expanded("multiline"));
        let transcript = view.cockpit.thread(thread).unwrap().transcript();
        assert!(transcript
            .blocks()
            .iter()
            .any(|block| matches!(&block.body, Body::Tool(tool) if tool.summary == command)));
    });
}

#[gpui::test]
fn clicking_the_tool_chevron_replaces_the_compact_line_with_selectable_output(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("tool-disclosure-pointer", 1);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let output = format!("first line\n\nλ🙂\t{}TAIL", "x".repeat(65_540));
    let stream = fake.streams.borrow();
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
            output,
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::TextDelta {
            text: "Next file.\n\n".into(),
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::ToolStarted {
            id: "toolu_10".into(),
            name: "Read".into(),
            input: serde_json::json!({ "file_path": "/workspace/pending" }),
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::ToolStarted {
            id: "toolu_11".into(),
            name: "Read".into(),
            input: serde_json::json!({ "file_path": "/workspace/later" }),
        })
        .unwrap();
    drop(stream);
    tick(cx);
    // The trailing calls stand behind the live row: open them.
    open_live(cx);

    view.read_with(cx, |view, cx| {
        let transcript = view.cockpit.thread(thread).unwrap().transcript();
        let id = |call: &str| {
            transcript
                .blocks()
                .iter()
                .find_map(|block| match &block.body {
                    Body::Tool(tool) if tool.call == call => Some(block.id),
                    _ => None,
                })
                .unwrap()
        };
        let x = |block| {
            ferrite_pane::rich::testing::bounds(
                &format!("thinking-{}-{block:?}", view.panes[0].text_namespace()),
                0,
                cx,
            )
            .unwrap()
            .left()
        };
        // A call on a file draws its verb as chrome and its path as the run,
        // so its text starts after the verb, never before a command's.
        assert!(
            x(id("toolu_10")) >= x(id("toolu_9")),
            "a disclosure must occupy the existing gutter, not shift call text"
        );
    });

    let collapsed = view.read_with(cx, |view, _| view.selection.registered(thread));
    // Shut, the call line is the call alone: what it printed folds behind
    // it, its first line too.
    assert!(collapsed
        .iter()
        .any(|(_, _, _, text)| text == "Ran `echo hi`"));
    assert!(!collapsed
        .iter()
        .any(|(_, _, _, text)| text.contains("first line")));
    assert!(!collapsed
        .iter()
        .any(|(_, _, _, text)| text.starts_with("first line\n\n")));
    let chevron = view.read_with(cx, |view, cx| {
        // The whole call line is the toggle.
        view.panes[0]
            .tool_bounds("toolu_9", cx)
            .expect("the actual rendered toggle bounds")
            .center()
    });
    cx.simulate_click(chevron, gpui::Modifiers::none());
    cx.run_until_parked();

    view.read_with(cx, |view, _| {
        assert!(view.panes[0].tool_expanded("toolu_9"));
        let runs = view.selection.registered(thread);
        // Large output is one native read-only document, retaining exact
        // hard lines and Unicode without per-character Markdown layout.
        let tool_block = view
            .cockpit
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .find_map(|block| match &block.body {
                Body::Tool(tool) if tool.call == "toolu_9" => Some(block.id),
                _ => None,
            })
            .unwrap();
        let lines: Vec<&(_, _, _, String)> = runs
            .iter()
            .filter(|(block, _, _, _)| *block == tool_block)
            .collect();
        assert!(lines
            .iter()
            .any(|(_, _, starts, text)| *starts && text.starts_with("first line\n\nλ🙂\t")));
        let total: usize = lines.iter().map(|(_, _, _, text)| text.len() + 1).sum();
        assert!(total >= 64 * 1024, "{total}");
        assert!(!runs
            .iter()
            .any(|(_, _, _, text)| text.contains('▾') || text.contains("bytes omitted")));
    });

    let (output_state, expected) = view.read_with(cx, |view, cx| {
        let block = view
            .cockpit
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .find(|block| matches!(&block.body, Body::Tool(tool) if tool.call == "toolu_9"))
            .unwrap();
        let Body::Tool(tool) = &block.body else {
            unreachable!()
        };
        let id = format!(
            "output-{}-{:?}-result",
            view.panes[0].text_namespace(),
            block.id
        );
        (
            ferrite_pane::rich::testing::output(&id, cx).unwrap(),
            tool.output.as_ref().unwrap().text.clone(),
        )
    });
    let bounds = output_state.read_with(cx, |state, _| state.text_bounds().unwrap());
    assert!(
        bounds.size.height < px(350.),
        "large output has a bounded native viewport"
    );
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    tick(cx);
    cx.update(|window, cx| {
        assert!(
            output_state.focus_handle(cx).is_focused(window),
            "progress pumps preserve native output focus"
        )
    });
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-down"
    } else {
        "ctrl-end"
    });
    cx.run_until_parked();
    output_state.read_with(cx, |state, _| {
        assert_eq!(
            state.cursor(),
            expected.len(),
            "native navigation reaches the retained tail"
        );
        assert!(
            state.scroll_offset().y < px(0.),
            "the native viewer scrolls to its tail"
        );
    });
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-a cmd-c"
    } else {
        "ctrl-a ctrl-c"
    });
    assert_eq!(clipboard(cx).as_deref(), Some(expected.as_str()));
    cx.simulate_keystrokes("backspace");
    output_state.read_with(cx, |state, _| {
        assert_eq!(state.value().as_ref(), expected.as_str());
    });
    // Expanding long output can scroll the header out of view while
    // following the tail. Bring it back before clicking its disclosure.
    view.update(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .update(cx, |transcript, _| {
                transcript.scroll().pause_following_tail();
                transcript.scroll().set_offset(gpui::point(px(0.), px(0.)));
            });
        cx.notify();
    });
    cx.run_until_parked();

    let chevron = view.read_with(cx, |view, cx| {
        view.panes[0].tool_bounds("toolu_9", cx).unwrap().center()
    });
    cx.simulate_click(chevron, gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.panes[0].tool_expanded("toolu_9"));
        assert!(!view
            .selection
            .registered(thread)
            .iter()
            .any(|(_, _, _, text)| text.starts_with("first line\n\n")));
    });

    let composer = view.read_with(cx, |view, cx| {
        let body = view.panes[0]
            .transcript()
            .unwrap()
            .read(cx)
            .scroll()
            .bounds();
        gpui::point(body.center().x, body.bottom() + px(20.))
    });
    cx.simulate_click(composer, gpui::Modifiers::none());
    cx.update(|window, cx| {
        view.read_with(cx, |view, cx| {
            assert!(!view.panes[0].tool_targeted("toolu_9"));
            assert!(view.panes[0].composer.focus_handle(cx).is_focused(window));
        });
    });
}

#[gpui::test]
fn streaming_output_preserves_native_selection_scroll_and_command_identity(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("streaming-output-view", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let command = format!("cat <<'EOF'\n{}\nEOF", "command ".repeat(1200));
    let original = "a line of exact output λ🙂 with spaces\n".repeat(300);
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: "stream".into(),
            name: "Bash".into(),
            input: serde_json::json!({"command": command}),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolOutputDelta {
            id: "stream".into(),
            text: original.clone(),
        })
        .unwrap();
    tick(cx);
    open_live(cx);
    let chevron = view.read_with(cx, |view, cx| {
        view.panes[0].tool_bounds("stream", cx).unwrap().center()
    });
    cx.simulate_click(chevron, gpui::Modifiers::none());
    cx.run_until_parked();
    let (command_state, result_state) = view.read_with(cx, |view, cx| {
        let block = view
            .cockpit
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .find(|block| matches!(&block.body, Body::Tool(tool) if tool.call == "stream"))
            .unwrap();
        let prefix = format!("output-{}-{:?}", view.panes[0].text_namespace(), block.id);
        (
            ferrite_pane::rich::testing::output(&format!("{prefix}-command"), cx).unwrap(),
            ferrite_pane::rich::testing::output(&format!("{prefix}-result"), cx).unwrap(),
        )
    });
    assert_ne!(command_state.entity_id(), result_state.entity_id());
    command_state.read_with(cx, |state, _| assert_eq!(state.value().as_ref(), command));
    let bounds = result_state.read_with(cx, |state, _| state.text_bounds().unwrap());
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-down shift-up"
    } else {
        "ctrl-end shift-up"
    });
    cx.run_until_parked();
    let (selected, cursor, scroll) = result_state.read_with(cx, |state, _| {
        (
            state.selected_value(),
            state.cursor(),
            state.scroll_offset(),
        )
    });
    assert!(!selected.is_empty());
    assert!(scroll.y < px(0.));
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolOutputDelta {
            id: "stream".into(),
            text: "new output\n".into(),
        })
        .unwrap();
    tick(cx);
    result_state.read_with(cx, |state, _| {
        assert_eq!(state.value().as_ref(), format!("{original}new output\n"));
        assert_eq!(state.selected_value(), selected);
        assert_eq!(state.cursor(), cursor);
        assert_eq!(state.scroll_offset(), scroll);
    });
    command_state.read_with(cx, |state, _| assert_eq!(state.value().as_ref(), command));
    // A final replacement may be shorter. Native clipping keeps the
    // retained selection on character boundaries without invalid offsets.
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolCompleted {
            id: "stream".into(),
            output: "λ🙂\n".repeat(1500),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        })
        .unwrap();
    tick(cx);
    result_state.read_with(cx, |state, _| {
        let value = state.value();
        let range = state.selected_range();
        assert!(value.is_char_boundary(range.start) && value.is_char_boundary(range.end));
    });
}

#[gpui::test]
fn native_progress_follows_the_tail_and_metadata_refreshes_the_wall(cx: &mut TestAppContext) {
    use ferrite_core::progress::{Phase, ProgressEvent};
    let (mut core, fake) = cockpit("native-progress", 1);
    let thread = core.threads()[0];
    core.send(thread, "Inspect progress".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new_thinking(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    for index in 0..70 {
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: format!("Earlier answer {index}.\n\n"),
            })
            .unwrap();
    }
    fake.streams.borrow()[0]
        .send(SessionEvent::ReasoningSummaryDelta {
            text: "**Checking fold call sites**".into(),
            summary_index: 0,
        })
        .unwrap();
    tick(cx);
    cx.debug_bounds("transcript-progress")
        .expect("the live row is the transcript's last row");
    assert!(
        cx.debug_bounds("live-thinking").is_some(),
        "with no call running, the live row thinks"
    );
    cx.debug_bounds("live-thought")
        .expect("the trailing thought stands over the live row, once");
    view.update(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .update(cx, |transcript, cx| {
                transcript.scroll().pause_following_tail();
                transcript.scroll().set_offset(gpui::point(px(0.), px(0.)));
                // As a wheel's scroll does: the transcript draws again.
                cx.notify();
            });
        cx.notify();
    });
    cx.run_until_parked();
    // The live row is the transcript's last row: scrolled back, it is out
    // of view with the tail, and the way back to both is in view.
    assert!(
        cx.debug_bounds("jump-to-latest").is_some(),
        "scrolled away from a working tail, jump to latest is offered"
    );
    // Working-phase detail appends no transcript block. Its wall caption
    // must still change, or the smallest panes freeze on the old heading.
    fake.streams.borrow()[0]
        .send(SessionEvent::Progress {
            event: ProgressEvent::Phase {
                phase: Phase::Compacting,
                detail: String::new(),
            },
        })
        .unwrap();
    tick(cx);
    fake.streams.borrow()[0]
        .send(SessionEvent::Progress {
            event: ProgressEvent::Phase {
                phase: Phase::Working,
                detail: "Context compacted".into(),
            },
        })
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view
            .facts
            .get(thread)
            .unwrap()
            .wall
            .working
            .contains("Checking fold call sites"));
    });
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    assert!(
        cx.debug_bounds("transcript-progress").is_none() && cx.debug_bounds("live-thought").is_none(),
        "completed turns stop their live indication"
    );
}

#[gpui::test]
fn edited_files_are_in_the_panel_and_one_click_away(cx: &mut TestAppContext) {
    let (core, fake, workspace) = bound_cockpit("thread-documents", Provider::Claude);
    let document = workspace.join("docs").join("generated.md");
    let source = workspace.join("src").join("generated.rs");
    std::fs::create_dir_all(document.parent().unwrap()).unwrap();
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&document, "# Generated guide\n").unwrap();
    std::fs::write(&source, "fn generated() {}\n").unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    let stream = fake.streams.borrow();
    stream[0]
        .send(SessionEvent::ToolStarted {
            id: "edit-doc".into(),
            name: "Edit".into(),
            input: serde_json::json!({ "file_path": "docs/generated.md" }),
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::ToolCompleted {
            id: "edit-doc".into(),
            output: "updated".into(),
            is_error: false,
            result: ferrite_core::ToolResult::FileEdit {
                path: "docs/generated.md".into(),
                hunks: vec![ferrite_core::Hunk {
                    old_start: 1,
                    old_lines: 0,
                    new_start: 1,
                    new_lines: 1,
                    lines: vec!["+# Generated guide".into()],
                    section: None,
                }],
            },
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::ToolStarted {
            id: "edit-code".into(),
            name: "Edit".into(),
            input: serde_json::json!({ "file_path": "src/generated.rs" }),
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::ToolCompleted {
            id: "edit-code".into(),
            output: "updated".into(),
            is_error: false,
            result: ferrite_core::ToolResult::FileEdit {
                path: "src/generated.rs".into(),
                hunks: vec![ferrite_core::Hunk {
                    old_start: 1,
                    old_lines: 1,
                    new_start: 1,
                    new_lines: 1,
                    lines: vec!["-fn old() {}".into(), "+fn generated() {}".into()],
                    section: None,
                }],
            },
        })
        .unwrap();
    drop(stream);
    tick(cx);

    // The files are the panel's Changes tab, beside the Pane: what
    // the Thread has done, apart from what is being typed.
    let section = cx
        .debug_bounds("panel-changes-section")
        .expect("the panel counts the edited files");
    let card = cx.update(|window, cx| view.read(cx).pane_rects(window))[0].1;
    assert!(
        f32::from(section.left()) >= card.x + card.w,
        "beside the Pane, not in it"
    );
    let link = cx
        .debug_bounds("panel-file-0")
        .expect("the edited Markdown file is listed in its Thread");
    cx.simulate_click(link.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.files_active_path(view.panes[0].thread().unwrap())),
        Some(document)
    );
    assert!(
        cx.debug_bounds("panel").is_some(),
        "the preview is the Panel's"
    );
    view.update(cx, |view, cx| view.close_preview(0, cx));
    tick(cx);
    let code = cx
        .debug_bounds("panel-file-1")
        .expect("a non-Markdown file is listed too");
    cx.simulate_click(code.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.files_active_path(view.panes[0].thread().unwrap())),
        Some(source)
    );
}

#[gpui::test]
fn keyboard_cycles_tool_controls_and_enter_toggles_without_submitting(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("tool-disclosure-keyboard", 1);
    let thread = core.threads()[0];
    core.send(thread, "prior prompt".into());
    bind_production_keys(cx);
    let (root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| CockpitView::new(core, cx));
        gpui::component::Root::new(
            cx.new(|cx| crate::cockpit_window::CockpitWindow::new(view, window, cx)),
            window,
            cx,
        )
    });
    let view = root.read_with(cx, |root, cx| {
        crate::cockpit_window::testing::cockpit(root.view(), cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let stream = fake.streams.borrow();
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
            output: "first line\nsecond line".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    drop(stream);
    tick(cx);
    open_folds(&view, cx);
    cx.simulate_input("unsent draft");

    // The turn's fold is the first stop; then every call is its own row:
    // one Tab stop per call.
    cx.simulate_keystrokes("tab");
    view.read_with(cx, |view, _| {
        assert!(matches!(
            view.panes[0].targeted_tool(),
            Some(pane::DisclosureId::Turn(_))
        ));
    });
    cx.simulate_keystrokes("tab");
    view.read_with(cx, |view, _| {
        assert!(view.panes[0].tool_targeted("toolu_9"));
        assert!(!view.history_available(0, Level::Transcript));
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.panes[0].tool_expanded("toolu_9"));
        assert_eq!(
            fake.sent.borrow().as_slice(),
            ["prior prompt"],
            "Enter must not submit"
        );
    });

    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| {
        view.read_with(cx, |view, cx| {
            assert!(!view.panes[0].tool_targeted("toolu_9"));
            assert!(view.panes[0].composer.focus_handle(cx).is_focused(window));
        });
    });

    cx.simulate_keystrokes("shift-tab");
    view.read_with(
        cx,
        |view, _| assert!(view.panes[0].tool_targeted("toolu_9")),
    );
    for line in 0..200 {
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: format!("later block {line}\n\n"),
            })
            .unwrap();
    }
    tick(cx);
    cx.update(|window, cx| {
        view.read_with(cx, |view, cx| {
            assert!(!view.panes[0].tool_targeted("toolu_9"));
            assert!(view.panes[0].composer.focus_handle(cx).is_focused(window));
        });
    });
}

/// #15 review, at the rendered window (#27): streaming slides the
/// window of Blocks a Pane draws, shifting every rendered position — a
/// selection stored as positions would quietly slide onto rows the
/// operator never touched. Ids pin it; an endpoint that leaves the
/// window clamps the copy to the window start; with both ends gone the
/// selection dies instead of resurrecting elsewhere.
#[gpui::test]
fn an_evicted_native_selection_clears_instead_of_sliding_onto_later_blocks(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("select-evict", 1);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let say = |from: usize, to: usize| {
        for line in from..to {
            fake.streams.borrow()[0]
                .send(SessionEvent::TextDelta {
                    text: format!("filler {line:04}\n\n"),
                })
                .unwrap();
        }
    };
    // The counts below straddle Level::Transcript's 200-Block rendered
    // window: past 200 total, the window's start slides.
    say(0, 60);
    tick(cx);
    // Wheel to the very top, where Blocks 5..=7 are on screen.
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: gpui::point(px(500.), px(350.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(100000.))),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::default(),
    });
    cx.run_until_parked();
    let texts: Vec<String> = (5..=7).map(|line| format!("filler {line:04}")).collect();
    let from = caret(&view, cx, 5, 0);
    let mut to = caret(&view, cx, 7, texts[2].len() - 1);
    // Past the line's right edge: the nearest-index clamp takes the
    // last character too.
    to.x += px(40.);
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some(texts.join("\n").as_str()));

    // A native document that leaves the rendered window loses its
    // selection. Copy must never silently move onto replacement text.
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("kept".into())));
    say(60, 203);
    tick(cx);
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some("kept"));
    cx.update(|window, cx| assert!(!gpui::base::TextSelection::has_selection(window, cx)));
    say(203, 208);
    tick(cx);
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(
        clipboard(cx).as_deref(),
        Some("kept"),
        "evicted selection must not resurrect on other Blocks"
    );
}

/// #15 review: a drag in a scrolled-back transcript selects the
/// characters under the pointer — TextLayout records screen-space
/// geometry each frame, so the hit test needs no offset math of its
/// own (#27), and must not land on the rows at those coordinates in
/// the unscrolled layout.
#[gpui::test]
fn a_drag_in_a_scrolled_back_transcript_selects_the_rows_under_the_pointer(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("select-scrolled", 1);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    for line in 0..60 {
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: format!("history line {line:02}\n\n"),
            })
            .unwrap();
    }
    tick(cx);

    // Wheel well back into history, then drag across on-screen rows.
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: gpui::point(px(500.), px(350.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(300.))),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::default(),
    });
    cx.run_until_parked();
    // The first row fully inside the viewport — nonzero, or the wheel
    // did not actually scroll anything back.
    let viewport = view.read_with(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .read(cx)
            .scroll()
            .bounds()
    });
    let row = (0..58)
        .find(|row| caret(&view, cx, *row, 0).y > viewport.top() + px(20.))
        .expect("a paragraph in the viewport");
    assert!(row > 0, "the wheel put earlier rows above the viewport");
    // All 60 Blocks render, so row indices are block indices here.
    let expected: Vec<String> = (row..=row + 2)
        .map(|line| format!("history line {line:02}"))
        .collect();
    let from = caret(&view, cx, row, 0);
    let mut to = caret(&view, cx, row + 2, expected[2].len() - 1);
    to.x += px(40.);

    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");

    assert_eq!(clipboard(cx).as_deref(), Some(expected.join("\n").as_str()));
}

/// The live row stands for the running turn's trailing work (`Running
/// sleep`); a press opens that work's own rows above it, and shuts them
/// again.
#[gpui::test]
fn the_live_row_opens_the_work_it_stands_for(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("working-line-disclosure", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: "wait".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "sleep 15 && echo done" }),
        })
        .unwrap();
    tick(cx);
    let running_rows = |view: &CockpitView| {
        view.selection
            .registered(thread)
            .iter()
            .filter(|(_, _, _, text)| text.starts_with("Running "))
            .count()
    };
    view.read_with(cx, |view, _| {
        assert_eq!(running_rows(view), 0, "the live row stands for the call");
    });
    let live = cx
        .debug_bounds("live-call")
        .expect("the live row names the running call")
        .center();
    cx.simulate_click(live, gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.panes[0].tool_expanded(pane::DisclosureId::Group("wait".into())));
        assert_eq!(running_rows(view), 1, "opened: the call's own row shows");
    });
    let live = cx.debug_bounds("live-call").unwrap().center();
    cx.simulate_click(live, gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.panes[0].tool_expanded(pane::DisclosureId::Group("wait".into())));
        assert_eq!(running_rows(view), 0);
    });
}

/// A turn opens with `Working for …` under its prompt, its running call
/// the live row at its tail (not hidden: the live row is it). Once it
/// settles, the header's place is the fold, `Worked for …`, shut over its
/// work with only the answer under it; a press opens the work in place and
/// another shuts it, the chevron swapping glyphs.
#[gpui::test]
fn a_turn_heads_with_working_for_and_settles_into_its_fold(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("turn-fold", 1);
    let thread = core.threads()[0];
    core.send(thread, "Edit the file".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let send = |event| fake.streams.borrow()[0].send(event).unwrap();
    send(SessionEvent::ToolStarted {
        id: "fold-sed".into(),
        name: "Bash".into(),
        input: serde_json::json!({"command": "sed -i '' 's/a/b/' lib.rs"}),
    });
    tick(cx);
    let prompt = cx.debug_bounds("transcript-prompt").unwrap();
    let header = cx.debug_bounds("turn-working").expect("the running turn's header");
    let live = cx.debug_bounds("live-call").expect("the live row");
    assert!(header.top() > prompt.bottom() && live.top() > header.bottom());
    assert!(cx.debug_bounds("turn-fold").is_none());
    send(SessionEvent::ToolCompleted {
        id: "fold-sed".into(),
        output: String::new(),
        is_error: false,
        result: ferrite_core::ToolResult::Opaque,
    });
    send(SessionEvent::TextDelta {
        text: "Edited.\n\n".into(),
    });
    send(SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Completed,
        cost_usd: None,
    });
    tick(cx);
    assert!(cx.debug_bounds("turn-working").is_none());
    assert!(cx.debug_bounds("transcript-progress").is_none());
    let fold = cx.debug_bounds("turn-fold").expect("the settled turn's fold");
    let answer = cx.debug_bounds("transcript-answer").unwrap();
    assert!(answer.top() > fold.bottom());
    assert!(cx.debug_bounds("turn-fold-shut").is_some());
    assert!(cx.debug_bounds("tool-row-fold-sed").is_none(), "shut, the work folds away");
    let words = cx.debug_bounds("turn-fold-words").unwrap();
    cx.simulate_click(words.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("turn-fold-open").is_some());
    let call = cx.debug_bounds("tool-row-fold-sed").expect("opened, the work shows");
    assert!(call.top() > fold.bottom() && call.bottom() < cx.debug_bounds("transcript-answer").unwrap().top());
    // The open state is the turn's: a next turn projects the rows again
    // and leaves it open.
    view.update(cx, |view, _| view.cockpit.send(thread, "And again".into()));
    send(SessionEvent::TextDelta {
        text: "Again.\n\n".into(),
    });
    send(SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Completed,
        cost_usd: None,
    });
    tick(cx);
    assert!(
        cx.debug_bounds("tool-row-fold-sed").is_some(),
        "the fold stays open across a re-projection"
    );
    let words = cx.debug_bounds("turn-fold-words").unwrap();
    cx.simulate_click(words.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("tool-row-fold-sed").is_none());
    assert!(cx.debug_bounds("turn-fold-shut").is_some());
}

/// The pointer on a fold brings in the turn's start time at the row's
/// end; it takes no room at rest.
#[gpui::test]
fn a_hovered_fold_shows_its_turns_start_time(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("turn-fold-time", 1);
    let thread = core.threads()[0];
    core.send(thread, "Run it".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let send = |event| fake.streams.borrow()[0].send(event).unwrap();
    send(SessionEvent::ToolStarted {
        id: "time-run".into(),
        name: "Bash".into(),
        input: serde_json::json!({"command": "cargo test"}),
    });
    send(SessionEvent::ToolCompleted {
        id: "time-run".into(),
        output: "ok".into(),
        is_error: false,
        result: ferrite_core::ToolResult::Opaque,
    });
    send(SessionEvent::TextDelta {
        text: "Passed.\n\n".into(),
    });
    send(SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Completed,
        cost_usd: None,
    });
    tick(cx);
    assert!(cx.debug_bounds("turn-fold-time").is_none());
    let fold = cx.debug_bounds("turn-fold").unwrap();
    cx.simulate_mouse_move(fold.center(), None, gpui::Modifiers::none());
    tick(cx);
    let time = cx.debug_bounds("turn-fold-time").expect("the start time on hover");
    assert!(time.right() <= fold.right() && time.left() > cx.debug_bounds("turn-fold-words").unwrap().right());
}
