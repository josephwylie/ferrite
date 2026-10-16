//! Projects in the cockpit (ADR 0030): the nav's project blocks — the
//! coordinator heading its project, its Threads by need under it, nothing
//! listed twice — the nav's focus on one project, and the plain rows while
//! no ticker runs.

use super::*;
use crate::projects::{Model, ProjectEntry, ThreadEntry};
use ferrite_projects::thread::Group;

fn sorted_threads(core: &Cockpit) -> Vec<ThreadId> {
    let mut threads = core.threads();
    threads.sort();
    threads
}

fn entry(id: &str, thread: ThreadId, group: Group, title: &str, sub: &str) -> ThreadEntry {
    ThreadEntry {
        id: id.into(),
        thread: Some(thread),
        group,
        display: format!("{id} \u{b7} {title}"),
        sub: sub.into(),
    }
}

/// Alpha: `threads[0]` coordinates, `threads[2]` waits on the operator,
/// `threads[1]` works — listed in that order, by need.
fn model(threads: &[ThreadId], ticker: bool) -> Model {
    Model {
        ticker,
        needs_you: Some("projects: 1 need you".into()),
        projects: vec![ProjectEntry {
            slug: "alpha".into(),
            name: "Alpha".into(),
            summary: "1 need you \u{b7} 1 working".into(),
            coordinator: Some(threads[0]),
            coordinator_sub: String::new(),
            threads: vec![
                entry(
                    "t-0002",
                    threads[2],
                    Group::WaitingOnYou,
                    "Second",
                    "needs you \u{b7} blocked",
                ),
                entry("t-0001", threads[1], Group::Working, "First", ""),
            ],
        }],
    }
}

#[gpui::test]
fn a_project_heads_its_threads_by_need_and_owns_their_rows(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-projects", 4);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.update(cx, |view, _| {
        view.projects.model = model(&threads, true);
        let state = view.nav_state();
        assert_eq!(state.projects.len(), 1);
        let block = &state.projects[0];
        assert_eq!(
            (block.name.as_ref(), block.summary.as_ref()),
            ("Alpha", "1 need you \u{b7} 1 working")
        );
        assert_eq!(block.head.as_ref().map(|row| row.thread), Some(threads[0]));
        let rows: Vec<(ThreadId, &str, Option<&str>)> = block
            .rows
            .iter()
            .map(|(row, sub)| (row.thread, row.name.as_ref(), sub.as_deref()))
            .collect();
        assert_eq!(
            rows,
            [
                (
                    threads[2],
                    "t-0002 \u{b7} Second",
                    Some("needs you \u{b7} blocked")
                ),
                (threads[1], "t-0001 \u{b7} First", None),
            ]
        );
        let loose: Vec<ThreadId> = state.ordered_solos().iter().map(|row| row.thread).collect();
        assert_eq!(
            loose,
            [threads[3]],
            "a project's Threads are listed once, in its block"
        );
    });
}

#[gpui::test]
fn focus_shows_one_project_and_no_ticker_shows_the_plain_rows(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-projects-focus", 4);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.update(cx, |view, cx| {
        view.projects.model = model(&threads, true);
        view.focus_nav_on_project(Some("alpha".into()), cx);
        let state = view.nav_state();
        assert_eq!(state.projects.len(), 1);
        assert!(state.sections.is_empty(), "focused: the project alone");
        view.focus_nav_on_project(None, cx);
        assert!(
            !view.nav_state().sections.is_empty(),
            "unfocused: everything again"
        );

        // A stopped ticker's records are stale: plain rows, every Thread.
        view.projects.model = model(&threads, false);
        let state = view.nav_state();
        assert!(state.projects.is_empty());
        let loose: Vec<ThreadId> = state.ordered_solos().iter().map(|row| row.thread).collect();
        assert_eq!(loose, threads);
    });
}

/// The toasts the window shows now.
fn toasts(cx: &mut gpui::VisualTestContext) -> usize {
    cx.update(|window, cx| {
        gpui::component::Root::read(window, cx)
            .notification
            .read(cx)
            .notifications()
            .len()
    })
}

/// `notification.show` from the Server: a toast with the chime it names —
/// request when the operator is needed, done for a result, none otherwise.
#[gpui::test]
fn a_server_notification_rings_the_chime_it_names(cx: &mut TestAppContext) {
    use ferrite_core::cockpit::Asked;
    use ferrite_core::protocol::WireSound;
    let (core, _fake) = cockpit("projects-chime", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.update(cx, |view, _| view.chimed.clear());
    let before = toasts(cx);

    view.update_in(cx, |view, window, cx| {
        for (sound, important) in [
            (WireSound::Request, true),
            (WireSound::Done, false),
            (WireSound::None, false),
        ] {
            let asked = Asked::Notification {
                title: "Alpha \u{b7} t-0001".into(),
                body: Some("needs you".into()),
                sound: sound.sound(),
                important,
            };
            view.present(asked, window, cx);
        }
        assert_eq!(view.chimed, [Sound::NeedsYou, Sound::Done]);
    });
    assert_eq!(toasts(cx), before + 3, "each one a toast");
}

/// The all-sounds switch off silences a Server notification's chime too;
/// the toast still shows.
#[gpui::test]
fn a_server_notification_is_silent_with_sounds_off(cx: &mut TestAppContext) {
    use ferrite_core::cockpit::Asked;
    use ferrite_core::protocol::WireSound;
    let (core, _fake) = cockpit("projects-chime-off", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.update(cx, |view, _| {
        view.chimed.clear();
        view.prefs.settings.sounds = false;
    });
    let before = toasts(cx);

    view.update_in(cx, |view, window, cx| {
        let asked = Asked::Notification {
            title: "Alpha \u{b7} t-0001".into(),
            body: Some("needs you".into()),
            sound: WireSound::Request.sound(),
            important: true,
        };
        view.present(asked, window, cx);
        assert!(view.chimed.is_empty(), "sounds off: no chime");
    });
    assert_eq!(toasts(cx), before + 1, "the toast still shows");
}

/// `nav.focus` from the Server focuses the nav as the palette's `focus`
/// does, and its clear shows everything again.
#[gpui::test]
fn the_servers_nav_focus_shows_one_project_and_its_clear_all(cx: &mut TestAppContext) {
    use ferrite_core::cockpit::Asked;
    let (core, _fake) = cockpit("projects-nav-focus", 4);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.update_in(cx, |view, window, cx| {
        view.projects.model = model(&threads, true);
        view.present(Asked::NavFocus(Some("alpha".into())), window, cx);
        let state = view.nav_state();
        assert_eq!(state.projects.len(), 1);
        assert_eq!(
            state.projects[0].head.as_ref().map(|row| row.thread),
            Some(threads[0]),
            "the coordinator first"
        );
        assert!(state.sections.is_empty(), "focused: the project alone");
        view.present(Asked::NavFocus(None), window, cx);
        assert_eq!(view.projects.focus, None);
        assert!(!view.nav_state().sections.is_empty(), "everything again");
    });
}

/// At wall range y/n/a and the digits answer the flagged Thread; a focused
/// work board reads them as its own keys instead, and the Decision still
/// waits.
#[gpui::test]
fn a_board_at_wall_range_takes_the_walls_answer_keys(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("wall-board", 24);
    let group = group_all(&mut core);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1060.), px(770.)));
    cx.update(|window, cx| assert_eq!(view.read(cx).level_now(window), Level::Wall));
    fake.streams.borrow()[7].send(decision("perm_08")).unwrap();
    tick(cx);
    let flagged = view.read_with(cx, |view, _| view.panes[7].thread().unwrap());

    view.update_in(cx, |view, window, cx| {
        view.stage_board(crate::board::fixture::board(), window, cx)
    });
    let board = view.read_with(cx, |view, _| {
        view.projects
            .board
            .as_ref()
            .expect("the board")
            .view
            .clone()
    });
    cx.update(|window, cx| {
        let focus = board.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    tick(cx);
    cx.update(|window, cx| assert_eq!(view.read(cx).level_now(window), Level::Wall));

    cx.simulate_keystrokes("y n a 1 2 3");

    board.update(cx, |board, _| {
        assert_eq!(board.state().expect("the board's state").value(), "yna123");
    });
    view.read_with(cx, |view, _| {
        assert!(
            view.cockpit
                .thread(flagged)
                .and_then(|open| open.pending())
                .is_some(),
            "the wall's Decision still waits"
        );
    });
}

/// The tab's work board, staged over the Panes and holding the keyboard.
fn focused_tab_board(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> Entity<crate::board::BoardView> {
    view.update_in(cx, |view, window, cx| {
        view.stage_board(crate::board::fixture::board(), window, cx)
    });
    let board = view.read_with(cx, |view, _| {
        view.projects
            .board
            .as_ref()
            .expect("the board")
            .view
            .clone()
    });
    cx.update(|window, cx| {
        let focus = board.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    tick(cx);
    board
}

fn board_holds_the_keyboard(
    board: &Entity<crate::board::BoardView>,
    cx: &mut gpui::VisualTestContext,
) -> bool {
    cx.update(|window, cx| board.read(cx).focus.contains_focused(window, cx))
}

/// A press on a Thread's nav row lands on that Thread: the tab's board
/// gives way, still open, and the keyboard goes to the Thread, as herdr's
/// board tab gives way to the tab you pick.
#[gpui::test]
fn a_nav_row_click_moves_the_tab_board_aside(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("board-nav-click", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    hold_nav_open(&view, cx);
    let other = view.read_with(cx, |view, _| {
        let focused = view.focused_thread();
        view.panes
            .iter()
            .filter_map(|pane| pane.thread())
            .find(|thread| Some(*thread) != focused)
            .expect("a second Thread")
    });
    let board = focused_tab_board(&view, cx);

    let row = cx
        .debug_bounds(format!("nav-thread-{}", other.get()).leak())
        .expect("the other Thread's nav row");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    tick(cx);

    view.read_with(cx, |view, _| {
        assert!(!view.projects.board_shown(), "the board gave way");
        assert!(view.projects.board.is_some(), "a tab's board stays open");
        assert_eq!(view.focused_thread(), Some(other));
    });
    assert!(
        !board_holds_the_keyboard(&board, cx),
        "the keyboard went to the Thread"
    );
}

/// ⌘] steps to the next Pane from the board too: the board gives way.
#[gpui::test]
fn the_next_pane_key_moves_the_tab_board_aside(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("board-next-pane", 2);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let before = view.read_with(cx, |view, _| view.focused_thread());
    let board = focused_tab_board(&view, cx);

    cx.simulate_keystrokes("cmd-]");
    tick(cx);

    view.read_with(cx, |view, _| {
        assert!(!view.projects.board_shown(), "the board gave way");
        assert_ne!(view.focused_thread(), before, "the next Pane");
    });
    assert!(!board_holds_the_keyboard(&board, cx));
}

/// The board key on a board that shows but lost the keyboard (a press in
/// the nav) takes you back to the board, not away from it.
#[gpui::test]
fn the_board_key_refocuses_a_shown_board(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("board-refocus", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let board = focused_tab_board(&view, cx);
    let elsewhere = cx.update(|window, cx| {
        let elsewhere = cx.focus_handle();
        window.focus(&elsewhere, cx);
        elsewhere
    });
    assert!(!board_holds_the_keyboard(&board, cx));

    view.update_in(cx, |view, window, cx| {
        view.projects.place = Some(crate::projects::Place {
            root: "/nonexistent/projects".into(),
            config_dir: "/nonexistent/projects-config".into(),
        });
        view.toggle_projects(window, cx);
    });
    assert!(view.read_with(cx, |view, _| view.projects.board_shown()));
    assert!(board_holds_the_keyboard(&board, cx), "back on the board");

    view.update_in(cx, |view, window, cx| view.toggle_projects(window, cx));
    assert!(
        !view.read_with(cx, |view, _| view.projects.board_shown()),
        "focused, the key leaves it"
    );
    cx.update(|window, _| assert!(elsewhere.is_focused(window)));
}
