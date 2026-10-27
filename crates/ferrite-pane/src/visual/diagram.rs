//! Diagrams: a ` ```mermaid ` fence drawn as the diagram it describes, in an
//! answer and in a preview (ADR 0016).
//!
//! The same off-screen engine that draws agents' visuals (ADR 0013) renders
//! the fence once, with the vendored `mermaid.js` inlined in the page and
//! Ferrite's palette as its theme, and the frame is kept as an image: a
//! diagram is a picture, so no page stays live. Until it is drawn, and
//! whenever it can't be (no engine in this build, visuals turned off, a
//! fence mermaid refuses, a fence still streaming), the fence shows as code.
//! A draw that fails for now (Chromium slow to start, a page that ran out of
//! time) is asked for again the next time the fence is painted, up to
//! [`MISSES`] times.
//!
//! - **Lazy.** A fence asks for its diagram only once its code is painted
//!   on screen; a long document draws what the operator scrolls to. Pages
//!   are drawn one at a time, so a document full of fences doesn't open a
//!   Chromium page (each parsing the whole bundle) per fence at once.
//! - **Cached by content.** Drawn images are kept by a hash of the fence,
//!   the reading size and the display scale, so a fence seen again (a
//!   re-render, another Pane, the reader) costs nothing; the least recently
//!   shown go past [`BUDGET`], never one shown since the last draw landed
//!   (it may be on screen, and letting it go would draw it again, and
//!   evict another, without end).
//! - **Sized like a fence.** The page is laid out at the fence's own measure
//!   ([`theme::MEASURE_CH`] cells) and cropped to what the diagram drew; a
//!   narrower column scales it down, never up.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use ferrite_core::settings::ReadingSize;
use futures::channel::mpsc;
use futures::StreamExt;
use gpui::prelude::*;
use gpui::{
    canvas, div, img, px, AnyElement, App, AsyncApp, EntityId, Global, RenderImage, Window,
};
use serde_json::{json, Value};

use super::engine::{Availability, Visuals};
use super::renderer::{self, Settled};
use super::tokens::{self, Scheme};
use super::web::{Engine, Frame, Page, Viewport};
use crate::theme;

/// The fence language drawn as a diagram.
pub(crate) const LANGUAGE: &str = "mermaid";

/// mermaid.js, vendored (`assets/mermaid/README.md`).
#[cfg(any(feature = "cef", test))]
const MERMAID_JS: &str = include_str!("../../assets/mermaid/mermaid.min.js");
/// A build without Chromium never draws a page, so it doesn't carry the
/// bundle (~5.5 MB).
#[cfg(not(any(feature = "cef", test)))]
const MERMAID_JS: &str = "";

/// What the drawn images may hold, in bytes, before the least recently
/// shown are let go (a diagram at the Standard measure on a 2x display is
/// a few MB).
const BUDGET: usize = 64 * 1024 * 1024;

/// The page's height until mermaid has drawn: the empty page lays out 0 px
/// tall, which the engine reports as 1 (it never reports less)...
const EMPTY: u32 = 1;
/// ...and once mermaid refused the fence: the page sets itself this tall,
/// which neither the empty page nor a diagram is.
const REFUSED: u32 = 2;

/// How many times a fence is drawn and fails for now before it stays code.
const MISSES: u8 = 3;

/// The page's script: draw the fence, or say it can't be drawn (see
/// [`REFUSED`]). `CONFIG` and `SOURCE` are JSON. Mermaid measures labels
/// as it lays nodes out, so the theme's face is loaded first (`@font-face`
/// loads lazily, and the page has no text before mermaid's).
const DRAW_SCRIPT: &str = r#"(async () => {
  const out = document.getElementById("diagram");
  const config = CONFIG;
  await document.fonts
    .load(`${config.themeVariables.fontSize} ${config.fontFamily}`)
    .catch(() => {});
  try {
    mermaid.initialize(config);
    const { svg } = await mermaid.render("ferrite-diagram", SOURCE);
    out.innerHTML = svg;
  } catch (_) {
    document.body.replaceChildren();
    document.documentElement.style.height = "2px";
  }
})();"#;

/// The process's diagrams (a GPUI global; [`init`] installs it).
pub(crate) struct Diagrams {
    /// Visuals are on (Settings): off, every fence stays code.
    enabled: bool,
    entries: HashMap<u64, Entry>,
    /// Bumped whenever a diagram is shown or lands, so the least recently
    /// shown can be found.
    clock: u64,
    /// The clock when the last draw landed: an image shown since is in use.
    landed: u64,
    /// Draws waiting their turn ([`init`]'s task draws one at a time).
    queue: mpsc::UnboundedSender<Job>,
}

impl Global for Diagrams {}

enum Entry {
    /// Being drawn (or waiting its turn); `views` painted the fence and
    /// redraw when it lands. `misses` is how often it failed for now.
    Drawing {
        views: Vec<EntityId>,
        misses: u8,
    },
    Drawn(Drawn),
    /// Failed for now this many times: asked for again when next painted.
    Missed(u8),
    /// Mermaid refused it, there is no engine, or it missed [`MISSES`]
    /// times: it stays code.
    Failed,
}

/// A fence to draw.
struct Job {
    key: u64,
    code: String,
    reading: ReadingSize,
    scale: f32,
}

/// Why a fence wasn't drawn.
#[derive(Debug, PartialEq)]
enum Failure {
    /// For good: mermaid refused the fence, or there is no engine.
    Final,
    /// For now: Chromium was slow to start, or the page ran out of time.
    Passing,
}

#[derive(Clone)]
struct Drawn {
    image: Arc<RenderImage>,
    /// Logical px.
    width: f32,
    height: f32,
    /// The clock when it was last shown (or landed).
    shown_at: u64,
}

impl Drawn {
    fn bytes(&self) -> usize {
        self.image.size(0).width.0 as usize * self.image.size(0).height.0 as usize * 4
    }
}

/// Install the global and start the task that draws: `enabled` is whether
/// visuals are on (Settings).
pub fn init(enabled: bool, cx: &mut App) {
    let (queue, mut jobs) = mpsc::unbounded::<Job>();
    cx.spawn(async move |cx| {
        while let Some(Job {
            key,
            code,
            reading,
            scale,
        }) = jobs.next().await
        {
            let drawn = match renderer::engine(cx).await {
                Ok(engine) => draw(engine, page(&code, reading), width(reading), scale, cx).await,
                // No engine in this build is for good; one still starting isn't.
                Err(_) => match cx.update(Visuals::engine) {
                    Availability::Unavailable(_) => Err(Failure::Final),
                    _ => Err(Failure::Passing),
                },
            };
            cx.update(|cx| land(key, drawn, cx));
        }
    })
    .detach();
    cx.set_global(Diagrams {
        enabled,
        entries: HashMap::new(),
        clock: 0,
        landed: 0,
        queue,
    });
}

impl Diagrams {
    /// The drawn diagram for `key`, noted as shown now.
    fn show(&mut self, key: u64) -> Option<Drawn> {
        self.clock += 1;
        match self.entries.get_mut(&key) {
            Some(Entry::Drawn(drawn)) => {
                drawn.shown_at = self.clock;
                Some(drawn.clone())
            }
            _ => None,
        }
    }
}

/// Turn diagrams on or off (Settings changed); fences drawn again follow.
pub fn set_enabled(enabled: bool, cx: &mut App) {
    if cx
        .try_global::<Diagrams>()
        .is_some_and(|diagrams| diagrams.enabled != enabled)
    {
        cx.global_mut::<Diagrams>().enabled = enabled;
        cx.refresh_windows();
    }
}

/// A ` ```mermaid ` fence as it draws: its diagram once drawn, else `code`
/// (the fence's own element). `fence` is the fence's whole Markdown, from
/// its opening line to its closing one if that has arrived; `code` is
/// what it holds. `size` is the prose size it sits in, px.
pub(crate) fn or_code(
    fence: &str,
    code: String,
    element: AnyElement,
    size: f32,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let enabled = cx
        .try_global::<Diagrams>()
        .is_some_and(|diagrams| diagrams.enabled);
    if !enabled || !closed(fence) || code.trim().is_empty() {
        return element;
    }
    let reading = ReadingSize::nearest(size.round() as u8);
    let scale = window.scale_factor();
    let key = key(&code, reading, scale);
    let diagrams = cx.global_mut::<Diagrams>();
    if let Some(drawn) = diagrams.show(key) {
        return diagram(drawn, size);
    }
    match diagrams.entries.get(&key) {
        Some(Entry::Failed) => element,
        // Asked for once its code is painted on screen (and waited on by
        // every view that paints it meanwhile).
        _ => div()
            .relative()
            .w_full()
            .min_w_0()
            .child(element)
            .child(
                canvas(
                    move |bounds, window, cx| {
                        if window.content_mask().bounds.intersects(&bounds) {
                            request(key, code, reading, scale, window.current_view(), cx);
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .into_any_element(),
    }
}

/// The drawn diagram on the fence's rule and indent, at its own size or
/// scaled down to the column.
fn diagram(drawn: Drawn, size: f32) -> AnyElement {
    div()
        .debug_selector(|| "mermaid-diagram".into())
        .w_full()
        .min_w_0()
        .pl(px(2.0 * theme::tx_cell(size)))
        .border_l_1()
        .border_color(theme::paint::LINE2)
        .child(
            img(drawn.image)
                .w_full()
                .max_w(px(drawn.width))
                .aspect_ratio(drawn.width / drawn.height),
        )
        .into_any_element()
}

/// Whether a fence's Markdown has its closing line: a fence still
/// streaming in isn't drawn yet. A fence in a quote carries the quote's
/// `>` on every line but the first.
pub(crate) fn closed(fence: &str) -> bool {
    let mut lines = fence.trim_end().lines().map(|line| {
        line.trim_start_matches(|c: char| c == '>' || c.is_whitespace())
            .trim_end()
    });
    let Some(open) = lines.next() else {
        return false;
    };
    let marker = if open.starts_with("~~~") { '~' } else { '`' };
    lines
        .next_back()
        .is_some_and(|line| line.len() >= 3 && line.chars().all(|c| c == marker))
}

/// The cache key: what the image depends on.
fn key(code: &str, reading: ReadingSize, scale: f32) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    code.hash(&mut hash);
    reading.hash(&mut hash);
    scale.to_bits().hash(&mut hash);
    hash.finish()
}

/// Draw `code` for the view `view` (it painted the fence), unless it is
/// already drawn or being drawn.
fn request(key: u64, code: String, reading: ReadingSize, scale: f32, view: EntityId, cx: &mut App) {
    let diagrams = cx.global_mut::<Diagrams>();
    let misses = match diagrams.entries.get_mut(&key) {
        Some(Entry::Drawing { views, .. }) => {
            if !views.contains(&view) {
                views.push(view);
            }
            return;
        }
        Some(Entry::Drawn(_) | Entry::Failed) => return,
        Some(Entry::Missed(misses)) => *misses,
        None => 0,
    };
    diagrams.entries.insert(
        key,
        Entry::Drawing {
            views: vec![view],
            misses,
        },
    );
    let _ = diagrams.queue.unbounded_send(Job {
        key,
        code,
        reading,
        scale,
    });
}

/// A diagram finished drawing (or didn't): keep it, redraw the views that
/// wait on it, and let the least recently shown go past the budget.
fn land(key: u64, drawn: Result<Drawn, Failure>, cx: &mut App) {
    let diagrams = cx.global_mut::<Diagrams>();
    let (views, misses) = match diagrams.entries.remove(&key) {
        Some(Entry::Drawing { views, misses }) => (views, misses),
        _ => (Vec::new(), 0),
    };
    diagrams.clock += 1;
    let entry = match drawn {
        Ok(drawn) => Entry::Drawn(Drawn {
            shown_at: diagrams.clock,
            ..drawn
        }),
        // It stays code, so nothing to redraw: asked for again the next
        // time it is painted...
        Err(Failure::Passing) if misses + 1 < MISSES => {
            diagrams.entries.insert(key, Entry::Missed(misses + 1));
            return;
        }
        // ...unless that was its last chance.
        Err(_) => Entry::Failed,
    };
    diagrams.entries.insert(key, entry);
    let evicted = evict(&mut diagrams.entries, diagrams.landed);
    diagrams.landed = diagrams.clock;
    for view in views {
        cx.notify(view);
    }
    if evicted.is_empty() {
        return;
    }
    // Free their place in each window's sprite atlas too.
    for window in cx.windows() {
        let _ = window.update(cx, |_, window, _| {
            for image in &evicted {
                let _ = window.drop_image(image.clone());
            }
        });
    }
}

/// Let the least recently shown images go until the rest fit [`BUDGET`],
/// never one shown (or landed) since `in_use` (the clock when the last draw
/// landed): it may be on screen. Returns them.
fn evict(entries: &mut HashMap<u64, Entry>, in_use: u64) -> Vec<Arc<RenderImage>> {
    let mut drawn: Vec<(u64, u64, usize)> = entries
        .iter()
        .filter_map(|(key, entry)| match entry {
            Entry::Drawn(drawn) => Some((drawn.shown_at, *key, drawn.bytes())),
            _ => None,
        })
        .collect();
    let mut held: usize = drawn.iter().map(|(_, _, bytes)| bytes).sum();
    drawn.sort_unstable();
    let mut evicted = Vec::new();
    for (shown_at, key, bytes) in drawn {
        if held <= BUDGET || shown_at >= in_use {
            break;
        }
        if let Some(Entry::Drawn(drawn)) = entries.remove(&key) {
            evicted.push(drawn.image);
            held -= bytes;
        }
    }
    evicted
}

/// The page is laid out at the fence's measure at `reading`, logical px.
fn width(reading: ReadingSize) -> u32 {
    (theme::MEASURE_CH * theme::tx_cell(theme::answer_text_size(reading))).round() as u32
}

/// The page that draws `code`: mermaid, themed, then the script that runs it.
fn page(code: &str, reading: ReadingSize) -> Page {
    let config = json!({
        "startOnLoad": false,
        // No click handlers, no HTML in labels beyond mermaid's sanitised set.
        "securityLevel": "strict",
        // A refusal draws nothing (the fence stays code), not mermaid's bomb.
        "suppressErrorRendering": true,
        "theme": "base",
        "fontFamily": tokens::value("font-mono", Scheme::APP, reading),
        "themeVariables": theme_variables(Scheme::APP, reading),
    });
    let script = DRAW_SCRIPT
        .replace("CONFIG", &script_json(&config))
        .replace("SOURCE", &script_json(&Value::from(code)));
    Page {
        html: format!(
            "<style>html,body{{margin:0;background:var(--ferrite-bg)}}#diagram svg{{display:block}}</style>\
             <div id=\"diagram\"></div><script>{MERMAID_JS}</script><script>{script}</script>"
        ),
        theme_css: tokens::theme_css(Scheme::APP, reading),
    }
}

/// `value` as JSON that can sit inside a `<script>` (no `</script>` in it).
fn script_json(value: &Value) -> String {
    value.to_string().replace('<', "\\u003c")
}

/// Mermaid's `base` theme in Ferrite's palette: flat nodes on the raised
/// band with the strong edge, edges and labels in the reading inks, the
/// chart series for pies.
fn theme_variables(scheme: Scheme, reading: ReadingSize) -> Value {
    let token = |name: &str| tokens::value(name, scheme, reading).unwrap_or_default();
    json!({
        "darkMode": scheme == Scheme::Dark,
        // Flat, as everything Ferrite draws (theme rule 2).
        "dropShadow": "none",
        "background": token("bg"),
        "fontFamily": token("font-mono"),
        "fontSize": token("font-size"),
        "textColor": token("fg"),
        "primaryColor": token("band2"),
        "primaryTextColor": token("strong"),
        "primaryBorderColor": token("border2"),
        "secondaryColor": token("band"),
        "secondaryTextColor": token("fg"),
        "secondaryBorderColor": token("border2"),
        "tertiaryColor": token("bg"),
        "tertiaryTextColor": token("fg"),
        "tertiaryBorderColor": token("border"),
        "lineColor": token("muted"),
        "edgeLabelBackground": token("bg"),
        "clusterBkg": token("band"),
        "clusterBorder": token("border"),
        "titleColor": token("strong"),
        "noteBkgColor": token("float"),
        "noteTextColor": token("fg"),
        "noteBorderColor": token("border2"),
        "pie1": token("chart-1"),
        "pie2": token("chart-2"),
        "pie3": token("chart-3"),
        "pie4": token("chart-4"),
        "pie5": token("chart-5"),
        "pie6": token("chart-6"),
    })
}

/// Load `page` off-screen and take the drawn diagram, cropped to what it
/// drew.
async fn draw(
    engine: Rc<dyn Engine>,
    page: Page,
    width: u32,
    scale: f32,
    cx: &AsyncApp,
) -> Result<Drawn, Failure> {
    let drawing_done = |height: Option<u32>| height.is_some_and(|height| height > EMPTY);
    let Settled { frame, height } =
        renderer::settle(engine, page, Viewport { width, scale }, drawing_done, cx)
            .await
            .map_err(|_| Failure::Passing)?;
    if height == Some(REFUSED) {
        return Err(Failure::Final);
    }
    if !drawing_done(height) {
        // Out of time before mermaid drew.
        return Err(Failure::Passing);
    }
    let frame = crop(frame, background()).ok_or(Failure::Passing)?;
    let (width, height) = (frame.width as f32 / scale, frame.height as f32 / scale);
    let image = image::RgbaImage::from_raw(frame.width, frame.height, frame.bgra)
        .ok_or(Failure::Passing)?;
    Ok(Drawn {
        image: Arc::new(RenderImage::new(vec![image::Frame::new(image)])),
        width,
        height,
        shown_at: 0,
    })
}

/// The page's background as BGRA bytes' first three (what an undrawn pixel is).
fn background() -> [u8; 3] {
    let hex = tokens::value("bg", Scheme::APP, ReadingSize::STANDARD).unwrap_or_default();
    let rgb = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(theme::CARD);
    [rgb as u8, (rgb >> 8) as u8, (rgb >> 16) as u8]
}

/// `frame` without the background to the right of what it drew; `None`
/// when it drew nothing.
fn crop(frame: Frame, bg: [u8; 3]) -> Option<Frame> {
    let stride = frame.width as usize * 4;
    let drawn = |x: usize| {
        frame
            .bgra
            .chunks_exact(stride)
            .any(|row| row[x * 4..x * 4 + 3] != bg)
    };
    let width = (0..frame.width as usize).rev().find(|x| drawn(*x))? + 1;
    let bgra = frame
        .bgra
        .chunks_exact(stride)
        .flat_map(|row| &row[..width * 4])
        .copied()
        .collect();
    Some(Frame {
        width: width as u32,
        height: frame.height,
        bgra,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::time::Duration;

    use gpui::TestAppContext;

    use super::*;
    use crate::visual::web::fake::{Call, FakeEngine};
    use crate::visual::web::{Input, Updates, View, Wake};

    /// An engine whose one page reports the heights the test gives it, when
    /// the test gives them (the fake engine always reports a drawn page).
    #[derive(Default)]
    struct Scripted {
        wake: RefCell<Option<Wake>>,
        pending: Rc<RefCell<Updates>>,
    }

    /// The width the scripted page is opened at, logical px.
    const PAGE: u32 = 8;

    impl Scripted {
        /// The page lays out `height` px tall, drawing something or nothing.
        fn report(&self, height: u32, drawn: bool) {
            let [b, g, r] = background();
            let mut bgra = [b, g, r, 255].repeat((PAGE * height) as usize);
            if drawn {
                bgra[..4].copy_from_slice(&[0, 0, 0, 255]);
            }
            *self.pending.borrow_mut() = Updates {
                frame: Some(Frame {
                    width: PAGE,
                    height,
                    bgra,
                }),
                content_height: Some(height),
                cursor: None,
            };
            (self.wake.borrow().as_ref().expect("the page is open"))();
        }
    }

    impl Engine for Scripted {
        fn open(&self, _: Page, _: Viewport, wake: Wake) -> Box<dyn View> {
            *self.wake.borrow_mut() = Some(wake);
            Box::new(ScriptedView(self.pending.clone()))
        }
        fn pump(&self) -> Duration {
            Duration::from_secs(1)
        }
    }

    struct ScriptedView(Rc<RefCell<Updates>>);

    impl View for ScriptedView {
        fn resize(&self, _: Viewport) {}
        fn input(&self, _: Input) {}
        fn set_theme(&self, _: &str) {}
        fn set_html(&self, _: &str) {}
        fn set_visible(&self, _: bool) {}
        fn take(&self) -> Updates {
            std::mem::take(&mut self.0.borrow_mut())
        }
    }

    /// Draw a fence on a scripted engine; `script` drives the page.
    fn draw_scripted(
        cx: &mut TestAppContext,
        script: impl FnOnce(&Scripted, &mut TestAppContext),
    ) -> Result<Drawn, Failure> {
        let engine = Rc::new(Scripted::default());
        let page = page("graph TD", ReadingSize::STANDARD);
        let task = cx.spawn({
            let engine: Rc<dyn Engine> = engine.clone();
            async move |cx| draw(engine, page, PAGE, 1.0, &cx).await
        });
        cx.run_until_parked();
        script(&engine, cx);
        cx.executor().advance_clock(Duration::from_secs(11));
        cx.run_until_parked();
        futures::executor::block_on(task)
    }

    #[gpui::test]
    fn an_empty_page_is_still_drawing(cx: &mut TestAppContext) {
        let drawn = draw_scripted(cx, |page, cx| {
            // The page is measured before mermaid draws, for longer than any
            // settling.
            page.report(EMPTY, false);
            cx.executor().advance_clock(Duration::from_secs(5));
            cx.run_until_parked();
            page.report(40, true);
        });
        let drawn = drawn.expect("drawn once mermaid has drawn");
        assert_eq!(
            (drawn.width, drawn.height),
            (1., 40.),
            "cropped to what it drew"
        );
    }

    #[gpui::test]
    fn a_refused_fence_stays_code_for_good(cx: &mut TestAppContext) {
        let drawn = draw_scripted(cx, |page, _| {
            page.report(EMPTY, false);
            page.report(REFUSED, false);
        });
        assert_eq!(drawn.err(), Some(Failure::Final));
    }

    #[gpui::test]
    fn a_page_that_never_draws_fails_for_now(cx: &mut TestAppContext) {
        let drawn = draw_scripted(cx, |page, _| page.report(EMPTY, false));
        assert_eq!(drawn.err(), Some(Failure::Passing));
    }

    /// Asked for again when next painted, until it misses [`MISSES`] times.
    #[gpui::test]
    fn a_failure_for_now_is_retried_a_few_times(cx: &mut TestAppContext) {
        cx.update(|cx| {
            init(true, cx);
            let land_failing = |failure, cx: &mut App| {
                let entries = &mut cx.global_mut::<Diagrams>().entries;
                let misses = match entries.get(&1) {
                    Some(Entry::Missed(misses)) => *misses,
                    _ => 0,
                };
                entries.insert(
                    1,
                    Entry::Drawing {
                        views: Vec::new(),
                        misses,
                    },
                );
                land(1, Err(failure), cx);
                matches!(cx.global::<Diagrams>().entries.get(&1), Some(Entry::Failed))
            };
            assert!(land_failing(Failure::Final, cx), "a refusal is for good");
            cx.global_mut::<Diagrams>().entries.clear();
            for miss in 1..MISSES {
                assert!(
                    !land_failing(Failure::Passing, cx),
                    "miss {miss}: asked for again"
                );
            }
            assert!(land_failing(Failure::Passing, cx), "then it stays code");
        });
    }

    /// Fences are drawn one page at a time.
    #[gpui::test]
    fn one_page_draws_at_a_time(cx: &mut TestAppContext) {
        let engine = Rc::new(FakeEngine::new());
        cx.update(|cx| {
            Visuals::install(engine.clone(), cx);
            init(true, cx);
            let view = cx.new(|_| ()).entity_id();
            for code in ["graph TD", "graph LR"] {
                request(
                    key(code, ReadingSize::STANDARD, 1.),
                    code.into(),
                    ReadingSize::STANDARD,
                    1.,
                    view,
                    cx,
                );
            }
        });
        let opened = || {
            engine
                .journal()
                .iter()
                .filter(|call| matches!(call, Call::Open(_)))
                .count()
        };
        cx.run_until_parked();
        assert_eq!((opened(), engine.live_views()), (1, 1));
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert_eq!(opened(), 2, "the second once the first landed");
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert_eq!(engine.live_views(), 0);
    }

    #[test]
    fn the_page_refuses_at_the_refused_height() {
        assert!(DRAW_SCRIPT.contains(&format!("style.height = \"{REFUSED}px\"")));
    }

    #[test]
    fn only_a_closed_fence_is_drawn() {
        assert!(closed("```mermaid\ngraph TD\n  a --> b\n```"));
        assert!(closed("```mermaid\n> graph TD\n> ```"), "in a quote");
        assert!(
            !closed("```mermaid\n> graph TD\n>"),
            "in a quote, still streaming"
        );
        assert!(closed("```mermaid\ngraph TD\n```\n"));
        assert!(closed("~~~mermaid\ngraph TD\n~~~"));
        assert!(
            !closed("```mermaid\ngraph TD\n  a --> b"),
            "still streaming"
        );
        assert!(!closed("```mermaid"));
        assert!(!closed("```mermaid\ngraph TD\n~~~"), "the other marker");
        assert!(!closed(""));
    }

    #[test]
    fn the_key_follows_the_code_the_reading_size_and_the_scale() {
        let base = key("graph TD", ReadingSize::STANDARD, 2.0);
        assert_eq!(base, key("graph TD", ReadingSize::STANDARD, 2.0));
        assert_ne!(base, key("graph LR", ReadingSize::STANDARD, 2.0));
        assert_ne!(base, key("graph TD", ReadingSize::LARGEST, 2.0));
        assert_ne!(base, key("graph TD", ReadingSize::STANDARD, 1.0));
    }

    #[test]
    fn the_page_inlines_mermaid_and_the_fence_and_fetches_nothing() {
        let code = "graph TD\n  a[\"</script><b>\"] --> b";
        let page = page(code, ReadingSize::STANDARD);
        assert!(
            page.html.contains("globalThis[\"mermaid\"]"),
            "the bundle is inlined"
        );
        assert_eq!(
            page.html.matches("</script>").count(),
            2,
            "the fence can't close a script"
        );
        let escaped = Value::from(code)
            .to_string()
            .replace('<', &format!("{}u003c", '\\'));
        assert!(
            page.html.contains(&escaped),
            "the fence, as a JSON string with no `<`"
        );
        assert!(
            !page.html.replace(MERMAID_JS, "").contains("src="),
            "nothing is fetched"
        );
        assert_eq!(
            page.theme_css,
            tokens::theme_css(Scheme::APP, ReadingSize::STANDARD)
        );
    }

    #[test]
    fn the_bundle_can_sit_inside_a_script_element() {
        let lower = MERMAID_JS.to_ascii_lowercase();
        assert!(!lower.contains("</script"));
        assert!(!lower.contains("<script"));
    }

    #[test]
    fn the_theme_is_ferrites_palette() {
        let reading = ReadingSize::STANDARD;
        let vars = theme_variables(Scheme::Dark, reading);
        let token = |name| Value::from(tokens::value(name, Scheme::Dark, reading).unwrap());
        assert_eq!(vars["darkMode"], true);
        assert_eq!(vars["background"], token("bg"));
        assert_eq!(vars["primaryColor"], token("band2"));
        assert_eq!(vars["lineColor"], token("muted"));
        assert_eq!(vars["fontSize"], "13px");
        assert_eq!(vars["pie1"], token("chart-1"));
    }

    #[test]
    fn a_frame_is_cropped_to_what_it_drew() {
        let bg = [1, 2, 3];
        let px = |drawn: bool| {
            if drawn {
                [9, 9, 9, 255]
            } else {
                [1, 2, 3, 255]
            }
        };
        // Two rows of four: something at x = 0 and, lower, at x = 1.
        let bgra = [
            px(true),
            px(false),
            px(false),
            px(false),
            px(false),
            px(true),
            px(false),
            px(false),
        ]
        .concat();
        let cropped = crop(
            Frame {
                width: 4,
                height: 2,
                bgra,
            },
            bg,
        )
        .unwrap();
        assert_eq!((cropped.width, cropped.height), (2, 2));
        assert_eq!(
            cropped.bgra,
            [px(true), px(false), px(false), px(true)].concat()
        );
        let blank = Frame {
            width: 2,
            height: 1,
            bgra: [px(false), px(false)].concat(),
        };
        assert!(crop(blank, bg).is_none());
    }

    /// A drawn diagram holding `share` of the budget.
    fn drawn(share: f64) -> Drawn {
        // 4 bytes a pixel, one pixel wide.
        let height = (BUDGET as f64 * share / 4.) as u32 + 1;
        let image = image::RgbaImage::new(1, height);
        Drawn {
            image: Arc::new(RenderImage::new(vec![image::Frame::new(image)])),
            width: 1.,
            height: 1.,
            shown_at: 0,
        }
    }

    #[test]
    fn the_least_recently_shown_go_past_the_budget() {
        let mut entries = HashMap::new();
        for (key, shown_at) in [(1, 3), (2, 1), (3, 2)] {
            entries.insert(
                key,
                Entry::Drawn(Drawn {
                    shown_at,
                    ..drawn(1. / 3.)
                }),
            );
        }
        entries.insert(4, Entry::Failed);
        let evicted = evict(&mut entries, 3);
        assert_eq!(evicted.len(), 1);
        assert!(!entries.contains_key(&2), "the least recently shown");
        assert!(entries.contains_key(&1) && entries.contains_key(&3) && entries.contains_key(&4));
    }

    /// Two diagrams on screen together, past the budget between them: each
    /// landing would otherwise let the other go, which redraws it, which
    /// lets this one go, without end.
    #[gpui::test]
    fn a_diagram_shown_since_the_last_landing_is_kept(cx: &mut TestAppContext) {
        cx.update(|cx| {
            init(true, cx);
            let held = |cx: &App| {
                let entries = &cx.global::<Diagrams>().entries;
                [1, 2, 3, 4].map(|key| matches!(entries.get(&key), Some(Entry::Drawn(_))))
            };
            land(1, Ok(drawn(0.6)), cx);
            land(2, Ok(drawn(0.6)), cx);
            assert_eq!(
                held(cx),
                [true, true, false, false],
                "1 has only just landed"
            );
            // The view redraws with 1 shown (and 3 asked for).
            assert!(cx.global_mut::<Diagrams>().show(1).is_some());
            land(3, Ok(drawn(0.6)), cx);
            assert_eq!(
                held(cx),
                [true, true, true, false],
                "1 is on screen, 2 only just landed"
            );
            // Nothing shown since: all but the last two to land go.
            land(4, Ok(drawn(0.6)), cx);
            assert_eq!(held(cx), [false, false, true, true]);
        });
    }
}
