//! The window's frame around the Cockpit: the titlebar, the bottom bar,
//! the Settings sheet, the key table's binding and the menu bar (ADR
//! 0035). The app owns them (`ferrite-frame`) and installs them at launch;
//! the Cockpit fills them through `Frame` with what only it knows (the
//! views, the vitals, which settings exist and what they change), so an
//! edit to the frame rebuilds no Cockpit.
//!
//! The Cockpit still draws the frame in its own render, in the slots its
//! layout gives it, so the frame rides the Cockpit's cached frame
//! (ADR 0012) exactly as before.

use std::sync::OnceLock;

use gpui::{AnyElement, App, ClickEvent, Entity, FocusHandle, SharedString, Window};

use crate::keymap::Binding;
use gpui::component::input::InputState;

pub mod settings;

/// What the app draws and binds around the Cockpit.
pub trait Frame: Send + Sync {
    /// The titlebar strip over the board, from `board_left`. `draggable`
    /// is false while an overlay may reach into the band.
    fn titlebar(&self, board_left: f32, draggable: bool, maximized: bool) -> AnyElement;
    /// The stretch the nav head drags the window by.
    fn head_drag(&self, draggable: bool, maximized: bool) -> AnyElement;
    /// The window's one bar (ADR 0014).
    fn bottom_bar(&self, bar: Bar) -> AnyElement;
    /// The Settings sheet over the Cockpit.
    fn settings(&self, sheet: SettingsSheet, window: &mut Window, cx: &mut App) -> AnyElement;
    /// Bind `table` in place of the key table bound before (ADR 0024), and
    /// set the menu bar again so its key equivalents follow.
    fn bind_keys(&self, table: Vec<Binding>, cx: &mut App);
}

static FRAME: OnceLock<Box<dyn Frame>> = OnceLock::new();

/// Make `frame` the window's frame, once per process: a second install
/// keeps the first.
pub fn install(frame: impl Frame + 'static) {
    let _ = FRAME.set(Box::new(frame));
}

/// The installed frame.
pub fn active() -> &'static dyn Frame {
    FRAME
        .get()
        .expect("the app installs the frame before it opens a window")
        .as_ref()
}

/// Whether this build draws its own titlebar. macOS keeps the host's, and
/// hiding it there would take the traffic lights with it.
pub const CUSTOM: bool = cfg!(target_os = "windows");

/// Where the board's cards start: under the head row (SPEC §4), on every
/// platform — the row is the slot the view head fills.
pub fn board_top() -> f32 {
    crate::theme::WIN_CHROME_H
}

// ------------------------------------------------------------ bottom bar

/// A press on something the frame draws for the Cockpit.
pub type Press = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// What the bottom bar shows (SPEC §5.7): the view tabs, then the vitals.
pub struct Bar {
    pub tabs: Vec<BarTab>,
    /// The need-you count and what a press on it does; nothing at zero.
    pub needs_you: Option<(usize, Press)>,
    /// Each provider's account limits, in bar order.
    pub usage: Vec<(
        ferrite_core::store::Provider,
        ferrite_core::transcript::RateLimits,
    )>,
    /// Free bytes on the Threads' volume, when read.
    pub free_disk: Option<u64>,
    pub clock: SharedString,
}

/// One view's tab: `1 Solo`, `2 Work`.
pub struct BarTab {
    pub id: SharedString,
    pub ordinal: usize,
    pub label: SharedString,
    pub current: bool,
    pub roll_up: Option<RollUp>,
    pub press: Press,
}

/// A view's roll-up dot (SPEC §5.7): `ATTENTION` when anything it shows is
/// blocked, else `RUNNING` when anything is working, else none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RollUp {
    Working,
    Blocked,
}

impl RollUp {
    /// The roll-up of `attentions`: blocked wins over working.
    pub fn of(attentions: impl IntoIterator<Item = ferrite_core::Attention>) -> Option<Self> {
        use ferrite_core::Attention;
        let mut working = false;
        for attention in attentions {
            match attention {
                Attention::Blocked => return Some(RollUp::Blocked),
                Attention::Working => working = true,
                _ => {}
            }
        }
        working.then_some(RollUp::Working)
    }

    pub fn ink(self) -> u32 {
        match self {
            RollUp::Working => crate::theme::RUNNING,
            RollUp::Blocked => crate::theme::ATTENTION,
        }
    }
}

/// Free space under which the bar shows it, and under which it asks for
/// attention (SPEC §5.7), in the bar's decimal bytes.
const DISK_SHOWN_UNDER: u64 = 20 * GB;
const DISK_LOW_UNDER: u64 = 5 * GB;
const GB: u64 = 1_000_000_000;

/// `7.5 GB free` and its ink, for `bytes` free on the Threads' volume:
/// one decimal under ten gigabytes, whole ones from there. `None` from
/// 20 GB up: the bar says nothing while space is plentiful.
pub fn free_disk_words(bytes: u64) -> Option<(String, u32)> {
    if bytes >= DISK_SHOWN_UNDER {
        return None;
    }
    let gb = bytes as f64 / GB as f64;
    let words = if gb < 9.95 {
        format!("{gb:.1} GB free")
    } else {
        format!("{gb:.0} GB free")
    };
    let ink = if bytes < DISK_LOW_UNDER {
        crate::theme::ATTENTION
    } else {
        crate::theme::CHROME_MUTED
    };
    Some((words, ink))
}

// -------------------------------------------------------------- Settings

/// The Settings sheet: the Cockpit's pages (`settings`) and what the
/// sheet's own presses do to the Cockpit's state.
pub struct SettingsSheet {
    pub pages: Vec<settings::Page>,
    pub selected: settings::PageKey,
    pub search: Entity<InputState>,
    /// The sheet's focus; ↑/↓ step the pages while it holds it.
    pub focus: FocusHandle,
    /// Show a page (`select`), or the page `delta` rows away (`step`).
    pub select: std::rc::Rc<dyn Fn(settings::PageKey, &mut Window, &mut App)>,
    pub step: Box<dyn Fn(isize, &mut Window, &mut App)>,
    /// The close button and a press on the veil.
    pub close: std::rc::Rc<dyn Fn(&mut Window, &mut App)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{ATTENTION, CHROME_MUTED};

    /// SPEC §5.7: free disk shows only under 20 GB, in `ATTENTION` under
    /// 5 GB, one decimal under ten.
    #[test]
    fn free_disk_speaks_only_when_space_runs_low() {
        assert_eq!(free_disk_words(20 * GB), None);
        assert_eq!(
            free_disk_words(19_600_000_000),
            Some(("20 GB free".into(), CHROME_MUTED))
        );
        assert_eq!(
            free_disk_words(12_300_000_000),
            Some(("12 GB free".into(), CHROME_MUTED))
        );
        assert_eq!(
            free_disk_words(7_500_000_000),
            Some(("7.5 GB free".into(), CHROME_MUTED))
        );
        assert_eq!(
            free_disk_words(4_990_000_000),
            Some(("5.0 GB free".into(), ATTENTION))
        );
    }

    /// SPEC §5.7: a view's dot says blocked over working, and nothing for
    /// a view at rest.
    #[test]
    fn a_tab_rolls_up_blocked_over_working() {
        use ferrite_core::Attention;
        assert_eq!(RollUp::of([Attention::Idle, Attention::Done]), None);
        assert_eq!(
            RollUp::of([Attention::Done, Attention::Working]),
            Some(RollUp::Working)
        );
        assert_eq!(
            RollUp::of([Attention::Working, Attention::Blocked, Attention::Idle]),
            Some(RollUp::Blocked)
        );
    }

    /// The cards start under the head row on every platform (SPEC §4).
    #[test]
    fn the_board_starts_under_the_head_row() {
        assert_eq!(board_top(), crate::theme::WIN_CHROME_H);
    }
}
