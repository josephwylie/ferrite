//! The empty board's side in the window: whether it shows, what it draws
//! (`empty_board_inputs`: the CLIs found, the ways to start and the Threads
//! used last) and what its rows ask. The view itself, its selection and
//! its list's focus, is `ferrite_nav::empty_board`'s (ADR 0041). With it,
//! an empty Pane's hero and the board with no environment.

use ferrite_nav::empty_board::{EmptyBoardEvent, EmptyBoardInputs, EmptyBoardView, RecentRow, StartRow};
use ferrite_ui::source::Source;

use super::*;

impl CockpitView {
    /// The empty board, reading what it draws through `empty_board_inputs`.
    pub(super) fn new_empty_board(cx: &mut Context<Self>) -> Entity<EmptyBoardView> {
        let inputs = Source::of(&cx.entity(), |root: &Self, _: &(), cx| Some(root.empty_board_inputs(cx)));
        cx.new(|cx| EmptyBoardView::new(inputs, cx))
    }

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
        let mut recent: Vec<ThreadKey> = self.cockpit.read(cx).threads();
        let side_chat = self.panel.side_chat.read(cx);
        recent.retain(|thread| !side_chat.owns(*thread));
        for thread in self.parked_threads(cx) {
            if !recent.contains(&thread) {
                recent.push(thread);
            }
        }
        recent.sort_by_key(|thread| std::cmp::Reverse(self.last_used(*thread)));
        recent.truncate(ferrite_ui::theme::EMPTY_RECENT_MAX);
        let recent = recent
            .into_iter()
            .map(|thread| {
                let nav::ThreadRow {
                    name,
                    status,
                    project,
                    ..
                } = self.thread_row(thread, cx);
                RecentRow {
                    thread,
                    status,
                    name,
                    project,
                    age: self
                        .facts
                        .last_used(thread)
                        .map(|at| ferrite_ui::age::since_label(at, now)),
                    open: self.pane_for(thread).is_some(),
                }
            })
            .collect();
        EmptyBoardInputs {
            cli_versions: self.environment.cli_versions.clone(),
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
                    .on_click(cx.listener(|view, _, _, cx| view.open_connections(cx))),
            )
    }
}
