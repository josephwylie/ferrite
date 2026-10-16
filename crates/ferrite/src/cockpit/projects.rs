//! Projects' cockpit half (ADR 0030, ADR 0031): the snapshot the nav and
//! the bottom bar read, refreshed off the UI thread; the palette's Projects
//! commands; the work board's place on the board (`board`) and what it asks
//! of the window; and the nav's project blocks.
//!
//! A project's scope is the focused Thread's: the project it heads or
//! works for, where upstream asked which Herdr workspace the pane was in.

use super::*;
use crate::board::{BoardEvent, BoardView, Opening, Placement};
use crate::float::{self, FloatPlace};
use crate::palette::{
    Act, Command as PaletteCommand, Row as PaletteRow, Section as PaletteSection,
};
use crate::projects::{self, Command, Model, Place};
use gpui::component::notification::{Notification, NotificationType};
use gpui::component::WindowExt as _;
use gpui::{FocusHandle, Subscription};

/// What the cockpit holds for Projects.
#[derive(Default)]
pub(super) struct ProjectsState {
    /// Where the projects live; `None` in the test and demo constructors,
    /// so no suite reads the operator's own (nor opens the board on them).
    pub place: Option<Place>,
    pub model: Model,
    /// The project the nav shows alone (`focus`), coordinator first.
    pub focus: Option<String>,
    /// The work board, while it is open.
    pub board: Option<BoardSlot>,
    /// What a command's work said, for the next frame's toast.
    toasts: Vec<(String, String, bool)>,
}

/// The work board as the cockpit holds it.
pub(super) struct BoardSlot {
    pub view: Entity<BoardView>,
    pub placement: Placement,
    /// Shown now: a tab's board stays open, hidden, while you are on a
    /// Thread, as herdr's board tab does.
    pub shown: bool,
    /// What held the keyboard before it showed: it goes back there.
    back: Option<FocusHandle>,
    _events: Subscription,
}

impl ProjectsState {
    pub(super) fn new(place: Option<Place>) -> Self {
        Self {
            place,
            ..Default::default()
        }
    }

    /// Whether the board shows and holds the keyboard.
    pub(super) fn board_shown(&self) -> bool {
        self.board.as_ref().is_some_and(|board| board.shown)
    }

    /// Whether the board shows as the popup, over a veil.
    pub(super) fn popup_shown(&self) -> bool {
        self.board
            .as_ref()
            .is_some_and(|board| board.shown && board.placement == Placement::Popup)
    }

    /// Take the popup down for another float; a tab's board stays.
    pub(super) fn drop_popup(&mut self) {
        if self.popup_shown() {
            self.board = None;
        }
    }
}

impl CockpitView {
    /// Read the snapshot every `popup::REFRESH`, off the UI thread; the
    /// window redraws only when it changed.
    pub(super) fn start_projects_refresh(place: Place, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            let reading = place.clone();
            let model = cx
                .background_spawn(async move { Model::load(&reading) })
                .await;
            let alive = this.update(cx, |view, cx| {
                if view.projects.model != model {
                    view.projects.model = model;
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
            cx.background_executor()
                .timer(ferrite_projects::popup::REFRESH)
                .await;
        })
        .detach();
    }

    /// The focused Thread's project, if it has one.
    fn projects_scope(&self) -> Option<String> {
        let thread = self.focused_thread()?;
        self.projects
            .model
            .project_of(thread)
            .map(|project| project.slug.clone())
    }

    /// The projects shortcut (herdr's `prefix+a`): the board, or back to
    /// where you came from when it shows.
    pub(super) fn toggle_projects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.projects.board_shown() {
            self.leave_board(window, cx);
        } else {
            self.show_board(window, cx);
        }
    }

    /// The board, opened where `board.open` says — or a tab's, shown again
    /// as you left it.
    fn show_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.projects.place.is_none() {
            return;
        }
        let back = window.focused(cx);
        if self.projects.board.is_none() {
            let placement = Placement::read();
            let opening = Opening {
                cwd: self
                    .focused_thread()
                    .and_then(|thread| self.thread_path(thread))
                    .unwrap_or_else(sw_board::store::files::user_home),
                popup: placement == Placement::Popup,
                board_key: crate::board::board_key(),
            };
            let view = cx.new(|cx| BoardView::open(opening, cx));
            let events = cx.subscribe_in(&view, window, Self::on_board_event);
            self.projects.board = Some(BoardSlot {
                view,
                placement,
                shown: false,
                back: None,
                _events: events,
            });
        }
        if let Some(board) = self.projects.board.as_mut() {
            if board.placement == Placement::Popup {
                self.clear_for_float();
                self.floats.palette = None;
            }
        }
        let Some(board) = self.projects.board.as_mut() else {
            return;
        };
        board.shown = true;
        board.back = back;
        let focus = board.view.read(cx).focus.clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Back to where you came from: a tab's board stays open, hidden; the
    /// popup closes.
    fn leave_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(board) = self.projects.board.as_mut() else {
            return;
        };
        board.shown = false;
        let back = board.back.take();
        if board.placement == Placement::Popup {
            self.projects.board = None;
        }
        if let Some(back) = back {
            window.focus(&back, cx);
        }
        cx.notify();
    }

    /// Landing on a Thread (an agent opened from the board, a nav row)
    /// takes a tab's board out of the way, still open.
    pub(super) fn board_gives_way(&mut self) {
        if let Some(board) = self.projects.board.as_mut() {
            if board.placement == Placement::Tab {
                board.shown = false;
                board.back = None;
            }
        }
    }

    fn on_board_event(
        &mut self,
        _: &Entity<BoardView>,
        event: &BoardEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            BoardEvent::Close => {
                let back = self.projects.board.take().and_then(|board| board.back);
                if let Some(back) = back {
                    window.focus(&back, cx);
                }
                cx.notify();
            }
            BoardEvent::Bell => window.play_system_bell(),
        }
    }

    /// A reference scene's board: `board` drawn as it is, in the board's
    /// place.
    #[cfg(feature = "visual-reference")]
    pub(super) fn stage_board(
        &mut self,
        board: crate::board::Board,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.new(|cx| BoardView::still(board, cx));
        let events = cx.subscribe_in(&view, window, Self::on_board_event);
        self.projects.board = Some(BoardSlot {
            view,
            placement: Placement::Tab,
            shown: true,
            back: None,
            _events: events,
        });
        cx.notify();
    }

    fn projects_toast(
        &mut self,
        title: String,
        body: String,
        error: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let kind = if error {
            NotificationType::Error
        } else {
            NotificationType::Info
        };
        window.push_notification(
            Notification::new()
                .title(title)
                .message(body)
                .with_type(kind)
                .autohide(!error),
            cx,
        );
    }

    /// What a command's work said, once it is back: the next frame shows it.
    pub(super) fn present_projects_toasts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (title, body, error) in std::mem::take(&mut self.projects.toasts) {
            self.projects_toast(title, body, error, window, cx);
        }
    }

    /// Run `work` off the UI thread, and toast what it says.
    fn projects_work(
        &mut self,
        work: impl FnOnce() -> (String, String, bool) + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let said = cx.background_spawn(async move { work() }).await;
            this.update(cx, |view, cx| {
                view.projects.toasts.push(said);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// One of the palette's Projects commands.
    pub(super) fn run_projects_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(place) = self.projects.place.clone() else {
            return;
        };
        match command {
            Command::Board => self.toggle_projects(window, cx),
            Command::Focus => match self.projects_scope() {
                Some(slug) => self.focus_nav_on_project(Some(slug), cx),
                None => self.projects_toast(
                    "Projects".into(),
                    "this Thread does not belong to a project".into(),
                    true,
                    window,
                    cx,
                ),
            },
            Command::Unfocus => self.focus_nav_on_project(None, cx),
            Command::Configure => self.projects_work(move || projects::configure(&place), cx),
            Command::Doctor => self.projects_work(
                move || {
                    let (healthy, _) = place.run(&["doctor"]);
                    (
                        projects::DOCTOR_TITLE.into(),
                        projects::doctor_body(healthy).into(),
                        !healthy,
                    )
                },
                cx,
            ),
        }
        cx.notify();
    }

    /// `focus`/`unfocus`: the nav shows only `slug`'s block, coordinator
    /// first, its Threads by need — or everything again.
    pub(crate) fn focus_nav_on_project(&mut self, slug: Option<String>, cx: &mut Context<Self>) {
        if self.projects.focus != slug {
            self.projects.focus = slug;
            cx.notify();
        }
    }

    /// The palette's Projects rows, while there is a place to read.
    pub(super) fn palette_projects_rows(&self) -> Vec<PaletteRow> {
        if self.projects.place.is_none() {
            return Vec::new();
        }
        let in_project = self.projects_scope().is_some();
        Command::ALL
            .into_iter()
            .filter(|command| match command {
                Command::Focus => in_project,
                Command::Unfocus => self.projects.focus.is_some(),
                _ => true,
            })
            .map(|command| PaletteRow {
                section: PaletteSection::Commands,
                name: command.title().into(),
                context: command.detail().into(),
                status: None,
                detail: None,
                keys: (command == Command::Board)
                    .then(|| crate::components::bound_chord("projects::Toggle"))
                    .flatten(),
                act: Act::Command(PaletteCommand::Projects(command)),
            })
            .collect()
    }

    /// The board where it shows: in a tab, over the whole board in the
    /// board's place; as the popup, over a veil, 90% of the board (all of
    /// it when that is small), centred, as herdr hangs its popup.
    pub(super) fn projects_float(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(board_slot) = self.projects.board.as_ref().filter(|board| board.shown) else {
            return Vec::new();
        };
        let board = self.board_bounds(window);
        let popup = board_slot.placement == Placement::Popup;
        let (width, height) = if popup {
            crate::board::popup_size(board.w, board.h)
        } else {
            (board.w, board.h)
        };
        let view = board_slot.view.clone();
        view.update(cx, |view, _| view.fit(width, height));
        let anchor = gpui::Bounds::new(
            gpui::point(px(board.x), px(board.y)),
            gpui::size(px(board.w), px(board.h)),
        );
        let float = float::hang(
            view.into(),
            anchor,
            FloatPlace::BoardTop {
                top: (board.h - height) / 2.0,
                width,
            },
            window,
            cx,
        );
        if !popup {
            return vec![float];
        }
        let veil = deferred(
            div()
                .debug_selector(|| "projects-veil".into())
                .absolute()
                .left(px(board.x))
                .top(px(board.y))
                .w(px(board.w))
                .h(px(board.h))
                .bg(rgba(crate::theme::VEIL))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        view.leave_board(window, cx);
                    }),
                ),
        )
        .with_priority(float::VEIL_PRIORITY)
        .into_any_element();
        vec![veil, float]
    }

    /// `projects: N need you`, while the ticker runs and someone does.
    pub(super) fn projects_bar_word(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let word = self.projects.model.needs_you.clone()?;
        Some(
            div()
                .id("bottom-bar-projects")
                .debug_selector(|| "bottom-bar-projects".into())
                .flex_shrink_0()
                .text_color(rgb(crate::theme::YELLOW))
                .cursor_pointer()
                .child(SharedString::from(word))
                .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    view.run_projects_command(Command::Board, window, cx);
                }))
                .into_any_element(),
        )
    }

    /// The nav's project blocks, from the snapshot, while the ticker runs:
    /// each project with a Thread here, by slug; focused, that one alone.
    pub(super) fn project_blocks(
        &self,
        row: impl Fn(ThreadId) -> nav::ThreadRow,
    ) -> Vec<nav::ProjectBlock> {
        let model = &self.projects.model;
        if !model.ticker {
            return Vec::new();
        }
        let known = |thread: &ThreadId| {
            self.facts.get(*thread).is_some() || self.cockpit.thread(*thread).is_some()
        };
        model
            .projects
            .iter()
            .filter(|project| {
                self.projects
                    .focus
                    .as_ref()
                    .is_none_or(|slug| *slug == project.slug)
            })
            .filter_map(|project| {
                let head = project.coordinator.filter(known).map(&row);
                let rows: Vec<(nav::ThreadRow, Option<SharedString>)> = project
                    .threads
                    .iter()
                    .filter_map(|entry| {
                        let thread = entry.thread.filter(known)?;
                        let sub = Some(SharedString::from(entry.sub.clone()))
                            .filter(|sub| !sub.is_empty());
                        Some((
                            nav::ThreadRow {
                                name: entry.display.clone().into(),
                                ..row(thread)
                            },
                            sub,
                        ))
                    })
                    .collect();
                (head.is_some() || !rows.is_empty()).then(|| nav::ProjectBlock {
                    slug: project.slug.clone().into(),
                    name: project.name.clone().into(),
                    summary: project.summary.clone().into(),
                    head_sub: Some(SharedString::from(project.coordinator_sub.clone()))
                        .filter(|sub| !sub.is_empty()),
                    head,
                    rows,
                })
            })
            .collect()
    }

    /// The project blocks at the top of the tree: each its head (a press
    /// lands on the coordinator) and its Threads' rows, each over its
    /// sub-line.
    pub(super) fn project_blocks_element(
        &self,
        blocks: &[nav::ProjectBlock],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        blocks
            .iter()
            .enumerate()
            .map(|(index, block)| {
                let head = nav::project_head(index, block);
                let head = match block.head.as_ref().map(|row| row.thread) {
                    Some(thread) => {
                        head.on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                            view.land_on_thread(thread, cx)
                        }))
                    }
                    None => head,
                };
                let mut section = nav::section(index == 0)
                    .child(head)
                    .children(block.head_sub.clone().map(nav::sub_line));
                for (row, sub) in &block.rows {
                    section = section
                        .child(self.thread_element(row, None, nav::RowPlace::Root, cx))
                        .children(sub.clone().map(nav::sub_line));
                }
                section.into_any_element()
            })
            .collect()
    }
}

/// Every project's Threads are its own rows: the nav lists them nowhere
/// else.
pub(super) fn claimed(blocks: &[nav::ProjectBlock]) -> std::collections::HashSet<ThreadId> {
    blocks.iter().flat_map(nav::ProjectBlock::threads).collect()
}

/// Whether the nav is focused on a project it can show.
pub(super) fn focused(state: &ProjectsState, blocks: &[nav::ProjectBlock]) -> bool {
    state
        .focus
        .as_ref()
        .is_some_and(|slug| blocks.iter().any(|block| block.slug == slug.as_str()))
}
