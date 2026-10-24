//! The board running in a GPUI view, as upstream's terminal runtime
//! (`board/run/mod.rs`, `start.rs`) runs it in a terminal: one `State`,
//! every event fed to `update`, what it answers done, the screen drawn.
//! Nothing slow runs on the UI thread: the loader reads once a second and
//! right after anything you did, each job runs on the background executor,
//! and each answers back as an event, so a key is answered from what was
//! last read.
//!
//! What only a terminal board does is not done here: herdr's first-run
//! setup, the held-size warning (`settle_ms` 0: a resize lays out at
//! once), OSC 7501 records, attaching this terminal to an agent, the web
//! UI's offer, and loading a new binary of itself (the board is part of
//! Ferrite, which updates as a whole, so `herdr-projects was updated` is
//! never said).

use std::path::PathBuf;

use futures::channel::mpsc;
use futures::StreamExt as _;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, App, Context, EventEmitter, FocusHandle, Focusable, KeyDownEvent, Window,
};
use sw_board::board::data::{self, Wants};
use sw_board::board::job;
use sw_board::board::keys::key_name;
use sw_board::board::run::{place, prefs, start};
use sw_board::board::state::{Ctx, State};
use sw_board::board::theme::Theme;
use sw_board::board::update::{update, Effect, Event};
use sw_board::board::view::screen;
use sw_board::shown::ink::{Ink, Inked};

use super::{aim, controls, keys, machines, mouse, render};
use crate::components;
use crate::theme::{CARD, CH, ROW, SPACE_1};

/// How often the board ticks and the loader looks again.
const BEAT: std::time::Duration = std::time::Duration::from_secs(1);
/// The popup's edge, inside which its cells are.
const POPUP_EDGE: f32 = 1.0;
/// The cells' inset from the board's sides and from the controls above:
/// with the cell the board's own lines start in, their words stand on the
/// controls' text edge (`SPACE_3`), and the first line is clear of the
/// controls' hairline.
pub(super) const GRID_INSET: f32 = SPACE_1;
/// How long the popup says how to come back before it closes on an
/// opened agent (`Ctx::leave_ms`, as upstream's runtime sets it).
const LEAVE_MS: i64 = 700;
/// What the board says where upstream would hand its terminal to an agent.
const ATTACH_REFUSED: &str =
    "This agent can only be attached from a terminal: open it on its own machine.";

/// Where the board opens, and what it is opened with.
#[derive(Clone, Debug)]
pub(crate) struct Opening {
    /// The folder it opens in, new tasks' (`aim`): a nav Project's root,
    /// else home.
    pub cwd: PathBuf,
    /// The focused Thread's id, the pane its agent is to the adapter: the
    /// popup opens on the task whose agent that is (`Ctx::from`).
    pub from: String,
    /// Over the board as a sheet (`board.open: popup`), not in its place.
    pub popup: bool,
    /// The key that opens and leaves it, as the board writes keys.
    pub board_key: String,
}

/// What the board asks of the cockpit.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BoardEvent {
    Close,
    /// `board.notify: bell`: a task newly needs you.
    Bell,
}

/// The board's state and the theme it draws in, once read.
pub(crate) struct Board {
    pub state: State,
    pub theme: Theme,
}

pub(crate) struct BoardView {
    pub focus: FocusHandle,
    pub(super) board: Option<Board>,
    /// Where new tasks go, and the strip that picks it.
    pub(super) aim: aim::Aim,
    pub(super) controls: controls::Controls,
    /// The view's size in pixels, as the cockpit hangs it, and in cells
    /// (columns, lines): what the board lays its screen out for.
    pixels: (f32, f32),
    size: (usize, usize),
    /// What the loader looks closer at, and the poke that has it read now.
    wants: Wants,
    kick: Option<mpsc::UnboundedSender<()>>,
    /// When other machines were last asked what (`machines`).
    polling: machines::Polling,
    /// The mouse: what is selected, the links, where the cells are.
    pub(super) pointer: mouse::Pointer,
    /// Whether its view is saved for the next time it opens: never for a
    /// still board, which is no one's.
    keeps_view: bool,
}

impl EventEmitter<BoardEvent> for BoardView {}

/// The view is kept for the next time the board opens, however it closes.
impl Drop for BoardView {
    fn drop(&mut self) {
        self.save();
    }
}

impl Focusable for BoardView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl BoardView {
    /// The board, read from disk alone off the UI thread (its first
    /// frame), then kept fresh by the loader and the beat.
    pub(crate) fn open(opening: Opening, cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            let board = cx.background_spawn(async move { prepare(opening) }).await;
            this.update(cx, |view, cx| view.start(board, cx)).ok();
        })
        .detach();
        Self::new(None, true, cx)
    }

    /// A board drawn from `board` as it is: no loader, no beat — what the
    /// tests and the reference scene draw.
    #[cfg(any(test, feature = "visual-reference"))]
    pub(crate) fn still(board: Board, cx: &mut Context<Self>) -> Self {
        Self::new(Some(board), false, cx)
    }

    fn new(board: Option<Board>, keeps_view: bool, cx: &mut Context<Self>) -> Self {
        crate::theme::init_components(cx);
        Self {
            focus: cx.focus_handle(),
            board,
            aim: aim::Aim::default(),
            controls: controls::Controls::default(),
            pixels: (120.0 * CH, 40.0 * ROW),
            size: (120, 40),
            wants: Wants::default(),
            kick: None,
            polling: machines::Polling::default(),
            pointer: mouse::Pointer::default(),
            keeps_view,
        }
    }

    /// Fit the board to a box `width` by `height` pixels: it lays its
    /// screen out for the cells inside, again on every change (the popup's
    /// inside is within its edge).
    pub(crate) fn fit(&mut self, width: f32, height: f32) {
        self.pixels = (width, height);
        let edge = if self.popup() { 2.0 * POPUP_EDGE } else { 0.0 };
        self.size = cells(
            width - edge - 2.0 * GRID_INSET,
            height - edge - GRID_INSET - self.aim.height() - controls::HEIGHT,
        );
    }

    fn popup(&self) -> bool {
        self.board
            .as_ref()
            .is_some_and(|board| board.state.ctx.popup)
    }

    /// The board read: the beat and the loader start, and other machines
    /// are asked at once when it shows them.
    fn start(&mut self, mut board: Board, cx: &mut Context<Self>) {
        if let Some(cwd) = self.aim.early.take() {
            board.state.ctx.cwd = cwd;
        }
        self.wants = Wants {
            ids: vec![board.state.sel.clone()],
            cwd: board.state.ctx.cwd.clone(),
            own_pane: board.state.ctx.own_pane.clone(),
            web: false,
        };
        let all = board.state.all();
        self.board = Some(board);
        self.start_loader(cx);
        self.start_beat(cx);
        if all {
            self.poll(true, cx);
        }
        cx.notify();
    }

    fn start_beat(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(BEAT).await;
            let now = sw_board::clock::now();
            if this
                .update(cx, |view, cx| view.feed(Event::Tick(now), cx))
                .is_err()
            {
                break;
            }
        })
        .detach();
    }

    /// The loader: a read now, then one each beat or as soon as it is
    /// poked. With no keeper alive it does the keeper's tick itself.
    fn start_loader(&mut self, cx: &mut Context<Self>) {
        let (kick, mut kicked) = mpsc::unbounded();
        self.kick = Some(kick);
        cx.spawn(async move |this, cx| loop {
            let Ok(wants) = this.read_with(cx, |view, _| view.wants.clone()) else {
                break;
            };
            let read = cx
                .background_spawn(async move {
                    if !sw_board::keeper::keeper_alive() {
                        sw_board::keeper::tick_once(&sw_board::herdr::cli::current_socket(), true);
                    }
                    data::load(&wants)
                })
                .await;
            let fed = this.update(cx, |view, cx| view.feed(Event::Loaded(Box::new(read)), cx));
            if fed.is_err() {
                break;
            }
            let beat = cx.background_executor().timer(BEAT);
            futures::future::select(Box::pin(beat), kicked.next()).await;
            while kicked.try_recv().is_ok() {}
        })
        .detach();
    }

    /// One event through `update`, and what it answers done.
    pub(crate) fn feed(&mut self, event: Event, cx: &mut Context<Self>) {
        let Some(board) = self.board.as_mut() else {
            return;
        };
        let effects = update(&mut board.state, event);
        for effect in effects {
            self.effect(effect, cx);
        }
        self.follow_selection();
        cx.notify();
    }

    pub(super) fn effect(&mut self, effect: Effect, cx: &mut Context<Self>) {
        match effect {
            Effect::Run(job) => answer(cx, move || Event::Done(job::run(job))),
            Effect::Load => self.kick(),
            Effect::Save => self.save(),
            Effect::Poll(now_too) => self.poll(now_too, cx),
            Effect::Bell => cx.emit(BoardEvent::Bell),
            Effect::Close => {
                self.save();
                cx.emit(BoardEvent::Close);
            }
            // Another machine's agent on an older plugin, or no herdr
            // client: upstream hands its terminal over; a window cannot.
            Effect::Attach { .. } => {
                if let Some(board) = self.board.as_mut() {
                    board.state.flash(ATTACH_REFUSED, Ink::Yellow);
                }
            }
            // A terminal's alone: the tasks told to it (OSC 7501), a new
            // binary loaded in place, a size taken back from a remote
            // view, herdr's web UI offered.
            Effect::Osc(_) | Effect::Reload | Effect::Take { .. } | Effect::LookWebUi => {}
        }
    }

    /// The view kept for the next time the board opens (`view.json`, the
    /// popup's `view-popup.json`).
    fn save(&self) {
        if let Some(board) = self.board.as_ref().filter(|_| self.keeps_view) {
            prefs::save(&board.state);
        }
    }

    fn kick(&self) {
        if let Some(kick) = &self.kick {
            kick.unbounded_send(()).ok();
        }
    }

    /// What the loader reads closer follows the selected task and the one
    /// the chat talks to; a change has it read now.
    pub(super) fn follow_selection(&mut self) {
        let Some(board) = &self.board else {
            return;
        };
        let state = &board.state;
        let ids = vec![state.sel.clone(), state.reply_id.clone()];
        if self.wants.ids != ids || self.wants.cwd != state.ctx.cwd {
            self.wants.ids = ids;
            self.wants.cwd = state.ctx.cwd.clone();
            self.kick();
        }
    }

    /// Ask other machines what is due (`machines::Polling`; `now_too`: also
    /// one that did not answer a moment ago), each answer fed back.
    fn poll(&mut self, now_too: bool, cx: &mut Context<Self>) {
        let Some(board) = self.board.as_mut() else {
            return;
        };
        for ask in self
            .polling
            .due(&mut board.state, sw_board::clock::now(), now_too)
        {
            answer(cx, move || ask.answer());
        }
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // The picker's list, while it is open, has the keys.
        if self.aim.open || self.controls.open.is_some() {
            return;
        }
        let keystroke = &event.keystroke;
        let event = if keys::is_paste(keystroke) {
            let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
                return;
            };
            Event::Paste(text)
        } else {
            match keys::press_of(keystroke) {
                Some(press) => Event::Key(press),
                None => return,
            }
        };
        cx.stop_propagation();
        window.prevent_default();
        self.feed(event, cx);
    }

    /// The screen's lines now, laid out for the view's size; a view that
    /// panics says so in two lines, as upstream's runtime does.
    pub(crate) fn screen(&mut self) -> Vec<Inked> {
        let Some(board) = self.board.as_mut() else {
            return Vec::new();
        };
        let (columns, rows) = self.size;
        let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            screen(&mut board.state, &board.theme, columns, rows)
        }));
        drawn.map_or_else(|_| view_error(), |screen| screen.lines)
    }

    pub(crate) fn state(&mut self) -> Option<&mut State> {
        self.board.as_mut().map(|board| &mut board.state)
    }
}

/// What a view that panicked whole says, as upstream's runtime says it.
pub(super) fn view_error() -> Vec<Inked> {
    vec![
        Inked::plain(" herdr-projects view error"),
        Inked::plain(format!(" press {} to close", key_name("back"))),
    ]
}

/// Run `work` on the background executor and feed the board what it
/// answers.
fn answer(cx: &mut Context<BoardView>, work: impl FnOnce() -> Event + Send + 'static) {
    cx.spawn(async move |this, cx| {
        let event = cx.background_spawn(async move { work() }).await;
        this.update(cx, |view, cx| view.feed(event, cx)).ok();
    })
    .detach();
}

/// What the board opens on, read from disk alone (`run/start.rs`'s
/// `prepare`, without the terminal's): the view it had when it last
/// closed, the notes the store has to say once, and its theme.
///
/// Two of upstream's are not done here. `own_key` stays unset: herdr hands
/// its popup its own key, which the board must catch to close; here
/// `projects::Toggle` is bound around the board and acts before it. And
/// the keeper is not started (`keeper::ensure_keeper` runs the
/// `herdr-projects keeper` command, which Ferrite's binary is not; ADR
/// 0031, Seam 3): with no keeper alive the loader ticks for it.
fn prepare(opening: Opening) -> Board {
    use sw_board::store::{files, keys_given, move_home};
    let ctx = Ctx {
        cwd: opening.cwd.display().to_string(),
        from: opening.from,
        popup: opening.popup,
        own_pane: if opening.popup {
            place::POPUP.into()
        } else {
            String::new()
        },
        board_key: opening.board_key,
        leave_ms: LEAVE_MS,
        // A window, never a bare terminal: another machine's agent is
        // asked to come to the front there rather than handed this
        // terminal. (This machine's agents are opened by the job, which
        // asks `cli::in_herdr`, true under Ferrite.)
        in_herdr: true,
        ..Ctx::default()
    };
    let keys = keys_given::keys_note();
    let mut state = start::view(ctx, &opening.cwd);
    let (home, env) = (files::user_home(), move_home::env_now());
    let moved = move_home::move_note(&home, &env);
    if !moved.is_empty() {
        move_home::move_told(&home, &env);
    }
    let said: Vec<String> = [moved, keys]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect();
    if !said.is_empty() {
        state.flash(said.join("  "), Ink::Green);
    }
    Board {
        state,
        theme: render::theme_set(),
    }
}

/// How many cells a box `width` by `height` pixels holds.
fn cells(width: f32, height: f32) -> (usize, usize) {
    let columns = (width / CH).floor().max(10.0) as usize;
    let rows = (height / ROW).floor().max(4.0) as usize;
    (columns, rows)
}

impl Render for BoardView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let screen = self.screen();
        let lines = self.pointer.frame(screen);
        let grounds = match self.board.as_ref() {
            Some(board) => self.pointer.grounds(&board.state),
            None => Vec::new(),
        };
        let theme = self.board.as_ref().map(|board| &board.theme);
        // The reading plane under the board, popup too: its greys are held
        // readable on it; a named theme paints its own ground.
        let ground = theme.and_then(render::ground).unwrap_or(CARD);
        let rows = theme
            .map(|theme| render::lines(&lines, theme))
            .unwrap_or_default();
        let surface = if self.popup() {
            crate::menu::float()
        } else {
            div()
        };
        let (width, height) = self.pixels;
        let strip = self.aim_strip(cx);
        let controls = self.controls(cx);
        let surface = surface
            .debug_selector(|| "projects-board".into())
            .track_focus(&self.focus)
            .key_context("Projects")
            .w(px(width))
            .h(px(height))
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(ground))
            .on_key_down(cx.listener(Self::on_key));
        mouse::listen(surface, cx)
            .children(strip)
            .child(controls)
            .child(
                div().px(px(GRID_INSET)).pt(px(GRID_INSET)).child(
                    components::text_ui()
                        .debug_selector(|| "projects-board-rows".into())
                        .relative()
                        .flex()
                        .flex_col()
                        .children(grounds)
                        .children(rows)
                        .child(self.pointer.layer()),
                ),
            )
    }
}
