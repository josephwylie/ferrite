//! A Task's pull request in its panel (#223): the PR and every revision
//! with who reported it, which revision was approved and whether the
//! current one is, the evidence kept apart by kind (an older revision's
//! said as such), and why a completed Task's worktree stayed.

use super::direction::{meta, section};
use super::*;
use ferrite_core::factory::{self as core_factory, EvidenceKind, Status, Task};

/// The PR's line on a card: `acme/widgets#12 · revision 2`.
pub(crate) fn card_line(task: &Task) -> Option<String> {
    let pr = task.delivery.pr.as_ref()?;
    let mut line = pr.label();
    if let Some(now) = pr.current() {
        line.push_str(&format!(" \u{b7} revision {}", now.revision));
    }
    if pr.merged {
        line.push_str(" \u{b7} merged");
    } else if pr.approval_current() {
        line.push_str(" \u{b7} approved");
    }
    Some(line)
}

/// What Approve is given for: the current revision, said with its head.
pub(crate) fn approving_line(task: &Task) -> Option<String> {
    let pr = task.delivery.pr.as_ref()?;
    Some(match pr.current() {
        Some(now) => format!(
            "You approve {} at revision {} (head {}).",
            pr.label(),
            now.revision,
            core_factory::short(&now.head)
        ),
        None => format!(
            "You approve {}; no revision of it has been reported yet.",
            pr.label()
        ),
    })
}

/// The evidence, each item with its kind, who gave it and its revision;
/// one about an older revision is dimmed and says so.
pub(crate) fn evidence_list(task: &Task) -> Div {
    use ferrite_ui::theme::*;
    let now = task.revision_now();
    let mut list = div()
        .debug_selector(|| "task-evidence".into())
        .flex()
        .flex_col()
        .gap(px(SPACE_1));
    for (at, each) in task.delivery.evidence.iter().enumerate() {
        let older = matches!((each.revision, now), (Some(at), Some(now)) if at < now);
        let kind = match each.kind {
            EvidenceKind::Conclusion => "conclusion",
            EvidenceKind::Tool => "tool",
            EvidenceKind::Decision => "decision",
        };
        let selector = format!(
            "task-evidence-{at}-{kind}{}",
            if older { "-older" } else { "" }
        );
        let about = match (each.revision, now) {
            (Some(at), Some(now)) if at < now => {
                format!("Revision {at} \u{b7} older than revision {now}, not evidence for it")
            }
            (Some(at), _) => format!("Revision {at}"),
            (None, _) => "No revision".into(),
        };
        list = list.child(
            div()
                .debug_selector(move || selector.clone())
                .flex()
                .flex_col()
                .gap(px(SPACE_0_5))
                .child(
                    div()
                        .text_size(px(FS_META))
                        .text_color(rgb(INK_SUBTLE))
                        .child(SharedString::from(format!(
                            "{} \u{b7} {} \u{b7} {about}",
                            each.kind.label(),
                            each.by
                        ))),
                )
                .child(
                    div()
                        .text_color(rgb(if older { INK_SUBTLE } else { INK_STRONG }))
                        .child(SharedString::from(each.text.clone())),
                ),
        );
    }
    list
}

impl CockpitView {
    /// The Overview's PR section, the evidence (Ready to approve shows
    /// its own) and why the worktree stayed.
    pub(super) fn task_delivery(&self, task: &Task, cx: &mut Context<Self>) -> Vec<AnyElement> {
        use ferrite_ui::theme::*;
        let mut out = Vec::new();
        if let Some(pr) = &task.delivery.pr {
            let url = pr.url();
            let link = div()
                .id("task-pr-link")
                .debug_selector(|| "task-pr-link".into())
                .flex()
                .items_center()
                .gap(px(SPACE_1))
                .cursor_pointer()
                .text_color(rgb(INK_STRONG))
                .child(SharedString::from(pr.label()))
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&url)));
            let state = if pr.merged {
                "Merged, as the merge agent confirmed.".to_string()
            } else {
                match (pr.approved, pr.current()) {
                    (Some(approved), Some(now)) if approved == now.revision => {
                        format!("Revision {approved} is approved.")
                    }
                    (Some(approved), Some(now)) => format!(
                        "Revision {approved} was approved; the current revision {} is not.",
                        now.revision
                    ),
                    (None, Some(_)) => "Not approved yet.".into(),
                    (_, None) => "No revision has been reported yet.".into(),
                }
            };
            let mut section = section("task-pr", "Pull request")
                .child(link)
                .child(meta(state).debug_selector(|| "task-pr-state".into()));
            for each in pr.revisions.iter().rev() {
                let approved = pr.approved == Some(each.revision);
                let selector = format!(
                    "task-pr-revision-{}{}",
                    each.revision,
                    if approved { "-approved" } else { "" }
                );
                section = section.child(
                    div()
                        .debug_selector(move || selector.clone())
                        .flex()
                        .gap(px(SPACE_2))
                        .text_size(px(FS_META))
                        .child(div().flex_shrink_0().text_color(rgb(INK_STRONG)).child(
                            SharedString::from(format!(
                                "Revision {} \u{b7} {}",
                                each.revision,
                                core_factory::short(&each.head)
                            )),
                        ))
                        .child(div().flex_1().min_w_0().text_color(rgb(INK_SUBTLE)).child(
                            SharedString::from(format!(
                                "Reported by {}{}",
                                each.by,
                                if approved { " \u{b7} approved" } else { "" }
                            )),
                        )),
                );
            }
            out.push(section.into_any_element());
        }
        if task.status != Status::Approval && !task.delivery.evidence.is_empty() {
            out.push(
                section("task-evidence-section", "Evidence")
                    .child(evidence_list(task))
                    .into_any_element(),
            );
        }
        if task.status == Status::Completed {
            if let Some(kept) = &task.delivery.kept {
                out.push(
                    section("task-kept", "Worktree")
                        .child(div().child(SharedString::from(kept.clone())))
                        .into_any_element(),
                );
            }
        }
        out
    }
}
