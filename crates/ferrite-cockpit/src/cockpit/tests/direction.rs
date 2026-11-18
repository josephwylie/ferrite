//! Where a Task needs the operator, in its panel (#222): the answer box,
//! a Worker's permission, Ready to approve, the Rework and Direct this
//! task sheets and the text typed in them, and a Workflow's approval
//! settings with what ends in Completed.
#[allow(unused_imports)]
use super::*;
use crate::cockpit::direction::{DirectSheet, TaskPick};
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
    window(cx, name, status, extra, true)
}

/// A window with the Task's panel open, its production keys bound; its
/// Task asks kept, or `recorded` false, refused for want of a Server.
fn window<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    status: Status,
    extra: serde_json::Value,
    recorded: bool,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    let (mut core, _fake) = cockpit(name, 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    if recorded {
        core.fixture_record_tasks();
    }
    bind_production_keys(cx);
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

/// A Rework or Direct this task sheet is up.
fn up(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> bool {
    view.read_with(cx, |view, cx| view.direct_sheet(cx).is_some())
}

/// What the Task's answer box holds.
fn answer(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> String {
    view.read_with(cx, |view, cx| view.answer_text("t-1", cx))
}

/// What the sheet up holds in its field: the feedback or the note.
fn typed(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> String {
    view.read_with(cx, |view, cx| {
        view.direct_sheet(cx)
            .map(|sheet| sheet.read(cx).field.read(cx).text().to_string())
            .unwrap_or_default()
    })
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
    assert_eq!(answer(&view, cx), "Off; opt in only");

    view.update(cx, |view, cx| {
        view.task_tab(crate::cockpit::factory::TaskTab::Activity, cx)
    });
    tick(cx);
    view.update(cx, |view, cx| {
        view.factory_mut().unwrap().open = None;
        cx.notify();
    });
    tick(cx);
    view.update(cx, |view, cx| view.open_task("t-1", cx));
    tick(cx);
    assert_eq!(
        answer(&view, cx),
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
        answer(&view, cx),
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
    assert!(!shows(cx, "task-why"), "the permission says why, once");
    assert!(!shows(cx, "task-question"));
    click(cx, "task-permission-open");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.factory().unwrap().tab,
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
        typed(&view, cx),
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
    assert_eq!(typed(&view, cx), "Only the docs changed");

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

/// Each sheet opens with the keyboard on its one field, and a press on
/// its other controls leaves it there.
#[gpui::test]
fn each_sheet_opens_with_the_keyboard_on_its_field(cx: &mut TestAppContext) {
    let (view, cx) = panel(
        cx,
        "direct-keyboard",
        Status::Approval,
        serde_json::json!({ "result": "Checks pass" }),
    );
    click(cx, "task-rework-open");
    assert!(shows(cx, "factory-field-focused"));
    cx.simulate_input("Tighten");
    tick(cx);
    assert_eq!(typed(&view, cx), "Tighten", "the feedback, on open");
    click(cx, "direct-cancel");

    click(cx, "task-redirect-open");
    assert!(shows(cx, "factory-field-focused"));
    cx.simulate_input("Docs");
    click(cx, "redirect-skip-s-2");
    click(cx, "redirect-then-1");
    cx.simulate_input(" only");
    tick(cx);
    assert_eq!(
        typed(&view, cx),
        "Docs only",
        "the note, on open and after a press on its choices"
    );
}

/// The answer box typed into before a sheet opened does not have the
/// keyboard back once the sheet closes.
#[gpui::test]
fn a_sheet_closed_leaves_the_answer_box_without_the_keyboard(cx: &mut TestAppContext) {
    let (view, cx) = panel(
        cx,
        "direct-answer-keyboard",
        Status::NeedsHuman,
        serde_json::json!({
            "question": {"id": 3, "text": "Which fruit?", "asker": "Review", "worker": 40},
            "asked": 3,
            "reason": "Review asks: Which fruit?",
        }),
    );
    click(cx, "task-answer");
    cx.simulate_input("Mango");
    click(cx, "task-send-back");
    assert!(shows(cx, "rework-sheet"));
    cx.simulate_input("Again");
    tick(cx);
    assert_eq!(typed(&view, cx), "Again");
    assert_eq!(answer(&view, cx), "Mango");
    click(cx, "direct-cancel");
    assert!(!shows(cx, "rework-sheet"));
    let answer_holds = cx.update(|window, cx| {
        view.read(cx)
            .answer_box("t-1")
            .is_some_and(|answer| answer.read(cx).focus_handle(cx).is_focused(window))
    });
    assert!(!answer_holds);
}

/// What each sheet was left holding stays for its next opening, apart from
/// the other's, and a send spends its own only.
#[gpui::test]
fn each_sheet_keeps_its_own_words_and_choices_until_sent(cx: &mut TestAppContext) {
    let (view, cx) = panel(
        cx,
        "direct-kept-apart",
        Status::Approval,
        serde_json::json!({ "result": "Checks pass" }),
    );
    let pick = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext, pick: TaskPick| {
        view.update(cx, |view, cx| view.apply_pick(Pick::Task(pick), cx));
        tick(cx);
    };
    click(cx, "task-rework-open");
    cx.simulate_input("Fix the docs");
    pick(&view, cx, TaskPick::ReworkTo(Destination::Step("s-2".into())));
    click(cx, "direct-cancel");
    click(cx, "task-redirect-open");
    cx.simulate_input("Docs only");
    pick(&view, cx, TaskPick::RedirectTo("s-3".into()));
    click(cx, "direct-cancel");

    click(cx, "task-rework-open");
    assert_eq!(typed(&view, cx), "Fix the docs");
    click(cx, "rework-send");
    click(cx, "task-rework-open");
    assert_eq!(typed(&view, cx), "", "spent once sent");
    cx.simulate_input("Again");
    click(cx, "rework-send");

    click(cx, "task-redirect-open");
    assert_eq!(typed(&view, cx), "Docs only", "the other sheet's stays");
    click(cx, "redirect-apply");
    assert_eq!(
        asks(&view, cx),
        [
            TaskAction::Rework {
                task: "t-1".into(),
                feedback: "Fix the docs".into(),
                to: Some(Destination::Step("s-2".into())),
            },
            TaskAction::Rework {
                task: "t-1".into(),
                feedback: "Again".into(),
                to: None,
            },
            TaskAction::Redirect {
                task: "t-1".into(),
                direction: Override {
                    to: "s-3".into(),
                    skip: Vec::new(),
                    then: ThenReturn::Routes,
                    note: "Docs only".into(),
                },
            },
        ]
    );
}

/// A send the Server refuses leaves the sheet up with what was typed, and
/// the refusal said on the panel.
#[gpui::test]
fn a_refused_send_stays_on_its_sheet(cx: &mut TestAppContext) {
    let (view, cx) = window(
        cx,
        "direct-refused",
        Status::Approval,
        serde_json::json!({ "result": "Checks pass" }),
        false,
    );
    click(cx, "task-rework-open");
    cx.simulate_input("Fix it");
    click(cx, "rework-send");
    assert!(shows(cx, "rework-sheet"), "the sheet stays up");
    assert!(!shows(cx, "rework-missing"));
    assert_eq!(typed(&view, cx), "Fix it");
    assert!(view.read_with(cx, |view, _| view.factory().unwrap().note.is_some()));
}

/// A sheet stays up as its Task moves on, and goes once its Task leaves
/// the board.
#[gpui::test]
fn a_sheet_follows_its_task_and_goes_with_it(cx: &mut TestAppContext) {
    let (view, cx) = panel(
        cx,
        "direct-task-moves",
        Status::Approval,
        serde_json::json!({ "result": "Checks pass" }),
    );
    let project = view.read_with(cx, |view, _| view.factory().unwrap().project);
    click(cx, "task-redirect-open");
    view.update(cx, |view, _| {
        view.cockpit.fixture_factory(Board {
            workflows: Vec::new(),
            tasks: vec![task(project, Status::Working, serde_json::json!({}))],
            ..Default::default()
        })
    });
    tick(cx);
    assert!(shows(cx, "redirect-sheet"), "stays up as its Task moves on");
    view.update(cx, |view, _| {
        view.cockpit.fixture_factory(Board {
            workflows: Vec::new(),
            tasks: Vec::new(),
            ..Default::default()
        })
    });
    tick(cx);
    assert!(!shows(cx, "redirect-sheet"), "goes with its Task");
    assert!(!up(&view, cx), "closed, not only undrawn");
}

/// Leaving the Factory takes the sheet, and what was typed in it.
#[gpui::test]
fn leaving_the_factory_takes_the_sheet_and_its_words(cx: &mut TestAppContext) {
    let (view, cx) = panel(
        cx,
        "direct-leave",
        Status::Approval,
        serde_json::json!({ "result": "Checks pass" }),
    );
    click(cx, "task-rework-open");
    cx.simulate_input("Gone");
    tick(cx);
    view.update(cx, |view, cx| {
        let thread = view.focused_thread().unwrap();
        view.focus_thread(thread, cx)
    });
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.factory().is_none()));
    assert!(!shows(cx, "rework-sheet"));
    assert!(!up(&view, cx), "closed, not only undrawn");
    view.update(cx, |view, cx| view.open_task("t-1", cx));
    tick(cx);
    click(cx, "task-rework-open");
    assert_eq!(typed(&view, cx), "");
}

/// A menu a sheet opened closes with the sheet.
#[gpui::test]
fn a_menu_the_sheet_opened_goes_with_it(cx: &mut TestAppContext) {
    let (view, cx) = panel(
        cx,
        "direct-menu",
        Status::Approval,
        serde_json::json!({ "result": "Checks pass" }),
    );
    click(cx, "task-rework-open");
    click(cx, "rework-to");
    assert!(shows(cx, "context-menu"), "the recipient menu");
    view.update(cx, |view, cx| view.hide_sheet(cx));
    tick(cx);
    assert!(!shows(cx, "context-menu"));
    assert!(!shows(cx, "rework-sheet"));
}

/// Escape, its close button, Cancel and a press on the veil each close
/// each sheet. A menu it opened closes before it does.
#[gpui::test]
fn escape_the_close_button_and_the_veil_each_close_each_sheet(cx: &mut TestAppContext) {
    let (view, cx) = panel(
        cx,
        "direct-close",
        Status::Approval,
        serde_json::json!({ "result": "Checks pass" }),
    );
    for sheet in [DirectSheet::Rework, DirectSheet::Redirect] {
        let open = |cx: &mut gpui::VisualTestContext| {
            view.update(cx, |view, cx| view.open_direct_sheet(sheet, cx));
            tick(cx);
            assert!(up(&view, cx), "{sheet:?}: the sheet is up");
        };
        open(cx);
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(!up(&view, cx), "{sheet:?}: escape closes it");

        for button in ["direct-close", "direct-cancel"] {
            open(cx);
            click(cx, button);
            assert!(!up(&view, cx), "{sheet:?}: {button} closes it");
        }

        open(cx);
        cx.simulate_click(gpui::point(px(20.), px(780.)), gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(!up(&view, cx), "{sheet:?}: a press on the veil closes it");
    }

    view.update(cx, |view, cx| view.open_direct_sheet(DirectSheet::Rework, cx));
    tick(cx);
    click(cx, "rework-to");
    assert!(shows(cx, "context-menu"), "the recipient menu");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!shows(cx, "context-menu"), "escape closes the menu first");
    assert!(up(&view, cx), "and leaves the sheet up");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!up(&view, cx));
}

/// A Task's sheet, Settings and the New task sheet replace one another
/// with no exit, and what was typed stays for the sheet's next opening.
#[gpui::test]
fn a_tasks_sheet_and_another_sheet_replace_one_another(cx: &mut TestAppContext) {
    let (view, cx) = panel(
        cx,
        "direct-replace",
        Status::Approval,
        serde_json::json!({ "result": "Checks pass" }),
    );
    let open = |cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| view.open_direct_sheet(DirectSheet::Rework, cx));
        tick(cx);
    };
    open(cx);
    cx.simulate_input("Kept");
    tick(cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    assert!(view.read_with(cx, |view, cx| view.settings_open(cx)), "Settings replaces the sheet");
    assert!(!up(&view, cx));
    assert!(!shows(cx, "rework-sheet"), "with no exit");

    open(cx);
    assert!(!view.read_with(cx, |view, cx| view.settings_open(cx)), "the sheet replaces Settings");
    assert!(!shows(cx, "settings-card"));
    assert_eq!(typed(&view, cx), "Kept");

    view.update(cx, |view, cx| view.open_new_task(cx));
    tick(cx);
    assert!(view.read_with(cx, |view, cx| view.new_task(cx).is_some()), "New task replaces it");
    assert!(!up(&view, cx));
    assert!(!shows(cx, "rework-sheet"));
    open(cx);
    assert!(view.read_with(cx, |view, cx| view.new_task(cx).is_none()), "and it replaces New task");
    assert!(!shows(cx, "new-task-card"));
    assert_eq!(typed(&view, cx), "Kept");
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
