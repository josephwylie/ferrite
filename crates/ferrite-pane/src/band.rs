//! The band over a draft's Composer, where its Pane was last laid out: a
//! sheet opened for the draft keeps its card clear of it
//! (`ferrite_ui::form::card_over`).

use std::cell::Cell;
use std::rc::Rc;

use crate::pane::PaneGeometry;

#[derive(Clone)]
pub struct Band(Rc<Cell<PaneGeometry>>);

impl Band {
    /// The band of the draft whose Pane is laid out in `geometry`.
    pub fn new(geometry: Rc<Cell<PaneGeometry>>) -> Self {
        Self(geometry)
    }

    /// Where the band's top edge is, once the draft has been laid out.
    pub fn top(&self) -> Option<f32> {
        let composer = self.0.get().composer?;
        Some(f32::from(composer.top()) - ferrite_ui::theme::ENV_STRIP_H)
    }
}
