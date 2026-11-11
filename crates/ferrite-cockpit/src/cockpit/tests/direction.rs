//! Where a Task needs the operator, in its panel (#222): the answer box,
//! a Worker's permission, Ready to approve, the Rework and Direct this
//! task sheets and the text typed in them, and a Workflow's approval
//! settings with what ends in Completed.
#[allow(unused_imports)]
use super::*;
use crate::cockpit::direction::{DirectField, TaskPick};
use crate::cockpit::workflows::{Field, Pick};
use ferrite_core::factory::{Board, Destination, Override, Status, Task, ThenReturn};
use ferrite_core::protocol::TaskAction;

fn shows(cx: &mut gpui::VisualTestContext, selector: &str) -> bool {
    debug_bounds(cx, selector.to_string()).is_some()
}

fn click(cx: &mut gpui::VisualTestContext, selector: &str) {
    let at = bounds(cx, selector.to_string());
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
}

/// A Workflow Task of three Steps as the Server would publish it, with
/// `extra` laid over it.
fn task(project: ProjectId, status: Status, extra: serde_json::Value) -> Task {
    let step = |id: &str, name: &str, finished: &str| {
        serde_json::json!({
            "id": id, "name": name, "instructions": format!("{name} it"),
            "provider": "claude", "on_finished": finished, "on_unable": "human",
        })
    };
    let mut task = serde_json::json!({
        "id": "t-1",
        "project": project,
        "title": "Add a --fast flag",
        "instructions": "Add a --fast flag",
        "status": status,
        "agent": {"provider": "claude", "model": "claude-opus-4"},
        "branch": "factory/t-1",
        "activity": [{"at_ms": 1, "by": "ferrite", "text": "Started Build"}],
        "created_ms": 1,
        "workflow": {
            "id": "w-1",
            "version": 2,
            "definition": {
                "name": "Ship it",
                "steps": [
                    step("s-1", "Build", "next"),
                    step("s-2", "Review", "next"),
                    step("s-3", "Check", "approval"),
                ],
                "approval": {"after": "complete", "rework_to": "step:s-1"},
            },
        },
        "step": "s-2",
    });
    for (key, value) in extra.as_object().unwrap() {
        task[key] = value.clone();
    }
    serde_json::from_value(task).unwrap()
}

/// A window whose Task asks are kept, with the Task's panel open.
fn panel<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    status: Status,
    extra: serde_json::Value,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    let (mut core, _fake) = cockpit(name, 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_record_tasks();
    core.fixture_factory(Board {
        workflows: Vec::new(),
        tasks: vec![task(project, status, extra)],
        ..Default::default()
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    view.update(cx, |view, cx| view.open_task("t-1", cx));
    tick(cx);
    (view, cx)
}

fn asks(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<TaskAction> {
    view.read_with(cx, |view, _| view.cockpit.fixture_task_asks().to_vec())
}

fn text(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
    field: DirectField,
) -> String {
    view.read_with(cx, |view, cx| view.direct_text("t-1", field, cx))
}

/// AC: a question is answered in the Task: the asker and the question
/// shown, the answer typed survives a tab switch and closing the panel,
/// and Send answer asks the Server once with the question's id, then
/// clears.
#[gpui::test]
fn the_answer_box_answers_the_question_and_keeps_what_was_typed(cx: &mut TestAppContext) {
    let (view, cx) = panel(
        cx,
        "direct-answer",
        Status::NeedsHuman,
        serde_json::json!({
            "question": {"id": 3, "text": "Should --fast default to on?", "asker": "Review", "worker": 40},
            "asked": 3,
            "reason": "Review asks: Should --fast default to on?",
        }),
    );
    assert!(shows(cx, "task-question"));
    assert!(shows(cx, "task-question-text"));
    click(cx, "task-answer");
    cx.simulate_input("Off; opt in only");
    tick(cx);
    assert_eq!(text(&view, cx, DirectField::Answer), "Off; opt in only");

    view.update(cx, |view, cx| {
        view.task_tab(crate::cockpit::factory::TaskTab::Activity, cx)
    });
    tick(cx);
    view.update(cx, |view, cx| {
        view.factory_view.as_mut().unwrap().open = None;
        cx.notify();
    });
    tick(cx);
    view.update(cx, |view, cx| view.open_task("t-1", cx));
    tick(cx);
    assert_eq!(
        text(&view, cx, DirectField::Answer),
        "Off; opt in only",
        "survives a tab switch and the panel closing"
    );

    click(cx, "task-answer-send");
    assert_eq!(
        asks(&view, cx),
        [TaskAction::Answer {
            task: "t-1".into(),
            text: "Off; opt in only".into(),
            question: Some(3),
        }]
    );
    assert_eq!(
        text(&view, cx, DirectField::Answer),
        "",
        "cleared once taken"
    );
}

/// AC: a Worker's Provider Decision is the Task's: it says what is asked
/// and leads to the Worker tab, where the Worker's Pane answers it.
#[gpui::test]
fn a_workers_permission_is_answered_from_the_task(cx: &mut TestAppContext) {
    let (view, cx) = panel(
        cx,
        "direct-permission",
        Status::NeedsHuman,
        serde_json::json!({
            "permission": "Run cargo test",
            "reason": "Review asks permission: Run cargo test",
        }),
    );
    assert!(shows(cx, "task-permission"));
    assert!(!shows(cx, "task-question"));
    click(cx, "task-permission-open");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.factory_view.as_ref().unwrap().tab,
            crate::cockpit::factory::TaskTab::Worker
        );
    });
}

/// AC: Ready to approve shows the result, its evidence and what follows
/// approval; Approve asks the Server.
#[gpui::test]
fn ready_to_approve_shows_what_is_approved_and_approves(cx: &mut TestAppContext) {
    let (view, cx) = panel(
        cx,
        "direct-approve",
        Status::Approval,
        serde_json::json!({
            "result": "Checks pass",
            "results": [
                {"step": "s-1", "name": "Build", "outcome": {"finished": {"summary": "Added --fast"}}},
                {"step": "s-3", "name": "Check", "outcome": {"finished": {"summary": "Checks pass"}}},
            ],
        }),
    );
    assert!(shows(cx, "task-approval"));
    assert!(shows(cx, "task-approval-result"));
    assert!(shows(cx, "task-evidence-0") && shows(cx, "task-evidence-1"));
    assert!(shows(cx, "task-approval-after"));
    click(cx, "task-approve");
    assert_eq!(
        asks(&view, cx),
        [TaskAction::Approve { task: "t-1".into() }]
    );
}

/// AC: the Rework sheet needs feedback, offers the Workflow's recipient
/// unless another is chosen, and keeps what was typed when closed and
/// reopened; sending asks the Server and closes it.
#[gpui::test]
fn the_rework_sheet_needs_feedback_and_keeps_it(cx: &mut TestAppContext) {
    let (view, cx) = panel(
        cx,
        "direct-rework",
        Status::Approval,
        serde_json::json!({ "result": "Checks pass" }),
    );
    click(cx, "task-rework-open");
    assert!(shows(cx, "rework-sheet"));
    assert!(shows(cx, "rework-to"));
    click(cx, "rework-send");
    assert!(shows(cx, "rework-missing"), "feedback is needed");
    assert!(asks(&view, cx).is_empty());

    click(cx, "rework-feedback");
    cx.simulate_input("Mention --fast in the README");
    tick(cx);
    click(cx, "direct-cancel");
    assert!(!shows(cx, "rework-sheet"));
    click(cx, "task-rework-open");
    assert_eq!(
        text(&view, cx, DirectField::Feedback),
        "Mention --fast in the README",
        "survives the sheet closing"
    );
    assert!(!shows(cx, "rework-missing"));

    view.update(cx, |view, cx| {
        view.apply_pick(
            Pick::Task(TaskPick::ReworkTo(Destination::Step("s-2".into()))),
            cx,
        )
    });
    tick(cx);
    click(cx, "rework-send");
    assert_eq!(
        asks(&view, cx),
        [TaskAction::Rework {
            task: "t-1".into(),
            feedback: "Mention --fast in the README".into(),
            to: Some(Destination::Step("s-2".into())),
        }]
    );
    assert!(!shows(cx, "rework-sheet"));
}

/// AC: the Direct this task sheet needs a Step, skips Steps, returns the
/// Task to the operator when asked, keeps its choices and note when
/// closed and reopened, and applies to this Task only.
#[gpui::test]
fn the_redirect_sheet_sends_one_task_and_keeps_its_choices(cx: &mut TestAppContext) {
    let (view, cx) = panel(
        cx,
        "direct-redirect",
        Status::Working,
        serde_json::json!({}),
    );
    click(cx, "task-redirect-open");
    assert!(shows(cx, "redirect-sheet"));
    click(cx, "redirect-apply");
    assert!(shows(cx, "redirect-missing"), "a Step is needed");
    assert!(asks(&view, cx).is_empty());

    view.update(cx, |view, cx| {
        view.apply_pick(Pick::Task(TaskPick::RedirectTo("s-3".into())), cx)
    });
    tick(cx);
    assert!(!shows(cx, "redirect-skip-s-3"), "not the one it goes to");
    click(cx, "redirect-skip-s-2");
    assert!(shows(cx, "redirect-skip-s-2-on"));
    click(cx, "redirect-then-1");
    assert!(shows(cx, "redirect-then-1-on"));
    click(cx, "redirect-note");
    cx.simulate_input("Only the docs changed");
    tick(cx);

    click(cx, "direct-cancel");
    click(cx, "task-redirect-open");
    assert!(shows(cx, "redirect-skip-s-2-on"), "choices kept");
    assert!(shows(cx, "redirect-then-1-on"));
    assert_eq!(text(&view, cx, DirectField::Note), "Only the docs changed");

    click(cx, "redirect-apply");
    assert_eq!(
        asks(&view, cx),
        [TaskAction::Redirect {
            task: "t-1".into(),
            direction: Override {
                to: "s-3".into(),
                skip: vec!["s-2".into()],
                then: ThenReturn::Me,
                note: "Only the docs changed".into(),
            },
        }]
    );
    assert!(!shows(cx, "redirect-sheet"));
}

/// AC: a Workflow's approval settings choose who approves; agent approval
/// is flagged until it has a deciding Step and instructions, typed into
/// the draft; the read-only list says what ends in Completed.
#[gpui::test]
fn approval_settings_choose_who_approves_and_list_what_completes(cx: &mut TestAppContext) {
    let (core_fake, dir) = (Fake::default(), scratch("direct-settings"));
    let mut core = Cockpit::new(Store::open(&dir).unwrap(), Box::new(core_fake.clone()));
    let thread = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    did_something_now(&dir, thread);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_live_factory();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    view.update(cx, |view, cx| view.open_factory(project, cx));
    tick(cx);
    click(cx, "factory-tab-workflows");
    click(cx, "workflow-new");
    click(cx, "context-menu-row-1");

    reveal(cx, "outline", "outline-approval");
    click(cx, "outline-approval");
    assert!(shows(cx, "approval-settings"));
    assert!(shows(cx, "approval-by-0-on"), "you approve by default");
    assert!(shows(cx, "completion-path-0"));
    view.read_with(cx, |view, _| {
        let draft = &view.shown_workflow().unwrap().draft;
        assert_eq!(
            draft.completion_paths(),
            ["Merge agent reports merge confirmed"]
        );
    });

    click(cx, "approval-by-1");
    assert!(shows(cx, "approval-by-1-on"));
    assert!(shows(cx, "approval-decider-flagged"));
    assert!(shows(cx, "approval-problem-0") && shows(cx, "approval-problem-1"));
    assert!(shows(cx, "outline-approval-selected-flagged"));

    view.update(cx, |view, cx| {
        view.apply_pick(Pick::Decider("s-4".into()), cx)
    });
    tick(cx);
    let workflow = view.read_with(cx, |view, _| view.shown_workflow().unwrap().id.clone());
    click(cx, "approval-instructions");
    cx.simulate_input("Approve when a test covers it");
    tick(cx);
    view.read_with(cx, |view, cx| {
        let approval = &view.shown_workflow().unwrap().draft.approval;
        assert_eq!(approval.step.as_deref(), Some("s-4"));
        assert_eq!(approval.instructions, "Approve when a test covers it");
        assert_eq!(
            view.field_text(&Field::ApprovalInstructions(workflow.clone()), cx)
                .as_deref(),
            Some("Approve when a test covers it")
        );
    });
    assert!(!shows(cx, "approval-problem-0"), "no problems left");
    view.update(cx, |view, cx| view.select_step("s-1".into(), cx));
    tick(cx);
    assert!(!shows(cx, "approval-settings"), "a Step shows again");
}
