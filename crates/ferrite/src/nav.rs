//! The left navigation column (#21): one section per Project — its loose
//! Threads and its Groups, each Group's members hanging under it — and, at
//! the foot of the column, the **Parked** section: every parked Thread no
//! Group claims, folded shut by default so the tree above holds only what
//! is running. It is a view, never the only door: everything a row does
//! (focus, revive, regroup) stays reachable from the keyboard and the ⌘K
//! palette, which also holds what the column no longer draws — the Project
//! filter, the sort, a new Project, a new Thread in a Project.
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
//! `TEXT_STRONG` on the selected row), a 2ch mark cell (the status dot, a
//! working Thread's braille spinner, a faint disclosure triangle), the
//! title, its checkout's facts (`RowDetails`) and one dim word at the
//! right. A Group's members hang under it on faint tree glyphs (`├ ` /
//! `└ `). There is no subtitle line, no provider logo, no tooltip and no
//! keyboard-hint row: a row's other facts live in its Pane head and in the
//! palette's context column. Every heading — a Project, `groups`, `parked`,
//! a smart section — is one style: a lowercase label in `CHROME_MUTED`.
//!
//! Colour is state, carried by the mark alone (`cockpit::Attention`: a
//! Decision's yellow, a failure's red, a finish not yet seen the teal `✓`,
//! work the green spinner or ring); every right-hand word is
//! `CHROME_MUTED`. A Project heading and a Group row carry the roll-up of
//! their rows — the most urgent one's mark — so a folded section still
//! says what needs the operator, and `sort: attention` ranks by it:
//! sections, the Groups in a section, the rows in each. The selection is
//! `paint::SELECTION` on the row of what the board shows — the Group being
//! viewed, or the Solo Thread — with the `❯` and a `TEXT_STRONG` title;
//! hover is `paint::HOVER`, reaching full in 80ms, and the pointer stays
//! the arrow. One thing moves: a working Thread's spinner. Nothing pulses.
//!
//! **Folding** (cmd-B) rides the column down to the **rail** while its
//! content fades on its own clock: one line per open Thread in the tree's
//! order, its mark and, where ⌘1–9 reaches it, that digit (`rail`), so a
//! folded nav still says what needs the operator and how to get there.
//! The sidebar toggle and the bell wait in the bottom bar (ADR 0014).
//!
//! Every colour and metric is a `theme` token; this file holds no literal
//! of its own.

use ferrite_core::groups::GroupId;
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::ThreadId;

use gpui::prelude::*;
use gpui::{
    div, px, relative, rgb, rgba, AnyElement, Div, FontWeight, ScrollHandle, SharedString, Stateful,
};

use crate::cockpit::{thread_status, Attention, ThreadStatus};
use crate::components;
use crate::icons::{self, icon};
use crate::machines::{Connection, MachineRow};
use crate::motion::{self, MotionSpec};
use crate::pane::WallState;
use crate::theme::*;
use ferrite_core::cockpit::StatusRow;

/// The nav's two widths — 36ch open, and the 6ch rail folded (cmd-B).
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
/// The sidebar toggle's hover, ground and glyph (`.ib`'s `.1s`).
pub(crate) const DOOR_HOVER: MotionSpec = MotionSpec::new(MOTION_NAV_DOOR_MS, motion::EASE);
/// The column's content fading as it folds or opens (`.nav > *`'s `.15s
/// ease`), on its own clock apart from the width's ride.
pub(crate) const CONTENT_FADE: MotionSpec = MotionSpec::new(MOTION_NAV_FADE_MS, motion::EASE);
/// What a triangle in the nav folds (N-4): a Project section shut to its
/// heading, a Group shut to its row (its count kept), or the Parked section
/// shut to its header. Held in memory by the cockpit
/// (`CockpitView::set_nav_fold`), so a fold survives every re-render and
/// view switch; opening a folded Group never unfolds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum NavFold {
    Project(ProjectId),
    /// The Groups section: the Groups that span Projects.
    Groups,
    Group(GroupId),
    Parked,
}

/// What the nav draws this frame: the Project sections and the Parked
/// section. Nothing here is a store read — the cockpit assembles it from
/// O(1) reads plus its project/branch caches.
pub struct NavState {
    /// Smart filtering's sections over the tree, or `None` while it is off.
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
    /// folded section or Group draws none. The Parked section is not the
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

/// One Project's run of the tree: its heading (a label like every nav
/// heading, its roll-up mark, its branch dim at the right), its loose rows,
/// then its Groups — each Group listed under the one Project its members
/// share. A Group spanning Projects belongs to none of them: it is listed
/// in the Groups section (`mixed`), first in the tree unless `sort:
/// attention` ranks it.
pub struct ProjectSection {
    pub project: Option<ProjectId>,
    /// The Groups section: no Project, no loose rows, only Groups whose
    /// members span Projects.
    pub mixed: bool,
    /// Lowercase: the one heading style.
    pub label: SharedString,
    /// The branch the heading names at its right: the checkout every row
    /// shares, else the Project's default. `None` says nothing.
    pub branch: Option<SharedString>,
    /// A task's own space: its row's numbers (`8% 16m`), under the heading
    /// (ADR 0031).
    pub numbers: Option<SharedString>,
    /// The heading's triangle folded it: it draws `▸` and nothing under it.
    pub folded: bool,
    /// The most urgent of its rows (`RowStatus::roll_up`), which its
    /// heading's mark says — folded or not.
    pub status: RowStatus,
    /// Indices into `NavState::solos`.
    pub solos: Vec<usize>,
    /// Indices into `NavState::groups`.
    pub groups: Vec<usize>,
}

/// Smart filtering (`Settings::smart_filtering`): two sections over the
/// Project tree, a rule between them and it. Each row is a reference to
/// the Thread — its own row stays in the tree.
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
    /// Its triangle folded it: `▸`, the members hidden, the count kept.
    pub folded: bool,
    /// The board shows this Group (its view, or its wall): the row carries
    /// the tree's one selection, its `❯` and a `TEXT_STRONG` title.
    pub selected: bool,
    /// The most urgent of its members (`RowStatus::roll_up`), which its
    /// row's mark says — folded or not.
    pub status: RowStatus,
}

/// One Thread's row — identical whether it is a Group member, a loose
/// Thread or a parked one; only the container differs. One line: cursor,
/// status mark, title, details, word.
#[derive(Clone)]
pub struct ThreadRow {
    pub thread: ThreadId,
    pub name: SharedString,
    /// What the Thread is doing right now — the one glance the operator
    /// asked for from the tree: which agents are working, which wait,
    /// which finished unseen.
    pub status: RowStatus,
    /// The Project's name, from the facts cache: what a Project section is
    /// headed with.
    pub project: Option<SharedString>,
    /// The board shows this Thread alone (Solo): the row carries the
    /// tree's one selection, its `❯` and the `TEXT_STRONG` title. A Group
    /// member on a Group's board is not selected — its Group row is.
    pub selected: bool,
    /// Its checkout and its PR at a glance (`RowDetails`), dim before the
    /// word; empty says nothing.
    pub details: RowDetails,
    /// The one word at the row's right (C10).
    pub tail: NavTail,
    /// The board's words for the Thread's agent, under its row (ADR 0031):
    /// a task's agent, its reviewer, the Coordinator. Its row 1 is `name`,
    /// the task's number and title, with no word at the right.
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

    /// The facts as the row types them, terse and in a fixed order:
    /// `↑2↓1` drift, `*` a dirty tree, then the PR's `#123 ✓`. `None` when
    /// there is nothing to say.
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
        if let Some(pr) = &self.pr {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(pr);
        }
        (!text.is_empty()).then(|| text.into())
    }
}

/// What a row's word says (C10), in priority order: a pending Decision
/// names what it waits for (`approval` / `plan` / `question`); red tests
/// `failing`/`failing N` and a failed turn or closed Session `failed`; a
/// working Thread its live elapsed time (`1m04s`, `12s`, never coarsened);
/// a finish not yet seen `done`; work still running behind an idle Main
/// `background`; otherwise the age, once it reaches a minute
/// (`facts::since_label` says nothing before that). Never `now`. Every one
/// is `CHROME_MUTED`: the mark carries the colour (the prototype's `.nrow
/// .s` outranks its `.warn`/`.bad` classes, so its words read dim on
/// screen).
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

    /// The word is a time — a working Thread's elapsed or the age — which
    /// a short row drops before its title is cut.
    pub fn timed(&self) -> bool {
        matches!(self, NavTail::Working(_) | NavTail::Age(_))
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

    /// What a set of rows rolls up to — a Project's or a Group's heading:
    /// the most urgent of them. `None` for no rows.
    pub fn roll_up<'a>(rows: impl IntoIterator<Item = &'a ThreadRow>) -> Option<Self> {
        rows.into_iter().map(|row| row.status).max()
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

/// The cursor cell: `❯` in `TEXT_STRONG` on the selected row, blank otherwise.
fn cursor_cell(selected: bool) -> Div {
    if selected {
        cell(components::prompt_mark(TEXT_STRONG))
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
        .text_color(rgb(CHROME_FAINT))
        .child(if last { "\u{2514}" } else { "\u{251c}" })
}

/// A member's tree lead as the prototype types it, one run: `  ├ `, or
/// `  └ ` closing the Group, in the faint structure ink.
fn tree_lead(last: bool) -> Div {
    components::glyph(
        if last { "  \u{2514} " } else { "  \u{251c} " },
        CHROME_FAINT,
    )
    .whitespace_nowrap()
    .h(px(NAV_LINE))
}

/// The disclosure triangle and its space, typed as the prototype's `.tr`
/// types them — `▾ ` open, `▸ ` shut, in the faint structure ink, the
/// triangle in the face the platform falls back to — the run's own width
/// before the title.
fn disclosure(open: bool) -> Div {
    components::glyph(if open { "\u{25be} " } else { "\u{25b8} " }, CHROME_FAINT)
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
        .text_color(rgb(CHROME_MUTED))
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
        #[cfg(test)]
        testing::DRAWN_HOVER.with(|drawn| drawn.borrow_mut().insert(key.clone(), t));
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

/// The status mark in a Thread row's mark cell (`RowStatus::face`, the
/// one truth the Panes share): a working Thread's braille spinner in
/// `RUNNING` — the one thing in the column that moves, still under reduced
/// motion — and every other state its still face: a failing or failed
/// Thread's red dot, a Decision's yellow, a finish not yet seen the teal
/// check, background work the hollow running ring, idle the metadata ink,
/// and a parked Thread a faint ring.
fn status_mark(status: RowStatus) -> AnyElement {
    match status {
        RowStatus::Working => components::braille_spinner(RUNNING),
        RowStatus::Parked => components::status_ring(CHROME_FAINT).into_any_element(),
        status => status.face().dot().into_any_element(),
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

/// The column's ground: `paint::CHROME`, top to bottom. The titlebar band
/// over the column draws nothing (ADR 0014), so the column's own chrome is
/// what the host's traffic lights sit on.
pub fn body() -> Div {
    div()
        .debug_selector(|| "nav-body".into())
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .right_0()
        .bg(paint::CHROME)
}

/// The column's content at its own width, under the titlebar band (the
/// traffic lights' row) and a half row of air, a half row clear of the
/// foot.
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

/// The sidebar toggle (the prototype's `#b-side`): the sidebar glyph at
/// `ICON_BUTTON_GLYPH` in a door `ICON_BUTTON` wide at the bottom bar's
/// left (`titlebar::bottom_bar`), the full height of the bar — the same
/// door open or folded. Under the pointer the ground takes `paint::HOVER`
/// and the glyph `TEXT`, both over 100ms (`.ib:hover`).
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
        .h_full()
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
            icon(icons::SIDEBAR, ICON_BUTTON_GLYPH, CHROME_MUTED).text_color(motion::mix(
                rgb(CHROME_MUTED).into(),
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
/// shows this Group), its triangle (`group_fold`), its members' roll-up
/// mark (`roll_up_cell`), the title in body ink (`TEXT_STRONG` when
/// selected), and how many members it holds, dim, at the right — the same
/// count folded or open. No logo and no word of its own: the member that
/// waits says why.
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
        .child(roll_up_cell(row.status))
        .child(fitted(
            title_w(3.0, Some(&count)),
            title_ink(row.selected, false),
            W_BODY,
            title,
        ))
        .child(
            word_cell(Some(count)).debug_selector(move || format!("nav-group-count-{}", id.get())),
        )
}

/// A heading's roll-up mark (`RowStatus::roll_up`), under its rows' own
/// marks: the most urgent row's still face, and nothing while all are
/// idle — the cell keeps its place so a heading never reflows.
fn roll_up_cell(status: RowStatus) -> Div {
    if status.attention() == Attention::Idle {
        blank_cell()
    } else {
        cell(status.face().dot())
    }
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
/// leaves it, and at the right its checkout's facts and its word. `group`
/// scopes the row's blend when the Thread is listed under several Groups.
/// `editing` is a rename in progress: the field takes the title's box and
/// the word's. A Thread the board runs has its entry's rows under that line
/// (`task_lines`), the whole entry one row to the pointer.
pub fn thread_row_with_title(
    row: &ThreadRow,
    title: impl IntoElement,
    place: RowPlace,
    group: Option<GroupId>,
    editing: bool,
) -> Stateful<Div> {
    let thread = row.thread;
    let cells = match place {
        RowPlace::Root => 2.0,
        RowPlace::Member { .. } => 3.0,
    };
    let word = (!editing).then(|| row_words(row, cells)).flatten();
    let key = SharedString::from(format!(
        "nav-thread-{}-{}",
        group.map_or(0, GroupId::get),
        thread.get()
    ));
    // A member's lead is the prototype's typed `  ├ ` (one run); the
    // cursor takes its first two cells when the row is selected.
    let head = match place {
        RowPlace::Root => line().child(cursor_cell(row.selected)),
        RowPlace::Member { last } if row.selected => {
            line().child(cursor_cell(true)).child(tree_cell(last))
        }
        RowPlace::Member { last } => line().child(tree_lead(last)),
    };
    let head = head
        .child(cell(status_mark(row.status)))
        .child(
            fitted(
                title_w(cells, word.as_deref()),
                title_ink(row.selected, row.status == RowStatus::Parked),
                W_BODY,
                title,
            )
            .debug_selector(move || format!("nav-title-{}", thread.get())),
        )
        .children((!editing).then(|| {
            word_cell(word).debug_selector(move || format!("nav-since-{}", thread.get()))
        }));
    // A task row is its entry's lines, one row under the pointer.
    let frame = match &row.task {
        None => head,
        Some(task) => div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(head)
            .children(
                task_lines(task, cells)
                    .into_iter()
                    .map(|(text, ink)| task_line(text, ink, cells)),
            ),
    };
    wash(
        frame.id(("nav-thread", thread.get() as usize)),
        key,
        row.selected,
    )
    .debug_selector(move || format!("nav-thread-{}", thread.get()))
}

/// What a tree row says at its right, beside a title in `cells` mark
/// cells: its checkout's facts, then its word — as much as leaves the
/// title whole. The title gives way last: a time (`NavTail::timed`) goes
/// first, as soon as the title would be cut; then, only while the title
/// would keep less than `NAV_TITLE_MIN_CH`, the word, the drift, and the
/// PR's label, in that order.
fn row_words(row: &ThreadRow, cells: f32) -> Option<SharedString> {
    let join = |details: Option<SharedString>, word: Option<SharedString>| match (details, word) {
        (Some(details), Some(word)) => Some(SharedString::from(format!("{details} {word}"))),
        (details, word) => details.or(word),
    };
    let word = row.tail.text();
    let full = join(row.details.text(), word.clone());
    let title = row.name.chars().count() as f32 * CH;
    if title <= title_w(cells, full.as_deref()) {
        return full;
    }
    let untimed = (!row.tail.timed()).then_some(word).flatten();
    let pr = RowDetails {
        pr: row.details.pr.clone(),
        ..RowDetails::default()
    };
    [
        join(row.details.text(), untimed),
        row.details.text(),
        pr.text(),
    ]
    .into_iter()
    .find(|words| title_w(cells, words.as_deref()) >= NAV_TITLE_MIN_CH * CH)
    .flatten()
}

// ------------------------------------------------------- smart filtering

/// A smart section's header: its word and count, dim (`done 2`,
/// `working 3`), the one heading style.
pub fn smart_header(word: &'static str, count: usize) -> Stateful<Div> {
    let key = SharedString::from(format!("nav-smart-{word}"));
    row_frame(("nav-smart", word.len()), key.clone(), false)
        .debug_selector(move || key.to_string())
        .text_color(rgb(CHROME_MUTED))
        .child(SharedString::from(format!("{word} {count}")))
}

/// One row of a smart section: the Thread's own mark, its title and its
/// word (`done`, or the live elapsed time). A reference to the Thread's
/// own row: no cursor cell and no selection.
pub fn smart_row(row: &ThreadRow, section: &'static str) -> Stateful<Div> {
    let thread = row.thread;
    let word = row.tail.text();
    let key = SharedString::from(format!("nav-smart-{section}-{}", thread.get()));
    row_frame(("nav-smart-row", thread.get() as usize), key.clone(), false)
        .debug_selector(move || key.to_string())
        .child(cell(status_mark(row.status)))
        .child(fitted(
            title_w(1.0, word.as_deref()),
            TEXT,
            W_BODY,
            row.name.clone(),
        ))
        .child(word_cell(word))
}

/// The rule between the smart sections and the Project tree: one seam-wide
/// line in the seam ink, a half row clear on either side.
pub fn smart_rule() -> Div {
    div()
        .debug_selector(|| "nav-smart-rule".into())
        .flex_shrink_0()
        .h(px(CHROME_SEAM_W))
        .mx(px(NAV_PAD_X))
        .my(px(NAV_SECTION_GAP))
        .bg(paint::CHROME_SEAM)
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
        CHROME_MUTED
    } else {
        TEXT
    }
}

// ------------------------------------------------------- the board's rows

/// What a task row says under its row, at its title's edge `cells` in,
/// each line in its ink (ADR 0031): row 2, its time dropped first when the
/// whole would be cut (the rule `row_words` keeps), then row 3. A line
/// with nothing to say is not drawn.
pub fn task_lines(task: &TaskRows, cells: f32) -> Vec<(SharedString, u32)> {
    let whole = task.stands_timed.chars().count() as f32 * CH <= title_w(cells, None);
    let stands = if whole {
        &task.stands_timed
    } else {
        &task.stands
    };
    [
        (stands.clone(), task.ink),
        (task.third.clone(), CHROME_MUTED),
    ]
    .into_iter()
    .filter(|(text, _)| !text.is_empty())
    .collect()
}

/// One line under a task row, at its title's edge, cut with `…` where it
/// is long.
fn task_line(text: SharedString, ink: u32, cells: f32) -> Div {
    line().pl(px(NAV_PAD_X + cells * NAV_CELL)).child(fitted(
        title_w(cells, None),
        ink,
        W_BODY,
        text,
    ))
}

/// A Project heading (or the `groups` heading), one line: its triangle
/// where a row's cursor sits (`▾` open, `▸` folded), its rows' roll-up mark
/// where theirs sit, the label — lowercase in `CHROME_MUTED`, the one
/// heading style — and its branch, dim, at the right, still there under
/// the pointer. A plain row: it washes like any other, and the cockpit
/// wires its press to the fold. A new Thread in this Project is a palette
/// command. A task's own space has its numbers under it
/// (`space_numbers`).
pub fn project_section(index: usize, section: &ProjectSection) -> Stateful<Div> {
    let branch = section.branch.as_deref().map(short_branch);
    let key = SharedString::from(format!("nav-project-section-{index}"));
    row_frame(("nav-project-section", index), key, false)
        .debug_selector(move || format!("nav-project-section-{index}"))
        .child(disclosure(!section.folded))
        .child(roll_up_cell(section.status))
        .child(fitted(
            title_w(2.0, branch.as_deref()),
            CHROME_MUTED,
            W_BODY,
            section.label.clone(),
        ))
        .child(word_cell(branch))
}

/// A task's own space's numbers (`8% 16m`) under its heading, dim at the
/// label's edge, as herdr's sidebar has them under the space's name (ADR
/// 0031).
pub fn space_numbers(numbers: SharedString) -> Div {
    task_line(numbers, CHROME_MUTED, 2.0)
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
/// no full stop: a line in `CHROME_MUTED` and, where there is one, a way
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
                .child(components::key_combo(&keys, CHROME_MUTED))
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
                .text_color(rgb(CHROME_MUTED))
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

// ------------------------------------------------------------- machines

/// A machine's heading (ADR 0026), in the heading style: its triangle
/// (`fold`, from `machine_fold`, with its own press), its connection glyph
/// where a row's mark sits, its name — the strong ink on the machine the
/// window is on — and a word for a connection that is not up. The cockpit
/// wires the press: onto the machine.
pub fn machine_heading(row: &MachineRow, fold: impl IntoElement) -> Stateful<Div> {
    let index = row.index;
    let word: Option<SharedString> = match row.connection {
        Connection::Online => None,
        Connection::Connecting => Some(words::CONNECTING.into()),
        Connection::Attention => Some(words::NEEDS_YOU.into()),
        Connection::Disabled => Some(words::OFF.into()),
    };
    let glyph = match row.connection {
        Connection::Online => components::status_dot(RUNNING),
        Connection::Connecting => components::status_ring(CHROME_MUTED),
        Connection::Attention => components::status_dot(ATTENTION),
        Connection::Disabled => components::status_ring(CHROME_FAINT),
    };
    let key = SharedString::from(format!("nav-machine-{index}"));
    row_frame(("nav-machine", index), key, false)
        .debug_selector(move || format!("nav-machine-{index}"))
        .child(fold)
        .child(cell(glyph))
        .child(fitted(
            title_w(2.0, word.as_deref()),
            if row.selected {
                TEXT_STRONG
            } else {
                CHROME_MUTED
            },
            W_BODY,
            SharedString::from(row.name.clone()),
        ))
        .child(word_cell(word))
}

/// A machine heading's triangle (`▾` open, `▸` folded): its press folds
/// the machine to its heading.
pub fn machine_fold(index: usize, folded: bool) -> Stateful<Div> {
    div()
        .id(("nav-machine-fold", index))
        .debug_selector(move || format!("nav-machine-fold-{index}"))
        .flex_shrink_0()
        .child(disclosure(!folded))
}

/// One open Thread on a machine the window is not on: its mark, its title
/// and its word, from the status the machine streams. A press goes to the
/// machine. A machine that went keeps its rows, dimmed.
pub fn machine_thread_row(machine: usize, row: &StatusRow, lost: bool) -> Stateful<Div> {
    let (status, word) = if row.waiting {
        (RowStatus::NeedsYou, Some(words::NEEDS_YOU))
    } else if row.working {
        (RowStatus::Working, Some(words::WORKING))
    } else {
        (RowStatus::Idle, None)
    };
    let thread = row.thread.get();
    let key = SharedString::from(format!("nav-machine-{machine}-{thread}"));
    let title: SharedString = row.title.clone().unwrap_or_default().into();
    // Thread ids are per Server: the machine is part of the id.
    wash(line().id(key.clone()), key.clone(), false)
        .debug_selector(move || key.to_string())
        .when(lost, |row| row.opacity(MACHINE_LOST_OPACITY))
        .child(blank_cell())
        .child(cell(status_mark(status)))
        .child(fitted(title_w(2.0, word), TEXT, W_BODY, title))
        .child(word_cell(word.map(SharedString::from)))
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
        .text_color(rgb(CHROME_MUTED))
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

// ------------------------------------------------------------- the rail

/// One open Thread on the rail: its mark, and the digit of the ⌘1–9 that
/// focuses it, if one does.
#[derive(Clone)]
pub struct RailRow {
    pub key: Option<usize>,
    pub thread: ThreadId,
    pub status: RowStatus,
    /// The board shows this Thread (Solo) or its Group: the rail's one
    /// selection, as the tree's.
    pub selected: bool,
}

/// The rail's content (cmd-B folded): under the titlebar band and a half
/// row of air, one line per open Thread. It scrolls like the tree.
pub fn rail(scroll: &ScrollHandle) -> Stateful<Div> {
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
        .pt(px(WIN_CHROME_H + NAV_PAD_Y))
        .pb(px(NAV_PAD_Y))
        .overflow_y_scroll()
        .track_scroll(scroll)
}

/// One rail line: the Thread's ⌘ digit right-aligned in two cells, dim
/// (the selected one `TEXT_STRONG`) and blank where no ⌘ key reaches it, a
/// space, and its mark — the tree row's own (`status_mark`), spinner
/// included; the mark cell's own space is the air at the right. It washes
/// and selects as a tree row does.
pub fn rail_row(row: &RailRow) -> Stateful<Div> {
    let thread = row.thread;
    let key = SharedString::from(format!("nav-rail-{}", thread.get()));
    row_frame(("nav-rail-row", thread.get() as usize), key, row.selected)
        .debug_selector(move || format!("nav-rail-{}", thread.get()))
        .pr(px(0.0))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .justify_end()
                .w(px(2.0 * CH))
                .text_color(rgb(if row.selected {
                    TEXT_STRONG
                } else {
                    CHROME_MUTED
                }))
                .children(row.key.map(|key| {
                    components::cells(key.to_string())
                        .debug_selector(move || format!("nav-rail-key-{key}"))
                })),
        )
        .child(div().flex_shrink_0().w(px(CH)))
        .child(cell(status_mark(row.status)))
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
            status: RowStatus::Idle,
        }
    }

    fn heading(branch: Option<&str>, folded: bool) -> ProjectSection {
        ProjectSection {
            project: None,
            mixed: false,
            label: "Ferrite".into(),
            branch: branch.map(SharedString::from),
            numbers: None,
            folded,
            status: RowStatus::Idle,
            solos: Vec::new(),
            groups: Vec::new(),
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
        assert_eq!(WIDTH, 264.0, "the mockups' 264px column");
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
        assert_eq!(
            fill(smart_row(&selected_thread(true), words::DONE)),
            None,
            "a smart row is a reference, never the selection"
        );
    }

    /// N-6: every unselected title is the body ink, read or unread; only
    /// the selected row reaches `TEXT_STRONG`; a parked title steps down.
    #[test]
    fn titles_are_body_ink_but_the_selected_and_the_parked() {
        assert_eq!(title_ink(false, false), TEXT);
        assert_eq!(title_ink(true, false), TEXT_STRONG);
        assert_eq!(title_ink(false, true), CHROME_MUTED);
        assert_eq!(
            title_ink(true, true),
            TEXT_STRONG,
            "the selection outranks the step-down"
        );
        assert_eq!(TEXT, 0xd0d0d0, "the refresh's default chrome ink");
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
        assert_eq!(cursor(project_section(0, &heading(None, false))), None);
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
        assert_eq!(fold.style().text.color, Some(rgb(CHROME_FAINT).into()));
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
            details: RowDetails::default(),
            tail: NavTail::None,
            task: None,
        };
        let height = |mut drawn: Stateful<Div>| drawn.style().size.height;
        let line = Some(px(NAV_LINE).into());
        assert_eq!(height(thread_row(&bare)), line);
        assert_eq!(height(thread_row(&thread())), line);
        assert_eq!(height(thread_row(&selected_thread(true))), line);
        for tail in [
            NavTail::Waiting(words::APPROVAL),
            NavTail::Failing(Some(12)),
            NavTail::Failed,
            NavTail::Done,
            NavTail::Background,
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
        for status in [
            RowStatus::Working,
            RowStatus::Failing,
            RowStatus::Background,
            RowStatus::Done,
            RowStatus::Parked,
        ] {
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
        assert_eq!(
            height(thread_row(&ThreadRow {
                details: RowDetails {
                    ahead: 12,
                    behind: 3,
                    dirty: true,
                    pr: Some("#123 \u{2717}".into()),
                },
                ..bare.clone()
            })),
            line,
            "its checkout's facts are the same line"
        );
        assert_eq!(height(parked_header(3, false, false)), line);
        assert_eq!(height(parked_header(3, true, false)), line);
        assert_eq!(height(project_section(0, &heading(None, false))), line);
        assert_eq!(
            height(project_section(0, &heading(Some("dev"), true))),
            line
        );
        let rail = RailRow {
            key: Some(9),
            thread: ThreadId::new(9),
            status: RowStatus::Done,
            selected: false,
        };
        assert_eq!(height(rail_row(&rail)), line);
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
        assert!(close(NAV_TEXT_W, 31.0 * CH));
        let solo = title_w(2.0, Some("done"));
        assert!(
            close(solo, 22.0 * CH),
            "the solo row: 31 less 2+2 cells, 4+1: {solo}"
        );
        let title = "Nav rows jitter on stream start";
        let keep = kept(title.chars().count(), solo / CH);
        let cut: String = title.chars().take(keep).collect();
        assert_eq!(format!("{cut}\u{2026}"), "Nav rows jitter on st\u{2026}");
        assert_eq!(
            keep + 1,
            22,
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
    /// decides it: what a waiting request waits for, red tests, a failure,
    /// a working Thread's elapsed time, a finish not yet seen; then the
    /// row's own background work; everything else is its age (a finish
    /// already seen included: the Pane says nothing of it). Every
    /// word is the metadata ink: the mark carries the colour.
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
        assert_eq!(of(Some(&HeadSlot::Failing(None))), NavTail::Failing(None));
        assert_eq!(of(Some(&HeadSlot::Failed)), NavTail::Failed);
        assert_eq!(
            of(Some(&HeadSlot::Working("1m04s".into()))),
            NavTail::Working("1m04s".into()),
            "a working row says how long it has worked, to the second"
        );
        assert_eq!(of(Some(&HeadSlot::Working(String::new()))), NavTail::None);
        assert_eq!(
            of(Some(&HeadSlot::Done)),
            NavTail::Done,
            "the Pane says done only while the finish is unseen"
        );
        assert_eq!(
            NavTail::of(RowStatus::Background, None, age()),
            NavTail::Background
        );
        for quiet in [None, Some(&HeadSlot::Interrupted), Some(&HeadSlot::Parked)] {
            assert_eq!(of(quiet), NavTail::Age(age()));
        }
        let text = |tail: NavTail| tail.text();
        assert_eq!(
            text(NavTail::Waiting(words::QUESTION)),
            Some(words::QUESTION.into())
        );
        assert_eq!(text(NavTail::Failing(Some(2))), Some("failing 2".into()));
        assert_eq!(text(NavTail::Failing(None)), Some(words::FAILING.into()));
        assert_eq!(text(NavTail::Failed), Some(words::FAILED.into()));
        assert_eq!(text(NavTail::Done), Some(words::DONE.into()));
        assert_eq!(text(NavTail::Background), Some(words::BACKGROUND.into()));
        assert_eq!(text(NavTail::Working("12s".into())), Some("12s".into()));
        assert_eq!(text(NavTail::Age(age())), Some(age()));
        assert_eq!(text(NavTail::None), None);
        let mut word = word_cell(text(NavTail::Failing(Some(2))));
        assert_eq!(word.style().text.color, Some(rgb(CHROME_MUTED).into()));
    }

    /// A row's checkout facts are terse and fixed in order — drift, dirt,
    /// the PR's label — and come before its word; a clean, even checkout
    /// with no PR says nothing.
    #[test]
    fn the_checkout_facts_come_before_the_word() {
        let details = |ahead, behind, dirty, pr: Option<&str>| RowDetails {
            ahead,
            behind,
            dirty,
            pr: pr.map(str::to_string),
        };
        assert_eq!(details(0, 0, false, None).text(), None);
        assert_eq!(
            details(2, 1, true, Some("#48 \u{2717}")).text(),
            Some("\u{2191}2\u{2193}1* #48 \u{2717}".into())
        );
        assert_eq!(
            details(0, 3, false, Some("#48 \u{2713}")).text(),
            Some("\u{2193}3 #48 \u{2713}".into())
        );
        assert_eq!(details(0, 0, false, Some("#48")).text(), Some("#48".into()));
        let row = ThreadRow {
            details: details(1, 0, true, None),
            ..thread()
        };
        assert_eq!(row_words(&row, 2.0), Some("\u{2191}1* 2h".into()));
        let quiet = ThreadRow {
            tail: NavTail::None,
            ..row.clone()
        };
        assert_eq!(row_words(&quiet, 2.0), Some("\u{2191}1*".into()));
        assert_eq!(row_words(&thread(), 2.0), Some("2h".into()));
    }

    fn task(stands: &str, time: &str, third: &str) -> TaskRows {
        let timed = [stands, time]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" \u{b7} ");
        TaskRows {
            stands_timed: timed.into(),
            stands: stands.into(),
            ink: YELLOW,
            third: third.into(),
        }
    }

    fn lines(task: &TaskRows, cells: f32) -> Vec<(String, u32)> {
        task_lines(task, cells)
            .into_iter()
            .map(|(text, ink)| (text.to_string(), ink))
            .collect()
    }

    /// A Thread the board runs draws its sidebar entry under its row (ADR
    /// 0031): row 2 where it stands in its task's ink, its time last and
    /// the first thing a short row drops, then row 3 dim — and no line
    /// where a row says nothing.
    #[test]
    fn a_task_rows_entry_drops_its_time_before_it_cuts_where_it_stands() {
        assert_eq!(
            lines(&task("agent \u{b7} working", "6m", "8% 16m"), 2.0),
            [
                ("agent \u{b7} working \u{b7} 6m".to_string(), YELLOW),
                ("8% 16m".to_string(), CHROME_MUTED)
            ],
            "room for the whole"
        );
        // 33ch against a root row's 30.
        assert_eq!(
            lines(
                &task("agent \u{b7} working \u{b7} R#1 PR#113", "6m", ""),
                2.0
            ),
            [("agent \u{b7} working \u{b7} R#1 PR#113".to_string(), YELLOW)],
            "the time goes first; no numbers, no row 3"
        );
        // 29ch against a member's 28.
        assert_eq!(
            lines(&task("reviewer \u{b7} under review", "12m", "40% 3m"), 3.0),
            [
                ("reviewer \u{b7} under review".to_string(), YELLOW),
                ("40% 3m".to_string(), CHROME_MUTED)
            ],
            "a member's title edge is a cell further in"
        );
        assert_eq!(
            lines(
                &task("Coordinator \u{b7} idle", "", "Done. Both tasks are added."),
                2.0
            ),
            [
                ("Coordinator \u{b7} idle".to_string(), YELLOW),
                ("Done. Both tasks are added.".to_string(), CHROME_MUTED)
            ],
            "the Coordinator's message under it, dim"
        );
    }

    /// The title gives way last: a time goes as soon as the title would be
    /// cut, and the word, the drift and the PR's label only while the title
    /// would keep fewer than `NAV_TITLE_MIN_CH` cells.
    #[test]
    fn a_short_row_drops_its_time_before_it_cuts_the_title() {
        let row = |name: &str, tail: NavTail, ahead: u32| ThreadRow {
            name: name.into(),
            tail,
            details: RowDetails {
                ahead,
                pr: Some("#12 \u{2713}".into()),
                ..RowDetails::default()
            },
            ..thread()
        };
        let working = || NavTail::Working("1s".into());
        // 30ch beside a root row's two cells.
        assert_eq!(
            row_words(&row("Tax rounding", working(), 0), 2.0),
            Some("#12 \u{2713} 1s".into()),
            "room for everything"
        );
        assert_eq!(
            row_words(&row("Stripe webhook retries", working(), 0), 2.0),
            Some("#12 \u{2713}".into()),
            "the time goes so the title stays whole"
        );
        assert_eq!(
            row_words(
                &row("Order confirmation emails and receipts", NavTail::Done, 0),
                2.0
            ),
            Some("#12 \u{2713} done".into()),
            "a state word stays while the title keeps its minimum"
        );
        assert_eq!(
            row_words(
                &row(
                    "Order confirmation emails and receipts",
                    NavTail::Waiting("approval"),
                    12
                ),
                1.0
            ),
            Some("\u{2191}12 #12 \u{2713}".into()),
            "the word goes before the checkout's facts"
        );
        assert!(NavTail::Age("2h".into()).timed());
        assert!(!NavTail::Done.timed());
    }

    /// The nav never says `now` (C10): a Thread used seconds ago has an
    /// empty age, which draws nothing.
    #[test]
    fn the_nav_never_says_now() {
        let at = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10_000);
        for secs in [0, 1, 30, 59] {
            let age = crate::facts::since_label(at, at + std::time::Duration::from_secs(secs));
            let tail = NavTail::of(RowStatus::Idle, None, age);
            assert_eq!(tail.text(), None, "{secs}s says nothing");
        }
        let minute = crate::facts::since_label(at, at + std::time::Duration::from_secs(90));
        assert_eq!(
            NavTail::of(RowStatus::Idle, None, minute).text(),
            Some("1m".into())
        );
        for tail in [
            NavTail::Waiting(words::APPROVAL),
            NavTail::Failing(None),
            NavTail::Failed,
            NavTail::Done,
            NavTail::Background,
        ] {
            assert_ne!(tail.text().unwrap().as_ref(), "now");
        }
    }

    /// The marks are the Pane's own colours: green only for work (the
    /// hollow ring when it runs behind an idle Main), a failure red, a
    /// Decision yellow, a finish not yet seen the teal check, idle the
    /// metadata ink. Nothing pulses.
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
        use crate::cockpit::DotShape;
        let shape = |status: RowStatus| status.face().shape;
        assert_eq!(shape(RowStatus::Done), DotShape::Check);
        assert_eq!(shape(RowStatus::Background), DotShape::Ring);
        assert_eq!(shape(RowStatus::Idle), DotShape::Solid);
        assert_eq!(shape(RowStatus::Parked), DotShape::Ring);
    }

    /// Herdr's four states over Ferrite's own: an unseen finish outranks
    /// background work and is outranked by any live state; a failure reads
    /// as blocked, background work as working, parked as idle. A roll-up is
    /// the most urgent row's state.
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
        for (status, attention) in [
            (RowStatus::NeedsYou, Attention::Blocked),
            (RowStatus::Failed, Attention::Idle),
            (RowStatus::Done, Attention::Done),
            (RowStatus::Working, Attention::Working),
            (RowStatus::Failing, Attention::Working),
            (RowStatus::Background, Attention::Working),
            (RowStatus::Idle, Attention::Idle),
            (RowStatus::Parked, Attention::Idle),
        ] {
            assert_eq!(status.attention(), attention, "{status:?}");
        }
        let rows = |statuses: &[RowStatus]| -> Vec<ThreadRow> {
            statuses
                .iter()
                .map(|status| ThreadRow {
                    status: *status,
                    ..thread()
                })
                .collect()
        };
        let up = |statuses: &[RowStatus]| RowStatus::roll_up(&rows(statuses));
        assert_eq!(up(&[]), None);
        assert_eq!(
            up(&[RowStatus::Idle, RowStatus::Working, RowStatus::Done]),
            Some(RowStatus::Done)
        );
        assert_eq!(
            up(&[RowStatus::Done, RowStatus::NeedsYou, RowStatus::Failed]),
            Some(RowStatus::NeedsYou)
        );
        assert_eq!(
            up(&[RowStatus::Parked, RowStatus::Background]),
            Some(RowStatus::Background)
        );
        assert_eq!(
            up(&[RowStatus::Failed, RowStatus::Working]),
            Some(RowStatus::Working),
            "a dead Session never outranks live work"
        );
        // Ord and the four states agree, so the max is always the most
        // urgent state.
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
    /// the nav row's still mark is the Pane's own. A parked Thread is the
    /// one step the nav takes: its ring is faint (the prototype's `○`),
    /// typed like the dot.
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
        let mut ring = components::status_ring(CHROME_FAINT);
        assert_eq!(ring.style().text.color, Some(rgb(CHROME_FAINT).into()));
    }

    /// A heading's roll-up cell is kept whether or not its mark says
    /// anything, so a heading never reflows when a row starts or stops.
    #[test]
    fn a_heading_keeps_its_roll_up_cell() {
        let width = |status| roll_up_cell(status).style().size.width;
        for status in [RowStatus::Parked, RowStatus::Idle, RowStatus::Done] {
            assert_eq!(width(status), Some(px(NAV_CELL).into()), "{status:?}");
        }
    }

    /// N-8: the column is chrome from the window's top — the head row's
    /// strip over it paints nothing — with no hairline and no seam beside
    /// it: the card's edge separates (SPEC §5.1).
    #[test]
    fn the_column_is_chrome_from_the_top() {
        let mut column = shell(WIDTH);
        let style = column.style();
        assert_eq!(style.background, None, "the body paints the ground");
        let edges = &style.border_widths;
        assert!(edges.top.is_none() && edges.right.is_none());
        assert!(edges.bottom.is_none() && edges.left.is_none());
        assert_eq!(style.size.width, Some(px(WIDTH).into()));
        assert_eq!(FOLDED_WIDTH, 6.0 * CH, "folded, the rail");
        let mut ground = body();
        let style = ground.style();
        assert_eq!(
            style.background,
            Some(gpui::Hsla::from(paint::CHROME).into())
        );
        assert_eq!(style.inset.top, Some(px(0.).into()));
    }

    /// The rail keeps every open Thread's line: its ⌘ digit, dim, and its
    /// mark, in 6ch; the selected line is the tree's selection.
    #[test]
    fn the_rail_keys_its_threads_and_keeps_their_marks() {
        let row = |selected| RailRow {
            key: Some(3),
            thread: ThreadId::new(4),
            status: RowStatus::NeedsYou,
            selected,
        };
        let mut line = rail_row(&row(false));
        let style = line.style();
        assert_eq!(style.padding.left, Some(px(NAV_PAD_X).into()));
        assert_eq!(style.padding.right, Some(px(0.0).into()));
        assert_eq!(style.background, None);
        assert_eq!(
            rail_row(&row(true)).style().background,
            Some(gpui::Hsla::from(paint::SELECTION).into())
        );
        assert_eq!(NAV_PAD_X + 2.0 * CH + CH + NAV_CELL, FOLDED_WIDTH);
        let scroll = ScrollHandle::new();
        assert_eq!(
            rail(&scroll).style().size.width,
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
            Some(rgb(CHROME_MUTED).into()),
            "a dim header line"
        );
    }

    /// The nav's own clocks: the row wash reaches full in 80ms, the
    /// sidebar toggle in 100ms and the content fades over 150ms, each on
    /// CSS `ease` (the prototype's transitions).
    #[test]
    fn the_nav_keeps_the_prototypes_clocks() {
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
