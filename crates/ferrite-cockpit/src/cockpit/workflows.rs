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
//! that holds the keyboard are the editor's own view's (`editor`).

use std::collections::HashMap;

use super::*;
use ferrite_core::factory::{
    ApprovalChange, Approver, Catalog, Destination, Problem, RouteKind, Step, StepChange, StepId,
    Template, Workflow, WorkflowId,
};
use ferrite_core::protocol::{Done, WorkflowAction};
use ferrite_pane::composer::{Composer, Edited, Role};
use gpui::component::Disableable as _;

mod editor;
pub(crate) use editor::WorkflowsView;
mod inspector;
mod outline;
pub(crate) use outline::field_input;
mod pickers;
pub(crate) use pickers::{model_choices, model_chosen, model_name, Trigger};
mod save;
mod skills;

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

/// A text field of the editor.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Field {
    WorkflowName(WorkflowId),
    StepName(WorkflowId, StepId),
    Instructions(WorkflowId, StepId),
    /// The skills picker's search.
    SkillSearch,
    /// Agent approval's decision instructions (#222).
    ApprovalInstructions(WorkflowId),
    /// What each scheduled run's Task is told (#225).
    ScheduleInstructions(WorkflowId),
    /// A GitHub start's extra instructions for every Task.
    ExtraInstructions(WorkflowId),
}

impl Field {
    /// Instructions, written as prose in a text area that grows.
    pub(crate) fn prose(&self) -> bool {
        matches!(
            self,
            Field::Instructions(..)
                | Field::ApprovalInstructions(_)
                | Field::ScheduleInstructions(_)
                | Field::ExtraInstructions(_)
        )
    }

    /// Its selector.
    pub(crate) fn key(&self) -> String {
        match self {
            Field::WorkflowName(_) => "workflow-name".into(),
            Field::StepName(..) => "inspector-name".into(),
            Field::Instructions(..) => "inspector-instructions".into(),
            Field::SkillSearch => "skill-search".into(),
            Field::ApprovalInstructions(_) => "approval-instructions".into(),
            Field::ScheduleInstructions(_) => "start-schedule-instructions".into(),
            Field::ExtraInstructions(_) => "start-extra-instructions".into(),
        }
    }
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
    Task(super::direction::TaskPick),
    /// Who approves the shown Workflow's results.
    Approver(Approver),
    /// The Step that decides agent approval.
    Decider(StepId),
    /// What runs once approved.
    AfterApproval(Destination),
    /// Where the Workflow sends rework.
    ReworkGoesTo(Destination),
    /// A "Tasks start" setting (#225).
    Start(super::starts::StartPick),
    /// The Coordinator's Provider, model or effort (#224).
    Coordinator(super::coordinator::CoordinatorPick),
}

/// What the Workflows surface keeps beside its editor's view.
#[derive(Default)]
pub(crate) struct Workflows {
    /// The skills search is revealed.
    pub(crate) skills_open: bool,
    /// The skills each Step's Provider has installed.
    pub(crate) skills: skills::Skills,
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
    fn factory_project(&self) -> Option<ProjectId> {
        self.factory().map(|view| view.project)
    }

    /// Switch the Factory view between its Board and its Workflows.
    pub(crate) fn factory_surface(&mut self, surface: Surface, cx: &mut Context<Self>) {
        let Some(view) = self.factory_mut() else {
            return;
        };
        view.surface = surface;
        view.editor.update(cx, |editor, _| editor.focused = None);
        self.ensure_fields(cx);
        if surface == Surface::Coordinator {
            self.ensure_coordinator_field(cx);
            // Its Pane takes the keyboard, as a Worker's on its tab.
            if let Some(index) = self
                .coordinator_thread()
                .and_then(|thread| self.pane_for(thread))
            {
                self.focus_pane(index);
            }
        }
        cx.notify();
    }

    /// The Workflow the surface shows: the one chosen, else the first.
    pub(crate) fn shown_workflow(&self, cx: &gpui::App) -> Option<&Workflow> {
        let view = self.factory()?;
        let board = self.cockpit.factory();
        view.editor
            .read(cx)
            .selected
            .as_deref()
            .and_then(|id| board.workflow(id))
            .filter(|workflow| workflow.project == view.project)
            .or_else(|| board.workflows_of(view.project).next())
    }

    /// The Step the inspector shows: the one chosen, else the first.
    pub(crate) fn shown_step(&self, cx: &gpui::App) -> Option<&Step> {
        let workflow = self.shown_workflow(cx)?;
        let chosen = self.factory()?.editor.read(cx).step.as_deref();
        chosen
            .and_then(|id| workflow.draft.step(id))
            .or_else(|| workflow.draft.steps.first())
    }

    /// One Workflow ask; a refusal is said in the view's note.
    pub(crate) fn workflow_ask(
        &mut self,
        action: WorkflowAction,
        cx: &mut Context<Self>,
    ) -> Option<Done> {
        let done = match self.cockpit.workflow(action) {
            Ok(done) => {
                if let Some(view) = self.factory_mut() {
                    view.note = None;
                }
                done
            }
            Err(error) => {
                if let Some(view) = self.factory_mut() {
                    view.note = Some(SharedString::from(error.to_string()));
                }
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
            self.workflow_ask(WorkflowAction::Create { project, template }, cx)
        {
            self.select_workflow(workflow, cx);
        }
    }

    pub(crate) fn select_workflow(&mut self, workflow: WorkflowId, cx: &mut Context<Self>) {
        if let Some(view) = self.factory_mut() {
            view.surface = Surface::Workflows;
            view.editor
                .update(cx, |editor, _| editor.select_workflow(workflow));
        }
        self.ensure_fields(cx);
        cx.notify();
    }

    pub(crate) fn select_step(&mut self, step: StepId, cx: &mut Context<Self>) {
        if let Some(view) = self.factory() {
            view.editor.update(cx, |editor, _| editor.select_step(step));
        }
        self.ensure_fields(cx);
        cx.notify();
    }

    /// Show the approval settings in the inspector.
    pub(crate) fn select_approval(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.factory() {
            view.editor.update(cx, |editor, _| editor.select_approval());
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
        if let Some(view) = self.factory() {
            view.editor.update(cx, |editor, _| editor.step = next);
        }
        self.ensure_fields(cx);
        cx.notify();
    }

    /// The outline's keys: ↑ ↓ choose a Step, ⌥↑ ⌥↓ move it.
    pub(crate) fn outline_key(&mut self, keystroke: &gpui::Keystroke, cx: &mut Context<Self>) {
        let (Some(workflow), Some(step)) = (self.shown_workflow(cx), self.shown_step(cx)) else {
            return;
        };
        let steps: Vec<StepId> = workflow.draft.steps.iter().map(|s| s.id.clone()).collect();
        let step = step.id.clone();
        let Some(at) = steps.iter().position(|each| *each == step) else {
            return;
        };
        let up = match keystroke.key.as_str() {
            "up" => true,
            "down" => false,
            _ => return,
        };
        let to = if up {
            at.checked_sub(1)
        } else {
            (at + 1 < steps.len()).then_some(at + 1)
        };
        let Some(to) = to else {
            return;
        };
        if keystroke.modifiers.alt {
            self.move_step(step, to, cx);
        } else {
            self.select_step(steps[to].clone(), cx);
        }
        // The chosen row stays in view: the head is the outline's first child.
        if let Some(view) = self.factory() {
            view.editor.read(cx).outline_scroll.scroll_to_item(to + 1);
        }
    }

    /// Keep a Composer for each text field the surface shows, filled from
    /// the draft when it is first made; one already made keeps what was
    /// typed in it.
    pub(crate) fn ensure_fields(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.factory() else {
            return;
        };
        if view.surface != Surface::Workflows {
            return;
        }
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
        view.editor.clone().update(cx, |editor, cx| {
            for (field, text) in wanted {
                editor.ensure(field, &text, cx);
            }
        });
        if let Some(provider) = self.shown_step(cx).map(|step| step.agent.provider) {
            self.discover_skills(provider);
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
        if let Some(view) = self.factory() {
            view.editor.update(cx, |editor, _| editor.focused = Some(field));
        }
        cx.notify();
    }

    /// The text a field holds now.
    pub(crate) fn field_text(&self, field: &Field, cx: &gpui::App) -> Option<String> {
        self.factory()?
            .editor
            .read(cx)
            .field(field)
            .map(|composer| composer.read(cx).text().to_string())
    }

    /// What holds the keyboard on the Workflows surface: the field
    /// chosen, else the outline.
    pub(super) fn workflows_focus(&self, cx: &gpui::App) -> Option<FocusHandle> {
        let view = self.factory()?;
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
