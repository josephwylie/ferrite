//! The screenshot an agent gets back from `show_visual`, so it can check its
//! own work: the page rendered off-screen at a width, as a PNG, with the
//! height it laid out to.
//!
//! [`RenderQueue`] is the `Send` end core's providers hold (they run on
//! reader threads); the engine is main-thread only, so each request is
//! carried to a foreground task, rendered there through the same engine seam
//! the transcript uses, and answered through its callback. Nothing blocks the
//! caller. [`snapshot`] is the render itself.
//!
//! A render opens a view of its own (never drawn; it isn't the transcript's
//! live one), waits for the laid-out page, lets it settle (scripts that draw
//! after load, fonts), and takes the last frame. Pages paint their own
//! background (`--ferrite-bg`); anything left transparent is composited over
//! it, so the picture is what the operator sees.

use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use ferrite_core::settings::ReadingSize;
use futures::channel::mpsc;
use futures::{FutureExt, StreamExt};
use gpui::{App, AsyncApp};

use super::engine::{Availability, Visuals};
use super::tokens::{self, Scheme};
use super::web::{Engine, Frame, Page, Viewport};

/// A rendered visual.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub png: Vec<u8>,
    /// Logical px (the PNG is `width × scale` wide).
    pub width: u32,
    /// The page's laid-out height, logical px.
    pub height: u32,
}

/// The width an agent's screenshot is taken at when no transcript has laid a
/// visual out yet: a comfortable Solo Pane's page.
const DEFAULT_WIDTH: u32 = 760;

/// The width the transcripts last laid a page out at (0: none yet).
static PAGE_WIDTH: AtomicU32 = AtomicU32::new(0);

/// Note the width a transcript lays its pages out at: the agent's
/// screenshot is taken at it, so it checks what the operator sees.
pub(crate) fn note_page_width(width: f32) {
    if width >= 1. {
        PAGE_WIDTH.store(width.round() as u32, Ordering::Relaxed);
    }
}

fn page_width() -> u32 {
    match PAGE_WIDTH.load(Ordering::Relaxed) {
        0 => DEFAULT_WIDTH,
        width => width,
    }
}

/// Longest a render may take, start to answer.
const TIMEOUT: Duration = Duration::from_secs(10);
/// A page that sends nothing new for this long has settled.
const SETTLE: Duration = Duration::from_millis(300);
/// After the first frame, take what there is after this long even if the
/// page keeps painting (an animation never settles).
const SETTLE_MAX: Duration = Duration::from_secs(3);
/// Longest to wait for Chromium to start on the first visual.
const START_TIMEOUT: Duration = Duration::from_secs(15);

/// Render `html` at `width` logical px and `scale` in `scheme`.
pub(crate) async fn snapshot(
    engine: Rc<dyn Engine>,
    html: String,
    width: u32,
    scale: f32,
    scheme: Scheme,
    cx: &AsyncApp,
) -> Result<Snapshot, String> {
    let theme_css = tokens::theme_css(scheme, ReadingSize::STANDARD);
    let bg = background(scheme);
    let (tx, mut rx) = mpsc::unbounded::<()>();
    let view = engine.open(
        Page { html, theme_css },
        Viewport { width: width.max(1), scale },
        Box::new(move || {
            let _ = tx.unbounded_send(());
        }),
    );
    let started = Instant::now();
    let mut first_frame: Option<Instant> = None;
    let mut frame: Option<Frame> = None;
    let mut height: Option<u32> = None;
    loop {
        let elapsed = started.elapsed();
        if elapsed >= TIMEOUT {
            break;
        }
        let quiet = if first_frame.is_some() { SETTLE } else { TIMEOUT - elapsed };
        let timer = cx.background_executor().timer(quiet.min(TIMEOUT - elapsed));
        futures::select_biased! {
            woke = rx.next() => {
                if woke.is_none() {
                    break;
                }
                let updates = view.take();
                if let Some(h) = updates.content_height {
                    height = Some(h);
                }
                if let Some(f) = updates.frame {
                    first_frame.get_or_insert_with(Instant::now);
                    frame = Some(f);
                }
            }
            _ = timer.fuse() => {
                // Quiet for SETTLE after the first frame: settled, if the
                // frame we hold is the page at its reported height.
                if first_frame.is_some() && frame_matches(frame.as_ref(), height, scale) {
                    break;
                }
            }
        }
        if first_frame.is_some_and(|t| t.elapsed() >= SETTLE_MAX) {
            break;
        }
    }
    drop(view);
    let frame = frame.ok_or_else(|| format!("the page didn't draw within {} s", TIMEOUT.as_secs()))?;
    let height = height.unwrap_or_else(|| (frame.height as f32 / scale).round() as u32);
    Ok(Snapshot { png: encode_png(&frame, bg)?, width, height })
}

/// The frame is the page at its reported height (a frame can lag a resize).
fn frame_matches(frame: Option<&Frame>, height: Option<u32>, scale: f32) -> bool {
    match (frame, height) {
        (Some(frame), Some(height)) => (frame.height as f32 - height as f32 * scale).abs() <= 1.0,
        (Some(_), None) => true,
        _ => false,
    }
}

/// The scheme's page background, as RGB.
fn background(scheme: Scheme) -> [u8; 3] {
    let hex = tokens::value("bg", scheme, ReadingSize::STANDARD).unwrap_or_default();
    let rgb = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0x1b1b1c);
    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
}

/// Premultiplied BGRA over an opaque `bg` → an opaque RGB PNG.
pub(crate) fn encode_png(frame: &Frame, bg: [u8; 3]) -> Result<Vec<u8>, String> {
    let mut rgb = Vec::with_capacity((frame.width * frame.height * 3) as usize);
    for px in frame.bgra.chunks_exact(4) {
        let a = px[3] as u32;
        for (src, under) in [(px[2], bg[0]), (px[1], bg[1]), (px[0], bg[2])] {
            // Premultiplied: out = src + under × (1 − a).
            rgb.push((src as u32 + (under as u32 * (255 - a) + 127) / 255).min(255) as u8);
        }
    }
    let image = image::RgbImage::from_raw(frame.width, frame.height, rgb).ok_or("a frame is width × height × 4 bytes")?;
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| format!("png: {e}"))?;
    Ok(png)
}

/// One render asked for from any thread.
pub(crate) struct Request {
    pub html: String,
    /// Logical px.
    pub width: u32,
    pub scheme: Scheme,
    pub reply: Box<dyn FnOnce(Result<Snapshot, String>) + Send>,
}

/// The `Send` end of the render queue (see the module docs). Clone freely.
#[derive(Clone)]
pub(crate) struct RenderQueue {
    requests: mpsc::UnboundedSender<Request>,
}

impl RenderQueue {
    /// Ask for a render; `reply` is called (on the main thread) with the
    /// result. If the app is gone the reply is never called... except that
    /// dropping it answers an error first.
    pub(crate) fn render(&self, request: Request) {
        if let Err(error) = self.requests.unbounded_send(request) {
            let request = error.into_inner();
            (request.reply)(Err("Ferrite is shutting down".into()));
        }
    }
}

/// Core's seam (`ferrite_core::visual::Renderer`): called on a provider's
/// reader thread, it only queues; the reply goes back from the main thread.
impl ferrite_core::visual::Renderer for RenderQueue {
    fn render(&self, request: ferrite_core::visual::RenderRequest, reply: ferrite_core::visual::Reply) {
        self.render(Request {
            html: request.html,
            width: page_width(),
            scheme: Scheme::APP,
            reply: Box::new(move |result| {
                reply.send(result.map(|shot| ferrite_core::visual::Rendered {
                    png: shot.png,
                    width: shot.width,
                    height: shot.height,
                }))
            }),
        });
    }
}

/// Start the queue's foreground task. One per app.
pub(crate) fn start(cx: &mut App) -> RenderQueue {
    let (requests, mut incoming) = mpsc::unbounded::<Request>();
    cx.spawn(async move |cx| {
        while let Some(request) = incoming.next().await {
            // Each render waits on its own page; they run side by side.
            cx.spawn(async move |cx| {
                let result = match engine(cx).await {
                    Ok(engine) => snapshot(engine, request.html, request.width, 1.0, request.scheme, cx).await,
                    Err(why) => Err(why),
                };
                (request.reply)(result);
            })
            .detach();
        }
    })
    .detach();
    RenderQueue { requests }
}

/// The engine, waiting for Chromium to start if this is the first visual.
async fn engine(cx: &AsyncApp) -> Result<Rc<dyn Engine>, String> {
    let started = Instant::now();
    loop {
        match cx.update(Visuals::engine) {
            Availability::Ready(engine) => return Ok(engine),
            Availability::Unavailable(why) => return Err(why.to_string()),
            Availability::Starting if started.elapsed() < START_TIMEOUT => {
                cx.background_executor().timer(Duration::from_millis(50)).await;
            }
            Availability::Starting => return Err("Chromium didn't start in time".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visual::web::fake::{FakeEngine, CONTENT_HEIGHT};
    use gpui::TestAppContext;

    #[test]
    fn a_frame_becomes_an_opaque_png_composited_over_the_background() {
        // One opaque red pixel, one transparent one.
        let frame = Frame { width: 2, height: 1, bgra: vec![0, 0, 255, 255, 0, 0, 0, 0] };
        let png = encode_png(&frame, [0x1b, 0x1b, 0x1c]).unwrap();
        let decoded = image::load_from_memory(&png).unwrap().to_rgb8();
        assert_eq!(decoded.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(decoded.get_pixel(1, 0).0, [0x1b, 0x1b, 0x1c]);
    }

    #[gpui::test]
    async fn a_snapshot_is_the_laid_out_page(cx: &mut TestAppContext) {
        let engine: Rc<dyn Engine> = Rc::new(FakeEngine::new());
        let task = cx.spawn(async move |cx| snapshot(engine, "<p>hi</p>".into(), 300, 1.0, Scheme::Dark, &cx).await);
        cx.run_until_parked();
        cx.executor().advance_clock(SETTLE * 2);
        let shot = task.await.unwrap();
        assert_eq!((shot.width, shot.height), (300, CONTENT_HEIGHT));
        let decoded = image::load_from_memory(&shot.png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (300, CONTENT_HEIGHT));
    }

    #[gpui::test]
    async fn the_queue_answers_from_the_installed_engine(cx: &mut TestAppContext) {
        let queue = cx.update(|cx| {
            Visuals::install(Rc::new(FakeEngine::new()), cx);
            start(cx)
        });
        let (tx, rx) = futures::channel::oneshot::channel();
        queue.render(Request {
            html: "<p>hi</p>".into(),
            width: 200,
            scheme: Scheme::Dark,
            reply: Box::new(move |result| {
                let _ = tx.send(result);
            }),
        });
        cx.run_until_parked();
        cx.executor().advance_clock(SETTLE * 2);
        cx.run_until_parked();
        let shot = rx.await.unwrap().unwrap();
        assert_eq!(shot.width, 200);
    }
}
