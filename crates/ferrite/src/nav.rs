//! The left navigation column (SPEC §5.1, `mockups/solo.png`): the head
//! (the traffic lights' spacer and the sidebar toggle), the views (`Search`,
//! `Needs you`, `Parked`, `Tasks`, and smart filtering's `Working` and
//! `Done`), the tree — one section per Project, its loose Threads and its
//! Groups, each Group's members hanging under it — and the foot (`Ferrite
//! 0.5` and the gear). A view other than the tree shows its list in the
//! tree's place; Esc or a click on the active view returns to the tree.
//! It is a view, never the only door: everything a row does (focus,
//! revive, regroup) stays reachable from the keyboard and the ⌘K palette.
//!
//! Drawing only, like `pane.rs`: the cockpit assembles a `NavState` per
//! frame from O(1) reads plus its project/branch/parked caches —
//! `Store::load` and `Instruments::of` are banned here, which is what keeps
//! the 24-Pane wall smooth with the nav open. Click wiring stays in
//! `cockpit.rs`, the same split `pane_cell` uses.
//!
//! **A list on the frame.** The column is `paint::FRAME` from the window's
//! top. Rows are rounded, wear `paint::RAISED` under the pointer (80ms) and
//! `paint::SELECTED` with an inset `ROW_RING` when they stand for what the
//! board shows: the Solo Thread's row, or the shown Group's. A Thread row
//! is two lines — what it is (its title, its diffstat at the right), then
//! where it is (its branch, or its state's reason; its PR's label at the
//! right) — with its mark (§6) in the column before the title.
//!
//! Colour is state (ADR 0033): the mark, a blocked or failed row's line 2,
//! a Group's roll-up word, the needs-you pill. One thing moves: a working
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

use crate::cockpit::{thread_status, Attention, ThreadStatus};
use crate::components;
use crate::icons::{self, icon};
use crate::machines::{Connection, MachineRow};
use crate::motion::{self, MotionSpec};
use crate::notifications::{Row as NoticeRow, State as NoticeState};
use crate::pane::WallState;
use crate::theme::*;
use ferrite_core::cockpit::StatusRow;

/// The nav's two widths — 264 open, and the 52px rail folded (cmd-B).
/// `CockpitView::cell()` subtracts whichever is live, so the nav stays part
/// of the semantic-zoom input rather than a special case.
pub use crate::theme::{NAV_FOLDED_W as FOLDED_WIDTH, NAV_WIDTH as WIDTH};

gpui::actions!(nav, [CloseView]);

/// The column's inner width: its rows reach this far.
const INNER_W: f32 = WIDTH - 2.0 * NAV_COL_PAD;

/// The slack a truncating title's budget gets over its visible box.
///
/// gpui truncates by summing each character's advance measured **alone**
/// (gpui text_system/line_wrapper.rs `width_for_char`, cached per char) and
/// keeps a prefix only while `width + suffix_width < truncate_width` — a
/// strict `<`, against CSS's `<=`. At an exact fit the last glyph that
/// would still fit is dropped. Handing the truncator this much extra budget
/// restores the glyph; the visible box stays exactly its pinned width, and
/// clips.
const TRUNCATE_SLOP: f32 = SPACE_1;

const COLLAPSE_KEY: &str = "nav-collapse";

/// A nav row's hover wash.
pub(crate) const ROW_HOVER: MotionSpec = MotionSpec::new(MOTION_NAV_HOVER_MS, motion::EASE);
/// An icon button's hover, ground and glyph.
pub(crate) const DOOR_HOVER: MotionSpec = MotionSpec::new(MOTION_NAV_DOOR_MS, motion::EASE);
/// The column's content fading as it folds or opens, on its own clock
/// apart from the width's ride.
pub(crate) const CONTENT_FADE: MotionSpec = MotionSpec::new(MOTION_NAV_FADE_MS, motion::EASE);

/// What a fold in the nav holds shut (N-4): a Project section shut to its
/// heading, the Groups section, or a Group shut to its row (its count
/// kept). Held in memory by the cockpit (`CockpitView::set_nav_fold`), so
/// a fold survives every re-render and view switch; opening a folded Group
/// never unfolds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum NavFold {
    Project(ProjectId),
    /// The Groups section: the Groups that span Projects.
    Groups,
    Group(GroupId),
}

/// What the nav shows under its views (§5.1.2): the tree, or one view's
/// list in its place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavView {
    #[default]
    Tree,
    /// The Needs-you list: the blocked Threads oldest-first, then the
    /// Notices under `Recent`. It is the bell's list.
    NeedsYou,
    /// The parked Threads no Group claims.
    Parked,
    /// Smart filtering's working Threads.
    Working,
    /// Smart filtering's Threads that finished unseen.
    Done,
}

/// What the nav draws this frame: the Project sections and the views'
/// lists. Nothing here is a store read — the cockpit assembles it from
/// O(1) reads plus its project/branch caches.
pub struct NavState {
    /// Smart filtering's lists, or `None` while it is off.
    pub smart: Option<SmartSections>,
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
    /// Every row the tree draws, in the order it draws them: each section's
    /// loose rows, then the members of each of its unfolded Groups. A
    /// folded section or Group draws none. The views' lists are not the
    /// tree; tests read the tree's order from here.
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
    /// What the heading says at its right: the shared branch's drift
    /// against its upstream (`↑715`, `↑1 ↓5`). `None` says nothing.
    pub drift: Option<SharedString>,
    /// A task's own space: its row's numbers (`8% 16m`), under the heading
    /// (ADR 0031).
    pub numbers: Option<SharedString>,
    /// The heading folded it: one row, and nothing under it.
    pub folded: bool,
    /// Its own rows' roll-up word (`RollUp::of`), which a folded heading
    /// says.
    pub roll_up: Option<RollUp>,
    /// How many rows it holds, which a quiet folded heading says.
    pub count: usize,
    /// Indices into `NavState::solos`.
    pub solos: Vec<usize>,
    /// Indices into `NavState::groups`.
    pub groups: Vec<usize>,
}

impl ProjectSection {
    /// What a folded heading says at its right, the first that has
    /// something to say: its rows' roll-up word, its drift, its branch when
    /// that is not the default, its row count.
    pub fn folded_summary(&self) -> (SharedString, u32) {
        if let Some(word) = &self.roll_up {
            return (word.text.clone(), word.ink);
        }
        let text = self
            .drift
            .clone()
            .or_else(|| {
                self.branch
                    .as_ref()
                    .filter(|(_, default)| !default)
                    .map(|(branch, _)| short_branch(branch))
            })
            .unwrap_or_else(|| self.count.to_string().into());
        (text, TEXT_DIM)
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
            let word = if waiting == 1 {
                words::NEEDS_YOU
            } else {
                words::NEED_YOU
            };
            return Some(RollUp {
                text: format!("{waiting} {word}").into(),
                ink: ATTENTION,
            });
        }
        (failed > 0).then(|| RollUp {
            text: format!("{failed} {}", words::FAILED).into(),
            ink: BLOCKED,
        })
    }
}

/// Smart filtering (`Settings::smart_filtering`): two lists, each a view.
/// Each row is a reference to the Thread — its own row stays in the tree.
#[derive(Clone, Default)]
pub struct SmartSections {
    /// Threads that finished unseen (`RowStatus::Done`). Seeing one reads
    /// it, and it leaves.
    pub done: Vec<ThreadRow>,
    /// Threads whose Main or a Subagent is working right now.
    pub working: Vec<ThreadRow>,
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
    /// The branch its checkout is on (`facts.branch`, the live status's
    /// first).
    pub branch: Option<SharedString>,
    /// The agent's edits summed over `facts.changed_files` (added,
    /// removed): what this Thread changed, not a diff against the base.
    pub diffstat: Option<(usize, usize)>,
    /// Since it was last used (`facts::since_label`), which a failed row's
    /// line 2 says.
    pub age: SharedString,
    /// Its checkout and its PR at a glance (`RowDetails`).
    pub details: RowDetails,
    /// Its state's word (C10): what line 2 says for a state with a reason.
    pub tail: NavTail,
    /// The board's words for the Thread's agent, under its line 2 (ADR
    /// 0031).
    pub task: Option<TaskRows>,
}

/// Rows 2 and 3 of herdr's sidebar entry for a Thread the board runs, in
/// the board's own words (`sidebar::Told::rows`).
#[derive(Clone, Debug, PartialEq)]
pub struct TaskRows {
    /// Row 2: who is on it and where it stands, then its time, last
    /// (`agent · working · R#1 PR#113 · 6m`).
    pub stands_timed: SharedString,
    /// Row 2 without its time, for a row too short for the whole.
    pub stands: SharedString,
    /// Row 2's ink: where the task stands.
    pub ink: u32,
    /// Row 3, dim: the row's numbers, bare (`8% 16m`, `Q#2`), or the
    /// Coordinator's latest message.
    pub third: SharedString,
}

/// A row's checkout facts: commits ahead of and behind its upstream and a
/// dirty working tree, from the slow branch-status cadence, and its PR's
/// label (`pr::PrWatch::label`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowDetails {
    pub ahead: u32,
    pub behind: u32,
    pub dirty: bool,
    pub pr: Option<String>,
}

impl RowDetails {
    pub fn of(status: Option<&ferrite_core::workspace::BranchStatus>, pr: Option<String>) -> Self {
        RowDetails {
            ahead: status.map_or(0, |status| status.ahead),
            behind: status.map_or(0, |status| status.behind),
            dirty: status.is_some_and(|status| status.dirty > 0),
            pr,
        }
    }

    /// The checkout's facts as line 2 types them after the branch: `↑2↓1`
    /// drift, `*` a dirty tree. `None` when there is nothing to say.
    pub fn text(&self) -> Option<SharedString> {
        let mut text = String::new();
        if self.ahead > 0 {
            text.push_str(&format!("\u{2191}{}", self.ahead));
        }
        if self.behind > 0 {
            text.push_str(&format!("\u{2193}{}", self.behind));
        }
        if self.dirty {
            text.push('*');
        }
        (!text.is_empty()).then(|| text.into())
    }
}

/// A drift as a Project heading says it: `↑715`, `↑1 ↓5`; `None` when
/// even.
pub fn drift_text(ahead: u32, behind: u32) -> Option<SharedString> {
    let parts: Vec<String> = [(ahead, '\u{2191}'), (behind, '\u{2193}')]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, arrow)| format!("{arrow}{count}"))
        .collect();
    (!parts.is_empty()).then(|| parts.join(" ").into())
}

/// What a row's state says (C10), in priority order: a pending Decision
/// names what it waits for (`approval` / `plan` / `question`); red tests
/// `failing`/`failing N` and a failed turn or closed Session `failed`; a
/// working Thread its live elapsed time (`1m04s`, never coarsened); a
/// finish not yet seen `done`; work still running behind an idle Main
/// `background`; otherwise the age, once it reaches a minute. Never `now`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavTail {
    Waiting(&'static str),
    Failing(Option<u32>),
    Failed,
    Working(SharedString),
    Done,
    Background,
    Age(SharedString),
    None,
}

impl NavTail {
    /// The word from the Pane's own reading of the Thread (`pane::thread_word`
    /// — the head's slot word, `done` only while unseen), so the nav and the
    /// Pane never disagree, and the row's own state for what only the row
    /// says: background work.
    pub fn of(status: RowStatus, slot: Option<&crate::pane::HeadSlot>, age: SharedString) -> Self {
        use crate::pane::HeadSlot;
        match slot {
            Some(HeadSlot::NeedsYou(kind)) => NavTail::Waiting(kind),
            Some(HeadSlot::Failing(count)) => {
                NavTail::Failing(count.map(|count| count.min(u32::MAX as usize) as u32))
            }
            Some(HeadSlot::Failed) => NavTail::Failed,
            Some(HeadSlot::Working(elapsed)) if elapsed.is_empty() => NavTail::None,
            Some(HeadSlot::Working(elapsed)) => NavTail::Working(elapsed.clone().into()),
            Some(HeadSlot::Done) => NavTail::Done,
            _ if status == RowStatus::Background => NavTail::Background,
            _ => NavTail::Age(age),
        }
    }

    /// The word itself, from the lexicon (`theme::words`). An empty age says
    /// nothing.
    pub fn text(&self) -> Option<SharedString> {
        match self {
            NavTail::Waiting(kind) => Some((*kind).into()),
            NavTail::Failing(Some(count)) => Some(format!("{} {count}", words::FAILING).into()),
            NavTail::Failing(None) => Some(words::FAILING.into()),
            NavTail::Failed => Some(words::FAILED.into()),
            NavTail::Done => Some(words::DONE.into()),
            NavTail::Background => Some(words::BACKGROUND.into()),
            NavTail::Working(elapsed) | NavTail::Age(elapsed) if !elapsed.is_empty() => {
                Some(elapsed.clone())
            }
            NavTail::Working(_) | NavTail::Age(_) | NavTail::None => None,
        }
    }

    /// The word a shown Group's member row says at its right (§5.1.6), and
    /// a Group Pane's head after its title (§5.9): the state words and a
    /// working Thread's elapsed time, nothing for an age or for background
    /// work, with the ink the state takes.
    pub fn member_word(&self) -> Option<(SharedString, u32)> {
        match self {
            NavTail::Waiting(_) => Some((words::NEEDS_YOU.into(), ATTENTION)),
            NavTail::Failed => Some((words::FAILED.into(), BLOCKED)),
            NavTail::Failing(_) => self.text().map(|text| (text, BLOCKED)),
            NavTail::Working(elapsed) if !elapsed.is_empty() => Some((elapsed.clone(), TEXT_DIM)),
            NavTail::Done => Some((words::DONE.into(), TEXT_DIM)),
            _ => None,
        }
    }
}

/// A Thread row's state, for its mark, least urgent first: the order a
/// roll-up takes its `max` in, consistent with `Attention`. The face comes
/// from `cockpit::thread_status`, the one status truth the Panes share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum RowStatus {
    Parked,
    #[default]
    Idle,
    /// The Session closed under it, or its turn failed: a secondary state
    /// at rest, red on its own row, rolled up and ranked as idle.
    Failed,
    /// Main is idle while its Subagents or background tasks still work.
    Background,
    Working,
    /// Working with a red test suite.
    Failing,
    /// A turn finished that the operator has not seen.
    Done,
    /// A Decision waits on the operator.
    NeedsYou,
}

impl RowStatus {
    /// The row's state from the Pane's own reading, whether the Thread
    /// holds an unseen Notice, and whether work runs behind its idle Main.
    /// A finish not yet seen outranks background work, as Done outranks
    /// Working.
    pub fn of(state: WallState, unread: bool, background: bool) -> Self {
        match state {
            WallState::Working => RowStatus::Working,
            WallState::Failing => RowStatus::Failing,
            WallState::Decision => RowStatus::NeedsYou,
            WallState::Blocked => RowStatus::Failed,
            WallState::Parked => RowStatus::Parked,
            WallState::Idle | WallState::Done if unread => RowStatus::Done,
            WallState::Idle | WallState::Done if background => RowStatus::Background,
            WallState::Idle | WallState::Done => RowStatus::Idle,
        }
    }

    /// Which of the four states this is.
    pub fn attention(self) -> Attention {
        match self {
            RowStatus::NeedsYou => Attention::Blocked,
            RowStatus::Done => Attention::Done,
            RowStatus::Working | RowStatus::Failing | RowStatus::Background => Attention::Working,
            RowStatus::Idle | RowStatus::Parked | RowStatus::Failed => Attention::Idle,
        }
    }

    /// The still face: the one status truth's (`thread_status`), and the
    /// hollow running ring of background work.
    pub fn face(self) -> ThreadStatus {
        let wall = match self {
            RowStatus::Background => return ThreadStatus::background(),
            RowStatus::Working => WallState::Working,
            RowStatus::Failing => WallState::Failing,
            RowStatus::NeedsYou => WallState::Decision,
            RowStatus::Failed => WallState::Blocked,
            RowStatus::Done | RowStatus::Idle => WallState::Idle,
            RowStatus::Parked => WallState::Parked,
        };
        thread_status(wall, self == RowStatus::Done)
    }

    /// What a set of rows rolls up to: the most urgent of them. `None` for
    /// no rows.
    pub fn roll_up<'a>(rows: impl IntoIterator<Item = &'a ThreadRow>) -> Option<Self> {
        rows.into_iter().map(|row| row.status).max()
    }
}

/// Where a Thread row hangs: at root (a loose row, a parked row, a view's
/// row), under a Group the board does not show (`Nested`: two lines,
/// indented), or under the shown Group (`Member`: one line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowPlace {
    Root,
    Nested,
    Member,
}

impl RowPlace {
    /// How far in its mark column starts.
    fn mark_x(self) -> f32 {
        match self {
            RowPlace::Root => NAV_ROW_PAD_X,
            RowPlace::Nested => NAV_ROW_PAD_X + NAV_NEST,
            RowPlace::Member => NAV_TITLE_X - 1.0,
        }
    }

    /// Where its title starts.
    fn title_x(self) -> f32 {
        match self {
            RowPlace::Root => NAV_TITLE_X,
            RowPlace::Nested => NAV_TITLE_X + NAV_NEST,
            RowPlace::Member => NAV_MEMBER_X,
        }
    }

    /// The width its lines have, from the title to the row's right padding.
    fn text_w(self) -> f32 {
        INNER_W - self.title_x() - NAV_ROW_PAD_X
    }
}

// ------------------------------------------------------------- measuring

/// The faces' own advances, summed a character at a time as gpui's
/// truncation sums them: what a truncating box is pinned against.
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

    /// `text` in Geist Mono at `size`.
    pub fn code(&self, text: &str, size: f32) -> f32 {
        self.width(FONT_CODE, text, size, W_BODY)
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
                    .map(|ch| {
                        system
                            .advance(id, px(size), ch)
                            .map_or(size * 0.6, |advance| f32::from(advance.width))
                    })
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
                    .min_w(px(width.max(0.0) + TRUNCATE_SLOP))
                    .max_w(px(width.max(0.0) + TRUNCATE_SLOP))
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

/// The nav's one pointer role: nothing at rest, `paint::RAISED` under the
/// pointer reaching full in 80ms, and the selected row `paint::SELECTED`
/// whatever the pointer does (its ring is `selection_ring`). The pointer
/// stays the arrow.
fn wash<E: Styled + InteractiveElement>(mut element: E, key: SharedString, selected: bool) -> E {
    if selected {
        element = element.bg(paint::SELECTED);
    } else {
        let t = motion::hover_t(&key);
        #[cfg(test)]
        testing::DRAWN_HOVER.with(|drawn| drawn.borrow_mut().insert(key.clone(), t));
        if t > 0.0 {
            element = element.bg(motion::mix(
                rgba(TRANSPARENT).into(),
                paint::RAISED.into(),
                t,
            ));
        }
    }
    element
        .interactivity()
        .on_hover(motion::hover_listener_with(key, ROW_HOVER));
    element
}

/// The selected row's inset 1px `ROW_RING`, laid over the row so it moves
/// nothing.
fn selection_ring() -> Div {
    div()
        .absolute()
        .inset_0()
        .rounded(px(R_CONTROL))
        .border_1()
        .border_color(rgba(ROW_RING))
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
            .text_size(px(FS_UI))
            .line_height(px(NAV_LINE)),
        key,
        selected,
    )
    .when(selected, |row| row.child(selection_ring()))
}

/// A one-line row's frame: 30 high, its items centred, `NAV_ROW_PAD_X` in.
fn line_frame(id: impl Into<gpui::ElementId>, key: SharedString, selected: bool) -> Stateful<Div> {
    row_frame(id, key, selected)
        .flex_row()
        .items_center()
        .h(px(NAV_ROW_H))
        .px(px(NAV_ROW_PAD_X))
}

// ------------------------------------------------------------- the marks

/// A row's mark (§6), at its natural size: a working Thread's braille
/// spinner in `RUNNING` (`BLOCKED` while its tests fail) — the one thing in
/// the column that moves — a blocked Thread's `ATTENTION` dot with its
/// halo, a failed one's `BLOCKED` dot, background work's `RUNNING` ring, a
/// finish not yet seen `check-circle`, idle `terminal`, parked `terminal`
/// at half ink. `small` is a member row's: 6px dots, 13px icons.
pub fn mark(status: RowStatus, small: bool) -> AnyElement {
    if small {
        mark_sized(status, NAV_DOT_SMALL, NAV_MARK_ICON_SMALL)
    } else {
        mark_sized(status, NAV_DOT, NAV_MARK_ICON)
    }
}

/// A Group Pane head's mark (SPEC §5.9): the row's face with its 7px dots
/// and 13px icons, laid out at the width the mockup's box takes — a dot
/// keeps a pixel each side and its halo overflows the box, as a CSS
/// `box-shadow` does, so the title starts where it is drawn.
pub fn head_mark(status: RowStatus) -> AnyElement {
    let face = mark_sized(status, NAV_DOT, crate::theme::PANE_HEAD_ICON);
    match status {
        RowStatus::NeedsYou | RowStatus::Failed | RowStatus::Background => div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .w(px(NAV_DOT + 2.0))
            .child(face)
            .into_any_element(),
        _ => div().flex().flex_shrink_0().child(face).into_any_element(),
    }
}

fn mark_sized(status: RowStatus, dot: f32, glyph: f32) -> AnyElement {
    let solid = |ink: u32| div().size(px(dot)).rounded_full().bg(rgb(ink));
    match status {
        RowStatus::Working | RowStatus::Failing => div()
            .font_family(FONT_CODE)
            .text_size(px(NAV_SPINNER_FS))
            .line_height(px(NAV_LINE))
            .child(components::braille_spinner(
                if status == RowStatus::Working {
                    RUNNING
                } else {
                    BLOCKED
                },
            ))
            .into_any_element(),
        RowStatus::NeedsYou => div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(dot + 2.0 * NAV_HALO))
            .rounded_full()
            .bg(rgba(NAV_HALO_FILL))
            .child(solid(ATTENTION))
            .into_any_element(),
        RowStatus::Failed => solid(BLOCKED).into_any_element(),
        RowStatus::Background => div()
            .size(px(dot))
            .rounded_full()
            .border_1()
            .border_color(rgb(RUNNING))
            .into_any_element(),
        RowStatus::Done => icon(icons::CHECK_CIRCLE, glyph, TEXT_DIM).into_any_element(),
        RowStatus::Idle => icon(icons::TERMINAL, glyph, TEXT_DIM).into_any_element(),
        RowStatus::Parked => div()
            .opacity(NAV_PARKED_OPACITY)
            .child(icon(icons::TERMINAL, glyph, TEXT_DIM))
            .into_any_element(),
    }
}

/// A row's mark where the mockups place it, from its mark column's left
/// (`x`) and its row's top: on line 1 of a two-line row, centred on a
/// one-line row.
fn mark_at(status: RowStatus, x: f32, small: bool) -> Div {
    let (left, top) = match (status, small) {
        (RowStatus::Working | RowStatus::Failing, false) => (2.0, 6.0),
        (RowStatus::Working | RowStatus::Failing, true) => (1.0, 5.0),
        (RowStatus::NeedsYou, false) => (4.0 - NAV_HALO, 13.0 - NAV_HALO),
        (RowStatus::NeedsYou, true) => (3.0 - NAV_HALO, 12.0 - NAV_HALO),
        (RowStatus::Failed | RowStatus::Background, false) => (4.0, 13.0),
        (RowStatus::Failed | RowStatus::Background, true) => (3.0, 12.0),
        (_, false) => (0.0, 8.0),
        (_, true) => (0.0, 8.0),
    };
    div()
        .absolute()
        .left(px(x + left))
        .top(px(top))
        .child(mark(status, small))
}

/// A diffstat as the terminal world types it, Geist Mono 11.5: `+N` in
/// `RUNNING`, `−N` in `BLOCKED`, each only when it is not zero.
fn diffstat_text((added, removed): (usize, usize)) -> Vec<(String, u32)> {
    let mut parts = Vec::new();
    if added > 0 {
        parts.push((format!("+{added}"), RUNNING));
    }
    if removed > 0 {
        parts.push((format!("\u{2212}{removed}"), BLOCKED));
    }
    parts
}

fn diffstat_width(diffstat: Option<(usize, usize)>, measure: &Measure) -> f32 {
    let parts = diffstat.map(diffstat_text).unwrap_or_default();
    if parts.is_empty() {
        return 0.0;
    }
    let text = parts
        .iter()
        .map(|(text, _)| text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    measure.code(&text, FS_CHROME_CODE) + NAV_WORD_GAP
}

fn diffstat(stat: (usize, usize), thread: ThreadId) -> Div {
    div()
        .debug_selector(move || format!("nav-diffstat-{}", thread.get()))
        .flex()
        .flex_shrink_0()
        .gap(px(measure_space()))
        .pl(px(NAV_WORD_GAP))
        .font_family(FONT_CODE)
        .text_size(px(FS_CHROME_CODE))
        .line_height(px(NAV_LINE))
        .children(
            diffstat_text(stat)
                .into_iter()
                .map(|(text, ink)| div().text_color(rgb(ink)).child(text)),
        )
}

/// One Geist Mono space at the diffstat's size: between its `+N` and `−N`.
fn measure_space() -> f32 {
    FS_CHROME_CODE * 0.6
}

// ------------------------------------------------------------- line 2

/// A Thread row's line 2 (§5.1.5), in the parts a short row gives up:
/// its lead (the branch, or the state's words), its time (a working
/// Thread's elapsed, a failure's age) and its details (`↑2↓1*`), and the
/// ink the whole line takes.
#[derive(Clone, Debug, PartialEq)]
pub struct LineTwo {
    pub lead: String,
    pub time: Option<String>,
    pub details: Option<String>,
    pub ink: u32,
}

impl LineTwo {
    pub fn of(row: &ThreadRow) -> Self {
        let branch = row.branch.as_deref().unwrap_or_default().to_string();
        let plain = |lead: String| LineTwo {
            lead,
            time: None,
            details: None,
            ink: TEXT_DIM,
        };
        match &row.tail {
            NavTail::Waiting(kind) => LineTwo {
                ink: ATTENTION,
                ..plain(format!("{} \u{b7} {kind}", words::NEEDS_YOU))
            },
            NavTail::Failed => LineTwo {
                time: (!row.age.is_empty()).then(|| row.age.to_string()),
                ink: BLOCKED,
                ..plain(words::FAILED.to_string())
            },
            failing @ NavTail::Failing(_) => plain(failing.text().unwrap_or_default().to_string()),
            NavTail::Working(elapsed) => LineTwo {
                time: (!elapsed.is_empty()).then(|| elapsed.to_string()),
                ..plain(branch)
            },
            _ => LineTwo {
                details: row.details.text().map(|text| text.to_string()),
                ..plain(branch)
            },
        }
    }

    /// The line at its fullest, then without its details, then without
    /// its time: what a row too short for the whole gives up, in that
    /// order. The lead is cut last, with an ellipsis.
    pub fn candidates(&self) -> Vec<String> {
        let join = |time: bool, details: bool| {
            let mut text = self.lead.clone();
            if let Some(time) = self.time.as_ref().filter(|_| time) {
                if !text.is_empty() {
                    text.push_str(" \u{b7} ");
                }
                text.push_str(time);
            }
            if let Some(details) = self.details.as_ref().filter(|_| details) {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(details);
            }
            text
        };
        vec![join(true, true), join(true, false), join(false, false)]
    }

    /// The fullest candidate that fits, else the lead (which its box cuts).
    pub fn fit(&self, fits: impl Fn(&str) -> bool) -> String {
        let candidates = self.candidates();
        candidates
            .iter()
            .find(|text| fits(text))
            .unwrap_or_else(|| candidates.last().expect("three candidates"))
            .clone()
    }
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
        .text_size(px(FS_UI))
        .line_height(px(NAV_LINE))
        .text_color(rgb(TEXT))
}

/// The column's ground: `paint::FRAME`, top to bottom — what the host's
/// traffic lights sit on.
pub fn body() -> Div {
    div()
        .debug_selector(|| "nav-body".into())
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .right_0()
        .bg(paint::FRAME)
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
/// traffic lights sit over), then the sidebar toggle.
pub fn head(drag: impl IntoElement, toggle: impl IntoElement) -> Div {
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
        .child(toggle)
}

/// An icon button of the column (the toggle, the gear, the heading's `+`):
/// `size` square, its glyph `TEXT_DIM`, under the pointer `paint::RAISED`
/// and the glyph `TEXT`, both over 100ms.
pub fn icon_button(
    key: &'static str,
    glyph: &'static str,
    glyph_size: f32,
    size: f32,
    radius: f32,
) -> Stateful<Div> {
    let blend = SharedString::from(key);
    let t = motion::hover_t(&blend);
    div()
        .id(key)
        .debug_selector(move || key.into())
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
        .on_hover(motion::hover_listener_with(blend, DOOR_HOVER))
        .child(icon(glyph, glyph_size, TEXT_DIM).text_color(motion::mix(
            rgb(TEXT_DIM).into(),
            rgb(TEXT).into(),
            t,
        )))
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
        R_TAB,
    )
    .tooltip(crate::menu::action_tooltip(
        "Toggle sidebar",
        "cockpit::ToggleNav",
    ))
}

// ------------------------------------------------------------- the views

/// What a view row says at its right (§5.1.2).
pub enum Trailing {
    None,
    /// A chord, as the keymap binds it (`⌘K`).
    Chord(String),
    /// The needs-you pill, hidden at 0.
    Pill(usize),
    /// A dim count, hidden at 0.
    Count(usize),
    /// Dim words (the Tasks tally).
    Text(SharedString),
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

/// One view row: its 15px icon, its label and its trailing item. At rest
/// `TEXT_MUTED` (`TEXT` while it holds something for the operator), under
/// the pointer `paint::RAISED` and `TEXT`, the active view `SELECTED` and
/// `TEXT_STRONG`.
pub fn view_row(
    key: &'static str,
    glyph: AnyElement,
    label: &'static str,
    tail: Trailing,
    active: bool,
    lit: bool,
) -> Stateful<Div> {
    let blend = SharedString::from(key);
    let t = motion::hover_t(&blend);
    let rest = if lit { TEXT } else { TEXT_MUTED };
    let ink: gpui::Hsla = if active {
        rgb(TEXT_STRONG).into()
    } else {
        motion::mix(rgb(rest).into(), rgb(TEXT).into(), t)
    };
    let end: Option<AnyElement> = match tail {
        Trailing::None => None,
        Trailing::Chord(keys) => Some(
            trailing(chord_glyphs(&keys), FS_META, TEXT_DIM)
                .debug_selector(move || format!("{key}-chord"))
                .into_any_element(),
        ),
        Trailing::Pill(0) | Trailing::Count(0) => None,
        Trailing::Pill(count) => Some(pill(count).into_any_element()),
        Trailing::Count(count) => Some(
            components::tabular(trailing(count.to_string(), FS_META, TEXT_DIM))
                .debug_selector(move || format!("{key}-count"))
                .into_any_element(),
        ),
        Trailing::Text(text) => Some(trailing(text, FS_META, TEXT_DIM).into_any_element()),
    };
    line_frame(key, blend, active)
        .debug_selector(move || key.into())
        .gap(px(NAV_ICON_GAP))
        .text_color(ink)
        .child(div().flex().flex_shrink_0().size(px(NAV_ICON)).child(glyph))
        .child(div().flex_1().min_w_0().truncate().child(label))
        .children(end)
}

/// A view row's icon, in the ink its label takes (an `svg` paints from its
/// own style, never the ambient text colour).
pub fn view_icon(path: &'static str, key: &'static str, active: bool, lit: bool) -> AnyElement {
    let t = motion::hover_t(key);
    let rest = if lit { TEXT } else { TEXT_MUTED };
    let ink: gpui::Hsla = if active {
        rgb(TEXT_STRONG).into()
    } else {
        motion::mix(rgb(rest).into(), rgb(TEXT).into(), t)
    };
    icon(path, NAV_ICON, rest)
        .text_color(ink)
        .into_any_element()
}

/// The needs-you pill: the count in `ATTENTION` 11/600 on `ATTENTION` at
/// 16%, 18 high and at least 18 wide, round.
fn pill(count: usize) -> Div {
    components::tabular(
        div()
            .debug_selector(|| "nav-needs-you-pill".into())
            .flex()
            .flex_shrink_0()
            .justify_center()
            .min_w(px(NAV_PILL))
            .h(px(NAV_PILL))
            .px(px(NAV_PILL_PAD_X))
            .rounded(px(NAV_PILL / 2.0))
            .bg(rgba(NAV_PILL_FILL))
            .text_size(px(FS_BADGE))
            .line_height(px(NAV_PILL))
            .font_weight(W_STRONG)
            .text_color(rgb(ATTENTION))
            .child(count.to_string()),
    )
}

/// A chord as it is drawn, its key a capital: `cmd-k` → `⌘K`.
fn chord_glyphs(keys: &str) -> String {
    components::chord_text(keys).to_uppercase()
}

// ------------------------------------------------------------- headings

/// A section heading (§5.1.3): `Projects`, `Groups`, `Recent`, in 12/500
/// `TEXT_DIM`, with its button (`heading_button`) at the right.
pub fn heading(key: &'static str, label: &'static str) -> Div {
    div()
        .debug_selector(move || key.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(NAV_HEADING_H))
        .pl(px(NAV_ROW_PAD_X))
        .pr(px(NAV_HEADING_PAD_R))
        .text_size(px(FS_META))
        .font_weight(W_LABEL)
        .text_color(rgb(TEXT_DIM))
        .child(div().flex_1().child(label))
}

/// A heading's 22px icon button (`Projects`' `+`).
pub fn heading_button(key: &'static str, glyph: &'static str) -> Stateful<Div> {
    icon_button(key, glyph, NAV_HEADING_GLYPH, NAV_HEADING_BUTTON, R_SMALL)
}

/// A Project heading (§5.1.4): its `folder` icon in `TEXT_DIM`, the label
/// in 13/500 `TEXT`, and at its right the shared branch's drift. Folded, it
/// is one quiet row: the label in `TEXT_MUTED` at 400, and at its right
/// what `ProjectSection::folded_summary` says. The cockpit wires its press
/// to the fold.
pub fn project_section(index: usize, section: &ProjectSection, measure: &Measure) -> Stateful<Div> {
    let key = SharedString::from(format!("nav-project-section-{index}"));
    let (right, right_ink) = if section.folded {
        let (text, ink) = section.folded_summary();
        (Some(text), ink)
    } else {
        (section.drift.clone(), TEXT_DIM)
    };
    let (ink, weight) = if section.folded {
        (TEXT_MUTED, W_BODY)
    } else {
        (TEXT, W_LABEL)
    };
    // The row's gap stands between the title and its right, too.
    let right_w = right
        .as_deref()
        .map_or(0.0, |text| measure.ui(text, FS_META, W_BODY) + NAV_ICON_GAP);
    let title_w = INNER_W - 2.0 * NAV_ROW_PAD_X - NAV_ICON - NAV_ICON_GAP - right_w;
    line_frame(("nav-project-section", index), key, false)
        .debug_selector(move || format!("nav-project-section-{index}"))
        .gap(px(NAV_ICON_GAP))
        .child(icon(icons::FOLDER, NAV_ICON, TEXT_DIM))
        .child(fitted(title_w, FS_UI, ink, weight, section.label.clone()))
        .children(right.map(|right| {
            div()
                .flex()
                .flex_1()
                .justify_end()
                .child(trailing(right, FS_META, right_ink).pl_0())
                .debug_selector(move || format!("nav-project-summary-{index}"))
        }))
}

/// A task's own space's numbers (`8% 16m`) under its heading, dim at the
/// label's edge (ADR 0031).
pub fn space_numbers(numbers: SharedString) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .h(px(NAV_LINE))
        .pl(px(NAV_TITLE_X))
        .text_size(px(FS_META))
        .text_color(rgb(TEXT_DIM))
        .whitespace_nowrap()
        .child(numbers)
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

/// The tree's scrollbar, over the tree it scrolls. See
/// [`components::scrollbar`] for the shape and the sibling rule.
pub fn scrollbar(scroll: &ScrollHandle) -> Div {
    components::scrollbar("nav-scrollbar", scroll)
}

/// One Project section: its heading, then everything under it, 6px below
/// the section above it.
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
/// the title — `TEXT_STRONG` 500 while the board shows the Group — and at
/// the right its roll-up: the worst state's word in its ink, then `· N`
/// members dim. Shown, it takes the selected style and says only `N`: its
/// members say the rest.
#[cfg(test)]
pub fn group_row(row: &GroupBlock) -> Stateful<Div> {
    group_row_with_title(
        row,
        row.title.clone(),
        group_fold(row),
        &Measure::Cells(7.0),
    )
}

/// `group_row` with the title leaf and the chevron supplied by the caller
/// — the cockpit hands in a click-to-rename wrapper, or the live editor
/// while renaming, and wires the chevron's press. The title box is pinned
/// (`fitted`), so the title truncates and renaming never moves the row.
pub fn group_row_with_title(
    row: &GroupBlock,
    title: impl IntoElement,
    fold: Stateful<Div>,
    measure: &Measure,
) -> Stateful<Div> {
    let id = row.id;
    let key = SharedString::from(format!("nav-group-{}", id.get()));
    let count = row.members.len().to_string();
    let word = (!row.selected).then(|| RollUp::of(&row.members)).flatten();
    let count_text = if word.is_some() {
        format!("\u{b7} {count}")
    } else {
        count
    };
    let right_w = word.as_ref().map_or(0.0, |word| {
        measure.ui(&word.text, FS_META, W_BODY) + NAV_WORD_GAP
    }) + measure.ui(&count_text, FS_META, W_BODY)
        + NAV_WORD_GAP;
    let (ink, weight) = if row.selected {
        (TEXT_STRONG, W_LABEL)
    } else {
        (TEXT, W_BODY)
    };
    row_frame(("nav-group", id.get() as usize), key, row.selected)
        .debug_selector(move || format!("nav-group-{}", id.get()))
        .flex_row()
        .items_center()
        .h(px(NAV_ROW_H))
        .pl(px(NAV_TITLE_X))
        .pr(px(NAV_ROW_PAD_X))
        .child(fold)
        .child(fitted(
            RowPlace::Root.text_w() - right_w,
            FS_UI,
            ink,
            weight,
            title,
        ))
        .child(
            div()
                .flex()
                .flex_1()
                .justify_end()
                .children(word.map(|word| trailing(word.text, FS_META, word.ink)))
                .child(
                    components::tabular(trailing(count_text, FS_META, TEXT_DIM))
                        .debug_selector(move || format!("nav-group-count-{}", id.get())),
                ),
        )
}

/// A Group row's chevron, at x 11: `chevron-down` open, `chevron-right`
/// folded (`TEXT_MUTED` on the shown Group, `TEXT_DIM` otherwise). A
/// control of its own — the cockpit wires its press to the fold, which
/// stops there, so the rest of the row still opens the Group.
pub fn group_fold(row: &GroupBlock) -> Stateful<Div> {
    let id = row.id;
    let glyph = if row.folded {
        icons::CHEVRON_RIGHT
    } else {
        icons::CHEVRON_DOWN
    };
    let hit = NAV_ROW_H - 2.0 * SPACE_1;
    div()
        .id(("nav-group-fold", id.get() as usize))
        .debug_selector(move || format!("nav-group-fold-{}", id.get()))
        .absolute()
        .left(px(NAV_CHEVRON_X + NAV_CHEVRON / 2.0 - hit / 2.0))
        .top(px(SPACE_1))
        .size(px(hit))
        .flex()
        .items_center()
        .justify_center()
        .child(icon(
            glyph,
            NAV_CHEVRON,
            if row.selected { TEXT_MUTED } else { TEXT_DIM },
        ))
}

/// The members container: the member rows under their Group.
pub fn members(rows: Vec<AnyElement>) -> Div {
    div().relative().flex().flex_col().children(rows)
}

/// The Thread row at root — see `thread_row_with_title`.
#[cfg(test)]
pub fn thread_row(row: &ThreadRow) -> Stateful<Div> {
    thread_row_with_title(
        row,
        row.name.clone(),
        RowPlace::Root,
        None,
        false,
        &Measure::Cells(7.0),
    )
}

/// A title's ink and weight (§5.1.5): `TEXT`, `TEXT_STRONG` 500 on the
/// selected row, `TEXT_STRONG` while a finish is unseen (the unread rule),
/// and a parked Thread's a step down so what is running reads first.
fn title_face(row: &ThreadRow) -> (u32, FontWeight) {
    if row.selected {
        (TEXT_STRONG, W_LABEL)
    } else if row.status == RowStatus::Done {
        (TEXT_STRONG, W_BODY)
    } else if row.status == RowStatus::Parked {
        (TEXT_MUTED, W_BODY)
    } else {
        (TEXT, W_BODY)
    }
}

/// The one Thread row builder, with the title leaf supplied by the caller —
/// see `group_row_with_title`. At root or nested under a Group the board
/// does not show, two lines (§5.1.5): the mark, the title and the diffstat,
/// then line 2 (`LineTwo`) and the PR's label; a Thread the board runs has
/// its entry's lines under that (`task_lines`). Under the shown Group
/// (`RowPlace::Member`), one line: the mark at 35, the title, and its
/// state's word (`NavTail::member_word`). `group` scopes the row's blend
/// when the Thread is listed under several Groups. `editing` is a rename in
/// progress: the field takes the title's box.
pub fn thread_row_with_title(
    row: &ThreadRow,
    title: impl IntoElement,
    place: RowPlace,
    group: Option<GroupId>,
    editing: bool,
    measure: &Measure,
) -> Stateful<Div> {
    let thread = row.thread;
    let key = SharedString::from(format!(
        "nav-thread-{}-{}",
        group.map_or(0, GroupId::get),
        thread.get()
    ));
    // The shown Group's row carries the selection: its focused member
    // says so in its title's ink alone.
    let filled = row.selected && place != RowPlace::Member;
    let (ink, weight) = match title_face(row) {
        (ink, _) if place == RowPlace::Member => (ink, W_BODY),
        face => face,
    };
    let text_w = place.text_w();
    let frame = row_frame(("nav-thread", thread.get() as usize), key, filled)
        .debug_selector(move || format!("nav-thread-{}", thread.get()))
        .pl(px(place.title_x()))
        .pr(px(NAV_ROW_PAD_X));
    if place == RowPlace::Member {
        let word = (!editing).then(|| row.tail.member_word()).flatten();
        let word_w = word.as_ref().map_or(0.0, |(text, _)| {
            measure.ui(text, FS_META, W_BODY) + NAV_WORD_GAP
        });
        return frame
            .flex_row()
            .items_center()
            .h(px(NAV_ROW_H))
            .child(mark_at(row.status, place.mark_x(), true))
            .child(
                fitted(text_w - word_w, FS_UI, ink, weight, title)
                    .debug_selector(move || format!("nav-title-{}", thread.get())),
            )
            .children(word.map(|(text, ink)| {
                div()
                    .flex()
                    .flex_1()
                    .justify_end()
                    .debug_selector(move || format!("nav-since-{}", thread.get()))
                    .child(trailing(text, FS_META, ink))
            }));
    }
    let stat = row
        .diffstat
        .filter(|_| !editing)
        .filter(|stat| !diffstat_text(*stat).is_empty());
    let stat_w = diffstat_width(stat, measure);
    let line_one = div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(NAV_LINE))
        .child(
            fitted(text_w - stat_w, FS_UI, ink, weight, title)
                .debug_selector(move || format!("nav-title-{}", thread.get())),
        )
        .children(stat.map(|stat| diffstat(stat, thread)));
    let two = LineTwo::of(row);
    let pr = row.details.pr.clone();
    let pr_w = pr
        .as_deref()
        .map_or(0.0, |pr| measure.ui(pr, FS_META, W_BODY) + NAV_WORD_GAP);
    let room = text_w - pr_w;
    let text = two.fit(|text| measure.ui(text, FS_META, W_BODY) <= room);
    let line_ink = if row.selected && two.ink == TEXT_DIM {
        NAV_LINE2_SELECTED
    } else {
        two.ink
    };
    let line_two = div()
        .debug_selector(move || format!("nav-line2-{}", thread.get()))
        .flex()
        .flex_row()
        .items_center()
        .h(px(NAV_LINE))
        .child(fitted(
            room,
            FS_META,
            line_ink,
            W_BODY,
            SharedString::from(text),
        ))
        .children(pr.map(|pr| {
            div()
                .flex()
                .flex_1()
                .justify_end()
                .debug_selector(move || format!("nav-pr-{}", thread.get()))
                .child(trailing(pr, FS_META, line_ink))
        }));
    let extra: Vec<Div> = row
        .task
        .as_ref()
        .map(|task| {
            task_lines(task, |text| measure.ui(text, FS_META, W_BODY) <= text_w)
                .into_iter()
                .map(|(text, ink)| {
                    div()
                        .flex()
                        .h(px(NAV_LINE))
                        .child(fitted(text_w, FS_META, ink, W_BODY, text))
                })
                .collect()
        })
        .unwrap_or_default();
    let height = NAV_ROW2_H + extra.len() as f32 * NAV_LINE;
    frame
        .flex_col()
        .justify_center()
        .h(px(height))
        .child(mark_at(row.status, place.mark_x(), false))
        .child(line_one)
        .child(line_two)
        .children(extra)
}

// ------------------------------------------------------- the board's rows

/// What a task row says under its line 2, each line in its ink (ADR
/// 0031): row 2, its time dropped first when the whole would not `fit`,
/// then row 3. A line with nothing to say is not drawn.
pub fn task_lines(task: &TaskRows, fits: impl Fn(&str) -> bool) -> Vec<(SharedString, u32)> {
    let stands = if fits(&task.stands_timed) {
        &task.stands_timed
    } else {
        &task.stands
    };
    [(stands.clone(), task.ink), (task.third.clone(), TEXT_DIM)]
        .into_iter()
        .filter(|(text, _)| !text.is_empty())
        .collect()
}

// ------------------------------------------------------------- the views' lists

/// A Notice under `Recent` in the Needs-you view (or a request from a
/// machine off screen above it), two lines: its mark, its title
/// (`TEXT_STRONG` while unread) and its age, then its state's word and
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
                NAV_LINE2_SELECTED
            } else {
                TEXT_DIM
            }
        }
    };
    let detail = row.detail.text();
    let line = if detail.is_empty() {
        row.word().to_string()
    } else {
        format!("{} \u{b7} {detail}", row.word())
    };
    let text_w = RowPlace::Root.text_w();
    let age_w = measure.ui(&row.when, FS_META, W_BODY) + NAV_WORD_GAP;
    let title_ink = if cursor || !row.read {
        TEXT_STRONG
    } else {
        TEXT
    };
    row_frame(("notice-row", index), key, cursor)
        .debug_selector(move || format!("notice-row-{index}"))
        .flex_col()
        .justify_center()
        .h(px(NAV_ROW2_H))
        .pl(px(NAV_TITLE_X))
        .pr(px(NAV_ROW_PAD_X))
        .child(mark_at(status, RowPlace::Root.mark_x(), false))
        .child(
            div()
                .flex()
                .items_center()
                .h(px(NAV_LINE))
                .child(fitted(
                    text_w - age_w,
                    FS_UI,
                    title_ink,
                    W_BODY,
                    row.title.clone(),
                ))
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .justify_end()
                        .child(components::tabular(trailing(
                            row.when.clone(),
                            FS_META,
                            TEXT_DIM,
                        ))),
                ),
        )
        .child(div().flex().h(px(NAV_LINE)).child(fitted(
            text_w,
            FS_META,
            ink,
            W_BODY,
            SharedString::from(line),
        )))
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
        .text_color(rgb(TEXT_DIM))
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
/// `TEXT_DIM` and, where there is one, a way forward in the same voice.
/// Filtered to a Project it names the Project rather than shrugging; when
/// the Parked view holds Threads the filter admits, it says *open* and
/// points there. An empty store offers the key that starts one.
pub fn empty_filter(project: Option<&str>, parked: bool) -> Div {
    let message = match (project, parked) {
        (Some(project), _) => format!("No open threads in {project}"),
        (None, false) => "No threads yet".to_string(),
        (None, true) => "No open threads".to_string(),
    };
    let hint: Option<AnyElement> = match (project, parked) {
        (_, true) => Some(
            div()
                .text_color(rgb(TEXT_DIM))
                .child("Parked threads are under Parked")
                .into_any_element(),
        ),
        (None, false) => components::bound_chord("cockpit::NewThread").map(|keys| {
            div()
                .flex()
                .items_center()
                .gap(px(SPACE_1))
                .text_color(rgb(TEXT_DIM))
                .child(chord_glyphs(&keys))
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
        .px(px(NAV_ROW_PAD_X))
        .text_size(px(FS_META))
        .line_height(px(NAV_LINE))
        .child(
            div()
                .text_color(rgb(TEXT_DIM))
                .child(SharedString::from(message)),
        )
        .children(hint)
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
        .text_color(rgb(TEXT_DIM))
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
        Connection::Connecting => ring(TEXT_DIM),
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
                .text_color(rgb(if row.selected { TEXT_STRONG } else { TEXT_DIM }))
                .child(SharedString::from(row.name.clone())),
        )
        .children(word.map(|word| trailing(word, FS_META, TEXT_DIM)))
}

/// A machine heading's chevron: its press folds the machine to its
/// heading.
pub fn machine_fold(index: usize, folded: bool) -> Stateful<Div> {
    div()
        .id(("nav-machine-fold", index))
        .debug_selector(move || format!("nav-machine-fold-{index}"))
        .flex_shrink_0()
        .child(icon(
            if folded {
                icons::CHEVRON_RIGHT
            } else {
                icons::CHEVRON_DOWN
            },
            NAV_CHEVRON,
            TEXT_DIM,
        ))
}

/// One open Thread on a machine the window is not on: its mark, its title
/// and its word, from the status the machine streams. A press goes to the
/// machine. A machine that went keeps its rows, dimmed.
pub fn machine_thread_row(machine: usize, row: &StatusRow, lost: bool) -> Stateful<Div> {
    let (status, word) = if row.waiting {
        (RowStatus::NeedsYou, Some((words::NEEDS_YOU, ATTENTION)))
    } else if row.working {
        (RowStatus::Working, Some((words::WORKING, TEXT_DIM)))
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
        .child(mark_at(status, RowPlace::Root.mark_x() - 1.0, true))
        .child(div().flex_1().min_w_0().truncate().child(title))
        .children(word.map(|(word, ink)| trailing(word, FS_META, ink)))
}

// ------------------------------------------------------------- the foot

/// The foot (§5.1.8): `Ferrite` and the version in 12 `TEXT_DIM`, and
/// the gear.
pub fn foot(version: &str, gear: impl IntoElement) -> Div {
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
                .debug_selector(|| "nav-version".into())
                .flex_1()
                .text_size(px(FS_META))
                .text_color(rgb(TEXT_DIM))
                .child(SharedString::from(format!(
                    "Ferrite {}",
                    short_version(version)
                ))),
        )
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
        R_TAB,
    )
    .tooltip(crate::menu::action_tooltip(
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
/// included), and under it its ⌘ digit in 11 `TEXT_DIM` where a ⌘ key
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
                    .text_color(rgb(if row.selected { TEXT_STRONG } else { TEXT_DIM }))
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
        .text_size(px(FS_UI))
        .line_height(px(NAV_LINE))
        .text_color(rgb(TEXT))
        .bg(paint::FLOAT)
        .rounded(px(R_BLOCK))
        .border_1()
        .border_color(paint::HAIRLINE_STRONG)
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
            branch: Some("main".into()),
            diffstat: None,
            age: "2h".into(),
            details: RowDetails::default(),
            tail: NavTail::Age("2h".into()),
            task: None,
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
            drift: None,
            numbers: None,
            folded,
            roll_up: None,
            count: 2,
            solos: Vec::new(),
            groups: Vec::new(),
        }
    }

    /// The row grammar (§5.1): rounded rows, a Thread two lines at 44 with
    /// its title at 35, a Group and a Project heading one line at 30, a
    /// Project section 6px below the one above it.
    #[test]
    fn nav_rows_are_rounded_two_line_and_one_line() {
        let mut row = thread_row(&thread());
        let style = row.style();
        assert_eq!(
            style.corner_radii.top_left,
            Some(px(R_CONTROL).into()),
            "radius 7"
        );
        assert_eq!(style.size.height, Some(px(NAV_ROW2_H).into()));
        assert_eq!(style.padding.left, Some(px(NAV_TITLE_X).into()));
        assert_eq!(style.padding.right, Some(px(NAV_ROW_PAD_X).into()));
        let mut row = group_row(&group(false, false));
        assert_eq!(row.style().size.height, Some(px(NAV_ROW_H).into()));
        let mut later = section(false);
        assert_eq!(
            later.style().margin.top,
            Some(px(NAV_SECTION_GAP).into()),
            "6px above every Project section but the first"
        );
        assert_eq!(section(true).style().margin.top, None);
        assert_eq!((NAV_ROW_H, NAV_ROW2_H, NAV_SECTION_GAP), (30.0, 44.0, 6.0));
        assert_eq!(WIDTH, 264.0, "the mockups' 264px column");
        assert_eq!(INNER_W, 248.0, "padding 0 8 8");
        assert_eq!(RowPlace::Root.text_w(), 203.0);
    }

    /// N-5, the selection rule: one selection, on the row of what the board
    /// shows — `SELECTED`, never a coloured outline.
    #[test]
    fn one_selection_marks_what_the_board_shows() {
        let fill = |mut drawn: Stateful<Div>| drawn.style().background.clone();
        let selection = Some(gpui::Hsla::from(paint::SELECTED).into());
        assert_eq!(fill(group_row(&group(true, false))), selection);
        assert_eq!(fill(group_row(&group(false, false))), None);
        assert_eq!(
            fill(thread_row_with_title(
                &thread(),
                "thread-08",
                RowPlace::Member,
                Some(GroupId::new(1)),
                false,
                &Measure::Cells(7.0)
            )),
            None,
            "an unfocused member carries no fill"
        );
        assert_eq!(fill(thread_row(&selected_thread(true))), selection);
        assert_eq!(fill(thread_row(&thread())), None);
    }

    /// §5.1.5: a title is `TEXT`, `TEXT_STRONG` 500 when selected,
    /// `TEXT_STRONG` while its finish is unseen, a step down when parked.
    #[test]
    fn titles_follow_selection_unread_and_parked() {
        assert_eq!(title_face(&thread()), (TEXT, W_BODY));
        assert_eq!(title_face(&selected_thread(true)), (TEXT_STRONG, W_LABEL));
        let done = ThreadRow {
            status: RowStatus::Done,
            ..thread()
        };
        assert_eq!(title_face(&done), (TEXT_STRONG, W_BODY));
        let parked = ThreadRow {
            status: RowStatus::Parked,
            ..thread()
        };
        assert_eq!(title_face(&parked), (TEXT_MUTED, W_BODY));
    }

    /// N-13: the pointer stays the arrow over every row.
    #[test]
    fn every_nav_row_keeps_the_arrow() {
        let cursor = |mut drawn: Stateful<Div>| drawn.style().mouse_cursor;
        assert_eq!(cursor(thread_row(&thread())), None);
        assert_eq!(cursor(group_row(&group(false, true))), None);
        assert_eq!(
            cursor(project_section(0, &heading_of(false), &Measure::Cells(7.0))),
            None
        );
    }

    /// Line 2 by state (§5.1.5): what a blocked row waits for in
    /// `ATTENTION`, a failure and its age in `BLOCKED`, red tests, a
    /// working row's branch and elapsed, otherwise the branch and its
    /// details.
    #[test]
    fn line_two_says_where_a_row_is_or_why_it_waits() {
        let with = |tail: NavTail| LineTwo::of(&ThreadRow { tail, ..thread() });
        let waiting = with(NavTail::Waiting(words::APPROVAL));
        assert_eq!(waiting.candidates()[0], "needs you \u{b7} approval");
        assert_eq!(waiting.ink, ATTENTION);
        let failed = with(NavTail::Failed);
        assert_eq!(failed.candidates()[0], "failed \u{b7} 2h");
        assert_eq!(failed.ink, BLOCKED);
        assert_eq!(with(NavTail::Failing(Some(3))).candidates()[0], "failing 3");
        let working = with(NavTail::Working("5m22s".into()));
        assert_eq!(working.candidates()[0], "main \u{b7} 5m22s");
        assert_eq!(working.ink, TEXT_DIM);
        let idle = LineTwo::of(&ThreadRow {
            details: RowDetails {
                ahead: 2,
                behind: 1,
                dirty: true,
                pr: Some("#12 \u{2713}".into()),
            },
            ..thread()
        });
        assert_eq!(idle.candidates()[0], "main \u{2191}2\u{2193}1*");
        assert_eq!(idle.candidates()[1], "main", "the details go first");
    }

    /// Truncation order (§5.1.5): the details go, then the time, then the
    /// branch is cut — the PR's label stays.
    #[test]
    fn a_short_line_two_drops_details_then_time() {
        let two = LineTwo {
            lead: "scheduled-work-crash".into(),
            time: Some("5m22s".into()),
            details: Some("\u{2191}2*".into()),
            ink: TEXT_DIM,
        };
        let fits = |room: usize| move |text: &str| text.chars().count() <= room;
        assert_eq!(
            two.fit(fits(40)),
            "scheduled-work-crash \u{b7} 5m22s \u{2191}2*"
        );
        assert_eq!(two.fit(fits(28)), "scheduled-work-crash \u{b7} 5m22s");
        assert_eq!(two.fit(fits(22)), "scheduled-work-crash");
        assert_eq!(two.fit(fits(4)), "scheduled-work-crash", "its box cuts it");
    }

    /// Every Thread row is the one 44px pair of lines whatever arrives: a
    /// word, a mark, its checkout's facts. A task row adds its lines.
    #[test]
    fn every_row_keeps_its_height_whatever_its_facts() {
        let height = |mut drawn: Stateful<Div>| drawn.style().size.height;
        let two = Some(px(NAV_ROW2_H).into());
        let bare = ThreadRow {
            branch: None,
            tail: NavTail::None,
            ..thread()
        };
        assert_eq!(height(thread_row(&bare)), two);
        for tail in [
            NavTail::Waiting(words::APPROVAL),
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
                two,
                "{tail:?}"
            );
        }
        for status in [RowStatus::Working, RowStatus::Done, RowStatus::Parked] {
            assert_eq!(
                height(thread_row(&ThreadRow {
                    status,
                    diffstat: Some((4, 1)),
                    ..bare.clone()
                })),
                two,
                "{status:?}"
            );
        }
        let task = ThreadRow {
            task: Some(TaskRows {
                stands_timed: "agent \u{b7} working \u{b7} 6m".into(),
                stands: "agent \u{b7} working".into(),
                ink: YELLOW,
                third: "8% 16m".into(),
            }),
            ..bare.clone()
        };
        assert_eq!(
            height(thread_row(&task)),
            Some(px(NAV_ROW2_H + 2.0 * NAV_LINE).into())
        );
        let mut member = thread_row_with_title(
            &bare,
            "thread-08",
            RowPlace::Member,
            None,
            false,
            &Measure::Cells(7.0),
        );
        assert_eq!(member.style().size.height, Some(px(NAV_ROW_H).into()));
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
    /// roll-up word, its drift, a branch off the default, its count.
    #[test]
    fn a_folded_heading_says_the_first_thing_it_has() {
        let mut section = heading_of(true);
        assert_eq!(section.folded_summary(), ("2".into(), TEXT_DIM));
        section.branch = Some(("main".into(), true));
        assert_eq!(section.folded_summary().0, "2", "the default says nothing");
        section.branch = Some(("overnight/coach".into(), false));
        assert_eq!(section.folded_summary().0, "overnight/coach");
        section.drift = drift_text(715, 0);
        assert_eq!(section.folded_summary().0, "\u{2191}715");
        section.roll_up = Some(RollUp {
            text: "1 failed".into(),
            ink: BLOCKED,
        });
        assert_eq!(section.folded_summary(), ("1 failed".into(), BLOCKED));
        assert_eq!(drift_text(1, 5).as_deref(), Some("\u{2191}1 \u{2193}5"));
        assert_eq!(drift_text(0, 0), None);
    }

    /// The word says one thing, in priority order, and the Pane's head slot
    /// decides it.
    #[test]
    fn the_word_is_one_fact_in_priority_order() {
        use crate::pane::HeadSlot;
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
            Some(("1m".into(), TEXT_DIM))
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

    /// A row's checkout facts are terse and fixed in order — drift, then
    /// dirt — and a clean, even checkout says nothing.
    #[test]
    fn the_checkout_facts_are_terse() {
        let details = |ahead, behind, dirty| RowDetails {
            ahead,
            behind,
            dirty,
            pr: None,
        };
        assert_eq!(details(0, 0, false).text(), None);
        assert_eq!(
            details(2, 1, true).text(),
            Some("\u{2191}2\u{2193}1*".into())
        );
        assert_eq!(details(0, 3, false).text(), Some("\u{2193}3".into()));
    }

    /// A task row's entry (ADR 0031): row 2 in its task's ink, its time
    /// the first thing a short row drops, then row 3 dim.
    #[test]
    fn a_task_rows_entry_drops_its_time_before_it_cuts_where_it_stands() {
        let task = TaskRows {
            stands_timed: "agent \u{b7} working \u{b7} 6m".into(),
            stands: "agent \u{b7} working".into(),
            ink: YELLOW,
            third: "8% 16m".into(),
        };
        let lines = |room: usize| -> Vec<(String, u32)> {
            task_lines(&task, |text| text.chars().count() <= room)
                .into_iter()
                .map(|(text, ink)| (text.to_string(), ink))
                .collect()
        };
        assert_eq!(
            lines(30),
            [
                ("agent \u{b7} working \u{b7} 6m".to_string(), YELLOW),
                ("8% 16m".to_string(), TEXT_DIM)
            ]
        );
        assert_eq!(lines(16)[0].0, "agent \u{b7} working");
    }

    /// The marks (§6): a spinner for work, the blocked dot's halo, a red
    /// dot for a failure, a ring for background work, icons at rest.
    #[test]
    fn status_marks_say_state_and_green_only_means_live() {
        let ink = |status: RowStatus| status.face().dot().style().text.color;
        assert_eq!(ink(RowStatus::Working), Some(rgb(RUNNING).into()));
        assert_eq!(ink(RowStatus::Background), Some(rgb(RUNNING).into()));
        assert_eq!(ink(RowStatus::Failing), Some(rgb(BLOCKED).into()));
        assert_eq!(ink(RowStatus::Failed), Some(rgb(BLOCKED).into()));
        assert_eq!(ink(RowStatus::NeedsYou), Some(rgb(ATTENTION).into()));
        assert_eq!(ink(RowStatus::Done), Some(rgb(TEXT_DIM).into()));
        assert_eq!(ink(RowStatus::Idle), Some(rgb(IDLE).into()));
        let mut halo = mark_at(RowStatus::NeedsYou, NAV_ROW_PAD_X, false);
        assert_eq!(
            halo.style().inset.left,
            Some(px(NAV_ROW_PAD_X + 4.0 - NAV_HALO).into()),
            "the dot at 14, its halo 3px around it"
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

    /// One status truth: for every Pane state and either side of unread,
    /// the nav row's still face is the Pane's own.
    #[test]
    fn the_nav_mark_is_the_panes_mark() {
        use WallState::*;
        let paint = |mut dot: Div| dot.style().text.color;
        for state in [Working, Failing, Decision, Blocked, Done, Idle] {
            for unread in [false, true] {
                let status = RowStatus::of(state, unread, false);
                assert_eq!(
                    paint(status.face().dot()),
                    paint(crate::pane::cell_dot(state, unread)),
                    "{state:?} unread={unread}"
                );
            }
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
            Some(gpui::Hsla::from(paint::FRAME).into())
        );
        let mut inner = content();
        let style = inner.style();
        assert_eq!(style.padding.left, Some(px(NAV_COL_PAD).into()));
        assert_eq!(style.padding.bottom, Some(px(NAV_COL_PAD).into()));
        assert_eq!(style.padding.top, None, "the head is in the head row");
        assert_eq!(
            head(div(), div()).style().size.height,
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
            Some(gpui::Hsla::from(paint::SELECTED).into())
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

    /// The foot names the app and its major and minor version.
    #[test]
    fn the_foot_says_ferrite_and_its_version() {
        assert_eq!(short_version("0.5.0"), "0.5");
        assert_eq!(short_version("1.12.3-beta"), "1.12");
        assert_eq!(short_branch("dev").as_ref(), "dev");
        let long = short_branch("worktree-pay-api-migration-cleanup");
        assert_eq!(long.chars().count(), NAV_BRANCH_MAX_CH);
        assert!(long.ends_with('\u{2026}'));
    }

    /// The nav's own clocks: the row wash reaches full in 80ms, an icon
    /// button in 100ms and the content fades over 150ms, each on CSS
    /// `ease`.
    #[test]
    fn the_nav_keeps_its_clocks() {
        assert_eq!(ROW_HOVER.duration_ms, 80);
        assert_eq!(DOOR_HOVER.duration_ms, 100);
        assert_eq!(CONTENT_FADE.duration_ms, 150);
        for spec in [ROW_HOVER, DOOR_HOVER, CONTENT_FADE] {
            assert_eq!(spec.curve, motion::EASE);
        }
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
