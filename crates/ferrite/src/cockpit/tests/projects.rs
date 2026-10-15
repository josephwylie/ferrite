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
