//! The left navigation column (#21): one Project filter, then the Groups
//! with their member Threads indented under a rail, then the solo Threads
//! at root — and, at the foot of the column, the **Parked** section: every
//! parked Thread no Group claims, folded shut by default so the tree above
//! holds only what is running. It is a view, never the only door:
//! everything a row does (focus, revive, regroup) stays reachable from the
//! keyboard.
//!
//! Drawing only, like `pane.rs`: the cockpit assembles a `NavState` per
//! frame from O(1) reads plus its project/branch/parked caches —
//! `Store::load` and `Instruments::of` are banned here, which is what keeps
//! the 24-Pane wall smooth with the nav open. Click wiring stays in
//! `cockpit.rs`, the same split `pane_cell` uses.
//!
//! **One column grid, on the window's own ground.** The nav is `GROUND`,
//! the plane the Panes sit on, with no edge: the Panes separate themselves.
//! Every row lays out lead slot · text · tail · mark (see the WP-G section
//! of `theme.rs`), so a Thread's dot, a Group's glyph, the Parked chevron
//! and the head's folder share one axis, and every title and label starts
//! on the next.
//!
//! **One 28px Geist line per row** (`NAV_ROW_H`): the dot, the title in
//! `FS_UI` `W_BODY`, the branch inline only when it is not the Project's
//! default, then the tail — one state word or an age, never `now` — and the
//! provider mark in its brand colour. A Group title is the one `W_LABEL`
//! title of its block; metadata is `TEXT_MUTED`. Colour is state (and the
//! provider's brand, on its mark alone): a working dot is a still sage dot,
//! and only unread breathes. Selection is one `FILL` on the focused
//! Thread's row with its title in `TEXT_STRONG`, and no ring — nothing else
//! in the tree fills.
//!
//! While any Thread waits on the operator, the **Needs-you strip** sits
//! under the head: its rows are the answer order, and its first row is what
//! ⌘D and the wall's `y`/`n`/`a` act on.
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
    div, px, radians, relative, rgb, rgba, AnyElement, App, CursorStyle, Div, ScrollHandle,
    SharedString, Stateful, Transformation,
};

use crate::cockpit::thread_status;
use crate::components;
use crate::icons::{self, icon};
use crate::pane::WallState;
use crate::pointer::{Pointer, PointerFaded, PointerPressed};
use crate::theme::*;

/// The nav's two widths—286px, and the platform rail cmd-b folds it to.
/// macOS uses the traffic-light reserve; other platforms use 56px.
/// `CockpitView::cell()` subtracts whichever is live, so the nav stays part
/// of the semantic-zoom input rather than a special case.
pub use crate::theme::{NAV_RAIL_WIDTH as RAIL_WIDTH, NAV_WIDTH as WIDTH};

/// A row's one line: `FS_UI` on its 20px line box, inside the 28px row.
/// A row keeps its height whatever its facts, so nothing reflows when a
/// cache fills or a word arrives.
const TITLE_H: f32 = LH_UI;

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

/// The two icon buttons tint their glyph on hover, and a child SVG paints
/// from its **own** style — an ambient text colour reaches text but never an
/// `svg()`. `group_hover` is the only mechanism that carries a parent's
/// hover down to a child's colour, so each button names a group.
const COLLAPSE_GROUP: &str = "nav-collapse";
const RAIL_FILTER_GROUP: &str = "nav-rail-filter";
const FILTER_GROUP: &str = "nav-filter";
const ORDER_GROUP: &str = "nav-order";
const PROJECT_SECTION_GROUP: &str = "nav-project-section";
const PROJECT_ADD_GROUP: &str = "nav-project-add";
const PARKED_GROUP: &str = "nav-parked";
const RAIL_ITEM_GROUP: &str = "nav-rail-item";

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
/// with what it waits for (`approval` / `question`) as its tail.
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
    pub rows: Vec<ThreadRow>,
}

impl NavState {
    /// Every row in the order the tree draws it: a Group's members where
    /// their Group sits, a solo where it sits. The Parked section is not
    /// the tree, so its rows are not here. The rail folds to exactly this
    /// sequence, and tests read the tree's order from it.
    pub fn ordered_rows(&self) -> Vec<&ThreadRow> {
        self.order
            .iter()
            .flat_map(|item| match item {
                NavItem::Group(index) => self.groups[*index].members.iter(),
                NavItem::Solo(index) => std::slice::from_ref(&self.solos[*index]).iter(),
            })
            .collect()
    }

    /// The rail's order: every Thread that needs you pinned first, in the
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

    /// The solo Threads alone, in the tree's order.
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

/// One Group and the Threads indented under it.
pub struct GroupBlock {
    pub id: GroupId,
    pub title: SharedString,
    /// One Project's name, or the count of Projects across the whole Group.
    /// None when no member resolves one. The row does not print it; it is
    /// the Group glyph's tooltip.
    pub projects: Option<SharedString>,
    pub members: Vec<ThreadRow>,
}

/// One Thread's row — identical whether it is a Group member or a solo; only
/// the container differs. One line: status dot, title, the branch when it
/// is not the default, the tail, the provider mark.
#[derive(Clone)]
pub struct ThreadRow {
    pub thread: ThreadId,
    pub name: SharedString,
    /// What the Thread is doing right now — the one glance the operator
    /// asked for from the tree: which agents are working, which wait.
    pub status: RowStatus,
    /// The Project's name: what the filter and the Project sections read.
    /// The row itself does not print it.
    pub project: Option<SharedString>,
    /// The branch the Thread's checkout is on, from the facts cache, only
    /// when it is not the Project's default (`ThreadFacts::off_default_branch`).
    /// `None` says nothing; it is never guessed.
    pub branch: Option<SharedString>,
    /// `None` → no logomark. Never a `cl`/`cx` string.
    pub provider: Option<Provider>,
    /// This is the focused Pane's Thread: it carries the tree's one selected
    /// fill and the `TEXT_STRONG` title.
    pub current: bool,
    /// The Thread finished while the operator looked elsewhere (an unread
    /// Notice). Its own axis, never a state: a quiet unread row wears the
    /// unread dot and a `TEXT_STRONG` title, ink only, never weight.
    pub unread: bool,
    /// The one word or age at the row's right (C10).
    pub tail: NavTail,
    /// Subagents known for this Thread. Zero draws nothing; a positive
    /// count shows before an age (never beside a state word) and is named
    /// in the tooltip.
    pub subagents: usize,
}

/// What a row's tail says (C10), in priority order: a pending Decision is
/// `needs you` (`ATTENTION`); red tests `failing`/`failing N` and a failed
/// turn or closed Session `failed` (`BLOCKED`); an unread finish `done`
/// (`TEXT_MUTED`); a working row nothing at all (its sage dot says it);
/// otherwise the age, once it reaches a minute (`facts::since_label` says
/// nothing before that). Never `now`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavTail {
    NeedsYou,
    Failing(Option<u32>),
    Failed,
    Done,
    Age(SharedString),
    None,
}

impl NavTail {
    /// The tail from the Pane's own reading of the Thread (`pane::thread_face`
    /// — the head's slot word), so the nav and the Pane never disagree: a
    /// waiting request is `needs you`, red tests `failing N`, a failed turn
    /// or closed Session `failed`, an unread finish `done`, a working Thread
    /// nothing; everything else (idle, interrupted, parked) is its age.
    pub fn of(slot: Option<&crate::pane::HeadSlot>, unread: bool, age: SharedString) -> Self {
        use crate::pane::HeadSlot;
        match slot {
            Some(HeadSlot::NeedsYou(_)) => NavTail::NeedsYou,
            Some(HeadSlot::Failing(count)) => {
                NavTail::Failing(count.map(|count| count.min(u32::MAX as usize) as u32))
            }
            Some(HeadSlot::Failed) => NavTail::Failed,
            Some(HeadSlot::Working(_)) => NavTail::None,
            Some(HeadSlot::Done) if unread => NavTail::Done,
            _ => NavTail::Age(age),
        }
    }

    /// The tail's words, from the lexicon (`theme::words`), and their ink.
    /// An empty age says nothing.
    pub fn face(&self) -> Option<(SharedString, u32)> {
        match self {
            NavTail::NeedsYou => Some((words::NEEDS_YOU.into(), ATTENTION)),
            NavTail::Failing(Some(count)) => {
                Some((format!("{} {count}", words::FAILING).into(), BLOCKED))
            }
            NavTail::Failing(None) => Some((words::FAILING.into(), BLOCKED)),
            NavTail::Failed => Some((words::FAILED.into(), BLOCKED)),
            NavTail::Done => Some((words::DONE.into(), TEXT_MUTED)),
            NavTail::Age(age) if !age.is_empty() => Some((age.clone(), TEXT_MUTED)),
            NavTail::Age(_) | NavTail::None => None,
        }
    }

    /// A state word, as against an age or nothing: a word hides the
    /// subagent count, so the row never spends two facts in its tail.
    pub fn is_word(&self) -> bool {
        !matches!(self, NavTail::Age(_) | NavTail::None)
    }
}

/// A Thread row's state, for its dot. The nav's original no-dot ruling
/// gave way to the operator's need to see, from the tree, which Threads
/// are working and which sit idle or wait on them. The face itself comes
/// from `cockpit::thread_status`, the one status truth the Panes share.
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

/// The status dot before a row's title, one recipe for the tree and the
/// rail (`thread_status`): running sage (the only green in the column, and
/// it means live), a Decision ochre, closed or failing red, unread the
/// accent, idle the idle ink, and parked a hollow ring.
///
/// **Every dot is still but unread's** (C11). Working is the normal state,
/// so it does not move; motion means "look here". An unread row's `ACCENT`
/// dot breathes its own opacity alone (`PULSE_MIN`..1) on the shared clock
/// at `MOTION_BREATH_MS`, with no halo, and holds at full ink under reduced
/// motion. The box is a fixed `STATUS_DOT` either way, so nothing moves.
fn status_dot(row: &ThreadRow, reduce_motion: bool) -> AnyElement {
    let face = thread_status(row.status.wall(), row.unread);
    if breathes(row) {
        if reduce_motion {
            return face.dot().into_any_element();
        }
        return components::breathing_dot(face.ink, false);
    }
    face.dot().into_any_element()
}

/// Whether a row's dot breathes: an unread row whose state lets the unread
/// face show (a live state is the louder truth and stays still).
fn breathes(row: &ThreadRow) -> bool {
    row.unread && thread_status(row.status.wall(), true).ink == ACCENT
}

/// The still face of a row: the dot alone, no halo — the same face the
/// Thread's Pane draws.
fn dot_face(row: &ThreadRow) -> Div {
    thread_status(row.status.wall(), row.unread).dot()
}

/// The lead slot: `NAV_LEAD_W` wide, one title line high, its glyph
/// centred — so a dot, a Group glyph or a chevron centres on the first
/// line's box, and the text after it starts at `NAV_TEXT_X`.
fn lead(glyph: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(NAV_LEAD_W))
        .h(px(TITLE_H))
        .child(glyph)
}

/// The nav column itself: full height, on `GROUND` — the window's own
/// plane — with **no edge on any side**. The Panes sit on the same ground
/// and separate themselves by their own plane and hairline; the nav is the
/// field they lie on, not a slab of its own. The ground is painted
/// explicitly because the width animation slides the column over the board.
///
/// Nothing is clipped here — the tree scrolls itself.
pub fn shell(collapsed: bool) -> Div {
    div()
        .debug_selector(|| "nav-column".into())
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(if collapsed { RAIL_WIDTH } else { WIDTH }))
        .overflow_hidden()
        .bg(rgb(NAV))
        .font_family(FONT_UI)
}

/// The 42px window-chrome band at the top of the column.
///
/// On macOS the traffic lights are the **host's**, positioned by
/// `TitlebarOptions`, so the band reserves their room rather than drawing
/// fakes: a `TRAFFIC_RESERVE`-wide spacer that holds nothing, so the
/// collapse button's left edge lands at x = 77. The binding fact is that
/// edge and the empty band before it — anything drawn or hit-testable in
/// that strip kills AppKit's drag region.
///
/// Everywhere else there are no lights to reserve for, and reserving anyway
/// is what pushed the collapse button 77px off the column it belongs to:
/// the band takes the row inset instead, so the sidebar glyph centres on
/// the same axis as every row's lead slot under it. The caption buttons sit at the *window's*
/// corner, not the column's — `titlebar.rs` draws them.
///
/// Collapsed the band becomes the rail's single expand control. On macOS
/// its top padding is a full titlebar band, keeping it clear of the native
/// traffic lights; rail actions follow below in the content column.
pub fn win_chrome(collapsed: bool) -> Div {
    if collapsed {
        return div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .items_center()
            .pt(px(NAV_RAIL_CHROME_PAD_T))
            .pb(px(NAV_RAIL_CHROME_PAD_B));
    }
    let band = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(WIN_CHROME_H))
        .pr(px(ROW_PAD_X));
    if crate::titlebar::CUSTOM {
        return band.pl(px(ROW_PAD_X));
    }
    band.child(div().flex_shrink_0().w(px(TRAFFIC_RESERVE)))
}

/// The collapse button and its 16px sidebar glyph. It grows with the macOS
/// rail while the expanded column keeps the compact 28px titlebar control.
pub fn collapse_button(collapsed: bool) -> Stateful<Div> {
    let size = if collapsed {
        NAV_RAIL_CONTROL
    } else {
        ICON_BUTTON
    };
    div()
        .id(("nav-collapse", 0usize))
        .group(COLLAPSE_GROUP)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(size))
        .h(px(size))
        .rounded(px(R_CONTROL))
        .hover_control()
        .press_control()
        .child(
            icon(icons::SIDEBAR, ICON_BUTTON_GLYPH, TEXT_MUTED)
                .group_hover(COLLAPSE_GROUP, |style| style.text_color(rgb(TEXT))),
        )
}

/// The 42px nav head. `relative`, because the filter menu hangs off it. The
/// caller supplies the trigger and, when open, the menu — and the menu must
/// be wrapped in `gpui::deferred(..)` so the scrolling tree below cannot
/// overpaint it. Its inline inset is the tree's, so the trigger's folder
/// lands in the rows' lead slot and the `+` glyph centres over their marks.
pub fn nav_head() -> Div {
    div()
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(NAV_HEAD_H))
        .px(px(NAV_TREE_PAD))
        .gap(px(NAV_HEAD_GAP))
}

/// The persistent door to a new Thread. It sits beside the Project filter,
/// reusing the same compact icon-control grammar as the rest of the nav.
/// The cockpit owns the click because opening a draft changes its roster.
pub fn add_thread_button(cx: &App) -> Button {
    components::icon_button("add-thread", icons::PLUS, "New thread", cx)
        .debug_selector(|| "add-thread".into())
}

/// The rail's primary creation door gets the same generous target as its
/// Thread avatars; the expanded header retains its denser 28px control.
pub fn rail_add_thread_button(cx: &App) -> Button {
    components::icon_button("rail-add-thread", icons::PLUS, "New thread", cx)
        .debug_selector(|| "rail-add-thread".into())
        .w(px(NAV_RAIL_CONTROL))
        .h(px(NAV_RAIL_CONTROL))
}

/// Easy-access ordering control beside New Thread. Its selected state is
/// visible even while the menu is closed.
pub fn order_button(active: bool, open: bool) -> Button {
    components::button("thread-list-order")
        .tab_stop(true)
        .debug_selector(|| "thread-list-order".into())
        .group(ORDER_GROUP)
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .when(open, |button| button.bg(rgb(FILL)))
        .tooltip("Thread order")
        .child(
            icon(
                icons::SORT,
                ICON_BUTTON_GLYPH,
                if active || open { TEXT } else { TEXT_MUTED },
            )
            .group_hover(ORDER_GROUP, |style| style.text_color(rgb(TEXT))),
        )
}

/// The order menu: a floating surface anchored under its button at the
/// head's right, a caption naming what the rows choose, then the rows.
pub fn order_menu() -> Div {
    components::floating_surface()
        .absolute()
        .top(px(MENU_TOP))
        .right(px(NAV_TREE_PAD))
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

/// A Project heading in Project order: the folder in the lead slot, the
/// Project's name and its row count (tabular) in the metadata voice — a
/// quiet label over its rows, not a card. The caller hangs
/// `project_add_button` on the end: the heading is the only place a Project
/// is named in this view, so it is where a new Thread in that Project is
/// asked for. No right inset, so the `+` glyph centres over the rows'
/// provider marks.
pub fn project_section(
    index: usize,
    label: SharedString,
    count: usize,
    first: bool,
) -> Stateful<Div> {
    div()
        .id(("nav-project-section", index))
        .on_hover(project_section_hover(index))
        .group(PROJECT_SECTION_GROUP)
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(NAV_SECTION_H))
        .when(!first, |section| section.mt(px(NAV_SECTION_GAP)))
        .pl(px(ROW_PAD_X))
        .child(lead(icon(icons::FOLDER, ROW_ICON, TEXT_FAINT)))
        .child(
            components::text_meta()
                .flex()
                .flex_1()
                .min_w_0()
                .gap(px(NAV_TAIL_GAP))
                .ml(px(NAV_LEAD_GAP))
                .child(div().min_w_0().truncate().child(label))
                .child(components::tabular(
                    div().flex_shrink_0().child(count.to_string()),
                )),
        )
}

/// New Thread in *this* Project. It keeps its 28px box at all times — the
/// heading never reflows when it shows — but its glyph has no ink at rest:
/// it reaches `TEXT_MUTED` while the pointer is on the heading (the 150ms
/// hover blend), and `TEXT` under the pointer itself. The keyboard reaches
/// it as a tab stop and finds it by its focus ring.
pub fn project_add_button(index: usize, project: &str) -> Button {
    let key = SharedString::from(format!("nav-project-section-{index}"));
    let rest = motion_ink(&key, TRANSPARENT, TEXT_MUTED);
    components::button(("nav-project-add", index))
        .tab_stop(true)
        .debug_selector(move || format!("nav-project-add-{index}"))
        .group(PROJECT_ADD_GROUP)
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .tooltip(format!("New thread in {project}"))
        .child(
            icon(icons::PLUS, ICON_BUTTON_GLYPH, TEXT_MUTED)
                .text_color(rest)
                .group_hover(PROJECT_ADD_GROUP, |style| style.text_color(rgb(TEXT))),
        )
}

/// A glyph ink riding the 150ms hover blend of `key` (`motion::hover_blend`)
/// between two inks, the resting one possibly transparent.
fn motion_ink(key: &str, rest: u32, hover: u32) -> gpui::Hsla {
    let rest = if rest == TRANSPARENT {
        rgba(rest).into()
    } else {
        rgb(rest).into()
    };
    crate::motion::hover_blend(key, rest, rgb(hover).into())
}

/// The hover listener that drives a Project heading's blend (`motion_ink`).
pub fn project_section_hover(index: usize) -> impl Fn(&bool, &mut gpui::Window, &mut App) {
    crate::motion::hover_listener(format!("nav-project-section-{index}").into())
}

/// The Project filter trigger: the head's one title. Its folder sits in the
/// rows' lead slot and its label on their text column; it rests on the
/// ground, and hover and open supply a face only while it is engaged.
pub fn filter_trigger(state: &FilterState) -> Stateful<Div> {
    let chevron = icon(icons::CHEVRON_DOWN, ICON_CHEVRON, TEXT_MUTED);
    let chevron = if state.open {
        chevron.with_transformation(Transformation::rotate(radians(std::f32::consts::PI)))
    } else {
        chevron
    };
    div()
        .id(("nav-filter", 0usize))
        .debug_selector(|| "nav-filter".into())
        .group(FILTER_GROUP)
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .h(px(FILTER_TRIGGER_H))
        .pl(px(ROW_PAD_X))
        .pr(px(ROW_PAD_X))
        .gap(px(NAV_LEAD_GAP))
        .rounded(px(R_CONTROL))
        .text_size(px(FS_UI))
        .font_weight(W_BODY)
        .line_height(px(LH_UI))
        // The filter is a quiet line over the tree, not a second toolbar:
        // body weight in `TEXT_2`, its glyphs muted. An open trigger wears
        // its hover face: the menu is the hover made permanent, so the
        // control does not blink when the pointer leaves.
        .when(state.open, |open| {
            open.bg(rgb(FILL))
                .text_color(rgb(TEXT_STRONG))
                .hover_carried()
                .press_row()
        })
        .when(!state.open, |shut| {
            shut.text_color(rgb(TEXT_2)).hover_control().press_control()
        })
        .child(lead(
            icon(icons::FOLDER, ROW_ICON, TEXT_MUTED)
                .group_hover(FILTER_GROUP, |style| style.text_color(rgb(TEXT))),
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .group_hover(FILTER_GROUP, |style| style.text_color(rgb(TEXT_STRONG)))
                .child(state.label.clone()),
        )
        .child(chevron)
}

/// The floating filter menu: the shared floating surface, spanning the
/// head under the trigger. The caller pushes `filter_option` rows, then a
/// separator and `filter_action`, and defers the whole thing.
pub fn filter_menu() -> Div {
    components::floating_surface()
        .absolute()
        .top(px(MENU_TOP))
        .left(px(NAV_TREE_PAD))
        .right(px(NAV_TREE_PAD))
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
/// the filter trigger, before the head's actions, so it reads as part of
/// the control that names the Project rather than one more thing to do
/// with the list. It is drawn only while the filter names a Project —
/// `All projects` is a filter state, not a Project, and has nothing to
/// edit.
pub fn project_edit_button() -> gpui::component::button::Button {
    components::button("project-edit")
        .tab_stop(true)
        .debug_selector(|| "project-edit".into())
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .tooltip("Edit project")
        .child(icon(icons::PENCIL, ROW_ICON, TEXT_MUTED))
}

/// The filter menu's last row: a verb, not an option — `Add project…`,
/// its label on the rows' own edge (no mark), `TEXT_MUTED` at rest and
/// `TEXT` under the pointer, on the raised hover face. The caller sets a
/// separator above it and wires the press to the folder picker.
pub fn filter_action(index: usize, label: &'static str) -> Stateful<Div> {
    components::menu_row_content(&components::MenuItem::new(label), false, false)
        .id(("nav-filter-action", index))
        .text_color(rgb(TEXT_MUTED))
        .cursor_pointer()
        .hover(|row| row.bg(rgb(FILL)).text_color(rgb(TEXT)))
        .press_raised()
}

/// The scrolling tree. It is the only thing in the column that scrolls, and
/// it carries the whole content inset: 8px top and inline, 16px bottom.
pub fn nav_tree(scroll: &ScrollHandle) -> Stateful<Div> {
    div()
        .id(("nav-tree", 0usize))
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(scroll)
        .pt(px(NAV_TREE_PAD))
        .px(px(NAV_TREE_PAD))
        .pb(px(NAV_TREE_PAD_B))
}

/// The nav tree's scrollbar, over the tree it scrolls. See
/// [`components::scrollbar`] for the shape and the sibling rule.
pub fn scrollbar(scroll: &ScrollHandle) -> Div {
    components::scrollbar("nav-scrollbar", scroll)
}

/// One Group section: the parent row, then its members. Blocks after the
/// first take a `GROUP_GAP` margin — the caller applies it from the index,
/// because only the caller knows which block is first once the filter has
/// run.
pub fn group_block() -> Div {
    div().relative().flex().flex_col().flex_shrink_0()
}

/// The `GROUP_GAP` band between two Group blocks — real air between the
/// blocks, doubling as the "insert between these two" drop target.
pub fn group_gap(index: usize) -> Stateful<Div> {
    div()
        .id(("group-gap", index))
        .debug_selector(move || format!("group-gap-{index}"))
        .flex_shrink_0()
        .h(px(GROUP_GAP))
}

/// "Insert above the first Group", which has no band of its own: the tree
/// starts at its own padding and draws nothing there. So the target is its
/// own absolute `NAV_DROP_BAND` hit band — laid over the first Group
/// header's top edge, taking no layout and, without `occlude`, stealing
/// none of its clicks either.
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

/// The 28px (`GROUP_ROW_H`) Group parent row, one line: the four-Pane Group
/// glyph (`TEXT_MUTED`) centred in the lead slot, the title — the block's
/// one `W_LABEL` title, a section label — and how many members it holds,
/// tabular `FS_SM` `TEXT_MUTED`, at the right. Its Projects are the glyph's
/// tooltip. No fill: the one selected fill is the focused member's. No
/// provider mark, no disclosure glyph, and no `needs you` of its own: the
/// member that waits says so.
#[cfg(test)]
pub fn group_row(row: &GroupBlock) -> Stateful<Div> {
    group_row_with_title(row, row.title.clone())
}

/// `group_row` with the title leaf supplied by the caller — the cockpit
/// hands in a click-to-rename wrapper, or the live editor while renaming.
/// The cell around it is unchanged either way: the geometry below is what
/// makes the title truncate at all. The title box sets the 20px line the
/// editor inherits, so renaming never moves the row.
pub fn group_row_with_title(row: &GroupBlock, title: impl IntoElement) -> Stateful<Div> {
    const TEXT_W: f32 = ROW_TEXT_W - NAV_TEXT_X - NAV_TAIL_MIN_W - NAV_TAIL_GAP;
    let count = row.members.len();
    row_frame(("nav-group", row.id.get() as usize), GROUP_ROW_H, false)
        .debug_selector({
            let id = row.id;
            move || format!("nav-group-{}", id.get())
        })
        .flex_row()
        .items_center()
        .gap(px(NAV_LEAD_GAP))
        .child(group_header_icon(row.id, row.projects.clone()))
        // A truncating title needs a **definite** width on its very first
        // measure. gpui caches a nowrap line's first measure permanently
        // (gpui-0.2.2 elements/text.rs:373 — `wrap_width` is `None` for
        // nowrap, so the early return fires on every later call), and taffy
        // only hands a text leaf a definite width when the leaf's flex
        // container is a **column** whose own available width is definite —
        // which taffy derives from the child's own min/max width
        // (taffy-0.9.0 compute/flexbox.rs:661-679). A `flex_1`, a `w_full` or
        // even a `w(px(..))` cell is measured at max-content first, so
        // `truncate_line` never runs and the line is only visually clipped.
        // Hence: flex **column**, with min and max width pinned to the row's
        // own text box.
        .child(
            div().w(px(TEXT_W)).flex().flex_col().child(
                div().h(px(TITLE_H)).overflow_hidden().child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w(px(TEXT_W + TRUNCATE_SLOP))
                        .max_w(px(TEXT_W + TRUNCATE_SLOP))
                        .truncate()
                        .h(px(TITLE_H))
                        .text_size(px(FS_UI))
                        .font_weight(W_LABEL)
                        .line_height(px(TITLE_H))
                        .text_color(rgb(TEXT))
                        .child(title),
                ),
            ),
        )
        .child(
            components::tabular(components::text_meta())
                .debug_selector({
                    let id = row.id;
                    move || format!("nav-group-count-{}", id.get())
                })
                .flex()
                .flex_shrink_0()
                .justify_end()
                .ml_auto()
                .child(count.to_string()),
        )
}

/// The members container, and the one line the tree draws: a 1px
/// `NAV_GROUP_RAIL` (a `HAIRLINE`) hanging from the Group glyph's centre,
/// inset 3px top and bottom. Square ends, no radius. It is the indent made
/// visible, so it is absolute and takes no layout of its own. Members sit
/// flush: each 28px row carries its own air.
pub fn members(rows: Vec<AnyElement>) -> Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .gap(px(MEMBER_GAP))
        .mt(px(MEMBERS_TOP))
        .ml(px(MEMBER_INDENT))
        .child(
            div()
                .absolute()
                .left(px(-RAIL_OFFSET))
                .top(px(RAIL_INSET))
                .bottom(px(RAIL_INSET))
                .w(px(1.))
                .bg(rgba(NAV_GROUP_RAIL)),
        )
        .children(rows)
}

/// The 28px (`THREAD_ROW_H`) Thread row, one line: the status dot in the
/// 16px lead slot, the title, the branch inline when it is not the
/// default, the tail (`NavTail`) right-aligned in its reserved box, and the
/// provider mark in a fixed 12px slot at the right — drawn even when the
/// provider is unknown, so the title never widens.
#[cfg(test)]
pub fn thread_row(row: &ThreadRow) -> Stateful<Div> {
    project_thread_row_with_title(row, row.name.clone(), false, false)
}

/// The one Thread row builder, with the title leaf supplied by the caller —
/// see `group_row_with_title`. `grouped` is Project order's membership
/// mark: a Thread that is still a Group member says so with the Group
/// glyph after its title, so every title keeps the same x.
/// `reduce_motion` holds an unread dot's breath at full ink.
pub fn project_thread_row_with_title(
    row: &ThreadRow,
    title: impl IntoElement,
    grouped: bool,
    reduce_motion: bool,
) -> Stateful<Div> {
    row_frame(
        ("nav-thread", row.thread.get() as usize),
        THREAD_ROW_H,
        row.current,
    )
    .debug_selector({
        let thread = row.thread;
        move || format!("nav-thread-{}", thread.get())
    })
    .tooltip(row_tooltip(row))
    .child(lead(status_dot(row, reduce_motion)))
    .child(title_cell(row, title).ml(px(NAV_LEAD_GAP)))
    .children(
        row.branch
            .clone()
            .map(|branch| branch_cell(row.thread, branch)),
    )
    .children(grouped.then(|| group_membership_indicator(row.thread)))
    .child(tail_cell(row))
    .child(mark_cell(row))
}

/// One row of the Needs-you strip: the Thread's own row shape, 28px, with
/// an `ATTENTION` dot and what it waits for (`approval` / `question`) as a
/// quiet tail. It is a reference to the Thread, not a second copy of it: it
/// carries no fill, and a press lands on the Thread like its own row does.
pub fn needs_you_row(entry: &NeedsYouRow) -> Stateful<Div> {
    let row = &entry.row;
    let thread = row.thread;
    let frame = div()
        .id(("nav-needs", thread.get() as usize))
        .debug_selector(move || format!("nav-needs-{}", thread.get()))
        .flex()
        .flex_row()
        .items_center()
        .flex_shrink_0()
        .h(px(NAV_ROW_H))
        .px(px(ROW_PAD_X))
        .py(px(NAV_ROW_PAD_Y))
        .rounded(px(NAV_ROW_R));
    let key = SharedString::from(format!("nav-needs-{}", thread.get()));
    frame
        .hover_row_faded(key)
        .press_row()
        .tooltip(row_tooltip(row))
        .child(lead(components::status_dot(ATTENTION)))
        .child(title_cell(row, row.name.clone()).ml(px(NAV_LEAD_GAP)))
        .child(
            components::text_meta()
                .flex()
                .flex_shrink_0()
                .justify_end()
                .min_w(px(NAV_TAIL_MIN_W))
                .ml(px(NAV_TAIL_GAP))
                .child(entry.kind),
        )
        .child(mark_cell(row))
}

/// The strip's header: `Needs you` in the section-label voice, the count in
/// `ATTENTION` (tabular), and `⌘D` — the key that answers it — right-aligned
/// as its keycap reads (`key_combo`) in `TEXT_MUTED`, on the rows' text
/// column.
pub fn needs_you_header(count: usize) -> Div {
    let key = components::bound_chord("cockpit::NextDecision")
        .map(|keys| components::key_combo(&keys, TEXT_MUTED).text_size(px(FS_SM)));
    components::text_meta()
        .debug_selector(|| "nav-needs-you".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(NAV_SECTION_H))
        .px(px(ROW_PAD_X))
        .gap(px(NAV_TAIL_GAP))
        .child(div().w(px(NAV_LEAD_W)).flex_shrink_0())
        .child(
            div()
                .ml(px(NAV_LEAD_GAP - NAV_TAIL_GAP))
                .font_weight(W_LABEL)
                .child("Needs you"),
        )
        .child(components::tabular(
            div().text_color(rgb(ATTENTION)).child(count.to_string()),
        ))
        .children(key.map(|key| div().ml_auto().child(key)))
}

/// The strip: its header and one row per waiting Thread, pinned under the
/// nav head (it does not scroll with the tree), at the tree's inset.
pub fn needs_you_strip() -> Div {
    div()
        .debug_selector(|| "nav-needs-you-strip".into())
        .flex()
        .flex_col()
        .flex_shrink_0()
        .px(px(NAV_TREE_PAD))
        .pb(px(NAV_SECTION_GAP))
}

/// A row's tooltip: the whole title, and how many subagents it runs.
fn row_tooltip(row: &ThreadRow) -> impl Fn(&mut gpui::Window, &mut App) -> gpui::AnyView {
    let text = match row.subagents {
        0 => row.name.to_string(),
        1 => format!("{}\n1 subagent", row.name),
        count => format!("{}\n{count} subagents", row.name),
    };
    crate::menu::tooltip(text)
}

/// A Thread row's title: one UI line in body weight that truncates, in
/// `title_ink`. The box sets the 20px line the rename editor inherits.
fn title_cell(row: &ThreadRow, title: impl IntoElement) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .h(px(TITLE_H))
        .text_size(px(FS_UI))
        .font_weight(W_BODY)
        .line_height(px(TITLE_H))
        .text_color(rgb(title_ink(row)))
        .child(title)
}

/// The focused Thread's title is the strongest ink in the tree, and so is
/// an unread one's: both ask to be read. A parked Thread's steps down a
/// rung, so what is running reads first. Ink only: the weight is always
/// `W_BODY`, so a row never reflows when it is read.
fn title_ink(row: &ThreadRow) -> u32 {
    if row.current || row.unread {
        TEXT_STRONG
    } else if row.status == RowStatus::Parked {
        TEXT_2
    } else {
        TEXT
    }
}

/// The branch, inline after the title (only ever a branch that is not the
/// Project's default): a faint `·`, then the name in `FS_SM` `TEXT_MUTED`.
/// It takes at most 40% of the row and truncates before the title does.
fn branch_cell(thread: ThreadId, branch: SharedString) -> Div {
    components::text_meta()
        .debug_selector(move || format!("nav-branch-{}", thread.get()))
        .flex()
        .flex_shrink(1.)
        .min_w_0()
        .max_w(relative(0.4))
        .items_center()
        .gap(px(NAV_TAIL_GAP))
        .ml(px(NAV_TAIL_GAP))
        .child(seam())
        .child(div().min_w_0().truncate().child(branch))
}

/// The tail: the subagent count (only beside an age or nothing, never a
/// state word), then the one word or age, right-aligned in a box that
/// keeps `NAV_TAIL_MIN_W` so a word arriving moves nothing.
fn tail_cell(row: &ThreadRow) -> Div {
    let thread = row.thread;
    let face = row.tail.face();
    let word = components::tabular(components::text_meta())
        .debug_selector(move || format!("nav-since-{thread}", thread = thread.get()))
        .flex()
        .flex_shrink_0()
        .justify_end()
        .min_w(px(NAV_TAIL_MIN_W))
        .children(face.map(|(text, ink)| div().text_color(rgb(ink)).child(text)));
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .ml_auto()
        .pl(px(NAV_TAIL_GAP))
        .gap(px(NAV_TAIL_GAP))
        .children((!row.tail.is_word()).then(|| subagent_tail(thread, row.subagents)))
        .child(word)
}

/// The provider mark's fixed slot at a row's right edge.
fn mark_cell(row: &ThreadRow) -> Div {
    let thread = row.thread;
    div()
        .flex()
        .flex_shrink_0()
        .ml(px(NAV_MARK_GAP))
        .debug_selector(move || format!("nav-mark-{}", thread.get()))
        .child(provider_mark(row.provider, PROVIDER_MARK))
}

/// The Group glyph in a Group row's lead slot, centred on the title line.
/// Its tooltip names the Group's Projects.
fn group_header_icon(group: GroupId, projects: Option<SharedString>) -> Stateful<Div> {
    let tip = match projects {
        Some(projects) => format!("Group \u{b7} {projects}"),
        None => "Group".to_string(),
    };
    div()
        .id(("nav-group-icon", group.get() as usize))
        .debug_selector(move || format!("nav-group-icon-{}", group.get()))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(NAV_LEAD_W))
        .h(px(TITLE_H))
        .child(icon(icons::GROUP, ROW_ICON, TEXT_MUTED))
        .tooltip(crate::menu::tooltip(tip))
}

/// The four-Pane Group mark after a title in Project order. Project order
/// flattens Groups into their Projects, so this keeps durable membership
/// visible without competing with the Thread's status dot or provider mark.
fn group_membership_indicator(thread: ThreadId) -> Stateful<Div> {
    div()
        .id(("nav-group-membership", thread.get() as usize))
        .debug_selector(move || format!("nav-group-membership-{}", thread.get()))
        .flex()
        .flex_shrink_0()
        .items_center()
        .ml(px(NAV_MARK_GAP))
        .child(icon(icons::GROUP, ROW_ICON, TEXT_MUTED))
        .tooltip(|window, cx| Tooltip::new("In a group").build(window, cx))
}

/// The number of subagents attached to a Thread: the `SUBAGENTS` mark in
/// `TEXT_FAINT` and a tabular digit in `TEXT_MUTED`. Threads without
/// children spend no space here; the row's tooltip names the count.
fn subagent_tail(thread: ThreadId, count: usize) -> Div {
    let cell = components::tabular(components::text_meta())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(NAV_TAIL_GAP))
        .debug_selector(move || format!("nav-subagents-{}", thread.get()));
    let Some(label) = subagent_label(count) else {
        return cell;
    };
    cell.child(icon(icons::SUBAGENTS, ROW_ICON, TEXT_FAINT))
        .child(label)
}

fn subagent_label(count: usize) -> Option<SharedString> {
    (count > 0).then(|| SharedString::from(count.to_string()))
}

/// The `·` between two metadata facts: structure, so the faint ink.
fn seam() -> Div {
    components::text_meta()
        .flex_shrink_0()
        .text_color(rgb(TEXT_FAINT))
        .child("·")
}

/// One run of solo Threads — those no Group claims — at root indent with
/// no rail. A run is however many solo rows the recency order happens to
/// put together between two Groups, so the tree holds several; each is a
/// place to drop a row to get it out of its Group, and each carries its
/// own id. The caller sets the margin above: a run that follows a Group
/// takes `SOLOS_TOP`, and a run that opens the tree takes none.
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
        .min_h(px(SOLOS_TOP))
}

/// What an empty tree says, on the rows' text column, sentence case and no
/// full stop: a line in `FS_UI` `TEXT_2` and, where there is one, a way
/// forward in the metadata voice. Filtered to a Project it names the
/// Project rather than shrugging; when the Parked section below holds
/// Threads the filter admits, it says *open* and points below, so the
/// operator is not told a tree is empty while its Threads sit one fold
/// away. An empty store offers the key that starts one.
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
                .gap(px(SPACE_2))
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
        .py(px(ROW_PAD_Y))
        .pl(px(ROW_PAD_X + NAV_TEXT_X))
        .pr(px(ROW_PAD_X))
        .child(
            components::text_ui()
                .text_color(rgb(TEXT_2))
                .child(SharedString::from(message)),
        )
        .children(hint)
}

/// A refusal from the last Group change, at the top of the tree, until the
/// next change succeeds: a 6px `BLOCKED` dot in the lead slot and the
/// refusal in `FS_SM` `TEXT_2` on the rows' text column. No ground, no
/// ochre: colour on the dot, never the row.
pub fn notice(text: SharedString) -> Div {
    components::text_meta()
        .debug_selector(|| "nav-notice".into())
        .flex()
        .flex_shrink_0()
        .items_start()
        .px(px(ROW_PAD_X))
        .py(px(ROW_PAD_Y))
        .text_color(rgb(TEXT_2))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .w(px(NAV_LEAD_W))
                .h(px(LH_META))
                .child(components::status_dot(BLOCKED)),
        )
        .child(div().min_w_0().ml(px(NAV_LEAD_GAP)).child(text))
}

/// The Parked section at the foot of the column, under the scrolling tree
/// rather than inside it: its header stays in reach however long the tree
/// grows. It takes the tree's inline inset so its rows line up with the
/// tree's, and it is capped at half the column — the list inside scrolls
/// past that, so unfolding it never hides the running Threads above.
pub fn parked_section() -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .max_h(relative(NAV_PARKED_MAX_SHARE))
        .px(px(NAV_TREE_PAD))
        .pb(px(NAV_TREE_PAD))
}

/// The Parked section's header: the fold chevron in the lead slot, the
/// word on the text column, and how many wait — a heading in the Project
/// headings' quiet voice, but a control: the press toggles the fold, and a
/// right press offers the section's own menu. The cockpit wires both.
pub fn parked_header(count: usize, open: bool) -> Stateful<Div> {
    let chevron = if open {
        icons::CHEVRON_DOWN
    } else {
        icons::CHEVRON_RIGHT
    };
    components::text_meta()
        .id(("nav-parked", 0usize))
        .debug_selector(|| "nav-parked".into())
        .group(PARKED_GROUP)
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(NAV_SECTION_H))
        .mt(px(NAV_SECTION_GAP))
        .px(px(ROW_PAD_X))
        .rounded(px(NAV_ROW_R))
        .hover_row()
        .press_row()
        .child(lead(
            icon(chevron, ROW_ICON, TEXT_FAINT)
                .group_hover(PARKED_GROUP, |style| style.text_color(rgb(TEXT_MUTED))),
        ))
        .child(
            div()
                .min_w_0()
                .truncate()
                .ml(px(NAV_LEAD_GAP))
                .group_hover(PARKED_GROUP, |style| style.text_color(rgb(TEXT)))
                .child("Parked"),
        )
        .child(components::tabular(
            div()
                .flex_shrink_0()
                .ml(px(NAV_TAIL_GAP))
                .child(count.to_string()),
        ))
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
        .pt(px(MEMBER_GAP))
}

/// The Parked list's scrollbar — its own id, because the toolkit keys a
/// bar's state off it and the tree's bar is already `nav-scrollbar`.
pub fn parked_scrollbar(scroll: &ScrollHandle) -> Div {
    components::scrollbar("nav-parked-scrollbar", scroll)
}

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

/// The collapsed rail. Primary navigation actions sit at the top, recent
/// Threads occupy the scrolling middle, and utilities are supplied by the
/// caller at the bottom—the familiar desktop navigation-rail hierarchy.
pub fn rail(_filtered: bool) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .items_center()
        .py(px(NAV_RAIL_PAD_Y))
}

/// A compact cluster for the rail's primary actions.
pub fn rail_actions() -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .items_center()
        .gap(px(NAV_RAIL_ITEM_GAP))
}

/// Utilities stay pinned to the bottom rather than competing with Threads.
pub fn rail_utilities() -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .items_center()
        .gap(px(NAV_RAIL_ITEM_GAP))
        .pt(px(NAV_RAIL_PAD_Y))
}

/// The rail's filter button: the compact column's Project affordance, the
/// folder the expanded filter wears. Its glyph brightens to `TEXT` when a
/// Project filter is active — the only way the collapsed nav can admit it
/// is hiding Threads — and the tooltip names the scope.
pub fn rail_filter(filtered: bool, scope: SharedString) -> Stateful<Div> {
    let resting = if filtered { TEXT } else { TEXT_MUTED };
    div()
        .id(("nav-rail-filter", 0usize))
        .debug_selector(|| "nav-rail-filter".into())
        .group(RAIL_FILTER_GROUP)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(NAV_RAIL_CONTROL))
        .h(px(NAV_RAIL_CONTROL))
        .rounded(px(R_CONTROL))
        .hover_control()
        .press_control()
        .tooltip(crate::menu::tooltip(scope))
        .child(
            icon(icons::FOLDER, ICON_BUTTON_GLYPH, resting)
                .group_hover(RAIL_FILTER_GROUP, |style| style.text_color(rgb(TEXT))),
        )
}

/// The rail's item column scrolls independently between the pinned primary
/// actions and utilities, with no thumb: the rail is too narrow to carry
/// one. A stable element id retains its scroll offset as Thread status
/// updates arrive; the fixed-size buttons keep their targets instead of
/// being squeezed into the available height.
pub fn rail_items() -> Stateful<Div> {
    div()
        .id("nav-rail-items")
        .debug_selector(|| "nav-rail-items".into())
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .w_full()
        .items_center()
        .gap(px(NAV_RAIL_ITEM_GAP))
        .mt(px(NAV_RAIL_ITEMS_TOP))
        .overflow_y_scroll()
}

/// The ordinal a rail item wears and the ⌘ key that lands on it: the first
/// nine items are ⌘1…⌘9 (`cockpit::FocusThread1`…`9`), the rest none.
pub fn rail_ordinal(position: usize) -> Option<usize> {
    (position < 9).then_some(position + 1)
}

/// One rail item: the Thread's provider mark in its brand colour, centred
/// in a 16px box, its status dot at the bottom-right corner and, for the
/// first nine, its ordinal at the top-left in `FS_SM` `TEXT_MUTED`
/// (tabular) — the ⌘1…⌘9 that lands on it. No initials. The tooltip is the
/// title and its key. The focused Thread's item carries the tree's one
/// selected fill.
pub fn rail_item(row: &ThreadRow, current: bool, position: usize) -> Button {
    let ordinal = rail_ordinal(position);
    let title = row.name.clone();
    let tip = match ordinal {
        Some(ordinal) => SharedString::from(format!("{title} \u{2318}{ordinal}")),
        None => title.clone(),
    };
    components::button(("nav-rail-item", row.thread.get() as usize))
        .debug_selector(move || format!("nav-rail-item-{}", row.thread.get()))
        .group(RAIL_ITEM_GROUP)
        .w(px(NAV_RAIL_CONTROL))
        .h(px(NAV_RAIL_CONTROL))
        .p_0()
        .tooltip(tip)
        .accessibility_label(title)
        .when(current, |button| button.bg(rgb(FILL)))
        .child(
            div()
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .w(px(NAV_RAIL_CONTROL))
                .h(px(NAV_RAIL_CONTROL))
                .font_family(FONT_UI)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(NAV_LEAD_W))
                        .child(provider_mark(row.provider, PROVIDER_MARK)),
                )
                .children(ordinal.map(|ordinal| {
                    components::tabular(components::text_meta())
                        .absolute()
                        .left(px(NAV_RAIL_DOT_INSET))
                        .top(px(NAV_RAIL_DOT_INSET / 2.0))
                        .child(ordinal.to_string())
                }))
                .child(
                    div()
                        .absolute()
                        .right(px(NAV_RAIL_DOT_INSET))
                        .bottom(px(NAV_RAIL_DOT_INSET))
                        .child(dot_face(row)),
                ),
        )
}

/// The frame every tree row shares: the `R_CONTROL` box, its padding, and
/// the fill language. The height is fixed so a row that cannot resolve its
/// Project or its checkout still occupies exactly the space it will occupy
/// once the cache fills.
///
/// `selected` is only ever the focused Thread's row, the tree's one fill:
/// hover cannot wash over a ground stronger than itself, so that row steps
/// its ground up instead (`FILL` → `FILL_HOVER`) rather than being washed
/// down.
fn row_frame(id: (&'static str, usize), height: f32, selected: bool) -> Stateful<Div> {
    let frame = div()
        .id(id)
        .relative()
        .flex()
        .flex_row()
        .items_center()
        .flex_shrink_0()
        .h(px(height))
        .px(px(ROW_PAD_X))
        .py(px(NAV_ROW_PAD_Y))
        .rounded(px(NAV_ROW_R));
    // The hover face fades in and out (`motion::HOVER_FADE`): the pointer
    // sweeps these rows constantly, so a snap would flicker the column.
    let key = SharedString::from(format!("{}-{}", id.0, id.1));
    let frame = if selected {
        frame.hover_carried_faded(key).press_row()
    } else {
        frame.hover_row_faded(key).press_row()
    };
    // Rows are draggable into Groups, so they wear the open hand rather than
    // the pointer: the drag is the row's second verb, and the only one the
    // cursor can advertise before the press. It is set **after** the hover
    // role, whose `cursor_pointer` would otherwise overwrite it — the roles
    // in `pointer.rs` set the base cursor, not a hover refinement.
    frame.cursor(CursorStyle::OpenHand)
}

/// The provider logomark in its own brand colour (rule 2.2.2, the operator's
/// call) — or an empty box of the same width when the provider is
/// unknowable (an unreadable parked log). The box is never a placeholder
/// glyph and never a `cl` / `cx` string: it holds the column open and says
/// nothing.
fn provider_mark(provider: Option<Provider>, size: f32) -> AnyElement {
    match provider {
        Some(Provider::Codex) => icon(icons::CODEX, size, PROVIDER_CODEX).into_any_element(),
        Some(Provider::Claude) => icon(icons::CLAUDE, size, PROVIDER_CLAUDE).into_any_element(),
        None => div()
            .flex_shrink_0()
            .w(px(size))
            .h(px(size))
            .into_any_element(),
    }
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

    /// A nav row is a soft card (the block radius), the filter over the
    /// tree is a quiet body-weight line in `TEXT_2`, and a section heading
    /// keeps the section step above it.
    #[test]
    fn nav_rows_and_the_filter_read_quietly() {
        let mut row = thread_row(&thread(Some(Provider::Claude)));
        let radii = row.style().corner_radii.clone();
        assert_eq!(radii.top_left, Some(px(NAV_ROW_R).into()));
        assert_eq!(NAV_ROW_R, R_MENU_ROW, "a nav row is a menu row's shape");
        let mut filter = filter_trigger(&FilterState {
            label: "All projects".into(),
            open: false,
            options: Vec::new(),
        });
        let style = filter.style();
        assert_eq!(style.text.font_weight, Some(W_BODY));
        assert_eq!(style.text.color, Some(rgb(TEXT_2).into()));
        let mut parked = parked_header(3, false);
        assert_eq!(parked.style().margin.top, Some(px(NAV_SECTION_GAP).into()));
    }

    /// The selection rule: one fill in the whole tree, on the focused
    /// Thread's row. The Group holding it stays unfilled — two stacked
    /// pills would say two things are selected — and a Thread that merely
    /// sits in the current Group is not itself current.
    #[test]
    fn one_fill_marks_the_focused_thread() {
        let fill = |mut drawn: Stateful<Div>| drawn.style().background.clone();
        assert_eq!(
            fill(group_row(&group())),
            None,
            "the Group holding the focused Thread draws no fill of its own"
        );
        assert_eq!(
            fill(thread_row(&current_thread(Some(Provider::Claude), true))),
            Some(rgb(FILL).into()),
            "the focused Thread's own row carries the tree's one fill"
        );
        assert_eq!(fill(thread_row(&thread(Some(Provider::Claude)))), None);
        assert_eq!(
            fill(project_thread_row_with_title(
                &current_thread(None, true),
                "thread-08",
                true,
                false
            )),
            Some(rgb(FILL).into()),
            "Project order marks the same row the same way"
        );
    }

    /// The focused title is the strongest ink in the tree, an unread one is
    /// as strong, a parked title steps down a rung, and every other title is
    /// the body ink. Unread changes ink only, never weight.
    #[test]
    fn titles_rank_focus_then_running_then_parked() {
        assert_eq!(title_ink(&current_thread(None, true)), TEXT_STRONG);
        assert_eq!(title_ink(&thread(None)), TEXT);
        let parked = ThreadRow {
            status: RowStatus::Parked,
            ..thread(None)
        };
        assert_eq!(title_ink(&parked), TEXT_2);
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
        let weight = |row: &ThreadRow| {
            let mut cell = title_cell(row, row.name.clone());
            cell.style().text.font_weight
        };
        assert_eq!(weight(&unread), Some(W_BODY), "unread is ink, never weight");
        assert_eq!(weight(&unread), weight(&thread(None)));
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

    #[test]
    fn the_order_button_is_clear_at_rest_and_filled_while_open() {
        let background = |mut button: Button| button.style().background.clone();
        assert_eq!(background(order_button(false, false)), None);
        assert_eq!(background(order_button(true, false)), None);
        assert_eq!(
            background(order_button(false, true)),
            Some(rgb(FILL).into())
        );
    }

    /// A row whose Project or checkout has not resolved keeps its full
    /// height: the caches fill asynchronously, and the tree must not jump
    /// under the pointer when they do.
    #[test]
    fn an_unresolved_row_keeps_its_height() {
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
        };
        let branch_only = ThreadRow {
            branch: Some("main".into()),
            ..bare.clone()
        };
        let height = |mut drawn: Stateful<Div>| drawn.style().size.height;
        assert_eq!(height(thread_row(&bare)), height(thread_row(&thread(None))));
        assert_eq!(
            height(thread_row(&bare)),
            height(thread_row(&branch_only)),
            "a branch without its Project is the same box"
        );
        assert_eq!(
            height(thread_row(&bare)),
            height(thread_row(&current_thread(None, true))),
            "the current row is the same box as any other, only filled"
        );
        assert_eq!(
            height(thread_row(&bare)),
            Some(px(THREAD_ROW_H).into()),
            "NAV_ROW_PAD_Y 4 + LH_UI 20 + NAV_ROW_PAD_Y 4"
        );
        assert_eq!(THREAD_ROW_H, 2.0 * NAV_ROW_PAD_Y + TITLE_H);
        for tail in [
            NavTail::NeedsYou,
            NavTail::Failing(Some(12)),
            NavTail::Failed,
            NavTail::Done,
        ] {
            assert_eq!(
                height(thread_row(&ThreadRow {
                    tail: tail.clone(),
                    ..bare.clone()
                })),
                height(thread_row(&bare)),
                "{tail:?}: a word arriving is the same box"
            );
        }
        assert_eq!(
            height(group_row(&group())),
            Some(px(GROUP_ROW_H).into()),
            "the same one line as a Thread row"
        );
        assert_eq!(GROUP_ROW_H, THREAD_ROW_H);
        assert_eq!(
            height(project_thread_row_with_title(
                &bare,
                "thread-09",
                false,
                false
            )),
            Some(px(THREAD_ROW_H).into()),
            "Project order's row is the same one line"
        );
        assert_eq!(THREAD_ROW_H, MENU_ROW_H, "one list pitch (C9)");
    }

    /// The tail says one thing, in priority order, and the Pane's head slot
    /// decides it: a waiting request, red tests, a failure, an unread
    /// finish; a working row says nothing; everything else is its age.
    #[test]
    fn the_tail_is_one_word_or_an_age_in_priority_order() {
        use crate::pane::HeadSlot;
        let age = || SharedString::from("40m");
        assert_eq!(
            NavTail::of(Some(&HeadSlot::NeedsYou(words::QUESTION)), true, age()),
            NavTail::NeedsYou
        );
        assert_eq!(
            NavTail::of(Some(&HeadSlot::Failing(Some(3))), true, age()),
            NavTail::Failing(Some(3))
        );
        assert_eq!(
            NavTail::of(Some(&HeadSlot::Failing(None)), false, age()),
            NavTail::Failing(None)
        );
        assert_eq!(
            NavTail::of(Some(&HeadSlot::Failed), true, age()),
            NavTail::Failed
        );
        assert_eq!(
            NavTail::of(Some(&HeadSlot::Done), true, age()),
            NavTail::Done
        );
        assert_eq!(
            NavTail::of(Some(&HeadSlot::Done), false, age()),
            NavTail::Age(age()),
            "done is said only while it is unread"
        );
        assert_eq!(
            NavTail::of(Some(&HeadSlot::Working("12s".into())), true, age()),
            NavTail::None,
            "a working row's sage dot says it"
        );
        for quiet in [None, Some(&HeadSlot::Interrupted), Some(&HeadSlot::Parked)] {
            assert_eq!(NavTail::of(quiet, false, age()), NavTail::Age(age()));
        }
        let face = |tail: NavTail| tail.face();
        assert_eq!(
            face(NavTail::NeedsYou),
            Some((words::NEEDS_YOU.into(), ATTENTION))
        );
        assert_eq!(
            face(NavTail::Failing(Some(2))),
            Some(("failing 2".into(), BLOCKED))
        );
        assert_eq!(
            face(NavTail::Failing(None)),
            Some((words::FAILING.into(), BLOCKED))
        );
        assert_eq!(face(NavTail::Failed), Some((words::FAILED.into(), BLOCKED)));
        assert_eq!(face(NavTail::Done), Some((words::DONE.into(), TEXT_MUTED)));
        assert_eq!(face(NavTail::Age(age())), Some((age(), TEXT_MUTED)));
        assert_eq!(face(NavTail::None), None);
        assert!(NavTail::NeedsYou.is_word() && NavTail::Done.is_word());
        assert!(!NavTail::Age(age()).is_word() && !NavTail::None.is_word());
    }

    /// The nav never says `now` (C10): a Thread used seconds ago has an
    /// empty age, which draws nothing in its reserved box.
    #[test]
    fn the_nav_never_says_now() {
        let at = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10_000);
        for secs in [0, 1, 30, 59] {
            let age = crate::facts::since_label(at, at + std::time::Duration::from_secs(secs));
            let tail = NavTail::of(None, false, age);
            assert_eq!(tail.face(), None, "{secs}s says nothing");
        }
        let minute = crate::facts::since_label(at, at + std::time::Duration::from_secs(90));
        assert_eq!(
            NavTail::of(None, false, minute).face(),
            Some(("1m".into(), TEXT_MUTED))
        );
        for tail in [
            NavTail::NeedsYou,
            NavTail::Failing(None),
            NavTail::Failed,
            NavTail::Done,
        ] {
            let (text, _) = tail.face().unwrap();
            assert_ne!(text.as_ref(), "now");
        }
    }

    /// Only unread breathes (C11): a working or failing dot is still, and a
    /// live state that outranks unread is still too.
    #[test]
    fn only_an_unread_dot_breathes() {
        let row = |status, unread| ThreadRow {
            status,
            unread,
            ..thread(None)
        };
        assert!(breathes(&row(RowStatus::Idle, true)));
        assert!(!breathes(&row(RowStatus::Idle, false)));
        for status in [
            RowStatus::Working,
            RowStatus::Failing,
            RowStatus::NeedsYou,
            RowStatus::Failed,
        ] {
            assert!(!breathes(&row(status, false)), "{status:?}");
            assert!(!breathes(&row(status, true)), "{status:?} outranks unread");
        }
    }

    /// A working dot is still and sits in a fixed dot box, so a Thread that
    /// starts inferring does not shift its own row — or any row under it —
    /// by a pixel.
    #[test]
    fn a_working_row_is_the_same_box_as_a_quiet_one() {
        let height = |mut drawn: Stateful<Div>| drawn.style().size.height;
        for status in [RowStatus::Working, RowStatus::Failing] {
            let live = ThreadRow {
                status,
                ..thread(Some(Provider::Claude))
            };
            assert_eq!(
                height(thread_row(&live)),
                height(thread_row(&thread(Some(Provider::Claude)))),
                "{status:?}"
            );
            assert_eq!(height(thread_row(&live)), Some(px(THREAD_ROW_H).into()));
        }
    }

    /// The dots are the Pane's own colours: green only for live work, a
    /// failing Thread in the failure's red (it still breathes, because it
    /// is still inferring), a Decision ochre, unread the accent, idle muted,
    /// parked hollow.
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
            Some(rgb(ACCENT).into()),
            "an unread quiet row is the accent, never ochre"
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
        let mut parked = face(RowStatus::Parked, false);
        assert_eq!(parked.style().background, None, "a parked dot is a ring");
        assert_eq!(
            parked.style().border_color,
            Some(rgb(TEXT_MUTED).into()),
            "the ring is the metadata ink"
        );
    }

    /// One status truth: for every Pane state and either side of unread,
    /// the nav row's dot is the Pane's own dot, face and ink.
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
    }

    /// The grid: the head's folder, every row's lead glyph and the Parked
    /// chevron sit on one axis, every title and label on the next, a
    /// member's lead slot starts under its Group's title, and the rail
    /// hangs from the Group glyph's centre.
    #[test]
    fn every_row_shares_one_column_grid() {
        assert_eq!(NAV_TEXT_X, NAV_LEAD_W + NAV_LEAD_GAP);
        assert_eq!(MEMBER_INDENT, NAV_TEXT_X);
        assert_eq!(
            MEMBER_INDENT - RAIL_OFFSET,
            ROW_PAD_X + NAV_LEAD_W / 2.0,
            "the rail's x is the Group glyph's centre"
        );
        assert_eq!(NAV_LEAD_W, 16.0, "a 6px dot centred in a 16px lead");
        assert_eq!(NAV_TEXT_X, 22.0);
        assert_eq!(
            PROVIDER_MARK, ROW_ICON,
            "the provider mark and the Group glyph are one glyph size"
        );
        let mut head = nav_head();
        assert_eq!(
            head.style().padding.left,
            Some(px(NAV_TREE_PAD).into()),
            "the head takes the tree's inset"
        );
        let mut trigger = filter_trigger(&FilterState {
            label: "All projects".into(),
            open: false,
            options: Vec::new(),
        });
        assert_eq!(
            trigger.style().padding.left,
            Some(px(ROW_PAD_X).into()),
            "head inset + trigger inset == tree inset + row inset"
        );
        assert_eq!(trigger.style().gap.width, Some(px(NAV_LEAD_GAP).into()));
    }

    #[test]
    fn subagent_count_is_hidden_at_zero_and_uses_compact_numeric_copy() {
        assert_eq!(subagent_label(0), None);
        assert_eq!(subagent_label(1).as_deref(), Some("1"));
        assert_eq!(subagent_label(3).as_deref(), Some("3"));
    }

    /// The Parked header is a control in a heading's clothes: it is
    /// pressed like a row, and it is the same 28px box the Project
    /// headings are, folded or not — unfolding must not move the tree's
    /// bottom edge by a pixel.
    #[test]
    fn the_parked_header_is_a_row_sized_heading() {
        let mut shut = parked_header(3, false);
        assert_eq!(shut.style().mouse_cursor, Some(CursorStyle::PointingHand));
        assert_eq!(shut.style().size.height, Some(px(NAV_SECTION_H).into()));
        let mut open = parked_header(3, true);
        assert_eq!(open.style().size.height, shut.style().size.height);
        let mut heading = project_section(0, "ferrite".into(), 3, true);
        assert_eq!(heading.style().size.height, shut.style().size.height);
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
    }

    /// One plane with the window: the column is `GROUND` with no edge on
    /// any side. The Panes separate themselves by their own plane and
    /// hairline; the nav is the field they lie on, not a slab.
    #[test]
    fn the_column_has_no_edge() {
        let mut column = shell(false);
        let style = column.style();
        assert_eq!(style.background, Some(rgb(GROUND).into()));
        let edges = &style.border_widths;
        assert!(edges.top.is_none() && edges.right.is_none());
        assert!(edges.bottom.is_none() && edges.left.is_none());
        assert_eq!(style.size.width, Some(px(WIDTH).into()));
        let mut rail = shell(true);
        assert_eq!(rail.style().size.width, Some(px(RAIL_WIDTH).into()));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_mac_rail_owns_the_full_traffic_light_reserve() {
        assert_eq!(RAIL_WIDTH, TRAFFIC_RESERVE);
        let mut item = rail_item(&thread(Some(Provider::Codex)), true, 0);
        assert_eq!(item.style().size.width, Some(px(36.0).into()));
        let mut filter = rail_filter(false, "All projects".into());
        assert_eq!(filter.style().size.width, Some(px(36.0).into()));
    }

    /// The rail's ordinals are the ⌘1…⌘9 keys, one each for the first nine
    /// items and none past them, and each ordinal names the action its key
    /// is bound to.
    #[test]
    fn rail_ordinals_are_the_command_digits() {
        assert_eq!(rail_ordinal(0), Some(1));
        assert_eq!(rail_ordinal(8), Some(9));
        assert_eq!(rail_ordinal(9), None);
        assert_eq!(rail_ordinal(40), None);
        for position in 0..9 {
            let ordinal = rail_ordinal(position).unwrap();
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
                "⌘{ordinal} lands on item {ordinal}"
            );
        }
    }

    /// Threads that need you pin to the top of the rail in the answer
    /// order; every other item keeps the tree's order.
    #[test]
    fn the_rail_pins_threads_that_need_you_first() {
        let row = |id: u64| ThreadRow {
            thread: ThreadId::new(id),
            ..thread(None)
        };
        let state = NavState {
            filter: FilterState {
                label: "All projects".into(),
                open: false,
                options: Vec::new(),
            },
            groups: Vec::new(),
            solos: vec![row(1), row(2), row(3), row(4)],
            parked: Vec::new(),
            parked_open: false,
            order: (0..4).map(NavItem::Solo).collect(),
            project_sections: Vec::new(),
            thread_list_order: ThreadListOrder::Recent,
            order_open: false,
            collapsed: true,
            needs_you: vec![
                NeedsYouRow {
                    row: row(4),
                    kind: words::APPROVAL,
                },
                NeedsYouRow {
                    row: row(2),
                    kind: words::QUESTION,
                },
            ],
        };
        let ids: Vec<u64> = state
            .rail_rows()
            .iter()
            .map(|row| row.thread.get())
            .collect();
        assert_eq!(ids, vec![4, 2, 1, 3]);
    }
}
