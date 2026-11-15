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
    /// The tab `reveal` asked for, and whether a frame has laid it out. The
    /// next frame aims at it again, since slots that frame brought in may
    /// have pushed it part way out.
    revealing: Cell<Option<(usize, bool)>>,
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
            Some((ix, false)) => self.revealing.set(Some((ix, true))),
            Some((ix, true)) => self.handle.scroll_to_item(ix),
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

    /// Whether the row is mid-ease or mid-reveal: the frame must be drawn
    /// again.
    pub(crate) fn running(&self, cx: &gpui::App) -> bool {
        self.revealing.get().is_some()
            || self.tween.get().is_some_and(|tween| {
            tween.running(cx.background_executor().now(), motion::reduced_motion(cx))
        })
    }

    /// A chevron's press: the row eases `side`ward by `tab_scroll_step` of
    /// what shows.
    pub(crate) fn ease(&self, side: Side, cx: &gpui::App) {
        let visible = f32::from(self.handle.bounds().size.width);
        let at = self.at();
        let to = (at + side.toward() * tab_scroll_step(visible)).clamp(0., self.most());
        self.revealing.set(None);
        self.tween.set(Some(Tween::retarget(
            self.tween.take(),
            at,
            to,
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
        self.revealing.set(Some((ix, false)));
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
