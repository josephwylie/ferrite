//! Published PR presentation over the existing Changes operations and revision marks.
use super::*;
use gpui::component::checkbox::Checkbox;
use theme::*;

impl CockpitView {
    pub(super) fn pr_diff_head(
        &self,
        diff: Diff,
        changes: &Changes,
        cx: &mut Context<Self>,
    ) -> Div {
        let files = match &changes.load {
            Load::Read(files) => Some(files.as_slice()),
            _ => None,
        };
        let all_folded = files.is_some_and(|files| {
            !files.is_empty() && files.iter().all(|file| changes.folded.contains(&file.path))
        });
        let visible: Vec<_> = files
            .into_iter()
            .flatten()
            .filter(|file| !changes.hide_tests || !is_test_path(&file.path))
            .collect();
        let total = visible.len();
        let reviewed = visible
            .iter()
            .filter(|file| changes.reviewed.get(&file.path) == Some(&file.revision))
            .count();
        let hidden = files.map_or(0, |files| files.len() - total);
        let tool = |id: &'static str,
                    glyph: &'static str,
                    label: &'static str,
                    pressed: bool,
                    verb: Verb| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex()
                .items_center()
                .justify_center()
                .size(px(28.))
                .rounded(px(6.))
                .when(pressed, |tool| tool.bg(paint::SELECTED))
                .hover_control(id)
                .tooltip(ferrite_ui::menu::tooltip(label))
                .child(icons::icon(glyph, 14., INK_SUBTLE))
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.run_changes_verb(diff, verb, cx);
                }))
        };
        let metadata = div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(12.))
            // All-commit content is supported; individual commit selection is not.
            .child(
                div()
                    .debug_selector(|| "pr-code-scope".into())
                    .flex_shrink_0()
                    .child("All commits"),
            )
            .when(files.is_some(), |left| {
                left.child(
                    div()
                        .debug_selector(move || format!("pr-code-files-{total}"))
                        .flex_shrink_0()
                        .child(SharedString::from(format!(
                            "{total} {}",
                            if total == 1 { "file" } else { "files" }
                        ))),
                )
                .when(total > 0, |left| {
                    left.child(
                        div()
                            .id("pr-code-viewed-count")
                            .debug_selector(move || format!("pr-code-viewed-{reviewed}-{total}"))
                            .flex_shrink_0()
                            .tooltip(ferrite_ui::menu::tooltip(
                                "Viewed marks are kept in this Ferrite panel, not on the host",
                            ))
                            .child(SharedString::from(format!("{reviewed} / {total} viewed"))),
                    )
                })
                .when(hidden > 0, |left| {
                    left.child(
                        div()
                            .id("pr-code-hidden-tests")
                            .debug_selector(|| "pr-code-hidden-tests".into())
                            .cursor_pointer()
                            .text_color(rgb(LINK_INK))
                            .child(SharedString::from(format!(
                                "{hidden} test {} hidden",
                                if hidden == 1 { "file" } else { "files" }
                            )))
                            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                                view.run_changes_verb(diff, Verb::ToggleTests, cx)
                            })),
                    )
                })
            });
        let tools = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(4.))
            .child(tool(
                "changes-whitespace",
                icons::PILCROW,
                "Hide whitespace changes",
                changes.hide_whitespace,
                Verb::ToggleWhitespace,
            ))
            .child(tool(
                "changes-wrap",
                icons::TEXT_WRAP,
                "Wrap lines",
                changes.wrap,
                Verb::ToggleWrap,
            ))
            .child(tool(
                "changes-fold-all",
                icons::COLLAPSE_ALL,
                if all_folded {
                    "Expand all files"
                } else {
                    "Collapse all files"
                },
                false,
                if all_folded {
                    Verb::ExpandAll
                } else {
                    Verb::CollapseAll
                },
            ))
            .child(tool(
                "changes-hide-tests",
                icons::FLASK_OFF,
                "Hide test files",
                changes.hide_tests,
                Verb::ToggleTests,
            ))
            .child(
                div()
                    .id("changes-more")
                    .debug_selector(|| "changes-more".into())
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(28.))
                    .rounded(px(6.))
                    .hover_control("changes-more")
                    .tooltip(ferrite_ui::menu::tooltip("Diff options"))
                    .child(icons::icon(icons::ELLIPSIS, 14., INK_SUBTLE))
                    .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.open_context_menu(MenuTarget::ChangesMore(diff), event.position(), cx);
                    })),
            );
        div()
            .debug_selector(|| "changes-toolbar".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .h(px(40.))
            .px(px(16.))
            .gap(px(8.))
            .border_b(px(1.))
            .border_color(crate::cockpit::panel::pull_request::soft_line())
            .text_size(px(12.))
            .text_color(rgb(INK_SUBTLE))
            .child(metadata)
            .child(tools)
    }

    pub(super) fn pr_diff_file_head(
        &self,
        diff: Diff,
        changes: &Changes,
        at: usize,
        file: &File,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let folded = changes.folded.contains(&file.path);
        let reviewed = changes.reviewed.get(&file.path) == Some(&file.revision);
        let path = file.path.clone();
        let review_path = file.path.clone();
        let key = format!("changes-reviewed-{at}");
        div()
            .id(SharedString::from(format!("changes-file-{at}")))
            .debug_selector(move || format!("changes-file-{at}"))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(12.))
            .h(px(32.))
            .px(px(16.))
            .text_size(px(12.))
            .cursor_pointer()
            .on_click(
                cx.listener(move |view, _: &ClickEvent, _, cx| view.toggle_fold(diff, &path, cx)),
            )
            .child(ferrite_ui::components::chevron_at(
                if folded { 0. } else { 1. },
                14.,
                INK_SUBTLE,
            ))
            .child(file_status(file.kind))
            .child(
                div()
                    .debug_selector(move || format!("pr-code-path-{at}"))
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(if reviewed { INK_SUBTLE } else { INK_STRONG }))
                    .child(SharedString::from(file.path.clone())),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .gap(px(4.))
                    .font_family(FONT_CODE)
                    .child(
                        div()
                            .text_color(rgb(GREEN))
                            .child(SharedString::from(format!("+{}", file.added))),
                    )
                    .child(
                        div()
                            .debug_selector(move || format!("pr-code-deletions-{at}"))
                            .text_color(rgb(RED))
                            .child(SharedString::from(format!("−{}", file.removed))),
                    ),
            )
            .child(
                div()
                    .id(("changes-viewed", at))
                    .debug_selector(move || format!("changes-reviewed-{at}"))
                    .flex_shrink_0()
                    .tooltip(ferrite_ui::menu::tooltip(
                        "Viewed marks are kept in this Ferrite panel, not on the host",
                    ))
                    .child(
                        Checkbox::new(SharedString::from(key))
                            .checked(reviewed)
                            .accessibility_label(format!("Viewed {}", file.path))
                            .text_size(px(12.))
                            .text_color(rgb(INK_SUBTLE))
                            .items_center()
                            .child("Viewed")
                            .on_click(cx.listener(move |view, _: &bool, _, cx| {
                                cx.stop_propagation();
                                view.toggle_reviewed(diff, &review_path, cx);
                            })),
                    ),
            )
    }
}

fn file_status(kind: ferrite_reviewr::model::ChangeKind) -> Div {
    use ferrite_reviewr::model::ChangeKind;
    let (glyph, ink) = match kind {
        ChangeKind::Modified => (None, BLUE),
        ChangeKind::Added | ChangeKind::Untracked => (Some(icons::PLUS), GREEN),
        ChangeKind::Deleted => (None, RED),
        ChangeKind::Renamed => (Some(icons::ARROW_RIGHT), INK_SUBTLE),
        ChangeKind::Copied => (Some(icons::COPY), INK_SUBTLE),
    };
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(14.))
        .rounded(px(4.))
        .border_1()
        .border_color(rgb(ink))
        .when(kind == ChangeKind::Modified, |status| {
            status.child(div().size(px(4.)).rounded_full().bg(rgb(ink)))
        })
        .when(kind == ChangeKind::Deleted, |status| {
            status.child(div().w(px(6.)).h(px(1.)).bg(rgb(ink)))
        })
        .children(glyph.map(|glyph| icons::icon(glyph, 10., ink)))
}
