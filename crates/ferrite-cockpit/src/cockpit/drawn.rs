//! What the last frame drew for each Pane and nav row, and how often each
//! was built: the stale-chrome tests read what is on screen, not the model,
//! and the frame budgets count the work each part did.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use ferrite_core::roster::PaneIdentity;
use ferrite_core::ThreadKey;

/// One Pane as its last build drew it, at the width the board last
/// laid its slot out.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Pane {
    pub focused: bool,
    pub unread: bool,
    pub drop_target: bool,
    pub editing: bool,
    pub reduce_motion: bool,
    pub cell_width: f32,
    /// The working clock its lines read (`12s`).
    pub clock: Option<String>,
}


thread_local! {
    static WIDTHS: RefCell<HashMap<PaneIdentity, f32>> = RefCell::new(HashMap::new());
    static HEAD_RENDERS: Cell<usize> = const { Cell::new(0) };
    static HEARINGS: Cell<usize> = const { Cell::new(0) };
    static NAV_ROWS: RefCell<HashMap<ThreadKey, String>> = RefCell::new(HashMap::new());
    static PUMPS: Cell<usize> = const { Cell::new(0) };
    static LISTS: Cell<usize> = const { Cell::new(0) };
}

pub(crate) fn listed() {
    LISTS.with(|lists| lists.set(lists.get() + 1));
}

/// How many times the window has worked out which Threads the nav
/// lists, and in which section, on this thread.
pub(crate) fn lists() -> usize {
    LISTS.with(Cell::get)
}

pub(crate) fn pumped() {
    PUMPS.with(|pumps| pumps.set(pumps.get() + 1));
}

/// How many times the pump's body has run on this thread.
pub(crate) fn pumps() -> usize {
    PUMPS.with(Cell::get)
}

/// The width the board laid Pane `identity`'s slot out at, read as its
/// cell draws.
pub(crate) fn laid_out(identity: PaneIdentity, width: f32) {
    WIDTHS.with(|widths| widths.borrow_mut().insert(identity, width));
}

pub(crate) fn built_head() {
    HEAD_RENDERS.with(|renders| renders.set(renders.get() + 1));
}

/// How many times the view head over the board has been built on this
/// thread.
pub(crate) fn head_renders() -> usize {
    HEAD_RENDERS.with(Cell::get)
}

pub(crate) fn heard() {
    HEARINGS.with(|heard| heard.set(heard.get() + 1));
}

/// How many times the toasts have read the Notices and the machines'
/// news on this thread.
pub(crate) fn hearings() -> usize {
    HEARINGS.with(Cell::get)
}

/// How many times the toast float has been built on this thread.
pub(crate) fn toast_renders() -> usize {
    ferrite_nav::testing::toast_renders()
}

pub(crate) fn record_row(key: ThreadKey, row: String) {
    NAV_ROWS.with(|rows| rows.borrow_mut().insert(key, row));
}

/// The Pane as last drawn.
pub(crate) fn pane(identity: PaneIdentity) -> Pane {
    let Some(cell) = ferrite_pane::cell::testing::drawn(identity) else {
        return Pane::default();
    };
    Pane {
        focused: cell.focused,
        unread: cell.unread,
        drop_target: cell.drop_target,
        editing: cell.editing,
        reduce_motion: cell.reduce_motion,
        cell_width: WIDTHS.with(|widths| widths.borrow().get(&identity).copied().unwrap_or_default()),
        clock: cell.clock,
    }
}

/// How many times this Pane has been built on this thread.
pub(crate) fn pane_renders(identity: PaneIdentity) -> usize {
    ferrite_pane::cell::testing::pane_renders(identity)
}

/// How many times the nav has been built on this thread.
pub(crate) fn nav_renders() -> usize {
    ferrite_nav::testing::nav_renders()
}

/// The nav row for `key` as last drawn: its status, unread, tail and
/// name.
pub(crate) fn row(key: ThreadKey) -> String {
    NAV_ROWS.with(|rows| rows.borrow().get(&key).cloned().unwrap_or_default())
}
