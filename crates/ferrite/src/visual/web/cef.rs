//! The real engine: Chromium (CEF, via tauri-apps/cef-rs) rendering off-screen.
//!
//! Everything CEF-specific stays in here; the host only sees [`Engine`] and
//! [`View`]. In short:
//!
//! - **Processes.** CEF is multi-process and on macOS spawns its renderer, GPU
//!   and utility processes from `<App> Helper*.app` bundles inside the main
//!   bundle. This binary is its own helper: [`boot`] detects a `--type=`
//!   switch and runs `cef_execute_process` instead of the app
//!   ([`Boot::Subprocess`]). `scripts/install-app.sh` builds the `.app`.
//! - **NSApp.** `cef_initialize` requires `NSApp` to implement
//!   `CefAppProtocol`; [`boot`] retrofits that onto GPUI's `GPUIApplication`
//!   class before GPUI starts (see `nsapp`).
//! - **Message loop.** GPUI owns the run loop, so CEF runs with
//!   `external_message_pump`: the host calls [`Engine::pump`], which runs
//!   `cef_do_message_loop_work` and returns the delay CEF last asked for.
//! - **Pages** are served from memory under the custom `ferrite-view:` scheme;
//!   every other request, navigation, popup, context menu and JS dialog is
//!   refused. See `handlers` and `document`.
//! - **Frames** come from `OnPaint` (CPU BGRA, device pixels). Each view keeps
//!   only the newest untaken [`Updates`]; the host is woken from `pump()`,
//!   never from inside a CEF callback. After a (re)load no frame is passed on
//!   until the page has reported its height, so the first one the host sees is
//!   the laid-out page, never a blank or half-built one.
//! - **Memory.** A view hidden for [`RELEASE_AFTER`] has its browser closed
//!   (its page leaves the renderer); the view keeps its page, theme, geometry
//!   and content height, and loads the page again when shown. All pages share
//!   one site, and `process-per-site` puts them in one renderer process.
//! - **Main-thread budget.** Browsers are created from `pump()`, at most one
//!   per call, so a scroll that brings several views back staggers them.

mod document;
mod handlers;
mod input;
mod nsapp;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use cef::{
    args::Args, browser_host_create_browser_sync, do_message_loop_work, execute_process,
    initialize, Browser, BrowserSettings, CefString, ImplBrowser, ImplBrowserHost, ImplFrame,
    PaintElementType, RuntimeStyle, Settings, WindowInfo,
};

use super::{Cursor, Engine, Input, Page, Updates, View, Viewport, Wake};

/// The custom scheme pages are served from (`ferrite-view://page/<n>`).
const SCHEME: &str = "ferrite-view";
/// The view's height until the page reports one (an `<iframe>`'s default).
const INITIAL_HEIGHT: u32 = 150;
/// Tallest view we render, in logical pixels. Bounds texture size and memory.
const MAX_HEIGHT: u32 = 8000;
/// Longest `pump()` will ask the host to wait while pages are live. CEF's own
/// cefclient caps its external-pump timer at 1000/30 ms the same way.
const MAX_PUMP_DELAY: Duration = Duration::from_millis(33);
/// With no browsers alive there is nothing to pump for but housekeeping.
const IDLE_PUMP_DELAY: Duration = Duration::from_millis(250);
/// Longest a hide waits for a restyle's paint (see `ViewInner::hide_deferred`).
const HIDE_DEFER_MAX: Duration = Duration::from_millis(250);
/// Least time between two loads of one view's page while its HTML is being
/// rewritten (an agent streaming it): a load costs the main thread a browser
/// create and the renderer a full layout.
const MIN_RELOAD_INTERVAL: Duration = Duration::from_millis(250);
/// A load that hasn't reported a height in this long is given up on when new
/// HTML arrives.
const STUCK_LOAD: Duration = Duration::from_secs(2);
/// How long a view stays hidden before its browser is closed. Long enough
/// that scrolling past and back, or a quick look elsewhere, costs nothing.
pub const RELEASE_AFTER: Duration = Duration::from_secs(2);

/// What [`boot`] found this process to be.
pub enum Boot {
    /// A CEF helper process ran to completion: exit with this code, now.
    Subprocess(i32),
    /// The browser process, inside a bundle that ships Chromium: the
    /// framework is loaded and `NSApp` made CEF-compatible, but nothing is
    /// running yet. [`Dormant::start`] starts Chromium when the first page
    /// is wanted.
    Dormant(Dormant),
}

/// Chromium, ready to start. Holding it costs nothing but the mapped
/// framework: no CEF process exists until [`Dormant::start`].
pub struct Dormant {
    global: Arc<Global>,
    library: cef::library_loader::LibraryLoader,
}

/// Check how this process was started. Call it first thing in `main`, before
/// anything else runs (a helper must not spawn shells or open stores) and
/// before GPUI is created (and anything touches `NSApp`). In a helper process
/// this runs the helper and returns [`Boot::Subprocess`]; `main` must exit
/// with that code. `Err` means Chromium isn't available here (not running
/// from a bundle that ships it); the app runs without visuals.
pub fn boot() -> Result<Boot, String> {
    use cef::library_loader::LibraryLoader;

    let is_subprocess = std::env::args().any(|arg| arg.starts_with("--type="));
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let exe_dir = exe.parent().ok_or("executable has no parent directory")?;

    // CEF's loader aborts the process if the framework is missing, so check
    // first: `cargo run` outside the bundle should get an error, not a crash.
    let frameworks = if is_subprocess { exe_dir.join("../../..") } else { exe_dir.join("../Frameworks") };
    let framework = frameworks.join("Chromium Embedded Framework.framework");
    if !framework.join("Chromium Embedded Framework").exists() {
        return Err(format!(
            "Chromium Embedded Framework not found at {} (visuals need the app bundle \
             scripts/install-app.sh builds)",
            framework.display()
        ));
    }

    let global = Arc::new(Global::default());
    global.trace.store(std::env::var_os("FERRITE_CEF_TRACE").is_some(), Ordering::SeqCst);

    if is_subprocess {
        let args = Args::new();
        // cef-dll-sys is built with the sandbox on: a helper must enter it
        // before loading the framework, or the GPU/network children die with
        // exit code 5. Skipped when the browser was started with --no-sandbox.
        let sandbox = (!std::env::args().any(|a| a == "--no-sandbox")).then(|| {
            let mut sandbox = cef::sandbox::Sandbox::new();
            sandbox.initialize(args.as_main_args());
            sandbox
        });
        let loader = LibraryLoader::new(&exe, true);
        if !loader.load() {
            return Err("failed to load the CEF framework (helper)".into());
        }
        let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
        let mut app = handlers::InlineApp::new(global);
        let code = execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());
        drop(app);
        drop(loader);
        drop(sandbox);
        return Ok(Boot::Subprocess(code));
    }

    let library = LibraryLoader::new(&exe, false);
    if !library.load() {
        return Err("failed to load the CEF framework".into());
    }
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);

    // Before GPUI's first `+sharedApplication`: NSApp must be GPUI's class
    // with CEF's protocol retrofitted, whenever Chromium starts later.
    nsapp::adopt_cef_app_protocol()?;

    Ok(Boot::Dormant(Dormant { global, library }))
}

impl Dormant {
    /// Start Chromium (`cef_initialize`): its GPU and utility processes
    /// spawn, and pages can open. Takes ~0.1 s of main thread. Call it on the
    /// main thread, outside any GPUI update (CEF may run the native loop).
    pub fn start(self) -> Result<CefEngine, String> {
        let Dormant { global, library } = self;
        let args = Args::new();
        let mut app = handlers::InlineApp::new(global.clone());
        let code = execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());
        if code >= 0 {
            return Err(format!("cef_execute_process ran as a helper ({code}) in the browser process"));
        }

        let profile = profile_dir();
        let settings = Settings {
            // FERRITE_CEF_NO_SANDBOX=1 is the escape hatch if helpers won't start.
            no_sandbox: std::env::var_os("FERRITE_CEF_NO_SANDBOX").is_some() as i32,
            windowless_rendering_enabled: 1,
            // GPUI owns the run loop; CEF is driven from Engine::pump.
            multi_threaded_message_loop: 0,
            external_message_pump: 1,
            // Throwaway profile: nothing persists between runs.
            root_cache_path: path_string(&profile),
            cache_path: path_string(&profile.join("default")),
            persist_session_cookies: 0,
            log_severity: cef::LogSeverity::WARNING,
            ..Default::default()
        };
        if initialize(Some(args.as_main_args()), Some(&settings), Some(&mut app), std::ptr::null_mut()) != 1 {
            return Err("cef_initialize failed".into());
        }

        Ok(CefEngine {
            inner: Rc::new(EngineInner {
                global,
                views: RefCell::new(Vec::new()),
                next_id: Cell::new(1),
                pumping: Cell::new(false),
                profile,
                _app: app,
                _library: library,
            }),
        })
    }
}

/// A fresh per-process profile under the temp dir; leftovers from processes
/// that are gone are removed.
fn profile_dir() -> PathBuf {
    let base = std::env::temp_dir();
    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|n| n.strip_prefix("ferrite-cef-profile-")) else {
                continue;
            };
            if pid.parse::<i32>().map(|pid| !process_alive(pid)).unwrap_or(false) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let dir = base.join(format!("ferrite-cef-profile-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir.canonicalize().unwrap_or(dir)
}

fn process_alive(pid: i32) -> bool {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    // SAFETY: signal 0 only checks that the process exists.
    unsafe { kill(pid, 0) == 0 }
}

fn path_string(path: &Path) -> CefString {
    CefString::from(path.to_string_lossy().as_ref())
}

// ---------------------------------------------------------------------------
// State shared with the CEF callbacks (which may run off the main thread).

/// Process-wide CEF state.
#[derive(Default)]
struct Global {
    /// `OnContextInitialized` has run: browsers can be created.
    context_ready: AtomicBool,
    /// When CEF next wants `do_message_loop_work`, in ms since [`epoch`];
    /// `u64::MAX` when it hasn't asked.
    due_ms: AtomicU64Max,
    /// Browsers created and not yet closed.
    live_browsers: AtomicUsize,
    /// `FERRITE_CEF_TRACE`: log browser creates and releases with timings.
    trace: AtomicBool,
}

struct AtomicU64Max(AtomicU64);

impl Default for AtomicU64Max {
    fn default() -> Self {
        Self(AtomicU64::new(u64::MAX))
    }
}

impl Global {
    fn schedule(&self, delay_ms: i64) {
        let due = now_ms() + delay_ms.max(0) as u64;
        self.due_ms.0.fetch_min(due, Ordering::SeqCst);
    }
}

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

fn now_ms() -> u64 {
    epoch().elapsed().as_millis() as u64
}

/// The document a view serves, at its fixed URL. The page is kept (not the
/// rendered bytes) so a theme change before load or a reload uses the latest.
struct PageDoc {
    url: String,
    page: Mutex<Page>,
}

impl PageDoc {
    fn document(&self) -> String {
        document::assemble(&self.page.lock().unwrap())
    }
}

#[derive(Clone, Copy, Debug)]
struct Geometry {
    /// Logical pixels (CEF's DIPs).
    width: u32,
    height: u32,
    scale: f32,
}

/// One view's state as the callbacks see it. It outlives the view's browsers:
/// a released view keeps it, and the next browser picks up where it left off.
struct ViewState {
    geometry: Mutex<Geometry>,
    updates: Mutex<Updates>,
    last_cursor: Mutex<Cursor>,
    /// Updates are waiting and the host hasn't been woken for them yet.
    dirty: AtomicBool,
    /// Which browser is current. Each browser's handlers carry the generation
    /// they were made for and ignore themselves once it has moved on, so a
    /// closing browser can't deliver into its successor.
    generation: AtomicU64,
    /// The current browser's page has reported its height: frames from now on
    /// show the laid-out page. Until then they are dropped.
    loaded: AtomicBool,
    /// The host's `set_visible`. A hidden OSR browser still paints now and
    /// then (a restyle, a resize) but not reliably the latest state, so its
    /// frames are dropped: the host's frozen frame stays the last one it saw
    /// while visible, and showing the view asks for a fresh paint.
    visible: AtomicBool,
    /// A restyle was sent to the page and no frame has been delivered since.
    restyle_unpainted: AtomicBool,
    trace: bool,
}

impl ViewState {
    fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == generation
    }

    fn geometry(&self) -> Geometry {
        *self.geometry.lock().unwrap()
    }

    fn update(&self, f: impl FnOnce(&mut Updates)) {
        f(&mut self.updates.lock().unwrap());
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Record the page's height; true if the view must be resized for it.
    fn set_content_height(&self, height: u32) -> bool {
        let changed = {
            let mut geometry = self.geometry.lock().unwrap();
            let changed = geometry.height != height;
            geometry.height = height;
            changed
        };
        self.update(|updates| updates.content_height = Some(height));
        changed
    }

    fn set_cursor(&self, cursor: Cursor) {
        let mut last = self.last_cursor.lock().unwrap();
        if *last != cursor {
            *last = cursor;
            self.update(|updates| updates.cursor = Some(cursor));
        }
    }
}

// ---------------------------------------------------------------------------
// The engine.

/// The CEF engine. Main thread only; one per process.
pub struct CefEngine {
    inner: Rc<EngineInner>,
}

struct EngineInner {
    global: Arc<Global>,
    views: RefCell<Vec<Weak<ViewInner>>>,
    next_id: Cell<u64>,
    pumping: Cell<bool>,
    profile: PathBuf,
    _app: cef::App,
    _library: cef::library_loader::LibraryLoader,
}

impl Engine for CefEngine {
    fn open(&self, page: Page, viewport: Viewport, wake: Wake) -> Box<dyn View> {
        let engine = &self.inner;
        let id = engine.next_id.get();
        engine.next_id.set(id + 1);

        let doc = Arc::new(PageDoc { url: format!("{SCHEME}://page/{id}"), page: Mutex::new(page) });
        let state = Arc::new(ViewState {
            geometry: Mutex::new(Geometry {
                width: viewport.width.max(1),
                height: INITIAL_HEIGHT,
                scale: viewport.scale.max(0.5),
            }),
            updates: Mutex::new(Updates::default()),
            last_cursor: Mutex::new(Cursor::Arrow),
            dirty: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            loaded: AtomicBool::new(false),
            visible: AtomicBool::new(true),
            restyle_unpainted: AtomicBool::new(false),
            trace: engine.global.trace.load(Ordering::SeqCst),
        });
        let view = Rc::new(ViewInner {
            _engine: self.inner.clone(),
            global: engine.global.clone(),
            state,
            doc,
            browser: RefCell::new(None),
            wake,
            visible: Cell::new(true),
            hidden_since: Cell::new(None),
            released: Cell::new(false),
            hide_deferred: Cell::new(false),
            focused: Cell::new(false),
            pressed: Cell::new(0),
            pointer: Cell::new((0.0, 0.0)),
            html_pending: Cell::new(false),
            created_at: Cell::new(None),
        });
        engine.views.borrow_mut().push(Rc::downgrade(&view));
        // The browser is created by the next pump (see `pump`).
        Box::new(CefView(view))
    }

    fn pump(&self) -> Duration {
        let engine = &self.inner;
        // cef_do_message_loop_work must not be re-entered.
        if engine.pumping.replace(true) {
            return Duration::from_millis(1);
        }
        let started = Instant::now();
        let views = engine.live_views();
        for view in &views {
            view.finish_deferred_hide();
            view.release_if_hidden_long_enough();
            view.retire_for_new_html();
        }
        // At most one browser per pump, and nothing else in that pump:
        // creating one costs the main thread a few ms (more under load), and
        // a scroll can bring several views back at once. CEF's own work runs
        // on the next pump, 1 ms later, so the two never add up in one go.
        let ready = engine.global.context_ready.load(Ordering::SeqCst);
        if let (true, Some(view)) = (ready, views.iter().find(|view| view.wants_browser())) {
            view.create_browser();
            engine.pumping.set(false);
            return Duration::from_millis(1);
        }
        engine.global.due_ms.0.store(u64::MAX, Ordering::SeqCst);
        let before_work = Instant::now();
        do_message_loop_work();
        if engine.global.trace.load(Ordering::SeqCst) && started.elapsed() > Duration::from_millis(4) {
            eprintln!(
                "ferrite visuals: pump {:.1}ms (cef work {:.1}ms)",
                started.elapsed().as_secs_f64() * 1e3,
                before_work.elapsed().as_secs_f64() * 1e3
            );
        }
        engine.pumping.set(false);

        // Wake hosts outside CEF's callbacks, after the work is done.
        for view in &views {
            if view.state.dirty.swap(false, Ordering::SeqCst) {
                (view.wake)();
            }
        }

        let due = engine.global.due_ms.0.load(Ordering::SeqCst);
        let idle = views.is_empty()
            && engine.global.live_browsers.load(Ordering::SeqCst) == 0
            && engine.global.context_ready.load(Ordering::SeqCst);
        if due == u64::MAX {
            return if idle { IDLE_PUMP_DELAY } else { MAX_PUMP_DELAY };
        }
        Duration::from_millis(due.saturating_sub(now_ms())).clamp(Duration::from_millis(1), MAX_PUMP_DELAY)
    }
}

impl EngineInner {
    fn live_views(&self) -> Vec<Rc<ViewInner>> {
        let mut views = self.views.borrow_mut();
        views.retain(|view| view.strong_count() > 0);
        views.iter().filter_map(Weak::upgrade).collect()
    }
}

impl Drop for EngineInner {
    /// Runs once the engine and every view are gone (views keep the engine
    /// alive). Their browsers were asked to close on drop; CEF must have
    /// finished closing them before `cef_shutdown`, so pump until it has
    /// (bounded).
    fn drop(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.global.live_browsers.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
            do_message_loop_work();
            std::thread::sleep(Duration::from_millis(5));
        }
        cef::shutdown();
        let _ = std::fs::remove_dir_all(&self.profile);
    }
}

// ---------------------------------------------------------------------------
// Views.

struct CefView(Rc<ViewInner>);

struct ViewInner {
    /// Keeps CEF up while any view lives, whatever order the host drops in.
    _engine: Rc<EngineInner>,
    global: Arc<Global>,
    state: Arc<ViewState>,
    doc: Arc<PageDoc>,
    /// None until the first pump after opening (CEF's context may not be
    /// initialised yet when the host opens), and again once released.
    browser: RefCell<Option<Browser>>,
    wake: Wake,
    visible: Cell<bool>,
    /// When the view was last hidden, while it stays hidden.
    hidden_since: Cell<Option<Instant>>,
    /// The browser was closed for being hidden; showing the view reloads.
    released: Cell<bool>,
    /// Hidden by the host while a restyle was still unpainted. A hidden
    /// browser never paints it, so the host's frozen frame would keep the old
    /// theme; the browser stays shown until that paint arrives (at most
    /// [`HIDE_DEFER_MAX`]), then `pump` hides it.
    hide_deferred: Cell<bool>,
    focused: Cell<bool>,
    /// Held mouse buttons, as CEF event flags.
    pressed: Cell<u32>,
    /// Last pointer position, for the leave event.
    pointer: Cell<(f32, f32)>,
    /// `set_html` gave the view a document its browser hasn't loaded.
    html_pending: Cell<bool>,
    /// When the current browser was created.
    created_at: Cell<Option<Instant>>,
}

impl ViewInner {
    /// Visible and without a browser: never created yet, or released.
    fn wants_browser(&self) -> bool {
        self.visible.get() && self.browser.borrow().is_none()
    }

    /// Load the page in a fresh browser. The view's geometry (including the
    /// last content height) carries over, so a reload starts at the size the
    /// host is already showing.
    fn create_browser(&self) {
        let started = Instant::now();
        let generation = self.state.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.state.loaded.store(false, Ordering::SeqCst);
        // The reloaded page has the latest theme built in.
        self.state.restyle_unpainted.store(false, Ordering::SeqCst);
        let mut client = handlers::client(self.state.clone(), self.doc.clone(), self.global.clone(), generation);
        let window_info = WindowInfo {
            windowless_rendering_enabled: 1,
            // CPU OnPaint path: GPUI's surface() can't take BGRA IOSurfaces.
            shared_texture_enabled: 0,
            external_begin_frame_enabled: 0,
            runtime_style: RuntimeStyle::ALLOY,
            ..Default::default()
        };
        let settings = BrowserSettings {
            windowless_frame_rate: 60,
            // Transparent: whatever the page doesn't paint shows the host.
            background_color: 0,
            ..Default::default()
        };
        let browser = browser_host_create_browser_sync(
            Some(&window_info),
            Some(&mut client),
            Some(&self.doc.url.as_str().into()),
            Some(&settings),
            None,
            None,
        );
        if browser.is_none() {
            eprintln!("ferrite visuals: cef_browser_host_create_browser_sync returned null");
        }
        *self.browser.borrow_mut() = browser;
        // The browser loads the document as it is now.
        self.html_pending.set(false);
        self.created_at.set(Some(Instant::now()));
        if self.global.trace.load(Ordering::SeqCst) {
            eprintln!(
                "ferrite visuals: {} {} in {:.1}ms",
                if self.released.get() { "reopened" } else { "opened" },
                self.doc.url,
                started.elapsed().as_secs_f64() * 1e3
            );
        }
        self.released.set(false);
    }

    fn finish_deferred_hide(&self) {
        let waited = self.hidden_since.get().is_some_and(|t| t.elapsed() >= HIDE_DEFER_MAX);
        if self.hide_deferred.get() && (waited || !self.state.restyle_unpainted.load(Ordering::SeqCst)) {
            self.hide_now();
        }
    }

    fn hide_now(&self) {
        self.hide_deferred.set(false);
        self.state.visible.store(false, Ordering::SeqCst);
        if let Some(host) = self.host() {
            host.was_hidden(1);
        }
    }

    /// Close the browser of a view that has been hidden for [`RELEASE_AFTER`].
    /// Everything the host gave the view stays; in-page state goes.
    fn release_if_hidden_long_enough(&self) {
        let due = self.hidden_since.get().is_some_and(|t| t.elapsed() >= RELEASE_AFTER);
        if !due || self.browser.borrow().is_none() {
            return;
        }
        let started = Instant::now();
        // Retire the generation first: anything the closing browser still
        // sends is ignored.
        self.state.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(host) = self.browser.borrow_mut().take().and_then(|browser| browser.host()) {
            host.close_browser(1);
        }
        self.released.set(true);
        self.focused.set(false);
        self.pressed.set(0);
        if self.global.trace.load(Ordering::SeqCst) {
            eprintln!(
                "ferrite visuals: released {} in {:.1}ms",
                self.doc.url,
                started.elapsed().as_secs_f64() * 1e3
            );
        }
    }

    /// Close the browser of a visible view whose HTML changed, once its
    /// current page is laid out (so reloads never pile up) and not more often
    /// than [`MIN_RELOAD_INTERVAL`]; the next pump creates one that loads the
    /// new document. Until that is laid out nothing reaches the host, which
    /// keeps its last frame.
    fn retire_for_new_html(&self) {
        let age = self.created_at.get().map(|t| t.elapsed());
        // A page that never reports its height (a script stuck in a loop)
        // still gives way to new HTML, after a while.
        let settled = self.state.loaded.load(Ordering::SeqCst) || age.is_some_and(|age| age >= STUCK_LOAD);
        let due = self.html_pending.get()
            && self.visible.get()
            && settled
            && age.is_none_or(|age| age >= MIN_RELOAD_INTERVAL);
        if !due || self.browser.borrow().is_none() {
            return;
        }
        self.state.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(host) = self.browser.borrow_mut().take().and_then(|browser| browser.host()) {
            host.close_browser(1);
        }
        self.focused.set(false);
        self.pressed.set(0);
    }

    fn host(&self) -> Option<cef::BrowserHost> {
        self.browser.borrow().as_ref().and_then(|browser| browser.host())
    }
}

impl Drop for ViewInner {
    fn drop(&mut self) {
        if let Some(host) = self.host() {
            host.close_browser(1);
        }
    }
}

impl View for CefView {
    fn resize(&self, viewport: Viewport) {
        let view = &self.0;
        let (width_changed, scale_changed) = {
            let mut geometry = view.state.geometry.lock().unwrap();
            let width = viewport.width.max(1);
            let scale = viewport.scale.max(0.5);
            let changed = (geometry.width != width, geometry.scale != scale);
            geometry.width = width;
            geometry.scale = scale;
            changed
        };
        let Some(host) = view.host() else { return };
        if scale_changed {
            host.notify_screen_info_changed();
        }
        if width_changed || scale_changed {
            host.was_resized();
        }
    }

    fn input(&self, event: Input) {
        let view = &self.0;
        let Some(host) = view.host() else { return };
        match event {
            Input::Move { x, y, modifiers } => {
                view.pointer.set((x, y));
                let flags = input::modifier_flags(modifiers) | view.pressed.get();
                host.send_mouse_move_event(Some(&input::mouse_event(x, y, flags)), 0);
            }
            Input::Leave => {
                let (x, y) = view.pointer.get();
                host.send_mouse_move_event(Some(&input::mouse_event(x, y, view.pressed.get())), 1);
            }
            Input::Down { x, y, button, clicks, modifiers } => {
                if !view.focused.replace(true) {
                    // Keys only reach a focused browser. There is no blur in
                    // the seam, so focus stays once given.
                    host.set_focus(1);
                }
                view.pressed.set(view.pressed.get() | input::button_flag(button));
                let flags = input::modifier_flags(modifiers) | view.pressed.get();
                let event = input::mouse_event(x, y, flags);
                host.send_mouse_click_event(Some(&event), input::button_type(button), 0, clicks.max(1) as i32);
            }
            Input::Up { x, y, button, clicks, modifiers } => {
                view.pressed.set(view.pressed.get() & !input::button_flag(button));
                let flags = input::modifier_flags(modifiers) | view.pressed.get();
                let event = input::mouse_event(x, y, flags);
                host.send_mouse_click_event(Some(&event), input::button_type(button), 1, clicks.max(1) as i32);
            }
            Input::Wheel { x, y, dx, dy, modifiers } => {
                // Same sign convention as GPUI's ScrollDelta (positive = toward
                // the top/left), in logical pixels.
                let flags = input::modifier_flags(modifiers) | view.pressed.get();
                host.send_mouse_wheel_event(
                    Some(&input::mouse_event(x, y, flags)),
                    dx.round() as i32,
                    dy.round() as i32,
                );
            }
            Input::Key { key, text, down, modifiers } => {
                for event in input::key_events(&key, text.as_deref(), down, modifiers) {
                    host.send_key_event(Some(&event));
                }
            }
        }
    }

    fn set_theme(&self, theme_css: &str) {
        let view = &self.0;
        view.doc.page.lock().unwrap().theme_css = theme_css.to_owned();
        let frame = view.browser.borrow().as_ref().and_then(|browser| browser.main_frame());
        if let Some(frame) = frame {
            view.state.restyle_unpainted.store(true, Ordering::SeqCst);
            let script = document::theme_script(theme_css);
            frame.execute_java_script(Some(&script.as_str().into()), Some(&view.doc.url.as_str().into()), 0);
        }
    }

    fn set_html(&self, html: &str) {
        let view = &self.0;
        {
            let mut page = view.doc.page.lock().unwrap();
            if page.html == html {
                return;
            }
            page.html = html.to_owned();
        }
        // A view without a browser loads the new document when it gets one.
        view.html_pending.set(view.browser.borrow().is_some());
    }

    fn set_visible(&self, visible: bool) {
        let view = &self.0;
        if view.visible.replace(visible) == visible {
            return;
        }
        view.hidden_since.set((!visible).then(Instant::now));
        if !visible {
            let unpainted = view.state.restyle_unpainted.load(Ordering::SeqCst);
            if unpainted && view.host().is_some() {
                view.hide_deferred.set(true);
            } else {
                view.hide_now();
            }
            return;
        }
        view.hide_deferred.set(false);
        view.state.visible.store(true, Ordering::SeqCst);
        // A view without a browser gets one from the next pump.
        if let Some(host) = view.host() {
            host.was_hidden(0);
            host.invalidate(PaintElementType::VIEW);
        }
    }

    fn take(&self) -> Updates {
        let state = &self.0.state;
        state.dirty.store(false, Ordering::SeqCst);
        std::mem::take(&mut *state.updates.lock().unwrap())
    }
}
