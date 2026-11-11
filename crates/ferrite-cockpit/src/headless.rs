//! A Cockpit window drawn off-screen and driven by keys, clicks and shots
//! (ADR 0038): the live reference's window, and `ferrite --headless`.
//! Never on screen, never frontmost, and needing no Accessibility grant:
//! the input is dispatched to the window itself and the frames are drawn
//! to an image, through gpui's test support (the `headless` feature).

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ferrite_core::drive::{Action, Answer, Click, Find, Keys, Press, Shot, Target, Text};
use ferrite_core::settings::Settings;
use ferrite_ui::keymap::{self, Binding};
use gpui::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, HeadlessAppContext, Keystroke,
    Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput, Point,
    Styled as _, Window,
};

use crate::cockpit::{CockpitView, Preferences};
use crate::machines::{Machines, Reach};

/// How often the window's clock moves while nothing drives it: the
/// Server's updates and the API's actions wait at most this long.
const TICK_MS: u64 = 16;

/// `ferrite --headless`: the production window on this Store's Server,
/// drawn off-screen and driven only by the Server's `window.*` actions.
/// It runs until it is stopped.
pub fn run() {
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
    let machines = cx.new(|_| {
        Machines::new(
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
                bridge: ferrite_core::machine::Bridge::ssh,
            },
            &settings.machines,
            None,
        )
    });
    let core = match cx.update(|cx| machines.update(cx, |machines, _| machines.cockpit())) {
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
        CockpitView::new_with_settings(core, provider, prefs, cx).with_machines(machines)
    });
    eprintln!(
        "ferrite --headless: pid {}, on the server of {}",
        std::process::id(),
        store.display()
    );
    loop {
        driver.pass(TICK_MS);
        for (request, action) in driver.update(|view, _, _| view.cockpit.take_drives()) {
            let answer = act(&mut driver, action);
            driver.update(|view, _, _| view.cockpit.driven(request, answer));
        }
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
        Action::Click(Click { at, press }) => {
            let (button, clicks) = match press {
                Press::Single => (MouseButton::Left, 1),
                Press::Double => (MouseButton::Left, 2),
                Press::Right => (MouseButton::Right, 1),
            };
            let modifiers = Modifiers::default();
            let at = match at {
                Target::Selector(selector) => driver
                    .click(&selector, button, clicks, modifiers)
                    .ok_or_else(|| not_drawn(&selector))?,
                Target::Point { x, y } => {
                    let at = gpui::point(gpui::px(x as f32), gpui::px(y as f32));
                    driver.click_at(at, button, clicks, modifiers);
                    at
                }
            };
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
pub const SIZE: (f32, f32) = (1440., 900.);

/// One off-screen Cockpit window and the hands that drive it.
pub struct Driver {
    cx: HeadlessAppContext,
    window: AnyWindowHandle,
    view: Entity<CockpitView>,
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
        let mut view = None;
        let window = cx
            .open_window(gpui::size(gpui::px(SIZE.0), gpui::px(SIZE.1)), |window, cx| {
                let cockpit = cx.new(build);
                view = Some(cockpit.clone());
                let content =
                    cx.new(|cx| crate::cockpit_window::CockpitWindow::new(cockpit, window, cx));
                cx.new(|cx| {
                    gpui::component::Root::new(content, window, cx)
                        .bordered(false)
                        .bg(ferrite_ui::theme::paint::WINDOW)
                })
            })
            .expect("open the off-screen window");
        Self {
            cx,
            window: window.into(),
            view: view.expect("the window built its Cockpit"),
        }
    }

    pub fn read<R>(&mut self, f: impl FnOnce(&CockpitView, &App) -> R) -> R {
        let view = self.view.clone();
        self.cx.update(|cx| f(view.read(cx), cx))
    }

    pub fn update<R>(
        &mut self,
        f: impl FnOnce(&mut CockpitView, &mut Window, &mut Context<CockpitView>) -> R,
    ) -> R {
        let view = self.view.clone();
        self.cx
            .update_window(self.window, |_, window, cx| {
                view.update(cx, |view, cx| f(view, window, cx))
            })
            .expect("the off-screen window stands")
    }

    /// One slice of real time, mirrored on the executor's clock: the pump
    /// runs and the frames the last draw asked for are delivered. The
    /// window draws only if something in it changed, as on screen.
    pub fn pass(&mut self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
        self.cx.advance_clock(Duration::from_millis(ms));
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

    fn draw(&mut self) {
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
            handled.push(
                self.cx
                    .update_window(self.window, |_, window, cx| {
                        window.dispatch_keystroke(keystroke, cx)
                    })
                    .expect("the off-screen window stands"),
            );
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
            self.cx
                .update_window(self.window, |_, window, cx| {
                    window.dispatch_keystroke(key, cx)
                })
                .expect("the off-screen window stands");
            self.cx.run_until_parked();
        }
        self.tick(60);
        Ok(())
    }

    /// Where the last frame drew the element of this `debug_selector`.
    pub fn bounds(&mut self, selector: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.cx
            .update_window(self.window, |_, window, _| window.debug_bounds(selector))
            .ok()
            .flatten()
    }

    pub fn mouse(&mut self, event: PlatformInput) {
        self.cx
            .update_window(self.window, |_, window, cx| {
                window.dispatch_event(event, cx);
            })
            .expect("the off-screen window stands");
        self.tick(30);
    }

    /// `clicks` presses of `button` at the centre of what `selector` drew,
    /// the pointer moved there first so hover state is honest. Where it
    /// clicked; `None` when the last frame drew no such element.
    pub fn click(
        &mut self,
        selector: &str,
        button: MouseButton,
        clicks: usize,
        modifiers: Modifiers,
    ) -> Option<Point<gpui::Pixels>> {
        let at = self.bounds(selector)?.center();
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
        let Self { cx, view, .. } = self;
        drop(view);
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(cx)));
        std::panic::set_hook(hook);
    }
}

fn parse(keystroke: &str) -> Result<Keystroke, String> {
    Keystroke::parse(keystroke).map_err(|_| format!("`{keystroke}` is not a keystroke"))
}
