//! The Thread details card's own state: one per Pane, so one per Thread.
//! The card is the Thread's lightweight context floating over its
//! transcript (T3's `ThreadDetailsCard`), shown and dismissed on its own,
//! whatever tool the right panel holds. The cockpit draws what it shows
//! (`cockpit/details.rs`) and the Pane only places it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use ferrite_ui::motion::{Exits, MotionSpec};
use gpui::{Bounds, Context, FocusHandle, Pixels, ScrollHandle};

/// How the card stands on a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Standing {
    Hidden,
    Open,
    /// Went at the instant: still drawn, answering no pointer, until its
    /// exit has run.
    Leaving(Instant),
}

pub struct DetailsCard {
    open: Cell<bool>,
    exit: RefCell<Exits<()>>,
    /// The card's keys (Escape closes it) while the keyboard is in it.
    pub focus: FocusHandle,
    /// Its body's scroll, apart from the panel's Overview.
    pub scroll: ScrollHandle,
    /// Where the opening control was last laid out: a press on it is the
    /// toggle's, not a press outside the card.
    opener: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl DetailsCard {
    pub fn new<T: 'static>(cx: &mut Context<T>) -> Self {
        Self {
            open: Cell::new(false),
            exit: RefCell::default(),
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            opener: Rc::default(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open.get()
    }

    pub fn set_open(&self, open: bool) {
        self.open.set(open);
    }

    /// This frame's standing, `exit` after it was closed. Reduced motion
    /// leaves nothing to stand out.
    pub fn standing(&self, exit: MotionSpec, now: Instant, reduced: bool) -> Standing {
        let open = self.open.get();
        let leaving =
            self.exit
                .borrow_mut()
                .stage(open.then_some(&()), |_, _| true, exit, now, reduced);
        match (open, leaving) {
            (true, _) => Standing::Open,
            (false, Some(((), left))) => Standing::Leaving(left),
            (false, None) => Standing::Hidden,
        }
    }

    /// The cell the opening control lays its bounds into.
    pub fn opener(&self) -> Rc<Cell<Option<Bounds<Pixels>>>> {
        self.opener.clone()
    }

    /// Whether a press at `at` is on the control that opens the card.
    pub fn on_opener(&self, at: gpui::Point<Pixels>) -> bool {
        self.opener.get().is_some_and(|bounds| bounds.contains(&at))
    }
}
