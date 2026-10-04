//! The left navigation column (#21): the Needs-you strip, then one section
//! per Project — its loose Threads and its Groups, each Group's members
//! hanging under it — and, at the foot of the column, the **Parked**
//! section: every parked Thread no Group claims, folded shut by default so
//! the tree above holds only what is running. It is a view, never the only
//! door: everything a row does (focus, revive, regroup) stays reachable
//! from the keyboard and the ⌘K palette, which also holds what the column
//! no longer draws — the Project filter, the sort, a new Project, a new
//! Thread in a Project.
//!
//! Drawing only, like `pane.rs`: the cockpit assembles a `NavState` per
//! frame from O(1) reads plus its project/branch/parked caches —
//! `Store::load` and `Instruments::of` are banned here, which is what keeps
//! the 24-Pane wall smooth with the nav open. Click wiring stays in
//! `cockpit.rs`, the same split `pane_cell` uses.
//!
//! **A terminal list on the chrome** (the prototype's `.nav`). The column
//! paints `paint::CHROME` under the titlebar band, with no hairline to the
//! content: its one edge is the seam column right of it (`seam`). Every
//! item is one 20px line of Geist Mono laid out in character cells (the
//! WP-G section of `theme.rs` draws the grid): a 2ch cursor cell (`❯` in
//! the accent on the selected row), a 2ch mark cell (the status dot, a
//! working Thread's braille spinner, a faint disclosure triangle), the
//! title, and one dim word at the right. A Group's members hang under it on
//! faint tree glyphs (`├ ` / `└ `). There is no subtitle line, no provider
//! logo, no tooltip and no keyboard-hint row: a row's other facts live in
//! its Pane head and in the palette's context column.
//!
//! Colour is state, carried by the dot alone; every right-hand word is
//! `TEXT_MUTED`. The selection is `paint::SELECTION` on the row of what the
//! board shows — the Group being viewed, or the Solo Thread — with the `❯`
//! and a `TEXT_STRONG` title; hover is `paint::HOVER`, reaching full in
//! 80ms, and the pointer stays the arrow. One thing moves: a working
//! Thread's spinner. Nothing pulses.
//!
//! While any Thread waits on the operator, the **Needs-you strip** opens
//! the column: its rows are the answer order, and its first row is what ⌘D
//! and the wall's `1`/`2`/`3` act on.
//!
//! **Folding** (cmd-B) rides the column to nothing while its content fades
//! on its own clock; the titlebar cell over it (`chrome_band`) rides to the
//! folded cell that keeps the traffic lights, the sidebar toggle and the
//! bell, cross-fading from the chrome to the reading plane.
//!
//! Every colour and metric is a `theme` token; this file holds no literal
//! of its own.

use ferrite_core::groups::GroupId;
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::ThreadId;

use gpui::prelude::*;
use gpui::{
    div, px, relative, rgb, rgba, AnyElement, Div, FontWeight, Hsla, ScrollHandle, SharedString,
    Stateful,
};

use crate::cockpit::thread_status;
use crate::components;
use crate::icons::{self, icon};
use crate::motion::{self, MotionSpec};
use crate::pane::WallState;
use crate::theme::*;

/// The nav's two widths — 36ch open, and nothing folded (cmd-B).
/// `CockpitView::cell()` subtracts whichever is live, so the nav stays part
/// of the semantic-zoom input rather than a special case.
pub use crate::theme::{NAV_FOLDED_W as FOLDED_WIDTH, NAV_WIDTH as WIDTH};

/// The slack a truncating title's budget gets over its visible box.
///
/// gpui truncates by summing each character's advance measured **alone**
/// (gpui text_system/line_wrapper.rs `width_for_char`, cached per char) and
/// keeps a prefix only while `width + suffix_width < truncate_width` — a
/// strict `<`, against CSS's `<=`. At an exact fit the last glyph that
/// would still fit is dropped and its cell is left empty. Handing the
/// truncator this much extra budget restores the glyph; the visible box
/// stays exactly its pinned width, and clips. It is under one cell, so it
/// can never admit a glyph past the box: a cut title is its box's cells
/// less one, and a whole `…` in the last.
const TRUNCATE_SLOP: f32 = SPACE_1;

/// A child SVG paints from its **own** style — an ambient text colour
/// reaches text but never an `svg()` — so the toggle's glyph reads the
/// door's blend itself.
const COLLAPSE_KEY: &str = "nav-collapse";

/// A nav row's hover wash (the prototype's `.nrow`'s `.08s`).
pub(crate) const ROW_HOVER: MotionSpec = MotionSpec::new(MOTION_NAV_HOVER_MS, motion::EASE);
/// A titlebar door's hover, ground and glyph (`.ib`'s `.1s`).
pub(crate) const DOOR_HOVER: MotionSpec = MotionSpec::new(MOTION_NAV_DOOR_MS, motion::EASE);
/// The column's content fading as it folds or opens (`.nav > *`'s `.15s
/// ease`), on its own clock apart from the width's ride.
pub(crate) const CONTENT_FADE: MotionSpec = MotionSpec::new(MOTION_NAV_FADE_MS, motion::EASE);
/// The titlebar cell's cross-fade between the chrome and the plane
/// (`.tbl`'s `.2s ease`).
pub(crate) const BAND_FADE: MotionSpec = MotionSpec::new(MOTION_NAV_BAND_MS, motion::EASE);

/// What a triangle in the nav folds (N-4): a Project section shut to its
/// heading, a Group shut to its row (its count kept), or the Parked section
/// shut to its header. Held in memory by the cockpit
/// (`CockpitView::set_nav_fold`), so a fold survives every re-render and
/// view switch; opening a folded Group never unfolds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum NavFold {
    Project(ProjectId),
    Group(GroupId),
    Parked,
}

/// What the nav draws this frame: the Needs-you strip, the Project
/// sections, the Parked section. Nothing here is a store read — the
/// cockpit assembles it from O(1) reads plus its project/branch caches.
pub struct NavState {
    /// Every Thread waiting on the operator, in the answer order
    /// (`Cockpit::needs_you`), whatever the Project filter says: the strip
    /// is the queue ⌘D walks, and its first row is the answer target.
    pub needs_you: Vec<NeedsYouRow>,
    /// One section per Project with a row to show, in creation order, then
    /// `Other` for Threads whose Project cannot be read.
    pub sections: Vec<ProjectSection>,
    /// Every Group block the sections draw, in the order they draw them;
    /// a section names its own by index.
    pub groups: Vec<GroupBlock>,
    /// Every loose Thread row — open, in no home Group — in the order the
    /// sections draw them; a section names its own by index.
    pub solos: Vec<ThreadRow>,
    /// The Parked section's rows: parked Threads no Group claims as home,
    /// in the park order. A parked Group member stays under its Group — the
    /// Group is its place, and opening the Group revives it there.
    pub parked: Vec<ThreadRow>,
    /// Whether the Parked section is unfolded. Shut by default: the tree
    /// is for what is running, and the section is where the rest wait.
    pub parked_open: bool,
}

impl NavState {
    /// Every row the tree draws, in the order it draws them: each section's
    /// loose rows, then the members of each of its unfolded Groups. A
    /// folded section or Group draws none. The strip and the Parked
    /// section are not the tree; tests read the tree's order from here.
    #[cfg(test)]
    pub fn ordered_rows(&self) -> Vec<&ThreadRow> {
        let mut rows = Vec::new();
        for section in self.sections.iter().filter(|section| !section.folded) {
            rows.extend(section.solos.iter().map(|index| &self.solos[*index]));
            for group in section.groups.iter().map(|index| &self.groups[*index]) {
                if !group.folded {
                    rows.extend(group.members.iter());
                }
            }
        }
        rows
    }

    /// The loose rows alone, in the tree's order (folded sections too).
    #[cfg(test)]
    pub fn ordered_solos(&self) -> Vec<&ThreadRow> {
        self.sections
            .iter()
            .flat_map(|section| section.solos.iter().map(|index| &self.solos[*index]))
            .collect()
    }
}

/// One Project's run of the tree: its heading (the name in `W_STRONG`
/// `TEXT_STRONG`, its branch dim at the right), its loose rows, then its
/// Groups — each Group listed under the Project of its first member.
pub struct ProjectSection {
    pub project: Option<ProjectId>,
    pub label: SharedString,
    /// The branch the heading names at its right: the checkout every row
    /// shares, else the Project's default. `None` says nothing.
    pub branch: Option<SharedString>,
    /// The heading's triangle folded it: it draws `▸` and nothing under it.
    pub folded: bool,
    /// Indices into `NavState::solos`.
    pub solos: Vec<usize>,
    /// Indices into `NavState::groups`.
    pub groups: Vec<usize>,
}

/// One row of the Needs-you strip: a second, reference row for a Thread
/// that waits on the operator (its own row stays where it is in the tree),
/// with what it waits for (`approval` / `question`) as its word.
#[derive(Clone)]
pub struct NeedsYouRow {
    pub row: ThreadRow,
    pub kind: &'static str,
}

/// One Group and the Threads hanging under it.
#[derive(Clone)]
pub struct GroupBlock {
    pub id: GroupId,
    pub title: SharedString,
    pub members: Vec<ThreadRow>,
    /// Its triangle folded it: `▸`, the members hidden, the count kept.
    pub folded: bool,
    /// The board shows this Group (its view, or its wall): the row carries
    /// the tree's one selection, its `❯` and a `TEXT_STRONG` title.
    pub selected: bool,
}

/// One Thread's row — identical whether it is a Group member, a loose
/// Thread or a parked one; only the container differs. One line: cursor,
/// status mark, title, word.
#[derive(Clone)]
pub struct ThreadRow {
    pub thread: ThreadId,
    pub name: SharedString,
    /// What the Thread is doing right now — the one glance the operator
    /// asked for from the tree: which agents are working, which wait.
    pub status: RowStatus,
    /// The Project's name, from the facts cache: what a Project section is
    /// headed with.
    pub project: Option<SharedString>,
    /// The board shows this Thread alone (Solo): the row carries the
    /// tree's one selection, its `❯` and the `TEXT_STRONG` title. A Group
    /// member on a Group's board is not selected — its Group row is.
    pub selected: bool,
    /// The Thread finished while the operator looked elsewhere (an unread
    /// Notice). Its own axis, never a state; the nav draws it no
    /// differently (the bell and the Pane carry it).
    pub unread: bool,
    /// The one word at the row's right (C10).
    pub tail: NavTail,
}

/// What a row's word says (C10), in priority order: a pending Decision is
/// `needs you`; red tests `failing`/`failing N` and a failed turn or closed
/// Session `failed`; a working Thread its live elapsed time (`1m04s`,
/// `12s`, never coarsened); a finished turn `done`; otherwise the age, once
/// it reaches a minute (`facts::since_label` says nothing before that).
/// Never `now`. Every one is `TEXT_MUTED`: the dot carries the colour (the
/// prototype's `.nrow .s` outranks its `.warn`/`.bad` classes, so its words
/// read dim on screen).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavTail {
    NeedsYou,
    Failing(Option<u32>),
    Failed,
    Working(SharedString),
    Done,
    Age(SharedString),
    None,
}

impl NavTail {
    /// The word from the Pane's own reading of the Thread (`pane::thread_face`
    /// — the head's slot word), so the nav and the Pane never disagree: a
    /// waiting request is `needs you`, red tests `failing N`, a failed turn
    /// or closed Session `failed`, a working Thread its elapsed time, a
    /// finished turn `done`; everything else (idle, interrupted, parked) is
    /// its age.
    pub fn of(slot: Option<&crate::pane::HeadSlot>, age: SharedString) -> Self {
        use crate::pane::HeadSlot;
        match slot {
            Some(HeadSlot::NeedsYou(_)) => NavTail::NeedsYou,
            Some(HeadSlot::Failing(count)) => {
                NavTail::Failing(count.map(|count| count.min(u32::MAX as usize) as u32))
            }
            Some(HeadSlot::Failed) => NavTail::Failed,
            Some(HeadSlot::Working(elapsed)) if elapsed.is_empty() => NavTail::None,
            Some(HeadSlot::Working(elapsed)) => NavTail::Working(elapsed.clone().into()),
            Some(HeadSlot::Done) => NavTail::Done,
            _ => NavTail::Age(age),
        }
    }

    /// The word itself, from the lexicon (`theme::words`). An empty age says
    /// nothing.
    pub fn text(&self) -> Option<SharedString> {
        match self {
            NavTail::NeedsYou => Some(words::NEEDS_YOU.into()),
            NavTail::Failing(Some(count)) => Some(format!("{} {count}", words::FAILING).into()),
            NavTail::Failing(None) => Some(words::FAILING.into()),
            NavTail::Failed => Some(words::FAILED.into()),
            NavTail::Done => Some(words::DONE.into()),
            NavTail::Working(elapsed) | NavTail::Age(elapsed) if !elapsed.is_empty() => {
                Some(elapsed.clone())
            }
            NavTail::Working(_) | NavTail::Age(_) | NavTail::None => None,
        }
    }
}

/// A Thread row's state, for its dot. The face itself comes from
/// `cockpit::thread_status`, the one status truth the Panes share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RowStatus {
    Working,
    /// Working with a red test suite.
    Failing,
    /// A Decision waits on the operator.
    NeedsYou,
    /// The Session closed under it.
    Failed,
    #[default]
    Idle,
    Parked,
}

impl RowStatus {
    /// The row's state from the Pane's own reading. Done and Idle are one
    /// quiet row; unread is carried beside it (`ThreadRow::unread`).
    pub fn of(state: WallState) -> Self {
        match state {
            WallState::Working => RowStatus::Working,
            WallState::Failing => RowStatus::Failing,
            WallState::Decision => RowStatus::NeedsYou,
            WallState::Blocked => RowStatus::Failed,
            WallState::Parked => RowStatus::Parked,
            WallState::Idle | WallState::Done => RowStatus::Idle,
        }
    }

    /// The Pane state this row stands for, to read its face from
    /// `thread_status`.
    pub fn wall(self) -> WallState {
        match self {
            RowStatus::Working => WallState::Working,
            RowStatus::Failing => WallState::Failing,
            RowStatus::NeedsYou => WallState::Decision,
            RowStatus::Failed => WallState::Blocked,
            RowStatus::Idle => WallState::Idle,
            RowStatus::Parked => WallState::Parked,
        }
    }
}

/// Where a Thread row hangs: at root (a loose row, a parked row), or under
/// its Group on a tree glyph — `├ `, or `└ ` on the Group's last member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowPlace {
    Root,
    Member { last: bool },
}

// ------------------------------------------------------------- the grid

/// The width a title box gets: the row's text width less `cells` 2ch mark
/// cells and the word at its right (its characters and the 1ch before
/// it). One monospace face, so every advance is one `CH`, and at the
/// 36ch column every box is whole cells.
fn title_w(cells: f32, word: Option<&str>) -> f32 {
    (NAV_TEXT_W - cells * NAV_CELL - word_w(word)).max(0.0)
}

/// The room a right-hand word takes: its cells and the gap before it.
fn word_w(word: Option<&str>) -> f32 {
    word.map_or(0.0, |word| word.chars().count() as f32 * CH + NAV_WORD_GAP)
}

/// A title pinned to `width`, truncating with an ellipsis.
///
/// A truncating line needs a **definite** width on its very first measure.
/// gpui caches a nowrap line's first measure permanently (`wrap_width` is
/// `None` for nowrap, so the early return fires on every later call), and
/// taffy only hands a text leaf a definite width when the leaf's flex
/// container is a **column** whose own available width is definite — which
/// taffy derives from the child's own min/max width. A `flex_1`, a `w_full`
/// or even a `w(px(..))` cell is measured at max-content first, so
/// `truncate_line` never runs and the line is only visually clipped.
/// Hence: flex **column**, with min and max width pinned to the box.
fn fitted(width: f32, ink: u32, weight: FontWeight, title: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink(1.)
        .min_w_0()
        .w(px(width))
        .child(
            div().h(px(NAV_LINE)).overflow_hidden().child(
                div()
                    .flex()
                    .flex_col()
                    .min_w(px(width + TRUNCATE_SLOP))
                    .max_w(px(width + TRUNCATE_SLOP))
                    .truncate()
                    .h(px(NAV_LINE))
                    .text_size(px(FS_UI))
                    .font_weight(weight)
                    .line_height(px(NAV_LINE))
                    .text_color(rgb(ink))
                    .child(title),
            ),
        )
}

/// A 2ch mark cell, its glyph typed at its start (the prototype's `.dot`,
/// `.cur`), the second cell being its space.
fn cell(mark: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(NAV_CELL))
        .h(px(NAV_LINE))
        .child(mark)
}

/// An empty 2ch cell: the cursor cell of a row that is not selected.
fn blank_cell() -> Div {
    div().flex_shrink_0().w(px(NAV_CELL)).h(px(NAV_LINE))
}

/// The cursor cell: `❯` in the accent on the selected row, blank otherwise.
fn cursor_cell(selected: bool) -> Div {
    if selected {
        cell(components::prompt_mark(ACCENT))
    } else {
        blank_cell()
    }
}

/// A member's tree glyph, typed (box drawing is in Geist Mono): `├ `, or
/// `└ ` closing the Group. Structure, so the faint ink.
fn tree_cell(last: bool) -> Div {
    div()
        .flex_shrink_0()
        .w(px(NAV_CELL))
        .h(px(NAV_LINE))
        .text_color(rgb(TEXT_FAINT))
        .child(if last { "\u{2514}" } else { "\u{251c}" })
}

/// A member's tree lead as the prototype types it, one run: `  ├ `, or
/// `  └ ` closing the Group, in the faint structure ink.
fn tree_lead(last: bool) -> Div {
    components::glyph(
        if last {
            "  \u{2514} "
        } else {
            "  \u{251c} "
        },
        TEXT_FAINT,
    )
    .whitespace_nowrap()
    .h(px(NAV_LINE))
}

/// The disclosure triangle and its space, typed as the prototype's `.tr`
/// types them — `▾ ` open, `▸ ` shut, in the faint structure ink, the
/// triangle in the face the platform falls back to — the run's own width
/// before the title.
fn disclosure(open: bool) -> Div {
    components::glyph(if open { "\u{25be} " } else { "\u{25b8} " }, TEXT_FAINT)
        .whitespace_nowrap()
        .h(px(NAV_LINE))
}

/// The word at a row's right, 1ch after its title, in the metadata ink. It
/// takes whatever the title leaves, so every row's word ends on one edge.
fn word_cell(word: Option<SharedString>) -> Div {
    div()
        .flex()
        .flex_grow(1.)
        .flex_shrink_0()
        .justify_end()
        .h(px(NAV_LINE))
        .when(word.is_some(), |cell| cell.pl(px(NAV_WORD_GAP)))
        .text_color(rgb(TEXT_MUTED))
        // Exactly its cells: the word is right-aligned, and a measured run
        // rounds up a pixel and would stand it that much left.
        .children(word.map(components::cells))
}

/// One line of the column at its inline padding: the frame every row,
/// heading and header shares.
fn line() -> Div {
    div()
        .relative()
        .flex()
        .flex_row()
        .items_center()
        .flex_shrink_0()
        .h(px(NAV_LINE))
        .px(px(NAV_PAD_X))
        .whitespace_nowrap()
        .text_size(px(FS_UI))
        .line_height(px(NAV_LINE))
}

/// The nav's one pointer role (the prototype's `.nrow:hover`): nothing at
/// rest, `paint::HOVER` under the pointer reaching full in 80ms, and the
/// selected row its `paint::SELECTION` whatever the pointer does. No press
/// tint and no cursor change: a nav line is a terminal line, so the pointer
/// stays the arrow over every one of them.
fn wash<E: Styled + InteractiveElement>(mut element: E, key: SharedString, selected: bool) -> E {
    if selected {
        element = element.bg(paint::SELECTION);
    } else {
        let t = motion::hover_t(&key);
        if t > 0.0 {
            element = element.bg(motion::mix(
                rgba(TRANSPARENT).into(),
                paint::HOVER.into(),
                t,
            ));
        }
    }
    element
        .interactivity()
        .on_hover(motion::hover_listener_with(key, ROW_HOVER));
    element
}

/// The frame every tree row shares: one line, square, washed under the
/// pointer (`wash`). The height is fixed so a row that cannot resolve its
/// Project or its checkout still occupies exactly the space it will once
/// the cache fills. `key` names its blend; a Thread listed under two
/// Groups gets one per listing.
fn row_frame(id: (&'static str, usize), key: SharedString, selected: bool) -> Stateful<Div> {
    wash(line().id(id), key, selected)
}

// ------------------------------------------------------------- the marks

/// The status mark in a Thread row's mark cell (`thread_status`, the one
/// truth the Panes share): a working Thread's braille spinner in `RUNNING`
/// — the one thing in the column that moves, still under reduced motion —
/// a failing or failed Thread's red dot, a Decision's yellow, idle the
/// metadata ink (read or not), and a parked Thread a faint ring.
fn status_mark(row: &ThreadRow) -> AnyElement {
    match row.status {
        RowStatus::Working => components::braille_spinner(RUNNING),
        RowStatus::Parked => components::status_ring(TEXT_FAINT).into_any_element(),
        _ => dot_face(row).into_any_element(),
    }
}

/// The still face of a row: the dot alone — the same face the Thread's Pane
/// draws.
fn dot_face(row: &ThreadRow) -> Div {
    thread_status(row.status.wall(), row.unread).dot()
}

// ------------------------------------------------------------- the column

/// The nav column itself: full height, `width` wide (the caller rides it
/// on cmd-B), clipped, so its content keeps `WIDTH` (`content`) and nothing
/// reflows while it moves. It paints no ground of its own: `body` lays the
/// chrome under the titlebar band, which paints its own (`chrome_band`).
pub fn shell(width: f32) -> Div {
    div()
        .debug_selector(|| "nav-column".into())
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(width))
        .overflow_hidden()
        .font_family(FONT_UI)
        .text_size(px(FS_UI))
        .line_height(px(NAV_LINE))
        .text_color(rgb(TEXT))
}

/// The column's ground: `paint::CHROME` from the titlebar band down — the
/// band over it paints its own, so the two never stack two layers of glass.
pub fn body() -> Div {
    div()
        .debug_selector(|| "nav-body".into())
        .absolute()
        .top(px(WIN_CHROME_H))
        .bottom_0()
        .left_0()
        .right_0()
        .bg(paint::CHROME)
}

/// The column's content at its own width, under the titlebar band and a
/// half row of air, a half row clear of the foot.
pub fn content() -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(WIDTH))
        .pt(px(WIN_CHROME_H + NAV_PAD_Y))
        .pb(px(NAV_PAD_Y))
}

/// The one seam column, right of the nav at its own x (the prototype's
/// `.vline`): `paint::CHROME_SEAM` — a dark seam on glass, nothing on the
/// opaque greys — with the nav open or riding, and the reading plane once
/// it has folded (`.collapsed .vline`), so a folded nav leaves nothing
/// drawn at x = 0. It never darkens the nav's own last column.
pub fn seam(folded: bool) -> Div {
    div()
        .debug_selector(|| "nav-seam".into())
        .flex_shrink_0()
        .h_full()
        .w(px(CHROME_SEAM_W))
        .bg(if folded {
            paint::PLANE
        } else {
            paint::CHROME_SEAM
        })
}

/// How wide the titlebar cell over the column is while the column is
/// `column` wide: the column itself when open (36ch), riding down to the
/// folded cell (`NAV_CHROME_FOLDED_W`, 20ch, plus `extra_doors` icon doors)
/// as it folds. The titlebar strip over the board starts where it ends.
pub fn chrome_width(column: f32, extra_doors: usize) -> f32 {
    let folded = NAV_CHROME_FOLDED_W + extra_doors as f32 * ICON_BUTTON;
    let open = (column / WIDTH).clamp(0.0, 1.0);
    folded + (WIDTH - folded) * open
}

/// The ground of the titlebar cell `t` of the way from open to folded: the
/// chrome cross-fading as one colour to the reading plane (`.tbl`'s
/// background transition) — never a positional wipe.
pub fn band_ground(t: f32) -> Hsla {
    motion::mix(paint::CHROME.into(), paint::PLANE.into(), t)
}

/// The titlebar cell over the column: `width` wide (`chrome_width`), one
/// band of `WIN_CHROME_H` at the window's top-left, laid over everything
/// under it, on `ground` (`band_ground`), with the 1px seam carried up
/// through the titlebar row at its right edge (`seam`'s colours; `folded`
/// paints it the plane).
///
/// On macOS the traffic lights are the **host's**, positioned by
/// `TitlebarOptions`, so the band reserves their room rather than drawing
/// fakes: a `NAV_CHROME_LEAD`-wide spacer that holds nothing — anything
/// drawn or hit-testable in that strip kills AppKit's drag region. Where
/// the app draws its own titlebar, the lead is one cell. The caller hangs
/// the doors after it: a stretch, then the sidebar toggle and the bell (and
/// the gear off macOS), 1ch clear of the cell's right edge.
pub fn chrome_band(width: f32, ground: Hsla, folded: bool) -> Div {
    div()
        .debug_selector(|| "nav-chrome".into())
        .absolute()
        .top_0()
        .left_0()
        .w(px(width))
        .h(px(WIN_CHROME_H))
        .flex()
        .flex_row()
        .items_center()
        .pr(px(NAV_PAD_X))
        .font_family(FONT_UI)
        .bg(ground)
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(width))
                .w(px(CHROME_SEAM_W))
                .bg(if folded {
                    paint::PLANE
                } else {
                    paint::CHROME_SEAM
                }),
        )
        .child(div().flex_shrink_0().w(px(NAV_CHROME_LEAD)))
}

/// The sidebar toggle (the prototype's `#b-side`): the sidebar glyph at
/// `ICON_BUTTON_GLYPH` in a 28×24 titlebar door, centred in the band — the
/// same door open or folded; the cell rides, the door does not change.
/// Under the pointer the ground takes `paint::HOVER` and the glyph `TEXT`,
/// both over 100ms (`.ib:hover`).
pub fn collapse_button() -> Stateful<Div> {
    let key = SharedString::from(COLLAPSE_KEY);
    let t = motion::hover_t(&key);
    div()
        .id((COLLAPSE_KEY, 0usize))
        .debug_selector(|| COLLAPSE_KEY.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON_H))
        .rounded(px(R_CONTROL))
        .cursor_pointer()
        .when(t > 0.0, |door| {
            door.bg(motion::mix(
                rgba(TRANSPARENT).into(),
                paint::HOVER.into(),
                t,
            ))
        })
        .on_hover(motion::hover_listener_with(key, DOOR_HOVER))
        .tooltip(crate::menu::action_tooltip(
            "Toggle sidebar",
            "cockpit::ToggleNav",
        ))
        .child(
            icon(icons::SIDEBAR, ICON_BUTTON_GLYPH, TEXT_MUTED).text_color(motion::mix(
                rgb(TEXT_MUTED).into(),
                rgb(TEXT).into(),
                t,
            )),
        )
}

// ------------------------------------------------------------- the tree

/// The scrolling tree. It is the only thing in the column that scrolls;
/// its rows reach the column's edges and carry their own inline padding.
pub fn nav_tree(scroll: &ScrollHandle) -> Stateful<Div> {
    div()
        .id(("nav-tree", 0usize))
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(scroll)
}

/// The nav tree's scrollbar, over the tree it scrolls. See
/// [`components::scrollbar`] for the shape and the sibling rule.
pub fn scrollbar(scroll: &ScrollHandle) -> Div {
    components::scrollbar("nav-scrollbar", scroll)
}

/// One Project section: its heading, then everything under it, a half row
/// below the section above it (the prototype's `.half`).
pub fn section(first: bool) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .when(!first, |section| section.mt(px(NAV_SECTION_GAP)))
}

/// One Group block: the parent row, then its members, flush with whatever
/// is above it — the tree glyphs draw the block. Its own id scopes its rows,
/// so a Thread listed under two Groups is two distinct rows.
pub fn group_block(id: GroupId) -> Stateful<Div> {
    div()
        .id(("nav-group-block", id.get() as usize))
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
}

/// "Insert this Group before that one": the blocks sit flush, so the
/// target is an absolute `NAV_DROP_BAND` hit band over the top edge of the
/// block's header, taking no layout and, without `occlude`, stealing none
/// of its clicks. `index` is the Group's place in the durable order. The
/// caller lays it **after** the block's rows, so it is the topmost hitbox
/// there.
pub fn group_gap(index: usize) -> Stateful<Div> {
    div()
        .id(("group-gap", index))
        .debug_selector(move || format!("group-gap-{index}"))
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(NAV_DROP_BAND))
}

/// "Append after the last member", by the same trick: members sit flush,
/// so the target is an absolute `NAV_DROP_BAND` hit band over the last
/// row's foot. A drop between two members lands on the row it is over.
pub fn member_tail(id: GroupId) -> Stateful<Div> {
    div()
        .id(("member-tail", id.get() as usize))
        .debug_selector(move || format!("member-tail-{}", id.get()))
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
        .h(px(NAV_DROP_BAND))
}

/// The Group parent row, one line: the cursor cell (`❯` when the board
/// shows this Group), its triangle (`group_fold`), the title in body ink
/// (`TEXT_STRONG` when selected), and how many members it holds, dim, at
/// the right — the same count folded or open. No logo, and no `needs you`
/// of its own: the member that waits says so.
#[cfg(test)]
pub fn group_row(row: &GroupBlock) -> Stateful<Div> {
    group_row_with_title(row, row.title.clone(), group_fold(row))
}

/// `group_row` with the title leaf and the triangle supplied by the caller
/// — the cockpit hands in a click-to-rename wrapper, or the live editor
/// while renaming, and wires the triangle's press. The title box is pinned
/// (`fitted`), so the title truncates and the editor inherits its 20px
/// line: renaming never moves the row.
pub fn group_row_with_title(
    row: &GroupBlock,
    title: impl IntoElement,
    fold: Stateful<Div>,
) -> Stateful<Div> {
    let count = SharedString::from(row.members.len().to_string());
    let id = row.id;
    let key = SharedString::from(format!("nav-group-{}", id.get()));
    row_frame(("nav-group", id.get() as usize), key, row.selected)
        .debug_selector(move || format!("nav-group-{}", id.get()))
        .child(cursor_cell(row.selected))
        .child(fold)
        .child(fitted(
            title_w(2.0, Some(&count)),
            title_ink(row.selected, false),
            W_BODY,
            title,
        ))
        .child(
            word_cell(Some(count)).debug_selector(move || format!("nav-group-count-{}", id.get())),
        )
}

/// A Group row's triangle, in its mark cell: `▾` open, `▸` folded. A
/// control of its own — the cockpit wires its press to the fold, which
/// stops there, so the rest of the row still opens the Group.
pub fn group_fold(row: &GroupBlock) -> Stateful<Div> {
    let id = row.id;
    disclosure(!row.folded)
        .id(("nav-group-fold", id.get() as usize))
        .debug_selector(move || format!("nav-group-fold-{}", id.get()))
}

/// The members container: the member rows, flush under their Group, each
/// carrying its own tree glyph (`RowPlace::Member`).
pub fn members(rows: Vec<AnyElement>) -> Div {
    div().relative().flex().flex_col().children(rows)
}

/// The Thread row, one line, at root — see `thread_row_with_title`.
#[cfg(test)]
pub fn thread_row(row: &ThreadRow) -> Stateful<Div> {
    thread_row_with_title(row, row.name.clone(), RowPlace::Root, None, false)
}

/// The one Thread row builder, with the title leaf supplied by the caller —
/// see `group_row_with_title`. One line: the cursor cell (`❯` when
/// selected), a member's tree glyph (`place`: `├` at 3ch, the dot at 5ch,
/// the title at 7ch), the status mark, the title pinned to what the row
/// leaves it, and the word at the right. `group` scopes the row's blend
/// when the Thread is listed under several Groups. `editing` is a rename
/// in progress: the field takes the title's box and the word's.
pub fn thread_row_with_title(
    row: &ThreadRow,
    title: impl IntoElement,
    place: RowPlace,
    group: Option<GroupId>,
    editing: bool,
) -> Stateful<Div> {
    let thread = row.thread;
    let word = (!editing).then(|| row.tail.text()).flatten();
    let cells = match place {
        RowPlace::Root => 2.0,
        RowPlace::Member { .. } => 3.0,
    };
    let key = SharedString::from(format!(
        "nav-thread-{}-{}",
        group.map_or(0, GroupId::get),
        thread.get()
    ));
    let frame = row_frame(("nav-thread", thread.get() as usize), key, row.selected)
        .debug_selector(move || format!("nav-thread-{}", thread.get()));
    // A member's lead is the prototype's typed `  ├ ` (one run); the
    // cursor takes its first two cells when the row is selected.
    let frame = match place {
        RowPlace::Root => frame.child(cursor_cell(row.selected)),
        RowPlace::Member { last } if row.selected => frame
            .child(cursor_cell(true))
            .child(tree_cell(last)),
        RowPlace::Member { last } => frame.child(tree_lead(last)),
    };
    frame
        .child(cell(status_mark(row)))
        .child(
            fitted(
                title_w(cells, word.as_deref()),
                title_ink(row.selected, row.status == RowStatus::Parked),
                W_BODY,
                title,
            )
            .debug_selector(move || format!("nav-title-{}", thread.get())),
        )
        .children(
            (!editing).then(|| {
                word_cell(word).debug_selector(move || format!("nav-since-{}", thread.get()))
            }),
        )
}

/// One row of the Needs-you strip: the Thread's dot in `ATTENTION`, its
/// title, and what it waits for (`approval` / `question`), dim. It is a
/// reference to the Thread, not a second copy of it: no cursor cell, no
/// selection, and a press lands on the Thread like its own row does.
pub fn needs_you_row(entry: &NeedsYouRow) -> Stateful<Div> {
    let row = &entry.row;
    let thread = row.thread;
    let key = SharedString::from(format!("nav-needs-{}", thread.get()));
    row_frame(("nav-needs", thread.get() as usize), key, false)
        .debug_selector(move || format!("nav-needs-{}", thread.get()))
        .child(cell(components::status_dot(ATTENTION)))
        .child(fitted(
            title_w(1.0, Some(entry.kind)),
            TEXT,
            W_BODY,
            row.name.clone(),
        ))
        .child(word_cell(Some(entry.kind.into())))
}

/// The strip's header, the first line under the titlebar: `needs you N`,
/// dim, and `⌘D` — the key that answers it — dim at the right, as its
/// keycap reads (`key_combo`). It washes under the pointer like any row.
pub fn needs_you_header(count: usize) -> Stateful<Div> {
    let key = components::bound_chord("cockpit::NextDecision")
        .map(|keys| components::key_combo(&keys, TEXT_MUTED));
    row_frame(("nav-needs-you", 0), "nav-needs-you".into(), false)
        .debug_selector(|| "nav-needs-you".into())
        .text_color(rgb(TEXT_MUTED))
        .child(SharedString::from(format!("{} {count}", words::NEEDS_YOU)))
        .children(key.map(|key| div().flex().ml_auto().child(key)))
}

/// The strip: its header and one row per waiting Thread, pinned at the top
/// of the column (it does not scroll with the tree), a half row above the
/// tree.
pub fn needs_you_strip() -> Div {
    div()
        .debug_selector(|| "nav-needs-you-strip".into())
        .flex()
        .flex_col()
        .flex_shrink_0()
        .pb(px(NAV_SECTION_GAP))
}

/// A title's ink. Every unselected title is the body ink, read or unread —
/// unread is the bell's and the Pane's to say — and only the selected row's
/// reaches `TEXT_STRONG`. A parked Thread's steps down to the metadata
/// ink, so what is running reads first. Ink only: the weight is always
/// `W_BODY`, so a row never reflows.
fn title_ink(selected: bool, parked: bool) -> u32 {
    if selected {
        TEXT_STRONG
    } else if parked {
        TEXT_MUTED
    } else {
        TEXT
    }
}

// ------------------------------------------------------------- Projects

/// A Project heading, one line: its triangle where a row's cursor sits
/// (`▾` open, `▸` folded), the Project's name in `W_STRONG` `TEXT_STRONG`,
/// and its branch, dim, at the right — still there under the pointer. A
/// plain row: it washes like any other, and the cockpit wires its press to
/// the fold. A new Thread in this Project is a palette command.
pub fn project_section(
    index: usize,
    label: SharedString,
    branch: Option<SharedString>,
    folded: bool,
) -> Stateful<Div> {
    let branch = branch.map(|branch| short_branch(&branch));
    let key = SharedString::from(format!("nav-project-section-{index}"));
    row_frame(("nav-project-section", index), key, false)
        .debug_selector(move || format!("nav-project-section-{index}"))
        .child(disclosure(!folded))
        .child(fitted(
            title_w(1.0, branch.as_deref()),
            TEXT_STRONG,
            W_STRONG,
            label,
        ))
        .child(word_cell(branch))
}

/// A branch held to `NAV_BRANCH_MAX_CH` cells, an ellipsis standing for the
/// rest — mono, so the cut is exact without measuring.
fn short_branch(branch: &str) -> SharedString {
    if branch.chars().count() <= NAV_BRANCH_MAX_CH {
        return branch.to_string().into();
    }
    let kept: String = branch.chars().take(NAV_BRANCH_MAX_CH - 1).collect();
    format!("{kept}\u{2026}").into()
}

// ------------------------------------------------------------- loose rows and empties

/// One section's loose rows — those no Group claims as home — at root: a
/// place to drop a row to get it out of its Group, each with its own id.
pub fn solos(index: usize, rows: Vec<AnyElement>) -> Stateful<Div> {
    div()
        .id(("loose-zone", index))
        .debug_selector(move || {
            if index == 0 {
                "loose-zone".into()
            } else {
                format!("loose-zone-{index}")
            }
        })
        .flex()
        .flex_col()
        .flex_shrink_0()
        .children(rows)
}

/// The empty ground under the last row: the tree's own remainder, and the
/// drop target that gets a row out of its Group when every Thread is in
/// one and there is no loose run to aim at.
pub fn loose_ground(index: usize) -> Stateful<Div> {
    div()
        .id(("loose-zone", index))
        .debug_selector(move || {
            if index == 0 {
                "loose-zone".into()
            } else {
                format!("loose-zone-{index}")
            }
        })
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(NAV_LINE))
}

/// What an empty tree says, on the rows' title column, sentence case and
/// no full stop: a line in `TEXT_MUTED` and, where there is one, a way
/// forward in the same voice. Filtered to a Project it names the Project
/// rather than shrugging; when the Parked section below holds Threads the
/// filter admits, it says *open* and points below, so the operator is not
/// told a tree is empty while its Threads sit one fold away. An empty store
/// offers the key that starts one.
pub fn empty_filter(project: Option<&str>, parked_below: bool) -> Div {
    let message = match (project, parked_below) {
        (Some(project), _) => format!("No open threads in {project}"),
        (None, false) => "No threads yet".to_string(),
        (None, true) => "No open threads".to_string(),
    };
    let hint: Option<AnyElement> = match (project, parked_below) {
        (_, true) => Some(
            components::text_meta()
                .child("Parked threads below")
                .into_any_element(),
        ),
        (None, false) => components::bound_chord("cockpit::NewThread").map(|keys| {
            components::text_meta()
                .flex()
                .items_center()
                .gap(px(CH))
                .child(components::key_combo(&keys, TEXT_MUTED))
                .child("new thread")
                .into_any_element()
        }),
        (Some(_), false) => None,
    };
    div()
        .debug_selector(|| "nav-empty".into())
        .flex()
        .flex_col()
        .flex_shrink_0()
        .pl(px(NAV_PAD_X + 2.0 * NAV_CELL))
        .pr(px(NAV_PAD_X))
        .child(
            components::text_ui()
                .text_color(rgb(TEXT_MUTED))
                .child(SharedString::from(message)),
        )
        .children(hint)
}

/// A refusal from the last Group change, at the top of the tree, until the
/// next change succeeds: a `BLOCKED` dot in the mark cell and the refusal,
/// dim, on the title column, wrapping. Colour on the dot, never the row.
pub fn notice(text: SharedString) -> Div {
    components::text_meta()
        .debug_selector(|| "nav-notice".into())
        .flex()
        .flex_shrink_0()
        .items_start()
        .px(px(NAV_PAD_X))
        .child(blank_cell())
        .child(cell(components::status_dot(BLOCKED)))
        .child(div().flex_1().min_w_0().child(text))
}

// ------------------------------------------------------------- Parked

/// The Parked section at the foot of the column, under the scrolling tree
/// rather than inside it: its header stays in reach however long the tree
/// grows. It is capped at half the column — the list inside scrolls past
/// that, so unfolding it never hides the running Threads above.
pub fn parked_section() -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .max_h(relative(NAV_PARKED_MAX_SHARE))
}

/// The Parked section's header, one dim line: the faint `▸` where a
/// heading's triangle sits, then `parked N`. A control in a heading's
/// clothes: the press toggles the fold, and a right press offers the
/// section's own menu (the cockpit wires both). Under the pointer only its
/// ground changes.
pub fn parked_header(count: usize, open: bool, eased: bool) -> Stateful<Div> {
    // `▸ ` shut, `▾ ` open: typed, as the prototype's `.tr` types it. A
    // pointer toggle turns it over the quarter turn's 150ms (the triangle
    // changes halfway), a keyboard or menu toggle at once (rule 2.10.5).
    let triangle = |turn: f32| disclosure(turn >= 0.5);
    let mark = if eased {
        crate::motion::settled("nav-parked-chevron", open, crate::motion::TURN, triangle)
            .into_any_element()
    } else {
        triangle(if open { 1. } else { 0. }).into_any_element()
    };
    row_frame(("nav-parked", 0), "nav-parked".into(), false)
        .debug_selector(|| "nav-parked".into())
        .text_color(rgb(TEXT_MUTED))
        .child(mark)
        .child(
            div()
                .min_w_0()
                .truncate()
                .child(SharedString::from(format!("{} {count}", words::PARKED))),
        )
}

/// The unfolded Parked list. It scrolls on its own handle — the tree's
/// scroll must not move when the operator wheels through parked rows —
/// and shrinks to the section's cap rather than growing past it.
pub fn parked_list(scroll: &ScrollHandle) -> Stateful<Div> {
    div()
        .id(("nav-parked-list", 0usize))
        .debug_selector(|| "nav-parked-list".into())
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(scroll)
}

/// The Parked list's scrollbar — its own id, because the toolkit keys a
/// bar's state off it and the tree's bar is already `nav-scrollbar`.
pub fn parked_scrollbar(scroll: &ScrollHandle) -> Div {
    components::scrollbar("nav-parked-scrollbar", scroll)
}

// ------------------------------------------------------------- rename and drag

/// A Group title that can be renamed: the row's own title text, and
/// nothing else. It wears **no** hover wash — the wash would advertise a
/// control the single click no longer operates, and a title box lighting
/// up inside an already-hovered row reads as a second target where there
/// is one. The double click is the affordance; the row is the control.
pub fn rename_target_group(id: GroupId, title: SharedString) -> Stateful<Div> {
    div()
        .id(("rename-group", id.get() as usize))
        .debug_selector(move || format!("rename-group-{}", id.get()))
        .min_w_0()
        .truncate()
        .child(title)
}

/// A Thread title that can be renamed — `rename_target_group`'s twin, on
/// the Thread row, and equally unwashed.
pub fn rename_target_thread(thread: ThreadId, title: SharedString) -> Stateful<Div> {
    div()
        .id(("rename-thread", thread.get() as usize))
        .debug_selector(move || format!("rename-thread-{}", thread.get()))
        .min_w_0()
        .truncate()
        .child(title)
}

/// A nav row lifted off the tree while it is dragged: the same row the
/// tree draws, at the column's width, on the float ground with its edge and
/// the one float shadow — so what rides the pointer is recognisably the
/// row that was picked up.
pub fn drag_row(row: Stateful<Div>) -> Div {
    div()
        .w(px(WIDTH))
        .font_family(FONT_UI)
        .text_size(px(FS_UI))
        .line_height(px(NAV_LINE))
        .text_color(rgb(TEXT))
        .bg(paint::FLOAT)
        .border_1()
        .border_color(paint::LINE2)
        .shadow(crate::components::float_shadow())
        .child(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::ThreadId;

    fn thread() -> ThreadRow {
        selected_thread(false)
    }

    fn selected_thread(selected: bool) -> ThreadRow {
        ThreadRow {
            thread: ThreadId::new(8),
            status: RowStatus::Idle,
            name: "thread-08".into(),
            project: Some("ferrite".into()),
            selected,
            unread: false,
            tail: NavTail::Age("2h".into()),
        }
    }

    fn group(selected: bool, folded: bool) -> GroupBlock {
        GroupBlock {
            id: GroupId::new(1),
            title: "Perf sweep".into(),
            members: vec![thread(), thread(), thread(), thread()],
            folded,
            selected,
        }
    }

    /// The terminal grammar: square rows with no radius, every item one
    /// 20px line 1ch in from the column's edge, and a Project section a
    /// half row below the one above it.
    #[test]
    fn nav_rows_are_square_terminal_lines() {
        let mut row = thread_row(&thread());
        let style = row.style();
        assert_eq!(style.corner_radii.top_left, None, "square: no radius");
        assert_eq!(style.size.height, Some(px(NAV_LINE).into()));
        assert_eq!(style.padding.left, Some(px(NAV_PAD_X).into()), "1ch in");
        let mut later = section(false);
        assert_eq!(
            later.style().margin.top,
            Some(px(NAV_SECTION_GAP).into()),
            "a half row above every Project section but the first"
        );
        assert_eq!(section(true).style().margin.top, None);
        assert_eq!(NAV_LINE, ROW);
        assert_eq!(NAV_SECTION_GAP, HALF_ROW);
        assert_eq!(NAV_PAD_X, CH);
        assert_eq!(NAV_CELL, 2.0 * CH);
        assert_eq!(WIDTH, 36.0 * CH, "the prototype's 36ch column");
    }

    /// N-5, the selection rule: one selection, on the row of what the board
    /// shows. A Group's board selects its Group row — `❯`, the selection
    /// fill and a `TEXT_STRONG` title — and none of its members; Solo
    /// selects the Thread's own row.
    #[test]
    fn one_selection_marks_what_the_board_shows() {
        let fill = |mut drawn: Stateful<Div>| drawn.style().background.clone();
        let selection = Some(gpui::Hsla::from(paint::SELECTION).into());
        assert_eq!(fill(group_row(&group(true, false))), selection);
        assert_eq!(fill(group_row(&group(false, false))), None);
        assert_eq!(
            fill(thread_row_with_title(
                &thread(),
                "thread-08",
                RowPlace::Member { last: false },
                Some(GroupId::new(1)),
                false
            )),
            None,
            "a member on its Group's board carries no fill"
        );
        assert_eq!(fill(thread_row(&selected_thread(true))), selection);
        assert_eq!(fill(thread_row(&thread())), None);
        let entry = NeedsYouRow {
            row: selected_thread(true),
            kind: words::APPROVAL,
        };
        assert_eq!(
            fill(needs_you_row(&entry)),
            None,
            "a strip row is a reference, never the selection"
        );
    }

    /// N-6: every unselected title is the body ink, read or unread; only
    /// the selected row reaches `TEXT_STRONG`; a parked title steps down.
    #[test]
    fn titles_are_body_ink_but_the_selected_and_the_parked() {
        assert_eq!(title_ink(false, false), TEXT);
        assert_eq!(title_ink(true, false), TEXT_STRONG);
        assert_eq!(title_ink(false, true), TEXT_MUTED);
        assert_eq!(
            title_ink(true, true),
            TEXT_STRONG,
            "the selection outranks the step-down"
        );
        assert_eq!(TEXT, 0xd7d7d9, "the prototype's --fg");
    }

    /// N-13: a nav line is a terminal line — the pointer stays the arrow
    /// over every row, heading and header, draggable or not.
    #[test]
    fn every_nav_line_keeps_the_arrow() {
        let cursor = |mut drawn: Stateful<Div>| drawn.style().mouse_cursor;
        assert_eq!(cursor(thread_row(&thread())), None);
        assert_eq!(cursor(thread_row(&selected_thread(true))), None);
        assert_eq!(cursor(group_row(&group(false, false))), None);
        assert_eq!(cursor(group_row(&group(true, true))), None);
        assert_eq!(cursor(parked_header(3, false, false)), None);
        assert_eq!(cursor(needs_you_header(2)), None);
        assert_eq!(
            cursor(project_section(0, "ferrite".into(), None, false)),
            None
        );
    }

    /// N-4: a folded Group keeps its row and its count; only its triangle
    /// turns. The triangle is its own control.
    #[test]
    fn a_folded_group_keeps_its_count() {
        for folded in [false, true] {
            let block = group(false, folded);
            assert_eq!(block.members.len(), 4);
            let mut row = group_row(&block);
            assert_eq!(row.style().size.height, Some(px(NAV_LINE).into()));
        }
        // The triangle and its space are typed: the run's own width.
        let mut fold = group_fold(&group(false, true));
        assert_eq!(fold.style().text.color, Some(rgb(TEXT_FAINT).into()));
    }

    /// A row whose Project or checkout has not resolved keeps its line: the
    /// caches fill asynchronously, and the tree must not jump under the
    /// pointer when they do. Every kind of row is the one 20px line.
    #[test]
    fn every_row_is_one_line_whatever_its_facts() {
        let bare = ThreadRow {
            thread: ThreadId::new(9),
            status: RowStatus::Idle,
            name: "thread-09".into(),
            project: None,
            selected: false,
            unread: false,
            tail: NavTail::None,
        };
        let height = |mut drawn: Stateful<Div>| drawn.style().size.height;
        let line = Some(px(NAV_LINE).into());
        assert_eq!(height(thread_row(&bare)), line);
        assert_eq!(height(thread_row(&thread())), line);
        assert_eq!(height(thread_row(&selected_thread(true))), line);
        for tail in [
            NavTail::NeedsYou,
            NavTail::Failing(Some(12)),
            NavTail::Failed,
            NavTail::Done,
            NavTail::Working("1m04s".into()),
        ] {
            assert_eq!(
                height(thread_row(&ThreadRow {
                    tail: tail.clone(),
                    ..bare.clone()
                })),
                line,
                "{tail:?}: a word arriving is the same line"
            );
        }
        for status in [RowStatus::Working, RowStatus::Failing, RowStatus::Parked] {
            assert_eq!(
                height(thread_row(&ThreadRow {
                    status,
                    ..bare.clone()
                })),
                line,
                "{status:?}: a spinner or a ring is the same line"
            );
        }
        assert_eq!(height(group_row(&group(false, false))), line);
        assert_eq!(
            height(thread_row_with_title(
                &bare,
                "thread-09",
                RowPlace::Member { last: false },
                Some(GroupId::new(1)),
                false
            )),
            line
        );
        let entry = NeedsYouRow {
            row: bare.clone(),
            kind: words::QUESTION,
        };
        assert_eq!(height(needs_you_row(&entry)), line);
        assert_eq!(height(needs_you_header(1)), line);
        assert_eq!(height(parked_header(3, false, false)), line);
        assert_eq!(height(parked_header(3, true, false)), line);
        assert_eq!(
            height(project_section(0, "ferrite".into(), None, false)),
            line
        );
        assert_eq!(
            height(project_section(
                0,
                "ferrite".into(),
                Some("dev".into()),
                true
            )),
            line
        );
    }

    /// The characters gpui keeps of `chars` monospace characters cut to a
    /// box of `cells` cells (gpui's own rule, `should_truncate_line`, at one
    /// `CH` per glyph): the whole title when it fits, else the prefix whose
    /// width and the ellipsis's stay strictly under the budget.
    fn kept(chars: usize, cells: f32) -> usize {
        let budget = cells * CH + TRUNCATE_SLOP;
        if (chars as f32 * CH).floor() <= budget {
            return chars;
        }
        let mut width = 0.0;
        let mut kept = 0;
        for index in 0..chars {
            if width + CH < budget {
                kept = index;
            }
            width += CH;
        }
        kept
    }

    /// N-7: the title box is pinned to what its row leaves it — whole cells
    /// at the 36ch column — so a cut title ends in a whole `…` exactly one
    /// cell before its word, every row's word ending on the one right edge:
    /// `Nav rows jitter on strea… done`.
    #[test]
    fn a_cut_title_ends_in_a_whole_ellipsis_one_cell_before_its_word() {
        let close = |left: f32, right: f32| (left - right).abs() < 1e-3;
        assert!(close(NAV_TEXT_W, 34.0 * CH));
        let solo = title_w(2.0, Some("done"));
        assert!(
            close(solo, 25.0 * CH),
            "the solo row: 34 less 2+2 cells, 4+1: {solo}"
        );
        let title = "Nav rows jitter on stream start";
        let keep = kept(title.chars().count(), solo / CH);
        let cut: String = title.chars().take(keep).collect();
        assert_eq!(format!("{cut}\u{2026}"), "Nav rows jitter on strea\u{2026}");
        assert_eq!(
            keep + 1,
            25,
            "the ellipsis takes the box's last cell, one gap before the word"
        );
        // Every kind of row, every word length: a title one cell too long
        // keeps all but two characters and ends on the box's edge, and one
        // that fits is whole.
        for (cells, word) in [
            (2.0, Some("1m04s")),
            (3.0, Some("needs you")),
            (3.0, Some("failing 2")),
            (1.0, Some("approval")),
            (2.0, Some("4")),
        ] {
            let width = title_w(cells, word) / CH;
            assert!(close(width, width.round()), "whole cells: {cells} {word:?}");
            let box_cells = width.round() as usize;
            assert_eq!(kept(box_cells, width), box_cells, "a title that fits");
            assert_eq!(kept(box_cells + 1, width) + 1, box_cells, "cut to the edge");
        }
        assert_eq!(title_w(40.0, Some("needs you")), 0.0, "never negative");
        assert_eq!(short_branch("dev").as_ref(), "dev");
        let long = short_branch("worktree-pay-api-migration-cleanup");
        assert_eq!(long.chars().count(), NAV_BRANCH_MAX_CH);
        assert!(long.ends_with('\u{2026}'));
    }

    /// The word says one thing, in priority order, and the Pane's head slot
    /// decides it: a waiting request, red tests, a failure, a working
    /// Thread's elapsed time, a finished turn; everything else is its age.
    /// Every word is the metadata ink: the dot carries the colour.
    #[test]
    fn the_word_is_one_fact_in_priority_order() {
        use crate::pane::HeadSlot;
        let age = || SharedString::from("40m");
        assert_eq!(
            NavTail::of(Some(&HeadSlot::NeedsYou(words::QUESTION)), age()),
            NavTail::NeedsYou
        );
        assert_eq!(
            NavTail::of(Some(&HeadSlot::Failing(Some(3))), age()),
            NavTail::Failing(Some(3))
        );
        assert_eq!(
            NavTail::of(Some(&HeadSlot::Failing(None)), age()),
            NavTail::Failing(None)
        );
        assert_eq!(NavTail::of(Some(&HeadSlot::Failed), age()), NavTail::Failed);
        assert_eq!(NavTail::of(Some(&HeadSlot::Done), age()), NavTail::Done);
        assert_eq!(
            NavTail::of(Some(&HeadSlot::Working("1m04s".into())), age()),
            NavTail::Working("1m04s".into()),
            "a working row says how long it has worked, to the second"
        );
        assert_eq!(
            NavTail::of(Some(&HeadSlot::Working(String::new())), age()),
            NavTail::None
        );
        for quiet in [None, Some(&HeadSlot::Interrupted), Some(&HeadSlot::Parked)] {
            assert_eq!(NavTail::of(quiet, age()), NavTail::Age(age()));
        }
        let text = |tail: NavTail| tail.text();
        assert_eq!(text(NavTail::NeedsYou), Some(words::NEEDS_YOU.into()));
        assert_eq!(text(NavTail::Failing(Some(2))), Some("failing 2".into()));
        assert_eq!(text(NavTail::Failing(None)), Some(words::FAILING.into()));
        assert_eq!(text(NavTail::Failed), Some(words::FAILED.into()));
        assert_eq!(text(NavTail::Done), Some(words::DONE.into()));
        assert_eq!(text(NavTail::Working("12s".into())), Some("12s".into()));
        assert_eq!(text(NavTail::Age(age())), Some(age()));
        assert_eq!(text(NavTail::None), None);
        let mut word = word_cell(text(NavTail::Failing(Some(2))));
        assert_eq!(word.style().text.color, Some(rgb(TEXT_MUTED).into()));
    }

    /// The nav never says `now` (C10): a Thread used seconds ago has an
    /// empty age, which draws nothing.
    #[test]
    fn the_nav_never_says_now() {
        let at = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10_000);
        for secs in [0, 1, 30, 59] {
            let age = crate::facts::since_label(at, at + std::time::Duration::from_secs(secs));
            let tail = NavTail::of(None, age);
            assert_eq!(tail.text(), None, "{secs}s says nothing");
        }
        let minute = crate::facts::since_label(at, at + std::time::Duration::from_secs(90));
        assert_eq!(NavTail::of(None, minute).text(), Some("1m".into()));
        for tail in [
            NavTail::NeedsYou,
            NavTail::Failing(None),
            NavTail::Failed,
            NavTail::Done,
        ] {
            assert_ne!(tail.text().unwrap().as_ref(), "now");
        }
    }

    /// The dots are the Pane's own colours: green only for live work, a
    /// failing Thread in the failure's red, a Decision yellow, idle the
    /// metadata ink whether read or not. Nothing pulses.
    #[test]
    fn status_dots_say_state_and_green_only_means_live() {
        let face = |status, unread| {
            dot_face(&ThreadRow {
                status,
                unread,
                ..thread()
            })
        };
        let fill = |status| face(status, false).style().text.color;
        assert_eq!(fill(RowStatus::Working), Some(rgb(RUNNING).into()));
        assert_eq!(fill(RowStatus::Failing), Some(rgb(BLOCKED).into()));
        assert_eq!(fill(RowStatus::Failed), Some(rgb(BLOCKED).into()));
        assert_eq!(fill(RowStatus::NeedsYou), Some(rgb(ATTENTION).into()));
        assert_eq!(fill(RowStatus::Idle), Some(rgb(IDLE).into()));
        assert_eq!(
            face(RowStatus::Idle, true).style().text.color,
            Some(rgb(IDLE).into()),
            "an unread quiet row keeps its dot"
        );
        for status in [
            RowStatus::Working,
            RowStatus::Failing,
            RowStatus::NeedsYou,
            RowStatus::Failed,
        ] {
            assert_eq!(
                face(status, true).style().text.color,
                fill(status),
                "{status:?}: a live state is the louder truth"
            );
        }
    }

    /// One status truth: for every Pane state but parked, and either side of
    /// unread, the nav row's still dot is the Pane's own dot. A parked
    /// Thread is the one step the nav takes: its ring is faint (the
    /// prototype's `○`), typed like the dot.
    #[test]
    fn the_nav_dot_is_the_panes_dot() {
        use WallState::*;
        let paint = |mut dot: Div| dot.style().text.color;
        for state in [Working, Failing, Decision, Blocked, Done, Idle, Parked] {
            for unread in [false, true] {
                let row = ThreadRow {
                    status: RowStatus::of(state),
                    unread,
                    ..thread()
                };
                assert_eq!(
                    paint(dot_face(&row)),
                    paint(crate::pane::cell_dot(state, unread)),
                    "{state:?} unread={unread}"
                );
                assert_eq!(RowStatus::of(row.status.wall()), row.status);
            }
        }
        let mut ring = components::status_ring(TEXT_FAINT);
        assert_eq!(ring.style().text.color, Some(rgb(TEXT_FAINT).into()));
    }

    /// N-8: the column paints no ground over the titlebar band — its chrome
    /// starts under it — and has no hairline; the one seam is its own
    /// column right of the nav: the dark seam open, the plane folded.
    #[test]
    fn the_column_is_chrome_under_the_band_with_one_seam_beside_it() {
        let mut column = shell(WIDTH);
        let style = column.style();
        assert_eq!(style.background, None, "the band paints its own ground");
        let edges = &style.border_widths;
        assert!(edges.top.is_none() && edges.right.is_none());
        assert!(edges.bottom.is_none() && edges.left.is_none());
        assert_eq!(style.size.width, Some(px(WIDTH).into()));
        assert_eq!(FOLDED_WIDTH, 0.0, "folded, the board takes the width");
        let mut ground = body();
        let style = ground.style();
        assert_eq!(
            style.background,
            Some(gpui::Hsla::from(paint::CHROME).into())
        );
        assert_eq!(style.inset.top, Some(px(WIN_CHROME_H).into()));
        let mut open = seam(false);
        assert_eq!(open.style().size.width, Some(px(CHROME_SEAM_W).into()));
        assert_eq!(
            open.style().background,
            Some(gpui::Hsla::from(paint::CHROME_SEAM).into())
        );
        let mut folded = seam(true);
        assert_eq!(
            folded.style().background,
            Some(gpui::Hsla::from(paint::PLANE).into()),
            "folded at rest, nothing darker than the plane at x = 0"
        );
    }

    /// N-16: the titlebar cell rides with the column — 36ch open, 20ch
    /// folded (the lights, the toggle and the bell, and the gear off macOS)
    /// — and cross-fades as one colour from the chrome to the plane. Its
    /// doors are 28×24 at its right, 1ch clear of its edge: open, the
    /// toggle spans 217–245 and the bell 245–273; folded, 92.2–120.2 and
    /// 120.2–148.2.
    #[test]
    fn the_titlebar_cell_rides_with_the_column() {
        assert_eq!(chrome_width(WIDTH, 0), WIDTH);
        assert_eq!(chrome_width(FOLDED_WIDTH, 0), NAV_CHROME_FOLDED_W);
        assert_eq!(
            chrome_width(FOLDED_WIDTH, 1),
            NAV_CHROME_FOLDED_W + ICON_BUTTON,
            "a waiting update adds its door"
        );
        let mid = chrome_width(WIDTH / 2.0, 0);
        assert!(mid > NAV_CHROME_FOLDED_W && mid < WIDTH);
        if cfg!(target_os = "macos") {
            assert!((NAV_CHROME_FOLDED_W - 156.0).abs() < 1e-3, "20ch");
            let doors = |cell: f32| {
                let bell_right = cell - NAV_PAD_X;
                (
                    bell_right - 2.0 * ICON_BUTTON,
                    bell_right - ICON_BUTTON,
                    bell_right,
                )
            };
            let (toggle, bell, end) = doors(WIDTH);
            assert!((toggle - 217.0).abs() < 1e-3 && (bell - 245.0).abs() < 1e-3);
            assert!((end - 273.0).abs() < 1e-3);
            let (toggle, bell, end) = doors(NAV_CHROME_FOLDED_W);
            assert!((toggle - 92.2).abs() < 1e-3 && (bell - 120.2).abs() < 1e-3);
            assert!((end - 148.2).abs() < 1e-3);
            assert!(toggle >= NAV_CHROME_LEAD, "clear of the host's lights");
        }
        let mut open = chrome_band(WIDTH, band_ground(0.0), false);
        assert_eq!(
            open.style().background,
            Some(gpui::Hsla::from(paint::CHROME).into()),
            "over the open column, the column's chrome"
        );
        assert_eq!(open.style().size.height, Some(px(WIN_CHROME_H).into()));
        assert_eq!(band_ground(1.0), gpui::Hsla::from(paint::PLANE));
        let half = band_ground(0.5);
        assert_ne!(half, gpui::Hsla::from(paint::CHROME));
        assert_ne!(half, gpui::Hsla::from(paint::PLANE));
        let mut door = collapse_button();
        let style = door.style();
        assert_eq!(style.size.width, Some(px(ICON_BUTTON).into()));
        assert_eq!(style.size.height, Some(px(ICON_BUTTON_H).into()));
        assert_eq!(style.background, None, "clear at rest");
        assert_eq!(ICON_BUTTON_GLYPH, 15.0);
    }

    /// The ⌘ digits are bound: each names the action its key is bound to.
    #[test]
    fn the_command_digits_are_bound() {
        for ordinal in 1..=9 {
            let action = crate::cockpit::focus_rail_action(ordinal).expect("one action per digit");
            assert_eq!(
                crate::components::bound_chord(action),
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
        assert_eq!(crate::cockpit::focus_rail_action(10), None);
    }

    /// The section is capped at half the column and its list scrolls,
    /// so a long parked history can never push the running tree out.
    #[test]
    fn the_parked_section_is_capped_and_its_list_scrolls() {
        let mut section = parked_section();
        assert_eq!(
            section.style().max_size.height,
            Some(relative(NAV_PARKED_MAX_SHARE).into())
        );
        let scroll = ScrollHandle::new();
        let mut list = parked_list(&scroll);
        assert_eq!(
            list.style().overflow.y,
            Some(gpui::Overflow::Scroll),
            "the list scrolls on its own handle"
        );
        let mut header = parked_header(3, false, false);
        assert_eq!(
            header.style().text.color,
            Some(rgb(TEXT_MUTED).into()),
            "a dim header line"
        );
    }

    /// The nav's own clocks: the row wash reaches full in 80ms, a titlebar
    /// door in 100ms, the content fades over 150ms and the titlebar cell
    /// cross-fades over 200ms, each on CSS `ease` (the prototype's
    /// transitions).
    #[test]
    fn the_nav_keeps_the_prototypes_clocks() {
        assert_eq!(ROW_HOVER.duration_ms, 80);
        assert_eq!(DOOR_HOVER.duration_ms, 100);
        assert_eq!(CONTENT_FADE.duration_ms, 150);
        assert_eq!(BAND_FADE.duration_ms, 200);
        for spec in [ROW_HOVER, DOOR_HOVER, CONTENT_FADE, BAND_FADE] {
            assert_eq!(spec.curve, motion::EASE);
        }
    }
}
