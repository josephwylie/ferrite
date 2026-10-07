//! The loops overlay: the window's content is the Cockpit, cached, with the
//! few loops it shows drawn by a view of their own above it.
//!
//! A loop's notify redraws its view and every view above it, and the Cockpit
//! is one large view: the caret's blink alone rebuilt the whole Cockpit a
//! few times a second. Here the Cockpit lays every loop out as today — the
//! focused Composer's text and caret, the spinners, the working caption's
//! shimmer, Ferrite's starting mark — but paints it invisibly, and records
//! where ([`Hosted`], the Composer's caret): its bounds, content mask, text
//! style and opacity. [`LoopsOverlay`], a sibling drawn after the Cockpit,
//! builds the same element again at that place each time the loop changes
//! and declares itself on the pulse clock. A blink redraws the overlay and
//! replays the cached Cockpit; the Cockpit is rebuilt only for what changes
//! it.
//!
//! The overlay paints after the Cockpit's tree and before its `deferred`
//! floats, so a menu, a sheet or a toast still covers a loop under it, and a
//! loop inside a float stays in the float (`Window::drawing_deferred`).
//! Under reduced motion, a held capture, or a drag (whose indicators the
//! Cockpit draws over the board), every loop is drawn in place, as it is
//! with `FERRITE_LOOPS_OVERLAY=0`, which also leaves the Cockpit uncached.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{
    div, prelude::*, AnyElement, AnyView, App, AvailableSpace, Bounds, ContentMask, Context,
    ElementId, Entity, EntityId, GlobalElementId, InspectorElementId, LayoutId, Pixels,
    StyleRefinement, TextStyle, TextStyleRefinement, Window, WindowId,
};

use crate::cockpit::CockpitView;

/// Whether the overlay runs: on unless `FERRITE_LOOPS_OVERLAY=0`.
fn enabled() -> bool {
    #[cfg(test)]
    if let Some(forced) = testing::FORCED.with(std::cell::Cell::get) {
        return forced;
    }
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("FERRITE_LOOPS_OVERLAY").map_or(true, |value| value.trim() != "0")
    })
}

// ---------------------------------------------------------------- the window

/// The kit Root's view: the Cockpit, cached, and the loops over it.
pub struct CockpitWindow {
    cockpit: Entity<CockpitView>,
    overlay: Option<Entity<LoopsOverlay>>,
}

impl CockpitWindow {
    pub fn new(cockpit: Entity<CockpitView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_overlay(cockpit, enabled(), window, cx)
    }

    /// With the overlay, or without it (the Cockpit uncached, every loop
    /// drawn in it): what `FERRITE_LOOPS_OVERLAY` picks, and what a parity
    /// capture sets side by side.
    pub fn with_overlay(
        cockpit: Entity<CockpitView>,
        overlay: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let overlay = overlay.then(|| {
            let overlay = cx.new(|_| LoopsOverlay);
            let id = window.window_handle().window_id();
            let view = overlay.entity_id();
            with_marks(id, |marks| marks.overlay = Some(view));
            cx.on_release(move |_, _| {
                WINDOWS.with(|windows| windows.borrow_mut().remove(&id));
            })
            .detach();
            overlay
        });
        Self { cockpit, overlay }
    }

    /// The Cockpit this window shows.
    pub fn cockpit(&self) -> &Entity<CockpitView> {
        &self.cockpit
    }
}

impl Render for CockpitWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let shell = div().relative().size_full();
        match &self.overlay {
            // Rebuilt only when it, a view in it, or anything it read was
            // notified; the overlay's own redraws replay it.
            Some(overlay) => shell
                .child(
                    AnyView::from(self.cockpit.clone())
                        .cached(StyleRefinement::default().size_full())
                        .tracking_reads(),
                )
                .child(overlay.clone()),
            None => shell.child(self.cockpit.clone()),
        }
    }
}

// ---------------------------------------------------------------- the marks

/// One loop the overlay draws: the element to build, and where the Cockpit
/// laid it out.
struct Mark {
    /// The element's box, unrounded: where the overlay places its copy.
    bounds: Bounds<Pixels>,
    mask: ContentMask<Pixels>,
    text_style: TextStyle,
    /// The opacity it was painted under (the Cockpit's paint fills it in).
    opacity: f32,
    build: Rc<dyn Fn() -> AnyElement>,
}

/// One window's overlay and the loops its Cockpit laid out on its last
/// render.
#[derive(Default)]
struct WindowMarks {
    overlay: Option<EntityId>,
    /// The views whose loops go to the overlay this frame: the Cockpit and
    /// the Composers it renders (never a cached view inside it, whose loops
    /// would be lost the next time the Cockpit renders without it).
    hosts: HashSet<EntityId>,
    marks: Vec<Mark>,
}

thread_local! {
    static WINDOWS: RefCell<HashMap<WindowId, WindowMarks>> = RefCell::new(HashMap::new());
}

fn with_marks<R>(window: WindowId, f: impl FnOnce(&mut WindowMarks) -> R) -> R {
    WINDOWS.with(|windows| f(windows.borrow_mut().entry(window).or_default()))
}

/// `f` on the marks of a window that has an overlay; nothing otherwise.
fn with_overlay_marks(window: &Window, f: impl FnOnce(&mut WindowMarks)) {
    WINDOWS.with(|windows| {
        if let Some(marks) = windows
            .borrow_mut()
            .get_mut(&window.window_handle().window_id())
        {
            f(marks);
        }
    });
}

/// The Cockpit starts rendering: it lays its loops out again, so forget the
/// last render's.
pub fn begin(window: &Window, cockpit: EntityId) {
    with_overlay_marks(window, |marks| {
        marks.marks.clear();
        marks.hosts.clear();
        marks.hosts.insert(cockpit);
    });
}

/// A Composer renders inside the Cockpit: its caret can go to the overlay.
pub fn host(window: &Window, view: EntityId) {
    with_overlay_marks(window, |marks| {
        marks.hosts.insert(view);
    });
}

/// Whether the loop now laid out goes to the overlay: there is one, the view
/// drawing it hosts its loops, the loop moves, and nothing must stay above
/// it (a float it sits in, a drag's indicators).
pub(crate) fn hosting(window: &Window, cx: &App) -> bool {
    if window.drawing_deferred()
        || cx.has_active_drag()
        || crate::motion::reduced_motion(cx)
        || crate::motion::held_loops().is_some()
    {
        return false;
    }
    let view = window.current_view();
    WINDOWS.with(|windows| {
        windows
            .borrow()
            .get(&window.window_handle().window_id())
            .is_some_and(|marks| marks.overlay.is_some() && marks.hosts.contains(&view))
    })
}

/// Record a loop for the overlay, at prepaint; returns its slot, which the
/// paint fills with its opacity.
fn push(window: &Window, mark: Mark) -> usize {
    with_marks(window.window_handle().window_id(), |marks| {
        marks.marks.push(mark);
        marks.marks.len() - 1
    })
}

fn set_opacity(window: &Window, slot: usize, opacity: f32) {
    with_marks(window.window_handle().window_id(), |marks| {
        if let Some(mark) = marks.marks.get_mut(slot) {
            mark.opacity = opacity;
        }
    });
}

/// A full text style as a refinement, so the overlay lays a loop out under
/// exactly the style it inherited in the Cockpit.
fn refinement(style: &TextStyle) -> TextStyleRefinement {
    TextStyleRefinement {
        color: Some(style.color),
        font_family: Some(style.font_family.clone()),
        font_features: Some(style.font_features.clone()),
        font_fallbacks: style.font_fallbacks.clone(),
        font_size: Some(style.font_size),
        line_height: Some(style.line_height),
        font_weight: Some(style.font_weight),
        font_style: Some(style.font_style),
        background_color: style.background_color,
        underline: style.underline.clone(),
        strikethrough: style.strikethrough.clone(),
        white_space: Some(style.white_space),
        text_overflow: style.text_overflow.clone(),
        text_align: Some(style.text_align),
        line_clamp: style.line_clamp,
    }
}

// ---------------------------------------------------------------- hosted

/// An element whose picture moves with a loop (`build` makes it; it reads
/// its phase from the pulse clock as it renders). Hosted by the overlay, the
/// Cockpit lays it out at its current frame, quietly — declaring nothing on
/// the clock — paints it invisibly and records where; the overlay builds
/// and paints the live one there. Otherwise it is just the element.
pub fn hosted(build: impl Fn() -> AnyElement + 'static) -> Hosted {
    Hosted {
        build: Rc::new(build),
        child: None,
        slot: None,
    }
}

pub struct Hosted {
    build: Rc<dyn Fn() -> AnyElement>,
    child: Option<AnyElement>,
    /// The overlay slot it was recorded in, when hosted.
    slot: Option<usize>,
}

impl IntoElement for Hosted {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Hosted {
    /// The child's layout, and whether the overlay hosts it.
    type RequestLayoutState = (LayoutId, bool);
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
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
    ) -> (LayoutId, (LayoutId, bool)) {
        let hosted = hosting(window, cx);
        let mut child = (self.build)();
        // A loop's element reads its phase as it renders, here.
        let layout = if hosted {
            crate::motion::quietly(|| child.request_layout(window, cx))
        } else {
            child.request_layout(window, cx)
        };
        self.child = Some(child);
        (layout, (layout, hosted))
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (layout, hosted): &mut (LayoutId, bool),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
        if *hosted {
            let mark = Mark {
                bounds: window.unsnapped_layout_bounds(*layout),
                mask: window.content_mask(),
                text_style: window.text_style(),
                opacity: 1.0,
                build: self.build.clone(),
            };
            self.slot = Some(push(window, mark));
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (LayoutId, bool),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        match self.slot {
            Some(slot) => {
                set_opacity(window, slot, window.element_opacity());
                window.with_element_opacity(Some(0.0), |window| child.paint(window, cx));
            }
            None => child.paint(window, cx),
        }
    }
}

/// Record a loop the caller paints itself (the Composer's caret and the
/// text under it): `build` makes the element the overlay draws in its place,
/// laid out over `bounds`. Call at prepaint; returns the slot whose opacity
/// [`paint_hosted`] fills in.
pub(crate) fn record(
    window: &Window,
    bounds: Bounds<Pixels>,
    build: impl Fn() -> AnyElement + 'static,
) -> usize {
    push(
        window,
        Mark {
            bounds,
            mask: window.content_mask(),
            text_style: window.text_style(),
            opacity: 1.0,
            build: Rc::new(build),
        },
    )
}

/// How many loops the overlay draws in `window` now (a parity capture's
/// premise).
#[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
pub(crate) fn hosted_marks(window: &Window) -> usize {
    WINDOWS.with(|windows| {
        windows
            .borrow()
            .get(&window.window_handle().window_id())
            .map_or(0, |marks| marks.marks.len())
    })
}

/// At paint, the opacity a recorded loop would have been painted under.
pub(crate) fn paint_hosted(window: &Window, slot: usize) {
    set_opacity(window, slot, window.element_opacity());
}

// ---------------------------------------------------------------- overlay

/// The view that draws the hosted loops, above the Cockpit. It takes no
/// pointer; each of its draws declares itself on the pulse clock through the
/// loops it builds.
pub struct LoopsOverlay;

impl Render for LoopsOverlay {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        testing::OVERLAY_RENDERS.with(|renders| renders.set(renders.get() + 1));
        Marks { built: Vec::new() }
    }
}

/// The hosted loops, built again where the Cockpit laid them out, each
/// with the content mask, text style and opacity it was drawn under there.
struct Marks {
    built: Vec<(AnyElement, ContentMask<Pixels>, TextStyleRefinement, f32)>,
}

impl IntoElement for Marks {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Marks {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
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
        let mut style = gpui::Style::default();
        style.position = gpui::Position::Absolute;
        (window.request_layout(style, None, cx), ())
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
        // The Cockpit before it in the tree has prepainted: its marks are
        // this frame's, or, replayed from its cache, its last render's.
        let marks: Vec<(
            Bounds<Pixels>,
            ContentMask<Pixels>,
            TextStyle,
            f32,
            Rc<dyn Fn() -> AnyElement>,
        )> = with_marks(window.window_handle().window_id(), |marks| {
            marks
                .marks
                .iter()
                .map(|mark| {
                    (
                        mark.bounds,
                        mark.mask.clone(),
                        mark.text_style.clone(),
                        mark.opacity,
                        mark.build.clone(),
                    )
                })
                .collect()
        });
        for (bounds, mask, text_style, opacity, build) in marks {
            let mut element = build();
            let text_style = refinement(&text_style);
            window.with_text_style(Some(text_style.clone()), |window| {
                window.with_content_mask(Some(mask.clone()), |window| {
                    element.layout_as_root(
                        gpui::size(
                            AvailableSpace::Definite(bounds.size.width),
                            AvailableSpace::Definite(bounds.size.height),
                        ),
                        window,
                        cx,
                    );
                    element.prepaint_at(bounds.origin, window, cx);
                });
            });
            self.built.push((element, mask, text_style, opacity));
        }
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
        // The Cockpit painted first; each loop's opacity is now known.
        let opacities: Vec<f32> = with_marks(window.window_handle().window_id(), |marks| {
            marks.marks.iter().map(|mark| mark.opacity).collect()
        });
        for (index, (element, mask, text_style, opacity)) in self.built.iter_mut().enumerate() {
            let opacity = opacities.get(index).copied().unwrap_or(*opacity);
            // An element can read its text style at paint too (the
            // shimmer's line height).
            window.with_text_style(Some(text_style.clone()), |window| {
                window.with_element_opacity(Some(opacity), |window| {
                    window
                        .with_content_mask(Some(mask.clone()), |window| element.paint(window, cx));
                });
            });
        }
    }
}

#[cfg(test)]
pub mod testing {
    use std::cell::Cell;

    thread_local! {
        pub(super) static FORCED: Cell<Option<bool>> = const { Cell::new(None) };
        pub(super) static OVERLAY_RENDERS: Cell<usize> = const { Cell::new(0) };
    }

    /// This test's windows (its thread's) draw every loop in the Cockpit,
    /// uncached: the behaviour before the overlay.
    pub fn disable() {
        FORCED.with(|forced| forced.set(Some(false)));
    }

    /// How many times this thread's overlays have drawn.
    pub fn overlay_renders() -> usize {
        OVERLAY_RENDERS.with(Cell::get)
    }

    /// The Cockpit under a kit Root's view (a test window's root).
    pub fn cockpit(
        view: &gpui::AnyView,
        cx: &gpui::App,
    ) -> gpui::Entity<crate::cockpit::CockpitView> {
        view.clone()
            .downcast::<super::CockpitWindow>()
            .expect("the window's content")
            .read(cx)
            .cockpit()
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{
        div, prelude::*, AnyView, Context, Entity, StyleRefinement, TestAppContext, Window,
    };

    struct Model(usize);

    struct Grandchild(Rc<Cell<usize>>);

    impl Render for Grandchild {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.0.set(self.0.get() + 1);
            div().size_4()
        }
    }

    /// Reads a model it does not mount, and holds a cached view of its own.
    struct Child {
        model: Entity<Model>,
        grandchild: Entity<Grandchild>,
        renders: Rc<Cell<usize>>,
    }

    impl Render for Child {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            let _ = self.model.read(cx).0;
            div().size_full().child(
                AnyView::from(self.grandchild.clone()).cached(StyleRefinement::default().size_4()),
            )
        }
    }

    struct Parent {
        child: Entity<Child>,
        tracking: bool,
    }

    impl Render for Parent {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let cached =
                AnyView::from(self.child.clone()).cached(StyleRefinement::default().size_full());
            div().size_full().child(if self.tracking {
                cached.tracking_reads().into_any_element()
            } else {
                cached.into_any_element()
            })
        }
    }

    type Fixture = (
        Entity<Parent>,
        Entity<Model>,
        Rc<Cell<usize>>,
        Rc<Cell<usize>>,
    );

    fn fixture<'a>(
        tracking: bool,
        cx: &'a mut TestAppContext,
    ) -> (Fixture, &'a mut gpui::VisualTestContext) {
        let (child_renders, grandchild_renders) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        let (counted, grand) = (child_renders.clone(), grandchild_renders.clone());
        let mut model = None;
        let (parent, cx) = cx.add_window_view(|_, cx| {
            let read = cx.new(|_| Model(0));
            model = Some(read.clone());
            let grandchild = cx.new(|_| Grandchild(grand));
            let child = cx.new(|_| Child {
                model: read,
                grandchild,
                renders: counted,
            });
            Parent { child, tracking }
        });
        cx.run_until_parked();
        (
            (parent, model.unwrap(), child_renders, grandchild_renders),
            cx,
        )
    }

    /// The vendored `tracking_reads`: a cached view is redrawn when a model
    /// it read is notified, as it was when its parent redrew it — and the
    /// cached view inside it keeps its own cache. Without it, the model's
    /// notify left the cached view stale: the class of staleness caching the
    /// Cockpit would have opened.
    #[gpui::test]
    fn a_tracking_cached_view_redraws_for_what_it_read_and_keeps_nested_caches(
        cx: &mut TestAppContext,
    ) {
        let ((parent, model, child, grandchild), cx) = fixture(true, cx);
        let (child_before, grand_before) = (child.get(), grandchild.get());
        parent.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        assert_eq!(child.get(), child_before, "its parent's frame replays it");
        model.update(cx, |model, cx| {
            model.0 += 1;
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(
            child.get(),
            child_before + 1,
            "the model it read redraws it"
        );
        assert_eq!(
            grandchild.get(),
            grand_before,
            "and the cached view inside it is reused"
        );
    }

    #[gpui::test]
    fn without_tracking_a_cached_view_misses_what_it_read(cx: &mut TestAppContext) {
        let ((_parent, model, child, _), cx) = fixture(false, cx);
        let before = child.get();
        model.update(cx, |model, cx| {
            model.0 += 1;
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(child.get(), before, "stale: what tracking_reads closes");
    }
}
