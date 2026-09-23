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
//! Every row lays out lead slot · text · mark (see the WP-G section of
//! `theme.rs`), so a Thread's dot, a Group's glyph, the Parked chevron and
//! the head's folder share one axis, and every title, label and meta line
//! starts on the next. The meta line hangs under the title — `project ·
//! branch` — and its tail (subagents · age) ends under the provider mark.
//!
//! The voice is mono, the chrome face. A Thread title is body weight in
//! `TEXT`; a Group title is the one `W_LABEL` title of its block; metadata
//! is `TEXT_MUTED`. Colour is state: the status dot is the only hue in a
//! row, provider marks are monochrome, and selection is one `FILL` on the
//! focused Thread's row with its title in `TEXT_STRONG` — nothing else in
//! the tree fills.
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

use crate::components;
use crate::icons::{self, icon};
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::*;

/// The nav's two widths—286px, and the platform rail cmd-b folds it to.
/// macOS uses the traffic-light reserve; other platforms use 56px.
/// `CockpitView::cell()` subtracts whichever is live, so the nav stays part
/// of the semantic-zoom input rather than a special case.
pub use crate::theme::{NAV_RAIL_WIDTH as RAIL_WIDTH, NAV_WIDTH as WIDTH};

/// A row's title line: `FS_UI` on the stacked rows' 16px line box.
const TITLE_H: f32 = LH_TIGHT;
/// The meta line: `FS_SM` on 16px. A row keeps this height even when the
/// facts are unknown, so nothing reflows when a cache fills.
const META_H: f32 = LH_META;

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
    pub members: Vec<ThreadRow>,
}

/// One Thread's row — identical whether it is a Group member or a solo; only
/// the container differs. Status dot, title and provider mark on line 1;
/// `project · branch` hanging under the title on line 2, with the subagent
/// count and the age at its tail.
#[derive(Clone)]
pub struct ThreadRow {
    pub thread: ThreadId,
    pub name: SharedString,
    /// What the Thread is doing right now — the one glance the operator
    /// asked for from the tree: which agents are working, which wait.
    pub status: RowStatus,
    /// `None` → line 2 starts at the branch, or is empty and keeps its
    /// height.
    pub project: Option<SharedString>,
    /// The branch the Thread's checkout is on, from the facts cache.
    /// `None` says nothing; it is never guessed.
    pub branch: Option<SharedString>,
    /// `None` → no logomark. Never a `cl`/`cx` string.
    pub provider: Option<Provider>,
    /// This is the focused Pane's Thread: it carries the tree's one selected
    /// fill and the `TEXT_STRONG` title.
    pub current: bool,
    /// How long since the Thread was last used — `40m`, `2h`, `3d` — at the
    /// tail of the Project line. `None` says nothing at all.
    pub last_used: Option<SharedString>,
    /// Subagents known for this Thread. Zero draws nothing; a positive
    /// count is named on line 2 so the number is meaningful without a legend.
    pub subagents: usize,
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

/// The status dot before a row's title, one recipe for the tree and the
/// rail: running green (the only green in the column, and it means live),
/// a Decision amber, closed or failing red, idle the idle ink, and parked a
/// hollow ring.
///
/// A **working** Thread's dot breathes: a halo behind it swells and fades
/// on a 1.4s loop. Motion is the one thing a still row cannot fake, and
/// inference is the one fact the operator scans the tree for. A failing
/// Thread is still inferring, so it breathes too, in its failure's ink.
/// Under reduced motion the halo holds still at its dimmest. The halo is
/// absolute inside a fixed `STATUS_DOT` box, so nothing in the row moves.
fn status_dot(thread: ThreadId, status: RowStatus, reduce_motion: bool) -> AnyElement {
    let id = ("nav-working", thread.get() as usize);
    match status {
        RowStatus::Working => components::pulsing_dot(id, RUNNING, RUNNING_HALO, reduce_motion),
        RowStatus::Failing => components::pulsing_dot(id, BLOCKED, NAV_FAILING_HALO, reduce_motion),
        status => dot_face(status).into_any_element(),
    }
}

/// The still face of a status: the dot alone, no halo.
fn dot_face(status: RowStatus) -> Div {
    match status {
        RowStatus::Working => components::status_dot(RUNNING),
        RowStatus::Failing | RowStatus::Blocked => components::status_dot(BLOCKED),
        RowStatus::Attention => components::status_dot(ATTENTION),
        RowStatus::Idle => components::status_dot(IDLE),
        RowStatus::Parked => components::status_ring(TEXT_MUTED),
    }
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
    components::icon_button("add-thread", icons::PLUS, "New Thread", cx)
        .debug_selector(|| "add-thread".into())
}

/// The rail's primary creation door gets the same generous target as its
/// Thread avatars; the expanded header retains its denser 28px control.
pub fn rail_add_thread_button(cx: &App) -> Button {
    components::icon_button("rail-add-thread", icons::PLUS, "New Thread", cx)
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
        .tooltip(if open {
            "Close thread order menu"
        } else if active {
            "Threads grouped by Project"
        } else {
            "Thread list order"
        })
        .child(
            icon(
                icons::LIST_FILTER,
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
        .child(components::menu_section("Order threads by", None, None))
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
/// Project's name and its row count in the metadata voice — a quiet label
/// over its rows, not a card. The caller hangs `project_add_button` on the
/// end: the heading is the only place a Project is named in this view, so
/// it is where a new Thread in that Project is asked for. No right inset,
/// so the `+` glyph centres over the rows' provider marks.
pub fn project_section(label: SharedString, count: usize, first: bool) -> Div {
    div()
        .group(PROJECT_SECTION_GROUP)
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(NAV_SECTION_H))
        .when(!first, |section| section.mt(px(GROUP_GAP)))
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
                .child(div().flex_shrink_0().child(count.to_string())),
        )
}

/// New Thread in *this* Project. It keeps a heading's reserved slot at all
/// times — a control that appears only under the pointer cannot be found —
/// and rests at the muted ink, brightening when the pointer is anywhere on
/// the heading.
pub fn project_add_button(index: usize) -> Button {
    components::button(("nav-project-add", index))
        .tab_stop(true)
        .debug_selector(move || format!("nav-project-add-{index}"))
        .group(PROJECT_ADD_GROUP)
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .tooltip("New Thread in this Project")
        .child(
            icon(icons::PLUS, ICON_BUTTON_GLYPH, TEXT_MUTED)
                .group_hover(PROJECT_SECTION_GROUP, |style| style.text_color(rgb(TEXT_2)))
                .group_hover(PROJECT_ADD_GROUP, |style| style.text_color(rgb(TEXT))),
        )
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
        .font_weight(W_LABEL)
        .line_height(px(LH_UI))
        // An open trigger wears its hover face: the menu is the hover made
        // permanent, so the control does not blink when the pointer leaves.
        .when(state.open, |open| {
            open.bg(rgb(FILL)).text_color(rgb(TEXT_STRONG))
        })
        .when(!state.open, |shut| shut.text_color(rgb(TEXT)))
        .hover_control()
        .press_control()
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
/// `All Projects` is a filter state, not a Project, and has nothing to
/// edit.
pub fn project_edit_button() -> gpui::component::button::Button {
    components::button("project-edit")
        .tab_stop(true)
        .debug_selector(|| "project-edit".into())
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .tooltip("Edit Project")
        .child(icon(icons::PENCIL, ROW_ICON, TEXT_MUTED))
}

/// The filter menu's last row: a verb, not an option — `Add Project…`
/// with a `+` mark. The caller sets a separator above it and wires the
/// press to the folder picker.
pub fn filter_action(index: usize, label: &'static str) -> Stateful<Div> {
    components::menu_row(
        ("nav-filter-action", index),
        &components::MenuItem::new(label).leading(icons::PLUS, TEXT_MUTED),
        false,
        false,
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

/// The 16px band between two Group blocks — real air between the blocks,
/// doubling as the "insert between these two" drop target.
pub fn group_gap(index: usize) -> Stateful<Div> {
    div()
        .id(("group-gap", index))
        .debug_selector(move || format!("group-gap-{index}"))
        .flex_shrink_0()
        .h(px(GROUP_GAP))
}

/// "Insert above the first Group", which has no band of its own: the tree
/// starts at its own padding and draws nothing there. So the
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

/// The 44px (`GROUP_ROW_H`) Group parent row: the four-Pane Group glyph in
/// the lead slot, the title — the block's one `W_LABEL` title — and its
/// Projects summary hanging under it. No fill: the one selected fill is the
/// focused member's. No provider mark, no disclosure glyph, no member
/// count.
#[cfg(test)]
pub fn group_row(row: &GroupBlock) -> Stateful<Div> {
    group_row_with_title(row, row.title.clone())
}

/// `group_row` with the title leaf supplied by the caller — the cockpit
/// hands in a click-to-rename wrapper, or the live editor while renaming.
/// The cell around it is unchanged either way: the geometry below is what
/// makes the title truncate at all, and an editor swapped in at the row
/// level instead would take the Project line with it. The title box sets
/// the 16px line the editor inherits, so renaming never moves the row.
pub fn group_row_with_title(row: &GroupBlock, title: impl IntoElement) -> Stateful<Div> {
    const TEXT_W: f32 = ROW_TEXT_W - NAV_TEXT_X;
    row_frame(("nav-group", row.id.get() as usize), GROUP_ROW_H, false)
        .debug_selector({
            let id = row.id;
            move || format!("nav-group-{}", id.get())
        })
        .flex_row()
        .items_start()
        .gap(px(NAV_LEAD_GAP))
        .child(group_header_icon(row.id))
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
            div()
                .w(px(TEXT_W))
                .flex()
                .flex_col()
                .gap(px(ROW_GAP))
                .child(
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
                )
                .child(meta_line(row.projects.clone(), None)),
        )
}

/// The members container, and the one line the tree draws: a 1px
/// `NAV_GROUP_RAIL` hanging from the Group glyph's centre, inset 3px top and
/// bottom. Square ends, no radius. It is the indent made visible, so it is
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
                .bg(rgba(NAV_GROUP_RAIL)),
        )
        .children(rows)
}

/// The 44px (`THREAD_ROW_H`) Thread row. Line 1: the status dot in the
/// lead slot, the title, the provider mark in a fixed 12px slot at the
/// right — drawn even when the provider is unknown, so the title never
/// widens. Line 2: `project · branch` hanging under the title, and the
/// subagent count and age at its tail, ending under the mark.
#[cfg(test)]
pub fn thread_row(row: &ThreadRow) -> Stateful<Div> {
    thread_row_with_title(row, row.name.clone(), false)
}

/// `thread_row` with the title leaf supplied by the caller — see
/// `group_row_with_title`. `reduce_motion` holds a working dot's halo still.
pub fn thread_row_with_title(
    row: &ThreadRow,
    title: impl IntoElement,
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
    .child(
        div()
            .flex()
            .items_center()
            .child(lead(status_dot(row.thread, row.status, reduce_motion)))
            .child(title_cell(row, title).ml(px(NAV_LEAD_GAP)))
            .child(mark_cell(row)),
    )
    .child(
        meta_line(row.project.clone(), row.branch.clone())
            .pl(px(NAV_TEXT_X))
            .child(meta_tail(row.thread, row.subagents, row.last_used.clone())),
    )
}

/// The grouped view has already named the Project, so its Thread rows keep
/// the title, state, provider, subagent count and recency on one line and
/// drop the Project line. A Thread that is still a Group member says so with
/// the Group glyph after its title, so every title keeps the same x.
pub fn project_thread_row_with_title(
    row: &ThreadRow,
    title: impl IntoElement,
    grouped: bool,
    reduce_motion: bool,
) -> Stateful<Div> {
    row_frame(
        ("nav-thread", row.thread.get() as usize),
        NAV_COMPACT_ROW_H,
        row.current,
    )
    .debug_selector({
        let thread = row.thread;
        move || format!("nav-thread-{}", thread.get())
    })
    .flex_row()
    .items_center()
    .child(lead(status_dot(row.thread, row.status, reduce_motion)))
    .child(title_cell(row, title).ml(px(NAV_LEAD_GAP)))
    .children(grouped.then(|| group_membership_indicator(row.thread)))
    .child(meta_tail(row.thread, row.subagents, row.last_used.clone()))
    .child(mark_cell(row))
}

/// A Thread row's title: one UI line in body weight that truncates, in
/// `title_ink`. The box sets the 16px line the rename editor inherits.
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

/// The focused Thread's title is the strongest ink in the tree; a parked
/// Thread's steps down a rung, so what is running reads first.
fn title_ink(row: &ThreadRow) -> u32 {
    if row.current {
        TEXT_STRONG
    } else if row.status == RowStatus::Parked {
        TEXT_2
    } else {
        TEXT
    }
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
fn group_header_icon(group: GroupId) -> Stateful<Div> {
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
        .tooltip(|window, cx| Tooltip::new("Group").build(window, cx))
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

/// The compact facts at the right edge of line 2. They stay one group so
/// free space separates them from the Project, not from each other.
fn meta_tail(thread: ThreadId, subagents: usize, since: Option<SharedString>) -> Div {
    let separated = subagents > 0 && since.is_some();
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .ml_auto()
        .pl(px(NAV_MARK_GAP))
        .gap(px(NAV_TAIL_GAP))
        .child(subagent_tail(thread, subagents))
        .children(separated.then(seam))
        .child(since_tail(thread, since))
}

/// The number of subagents attached to a Thread. Its branching mark keeps
/// the compact count distinct from recency without spelling out a noun.
/// Threads without children spend no space here.
fn subagent_tail(thread: ThreadId, count: usize) -> Stateful<Div> {
    let cell = components::text_meta()
        .id(("nav-subagents", thread.get() as usize))
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(NAV_TAIL_GAP))
        .debug_selector(move || format!("nav-subagents-{}", thread.get()));
    let Some(label) = subagent_label(count) else {
        return cell;
    };
    let tooltip = if count == 1 {
        "1 subagent".to_owned()
    } else {
        format!("{count} subagents")
    };
    cell.tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .child(icon(icons::SUBAGENTS, ROW_ICON, TEXT_MUTED))
        .child(label)
}

fn subagent_label(count: usize) -> Option<SharedString> {
    (count > 0).then(|| SharedString::from(count.to_string()))
}

/// The age at the tail of a row's last line — `40m`, `2h`, `3d`. It is
/// pushed right by its own auto margin rather than by a spacer, so a line
/// whose Project is unknown still puts the age where every other row's age
/// is. Never a date: the nav says how long ago, and the Pane says when.
fn since_tail(thread: ThreadId, label: Option<SharedString>) -> Div {
    let cell = components::text_meta()
        .flex_shrink_0()
        .debug_selector(move || format!("nav-since-{}", thread.get()));
    let Some(label) = label else {
        return cell;
    };
    cell.child(label)
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

/// What an empty tree says, on the rows' text column: a line in the
/// secondary ink and, where there is one, a way forward in the metadata
/// voice. Filtered to a Project it names the Project rather than shrugging;
/// when the Parked section below holds Threads the filter admits, it says
/// *open*, so the operator is not told a tree is empty while its Threads
/// sit one fold away.
pub fn empty_filter(project: Option<&str>, parked_below: bool) -> Div {
    let (message, hint) = match (project, parked_below) {
        (Some(project), false) => (format!("No Groups or Threads in {project}."), None),
        (Some(project), true) => (
            format!("No open Groups or Threads in {project}."),
            Some("Its parked Threads wait below."),
        ),
        (None, false) => ("No Threads yet.".to_string(), Some("+ starts one.")),
        (None, true) => (
            "No open Threads.".to_string(),
            Some("Parked Threads wait below."),
        ),
    };
    div()
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
        .children(hint.map(|hint| components::text_meta().child(hint)))
}

/// A refusal from the last Group change, at the top of the tree: the
/// metadata voice in the Decision ink on its wash, until the next change
/// succeeds.
pub fn notice(text: SharedString) -> Div {
    components::text_meta()
        .flex_shrink_0()
        .px(px(ROW_PAD_X))
        .py(px(ROW_PAD_Y))
        .rounded(px(R_CONTROL))
        .text_color(rgb(ATTENTION))
        .bg(rgba(ATTENTION_WASH))
        .child(text)
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
        .px(px(ROW_PAD_X))
        .rounded(px(R_CONTROL))
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
        .child(
            div()
                .flex_shrink_0()
                .ml(px(NAV_TAIL_GAP))
                .child(count.to_string()),
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
        .gap(px(MEMBER_GAP))
        .pt(px(NAV_RAIL_PAD_Y))
}

/// The rail's filter button: the compact column's Project affordance.
/// Its glyph brightens to `TEXT` when a Project filter is active — the
/// only way the collapsed nav can admit it is hiding Threads.
pub fn rail_filter(filtered: bool) -> Stateful<Div> {
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
        .child(
            icon(icons::CHEVRON_DOWN, ICON_BUTTON_GLYPH, resting)
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

/// One rail item: a Thread reduced to a two-letter monogram plus its
/// still status dot in the corner (the rail's box is too tight for a
/// breathing halo). Provider logos made every Codex or Claude Thread
/// identical; the monogram keeps the rail scannable while the tooltip
/// preserves the full name. The focused Thread's item carries the tree's
/// one selected fill and the strong ink.
pub fn rail_item(row: &ThreadRow, current: bool) -> Button {
    let title = row.name.clone();
    let monogram = rail_monogram(&row.name);
    components::button(("nav-rail-item", row.thread.get() as usize))
        .debug_selector(move || format!("nav-rail-item-{}", row.thread.get()))
        .w(px(NAV_RAIL_CONTROL))
        .h(px(NAV_RAIL_CONTROL))
        .p_0()
        .tooltip(title.clone())
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
                .text_size(px(if cfg!(target_os = "macos") {
                    FS_UI
                } else {
                    FS_SM
                }))
                .font_weight(W_BODY)
                .text_color(rgb(if current { TEXT_STRONG } else { TEXT_2 }))
                .child(monogram)
                .child(
                    div()
                        .absolute()
                        .right(px(NAV_RAIL_DOT_INSET))
                        .bottom(px(NAV_RAIL_DOT_INSET))
                        .child(dot_face(row.status)),
                ),
        )
}

fn rail_monogram(name: &str) -> String {
    let monogram: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
    if monogram.is_empty() {
        "?".into()
    } else {
        monogram
    }
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
        .flex_col()
        .flex_shrink_0()
        .h(px(height))
        .px(px(ROW_PAD_X))
        .py(px(ROW_PAD_Y))
        .gap(px(ROW_GAP))
        .rounded(px(R_CONTROL));
    let frame = if selected {
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

/// A row's meta line: `project · branch` in the metadata voice, drawn from
/// whichever facts are known. With both, the Project takes at most half the
/// line and the branch truncates into the rest; with one, it takes the
/// line; with neither the line is **empty** and keeps its height. A row
/// never invents a Project or a branch it cannot name.
fn meta_line(project: Option<SharedString>, branch: Option<SharedString>) -> Div {
    let line = components::text_meta()
        .flex()
        .items_center()
        .min_w_0()
        .h(px(META_H))
        .gap(px(NAV_TAIL_GAP));
    match (project, branch) {
        (Some(project), Some(branch)) => line
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(relative(0.5))
                    .truncate()
                    .child(project),
            )
            .child(seam())
            .child(div().flex_1().min_w_0().truncate().child(branch)),
        (Some(fact), None) | (None, Some(fact)) => {
            line.child(div().flex_1().min_w_0().truncate().child(fact))
        }
        (None, None) => line,
    }
}

/// The provider logomark, monochrome in the metadata ink — colour is state,
/// and a green Codex mark beside a green running dot would read as one — or
/// an empty box of the same width when the provider is unknowable (an
/// unreadable parked log). The box is never a placeholder glyph and never a
/// `cl` / `cx` string: it holds the column open and says nothing.
fn provider_mark(provider: Option<Provider>, size: f32) -> AnyElement {
    match provider {
        Some(Provider::Codex) => icon(icons::CODEX, size, TEXT_MUTED).into_any_element(),
        Some(Provider::Claude) => icon(icons::CLAUDE, size, TEXT_MUTED).into_any_element(),
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
            last_used: Some("2h".into()),
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

    /// The focused title is the strongest ink in the tree, a parked title
    /// steps down a rung, and every other title is the body ink.
    #[test]
    fn titles_rank_focus_then_running_then_parked() {
        assert_eq!(title_ink(&current_thread(None, true)), TEXT_STRONG);
        assert_eq!(title_ink(&thread(None)), TEXT);
        let parked = ThreadRow {
            status: RowStatus::Parked,
            ..thread(None)
        };
        assert_eq!(title_ink(&parked), TEXT_2);
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
            last_used: None,
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
            "ROW_PAD_Y 6 + LH_TIGHT 16 + ROW_GAP 0 + LH_META 16 + ROW_PAD_Y 6"
        );
        assert_eq!(THREAD_ROW_H, 2.0 * ROW_PAD_Y + TITLE_H + ROW_GAP + META_H);
        assert_eq!(
            height(group_row(&group())),
            Some(px(GROUP_ROW_H).into()),
            "the same two lines as a Thread row"
        );
        assert_eq!(GROUP_ROW_H, THREAD_ROW_H);
        assert_eq!(
            height(project_thread_row_with_title(
                &bare,
                "thread-09",
                false,
                false
            )),
            Some(px(NAV_COMPACT_ROW_H).into()),
            "Project order's one-line row: the same padding around the title line"
        );
        assert_eq!(NAV_COMPACT_ROW_H, 2.0 * ROW_PAD_Y + TITLE_H);
    }

    /// The working halo is absolute inside a fixed dot box, so a Thread
    /// that starts inferring does not shift its own row — or any row under
    /// it — by a pixel.
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
    /// is still inferring), a Decision amber, idle muted, parked hollow.
    #[test]
    fn status_dots_say_state_and_green_only_means_live() {
        let fill = |status| dot_face(status).style().background.clone();
        assert_eq!(fill(RowStatus::Working), Some(rgb(RUNNING).into()));
        assert_eq!(fill(RowStatus::Failing), Some(rgb(BLOCKED).into()));
        assert_eq!(fill(RowStatus::Blocked), Some(rgb(BLOCKED).into()));
        assert_eq!(fill(RowStatus::Attention), Some(rgb(ATTENTION).into()));
        assert_eq!(fill(RowStatus::Idle), Some(rgb(IDLE).into()));
        let mut parked = dot_face(RowStatus::Parked);
        assert_eq!(parked.style().background, None, "a parked dot is a ring");
        assert_eq!(
            parked.style().border_color,
            Some(rgb(TEXT_MUTED).into()),
            "the ring is the metadata ink"
        );
    }

    /// The grid: the head's folder, every row's lead glyph and the Parked
    /// chevron sit on one axis, every title and meta line on the next, a
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
        assert_eq!(PROVIDER_MARK, NAV_LEAD_W, "the two glyph columns match");
        let mut head = nav_head();
        assert_eq!(
            head.style().padding.left,
            Some(px(NAV_TREE_PAD).into()),
            "the head takes the tree's inset"
        );
        let mut trigger = filter_trigger(&FilterState {
            label: "All Projects".into(),
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
        let mut heading = project_section("ferrite".into(), 3, true);
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
        let mut item = rail_item(&thread(Some(Provider::Codex)), true);
        assert_eq!(item.style().size.width, Some(px(36.0).into()));
        let mut filter = rail_filter(false);
        assert_eq!(filter.style().size.width, Some(px(36.0).into()));
    }

    #[test]
    fn rail_monograms_identify_threads_instead_of_repeating_provider_marks() {
        assert_eq!(rail_monogram("fix sidebar"), "FI");
        assert_eq!(rail_monogram("éclair polish"), "ÉC");
        assert_eq!(rail_monogram("---"), "?");
    }
}
