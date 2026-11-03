//! The window's frame around the Cockpit: the titlebar, the Settings
//! sheet, the key table's binding and the menu bar (ADR 0035). The app
//! owns them (`ferrite-frame`) and installs them at launch; the Cockpit
//! fills them through `Frame` with what only it knows (which settings
//! exist and what they change), so an edit to the frame rebuilds no
//! Cockpit.
//!
//! The Cockpit still draws the frame in its own render, in the slots its
//! layout gives it, so the frame rides the Cockpit's cached frame
//! (ADR 0012) exactly as before.

use std::sync::OnceLock;

use gpui::{AnyElement, App, Entity, FocusHandle, Window};

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

// ------------------------------------------------------------ free disk

/// Free space under which the nav's foot shows it, and under which it asks
/// for attention (SPEC §5.7), in decimal bytes.
const DISK_SHOWN_UNDER: u64 = 20 * GB;
const DISK_LOW_UNDER: u64 = 5 * GB;
const GB: u64 = 1_000_000_000;

/// `7.5 GB free` and its ink, for `bytes` free on the Threads' volume:
/// one decimal under ten gigabytes, whole ones from there. `None` from
/// 20 GB up: the foot says nothing while space is plentiful.
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
        crate::theme::INK_SUBTLE
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
    use crate::theme::{ATTENTION, INK_SUBTLE};

    /// SPEC §5.7: free disk shows only under 20 GB, in `ATTENTION` under
    /// 5 GB, one decimal under ten.
    #[test]
    fn free_disk_speaks_only_when_space_runs_low() {
        assert_eq!(free_disk_words(20 * GB), None);
        assert_eq!(
            free_disk_words(19_600_000_000),
            Some(("20 GB free".into(), INK_SUBTLE))
        );
        assert_eq!(
            free_disk_words(12_300_000_000),
            Some(("12 GB free".into(), INK_SUBTLE))
        );
        assert_eq!(
            free_disk_words(7_500_000_000),
            Some(("7.5 GB free".into(), INK_SUBTLE))
        );
        assert_eq!(
            free_disk_words(4_990_000_000),
            Some(("5.0 GB free".into(), ATTENTION))
        );
    }

    /// The cards start under the head row on every platform (SPEC §4).
    #[test]
    fn the_board_starts_under_the_head_row() {
        assert_eq!(board_top(), crate::theme::WIN_CHROME_H);
    }
}
