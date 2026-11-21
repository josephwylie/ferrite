use super::*;
use ferrite_core::settings::ContextCardItem;
use ferrite_pane::details::Density;
use ferrite_ui::icons::{self, icon};

#[derive(Clone, Debug)]
enum Action {
    Reveal(std::path::PathBuf),
    Surface(super::super::surfaces::SurfaceId),
    Subagent(ferrite_core::activity::Subject),
    CreatePr,
}

#[derive(Clone, Debug)]
struct Row {
    item: ContextCardItem,
    label: String,
    meta: Option<String>,
    /// The pull request's block, which this row draws instead of a label.
    pr: Option<PrBlock>,
    glyph: &'static str,
    action: Option<Action>,
    fill: Option<f32>,
    tooltip: Option<String>,
}

impl Row {
    fn new(item: ContextCardItem, label: impl Into<String>, glyph: &'static str) -> Self {
        Self {
            item,
            label: label.into(),
            meta: None,
            pr: None,
            glyph,
            action: None,
            fill: None,
            tooltip: None,
        }
    }

    fn shown_at(&self, density: Density) -> bool {
        shown_at(self.item, density)
            && (self.item != ContextCardItem::GitActions || density == Density::Full)
    }

    /// The row's height: a pull request block with its quiet line counts
    /// its title at two lines.
    fn height(&self) -> f32 {
        match &self.pr {
            Some(block) if block.mark.is_some() => {
                PR_TITLE_TOP + PR_TITLE_LH * 2. + PR_TITLE_BOTTOM + PR_META_H + PR_GAP_BELOW
            }
            _ => 32.,
        }
    }
}

/// The pull request block's measures (T3's `leading-4.5`, `h-5`, `pb-1.5`).
const PR_TITLE_TOP: f32 = 7.;
const PR_TITLE_BOTTOM: f32 = 2.;
const PR_TITLE_LH: f32 = 18.;
const PR_META_H: f32 = 20.;
const PR_GAP_BELOW: f32 = 6.;

pub(super) struct Content {
    rows: Vec<Row>,
}

pub(super) fn shown_at(item: ContextCardItem, density: Density) -> bool {
    use ContextCardItem::*;
    match item {
        Environment | Servers | Automations | Lineage | SideChats => density == Density::Full,
        OpenIn => density != Density::Essential,
        Scripts | Branch | GitActions | Changes | Usage | Context => true,
    }
}

fn section(item: ContextCardItem) -> u8 {
    use ContextCardItem::*;
    match item {
        Environment | OpenIn | Scripts => 0,
        Branch | GitActions | Changes => 1,
        Servers => 2,
        Automations => 3,
        Lineage => 4,
        SideChats => 5,
        Usage => 6,
        Context => 7,
    }
}

impl Content {
    pub(super) fn scrolling_height(&self, density: Density) -> f32 {
        self.rows
            .iter()
            .filter(|row| {
                row.shown_at(density)
                    && (matches!(
                        row.item,
                        ContextCardItem::Lineage | ContextCardItem::SideChats
                    ) || matches!(
                        row.action,
                        Some(Action::Surface(
                            super::super::surfaces::SurfaceId::PullRequest(_)
                        ))
                    ))
            })
            .map(Row::height)
            .sum()
    }

    pub(super) fn key(&self, thread: ThreadId, width: f32, density: Density) -> String {
        format!(
            "{}:{width}:{:?}",
            thread.get(),
            self.rows
                .iter()
                .filter(|row| row.shown_at(density))
                .collect::<Vec<_>>()
        )
    }

    pub(super) fn render(
        &self,
        thread: ThreadId,
        density: Density,
        cx: &mut Context<CockpitView>,
    ) -> Div {
        let rows: Vec<_> = self
            .rows
            .iter()
            .filter(|row| row.shown_at(density))
            .collect();
        let mut content = div()
            .flex()
            .flex_col()
            .text_size(px(13.))
            .font_weight(W_LABEL);
        if rows.is_empty() {
            return content.p(px(8.)).child(
                div()
                    .id("details-choose-items")
                    .debug_selector(|| "details-choose-items".into())
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .h(px(32.))
                    .px(px(10.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgba(0xffffff14)))
                    .child(icon(icons::GEAR, 16., INK_SUBTLE))
                    .child("Choose what to show")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.open_context_menu(MenuTarget::Details(thread), event.position, cx);
                        }),
                    ),
            );
        }
        for group in 0..8 {
            let group_rows: Vec<_> = rows
                .iter()
                .copied()
                .filter(|row| section(row.item) == group)
                .collect();
            if group_rows.is_empty() {
                continue;
            }
            let divided = group > 1 || (group == 1 && density == Density::Full);
            let mut block = div()
                .flex()
                .flex_col()
                .px(px(8.))
                .pt(px(8.))
                .pb(px(10.))
                .when(
                    divided && rows.iter().any(|row| section(row.item) < group),
                    |block| block.border_t(px(1.)).border_color(rgba(0xffffff1a)),
                );
            if group == 4 || group == 5 {
                block = block.child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(32.))
                        .px(px(10.))
                        .text_size(px(11.))
                        .text_color(rgb(INK_SUBTLE))
                        .child(if group == 4 { "Lineage" } else { "Side chats" }),
                );
            }
            for (at, row) in group_rows.iter().enumerate() {
                let selector = format!("details-item-{}-{at}", row.item.id());
                let action = row.action.clone();
                let usage = matches!(row.item, ContextCardItem::Usage | ContextCardItem::Context);
                let control = div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .min_w_0()
                    .px(px(10.))
                    .rounded(px(8.))
                    .text_color(rgba(0xffffffcc));
                let mut control = match &row.pr {
                    Some(block) => pr_view(control, block),
                    None => control
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .h(px(if usage { 24. } else { 32. }))
                        .when(!usage, |control| {
                            control.child(icon(row.glyph, 16., INK_SUBTLE))
                        })
                        .child(
                            div()
                                .min_w_0()
                                .when(!usage, |label| label.flex_1())
                                .truncate()
                                .when(usage, |label| {
                                    label.text_color(rgb(INK_SUBTLE)).text_size(px(12.))
                                })
                                .child(row.label.clone()),
                        ),
                };
                if let Some(fill) = row.fill {
                    control = control.child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w(px(24.))
                            .h(px(3.))
                            .rounded(px(2.))
                            .bg(rgba(0xffffff33))
                            .child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top_0()
                                    .h_full()
                                    .w(gpui::relative(fill.clamp(0., 1.)))
                                    .rounded(px(2.))
                                    .bg(rgba(0xffffffbb)),
                            ),
                    );
                }
                if let Some(meta) = &row.meta {
                    control = control.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(12.))
                            .text_color(rgb(INK_SUBTLE))
                            .child(meta.clone()),
                    );
                }
                if let Some(tooltip) = &row.tooltip {
                    control = control.tooltip(ferrite_ui::menu::tooltip(tooltip.clone()));
                }
                if let Some(action) = action {
                    control = control
                        .cursor_pointer()
                        .hover(|style| style.bg(rgba(0xffffff14)))
                        .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            match &action {
                                Action::Reveal(path) => {
                                    ferrite_pane::preview::open_original(path, window, cx)
                                }
                                Action::Surface(surface) => {
                                    view.upsert_surface(thread, surface.clone(), cx)
                                }
                                Action::Subagent(subject) => {
                                    view.open_subagent(thread, subject.clone(), cx)
                                }
                                Action::CreatePr => view.create_pr(thread, cx),
                            }
                        }));
                }
                block = block.child(control);
            }
            content = content.child(block);
        }
        content
    }
}

impl CockpitView {
    pub(super) fn details_content(&self, thread: ThreadId, cx: &gpui::App) -> Content {
        use super::super::surfaces::SurfaceId;
        use ContextCardItem as Item;
        let mut rows = Vec::new();
        let mut environment = Row::new(
            Item::Environment,
            self.cockpit
                .machine()
                .map_or_else(|| "Local".to_string(), |machine| machine.name().to_string()),
            icons::LAPTOP,
        );
        environment.meta = self.facts.get(thread).map(|facts| {
            if facts.worktree.is_some() {
                "Worktree"
            } else {
                "Local checkout"
            }
            .to_string()
        });
        rows.push(environment);
        if let Some(path) = self.thread_path(thread) {
            let mut folder = Row::new(
                Item::Environment,
                path.file_name().map_or_else(
                    || path.to_string_lossy().into_owned(),
                    |name| name.to_string_lossy().into_owned(),
                ),
                icons::FOLDER,
            );
            folder.tooltip = Some(path.to_string_lossy().into_owned());
            folder.action = Some(Action::Reveal(path.clone()));
            rows.push(folder);
            if !self.on_machine() {
                let label = if cfg!(target_os = "macos") {
                    "Open in Finder"
                } else if cfg!(target_os = "windows") {
                    "Open in File Explorer"
                } else {
                    "Open in file manager"
                };
                let mut open_in = Row::new(Item::OpenIn, label, icons::FOLDER);
                open_in.tooltip = Some(path.to_string_lossy().into_owned());
                open_in.action = Some(Action::Reveal(path));
                rows.push(open_in);
            }
        }
        if let Some(branch) = self.thread_branch(thread) {
            let mut row = Row::new(Item::Branch, branch.to_string(), icons::BRANCH);
            row.tooltip = Some(branch.to_string());
            rows.push(row);
        }
        if let Some(pr) = self.facts.pr(thread) {
            let block = pr_block(pr);
            let mut row = Row::new(Item::Branch, block.title.clone(), block.glyph);
            row.tooltip = Some(block.tooltip.clone());
            row.action = Some(Action::Surface(SurfaceId::PullRequest(pr.number)));
            row.pr = Some(block);
            rows.push(row);
        }
        if let Some(facts) = self.facts.get(thread) {
            if self.thread_branch(thread).is_some() {
                if self.facts.pr(thread).is_none()
                    && facts.status.as_ref().is_some_and(|status| status.dirty > 0)
                {
                    let mut create =
                        Row::new(Item::GitActions, "Create PR", icons::GIT_PULL_REQUEST);
                    create.tooltip = Some(
                        "Ask the agent to commit, push, and create a pull request".to_string(),
                    );
                    create.action = Some(Action::CreatePr);
                    rows.push(create);
                }
                let mut row = Row::new(Item::Changes, "Changes", icons::DIFF);
                row.meta = facts.status.as_ref().map(|status| match status.dirty {
                    0 => "No changes".to_string(),
                    1 => "1 file".to_string(),
                    count => format!("{count} files"),
                });
                if self
                    .opener_unavailable(thread, panel::Opener::Changes)
                    .is_none()
                {
                    row.action = Some(Action::Surface(SurfaceId::Changes));
                }
                rows.push(row);
            }
        }
        if let Some(open) = self.cockpit.thread(thread) {
            for agent in open.activity().children() {
                let mut row = Row::new(
                    Item::Lineage,
                    agent
                        .info()
                        .name
                        .clone()
                        .or_else(|| agent.info().description.clone())
                        .unwrap_or_else(|| "Agent".to_string()),
                    icons::SUBAGENTS,
                );
                row.meta = Some(format!("{:?}", agent.status()));
                row.action = Some(Action::Subagent(agent.subject()));
                rows.push(row);
            }
            if let Some(surfaces) = self.panel.surfaces.get(thread) {
                for surface in &surfaces.tabs {
                    if let SurfaceId::SideChat(tab) = surface {
                        let mut row = Row::new(
                            Item::SideChats,
                            self.side_chat.read(cx).title(thread, *tab).to_string(),
                            icons::MESSAGES_SQUARE,
                        );
                        if self.side_chat_running(thread, *tab, cx) {
                            row.meta = Some("Running".to_string());
                        }
                        row.action = Some(Action::Surface(surface.clone()));
                        rows.push(row);
                    }
                }
            }
            let limits = self.cockpit.account_limits(open.provider());
            for (name, limit) in [("5 hours", limits.five_hour), ("Weekly", limits.weekly)] {
                if let Some(limit) = limit {
                    let mut row = Row::new(Item::Usage, name, icons::CLOCK);
                    let left = (1. - limit.used_fraction).clamp(0., 1.);
                    row.fill = Some(left);
                    row.meta = Some(format!("{:.0}%", left * 100.));
                    row.tooltip = Some(format!("{name}: {:.0}% left", left * 100.));
                    rows.push(row);
                }
            }
            if let Some(usage) = open.transcript().usage() {
                let mut row = Row::new(Item::Context, "Context", icons::DETAILS);
                row.meta = Some(usage.context_window.filter(|max| *max > 0).map_or_else(
                    || tokens(usage.total_tokens),
                    |max| format!("{} / {}", tokens(usage.total_tokens), tokens(max)),
                ));
                row.fill = usage
                    .context_window
                    .filter(|max| *max > 0)
                    .map(|max| usage.total_tokens as f32 / max as f32);
                row.tooltip = Some(format!("{} tokens used", usage.total_tokens));
                rows.push(row);
            }
        }
        rows.retain(|row| {
            self.prefs
                .settings
                .context_card_preferences
                .visible(row.item)
        });
        Content { rows }
    }
}

/// The card's pull request block, drawn as T3 Code draws it: the state's
/// glyph and the title (two lines at most), and under it one quiet line,
/// `#N` and then the mark. With no mark, one row: `#N: title`.
#[derive(Clone, Debug, PartialEq)]
struct PrBlock {
    number: u64,
    title: String,
    glyph: &'static str,
    ink: u32,
    mark: Option<PrMark>,
    /// T3's hover: the title in full and `#N`, the state, then the checks
    /// in words while it is open.
    tooltip: String,
}

/// Draws a [`PrBlock`] into its row's control: the glyph and the title
/// over the quiet line, or with no mark one row, `#N: title`.
fn pr_view(control: gpui::Stateful<Div>, block: &PrBlock) -> gpui::Stateful<Div> {
    let glyph = icon(block.glyph, 16., block.ink);
    let Some(mark) = &block.mark else {
        return control
            .flex()
            .items_center()
            .gap(px(10.))
            .h(px(32.))
            .child(glyph)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .child(format!("#{}: {}", block.number, block.title)),
            );
    };
    let words = mark.words.clone().map(|words| {
        div()
            .min_w_0()
            .truncate()
            .when(mark.glyph.is_none(), |words| {
                words.text_color(rgb(mark.ink))
            })
            .child(words)
    });
    control
        .flex()
        .flex_col()
        .pb(px(PR_GAP_BELOW))
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(10.))
                .pt(px(PR_TITLE_TOP))
                .pb(px(PR_TITLE_BOTTOM))
                .child(div().flex().items_center().h(px(PR_TITLE_LH)).child(glyph))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .line_height(px(PR_TITLE_LH))
                        .line_clamp(2)
                        .text_ellipsis()
                        .child(block.title.clone()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .h(px(PR_META_H))
                .pl(px(26.))
                .text_size(px(12.))
                .text_color(rgb(INK_SUBTLE))
                .child(
                    div()
                        .flex_shrink_0()
                        .mr(px(4.))
                        .child(format!("#{}", block.number)),
                )
                .children(mark.glyph.map(|ring| icon(ring, 14., mark.ink)))
                .children(words),
        )
}

/// What the quiet line says after `#N`: the checks' ring and count, or a
/// conflict in red. `ink` paints the ring, or the words when there is no
/// ring; a count's words stay dim.
#[derive(Clone, Debug, PartialEq)]
struct PrMark {
    glyph: Option<&'static str>,
    ink: u32,
    words: Option<String>,
}

/// The block a pull request draws as (see [`PrBlock`]), as T3 Code's
/// `ThreadDetailsPrRow` decides it: conflicts replace the checks, a draft
/// shows none, and a pull request with no checks has no quiet line.
fn pr_block(pr: &ferrite_core::workspace::PullRequest) -> PrBlock {
    use ferrite_core::workspace::PrState;
    let title = pr
        .title
        .clone()
        .unwrap_or_else(|| format!("Pull request #{}", pr.number));
    let (glyph, ink) = match (pr.state, pr.draft) {
        (PrState::Open, false) => (icons::GIT_PULL_REQUEST, GREEN),
        (PrState::Open, true) => (icons::GIT_PULL_REQUEST_DRAFT, INK_SUBTLE),
        (PrState::Closed, _) => (icons::GIT_PULL_REQUEST_CLOSED, RED),
        (PrState::Merged, _) => (icons::GIT_MERGE, MAGENTA),
    };
    let open = pr.state == PrState::Open;
    let checks = Checks::of(pr);
    let mark = if open && pr.conflicting {
        Some(PrMark {
            glyph: None,
            ink: RED,
            words: Some("Conflicts".to_string()),
        })
    } else if pr.draft {
        None
    } else {
        checks.mark()
    };
    let mut tooltip = format!("{title} #{}\n{}", pr.number, crate::pr::state_word(pr));
    if let Some(words) = checks.words().filter(|_| open) {
        tooltip = format!("{tooltip}\n{words}");
    }
    PrBlock {
        number: pr.number,
        title,
        glyph,
        ink,
        mark,
        tooltip,
    }
}

/// A pull request's runs counted as T3 Code counts them: a run awaiting
/// approval has not finished, and has not failed.
#[derive(Clone, Copy, Default)]
struct Checks {
    total: u32,
    passed: u32,
    failed: u32,
    running: u32,
    awaiting: u32,
}

impl Checks {
    fn of(pr: &ferrite_core::workspace::PullRequest) -> Self {
        use ferrite_core::workspace::CheckState;
        let mut checks = Self::default();
        for run in &pr.runs {
            checks.total += 1;
            match run.state {
                _ if run.detail == "action_required" => checks.awaiting += 1,
                CheckState::Passing => checks.passed += 1,
                CheckState::Failing => checks.failed += 1,
                CheckState::Pending => checks.running += 1,
                CheckState::Skipped => {}
            }
        }
        checks
    }

    /// The ring and count: `5/6 checks` while any has not finished, else
    /// `N failed`, else the ring alone. No ring with nothing passed,
    /// failed or unfinished.
    fn mark(self) -> Option<PrMark> {
        let unfinished = self.running + self.awaiting;
        let (glyph, ink) = if self.failed > 0 {
            (icons::X_CIRCLE, RED)
        } else if unfinished > 0 {
            (icons::CIRCLE_DOT, YELLOW)
        } else if self.passed > 0 {
            (icons::CHECK_CIRCLE, GREEN)
        } else {
            return None;
        };
        let words = if unfinished > 0 {
            Some(format!("{}/{} checks", self.total - unfinished, self.total))
        } else if self.failed > 0 {
            Some(format!("{} failed", self.failed))
        } else {
            None
        };
        Some(PrMark {
            glyph: Some(glyph),
            ink,
            words,
        })
    }

    /// The checks in words, for the hover; `None` with no runs.
    fn words(self) -> Option<String> {
        let total = self.total;
        if total == 0 {
            return None;
        }
        let mut parts = Vec::new();
        if self.running > 0 {
            parts.push(format!("{} of {total} running", self.running));
        }
        if self.awaiting > 0 {
            parts.push(format!("{} of {total} awaiting action", self.awaiting));
        }
        if self.failed > 0 {
            parts.push(if parts.is_empty() {
                format!("{} of {total} failing", self.failed)
            } else {
                format!("{} failed", self.failed)
            });
        }
        Some(if !parts.is_empty() {
            parts.join(" · ")
        } else if self.passed == total {
            "All checks passed".to_string()
        } else {
            format!("{} of {total} passing", self.passed)
        })
    }
}

fn tokens(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.)
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_keeps_workspace_actions_and_usage_at_the_source_levels() {
        use ContextCardItem::*;
        let visible = |density| {
            ContextCardItem::ALL
                .into_iter()
                .filter(|item| shown_at(*item, density))
                .collect::<Vec<_>>()
        };
        assert_eq!(visible(Density::Full), ContextCardItem::ALL);
        assert_eq!(
            visible(Density::Compact),
            [OpenIn, Scripts, Branch, GitActions, Changes, Usage, Context]
        );
        assert_eq!(
            visible(Density::Essential),
            [Scripts, Branch, GitActions, Changes, Usage, Context]
        );
    }

    /// The card draws the Thread's pull request as T3 Code does: one
    /// block, the title over a quiet line of `#N` and the checks' ring and
    /// count (`5/6 checks` while any run, `N failed` once they settle,
    /// the ring alone when all passed). A draft shows no checks; a
    /// conflict says so in red where T3 offers Resolve; with nothing for
    /// that line, one row, `#N: title`.
    #[test]
    fn details_card_draws_the_pull_request_as_one_block() {
        use CheckState::*;
        use ferrite_core::workspace::{Check, CheckState, PrState, PullRequest};
        let run = |(state, detail): (CheckState, &str)| Check {
            name: "ci".into(),
            workflow: None,
            state,
            detail: detail.into(),
            url: None,
        };
        let pr = |runs: &[(CheckState, &str)]| PullRequest {
            number: 418,
            state: PrState::Open,
            draft: false,
            url: None,
            title: Some("feat(details): the card".into()),
            checks: None,
            runs: runs.iter().copied().map(run).collect(),
            conflicting: false,
        };
        let line = |block: &PrBlock| {
            block
                .mark
                .as_ref()
                .map(|mark| (mark.glyph, mark.ink, mark.words.clone()))
        };
        let words = |words: &str| Some(words.to_string());
        let passed = (Passing, "success");
        let running = (Pending, "in_progress");
        let failed = (Failing, "failure");
        let awaiting = (Failing, "action_required");
        let skipped = (Skipped, "skipped");

        let block = pr_block(&pr(&[passed, passed, passed, passed, passed, running]));
        assert_eq!(
            (block.number, block.title.as_str(), block.glyph, block.ink),
            (
                418,
                "feat(details): the card",
                icons::GIT_PULL_REQUEST,
                GREEN
            )
        );
        assert_eq!(
            line(&block),
            Some((Some(icons::CIRCLE_DOT), YELLOW, words("5/6 checks"))),
            "checks running: the amber ring and the settled count"
        );
        assert_eq!(
            block.tooltip,
            "feat(details): the card #418\nOpen\n1 of 6 running"
        );

        let block = pr_block(&pr(&[passed, passed, passed, passed, failed, failed]));
        assert_eq!(
            line(&block),
            Some((Some(icons::X_CIRCLE), RED, words("2 failed"))),
            "some failed, the rest settled"
        );
        assert_eq!(
            block.tooltip,
            "feat(details): the card #418\nOpen\n2 of 6 failing"
        );

        let block = pr_block(&pr(&[passed, failed, running]));
        assert_eq!(
            line(&block),
            Some((Some(icons::X_CIRCLE), RED, words("2/3 checks"))),
            "a failure while another still runs: the red ring, still counting"
        );
        assert_eq!(
            block.tooltip,
            "feat(details): the card #418\nOpen\n1 of 3 running · 1 failed"
        );

        let block = pr_block(&pr(&[passed, passed, passed, passed, passed, awaiting]));
        assert_eq!(
            line(&block),
            Some((Some(icons::CIRCLE_DOT), YELLOW, words("5/6 checks"))),
            "a run awaiting approval has not finished"
        );
        assert_eq!(
            block.tooltip,
            "feat(details): the card #418\nOpen\n1 of 6 awaiting action"
        );

        let block = pr_block(&pr(&[passed, passed, skipped]));
        assert_eq!(
            line(&block),
            Some((Some(icons::CHECK_CIRCLE), GREEN, None)),
            "all passed: the green ring alone"
        );
        assert_eq!(
            block.tooltip,
            "feat(details): the card #418\nOpen\n2 of 3 passing"
        );
        let block = pr_block(&pr(&[passed, passed]));
        assert_eq!(
            block.tooltip,
            "feat(details): the card #418\nOpen\nAll checks passed"
        );

        for runs in [&[][..], &[skipped][..]] {
            let block = pr_block(&pr(runs));
            assert_eq!(line(&block), None, "no checks to count: one row");
        }

        let mut draft = pr(&[passed, running]);
        draft.draft = true;
        let block = pr_block(&draft);
        assert_eq!(
            (block.glyph, block.ink),
            (icons::GIT_PULL_REQUEST_DRAFT, INK_SUBTLE)
        );
        assert_eq!(line(&block), None, "a draft shows no checks");
        assert_eq!(
            block.tooltip,
            "feat(details): the card #418\nDraft\n1 of 2 running"
        );

        let mut clash = pr(&[passed, running]);
        clash.conflicting = true;
        assert_eq!(
            line(&pr_block(&clash)),
            Some((None, RED, words("Conflicts"))),
            "conflicts replace the checks"
        );

        let mut merged = pr(&[passed]);
        merged.state = PrState::Merged;
        let block = pr_block(&merged);
        assert_eq!((block.glyph, block.ink), (icons::GIT_MERGE, MAGENTA));
        assert_eq!(line(&block), Some((Some(icons::CHECK_CIRCLE), GREEN, None)));
        assert_eq!(block.tooltip, "feat(details): the card #418\nMerged");

        let mut closed = pr(&[]);
        closed.state = PrState::Closed;
        closed.title = None;
        let block = pr_block(&closed);
        assert_eq!(
            (block.glyph, block.ink),
            (icons::GIT_PULL_REQUEST_CLOSED, RED)
        );
        assert_eq!(block.title, "Pull request #418");
    }

    #[test]
    fn actual_rendered_values_and_width_invalidate_density_measurements() {
        let thread = ThreadId::new(7);
        let mut content = Content {
            rows: vec![
                Row::new(ContextCardItem::Environment, "Local", icons::LAPTOP),
                Row::new(ContextCardItem::Context, "Context", icons::DETAILS),
            ],
        };
        let full = content.key(thread, 280., Density::Full);
        let compact = content.key(thread, 280., Density::Compact);
        content.rows[0].label = "Remote build server".into();
        assert_ne!(full, content.key(thread, 280., Density::Full));
        assert_eq!(compact, content.key(thread, 280., Density::Compact));
        content.rows[1].meta = Some("128k / 200k".into());
        assert_ne!(compact, content.key(thread, 280., Density::Compact));
        assert_ne!(
            content.key(thread, 280., Density::Full),
            content.key(thread, 220., Density::Full)
        );
    }
}
