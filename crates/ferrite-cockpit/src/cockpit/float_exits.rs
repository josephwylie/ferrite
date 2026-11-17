//! What the menus and popovers that go as a face last showed (`motion::float_gone`).
//!
//! Each builder reads its own state to draw; once the operator closes one
//! that state is gone, so the last open one is kept here for the length of
//! its exit.

use std::cell::RefCell;
use std::time::Instant;

use ferrite_core::ThreadId;
use ferrite_ui::motion::{self, Exits, FloatMotion};

use super::context_menu::ContextMenu;
use super::CockpitView;

#[derive(Default)]
pub(super) struct FloatExits {
    pub(super) context_menu: Exits<ContextMenu>,
    pub(super) plan: Exits<ThreadId>,
    pub(super) session: Exits<(ThreadId, u64)>,
    pub(super) checks: Exits<ThreadId>,
    pub(super) mode: Exits<(ThreadId, u64, bool)>,
}

pub(super) type Held = RefCell<FloatExits>;

impl CockpitView {
    /// `open`, or the last one open with when it went while its exit runs.
    pub(super) fn standing<T: Clone>(
        &self,
        pick: fn(&mut FloatExits) -> &mut Exits<T>,
        open: Option<T>,
        motion: FloatMotion,
        cx: &gpui::App,
    ) -> Option<(T, Option<Instant>)> {
        pick(&mut self.float_exits.borrow_mut()).standing(
            open,
            motion.out,
            cx.background_executor().now(),
            motion::reduced_motion(cx),
        )
    }
}
