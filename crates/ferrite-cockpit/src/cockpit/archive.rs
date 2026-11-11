//! A Project's Archive in the window (#226): Completed Tasks leave the
//! board 7 days after completing, or at once with the Completed column's
//! Clear. The Archive is searched by title and instructions; an archived
//! Task opens in the ordinary Task panel, read-only, with its activity
//! and the Workers it had.

use super::*;
use ferrite_core::factory::{self as core_factory, Task};
use ferrite_core::protocol::TaskAction;
use ferrite_pane::composer::{Composer, Role};

/// Whether the Archive shows in the board's place, and its search.
#[derive(Default)]
pub(crate) struct Archive {
    pub(crate) shown: bool,
    pub(crate) search: Option<Entity<Composer>>,
}

impl CockpitView {
    /// Show the shown Project's Archive in the board's place.
    pub(crate) fn open_archive(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.factory_view.as_ref() else {
            return;
        };
        if view.archive.search.is_none() {
            let search = cx.new(Composer::new);
            search.update(cx, |field, cx| field.set_role(Role::Field(None), cx));
            cx.observe(&search, |_, _, cx| cx.notify()).detach();
            if let Some(view) = self.factory_view.as_mut() {
                view.archive.search = Some(search);
            }
        }
        if let Some(view) = self.factory_view.as_mut() {
            view.archive.shown = true;
            view.open = None;
        }
        cx.notify();
    }

    pub(crate) fn close_archive(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.factory_view.as_mut() {
            view.archive.shown = false;
            view.open = None;
        }
        cx.notify();
    }

    /// What the Archive is searched for now.
    fn archive_query(&self, cx: &gpui::App) -> String {
        self.factory_view
            .as_ref()
            .and_then(|view| view.archive.search.as_ref())
            .map(|search| search.read(cx).text().to_string())
            .unwrap_or_default()
    }

    /// Replace what the Archive is searched for (tests, and a fresh
    /// search).
    pub(crate) fn set_archive_search(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(search) = self
            .factory_view
            .as_ref()
            .and_then(|view| view.archive.search.clone())
        else {
            return;
        };
        search.update(cx, |field, cx| field.set(text.to_string(), cx));
        cx.notify();
    }

    /// The Archive's search field holds the keyboard while it shows.
    pub(super) fn archive_focus(&self, cx: &gpui::App) -> Option<FocusHandle> {
        let view = self.factory_view.as_ref()?;
        if !view.archive.shown || view.surface != super::workflows::Surface::Board {
            return None;
        }
        Some(view.archive.search.as_ref()?.read(cx).focus_handle(cx))
    }

    /// The Completed column's Clear.
    pub(crate) fn clear_completed(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.factory_view.as_ref().map(|view| view.project) else {
            return;
        };
        self.task_command(TaskAction::Clear { project }, cx);
    }

    /// The Clear in the Completed column's heading.
    pub(super) fn clear_button(&self, cx: &mut Context<Self>) -> AnyElement {
        ferrite_ui::components::ghost_button("factory-clear", "Clear", cx)
            .debug_selector(|| "factory-clear".into())
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.clear_completed(cx);
            }))
            .into_any_element()
    }

    /// The board head's way to the Archive, with how many it holds.
    pub(super) fn archive_button(&self, archived: usize, cx: &mut Context<Self>) -> AnyElement {
        let label = match archived {
            0 => "Archive".to_string(),
            n => format!("Archive \u{b7} {n}"),
        };
        ferrite_ui::components::ghost_button("factory-archive", label, cx)
            .debug_selector(|| "factory-archive".into())
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.open_archive(cx);
            }))
            .into_any_element()
    }

    /// Why the Factory's store could not be read, over the board.
    pub(super) fn factory_trouble(&self) -> Option<AnyElement> {
        use ferrite_ui::theme::*;
        let trouble = self.cockpit.factory().trouble.clone()?;
        Some(
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
                .into_any_element(),
        )
    }

    /// The Archive: its search, and each archived Task that matches,
    /// most recently archived first, the open one's panel beside them.
    pub(super) fn archive_surface(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let Some(view) = self.factory_view.as_ref() else {
            return div();
        };
        let project = view.project;
        let query = self.archive_query(cx);
        let board = self.cockpit.factory();
        let found: Vec<&Task> = board.archive(project, &query).collect();
        let any = board.archive(project, "").next().is_some();
        let open = view.open.as_deref();
        let search = view.archive.search.clone();
        let focused = search
            .as_ref()
            .is_some_and(|field| field.read(cx).focus_handle(cx).is_focused(window));

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
            .child(
                div()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child("Completed tasks, 7 days after they complete or when cleared"),
            )
            .child(div().flex_1())
            .child(
                ferrite_ui::components::ghost_button("factory-archive-close", "Board", cx)
                    .debug_selector(|| "factory-archive-close".into())
                    .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.close_archive(cx);
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
            .children(search.map(|field| {
                div()
                    .debug_selector(|| "factory-archive-search".into())
                    .child(crate::project_editor::text_field(
                        "Search titles and instructions".into(),
                        field,
                        focused,
                        "factory-archive-search-focused",
                    ))
            }));
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
            list = list.child(self.archive_row(task, open == Some(task.id.as_str()), cx));
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_row()
            .child(list)
            .children(self.task_panel(window, cx))
    }

    /// One archived Task: its title, when it completed and was archived,
    /// and its result. Opening it starts nothing.
    fn archive_row(&self, task: &Task, open: bool, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let id = task.id.clone();
        let key = format!("factory-archive-{}", task.id);
        let when = |label: &str, at: Option<i64>| {
            at.map(|at| format!("{label} {}", core_factory::when_label(at)))
        };
        let meta: Vec<String> = [
            Some(format!("#{}", task.id)),
            when("Completed", task.keeping.completed_ms),
            when("Archived", task.keeping.archived_ms),
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
                    .child(SharedString::from(task.title.clone())),
            )
            .child(
                div()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(meta.join(" \u{b7} "))),
            )
            .children(task.result.clone().map(|result| {
                div()
                    .text_size(px(FS_META))
                    .line_clamp(2)
                    .child(SharedString::from(result))
            }))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.open_task(&id, cx)))
            .into_any_element()
    }

    /// The Workers a Task had, each Step's and the last, for a Task with
    /// no Worker Pane in this window (a completed one, its Workers parked).
    pub(super) fn worker_history(&self, task: &Task) -> Option<AnyElement> {
        use ferrite_ui::theme::*;
        let threads = task.every_worker();
        if threads.is_empty() {
            return None;
        }
        let step_of = |thread: ThreadId| {
            let step = task.workers.iter().find(|each| each.thread == thread)?;
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
                    .debug_selector(move || format!("task-worker-history-{at}"))
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
                            .child(SharedString::from(format!("Thread {}", thread.get()))),
                    ),
            );
        }
        Some(list.into_any_element())
    }
}
