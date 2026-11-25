//! Where a Factory picker's trigger was last drawn, so the window's menu
//! hangs under it rather than over it. A view draws the probe inside its
//! trigger and says the `Trigger` with what it asks; the window reads its
//! bounds as it opens the menu.

use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{Bounds, Pixels};

/// Where a picker's trigger was last drawn, so its menu hangs under it
/// rather than over it.
#[derive(Clone, Default)]
pub struct Trigger(Rc<Cell<Option<Bounds<Pixels>>>>);

impl Trigger {
    /// The trigger's child that records where it is drawn; the trigger is
    /// `relative`, so this fills it.
    pub fn probe(&self) -> impl IntoElement {
        let drawn = self.0.clone();
        gpui::canvas(move |bounds, _, _| drawn.set(Some(bounds)), |_, _, _, _| {})
            .absolute()
            .size_full()
    }

    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.0.get()
    }
}
