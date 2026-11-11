//! The Archive and recovery in the window (#226): Completed Tasks leave
//! the board for the Archive, Clear asks for it at once, the Archive is
//! searched and an archived Task opens read-only with its activity and
//! Worker history; an unreadable Factory store is said on the board.
#[allow(unused_imports)]
use super::*;
use ferrite_core::factory::{Board, Status, Task};
use ferrite_core::protocol::TaskAction;

fn shows(cx: &mut gpui::VisualTestContext, selector: &str) -> bool {
    debug_bounds(cx, selector.to_string()).is_some()
}

fn click(cx: &mut gpui::VisualTestContext, selector: &str) {
    let at = bounds(cx, selector.to_string());
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
}

/// A Task as the Server would publish it, with `extra` laid over it.
fn task(id: &str, project: ProjectId, status: Status, extra: serde_json::Value) -> Task {
    let mut task = serde_json::json!({
        "id": id,
        "project": project,
        "title": format!("Task {id}"),
        "instructions": format!("Do the work of {id}"),
        "status": status,
        "agent": {"provider": "claude", "model": "claude-opus-4"},
        "branch": format!("factory/{id}"),
        "activity": [
            {"at_ms": 1, "by": "ferrite", "text": "Started the Worker"},
            {"at_ms": 2, "by": "agent", "text": "Finished: Fixed it"},
        ],
        "created_ms": 1,
    });
    for (key, value) in extra.as_object().unwrap() {
        task[key] = value.clone();
    }
    serde_json::from_value(task).unwrap()
}

/// A window on a board of a Task archived, one Completed and one
/// Working, whose Task asks are kept, its Factory open.
fn world<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (
    Entity<CockpitView>,
    &'a mut gpui::VisualTestContext,
    ProjectId,
) {
    let (mut core, _fake) = cockpit(name, 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_record_tasks();
    core.fixture_factory(Board {
        tasks: vec![
            task(
                "t-1",
                project,
                Status::Completed,
                serde_json::json!({
                    "result": "Fixed the race in setup",
                    "instructions": "Fix the flaky login test\nIt fails one run in ten.",
                    "title": "Fix the flaky login test",
                    "worker": 41,
                    "workers": [{"step": "s-1", "thread": 40}, {"step": "s-2", "thread": 41}],
                    "completed_ms": 1_000,
                    "archived_ms": 2_000,
                }),
            ),
            task(
                "t-2",
                project,
                Status::Completed,
                serde_json::json!({ "result": "Bumped", "completed_ms": 3_000 }),
            ),
            task("t-3", project, Status::Working, serde_json::json!({})),
        ],
        ..Default::default()
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(2200.), px(1000.)));
    tick(cx);
    view.update(cx, |view, cx| view.open_factory(project, cx));
    tick(cx);
    (view, cx, project)
}

/// AC: an archived Task is off the board; the Completed column's Clear
/// asks the Server to archive the Project's Completed Tasks at once.
#[gpui::test]
fn archived_tasks_leave_the_board_and_clear_asks_for_the_rest(cx: &mut TestAppContext) {
    let (view, cx, project) = world(cx, "archive-clear");
    assert!(shows(cx, "factory-card-t-2"));
    assert!(shows(cx, "factory-card-t-3"));
    assert!(!shows(cx, "factory-card-t-1"), "archived: off the board");
    assert!(shows(cx, "factory-archive"), "the Archive is offered");
    click(cx, "factory-clear");
    let asks = view.read_with(cx, |view, _| view.cockpit.fixture_task_asks().to_vec());
    assert_eq!(asks, vec![TaskAction::Clear { project }]);
}

/// AC: the Archive opens and is searched by title and instructions; an
/// archived Task opens read-only, with its activity and its Workers.
#[gpui::test]
fn the_archive_is_searched_and_opens_a_task_read_only(cx: &mut TestAppContext) {
    let (view, cx, _) = world(cx, "archive-search");
    click(cx, "factory-archive");
    assert!(shows(cx, "factory-archive-list"));
    assert!(shows(cx, "factory-archive-t-1"));
    assert!(!shows(cx, "factory-archive-t-2"), "not archived");

    click(cx, "factory-archive-search");
    cx.simulate_input("one run in ten");
    tick(cx);
    assert!(
        shows(cx, "factory-archive-t-1"),
        "found by its instructions"
    );
    view.update(cx, |view, cx| view.set_archive_search("deploy", cx));
    tick(cx);
    assert!(!shows(cx, "factory-archive-t-1"));
    assert!(shows(cx, "factory-archive-empty"));
    view.update(cx, |view, cx| view.set_archive_search("FLAKY LOGIN", cx));
    tick(cx);

    click(cx, "factory-archive-t-1");
    assert!(shows(cx, "task-panel"));
    assert!(shows(cx, "task-archived"), "said archived");
    for action in ["task-delete", "task-resume", "task-stop"] {
        assert!(!shows(cx, action), "read-only: {action}");
    }
    view.update(cx, |view, cx| {
        view.task_tab(crate::cockpit::factory::TaskTab::Activity, cx)
    });
    tick(cx);
    assert!(shows(cx, "task-activity-1"), "its activity");
    view.update(cx, |view, cx| {
        view.task_tab(crate::cockpit::factory::TaskTab::Worker, cx)
    });
    tick(cx);
    assert!(shows(cx, "task-worker-history-0"));
    assert!(shows(cx, "task-worker-history-1"));

    click(cx, "factory-archive-close");
    assert!(shows(cx, "factory-card-t-2"), "back on the board");
}

/// AC: a Factory store that could not be read is said on the board.
#[gpui::test]
fn an_unreadable_store_is_said_on_the_board(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("archive-trouble", 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_factory(Board {
        trouble: Some("The Factory's store at /x/factory.json could not be read".into()),
        ..Default::default()
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(2200.), px(1000.)));
    tick(cx);
    view.update(cx, |view, cx| view.open_factory(project, cx));
    tick(cx);
    assert!(shows(cx, "factory-trouble"));
}
