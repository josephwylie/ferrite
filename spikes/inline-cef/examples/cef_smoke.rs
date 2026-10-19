//! End-to-end check of the CEF engine without GPUI: boot, open a page with
//! known colours, pump until frames arrive, and check pixels, content height,
//! hover, cursor, theme changes and resizing. Exits 0 and prints PASS on
//! success.
//!
//! CEF needs its bundle, so run it through the bundle script:
//!
//! ```sh
//! ./bundle-macos.sh --example cef_smoke --run
//! ```
//!
//! `SMOKE_HOLD=<secs>` keeps the page open at the end (to measure RSS).

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use inline_cef::web::cef::{boot, Boot};
use inline_cef::web::{Cursor, Engine, Frame, Input, Modifiers, Page, View, Viewport};

const PAGE: &str = r#"
<style>
  body { margin: 0 }
  .a { height: 300px; background: rgb(255, 0, 0) }
  .a:hover { background: rgb(255, 0, 255) }
  .b { height: 200px; background: rgb(0, 0, 255); cursor: pointer }
  .c { height: 100px; background: var(--ferrite-accent) }
</style>
<div class="a"></div><div class="b"></div><div class="c"></div>
"#;

fn main() {
    let started = Instant::now();
    let engine = match boot() {
        Ok(Boot::Subprocess(code)) => std::process::exit(code),
        Ok(Boot::Engine(engine)) => engine,
        Err(error) => {
            eprintln!("boot failed: {error}");
            std::process::exit(2);
        }
    };
    println!("boot: {:?}", started.elapsed());

    let woken = Rc::new(Cell::new(0u32));
    let opened = Instant::now();
    let view = engine.open(
        Page { html: PAGE.into(), theme_css: ":root { --ferrite-accent: rgb(0, 255, 0) }".into() },
        Viewport { width: 400, scale: 2.0 },
        Box::new({
            let woken = woken.clone();
            move || woken.set(woken.get() + 1)
        }),
    );

    let mut probe = Probe { engine: &engine, view: view.as_ref(), frame: None, height: None, cursor: None };

    // First frame at the reported content height.
    let first = probe.until("first frame", |p| p.frame.is_some());
    println!("first frame: {:?} after open ({:?} since process start)", first, started.elapsed());
    probe.until("content height 600", |p| p.height == Some(600));
    probe.until("frame at 800x1200", |p| p.frame_size() == Some((800, 1200)));
    println!("sized frame: {:?} after open", opened.elapsed());
    probe.until("colours", |p| {
        p.pixel(10, 10) == Some(RED) && p.pixel(10, 350) == Some(BLUE) && p.pixel(10, 550) == Some(GREEN)
    });
    println!("pixels ok: red/blue/green at y=10/350/550; wakes so far: {}", woken.get());

    // Hover: the first block turns magenta, and back on leave.
    let hover = Instant::now();
    view.input(Input::Move { x: 10.0, y: 10.0, modifiers: Modifiers::default() });
    probe.until("hover colour", |p| p.pixel(10, 10) == Some(MAGENTA));
    println!("hover → pixel: {:?}", hover.elapsed());
    view.input(Input::Leave);
    probe.until("leave colour", |p| p.pixel(10, 10) == Some(RED));

    // Cursor over the second block.
    view.input(Input::Move { x: 10.0, y: 350.0, modifiers: Modifiers::default() });
    probe.until("pointer cursor", |p| p.cursor == Some(Cursor::Pointer));
    view.input(Input::Leave);

    // Theme swap without reload.
    view.set_theme(":root { --ferrite-accent: rgb(255, 255, 0) }");
    probe.until("theme colour", |p| p.pixel(10, 550) == Some(YELLOW));

    // Resize: narrower view, same content height.
    view.resize(Viewport { width: 300, scale: 2.0 });
    probe.until("frame at 600x1200", |p| p.frame_size() == Some((600, 1200)));

    // Idle: nothing changes, so no frames should arrive.
    probe.frame = None;
    let mut idle_frames = 0;
    let idle_until = Instant::now() + Duration::from_secs(1);
    while Instant::now() < idle_until {
        std::thread::sleep(engine.pump());
        if probe.view.take().frame.is_some() {
            idle_frames += 1;
        }
    }
    println!("frames during 1s idle: {idle_frames}");

    if let Some(hold) = std::env::var("SMOKE_HOLD").ok().and_then(|s| s.parse::<u64>().ok()) {
        println!("holding {hold}s, pid {}", std::process::id());
        let until = Instant::now() + Duration::from_secs(hold);
        while Instant::now() < until {
            std::thread::sleep(engine.pump());
            let _ = view.take();
        }
    }

    drop(view);
    drop(engine);
    println!("PASS in {:?}", started.elapsed());
}

/// BGRA bytes.
const RED: [u8; 4] = [0, 0, 255, 255];
const MAGENTA: [u8; 4] = [255, 0, 255, 255];
const BLUE: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const YELLOW: [u8; 4] = [0, 255, 255, 255];

struct Probe<'a> {
    engine: &'a dyn Engine,
    view: &'a dyn View,
    frame: Option<Frame>,
    height: Option<u32>,
    cursor: Option<Cursor>,
}

impl Probe<'_> {
    /// Pump until `done`, folding updates in; abort after 10s.
    fn until(&mut self, what: &str, done: impl Fn(&Self) -> bool) -> Duration {
        let start = Instant::now();
        loop {
            let updates = self.view.take();
            if let Some(frame) = updates.frame {
                self.frame = Some(frame);
            }
            if let Some(height) = updates.content_height {
                self.height = Some(height);
            }
            if let Some(cursor) = updates.cursor {
                self.cursor = Some(cursor);
            }
            if done(self) {
                return start.elapsed();
            }
            if start.elapsed() > Duration::from_secs(10) {
                eprintln!(
                    "FAIL: timed out waiting for {what}; frame {:?}, height {:?}, cursor {:?}, \
                     pixels {:?} {:?} {:?}",
                    self.frame_size(),
                    self.height,
                    self.cursor,
                    self.pixel(10, 10),
                    self.pixel(10, 350),
                    self.pixel(10, 550),
                );
                std::process::exit(1);
            }
            std::thread::sleep(self.engine.pump().min(Duration::from_millis(5)));
        }
    }

    fn frame_size(&self) -> Option<(u32, u32)> {
        self.frame.as_ref().map(|f| (f.width, f.height))
    }

    /// The pixel at logical (`x`, `y`), at scale 2.
    fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let frame = self.frame.as_ref()?;
        let (x, y) = (x * 2, y * 2);
        if x >= frame.width || y >= frame.height {
            return None;
        }
        let i = ((y * frame.width + x) * 4) as usize;
        frame.bgra.get(i..i + 4).map(|p| [p[0], p[1], p[2], p[3]])
    }
}
