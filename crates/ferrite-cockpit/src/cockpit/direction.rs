//! Where a Task needs the operator, in its panel (#222), in the window.
//! The say is `ferrite_factory::direction::DirectionView`, which the
//! Factory holds; it reads the open Task through a `Source` the root makes
//! over itself (`direction_inputs`) and says a press as a
//! `DirectionEvent`. The sends are Server calls, so they stay here, as
//! does opening a sheet, which the sheet host shows.

use super::*;
use ferrite_factory::board::Surface;
use ferrite_core::factory_board::{Override, Task};
use ferrite_core::protocol::TaskAction;
use ferrite_factory::direct::{Choice, DirectSheet, TaskPick};
use ferrite_factory::direction::{DirectionEvent, DirectionView};
use ferrite_pane::composer::Composer;
use ferrite_ui::source::Source;

impl CockpitView {
    /// A Factory's say state, heard from its making.
    pub(super) fn new_direction(&mut self, cx: &mut Context<Self>) -> Entity<DirectionView> {
        let inputs = Source::of(&cx.entity(), |root: &Self, _: &(), cx| root.direction_inputs(cx));
        let direction = cx.new(|_| DirectionView::new(inputs));
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
            DirectionEvent::WorkerTab => self.task_tab(ferrite_factory::board::TaskTab::Worker, cx),
        }
    }

    pub(super) fn direction(&self, cx: &gpui::App) -> Option<Entity<DirectionView>> {
        self.factory().map(|view| view.read(cx).direction().clone())
    }

    /// The Task's answer box, made once.
    pub(crate) fn ensure_answer(&mut self, task: &str, cx: &mut Context<Self>) {
        if let Some(direction) = self.direction(cx) {
            direction.update(cx, |direction, cx| direction.ensure(task, cx));
        }
    }

    pub(crate) fn answer_box(&self, task: &str, cx: &gpui::App) -> Option<Entity<Composer>> {
        self.factory()?.read(cx).direction().read(cx).answer(task).cloned()
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
        let direction = view.direction().read(cx);
        if !direction.answering()
            || view.surface() != Surface::Board
            || view.tab() != ferrite_factory::board::TaskTab::Overview
        {
            return None;
        }
        let answer = direction.answer(view.open().map(String::as_str)?)?;
        Some(answer.read(cx).focus_handle(cx))
    }

    pub(crate) fn focus_answer(&mut self, cx: &mut Context<Self>) {
        if let Some(direction) = self.direction(cx) {
            direction.update(cx, |direction, _| direction.set_answering(true));
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
        self.update_factory(cx, |view, _| view.set_note(note));
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
            self.update_factory(cx, |view, _| view.set_note(Some("Write the answer first.".into())));
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
                direction.update(cx, |direction, _| direction.set_answering(false));
            }
        }
    }

    pub(crate) fn approve_task(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.open_task_now(cx).map(|task| task.id.clone()) else {
            return;
        };
        self.direct_command(TaskAction::Approve { task }, cx);
    }

    /// A send taken: the sheet closes and what it held is spent.
    fn direct_sent(&mut self, task: &str, sheet: DirectSheet, cx: &mut Context<Self>) {
        self.hide_sheet(cx);
        if let Some(direction) = self.direction(cx) {
            direction.update(cx, |direction, _| direction.spend(task, sheet));
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
