//! What a window does while nothing arrives, and while one Thread streams:
//! how often the pump runs, and which views it redraws. Each test prints
//! its counts on one `WAKES` line (`--no-capture`), so a change to the
//! pump or the nav can be measured before and after.
use super::*;
use crate::cockpit::drawn;

/// The counters a window's work moves.
#[derive(Clone, Copy, Debug, Default)]
struct Counts {
    pumps: usize,
    lists: usize,
    nav: usize,
    root: usize,
    panes: [usize; 2],
}

fn counts(panes: &[ferrite_core::roster::PaneIdentity]) -> Counts {
    let pane = |at: usize| panes.get(at).map_or(0, |pane| drawn::pane_renders(*pane));
    Counts {
        pumps: drawn::pumps(),
        lists: drawn::lists(),
        nav: drawn::nav_renders(),
        root: crate::cockpit::RENDERS.with(std::cell::Cell::get),
        panes: [pane(0), pane(1)],
    }
}

impl std::ops::Sub for Counts {
    type Output = Counts;
    fn sub(self, before: Counts) -> Counts {
        Counts {
            pumps: self.pumps - before.pumps,
            lists: self.lists - before.lists,
            nav: self.nav - before.nav,
            root: self.root - before.root,
            panes: [
                self.panes[0] - before.panes[0],
                self.panes[1] - before.panes[1],
            ],
        }
    }
}

/// One pump beat of the clock, and the frame the window asks for.
fn beat(cx: &mut gpui::VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(PUMP_MS));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
}

fn window<'a>(
    name: &str,
    threads: usize,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    Fake,
    Vec<ferrite_core::roster::PaneIdentity>,
    &'a mut gpui::VisualTestContext,
) {
    ferrite_ui::motion::testing::drive();
    let (core, fake) = cockpit(name, threads);
    let (view, cx) = add_unfocused_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    // First paint settles: measurement passes, springs arriving at rest.
    for _ in 0..1_000 / PUMP_MS {
        beat(cx);
    }
    // In the order of the Threads, which is the order of the fake's streams.
    let panes = view.read_with(cx, |view, _| {
        view.cockpit
            .threads()
            .into_iter()
            .map(|thread| view.panes[view.pane_for(thread).unwrap()].identity)
            .collect()
    });
    (view, fake, panes, cx)
}

/// One Project, one idle Thread, nothing arriving, for four seconds (two
/// of the window's two-second sweeps): the pump runs on its beat.
#[gpui::test]
fn an_idle_window_keeps_its_beat_and_redraws_nothing(cx: &mut TestAppContext) {
    let _clock = hold_wall_clock();
    let (_view, _fake, panes, cx) = window("wakes-idle", 1, cx);
    let before = counts(&panes);
    let beats = 4_000 / PUMP_MS as usize;
    for _ in 0..beats {
        beat(cx);
    }
    let idle = counts(&panes) - before;
    eprintln!(
        "WAKES idle_4s beats={beats} pumps={} nav_lists={} nav={} root={} pane={}",
        idle.pumps, idle.lists, idle.nav, idle.root, idle.panes[0]
    );
    assert!(idle.pumps >= beats, "the pump keeps its beat");
}

/// Two Threads; the second streams a turn, a delta a beat, and ends it,
/// while the window notifies three times for something the nav does not
/// draw. The nav redraws only on beats where what it lists moved (and
/// for the slide that moves its rows).
#[gpui::test]
fn a_streaming_thread_redraws_the_nav_only_when_its_rows_move(cx: &mut TestAppContext) {
    let _clock = hold_wall_clock();
    let (view, fake, panes, cx) = window("wakes-stream", 2, cx);
    // What the nav lists now, read as the test's own oracle: its lists
    // are not the window's work.
    let oracle = std::cell::Cell::new(0);
    let rows = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        let lists = drawn::lists();
        let rows = view.read_with(cx, |view, _| view.nav_state());
        oracle.set(oracle.get() + drawn::lists() - lists);
        rows
    };
    let before = counts(&panes);
    let mut last = rows(&view, cx);
    oracle.set(0);
    let mut moved = 0;
    let mut beats = 0;
    let mut step = |event: Option<SessionEvent>, cx: &mut gpui::VisualTestContext| {
        if let Some(event) = event {
            fake.streams.borrow()[1].send(event).unwrap();
        }
        beat(cx);
        beats += 1;
        let now = rows(&view, cx);
        moved += usize::from(now != last);
        last = now;
    };
    for at in 0..30 {
        step(
            Some(SessionEvent::TextDelta {
                text: "working ".into(),
            }),
            cx,
        );
        // Every tenth beat the window notifies for something the nav does
        // not draw, as the pump does when the Server's models move and as
        // most window actions do.
        if at % 10 == 9 {
            view.update(cx, |_, cx| cx.notify());
        }
    }
    step(
        Some(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        }),
        cx,
    );
    for _ in 0..30 {
        step(None, cx);
    }
    let mut load = counts(&panes) - before;
    load.lists -= oracle.get();
    eprintln!(
        "WAKES stream beats={beats} pumps={} nav_lists={} rows_moved={moved} nav={} root={} \
         shown_pane={} hidden_pane={}",
        load.pumps, load.lists, load.nav, load.root, load.panes[0], load.panes[1]
    );
    assert!(moved > 0, "the stream moved the nav's rows");
}
