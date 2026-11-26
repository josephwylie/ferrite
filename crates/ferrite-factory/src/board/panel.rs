//! The open Task's panel beside the Board's columns or the Archive's list:
//! its status, Stop, Resume, Delete and Close, its tabs, and the tab shown:
//! the Overview with the Task's say and pull request, Activity, Context,
//! and its Worker tab, which draws the Worker's Pane the window lays out,
//! an earlier Worker read, or the Workers it had.

use ferrite_core::factory_board::{self, Status, Task};
use ferrite_core::protocol::TaskAction;
use ferrite_core::ThreadKey;
use ferrite_pane::cell::CellSlot;
use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, ClickEvent, Context, SharedString};

use super::{status_ink, FactoryEvent, FactoryView, TaskTab, TASK_PANEL_W};

/// What the open Task's panel draws from the window.
pub struct TaskPanel {
    /// The Task, as the board says it now.
    pub task: Task,
    /// On its Worker tab, what its Workers say; none while no agent works
    /// on it.
    pub worker: Option<WorkerTab>,
}

/// What a Task's Worker tab draws.
pub enum WorkerTab {
    /// Its Worker's own Pane, which this window holds and lays out.
    Pane(CellSlot),
    /// An earlier Worker opened to be read: what was said in its Thread.
    Read(String),
    /// The Workers it had, each Step's and the last, with none working.
    Had(Vec<HadWorker>),
}

/// One Worker a Task had: its Thread and its Step's name.
pub struct HadWorker {
    pub thread: ThreadKey,
    pub step: Option<String>,
}

impl FactoryView {
    /// The open Task's panel: its status, tabs, and the tab's content.
    pub(super) fn task_panel(&self, panel: TaskPanel, cx: &mut Context<Self>) -> AnyElement {
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
                    .children(crate::direction::says(task).then(|| self.leaves.direction.clone()))
                    .children(crate::delivery::task_delivery(task, cx))
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
            TaskTab::Worker => match worker {
                Some(WorkerTab::Pane(slot)) => div()
                    .debug_selector(|| "task-worker".into())
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .p(px(SPACE_2))
                    .child(slot.draw())
                    .into_any_element(),
                Some(WorkerTab::Read(text)) => worker_read(text, cx),
                Some(WorkerTab::Had(workers)) => worker_history(task, workers, cx),
                None => div()
                    .debug_selector(|| "task-worker-none".into())
                    .p(px(SPACE_4))
                    .text_color(rgb(INK_SUBTLE))
                    .child("No agent is working on this task right now.")
                    .into_any_element(),
            },
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
}

/// An earlier Worker's transcript, to read: no Composer, nothing to send.
fn worker_read(text: String, cx: &mut Context<FactoryView>) -> AnyElement {
    use ferrite_ui::theme::*;
    let text = if text.trim().is_empty() {
        "Nothing was said in this agent's Thread.".to_string()
    } else {
        text
    };
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
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                    cx.emit(FactoryEvent::ReadWorker(None))
                })),
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
        .into_any_element()
}

/// The Workers `task` had, each Step's and the last, for a Task with no
/// Worker Pane in this window (a completed one, its Workers parked).
fn worker_history(task: &Task, workers: Vec<HadWorker>, cx: &mut Context<FactoryView>) -> AnyElement {
    use ferrite_ui::theme::*;
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
    for (at, HadWorker { thread, step }) in workers.into_iter().enumerate() {
        let name = step.unwrap_or_else(|| "Agent".to_string());
        list = list.child(
            div()
                .id(SharedString::from(format!("task-worker-history-{at}")))
                .debug_selector(move || format!("task-worker-history-{at}"))
                .cursor_pointer()
                .hover(|style| style.bg(paint::RAISED))
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.emit(FactoryEvent::ReadWorker(Some(thread)))
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
    list.into_any_element()
}
