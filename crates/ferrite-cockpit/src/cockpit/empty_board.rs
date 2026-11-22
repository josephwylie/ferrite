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
//!
//! Its state lives in a view of its own (ADR 0041), `EmptyBoardView`: the
//! row selected and the list's focus, which its own keys and presses
//! write. It is held for the window's life, so the selection is where it
//! was left at the next visit. What it draws of the window it reads
//! through `empty_board_inputs`; what a row asks it says as an
//! `EmptyBoardEvent`. Whether it shows, and the CLI probe whose versions
//! it names, stay the root's.

use gpui::WeakEntity;

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

/// A Thread used lately, as its nav row says it, how long since, and
/// whether a Pane shows it.
struct RecentRow {
    row: nav::ThreadRow,
    age: Option<SharedString>,
    open: bool,
}

/// What the empty board draws from the window.
pub(crate) struct EmptyBoardInputs {
    /// The CLIs' versions, once probed: (claude, codex).
    cli_versions: Option<(SharedString, SharedString)>,
    /// The ways to start, in order: only those that would do something
    /// now (a parked Thread to reopen), as the palette offers them.
    rows: Vec<StartRow>,
    /// R13: the four Threads used last, open or parked.
    recent: Vec<RecentRow>,
}

/// What the empty board's rows ask of the window.
pub(crate) enum EmptyBoardEvent {
    /// A way to start: the action its row dispatches.
    Run(Box<dyn gpui::Action>),
    /// A recent Thread a Pane shows: focus it.
    Focus(ThreadId),
    /// A recent Thread no Pane shows: bring it back.
    Revive(ThreadId),
}

/// The empty board: its selected row and its list's focus.
pub(crate) struct EmptyBoardView {
    cockpit: WeakEntity<CockpitView>,
    /// The way to start that ⏎ runs.
    pub(super) cursor: usize,
    /// The list's own focus, which the root's focus pump hands the
    /// keyboard while the empty board shows.
    pub(super) focus: FocusHandle,
}

impl EventEmitter<EmptyBoardEvent> for EmptyBoardView {}

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

/// `claude 2.1.316 · codex 0.166.0`: the provider CLIs found, each led
/// by its logomark, one dim `FS_META` line. Nothing until the probe
/// lands, and the plain fact when it found neither.
fn cli_facts(cli_versions: Option<&(SharedString, SharedString)>) -> Option<Div> {
    use ferrite_ui::theme::*;
    let (claude, codex) = cli_versions?;
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

impl EmptyBoardView {
    pub(super) fn new(cockpit: WeakEntity<CockpitView>, cx: &mut Context<Self>) -> Self {
        Self {
            cockpit,
            cursor: 0,
            focus: cx.focus_handle(),
        }
    }

    fn inputs(&self, cx: &gpui::App) -> Option<EmptyBoardInputs> {
        let cockpit = self.cockpit.upgrade()?;
        Some(cockpit.read(cx).empty_board_inputs(cx))
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(inputs) = self.inputs(cx) else {
            return;
        };
        let count = inputs.rows.len() as isize;
        let at = (self.cursor as isize).min(count - 1);
        let next = (at + delta).rem_euclid(count) as usize;
        if next != self.cursor {
            self.cursor = next;
            cx.notify();
        }
    }

    fn run(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(inputs) = self.inputs(cx) else {
            return;
        };
        let rows = inputs.rows;
        let at = at.min(rows.len() - 1);
        let Some(row) = rows.into_iter().nth(at) else {
            return;
        };
        self.cursor = at;
        cx.emit(EmptyBoardEvent::Run(row.run));
        cx.notify();
    }
}

impl Render for EmptyBoardView {
    /// The board with no Pane on it (see the module doc).
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use ferrite_ui::theme::*;
        let Some(EmptyBoardInputs {
            cli_versions,
            rows,
            recent,
        }) = self.inputs(cx)
        else {
            return div();
        };
        let head = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(EMPTY_FACTS_GAP))
            .child(ferrite_ui::components::greeting())
            .children(cli_facts(cli_versions.as_ref()));

        let mut actions = div()
            .debug_selector(|| "empty-board-commands".into())
            .track_focus(&self.focus)
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(|board, _: &Next, _, cx| board.step(1, cx)))
            .on_action(cx.listener(|board, _: &Previous, _, cx| board.step(-1, cx)))
            .on_action(cx.listener(|board, _: &Run, _, cx| board.run(board.cursor, cx)))
            .flex()
            .flex_col()
            .w_full();
        // The rows offered can shrink under the cursor (the last parked
        // Thread reopened): it holds on the last.
        let cursor = self.cursor.min(rows.len() - 1);
        for (at, row) in rows.into_iter().enumerate() {
            let StartRow {
                action,
                icon: glyph,
                verb,
                ..
            } = row;
            let selected = at == cursor;
            let keys = CockpitView::key_label(action);
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
            actions = actions.child(row.on_click(
                cx.listener(move |board, _: &ClickEvent, _, cx| board.run(at, cx)),
            ));
        }

        let any_recent = !recent.is_empty();
        let recent_rows = recent.into_iter().map(|RecentRow { row, age, open }| {
            let thread = row.key.thread();
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
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.emit(if open {
                    EmptyBoardEvent::Focus(thread)
                } else {
                    EmptyBoardEvent::Revive(thread)
                });
            }))
        });
        let recent_list = any_recent.then(|| {
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

impl CockpitView {
    /// Whether the empty board is what the board draws this frame.
    pub(super) fn empty_board_shown(&self, cx: &gpui::App) -> bool {
        self.visible_indices(cx).is_empty()
    }

    /// What the empty board draws: the CLIs found, the ways to start that
    /// would do something now, and the Threads used last.
    pub(super) fn empty_board_inputs(&self, cx: &gpui::App) -> EmptyBoardInputs {
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
        rows.push(StartRow::new(
            "palette::Toggle",
            icons::SEARCH,
            "Search threads and commands",
            Box::new(crate::palette::Toggle),
        ));

        let now = ferrite_core::clock::system_time();
        let mut recent: Vec<ThreadId> = self.cockpit.read(cx).threads();
        let side_chat = self.side_chat.read(cx);
        recent.retain(|thread| !side_chat.owns(self.cockpit.read(cx).thread_key(*thread)));
        for thread in self.parked_threads(cx) {
            if !recent.contains(&thread) {
                recent.push(thread);
            }
        }
        recent.sort_by_key(|thread| std::cmp::Reverse(self.last_used(*thread, cx)));
        recent.truncate(ferrite_ui::theme::EMPTY_RECENT_MAX);
        let recent = recent
            .into_iter()
            .map(|thread| RecentRow {
                row: self.thread_row(self.cockpit.read(cx).thread_key(thread), cx),
                age: self
                    .facts
                    .last_used(self.cockpit.read(cx).thread_key(thread))
                    .map(|at| crate::facts::since_label(at, now)),
                open: self.pane_for(thread).is_some(),
            })
            .collect();
        EmptyBoardInputs {
            cli_versions: self.cli_versions.clone(),
            rows,
            recent,
        }
    }

    /// An empty board's row: the way to start it runs, or the recent
    /// Thread it opens.
    pub(super) fn on_empty_board_event(
        &mut self,
        _: &Entity<EmptyBoardView>,
        event: &EmptyBoardEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            EmptyBoardEvent::Run(action) => window.dispatch_action(action.boxed_clone(), cx),
            EmptyBoardEvent::Focus(thread) => self.focus_thread(*thread, cx),
            EmptyBoardEvent::Revive(thread) => {
                self.revive_thread(*thread, cx);
            }
        }
    }

    /// An empty draft's hero project word (`pane::start_hero`, Codex's
    /// home): the draft's Project, a switcher that drops the Project menu
    /// under it (`Band::from_hero`).
    pub(super) fn draft_hero(&self, index: usize, cx: &mut Context<Self>) -> Option<pane::Hero> {
        let draft = self.panes[index].draft()?;
        let project = self
            .cockpit
            .read(cx)
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
                    view.focus_pane(at, cx);
                }
                view.open_band_menu(pane::BandChip::Project, true, cx);
            }),
        );
        Some(pane::Hero::Project(
            ferrite_ui::components::on_bounds(word, self.record_trigger(HERO_TRIGGER.into()))
                .into_any_element(),
        ))
    }

    /// An empty Thread's hero: its Project's word, settled, so a word and
    /// not a switcher, or the scratch wording for a Thread with no Project.
    /// None while the Thread's facts are unknown.
    pub(super) fn thread_hero(&self, thread: ThreadId, cx: &gpui::App) -> Option<pane::Hero> {
        Some(match self.facts.get(self.cockpit.read(cx).thread_key(thread))?.project_label.clone() {
            Some(project) => pane::Hero::Project(pane::hero_project(project, false).into_any_element()),
            None => pane::Hero::Scratch,
        })
    }

    pub(super) fn no_environment(&self, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        div()
            .debug_selector(|| "no-environment".into())
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(16.))
            .px(px(24.))
            .child(
                div()
                    .text_size(px(20.))
                    .text_color(rgb(INK_STRONG))
                    .child("No environment enabled"),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(rgb(INK_SUBTLE))
                    .child("Add a remote environment or turn on Local in Connections."),
            )
            .child(
                ferrite_ui::components::form_button("no-environment-connections", cx)
                    .debug_selector(|| "no-environment-connections".into())
                    .accessibility_label("Open Connections")
                    .px(px(16.))
                    .py(px(8.))
                    .child("Open Connections")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.show_settings(cx).update(cx, |settings, _| {
                            settings.page = settings::PageKey::Connections;
                        });
                    })),
            )
    }
}
