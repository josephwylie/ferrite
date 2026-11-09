//! Packaged real-engine feasibility fixture. The native controller driver runs
//! out of process while this thread pumps CEF and inspects the displayed views.
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Engine, Frame, Page, View, Viewport, BLUE, GREEN, RED};

struct Fixture(Arc<AtomicBool>);
impl Drop for Fixture {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

pub(super) fn run(engine: &dyn Engine) -> Result<(), String> {
    let driver = std::env::var_os("FERRITE_BROWSER_SMOKE_DRIVER")
        .ok_or("FERRITE_BROWSER_SMOKE_DRIVER must name the native controller probe")?;
    let listener =
        TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
    let address = listener.local_addr().map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let fixture = Fixture(Arc::new(AtomicBool::new(false)));
    let stop = fixture.0.clone();
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let Ok((mut stream, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
            let mut request = [0; 4096];
            let Ok(length) = stream.read(&mut request) else {
                continue;
            };
            let request = String::from_utf8_lossy(&request[..length]);
            let color = if request.starts_with("GET /one ") {
                "rgb(0, 0, 255)"
            } else {
                "rgb(0, 255, 0)"
            };
            let page = format!(
                r#"<!doctype html><html style="background:rgb(255,0,0)"><head><title>Ferrite browser fixture</title></head><body style="margin:0;background:rgb(255,0,0)"><button id="paint" onclick="document.documentElement.style.background='{color}';document.body.style.background='{color}';this.textContent='Painted'">Paint</button><input id="value"><script>window.fixtureReady=true</script></body></html>"#
            );
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
            let _ = stream.write_all(response.as_bytes());
        }
    });
    let viewport = Viewport {
        width: 400,
        scale: 1.0,
    };
    let one = engine.open_browser(&format!("http://{address}/one"), viewport, Box::new(|| {}))?;
    let two = engine.open_browser(&format!("http://{address}/two"), viewport, Box::new(|| {}))?;
    let sealed = engine.open(
        Page {
            html: "<style>body{margin:0;height:600px;background:rgb(255,0,0)}</style>sealed".into(),
            theme_css: String::new(),
        },
        viewport,
        Box::new(|| {}),
    );
    let mut frames = [None, None, None];
    let views = [one.as_ref(), two.as_ref(), sealed.as_ref()];
    wait_pixels(engine, &views, &mut frames, [RED, RED, RED])?;
    let endpoint = engine.controller()?;
    let python = if cfg!(windows) { "python" } else { "python3" };
    let native = std::env::var_os("FERRITE_BROWSER_CONTROLLER")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::current_exe().ok().and_then(|p| {
                p.parent().map(|p| {
                    p.join(if cfg!(windows) {
                        "agent-browser.exe"
                    } else {
                        "agent-browser"
                    })
                })
            })
        })
        .ok_or("Cannot locate packaged native browser controller")?;
    let mut child = Command::new(python)
        .arg(driver)
        .env("FERRITE_BROWSER_CONTROLLER", native)
        .env("FERRITE_CDP_URL", endpoint.url)
        .env("FERRITE_CDP_AUTHORIZATION", endpoint.authorization)
        .spawn()
        .map_err(|e| format!("Start native browser probe: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        collect(&views, &mut frames);
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) if status.success() => break,
            Some(status) => return Err(format!("Native controller probe exited {status}")),
            None => {}
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Native browser controller probe timed out".into());
        }
        std::thread::sleep(engine.pump().min(Duration::from_millis(5)));
    }
    wait_pixels(engine, &views, &mut frames, [BLUE, GREEN, RED])?;
    // The views are still alive here: the controller painted exactly the
    // displayed external pages, while the sealed page retained its own pixels.
    println!("PASS native controller: exact live view pixels blue/green, sealed visual unchanged");
    Ok(())
}

fn collect(views: &[&dyn View; 3], frames: &mut [Option<Frame>; 3]) {
    for (view, frame) in views.iter().zip(frames.iter_mut()) {
        if let Some(next) = view.take().frame {
            *frame = Some(next);
        }
    }
}

fn wait_pixels(
    engine: &dyn Engine,
    views: &[&dyn View; 3],
    frames: &mut [Option<Frame>; 3],
    colors: [[u8; 4]; 3],
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        collect(views, frames);
        if frames.iter().zip(colors).all(|(frame, color)| {
            frame.as_ref().is_some_and(|f| {
                let offset = ((100 * f.width + 100) * 4) as usize;
                f.bgra.get(offset..offset + 4) == Some(color.as_slice())
            })
        }) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Embedded view pixel agreement timed out; expected {colors:?}"
            ));
        }
        std::thread::sleep(engine.pump().min(Duration::from_millis(5)));
    }
}
