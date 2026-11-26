use ferrite_core::settings::ContextCardItem;
use ferrite_pane::cell::DetailsOpen;
use ferrite_pane::details::content::{changes_tally, pr_block, tokens, Action, Content, Row};
use ferrite_pane::details::lineage::Lineage;
use ferrite_ui::icons;

use super::*;

impl CockpitView {
    /// The card's content for `thread` while its Pane shows `subject`.
    pub(in crate::cockpit) fn details_content(
        &self,
        thread: ThreadKey,
        subject: &ferrite_core::activity::Subject,
        previous_pages: usize,
        cx: &gpui::App,
    ) -> Content {
        use super::super::surfaces::SurfaceId;
        use ContextCardItem as Item;
        let mut rows = Vec::new();
        let bridge = self.cockpit.read(cx).bridge(thread.machine());
        let mut environment = Row::new(
            Item::Environment,
            bridge.map_or_else(|| "Local".to_string(), |machine| machine.name().to_string()),
            icons::machine(bridge.is_none()),
        );
        // "Local" says this computer: a linked machine's own checkout is
        // just its checkout.
        environment.meta = self.facts.get(thread).map(|facts| {
            if facts.worktree.is_some() {
                "Worktree"
            } else if bridge.is_some() {
                "Checkout"
            } else {
                "Local checkout"
            }
            .to_string()
        });
        rows.push(environment);
        if let Some(path) = self.thread_path(thread, cx) {
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
            if !self.elsewhere(thread.machine(), cx) {
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
            let block = pr_block(pr, self.facts.watched_pr(thread));
            let mut row = Row::new(Item::Branch, block.title.clone(), block.glyph);
            row.tooltip = Some(block.tooltip.clone());
            row.action = Some(Action::Surface(DetailsOpen::PullRequest(pr.number)));
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
                row.changes = facts
                    .status
                    .as_ref()
                    .and_then(|status| status.changes.as_ref())
                    .map(changes_tally);
                row.tooltip = row.changes.as_ref().map(|tally| tally.against.clone());
                if self
                    .opener_unavailable(thread, panel::Opener::Changes, cx)
                    .is_none()
                {
                    row.action = Some(Action::Surface(DetailsOpen::Changes));
                }
                rows.push(row);
            }
        }
        let mut lineage = Lineage::default();
        let mut provider = Provider::Claude;
        if let Some(open) = self.cockpit.read(cx).thread(thread) {
            provider = open.provider();
            if self
                .prefs
                .settings
                .context_card_preferences
                .visible(Item::Lineage)
            {
                let facts = self.facts.get(thread);
                let nothing = Default::default();
                lineage = super::lineage::lineage(
                    open.activity(),
                    subject,
                    &self.facts.name(thread),
                    super::lineage::Elsewhere {
                        delegating: facts.map_or(&nothing, |facts| &facts.delegating),
                        delegated_from: facts
                            .and_then(|facts| facts.delegated_from.as_ref())
                            .map(|from| self.delegator(from, cx)),
                        serving: |server: &ferrite_core::server::ServerId| self.serving(server, cx),
                    },
                );
            }
            if let Some(surfaces) = self.panel.surfaces.get(thread) {
                for surface in &surfaces.tabs {
                    if let SurfaceId::SideChat(tab) = surface {
                        let mut row = Row::new(
                            Item::SideChats,
                            self.panel.side_chat.read(cx).title(thread, *tab).to_string(),
                            icons::MESSAGES_SQUARE,
                        );
                        if self.side_chat_running(thread, *tab, cx) {
                            row.meta = Some("Running".to_string());
                        }
                        row.action = Some(Action::Surface(DetailsOpen::SideChat(*tab)));
                        rows.push(row);
                    }
                }
            }
            let limits = self
                .cockpit
                .read(cx)
                .account_limits(thread.machine(), open.provider());
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
