//! Projects in the cockpit (ADR 0030): the nav's project blocks — the
//! coordinator heading its project, its Threads by need under it, nothing
//! listed twice — the nav's focus on one project, and the plain rows while
//! no ticker runs.

use super::*;
use crate::projects::{Model, Place, ProjectEntry, ThreadEntry};
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
