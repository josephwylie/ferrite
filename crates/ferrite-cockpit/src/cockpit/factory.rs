//! A Project's Factory in the window (ADR 0037, #220): its nav entry, its
//! board in the board's place, and the alerts a Task raises. The board is
//! `ferrite_factory::board::FactoryView`, which keeps what the Factory
//! keeps while it shows (ADR 0041); which Project's Factory shows stays the
//! root's `destination`.
//!
//! The Factory runs in the Server (`ferrite_engine::factory`): the window
//! answers what the board draws (`factory_inputs`) and sends what it asks
//! through `Cockpit::task`. Opening a Task's panel starts nothing; its
//! Worker tab draws the Worker's ordinary Pane, which this window already
//! holds and hands the board as its cell (`make_factory_cells`).

use super::*;
use super::destination::{Destination, Shows};
use ferrite_core::factory_board::{Status, Task};
use ferrite_core::protocol::{Done, TaskAction};
use ferrite_core::ProjectKey;
use ferrite_factory::board::{
    FactoryEvent, FactoryInputs, FactoryQuery, FactoryView, HadWorker, Leaves, PanelQuery, Surface,
    TaskCard, TaskPanel, TaskTab, WorkerTab, TASK_PANEL_W,
};
use ferrite_ui::source::Source;

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

impl CockpitView {
    /// A Project's Factory, heard from its making, with its leaves.
    fn new_factory(&mut self, cx: &mut Context<Self>) -> Entity<FactoryView> {
        let archive = self.new_archive(cx);
        let direction = self.new_direction(cx);
        let starts = self.new_starts(cx);
        let editor = self.new_workflows(cx);
        let coordinator = self.new_coordinator(cx);
        let inputs = Source::of(&cx.entity(), |root: &Self, query: &FactoryQuery, cx| {
            root.factory_inputs(query, cx)
        });
        let leaves = Leaves {
            editor,
            direction,
            coordinator,
            starts,
            archive,
        };
        let view = cx.new(|_| FactoryView::new(inputs, leaves));
        cx.subscribe(&view, Self::on_factory_event).detach();
        view
    }

    /// The head's presses (a surface tab, Clear completed, New task), a
    /// Task's card, and the open Task's panel's tabs, commands and the
    /// Workers it had.
    fn on_factory_event(
        &mut self,
        _: Entity<FactoryView>,
        event: &FactoryEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            FactoryEvent::Surface(surface) => {
                if *surface == Surface::Board
                    && self.factory().is_some_and(|view| view.read(cx).archive_shown())
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
            FactoryEvent::ReadWorker(thread) => self.read_worker(*thread, cx),
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
            .is_some_and(|view| view.read(cx).archive_shown());
        if archived && !archive_shown {
            self.open_archive(cx);
        }
        self.update_factory(cx, |view, cx| view.open_task(task.to_string(), archived, cx));
        self.ensure_answer(task, cx);
        cx.notify();
    }

    pub(crate) fn task_tab(&mut self, tab: TaskTab, cx: &mut Context<Self>) {
        if self.update_factory(cx, |view, _| view.set_tab(tab)).is_none() {
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
            .is_some_and(|view| view.read(cx).surface() == Surface::Board)
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
                .is_some_and(|view| view.read(cx).surface() == Surface::Board)
            && self.open_task_now(cx).is_some_and(|open| open.id == task)
    }

    /// The open Task, as the board says it now, while it is where the
    /// Factory looks: a Task cleared to the Archive leaves the board's panel.
    pub(super) fn open_task_now<'a>(&self, cx: &'a gpui::App) -> Option<&'a Task> {
        let view = self.factory()?.read(cx);
        self.task_where_looked(view.open()?, view.archive_shown(), cx)
    }

    /// `task`, as the board says it now, while it is where a Factory
    /// looking at the Archive (`archived`) or the Board looks.
    fn task_where_looked<'a>(&self, task: &str, archived: bool, cx: &'a gpui::App) -> Option<&'a Task> {
        self.cockpit
            .read(cx)
            .factory(self.factory_machine()?)
            .reading()
            .task(task)
            .filter(|task| task.archived() == archived)
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
        if view.tab() != TaskTab::Worker {
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
            .and_then(|view| view.read(cx).open().map(String::as_str))
            .is_some_and(|id| {
                self.factory_machine()
                    .is_none_or(|machine| self.cockpit.read(cx).factory(machine).task(id).is_none())
            });
        if gone {
            self.update_factory(cx, |view, _| view.close_task());
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
        if view.tab() == TaskTab::Worker {
            if let Some(index) = self.open_worker_pane(cx) {
                return Some(self.panes[index].read(cx).composer.read(cx).focus_target(window, cx));
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
                self.update_factory(cx, |view, _| view.close_task());
                kept.map(SharedString::from)
            }
            Ok(_) => None,
            Err(error) => Some(SharedString::from(error.to_string())),
        };
        self.update_factory(cx, |view, _| view.set_note(note));
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
                view.surface() == Surface::Board && !view.archive_shown()
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
                ferrite_nav::notifications::Posting {
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
                    self.update_factory(cx, |view, _| view.set_tab(TaskTab::Worker));
                }
            }
            ferrite_core::ThreadOwner::Factory(project) => {
                let project = self.cockpit.read(cx).registry(thread.machine()).project_key(project);
                self.open_factory(project, cx);
                self.factory_surface(Surface::Coordinator, cx);
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
    /// Factory as the board says them now, at a window of `query`'s size,
    /// and the open Task's panel it asks for. Read while the Factory's view
    /// draws, so it never reads that view: what it keeps comes in `query`.
    fn factory_inputs(&self, query: &FactoryQuery, cx: &gpui::App) -> Option<FactoryInputs> {
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
            board_w: self.board_rect(query.viewport, cx).w,
            archived: board.archive(project, "").count(),
            proposals: board
                .proposals_of(project)
                .filter(|proposal| proposal.status == ferrite_core::factory_board::ProposalStatus::Pending)
                .count(),
            workflows: board.workflows_of(project).count(),
            trouble: board.trouble_of(project).map(str::to_string),
            panel: query.panel.as_ref().and_then(|panel| self.task_panel_inputs(panel, cx)),
        })
    }

    /// The Factory board, in the board's place: its view, once the Panes
    /// it lays out this frame have their cells.
    pub(super) fn factory_board(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(factory) = self.factory().cloned() else {
            return div().into_any_element();
        };
        self.make_factory_cells(window, cx);
        factory.into_any_element()
    }

    /// Make the cells of the Panes the Factory lays out this frame, the
    /// Coordinator's on its surface and the open Task's Worker's on its
    /// tab, before the Factory reads them (`made_cell_slot`): a read of the
    /// root cannot make one.
    fn make_factory_cells(&self, window: &Window, cx: &mut Context<Self>) {
        let Some(view) = self.factory().map(|view| view.read(cx)) else {
            return;
        };
        let pane = match view.surface() {
            Surface::Coordinator => self.coordinator_pane(cx),
            Surface::Board if view.tab() == TaskTab::Worker => self.open_worker_pane(cx),
            Surface::Board | Surface::Workflows => None,
        };
        if let Some(index) = pane {
            self.cell_slot(index, window, cx);
        }
    }

    /// What the open Task's panel draws from the window: the Task as the
    /// board says it now and, on its Worker tab, the Worker's Pane, the
    /// earlier Worker being read, or the Workers it had.
    fn task_panel_inputs(&self, panel: &PanelQuery, cx: &gpui::App) -> Option<TaskPanel> {
        let task = self.task_where_looked(&panel.task, panel.archived, cx)?.clone();
        let worker = (panel.tab == TaskTab::Worker)
            .then(|| self.worker_tab(&task, panel.reading, cx))
            .flatten();
        Some(TaskPanel { task, worker })
    }

    /// What `task`'s Worker tab draws: its Worker's Pane, else the earlier
    /// Worker `reading`, if one of its own, else the Workers it had.
    fn worker_tab(&self, task: &Task, reading: Option<ThreadKey>, cx: &gpui::App) -> Option<WorkerTab> {
        if let Some(index) = self.pane_for_worker(task, cx) {
            return self.made_cell_slot(index).map(WorkerTab::Pane);
        }
        if let Some(thread) = self.reading_of(task, reading, cx) {
            let text = self
                .cockpit
                .read(cx)
                .thread(thread)
                .map(|open| ferrite_core::transcript::plain_text(open.transcript().blocks()))
                .unwrap_or_default();
            return Some(WorkerTab::Read(text));
        }
        let board = self.cockpit.read(cx).factory(self.factory_machine()?);
        let step_of = |thread: ThreadKey| {
            let step = task.workers.iter().find(|each| each.thread == thread.thread())?;
            let run = task.workflow.as_ref()?;
            run.definition.step(&step.step).map(|step| step.name.clone())
        };
        let workers: Vec<HadWorker> = task
            .every_worker()
            .into_iter()
            .map(|worker| {
                let thread = board.key(worker);
                HadWorker {
                    thread,
                    step: step_of(thread),
                }
            })
            .collect();
        (!workers.is_empty()).then_some(WorkerTab::Had(workers))
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
