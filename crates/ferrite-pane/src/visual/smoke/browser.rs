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
    let selected = Arc::new(AtomicBool::new(false));
    let observed_selection = selected.clone();
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let Ok((mut stream, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            };
            let _ = stream.set_nonblocking(false);
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
            if request.starts_with("GET /selected?value=two ") {
                observed_selection.store(true, Ordering::SeqCst);
            }
            let color = if request.starts_with("GET /one ") {
                "rgb(0, 0, 255)"
            } else {
                "rgb(0, 255, 0)"
            };
            let page = if request.starts_with("GET /select-popup ") {
                // The control and menu extend beyond the right viewport edge:
                // the popup compositor must clip/shift it and remap its inputs.
                r#"<!doctype html><title>Select popup fixture</title><style>html,body{margin:0;background:rgb(255,0,0);height:100%;overflow:hidden}select{position:absolute;left:260px;top:185px;width:220px;height:36px;font:20px sans-serif;background:rgb(255,0,0);color:white}option{font:20px sans-serif;color:white}option[value=one]{background:rgb(110,20,20)}option[value=two]{background:rgb(13,71,151)}option[value=three]{background:rgb(20,110,20)}</style><select onchange="fetch('/selected?value='+this.value);if(this.value==='two'){document.documentElement.style.background='rgb(0,255,0)';document.body.style.background='rgb(0,255,0)'}"><option value=one>First option</option><option value=two>Second option</option><option value=three>Third option</option></select>"#.to_owned()
            } else if request.starts_with("GET /clean ") {
                "<!doctype html><title>Clean lifecycle fixture</title><style>html,body{margin:0;background:rgb(255,0,0);height:100%}</style>".to_owned()
            } else if request.starts_with("GET /script-form ") {
                "<!doctype html><title>Draft</title><input id=draft><script>document.querySelector('input').value='unsaved work'</script>".to_owned()
            } else if request.starts_with("GET /uncertain ") {
                "<!doctype html><title>Closed shadow</title><body><script>document.body.attachShadow({mode:'closed'}).innerHTML='<input value=unfinished>';console.log('ferrite-clean')</script>".to_owned()
            } else {
                format!(
                    r#"<!doctype html><html style="background:rgb(255,0,0)"><head><title>Ferrite browser fixture</title></head><body style="margin:0;background:rgb(255,0,0)"><button id="paint" onclick="document.documentElement.style.background='{color}';document.body.style.background='{color}';this.textContent='Painted'">Paint</button><input id="value" oninput="fetch('/dirty?value='+encodeURIComponent(this.value))"><script>window.fixtureReady=true</script></body></html>"#
                )
            };
            let status = if request.starts_with("GET /error ") {
                "503 Service Unavailable"
            } else {
                "200 OK"
            };
            let response = format!("HTTP/1.1 {status}\r\nContent-Type: text/html\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
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
    for url in [
        "file:///private/forbidden",
        "https://:443/",
        "http://127.0.0.1\n/",
        " http://127.0.0.1/",
        "http://user:password@127.0.0.1/",
    ] {
        if one.navigate(url).is_ok() {
            return Err(format!("Browser accepted forbidden navigation: {url:?}"));
        }
    }
    one.resize_browser(viewport);
    let initial = one.state().document_generation;
    let next = format!("http://{address}/next");
    one.navigate(&next)?;
    if !one.state().loading || one.state().dirty != crate::visual::web::BrowserDirty::Unknown {
        return Err("Accepted navigation retained pre-command clean/loading state".into());
    }
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
    let same_document = one.state().document_generation;
    let fragment = format!("{next}#section");
    one.navigate(&fragment)?;
    wait_browser(engine, one.as_ref(), |state| {
        !state.loading && state.url == fragment
    })?;
    if one.state().document_generation != same_document {
        return Err("Fragment navigation replaced the document identity".into());
    }
    one.back()?;
    wait_browser(engine, one.as_ref(), |state| {
        !state.loading && state.url == next && state.can_forward
    })?;
    one.forward()?;
    wait_browser(engine, one.as_ref(), |state| {
        !state.loading && state.url == fragment
    })?;
    one.navigate(&format!("http://{address}/error"))?;
    wait_browser(engine, one.as_ref(), |state| {
        !state.loading
            && state
                .error
                .as_ref()
                .is_some_and(|error| error.contains("503"))
    })?;
    one.navigate(&format!("http://{address}/one"))?;
    wait_browser(engine, one.as_ref(), |state| {
        !state.loading && state.url.ends_with("/one")
    })?;
    select_popup(engine, &format!("http://{address}/select-popup"), &selected)?;
    let clean = engine.open_browser(
        &format!("http://{address}/clean"),
        viewport,
        Box::new(|| {}),
    )?;
    clean.set_protected(true);
    let deadline = Instant::now() + Duration::from_secs(30);
    while clean.target_id().is_none() {
        if Instant::now() >= deadline {
            return Err("Protected first-use tab did not become live".into());
        }
        std::thread::sleep(engine.pump().min(Duration::from_millis(5)));
    }
    clean.set_protected(false);
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
    if clean.state().dirty != crate::visual::web::BrowserDirty::Unknown || clean.try_release()? {
        return Err("Reclamation reused clean evidence from before a protected operation".into());
    }
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
    let draft = engine.open_browser(
        &format!("http://{address}/script-form"),
        viewport,
        Box::new(|| {}),
    )?;
    wait_browser(engine, draft.as_ref(), |state| {
        !state.loading && state.dirty == crate::visual::web::BrowserDirty::Dirty
    })?;
    draft.set_visible(false);
    if draft.try_release()? || draft.target_id().is_none() {
        return Err("Changed form defaults were discarded".into());
    }
    drop(draft);
    let uncertain = engine.open_browser(
        &format!("http://{address}/uncertain"),
        viewport,
        Box::new(|| {}),
    )?;
    wait_browser(engine, uncertain.as_ref(), |state| {
        !state.loading && state.document_generation > 0
    })?;
    uncertain.set_visible(false);
    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until {
        if uncertain.try_release()? || uncertain.target_id().is_none() {
            return Err("Uncertain closed-shadow work was discarded".into());
        }
        std::thread::sleep(engine.pump().min(Duration::from_millis(5)));
    }
    drop(uncertain);
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

/// Exercises the operator's real OSR popup, independently of native Select
/// (which changes a value without ever painting or clicking the menu).
fn select_popup(engine: &dyn Engine, url: &str, selected: &AtomicBool) -> Result<(), String> {
    let view = engine.open_browser(
        url,
        BrowserViewport {
            width: 400,
            height: 240,
            scale: 2.0,
        },
        Box::new(|| {}),
    )?;
    wait_browser(engine, view.as_ref(), |state| {
        !state.loading && state.document_generation > 0
    })?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if view.take().frame.is_some_and(|frame| {
            frame.width == 800
                && frame.height == 480
                && frame.bgra.get(0..4) == Some(RED.as_slice())
        }) {
            break;
        }
        if Instant::now() >= deadline {
            return Err("Select fixture did not paint its base frame".into());
        }
        std::thread::sleep(engine.pump().min(Duration::from_millis(5)));
    }
    pointer_click(view.as_ref(), 280.0, 200.0);
    let deadline = Instant::now() + Duration::from_secs(10);
    let (x, y) = loop {
        if let Some(frame) = view.take().frame {
            // Exact fixture option color is absent from the closed select.
            // A wide run proves the popup pixels reached the displayed frame;
            // native controller selection or base-only paint cannot pass.
            let mut candidates = Vec::new();
            for y in 0..frame.height {
                let mut longest = (0u32, 0u32);
                let mut start = 0u32;
                for x in 0..=frame.width {
                    let offset = ((y * frame.width + x.min(frame.width - 1)) * 4) as usize;
                    let target = x < frame.width
                        && frame.bgra.get(offset..offset + 4)
                            == Some([151, 71, 13, 255].as_slice());
                    if !target {
                        if x - start > longest.1 {
                            longest = (start, x - start);
                        }
                        start = x + 1;
                    }
                }
                if longest.1 >= 120 {
                    candidates.push(((longest.0 + longest.1 / 2) as f32 / 2.0, y as f32 / 2.0));
                }
            }
            if let Some(point) = candidates.get(candidates.len() / 2) {
                break *point;
            }
        }
        if Instant::now() >= deadline {
            return Err("HTML select popup option was not composited into the live frame".into());
        }
        std::thread::sleep(engine.pump().min(Duration::from_millis(5)));
    };
    pointer_click(view.as_ref(), x, y);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut green = false;
    loop {
        if let Some(frame) = view.take().frame {
            green = frame.bgra.get(0..4) == Some(GREEN.as_slice());
        }
        if selected.load(Ordering::SeqCst) && green {
            break;
        }
        if Instant::now() >= deadline {
            return Err("Pointer selection of the painted HTML option did not confirm value=two and green base pixels".into());
        }
        std::thread::sleep(engine.pump().min(Duration::from_millis(5)));
    }
    println!("PASS operator select popup: composited option pixels, pointer-selected value=two at scale 2");
    Ok(())
}

fn pointer_click(view: &dyn View, x: f32, y: f32) {
    use crate::visual::web::{Input, MouseButton};
    view.input(Input::Move {
        x,
        y,
        modifiers: Default::default(),
    });
    view.input(Input::Down {
        x,
        y,
        button: MouseButton::Left,
        clicks: 1,
        modifiers: Default::default(),
    });
    view.input(Input::Up {
        x,
        y,
        button: MouseButton::Left,
        clicks: 1,
        modifiers: Default::default(),
    });
}
