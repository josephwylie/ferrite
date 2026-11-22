//! The nav as a view of its own (ADR 0041). It owns what only the nav
//! reads: its list's motion and folds, its scroll, its keyboard and
//! cursor, its search, the cards picked or swept, the undo notice, the
//! Snooze field, and when each Thread came into its section. It reads
//! what it draws (`NavInputs`) itself, as the Cockpit notifies or the
//! client's model says something `Arrived` (`look`), and its render draws only that, so it redraws
//! only when what it draws moved. What the operator asks of the window it
//! says as a `NavEvent`, which the Cockpit answers once the nav's own
//! update has ended.

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
    /// The client's model, which the nav reads what it lists from.
    model: Entity<Cockpit>,
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
    pub(super) shown: Rc<Cell<Option<ThreadKey>>>,
    /// The search field (`nav_chrome`), and the Thread its ↑/↓ stand on: a
    /// Thread, not a place in the results, which live updates reorder.
    pub(super) search: Entity<Composer>,
    pub(super) search_pick: Cell<Option<ThreadKey>>,
    /// The foot's undo notice: what the last Park or Snooze did, until it
    /// fades.
    pub(super) undo: Option<NavUndo>,
    /// The cards ⌘- and ⇧-click picked (`nav_pick`), in picking order.
    pub(super) picked: Vec<ThreadKey>,
    /// A Park press held down and sweeping across cards.
    pub(super) park_sweep: Option<ParkSweep>,
    /// The Thread cards in the order the list last drew them: what a
    /// ⇧-click and a sweep span.
    pub(super) drawn: RefCell<Vec<ThreadKey>>,
    /// The pointer is over the nav's column: its scrollbar shows.
    pub(super) hovered: bool,
    /// A Thread's Custom… snooze field, over the list (`snooze`).
    pub(super) snooze: Option<snooze::Editor>,
    /// What the nav draws, as it last read it (`look`). Its render reads
    /// nothing else, so only a change here redraws it.
    inputs: Option<NavInputs>,
    /// When each listed Thread came into its section, which the list and
    /// the Working shelf order by. Only `look` writes it.
    arrivals: nav::Arrivals,
    /// The Projects the Factory door lists, as `look` last read them: its
    /// menu's rows (`factory_rows`). The door draws only whether one needs
    /// the operator, so a change here alone does not redraw the nav.
    factories: Vec<ProjectId>,
    /// The Cockpit's notify and the model's `Arrived`, each of which has
    /// the nav look again.
    watching: Vec<gpui::Subscription>,
}

/// What the nav draws from, as it last read it.
#[derive(Clone, PartialEq)]
pub(crate) struct NavInputs {
    pub state: nav::NavState,
    pub machines: Option<Vec<MachineRow>>,
    pub machine_lost: bool,
    pub inbox: Vec<NoticeRow>,
    pub open_shelves: Vec<Shelf>,
    pub width: f32,
    /// The Thread the board shows, the empty board's too.
    pub shown: Option<ThreadKey>,
    pub notice: Option<SharedString>,
    /// The Project the list is filtered to, by its title.
    pub filtered: Option<SharedString>,
    /// The Factory door, and whether one of its boards needs the operator;
    /// none without a Project to show.
    pub factory: Option<bool>,
    /// The context menu standing, by what it is on.
    pub menu: Option<MenuTarget>,
    pub doors: menu_button::Doors,
    pub open: HashMap<ThreadKey, OpenThread>,
    pub rename: Option<(ThreadKey, Entity<Composer>)>,
    pub limits: RateLimits,
    pub free_disk: Option<u64>,
    /// The update door's tip, while a release waits.
    pub update: Option<SharedString>,
}

/// An open Thread as its row's doors need it.
#[derive(Clone, PartialEq)]
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
    Open(ThreadKey),
    /// A search result's press or ⏎: it opens, and then the search clears
    /// and the keys go back to the board.
    OpenResult(ThreadKey),
    Notice(Verb),
    Revive(ThreadKey),
    Wake(ThreadKey),
    Park(Vec<ThreadKey>),
    Unpin(ThreadKey),
    Rename(ThreadKey),
    OpenPr(ThreadKey, Open),
    Menu(MenuTarget, MenuAt),
    /// A press on a menu button (`menu_button`).
    Door(menu_button::Door),
    SetShelfOpen(Shelf, bool),
    Undo(UndoAct),
    FoldMachine(usize),
    GoToMachine(usize),
    /// The Snooze field's ⏎ on a time it read: snooze the Thread until
    /// then, and answer the field (`snooze_answered`).
    Snooze(ThreadKey, std::time::SystemTime),
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
    pub(super) fn new(
        cockpit: WeakEntity<CockpitView>,
        model: Entity<Cockpit>,
        cx: &mut Context<Self>,
    ) -> Self {
        // The Cockpit is still being made: the nav first looks once it is.
        let nav = cx.weak_entity();
        cx.defer(move |cx| {
            nav.update(cx, |nav, cx| nav.watch(cx)).ok();
        });
        Self {
            cockpit,
            model,
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
            inputs: None,
            arrivals: nav::Arrivals::default(),
            factories: Vec::new(),
            watching: Vec::new(),
        }
    }

    fn watch(&mut self, cx: &mut Context<Self>) {
        let Some(cockpit) = self.cockpit.upgrade() else {
            return;
        };
        self.watching = vec![
            cx.observe(&cockpit, |nav, _, cx| nav.look(cx)),
            cx.subscribe(&self.model, |nav, _, _: &Arrived, cx| nav.look(cx)),
        ];
        self.look(cx);
    }

    /// Read what the nav draws from the Cockpit and the model, note which
    /// Threads came into a section, and redraw when any of it moved. It
    /// runs as either notifies, never in a draw, so a draw never writes.
    pub(super) fn look(&mut self, cx: &mut Context<Self>) {
        let Some(cockpit) = self.cockpit.upgrade() else {
            return;
        };
        let view = cockpit.read(cx);
        let model = self.model.read(cx);
        let listed = listed_threads(view, model, cx);
        self.arrivals
            .note(&listed, ferrite_core::clock::system_time());
        let (inputs, factories) = nav_inputs(view, model, &listed, &self.arrivals, cx);
        self.factories = factories;
        if self.inputs.as_ref() != Some(&inputs) {
            self.inputs = Some(inputs);
            cx.notify();
        }
    }

    /// `look`, once the update under way has ended: what the Cockpit's
    /// own code asks (`notify_part(Part::Nav)`) as it changes something
    /// the nav draws without notifying itself (a PR or a title landed,
    /// branch labels, the column's width).
    pub(super) fn look_again(&mut self, cx: &mut Context<Self>) {
        let nav = cx.weak_entity();
        cx.defer(move |cx| {
            nav.update(cx, |nav, cx| nav.look(cx)).ok();
        });
    }

    /// The Cockpit measured the nav's column anew as it drew.
    pub(super) fn set_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if let Some(inputs) = &mut self.inputs {
            if inputs.width != width {
                inputs.width = width;
                cx.notify();
            }
        }
    }

    /// What the nav draws, as it last read it.
    pub(super) fn inputs(&self) -> Option<&NavInputs> {
        self.inputs.as_ref()
    }

    /// The Projects the Factory door lists, as the nav last read them.
    pub(super) fn factories(&self) -> &[ProjectId] {
        &self.factories
    }

    /// The list holds the keyboard it was handed.
    pub(crate) fn holds_keys(&self, window: &Window) -> bool {
        self.keys_held && self.focus.is_focused(window)
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
        let Some(mut inputs) = self.inputs.clone() else {
            return div().into_any_element();
        };
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
            row.picked = self.card_picked(row.key);
        }
        self.content(&inputs, window, cx).into_any_element()
    }
}

/// Everything the nav draws from the Cockpit, as plain values: `listed`
/// and `arrivals` as `look` noted them. With them, the Projects the
/// Factory door lists.
fn nav_inputs(
    view: &CockpitView,
    model: &Cockpit,
    listed: &[(ThreadKey, nav::Section, Option<std::time::SystemTime>)],
    arrivals: &nav::Arrivals,
    cx: &App,
) -> (NavInputs, Vec<ProjectId>) {
    use crate::ferrite_update::State;
    let state = nav_state(view, model, listed, arrivals, cx);
    let inbox = view.inbox_rows(&state, cx);
    let projects = factory_projects(view, model, &state);
    let factory = (!projects.is_empty()).then(|| {
        projects
            .iter()
            .any(|project| model.factory().needs_you(*project) > 0)
    });
    let open = state
        .open_rows()
        .chain(&state.parked)
        .filter_map(|row| {
            let open = model.thread(row.key.thread())?;
            Some((
                row.key,
                OpenThread {
                    queued: !open.queue().is_empty(),
                    snooze_until: open.snooze().map(|snooze| snooze.until),
                },
            ))
        })
        .collect();
    let shown = view.focused_thread(cx).map(|thread| model.thread_key(thread));
    let provider = shown
        .and_then(|key| model.thread(key.thread()))
        .map(|open| open.provider())
        .unwrap_or(view.launch_provider);
    let update = match view.ferrite_update.state() {
        State::Ready { version, .. } => Some(format!("Restart to update to {version}").into()),
        State::HandedOver { version } => Some(format!("Install Ferrite {version}").into()),
        _ => None,
    };
    let inputs = NavInputs {
        machines: view.machine_rows(cx),
        machine_lost: view.machine_lost(cx),
        inbox,
        open_shelves: view.prefs.settings.open_shelves.clone(),
        width: view.nav_expanded_width(),
        shown,
        notice: view.nav_notice.text().cloned(),
        filtered: view
            .nav_filter
            .and_then(|project| model.registry().project(project))
            .map(|project| project.title.clone().into()),
        factory,
        menu: view.context_menu().map(|menu| menu.target),
        doors: view.doors(),
        open,
        rename: match view.rename.shown() {
            Some((RenameTarget::Thread(thread), editor)) => Some((model.thread_key(thread), editor.clone())),
            _ => None,
        },
        limits: model.account_limits(provider),
        free_disk: view.free_disk,
        update,
        state,
    };
    (inputs, projects)
}

/// The Projects with a Factory entry over the nav's cards: each one an
/// operator's Thread or a Task is in or whose automatic starts are on,
/// the chosen one's alone while the list is filtered to a Project,
/// which shows it even when empty. Any other Project's Factory is the
/// palette's (`Factory: <Project>`).
fn factory_projects(view: &CockpitView, model: &Cockpit, state: &nav::NavState) -> Vec<ProjectId> {
    let board = model.factory();
    let automatic = board
        .workflows
        .iter()
        .filter(|workflow| workflow.automation.switch == ferrite_core::factory::Switch::On)
        .map(|workflow| workflow.project);
    let mut projects: Vec<ProjectId> = state
        .open_rows()
        .chain(&state.parked)
        .filter_map(|row| view.facts.get(row.key).and_then(|facts| facts.project))
        .chain(board.tasks.iter().map(|task| task.project))
        .chain(automatic)
        .chain(view.nav_filter)
        .filter(|project| view.nav_filter.is_none_or(|wanted| wanted == *project))
        .collect();
    projects.sort();
    projects.dedup();
    projects
}

/// The nav's per-frame state (ADR 0040), from caches and O(1) reads
/// only; the Project filter narrows it and nothing else. Every open
/// Thread is in one section, by priority: snoozed, pinned, working,
/// else active. A Thread works while its turn, its background work
/// or a prompt waiting its turn runs, unless a Decision waits. Pinned
/// keeps pin order; Active puts pending Decisions first, oldest
/// request first, then the latest arrival (`nav::Arrivals`), so a
/// Thread done with its work comes back at the top; Working the latest
/// arrival first; Snoozed the soonest to wake first; Parked the park
/// order. Drafts are not rows (#29).
///
/// The one selection is the shown Thread. The empty board shows none.
fn nav_state(
    view: &CockpitView,
    model: &Cockpit,
    listed: &[(ThreadKey, nav::Section, Option<std::time::SystemTime>)],
    arrivals: &nav::Arrivals,
    cx: &App,
) -> nav::NavState {
    use nav::Section;
    let roster = model.roster();
    let shown = (!view.empty_board_shown(cx))
        .then(|| roster.focused_thread())
        .flatten()
        .map(|thread| model.thread_key(thread));
    let row = |key: ThreadKey| nav::ThreadRow {
        selected: shown == Some(key),
        ..view.thread_row(key, cx)
    };
    let pins = view.pins(cx);
    // The latest arrival first; arrivals alike keep creation order.
    let latest = |row: &nav::ThreadRow| (std::cmp::Reverse(arrivals.since(row.key)), row.key);
    let mut state = nav::NavState::default();
    let mut pinned = Vec::new();
    let mut snoozed = Vec::new();
    for &(key, section, _) in listed {
        if !view.admitted(key.thread(), cx) {
            continue;
        }
        match section {
            Section::Snoozed => {
                let until = model.thread(key.thread())
                    .and_then(|open| open.snooze())
                    .map(|snooze| snooze.until);
                snoozed.push((until, row(key)));
            }
            Section::Pinned => {
                let at = pins.iter().position(|pin| *pin == key);
                pinned.push((at, row(key)));
            }
            Section::Working => state.working.push(row(key)),
            Section::Active => state.active.push(row(key)),
        }
    }
    pinned.sort_by_key(|(at, _)| *at);
    state.pinned = pinned.into_iter().map(|(_, row)| row).collect();
    snoozed.sort_by_key(|(until, _)| *until);
    state.snoozed = snoozed.into_iter().map(|(_, row)| row).collect();
    let queue = model.needs_you();
    state.active.sort_by_key(|row| {
        (
            queue
                .iter()
                .position(|id| *id == row.key.thread())
                .unwrap_or(usize::MAX),
            latest(row),
        )
    });
    state.working.sort_by_key(latest);
    state.parked = view
        .parked_threads(cx)
        .into_iter()
        .map(|thread| row(model.thread_key(thread)))
        .collect();
    state
}

/// Every open Thread the nav lists, the Project filter's or not, with
/// the section it is in and its last activity: what `nav::Arrivals`
/// notes.
fn listed_threads(
    view: &CockpitView,
    model: &Cockpit,
    cx: &App,
) -> Vec<(ThreadKey, nav::Section, Option<std::time::SystemTime>)> {
    use nav::Section;
    #[cfg(test)]
    drawn::listed();
    let mut threads: Vec<ThreadId> = view
        .panes
        .iter()
        .filter_map(|pane| pane.thread())
        .filter(|thread| {
            model.thread(*thread).is_some() && model.operators(*thread)
        })
        .collect();
    threads.sort();
    threads.dedup();
    let pins = view.pins(cx);
    // Every listed Thread's section, the filter's or not: a Thread
    // that moves while filtered out has still moved.
    threads
        .into_iter()
        .map(|thread| {
            let key = model.thread_key(thread);
            let open = model.thread(thread);
            let section = if open.is_some_and(|open| open.snooze().is_some()) {
                Section::Snoozed
            } else if pins.contains(&key) {
                Section::Pinned
            } else if open.is_some_and(|open| {
                open.pending_decisions().is_empty()
                    && (open.needs_queue() || open.background_work() || !open.queue().is_empty())
            }) {
                Section::Working
            } else {
                Section::Active
            };
            (key, section, view.facts.last_used(key))
        })
        .collect()
}

impl CockpitView {
    pub(super) fn on_nav_event(
        &mut self,
        _: &Entity<NavView>,
        event: &NavEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.clone() {
            NavEvent::Open(key) => {
                self.open_nav_row(key, cx);
            }
            NavEvent::OpenResult(key) => {
                if self.open_nav_row(key, cx) {
                    self.nav_view.update(cx, |nav, cx| nav.clear_search(cx));
                    window.blur(cx);
                }
            }
            NavEvent::Notice(verb) => self.notice_verb(verb, cx),
            NavEvent::Revive(key) => {
                self.revive_thread(key.thread(), cx);
            }
            NavEvent::Wake(key) => self.snooze_thread(key, None, cx),
            NavEvent::Park(keys) => self.park_listed(&keys, cx),
            NavEvent::Unpin(key) => self.set_pinned(key, false, cx),
            NavEvent::Rename(key) => self.start_rename(RenameTarget::Thread(key.thread()), cx),
            NavEvent::OpenPr(key, how) => self.open_pr_as(key.thread(), how, window, cx),
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
            NavEvent::Undo(UndoAct::Snoozed(key)) => self.snooze_thread(key, None, cx),
            NavEvent::FoldMachine(index) => self.fold_machine(index, cx),
            NavEvent::GoToMachine(index) => cx.emit(GoToMachine {
                index: Some(index),
                open: None,
            }),
            NavEvent::Snooze(key, until) => {
                let refused = match self.cockpit.update(cx, |cockpit, _| cockpit.snooze_thread(key.thread(), Some(until))) {
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
