//! A Project's Workflows in the Factory view (ADR 0037, #221): an ordered
//! outline of Steps beside an inspector, the pickers that set a Step's
//! routes, Provider, model, Effort and skills, the draft's problems, and
//! Save, which makes the next version and starts nothing.
//!
//! The Workflow lives in the Server's Factory: every edit is one command
//! (`Cockpit::workflow`) and the draft the Server publishes is what is
//! drawn. A text field is a Composer this window keeps for as long as the
//! Factory view is open, so what was typed survives a picker, a revealed
//! field, a notice, a scroll and a switch of surface; the draft keeps it
//! across a save and a reopen. What is chosen, the Composers and the field
//! that holds the keyboard are the editor's own view's (`editor`), whose
//! render is the surface; the root hosts it and answers every edit.

use std::collections::HashMap;

use super::*;
use ferrite_core::factory_board::{
    ApprovalChange, Approver, Catalog, Definition, Destination, Problem, RouteKind, Step,
    StepChange, StepId, Switch, Template, Workflow, WorkflowId,
};
use ferrite_core::protocol::{Done, WorkflowAction};
use ferrite_pane::composer::{Composer, Edited, Role};
use gpui::component::Disableable as _;

pub(crate) use ferrite_factory::field::{field_input, Field};
mod editor;
pub(crate) use editor::{Inspected, WorkflowsEvent, WorkflowsInputs, WorkflowsView};
mod inspector;
mod outline;
mod pickers;
pub(crate) use ferrite_factory::picker::Trigger;
pub(crate) use pickers::{model_choices, model_chosen, model_name};
use pickers::{effort_picks, model_picks, provider_picks};
mod save;
mod skills;
pub(crate) use skills::Skills;

/// The outline's width beside the inspector.
const OUTLINE_W: f32 = 440.;
/// How many skills the search offers at once.
const SKILL_ROWS: usize = 8;

/// What the Factory view shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Surface {
    /// The Coordinator's conversation and proposals (#224).
    Coordinator,
    Board,
    Workflows,
}

/// What a Factory picker's row does.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Pick {
    Select(WorkflowId),
    Create(Template),
    Route {
        step: StepId,
        kind: RouteKind,
        to: Destination,
    },
    Provider(StepId, Provider),
    /// Empty: the Provider's default.
    Model(StepId, String),
    Effort(StepId, String),
    MoveUp(StepId),
    MoveDown(StepId),
    AddAfter(StepId),
    Delete(StepId),
    /// The New task sheet's process: one-off, or a Workflow.
    Process(Option<WorkflowId>),
    /// A choice on a Task's Rework or Direct this task sheet (#222).
    Task(ferrite_factory::direct::TaskPick),
    /// Who approves the shown Workflow's results.
    Approver(Approver),
    /// The Step that decides agent approval.
    Decider(StepId),
    /// What runs once approved.
    AfterApproval(Destination),
    /// Where the Workflow sends rework.
    ReworkGoesTo(Destination),
    /// A "Tasks start" setting (#225).
    Start(ferrite_factory::starts::StartPick),
    /// The Coordinator's Provider, model or effort (#224).
    Coordinator(super::coordinator::CoordinatorPick),
}

/// A Step's row, dragged to a new place in the outline.
#[derive(Clone)]
pub(crate) struct StepDrag {
    pub(crate) step: StepId,
    name: SharedString,
}

impl StepDrag {
    /// A drag of `step`, as a press on its row starts one.
    #[cfg(test)]
    pub(crate) fn of(step: &Step) -> Self {
        Self {
            step: step.id.clone(),
            name: SharedString::from(step.name.clone()),
        }
    }
}

struct StepDragPreview(SharedString);

impl Render for StepDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        use ferrite_ui::theme::*;
        div()
            .px(px(SPACE_3))
            .py(px(SPACE_1))
            .rounded(px(R_CONTROL))
            .bg(paint::RAISED)
            .text_color(rgb(INK_STRONG))
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .child(self.0.clone())
    }
}

fn provider_mark(provider: Provider) -> AnyElement {
    use ferrite_ui::theme::*;
    let (mark, ink) = ferrite_ui::provider::mark(provider);
    ferrite_ui::icons::icon(mark, BAR_MARK, ink)
    .into_any_element()
}

/// A Step's skill, matched by name or description.
fn skill_matches(skill: &ferrite_core::SessionCommand, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    filter.is_empty()
        || skill.name.to_lowercase().contains(&filter)
        || skill.description.to_lowercase().contains(&filter)
        || crate::fuzzy::matches(&filter, &skill.name).is_some()
}

/// A command the provider lists that is a skill.
impl CockpitView {
    /// Switch the Factory view between its Board and its Workflows.
    pub(crate) fn factory_surface(&mut self, surface: Surface, cx: &mut Context<Self>) {
        let shown = self.update_factory(cx, |view, cx| {
            view.surface = surface;
            view.editor.update(cx, |editor, _| editor.focused = None);
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
        self.factory().map(|view| view.read(cx).editor.clone())
    }

    /// The Workflow the surface shows: the one chosen, else the first.
    pub(crate) fn shown_workflow<'a>(&self, cx: &'a gpui::App) -> Option<&'a Workflow> {
        self.chosen_workflow(self.factory()?.read(cx).editor.read(cx).selected.as_deref(), cx)
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
        self.factory()?.read(cx).editor.read(cx).shown_step(&workflow.draft)
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
                self.update_factory(cx, |view, _| view.note = None);
                done
            }
            Err(error) => {
                self.update_factory(cx, |view, _| {
                    view.note = Some(SharedString::from(error.to_string()))
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
            view.surface = Surface::Workflows;
            view.editor
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
            editor.update(cx, |editor, _| editor.step = next);
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
            .filter(|view| view.surface == Surface::Workflows)
            .map(|view| view.editor.clone())
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
            editor.update(cx, |editor, _| editor.focused = Some(field));
        }
        cx.notify();
    }

    /// The text a field holds now.
    pub(crate) fn field_text(&self, field: &Field, cx: &gpui::App) -> Option<String> {
        self.factory()?
            .read(cx)
            .editor
            .read(cx)
            .field(field)
            .map(|composer| composer.read(cx).text().to_string())
    }

    /// What holds the keyboard on the Workflows surface: the field
    /// chosen, else the outline.
    pub(super) fn workflows_focus(&self, cx: &gpui::App) -> Option<FocusHandle> {
        let view = self.factory()?.read(cx);
        if view.surface != Surface::Workflows {
            return None;
        }
        let editor = view.editor.read(cx);
        Some(
            editor
                .focus(cx)
                .unwrap_or_else(|| editor.outline_focus.clone()),
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
