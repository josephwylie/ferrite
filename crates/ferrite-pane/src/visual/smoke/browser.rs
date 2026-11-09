//! Packaged real-engine feasibility fixture. The native controller driver runs
//! out of process while this thread pumps CEF and inspects the displayed views.
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Engine, Frame, Page, View, Viewport, BLUE, GREEN, RED};
use crate::visual::web::BrowserViewport;

struct ProbeFile(std::path::PathBuf);
impl ProbeFile {
    fn create() -> Result<Self, String> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "ferrite-private-browser-probe-{}-{nonce}.html",
            std::process::id()
        ));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let probe = Self(path);
        file.write_all(b"<!doctype html><title>Private filesystem sentinel</title>FERRITE_PRIVATE_FILE_MUST_NOT_BE_READ").map_err(|e| e.to_string())?;
        Ok(probe)
    }
}
impl Drop for ProbeFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

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
    let edited = Arc::new(AtomicBool::new(false));
    let observed_edit = edited.clone();
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
            if request.starts_with("GET /dirty?value=x ") {
                observed_edit.store(true, Ordering::SeqCst);
            }
            let color = if request.starts_with("GET /one ") {
                "rgb(0, 0, 255)"
            } else {
                "rgb(0, 255, 0)"
            };
            let page = if request.starts_with("GET /clean ") {
                "<!doctype html><title>Clean lifecycle fixture</title><style>html,body{margin:0;background:rgb(255,0,0);height:100%}</style>".to_owned()
            } else {
                format!(
                    r#"<!doctype html><html style="background:rgb(255,0,0)"><head><title>Ferrite browser fixture</title></head><body style="margin:0;background:rgb(255,0,0)"><button id="paint" onclick="document.documentElement.style.background='{color}';document.body.style.background='{color}';this.textContent='Painted'">Paint</button><input id="value" oninput="fetch('/dirty?value='+encodeURIComponent(this.value))"><script>window.fixtureReady=true</script></body></html>"#
                )
            };
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
            let _ = stream.write_all(response.as_bytes());
        }
    });
    let viewport = BrowserViewport {
        width: 400,
        height: 600,
        scale: 1.0,
    };
    let one = engine.open_browser(&format!("http://{address}/one"), viewport, Box::new(|| {}))?;
    let two = engine.open_browser(&format!("http://{address}/two"), viewport, Box::new(|| {}))?;
    let sealed = engine.open(
        Page {
            html: "<style>body{margin:0;height:600px;background:rgb(255,0,0)}</style>sealed".into(),
            theme_css: String::new(),
        },
        Viewport {
            width: viewport.width,
            scale: viewport.scale,
        },
        Box::new(|| {}),
    );
    let mut frames = [None, None, None];
    let views: [&dyn View; 3] = [one.as_ref(), two.as_ref(), sealed.as_ref()];
    wait_pixels(engine, &views, &mut frames, [RED, RED, RED])?;
    one.resize_browser(BrowserViewport {
        width: 320,
        height: 240,
        scale: 2.0,
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if one
            .take()
            .frame
            .is_some_and(|frame| frame.width == 640 && frame.height == 480)
        {
            break;
        }
        if Instant::now() >= deadline {
            return Err("Browser viewport resize did not produce 640x480 pixels".into());
        }
        std::thread::sleep(engine.pump().min(Duration::from_millis(5)));
    }
    if one.navigate("file:///private/forbidden").is_ok() {
        return Err("Browser accepted forbidden navigation".into());
    }
    one.resize_browser(viewport);
    let initial = one.state().document_generation;
    let next = format!("http://{address}/next");
    one.navigate(&next)?;
    wait_browser(engine, one.as_ref(), |state| {
        !state.loading && state.url == next && state.can_back && state.document_generation > initial
    })?;
    one.back()?;
    wait_browser(engine, one.as_ref(), |state| {
        !state.loading && state.url.ends_with("/one") && state.can_forward
    })?;
    one.forward()?;
    wait_browser(engine, one.as_ref(), |state| {
        !state.loading && state.url == next
    })?;
    let before_reload = one.state().document_generation;
    one.reload()?;
    wait_browser(engine, one.as_ref(), |state| {
        !state.loading && state.document_generation > before_reload
    })?;
    one.navigate(&format!("http://{address}/one"))?;
    wait_browser(engine, one.as_ref(), |state| {
        !state.loading && state.url.ends_with("/one")
    })?;
    let clean = engine.open_browser(
        &format!("http://{address}/clean"),
        viewport,
        Box::new(|| {}),
    )?;
    wait_browser(engine, clean.as_ref(), |state| {
        !state.loading && state.dirty == crate::visual::web::BrowserDirty::Clean
    })?;
    let old_target = clean.target_id().ok_or("Clean page has no live target")?;
    let old_generation = clean.state().document_generation;
    clean.set_visible(false);
    clean.set_protected(true);
    if clean.try_release()? {
        return Err("Protected page was reclaimed".into());
    }
    clean.set_protected(false);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if clean.try_release()? {
            break;
        }
        if Instant::now() >= deadline {
            return Err("Clean inactive page was not reclaimed".into());
        }
        std::thread::sleep(engine.pump().min(Duration::from_millis(5)));
    }
    if clean.target_id().is_some() {
        return Err("Reclaimed page still has a live target".into());
    }
    clean.set_visible(true);
    wait_browser(engine, clean.as_ref(), |state| {
        !state.loading && state.document_generation > old_generation
    })?;
    if clean.target_id().as_deref() == Some(&old_target) {
        return Err("Recreated page retained its retired target".into());
    }
    drop(clean);
    let endpoint = engine.controller()?;
    let private_file = ProbeFile::create()?;
    let private_file_url = url::Url::from_file_path(&private_file.0)
        .map_err(|_| "Cannot represent private fixture file as a URL")?;
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
        .env("FERRITE_BROWSER_PRIVATE_FILE", private_file_url.as_str())
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
    one.input(crate::visual::web::Input::Down {
        x: 100.0,
        y: 10.0,
        button: crate::visual::web::MouseButton::Left,
        clicks: 1,
        modifiers: Default::default(),
    });
    one.input(crate::visual::web::Input::Up {
        x: 100.0,
        y: 10.0,
        button: crate::visual::web::MouseButton::Left,
        clicks: 1,
        modifiers: Default::default(),
    });
    one.input(crate::visual::web::Input::Key {
        key: "x".into(),
        text: Some("x".into()),
        down: true,
        modifiers: Default::default(),
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while !edited.load(Ordering::SeqCst) {
        if Instant::now() >= deadline {
            return Err("Native form input was not confirmed by fixture HTTP oracle".into());
        }
        std::thread::sleep(engine.pump().min(Duration::from_millis(5)));
    }
    wait_browser(engine, one.as_ref(), |state| {
        state.dirty == crate::visual::web::BrowserDirty::Dirty
    })?;
    one.set_visible(false);
    one.set_protected(false);
    if one.try_release()? || one.target_id().is_none() {
        return Err("Unfinished form was discarded".into());
    }
    one.set_visible(true);
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

fn wait_browser(
    engine: &dyn Engine,
    view: &dyn crate::visual::web::BrowserView,
    ready: impl Fn(&crate::visual::web::BrowserState) -> bool,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let state = view.state();
        if ready(&state) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("Browser navigation timed out: {state:?}"));
        }
        std::thread::sleep(engine.pump().min(Duration::from_millis(5)));
    }
}
