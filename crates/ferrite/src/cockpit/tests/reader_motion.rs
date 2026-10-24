//! The reader slot's slide (`beside.rs`): it opens and closes beside its
//! Pane as one move, the Pane giving way and taking the room back, turns
//! around mid-flight, never slides for a document swapped in place, stays
//! inside a Group's or a fullscreen Pane's cell, lands for a seam drag, and
//! snaps under reduced motion; esc in its text closes it, the keyboard the
//! Pane's at once.

use super::*;
use crate::theme::{BOARD_SEAM, MOTION_SLOT_CLOSE_MS, MOTION_SLOT_OPEN_MS};

/// A source file of `lines` numbered lines, in its own scratch directory.
fn source(name: &str, lines: usize) -> std::path::PathBuf {
    let dir = scratch(name);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("lines.rs");
    let text: String = (1..=lines)
        .map(|n| format!("fn line_{n}() {{}}\n"))
        .collect();
    std::fs::write(&file, text).unwrap();
    file
}

fn read(
    view: &Entity<CockpitView>,
    path: &std::path::Path,
    line: Option<u32>,
    cx: &mut gpui::VisualTestContext,
) {
    let path = path.to_path_buf();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_beside(beside::Beside::Reader { path, line }, window, cx)
        })
    });
    cx.run_until_parked();
}

fn close(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| view.close_reader(0, cx));
    cx.run_until_parked();
}

/// Deliver the frames the last draw asked for: how many it asked for.
fn frame(cx: &mut gpui::VisualTestContext) -> usize {
    let frames = cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    frames
}

/// Step the clock `ms` and draw the frame due then.
fn at(ms: u64, cx: &mut gpui::VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    frame(cx);
}

fn settle(cx: &mut gpui::VisualTestContext) {
    for _ in 0..64 {
        if frame(cx) == 0 {
            return;
        }
        cx.executor().advance_clock(Duration::from_millis(16));
    }
    panic!("the window never stopped asking for frames");
}

/// Pane `index`'s rect as the last frame drew it.
fn pane(
    view: &Entity<CockpitView>,
    index: usize,
    cx: &mut gpui::VisualTestContext,
) -> layout::Rect {
    cx.update(|window, cx| {
        view.read(cx)
            .pane_rects(window)
            .into_iter()
            .find(|(at, _)| *at == index)
            .expect("the Pane is on the board")
            .1
    })
}

fn solo<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    crate::motion::testing::drive();
    let (core, _fake) = cockpit(name, 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    settle(cx);
    (view, cx)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.0
}

/// Opening, the Pane narrows from the whole board to its half over
/// `SLOT_OPEN` while the reader slides in from the board's edge, laid out
/// at its final width and pinned to the seam, so its text never re-wraps;
/// closing, the keyboard is the Pane's at once, the reader slides out over
/// the quicker `SLOT_CLOSE` and only then leaves the board. Frames come
/// only while it moves.
#[gpui::test]
fn the_reader_slides_open_and_shut_beside_its_pane(cx: &mut TestAppContext) {
    let (view, cx) = solo("reader-slide", cx);
    let file = source("reader-slide-file", 200);
    let whole = pane(&view, 0, cx);
    let half = (whole.w - BOARD_SEAM) / 2.0;

    read(&view, &file, Some(120), cx);
    assert!(
        near(pane(&view, 0, cx).w, whole.w),
        "its first frame is the board as it was"
    );
    at(MOTION_SLOT_OPEN_MS / 4, cx);
    let opening = pane(&view, 0, cx);
    assert!(
        opening.w < whole.w - 50.0 && opening.w > half + 1.0,
        "a quarter in, the Pane is giving way: {} of {} → {half}",
        opening.w,
        whole.w
    );
    let head = cx.debug_bounds("reader-head").expect("the reader is in");
    assert!(
        near(f32::from(head.size.width), half),
        "the reader is laid out at its final width: {head:?}"
    );
    assert!(
        near(f32::from(head.left()), opening.x + opening.w + BOARD_SEAM),
        "and pinned to the seam it slides in from: {head:?} beside {opening:?}"
    );

    at(MOTION_SLOT_OPEN_MS, cx);
    assert!(near(pane(&view, 0, cx).w, half), "it lands half and half");
    assert_eq!(frame(cx), 0, "at rest, nothing asks for a frame");

    close(&view, cx);
    assert!(
        view.read_with(cx, |view, _| view.panes[0].preview.document().is_none()),
        "closed at once"
    );
    assert!(
        cx.debug_bounds("reader-head").is_some(),
        "yet still drawn, sliding out"
    );
    at(MOTION_SLOT_CLOSE_MS / 4, cx);
    let closing = pane(&view, 0, cx).w;
    assert!(
        closing > half + 50.0 && closing < whole.w - 1.0,
        "the Pane takes its room back: {closing}"
    );
    at(MOTION_SLOT_CLOSE_MS, cx);
    frame(cx);
    assert!(near(pane(&view, 0, cx).w, whole.w), "the board as it was");
    assert!(cx.debug_bounds("reader-head").is_none(), "the slot is gone");
    assert!(
        view.read_with(cx, |view, _| view.panes[0].preview.shown().is_none()),
        "and its document let go"
    );
    assert_eq!(frame(cx), 0);
}

/// A flip mid-slide turns around from the width on screen, either way.
#[gpui::test]
fn a_reader_turns_around_mid_slide(cx: &mut TestAppContext) {
    let (view, cx) = solo("reader-turn", cx);
    let file = source("reader-turn-file", 40);
    let whole = pane(&view, 0, cx).w;

    read(&view, &file, None, cx);
    at(MOTION_SLOT_OPEN_MS / 3, cx);
    let opening = pane(&view, 0, cx).w;
    close(&view, cx);
    assert!(
        near(pane(&view, 0, cx).w, opening),
        "the close starts where the open was: {opening}"
    );
    at(MOTION_SLOT_CLOSE_MS / 4, cx);
    let closing = pane(&view, 0, cx).w;
    assert!(closing > opening + 1.0, "and widens from there: {closing}");

    read(&view, &file, None, cx);
    assert!(
        near(pane(&view, 0, cx).w, closing),
        "reopened, it turns around on screen"
    );
    settle(cx);
    assert!(near(pane(&view, 0, cx).w, (whole - BOARD_SEAM) / 2.0));
    assert!(view.read_with(cx, |view, _| view.panes[0].preview.document().is_some()));
}

/// Another document opening in an open reader swaps in place: the board
/// holds still and no slide runs.
#[gpui::test]
fn another_document_swaps_in_without_sliding(cx: &mut TestAppContext) {
    let (view, cx) = solo("reader-swap", cx);
    let first = source("reader-swap-first", 40);
    let second = source("reader-swap-second", 80);
    read(&view, &first, None, cx);
    settle(cx);
    let open = pane(&view, 0, cx);

    read(&view, &second, Some(60), cx);
    assert!(
        !view.read_with(cx, |view, _| view.reader_slides.moving()),
        "no slide for a swap"
    );
    for _ in 0..4 {
        at(30, cx);
        assert_eq!(pane(&view, 0, cx), open, "the board holds still");
    }
    assert_eq!(
        view.read_with(cx, |view, _| view.panes[0]
            .preview
            .document()
            .map(|document| document.path)),
        Some(second)
    );
}

/// In a Group the Pane's own cell splits as the reader slides in: its
/// neighbour never moves, and at rest the Pane and its reader share the
/// cell the Pane had.
#[gpui::test]
fn in_a_group_the_slide_stays_inside_the_pane_cell(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let (mut core, _fake) = cockpit("reader-slide-group", 2);
    let group = group_all(&mut core);
    core.enter_group(group).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    tick(cx);
    settle(cx);
    let (cell, neighbour) = (pane(&view, 0, cx), pane(&view, 1, cx));
    let file = source("reader-slide-group-file", 40);

    read(&view, &file, None, cx);
    for step in 1..=4 {
        at(MOTION_SLOT_OPEN_MS / 4, cx);
        assert_eq!(
            pane(&view, 1, cx),
            neighbour,
            "step {step}: the neighbour holds"
        );
        let owner = pane(&view, 0, cx);
        assert!(
            owner.x == cell.x && owner.w <= cell.w,
            "step {step}: {owner:?}"
        );
    }
    settle(cx);
    let owner = pane(&view, 0, cx);
    let head = cx.debug_bounds("reader-head").expect("the reader is in");
    assert!(
        near(f32::from(head.right()), cell.x + cell.w),
        "the reader ends where the cell did: {head:?} in {cell:?}"
    );
    assert!(near(owner.w, (cell.w - BOARD_SEAM) / 2.0), "{owner:?}");
}

/// A seam grabbed mid-slide lands the slide where it was going: the drag
/// starts from the board at rest and is never tweened.
#[gpui::test]
fn a_seam_grabbed_mid_slide_lands_it(cx: &mut TestAppContext) {
    let (view, cx) = solo("reader-slide-drag", cx);
    let file = source("reader-slide-drag-file", 40);
    let whole = pane(&view, 0, cx).w;
    read(&view, &file, None, cx);
    at(MOTION_SLOT_OPEN_MS / 4, cx);
    view.update(cx, |view, cx| {
        let board = view.board().expect("a reader board");
        view.begin_seam_drag(board, layout::SeamId(Vec::new()), cx);
    });
    cx.run_until_parked();
    assert!(near(pane(&view, 0, cx).w, (whole - BOARD_SEAM) / 2.0));
    assert!(!view.read_with(cx, |view, _| view.reader_slides.moving()));
    view.update(cx, |view, cx| view.end_seam_drag(cx));
}

/// Reduced motion: the reader is in, and gone, on the frame it opens and
/// closes, and nothing asks for a frame.
#[gpui::test]
fn reduced_motion_snaps_the_reader(cx: &mut TestAppContext) {
    let (view, cx) = solo("reader-slide-reduced", cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    let file = source("reader-slide-reduced-file", 40);
    let whole = pane(&view, 0, cx).w;
    read(&view, &file, None, cx);
    assert!(near(pane(&view, 0, cx).w, (whole - BOARD_SEAM) / 2.0));
    assert_eq!(frame(cx), 0);
    close(&view, cx);
    assert!(near(pane(&view, 0, cx).w, whole));
    assert!(cx.debug_bounds("reader-head").is_none());
    assert!(view.read_with(cx, |view, _| view.panes[0].preview.shown().is_none()));
    assert_eq!(frame(cx), 0);
}

/// Esc with the reader's own text holding the keyboard closes the reader
/// — not the Pane's turn — and the keyboard is the Pane's at once, while
/// the reader is still sliding out.
#[gpui::test]
fn escape_in_the_reader_closes_it_and_hands_the_keyboard_back(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let (core, fake) = cockpit("reader-escape", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let dir = scratch("reader-escape-file");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("notes.md");
    std::fs::write(
        &file,
        "# Notes\n\nA paragraph long enough to select a few words in.\n",
    )
    .unwrap();
    read(&view, &file, None, cx);
    settle(cx);
    // Select a word or two of the rendered document: its text holds the
    // keyboard.
    let text = cx
        .update(|_, cx| {
            crate::rich::testing::bounds(&format!("document-{}", file.display()), 0, cx)
        })
        .expect("the document is drawn");
    let y = text.top() + px(12.);
    let from = gpui::point(text.left() + px(4.), y);
    let to = gpui::point(text.left() + px(60.), y);
    cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    assert!(
        cx.update(|window, cx| view.read(cx).panes[0]
            .preview
            .reader_text_focused(window, cx)),
        "the premise: the reader's text holds the keyboard"
    );

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        view.read_with(cx, |view, _| view.panes[0].preview.document().is_none()),
        "esc closed the reader"
    );
    assert_eq!(*fake.interrupts.borrow(), 0, "and interrupted nothing");
    assert!(
        cx.debug_bounds("reader-head").is_some(),
        "the reader is still sliding out"
    );
    cx.simulate_input("x");
    assert_eq!(
        view.read_with(cx, |view, cx| view.panes[0]
            .composer
            .read(cx)
            .text()
            .to_string()),
        "x",
        "the keyboard is the Pane's already"
    );
    settle(cx);
    assert!(cx.debug_bounds("reader-head").is_none());
}

/// A fullscreen Pane's own cell fills the board, so a reader opened from
/// it splits that cell, where it can be seen, and slides like any other;
/// closed, the Pane fills the board again.
#[gpui::test]
fn a_fullscreen_pane_splits_its_cell_with_its_reader(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let (mut core, _fake) = cockpit("reader-fullscreen", 2);
    let group = group_all(&mut core);
    core.enter_group(group).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        view.cockpit.toggle_fullscreen();
        cx.notify();
    });
    tick(cx);
    settle(cx);
    let whole = pane(&view, 0, cx);
    let file = source("reader-fullscreen-file", 40);

    read(&view, &file, None, cx);
    at(MOTION_SLOT_OPEN_MS / 4, cx);
    let opening = pane(&view, 0, cx).w;
    assert!(opening < whole.w - 1.0, "the Pane gives way: {opening}");
    settle(cx);
    assert!(near(pane(&view, 0, cx).w, (whole.w - BOARD_SEAM) / 2.0));
    assert!(
        cx.debug_bounds("reader-head").is_some(),
        "the reader is in sight"
    );
    assert_eq!(
        cx.update(|window, cx| view.read(cx).pane_rects(window).len()),
        1,
        "still the one Pane"
    );

    close(&view, cx);
    settle(cx);
    assert_eq!(pane(&view, 0, cx), whole, "the Pane fills the board again");
    assert!(cx.debug_bounds("reader-head").is_none());
}
