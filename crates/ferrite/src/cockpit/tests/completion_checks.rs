//! Root-owned acceptance checks for the remaining transcript findings.
use super::*;

#[gpui::test]
fn streamed_markdown_keeps_its_native_document_and_selected_prose(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("markdown-stream-reference", 1);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "alpha  beta\n\n".into(),
        })
        .unwrap();
    tick(cx);
    let namespace = view.read_with(cx, |view, _| view.panes[0].text_namespace());
    let entity = cx.update(|_, cx| {
        crate::rich::testing::first_entity(&format!("markdown-{namespace}-"), cx).unwrap()
    });
    let from = caret(&view, cx, 0, 0);
    let to = caret(&view, cx, 0, 5);
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some("alpha"));
    for chunk in [
        "## Head",
        "ing\n\n- one\n",
        "- two\n\n```rust\nlet x",
        " = 1;\n```\n\nDone.",
    ] {
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta { text: chunk.into() })
            .unwrap();
        tick(cx);
        assert_eq!(
            cx.update(|_, cx| crate::rich::testing::first_entity(
                &format!("markdown-{namespace}-"),
                cx
            )
            .unwrap()),
            entity
        );
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(
            clipboard(cx).as_deref(),
            Some("alpha"),
            "streaming must retain the selected prose"
        );
    }
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    assert_eq!(
        cx.update(|_, cx| crate::rich::testing::first_entity(
            &format!("markdown-{namespace}-"),
            cx
        )
        .unwrap()),
        entity
    );
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some("alpha"));
}

#[gpui::test]
fn wrapped_question_retains_exact_picks_and_note_through_rejection_and_ack(
    cx: &mut TestAppContext,
) {
    use ferrite_core::activity::{ActivityEvent, Subject};
    let (core, fake) = cockpit("question-retry-reference", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(740.), px(1200.)));
    cx.simulate_input("draft  untouched");
    let label = "Keep the existing implementation and its meaningful suffix (Recommended)";
    let SessionEvent::DecisionRequested { mut decision } = question("retry-q") else {
        unreachable!()
    };
    decision.delivery = ferrite_core::DecisionDelivery::Async;
    decision.input = serde_json::json!({"questions":[{"question":"Which approach?", "multiSelect":true,
        "options":[{"label":label,"description":"Preserve existing controls and all the spacing in their wrapped descriptions."},
        {"label":"B","description":"Also keep this option"}]}]});
    let ferrite_core::DecisionKind::Questions(questions) = &mut decision.kind else {
        unreachable!()
    };
    questions[0].multi_select = true;
    questions[0].options = vec![
        ferrite_core::questions::Choice {
            label: label.into(),
            description:
                "Preserve existing controls and all the spacing in their wrapped descriptions."
                    .into(),
            preview: None,
        },
        ferrite_core::questions::Choice {
            label: "B".into(),
            description: "Also keep this option".into(),
            preview: None,
        },
    ];
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(ActivityEvent::Decision {
            subject: Some(Subject::Main),
            decision,
        }))
        .unwrap();
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
    let first = cx.debug_bounds("question-choice-0-0").unwrap();
    let second = cx.debug_bounds("question-choice-0-1").unwrap();
    let island = cx.debug_bounds("question-island").unwrap();
    assert!(
        first.size.height > px(50.),
        "long label and description must wrap"
    );
    assert!(first.right() <= island.right() && second.top() >= first.bottom());
    cx.simulate_click(first.center(), gpui::Modifiers::none());
    cx.simulate_click(second.center(), gpui::Modifiers::none());
    let other_point = bounds(cx, format!("request-other-{}-{serial}-0", thread.get())).center();
    cx.simulate_click(other_point, gpui::Modifiers::none());
    let note = "  keep  two   spaces  ";
    cx.simulate_input(note);
    let submit_point = bounds(cx, format!("request-submit-{}-{serial}", thread.get())).center();
    cx.simulate_click(submit_point, gpui::Modifiers::none());
    tick(cx);
    assert!(
        cx.debug_bounds("question-island").is_some(),
        "keep the submitting form until acknowledgement"
    );
    let assert_answer = || {
        let answered = fake.answered.borrow();
        let (_, DecisionAnswer::Questions { answers }) = answered.last().unwrap() else {
            panic!("form answer")
        };
        assert_eq!(answers[0].picks, [0, 1]);
        assert_eq!(answers[0].other.as_deref(), Some(note));
    };
    assert_answer();
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(ActivityEvent::DecisionReply {
            id: "retry-q".into(),
            error: Some("delivery refused".into()),
        }))
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("question-island").is_some());
    let submit_point = bounds(cx, format!("request-submit-{}-{serial}", thread.get())).center();
    cx.simulate_click(submit_point, gpui::Modifiers::none());
    tick(cx);
    assert_eq!(fake.answered.borrow().len(), 2);
    assert_answer();
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(ActivityEvent::DecisionReply {
            id: "retry-q".into(),
            error: None,
        }))
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("question-island").is_none());
    assert_eq!(composer_text(&view, cx), "draft  untouched");
}

#[gpui::test]
fn reading_anchor_survives_streaming_disclosure_and_narrower_window(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("reading-anchor-reference", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    let paragraph = "Earlier material with enough words to reflow when the pane becomes narrower. ";
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: format!("{}\n\n", paragraph.repeat(3)).repeat(12),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: "anchor-tool".into(),
            name: "Read".into(),
            input: serde_json::json!({"file_path":"src/lib.rs"}),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolCompleted {
            id: "anchor-tool".into(),
            output: "one\ntwo\nthree".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Later paragraph.\n\n".repeat(25),
        })
        .unwrap();
    tick(cx);
    let viewport = view.read_with(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .read(cx)
            .scroll()
            .bounds()
    });
    // The retained list mounts only its viewport. Scroll the existing tool
    // into view before measuring the reading anchor, as a reader would.
    for _ in 0..30 {
        if cx.debug_bounds("tool-group-anchor-tool").is_some() {
            break;
        }
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: viewport.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(300.))),
            ..Default::default()
        });
        tick(cx);
    }
    let initial = cx.debug_bounds("tool-group-anchor-tool").unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(
            px(0.),
            viewport.top() + px(120.) - initial.top(),
        )),
        ..Default::default()
    });
    tick(cx);
    let held = cx.debug_bounds("tool-group-anchor-tool").unwrap().top();
    assert!(!view.read_with(cx, |view, cx| view.panes[0]
        .transcript()
        .unwrap()
        .read(cx)
        .is_following_tail()));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "New streamed material.\n\n".repeat(5),
        })
        .unwrap();
    tick(cx);
    assert!((cx.debug_bounds("tool-group-anchor-tool").unwrap().top() - held).abs() < px(1.));
    let group = view.read_with(cx, |view, _| {
        view.panes[0]
            .tool_bounds(pane::DisclosureId::Group("anchor-tool".into()))
            .unwrap()
            .center()
    });
    cx.simulate_click(group, gpui::Modifiers::none());
    tick(cx);
    assert!((cx.debug_bounds("tool-group-anchor-tool").unwrap().top() - held).abs() < px(1.));
    cx.simulate_resize(gpui::size(px(740.), px(900.)));
    tick(cx);
    let after = cx.debug_bounds("tool-group-anchor-tool").unwrap().top();
    assert!(
        (after - held).abs() < px(1.),
        "resize must preserve the visible reading anchor: {held:?} -> {after:?}"
    );
    assert!(view.read_with(cx, |view, _| view.panes[0]
        .tool_expanded(pane::DisclosureId::Group("anchor-tool".into()))));
    assert!(!view.read_with(cx, |view, cx| view.panes[0]
        .transcript()
        .unwrap()
        .read(cx)
        .is_following_tail()));
}

#[gpui::test]
fn multiline_composer_keeps_its_lower_edge_and_end_caret_on_resize(cx: &mut TestAppContext) {
    let (core, _) = cockpit("composer-resize-reference", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    tick(cx);
    let bottom = cx.debug_bounds("focused-prompt-editor").unwrap().bottom();
    let draft =
        "    indented  text\twith a long continuation that must wrap across the narrow composer\n"
            .repeat(12)
            + "    final  line";
    cx.simulate_input(&draft);
    tick(cx);
    let expanded = cx.debug_bounds("focused-prompt-editor").unwrap();
    assert_eq!(
        expanded.bottom(),
        bottom,
        "composer grows upward from its fixed lower edge"
    );
    assert!(expanded.size.height > px(40.));
    for width in [740., 1000.] {
        cx.simulate_resize(gpui::size(px(width), px(900.)));
        tick(cx);
        let bounds = cx.debug_bounds("focused-prompt-editor").unwrap();
        assert_eq!(bounds.bottom(), bottom);
        assert_eq!(composer_text(&view, cx), draft);
        // The final logical line must be in the visible last row. A click at
        // its far right reaches the end only if the caret was kept visible.
        cx.simulate_click(
            gpui::point(bounds.right() - px(2.), bounds.bottom() - px(5.)),
            gpui::Modifiers::none(),
        );
        assert_eq!(
            view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).cursor()),
            draft.len()
        );
    }
}

#[gpui::test]
fn typed_and_pasted_question_marks_remain_literal_composer_text(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("question-mark-paste-reference", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    cx.simulate_keystrokes("?");
    assert_eq!(composer_text(&view, cx), "?");
    cx.update(|_, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("?  literal\ttext\n  next".into()))
    });
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(composer_text(&view, cx), "??  literal\ttext\n  next");
    assert!(
        fake.sent.borrow().is_empty(),
        "pasting never submits the draft"
    );
}

#[gpui::test]
fn reading_anchor_survives_expanding_earlier_tool_details(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("anchor-earlier-disclosure", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    for id in ["earlier", "visible"] {
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolStarted {
                id: id.into(),
                name: "Read".into(),
                input: serde_json::json!({"file_path":id}),
            })
            .unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolCompleted {
                id: id.into(),
                output: "long output line\n".repeat(30),
                is_error: false,
                result: ferrite_core::ToolResult::Opaque,
            })
            .unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: "Separating commentary.\n\n".repeat(12),
            })
            .unwrap();
    }
    tick(cx);
    let viewport = view.read_with(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .read(cx)
            .scroll()
            .bounds()
    });
    let initial = cx.debug_bounds("tool-group-visible").unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(
            px(0.),
            viewport.top() + px(60.) - initial.top(),
        )),
        ..Default::default()
    });
    tick(cx);
    let held = cx.debug_bounds("tool-group-visible").unwrap().top();
    view.update(cx, |view, cx| {
        view.panes[0].toggle_tool(&pane::DisclosureId::Group("earlier".into()));
        view.panes[0].toggle_tool(&pane::DisclosureId::Tool("earlier".into()));
        cx.notify();
    });
    tick(cx);
    let after = cx.debug_bounds("tool-group-visible").unwrap().top();
    assert!(
        (after - held).abs() < px(1.),
        "opening earlier details must preserve reading position: {held:?} -> {after:?}"
    );
}

#[gpui::test]
fn small_tool_output_copies_blank_lines_and_trailing_spaces_exactly(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("small-output-exact", 1);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    let source = "first  \n\nlast   \n";
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: "exact".into(),
            name: "Bash".into(),
            input: serde_json::json!({"command":"fixture"}),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolCompleted {
            id: "exact".into(),
            output: source.into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        })
        .unwrap();
    tick(cx);
    view.update(cx, |view, cx| {
        view.panes[0].toggle_tool(&pane::DisclosureId::Group("exact".into()));
        view.panes[0].toggle_tool(&pane::DisclosureId::Tool("exact".into()));
        cx.notify();
    });
    tick(cx);
    cx.update(|_, cx| crate::rich::testing::select_all(cx));
    cx.simulate_keystrokes("cmd-c");
    let copied = clipboard(cx).unwrap();
    assert!(
        copied.contains(source),
        "literal output must survive copy without synthetic blank-line spaces: {copied:?}"
    );
}

#[gpui::test]
fn received_reasoning_is_shown_once_during_live_to_settled_handoff(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("reasoning-once", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    let text = "Checking the supplied spacing and tool results";
    let historical_count = |cx: &gpui::VisualTestContext, source: &str| {
        view.read_with(cx, |view, _| {
            view.selection
                .registered(view.panes[0].thread().unwrap())
                .iter()
                .filter(|(_, _, _, value)| value == source)
                .count()
        })
    };
    let bold = format!("**{text}**");
    for source in [text, bold.as_str()] {
        fake.streams.borrow()[0]
            .send(SessionEvent::ReasoningSummaryPart {
                item_id: "reasoning".into(),
                summary_index: 0,
                snapshot: true,
                text: source.into(),
            })
            .unwrap();
        tick(cx);
        let historical = historical_count(cx, source);
        let live = usize::from(
            cx.debug_bounds("progress-caption-Checking the supplied spacing and tool results")
                .is_some(),
        );
        assert_eq!(historical+live,1,"plain and formatted supplied summaries both require one visible presentation: {source}");
    }
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    assert_eq!(
        historical_count(cx, &bold),
        1,
        "completion retains exactly one historical summary"
    );
    assert!(cx
        .debug_bounds("progress-caption-Checking the supplied spacing and tool results")
        .is_none());
}

#[gpui::test]
fn pending_approval_exposes_exact_selectable_command_before_answering(cx: &mut TestAppContext) {
    use ferrite_core::activity::{ActivityEvent, AgentKey, ExecutionEvent, Subject};
    let (core, fake) = cockpit("approval-exact-command", 1);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(740.), px(900.)));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let command = "printf 'one  two\\n'\n  cat formatting.md\n".repeat(12);
    let key = AgentKey::new(Provider::Claude, "root", "approval-child");
    for (id, subject) in [
        ("main-approval", Subject::Main),
        ("child-approval", Subject::Subagent(key.clone())),
    ] {
        let is_main = subject == Subject::Main;
        if let Subject::Subagent(key) = &subject {
            fake.streams.borrow()[0]
                .send(SessionEvent::Activity(ActivityEvent::Content {
                    key: key.clone(),
                    id: None,
                    event: ExecutionEvent::Text {
                        text: "Ready to inspect".into(),
                    },
                }))
                .unwrap();
            tick(cx);
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.select_subject(thread, subject.clone(), window, cx)
                })
            });
        }
        fake.streams.borrow()[0]
            .send(SessionEvent::Activity(ActivityEvent::Decision {
                subject: Some(subject),
                decision: Decision {
                    kind: Default::default(),
                    policy: Default::default(),
                    delivery: Default::default(),
                    id: id.into(),
                    tool_use_id: id.into(),
                    tool_name: "Bash".into(),
                    description: "Inspect the fixture without changing it.".into(),
                    suggestions: vec![],
                    input: serde_json::json!({"command":command}),
                },
            }))
            .unwrap();
        tick(cx);
        let input = cx
            .debug_bounds("approval-input")
            .expect("the command must be inspectable before an approval is sent");
        assert!(input.size.height > px(30.));
        assert!(input.right() <= px(740.));
        assert_eq!(
            input.size.height,
            px(160.),
            "long commands have a bounded viewport"
        );
        let text_id = view.read_with(cx, |view, _| {
            if is_main {
                format!("approval-input-{}-{id}", view.panes[0].text_namespace())
            } else {
                let handle = &view
                    .cockpit
                    .thread(thread)
                    .unwrap()
                    .activity()
                    .pending_decisions()
                    .iter()
                    .find(|request| request.decision.id == id)
                    .unwrap()
                    .handle;
                format!(
                    "approval-input-request-{}-{}-{}",
                    thread.get(),
                    handle.generation,
                    handle.serial
                )
            }
        });
        let command_top = |cx: &mut gpui::VisualTestContext| {
            cx.update(|_, cx| crate::rich::testing::bounds(&text_id, 0, cx).unwrap().top())
        };
        let initial_top = command_top(cx);
        let from = gpui::point(input.right() - px(3.), input.top() + px(10.));
        let to = from + gpui::point(px(0.), px(60.));
        cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
        // The scrollbar throttles dragging using a wall-clock 120 Hz limit.
        std::thread::sleep(Duration::from_millis(12));
        cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
        tick(cx);
        let scrolled_top = command_top(cx);
        assert!(
            scrolled_top < initial_top,
            "the command scrollbar thumb must drag in {id}"
        );
        view.update(cx, |_, cx| cx.notify());
        tick(cx);
        assert_eq!(
            command_top(cx),
            scrolled_top,
            "repainting must retain the scroll position"
        );
        cx.update(|_, cx| crate::rich::testing::select_all(cx));
        cx.simulate_keystrokes("cmd-c");
        assert!(
            clipboard(cx).unwrap().contains(&command),
            "approval copies the exact supplied command including trailing newline"
        );
        tick(cx);
        cx.simulate_keystrokes("cmd-c");
        assert!(
            clipboard(cx).unwrap().contains(&command),
            "a repaint retains the selected command"
        );
        assert!(
            fake.answered.borrow().is_empty(),
            "inspection must not approve execution"
        );
    }
}

#[gpui::test]
fn long_subagent_approval_keeps_allow_and_deny_inside_the_island(cx: &mut TestAppContext) {
    use ferrite_core::activity::{ActivityEvent, AgentKey, ExecutionEvent, Subject};
    let (core, fake) = cockpit("approval-long-command-island", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(900.), px(600.)));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let command = "/bin/zsh -lc 'rm -rf /Users/example/.agents/skills/pane-browser /Users/example/.claude/skills/pane-browser\n test ! -e /Users/example/.agents/skills/pane-browser && test ! -L /Users/example/.claude/skills/pane-browser && test ! -e /Users/example/.codex/skills/pane-browser && printf '\"'\"'Verified: shared skill and Claude link removed; no Codex-specific copy exists.\\n'\"'\"''";
    let key = AgentKey::new(Provider::Claude, "root", "approval-long");
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(ActivityEvent::Content {
            key: key.clone(),
            id: None,
            event: ExecutionEvent::Text {
                text: "Ready to clean up".into(),
            },
        }))
        .unwrap();
    tick(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_subject(thread, Subject::Subagent(key.clone()), window, cx)
        })
    });
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(ActivityEvent::Decision {
            subject: Some(Subject::Subagent(key)),
            decision: Decision {
                kind: Default::default(),
                policy: Default::default(),
                delivery: Default::default(),
                id: "long-approval".into(),
                tool_use_id: "long-approval".into(),
                tool_name: "commandExecution".into(),
                description: command.into(),
                suggestions: vec![],
                input: serde_json::json!({"command":command}),
            },
        }))
        .unwrap();
    tick(cx);
    let island = cx.debug_bounds("question-island").unwrap();
    let input = cx.debug_bounds("approval-input").unwrap();
    let serial = view.read_with(cx, |view, _| {
        view.cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .pending_decisions()[0]
            .handle
            .serial
    });
    let allow = bounds(cx, format!("request-allow-{}-{serial}", thread.get()));
    let deny = bounds(cx, format!("request-deny-{}-{serial}", thread.get()));
    let title = bounds(cx, format!("request-title-{}-{serial}", thread.get()));
    assert!(
        title.size.height >= px(crate::theme::FS_MD * crate::theme::LINE_UI * 3.),
        "the repro must retain a title spanning at least three lines: {title:?}"
    );
    assert!(
        title.bottom() <= input.top(),
        "the title wraps above the command: title={title:?} input={input:?}"
    );
    for (label, button) in [("Allow", allow), ("Deny", deny)] {
        assert!(
            island.contains(&button.origin) && island.contains(&button.bottom_right()),
            "{label} must sit inside the island: button={button:?} island={island:?}"
        );
        let gap = button.top() - input.bottom();
        assert!(
            gap >= px(0.) && gap <= px(16.),
            "the command-to-{label} gap must stay near the 12px design gap: {gap:?}"
        );
    }
}
