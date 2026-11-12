//! The Factory at the size the operator works it (#238's visual check):
//! 1440x900 with the sidebar open, a Task's panel open. Every column and
//! control is reachable, the keyboard follows what was last clicked, and
//! what the window names and raises is what is on screen.
#[allow(unused_imports)]
use super::*;
use crate::cockpit::direction::DirectField;
use crate::cockpit::workflows::Surface;
use ferrite_core::factory::{Board, Command, Status, Task, Template};

const WIDE: f32 = 1440.;
const TALL: f32 = 900.;

fn shows(cx: &mut gpui::VisualTestContext, selector: &str) -> bool {
    debug_bounds(cx, selector.to_string()).is_some()
}

fn click(cx: &mut gpui::VisualTestContext, selector: &str) {
    let at = bounds(cx, selector.to_string());
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
}

fn wide(cx: &mut gpui::VisualTestContext) {
    cx.simulate_resize(gpui::size(px(WIDE), px(TALL)));
    tick(cx);
}

/// `inner` lies inside `outer`, give or take a rounding pixel.
fn inside(inner: gpui::Bounds<gpui::Pixels>, outer: gpui::Bounds<gpui::Pixels>) -> bool {
    inner.left() >= outer.left() - px(1.)
        && inner.right() <= outer.right() + px(1.)
        && inner.top() >= outer.top() - px(1.)
        && inner.bottom() <= outer.bottom() + px(1.)
}

fn window_bounds() -> gpui::Bounds<gpui::Pixels> {
    gpui::Bounds::new(gpui::point(px(0.), px(0.)), gpui::size(px(WIDE), px(TALL)))
}

fn task(id: &str, project: ProjectId, status: Status, extra: serde_json::Value) -> Task {
    let mut task = serde_json::json!({
        "id": id,
        "project": project,
        "title": format!("Task {id} with a title long enough to wrap in a column"),
        "instructions": format!("Do the work of {id}"),
        "status": status,
        "agent": {"provider": "claude", "model": "claude-opus-4"},
        "branch": format!("factory/{id}"),
        "activity": [{"at_ms": 1, "by": "ferrite", "text": "Started the Worker"}],
        "created_ms": 1,
    });
    for (key, value) in extra.as_object().unwrap() {
        task[key] = value.clone();
    }
    serde_json::from_value(task).unwrap()
}

/// One Task in each of the board's six columns.
fn every_column(project: ProjectId) -> Board {
    Board {
        workflows: Vec::new(),
        tasks: vec![
            task("t-1", project, Status::Queued, serde_json::json!({})),
            task("t-2", project, Status::Working, serde_json::json!({})),
            task(
                "t-3",
                project,
                Status::NeedsHuman,
                serde_json::json!({"reason": "No access"}),
            ),
            task(
                "t-4",
                project,
                Status::Approval,
                serde_json::json!({"result": "checked"}),
            ),
            task(
                "t-5",
                project,
                Status::Stopped,
                serde_json::json!({"reason": "Stopped by you"}),
            ),
            task(
                "t-6",
                project,
                Status::Completed,
                serde_json::json!({"result": "Created hello.txt"}),
            ),
        ],
        ..Default::default()
    }
}

/// V1, V2: at 1440x900 with the sidebar open the six columns fit; with a
/// Task's panel open, opening a Task brings its column into view; Clear
/// and the panel are inside the window either way.
#[gpui::test]
fn the_board_fits_and_brings_an_open_tasks_column_into_view(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("fit-board", 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_factory(every_column(project));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    wide(cx);
    view.update(cx, |view, cx| view.open_factory(project, cx));
    tick(cx);
    let board = bounds(cx, "factory-board".into());
    assert!(board.left() > px(150.), "the sidebar is open: {board:?}");
    let columns = bounds(cx, "factory-columns".into());
    for key in [
        "queued",
        "working",
        "needs_human",
        "approval",
        "stopped",
        "completed",
    ] {
        let column = bounds(cx, format!("factory-column-{key}"));
        assert!(
            inside(column, columns),
            "{key} in view, panel closed: {column:?} in {columns:?}"
        );
    }
    let clear = bounds(cx, "factory-clear".into());
    assert!(
        inside(clear, board) && inside(clear, window_bounds()),
        "{clear:?}"
    );

    for open in ["t-4", "t-6", "t-1"] {
        view.update(cx, |view, cx| view.open_task(open, cx));
        tick(cx);
        tick(cx);
        let panel = bounds(cx, "task-panel".into());
        assert!(inside(panel, window_bounds()), "the panel: {panel:?}");
        let columns = bounds(cx, "factory-columns".into());
        let card = bounds(cx, format!("factory-card-{open}"));
        assert!(
            inside(card, columns),
            "{open}'s card in view beside its panel: {card:?} in {columns:?}"
        );
        whole_columns(cx, open);
        let board = bounds(cx, "factory-board".into());
        for control in ["factory-clear", "factory-new-task"] {
            let at = bounds(cx, control.into());
            assert!(
                inside(at, board) && inside(at, window_bounds()),
                "{control} with {open} open: {at:?}"
            );
        }
    }
}

/// Beside the open panel the board shows whole columns: each is in view
/// or out of it, none half under the panel or the sidebar.
fn whole_columns(cx: &mut gpui::VisualTestContext, open: &str) {
    let columns = bounds(cx, "factory-columns".into());
    let mut in_view = 0;
    for key in [
        "queued",
        "working",
        "needs_human",
        "approval",
        "stopped",
        "completed",
    ] {
        let column = bounds(cx, format!("factory-column-{key}"));
        let out =
            column.right() <= columns.left() + px(1.) || column.left() >= columns.right() - px(1.);
        assert!(
            inside(column, columns) || out,
            "{key} is whole or gone with {open} open: {column:?} in {columns:?}"
        );
        in_view += usize::from(!out);
    }
    assert!(in_view >= 2, "{in_view} columns in view with {open} open");
}

/// V1: a Task that moves column while its panel is open is followed there:
/// one that completes while open shows in Completed beside its panel.
#[gpui::test]
fn an_open_task_that_moves_column_stays_in_view(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("fit-moves", 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_factory(every_column(project));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    wide(cx);
    view.update(cx, |view, cx| {
        view.open_factory(project, cx);
        view.open_task("t-3", cx);
    });
    tick(cx);
    tick(cx);
    let mut board = every_column(project);
    board.tasks[2].status = Status::Completed;
    view.update(cx, |view, cx| {
        view.cockpit.fixture_factory(board);
        cx.notify();
    });
    tick(cx);
    tick(cx);
    let columns = bounds(cx, "factory-columns".into());
    let card = bounds(cx, "factory-card-t-3".into());
    let completed = bounds(cx, "factory-column-completed".into());
    assert!(
        completed.left() <= card.left() && card.right() <= completed.right(),
        "t-3 moved to Completed: {card:?} in {completed:?}"
    );
    assert!(
        inside(card, columns),
        "t-3's card followed beside its panel: {card:?} in {columns:?}"
    );
    whole_columns(cx, "t-3");
}

/// V3: an empty Project gets no nav row of its own, but its Factory is one
/// palette search away.
#[gpui::test]
fn an_empty_projects_factory_is_one_palette_search_away(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("fit-palette", 1);
    let repo = repo_in(&scratch("fit-palette-repo"));
    let empty = core.register_project(&repo).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    wide(cx);
    assert!(
        !shows(cx, &format!("nav-factory-{}", empty.get())),
        "no row for every Project"
    );
    let title = view.read_with(cx, |view, _| {
        view.cockpit
            .registry()
            .project(empty)
            .unwrap()
            .title
            .clone()
    });
    view.update_in(cx, |view, window, cx| {
        view.open_palette(
            crate::palette::PaletteScope::All,
            &format!("Factory: {title}"),
            window,
            cx,
        )
    });
    tick(cx);
    let wanted = format!("Factory: {title}");
    view.update(cx, |view, cx| {
        let palette = view.floats.palette.clone().expect("the palette");
        palette.update(cx, |palette, cx| {
            let at = palette
                .survivors()
                .iter()
                .position(|row| row.name.as_ref() == wanted)
                .unwrap_or_else(|| panic!("{wanted} among the rows"));
            palette.set_cursor(at, cx);
        });
    });
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.factory_view.as_ref().map(|view| view.project),
            Some(empty)
        );
    });
    assert!(shows(cx, "factory-board"));
}

/// V4: once the answer is sent the answer box lets the keyboard go: the
/// next field the operator turns to takes what they type.
#[gpui::test]
fn an_answer_sent_lets_the_keyboard_go(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("fit-answer-focus", 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_record_tasks();
    core.fixture_factory(Board {
        workflows: Vec::new(),
        tasks: vec![task(
            "t-1",
            project,
            Status::NeedsHuman,
            serde_json::json!({
                "question": {"id": 3, "text": "Which fruit?", "asker": "Worker", "worker": 40},
                "asked": 3,
                "reason": "Which fruit?",
            }),
        )],
        ..Default::default()
    });
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    wide(cx);
    view.update(cx, |view, cx| {
        view.open_task("t-1", cx);
        view.focus_direct(DirectField::Answer, cx);
    });
    tick(cx);
    cx.simulate_input("mango");
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, cx| view.direct_text(
            "t-1",
            DirectField::Answer,
            cx
        )),
        "mango"
    );
    view.update(cx, |view, cx| view.send_answer(cx));
    tick(cx);
    let answer_holds = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            let view = view.read(cx);
            view.task_drafts("t-1")
                .is_some_and(|drafts| drafts.answer.read(cx).focus_handle(cx).is_focused(window))
        })
    };
    assert!(!answer_holds(&view, cx), "sent: the answer box lets go");

    view.update(cx, |view, cx| {
        view.factory_surface(Surface::Coordinator, cx)
    });
    tick(cx);
    cx.simulate_input("plan the week");
    tick(cx);
    let opening = view.read_with(cx, |view, cx| {
        view.factory_view
            .as_ref()
            .and_then(|view| view.opening.as_ref())
            .map(|field| field.read(cx).text().to_string())
    });
    assert_eq!(opening.as_deref(), Some("plan the week"));
}

/// V4: a field typed into on the Overview does not keep the keyboard from
/// the Worker tab the operator then opens.
#[gpui::test]
fn the_worker_tab_takes_the_keyboard_from_the_overviews_field(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("fit-worker-focus", 1);
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
            Status::NeedsHuman,
            serde_json::json!({
                "worker": worker,
                "question": {"id": 3, "text": "Which fruit?", "asker": "Worker", "worker": worker},
                "asked": 3,
                "reason": "Which fruit?",
            }),
        )],
        ..Default::default()
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    wide(cx);
    view.update(cx, |view, cx| {
        view.open_task("t-1", cx);
        view.focus_direct(DirectField::Answer, cx);
    });
    tick(cx);
    click(cx, "task-tab-worker");
    let composer_holds = cx.update(|window, cx| {
        let view = view.read(cx);
        let index = view.pane_for(worker).expect("the Worker's Pane");
        view.panes[index]
            .composer
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx)
    });
    assert!(composer_holds, "the Worker's Composer has the keyboard");
}

/// V5: a Coordinator whose Pane only the Factory draws (the roster shows
/// the operator's own Thread) draws its transcript without a click.
#[gpui::test]
fn the_coordinators_pane_draws_without_a_click(cx: &mut TestAppContext) {
    let fake = Fake::default();
    let dir = scratch("fit-coordinator-draws");
    let mut core = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));
    let first = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    let project = core.project_id(first).unwrap();
    core.fixture_live_factory();
    let coordinator = core
        .fixture_open_owned(
            Provider::Claude,
            WorkspaceChoice::Main { checkout: here() },
            ferrite_core::ThreadOwner::Factory(project),
        )
        .unwrap();
    core.fixture_factory_command(Command::CoordinatorStarted {
        project,
        thread: coordinator,
    });
    // The operator's own Thread is the one on screen from the first frame.
    let mine = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    did_something_now(&dir, mine);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    wide(cx);
    view.update(cx, |view, cx| {
        view.focus_thread(mine, cx);
        view.open_factory(project, cx);
        // The surface already shows when the Coordinator's Pane arrives:
        // nothing moved the roster to it.
        view.factory_view.as_mut().unwrap().surface = Surface::Coordinator;
        cx.notify();
    });
    tick(cx);
    tick(cx);
    view.read_with(cx, |view, _| {
        let index = view.pane_for(coordinator).expect("its Pane");
        assert!(
            !view.visible_indices().contains(&index),
            "the roster does not show it: only the Factory draws it"
        );
    });
    assert!(shows(cx, "coordinator-pane"));
    assert!(
        shows(cx, &format!("pane-body-{}", coordinator.get())),
        "its transcript draws without a click"
    );
}

/// V7: a Task's toast is not raised over its own open panel; its chime
/// still rings, and another Task's toast still shows.
#[gpui::test]
fn a_tasks_toast_is_not_raised_over_its_own_panel(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("fit-toast", 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    let mut board = every_column(project);
    board
        .tasks
        .retain(|task| task.id == "t-3" || task.id == "t-4");
    core.fixture_factory(board);
    cx.update(|cx| cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite"));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    wide(cx);
    let toasts = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            use gpui::component::WindowExt as _;
            window.notifications(cx).len()
        })
    };
    view.update(cx, |view, cx| view.open_task("t-4", cx));
    tick(cx);
    let before = toasts(cx);
    let alert = |task: &str, status: Status| ferrite_core::protocol::ServerMessage::TaskAlert {
        project,
        task: task.into(),
        status,
        title: format!("Task {task}"),
        detail: None,
    };
    view.update(cx, |view, cx| {
        view.cockpit
            .fixture_task_alert(alert("t-4", Status::Approval));
        cx.notify();
    });
    tick(cx);
    ring(cx);
    assert_eq!(toasts(cx), before, "no toast over its own panel");
    view.read_with(cx, |view, _| assert_eq!(view.chimed, [Sound::NeedsYou]));

    view.update(cx, |view, cx| {
        view.cockpit
            .fixture_task_alert(alert("t-3", Status::NeedsHuman));
        cx.notify();
    });
    tick(cx);
    ring(cx);
    assert_eq!(toasts(cx), before + 1, "another Task's toast shows");
    let card = bounds(cx, "notification-card".into());
    let panel = bounds(cx, "task-panel".into());
    assert!(
        card.right() <= panel.left() + px(1.),
        "beside the panel, its Close in reach: {card:?} {panel:?}"
    );
    assert!(card.size.height <= px(80.), "a title and a line: {card:?}");
    click(cx, "task-close");
    assert!(!shows(cx, "task-panel"), "Close takes with the toast up");
    cx.executor().advance_clock(Duration::from_secs(6));
    tick(cx);
    assert!(!shows(cx, "notification-card"), "on its board it passes by");

    view.update(cx, |view, cx| {
        view.factory_surface(Surface::Workflows, cx);
        view.cockpit
            .fixture_task_alert(alert("t-3", Status::NeedsHuman));
        cx.notify();
    });
    tick(cx);
    ring(cx);
    cx.executor().advance_clock(Duration::from_secs(6));
    tick(cx);
    assert!(
        shows(cx, "notification-card"),
        "off the board, it stands until seen"
    );
}

/// V8: while the Factory shows, the title bar names it (and the open
/// Task), never the Thread behind it.
#[gpui::test]
fn the_title_bar_names_the_factory_and_its_open_task(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("fit-title", 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_factory(every_column(project));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    wide(cx);
    assert!(shows(cx, "view-title"), "a Thread's title before");
    view.update(cx, |view, cx| view.open_factory(project, cx));
    tick(cx);
    assert!(!shows(cx, "view-title"), "not the Thread behind the board");
    assert!(!shows(cx, "title-tools"), "nor its Panel's toggle");
    assert!(shows(cx, "view-head-factory"));
    assert!(!shows(cx, "view-head-task"));
    view.update(cx, |view, cx| view.open_task("t-2", cx));
    tick(cx);
    assert!(shows(cx, "view-head-task"), "the open Task");
    view.update(cx, |view, cx| view.factory_surface(Surface::Workflows, cx));
    tick(cx);
    assert!(!shows(cx, "view-head-task"), "the panel is not on screen");
    assert!(shows(cx, "view-head-factory"));

    view.update(cx, |view, cx| view.open_task("t-6", cx));
    tick(cx);
    assert!(shows(cx, "view-head-task"), "the Completed Task");
    let mut cleared = every_column(project);
    cleared.tasks[5] = task(
        "t-6",
        project,
        Status::Completed,
        serde_json::json!({"result": "Created hello.txt", "archived_ms": 2_000}),
    );
    view.update(cx, |view, cx| {
        view.cockpit.fixture_factory(cleared);
        cx.notify();
    });
    tick(cx);
    assert!(
        !shows(cx, "task-panel"),
        "cleared to the Archive: off the board"
    );
    assert!(!shows(cx, "view-head-task"), "and out of the title bar");
}

/// V9, O7, O6: an outside click closes the New task sheet and the menu it
/// opened, which hangs under its trigger; what was typed is there when it
/// reopens.
#[gpui::test]
fn the_new_task_sheet_closes_with_its_menu_and_keeps_its_words(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("fit-sheet", 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_factory(every_column(project));
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    wide(cx);
    view.update(cx, |view, cx| {
        view.open_factory(project, cx);
        view.open_new_task(cx);
    });
    tick(cx);
    cx.simulate_input("Fix the login flake");
    tick(cx);
    click(cx, "new-task-process");
    assert!(shows(cx, "context-menu"), "the Process menu");
    let trigger = bounds(cx, "new-task-process".into());
    let menu = bounds(cx, "context-menu".into());
    assert!(
        menu.top() >= trigger.bottom() || menu.bottom() <= trigger.top(),
        "the menu leaves its trigger in view: {menu:?} {trigger:?}"
    );
    cx.simulate_click(
        gpui::point(px(WIDE - 8.), px(TALL - 8.)),
        gpui::Modifiers::none(),
    );
    tick(cx);
    assert!(!shows(cx, "new-task-card"), "the sheet closed");
    assert!(!shows(cx, "context-menu"), "and its menu with it");
    view.update(cx, |view, cx| view.open_new_task(cx));
    tick(cx);
    let typed = view.read_with(cx, |view, cx| {
        view.factory_view
            .as_ref()
            .and_then(|view| view.sheet.as_ref())
            .map(|sheet| sheet.instructions.read(cx).text().to_string())
    });
    assert_eq!(typed.as_deref(), Some("Fix the login flake"));
}

/// V10: focusing a Worker by id from the Workflows surface brings its
/// Task's panel forward on the Board.
#[gpui::test]
fn focusing_a_worker_by_id_brings_its_panel_forward_from_workflows(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("fit-focus-worker", 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    let worker = core
        .fixture_open_owned(
            Provider::Claude,
            WorkspaceChoice::Main { checkout: here() },
            ferrite_core::ThreadOwner::Task("t-2".into()),
        )
        .unwrap();
    core.fixture_factory(Board {
        workflows: Vec::new(),
        tasks: vec![task(
            "t-2",
            project,
            Status::Working,
            serde_json::json!({"worker": worker}),
        )],
        ..Default::default()
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    wide(cx);
    view.update(cx, |view, cx| {
        view.open_factory(project, cx);
        view.factory_surface(Surface::Workflows, cx);
    });
    tick(cx);
    view.update(cx, |view, cx| {
        view.cockpit
            .fixture_task_alert(ferrite_core::protocol::ServerMessage::Focus { thread: worker });
        cx.notify();
    });
    tick(cx);
    assert!(shows(cx, "task-panel"), "the panel is on screen");
    assert!(shows(cx, "task-worker"));
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.factory_view.as_ref().map(|view| view.surface),
            Some(Surface::Board)
        );
    });
}

/// A window on a live fixture Factory, on the Issue to reviewed PR
/// template's Workflow.
fn template_window<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    models: Vec<ferrite_core::ModelInfo>,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    let fake = Fake::default();
    let (announce, announced) = std::sync::mpsc::channel();
    *fake.model_discovery.borrow_mut() = Some(announced);
    let dir = scratch(name);
    let mut core = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));
    let thread = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    did_something_now(&dir, thread);
    let project = core.project_id(thread).unwrap();
    core.fixture_live_factory();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    wide(cx);
    view.update(cx, |view, cx| {
        view.open_factory(project, cx);
        view.factory_surface(Surface::Workflows, cx);
        view.create_workflow(Template::IssueToReviewedPr, cx);
    });
    announce.send((Provider::Claude, models)).unwrap();
    tick(cx);
    (view, cx)
}

/// V11: the start's settings sit right under their picker; the outline's
/// Approval row and + Add agent, and the start's Turn on, can be reached.
#[gpui::test]
fn start_settings_have_no_gap_and_their_controls_are_reachable(cx: &mut TestAppContext) {
    let (_view, cx) = template_window(cx, "fit-start", Vec::new());
    click(cx, "outline-start");
    let kind = bounds(cx, "start-kind".into());
    let note = bounds(cx, "start-kind-note".into());
    assert!(
        note.top() - kind.bottom() <= px(ferrite_ui::theme::SPACE_3 + 1.),
        "the note under its picker: {kind:?} {note:?}"
    );
    click(cx, "start-kind");
    click(cx, "context-menu-row-1");
    for (area, control) in [
        ("inspector-start", "automation-turn"),
        ("outline", "outline-add"),
    ] {
        reveal(cx, area, control);
        let at = bounds(cx, control.into());
        assert!(
            inside(at, bounds(cx, area.into())) && inside(at, window_bounds()),
            "{control} reachable: {at:?}"
        );
    }
    let approval = [
        "outline-approval",
        "outline-approval-flagged",
        "outline-approval-selected",
        "outline-approval-selected-flagged",
    ]
    .into_iter()
    .find(|selector| shows(cx, selector))
    .expect("the Approval row");
    reveal(cx, "outline", approval);
    assert!(inside(bounds(cx, approval.into()), window_bounds()));
}

/// V12: a Step's instructions are a text area, taller than a one-line
/// field, that grows with what is typed.
#[gpui::test]
fn step_instructions_are_a_text_area_that_grows(cx: &mut TestAppContext) {
    let (_view, cx) = template_window(cx, "fit-instructions", Vec::new());
    let name = bounds(cx, "inspector-name".into());
    let empty = bounds(cx, "inspector-instructions".into());
    assert!(
        empty.size.height > name.size.height + px(ferrite_ui::theme::LH_UI),
        "an area, not a line: {empty:?} vs {name:?}"
    );
    click(cx, "inspector-instructions");
    for _ in 0..6 {
        cx.simulate_input(
            "Use the triage skill on the incoming issue and ask the owner about anything missing. ",
        );
    }
    tick(cx);
    let grown = bounds(cx, "inspector-instructions".into());
    assert!(
        grown.size.height > empty.size.height,
        "it grows: {grown:?} from {empty:?}"
    );
}

/// O6: one Default in the model menu; no Move up on the first row nor Move
/// down on the last; a route's menu hangs under its trigger; ↓ keeps the
/// chosen row in view.
#[gpui::test]
fn the_workflow_menus_offer_what_applies_and_keep_the_selection_in_view(cx: &mut TestAppContext) {
    let model = |value: &str, display: &str| ferrite_core::ModelInfo {
        value: value.into(),
        display: display.into(),
        detail: String::new(),
        resolved: None,
        efforts: vec!["high".into()],
        default_effort: None,
    };
    let (view, cx) = template_window(
        cx,
        "fit-menus",
        vec![
            model("default", "Default \u{b7} Opus 5.5"),
            model("claude-opus-4", "Opus 4"),
        ],
    );
    let labels = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.factory_pick_rows()
                .iter()
                .map(|item| item.label.to_string())
                .collect::<Vec<_>>()
        })
    };
    click(cx, "inspector-model");
    let models = labels(&view, cx);
    assert_eq!(
        models
            .iter()
            .filter(|label| label.starts_with("Default"))
            .count(),
        1,
        "{models:?}"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);

    let last = view.read_with(cx, |view, _| {
        view.shown_workflow()
            .unwrap()
            .draft
            .steps
            .last()
            .unwrap()
            .id
            .clone()
    });
    click(cx, "outline-row-s-1-more");
    assert!(!labels(&view, cx).contains(&"Move up".to_string()));
    assert!(labels(&view, cx).contains(&"Move down".to_string()));
    cx.simulate_keystrokes("escape");
    tick(cx);
    reveal(cx, "outline", &format!("outline-row-{last}-more"));
    click(cx, &format!("outline-row-{last}-more"));
    assert!(!labels(&view, cx).contains(&"Move down".to_string()));
    assert!(labels(&view, cx).contains(&"Move up".to_string()));
    cx.simulate_keystrokes("escape");
    tick(cx);

    reveal(cx, "outline", "outline-route-s-1-finished");
    let trigger = bounds(cx, "outline-route-s-1-finished".into());
    click(cx, "outline-route-s-1-finished");
    let menu = bounds(cx, "context-menu".into());
    assert!(
        menu.top() >= trigger.bottom() || menu.bottom() <= trigger.top(),
        "the route menu leaves its trigger in view: {menu:?} {trigger:?}"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);

    click(cx, "outline-row-s-1-selected");
    for _ in 0..5 {
        cx.simulate_keystrokes("down");
        tick(cx);
    }
    let chosen = format!("outline-row-{last}-selected");
    tick(cx);
    let row = bounds(cx, chosen.clone());
    let outline = bounds(cx, "outline".into());
    assert!(
        inside(row, outline) && inside(row, window_bounds()),
        "the chosen row in view: {row:?} in {outline:?}"
    );
}

/// O5: the Coordinator's settings fit beside each other in the window.
#[gpui::test]
fn the_coordinators_settings_fit_the_window(cx: &mut TestAppContext) {
    let fake = Fake::default();
    let dir = scratch("fit-coordinator-settings");
    let mut core = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));
    let mine = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    did_something_now(&dir, mine);
    let project = core.project_id(mine).unwrap();
    core.fixture_live_factory();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    wide(cx);
    view.update(cx, |view, cx| {
        view.open_factory(project, cx);
        view.factory_surface(Surface::Coordinator, cx);
    });
    tick(cx);
    let board = bounds(cx, "factory-board".into());
    for chooser in [
        "coordinator-provider",
        "coordinator-model",
        "coordinator-effort",
    ] {
        let at = bounds(cx, chooser.into());
        assert!(inside(at, board), "{chooser}: {at:?} in {board:?}");
    }
}

/// #238 round 2: the New task sheet's Process menu opens over the sheet,
/// its row takes a press, and Start starts the Task on the Workflow picked.
#[gpui::test]
fn the_process_menu_opens_over_the_sheet_and_its_pick_starts_the_task(cx: &mut TestAppContext) {
    let opus = ferrite_core::ModelInfo {
        value: "claude-opus-4".into(),
        display: "Opus 4".into(),
        detail: String::new(),
        resolved: None,
        efforts: vec!["high".into()],
        default_effort: None,
    };
    let (view, cx) = template_window(cx, "fit-process", vec![opus]);
    view.update(cx, |view, cx| {
        view.cockpit.fixture_record_tasks();
        view.save_workflow(cx);
    });
    tick(cx);
    let workflow = view.read_with(cx, |view, _| {
        let shown = view.shown_workflow().unwrap();
        assert!(!shown.versions.is_empty(), "saved");
        shown.id.clone()
    });
    view.update(cx, |view, cx| {
        view.factory_surface(Surface::Board, cx);
        view.open_new_task(cx);
    });
    tick(cx);
    cx.simulate_input("Fix the login flake");
    tick(cx);
    click(cx, "new-task-process");
    let sheet = bounds(cx, "new-task-card".into());
    let row = bounds(cx, "context-menu-row-1".into());
    assert!(
        sheet.intersects(&row),
        "the menu hangs over the sheet: {row:?} {sheet:?}"
    );
    click(cx, "context-menu-row-1");
    assert!(!shows(cx, "context-menu"), "the pick closes the menu");
    assert!(shows(cx, "new-task-card"), "and leaves the sheet up");
    click(cx, "new-task-start");
    let asks = view.read_with(cx, |view, _| view.cockpit.fixture_task_asks().to_vec());
    match asks.as_slice() {
        [ferrite_core::protocol::TaskAction::Start {
            instructions,
            workflow: Some(on),
            ..
        }] => {
            assert_eq!(instructions, "Fix the login flake");
            assert_eq!(*on, workflow);
        }
        other => panic!("one start, on the Workflow picked: {other:?}"),
    }
}

/// #238 round 2: a menu hung under its trigger opens down, over nothing
/// above it, and fits the window however many rows it has.
#[gpui::test]
fn a_long_model_menu_opens_down_and_fits(cx: &mut TestAppContext) {
    let models = (0..30)
        .map(|at| ferrite_core::ModelInfo {
            value: format!("model-{at}"),
            display: format!("Model {at}"),
            detail: String::new(),
            resolved: None,
            efforts: vec!["high".into()],
            default_effort: None,
        })
        .collect();
    let (_view, cx) = template_window(cx, "fit-long-menu", models);
    let trigger = bounds(cx, "inspector-model".into());
    click(cx, "inspector-model");
    let menu = bounds(cx, "context-menu".into());
    assert!(
        menu.top() >= trigger.bottom(),
        "under its trigger: {menu:?} {trigger:?}"
    );
    assert!(inside(menu, window_bounds()), "in the window: {menu:?}");
}

/// #238 round 2: the Archive is left by the Board tab, as by its own
/// Board button.
#[gpui::test]
fn the_board_tab_leaves_the_archive(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("fit-archive-tab", 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_factory(every_column(project));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    wide(cx);
    view.update(cx, |view, cx| view.open_factory(project, cx));
    tick(cx);
    click(cx, "factory-archive");
    assert!(shows(cx, "factory-archive-list"));
    click(cx, "factory-tab-board");
    assert!(!shows(cx, "factory-archive-list"), "the Archive closed");
    assert!(shows(cx, "factory-columns"), "on the board");
}
