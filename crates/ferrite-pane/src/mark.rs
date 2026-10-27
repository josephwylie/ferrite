//! The one status truth: a Thread's state as every surface reads it
//! (`RowStatus`), the word it says (`NavTail`), and its mark, drawn here for
//! the nav row, the rail, the Pane head, the palette, a subagent's tab and
//! the empty board's recent rows.

use ferrite_core::Attention;
use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, AnyElement, SharedString};

use crate::pane::WallState;
use ferrite_ui::components;
use ferrite_ui::icons::{self, icon};
use ferrite_ui::theme::*;

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

/// A Thread's state, for its mark, least urgent first: the order a
/// roll-up takes its `max` in, consistent with `Attention`. Every surface
/// reads a Thread's state as one of these (`RowStatus::of`) and draws it
/// with `mark`, so no two can disagree.
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

    /// The ink its mark (`mark`) wears: hue for state (running, failing or
    /// failed, needs you), `IDLE` at rest — done is a shape, not a hue.
    pub fn ink(self) -> u32 {
        match self {
            RowStatus::Working | RowStatus::Background => RUNNING,
            RowStatus::Failing | RowStatus::Failed => BLOCKED,
            RowStatus::NeedsYou => ATTENTION,
            RowStatus::Done | RowStatus::Idle | RowStatus::Parked => IDLE,
        }
    }
}

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
/// and 13px icons, laid out at the width the mockup's box takes
/// (`PANE_HEAD_DOT_W`: a dot's halo overflows it), so the title starts
/// where it is drawn.
pub(crate) fn head_mark(status: RowStatus) -> AnyElement {
    let face = mark_sized(status, NAV_DOT, ferrite_ui::theme::PANE_HEAD_ICON);
    match status {
        RowStatus::NeedsYou | RowStatus::Failed | RowStatus::Background => div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .w(px(ferrite_ui::theme::PANE_HEAD_DOT_W))
            .child(face)
            .into_any_element(),
        _ => div().flex().flex_shrink_0().child(face).into_any_element(),
    }
}

/// **The one status truth**: a state's mark, drawn here for every surface
/// that shows a Thread's state (the nav row, the rail, the Pane head, the
/// palette, a subagent's tab, the empty board's recent rows), in
/// `RowStatus::ink`. `dot` and `glyph` are the surface's sizes.
fn mark_sized(status: RowStatus, dot: f32, glyph: f32) -> AnyElement {
    let ink = status.ink();
    let solid = || div().size(px(dot)).rounded_full().bg(rgb(ink));
    match status {
        RowStatus::Working | RowStatus::Failing => div()
            .font_family(FONT_CODE)
            .text_size(px(NAV_SPINNER_FS))
            .line_height(px(NAV_LINE))
            .child(components::braille_spinner(ink))
            .into_any_element(),
        RowStatus::NeedsYou => div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(nav_halo_box(dot)))
            .rounded_full()
            .bg(rgba(NAV_HALO_FILL))
            .child(solid())
            .into_any_element(),
        RowStatus::Failed => solid().into_any_element(),
        RowStatus::Background => div()
            .size(px(dot))
            .rounded_full()
            .border_1()
            .border_color(rgb(ink))
            .into_any_element(),
        RowStatus::Done => icon(icons::CHECK_CIRCLE, glyph, ink).into_any_element(),
        RowStatus::Idle => icon(icons::TERMINAL, glyph, ink).into_any_element(),
        RowStatus::Parked => div()
            .opacity(NAV_PARKED_OPACITY)
            .child(icon(icons::TERMINAL, glyph, ink))
            .into_any_element(),
    }
}
