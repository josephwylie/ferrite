//! The board with nothing on it (F-13), the way a terminal greets you: a
//! banner — the steel mark three rows tall, `Ferrite <version>`, the
//! provider CLIs it found and how many Projects and open Threads it holds,
//! the launch directory as a path — then the five things to do next as rows
//! behind a `❯` selection bar, each with its key from the platform's key
//! table, and the four Threads used last.
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
        ("cockpit::NewThread", "new thread", Box::new(NewThread)),
        (
            "cockpit::NewWorktreeThread",
            "new worktree thread",
            Box::new(NewWorktreeThread),
        ),
        (
            "cockpit::ReopenThread",
            "reopen last",
            Box::new(ReopenThread),
        ),
        (
            "palette::OpenGroups",
            "open a group",
            Box::new(crate::palette::OpenGroups),
        ),
        (
            "palette::Toggle",
            "everything else",
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
        use crate::theme::*;
        let version = env!("CARGO_PKG_VERSION");
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
        // one dim run of words, each logo half a cell before its name.
        let seam = || crate::components::cells("\u{b7}").w(px(STATUS_SEAM_W));
        let mut facts = div()
            .debug_selector(|| "empty-board-facts".into())
            .flex()
            .items_center()
            .gap(px(CH))
            .whitespace_nowrap()
            .text_color(rgb(TEXT_MUTED));
        for (provider, version) in &clis {
            let (glyph, ink, name) = match provider {
                Provider::Claude => (crate::icons::CLAUDE, PROVIDER_CLAUDE, "claude"),
                Provider::Codex => (crate::icons::CODEX, PROVIDER_CODEX, "codex"),
            };
            facts = facts
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(CH / 2.0))
                        .child(crate::icons::icon(glyph, BAR_MARK, ink))
                        .child(crate::components::cells(format!("{name} {version}"))),
                )
                .child(seam());
        }
        facts = facts
            .child(crate::components::cells(plural(projects, "project")))
            .child(seam())
            .child(crate::components::cells(plural(open, "open thread")));
        // The launch directory, `~`-abbreviated, as a path reads: cyan, its
        // underline under the pointer, a press opening it.
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
                // A path as every path reads (`file_links::path_text`): cyan,
                // its underline under the pointer; a press opens it.
                let target = crate::file_links::PathTarget::new(root.clone());
                div()
                    .id("empty-board-path")
                    .debug_selector(|| "empty-board-path".into())
                    .flex_shrink_0()
                    .cursor_pointer()
                    .text_color(rgb(PATH_INK))
                    .child(crate::file_links::path_text(shown, target))
                    .on_click(move |_, window, cx| {
                        crate::file_links::FileLink {
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
                    .child(crate::components::steel_mark(EMPTY_MARK_H)),
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
                            .child(div().text_color(rgb(TEXT_MUTED)).child(version)),
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
                    .pr(px(CH))
                    .map(|row| {
                        if selected {
                            row.bg(paint::SELECTION).cursor_pointer()
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
                            .children(selected.then(|| crate::components::prompt_mark(ACCENT))),
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
                                keys.map(|keys| crate::components::key_combo(&keys, TEXT_MUTED)),
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
            let face = thread_status(row.status.wall(), false);
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
                    .hover_row(format!("empty-board-recent-{}", thread.get()))
                    .press_row()
                    .child(div().flex_shrink_0().w(px(GLYPH_GUTTER)))
                    // The dot the size of the face's `●`, where it would sit.
                    .child(
                        div().flex().flex_shrink_0().w(px(GLYPH_GUTTER)).child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(CH))
                                .h(px(ROW))
                                .child(face.dot()),
                        ),
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
                            .text_color(rgb(TEXT_MUTED))
                            .children(row.project.clone()),
                    )
                    .child(
                        div()
                            .w(px(EMPTY_RECENT_AGE_W))
                            .flex_shrink_0()
                            .text_color(rgb(TEXT_MUTED))
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
            .bg(paint::PLANE)
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
                    .child(div().text_color(rgb(TEXT_MUTED)).child("recent"))
                    .child(div().flex_shrink_0().h(px(HALF_ROW)))
                    .child(rows_list)
            })
    }
}
