//! The image viewer: an image in the reader slot beside its Pane
//! (`attachment_preview::Body::Image`; the board tree lays the slot out and
//! the reader's shell gives it its head, the file's name, the original a
//! click away and the ×). Every way an image opens comes through
//! `Preview::open_image`: a sent image under its prompt or the prompt's
//! chip, a chip on the Composer's shelf, a file link in an answer, a
//! ⌘-clicked path. Another image swaps in place; ← → step among the images
//! it was opened with (a prompt's); esc closes the slot.
//!
//! The image lies centred on the reader's ground, fitted inside
//! `IMAGE_INSET` and never drawn past its own size (an image pixel to a
//! point), at `R_CHIP` with a 1px white-at-10% ring
//! (`paint::HAIRLINE_STRONG`) for its edge on the dark ground. A click
//! toggles fit and actual size, holding the point under the pointer still;
//! ⌘= and ⌘- step through `IMAGE_ZOOM_STOPS` and ⌘0 fits again — each easing
//! the scale over `motion::IMAGE_ZOOM`, interruptibly (a zoom mid-flight
//! turns around from the scale on screen). A pinch zooms around the fingers
//! and a drag or a scroll pans an image larger than the slot, following the
//! hand with no ease. `div` has no transform, so a zoom lays the picture out
//! afresh: a canvas measures the slot and places the `img` at the zoom's
//! rect in its prepaint, so the picture never trails a slot being resized.
//!
//! The decode is gpui's (`ImgResourceLoader`, the cache the transcript's
//! thumbnails fill, so an image opened from one is usually ready and
//! arrives with its slot). One that lands after the slot is up fades in
//! over `motion::FADE_QUICK`; one that replaces another crossfades with it,
//! the old picture held where it lay until the new one is ready, so the
//! slot never shows an empty frame between them; stepping on mid-crossfade
//! holds whichever of the two shows more. A file that cannot be
//! read says so quietly, with the original a click away. The view asks for
//! frames only while a fade or a zoom runs.

use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};

use futures::FutureExt as _;
use gpui::{
    canvas, div, point, prelude::*, px, rgb, size, AnyElement, App, AvailableSpace, Bounds,
    Context, CursorStyle, DispatchPhase, FocusHandle, Focusable, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PinchEvent, Pixels, Point, ScrollWheelEvent, Size, WeakEntity,
    Window,
};

use crate::motion::{self, Tween};
use crate::theme;

gpui::actions!(
    image_viewer,
    [
        // In the `ImageViewer` context: zoom in or out a stop, fit again,
        // step to the next or the previous image. Esc is the reader's
        // (`reader::Close`), around it.
        ZoomIn, ZoomOut, Fit, Next, Previous,
    ]
);

/// The viewer's key context: deeper than the Cockpit's, so ⌘= ⌘- ⌘0 zoom
/// the image rather than the reading size and esc closes the slot rather
/// than interrupting the turn.
pub(crate) const KEY_CONTEXT: &str = "ImageViewer";

/// Whether gpui draws the file as an image: by its extension, as `img`
/// decides.
pub(crate) fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            gpui::Img::extensions().contains(&extension.to_ascii_lowercase().as_str())
        })
}

pub(crate) struct ImageViewer {
    focus: FocusHandle,
    /// The image shown.
    path: PathBuf,
    /// The picture `path` replaced, held where it last lay — at the
    /// opacity it had then — until the new one has faded in over it.
    leaving: Option<(PathBuf, Bounds<Pixels>, f32)>,
    /// How far `path`'s picture has come in.
    arrival: Arrival,
    zoom: Zoom,
    /// The stage and the picture as last laid out: what a click, a pinch or
    /// a pan is measured against.
    laid: Rc<Cell<Laid>>,
    /// A press on the stage, while the button is down.
    press: Option<Press>,
}

/// The stage and the picture as a frame laid them out.
#[derive(Clone, Copy, Debug, Default)]
struct Laid {
    /// The slot's body, in window coordinates.
    stage: Bounds<Pixels>,
    /// The image's own size, once decoded.
    natural: Option<Size<f32>>,
    /// Where the picture lay.
    picture: Option<Bounds<Pixels>>,
}

impl Laid {
    fn frame(&self) -> Option<Frame> {
        Some(Frame {
            stage: self.stage,
            natural: self.natural?,
        })
    }
}

/// How far the picture shown has come in.
#[derive(Clone, Copy, Debug)]
enum Arrival {
    /// Not drawn yet: its decode is on the way, or it is ready but has yet
    /// to paint over the picture it replaces.
    Awaited,
    /// Fading in (0 → 1) from the first frame that drew it.
    Fading(Tween),
    /// All there.
    Arrived,
}

impl Arrival {
    /// The picture's opacity at `now`.
    fn opacity(self, now: Instant, reduced: bool) -> f32 {
        match self {
            Self::Awaited => 0.0,
            Self::Fading(fade) => fade.value(now, reduced),
            Self::Arrived => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Press {
    from: Point<Pixels>,
    last: Point<Pixels>,
    /// Past `IMAGE_DRAG_SLOP`: a pan, which no release turns into a click.
    travelled: bool,
}

impl ImageViewer {
    pub(crate) fn new(path: PathBuf, cx: &mut Context<Self>) -> Self {
        let mut viewer = Self {
            focus: cx.focus_handle(),
            path,
            leaving: None,
            arrival: Arrival::Awaited,
            zoom: Zoom::default(),
            laid: Rc::default(),
            press: None,
        };
        viewer.load(cx);
        viewer
    }

    /// Show `path` in place of the image on screen, at fit: it crossfades
    /// in over the old picture once it is ready.
    pub(crate) fn show(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if path == self.path {
            return;
        }
        let previous = std::mem::replace(&mut self.path, path);
        // Stepping on mid-crossfade keeps whichever picture shows more;
        // the fainter goes. One not drawn yet shows nothing.
        let now = cx.background_executor().now();
        let shown = self.arrival.opacity(now, motion::reduced_motion(cx));
        let replaced = self.laid.get().picture.map(|rect| (previous, rect, shown));
        let older = self
            .leaving
            .take()
            .map(|(path, rect, from)| (path, rect, from * (1.0 - shown)));
        self.leaving = replaced
            .into_iter()
            .chain(older)
            .max_by(|a, b| a.2.total_cmp(&b.2));
        self.zoom = Zoom::default();
        self.press = None;
        self.load(cx);
        cx.notify();
    }

    /// Start `path`'s decode. Ready already with nothing to replace, it
    /// shows at once; otherwise it fades in from the first frame that has
    /// it (`render`), redrawn when it lands.
    fn load(&mut self, cx: &mut Context<Self>) {
        let (decode, _) = cx.fetch_asset::<gpui::ImgResourceLoader>(&resource(&self.path));
        let ready = decode.clone().now_or_never().is_some();
        self.arrival = if ready && self.leaving.is_none() {
            Arrival::Arrived
        } else {
            Arrival::Awaited
        };
        if !ready {
            // `use_asset` would redraw only the view that asked first, and
            // a transcript thumbnail may have.
            cx.spawn(async move |viewer, cx| {
                let _ = decode.await;
                viewer.update(cx, |_, cx| cx.notify()).ok();
            })
            .detach();
        }
    }

    /// The keyboard focus it holds, kept from its first frame: the focus
    /// snap leaves it here while it does.
    pub(crate) fn held_focus(&self, window: &Window) -> Option<FocusHandle> {
        self.focus.is_focused(window).then(|| self.focus.clone())
    }

    /// The zoom's frame as last laid out, the clock and the motion flag.
    fn moment(&self, cx: &App) -> Option<(Frame, Instant, bool)> {
        Some((
            self.laid.get().frame()?,
            cx.background_executor().now(),
            motion::reduced_motion(cx),
        ))
    }

    /// Change the zoom by `change`, against the stage as last laid out;
    /// redrawn if it moved anything.
    fn zoom_with(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Zoom, &Frame, Instant, bool),
    ) -> bool {
        let Some((frame, now, reduced)) = self.moment(cx) else {
            return false;
        };
        let before = self.zoom;
        change(&mut self.zoom, &frame, now, reduced);
        let moved = self.zoom != before;
        if moved {
            cx.notify();
        }
        moved
    }

    fn zoom_in(&mut self, _: &ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        self.zoom_with(cx, |zoom, frame, now, reduced| {
            zoom.step(true, frame, now, reduced)
        });
    }

    fn zoom_out(&mut self, _: &ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.zoom_with(cx, |zoom, frame, now, reduced| {
            zoom.step(false, frame, now, reduced)
        });
    }

    fn fit(&mut self, _: &Fit, _: &mut Window, cx: &mut Context<Self>) {
        self.zoom_with(cx, |zoom, frame, now, reduced| {
            if zoom.target != Scale::Fit {
                zoom.ease_to(Scale::Fit, point(0., 0.), frame, now, reduced);
            }
        });
    }

    fn pressed(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.laid.get().frame().is_none() {
            return;
        }
        self.press = Some(Press {
            from: event.position,
            last: event.position,
            travelled: false,
        });
        // The paint listens to the whole window while the button is down.
        cx.notify();
    }

    /// The pointer moved with the button down: past the slop, the picture
    /// follows it.
    fn dragged(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(mut press) = self.press else {
            return;
        };
        if !press.travelled {
            let travel = offset(position - press.from);
            if travel.x.abs().max(travel.y.abs()) < theme::IMAGE_DRAG_SLOP {
                return;
            }
            press.travelled = true;
            cx.notify();
        }
        let by = position - press.last;
        press.last = position;
        self.press = Some(press);
        self.zoom_with(cx, |zoom, frame, now, reduced| {
            zoom.pan(offset(by), frame, now, reduced)
        });
    }

    /// The button came up: a press that never travelled is a click, which
    /// toggles fit and actual size around the pointer.
    fn released(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(press) = self.press.take() else {
            return;
        };
        cx.notify();
        if press.travelled {
            return;
        }
        let at = position - self.laid.get().stage.center();
        self.zoom_with(cx, |zoom, frame, now, reduced| {
            zoom.toggle(offset(at), frame, now, reduced)
        });
    }

    fn scrolled(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        let by = event.delta.pixel_delta(window.line_height());
        if self.zoom_with(cx, |zoom, frame, now, reduced| {
            zoom.pan(offset(by), frame, now, reduced)
        }) {
            cx.stop_propagation();
        }
    }

    fn pinched(&mut self, event: &PinchEvent, _: &mut Window, cx: &mut Context<Self>) {
        let at = event.position - self.laid.get().stage.center();
        self.zoom_with(cx, |zoom, frame, now, reduced| {
            zoom.pinch(event.delta, offset(at), frame, now, reduced)
        });
    }

    /// What the pointer says over the stage: a hand to pan with over a
    /// picture larger than the slot (closed while it pans), a pointer
    /// where a click would zoom, else the arrow.
    fn cursor(&self) -> CursorStyle {
        let laid = self.laid.get();
        let Some(frame) = laid.frame() else {
            return CursorStyle::Arrow;
        };
        let overflows = laid.picture.is_some_and(|picture| {
            picture.size.width > laid.stage.size.width + px(0.5)
                || picture.size.height > laid.stage.size.height + px(0.5)
        });
        if self.press.is_some_and(|press| press.travelled) {
            CursorStyle::ClosedHand
        } else if overflows {
            CursorStyle::OpenHand
        } else if self.zoom.target != Scale::Fit || frame.fit() < 1.0 {
            CursorStyle::PointingHand
        } else {
            CursorStyle::Arrow
        }
    }

    /// Where the picture lay in the last frame drawn.
    #[cfg(test)]
    pub(crate) fn picture(&self) -> Option<Bounds<Pixels>> {
        self.laid.get().picture
    }

    /// The picture held while the one shown comes in, at the opacity it
    /// fades out from.
    #[cfg(test)]
    pub(crate) fn leaving(&self) -> Option<(PathBuf, f32)> {
        self.leaving
            .as_ref()
            .map(|(path, _, from)| (path.clone(), *from))
    }
}

impl Focusable for ImageViewer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ImageViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = cx.background_executor().now();
        let reduced = motion::reduced_motion(cx);
        // Decoded or failed, the picture starts in with the first frame
        // that has it; one all in lets the old picture go.
        let decoded = window.get_asset::<gpui::ImgResourceLoader>(&resource(&self.path), cx);
        if matches!(self.arrival, Arrival::Awaited) && decoded.is_some() {
            self.arrival = Arrival::Fading(Tween::new(0.0, 1.0, motion::FADE_QUICK, now));
        }
        if let Arrival::Fading(fade) = self.arrival {
            if !fade.running(now, reduced) {
                self.arrival = Arrival::Arrived;
                self.leaving = None;
            }
        }
        if matches!(self.arrival, Arrival::Fading(_)) || self.zoom.running(now, reduced) {
            window.request_animation_frame();
        }
        let shown = self.arrival.opacity(now, reduced);
        let failed = matches!(decoded, Some(Err(_)));
        let current = decoded.and_then(Result::ok).map(|image| {
            let natural = image.size(0);
            (
                self.path.clone(),
                size(natural.width.0 as f32, natural.height.0 as f32),
            )
        });
        // Until the new picture is drawn, the old one holds as it was.
        let leaving = self
            .leaving
            .clone()
            .map(|(path, rect, from)| (path, rect, from * (1.0 - shown)));
        let laid = self.laid.clone();
        let zoom = self.zoom;
        let pictures = canvas(
            move |stage, window, cx| {
                let mut pictures = Vec::new();
                if let Some((path, rect, opacity)) = leaving {
                    pictures.push((picture(path, rect.size, opacity), rect));
                }
                let natural = current.as_ref().map(|(_, natural)| *natural);
                let mut shown_at = None;
                if let Some((path, natural)) = current {
                    let rect = zoom.rect(&Frame { stage, natural }, now, reduced);
                    pictures.push((picture(path, rect.size, shown), rect));
                    shown_at = Some(rect);
                }
                laid.set(Laid {
                    stage,
                    natural,
                    picture: shown_at,
                });
                for (picture, rect) in &mut pictures {
                    picture.prepaint_as_root(
                        rect.origin,
                        rect.size.map(AvailableSpace::Definite),
                        window,
                        cx,
                    );
                }
                pictures
            },
            {
                let viewer = self.press.map(|_| cx.entity().downgrade());
                move |_, pictures: Vec<(AnyElement, Bounds<Pixels>)>, window, cx| {
                    for (mut picture, _) in pictures {
                        picture.paint(window, cx);
                    }
                    if let Some(viewer) = viewer {
                        follow_press(viewer, window);
                    }
                }
            },
        )
        .absolute()
        .inset_0();
        let error = failed.then(|| {
            let original = self.path.clone();
            div()
                .debug_selector(|| "image-viewer-error".into())
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(theme::SPACE_2))
                .opacity(shown)
                .child(
                    div()
                        .text_color(rgb(theme::TEXT_DIM))
                        .child("Could not read this image"),
                )
                .child(
                    crate::components::form_button("image-viewer-open-original", cx)
                        .debug_selector(|| "image-viewer-open-original".into())
                        .h(px(theme::ROW))
                        .px(px(theme::CH))
                        .child(crate::components::form_label(
                            "open original",
                            theme::TEXT_DIM,
                        ))
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            crate::attachment_preview::open_original(&original, window, cx);
                        }),
                )
        });
        div()
            .id("image-viewer")
            .debug_selector(|| "image-viewer".into())
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .cursor(self.cursor())
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
            .on_action(cx.listener(Self::fit))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::pressed))
            // A quick click lands before the paint that follows the press
            // window-wide; the stage hears its release itself.
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|viewer, event: &MouseUpEvent, _, cx| {
                    viewer.released(event.position, cx)
                }),
            )
            .on_scroll_wheel(cx.listener(Self::scrolled))
            .on_pinch(cx.listener(Self::pinched))
            .child(pictures)
            .children(error)
    }
}

/// While the button is down, the pointer anywhere in the window drags the
/// picture and its release ends the press, so a pan carries on past the
/// slot's edge.
fn follow_press(viewer: WeakEntity<ImageViewer>, window: &mut Window) {
    let moved = viewer.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
        if phase == DispatchPhase::Capture && event.pressed_button == Some(MouseButton::Left) {
            moved
                .update(cx, |viewer, cx| viewer.dragged(event.position, cx))
                .ok();
        }
    });
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
        if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
            viewer
                .update(cx, |viewer, cx| viewer.released(event.position, cx))
                .ok();
        }
    });
}

/// The cache key the transcript's `img` thumbnails decode under too.
fn resource(path: &Path) -> gpui::Resource {
    gpui::Resource::from(path.to_path_buf())
}

/// A pointer offset in points, as the zoom reckons it.
fn offset(by: Point<Pixels>) -> Point<f32> {
    point(f32::from(by.x), f32::from(by.y))
}

/// One picture `size` large: the image filling it at `R_CHIP`, its edge
/// ringed just inside by the strong hairline (white at 10%), so it holds
/// its shape on the dark ground whatever its own edge is.
fn picture(path: PathBuf, size: Size<Pixels>, opacity: f32) -> AnyElement {
    let radius = px(theme::R_CHIP);
    let id = gpui::SharedString::from(format!("image-viewer-{}", path.display()));
    div()
        .debug_selector(|| "image-viewer-picture".into())
        .relative()
        .w(size.width)
        .h(size.height)
        .opacity(opacity)
        .child(
            gpui::img(path)
                .object_fit(gpui::ObjectFit::Fill)
                .id(id)
                .size_full()
                .rounded(radius),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(radius)
                .border_1()
                .border_color(theme::paint::HAIRLINE_STRONG),
        )
        .into_any_element()
}

// ---------------------------------------------------------------- the zoom

/// What the scale settles on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Scale {
    /// The image fitted to the stage, never past its own size: it follows
    /// the slot as it resizes.
    Fit,
    /// Points per image pixel (1 is actual size).
    Of(f32),
}

/// An image on its stage: what the zoom is measured against.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Frame {
    /// The slot's body, in window coordinates.
    stage: Bounds<Pixels>,
    /// The image's own size, in pixels.
    natural: Size<f32>,
}

impl Frame {
    /// The scale that fits the image inside the stage's `IMAGE_INSET`,
    /// never past its own size.
    fn fit(&self) -> f32 {
        let room = |side: Pixels| (f32::from(side) - 2.0 * theme::IMAGE_INSET).max(1.0);
        (room(self.stage.size.width) / self.natural.width.max(1.0))
            .min(room(self.stage.size.height) / self.natural.height.max(1.0))
            .min(1.0)
    }

    /// `centre` (the image point at the stage's centre, in fractions of the
    /// image) kept where the picture still covers the stage on an axis it
    /// overflows; on an axis it fits, the picture is centred.
    fn clamp(&self, centre: Point<f32>, scale: f32) -> Point<f32> {
        let axis = |centre: f32, stage: Pixels, natural: f32| {
            let shown = natural * scale;
            let stage = f32::from(stage);
            if shown <= stage {
                return 0.5;
            }
            let half = stage / (2.0 * shown);
            centre.clamp(half, 1.0 - half)
        };
        point(
            axis(centre.x, self.stage.size.width, self.natural.width),
            axis(centre.y, self.stage.size.height, self.natural.height),
        )
    }

    /// Where the picture lies at `scale` with `centre` at the stage's
    /// centre.
    fn lay(&self, scale: f32, centre: Point<f32>) -> Bounds<Pixels> {
        let centre = self.clamp(centre, scale);
        let (width, height) = (self.natural.width * scale, self.natural.height * scale);
        let middle = self.stage.center();
        Bounds {
            origin: point(
                middle.x - px(centre.x * width),
                middle.y - px(centre.y * height),
            ),
            size: size(px(width), px(height)),
        }
    }
}

/// The zoom: a scale and the image point held at a point of the stage.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Zoom {
    target: Scale,
    /// The ease toward it while one runs, on the scale's logarithm.
    tween: Option<Tween>,
    /// The image point (in fractions of the image) held at `at`, an offset
    /// in points from the stage's centre: the point a zoom keeps under the
    /// pointer.
    held: Point<f32>,
    at: Point<f32>,
}

impl Default for Zoom {
    fn default() -> Self {
        Self {
            target: Scale::Fit,
            tween: None,
            held: point(0.5, 0.5),
            at: point(0., 0.),
        }
    }
}

impl Zoom {
    fn resolve(&self, frame: &Frame) -> f32 {
        match self.target {
            Scale::Fit => frame.fit(),
            Scale::Of(scale) => scale,
        }
    }

    fn running(&self, now: Instant, reduced: bool) -> bool {
        self.tween.is_some_and(|tween| tween.running(now, reduced))
    }

    /// The scale on screen.
    fn scale(&self, frame: &Frame, now: Instant, reduced: bool) -> f32 {
        match self.tween {
            Some(tween) if tween.running(now, reduced) => tween.value(now, reduced).exp(),
            _ => self.resolve(frame),
        }
    }

    /// The image point at the stage's centre at `scale`, before clamping.
    fn centre(&self, frame: &Frame, scale: f32) -> Point<f32> {
        point(
            self.held.x - self.at.x / (frame.natural.width * scale),
            self.held.y - self.at.y / (frame.natural.height * scale),
        )
    }

    /// Where the picture lies now.
    fn rect(&self, frame: &Frame, now: Instant, reduced: bool) -> Bounds<Pixels> {
        let scale = self.scale(frame, now, reduced);
        frame.lay(scale, self.centre(frame, scale))
    }

    /// Hold the image point that lies under `at` now (an offset from the
    /// stage's centre) there from here on; the scale on screen.
    fn hold(&mut self, at: Point<f32>, frame: &Frame, now: Instant, reduced: bool) -> f32 {
        let scale = self.scale(frame, now, reduced);
        let centre = frame.clamp(self.centre(frame, scale), scale);
        self.held = point(
            centre.x + at.x / (frame.natural.width * scale),
            centre.y + at.y / (frame.natural.height * scale),
        );
        self.at = at;
        scale
    }

    /// Ease to `to` from the scale on screen, the point under `at` held
    /// still.
    fn ease_to(&mut self, to: Scale, at: Point<f32>, frame: &Frame, now: Instant, reduced: bool) {
        let from = self.hold(at, frame, now, reduced);
        self.target = to;
        let to = self.resolve(frame);
        self.tween = (from != to).then(|| Tween::new(from.ln(), to.ln(), motion::IMAGE_ZOOM, now));
    }

    /// A click at `at`: fit goes to actual size (when the image is larger
    /// than the slot), anything else back to fit.
    fn toggle(&mut self, at: Point<f32>, frame: &Frame, now: Instant, reduced: bool) {
        let to = match self.target {
            Scale::Of(_) => Scale::Fit,
            Scale::Fit if frame.fit() < 1.0 => Scale::Of(1.0),
            Scale::Fit => return,
        };
        self.ease_to(to, at, frame, now, reduced);
    }

    /// ⌘= or ⌘-: the next stop closer or further, around the stage's
    /// centre. Further than the fit is the fit.
    fn step(&mut self, closer: bool, frame: &Frame, now: Instant, reduced: bool) {
        let fit = frame.fit();
        let from = self.resolve(frame);
        let stops = theme::IMAGE_ZOOM_STOPS;
        let to = if closer {
            match stops.into_iter().find(|stop| *stop > from * 1.001) {
                Some(stop) => Scale::Of(stop),
                None => return,
            }
        } else {
            match stops.into_iter().rev().find(|stop| *stop < from * 0.999) {
                Some(stop) if stop > fit => Scale::Of(stop),
                _ => Scale::Fit,
            }
        };
        if to != self.target {
            self.ease_to(to, point(0., 0.), frame, now, reduced);
        }
    }

    /// A pinch: the scale by `by` (0.1 is 10% closer) at once, between the
    /// fit and `IMAGE_ZOOM_MAX`, around the fingers at `at`.
    fn pinch(&mut self, by: f32, at: Point<f32>, frame: &Frame, now: Instant, reduced: bool) {
        let from = self.hold(at, frame, now, reduced);
        let fit = frame.fit();
        let to = (from * (1.0 + by)).clamp(fit, theme::IMAGE_ZOOM_MAX.max(fit));
        self.target = if to <= fit { Scale::Fit } else { Scale::Of(to) };
        self.tween = None;
    }

    /// A drag or a scroll: the picture moves `by` points, as far as it
    /// still covers the stage.
    fn pan(&mut self, by: Point<f32>, frame: &Frame, now: Instant, reduced: bool) {
        let scale = self.hold(point(0., 0.), frame, now, reduced);
        let moved = point(
            self.held.x - by.x / (frame.natural.width * scale),
            self.held.y - by.y / (frame.natural.height * scale),
        );
        self.held = frame.clamp(moved, scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A 1000×700 slot body at the window's origin and an image in it.
    fn frame(width: f32, height: f32) -> Frame {
        Frame {
            stage: Bounds {
                origin: point(px(0.), px(0.)),
                size: size(px(1000.), px(700.)),
            },
            natural: size(width, height),
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    /// Fitted, an image keeps the inset and its own proportions, centred;
    /// one smaller than the room is never drawn past its own size.
    #[test]
    fn the_fit_contains_the_image_and_never_upscales() {
        let wide = frame(2000., 1000.);
        let rect = wide.lay(wide.fit(), point(0.5, 0.5));
        let room = 1000. - 2. * theme::IMAGE_INSET;
        assert!(close(f32::from(rect.size.width), room), "{rect:?}");
        assert!(close(
            f32::from(rect.size.width) / f32::from(rect.size.height),
            2.0
        ));
        assert!(close(f32::from(rect.center().y), 350.));
        let small = frame(200., 100.);
        assert_eq!(small.fit(), 1.0);
        let rect = small.lay(1.0, point(0.9, 0.1));
        assert_eq!(rect.size, size(px(200.), px(100.)));
        assert_eq!(rect.center(), point(px(500.), px(350.)), "centred");
    }

    /// A click toggles fit and actual size and keeps the image point under
    /// the pointer where it was — clamped so the picture still covers the
    /// stage — and back again.
    #[test]
    fn a_click_zooms_to_actual_size_around_the_pointer_and_back() {
        let frame = frame(4000., 2800.);
        let now = Instant::now();
        let mut zoom = Zoom::default();
        let at = point(100., 50.);
        let before = zoom.rect(&frame, now, false);
        let image_point = |rect: Bounds<Pixels>| {
            let pointer = frame.stage.center() + point(px(at.x), px(at.y));
            (
                (pointer.x - rect.origin.x) / rect.size.width,
                (pointer.y - rect.origin.y) / rect.size.height,
            )
        };
        let under = image_point(before);
        zoom.toggle(at, &frame, now, false);
        assert_eq!(zoom.target, Scale::Of(1.0));
        let after = zoom.rect(&frame, now + Duration::from_secs(1), false);
        assert_eq!(after.size, size(px(4000.), px(2800.)), "actual size");
        let held = image_point(after);
        assert!(close(held.0, under.0) && close(held.1, under.1));
        // Mid-flight the same point is held too.
        let mid = zoom.rect(&frame, now + Duration::from_millis(40), false);
        assert!(mid.size.width > before.size.width && mid.size.width < after.size.width);
        let held = image_point(mid);
        assert!(close(held.0, under.0) && close(held.1, under.1));
        zoom.toggle(at, &frame, now + Duration::from_secs(1), false);
        assert_eq!(zoom.target, Scale::Fit);
        let back = zoom.rect(&frame, now + Duration::from_secs(2), false);
        assert_eq!(back, before, "fitted and centred again");
        // An image that already fits has nothing to zoom to.
        let small = self::frame(200., 100.);
        let mut zoom = Zoom::default();
        zoom.toggle(at, &small, now, false);
        assert_eq!(zoom, Zoom::default());
    }

    /// A zoom flipped back mid-flight turns around from the scale on
    /// screen; reduced motion lands at once.
    #[test]
    fn a_zoom_is_interruptible_and_snaps_under_reduced_motion() {
        let frame = frame(4000., 2800.);
        let now = Instant::now();
        let mut zoom = Zoom::default();
        zoom.toggle(point(0., 0.), &frame, now, false);
        let later = now + Duration::from_millis(30);
        let mid = zoom.scale(&frame, later, false);
        assert!(mid > frame.fit() && mid < 1.0, "{mid}");
        zoom.toggle(point(0., 0.), &frame, later, false);
        assert!(close(zoom.scale(&frame, later, false), mid), "no jump");
        assert!(zoom.running(later, false));
        let mut reduced = Zoom::default();
        reduced.toggle(point(0., 0.), &frame, now, true);
        assert_eq!(reduced.scale(&frame, now, true), 1.0);
        assert!(!reduced.running(now, true));
    }

    /// ⌘= and ⌘- walk the stops from the fit, and ⌘- lands back on it.
    #[test]
    fn keys_step_the_zoom_through_its_stops() {
        let frame = frame(4000., 2800.);
        let now = Instant::now();
        let mut zoom = Zoom::default();
        let fit = frame.fit();
        assert!(fit < 0.25, "{fit}");
        zoom.step(true, &frame, now, true);
        assert_eq!(zoom.target, Scale::Of(0.25));
        zoom.step(true, &frame, now, true);
        assert_eq!(zoom.target, Scale::Of(1.0 / 3.0));
        zoom.step(false, &frame, now, true);
        zoom.step(false, &frame, now, true);
        assert_eq!(zoom.target, Scale::Fit);
        for _ in 0..20 {
            zoom.step(true, &frame, now, true);
        }
        assert_eq!(zoom.target, Scale::Of(theme::IMAGE_ZOOM_MAX));
    }

    /// A pan moves a picture larger than the stage only as far as it still
    /// covers it; a fitted picture stays put.
    #[test]
    fn a_pan_stops_at_the_pictures_edges() {
        let frame = frame(4000., 2800.);
        let now = Instant::now();
        let mut zoom = Zoom {
            target: Scale::Of(1.0),
            ..Zoom::default()
        };
        zoom.pan(point(100_000., 100_000.), &frame, now, false);
        let rect = zoom.rect(&frame, now, false);
        assert_eq!(rect.origin, point(px(0.), px(0.)), "its top left corner");
        zoom.pan(point(-100_000., 0.), &frame, now, false);
        let rect = zoom.rect(&frame, now, false);
        assert_eq!(rect.right(), px(1000.), "its right edge");
        let mut fitted = Zoom::default();
        let before = fitted.rect(&frame, now, false);
        fitted.pan(point(80., 40.), &frame, now, false);
        assert_eq!(fitted.rect(&frame, now, false), before);
    }

    /// A pinch zooms around the fingers and stops at the fit and the
    /// closest zoom.
    #[test]
    fn a_pinch_follows_the_fingers_between_fit_and_the_closest_zoom() {
        let frame = frame(4000., 2800.);
        let now = Instant::now();
        let mut zoom = Zoom::default();
        zoom.pinch(-0.5, point(0., 0.), &frame, now, false);
        assert_eq!(zoom.target, Scale::Fit);
        zoom.pinch(1.0, point(30., 0.), &frame, now, false);
        assert!(matches!(zoom.target, Scale::Of(scale) if close(scale, frame.fit() * 2.0)));
        assert!(!zoom.running(now, false), "a pinch never eases");
        for _ in 0..40 {
            zoom.pinch(1.0, point(30., 0.), &frame, now, false);
        }
        assert_eq!(zoom.target, Scale::Of(theme::IMAGE_ZOOM_MAX));
    }

    #[test]
    fn images_are_told_by_their_extension() {
        assert!(is_image(Path::new("shot.PNG")));
        assert!(is_image(Path::new("/tmp/a.jpeg")));
        assert!(!is_image(Path::new("notes.md")));
        assert!(!is_image(Path::new("Makefile")));
    }
}
