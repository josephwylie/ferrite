//! Projects in the cockpit: the board's sidebar entries as nav rows and
//! its tally in the bottom bar, Server notifications, and the work board
//! in the window (ADR 0031).

use super::*;
use crate::board::render;
use crate::board::Placement;
use crate::cockpit::drawn;
use crate::sidebar::{coordinator_told, Sidebar, Told};
use sw_board::flow::sidebar_entry::{coordinator_entry, Entry};
use sw_board::shown::ink::Ink;

fn sorted_threads(core: &Cockpit) -> Vec<ThreadId> {
    let mut threads = core.threads();
    threads.sort();
    threads
}

fn entry(name: &str, stands: &str, age: &str, progress: &str) -> Entry {
    Entry {
        name: name.into(),
        place: stands.into(),
        stands: stands.into(),
        age: age.into(),
        line: String::new(),
        progress: progress.into(),
    }
}

/// `threads[0]` is the Coordinator, `threads[1]` a task's agent at work and
/// `threads[2]` its reviewer; `threads[3]` is no agent of the board's.
fn sidebar(threads: &[ThreadId]) -> Sidebar {
    let task = "HSW-152 Show automations in the main list";
    let agent = entry(task, "agent \u{b7} under review \u{b7} R#1", "6m", "40% 3m");
    let reviewer = entry(
        task,
        "reviewer \u{b7} under review \u{b7} R#1",
        "6m",
        "40% 3m",
    );
    let coordinator = coordinator_entry("idle", 1, "Done.\nBoth tasks are added.");
    let theme = render::ferrite_theme(false);
    let review = render::ink(&theme, Ink::Review);
    Sidebar {
        agents: [
            (threads[0], coordinator_told(coordinator, &theme)),
            (
                threads[1],
                Told {
                    entry: agent.clone(),
                    ink: review,
                },
            ),
            (
                threads[2],
                Told {
                    entry: reviewer,
                    ink: review,
                },
            ),
        ]
        .into_iter()
        .collect(),
        tally: "1 needs you \u{b7} 2 working".into(),
        ..Sidebar::default()
    }
}

/// Each live agent's Thread is its entry: row 1 the task's number and
/// title (the Thread's own name hidden, no word at its right), rows 2 and 3
/// under it; the Coordinator's message under its row; a Thread the board
/// does not run keeps its own row.
#[gpui::test]
fn the_boards_agents_are_their_sidebar_entries(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-board-rows", 4);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.update(cx, |view, _| {
        view.projects.sidebar = sidebar(&threads);
        let state = view.nav_state();
        let rows: Vec<&nav::ThreadRow> = state.ordered_solos();
        let row = |thread: ThreadId| *rows.iter().find(|row| row.thread == thread).unwrap();

        let agent = row(threads[1]);
        assert_eq!(
            agent.name.as_ref(),
            "HSW-152 Show automations in the main list"
        );
        assert_eq!(
            (&agent.tail, agent.details.text()),
            (&nav::NavTail::None, None)
        );
        let task = agent.task.as_ref().expect("the agent's entry");
        assert_eq!(
            (task.stands_timed.as_ref(), task.third.as_ref()),
            ("agent \u{b7} under review \u{b7} R#1 \u{b7} 6m", "40% 3m")
        );
        let review = render::ink(&render::ferrite_theme(false), Ink::Review);
        assert_eq!(task.ink, review);
        assert_eq!(
            row(threads[2])
                .task
                .as_ref()
                .map(|task| task.stands.as_ref()),
            Some("reviewer \u{b7} under review \u{b7} R#1"),
            "the reviewer's own entry"
        );

        let coordinator = row(threads[0]);
        assert_eq!(coordinator.name.as_ref(), "Coordinator");
        let said = coordinator.task.as_ref().expect("the Coordinator's entry");
        assert_eq!(
            (said.stands.as_ref(), said.third.as_ref()),
            (
                "Coordinator \u{b7} idle \u{b7} 1 message waits to be sent",
                "Done. Both tasks are added."
            )
        );

        let own = row(threads[3]);
        assert!(own.task.is_none(), "no agent of the board's");
        assert_eq!(own.name, view.facts.name(threads[3]));
    });
}

/// A task archived or removed while its Thread lives has the board's words
/// taken off: its row is the Thread's own again.
#[gpui::test]
fn a_thread_whose_task_went_keeps_its_own_row(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-board-gone", 2);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.update(cx, |view, _| {
        view.projects.sidebar = sidebar(&[threads[0], threads[1], threads[1]]);
        assert!(view.thread_row(threads[1]).task.is_some());
        view.projects.sidebar = Sidebar::default();
        let row = view.thread_row(threads[1]);
        assert!(row.task.is_none());
        assert_eq!(row.name, view.facts.name(threads[1]));
    });
}

/// A task's own space is headed with the task's number and title as the
/// entry says them, its branch still at the heading's right and its
/// numbers under it: the Space the board made for it, its Thread in it.
#[gpui::test]
fn a_tasks_space_is_named_for_the_task_with_its_numbers(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-board-space", 1);
    let thread = core.threads()[0];
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.update(cx, |view, _| {
        let project = view.nav_state().sections[0]
            .project
            .expect("a named Project");
        let branch = view.nav_state().sections[0].branch.clone();
        let sidebar = &mut view.projects.sidebar;
        sidebar.places.insert(thread, "s1".into());
        sidebar.spaces.insert(
            "s1".into(),
            entry(
                "HSW-152 Show automations",
                "agent \u{b7} working",
                "6m",
                "8% 16m",
            ),
        );
        let state = view.nav_state();
        let section = state
            .sections
            .iter()
            .find(|section| section.project == Some(project))
            .expect("the space's section");
        assert_eq!(section.label.as_ref(), "HSW-152 Show automations");
        assert_eq!(section.numbers.as_deref(), Some("8% 16m"));
        assert_eq!(section.branch, branch, "the branch stays");
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

    let board = focused_board(&view, Placement::Tab, cx);
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

/// The work board at `placement`, staged and holding the keyboard.
fn focused_board(
    view: &Entity<CockpitView>,
    placement: Placement,
    cx: &mut gpui::VisualTestContext,
) -> Entity<crate::board::BoardView> {
    view.update_in(cx, |view, window, cx| {
        view.stage_board(crate::board::fixture::board(), placement, window, cx)
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
    let board = focused_board(&view, Placement::Tab, cx);

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

/// ⌘] steps to the next Pane of a Group from the board too: the board
/// gives way.
#[gpui::test]
fn the_next_pane_key_moves_the_tab_board_aside(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("board-next-pane", 2);
    let group = group_all(&mut core);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);
    let before = view.read_with(cx, |view, _| view.focused_thread());
    let board = focused_board(&view, Placement::Tab, cx);

    cx.simulate_keystrokes("cmd-]");
    tick(cx);

    view.read_with(cx, |view, _| {
        assert!(!view.projects.board_shown(), "the board gave way");
        assert_ne!(view.focused_thread(), before, "the next Pane");
    });
    assert!(!board_holds_the_keyboard(&board, cx));
}

/// A press on a Thread's nav row closes the popup, as herdr's closes when
/// another tab shows: it is never left over the Thread.
#[gpui::test]
fn a_nav_row_click_closes_the_popup(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("popup-nav-click", 2);
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
    let board = focused_board(&view, Placement::Popup, cx);

    let row = cx
        .debug_bounds(format!("nav-thread-{}", other.get()).leak())
        .expect("the other Thread's nav row");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    tick(cx);

    view.read_with(cx, |view, _| {
        assert!(view.projects.board.is_none(), "the popup closed");
        assert_eq!(view.focused_thread(), Some(other));
    });
    assert!(
        !board_holds_the_keyboard(&board, cx),
        "the keyboard went to the Thread"
    );
}

/// ⌘] from the popup steps to the next Pane and closes the popup.
#[gpui::test]
fn the_next_pane_key_closes_the_popup(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("popup-next-pane", 2);
    let group = group_all(&mut core);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);
    let before = view.read_with(cx, |view, _| view.focused_thread());
    let board = focused_board(&view, Placement::Popup, cx);

    cx.simulate_keystrokes("cmd-]");
    tick(cx);

    view.read_with(cx, |view, _| {
        assert!(view.projects.board.is_none(), "the popup closed");
        assert_ne!(view.focused_thread(), before, "the next Pane");
    });
    assert!(!board_holds_the_keyboard(&board, cx));
}

/// The board key on a board that shows but lost the keyboard (a press in
/// the Composer) takes you back to the board, not away from it; pressed
/// on the board, it takes you back where you were. (Where you were is
/// drawn: the window's focus pump moves the keyboard off a handle that
/// is not.)
#[gpui::test]
fn the_board_key_refocuses_a_shown_board(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("board-refocus", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let board = focused_board(&view, Placement::Tab, cx);
    let elsewhere = view.update_in(cx, |view, window, cx| {
        let elsewhere = view.panes[view.focused()]
            .composer
            .read(cx)
            .focus_handle(cx);
        window.focus(&elsewhere, cx);
        elsewhere
    });
    assert!(!board_holds_the_keyboard(&board, cx));

    view.update_in(cx, |view, window, cx| {
        view.projects.live = true;
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

/// The board left the bar (SPEC §5.7: the nav's `Tasks` view takes its
/// door and tally): with tasks on a live board the bar carries neither,
/// and the board's key still opens it.
#[gpui::test]
fn the_board_is_reached_by_its_key_not_the_bar(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("board-door", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let board = focused_board(&view, Placement::Tab, cx);
    view.update_in(cx, |view, window, cx| {
        let composer = view.panes[view.focused()]
            .composer
            .read(cx)
            .focus_handle(cx);
        window.focus(&composer, cx);
        view.projects.live = true;
        view.projects.sidebar.tally = "3 need you \u{b7} 1 ready".into();
        cx.notify();
    });
    tick(cx);
    for gone in ["projects-board-door", "bottom-bar-projects"] {
        assert!(cx.debug_bounds(gone).is_none(), "{gone} left the bar");
    }
    let toggles = drawn::projects_toggles();
    cx.dispatch_action(crate::projects::Toggle);
    tick(cx);
    assert_eq!(drawn::projects_toggles(), toggles + 1, "projects::Toggle");
    assert!(
        board_holds_the_keyboard(&board, cx),
        "the key shows the board"
    );
}

/// Two Projects: `here()`'s, with `threads[0]`, and another repo's, with
/// `threads[1]`, focused. Their roots, in the registry's order.
fn two_projects(name: &str) -> (Cockpit, Fake, [ThreadId; 2], [std::path::PathBuf; 2]) {
    let (mut core, fake) = cockpit(name, 1);
    let first = core.threads()[0];
    let repo = repo_in(&scratch(&format!("{name}-other")));
    let opened = core
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: repo.clone(),
            },
        )
        .unwrap();
    core.focus_thread(opened);
    let roots = core
        .registry()
        .projects()
        .iter()
        .map(|project| project.root.clone())
        .collect::<Vec<_>>();
    assert_eq!(roots.len(), 2, "two Projects: {roots:?}");
    assert_eq!(roots[1], repo.canonicalize().unwrap());
    (
        core,
        fake,
        [first, opened],
        [roots[0].clone(), roots[1].clone()],
    )
}

/// The folder a still board's new tasks go from.
fn board_cwd(board: &Entity<crate::board::BoardView>, cx: &mut gpui::VisualTestContext) -> String {
    board.read_with(cx, |board, _| board.cwd().expect("a folder").to_string())
}

/// New tasks go to the focused Thread's Project when the board opens,
/// else to the first Project.
#[gpui::test]
fn new_tasks_go_to_the_focused_threads_project(cx: &mut TestAppContext) {
    let (core, _fake, threads, roots) = two_projects("tasks-aim-focused");
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.update(cx, |view, cx| {
        assert_eq!(view.task_aim().as_ref(), Some(&roots[1]), "the focused one");
        view.focus_thread(threads[0], cx);
        assert_eq!(view.task_aim().as_ref(), Some(&roots[0]));
    });

    let (mut bare, _fake) = cockpit("tasks-aim-none", 0);
    let repo = repo_in(&scratch("tasks-aim-none-repo"));
    bare.register_project(&repo).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(bare, cx));
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.task_aim(),
            Some(repo.canonicalize().unwrap()),
            "no Thread focused: the first Project"
        );
    });
}

/// The board's strip names where new tasks go; a pick in its picker moves
/// the board there live — the selection and the words in the box stay —
/// and Enter (to the Coordinator) and Tab (to delegate) go from there.
#[gpui::test]
fn the_tasks_picker_retargets_new_tasks_live(cx: &mut TestAppContext) {
    use sw_board::board::job::Job;
    use sw_board::board::keys::Press;
    use sw_board::board::update::{update, Effect, Event};
    let (core, _fake, threads, roots) = two_projects("tasks-picker");
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    view.update(cx, |view, cx| view.focus_thread(threads[0], cx));
    tick(cx);
    let board = focused_board(&view, Placement::Tab, cx);
    assert_eq!(board_cwd(&board, cx), roots[0].display().to_string());
    assert!(cx.debug_bounds("board-aim-strip").is_some(), "the strip");
    let sel = board.update(cx, |board, _| {
        let state = board.state().unwrap();
        state.input.set_text("Fix the flaky test");
        state.sel.clone()
    });

    let trigger = cx.debug_bounds("board-aim").expect("the picker");
    cx.simulate_click(trigger.center(), gpui::Modifiers::none());
    tick(cx);
    let other = cx.debug_bounds("choice-row-1").expect("the other Project");
    cx.simulate_click(other.center(), gpui::Modifiers::none());
    tick(cx);

    assert_eq!(board_cwd(&board, cx), roots[1].display().to_string());
    board.update(cx, |board, _| {
        assert_eq!(board.aimed(), Some(1), "the strip names it");
        let state = board.state().unwrap();
        assert_eq!(state.sel, sel, "the selection stays");
        assert_eq!(
            state.value(),
            "Fix the flaky test",
            "the box keeps its words"
        );
        // Straight through `update`: what it answers is not run.
        for key in ["enter", "tab"] {
            state.input.set_text("Fix the flaky test");
            let effects = update(state, Event::Key(Press::chord(key)));
            let cwd = effects.iter().find_map(|effect| match effect {
                Effect::Run(Job::CoordSend { cwd, .. } | Job::Add { cwd, .. }) => Some(cwd),
                _ => None,
            });
            assert_eq!(
                cwd,
                Some(&roots[1].display().to_string()),
                "{key}: {effects:?}"
            );
        }
    });
}

/// A Project heading's menu opens Tasks set to that Project: an open board
/// is moved there and shown.
#[gpui::test]
fn a_project_headings_menu_opens_its_tasks(cx: &mut TestAppContext) {
    let (core, _fake, _threads, roots) = two_projects("tasks-nav");
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    hold_nav_open(&view, cx);
    let board = focused_board(&view, Placement::Tab, cx);
    assert_eq!(board_cwd(&board, cx), roots[1].display().to_string());
    view.update(cx, |view, cx| {
        view.board_gives_way();
        view.projects.live = true;
        cx.notify();
    });
    tick(cx);
    let index = view.read_with(cx, |view, _| {
        let project = view.cockpit.registry().projects()[0].id;
        view.nav_state()
            .sections
            .iter()
            .position(|section| section.project == Some(project))
            .expect("the first Project's section")
    });

    let heading = cx
        .debug_bounds(format!("nav-project-section-{index}").leak())
        .expect("its heading");
    cx.simulate_mouse_down(
        heading.center(),
        MouseButton::Right,
        gpui::Modifiers::none(),
    );
    cx.simulate_mouse_up(
        heading.center(),
        MouseButton::Right,
        gpui::Modifiers::none(),
    );
    tick(cx);
    view.update_in(cx, |view, window, cx| {
        let menu = view.context_menu.as_ref().expect("the heading's menu");
        let project = view.cockpit.registry().projects()[0].id;
        assert_eq!(menu.target, MenuTarget::Project(project));
        let labels: Vec<_> = menu
            .rows
            .iter()
            .flatten()
            .map(|(item, _)| item.label.to_string())
            .collect();
        assert_eq!(labels, ["Open tasks for this project"]);
        view.press_menu_row(0, window, cx);
    });
    tick(cx);

    assert_eq!(board_cwd(&board, cx), roots[0].display().to_string());
    assert!(view.read_with(cx, |view, _| view.projects.board_shown()));
    assert!(board_holds_the_keyboard(&board, cx), "shown, with the keys");
}

#[gpui::test]
fn board_owned_threads_stay_out_of_navigation_even_when_opened(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("board-hidden", 4);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.update(cx, |view, cx| {
        view.projects.sidebar.owned = threads[..3].iter().copied().collect();
        for thread in &threads[..3] {
            let at = view
                .panes
                .iter()
                .position(|p| p.thread() == Some(*thread))
                .unwrap();
            view.focus_pane(at);
            let state = view.nav_state();
            assert_eq!(
                state
                    .ordered_solos()
                    .iter()
                    .map(|r| r.thread)
                    .collect::<Vec<_>>(),
                vec![threads[3]]
            );
            assert!(
                view.cockpit.thread(*thread).is_some(),
                "board can still open the agent"
            );
        }
        view.cockpit.park(threads[1]).unwrap();
        view.pump(cx);
        assert!(!view.parked_threads().contains(&threads[1]));
    });
}

/// The two Tasks views reuse a running Coordinator and retain the board's
/// draft/selection. Clicking or typing in the chat never leaves Tasks.
#[gpui::test]
fn coordinator_chat_keeps_history_and_board_state_without_restarting(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("tasks-chat", 2);
    let threads = sorted_threads(&core);
    let coordinator = threads[0];
    for text in [
        "First update from the Coordinator.",
        "A later update with the next steps.",
    ] {
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta { text: text.into() })
            .unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
        core.pump();
    }
    bind_band_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    let board = focused_board(&view, Placement::Tab, cx);
    let selection = board.update(cx, |board, _| {
        let state = board.state().unwrap();
        state.input.set_text("Keep this unfinished task");
        state.sel.clone()
    });
    view.update(cx, |view, cx| {
        view.projects.live = true;
        view.projects.sidebar.coordinator = Some(coordinator);
        cx.notify();
    });
    tick(cx);
    let click = |id: &'static str, cx: &mut gpui::VisualTestContext| {
        let bounds = cx.debug_bounds(id).expect(id);
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        tick(cx);
    };
    click("tasks-coordinator-tab", cx);
    assert!(cx.debug_bounds("tasks-coordinator-page").is_some());
    let transcript = view.update(cx, |view, _cx| {
        assert_eq!(view.focused_thread(), Some(coordinator));
        assert_eq!(view.visible_indices().len(), 1);
        let index = view.pane_for(coordinator).unwrap();
        view.focus_pane(index);
        assert!(
            view.projects.coordinator_shown(),
            "clicks within the chat keep its tabs"
        );
        let blocks = view
            .cockpit
            .thread(coordinator)
            .unwrap()
            .transcript()
            .blocks();
        assert!(blocks.iter().any(
            |b| ferrite_core::transcript::plain_text(std::slice::from_ref(b))
                .contains("First update")
        ));
        assert!(blocks.iter().any(
            |b| ferrite_core::transcript::plain_text(std::slice::from_ref(b))
                .contains("later update")
        ));
        view.panes[index].transcript().unwrap().clone()
    });
    cx.simulate_input("What is next?");
    click("tasks-board-tab", cx);
    assert!(board_holds_the_keyboard(&board, cx));
    board.update(cx, |board, _| {
        let state = board.state().unwrap();
        assert_eq!(state.sel, selection);
        assert_eq!(state.value(), "Keep this unfinished task");
    });
    click("tasks-coordinator-tab", cx);
    view.read_with(cx, |view, cx| {
        let pane = &view.panes[view.pane_for(coordinator).unwrap()];
        assert_eq!(
            pane.transcript().unwrap().entity_id(),
            transcript.entity_id()
        );
        assert_eq!(pane.composer.read(cx).text(), "What is next?");
    });
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(fake
        .sent
        .borrow()
        .iter()
        .any(|text| text == "What is next?"));
    assert_eq!(fake.spawned.borrow().len(), 2, "neither agent restarted");
    assert_eq!(*fake.interrupts.borrow(), 0);
    view.update(cx, |view, cx| view.land_on_thread(threads[1], cx));
    assert!(!view.read_with(cx, |view, _| view.projects.coordinator_shown()));
}
