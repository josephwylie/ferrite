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
//! drawn as a fading ghost. Shelf folds snap.
//!
//! Drawing only, like `pane.rs`: the cockpit assembles a `NavState` per
//! frame from O(1) reads plus its project/branch/parked caches —
//! `Store::load` and `Instruments::of` are banned here, which is what keeps
//! the 24-Pane wall smooth with the nav open. Click wiring stays in
//! `cockpit.rs` and `cockpit/nav_list.rs`, the same split `pane_cell` uses.
//!
//! Every colour and metric is a `theme` token; this file holds no literal
//! of its own.

use std::time::{Instant, SystemTime};

use ferrite_core::store::Provider;
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::ThreadId;

use gpui::prelude::*;
use gpui::{
    div, px, rgb, rgba, App, Div, FontWeight, Hsla, ScrollHandle, SharedString,
    Stateful,
};

use crate::machines::{Connection, MachineRow};
use crate::notifications::{Row as NoticeRow, State as NoticeState};
use ferrite_core::cockpit::StatusRow;
pub use ferrite_pane::mark::{mark, NavTail, RowStatus};
use ferrite_ui::components;
use ferrite_ui::icons::{self, icon};
use ferrite_ui::motion;
use ferrite_ui::theme::*;

mod card;
pub(crate) mod chrome;
pub(crate) mod flip;
pub(crate) mod list;
pub(crate) use card::{card_hover, card_key};
#[cfg(test)]
pub(crate) use card::status_label;
pub use card::{
    card_age, ghost_card, park_button, pin_glyph, pr_chip, snooze_button, strip,
    strip_w, thread_card,
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
pub(crate) trait NavTooltip: gpui::StatefulInteractiveElement + Sized {
    fn nav_tooltip(self, text: impl Into<SharedString>) -> Self {
        self.tooltip(ferrite_ui::menu::nav_tooltip(text)).nav_timing()
    }

    /// `nav_tooltip` naming a bound action's key.
    fn nav_action_tooltip(self, label: impl Into<SharedString>, action: &str) -> Self {
        self.tooltip(ferrite_ui::menu::nav_action_tooltip(label, action))
            .nav_timing()
    }

    fn nav_timing(self) -> Self {
        self.tooltip_show_delay(std::time::Duration::from_millis(NAV_TOOLTIP_DELAY_MS))
            .tooltip_handoff(std::time::Duration::from_millis(NAV_TOOLTIP_HANDOFF_MS))
    }
}

impl<E: gpui::StatefulInteractiveElement> NavTooltip for E {}

/// A row the nav's keyboard can stand on (`cockpit/nav_keys.rs`): a
/// Project's Factory entry, a shelf's header, or a Thread's card or slim
/// row, wherever it is listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NavItem {
    Factory(ProjectId),
    Shelf(Shelf),
    Thread(ThreadId),
}

/// The keyboard's place in the nav: its row, and the blend key that row's
/// hover face reads, so the cursor wears the pointer's face.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NavCursor {
    pub item: NavItem,
    pub key: SharedString,
}

impl NavCursor {
    pub fn factory(project: ProjectId) -> Self {
        NavCursor {
            item: NavItem::Factory(project),
            key: factory_key(project),
        }
    }

    pub fn shelf(shelf: Shelf) -> Self {
        NavCursor {
            item: NavItem::Shelf(shelf),
            key: list::shelf_key(shelf),
        }
    }

    /// The key mirrors the row's own (`thread_card`, `list::slim_row`).
    pub fn thread(thread: ThreadId) -> Self {
        NavCursor {
            item: NavItem::Thread(thread),
            key: card::card_key(thread),
        }
    }
}

const INNER_W: f32 = WIDTH - 2.0 * NAV_COL_PAD;

/// A shelf at the foot of the list (ADR 0040), in drawing order.
pub use ferrite_core::settings::NavShelf as Shelf;

/// The shelves, in the order the list draws them.
pub const SHELVES: [Shelf; 3] = [Shelf::Working, Shelf::Snoozed, Shelf::Parked];

/// What the nav draws this frame (ADR 0040): one list, section by section
/// in drawing order. A Thread is in one section, by priority: snoozed,
/// parked, pinned, working, active. Nothing here is a store read — the
/// cockpit assembles it from O(1) reads plus its caches.
#[derive(Clone, Default)]
pub struct NavState {
    /// Pinned cards (`Settings::pinned_threads`), in pin order.
    pub pinned: Vec<ThreadRow>,
    /// The Inbox: open Threads awaiting the operator, pending Decisions
    /// first, oldest request first.
    pub active: Vec<ThreadRow>,
    /// Working Threads, newest send first.
    pub working: Vec<ThreadRow>,
    /// Snoozed Threads, the soonest to wake first.
    pub snoozed: Vec<ThreadRow>,
    /// Parked Threads, in the park order.
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

    pub fn open_rows(&self) -> impl Iterator<Item = &ThreadRow> {
        self.pinned
            .iter()
            .chain(&self.active)
            .chain(&self.working)
            .chain(&self.snoozed)
    }
}

/// What a set of rows rolls up to: the most urgent of them. `None` for
/// no rows.
pub fn roll_up<'a>(rows: impl IntoIterator<Item = &'a ThreadRow>) -> Option<RowStatus> {
    rows.into_iter().map(|row| row.status).max()
}

/// One Thread's row — its card (`card::thread_card`), identical whether
/// it is a loose Thread, a Group member or a parked one; only the
/// container differs. Every field is a cache read (`CockpitView::thread_row`).
#[derive(Clone)]
pub struct ThreadRow {
    pub thread: ThreadId,
    pub name: SharedString,
    /// What the Thread is doing right now.
    pub status: RowStatus,
    /// The Project's name, from the facts cache: the badge's monogram and
    /// hue read it.
    pub project: Option<SharedString>,
    /// The board shows this Thread (Solo), or it is the shown Group's
    /// focused member: the row carries the selection.
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
}

#[cfg(test)]
impl ThreadRow {
    /// A row with nothing known but its Thread, name and state.
    pub fn bare(thread: ThreadId, name: impl Into<SharedString>, status: RowStatus) -> Self {
        ThreadRow {
            thread,
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
        }
    }
}

/// The width a card's lines have, from its content inset to its right
/// padding.
const TEXT_W: f32 = INNER_W - 2.0 * NAV_ROW_PAD_X;

// ------------------------------------------------------------- measuring

/// The faces' own widths, a character at a time as each lays out alone:
/// what gpui's truncation sums (`LineWrapper::width_for_char`), and what a
/// truncating box is pinned against. Never the glyph's bare advance: the
/// system face lays out wider than its advances at the UI's sizes (its
/// optical size and tracking), so a box sized from them cut a name that
/// fits (`ferri…` for `ferrite`). A sum without kerning errs wide, which
/// keeps a name whole.
#[derive(Clone, Copy)]
pub enum Measure<'a> {
    Text(&'a App),
    /// A fixed advance per character, for unit tests.
    #[cfg(test)]
    Cells(f32),
}

impl Measure<'_> {
    /// `text` in Geist at `size` and `weight`.
    pub fn ui(&self, text: &str, size: f32, weight: FontWeight) -> f32 {
        self.width(FONT_UI, text, size, weight)
    }

    fn width(&self, family: &'static str, text: &str, size: f32, weight: FontWeight) -> f32 {
        match self {
            Measure::Text(cx) => {
                let system = cx.text_system();
                let font = gpui::Font {
                    weight,
                    ..gpui::font(family)
                };
                let id = system.resolve_font(&font);
                text.chars()
                    .map(|ch| f32::from(system.layout_width(id, px(size), ch)))
                    .sum()
            }
            #[cfg(test)]
            Measure::Cells(advance) => text.chars().count() as f32 * advance,
        }
    }
}

/// A line pinned to `width` that fades out at its right edge, as Claude's
/// sidebar titles do, rather than ending in "…": the line lays out whole,
/// the box clips it, and `components::line_fade` in `ground` — the row's
/// fill under it (`row_ground`) — fades the cut. `ground` is `None` where
/// the line fits (`cut_ground`), so a whole name keeps its last letters,
/// and where the box holds a rename field, whose caret the fade would hide.
///
/// A pinned line needs a **definite** width on its very first measure.
/// gpui caches a nowrap line's first measure permanently, and taffy only
/// hands a text leaf a definite width when the leaf's flex container is a
/// **column** whose own available width is definite — which taffy derives
/// from the child's own min/max width. Hence: flex **column**, with min and
/// max width pinned to the box.
fn fitted(
    width: f32,
    size: f32,
    ink: u32,
    weight: FontWeight,
    text: impl IntoElement,
    ground: Option<Hsla>,
) -> Div {
    fitted_in(width, size, rgb(ink).into(), weight, text, ground)
}

/// `fitted` in any ink, an alpha one included (a receded card's title).
fn fitted_in(
    width: f32,
    size: f32,
    ink: Hsla,
    weight: FontWeight,
    text: impl IntoElement,
    ground: Option<Hsla>,
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
        .children(ground.map(components::line_fade))
}

/// `ground`, for `fitted`'s fade, only where `text` at `size` and `weight`
/// runs past `width`: a line that fits is drawn whole.
fn cut_ground(
    measure: &Measure,
    text: &str,
    size: f32,
    weight: FontWeight,
    width: f32,
    ground: Hsla,
) -> Option<Hsla> {
    (measure.ui(text, size, weight) > width).then_some(ground)
}

/// What sits at a line's right: never cut, `NAV_WORD_GAP` after what it
/// follows.
fn trailing(text: impl Into<SharedString>, size: f32, ink: u32) -> Div {
    div()
        .flex_shrink_0()
        .pl(px(NAV_WORD_GAP))
        .text_size(px(size))
        .line_height(px(NAV_LINE))
        .text_color(rgb(ink))
        .whitespace_nowrap()
        .child(text.into())
}

// ------------------------------------------------------------- the rows

/// The nav's one pointer role: nothing at rest, white 8% under the
/// pointer at once (T3's rows snap), and the selected row white 11%
/// whatever the pointer does: no ring, the fill alone. The pointer stays
/// the arrow.
fn wash<E: Styled + InteractiveElement>(mut element: E, key: SharedString, selected: bool) -> E {
    if selected {
        element = element.bg(rgba(NAV_CARD_ACTIVE));
    } else {
        let t = motion::hover_t(&key);
        #[cfg(test)]
        testing::DRAWN_HOVER.with(|drawn| drawn.borrow_mut().insert(key.clone(), t));
        if t > 0.0 {
            element = element.bg(motion::mix(
                rgba(TRANSPARENT).into(),
                rgba(NAV_CARD_HOVER).into(),
                t,
            ));
        }
    }
    element.interactivity().on_hover(motion::hover_snap(key));
    element
}

/// The frame every row shares: rounded, washed under the pointer.
fn row_frame(id: impl Into<gpui::ElementId>, key: SharedString, selected: bool) -> Stateful<Div> {
    wash(
        div()
            .id(id)
            .relative()
            .flex()
            .flex_shrink_0()
            .rounded(px(R_CONTROL))
            .whitespace_nowrap()
            .text_size(px(FS_NAV))
            .line_height(px(NAV_LINE)),
        key,
        selected,
    )
}

/// A one-line row's frame: 30 high, its items centred, `NAV_ROW_PAD_X` in.
fn line_frame(id: impl Into<gpui::ElementId>, key: SharedString, selected: bool) -> Stateful<Div> {
    pitched(row_frame(id, key, selected))
        .flex_row()
        .items_center()
        .h(px(NAV_ROW_H))
        .px(px(NAV_ROW_PAD_X))
}

/// A list row's share of the gap between rows (`NAV_ROW_GAP`), half above
/// and half below, so a run of rows steps 34.
fn pitched<E: Styled>(row: E) -> E {
    row.my(px(NAV_ROW_GAP / 2.0))
}

// ------------------------------------------------------------- the marks

/// The row a mark sits in, in a view's list: on line 1 of a two-line
/// Notice row, or small, centred on a machine's one-line row.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MarkSeat {
    TwoLine,
    Member,
}

/// A row's mark where the mockups place it (`NAV_*_AT`), from its mark
/// column's left (`x`) and its row's top.
fn mark_at(status: RowStatus, x: f32, seat: MarkSeat) -> Div {
    let small = seat == MarkSeat::Member;
    let dot = matches!(
        status,
        RowStatus::NeedsYou | RowStatus::Failed | RowStatus::Background
    );
    let (left, top) = match seat {
        MarkSeat::TwoLine if dot => NAV_DOT_AT,
        MarkSeat::Member if dot => NAV_DOT_AT_SMALL,
        MarkSeat::TwoLine => NAV_ICON_AT,
        MarkSeat::Member => NAV_ICON_AT_LINE,
    };
    div()
        .absolute()
        .left(px(x + left))
        .top(px(top))
        .child(mark(status, small))
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

pub const CARD_TOP: f32 = WIN_CHROME_H + NAV_INSET_Y;
pub const EDGE_TAB_OPEN_X_OVER_RING: f32 = NAV_INSET + WIDTH - HAIRLINE_W;

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
        .bg(paint::SIDEBAR)
        .shadow(components::float_shadow())
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
                .border_1()
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
        .mb(px(NAV_SEARCH_ROW_GAP))
        .child(arrows)
}

pub fn icon_button(
    key: impl Into<SharedString>,
    glyph: &'static str,
    glyph_size: f32,
    size: f32,
    radius: f32,
) -> Stateful<Div> {
    let blend: SharedString = key.into();
    let t = motion::hover_t(&blend);
    let selector = blend.clone();
    div()
        .id(blend.clone())
        .debug_selector(move || selector.to_string())
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(size))
        .rounded(px(radius))
        .when(t > 0.0, |door| {
            door.bg(motion::mix(
                rgba(TRANSPARENT).into(),
                rgba(NAV_CARD_HOVER).into(),
                t,
            ))
        })
        .on_hover(motion::hover_snap(blend))
        .child(icon_ink(glyph, glyph_size, t))
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
        .bg(paint::SIDEBAR)
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

fn factory_key(project: ProjectId) -> SharedString {
    format!("nav-factory-{}", project.get()).into()
}

/// A Project's Factory entry (ADR 0037), over the list's cards: the
/// board's icon, `Factory` and the Project's name, and how many of its
/// Tasks need the operator. Selected while its board shows.
pub fn factory_row(
    project: ProjectId,
    title: SharedString,
    count: usize,
    active: bool,
) -> Stateful<Div> {
    let key = factory_key(project).to_string();
    let label = key.clone();
    let tally = key.clone();
    line_frame(
        SharedString::from(key.clone()),
        SharedString::from(key.clone()),
        active,
    )
    .debug_selector(move || key.clone())
    .gap(px(NAV_ICON_GAP))
    .text_color(rgb(INK_STRONG))
    .child(
        div()
            .flex()
            .flex_shrink_0()
            .size(px(NAV_ICON))
            .child(icon(icons::BOARD, NAV_ICON, INK_STRONG)),
    )
    .child(
        div()
            .debug_selector(move || format!("{label}-label"))
            .flex()
            .flex_1()
            .min_w_0()
            .child("Factory")
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .pl(px(NAV_WORD_GAP))
                    .text_color(rgb(INK_SUBTLE))
                    .child(title),
            ),
    )
    .when(count > 0, |row| {
        row.child(
            components::tabular(trailing(count.to_string(), FS_META, INK_SUBTLE))
                .debug_selector(move || format!("{tally}-count")),
        )
    })
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

pub fn edge_fades(scroll: &ScrollHandle) -> Vec<Div> {
    let y = scroll.offset().y;
    let below = scroll.max_offset().y + y > px(0.);
    let fill = paint::SIDEBAR.hsla();
    let top = (y < px(0.)).then(|| {
        div()
            .debug_selector(|| "nav-fade-top".into())
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .child(components::edge_fade_over(fill, false, NAV_EDGE_FADE))
    });
    let foot = below.then(|| {
        div()
            .debug_selector(|| "nav-fade-foot".into())
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .child(components::edge_fade_over(fill, true, NAV_EDGE_FADE))
    });
    top.into_iter().chain(foot).collect()
}

/// The Thread card at root — see `card::thread_card`.
#[cfg(test)]
pub fn thread_row(row: &ThreadRow) -> Stateful<Div> {
    thread_card(
        row,
        row.name.clone(),
        CardDoors::default(),
        false,
        &Measure::Cells(7.0),
    )
}

/// A request from a machine off screen, among the Active cards, two
/// lines: its mark, its title
/// (`INK_STRONG` while unread) and its age, then its state's word and
/// detail in the word's ink. The cursor's row is selected.
pub fn notice_row(index: usize, row: &NoticeRow, cursor: bool, measure: &Measure) -> Stateful<Div> {
    let key = SharedString::from(format!("notice-row-{index}"));
    let status = match row.state {
        NoticeState::NeedsYou(_) => RowStatus::NeedsYou,
        NoticeState::Failed => RowStatus::Failed,
        NoticeState::Done => RowStatus::Done,
        NoticeState::Idle => RowStatus::Idle,
    };
    let ink = match row.state {
        NoticeState::NeedsYou(_) => ATTENTION,
        NoticeState::Failed => BLOCKED,
        NoticeState::Done | NoticeState::Idle => {
            if cursor {
                INK_STRONG
            } else {
                INK_SUBTLE
            }
        }
    };
    let detail = row.detail.text();
    let line = if detail.is_empty() {
        row.word().to_string()
    } else {
        format!("{} \u{b7} {detail}", row.word())
    };
    let text_w = INNER_W - NAV_TITLE_X - NAV_ROW_PAD_X;
    // In its first minute a Notice has no age to say: the title takes the
    // line.
    let age = (!row.when.is_empty()).then(|| row.when.clone());
    let age_w = age
        .as_ref()
        .map_or(0.0, |age| measure.ui(age, FS_META, W_BODY) + NAV_WORD_GAP);
    // Primary ink, as every nav title; unread says so in weight.
    let title_weight = if cursor || !row.read { W_LABEL } else { W_BODY };
    let ground = card::card_ground(&key, cursor, false);
    let line_ground = cut_ground(measure, &line, FS_META, W_BODY, text_w, ground);
    row_frame(("notice-row", index), key, cursor)
        .debug_selector(move || format!("notice-row-{index}"))
        .flex_col()
        .justify_center()
        .h(px(NAV_ROW2_H))
        .pl(px(NAV_TITLE_X))
        .pr(px(NAV_ROW_PAD_X))
        .child(mark_at(status, NAV_ROW_PAD_X, MarkSeat::TwoLine))
        .child(
            div()
                .flex()
                .items_center()
                .h(px(NAV_LINE))
                .child(fitted(
                    text_w - age_w,
                    FS_NAV,
                    INK_STRONG,
                    title_weight,
                    row.title.clone(),
                    cut_ground(
                        measure,
                        &row.title,
                        FS_NAV,
                        title_weight,
                        text_w - age_w,
                        ground,
                    ),
                ))
                .children(age.map(|age| {
                    div()
                        .flex()
                        .flex_1()
                        .justify_end()
                        .child(components::tabular(trailing(age, FS_META, INK_SUBTLE)))
                })),
        )
        .child(div().flex().h(px(NAV_LINE)).child(fitted(
            text_w,
            FS_META,
            ink,
            W_BODY,
            SharedString::from(line),
            line_ground,
        )))
}

/// A refusal from the last change (a Group's, a Snooze's), at the top of
/// the list, until the next change succeeds: a `BLOCKED` dot and the refusal, dim, wrapping.
/// Colour on the dot, never the row.
pub fn notice(text: SharedString) -> Div {
    div()
        .debug_selector(|| "nav-notice".into())
        .flex()
        .flex_shrink_0()
        .items_start()
        .gap(px(NAV_ICON_GAP))
        .px(px(NAV_ROW_PAD_X))
        .py(px(SPACE_1))
        .text_size(px(FS_META))
        .line_height(px(NAV_LINE))
        .text_color(rgb(INK_SUBTLE))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .w(px(NAV_ICON))
                .h(px(NAV_LINE))
                .child(div().size(px(NAV_DOT)).rounded_full().bg(rgb(BLOCKED))),
        )
        .child(div().flex_1().min_w_0().whitespace_normal().child(text))
}

// ------------------------------------------------------------- machines

/// A machine's heading (ADR 0026), in the heading style: its chevron
/// (`fold`, from `machine_fold`, with its own press), its connection mark,
/// its name — the strong ink on the machine the window is on — and a word
/// for a connection that is not up. The cockpit wires the press: onto the
/// machine.
pub fn machine_heading(row: &MachineRow, fold: impl IntoElement) -> Stateful<Div> {
    let index = row.index;
    let word: Option<SharedString> = match row.connection {
        Connection::Online => None,
        Connection::Connecting => Some(words::CONNECTING.into()),
        Connection::Attention => Some(words::NEEDS_YOU.into()),
        Connection::Disabled => Some(words::OFF.into()),
    };
    let dot = |ink: u32| div().size(px(NAV_DOT)).rounded_full().bg(rgb(ink));
    let ring = |ink: u32| {
        div()
            .size(px(NAV_DOT))
            .rounded_full()
            .border_1()
            .border_color(rgb(ink))
    };
    let glyph = match row.connection {
        Connection::Online => dot(RUNNING),
        Connection::Connecting => ring(INK_SUBTLE),
        Connection::Attention => dot(ATTENTION),
        Connection::Disabled => ring(TEXT_FAINT),
    };
    let key = SharedString::from(format!("nav-machine-{index}"));
    line_frame(("nav-machine", index), key, false)
        .debug_selector(move || format!("nav-machine-{index}"))
        .gap(px(SPACE_2))
        .text_size(px(FS_META))
        .font_weight(W_LABEL)
        .child(fold)
        .child(glyph)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(rgb(if row.selected { INK_STRONG } else { INK_SUBTLE }))
                .child(SharedString::from(row.name.clone())),
        )
        .children(word.map(|word| trailing(word, FS_META, INK_SUBTLE)))
}

/// A machine heading's chevron: its press folds the machine to its
/// heading.
pub fn machine_fold(index: usize, folded: bool) -> Stateful<Div> {
    div()
        .id(("nav-machine-fold", index))
        .debug_selector(move || format!("nav-machine-fold-{index}"))
        .flex_shrink_0()
        .child(components::fold_chevron(
            ("nav-machine-chevron", index),
            !folded,
            NAV_CHEVRON,
            INK_SUBTLE,
        ))
}

/// One open Thread on a machine the window is not on: its mark, its title
/// and its word, from the status the machine streams. A press goes to the
/// machine. A machine that went keeps its rows, dimmed.
pub fn machine_thread_row(machine: usize, row: &StatusRow, lost: bool) -> Stateful<Div> {
    let (status, word) = if row.waiting {
        (RowStatus::NeedsYou, Some((words::NEEDS_YOU, ATTENTION)))
    } else if row.working {
        (RowStatus::Working, Some((words::WORKING, INK_SUBTLE)))
    } else {
        (RowStatus::Idle, None)
    };
    let thread = row.thread.get();
    let key = SharedString::from(format!("nav-machine-{machine}-{thread}"));
    let title: SharedString = row.title.clone().unwrap_or_default().into();
    // Thread ids are per Server: the machine is part of the id.
    row_frame(key.clone(), key.clone(), false)
        .debug_selector(move || key.to_string())
        .when(lost, |row| row.opacity(MACHINE_LOST_OPACITY))
        .flex_row()
        .items_center()
        .h(px(NAV_ROW_H))
        .pl(px(NAV_TITLE_X))
        .pr(px(NAV_ROW_PAD_X))
        .child(mark_at(status, NAV_ROW_PAD_X - 1.0, MarkSeat::Member))
        .child(div().flex_1().min_w_0().truncate().child(title))
        .children(word.map(|(word, ink)| trailing(word, FS_META, ink)))
}

// ------------------------------------------------------------- rename and drag

/// A Thread title that can be renamed: the row's own title text, and
/// nothing else. It wears **no** hover wash: the double click is the
/// affordance; the row is the control.
pub fn rename_target_thread(thread: ThreadId, title: SharedString) -> Stateful<Div> {
    div()
        .id(("rename-thread", thread.get() as usize))
        .debug_selector(move || format!("rename-thread-{}", thread.get()))
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .child(title)
}

/// A nav row lifted off the list while it is dragged: the same row the
/// list draws, at the column's inner width, on the float ground with its
/// ring and the one float shadow.
pub fn drag_row(row: Stateful<Div>) -> Div {
    div()
        .w(px(INNER_W))
        .font_family(FONT_UI)
        .text_size(px(FS_NAV))
        .line_height(px(NAV_LINE))
        .text_color(rgb(INK_STRONG))
        .bg(paint::FLOAT)
        .rounded(px(R_CONTROL))
        .border_1()
        .border_color(paint::FLOAT_EDGE)
        .shadow(ferrite_ui::components::float_shadow())
        .child(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::ThreadId;
    use ferrite_pane::pane::WallState;

    fn thread() -> ThreadRow {
        selected_thread(false)
    }

    fn selected_thread(selected: bool) -> ThreadRow {
        ThreadRow {
            project: Some("ferrite".into()),
            selected,
            tail: NavTail::Age("2h".into()),
            ..ThreadRow::bare(ThreadId::new(8), "thread-08", RowStatus::Idle)
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

    /// N-13: the pointer stays the arrow over every row.
    #[test]
    fn every_nav_row_keeps_the_arrow() {
        let cursor = |mut drawn: Stateful<Div>| drawn.style().mouse_cursor;
        assert_eq!(cursor(thread_row(&thread())), None);
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
        assert_eq!(
            NavTail::Waiting(words::APPROVAL).member_word(),
            Some((words::NEEDS_YOU.into(), ATTENTION))
        );
        assert_eq!(
            NavTail::Failed.member_word(),
            Some((words::FAILED.into(), BLOCKED))
        );
        assert_eq!(
            NavTail::Working("1m".into()).member_word(),
            Some(("1m".into(), ferrite_ui::theme::INK_SUBTLE))
        );
        assert_eq!(NavTail::Age(age()).member_word(), None);
    }

    /// The nav never says `now` (C10).
    #[test]
    fn the_nav_never_says_now() {
        let at = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10_000);
        for secs in [0, 1, 30, 59] {
            let age = crate::facts::since_label(at, at + std::time::Duration::from_secs(secs));
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
        let mut dot = mark_at(RowStatus::NeedsYou, NAV_ROW_PAD_X, MarkSeat::TwoLine);
        assert_eq!(
            dot.style().inset.left,
            Some(px(NAV_ROW_PAD_X + 3.0).into()),
            "the 8px dot centred where an icon is, no halo"
        );
    }

    /// Herdr's four states over Ferrite's own, and a roll-up's order.
    #[test]
    fn a_row_is_one_of_four_states_and_a_heading_rolls_its_rows_up() {
        use WallState::*;
        assert_eq!(RowStatus::of(Done, true, true), RowStatus::Done);
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
    fn the_nav_is_a_card_floating_under_the_head_row() {
        let mut column = column(Paint::solid(TX_GROUND));
        assert_eq!(
            column.style().size.width,
            Some(px(280.).into()),
            "the card and a 12px gap either side"
        );
        let mut drawn = card(div());
        let style = drawn.style();
        assert_eq!(style.inset.top, Some(px(WIN_CHROME_H + 40.).into()), "under the head row");
        assert_eq!(style.inset.left, Some(px(12.).into()));
        assert_eq!(style.inset.bottom, Some(px(40.).into()), "short of the window's foot");
        assert_eq!(style.size.width, Some(px(256.).into()));
        assert_eq!(style.corner_radii.top_left, Some(px(18.).into()));
        assert_eq!(style.corner_radii.bottom_right, Some(px(18.).into()));
        assert_eq!(style.background, Some(paint::SIDEBAR.into()));
        assert_eq!(style.box_shadow, Some(components::float_shadow()), "it floats");
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
            assert_eq!(style.background, Some(paint::SIDEBAR.into()));
            assert_eq!(style.border_color, Some(gpui::Hsla::from(rgba(NAV_BORDER))));
            assert_eq!(style.border_widths.left, None, "no edge on the card's side");
            assert_eq!(style.corner_radii.top_right, Some(px(8.).into()));
            assert_eq!(style.corner_radii.top_left, None);
        }
    }

    /// The ⌘ digits are bound: each names the action its key is bound to.
    #[test]
    fn the_command_digits_are_bound() {
        for ordinal in 1..=9 {
            let action = crate::cockpit::focus_thread_action(ordinal).expect("one action per digit");
            assert_eq!(
                ferrite_ui::components::bound_chord(action),
                Some(format!(
                    "{}-{ordinal}",
                    if cfg!(target_os = "macos") {
                        "cmd"
                    } else {
                        "ctrl"
                    }
                )),
                "⌘{ordinal}"
            );
        }
        assert_eq!(crate::cockpit::focus_thread_action(10), None);
    }

    #[test]
    fn a_title_fades_into_its_cards_ground() {
        let key = SharedString::from("nav-thread-0-fade");
        assert_ne!(card::card_ground(&key, true, false), paint::SIDEBAR.hsla());
        assert_eq!(card::card_ground(&key, false, false), paint::SIDEBAR.hsla());
        assert_eq!(paint::SIDEBAR, paint::PANEL);
    }

    /// CP7 regression 1: the fade is drawn only over a line that runs
    /// past its box — `ferrite` at seven 7px cells fits 49px whole and
    /// keeps its `e`; in 40px it is cut, and fades.
    #[test]
    fn only_a_cut_line_fades() {
        let measure = Measure::Cells(7.0);
        let ground = paint::SIDEBAR.hsla();
        let cut = |width| cut_ground(&measure, "ferrite", FS_META, W_BODY, width, ground);
        assert_eq!(cut(49.0), None, "a name that fits is whole");
        assert_eq!(cut(120.0), None);
        assert_eq!(cut(40.0), Some(ground), "a cut name fades");
    }

    /// CP7 regression 2: a title the box cuts ends in the fade alone —
    /// clipped on one line, never ellipsized under it.
    #[test]
    fn a_title_is_clipped_not_ellipsized() {
        let mut title = rename_target_thread(
            ThreadId::new(1),
            "UI test: color tokens and shell commands".into(),
        );
        let style = title.style().clone();
        assert_eq!(style.text.text_overflow, None, "no …");
        assert_eq!(style.text.white_space, Some(gpui::WhiteSpace::Nowrap));
        assert_eq!(style.overflow.x, Some(gpui::Overflow::Hidden));
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
#[cfg(test)]
pub(crate) mod testing {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use ferrite_core::ThreadId;
    use gpui::{FontWeight, Hsla, SharedString};

    /// A slim row's lit level as last drawn (its action's fade, its
    /// badge's colour).
    pub fn drawn_slim(thread: ThreadId) -> Option<f32> {
        DRAWN_SLIM.with(|drawn| drawn.borrow().get(&thread).copied())
    }

    thread_local! {
        pub(super) static DRAWN_SLIM: RefCell<HashMap<ThreadId, f32>> = RefCell::default();
        pub(super) static DRAWN_HOVER: RefCell<HashMap<SharedString, f32>> =
            RefCell::new(HashMap::new());
        pub(super) static DRAWN_CARDS: RefCell<HashMap<ThreadId, DrawnCard>> =
            RefCell::new(HashMap::new());
    }

    /// A Thread's card as last drawn: whether it receded, the opacity it
    /// painted at, its title's ink and weight, and whether its hover strip
    /// stood in its status slot.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(crate) struct DrawnCard {
        pub receded: bool,
        pub opacity: f32,
        pub title: (Hsla, FontWeight),
        pub strip: bool,
    }

    /// The card for `thread` as last drawn.
    pub(crate) fn drawn_card(thread: ThreadId) -> Option<DrawnCard> {
        DRAWN_CARDS.with(|drawn| drawn.borrow().get(&thread).copied())
    }

    /// The hover the row behind `key` was last drawn at.
    pub(crate) fn drawn_hover(key: &str) -> Option<f32> {
        DRAWN_HOVER.with(|drawn| drawn.borrow().get(key).copied())
    }
}
