//! The Thread details card's own state, one per Pane (the cockpit draws it,
//! `cockpit/details.rs`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use ferrite_ui::motion::{Exits, MotionSpec};
use gpui::{Bounds, Context, FocusHandle, Pixels, ScrollHandle};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Standing {
    Hidden,
    Open,
    /// Went at the instant: still drawn, answering no pointer, until its
    /// exit has run.
    Leaving(Instant),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Density {
    Full,
    Compact,
    Essential,
}

#[derive(Default)]
struct Measurement {
    key: String,
    height: Option<f32>,
}

#[derive(Default)]
pub struct Measurements {
    full: Measurement,
    compact: Measurement,
}

impl Measurements {
    pub fn prepare(&mut self, full: String, compact: String) {
        for (cached, key) in [(&mut self.full, full), (&mut self.compact, compact)] {
            if cached.key != key {
                *cached = Measurement { key, height: None };
            }
        }
    }

    pub fn density(&self, available: f32) -> Density {
        if self.full.height.is_none_or(|height| height <= available) {
            Density::Full
        } else if self.compact.height.is_none_or(|height| height <= available) {
            Density::Compact
        } else {
            Density::Essential
        }
    }

    pub fn record(&mut self, density: Density, key: &str, height: f32) -> bool {
        let cached = match density {
            Density::Full => &mut self.full,
            Density::Compact => &mut self.compact,
            Density::Essential => return false,
        };
        if cached.key != key || cached.height == Some(height) {
            return false;
        }
        cached.height = Some(height);
        true
    }
}

pub struct DetailsCard {
    open: Cell<bool>,
    exit: RefCell<Exits<()>>,
    /// The card's keys (Escape closes it) while the keyboard is in it.
    pub focus: FocusHandle,
    pub scroll: ScrollHandle,
    pub measurements: Rc<RefCell<Measurements>>,
    pub canvas: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Where the card was last drawn, `None` while it is not: the cards
    /// that hang from the Pane's head hang under it.
    pub drawn: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub top_inset: Cell<f32>,
    pub fit_width: Cell<Option<f32>>,
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
            measurements: Rc::default(),
            canvas: Rc::default(),
            drawn: Rc::default(),
            top_inset: Cell::new(0.),
            fit_width: Cell::new(None),
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

    pub fn opener(&self) -> Rc<Cell<Option<Bounds<Pixels>>>> {
        self.opener.clone()
    }

    pub fn on_opener(&self, at: gpui::Point<Pixels>) -> bool {
        self.opener.get().is_some_and(|bounds| bounds.contains(&at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_falls_back_after_each_content_measurement() {
        let mut measured = Measurements::default();
        measured.prepare("full".into(), "compact".into());
        assert_eq!(measured.density(160.), Density::Full);
        assert!(measured.record(Density::Full, "full", 220.));
        assert_eq!(measured.density(160.), Density::Compact);
        assert!(measured.record(Density::Compact, "compact", 180.));
        assert_eq!(measured.density(160.), Density::Essential);
        assert_eq!(measured.density(180.), Density::Compact);
        assert_eq!(measured.density(220.), Density::Full);
    }

    #[test]
    fn changed_content_invalidates_each_density_independently() {
        let mut measured = Measurements::default();
        measured.prepare("full".into(), "compact".into());
        measured.record(Density::Full, "full", 220.);
        measured.record(Density::Compact, "compact", 180.);
        measured.prepare("new full".into(), "compact".into());
        assert_eq!(measured.density(160.), Density::Full);
        assert!(
            !measured.record(Density::Full, "full", 100.),
            "stale layout cannot restore an old measurement"
        );
        measured.record(Density::Full, "new full", 230.);
        assert_eq!(
            measured.density(160.),
            Density::Essential,
            "unchanged compact content keeps its measurement"
        );
        measured.prepare("new full".into(), "new compact".into());
        assert_eq!(measured.density(160.), Density::Compact);
    }
}
