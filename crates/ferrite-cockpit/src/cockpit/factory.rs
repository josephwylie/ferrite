//! A Project's Factory in the window (ADR 0037, #220): its sidebar entry,
//! its board of Tasks in the board's place, a Task's panel beside the
//! columns, the New task sheet, and the alerts a Task raises. Its
//! Workflows surface is `workflows.rs` (#221).
//!
//! The Factory runs in the Server (`ferrite_core::factory`): the window
//! draws the board the Server publishes and sends commands through
//! `Cockpit::task`. Opening a Task's panel starts nothing; its Worker
//! tab draws the Worker's ordinary Pane, which this window already holds.

use super::*;
use ferrite_core::factory::{self as core_factory, Status, Task, TaskId};
use ferrite_core::protocol::{Done, TaskAction};
use ferrite_core::workspace::registry::ProjectId;
use ferrite_pane::composer::{Composer, Role};

/// The Task panel's width beside the columns.
const TASK_PANEL_W: f32 = 480.;
/// A column's width; the board scrolls sideways when they do not fit.
const COLUMN_W: f32 = 232.;

/// A Task panel's tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskTab {
    Overview,
    Activity,
    Context,
    Worker,
}

impl TaskTab {
    const ALL: [TaskTab; 4] = [
        TaskTab::Overview,
        TaskTab::Activity,
        TaskTab::Context,
        TaskTab::Worker,
    ];

    fn word(self) -> &'static str {
        match self {
            TaskTab::Overview => "Overview",
            TaskTab::Activity => "Activity",
            TaskTab::Context => "Context",
            TaskTab::Worker => "Worker",
        }
    }

    fn key(self) -> &'static str {
        match self {
            TaskTab::Overview => "overview",
            TaskTab::Activity => "activity",
            TaskTab::Context => "context",
            TaskTab::Worker => "worker",
        }
    }
}

/// The New task sheet: the instructions field, and the request that makes
/// a second press of Start the same Task.
pub(crate) struct NewTask {
    pub(crate) instructions: Entity<Composer>,
    request: String,
    error: Option<SharedString>,
    /// The process it runs: a Workflow's latest version, else one-off.
    pub(crate) workflow: Option<core_factory::WorkflowId>,
    /// Its optional pull request, `owner/repository#number` or a URL
    /// (#223), and whether that field has the keyboard.
    pub(crate) pr: Entity<Composer>,
    pub(crate) pr_focused: bool,
}

/// What the Factory view shows.
pub(crate) struct FactoryView {
    pub(crate) project: ProjectId,
    /// The Pane focused when the board opened: focus moving to another of
    /// the operator's own leaves the board.
    pub(crate) focus_at_open: Option<PaneIdentity>,
    /// The Task whose panel is open.
    pub(crate) open: Option<TaskId>,
    pub(crate) tab: TaskTab,
    /// What the last command said back (a refusal, a worktree kept).
    pub(crate) note: Option<SharedString>,
    pub(crate) sheet: Option<NewTask>,
    /// The Board or the Workflows.
    pub(crate) surface: super::workflows::Surface,
    pub(crate) workflows: super::workflows::Workflows,
    /// The open Factory picker's rows.
    pub(crate) picks: Vec<(ferrite_ui::menu::Item, super::workflows::Pick)>,
    /// Answers, rework and redirects being written, and their sheets.
    pub(crate) direct: super::direction::Direct,
    /// The Coordinator surface's field before the Coordinator runs (#224).
    pub(crate) opening: Option<Entity<Composer>>,
    /// The Archive, when it shows in the board's place (#226).
    pub(crate) archive: super::archive::Archive,
}

/// A system notification's tag for a Task.
pub(crate) fn task_tag(task: &str) -> String {
    format!("ferrite-task-{task}")
}

/// The ink a status is said in.
fn status_ink(status: Status) -> u32 {
    use ferrite_ui::theme::*;
    match status {
        Status::Working => RUNNING,
        Status::NeedsHuman | Status::Approval => ATTENTION,
        Status::Stopped => BLOCKED,
        Status::Queued | Status::Completed => INK_SUBTLE,
    }
}

/// What a card says under its title: the result, the stop reason.
fn outcome_line(task: &Task) -> Option<String> {
    match task.status {
        Status::Completed | Status::Approval => task.result.clone(),
        Status::NeedsHuman | Status::Stopped => task.reason.clone(),
        _ => None,
    }
}

static REQUESTS: AtomicU64 = AtomicU64::new(1);

impl CockpitView {
    /// Show `project`'s Factory board in the board's place.
    pub(crate) fn open_factory(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        self.empty_shown = false;
        let focus_at_open = self.cockpit.roster().focused();
        match self.factory_view.as_mut() {
            Some(view) if view.project == project => view.focus_at_open = focus_at_open,
            _ => {
                let workflows = super::workflows::Workflows::new(cx);
                self.factory_view = Some(FactoryView {
                    project,
                    focus_at_open,
                    open: None,
                    tab: TaskTab::Overview,
                    note: None,
                    sheet: None,
                    surface: super::workflows::Surface::Board,
                    workflows,
                    picks: Vec::new(),
                    direct: Default::default(),
                    opening: None,
                    archive: Default::default(),
                })
            }
        }
        cx.notify();
    }

    /// Open a Task's panel: what it shows is the board's; nothing starts.
    pub(crate) fn open_task(&mut self, task: &str, cx: &mut Context<Self>) {
        let Some(project) = self.cockpit.factory().task(task).map(|task| task.project) else {
            return;
        };
        if self.factory_view.as_ref().map(|view| view.project) != Some(project) {
            self.open_factory(project, cx);
        }
        self.read_worker(None, cx);
        if let Some(view) = self.factory_view.as_mut() {
            view.open = Some(task.to_string());
            view.tab = TaskTab::Overview;
            view.note = None;
            view.direct.focused = None;
        }
        self.ensure_drafts(task, cx);
        cx.notify();
    }

    pub(crate) fn task_tab(&mut self, tab: TaskTab, cx: &mut Context<Self>) {
        let Some(view) = self.factory_view.as_mut() else {
            return;
        };
        view.tab = tab;
        // The Worker's Pane takes the keyboard, as any Pane the operator
        // looks at: what is typed there is said to the Worker.
        if tab == TaskTab::Worker {
            if let Some(index) = self.open_worker_pane() {
                self.focus_pane(index);
            }
        }
        cx.notify();
    }

    /// The open Task, as the board says it now.
    pub(super) fn open_task_now(&self) -> Option<&Task> {
        let id = self.factory_view.as_ref()?.open.as_deref()?;
        self.cockpit.factory().task(id)
    }

    /// The open Task's Worker's Pane in this window.
    fn open_worker_pane(&self) -> Option<usize> {
        self.pane_for_worker(self.open_task_now()?)
    }

    /// The Worker drawn in the open panel's Worker tab, which streams whole.
    pub(super) fn factory_worker_drawn(&self) -> Option<ThreadId> {
        let view = self.factory_view.as_ref()?;
        if let Some(coordinator) = self.coordinator_drawn() {
            return Some(coordinator);
        }
        let task = self.open_task_now()?;
        if view.tab != TaskTab::Worker {
            return None;
        }
        if let Some(reading) = self.worker_reading(task) {
            return Some(reading);
        }
        task.worker.filter(|_| self.pane_for_worker(task).is_some())
    }

    /// Focus moved to another of the operator's own Panes (a new draft, a
    /// landing): the board gives way to it.
    pub(super) fn leave_factory_if_moved(&mut self) {
        let Some(view) = self.factory_view.as_ref() else {
            return;
        };
        let now = self.cockpit.roster().focused();
        if now == view.focus_at_open {
            return;
        }
        // Read through the identity: an owned Thread just reopened may
        // have no Pane in this window yet.
        let owned = now
            .and_then(PaneIdentity::thread)
            .is_some_and(|thread| !self.cockpit.operators(thread));
        if owned {
            return;
        }
        self.factory_view = None;
    }

    /// The Tasks changed: the open panel's Task may be gone.
    pub(super) fn factory_changed(&mut self, cx: &mut Context<Self>) {
        let gone = self
            .factory_view
            .as_ref()
            .and_then(|view| view.open.as_deref())
            .is_some_and(|id| self.cockpit.factory().task(id).is_none());
        if gone {
            if let Some(view) = self.factory_view.as_mut() {
                view.open = None;
            }
        }
        if let Some(open) = self.open_task_now().map(|task| task.id.clone()) {
            self.ensure_drafts(&open, cx);
        }
        self.ensure_fields(cx);
        cx.notify();
    }

    /// What holds the keyboard while the Factory shows: the sheet's field,
    /// the Worker's Composer on its tab, else the window's own.
    pub(super) fn factory_focus(&self, window: &Window, cx: &gpui::App) -> Option<FocusHandle> {
        let view = self.factory_view.as_ref()?;
        if let Some(sheet) = &view.sheet {
            let field = if sheet.pr_focused {
                &sheet.pr
            } else {
                &sheet.instructions
            };
            return Some(field.read(cx).focus_handle(cx));
        }
        if let Some(focus) = self.direct_focus(cx) {
            return Some(focus);
        }
        if let Some(focus) = self.coordinator_focus(window, cx) {
            return Some(focus);
        }
        if let Some(focus) = self.archive_focus(cx) {
            return Some(focus);
        }
        if let Some(focus) = self.workflows_focus(cx) {
            return Some(focus);
        }
        if view.tab == TaskTab::Worker {
            if let Some(index) = self.open_worker_pane() {
                return Some(self.panes[index].composer.read(cx).focus_target(window, cx));
            }
        }
        Some(self.focus.clone())
    }

    /// The Factory command a press makes, its answer in the view's note.
    pub(crate) fn task_command(&mut self, action: TaskAction, cx: &mut Context<Self>) {
        let note = match self.cockpit.task(action) {
            Ok(Some(Done::Deleted { kept })) => {
                if let Some(view) = self.factory_view.as_mut() {
                    view.open = None;
                }
                kept.map(SharedString::from)
            }
            Ok(_) => None,
            Err(error) => Some(SharedString::from(error.to_string())),
        };
        if let Some(view) = self.factory_view.as_mut() {
            view.note = note;
        }
        cx.notify();
    }

    pub(crate) fn open_new_task(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.factory_view.as_mut() else {
            return;
        };
        let instructions = cx.new(Composer::new);
        instructions.update(cx, |field, cx| field.set_role(Role::Field(None), cx));
        let pr = cx.new(Composer::new);
        pr.update(cx, |field, cx| field.set_role(Role::Field(None), cx));
        view.sheet = Some(NewTask {
            pr,
            pr_focused: false,
            instructions,
            request: format!(
                "window-{}-{}",
                std::process::id(),
                REQUESTS.fetch_add(1, Ordering::Relaxed)
            ),
            error: None,
            workflow: None,
        });
        cx.notify();
    }

    /// The New task sheet's keyboard goes to its PR field, or back to
    /// the instructions.
    pub(crate) fn focus_new_task_pr(&mut self, pr: bool, cx: &mut Context<Self>) {
        if let Some(sheet) = self
            .factory_view
            .as_mut()
            .and_then(|view| view.sheet.as_mut())
        {
            sheet.pr_focused = pr;
        }
        cx.notify();
    }

    pub(crate) fn close_new_task(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.factory_view.as_mut() {
            view.sheet = None;
        }
        cx.notify();
    }

    /// Start: the Task, its worktree and its Worker, in the Server. Its
    /// panel opens on it; a refusal stays on the sheet.
    pub(crate) fn start_new_task(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.factory_view.as_ref() else {
            return;
        };
        let project = view.project;
        let Some(sheet) = view.sheet.as_ref() else {
            return;
        };
        let instructions = sheet.instructions.read(cx).text().trim().to_string();
        let pr = Some(sheet.pr.read(cx).text().trim().to_string()).filter(|pr| !pr.is_empty());
        let error = if instructions.is_empty() {
            Some(SharedString::from("Say what the task is."))
        } else {
            let action = TaskAction::Start {
                project,
                instructions,
                request: Some(sheet.request.clone()),
                workflow: sheet.workflow.clone(),
                pr,
            };
            match self.cockpit.task(action) {
                Ok(Some(Done::Task { task })) => {
                    if let Some(view) = self.factory_view.as_mut() {
                        view.sheet = None;
                        view.surface = super::workflows::Surface::Board;
                        view.open = Some(task);
                        view.tab = TaskTab::Overview;
                        view.note = None;
                    }
                    None
                }
                Ok(_) => Some(SharedString::from("The Server did not start the task.")),
                Err(error) => Some(SharedString::from(error.to_string())),
            }
        };
        if let Some(sheet) = self
            .factory_view
            .as_mut()
            .and_then(|view| view.sheet.as_mut())
        {
            sheet.error = error;
        }
        cx.notify();
    }

    /// A Task needs the operator, stopped or completed: a toast, a chime
    /// and a system notification, and nothing in the Inbox.
    pub(super) fn present_task_alert(
        &mut self,
        project: ProjectId,
        task: String,
        status: Status,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use gpui::component::notification::{Notification, NotificationType};
        use gpui::component::WindowExt as _;
        let found = self.cockpit.factory().task(&task).cloned();
        let detail = found
            .as_ref()
            .and_then(outcome_line)
            .unwrap_or_else(|| title.clone());
        let project_title = self
            .cockpit
            .registry()
            .project(project)
            .map(|project| project.title.clone());
        let heading = format!("{} \u{b7} {title}", status.word());
        let mut toast = Notification::new()
            .title(heading)
            .message(detail.clone())
            .with_type(if status.needs_you() {
                NotificationType::Warning
            } else {
                NotificationType::Info
            })
            .autohide(!status.needs_you());
        if let Some(project_title) = &project_title {
            toast = toast.message(format!("{project_title} \u{b7} {detail}"));
        }
        window.push_notification(toast, cx);
        let provider = found.as_ref().map(|task| task.agent.provider);
        if provider.map_or(self.prefs.settings.sounds, |provider| {
            self.prefs.settings.sounds_for(provider)
        }) {
            self.chime(if status.needs_you() {
                Sound::NeedsYou
            } else {
                Sound::Done
            });
        }
        if self.prefs.settings.system_notifications
            && crate::system_notifications::wanted(false, self.cockpit.window_active())
        {
            crate::system_notifications::post(
                crate::system_notifications::Posting {
                    tag: task_tag(&task),
                    needs_you: status.needs_you(),
                    word: status.word(),
                    title,
                    detail,
                    agent: provider.map(|provider| provider.to_string()),
                    cwd: found.and_then(|task| task.worktree),
                },
                cx,
            );
        }
    }

    /// A Task's system notification, clicked: its panel opens.
    pub(super) fn open_task_notification(&mut self, tag: &str, cx: &mut Context<Self>) -> bool {
        let Some(task) = tag.strip_prefix("ferrite-task-") else {
            return false;
        };
        let task = task.to_string();
        self.open_task(&task, cx);
        true
    }

    /// A Project's Factory entry in the sidebar.
    pub(super) fn factory_nav_row(&self, project: ProjectId, cx: &mut Context<Self>) -> AnyElement {
        let count = self.cockpit.factory().needs_you(project);
        let active = self
            .factory_view
            .as_ref()
            .is_some_and(|view| view.project == project);
        nav::factory_row(project, count, active)
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.open_factory(project, cx);
            }))
            .into_any_element()
    }

    /// The Factory board: its head, its six columns, and the open Task's
    /// panel beside them.
    pub(super) fn factory_board(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let Some(view) = self.factory_view.as_ref() else {
            return div();
        };
        let project = view.project;
        let board = self.cockpit.factory();
        let tasks: Vec<&Task> = board.on_board(project).collect();
        let archived = board.archive(project, "").count();
        let title = self
            .cockpit
            .registry()
            .project(project)
            .map(|project| project.title.clone())
            .unwrap_or_else(|| "Project".into());
        let count = match tasks.len() {
            0 => "No tasks".to_string(),
            1 => "1 task".to_string(),
            n => format!("{n} tasks"),
        };
        let head = div()
            .debug_selector(|| "factory-head".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_3))
            .h(px(CONTROL_H + SPACE_4))
            .px(px(SPACE_4))
            // The title gives way first, so the head's buttons stay in a
            // narrow board: text never shrinks below its width otherwise.
            .child(
                div()
                    .debug_selector(|| "factory-title".into())
                    .min_w_0()
                    .truncate()
                    .font_weight(W_LABEL)
                    .text_color(rgb(INK_STRONG))
                    .child(SharedString::from(format!("{title} \u{b7} Factory"))),
            )
            .child(
                div()
                    .debug_selector(|| "factory-count".into())
                    .flex_shrink_0()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(count)),
            )
            .child(self.factory_surface_tabs(cx))
            .child(div().flex_1())
            .children(view.note.clone().map(|note| {
                div()
                    .id("factory-note")
                    .debug_selector(|| "factory-note".into())
                    .flex()
                    .items_center()
                    .gap(px(SPACE_1))
                    .min_w_0()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(div().min_w_0().truncate().child(note))
                    .child(
                        div()
                            .id("factory-note-dismiss")
                            .debug_selector(|| "factory-note-dismiss".into())
                            .flex_shrink_0()
                            .cursor_pointer()
                            .child(ferrite_ui::icons::icon(
                                ferrite_ui::icons::CLOSE,
                                ROW_ICON,
                                INK_SUBTLE,
                            ))
                            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                                view.dismiss_factory_note(cx)
                            })),
                    )
            }))
            .when(
                view.surface == super::workflows::Surface::Board && !view.archive.shown,
                |head| head.child(self.archive_button(archived, cx)),
            )
            .child(
                ferrite_ui::components::secondary_button(
                    "factory-new-task",
                    Some(ferrite_ui::icons::PLUS),
                    "New task",
                    cx,
                )
                .debug_selector(|| "factory-new-task".into())
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_new_task(cx);
                })),
            );

        let body = if view.surface == super::workflows::Surface::Workflows {
            self.workflows_surface(window, cx)
        } else if view.surface == super::workflows::Surface::Coordinator {
            self.coordinator_surface(window, cx)
        } else if view.archive.shown {
            self.archive_surface(window, cx)
        } else if tasks.is_empty() {
            div()
                .debug_selector(|| "factory-empty".into())
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(SPACE_2))
                .child(ferrite_ui::icons::icon(
                    ferrite_ui::icons::BOARD,
                    SPACE_6,
                    INK_SUBTLE,
                ))
                .child(
                    div()
                        .font_weight(W_LABEL)
                        .text_color(rgb(INK_STRONG))
                        .child("No tasks yet"),
                )
                .child(
                    div()
                        .text_size(px(FS_META))
                        .text_color(rgb(INK_SUBTLE))
                        .child("Start one with New task. Nothing runs until you do."),
                )
        } else {
            let open = view.open.as_deref();
            let mut columns = div()
                .id("factory-columns")
                .debug_selector(|| "factory-columns".into())
                .flex_1()
                .min_w_0()
                .min_h_0()
                .flex()
                .flex_row()
                .gap(px(SPACE_3))
                .px(px(SPACE_4))
                .pb(px(SPACE_4))
                .overflow_x_scroll();
            for status in Status::ALL {
                let in_column: Vec<&&Task> =
                    tasks.iter().filter(|task| task.status == status).collect();
                let key = status_key(status);
                let mut column = div()
                    .id(SharedString::from(format!("factory-column-{key}")))
                    .debug_selector({
                        let key = key.clone();
                        move || format!("factory-column-{key}")
                    })
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .w(px(COLUMN_W))
                    .min_h_0()
                    .gap(px(SPACE_2))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(SPACE_2))
                            // As tall as Completed's Clear, so the column's
                            // scroll never clips its top.
                            .h(px(CONTROL_H))
                            .flex_shrink_0()
                            .child(ferrite_ui::components::status_dot(status_ink(status)))
                            .child(
                                div()
                                    .font_weight(W_LABEL)
                                    .text_size(px(FS_LABEL))
                                    .text_color(rgb(INK_STRONG))
                                    .child(status.column()),
                            )
                            .child(
                                div()
                                    .text_size(px(FS_META))
                                    .text_color(rgb(INK_SUBTLE))
                                    .child(SharedString::from(in_column.len().to_string())),
                            )
                            .when(
                                status == Status::Completed && !in_column.is_empty(),
                                |heading| {
                                    heading.child(div().flex_1()).child(self.clear_button(cx))
                                },
                            ),
                    );
                if in_column.is_empty() {
                    column = column.child(
                        div()
                            .text_size(px(FS_META))
                            .text_color(rgb(INK_SUBTLE))
                            .child("None"),
                    );
                }
                for task in in_column {
                    column = column.child(self.task_card(task, open == Some(task.id.as_str()), cx));
                }
                columns = columns.child(column);
            }
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_row()
                .child(columns)
                .children(self.task_panel(window, cx))
        };

        div()
            .debug_selector(|| "factory-board".into())
            .flex_1()
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .rounded(px(R_CARD))
            .bg(paint::CARD)
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .text_color(rgb(INK_STRONG))
            .child(head)
            .children(self.factory_trouble())
            .child(body)
    }

    /// One Task's card: its title, its Worker's Provider and model, and
    /// its result or stop reason.
    fn task_card(&self, task: &Task, open: bool, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let id = task.id.clone();
        let key = format!("factory-card-{}", task.id);
        let agent_key = format!("{key}-agent");
        let outcome_key = format!("{key}-outcome");
        let card = div()
            .id(SharedString::from(key.clone()))
            .debug_selector(move || key.clone())
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(SPACE_1))
            .p(px(SPACE_3))
            .rounded(px(R_CONTROL))
            .border_1()
            .border_color(paint::CARD_RING)
            .cursor_pointer()
            .map(|card| {
                if open {
                    card.bg(paint::SELECTED)
                } else {
                    card.bg(paint::RAISED)
                        .hover(|card| card.bg(paint::SELECTED))
                }
            })
            .child(
                div()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(format!("#{}", task.id))),
            )
            .child(
                div()
                    .text_color(rgb(INK_STRONG))
                    .line_clamp(2)
                    .child(SharedString::from(task.title.clone())),
            )
            .child(
                div()
                    .debug_selector(move || agent_key.clone())
                    .flex()
                    .items_center()
                    .gap(px(SPACE_1))
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child({
                        let (mark, ink) = ferrite_ui::provider::mark(task.agent.provider);
                        ferrite_ui::icons::icon(mark, BAR_MARK, ink)
                    })
                    .child(SharedString::from(match task.current_step() {
                        Some(step) => format!(
                            "{}{} \u{b7} {}",
                            step.name,
                            if task.oversight.deciding {
                                " \u{b7} approval decision"
                            } else {
                                ""
                            },
                            core_factory::agent_label(&task.agent)
                        ),
                        None => core_factory::agent_label(&task.agent),
                    })),
            )
            .children(outcome_line(task).map(|line| {
                div()
                    .debug_selector(move || outcome_key.clone())
                    .text_size(px(FS_META))
                    .text_color(rgb(if task.status == Status::Stopped {
                        BLOCKED
                    } else {
                        INK_STRONG
                    }))
                    .line_clamp(3)
                    .child(SharedString::from(line))
            }))
            .children(super::delivery::card_line(task).map(|line| {
                let pr_key = format!("factory-card-{}-pr", task.id);
                div()
                    .debug_selector(move || pr_key.clone())
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(line))
            }))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                let shut = view
                    .factory_view
                    .as_ref()
                    .is_some_and(|shown| shown.open.as_deref() == Some(id.as_str()));
                if shut {
                    if let Some(shown) = view.factory_view.as_mut() {
                        shown.open = None;
                    }
                    cx.notify();
                } else {
                    view.open_task(&id, cx);
                }
            }));
        card.into_any_element()
    }

    /// The open Task's panel: its status, tabs, and the tab's content.
    pub(super) fn task_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use ferrite_ui::theme::*;
        let view = self.factory_view.as_ref()?;
        let task = self.open_task_now()?;
        let tab = view.tab;
        let id = task.id.clone();

        let mut actions = div().flex().items_center().gap(px(SPACE_2));
        // Archived, a Task is read-only (#226).
        let archived = task.archived();
        if archived {
            actions = actions.child(
                div()
                    .debug_selector(|| "task-archived".into())
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(match task.keeping.archived_ms {
                        Some(at) => format!("Archived {}", core_factory::when_label(at)),
                        None => "Archived".to_string(),
                    })),
            );
        } else if task.status.stoppable() {
            let task = id.clone();
            actions = actions.child(
                ferrite_ui::components::secondary_button("task-stop", None, "Stop", cx)
                    .debug_selector(|| "task-stop".into())
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.task_command(TaskAction::Stop { task: task.clone() }, cx)
                    })),
            );
        }
        if task.status == Status::Stopped && !archived {
            let task = id.clone();
            actions = actions.child(
                ferrite_ui::components::secondary_button("task-resume", None, "Resume", cx)
                    .debug_selector(|| "task-resume".into())
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.task_command(TaskAction::Resume { task: task.clone() }, cx)
                    })),
            );
        }
        if task.status.deletable() && !archived {
            let task = id.clone();
            actions = actions.child(
                crate::project_editor::destructive_button("task-delete", "Delete", false, cx)
                    .debug_selector(|| "task-delete".into())
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.task_command(TaskAction::Delete { task: task.clone() }, cx)
                    })),
            );
        }
        let close = ferrite_ui::components::ghost_button("task-close", "Close", cx).on_click(
            cx.listener(|view, _: &ClickEvent, _, cx| {
                if let Some(view) = view.factory_view.as_mut() {
                    view.open = None;
                }
                cx.notify();
            }),
        );
        let head = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(SPACE_2))
            .p(px(SPACE_4))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_weight(W_LABEL)
                            .truncate()
                            .child(SharedString::from(task.title.clone())),
                    )
                    .child(close),
            )
            .child(
                div()
                    .debug_selector(|| "task-status".into())
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(ferrite_ui::components::status_dot(status_ink(task.status)))
                    .child(task.status.word())
                    .child("\u{b7}")
                    .child(SharedString::from(core_factory::agent_label(&task.agent)))
                    .when(task.oversight.deciding, |line| {
                        line.child("\u{b7} approval decision")
                    }),
            )
            .child(actions);

        let mut tabs = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1))
            .px(px(SPACE_4))
            .border_b_1()
            .border_color(paint::LINE);
        for each in TaskTab::ALL {
            let key = format!("task-tab-{}", each.key());
            let selected = each == tab;
            tabs = tabs.child(
                div()
                    .id(SharedString::from(key.clone()))
                    .debug_selector(move || key.clone())
                    .px(px(SPACE_2))
                    .h(px(CONTROL_H))
                    .flex()
                    .items_center()
                    .rounded(px(R_CONTROL))
                    .cursor_pointer()
                    .text_size(px(FS_LABEL))
                    .font_weight(if selected { W_LABEL } else { W_BODY })
                    .text_color(rgb(if selected { INK_STRONG } else { INK_SUBTLE }))
                    .when(selected, |tab| tab.bg(paint::RAISED))
                    .when(!selected, |tab| tab.hover(|tab| tab.bg(paint::HOVER)))
                    .child(each.word())
                    .on_click(
                        cx.listener(move |view, _: &ClickEvent, _, cx| view.task_tab(each, cx)),
                    ),
            );
        }

        let content: AnyElement = match tab {
            TaskTab::Overview => {
                let line = |label: &'static str, text: String| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(SPACE_0_5))
                        .child(
                            div()
                                .text_size(px(FS_META))
                                .text_color(rgb(INK_SUBTLE))
                                .child(label),
                        )
                        .child(div().child(SharedString::from(text)))
                };
                div()
                    .id("task-overview")
                    .debug_selector(|| "task-overview".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_3))
                    .p(px(SPACE_4))
                    .children(
                        task.result
                            .clone()
                            .filter(|_| task.status != Status::Approval)
                            .map(|result| line("Result", result)),
                    )
                    .children(
                        task.reason
                            .clone()
                            .filter(|_| {
                                task.oversight.question.is_none() && task.status != Status::Approval
                            })
                            .map(|reason| line("Why", reason)),
                    )
                    .child(line("Status", task.status.word().to_string()))
                    .children(self.task_say(task, window, cx))
                    .children(self.task_delivery(task, cx))
                    .into_any_element()
            }
            TaskTab::Activity => {
                let mut list = div()
                    .id("task-activity")
                    .debug_selector(|| "task-activity".into())
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_2))
                    .p(px(SPACE_4))
                    .overflow_y_scroll();
                for (at, entry) in task.activity.iter().enumerate() {
                    let who = entry.by.word();
                    let text = match entry.revision {
                        Some(revision) => format!("{} \u{b7} revision {revision}", entry.text),
                        None => entry.text.clone(),
                    };
                    list = list.child(
                        div()
                            .debug_selector(move || format!("task-activity-{at}"))
                            .flex()
                            .gap(px(SPACE_2))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .w(px(64.))
                                    .text_size(px(FS_META))
                                    .text_color(rgb(INK_SUBTLE))
                                    .child(who),
                            )
                            .child(div().flex_1().min_w_0().child(SharedString::from(text))),
                    );
                }
                list.into_any_element()
            }
            TaskTab::Context => {
                let fact = |label: &'static str, text: String| {
                    div()
                        .flex()
                        .gap(px(SPACE_2))
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(px(80.))
                                .text_size(px(FS_META))
                                .text_color(rgb(INK_SUBTLE))
                                .child(label),
                        )
                        .child(div().flex_1().min_w_0().child(SharedString::from(text)))
                };
                div()
                    .id("task-context")
                    .debug_selector(|| "task-context".into())
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_2))
                    .p(px(SPACE_4))
                    .overflow_y_scroll()
                    .child(fact("Branch", task.branch.clone()))
                    .children(
                        task.worktree.as_ref().map(|path| {
                            fact("Worktree", ferrite_ui::components::home_relative(path))
                        }),
                    )
                    .child(fact("Agent", core_factory::agent_label(&task.agent)))
                    .children(task.delivery.pr.as_ref().map(|pr| fact("PR", pr.url())))
                    .children(task.workflow.as_ref().map(|run| {
                        fact(
                            "Workflow",
                            format!("{} \u{b7} version {}", run.definition.name, run.version),
                        )
                    }))
                    .children(
                        task.current_step()
                            .map(|step| fact("Step", step.name.clone())),
                    )
                    .child(
                        div()
                            .pt(px(SPACE_2))
                            .text_size(px(FS_META))
                            .text_color(rgb(INK_SUBTLE))
                            .child("Instructions"),
                    )
                    .child(
                        div()
                            .debug_selector(|| "task-instructions".into())
                            .child(SharedString::from(task.instructions.clone())),
                    )
                    .into_any_element()
            }
            TaskTab::Worker => match self.pane_for_worker(task) {
                Some(index) => div()
                    .debug_selector(|| "task-worker".into())
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .p(px(SPACE_2))
                    .child(self.pane_slot(index, Level::Transcript, window, cx))
                    .into_any_element(),
                None => self
                    .worker_read(task, cx)
                    .or_else(|| self.worker_history(task, cx))
                    .unwrap_or_else(|| {
                        div()
                            .debug_selector(|| "task-worker-none".into())
                            .p(px(SPACE_4))
                            .text_color(rgb(INK_SUBTLE))
                            .child("No agent is working on this task right now.")
                            .into_any_element()
                    }),
            },
        };

        Some(
            div()
                .debug_selector(|| "task-panel".into())
                .flex()
                .flex_col()
                .flex_shrink_0()
                .w(px(TASK_PANEL_W))
                .min_h_0()
                .border_l_1()
                .border_color(paint::LINE)
                .child(head)
                .child(tabs)
                .child(div().flex_1().min_h_0().flex().flex_col().child(content))
                .into_any_element(),
        )
    }

    /// The Worker's live Pane, for a Task whose Workers are not parked:
    /// a parked one's is only read (G4).
    fn pane_for_worker(&self, task: &Task) -> Option<usize> {
        if task.archived() || task.keeping.parked {
            return None;
        }
        self.pane_for(task.worker?)
    }

    /// The New task sheet, over the window.
    pub(super) fn new_task_sheet(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sheet = self.factory_view.as_ref()?.sheet.as_ref()?;
        let close = ferrite_ui::sheet::sheet_close("new-task-close", "Close New task", cx)
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.close_new_task(cx);
            }));
        let focused = sheet
            .instructions
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let pr_focused = sheet.pr.read(cx).focus_handle(cx).is_focused(window);
        let process = match sheet
            .workflow
            .as_deref()
            .and_then(|id| self.cockpit.factory().workflow(id))
            .and_then(|workflow| workflow.latest())
        {
            Some(latest) => format!(
                "{} \u{b7} version {}",
                latest.definition.name, latest.version
            ),
            None => "One-off: a single agent".into(),
        };
        let mut body = crate::project_editor::body()
            .child(
                div()
                    .id("new-task-instructions")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            view.focus_new_task_pr(false, cx)
                        }),
                    )
                    .child(crate::project_editor::text_field(
                        "Instructions".into(),
                        sheet.instructions.clone(),
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
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            view.focus_new_task_pr(true, cx)
                        }),
                    )
                    .child(crate::project_editor::text_field(
                        "Pull request (optional): owner/repository#number or its URL".into(),
                        sheet.pr.clone(),
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
                            .child(div().flex_1().child(SharedString::from(process)))
                            .child(ferrite_ui::icons::icon(
                                ferrite_ui::icons::CHEVRON_DOWN,
                                ferrite_ui::theme::ROW_ICON,
                                ferrite_ui::theme::INK_SUBTLE,
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|view, event: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    let picks = view.process_picks();
                                    view.open_picks(picks, event.position, cx);
                                }),
                            ),
                    ),
            );
        if let Some(error) = sheet.error.clone() {
            body = body.child(crate::project_editor::error_line(error));
        }
        let start = crate::project_editor::primary_button("new-task-start", "Start", false, cx)
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.start_new_task(cx);
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
        let veil = crate::project_editor::veil()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.close_new_task(cx);
                }),
            )
            .child(card);
        Some(deferred(veil).with_priority(3).into_any_element())
    }
}

/// A status's selector word: its name on the wire.
fn status_key(status: Status) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
