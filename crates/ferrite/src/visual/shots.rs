//! Captures (`--features visual-reference`): the headless capture has no
//! platform frame loop and its executor's clock only moves when told, while
//! Chromium renders in its own processes on the wall clock. [`settle`] pumps
//! the engine and draws until the pages on screen have arrived.

use std::time::{Duration, Instant};

use gpui::{AnyWindowHandle, HeadlessAppContext};

use super::engine::{Availability, Visuals};

/// Longest to wait for Chromium to start and the pages to draw.
const DEADLINE: Duration = Duration::from_secs(12);
/// Once something has drawn, this long without a new frame is settled.
const QUIET: Duration = Duration::from_millis(1500);

/// Pump the engine and draw `window` until its visuals have settled.
pub(crate) fn settle(cx: &mut HeadlessAppContext, window: AnyWindowHandle) {
    let started = Instant::now();
    let mut last_change = Instant::now();
    let mut seen = 0u64;
    loop {
        let engine = match cx.update(Visuals::engine) {
            Availability::Ready(engine) => Some(engine),
            Availability::Starting => None,
            Availability::Unavailable(_) => return,
        };
        if let Some(engine) = &engine {
            engine.pump();
        }
        cx.run_until_parked();
        cx.advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
        let _ = cx.update_window(window, |_, window, cx| {
            window.simulate_next_frame(cx);
            let _ = window.draw(cx);
        });
        let frames = super::element::frames_taken();
        if frames != seen {
            seen = frames;
            last_change = Instant::now();
        }
        let elapsed = started.elapsed();
        if elapsed >= DEADLINE || (seen > 0 && last_change.elapsed() >= QUIET) {
            if std::env::var_os("FERRITE_CEF_TRACE").is_some() {
                eprintln!("ferrite visuals: settled after {elapsed:?}, {seen} frames");
            }
            return;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}

/// `--visual-snapshots <dir>`: the agent's screenshots (`renderer::snapshot`)
/// of the capture pages, in both schemes, as `<page>-<scheme>.png`. Proves
/// the render path the providers' `show_visual` answers go through.
pub(crate) fn snapshots(output: String) {
    use super::renderer;
    use super::tokens::Scheme;
    let output = std::path::PathBuf::from(output);
    std::fs::create_dir_all(&output).expect("create artifact directory");
    let platform = gpui::platform::current_platform(true);
    let mut cx = HeadlessAppContext::with_platform(
        platform.text_system(),
        std::sync::Arc::new(crate::icons::Assets),
        gpui::platform::current_headless_renderer,
    );
    cx.update(super::init);
    let pages = [
        ("stats", include_str!("../cockpit/visual_reference/visuals/stats.html")),
        ("mockups", include_str!("../cockpit/visual_reference/visuals/mockups.html")),
        ("treemap", include_str!("../cockpit/visual_reference/visuals/treemap.html")),
    ];
    let started = Instant::now();
    let done = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let mut count = 0;
    for (name, html) in pages {
        for (scheme, word) in [(Scheme::Dark, "dark"), (Scheme::Light, "light")] {
            let path = output.join(format!("{name}-{word}.png"));
            let html = html.to_string();
            let done = done.clone();
            count += 1;
            cx.update(|cx| {
                cx.spawn(async move |cx| {
                    let engine = loop {
                        match cx.update(Visuals::engine) {
                            Availability::Ready(engine) => break Ok(engine),
                            Availability::Unavailable(why) => break Err(why.to_string()),
                            Availability::Starting => {
                                cx.background_executor().timer(Duration::from_millis(16)).await
                            }
                        }
                    };
                    let result = match engine {
                        Ok(engine) => renderer::snapshot(engine, html, 760, 1.0, scheme, cx).await,
                        Err(why) => Err(why),
                    };
                    match result {
                        Ok(shot) => {
                            std::fs::write(&path, &shot.png).expect("write snapshot");
                            eprintln!("{}: {}x{} ({} bytes)", path.display(), shot.width, shot.height, shot.png.len());
                        }
                        Err(error) => eprintln!("{}: {error}", path.display()),
                    }
                    done.set(done.get() + 1);
                })
                .detach()
            });
        }
    }
    // The executor's clock moves only when told; keep it with the wall clock
    // while Chromium renders.
    while done.get() < count && started.elapsed() < Duration::from_secs(60) {
        if let Availability::Ready(engine) = cx.update(Visuals::engine) {
            engine.pump();
        }
        cx.advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(16));
    }
}
