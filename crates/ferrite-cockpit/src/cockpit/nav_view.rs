//! The nav as a view of its own (ADR 0041). It owns what only the nav
//! reads: its list's motion and folds, its scroll, its keyboard and
//! cursor, its search, the cards picked or swept, the undo notice and the
//! Snooze field. It draws from what the Cockpit gives it (`NavInputs`),
//! read through the Cockpit each render, so the Cockpit's own notify still
//! redraws it; and what the operator asks of the window it says as a
//! `NavEvent`, which the Cockpit answers once the nav's own update has
//! ended.

use super::nav_chrome::{NavUndo, UndoAct};
use super::nav_pick::ParkSweep;
use super::*;
use crate::machines::MachineRow;
use crate::nav::{NavCursor, Shelf};
use ferrite_core::transcript::RateLimits;
use ferrite_pane::link_target::Open;
use ferrite_ui::motion::Reveal;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use gpui::{App, WeakEntity};
use std::rc::Rc;

pub(crate) struct NavView {
    pub(super) cockpit: WeakEntity<CockpitView>,
    /// The list's motion (`nav::flip`), shared with its elements.
    pub(super) motion: nav_list::NavMotion,
    /// The next frame's list stands still rather than sliding: a shelf
    /// folding or the column sliding moves its rows already.
    pub(super) snap: Cell<bool>,
    /// Each shelf's body as far open as it is on screen.
    pub(super) shelf_folds: RefCell<Vec<(Shelf, Reveal)>>,
    /// The window's size the list was last drawn at: a resize moves rows
    /// without the list's motion.
    pub(super) viewport: Cell<gpui::Size<Pixels>>,
    /// How many of the Parked shelf's rows it draws open (`Show more`).
    pub(super) parked_shown: usize,
    /// The list's scroll, shared with the hand-drawn scrollbar beside it:
    /// gpui 0.2.2 paints none of its own.
    pub(super) scroll: ScrollHandle,
    /// The list's keyboard (⌘I, ⌘⇧E, `nav_keys`): Esc gives it back to the
    /// board.
    pub(super) focus: FocusHandle,
    /// Where the list's keyboard stands (`nav_keys`), while it has one.
    pub(super) cursor: Option<NavCursor>,
    /// ⌘I or ⌘⇧E handed the list the keys, and it has not let go: only
    /// then does the Cockpit's focus rule leave them on the list.
    pub(super) keys_held: bool,
    /// Clears the cursor when the list lets go of the keys.
    pub(super) blur: Option<gpui::Subscription>,
    /// Set by a cursor move until its row has scrolled into view
    /// (`cursor_reveal`).
    pub(super) reveal: Rc<Cell<bool>>,
    /// Where the cursor's row was last drawn: ⇧F10 opens its menu there.
    pub(super) cursor_bounds: Rc<Cell<Option<gpui::Bounds<Pixels>>>>,
    /// The Thread whose card last scrolled into view for being shown
    /// (`shown_reveal`): a focus change scrolls the new one's.
    pub(super) shown: Rc<Cell<Option<ThreadId>>>,
    /// The search field (`nav_chrome`), and the Thread its ↑/↓ stand on: a
    /// Thread, not a place in the results, which live updates reorder.
    pub(super) search: Entity<Composer>,
    pub(super) search_pick: Cell<Option<ThreadId>>,
    /// The foot's undo notice: what the last Park or Snooze did, until it
    /// fades.
    pub(super) undo: Option<NavUndo>,
    /// The cards ⌘- and ⇧-click picked (`nav_pick`), in picking order.
    pub(super) picked: Vec<ThreadId>,
    /// A Park press held down and sweeping across cards.
    pub(super) park_sweep: Option<ParkSweep>,
    /// The Thread cards in the order the list last drew them: what a
    /// ⇧-click and a sweep span.
    pub(super) drawn: RefCell<Vec<ThreadId>>,
    /// The pointer is over the nav's column: its scrollbar shows.
    pub(super) hovered: bool,
    /// A Thread's Custom… snooze field, over the list (`snooze`).
    pub(super) snooze: Option<snooze::Editor>,
    /// The rows the nav last drew: a stream that leaves them as they are
    /// draws no nav (`rows_moved`).
    built: Option<nav::NavState>,
}

/// What the nav draws from, as the Cockpit has it this frame.
pub(crate) struct NavInputs {
    pub state: nav::NavState,
    pub machines: Option<Vec<MachineRow>>,
    pub machine_lost: bool,
    pub inbox: Vec<NoticeRow>,
    pub open_shelves: Vec<Shelf>,
    pub width: f32,
    /// The Thread the board shows, the empty board's too.
    pub shown: Option<ThreadId>,
    pub notice: Option<SharedString>,
    /// The Project the list is filtered to, by its title.
    pub filtered: Option<SharedString>,
    /// The Factory door, and whether one of its boards needs the operator;
    /// none without a Project to show.
    pub factory: Option<bool>,
    /// The context menu standing, by what it is on.
    pub menu: Option<MenuTarget>,
    pub doors: menu_button::Doors,
    pub open: HashMap<ThreadId, OpenThread>,
    pub rename: Option<(ThreadId, Entity<Composer>)>,
    pub limits: RateLimits,
    pub free_disk: Option<u64>,
    /// The update door's tip, while a release waits.
    pub update: Option<SharedString>,
}

/// An open Thread as its row's doors need it.
pub(crate) struct OpenThread {
    pub queued: bool,
    pub snooze_until: Option<std::time::SystemTime>,
}

impl NavInputs {
    pub(crate) fn shelf_open(&self, shelf: Shelf) -> bool {
        self.open_shelves.contains(&shelf)
    }

    /// The window is on a machine whose list folds under its heading.
    pub(crate) fn machine_folded(&self) -> bool {
        self.machines
            .as_ref()
            .and_then(|rows| rows.iter().find(|row| row.selected))
            .is_some_and(|on| on.folded)
    }
}

/// Where a menu the nav asks for stands.
#[derive(Clone, Copy, Debug)]
pub(crate) enum MenuAt {
    Point(gpui::Point<Pixels>),
    /// Under a row, as ⇧F10 opens it.
    Under(gpui::Bounds<Pixels>),
}

/// What the nav asks of the window.
#[derive(Clone, Debug)]
pub(crate) enum NavEvent {
    /// A Thread row's press, or ⏎ on it: an open Thread takes the screen,
    /// a parked one revives.
    Open(ThreadId),
    /// A search result's press or ⏎: it opens, and then the search clears
    /// and the keys go back to the board.
    OpenResult(ThreadId),
    Notice(Verb),
    Revive(ThreadId),
    Wake(ThreadId),
    Park(Vec<ThreadId>),
    Unpin(ThreadId),
    Rename(ThreadId),
    OpenPr(ThreadId, Open),
    Menu(MenuTarget, MenuAt),
    /// A press on a menu button (`menu_button`).
    Door(menu_button::Door),
    SetShelfOpen(Shelf, bool),
    Undo(UndoAct),
    FoldMachine(usize),
    GoToMachine(usize),
    /// The Snooze field's ⏎ on a time it read: snooze the Thread until
    /// then, and answer the field (`snooze_answered`).
    Snooze(ThreadId, std::time::SystemTime),
    AddProject,
    Settings,
    PullRequests,
    Update,
}

impl EventEmitter<NavEvent> for NavView {}

impl menu_button::MenuHost for NavView {
    fn press_door(&mut self, door: menu_button::Door, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(NavEvent::Door(door));
    }

    fn tip<E: InteractiveElement>(element: E, text: SharedString) -> E {
        use crate::nav::NavTooltip as _;
        element.nav_tooltip(text)
    }
}

impl NavView {
    pub(super) fn new(cockpit: WeakEntity<CockpitView>, cx: &mut Context<Self>) -> Self {
        Self {
            cockpit,
            motion: Default::default(),
            snap: Cell::new(false),
            shelf_folds: Default::default(),
            viewport: Default::default(),
            parked_shown: ferrite_ui::theme::NAV_PARKED_PAGE_FIRST,
            scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            cursor: None,
            keys_held: false,
            blur: None,
            reveal: Default::default(),
            cursor_bounds: Default::default(),
            shown: Default::default(),
            search: nav_chrome::search_line(cx),
            search_pick: Default::default(),
            undo: None,
            picked: Vec::new(),
            park_sweep: None,
            drawn: Default::default(),
            hovered: false,
            snooze: None,
            built: None,
        }
    }

    /// What the Cockpit gives the nav now. Read only outside the
    /// Cockpit's own update: from the nav's render and its listeners.
    pub(super) fn inputs(&self, cx: &App) -> Option<NavInputs> {
        let cockpit = self.cockpit.upgrade()?;
        Some(cockpit.read(cx).nav_inputs(cx))
    }

    /// The list holds the keyboard it was handed.
    pub(crate) fn holds_keys(&self, window: &Window) -> bool {
        self.keys_held && self.focus.is_focused(window)
    }

    /// Whether `state`'s rows differ from the ones the nav last drew.
    pub(super) fn rows_moved(&self, state: &nav::NavState) -> bool {
        self.built.as_ref() != Some(state)
    }

    /// The pointer came into the nav's column or left it.
    pub(super) fn set_hovered(&mut self, inside: bool, cx: &mut Context<Self>) {
        if inside != self.hovered {
            self.hovered = inside;
            cx.notify();
        }
    }
}

impl Render for NavView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ferrite_ui::loops_overlay::begin(window, cx.entity_id());
        let Some(mut inputs) = self.inputs(cx) else {
            return div().into_any_element();
        };
        self.built = Some(inputs.state.clone());
        for row in [
            &mut inputs.state.pinned,
            &mut inputs.state.active,
            &mut inputs.state.working,
            &mut inputs.state.snoozed,
            &mut inputs.state.parked,
        ]
        .into_iter()
        .flatten()
        {
            row.picked = self.card_picked(row.thread);
        }
        self.content(&inputs, window, cx).into_any_element()
    }
}

impl CockpitView {
    /// Everything the nav draws from, as plain values. It never reads the
    /// nav: the nav calls it while its own update is running.
    pub(super) fn nav_inputs(&self, cx: &App) -> NavInputs {
        use crate::ferrite_update::State;
        let state = self.nav_state();
        let inbox = self.inbox_rows(&state, cx);
        let projects = self.factory_nav_projects(&state);
        let factory = (!projects.is_empty()).then(|| {
            projects
                .iter()
                .any(|project| self.cockpit.factory().needs_you(*project) > 0)
        });
        let open = state
            .open_rows()
            .chain(&state.parked)
            .filter_map(|row| {
                let open = self.cockpit.thread(row.thread)?;
                Some((
                    row.thread,
                    OpenThread {
                        queued: !open.queue().is_empty(),
                        snooze_until: open.snooze().map(|snooze| snooze.until),
                    },
                ))
            })
            .collect();
        let shown = self.focused_thread();
        let provider = shown
            .and_then(|thread| self.cockpit.thread(thread))
            .map(|open| open.provider())
            .unwrap_or(self.launch_provider);
        let update = match self.ferrite_update.state() {
            State::Ready { version, .. } => Some(format!("Restart to update to {version}").into()),
            State::HandedOver { version } => Some(format!("Install Ferrite {version}").into()),
            _ => None,
        };
        NavInputs {
            machines: self.machine_rows(cx),
            machine_lost: self.machine_lost(),
            inbox,
            open_shelves: self.prefs.settings.open_shelves.clone(),
            width: self.nav_expanded_width(),
            shown,
            notice: self.nav_notice.text().cloned(),
            filtered: self
                .nav_filter
                .and_then(|project| self.cockpit.registry().project(project))
                .map(|project| project.title.clone().into()),
            factory,
            menu: self.context_menu().map(|menu| menu.target),
            doors: self.doors(),
            open,
            rename: match &self.rename {
                Some((RenameTarget::Thread(thread), editor)) => Some((*thread, editor.clone())),
                _ => None,
            },
            limits: self.cockpit.account_limits(provider),
            free_disk: self.free_disk,
            update,
            state,
        }
    }

    pub(super) fn on_nav_event(
        &mut self,
        _: &Entity<NavView>,
        event: &NavEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.clone() {
            NavEvent::Open(thread) => {
                self.open_nav_row(thread, cx);
            }
            NavEvent::OpenResult(thread) => {
                if self.open_nav_row(thread, cx) {
                    self.nav_view.update(cx, |nav, cx| nav.clear_search(cx));
                    window.blur(cx);
                }
            }
            NavEvent::Notice(verb) => self.notice_verb(verb, cx),
            NavEvent::Revive(thread) => {
                self.revive_thread(thread, cx);
            }
            NavEvent::Wake(thread) => self.snooze_thread(thread, None, cx),
            NavEvent::Park(threads) => self.park_listed(&threads, cx),
            NavEvent::Unpin(thread) => self.set_pinned(thread, false, cx),
            NavEvent::Rename(thread) => self.start_rename(RenameTarget::Thread(thread), cx),
            NavEvent::OpenPr(thread, how) => self.open_pr_as(thread, how, window, cx),
            NavEvent::Menu(target, at) => match at {
                MenuAt::Point(at) => self.open_context_menu(target, at, cx),
                MenuAt::Under(row) => self.open_context_menu_under(target, row, cx),
            },
            NavEvent::Door(door) => self.toggle_door(door, cx),
            NavEvent::SetShelfOpen(shelf, open) => self.set_shelf_open(shelf, open, cx),
            NavEvent::Undo(UndoAct::Parked(parked)) => {
                self.undo_parked(parked, cx);
                cx.notify();
            }
            NavEvent::Undo(UndoAct::Snoozed(thread)) => self.snooze_thread(thread, None, cx),
            NavEvent::FoldMachine(index) => self.fold_machine(index, cx),
            NavEvent::GoToMachine(index) => cx.emit(GoToMachine {
                index: Some(index),
                open: None,
            }),
            NavEvent::Snooze(thread, until) => {
                let refused = match self.cockpit.snooze_thread(thread, Some(until)) {
                    Ok(()) => {
                        self.nav_notice.clear();
                        None
                    }
                    Err(error) => Some(error.to_string().into()),
                };
                self.nav_view
                    .update(cx, |nav, cx| nav.snooze_answered(refused, cx));
                cx.notify();
            }
            NavEvent::AddProject => self.open_project_creator(cx),
            NavEvent::Settings => self.toggle_settings(cx),
            NavEvent::PullRequests => self.open_prs_from_foot(window, cx),
            NavEvent::Update => self.press_ferrite_update(window, cx),
        }
    }
}
