//! The board with nothing on it (F-13): the home. One calm column, centred
//! a touch above the middle: the greeting (`components::greeting`), the
//! provider CLIs Ferrite found, the ways to start as rows with their keys,
//! and the four Threads used last.
//!
//! In the chrome's grammar (theme `EMPTY_*`, SPEC §2, §5.8): sentence-case
//! labels, an icon column, keys read from the key table in `INK_SUBTLE`, the
//! selection a rounded `SELECTED` row and the rest the row hover.
//!
//! The rows are a list with its own focus and key context (`EmptyBoard`):
//! `empty_board::Next` / `Previous` move the selection, `Run` does what the
//! row says (the floats package binds ↑ ↓ ⏎). A press does the same.
//! `show_empty_board` shows it while Threads stay open.

use super::*;

actions!(empty_board, [Next, Previous, Run]);

/// The key context the list holds focus under.
pub(crate) const KEY_CONTEXT: &str = "EmptyBoard";

/// One way to start: the action's name in the key table (its chord), its
/// icon, what the row says (the palette's words for the same command), and
/// what it dispatches.
struct StartRow {
    action: &'static str,
    icon: &'static str,
    verb: &'static str,
    run: Box<dyn gpui::Action>,
}

impl StartRow {
    fn new(
        action: &'static str,
        icon: &'static str,
        verb: &'static str,
        run: Box<dyn gpui::Action>,
    ) -> Self {
        Self {
            action,
            icon,
            verb,
            run,
        }
    }
}

/// A home row's frame: `EMPTY_ROW_H`, the icon column, the label filling
/// the row, `trail` at the right.
fn home_row(
    id: impl Into<gpui::ElementId>,
    mark: AnyElement,
    label: Div,
    trail: Div,
) -> Stateful<Div> {
    use ferrite_ui::theme::*;
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(EMPTY_ROW_GAP))
        .w_full()
        .min_w_0()
        .h(px(EMPTY_ROW_H))
        .px(px(EMPTY_ROW_PAD_X))
        .rounded(px(R_CONTROL))
        .whitespace_nowrap()
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .w(px(EMPTY_ICON_W))
                .child(mark),
        )
        .child(label.flex_1().min_w_0().truncate())
        .child(
            trail
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(SPACE_3))
                .text_size(px(FS_META))
                .text_color(rgb(INK_SUBTLE)),
        )
}

impl CockpitView {
    /// Show the empty board while Threads stay open — a fixture's door
    /// (tests, visual references): the app reaches it only when no Pane is
    /// visible. The next landing on a Pane leaves it.
    #[cfg(any(test, feature = "visual-reference"))]
    pub(crate) fn show_empty_board(&mut self, cx: &mut Context<Self>) {
        self.empty_shown = true;
        self.empty_cursor = 0;
        cx.notify();
    }

    /// Whether the empty board is what the board draws this frame.
    pub(super) fn empty_board_shown(&self) -> bool {
        self.empty_shown || self.visible_indices().is_empty()
    }

    /// The hook the root render calls (frame's actions): the empty board's
    /// list keys and the wall's answers.
    pub(super) fn register_board_actions(&self, root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(|view, _: &Next, _, cx| view.step_empty(1, cx)))
            .on_action(cx.listener(|view, _: &Previous, _, cx| view.step_empty(-1, cx)))
            .on_action(cx.listener(|view, _: &Run, window, cx| {
                view.run_empty_row(view.empty_cursor, window, cx)
            }))
    }

    /// The ways to start, in order: only those that would do something
    /// now (a parked Thread to reopen, a Group to open), as the palette
    /// offers them.
    fn start_rows(&self) -> Vec<StartRow> {
        use ferrite_ui::icons;
        let mut rows = vec![
            StartRow::new(
                "cockpit::NewThread",
                icons::PLUS,
                "New thread",
                Box::new(NewThread),
            ),
            StartRow::new(
                "cockpit::NewWorktreeThread",
                icons::BRANCH,
                "New worktree thread",
                Box::new(NewWorktreeThread),
            ),
        ];
        if !self.parked_threads_all().is_empty() {
            rows.push(StartRow::new(
                "cockpit::ReopenThread",
                icons::REFRESH,
                "Reopen last",
                Box::new(ReopenThread),
            ));
        }
        if self.cockpit.groups().iter().next().is_some() {
            rows.push(StartRow::new(
                "palette::OpenGroups",
                icons::BOARD,
                "Open a group",
                Box::new(crate::palette::OpenGroups),
            ));
        }
        rows.push(StartRow::new(
            "palette::Toggle",
            icons::SEARCH,
            "Search threads and commands",
            Box::new(crate::palette::Toggle),
        ));
        rows
    }

    /// An empty draft's hero project word (`pane::start_hero`, Codex's
    /// home): the draft's Project, a switcher that drops the Project menu
    /// under it (`band_from_hero`).
    pub(super) fn draft_hero(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let draft = self.panes[index].draft()?;
        let project = self
            .cockpit
            .registry()
            .project(draft.binding.project())
            .map(|project| SharedString::from(project.title.clone()))
            .unwrap_or_else(|| "Project".into());
        let composer = self.panes[index].composer.clone();
        let word = pane::hero_project(project, true).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                // Stop the press so the root's dismissal cannot close what
                // this opens.
                cx.stop_propagation();
                if let Some(at) = view.panes.iter().position(|pane| pane.composer == composer) {
                    view.focus_pane(at);
                }
                view.open_band_popover(pane::BandChip::Project, cx);
                view.band_from_hero = true;
            }),
        );
        Some(
            ferrite_ui::components::on_bounds(word, self.record_trigger(HERO_TRIGGER.into()))
                .into_any_element(),
        )
    }

    /// An empty Thread's hero project word: its Project, settled, so a word
    /// and not a switcher. None while the Thread's facts are unknown.
    pub(super) fn thread_hero(&self, thread: ThreadId) -> Option<AnyElement> {
        let project = self.facts.get(thread)?.project_label.clone()?;
        Some(pane::hero_project(project, false).into_any_element())
    }

    fn step_empty(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.start_rows().len() as isize;
        let at = (self.empty_cursor as isize).min(count - 1);
        let next = (at + delta).rem_euclid(count) as usize;
        if next != self.empty_cursor {
            self.empty_cursor = next;
            cx.notify();
        }
    }

    fn run_empty_row(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.start_rows();
        let at = at.min(rows.len() - 1);
        let Some(row) = rows.into_iter().nth(at) else {
            return;
        };
        self.empty_cursor = at;
        self.empty_shown = false;
        window.dispatch_action(row.run, cx);
        cx.notify();
    }

    /// `claude 2.1.316 · codex 0.166.0`: the provider CLIs found, each led
    /// by its logomark, one dim `FS_META` line. Nothing until the probe
    /// lands, and the plain fact when it found neither.
    fn empty_board_facts(&self) -> Option<Div> {
        use ferrite_ui::theme::*;
        let (claude, codex) = self.cli_versions.as_ref()?;
        let version = |found: &SharedString| -> Option<SharedString> {
            (!found.starts_with("not found"))
                .then(|| {
                    found
                        .split(" \u{b7} ")
                        .next()
                        .unwrap_or_default()
                        .to_string()
                })
                .filter(|version| !version.is_empty())
                .map(SharedString::from)
        };
        let found: Vec<(&'static str, u32, &'static str, SharedString)> = [
            (
                ferrite_ui::icons::CLAUDE,
                PROVIDER_CLAUDE,
                "claude",
                version(claude),
            ),
            (
                ferrite_ui::icons::CODEX,
                PROVIDER_CODEX,
                "codex",
                version(codex),
            ),
        ]
        .into_iter()
        .filter_map(|(glyph, ink, name, version)| version.map(|v| (glyph, ink, name, v)))
        .collect();
        let line = div()
            .debug_selector(|| "empty-board-facts".into())
            .flex()
            .items_center()
            .gap(px(SPACE_2))
            .whitespace_nowrap()
            .text_size(px(FS_META))
            .line_height(px(LH_META))
            .text_color(rgb(INK_SUBTLE));
        if found.is_empty() {
            return Some(line.child("No provider CLI found"));
        }
        Some(
            found
                .into_iter()
                .enumerate()
                .fold(line, |line, (at, (glyph, ink, name, version))| {
                    line.when(at > 0, |line| line.child("\u{b7}")).child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(SPACE_1))
                            .child(
                                ferrite_ui::icons::icon(glyph, BAR_MARK, ink)
                                    .mt(px(-STATUS_LOGO_FOOT)),
                            )
                            .child(SharedString::from(format!("{name} {version}"))),
                    )
                }),
        )
    }

    /// The board with no Pane on it (see the module doc).
    pub(super) fn empty_board(&self, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let head = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(EMPTY_FACTS_GAP))
            .child(ferrite_ui::components::greeting())
            .children(self.empty_board_facts());

        let mut actions = div()
            .debug_selector(|| "empty-board-commands".into())
            .track_focus(&self.empty_focus)
            .key_context(KEY_CONTEXT)
            .flex()
            .flex_col()
            .w_full();
        let rows = self.start_rows();
        // The rows offered can shrink under the cursor (the last Group
        // dissolved): it holds on the last.
        let cursor = self.empty_cursor.min(rows.len() - 1);
        for (at, row) in rows.into_iter().enumerate() {
            let StartRow {
                action,
                icon: glyph,
                verb,
                ..
            } = row;
            let selected = at == cursor;
            let keys = Self::key_label(action);
            let row = home_row(
                ("empty-board-command", at),
                ferrite_ui::icons::icon(glyph, NAV_ICON, INK_STRONG).into_any_element(),
                div().text_color(rgb(INK_STRONG)).child(verb),
                div().children(keys.map(|keys| {
                    ferrite_ui::components::key_combo(&keys, INK_SUBTLE)
                        .font_family(FONT_UI)
                        .text_size(px(FS_META))
                })),
            )
            .debug_selector(move || format!("empty-board-command-{at}"));
            let row = if selected {
                row.bg(paint::SELECTED).cursor_pointer()
            } else {
                row.hover_row(format!("empty-board-command-{at}"))
                    .press_row()
            };
            actions = actions.child(row.on_click(cx.listener(
                move |view, _: &ClickEvent, window, cx| view.run_empty_row(at, window, cx),
            )));
        }

        let now = ferrite_core::clock::system_time();
        // R13: the four Threads used last, open or parked.
        let mut recent: Vec<ThreadId> = self.cockpit.threads();
        recent.retain(|thread| !self.side_chats.owns(*thread));
        for thread in self.parked_threads() {
            if !recent.contains(&thread) {
                recent.push(thread);
            }
        }
        recent.sort_by_key(|thread| std::cmp::Reverse(self.last_used(*thread)));
        recent.truncate(EMPTY_RECENT_MAX);
        let recent_rows = recent.iter().copied().map(|thread| {
            let row = self.thread_row(thread);
            let open = self.pane_for(thread).is_some();
            let age = self
                .facts
                .last_used(thread)
                .map(|at| crate::facts::since_label(at, now));
            home_row(
                ("empty-board-recent", thread.get() as usize),
                nav::mark(row.status, false),
                div().text_color(rgb(INK_STRONG)).child(row.name.clone()),
                div()
                    .children(row.project.clone())
                    .children(age.filter(|age| !age.is_empty())),
            )
            .debug_selector(move || format!("empty-board-recent-{}", thread.get()))
            .hover_row(format!("empty-board-recent-{}", thread.get()))
            .press_row()
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.empty_shown = false;
                if open {
                    view.focus_thread(thread, cx);
                } else {
                    view.revive_thread(thread, cx);
                }
            }))
        });
        let recent_list = (!recent.is_empty()).then(|| {
            div()
                .debug_selector(|| "empty-board-recent".into())
                .flex()
                .flex_col()
                .w_full()
                // A section heading, as the nav's and the palette's, on the
                // labels' edge.
                .child(
                    div()
                        .pl(px(EMPTY_ROW_PAD_X + EMPTY_ICON_W + EMPTY_ROW_GAP))
                        .pb(px(EMPTY_HEADING_PAD_B))
                        .text_size(px(FS_META))
                        .line_height(px(LH_META))
                        .font_weight(W_LABEL)
                        .text_color(rgb(INK_SUBTLE))
                        .child("Recent"),
                )
                .children(recent_rows)
        });

        let column = div()
            .flex()
            .flex_col()
            .items_center()
            .flex_shrink_0()
            .w_full()
            .max_w(px(TX_COLUMN_MAX_W))
            .gap(px(EMPTY_SECTION_GAP))
            .child(head)
            .child(actions)
            .children(recent_list);
        div()
            .debug_selector(|| "empty-board".into())
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .min_w_0()
            .overflow_hidden()
            .bg(paint::CARD)
            .p(px(EMPTY_PAD))
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .text_color(rgb(INK_STRONG))
            // Centred, a touch above the middle (`EMPTY_LIFT`), and pinned
            // to the top once it no longer fits.
            .child(div().flex_1().min_h_0())
            .child(column)
            .child(div().flex_1().min_h_0().pb(px(EMPTY_LIFT)))
    }
}
