//! The board with nothing on it (F-13), the way a terminal greets you: a
//! banner — the steel mark three rows tall, `Ferrite <version>`, the
//! provider CLIs it found and how many Projects and open Threads it holds,
//! the launch directory as a path — then the five things to do next as rows
//! behind a `❯` selection bar, each with its key from the platform's key
//! table, and the four Threads used last.
//!
//! In the chrome's grammar (SPEC §2, §5.8): the labels sentence case, the
//! selection a rounded `SELECTED` row (`R_CONTROL`) that hangs
//! `EMPTY_ROW_INSET` outside the text's edge, so the `❯` and the words keep
//! the banner's column while the fill clears them, and a recent row the
//! same band under the pointer.
//!
//! The rows are a list with its own focus and key context (`EmptyBoard`):
//! `empty_board::Next` / `Previous` move the `❯` and the selection ground,
//! `Run` does what the row says (the floats package binds ↑ ↓ ⏎). A press
//! does the same. `show_empty_board` shows it while Threads stay open.

use super::*;
use std::path::PathBuf;

actions!(empty_board, [Next, Previous, Run]);

/// The key context the list holds focus under.
pub(crate) const KEY_CONTEXT: &str = "EmptyBoard";

/// The rows, in order: the action's name in the key table (its chord), what
/// the row says, and what it dispatches.
fn rows() -> [(&'static str, &'static str, Box<dyn gpui::Action>); 5] {
    [
        ("cockpit::NewThread", "New thread", Box::new(NewThread)),
        (
            "cockpit::NewWorktreeThread",
            "New worktree thread",
            Box::new(NewWorktreeThread),
        ),
        (
            "cockpit::ReopenThread",
            "Reopen last",
            Box::new(ReopenThread),
        ),
        (
            "palette::OpenGroups",
            "Open a group",
            Box::new(crate::palette::OpenGroups),
        ),
        (
            "palette::Toggle",
            "Everything else",
            Box::new(crate::palette::Toggle),
        ),
    ]
}

impl CockpitView {
    /// Show the empty board while Threads stay open (the `1 solo` tab with
    /// no loose Thread, a palette command). The next landing on a Pane
    /// leaves it.
    pub(crate) fn show_empty_board(&mut self, cx: &mut Context<Self>) {
        self.empty_shown = true;
        self.empty_cursor = 0;
        cx.notify();
    }

    /// Where Ferrite was launched: the empty board's path line. Defaults to
    /// the process's directory at startup.
    #[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
    pub(crate) fn set_launch_dir(&mut self, dir: PathBuf) {
        self.launch_dir = Some(dir);
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

    fn step_empty(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = rows().len() as isize;
        let next = (self.empty_cursor as isize + delta).rem_euclid(count) as usize;
        if next != self.empty_cursor {
            self.empty_cursor = next;
            cx.notify();
        }
    }

    fn run_empty_row(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, _, action)) = rows().into_iter().nth(at) else {
            return;
        };
        self.empty_cursor = at;
        self.empty_shown = false;
        window.dispatch_action(action, cx);
        cx.notify();
    }

    /// The board with no Pane on it (see the module doc).
    pub(super) fn empty_board(&self, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let version = self.shown_version.clone();
        let cli = |found: &SharedString| -> Option<SharedString> {
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
        let clis: Vec<(Provider, SharedString)> = self
            .cli_versions
            .as_ref()
            .map(|(claude, codex)| {
                [
                    (Provider::Claude, cli(claude)),
                    (Provider::Codex, cli(codex)),
                ]
                .into_iter()
                .filter_map(|(provider, version)| version.map(|version| (provider, version)))
                .collect()
            })
            .unwrap_or_default();
        let projects = self.cockpit.registry().projects().len();
        let open = self.cockpit.threads().len();
        let plural = |count: usize, word: &str| {
            format!("{count} {word}{}", if count == 1 { "" } else { "s" })
        };
        // `claude 2.1.289 · codex 0.160.0 · 3 projects · 9 open threads`:
        // one dim run of words, each logo `SPACE_1` before its name, a
        // word space either side of each `·`. The words are Geist, so
        // they take their own widths (a mono cell a character would pad
        // each by a different amount and leave the seams uneven).
        let words = |text: String| div().flex_shrink_0().child(SharedString::from(text));
        let seam = || words("\u{b7}".into());
        let mut facts = div()
            .debug_selector(|| "empty-board-facts".into())
            .flex()
            .items_center()
            .gap(px(SPACE_1))
            .whitespace_nowrap()
            .text_color(rgb(TEXT_DIM));
        for (provider, version) in &clis {
            let (glyph, ink, name) = match provider {
                Provider::Claude => (ferrite_ui::icons::CLAUDE, PROVIDER_CLAUDE, "claude"),
                Provider::Codex => (ferrite_ui::icons::CODEX, PROVIDER_CODEX, "codex"),
            };
            facts = facts
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(SPACE_1))
                        // Inline in the line, `vertical-align: -1px`, as the
                        // status line's logos sit (`STATUS_LOGO_FOOT`).
                        .child(
                            ferrite_ui::icons::icon(glyph, BAR_MARK, ink)
                                .mt(px(-ferrite_ui::theme::STATUS_LOGO_FOOT)),
                        )
                        .child(words(format!("{name} {version}"))),
                )
                .child(seam());
        }
        facts = facts
            .child(words(plural(projects, "project")))
            .child(seam())
            .child(words(plural(open, "open thread")));
        // The launch directory, `~`-abbreviated, as a path reads: the link
        // ink, its underline under the pointer, a press opening it.
        let root = self
            .launch_dir
            .clone()
            .or_else(|| {
                self.launch_project
                    .and_then(|project| self.cockpit.registry().project(project))
                    .or_else(|| self.cockpit.registry().projects().first())
                    .map(|project| project.root.clone())
            })
            .map(|root| {
                let shown = SharedString::from(home_relative(&root));
                // A path as every path reads (`file_links::path_text`): the
                // link ink, its underline under the pointer; a press opens it.
                let target = ferrite_pane::file_links::PathTarget::new(root.clone());
                div()
                    .id("empty-board-path")
                    .debug_selector(|| "empty-board-path".into())
                    .flex_shrink_0()
                    .cursor_pointer()
                    .text_color(rgb(LINK_INK))
                    .child(ferrite_pane::file_links::path_text(shown, target))
                    .on_click(move |_, window, cx| {
                        ferrite_pane::file_links::FileLink {
                            path: root.clone(),
                            location: None,
                        }
                        .open(window, cx)
                    })
            });
        let banner = div()
            .debug_selector(|| "empty-board-banner".into())
            .flex()
            .items_start()
            .gap(px(EMPTY_BANNER_GAP))
            .child(
                div()
                    .debug_selector(|| "empty-board-mark".into())
                    .child(ferrite_ui::components::steel_mark(EMPTY_MARK_H)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .gap(px(CH))
                            .child(
                                div()
                                    .font_weight(W_STRONG)
                                    .text_color(rgb(TEXT_STRONG))
                                    .child("Ferrite"),
                            )
                            .child(div().text_color(rgb(TEXT_DIM)).child(version)),
                    )
                    .child(facts)
                    .children(root),
            );
        let mut list = div()
            .debug_selector(|| "empty-board-commands".into())
            .track_focus(&self.empty_focus)
            .key_context(KEY_CONTEXT)
            .flex()
            .flex_col()
            .items_start();
        for (at, (action, verb, _)) in rows().into_iter().enumerate() {
            let selected = at == self.empty_cursor;
            let keys = Self::key_label(action);
            let hover = SharedString::from(format!("empty-board-command-{at}"));
            list = list.child(
                div()
                    .id(("empty-board-command", at))
                    .debug_selector(move || format!("empty-board-command-{at}"))
                    .flex()
                    .items_center()
                    .h(px(ROW))
                    .ml(px(-EMPTY_ROW_INSET))
                    .pl(px(EMPTY_ROW_INSET))
                    .pr(px(CH))
                    .rounded(px(R_CONTROL))
                    .map(|row| {
                        if selected {
                            row.bg(paint::SELECTED).cursor_pointer()
                        } else {
                            row.hover_row(hover.clone()).press_row()
                        }
                    })
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .w(px(GLYPH_GUTTER))
                            .children(
                                selected.then(|| ferrite_ui::components::prompt_mark(TEXT_STRONG)),
                            ),
                    )
                    .child(
                        div()
                            .w(px(EMPTY_VERB_W))
                            .flex_shrink_0()
                            .text_color(rgb(if selected { TEXT_STRONG } else { TEXT }))
                            .child(verb),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .justify_end()
                            .w(px(EMPTY_KEY_W))
                            .children(
                                keys.map(|keys| ferrite_ui::components::key_combo(&keys, TEXT_DIM)),
                            ),
                    )
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        view.run_empty_row(at, window, cx);
                    })),
            );
        }
        let now = ferrite_core::clock::system_time();
        // R13: the four Threads used last, open or parked.
        let mut recent: Vec<ThreadId> = self.cockpit.threads();
        for thread in self.parked_threads() {
            if !recent.contains(&thread) {
                recent.push(thread);
            }
        }
        recent.sort_by_key(|thread| std::cmp::Reverse(self.last_used(*thread)));
        recent.truncate(EMPTY_RECENT_MAX);
        let mut rows_list = div()
            .debug_selector(|| "empty-board-recent".into())
            .flex()
            .flex_col()
            .items_start();
        for thread in recent.iter().copied() {
            let row = self.thread_row(thread);
            let open = self.pane_for(thread).is_some();
            let age = self
                .facts
                .last_used(thread)
                .map(|at| crate::facts::since_label(at, now))
                .unwrap_or_default();
            rows_list = rows_list.child(
                div()
                    .id(("empty-board-recent", thread.get() as usize))
                    .debug_selector(move || format!("empty-board-recent-{}", thread.get()))
                    .flex()
                    .items_center()
                    .h(px(ROW))
                    .ml(px(-EMPTY_ROW_INSET))
                    .pl(px(EMPTY_ROW_INSET))
                    .rounded(px(R_CONTROL))
                    .hover_row(format!("empty-board-recent-{}", thread.get()))
                    .press_row()
                    .child(div().flex_shrink_0().w(px(GLYPH_GUTTER)))
                    // The row's mark (`nav::mark`), at its column's start,
                    // the nav's gap before the title.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_shrink_0()
                            .w(px(GLYPH_GUTTER))
                            .h(px(ROW))
                            .mr(px(NAV_ICON_GAP))
                            .child(nav::mark(row.status, false)),
                    )
                    .child(
                        div()
                            .w(px(EMPTY_RECENT_TITLE_W))
                            .flex_shrink_0()
                            .truncate()
                            .text_color(rgb(TEXT))
                            .child(row.name.clone()),
                    )
                    .child(
                        div()
                            .w(px(EMPTY_RECENT_PROJECT_W))
                            .flex_shrink_0()
                            .truncate()
                            .text_color(rgb(TEXT_DIM))
                            .children(row.project.clone()),
                    )
                    .child(
                        div()
                            .w(px(EMPTY_RECENT_AGE_W))
                            .flex_shrink_0()
                            .text_color(rgb(TEXT_DIM))
                            .child(age),
                    )
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.empty_shown = false;
                        if open {
                            view.focus_thread(thread, cx);
                        } else {
                            view.revive_thread(thread, cx);
                        }
                    })),
            );
        }
        let has_recent = !recent.is_empty();
        div()
            .debug_selector(|| "empty-board".into())
            .flex_1()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .bg(paint::CARD)
            .py(px(EMPTY_PAD_Y))
            .px(px(EMPTY_PAD_X))
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .text_color(rgb(TEXT))
            .child(banner)
            .child(div().flex_shrink_0().h(px(2.0 * ROW)))
            .child(list)
            .when(has_recent, |board| {
                board
                    .child(div().flex_shrink_0().h(px(2.0 * ROW)))
                    // A section heading, as the nav's and the palette's.
                    .child(
                        div()
                            .text_size(px(FS_META))
                            .font_weight(W_LABEL)
                            .text_color(rgb(TEXT_DIM))
                            .child("Recent"),
                    )
                    .child(div().flex_shrink_0().h(px(HALF_ROW)))
                    .child(rows_list)
            })
    }
}
