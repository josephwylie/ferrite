//! Projects in the cockpit: the board's sidebar entries as nav rows and
//! its tally in the bottom bar (ADR 0031), Server notifications, and the
//! adopt dialog (ADR 0030).

use super::*;
use crate::projects::Place;
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
    Sidebar {
        agents: [
            (threads[0], coordinator_told(coordinator)),
            (
                threads[1],
                Told {
                    entry: agent.clone(),
                    ink: Ink::Review,
                },
            ),
            (
                threads[2],
                Told {
                    entry: reviewer,
                    ink: Ink::Review,
                },
            ),
        ]
        .into_iter()
        .collect(),
        spaces: Default::default(),
        tally: "1 needs you \u{b7} 2 working".into(),
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
        assert_eq!(task.ink, crate::sidebar::ink(Ink::Review));
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
/// entry says them, its numbers at the heading's right.
#[gpui::test]
fn a_tasks_space_is_named_for_the_task_with_its_numbers(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-board-space", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    view.update(cx, |view, _| {
        let project = view.nav_state().sections[0]
            .project
            .expect("a named Project");
        let space = ferrite_core::api::project_id(project);
        view.projects.sidebar.spaces.insert(
            space,
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
    });
}

/// The bottom bar says the board header's tally while the board has tasks.
#[gpui::test]
fn the_bottom_bar_says_the_boards_tally(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("bar-board-tally", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    assert!(
        cx.debug_bounds("bottom-bar-projects").is_none(),
        "no tasks, no word"
    );

    view.update(cx, |view, cx| {
        view.projects.sidebar.tally = "3 need you \u{b7} 1 ready \u{b7} 2 working".into();
        cx.notify();
    });
    tick(cx);
    assert!(cx.debug_bounds("bottom-bar-projects").is_some());
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

/// "Continue this Thread as a project" opens its dialog only once the
/// adoptable-agent check passed; a refusal is a toast, and no dialog.
#[gpui::test]
fn adopting_opens_the_dialog_only_once_the_check_passed(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("projects-adopt", 1);
    let thread = core.threads()[0];
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let place = || Place {
        root: std::env::temp_dir().join("ferrite-projects-adopt-unused"),
        config_dir: std::env::temp_dir().join("ferrite-projects-adopt-unused"),
    };
    let before = toasts(cx);

    view.update_in(cx, |view, window, cx| {
        let refusal = Err("no agent is detected in pane 1".to_string());
        view.adopt_or_refuse(place(), thread, refusal, window, cx);
        assert!(view.projects.sheet.is_none(), "refused: no dialog");
    });
    assert_eq!(toasts(cx), before + 1, "the refusal is a toast");

    view.update_in(cx, |view, window, cx| {
        view.adopt_or_refuse(place(), thread, Ok(()), window, cx);
        assert!(view.projects.sheet.is_some(), "the dialog");
    });
}
