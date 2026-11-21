//! The Factory's New task sheet as a view of its own (ADR 0041), made when
//! it opens and dropped once it has gone (ADR 0042). It owns its two
//! fields, which of them holds the keyboard, the process chosen, the
//! request that makes a second Start the same Task, and a refusal. It draws
//! the process's name from what the Cockpit gives it (`NewTaskInputs`),
//! asked for each frame, and what the operator asks of it it says as a
//! `NewTaskEvent`. Starting the Task is the Cockpit's (`start_new_task`).
//!
//! What was typed outlives the view: a close leaves it on the sheet's
//! Factory as a `KeptNewTask`, plain values that seed the next sheet.

use super::factory::TaskTab;
use super::workflows::{Surface, Trigger};
use super::*;
use ferrite_core::draft_content::DraftContent;
use ferrite_core::factory::WorkflowId;
use ferrite_core::protocol::{Done, TaskAction};
use ferrite_core::workspace::registry::ProjectId;
use ferrite_pane::composer::{Composer, Role};
use gpui::{App, WeakEntity};

static REQUESTS: AtomicU64 = AtomicU64::new(1);

/// The New task sheet as it was closed, kept on its Factory for when it
/// opens again. A refusal and which field held the keyboard are not kept:
/// the sheet reopens on its instructions with nothing refused.
pub(crate) struct KeptNewTask {
    instructions: DraftContent,
    pr: DraftContent,
    workflow: Option<WorkflowId>,
    request: String,
}

impl KeptNewTask {
    /// Nothing typed yet, and a request of its own.
    fn new() -> Self {
        Self {
            instructions: DraftContent::default(),
            pr: DraftContent::default(),
            workflow: None,
            request: format!(
                "window-{}-{}",
                std::process::id(),
                REQUESTS.fetch_add(1, Ordering::Relaxed)
            ),
        }
    }
}

pub(crate) struct NewTaskView {
    cockpit: WeakEntity<CockpitView>,
    /// The Project whose Factory opened it: the sheet is that Factory's.
    pub(super) project: ProjectId,
    pub(super) instructions: Entity<Composer>,
    /// Its optional pull request, `owner/repository#number` or a URL
    /// (#223).
    pub(super) pr: Entity<Composer>,
    /// The PR field holds the keyboard, else the instructions do.
    pr_focused: bool,
    /// The process it runs: a Workflow's latest version, else one-off.
    pub(super) workflow: Option<WorkflowId>,
    /// What a Start sends, so a second press is the same Task.
    request: String,
    pub(super) error: Option<SharedString>,
}

/// What the sheet draws from, as the Cockpit has it this frame.
struct NewTaskInputs {
    /// The process's name: the Workflow at its latest version, or one-off.
    process: SharedString,
}

/// What the sheet asks of the window.
#[derive(Clone)]
pub(crate) enum NewTaskEvent {
    /// Start pressed.
    Start,
    /// The Process picker pressed, at the pointer.
    PickProcess(Trigger, Point<Pixels>),
    /// The close button closed it.
    Closed,
}

impl EventEmitter<NewTaskEvent> for NewTaskView {}

impl NewTaskView {
    fn new(
        cockpit: WeakEntity<CockpitView>,
        project: ProjectId,
        kept: KeptNewTask,
        cx: &mut Context<Self>,
    ) -> Self {
        let field = |content: DraftContent, cx: &mut Context<Self>| {
            let field = cx.new(Composer::new);
            field.update(cx, |field, cx| {
                field.set_role(Role::Field(None), cx);
                field.load(content, cx);
            });
            field
        };
        Self {
            cockpit,
            project,
            instructions: field(kept.instructions, cx),
            pr: field(kept.pr, cx),
            pr_focused: false,
            workflow: kept.workflow,
            request: kept.request,
            error: None,
        }
    }

    fn kept(&self, cx: &App) -> KeptNewTask {
        KeptNewTask {
            instructions: self.instructions.read(cx).content(),
            pr: self.pr.read(cx).content(),
            workflow: self.workflow.clone(),
            request: self.request.clone(),
        }
    }

    /// The field that holds the keyboard while the sheet is up.
    pub(super) fn focus(&self, cx: &App) -> FocusHandle {
        let field = if self.pr_focused {
            &self.pr
        } else {
            &self.instructions
        };
        field.read(cx).focus_handle(cx)
    }

    fn focus_pr(&mut self, pr: bool, cx: &mut Context<Self>) {
        self.pr_focused = pr;
        cx.notify();
    }

    /// The card, for the sheet host.
    pub(super) fn card(&self, window: &Window, cx: &mut Context<Self>) -> Option<ferrite_ui::sheet::Drawn> {
        let (project, workflow) = (self.project, self.workflow.clone());
        let NewTaskInputs { process } = self
            .cockpit
            .read_with(cx, |cockpit, _| cockpit.new_task_inputs(project, workflow.as_deref()))
            .ok()??;
        let close = ferrite_ui::sheet::sheet_close("new-task-close", "Close New task", cx)
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(NewTaskEvent::Closed);
            }));
        let focused = self.instructions.read(cx).focus_handle(cx).is_focused(window);
        let pr_focused = self.pr.read(cx).focus_handle(cx).is_focused(window);
        let process_trigger = Trigger::default();
        let mut body = crate::project_editor::body()
            .child(
                div()
                    .id("new-task-instructions")
                    .debug_selector(|| "new-task-instructions".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| view.focus_pr(false, cx)),
                    )
                    .child(crate::project_editor::text_area(
                        "Instructions".into(),
                        self.instructions.clone(),
                        focused,
                        "new-task-instructions-focused",
                    )),
            )
            .child(
                div()
                    .id("new-task-pr")
                    .debug_selector(|| "new-task-pr".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| view.focus_pr(true, cx)),
                    )
                    .child(crate::project_editor::text_field(
                        "Pull request (optional): owner/repository#number or its URL".into(),
                        self.pr.clone(),
                        pr_focused,
                        "new-task-pr-focused",
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(ferrite_ui::theme::SPACE_1))
                    .child(ferrite_ui::components::section_label("Process"))
                    .child(
                        div()
                            .id("new-task-process")
                            .debug_selector(|| "new-task-process".into())
                            .flex()
                            .items_center()
                            .gap(px(ferrite_ui::theme::SPACE_1))
                            .h(px(ferrite_ui::theme::CONTROL_H))
                            .px(px(ferrite_ui::theme::SPACE_2))
                            .rounded(px(ferrite_ui::theme::R_CONTROL))
                            .border_1()
                            .border_color(ferrite_ui::theme::paint::FIELD_RING)
                            .cursor_pointer()
                            .child(div().flex_1().child(process))
                            .child(ferrite_ui::icons::icon(
                                ferrite_ui::icons::CHEVRON_DOWN,
                                ferrite_ui::theme::ROW_ICON,
                                ferrite_ui::theme::INK_SUBTLE,
                            ))
                            .relative()
                            .child(process_trigger.probe())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    cx.emit(NewTaskEvent::PickProcess(
                                        process_trigger.clone(),
                                        event.position,
                                    ));
                                }),
                            ),
                    ),
            );
        if let Some(error) = self.error.clone() {
            body = body.child(crate::project_editor::error_line(error));
        }
        let start = crate::project_editor::primary_button("new-task-start", "Start", false, cx)
            .debug_selector(|| "new-task-start".into())
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(NewTaskEvent::Start);
            }));
        let card = crate::project_editor::card(f32::from(window.viewport_size().height))
            .id("new-task-card")
            .debug_selector(|| "new-task-card".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(crate::project_editor::head("New task".into(), close))
            .child(body)
            .child(crate::project_editor::footer(
                div().child(
                    ferrite_ui::components::text_ui()
                        .text_color(gpui::rgb(ferrite_ui::theme::INK_SUBTLE))
                        .child("Runs in its own worktree and branch."),
                ),
                div().flex().items_center().child(start),
            ));
        Some(ferrite_ui::sheet::Drawn { card, above: None })
    }
}

impl CockpitView {
    /// What the sheet of `project`'s Factory draws, none once that Factory
    /// no longer shows. It never reads the sheet: the sheet calls it while
    /// its own update is running.
    fn new_task_inputs(&self, project: ProjectId, workflow: Option<&str>) -> Option<NewTaskInputs> {
        if self.factory_project()? != project {
            return None;
        }
        let process = match workflow
            .and_then(|id| self.cockpit.factory().workflow(id))
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
        let cockpit = cx.weak_entity();
        let sheet = cx.new(|cx| NewTaskView::new(cockpit, project, kept, cx));
        cx.subscribe(&sheet, Self::on_new_task_event).detach();
        self.show_sheet(sheets::Sheet::NewTask(sheet), cx);
    }

    /// The process the New task sheet runs, while it is up.
    pub(crate) fn set_new_task_workflow(&mut self, workflow: Option<WorkflowId>, cx: &mut Context<Self>) {
        if let Some(sheet) = self.new_task(cx) {
            sheet.update(cx, |sheet, cx| {
                sheet.workflow = workflow;
                cx.notify();
            });
        }
    }

    /// The sheet gone, closed or replaced: what was typed stays on its
    /// Factory, if that still shows.
    pub(super) fn new_task_gone(&mut self, sheet: &Entity<NewTaskView>, cx: &mut App) {
        let (project, kept) = {
            let sheet = sheet.read(cx);
            (sheet.project, sheet.kept(cx))
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
        let (project, instructions, pr, request, workflow) = {
            let sheet = sheet.read(cx);
            (
                sheet.project,
                sheet.instructions.read(cx).text().trim().to_string(),
                Some(sheet.pr.read(cx).text().trim().to_string()).filter(|pr| !pr.is_empty()),
                sheet.request.clone(),
                sheet.workflow.clone(),
            )
        };
        let error = if instructions.is_empty() {
            Some(SharedString::from("Say what the task is."))
        } else {
            let action = TaskAction::Start {
                project,
                instructions,
                request: Some(request),
                workflow,
                pr,
            };
            match self.cockpit.task(action) {
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
        sheet.update(cx, |sheet, cx| {
            sheet.error = error;
            cx.notify();
        });
        cx.notify();
    }
}
