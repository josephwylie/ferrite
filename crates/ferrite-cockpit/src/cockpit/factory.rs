//! A Project's Factory in the window (ADR 0037, #220): its nav entry, its
//! board of Tasks in the board's place, a Task's panel beside the columns,
//! and the alerts a Task raises. Its Workflows surface is `workflows.rs`
//! (#221), and its New task sheet `ferrite_factory::new_task`.
//!
//! The Factory runs in the Server (`ferrite_engine::factory`): the window
//! draws the board the Server publishes and sends commands through
//! `Cockpit::task`. Opening a Task's panel starts nothing; its Worker
//! tab draws the Worker's ordinary Pane, which this window already holds.
//!
//! What the Factory keeps while it shows lives in a view of its own (ADR
//! 0041), `FactoryView`, which the root holds in the board's place; which
//! Project's Factory shows stays the root's `destination`.

use super::*;
use super::destination::{Destination, Shows};
use ferrite_core::factory_board::{self, Status, Task, TaskId};
use ferrite_core::protocol::{Done, TaskAction};
use ferrite_core::ProjectKey;

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

/// What a Project's Factory keeps while it stands in the board's place
/// (ADR 0041): which Project's it is stays the root's `destination`.
pub(crate) struct FactoryView {
    /// The Task whose panel is open.
    pub(crate) open: Option<TaskId>,
    pub(crate) tab: TaskTab,
    /// What the last command said back (a refusal, a worktree kept).
    pub(crate) note: Option<SharedString>,
    /// The New task sheet as it was closed, for when it opens again.
    pub(super) kept: Option<ferrite_factory::new_task::KeptNewTask>,
    /// The columns' sideways scroll, and the open Task, its column and how
    /// many columns fit when it last brought that column into view.
    columns: gpui::ScrollHandle,
    revealed: std::cell::Cell<Option<(String, usize, usize)>>,
    /// The Board or the Workflows.
    pub(crate) surface: super::workflows::Surface,
    /// The skills each Workflow Step's Provider has installed.
    pub(crate) skills: super::workflows::Skills,
    /// The Workflows editor: what is chosen, the outline's focus and
    /// scroll, its fields and the one that holds the keyboard.
    pub(crate) editor: Entity<super::workflows::WorkflowsView>,
    /// The open Factory picker's rows.
    pub(crate) picks: Vec<(ferrite_ui::menu::Item, super::workflows::Pick)>,
    /// Answers, rework and redirects being written, and their sheets.
    pub(crate) direction: Entity<ferrite_factory::direction::DirectionView>,
    /// The Coordinator surface's field before the Coordinator runs (#224).
    pub(crate) coordinator: Entity<super::coordinator::CoordinatorView>,
    /// How Tasks start (#225): what the GitHub login listed for its
    /// pickers.
    pub(crate) starts: Entity<super::starts::StartsView>,
    /// The Archive (#226): its search and the earlier Worker being read.
    pub(crate) archive: Entity<ferrite_factory::archive::ArchiveView>,
    /// Whether the Archive shows in the board's place.
    pub(crate) archive_shown: bool,
}

/// What the Factory asks of the window.
pub(crate) enum FactoryEvent {
    /// A surface tab: that surface shows; the Board's gives the Archive
    /// back.
    Surface(super::workflows::Surface),
    /// Clear completed: the Completed Tasks go to the Archive at once.
    Clear,
    /// New task: its sheet opens, as it was last closed.
    NewTask,
    /// A Task's card: its panel opens.
    OpenTask(TaskId),
    /// A tab of the open Task's panel: the Worker's takes the keyboard to
    /// its Pane.
    Tab(TaskTab),
    /// Stop, Resume or Delete on the open Task.
    Command(TaskAction),
}

/// What the open Task's panel draws from the window.
pub(crate) struct TaskPanel {
    /// The Task, as the board says it now.
    task: Task,
    /// On its Worker tab, what only the root draws: the Worker's Pane, an
    /// earlier Worker read, or the Workers it had; none while no agent
    /// works on it.
    worker: Option<AnyElement>,
}

/// One Task as its card says it.
pub(crate) struct TaskCard {
    id: TaskId,
    title: String,
    status: Status,
    provider: Provider,
    /// Its Step, whether it is deciding an approval, and what it runs on.
    agent: String,
    /// Its result or stop reason.
    outcome: Option<String>,
    /// Its pull request.
    pr: Option<String>,
}

impl TaskCard {
    fn of(task: &Task) -> Self {
        Self {
            id: task.id.clone(),
            title: task.title.clone(),
            status: task.status,
            provider: task.agent.provider,
            agent: match task.current_step() {
                Some(step) => format!(
                    "{}{} \u{b7} {}",
                    step.name,
                    if task.oversight.deciding {
                        " \u{b7} approval decision"
                    } else {
                        ""
                    },
                    factory_board::agent_label(&task.agent)
                ),
                None => factory_board::agent_label(&task.agent),
            },
            outcome: task.outcome().map(str::to_string),
            pr: ferrite_factory::delivery::card_line(task),
        }
    }
}

/// What the Factory's head draws from the window: the Project and its
/// Factory as the board says them now.
pub(crate) struct FactoryInputs {
    /// The Project's title.
    title: String,
    /// The Tasks on the board, as their cards say them.
    cards: Vec<TaskCard>,
    /// The board's width, which the columns and the Task panel share.
    board_w: f32,
    /// How many the Archive holds.
    archived: usize,
    /// The Coordinator's waiting proposals, and the Project's Workflows.
    proposals: usize,
    workflows: usize,
    /// Why the Factory's store could not be read.
    trouble: Option<String>,
}

impl EventEmitter<FactoryEvent> for FactoryView {}

impl FactoryView {
    /// Show the Archive in the board's place, no Task's panel open.
    pub(super) fn open_archive(&mut self) {
        self.archive_shown = true;
        self.open = None;
    }

    /// The Factory board: its head, why its store could not be read, and
    /// the surface shown: the Coordinator's (`coordinator`, which the root
    /// draws round its Pane), the Workflows editor, the Archive's list or
    /// the Board's columns, with the open Task's `panel` beside them.
    fn board(
        &self,
        inputs: &FactoryInputs,
        coordinator: Option<AnyElement>,
        panel: Option<TaskPanel>,
        cx: &mut Context<Self>,
    ) -> Div {
        use super::workflows::Surface;
        let panel = panel.map(|panel| self.task_panel(panel, cx));
        let body = match self.surface {
            Surface::Workflows => self.editor.clone().into_any_element(),
            Surface::Coordinator => coordinator.unwrap_or_else(|| div().into_any_element()),
            Surface::Board if self.archive_shown => div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_row()
                .child(self.archive.clone())
                .children(panel)
                .into_any_element(),
            Surface::Board => self.board_columns(inputs, panel, cx),
        };
        use ferrite_ui::theme::*;
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
            .child(self.head(inputs, cx))
            .children(inputs.trouble.clone().map(trouble))
            .child(body)
    }

    /// The head: the Project's title, its Tasks, the surface tabs, the
    /// note, and the Board's own buttons.
    fn head(&self, inputs: &FactoryInputs, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let count = match inputs.cards.len() {
            0 => "No tasks".to_string(),
            1 => "1 task".to_string(),
            n => format!("{n} tasks"),
        };
        let completed = inputs.cards.iter().any(|card| card.status == Status::Completed);
        let on_board = self.surface == super::workflows::Surface::Board && !self.archive_shown;
        div()
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
                    .child(SharedString::from(format!("{} \u{b7} Factory", inputs.title))),
            )
            .child(
                div()
                    .debug_selector(|| "factory-count".into())
                    .flex_shrink_0()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(count)),
            )
            .child(self.surface_tabs(inputs, cx))
            .child(div().flex_1())
            .children(self.note.clone().map(|note| {
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
                                view.note = None;
                                cx.notify();
                            })),
                    )
            }))
            // Clear, while Completed holds a Task: in the head, where it
            // stays clickable however far the columns scroll.
            .when(on_board && completed, |head| {
                head.child(
                    ferrite_ui::components::ghost_button("factory-clear", "Clear completed", cx)
                        .flex_shrink_0()
                        .debug_selector(|| "factory-clear".into())
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            cx.emit(FactoryEvent::Clear);
                        })),
                )
            })
            // The way to the Archive, with how many it holds.
            .when(on_board, |head| {
                let label = match inputs.archived {
                    0 => "Archive".to_string(),
                    n => format!("Archive \u{b7} {n}"),
                };
                head.child(
                    ferrite_ui::components::ghost_button("factory-archive", label, cx)
                        .debug_selector(|| "factory-archive".into())
                        .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            view.open_archive();
                            cx.notify();
                        })),
                )
            })
            .child(
                ferrite_ui::components::secondary_button(
                    "factory-new-task",
                    Some(ferrite_ui::icons::PLUS),
                    "New task",
                    cx,
                )
                .debug_selector(|| "factory-new-task".into())
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    cx.emit(FactoryEvent::NewTask);
                })),
            )
    }

    /// The Board: its six columns of Task cards with the open Task's
    /// `panel` beside them, which the root draws; while it holds no Task,
    /// how to start one.
    fn board_columns(
        &self,
        inputs: &FactoryInputs,
        panel: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        if inputs.cards.is_empty() {
            return div()
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
                .into_any_element();
        }
        let open = self.open.as_deref();
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
            .track_scroll(&self.columns);
        let fit = ColumnFit::of(
            inputs.board_w - if panel.is_some() { TASK_PANEL_W } else { 0. },
        );
        // The open Task's column comes into view beside its panel, and
        // again when the Task moves column, whole columns either side.
        if let Some(task) = open.and_then(|open| inputs.cards.iter().find(|card| card.id == open)) {
            let at = Status::ALL
                .iter()
                .position(|each| *each == task.status)
                .unwrap_or_default();
            let key = (task.id.clone(), at, fit.shown);
            if self.revealed.take().as_ref() != Some(&key) {
                let first = fit.first_showing(at, self.columns.offset().x);
                self.columns
                    .set_offset(gpui::point(px(-fit.offset(first)), px(0.)));
            }
            self.revealed.set(Some(key));
        }
        for status in Status::ALL {
            let in_column: Vec<&TaskCard> = inputs
                .cards
                .iter()
                .filter(|card| card.status == status)
                .collect();
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
            .into_any_element()
    }

    /// One Task's card: its title, its Worker's Provider and model, and
    /// its result or stop reason.
    fn task_card(&self, task: &TaskCard, open: bool, cx: &mut Context<Self>) -> AnyElement {
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
                        let (mark, ink) = ferrite_ui::provider::mark(task.provider);
                        ferrite_ui::icons::icon(mark, BAR_MARK, ink)
                    })
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(SharedString::from(task.agent.clone())),
                    ),
            )
            .children(task.outcome.clone().map(|line| {
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
            .children(task.pr.clone().map(|line| {
                let pr_key = format!("factory-card-{}-pr", task.id);
                div()
                    .debug_selector(move || pr_key.clone())
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(line))
            }))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                if view.open.as_deref() == Some(id.as_str()) {
                    view.open = None;
                    cx.notify();
                } else {
                    cx.emit(FactoryEvent::OpenTask(id.clone()));
                }
            }));
        card.into_any_element()
    }

    /// The open Task's panel: its status, tabs, and the tab's content.
    fn task_panel(&self, panel: TaskPanel, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let TaskPanel { task, worker } = panel;
        let task = &task;
        let tab = self.tab;
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
                        Some(at) => format!("Archived {}", factory_board::when_label(at)),
                        None => "Archived".to_string(),
                    })),
            );
        } else if task.status.stoppable() {
            let task = id.clone();
            actions = actions.child(
                ferrite_ui::components::secondary_button("task-stop", None, "Stop", cx)
                    .debug_selector(|| "task-stop".into())
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(FactoryEvent::Command(TaskAction::Stop { task: task.clone() }))
                    })),
            );
        }
        if task.status == Status::Stopped && !archived {
            let task = id.clone();
            actions = actions.child(
                ferrite_ui::components::secondary_button("task-resume", None, "Resume", cx)
                    .debug_selector(|| "task-resume".into())
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(FactoryEvent::Command(TaskAction::Resume { task: task.clone() }))
                    })),
            );
        }
        if task.status.deletable() && !archived {
            let task = id.clone();
            actions = actions.child(
                ferrite_ui::form::destructive_button("task-delete", "Delete", false, cx)
                    .debug_selector(|| "task-delete".into())
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(FactoryEvent::Command(TaskAction::Delete { task: task.clone() }))
                    })),
            );
        }
        let close = ferrite_ui::components::ghost_button("task-close", "Close", cx)
            .debug_selector(|| "task-close".into())
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                view.open = None;
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
                    .child(SharedString::from(factory_board::agent_label(&task.agent)))
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
                        cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(FactoryEvent::Tab(each))),
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
                    .children(ferrite_factory::direction::says(task).then(|| self.direction.clone()))
                    .children(ferrite_factory::delivery::task_delivery(task, cx))
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
                    .child(fact("Agent", factory_board::agent_label(&task.agent)))
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
            TaskTab::Worker => worker.unwrap_or_else(|| {
                div()
                    .debug_selector(|| "task-worker-none".into())
                    .p(px(SPACE_4))
                    .text_color(rgb(INK_SUBTLE))
                    .child("No agent is working on this task right now.")
                    .into_any_element()
            }),
        };

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
            .into_any_element()
    }

    /// The surface tabs: Coordinator, Board and Workflows, each with its
    /// count.
    fn surface_tabs(&self, inputs: &FactoryInputs, cx: &mut Context<Self>) -> Div {
        use super::workflows::Surface;
        use ferrite_ui::theme::*;
        let counts = [
            (Surface::Coordinator, "Coordinator", inputs.proposals),
            (Surface::Board, "Board", inputs.cards.len()),
            (Surface::Workflows, "Workflows", inputs.workflows),
        ];
        let mut tabs = div().flex().items_center().gap(px(SPACE_1));
        for (surface, word, count) in counts {
            let key = format!("factory-tab-{}", word.to_lowercase());
            // The Archive is the board's own view, under none of the tabs.
            let selected =
                self.surface == surface && !(surface == Surface::Board && self.archive_shown);
            tabs = tabs.child(
                div()
                    .id(SharedString::from(key.clone()))
                    .debug_selector(move || key.clone())
                    .flex()
                    .items_center()
                    .gap(px(SPACE_1))
                    .px(px(SPACE_2))
                    .h(px(CONTROL_H))
                    .rounded(px(R_CONTROL))
                    .cursor_pointer()
                    .text_size(px(FS_LABEL))
                    .font_weight(if selected { W_LABEL } else { W_BODY })
                    .text_color(rgb(if selected { INK_STRONG } else { INK_SUBTLE }))
                    .when(selected, |tab| tab.bg(paint::RAISED))
                    .when(!selected, |tab| tab.hover(|tab| tab.bg(paint::HOVER)))
                    .child(word)
                    .child(
                        div()
                            .text_color(rgb(INK_SUBTLE))
                            .child(SharedString::from(count.to_string())),
                    )
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(FactoryEvent::Surface(surface))
                    })),
            );
        }
        tabs
    }
}

/// Why the Factory's store could not be read, over the board.
fn trouble(trouble: String) -> AnyElement {
    use ferrite_ui::theme::*;
    div()
        .debug_selector(|| "factory-trouble".into())
        .flex()
        .items_center()
        .gap(px(SPACE_2))
        .mx(px(SPACE_4))
        .mb(px(SPACE_3))
        .p(px(SPACE_3))
        .rounded(px(R_CONTROL))
        .border_1()
        .border_color(paint::CARD_RING)
        .bg(paint::RAISED)
        .child(ferrite_ui::components::status_dot(BLOCKED))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(FS_META))
                .text_color(rgb(INK_STRONG))
                .child(SharedString::from(trouble)),
        )
        .into_any_element()
}

/// What identifies a Task's toast among the window's notifications.
struct TaskToast;

/// A Task's toast among the window's notifications: Task ids are a
/// machine's own.
fn task_toast(machine: ferrite_core::MachineId, task: &str) -> SharedString {
    format!("{}:{task}", machine.get()).into()
}

const TASK_TAG: &str = "ferrite-task-";

/// A system notification's tag for a Task. Task ids are a machine's own,
/// so it names the machine as a Thread's tag does (`system_tag`).
pub(crate) fn task_tag(machine: &ferrite_core::MachineKey, task: &str) -> String {
    match &machine.0 {
        None => format!("{TASK_TAG}{task}"),
        Some(target) => format!("{TASK_TAG}{task}@{target}"),
    }
}

/// The Task a tag names, and its machine: `task_tag` read back. A Task id
/// holds no `@` (`t-N`), so the first one starts the machine's name.
pub(crate) fn tagged_task(tag: &str) -> Option<(ferrite_core::MachineKey, &str)> {
    let named = tag.strip_prefix(TASK_TAG)?;
    Some(match named.split_once('@') {
        Some((task, target)) => (ferrite_core::MachineKey(Some(target.to_string())), task),
        None => (ferrite_core::MachineKey(None), named),
    })
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
    /// A Project's Factory, heard from its making, with its leaves.
    fn new_factory(&mut self, cx: &mut Context<Self>) -> Entity<FactoryView> {
        let archive = self.new_archive(cx);
        let direction = self.new_direction(cx);
        let starts = self.new_starts(cx);
        let editor = self.new_workflows(cx);
        let coordinator = self.new_coordinator(cx);
        let view = cx.new(|_| FactoryView {
            open: None,
            tab: TaskTab::Overview,
            note: None,
            kept: None,
            columns: gpui::ScrollHandle::new(),
            revealed: Default::default(),
            surface: super::workflows::Surface::Board,
            skills: Default::default(),
            editor,
            picks: Vec::new(),
            direction,
            starts,
            coordinator,
            archive,
            archive_shown: false,
        });
        cx.subscribe(&view, Self::on_factory_event).detach();
        view
    }

    /// The head's presses (a surface tab, Clear completed, New task), a
    /// Task's card, and the open Task's panel's tabs and commands.
    fn on_factory_event(
        &mut self,
        _: Entity<FactoryView>,
        event: &FactoryEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            FactoryEvent::Surface(surface) => {
                if *surface == super::workflows::Surface::Board
                    && self.factory().is_some_and(|view| view.read(cx).archive_shown)
                {
                    self.close_archive(cx);
                }
                self.factory_surface(*surface, cx)
            }
            FactoryEvent::Clear => self.clear_completed(cx),
            FactoryEvent::NewTask => self.open_new_task(cx),
            FactoryEvent::OpenTask(task) => {
                if let Some(machine) = self.factory_machine() {
                    self.open_task(machine, task, cx);
                }
            }
            FactoryEvent::Tab(tab) => self.task_tab(*tab, cx),
            FactoryEvent::Command(action) => self.task_command(action.clone(), cx),
        }
    }

    /// Show `project`'s Factory board in the board's place.
    pub(crate) fn open_factory(&mut self, project: ProjectKey, cx: &mut Context<Self>) {
        let focus_at_open = self.cockpit.read(cx).roster().focused();
        match self.destination.as_mut() {
            Some(Destination {
                shows: Shows::Factory(shown, _),
                focus_at_open: at,
            }) if *shown == project => *at = focus_at_open,
            _ => {
                let view = self.new_factory(cx);
                self.destination = Some(Destination {
                    shows: Shows::Factory(project, view),
                    focus_at_open,
                });
            }
        }
        cx.notify();
    }

    /// The machine of the Factory shown, whose board its Tasks are on.
    pub(super) fn factory_machine(&self) -> Option<ferrite_core::MachineId> {
        self.factory_project().map(|project| project.machine())
    }

    /// Open `machine`'s Task `task`'s panel, brought forward from whichever
    /// surface shows: on the Board, or the Archive for an archived Task.
    /// What it shows is the board's; nothing starts.
    pub(crate) fn open_task(
        &mut self,
        machine: ferrite_core::MachineId,
        task: &str,
        cx: &mut Context<Self>,
    ) {
        let board = self.cockpit.read(cx).factory(machine);
        let Some((project, archived)) = board
            .task(task)
            .map(|task| (board.project_key(task.project), task.archived()))
        else {
            return;
        };
        if self.factory_project() != Some(project) {
            self.open_factory(project, cx);
        }
        self.read_worker(None, cx);
        let archive_shown = self
            .factory()
            .is_some_and(|view| view.read(cx).archive_shown);
        if archived && !archive_shown {
            self.open_archive(cx);
        }
        self.update_factory(cx, |view, cx| {
            view.surface = super::workflows::Surface::Board;
            view.archive_shown = archived;
            view.open = Some(task.to_string());
            view.tab = TaskTab::Overview;
            view.note = None;
            view.revealed.set(None);
            view.direction
                .update(cx, |direction, _| direction.set_answering(false));
        });
        self.ensure_answer(task, cx);
        cx.notify();
    }

    pub(crate) fn task_tab(&mut self, tab: TaskTab, cx: &mut Context<Self>) {
        if self.update_factory(cx, |view, _| view.tab = tab).is_none() {
            return;
        }
        // The Worker's Pane takes the keyboard, as any Pane the operator
        // looks at: what is typed there is said to the Worker.
        if tab == TaskTab::Worker {
            if let Some(index) = self.open_worker_pane(cx) {
                self.focus_pane(index, cx);
            }
        }
        cx.notify();
    }

    /// The open Task's panel's width on the Factory's board, which a
    /// toast keeps clear of as it does the right panel.
    pub(super) fn task_panel_w(&self, cx: &gpui::App) -> f32 {
        let shown = self
            .factory()
            .is_some_and(|view| view.read(cx).surface == super::workflows::Surface::Board)
            && self.open_task_now(cx).is_some();
        if shown {
            TASK_PANEL_W
        } else {
            0.
        }
    }

    /// Whether `task`'s panel is on screen.
    fn task_panel_shown(&self, machine: ferrite_core::MachineId, task: &str, cx: &gpui::App) -> bool {
        self.factory_machine() == Some(machine)
            && self
                .factory()
                .is_some_and(|view| view.read(cx).surface == super::workflows::Surface::Board)
            && self.open_task_now(cx).is_some_and(|open| open.id == task)
    }

    /// The open Task, as the board says it now, while it is where the
    /// Factory looks: a Task cleared to the Archive leaves the board's panel.
    pub(super) fn open_task_now<'a>(&self, cx: &'a gpui::App) -> Option<&'a Task> {
        let view = self.factory()?.read(cx);
        self.cockpit
            .read(cx)
            .factory(self.factory_machine()?)
            .reading()
            .task(view.open.as_deref()?)
            .filter(|task| task.archived() == view.archive_shown)
    }

    /// The open Task's Worker's Pane in this window.
    fn open_worker_pane(&self, cx: &gpui::App) -> Option<usize> {
        self.pane_for_worker(self.open_task_now(cx)?, cx)
    }

    /// The Worker drawn in the open panel's Worker tab, which streams whole.
    pub(super) fn factory_worker_drawn(&self, cx: &gpui::App) -> Option<ThreadKey> {
        let view = self.factory()?.read(cx);
        if let Some(coordinator) = self.coordinator_drawn(cx) {
            return Some(coordinator);
        }
        let task = self.open_task_now(cx)?;
        if view.tab != TaskTab::Worker {
            return None;
        }
        if let Some(reading) = self.worker_reading(task, cx) {
            return Some(reading);
        }
        let worker = self
            .cockpit
            .read(cx)
            .factory(self.factory_machine()?)
            .key(task.worker?);
        self.pane_for_worker(task, cx).is_some().then_some(worker)
    }

    /// The Tasks changed: the open panel's Task may be gone.
    pub(super) fn factory_changed(&mut self, cx: &mut Context<Self>) {
        let gone = self
            .factory()
            .and_then(|view| view.read(cx).open.as_deref())
            .is_some_and(|id| {
                self.factory_machine()
                    .is_none_or(|machine| self.cockpit.read(cx).factory(machine).task(id).is_none())
            });
        if gone {
            self.update_factory(cx, |view, _| view.open = None);
        }
        if let Some(open) = self.open_task_now(cx).map(|task| task.id.clone()) {
            self.ensure_answer(&open, cx);
        }
        self.ensure_fields(cx);
        cx.notify();
    }

    /// What holds the keyboard while the Factory shows: the Worker's
    /// Composer on its tab, else the window's own. A sheet's field is the
    /// sheet host's (`Sheet::focus`).
    pub(super) fn factory_focus(&self, window: &Window, cx: &gpui::App) -> Option<FocusHandle> {
        let view = self.factory()?.read(cx);
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
            if let Some(index) = self.open_worker_pane(cx) {
                return Some(self.panes[index].composer.read(cx).focus_target(window, cx));
            }
        }
        Some(self.focus.clone())
    }

    /// The Factory command a press makes, on the machine of the Factory
    /// shown, its answer in the view's note.
    pub(crate) fn task_command(&mut self, action: TaskAction, cx: &mut Context<Self>) {
        let Some(machine) = self.factory_machine() else {
            return;
        };
        let note = match self.cockpit.update(cx, |cockpit, _| cockpit.task(machine, action)) {
            Ok(Some(Done::Deleted { kept })) => {
                self.update_factory(cx, |view, _| view.open = None);
                kept.map(SharedString::from)
            }
            Ok(_) => None,
            Err(error) => Some(SharedString::from(error.to_string())),
        };
        self.update_factory(cx, |view, _| view.note = note);
        cx.notify();
    }

    /// A Task needs the operator, stopped or completed: a toast, a chime
    /// and a system notification, and nothing in the Inbox.
    pub(super) fn present_task_alert(
        &mut self,
        project: ProjectKey,
        task: String,
        status: Status,
        title: String,
        detail: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use gpui::component::WindowExt as _;
        use gpui::component::notification::{Notification, NotificationType};
        let machine = project.machine();
        let found = self.cockpit.read(cx).factory(machine).reading().task(&task);
        let provider = found.map(|found| found.agent.provider);
        let cwd = found.and_then(|found| found.worktree.clone());
        let detail = detail.unwrap_or_else(|| title.clone());
        let project_title = self
            .cockpit
            .read(cx)
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
        let board_shown = self.factory_project() == Some(project)
            && self.factory().is_some_and(|view| {
                let view = view.read(cx);
                view.surface == super::workflows::Surface::Board && !view.archive_shown
            });
        let toast = Notification::new()
            .id1::<TaskToast>(task_toast(machine, &task))
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
        if !self.task_panel_shown(machine, &task, cx) {
            window.push_notification(toast, cx);
            if status.needs_you() {
                self.task_toasts.insert((machine, task.clone()), status);
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
            && crate::system_notifications::wanted(false, self.cockpit.read(cx).window_active())
        {
            let saved_as = self.cockpit.read(cx).saved_as(machine).cloned().unwrap_or_default();
            crate::system_notifications::post(
                crate::system_notifications::Posting {
                    tag: task_tag(&saved_as, &task),
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
        let cockpit = self.cockpit.read(cx);
        let gone: Vec<(ferrite_core::MachineId, String)> = self
            .task_toasts
            .iter()
            .filter(|((machine, task), status)| {
                cockpit
                    .factory(*machine)
                    .task(task)
                    .is_none_or(|found| found.status != **status)
            })
            .map(|(key, _)| key.clone())
            .collect();
        for (machine, task) in gone {
            self.task_toasts.remove(&(machine, task.clone()));
            window.remove_notification1::<TaskToast>(task_toast(machine, &task), cx);
            let saved_as = self.cockpit.read(cx).saved_as(machine).cloned().unwrap_or_default();
            crate::system_notifications::withdraw(&task_tag(&saved_as, &task), cx);
        }
    }

    /// Land on an owned Thread through what owns it: a Worker, on its
    /// Task's Worker tab (an earlier Worker's transcript read there); a
    /// Coordinator, on its Project's Coordinator tab.
    pub(super) fn land_on_owned(
        &mut self,
        thread: ThreadKey,
        owner: ferrite_core::ThreadOwner,
        cx: &mut Context<Self>,
    ) {
        match owner {
            ferrite_core::ThreadOwner::Task(task) => {
                self.open_task(thread.machine(), &task, cx);
                if self
                    .open_task_now(cx)
                    .is_some_and(|task| task.worker == Some(thread.thread()))
                {
                    self.task_tab(TaskTab::Worker, cx);
                } else {
                    self.read_worker(Some(thread), cx);
                    self.update_factory(cx, |view, _| view.tab = TaskTab::Worker);
                }
            }
            ferrite_core::ThreadOwner::Factory(project) => {
                let project = self.cockpit.read(cx).registry(thread.machine()).project_key(project);
                self.open_factory(project, cx);
                self.factory_surface(super::workflows::Surface::Coordinator, cx);
            }
        }
    }

    /// A Task's system notification, clicked: its panel opens, on the
    /// machine its tag names, while the window holds it and its board has
    /// the Task.
    pub(super) fn open_task_notification(&mut self, tag: &str, cx: &mut Context<Self>) -> bool {
        let Some((machine, task)) = tagged_task(tag) else {
            return false;
        };
        let cockpit = self.cockpit.read(cx);
        let machine = cockpit
            .machine_saved_as(&machine)
            .filter(|machine| cockpit.factory(*machine).task(task).is_some());
        if let Some(machine) = machine {
            let task = task.to_string();
            self.open_task(machine, &task, cx);
        }
        true
    }

    /// The Factory menu's rows: a Project's board each, checked while it
    /// shows, with how many of its Tasks need the operator.
    pub(super) fn factory_rows(&self, cx: &gpui::App) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let cockpit = self.cockpit.read(cx);
        self.nav_view
            .read(cx)
            .factories()
            .iter()
            .map(|&project| {
                let title = cockpit
                    .project(project)
                    .map(|project| project.title.clone())
                    .unwrap_or_default();
                let title = match self.machine_label(project.machine(), cx) {
                    Some(machine) => format!("{machine} \u{203a} {title}"),
                    None => title,
                };
                let shown = self.factory_project() == Some(project);
                let item = menu::Item::new(title).checked(shown);
                let item = match cockpit.factory(project.machine()).needs_you(project.project()) {
                    0 => item,
                    count => item.detail(count.to_string()),
                };
                Some((item, MenuVerb::OpenFactory(project)))
            })
            .collect()
    }

    /// What the Factory's head and the Board draw: the Project and its
    /// Factory as the board says them now.
    fn factory_inputs(&self, window: &Window, cx: &gpui::App) -> Option<FactoryInputs> {
        let key = self.factory_project()?;
        let board = self.cockpit.read(cx).factory(key.machine());
        let project = key.project();
        Some(FactoryInputs {
            title: self
                .cockpit
                .read(cx)
                .project(key)
                .map(|project| project.title.clone())
                .unwrap_or_else(|| "Project".into()),
            cards: board.on_board(project).map(TaskCard::of).collect(),
            board_w: self.board_bounds(window, cx).w,
            archived: board.archive(project, "").count(),
            proposals: board
                .proposals_of(project)
                .filter(|proposal| proposal.status == ferrite_core::factory_board::ProposalStatus::Pending)
                .count(),
            workflows: board.workflows_of(project).count(),
            trouble: board.trouble_of(project).map(str::to_string),
        })
    }

    /// The Factory board: the Factory's head over the surface shown.
    pub(super) fn factory_board(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let (Some(factory), Some(inputs)) = (self.factory().cloned(), self.factory_inputs(window, cx))
        else {
            return div();
        };
        let (surface, archive_shown) = {
            let view = factory.read(cx);
            (view.surface, view.archive_shown)
        };
        // What only the root draws: the Coordinator's surface round its
        // Pane, else the open Task's panel's inputs on the Board or the
        // Archive.
        let (coordinator, panel) = match surface {
            super::workflows::Surface::Coordinator => {
                (Some(self.coordinator_surface(window, cx).into_any_element()), None)
            }
            super::workflows::Surface::Board if archive_shown || !inputs.cards.is_empty() => {
                (None, self.task_panel_inputs(window, cx))
            }
            _ => (None, None),
        };
        factory.update(cx, |factory, cx| factory.board(&inputs, coordinator, panel, cx))
    }

    /// What the open Task's panel draws from the window: the Task as the
    /// board says it now and, on its Worker tab, what only the root draws
    /// (the Worker's Pane, an earlier Worker read, the Workers it had).
    fn task_panel_inputs(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<TaskPanel> {
        let tab = self.factory()?.read(cx).tab;
        let task = self.open_task_now(cx)?.clone();
        let worker = if tab == TaskTab::Worker {
            match self.pane_for_worker(&task, cx) {
                Some(index) => Some(
                    div()
                        .debug_selector(|| "task-worker".into())
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .p(px(ferrite_ui::theme::SPACE_2))
                        .child(self.pane_slot(index, window, cx))
                        .into_any_element(),
                ),
                None => self
                    .worker_read(&task, cx)
                    .or_else(|| self.worker_history(&task, cx)),
            }
        } else {
            None
        };
        Some(TaskPanel { task, worker })
    }

    /// The Worker's live Pane, for a Task whose Workers are not parked:
    /// a parked one's is only read (G4).
    fn pane_for_worker(&self, task: &Task, cx: &gpui::App) -> Option<usize> {
        if task.archived() || task.keeping.parked {
            return None;
        }
        self.pane_for(
            self.cockpit
                .read(cx)
                .factory(self.factory_machine()?)
                .key(task.worker?),
        )
    }
}

/// A status's selector word: its name on the wire.
fn status_key(status: Status) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
