//! Background work the Session is running — a shell command sent to the
//! background, a background agent, a watch — as chips docked at the
//! Composer's right edge. They ride the same shelf above the Composer as
//! the pending files, on the same chip recipe, so the two read as one
//! surface: files going *in* at the left, work going *on* at the right. A
//! chip names its task and, where the Session can stop it, ends it with one
//! click. A task the provider reports finished leaves the shelf on its own;
//! the shelf itself leaves when nothing is running.
//!
//! The cockpit builds the shelf (its stop wiring needs the view's own
//! `Context`) and the Pane only places it, exactly as with attachments.

use std::rc::Rc;

use ferrite_core::progress::{BackgroundTask, TaskStatus};
use gpui::{
    div, prelude::*, px, rgb, App, ClickEvent, ElementId, IntoElement, SharedString, Window,
};

use crate::icons::{self, icon};
use crate::pointer::Pointer;
use crate::theme;

type Stop = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// The shelf of running background tasks.
#[derive(IntoElement)]
pub struct BackgroundChips {
    id: ElementId,
    tasks: Vec<BackgroundTask>,
    on_stop: Option<Stop>,
}

impl BackgroundChips {
    /// Only tasks still working are kept: the caller may hand over the
    /// provider's whole list, finished entries included.
    pub fn new(id: impl Into<ElementId>, tasks: impl IntoIterator<Item = BackgroundTask>) -> Self {
        Self {
            id: id.into(),
            tasks: tasks
                .into_iter()
                .filter(|task| task.status == TaskStatus::Working)
                .collect(),
            on_stop: None,
        }
    }

    /// Nothing running — no shelf to draw.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Wire each chip's `×` to end its task. Without this the chips are
    /// read-only, which is what a Session that cannot stop tasks gets.
    pub fn on_stop(mut self, callback: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_stop = Some(Rc::new(callback));
        self
    }
}

impl RenderOnce for BackgroundChips {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let reduce_motion = cx.reduce_motion();
        let mut shelf = div()
            .id(self.id)
            .debug_selector(|| "background-chips".into())
            .flex()
            .flex_wrap()
            .justify_end()
            .items_center()
            .gap(px(theme::SPACE_1_5))
            .min_w_0()
            .max_w_full();
        for (index, task) in self.tasks.into_iter().enumerate() {
            shelf = shelf.child(chip(index, task, reduce_motion, self.on_stop.clone()));
        }
        shelf
    }
}

/// One chip: the shared pulsing `RUNNING` dot, the task's own description
/// cut to the chip's width, and the `×` where stopping is wired. The whole
/// description and the task's kind wait in the tooltip.
fn chip(
    index: usize,
    task: BackgroundTask,
    reduce_motion: bool,
    on_stop: Option<Stop>,
) -> impl IntoElement {
    let kind = kind_label(&task.detail);
    let label: SharedString = if task.label.trim().is_empty() {
        kind.into()
    } else {
        task.label.clone().into()
    };
    let tooltip: SharedString = if task.label.trim().is_empty() {
        format!("{kind} running in the background").into()
    } else {
        format!("{kind} · {}", task.label).into()
    };
    let mut chip = div()
        .id(("background-chip", index))
        .debug_selector(move || format!("background-chip-{index}"))
        .flex()
        .items_center()
        .gap(px(theme::SPACE_1_5))
        .min_w_0()
        .max_w(px(theme::BG_CHIP_MAX_W))
        .h(px(theme::CHIP_H))
        .px(px(theme::CHIP_PAD_X))
        .rounded(px(theme::R_CHIP))
        .bg(rgb(theme::FILL))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(theme::TEXT_2))
        .tooltip(move |window, cx| {
            gpui::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .child(crate::components::pulsing_dot(
            ("background-chip-pulse", index),
            theme::RUNNING,
            theme::RUNNING_HALO,
            reduce_motion,
        ))
        .child(div().min_w_0().truncate().child(label));
    if let Some(stop) = on_stop {
        let id = task.id.clone();
        chip = chip.child(
            div()
                .id(("background-chip-stop", index))
                .debug_selector(move || format!("background-chip-stop-{index}"))
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .size(px(theme::BG_CHIP_STOP))
                .rounded(px(theme::R_TIGHT))
                .hover_carried()
                .child(icon(
                    icons::CLOSE,
                    theme::BG_CHIP_STOP_GLYPH,
                    theme::TEXT_MUTED,
                ))
                .on_click(move |_: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    stop(&id, window, cx);
                }),
        );
    }
    chip
}

/// The provider's task kind in the operator's words. Claude names its
/// kinds `local_bash`, `local_agent`, `monitor`, `workflow`, …; an unknown
/// kind is shown as the provider wrote it rather than hidden.
fn kind_label(detail: &str) -> &str {
    match detail {
        "local_bash" | "bash" | "shell" => "shell",
        "local_agent" | "agent" | "subagent" => "agent",
        "remote_agent" => "remote agent",
        "monitor" => "monitor",
        "workflow" => "workflow",
        "" => "task",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, status: TaskStatus) -> BackgroundTask {
        BackgroundTask {
            id: id.into(),
            label: format!("task {id}"),
            status,
            detail: "local_bash".into(),
        }
    }

    #[test]
    fn only_working_tasks_make_chips() {
        let chips = BackgroundChips::new(
            "chips",
            [
                task("1", TaskStatus::Working),
                task("2", TaskStatus::Completed),
                task("3", TaskStatus::Failed),
                task("4", TaskStatus::Stopped),
                task("5", TaskStatus::Unknown),
            ],
        );
        assert_eq!(chips.tasks.len(), 1);
        assert_eq!(chips.tasks[0].id, "1");
        assert!(!chips.is_empty());
        assert!(BackgroundChips::new("none", [task("2", TaskStatus::Completed)]).is_empty());
    }

    #[test]
    fn kinds_read_in_plain_words() {
        assert_eq!(kind_label("local_bash"), "shell");
        assert_eq!(kind_label("local_agent"), "agent");
        assert_eq!(kind_label("monitor"), "monitor");
        assert_eq!(kind_label(""), "task");
        assert_eq!(kind_label("something_new"), "something_new");
    }
}
