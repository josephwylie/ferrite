//! Where a Task needs the operator, in its panel (#222): the question and
//! its answer box, a Worker's Provider Decision (answered in the Worker
//! tab, never the Inbox), Ready to approve with what is approved, and the
//! Rework and Direct this task sheets.
//!
//! What the operator types is kept per Task for as long as the Factory
//! view is open, so it survives closing a sheet, the panel, a tab switch
//! and a new board; it is cleared only once the Server took it.

use std::collections::HashMap;

use super::*;
use ferrite_core::factory::{
    self as core_factory, Approver, Destination, Override, Status, StepId, Task, TaskId, ThenReturn,
};
use ferrite_core::protocol::TaskAction;
use ferrite_pane::composer::{Composer, Role};

/// One Task's unsent words and choices.
pub(crate) struct TaskDrafts {
    pub(crate) answer: Entity<Composer>,
    pub(crate) feedback: Entity<Composer>,
    pub(crate) note: Entity<Composer>,
    /// The rework recipient chosen for this Task; none, the Workflow's.
    pub(crate) rework_to: Option<Destination>,
    pub(crate) redirect_to: Option<StepId>,
    pub(crate) skip: Vec<StepId>,
    pub(crate) then: ThenReturn,
}

/// A text field of a Task's say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirectField {
    Answer,
    Feedback,
    Note,
}

impl DirectField {
    fn key(self) -> &'static str {
        match self {
            DirectField::Answer => "task-answer",
            DirectField::Feedback => "rework-feedback",
            DirectField::Note => "redirect-note",
        }
    }

    fn of(self, drafts: &TaskDrafts) -> &Entity<Composer> {
        match self {
            DirectField::Answer => &drafts.answer,
            DirectField::Feedback => &drafts.feedback,
            DirectField::Note => &drafts.note,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirectSheet {
    Rework,
    Redirect,
}

/// The Factory view's say state.
#[derive(Default)]
pub(crate) struct Direct {
    pub(crate) drafts: HashMap<TaskId, TaskDrafts>,
    /// The open sheet, and its Task.
    pub(crate) sheet: Option<(TaskId, DirectSheet)>,
    /// Its primary action was pressed: say what is missing.
    pub(crate) tried: bool,
    pub(crate) focused: Option<DirectField>,
}

/// What a press of one of the sheets' choosers sets.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TaskPick {
    ReworkTo(Destination),
    RedirectTo(StepId),
}

/// A Factory picker's button: the shared chooser face, full width, its
/// rows opened in the window's picker under it. Flagged, it is selected
/// as `{selector}-flagged`.
pub(crate) fn chooser(
    selector: &'static str,
    text: String,
    flagged: bool,
    picks: Vec<(ferrite_ui::menu::Item, super::workflows::Pick)>,
    cx: &mut Context<CockpitView>,
) -> gpui::component::button::Button {
    let picks = std::rc::Rc::new(picks);
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
            view.open_picks((*picks).clone(), &trigger, event.position(), cx);
        }))
}

/// Two or more choices side by side, full width: the shared segments, the
/// chosen one selected as `{selector}-{at}-on`.
pub(crate) fn segmented<T: Copy + PartialEq + 'static>(
    selector: &'static str,
    chosen: T,
    options: &[(T, &'static str)],
    pick: impl Fn(&mut CockpitView, T, &mut Context<CockpitView>) + Clone + 'static,
    cx: &mut Context<CockpitView>,
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
fn read(task: &Task, to: &Destination) -> String {
    match &task.workflow {
        Some(run) => run.definition.route_label("", to),
        None => to.word(),
    }
}

impl CockpitView {
    /// The Task's drafts, made once.
    pub(crate) fn ensure_drafts(&mut self, task: &str, cx: &mut Context<Self>) {
        let Some(view) = self.factory_view.as_ref() else {
            return;
        };
        if view.direct.drafts.contains_key(task) {
            return;
        }
        let field = |cx: &mut Context<Self>| {
            let composer = cx.new(Composer::new);
            composer.update(cx, |composer, cx| composer.set_role(Role::Field(None), cx));
            cx.observe(&composer, |_, _, cx| cx.notify()).detach();
            composer
        };
        let drafts = TaskDrafts {
            answer: field(cx),
            feedback: field(cx),
            note: field(cx),
            rework_to: None,
            redirect_to: None,
            skip: Vec::new(),
            then: ThenReturn::Routes,
        };
        if let Some(view) = self.factory_view.as_mut() {
            view.direct.drafts.insert(task.to_string(), drafts);
        }
    }

    pub(crate) fn task_drafts(&self, task: &str) -> Option<&TaskDrafts> {
        self.factory_view.as_ref()?.direct.drafts.get(task)
    }

    fn task_drafts_mut(&mut self, task: &str) -> Option<&mut TaskDrafts> {
        self.factory_view.as_mut()?.direct.drafts.get_mut(task)
    }

    /// What a Task's field holds now.
    pub(crate) fn direct_text(&self, task: &str, field: DirectField, cx: &gpui::App) -> String {
        self.task_drafts(task)
            .map(|drafts| field.of(drafts).read(cx).text().to_string())
            .unwrap_or_default()
    }

    /// The field holding the keyboard on the open Task's say, while it is
    /// on screen: its sheet, or the answer box on the Board's Overview.
    pub(super) fn direct_focus(&self, cx: &gpui::App) -> Option<FocusHandle> {
        let view = self.factory_view.as_ref()?;
        let field = view.direct.focused?;
        let task = match &view.direct.sheet {
            Some((task, _)) => task.as_str(),
            None if view.surface == super::workflows::Surface::Board
                && view.tab == super::factory::TaskTab::Overview =>
            {
                view.open.as_deref()?
            }
            None => return None,
        };
        let drafts = view.direct.drafts.get(task)?;
        Some(field.of(drafts).read(cx).focus_handle(cx))
    }

    pub(crate) fn focus_direct(&mut self, field: DirectField, cx: &mut Context<Self>) {
        if let Some(view) = self.factory_view.as_mut() {
            view.direct.focused = Some(field);
        }
        cx.notify();
    }

    /// A Task command; whether the Server took it. A refusal is said.
    fn direct_command(&mut self, action: TaskAction, cx: &mut Context<Self>) -> bool {
        let (taken, note) = match self.cockpit.task(action) {
            Ok(_) => (true, None),
            Err(error) => (false, Some(SharedString::from(error.to_string()))),
        };
        if let Some(view) = self.factory_view.as_mut() {
            view.note = note;
        }
        cx.notify();
        taken
    }

    fn clear_field(&mut self, task: &str, field: DirectField, cx: &mut Context<Self>) {
        if let Some(composer) = self
            .task_drafts(task)
            .map(|drafts| field.of(drafts).clone())
        {
            composer.update(cx, |composer, cx| {
                composer.take(cx);
            });
        }
    }

    /// Send the answer typed for the open Task's question.
    pub(crate) fn send_answer(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.open_task_now().cloned() else {
            return;
        };
        let Some(question) = task.oversight.question.as_ref() else {
            return;
        };
        let text = self.direct_text(&task.id, DirectField::Answer, cx);
        if text.trim().is_empty() {
            if let Some(view) = self.factory_view.as_mut() {
                view.note = Some("Write the answer first.".into());
            }
            cx.notify();
            return;
        }
        let action = TaskAction::Answer {
            task: task.id.clone(),
            text: text.trim().to_string(),
            question: Some(question.id),
        };
        if self.direct_command(action, cx) {
            self.clear_field(&task.id, DirectField::Answer, cx);
            // Answered, the box lets the keyboard go.
            if let Some(view) = self.factory_view.as_mut() {
                view.direct.focused = None;
            }
        }
    }

    pub(crate) fn approve_task(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.open_task_now().map(|task| task.id.clone()) else {
            return;
        };
        self.direct_command(TaskAction::Approve { task }, cx);
    }

    /// Open a sheet on the open Task, with what was typed in it before.
    pub(crate) fn open_direct_sheet(&mut self, sheet: DirectSheet, cx: &mut Context<Self>) {
        let Some(task) = self.open_task_now().map(|task| task.id.clone()) else {
            return;
        };
        self.ensure_drafts(&task, cx);
        if let Some(view) = self.factory_view.as_mut() {
            view.direct.sheet = Some((task, sheet));
            view.direct.tried = false;
            view.direct.focused = Some(match sheet {
                DirectSheet::Rework => DirectField::Feedback,
                DirectSheet::Redirect => DirectField::Note,
            });
        }
        cx.notify();
    }

    /// Close the sheet; what was typed stays for the next time.
    pub(crate) fn close_direct_sheet(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.factory_view.as_mut() {
            view.direct.sheet = None;
            view.direct.tried = false;
            view.direct.focused = None;
        }
        self.close_factory_menu();
        cx.notify();
    }

    fn sheet_task(&self) -> Option<Task> {
        let (task, _) = self.factory_view.as_ref()?.direct.sheet.as_ref()?;
        self.cockpit.factory().task(task).cloned()
    }

    /// Send for rework: feedback is needed; the recipient is the one
    /// chosen, else the Workflow's.
    pub(crate) fn send_rework(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.sheet_task() else {
            return;
        };
        if let Some(view) = self.factory_view.as_mut() {
            view.direct.tried = true;
        }
        let feedback = self.direct_text(&task.id, DirectField::Feedback, cx);
        if feedback.trim().is_empty() {
            cx.notify();
            return;
        }
        let to = self
            .task_drafts(&task.id)
            .and_then(|drafts| drafts.rework_to.clone());
        let action = TaskAction::Rework {
            task: task.id.clone(),
            feedback: feedback.trim().to_string(),
            to,
        };
        if self.direct_command(action, cx) {
            self.clear_field(&task.id, DirectField::Feedback, cx);
            if let Some(drafts) = self.task_drafts_mut(&task.id) {
                drafts.rework_to = None;
            }
            self.close_direct_sheet(cx);
        }
    }

    /// Apply the redirect: a Step is needed.
    pub(crate) fn apply_redirect(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.sheet_task() else {
            return;
        };
        if let Some(view) = self.factory_view.as_mut() {
            view.direct.tried = true;
        }
        let note = self.direct_text(&task.id, DirectField::Note, cx);
        let Some(drafts) = self.task_drafts(&task.id) else {
            return;
        };
        let Some(to) = drafts.redirect_to.clone() else {
            cx.notify();
            return;
        };
        let action = TaskAction::Redirect {
            task: task.id.clone(),
            direction: Override {
                skip: drafts
                    .skip
                    .iter()
                    .filter(|step| **step != to)
                    .cloned()
                    .collect(),
                to,
                then: drafts.then,
                note: note.trim().to_string(),
            },
        };
        if self.direct_command(action, cx) {
            self.clear_field(&task.id, DirectField::Note, cx);
            if let Some(drafts) = self.task_drafts_mut(&task.id) {
                drafts.redirect_to = None;
                drafts.skip.clear();
                drafts.then = ThenReturn::Routes;
            }
            self.close_direct_sheet(cx);
        }
    }

    pub(crate) fn toggle_skip(&mut self, step: StepId, cx: &mut Context<Self>) {
        let Some((task, _)) = self
            .factory_view
            .as_ref()
            .and_then(|view| view.direct.sheet.clone())
        else {
            return;
        };
        if let Some(drafts) = self.task_drafts_mut(&task) {
            match drafts.skip.iter().position(|each| *each == step) {
                Some(at) => {
                    drafts.skip.remove(at);
                }
                None => drafts.skip.push(step),
            }
        }
        cx.notify();
    }

    pub(crate) fn set_then(&mut self, then: ThenReturn, cx: &mut Context<Self>) {
        let Some((task, _)) = self
            .factory_view
            .as_ref()
            .and_then(|view| view.direct.sheet.clone())
        else {
            return;
        };
        if let Some(drafts) = self.task_drafts_mut(&task) {
            drafts.then = then;
        }
        cx.notify();
    }

    pub(crate) fn apply_task_pick(&mut self, pick: TaskPick, cx: &mut Context<Self>) {
        let Some((task, _)) = self
            .factory_view
            .as_ref()
            .and_then(|view| view.direct.sheet.clone())
        else {
            return;
        };
        if let Some(drafts) = self.task_drafts_mut(&task) {
            match pick {
                TaskPick::ReworkTo(to) => drafts.rework_to = Some(to),
                TaskPick::RedirectTo(to) => {
                    drafts.skip.retain(|each| *each != to);
                    drafts.redirect_to = Some(to);
                }
            }
        }
        cx.notify();
    }

    // ----------------------------------------------------------- drawing

    /// One of the Task's text fields: its label over its Composer.
    fn direct_input(
        &self,
        task: &str,
        field: DirectField,
        label: &'static str,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let composer = field.of(self.task_drafts(task)?).clone();
        let focused = composer.read(cx).focus_handle(cx).is_focused(window);
        let key = field.key();
        Some(
            div()
                .id(key)
                .debug_selector(move || key.to_string())
                .capture_any_mouse_down(
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        view.focus_direct(field, cx)
                    }),
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(crate::project_editor::text_field(
                    label.into(),
                    composer,
                    focused,
                    "factory-field-focused",
                ))
                .into_any_element(),
        )
    }

    /// The Overview's say for `task`: what it asks, what is approved,
    /// this Task's own direction, and the buttons that act on it.
    pub(super) fn task_say(
        &self,
        task: &Task,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
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
                let send = crate::project_editor::primary_button(
                    "task-answer-send",
                    "Send answer",
                    false,
                    cx,
                )
                .debug_selector(|| "task-answer-send".into())
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.send_answer(cx)
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
                        .children(self.direct_input(
                            &task.id,
                            DirectField::Answer,
                            "Your answer",
                            window,
                            cx,
                        ))
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
                                    |view, _: &ClickEvent, _, cx| {
                                        view.task_tab(super::factory::TaskTab::Worker, cx)
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
                        core_factory::Outcome::Finished { summary } => ("finished", summary),
                        core_factory::Outcome::Unable { tried } => ("could not finish", tried),
                        core_factory::Outcome::Approved { summary } => ("approved", summary),
                        core_factory::Outcome::Rejected { reasons } => ("did not approve", reasons),
                        core_factory::Outcome::Merge { summary, .. } => ("merge result", summary),
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
                crate::project_editor::primary_button("task-approve", "Approve", false, cx)
                    .debug_selector(|| "task-approve".into())
                    .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.approve_task(cx)
                    }));
            let rework = ferrite_ui::components::secondary_button(
                "task-rework-open",
                None,
                "Request rework",
                cx,
            )
            .debug_selector(|| "task-rework-open".into())
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                view.open_direct_sheet(DirectSheet::Rework, cx)
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
                    .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                        view.open_direct_sheet(DirectSheet::Redirect, cx)
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
                    .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                        view.open_direct_sheet(DirectSheet::Rework, cx)
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

    /// The open Rework or Direct this task sheet, over the window.
    pub(super) fn direct_sheet(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use ferrite_ui::theme::*;
        let view = self.factory_view.as_ref()?;
        let (id, sheet) = view.direct.sheet.clone()?;
        let task = self.cockpit.factory().task(&id)?.clone();
        let run = task.workflow.as_ref()?;
        let definition = &run.definition;
        let drafts = self.task_drafts(&id)?;
        let tried = view.direct.tried;
        let close = ferrite_ui::sheet::sheet_close("direct-close", "Close", cx).on_click(
            cx.listener(|view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.close_direct_sheet(cx);
            }),
        );
        let cancel = ferrite_ui::components::ghost_button("direct-cancel", "Cancel", cx)
            .debug_selector(|| "direct-cancel".into())
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.close_direct_sheet(cx);
            }));
        let label = |text: &'static str| {
            div()
                .text_size(px(FS_META))
                .font_weight(W_LABEL)
                .text_color(rgb(INK_SUBTLE))
                .child(text)
        };
        let (title, intro, body, primary) = match sheet {
            DirectSheet::Rework => {
                let feedback = self.direct_text(&id, DirectField::Feedback, cx);
                let missing = tried && feedback.trim().is_empty();
                let to = drafts
                    .rework_to
                    .clone()
                    .unwrap_or_else(|| definition.approval.rework_to.clone());
                let picks: Vec<(ferrite_ui::menu::Item, super::workflows::Pick)> = definition
                    .steps
                    .iter()
                    .map(|step| Destination::Step(step.id.clone()))
                    .chain([Destination::Coordinator])
                    .map(|each| {
                        (
                            ferrite_ui::menu::Item::new(read(&task, &each)).checked(each == to),
                            super::workflows::Pick::Task(TaskPick::ReworkTo(each)),
                        )
                    })
                    .collect();
                let mut body =
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(SPACE_3))
                        .children(self.direct_input(
                            &id,
                            DirectField::Feedback,
                            "Feedback",
                            window,
                            cx,
                        ));
                if missing {
                    body = body.child(
                        crate::project_editor::error_line(
                            "Write the feedback the agent works from.".into(),
                        )
                        .debug_selector(|| "rework-missing".into()),
                    );
                }
                body = body
                    .child(label("Send to"))
                    .child(chooser("rework-to", read(&task, &to), false, picks, cx))
                    .child(meta(format!(
                        "The workflow sends rework to {}. Choosing another recipient affects this \
                         task only.",
                        read(&task, &definition.approval.rework_to)
                    )));
                let send = crate::project_editor::primary_button(
                    "rework-send",
                    "Send for rework",
                    false,
                    cx,
                )
                .debug_selector(|| "rework-send".into())
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.send_rework(cx)
                }));
                (
                    format!("Rework task #{id}"),
                    "Your feedback is added to the task's context and travels with every later step.",
                    body,
                    send,
                )
            }
            DirectSheet::Redirect => {
                let target = drafts
                    .redirect_to
                    .as_deref()
                    .and_then(|step| definition.step(step));
                let picks = definition
                    .steps
                    .iter()
                    .map(|step| {
                        let current = task.step.as_deref() == Some(step.id.as_str());
                        let mut item = ferrite_ui::menu::Item::new(if current {
                            format!("{} \u{b7} current", step.name)
                        } else {
                            step.name.clone()
                        });
                        item = item.checked(target.is_some_and(|target| target.id == step.id));
                        (
                            item,
                            super::workflows::Pick::Task(TaskPick::RedirectTo(step.id.clone())),
                        )
                    })
                    .collect();
                let mut body = div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_3))
                    .child(label("Send to"))
                    .child(chooser(
                        "redirect-to",
                        target.map_or_else(
                            || "Choose an agent".to_string(),
                            |step| step.name.clone(),
                        ),
                        tried && target.is_none(),
                        picks,
                        cx,
                    ));
                if tried && target.is_none() {
                    body = body.child(
                        crate::project_editor::error_line(
                            "Choose the agent that takes the task.".into(),
                        )
                        .debug_selector(|| "redirect-missing".into()),
                    );
                }
                if task.status == Status::Working {
                    body = body.child(meta(
                        "The current attempt is superseded; any late result from it is ignored.",
                    ));
                }
                let mut skips = div()
                    .debug_selector(|| "redirect-skips".into())
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_1));
                for step in definition
                    .steps
                    .iter()
                    .filter(|step| Some(step.id.as_str()) != drafts.redirect_to.as_deref())
                {
                    let on = drafts.skip.contains(&step.id);
                    let key = format!("redirect-skip-{}{}", step.id, if on { "-on" } else { "" });
                    let id = step.id.clone();
                    skips = skips.child(
                        div()
                            .id(SharedString::from(key.clone()))
                            .debug_selector(move || key.clone())
                            .flex()
                            .items_center()
                            .gap(px(SPACE_2))
                            .cursor_pointer()
                            .child(
                                div()
                                    .size(px(ROW_ICON + SPACE_1))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(R_CONTROL))
                                    .border_1()
                                    .border_color(paint::FIELD_RING)
                                    .when(on, |box_| {
                                        box_.child(ferrite_ui::icons::icon(
                                            ferrite_ui::icons::CHECK,
                                            ROW_ICON,
                                            INK_STRONG,
                                        ))
                                    }),
                            )
                            .child(SharedString::from(step.name.clone()))
                            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                                view.toggle_skip(id.clone(), cx)
                            })),
                    );
                }
                body = body
                    .child(label("Skip for this task"))
                    .child(meta(
                        "When a route reaches a skipped agent, the task moves on as if it had \
                         finished.",
                    ))
                    .child(skips)
                    .child(label("Then"))
                    .child(segmented(
                        "redirect-then",
                        drafts.then,
                        &[
                            (ThenReturn::Routes, "Follow the workflow's routes"),
                            (ThenReturn::Me, "Return it to me to approve"),
                        ],
                        |view, then, cx| view.set_then(then, cx),
                        cx,
                    ))
                    .children(self.direct_input(
                        &id,
                        DirectField::Note,
                        "Feedback (optional)",
                        window,
                        cx,
                    ));
                let apply = crate::project_editor::primary_button(
                    "redirect-apply",
                    "Apply to this task",
                    false,
                    cx,
                )
                .debug_selector(|| "redirect-apply".into())
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.apply_redirect(cx)
                }));
                (
                    format!("Direct task #{id}"),
                    "Applies to this task only. The saved workflow and its other tasks are \
                     unchanged.",
                    body,
                    apply,
                )
            }
        };
        let key = match sheet {
            DirectSheet::Rework => "rework-sheet",
            DirectSheet::Redirect => "redirect-sheet",
        };
        let card = crate::project_editor::card(f32::from(window.viewport_size().height))
            .id(key)
            .debug_selector(move || key.to_string())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(crate::project_editor::head(title.into(), close))
            .child(crate::project_editor::body().child(meta(intro)).child(body))
            .child(crate::project_editor::footer(
                div(),
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(cancel)
                    .child(primary),
            ));
        let veil = crate::project_editor::veil()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.close_direct_sheet(cx);
                }),
            )
            .child(card);
        Some(deferred(veil).with_priority(3).into_any_element())
    }
}
