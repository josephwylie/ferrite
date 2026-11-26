//! A Project's Factory board (ADR 0037, #220): its head, the surface shown
//! (the Coordinator's, the Workflows editor, the Archive's list or the
//! Board's six columns of Task cards) and the open Task's panel beside the
//! columns.
//!
//! `FactoryView` owns what the Factory keeps while it stands in the
//! window's board (ADR 0041): which surface shows and whether the Archive
//! stands in the Board's place, the open Task and its panel's tab, the last
//! command's note, the New task sheet as last closed, the columns' scroll,
//! the skills found, the open picker's rows and its five leaf views. It
//! reads the board through a `Source` the window makes over itself and a
//! test over a fixture, asking with the open Task (`FactoryQuery`). What
//! only the window lays out, a Worker's Pane, comes as its cell
//! (`ferrite_pane::cell::CellSlot`), which the view draws where the panel
//! has it. What the operator asks it says as a `FactoryEvent`; every Server
//! ask, opening a Task and a Task's alerts stay the window's
//! (`cockpit/factory.rs`).

use std::cell::Cell;

use ferrite_core::factory_board::{self, Status, Task, TaskId};
use ferrite_core::protocol::TaskAction;
use ferrite_core::provider::Provider;
use ferrite_core::ThreadKey;
use ferrite_ui::source::Source;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, AnyElement, App, ClickEvent, Context, Div, Entity, EventEmitter, Pixels,
    SharedString, Size, Window,
};

use crate::archive::ArchiveView;
use crate::coordinator::CoordinatorView;
use crate::direction::DirectionView;
use crate::new_task::KeptNewTask;
use crate::picker::Pick;
use crate::starts::StartsView;
use crate::workflows::{Skills, WorkflowsView};

mod panel;
#[cfg(test)]
mod tests;
pub use panel::{HadWorker, TaskPanel, WorkerTab};

/// The Task panel's width beside the columns.
pub const TASK_PANEL_W: f32 = 480.;
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

/// What the Factory shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// The Coordinator's conversation and proposals (#224).
    Coordinator,
    Board,
    Workflows,
}

/// A Task panel's tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskTab {
    Overview,
    Activity,
    Context,
    Worker,
}

impl TaskTab {
    pub const ALL: [TaskTab; 4] = [
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
/// (ADR 0041): which Project's it is stays the window's.
pub struct FactoryView {
    /// The board, as the window has it.
    inputs: Source<FactoryInputs, FactoryQuery>,
    /// The Task whose panel is open.
    open: Option<TaskId>,
    tab: TaskTab,
    /// What the last command said back (a refusal, a worktree kept).
    note: Option<SharedString>,
    /// The New task sheet as it was closed, for when it opens again.
    kept: Option<KeptNewTask>,
    /// The columns' sideways scroll, and the open Task, its column and how
    /// many columns fit when it last brought that column into view.
    columns: gpui::ScrollHandle,
    revealed: Cell<Option<(String, usize, usize)>>,
    /// The Board, the Workflows or the Coordinator's.
    surface: Surface,
    /// The skills each Workflow Step's Provider has installed.
    skills: Skills,
    /// The open Factory picker's rows.
    picks: Vec<(ferrite_ui::menu::Item, Pick)>,
    leaves: Leaves,
    /// Whether the Archive shows in the board's place.
    archive_shown: bool,
}

/// The Factory's leaf views, which the window makes and hears as the
/// Factory opens, so they go with it.
pub struct Leaves {
    /// The Workflows editor: what is chosen, the outline's focus and
    /// scroll, its fields and the one that holds the keyboard.
    pub editor: Entity<WorkflowsView>,
    /// Answers, rework and redirects being written, and their sheets.
    pub direction: Entity<DirectionView>,
    /// The Coordinator surface's field before the Coordinator runs (#224).
    pub coordinator: Entity<CoordinatorView>,
    /// How Tasks start (#225): what the GitHub login listed for its
    /// pickers.
    pub starts: Entity<StartsView>,
    /// The Archive (#226): its search and the earlier Worker being read.
    pub archive: Entity<ArchiveView>,
}

/// What the Factory asks of the window.
#[derive(Clone, Debug, PartialEq)]
pub enum FactoryEvent {
    /// A surface tab: that surface shows; the Board's gives the Archive
    /// back.
    Surface(Surface),
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
    /// One of the Workers the open Task had, opened to be read; `None`
    /// goes back to the list.
    ReadWorker(Option<ThreadKey>),
}

/// What the Factory asks the window to answer: the size of the window it
/// draws in, and the open Task's panel, while the Board shows.
pub struct FactoryQuery {
    pub viewport: Size<Pixels>,
    pub panel: Option<PanelQuery>,
}

/// The open Task's panel, as the Factory keeps it.
pub struct PanelQuery {
    pub task: TaskId,
    /// Whether it opened from the Archive: a Task cleared to the Archive
    /// since leaves the Board's panel.
    pub archived: bool,
    pub tab: TaskTab,
    /// The earlier Worker being read, if any.
    pub reading: Option<ThreadKey>,
}

/// What the Factory's head and the Board draw from the window: the Project
/// and its Factory as the board says them now.
pub struct FactoryInputs {
    /// The Project's title.
    pub title: String,
    /// The Tasks on the board, as their cards say them.
    pub cards: Vec<TaskCard>,
    /// The board's width, which the columns and the Task panel share.
    pub board_w: f32,
    /// How many the Archive holds.
    pub archived: usize,
    /// The Coordinator's waiting proposals, and the Project's Workflows.
    pub proposals: usize,
    pub workflows: usize,
    /// Why the Factory's store could not be read.
    pub trouble: Option<String>,
    /// The open Task's panel, as asked: none once the Task is gone.
    pub panel: Option<TaskPanel>,
}

/// One Task as its card says it.
pub struct TaskCard {
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
    pub fn of(task: &Task) -> Self {
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
            pr: crate::delivery::card_line(task),
        }
    }
}

impl EventEmitter<FactoryEvent> for FactoryView {}

impl FactoryView {
    /// A Factory on the Board, nothing open, reading the board through
    /// `inputs`.
    pub fn new(inputs: Source<FactoryInputs, FactoryQuery>, leaves: Leaves) -> Self {
        Self {
            inputs,
            open: None,
            tab: TaskTab::Overview,
            note: None,
            kept: None,
            columns: gpui::ScrollHandle::new(),
            revealed: Default::default(),
            surface: Surface::Board,
            skills: Default::default(),
            picks: Vec::new(),
            leaves,
            archive_shown: false,
        }
    }

    pub fn surface(&self) -> Surface {
        self.surface
    }

    pub fn set_surface(&mut self, surface: Surface) {
        self.surface = surface;
    }

    /// The Task whose panel is open.
    pub fn open(&self) -> Option<&TaskId> {
        self.open.as_ref()
    }

    /// The open Task's panel's tab.
    pub fn tab(&self) -> TaskTab {
        self.tab
    }

    pub fn set_tab(&mut self, tab: TaskTab) {
        self.tab = tab;
    }

    pub fn archive_shown(&self) -> bool {
        self.archive_shown
    }

    /// What the last command said back.
    pub fn note(&self) -> Option<&SharedString> {
        self.note.as_ref()
    }

    pub fn set_note(&mut self, note: Option<SharedString>) {
        self.note = note;
    }

    /// `task`'s panel opens on its Overview, on the Board, or beside the
    /// Archive for an `archived` one, its column brought into view.
    pub fn open_task(&mut self, task: TaskId, archived: bool, cx: &mut App) {
        self.surface = Surface::Board;
        self.archive_shown = archived;
        self.open = Some(task);
        self.tab = TaskTab::Overview;
        self.note = None;
        self.revealed.set(None);
        self.leaves
            .direction
            .update(cx, |direction, _| direction.set_answering(false));
    }

    /// A Task the New task sheet started: its panel opens on the Board, and
    /// the next New task starts empty.
    pub fn open_started(&mut self, task: TaskId) {
        self.kept = None;
        self.surface = Surface::Board;
        self.open = Some(task);
        self.tab = TaskTab::Overview;
        self.note = None;
    }

    /// The open Task's panel closes.
    pub fn close_task(&mut self) {
        self.open = None;
    }

    /// Show the Archive in the board's place, no Task's panel open.
    pub fn open_archive(&mut self) {
        self.archive_shown = true;
        self.open = None;
    }

    /// The Board again in the Archive's place, no Task's panel open.
    pub fn close_archive(&mut self) {
        self.archive_shown = false;
        self.open = None;
    }

    /// The New task sheet as it was last closed, taken to open it again.
    pub fn take_kept(&mut self) -> Option<KeptNewTask> {
        self.kept.take()
    }

    /// What the New task sheet held as it closed.
    pub fn keep(&mut self, kept: KeptNewTask) {
        self.kept = Some(kept);
    }

    /// The open picker's rows.
    pub fn picks(&self) -> &[(ferrite_ui::menu::Item, Pick)] {
        &self.picks
    }

    pub fn set_picks(&mut self, picks: Vec<(ferrite_ui::menu::Item, Pick)>) {
        self.picks = picks;
    }

    pub fn skills(&self) -> &Skills {
        &self.skills
    }

    pub fn skills_mut(&mut self) -> &mut Skills {
        &mut self.skills
    }

    pub fn editor(&self) -> &Entity<WorkflowsView> {
        &self.leaves.editor
    }

    pub fn direction(&self) -> &Entity<DirectionView> {
        &self.leaves.direction
    }

    pub fn coordinator(&self) -> &Entity<CoordinatorView> {
        &self.leaves.coordinator
    }

    pub fn starts(&self) -> &Entity<StartsView> {
        &self.leaves.starts
    }

    pub fn archive(&self) -> &Entity<ArchiveView> {
        &self.leaves.archive
    }

    /// The open Task's panel, as it asks the window for it: while the
    /// Board shows.
    fn panel_query(&self, cx: &App) -> Option<PanelQuery> {
        if self.surface != Surface::Board {
            return None;
        }
        Some(PanelQuery {
            task: self.open.clone()?,
            archived: self.archive_shown,
            tab: self.tab,
            reading: self.leaves.archive.read(cx).reading.map(|(thread, _)| thread),
        })
    }

    /// The board as `inputs` say it: its head, why its store could not be
    /// read, and the surface shown: the Coordinator's, the Workflows
    /// editor, the Archive's list or the Board's columns, with the open
    /// Task's panel beside the Archive and the columns.
    fn board(&self, mut inputs: FactoryInputs, cx: &mut Context<Self>) -> Div {
        // With no Task on the Board, how to start one stands in its place.
        let panel = inputs
            .panel
            .take()
            .filter(|_| self.archive_shown || !inputs.cards.is_empty())
            .map(|panel| self.task_panel(panel, cx));
        let inputs = &inputs;
        let body = match self.surface {
            Surface::Workflows => self.leaves.editor.clone().into_any_element(),
            Surface::Coordinator => self.leaves.coordinator.clone().into_any_element(),
            Surface::Board if self.archive_shown => div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_row()
                .child(self.leaves.archive.clone())
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
        let on_board = self.surface == Surface::Board && !self.archive_shown;
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

    /// The surface tabs: Coordinator, Board and Workflows, each with its
    /// count.
    fn surface_tabs(&self, inputs: &FactoryInputs, cx: &mut Context<Self>) -> Div {
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

impl Render for FactoryView {
    /// The Factory board: its head, why its store could not be read, and
    /// the surface shown.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = FactoryQuery {
            viewport: window.viewport_size(),
            panel: self.panel_query(cx),
        };
        match self.inputs.read(&query, cx) {
            Some(inputs) => self.board(inputs, cx),
            None => div(),
        }
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

/// The ink a status is said in.
pub(crate) fn status_ink(status: Status) -> u32 {
    use ferrite_ui::theme::*;
    match status {
        Status::Working => RUNNING,
        Status::NeedsHuman | Status::Approval => ATTENTION,
        Status::Stopped => BLOCKED,
        Status::Queued | Status::Completed => INK_SUBTLE,
    }
}

/// A status's selector word: its name on the wire.
fn status_key(status: Status) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
