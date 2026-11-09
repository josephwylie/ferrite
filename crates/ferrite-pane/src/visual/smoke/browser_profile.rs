//! Real HTTP cookie oracle across tabs, processes and packaged restarts.
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::web::BrowserViewport;
use super::{Engine, Frame, Page, View, Viewport, BLUE, GREEN, RED};

struct Fixture {
    stop: Arc<AtomicBool>,
    authenticated: Arc<AtomicUsize>,
    last_request: Arc<Mutex<String>>,
    address: std::net::SocketAddr,
}

impl Fixture {
    fn start() -> Result<Self, String> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let fixture = Self {
            address: listener.local_addr().map_err(|e| e.to_string())?,
            stop: Arc::new(AtomicBool::new(false)),
            authenticated: Arc::new(AtomicUsize::new(0)),
            last_request: Arc::new(Mutex::new(String::new())),
        };
        let stop = fixture.stop.clone();
        let authenticated = fixture.authenticated.clone();
        let last_request = fixture.last_request.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let mut bytes = Vec::new();
                loop {
                    let mut part = [0; 2048];
                    match stream.read(&mut part) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => bytes.extend_from_slice(&part[..n]),
                    }
                    if bytes.windows(4).any(|s| s == b"\r\n\r\n") || bytes.len() > 16384 {
                        break;
                    }
                }
                let request = String::from_utf8_lossy(&bytes);
                *last_request.lock().unwrap() = request
                    .lines()
                    .next()
                    .unwrap_or("<empty request>")
                    .to_owned();
                let login = request.starts_with("GET /login ");
                let check = request.starts_with("GET /check ");
                let cookie =
                    request
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .any(|(name, value)| {
                            name.eq_ignore_ascii_case("cookie")
                                && value
                                    .split(';')
                                    .any(|cookie| cookie.trim() == "ferrite_profile=authenticated")
                        });
                if check && cookie {
                    authenticated.fetch_add(1, Ordering::SeqCst);
                }
                let color = if login {
                    "0,0,255"
                } else if check && cookie {
                    "0,255,0"
                } else {
                    "255,0,0"
                };
                let page = format!("<!doctype html><title>Profile oracle</title><style>html,body{{margin:0;background:rgb({color});height:100%}}</style>");
                let header = if login {
                    "Set-Cookie: ferrite_profile=authenticated; Path=/; Max-Age=3600; HttpOnly; SameSite=Lax\r\n"
                } else {
                    ""
                };
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nCache-Control: no-store\r\n{header}Content-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Ok(fixture)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub(super) fn run(engine: &dyn Engine) -> Result<(), String> {
    if std::env::var_os("FERRITE_BROWSER_PROFILE").is_none() {
        return Err("Profile smoke requires an explicit isolated FERRITE_BROWSER_PROFILE".into());
    }
    let stage = std::env::var("FERRITE_BROWSER_PROFILE_SMOKE").map_err(|e| e.to_string())?;
    let viewport = BrowserViewport {
        width: 400,
        height: 300,
        scale: 1.0,
    };
    if stage == "contender" {
        let error = engine
            .browser_available()
            .err()
            .ok_or("Competing process acquired website profile")?;
        if !error.to_lowercase().contains("restart") || !error.to_lowercase().contains("close") {
            return Err(format!("Profile conflict is not actionable: {error}"));
        }
        if engine
            .open_browser("http://127.0.0.1:9/", viewport, Box::new(|| {}))
            .is_ok()
            || engine.controller().is_ok()
        {
            return Err("Competing process exposed external browsing/controller".into());
        }
        let sealed = engine.open(
            Page {
                html:
                    "<style>html,body{margin:0;height:600px;background:rgb(255,0,0)}</style>sealed"
                        .into(),
                theme_css: String::new(),
            },
            Viewport {
                width: 400,
                scale: 1.0,
            },
            Box::new(|| {}),
        );
        wait_color(engine, sealed.as_ref(), RED)?;
        println!("PASS profile contender: external browsing unavailable, sealed pixels remain red");
        return Ok(());
    }
    if stage != "write" && stage != "read" {
        return Err(format!("Unknown profile smoke stage: {stage}"));
    }
    engine.browser_available()?;
    let fixture = Fixture::start()?;
    let base = format!("http://{}", fixture.address);
    let one = engine.open_browser(
        &format!(
            "{base}/{}",
            if stage == "write" { "login" } else { "check" }
        ),
        viewport,
        Box::new(|| {}),
    )?;
    wait_color(
        engine,
        one.as_ref(),
        if stage == "write" { BLUE } else { GREEN },
    )
    .map_err(|error| {
        format!(
            "{error}; native={:?}; last HTTP request={:?}",
            one.state(),
            fixture.last_request.lock().unwrap()
        )
    })?;
    let two = engine.open_browser(&format!("{base}/check"), viewport, Box::new(|| {}))?;
    wait_color(engine, two.as_ref(), GREEN)?;
    wait_color(
        engine,
        one.as_ref(),
        if stage == "write" { BLUE } else { GREEN },
    )?;
    let expected = if stage == "write" { 1 } else { 2 };
    if fixture.authenticated.load(Ordering::SeqCst) < expected {
        return Err("Rendered login was not confirmed by fixture HTTP cookie oracle".into());
    }
    if stage == "write" {
        let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--visual-smoke")
            .env("FERRITE_BROWSER_PROFILE_SMOKE", "contender")
            .spawn()
            .map_err(|e| format!("Start profile contender: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                if !status.success() {
                    return Err(format!("Profile contender exited {status}"));
                }
                break;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Profile contender timed out".into());
            }
            std::thread::sleep(engine.pump().min(Duration::from_millis(5)));
        }
    }
    println!("PASS profile {stage}: two independent live tabs share authenticated HTTP cookies");
    Ok(())
}

fn wait_color(engine: &dyn Engine, view: &dyn View, color: [u8; 4]) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut frame: Option<Frame> = None;
    loop {
        frame = view.take().frame.or(frame);
        if frame.as_ref().is_some_and(|frame| {
            let offset = ((100 * frame.width + 100) * 4) as usize;
            frame.bgra.get(offset..offset + 4) == Some(color.as_slice())
        }) {
            return Ok(());
        }
        if Instant::now() > deadline {
            let observed = frame.as_ref().map(|frame| {
                let offset = ((100 * frame.width + 100) * 4) as usize;
                (
                    frame.width,
                    frame.height,
                    frame.bgra.get(offset..offset + 4),
                )
            });
            return Err(format!(
                "Timed out waiting for profile pixels {color:?}; observed={observed:?}"
            ));
        }
        std::thread::sleep(engine.pump().min(Duration::from_millis(5)));
    }
}
