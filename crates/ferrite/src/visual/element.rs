//! The GPUI side: a web page drawn inline as an ordinary image.
//!
//! [`WebView`] is an entity you put anywhere in a GPUI tree (a list row, a
//! popover, a pane). It owns one engine [`View`] and hides everything about it:
//!
//! - **Opening and sizing.** The view opens on the first layout that knows the
//!   element's width, at that width and the window's scale factor, and is
//!   resized whenever either changes. The element is as tall as the page says
//!   its content is ([`Updates::content_height`]); until then, a placeholder.
//! - **Frames.** The engine's `wake` notifies the entity; render takes the
//!   updates and turns a new frame into a `RenderImage`, painted at the frame's
//!   own logical size from the element's top-left (so a frame that lags a
//!   resize is clipped or short, never stretched). Replaced frames are dropped
//!   from the sprite atlas on the next paint.
//! - **Input.** Mouse move, down and up over the element are forwarded in view
//!   coordinates; leaving it (or the window) sends `Leave`. The wheel is not
//!   forwarded: the surrounding list scrolls. The cursor follows the page.
//! - **Visibility.** The view is live (visible to the engine) while the
//!   element is painted *or* the host says it is near ([`WebView::set_reach`]).
//!   When a frame is drawn without the element (a virtualized list scrolled it
//!   away) and it isn't near, the view is told it is hidden; the engine may
//!   then release it after a grace period and reload it when shown again. The
//!   last frame is kept and drawn meanwhile, so a reload shows the old frame
//!   until the new one arrives: no placeholder, and the height stays the last
//!   one the page reported.
//! - **Pre-opening.** A view the host marks near is opened (or shown again)
//!   before it is ever painted, at the width the host says it will have, so by
//!   the time it scrolls in its frame and height are already there. Updates
//!   are taken as they arrive, painted or not, and a content height change is
//!   announced with [`HeightChanged`] so a list can remeasure the row while it
//!   is still off-screen.
//! - **Pointer capture.** A press inside the view keeps `Move` and `Up` going
//!   to the page until the button is released, wherever the pointer goes, so
//!   a drag never leaves the page thinking a button is still down.
//!
//! Ported from `spikes/inline-cef/src/element.rs` (its measurements are in
//! that spike's NOTES-host.md). [`pump`] drives the engine; start it once.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

use futures::channel::mpsc;
use futures::StreamExt;
use gpui::{
    fill, hsla, px, relative, size, App, AppContext, Bounds, Context, Corners, CursorStyle,
    DispatchPhase, Element, ElementId, Entity, EventEmitter, GlobalElementId, Hitbox, HitboxBehavior,
    InspectorElementId, IntoElement, LayoutId, MouseDownEvent, MouseExitEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Render, RenderImage, Style, Window,
};

use crate::visual::web::{self, Cursor, Engine, Frame, Input, Page, View, Viewport};

/// The element's height before the page has reported its own.
pub const PLACEHOLDER_HEIGHT: f32 = 240.0;
/// The pump never sleeps less than this, whatever the engine asks.
const MIN_PUMP: Duration = Duration::from_millis(1);

/// A web page drawn inline. See the module docs.
pub struct WebView {
    inner: Rc<Inner>,
    _wake: gpui::Task<()>,
}

/// How close to the screen the host says the view is. A list knows; the view
/// can't, since an element that isn't painted is never laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reach {
    /// Off-screen and not about to come back: once it isn't painted either,
    /// the engine may release it.
    Far,
    /// About to scroll in (or just scrolled out): keep it live, and open it
    /// now if it never was, at the width it will be laid out at.
    Near { width: Pixels },
}

/// Emitted when the page's content height changes, painted or not. A
/// virtualized list should remeasure the row, so that a view that loaded
/// off-screen doesn't change height under the user when it scrolls in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeightChanged;

impl EventEmitter<HeightChanged> for WebView {}

impl WebView {
    pub fn new(engine: Rc<dyn Engine>, page: Page, cx: &mut App) -> Entity<WebView> {
        cx.new(|cx| {
            let (tx, mut rx) = mpsc::unbounded::<()>();
            let inner = Rc::new(Inner {
                engine,
                page: RefCell::new(page),
                view: RefCell::new(None),
                tx,
                wake_pending: Rc::new(Cell::new(false)),
                viewport: Cell::new(None),
                visible: Cell::new(false),
                painted: Cell::new(false),
                near: Cell::new(false),
                content_height: Cell::new(None),
                cursor: Cell::new(Cursor::Arrow),
                image: RefCell::new(None),
                retired: RefCell::new(Vec::new()),
                hovered: Cell::new(false),
                pressed: Cell::new(false),
                frames: Cell::new(0),
            });
            // `wake` is a plain Fn the engine may call from anywhere on the main
            // thread (even inside one of our own calls into it), so it only
            // queues; this task takes the updates (painted or not, so an
            // off-screen view is current when it scrolls in) and notifies.
            let pending = inner.wake_pending.clone();
            let task = cx.spawn(async move |this, cx| {
                while rx.next().await.is_some() {
                    pending.set(false);
                    let updated = this.update(cx, |web: &mut WebView, cx| {
                        if web.inner.take_updates() {
                            cx.emit(HeightChanged);
                        }
                        cx.notify();
                    });
                    if updated.is_err() {
                        break;
                    }
                }
            });
            WebView { inner, _wake: task }
        })
    }

    /// Restyle the live page.
    pub fn set_theme(&mut self, theme_css: String, cx: &mut Context<Self>) {
        if self.inner.page.borrow().theme_css == theme_css {
            return;
        }
        self.inner.page.borrow_mut().theme_css = theme_css.clone();
        if let Some(view) = self.inner.view.borrow().as_ref() {
            view.set_theme(&theme_css);
        }
        cx.notify();
    }

    /// Replace the page's HTML (see `View::set_html`): the page reloads, and
    /// the last frame stays up until the new one is laid out.
    pub fn set_html(&mut self, html: String, cx: &mut Context<Self>) {
        if self.inner.page.borrow().html == html {
            return;
        }
        self.inner.page.borrow_mut().html = html.clone();
        if let Some(view) = self.inner.view.borrow().as_ref() {
            view.set_html(&html);
        }
        cx.notify();
    }

    /// Say how close to the screen the view is (see [`Reach`]). Call it as the
    /// view scrolls; it only acts on changes.
    pub fn set_reach(&mut self, reach: Reach, window: &Window, _cx: &mut Context<Self>) {
        match reach {
            Reach::Near { width } => {
                self.inner.near.set(true);
                let width = f32::from(width).round();
                if width >= 1.0 && !self.inner.painted.get() {
                    self.inner.show_at(Viewport { width: width as u32, scale: window.scale_factor() });
                }
            }
            Reach::Far => {
                self.inner.near.set(false);
                if !self.inner.painted.get() {
                    self.inner.hide();
                }
            }
        }
    }

    /// Frames taken from the engine so far.
    #[cfg(test)]
    pub fn frames(&self) -> u64 {
        self.inner.frames.get()
    }
}

impl Render for WebView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.inner.take_updates() {
            cx.emit(HeightChanged);
        }
        WebViewElement { inner: self.inner.clone() }
    }
}

struct Inner {
    engine: Rc<dyn Engine>,
    page: RefCell<Page>,
    view: RefCell<Option<Box<dyn View>>>,
    tx: mpsc::UnboundedSender<()>,
    wake_pending: Rc<Cell<bool>>,
    /// What the view was last opened or resized to.
    viewport: Cell<Option<Viewport>>,
    /// What the view was last told about its visibility.
    visible: Cell<bool>,
    /// The element was painted in the last frame (its `Presence` lives).
    painted: Cell<bool>,
    /// The host says the view is near the screen ([`Reach::Near`]).
    near: Cell<bool>,
    content_height: Cell<Option<u32>>,
    cursor: Cell<Cursor>,
    image: RefCell<Option<Shown>>,
    /// Replaced images, dropped from the atlas on the next paint.
    retired: RefCell<Vec<Arc<RenderImage>>>,
    hovered: Cell<bool>,
    /// A button went down over the view and hasn't come up: moves and the
    /// release go to the page wherever the pointer is.
    pressed: Cell<bool>,
    frames: Cell<u64>,
}

/// The frame on screen.
struct Shown {
    image: Arc<RenderImage>,
    /// Device pixels per logical pixel when it was taken.
    scale: f32,
}

impl Inner {
    fn wake(&self) -> web::Wake {
        let tx = self.tx.clone();
        let pending = self.wake_pending.clone();
        Box::new(move || {
            if !pending.replace(true) {
                let _ = tx.unbounded_send(());
            }
        })
    }

    /// Take what the engine has; true if the content height changed.
    fn take_updates(&self) -> bool {
        let updates = match self.view.borrow().as_ref() {
            Some(view) => view.take(),
            None => return false,
        };
        let mut height_changed = false;
        if let Some(height) = updates.content_height {
            height_changed = self.content_height.replace(Some(height)) != Some(height);
        }
        if let Some(cursor) = updates.cursor {
            self.cursor.set(cursor);
        }
        if let Some(frame) = updates.frame {
            self.frames.set(self.frames.get() + 1);
            FRAMES_TAKEN.with(|n| n.set(n.get() + 1));
            let scale = self.viewport.get().map_or(1.0, |v| v.scale);
            let shown = Shown { image: to_image(frame), scale };
            if let Some(old) = self.image.borrow_mut().replace(shown) {
                self.retired.borrow_mut().push(old.image);
            }
        }
        height_changed
    }

    /// Open the view, or bring it to `viewport`; and mark it visible.
    fn show_at(&self, viewport: Viewport) {
        let mut view = self.view.borrow_mut();
        match view.as_ref() {
            None => {
                let page = self.page.borrow().clone();
                *view = Some(self.engine.open(page, viewport, self.wake()));
                self.viewport.set(Some(viewport));
                self.visible.set(true);
            }
            Some(open) => {
                if self.viewport.get() != Some(viewport) {
                        open.resize(viewport);
                    self.viewport.set(Some(viewport));
                }
                if !self.visible.get() {
                        open.set_visible(true);
                    self.visible.set(true);
                }
            }
        }
    }

    /// The element stopped being painted. The view stays live if near.
    fn unpainted(&self) {
        self.painted.set(false);
        if self.hovered.replace(false) {
            self.input(Input::Leave);
        }
        if !self.near.get() {
            self.hide();
        }
    }

    fn hide(&self) {
        if !self.visible.replace(false) {
            return;
        }
        if let Some(view) = self.view.borrow().as_ref() {
            view.set_visible(false);
        }
    }

    fn input(&self, input: Input) {
        let view = self.view.borrow();
        let Some(view) = view.as_ref() else { return };
        view.input(input);
    }

    fn height(&self) -> f32 {
        self.content_height.get().map_or(PLACEHOLDER_HEIGHT, |h| h as f32)
    }
}

/// Lives in the window's element state while the element is painted. Element
/// state nobody touched during a frame is dropped when that frame is done, so
/// this drops exactly when a frame was drawn without the web view.
struct Presence(Weak<Inner>);

impl Drop for Presence {
    fn drop(&mut self) {
        if let Some(inner) = self.0.upgrade() {
            inner.unpainted();
        }
    }
}

struct WebViewElement {
    inner: Rc<Inner>,
}

impl IntoElement for WebViewElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for WebViewElement {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::Name("web-view".into()))
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(self.inner.height()).into();
        style.flex_shrink = 0.;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) -> Hitbox {
        let width = f32::from(bounds.size.width).round();
        if width >= 1.0 {
            self.inner.painted.set(true);
            self.inner.show_at(Viewport { width: width as u32, scale: window.scale_factor() });
        }
        let weak = Rc::downgrade(&self.inner);
        window.with_element_state::<Presence, _>(id.expect("web views have an id"), |presence, _| {
            ((), presence.unwrap_or_else(|| Presence(weak)))
        });
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        hitbox: &mut Hitbox,
        window: &mut Window,
        _cx: &mut App,
    ) {
        for old in self.inner.retired.borrow_mut().drain(..) {
            let _ = window.drop_image(old);
        }

        match self.inner.image.borrow().as_ref() {
            Some(shown) => {
                let image_size = shown.image.size(0);
                let logical = size(
                    px(image_size.width.0 as f32 / shown.scale),
                    px(image_size.height.0 as f32 / shown.scale),
                );
                let image_bounds = Bounds { origin: bounds.origin, size: logical };
                let _ = window.paint_image(
                    bounds,
                    image_bounds,
                    Corners::default(),
                    shown.image.clone(),
                    0,
                    false,
                );
            }
            None => window.paint_quad(fill(bounds, hsla(0., 0., 0.5, 0.06))),
        }

        window.set_cursor_style(cursor_style(self.inner.cursor.get()), hitbox);

        let origin = bounds.origin;
        let local = move |p: Point<Pixels>| {
            let p = p - origin;
            (f32::from(p.x), f32::from(p.y))
        };

        let inner = self.inner.clone();
        let hb = hitbox.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            if hb.is_hovered(window) || inner.pressed.get() {
                inner.hovered.set(true);
                let (x, y) = local(event.position);
                inner.input(Input::Move { x, y, modifiers: modifiers(event.modifiers) });
            } else if inner.hovered.replace(false) {
                inner.input(Input::Leave);
            }
        });

        let inner = self.inner.clone();
        window.on_mouse_event(move |_: &MouseExitEvent, phase, _, _| {
            if phase == DispatchPhase::Bubble && inner.hovered.replace(false) {
                inner.input(Input::Leave);
            }
        });

        let inner = self.inner.clone();
        let hb = hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _| {
            if phase != DispatchPhase::Bubble || !hb.is_hovered(window) {
                return;
            }
            let Some(button) = button(event.button) else { return };
            inner.pressed.set(true);
            let (x, y) = local(event.position);
            inner.input(Input::Down {
                x,
                y,
                button,
                clicks: event.click_count as u32,
                modifiers: modifiers(event.modifiers),
            });
        });

        let inner = self.inner.clone();
        let hb = hitbox.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, _| {
            let captured = inner.pressed.replace(false);
            if phase != DispatchPhase::Bubble || !(captured || hb.is_hovered(window)) {
                return;
            }
            let Some(button) = button(event.button) else { return };
            let (x, y) = local(event.position);
            inner.input(Input::Up {
                x,
                y,
                button,
                clicks: event.click_count as u32,
                modifiers: modifiers(event.modifiers),
            });
        });
    }
}

/// BGRA bytes are exactly what `RenderImage` holds (it is BGRA despite the
/// `Rgba` buffer type), so this is a move, not a conversion.
fn to_image(frame: Frame) -> Arc<RenderImage> {
    let buffer = image::RgbaImage::from_raw(frame.width, frame.height, frame.bgra)
        .expect("a frame is width * height * 4 bytes");
    Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}

fn cursor_style(cursor: Cursor) -> CursorStyle {
    match cursor {
        Cursor::Arrow => CursorStyle::Arrow,
        Cursor::Pointer => CursorStyle::PointingHand,
        Cursor::Text => CursorStyle::IBeam,
        Cursor::Grab => CursorStyle::OpenHand,
        Cursor::Grabbing => CursorStyle::ClosedHand,
        Cursor::Crosshair => CursorStyle::Crosshair,
        Cursor::NotAllowed => CursorStyle::OperationNotAllowed,
    }
}

fn button(button: gpui::MouseButton) -> Option<web::MouseButton> {
    match button {
        gpui::MouseButton::Left => Some(web::MouseButton::Left),
        gpui::MouseButton::Middle => Some(web::MouseButton::Middle),
        gpui::MouseButton::Right => Some(web::MouseButton::Right),
        _ => None,
    }
}

fn modifiers(m: gpui::Modifiers) -> web::Modifiers {
    web::Modifiers { shift: m.shift, control: m.control, alt: m.alt, command: m.platform }
}

thread_local! {
    /// Frames every web view took, for captures waiting on pages to draw.
    static FRAMES_TAKEN: Cell<u64> = const { Cell::new(0) };
}

/// Frames every web view on this thread has taken so far.
#[allow(dead_code)]
pub fn frames_taken() -> u64 {
    FRAMES_TAKEN.with(Cell::get)
}

/// Drive `engine` for the life of the app: call [`Engine::pump`], sleep for as
/// long as it asks, repeat. Start it once. The pump runs in a foreground task,
/// never inside a GPUI update: CEF's message-loop work may spin the native run
/// loop, and GPUI callbacks would then re-enter a borrowed `App`.
pub fn pump(engine: Rc<dyn Engine>, cx: &mut App) {
    cx.spawn(async move |cx| loop {
        let wait = engine.pump().max(MIN_PUMP);
        cx.background_executor().timer(wait).await;
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visual::web::fake::{Call, FakeEngine, CONTENT_HEIGHT};
    use gpui::{div, point, Modifiers, ParentElement, Styled, TestAppContext, VisualTestContext};

    const LEFT: f32 = 30.;
    const TOP: f32 = 50.;

    struct Host {
        web: Entity<WebView>,
        shown: bool,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let root = div().size_full().flex().flex_col().pl(px(LEFT)).pr(px(10.)).pt(px(TOP));
            if self.shown { root.child(self.web.clone()) } else { root }
        }
    }

    fn fixture(
        cx: &mut TestAppContext,
    ) -> (Rc<FakeEngine>, Entity<Host>, &mut VisualTestContext) {
        fixture_with(cx, FakeEngine::new(), true)
    }

    fn fixture_with(
        cx: &mut TestAppContext,
        engine: FakeEngine,
        shown: bool,
    ) -> (Rc<FakeEngine>, Entity<Host>, &mut VisualTestContext) {
        let engine = Rc::new(engine);
        let dyn_engine: Rc<dyn Engine> = engine.clone();
        let page = Page { html: String::new(), theme_css: ":root{--ferrite-bg:#000}".into() };
        let (host, cx) = cx.add_window_view(|_, cx| Host { web: WebView::new(dyn_engine, page, cx), shown });
        cx.simulate_resize(size(px(400.), px(600.)));
        cx.run_until_parked();
        (engine, host, cx)
    }

    fn set_shown(host: &Entity<Host>, shown: bool, cx: &mut VisualTestContext) {
        host.update(cx, |h, cx| {
            h.shown = shown;
            cx.notify();
        });
        cx.run_until_parked();
    }

    fn set_reach(host: &Entity<Host>, reach: Reach, cx: &mut VisualTestContext) {
        let web = host.read_with(cx, |h, _| h.web.clone());
        cx.update(|window, cx| web.update(cx, |w, cx| w.set_reach(reach, window, cx)));
        cx.run_until_parked();
    }

    /// The element's frame and height, as it would draw them now.
    fn on_screen(host: &Entity<Host>, cx: &mut VisualTestContext) -> (Option<Arc<RenderImage>>, f32) {
        let web = host.read_with(cx, |h, _| h.web.clone());
        cx.update(|_, cx| {
            let inner = web.read(cx).inner.clone();
            let image = inner.image.borrow().as_ref().map(|s| s.image.clone());
            (image, inner.height())
        })
    }

    /// The width the fixture lays the view out at.
    const LAID_OUT: f32 = 400. - LEFT - 10.;

    fn opened(engine: &FakeEngine) -> Viewport {
        engine
            .journal()
            .iter()
            .find_map(|c| match c {
                Call::Open(v) => Some(*v),
                _ => None,
            })
            .expect("the element opened a view")
    }

    fn inputs(engine: &FakeEngine) -> Vec<Input> {
        engine
            .journal()
            .into_iter()
            .filter_map(|c| match c {
                Call::Input(i) => Some(i),
                _ => None,
            })
            .collect()
    }

    #[gpui::test]
    fn it_opens_a_view_at_its_width_and_draws_the_first_frame(cx: &mut TestAppContext) {
        let (engine, host, cx) = fixture(cx);
        let scale = cx.update(|window, _| window.scale_factor());
        let viewport = engine
            .journal()
            .iter()
            .rev()
            .find_map(|c| match c {
                Call::Open(v) | Call::Resize(v) => Some(*v),
                _ => None,
            })
            .unwrap();
        assert_eq!(viewport, Viewport { width: 400 - 30 - 10, scale });
        let web = host.read_with(cx, |h, _| h.web.clone());
        assert!(web.read_with(cx, |w, _| w.frames()) >= 1);
        let (height, painted) = cx.update(|window, cx| {
            let inner = web.read(cx).inner.clone();
            let image = inner.image.borrow().as_ref().unwrap().image.clone();
            (inner.height(), window.has_image_atlas_entry(&image))
        });
        assert_eq!(height, CONTENT_HEIGHT as f32, "sized by the page, not the placeholder");
        assert!(painted, "the frame was uploaded and drawn");
    }

    #[gpui::test]
    fn it_forwards_moves_in_view_coordinates_and_leaves(cx: &mut TestAppContext) {
        let (engine, _host, cx) = fixture(cx);
        let _ = opened(&engine);
        cx.simulate_mouse_move(point(px(LEFT + 15.), px(TOP + 20.)), None, Modifiers::default());
        cx.simulate_mouse_move(point(px(5.), px(5.)), None, Modifiers::default());
        assert_eq!(
            inputs(&engine),
            [
                Input::Move { x: 15., y: 20., modifiers: web::Modifiers::default() },
                Input::Leave,
            ]
        );
    }

    #[gpui::test]
    fn clicks_carry_view_coordinates_and_count(cx: &mut TestAppContext) {
        let (engine, _host, cx) = fixture(cx);
        cx.simulate_click(point(px(LEFT + 40.), px(TOP + 2.)), Modifiers::default());
        let clicks: Vec<_> =
            inputs(&engine).into_iter().filter(|i| !matches!(i, Input::Move { .. })).collect();
        let m = web::Modifiers::default();
        assert_eq!(
            clicks,
            [
                Input::Down { x: 40., y: 2., button: web::MouseButton::Left, clicks: 1, modifiers: m },
                Input::Up { x: 40., y: 2., button: web::MouseButton::Left, clicks: 1, modifiers: m },
            ]
        );
    }

    #[gpui::test]
    fn it_hides_the_view_when_not_drawn_and_shows_it_again(cx: &mut TestAppContext) {
        let (engine, host, cx) = fixture(cx);
        host.update(cx, |h, cx| {
            h.shown = false;
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(engine.journal().last(), Some(&Call::Visible(false)));
        host.update(cx, |h, cx| {
            h.shown = true;
            cx.notify();
        });
        cx.run_until_parked();
        assert!(engine.journal().contains(&Call::Visible(true)));
        assert!(!engine.journal().contains(&Call::Close), "hidden, not closed");
    }

    #[gpui::test]
    fn a_near_view_opens_before_it_is_painted_with_its_height_ready(cx: &mut TestAppContext) {
        let (engine, host, cx) = fixture_with(cx, FakeEngine::new(), false);
        assert!(engine.journal().is_empty(), "not painted, not near: nothing opened");
        let web = host.read_with(cx, |h, _| h.web.clone());
        let heights = Rc::new(Cell::new(0));
        let counter = heights.clone();
        let _subscription = cx.update(|_, cx| cx.subscribe(&web, move |_, _: &HeightChanged, _| counter.set(counter.get() + 1)));

        set_reach(&host, Reach::Near { width: px(LAID_OUT) }, cx);
        let scale = cx.update(|window, _| window.scale_factor());
        assert_eq!(engine.journal(), [Call::Open(Viewport { width: LAID_OUT as u32, scale })]);
        let (image, height) = on_screen(&host, cx);
        assert!(image.is_some(), "the first frame is taken before the view is ever painted");
        assert_eq!(height, CONTENT_HEIGHT as f32);
        assert_eq!(heights.get(), 1, "the list is told to remeasure the row");

        // Scrolling in at that width costs nothing: no resize, no new frame.
        set_shown(&host, true, cx);
        assert_eq!(engine.journal().len(), 1, "{:?}", engine.journal());
        assert!(Arc::ptr_eq(&on_screen(&host, cx).0.unwrap(), &image.unwrap()));
    }

    #[gpui::test]
    fn a_near_view_stays_live_when_scrolled_out_and_a_far_one_is_hidden(cx: &mut TestAppContext) {
        let (engine, host, cx) = fixture(cx);
        set_reach(&host, Reach::Near { width: px(LAID_OUT) }, cx);
        set_shown(&host, false, cx);
        assert!(!engine.journal().contains(&Call::Visible(false)), "near: still live");
        set_reach(&host, Reach::Far, cx);
        assert_eq!(engine.journal().last(), Some(&Call::Visible(false)));
        // Painted again while far: shown (painting wins).
        set_shown(&host, true, cx);
        assert_eq!(engine.journal().last(), Some(&Call::Visible(true)));
    }

    #[gpui::test]
    fn a_released_view_keeps_its_frame_and_height_until_it_reloads(cx: &mut TestAppContext) {
        let (engine, host, cx) = fixture_with(cx, FakeEngine::with_grace(Duration::ZERO), true);
        let (frozen, height) = on_screen(&host, cx);
        set_shown(&host, false, cx);
        engine.pump();
        assert!(engine.journal().contains(&Call::Released));
        assert_eq!(engine.live_views(), 0);
        let (image, kept) = on_screen(&host, cx);
        assert!(Arc::ptr_eq(&image.unwrap(), frozen.as_ref().unwrap()), "the last frame is kept");
        assert_eq!(kept, height);

        // Pre-opening brings it back before it is painted.
        set_reach(&host, Reach::Near { width: px(LAID_OUT) }, cx);
        assert_eq!(engine.journal().last(), Some(&Call::Reopened));
        assert_eq!(engine.live_views(), 1);
        let (reloaded, after) = on_screen(&host, cx);
        assert!(!Arc::ptr_eq(&reloaded.unwrap(), &frozen.unwrap()), "a fresh frame replaced it");
        assert_eq!(after, height, "no height jump");
        assert!(!engine.journal().contains(&Call::Close));
    }

    #[gpui::test]
    fn steady_redraws_neither_flap_visibility_nor_ask_for_frames(cx: &mut TestAppContext) {
        let (engine, host, cx) = fixture(cx);
        let web = host.read_with(cx, |h, _| h.web.clone());
        let frames = web.read_with(cx, |w, _| w.frames());
        let calls = engine.journal().len();
        for _ in 0..3 {
            host.update(cx, |_, cx| cx.notify());
            cx.run_until_parked();
        }
        assert_eq!(engine.journal().len(), calls, "{:?}", &engine.journal()[calls..]);
        assert_eq!(web.read_with(cx, |w, _| w.frames()), frames);
    }

    #[gpui::test]
    fn window_resizes_resize_the_view(cx: &mut TestAppContext) {
        let (engine, _host, cx) = fixture(cx);
        cx.simulate_resize(size(px(500.), px(600.)));
        cx.run_until_parked();
        let scale = cx.update(|window, _| window.scale_factor());
        assert!(engine.journal().contains(&Call::Resize(Viewport { width: 460, scale })));
    }
}
