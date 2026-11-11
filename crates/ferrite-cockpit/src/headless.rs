//! A Cockpit window drawn off-screen and driven by keys, clicks and shots
//! (ADR 0038): the live reference's window, and `ferrite --headless`.
//! Never on screen, never frontmost, and needing no Accessibility grant:
//! the input is dispatched to the window itself and the frames are drawn
//! to an image, through gpui's test support (the `headless` feature).

use std::path::Path;
use std::time::Duration;

use ferrite_ui::keymap::Binding;
use gpui::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, HeadlessAppContext, Keystroke,
    Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput, Point,
    Styled as _, Window,
};

use crate::cockpit::CockpitView;

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
        let parsed = parse(keystroke)?;
        let handled = self
            .cx
            .update_window(self.window, |_, window, cx| {
                window.dispatch_keystroke(parsed, cx)
            })
            .expect("the off-screen window stands");
        self.tick(60);
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
