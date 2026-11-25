//! A Project's Archive (#226): Completed Tasks leave the board 7 days
//! after completing, or at once with the board head's Clear completed. The
//! Archive is searched by title and instructions; an archived Task opens in
//! the ordinary Task panel, read-only, with its activity and the Workers it
//! had.
//!
//! `ArchiveView` owns the search field and the earlier Worker being read
//! (ADR 0041). Its render is the list beside the Task panel: it reads the
//! archived Tasks that match its search, and the open Task, through a
//! `Source`, which the Cockpit makes over itself and a test over a fixture.
//! What a row or Board asks it says as an `ArchiveEvent`. Whether the
//! Archive shows, the Task panel, and reopening an earlier Worker to be
//! read stay the Cockpit's (`cockpit/archive.rs`).

use ferrite_core::factory_board::{self, TaskId};
use ferrite_core::ThreadKey;
use ferrite_pane::composer::{Composer, Role};
use ferrite_ui::source::Source;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, AnyElement, App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable,
    SharedString, Window,
};

/// One archived Task as its row says it.
pub struct ArchivedRow {
    pub id: TaskId,
    pub title: String,
    pub completed_ms: Option<i64>,
    pub archived_ms: Option<i64>,
    pub result: Option<String>,
}

/// What the Archive's list draws from the window.
pub struct ArchiveInputs {
    /// The archived Tasks that match the search, most recently archived
    /// first.
    pub found: Vec<ArchivedRow>,
    /// Whether anything is archived at all.
    pub any: bool,
    /// The Task whose panel is open.
    pub open: Option<TaskId>,
}

/// What the Archive's list asks of the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveEvent {
    /// An archived Task's row: open its panel.
    Open(TaskId),
    /// Board: give the board back.
    Close,
}

/// The Archive: its search, and the earlier Worker being read.
pub struct ArchiveView {
    /// The archived Tasks that match a search, as the window has them.
    inputs: Source<ArchiveInputs, String>,
    search: Entity<Composer>,
    /// An earlier Worker of the open Task, opened to be read (G4), and
    /// whether this window reopened it, to park it again. The Cockpit sets
    /// and takes it as it opens and parks the Thread.
    pub reading: Option<(ThreadKey, bool)>,
}

impl EventEmitter<ArchiveEvent> for ArchiveView {}

impl ArchiveView {
    pub fn new(inputs: Source<ArchiveInputs, String>, cx: &mut Context<Self>) -> Self {
        let search = cx.new(Composer::new);
        search.update(cx, |field, cx| field.set_role(Role::Field(None), cx));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        Self {
            inputs,
            search,
            reading: None,
        }
    }

    /// The search field's focus, which holds the keyboard while the
    /// Archive shows.
    pub fn focus(&self, cx: &App) -> FocusHandle {
        self.search.read(cx).focus_handle(cx)
    }

    /// Forget an earlier Worker whose Thread is gone.
    pub fn retain_threads(&mut self, keep: impl Fn(ThreadKey) -> bool) {
        if self.reading.is_some_and(|(thread, _)| !keep(thread)) {
            self.reading = None;
        }
    }

    /// One archived Task: its title, when it completed and was archived,
    /// and its result. Opening it starts nothing.
    fn row(&self, task: ArchivedRow, open: bool, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let ArchivedRow {
            id,
            title,
            completed_ms,
            archived_ms,
            result,
        } = task;
        let key = format!("factory-archive-{id}");
        let when = |label: &str, at: Option<i64>| {
            at.map(|at| format!("{label} {}", factory_board::when_label(at)))
        };
        let meta: Vec<String> = [
            Some(format!("#{id}")),
            when("Completed", completed_ms),
            when("Archived", archived_ms),
        ]
        .into_iter()
        .flatten()
        .collect();
        div()
            .id(SharedString::from(key.clone()))
            .debug_selector(move || key.clone())
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(SPACE_1))
            .p(px(SPACE_3))
            .rounded(px(R_CONTROL))
            .border_1()
            .border_color(paint::CARD_RING)
            .cursor_pointer()
            .map(|row| {
                if open {
                    row.bg(paint::SELECTED)
                } else {
                    row.bg(paint::RAISED).hover(|row| row.bg(paint::SELECTED))
                }
            })
            .child(
                div()
                    .text_color(rgb(INK_STRONG))
                    .truncate()
                    .child(SharedString::from(title)),
            )
            .child(
                div()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(meta.join(" \u{b7} "))),
            )
            .children(result.map(|result| {
                div()
                    .text_size(px(FS_META))
                    .line_clamp(2)
                    .text_ellipsis()
                    .child(SharedString::from(result))
            }))
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.emit(ArchiveEvent::Open(id.clone()))
            }))
            .into_any_element()
    }
}

impl Render for ArchiveView {
    /// The Archive's list: its head, its search, and each archived Task
    /// that matches.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use ferrite_ui::theme::*;
        let query = self.search.read(cx).text().to_string();
        let Some(ArchiveInputs { found, any, open }) = self.inputs.read(&query, cx) else {
            return div().id("factory-archive-list");
        };
        let search = self.search.clone();
        let focused = search.read(cx).focus_handle(cx).is_focused(window);

        let head = div()
            .flex()
            .items_center()
            .gap(px(SPACE_2))
            .child(
                div()
                    .font_weight(W_LABEL)
                    .text_color(rgb(INK_STRONG))
                    .child("Archive"),
            )
            // The note gives way, so Board stays in a narrow board.
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child("Completed tasks, 7 days after they complete or when cleared"),
            )
            .child(div().flex_1())
            .child(
                ferrite_ui::components::ghost_button("factory-archive-close", "Board", cx)
                    .debug_selector(|| "factory-archive-close".into())
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        cx.emit(ArchiveEvent::Close);
                    })),
            );
        let mut list = div()
            .id("factory-archive-list")
            .debug_selector(|| "factory-archive-list".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .px(px(SPACE_4))
            .pb(px(SPACE_4))
            .overflow_y_scroll()
            .child(head)
            .child(
                div()
                    .debug_selector(|| "factory-archive-search".into())
                    .child(ferrite_ui::field::text_field(
                        "Search titles and instructions".into(),
                        search,
                        focused,
                        "factory-archive-search-focused",
                    )),
            );
        if found.is_empty() {
            list = list.child(
                div()
                    .debug_selector(|| "factory-archive-empty".into())
                    .py(px(SPACE_3))
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(if any {
                        "No archived task matches."
                    } else {
                        "Nothing is archived yet."
                    }),
            );
        }
        for task in found {
            let open = open.as_deref() == Some(task.id.as_str());
            list = list.child(self.row(task, open, cx));
        }
        list
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use ferrite_ui::testing::{click, mount, record, shows};
    use gpui::{TestAppContext, VisualTestContext};

    use super::*;

    /// What the window keeps of a Project's Archive, as a test keeps it:
    /// each archived Task's id, title and instructions, and the open one.
    struct Kept {
        tasks: Vec<(&'static str, &'static str, &'static str)>,
        open: Option<&'static str>,
        /// Each search the Archive asked the window to answer.
        asked: Rc<RefCell<Vec<String>>>,
    }

    fn kept() -> Kept {
        Kept {
            tasks: vec![
                ("t-1", "Fix the flaky login test", "It fails one run in ten."),
                ("t-4", "Bump the toolchain", "To 1.91."),
            ],
            open: None,
            asked: Rc::default(),
        }
    }

    /// The Archive in a window of its own, reading `kept` as the Cockpit
    /// reads its Board: the archived Tasks whose title or instructions hold
    /// the search.
    fn open(
        kept: Kept,
        cx: &mut TestAppContext,
    ) -> (Entity<ArchiveView>, Entity<Kept>, &mut VisualTestContext) {
        let kept = cx.new(|_| kept);
        let inputs = Source::of(&kept, |kept: &Kept, query: &String, _| {
            kept.asked.borrow_mut().push(query.clone());
            let found = kept
                .tasks
                .iter()
                .filter(|(_, title, instructions)| {
                    title.contains(query.as_str()) || instructions.contains(query.as_str())
                })
                .map(|(id, title, _)| ArchivedRow {
                    id: id.to_string(),
                    title: title.to_string(),
                    completed_ms: Some(1_000),
                    archived_ms: Some(2_000),
                    result: Some("Fixed".into()),
                })
                .collect();
            Some(ArchiveInputs {
                found,
                any: !kept.tasks.is_empty(),
                open: kept.open.map(str::to_string),
            })
        });
        let (view, cx) = mount(cx, |_, cx| ArchiveView::new(inputs, cx));
        (view, kept, cx)
    }

    /// Each archived Task that matches is a row under the search field.
    #[gpui::test]
    fn each_archived_task_is_a_row(cx: &mut TestAppContext) {
        let (_view, _kept, cx) = open(kept(), cx);
        for drawn in ["factory-archive-list", "factory-archive-search", "factory-archive-close"] {
            assert!(shows(cx, drawn), "{drawn}");
        }
        assert!(shows(cx, "factory-archive-t-1"));
        assert!(shows(cx, "factory-archive-t-4"));
        assert!(!shows(cx, "factory-archive-empty"));
    }

    /// What is typed in the search is what the window is asked to answer,
    /// and the list is what it answers.
    #[gpui::test]
    fn the_search_typed_is_what_the_window_answers(cx: &mut TestAppContext) {
        let (_view, kept, cx) = open(kept(), cx);
        click(cx, "factory-archive-search");
        assert!(shows(cx, "factory-archive-search-focused"), "the field takes the keyboard");
        cx.simulate_input("one run in ten");
        cx.run_until_parked();
        let asked = kept.read_with(cx, |kept, _| kept.asked.borrow().last().cloned());
        assert_eq!(asked.as_deref(), Some("one run in ten"));
        assert!(shows(cx, "factory-archive-t-1"), "found by its instructions");
        assert!(!shows(cx, "factory-archive-t-4"));
    }

    /// With none that match, or none archived, the list says so.
    #[gpui::test]
    fn the_list_says_when_nothing_matches_or_nothing_is_archived(cx: &mut TestAppContext) {
        let (_view, kept, cx) = open(kept(), cx);
        click(cx, "factory-archive-search");
        cx.simulate_input("deploy");
        cx.run_until_parked();
        assert!(!shows(cx, "factory-archive-t-1"));
        let says = |cx: &mut VisualTestContext| {
            cx.update(|window, _| window.debug_text("factory-archive-empty"))
        };
        let no_match = says(cx);
        assert!(no_match.is_some());
        kept.update(cx, |kept, cx| {
            kept.tasks.clear();
            cx.notify();
        });
        cx.run_until_parked();
        let none_archived = says(cx);
        assert!(none_archived.is_some(), "nothing is archived");
        assert_ne!(none_archived, no_match, "and it says which");
    }

    /// A row asks for its Task's panel; Board asks for the board back.
    #[gpui::test]
    fn a_row_opens_its_task_and_board_gives_the_board_back(cx: &mut TestAppContext) {
        let (view, _kept, cx) = open(kept(), cx);
        let heard = record(cx, &view, ArchiveEvent::clone);
        click(cx, "factory-archive-t-4");
        click(cx, "factory-archive-close");
        assert_eq!(heard.take(), [ArchiveEvent::Open("t-4".into()), ArchiveEvent::Close]);
    }

    /// The window's notify redraws the list from what it now keeps: a Task
    /// archived since.
    #[gpui::test]
    fn the_windows_notify_redraws_the_list(cx: &mut TestAppContext) {
        let (_view, kept, cx) = open(kept(), cx);
        assert!(!shows(cx, "factory-archive-t-9"));
        kept.update(cx, |kept, cx| {
            kept.tasks.push(("t-9", "Retire the old CLI", "Now."));
            cx.notify();
        });
        cx.run_until_parked();
        assert!(shows(cx, "factory-archive-t-9"));
    }

    /// Once the window that kept it is gone, the Archive draws nothing.
    #[gpui::test]
    fn with_the_window_gone_it_draws_nothing(cx: &mut TestAppContext) {
        let (view, kept, cx) = open(kept(), cx);
        drop(kept);
        view.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        assert!(!shows(cx, "factory-archive-list"));
    }

    /// Its focus is the search field's.
    #[gpui::test]
    fn its_focus_is_the_search_fields(cx: &mut TestAppContext) {
        let (view, _kept, cx) = open(kept(), cx);
        assert!(!shows(cx, "factory-archive-search-focused"));
        cx.update(|window, cx| {
            let focus = view.read(cx).focus(cx);
            window.focus(&focus, cx);
        });
        cx.run_until_parked();
        assert!(shows(cx, "factory-archive-search-focused"));
    }

    /// The earlier Worker being read goes with its Thread, and stays with
    /// any other.
    #[gpui::test]
    fn a_gone_threads_worker_is_forgotten(cx: &mut TestAppContext) {
        let (view, _kept, cx) = open(kept(), cx);
        let (read, other) = (ThreadKey::fixture(1, 7), ThreadKey::fixture(1, 8));
        view.update(cx, |view, _| {
            view.reading = Some((read, true));
            view.retain_threads(|thread| thread != other);
            assert_eq!(view.reading, Some((read, true)));
            view.retain_threads(|thread| thread != read);
            assert_eq!(view.reading, None);
        });
    }
}
