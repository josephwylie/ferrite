//! A Project's Archive in the window (#226): Completed Tasks leave the
//! board 7 days after completing, or at once with the board head's Clear
//! completed. An archived Task opens in the ordinary Task panel, read-only,
//! with its activity and the Workers it had.
//!
//! The Archive's list is `ferrite_factory::archive::ArchiveView`, which the
//! Factory holds, so it goes when the Factory leaves the board's place. It
//! reads the archived Tasks that match its search through a `Source` the
//! root makes over itself (`archive_inputs`); what a row or Board asks it
//! says as an `ArchiveEvent`. Whether the Archive shows stays the Factory's
//! (`archive_shown`), which sets the list beside the Task panel; reopening
//! an earlier Worker to be read, and drawing it on the panel's Worker tab,
//! stay the root's.

use super::*;
use super::destination::{Destination, Shows};
use ferrite_core::factory_board::Task;
use ferrite_core::protocol::TaskAction;
use ferrite_core::ThreadKey;
use ferrite_factory::archive::{ArchiveEvent, ArchiveInputs, ArchiveView, ArchivedRow};
use ferrite_ui::source::Source;

impl CockpitView {
    /// A Factory's Archive, heard from its making.
    pub(super) fn new_archive(&mut self, cx: &mut Context<Self>) -> Entity<ArchiveView> {
        let inputs = Source::of(&cx.entity(), |root: &Self, query: &String, cx| root.archive_inputs(query, cx));
        let archive = cx.new(|cx| ArchiveView::new(inputs, cx));
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
