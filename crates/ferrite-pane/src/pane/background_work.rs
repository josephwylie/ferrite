//! The lines at the transcript's foot (T3's `ThreadNowLine`, ADR 0038):
//! the last thing in the transcript, directly above the Composer. The
//! background-work line stands there while the Thread has work running in
//! the background; over it, the one that needs you stands while a Delegate
//! waits on the operator (T3's `childInputNowItem`). Each is a line of
//! muted words, not a banner: an icon, the title (`Waiting on 2
//! subagents`), what it reads, and its actions (`Details`, `Stop`).
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
    /// A watch that goes on of its own, a PR watch (T3's `monitor`).
    Monitor,
}

impl WorkKind {
    fn noun(self, count: usize) -> &'static str {
        match (self, count) {
            (Self::Subagent, 1) => "subagent",
            (Self::Subagent, _) => "subagents",
            (Self::Command, 1) => "command",
            (Self::Command, _) => "commands",
            (Self::Monitor, 1) => "monitor",
            (Self::Monitor, _) => "monitors",
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
    /// What it is doing now, where it says (a subagent's progress).
    pub activity: Option<String>,
}

/// What the line says of `tasks`, before it is drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Presentation {
    /// `Terminal` when every task is a command, else `CircleDashed`.
    pub icon: &'static str,
    pub title: String,
    /// What the work is doing (T3's): a lone task's activity; of several,
    /// the first with one as `label: activity`, else their labels.
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
            description: activity(only).map(str::to_owned),
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
    let busy = ordered
        .iter()
        .find_map(|task| activity(task).map(|doing| format!("{}: {doing}", task.label.trim())));
    Some(Presentation {
        icon,
        title: format!("{verb}{on} {joined}"),
        description: busy.or_else(|| (!labels.is_empty()).then(|| labels.join(", "))),
    })
}

/// What `task` says it is doing, on one line.
fn activity(task: &BackgroundTask) -> Option<&str> {
    task.activity.as_deref().map(str::trim).filter(|doing| !doing.is_empty())
}

/// The background-work line ([`now_line`]), its description in words.
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
    now_line(
        "background-work",
        key,
        NowLine {
            icon,
            needs_you: false,
            title,
            description: description.map(|description| {
                div()
                    .debug_selector(|| "background-work-detail".into())
                    .min_w_0()
                    .truncate()
                    .child(SharedString::from(description))
                    .into_any_element()
            }),
        },
        actions,
    )
}

/// What one line at the transcript's foot says (T3's `ThreadNowItem`).
pub struct NowLine {
    pub icon: &'static str,
    /// It needs the operator: its icon wears the info ink, as T3 inks the
    /// only line at the timeline's end that needs you.
    pub needs_you: bool,
    pub title: String,
    pub description: Option<AnyElement>,
}

/// One line (T3's `ThreadNowLine`): icon, title and description in 12px
/// muted words that give way to the actions at the right (so `Stop` stays
/// reachable), fading in over `motion::LINE_IN`, named `{name}-line`.
/// `actions` are inline buttons or a clock.
pub fn now_line(name: &'static str, key: u64, line: NowLine, actions: Vec<AnyElement>) -> impl IntoElement {
    let NowLine {
        icon,
        needs_you,
        title,
        description,
    } = line;
    let icon_ink = if needs_you { theme::BLUE } else { INK_SUBTLE };
    let line = div()
        .debug_selector(move || format!("{name}-line"))
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
                .child(icons::icon(icon, theme::BACKGROUND_LINE_ICON, icon_ink)),
        )
        .child(
            div()
                .flex()
                .min_w_0()
                .gap(px(theme::SPACE_1))
                .child(div().min_w_0().truncate().child(SharedString::from(title)))
                .children(description.map(|description| {
                    div()
                        .flex()
                        .min_w_0()
                        .opacity(theme::BACKGROUND_LINE_DETAIL_OPACITY)
                        .child(description)
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
        ElementId::Name(SharedString::from(format!("{name}-line-{key}"))),
        line,
        motion::LINE_IN,
        1.0,
    )
}

/// An inline action through the shared press (ADR 0044, T3's
/// `InlineButton`): `inline_button`'s words in `ink` on a clear ground,
/// underlined under the pointer, one line of the thread-state line tall
/// (a kit button keeps its own height otherwise). The caller wires the
/// press.
pub fn inline_action(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    ink: u32,
    cx: &gpui::App,
) -> gpui::component::button::Button {
    let clear: gpui::Hsla = gpui::rgba(0).into();
    ferrite_ui::components::faded_button(id, clear, clear, clear, rgb(ink).into(), cx)
        .h(px(theme::LH_META))
        .p_0()
        .child(
            div()
                .text_size(px(theme::FS_BADGE))
                .line_height(px(theme::LH_META))
                .font_weight(theme::W_LABEL)
                .text_color(rgb(ink))
                .hover(|style| style.underline())
                .child(label.into()),
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
            activity: None,
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

    /// T3's description: a lone task's activity (a subagent's progress);
    /// of several, the first with an activity as `label: activity`, else
    /// the labels.
    #[test]
    fn the_description_is_what_the_work_is_doing() {
        let mut agent = task(WorkKind::Subagent, "Run sleep command");
        agent.activity = Some("Running python".into());
        let alone = present(std::slice::from_ref(&agent), true).unwrap();
        assert_eq!(alone.title, "Waiting \u{b7} Run sleep command");
        assert_eq!(alone.description.as_deref(), Some("Running python"));
        let several = present(&[task(WorkKind::Command, "cargo test"), agent], true).unwrap();
        assert_eq!(
            several.description.as_deref(),
            Some("Run sleep command: Running python")
        );
    }
}
