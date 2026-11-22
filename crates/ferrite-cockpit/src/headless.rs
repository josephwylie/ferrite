//! A Cockpit window drawn off-screen and driven by keys, clicks and shots
//! (ADR 0038): the live reference's window, and `ferrite --headless`.
//! Never on screen, never frontmost, and needing no Accessibility grant:
//! the input is dispatched to the window itself and the frames are drawn
//! to an image, through gpui's test support (the `headless` feature).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ferrite_core::drive::{
    Action, Answer, Click, Drag, Find, Focused, Keys, Press, Resize, Shot, Target, Text,
};
use ferrite_core::settings::Settings;
use ferrite_ui::keymap::{self, Binding};
use gpui::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, HeadlessAppContext, KeyUpEvent,
    Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PlatformInput, Point, Styled as _, Window,
};

use crate::cockpit::{CockpitView, Preferences};
use crate::machines::{Machines, Reach};

/// How often the window's clock moves while nothing drives it: the
/// Server's updates and the API's actions wait at most this long.
const TICK_MS: u64 = 16;

/// `ferrite --headless`: the production window on this Store's Server,
/// drawn off-screen and driven only by the Server's `window.*` actions.
/// It runs until it is stopped. Remote images load through `http`, as the
/// app's do.
pub fn run(http: Arc<dyn gpui::http_client::HttpClient>) {
    // Never the operator's Store: a driven window on it would let any API
    // caller type into their Threads and answer their Decisions.
    throwaway("FERRITE_STORE", "");
    let settings_dir = ferrite_core::server::settings_dir();
    let store = ferrite_core::server::store_dir();
    ferrite_core::diagnostics::open(&settings_dir);
    let (settings, unreadable) = match Settings::load(&settings_dir) {
        Ok(settings) => (settings, None),
        Err(e) => {
            ferrite_core::log!("settings.json could not be read: {e}");
            (Settings::default(), Some(e.to_string()))
        }
    };
    let (keys, keys_refused) = keymap::with_user_keys(keymap::PLATFORM, &settings.keys);
    let defaults = Arc::new(Mutex::new(
        ferrite_core::spawner::SessionDefaults::from_settings(&settings),
    ));
    let mut cx = app(keys);
    cx.update(|cx| cx.set_http_client(http));
    let mut machines = Machines::new(
        Reach {
            settings_dir: settings_dir.clone(),
            store: store.clone(),
            executable: std::env::current_exe().unwrap_or_default(),
            client: ferrite_core::protocol::Peer {
                name: "ferrite".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            renderer: None,
            driven: true,
            defaults: defaults.clone(),
            bridge: ferrite_core::machine::Bridge::for_machine,
        },
        &settings.machines,
        None,
        settings.local_environment_enabled,
    );
    let core = match machines.cockpit() {
        Ok(core) => core,
        Err(e) => {
            eprintln!("ferrite --headless: cannot reach the Ferrite server: {e}");
            std::process::exit(1);
        }
    };
    let provider = settings.default_provider;
    let prefs = Preferences {
        settings,
        dir: settings_dir,
        defaults,
        titler: true,
        cli_updates: false,
        ferrite_updates: false,
        whats_new: None,
        unreadable,
        keys_refused,
    };
    let mut driver = Driver::open(cx, |cx| {
        CockpitView::new_with_machines(core, provider, prefs, Some(machines), cx)
    });
    eprintln!(
        "ferrite --headless: pid {}, on the server of {}",
        std::process::id(),
        store.display()
    );
    loop {
        driver.pass(TICK_MS);
        let source = driver.current_view();
        let requests = driver.cx.update(|cx| {
            source.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| cockpit.take_drives()))
        });
        for (machine, request, action) in requests {
            let answer = act(&mut driver, action);
            driver.cx.update(|cx| {
                source.update(cx, |view, cx| {
                    view.cockpit.update(cx, |cockpit, _| cockpit.driven(machine, request, answer))
                });
            });
        }
    }
}

/// The point a target names: a selector's centre as the last frame drew
/// it, refused while out of sight ([`Driver::aim`]).
fn place(driver: &mut Driver, target: Target) -> Result<Point<gpui::Pixels>, String> {
    match target {
        Target::Selector(selector) => driver.aim(&selector),
        Target::Point { x, y } => Ok(gpui::point(gpui::px(x as f32), gpui::px(y as f32))),
    }
}

/// Carry out one action, as the operator's hands would.
fn act(driver: &mut Driver, action: Action) -> Result<Answer, String> {
    let not_drawn = |selector: &str| format!("nothing drawn as `{selector}`");
    match action {
        Action::Key(Keys { keys }) => {
            let pressed: Vec<&str> = keys.iter().map(String::as_str).collect();
            let handled = driver.keys(&pressed)?;
            let unhandled = keys
                .into_iter()
                .zip(handled)
                .filter_map(|(key, handled)| (!handled).then_some(key))
                .collect();
            Ok(Answer::Pressed { unhandled })
        }
        Action::Type(Text { text }) => driver.type_text(&text).map(|()| Answer::Typed),
        Action::Click(Click { at, press, command }) => {
            let (button, clicks) = match press {
                Press::Single => (MouseButton::Left, 1),
                Press::Double => (MouseButton::Left, 2),
                Press::Right => (MouseButton::Right, 1),
            };
            let modifiers = if command {
                Modifiers::secondary_key()
            } else {
                Modifiers::default()
            };
            let at = place(driver, at)?;
            driver.click_at(at, button, clicks, modifiers);
            Ok(Answer::Clicked {
                x: at.x.into(),
                y: at.y.into(),
            })
        }
        Action::Find(Find { selector }) => {
            let bounds = driver
                .bounds(&selector)
                .ok_or_else(|| not_drawn(&selector))?;
            Ok(Answer::Found {
                x: bounds.origin.x.into(),
                y: bounds.origin.y.into(),
                width: bounds.size.width.into(),
                height: bounds.size.height.into(),
            })
        }
        Action::Read(Find { selector }) => {
            let lines = driver.text(&selector).ok_or_else(|| not_drawn(&selector))?;
            Ok(Answer::Read { lines })
        }
        Action::Focus => Ok(Answer::Focused(driver.focus())),
        Action::Attach(files) => {
            if files.paths.is_empty() || files.paths.iter().any(|path| !path.is_absolute()) {
                return Err("attachments require absolute local file paths".into());
            }
            driver.update(|view, _, cx| view.attach_local_files(&files.paths, cx))?;
            driver.tick(60);
            Ok(Answer::Attached)
        }
        Action::Resize(Resize { width, height }) => {
            let size = driver.resize(gpui::size(gpui::px(width as f32), gpui::px(height as f32)));
            Ok(Answer::Resized {
                width: size.width.into(),
                height: size.height.into(),
            })
        }
        Action::Drag(Drag { from, to }) => {
            let (from, to) = (place(driver, from)?, place(driver, to)?);
            for event in crate::hands::drag(from, to) {
                driver.mouse(event);
            }
            Ok(Answer::Dragged {
                from_x: from.x.into(),
                from_y: from.y.into(),
                to_x: to.x.into(),
                to_y: to.y.into(),
            })
        }
        Action::Shot(Shot { path }) => {
            if !path.is_absolute() {
                return Err(format!("a shot's path is absolute, not {}", path.display()));
            }
            let saved = driver.shot(&path)?;
            Ok(Answer::Shot {
                path,
                width: saved.width,
                height: saved.height,
                scale: saved.scale,
            })
        }
    }
}

/// The app every off-screen window lives in: real text, the Cockpit's
/// components and fonts, and `keys` bound.
pub fn app(keys: Vec<Binding>) -> HeadlessAppContext {
    let platform = gpui::platform::current_platform(true);
    let mut cx = HeadlessAppContext::with_platform(
        platform.text_system(),
        std::sync::Arc::new(ferrite_ui::icons::Assets),
        gpui::platform::current_headless_renderer,
    );
    cx.update(|cx| {
        ferrite_pane::pane::init_components(cx);
        ferrite_ui::motion::init(cx);
        ferrite_ui::register_fonts(cx);
        crate::frame().bind_keys(keys, cx);
    });
    cx
}

/// The window's size in logical pixels, as the app's opens at.
const SIZE: (f32, f32) = (1440., 900.);

/// `SIZE`, or `WIDTHxHEIGHT` from `FERRITE_HEADLESS_SIZE`, so a capture can
/// show a narrow window.
fn window_size() -> (f32, f32) {
    std::env::var("FERRITE_HEADLESS_SIZE")
        .ok()
        .and_then(|size| {
            let (width, height) = size.split_once('x')?;
            Some((width.trim().parse().ok()?, height.trim().parse().ok()?))
        })
        .unwrap_or(SIZE)
}

/// One off-screen Cockpit window and the hands that drive it.
pub struct Driver {
    cx: HeadlessAppContext,
    window: AnyWindowHandle,
    shell: Entity<crate::cockpit_window::CockpitWindow>,
    /// The real instant the executor's clock last caught up to.
    synced: std::time::Instant,
}

/// What a shot saved: its size in pixels, and how many to a logical one.
pub struct Saved {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

impl Driver {
    /// The production window around the Cockpit `build` makes: the
    /// Cockpit's window inside the kit Root, as the app opens it.
    pub fn open(
        mut cx: HeadlessAppContext,
        build: impl FnOnce(&mut Context<CockpitView>) -> CockpitView,
    ) -> Self {
        let mut shell = None;
        let (width, height) = window_size();
        let window = cx
            .open_window(
                gpui::size(gpui::px(width), gpui::px(height)),
                |window, cx| {
                    let cockpit = cx.new(build);
                    let content =
                        cx.new(|cx| crate::cockpit_window::CockpitWindow::new(cockpit, window, cx));
                    shell = Some(content.clone());
                    cx.new(|cx| {
                        gpui::component::Root::new(content, window, cx)
                            .bordered(false)
                            .bg(ferrite_ui::theme::paint::WINDOW)
                    })
                },
            )
            .expect("open the off-screen window");
        Self {
            cx,
            window: window.into(),
            shell: shell.expect("the window built its shell"),
            synced: std::time::Instant::now(),
        }
    }

    fn current_view(&mut self) -> Entity<CockpitView> {
        self.cx.update(|cx| self.shell.read(cx).cockpit().clone())
    }

    pub fn read<R>(&mut self, f: impl FnOnce(&CockpitView, &App) -> R) -> R {
        let view = self.current_view();
        self.cx.update(|cx| f(view.read(cx), cx))
    }

    pub fn update<R>(
        &mut self,
        f: impl FnOnce(&mut CockpitView, &mut Window, &mut Context<CockpitView>) -> R,
    ) -> R {
        let view = self.current_view();
        self.cx
            .update_window(self.window, |_, window, cx| {
                view.update(cx, |view, cx| f(view, window, cx))
            })
            .expect("the off-screen window stands")
    }

    /// One slice of real time, mirrored on the executor's clock: the pump
    /// runs and the frames the last draw asked for are delivered. The
    /// window draws only if something in it changed, as on screen. The
    /// executor catches up on all the real time since its last pass, the
    /// time spent drawing and shooting too, so a timer set for a clock's
    /// next reading fires when the clock reads it.
    pub fn pass(&mut self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
        let now = std::time::Instant::now();
        self.cx.advance_clock(now - self.synced);
        self.synced = now;
        self.cx.run_until_parked();
        self.cx
            .update_window(self.window, |_, window, cx| {
                window.simulate_next_frame(cx);
            })
            .expect("the off-screen window stands");
    }

    /// A [`Self::pass`] that draws a frame whatever changed.
    pub fn tick(&mut self, ms: u64) {
        self.pass(ms);
        self.draw();
    }

    pub(crate) fn draw(&mut self) {
        self.cx
            .update_window(self.window, |_, window, cx| {
                let _ = window.draw(cx);
            })
            .expect("the off-screen window stands");
    }

    /// A keystroke (`cmd-,`, `escape`), as typed: true when something
    /// took it.
    pub fn key(&mut self, keystroke: &str) -> Result<bool, String> {
        self.keys(&[keystroke]).map(|handled| handled[0])
    }

    /// Keystrokes pressed in order, each given time to land; none when
    /// one is not a keystroke. Whether something took each.
    pub fn keys(&mut self, keystrokes: &[&str]) -> Result<Vec<bool>, String> {
        let parsed = keystrokes
            .iter()
            .map(|keystroke| parse(keystroke))
            .collect::<Result<Vec<_>, _>>()?;
        let mut handled = Vec::new();
        for keystroke in parsed {
            handled.push(self.press(keystroke));
            self.tick(60);
        }
        Ok(handled)
    }

    /// Words typed key by key into whatever holds the keyboard.
    pub fn type_text(&mut self, text: &str) -> Result<(), String> {
        let keys = text
            .chars()
            .map(|ch| match ch {
                ' ' => parse("space"),
                ch => parse(&ch.to_string()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        for key in keys {
            self.press(key);
            self.cx.run_until_parked();
        }
        self.tick(60);
        Ok(())
    }

    /// One key pressed and let go, as a keyboard sends it: the down (and
    /// the text it types when nothing binds it), its work run, then the
    /// up, which is where a focused control clicks from ⏎ or space. True
    /// when either was taken.
    fn press(&mut self, keystroke: Keystroke) -> bool {
        let down = self
            .cx
            .update_window(self.window, |_, window, cx| {
                window.dispatch_keystroke(keystroke.clone(), cx)
            })
            .expect("the off-screen window stands");
        self.cx.run_until_parked();
        let up = self
            .cx
            .update_window(self.window, |_, window, cx| {
                window.dispatch_event(PlatformInput::KeyUp(KeyUpEvent { keystroke }), cx)
            })
            .expect("the off-screen window stands");
        down || !up.propagate
    }

    /// Where the last frame drew the element of this `debug_selector`.
    pub fn bounds(&mut self, selector: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.cx
            .update_window(self.window, |_, window, _| window.debug_bounds(selector))
            .ok()
            .flatten()
    }

    /// The centre of what `selector` drew in the last frame, where a
    /// pointer presses it. Refused when that centre is scrolled or clipped
    /// out of sight (a tab half out of its row, its middle under the row's
    /// chevron): a press there would land on whatever shows in its place.
    pub fn aim(&mut self, selector: &str) -> Result<Point<gpui::Pixels>, String> {
        let (drawn, shown) = self
            .cx
            .update_window(self.window, |_, window, _| {
                Some((window.debug_bounds(selector)?, window.debug_shown_bounds(selector)?))
            })
            .ok()
            .flatten()
            .ok_or_else(|| format!("nothing drawn as `{selector}`"))?;
        let centre = drawn.center();
        if !shown.contains(&centre) {
            return Err(format!(
                "`{selector}` is drawn with its centre scrolled or clipped out of sight; \
                 bring it into view first"
            ));
        }
        Ok(centre)
    }

    /// The lines of text the last frame showed inside the element of this
    /// `debug_selector`, in paint order; `None` when it drew no such
    /// element.
    pub fn text(&mut self, selector: &str) -> Option<Vec<String>> {
        self.cx
            .update_window(self.window, |_, window, _| window.debug_text(selector))
            .ok()
            .flatten()
            .map(|lines| lines.into_iter().map(String::from).collect())
    }

    /// What holds the keyboard in the last frame: the key contexts a key
    /// goes through, the nearest selector on that path, and what the
    /// Cockpit names the place.
    pub fn focus(&mut self) -> Focused {
        let view = self.current_view();
        self.cx
            .update_window(self.window, |_, window, cx| Focused {
                contexts: window
                    .context_stack()
                    .iter()
                    .map(|context| format!("{context:?}"))
                    .collect(),
                selector: window.focused_debug_selector(),
                label: view.read(cx).keyboard_label(window, cx).map(String::from),
            })
            .expect("the off-screen window stands")
    }

    pub fn mouse(&mut self, event: PlatformInput) {
        self.cx
            .update_window(self.window, |_, window, cx| {
                window.dispatch_event(event, cx);
            })
            .expect("the off-screen window stands");
        self.tick(30);
    }

    /// `clicks` presses of `button` at the centre of what `selector` drew
    /// ([`Self::aim`]), the pointer moved there first so hover state is
    /// honest. Where it clicked; `None` when the last frame drew no such
    /// element or its centre is out of sight.
    pub fn click(
        &mut self,
        selector: &str,
        button: MouseButton,
        clicks: usize,
        modifiers: Modifiers,
    ) -> Option<Point<gpui::Pixels>> {
        let at = self.aim(selector).ok()?;
        self.click_at(at, button, clicks, modifiers);
        Some(at)
    }

    /// The pointer moved to `at`, then `clicks` presses of `button`: two
    /// make a double click.
    pub fn click_at(
        &mut self,
        at: Point<gpui::Pixels>,
        button: MouseButton,
        clicks: usize,
        modifiers: Modifiers,
    ) {
        self.mouse(PlatformInput::MouseMove(MouseMoveEvent {
            position: at,
            pressed_button: None,
            modifiers,
        }));
        for click_count in 1..=clicks {
            self.mouse(PlatformInput::MouseDown(MouseDownEvent {
                button,
                position: at,
                modifiers,
                click_count,
                first_mouse: false,
            }));
            self.mouse(PlatformInput::MouseUp(MouseUpEvent {
                button,
                position: at,
                modifiers,
                click_count,
            }));
        }
    }

    /// The window resized to `size` ([`crate::hands::resize`]) and drawn
    /// at it: the size it has now.
    pub fn resize(&mut self, size: gpui::Size<gpui::Pixels>) -> gpui::Size<gpui::Pixels> {
        let size = self
            .cx
            .update_window(self.window, |_, window, cx| crate::hands::resize(window, cx, size))
            .expect("the off-screen window stands");
        self.tick(60);
        size
    }

    /// Settle the frame and save it as a PNG at `path`.
    pub fn shot(&mut self, path: &Path) -> Result<Saved, String> {
        for _ in 0..6 {
            self.tick(60);
        }
        let scale = self
            .cx
            .update_window(self.window, |_, window, cx| {
                let _ = window.draw(cx);
                let _ = window.draw(cx);
                window.scale_factor()
            })
            .expect("the off-screen window stands");
        let image = self
            .cx
            .capture_screenshot(self.window)
            .map_err(|error| error.to_string())?;
        image.save(path).map_err(|error| error.to_string())?;
        Ok(Saved {
            width: image.width(),
            height: image.height(),
            scale,
        })
    }

    /// Close the window and its app. gpui's leak check may panic on the
    /// way out; nothing is left to report by then.
    pub fn close(self) {
        let Self { cx, shell, .. } = self;
        drop(shell);
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(cx)));
        std::panic::set_hook(hook);
    }
}

fn parse(keystroke: &str) -> Result<Keystroke, String> {
    Keystroke::parse(keystroke).map_err(|_| format!("`{keystroke}` is not a keystroke"))
}

fn temp_root() -> PathBuf {
    std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir())
}

/// Refuses any path outside the temp directories: the operator's store
/// and repos are never touched.
pub(crate) fn throwaway(var: &str, default: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var(var).unwrap_or_else(|_| default.to_string()));
    let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
    let allowed = [
        temp_root(),
        PathBuf::from("/private/tmp"),
        PathBuf::from("/tmp"),
    ];
    if !allowed.iter().any(|root| canonical.starts_with(root)) {
        eprintln!(
            "ferrite: {var} must be a throwaway path under the temp directory, not {}",
            path.display()
        );
        std::process::exit(2);
    }
    canonical
}
