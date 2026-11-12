//! A Project's Factory in the window (#220, ADR 0037): its sidebar entry,
//! the board's columns and cards, a Task's panel, and a Task's alerts.
#[allow(unused_imports)]
use super::*;
use ferrite_core::factory::{Board, Status, Task};

/// A Task as the Server would publish it.
fn task(
    id: &str,
    project: ProjectId,
    status: Status,
    worker: Option<ThreadId>,
    result: Option<&str>,
    reason: Option<&str>,
) -> Task {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "project": project,
        "title": format!("Task {id}"),
        "instructions": format!("Do the work of {id}"),
        "status": status,
        "agent": {"provider": "claude", "model": "claude-opus-4"},
        "branch": format!("factory/{id}"),
        "worker": worker,
        "result": result,
        "reason": reason,
        "activity": [
            {"at_ms": 1, "by": "ferrite", "text": "Started the Worker"},
            {"at_ms": 2, "by": "operator", "text": "Use the staging data"},
        ],
        "created_ms": 1,
    }))
    .unwrap()
}

fn shows(cx: &mut gpui::VisualTestContext, selector: &str) -> bool {
    debug_bounds(cx, selector.to_string()).is_some()
}

/// Each Project has a Factory entry over the nav's cards; a fresh one opens an
/// empty board that started nothing and says nothing needs the operator.
#[gpui::test]
fn each_project_has_a_factory_entry_and_a_fresh_one_is_empty(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("factory-entry", 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    let entry = format!("nav-factory-{}", project.get());
    let row = bounds(cx, entry.clone());
    assert!(!shows(cx, &format!("{entry}-count")), "nothing needs you");
    let streams = fake.streams.borrow().len();
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(shows(cx, "factory-board"));
    assert!(shows(cx, "factory-empty"), "no invented Tasks");
    assert!(!shows(cx, "factory-columns"));
    assert_eq!(fake.streams.borrow().len(), streams, "nothing started");
    view.read_with(cx, |view, _| {
        assert!(view.cockpit.factory().tasks.is_empty());
        assert_eq!(view.cockpit.threads().len(), 1);
    });
}

/// The board's six columns, each card in its status's column with its
/// Worker's Provider and model and its result or stop reason; the entry
/// counts what needs the operator.
#[gpui::test]
fn the_board_lays_cards_in_their_columns(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("factory-columns", 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_factory(Board {
        workflows: Vec::new(),
        tasks: vec![
            task("t-1", project, Status::Working, None, None, None),
            task(
                "t-2",
                project,
                Status::NeedsHuman,
                None,
                None,
                Some("No access to the staging database"),
            ),
            task(
                "t-3",
                project,
                Status::Completed,
                None,
                Some("Fixed it"),
                None,
            ),
            task(
                "t-4",
                project,
                Status::Stopped,
                None,
                None,
                Some("Stopped by you"),
            ),
        ],
        ..Default::default()
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    let entry = format!("nav-factory-{}", project.get());
    assert!(shows(cx, &format!("{entry}-count")), "one needs you");
    view.update(cx, |view, cx| {
        assert_eq!(view.cockpit.factory().needs_you(project), 1);
        view.open_factory(project, cx);
    });
    tick(cx);
    for column in [
        "queued",
        "working",
        "needs_human",
        "approval",
        "stopped",
        "completed",
    ] {
        assert!(shows(cx, &format!("factory-column-{column}")), "{column}");
    }
    for (card, column) in [
        ("t-1", "working"),
        ("t-2", "needs_human"),
        ("t-3", "completed"),
        ("t-4", "stopped"),
    ] {
        let card_at = bounds(cx, format!("factory-card-{card}"));
        let column_at = bounds(cx, format!("factory-column-{column}"));
        assert_eq!(card_at.origin.x, column_at.origin.x, "{card} in {column}");
        assert!(shows(cx, &format!("factory-card-{card}-agent")));
    }
    assert!(shows(cx, "factory-card-t-2-outcome"), "the reason");
    assert!(shows(cx, "factory-card-t-3-outcome"), "the result");
    assert!(shows(cx, "factory-card-t-4-outcome"), "the stop reason");
    assert!(!shows(cx, "factory-card-t-1-outcome"));
}

/// Opening a Task's panel starts no second run: its Worker tab draws the
/// Worker's own Pane. Stop is offered while it works; Delete only once
/// it is Queued, Stopped or Completed.
#[gpui::test]
fn a_tasks_panel_shows_its_worker_and_offers_delete_only_at_rest(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("factory-panel", 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    let worker = core
        .fixture_open_owned(
            Provider::Claude,
            WorkspaceChoice::Main { checkout: here() },
            ferrite_core::ThreadOwner::Task("t-1".into()),
        )
        .unwrap();
    core.fixture_factory(Board {
        workflows: Vec::new(),
        tasks: vec![task(
            "t-1",
            project,
            Status::Working,
            Some(worker),
            None,
            None,
        )],
        ..Default::default()
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    let streams = fake.streams.borrow().len();
    view.update(cx, |view, cx| view.open_task("t-1", cx));
    tick(cx);
    assert!(shows(cx, "task-panel"));
    assert!(shows(cx, "task-status"));
    assert!(shows(cx, "task-stop"));
    assert!(!shows(cx, "task-delete"), "not while it works");
    let tab = bounds(cx, "task-tab-activity".into());
    cx.simulate_click(tab.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(shows(cx, "task-activity-0") && shows(cx, "task-activity-1"));
    view.update(cx, |view, cx| {
        view.task_tab(crate::cockpit::factory::TaskTab::Context, cx)
    });
    tick(cx);
    assert!(shows(cx, "task-instructions"));
    view.update(cx, |view, cx| {
        view.task_tab(crate::cockpit::factory::TaskTab::Worker, cx)
    });
    tick(cx);
    assert!(shows(cx, "task-worker"), "the Worker's Pane");
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused_thread(), Some(worker));
        assert!(view.factory_view.is_some(), "the board stays");
    });
    assert_eq!(fake.streams.borrow().len(), streams, "no second run");

    for (status, delete, resume) in [
        (Status::Queued, true, false),
        (Status::NeedsHuman, false, false),
        (Status::Stopped, true, true),
        (Status::Completed, true, false),
    ] {
        view.update(cx, |view, cx| {
            view.cockpit.fixture_factory(Board {
                workflows: Vec::new(),
                tasks: vec![task("t-1", project, status, Some(worker), None, None)],
                ..Default::default()
            });
            view.open_task("t-1", cx);
        });
        tick(cx);
        assert_eq!(shows(cx, "task-delete"), delete, "{status:?}");
        assert_eq!(shows(cx, "task-resume"), resume, "{status:?}");
    }
}

/// A Task that needs the operator toasts, chimes and posts a system
/// notification while the operator works in a Thread of their own off
/// the board's Factory; nothing reaches the Inbox.
#[gpui::test]
fn a_task_alert_toasts_chimes_and_posts_but_not_to_the_inbox(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("factory-alert", 2);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_factory(Board {
        workflows: Vec::new(),
        tasks: vec![task(
            "t-1",
            project,
            Status::NeedsHuman,
            None,
            None,
            Some("No access"),
        )],
        ..Default::default()
    });
    cx.update(|cx| cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite"));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    let toasts = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            use gpui::component::WindowExt as _;
            window.notifications(cx).len()
        })
    };
    let before = toasts(cx);
    let inbox_len = |view: &CockpitView, cx: &gpui::App| {
        let state = view.nav_state();
        view.inbox_rows(&state, cx).len()
    };
    let inbox_before = view.read_with(cx, |view, cx| inbox_len(view, cx));
    view.update(cx, |view, cx| {
        let watched = view.cockpit.threads()[0];
        view.focus_thread(watched, cx);
        view.cockpit
            .fixture_task_alert(ferrite_core::protocol::ServerMessage::TaskAlert {
                project,
                task: "t-1".into(),
                status: Status::NeedsHuman,
                title: "Task t-1".into(),
            });
        cx.notify();
    });
    tick(cx);
    ring(cx);
    assert_eq!(toasts(cx), before + 1, "a toast");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.chimed, [Sound::NeedsYou], "a chime");
        assert_eq!(
            inbox_len(view, cx),
            inbox_before,
            "nothing reaches the Inbox"
        );
        assert!(view.needs_you.toasts().is_empty());
    });
    assert_eq!(
        cx.delivered_system_notifications()
            .iter()
            .map(|notification| notification.tag.to_string())
            .collect::<Vec<_>>(),
        [crate::cockpit::factory::task_tag("t-1")]
    );
}

/// A Task's toast says the event it announced and goes when the Task
/// leaves that state, as a Thread's goes once answered; an alert for a
/// state the Task is already out of says nothing.
#[gpui::test]
fn a_task_toast_goes_when_the_task_leaves_the_state_it_announced(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("factory-alert-leaves", 2);
    let project = core.project_id(core.threads()[0]).unwrap();
    let needs_human = |status, result| Board {
        workflows: Vec::new(),
        tasks: vec![task(
            "t-1",
            project,
            status,
            None,
            result,
            Some("No access"),
        )],
        ..Default::default()
    };
    core.fixture_factory(needs_human(Status::NeedsHuman, None));
    cx.update(|cx| cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite"));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    let toasts = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            use gpui::component::WindowExt as _;
            window.notifications(cx).len()
        })
    };
    let alert = |view: &mut CockpitView, cx: &mut Context<CockpitView>| {
        view.cockpit
            .fixture_task_alert(ferrite_core::protocol::ServerMessage::TaskAlert {
                project,
                task: "t-1".into(),
                status: Status::NeedsHuman,
                title: "Task needs you: Task t-1".into(),
            });
        cx.notify();
    };
    let before = toasts(cx);
    view.update(cx, |view, cx| {
        let watched = view.cockpit.threads()[0];
        view.focus_thread(watched, cx);
        alert(view, cx);
    });
    tick(cx);
    ring(cx);
    assert_eq!(toasts(cx), before + 1, "it needs the operator: a toast");

    view.update(cx, |view, cx| {
        view.cockpit
            .fixture_factory(needs_human(Status::Working, None));
        cx.notify();
    });
    tick(cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    tick(cx);
    assert_eq!(toasts(cx), before, "answered: the toast goes");

    view.update(cx, |view, cx| {
        view.cockpit
            .fixture_factory(needs_human(Status::Completed, Some("Created hello.txt")));
        alert(view, cx);
    });
    tick(cx);
    ring(cx);
    assert_eq!(
        toasts(cx),
        before,
        "a state the Task has left is not announced"
    );
    view.read_with(cx, |view, _| assert_eq!(view.chimed, [Sound::NeedsYou]));
}

/// An empty Project's Factory opens from the nav: the Filter menu names
/// the Project, its Factory row stands, and the list's keyboard walks to
/// it and opens it.
#[gpui::test]
fn an_empty_projects_factory_opens_from_the_nav_and_its_keys(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("factory-empty-project", 1);
    let repo = repo_in(&scratch("factory-empty-project-repo"));
    let empty = core.register_project(&repo).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    view.update_in(cx, |view, window, cx| {
        view.run_menu_verb(
            MenuTarget::NavFilter,
            MenuVerb::FilterProject(Some(empty)),
            window,
            cx,
        )
    });
    tick(cx);
    assert!(
        shows(cx, &format!("nav-factory-{}", empty.get())),
        "its row"
    );
    view.update_in(cx, |view, window, cx| view.focus_nav(window, cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.nav_cursor.clone().map(|cursor| cursor.item),
            Some(nav::NavItem::Factory(empty)),
            "the keyboard starts on the Factory row"
        );
    });
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(shows(cx, "factory-board"));
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.factory_view.as_ref().map(|view| view.project),
            Some(empty)
        );
    });
}

/// Focusing a Worker by id (the CLI's `thread focus`) lands on its Task's
/// panel with the Worker tab showing, not on a free-standing Thread.
#[gpui::test]
fn focusing_a_worker_by_id_opens_its_tasks_worker_tab(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("factory-focus-worker", 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    let worker = core
        .fixture_open_owned(
            Provider::Claude,
            WorkspaceChoice::Main { checkout: here() },
            ferrite_core::ThreadOwner::Task("t-1".into()),
        )
        .unwrap();
    core.fixture_factory(Board {
        workflows: Vec::new(),
        tasks: vec![task(
            "t-1",
            project,
            Status::Working,
            Some(worker),
            None,
            None,
        )],
        ..Default::default()
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    view.update(cx, |view, cx| {
        view.cockpit
            .fixture_task_alert(ferrite_core::protocol::ServerMessage::Focus { thread: worker });
        cx.notify();
    });
    tick(cx);
    assert!(shows(cx, "task-panel"));
    assert!(shows(cx, "task-worker"), "the Worker tab");
    view.read_with(cx, |view, _| {
        let factory = view.factory_view.as_ref().expect("the Factory shows");
        assert_eq!(factory.open.as_deref(), Some("t-1"));
        assert_eq!(factory.tab, crate::cockpit::factory::TaskTab::Worker);
    });
}
