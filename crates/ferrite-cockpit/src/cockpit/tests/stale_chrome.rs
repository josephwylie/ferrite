//! The stale-chrome matrix: the Cockpit draws its nav and each Pane as a
//! cached part, rebuilt only when something it shows changes — so for every
//! way chrome changes, a test that what is on screen (what the part last
//! drew, `cockpit::drawn`) follows. One Thread on screen, the motion kit
//! live, an active window, the keyboard in the first Pane.

use super::*;
use crate::cockpit::drawn;
use ferrite_core::roster::PaneIdentity;

/// Let `ms` pass a display frame at a time, delivering each frame asked for.
fn frames(cx: &mut gpui::VisualTestContext, ms: u64) {
    for _ in 0..ms / 16 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
}

struct Board<'a> {
    view: Entity<CockpitView>,
    fake: Fake,
    threads: Vec<ThreadId>,
    cx: &'a mut gpui::VisualTestContext,
}

/// One Thread, Solo, mid-turn when `working`.
fn solo<'a>(name: &str, working: bool, cx: &'a mut TestAppContext) -> Board<'a> {
    ferrite_ui::motion::testing::drive();
    let (core, fake) = cockpit(name, 1);
    let working: &[usize] = if working { &[0] } else { &[] };
    open(core, fake, working, cx)
}

/// A window on `core`, the keyboard in the first Pane, `working` mid-turn, first paint settled.
fn open<'a>(
    mut core: Cockpit,
    fake: Fake,
    working: &[usize],
    cx: &'a mut TestAppContext,
) -> Board<'a> {
    let threads = core.threads().to_vec();
    for index in working {
        let _ = core.send(threads[*index], "Keep going".into());
    }
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(1000.)));
    cx.update(|window, _| window.activate_window());
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    for index in working {
        fake.streams.borrow()[*index]
            .send(SessionEvent::ReasoningSummaryDelta {
                text: "**Working**".into(),
                summary_index: 0,
            })
            .unwrap();
    }
    tick(cx);
    frames(cx, 600);
    Board {
        view,
        fake,
        threads,
        cx,
    }
}

impl Board<'_> {
    fn pane(&self, thread: usize) -> drawn::Pane {
        drawn::pane(PaneIdentity::Thread(self.threads[thread]))
    }

    fn send(&self, thread: usize, event: SessionEvent) {
        self.fake.streams.borrow()[thread].send(event).unwrap();
    }

    fn rect(&mut self, thread: usize) -> layout::Rect {
        let identity = PaneIdentity::Thread(self.threads[thread]);
        let view = self.view.clone();
        self.cx.update(|window, cx| {
            let view = view.read(cx);
            let index = view.index_of(identity).expect("on the board");
            view.pane_rects(window)
                .into_iter()
                .find(|(at, _)| *at == index)
                .map(|(_, rect)| rect)
                .expect("laid out")
        })
    }
}

/// Seen: a Thread on the board finishes while the window is away; its
/// Pane's head and its nav row say done at once, and both redraw it read
/// the moment the window comes back.
#[gpui::test]
fn a_finish_while_the_window_is_away_is_drawn_done_until_it_returns(cx: &mut TestAppContext) {
    let b = solo("stale-attention", true, cx);
    assert!(!b.pane(0).unread);
    b.cx.deactivate_window();
    b.send(
        0,
        SessionEvent::TextDelta {
            text: "Done here.".into(),
        },
    );
    b.send(
        0,
        SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        },
    );
    tick(b.cx);
    frames(b.cx, 300);
    assert!(b.pane(0).unread, "the Pane draws it done");
    assert!(
        drawn::row(b.threads[0]).starts_with("Done "),
        "and its nav row: {}",
        drawn::row(b.threads[0])
    );
    b.cx.update(|window, _| window.activate_window());
    tick(b.cx);
    frames(b.cx, 300);
    assert!(!b.pane(0).unread, "seen: the Pane redraws it read");
    assert!(
        drawn::row(b.threads[0]).starts_with("Idle "),
        "and so does its nav row: {}",
        drawn::row(b.threads[0])
    );
}

/// cmd-B slides the column to the rail: the Panes take the board's width
/// on screen every frame, and rest on the wider one.
#[gpui::test]
fn the_panes_ride_the_nav_fold(cx: &mut TestAppContext) {
    let mut b = solo("stale-nav-ride", false, cx);
    let view = b.view.clone();
    let before = b.pane(0).cell_width;
    view.update(b.cx, |view, cx| view.set_nav_collapsed(true, cx));
    let mut last = before;
    for _ in 0..6 {
        frames(b.cx, 16);
        let width = b.pane(0).cell_width;
        assert!(
            (width - b.rect(0).w).abs() < 0.5,
            "on the board's own width each frame: {width} vs {}",
            b.rect(0).w
        );
        assert!(width > last, "widening: {last} -> {width}");
        last = width;
    }
    frames(b.cx, 400);
    let landed = b.pane(0).cell_width;
    assert!(landed > last, "wider beside the rail: {last} -> {landed}");
    frames(b.cx, 400);
    assert_eq!(b.pane(0).cell_width, landed, "and stays there");
}

/// Window activation: leaving the window takes the keyboard from every
/// line; coming back gives it to the focused Pane's again.
#[gpui::test]
fn leaving_and_returning_to_the_window_reaches_the_panes(cx: &mut TestAppContext) {
    let b = solo("stale-activation", false, cx);
    assert!(b.pane(0).editing);
    b.cx.deactivate_window();
    frames(b.cx, 100);
    assert!(!b.pane(0).editing, "an inactive window edits nowhere");
    b.cx.update(|window, _| window.activate_window());
    frames(b.cx, 100);
    assert!(b.pane(0).editing, "back: the focused line edits again");
}

/// Hover in the nav: a row under the pointer draws its wash at once, and
/// drops it once the pointer leaves.
#[gpui::test]
fn a_nav_rows_hover_fades_in_and_out(cx: &mut TestAppContext) {
    let b = solo("stale-hover", false, cx);
    let row = debug_bounds(b.cx, format!("nav-thread-{}", b.threads[0].get())).expect("a row");
    b.cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::none());
    frames(b.cx, 200);
    let key =
        b.cx.update(|_, _| ferrite_ui::motion::testing::hover_values())
            .into_iter()
            .find(|(key, value)| key.starts_with("nav-thread-") && *value == 1.0)
            .map(|(key, _)| key)
            .expect("a row is hovered");
    assert_eq!(
        crate::nav::testing::drawn_hover(&key),
        Some(1.0),
        "drawn in"
    );
    b.cx.simulate_mouse_move(
        gpui::point(px(1200.), px(500.)),
        None,
        gpui::Modifiers::none(),
    );
    frames(b.cx, 300);
    assert!(
        crate::nav::testing::drawn_hover(&key).is_none_or(|t| t == 0.0),
        "drawn out: {:?}",
        crate::nav::testing::drawn_hover(&key)
    );
}

/// A popover: the `/` menu opens over the focused Pane's line as it is
/// typed, and closes on escape.
#[gpui::test]
fn the_slash_menu_opens_and_closes_over_its_pane(cx: &mut TestAppContext) {
    let b = solo("stale-popover", false, cx);
    bind_production_keys(b.cx);
    b.cx.simulate_input("/");
    frames(b.cx, 100);
    assert!(
        b.cx.debug_bounds("composer-menu-rows").is_some(),
        "the menu is drawn"
    );
    b.cx.simulate_keystrokes("escape");
    frames(b.cx, 100);
    assert!(
        b.cx.debug_bounds("composer-menu-rows").is_none(),
        "and gone on escape"
    );
}

/// A working clock turns over on its Pane while the board sits still.
#[gpui::test]
fn a_working_clock_turns_over_on_its_pane(cx: &mut TestAppContext) {
    let b = solo("stale-clock", true, cx);
    let before = b
        .pane(0)
        .clock
        .clone()
        .expect("the premise: a working clock");
    // The turn's clock is the wall clock: let a real second pass, and the
    // pulse clock's ride for it.
    std::thread::sleep(Duration::from_millis(1_100));
    frames(b.cx, 1_100);
    let after = b.pane(0).clock.clone().unwrap_or_default();
    assert_ne!(after, before, "the Pane redrew its clock");
}

/// The model picker opens placed over its box with no frame asked for
/// beyond its own, and its cursor follows the arrows.
/// The chip is Solo's (SPEC §5.5): the board's Pane fills it first.
#[gpui::test]
fn the_model_picker_opens_placed_and_its_cursor_follows_the_arrows(cx: &mut TestAppContext) {
    let b = solo("stale-picker", false, cx);
    bind_production_keys(b.cx);
    let thread = b.threads[0];
    b.view
        .update(b.cx, |view, cx| view.open_provider_picker(thread, cx));
    frames(b.cx, 300);
    let menu =
        b.cx.debug_bounds("model-picker")
            .expect("the picker is open");
    b.cx.update(|window, _| window.refresh());
    frames(b.cx, 50);
    assert_eq!(
        b.cx.debug_bounds("model-picker"),
        Some(menu),
        "already where a full redraw puts it"
    );
    let cursor = |view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.popover.as_ref().map(|open| open.selected))
    };
    let start = cursor(&b.view, b.cx);
    b.cx.simulate_keystrokes("down");
    frames(b.cx, 50);
    let moved = cursor(&b.view, b.cx);
    assert_ne!(moved, start, "↓ moves the cursor");
    b.cx.simulate_keystrokes("up");
    frames(b.cx, 50);
    assert_eq!(cursor(&b.view, b.cx), start, "and ↑ back");
}
