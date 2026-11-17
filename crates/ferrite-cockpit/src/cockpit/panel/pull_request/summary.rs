//! T3's Summary: metadata and collapsible sections share one scroll.
mod scroll;

use std::cell::Cell;
use std::rc::Rc;

use super::super::pr_read::{COMMENT_PAGE, CommentFilter, PrSummary, SummarySection, comment_page};
use super::*;
use ferrite_ui::theme::*;

impl CockpitView {
    pub(super) fn pr_summary(
        &self,
        target: PrTarget,
        pr: &WirePr,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self
            .panel
            .pr_reads
            .get(target)
            .map_or_else(PrSummary::default, |read| read.summary);
        let scroll = self
            .panel
            .pr_reads
            .get(target)
            .map(|read| read.summary_scroll.clone())
            .unwrap_or_default();
        let page = comment_page(pr, self.panel.pr_reads.comment_filters, state);
        let description = if pr.body.trim().is_empty() {
            "_No description provided._"
        } else {
            &pr.body
        };
        let checks = div()
            .children(pr.checks.iter().enumerate().map(|(index, check)| {
                let (glyph, ink, status) = match check.status {
                    WireCheckStatus::Success => (icons::CHECK_CIRCLE, GREEN, "Passed"),
                    WireCheckStatus::Failure => (icons::X_CIRCLE, RED, "Failed"),
                    WireCheckStatus::Running => (icons::CIRCLE_DASHED, YELLOW, "Running"),
                    WireCheckStatus::Pending => (icons::CIRCLE_DASHED, YELLOW, "Pending"),
                    WireCheckStatus::Skipped => (icons::CIRCLE_MINUS, INK_SUBTLE, "Skipped"),
                    WireCheckStatus::Unknown => (icons::CIRCLE_MINUS, INK_SUBTLE, "Unknown"),
                };
                div()
                    .id(("pr-check", index))
                    .debug_selector(move || format!("pr-check-{index}"))
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .px(px(8.))
                    .py(px(8.))
                    .rounded(px(6.))
                    .text_size(px(12.))
                    .line_height(px(20.))
                    .child(icon(glyph, 14., ink))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(SharedString::from(check.name.clone())),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(rgb(INK_SUBTLE))
                            .child(status),
                    )
                    .when_some(check.url.clone(), |row, url| {
                        row.cursor_pointer()
                            .hover(|row| row.bg(paint::RAISED))
                            .on_click(
                                cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&url)),
                            )
                    })
            }))
            .when(pr.checks.is_empty(), |checks| {
                checks.child(summary_note("No checks reported."))
            })
            .when(pr.more_checks(), |checks| {
                checks.child(pr_more(
                    "pr-more-checks",
                    "More checks than are shown. See all on the forge",
                    &pr.url,
                    cx,
                ))
            });

        let hidden = page.hidden_comments;
        let actions = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(
                summary_button(
                    "pr-comments-filter",
                    Some(icons::LIST_FILTER),
                    if hidden > 0 {
                        format!("{hidden} hidden")
                    } else {
                        "Filter".into()
                    },
                )
                .tooltip(ferrite_ui::menu::tooltip("Filter comments"))
                .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
                    view.open_context_menu(
                        MenuTarget::PrCommentFilter(target),
                        event.position(),
                        cx,
                    );
                })),
            )
            .child(
                summary_button(
                    "pr-comments-order",
                    Some(icons::ARROW_DOWN_UP),
                    if state.oldest_first {
                        "Oldest first"
                    } else {
                        "Newest first"
                    },
                )
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    if let Some(summary) = view.panel.pr_reads.summary_mut(target) {
                        summary.oldest_first = !summary.oldest_first;
                        cx.notify();
                    }
                })),
            )
            .into_any_element();
        let notice = pr.more_comments().then(|| {
            pr_more(
                "pr-more-comments",
                "This conversation is only partially loaded. Open it on the host to read the rest.",
                &pr.url,
                cx,
            )
        });
        let older = page.older_conversations;
        let more = || {
            summary_button(
                "pr-comments-older",
                None,
                format!(
                    "Show {} older conversation{} ({older} hidden)",
                    older.min(COMMENT_PAGE),
                    if older == 1 { "" } else { "s" }
                ),
            )
            .w_full()
            .justify_center()
            .border_1()
            .border_color(soft_line())
            .py(px(6.))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                if let Some(summary) = view.panel.pr_reads.summary_mut(target) {
                    summary.shown += COMMENT_PAGE;
                    cx.notify();
                }
            }))
        };
        let comments = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .children(notice)
            .when(pr.comments.is_empty(), |comments| {
                comments.child(summary_note("No comments yet."))
            })
            .when(
                !pr.comments.is_empty() && page.entries.is_empty(),
                |comments| {
                    comments.child(summary_note(if hidden == 1 {
                        "The only comment is hidden by the filter.".into()
                    } else {
                        format!("All {hidden} comments are hidden by the filter.")
                    }))
                },
            )
            .when(state.oldest_first && older > 0, |comments| {
                comments.child(more())
            })
            .children(
                page.entries
                    .iter()
                    .map(|(index, comment)| self.pr_comment(target, *index, comment)),
            )
            .when(!state.oldest_first && older > 0, |comments| {
                comments.child(more())
            })
            .when(state.shown > COMMENT_PAGE, |comments| {
                comments.child(
                    summary_button(
                        "pr-comments-recent",
                        None,
                        "Show only 10 recent conversations",
                    )
                    .w_full()
                    .justify_center()
                    .on_click(cx.listener(
                        move |view, _: &ClickEvent, _, cx| {
                            if let Some(summary) = view.panel.pr_reads.summary_mut(target) {
                                summary.shown = COMMENT_PAGE;
                                cx.notify();
                            }
                        },
                    )),
                )
            });

        pr_scroll("pr-summary")
            .track_scroll(&scroll)
            .px(px(0.))
            .py(px(0.))
            .child(pr_metadata(pr))
            .child(
                self.pr_summary_section(
                    target,
                    SummarySection::Description,
                    "Description".into(),
                    None,
                    div()
                        .debug_selector(|| "pr-body".into())
                        .child(self.pr_prose(format!("pr-body-{target:?}"), description))
                        .into_any_element(),
                    cx,
                ),
            )
            .child(self.pr_summary_section(
                target,
                SummarySection::Checks,
                "Checks".into(),
                None,
                checks.into_any_element(),
                cx,
            ))
            .child(self.pr_summary_section(
                target,
                SummarySection::Comments,
                format!("Comments ({})", page.total_comments),
                Some(actions),
                comments.into_any_element(),
                cx,
            ))
            .into_any_element()
    }

    fn pr_summary_section(
        &self,
        target: PrTarget,
        section: SummarySection,
        title: String,
        actions: Option<AnyElement>,
        body: AnyElement,
        cx: &mut Context<Self>,
    ) -> Div {
        let open = self
            .panel
            .pr_reads
            .get(target)
            .map_or_else(PrSummary::default, |read| read.summary)
            .is_open(section);
        let id = match section {
            SummarySection::Description => "pr-section-description",
            SummarySection::Checks => "pr-section-checks",
            SummarySection::Comments => "pr-section-comments",
        };
        let scroll = self
            .panel
            .pr_reads
            .get(target)
            .map(|read| read.summary_scroll.clone())
            .unwrap_or_default();
        let heading_top = Rc::new(Cell::new(None));
        let drawn_top = heading_top.clone();
        let collapse_scroll = scroll.clone();
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(scroll::StickyHeading {
                scroll,
                section,
                top: drawn_top,
                child: Some(
                    div()
                        .flex()
                        .items_center()
                        .bg(rgb(TX_GROUND))
                        .pr(px(16.))
                        .child(
                            div()
                                .id(id)
                                .debug_selector(move || id.into())
                                .flex()
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .gap(px(6.))
                                .px(px(16.))
                                .py(px(12.))
                                .text_size(px(12.))
                                .font_weight(W_LABEL)
                                .text_color(rgb(INK_SUBTLE))
                                .cursor_pointer()
                                .hover(|row| row.text_color(rgb(INK_STRONG)))
                                .child(SharedString::from(title))
                                .child(icon(
                                    if open {
                                        icons::CHEVRON_DOWN
                                    } else {
                                        icons::CHEVRON_RIGHT
                                    },
                                    14.,
                                    INK_SUBTLE,
                                ))
                                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                                    if let Some(summary) = view.panel.pr_reads.summary_mut(target) {
                                        if summary.is_open(section) {
                                            scroll::anchor_before_collapse(
                                                &collapse_scroll,
                                                section,
                                                heading_top.get(),
                                            );
                                        }
                                        summary.toggle(section);
                                        cx.notify();
                                    }
                                })),
                        )
                        .children(actions)
                        .into_any_element(),
                ),
            })
            .when(open, |section| {
                section.child(div().px(px(16.)).pb(px(16.)).child(body))
            })
    }

    pub(in crate::cockpit) fn pr_comment_filter_rows(&self) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let filters = self.panel.pr_reads.comment_filters;
        vec![
            Some((
                menu::Item::new("Bot comments").checked(filters.bots),
                MenuVerb::PrCommentFilter(CommentFilter::Bots),
            )),
            Some((
                menu::Item::new("Resolved or dismissed").checked(filters.resolved),
                MenuVerb::PrCommentFilter(CommentFilter::Resolved),
            )),
        ]
    }
}

fn summary_note(text: impl Into<SharedString>) -> Div {
    div()
        .py(px(8.))
        .text_size(px(12.))
        .text_color(rgb(INK_SUBTLE))
        .child(text.into())
}

fn summary_button(
    id: &'static str,
    glyph: Option<&'static str>,
    text: impl Into<SharedString>,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex()
        .items_center()
        .gap(px(4.))
        .px(px(6.))
        .py(px(4.))
        .rounded(px(6.))
        .text_size(px(12.))
        .text_color(rgb(INK_SUBTLE))
        .cursor_pointer()
        .hover(|button| button.bg(paint::RAISED).text_color(rgb(INK_STRONG)))
        .children(glyph.map(|glyph| icon(glyph, 12., INK_SUBTLE)))
        .child(text.into())
}

fn pr_metadata(pr: &WirePr) -> Div {
    let reviewers = div()
        .flex()
        .items_center()
        .flex_wrap()
        .children(pr.reviewers.as_ref().into_iter().flatten().enumerate().map(
            |(index, reviewer)| {
                let verdict = reviewer.verdict.and_then(crate::pr::review_verdict);
                let name = reviewer
                    .name
                    .as_ref()
                    .filter(|name| *name != &reviewer.login)
                    .map_or_else(
                        || reviewer.login.clone(),
                        |name| format!("{name} (@{})", reviewer.login),
                    );
                let title =
                    verdict.map_or(name.clone(), |(verdict, _)| format!("{name}: {verdict}"));
                div()
                    .id(("pr-reviewer", index))
                    .debug_selector(move || format!("pr-reviewer-{index}"))
                    .relative()
                    .when(index > 0, |avatar| avatar.ml(px(-4.)))
                    .rounded_full()
                    .border_1()
                    .border_color(verdict.map_or(rgba(TRANSPARENT), |(_, ink)| rgb(ink)))
                    .tooltip(ferrite_ui::menu::tooltip(title))
                    .child(pr_avatar(&reviewer.login, reviewer.avatar_url.as_deref()))
            },
        ))
        .when(pr.reviewers.as_ref().is_none_or(Vec::is_empty), |row| {
            row.text_color(rgb(INK_SUBTLE))
                .child(if pr.reviewers.is_some() {
                    "None"
                } else {
                    "Unavailable"
                })
        });
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .px(px(16.))
        .pt(px(10.))
        .pb(px(4.))
        .flex_shrink_0()
        .child(metadata_row(
            icons::USERS,
            "Reviewers",
            reviewers.into_any_element(),
        ))
        .when_some(pr.labels.as_ref(), |metadata, labels| {
            let chips = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(4.))
                .when(labels.is_empty(), |row| {
                    row.text_color(rgb(INK_SUBTLE)).child("None")
                })
                .children(labels.iter().enumerate().map(|(index, label)| {
                    let color = label
                        .color
                        .as_deref()
                        .filter(|color| color.len() == 6)
                        .and_then(|color| u32::from_str_radix(color, 16).ok());
                    div()
                        .id(("pr-label", index))
                        .debug_selector(move || format!("pr-label-{index}"))
                        .max_w(px(192.))
                        .truncate()
                        .h(px(18.))
                        .px(px(3.))
                        .rounded(px(2.))
                        .font_weight(W_LABEL)
                        .border_1()
                        .border_color(rgba(TRANSPARENT))
                        .bg(color.map_or(rgb(RAISED), |color| rgba((color << 8) | 0x1f)))
                        .text_color(color.map_or(rgb(INK_STRONG), label_ink))
                        .when_some(label.description.clone(), |chip, description| {
                            chip.tooltip(ferrite_ui::menu::tooltip(description))
                        })
                        .child(SharedString::from(label.name.clone()))
                }));
            metadata.child(metadata_row(icons::TAG, "Labels", chips.into_any_element()))
        })
}

fn metadata_row(glyph: &'static str, label: &'static str, value: AnyElement) -> Div {
    div()
        .debug_selector(move || format!("pr-meta-{}", label.to_ascii_lowercase()))
        .flex()
        .items_center()
        .min_h(px(24.))
        .gap(px(8.))
        .text_size(px(12.))
        .child(
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .w(px(96.))
                .gap(px(6.))
                .text_color(rgb(INK_SUBTLE))
                .child(icon(glyph, 14., INK_SUBTLE))
                .child(label),
        )
        .child(div().flex_1().min_w_0().child(value))
}

/// T3's dark label ink mixes 45% label RGB with the foreground.
fn label_ink(color: u32) -> gpui::Rgba {
    let channel = |shift: u32| {
        (((color >> shift) & 255u32) * 45 + ((INK_STRONG >> shift) & 255u32) * 55) / 100
    };
    rgb((channel(16) << 16) | (channel(8) << 8) | channel(0))
}
