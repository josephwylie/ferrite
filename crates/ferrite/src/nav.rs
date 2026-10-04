//! The left navigation column (#21): the Project filter, the Needs-you
//! strip, then the Groups with their member Threads hanging under them and
//! the solo Threads at root — and, at the foot of the column, the
//! **Parked** section: every parked Thread no Group claims, folded shut by
//! default so the tree above holds only what is running. It is a view,
//! never the only door: everything a row does (focus, revive, regroup)
//! stays reachable from the keyboard.
//!
//! Drawing only, like `pane.rs`: the cockpit assembles a `NavState` per
//! frame from O(1) reads plus its project/branch/parked caches —
//! `Store::load` and `Instruments::of` are banned here, which is what keeps
//! the 24-Pane wall smooth with the nav open. Click wiring stays in
//! `cockpit.rs`, the same split `pane_cell` uses.
//!
//! **A terminal list on the chrome.** The column paints `paint::CHROME`
//! once — thin glass on macOS — with no hairline to the content, and every
//! item in it is one 20px line of Geist Mono laid out in character cells
//! (the WP-G section of `theme.rs` draws the grid): a 2ch cursor cell (`❯`
//! in the accent on the selected row), a 2ch mark cell (the status dot, a
//! working Thread's braille spinner, a faint disclosure triangle), the
//! title, and one dim word at the right. A Group's members hang under it
//! on faint tree glyphs (`├ ` / `└ `). There is no subtitle line, no
//! provider logo and no keyboard-hint row: a row's other facts (provider,
//! Project, branch, subagents, its ⌘ digit) are its tooltip.
//!
//! Colour is state, carried by the dot alone; every right-hand word is
//! `TEXT_MUTED`. Selection is `paint::SELECTION` on the focused Thread's
//! row with the `❯` and a `TEXT_STRONG` title; hover is `paint::HOVER`.
//! One thing moves: a working Thread's spinner. Nothing pulses.
//!
//! While any Thread waits on the operator, the **Needs-you strip** sits
//! under the head: its rows are the answer order, and its first row is what
//! ⌘D and the wall's `y`/`n`/`a` act on.
//!
//! **Folding** (cmd-B) rides the column to nothing; the titlebar cell over
//! it (`chrome_band`) rides along to the folded cell that keeps the traffic
//! lights, the sidebar toggle, the bell and the gear.
//!
//! Every colour and metric is a `theme` token; this file holds no literal
//! of its own.

use ferrite_core::groups::GroupId;
use ferrite_core::settings::ThreadListOrder;
use ferrite_core::store::Provider;
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::ThreadId;

use gpui::component::button::Button;
use gpui::component::tooltip::Tooltip;
use gpui::prelude::*;
use gpui::{
    div, px, radians, relative, rgb, rgba, AnyElement, App, CursorStyle, Div, ElementId,
    FontWeight, ScrollHandle, SharedString, Stateful, Transformation,
};

use crate::cockpit::thread_status;
use crate::components::{self, Tip};
use crate::icons::{self, icon};
use crate::pane::WallState;
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::*;

/// The nav's two widths — 286px open, and nothing folded (cmd-B).
/// `CockpitView::cell()` subtracts whichever is live, so the nav stays part
/// of the semantic-zoom input rather than a special case.
pub use crate::theme::{NAV_FOLDED_W as FOLDED_WIDTH, NAV_WIDTH as WIDTH};

/// The slack a truncating title's budget gets over its visible box.
///
/// gpui truncates by summing each character's advance measured **alone**
/// (gpui-0.2.2 text_system/line_wrapper.rs:193 `width_for_char`, cached per
/// char) and keeps a prefix only while `width + suffix_width <
/// truncate_width` — a strict `<`, against CSS's `<=`. At an exact fit the
/// last glyph that would still fit is dropped and its cell is left empty.
/// Handing the truncator this much extra budget restores the glyph; the
/// visible box stays exactly its pinned width, and clips.
const TRUNCATE_SLOP: f32 = SPACE_1;

/// A child SVG paints from its **own** style — an ambient text colour
/// reaches text but never an `svg()`. `group_hover` is the only mechanism
/// that carries a parent's hover down to a child's colour, so each control
/// names a group.
const COLLAPSE_GROUP: &str = "nav-collapse";
const FILTER_GROUP: &str = "nav-filter";
const DOOR_GROUP: &str = "nav-door";
const PARKED_GROUP: &str = "nav-parked";

/// What the nav draws this frame: one filter, then Groups with their
/// members, then the solos, then the Parked section. Nothing here is a
/// store read — the cockpit assembles it from O(1) reads plus its
/// project/branch caches.
pub struct NavState {
    pub filter: FilterState,
    pub groups: Vec<GroupBlock>,
    /// The solo Threads that are open: a parked solo is in `parked`, not
    /// here.
    pub solos: Vec<ThreadRow>,
    /// The Parked section's rows: parked Threads no Group claims, in the
    /// park order. A parked Group member stays under its Group — the Group
    /// is its place, and opening the Group revives it there.
    pub parked: Vec<ThreadRow>,
    /// Whether the Parked section is unfolded. Shut by default: the tree
    /// is for what is running, and the section is where the rest wait.
    pub parked_open: bool,
    /// The one order the tree draws in — Groups and solo Threads
    /// interleaved, most recently used first. The two lists above are the
    /// membership; this is the sequence.
    pub order: Vec<NavItem>,
    pub project_sections: Vec<ProjectSection>,
    pub thread_list_order: ThreadListOrder,
    pub order_open: bool,
    pub collapsed: bool,
    /// Every Thread waiting on the operator, in the answer order
    /// (`Cockpit::needs_you`), whatever the Project filter says: the strip
    /// is the queue ⌘D walks, and its first row is the answer target.
    pub needs_you: Vec<NeedsYouRow>,
}

/// One row of the Needs-you strip: a second, reference row for a Thread
/// that waits on the operator (its own row stays where it is in the tree),
/// with what it waits for (`approval` / `question`) as its word.
#[derive(Clone)]
pub struct NeedsYouRow {
    pub row: ThreadRow,
    pub kind: &'static str,
}

/// A flat newest-first run of Threads under one Project heading. Group
/// membership still exists and is restored when a row is opened; this view
/// simply makes Project the visible hierarchy.
pub struct ProjectSection {
    pub project: Option<ProjectId>,
    pub label: SharedString,
    /// The branch the heading names at its right: the checkout every row
    /// shares, else the Project's default. `None` says nothing.
    pub branch: Option<SharedString>,
    pub rows: Vec<ThreadRow>,
}

impl NavState {
    /// Every row in the order the tree draws it: a Group's members where
    /// their Group sits, a solo where it sits. The Parked section is not
    /// the tree, so its rows are not here; tests read the tree's order
    /// from it.
    pub fn ordered_rows(&self) -> Vec<&ThreadRow> {
        self.order
            .iter()
            .flat_map(|item| match item {
                NavItem::Group(index) => self.groups[*index].members.iter(),
                NavItem::Solo(index) => std::slice::from_ref(&self.solos[*index]).iter(),
            })
            .collect()
    }

    /// The ⌘1…⌘9 order: every Thread that needs you pinned first, in the
    /// answer order, then the tree's order — so ⌘1 is the next answer.
    pub fn rail_rows(&self) -> Vec<&ThreadRow> {
        let waiting = |thread: ThreadId| {
            self.needs_you
                .iter()
                .position(|entry| entry.row.thread == thread)
        };
        let mut rows = self.ordered_rows();
        // Stable: rows that wait keep the answer order among themselves,
        // and the rest keep the tree's.
        rows.sort_by_key(|row| waiting(row.thread).unwrap_or(usize::MAX));
        rows
    }

    /// Name the ⌘ digit on every row it lands on (the first nine of
    /// `rail_rows`), wherever the row is drawn — the tree, a Project
    /// section, the Needs-you strip — so its tooltip can say the key.
    pub fn number_rows(&mut self) {
        let order: Vec<ThreadId> = self
            .rail_rows()
            .iter()
            .take(9)
            .map(|row| row.thread)
            .collect();
        let ordinal = |thread: ThreadId| {
            order
                .iter()
                .position(|numbered| *numbered == thread)
                .map(|index| index + 1)
        };
        for row in self
            .groups
            .iter_mut()
            .flat_map(|group| group.members.iter_mut())
            .chain(self.solos.iter_mut())
            .chain(
                self.project_sections
                    .iter_mut()
                    .flat_map(|section| section.rows.iter_mut()),
            )
            .chain(self.needs_you.iter_mut().map(|entry| &mut entry.row))
        {
            row.ordinal = ordinal(row.thread);
        }
    }

    /// The solo Threads alone, in the tree's order.
    #[cfg(test)]
    pub fn ordered_solos(&self) -> Vec<&ThreadRow> {
        self.order
            .iter()
            .filter_map(|item| match item {
                NavItem::Solo(index) => Some(&self.solos[*index]),
                NavItem::Group(_) => None,
            })
            .collect()
    }
}

/// One entry in the tree's order: an index into `NavState::groups`, or one
/// into `NavState::solos`. Indices rather than the blocks themselves, so
/// the two kinds keep their own types and nothing is cloned to be ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavItem {
    Group(usize),
    Solo(usize),
}

/// The single Project dropdown at the top of navigation. Default label
/// `All projects`.
pub struct FilterState {
    pub label: SharedString,
    pub open: bool,
    pub options: Vec<FilterOption>,
}

/// One row of the filter menu. `project: None` is the `All projects` row and
/// is always first.
pub struct FilterOption {
    pub project: Option<ProjectId>,
    pub label: SharedString,
    pub selected: bool,
}

/// One Group and the Threads hanging under it.
#[derive(Clone)]
pub struct GroupBlock {
    pub id: GroupId,
    pub title: SharedString,
    /// One Project's name, or the count of Projects across the whole Group.
    /// None when no member resolves one. The row does not print it; it is
    /// the disclosure mark's tooltip.
    pub projects: Option<SharedString>,
    pub members: Vec<ThreadRow>,
}

/// One Thread's row — identical whether it is a Group member or a solo; only
/// the container differs. One line: cursor, status mark, title, word.
#[derive(Clone)]
pub struct ThreadRow {
    pub thread: ThreadId,
    pub name: SharedString,
    /// What the Thread is doing right now — the one glance the operator
    /// asked for from the tree: which agents are working, which wait.
    pub status: RowStatus,
    /// The Project's name: what the filter and the Project sections read.
    /// The row names it in its tooltip.
    pub project: Option<SharedString>,
    /// The branch the Thread's checkout is on, from the facts cache, only
    /// when it is not the Project's default (`ThreadFacts::off_default_branch`).
    /// `None` says nothing; it is never guessed. The row names it in its
    /// tooltip.
    pub branch: Option<SharedString>,
    /// The provider, named in the tooltip; rows draw no logo.
    pub provider: Option<Provider>,
    /// This is the focused Pane's Thread: it carries the tree's one
    /// selection, its `❯`, and the `TEXT_STRONG` title.
    pub current: bool,
    /// The Thread finished while the operator looked elsewhere (an unread
    /// Notice). Its own axis, never a state: a quiet unread row wears the
    /// still unread dot and a `TEXT_STRONG` title, ink only, never weight.
    pub unread: bool,
    /// The one word at the row's right (C10).
    pub tail: NavTail,
    /// Subagents known for this Thread, named in the tooltip.
    pub subagents: usize,
    /// The ⌘ digit that lands on this Thread (`NavState::number_rows`),
    /// named in the tooltip; `None` past the ninth.
    pub ordinal: Option<usize>,
}

/// What a row's word says (C10), in priority order: a pending Decision is
/// `needs you`; red tests `failing`/`failing N` and a failed turn or closed
/// Session `failed`; a working Thread its elapsed time (`1m04s`); a
/// finished turn `done`; otherwise the age, once it reaches a minute
/// (`facts::since_label` says nothing before that). Never `now`. Every one
/// is `TEXT_MUTED`: the dot carries the colour.
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

/// Where a Thread row hangs: at root (a solo, a Project section's row, a
/// parked row), or under its Group on a tree glyph — `├ `, or `└ ` on the
/// Group's last member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowPlace {
    Root,
    Member { last: bool },
}

// ------------------------------------------------------------- the grid

/// The width a title box gets: the row's text width less `cells` 2ch mark
/// cells and the word at its right (its characters and the 1ch before
/// it). One monospace face, so every advance is one `CH`.
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
/// gpui caches a nowrap line's first measure permanently (gpui-0.2.2
/// elements/text.rs:373 — `wrap_width` is `None` for nowrap, so the early
/// return fires on every later call), and taffy only hands a text leaf a
/// definite width when the leaf's flex container is a **column** whose own
/// available width is definite — which taffy derives from the child's own
/// min/max width (taffy-0.9.0 compute/flexbox.rs:661-679). A `flex_1`, a
/// `w_full` or even a `w(px(..))` cell is measured at max-content first, so
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

/// A 2ch mark cell, its mark centred on the cell's first character — where
/// a typed glyph would sit, the second cell being its space.
fn cell(mark: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(NAV_CELL))
        .h(px(NAV_LINE))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .w(px(CH))
                .h(px(NAV_LINE))
                .child(mark),
        )
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

/// The disclosure triangle, drawn (Geist Mono has no small triangles): `▾`
/// open, `▸` shut, in the faint structure ink.
fn disclosure(open: bool) -> gpui::Svg {
    icon(
        if open {
            icons::DISCLOSURE_DOWN
        } else {
            icons::DISCLOSURE_RIGHT
        },
        NAV_GLYPH,
        TEXT_FAINT,
    )
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
        .children(word)
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

// ------------------------------------------------------------- the marks

/// The status mark in a Thread row's mark cell (`thread_status`, the one
/// truth the Panes share): a working Thread's braille spinner in `RUNNING`
/// — the one thing in the column that moves, still under reduced motion —
/// a failing or failed Thread's red dot, a Decision's yellow, idle the
/// metadata ink (read or not), and a parked Thread a faint ring.
fn status_mark(row: &ThreadRow) -> AnyElement {
    match row.status {
        RowStatus::Working => components::braille_spinner(RUNNING, NAV_GLYPH),
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

/// The nav column itself: full height, on `paint::CHROME`, with **no
/// hairline** to the content. Its width is the caller's to ride (cmd-B);
/// its content keeps `WIDTH` (`content`) and is clipped, so nothing
/// reflows while it moves. The chrome runs up under the titlebar cell
/// (`chrome_band`), which paints nothing of its own over it.
pub fn shell(collapsed: bool) -> Div {
    div()
        .debug_selector(|| "nav-column".into())
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(if collapsed { FOLDED_WIDTH } else { WIDTH }))
        .overflow_hidden()
        .bg(paint::CHROME)
        .font_family(FONT_UI)
        .text_size(px(FS_UI))
        .line_height(px(NAV_LINE))
        .text_color(rgb(TEXT))
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

/// The 1px seam at the column's right edge: on glass the dark seam the
/// prototype leaves between the sidebar and the board
/// (`paint::CHROME_SEAM`), on opaque platforms nothing. Laid last, over
/// the rows that reach the edge.
pub fn seam() -> Div {
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .right_0()
        .w(px(1.))
        .bg(paint::CHROME_SEAM)
}

/// How wide the titlebar cell over the column is while the column is
/// `column` wide: the column itself when open, riding down to the folded
/// cell (`NAV_CHROME_FOLDED_W`, plus `extra_doors` icon doors) as it folds.
/// The titlebar strip over the board starts where it ends.
pub fn chrome_width(column: f32, extra_doors: usize) -> f32 {
    let folded = NAV_CHROME_FOLDED_W + extra_doors as f32 * ICON_BUTTON;
    let open = (column / WIDTH).clamp(0.0, 1.0);
    folded + (WIDTH - folded) * open
}

/// The titlebar cell over the column: `width` wide (`chrome_width`), one
/// band of `WIN_CHROME_H` at the window's top-left, laid over everything
/// under it.
///
/// On macOS the traffic lights are the **host's**, positioned by
/// `TitlebarOptions`, so the band reserves their room rather than drawing
/// fakes: a `NAV_CHROME_LEAD`-wide spacer that holds nothing — anything
/// drawn or hit-testable in that strip kills AppKit's drag region. Where
/// the app draws its own titlebar, the lead is one cell. The caller hangs
/// the doors after it: a stretch, then the sidebar toggle, the bell and
/// the gear (the prototype's titlebar).
///
/// Over the open column the band paints nothing — the column's chrome
/// runs under it. Wherever the column has folded out from under it
/// (`column` narrower than `width`) it paints the reading plane, as the
/// titlebar over the board does.
pub fn chrome_band(width: f32, column: f32) -> Div {
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
        .when(column < width, |band| {
            band.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(column.max(0.0)))
                    .right_0()
                    .bg(paint::PLANE),
            )
        })
        .child(div().flex_shrink_0().w(px(NAV_CHROME_LEAD)))
}

/// The sidebar toggle: the 16px sidebar glyph in a titlebar icon door, the
/// same door open or folded — the cell rides, the door does not change.
pub fn collapse_button() -> Stateful<Div> {
    div()
        .id(("nav-collapse", 0usize))
        .debug_selector(|| "nav-collapse".into())
        .group(COLLAPSE_GROUP)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .rounded(px(R_CONTROL))
        .hover_control("nav-collapse")
        .press_control()
        .tooltip(crate::menu::action_tooltip(
            "Toggle sidebar",
            "cockpit::ToggleNav",
        ))
        // The glyph lifts to `TEXT` through the same blend as the ground.
        .child(
            icon(icons::SIDEBAR, ICON_BUTTON_GLYPH, TEXT_MUTED).text_color(
                crate::motion::hover_blend(
                    "nav-collapse",
                    rgb(TEXT_MUTED).into(),
                    rgb(TEXT).into(),
                ),
            ),
        )
}

// ------------------------------------------------------------- the head

/// The head: one line holding the Project filter and its doors, a half
/// row above the strip or the tree. `relative`, because the filter and
/// order menus hang off it; the caller wraps them in `gpui::deferred(..)`
/// so the scrolling tree below cannot overpaint them.
pub fn nav_head() -> Div {
    div()
        .relative()
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .h(px(NAV_HEAD_H))
        .mb(px(NAV_SECTION_GAP))
}

/// An icon door's frame on the grid: 3ch × one line, clear at rest,
/// `paint::HOVER` under the pointer (blended), `paint::PRESS` pressed.
fn door_frame(id: impl Into<ElementId>, cx: &App) -> Button {
    components::faded_button(
        id,
        rgba(TRANSPARENT).into(),
        paint::HOVER.into(),
        paint::PRESS.into(),
        rgb(TEXT_MUTED).into(),
        cx,
    )
    .group(DOOR_GROUP)
    .flex_shrink_0()
    .w(px(NAV_DOOR_W))
    .h(px(NAV_LINE))
    .p_0()
}

/// An icon door: its glyph at the type size in `ink`, brightening to
/// `TEXT` under the pointer.
fn door(id: impl Into<ElementId>, glyph: &'static str, ink: u32, cx: &App) -> Button {
    door_frame(id, cx).child(
        icon(glyph, NAV_DOOR_GLYPH, ink)
            .group_hover(DOOR_GROUP, |style| style.text_color(rgb(TEXT))),
    )
}

/// The persistent door to a new Thread, at the head's right. The cockpit
/// owns the click because opening a draft changes its roster.
pub fn add_thread_button(cx: &App) -> Button {
    components::icon_button("add-thread", icons::PLUS, "New thread", cx)
        .debug_selector(|| "add-thread".into())
        .flex_shrink_0()
        .w(px(NAV_DOOR_W))
        .h(px(NAV_LINE))
}

/// The order door beside New Thread. Its chosen order shows even while the
/// menu is shut (`active` lifts the glyph to `TEXT`); open, it wears the
/// selection.
pub fn order_button(active: bool, open: bool, cx: &App) -> Button {
    door(
        "thread-list-order",
        icons::SORT,
        if active || open { TEXT } else { TEXT_MUTED },
        cx,
    )
    .when(open, |button| button.bg(paint::SELECTION))
    .tab_stop(true)
    .debug_selector(|| "thread-list-order".into())
    .tip("Thread order")
    .accessibility_label("Thread order")
}

/// The order menu: a floating surface anchored under its door at the
/// head's right, a caption naming what the rows choose, then the rows.
pub fn order_menu() -> Div {
    components::floating_surface()
        .absolute()
        .top(px(MENU_TOP))
        .right(px(NAV_PAD_X))
        .w(px(NAV_ORDER_MENU_W))
        .child(components::menu_section("Show threads by", None, None))
}

/// One order row: the shared menu row, its check in the accent on the
/// chosen order. It stays a tab stop, so the keyboard reaches it.
pub fn order_option(index: usize, label: &'static str, selected: bool) -> Button {
    components::button(("thread-list-order-option", index))
        .tab_stop(true)
        .debug_selector(move || format!("thread-list-order-option-{index}"))
        .w_full()
        .h(px(MENU_ROW_H))
        .p_0()
        .rounded(px(R_MENU_ROW))
        .child(
            components::menu_row_content(
                &components::MenuItem::new(label).checked(selected),
                false,
                false,
            )
            .w_full(),
        )
}

/// The Project filter trigger: the head's one title, a dim line over the
/// tree — the scope's name, then a faint `▾`. Hover lifts it to `TEXT` on
/// `paint::HOVER`; open, it wears the selection, the menu being the hover
/// made permanent, so the control does not blink when the pointer leaves.
pub fn filter_trigger(state: &FilterState) -> Stateful<Div> {
    let mark = disclosure(true);
    let mark = if state.open {
        mark.with_transformation(Transformation::rotate(radians(std::f32::consts::PI)))
    } else {
        mark
    };
    div()
        .id(("nav-filter", 0usize))
        .debug_selector(|| "nav-filter".into())
        .group(FILTER_GROUP)
        .flex()
        .flex_row()
        .flex_1()
        .min_w_0()
        .items_center()
        .h(px(NAV_LINE))
        .px(px(NAV_PAD_X))
        .whitespace_nowrap()
        .text_size(px(FS_UI))
        .font_weight(W_BODY)
        .line_height(px(NAV_LINE))
        .when(state.open, |open| {
            open.text_color(rgb(TEXT))
                .hover_carried("nav-filter")
                .press_row()
        })
        .when(!state.open, |shut| {
            shut.text_color(rgb(TEXT_MUTED))
                .hover_row("nav-filter")
                .press_row()
        })
        .child(
            div()
                .min_w_0()
                .truncate()
                .group_hover(FILTER_GROUP, |style| style.text_color(rgb(TEXT)))
                .child(state.label.clone()),
        )
        .child(cell(mark).ml(px(NAV_WORD_GAP)))
}

/// The floating filter menu: the shared floating surface, spanning the
/// head under the trigger. The caller pushes `filter_option` rows, then a
/// separator and `filter_action`, and defers the whole thing.
pub fn filter_menu() -> Div {
    components::floating_surface()
        .absolute()
        .top(px(MENU_TOP))
        .left(px(NAV_PAD_X))
        .right(px(NAV_PAD_X))
}

/// One filter row: the shared menu row, the current scope checked in the
/// accent.
pub fn filter_option(index: usize, option: &FilterOption) -> Stateful<Div> {
    components::menu_row(
        ("nav-filter-option", index),
        &components::MenuItem::new(option.label.clone()).checked(option.selected),
        false,
        false,
    )
}

/// The visible door to Project management: the pencil directly right of
/// the filter trigger, before the head's other doors, so it reads as part
/// of the control that names the Project. It is drawn only while the
/// filter names a Project — `All projects` is a filter state, not a
/// Project, and has nothing to edit.
pub fn project_edit_button(cx: &App) -> Button {
    door("project-edit", icons::PENCIL, TEXT_MUTED, cx)
        .tab_stop(true)
        .debug_selector(|| "project-edit".into())
        .tip("Edit project")
        .accessibility_label("Edit project")
}

/// The filter menu's last row: a verb, not an option — `Add project…`,
/// its label on the rows' own edge (no mark), `TEXT_MUTED` at rest and
/// `TEXT` under the pointer, on the hover face. The caller sets a
/// separator above it and wires the press to the folder picker.
pub fn filter_action(index: usize, label: &'static str) -> Stateful<Div> {
    let key = SharedString::from(format!("nav-filter-action-{index}"));
    components::menu_row_content(&components::MenuItem::new(label), false, false)
        .id(("nav-filter-action", index))
        .text_color(crate::motion::hover_blend(
            &key,
            rgb(TEXT_MUTED).into(),
            rgb(TEXT).into(),
        ))
        .hover_raised(key)
        .press_raised()
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

/// One Group section: the parent row, then its members, flush with
/// whatever is above it — the tree glyphs draw the block.
pub fn group_block() -> Div {
    div().relative().flex().flex_col().flex_shrink_0()
}

/// "Insert between these two Groups": the blocks sit flush, so the target
/// is an absolute `NAV_DROP_BAND` hit band over the top edge of the second
/// block's header, taking no layout. The caller lays it **after** the
/// block, so it is the topmost hitbox there.
pub fn group_gap(index: usize) -> Stateful<Div> {
    group_gap_lead(index)
}

/// "Insert above the first Group", by the same trick: an absolute
/// `NAV_DROP_BAND` hit band laid over the first Group header's top edge,
/// taking no layout and, without `occlude`, stealing none of its clicks.
pub fn group_gap_lead(index: usize) -> Stateful<Div> {
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

/// The Group parent row, one line: a blank cursor cell, the faint `▾` (its
/// tooltip names the Group's Projects), the title in body ink and weight,
/// and how many members it holds, dim, at the right. No selection of its
/// own — the one selection is the focused member's — no logo, and no
/// `needs you` of its own: the member that waits says so.
#[cfg(test)]
pub fn group_row(row: &GroupBlock) -> Stateful<Div> {
    group_row_with_title(row, row.title.clone())
}

/// `group_row` with the title leaf supplied by the caller — the cockpit
/// hands in a click-to-rename wrapper, or the live editor while renaming.
/// The title box is pinned (`fitted`), so the title truncates and the
/// editor inherits its 20px line: renaming never moves the row.
pub fn group_row_with_title(row: &GroupBlock, title: impl IntoElement) -> Stateful<Div> {
    let count = SharedString::from(row.members.len().to_string());
    let id = row.id;
    row_frame(("nav-group", id.get() as usize), false)
        .debug_selector(move || format!("nav-group-{}", id.get()))
        .child(blank_cell())
        .child(group_disclosure(id, row.projects.clone()))
        .child(fitted(title_w(2.0, Some(&count)), TEXT, W_BODY, title))
        .child(
            word_cell(Some(count)).debug_selector(move || format!("nav-group-count-{}", id.get())),
        )
}

/// A Group row's `▾`, in its mark cell. Its tooltip names the Group's
/// Projects.
fn group_disclosure(group: GroupId, projects: Option<SharedString>) -> Stateful<Div> {
    let tip = match projects {
        Some(projects) => format!("Group \u{b7} {projects}"),
        None => "Group".to_string(),
    };
    cell(disclosure(true))
        .id(("nav-group-icon", group.get() as usize))
        .debug_selector(move || format!("nav-group-icon-{}", group.get()))
        .tooltip(crate::menu::tooltip(tip))
}

/// The members container: the member rows, flush under their Group, each
/// carrying its own tree glyph (`RowPlace::Member`).
pub fn members(rows: Vec<AnyElement>) -> Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .gap(px(MEMBER_GAP))
        .mt(px(MEMBERS_TOP))
        .children(rows)
}

/// The Thread row, one line, at root — see `project_thread_row_with_title`.
#[cfg(test)]
pub fn thread_row(row: &ThreadRow) -> Stateful<Div> {
    project_thread_row_with_title(row, row.name.clone(), RowPlace::Root, false, false)
}

/// The one Thread row builder, with the title leaf supplied by the caller —
/// see `group_row_with_title`. One line: the cursor cell (`❯` when
/// selected), a member's tree glyph (`place`), the status mark, the title
/// pinned to what the row leaves it, and the word at the right.
///
/// `grouped` is Project order's membership mark: a Thread that is still a
/// Group member says so with the faint Group glyph after its title.
/// `editing` is a rename in progress: the field takes the title's box and
/// the word's.
pub fn project_thread_row_with_title(
    row: &ThreadRow,
    title: impl IntoElement,
    place: RowPlace,
    grouped: bool,
    editing: bool,
) -> Stateful<Div> {
    let thread = row.thread;
    let word = (!editing).then(|| row.tail.text()).flatten();
    let marked = grouped && !editing;
    let cells = match place {
        RowPlace::Root => 2.0,
        RowPlace::Member { .. } => 3.0,
    } + if marked { 1.0 } else { 0.0 };
    let frame = row_frame(("nav-thread", thread.get() as usize), row.current)
        .debug_selector(move || format!("nav-thread-{}", thread.get()))
        .tooltip(row_tooltip(row))
        .child(cursor_cell(row.current));
    let frame = match place {
        RowPlace::Root => frame,
        RowPlace::Member { last } => frame.child(tree_cell(last)),
    };
    frame
        .child(cell(status_mark(row)))
        .child(
            fitted(
                title_w(cells, word.as_deref()),
                title_ink(row),
                W_BODY,
                title,
            )
            .debug_selector(move || format!("nav-title-{}", thread.get())),
        )
        .children(marked.then(|| group_membership_indicator(thread)))
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
    line()
        .id(("nav-needs", thread.get() as usize))
        .debug_selector(move || format!("nav-needs-{}", thread.get()))
        .hover_row(key)
        .press_row()
        .tooltip(row_tooltip(row))
        .child(cell(components::status_dot(ATTENTION)))
        .child(fitted(
            title_w(1.0, Some(entry.kind)),
            TEXT,
            W_BODY,
            row.name.clone(),
        ))
        .child(word_cell(Some(entry.kind.into())))
}

/// The strip's header: `needs you N`, dim, and `⌘D` — the key that answers
/// it — at the right, as its keycap reads (`key_combo`).
pub fn needs_you_header(count: usize) -> Div {
    let key = components::bound_chord("cockpit::NextDecision")
        .map(|keys| components::key_combo(&keys, TEXT_MUTED));
    line()
        .debug_selector(|| "nav-needs-you".into())
        .text_color(rgb(TEXT_MUTED))
        .child(SharedString::from(format!("{} {count}", words::NEEDS_YOU)))
        .children(key.map(|key| div().flex().ml_auto().child(key)))
}

/// The strip: its header and one row per waiting Thread, pinned under the
/// head (it does not scroll with the tree), a half row above the tree.
pub fn needs_you_strip() -> Div {
    div()
        .debug_selector(|| "nav-needs-you-strip".into())
        .flex()
        .flex_col()
        .flex_shrink_0()
        .pb(px(NAV_SECTION_GAP))
}

/// A row's tooltip: the whole title, then its other facts in the order
/// the Pane names them — provider, Project, branch, subagents — and the ⌘
/// digit that lands on it.
fn row_tooltip(row: &ThreadRow) -> impl Fn(&mut gpui::Window, &mut App) -> gpui::AnyView {
    let mut facts: Vec<String> = Vec::new();
    if let Some(provider) = row.provider {
        facts.push(
            match provider {
                Provider::Claude => "claude",
                Provider::Codex => "codex",
            }
            .to_string(),
        );
    }
    match (&row.project, &row.branch) {
        (Some(project), Some(branch)) => facts.push(format!("{project} on {branch}")),
        (Some(project), None) => facts.push(project.to_string()),
        (None, Some(branch)) => facts.push(branch.to_string()),
        (None, None) => {}
    }
    match row.subagents {
        0 => {}
        1 => facts.push("1 subagent".to_string()),
        count => facts.push(format!("{count} subagents")),
    }
    let text = if facts.is_empty() {
        row.name.to_string()
    } else {
        format!("{}\n{}", row.name, facts.join(" \u{b7} "))
    };
    let key = row
        .ordinal
        .and_then(crate::cockpit::focus_rail_action)
        .and_then(components::bound_chord);
    crate::menu::tooltip_with_key(text, key)
}

/// The focused Thread's title is the strongest ink in the tree, and so is
/// an unread one's: both ask to be read. A parked Thread's steps down to
/// the metadata ink, so what is running reads first. Ink only: the weight
/// is always `W_BODY`, so a row never reflows when it is read.
fn title_ink(row: &ThreadRow) -> u32 {
    if row.current || row.unread {
        TEXT_STRONG
    } else if row.status == RowStatus::Parked {
        TEXT_MUTED
    } else {
        TEXT
    }
}

/// The Group glyph after a title in Project order, in its own 2ch cell.
/// Project order flattens Groups into their Projects, so this keeps durable
/// membership visible: faint, structure rather than state.
fn group_membership_indicator(thread: ThreadId) -> Stateful<Div> {
    cell(icon(icons::GROUP, NAV_GLYPH, TEXT_FAINT))
        .id(("nav-group-membership", thread.get() as usize))
        .debug_selector(move || format!("nav-group-membership-{}", thread.get()))
        .ml(px(NAV_WORD_GAP))
        .tooltip(|window, cx| Tooltip::new("In a group").build(window, cx))
}

// ------------------------------------------------------------- Project order

/// A Project heading in Project order, one line: the faint `▾` where a
/// row's cursor sits, the Project's name in `W_STRONG` `TEXT_STRONG`, and
/// its branch, dim, at the right. The caller hangs `project_add_button` on
/// it: the heading is the only place a Project is named in this view, so it
/// is where a new Thread in that Project is asked for. Under the pointer
/// the branch gives its place to the door.
pub fn project_section(
    index: usize,
    label: SharedString,
    branch: Option<SharedString>,
    first: bool,
) -> Stateful<Div> {
    let branch = branch.map(|branch| short_branch(&branch));
    let key = SharedString::from(format!("nav-project-section-{index}"));
    let ink = crate::motion::hover_blend(&key, rgb(TEXT_MUTED).into(), rgba(TRANSPARENT).into());
    line()
        .id(("nav-project-section", index))
        .debug_selector(move || format!("nav-project-section-{index}"))
        .on_hover(project_section_hover(index))
        .when(!first, |section| section.mt(px(NAV_SECTION_GAP)))
        .child(cell(disclosure(true)))
        .child(fitted(
            title_w(1.0, branch.as_deref()),
            TEXT_STRONG,
            W_STRONG,
            label,
        ))
        .child(word_cell(branch).text_color(ink))
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

/// New Thread in *this* Project: an icon door laid over the heading's
/// right end, so the heading never reflows when it shows. Its glyph has no
/// ink at rest: it reaches `TEXT_MUTED` while the pointer is on the heading
/// (the 150ms hover blend, as the branch fades out from under it), and
/// `TEXT` under the pointer itself. The keyboard reaches it as a tab stop
/// and finds it by its focus ring.
pub fn project_add_button(index: usize, project: &str, cx: &App) -> Button {
    let key = SharedString::from(format!("nav-project-section-{index}"));
    let rest = crate::motion::hover_blend(&key, rgba(TRANSPARENT).into(), rgb(TEXT_MUTED).into());
    door_frame(("nav-project-add", index), cx)
        .tab_stop(true)
        .debug_selector(move || format!("nav-project-add-{index}"))
        .absolute()
        .top_0()
        .right_0()
        .tip(format!("New thread in {project}"))
        .accessibility_label(format!("New thread in {project}"))
        .child(
            icon(icons::PLUS, NAV_DOOR_GLYPH, TEXT_MUTED)
                .text_color(rest)
                .group_hover(DOOR_GROUP, |style| style.text_color(rgb(TEXT))),
        )
}

/// The hover listener that drives a Project heading's blend (the branch
/// fading out, the door's glyph fading in).
pub fn project_section_hover(index: usize) -> impl Fn(&bool, &mut gpui::Window, &mut App) {
    crate::motion::hover_listener(format!("nav-project-section-{index}").into())
}

// ------------------------------------------------------------- solos and empties

/// One run of solo Threads — those no Group claims — at root. A run is
/// however many solo rows the recency order happens to put together
/// between two Groups, so the tree holds several; each is a place to drop
/// a row to get it out of its Group, and each carries its own id.
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
        .gap(px(MEMBER_GAP))
        .children(rows)
}

/// The empty ground under the last row: the tree's own remainder, and the
/// drop target that gets a row out of its Group when every Thread is in
/// one and there is no solo run to aim at.
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
/// section's own menu. The cockpit wires both.
pub fn parked_header(count: usize, open: bool, eased: bool) -> Stateful<Div> {
    // One `▸` that turns a quarter to `▾` when open: over 150ms on a
    // pointer toggle, at once on a keyboard or menu toggle (rule 2.10.5).
    let triangle = |turn: f32| {
        disclosure(false)
            .group_hover(PARKED_GROUP, |style| style.text_color(rgb(TEXT_MUTED)))
            .with_transformation(Transformation::rotate(radians(
                std::f32::consts::FRAC_PI_2 * turn,
            )))
    };
    let mark = if eased {
        crate::motion::settled("nav-parked-chevron", open, crate::motion::TURN, triangle)
            .into_any_element()
    } else {
        triangle(if open { 1. } else { 0. }).into_any_element()
    };
    line()
        .id(("nav-parked", 0usize))
        .debug_selector(|| "nav-parked".into())
        .group(PARKED_GROUP)
        .text_color(rgb(TEXT_MUTED))
        .hover_row("nav-parked")
        .press_row()
        .child(cell(mark))
        .child(
            div()
                .min_w_0()
                .truncate()
                .group_hover(PARKED_GROUP, |style| style.text_color(rgb(TEXT)))
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
        .gap(px(MEMBER_GAP))
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

/// The frame every tree row shares: one line, square, the fill language.
/// The height is fixed so a row that cannot resolve its Project or its
/// checkout still occupies exactly the space it will once the cache fills.
///
/// `selected` is only ever the focused Thread's row, the tree's one
/// selection: hover cannot wash over a ground stronger than itself, so
/// that row steps its ground up instead (`SELECTION` → `SELECTION_HOVER`).
fn row_frame(id: (&'static str, usize), selected: bool) -> Stateful<Div> {
    let frame = line().id(id);
    // The hover face fades in and out (`motion::HOVER_FADE`): the pointer
    // sweeps these rows constantly, so a snap would flicker the column.
    let key = SharedString::from(format!("{}-{}", id.0, id.1));
    let frame = if selected {
        frame.hover_carried(key).press_row()
    } else {
        frame.hover_row(key).press_row()
    };
    // Rows are draggable into Groups, so they wear the open hand rather than
    // the pointer: the drag is the row's second verb, and the only one the
    // cursor can advertise before the press. It is set **after** the hover
    // role, whose `cursor_pointer` would otherwise overwrite it — the roles
    // in `pointer.rs` set the base cursor, not a hover refinement.
    frame.cursor(CursorStyle::OpenHand)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::ThreadId;

    fn thread(provider: Option<Provider>) -> ThreadRow {
        current_thread(provider, false)
    }

    fn current_thread(provider: Option<Provider>, current: bool) -> ThreadRow {
        ThreadRow {
            thread: ThreadId::new(8),
            status: RowStatus::Idle,
            name: "thread-08".into(),
            project: Some("ferrite".into()),
            branch: Some("feat/ui-overhaul".into()),
            provider,
            current,
            unread: false,
            tail: NavTail::Age("2h".into()),
            subagents: 2,
            ordinal: None,
        }
    }

    fn group() -> GroupBlock {
        GroupBlock {
            id: GroupId::new(1),
            title: "Project-scoped navigation prototype".into(),
            projects: Some("ferrite".into()),
            members: vec![current_thread(Some(Provider::Codex), true)],
        }
    }

    fn nav_state(rows: Vec<ThreadRow>, needs_you: Vec<NeedsYouRow>) -> NavState {
        let count = rows.len();
        NavState {
            filter: FilterState {
                label: "All projects".into(),
                open: false,
                options: Vec::new(),
            },
            groups: Vec::new(),
            solos: rows,
            parked: Vec::new(),
            parked_open: false,
            order: (0..count).map(NavItem::Solo).collect(),
            project_sections: Vec::new(),
            thread_list_order: ThreadListOrder::Recent,
            order_open: false,
            collapsed: false,
            needs_you,
        }
    }

    /// The terminal grammar: square rows with no radius, the filter over the
    /// tree a dim body-weight line, every item one 20px line, and a
    /// Project heading a half row below the section above it.
    #[test]
    fn nav_rows_are_square_terminal_lines() {
        let mut row = thread_row(&thread(Some(Provider::Claude)));
        let style = row.style();
        assert_eq!(style.corner_radii.top_left, None, "square: no radius");
        assert_eq!(style.size.height, Some(px(NAV_LINE).into()));
        assert_eq!(style.padding.left, Some(px(NAV_PAD_X).into()), "1ch in");
        let mut filter = filter_trigger(&FilterState {
            label: "All projects".into(),
            open: false,
            options: Vec::new(),
        });
        let style = filter.style();
        assert_eq!(style.text.font_weight, Some(W_BODY));
        assert_eq!(style.text.color, Some(rgb(TEXT_MUTED).into()));
        assert_eq!(style.size.height, Some(px(NAV_LINE).into()));
        let mut heading = project_section(1, "ferrite".into(), Some("dev".into()), false);
        assert_eq!(
            heading.style().margin.top,
            Some(px(NAV_SECTION_GAP).into()),
            "a half row above every Project heading but the first"
        );
        assert_eq!(NAV_LINE, ROW);
        assert_eq!(NAV_SECTION_GAP, HALF_ROW);
        assert_eq!(NAV_PAD_X, CH);
        assert_eq!(NAV_CELL, 2.0 * CH);
    }

    /// The selection rule: one selection in the whole tree, on the focused
    /// Thread's row. The Group holding it stays clear — two stacked
    /// selections would say two things are selected — and a Thread that
    /// merely sits in the current Group is not itself current.
    #[test]
    fn one_selection_marks_the_focused_thread() {
        let fill = |mut drawn: Stateful<Div>| drawn.style().background.clone();
        assert_eq!(
            fill(group_row(&group())),
            None,
            "the Group holding the focused Thread draws no fill of its own"
        );
        assert_eq!(
            fill(thread_row(&current_thread(Some(Provider::Claude), true))),
            Some(gpui::Hsla::from(paint::SELECTION).into()),
            "the focused Thread's own row carries the tree's one selection"
        );
        assert_eq!(fill(thread_row(&thread(Some(Provider::Claude)))), None);
        assert_eq!(
            fill(project_thread_row_with_title(
                &current_thread(None, true),
                "thread-08",
                RowPlace::Member { last: true },
                true,
                false
            )),
            Some(gpui::Hsla::from(paint::SELECTION).into()),
            "a member and Project order mark the same row the same way"
        );
    }

    /// The focused title is the strongest ink in the tree, an unread one is
    /// as strong, a parked title steps down to the metadata ink, and every
    /// other title is the body ink. Unread changes ink only, never weight.
    #[test]
    fn titles_rank_focus_then_running_then_parked() {
        assert_eq!(title_ink(&current_thread(None, true)), TEXT_STRONG);
        assert_eq!(title_ink(&thread(None)), TEXT);
        let parked = ThreadRow {
            status: RowStatus::Parked,
            ..thread(None)
        };
        assert_eq!(title_ink(&parked), TEXT_MUTED);
        let unread = ThreadRow {
            unread: true,
            ..thread(None)
        };
        assert_eq!(title_ink(&unread), TEXT_STRONG);
        assert_eq!(
            title_ink(&ThreadRow {
                status: RowStatus::Parked,
                ..unread.clone()
            }),
            TEXT_STRONG,
            "unread outranks the parked step-down"
        );
        assert_eq!(
            title_ink(&ThreadRow {
                current: true,
                ..unread.clone()
            }),
            TEXT_STRONG
        );
    }

    /// Every expanded row is a drag source before it is a button, so it
    /// wears the open hand whether or not it is current.
    #[test]
    fn every_draggable_row_advertises_its_grab() {
        let cursor = |mut drawn: Stateful<Div>| drawn.style().mouse_cursor;
        assert_eq!(
            cursor(thread_row(&thread(None))),
            Some(CursorStyle::OpenHand)
        );
        assert_eq!(
            cursor(thread_row(&current_thread(None, true))),
            Some(CursorStyle::OpenHand)
        );
        assert_eq!(cursor(group_row(&group())), Some(CursorStyle::OpenHand));
    }

    #[gpui::test]
    fn the_order_button_is_clear_at_rest_and_selected_while_open(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            crate::theme::init_components(cx);
            let background = |mut button: Button| button.style().background.clone();
            assert_eq!(background(order_button(false, false, cx)), None);
            assert_eq!(background(order_button(true, false, cx)), None);
            assert_eq!(
                background(order_button(false, true, cx)),
                Some(gpui::Hsla::from(paint::SELECTION).into())
            );
        });
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
            branch: None,
            provider: None,
            current: false,
            unread: false,
            tail: NavTail::None,
            subagents: 0,
            ordinal: None,
        };
        let height = |mut drawn: Stateful<Div>| drawn.style().size.height;
        let line = Some(px(NAV_LINE).into());
        assert_eq!(height(thread_row(&bare)), line);
        assert_eq!(height(thread_row(&thread(None))), line);
        assert_eq!(height(thread_row(&current_thread(None, true))), line);
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
        assert_eq!(height(group_row(&group())), line);
        assert_eq!(
            height(project_thread_row_with_title(
                &bare,
                "thread-09",
                RowPlace::Member { last: false },
                true,
                false
            )),
            line
        );
        let entry = NeedsYouRow {
            row: bare.clone(),
            kind: words::QUESTION,
        };
        assert_eq!(height(needs_you_row(&entry)), line);
        assert_eq!(height(parked_header(3, false, false)), line);
        assert_eq!(height(parked_header(3, true, false)), line);
        assert_eq!(
            height(project_section(0, "ferrite".into(), None, true)),
            line
        );
    }

    /// The title box is pinned to what its row leaves it — the text width
    /// less its cells and its word — so it truncates, and every row's word
    /// ends on the one right edge.
    #[test]
    fn a_title_takes_what_its_cells_and_word_leave() {
        assert_eq!(NAV_TEXT_W, WIDTH - 2.0 * CH);
        assert_eq!(title_w(2.0, None), NAV_TEXT_W - 4.0 * CH);
        assert_eq!(
            title_w(2.0, Some("1m04s")),
            NAV_TEXT_W - 4.0 * CH - 5.0 * CH - CH,
            "five cells and the 1ch gap"
        );
        assert_eq!(
            title_w(3.0, Some("done")),
            title_w(2.0, Some("done")) - NAV_CELL,
            "a member's tree glyph takes one more cell"
        );
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
            "a working row says how long it has worked"
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
    /// metadata ink whether read or not (unread is the title's ink, never
    /// the dot's; before the redesign it was the accent). Nothing pulses.
    #[test]
    fn status_dots_say_state_and_green_only_means_live() {
        let face = |status, unread| {
            dot_face(&ThreadRow {
                status,
                unread,
                ..thread(None)
            })
        };
        let fill = |status| face(status, false).style().background.clone();
        assert_eq!(fill(RowStatus::Working), Some(rgb(RUNNING).into()));
        assert_eq!(fill(RowStatus::Failing), Some(rgb(BLOCKED).into()));
        assert_eq!(fill(RowStatus::Failed), Some(rgb(BLOCKED).into()));
        assert_eq!(fill(RowStatus::NeedsYou), Some(rgb(ATTENTION).into()));
        assert_eq!(fill(RowStatus::Idle), Some(rgb(IDLE).into()));
        assert_eq!(
            face(RowStatus::Idle, true).style().background,
            Some(rgb(IDLE).into()),
            "an unread quiet row keeps its dot: the title carries unread"
        );
        for status in [
            RowStatus::Working,
            RowStatus::Failing,
            RowStatus::NeedsYou,
            RowStatus::Failed,
        ] {
            assert_eq!(
                face(status, true).style().background,
                fill(status),
                "{status:?}: a live state is the louder truth"
            );
        }
    }

    /// One status truth: for every Pane state but parked, and either side of
    /// unread, the nav row's still dot is the Pane's own dot. A parked
    /// Thread is the one step the nav takes: its ring is faint (the
    /// prototype's `○`).
    #[test]
    fn the_nav_dot_is_the_panes_dot() {
        use WallState::*;
        let paint = |mut dot: Div| {
            let style = dot.style();
            (style.background.clone(), style.border_color)
        };
        for state in [Working, Failing, Decision, Blocked, Done, Idle, Parked] {
            for unread in [false, true] {
                let row = ThreadRow {
                    status: RowStatus::of(state),
                    unread,
                    ..thread(None)
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
        assert_eq!(ring.style().border_color, Some(rgb(TEXT_FAINT).into()));
    }

    /// The column: chrome, one plane, no hairline to the content; open at
    /// `WIDTH`, folded to nothing.
    #[test]
    fn the_column_is_chrome_with_no_hairline() {
        let mut column = shell(false);
        let style = column.style();
        assert_eq!(
            style.background,
            Some(gpui::Hsla::from(paint::CHROME).into())
        );
        let edges = &style.border_widths;
        assert!(edges.top.is_none() && edges.right.is_none());
        assert!(edges.bottom.is_none() && edges.left.is_none());
        assert_eq!(style.size.width, Some(px(WIDTH).into()));
        let mut folded = shell(true);
        assert_eq!(folded.style().size.width, Some(px(FOLDED_WIDTH).into()));
        assert_eq!(FOLDED_WIDTH, 0.0, "folded, the board takes the width");
    }

    /// The titlebar cell rides with the column: the column's own width when
    /// open, the folded cell — lights, toggle, bell (and the gear off
    /// macOS) — when folded, and
    /// in between as the column moves. Over the folded column it paints the
    /// reading plane; over the open column, nothing.
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
        assert_eq!(
            NAV_CHROME_FOLDED_W,
            NAV_CHROME_LEAD + NAV_CHROME_DOORS * ICON_BUTTON + NAV_PAD_X
        );
        let mut open = chrome_band(WIDTH, WIDTH);
        assert_eq!(open.style().background, None, "the column's chrome shows");
        assert_eq!(open.style().size.height, Some(px(WIN_CHROME_H).into()));
    }

    /// Threads that need you pin to the top of the ⌘ order in the answer
    /// order; every other row keeps the tree's order. The first nine rows
    /// carry their digit, wherever they are drawn.
    #[test]
    fn the_command_digits_pin_threads_that_need_you_first() {
        let row = |id: u64| ThreadRow {
            thread: ThreadId::new(id),
            ..thread(None)
        };
        let mut state = nav_state(
            vec![row(1), row(2), row(3), row(4)],
            vec![
                NeedsYouRow {
                    row: row(4),
                    kind: words::APPROVAL,
                },
                NeedsYouRow {
                    row: row(2),
                    kind: words::QUESTION,
                },
            ],
        );
        let ids: Vec<u64> = state
            .rail_rows()
            .iter()
            .map(|row| row.thread.get())
            .collect();
        assert_eq!(ids, vec![4, 2, 1, 3]);
        state.number_rows();
        let ordinals: Vec<(u64, Option<usize>)> = state
            .solos
            .iter()
            .map(|row| (row.thread.get(), row.ordinal))
            .collect();
        assert_eq!(
            ordinals,
            vec![(1, Some(3)), (2, Some(2)), (3, Some(4)), (4, Some(1))]
        );
        assert_eq!(state.needs_you[0].row.ordinal, Some(1));
        let mut many = nav_state((1..=12).map(row).collect(), Vec::new());
        many.number_rows();
        assert_eq!(many.solos[8].ordinal, Some(9));
        assert_eq!(many.solos[9].ordinal, None, "nothing past ⌘9");
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
            header.style().mouse_cursor,
            Some(CursorStyle::PointingHand),
            "the header is pressed like a row"
        );
    }
}
