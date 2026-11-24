//! A Project's Archive in the window (#226): Completed Tasks leave the
//! board 7 days after completing, or at once with the board head's Clear
//! completed. The Archive is searched by title and instructions; an archived
//! Task opens in the ordinary Task panel, read-only, with its activity
//! and the Workers it had.
//!
//! Its state lives in a view of its own (ADR 0041), `ArchiveView`: the
//! search field and the earlier Worker being read. The Factory holds it,
//! so it goes when the Factory leaves the board's place, as it did. Its
//! render is the list beside the Task panel, reading the archived Tasks
//! that match through `archive_inputs`; what a row or Board asks it says
//! as an `ArchiveEvent`. Whether the Archive shows stays the Factory's
//! (`archive_shown`), which sets the list beside the Task panel; reopening
//! an earlier Worker to be read, and drawing it on the panel's Worker tab,
//! stay the root's.

use gpui::WeakEntity;

use super::*;
use super::destination::{Destination, Shows};
use ferrite_core::factory_board::{self, Task, TaskId};
use ferrite_core::protocol::TaskAction;
use ferrite_core::ThreadKey;
use ferrite_pane::composer::{Composer, Role};

/// One archived Task as its row says it.
pub(crate) struct ArchivedRow {
    id: TaskId,
    title: String,
    completed_ms: Option<i64>,
    archived_ms: Option<i64>,
    result: Option<String>,
}

/// What the Archive's list draws from the window.
pub(crate) struct ArchiveInputs {
    /// The archived Tasks that match the search, most recently archived
    /// first.
    found: Vec<ArchivedRow>,
    /// Whether anything is archived at all.
    any: bool,
    /// The Task whose panel is open.
    open: Option<TaskId>,
}

/// What the Archive's list asks of the window.
pub(crate) enum ArchiveEvent {
    /// An archived Task's row: open its panel.
    Open(TaskId),
    /// Board: give the board back.
    Close,
}

/// The Archive: its search, and the earlier Worker being read.
pub(crate) struct ArchiveView {
    cockpit: WeakEntity<CockpitView>,
    search: Entity<Composer>,
    /// An earlier Worker of the open Task, opened to be read (G4), and
    /// whether this window reopened it, to park it again.
    pub(super) reading: Option<(ThreadKey, bool)>,
}

impl EventEmitter<ArchiveEvent> for ArchiveView {}

impl ArchiveView {
    pub(super) fn new(cockpit: WeakEntity<CockpitView>, cx: &mut Context<Self>) -> Self {
        let search = cx.new(Composer::new);
        search.update(cx, |field, cx| field.set_role(Role::Field(None), cx));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        Self {
            cockpit,
            search,
            reading: None,
        }
    }

    /// The search field's focus, which holds the keyboard while the
    /// Archive shows.
    pub(super) fn focus(&self, cx: &gpui::App) -> FocusHandle {
        self.search.read(cx).focus_handle(cx)
    }

    /// Replace what the Archive is searched for.
    #[cfg(test)]
    pub(super) fn set_search(&self, text: &str, cx: &mut Context<Self>) {
        self.search
            .update(cx, |field, cx| field.set(text.to_string(), cx));
    }

    /// Forget an earlier Worker whose Thread is gone.
    pub(super) fn retain_threads(&mut self, keep: impl Fn(ThreadKey) -> bool) {
        if self.reading.is_some_and(|(thread, _)| !keep(thread)) {
            self.reading = None;
        }
    }

    #[cfg(test)]
    pub(super) fn held_for_test(&self, thread: ThreadKey) -> usize {
        usize::from(self.reading.is_some_and(|(held, _)| held == thread))
    }

    /// One archived Task: its title, when it completed and was archived,
    /// and its result. Opening it starts nothing.
    fn row(&self, task: ArchivedRow, open: bool, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let ArchivedRow {
            id,
            title,
            completed_ms,
            archived_ms,
            result,
        } = task;
        let key = format!("factory-archive-{id}");
        let when = |label: &str, at: Option<i64>| {
            at.map(|at| format!("{label} {}", factory_board::when_label(at)))
        };
        let meta: Vec<String> = [
            Some(format!("#{id}")),
            when("Completed", completed_ms),
            when("Archived", archived_ms),
        ]
        .into_iter()
        .flatten()
        .collect();
        div()
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
            .map(|row| {
                if open {
                    row.bg(paint::SELECTED)
                } else {
                    row.bg(paint::RAISED).hover(|row| row.bg(paint::SELECTED))
                }
            })
            .child(
                div()
                    .text_color(rgb(INK_STRONG))
                    .truncate()
                    .child(SharedString::from(title)),
            )
            .child(
                div()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(meta.join(" \u{b7} "))),
            )
            .children(result.map(|result| {
                div()
                    .text_size(px(FS_META))
                    .line_clamp(2)
                    .text_ellipsis()
                    .child(SharedString::from(result))
            }))
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.emit(ArchiveEvent::Open(id.clone()))
            }))
            .into_any_element()
    }
}

impl Render for ArchiveView {
    /// The Archive's list: its head, its search, and each archived Task
    /// that matches.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use ferrite_ui::theme::*;
        let query = self.search.read(cx).text().to_string();
        let Some(ArchiveInputs { found, any, open }) = self
            .cockpit
            .upgrade()
            .and_then(|cockpit| cockpit.read(cx).archive_inputs(&query, cx))
        else {
            return div().id("factory-archive-list");
        };
        let search = self.search.clone();
        let focused = search.read(cx).focus_handle(cx).is_focused(window);

        let head = div()
            .flex()
            .items_center()
            .gap(px(SPACE_2))
            .child(
                div()
                    .font_weight(W_LABEL)
                    .text_color(rgb(INK_STRONG))
                    .child("Archive"),
            )
            // The note gives way, so Board stays in a narrow board.
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child("Completed tasks, 7 days after they complete or when cleared"),
            )
            .child(div().flex_1())
            .child(
                ferrite_ui::components::ghost_button("factory-archive-close", "Board", cx)
                    .debug_selector(|| "factory-archive-close".into())
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        cx.emit(ArchiveEvent::Close);
                    })),
            );
        let mut list = div()
            .id("factory-archive-list")
            .debug_selector(|| "factory-archive-list".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .px(px(SPACE_4))
            .pb(px(SPACE_4))
            .overflow_y_scroll()
            .child(head)
            .child(
                div()
                    .debug_selector(|| "factory-archive-search".into())
                    .child(crate::project_editor::text_field(
                        "Search titles and instructions".into(),
                        search,
                        focused,
                        "factory-archive-search-focused",
                    )),
            );
        if found.is_empty() {
            list = list.child(
                div()
                    .debug_selector(|| "factory-archive-empty".into())
                    .py(px(SPACE_3))
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(if any {
                        "No archived task matches."
                    } else {
                        "Nothing is archived yet."
                    }),
            );
        }
        for task in found {
            let open = open.as_deref() == Some(task.id.as_str());
            list = list.child(self.row(task, open, cx));
        }
        list
    }
}

impl CockpitView {
    /// A Factory's Archive, heard from its making.
    pub(super) fn new_archive(&mut self, cx: &mut Context<Self>) -> Entity<ArchiveView> {
        let me = cx.entity().downgrade();
        let archive = cx.new(|cx| ArchiveView::new(me, cx));
        cx.subscribe(&archive, Self::on_archive_event).detach();
        archive
    }

    /// What the Archive's list draws: the shown Project's archived Tasks
    /// that match `query`, and the open one.
    fn archive_inputs(&self, query: &str, cx: &gpui::App) -> Option<ArchiveInputs> {
        let key = self.factory_project()?;
        let project = key.project();
        let view = self.factory()?.read(cx);
        let board = self.cockpit.read(cx).factory(key.machine()).reading();
        let found = board
            .archive(project, query)
            .map(|task| ArchivedRow {
                id: task.id.clone(),
                title: task.title.clone(),
                completed_ms: task.keeping.completed_ms,
                archived_ms: task.keeping.archived_ms,
                result: task.result.clone(),
            })
            .collect();
        Some(ArchiveInputs {
            found,
            any: board.archive(project, "").next().is_some(),
            open: view.open.clone(),
        })
    }

    /// The Archive's list: a row opens its Task, Board closes it.
    fn on_archive_event(
        &mut self,
        _: Entity<ArchiveView>,
        event: &ArchiveEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ArchiveEvent::Open(task) => {
                if let Some(machine) = self.factory_machine() {
                    self.open_task(machine, task, cx);
                }
            }
            ArchiveEvent::Close => self.close_archive(cx),
        }
    }

    /// Show the shown Project's Archive in the board's place.
    pub(crate) fn open_archive(&mut self, cx: &mut Context<Self>) {
        self.update_factory(cx, |view, _| view.open_archive());
        cx.notify();
    }

    pub(crate) fn close_archive(&mut self, cx: &mut Context<Self>) {
        self.read_worker(None, cx);
        self.update_factory(cx, |view, _| {
            view.archive_shown = false;
            view.open = None;
        });
        cx.notify();
    }

    /// Open an earlier Worker of a Task that has none working to be read:
    /// its Thread reopened with nothing running and drawn without a
    /// Composer (G4). `None` goes back to the list; a Thread this window
    /// reopened is parked again.
    pub(crate) fn read_worker(&mut self, thread: Option<ThreadKey>, cx: &mut Context<Self>) {
        let was = self.factory().map(|view| view.read(cx).archive.clone()).and_then(|archive| {
            archive.update(cx, |archive, _| archive.reading.take())
        });
        if let Some((was, true)) = was.filter(|(was, _)| Some(*was) != thread) {
            let parked = self.cockpit.update(cx, |cockpit, _| cockpit.park(was));
            self.when_answered(
                parked,
                |view, parked, cx| {
                    if let Err(error) = parked {
                        ferrite_core::log!("factory: an earlier Worker could not be parked again: {error}");
                    }
                    // Parking it moves the roster's focus: that is not the
                    // operator leaving the board.
                    let focused = view.cockpit.read(cx).roster().focused();
                    if let Some(Destination {
                        shows: Shows::Factory(..),
                        focus_at_open,
                    }) = view.destination.as_mut()
                    {
                        *focus_at_open = focused;
                    }
                },
                cx,
            );
        }
        if let Some(thread) = thread {
            match was {
                Some((same, reopened)) if same == thread => self.worker_opened(thread, Ok(reopened), cx),
                _ if self.cockpit.read(cx).thread(thread).is_some() => self.worker_opened(thread, Ok(false), cx),
                _ => {
                    let revived = self.cockpit.update(cx, |cockpit, _| cockpit.revive(thread));
                    self.when_answered(
                        revived,
                        move |view, revived, cx| view.worker_opened(thread, revived.map(|()| true), cx),
                        cx,
                    );
                }
            }
        }
        cx.notify();
    }

    /// The earlier Worker asked for is open to be read, or is not:
    /// `reopened` when this window opened its Thread for it.
    fn worker_opened(&mut self, thread: ThreadKey, reopened: std::io::Result<bool>, cx: &mut Context<Self>) {
        // Reopening it moves the roster's focus before this window has
        // its Pane: that is not the operator leaving the board.
        let focused = self.cockpit.read(cx).roster().focused();
        if let Some(Destination {
            shows: Shows::Factory(_, view),
            focus_at_open,
        }) = self.destination.as_mut()
        {
            *focus_at_open = focused;
            view.update(cx, |view, cx| match reopened {
                Ok(reopened) => view.archive.update(cx, |archive, _| {
                    archive.reading = Some((thread, reopened))
                }),
                Err(error) => {
                    view.note = Some(SharedString::from(format!(
                        "That agent's transcript could not be opened: {error}"
                    )))
                }
            });
        }
        cx.notify();
    }

    /// The earlier Worker being read, if it is one of `task`'s.
    pub(super) fn worker_reading(&self, task: &Task, cx: &gpui::App) -> Option<ThreadKey> {
        let (thread, _) = self.factory()?.read(cx).archive.read(cx).reading?;
        let board = self.cockpit.read(cx).factory(self.factory_machine()?);
        task.every_worker()
            .into_iter()
            .any(|worker| board.key(worker) == thread)
            .then_some(thread)
    }

    /// The earlier Worker's transcript, to read: no Composer, nothing to
    /// send.
    pub(super) fn worker_read(&self, task: &Task, cx: &mut Context<Self>) -> Option<AnyElement> {
        use ferrite_ui::theme::*;
        let thread = self.worker_reading(task, cx)?;
        let text = self
            .cockpit
            .read(cx)
            .thread(thread)
            .map(|open| ferrite_core::transcript::plain_text(open.transcript().blocks()))
            .unwrap_or_default();
        let text = if text.trim().is_empty() {
            "Nothing was said in this agent's Thread.".to_string()
        } else {
            text
        };
        Some(
            div()
                .debug_selector(|| "task-worker-read".into())
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .gap(px(SPACE_2))
                .p(px(SPACE_4))
                .child(
                    div()
                        .id("task-worker-read-back")
                        .debug_selector(|| "task-worker-read-back".into())
                        .cursor_pointer()
                        .text_size(px(FS_META))
                        .text_color(rgb(INK_SUBTLE))
                        .hover(|style| style.text_color(rgb(INK_STRONG)))
                        .child("\u{2190} Its agents")
                        .on_click(
                            cx.listener(|view, _: &ClickEvent, _, cx| view.read_worker(None, cx)),
                        ),
                )
                .child(
                    div()
                        .id("task-worker-read-text")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .whitespace_normal()
                        .child(SharedString::from(text)),
                )
                .into_any_element(),
        )
    }

    /// Replace what the Archive is searched for.
    #[cfg(test)]
    pub(crate) fn set_archive_search(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(archive) = self.factory().map(|view| view.read(cx).archive.clone()) else {
            return;
        };
        archive.update(cx, |archive, cx| archive.set_search(text, cx));
        cx.notify();
    }

    /// The Archive's search field holds the keyboard while it shows.
    pub(super) fn archive_focus(&self, cx: &gpui::App) -> Option<FocusHandle> {
        let view = self.factory()?.read(cx);
        if !view.archive_shown || view.surface != super::workflows::Surface::Board {
            return None;
        }
        Some(view.archive.read(cx).focus(cx))
    }

    /// Clear: the Completed Tasks go to the Archive at once.
    pub(crate) fn clear_completed(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.factory_project() else {
            return;
        };
        self.task_command(
            TaskAction::Clear {
                project: project.project(),
            },
            cx,
        );
    }

    /// The Workers a Task had, each Step's and the last, for a Task with
    /// no Worker Pane in this window (a completed one, its Workers parked).
    pub(super) fn worker_history(&self, task: &Task, cx: &mut Context<Self>) -> Option<AnyElement> {
        use ferrite_ui::theme::*;
        let board = self.cockpit.read(cx).factory(self.factory_machine()?);
        let threads: Vec<ThreadKey> = task
            .every_worker()
            .into_iter()
            .map(|worker| board.key(worker))
            .collect();
        if threads.is_empty() {
            return None;
        }
        let step_of = |thread: ThreadKey| {
            let step = task.workers.iter().find(|each| each.thread == thread.thread())?;
            let run = task.workflow.as_ref()?;
            run.definition
                .step(&step.step)
                .map(|step| step.name.clone())
        };
        let mut list = div()
            .debug_selector(|| "task-worker-history".into())
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .p(px(SPACE_4))
            .child(
                div()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(if task.archived() || task.keeping.parked {
                        "Its agents are parked: their Threads are kept until the task is deleted."
                    } else {
                        "No agent is working on this task right now."
                    }),
            );
        for (at, thread) in threads.into_iter().enumerate() {
            let name = step_of(thread).unwrap_or_else(|| "Agent".to_string());
            list = list.child(
                div()
                    .id(SharedString::from(format!("task-worker-history-{at}")))
                    .debug_selector(move || format!("task-worker-history-{at}"))
                    .cursor_pointer()
                    .hover(|style| style.bg(paint::RAISED))
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.read_worker(Some(thread), cx)
                    }))
                    .flex()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(SharedString::from(name)),
                    )
                    .child(
                        div()
                            .text_size(px(FS_META))
                            .text_color(rgb(INK_SUBTLE))
                            .child(SharedString::from(format!("Thread {}", thread.thread().get()))),
                    ),
            );
        }
        Some(list.into_any_element())
    }
}
