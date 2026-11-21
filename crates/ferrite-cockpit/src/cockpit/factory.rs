//! A Project's Factory in the window (ADR 0037, #220): its nav entry, its
//! board of Tasks in the board's place, a Task's panel beside the columns,
//! and the alerts a Task raises. Its Workflows surface is `workflows.rs`
//! (#221), and its New task sheet `new_task_view.rs`.
//!
//! The Factory runs in the Server (`ferrite_core::factory`): the window
//! draws the board the Server publishes and sends commands through
//! `Cockpit::task`. Opening a Task's panel starts nothing; its Worker
//! tab draws the Worker's ordinary Pane, which this window already holds.

use super::*;
use super::destination::{Destination, Shows};
use ferrite_core::factory::{self as core_factory, Status, Task, TaskId};
use ferrite_core::protocol::{Done, TaskAction};
use ferrite_core::workspace::registry::ProjectId;
use ferrite_pane::composer::Composer;

/// The Task panel's width beside the columns.
const TASK_PANEL_W: f32 = 480.;
/// A column's narrowest: the six share the board's width down to this,
/// so they fit a 1440 pt window with the sidebar open, and the board
/// scrolls sideways past it (the Task panel open, a narrower window).
const COLUMN_MIN_W: f32 = 168.;

/// How the six columns take the board's width: shared, when they fit at
/// their narrowest; otherwise as many whole columns as fit share it, so a
/// sideways scroll a column at a time starts and ends on a column's edge.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ColumnFit {
    /// Columns in view at once.
    shown: usize,
    /// Each one's width while they scroll; `None` while all six fit.
    width: Option<f32>,
}

impl ColumnFit {
    /// In a scroll area `area` wide: the columns in view and the gap after
    /// each fill it but for one gap, the gutter its left pad keeps once
    /// scrolled, so the next column starts at its right edge.
    fn of(area: f32) -> Self {
        let gap = ferrite_ui::theme::SPACE_3;
        let room = area - gap;
        let all = Status::ALL.len();
        let shown = ((room / (COLUMN_MIN_W + gap)).floor() as usize).clamp(1, all);
        Self {
            shown,
            width: (shown < all).then(|| (room / shown as f32 - gap).max(0.)),
        }
    }

    /// A column and the gap after it.
    fn step(self) -> f32 {
        self.width
            .map_or(0., |width| width + ferrite_ui::theme::SPACE_3)
    }

    /// The scroll that puts column `first` first: past the left pad, a
    /// gap's gutter before it and the column before ending at the edge.
    fn offset(self, first: usize) -> f32 {
        match first {
            0 => 0.,
            _ => {
                self.step() * first as f32 + ferrite_ui::theme::SPACE_4
                    - ferrite_ui::theme::SPACE_3
            }
        }
    }

    /// The first column in view once column `at` is: the one now first
    /// (at scroll `offset`) while `at` is in view from it, else the
    /// nearest that brings `at` in.
    fn first_showing(self, at: usize, offset: Pixels) -> usize {
        let step = self.step();
        if step <= 0. {
            return 0;
        }
        let now = (-f32::from(offset) / step).round().max(0.) as usize;
        let first = if at < now {
            at
        } else if at >= now + self.shown {
            at + 1 - self.shown
        } else {
            now
        };
        first.min(Status::ALL.len() - self.shown)
    }
}

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

/// What the Factory view shows.
pub(crate) struct FactoryView {
    pub(crate) project: ProjectId,
    /// The Task whose panel is open.
    pub(crate) open: Option<TaskId>,
    pub(crate) tab: TaskTab,
    /// What the last command said back (a refusal, a worktree kept).
    pub(crate) note: Option<SharedString>,
    /// The New task sheet as it was closed, for when it opens again.
    pub(super) kept: Option<super::new_task_view::KeptNewTask>,
    /// The columns' sideways scroll, and the open Task, its column and how
    /// many columns fit when it last brought that column into view.
    columns: gpui::ScrollHandle,
    revealed: std::cell::Cell<Option<(String, usize, usize)>>,
    /// The Board or the Workflows.
    pub(crate) surface: super::workflows::Surface,
    pub(crate) workflows: super::workflows::Workflows,
    /// The open Factory picker's rows.
    pub(crate) picks: Vec<(ferrite_ui::menu::Item, super::workflows::Pick)>,
    /// Answers, rework and redirects being written, and their sheets.
    pub(crate) direction: Entity<super::direction::DirectionView>,
    /// The Coordinator surface's field before the Coordinator runs (#224).
    pub(crate) opening: Option<Entity<Composer>>,
    /// How Tasks start (#225): what the GitHub login listed for its
    /// pickers.
    pub(crate) starts: Entity<super::starts::StartsView>,
    /// The Archive (#226): its search and the earlier Worker being read.
    pub(crate) archive: Entity<super::archive::ArchiveView>,
    /// Whether the Archive shows in the board's place.
    pub(crate) archive_shown: bool,
}

/// What identifies a Task's toast among the window's notifications.
struct TaskToast;

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

impl CockpitView {
    /// Show `project`'s Factory board in the board's place.
    pub(crate) fn open_factory(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        let focus_at_open = self.cockpit.roster().focused();
        match self.destination.as_mut() {
            Some(Destination {
                shows: Shows::Factory(view),
                focus_at_open: at,
            }) if view.project == project => *at = focus_at_open,
            _ => {
                let workflows = super::workflows::Workflows::new(cx);
                let archive = self.new_archive(cx);
                let direction = self.new_direction(cx);
                let starts = self.new_starts(cx);
                let view = FactoryView {
                    project,
                    open: None,
                    tab: TaskTab::Overview,
                    note: None,
                    kept: None,
                    columns: gpui::ScrollHandle::new(),
                    revealed: Default::default(),
                    surface: super::workflows::Surface::Board,
                    workflows,
                    picks: Vec::new(),
                    direction,
                    starts,
                    opening: None,
                    archive,
                    archive_shown: false,
                };
                self.destination = Some(Destination {
                    shows: Shows::Factory(view),
                    focus_at_open,
                });
            }
        }
        cx.notify();
    }

    /// Open a Task's panel, brought forward from whichever surface shows:
    /// on the Board, or the Archive for an archived Task. What it shows is
    /// the board's; nothing starts.
    pub(crate) fn open_task(&mut self, task: &str, cx: &mut Context<Self>) {
        let Some((project, archived)) = self
            .cockpit
            .factory()
            .task(task)
            .map(|task| (task.project, task.archived()))
        else {
            return;
        };
        if self.factory().map(|view| view.project) != Some(project) {
            self.open_factory(project, cx);
        }
        self.read_worker(None, cx);
        let archive_shown = self
            .factory()
            .is_some_and(|view| view.archive_shown);
        if archived && !archive_shown {
            self.open_archive(cx);
        }
        if let Some(view) = self.factory_mut() {
            view.surface = super::workflows::Surface::Board;
            view.archive_shown = archived;
            view.open = Some(task.to_string());
            view.tab = TaskTab::Overview;
            view.note = None;
            view.revealed.set(None);
            view.direction
                .update(cx, |direction, _| direction.answering = false);
        }
        self.ensure_answer(task, cx);
        cx.notify();
    }

    pub(crate) fn task_tab(&mut self, tab: TaskTab, cx: &mut Context<Self>) {
        let Some(view) = self.factory_mut() else {
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

    /// The open Task's panel's width on the Factory's board, which a
    /// toast keeps clear of as it does the right panel.
    pub(super) fn task_panel_w(&self) -> f32 {
        let shown = self
            .factory()
            .is_some_and(|view| view.surface == super::workflows::Surface::Board)
            && self.open_task_now().is_some();
        if shown {
            TASK_PANEL_W
        } else {
            0.
        }
    }

    /// Whether `task`'s panel is on screen.
    fn task_panel_shown(&self, task: &str) -> bool {
        self.factory()
            .is_some_and(|view| view.surface == super::workflows::Surface::Board)
            && self.open_task_now().is_some_and(|open| open.id == task)
    }

    /// The open Task, as the board says it now, while it is where the
    /// Factory looks: a Task cleared to the Archive leaves the board's panel.
    pub(super) fn open_task_now(&self) -> Option<&Task> {
        let view = self.factory()?;
        self.cockpit
            .factory()
            .task(view.open.as_deref()?)
            .filter(|task| task.archived() == view.archive_shown)
    }

    /// The open Task's Worker's Pane in this window.
    fn open_worker_pane(&self) -> Option<usize> {
        self.pane_for_worker(self.open_task_now()?)
    }

    /// The Worker drawn in the open panel's Worker tab, which streams whole.
    pub(super) fn factory_worker_drawn(&self, cx: &gpui::App) -> Option<ThreadId> {
        let view = self.factory()?;
        if let Some(coordinator) = self.coordinator_drawn() {
            return Some(coordinator);
        }
        let task = self.open_task_now()?;
        if view.tab != TaskTab::Worker {
            return None;
        }
        if let Some(reading) = self.worker_reading(task, cx) {
            return Some(reading);
        }
        task.worker.filter(|_| self.pane_for_worker(task).is_some())
    }

    /// The Tasks changed: the open panel's Task may be gone.
    pub(super) fn factory_changed(&mut self, cx: &mut Context<Self>) {
        let gone = self
            .factory()
            .and_then(|view| view.open.as_deref())
            .is_some_and(|id| self.cockpit.factory().task(id).is_none());
        if gone {
            if let Some(view) = self.factory_mut() {
                view.open = None;
            }
        }
        if let Some(open) = self.open_task_now().map(|task| task.id.clone()) {
            self.ensure_answer(&open, cx);
        }
        self.ensure_fields(cx);
        cx.notify();
    }

    /// What holds the keyboard while the Factory shows: the Worker's
    /// Composer on its tab, else the window's own. A sheet's field is the
    /// sheet host's (`Sheet::focus`).
    pub(super) fn factory_focus(&self, window: &Window, cx: &gpui::App) -> Option<FocusHandle> {
        let view = self.factory()?;
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
                if let Some(view) = self.factory_mut() {
                    view.open = None;
                }
                kept.map(SharedString::from)
            }
            Ok(_) => None,
            Err(error) => Some(SharedString::from(error.to_string())),
        };
        if let Some(view) = self.factory_mut() {
            view.note = note;
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
        detail: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use gpui::component::WindowExt as _;
        use gpui::component::notification::{Notification, NotificationType};
        let found = self.cockpit.factory().task(&task);
        let provider = found.map(|found| found.agent.provider);
        let cwd = found.and_then(|found| found.worktree.clone());
        let detail = detail.unwrap_or_else(|| title.clone());
        let project_title = self
            .cockpit
            .registry()
            .project(project)
            .map(|project| project.title.clone());
        let heading = SharedString::from(format!(
            "{} \u{b7} {}",
            status.word(),
            found.map_or(title.as_str(), |found| found.title.as_str())
        ));
        let line = SharedString::from(match &project_title {
            Some(project_title) => format!("{project_title} \u{b7} {detail}"),
            None => detail.clone(),
        });
        // Its board on screen shows the Task's state in its column, so the
        // toast only passes by there.
        let board_shown = self.factory().is_some_and(|view| {
            view.project == project
                && view.surface == super::workflows::Surface::Board
                && !view.archive_shown
        });
        let toast = Notification::new()
            .id1::<TaskToast>(SharedString::from(task.clone()))
            .content(move |_, _, _| {
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .text_sm()
                    .child(
                        div()
                            .truncate()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(heading.clone()),
                    )
                    .child(div().truncate().child(line.clone()))
                    .into_any_element()
            })
            .with_type(if status.needs_you() {
                NotificationType::Warning
            } else {
                NotificationType::Info
            })
            .autohide(board_shown || !status.needs_you());
        // As a Thread's toast is not raised over the Thread on screen.
        if !self.task_panel_shown(&task) {
            window.push_notification(toast, cx);
            if status.needs_you() {
                self.task_toasts.insert(task.clone(), status);
            }
        }
        let sounds = match provider {
            Some(provider) => self.prefs.settings.sounds_for(provider),
            None => self.prefs.settings.sounds,
        };
        if sounds {
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
                    cwd,
                },
                cx,
            );
        }
    }

    /// A Task's standing toast goes, with its system notification, once
    /// the Task is out of the state it announced, as a Thread's does once
    /// its request is answered.
    pub(super) fn retire_task_toasts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use gpui::component::WindowExt as _;
        let board = self.cockpit.factory();
        let gone: Vec<String> = self
            .task_toasts
            .iter()
            .filter(|(task, status)| {
                board
                    .task(task)
                    .is_none_or(|found| found.status != **status)
            })
            .map(|(task, _)| task.clone())
            .collect();
        for task in gone {
            self.task_toasts.remove(&task);
            window.remove_notification1::<TaskToast>(SharedString::from(task.clone()), cx);
            crate::system_notifications::withdraw(&task_tag(&task), cx);
        }
    }

    /// Land on an owned Thread through what owns it: a Worker, on its
    /// Task's Worker tab (an earlier Worker's transcript read there); a
    /// Coordinator, on its Project's Coordinator tab.
    pub(super) fn land_on_owned(
        &mut self,
        thread: ThreadId,
        owner: ferrite_core::ThreadOwner,
        cx: &mut Context<Self>,
    ) {
        match owner {
            ferrite_core::ThreadOwner::Task(task) => {
                self.open_task(&task, cx);
                if self
                    .open_task_now()
                    .is_some_and(|task| task.worker == Some(thread))
                {
                    self.task_tab(TaskTab::Worker, cx);
                } else {
                    self.read_worker(Some(thread), cx);
                    if let Some(view) = self.factory_mut() {
                        view.tab = TaskTab::Worker;
                    }
                }
            }
            ferrite_core::ThreadOwner::Factory(project) => {
                self.open_factory(project, cx);
                self.factory_surface(super::workflows::Surface::Coordinator, cx);
            }
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

    /// The Projects with a Factory entry over the nav's cards: each one an
    /// operator's Thread or a Task is in or whose automatic starts are on,
    /// the chosen one's alone while the list is filtered to a Project,
    /// which shows it even when empty. Any other Project's Factory is the
    /// palette's (`Factory: <Project>`).
    pub(super) fn factory_nav_projects(&self, state: &nav::NavState) -> Vec<ProjectId> {
        let board = self.cockpit.factory();
        let automatic = board
            .workflows
            .iter()
            .filter(|workflow| workflow.automation.switch == ferrite_core::factory::Switch::On)
            .map(|workflow| workflow.project);
        let mut projects: Vec<ProjectId> = state
            .open_rows()
            .chain(&state.parked)
            .filter_map(|row| self.facts.get(row.thread).and_then(|facts| facts.project))
            .chain(board.tasks.iter().map(|task| task.project))
            .chain(automatic)
            .chain(self.nav_filter)
            .filter(|project| self.nav_filter.is_none_or(|wanted| wanted == *project))
            .collect();
        projects.sort();
        projects.dedup();
        projects
    }

    /// The Factory menu's rows: a Project's board each, checked while it
    /// shows, with how many of its Tasks need the operator.
    pub(super) fn factory_rows(&self) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let state = self.nav_state();
        let board = self.cockpit.factory();
        self.factory_nav_projects(&state)
            .into_iter()
            .map(|project| {
                let title = self
                    .cockpit
                    .registry()
                    .project(project)
                    .map(|project| project.title.clone())
                    .unwrap_or_default();
                let shown = self
                    .factory()
                    .is_some_and(|view| view.project == project);
                let item = menu::Item::new(title).checked(shown);
                let item = match board.needs_you(project) {
                    0 => item,
                    count => item.detail(count.to_string()),
                };
                Some((item, MenuVerb::OpenFactory(project)))
            })
            .collect()
    }

    /// The Factory board: its head, its six columns, and the open Task's
    /// panel beside them.
    pub(super) fn factory_board(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let Some(view) = self.factory() else {
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
                view.surface == super::workflows::Surface::Board
                    && !view.archive_shown
                    && tasks.iter().any(|task| task.status == Status::Completed),
                |head| head.child(self.clear_button(cx)),
            )
            .when(
                view.surface == super::workflows::Surface::Board && !view.archive_shown,
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
        } else if view.archive_shown {
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
                .overflow_x_scroll()
                .track_scroll(&view.columns);
            let panel = self.task_panel(window, cx);
            let fit = ColumnFit::of(
                self.board_bounds(window).w - if panel.is_some() { TASK_PANEL_W } else { 0. },
            );
            // The open Task's column comes into view beside its panel, and
            // again when the Task moves column, whole columns either side.
            if let Some(task) = open.and_then(|open| tasks.iter().find(|task| task.id == open)) {
                let at = Status::ALL
                    .iter()
                    .position(|each| *each == task.status)
                    .unwrap_or_default();
                let key = (task.id.clone(), at, fit.shown);
                if view.revealed.take().as_ref() != Some(&key) {
                    let first = fit.first_showing(at, view.columns.offset().x);
                    view.columns
                        .set_offset(gpui::point(px(-fit.offset(first)), px(0.)));
                }
                view.revealed.set(Some(key));
            }
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
                    .map(|column| match fit.width {
                        Some(width) => column.flex_shrink_0().w(px(width)),
                        None => column.flex_1().min_w(px(COLUMN_MIN_W)),
                    })
                    .min_h_0()
                    .gap(px(SPACE_2))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(SPACE_2))
                            .h(px(CONTROL_H))
                            .flex_shrink_0()
                            .child(ferrite_ui::components::status_dot(status_ink(status)))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(W_LABEL)
                                    .text_size(px(FS_LABEL))
                                    .text_color(rgb(INK_STRONG))
                                    .child(status.column()),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(FS_META))
                                    .text_color(rgb(INK_SUBTLE))
                                    .child(SharedString::from(in_column.len().to_string())),
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
                .children(panel)
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
            .min_w_0()
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
                    .text_ellipsis()
                    .child(SharedString::from(task.title.clone())),
            )
            .child(
                div()
                    .debug_selector(move || agent_key.clone())
                    .flex()
                    .items_center()
                    .min_w_0()
                    .gap(px(SPACE_1))
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child({
                        let (mark, ink) = ferrite_ui::provider::mark(task.agent.provider);
                        ferrite_ui::icons::icon(mark, BAR_MARK, ink)
                    })
                    .child(div().min_w_0().truncate().child(SharedString::from(
                        match task.current_step() {
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
                        },
                    ))),
            )
            .children(task.outcome().map(|line| {
                div()
                    .debug_selector(move || outcome_key.clone())
                    .text_size(px(FS_META))
                    .text_color(rgb(if task.status == Status::Stopped {
                        BLOCKED
                    } else {
                        INK_STRONG
                    }))
                    .line_clamp(3)
                    .text_ellipsis()
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
                    .factory()
                    .is_some_and(|shown| shown.open.as_deref() == Some(id.as_str()));
                if shut {
                    if let Some(shown) = view.factory_mut() {
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
        let view = self.factory()?;
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
        let close = ferrite_ui::components::ghost_button("task-close", "Close", cx)
            .debug_selector(|| "task-close".into())
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                if let Some(view) = view.factory_mut() {
                    view.open = None;
                }
                cx.notify();
            }));
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
                // A question, a permission or the approval block says it.
                let why_said = task.oversight.question.is_some()
                    || task.oversight.permission.is_some()
                    || task.status == Status::Approval;
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
                            .filter(|_| !why_said)
                            .map(|reason| line("Why", reason).debug_selector(|| "task-why".into())),
                    )
                    .child(line("Status", task.status.word().to_string()))
                    .children(self.direction().filter(|_| super::direction::says(task)))
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
                    .child(self.pane_slot(index, window, cx))
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
}

/// A status's selector word: its name on the wire.
fn status_key(status: Status) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
