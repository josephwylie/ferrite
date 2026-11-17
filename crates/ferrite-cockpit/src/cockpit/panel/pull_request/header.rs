//! Measured PR identity condensation, shared by the three scrollable tabs.
use std::{cell::RefCell, rc::Rc};

use super::*;
use crate::cockpit::panel::pr_read::PrHeaderState;
use gpui::{
    App, Bounds, Element, GlobalElementId, InspectorElementId, LayoutId, Pixels, ScrollHandle,
    WeakEntity,
};

type State = Rc<RefCell<PrHeaderState>>;

/// Both forms participate in layout so the threshold and compensation use
/// their actual heights. Only the active form prepaints: the other has no
/// hitboxes, focus stops, accessibility nodes or paint callbacks.
pub(super) struct Identity {
    active: AnyElement,
    measured: AnyElement,
    state: State,
    condensed: bool,
}

pub(super) fn identity(pr: &WirePr, state: State, condensed: bool) -> Identity {
    let expanded = pr_identity(pr).into_any_element();
    let compact = compact(pr).into_any_element();
    let (active, measured) = if condensed {
        (compact, expanded)
    } else {
        (expanded, compact)
    };
    Identity {
        active,
        measured: div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .child(measured)
            .into_any_element(),
        state,
        condensed,
    }
}

impl IntoElement for Identity {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Identity {
    type RequestLayoutState = [LayoutId; 2];
    type PrepaintState = ();
    fn id(&self) -> Option<gpui::ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let children = [
            self.active.request_layout(window, cx),
            self.measured.request_layout(window, cx),
        ];
        let mut style = gpui::Style::default();
        style.size.width = gpui::relative(1.).into();
        style.flex_shrink = 0.;
        (window.request_layout(style, children, cx), children)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        layouts: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let active = f32::from(window.layout_bounds(layouts[0]).size.height);
        let measured = f32::from(window.layout_bounds(layouts[1]).size.height);
        let (expanded, compact) = if self.condensed {
            (measured, active)
        } else {
            (active, measured)
        };
        {
            let mut state = self.state.borrow_mut();
            state.expanded_height = expanded;
            state.compact_height = compact;
        }
        self.active.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.active.paint(window, cx);
    }
}

pub(super) struct ObservedBody {
    pub body: AnyElement,
    pub view: WeakEntity<CockpitView>,
    pub target: PrTarget,
    pub tab: PrTab,
    pub scroll: ScrollHandle,
    pub state: State,
}

impl IntoElement for ObservedBody {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for ObservedBody {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<gpui::ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.body.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.body.prepaint(window, cx);
        let transition = {
            let mut state = self.state.borrow_mut();
            if state.queued {
                return;
            }
            let transition = state.transition(self.tab, -f32::from(self.scroll.offset().y));
            state.queued = transition.is_some();
            transition
        };
        let Some((condensed, top)) = transition else {
            return;
        };
        let (view, state, scroll, target, tab) = (
            self.view.clone(),
            self.state.clone(),
            self.scroll.clone(),
            self.target,
            self.tab,
        );
        cx.defer(move |cx| {
            state.borrow_mut().queued = false;
            view.update(cx, |view, cx| {
                let current = view
                    .panel
                    .pr_reads
                    .get(target)
                    .is_some_and(|read| Rc::ptr_eq(&state, &read.header));
                if !current || !view.pr_view_is_shown(target, tab) {
                    return;
                }
                state.borrow_mut().set_condensed(tab, condensed);
                if let Some(top) = top {
                    // Absolute target saved before the viewport grows; do not
                    // subtract a delta from an offset already clamped by layout.
                    scroll.set_offset(gpui::point(scroll.offset().x, px(-top)));
                }
                cx.notify();
            })
            .ok();
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.body.paint(window, cx);
    }
}

fn compact(pr: &WirePr) -> Div {
    use ferrite_ui::theme::*;
    div()
        .debug_selector(|| "pr-identity-compact".into())
        .min_w_0()
        .px(px(16.))
        .pt(px(4.))
        .pb(px(8.))
        .text_size(px(12.))
        .line_height(px(16.))
        .text_color(rgb(INK_SUBTLE))
        .child(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(8.))
                .children(
                    pr.author
                        .as_ref()
                        .map(|author| pr_avatar(author, pr.author_avatar_url.as_deref())),
                )
                .children(pr.updated_at.as_ref().map(|stamp| {
                    let age = stamp_age(stamp);
                    let label = if !age.is_empty() {
                        format!("{age} ago")
                    } else if ferrite_core::clock::parse_stamp(stamp).is_some() {
                        "just now".into()
                    } else {
                        stamp.clone()
                    };
                    div().flex_shrink_0().child(SharedString::from(label))
                }))
                .child(div().flex_shrink_0().h(px(12.)).w(px(1.)).bg(soft_line()))
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .gap(px(6.))
                        .font_family(FONT_CODE)
                        .text_size(px(11.))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .child(SharedString::from(pr.base.clone())),
                        )
                        .child("←")
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(SharedString::from(pr.head.clone())),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(11.))
                        .children(pr.changed_files.map(|count| {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .child(icon(icons::DIFF, 12., INK_SUBTLE))
                                .child(SharedString::from(count.to_string()))
                        }))
                        .children(pr.additions.map(|count| {
                            div()
                                .font_family(FONT_CODE)
                                .text_color(rgb(GREEN))
                                .child(SharedString::from(format!("+{count}")))
                        }))
                        .children(pr.deletions.map(|count| {
                            div()
                                .font_family(FONT_CODE)
                                .text_color(rgb(RED))
                                .child(SharedString::from(format!("−{count}")))
                        })),
                ),
        )
}
