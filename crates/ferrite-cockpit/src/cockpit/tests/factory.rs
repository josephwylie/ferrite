//! A Project's Factory in the window (#220, ADR 0037): its sidebar entry,
//! the board's columns and cards, a Task's panel, and a Task's alerts.
#[allow(unused_imports)]
use super::*;
use ferrite_core::factory_board::{Board, Status, Task};

/// A Task as the Server would publish it.
fn task(
    id: &str,
    project: ProjectId,
    status: Status,
    worker: Option<ThreadKey>,
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
        "worker": worker.map(ThreadKey::thread),
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

fn click(cx: &mut gpui::VisualTestContext, selector: &str) {
    let at = bounds(cx, selector.to_string());
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
}

/// The Factory button in the search row opens a menu with a row per Project,
/// not a row per Project in the list; a fresh Project's row opens an empty
/// board that started nothing, and nothing needs the operator.
#[gpui::test]
fn the_factory_button_lists_each_project_and_a_fresh_one_is_empty(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("factory-entry", 1);
    let project = core.project_of(core.threads()[0]).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    assert!(
        !shows(cx, &format!("nav-factory-{}", project.project().get())),
        "no row in the list"
    );
    assert!(!shows(cx, "nav-factory-attention"), "nothing needs you");
    click(cx, "nav-factory");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.context_menu().map(|menu| menu.target),
            Some(MenuTarget::NavFactory)
        );
    });
    assert!(shows(cx, "context-menu-row-0"));
    assert!(!shows(cx, "context-menu-row-1"), "one row per Project");
    let button = bounds(cx, "nav-factory".to_string());
    let menu = bounds(cx, "context-menu".to_string());
    assert!(
        menu.top() > button.bottom() && (menu.left() - button.left()).abs() < px(1.),
        "the menu hangs under the button from its left edge: {menu:?} {button:?}"
    );
    let streams = fake.streams.borrow().len();
    click(cx, "context-menu-row-0");
    tick(cx);
    assert!(shows(cx, "factory-board"));
    assert!(shows(cx, "factory-empty"), "no invented Tasks");
    assert!(!shows(cx, "factory-columns"));
    assert_eq!(fake.streams.borrow().len(), streams, "nothing started");
    view.read_with(cx, |view, cx| {
        assert!(view.cockpit.read(cx).factory(view.cockpit.read(cx).first_machine()).tasks.is_empty());
        assert_eq!(view.cockpit.read(cx).threads().len(), 1);
    });
}

/// Opening a Task's panel starts no second run: its Worker tab, pressed,
/// draws the Worker's own Pane, which takes the keyboard, and the board
/// stays. What the panel offers by status is `ferrite_factory::board`'s.
#[gpui::test]
fn a_tasks_worker_tab_draws_its_own_pane_and_takes_the_keyboard(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("factory-panel", 1);
    let project = core.project_of(core.threads()[0]).unwrap();
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
            project.project(),
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
    view.update(cx, |view, cx| view.open_factory(project, cx));
    tick(cx);
    click(cx, "factory-card-t-1");
    assert!(shows(cx, "task-panel"), "the card opened its panel");
    click(cx, "task-tab-activity");
    assert!(shows(cx, "task-activity-0") && shows(cx, "task-activity-1"));
    click(cx, "task-tab-worker");
    assert!(shows(cx, "task-worker"), "the Worker's Pane");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(cx), Some(worker));
        assert!(view.factory().is_some(), "the board stays");
    });
    assert_eq!(fake.streams.borrow().len(), streams, "no second run");
}

/// A Task that needs the operator toasts, chimes and posts a system
/// notification while the operator works in a Thread of their own off
/// the board's Factory; nothing reaches the Inbox.
#[gpui::test]
fn a_task_alert_toasts_chimes_and_posts_but_not_to_the_inbox(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("factory-alert", 2);
    let project = core.project_of(core.threads()[0]).unwrap();
    core.fixture_factory(Board {
        workflows: Vec::new(),
        tasks: vec![task(
            "t-1",
            project.project(),
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
        view.needs_you_rows(cx).len()
    };
    let inbox_before = view.read_with(cx, |view, cx| inbox_len(view, cx));
    view.update(cx, |view, cx| {
        let watched = view.cockpit.read(cx).threads()[0];
        view.focus_thread(watched, cx);
        view.cockpit
            .update(cx, |cockpit, _| cockpit.fixture_task_alert(ferrite_core::protocol::ServerMessage::TaskAlert {
                project: project.project(),
                task: "t-1".into(),
                status: Status::NeedsHuman,
                title: "Task t-1".into(),
                detail: Some("No access".into()),
            }));
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
        assert!(view.toasts.read(cx).needs_you().toasts().is_empty());
    });
    assert_eq!(
        cx.delivered_system_notifications()
            .iter()
            .map(|notification| notification.tag.to_string())
            .collect::<Vec<_>>(),
        [crate::cockpit::factory::task_tag(&ferrite_core::MachineKey(None), "t-1")]
    );
    // Its click lands on the Task, on its machine.
    let tag = cx.delivered_system_notifications()[0].tag.to_string();
    view.update(cx, |view, cx| view.open_system_notification(&tag, cx));
    view.read_with(cx, |view, cx| {
        assert_eq!(view.open_task_now(cx).map(|task| task.id.as_str()), Some("t-1"));
    });
}

/// A Task's notification tag names its machine, as a Thread's does: Task
/// ids count from 1 on each machine, so `t-1` here and `t-1` on box are
/// two Tasks, and a click reads back which.
#[test]
fn a_tasks_tag_names_its_machine() {
    use crate::cockpit::factory::{task_tag, tagged_task};
    use ferrite_core::MachineKey;
    let here = MachineKey(None);
    let on_box = MachineKey(Some("me@box".into()));
    assert_eq!(task_tag(&here, "t-1"), "ferrite-task-t-1");
    assert_eq!(task_tag(&on_box, "t-1"), "ferrite-task-t-1@me@box");
    assert_ne!(task_tag(&here, "t-1"), task_tag(&on_box, "t-1"));
    for machine in [here, on_box] {
        assert_eq!(tagged_task(&task_tag(&machine, "t-1")), Some((machine, "t-1")));
    }
    assert_eq!(tagged_task("ferrite-thread-1@me@box"), None, "a Thread's is not a Task's");
}

/// A Task's toast says the event it announced, from the alert itself,
/// and goes when the Task leaves that state, as a Thread's goes once
/// answered.
#[gpui::test]
fn a_task_toast_goes_when_the_task_leaves_the_state_it_announced(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("factory-alert-leaves", 2);
    let project = core.project_of(core.threads()[0]).unwrap();
    let needs_human = |status, result| Board {
        workflows: Vec::new(),
        tasks: vec![task(
            "t-1",
            project.project(),
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
            .update(cx, |cockpit, _| cockpit.fixture_task_alert(ferrite_core::protocol::ServerMessage::TaskAlert {
                project: project.project(),
                task: "t-1".into(),
                status: Status::NeedsHuman,
                title: "Task needs you: Task t-1".into(),
                detail: Some("No access".into()),
            }));
        cx.notify();
    };
    let before = toasts(cx);
    view.update(cx, |view, cx| {
        let watched = view.cockpit.read(cx).threads()[0];
        view.focus_thread(watched, cx);
        alert(view, cx);
    });
    tick(cx);
    ring(cx);
    assert_eq!(toasts(cx), before + 1, "it needs the operator: a toast");

    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.fixture_factory(needs_human(Status::Working, None)));
        cx.notify();
    });
    tick(cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    tick(cx);
    assert_eq!(toasts(cx), before, "answered: the toast goes");

    view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_factory(Board {
            workflows: Vec::new(),
            tasks: vec![task(
                "t-1",
                project.project(),
                Status::NeedsHuman,
                None,
                None,
                Some("Out of disk"),
            )],
            ..Default::default()
        }));
        alert(view, cx);
    });
    tick(cx);
    ring(cx);
    assert_eq!(toasts(cx), before + 1, "needed again: a toast");
    assert_eq!(
        cx.delivered_system_notifications()
            .last()
            .map(|notification| notification.body.to_string()),
        Some("No access".into()),
        "it says the event, not what the board reads later"
    );

    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.fixture_factory(needs_human(Status::Completed, Some("Created hello.txt"))));
        cx.notify();
    });
    tick(cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    tick(cx);
    assert_eq!(toasts(cx), before, "completed: the toast goes");
    view.read_with(cx, |view, _| {
        assert_eq!(view.chimed, [Sound::NeedsYou, Sound::NeedsYou])
    });
}

/// An empty Project's Factory opens from the nav: filtered to the Project,
/// the Factory menu names it and its row opens the board.
#[gpui::test]
fn an_empty_projects_factory_opens_from_the_nav(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("factory-empty-project", 1);
    let repo = repo_in(&scratch("factory-empty-project-repo"));
    let empty = core.register_project(core.first_machine(), &repo).now().unwrap();
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
    click(cx, "nav-factory");
    tick(cx);
    click(cx, "context-menu-row-0");
    tick(cx);
    assert!(shows(cx, "factory-board"));
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.factory_project(),
            Some(empty)
        );
    });
}

/// Focusing a Worker by id (the CLI's `thread focus`) lands on its Task's
/// panel with the Worker tab showing, not on a free-standing Thread.
#[gpui::test]
fn focusing_a_worker_by_id_opens_its_tasks_worker_tab(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("factory-focus-worker", 1);
    let project = core.project_of(core.threads()[0]).unwrap();
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
            project.project(),
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
            .update(cx, |cockpit, _| cockpit.fixture_task_alert(ferrite_core::protocol::ServerMessage::Focus {
                thread: worker.thread(),
                fork_of: None,
            }));
        cx.notify();
    });
    tick(cx);
    assert!(shows(cx, "task-panel"));
    assert!(shows(cx, "task-worker"), "the Worker tab");
    view.read_with(cx, |view, cx| {
        let factory = view.factory().expect("the Factory shows").read(cx);
        assert_eq!(factory.open().map(String::as_str), Some("t-1"));
        assert_eq!(factory.tab(), ferrite_factory::board::TaskTab::Worker);
    });
}

/// New thread (the nav's button) gives the board back from the Factory
/// even when the draft it focuses is the one that already stood there, as
/// in a store where no Thread was ever opened: the draft is what the
/// operator asked to see.
#[gpui::test]
fn new_thread_leaves_the_factory_for_the_draft_that_stood(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("factory-new-thread", 0);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let project = view.read_with(cx, |view, cx| {
        assert!(view.panes[view.focused(cx)].read(cx).draft().is_some(), "a draft stands");
        view.launch_project.unwrap()
    });
    view.update(cx, |view, cx| view.open_factory(project, cx));
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.factory().is_some()));
    let new = cx.debug_bounds("nav-new-thread").expect("the nav's New thread");
    cx.simulate_click(new.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(!view.destination_shown(), "the board is back");
        assert!(view.panes[view.focused(cx)].read(cx).draft().is_some(), "on the draft");
    });
}
