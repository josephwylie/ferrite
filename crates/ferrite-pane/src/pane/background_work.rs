//! The background-work line (T3's `ThreadNowLine`, ADR 0038): the last
//! thing in the transcript, directly above the Composer, while the Thread
//! has work running in the background. It is a line of muted words, not a
//! banner: an icon, the title (`Waiting on 2 subagents`), what the work
//! reads, and its actions (`Details`, `Stop`).
use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, Div, ElementId, SharedString, Stateful};
use std::time::Instant;

use ferrite_ui::theme::{self, INK_STRONG, INK_SUBTLE};
use ferrite_ui::{icons, motion};

/// What a piece of background work is. Declaration order is display order
/// (T3's `BACKGROUND_WORK_KINDS`): subagents first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkKind {
    Subagent,
    Command,
}

impl WorkKind {
    fn noun(self, count: usize) -> &'static str {
        match (self, count) {
            (Self::Subagent, 1) => "subagent",
            (Self::Subagent, _) => "subagents",
            (Self::Command, 1) => "command",
            (Self::Command, _) => "commands",
        }
    }
}

/// One piece of background work as the line names it: `label` is what the
/// provider called it, or what the call ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackgroundTask {
    pub kind: WorkKind,
    pub label: String,
    /// When it started, where Ferrite knows (the clock counts from it).
    pub launched: Option<Instant>,
}

/// What the line says of `tasks`, before it is drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Presentation {
    /// `Terminal` when every task is a command, else `CircleDashed`.
    pub icon: &'static str,
    pub title: String,
    /// The labels, when several tasks share the title.
    pub description: Option<String>,
}

/// T3's `presentPendingBackgroundWork`. `waiting`: the work holds the
/// Thread's turn open (no turn runs of its own), so the line says
/// `Waiting`, not `Running`. None when nothing runs.
pub fn present(tasks: &[BackgroundTask], waiting: bool) -> Option<Presentation> {
    let mut ordered: Vec<&BackgroundTask> = tasks.iter().collect();
    ordered.sort_by_key(|task| task.kind);
    if ordered.is_empty() {
        return None;
    }
    let icon = if ordered.iter().all(|task| task.kind == WorkKind::Command) {
        icons::TERMINAL
    } else {
        icons::CIRCLE_DASHED
    };
    let verb = if waiting { "Waiting" } else { "Running" };
    if let [only] = ordered[..] {
        let label = only.label.trim();
        let title = match (label.is_empty(), waiting) {
            (false, _) => format!("{verb} \u{b7} {label}"),
            (true, true) => format!("Waiting on a {}", only.kind.noun(1)),
            (true, false) => format!("Running a {}", only.kind.noun(1)),
        };
        return Some(Presentation {
            icon,
            title,
            description: None,
        });
    }
    let mut counts: Vec<(WorkKind, usize)> = Vec::new();
    for task in &ordered {
        match counts.last_mut() {
            Some((kind, count)) if *kind == task.kind => *count += 1,
            _ => counts.push((task.kind, 1)),
        }
    }
    let parts: Vec<String> = counts
        .iter()
        .map(|(kind, count)| format!("{count} {}", kind.noun(*count)))
        .collect();
    let joined = match parts.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => parts.join(""),
    };
    let on = if waiting { " on" } else { "" };
    let labels: Vec<&str> = ordered
        .iter()
        .map(|task| task.label.trim())
        .filter(|label| !label.is_empty())
        .collect();
    Some(Presentation {
        icon,
        title: format!("{verb}{on} {joined}"),
        description: (!labels.is_empty()).then(|| labels.join(", ")),
    })
}

/// The line: icon, title and description in 12px muted words that give way
/// to the actions at the right (so `Stop` stays reachable), fading in over
/// `motion::LINE_IN`. `actions` are inline buttons ([`inline_button`]) or
/// a clock.
pub fn background_work_line(
    key: u64,
    presented: Presentation,
    actions: Vec<AnyElement>,
) -> impl IntoElement {
    let Presentation {
        icon,
        title,
        description,
    } = presented;
    let line = div()
        .debug_selector(|| "background-work-line".into())
        .flex()
        .flex_shrink_0()
        .items_baseline()
        .gap(px(theme::BACKGROUND_LINE_GAP))
        .w_full()
        .min_w_0()
        .px(px(theme::BACKGROUND_LINE_PAD_X))
        .pb(px(theme::BACKGROUND_LINE_PAD_B))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_BADGE))
        .line_height(px(theme::LH_META))
        .whitespace_nowrap()
        .text_color(rgb(INK_SUBTLE))
        .child(
            div()
                .flex_shrink_0()
                .self_center()
                .child(icons::icon(icon, theme::BACKGROUND_LINE_ICON, INK_SUBTLE)),
        )
        .child(
            div()
                .flex()
                .min_w_0()
                .gap(px(theme::SPACE_1))
                .child(div().min_w_0().truncate().child(SharedString::from(title)))
                .children(description.map(|description| {
                    div()
                        .debug_selector(|| "background-work-detail".into())
                        .min_w_0()
                        .truncate()
                        .opacity(theme::BACKGROUND_LINE_DETAIL_OPACITY)
                        .child(SharedString::from(description))
                })),
        )
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_baseline()
                .gap(px(theme::BACKGROUND_LINE_ACTIONS_GAP))
                .ml(px(theme::SPACE_1_5))
                .children(actions),
        );
    motion::fade_in(
        ElementId::Name(SharedString::from(format!("background-work-line-{key}"))),
        line,
        motion::LINE_IN,
        1.0,
    )
}

/// An inline action on a thread-state line (`Details`, `Stop`): medium
/// weight, foreground, underlined under the pointer. The caller wires the
/// press.
pub fn inline_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .cursor_pointer()
        .font_weight(theme::W_LABEL)
        .text_color(rgb(INK_STRONG))
        .hover(|style| style.underline())
        .child(label.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(kind: WorkKind, label: &str) -> BackgroundTask {
        BackgroundTask {
            kind,
            label: label.into(),
            launched: None,
        }
    }

    #[test]
    fn several_tasks_are_counted_by_kind_subagents_first() {
        let tasks = [
            task(WorkKind::Command, "cargo test"),
            task(WorkKind::Subagent, "Review"),
            task(WorkKind::Subagent, "Explore"),
        ];
        let waiting = present(&tasks, true).unwrap();
        assert_eq!(waiting.title, "Waiting on 2 subagents and 1 command");
        assert_eq!(waiting.description.as_deref(), Some("Review, Explore, cargo test"));
        assert_eq!(waiting.icon, icons::CIRCLE_DASHED);
        let commands = [
            task(WorkKind::Command, "a"),
            task(WorkKind::Command, "b"),
        ];
        assert_eq!(present(&commands, false).unwrap().title, "Running 2 commands");
    }

    #[test]
    fn one_named_task_reads_waiting_dot_label_and_an_unnamed_one_its_kind() {
        let named = [task(WorkKind::Command, "npm run dev")];
        let presented = present(&named, true).unwrap();
        assert_eq!(presented.title, "Waiting \u{b7} npm run dev");
        assert_eq!(presented.icon, icons::TERMINAL);
        assert_eq!(presented.description, None);
        assert_eq!(
            present(&[task(WorkKind::Subagent, " ")], true).unwrap().title,
            "Waiting on a subagent"
        );
        assert_eq!(
            present(&[task(WorkKind::Subagent, "")], false).unwrap().title,
            "Running a subagent"
        );
        assert_eq!(present(&[], true), None);
    }
}
