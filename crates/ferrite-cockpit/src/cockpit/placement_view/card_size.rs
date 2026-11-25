//! The setup sheet's card size as it moves between faces: from choosing
//! where a copy goes to the compact progress card, to what the copy did,
//! and as the folder browser opens. Its width and height ease to the next
//! face's over the sheet's own motion (`DIALOG_IN`), interruptible; the
//! height a face takes is its content's, measured as it is drawn, so the
//! card never guesses it.

use ferrite_ui::motion::{self, Tween};
use gpui::{px, App, IntoElement, Styled, Window};
use std::cell::Cell;
use std::rc::Rc;

/// Heights within this of each other are one height: layout rounds.
const SAME: f32 = 0.5;

#[derive(Default)]
pub(super) struct CardSize {
    /// The card's width and height on their way to `target`.
    width: Option<Tween>,
    height: Option<Tween>,
    target: Option<(f32, f32)>,
    /// As last laid out: the card, the body between its head and footer,
    /// and what the body holds.
    card: Rc<Cell<f32>>,
    body: Rc<Cell<f32>>,
    content: Rc<Cell<f32>>,
}

impl CardSize {
    /// The height the card takes to show all it holds, once it has been
    /// laid out: its head and footer, and its content whole.
    fn natural(&self) -> Option<f32> {
        let (card, body, content) = (self.card.get(), self.body.get(), self.content.get());
        (card > 0. && content > 0.).then(|| card - body + content)
    }

    /// The card's size this frame: `width` wide, and `steady` tall, or as
    /// tall as its content up to `room`. A change from the last eases
    /// there; no height until the content has been laid out once, when
    /// the card takes its content's height on its own.
    pub(super) fn frame(
        &mut self,
        width: f32,
        steady: Option<f32>,
        room: f32,
        window: &mut Window,
        cx: &App,
    ) -> (f32, Option<f32>) {
        let now = cx.background_executor().now();
        let reduced = motion::reduced_motion(cx);
        let Some(height) = steady.or_else(|| self.natural().map(|natural| natural.min(room))) else {
            return (width, None);
        };
        let moved = self
            .target
            .is_none_or(|(w, h)| (w - width).abs() > SAME || (h - height).abs() > SAME);
        if moved {
            let ease = |tween: Option<Tween>, to: f32| match tween {
                Some(tween) => Tween::retarget(Some(tween), to, to, motion::DIALOG_IN, now, reduced),
                None => Tween::new(to, to, motion::DIALOG_IN, now),
            };
            self.width = Some(ease(self.width, width));
            self.height = Some(ease(self.height, height));
            self.target = Some((width, height));
        }
        let (Some(w), Some(h)) = (self.width, self.height) else {
            return (width, None);
        };
        if w.running(now, reduced) || h.running(now, reduced) {
            motion::request_frame(window);
        }
        (w.value(now, reduced), Some(h.value(now, reduced)))
    }

    /// Laid over the card, the body and the content: each notes its
    /// height, and draws the card again when it moved.
    pub(super) fn card_probe(&self) -> impl IntoElement {
        probe(self.card.clone())
    }

    pub(super) fn body_probe(&self) -> impl IntoElement {
        probe(self.body.clone())
    }

    pub(super) fn content_probe(&self) -> impl IntoElement {
        probe(self.content.clone())
    }
}

fn probe(height: Rc<Cell<f32>>) -> impl IntoElement {
    let noted = move |bounds: gpui::Bounds<gpui::Pixels>, window: &mut Window, _: &mut App| {
        let now = f32::from(bounds.size.height);
        if (height.replace(now) - now).abs() > SAME {
            motion::request_frame(window);
        }
    };
    gpui::canvas(noted, |_, _, _, _| {})
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .min_h(px(0.))
}
