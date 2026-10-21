//! Web views drawn inline: a page rendered off-screen by an engine, handed back
//! as frames for the host to draw like any other image.
//!
//! The host never positions anything on screen for the engine. It draws the
//! latest [`Frame`] wherever its own layout puts the view, so scrolling,
//! clipping and stacking need no coordination. The only things the engine is
//! told are the view's [`Viewport`] (its width and scale), the [`Input`] that
//! lands on it (in view coordinates), and whether it is visible.
//!
//! This module is GPUI-free: frames are plain BGRA pixels, and engines are
//! adapters at the [`Engine`] seam. [`fake::FakeEngine`] draws synthetic
//! frames for tests and for running without Chromium; `cef` (behind the `cef`
//! feature) is the real thing.

pub mod fake;
#[cfg(feature = "cef")]
pub mod cef;

use std::time::Duration;

/// What a view shows: the agent's HTML, and the host's theme as CSS custom
/// properties (`--ferrite-*`), applied before the page's own styles.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub html: String,
    /// A `:root { --ferrite-…: … }` block. Changing it restyles the live page.
    pub theme_css: String,
}

/// The view's width in logical pixels and the display's scale factor. Height
/// is not the host's to choose: the page reports it ([`Updates::content_height`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub width: u32,
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub command: bool,
}

/// Input that landed on the view. Positions are logical pixels from the view's
/// top-left corner; the host converts from window coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Move { x: f32, y: f32, modifiers: Modifiers },
    /// The pointer left the view (hover states end).
    Leave,
    Down { x: f32, y: f32, button: MouseButton, clicks: u32, modifiers: Modifiers },
    Up { x: f32, y: f32, button: MouseButton, clicks: u32, modifiers: Modifiers },
    /// Only forwarded when the page should scroll or zoom itself; by default the
    /// host keeps the wheel for its own scrolling.
    Wheel { x: f32, y: f32, dx: f32, dy: f32, modifiers: Modifiers },
    /// A key while the view has focus. `text` is what it types, if anything.
    Key { key: String, text: Option<String>, down: bool, modifiers: Modifiers },
}

/// One rendered frame: tightly packed BGRA, `width * height * 4` bytes, in
/// device pixels (logical size × scale).
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Cursor {
    #[default]
    Arrow,
    Pointer,
    Text,
    Grab,
    Grabbing,
    Crosshair,
    NotAllowed,
}

/// Everything that changed since the last [`View::take`]. Each field is the
/// latest value only: frames that were superseded before the host looked are
/// dropped, never queued.
#[derive(Debug, Default, PartialEq)]
pub struct Updates {
    pub frame: Option<Frame>,
    /// The page's full content height in logical pixels (its `scrollHeight`).
    pub content_height: Option<u32>,
    pub cursor: Option<Cursor>,
}

impl Updates {
    pub fn is_empty(&self) -> bool {
        self.frame.is_none() && self.content_height.is_none() && self.cursor.is_none()
    }
}

/// Called (on the main thread) whenever a view has new [`Updates`] to take.
pub type Wake = Box<dyn Fn()>;

/// A web engine. One per process, used from the main thread only.
pub trait Engine {
    /// Start rendering `page` at `viewport`. `wake` is called whenever the view
    /// has updates; the first frame and content height follow shortly.
    fn open(&self, page: Page, viewport: Viewport, wake: Wake) -> Box<dyn View>;

    /// Do any work that is due on the main thread, and say how long until the
    /// engine next needs this called. The host calls it on a timer.
    fn pump(&self) -> Duration;
}

/// One page. Dropping it closes the page and frees its resources; hiding it
/// for a while may free most of them too (see [`View::set_visible`]).
pub trait View {
    fn resize(&self, viewport: Viewport);
    fn input(&self, input: Input);
    /// Replace the theme without reloading the page.
    fn set_theme(&self, theme_css: &str);
    /// Whether the host wants the view live: on screen, or close enough that
    /// it soon will be. A view starts visible.
    ///
    /// **Hidden**, it stops producing frames at once; the host keeps showing
    /// the last frame it took. A view that stays hidden for the engine's grace
    /// period (about 2 s, so scrolling past and straight back costs nothing)
    /// may be **released**: the engine frees the page and whatever runs it
    /// (for Chromium, the page's share of a renderer process). The view
    /// object stays valid and keeps everything the host gave it: the page,
    /// the latest theme and viewport, and the content height it last
    /// reported. Input sent while released is dropped.
    ///
    /// **Shown again** before the grace period ends, the live page simply
    /// repaints. Shown after a release, the page is loaded again **from
    /// scratch**: in-page state is lost (an expanded section, a zoomed-in
    /// treemap, a sort order, hover, focus, anything the page's script built
    /// up after load). Reloading takes as long as the first open (tens to a
    /// few hundred ms), during which the engine sends nothing: no blank or
    /// half-built frame and no placeholder height. Its first frame is the
    /// laid-out page, and until its page reports otherwise the view's height
    /// is the one it last reported. So a host that keeps drawing its last
    /// frame meanwhile shows no flash and no height jump.
    fn set_visible(&self, visible: bool);
    /// Everything that changed since the last call; empty if nothing did.
    fn take(&self) -> Updates;
}
