//! Find in the transcript (⌘F): the bar opens on the focused Pane, washes
//! every occurrence, brings the one it is on into view — laid out or not —
//! steps with ⏎ / ⇧⏎ / ⌘G / ⌘⇧G, and closes on esc without interrupting.
use super::*;
use crate::transcript::FindState;

fn say(fake: &Fake, text: &str) {
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta { text: text.into() })
        .unwrap();
}

fn read(fake: &Fake, id: &str) {
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: id.into(),
            name: "Read".into(),
            input: serde_json::json!({ "file_path": "src/lib.rs" }),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolCompleted {
            id: id.into(),
            output: "one\ntwo".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        })
        .unwrap();
}

fn transcript(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> Entity<crate::transcript::TranscriptView> {
    view.read_with(cx, |view, _| view.panes[0].transcript().unwrap())
}

fn state(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Option<FindState> {
    let transcript = transcript(view, cx);
    transcript.read_with(cx, |transcript, _| transcript.find_state())
}

/// Draw the cached transcript again until a reveal has settled: the test
/// clock asks for no frames of its own.
fn settle(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    for _ in 0..6 {
        let transcript = transcript(view, cx);
        transcript.update(cx, |_, cx| cx.notify());
        tick(cx);
    }
}

fn viewport(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> gpui::Bounds<gpui::Pixels> {
    let transcript = transcript(view, cx);
    transcript.read_with(cx, |transcript, _| transcript.scroll().bounds())
}

fn open(cx: &mut gpui::VisualTestContext) {
    cx.simulate_keystrokes("cmd-f");
    tick(cx);
}

#[gpui::test]
fn find_scrolls_to_an_occurrence_the_list_has_not_laid_out(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("find-offscreen", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    super::hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    say(&fake, "The opening Needle sits in the first answer.\n\n");
    for call in 0..6 {
        read(&fake, &format!("call-{call}"));
        say(
            &fake,
            &"Filler that only pads the transcript.\n\n".repeat(8),
        );
    }
    tick(cx);
    assert!(transcript(&view, cx).read_with(cx, |view, _| view.is_following_tail()));

    open(cx);
    assert!(cx.debug_bounds("find-bar").is_some(), "⌘F opens the bar");
    cx.simulate_input("needle");
    settle(&view, cx);
    let found = state(&view, cx).unwrap();
    assert_eq!(found.count.as_deref(), Some("1/1"));
    let row = found.row.unwrap();
    let painted = found.painted.expect("the occurrence painted");
    let shown = viewport(&view, cx);
    assert!(
        painted.top() >= shown.top() && painted.bottom() <= shown.bottom(),
        "the occurrence is in view: {painted:?} in {shown:?}"
    );
    assert!(transcript(&view, cx).read_with(cx, |view, _| view.scroll().item_is_visible(row)));
    assert!(cx.debug_bounds("find-count").is_some());

    // Smart case: a capital matches case.
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("NEEDLE");
    tick(cx);
    assert_eq!(
        state(&view, cx).unwrap().count.as_deref(),
        Some("no matches")
    );

    // Esc closes the bar, interrupts nothing, and the keyboard goes home.
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(cx.debug_bounds("find-bar").is_none(), "esc closes the bar");
    assert!(state(&view, cx).is_none());
    assert_eq!(*fake.interrupts.borrow(), 0, "and interrupts nothing");
    cx.simulate_input("x");
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, cx| view.panes[0]
            .composer
            .read(cx)
            .text()
            .to_string()),
        "x",
        "typing reaches the Composer again"
    );
}

#[gpui::test]
fn find_steps_both_ways_and_wraps(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("find-steps", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    super::hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    for call in 0..3 {
        say(&fake, &format!("Step {call} is a mark.\n\n"));
        read(&fake, &format!("call-{call}"));
    }
    tick(cx);
    open(cx);
    cx.simulate_input("mark");
    settle(&view, cx);
    let count = |cx: &mut gpui::VisualTestContext| state(&view, cx).unwrap().count.unwrap();
    // Everything is in view: the first occurrence in view is the first.
    assert_eq!(count(cx).as_ref(), "1/3");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(count(cx).as_ref(), "2/3", "⏎ steps forward");
    cx.simulate_keystrokes("cmd-g");
    tick(cx);
    assert_eq!(count(cx).as_ref(), "3/3", "and ⌘G");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(count(cx).as_ref(), "1/3", "wrapping");
    cx.simulate_keystrokes("shift-enter");
    tick(cx);
    assert_eq!(count(cx).as_ref(), "3/3", "⇧⏎ steps back");
    cx.simulate_keystrokes("cmd-shift-g");
    tick(cx);
    assert_eq!(count(cx).as_ref(), "2/3", "and ⌘⇧G");
    assert!(
        view.read_with(cx, |view, _| view.floats.palette.is_none()),
        "⌘G steps instead of opening the Groups"
    );
    assert!(
        view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).is_empty()),
        "⏎ sent nothing"
    );

    // New text under the same query keeps the bar's place.
    say(&fake, "A late mark arrives.\n\n");
    tick(cx);
    assert_eq!(count(cx).as_ref(), "2/4");
}

/// A long answer at the followed tail: its row is in view but the
/// occurrence at its head is not. The bar scrolls within the row.
#[gpui::test]
fn find_settles_an_occurrence_inside_a_tall_row(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("find-tall-row", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    super::hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    read(&fake, "call-0");
    say(&fake, "The haystack opens here.\n\n");
    say(
        &fake,
        &"Filler that only pads the transcript.\n\n".repeat(60),
    );
    tick(cx);
    open(cx);
    cx.simulate_input("haystack");
    settle(&view, cx);
    let painted = state(&view, cx).unwrap().painted.expect("painted");
    let shown = viewport(&view, cx);
    assert!(
        painted.top() >= shown.top() && painted.bottom() <= shown.bottom(),
        "the occurrence is in view: {painted:?} in {shown:?}"
    );
    assert!(!transcript(&view, cx).read_with(cx, |view, _| view.is_following_tail()));
}

/// Open `file` in a reader beside the Solo Pane and press in it.
fn read_beside(
    view: &Entity<CockpitView>,
    file: std::path::PathBuf,
    cx: &mut gpui::VisualTestContext,
) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_beside(
                beside::Beside::Reader {
                    path: file,
                    line: None,
                },
                window,
                cx,
            )
        })
    });
    tick(cx);
    // Under the reader's head: its body.
    let head = cx.debug_bounds("reader-head").expect("the reader is open");
    let body = gpui::point(head.center().x, head.bottom() + px(100.));
    cx.simulate_click(body, gpui::Modifiers::none());
    tick(cx);
}

fn reader_state(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> Option<(Option<SharedString>, Option<gpui::Bounds<gpui::Pixels>>)> {
    view.read_with(cx, |view, _| {
        view.panes[0].reader_find.as_ref().map(|find| find.state())
    })
}

/// A file's lines: the bar opens on the reader that took the last press,
/// washes the line and centres it; esc closes the bar and keeps the reader.
#[gpui::test]
fn find_in_a_code_reader_centres_its_line(cx: &mut TestAppContext) {
    let dir = scratch("find-reader-file");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("lines.rs");
    let source: String = (1..=200)
        .map(|n| match n {
            150 => "fn needle() {}\n".to_string(),
            n => format!("fn line_{n}() {{}}\n"),
        })
        .collect();
    std::fs::write(&file, source).unwrap();
    let (core, fake) = cockpit("find-reader", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    super::hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    say(&fake, "A needle in the transcript too.\n\n");
    tick(cx);
    read_beside(&view, file, cx);
    open(cx);
    assert!(cx.debug_bounds("find-bar").is_some());
    assert!(
        state(&view, cx).is_none(),
        "the transcript's bar stays shut"
    );
    cx.simulate_input("needle");
    tick(cx);
    assert_eq!(reader_state(&view, cx).unwrap().0.as_deref(), Some("1/1"));
    let scrolled = view.read_with(cx, |view, _| {
        view.panes[0]
            .reader_scroll
            .0
            .borrow()
            .base_handle
            .offset()
            .y
    });
    assert!(
        scrolled < px(-100. * crate::theme::LH_UI),
        "line 150 is brought into view: {scrolled:?}"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(reader_state(&view, cx).is_none(), "esc closes the bar");
    assert!(
        view.read_with(cx, |view, _| view.panes[0].preview.document().is_some()),
        "and keeps the reader"
    );
}

/// A rendered Markdown document: the occurrence is marked in its text and
/// scrolled into the reader's view once it paints.
#[gpui::test]
fn find_in_a_markdown_reader_scrolls_to_the_occurrence(cx: &mut TestAppContext) {
    let dir = scratch("find-markdown-reader-file");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("notes.md");
    let source = format!(
        "# Notes\n\n{}The needle is near the end.\n",
        "A paragraph that only pads the document.\n\n".repeat(80)
    );
    std::fs::write(&file, source).unwrap();
    let (core, _fake) = cockpit("find-markdown-reader", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    super::hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    read_beside(&view, file, cx);
    open(cx);
    cx.simulate_input("needle");
    for _ in 0..6 {
        view.update(cx, |_, cx| cx.notify());
        tick(cx);
    }
    let (count, painted) = reader_state(&view, cx).unwrap();
    assert_eq!(count.as_deref(), Some("1/1"));
    let painted = painted.expect("the occurrence painted");
    let shown = view.read_with(cx, |view, _| view.panes[0].reader_markdown_scroll.bounds());
    assert!(
        painted.top() >= shown.top() && painted.bottom() <= shown.bottom(),
        "the occurrence is in view: {painted:?} in {shown:?}"
    );
}
