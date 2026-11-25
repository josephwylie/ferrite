//! The nav as a view of its own (ADR 0041). It owns what only the nav
//! reads: its list's motion and folds, its scroll, its keyboard and
//! cursor, its search, the cards picked or swept, the undo notice, the
//! Snooze field, and when each Thread came into its section. It reads
//! what it draws (`NavInputs`) through the `Source`s the window makes
//! over itself (`NavSources`), as often as the window says (`watch`: its
//! notify, the model's `Arrived`), and its render draws only that, so it
//! redraws only when what it draws moved. What the operator asks of the
//! window it says as a `NavEvent`, which the window answers once the
//! nav's own update has ended.

mod chrome;
mod keys;
mod list;
mod pick;
mod snooze;

#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

use ferrite_core::provider::Provider;
use ferrite_core::transcript::RateLimits;
use ferrite_core::{MachineId, ProjectKey, ThreadKey};
use ferrite_pane::composer::Composer;
use ferrite_pane::link_target::Open;
use ferrite_ui::components::{MenuButton, MenuDoor};
use ferrite_ui::motion::Reveal;
use ferrite_ui::source::Source;
use gpui::prelude::*;
use gpui::{
    div, App, Bounds, Context, Entity, EventEmitter, FocusHandle, Pixels, ScrollHandle,
    SharedString, Subscription, Window,
};

use crate::notifications::{Row as NoticeRow, Verb};
use crate::{Arrivals, NavCursor, NavState, NavTooltip, Section, Shelf};

pub use chrome::{NavUndo, UndoAct};
pub use list::{NavFace, NavKey, NavMotion};
pub use snooze::Editor as SnoozeEditor;

/// A machine the window holds and cannot reach, as the notice at the top
/// of the nav's list says it, in T3 Code's words for an environment that
/// is not there (`useSystemComposerBanners`), and what its press does.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineNotice {
    pub machine: MachineId,
    pub words: SharedString,
    /// Its press signs in, in a terminal; else it tries again now.
    pub sign_in: bool,
    /// It is reaching the machine again: T3 says so only once that has
    /// taken `RECONNECT_GRACE`.
    pub reconnecting: bool,
}

/// How long a machine is being reached again before its notice says so
/// (T3 Code's `ENVIRONMENT_RECONNECT_WARNING_GRACE_MS`).
pub const RECONNECT_GRACE: Duration = Duration::from_secs(2);

/// One open Thread the nav lists: the section it is in and its last
/// activity, which `Arrivals` notes.
pub type Listed = (ThreadKey, Section, Option<SystemTime>);

/// What the nav asks the window to draw from: the Threads it lists, and
/// when each came into its section, which orders them.
pub struct NavAsked {
    pub listed: Vec<Listed>,
    pub arrivals: Arrivals,
}

/// What the window answers: what the nav draws, and the Projects the
/// Factory door lists.
pub struct NavLook {
    pub inputs: NavInputs,
    pub factories: Vec<ProjectKey>,
}

/// What the nav reads of the window.
pub struct NavSources {
    /// Every open Thread the nav lists, on every machine, the Project
    /// filter's or not.
    pub listed: Source<Vec<Listed>>,
    /// What the nav draws, from the Threads it lists.
    pub look: Source<NavLook, NavAsked>,
    /// The model a row's Thread runs, named as its Composer names it.
    pub model_name: Source<SharedString, (ThreadKey, Provider)>,
    /// The menu button for `menu` as the nav builds it, up or not as the
    /// window last said: it records where it was laid out for that menu
    /// alone to hang from (ADR 0043), once.
    pub door: Rc<dyn Fn(NavMenu, bool) -> MenuDoor>,
}

pub struct NavView {
    sources: NavSources,
    /// The list's motion (`flip`), shared with its elements.
    pub motion: NavMotion,
    /// The next frame's list stands still rather than sliding: a shelf
    /// folding or the column sliding moves its rows already.
    pub snap: Cell<bool>,
    /// Each shelf's body as far open as it is on screen.
    shelf_folds: RefCell<Vec<(Shelf, Reveal)>>,
    /// The window's size the list was last drawn at: a resize moves rows
    /// without the list's motion.
    viewport: Cell<gpui::Size<Pixels>>,
    /// How many of the Parked shelf's rows it draws open (`Show more`).
    parked_shown: usize,
    /// The list's scroll, shared with the hand-drawn scrollbar beside it:
    /// gpui 0.2.2 paints none of its own.
    pub scroll: ScrollHandle,
    /// The list's keyboard (⌘I, ⌘⇧E, `keys`): Esc gives it back to the
    /// board.
    pub focus: FocusHandle,
    /// Where the list's keyboard stands (`keys`), while it has one.
    pub cursor: Option<NavCursor>,
    /// ⌘I or ⌘⇧E handed the list the keys, and it has not let go: only
    /// then does the window's focus rule leave them on the list.
    pub keys_held: bool,
    /// Clears the cursor when the list lets go of the keys.
    blur: Option<Subscription>,
    /// Set by a cursor move until its row has scrolled into view
    /// (`cursor_reveal`).
    reveal: Rc<Cell<bool>>,
    /// Where the cursor's row was last drawn: ⇧F10 opens its menu there.
    cursor_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The Thread whose card last scrolled into view for being shown
    /// (`shown_reveal`): a focus change scrolls the new one's.
    shown: Rc<Cell<Option<ThreadKey>>>,
    /// The search field (`chrome`), and the Thread its ↑/↓ stand on: a
    /// Thread, not a place in the results, which live updates reorder.
    pub search: Entity<Composer>,
    pub search_pick: Cell<Option<ThreadKey>>,
    /// The foot's undo notice: what the last Park or Snooze did, until it
    /// fades.
    pub undo: Option<NavUndo>,
    /// The cards ⌘- and ⇧-click picked (`pick`), in picking order.
    pub picked: Vec<ThreadKey>,
    /// A Park press held down and sweeping across cards.
    park_sweep: Option<pick::ParkSweep>,
    /// The Thread cards in the order the list last drew them: what a
    /// ⇧-click and a sweep span.
    drawn: RefCell<Vec<ThreadKey>>,
    /// The pointer is over the nav's column: its scrollbar shows.
    hovered: bool,
    /// A Thread's Custom… snooze field, over the list (`snooze`).
    pub snooze: Option<SnoozeEditor>,
    /// What the nav draws, as it last read it (`look`). Its render reads
    /// nothing else, so only a change here redraws it.
    inputs: Option<NavInputs>,
    /// When each listed Thread came into its section, which the list and
    /// the Working shelf order by. Only `look` writes it.
    arrivals: Arrivals,
    /// The Projects the Factory door lists, as `look` last read them: its
    /// menu's rows. The door draws only whether one needs the operator,
    /// so a change here alone does not redraw the nav.
    factories: Vec<ProjectKey>,
    /// Since when each machine has been reached again, for its notice's
    /// grace (`RECONNECT_GRACE`), and the look due once the first ends.
    /// Only `look` writes them.
    reconnecting: HashMap<MachineId, Instant>,
    grace: Option<gpui::Task<()>>,
    /// What the window has the nav look again on (`watch`).
    watching: Vec<Subscription>,
}

/// What the nav draws from, as it last read it.
#[derive(Clone, PartialEq)]
pub struct NavInputs {
    pub state: NavState,
    /// The notice of each machine the window cannot reach, once it has
    /// something to say.
    pub machines: Vec<MachineNotice>,
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
    /// The context menu standing, when it is on something of the nav's.
    pub menu: Option<NavMenu>,
    /// The menu up from one of the nav's buttons.
    pub door_up: Option<NavMenu>,
    pub open: HashMap<ThreadKey, OpenThread>,
    pub rename: Option<(ThreadKey, Entity<Composer>)>,
    pub limits: RateLimits,
    pub free_disk: Option<u64>,
    /// The update door's tip, while a release waits.
    pub update: Option<SharedString>,
}

/// An open Thread as its row's doors need it.
#[derive(Clone, PartialEq)]
pub struct OpenThread {
    pub queued: bool,
    pub snooze_until: Option<SystemTime>,
}

impl NavInputs {
    pub fn shelf_open(&self, shelf: Shelf) -> bool {
        self.open_shelves.contains(&shelf)
    }
}

/// What a menu the nav asks the window for is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavMenu {
    /// A Thread's row.
    Thread(ThreadKey),
    /// A row's Snooze button: the reminder choices directly.
    Snooze(ThreadKey),
    /// The Parked shelf's header.
    Parked,
    /// A row's PR chip.
    PrLink(ThreadKey),
    /// The search row's Filter button: a Project to filter by.
    Filter,
    /// The search row's Factory button: a Project's board to open.
    Factory,
}

/// Where a menu the nav asks for stands.
#[derive(Clone, Copy, Debug)]
pub enum MenuAt {
    Point(gpui::Point<Pixels>),
    /// Under a row, as ⇧F10 opens it.
    Under(Bounds<Pixels>),
}

/// What the nav asks of the window.
#[derive(Clone, Debug)]
pub enum NavEvent {
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
    Menu(NavMenu, MenuAt),
    /// A press on a menu button: its menu opens under it, or shuts.
    Door(NavMenu),
    SetShelfOpen(Shelf, bool),
    Undo(UndoAct),
    /// A machine's notice: sign in to it, in a terminal.
    SignIn(MachineId),
    /// A machine's notice: reach it again now.
    Reconnect(MachineId),
    /// The Snooze field's ⏎ on a time it read: snooze the Thread until
    /// then, and answer the field (`snooze_answered`).
    Snooze(ThreadKey, SystemTime),
    AddProject,
    NewThread,
    Settings,
    PullRequests,
    Update,
}

impl EventEmitter<NavEvent> for NavView {}

impl NavView {
    pub fn new(sources: NavSources, cx: &mut Context<Self>) -> Self {
        Self {
            sources,
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
            search: chrome::search_line(cx),
            search_pick: Default::default(),
            undo: None,
            picked: Vec::new(),
            park_sweep: None,
            drawn: Default::default(),
            hovered: false,
            snooze: None,
            inputs: None,
            arrivals: Arrivals::default(),
            factories: Vec::new(),
            reconnecting: HashMap::new(),
            grace: None,
            watching: Vec::new(),
        }
    }

    /// Look as each of `wakes` fires (the window's notify, the model's
    /// `Arrived`), and now.
    pub fn watch(&mut self, wakes: Vec<Subscription>, cx: &mut Context<Self>) {
        self.watching = wakes;
        self.look(cx);
    }

    /// The model a row's Thread runs, named as its Composer names it; the
    /// Provider's name until the window is told, since a Thread on another
    /// machine says only once it is opened and the default may not be it.
    fn row_model(&self, row: &crate::ThreadRow, provider: Provider, cx: &App) -> SharedString {
        self.sources
            .model_name
            .read(&(row.key, provider), cx)
            .unwrap_or_else(|| ferrite_ui::provider::title(provider).into())
    }

    /// Read what the nav draws from the window, note which Threads came
    /// into a section, and redraw when any of it moved. It runs as the
    /// window says (`watch`), never in a draw, so a draw never writes.
    pub fn look(&mut self, cx: &mut Context<Self>) {
        let Some(listed) = self.sources.listed.read(&(), cx) else {
            return;
        };
        self.arrivals.note(&listed, ferrite_core::clock::system_time());
        let asked = NavAsked {
            listed,
            arrivals: std::mem::take(&mut self.arrivals),
        };
        let look = self.sources.look.read(&asked, cx);
        self.arrivals = asked.arrivals;
        let Some(NavLook {
            mut inputs,
            factories,
        }) = look
        else {
            return;
        };
        self.factories = factories;
        inputs.machines = self.past_grace(std::mem::take(&mut inputs.machines), cx);
        if self.inputs.as_ref() != Some(&inputs) {
            self.inputs = Some(inputs);
            cx.notify();
        }
    }

    /// `notices` but those of a machine being reached again for less than
    /// `RECONNECT_GRACE` (T3 Code's grace), noting since when each has
    /// been; the nav looks again as the first grace ends.
    fn past_grace(&mut self, notices: Vec<MachineNotice>, cx: &mut Context<Self>) -> Vec<MachineNotice> {
        let now = cx.background_executor().now();
        let since = |machine| self.reconnecting.get(&machine).copied().unwrap_or(now);
        self.reconnecting = notices
            .iter()
            .filter(|notice| notice.reconnecting)
            .map(|notice| (notice.machine, since(notice.machine)))
            .collect();
        let due = self
            .reconnecting
            .values()
            .map(|since| *since + RECONNECT_GRACE)
            .filter(|due| *due > now)
            .min();
        self.grace = due.map(|due| {
            cx.spawn(async move |nav, cx| {
                cx.background_executor().timer(due - now).await;
                nav.update(cx, |nav, cx| nav.look(cx)).ok();
            })
        });
        notices
            .into_iter()
            .filter(|notice| {
                !notice.reconnecting || self.reconnecting[&notice.machine] + RECONNECT_GRACE <= now
            })
            .collect()
    }

    /// `look`, once the update under way has ended: what the window's
    /// own code asks as it changes something the nav draws without
    /// notifying itself (a PR or a title landed, branch labels, the
    /// column's width).
    pub fn look_again(&mut self, cx: &mut Context<Self>) {
        let nav = cx.weak_entity();
        cx.defer(move |cx| {
            nav.update(cx, |nav, cx| nav.look(cx)).ok();
        });
    }

    /// The window measured the nav's column anew as it drew.
    pub fn set_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if let Some(inputs) = &mut self.inputs {
            if inputs.width != width {
                inputs.width = width;
                cx.notify();
            }
        }
    }

    /// What the nav draws, as it last read it.
    pub fn inputs(&self) -> Option<&NavInputs> {
        self.inputs.as_ref()
    }

    /// The Projects the Factory door lists, as the nav last read them.
    pub fn factories(&self) -> &[ProjectKey] {
        &self.factories
    }

    /// The list holds the keyboard it was handed.
    pub fn holds_keys(&self, window: &Window) -> bool {
        self.keys_held && self.focus.is_focused(window)
    }

    /// The pointer came into the nav's column or left it.
    pub fn set_hovered(&mut self, inside: bool, cx: &mut Context<Self>) {
        if inside != self.hovered {
            self.hovered = inside;
            cx.notify();
        }
    }

    /// `face` as the button that opens `menu` under itself (ADR 0043):
    /// while that menu is up the button wears `CHIP_OPEN` and shows no
    /// tip, and its press asks the window (`NavEvent::Door`).
    fn door<F: MenuButton + 'static>(
        &self,
        face: F,
        menu: NavMenu,
        tip: &'static str,
        inputs: &NavInputs,
        cx: &mut Context<Self>,
    ) -> F {
        let press = cx.listener(move |_, _: &(), _, cx| cx.emit(NavEvent::Door(menu)));
        (self.sources.door)(menu, inputs.door_up == Some(menu)).dressed(
            face,
            move |face| face.nav_tooltip(tip),
            move |window, cx| press(&(), window, cx),
        )
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
