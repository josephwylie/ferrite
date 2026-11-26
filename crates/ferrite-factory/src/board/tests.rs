//! The Factory board drawn from a fixture, in a window of its own: its
//! head and surface tabs, the columns and their cards, the open Task's
//! panel and what each press asks of the window.

use std::cell::RefCell;
use std::rc::Rc;

use ferrite_core::factory_board::Status;
use ferrite_core::ThreadKey;
use ferrite_ui::testing::{click, mount, record, shows};
use gpui::{px, size, Bounds, TestAppContext, VisualTestContext};

use super::*;
use crate::archive::{ArchiveInputs, ArchiveView};
use crate::coordinator::CoordinatorView;
use crate::direction::DirectionView;
use crate::starts::StartsView;
use crate::workflows::WorkflowsView;

const WIDE: f32 = 1440.;
const TALL: f32 = 900.;

/// A Task as the Server would publish it, `extra` laid over it.
fn task(id: &str, status: Status, extra: serde_json::Value) -> Task {
    let mut task = serde_json::json!({
        "id": id,
        "project": 4,
        "title": format!("Task {id} with a title long enough to wrap in a column"),
        "instructions": format!("Do the work of {id}"),
        "status": status,
        "agent": {"provider": "claude", "model": "claude-opus-4"},
        "branch": format!("factory/{id}"),
        "activity": [
            {"at_ms": 1, "by": "ferrite", "text": "Started the Worker"},
            {"at_ms": 2, "by": "operator", "text": "Use the staging data"},
        ],
        "created_ms": 1,
    });
    for (key, value) in extra.as_object().unwrap() {
        task[key] = value.clone();
    }
    serde_json::from_value(task).unwrap()
}

/// One Task in each of the board's six columns.
fn every_column() -> Vec<Task> {
    vec![
        task("t-1", Status::Queued, serde_json::json!({})),
        task("t-2", Status::Working, serde_json::json!({})),
        task("t-3", Status::NeedsHuman, serde_json::json!({"reason": "No access"})),
        task("t-4", Status::Approval, serde_json::json!({"result": "checked"})),
        task("t-5", Status::Stopped, serde_json::json!({"reason": "Stopped by you"})),
        task("t-6", Status::Completed, serde_json::json!({"result": "Created hello.txt"})),
    ]
}

/// What the window keeps of a Project's Factory, as a test keeps it.
struct Kept {
    tasks: Vec<Task>,
    archived: Vec<Task>,
    /// What the open Task's Worker tab draws, as the window has it.
    worker: Option<fn() -> WorkerTab>,
    trouble: Option<String>,
    /// Each panel the board asked for: its Task, whether from the Archive,
    /// its tab and the Worker being read.
    asked: Rc<RefCell<Vec<Option<(TaskId, bool, TaskTab, Option<ThreadKey>)>>>>,
}

fn kept(tasks: Vec<Task>) -> Kept {
    Kept {
        tasks,
        archived: Vec::new(),
        worker: None,
        trouble: None,
        asked: Rc::default(),
    }
}

impl Kept {
    /// What the window answers the board with, as `factory_inputs` does.
    fn inputs(&self, query: &FactoryQuery) -> FactoryInputs {
        self.asked.borrow_mut().push(
            query
                .panel
                .as_ref()
                .map(|panel| (panel.task.clone(), panel.archived, panel.tab, panel.reading)),
        );
        let panel = query.panel.as_ref().and_then(|panel| {
            let task = self
                .tasks
                .iter()
                .chain(&self.archived)
                .find(|task| task.id == panel.task && task.archived() == panel.archived)?;
            Some(TaskPanel {
                task: task.clone(),
                worker: (panel.tab == TaskTab::Worker)
                    .then_some(self.worker)
                    .flatten()
                    .map(|worker| worker()),
            })
        });
        FactoryInputs {
            title: "ferrite".into(),
            cards: self.tasks.iter().map(TaskCard::of).collect(),
            board_w: f32::from(query.viewport.width),
            archived: self.archived.len(),
            proposals: 2,
            workflows: 1,
            trouble: self.trouble.clone(),
            panel,
        }
    }
}

/// The board in a window of its own, `WIDE` by `TALL`, reading `kept` as
/// the window's root reads its Factory; its leaves read nothing but the
/// Archive's list.
fn open(kept: Kept, cx: &mut TestAppContext) -> (Entity<FactoryView>, Entity<Kept>, &mut VisualTestContext) {
    let kept = cx.new(|_| kept);
    let inputs = Source::of(&kept, |kept: &Kept, query: &FactoryQuery, _| Some(kept.inputs(query)));
    let archived = Source::of(&kept, |kept: &Kept, _: &String, _| {
        Some(ArchiveInputs {
            found: Vec::new(),
            any: !kept.archived.is_empty(),
            open: None,
        })
    });
    let nothing = kept.clone();
    let (view, cx) = mount(cx, move |_, cx| {
        let leaves = Leaves {
            editor: cx.new(|cx| WorkflowsView::new(Source::of(&nothing, |_, _, _| None), cx)),
            direction: cx.new(|_| DirectionView::new(Source::of(&nothing, |_, _, _| None))),
            coordinator: cx.new(|cx| CoordinatorView::new(Source::of(&nothing, |_, _, _| None), cx)),
            starts: cx.new(|_| StartsView::new(Source::of(&nothing, |_, _, _| None))),
            archive: cx.new(|cx| ArchiveView::new(archived, cx)),
        };
        FactoryView::new(inputs, leaves)
    });
    cx.simulate_resize(size(px(WIDE), px(TALL)));
    cx.run_until_parked();
    (view, kept, cx)
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> Bounds<gpui::Pixels> {
    cx.update(|window, _| window.debug_bounds(selector))
        .unwrap_or_else(|| panic!("nothing drawn as {selector}"))
}

/// The words drawn where `selector` is, joined.
fn text(cx: &mut VisualTestContext, selector: &str) -> String {
    cx.update(|window, _| window.debug_text(selector))
        .map(|lines| lines.join(" "))
        .unwrap_or_default()
}

/// `inner` lies inside `outer`, give or take a rounding pixel.
fn inside(inner: Bounds<gpui::Pixels>, outer: Bounds<gpui::Pixels>) -> bool {
    inner.left() >= outer.left() - px(1.)
        && inner.right() <= outer.right() + px(1.)
        && inner.top() >= outer.top() - px(1.)
        && inner.bottom() <= outer.bottom() + px(1.)
}

/// Open `task`'s panel, as the window does when its card asks.
fn open_task(view: &Entity<FactoryView>, task: &str, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        view.open_task(task.into(), false, cx);
        cx.notify();
    });
    cx.run_until_parked();
}

const COLUMNS: [&str; 6] = ["queued", "working", "needs_human", "approval", "stopped", "completed"];

/// Each card is in its status's column with its Worker's Provider and
/// model, and its result or stop reason.
#[gpui::test]
fn the_board_lays_cards_in_their_columns(cx: &mut TestAppContext) {
    let tasks = vec![
        task("t-1", Status::Working, serde_json::json!({})),
        task("t-2", Status::NeedsHuman, serde_json::json!({"reason": "No access to staging"})),
        task("t-3", Status::Completed, serde_json::json!({"result": "Fixed it"})),
        task("t-4", Status::Stopped, serde_json::json!({"reason": "Stopped by you"})),
    ];
    let (_view, _kept, cx) = open(kept(tasks), cx);
    for column in COLUMNS {
        assert!(shows(cx, &format!("factory-column-{column}")), "{column}");
    }
    for (card, column) in [("t-1", "working"), ("t-2", "needs_human"), ("t-3", "completed"), ("t-4", "stopped")] {
        let card_at = bounds(cx, &format!("factory-card-{card}"));
        let column_at = bounds(cx, &format!("factory-column-{column}"));
        assert_eq!(card_at.origin.x, column_at.origin.x, "{card} in {column}");
        assert!(shows(cx, &format!("factory-card-{card}-agent")));
    }
    assert!(shows(cx, "factory-card-t-2-outcome"), "the reason");
    assert!(shows(cx, "factory-card-t-3-outcome"), "the result");
    assert!(shows(cx, "factory-card-t-4-outcome"), "the stop reason");
    assert!(!shows(cx, "factory-card-t-1-outcome"));
    assert_eq!(text(cx, "factory-count"), "4 tasks");
}

/// With no Task, the board says how to start one, and invents none.
#[gpui::test]
fn an_empty_board_says_how_to_start(cx: &mut TestAppContext) {
    let (_view, _kept, cx) = open(kept(Vec::new()), cx);
    assert!(shows(cx, "factory-board"));
    assert!(shows(cx, "factory-empty"));
    assert!(!shows(cx, "factory-columns"));
    assert!(!shows(cx, "factory-clear"), "nothing to clear");
    assert_eq!(text(cx, "factory-count"), "No tasks");
}

/// The head's buttons: a surface tab, Clear completed while Completed holds
/// a Task, and New task, each asked of the window; the Archive is the
/// board's own.
#[gpui::test]
fn the_heads_buttons_ask_the_window_and_the_archive_is_its_own(cx: &mut TestAppContext) {
    let (view, kept, cx) = open(kept(every_column()), cx);
    let heard = record(cx, &view, FactoryEvent::clone);
    click(cx, "factory-tab-coordinator");
    click(cx, "factory-tab-workflows");
    click(cx, "factory-tab-board");
    click(cx, "factory-clear");
    click(cx, "factory-new-task");
    assert_eq!(
        heard.take(),
        [
            FactoryEvent::Surface(Surface::Coordinator),
            FactoryEvent::Surface(Surface::Workflows),
            FactoryEvent::Surface(Surface::Board),
            FactoryEvent::Clear,
            FactoryEvent::NewTask,
        ]
    );

    kept.update(cx, |kept, cx| {
        kept.tasks.retain(|task| task.status != Status::Completed);
        kept.archived.push(task("t-9", Status::Completed, serde_json::json!({"archived_ms": 5})));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(!shows(cx, "factory-clear"), "none Completed on the board");
    let label = text(cx, "factory-archive");
    assert!(label.contains('1'), "with how many it holds: {label}");
    click(cx, "factory-archive");
    assert!(heard.take().is_empty(), "the Archive asks nothing of the window");
    assert!(shows(cx, "factory-archive-list"));
    assert!(!shows(cx, "factory-columns"), "in the Board's place");
    assert!(view.read_with(cx, |view, _| view.archive_shown()));
}

/// A card asks for its Task's panel; once open, the card pressed again or
/// Close closes it, asking nothing.
#[gpui::test]
fn a_card_asks_for_its_panel_and_pressed_again_closes_it(cx: &mut TestAppContext) {
    let (view, _kept, cx) = open(kept(every_column()), cx);
    let heard = record(cx, &view, FactoryEvent::clone);
    click(cx, "factory-card-t-2");
    assert_eq!(heard.take(), [FactoryEvent::OpenTask("t-2".into())]);
    assert!(!shows(cx, "task-panel"), "the window opens it");

    open_task(&view, "t-2", cx);
    assert!(shows(cx, "task-panel"));
    click(cx, "factory-card-t-2");
    assert!(!shows(cx, "task-panel"), "pressed again, it closes");
    open_task(&view, "t-2", cx);
    click(cx, "task-close");
    assert!(!shows(cx, "task-panel"), "Close closes it");
    assert!(heard.take().is_empty());
}

/// Stop is offered while a Task works, Resume once stopped, Delete only at
/// rest, and an archived Task is read-only; each asks for its command.
#[gpui::test]
fn the_panel_offers_stop_resume_and_delete_by_status(cx: &mut TestAppContext) {
    let (view, kept, cx) = open(kept(every_column()), cx);
    let heard = record(cx, &view, FactoryEvent::clone);
    for (id, stop, resume, delete) in [
        ("t-1", true, false, true),
        ("t-2", true, false, false),
        ("t-3", true, false, false),
        ("t-5", false, true, true),
        ("t-6", false, false, true),
    ] {
        open_task(&view, id, cx);
        assert_eq!(shows(cx, "task-stop"), stop, "{id} stop");
        assert_eq!(shows(cx, "task-resume"), resume, "{id} resume");
        assert_eq!(shows(cx, "task-delete"), delete, "{id} delete");
    }
    open_task(&view, "t-2", cx);
    click(cx, "task-stop");
    open_task(&view, "t-5", cx);
    click(cx, "task-resume");
    click(cx, "task-delete");
    assert_eq!(
        heard.take(),
        [
            FactoryEvent::Command(TaskAction::Stop { task: "t-2".into() }),
            FactoryEvent::Command(TaskAction::Resume { task: "t-5".into() }),
            FactoryEvent::Command(TaskAction::Delete { task: "t-5".into() }),
        ]
    );

    kept.update(cx, |kept, cx| {
        kept.archived.push(task("t-9", Status::Completed, serde_json::json!({"archived_ms": 5})));
        cx.notify();
    });
    view.update(cx, |view, cx| {
        view.open_task("t-9".into(), true, cx);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(shows(cx, "task-archived"), "read-only");
    assert!(!shows(cx, "task-delete") && !shows(cx, "task-stop"));
}

/// A tab asks for itself; the tab the window sets draws its content and is
/// what the board asks the window for.
#[gpui::test]
fn a_panels_tab_asks_and_draws_its_content(cx: &mut TestAppContext) {
    let (view, kept, cx) = open(kept(every_column()), cx);
    let heard = record(cx, &view, FactoryEvent::clone);
    open_task(&view, "t-2", cx);
    assert!(shows(cx, "task-overview"));
    click(cx, "task-tab-activity");
    click(cx, "task-tab-worker");
    assert_eq!(heard.take(), [FactoryEvent::Tab(TaskTab::Activity), FactoryEvent::Tab(TaskTab::Worker)]);

    view.update(cx, |view, cx| {
        view.set_tab(TaskTab::Activity);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(shows(cx, "task-activity-0") && shows(cx, "task-activity-1"));
    view.update(cx, |view, cx| {
        view.set_tab(TaskTab::Context);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(shows(cx, "task-instructions"));
    let asked = kept.read_with(cx, |kept, _| kept.asked.borrow().last().cloned().flatten());
    assert_eq!(asked, Some(("t-2".into(), false, TaskTab::Context, None)));
}

/// The Worker tab draws what the window hands it: an earlier Worker's
/// words, whose back link asks for the list; the Workers it had, each of
/// which asks to be read; or that none works on it.
#[gpui::test]
fn the_worker_tab_draws_what_the_window_hands_it(cx: &mut TestAppContext) {
    let (view, kept, cx) = open(kept(every_column()), cx);
    let heard = record(cx, &view, FactoryEvent::clone);
    open_task(&view, "t-6", cx);
    view.update(cx, |view, cx| {
        view.set_tab(TaskTab::Worker);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(shows(cx, "task-worker-none"), "none handed");

    kept.update(cx, |kept, cx| {
        kept.worker = Some(|| {
            WorkerTab::Had(vec![
                HadWorker {
                    thread: ThreadKey::fixture(1, 40),
                    step: Some("Build".into()),
                },
                HadWorker {
                    thread: ThreadKey::fixture(1, 41),
                    step: None,
                },
            ])
        });
        cx.notify();
    });
    cx.run_until_parked();
    assert!(shows(cx, "task-worker-history"));
    click(cx, "task-worker-history-1");
    assert_eq!(heard.take(), [FactoryEvent::ReadWorker(Some(ThreadKey::fixture(1, 41)))]);

    kept.update(cx, |kept, cx| {
        kept.worker = Some(|| WorkerTab::Read(String::new()));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(shows(cx, "task-worker-read"));
    let said = text(cx, "task-worker-read");
    assert!(said.contains("Nothing was said"), "an empty Thread says so: {said}");
    click(cx, "task-worker-read-back");
    assert_eq!(heard.take(), [FactoryEvent::ReadWorker(None)]);
}

/// The board asks the window for the open Task's panel only while the
/// Board shows, with the Worker the Archive is reading.
#[gpui::test]
fn the_board_asks_for_the_open_panel_only_on_the_board(cx: &mut TestAppContext) {
    let (view, kept, cx) = open(kept(every_column()), cx);
    open_task(&view, "t-4", cx);
    let read = ThreadKey::fixture(1, 7);
    view.update(cx, |view, cx| {
        view.archive().update(cx, |archive, _| archive.reading = Some((read, false)));
        cx.notify();
    });
    cx.run_until_parked();
    let last = |cx: &mut VisualTestContext| kept.read_with(cx, |kept, _| kept.asked.borrow().last().cloned().flatten());
    assert_eq!(last(cx), Some(("t-4".into(), false, TaskTab::Overview, Some(read))));
    view.update(cx, |view, cx| {
        view.set_surface(Surface::Workflows);
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(last(cx), None, "off the Board it asks for none");
    assert!(!shows(cx, "task-panel"));
}

/// At 1440 wide the six columns fit; beside the open panel the Task's
/// column comes into view, whole columns either side, and follows the Task
/// as it moves column.
#[gpui::test]
fn the_columns_fit_and_bring_the_open_tasks_column_into_view(cx: &mut TestAppContext) {
    let (view, kept, cx) = open(kept(every_column()), cx);
    let columns = bounds(cx, "factory-columns");
    for key in COLUMNS {
        let column = bounds(cx, &format!("factory-column-{key}"));
        assert!(inside(column, columns), "{key} in view, panel closed: {column:?} in {columns:?}");
    }
    for open in ["t-4", "t-6", "t-1"] {
        open_task(&view, open, cx);
        cx.run_until_parked();
        let columns = bounds(cx, "factory-columns");
        let card = bounds(cx, &format!("factory-card-{open}"));
        assert!(inside(card, columns), "{open}'s card beside its panel: {card:?} in {columns:?}");
        whole_columns(cx, open);
    }

    open_task(&view, "t-3", cx);
    kept.update(cx, |kept, cx| {
        kept.tasks[2].status = Status::Completed;
        cx.notify();
    });
    cx.run_until_parked();
    cx.run_until_parked();
    let columns = bounds(cx, "factory-columns");
    let card = bounds(cx, "factory-card-t-3");
    let completed = bounds(cx, "factory-column-completed");
    assert!(
        completed.left() <= card.left() && card.right() <= completed.right(),
        "t-3 moved to Completed: {card:?} in {completed:?}"
    );
    assert!(inside(card, columns), "t-3's card followed: {card:?} in {columns:?}");
    whole_columns(cx, "t-3");
}

/// Beside the open panel each column is in view or out of it, none half.
fn whole_columns(cx: &mut VisualTestContext, open: &str) {
    let columns = bounds(cx, "factory-columns");
    let mut in_view = 0;
    for key in COLUMNS {
        let column = bounds(cx, &format!("factory-column-{key}"));
        let out = column.right() <= columns.left() + px(1.) || column.left() >= columns.right() - px(1.);
        assert!(inside(column, columns) || out, "{key} whole or gone with {open} open: {column:?} in {columns:?}");
        in_view += usize::from(!out);
    }
    assert!(in_view >= 2, "{in_view} columns in view with {open} open");
}

/// The last command's note is said in the head until dismissed; the
/// store's trouble over the board.
#[gpui::test]
fn the_note_stays_until_dismissed_and_the_trouble_is_said(cx: &mut TestAppContext) {
    let (view, kept, cx) = open(kept(every_column()), cx);
    assert!(!shows(cx, "factory-note"));
    view.update(cx, |view, cx| {
        view.set_note(Some("Kept the worktree".into()));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(shows(cx, "factory-note"));
    click(cx, "factory-note-dismiss");
    assert!(!shows(cx, "factory-note"));
    assert!(view.read_with(cx, |view, _| view.note().is_none()));

    assert!(!shows(cx, "factory-trouble"));
    kept.update(cx, |kept, cx| {
        kept.trouble = Some("tasks.json is not valid".into());
        cx.notify();
    });
    cx.run_until_parked();
    assert!(shows(cx, "factory-trouble"));
}

/// Once the window that kept it is gone, the board draws nothing.
#[gpui::test]
fn with_the_window_gone_it_draws_nothing(cx: &mut TestAppContext) {
    let (view, kept, cx) = open(kept(every_column()), cx);
    assert!(shows(cx, "factory-board"));
    drop(kept);
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert!(!shows(cx, "factory-board"));
}

/// The columns share a board they fit at their narrowest; past that, as
/// many whole columns as fit share it, and a column comes into view at
/// either edge.
#[test]
fn whole_columns_share_a_narrow_board() {
    let wide = ColumnFit::of(1200.);
    assert_eq!(wide, ColumnFit { shown: 6, width: None }, "all six fit");
    let narrow = ColumnFit::of(700.);
    assert_eq!(narrow.shown, 3);
    let width = narrow.width.expect("they scroll");
    assert!(width >= COLUMN_MIN_W, "{width}");
    assert_eq!(narrow.offset(0), 0.);
    assert_eq!(narrow.first_showing(1, px(0.)), 0, "in view from the start");
    assert_eq!(narrow.first_showing(4, px(0.)), 2, "the last in view at the right edge");
    let scrolled = px(-narrow.offset(3));
    assert_eq!(narrow.first_showing(1, scrolled), 1, "in at the left edge");
    assert_eq!(narrow.first_showing(5, scrolled), 3, "never past the last");
}
