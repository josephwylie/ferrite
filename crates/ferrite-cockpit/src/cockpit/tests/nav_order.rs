//! Where a Thread sits in the nav, and what it says there: a Thread works
//! while its turn or any background work runs, and sits on the Working
//! shelf; truly done, it goes to the top of the list. Nothing else moves
//! a row.
use super::*;
use ferrite_core::progress::{ProgressEvent, TaskStatus};

fn shell(status: TaskStatus) -> SessionEvent {
    SessionEvent::Progress {
        event: ProgressEvent::Background {
            id: "shell".into(),
            label: "sleep 120".into(),
            status,
            detail: String::new(),
        },
    }
}

fn turn_ended() -> SessionEvent {
    SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Completed,
        cost_usd: None,
    }
}

fn streaming() -> SessionEvent {
    SessionEvent::TextDelta {
        text: "working".into(),
    }
}

fn ids(rows: &[nav::ThreadRow]) -> Vec<ThreadId> {
    rows.iter().map(|row| row.thread).collect()
}

/// A shell that outlives Main's turn keeps the Thread working: on the
/// Working shelf, never `done`, no Notice. When it exits the Thread is
/// done and comes back at the top of the list, the others where they were.
#[gpui::test]
fn a_thread_whose_shell_outlives_its_turn_works_until_the_shell_ends(cx: &mut TestAppContext) {
    let clock = ferrite_core::clock::ThreadFixture::install(ferrite_core::clock::now());
    let (core, fake) = cockpit("nav-shell-outlives-turn", 3);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let before = view.read_with(cx, |view, _| ids(&view.nav_state().active));
    let last = *before.last().unwrap();
    let send = |event: SessionEvent| {
        let at = threads.iter().position(|thread| *thread == last).unwrap();
        fake.streams.borrow()[at].send(event).unwrap();
    };
    send(streaming());
    tick(cx);
    send(shell(TaskStatus::Working));
    send(turn_ended());
    tick(cx);
    clock.advance(Duration::from_secs(60));
    tick(cx);
    view.read_with(cx, |view, _| {
        let state = view.nav_state();
        assert_eq!(ids(&state.working), vec![last], "its shell still runs");
        assert_eq!(
            state.working[0].status.attention(),
            ferrite_core::Attention::Working,
            "never done while its shell runs"
        );
        assert!(
            !view
                .cockpit
                .notifications()
                .notices()
                .any(|notice| notice.thread == last),
            "a running shell is no finish"
        );
    });
    send(shell(TaskStatus::Completed));
    tick(cx);
    clock.advance(Duration::from_secs(4));
    tick(cx);
    view.read_with(cx, |view, _| {
        let state = view.nav_state();
        assert!(state.working.is_empty());
        let mut expected = vec![last];
        expected.extend(&before[..before.len() - 1]);
        assert_eq!(
            ids(&state.active),
            expected,
            "done goes to the top; the others keep their places"
        );
        assert!(view
            .cockpit
            .notifications()
            .notices()
            .any(|notice| notice.thread == last));
    });
}

/// The Working shelf keeps the newest send first: a Thread whose turn
/// ends while its shell runs is still working and keeps its place.
#[gpui::test]
fn a_working_row_keeps_its_place_when_its_turn_hands_over_to_a_shell(cx: &mut TestAppContext) {
    let clock = ferrite_core::clock::ThreadFixture::install(ferrite_core::clock::now());
    let (core, fake) = cockpit("nav-working-holds-still", 2);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let send = |at: usize, event: SessionEvent| fake.streams.borrow()[at].send(event).unwrap();
    send(0, streaming());
    tick(cx);
    clock.advance(Duration::from_secs(5));
    send(1, streaming());
    tick(cx);
    let working = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| ids(&view.nav_state().working))
    };
    assert_eq!(working(cx), vec![threads[1], threads[0]], "newest send first");
    send(1, shell(TaskStatus::Working));
    send(1, turn_ended());
    tick(cx);
    assert_eq!(
        working(cx),
        vec![threads[1], threads[0]],
        "its work goes on, so its row stays"
    );
}

/// A window that starts watching a Thread while its shell runs (it opens,
/// or the work began between two of its frames) is told the shell runs:
/// the Thread is working there too.
#[gpui::test]
fn a_window_that_starts_watching_mid_shell_sees_the_thread_working(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("nav-watch-mid-shell", 2);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    for event in [streaming(), shell(TaskStatus::Working), turn_ended()] {
        fake.streams.borrow()[1].send(event).unwrap();
    }
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.cockpit.thread(threads[1]).unwrap().background_work());
        assert_eq!(ids(&view.nav_state().working), vec![threads[1]]);
    });
}
