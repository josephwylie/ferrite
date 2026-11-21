//! What a window does while nothing arrives, and while one Thread streams:
//! how often the pump runs, and which views it redraws. Each test prints
//! its counts on one `WAKES` line (`--no-capture`), so a change to the
//! pump, the nav or the toast float can be measured before and after.
use super::*;
use crate::cockpit::drawn;

/// The counters a window's work moves.
#[derive(Clone, Copy, Debug, Default)]
struct Counts {
    pumps: usize,
    lists: usize,
    nav: usize,
    toasts: usize,
    root: usize,
    panes: [usize; 2],
}

fn counts(panes: &[ferrite_core::roster::PaneIdentity]) -> Counts {
    let pane = |at: usize| panes.get(at).map_or(0, |pane| drawn::pane_renders(*pane));
    Counts {
        pumps: drawn::pumps(),
        lists: drawn::lists(),
        nav: drawn::nav_renders(),
        toasts: drawn::toast_renders(),
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
            toasts: self.toasts - before.toasts,
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
    let panes = view.read_with(cx, |view, cx| {
        view.cockpit
            .read(cx)
            .threads()
            .into_iter()
            .map(|thread| view.panes[view.pane_for(thread).unwrap()].identity)
            .collect()
    });
    (view, fake, panes, cx)
}

/// One Project, one idle Thread, nothing arriving, for four seconds (two
/// of the window's two-second sweeps): the pump runs on its beat, and the
/// nav neither redraws nor works out its list again.
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
        "WAKES idle_4s beats={beats} pumps={} nav_lists={} nav={} toasts={} root={} pane={}",
        idle.pumps, idle.lists, idle.nav, idle.toasts, idle.root, idle.panes[0]
    );
    assert!(idle.pumps >= beats, "the pump keeps its beat");
    assert_eq!(idle.nav, 0, "an idle nav is not redrawn");
    assert_eq!(idle.toasts, 0, "an idle toast float is not redrawn");
    assert!(
        idle.lists <= idle.root,
        "the nav reads its list again only as the window notifies"
    );
}

/// Two Threads; the window notifies three times for something the nav
/// does not draw, then the second streams a turn, a delta a beat, and
/// ends it. The nav redraws only on beats where what it lists moved (and
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
        let rows = view.read_with(cx, |view, cx| view.nav_state(cx));
        oracle.set(oracle.get() + drawn::lists() - lists);
        rows
    };
    // Before it, at rest, the window notifies three times for something the
    // nav does not draw, as the pump does when the Server's models move and
    // as most window actions do: the nav is not redrawn for it.
    let nav = drawn::nav_renders();
    for _ in 0..3 {
        view.update(cx, |_, cx| cx.notify());
        beat(cx);
    }
    let bare_redraws = drawn::nav_renders() - nav;
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
    for _ in 0..30 {
        step(
            Some(SessionEvent::TextDelta {
                text: "working ".into(),
            }),
            cx,
        );
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
        "WAKES stream beats={beats} pumps={} nav_lists={} rows_moved={moved} nav={} \
         nav_for_bare_notifies={bare_redraws} toasts={} root={} \
         shown_pane={} hidden_pane={}",
        load.pumps, load.lists, load.nav, load.toasts, load.root, load.panes[0], load.panes[1]
    );
    assert!(moved > 0, "the stream moved the nav's rows");
    assert_eq!(bare_redraws, 0, "a notify that moves nothing it draws leaves the nav");
}

/// A Thread whose turn starts moves to the Working shelf, and back to the
/// top of the list when it ends, each on the beat it happens: the nav
/// hears it from the pump's beat itself, and the window is not notified
/// for it.
#[gpui::test]
fn the_nav_moves_a_working_thread_on_the_beat_without_the_window(cx: &mut TestAppContext) {
    let clock = hold_wall_clock();
    let (view, fake, _panes, cx) = window("wakes-section", 2, cx);
    let threads = view.read_with(cx, |view, cx| view.cockpit.read(cx).threads());
    let notified = std::rc::Rc::new(std::cell::Cell::new(0));
    let _watch = cx.update({
        let notified = notified.clone();
        |_, cx| cx.observe(&view, move |_, _| notified.set(notified.get() + 1))
    });
    let rows = |cx: &mut gpui::VisualTestContext| view.read_with(cx, |view, cx| view.nav_state(cx));
    let ids = |rows: &[crate::nav::ThreadRow]| rows.iter().map(|row| row.thread).collect::<Vec<_>>();
    // Its first event settles what the Session says of itself.
    fake.streams.borrow()[1]
        .send(SessionEvent::TextDelta { text: "working ".into() })
        .unwrap();
    beat(cx);
    assert_eq!(ids(&rows(cx).working), vec![threads[1]], "working on the beat it starts");
    let before = (notified.get(), drawn::nav_renders());
    fake.streams.borrow()[1]
        .send(SessionEvent::Progress {
            event: ferrite_core::progress::ProgressEvent::Background {
                id: "shell".into(),
                label: "sleep 1".into(),
                status: ferrite_core::progress::TaskStatus::Working,
                detail: String::new(),
            },
        })
        .unwrap();
    fake.streams.borrow()[1]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    beat(cx);
    assert_eq!(ids(&rows(cx).working), vec![threads[1]], "its shell still works");
    fake.streams.borrow()[1]
        .send(SessionEvent::Progress {
            event: ferrite_core::progress::ProgressEvent::Background {
                id: "shell".into(),
                label: "sleep 1".into(),
                status: ferrite_core::progress::TaskStatus::Completed,
                detail: String::new(),
            },
        })
        .unwrap();
    // Arrivals are ordered by the wall clock: done comes later than started.
    clock.advance(Duration::from_secs(5));
    beat(cx);
    let after = rows(cx);
    assert!(after.working.is_empty(), "done");
    assert_eq!(after.active.first().map(|row| row.thread), Some(threads[1]), "on top");
    eprintln!(
        "WAKES section window_notifies={} nav={}",
        notified.get() - before.0,
        drawn::nav_renders() - before.1
    );
    assert!(drawn::nav_renders() > before.1, "the nav drew the move");
    assert_eq!(notified.get(), before.0, "the window was not notified for it");
}

/// Two Threads with the nav folded; the second, off the board, asks for a
/// Decision. The toast float draws it on the beat it rings and not before,
/// stands through idle beats and through window notifies that move nothing
/// it draws, and takes it down as the request is withdrawn.
#[gpui::test]
fn the_toast_float_redraws_only_as_its_toast_comes_and_goes(cx: &mut TestAppContext) {
    let _clock = hold_wall_clock();
    let (view, fake, panes, cx) = window("wakes-toast", 2, cx);
    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    for _ in 0..1_000 / PUMP_MS {
        beat(cx);
    }
    let (off, at) = view.read_with(cx, |view, cx| {
        let threads = view.cockpit.read(cx).threads();
        let at = threads
            .iter()
            .position(|thread| Some(*thread) != view.focused_thread(cx))
            .unwrap();
        (threads[at], at)
    });
    let toast: &'static str = format!("toast-{}", off.get()).leak();
    let bare = |cx: &mut gpui::VisualTestContext| {
        let before = drawn::toast_renders();
        for _ in 0..3 {
            view.update(cx, |_, cx| cx.notify());
            beat(cx);
        }
        drawn::toast_renders() - before
    };
    let bare_before = bare(cx);
    let before = counts(&panes);
    fake.streams.borrow()[at].send(decision("waits")).unwrap();
    beat(cx);
    assert!(cx.debug_bounds(toast).is_none(), "it waits its second");
    let mut waited = 0;
    while cx.debug_bounds(toast).is_none() {
        beat(cx);
        waited += 1;
        assert!(waited < 2 * 1_000 / PUMP_MS as usize, "the toast never came");
    }
    let came = counts(&panes) - before;
    // It rises in, a frame a beat, and lands.
    let rising_from = drawn::toast_renders();
    for _ in 0..(ferrite_ui::theme::MOTION_TOAST_IN_MS + 32) / PUMP_MS {
        beat(cx);
    }
    let rising = drawn::toast_renders() - rising_from;
    // It stands: a second of idle beats, then three bare notifies.
    let standing_from = counts(&panes);
    for _ in 0..1_000 / PUMP_MS {
        beat(cx);
    }
    let standing_counts = counts(&panes) - standing_from;
    let standing = standing_counts.toasts;
    let bare_standing = bare(cx);
    assert!(cx.debug_bounds(toast).is_some(), "it stands");
    let going = counts(&panes);
    fake.streams.borrow()[at]
        .send(SessionEvent::Activity(
            ferrite_core::activity::ActivityEvent::DecisionCancelled { id: "waits".into() },
        ))
        .unwrap();
    beat(cx);
    assert!(cx.debug_bounds(toast).is_none(), "withdrawn, it goes on the beat");
    let went = counts(&panes) - going;
    eprintln!(
        "WAKES toast bare_before={bare_before} came_beats={waited} came_toasts={} came_root={} \
         rising={rising} standing_1s={standing} standing_root={} bare_standing={bare_standing} went_toasts={} went_root={}",
        came.toasts, came.root, standing_counts.root, went.toasts, went.root
    );
    assert_eq!(bare_before, 0, "a notify with no toast leaves the float");
    assert!(came.toasts > 0, "the float drew the toast");
    assert_eq!(standing, 0, "a standing toast is not redrawn while nothing moves");
    assert_eq!(bare_standing, 0, "a notify that moves nothing it draws leaves the float");
    assert!(went.toasts > 0, "the float drew it going");
}

/// A Factory board arrives from the Server with a Task that needs the
/// operator: the nav's Factory door wears its dot on that beat, and the
/// door's menu counts the Task beside its Project.
#[gpui::test]
fn the_factory_door_counts_what_needs_you_as_the_board_arrives(cx: &mut TestAppContext) {
    use ferrite_core::factory::{Board, Status, Task};
    let _clock = hold_wall_clock();
    let (view, _fake, _panes, cx) = window("wakes-factory-door", 1, cx);
    let project = view.read_with(cx, |view, cx| {
        let model = view.cockpit.read(cx);
        model.project_id(model.threads()[0]).unwrap()
    });
    assert!(cx.debug_bounds("nav-factory").is_some(), "the door");
    assert!(cx.debug_bounds("nav-factory-attention").is_none(), "nothing needs you");
    let idle = drawn::nav_renders();
    for _ in 0..1_000 / PUMP_MS {
        beat(cx);
    }
    assert_eq!(drawn::nav_renders(), idle, "an idle door is not redrawn");
    let task: Task = serde_json::from_value(serde_json::json!({
        "id": "t-1",
        "project": project,
        "title": "Task t-1",
        "instructions": "Do the work",
        "status": Status::NeedsHuman,
        "agent": {"provider": "claude", "model": "claude-opus-4"},
        "branch": "factory/t-1",
        "worker": null,
        "result": null,
        "reason": "No access",
        "activity": [],
        "created_ms": 1,
    }))
    .unwrap();
    view.update(cx, |view, cx| {
        view.cockpit.update(cx, |model, _| {
            model.fixture_factory(Board {
                tasks: vec![task],
                ..Default::default()
            })
        })
    });
    beat(cx);
    assert!(
        cx.debug_bounds("nav-factory-attention").is_some(),
        "the door's dot, on the beat the board arrives"
    );
    let rows = view.read_with(cx, |view, cx| view.factory_rows(cx));
    let counts: Vec<_> = rows
        .iter()
        .flatten()
        .map(|(item, _)| item.detail.clone())
        .collect();
    assert_eq!(counts, [Some("1".into())], "its Project's row counts the Task");
}
