//! The left navigation column (#21): one Project filter, then the Groups
//! with their member Threads indented under a rail, then the solo Threads
//! at root. It is a view, never the only door: everything a row does
//! (focus, revive, regroup) stays reachable from the keyboard.
//!
//! Drawing only, like `pane.rs`: the cockpit assembles a `NavState` per
//! frame from O(1) reads plus its project/branch/parked caches —
//! `Store::load` and `Instruments::of` are banned here, which is what keeps
//! the 24-Pane wall smooth with the nav open. Click wiring stays in
//! `cockpit.rs`, the same split `pane_cell` uses.
//!
//! What the nav deliberately does **not** draw, per the approved prototype:
//! no status dot, no state word, no state colour, no counts, no badges, no
//! provider text tag, no section headers, no dividers, and no border on the
//! column itself. A running Thread and a blocked Thread are pixel-identical
//! here — state lives in the Pane, position lives in the nav. The only ink
//! that ever moves is the selected fill, and it lands on the **Group**.
//!
//! Every colour and metric is a `theme` token; this file holds no literal
//! of its own. Everything outside a Pane is the system UI face, which —
//! unlike the bundled mono family — exposes a real weight axis, so
//! `.font_weight(..)` is correct here.

use ferrite_core::groups::GroupId;
use ferrite_core::store::Provider;
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::ThreadId;
use gpui::prelude::*;
use gpui::{
    div, point, px, radians, relative, rgb, rgba, AnyElement, BoxShadow, CursorStyle, Div,
    FontWeight, ScrollHandle, SharedString, Stateful, Transformation,
};

use crate::components;
use crate::icons::{self, icon};
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::{
    ATTENTION, BLOCKED, FILL, FONT_UI, FS_LG, FS_MD, FS_SM, GROUP_GAP, GROUP_RAIL, GROUP_ROW_H,
    HOVER, ICON_BUTTON, ICON_BUTTON_GLYPH, ICON_CHEVRON_LG, IDLE, LINE_TIGHT, MEMBERS_TOP,
    MEMBER_GAP, MEMBER_INDENT, MENU, MENU_PAD, MENU_ROW_H, MENU_TOP, NAV, NAV_HEAD_H, NAV_TREE_PAD,
    NAV_TREE_PAD_B, PROVIDER_CLAUDE, PROVIDER_CODEX, PROVIDER_MARK, RAIL_INSET, RAIL_OFFSET,
    ROW_GAP, ROW_ICON, ROW_ICON_GAP, ROW_PAD_X, ROW_PAD_Y, ROW_TEXT_W, RUNNING, R_CONTROL, R_MENU,
    R_TIGHT, SEP, SHADOW_FAR, SHADOW_FAR_BLUR, SHADOW_FAR_SPREAD, SHADOW_FAR_Y, SHADOW_NEAR,
    SHADOW_NEAR_BLUR, SHADOW_NEAR_Y, SOLOS_TOP, STATUS_DOT, TEXT, TEXT_2, TEXT_MUTED, TEXT_STRONG,
    THREAD_ROW_H, TRAFFIC_RESERVE, WIN_CHROME_H,
};

/// The nav's two widths — 286px, and the 56px rail cmd-b folds it to.
/// `CockpitView::cell()` subtracts whichever is live, so the nav stays part
/// of the semantic-zoom input rather than a special case.
pub use crate::theme::{NAV_RAIL_WIDTH as RAIL_WIDTH, NAV_WIDTH as WIDTH};

/// A Group row's title line: 13px on the tight 1.25 leading → 16.25px.
const TITLE_LG_H: f32 = FS_LG * LINE_TIGHT;
/// A Thread row's title line: 12px tight → 15px.
const TITLE_MD_H: f32 = FS_MD * LINE_TIGHT;
/// The Project and checkout lines: 11px tight → 13.75px. A row keeps this
/// height even when the fact is unknown, so nothing reflows on a cache fill.
const META_H: f32 = FS_SM * LINE_TIGHT;

/// The slack a truncating title's budget gets over its visible box.
///
/// gpui truncates by summing each character's advance measured **alone**
/// (gpui-0.2.2 text_system/line_wrapper.rs:193 `width_for_char`, cached per
/// char) and keeps a prefix only while `width + suffix_width <
/// truncate_width` — a strict `<`, against CSS's `<=`. The rendered line is
/// *shaped*, so the kerned run is narrower than that sum, and the last
/// glyph that would still fit is dropped: the prototype's Group title ends
/// `& r…` where the unslacked port ends `& …`, 8px of the 254px cell left
/// empty. Handing the truncator this much extra budget restores the glyph;
/// the visible box below stays exactly `ROW_TEXT_W`, and clips.
const TRUNCATE_SLOP: f32 = 4.0;

/// The two icon buttons tint their glyph on hover, and a child SVG paints
/// from its **own** style — an ambient text colour reaches text but never an
/// `svg()`. `group_hover` is the only mechanism that carries a parent's
/// hover down to a child's colour, so each button names a group.
const COLLAPSE_GROUP: &str = "nav-collapse";
const RAIL_FILTER_GROUP: &str = "nav-rail-filter";
const FILTER_GROUP: &str = "nav-filter";
const FILTER_OPTION_GROUP: &str = "nav-filter-option";

// The handful of nav metrics `theme.rs` does not name, kept here rather
// than written inline so each one is said once and explained once.
//
/// 4px — the gap between the filter trigger's label and its chevron. The
/// chevron belongs to the word, not to the control's right edge.
const TRIGGER_GAP: f32 = 4.0;
/// 9px — a filter option's leading inset. One more than a row's, so the
/// option's label hangs under the trigger's label rather than under its box.
const MENU_ROW_PAD_L: f32 = 9.0;
/// The collapsed window-chrome band's block padding, 10px above and 4px
/// below: the host traffic lights are gone at 56px, so the button carries
/// the whole band and sits lower in it than centred.
const RAIL_CHROME_PAD_T: f32 = 10.0;
const RAIL_CHROME_PAD_B: f32 = 4.0;
/// 7px — the collapsed rail's own block padding.
const RAIL_PAD_Y: f32 = 7.0;
/// 12px — the gap between the rail's filter button and its first item, and
/// the empty-filter message's block margin.
const RAIL_ITEMS_TOP: f32 = 12.0;

/// What the nav draws this frame: one filter, then Groups with their
/// members, then the solos. Nothing here is a store read — the cockpit
/// assembles it from O(1) reads plus its project/branch caches.
pub struct NavState {
    pub filter: FilterState,
    pub groups: Vec<GroupBlock>,
    pub solos: Vec<ThreadRow>,
    /// The one order the tree draws in — Groups and solo Threads
    /// interleaved, most recently used first. The two lists above are the
    /// membership; this is the sequence.
    pub order: Vec<NavItem>,
    pub collapsed: bool,
}

impl NavState {
    /// Every row in the order the tree draws it: a Group's members where
    /// their Group sits, a solo where it sits. The rail folds to exactly
    /// this sequence, and tests read the tree's order from it.
    pub fn ordered_rows(&self) -> Vec<&ThreadRow> {
        self.order
            .iter()
            .flat_map(|item| match item {
                NavItem::Group(index) => self.groups[*index].members.iter(),
                NavItem::Solo(index) => std::slice::from_ref(&self.solos[*index]).iter(),
            })
            .collect()
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
/// `All Projects`.
pub struct FilterState {
    pub label: SharedString,
    pub open: bool,
    pub options: Vec<FilterOption>,
}

/// One row of the filter menu. `project: None` is the `All Projects` row and
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
    /// None when no member resolves one.
    pub projects: Option<SharedString>,
    /// This Group holds the focused Pane's Thread: it carries the selected
    /// fill **and** the white title. The Group carries the fill; a Thread
    /// row never does.
    pub current: bool,
    pub members: Vec<ThreadRow>,
}

/// One Thread's row — identical whether it is a Group member or a solo; only
/// the container differs. Title, Project, checkout, and the provider mark in
/// the top-right corner. No status dot, no state label, no counts.
pub struct ThreadRow {
    pub thread: ThreadId,
    pub name: SharedString,
    /// What the Thread is doing right now — the one glance the operator
    /// asked for from the tree: which agents are working, which wait.
    pub status: RowStatus,
    /// `None` → line 2 draws neither icon nor label, and keeps its height.
    pub project: Option<SharedString>,
    /// `None` → line 3 draws neither icon nor label, and keeps its height.
    pub branch: Option<SharedString>,
    /// `None` → no logomark. Never a `cl`/`cx` string.
    pub provider: Option<Provider>,
    /// This is the focused Pane's Thread: it carries the selected fill and
    /// the white title. The Group around it carries the fill too, so the
    /// pair reads as one selection.
    pub current: bool,
    /// How long since the Thread was last used — `40m`, `2h`, `3d` — at the
    /// tail of the checkout line. `None` says nothing at all.
    pub last_used: Option<SharedString>,
}

/// A Thread row's state, for its dot. The nav's original no-dot ruling
/// gave way to the operator's need to see, from the tree, which Threads
/// are working and which sit idle or wait on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RowStatus {
    Working,
    /// Working with a red test suite.
    Failing,
    /// A Decision waits on the operator.
    Attention,
    /// The Session closed under it.
    Blocked,
    #[default]
    Idle,
    Parked,
}

/// The status dot before a row's title: the Pane head's own colours, so
/// the tree and the Pane can never disagree. An idle Thread keeps a dim
/// dot — it is alive, just quiet — and a parked one a hollow ring.
fn status_dot(status: RowStatus) -> Div {
    let dot = div()
        .flex_shrink_0()
        .w(px(STATUS_DOT))
        .h(px(STATUS_DOT))
        .rounded_full();
    match status {
        RowStatus::Working => dot.bg(rgb(RUNNING)),
        RowStatus::Failing => dot.bg(rgb(RUNNING)).border_1().border_color(rgb(BLOCKED)),
        RowStatus::Attention => dot.bg(rgb(ATTENTION)),
        RowStatus::Blocked => dot.bg(rgb(BLOCKED)),
        RowStatus::Idle => dot.bg(rgb(IDLE)),
        RowStatus::Parked => dot.border_1().border_color(rgb(SEP)),
    }
}

/// The nav column itself: full height, the `--nav` ground, and **no border
/// on any edge** — `#232323` meets the Cockpit's `#0e0e0e` directly, because
/// Soft separates by fill contrast and draws no hairlines at all. The
/// column is the lightest field in the system: navigation reads as nearer
/// than the Cockpit, which is the inversion Soft makes.
///
/// Nothing is clipped here — the tree scrolls itself.
pub fn shell(collapsed: bool) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(if collapsed { RAIL_WIDTH } else { WIDTH }))
        .bg(rgb(NAV))
        .font_family(FONT_UI)
}

/// The 42px window-chrome band at the top of the column.
///
/// On macOS the traffic lights are the **host's**, positioned by
/// `TitlebarOptions`, so the band reserves their room rather than drawing
/// fakes: a `TRAFFIC_RESERVE`-wide spacer that holds nothing. The prototype
/// reaches the same x = 77 button edge with 13px of padding plus an 8px flex
/// gap plus a 4px margin; those three sum into the reserve here, because the
/// binding fact is the button's left edge and the empty band before it —
/// anything drawn or hit-testable in that strip kills AppKit's drag region.
///
/// Everywhere else there are no lights to reserve for, and reserving anyway
/// is what pushed the collapse button 77px off the column it belongs to:
/// the band takes the row inset instead, so the button's left edge lines up
/// with every nav row under it. The caption buttons sit at the *window's*
/// corner, not the column's — `titlebar.rs` draws them.
///
/// Collapsed the band becomes a vertical stack. The button is the caller's
/// to append: its click lives where the view state does.
pub fn win_chrome(collapsed: bool) -> Div {
    if collapsed {
        return div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .items_center()
            .pt(px(RAIL_CHROME_PAD_T))
            .pb(px(RAIL_CHROME_PAD_B))
            .gap(px(ROW_PAD_X));
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

/// The 28×28 collapse button and its 16px sidebar glyph. The cockpit wires
/// cmd-b and the click; the button only says what it looks like.
pub fn collapse_button() -> Stateful<Div> {
    div()
        .id(("nav-collapse", 0usize))
        .group(COLLAPSE_GROUP)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
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
/// overpaint it.
pub fn nav_head() -> Div {
    div()
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(NAV_HEAD_H))
        .px(px(ROW_PAD_X))
        .gap(px(ROW_PAD_Y))
}

/// The Project filter trigger — the one dropdown navigation has. The
/// chevron sits **immediately after the label**, never pushed to the right
/// edge: the control is a word with a mark, not a full-width select.
pub fn filter_trigger(state: &FilterState) -> Stateful<Div> {
    let chevron = icon(icons::CHEVRON_DOWN, ICON_CHEVRON_LG, TEXT_MUTED);
    let chevron = if state.open {
        chevron.with_transformation(Transformation::rotate(radians(std::f32::consts::PI)))
    } else {
        chevron
    };
    div()
        .id(("nav-filter", 0usize))
        .group(FILTER_GROUP)
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .h(px(ICON_BUTTON))
        .pl(px(ROW_PAD_X))
        .pr(px(R_CONTROL))
        .gap(px(TRIGGER_GAP))
        .rounded(px(R_CONTROL))
        .text_size(px(FS_LG))
        .font_weight(FontWeight::SEMIBOLD)
        // NOT `relative(LINE_UI)`: 13 x 1.45 = 18.85 leaves the line box at
        // 53.575 inside the 28px control, taffy rounds that to 54, and the run
        // lands a pixel below the prototype. 20.5 puts the box top at a whole
        // 53 and the baseline at 63.25 - measured cap band y58-67, ink bottom
        // y70, matching 00-target-soft.png exactly.
        .line_height(px(20.5))
        // An open trigger wears its hover face: the menu is the hover made
        // permanent, so the control does not blink when the pointer leaves.
        .when(state.open, |open| {
            open.bg(rgb(HOVER)).text_color(rgb(TEXT_STRONG))
        })
        .when(!state.open, |shut| shut.text_color(rgb(TEXT)))
        .hover_control()
        .press_control()
        .child(
            div()
                .min_w_0()
                .truncate()
                .group_hover(FILTER_GROUP, |style| style.text_color(rgb(TEXT_STRONG)))
                .child(state.label.clone()),
        )
        .child(chevron)
}

/// The floating filter menu: `--menu` ground, the two-layer float shadow,
/// and **no border** — Soft's elevation is shadow and fill, never a line.
/// The caller pushes `filter_option` children and defers the whole thing.
pub fn filter_menu() -> Div {
    div()
        .absolute()
        .top(px(MENU_TOP))
        .left(px(ROW_PAD_X))
        .right(px(ROW_PAD_X))
        .flex()
        .flex_col()
        .gap(px(ROW_GAP))
        .p(px(MENU_PAD))
        .rounded(px(R_MENU))
        .bg(rgb(MENU))
        .shadow(vec![
            BoxShadow {
                color: rgba(SHADOW_FAR).into(),
                offset: point(px(0.), px(SHADOW_FAR_Y)),
                blur_radius: px(SHADOW_FAR_BLUR),
                spread_radius: px(SHADOW_FAR_SPREAD),
            },
            BoxShadow {
                color: rgba(SHADOW_NEAR).into(),
                offset: point(px(0.), px(SHADOW_NEAR_Y)),
                blur_radius: px(SHADOW_NEAR_BLUR),
                spread_radius: px(0.),
            },
        ])
}

/// One filter row. The selected Project is named in white at weight 500 and
/// carries a trailing check — the only tick the nav draws.
pub fn filter_option(index: usize, option: &FilterOption) -> Stateful<Div> {
    div()
        .id(("nav-filter-option", index))
        .group(FILTER_OPTION_GROUP)
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .min_h(px(MENU_ROW_H))
        .pl(px(MENU_ROW_PAD_L))
        .pr(px(ROW_PAD_X))
        .gap(px(ROW_PAD_X))
        .rounded(px(R_CONTROL))
        .text_size(px(FS_MD))
        .when(option.selected, |on| {
            on.text_color(rgb(TEXT_STRONG))
                .font_weight(FontWeight::MEDIUM)
        })
        .when(!option.selected, |off| off.text_color(rgb(TEXT_2)))
        .hover_row()
        .press_row()
        .child(
            div()
                .min_w_0()
                .truncate()
                .group_hover(FILTER_OPTION_GROUP, |style| {
                    style.text_color(rgb(TEXT_STRONG))
                })
                .child(option.label.clone()),
        )
        .children(
            option
                .selected
                .then(|| icon(icons::CHECK, ICON_CHEVRON_LG, TEXT_MUTED)),
        )
}

/// The filter menu's last row: a verb, not an option — `Add Project…`
/// with a `+` mark, in the muted ink until hovered. The caller wires the
/// press to the folder picker.
pub fn filter_action(index: usize, label: &'static str) -> Stateful<Div> {
    div()
        .id(("nav-filter-action", index))
        .group(FILTER_OPTION_GROUP)
        .flex()
        .items_center()
        .w_full()
        .min_h(px(MENU_ROW_H))
        .mt(px(MENU_PAD))
        .pl(px(MENU_ROW_PAD_L))
        .pr(px(ROW_PAD_X))
        .gap(px(ROW_ICON_GAP))
        .rounded(px(R_CONTROL))
        .text_size(px(FS_MD))
        .text_color(rgb(TEXT_2))
        .hover_row()
        .press_row()
        .child(
            icon(icons::PLUS, ROW_ICON, TEXT_MUTED)
                .group_hover(FILTER_OPTION_GROUP, |style| style.text_color(rgb(TEXT))),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .group_hover(FILTER_OPTION_GROUP, |style| {
                    style.text_color(rgb(TEXT_STRONG))
                })
                .child(label),
        )
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
/// first take a 16px margin — the caller applies it from the index, because
/// only the caller knows which block is first once the filter has run.
pub fn group_block() -> Div {
    div().relative().flex().flex_col().flex_shrink_0()
}

/// The 16px band between two Group blocks — real space the prototype
/// already draws, doubling as the "insert between these two" drop target.
pub fn group_gap(index: usize) -> Stateful<Div> {
    div()
        .id(("group-gap", index))
        .debug_selector(move || format!("group-gap-{index}"))
        .flex_shrink_0()
        .h(px(GROUP_GAP))
}

/// "Insert above the first Group", which has no band of its own: the tree
/// starts at its own padding and the prototype draws nothing there. So the
/// target is absolute — laid over the first Group header's top edge, taking
/// no layout and, without `occlude`, stealing none of its clicks either.
pub fn group_gap_lead(index: usize) -> Stateful<Div> {
    div()
        .id(("group-gap", index))
        .debug_selector(move || format!("group-gap-{index}"))
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(MEMBERS_TOP))
}

/// "Append after the last member", by the same trick: the 2px the members
/// column already leaves below its last row, claimed as a drop target.
pub fn member_tail(id: GroupId) -> Stateful<Div> {
    div()
        .id(("member-tail", id.get() as usize))
        .debug_selector(move || format!("member-tail-{}", id.get()))
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
        .h(px(MEMBER_GAP))
}

/// The 43px Group parent row: the title, then its Projects summary.
/// **The Group is what carries the selected fill** — a Thread row never
/// does — and the current Group also takes the white title. No provider
/// mark, no checkout line, no disclosure glyph, no member count.
#[cfg(test)]
pub fn group_row(row: &GroupBlock) -> Stateful<Div> {
    group_row_with_title(row, row.title.clone())
}

/// `group_row` with the title leaf supplied by the caller — the cockpit
/// hands in a click-to-rename wrapper, or the live editor while renaming.
/// The cell around it is unchanged either way: the geometry below is what
/// makes the title truncate at all, and an editor swapped in at the row
/// level instead would take the Project line with it.
pub fn group_row_with_title(row: &GroupBlock, title: impl IntoElement) -> Stateful<Div> {
    row_frame(
        ("nav-group", row.id.get() as usize),
        GROUP_ROW_H,
        row.current,
    )
    .debug_selector({
        let id = row.id;
        move || format!("nav-group-{}", id.get())
    })
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
    // own content box.
    .child(
        div()
            .w(px(ROW_TEXT_W))
            .h(px(TITLE_LG_H))
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w(px(ROW_TEXT_W + TRUNCATE_SLOP))
                    .max_w(px(ROW_TEXT_W + TRUNCATE_SLOP))
                    .truncate()
                    .h(px(TITLE_LG_H))
                    .text_size(px(FS_LG))
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(relative(LINE_TIGHT))
                    .text_color(rgb(if row.current { TEXT_STRONG } else { TEXT }))
                    .child(title),
            ),
    )
    .child(meta_line(icons::FOLDER, row.projects.clone(), TEXT_2))
}

/// The members container, and the one line the whole Soft system draws: a
/// 1px rail 7px left of the indented rows, inset 3px top and bottom. Square
/// ends, no radius, full opacity. It is the indent made visible, so it is
/// absolute and takes no layout of its own.
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
                .bg(rgb(GROUP_RAIL)),
        )
        .children(rows)
}

/// The 56.5px Thread row: title and provider mark on line 1, the Project on
/// line 2, the checkout on line 3. The prototype's grid is
/// `minmax(0, 1fr) 14px` with an 8px column gap; gpui's grid has uniform
/// tracks only, so line 1 is flex — a `flex_1().min_w_0()` title beside a
/// fixed 14px mark is the same two columns, and the mark's box is drawn even
/// when the provider is unknown so the title never widens by 22px.
///
/// The row never carries the selected fill: that belongs to its Group.
#[cfg(test)]
pub fn thread_row(row: &ThreadRow) -> Stateful<Div> {
    thread_row_with_title(row, row.name.clone())
}

/// `thread_row` with the title leaf supplied by the caller — see
/// `group_row_with_title`.
pub fn thread_row_with_title(row: &ThreadRow, title: impl IntoElement) -> Stateful<Div> {
    row_frame(
        ("nav-thread", row.thread.get() as usize),
        THREAD_ROW_H,
        row.current,
    )
    .debug_selector({
        let thread = row.thread;
        move || format!("nav-thread-{}", thread.get())
    })
    .child(
        div()
            .flex()
            .items_center()
            .gap(px(ROW_PAD_X))
            .child(status_dot(row.status))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .h(px(TITLE_MD_H))
                    .text_size(px(FS_MD))
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(relative(LINE_TIGHT))
                    .text_color(rgb(if row.current { TEXT_STRONG } else { TEXT }))
                    .child(title),
            )
            .child(provider_mark(row.provider, PROVIDER_MARK)),
    )
    .child(meta_line(icons::FOLDER, row.project.clone(), TEXT_2))
    .child(
        meta_line(icons::BRANCH, row.branch.clone(), TEXT_MUTED)
            .child(since_tail(row.last_used.clone())),
    )
}

/// The age at the tail of a row's last line — `40m`, `2h`, `3d`. It is
/// pushed right by its own auto margin rather than by a spacer, so a line
/// whose branch is unknown still puts the age where every other row's age
/// is. Never a date: the nav says how long ago, and the Pane says when.
fn since_tail(label: Option<SharedString>) -> Div {
    let cell = div().flex_shrink_0().ml_auto().pl(px(ROW_ICON_GAP));
    let Some(label) = label else {
        return cell;
    };
    cell.text_size(px(FS_SM))
        .line_height(relative(LINE_TIGHT))
        .text_color(rgb(TEXT_MUTED))
        .child(label)
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

/// What a Project filter that matches nothing says. It names the Project
/// rather than shrugging, so the way out is obvious.
pub fn empty_filter(project: &str) -> Div {
    div()
        .my(px(RAIL_ITEMS_TOP))
        .mx(px(ROW_PAD_X))
        .text_size(px(FS_MD))
        .text_color(rgb(TEXT_MUTED))
        .child(SharedString::from(format!(
            "No Groups or Threads in {project}."
        )))
}

/// The badge that follows the pointer while a row is being dragged into a
/// Group. It rides the menu ground — it is floating, like a menu is.
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
        .rounded(px(R_TIGHT))
        .child(title)
}

/// A Thread title that can be renamed — `rename_target_group`'s twin, on
/// the smaller row, and equally unwashed.
pub fn rename_target_thread(thread: ThreadId, title: SharedString) -> Stateful<Div> {
    div()
        .id(("rename-thread", thread.get() as usize))
        .debug_selector(move || format!("rename-thread-{}", thread.get()))
        .min_w_0()
        .truncate()
        .rounded(px(R_TIGHT))
        .child(title)
}

pub fn drag_badge(label: SharedString) -> Div {
    div()
        .bg(rgb(MENU))
        .rounded(px(R_CONTROL))
        .px(px(ROW_PAD_X))
        .py(px(ROW_PAD_Y))
        .text_size(px(FS_SM))
        .text_color(rgb(TEXT))
        .child(label)
}

/// The collapsed rail. `filtered` reaches `rail_filter`, not the column —
/// the parameter is kept so callers pass the one fact the rail's contents
/// need through a single door.
pub fn rail(_filtered: bool) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .items_center()
        .py(px(RAIL_PAD_Y))
}

/// The rail's filter button: the one affordance a 56px column has room for.
/// Its glyph brightens to `--text` when a Project filter is active — the
/// only way the collapsed nav can admit it is hiding Threads.
pub fn rail_filter(filtered: bool) -> Stateful<Div> {
    let resting = if filtered { TEXT } else { TEXT_MUTED };
    div()
        .id(("nav-rail-filter", 0usize))
        .group(RAIL_FILTER_GROUP)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .rounded(px(R_CONTROL))
        .hover_control()
        .press_control()
        .child(
            icon(icons::CHEVRON_DOWN, ICON_BUTTON_GLYPH, resting)
                .group_hover(RAIL_FILTER_GROUP, |style| style.text_color(rgb(TEXT))),
        )
}

/// The rail's item column. It scrolls, and it shows no thumb: 28px marks
/// are already the coarsest possible index, and a bar beside them would be
/// the second line in a system that draws none.
pub fn rail_items() -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .items_center()
        .gap(px(MEMBER_GAP))
        .mt(px(RAIL_ITEMS_TOP))
        // The prototype scrolls this column; gpui can only scroll a
        // `Stateful`, and the pinned signature is a plain `Div`, so the
        // overflow is clipped rather than smeared over the Cockpit. At
        // 900px the rail holds 30 items before it matters.
        .overflow_y_hidden()
}

/// One rail item: a Thread reduced to its provider logomark. `current` is
/// the Group's, not the Thread's — the same fill, carried by the same
/// selection, at the same strength as the expanded tree.
pub fn rail_item(row: &ThreadRow, current: bool) -> Stateful<Div> {
    let cell = div()
        .id(("nav-rail-item", row.thread.get() as usize))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .rounded(px(R_CONTROL))
        .child(provider_mark(row.provider, PROVIDER_MARK));
    if current {
        return cell.bg(rgb(FILL)).hover_carried().press_row();
    }
    cell.hover_row().press_row()
}

/// The frame both row kinds share: the 6px-radius box, its padding, the
/// 1px gap between stacked lines, and the fill language. The height is
/// fixed so a row that cannot resolve its Project or its checkout still
/// occupies exactly the space it will occupy once the cache fills.
///
/// `carries_fill` is only ever true for a Group: hover cannot wash over a
/// ground stronger than itself, so a carrying row steps its ground up
/// instead (`FILL` → `FILL_HOVER`) rather than being washed down.
fn row_frame(id: (&'static str, usize), height: f32, carries_fill: bool) -> Stateful<Div> {
    let frame = div()
        .id(id)
        // The current mark hangs off this box's left edge.
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h(px(height))
        .px(px(ROW_PAD_X))
        .py(px(ROW_PAD_Y))
        .gap(px(ROW_GAP))
        .rounded(px(R_CONTROL));
    let frame = if carries_fill {
        frame.bg(rgb(FILL)).hover_carried().press_row()
    } else {
        frame.hover_row().press_row()
    };
    // Rows are draggable into Groups, so they wear the open hand rather than
    // the pointer: the drag is the row's second verb, and the only one the
    // cursor can advertise before the press. It is set **after** the hover
    // role, whose `cursor_pointer` would otherwise overwrite it — the roles
    // in `pointer.rs` set the base cursor, not a hover refinement.
    frame.cursor(CursorStyle::OpenHand)
}

/// A Project or checkout line: a 12px mark, 5px, then the label. When the
/// fact is unknown the line is drawn **empty** — icon included — and keeps
/// its height. A row never invents a Project it cannot name.
fn meta_line(mark: &'static str, label: Option<SharedString>, ink: u32) -> Div {
    let line = div().flex().items_center().h(px(META_H));
    let Some(label) = label else {
        return line;
    };
    line.gap(px(ROW_ICON_GAP))
        .child(icon(mark, ROW_ICON, ink))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(FS_SM))
                .line_height(relative(LINE_TIGHT))
                .text_color(rgb(ink))
                .child(label),
        )
}

/// The provider logomark in its brand colour, or an empty box of the same
/// width when the provider is unknowable (an unreadable parked log). The
/// box is never a placeholder glyph and never a `cl` / `cx` string: it holds
/// the column open and says nothing.
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
            branch: Some("codex/prototype-32".into()),
            provider,
            current,
            last_used: Some("2h".into()),
        }
    }

    fn group(current: bool) -> GroupBlock {
        GroupBlock {
            id: GroupId::new(1),
            title: "Project-scoped navigation prototype".into(),
            projects: Some("ferrite".into()),
            current,
            members: vec![thread(Some(Provider::Codex))],
        }
    }

    /// The selection rule: the fill lands on the focused Thread's row and
    /// on the Group holding it — and on nothing else. A Thread that merely
    /// sits in the current Group is not itself current.
    #[test]
    fn only_the_current_row_carries_the_selected_fill() {
        let fill = |mut drawn: Stateful<Div>| drawn.style().background.clone();
        assert_eq!(
            fill(group_row(&group(true))),
            Some(rgb(FILL).into()),
            "the current Group carries the selected fill"
        );
        assert_eq!(fill(group_row(&group(false))), None);
        assert_eq!(
            fill(thread_row(&current_thread(Some(Provider::Claude), true))),
            Some(rgb(FILL).into()),
            "the focused Thread's own row carries it too"
        );
        assert_eq!(
            fill(thread_row(&thread(Some(Provider::Claude)))),
            None,
            "a Thread that is not focused is not filled by its Group's state"
        );
    }

    /// Every row is a drag source before it is a button, so it wears the
    /// open hand — and it wears it whether or not it is the current row
    /// (#26's skip rule is about the wash, never about the cursor).
    #[test]
    fn every_row_and_rail_item_advertises_its_grab() {
        let cursor = |mut drawn: Stateful<Div>| drawn.style().mouse_cursor;
        assert_eq!(
            cursor(thread_row(&thread(None))),
            Some(CursorStyle::OpenHand)
        );
        assert_eq!(cursor(group_row(&group(true))), Some(CursorStyle::OpenHand));
        assert_eq!(
            cursor(group_row(&group(false))),
            Some(CursorStyle::OpenHand)
        );
        assert_eq!(
            cursor(rail_item(&thread(Some(Provider::Codex)), true)),
            Some(CursorStyle::PointingHand),
            "a rail item is a jump, not a drag handle"
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
            last_used: None,
        };
        let height = |mut drawn: Stateful<Div>| drawn.style().size.height;
        assert_eq!(height(thread_row(&bare)), height(thread_row(&thread(None))));
        assert_eq!(
            height(thread_row(&bare)),
            height(thread_row(&current_thread(None, true))),
            "the current row is the same box as any other, only filled"
        );
        assert_eq!(
            height(thread_row(&bare)),
            Some(px(THREAD_ROW_H).into()),
            "6 + 15 + 1 + 13.75 + 1 + 13.75 + 6"
        );
        assert_eq!(
            height(group_row(&group(false))),
            Some(px(GROUP_ROW_H).into()),
            "6 + 16.25 + 1 + 13.75 + 6"
        );
    }

    /// The nav column draws no border on any edge: Soft separates the
    /// `#232323` column from the `#0e0e0e` Cockpit by fill contrast alone.
    #[test]
    fn the_column_has_no_edge() {
        let mut column = shell(false);
        let style = column.style();
        assert_eq!(style.background, Some(rgb(NAV).into()));
        assert!(style.border_widths.right.is_none());
        assert_eq!(style.size.width, Some(px(WIDTH).into()));
        let mut rail = shell(true);
        assert_eq!(rail.style().size.width, Some(px(RAIL_WIDTH).into()));
    }
}
