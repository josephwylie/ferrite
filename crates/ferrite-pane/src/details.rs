//! The Thread details card: its own state, one per Pane, and its draw in
//! the Pane's cell (`card`), from the sections the window reads for it
//! (`content::Content`).

pub mod content;
pub mod lineage;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use ferrite_core::ThreadKey;
use ferrite_ui::motion::{self, Exits, MotionSpec};
use ferrite_ui::theme::*;
use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, AnyElement, Bounds, Context, Entity, FocusHandle, MouseButton, MouseDownEvent, Pixels, ScrollHandle};

use crate::cell::{DetailsRead, PaneAsk, PaneAsks};
use crate::pane::PaneView;

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
    /// How many `Show more` presses the open card's `Previous agents`
    /// has had: back to none when the card opens or its Subject changes.
    previous_pages: Cell<usize>,
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
            previous_pages: Cell::new(0),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open.get()
    }

    /// Opening a shut card starts its `Previous agents` on the first page.
    pub fn set_open(&self, open: bool) {
        if open && !self.open.get() {
            self.reset_pages();
        }
        self.open.set(open);
    }

    pub fn previous_pages(&self) -> usize {
        self.previous_pages.get()
    }

    /// A `Show more` press on `Previous agents`.
    pub fn show_more(&self) {
        self.previous_pages.set(self.previous_pages.get() + 1);
    }

    pub fn reset_pages(&self) {
        self.previous_pages.set(0);
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

/// The corner the card hangs from, which it grows and shrinks about.
const DETAILS_ORIGIN: motion::ScaleFrom = (1.0, 0.0);

/// Whether the card stands inline beside the transcript: a readable
/// conversation lane and 160px of usable height.
pub fn inline(details: &DetailsCard) -> bool {
    details.canvas.get().is_some_and(|card| {
        let current = f32::from(card.size.width);
        let fit = details.fit_width.get().unwrap_or(current);
        fit >= 280. + 12. + 32. + 48. + 640. && card.size.height >= px(160. + 24. + details.top_inset.get())
    })
}

/// The lane the card keeps beside the transcript, while it stands inline
/// and open there (`inline_open`).
pub(crate) fn lane_width(details: &DetailsCard, inline_open: bool) -> f32 {
    if inline(details) && inline_open {
        let width = details.canvas.get().map_or(0., |card| f32::from(card.size.width));
        let lane_right = width - DETAILS_W - DETAILS_GAP - 32.;
        let chat_width = TX_COLUMN_MAX_W.min(lane_right - 48.);
        let left = ((width - chat_width) / 2.).min(lane_right - chat_width).max(48.);
        (width - 2. * left - chat_width).max(0.)
    } else {
        0.
    }
}

/// Draw the thread's context using the canvas placement and available
/// density: `inline_open` as the window holds it, and `choosing` while
/// its menu is up.
pub(crate) fn card(
    thread: ThreadKey,
    pane: &Entity<PaneView>,
    (inline_open, choosing): (bool, bool),
    details: &DetailsRead,
    asks: &PaneAsks,
    cx: &mut gpui::App,
) -> Option<AnyElement> {
    let canvas = pane.read(cx).details.canvas.get()?;
    let find_open = pane
        .read(cx).transcript()
        .is_some_and(|transcript| transcript.read(cx).find_open());
    pane.read(cx).details.top_inset.set(if find_open {
        2.0 * FLOAT_OFFSET + FLOAT_ROW_H - DETAILS_GAP
    } else {
        0.
    });
    let inline = inline(&pane.read(cx).details);
    if inline {
        pane.read(cx).details.set_open(false);
    }
    let standing = if inline {
        if inline_open {
            Standing::Open
        } else {
            Standing::Hidden
        }
    } else {
        pane.read(cx).details.standing(
            motion::DIALOG_OUT,
            cx.background_executor().now(),
            motion::reduced_motion(cx),
        )
    };
    if standing == Standing::Hidden {
        pane.read(cx).details.drawn.set(None);
        return None;
    }
    let top = if find_open {
        2.0 * FLOAT_OFFSET + FLOAT_ROW_H
    } else {
        DETAILS_GAP
    };
    let available = (f32::from(canvas.size.height) - top - DETAILS_GAP).max(0.);
    let content = {
        let view = pane.read(cx);
        details(thread, &view.selected, view.details.previous_pages(), cx)?
    };
    let width = (f32::from(canvas.size.width) - 2. * DETAILS_GAP).clamp(0., DETAILS_W);
    let measured = pane.read(cx).details.measurements.clone();
    measured.borrow_mut().prepare(
        content.key(thread, width, Density::Full),
        content.key(thread, width, Density::Compact),
    );
    let density = measured.borrow().density(available);
    let key = content.key(thread, width, density);
    let scrolling_height = content.scrolling_height(density);
    let sections = ferrite_ui::components::on_bounds(
        div()
            .relative()
            .debug_selector(move || {
                format!(
                    "details-density-{}",
                    match density {
                        Density::Full => "full",
                        Density::Compact => "compact",
                        Density::Essential => "essential",
                    }
                )
            })
            .child(content.render(density, asks, cx)),
        move |bounds, _, cx| {
            let height = (f32::from(bounds.size.height) - scrolling_height + 2.).max(0.);
            if measured.borrow_mut().record(density, &key, height) {
                cx.refresh_windows();
            }
        },
    );
    let scroll = pane.read(cx).details.scroll.clone();
    let body = div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .child(
            div()
                .id(("details-body", thread.thread().get() as usize))
                .debug_selector(|| "details-body".into())
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&scroll)
                .text_size(px(13.))
                .font_weight(W_LABEL)
                .child(sections),
        )
        .child(ferrite_ui::components::scrollbar_layer().child(
            ferrite_ui::components::scrollbar_bar("details-scrollbar", &scroll),
        ));
    let card = ferrite_ui::menu::float()
        .id(("details-card", thread.thread().get() as usize))
        .debug_selector(|| "details-card".into())
        .key_context("ThreadDetails")
        .track_focus(&pane.read(cx).details.focus)
        .flex()
        .flex_col()
        .w(px(DETAILS_W))
        .rounded(px(16.))
        .bg(rgb(0x111111))
        .border_color(rgba(0xffffff1a))
        .max_w_full()
        .max_h_full()
        .min_w_0()
        .overflow_hidden()
        .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx: &mut gpui::App| {
            cx.stop_propagation()
        })
        .on_mouse_down(
            MouseButton::Right,
            asks.on(|event: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                Some(PaneAsk::DetailsMenu { at: event.position })
            }),
        )
        .on_mouse_down_out({
            // A press on the card's opener toggles it, and one in its menu
            // chooses what it shows: neither shuts it from here. Both are
            // read as the press lands, before its own listeners act.
            let opener = pane.downgrade();
            asks.on(move |event: &MouseDownEvent, _, cx| {
                let on_toggle = opener
                    .upgrade()
                    .is_some_and(|pane| pane.read(cx).details.on_opener(event.position));
                (!inline && !on_toggle && !choosing).then_some(PaneAsk::CloseDetails)
            })
        })
        .child(body);
    let drawn = pane.read(cx).details.drawn.clone();
    // The measure is inside the card's 1px edge.
    let card = ferrite_ui::components::on_bounds(card, move |bounds, _, _| {
        drawn.set(Some(bounds.dilate(px(1.))))
    });
    let card = match standing {
        Standing::Leaving(left) => {
            motion::float_out(left, motion::DIALOG_OUT, card, DETAILS_ORIGIN).into_any_element()
        }
        _ if inline => card.into_any_element(),
        _ => motion::dialog_in_casting(
            ("details-card-in", thread.thread().get() as usize),
            card,
            ferrite_ui::components::float_shadow,
            DETAILS_ORIGIN,
        )
        .into_any_element(),
    };
    Some(
        div()
            .absolute()
            .top(px(top))
            .bottom(px(DETAILS_GAP))
            .left(px(DETAILS_GAP))
            .right(px(DETAILS_GAP))
            .flex()
            .justify_end()
            .items_start()
            .child(card)
            .into_any_element(),
    )
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
