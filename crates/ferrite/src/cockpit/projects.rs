//! Projects' cockpit half (ADR 0030, ADR 0031), Tasks to the operator: the
//! board's sidebar the nav and the bottom bar read, refreshed off the UI
//! thread; the palette's Tasks commands; the work board's place on the
//! board (`board`), what it asks of the window, and which nav Project its
//! new tasks go to.

use super::*;
use crate::board::{BoardEvent, BoardView, Opening, Placement, Target};
use crate::float::{self, FloatPlace};
use crate::palette::{
    Act, Command as PaletteCommand, Row as PaletteRow, Section as PaletteSection,
};
use crate::projects::{self, Command};
use crate::sidebar::{self, Sidebar};
use crate::theme::{paint, ICON_BUTTON, ICON_BUTTON_GLYPH, TEXT, TEXT_MUTED, TRANSPARENT};
use gpui::component::notification::{Notification, NotificationType};
use gpui::component::WindowExt as _;
use gpui::{FocusHandle, Subscription};

/// What the cockpit holds for Projects.
#[derive(Default)]
pub(super) struct ProjectsState {
    /// Whether this is the operator's window: false in the test and demo
    /// constructors, so no suite opens the board on the operator's own.
    pub live: bool,
    /// What herdr's sidebar would show of the board: the nav's task rows
    /// and the bottom bar's tally.
    pub sidebar: Sidebar,
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
    pub(super) fn new(live: bool) -> Self {
        Self {
            live,
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
    /// Read the board's sidebar every `sidebar::REFRESH`, off the UI
    /// thread, and at once when an open Thread's Attention changes, as the
    /// upstream writer tells herdr at once; the window redraws only when it
    /// changed. Nothing is read while the window is on another machine,
    /// whose board tells its own.
    pub(super) fn start_sidebar_refresh(cx: &mut Context<Self>) {
        const LOOK: std::time::Duration = std::time::Duration::from_millis(100);
        cx.spawn(async move |this, cx| loop {
            let Ok(pulse) = this.update(cx, |view, cx| view.board_pulse(cx)) else {
                break;
            };
            let here = pulse.is_some();
            let sidebar = cx
                .background_spawn(async move { here.then(sidebar::read).unwrap_or_default() })
                .await;
            let alive = this.update(cx, |view, cx| {
                if view.projects.sidebar != sidebar {
                    view.projects.sidebar = sidebar;
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
            for _ in 0..(sidebar::REFRESH.as_millis() / LOOK.as_millis()) {
                cx.background_executor().timer(LOOK).await;
                let changed = this.update(cx, |view, cx| view.board_pulse(cx) != pulse);
                if changed.unwrap_or(true) {
                    break;
                }
            }
        })
        .detach();
    }

    /// Each open Thread's Attention as the Server reckons it (a Decision
    /// waiting, a turn running, an unseen Notice), from the mirror alone:
    /// what the board's agent list says changes with it. `None` while the
    /// window is on another machine.
    fn board_pulse(&self, cx: &gpui::App) -> Option<Vec<(ThreadId, Attention)>> {
        if self.away_machine(cx).is_some() {
            return None;
        }
        let notices = self.cockpit.notifications();
        let threads = self.cockpit.threads().into_iter();
        Some(
            threads
                .filter_map(|thread| {
                    let view = self.cockpit.thread(thread)?;
                    let attention = if view.pending().is_some() {
                        Attention::Blocked
                    } else if view.needs_queue() || view.background_work() {
                        Attention::Working
                    } else if notices.attention(thread) {
                        Attention::Done
                    } else {
                        Attention::Idle
                    };
                    Some((thread, attention))
                })
                .collect(),
        )
    }

    /// The projects shortcut (herdr's `prefix+a`): the board, or back to
    /// where you came from when it holds the keyboard.
    pub(super) fn toggle_projects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(test)]
        super::drawn::toggled_projects();
        if self.board_focused(window, cx) {
            self.leave_board(window, cx);
        } else {
            self.show_board(None, window, cx);
        }
    }

    /// The board with new tasks going to `project` (a nav Project's
    /// menu): opened there, or an open one moved there, and shown.
    pub(super) fn show_tasks_for(
        &mut self,
        project: ProjectId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = self
            .cockpit
            .registry()
            .project(project)
            .map(|project| project.root.clone());
        self.show_board(root, window, cx);
    }

    /// The nav's Projects, as the board's picker offers them.
    fn task_targets(&self) -> Vec<Target> {
        self.cockpit
            .registry()
            .projects()
            .iter()
            .filter(|project| {
                project.root != std::path::Path::new("/")
                    || self
                        .cockpit
                        .threads()
                        .into_iter()
                        .chain(self.facts.parked().iter().copied())
                        .any(|thread| {
                            self.cockpit.project_id(thread) == Some(project.id)
                                && !self.projects.sidebar.owned.contains(&thread)
                        })
            })
            .map(|project| Target {
                title: project.title.clone().into(),
                root: project.root.clone(),
                shown: home_relative(&project.root).into(),
            })
            .collect()
    }

    /// Where new tasks go when the board opens: the focused Thread's
    /// Project, else the first Project; `None` with no Projects.
    pub(super) fn task_aim(&self) -> Option<std::path::PathBuf> {
        let targets = self.task_targets();
        self.focused_thread()
            .and_then(|thread| self.cockpit.project_id(thread))
            .and_then(|project| self.cockpit.registry().project(project))
            .and_then(|project| targets.iter().find(|target| target.root == project.root))
            .or_else(|| targets.first())
            .map(|target| target.root.clone())
    }

    /// The board's door in the bottom bar, after the bell: the board glyph,
    /// lit while the board shows; a click does what the board key does.
    /// None outside the operator's window, where no board opens.
    pub(super) fn board_door(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.projects.live {
            return None;
        }
        let shown = self
            .projects
            .board
            .as_ref()
            .is_some_and(|board| board.shown);
        let id = gpui::ElementId::from("projects-board-door");
        let key = crate::pointer::hover_key(&id);
        let (rest, glyph): (gpui::Hsla, gpui::Hsla) = if shown {
            (paint::HOVER.into(), rgb(TEXT).into())
        } else {
            (
                gpui::rgba(TRANSPARENT).into(),
                crate::motion::hover_blend(&key, rgb(TEXT_MUTED).into(), rgb(TEXT).into()),
            )
        };
        let door = crate::components::faded_button(
            id,
            rest,
            paint::HOVER.into(),
            paint::PRESS.into(),
            rgb(TEXT_MUTED).into(),
            cx,
        )
        .debug_selector(|| "projects-board-door".into())
        .w(px(ICON_BUTTON))
        .h_full()
        .p_0()
        .accessibility_label("Tasks")
        .child(
            crate::icons::icon(crate::icons::BOARD, ICON_BUTTON_GLYPH, TEXT_MUTED)
                .text_color(glyph),
        )
        .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
            cx.stop_propagation();
            view.toggle_projects(window, cx);
            cx.notify();
        }));
        Some(
            div()
                .id("projects-board-door-tip")
                .flex_shrink_0()
                .h_full()
                .tooltip(crate::menu::action_tooltip("Tasks", "projects::Toggle"))
                .child(door)
                .into_any_element(),
        )
    }

    /// Whether the board shows and holds the keyboard: shown but left (a
    /// press in the nav), the board key takes you back to it.
    fn board_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.projects
            .board
            .as_ref()
            .filter(|board| board.shown)
            .is_some_and(|board| board.view.read(cx).focus.contains_focused(window, cx))
    }

    /// The board, opened where `board.open` says — or a tab's, shown again
    /// as you left it — new tasks going to `aim` when it names a folder,
    /// else, opening, to `task_aim`'s.
    fn show_board(
        &mut self,
        aim: Option<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.projects.live {
            return;
        }
        let back = window.focused(cx);
        let targets = self.task_targets();
        if let Some(board) = self.projects.board.as_ref() {
            board.view.update(cx, |view, cx| {
                view.set_targets(targets);
                if let Some(aim) = &aim {
                    view.aim_at(aim, cx);
                }
            });
        } else {
            let placement = Placement::read();
            let focused = self.focused_thread();
            let opening = Opening {
                cwd: aim
                    .or_else(|| self.task_aim())
                    .unwrap_or_else(sw_board::store::files::user_home),
                from: focused.map(|thread| thread.to_string()).unwrap_or_default(),
                popup: placement == Placement::Popup,
                board_key: crate::board::board_key(),
            };
            let view = cx.new(|cx| {
                let mut view = BoardView::open(opening, cx);
                view.set_targets(targets);
                view
            });
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
    /// takes a tab's board out of the way, still open; the popup closes,
    /// as herdr's does when another tab shows, never left over the Thread.
    /// The move owns the keyboard: nothing goes `back`.
    pub(super) fn board_gives_way(&mut self) {
        let Some(board) = self.projects.board.as_mut() else {
            return;
        };
        match board.placement {
            Placement::Tab => {
                board.shown = false;
                board.back = None;
            }
            Placement::Popup => self.projects.board = None,
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

    /// A reference scene's or a test's board: `board` drawn as it is, at
    /// `placement`.
    #[cfg(any(test, feature = "visual-reference"))]
    pub(super) fn stage_board(
        &mut self,
        board: crate::board::Board,
        placement: Placement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (targets, aim) = (self.task_targets(), self.task_aim());
        let view = cx.new(|cx| {
            let mut view = BoardView::still(board, cx);
            view.set_targets(targets);
            if let Some(aim) = aim {
                view.aim_at(&aim, cx);
            }
            view
        });
        let events = cx.subscribe_in(&view, window, Self::on_board_event);
        self.projects.board = Some(BoardSlot {
            view,
            placement,
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

    /// One of the palette's Tasks commands.
    pub(super) fn run_projects_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.projects.live {
            return;
        }
        match command {
            Command::Board => self.toggle_projects(window, cx),
            Command::Doctor => self.projects_work(projects::doctor, cx),
        }
        cx.notify();
    }

    /// The machine the window is on, by name, when that is not this
    /// computer.
    pub(super) fn away_machine(&self, cx: &gpui::App) -> Option<String> {
        let machines = self.machines.as_ref()?.read(cx);
        let index = machines.index(&machines.selected_key())?;
        machines.machine(index).map(|machine| machine.name.clone())
    }

    /// The palette's Tasks rows, in the operator's window.
    pub(super) fn palette_projects_rows(&self) -> Vec<PaletteRow> {
        if !self.projects.live {
            return Vec::new();
        }
        Command::ALL
            .into_iter()
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

    /// The board header's tally (`3 need you · 1 ready · 2 working`), while
    /// the board has tasks; a press opens the board.
    pub(super) fn projects_bar_word(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let tally = &self.projects.sidebar.tally;
        #[cfg(test)]
        super::drawn::record_bar_tally((!tally.is_empty()).then(|| tally.clone()));
        (!tally.is_empty()).then(|| {
            div()
                .id("bottom-bar-projects")
                .debug_selector(|| "bottom-bar-projects".into())
                .flex_shrink_0()
                .text_color(rgb(crate::theme::CHROME_MUTED))
                .cursor_pointer()
                .child(SharedString::from(tally.clone()))
                .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    window.dispatch_action(Box::new(projects::Toggle), cx);
                }))
                .into_any_element()
        })
    }
}
