//! Where a Task needs the operator, in its panel (#222): the question and
//! its answer box, a Worker's Provider Decision (answered in the Worker
//! tab, never the Inbox), Ready to approve with what is approved, and the
//! buttons that open the Rework and Direct this task sheets
//! (`direct_view`), whose sends are here.
//!
//! What the operator types is kept per Task for as long as the Factory
//! view is open, so it survives closing a sheet, the panel, a tab switch
//! and a new board; it is cleared only once the Server took it.
//!
//! That state lives in a view of its own (ADR 0041), `DirectionView`: each
//! Task's answer box, whether the open one holds the keyboard, and each
//! Task's sheets as last closed. The Factory holds it. Its render is the
//! open Task's say in its Overview, reading the Task through
//! `direction_inputs`; what a press asks it says as a `DirectionEvent`.
//! The sends are Server calls, so they stay the root's, as does opening a
//! sheet, which the sheet host shows.

use std::collections::HashMap;

use gpui::WeakEntity;

use super::direct_view::{Choice, KeptDirection};
use super::*;
use ferrite_core::factory_board::{
    self, Approver, Destination, Override, Status, StepId, Task, TaskId, ThenReturn,
};
use ferrite_core::protocol::TaskAction;
use ferrite_pane::composer::{Composer, Role};

/// Which of a Task's two sheets: Rework, or Direct this task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirectSheet {
    Rework,
    Redirect,
}

/// A Factory's say state: what is being written to its Tasks.
pub(crate) struct DirectionView {
    cockpit: WeakEntity<CockpitView>,
    /// Each Task's answer box, made once.
    answers: HashMap<TaskId, Entity<Composer>>,
    /// The open Task's answer box holds the keyboard.
    pub(super) answering: bool,
    /// Each Task's Rework and Direct this task sheets as last closed.
    pub(super) kept: HashMap<TaskId, KeptDirection>,
}

/// What the open Task's say asks of the window.
pub(crate) enum DirectionEvent {
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
    pub(super) fn new(cockpit: WeakEntity<CockpitView>) -> Self {
        Self {
            cockpit,
            answers: HashMap::new(),
            answering: false,
            kept: HashMap::new(),
        }
    }

    /// `task`'s answer box, made once.
    fn ensure(&mut self, task: &str, cx: &mut Context<Self>) {
        if self.answers.contains_key(task) {
            return;
        }
        let composer = cx.new(Composer::new);
        composer.update(cx, |composer, cx| composer.set_role(Role::Field(None), cx));
        cx.observe(&composer, |_, _, cx| cx.notify()).detach();
        self.answers.insert(task.to_string(), composer);
    }

    pub(super) fn answer(&self, task: &str) -> Option<&Entity<Composer>> {
        self.answers.get(task)
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
            if let Some(approving) = super::delivery::approving_line(task) {
                ready =
                    ready.child(meta(approving).debug_selector(|| "task-approval-revision".into()));
            }
            if !task.delivery.evidence.is_empty() {
                ready = ready
                    .child(meta("Evidence"))
                    .child(super::delivery::evidence_list(task));
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
        let task = self
            .cockpit
            .upgrade()
            .and_then(|cockpit| cockpit.read(cx).direction_inputs(cx));
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
pub(super) fn says(task: &Task) -> bool {
    task.oversight.direction.is_some()
        || (task.status == Status::NeedsHuman
            && (task.oversight.question.is_some() || task.oversight.permission.is_some()))
        || task.status == Status::Approval
        || (task.workflow.is_some() && task.status != Status::Completed)
}

/// What a press of one of the sheets' choosers sets.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TaskPick {
    ReworkTo(Destination),
    RedirectTo(StepId),
}

/// The chooser face for any view, `press` given where its rows go.
pub(crate) fn picker_button<V: 'static>(
    selector: &'static str,
    text: String,
    flagged: bool,
    press: impl Fn(&mut V, super::workflows::Trigger, Point<Pixels>, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> gpui::component::button::Button {
    let look = ferrite_ui::controls::Look {
        flagged,
        full_width: true,
        disabled: false,
    };
    let trigger = super::workflows::Trigger::default();
    ferrite_ui::controls::chooser(selector, "", text.into(), look, cx)
        .debug_selector(move || {
            if flagged {
                format!("{selector}-flagged")
            } else {
                selector.to_string()
            }
        })
        .relative()
        .child(trigger.probe())
        .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
            cx.stop_propagation();
            press(view, trigger.clone(), event.position(), cx);
        }))
}

/// Two or more choices side by side, full width: the shared segments, the
/// chosen one selected as `{selector}-{at}-on`.
pub(crate) fn segmented<V: 'static, T: Copy + PartialEq + 'static>(
    selector: &'static str,
    chosen: T,
    options: &[(T, &'static str)],
    pick: impl Fn(&mut V, T, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> Div {
    let mut track =
        ferrite_ui::controls::segments(true).debug_selector(move || selector.to_string());
    for (at, (value, word)) in options.iter().enumerate() {
        let value = *value;
        let on = value == chosen;
        let key = format!("{selector}-{at}{}", if on { "-on" } else { "" });
        let pick = pick.clone();
        track = track.child(
            ferrite_ui::controls::segment((selector, at), *word, on, true, cx)
                .debug_selector(move || key.clone())
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| pick(view, value, cx))),
        );
    }
    track
}

/// A section of the Task's Overview: its label over its content.
pub(super) fn section(selector: &'static str, title: impl Into<SharedString>) -> Div {
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

pub(super) fn meta(text: impl Into<SharedString>) -> Div {
    use ferrite_ui::theme::*;
    div()
        .text_size(px(FS_META))
        .text_color(rgb(INK_SUBTLE))
        .child(text.into())
}

/// How a Destination reads as a rework recipient or what follows approval.
pub(super) fn read(task: &Task, to: &Destination) -> String {
    match &task.workflow {
        Some(run) => run.definition.route_label("", to),
        None => to.word(),
    }
}

impl CockpitView {
    /// A Factory's say state, heard from its making.
    pub(super) fn new_direction(&mut self, cx: &mut Context<Self>) -> Entity<DirectionView> {
        let me = cx.entity().downgrade();
        let direction = cx.new(|_| DirectionView::new(me));
        cx.subscribe(&direction, Self::on_direction_event).detach();
        direction
    }

    /// What the say draws from the window: the open Task, as the board
    /// says it now.
    fn direction_inputs(&self, cx: &gpui::App) -> Option<Task> {
        self.open_task_now(cx).cloned()
    }

    /// The say's presses. One on the answer box has the focus pump hand it
    /// the keyboard.
    fn on_direction_event(
        &mut self,
        _: Entity<DirectionView>,
        event: &DirectionEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            DirectionEvent::Answering => cx.notify(),
            DirectionEvent::SendAnswer => self.send_answer(cx),
            DirectionEvent::Approve => self.approve_task(cx),
            DirectionEvent::Open(sheet) => self.open_direct_sheet(*sheet, cx),
            DirectionEvent::WorkerTab => self.task_tab(super::factory::TaskTab::Worker, cx),
        }
    }

    pub(super) fn direction(&self, cx: &gpui::App) -> Option<Entity<DirectionView>> {
        self.factory().map(|view| view.read(cx).direction.clone())
    }

    /// The Task's answer box, made once.
    pub(crate) fn ensure_answer(&mut self, task: &str, cx: &mut Context<Self>) {
        if let Some(direction) = self.direction(cx) {
            direction.update(cx, |direction, cx| direction.ensure(task, cx));
        }
    }

    pub(crate) fn answer_box(&self, task: &str, cx: &gpui::App) -> Option<Entity<Composer>> {
        self.factory()?.read(cx).direction.read(cx).answer(task).cloned()
    }

    /// What a Task's answer box holds now.
    pub(crate) fn answer_text(&self, task: &str, cx: &gpui::App) -> String {
        self.answer_box(task, cx)
            .map(|answer| answer.read(cx).text().to_string())
            .unwrap_or_default()
    }

    /// The open Task's answer box, while it holds the keyboard on the
    /// Board's Overview. A sheet's field is the sheet host's.
    pub(super) fn direct_focus(&self, cx: &gpui::App) -> Option<FocusHandle> {
        let view = self.factory()?.read(cx);
        let direction = view.direction.read(cx);
        if !direction.answering
            || view.surface != super::workflows::Surface::Board
            || view.tab != super::factory::TaskTab::Overview
        {
            return None;
        }
        let answer = direction.answer(view.open.as_deref()?)?;
        Some(answer.read(cx).focus_handle(cx))
    }

    pub(crate) fn focus_answer(&mut self, cx: &mut Context<Self>) {
        if let Some(direction) = self.direction(cx) {
            direction.update(cx, |direction, _| direction.answering = true);
        }
        cx.notify();
    }

    /// A Task command, on the machine of the Factory shown; whether its
    /// Server took it. A refusal is said.
    fn direct_command(&mut self, action: TaskAction, cx: &mut Context<Self>) -> bool {
        let Some(machine) = self.factory_machine() else {
            return false;
        };
        let (taken, note) = match self.cockpit.update(cx, |cockpit, _| cockpit.task(machine, action)) {
            Ok(_) => (true, None),
            Err(error) => (false, Some(SharedString::from(error.to_string()))),
        };
        self.update_factory(cx, |view, _| view.note = note);
        cx.notify();
        taken
    }

    /// Send the answer typed for the open Task's question.
    pub(crate) fn send_answer(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.open_task_now(cx).cloned() else {
            return;
        };
        let Some(question) = task.oversight.question.as_ref() else {
            return;
        };
        let text = self.answer_text(&task.id, cx);
        if text.trim().is_empty() {
            self.update_factory(cx, |view, _| view.note = Some("Write the answer first.".into()));
            cx.notify();
            return;
        }
        let action = TaskAction::Answer {
            task: task.id.clone(),
            text: text.trim().to_string(),
            question: Some(question.id),
        };
        if self.direct_command(action, cx) {
            if let Some(answer) = self.answer_box(&task.id, cx) {
                answer.update(cx, |answer, cx| {
                    answer.take(cx);
                });
            }
            // Answered, the box lets the keyboard go.
            if let Some(direction) = self.direction(cx) {
                direction.update(cx, |direction, _| direction.answering = false);
            }
        }
    }

    pub(crate) fn approve_task(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.open_task_now(cx).map(|task| task.id.clone()) else {
            return;
        };
        self.direct_command(TaskAction::Approve { task }, cx);
    }

    /// The sheet up and its Task as the board has it now, with what the
    /// sheet holds: its Task, its text trimmed and its choice.
    fn direct_send(&self, cx: &gpui::App) -> Option<(TaskId, String, Choice)> {
        let sheet = self.direct_sheet(cx)?;
        let sheet = sheet.read(cx);
        self.direct_inputs(sheet.project, &sheet.task, cx)?;
        let text = sheet.field.read(cx).text().trim().to_string();
        Some((sheet.task.clone(), text, sheet.choice.clone()))
    }

    /// A send taken: the sheet closes and what it held is spent.
    fn direct_sent(&mut self, task: &str, sheet: DirectSheet, cx: &mut Context<Self>) {
        self.hide_sheet(cx);
        if let Some(direction) = self.direction(cx) {
            direction.update(cx, |direction, _| {
                if let Some(kept) = direction.kept.get_mut(task) {
                    kept.spend(sheet);
                }
            });
        }
    }

    /// Send for rework: feedback is needed; the recipient is the one
    /// chosen, else the Workflow's.
    pub(crate) fn send_rework(&mut self, cx: &mut Context<Self>) {
        let Some((task, feedback, Choice::Rework(choice))) = self.direct_send(cx) else {
            return;
        };
        if feedback.is_empty() {
            return;
        }
        let action = TaskAction::Rework {
            task: task.clone(),
            feedback,
            to: choice.to,
        };
        if self.direct_command(action, cx) {
            self.direct_sent(&task, DirectSheet::Rework, cx);
        }
    }

    /// Apply the redirect: a Step is needed.
    pub(crate) fn apply_redirect(&mut self, cx: &mut Context<Self>) {
        let Some((task, note, Choice::Redirect(choice))) = self.direct_send(cx) else {
            return;
        };
        let Some(to) = choice.to else {
            return;
        };
        let action = TaskAction::Redirect {
            task: task.clone(),
            direction: Override {
                skip: choice
                    .skip
                    .iter()
                    .filter(|step| **step != to)
                    .cloned()
                    .collect(),
                to,
                then: choice.then,
                note,
            },
        };
        if self.direct_command(action, cx) {
            self.direct_sent(&task, DirectSheet::Redirect, cx);
        }
    }

    /// A recipient or a Step chosen in the picker a sheet opened.
    pub(crate) fn apply_task_pick(&mut self, pick: TaskPick, cx: &mut Context<Self>) {
        if let Some(sheet) = self.direct_sheet(cx) {
            sheet.update(cx, |sheet, cx| sheet.pick(pick, cx));
        }
    }
}
