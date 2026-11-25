//! Where a Task needs the operator, in its panel (#222): the question and
//! its answer box, a Worker's Provider Decision (answered in the Worker
//! tab, never the Inbox), Ready to approve with what is approved, and the
//! buttons that open the Rework and Direct this task sheets (`direct`).
//!
//! What the operator types is kept per Task for as long as the Factory
//! view is open, so it survives closing a sheet, the panel, a tab switch
//! and a new board; it is cleared only once the Server took it.
//!
//! That state lives in a view of its own (ADR 0041), `DirectionView`: each
//! Task's answer box, whether the open one holds the keyboard, and each
//! Task's sheets as last closed. The Factory holds it. Its render is the
//! open Task's say in its Overview, reading the open Task through a
//! `Source` the window makes over itself; what a press asks it says as a
//! `DirectionEvent`. The sends are Server calls, so they stay the
//! window's, as does opening a sheet, which the sheet host shows.

use std::collections::HashMap;

use ferrite_core::factory_board::{self, Approver, Destination, Status, Task, TaskId, ThenReturn};
use ferrite_pane::composer::{Composer, Role};
use ferrite_ui::source::Source;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, AnyElement, ClickEvent, Context, Div, Entity, EventEmitter, Focusable, MouseButton,
    MouseDownEvent, SharedString, Window,
};

use crate::direct::{meta, read, DirectSheet, KeptDirection};

/// A Factory's say state: what is being written to its Tasks.
pub struct DirectionView {
    /// The open Task, as the window has it now.
    inputs: Source<Task>,
    /// Each Task's answer box, made once.
    answers: HashMap<TaskId, Entity<Composer>>,
    /// The open Task's answer box holds the keyboard.
    answering: bool,
    /// Each Task's Rework and Direct this task sheets as last closed.
    kept: HashMap<TaskId, KeptDirection>,
}

/// What the open Task's say asks of the window.
pub enum DirectionEvent {
    /// A press on the answer box: it holds the keyboard now.
    Answering,
    /// Send answer.
    SendAnswer,
    /// Approve.
    Approve,
    /// Request rework, Send back with feedback, or Direct this task.
    Open(DirectSheet),
    /// Answer in the Worker tab.
    WorkerTab,
}

impl EventEmitter<DirectionEvent> for DirectionView {}

impl DirectionView {
    pub fn new(inputs: Source<Task>) -> Self {
        Self {
            inputs,
            answers: HashMap::new(),
            answering: false,
            kept: HashMap::new(),
        }
    }

    /// `task`'s answer box, made once.
    pub fn ensure(&mut self, task: &str, cx: &mut Context<Self>) {
        if self.answers.contains_key(task) {
            return;
        }
        let composer = cx.new(Composer::new);
        composer.update(cx, |composer, cx| composer.set_role(Role::Field(None), cx));
        cx.observe(&composer, |_, _, cx| cx.notify()).detach();
        self.answers.insert(task.to_string(), composer);
    }

    pub fn answer(&self, task: &str) -> Option<&Entity<Composer>> {
        self.answers.get(task)
    }

    /// The open Task's answer box holds the keyboard.
    pub fn answering(&self) -> bool {
        self.answering
    }

    /// Whether the open Task's answer box holds the keyboard: a press on
    /// it, a sheet taking it, an answer taken.
    pub fn set_answering(&mut self, answering: bool) {
        self.answering = answering;
    }

    /// `task`'s sheets as last closed, for a sheet to take as it opens and
    /// give back as it goes.
    pub fn kept(&mut self, task: &str) -> &mut KeptDirection {
        self.kept.entry(task.to_string()).or_default()
    }

    /// A send taken from `task`'s `sheet`: what that sheet held is spent.
    pub fn spend(&mut self, task: &str, sheet: DirectSheet) {
        if let Some(kept) = self.kept.get_mut(task) {
            kept.spend(sheet);
        }
    }

    /// The Task's answer box: its label over its Composer. A press on it
    /// gives it the keyboard.
    fn answer_input(&self, task: &str, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let composer = self.answers.get(task)?.clone();
        let focused = composer.read(cx).focus_handle(cx).is_focused(window);
        let key = "task-answer";
        Some(
            div()
                .id(key)
                .debug_selector(move || key.to_string())
                .capture_any_mouse_down(cx.listener(|direction, _: &MouseDownEvent, _, cx| {
                    direction.answering = true;
                    cx.emit(DirectionEvent::Answering);
                }))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(ferrite_ui::field::text_field(
                    "Your answer".into(),
                    composer,
                    focused,
                    "factory-field-focused",
                ))
                .into_any_element(),
        )
    }

    /// The Overview's say for `task`: what it asks, what is approved,
    /// this Task's own direction, and the buttons that act on it.
    fn say(&self, task: &Task, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        use ferrite_ui::theme::*;
        let mut out = Vec::new();
        let run = task.workflow.as_ref();

        if let Some(direction) = &task.oversight.direction {
            let name = |id: &str| {
                run.and_then(|run| run.definition.step(id))
                    .map_or_else(|| id.to_string(), |step| step.name.clone())
            };
            let mut said = format!("Sent to {}", name(&direction.to));
            if !direction.skip.is_empty() {
                let skipped: Vec<String> = direction.skip.iter().map(|id| name(id)).collect();
                said.push_str(&format!(", skipping {}", skipped.join(", ")));
            }
            said.push_str(match direction.then {
                ThenReturn::Me => ", then back to you",
                ThenReturn::Routes => ", then the workflow's routes",
            });
            out.push(
                section("task-direction", "This task only")
                    .child(div().child(SharedString::from(said)))
                    .into_any_element(),
            );
        }

        if task.status == Status::NeedsHuman {
            if let Some(question) = &task.oversight.question {
                let send = ferrite_ui::form::primary_button(
                    "task-answer-send",
                    "Send answer",
                    false,
                    cx,
                )
                .debug_selector(|| "task-answer-send".into())
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    cx.emit(DirectionEvent::SendAnswer)
                }));
                out.push(
                    section("task-question", "Needs you")
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(SPACE_1))
                                .text_color(rgb(ATTENTION))
                                .font_weight(W_LABEL)
                                .child(SharedString::from(format!("{} asks", question.asker))),
                        )
                        .child(
                            div()
                                .debug_selector(|| "task-question-text".into())
                                .child(SharedString::from(question.text.clone())),
                        )
                        .children(self.answer_input(&task.id, window, cx))
                        .child(meta(format!(
                            "{} resumes with the question and your answer.",
                            question.asker
                        )))
                        .child(div().flex().child(send))
                        .into_any_element(),
                );
            } else if let Some(permission) = &task.oversight.permission {
                out.push(
                    section("task-permission", "Needs your permission")
                        .child(div().child(SharedString::from(
                            task.reason.clone().unwrap_or_else(|| permission.clone()),
                        )))
                        .child(meta(
                            "The agent's own request. Answer it in the Worker tab; it never \
                             goes to the Inbox.",
                        ))
                        .child(
                            div().flex().child(
                                ferrite_ui::components::secondary_button(
                                    "task-permission-open",
                                    None,
                                    "Answer in the Worker tab",
                                    cx,
                                )
                                .debug_selector(|| "task-permission-open".into())
                                .on_click(cx.listener(
                                    |_, _: &ClickEvent, _, cx| {
                                        cx.emit(DirectionEvent::WorkerTab)
                                    },
                                )),
                            ),
                        )
                        .into_any_element(),
                );
            }
        }

        if task.status == Status::Approval {
            let mut ready = section("task-approval", "Ready to approve");
            if let Some(result) = &task.result {
                ready = ready.child(
                    div()
                        .debug_selector(|| "task-approval-result".into())
                        .child(SharedString::from(result.clone())),
                );
            }
            if let Some(reason) = &task.reason {
                ready = ready.child(
                    div()
                        .text_color(rgb(ATTENTION))
                        .child(SharedString::from(reason.clone())),
                );
            }
            if let Some(run) = run {
                ready = ready.child(meta(format!(
                    "You approve this result of {} (version {}).",
                    run.definition.name, run.version
                )));
                let after = match &run.definition.approval.after {
                    Destination::Step(_) => format!(
                        "After approval: {} runs next.",
                        read(task, &run.definition.approval.after)
                    ),
                    _ => "After approval: the task completes.".to_string(),
                };
                ready = ready.child(meta(after).debug_selector(|| "task-approval-after".into()));
                if run.definition.approval.by == Approver::Agent && task.oversight.to_me {
                    ready = ready.child(meta(
                        "Agent approval is set for this workflow; this task was returned to you.",
                    ));
                }
            }
            if let Some(approving) = crate::delivery::approving_line(task) {
                ready =
                    ready.child(meta(approving).debug_selector(|| "task-approval-revision".into()));
            }
            if !task.delivery.evidence.is_empty() {
                ready = ready
                    .child(meta("Evidence"))
                    .child(crate::delivery::evidence_list(task));
            } else if !task.results.is_empty() {
                let mut evidence = div()
                    .debug_selector(|| "task-evidence".into())
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_1));
                for (at, result) in task.results.iter().enumerate() {
                    let (word, text) = match &result.outcome {
                        factory_board::Outcome::Finished { summary } => ("finished", summary),
                        factory_board::Outcome::Unable { tried } => ("could not finish", tried),
                        factory_board::Outcome::Approved { summary } => ("approved", summary),
                        factory_board::Outcome::Rejected { reasons } => ("did not approve", reasons),
                        factory_board::Outcome::Merge { summary, .. } => ("merge result", summary),
                    };
                    evidence = evidence.child(
                        div()
                            .debug_selector(move || format!("task-evidence-{at}"))
                            .flex()
                            .gap(px(SPACE_2))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(FS_META))
                                    .text_color(rgb(INK_SUBTLE))
                                    .child(SharedString::from(format!(
                                        "Agent \u{b7} {} {word}",
                                        result.name
                                    ))),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(SharedString::from(text.clone())),
                            ),
                    );
                }
                ready = ready.child(meta("Evidence")).child(evidence);
            }
            let approve =
                ferrite_ui::form::primary_button("task-approve", "Approve", false, cx)
                    .debug_selector(|| "task-approve".into())
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        cx.emit(DirectionEvent::Approve)
                    }));
            let rework = ferrite_ui::components::secondary_button(
                "task-rework-open",
                None,
                "Request rework",
                cx,
            )
            .debug_selector(|| "task-rework-open".into())
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.emit(DirectionEvent::Open(DirectSheet::Rework))
            }));
            out.push(
                ready
                    .child(div().flex().gap(px(SPACE_2)).child(approve).child(rework))
                    .into_any_element(),
            );
        }

        if run.is_some() {
            let send_back = matches!(task.status, Status::Stopped)
                || (task.status == Status::NeedsHuman && task.oversight.permission.is_none());
            let mut direct = section("task-direct", "Direct this task").child(meta(
                "Send it to any agent, skip agents, or have it come back to you. Only this task \
                 changes.",
            ));
            let mut buttons = div().flex().gap(px(SPACE_2));
            if task.status != Status::Completed {
                buttons = buttons.child(
                    ferrite_ui::components::secondary_button(
                        "task-redirect-open",
                        None,
                        "Direct this task",
                        cx,
                    )
                    .debug_selector(|| "task-redirect-open".into())
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                        cx.emit(DirectionEvent::Open(DirectSheet::Redirect))
                    })),
                );
            }
            if send_back {
                buttons = buttons.child(
                    ferrite_ui::components::ghost_button(
                        "task-send-back",
                        "Send back with feedback",
                        cx,
                    )
                    .debug_selector(|| "task-send-back".into())
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                        cx.emit(DirectionEvent::Open(DirectSheet::Rework))
                    })),
                );
            }
            if task.status != Status::Completed {
                direct = direct.child(buttons);
                out.push(direct.into_any_element());
            }
        }
        out
    }
}

impl Render for DirectionView {
    /// The open Task's say, its sections spaced as the Overview's lines.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use ferrite_ui::theme::*;
        let task = self.inputs.read(&(), cx);
        let say = task.map(|task| self.say(&task, window, cx));
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_3))
            .children(say.into_iter().flatten())
    }
}

/// Whether the Overview has anything to say for `task`: its own
/// direction, a question or a permission it waits on, a result to approve,
/// or a Workflow's Task still to direct. `DirectionView::say` draws them.
pub fn says(task: &Task) -> bool {
    task.oversight.direction.is_some()
        || (task.status == Status::NeedsHuman
            && (task.oversight.question.is_some() || task.oversight.permission.is_some()))
        || task.status == Status::Approval
        || (task.workflow.is_some() && task.status != Status::Completed)
}

/// A section of the Task's Overview: its label over its content.
pub fn section(selector: &'static str, title: impl Into<SharedString>) -> Div {
    use ferrite_ui::theme::*;
    div()
        .debug_selector(move || selector.to_string())
        .flex()
        .flex_col()
        .gap(px(SPACE_2))
        .pt(px(SPACE_2))
        .border_t_1()
        .border_color(paint::LINE)
        .child(
            div()
                .text_size(px(FS_META))
                .font_weight(W_LABEL)
                .text_color(rgb(INK_SUBTLE))
                .child(title.into()),
        )
}

#[cfg(test)]
mod tests {
    use ferrite_ui::testing::{click, mount, record, shows};
    use gpui::{TestAppContext, VisualTestContext};

    use super::*;

    /// A Workflow Task of three Steps as the Server would publish it, at
    /// its second, with `status` and `extra` laid over it.
    fn task(status: Status, extra: serde_json::Value) -> Task {
        let step = |id: &str, name: &str| {
            serde_json::json!({
                "id": id, "name": name, "instructions": format!("{name} it"),
                "provider": "claude", "on_finished": "next", "on_unable": "human",
            })
        };
        let mut task = serde_json::json!({
            "id": "t-1",
            "project": 4,
            "title": "Add a --fast flag",
            "instructions": "Add a --fast flag",
            "status": status,
            "agent": {"provider": "claude", "model": "claude-opus-4"},
            "branch": "factory/t-1",
            "activity": [],
            "created_ms": 1,
            "workflow": {
                "id": "w-1",
                "version": 2,
                "definition": {
                    "name": "Ship it",
                    "steps": [step("s-1", "Build"), step("s-2", "Review"), step("s-3", "Check")],
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

    fn question() -> serde_json::Value {
        serde_json::json!({
            "question": {"id": 3, "text": "Should --fast default to on?", "asker": "Review", "worker": 40},
            "asked": 3,
            "reason": "Review asks: Should --fast default to on?",
        })
    }

    /// The open Task, as the window keeps it: none while no panel is open.
    struct Open(Option<Task>);

    /// The say over `open`, its Task's answer box made as the window makes
    /// it when the panel opens.
    fn open(open: Option<Task>, cx: &mut TestAppContext) -> (Entity<DirectionView>, Entity<Open>, &mut VisualTestContext) {
        let ensure = open.as_ref().map(|task| task.id.clone());
        let open = cx.new(|_| Open(open));
        let inputs = Source::of(&open, |open: &Open, _: &(), _| open.0.clone());
        let (view, cx) = mount(cx, |_, cx| {
            let mut view = DirectionView::new(inputs);
            if let Some(task) = ensure {
                view.ensure(&task, cx);
            }
            view
        });
        (view, open, cx)
    }

    fn said(event: &DirectionEvent) -> String {
        match event {
            DirectionEvent::Answering => "answering".into(),
            DirectionEvent::SendAnswer => "send answer".into(),
            DirectionEvent::Approve => "approve".into(),
            DirectionEvent::Open(sheet) => format!("open {sheet:?}"),
            DirectionEvent::WorkerTab => "worker tab".into(),
        }
    }

    fn text(cx: &mut VisualTestContext, selector: &str) -> String {
        cx.update(|window, _| window.debug_text(selector)).unwrap_or_default().join(" ")
    }

    /// A question: who asks, what, the answer box and Send answer; a press
    /// on the box takes the keyboard, and Send answer says so.
    #[gpui::test]
    fn a_question_is_asked_with_its_answer_box(cx: &mut TestAppContext) {
        let (view, _open, cx) = open(Some(task(Status::NeedsHuman, question())), cx);
        let heard = record(cx, &view, said);
        assert!(shows(cx, "task-question"));
        assert_eq!(text(cx, "task-question-text"), "Should --fast default to on?");
        assert!(text(cx, "task-question").contains("Review asks"));
        assert!(!shows(cx, "task-permission"));
        assert!(!view.read_with(cx, |view, _| view.answering()));
        click(cx, "task-answer");
        assert!(view.read_with(cx, |view, _| view.answering()), "the box holds the keyboard");
        click(cx, "task-answer-send");
        assert_eq!(heard.take(), ["answering", "send answer"]);
    }

    /// What is typed in a Task's box stays its own: each Task has a box of
    /// its own, made once.
    #[gpui::test]
    fn each_tasks_answer_box_is_its_own_and_made_once(cx: &mut TestAppContext) {
        let (view, _open, cx) = open(Some(task(Status::NeedsHuman, question())), cx);
        view.update(cx, |view, cx| {
            let first = view.answer("t-1").cloned().unwrap();
            view.ensure("t-1", cx);
            assert_eq!(view.answer("t-1"), Some(&first), "made once");
            view.ensure("t-2", cx);
            assert_ne!(view.answer("t-2"), Some(&first));
        });
        click(cx, "task-answer");
        cx.update(|window, cx| {
            let focus = view.read(cx).answer("t-1").unwrap().read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        });
        cx.simulate_input("Off");
        cx.run_until_parked();
        let typed = |task: &str, cx: &mut VisualTestContext| {
            view.read_with(cx, |view, cx| view.answer(task).unwrap().read(cx).text().to_string())
        };
        assert_eq!((typed("t-1", cx), typed("t-2", cx)), ("Off".into(), String::new()));
    }

    /// A Worker's permission says what is asked and leads to the Worker tab.
    #[gpui::test]
    fn a_workers_permission_leads_to_the_worker_tab(cx: &mut TestAppContext) {
        let permission = serde_json::json!({
            "permission": "Run cargo test",
            "reason": "Review asks permission: Run cargo test",
        });
        let (view, _open, cx) = open(Some(task(Status::NeedsHuman, permission)), cx);
        let heard = record(cx, &view, said);
        assert!(shows(cx, "task-permission"));
        assert!(text(cx, "task-permission").contains("Review asks permission: Run cargo test"));
        assert!(!shows(cx, "task-question"));
        click(cx, "task-permission-open");
        assert_eq!(heard.take(), ["worker tab"]);
    }

    /// Ready to approve: the result, each Step's result as evidence, what
    /// follows approval; Approve and Request rework say so.
    #[gpui::test]
    fn ready_to_approve_says_what_is_approved(cx: &mut TestAppContext) {
        let approval = serde_json::json!({
            "result": "Checks pass",
            "results": [
                {"step": "s-1", "name": "Build", "outcome": {"finished": {"summary": "Added --fast"}}},
                {"step": "s-3", "name": "Check", "outcome": {"unable": {"tried": "No CI"}}},
            ],
        });
        let (view, _open, cx) = open(Some(task(Status::Approval, approval)), cx);
        let heard = record(cx, &view, said);
        assert_eq!(text(cx, "task-approval-result"), "Checks pass");
        assert!(text(cx, "task-evidence-0").contains("Build finished"));
        assert!(text(cx, "task-evidence-1").contains("Check could not finish"));
        assert!(text(cx, "task-approval").contains("Ship it (version 2)"));
        assert_eq!(text(cx, "task-approval-after"), "After approval: the task completes.");
        click(cx, "task-approve");
        click(cx, "task-rework-open");
        assert_eq!(heard.take(), ["approve", "open Rework"]);
    }

    /// Direct this task stands while a Workflow Task is not Completed; a
    /// Stopped one is sent back with feedback.
    #[gpui::test]
    fn direct_this_task_and_send_back_open_their_sheets(cx: &mut TestAppContext) {
        let (view, open_, cx) = open(Some(task(Status::Working, serde_json::json!({}))), cx);
        let heard = record(cx, &view, said);
        assert!(shows(cx, "task-direct"));
        assert!(!shows(cx, "task-send-back"), "a Working Task is not sent back");
        click(cx, "task-redirect-open");
        open_.update(cx, |open, cx| {
            open.0 = Some(task(Status::Stopped, serde_json::json!({})));
            cx.notify();
        });
        cx.run_until_parked();
        click(cx, "task-send-back");
        assert_eq!(heard.take(), ["open Redirect", "open Rework"]);
        open_.update(cx, |open, cx| {
            open.0 = Some(task(Status::Completed, serde_json::json!({})));
            cx.notify();
        });
        cx.run_until_parked();
        assert!(!shows(cx, "task-direct"), "nothing to direct once Completed");
    }

    /// The Task's own direction says where it was sent, what it skips and
    /// who it comes back to.
    #[gpui::test]
    fn this_tasks_own_direction_is_said(cx: &mut TestAppContext) {
        let direction = serde_json::json!({
            "override": {"to": "s-3", "skip": ["s-1"], "then": "me", "note": ""},
        });
        let (_view, _open, cx) = open(Some(task(Status::Working, direction)), cx);
        assert_eq!(
            text(cx, "task-direction"),
            "This task only Sent to Check, skipping Build, then back to you"
        );
    }

    /// With no panel open the say draws nothing.
    #[gpui::test]
    fn with_no_task_open_it_says_nothing(cx: &mut TestAppContext) {
        let (_view, open_, cx) = open(Some(task(Status::Approval, serde_json::json!({}))), cx);
        assert!(shows(cx, "task-approval"));
        open_.update(cx, |open, cx| {
            open.0 = None;
            cx.notify();
        });
        cx.run_until_parked();
        assert!(!shows(cx, "task-approval") && !shows(cx, "task-direct"));
    }

    /// What it keeps of each Task's sheets is that Task's, and a send
    /// spends the one sheet.
    #[gpui::test]
    fn each_tasks_sheets_are_kept_apart(cx: &mut TestAppContext) {
        let (view, _open, cx) = open(None, cx);
        view.update(cx, |view, _| {
            view.set_answering(true);
            assert!(view.answering());
            let _ = view.kept("t-1");
            view.spend("t-2", DirectSheet::Rework);
            assert_eq!(view.kept.len(), 1, "spending none kept keeps none");
        });
    }

    /// Whether the Overview has anything to say: its own direction, a
    /// question or permission, a result to approve, or a Workflow Task
    /// still to direct.
    #[test]
    fn says_when_there_is_something_to_say() {
        let one_off = |status, extra| {
            let mut task = task(status, extra);
            task.workflow = None;
            task
        };
        let none = || serde_json::json!({});
        assert!(says(&task(Status::Working, none())));
        assert!(!says(&task(Status::Completed, none())));
        assert!(!says(&one_off(Status::Working, none())));
        assert!(says(&one_off(Status::Approval, none())));
        assert!(says(&one_off(Status::NeedsHuman, question())));
        assert!(!says(&one_off(Status::NeedsHuman, none())));
    }
}
