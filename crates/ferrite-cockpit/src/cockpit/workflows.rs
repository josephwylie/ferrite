//! A Project's Workflows in the Factory view (ADR 0037, #221), as the
//! window keeps them. The Workflow lives in the Server's Factory: every
//! edit is one command (`Cockpit::workflow`) and the draft the Server
//! publishes is what is drawn. The editor is
//! `ferrite_factory::workflows::WorkflowsView`, whose render is the
//! surface, and which keeps what is chosen, each field's Composer and the
//! field that holds the keyboard; the root hosts it, makes the fields the
//! draft shows and answers every edit.

use super::*;
use ferrite_core::factory_board::{
    ApprovalChange, Catalog, Problem, RouteKind, Step, StepChange, StepId, Template, Workflow,
    WorkflowId,
};
use ferrite_core::protocol::{Done, WorkflowAction};
use ferrite_factory::board::Surface;

pub(crate) use ferrite_factory::field::Field;
pub(crate) use ferrite_factory::picker::{Pick, Trigger};
pub(crate) use ferrite_factory::workflows::{
    Inspected, StepDrag, WorkflowsEvent, WorkflowsInputs, WorkflowsView,
};
mod editor;
mod pickers;
mod save;
mod skills;

impl CockpitView {
    /// Switch the Factory view between its Board and its Workflows.
    pub(crate) fn factory_surface(&mut self, surface: Surface, cx: &mut Context<Self>) {
        let shown = self.update_factory(cx, |view, cx| {
            view.set_surface(surface);
            view.editor().update(cx, |editor, _| editor.set_focused(None));
        });
        if shown.is_none() {
            return;
        }
        self.ensure_fields(cx);
        if surface == Surface::Coordinator {
            // Its Pane takes the keyboard, as a Worker's on its tab.
            if let Some(index) = self
                .coordinator_thread(cx)
                .and_then(|thread| self.pane_for(thread))
            {
                self.focus_pane(index, cx);
            }
        }
        cx.notify();
    }

    /// The Workflows editor, while the Factory shows.
    fn factory_editor(&self, cx: &gpui::App) -> Option<Entity<WorkflowsView>> {
        self.factory().map(|view| view.read(cx).editor().clone())
    }

    /// The Workflow the surface shows: the one chosen, else the first.
    pub(crate) fn shown_workflow<'a>(&self, cx: &'a gpui::App) -> Option<&'a Workflow> {
        self.chosen_workflow(self.factory()?.read(cx).editor().read(cx).selected().map(String::as_str), cx)
    }

    /// The Workflow shown with `chosen` chosen: it, else the Project's
    /// first.
    fn chosen_workflow<'a>(&self, chosen: Option<&str>, cx: &'a gpui::App) -> Option<&'a Workflow> {
        let key = self.factory_project()?;
        let project = key.project();
        let board = self.cockpit.read(cx).factory(key.machine()).reading();
        chosen
            .and_then(|id| board.workflow(id))
            .filter(|workflow| workflow.project == project)
            .or_else(|| board.workflows_of(project).next())
    }

    /// The Step the inspector shows: the one chosen, else the first.
    pub(crate) fn shown_step<'a>(&self, cx: &'a gpui::App) -> Option<&'a Step> {
        let workflow = self.shown_workflow(cx)?;
        self.factory()?.read(cx).editor().read(cx).shown_step(&workflow.draft)
    }

    /// The shown Workflow's Step `step`, as the draft holds it now.
    fn shown_step_of(&self, step: &str, cx: &gpui::App) -> Option<Step> {
        self.shown_workflow(cx)?.draft.step(step).cloned()
    }

    /// One Workflow ask, on the machine of the Factory shown; a refusal is
    /// said in the view's note.
    pub(crate) fn workflow_ask(
        &mut self,
        action: WorkflowAction,
        cx: &mut Context<Self>,
    ) -> Option<Done> {
        let machine = self.factory_machine()?;
        let done = match self.cockpit.update(cx, |cockpit, _| cockpit.workflow(machine, action)) {
            Ok(done) => {
                self.update_factory(cx, |view, _| view.set_note(None));
                done
            }
            Err(error) => {
                self.update_factory(cx, |view, _| {
                    view.set_note(Some(SharedString::from(error.to_string())))
                });
                None
            }
        };
        self.ensure_fields(cx);
        cx.notify();
        done
    }

    pub(crate) fn create_workflow(&mut self, template: Template, cx: &mut Context<Self>) {
        let Some(project) = self.factory_project() else {
            return;
        };
        if let Some(Done::Workflow { workflow }) =
            self.workflow_ask(
                WorkflowAction::Create {
                    project: project.project(),
                    template,
                },
                cx,
            )
        {
            self.select_workflow(workflow, cx);
        }
    }

    pub(crate) fn select_workflow(&mut self, workflow: WorkflowId, cx: &mut Context<Self>) {
        self.update_factory(cx, |view, cx| {
            view.set_surface(Surface::Workflows);
            view.editor()
                .update(cx, |editor, _| editor.select_workflow(workflow));
        });
        self.ensure_fields(cx);
        cx.notify();
    }

    pub(crate) fn select_step(&mut self, step: StepId, cx: &mut Context<Self>) {
        if let Some(editor) = self.factory_editor(cx) {
            editor.update(cx, |editor, _| editor.select_step(step));
        }
        self.ensure_fields(cx);
        cx.notify();
    }

    /// Show the approval settings in the inspector.
    pub(crate) fn select_approval(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.factory_editor(cx) {
            editor.update(cx, |editor, _| editor.inspect(Inspected::Approval));
        }
        self.ensure_fields(cx);
        cx.notify();
    }

    /// Change the shown Workflow's approval settings.
    pub(crate) fn edit_approval(&mut self, change: ApprovalChange, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_id(cx) else {
            return;
        };
        self.workflow_ask(WorkflowAction::EditApproval { workflow, change }, cx);
    }

    /// The shown Workflow's id.
    fn shown_id(&self, cx: &gpui::App) -> Option<WorkflowId> {
        self.shown_workflow(cx).map(|workflow| workflow.id.clone())
    }

    pub(crate) fn add_step(&mut self, after: Option<StepId>, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_id(cx) else {
            return;
        };
        if let Some(Done::Step { step }) =
            self.workflow_ask(WorkflowAction::AddStep { workflow, after }, cx)
        {
            self.select_step(step, cx);
        }
    }

    pub(crate) fn edit_step(&mut self, step: StepId, change: StepChange, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_id(cx) else {
            return;
        };
        self.workflow_ask(
            WorkflowAction::EditStep {
                workflow,
                step,
                change,
            },
            cx,
        );
    }

    /// Move a Step to index `to` of the outline: a drop, or a key.
    pub(crate) fn move_step(&mut self, step: StepId, to: usize, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_id(cx) else {
            return;
        };
        self.workflow_ask(WorkflowAction::MoveStep { workflow, step, to }, cx);
    }

    /// A dragged row let go on the row at `onto`: it takes that place.
    pub(crate) fn drop_step(&mut self, drag: &StepDrag, onto: usize, cx: &mut Context<Self>) {
        self.select_step(drag.step.clone(), cx);
        self.move_step(drag.step.clone(), onto, cx);
    }

    pub(crate) fn delete_step(&mut self, step: StepId, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_id(cx) else {
            return;
        };
        let at = self
            .shown_workflow(cx)
            .and_then(|shown| shown.draft.steps.iter().position(|each| each.id == step));
        self.workflow_ask(WorkflowAction::DeleteStep { workflow, step }, cx);
        // The selection lands on the row that took its place.
        let next = self.shown_workflow(cx).and_then(|shown| {
            let steps = &shown.draft.steps;
            at.and_then(|at| steps.get(at.min(steps.len().saturating_sub(1))))
                .map(|step| step.id.clone())
        });
        if let Some(editor) = self.factory_editor(cx) {
            editor.update(cx, |editor, _| editor.set_step(next));
        }
        self.ensure_fields(cx);
        cx.notify();
    }

    /// Keep a Composer for each text field the surface shows, filled from
    /// the draft when it is first made; one already made keeps what was
    /// typed in it.
    pub(crate) fn ensure_fields(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self
            .factory()
            .map(|view| view.read(cx))
            .filter(|view| view.surface() == Surface::Workflows)
            .map(|view| view.editor().clone())
        else {
            return;
        };
        let mut wanted: Vec<(Field, String)> = vec![(Field::SkillSearch, String::new())];
        if let Some(workflow) = self.shown_workflow(cx) {
            wanted.push((
                Field::WorkflowName(workflow.id.clone()),
                workflow.draft.name.clone(),
            ));
            wanted.push((
                Field::ApprovalInstructions(workflow.id.clone()),
                workflow.draft.approval.instructions.clone(),
            ));
            if let Some(step) = self.shown_step(cx) {
                wanted.push((
                    Field::StepName(workflow.id.clone(), step.id.clone()),
                    step.name.clone(),
                ));
                wanted.push((
                    Field::Instructions(workflow.id.clone(), step.id.clone()),
                    step.instructions.clone(),
                ));
            }
        }
        wanted.extend(self.start_fields(cx));
        editor.update(cx, |editor, cx| {
            for (field, text) in wanted {
                editor.ensure(field, &text, cx);
            }
        });
        if let Some(provider) = self.shown_step(cx).map(|step| step.agent.provider) {
            self.discover_skills(provider, cx);
        }
    }

    /// What was typed in a field goes to the draft at once.
    fn field_edited(&mut self, field: &Field, cx: &mut Context<Self>) {
        let Some(text) = self.field_text(field, cx) else {
            return;
        };
        let action = match field.clone() {
            Field::WorkflowName(workflow) => WorkflowAction::Rename {
                workflow,
                name: text,
            },
            Field::StepName(workflow, step) => WorkflowAction::EditStep {
                workflow,
                step,
                change: StepChange {
                    name: Some(text),
                    ..Default::default()
                },
            },
            Field::Instructions(workflow, step) => WorkflowAction::EditStep {
                workflow,
                step,
                change: StepChange {
                    instructions: Some(text),
                    ..Default::default()
                },
            },
            Field::SkillSearch => {
                cx.notify();
                return;
            }
            Field::ApprovalInstructions(workflow) => WorkflowAction::EditApproval {
                workflow,
                change: ApprovalChange {
                    instructions: Some(text),
                    ..Default::default()
                },
            },
            Field::ScheduleInstructions(_) | Field::ExtraInstructions(_) => {
                self.start_field_edited(field, text, cx);
                return;
            }
        };
        self.workflow_ask(action, cx);
    }

    /// Give a text field the keyboard.
    pub(crate) fn focus_field(&mut self, field: Field, cx: &mut Context<Self>) {
        if let Some(editor) = self.factory_editor(cx) {
            editor.update(cx, |editor, _| editor.set_focused(Some(field)));
        }
        cx.notify();
    }

    /// The text a field holds now.
    pub(crate) fn field_text(&self, field: &Field, cx: &gpui::App) -> Option<String> {
        self.factory()?
            .read(cx)
            .editor()
            .read(cx)
            .field(field)
            .map(|composer| composer.read(cx).text().to_string())
    }

    /// What holds the keyboard on the Workflows surface: the field
    /// chosen, else the outline.
    pub(super) fn workflows_focus(&self, cx: &gpui::App) -> Option<FocusHandle> {
        let view = self.factory()?.read(cx);
        if view.surface() != Surface::Workflows {
            return None;
        }
        let editor = view.editor().read(cx);
        Some(
            editor
                .focus(cx)
                .unwrap_or_else(|| editor.outline_focus().clone()),
        )
    }
}

/// A Workflow as its picker names it.
fn workflow_title(workflow: &Workflow) -> String {
    if workflow.draft.name.trim().is_empty() {
        "Untitled workflow".into()
    } else {
        workflow.draft.name.clone()
    }
}
