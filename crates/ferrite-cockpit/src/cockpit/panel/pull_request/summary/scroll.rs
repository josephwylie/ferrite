//! Summary's real headings stick within their section, clipped to its scroller.
use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Element, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, ScrollHandle, Window, point, px,
};

use crate::cockpit::panel::pr_read::SummarySection;

// The metadata row precedes the three sections in this scroller.
fn section_index(section: SummarySection) -> usize {
    section as usize + 1
}

pub(super) fn anchor_before_collapse(
    scroll: &ScrollHandle,
    section: SummarySection,
    heading_top: Option<Pixels>,
) {
    let Some(section) = scroll.bounds_for_item(section_index(section)) else {
        return;
    };
    let Some(heading_top) = heading_top else {
        return;
    };
    let offset = scroll.offset();
    if let Some(top) = collapse_anchor(
        -f32::from(offset.y),
        f32::from(scroll.bounds().top()),
        f32::from(section.top() + offset.y),
        f32::from(heading_top),
    ) {
        scroll.set_offset(point(offset.x, px(-top)));
    }
}

/// Pinned T3 pullRequestSummaryScroll.logic.ts, including its 1px edge tolerance.
fn collapse_anchor(
    scroll_top: f32,
    viewport_top: f32,
    section_top: f32,
    heading_top: f32,
) -> Option<f32> {
    (section_top < viewport_top && heading_top <= viewport_top + 1.)
        .then(|| (scroll_top + section_top - viewport_top).max(0.))
}

pub(super) struct StickyHeading {
    pub scroll: ScrollHandle,
    pub section: SummarySection,
    pub top: Rc<Cell<Option<Pixels>>>,
    pub child: Option<AnyElement>,
}

impl IntoElement for StickyHeading {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for StickyHeading {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (
            self.child
                .as_mut()
                .expect("heading laid out once")
                .request_layout(window, cx),
            (),
        )
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        let bottom = self
            .scroll
            .bounds_for_item(section_index(self.section))
            .map_or(bounds.bottom(), |section| {
                section.bottom() + self.scroll.offset().y
            });
        let offset = (self.scroll.bounds().top() - bounds.top())
            .max(px(0.))
            .min((bottom - bounds.bottom()).max(px(0.)));
        self.top.set(Some(bounds.top() + offset));
        // Paint above this section's prose, while retaining the scroller's clip
        // and the heading's own buttons/hitboxes. Menus use a higher priority.
        window.defer_draw(
            self.child.take().expect("heading painted once"),
            window.element_offset() + point(px(0.), offset),
            0,
            Some(window.content_mask()),
        );
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_pinned_section_moves_before_collapse() {
        assert_eq!(collapse_anchor(300., 100., -100., 100.), Some(100.));
        assert_eq!(collapse_anchor(300., 100., -100., 101.), Some(100.));
        assert_eq!(collapse_anchor(300., 100., -100., 102.), None);
        assert_eq!(collapse_anchor(0., 100., 150., 150.), None);
        assert_eq!(collapse_anchor(0., 100., 100., 100.), None);
        assert_eq!(collapse_anchor(20., 100., -100., 100.), Some(0.));
    }
}
