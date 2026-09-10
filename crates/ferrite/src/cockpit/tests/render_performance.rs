use super::*;

/// A render pass over the real Cockpit after an operator change. The counter
/// is reset after the pass that establishes the fixture, so it cannot mistake
/// cache population for work caused by the act under test.
fn reset_native_text_renders(cx: &mut gpui::VisualTestContext) {
    cx.update(|_, cx| crate::rich::testing::reset_renders(cx));
}

fn native_text_renders(prefix: &str, cx: &mut gpui::VisualTestContext) -> usize {
    cx.update(|_, cx| crate::rich::testing::renders_with_prefix(prefix, cx))
}

fn mounted_native_texts(prefix: &str, cx: &mut gpui::VisualTestContext) -> usize {
    cx.update(|_, cx| crate::rich::testing::rendered_identities_with_prefix(prefix, cx))
}

fn long_transcripts(fake: &Fake) {
    let streams = fake.streams.borrow();
    for (pane, stream) in streams.iter().enumerate() {
        stream
            .send(SessionEvent::TextDelta {
                text: (0..120)
                    .map(|line| format!("pane {pane} transcript line {line:03}\n\n"))
                    .collect(),
            })
            .unwrap();
    }
}

fn reasoning_rows(fake: &Fake, from: usize, to: usize) {
    let stream = fake.streams.borrow();
    for row in from..to {
        stream[0]
            .send(SessionEvent::ReasoningSummaryPart {
                item_id: format!("render-row-{row}"),
                summary_index: 0,
                // Four detail lines ensure an expanded row is taller than
                // 50px, so a 600px window has room for far fewer than 30.
                text: format!(
                    "retained transcript row {row:03}\n{}",
                    "detail line long enough to wrap a native transcript row\n".repeat(4)
                ),
                snapshot: false,
            })
            .unwrap();
    }
}

fn short_reasoning_rows(fake: &Fake, from: usize, to: usize) -> Vec<String> {
    let stream = fake.streams.borrow();
    (from..to)
        .map(|row| {
            let text = format!("logical selection row {row:03}");
            stream[0]
                .send(SessionEvent::ReasoningSummaryPart {
                    item_id: format!("logical-selection-row-{row}"),
                    summary_index: 0,
                    text: text.clone(),
                    snapshot: false,
                })
                .unwrap();
            text
        })
        .collect()
}

fn thinking_id(
    view: &gpui::Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
    row: usize,
) -> String {
    view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        let block = &view
            .cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&view.panes[0].selected)
            .unwrap()
            .transcript()
            .blocks()[row];
        assert!(matches!(&block.body, Body::Thinking(_)));
        format!("thinking-{}-{:?}", view.panes[0].text_namespace(), block.id)
    })
}

fn thinking_caret(
    view: &gpui::Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
    row: usize,
    byte: usize,
) -> gpui::Point<gpui::Pixels> {
    let id = thinking_id(view, cx, row);
    let text = thinking_details(view, cx, row);
    cx.update(|window, cx| {
        crate::rich::testing::caret(&id, 0, 1, &text, byte, window, cx)
            .expect("the jumped-to native Thinking row is mounted")
    })
}

/// The text a disclosed reasoning row actually mounts: the continuation of
/// its cut first line, since the header already shows the summary.
fn thinking_details(
    view: &gpui::Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
    row: usize,
) -> String {
    view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        let block = &view
            .cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&view.panes[0].selected)
            .unwrap()
            .transcript()
            .blocks()[row];
        let Body::Thinking(text) = &block.body else {
            unreachable!("fixture has one Thinking Block per virtual row")
        };
        crate::pane::reasoning_text(text)
            .1
            .unwrap_or_else(|| text.trim().to_owned())
    })
}

/// The prefix repeats leave the disclosed paragraph one word wider than the
/// shaping helper's grid at this width, so nudge the fixture until the two
/// agree: the reflow assertions below compare native positions, and they are
/// only meaningful where the helper models the same wrap.

fn wrapped_thinking_caret(
    view: &gpui::Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
    row: usize,
    byte: usize,
) -> gpui::Point<gpui::Pixels> {
    let id = thinking_id(view, cx, row);
    let text = view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        let block = &view
            .cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&view.panes[0].selected)
            .unwrap()
            .transcript()
            .blocks()[row];
        let Body::Thinking(text) = &block.body else {
            unreachable!("fixture has one Thinking Block per virtual row")
        };
        text.trim().to_owned()
    });
    cx.update(|window, cx| {
        crate::rich::testing::wrapped_caret(&id, &text, byte, window, cx)
            .expect("the wrapped native Thinking row is mounted")
    })
}

fn expand_reasoning_rows(view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    let rows = view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        view.cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&view.panes[0].selected)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .filter(|block| matches!(&block.body, Body::Thinking(_)))
            .map(|block| block.id)
            .collect::<Vec<_>>()
    });
    view.update(cx, |view, cx| {
        for row in rows {
            let disclosure = crate::pane::DisclosureId::Reasoning(row);
            if !view.panes[0].tool_expanded(disclosure.clone()) {
                view.panes[0].toggle_tool(&disclosure);
            }
        }
        cx.notify();
    });
    cx.run_until_parked();
}

#[gpui::test]
fn typing_in_a_focused_composer_does_not_render_sibling_native_transcripts(
    cx: &mut TestAppContext,
) {
    let (mut core, fake) = cockpit("render-composer-isolation", 3);
    let group = group_all(&mut core);
    long_transcripts(&fake);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1800.), px(800.)));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);
    tick(cx);

    // The first Pane owns the keyboard before the observed edit. The two
    // warmed siblings have long, actual native Markdown documents.
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(
            view.panes[0]
                .composer
                .read(cx)
                .focus_handle(cx)
                .is_focused(window),
            "the focused Pane's Composer owns the keyboard before typing"
        );
    });
    let siblings = view.read_with(cx, |view, _| {
        view.panes[1..]
            .iter()
            .map(|pane| format!("markdown-{}-", pane.text_namespace()))
            .collect::<Vec<_>>()
    });

    // GPUI changes hover/focus-visible styling once on mouse-to-keyboard
    // transition. Measure ordinary typing after that global state has settled.
    cx.simulate_keystrokes("left");
    cx.run_until_parked();
    reset_native_text_renders(cx);
    cx.simulate_input("x");
    cx.run_until_parked();

    assert!(
        siblings
            .iter()
            .all(|prefix| native_text_renders(prefix, cx) == 0),
        "typing in one Composer must not reconstruct a sibling transcript's native text"
    );
    assert_eq!(composer_text(&view, cx), "x");
}

#[gpui::test]
fn streaming_one_pane_does_not_render_sibling_native_transcripts(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("render-stream-isolation", 3);
    let group = group_all(&mut core);
    long_transcripts(&fake);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1800.), px(800.)));
    view.update(cx, |view, cx| {
        view.enter_group(group, cx);
        view.focus_pane(0);
    });
    tick(cx);
    tick(cx);
    tick(cx);
    // Flush the initial native parser's notification before measuring a new
    // stream event against the warmed scene.
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    let focused_prefix = view.read_with(cx, |view, _| {
        format!("markdown-{}-", view.panes[0].text_namespace())
    });
    let siblings = view.read_with(cx, |view, _| {
        view.panes[1..]
            .iter()
            .map(|pane| format!("markdown-{}-", pane.text_namespace()))
            .collect::<Vec<_>>()
    });

    reset_native_text_renders(cx);
    let streamed_line = "the focused Pane streamed one more ordinary line";
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: format!("{streamed_line}\n\n"),
        })
        .unwrap();
    tick(cx);

    assert!(
        native_text_renders(&focused_prefix, cx) > 0,
        "streaming must update the focused Pane's mounted native text wrapper"
    );
    assert!(
        siblings
            .iter()
            .all(|prefix| native_text_renders(prefix, cx) == 0),
        "streaming one Pane must not reconstruct sibling transcript native text"
    );
    assert!(
        cx.update(|_, cx| crate::rich::testing::full_text(&focused_prefix, cx))
            .is_some_and(|text| text.contains(streamed_line)),
        "the focused native parser must contain the streamed text"
    );
}

#[gpui::test]
fn retained_transcript_relative_file_links_use_the_thread_workspace_and_copy_text(
    cx: &mut TestAppContext,
) {
    let (core, fake, workspace) = bound_cockpit("retained-relative-file-link", Provider::Claude);
    let file = workspace.join("docs").join("guide.md");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "guide\n").unwrap();
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(600.)));

    let suffix = " after reading the detailed notes about this change and the next steps.";
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: format!("Before [guide](docs/guide.md:12){suffix}"),
        })
        .unwrap();
    tick(cx);

    let card = debug_bounds(cx, format!("file-attachment-{}", file.display()))
        .expect("the retained transcript resolves the relative link from its Thread workspace");
    cx.simulate_click(card.center(), gpui::Modifiers::none());
    assert_eq!(
        cx.opened_url(),
        Some(url::Url::from_file_path(&file).unwrap().to_string()),
        "the file card opens the Thread-workspace file, without its line suffix"
    );

    let id = view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        let block = &view.cockpit.thread(thread).unwrap().transcript().blocks()[0];
        format!(
            "markdown-{}-{:?}",
            view.panes[0].text_namespace(),
            block.markdown_run.unwrap_or(block.id)
        )
    });
    let before = cx.update(|_, cx| crate::rich::testing::bounds(&id, 0, cx).unwrap());
    assert!(
        before.size.height < px(35.),
        "the wide paragraph occupies one line: {before:?}"
    );
    let start = caret(&view, cx, 0, 0);
    // Select through the last word, leaving the final period unselected. The
    // native card has its own width, so measure the suffix from its right edge.
    let end = cx.update(|window, cx| {
        let suffix_caret =
            crate::rich::testing::caret(&id, 0, 1, suffix, suffix.len() - 1, window, cx).unwrap();
        gpui::point(
            card.right() + suffix_caret.x - before.left(),
            card.center().y,
        )
    });
    cx.simulate_mouse_down(start, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(end, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    let selected = format!("Before guide{}", suffix.trim_end_matches('.'));
    assert_eq!(
        clipboard(cx).as_deref(),
        Some(selected.as_str()),
        "the native file card remains selectable as its rendered link label"
    );

    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("stale".into())));
    cx.simulate_resize(gpui::size(px(700.), px(600.)));
    cx.run_until_parked();
    let after = cx.update(|_, cx| crate::rich::testing::bounds(&id, 0, cx).unwrap());
    assert!(
        after.size.height > before.size.height + px(10.),
        "the custom-link paragraph must rewrap into more fragments: {before:?} -> {after:?}"
    );
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(
        clipboard(cx).as_deref(),
        Some(selected.as_str()),
        "native inline fragments retain the same source bytes and file label through reflow"
    );
}

#[gpui::test]
fn native_transcript_rows_stay_bounded_by_a_fixed_viewport_as_history_grows(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("render-viewport-budget", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(600.)));
    reasoning_rows(&fake, 0, 30);
    tick(cx);
    expand_reasoning_rows(&view, cx);

    let short_prefix = view.read_with(cx, |view, _| {
        format!("thinking-{}-", view.panes[0].text_namespace())
    });
    reset_native_text_renders(cx);
    cx.simulate_resize(gpui::size(px(1001.), px(600.)));
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        let transcript = view.panes[0].transcript().unwrap();
        let viewport = transcript.read(cx).scroll().bounds();
        assert!(
            viewport.size.height > px(250.),
            "a real viewport must be allocated: {viewport:?}"
        );
        assert!(viewport.size.width > px(400.));
    });
    let short_history_rows = mounted_native_texts(&short_prefix, cx);

    reasoning_rows(&fake, 30, 200);
    tick(cx);
    let retained_thinking_rows = view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        view.cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&view.panes[0].selected)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .filter(|block| matches!(&block.body, Body::Thinking(_)))
            .count()
    });
    assert_eq!(
        retained_thinking_rows, 200,
        "the long-history viewport measurement requires all 200 retained Thinking rows"
    );
    expand_reasoning_rows(&view, cx);
    let long_prefix = view.read_with(cx, |view, _| {
        format!("thinking-{}-", view.panes[0].text_namespace())
    });
    reset_native_text_renders(cx);
    cx.simulate_resize(gpui::size(px(1000.), px(600.)));
    cx.run_until_parked();
    let long_history_rows = mounted_native_texts(&long_prefix, cx);

    // The scrollback is shorter than 600px after the Pane head and Composer.
    // Every expanded fixture row exceeds 50px, so 24 is a deliberately loose
    // mounted-row ceiling for this fixed viewport, not a history-cap mirror.
    const MAX_MOUNTED_ROWS: usize = 24;

    eprintln!(
        "viewport native rows: history=30 mounted={short_history_rows}; history=200 mounted={long_history_rows}"
    );

    assert!(
        short_history_rows > 0,
        "the short fixture renders native rows"
    );
    assert!(short_history_rows <= MAX_MOUNTED_ROWS);
    assert!(
        long_history_rows > 0,
        "the long fixture renders native rows"
    );
    assert!(
        long_history_rows <= MAX_MOUNTED_ROWS,
        "a fixed transcript viewport mounted {long_history_rows} native rows"
    );
    assert!(
        long_history_rows <= short_history_rows + 2,
        "growing retained history from 30 to 200 rows scaled mounted native text from {short_history_rows} to {long_history_rows}"
    );
}

#[gpui::test]
fn dragging_across_virtualized_thinking_rows_copies_the_logical_range(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("virtual-logical-selection", 1);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(600.)));
    let rows = short_reasoning_rows(&fake, 0, 160);
    tick(cx);

    // `ListState` owns the scroll. Freeze tail following before jumping into
    // retained history, or the live tail would replace our logical anchor.
    reset_native_text_renders(cx);
    view.update(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .update(cx, |transcript, cx| {
                cx.notify();
                transcript.scroll().pause_following_tail();
                transcript
                    .scroll()
                    .list_state()
                    .scroll_to(gpui::ListOffset {
                        item_ix: 25,
                        offset_in_item: px(0.),
                    });
            });
        cx.notify();
    });
    cx.run_until_parked();

    let row_75 = thinking_id(&view, cx, 75);
    let from = thinking_caret(&view, cx, 25, 0);
    cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());

    // Replacing the mounted range while the drag is held must retain a
    // logical endpoint. Rows 26..=129 are never all native TextViews.
    view.update(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .update(cx, |transcript, cx| {
                cx.notify();
                transcript
                    .scroll()
                    .list_state()
                    .scroll_to(gpui::ListOffset {
                        item_ix: 130,
                        offset_in_item: px(0.),
                    });
            });
        cx.notify();
    });
    cx.run_until_parked();
    let to = thinking_caret(&view, cx, 130, rows[130].len());
    cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");

    assert_eq!(
        clipboard(cx).as_deref(),
        Some(rows[25..=130].join("\n\n").as_str()),
        "logical selection copies every retained row, including never-mounted intermediates"
    );
    assert_eq!(
        native_text_renders(&row_75, cx),
        0,
        "the full copy must not mount row 75 to reconstruct the range"
    );

    // Both endpoints now leave the viewport, while retained membership and
    // the logical copy remain stable.
    view.update(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .update(cx, |transcript, cx| {
                cx.notify();
                transcript
                    .scroll()
                    .list_state()
                    .scroll_to(gpui::ListOffset {
                        item_ix: 0,
                        offset_in_item: px(0.),
                    });
            });
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(
        clipboard(cx).as_deref(),
        Some(rows[25..=130].join("\n\n").as_str()),
        "viewport unmounting must not clear a retained logical selection"
    );
}

#[gpui::test]
fn partial_thinking_selection_survives_a_wrapping_resize(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("selection-resize-reflow", 1);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(600.)));

    let selected = "βeta🙂 exact selection";
    let text = format!(
        "{} {selected} {}",
        "prefix that forces wrapped native geometry ".repeat(20),
        "suffix that keeps the same native row long ".repeat(12),
    );
    fake.streams.borrow()[0]
        .send(SessionEvent::ReasoningSummaryPart {
            item_id: "resize-reflow".into(),
            summary_index: 0,
            text: text.clone(),
            snapshot: false,
        })
        .unwrap();
    tick(cx);
    expand_reasoning_rows(&view, cx);

    // A reasoning row's disclosure opens on the continuation of its cut
    // first line, so these offsets are into the disclosed body rather than
    // the whole thought.
    let details = thinking_details(&view, cx, 0);
    // The disclosure opens on the continuation of the row's cut first line,
    // so these offsets are into the disclosed body. The selection runs from
    // the phrase to the row's end: a caret past the last byte clamps there,
    // where a caret mid-paragraph would ride the shaping helper's wrap grid
    // rather than the row's own.
    let start = details
        .find(selected)
        .expect("the cut line's tail carries it");
    let end = details.len();
    let expected = details[start..].to_owned();
    let from = wrapped_thinking_caret(&view, cx, 0, start);
    let to = wrapped_thinking_caret(&view, cx, 0, end);
    let id = thinking_id(&view, cx, 0);
    let before_bounds = cx.update(|_, cx| crate::rich::testing::bounds(&id, 0, cx).unwrap());
    view.read_with(cx, |view, cx| {
        let viewport = view.panes[0]
            .transcript()
            .unwrap()
            .read(cx)
            .scroll()
            .bounds();
        assert!(
            viewport.contains(&from) && viewport.contains(&to),
            "the selected wrapped range must be visible before reflow: {viewport:?}"
        );
    });
    cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some(expected.as_str()));

    // The selected endpoint is already below the first wrapped line. After
    // a narrower resize it must acquire a different native screen position.
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("stale".into())));
    // Both window sizes keep the Pane above the Transcript detail threshold.
    cx.simulate_resize(gpui::size(px(700.), px(600.)));
    cx.run_until_parked();
    let reflowed_from = wrapped_thinking_caret(&view, cx, 0, start);
    let after_bounds = cx.update(|_, cx| crate::rich::testing::bounds(&id, 0, cx).unwrap());
    assert!(after_bounds.size.width < before_bounds.size.width - px(100.));
    assert!(
        reflowed_from.y - after_bounds.top() > from.y - before_bounds.top() + px(10.),
        "the selected byte moved to a later wrapped native line: {from:?} -> {reflowed_from:?}"
    );
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(
        clipboard(cx).as_deref(),
        Some(expected.as_str()),
        "reflowing a native Thinking row preserves its exact partial selection"
    );
}

#[gpui::test]
fn replacing_an_offscreen_selected_thinking_row_clears_only_that_selection(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("selection-snapshot-replacement", 1);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(600.)));
    let rows = short_reasoning_rows(&fake, 0, 160);
    tick(cx);

    view.update(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .update(cx, |transcript, cx| {
                cx.notify();
                transcript.scroll().pause_following_tail();
                transcript
                    .scroll()
                    .list_state()
                    .scroll_to(gpui::ListOffset {
                        item_ix: 25,
                        offset_in_item: px(0.),
                    });
            });
        cx.notify();
    });
    cx.run_until_parked();

    let selected = "row 025";
    let start = rows[25].find(selected).unwrap();
    let end = start + selected.len();
    let from = thinking_caret(&view, cx, 25, start);
    let to = thinking_caret(&view, cx, 25, end);
    cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some(selected));

    // Leave both the row and its native text wrapper outside the viewport.
    view.update(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .update(cx, |transcript, cx| {
                cx.notify();
                transcript
                    .scroll()
                    .list_state()
                    .scroll_to(gpui::ListOffset {
                        item_ix: 130,
                        offset_in_item: px(0.),
                    });
            });
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| gpui::base::TextSelection::has_selection(window, cx)));
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("stale".into())));
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(
        clipboard(cx).as_deref(),
        Some(selected),
        "ordinary viewport unmounting keeps the exact logical selection"
    );

    // Replacing another retained row must not invalidate this endpoint.
    fake.streams.borrow()[0]
        .send(SessionEvent::ReasoningSummaryPart {
            item_id: "logical-selection-row-140".into(),
            summary_index: 0,
            text: "an unrelated snapshot replacement".into(),
            snapshot: true,
        })
        .unwrap();
    tick(cx);
    assert!(cx.update(|window, cx| gpui::base::TextSelection::has_selection(window, cx)));
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("stale".into())));
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some(selected));

    // This snapshot keeps the block identity but replaces its source, so a
    // partial endpoint within it has no valid old-text position to retain.
    let replacement = "replacement source invalidates the old endpoint";
    fake.streams.borrow()[0]
        .send(SessionEvent::ReasoningSummaryPart {
            item_id: "logical-selection-row-25".into(),
            summary_index: 0,
            text: replacement.into(),
            snapshot: true,
        })
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        let block = &view
            .cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&view.panes[0].selected)
            .unwrap()
            .transcript()
            .blocks()[25];
        assert!(matches!(&block.body, Body::Thinking(text) if text == replacement));
    });
    assert!(
        !cx.update(|window, cx| gpui::base::TextSelection::has_selection(window, cx)),
        "replacing the selected logical member clears native selection"
    );
    assert!(
        cx.update(|window, cx| { gpui::base::TextSelection::selected_text(window, cx).is_empty() })
    );
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("unchanged".into())));
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(
        clipboard(cx).as_deref(),
        Some("unchanged"),
        "copy cannot recover stale text from the replaced offscreen member"
    );
}
