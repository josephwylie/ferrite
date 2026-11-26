//! The Workflows editor in the window. Its view is
//! `ferrite_factory::workflows::WorkflowsView`, which the Factory holds; it
//! reads the shown Workflow through a `Source` the root makes over itself
//! (`workflows_inputs`), asked for what the editor chose, and says what is
//! typed or pressed as a `WorkflowsEvent`. Every edit is a Server ask, so
//! the root answers each here.

use super::*;
use ferrite_factory::workflows::WorkflowsQuery;
use ferrite_ui::source::Source;

impl CockpitView {
    /// A Factory's Workflows editor, heard from its making.
    pub(in crate::cockpit) fn new_workflows(&mut self, cx: &mut Context<Self>) -> Entity<WorkflowsView> {
        let inputs = Source::of(&cx.entity(), |root: &Self, (workflow, step): &WorkflowsQuery, cx| {
            root.workflows_inputs(workflow.as_deref(), step.as_deref(), cx)
        });
        let editor = cx.new(|cx| WorkflowsView::new(inputs, cx));
        cx.subscribe(&editor, Self::on_workflows_event).detach();
        editor
    }

    /// What the editor draws, for the Workflow and Step it chose: none
    /// while the Project has no Workflow.
    fn workflows_inputs(
        &self,
        chosen: Option<&str>,
        step: Option<&str>,
        cx: &gpui::App,
    ) -> Option<WorkflowsInputs> {
        let workflow = self.chosen_workflow(chosen, cx)?;
        let view = self.factory()?.read(cx);
        let step = ferrite_factory::workflows::shown_step(&workflow.draft, step);
        let provider = step.map(|step| step.agent.provider);
        let catalog = self.factory_catalog(cx);
        Some(WorkflowsInputs {
            workflow: workflow.id.clone(),
            title: workflow_title(workflow),
            saved: workflow.latest().map(|latest| latest.version),
            unsaved: workflow.unsaved(),
            step: step.map(|step| step.id.clone()),
            switch: workflow.automation.switch,
            problems: workflow.draft.problems(&catalog),
            catalog,
            draft: workflow.draft.clone(),
            models: Provider::ALL
                .into_iter()
                .map(|provider| {
                    let catalog = self
                        .factory_machine()
                        .map(|machine| self.cockpit.read(cx).model_catalog(machine, provider))
                        .unwrap_or_default();
                    (provider, catalog)
                })
                .collect(),
            installed: provider
                .and_then(|provider| view.skills().installed(provider))
                .map(<[_]>::to_vec),
            skills_note: provider
                .map(|provider| view.skills().note(provider))
                .unwrap_or_default(),
            starts: view.starts().clone(),
        })
    }

    /// A field typed in goes to the draft; one pressed has the focus pump
    /// hand it the keyboard; a row, a problem or a key chooses what the
    /// inspector shows; a picker opens its rows; every edit and Save ask
    /// the Server; Run task opens the New task sheet.
    fn on_workflows_event(
        &mut self,
        _: Entity<WorkflowsView>,
        event: &WorkflowsEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            WorkflowsEvent::Edited(field) => self.field_edited(field, cx),
            WorkflowsEvent::Focused => cx.notify(),
            WorkflowsEvent::Select(step) => self.select_step(step.clone(), cx),
            WorkflowsEvent::Approval => self.select_approval(cx),
            WorkflowsEvent::Start => self.open_start(cx),
            WorkflowsEvent::Move { step, to } => self.move_step(step.clone(), *to, cx),
            WorkflowsEvent::Drop { drag, onto } => self.drop_step(drag, *onto, cx),
            WorkflowsEvent::Route {
                step,
                kind,
                trigger,
                at,
            } => {
                let Some(step) = self.shown_step_of(step, cx) else {
                    return;
                };
                let picks = self.route_picks(&step, *kind, cx);
                self.open_picks(picks, trigger, *at, cx);
            }
            WorkflowsEvent::RowMenu { step, trigger, at } => {
                let Some(step) = self.shown_step_of(step, cx) else {
                    return;
                };
                let picks = self.row_picks(&step, cx);
                self.open_picks(picks, trigger, *at, cx);
            }
            WorkflowsEvent::Add => self.add_step(None, cx),
            WorkflowsEvent::Picks { picks, trigger, at } => {
                self.open_picks(picks.clone(), trigger, *at, cx)
            }
            WorkflowsEvent::Pick(pick) => self.apply_pick(pick.clone(), cx),
            WorkflowsEvent::EditStep { step, change } => {
                self.edit_step(step.clone(), change.clone(), cx)
            }
            WorkflowsEvent::AddSkill(name) => self.add_skill(name.clone(), cx),
            WorkflowsEvent::RemoveSkill(name) => self.remove_skill(name, cx),
            WorkflowsEvent::Delete(step) => self.delete_step(step.clone(), cx),
            WorkflowsEvent::Workflows { trigger, at } => {
                let picks = self.workflow_picks(cx);
                self.open_picks(picks, trigger, *at, cx)
            }
            WorkflowsEvent::Run => {
                let workflow = self.shown_id(cx);
                self.open_new_task(cx);
                self.set_new_task_workflow(workflow, cx);
            }
            WorkflowsEvent::Save => self.save_workflow(cx),
        }
    }
}
