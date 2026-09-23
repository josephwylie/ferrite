use super::*;
use crate::theme;

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

/// Aim at a byte of a disclosed Thinking row's body through its native wrap.
/// The offsets are into the disclosed body (`thinking_details`), so that is
/// the text the helper shapes; shaping the whole thought would wrap a
/// different string and aim off the row's own lines.
fn wrapped_thinking_caret(
    view: &gpui::Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
    row: usize,
    byte: usize,
) -> gpui::Point<gpui::Pixels> {
    let id = thinking_id(view, cx, row);
    let text = thinking_details(view, cx, row);
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
fn streaming_without_drawing_releases_temporary_transcript_elements(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("stream-without-drawing", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let cache = view.read_with(cx, |view, _| view.panes[0].rich.clone());
    let before = cache.retained_handles();
    // Provider updates continue while a window is occluded. Do not run a
    // draw between these updates: temporary metadata must not need a frame
    // to release its copies of the growing answer and native text cache.
    cx.update(|_, cx| {
        for _ in 0..100 {
            fake.streams.borrow()[0]
                .send(SessionEvent::TextDelta {
                    text: "streamed text\n\n".into(),
                })
                .unwrap();
            view.update(cx, |view, cx| {
                view.pump(cx);
                view.sync_visible_transcripts(cx);
            });
        }
    });
    assert_eq!(
        cache.retained_handles(),
        before,
        "stream updates must release all temporary native text handles"
    );
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

    // Short enough to sit on one line of the 720px reading column.
    let suffix = " after reading the notes on this change.";
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
    // native card has its own width and the flow reserves its trailing
    // `INLINE_CODE_OVERHANG` margin, so measure the suffix from there.
    let end = cx.update(|window, cx| {
        let suffix_caret =
            crate::rich::testing::caret(&id, 0, 1, suffix, suffix.len() - 1, window, cx).unwrap();
        gpui::point(
            card.right() + px(crate::theme::INLINE_CODE_OVERHANG) + suffix_caret.x - before.left(),
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
    // The narrowest window: its column is narrower than the line.
    cx.simulate_resize(gpui::size(px(crate::theme::WINDOW_MIN_W), px(600.)));
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
    super::hold_nav_open(&view, cx);
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

/// Reading preference changes layout, not the native source/selection identity.
/// Group rendering always keeps its compact type, including after fullscreen.
#[gpui::test]
fn solo_reading_size_reflows_without_replacing_text_or_selection(cx: &mut TestAppContext) {
    use ferrite_core::settings::SoloReadingSize;
    let (mut core, fake) = cockpit("solo-reading-size", 2);
    let group = group_all(&mut core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1100.), px(700.)));
    for stream in fake.streams.borrow().iter() {
        stream
            .send(SessionEvent::TextDelta {
                text: "A readable answer with **meaningful emphasis** and exact source text."
                    .into(),
            })
            .unwrap();
    }
    tick(cx);
    let index = view.read_with(cx, |view, _| view.focused());
    let prefix = view.read_with(cx, |view, _| {
        format!("markdown-{}-", view.panes[index].text_namespace())
    });
    let (identity, selected) = cx.update(|_, cx| {
        assert_eq!(crate::rich::testing::font_size(&prefix, cx), Some(px(14.)));
        (
            crate::rich::testing::first_entity(&prefix, cx).unwrap(),
            crate::rich::testing::full_text(&prefix, cx).unwrap(),
        )
    });
    view.update(cx, |view, cx| {
        view.prefs.settings.solo_reading_size = SoloReadingSize::Large;
        cx.notify();
    });
    tick(cx);
    cx.update(|_, cx| {
        assert_eq!(crate::rich::testing::font_size(&prefix, cx), Some(px(18.)));
        assert_eq!(
            crate::rich::testing::first_entity(&prefix, cx),
            Some(identity)
        );
        assert_eq!(
            crate::rich::testing::selected_text(&prefix, cx),
            Some(selected.clone())
        );
    });
    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);
    cx.update(|_, cx| {
        assert_eq!(crate::rich::testing::font_size(&prefix, cx), Some(px(14.)));
        assert_eq!(
            crate::rich::testing::first_entity(&prefix, cx),
            Some(identity)
        );
    });
    view.update(cx, |view, cx| {
        view.focus_pane(index);
        view.cockpit.toggle_fullscreen();
        cx.notify();
    });
    tick(cx);
    cx.update(|_, cx| {
        assert_eq!(crate::rich::testing::font_size(&prefix, cx), Some(px(18.)));
        assert_eq!(
            crate::rich::testing::first_entity(&prefix, cx),
            Some(identity)
        );
    });
}

#[gpui::test]
fn keyboard_reaches_code_actions_after_disclosures_and_returns_to_the_draft(
    cx: &mut TestAppContext,
) {
    fn enter(cx: &mut gpui::VisualTestContext) {
        cx.simulate_keystrokes("enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
        cx.run_until_parked();
    }
    let (mut core, fake) = cockpit("code-actions-keyboard", 1);
    let thread = core.threads()[0];
    core.send(thread, "prior prompt".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    let stream = fake.streams.borrow();
    stream[0]
        .send(SessionEvent::ReasoningSummaryPart {
            item_id: "reading-details".into(),
            summary_index: 0,
            text: "Checked the implementation\nThe details remain available.".into(),
            snapshot: false,
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::TextDelta {
            text: "```rust\n    first();\n```\n\n```html\n<p>second</p>\n```".into(),
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
    cx.simulate_input("unsent draft");
    cx.simulate_keystrokes("tab");
    view.read_with(cx, |view, _| assert!(view.panes[0].has_tool_target()));
    cx.simulate_keystrokes("tab");
    assert!(cx.update(|window, _| crate::rich::code_actions_focused(window)));
    enter(cx);
    assert_eq!(clipboard(cx).as_deref(), Some("    first();"));
    // HTML has Preview before Copy in the same native tab order.
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    enter(cx);
    assert_eq!(clipboard(cx).as_deref(), Some("<p>second</p>"));
    assert_eq!(fake.sent.borrow().as_slice(), ["prior prompt"]);
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| {
        let pane = &view.read(cx).panes[0];
        assert!(pane.composer.focus_handle(cx).is_focused(window));
        assert_eq!(pane.composer.read(cx).text(), "unsent draft");
    });
    cx.simulate_keystrokes("shift-tab");
    enter(cx);
    assert_eq!(clipboard(cx).as_deref(), Some("<p>second</p>"));
    cx.simulate_keystrokes("shift-tab");
    cx.simulate_keystrokes("shift-tab");
    enter(cx);
    assert_eq!(clipboard(cx).as_deref(), Some("    first();"));
    cx.simulate_keystrokes("shift-tab");
    view.read_with(cx, |view, _| assert!(view.panes[0].has_tool_target()));
    cx.simulate_keystrokes("shift-tab");
    cx.update(|window, cx| {
        assert!(view.read(cx).panes[0]
            .composer
            .focus_handle(cx)
            .is_focused(window));
    });
    assert_eq!(fake.sent.borrow().as_slice(), ["prior prompt"]);
}

#[gpui::test]
fn code_copy_traversal_does_not_accept_an_empty_composers_followup(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("code-copy-offered-followup", 1);
    let thread = core.threads()[0];
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "```rust\n    first();\n```".into(),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    core.pump();
    core.deliver_suggestion(thread, "Run the tests".into());
    core.pump();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    cx.update(|_, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("preserved clipboard".into()))
    });

    cx.simulate_keystrokes("shift-tab");
    assert!(cx.update(|window, _| crate::rich::code_actions_focused(window)));
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| {
        let view = view.read(cx);
        let composer = &view.panes[0].composer;
        assert!(
            composer.focus_handle(cx).is_focused(window),
            "Tab leaves Copy for the input"
        );
        assert!(
            composer.read(cx).is_empty(),
            "leaving Copy cannot accept ghost text"
        );
        assert_eq!(
            view.cockpit.thread(thread).unwrap().suggestion(),
            Some("Run the tests")
        );
    });
    assert_eq!(clipboard(cx).as_deref(), Some("preserved clipboard"));
    assert!(fake.sent.borrow().is_empty());

    // The next Tab is actually from the input, so the offered text remains
    // available to accept through its intended interaction.
    cx.simulate_keystrokes("tab");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes[0].composer.read(cx).text(), "Run the tests");
    });
    assert_eq!(clipboard(cx).as_deref(), Some("preserved clipboard"));
    assert!(fake.sent.borrow().is_empty(), "accepting is still unsent");
}

#[gpui::test]
fn answer_gutter_and_padding_survive_wrapping_resize(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("answer-layout-geometry", 1);
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "A paragraph with enough words to wrap in a narrow pane. ".repeat(5),
        })
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    super::hold_nav_open(&view, cx);
    let mut heights = Vec::new();
    for width in [1200., 700.] {
        cx.simulate_resize(gpui::size(px(width), px(800.)));
        tick(cx);
        tick(cx);
        let answer = debug_bounds(cx, "transcript-answer".into()).unwrap();
        let text = view.read_with(cx, |view, cx| {
            let thread = view.panes[0].thread().unwrap();
            let block = &view.cockpit.thread(thread).unwrap().transcript().blocks()[0];
            let id = format!(
                "markdown-{}-{:?}",
                view.panes[0].text_namespace(),
                block.markdown_run.unwrap_or(block.id)
            );
            crate::rich::testing::bounds(&id, 0, cx).unwrap()
        });
        let mark = debug_bounds(cx, "answer-mark".into()).unwrap();
        for delta in [
            // Prose starts on C1, the one content edge.
            text.left() - answer.left() - px(theme::GUTTER_W),
            // The row owns no padding: the list's gap table spaces rows.
            text.top() - answer.top(),
            answer.bottom() - text.bottom(),
            answer.right() - text.right(),
            // The mark's glyph box hangs at the row's left edge, centred on
            // the first prose line box.
            mark.left() - answer.left(),
            mark.size.width - px(theme::GLYPH_BOX),
            (mark.top() + mark.size.height / 2.) - (text.top() + px(theme::LH_PROSE / 2.)),
        ] {
            assert!(
                delta.abs() <= px(1.),
                "answer/text geometry differs: {answer:?} {text:?}"
            );
        }
        heights.push(text.size.height);
    }
    assert!(
        heights[1] > heights[0],
        "narrower Markdown must wrap naturally"
    );
}

/// Reproducible CPU layout probe; uses synthetic sessions and no local logs.
/// Run with `cargo test -p ferrite streaming_layout_probe -- --ignored --nocapture`.
#[gpui::test]
#[ignore = "local performance probe"]
fn streaming_layout_probe(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("streaming-layout-probe", 4);
    let group = group_all(&mut core);
    long_transcripts(&fake);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    for _ in 0..4 {
        tick(cx);
    }
    let started = std::time::Instant::now();
    for _ in 0..60 {
        for stream in fake.streams.borrow().iter() {
            stream
                .send(SessionEvent::TextDelta {
                    text: "more words ".into(),
                })
                .unwrap();
        }
        tick(cx);
    }
    eprintln!(
        "STREAM_LAYOUT iterations=60 panes=4 elapsed_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.
    );
    let prefix = view.read_with(cx, |view, _| {
        format!("markdown-{}-", view.panes[0].text_namespace())
    });
    assert!(cx
        .update(|_, cx| crate::rich::testing::full_text(&prefix, cx))
        .is_some_and(|text| text.contains("more words ".repeat(60).trim_end())));
}

/// Delivers the display frame the last draw asked for, and says how many
/// animation-frame requests were waiting on it.
fn display_frames(cx: &mut gpui::VisualTestContext) -> usize {
    let frames = cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    frames
}

/// First paint settles (measurement passes, springs arriving at rest) over
/// a few frames; deliver them until the window asks for none.
fn settle(cx: &mut gpui::VisualTestContext) {
    for _ in 0..64 {
        if display_frames(cx) == 0 {
            return;
        }
        cx.executor().advance_clock(Duration::from_millis(16));
    }
    panic!("the window never stopped asking for frames");
}

fn pulse_parked(cx: &mut gpui::VisualTestContext) -> bool {
    cx.update(|_, cx| crate::motion::pulse_parked(cx))
}

/// The motion kit's budget: with nothing animating, a window asks for no
/// display frame and arms no clock, however long it sits.
#[gpui::test]
fn an_idle_window_with_the_motion_kit_schedules_no_animation_frames(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let (core, _fake) = cockpit("motion-idle", 2);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    settle(cx);
    assert_eq!(display_frames(cx), 0, "an idle window asks for no frame");
    assert!(pulse_parked(cx), "and holds no pulse lease");
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(display_frames(cx), 0, "time passing changes nothing");
    assert!(pulse_parked(cx));
}

/// A working Thread's loops — the working line's mark, the nav's breathing
/// dot — ride the shared pulse clock instead of asking for every display
/// frame, and the clock parks once the turn ends and its lease lapses.
#[gpui::test]
fn a_working_thread_loops_on_the_pulse_clock_and_parks_when_it_ends(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let (mut core, fake) = cockpit("motion-working", 1);
    let thread = core.threads()[0];
    core.send(thread, "Inspect progress".into());
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::ReasoningSummaryDelta {
            text: "**Checking marks**".into(),
            summary_index: 0,
        })
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("progress-mark-live").is_some());
    assert!(!pulse_parked(cx), "the working mark leases the clock");
    settle(cx);
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    assert!(!pulse_parked(cx), "and keeps it while it is mounted");
    assert_eq!(
        display_frames(cx),
        0,
        "the loops ask the clock for ~30fps, never the display for every frame"
    );

    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("progress-mark-live").is_none());
    cx.executor().advance_clock(Duration::from_millis(
        crate::theme::MOTION_PULSE_LEASE_MS + 2 * crate::theme::MOTION_PULSE_TICK_MS,
    ));
    cx.run_until_parked();
    assert!(pulse_parked(cx), "the lapsed clock parks");
    // The turn's completion toast arrives on its own springs; once they
    // rest, the window is idle again.
    settle(cx);
    assert!(pulse_parked(cx));
}

/// A pulse tick repaints what paints the loop and nothing cached beside it:
/// the working mark animates while the Pane's retained transcript — the
/// expensive native text — is reused from its cache on every tick.
#[gpui::test]
fn a_pulse_tick_leaves_the_cached_transcript_untouched(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let (mut core, fake) = cockpit("motion-tick-isolation", 1);
    let thread = core.threads()[0];
    core.send(thread, "Inspect progress".into());
    long_transcripts(&fake);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    tick(cx);
    tick(cx);
    settle(cx);
    assert!(cx.debug_bounds("progress-mark-live").is_some());
    let prefix = view.read_with(cx, |view, _| {
        format!("markdown-{}-", view.panes[0].text_namespace())
    });
    assert!(
        mounted_native_texts(&prefix, cx) > 0,
        "the premise: the transcript's native text is mounted"
    );

    reset_native_text_renders(cx);
    for _ in 0..10 {
        cx.executor()
            .advance_clock(Duration::from_millis(crate::theme::MOTION_PULSE_TICK_MS));
        cx.run_until_parked();
    }
    assert!(!pulse_parked(cx), "the mark kept the clock running");
    assert_eq!(
        native_text_renders(&prefix, cx),
        0,
        "ten ticks rebuilt none of the cached transcript's native text"
    );
    assert_eq!(display_frames(cx), 0, "and asked the display for nothing");
}

/// Print, don't perform (rule 2.10.1): a tool row appended while the
/// operator watches lands on the frame it arrives, stamped for no entrance
/// and asking the display for nothing; the turn's first answer block is
/// turn-level and enters on `motion::ROW_IN` (180ms, opacity only), then
/// asks for nothing; a later answer block in the same turn is printed.
#[gpui::test]
fn an_appended_tool_row_schedules_no_animation_frames(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let (mut core, fake) = cockpit("motion-arrival", 1);
    let thread = core.threads()[0];
    core.send(thread, "Inspect progress".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    tick(cx);
    tick(cx);
    settle(cx);
    let arrivals = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.panes[0]
                .transcript()
                .map_or(0, |transcript| transcript.read(cx).arrivals())
        })
    };
    let send = |event: SessionEvent| fake.streams.borrow()[0].send(event).unwrap();

    send(SessionEvent::ToolStarted {
        id: "arrival-run".into(),
        name: "Bash".into(),
        input: serde_json::json!({"command": "cargo test"}),
    });
    tick(cx);
    assert_eq!(arrivals(cx), 0, "a tool row is printed, not staged");
    // At most the list's one measurement pass for the new row; then no
    // frame at all across what an entrance would have taken.
    assert!(
        display_frames(cx) <= 1,
        "the list measures the new row once"
    );
    let quiet = |cx: &mut gpui::VisualTestContext| {
        let mut frames = 0;
        for _ in 0..(crate::theme::MOTION_ROW_IN_MS / 16 + 1) {
            cx.executor().advance_clock(Duration::from_millis(16));
            frames += display_frames(cx);
        }
        frames
    };
    assert_eq!(quiet(cx), 0, "and asks the display for nothing");

    send(SessionEvent::TextDelta {
        text: "First answer of the turn.".into(),
    });
    tick(cx);
    assert_eq!(arrivals(cx), 1, "the turn's first answer block is stamped");
    assert!(display_frames(cx) > 0, "and fades in");
    cx.executor().advance_clock(Duration::from_millis(16));
    assert!(
        display_frames(cx) > 0,
        "still fading 16ms in: the entrance, not a measurement pass"
    );
    cx.executor()
        .advance_clock(Duration::from_millis(crate::theme::MOTION_ROW_IN_MS));
    settle(cx);
    assert_eq!(display_frames(cx), 0, "landed: no more frames");

    send(SessionEvent::ToolStarted {
        id: "arrival-run-2".into(),
        name: "Bash".into(),
        input: serde_json::json!({"command": "cargo check"}),
    });
    send(SessionEvent::TextDelta {
        text: "\n\nA later answer block.".into(),
    });
    tick(cx);
    assert_eq!(
        arrivals(cx),
        0,
        "a later answer block in the same turn is printed"
    );
    assert!(
        display_frames(cx) <= 1,
        "the list measures the new rows once"
    );
    assert_eq!(quiet(cx), 0, "and nothing enters");
}

/// One clock, one breath (rule 2.10.8): N working Panes and N unread Panes
/// on one board cost at most the pulse clock's 1000/33 ticks a second, and
/// never a display frame of their own.
#[gpui::test]
fn working_and_unread_panes_cost_at_most_one_pulse_tick_rate(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let (mut core, fake) = cockpit("motion-breath-budget", 4);
    let threads = core.threads().to_vec();
    for thread in &threads {
        core.send(*thread, "Inspect progress".into());
    }
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    for stream in 0..2 {
        fake.streams.borrow()[stream]
            .send(SessionEvent::ReasoningSummaryDelta {
                text: "**Checking marks**".into(),
                summary_index: 0,
            })
            .unwrap();
    }
    for stream in 2..4 {
        for event in [
            SessionEvent::TextDelta {
                text: "Done here.".into(),
            },
            SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            },
        ] {
            fake.streams.borrow()[stream].send(event).unwrap();
        }
    }
    tick(cx);
    settle(cx);
    assert!(
        cx.debug_bounds("progress-mark-live").is_some(),
        "the premise: a working mark is on screen"
    );
    assert!(
        cx.debug_bounds("breathing-dot").is_some(),
        "the premise: an unread dot breathes"
    );
    let before = cx.update(|_, _| crate::motion::testing::pulse_ticks());
    let step = Duration::from_millis(11);
    let mut elapsed = Duration::ZERO;
    while elapsed < Duration::from_secs(1) {
        cx.executor().advance_clock(step);
        cx.run_until_parked();
        assert_eq!(
            display_frames(cx),
            0,
            "the loops never ask the display for a frame"
        );
        elapsed += step;
    }
    let ticks = cx.update(|_, _| crate::motion::testing::pulse_ticks()) - before;
    assert!(ticks > 0, "the loops are alive: {ticks}");
    assert!(
        ticks <= (1000 / crate::theme::MOTION_PULSE_TICK_MS) as usize,
        "{ticks} ticks in one second: more than 1000/33"
    );
}

fn nav_column_width(cx: &mut gpui::VisualTestContext) -> f32 {
    f32::from(
        cx.debug_bounds("nav-column")
            .expect("the nav column")
            .size
            .width,
    )
}

/// cmd-B is instant (rule 2.10.2): the column lands at its new width on
/// the toggle frame and schedules no frame after it, either way.
#[gpui::test]
fn the_nav_collapse_lands_on_the_toggle_frame_and_schedules_nothing(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let (core, _fake) = cockpit("motion-nav", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    settle(cx);
    assert_eq!(nav_column_width(cx), nav::WIDTH, "first paint: the column");

    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    cx.run_until_parked();
    assert_eq!(nav_column_width(cx), nav::RAIL_WIDTH, "the toggle frame");
    assert_eq!(display_frames(cx), 0, "and nothing after it");

    view.update(cx, |view, cx| view.set_nav_collapsed(false, cx));
    cx.run_until_parked();
    assert_eq!(nav_column_width(cx), nav::WIDTH, "back on the toggle frame");
    assert_eq!(display_frames(cx), 0, "and nothing after it");
}

/// Reduced motion: the column lands at its new width at once.
#[gpui::test]
fn reduced_motion_snaps_the_nav_collapse(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let (core, _fake) = cockpit("motion-nav-reduced", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    cx.update(|_, cx| cx.set_reduce_motion(true));
    tick(cx);
    settle(cx);
    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    cx.run_until_parked();
    assert_eq!(nav_column_width(cx), nav::RAIL_WIDTH);
    assert_eq!(display_frames(cx), 0);
}

/// A nav row's hover blends in over `motion::HOVER_FADE`: frames while it
/// fades, none once it has landed or once the pointer has left and the
/// blend has returned to rest.
#[gpui::test]
fn a_nav_row_hover_fades_and_then_asks_for_no_frames(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let (core, _fake) = cockpit("motion-hover", 2);
    let thread = core.threads()[1];
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    settle(cx);
    let row = debug_bounds(cx, format!("nav-thread-{}", thread.get())).expect("the row");
    cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(50));
    assert!(display_frames(cx) > 0, "the blend is mid-flight");
    cx.executor()
        .advance_clock(Duration::from_millis(crate::theme::MOTION_HOVER_FADE_MS));
    display_frames(cx);
    assert_eq!(display_frames(cx), 0, "landed: no more frames");

    cx.simulate_mouse_move(
        gpui::point(px(900.), px(450.)),
        None,
        gpui::Modifiers::none(),
    );
    cx.run_until_parked();
    cx.executor()
        .advance_clock(Duration::from_millis(crate::theme::MOTION_HOVER_FADE_MS));
    display_frames(cx);
    assert_eq!(display_frames(cx), 0, "back at rest: no more frames");
}
