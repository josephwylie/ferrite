//! `ferrite --visual-smoke`: proof that Chromium runs where it ships, without
//! GPUI or a window. CI runs it on Windows (`.github/workflows/ci.yml`); on
//! macOS run the bundled binary
//! (`Ferrite.app/Contents/MacOS/ferrite --visual-smoke`).
//!
//! It starts the engine [`boot`](super::boot) readied, opens a page with known
//! colours at 400 px wide and scale 2, and pumps the way the app does. Then it
//! checks, all through the `Engine`/`View` seam: the page's content height,
//! the first frame's size and pixels, a hover changing pixels (and back on
//! leave), the cursor, a theme swap without reload, a resize, and that an
//! idle page sends (next to) no frames. Exits 0 and prints PASS, or prints
//! what it was waiting for and exits 1.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::web::{Cursor, Engine, Frame, Input, Modifiers, Page, View, Viewport};

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

/// BGRA bytes.
const RED: [u8; 4] = [0, 0, 255, 255];
const MAGENTA: [u8; 4] = [255, 0, 255, 255];
const BLUE: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const YELLOW: [u8; 4] = [0, 255, 255, 255];

const SCALE: u32 = 2;
/// Generous: a first start on a cold CI machine (a 290 MB DLL scanned by
/// the antivirus, no GPU) is far slower than the ~0.2 s it takes locally.
const PATIENCE: Duration = Duration::from_secs(60);

/// Run the smoke; the process's exit code.
pub fn run() -> i32 {
    match smoke() {
        Ok(elapsed) => {
            println!("PASS in {elapsed:?}");
            0
        }
        Err(error) => {
            eprintln!("FAIL: {error}");
            1
        }
    }
}

fn smoke() -> Result<Duration, String> {
    let started = Instant::now();
    let engine = super::engine::start_now()?;
    println!("engine started: {:?}", started.elapsed());

    let wakes = Rc::new(Cell::new(0u32));
    let opened = Instant::now();
    let view = engine.open(
        Page {
            html: PAGE.into(),
            theme_css: ":root { --ferrite-accent: rgb(0, 255, 0) }".into(),
        },
        Viewport {
            width: 400,
            scale: SCALE as f32,
        },
        Box::new({
            let wakes = wakes.clone();
            move || wakes.set(wakes.get() + 1)
        }),
    );
    let mut probe = Probe {
        engine: engine.as_ref(),
        view: view.as_ref(),
        frame: None,
        height: None,
        cursor: None,
    };

    probe.until("content height 600", |p| p.height == Some(600))?;
    probe.until("a frame at 800x1200", |p| {
        p.frame_size() == Some((800, 1200))
    })?;
    println!("laid-out frame: {:?} after open", opened.elapsed());
    probe.until("red/blue/green at y=10/350/550", |p| {
        p.pixel(10, 10) == Some(RED)
            && p.pixel(10, 350) == Some(BLUE)
            && p.pixel(10, 550) == Some(GREEN)
    })?;
    if wakes.get() == 0 {
        return Err("frames arrived but the view never woke its host".into());
    }

    let hover = Instant::now();
    view.input(Input::Move {
        x: 10.0,
        y: 10.0,
        modifiers: Modifiers::default(),
    });
    probe.until("hover turning the first block magenta", |p| {
        p.pixel(10, 10) == Some(MAGENTA)
    })?;
    println!("hover -> pixel: {:?}", hover.elapsed());
    view.input(Input::Leave);
    probe.until("leave turning it red again", |p| {
        p.pixel(10, 10) == Some(RED)
    })?;

    view.input(Input::Move {
        x: 10.0,
        y: 350.0,
        modifiers: Modifiers::default(),
    });
    probe.until("a pointer cursor over the second block", |p| {
        p.cursor == Some(Cursor::Pointer)
    })?;
    view.input(Input::Leave);

    view.set_theme(":root { --ferrite-accent: rgb(255, 255, 0) }");
    probe.until("the new theme's yellow", |p| {
        p.pixel(10, 550) == Some(YELLOW)
    })?;

    view.resize(Viewport {
        width: 300,
        scale: SCALE as f32,
    });
    probe.until("a frame at 600x1200 after the resize", |p| {
        p.frame_size() == Some((600, 1200))
    })?;

    let mut idle_frames = 0;
    let idle_until = Instant::now() + Duration::from_secs(1);
    while Instant::now() < idle_until {
        std::thread::sleep(engine.pump());
        idle_frames += view.take().frame.is_some() as u32;
    }
    println!("frames during 1 s idle: {idle_frames}");
    // A trailing frame after the resize is fine; a page repainting all the
    // time (60 a second) is not.
    if idle_frames > 3 {
        return Err(format!("an idle page sent {idle_frames} frames in 1 s"));
    }

    drop(view);
    drop(engine);
    Ok(started.elapsed())
}

struct Probe<'a> {
    engine: &'a dyn Engine,
    view: &'a dyn View,
    frame: Option<Frame>,
    height: Option<u32>,
    cursor: Option<Cursor>,
}

impl Probe<'_> {
    /// Pump until `done`, folding the view's updates in.
    fn until(&mut self, what: &str, done: impl Fn(&Self) -> bool) -> Result<(), String> {
        let start = Instant::now();
        loop {
            let updates = self.view.take();
            self.frame = updates.frame.or(self.frame.take());
            self.height = updates.content_height.or(self.height);
            self.cursor = updates.cursor.or(self.cursor);
            if done(self) {
                return Ok(());
            }
            if start.elapsed() > PATIENCE {
                return Err(format!(
                    "timed out waiting for {what}; frame {:?}, height {:?}, cursor {:?}, pixels {:?} {:?} {:?}",
                    self.frame_size(),
                    self.height,
                    self.cursor,
                    self.pixel(10, 10),
                    self.pixel(10, 350),
                    self.pixel(10, 550),
                ));
            }
            std::thread::sleep(self.engine.pump().min(Duration::from_millis(5)));
        }
    }

    fn frame_size(&self) -> Option<(u32, u32)> {
        self.frame.as_ref().map(|f| (f.width, f.height))
    }

    /// The pixel at logical (`x`, `y`).
    fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let frame = self.frame.as_ref()?;
        let (x, y) = (x * SCALE, y * SCALE);
        if x >= frame.width || y >= frame.height {
            return None;
        }
        let i = ((y * frame.width + x) * 4) as usize;
        frame.bgra.get(i..i + 4).map(|p| [p[0], p[1], p[2], p[3]])
    }
}
