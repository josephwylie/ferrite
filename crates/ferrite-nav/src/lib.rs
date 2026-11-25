//! The left navigation column (ADR 0040, `docs/design/t3-reference/
//! sidebar.md`): the nav is the Inbox, one list of Thread cards.
//!
//! - **The list**: Pinned cards, then the Active cards in the Inbox's
//!   order (pending Decisions first, oldest request first), then, anchored
//!   over the foot whatever the list's length, the **Working**,
//!   **Snoozed** and **Parked** shelves, each a 32px header with its count
//!   and chevron. A shelf starts folded and remembers its fold
//!   (`Settings::open_shelves`); folded, it still shows the shown Thread's
//!   row. Working holds cards; Snoozed and Parked hold slim rows
//!   (`list::slim_row`), Parked ten at first and then `Show 25 more`.
//! - **The foot**: Settings, Pull requests and Usage, the undo notice
//!   after a Park or a Snooze, and at the right free disk and the update
//!   door.
//!
//! Rows move as T3's do (`flip::ListMotion`): one that moves slides from
//! its old place over 150ms, one that enters drops in, one that leaves is
//! drawn as a fading ghost. A shelf's body eases open and shut
//! (`motion::fold_body`), the list's own motion standing still for it.
//!
//! Drawing only, like `pane.rs`: the cockpit assembles a `NavState` per
//! frame from O(1) reads plus its project/branch/parked caches —
//! `Store::load` and `Instruments::of` are banned here, which is what keeps
//! the 24-Pane wall smooth with the nav open. Click wiring stays in
//! `cockpit.rs` and `cockpit/nav_list.rs`, the same split `pane_cell` uses.
//!
//! Every colour and metric is a `theme` token; this file holds no literal
//! of its own.
//!
//! Out of the Cockpit, in a crate of its own: the nav's elements, the
//! Inbox's rows and toasts (`notifications`), and the views beside the nav
//! (the empty board). Each view owns its own state, reads what it draws of
//! the window through a `Source` (`ferrite_ui::source`) and says what the
//! operator asks as an event (ADR 0041). Nothing here names the Cockpit, so
//! an edit to one builds and tests without it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Instant, SystemTime};

use ferrite_core::provider::Provider;
use ferrite_core::ThreadKey;

use gpui::prelude::*;
use gpui::{
    div, px, rgb, rgba, AnyElement, App, Div, FontWeight, Hsla, ScrollHandle, SharedString,
    Stateful,
};

pub use ferrite_pane::mark::{mark, NavTail, RowStatus};
use ferrite_ui::components;
use ferrite_ui::icons::{self, icon};
use ferrite_ui::motion;
use ferrite_ui::theme::*;

mod card;
mod fade;
pub mod chrome;
pub mod empty_board;
pub mod flip;
pub mod list;
pub mod notifications;
pub use card::{card_hover, card_key};
#[cfg(any(test, feature = "test-support"))]
pub use card::status_label;
pub use card::{
    card_age, ghost_card, park_button, pin_glyph, pr_chip, snooze_button, strip,
    strip_w, thread_card, working_for,
    CardDoors, NavPr,
};

pub use ferrite_ui::theme::{NAV_COLUMN_W as COLUMN_WIDTH, NAV_WIDTH as WIDTH};

gpui::actions!(
    nav,
    [
        CloseView,
        Focus,
        SelectNext,
        SelectPrevious,
        Open,
        Fold,
        Unfold,
        Rename,
        SelectFirst,
        SelectLast,
        OpenMenu,
        Undo,
        SearchNext,
        SearchPrevious
    ]
);

/// The nav's tooltips (T3's `ui/tooltip.tsx`): `menu::nav_tooltip`'s face,
/// open after `NAV_TOOLTIP_DELAY_MS`, the next within
/// `NAV_TOOLTIP_HANDOFF_MS` at once.
pub trait NavTooltip: gpui::InteractiveElement + Sized {
    fn nav_tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.interactivity().tooltip(ferrite_ui::menu::nav_tooltip(text));
        self.nav_timing()
    }

    /// `nav_tooltip` naming a bound action's key.
    fn nav_action_tooltip(mut self, label: impl Into<SharedString>, action: &str) -> Self {
        self.interactivity()
            .tooltip(ferrite_ui::menu::nav_action_tooltip(label, action));
        self.nav_timing()
    }

    fn nav_timing(mut self) -> Self {
        let tip = self.interactivity();
        tip.tooltip_show_delay(std::time::Duration::from_millis(NAV_TOOLTIP_DELAY_MS));
        tip.tooltip_handoff(std::time::Duration::from_millis(NAV_TOOLTIP_HANDOFF_MS));
        self
    }
}

impl<E: gpui::InteractiveElement> NavTooltip for E {}

/// A row the nav's keyboard can stand on (`cockpit/nav_keys.rs`): a
/// shelf's header, or a Thread's card or slim row, wherever it is listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavItem {
    Shelf(Shelf),
    Thread(ThreadKey),
}

/// The keyboard's place in the nav: its row, and the blend key that row's
/// hover face reads, so the cursor wears the pointer's face.
#[derive(Clone, Debug, PartialEq)]
pub struct NavCursor {
    pub item: NavItem,
    pub key: SharedString,
}

impl NavCursor {
    pub fn shelf(shelf: Shelf) -> Self {
        NavCursor {
            item: NavItem::Shelf(shelf),
            key: list::shelf_key(shelf),
        }
    }

    /// The key mirrors the row's own (`thread_card`, `list::slim_row`).
    pub fn thread(row: &ThreadRow) -> Self {
        NavCursor {
            item: NavItem::Thread(row.key),
            key: card::card_key(row.tag()),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
const INNER_W: f32 = WIDTH - 2.0 * NAV_COL_PAD;

/// A shelf at the foot of the list (ADR 0040), in drawing order.
pub use ferrite_core::settings::NavShelf as Shelf;

/// The shelves, in the order the list draws them.
pub const SHELVES: [Shelf; 3] = [Shelf::Working, Shelf::Snoozed, Shelf::Parked];

/// What the nav draws this frame (ADR 0040): one list, section by section
/// in drawing order. A Thread is in one section, by priority: snoozed,
/// parked, pinned, working, active. Nothing here is a store read — the
/// cockpit assembles it from O(1) reads plus its caches.
#[derive(Clone, Default, PartialEq)]
pub struct NavState {
    /// Pinned cards (`Settings::pinned_threads`), in pin order.
    pub pinned: Vec<ThreadRow>,
    /// The Inbox: open Threads awaiting the operator, pending Decisions
    /// first, oldest request first, then the latest to arrive.
    pub active: Vec<ThreadRow>,
    /// Working Threads, the latest to start work first.
    pub working: Vec<ThreadRow>,
    /// Snoozed Threads, the soonest to wake first.
    pub snoozed: Vec<ThreadRow>,
    /// Parked Threads, every machine's, in the park order.
    pub parked: Vec<ThreadRow>,
}

impl NavState {
    /// A shelf's rows, folded or not.
    pub fn shelf(&self, shelf: Shelf) -> &[ThreadRow] {
        match shelf {
            Shelf::Working => &self.working,
            Shelf::Snoozed => &self.snoozed,
            Shelf::Parked => &self.parked,
        }
    }

    /// Every open Thread's row, every machine's.
    pub fn open_rows(&self) -> impl Iterator<Item = &ThreadRow> {
        self.pinned
            .iter()
            .chain(&self.active)
            .chain(&self.working)
            .chain(&self.snoozed)
    }
}

/// The section an open Thread is listed in, for `Arrivals`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Pinned,
    Active,
    Working,
    Snoozed,
}

/// When each open Thread came into the section it is listed in. The list
/// and the Working shelf put the latest arrival first, so a Thread comes
/// in at the top (truly done, it leaves Working for the top of the list)
/// and nothing but changing section moves it. A Thread first seen where it
/// is arrived at its last activity then, so a launch keeps the order the
/// work happened in.
#[derive(Default)]
pub struct Arrivals {
    /// `None` until a first-seen Thread's last activity is known.
    at: HashMap<ThreadKey, (Section, Option<SystemTime>)>,
}

impl Arrivals {
    /// Note where each Thread is listed now, with its last activity: one
    /// that moved arrived `now`. A Thread no longer listed is forgotten,
    /// so one that comes back (revived, reopened) is first seen again.
    /// True when any of it changed.
    pub fn note(
        &mut self,
        listed: &[(ThreadKey, Section, Option<SystemTime>)],
        now: SystemTime,
    ) -> bool {
        let mut at = HashMap::with_capacity(listed.len());
        for (thread, section, last_activity) in listed {
            let since = match self.at.get(thread) {
                Some((was, since)) if was == section => since.or(*last_activity),
                Some(_) => Some(now),
                None => *last_activity,
            };
            at.insert(*thread, (*section, since));
        }
        let changed = at != self.at;
        self.at = at;
        changed
    }

    /// When `thread` came into its section.
    pub fn since(&self, thread: ThreadKey) -> Option<SystemTime> {
        self.at.get(&thread).and_then(|(_, since)| *since)
    }

    /// The list's and the Working shelf's order: the latest arrival first,
    /// whatever machine it is on; arrivals alike keep creation order.
    pub fn latest_first(&self, thread: ThreadKey) -> (std::cmp::Reverse<Option<SystemTime>>, ThreadKey) {
        (std::cmp::Reverse(self.since(thread)), thread)
    }
}

/// What a set of rows rolls up to: the most urgent of them. `None` for
/// no rows.
pub fn roll_up<'a>(rows: impl IntoIterator<Item = &'a ThreadRow>) -> Option<RowStatus> {
    rows.into_iter().map(|row| row.status).max()
}

/// One Thread's row — its card (`card::thread_card`), identical whether
/// it is an open Thread or a parked one; only the
/// container differs. Every field is a cache read (`CockpitView::thread_row`).
#[derive(Clone, PartialEq)]
pub struct ThreadRow {
    pub key: ThreadKey,
    pub name: SharedString,
    /// What the Thread is doing right now.
    pub status: RowStatus,
    /// The Project's name, from the facts cache: the badge's monogram and
    /// hue read it.
    pub project: Option<SharedString>,
    /// The board shows this Thread: the row carries the selection.
    pub selected: bool,
    /// Its state's word (C10), which the status label says.
    pub tail: NavTail,
    /// The branch its checkout is on.
    pub branch: Option<SharedString>,
    /// Where it works when that is a worktree (`~/…`).
    pub worktree: Option<SharedString>,
    /// The branch's pull request.
    pub pr: Option<NavPr>,
    /// The Provider running it (an open Thread's, live; a parked one's,
    /// from its log).
    pub provider: Option<Provider>,
    /// Its last activity, which a quiet card's age reads.
    pub activity_at: Option<SystemTime>,
    /// When the running turn started; `None` while not working.
    pub working_since: Option<Instant>,
    /// Pinned (`Settings::pinned_threads`).
    pub pinned: bool,
    /// The titler is writing its title.
    pub retitling: bool,
    /// In the operator's ⌘/⇧-click selection, or swept by a Park press:
    /// it wears `NAV_CARD_SELECTED`, and Park takes it with the rest.
    pub picked: bool,
    /// The machine it lives on, when that is not this computer.
    pub elsewhere: Option<Elsewhere>,
}

impl ThreadRow {
    /// What its ids and selectors end with.
    pub fn tag(&self) -> RowTag {
        RowTag::of(self.key, self.elsewhere.is_some())
    }

    /// Its machine is not there: the row stays, dimmed.
    pub fn lost(&self) -> bool {
        self.elsewhere.as_ref().is_some_and(|elsewhere| elsewhere.lost)
    }
}

/// Another machine's Thread, as its row says so (ADR 0045, T3 Code's
/// remote row): the server glyph at the end of its meta line and the line
/// its hover card gives the machine.
#[derive(Clone, Debug, PartialEq)]
pub struct Elsewhere {
    /// The machine's name.
    pub machine: SharedString,
    /// The window cannot reach it now.
    pub lost: bool,
}

/// What a Thread's row's ids and selectors end with: its Thread's number,
/// after its machine's for another machine's Thread — Thread numbers are
/// per Server. This computer's rows keep the bare number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RowTag {
    thread: ThreadKey,
    elsewhere: bool,
}

impl RowTag {
    pub fn of(thread: ThreadKey, elsewhere: bool) -> Self {
        Self { thread, elsewhere }
    }

    /// `name-tag`, as a key or a selector.
    pub fn key(self, name: &str) -> SharedString {
        format!("{name}-{self}").into()
    }

    /// `name-tag`, as an element's id.
    pub fn id(self, name: &str) -> gpui::ElementId {
        self.key(name).into()
    }
}

impl std::fmt::Display for RowTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let thread = self.thread.thread().get();
        if self.elsewhere {
            write!(f, "{}-{thread}", self.thread.machine().get())
        } else {
            write!(f, "{thread}")
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ThreadRow {
    /// A row with nothing known but its Thread, name and state.
    pub fn bare(key: ThreadKey, name: impl Into<SharedString>, status: RowStatus) -> Self {
        ThreadRow {
            key,
            name: name.into(),
            status,
            project: None,
            selected: false,
            tail: NavTail::None,
            branch: None,
            worktree: None,
            pr: None,
            provider: None,
            activity_at: None,
            working_since: None,
            pinned: false,
            retitling: false,
            picked: false,
            elsewhere: None,
        }
    }
}

/// The width a card's lines have, from its content inset to its right
/// padding.
#[cfg(test)]
const TEXT_W: f32 = INNER_W - 2.0 * NAV_ROW_PAD_X;

// ------------------------------------------------------------- measuring

/// The faces' own widths, a character at a time as each lays out alone:
/// what gpui's truncation sums (`LineWrapper::width_for_char`), and what a
/// truncating box is pinned against. Never the glyph's bare advance: the
/// system face lays out wider than its advances at the UI's sizes (its
/// optical size and tracking), so a box sized from them cut a name that
/// fits (`ferri…` for `ferrite`). A sum without kerning errs wide, which
/// keeps a name whole.
///
/// Each character's width is laid out once per face and size and kept
/// (`ADVANCES`): a CoreText line per character on every nav build was most
/// of the nav's draw time.
#[derive(Clone, Copy)]
pub enum Measure<'a> {
    Text(&'a App, f32),
    /// A fixed advance per character, for unit tests.
    #[cfg(any(test, feature = "test-support"))]
    Cells(f32),
}

impl Measure<'_> {
    pub fn inner_width(&self) -> f32 {
        match self {
            Self::Text(_, width) => *width - 2.0 * NAV_COL_PAD,
            #[cfg(any(test, feature = "test-support"))]
            Self::Cells(_) => INNER_W,
        }
    }

    /// `text` in the platform UI font at `size` and `weight`.
    pub fn ui(&self, text: &str, size: f32, weight: FontWeight) -> f32 {
        self.width(FONT_UI, text, size, weight)
    }

    fn width(&self, family: &'static str, text: &str, size: f32, weight: FontWeight) -> f32 {
        match self {
            Measure::Text(cx, _) => {
                let system = cx.text_system();
                let font = gpui::Font {
                    weight,
                    ..gpui::font(family)
                };
                let id = system.resolve_font(&font);
                ADVANCES.with_borrow_mut(|advances| {
                    text.chars()
                        .map(|ch| {
                            *advances.entry((id, size.to_bits(), ch)).or_insert_with(|| {
                                #[cfg(any(test, feature = "test-support"))]
                                testing::SHAPED.with(|shaped| shaped.set(shaped.get() + 1));
                                f32::from(system.layout_width(id, px(size), ch))
                            })
                        })
                        .sum()
                })
            }
            #[cfg(any(test, feature = "test-support"))]
            Measure::Cells(advance) => text.chars().count() as f32 * advance,
        }
    }
}

thread_local! {
    /// `Measure::Text`'s width of each character, by face, size and character.
    static ADVANCES: RefCell<HashMap<(gpui::FontId, u32, char), f32>> = RefCell::default();
}

/// A line pinned to `width` that fades out at its right edge, as Claude's
/// sidebar titles do, rather than ending in "…": the line lays out whole,
/// the box clips it, and its leaf (`fade::title_line`) fades its own ink
/// where it is cut. A rename field in the box has no fade over its caret.
///
/// A pinned line needs a **definite** width on its very first measure.
/// gpui caches a nowrap line's first measure permanently, and taffy only
/// hands a text leaf a definite width when the leaf's flex container is a
/// **column** whose own available width is definite — which taffy derives
/// from the child's own min/max width. Hence: flex **column**, with min and
/// max width pinned to the box. Its ink may be an alpha one (a receded
/// card's title).
fn fitted_in(
    width: f32,
    size: f32,
    ink: Hsla,
    weight: FontWeight,
    text: impl IntoElement,
) -> Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .flex_shrink(1.)
        .min_w_0()
        .w(px(width.max(0.0)))
        .child(
            div().h(px(NAV_LINE)).overflow_hidden().child(
                div()
                    .flex()
                    .flex_col()
                    .min_w(px(width.max(0.0)))
                    .max_w(px(width.max(0.0)))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .h(px(NAV_LINE))
                    .text_size(px(size))
                    .font_weight(weight)
                    .line_height(px(NAV_LINE))
                    .text_color(ink)
                    .child(text),
            ),
        )
}

// ------------------------------------------------------------- the column

pub fn column(ground: Paint) -> Div {
    div()
        .debug_selector(|| "nav-column".into())
        .relative()
        .flex_shrink_0()
        .h_full()
        .w(px(COLUMN_WIDTH))
        .child(
            div()
                .absolute()
                .top(px(WIN_CHROME_H))
                .bottom_0()
                .left_0()
                .right_0()
                .bg(ground),
        )
}

pub const CARD_TOP: f32 = NAV_HEADER_H + NAV_INSET_Y;

pub fn card(content: impl IntoElement) -> Div {
    div()
        .debug_selector(|| "nav-card".into())
        .absolute()
        .top(px(CARD_TOP))
        .bottom(px(NAV_INSET_Y))
        .left(px(NAV_INSET))
        .w(px(WIDTH))
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(NAV_R))
        .bg(NAV_BACKGROUND)
        .font_family(FONT_UI)
        .text_size(px(FS_NAV))
        .line_height(px(NAV_LINE))
        .text_color(rgb(INK_STRONG))
        .child(list::grain(NAV_R))
        .child(content)
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(NAV_R))
                .border_r_1()
                .border_color(rgba(NAV_BORDER)),
        )
}

pub fn content() -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(WIDTH))
        .p(px(NAV_COL_PAD))
}

pub fn head(arrows: impl IntoElement) -> Div {
    div()
        .debug_selector(|| "nav-head".into())
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .h(px(ICON_BUTTON))
        .gap(px(SPACE_1))
        .child(div().debug_selector(|| "nav-brand".into()).text_size(px(FS_NAV_TITLE)).font_weight(W_LABEL).child("Ferrite"))
        .child(arrows)
}

fn icon_ink(glyph: &'static str, size: f32, t: f32) -> gpui::Svg {
    icon(glyph, size, INK_STRONG).text_color(motion::mix(
        rgba(NAV_ICON_INK).into(),
        rgb(INK_STRONG).into(),
        t,
    ))
}

const EDGE_TAB_KEY: &str = "nav-edge-tab";

pub fn edge_tab(open: bool) -> Stateful<Div> {
    let t = motion::hover_t(&SharedString::from(EDGE_TAB_KEY));
    let r = px(NAV_EDGE_TAB_R);
    div()
        .id(EDGE_TAB_KEY)
        .debug_selector(|| EDGE_TAB_KEY.into())
        .flex()
        .items_center()
        .justify_center()
        .w(px(NAV_EDGE_TAB_W))
        .h(px(NAV_EDGE_TAB_H))
        .rounded_tr(r)
        .rounded_br(r)
        .bg(NAV_BACKGROUND)
        .border_t_1()
        .border_r_1()
        .border_b_1()
        .border_color(rgba(NAV_BORDER))
        .occlude()
        .on_hover(motion::hover_snap(EDGE_TAB_KEY.into()))
        .child(
            icon_ink(icons::CHEVRON_RIGHT, NAV_EDGE_TAB_GLYPH, t)
                .when(open, |glyph| {
                    glyph.with_transformation(gpui::Transformation::rotate(gpui::radians(
                        std::f32::consts::PI,
                    )))
                }),
        )
        .tooltip(ferrite_ui::menu::action_tooltip(
            "Toggle sidebar",
            "cockpit::ToggleNav",
        ))
}

/// A chord as it is drawn, its key a capital: `cmd-k` → `⌘K`.
fn chord_glyphs(keys: &str) -> String {
    components::chord_text(keys).to_uppercase()
}

/// Is the row blended under the pointer, so its hover action stands?
fn hovered(key: &SharedString) -> bool {
    motion::hover_t(key) > 0.0
}

// ------------------------------------------------------------- the list

/// The scrolling list under the head: the only thing in the column that
/// scrolls. Appending rows never moves the view (T3's
/// `overflow-anchor: none`): gpui anchors nothing.
pub fn list_scroll(scroll: &ScrollHandle) -> Stateful<Div> {
    div()
        .id(("nav-list", 0usize))
        .debug_selector(|| "nav-list".into())
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(scroll)
}

/// The list's scrollbar (`NAV_SCROLLBAR_GUTTER`): the overlay thumb in
/// the column's right padding, beside the rows rather than over them, so
/// it never covers a count or cuts a selected row's fill. It shows while
/// the tree scrolls and while the pointer is over the nav (`revealed`),
/// then lingers and fades. A sibling of the tree, laid after it.
pub fn scrollbar(scroll: &ScrollHandle, revealed: bool) -> Div {
    components::scrollbar_layer()
        .debug_selector(|| "nav-scrollbar".into())
        .right(px(-NAV_COL_PAD))
        .child(
            components::scrollbar_bar("nav-scrollbar", scroll)
                .gutter(px(NAV_SCROLLBAR_GUTTER), px(NAV_SCROLLBAR_INSET))
                .revealed(revealed),
        )
}

/// The list's edge fades, each drawn while rows run past its edge. Laid
/// after the scroller, so each reads the scroll this frame's layout left.
pub fn edge_fades(scroll: &ScrollHandle) -> [AnyElement; 2] {
    let fill = NAV_BACKGROUND.hsla();
    let top = div()
        .debug_selector(|| "nav-fade-top".into())
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .child(components::edge_fade_over(fill, false, NAV_EDGE_FADE));
    let foot = div()
        .debug_selector(|| "nav-fade-foot".into())
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
        .child(components::edge_fade_over(fill, true, NAV_EDGE_FADE));
    [
        // As `f32`: the scroller clamps a list that fits to `-0`, which
        // `Pixels`' total order puts below zero.
        fade::past_edge(scroll, |scroll| f32::from(scroll.offset().y) < 0., top)
            .into_any_element(),
        fade::past_edge(
            scroll,
            |scroll| scroll.max_offset().y + scroll.offset().y > px(0.),
            foot,
        )
        .into_any_element(),
    ]
}

/// The Thread card at root — see `card::thread_card`.
#[cfg(any(test, feature = "test-support"))]
pub fn thread_row(row: &ThreadRow) -> Stateful<Div> {
    thread_card(
        row,
        fade::title_line(row.name.clone()),
        CardDoors::default(),
        false,
        &Measure::Cells(7.0),
    )
}

/// A refusal from the last change (a Park's, a Snooze's), at the top of
/// the list, until the next change succeeds: a `BLOCKED` dot and the refusal, dim, wrapping.
/// Colour on the dot, never the row.
pub fn notice(text: SharedString) -> Div {
    notice_row(text, NAV_LINE)
        .debug_selector(|| "nav-notice".into())
        .py(px(SPACE_1))
        .text_size(px(FS_META))
        .text_color(rgb(INK_SUBTLE))
}

/// A machine the window holds and cannot reach, at the top of the list in
/// T3 Code's words for it (`machine_notices`), on the rows' left edge in
/// the sidebar's notice type (T3's `SidebarThreadUndoNotice`: 11px on a
/// 24px line in the muted ink), strong under the pointer as T3's inline
/// button is. A button the cockpit wires: it signs in or tries again.
pub fn machine_notice(
    machine: u32,
    text: SharedString,
    cx: &App,
) -> gpui::component::button::Button {
    let id: gpui::ElementId = ("nav-machine-notice", machine as usize).into();
    let ink = motion::mix(
        rgb(INK_SUBTLE).into(),
        rgb(INK_STRONG).into(),
        motion::hover_t(&ferrite_ui::pointer::hover_key(&id)),
    );
    let clear: Hsla = rgba(TRANSPARENT).into();
    components::faded_button(id, clear, clear, clear, ink, cx)
        .debug_selector(move || format!("nav-machine-notice-{machine}"))
        .w_full()
        .h_auto()
        .p_0()
        .child(
            notice_row(text, NAV_NOTICE_LH)
                .w_full()
                .text_size(px(FS_NAV_NOTICE))
                .text_color(ink),
        )
}

/// A notice's row on lines `line` tall, on a card's grid: a `BLOCKED` dot
/// on the first line at the edge the Project badge starts at, and its words
/// where the Project name starts.
fn notice_row(text: SharedString, line: f32) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_start()
        .gap(px(NAV_CARD_GAP))
        .px(px(NAV_ROW_PAD_X))
        .line_height(px(line))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .w(px(NAV_BADGE))
                .h(px(line))
                .child(
                    div()
                        .debug_selector(|| "nav-notice-dot".into())
                        .size(px(NAV_DOT))
                        .rounded_full()
                        .bg(rgb(BLOCKED)),
                ),
        )
        .child(
            div()
                .debug_selector(|| "nav-notice-words".into())
                .flex_1()
                .min_w_0()
                .whitespace_normal()
                .child(text),
        )
}

// ------------------------------------------------------------- rename and drag

/// A Thread title that can be renamed: the row's own title text, and
/// nothing else. It wears **no** hover wash: the double click is the
/// affordance; the row is the control.
pub fn rename_target_thread(tag: RowTag, title: SharedString) -> Stateful<Div> {
    let key = tag.key("rename-thread");
    div()
        .id(tag.id("rename-thread"))
        .debug_selector(move || key.to_string())
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .child(fade::title_line(title))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_pane::pane::WallState;

    /// Rule 10 over the nav's render code (`ferrite_ui::face`).
    #[test]
    fn render_code_draws_only_covered_glyphs() {
        ferrite_ui::face::assert_render_code_draws_only_covered_glyphs(&[
            ("lib.rs", include_str!("lib.rs")),
            ("notifications.rs", include_str!("notifications.rs")),
        ]);
    }

    fn thread() -> ThreadRow {
        selected_thread(false)
    }

    fn selected_thread(selected: bool) -> ThreadRow {
        ThreadRow {
            project: Some("ferrite".into()),
            selected,
            tail: NavTail::Age("2h".into()),
            ..ThreadRow::bare(ThreadKey::fixture(0, 8), "thread-08", RowStatus::Idle)
        }
    }

    /// The row grammar (§5.1): a Thread is a rounded card, its content
    /// 10px in, in the column's 256px.
    #[test]
    fn nav_rows_are_rounded_cards() {
        let mut row = thread_row(&thread());
        let style = row.style();
        assert_eq!(
            style.corner_radii.top_left,
            Some(px(R_CONTROL).into()),
            "radius 6"
        );
        assert_eq!(style.size.height, Some(px(NAV_CARD_H_SHORT).into()));
        assert_eq!(style.margin.top, Some(px(2.0).into()), "2px item padding");
        assert_eq!(style.margin.bottom, Some(px(2.0).into()));
        assert_eq!(style.padding.left, Some(px(NAV_ROW_PAD_X).into()));
        assert_eq!(style.padding.right, Some(px(NAV_ROW_PAD_X).into()));
        assert_eq!(WIDTH, 256.0, "Claude's 256px sidebar");
        assert_eq!(INNER_W, 240.0, "padding 0 8 8");
        assert_eq!(TEXT_W, 220.0);
    }

    /// N-5, the selection rule: one selection, on the row of what the board
    /// shows — a fill, never a coloured outline. A card at rest is bare.
    #[test]
    fn one_selection_marks_what_the_board_shows() {
        let fill = |mut drawn: Stateful<Div>| drawn.style().background.clone();
        let active = Some(gpui::Hsla::from(rgba(0xffffff1c)).into());
        assert_eq!(fill(thread_row(&selected_thread(true))), active, "white 11%");
        assert_eq!(fill(thread_row(&thread())), None, "no fill at rest");
        assert_eq!(
            (NAV_CARD_HOVER, NAV_CARD_ACTIVE),
            (0xffffff14, 0xffffff1c),
            "hover 8%, active 11%"
        );
    }

    /// Busy cards recede so what needs the operator reads brightest: a
    /// working card's title is 400 at 75% ink, a Done, needs-you or
    /// failed card's 500 (a failure at 95%), and the shown Thread never
    /// recedes.
    #[test]
    fn busy_cards_recede_and_what_needs_you_reads_brightest() {
        let row = |status| ThreadRow { status, ..thread() };
        let receded = (rgba(NAV_CARD_TITLE_RECEDED).into(), W_BODY);
        for status in [
            RowStatus::Working,
            RowStatus::Failing,
            RowStatus::Background,
            RowStatus::Idle,
            RowStatus::Parked,
        ] {
            assert!(row(status).receded(), "{status:?}");
            assert_eq!(card::title_face(&row(status), 0.0), receded, "{status:?}");
        }
        for status in [RowStatus::Done, RowStatus::NeedsYou] {
            assert!(!row(status).receded(), "{status:?}");
            assert_eq!(
                card::title_face(&row(status), 0.0),
                (rgb(INK_STRONG).into(), W_LABEL),
                "{status:?}"
            );
        }
        assert!(!row(RowStatus::Failed).receded());
        assert_eq!(
            card::title_face(&row(RowStatus::Failed), 0.0),
            (rgba(0xefefeff2).into(), W_LABEL),
            "a failure at 95%"
        );
        let shown = ThreadRow {
            status: RowStatus::Working,
            ..selected_thread(true)
        };
        assert!(!shown.receded(), "the shown Thread never recedes");
        let mut picked = row(RowStatus::Working);
        picked.picked = true;
        assert!(!picked.receded(), "a selected working Thread never recedes");
        assert_eq!(card::title_face(&picked, 0.0), (rgb(INK_STRONG).into(), W_LABEL));
        assert_eq!(
            card::title_face(&row(RowStatus::Working), 1.0),
            (rgb(INK_STRONG).into(), W_BODY),
            "under the pointer a receded title comes back to full ink"
        );
        assert_eq!(NAV_CARD_TITLE_RECEDED, 0xefefefbf, "75%");
    }

    /// The status slot (sidebar.md §4): a glyph, a word and a clock in
    /// the state's ink; a quiet card its age.
    #[test]
    fn the_status_slot_says_the_state_in_its_ink() {
        let now = std::time::Instant::now();
        let wall = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let say = |status, tail| {
            let row = ThreadRow {
                status,
                tail,
                working_since: Some(now - std::time::Duration::from_secs(12 * 60 + 3)),
                activity_at: Some(wall - std::time::Duration::from_secs(3 * 3600)),
                ..thread()
            };
            let label = card::status_label(&row, now, wall).expect("a label");
            (label.glyph, label.ink, label.text.to_string())
        };
        use ferrite_ui::icons;
        assert_eq!(
            say(RowStatus::Working, NavTail::Working("12m03s".into())),
            (Some(icons::CIRCLE_DASHED), NAV_WORKING_INK, "Working 12m".into())
        );
        assert_eq!(
            say(RowStatus::NeedsYou, NavTail::Waiting(words::APPROVAL)),
            (Some(icons::SHIELD_QUESTION), ATTENTION, "Approval".into())
        );
        assert_eq!(
            say(RowStatus::NeedsYou, NavTail::Waiting(words::PLAN)).2,
            "Approval",
            "a plan reads as an approval"
        );
        assert_eq!(
            say(RowStatus::NeedsYou, NavTail::Waiting(words::QUESTION)),
            (Some(icons::MESSAGE_CIRCLE_QUESTION), NAV_INPUT_INK, "Input".into())
        );
        assert_eq!(
            say(RowStatus::Failed, NavTail::Failed),
            (Some(icons::CIRCLE_ALERT), BLOCKED, "Failed".into())
        );
        assert_eq!(
            say(RowStatus::Failing, NavTail::Failing(Some(3))).2,
            "Failing 3"
        );
        assert_eq!(
            say(RowStatus::Done, NavTail::Done),
            (Some(icons::CHECK_CIRCLE), NAV_DONE_INK, "Done".into())
        );
        assert_eq!(
            say(RowStatus::Idle, NavTail::Age("3h".into())),
            (None, INK_SECONDARY_LABEL, "3h".into())
        );
        assert_eq!(
            (NAV_WORKING_INK, NAV_DONE_INK),
            (0x51a2ff, 0x00d492),
            "working is blue, done green"
        );
    }

    /// A card is a fixed 78px box (8 + 20 + 4 + 20 + 2 + 16 + 8) with a
    /// checkout line, and hugs its two lines (60) without one.
    #[test]
    fn a_card_is_seventy_eight_with_its_checkout_and_hugs_without() {
        let height = |mut drawn: Stateful<Div>| drawn.style().size.height;
        let branched = ThreadRow {
            branch: Some("t3/improve-sidebar-architecture".into()),
            ..thread()
        };
        assert_eq!(height(thread_row(&branched)), Some(px(78.0).into()));
        assert_eq!(height(thread_row(&thread())), Some(px(60.0).into()));
        assert_eq!(
            (
                NAV_CARD_PAD_Y,
                NAV_CARD_LINE1_H,
                NAV_CARD_LINE2_GAP,
                NAV_CARD_LINE2_H,
                NAV_CARD_LINE3_GAP,
                NAV_CARD_LINE3_H
            ),
            (8.0, 20.0, 4.0, 20.0, 2.0, 16.0)
        );
        assert_eq!(
            NAV_CARD_H + 2.0 * NAV_CARD_ITEM_PAD_Y + 1.0,
            83.0,
            "cards pitch at 83 with the list's 1px gap"
        );
    }

    /// Interactive rows use the reference sidebar pointer.
    #[test]
    fn every_nav_row_uses_the_pointer() {
        let cursor = |mut drawn: Stateful<Div>| drawn.style().mouse_cursor;
        assert_eq!(cursor(thread_row(&thread())), Some(gpui::CursorStyle::PointingHand));
    }

    /// The word says one thing, in priority order, and the Pane's head slot
    /// decides it.
    #[test]
    fn the_word_is_one_fact_in_priority_order() {
        use ferrite_pane::pane::HeadSlot;
        let age = || SharedString::from("40m");
        fn of(slot: Option<&HeadSlot>) -> NavTail {
            NavTail::of(RowStatus::Idle, slot, "40m".into())
        }
        assert_eq!(
            of(Some(&HeadSlot::NeedsYou(words::PLAN))),
            NavTail::Waiting(words::PLAN)
        );
        assert_eq!(
            of(Some(&HeadSlot::Failing(Some(3)))),
            NavTail::Failing(Some(3))
        );
        assert_eq!(of(Some(&HeadSlot::Failed)), NavTail::Failed);
        assert_eq!(
            of(Some(&HeadSlot::Working("1m04s".into()))),
            NavTail::Working("1m04s".into())
        );
        assert_eq!(of(Some(&HeadSlot::Working(String::new()))), NavTail::None);
        assert_eq!(of(Some(&HeadSlot::Done)), NavTail::Done);
        assert_eq!(
            NavTail::of(RowStatus::Background, None, age()),
            NavTail::Background
        );
        for quiet in [None, Some(&HeadSlot::Interrupted), Some(&HeadSlot::Parked)] {
            assert_eq!(of(quiet), NavTail::Age(age()));
        }
    }

    /// The nav never says `now` (C10).
    #[test]
    fn the_nav_never_says_now() {
        let at = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10_000);
        for secs in [0, 1, 30, 59] {
            let age = ferrite_ui::age::since_label(at, at + std::time::Duration::from_secs(secs));
            let tail = NavTail::of(RowStatus::Idle, None, age);
            assert_eq!(tail.text(), None, "{secs}s says nothing");
        }
    }

    /// The marks (§6): a spinner for work, a solid dot for needs-you and
    /// a failure, a ring for background work, icons at rest.
    #[test]
    fn status_marks_say_state_and_green_only_means_live() {
        assert_eq!(RowStatus::Working.ink(), RUNNING);
        assert_eq!(RowStatus::Background.ink(), RUNNING);
        assert_eq!(RowStatus::Failing.ink(), BLOCKED);
        assert_eq!(RowStatus::Failed.ink(), BLOCKED);
        assert_eq!(RowStatus::NeedsYou.ink(), ATTENTION);
        assert_eq!(RowStatus::Done.ink(), IDLE);
        assert_eq!(RowStatus::Idle.ink(), IDLE);
        assert_eq!(RowStatus::Parked.ink(), IDLE);
    }

    /// Herdr's four states over Ferrite's own, and a roll-up's order.
    #[test]
    fn a_row_is_one_of_four_states_and_a_heading_rolls_its_rows_up() {
        use WallState::*;
        assert_eq!(
            RowStatus::of(Done, true, true),
            RowStatus::Background,
            "no finish is done while work runs behind Main"
        );
        assert_eq!(RowStatus::of(Idle, true, false), RowStatus::Done);
        assert_eq!(RowStatus::of(Done, false, true), RowStatus::Background);
        assert_eq!(RowStatus::of(Done, false, false), RowStatus::Idle);
        assert_eq!(RowStatus::of(Working, true, true), RowStatus::Working);
        assert_eq!(RowStatus::of(Decision, true, false), RowStatus::NeedsYou);
        assert_eq!(RowStatus::of(Blocked, true, false), RowStatus::Failed);
        let all = [
            RowStatus::Parked,
            RowStatus::Idle,
            RowStatus::Failed,
            RowStatus::Background,
            RowStatus::Working,
            RowStatus::Failing,
            RowStatus::Done,
            RowStatus::NeedsYou,
        ];
        for pair in all.windows(2) {
            assert!(pair[0] < pair[1]);
            assert!(pair[0].attention() <= pair[1].attention(), "{pair:?}");
        }
    }

    /// For every Pane state and either side of unread, the row's mark says
    /// what the Pane says: green only while it works, ochre while it waits,
    /// red when it failed, a dim check for an unseen finish.
    #[test]
    fn every_pane_state_reads_as_one_mark() {
        use WallState::*;
        for (state, unread, ink) in [
            (Working, false, RUNNING),
            (Failing, false, BLOCKED),
            (Decision, false, ATTENTION),
            (Blocked, false, BLOCKED),
            (Done, false, IDLE),
            (Done, true, IDLE),
            (Idle, false, IDLE),
            (Parked, false, IDLE),
        ] {
            assert_eq!(
                RowStatus::of(state, unread, false).ink(),
                ink,
                "{state:?} unread={unread}"
            );
        }
    }

    #[test]
    fn the_nav_is_a_flat_column_under_native_window_chrome() {
        let mut column = column(Paint::solid(TX_GROUND));
        assert_eq!(
            column.style().size.width,
            Some(px(256.).into()),
            "the reference sidebar has no exterior horizontal inset"
        );
        let mut drawn = card(div());
        let style = drawn.style();
        assert_eq!(style.inset.top, Some(px(NAV_HEADER_H).into()), "under the sidebar topbar");
        assert_eq!(style.inset.left, Some(px(0.).into()));
        assert_eq!(style.inset.bottom, Some(px(0.).into()), "extends to the window foot");
        assert_eq!(style.size.width, Some(px(256.).into()));
        assert_eq!(style.corner_radii.top_left, Some(px(0.).into()));
        assert_eq!(style.corner_radii.bottom_right, Some(px(0.).into()));
        assert_eq!(style.background, Some(NAV_BACKGROUND.into()));
        assert_eq!(style.box_shadow, None);
        let mut inner = content();
        let style = inner.style();
        assert_eq!(style.size.width, Some(px(WIDTH).into()));
        assert_eq!(style.padding.left, Some(px(NAV_COL_PAD).into()));
        assert_eq!(style.padding.top, Some(px(NAV_COL_PAD).into()));
    }

    #[test]
    fn the_edge_tab_wears_the_card_and_points_the_way() {
        for open in [true, false] {
            let mut tab = edge_tab(open);
            let style = tab.style();
            assert_eq!(style.background, Some(NAV_BACKGROUND.into()));
            assert_eq!(style.border_color, Some(gpui::Hsla::from(rgba(NAV_BORDER))));
            assert_eq!(style.border_widths.left, None, "no edge on the card's side");
            assert_eq!(style.corner_radii.top_right, Some(px(8.).into()));
            assert_eq!(style.corner_radii.top_left, None);
        }
    }

    /// A title the box cuts ends in the fade alone — clipped on one line,
    /// never ellipsized under it.
    #[test]
    fn a_title_is_clipped_not_ellipsized() {
        let mut title = rename_target_thread(
            RowTag::of(ThreadKey::fixture(0, 1), false),
            "UI test: color tokens and shell commands".into(),
        );
        let style = title.style().clone();
        assert_eq!(style.text.text_overflow, None, "no …");
        assert_eq!(style.text.white_space, Some(gpui::WhiteSpace::Nowrap));
        assert_eq!(style.overflow.x, Some(gpui::Overflow::Hidden));
    }

    /// Thread 3 here and Thread 3 on another machine are two rows: each
    /// arrives in its own section at its own time.
    #[test]
    fn a_nav_keeps_two_machines_threads_apart() {
        let here = ThreadKey::fixture(0, 3);
        let there = ThreadKey::fixture(1, 3);
        let start = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
        let later = start + std::time::Duration::from_secs(60);
        let mut arrivals = Arrivals::default();
        arrivals.note(
            &[(here, Section::Active, Some(start)), (there, Section::Active, Some(start))],
            start,
        );
        arrivals.note(
            &[(here, Section::Active, Some(start)), (there, Section::Working, Some(start))],
            later,
        );
        assert_eq!(arrivals.since(here), Some(start), "this computer's stays put");
        assert_eq!(arrivals.since(there), Some(later), "the other machine's moved");
    }

    /// Machines whose clocks disagree by an hour interleave as their work
    /// happened (F-A4): a machine's times read on this computer's clock
    /// (`ServerClock::local_ms`, as `Cockpit::last_used` reads them), so
    /// the latest arrival tops the list whatever machine it is on.
    #[test]
    fn skewed_clocks_interleave_as_the_work_happened() {
        use ferrite_core::clock::ServerClock;
        const MINUTE: i64 = 60_000;
        let now: i64 = 1_763_900_000_000;
        let hour = 60 * MINUTE;
        let at = |ms: i64| SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(ms as u64);
        // Each Server said its time as this computer read `now`.
        let fast = ServerClock::sampled(Some(now + hour), now, now);
        let slow = ServerClock::sampled(Some(now - hour), now, now);
        let here = ThreadKey::fixture(0, 1);
        let ahead = ThreadKey::fixture(1, 1);
        let behind = ThreadKey::fixture(2, 1);
        let listed = [
            (here, Section::Active, Some(at(now - 5 * MINUTE))),
            // Two minutes ago on a clock an hour fast.
            (ahead, Section::Active, Some(at(fast.local_ms(now + hour - 2 * MINUTE)))),
            // Nine minutes ago on a clock an hour slow.
            (behind, Section::Active, Some(at(slow.local_ms(now - hour - 9 * MINUTE)))),
        ];
        let mut arrivals = Arrivals::default();
        arrivals.note(&listed, at(now));
        let mut order = vec![behind, here, ahead];
        order.sort_by_key(|thread| arrivals.latest_first(*thread));
        assert_eq!(order, [ahead, here, behind]);
    }

    /// The list's rows travel over T3's 150ms `ease-out`; a shelf's
    /// chevron turns over 150ms, its rows never travel; a slim row's action
    /// fades over 150ms and the undo notice over Tailwind's 200ms.
    #[test]
    fn the_nav_keeps_its_clocks() {
        assert_eq!(motion::NAV_LIST.duration_ms, 150);
        assert_eq!(motion::NAV_LIST.curve, motion::EASE_OUT);
        assert_eq!(motion::SHELF_CHEVRON.duration_ms, 150);
        assert_eq!(motion::SLIM_ACTION.duration_ms, 150);
        assert_eq!(motion::UNDO_NOTICE.duration_ms, 200);
        assert_eq!(motion::UNDO_NOTICE.curve, motion::EASE_OUT_TW);
        assert_eq!(motion::NAV_TOOLTIP_IN.duration_ms, 150);
        assert_eq!((NAV_TOOLTIP_DELAY_MS, NAV_TOOLTIP_HANDOFF_MS), (150, 400));
    }
}

/// The hover each row's wash was last drawn at: the stale-chrome tests read
/// what is on screen, not the blend.
#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    use ferrite_core::ThreadKey;
    use gpui::{FontWeight, Hsla, SharedString};

    /// A slim row's lit level as last drawn (its action's fade, its
    /// badge's colour).
    pub fn drawn_slim(thread: ThreadKey) -> Option<f32> {
        DRAWN_SLIM.with(|drawn| drawn.borrow().get(&thread).copied())
    }

    /// How many characters `Measure::Text` has laid out on this thread.
    pub fn shaped() -> usize {
        SHAPED.with(Cell::get)
    }

    thread_local! {
        pub(super) static SHAPED: Cell<usize> = const { Cell::new(0) };
        pub(super) static DRAWN_SLIM: RefCell<HashMap<ThreadKey, f32>> = RefCell::default();
        pub(super) static DRAWN_HOVER: RefCell<HashMap<SharedString, f32>> =
            RefCell::new(HashMap::new());
        pub(super) static DRAWN_CARDS: RefCell<HashMap<ThreadKey, DrawnCard>> =
            RefCell::new(HashMap::new());
        pub(super) static DRAWN_HOVER_CARD: RefCell<Vec<String>> = RefCell::default();
    }

    /// The lines under the title of the hover detail last drawn: what it
    /// says beyond the card, in order.
    pub fn drawn_hover_card() -> Vec<String> {
        DRAWN_HOVER_CARD.with(|drawn| drawn.borrow().clone())
    }

    /// A Thread's card as last drawn: whether it receded, the opacity it
    /// painted at, its title's ink and weight, and whether its hover strip
    /// stood in its status slot.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct DrawnCard {
        pub receded: bool,
        pub opacity: f32,
        pub title: (Hsla, FontWeight),
        pub strip: bool,
    }

    /// The card for `thread` as last drawn.
    pub fn drawn_card(thread: ThreadKey) -> Option<DrawnCard> {
        DRAWN_CARDS.with(|drawn| drawn.borrow().get(&thread).copied())
    }

    /// The hover the row behind `key` was last drawn at.
    pub fn drawn_hover(key: &str) -> Option<f32> {
        DRAWN_HOVER.with(|drawn| drawn.borrow().get(key).copied())
    }
}
