//! The spike app: a transcript-like virtualized list of 400 rows. Every 30th
//! row from 100 is an interactive web page drawn inline (one per
//! [`page::Demo`]: treemap 100, mockups 130, stats 160, diagram 190, timeline
//! 220), each introduced by a user prompt on the row before. A GPUI popover
//! sits over the list, and the header shows live metrics.
//!
//! Keys: `t` theme, `p` popover, `q` quit. Flags:
//! - `--at <row>` start scrolled to a row (default 97, so the treemap is on
//!   screen); `--demo <name>` is shorthand for the row just above that demo.
//! - `--autopilot` drives the demo on screen (the first demo at or after the
//!   start row): sweeps a synthetic pointer over it, hovers and clicks what
//!   that demo is about, and prints the numbers. `--tour` does that for every
//!   demo in turn, scrolling each into view (so all five views end up open),
//!   and prints first-frame times and the RSS of the app and its helpers.
//! - `--quit` quits after the autopilot.
//!   After the five, the tour checks memory: physical footprint of the app
//!   and its helpers with every view opened, after scrolling away from all of
//!   them and waiting past the release grace period, and after scrolling back
//!   to one (also timing that view's first new frame).
//! - `--scroll-bench` opens every view, parks at the bottom until they are
//!   released, then flings up and down through all five several times at
//!   [`FLING_SPEED`] (or `INLINE_CEF_FLING=<px/s>`), one scroll step per
//!   display frame. Prints frame
//!   intervals, height jumps of the visible content, and main-thread time in
//!   the engine's pump.
//! - `--shots <dir>` (needs the `shots` feature) has the autopilot save
//!   rendered frames of the window there (`loaded`, `sweep`, `hover`, and
//!   per-demo extras; prefixed with the demo's name under `--tour`; `back-*`
//!   after the tour scrolls back; `fling-*` from an extra, untimed fling).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    div, list, point, prelude::*, px, rgb, size, App, Bounds, Context, Entity, FocusHandle,
    KeyDownEvent, ListAlignment, ListOffset, ListState, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, PlatformInput, Point, Rgba, TitlebarOptions, Window,
    WindowBounds, WindowOptions,
};
use inline_cef::element::{self, HeightChanged, Reach, WebView, PLACEHOLDER_HEIGHT};
use inline_cef::page::Demo;
use inline_cef::{page, web};

/// Long enough that the bottom is several screens from the last demo, so
/// scrolling there lets every view go.
const ROWS: usize = 400;
const FIRST_DEMO_ROW: usize = 100;
const DEMO_SPACING: usize = 30;
const MONO: &str = "Menlo";

/// Where a web view sits inside its row: the row's padding, the lead line and
/// its gap, the indent, and the border.
const ROW_PAD_X: f32 = 18.;
const ROW_PAD_Y: f32 = 10.;
const LEAD: f32 = 20. + 8.;
const INDENT: f32 = 20.;
const BORDER: f32 = 1.;

fn demo_row(index: usize) -> usize {
    FIRST_DEMO_ROW + DEMO_SPACING * index
}

/// The demo drawn in row `ix`, and its index in [`Demo::ALL`].
fn demo_at(ix: usize) -> Option<(usize, Demo)> {
    let k = ix.checked_sub(FIRST_DEMO_ROW)?;
    (k % DEMO_SPACING == 0).then(|| k / DEMO_SPACING).and_then(|i| Demo::ALL.get(i).map(|d| (i, *d)))
}

fn main() {
    // --- engine boot (owned by the CEF adapter) ---
    #[cfg(feature = "cef")]
    let engine: Rc<dyn web::Engine> = match web::cef::boot() {
        // Must run before GPUI exists: CEF helper processes are this binary
        // (they exit here), and NSApp must be made CEF-compatible first.
        Ok(web::cef::Boot::Subprocess(code)) => std::process::exit(code),
        Ok(web::cef::Boot::Engine(engine)) => Rc::new(engine),
        Err(error) => {
            // No silent fallback to the fake: its numbers would be mislabelled.
            eprintln!("inline-cef: Chromium failed to start: {error}");
            std::process::exit(1);
        }
    };
    // --- end engine boot ---
    #[cfg(not(feature = "cef"))]
    let engine: Rc<dyn web::Engine> = Rc::new(web::fake::FakeEngine::new());

    let args = Args::parse();
    let engine_name = if cfg!(feature = "cef") { "chromium (cef)" } else { "fake" };

    gpui::application().run(move |cx: &mut App| {
        element::pump(engine.clone(), cx);
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        let bounds = Bounds::centered(None, size(px(1100.), px(820.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("inline-cef spike".into()),
                    ..Default::default()
                }),
                // Measuring frame times needs a window nothing covers: an
                // occluded window loses its display link and draws at ~24 Hz.
                kind: if std::env::var_os("INLINE_CEF_ON_TOP").is_some() {
                    gpui::WindowKind::PopUp
                } else {
                    gpui::WindowKind::Normal
                },
                ..Default::default()
            },
            |window, cx| {
                let spike = cx.new(|cx| Spike::new(engine, engine_name, &args, window, cx));
                spike.update(cx, |spike, cx| window.focus(&spike.focus, cx));
                spike
            },
        )
        .expect("the window opens");
        cx.activate(true);
    });
}

struct Args {
    at: usize,
    autopilot: bool,
    tour: bool,
    bench: bool,
    quit: bool,
    shots: Option<std::path::PathBuf>,
}

impl Args {
    fn parse() -> Self {
        let mut args =
            Args { at: FIRST_DEMO_ROW - 3, autopilot: false, tour: false, bench: false, quit: false, shots: None };
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--at" => args.at = it.next().and_then(|v| v.parse().ok()).unwrap_or(args.at),
                "--demo" => match it.next().as_deref().and_then(Demo::from_title) {
                    Some(demo) => {
                        let i = Demo::ALL.iter().position(|d| *d == demo).unwrap();
                        args.at = demo_row(i) - 1;
                    }
                    None => eprintln!(
                        "--demo takes one of: {}",
                        Demo::ALL.map(|d| d.title()).join(", ")
                    ),
                },
                "--autopilot" => args.autopilot = true,
                "--tour" => {
                    args.autopilot = true;
                    args.tour = true;
                }
                "--scroll-bench" => args.bench = true,
                "--quit" => args.quit = true,
                "--shots" => args.shots = it.next().map(Into::into),
                other => eprintln!("ignoring {other}"),
            }
        }
        args
    }
}

struct Spike {
    list: ListState,
    /// One view per demo, in [`Demo::ALL`] order. A view opens when its row
    /// comes within [`NEAR`] viewports of the screen (or is laid out).
    views: Vec<Entity<WebView>>,
    /// Which views are near, and where off-screen rows are.
    proximity: Proximity,
    /// The view the header reports on.
    active: usize,
    engine_name: &'static str,
    dark: bool,
    popover: bool,
    focus: FocusHandle,
    rss_kb: Rc<Cell<u64>>,
    autopilot: Rc<Cell<&'static str>>,
}

impl Spike {
    fn new(
        engine: Rc<dyn web::Engine>,
        engine_name: &'static str,
        args: &Args,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let started = Instant::now();
        let list = ListState::new(ROWS, ListAlignment::Top, px(200.));
        list.scroll_to(ListOffset { item_ix: args.at.min(ROWS - 1), offset_in_item: px(0.) });
        let views: Vec<_> =
            Demo::ALL.iter().map(|demo| WebView::new(engine.clone(), demo.page(true), cx)).collect();
        // A page that changes height off-screen (it loaded, or reloaded) gets
        // its row remeasured now, so the list doesn't resize it under the user
        // when it scrolls in.
        for (i, view) in views.iter().enumerate() {
            cx.subscribe(view, move |spike: &mut Spike, _, _: &HeightChanged, _| {
                let row = demo_row(i);
                spike.list.remeasure_items(row..row + 1);
            })
            .detach();
        }
        // The header follows the first demo at or below the start row.
        let active = Demo::ALL
            .iter()
            .enumerate()
            .position(|(i, _)| demo_row(i) >= args.at)
            .unwrap_or(Demo::ALL.len() - 1);

        // Live header: refresh twice a second, sample RSS every two.
        let rss_kb = Rc::new(Cell::new(0));
        let rss = rss_kb.clone();
        cx.spawn(async move |this, cx| {
            let mut tick = 0u32;
            loop {
                if tick % 4 == 0 {
                    let kb = cx.background_executor().spawn(async { rss_kb_now() }).await;
                    rss.set(kb);
                }
                tick += 1;
                cx.background_executor().timer(Duration::from_millis(500)).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();

        cx.on_app_quit(|this: &mut Spike, cx| {
            for (demo, view) in Demo::ALL.iter().zip(&this.views) {
                let stats = view.read(cx).stats();
                if stats.frames > 0 {
                    println!("inline-cef [{}] {:<8} {stats}", this.engine_name, demo.title());
                }
            }
            println!("inline-cef [{}] rss {}", this.engine_name, mb(this.rss_kb.get()));
            async {}
        })
        .detach();

        let autopilot = Rc::new(Cell::new(""));
        if args.autopilot {
            let targets: Vec<usize> =
                if args.tour { (0..Demo::ALL.len()).collect() } else { vec![active] };
            let plan = Autopilot {
                spike: cx.entity(),
                targets,
                tour: args.tour,
                quit: args.quit,
                shots: args.shots.clone(),
                status: autopilot.clone(),
                started,
            };
            plan.run(window, cx);
        } else if args.bench {
            scroll_bench(cx.entity(), args.quit, args.shots.clone(), autopilot.clone(), window, cx);
        }

        Spike {
            list,
            proximity: Proximity::new(),
            views,
            active,
            engine_name,
            dark: true,
            popover: true,
            focus: cx.focus_handle(),
            rss_kb,
            autopilot,
        }
    }

    fn set_dark(&mut self, dark: bool, cx: &mut Context<Self>) {
        self.dark = dark;
        let css = page::theme_css(dark);
        for view in &self.views {
            view.update(cx, |web, cx| web.set_theme(css.clone(), cx));
        }
        cx.notify();
    }

    fn key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "t" => self.set_dark(!self.dark, cx),
            "p" => self.popover = !self.popover,
            "q" => cx.quit(),
            _ => return,
        }
        cx.notify();
    }
}

/// Keep views live within this many viewport heights of the screen: they open
/// (or reload) here, before they scroll in.
const NEAR: f32 = 1.5;
/// ...and let them go beyond this many. The gap keeps a row hovering at the
/// edge from flapping; the engine's grace period then delays the release.
const FAR: f32 = 2.5;
/// Height assumed for a row the list has never measured. Low on purpose:
/// underestimating distance only opens a view a little early.
const ROW_ESTIMATE: f32 = 20.;

/// Where the demo rows are relative to the screen, and which are near.
///
/// A virtualized list only lays out what is on screen, and `ListState`
/// exposes positions only for measured rows at or below its top row. So
/// this remembers every row height it has seen and sums them (or
/// [`ROW_ESTIMATE`]) to place rows above the top or not yet measured.
struct Proximity {
    heights: Vec<Option<f32>>,
    near: Vec<bool>,
    /// The list's scroll top and viewport when last looked at.
    seen: Option<(usize, Pixels, Bounds<Pixels>)>,
}

impl Proximity {
    fn new() -> Self {
        Proximity { heights: vec![None; ROWS], near: vec![false; Demo::ALL.len()], seen: None }
    }

    /// Whether the list scrolled or resized since the last call.
    fn moved(&mut self, list: &ListState) -> bool {
        let top = list.logical_scroll_top();
        let now = (top.item_ix, top.offset_in_item, list.viewport_bounds());
        self.seen.replace(now) != Some(now)
    }

    /// Reach changes since the last call, as (demo index, reach).
    fn update(&mut self, list: &ListState) -> Vec<(usize, Reach)> {
        let viewport = list.viewport_bounds();
        let span = f32::from(viewport.size.height);
        if span <= 0. {
            return Vec::new();
        }
        self.observe(list, span * FAR);
        let width = viewport.size.width - px(2. * ROW_PAD_X + INDENT + 2. * BORDER);
        let mut changes = Vec::new();
        for i in 0..Demo::ALL.len() {
            let distance = self.distance(list, demo_row(i), span * FAR);
            let near = if self.near[i] { distance <= span * FAR } else { distance <= span * NEAR };
            if near != self.near[i] {
                self.near[i] = near;
                changes.push((i, if near { Reach::Near { width } } else { Reach::Far }));
            }
        }
        changes
    }

    /// Record the heights of the rows the list has measured from its top row
    /// down to `beyond` px below the screen.
    fn observe(&mut self, list: &ListState, beyond: f32) {
        let bottom = list.viewport_bounds().bottom() + px(beyond);
        let mut ix = list.logical_scroll_top().item_ix;
        while let Some(bounds) = (ix < ROWS).then(|| list.bounds_for_item(ix)).flatten() {
            self.heights[ix] = Some(f32::from(bounds.size.height));
            if bounds.bottom() > bottom {
                break;
            }
            ix += 1;
        }
    }

    fn height(&self, list: &ListState, ix: usize) -> f32 {
        list.bounds_for_item(ix)
            .map(|b| f32::from(b.size.height))
            .or(self.heights[ix])
            .unwrap_or(ROW_ESTIMATE)
    }

    /// Px between row `ix` and the screen (0 when any of it is on screen),
    /// summed no further than `cap`.
    fn distance(&self, list: &ListState, ix: usize, cap: f32) -> f32 {
        let viewport = list.viewport_bounds();
        let top = list.logical_scroll_top();
        if ix < top.item_ix {
            // Above: the hidden part of the top row, plus every row between.
            let mut d = f32::from(top.offset_in_item);
            for r in (ix + 1..top.item_ix).rev() {
                if d > cap {
                    break;
                }
                d += self.height(list, r);
            }
            return d;
        }
        if let Some(b) = list.bounds_for_item(ix) {
            return f32::from((b.top() - viewport.bottom()).max(viewport.top() - b.bottom()).max(px(0.)));
        }
        // Below and never measured: walk down from the top row.
        let mut y = f32::from(viewport.top() - top.offset_in_item);
        let end = f32::from(viewport.bottom());
        for r in top.item_ix..ix {
            y += self.height(list, r);
            if y - end > cap {
                break;
            }
        }
        (y - end).max(0.)
    }
}

/// The host's own colours, matching the page's tokens.
struct Ink {
    bg: Rgba,
    band: Rgba,
    float: Rgba,
    fg: Rgba,
    strong: Rgba,
    muted: Rgba,
    faint: Rgba,
    line: Rgba,
    line2: Rgba,
    accent: Rgba,
    green: Rgba,
    magenta: Rgba,
}

impl Ink {
    fn new(dark: bool) -> Self {
        if dark {
            Ink {
                bg: rgb(0x1b1b1c),
                band: rgb(0x242425),
                float: rgb(0x2a2a2c),
                fg: rgb(0xd7d7d9),
                strong: rgb(0xf4f4f5),
                muted: rgb(0x98989d),
                faint: rgb(0x5e5e63),
                line: rgb(0x2f2f31),
                line2: rgb(0x3c3c3f),
                accent: rgb(0x8eb1f0),
                green: rgb(0x93cf8c),
                magenta: rgb(0xc59df0),
            }
        } else {
            Ink {
                bg: rgb(0xfbfbfa),
                band: rgb(0xf1f1f0),
                float: rgb(0xffffff),
                fg: rgb(0x2b2b2e),
                strong: rgb(0x111113),
                muted: rgb(0x6c6c72),
                faint: rgb(0xa3a3a8),
                line: rgb(0xe3e3e2),
                line2: rgb(0xd2d2d1),
                accent: rgb(0x2f63c8),
                green: rgb(0x3b8a36),
                magenta: rgb(0x8247c4),
            }
        }
    }
}

impl Render for Spike {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // From the last layout: which views should be live.
        for (i, reach) in self.proximity.update(&self.list) {
            self.views[i].update(cx, |web, cx| web.set_reach(reach, window, cx));
        }
        // That was the previous layout; if the list has moved since, look
        // again after this frame's.
        if self.proximity.moved(&self.list) {
            window.request_animation_frame();
        }
        let ink = Rc::new(Ink::new(self.dark));
        let stats = self.views[self.active].read(cx).stats();
        let open = self.views.iter().filter(|v| v.read(cx).stats().frames > 0).count();
        let views = Rc::new(self.views.clone());
        let row_ink = ink.clone();
        let rows = list(self.list.clone(), move |ix, _, _| row(ix, &views, &row_ink))
            .flex_1()
            .size_full();
        let title = Demo::ALL[self.active].title();

        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .size_full()
            .flex()
            .flex_col()
            .bg(ink.bg)
            .text_color(ink.fg)
            .font_family(MONO)
            .text_size(px(12.))
            .line_height(px(20.))
            .child(header(&ink, self.engine_name, title, open, &stats, self.rss_kb.get(), self.autopilot.get()))
            .child(div().flex_1().relative().overflow_hidden().child(rows).when(
                self.popover,
                |area| area.child(popover(&ink)),
            ))
    }
}

fn header(
    ink: &Ink,
    engine: &str,
    title: &str,
    open: usize,
    stats: &element::Summary,
    rss_kb: u64,
    autopilot: &str,
) -> impl IntoElement {
    let ms = |d: Option<Duration>| d.map_or("-".into(), |d| format!("{:.1}ms", d.as_secs_f64() * 1e3));
    let cell = |label: &str, value: String| {
        div()
            .flex()
            .gap(px(6.))
            .child(div().text_color(ink.faint).child(label.to_string()))
            .child(div().text_color(ink.strong).child(value))
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(18.))
        .h(px(30.))
        .px(px(14.))
        .bg(ink.band)
        .border_b_1()
        .border_color(ink.line)
        .child(div().text_color(ink.accent).child(format!("inline-cef · {engine}")))
        .child(cell("views", format!("{open}/{}", Demo::ALL.len())))
        .child(cell(title, format!("p50 {}", ms(stats.p50))))
        .child(cell("p95", ms(stats.p95)))
        .child(cell("frames", stats.frames.to_string()))
        .child(cell("idle", stats.idle_frames.to_string()))
        .child(cell("rss", mb(rss_kb)))
        .child(div().flex_1())
        .child(div().text_color(ink.magenta).child(autopilot.to_string()))
        .child(div().text_color(ink.faint).child("t · p · q"))
}

/// What the user asked, and how the agent introduces each demo. Keep the
/// introduction to one line at the default width: the autopilot assumes it.
fn intro(demo: Demo) -> (&'static str, &'static str) {
    match demo {
        Demo::Treemap => (
            "draw me a treemap of the repo",
            "Here's the repo by size. Hover a tile for details, click a directory to open it, switch to Churn to see where the edits land.",
        ),
        Demo::Mockups => (
            "the settled notice and the compact banner keep stacking up over the composer. show me options",
            "Three ways to lay it out, same thread state. Buttons are live; flip the app chrome with dark/light to check both.",
        ),
        Demo::Stats => (
            "how much have I spent on tokens this month?",
            "$671 over the last 30 days, up 29% on the 30 before; the Sep 24-25 refactor is the peak. Hover for a day's breakdown.",
        ),
        Demo::Diagram => (
            "how do the crates depend on each other now?",
            "Five layers and no cycles. Hover a module to trace what it depends on (blue) and what uses it (purple); click to pin.",
        ),
        Demo::Timeline => (
            "why did CI fail on the inline-cef branch?",
            "Run #4182 failed in test (macos): two tests, one of them flaky. Click a failure for its backtrace, or a job for its steps.",
        ),
    }
}

fn row(ix: usize, views: &[Entity<WebView>], ink: &Ink) -> gpui::AnyElement {
    let base = div().w_full().px(px(ROW_PAD_X));
    if let Some((i, demo)) = demo_at(ix) {
        return base
            .id(("demo", ix))
            .py(px(ROW_PAD_Y))
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .child(div().text_color(ink.green).child("●"))
                    .child(div().flex_1().min_w_0().text_color(ink.fg).child(intro(demo).1)),
            )
            .child(
                div()
                    .mt(px(8.))
                    .ml(px(INDENT))
                    .border_1()
                    .border_color(ink.line2)
                    .child(views[i].clone()),
            )
            .into_any_element();
    }
    if let Some((_, demo)) = demo_at(ix + 1) {
        return base
            .flex()
            .gap(px(10.))
            .pt(px(6.))
            .child(div().w(px(10.)).flex_none().text_color(ink.accent).child("❯"))
            .child(div().flex_1().text_color(ink.strong).child(intro(demo).0))
            .into_any_element();
    }
    let (gutter, gutter_ink, text, text_ink) = transcript_line(ix, ink);
    base.flex()
        .gap(px(10.))
        .child(div().w(px(10.)).flex_none().text_color(gutter_ink).child(gutter))
        .child(div().flex_1().text_color(text_ink).child(text))
        .into_any_element()
}

/// A deterministic, transcript-shaped line for row `ix`.
fn transcript_line(ix: usize, ink: &Ink) -> (&'static str, Rgba, String, Rgba) {
    const PROMPTS: [&str; 6] = [
        "why does the nav jitter when a stream starts?",
        "show me where the transcript allocates per frame",
        "can the board keep its scroll position across reloads?",
        "what's the biggest file under crates/?",
        "make the composer's status line match the titlebar",
        "does the popover still sit above the web view?",
    ];
    const SAYS: [&str; 6] = [
        "The list remeasures every visible row on stream start, so heights settle a frame late.",
        "Two places: the markdown re-parse on each delta, and the selection hitboxes for clipped text.",
        "Yes. The ListState offset can be persisted with the thread and restored before first layout.",
        "crates/ferrite/src/theme.rs at 3,013 lines, then rich.rs and transcript.rs.",
        "Both now read FS_UI on LH_UI; the composer had a stray 14px override.",
        "Yes: it's a GPUI element stacked over the view's image, no child window involved.",
    ];
    const TOOLS: [&str; 5] = [
        "Read crates/ferrite/src/transcript.rs (2,410 lines)",
        "Grep \"remeasure\" in crates/ — 14 matches",
        "Edit crates/ferrite/src/composer.rs (+3 −5)",
        "Bash cargo test -p ferrite --lib — 412 passed",
        "Read vendor/gpui-pre/src/elements/list.rs (1,910 lines)",
    ];
    match ix % 7 {
        0 => ("❯", ink.accent, PROMPTS[(ix / 7) % PROMPTS.len()].to_string(), ink.strong),
        1 | 4 => ("●", ink.green, SAYS[(ix / 7 + ix % 3) % SAYS.len()].to_string(), ink.fg),
        2 | 3 | 5 => (" ", ink.faint, format!("⎿ {}", TOOLS[(ix * 3 + ix / 7) % TOOLS.len()]), ink.muted),
        _ => (" ", ink.faint, format!("· row {ix}"), ink.faint),
    }
}

fn popover(ink: &Ink) -> impl IntoElement {
    div()
        .absolute()
        .top(px(150.))
        .left(px(520.))
        .w(px(300.))
        .p(px(12.))
        .bg(ink.float)
        .border_1()
        .border_color(ink.line2)
        .shadow_md()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(div().text_color(ink.strong).child("GPUI popover"))
        .child(div().text_color(ink.muted).child(
            "An ordinary GPUI element, stacked over the web view's image. No native child window, so it just works.",
        ))
        .child(div().text_color(ink.faint).child("p to hide"))
}

// ---------------------------------------------------------------------------
// Autopilot

/// One thing the autopilot does to a demo after loading and sweeping it.
/// Positions are logical px in the page (the view's top-left is 0,0), laid
/// out for the default 1100 px window, where views are 1042 px wide.
#[derive(Clone, Copy)]
enum Act {
    Hover(f32, f32),
    Click(f32, f32),
    Shot(&'static str),
    Wait(u64),
    /// Host theme: dark or light (restyles every view).
    Dark(bool),
    /// Print the row's height, labelled.
    Height(&'static str),
    /// Scroll the list so page y sits at the top of the list.
    ScrollTo(f32),
    /// Scroll back so the prompt row above the demo is at the top.
    ScrollHome,
    /// Move the pointer off every view.
    Away,
}

/// What to poke on each demo, beyond the generic sweep.
fn script(demo: Demo) -> Vec<Act> {
    use Act::*;
    match demo {
        Demo::Treemap => vec![
            Hover(229., 150.),
            Wait(1500),
            Shot("hover"),
            Dark(false),
            Wait(800),
            Shot("light"),
            Dark(true),
            Away,
            ScrollTo(150.),
            Wait(800),
            Shot("clipped"),
            ScrollHome,
        ],
        Demo::Mockups => vec![
            Hover(MOCK_COMPACT.0, MOCK_COMPACT.1),
            Wait(600),
            Shot("hover"),
            Click(MOCK_MODEL.0, MOCK_MODEL.1),
            Hover(MOCK_MENU_ITEM.0, MOCK_MENU_ITEM.1),
            Wait(600),
            Shot("menu"),
            Click(MOCK_LIGHT.0, MOCK_LIGHT.1),
            Hover(MOCK_STATUS.0, MOCK_STATUS.1),
            Wait(600),
            Shot("app-light"),
        ],
        Demo::Stats => vec![
            Hover(STATS_LINE.0, STATS_LINE.1),
            Wait(600),
            Shot("hover"),
            Click(STATS_SORT.0, STATS_SORT.1),
            Hover(STATS_BAR.0, STATS_BAR.1),
            Wait(600),
            Shot("sorted"),
            Dark(false),
            Hover(STATS_LINE.0 - 180., STATS_LINE.1),
            Wait(800),
            Shot("light"),
            Dark(true),
        ],
        Demo::Diagram => vec![
            Hover(DIAGRAM_AGENT.0, DIAGRAM_AGENT.1),
            Wait(600),
            Shot("hover"),
            Click(DIAGRAM_THEME.0, DIAGRAM_THEME.1),
            Wait(600),
            Shot("pinned"),
        ],
        Demo::Timeline => vec![
            Hover(TIMELINE_BAR.0, TIMELINE_BAR.1),
            Wait(600),
            Shot("hover"),
            Height("collapsed"),
            Click(TIMELINE_FAIL.0, TIMELINE_FAIL.1),
            Click(TIMELINE_JOB.0, TIMELINE_JOB.1),
            Away,
            Wait(800),
            Height("expanded"),
            ScrollTo(300.),
            Wait(600),
            Shot("expanded"),
            ScrollHome,
        ],
    }
}

// Targets in page coordinates at 1042 px wide (measured in a desktop browser).
const MOCK_COMPACT: (f32, f32) = (266., 311.);
const MOCK_MODEL: (f32, f32) = (851., 310.);
const MOCK_MENU_ITEM: (f32, f32) = (895., 257.);
const MOCK_LIGHT: (f32, f32) = (1002., 15.);
const MOCK_STATUS: (f32, f32) = (836., 345.);
const STATS_LINE: (f32, f32) = (600., 200.);
const STATS_SORT: (f32, f32) = (989., 421.);
const STATS_BAR: (f32, f32) = (293., 520.);
const DIAGRAM_AGENT: (f32, f32) = (397., 169.);
const DIAGRAM_THEME: (f32, f32) = (397., 227.);
const TIMELINE_BAR: (f32, f32) = (826., 329.);
const TIMELINE_FAIL: (f32, f32) = (300., 449.);
const TIMELINE_JOB: (f32, f32) = (100., 329.);

struct Autopilot {
    spike: Entity<Spike>,
    targets: Vec<usize>,
    tour: bool,
    quit: bool,
    shots: Option<std::path::PathBuf>,
    status: Rc<Cell<&'static str>>,
    /// When the app started (first-frame times outside `--tour` count from here).
    started: Instant,
}

impl Autopilot {
    fn run(self, window: &mut Window, cx: &mut Context<Spike>) {
        cx.spawn_in(window, async move |_, cx| {
            let executor = cx.background_executor().clone();
            let wait = |ms| executor.timer(Duration::from_millis(ms));
            let spike = self.spike.clone();
            let mut report = Vec::new();
            self.status.set("autopilot: loading");
            if !self.tour {
                wait(300).await;
            }
            for &i in &self.targets {
                let demo = Demo::ALL[i];
                let row = demo_row(i);
                let name = |shot: &str| if self.tour { format!("{}-{shot}", demo.title()) } else { shot.to_owned() };

                // Bring it on screen and time its first frames.
                let t0 = if self.tour {
                    spike.update(cx, |s, cx| {
                        s.active = i;
                        // The popover is the treemap's exhibit; elsewhere it hides the page.
                        s.popover = demo == Demo::Treemap;
                        s.list.scroll_to(ListOffset { item_ix: row - 1, offset_in_item: px(0.) });
                        cx.notify();
                    });
                    Instant::now()
                } else {
                    spike.update(cx, |s, cx| {
                        s.popover = demo == Demo::Treemap;
                        cx.notify();
                    });
                    self.started
                };
                let (mut first, mut sized) = (None, None);
                let placeholder = ROW_PAD_Y * 2. + LEAD + 2. * BORDER + PLACEHOLDER_HEIGHT;
                while t0.elapsed() < Duration::from_secs(8) && sized.is_none() {
                    wait(4).await;
                    let (frames, height) = spike.update(cx, |s, cx| {
                        let frames = s.views[i].read(cx).stats().frames;
                        (frames, s.list.bounds_for_item(row).map(|b| f32::from(b.size.height)))
                    });
                    if first.is_none() && frames > 0 {
                        first = Some(t0.elapsed());
                    }
                    if first.is_some() && height.is_some_and(|h| (h - placeholder).abs() > 0.5) {
                        sized = Some(t0.elapsed());
                    }
                }
                self.status.set("autopilot: settling");
                wait(1200).await;
                shot(cx, &self.shots, &name("loaded"));

                // Sweep the visible part of the view at ~60 Hz. (The window
                // only lays out when it draws; an occluded window may not
                // have yet, so give it a moment.)
                let mut rect = None;
                for _ in 0..50 {
                    rect = spike.update(cx, |s, _| view_rect(&s.list, row));
                    if rect.is_some() {
                        break;
                    }
                    wait(100).await;
                }
                let Some((origin, view)) = rect else {
                    eprintln!("autopilot: {} is not on screen (use --at {})", demo.title(), row - 1);
                    continue;
                };
                self.status.set("autopilot: sweeping");
                let steps = 300;
                let (w, h) = (f32::from(view.width), f32::from(view.height).max(80.));
                for k in 0..steps {
                    let t = k as f32 / steps as f32;
                    let x = 12. + (w - 24.) * (0.5 - 0.5 * (t * std::f32::consts::TAU * 2.).cos());
                    let y = 40. + (h - 60.) * (0.5 + 0.5 * (t * std::f32::consts::TAU * 3.).sin());
                    move_to(cx, origin + point(px(x), px(y)));
                    wait(16).await;
                    if k == steps / 3 {
                        shot(cx, &self.shots, &name("sweep"));
                    }
                }
                let sweep_stats = spike.update(cx, |s, cx| s.views[i].read(cx).stats());

                // Then whatever this demo is about.
                self.status.set("autopilot: poking");
                let mut heights = Vec::new();
                let mut hover_ms = Vec::new();
                for act in script(demo) {
                    let origin = spike.update(cx, |s, _| view_rect(&s.list, row).map(|(o, _)| o));
                    let at = |x: f32, y: f32| origin.map(|o| o + point(px(x), px(y)));
                    match act {
                        Act::Hover(x, y) => {
                            if let Some(p) = at(x, y) {
                                let before = spike.update(cx, |s, cx| s.views[i].read(cx).stats().frames);
                                let t = Instant::now();
                                move_to(cx, p);
                                // Time to the first frame that answers the hover.
                                while t.elapsed() < Duration::from_millis(500) {
                                    wait(2).await;
                                    if spike.update(cx, |s, cx| s.views[i].read(cx).stats().frames) > before {
                                        hover_ms.push(t.elapsed().as_secs_f64() * 1e3);
                                        break;
                                    }
                                }
                            }
                        }
                        Act::Click(x, y) => {
                            if let Some(p) = at(x, y) {
                                move_to(cx, p);
                                wait(50).await;
                                click(cx, p);
                                wait(250).await;
                            }
                        }
                        Act::Shot(n) => shot(cx, &self.shots, &name(n)),
                        Act::Wait(ms) => wait(ms).await,
                        Act::Dark(dark) => spike.update(cx, |s, cx| s.set_dark(dark, cx)),
                        Act::Height(label) => {
                            let h = spike.update(cx, |s, _| view_rect(&s.list, row).map(|(_, v)| f32::from(v.height)));
                            heights.push(format!("{label} {}px", h.map_or("?".into(), |h| h.to_string())));
                        }
                        Act::ScrollTo(y) => spike.update(cx, |s, cx| {
                            let into = ROW_PAD_Y + LEAD + BORDER + y;
                            s.list.scroll_to(ListOffset { item_ix: row, offset_in_item: px(into) });
                            cx.notify();
                        }),
                        Act::ScrollHome => spike.update(cx, |s, cx| {
                            s.list.scroll_to(ListOffset { item_ix: row - 1, offset_in_item: px(0.) });
                            cx.notify();
                        }),
                        Act::Away => move_to(cx, point(px(4.), px(4.))),
                    }
                }
                move_to(cx, point(px(4.), px(4.)));
                let ms = |d: Option<Duration>| d.map_or("-".into(), |d| format!("{:.0}ms", d.as_secs_f64() * 1e3));
                let hover = if hover_ms.is_empty() {
                    "-".into()
                } else {
                    hover_ms.iter().map(|m| format!("{m:.0}")).collect::<Vec<_>>().join("/") + "ms"
                };
                report.push(format!(
                    "{:<8} first frame {}  sized {}  hover→frame {}  sweep: {}{}",
                    demo.title(),
                    ms(first),
                    ms(sized),
                    hover,
                    sweep_stats,
                    if heights.is_empty() { String::new() } else { format!("  height: {}", heights.join(" → ")) },
                ));
            }
            self.status.set("autopilot: idle (3s)");
            wait(3000).await;
            let engine = spike.update(cx, |s, _| s.engine_name);
            for line in &report {
                println!("autopilot [{engine}] {line}");
            }
            let open = spike.update(cx, |s, cx| s.views.iter().filter(|v| v.read(cx).stats().frames > 0).count());
            let (app, helpers, n) = process_tree_rss();
            println!(
                "autopilot [{engine}] views open {open}  rss app {}  helpers {} in {n} processes  total {}",
                mb(app),
                mb(helpers),
                mb(app + helpers)
            );
            if self.tour {
                self.status.set("autopilot: memory");
                let footprint = |cx: &mut gpui::AsyncWindowContext| {
                    cx.background_executor().spawn(async { Footprint::now() })
                };
                let all = footprint(cx).await;
                println!("memory [{engine}] (a) all five opened: {all}");

                // Away from every view, past the release grace period.
                spike.update(cx, |s, cx| {
                    s.list.scroll_to(ListOffset { item_ix: ROWS - 1, offset_in_item: px(0.) });
                    cx.notify();
                });
                wait(AWAY_WAIT_MS).await;
                let away = footprint(cx).await;
                println!("memory [{engine}] (b) scrolled away {}s: {away}", AWAY_WAIT_MS / 1000);

                // Back to one: how long until it shows a new frame, and what
                // is on screen meanwhile.
                let i = 0;
                let row = demo_row(i);
                let before = spike.update(cx, |s, cx| s.views[i].read(cx).stats().frames);
                let t0 = Instant::now();
                spike.update(cx, |s, cx| {
                    s.active = i;
                    s.popover = false;
                    s.list.scroll_to(ListOffset { item_ix: row - 1, offset_in_item: px(0.) });
                    cx.notify();
                });
                let mut new_frame = None;
                let mut shots_due = vec![(0u64, "back-0ms"), (100, "back-100ms"), (250, "back-250ms")];
                while t0.elapsed() < Duration::from_secs(5) && (new_frame.is_none() || !shots_due.is_empty()) {
                    wait(2).await;
                    if let Some(&(at, name)) = shots_due.first() {
                        if t0.elapsed() >= Duration::from_millis(at + 20) {
                            shot(cx, &self.shots, name);
                            shots_due.remove(0);
                        }
                    }
                    if new_frame.is_none() && spike.update(cx, |s, cx| s.views[i].read(cx).stats().frames) > before {
                        new_frame = Some(t0.elapsed());
                        // The frame is taken; give it one paint.
                        wait(20).await;
                        shot(cx, &self.shots, "back-new-frame");
                    }
                }
                let height = spike.update(cx, |s, _| view_rect(&s.list, row).map(|(_, v)| f32::from(v.height)));
                wait(2000).await;
                let back = footprint(cx).await;
                println!(
                    "memory [{engine}] (c) back to {} (new frame after {}, height {}px): {back}",
                    Demo::ALL[i].title(),
                    new_frame.map_or("-".into(), |d| format!("{:.0}ms", d.as_secs_f64() * 1e3)),
                    height.map_or("?".into(), |h| h.to_string()),
                );
            }
            self.status.set("autopilot: done");
            if self.quit {
                let _ = cx.update(|_, cx| cx.quit());
            }
        })
        .detach();
    }
}

/// The web view's top-left in the window and its size, if `row` is laid out.
fn view_rect(list: &ListState, row: usize) -> Option<(Point<Pixels>, gpui::Size<Pixels>)> {
    let bounds = list.bounds_for_item(row)?;
    let origin = bounds.origin + point(px(ROW_PAD_X + INDENT + BORDER), px(ROW_PAD_Y + LEAD + BORDER));
    let size = size(
        bounds.size.width - px(2. * ROW_PAD_X + INDENT + 2. * BORDER),
        bounds.size.height - px(2. * ROW_PAD_Y + LEAD + 2. * BORDER),
    );
    Some((origin, size))
}

/// Save what the window last rendered (the real Metal renderer, drawn to a
/// texture instead of the screen) as `<dir>/<name>.png`.
fn shot(cx: &mut gpui::AsyncWindowContext, dir: &Option<std::path::PathBuf>, name: &str) {
    let Some(dir) = dir else { return };
    #[cfg(feature = "shots")]
    {
        let _ = std::fs::create_dir_all(dir);
        let path = dir.join(format!("{name}.png"));
        let result = cx.update(|window, _| window.render_to_image());
        match result.map_err(anyhow_string).and_then(|r| r.map_err(anyhow_string)) {
            Ok(image) => match image.save(&path) {
                Ok(()) => println!("shot {}", path.display()),
                Err(e) => eprintln!("shot {name}: {e}"),
            },
            Err(e) => eprintln!("shot {name}: {e}"),
        }
    }
    #[cfg(not(feature = "shots"))]
    {
        let _ = (cx, dir);
        eprintln!("shot {name}: build with --features shots");
    }
}

#[cfg(feature = "shots")]
fn anyhow_string(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn move_to(cx: &mut gpui::AsyncWindowContext, position: Point<Pixels>) {
    let _ = cx.update(|window, cx| {
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                pressed_button: None,
                modifiers: Modifiers::default(),
            }),
            cx,
        );
    });
}

fn click(cx: &mut gpui::AsyncWindowContext, position: Point<Pixels>) {
    let _ = cx.update(|window, cx| {
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
            }),
            cx,
        );
    });
}

fn rss_kb_now() -> u64 {
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// RSS of this process, and the summed RSS and count of its descendants (the
/// CEF helpers). RSS double-counts shared framework pages per process, so the
/// helper sum overstates; NOTES-cef.md has physical footprints.
fn process_tree_rss() -> (u64, u64, usize) {
    let me = std::process::id();
    let out = std::process::Command::new("ps")
        .args(["-A", "-o", "pid=,ppid=,rss="])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let procs: Vec<(u32, u32, u64)> = out
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace().map(|v| v.parse::<u64>().ok());
            Some((it.next()?? as u32, it.next()?? as u32, it.next()??))
        })
        .collect();
    let mut tree = vec![me];
    let (mut app, mut helpers, mut n) = (0, 0, 0);
    let mut k = 0;
    while k < tree.len() {
        let pid = tree[k];
        for &(child, parent, rss) in &procs {
            if child == pid {
                if pid == me {
                    app = rss;
                } else {
                    helpers += rss;
                    n += 1;
                }
            }
            if parent == pid && !tree.contains(&child) {
                tree.push(child);
            }
        }
        k += 1;
    }
    (app, helpers, n)
}

fn mb(kb: u64) -> String {
    if kb == 0 {
        "-".into()
    } else {
        format!("{:.0}MB", kb as f64 / 1024.)
    }
}

/// Physical footprint (what Activity Monitor calls "Memory") of this process
/// and every descendant (the CEF helpers), from `footprint(1)`.
struct Footprint {
    /// (name, pid, bytes), this process first.
    processes: Vec<(String, u32, u64)>,
}

impl Footprint {
    fn now() -> Self {
        let pids = process_tree();
        let out = std::process::Command::new("footprint")
            .args(["--noCategories", "-f", "bytes"])
            .args(pids.iter().map(|p| p.to_string()))
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        // "<name> [<pid>]: 64-bit    Footprint: <n> B (...)"
        let processes = out
            .lines()
            .filter_map(|l| {
                let (head, rest) = l.split_once("]: ")?;
                let (name, pid) = head.rsplit_once(" [")?;
                let bytes = rest.split("Footprint: ").nth(1)?.split_whitespace().next()?.parse().ok()?;
                Some((name.trim().to_owned(), pid.parse().ok()?, bytes))
            })
            .collect();
        Footprint { processes }
    }

    fn total(&self) -> u64 {
        self.processes.iter().map(|p| p.2).sum()
    }
}

impl std::fmt::Display for Footprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mb = |b: u64| b as f64 / (1024. * 1024.);
        write!(f, "total {:.0} MB in {} processes [", mb(self.total()), self.processes.len())?;
        for (k, (name, _, bytes)) in self.processes.iter().enumerate() {
            let short = name.strip_prefix("inline-cef Helper").map_or("app", |s| s.trim());
            let short = if short.is_empty() { "helper" } else { short };
            write!(f, "{}{short} {:.0}", if k > 0 { ", " } else { "" }, mb(*bytes))?;
        }
        write!(f, "]")
    }
}

/// This process and all its descendants.
fn process_tree() -> Vec<u32> {
    let out = std::process::Command::new("ps")
        .args(["-A", "-o", "pid=,ppid="])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let procs: Vec<(u32, u32)> = out
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace().map(|v| v.parse::<u32>().ok());
            Some((it.next()??, it.next()??))
        })
        .collect();
    let mut tree = vec![std::process::id()];
    let mut k = 0;
    while k < tree.len() {
        let pid = tree[k];
        for &(child, parent) in &procs {
            if parent == pid && !tree.contains(&child) {
                tree.push(child);
            }
        }
        k += 1;
    }
    tree
}

// ---------------------------------------------------------------------------
// Scroll benchmark

/// How long the tour and the bench wait, away from every view, for the engine
/// to release them (its grace period plus slack).
const AWAY_WAIT_MS: u64 = 4000;
/// A realistic trackpad fling, in logical px per second.
const FLING_SPEED: f32 = 4500.;

fn fling_speed() -> f32 {
    std::env::var("INLINE_CEF_FLING").ok().and_then(|v| v.parse().ok()).unwrap_or(FLING_SPEED)
}
const FLING_ROUNDS: usize = 3;
/// Flings up stop once this row is the list's top row (above every demo).
const FLING_TOP_ROW: usize = FIRST_DEMO_ROW - 6;

/// One fling, driven from `on_next_frame` so each display frame scrolls by
/// `velocity * (time since the previous frame)`.
struct Fling {
    spike: Entity<Spike>,
    /// Logical px per second; negative scrolls toward the top.
    velocity: f32,
    last: Option<Instant>,
    /// A row visible after the previous frame's layout, and its top then.
    anchor: Option<(usize, f32)>,
    /// How far the previous step scrolled.
    applied: f32,
    intervals: Vec<Duration>,
    /// Visible content that moved by something other than the scroll:
    /// (row, px off).
    jumps: Vec<(usize, f32)>,
    /// `--shots`: save a frame as each demo enters the viewport (untimed run).
    shots: Option<std::path::PathBuf>,
    shot: Vec<usize>,
    done: Option<futures::channel::oneshot::Sender<()>>,
}

impl Fling {
    fn finished(&self, list: &ListState) -> bool {
        if self.velocity < 0. {
            list.logical_scroll_top().item_ix <= FLING_TOP_ROW
        } else {
            at_bottom(list)
        }
    }

    fn step(this: Rc<RefCell<Fling>>, window: &mut Window, cx: &mut App) {
        let now = Instant::now();
        let mut f = this.borrow_mut();
        let list = f.spike.read(cx).list.clone();
        let at_end = at_bottom(&list);
        if let Some((row, top)) = f.anchor.take() {
            if let Some(bounds) = list.bounds_for_item(row) {
                let moved = f32::from(bounds.top()) - top;
                let off = moved + f.applied;
                if off.abs() > 1. && !at_end {
                    f.jumps.push((row, off));
                }
            }
        }
        if let Some(last) = f.last.replace(now) {
            f.intervals.push(now - last);
        }
        if let Some(dir) = f.shots.clone() {
            let viewport = list.viewport_bounds();
            for (i, _) in Demo::ALL.iter().enumerate() {
                let row = demo_row(i);
                if f.shot.contains(&row) {
                    continue;
                }
                // Entering at the top (flinging up) or bottom (down), 120 px in.
                let entering = list.bounds_for_item(row).is_some_and(|b| {
                    if f.velocity < 0. {
                        b.top() < viewport.top() && b.bottom() > viewport.top() + px(120.)
                    } else {
                        b.top() < viewport.bottom() - px(120.) && b.bottom() > viewport.bottom()
                    }
                });
                if entering {
                    f.shot.push(row);
                    #[cfg(feature = "shots")]
                    if let Ok(image) = window.render_to_image() {
                        let dir_name = if f.velocity < 0. { "up" } else { "down" };
                        let path = dir.join(format!("fling-{dir_name}-{}.png", Demo::ALL[i].title()));
                        let _ = std::fs::create_dir_all(&dir);
                        if image.save(&path).is_ok() {
                            println!("shot {}", path.display());
                        }
                    }
                    #[cfg(not(feature = "shots"))]
                    let _ = &dir;
                }
            }
        }
        if f.finished(&list) {
            if let Some(done) = f.done.take() {
                let _ = done.send(());
            }
            return;
        }
        let dt = f.intervals.last().map_or(1. / 120., |d| d.as_secs_f32());
        let delta = f.velocity * dt;
        let viewport = list.viewport_bounds();
        let top_ix = list.logical_scroll_top().item_ix;
        f.anchor = (top_ix..top_ix + 80).find_map(|ix| {
            list.bounds_for_item(ix).filter(|b| b.top() >= viewport.top()).map(|b| (ix, f32::from(b.top())))
        });
        let before = list.scroll_px_offset_for_scrollbar().y;
        list.scroll_by(px(delta));
        f.applied = f32::from(before - list.scroll_px_offset_for_scrollbar().y);
        let spike = f.spike.clone();
        drop(f);
        spike.update(cx, |_, cx| cx.notify());
        let next = this.clone();
        window.on_next_frame(move |window, cx| Fling::step(next, window, cx));
    }
}

/// The last row is fully on screen. (`ListState::is_scrolled_to_end` is
/// `None` while rows above were never measured.)
fn at_bottom(list: &ListState) -> bool {
    let viewport = list.viewport_bounds();
    list.bounds_for_item(ROWS - 1).is_some_and(|b| b.bottom() <= viewport.bottom() + px(1.))
}

/// Run one fling to the end; returns its frame intervals and jumps.
async fn fling(
    spike: &Entity<Spike>,
    velocity: f32,
    shots: Option<std::path::PathBuf>,
    cx: &mut gpui::AsyncWindowContext,
) -> (Vec<Duration>, Vec<(usize, f32)>) {
    let (tx, rx) = futures::channel::oneshot::channel();
    let state = Rc::new(RefCell::new(Fling {
        spike: spike.clone(),
        velocity,
        last: None,
        anchor: None,
        applied: 0.,
        intervals: Vec::new(),
        jumps: Vec::new(),
        shots,
        shot: Vec::new(),
        done: Some(tx),
    }));
    let step = state.clone();
    let _ = cx.update(|window, _| window.on_next_frame(move |window, cx| Fling::step(step, window, cx)));
    let _ = rx.await;
    let mut f = state.borrow_mut();
    (std::mem::take(&mut f.intervals), std::mem::take(&mut f.jumps))
}

fn scroll_bench(
    spike: Entity<Spike>,
    quit: bool,
    shots: Option<std::path::PathBuf>,
    status: Rc<Cell<&'static str>>,
    window: &mut Window,
    cx: &mut Context<Spike>,
) {
    cx.spawn_in(window, async move |_, cx| {
        let executor = cx.background_executor().clone();
        let wait = |ms| executor.timer(Duration::from_millis(ms));
        let engine = spike.update(cx, |s, _| s.engine_name);
        spike.update(cx, |s, cx| {
            s.popover = false;
            cx.notify();
        });

        // Open every view once (each row on screen until its first frame).
        // INLINE_CEF_BENCH_COLD=1 skips this: the first fling up then meets
        // views that were never opened (placeholder height until they load).
        let cold = std::env::var_os("INLINE_CEF_BENCH_COLD").is_some();
        status.set("bench: opening");
        for i in (0..Demo::ALL.len()).filter(|_| !cold) {
            spike.update(cx, |s, cx| {
                s.active = i;
                s.list.scroll_to(ListOffset { item_ix: demo_row(i) - 1, offset_in_item: px(0.) });
                cx.notify();
            });
            let t = Instant::now();
            while t.elapsed() < Duration::from_secs(3)
                && spike.update(cx, |s, cx| s.views[i].read(cx).stats().frames) == 0
            {
                wait(10).await;
            }
            wait(300).await;
        }
        let opened = cx.background_executor().spawn(async { Footprint::now() }).await;
        println!("bench [{engine}] {}: {opened}", if cold { "cold (nothing opened)" } else { "all opened" });

        // Park at the bottom, away from all of them.
        status.set("bench: parked");
        spike.update(cx, |s, cx| {
            s.list.scroll_to(ListOffset { item_ix: ROWS - 1, offset_in_item: px(0.) });
            cx.notify();
        });
        wait(AWAY_WAIT_MS).await;
        let parked = cx.background_executor().spawn(async { Footprint::now() }).await;
        println!("bench [{engine}] parked at the bottom {}s: {parked}", AWAY_WAIT_MS / 1000);

        status.set("bench: flinging");
        let _ = element::take_pump_stats();
        // Main-thread stalls, whatever the display link is doing: a 1 ms
        // foreground heartbeat and how late each beat runs.
        let stalls = Rc::new(RefCell::new(Vec::<Duration>::new()));
        let beating = Rc::new(Cell::new(true));
        {
            let (stalls, beating) = (stalls.clone(), beating.clone());
            let executor = cx.background_executor().clone();
            cx.spawn(async move |_| {
                while beating.get() {
                    let t = Instant::now();
                    executor.timer(Duration::from_millis(1)).await;
                    stalls.borrow_mut().push(t.elapsed().saturating_sub(Duration::from_millis(1)));
                }
            })
            .detach();
        }
        let (mut intervals, mut jumps) = (Vec::new(), Vec::new());
        for round in 0..FLING_ROUNDS {
            for velocity in [-fling_speed(), fling_speed()] {
                // Cold: the first fling up is the one that meets unopened views.
                let fling_shots = (cold && round == 0 && velocity < 0.).then(|| shots.clone()).flatten();
                let (i, j) = fling(&spike, velocity, fling_shots, cx).await;
                println!(
                    "bench [{engine}] round {round} {}: {}",
                    if velocity < 0. { "up  " } else { "down" },
                    FrameTimes::of(&i)
                );
                intervals.extend(i);
                jumps.extend(j);
            }
        }
        let pump = element::take_pump_stats();
        beating.set(false);
        let stalls = FrameTimes::of(&stalls.borrow());
        let after = cx.background_executor().spawn(async { Footprint::now() }).await;
        println!("bench [{engine}] all flings at {} px/s: {}", fling_speed(), FrameTimes::of(&intervals));
        println!("bench [{engine}] {pump}");
        println!(
            "bench [{engine}] main-thread stalls (1ms heartbeat lateness): p99 {:.1} max {:.1} ms, >8ms {}, >16.7ms {}",
            stalls.p99, stalls.max, stalls.over_8, stalls.over_16
        );
        println!(
            "bench [{engine}] height jumps of visible content: {}{}",
            jumps.len(),
            jumps.iter().take(8).map(|(r, d)| format!(" row {r} {d:+.0}px")).collect::<String>()
        );
        println!("bench [{engine}] after the flings: {after}");

        if shots.is_some() && !cold {
            // Untimed: rendering to an image costs frames.
            status.set("bench: shots");
            spike.update(cx, |s, cx| {
                s.list.scroll_to(ListOffset { item_ix: ROWS - 1, offset_in_item: px(0.) });
                cx.notify();
            });
            wait(AWAY_WAIT_MS).await;
            fling(&spike, -fling_speed(), shots.clone(), cx).await;
            fling(&spike, fling_speed(), shots.clone(), cx).await;
        }
        status.set("bench: done");
        if quit {
            let _ = cx.update(|_, cx| cx.quit());
        }
    })
    .detach();
}

/// Frame-interval percentiles.
struct FrameTimes {
    n: usize,
    p50: f64,
    p95: f64,
    p99: f64,
    max: f64,
    over_8: usize,
    over_16: usize,
    over_33: usize,
}

impl FrameTimes {
    fn of(intervals: &[Duration]) -> Self {
        let mut ms: Vec<f64> = intervals.iter().map(|d| d.as_secs_f64() * 1e3).collect();
        ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let at = |q: f64| if ms.is_empty() { 0. } else { ms[((ms.len() - 1) as f64 * q).round() as usize] };
        FrameTimes {
            n: ms.len(),
            p50: at(0.5),
            p95: at(0.95),
            p99: at(0.99),
            max: ms.last().copied().unwrap_or(0.),
            over_8: ms.iter().filter(|&&m| m > 8.3).count(),
            over_16: ms.iter().filter(|&&m| m > 16.7).count(),
            over_33: ms.iter().filter(|&&m| m > 33.3).count(),
        }
    }
}

impl std::fmt::Display for FrameTimes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} frames, p50 {:.1} p95 {:.1} p99 {:.1} max {:.1} ms, >16.7ms {}, >33ms {}",
            self.n, self.p50, self.p95, self.p99, self.max, self.over_16, self.over_33
        )
    }
}
