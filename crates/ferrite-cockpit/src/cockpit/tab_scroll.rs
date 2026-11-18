//! A row of tabs that scrolls sideways when it overflows: the Panel's head
//! row and the Files tool's file tabs share it. The wheel's vertical turn
//! scrolls the row, a chevron at each end with more to show eases it by
//! `tab_scroll_step` (it jumps under reduced motion), and `reveal` brings a
//! tab into view. Once the row overflows both chevron slots stay in the
//! layout, the end with nothing more dimmed and inert, so the viewport
//! never changes width as a chevron comes and goes.

use std::cell::Cell;
use std::ops::Deref;

use gpui::prelude::*;
use gpui::{div, px, Div, ScrollHandle, ScrollWheelEvent, SharedString, Stateful, Window};

use ferrite_ui::icons::{self, icon};
use ferrite_ui::motion::{self, Tween};
use ferrite_ui::pointer::Pointer;
use ferrite_ui::theme::*;

/// How far a chevron scrolls a row showing `visible` of its tabs.
pub(crate) fn tab_scroll_step(visible: f32) -> f32 {
    (visible * SURFACE_TAB_SCROLL_SHARE).max(SURFACE_TAB_SCROLL_MIN)
}

/// Which end of the row a chevron stands at.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Left,
    Right,
}

impl Side {
    fn toward(self) -> f32 {
        match self {
            Self::Left => -1.,
            Self::Right => 1.,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::Left => icons::BACK,
            Self::Right => icons::CHEVRON_RIGHT,
        }
    }
}

#[derive(Default)]
pub(crate) struct TabScroll {
    handle: ScrollHandle,
    /// The ease to where a chevron sent the row, until the frame that lands
    /// it lets it go.
    tween: Cell<Option<Tween>>,
    /// Whether the chevron slots are in the layout, settled once a frame.
    slotted: Cell<bool>,
    /// The tab `reveal` asked for, until the frame after the one that drew
    /// it aims at it again: slots the first frame brought in may have pushed
    /// it part way out.
    revealing: Cell<Option<Reveal>>,
}

#[derive(Clone, Copy)]
enum Reveal {
    /// No frame has drawn the row since `reveal`.
    Asked(usize),
    /// A frame drew it; the next must aim again.
    Drawn(usize),
    /// That next frame is asked for. A row hidden before it comes stays
    /// here, asking for nothing, until it is drawn again.
    Due(usize),
}

impl Deref for TabScroll {
    type Target = ScrollHandle;

    fn deref(&self) -> &ScrollHandle {
        &self.handle
    }
}

impl TabScroll {
    fn at(&self) -> f32 {
        -f32::from(self.handle.offset().x)
    }

    fn most(&self) -> f32 {
        f32::from(self.handle.max_offset().x).max(0.)
    }

    fn set(&self, at: f32) {
        self.handle
            .set_offset(gpui::point(px(-at), self.handle.offset().y));
    }

    /// Draw the ease's frame and decide whether the chevron slots stand:
    /// called once as the row is built. The row overflows when its tabs are
    /// wider than the row would be without the slots, which the viewport,
    /// the most it scrolls and the slots' own width recover whichever state
    /// the last frame drew.
    pub(crate) fn settle(&self, cx: &gpui::App) {
        let slots = if self.slotted.get() { 2. * SURFACE_PLUS } else { 0. };
        self.slotted.set(self.most() > slots + 0.5);
        match self.revealing.take() {
            Some(Reveal::Asked(ix)) => self.revealing.set(Some(Reveal::Drawn(ix))),
            Some(Reveal::Drawn(ix) | Reveal::Due(ix)) => self.handle.scroll_to_item(ix),
            None => {}
        }
        let Some(tween) = self.tween.get() else {
            return;
        };
        let now = cx.background_executor().now();
        let reduced = motion::reduced_motion(cx);
        self.set(tween.value(now, reduced));
        if !tween.running(now, reduced) {
            self.tween.set(None);
        }
    }

    /// Whether the row is mid-ease or owes a reveal its second frame: the
    /// frame must be drawn again. Read once a frame, it asks for that second
    /// frame once.
    pub(crate) fn running(&self, cx: &gpui::App) -> bool {
        let due = match self.revealing.get() {
            Some(Reveal::Drawn(ix)) => {
                self.revealing.set(Some(Reveal::Due(ix)));
                true
            }
            _ => false,
        };
        due || self.tween.get().is_some_and(|tween| {
            tween.running(cx.background_executor().now(), motion::reduced_motion(cx))
        })
    }

    /// A chevron's press: the row eases `side`ward by `tab_scroll_step` of
    /// what shows.
    pub(crate) fn ease(&self, side: Side, cx: &gpui::App) {
        let visible = f32::from(self.handle.bounds().size.width);
        self.ease_to(self.at() + side.toward() * tab_scroll_step(visible), cx);
    }

    /// The keyboard came to `stop`, drawn in the row: the row eases until
    /// it shows whole, as a chevron's press would have brought it.
    pub(crate) fn ease_into_view(&self, stop: gpui::Bounds<gpui::Pixels>, cx: &gpui::App) {
        let view = self.handle.bounds();
        let at = self.at();
        let to = if stop.left() < view.left() {
            at - f32::from(view.left() - stop.left())
        } else if stop.right() > view.right() {
            at + f32::from(stop.right() - view.right())
        } else {
            return;
        };
        // A chevron hides at its end, so what it covered is room too: a
        // stop that near the end brings the end into view.
        let chevron = SURFACE_PLUS + SURFACE_HEAD_GAP;
        let most = self.most();
        let to = if to <= chevron {
            0.
        } else if to >= most - chevron {
            most
        } else {
            to
        };
        self.ease_to(to, cx);
    }

    fn ease_to(&self, to: f32, cx: &gpui::App) {
        self.revealing.set(None);
        self.tween.set(Some(Tween::retarget(
            self.tween.take(),
            self.at(),
            to.clamp(0., self.most()),
            motion::TAB_SCROLL,
            cx.background_executor().now(),
            motion::reduced_motion(cx),
        )));
    }

    /// The wheel's vertical turn scrolls the row sideways, and stops a
    /// chevron's ease. Whether it did.
    pub(crate) fn wheel(&self, event: &ScrollWheelEvent, window: &Window) -> bool {
        let delta = event.delta.pixel_delta(window.line_height());
        if delta.y.abs() <= delta.x.abs() {
            return false;
        }
        self.tween.set(None);
        self.revealing.set(None);
        self.set((self.at() - f32::from(delta.y)).clamp(0., self.most()));
        true
    }

    /// Bring child `ix` of the row into view when it is next drawn.
    pub(crate) fn reveal(&self, ix: usize) {
        self.tween.set(None);
        self.revealing.set(Some(Reveal::Asked(ix)));
        self.handle.scroll_to_item(ix);
    }

    /// Whether the chevron slots stand at the row's ends.
    pub(crate) fn overflowing(&self) -> bool {
        self.slotted.get()
    }

    /// Whether there is more to show past `side`.
    pub(crate) fn more(&self, side: Side) -> bool {
        match side {
            Side::Left => self.at() > 0.5,
            Side::Right => self.at() < self.most() - 0.5,
        }
    }
}

/// The chevron slot at `side`'s end, named `<prefix>-left` or
/// `<prefix>-right` and calling `press` while the row has more that way,
/// dimmed and unnamed while it has not. Either way a press on it never
/// reaches the title bar beneath. `None` while the row does not overflow.
pub(crate) fn chevron(
    prefix: &str,
    side: Side,
    scroll: &TabScroll,
    press: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> Option<Stateful<Div>> {
    if !scroll.overflowing() {
        return None;
    }
    let key = SharedString::from(format!("{prefix}-{}", side.name()));
    let slot = div()
        .id(key.clone())
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(SURFACE_PLUS))
        .rounded(px(R_CONTROL))
        .occlude()
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(icon(side.glyph(), ICON_CHEVRON, INK_SUBTLE));
    Some(if scroll.more(side) {
        slot.debug_selector({
            let key = key.clone();
            move || key.to_string()
        })
        .hover_raised(key)
        .on_click(press)
    } else {
        slot.opacity(SURFACE_TAB_CHEVRON_SPENT_OPACITY)
    })
}
