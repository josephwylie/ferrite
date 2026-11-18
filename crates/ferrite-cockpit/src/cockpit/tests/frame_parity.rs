//! The frame package's parity tests (F-1 … F-18): the Solo card, the
//! empty board, the foot, a file in the panel, retry and the transcript's
//! top.
#[allow(unused_imports)]
use super::*;

/// One Thread, Solo, at the app's size.
fn solo_of<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, Fake, &'a mut gpui::VisualTestContext) {
    let (core, fake) = cockpit(name, 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    (view, fake, cx)
}

/// F-13: the empty board offers only what would do something (no parked
/// Thread to reopen here), its rows walk with `Next` /
/// `Previous`, wrapping.
#[gpui::test]
fn the_empty_board_walks_its_rows(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("frame-empty-board", 0);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    cx.simulate_keystrokes("cmd-w");
    tick(cx);
    assert!(cx.debug_bounds("empty-board").is_some());
    for at in 0..3 {
        assert!(debug_bounds(cx, format!("empty-board-command-{at}")).is_some());
    }
    assert!(
        debug_bounds(cx, "empty-board-command-3".to_string()).is_none(),
        "no `Reopen last` with nothing to reopen"
    );
    let step = |action: Box<dyn gpui::Action>, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| window.dispatch_action(action, cx));
        tick(cx);
    };
    step(Box::new(empty_board::Next), cx);
    step(Box::new(empty_board::Next), cx);
    view.read_with(cx, |view, _| assert_eq!(view.empty_cursor, 2));
    step(Box::new(empty_board::Next), cx);
    view.read_with(cx, |view, _| assert_eq!(view.empty_cursor, 0, "it wraps"));
    step(Box::new(empty_board::Previous), cx);
    view.read_with(cx, |view, _| assert_eq!(view.empty_cursor, 2, "both ways"));
    // Its first row, a new Thread, leaves the empty board.
    step(Box::new(empty_board::Next), cx);
    step(Box::new(empty_board::Run), cx);
    assert!(cx.debug_bounds("empty-board").is_none());
}

/// The nav's foot is Claude's account row alone: no view switcher over it
/// and no bar under the board.
#[gpui::test]
fn the_foot_is_the_account_row_alone(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("frame-foot-alone", 2);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    assert!(cx.debug_bounds("nav-foot").is_some());
    assert!(cx.debug_bounds("nav-view-tabs").is_none(), "no switcher");
    assert!(cx.debug_bounds("bottom-bar").is_none(), "no bottom bar");
}

/// Free disk shows in the nav's foot only under 20 GB, and never while
/// unknown.
#[gpui::test]
fn the_foot_says_free_disk_only_when_it_runs_low(cx: &mut TestAppContext) {
    let (view, _fake, cx) = solo_of("frame-foot-disk", cx);
    for (free, shown) in [
        (None, false),
        (Some(30_000_000_000), false),
        (Some(7_500_000_000), true),
    ] {
        view.update(cx, |view, cx| view.free_disk_landed(free, cx));
        tick(cx);
        assert_eq!(
            cx.debug_bounds("nav-free-disk").is_some(),
            shown,
            "{free:?} free"
        );
    }
}

/// F-14: a file opens at a line in the Panel's preview — the Panel wide,
/// the board giving up the width — and closing it gives the Pane its board
/// back.
#[gpui::test]
fn a_file_opens_in_the_panel_and_closes(cx: &mut TestAppContext) {
    let dir = scratch("frame-reader");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("nav.rs");
    std::fs::write(
        &file,
        "fn one() {}\nfn two() {}\nfn three() {}\nfn four() {}\n",
    )
    .unwrap();
    let (core, _fake) = cockpit("frame-reader-view", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let whole = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1.w);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_preview(file.clone(), Some(3), window, cx)
        })
    });
    tick(cx);
    cx.executor().advance_clock(Duration::from_millis(250));
    tick(cx);
    let (line, narrowed) = cx.update(|window, cx| {
        let view = view.read(cx);
        (
            view.panes[0].preview.document().and_then(|doc| doc.line()),
            view.pane_rects(window)[0].1.w,
        )
    });
    assert_eq!(line, Some(3));
    // The Panel widens to read it (SPEC §5.6.3): the board gives up what
    // it gains.
    let wide = view.read_with(cx, |view, _| view.board_right());
    assert!(wide > crate::cockpit::panel::least_w());
    assert!(
        (narrowed - (whole - (wide - crate::cockpit::panel::least_w()))).abs() <= 1.0,
        "the board gives way: {narrowed} of {whole}"
    );
    assert!(cx.debug_bounds("panel-preview").is_some());
    view.update(cx, |view, cx| view.close_preview(0, cx));
    tick(cx);
    cx.executor().advance_clock(Duration::from_millis(250));
    tick(cx);
    // Closed, the Panel is back at the operator's width: the file's floor
    // was never saved.
    let back = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1.w);
    assert!(cx.debug_bounds("panel-preview").is_none());
    assert!(
        (back - whole).abs() <= 0.5,
        "the board as it was before the file: {back} / {whole}"
    );
}

/// Theme rule 2: the Solo Pane is the plane itself, flush right of the
/// nav, under the 44px head row, down to the window's foot (no bottom bar)
/// and to the panel's column (§5.6), which runs from the window's top —
/// at the mockups' 1600×960.
#[gpui::test]
fn the_solo_card_sits_on_the_frame(cx: &mut TestAppContext) {
    use crate::cockpit::panel::least_w;
    use ferrite_ui::theme::{HAIRLINE_W, WIN_CHROME_H};
    let (core, _fake) = cockpit("frame-solo-card", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1600.), px(960.)));
    tick(cx);
    let rects = cx.update(|window, cx| view.read(cx).pane_rects(window));
    let card = rects[0].1;
    assert_eq!((card.x, card.y), (nav::COLUMN_WIDTH, WIN_CHROME_H));
    assert_eq!(card.x + card.w, 1600. - least_w());
    assert_eq!(card.y + card.h, 960., "no gutter under it");
    let panel = cx.debug_bounds("panel").expect("the panel");
    assert_eq!(
        panel,
        gpui::Bounds::new(
            gpui::point(px(1600. - least_w()), px(0.)),
            gpui::size(px(least_w()), px(960. - HAIRLINE_W))
        )
    );
    let key = view.read_with(cx, |view, _| view.panes[0].thread().unwrap().get());
    let root = bounds(cx, format!("pane-root-{key}"));
    assert!((root.left() - px(card.x)).abs() < px(1.) && (root.top() - px(card.y)).abs() < px(1.));
    assert!(cx.debug_bounds("bottom-bar").is_none(), "no bottom bar");
}

/// SPEC §5.9: a turn that failed leaves the failure strip under it, and
/// its `Retry` sends the failed turn's prompt again.
#[gpui::test]
fn retry_re_sends_the_failed_turns_prompt(cx: &mut TestAppContext) {
    let (view, fake, cx) = solo_of("frame-retry", cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        let _ = view.cockpit.send(thread, "build the range sheet".into());
        cx.notify();
    });
    tick(cx);
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Error("boom".into()),
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    let sent = |fake: &Fake| {
        fake.sent
            .borrow()
            .iter()
            .filter(|prompt| prompt.contains("build the range sheet"))
            .count()
    };
    assert_eq!(sent(&fake), 1);
    let key = thread.get();
    assert!(
        debug_bounds(cx, format!("failure-strip-{key}")).is_some(),
        "the strip stands under the failed turn"
    );
    let retry = bounds(cx, format!("failure-retry-{key}"));
    cx.simulate_click(retry.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(sent(&fake), 2, "Retry sent the prompt again");
}

/// A Session that closes mid-turn failed that turn: the strip says so,
/// and offers no `Retry`, there being no Session to send to.
#[gpui::test]
fn a_session_closed_mid_turn_leaves_the_strip_without_retry(cx: &mut TestAppContext) {
    let (view, fake, cx) = solo_of("frame-closed-mid-turn", cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        let _ = view.cockpit.send(thread, "build the range sheet".into());
        cx.notify();
    });
    tick(cx);
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Working on it".into(),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::Closed {
            reason: "exited".into(),
        })
        .unwrap();
    tick(cx);
    let key = thread.get();
    assert!(
        debug_bounds(cx, format!("failure-strip-{key}")).is_some(),
        "the strip stands under the unfinished turn"
    );
    assert!(
        debug_bounds(cx, format!("failure-retry-{key}")).is_none(),
        "a closed Session has nothing to retry on"
    );
}

/// A transcript shorter than its viewport reads down from the top, as a new
/// conversation does, the empty space under it.
#[gpui::test]
fn a_short_transcript_reads_from_the_top(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("frame-bottom-solo", 2);
    let thread = core.threads()[0];
    let _ = core.send(thread, "Explain the board".into());
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "One regression in the board.".into(),
        })
        .unwrap();
    tick(cx);
    let body = bounds(cx, format!("pane-body-{}", thread.get()));
    let prompt = cx.debug_bounds("transcript-prompt").expect("the prompt");
    let answer = cx.debug_bounds("transcript-answer").expect("the answer");
    assert!(
        (prompt.top() - body.top() - px(ferrite_ui::theme::TX_PAD_T)).abs() <= px(2.),
        "the first row heads the body: {prompt:?} / {body:?}"
    );
    assert!(
        body.bottom() - answer.bottom() > px(100.),
        "the empty space is under the rows: {answer:?} / {body:?}"
    );
}
