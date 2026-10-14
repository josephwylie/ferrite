//! Diagrams: a ` ```mermaid ` fence drawn as the diagram it describes, in an
//! answer and in the reader (ADR 0014).
//!
//! The same off-screen engine that draws agents' visuals (ADR 0013) renders
//! the fence once, with the vendored `mermaid.js` inlined in the page and
//! Ferrite's palette as its theme, and the frame is kept as an image: a
//! diagram is a picture, so no page stays live. Until it is drawn, and
//! whenever it can't be (no engine in this build, visuals turned off, a
//! fence mermaid refuses, a fence still streaming), the fence shows as code.
//!
//! - **Lazy.** A fence asks for its diagram only once its code is painted
//!   on screen; a long document draws what the operator scrolls to.
//! - **Cached by content.** Drawn images are kept by a hash of the fence,
//!   the reading size and the display scale, so a fence seen again (a
//!   re-render, another Pane, the reader) costs nothing; the least recently
//!   drawn go past [`BUDGET`].
//! - **Sized like a fence.** The page is laid out at the fence's own measure
//!   ([`theme::MEASURE_CH`] cells) and cropped to what the diagram drew; a
//!   narrower column scales it down, never up.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrite_core::settings::ReadingSize;
use futures::channel::mpsc;
use futures::{FutureExt, StreamExt};
use gpui::prelude::*;
use gpui::{canvas, div, img, px, AnyElement, App, AsyncApp, EntityId, Global, RenderImage, Window};
use serde_json::{json, Value};

use super::renderer;
use super::tokens::{self, Scheme};
use super::web::{Engine, Frame, Page, Viewport};
use crate::theme;

/// The fence language drawn as a diagram.
pub(crate) const LANGUAGE: &str = "mermaid";

/// mermaid.js, vendored (`assets/mermaid/README.md`).
const MERMAID_JS: &str = include_str!("../../assets/mermaid/mermaid.min.js");

/// What the drawn images may hold, in bytes, before the least recently
/// drawn are let go (a diagram at the Standard measure on a 2x display is
/// a few MB).
const BUDGET: usize = 64 * 1024 * 1024;
/// Longest a diagram may take to draw, Chromium's start aside.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The page's height while mermaid is still drawing (an empty page)...
const DRAWING: u32 = 0;
/// ...and once it refused the fence: the page sets itself one pixel tall.
const REFUSED: u32 = 1;

/// The page's script: draw the fence, or say it can't be drawn (see
/// [`REFUSED`]). `CONFIG` and `SOURCE` are JSON.
const DRAW_SCRIPT: &str = r#"(async () => {
  const out = document.getElementById("diagram");
  try {
    mermaid.initialize(CONFIG);
    const { svg } = await mermaid.render("ferrite-diagram", SOURCE);
    out.innerHTML = svg;
  } catch (_) {
    out.replaceChildren();
    document.documentElement.style.height = "1px";
  }
})();"#;

/// The process's diagrams (a GPUI global; [`init`] installs it).
pub(crate) struct Diagrams {
    /// Visuals are on (Settings): off, every fence stays code.
    enabled: bool,
    entries: HashMap<u64, Entry>,
    /// Bumped on every draw, so the least recently drawn can be found.
    clock: u64,
}

impl Global for Diagrams {}

enum Entry {
    /// Being drawn; these views painted the fence and redraw when it lands.
    Drawing(Vec<EntityId>),
    Drawn(Drawn),
    /// Mermaid refused it, or there is no engine: it stays code.
    Failed,
}

#[derive(Clone)]
struct Drawn {
    image: Arc<RenderImage>,
    /// Logical px.
    width: f32,
    height: f32,
    drawn_at: u64,
}

impl Drawn {
    fn bytes(&self) -> usize {
        self.image.size(0).width.0 as usize * self.image.size(0).height.0 as usize * 4
    }
}

/// Install the global: `enabled` is whether visuals are on (Settings).
pub(crate) fn init(enabled: bool, cx: &mut App) {
    cx.set_global(Diagrams { enabled, entries: HashMap::new(), clock: 0 });
}

/// Turn diagrams on or off (Settings changed); fences drawn again follow.
pub(crate) fn set_enabled(enabled: bool, cx: &mut App) {
    if cx.try_global::<Diagrams>().is_some_and(|diagrams| diagrams.enabled != enabled) {
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
    let enabled = cx.try_global::<Diagrams>().is_some_and(|diagrams| diagrams.enabled);
    if !enabled || !closed(fence) || code.trim().is_empty() {
        return element;
    }
    let reading = ReadingSize::nearest(size.round() as u8);
    let scale = window.scale_factor();
    let key = key(&code, reading, scale);
    let diagrams = cx.global_mut::<Diagrams>();
    diagrams.clock += 1;
    let now = diagrams.clock;
    match diagrams.entries.get_mut(&key) {
        Some(Entry::Drawn(drawn)) => {
            drawn.drawn_at = now;
            diagram(drawn.clone(), size)
        }
        Some(Entry::Failed) => element,
        // Asked for once its code is painted on screen (and waited on by
        // every view that paints it meanwhile).
        Some(Entry::Drawing(_)) | None => div()
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
/// streaming in isn't drawn yet.
pub(crate) fn closed(fence: &str) -> bool {
    let mut lines = fence.trim_end().lines();
    let Some(open) = lines.next() else { return false };
    let marker = if open.trim_start().starts_with("~~~") { '~' } else { '`' };
    lines.last().is_some_and(|line| {
        let line = line.trim();
        line.len() >= 3 && line.chars().all(|c| c == marker)
    })
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
    match diagrams.entries.get_mut(&key) {
        Some(Entry::Drawing(views)) => {
            if !views.contains(&view) {
                views.push(view);
            }
            return;
        }
        Some(_) => return,
        None => {
            diagrams.entries.insert(key, Entry::Drawing(vec![view]));
        }
    }
    cx.spawn(async move |cx| {
        let drawn = match renderer::engine(cx).await {
            Ok(engine) => draw(engine, page(&code, reading), width(reading), scale, cx).await,
            Err(why) => Err(why),
        };
        cx.update(|cx| land(key, drawn, cx));
    })
    .detach();
}

/// A diagram finished drawing (or didn't): keep it, redraw the views that
/// wait on it, and let the oldest go past the budget.
fn land(key: u64, drawn: Result<Drawn, String>, cx: &mut App) {
    let diagrams = cx.global_mut::<Diagrams>();
    diagrams.clock += 1;
    let entry = match drawn {
        Ok(drawn) => Entry::Drawn(Drawn { drawn_at: diagrams.clock, ..drawn }),
        Err(_) => Entry::Failed,
    };
    let views = match diagrams.entries.insert(key, entry) {
        Some(Entry::Drawing(views)) => views,
        _ => Vec::new(),
    };
    let evicted = evict(&mut diagrams.entries, key);
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

/// Let the least recently drawn images go until the rest fit [`BUDGET`],
/// never `keep` (just drawn). Returns them.
fn evict(entries: &mut HashMap<u64, Entry>, keep: u64) -> Vec<Arc<RenderImage>> {
    let mut drawn: Vec<(u64, u64, usize)> = entries
        .iter()
        .filter_map(|(key, entry)| match entry {
            Entry::Drawn(drawn) => Some((drawn.drawn_at, *key, drawn.bytes())),
            _ => None,
        })
        .collect();
    let mut held: usize = drawn.iter().map(|(_, _, bytes)| bytes).sum();
    drawn.sort_unstable();
    let mut evicted = Vec::new();
    for (_, key, bytes) in drawn {
        if held <= BUDGET {
            break;
        }
        if key == keep {
            continue;
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
/// drew. Fails when mermaid refuses the fence or nothing settles in time.
async fn draw(
    engine: Rc<dyn Engine>,
    page: Page,
    width: u32,
    scale: f32,
    cx: &AsyncApp,
) -> Result<Drawn, String> {
    let bg = background();
    let (tx, mut rx) = mpsc::unbounded::<()>();
    let view = engine.open(
        page,
        Viewport { width, scale },
        Box::new(move || {
            let _ = tx.unbounded_send(());
        }),
    );
    let deadline = Instant::now() + TIMEOUT;
    let mut frame: Option<Frame> = None;
    let mut height = DRAWING;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(format!("the diagram didn't draw within {} s", TIMEOUT.as_secs()));
        }
        let drawn = height > REFUSED && renderer::frame_matches(frame.as_ref(), Some(height), scale);
        let wait = if drawn { renderer::SETTLE.min(left) } else { left };
        futures::select_biased! {
            woke = rx.next() => {
                if woke.is_none() {
                    return Err("the engine closed the page".into());
                }
                let updates = view.take();
                if let Some(h) = updates.content_height {
                    height = h;
                }
                if updates.frame.is_some() {
                    frame = updates.frame;
                }
                if height == REFUSED {
                    return Err("mermaid refused the diagram".into());
                }
            }
            _ = cx.background_executor().timer(wait).fuse() => {
                if drawn {
                    break;
                }
            }
        }
    }
    drop(view);
    let frame = frame.and_then(|frame| crop(frame, bg)).ok_or("the diagram drew nothing")?;
    let (width, height) = (frame.width as f32 / scale, frame.height as f32 / scale);
    let image = image::RgbaImage::from_raw(frame.width, frame.height, frame.bgra)
        .ok_or("a frame is width × height × 4 bytes")?;
    Ok(Drawn {
        image: Arc::new(RenderImage::new(vec![image::Frame::new(image)])),
        width,
        height,
        drawn_at: 0,
    })
}

/// The page's background as BGRA bytes' first three (what an undrawn pixel is).
fn background() -> [u8; 3] {
    let hex = tokens::value("bg", Scheme::APP, ReadingSize::STANDARD).unwrap_or_default();
    let rgb = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(theme::PLANE);
    [rgb as u8, (rgb >> 8) as u8, (rgb >> 16) as u8]
}

/// `frame` without the background to the right of what it drew; `None`
/// when it drew nothing.
fn crop(frame: Frame, bg: [u8; 3]) -> Option<Frame> {
    let stride = frame.width as usize * 4;
    let drawn = |x: usize| {
        frame.bgra.chunks_exact(stride).any(|row| row[x * 4..x * 4 + 3] != bg)
    };
    let width = (0..frame.width as usize).rev().find(|x| drawn(*x))? + 1;
    let bgra = frame.bgra.chunks_exact(stride).flat_map(|row| &row[..width * 4]).copied().collect();
    Some(Frame { width: width as u32, height: frame.height, bgra })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_closed_fence_is_drawn() {
        assert!(closed("```mermaid\ngraph TD\n  a --> b\n```"));
        assert!(closed("```mermaid\ngraph TD\n```\n"));
        assert!(closed("~~~mermaid\ngraph TD\n~~~"));
        assert!(!closed("```mermaid\ngraph TD\n  a --> b"), "still streaming");
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
        assert!(page.html.contains("globalThis[\"mermaid\"]"), "the bundle is inlined");
        assert_eq!(page.html.matches("</script>").count(), 2, "the fence can't close a script");
        let escaped = Value::from(code).to_string().replace('<', &format!("{}u003c", '\\'));
        assert!(page.html.contains(&escaped), "the fence, as a JSON string with no `<`");
        assert!(!page.html.replace(MERMAID_JS, "").contains("src="), "nothing is fetched");
        assert_eq!(page.theme_css, tokens::theme_css(Scheme::APP, ReadingSize::STANDARD));
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
        let px = |drawn: bool| if drawn { [9, 9, 9, 255] } else { [1, 2, 3, 255] };
        // Two rows of four: something at x = 0 and, lower, at x = 1.
        let bgra = [px(true), px(false), px(false), px(false), px(false), px(true), px(false), px(false)].concat();
        let cropped = crop(Frame { width: 4, height: 2, bgra }, bg).unwrap();
        assert_eq!((cropped.width, cropped.height), (2, 2));
        assert_eq!(cropped.bgra, [px(true), px(false), px(false), px(true)].concat());
        let blank = Frame { width: 2, height: 1, bgra: [px(false), px(false)].concat() };
        assert!(crop(blank, bg).is_none());
    }

    #[test]
    fn the_least_recently_drawn_go_past_the_budget() {
        let image = |side: u32| {
            let buffer = image::RgbaImage::new(side, side);
            Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
        };
        // Each image is 4 × side² bytes: three of a third of the budget each.
        let side = ((BUDGET / 3 / 4) as f64).sqrt() as u32 + 1;
        let mut entries = HashMap::new();
        for (key, drawn_at) in [(1, 3), (2, 1), (3, 2)] {
            let drawn = Drawn { image: image(side), width: 1., height: 1., drawn_at };
            entries.insert(key, Entry::Drawn(drawn));
        }
        entries.insert(4, Entry::Failed);
        let evicted = evict(&mut entries, 2);
        assert_eq!(evicted.len(), 1);
        assert!(!entries.contains_key(&3), "the oldest but the one kept");
        assert!(entries.contains_key(&2) && entries.contains_key(&1) && entries.contains_key(&4));
    }
}
