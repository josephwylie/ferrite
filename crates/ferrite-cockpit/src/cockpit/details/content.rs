use super::lineage::Lineage;
use super::*;
use ferrite_core::settings::ContextCardItem;
use ferrite_pane::details::Density;
use ferrite_ui::icons::{self, icon};

#[derive(Clone, Debug)]
enum Action {
    Reveal(std::path::PathBuf),
    Surface(super::super::surfaces::SurfaceId),
    CreatePr,
}

#[derive(Clone, Debug)]
struct Row {
    item: ContextCardItem,
    label: String,
    meta: Option<String>,
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
}

pub(super) struct Content {
    rows: Vec<Row>,
    /// The shown Subject's Lineage, drawn in its own section.
    lineage: Lineage,
    provider: Provider,
    /// The Thread's `Previous agents` fold
    /// (`ThreadSurfaces::previous_agents_open`).
    previous_open: bool,
    /// Its `Show more` presses (`DetailsCard::previous_pages`).
    previous_pages: usize,
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
    fn lineage_shown(&self, density: Density) -> bool {
        shown_at(ContextCardItem::Lineage, density) && !self.lineage.is_empty()
    }

    pub(super) fn scrolling_height(&self, density: Density) -> f32 {
        let lineage = if self.lineage_shown(density) {
            self.lineage.rows(self.previous_open, self.previous_pages)
        } else {
            0
        };
        (self
            .rows
            .iter()
            .filter(|row| {
                row.shown_at(density)
                    && (row.item == ContextCardItem::SideChats
                        || matches!(
                            row.action,
                            Some(Action::Surface(
                                super::super::surfaces::SurfaceId::PullRequest(_)
                            ))
                        ))
            })
            .count()
            + lineage) as f32
            * 32.
    }

    pub(super) fn key(&self, thread: ThreadId, width: f32, density: Density) -> String {
        format!(
            "{}:{width}:{:?}:{:?}",
            thread.get(),
            self.lineage_shown(density)
                .then_some((&self.lineage, self.previous_open, self.previous_pages)),
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
        let lineage = self.lineage_shown(density);
        if rows.is_empty() && !lineage {
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
            let group_lineage = group == 4 && lineage;
            if group_rows.is_empty() && !group_lineage {
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
                    divided
                        && (rows.iter().any(|row| section(row.item) < group)
                            || (lineage && group > 4)),
                    |block| block.border_t(px(1.)).border_color(rgba(0xffffff1a)),
                );
            if group_lineage {
                content = content.child(self.lineage.render(
                    block,
                    thread,
                    self.provider,
                    self.previous_open,
                    self.previous_pages,
                    cx,
                ));
                continue;
            }
            if group == 5 {
                block = block.child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(32.))
                        .px(px(10.))
                        .text_size(px(11.))
                        .text_color(rgb(INK_SUBTLE))
                        .child("Side chats"),
                );
            }
            for (at, row) in group_rows.iter().enumerate() {
                let selector = format!("details-item-{}-{at}", row.item.id());
                let action = row.action.clone();
                let usage = matches!(row.item, ContextCardItem::Usage | ContextCardItem::Context);
                let mut control = div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .flex()
                    .items_center()
                    .min_w_0()
                    .gap(px(10.))
                    .h(px(if usage { 24. } else { 32. }))
                    .px(px(10.))
                    .rounded(px(8.))
                    .text_color(rgba(0xffffffcc))
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
                    );
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
    /// The card's content for `thread` while its Pane shows `subject`.
    pub(super) fn details_content(
        &self,
        thread: ThreadId,
        subject: &ferrite_core::activity::Subject,
        previous_pages: usize,
        cx: &gpui::App,
    ) -> Content {
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
            let mut row = Row::new(
                Item::Branch,
                pr.title
                    .clone()
                    .unwrap_or_else(|| format!("Pull request #{}", pr.number)),
                icons::GIT_PULL_REQUEST,
            );
            row.meta = Some(format!("#{}", pr.number));
            row.tooltip = Some(format!(
                "{:?}{}",
                pr.state,
                if pr.draft { " · Draft" } else { "" }
            ));
            row.action = Some(Action::Surface(SurfaceId::PullRequest(pr.number)));
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
        let mut lineage = Lineage::default();
        let mut provider = Provider::Claude;
        if let Some(open) = self.cockpit.thread(thread) {
            provider = open.provider();
            if self
                .prefs
                .settings
                .context_card_preferences
                .visible(Item::Lineage)
            {
                lineage =
                    super::lineage::lineage(open.activity(), subject, &self.facts.name(thread));
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
        Content {
            rows,
            lineage,
            provider,
            previous_open: self
                .panel
                .surfaces
                .get(thread)
                .is_some_and(|state| state.previous_agents_open),
            previous_pages,
        }
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

    #[test]
    fn actual_rendered_values_and_width_invalidate_density_measurements() {
        let thread = ThreadId::new(7);
        let mut content = Content {
            rows: vec![
                Row::new(ContextCardItem::Environment, "Local", icons::LAPTOP),
                Row::new(ContextCardItem::Context, "Context", icons::DETAILS),
            ],
            lineage: Lineage::default(),
            provider: Provider::Claude,
            previous_open: false,
            previous_pages: 0,
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
