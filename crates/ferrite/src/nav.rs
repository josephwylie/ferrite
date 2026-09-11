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
//! What the nav deliberately does **not** draw, per the approved prototype:
//! no state word, no badges, no provider text tag, no section headers, no
//! dividers, and no border on the column itself. Thread state stays in its
//! compact dot; the selected fill lands on the **Group** and focused Thread.
//!
//! Every colour and metric is a `theme` token; this file holds no literal
//! of its own. Everything outside a Pane is the system UI face, which —
//! unlike the bundled mono family — exposes a real weight axis, so
//! `.font_weight(..)` is correct here.

use ferrite_core::groups::GroupId;
use ferrite_core::settings::ThreadListOrder;
use ferrite_core::store::Provider;
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::ThreadId;
use std::time::Duration;

use gpui::component::button::Button;
use gpui::component::tooltip::Tooltip;
use gpui::prelude::*;
use gpui::{
    div, point, pulsating_between, px, radians, relative, rgb, rgba, Animation, AnimationExt,
    AnyElement, BoxShadow, CursorStyle, Div, FontWeight, ScrollHandle, SharedString, Stateful,
    Transformation,
};

use crate::components;
use crate::icons::{self, icon};
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::{
    ATTENTION, BLOCKED, FILL, FONT_UI, FS_LG, FS_MD, FS_SM, GROUP_GAP, GROUP_RAIL, GROUP_ROW_H,
    ICON_BUTTON, ICON_BUTTON_GLYPH, ICON_CHEVRON_LG, IDLE, LINE_TIGHT, MEMBERS_TOP, MEMBER_GAP,
    MEMBER_INDENT, MENU, MENU_PAD, MENU_ROW_H, MENU_TOP, NAV, NAV_HEAD_H, NAV_TREE_PAD,
    NAV_TREE_PAD_B, PROVIDER_CLAUDE, PROVIDER_CODEX, PROVIDER_MARK, PULSE_MIN, RAIL_INSET,
    RAIL_OFFSET, ROW_GAP, ROW_ICON, ROW_ICON_GAP, ROW_PAD_X, ROW_PAD_Y, ROW_TEXT_W,
    RUNNING, RUNNING_HALO, R_CONTROL, R_MENU, R_TIGHT, SEP, SHADOW_FAR, SHADOW_FAR_BLUR,
    SHADOW_FAR_SPREAD, SHADOW_FAR_Y, SHADOW_NEAR, SHADOW_NEAR_BLUR, SHADOW_NEAR_Y, SOLOS_TOP,
    STATUS_DOT, STATUS_HALO_INSET, STATUS_PULSE_MS, TEXT, TEXT_2, TEXT_MUTED, TEXT_STRONG,
    THREAD_ROW_H, TRAFFIC_RESERVE, WIN_CHROME_H,
};

/// The nav's two widths—286px, and the platform rail cmd-b folds it to.
/// macOS uses the traffic-light reserve; other platforms use 56px.
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
/// The Group card's mark spans both text rows instead of reading as title
/// decoration. It is deliberately larger than the 12px inline row icons.
const GROUP_MARK_LG: f32 = 20.0;

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
const ORDER_GROUP: &str = "nav-order";
const PROJECT_SECTION_GROUP: &str = "nav-project-section";
const PROJECT_ADD_GROUP: &str = "nav-project-add";
const PARKED_GROUP: &str = "nav-parked";

// The handful of nav metrics `theme.rs` does not name, kept here rather
// than written inline so each one is said once and explained once.
//
/// 7px — enough separation for the leading folder, label, and trailing
/// chevron to remain legible as one compact field.
const TRIGGER_GAP: f32 = 7.0;
/// 9px — a filter option's leading inset. One more than a row's, so the
/// option's label hangs under the trigger's label rather than under its box.
const MENU_ROW_PAD_L: f32 = 9.0;
/// The collapsed rail begins below the native macOS titlebar controls.
/// Other platforms keep the compact inset used by the expanded column.
const RAIL_CHROME_PAD_T: f32 = if cfg!(target_os = "macos") {
    WIN_CHROME_H
} else {
    7.0
};
const RAIL_CHROME_PAD_B: f32 = 4.0;
/// 7px — the collapsed rail's own block padding.
const RAIL_PAD_Y: f32 = 7.0;
/// 12px — the gap between the rail's filter button and its first item, and
/// the empty-filter message's block margin.
const RAIL_ITEMS_TOP: f32 = 12.0;
/// 30px — a section heading's row: the Project headings in Project order,
/// and the Parked section's header at the foot of the column.
const SECTION_H: f32 = 30.0;
/// The most of the column the open Parked section may take. Its list
/// scrolls past this, so a hundred parked Threads never push the running
/// tree out of sight.
const PARKED_MAX_SHARE: f32 = 0.5;

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
    /// This Group holds the focused Pane's Thread: it carries the selected
    /// fill **and** the white title. The Group carries the fill; a Thread
    /// row never does.
    pub current: bool,
    pub members: Vec<ThreadRow>,
}

/// One Thread's row — identical whether it is a Group member or a solo; only
/// the container differs. Title, Project, and the provider mark in
/// the top-right corner, plus a subagent count when the Thread has children.
#[derive(Clone)]
pub struct ThreadRow {
    pub thread: ThreadId,
    pub name: SharedString,
    /// What the Thread is doing right now — the one glance the operator
    /// asked for from the tree: which agents are working, which wait.
    pub status: RowStatus,
    /// `None` → line 2 draws neither icon nor label, and keeps its height.
    pub project: Option<SharedString>,
    /// `None` → no logomark. Never a `cl`/`cx` string.
    pub provider: Option<Provider>,
    /// This is the focused Pane's Thread: it carries the selected fill and
    /// the white title. The Group around it carries the fill too, so the
    /// pair reads as one selection.
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

/// The status dot before a row's title: the Pane head's own colours, so
/// the tree and the Pane can never disagree. An idle Thread keeps a dim
/// dot — it is alive, just quiet — and a parked one a hollow ring.
///
/// A **working** Thread's dot breathes: a halo behind it swells and fades
/// on a 1.4s loop. Motion is the one thing a still row cannot fake, and
/// inference is the one fact the operator scans the tree for — every other
/// state stays as still as the column around it. The halo is absolute and
/// the box is a fixed `STATUS_DOT`, so nothing in the row moves with it.
fn status_dot(thread: ThreadId, status: RowStatus) -> AnyElement {
    let dot = div()
        .flex_shrink_0()
        .w(px(STATUS_DOT))
        .h(px(STATUS_DOT))
        .rounded_full();
    let dot = match status {
        RowStatus::Working => dot.bg(rgb(RUNNING)),
        RowStatus::Failing => dot.bg(rgb(RUNNING)).border_1().border_color(rgb(BLOCKED)),
        RowStatus::Attention => dot.bg(rgb(ATTENTION)),
        RowStatus::Blocked => dot.bg(rgb(BLOCKED)),
        RowStatus::Idle => dot.bg(rgb(IDLE)),
        RowStatus::Parked => dot.border_1().border_color(rgb(SEP)),
    };
    if !matches!(status, RowStatus::Working | RowStatus::Failing) {
        return dot.into_any_element();
    }
    let halo = div()
        .absolute()
        .left(px(-STATUS_HALO_INSET))
        .top(px(-STATUS_HALO_INSET))
        .w(px(STATUS_DOT + 2. * STATUS_HALO_INSET))
        .h(px(STATUS_DOT + 2. * STATUS_HALO_INSET))
        .rounded_full()
        .bg(rgba(RUNNING_HALO))
        .with_animation(
            ("nav-working", thread.get() as usize),
            Animation::new(Duration::from_millis(STATUS_PULSE_MS))
                .repeat()
                .with_easing(pulsating_between(PULSE_MIN, 1.0)),
            |halo, delta| halo.opacity(delta),
        );
    div()
        .relative()
        .flex_shrink_0()
        .w(px(STATUS_DOT))
        .h(px(STATUS_DOT))
        .child(halo)
        .child(dot)
        .into_any_element()
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
        .overflow_hidden()
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

/// The persistent door to a new Thread. It sits beside the Project filter,
/// reusing the same compact icon-control grammar as the rest of the nav.
/// The cockpit owns the click because opening a draft changes its roster.
pub fn add_thread_button() -> Button {
    components::button("add-thread")
        .debug_selector(|| "add-thread".into())
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .tooltip("New Thread")
        .child(icon(icons::PLUS, ICON_BUTTON_GLYPH, TEXT_MUTED))
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

/// Ordering menu anchored to the compact button rather than occupying the
/// full Project-filter width.
pub fn order_menu() -> Div {
    filter_menu().left_auto().w(px(218.)).child(
        div()
            .h(px(24.))
            .px(px(ROW_PAD_X))
            .flex()
            .items_center()
            .text_size(px(FS_SM))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(TEXT_MUTED))
            .child("Order threads by"),
    )
}

pub fn order_option(index: usize, label: &'static str, selected: bool) -> Button {
    components::button(("thread-list-order-option", index))
        .tab_stop(true)
        .debug_selector(move || format!("thread-list-order-option-{index}"))
        .group(FILTER_OPTION_GROUP)
        .w_full()
        .min_h(px(MENU_ROW_H))
        .pl(px(MENU_ROW_PAD_L))
        .pr(px(ROW_PAD_X))
        .rounded(px(R_CONTROL))
        .when(selected, |on| {
            on.bg(rgb(FILL))
                .text_color(rgb(TEXT_STRONG))
                .font_weight(FontWeight::MEDIUM)
        })
        .when(!selected, |off| off.text_color(rgb(TEXT_2)))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .min_w_0()
                .gap(px(ROW_PAD_X))
                .text_size(px(FS_MD))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .group_hover(FILTER_OPTION_GROUP, |style| {
                            style.text_color(rgb(TEXT_STRONG))
                        })
                        .child(label),
                )
                .children(selected.then(|| icon(icons::CHECK, ICON_CHEVRON_LG, TEXT))),
        )
}

/// A quiet Project label separates grouped runs without turning each one
/// into a card. The count helps scan long lists and costs no extra row.
/// The caller hangs `project_add_button` on the end: the heading is the
/// only place a Project is named in this view, so it is where a new Thread
/// in that Project is asked for.
pub fn project_section(label: SharedString, count: usize, first: bool) -> Div {
    div()
        .group(PROJECT_SECTION_GROUP)
        .flex()
        .items_center()
        .h(px(SECTION_H))
        .when(!first, |section| section.mt(px(SOLOS_TOP)))
        .px(px(ROW_PAD_X))
        .gap(px(ROW_ICON_GAP))
        .text_size(px(FS_SM))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(TEXT_2))
        .child(div().flex_1().min_w_0().truncate().child(label))
        .child(
            div()
                .font_weight(FontWeight::NORMAL)
                .text_color(rgb(TEXT_MUTED))
                .child(count.to_string()),
        )
}

/// New Thread in *this* Project. It keeps a heading's reserved slot at all
/// times — a control that appears only under the pointer cannot be found —
/// and rests at the muted weight the count beside it uses, brightening
/// when the pointer is anywhere on the heading.
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

/// The Project filter trigger rests transparently with its neighboring actions.
/// Its folder and edge-aligned chevron frame the current Project name; hover
/// and open states supply the ground only while the control is engaged.
pub fn filter_trigger(state: &FilterState) -> Stateful<Div> {
    let chevron = icon(icons::CHEVRON_DOWN, ICON_CHEVRON_LG, TEXT_MUTED);
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
            open.bg(rgb(FILL)).text_color(rgb(TEXT_STRONG))
        })
        .when(!state.open, |shut| shut.text_color(rgb(TEXT)))
        .hover_control()
        .press_control()
        .child(
            icon(icons::FOLDER, ROW_ICON, TEXT_MUTED)
                .group_hover(FILTER_GROUP, |style| style.text_color(rgb(TEXT))),
        )
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

/// The floating filter menu: `--menu` ground, the two-layer float shadow,
/// and **no border** — Soft's elevation is shadow and fill, never a line.
/// The caller pushes `filter_option` children and defers the whole thing.
pub fn filter_menu() -> Div {
    div()
        .absolute()
        .top(px(MENU_TOP))
        .left(px(ROW_PAD_X))
        .right(px(ROW_PAD_X))
        .cursor_default()
        .occlude()
        .flex()
        .flex_col()
        .gap(px(ROW_GAP))
        .p(px(MENU_PAD))
        .rounded(px(R_MENU))
        .bg(rgb(MENU))
        .shadow(vec![
            BoxShadow {
                inset: false,
                color: rgba(SHADOW_FAR).into(),
                offset: point(px(0.), px(SHADOW_FAR_Y)),
                blur_radius: px(SHADOW_FAR_BLUR),
                spread_radius: px(SHADOW_FAR_SPREAD),
            },
            BoxShadow {
                inset: false,
                color: rgba(SHADOW_NEAR).into(),
                offset: point(px(0.), px(SHADOW_NEAR_Y)),
                blur_radius: px(SHADOW_NEAR_BLUR),
                spread_radius: px(0.),
            },
        ])
}

/// One filter row. The selected Project carries a restrained fill as well as
/// white medium-weight type and a trailing check, so the current scope is
/// apparent before the operator starts scanning labels.
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
            on.bg(rgb(FILL))
                .text_color(rgb(TEXT_STRONG))
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
                .then(|| icon(icons::CHECK, ICON_CHEVRON_LG, TEXT)),
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
/// does — and the current Group also takes the white title. Its four-Pane
/// mark is the same one Project order uses for grouped Threads. No provider
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
    .flex_row()
    .items_center()
    .gap(px(ROW_PAD_X))
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
            .w(px(ROW_TEXT_W - GROUP_MARK_LG - ROW_PAD_X))
            .flex()
            .flex_col()
            .gap(px(ROW_GAP))
            .child(
                div()
                    .h(px(TITLE_LG_H))
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w(px(
                                ROW_TEXT_W - GROUP_MARK_LG - ROW_PAD_X + TRUNCATE_SLOP,
                            ))
                            .max_w(px(
                                ROW_TEXT_W - GROUP_MARK_LG - ROW_PAD_X + TRUNCATE_SLOP,
                            ))
                            .truncate()
                            .h(px(TITLE_LG_H))
                            .text_size(px(FS_LG))
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_height(relative(LINE_TIGHT))
                            .text_color(rgb(if row.current { TEXT_STRONG } else { TEXT }))
                            .child(title),
                    ),
            )
            .child(meta_line(icons::FOLDER, row.projects.clone(), TEXT_2)),
    )
    .child(group_header_icon(row.id))
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

/// The 41.75px Thread row: title and provider mark on line 1, the Project
/// on line 2. The prototype's grid is
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
            .child(status_dot(row.thread, row.status))
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
            .child(
                div()
                    .flex_shrink_0()
                    .debug_selector({
                        let thread = row.thread;
                        move || format!("nav-mark-{}", thread.get())
                    })
                    .child(provider_mark(row.provider, PROVIDER_MARK)),
            ),
    )
    .child(
        meta_line(icons::FOLDER, row.project.clone(), TEXT_2).child(meta_tail(
            row.thread,
            row.subagents,
            row.last_used.clone(),
        )),
    )
}

/// The grouped view has already named the Project, so its Thread rows keep
/// the useful title, state, provider, subagent count and recency while
/// dropping only the now-redundant Project label. This is the screenshot's
/// compact section rhythm, expressed in Ferrite's existing row grammar.
pub fn project_thread_row_with_title(
    row: &ThreadRow,
    title: impl IntoElement,
    grouped: bool,
) -> Stateful<Div> {
    row_frame(
        ("nav-thread", row.thread.get() as usize),
        ICON_BUTTON,
        row.current,
    )
    .debug_selector({
        let thread = row.thread;
        move || format!("nav-thread-{}", thread.get())
    })
    .flex()
    .flex_row()
    .items_center()
    .gap(px(ROW_PAD_X))
    .child(status_dot(row.thread, row.status))
    .children(grouped.then(|| group_membership_indicator(row.thread)))
    .child(
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_size(px(FS_MD))
            .font_weight(FontWeight::MEDIUM)
            .line_height(relative(LINE_TIGHT))
            .text_color(rgb(if row.current { TEXT_STRONG } else { TEXT }))
            .child(title),
    )
    .child(meta_tail(row.thread, row.subagents, row.last_used.clone()))
    .child(
        div()
            .flex_shrink_0()
            .debug_selector({
                let thread = row.thread;
                move || format!("nav-mark-{}", thread.get())
            })
            .child(provider_mark(row.provider, PROVIDER_MARK)),
    )
}

fn group_header_icon(group: GroupId) -> Stateful<Div> {
    div()
        .id(("nav-group-icon", group.get() as usize))
        .debug_selector(move || format!("nav-group-icon-{}", group.get()))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(GROUP_MARK_LG))
        .h_full()
        .child(icon(icons::GROUP, GROUP_MARK_LG, TEXT_MUTED))
        .tooltip(|window, cx| Tooltip::new("Group").build(window, cx))
}

/// The four-Pane Group mark. Project order flattens Groups into their
/// Projects, so this keeps durable membership visible without competing
/// with the Thread's status dot or provider mark.
fn group_membership_indicator(thread: ThreadId) -> Stateful<Div> {
    div()
        .id(("nav-group-membership", thread.get() as usize))
        .debug_selector(move || format!("nav-group-membership-{}", thread.get()))
        .flex()
        .flex_shrink_0()
        .items_center()
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
        .pl(px(ROW_ICON_GAP))
        .gap(px(ROW_ICON_GAP))
        .child(subagent_tail(thread, subagents))
        .children(separated.then(|| meta_text().flex_shrink_0().child("·")))
        .child(since_tail(thread, since))
}

/// The number of subagents attached to a Thread. Its branching mark keeps
/// the compact count distinct from recency without spelling out a noun.
/// Threads without children spend no space here.
fn subagent_tail(thread: ThreadId, count: usize) -> Stateful<Div> {
    let cell = meta_text()
        .id(("nav-subagents", thread.get() as usize))
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(3.))
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
    let cell = meta_text()
        .flex_shrink_0()
        .debug_selector(move || format!("nav-since-{}", thread.get()));
    let Some(label) = label else {
        return cell;
    };
    cell.child(label)
}

fn meta_text() -> Div {
    div()
        .text_size(px(FS_SM))
        .line_height(relative(LINE_TIGHT))
        .text_color(rgb(TEXT_MUTED))
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

/// What an empty tree says. It names the Project rather than shrugging,
/// so the way out is obvious — and when the Parked section below holds
/// Threads the filter admits, it says *open*, so the operator is not told
/// a Project is empty while its Threads sit one fold away.
pub fn empty_filter(project: &str, parked_below: bool) -> Div {
    let message = if parked_below {
        format!("No open Groups or Threads in {project}.")
    } else {
        format!("No Groups or Threads in {project}.")
    };
    div()
        .my(px(RAIL_ITEMS_TOP))
        .mx(px(ROW_PAD_X))
        .text_size(px(FS_MD))
        .text_color(rgb(TEXT_MUTED))
        .child(SharedString::from(message))
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
        .max_h(relative(PARKED_MAX_SHARE))
        .px(px(NAV_TREE_PAD))
        .pb(px(NAV_TREE_PAD))
}

/// The Parked section's header: a chevron saying which way it is folded,
/// the word, and how many wait. A heading in the Project headings' voice,
/// but a control — the press toggles the fold, and a right press offers
/// the section's own menu. The cockpit wires both.
pub fn parked_header(count: usize, open: bool) -> Stateful<Div> {
    let chevron = if open {
        icons::CHEVRON_DOWN
    } else {
        icons::CHEVRON_RIGHT
    };
    div()
        .id(("nav-parked", 0usize))
        .debug_selector(|| "nav-parked".into())
        .group(PARKED_GROUP)
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(SECTION_H))
        .px(px(ROW_PAD_X))
        .gap(px(ROW_ICON_GAP))
        .rounded(px(R_CONTROL))
        .text_size(px(FS_SM))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(TEXT_2))
        .hover_row()
        .press_row()
        .child(
            icon(chevron, ROW_ICON, TEXT_MUTED)
                .group_hover(PARKED_GROUP, |style| style.text_color(rgb(TEXT))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .group_hover(PARKED_GROUP, |style| style.text_color(rgb(TEXT_STRONG)))
                .child("Parked"),
        )
        .child(
            div()
                .font_weight(FontWeight::NORMAL)
                .text_color(rgb(TEXT_MUTED))
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
        .py(px(RAIL_PAD_Y))
}

/// A compact cluster for the rail's primary actions.
pub fn rail_actions() -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .items_center()
        .gap(px(MEMBER_GAP))
}

/// Utilities stay pinned to the bottom rather than competing with Threads.
pub fn rail_utilities() -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .items_center()
        .gap(px(MEMBER_GAP))
        .pt(px(RAIL_PAD_Y))
}

/// The rail's filter button: the compact column's Project affordance.
/// Its glyph brightens to `--text` when a Project filter is active — the
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

/// One rail item: a Thread reduced to a two-letter monogram plus its live
/// status dot. Provider logos made every Codex or Claude Thread identical;
/// this keeps the rail scannable while the tooltip preserves the full name.
/// `current` is the Group's, not the Thread's—the expanded tree uses the
/// same selection ownership.
pub fn rail_item(row: &ThreadRow, current: bool) -> Button {
    let title = row.name.clone();
    let monogram = rail_monogram(&row.name);
    components::button(("nav-rail-item", row.thread.get() as usize))
        .debug_selector(move || format!("nav-rail-item-{}", row.thread.get()))
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
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
                .w(px(ICON_BUTTON))
                .h(px(ICON_BUTTON))
                .text_size(px(FS_SM))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_2))
                .child(monogram)
                .child(
                    div()
                        .absolute()
                        .right(px(3.0))
                        .bottom(px(3.0))
                        .child(rail_status_dot(row.status)),
                ),
        )
}

/// The expanded row's breathing halo needs more room than a 28px avatar.
/// Rail state stays still so it cannot clip into duplicate marks.
fn rail_status_dot(status: RowStatus) -> Div {
    let dot = div()
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
                .flex_1()
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
            provider,
            current,
            last_used: Some("2h".into()),
            subagents: 2,
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

    /// Every expanded row is a drag source before it is a button, so it
    /// wears the open hand whether or not it is current.
    #[test]
    fn every_draggable_row_advertises_its_grab() {
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
            provider: None,
            current: false,
            last_used: None,
            subagents: 0,
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

    /// The working halo is absolute inside a fixed dot box, so a Thread
    /// that starts inferring does not shift its own row — or any row under
    /// it — by a pixel.
    #[test]
    fn a_working_row_is_the_same_box_as_a_quiet_one() {
        let working = ThreadRow {
            status: RowStatus::Working,
            ..thread(Some(Provider::Claude))
        };
        let height = |mut drawn: Stateful<Div>| drawn.style().size.height;
        assert_eq!(
            height(thread_row(&working)),
            height(thread_row(&thread(Some(Provider::Claude))))
        );
        assert_eq!(height(thread_row(&working)), Some(px(THREAD_ROW_H).into()));
    }

    #[test]
    fn subagent_count_is_hidden_at_zero_and_uses_compact_numeric_copy() {
        assert_eq!(subagent_label(0), None);
        assert_eq!(subagent_label(1).as_deref(), Some("1"));
        assert_eq!(subagent_label(3).as_deref(), Some("3"));
    }

    /// The Parked header is a control in a heading's clothes: it is
    /// pressed like a row, and it is the same 30px box the Project
    /// headings are, folded or not — unfolding must not move the tree's
    /// bottom edge by a pixel.
    #[test]
    fn the_parked_header_is_a_row_sized_heading() {
        let mut shut = parked_header(3, false);
        assert_eq!(shut.style().mouse_cursor, Some(CursorStyle::PointingHand));
        assert_eq!(shut.style().size.height, Some(px(SECTION_H).into()));
        let mut open = parked_header(3, true);
        assert_eq!(open.style().size.height, shut.style().size.height);
    }

    /// The section is capped at half the column and its list scrolls,
    /// so a long parked history can never push the running tree out.
    #[test]
    fn the_parked_section_is_capped_and_its_list_scrolls() {
        let mut section = parked_section();
        assert_eq!(
            section.style().max_size.height,
            Some(relative(PARKED_MAX_SHARE).into())
        );
        let scroll = ScrollHandle::new();
        let mut list = parked_list(&scroll);
        assert_eq!(
            list.style().overflow.y,
            Some(gpui::Overflow::Scroll),
            "the list scrolls on its own handle"
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

    #[cfg(target_os = "macos")]
    #[test]
    fn the_mac_rail_owns_the_full_traffic_light_reserve() {
        assert_eq!(RAIL_WIDTH, TRAFFIC_RESERVE);
    }

    #[test]
    fn rail_monograms_identify_threads_instead_of_repeating_provider_marks() {
        assert_eq!(rail_monogram("fix sidebar"), "FI");
        assert_eq!(rail_monogram("éclair polish"), "ÉC");
        assert_eq!(rail_monogram("---"), "?");
    }
}
