//! The left navigation column (SPEC §5.1, `mockups/solo.png`): the head
//! (the traffic lights' spacer, the Activity bell, search and the sidebar
//! toggle, as Codex's header), the views (`New thread`, `Parked`), the
//! tree — `Pinned`, then one section per Project, its loose Threads (the
//! first `SHOWN_ROWS`, then `Show more`) and its Groups, each Group's
//! members hanging under it, then `Recents`, the Threads in no Project —
//! and the foot (the view switcher, the account row and the gear). A view
//! other than the tree shows its list in the tree's place; Esc or a click
//! on the active view returns to it. The Activity view (the bell, Codex's)
//! is one: what runs or needs the operator under `Priority`, the rest
//! touched today under `Today`, each row its title, Project and the last
//! message's opening lines. The Inbox (⌘I) is another, reached from the
//! keyboard and the palette.
//! It is a view, never the only door: everything a row does (focus,
//! revive, regroup) stays reachable from the keyboard and the ⌘K palette.
//!
//! Drawing only, like `pane.rs`: the cockpit assembles a `NavState` per
//! frame from O(1) reads plus its project/branch/parked caches —
//! `Store::load` and `Instruments::of` are banned here, which is what keeps
//! the 24-Pane wall smooth with the nav open. Click wiring stays in
//! `cockpit.rs`, the same split `pane_cell` uses.
//!
//! **A list on the sidebar.** The column is `paint::SIDEBAR` (a step over
//! the plane) from the window's top. Rows are rounded, wear
//! `paint::SIDEBAR_HOVER` under the pointer (the one 100ms hover blend) and
//! `paint::SIDEBAR_SELECTED` (no ring) when they stand for what the
//! board shows: the Solo Thread's row, or the shown Group's. A Thread row
//! is one quiet line, as Claude's list: its title, and at its right one
//! mark when a state is worth saying (`state_mark`). Under the pointer a
//! Thread row offers its menu at `⋯`, and a Project heading a new Thread
//! in it (`row_action`), standing in for that mark: the hover doors Codex
//! and Claude give their lists. Section headings are small and quiet, a
//! caret after the label; the foot is the account row.
//!
//! **One left edge** (§5.1.4, Claude's). A Project heads its rows as a
//! quiet label with no icon, and its rows' titles start on that label, as
//! every heading's do; a Group's members hang one `NAV_TREE_INDENT` in,
//! on its title past its chevron (`RowPlace`). No guides: the indent says
//! the nesting. Only content moves in; the fills keep the width. The
//! Parked view groups its rows the same way (`ViewSection`); the Inbox and
//! the Activity view stay flat.
//!
//! Colour is state (ADR 0033): the mark, a folded Project's roll-up word,
//! the needs-you pill. One thing moves: a working
//! Thread's spinner.
//!
//! **Folding** (cmd-B) rides the column down to the 52px **rail** while its
//! content fades on its own clock: one cell per open Thread in the tree's
//! order, its mark and, where ⌘1–9 reaches it, that digit, so a folded nav
//! still says what needs the operator and how to get there.
//!
//! Every colour and metric is a `theme` token; this file holds no literal
//! of its own.

use ferrite_core::groups::GroupId;
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::ThreadId;

use gpui::prelude::*;
use gpui::{
    div, px, rgb, rgba, AnyElement, App, Div, FontWeight, ScrollHandle, SharedString, Stateful,
};

use crate::machines::{Connection, MachineRow};
use crate::notifications::{Row as NoticeRow, State as NoticeState};
use ferrite_core::cockpit::StatusRow;
pub use ferrite_pane::mark::{mark, NavTail, RowStatus};
use ferrite_ui::components;
use ferrite_ui::icons::{self, icon};
use ferrite_ui::motion::{self, MotionSpec};
use ferrite_ui::theme::*;

/// The nav's two widths — 264 open, and the 52px rail folded (cmd-B).
/// `CockpitView::cell()` subtracts whichever is live, so the nav stays part
/// of the semantic-zoom input rather than a special case.
pub use ferrite_ui::theme::{NAV_FOLDED_W as FOLDED_WIDTH, NAV_WIDTH as WIDTH};

gpui::actions!(
    nav,
    [
        CloseView,
        Focus,
        SelectNext,
        SelectPrevious,
        Open,
        Fold,
        Unfold
    ]
);

/// A row the nav's keyboard can stand on (`cockpit/nav_keys.rs`): a
/// heading that folds, a tree Thread row (under its Group, if a member) or
/// a Group row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NavItem {
    Heading(NavFold),
    Thread(ThreadId, Option<GroupId>),
    Group(GroupId),
}

/// The keyboard's place in the nav: its row, and the blend key that row's
/// hover face reads, so the cursor wears the pointer's face.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NavCursor {
    pub item: NavItem,
    pub key: SharedString,
}

impl NavCursor {
    pub fn heading(fold: NavFold, key: impl Into<SharedString>) -> Self {
        NavCursor {
            item: NavItem::Heading(fold),
            key: key.into(),
        }
    }

    /// The keys mirror the rows' own (`thread_row_with_title`,
    /// `group_row_with_title`).
    pub fn thread(thread: ThreadId, group: Option<GroupId>) -> Self {
        NavCursor {
            item: NavItem::Thread(thread, group),
            key: format!(
                "nav-thread-{}-{}",
                group.map_or(0, GroupId::get),
                thread.get()
            )
            .into(),
        }
    }

    pub fn group(group: GroupId) -> Self {
        NavCursor {
            item: NavItem::Group(group),
            key: format!("nav-group-{}", group.get()).into(),
        }
    }
}

/// The column's inner width: its rows reach this far.
const INNER_W: f32 = WIDTH - 2.0 * NAV_COL_PAD;

const COLLAPSE_KEY: &str = "nav-collapse";

/// The column's content fading as it folds or opens, on its own clock
/// apart from the width's ride.
pub(crate) const CONTENT_FADE: MotionSpec = MotionSpec::new(MOTION_NAV_FADE_MS, motion::EASE);

/// What a fold in the nav holds shut (N-4): a Project section shut to its
/// heading, the Pinned or Groups section, or a Group shut to its row (its
/// count kept). A Project's fold is saved with the settings
/// (`Settings::folded_projects`); the rest are held in memory by the
/// cockpit (`CockpitView::set_nav_fold`), so a fold survives every
/// re-render and view switch; opening a folded Group never unfolds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum NavFold {
    Project(ProjectId),
    /// The Pinned section.
    Pinned,
    /// The Groups section: the Groups that span Projects.
    Groups,
    Group(GroupId),
}

/// How many loose rows a section draws before `Show more` (Codex's five).
pub const SHOWN_ROWS: usize = 5;

/// What the nav shows under its views (§5.1.2): the tree, or one view's
/// list in its place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavView {
    #[default]
    Tree,
    /// The Inbox: the blocked Threads oldest-first, then the Notices
    /// under `Recent`. (`notifications::NeedsYou` holds its cursor.)
    Inbox,
    /// The parked Threads no Group claims.
    Parked,
    /// The bell's (Codex's activity list): `Priority`, then `Today`.
    Activity,
}

/// What the nav draws this frame: the Project sections and the views'
/// lists. Nothing here is a store read — the cockpit assembles it from
/// O(1) reads plus its project/branch caches.
pub struct NavState {
    /// The `Pinned` section's rows (`Settings::pinned_threads`), in pin
    /// order: open Threads no Group claims. A pinned row is listed here
    /// and nowhere else in the tree.
    pub pinned: Vec<ThreadRow>,
    /// The `Pinned` heading folded its rows away.
    pub pinned_folded: bool,
    /// One section per Project with a row to show, in creation order, then
    /// `Other` for Threads whose Project cannot be read.
    pub sections: Vec<ProjectSection>,
    /// Every Group block the sections draw, in the order they draw them;
    /// a section names its own by index.
    pub groups: Vec<GroupBlock>,
    /// Every loose Thread row — open, in no home Group — in the order the
    /// sections draw them; a section names its own by index.
    pub solos: Vec<ThreadRow>,
    /// The Parked view's rows: parked Threads no Group claims as home,
    /// in the park order. A parked Group member stays under its Group — the
    /// Group is its place, and opening the Group revives it there.
    pub parked: Vec<ThreadRow>,
}

impl NavState {
    /// Every row the tree draws, in the order it draws them: the pinned
    /// rows, then each section's loose rows `Show more` leaves out, then
    /// the members of each of its unfolded Groups. A folded section or
    /// Group draws none. The views' lists are not the tree; tests read the
    /// tree's order from here.
    #[cfg(test)]
    pub fn ordered_rows(&self) -> Vec<&ThreadRow> {
        let mut rows = Vec::new();
        if !self.pinned_folded {
            rows.extend(self.pinned.iter());
        }
        for section in self.sections.iter().filter(|section| !section.folded) {
            rows.extend(section.drawn.iter().map(|index| &self.solos[*index]));
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

/// One Project's run of the tree: its heading, its loose rows, then its
/// Groups — each Group listed under the one Project its members share. A
/// Group spanning Projects belongs to none of them: it is listed in the
/// Groups section (`mixed`), above `Projects`.
pub struct ProjectSection {
    pub project: Option<ProjectId>,
    /// The Groups section: no Project, no loose rows, only Groups whose
    /// members span Projects.
    pub mixed: bool,
    /// Lowercase.
    pub label: SharedString,
    /// The branch every one of its own rows is on, when they share one,
    /// and whether it is the Project's default.
    pub branch: Option<(SharedString, bool)>,
    /// The heading folded it: one row, and nothing under it.
    pub folded: bool,
    /// Its own rows' roll-up word (`RollUp::of`), which a folded heading
    /// says.
    pub roll_up: Option<RollUp>,
    /// How many rows it holds, which a quiet folded heading says.
    pub count: usize,
    /// Indices into `NavState::solos`.
    pub solos: Vec<usize>,
    /// The loose rows it draws (indices into `NavState::solos`): all of
    /// them, or while `Show more` holds the rest back, the first
    /// `SHOWN_ROWS` and the selected row wherever it is.
    pub drawn: Vec<usize>,
    /// The line under its loose rows, when it has more than `SHOWN_ROWS`.
    pub more: Option<ShowMore>,
    /// Indices into `NavState::groups`.
    pub groups: Vec<usize>,
}

/// The line under a long section's loose rows (Codex's): `Show more`
/// while the rows past `SHOWN_ROWS` are held back, `Show less` once shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShowMore {
    More,
    Less,
}

impl ShowMore {
    pub fn label(self) -> &'static str {
        match self {
            ShowMore::More => "Show more",
            ShowMore::Less => "Show less",
        }
    }
}

impl ProjectSection {
    /// `Recents` (Codex's): the section of the Threads in no Project,
    /// headed by its word rather than a folder, newest first.
    pub fn recents(&self) -> bool {
        self.project.is_none() && !self.mixed
    }

    /// What a folded heading says at its right, the first that has
    /// something to say: its rows' roll-up word, its branch when that is
    /// not the default, its row count.
    pub fn folded_summary(&self) -> (SharedString, u32) {
        if let Some(word) = &self.roll_up {
            return (word.text.clone(), word.ink);
        }
        let text = self
            .branch
            .as_ref()
            .filter(|(_, default)| !default)
            .map(|(branch, _)| short_branch(branch))
            .unwrap_or_else(|| self.count.to_string().into());
        (text, INK_SUBTLE)
    }
}

/// A roll-up's word (§5.1.6): how many rows need the operator, in the
/// worst such state's ink — `2 need you`, else `1 failed`.
#[derive(Clone, Debug, PartialEq)]
pub struct RollUp {
    pub text: SharedString,
    pub ink: u32,
}

impl RollUp {
    /// `None` while nothing waits on the operator or failed.
    pub fn of<'a>(rows: impl IntoIterator<Item = &'a ThreadRow>) -> Option<Self> {
        let (mut waiting, mut failed) = (0, 0);
        for row in rows {
            match row.status {
                RowStatus::NeedsYou => waiting += 1,
                RowStatus::Failed => failed += 1,
                _ => {}
            }
        }
        if waiting > 0 {
            return Some(RollUp {
                text: words::needs_you(waiting).into(),
                ink: ATTENTION,
            });
        }
        (failed > 0).then(|| RollUp {
            text: format!("{failed} {}", words::FAILED).into(),
            ink: BLOCKED,
        })
    }
}

/// The Activity view's list (the bell, Codex's), from the tree's rows:
/// `Priority` — what needs the operator, in ⌘D's order, then what is at
/// work, in creation order — and `Today`, every other open Thread written
/// to since midnight, newest first. A Thread moves between the two as its
/// state does and never leaves while it was touched today, so a finish
/// stays in view. An empty section is gone.
#[derive(Clone, Default)]
pub struct Activity {
    pub priority: Vec<ActivityRow>,
    pub today: Vec<ActivityRow>,
}

/// One Activity row: the Thread's tree row (its title, Project, mark and
/// selection) and the opening of its last message, when the facts hold
/// one (`ThreadFacts::wall`'s last prose line).
#[derive(Clone)]
pub struct ActivityRow {
    pub row: ThreadRow,
    pub preview: Option<SharedString>,
}

/// One Project's run of a view's list (§5.1.2), drawn as the tree draws
/// it: a plain heading, the rows no Group claims, then each home Group its
/// rows share, its members under it. The Parked view lists its rows so;
/// the Inbox keeps its queue's order, flat.
#[derive(Clone)]
pub struct ViewSection {
    pub label: SharedString,
    pub solos: Vec<ThreadRow>,
    /// Never the board's: a view's Group is where its rows live. It folds
    /// as the tree's does.
    pub groups: Vec<GroupBlock>,
}

/// Where a view's row hangs (`ViewSection::tree`): its Project, and the
/// home Group that claims it, if one does.
pub struct ViewPlace {
    pub project: Option<ProjectId>,
    pub group: Option<HomeGroup>,
}

/// A row's home Group, as a view lists it: its place in the durable order,
/// its id, its title, and whether the tree holds it folded.
pub struct HomeGroup {
    pub order: usize,
    pub id: GroupId,
    pub title: SharedString,
    pub folded: bool,
}

impl ViewSection {
    /// A view's `rows` by Project, in creation order with `Other` (no
    /// Project) last, then within each by home Group, in the durable
    /// order. Rows keep the list's own order within their run. `label`
    /// names a Project's section from its rows.
    pub fn tree(
        rows: Vec<ThreadRow>,
        place: impl Fn(&ThreadRow) -> ViewPlace,
        label: impl Fn(Option<ProjectId>, &[&ThreadRow]) -> SharedString,
    ) -> Vec<Self> {
        let mut placed: Vec<(ViewPlace, ThreadRow)> =
            rows.into_iter().map(|row| (place(&row), row)).collect();
        placed.sort_by_key(|(place, _)| (place.project.is_none(), place.project));
        // Runs of one Project each: sorted, so a Project's rows are adjacent.
        let mut runs: Vec<Vec<(ViewPlace, ThreadRow)>> = Vec::new();
        for (place, row) in placed {
            match runs.last_mut() {
                Some(run) if run[0].0.project == place.project => run.push((place, row)),
                _ => runs.push(vec![(place, row)]),
            }
        }
        runs.into_iter()
            .map(|run| {
                let project = run[0].0.project;
                let all: Vec<&ThreadRow> = run.iter().map(|(_, row)| row).collect();
                let label = label(project, &all);
                let mut solos = Vec::new();
                let mut groups: Vec<(usize, GroupBlock)> = Vec::new();
                for (place, row) in run {
                    let Some(home) = place.group else {
                        solos.push(row);
                        continue;
                    };
                    match groups.iter_mut().find(|(_, group)| group.id == home.id) {
                        Some((_, group)) => group.members.push(row),
                        None => groups.push((
                            home.order,
                            GroupBlock {
                                id: home.id,
                                title: home.title,
                                members: vec![row],
                                folded: home.folded,
                                selected: false,
                            },
                        )),
                    }
                }
                groups.sort_by_key(|(order, _)| *order);
                ViewSection {
                    label,
                    solos,
                    groups: groups.into_iter().map(|(_, group)| group).collect(),
                }
            })
            .collect()
    }
}

/// One Group and the Threads hanging under it.
#[derive(Clone)]
pub struct GroupBlock {
    pub id: GroupId,
    pub title: SharedString,
    pub members: Vec<ThreadRow>,
    /// Folded: its chevron right, the members hidden, the count kept.
    pub folded: bool,
    /// The board shows this Group: the row takes the selected style and
    /// its members are one-line rows.
    pub selected: bool,
}

/// One Thread's row — identical whether it is a loose Thread, a Group
/// member or a parked one; only the container differs.
#[derive(Clone)]
pub struct ThreadRow {
    pub thread: ThreadId,
    pub name: SharedString,
    /// What the Thread is doing right now.
    pub status: RowStatus,
    /// The Project's name, from the facts cache.
    pub project: Option<SharedString>,
    /// The board shows this Thread (Solo), or it is the shown Group's
    /// focused member: the row carries the selection.
    pub selected: bool,
    /// Its state's word (C10): what line 2 says for a state with a reason.
    pub tail: NavTail,
}

/// What a set of rows rolls up to: the most urgent of them. `None` for
/// no rows.
pub fn roll_up<'a>(rows: impl IntoIterator<Item = &'a ThreadRow>) -> Option<RowStatus> {
    rows.into_iter().map(|row| row.status).max()
}

/// Where a Thread row hangs in the tree (§5.1.4): `Row(depth)` is its row
/// `depth` Groups in — 0 under a heading (a Project's, `Pinned`), 1 under
/// a Group — and `Member(depth)` a member of the shown Group whose own row
/// is `depth` in. A row's title starts on its heading's label
/// (`NAV_ROW_PAD_X`), each Group level `NAV_TREE_INDENT` further. `Inbox`
/// is a row among the Inbox's Notices, its title on theirs past their mark
/// column (`NAV_TITLE_X`). Only the content moves in: the row's fills keep
/// the column's width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowPlace {
    Row(usize),
    Member(usize),
    Inbox,
}

impl RowPlace {
    /// A row under a heading, on its label.
    pub const ROOT: RowPlace = RowPlace::Row(0);

    /// Where its title starts.
    fn title_x(self) -> f32 {
        match self {
            RowPlace::Row(depth) => indent(depth) + NAV_ROW_PAD_X,
            // A member hangs where a row under its Group would.
            RowPlace::Member(depth) => indent(depth + 1) + NAV_ROW_PAD_X,
            RowPlace::Inbox => NAV_TITLE_X,
        }
    }

    /// The width its lines have, from the title to the row's right padding.
    fn text_w(self) -> f32 {
        INNER_W - self.title_x() - NAV_ROW_PAD_X
    }
}

/// How far `depth` levels of the tree move a row's content in.
fn indent(depth: usize) -> f32 {
    depth as f32 * NAV_TREE_INDENT
}

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

/// A line pinned to `width`, truncating with an ellipsis.
///
/// A truncating line needs a **definite** width on its very first measure.
/// gpui caches a nowrap line's first measure permanently, and taffy only
/// hands a text leaf a definite width when the leaf's flex container is a
/// **column** whose own available width is definite — which taffy derives
/// from the child's own min/max width. Hence: flex **column**, with min and
/// max width pinned to the box.
///
/// The truncator gets exactly the box. The vendored text element keeps a
/// line whole when its shaped width fits (a layout unit of slack, so an
/// exact fit keeps its last glyph) and otherwise cuts it to fit with the
/// ellipsis, so nothing reaches past the box to be clipped: a budget
/// wider than the box would lay a slightly-too-long title out whole and
/// shave its last glyph with no ellipsis (`Supplier Price Reviev`).
fn fitted(width: f32, size: f32, ink: u32, weight: FontWeight, text: impl IntoElement) -> Div {
    div()
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
                    .truncate()
                    .h(px(NAV_LINE))
                    .text_size(px(size))
                    .font_weight(weight)
                    .line_height(px(NAV_LINE))
                    .text_color(rgb(ink))
                    .child(text),
            ),
        )
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

/// The nav's one pointer role: nothing at rest, `paint::SIDEBAR_HOVER`
/// under the pointer reaching full in 100ms, and the selected row
/// `paint::SIDEBAR_SELECTED` whatever the pointer does: no ring, the fill
/// alone (Claude's, +32 over the column to the hover's +8). The pointer
/// stays the arrow.
fn wash<E: Styled + InteractiveElement>(mut element: E, key: SharedString, selected: bool) -> E {
    if selected {
        element = element.bg(paint::SIDEBAR_SELECTED);
    } else {
        let t = motion::hover_t(&key);
        #[cfg(test)]
        testing::DRAWN_HOVER.with(|drawn| drawn.borrow_mut().insert(key.clone(), t));
        if t > 0.0 {
            element = element.bg(motion::mix(
                rgba(TRANSPARENT).into(),
                paint::SIDEBAR_HOVER.into(),
                t,
            ));
        }
    }
    element
        .interactivity()
        .on_hover(motion::hover_listener(key));
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

/// The nav column itself: full height, `width` wide (the caller rides it
/// on cmd-B), clipped, so its content keeps `WIDTH` (`content`) and nothing
/// reflows while it moves. It paints no ground of its own: `body` does.
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
        .text_size(px(FS_NAV))
        .line_height(px(NAV_LINE))
        .text_color(rgb(INK_STRONG))
}

/// The column's ground: `paint::SIDEBAR`, top to bottom — what the host's
/// traffic lights sit on.
pub fn body() -> Div {
    div()
        .debug_selector(|| "nav-body".into())
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .right_0()
        .bg(paint::SIDEBAR)
}

/// The column's content at its own width: `NAV_COL_PAD` in at its sides
/// and foot, its head in the window's head row.
pub fn content() -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(WIDTH))
        .px(px(NAV_COL_PAD))
        .pb(px(NAV_COL_PAD))
}

/// The head (§5.1.1), in the head row: `drag` (the window-drag stretch the
/// traffic lights and the cockpit's history arrows sit over), then the Activity bell (`activity_button`),
/// search (`search_button`) and the sidebar toggle, as Codex's header
/// holds its bell and search.
pub fn head(
    drag: impl IntoElement,
    bell: impl IntoElement,
    search: impl IntoElement,
    toggle: impl IntoElement,
) -> Div {
    div()
        .debug_selector(|| "nav-head".into())
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .h(px(WIN_CHROME_H))
        .pl(px(NAV_HEAD_PAD_L))
        .pr(px(NAV_HEAD_PAD_R))
        .child(
            div()
                .debug_selector(|| "nav-head-drag".into())
                .flex()
                .flex_1()
                .h_full()
                .child(drag),
        )
        .child(bell)
        .child(search)
        .child(toggle)
}

/// An icon button of the column (the toggle, the gear, the heading's `+`):
/// `size` square, its glyph `INK_STRONG` (Claude's header icons), under the
/// pointer `paint::RAISED` over 100ms.
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
                paint::RAISED.into(),
                t,
            ))
        })
        .on_hover(motion::hover_listener(blend))
        .child(icon(glyph, glyph_size, INK_STRONG))
}

/// The sidebar toggle (§5.1.1): the sidebar glyph in a 28px door, its
/// tooltip `Toggle sidebar` and the chord the keymap binds. `rail` is the
/// rail's own, at its top.
pub fn collapse_button(rail: bool) -> Stateful<Div> {
    icon_button(
        if rail {
            "nav-rail-collapse"
        } else {
            COLLAPSE_KEY
        },
        icons::SIDEBAR,
        NAV_TOGGLE_GLYPH,
        ICON_BUTTON,
        R_CONTROL,
    )
    .tooltip(ferrite_ui::menu::action_tooltip(
        "Toggle sidebar",
        "cockpit::ToggleNav",
    ))
}

/// The Activity view's door in the head: the bell, filled while the view
/// shows, its tooltip the view's name. The cockpit wires the press.
pub fn activity_button(on: bool) -> Stateful<Div> {
    let glyph = if on { icons::BELL_FILL } else { icons::BELL };
    icon_button(
        "nav-activity",
        glyph,
        NAV_TOGGLE_GLYPH,
        ICON_BUTTON,
        R_CONTROL,
    )
    .tooltip(ferrite_ui::menu::tooltip("Activity"))
}

/// Search's door in the head (Codex's): the magnifier, its tooltip `Search`
/// and the palette's chord. The cockpit wires the press to the palette.
pub fn search_button() -> Stateful<Div> {
    icon_button(
        "nav-search",
        icons::SEARCH,
        NAV_TOGGLE_GLYPH,
        ICON_BUTTON,
        R_CONTROL,
    )
    .tooltip(ferrite_ui::menu::action_tooltip(
        "Search",
        "palette::Toggle",
    ))
}

// ------------------------------------------------------------- the views

/// What a view row says at its right (§5.1.2).
pub enum Trailing {
    None,
    /// A chord, as the keymap binds it (`⌘K`), shown under the pointer.
    Chord(String),
    /// A dim count, hidden at 0.
    Count(usize),
}

/// The views under the head, 14px clear of the tree.
pub fn views(rows: Vec<AnyElement>) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .mb(px(NAV_VIEWS_GAP))
        .children(rows)
}

/// One view row: its 16px icon in `INK_STRONG`, its label in primary ink
/// (`INK_STRONG`, as the reference app's rows) and its trailing item;
/// under the pointer `paint::RAISED`, the active view `SELECTED`.
pub fn view_row(
    key: &'static str,
    glyph: AnyElement,
    label: &'static str,
    tail: Trailing,
    active: bool,
) -> Stateful<Div> {
    let blend = SharedString::from(key);
    let end: Option<AnyElement> = match tail {
        Trailing::None => None,
        // A chord is an action's hint, not content: under the pointer only.
        Trailing::Chord(_) if !hovered(&blend) => None,
        Trailing::Chord(keys) => Some(
            trailing(chord_glyphs(&keys), FS_META, INK_SUBTLE)
                .debug_selector(move || format!("{key}-chord"))
                .into_any_element(),
        ),
        Trailing::Count(0) => None,
        Trailing::Count(count) => Some(
            components::tabular(trailing(count.to_string(), FS_META, INK_SUBTLE))
                .debug_selector(move || format!("{key}-count"))
                .into_any_element(),
        ),
    };
    line_frame(key, blend, active)
        .debug_selector(move || key.into())
        .gap(px(NAV_ICON_GAP))
        .text_color(rgb(INK_STRONG))
        .child(div().flex().flex_shrink_0().size(px(NAV_ICON)).child(glyph))
        .child(
            div()
                .debug_selector(move || format!("{key}-label"))
                .flex_1()
                .min_w_0()
                .truncate()
                .child(label),
        )
        .children(end)
}

/// A view row's icon, in `INK_STRONG` beside its primary label.
pub fn view_icon(path: &'static str) -> AnyElement {
    icon(path, NAV_ICON, INK_STRONG).into_any_element()
}

/// A chord as it is drawn, its key a capital: `cmd-k` → `⌘K`.
fn chord_glyphs(keys: &str) -> String {
    components::chord_text(keys).to_uppercase()
}

// ------------------------------------------------------------- headings

/// A section heading (§5.1.3): `Projects`, `Groups`, `Recent`, small and
/// quiet — 12/400 `INK_SUBTLE` — with its button (`heading_button`) at the
/// right.
pub fn heading(key: &'static str, label: &'static str) -> Div {
    heading_frame(key)
        .text_color(rgb(INK_SUBTLE))
        .child(div().flex_1().child(label))
}

/// A heading with an action at its right (`Projects`' `+`) that stands only
/// while the pointer is over the heading, as Claude's add buttons do.
pub fn heading_with_action(
    key: &'static str,
    label: &'static str,
    action: impl FnOnce() -> AnyElement,
) -> Stateful<Div> {
    let blend = SharedString::from(key);
    let shown = hovered(&blend);
    heading_frame(key)
        .id(key)
        .text_color(rgb(INK_SUBTLE))
        .on_hover(motion::hover_listener(blend))
        .child(div().flex_1().child(label))
        .when(shown, |heading| heading.child(action()))
}

/// A heading's line, its label's slot left to the caller.
fn heading_frame(key: &'static str) -> Div {
    div()
        .debug_selector(move || key.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(NAV_HEADING_H))
        .pl(px(NAV_ROW_PAD_X))
        .pr(px(NAV_HEADING_PAD_R))
        .text_size(px(FS_META))
        .font_weight(W_BODY)
}

/// A heading whose press folds what it heads (`Groups`): `heading`'s line,
/// its label blending to `INK_STRONG` under the pointer, and right after the
/// label its caret (`components::fold_chevron`
/// at `NAV_HEADING_CARET`): `▸` shut, `▾` open, turning as the fold flips.
/// The cockpit wires the press.
pub fn fold_heading(key: &'static str, label: &'static str, folded: bool) -> Stateful<Div> {
    let blend = SharedString::from(key);
    let t = motion::hover_t(&blend);
    heading_frame(key)
        .id(key)
        .text_color(motion::mix(
            rgb(INK_SUBTLE).into(),
            rgb(INK_STRONG).into(),
            t,
        ))
        .on_hover(motion::hover_listener(blend))
        .child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .gap(px(SPACE_1))
                .child(label)
                .child(
                    div()
                        .debug_selector(move || format!("{key}-chevron"))
                        .flex()
                        .child(components::fold_chevron(
                            !folded,
                            NAV_HEADING_CARET,
                            INK_SUBTLE,
                        )),
                ),
        )
}

/// A heading's 22px icon button (`Projects`' `+`).
pub fn heading_button(key: &'static str, glyph: &'static str) -> Stateful<Div> {
    icon_button(key, glyph, NAV_HEADING_GLYPH, NAV_HEADING_BUTTON, R_CONTROL)
}

/// A row's hover action (§5.1.5): a line-high icon button that stands at
/// the right of the row while the pointer is over it, in place of what the
/// row says there (a Thread row's `⋯`, a Project heading's new Thread). The cockpit wires its press, which must stop at
/// the button: the row under it opens, folds or drags.
pub fn row_action(key: impl Into<SharedString>, glyph: &'static str) -> Stateful<Div> {
    // Its glyph, not its box, meets the row's right padding, where the
    // counts and words it stands in for end.
    icon_button(key, glyph, NAV_HEADING_GLYPH, NAV_LINE, R_CONTROL)
        .mr(px(-(NAV_LINE - NAV_HEADING_GLYPH) / 2.0))
}

/// Is the row blended under the pointer, so its hover action stands?
fn hovered(key: &SharedString) -> bool {
    motion::hover_t(key) > 0.0
}

/// A Project heading (§5.1.4, Claude's): the label alone, no icon, in the
/// headings' quiet face (13/400 `INK_SUBTLE`) with its fold's caret right
/// after it (`▾`, `▸` folded), and nothing at its right: Claude's carry
/// no ahead/behind counts. Folded, at its right what `ProjectSection::folded_summary` says. Under the pointer its
/// `action` (a new Thread in the Project, `row_action`) stands at its right
/// instead. The cockpit wires its press to the fold.
pub fn project_section(
    index: usize,
    section: &ProjectSection,
    action: Option<AnyElement>,
    measure: &Measure,
) -> Stateful<Div> {
    let right = section.folded.then(|| section.folded_summary());
    project_row(
        "nav-project-section",
        index,
        section.label.clone(),
        right,
        action,
        Some(section.folded),
        measure,
    )
}

/// A Project's heading in a view's list (§5.1.2): the tree's heading,
/// open, with nothing at its right and no fold.
pub fn view_project(index: usize, label: SharedString, measure: &Measure) -> Stateful<Div> {
    project_row("nav-view-project", index, label, None, None, None, measure)
}

/// A Project heading's row, whichever list it heads (`key`, with `index`
/// its id and selector). `fold` is `Some(folded)` where the heading folds
/// its section, which its caret says.
fn project_row(
    key: &'static str,
    index: usize,
    label: SharedString,
    right: Option<(SharedString, u32)>,
    action: Option<AnyElement>,
    fold: Option<bool>,
    measure: &Measure,
) -> Stateful<Div> {
    let blend = SharedString::from(format!("{key}-{index}"));
    let action = action.filter(|_| hovered(&blend));
    let right = right.filter(|_| action.is_none());
    // The row's gap stands between the title and its right, too.
    let right_w = match (&right, &action) {
        (_, Some(_)) => NAV_LINE + NAV_ICON_GAP,
        (Some((text, _)), None) => measure.ui(text, FS_META, W_BODY) + NAV_ICON_GAP,
        (None, None) => 0.0,
    };
    let title_w = INNER_W - 2.0 * NAV_ROW_PAD_X - right_w;
    // The caret follows the label itself, so the label's box is its own
    // width, out to what the caret leaves of the title's.
    let (label_w, caret) = match fold {
        Some(folded) => (
            measure
                .ui(&label, FS_META, W_BODY)
                .min(title_w - SPACE_1 - NAV_HEADING_CARET),
            Some(
                div()
                    .debug_selector(move || format!("{key}-{index}-chevron"))
                    .flex()
                    .flex_shrink_0()
                    .child(components::fold_chevron(
                        !folded,
                        NAV_HEADING_CARET,
                        INK_SUBTLE,
                    )),
            ),
        ),
        None => (title_w, None),
    };
    line_frame((key, index), blend, false)
        .debug_selector(move || format!("{key}-{index}"))
        .gap(px(NAV_ICON_GAP))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(SPACE_1))
                .child(fitted(label_w, FS_META, INK_SUBTLE, W_BODY, label))
                .children(caret),
        )
        .children(right.map(|(right, right_ink)| {
            div()
                .flex()
                .flex_1()
                .justify_end()
                .child(trailing(right, FS_META, right_ink).pl_0())
                .debug_selector(move || format!("nav-project-summary-{index}"))
        }))
        .children(action.map(at_right))
}

/// A hover action at the right of its line, after the title's box.
fn at_right(action: AnyElement) -> Div {
    div().flex().flex_1().justify_end().child(action)
}

/// A branch held to `NAV_BRANCH_MAX_CH` characters, an ellipsis standing
/// for the rest.
fn short_branch(branch: &str) -> SharedString {
    if branch.chars().count() <= NAV_BRANCH_MAX_CH {
        return branch.to_string().into();
    }
    let kept: String = branch.chars().take(NAV_BRANCH_MAX_CH - 1).collect();
    format!("{kept}\u{2026}").into()
}

// ------------------------------------------------------------- the tree

/// The scrolling list under the views: the tree, or a view's list. It is
/// the only thing in the column that scrolls.
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

/// The tree's scrollbar (`NAV_SCROLLBAR_GUTTER`): the overlay thumb in
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

/// the tree's edge fades (`EDGE_FADE`), in `paint::SIDEBAR`: at the top once it
/// has scrolled, at the foot while rows remain below, so no row is cut
/// mid-glyph against the head or the foot. Siblings of the tree, laid before
/// the scrollbar so they never cover its thumb.
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
            .child(components::edge_fade(fill, false))
    });
    let foot = below.then(|| {
        div()
            .debug_selector(|| "nav-fade-foot".into())
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .child(components::edge_fade(fill, true))
    });
    top.into_iter().chain(foot).collect()
}

/// One section, a Project's or an Activity list's: its heading, then
/// everything under it, `NAV_SECTION_GAP` below the section above it.
pub fn section(first: bool) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .when(!first, |section| section.mt(px(NAV_SECTION_GAP)))
}

/// One Group block: the parent row, then its members. Its own id scopes
/// its rows, so a Thread listed under two Groups is two distinct rows.
pub fn group_block(id: GroupId) -> Stateful<Div> {
    div()
        .id(("nav-group-block", id.get() as usize))
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
}

/// "Insert this Group before that one": an absolute `NAV_DROP_BAND` hit
/// band over the top edge of the block's header, taking no layout and,
/// without `occlude`, stealing none of its clicks. `index` is the Group's
/// place in the durable order. The caller lays it **after** the block's
/// rows, so it is the topmost hitbox there.
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

/// "Append after the last member", by the same trick, over the last row's
/// foot. A drop between two members lands on the row it is over.
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

/// The Group row (§5.1.6), one line: its chevron (`group_fold`) at x 11,
/// the title — `INK_STRONG` 500 while the board shows the Group — and at
/// the right its roll-up: the worst state's word in its ink, then
/// `· N threads` dim (`threads_count`). Shown, it takes the selected style
/// and says only `N threads`: its members say the rest. `depth` levels into the tree, its content moves
/// in with it.
#[cfg(test)]
pub fn group_row(row: &GroupBlock, depth: usize) -> Stateful<Div> {
    group_row_with_title(row, row.title.clone(), group_fold(row), depth)
}

/// `group_row` with the title leaf and the chevron supplied by the caller
/// — the cockpit hands in a click-to-rename wrapper, or the live editor
/// while renaming, and wires the chevron's press. The row lays the chevron
/// at its own depth, on the title column of a row there, and its title
/// `NAV_TREE_INDENT` past it, where its members' titles start. The title
/// box is pinned (`fitted`), so the title truncates and renaming never
/// moves the row.
pub fn group_row_with_title(
    row: &GroupBlock,
    title: impl IntoElement,
    fold: Stateful<Div>,
    depth: usize,
) -> Stateful<Div> {
    let id = row.id;
    let key = SharedString::from(format!("nav-group-{}", id.get()));
    // What its members roll up to, as one mark (`state_mark`): the
    // Group's row is as quiet as a Thread's.
    let mark = roll_up(&row.members).and_then(state_mark);
    let right_w = if mark.is_some() { RIGHT_ROOM } else { 0.0 };
    let (ink, weight) = if row.selected {
        (INK_STRONG, W_LABEL)
    } else {
        (INK_STRONG, W_BODY)
    };
    let place = RowPlace::Row(depth);
    pitched(row_frame(
        ("nav-group", id.get() as usize),
        key,
        row.selected,
    ))
    .debug_selector(move || format!("nav-group-{}", id.get()))
    .flex_row()
    .items_center()
    .h(px(NAV_ROW_H))
    .pl(px(place.title_x() + NAV_TREE_INDENT))
    .pr(px(NAV_ROW_PAD_X))
    .child(
        div()
            .absolute()
            .top_0()
            .left(px(place.title_x()))
            .child(fold),
    )
    .child(fitted(
        place.text_w() - NAV_TREE_INDENT - right_w,
        FS_NAV,
        ink,
        weight,
        title,
    ))
    .children(mark.map(|mark| {
        at_right(
            mark.debug_selector(move || format!("nav-group-state-{}", id.get()))
                .into_any_element(),
        )
    }))
}

/// A Group row's chevron, on its row's title column:
/// the shared fold chevron (`components::fold_chevron`), turned down while
/// open (`INK_STRONG` on the shown Group, `INK_SUBTLE` otherwise). A control
/// of its own — the cockpit wires its press to the fold, which stops
/// there, so the rest of the row still opens the Group.
pub fn group_fold(row: &GroupBlock) -> Stateful<Div> {
    let id = row.id;
    let hit = NAV_ROW_H - 2.0 * SPACE_1;
    div()
        .id(("nav-group-fold", id.get() as usize))
        .debug_selector(move || format!("nav-group-fold-{}", id.get()))
        .absolute()
        .left(px((NAV_CHEVRON - hit) / 2.0))
        .top(px(SPACE_1))
        .size(px(hit))
        .flex()
        .items_center()
        .justify_center()
        .child(components::fold_chevron(
            !row.folded,
            NAV_CHEVRON,
            if row.selected { INK_STRONG } else { INK_SUBTLE },
        ))
}

/// A Project's rows (its loose Threads and its Groups) under its heading.
pub fn project_rows(rows: Vec<AnyElement>) -> Div {
    branch(rows)
}

/// The members container: the member rows under their Group.
pub fn members(rows: Vec<AnyElement>) -> Div {
    branch(rows)
}

/// One level of the tree (§5.1.4): `rows` flush under the row above them
/// — a Project's heading, a Group's row.
fn branch(rows: Vec<AnyElement>) -> Div {
    div().flex().flex_col().flex_shrink_0().children(rows)
}

/// The Thread row at root — see `thread_row_with_title`.
#[cfg(test)]
pub fn thread_row(row: &ThreadRow) -> Stateful<Div> {
    thread_row_with_title(row, row.name.clone(), None, RowPlace::ROOT, None, false)
}

/// A title's ink and weight (§5.1.5): primary ink (`INK_STRONG`), as
/// the reference app's rows; 500 on the selected row and while a finish is
/// unseen (the unread rule, said in weight now that every title is
/// primary); a parked Thread's a step down so what is running reads first.
fn title_face(row: &ThreadRow) -> (u32, FontWeight) {
    if row.selected || row.status == RowStatus::Done {
        (INK_STRONG, W_LABEL)
    } else if row.status == RowStatus::Parked {
        (INK_SUBTLE, W_BODY)
    } else {
        (INK_STRONG, W_BODY)
    }
}

/// The one Thread row builder, with the title leaf supplied by the caller
/// — see `group_row_with_title`. Every row is one quiet line (§5.1.5),
/// the title and nothing else, as Claude's and Codex's lists: where a
/// Thread works and what its PR says live in its Pane and the panel. At
/// its right one mark says what needs saying (`state_mark`: the needs-you
/// dot, a failure's, the working spinner); under the pointer its `action`
/// (`row_action`, the cockpit's `⋯`) stands there instead. `group` scopes
/// the row's blend when the Thread is listed under several Groups.
/// `editing` is a rename in progress: the field takes the title's box.
pub fn thread_row_with_title(
    row: &ThreadRow,
    title: impl IntoElement,
    action: Option<AnyElement>,
    place: RowPlace,
    group: Option<GroupId>,
    editing: bool,
) -> Stateful<Div> {
    let thread = row.thread;
    let key = SharedString::from(format!(
        "nav-thread-{}-{}",
        group.map_or(0, GroupId::get),
        thread.get()
    ));
    // The shown Group's row carries the selection: its focused member
    // says so in its title's ink alone.
    let member = matches!(place, RowPlace::Member(_));
    let filled = row.selected && !member;
    let (ink, weight) = match title_face(row) {
        (ink, _) if member => (ink, W_BODY),
        face => face,
    };
    let action = action.filter(|_| !editing && hovered(&key));
    let mark = (!editing && action.is_none())
        .then(|| state_mark(row.status))
        .flatten();
    let title_w = title_w(place, action.is_some() || mark.is_some());
    pitched(row_frame(
        ("nav-thread", thread.get() as usize),
        key,
        filled,
    ))
    .debug_selector(move || format!("nav-thread-{}", thread.get()))
    .flex_row()
    .items_center()
    .h(px(NAV_ROW_H))
    .pl(px(place.title_x()))
    .pr(px(NAV_ROW_PAD_X))
    .child(
        fitted(title_w, FS_NAV, ink, weight, title)
            .debug_selector(move || format!("nav-title-{}", thread.get())),
    )
    .children(mark.map(|mark| {
        at_right(
            mark.debug_selector(move || format!("nav-state-{}", thread.get()))
                .into_any_element(),
        )
    }))
    .children(action.map(at_right))
}

/// What a row says at its right, where its `⋯` stands under the pointer —
/// only a state that asks for the operator, as Claude's list marks only
/// those: the needs-you dot (the one orange on the list), a failure's red
/// dot, the working spinner (the one thing that moves), or for a finish
/// not yet seen a smaller dot in the content ink (`NAV_UNREAD_DOT`),
/// cleared once seen. Idle and parked rows say nothing, nor does an idle
/// Main with work behind it (`Background`): its Pane and the chips over
/// its Composer say that.
pub(crate) fn state_mark(status: RowStatus) -> Option<Div> {
    let face = match status {
        RowStatus::NeedsYou | RowStatus::Failed | RowStatus::Working | RowStatus::Failing => {
            mark(status, true)
        }
        RowStatus::Done => div()
            .debug_selector(|| "nav-unread-dot".into())
            .size(px(NAV_UNREAD_DOT))
            .rounded_full()
            .bg(rgb(INK_STRONG))
            .into_any_element(),
        RowStatus::Idle | RowStatus::Parked | RowStatus::Background => return None,
    };
    // The `⋯` glyph's box, so the two centre on one spot.
    Some(
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(NAV_HEADING_GLYPH))
            .child(face),
    )
}

/// What a row's title gives up while its right holds a mark or its `⋯`
/// (`row_action`): the glyph's box, out to where the button's box meets
/// the row's padding, and `NAV_MARK_GAP` before it. Nothing while it holds
/// neither: the title takes the line.
const RIGHT_ROOM: f32 = NAV_LINE - (NAV_LINE - NAV_HEADING_GLYPH) / 2.0 + NAV_MARK_GAP;

/// How wide a Thread row's title box is at `place`, whether its right
/// holds a mark or a `⋯`.
pub fn title_w(place: RowPlace, marked: bool) -> f32 {
    place.text_w() - if marked { RIGHT_ROOM } else { 0.0 }
}

/// The Activity view's list (the bell's): `Priority` and `Today`, each a
/// plain heading (§5.1.3's grammar) over its rows, flush, a section gap
/// between them. The rows are the caller's (`activity_row`, wired).
pub fn activity_list(priority: Vec<AnyElement>, today: Vec<AnyElement>) -> Div {
    let mut list = view_list("nav-activity-list");
    if priority.is_empty() && today.is_empty() {
        return list.child(note("nav-activity-empty", "No activity today"));
    }
    let sections = [
        ("nav-activity-priority", "Priority", priority),
        ("nav-activity-today", "Today", today),
    ];
    let mut first = true;
    for (key, label, rows) in sections {
        if rows.is_empty() {
            continue;
        }
        list = list.child(section(first).child(heading(key, label)).children(rows));
        first = false;
    }
    list
}

/// One Activity row (Codex's): the title in primary ink with the row's
/// mark at its right (the spinner while it runs, the needs-you dot, a
/// failure's), then the Project in `INK_SUBTLE`, then up to
/// `NAV_ACTIVITY_PREVIEW_LINES` of the last message, wrapped and cut with
/// an ellipsis, in `INK_SUBTLE` too. It washes and selects as a tree row
/// does; the cockpit wires its press (`land_on_thread`).
pub fn activity_row(activity: &ActivityRow) -> Stateful<Div> {
    let row = &activity.row;
    let thread = row.thread;
    let key = SharedString::from(format!("nav-activity-row-{}", thread.get()));
    let mark = state_mark(row.status);
    let text_w = INNER_W - 2.0 * NAV_ROW_PAD_X;
    let title_w = text_w - if mark.is_some() { RIGHT_ROOM } else { 0.0 };
    let (ink, weight) = title_face(row);
    let meta = |text: SharedString| {
        div()
            .w(px(text_w))
            .text_size(px(FS_META))
            .line_height(px(LH_META))
            .text_color(rgb(INK_SUBTLE))
            .child(text)
    };
    pitched(row_frame(
        ("nav-activity-row", thread.get() as usize),
        key,
        row.selected,
    ))
    .debug_selector(move || format!("nav-activity-row-{}", thread.get()))
    .flex_col()
    .px(px(NAV_ROW_PAD_X))
    .py(px(NAV_ACTIVITY_PAD_Y))
    .child(
        div()
            .flex()
            .items_center()
            .h(px(NAV_LINE))
            .child(fitted(title_w, FS_NAV, ink, weight, row.name.clone()))
            .children(mark.map(|mark| {
                at_right(
                    mark.debug_selector(move || format!("nav-activity-state-{}", thread.get()))
                        .into_any_element(),
                )
            })),
    )
    .children(row.project.clone().map(|project| meta(project).truncate()))
    .children(activity.preview.clone().map(|preview| {
        meta(preview)
            .debug_selector(move || format!("nav-activity-preview-{}", thread.get()))
            .whitespace_normal()
            .line_clamp(NAV_ACTIVITY_PREVIEW_LINES)
            .text_ellipsis()
    }))
}

// ------------------------------------------------------------- the views' lists// ------------------------------------------------------------- the views' lists

/// A Notice under `Recent` in the Inbox (or a request from a
/// machine off screen above it), two lines: its mark, its title
/// (`INK_STRONG` while unread) and its age, then its state's word and
/// detail in the word's ink. The cursor's row is selected.
pub fn notice_row(index: usize, row: &NoticeRow, cursor: bool, measure: &Measure) -> Stateful<Div> {
    let key = SharedString::from(format!("notice-row-{index}"));
    let status = match row.state {
        NoticeState::NeedsYou(_) => RowStatus::NeedsYou,
        NoticeState::Failed => RowStatus::Failed,
        NoticeState::Done => RowStatus::Done,
    };
    let ink = match row.state {
        NoticeState::NeedsYou(_) => ATTENTION,
        NoticeState::Failed => BLOCKED,
        NoticeState::Done => {
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
    let text_w = RowPlace::Inbox.text_w();
    // In its first minute a Notice has no age to say: the title takes the
    // line.
    let age = (!row.when.is_empty()).then(|| row.when.clone());
    let age_w = age
        .as_ref()
        .map_or(0.0, |age| measure.ui(age, FS_META, W_BODY) + NAV_WORD_GAP);
    // Primary ink, as every nav title; unread says so in weight.
    let title_weight = if cursor || !row.read { W_LABEL } else { W_BODY };
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
        )))
}

/// A long section's `Show more` / `Show less` (Codex's): one line on the
/// title column in `INK_SUBTLE`, washed under the pointer as a row is. It
/// hangs at `place`. The cockpit wires its press.
pub fn show_more(index: usize, more: ShowMore, place: RowPlace) -> Stateful<Div> {
    let key = SharedString::from(format!("nav-show-more-{index}"));
    pitched(row_frame(("nav-show-more", index), key, false))
        .debug_selector(move || format!("nav-show-more-{index}"))
        .flex_row()
        .items_center()
        .h(px(NAV_ROW_H))
        .pl(px(place.title_x()))
        .pr(px(NAV_ROW_PAD_X))
        .text_color(rgb(INK_SUBTLE))
        .child(more.label())
}

/// One quiet line in a view's list or the tree, on the title column:
/// what an empty list says (`Nothing needs you`).
pub fn note(key: &'static str, text: impl Into<SharedString>) -> Div {
    div()
        .debug_selector(move || key.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(NAV_ROW_H))
        .px(px(NAV_ROW_PAD_X))
        .text_color(rgb(INK_SUBTLE))
        .child(text.into())
}

/// A view's list: its rows, flush, in the tree's place.
pub fn view_list(key: &'static str) -> Div {
    div()
        .debug_selector(move || key.into())
        .flex()
        .flex_col()
        .flex_shrink_0()
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
        .min_h(px(NAV_ROW_H))
}

/// What an empty tree says, sentence case and no full stop: a line in
/// `INK_SUBTLE` and, where there is one, a way forward in the same voice.
/// Filtered to a Project it names the Project rather than shrugging; when
/// the Parked view holds Threads the filter admits, it says *open* and
/// offers the door there (`parked_link`, which the cockpit wires). An
/// empty store offers the key that starts one.
pub fn empty_filter(project: Option<&str>, parked: Option<AnyElement>) -> Div {
    let message = match (project, parked.is_some()) {
        (Some(project), _) => format!("No open threads in {project}"),
        (None, false) => "No threads yet".to_string(),
        (None, true) => "No open threads".to_string(),
    };
    let hint: Option<AnyElement> = match (project, parked) {
        (_, Some(parked)) => Some(parked),
        (None, None) => components::bound_chord("cockpit::NewThread").map(|keys| {
            div()
                .flex()
                .items_center()
                .gap(px(SPACE_1))
                .text_color(rgb(INK_SUBTLE))
                .child(chord_glyphs(&keys))
                .child("New thread")
                .into_any_element()
        }),
        (Some(_), None) => None,
    };
    div()
        .debug_selector(|| "nav-empty".into())
        .flex()
        .flex_col()
        .flex_shrink_0()
        .items_start()
        .px(px(NAV_ROW_PAD_X))
        .text_size(px(FS_META))
        .line_height(px(NAV_LINE))
        .child(
            div()
                .text_color(rgb(INK_SUBTLE))
                .child(SharedString::from(message)),
        )
        .children(hint)
}

/// An empty tree's way to its parked Threads: `Show parked threads`, in
/// the empty line's `INK_SUBTLE` blending up to `INK_STRONG` under the pointer, a
/// link in that line's voice.
pub fn parked_link() -> Stateful<Div> {
    const KEY: &str = "nav-empty-parked";
    let blend = SharedString::from(KEY);
    let ink = motion::mix(
        rgb(INK_SUBTLE).into(),
        rgb(INK_STRONG).into(),
        motion::hover_t(&blend),
    );
    div()
        .id(KEY)
        .debug_selector(|| KEY.into())
        .text_color(ink)
        .on_hover(motion::hover_listener(blend))
        .child("Show parked threads")
}

/// A refusal from the last Group change, at the top of the tree, until the
/// next change succeeds: a `BLOCKED` dot and the refusal, dim, wrapping.
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
        .child(components::fold_chevron(!folded, NAV_CHEVRON, INK_SUBTLE))
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

// ------------------------------------------------------------- the foot

/// The foot (§5.1.8), Claude's account row: who is at the machine — their
/// initial on a `NAV_AVATAR` disc and their name in 13 `INK_STRONG` — then free
/// disk once it runs low (`frame::free_disk_words`, in its ink), and the
/// gear. The version shows under the pointer, as an app's About would.
pub fn foot(user: &str, version: &str, disk: Option<(String, u32)>, gear: impl IntoElement) -> Div {
    let initial: SharedString = user
        .chars()
        .next()
        .map(|first| first.to_uppercase().collect::<String>())
        .unwrap_or_default()
        .into();
    let about = SharedString::from(format!("Ferrite {}", short_version(version)));
    div()
        .debug_selector(|| "nav-foot".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(SPACE_1))
        .pt(px(NAV_FOOT_PAD_T))
        .pl(px(NAV_ROW_PAD_X))
        .pr(px(NAV_HEAD_PAD_R))
        .child(
            div()
                .id("nav-account")
                .debug_selector(|| "nav-account".into())
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(NAV_ICON_GAP - (NAV_AVATAR - NAV_ICON)))
                .tooltip(ferrite_ui::menu::tooltip(about))
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .size(px(NAV_AVATAR))
                        .rounded_full()
                        .bg(paint::SELECTED)
                        .text_size(px(FS_AVATAR))
                        .font_weight(W_LABEL)
                        .text_color(rgb(INK_STRONG))
                        .child(initial),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(FS_NAV))
                        .text_color(rgb(INK_STRONG))
                        .child(SharedString::from(user.to_string())),
                ),
        )
        .children(disk.map(|(words, ink)| {
            components::tabular(trailing(words, FS_META, ink))
                .debug_selector(|| "nav-free-disk".into())
        }))
        .child(gear)
}

/// A version as the foot says it: its major and minor (`0.5.0` → `0.5`).
fn short_version(version: &str) -> String {
    version.split('.').take(2).collect::<Vec<_>>().join(".")
}

/// The gear in the foot: the settings door.
pub fn gear_button() -> Stateful<Div> {
    icon_button(
        "nav-gear",
        icons::GEAR,
        NAV_TOGGLE_GLYPH,
        ICON_BUTTON,
        R_CONTROL,
    )
    .tooltip(ferrite_ui::menu::action_tooltip(
        "Settings",
        "cockpit::OpenSettings",
    ))
}

// ------------------------------------------------------------- the rail

/// One open Thread on the rail: its mark, and the digit of the ⌘1–9 that
/// focuses it, if one does.
#[derive(Clone)]
pub struct RailRow {
    pub key: Option<usize>,
    pub thread: ThreadId,
    pub status: RowStatus,
    /// The board shows this Thread (Solo) or it is the shown Group's
    /// focused member: the rail's one selection, as the tree's.
    pub selected: bool,
}

/// The rail's content (cmd-B folded, §5.1.7): under the head row, its
/// toggle (`toggle`), then one cell per open Thread. It scrolls like the
/// tree.
pub fn rail(scroll: &ScrollHandle, toggle: impl IntoElement) -> Stateful<Div> {
    div()
        .id(("nav-rail", 0usize))
        .debug_selector(|| "nav-rail".into())
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .w(px(FOLDED_WIDTH))
        .flex()
        .flex_col()
        .items_center()
        .px(px(NAV_COL_PAD))
        .pt(px(WIN_CHROME_H))
        .pb(px(NAV_COL_PAD))
        .overflow_y_scroll()
        .track_scroll(scroll)
        .child(div().flex_shrink_0().mb(px(SPACE_1)).child(toggle))
}

/// One rail cell, 32 high: the Thread's mark (the tree row's own, spinner
/// included), and under it its ⌘ digit in 11 `INK_SUBTLE` where a ⌘ key
/// reaches it. It washes and selects as a tree row does.
pub fn rail_row(row: &RailRow) -> Stateful<Div> {
    let thread = row.thread;
    let key = SharedString::from(format!("nav-rail-{}", thread.get()));
    row_frame(("nav-rail-row", thread.get() as usize), key, row.selected)
        .debug_selector(move || format!("nav-rail-{}", thread.get()))
        .flex_col()
        .items_center()
        .justify_center()
        .w_full()
        .h(px(NAV_RAIL_CELL))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .h(px(NAV_MARK_ICON))
                .child(mark(row.status, true)),
        )
        .children(row.key.map(|key| {
            components::tabular(
                div()
                    .debug_selector(move || format!("nav-rail-key-{key}"))
                    .text_size(px(NAV_RAIL_KEY_FS))
                    .line_height(px(NAV_RAIL_KEY_FS + 2.0))
                    .text_color(rgb(if row.selected { INK_STRONG } else { INK_SUBTLE }))
                    .child(key.to_string()),
            )
        }))
}

// ------------------------------------------------------------- rename and drag

/// A Group title that can be renamed: the row's own title text, and
/// nothing else. It wears **no** hover wash: the double click is the
/// affordance; the row is the control.
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
/// tree draws, at the column's inner width, on the float ground with its
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
            thread: ThreadId::new(8),
            status: RowStatus::Idle,
            name: "thread-08".into(),
            project: Some("ferrite".into()),
            selected,
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

    fn heading_of(folded: bool) -> ProjectSection {
        ProjectSection {
            project: None,
            mixed: false,
            label: "ferrite".into(),
            branch: None,
            folded,
            roll_up: None,
            count: 2,
            solos: Vec::new(),
            drawn: Vec::new(),
            more: None,
            groups: Vec::new(),
        }
    }

    /// The row grammar (§5.1): rounded rows, a Thread one quiet line at 30
    /// stepping 34 with its title on its heading's label, a Group and a Project heading one
    /// line too, a Project section 6px below the one above it.
    #[test]
    fn nav_rows_are_rounded_two_line_and_one_line() {
        let mut row = thread_row(&thread());
        let style = row.style();
        assert_eq!(
            style.corner_radii.top_left,
            Some(px(R_CONTROL).into()),
            "radius 6"
        );
        assert_eq!(style.size.height, Some(px(NAV_ROW_H).into()));
        assert_eq!(style.margin.top, Some(px(2.0).into()), "pitch 34");
        assert_eq!(style.margin.bottom, Some(px(2.0).into()));
        assert_eq!(style.padding.left, Some(px(NAV_ROW_PAD_X).into()));
        assert_eq!(style.padding.right, Some(px(NAV_ROW_PAD_X).into()));
        let mut row = group_row(&group(false, false), 0);
        assert_eq!(row.style().size.height, Some(px(NAV_ROW_H).into()));
        let mut later = section(false);
        assert_eq!(
            later.style().margin.top,
            Some(px(NAV_SECTION_GAP).into()),
            "6px above every Project section but the first"
        );
        assert_eq!(section(true).style().margin.top, None);
        assert_eq!((NAV_ROW_H, NAV_ROW2_H, NAV_SECTION_GAP), (30.0, 44.0, 6.0));
        assert_eq!(WIDTH, 256.0, "Claude's 256px sidebar");
        assert_eq!(INNER_W, 240.0, "padding 0 8 8");
        assert_eq!(RowPlace::ROOT.text_w(), 220.0);
        assert_eq!(
            RowPlace::ROOT.title_x(),
            NAV_ROW_PAD_X,
            "a heading's rows start on its label (Claude's 18 from the edge)"
        );
        assert_eq!(
            RowPlace::Inbox.title_x(),
            NAV_TITLE_X,
            "past the mark column"
        );
        assert_eq!(RowPlace::Row(1).title_x(), NAV_ROW_PAD_X + NAV_TREE_INDENT);
        assert_eq!(RowPlace::Member(0).title_x(), RowPlace::Row(1).title_x());
    }

    /// The Activity view's row (Codex's): the title with the spinner at
    /// its right while it runs, then the Project and the last message's
    /// opening, clamped to two lines; a row with no preview says none.
    #[test]
    fn an_activity_row_says_title_project_and_preview() {
        let working = ActivityRow {
            row: ThreadRow {
                status: RowStatus::Working,
                project: Some("ferrite".into()),
                ..thread()
            },
            preview: Some("Split the nav into its views".into()),
        };
        let mut drawn = activity_row(&working);
        assert_eq!(drawn.style().padding.left, Some(px(NAV_ROW_PAD_X).into()));
        assert!(state_mark(RowStatus::Working).is_some(), "the spinner");
        assert_eq!(NAV_ACTIVITY_PREVIEW_LINES, 2);
        let quiet = ActivityRow {
            preview: None,
            ..working
        };
        let _ = activity_row(&quiet);
    }

    /// N-5, the selection rule: one selection, on the row of what the board
    /// shows — `SELECTED`, never a coloured outline.
    #[test]
    fn one_selection_marks_what_the_board_shows() {
        let fill = |mut drawn: Stateful<Div>| drawn.style().background.clone();
        let selection = Some(gpui::Hsla::from(paint::SIDEBAR_SELECTED).into());
        assert_eq!(fill(group_row(&group(true, false), 0)), selection);
        assert_eq!(fill(group_row(&group(false, false), 0)), None);
        assert_eq!(
            fill(thread_row_with_title(
                &thread(),
                "thread-08",
                None,
                RowPlace::Member(0),
                Some(GroupId::new(1)),
                false,
            )),
            None,
            "an unfocused member carries no fill"
        );
        assert_eq!(fill(thread_row(&selected_thread(true))), selection);
        assert_eq!(fill(thread_row(&thread())), None);
    }

    /// §5.1.5: a title is `INK_STRONG`, 500 when selected,
    /// `INK_STRONG` while its finish is unseen, `INK_SUBTLE` when parked.
    #[test]
    fn titles_follow_selection_unread_and_parked() {
        assert_eq!(title_face(&thread()), (INK_STRONG, W_BODY));
        assert_eq!(title_face(&selected_thread(true)), (INK_STRONG, W_LABEL));
        let done = ThreadRow {
            status: RowStatus::Done,
            ..thread()
        };
        assert_eq!(title_face(&done), (INK_STRONG, W_LABEL), "unread in weight");
        let parked = ThreadRow {
            status: RowStatus::Parked,
            ..thread()
        };
        assert_eq!(title_face(&parked), (INK_SUBTLE, W_BODY));
    }

    /// N-13: the pointer stays the arrow over every row.
    #[test]
    fn every_nav_row_keeps_the_arrow() {
        let cursor = |mut drawn: Stateful<Div>| drawn.style().mouse_cursor;
        assert_eq!(cursor(thread_row(&thread())), None);
        assert_eq!(cursor(group_row(&group(false, true), 0)), None);
        assert_eq!(
            cursor(project_section(
                0,
                &heading_of(false),
                None,
                &Measure::Cells(7.0)
            )),
            None
        );
    }

    /// The Parked, Working and Done views group their rows as the tree
    /// does (§5.1.2): by Project in creation order, `Other` last, then by
    /// home Group in the durable order, each run keeping the list's order.
    #[test]
    fn a_views_rows_group_by_project_then_by_group() {
        let project = |id: u64| -> ProjectId { serde_json::from_str(&id.to_string()).unwrap() };
        let row = |id: u64| ThreadRow {
            thread: ThreadId::new(id),
            ..thread()
        };
        // Thread → (Project, home Group's durable place).
        let places: [(u64, Option<u64>, Option<usize>); 6] = [
            (1, Some(2), None),
            (2, Some(1), Some(1)),
            (3, None, None),
            (4, Some(1), None),
            (5, Some(1), Some(0)),
            (6, Some(1), Some(1)),
        ];
        let rows = places.iter().map(|(id, ..)| row(*id)).collect();
        let tree = ViewSection::tree(
            rows,
            |row| {
                let (_, owner, group) = places
                    .iter()
                    .find(|(id, ..)| *id == row.thread.get())
                    .copied()
                    .unwrap();
                ViewPlace {
                    project: owner.map(project),
                    group: group.map(|order| HomeGroup {
                        order,
                        id: GroupId::new(order as u64 + 10),
                        title: format!("g{order}").into(),
                        folded: false,
                    }),
                }
            },
            |owner, _| owner.map_or("other".into(), |id| format!("p{}", id.get()).into()),
        );
        let ids = |rows: &[ThreadRow]| rows.iter().map(|row| row.thread.get()).collect::<Vec<_>>();
        let labels: Vec<&str> = tree.iter().map(|section| section.label.as_ref()).collect();
        assert_eq!(labels, ["p1", "p2", "other"], "creation order, Other last");
        assert_eq!(ids(&tree[0].solos), [4], "its loose rows");
        let groups: Vec<&str> = tree[0].groups.iter().map(|g| g.title.as_ref()).collect();
        assert_eq!(groups, ["g0", "g1"], "its Groups in the durable order");
        assert_eq!(ids(&tree[0].groups[0].members), [5]);
        assert_eq!(ids(&tree[0].groups[1].members), [2, 6], "the list's order");
        assert!(
            tree[0].groups.iter().all(|g| !g.selected),
            "never the board's"
        );
        assert_eq!(ids(&tree[1].solos), [1]);
        assert_eq!(ids(&tree[2].solos), [3]);
        assert!(
            ViewSection::tree(Vec::new(), |_| unreachable!(), |_, _| unreachable!()).is_empty()
        );
    }

    /// Every Thread row is the one 30px line whatever arrives (§5.1.5):
    /// a word, a mark, a member's place. The rail keeps its cell.
    #[test]
    fn every_row_is_one_line_whatever_its_facts() {
        let height = |mut drawn: Stateful<Div>| drawn.style().size.height;
        let one = Some(px(NAV_ROW_H).into());
        for tail in [
            NavTail::None,
            NavTail::Waiting(words::APPROVAL),
            NavTail::Failing(Some(12)),
            NavTail::Failed,
            NavTail::Done,
            NavTail::Working("1m04s".into()),
        ] {
            let row = ThreadRow {
                tail: tail.clone(),
                ..thread()
            };
            assert_eq!(height(thread_row(&row)), one, "{tail:?}");
        }
        for status in [
            RowStatus::Working,
            RowStatus::NeedsYou,
            RowStatus::Done,
            RowStatus::Parked,
        ] {
            let row = ThreadRow { status, ..thread() };
            assert_eq!(height(thread_row(&row)), one, "{status:?}");
        }
        let member = thread_row_with_title(
            &thread(),
            "thread-08",
            None,
            RowPlace::Member(0),
            None,
            false,
        );
        assert_eq!(height(member), one);
        let rail = RailRow {
            key: Some(9),
            thread: ThreadId::new(9),
            status: RowStatus::Done,
            selected: false,
        };
        assert_eq!(
            rail_row(&rail).style().size.height,
            Some(px(NAV_RAIL_CELL).into())
        );
    }

    /// A row's right says one thing (§5.1.5): the needs-you dot, a
    /// failure's, the working spinner, an unseen finish's small grey dot;
    /// idle, parked and background-only rows say nothing there.
    #[test]
    fn only_a_state_worth_saying_marks_the_right() {
        for status in [
            RowStatus::NeedsYou,
            RowStatus::Failed,
            RowStatus::Working,
            RowStatus::Failing,
            RowStatus::Done,
        ] {
            assert!(state_mark(status).is_some(), "{status:?}");
        }
        for status in [RowStatus::Idle, RowStatus::Parked, RowStatus::Background] {
            assert!(state_mark(status).is_none(), "{status:?}");
        }
        assert!(
            NAV_UNREAD_DOT < NAV_DOT_SMALL,
            "the unread dot is a step under the needs-you dot"
        );
    }

    /// A Group's roll-up (§5.1.6): how many need the operator, else how
    /// many failed, in that state's ink; nothing while all is quiet.
    #[test]
    fn a_group_rolls_up_the_worst_word() {
        let rows = |statuses: &[RowStatus]| -> Vec<ThreadRow> {
            statuses
                .iter()
                .map(|status| ThreadRow {
                    status: *status,
                    ..thread()
                })
                .collect()
        };
        assert_eq!(
            RollUp::of(&rows(&[RowStatus::Idle, RowStatus::Working])),
            None
        );
        assert_eq!(
            RollUp::of(&rows(&[RowStatus::Failed, RowStatus::Working])),
            Some(RollUp {
                text: "1 failed".into(),
                ink: BLOCKED
            })
        );
        assert_eq!(
            RollUp::of(&rows(&[
                RowStatus::NeedsYou,
                RowStatus::Failed,
                RowStatus::NeedsYou
            ])),
            Some(RollUp {
                text: "2 need you".into(),
                ink: ATTENTION
            })
        );
        assert_eq!(
            RollUp::of(&rows(&[RowStatus::NeedsYou])).unwrap().text,
            "1 needs you"
        );
    }

    /// A folded Project heading says, first that has something: its
    /// roll-up word, a branch off the default, its count.
    #[test]
    fn a_folded_heading_says_the_first_thing_it_has() {
        let mut section = heading_of(true);
        assert_eq!(section.folded_summary(), ("2".into(), INK_SUBTLE));
        section.branch = Some(("main".into(), true));
        assert_eq!(section.folded_summary().0, "2", "the default says nothing");
        section.branch = Some(("overnight/coach".into(), false));
        assert_eq!(section.folded_summary().0, "overnight/coach");
        section.roll_up = Some(RollUp {
            text: "1 failed".into(),
            ink: BLOCKED,
        });
        assert_eq!(section.folded_summary(), ("1 failed".into(), BLOCKED));
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

    /// The column is the frame from the window's top, and its content is
    /// 8px in at its sides and foot.
    #[test]
    fn the_column_is_the_frame_from_the_top() {
        let mut column = shell(WIDTH);
        let style = column.style();
        assert_eq!(style.background, None, "the body paints the ground");
        assert_eq!(style.size.width, Some(px(WIDTH).into()));
        assert_eq!(FOLDED_WIDTH, 52.0, "folded, the rail");
        let mut ground = body();
        assert_eq!(
            ground.style().background,
            Some(gpui::Hsla::from(paint::SIDEBAR).into())
        );
        let mut inner = content();
        let style = inner.style();
        assert_eq!(style.padding.left, Some(px(NAV_COL_PAD).into()));
        assert_eq!(style.padding.bottom, Some(px(NAV_COL_PAD).into()));
        assert_eq!(style.padding.top, None, "the head is in the head row");
        assert_eq!(
            head(div(), div(), div(), div()).style().size.height,
            Some(px(WIN_CHROME_H).into())
        );
    }

    /// The rail keeps every open Thread's cell: its mark and its ⌘ digit;
    /// the selected cell is the tree's selection.
    #[test]
    fn the_rail_keys_its_threads_and_keeps_their_marks() {
        let row = |selected| RailRow {
            key: Some(3),
            thread: ThreadId::new(4),
            status: RowStatus::NeedsYou,
            selected,
        };
        assert_eq!(rail_row(&row(false)).style().background, None);
        assert_eq!(
            rail_row(&row(true)).style().background,
            Some(gpui::Hsla::from(paint::SIDEBAR_SELECTED).into())
        );
        let scroll = ScrollHandle::new();
        assert_eq!(
            rail(&scroll, div()).style().size.width,
            Some(px(FOLDED_WIDTH).into())
        );
    }

    /// The ⌘ digits are bound: each names the action its key is bound to.
    #[test]
    fn the_command_digits_are_bound() {
        for ordinal in 1..=9 {
            let action = crate::cockpit::focus_rail_action(ordinal).expect("one action per digit");
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
        assert_eq!(crate::cockpit::focus_rail_action(10), None);
    }

    /// The foot names the app and its major and minor version.
    #[test]
    fn the_foot_shortens_the_version_and_a_branch() {
        assert_eq!(short_version("0.5.0"), "0.5");
        assert_eq!(short_version("1.12.3-beta"), "1.12");
        assert_eq!(short_branch("dev").as_ref(), "dev");
        let long = short_branch("worktree-pay-api-migration-cleanup");
        assert_eq!(long.chars().count(), NAV_BRANCH_MAX_CH);
        assert!(long.ends_with('\u{2026}'));
    }

    /// The nav's own clock: the content fades over 150ms on CSS `ease`
    /// (its hovers take the one hover blend, `motion::HOVER_FADE`).
    #[test]
    fn the_nav_keeps_its_clocks() {
        assert_eq!(CONTENT_FADE.duration_ms, 150);
        assert_eq!(CONTENT_FADE.curve, motion::EASE);
    }
}

/// The hover each row's wash was last drawn at: the stale-chrome tests read
/// what is on screen, not the blend.
#[cfg(test)]
pub(crate) mod testing {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use gpui::SharedString;

    thread_local! {
        pub(super) static DRAWN_HOVER: RefCell<HashMap<SharedString, f32>> =
            RefCell::new(HashMap::new());
    }

    /// The hover the row behind `key` was last drawn at.
    pub(crate) fn drawn_hover(key: &str) -> Option<f32> {
        DRAWN_HOVER.with(|drawn| drawn.borrow().get(key).copied())
    }
}
