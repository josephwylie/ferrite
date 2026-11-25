//! The Factory's New task sheet in the window (ADR 0041, ADR 0042). The
//! sheet is `ferrite_factory::new_task::NewTaskView`, which the sheet host
//! holds while it is up. It reads the process's name through a `Source`
//! the root makes over itself (`new_task_inputs`), asking about its
//! Project and the process chosen; what it asks it says as a
//! `NewTaskEvent`. Opening it, keeping what was typed on its Factory once
//! it closes, and starting the Task, a Server call, stay the root's.

use super::factory::TaskTab;
use super::workflows::Surface;
use super::*;
use ferrite_core::factory_board::WorkflowId;
use ferrite_core::protocol::{Done, TaskAction};
use ferrite_core::ProjectKey;
use ferrite_factory::new_task::{Asked, KeptNewTask, NewTaskEvent, NewTaskInputs, NewTaskQuery, NewTaskView};
use ferrite_ui::source::Source;
use gpui::App;

impl CockpitView {
    /// What the sheet of `project`'s Factory draws, none once that Factory
    /// no longer shows. It never reads the sheet: the sheet calls it while
    /// its own update is running.
    fn new_task_inputs(&self, project: ProjectKey, workflow: Option<&str>, cx: &App) -> Option<NewTaskInputs> {
        if self.factory_project()? != project {
            return None;
        }
        let process = match workflow
            .and_then(|id| self.cockpit.read(cx).factory(project.machine()).reading().workflow(id))
            .and_then(|workflow| workflow.latest())
        {
            Some(latest) => format!("{} \u{b7} version {}", latest.definition.name, latest.version),
            None => "One-off: a single agent".into(),
        };
        Some(NewTaskInputs {
            process: process.into(),
        })
    }

    fn on_new_task_event(&mut self, _: Entity<NewTaskView>, event: &NewTaskEvent, cx: &mut Context<Self>) {
        match event.clone() {
            NewTaskEvent::Start => self.start_new_task(cx),
            NewTaskEvent::PickProcess(trigger, at) => {
                let picks = self.process_picks(cx);
                self.open_picks(picks, &trigger, at, cx);
            }
            NewTaskEvent::Closed => {
                self.hide_sheet(cx);
            }
        }
    }

    /// The New task sheet, while it is up.
    pub(crate) fn new_task(&self, cx: &App) -> Option<Entity<NewTaskView>> {
        self.sheets.read(cx).new_task().cloned()
    }

    /// The shown Factory's New task sheet, in place of any other sheet, as
    /// it was last closed.
    pub(crate) fn open_new_task(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.factory_project() else {
            return;
        };
        let kept = self
            .update_factory(cx, |view, _| view.kept.take())
            .flatten()
            .unwrap_or_else(KeptNewTask::new);
        let inputs = Source::of(&cx.entity(), |root: &Self, (project, workflow): &NewTaskQuery, cx| {
            root.new_task_inputs(*project, workflow.as_deref(), cx)
        });
        let sheet = cx.new(|cx| NewTaskView::new(inputs, project, kept, cx));
        cx.subscribe(&sheet, Self::on_new_task_event).detach();
        self.show_sheet(sheets::Sheet::NewTask(sheet), cx);
    }

    /// The process the New task sheet runs, while it is up.
    pub(crate) fn set_new_task_workflow(&mut self, workflow: Option<WorkflowId>, cx: &mut Context<Self>) {
        if let Some(sheet) = self.new_task(cx) {
            sheet.update(cx, |sheet, cx| sheet.set_workflow(workflow, cx));
        }
    }

    /// The sheet gone, closed or replaced: what was typed stays on its
    /// Factory, if that still shows.
    pub(super) fn new_task_gone(&mut self, sheet: &Entity<NewTaskView>, cx: &mut App) {
        let (project, kept) = {
            let sheet = sheet.read(cx);
            (sheet.project(), sheet.kept(cx))
        };
        if self.factory_project() == Some(project) {
            self.update_factory(cx, |view, _| view.kept = Some(kept));
        }
    }

    /// Start: the Task, its worktree and its Worker, in the Server. Its
    /// panel opens on it, and the next New task starts empty; a refusal
    /// stays on the sheet.
    pub(crate) fn start_new_task(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.new_task(cx) else {
            return;
        };
        let Asked {
            project,
            instructions,
            pr,
            request,
            workflow,
        } = sheet.read(cx).asked(cx);
        let error = if instructions.is_empty() {
            Some(SharedString::from("Say what the task is."))
        } else {
            let action = TaskAction::Start {
                project: project.project(),
                instructions,
                request: Some(request),
                workflow,
                pr,
            };
            match self
                .cockpit
                .update(cx, |cockpit, _| cockpit.task(project.machine(), action))
            {
                Ok(Some(Done::Task { task })) => {
                    self.hide_sheet(cx);
                    self.update_factory(cx, |view, _| {
                        view.kept = None;
                        view.surface = Surface::Board;
                        view.open = Some(task);
                        view.tab = TaskTab::Overview;
                        view.note = None;
                    });
                    None
                }
                Ok(_) => Some(SharedString::from("The Server did not start the task.")),
                Err(error) => Some(SharedString::from(error.to_string())),
            }
        };
        sheet.update(cx, |sheet, cx| sheet.refuse(error, cx));
        cx.notify();
    }
}
