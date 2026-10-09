//! A second real adapter at the [`Engine`] seam: draws synthetic frames with no
//! browser, so the host can be built, tested and run without Chromium.
//!
//! Every frame is deterministic: the theme's `--ferrite-bg` (or a fixed grey)
//! as the background, a few coloured blocks laid out like a tiny treemap, and a
//! highlight square under the last pointer position, so hover forwarding is
//! visible on screen. The page is always [`CONTENT_HEIGHT`] logical pixels tall.
//!
//! **Timing:** the fake renders synchronously. `open`, `resize`, `input`,
//! `set_theme` and `set_visible(true)` draw the new frame (if anything changed
//! and the view is visible) and call `wake` before returning. A real engine
//! renders later, on its own schedule; the host must not depend on either.
//!
//! **Release:** like the real engine, a view hidden for the engine's grace
//! period ([`RELEASE_AFTER`], or [`FakeEngine::with_grace`]) is released by
//! the next `pump`: its "page" is dropped, and with it the in-page state (the
//! hover square). Showing it again reloads: a fresh frame of the page as
//! first loaded, at the latest viewport and theme. [`FakeEngine::live_views`]
//! counts views that are not released.
//!
//! The engine also keeps a [`Call`] journal of everything the host told it, so
//! tests can check what the host forwarded without reaching into the host.

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use super::{Cursor, Engine, Frame, Input, Page, Updates, View, Viewport, Wake};

/// The fake page's height in logical pixels.
pub const CONTENT_HEIGHT: u32 = 360;
/// The real engine's grace period (`cef::RELEASE_AFTER`).
pub const RELEASE_AFTER: Duration = Duration::from_secs(2);
/// The highlight square's side, in logical pixels.
const HOVER_SIDE: f32 = 24.0;
/// Background when the theme has no parseable `--ferrite-bg`.
const FALLBACK_BG: [u8; 3] = [0x30, 0x30, 0x34];
const HOVER: [u8; 3] = [0xff, 0xff, 0xff];

/// Blocks as fractions of the page: (x, y, w, h, rgb).
const BLOCKS: [(f32, f32, f32, f32, [u8; 3]); 5] = [
    (0.04, 0.08, 0.50, 0.55, [0x82, 0xb1, 0xf2]),
    (0.56, 0.08, 0.40, 0.30, [0x93, 0xcf, 0x8c]),
    (0.56, 0.40, 0.19, 0.23, [0xe6, 0xc4, 0x7c]),
    (0.77, 0.40, 0.19, 0.23, [0xc5, 0x9d, 0xf0]),
    (0.04, 0.66, 0.92, 0.26, [0xef, 0x8a, 0x80]),
];

/// Something the host told the engine, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    Open(Viewport),
    Resize(Viewport),
    Input(Input),
    Theme(String),
    Visible(bool),
    /// The engine released a view that stayed hidden (not told by the host).
    Released,
    /// A released view was shown again and reloaded its page.
    Reopened,
    /// The view was dropped.
    Close,
}

pub struct FakeEngine {
    journal: Rc<RefCell<Vec<Call>>>,
    grace: Duration,
    views: RefCell<Vec<Weak<FakeView>>>,
}

impl Default for FakeEngine {
    fn default() -> Self {
        Self::with_grace(RELEASE_AFTER)
    }
}

impl FakeEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Release views hidden for `grace` (zero: on the next pump).
    pub fn with_grace(grace: Duration) -> Self {
        Self { journal: Rc::default(), grace, views: RefCell::default() }
    }

    /// Views that are open and not released.
    pub fn live_views(&self) -> usize {
        self.views.borrow().iter().filter_map(Weak::upgrade).filter(|v| !v.state.borrow().released).count()
    }

    /// Everything every view of this engine was told, oldest first.
    pub fn journal(&self) -> Vec<Call> {
        self.journal.borrow().clone()
    }
}

impl Engine for FakeEngine {
    fn open(&self, page: Page, viewport: Viewport, wake: Wake) -> Box<dyn View> {
        self.journal.borrow_mut().push(Call::Open(viewport));
        let view = Rc::new(FakeView {
            state: RefCell::new(State {
                viewport,
                bg: parse_bg(&page.theme_css),
                hover: None,
                visible: true,
                hidden_since: None,
                released: false,
                stale: true,
                cursor: Cursor::Arrow,
                pending: Updates::default(),
            }),
            wake,
            journal: self.journal.clone(),
        });
        self.views.borrow_mut().push(Rc::downgrade(&view));
        view.render();
        Box::new(Handle(view))
    }

    fn pump(&self) -> Duration {
        let mut views = self.views.borrow_mut();
        views.retain(|v| v.strong_count() > 0);
        for view in views.iter().filter_map(Weak::upgrade) {
            let release = {
                let mut s = view.state.borrow_mut();
                let due = !s.released && s.hidden_since.is_some_and(|t| t.elapsed() >= self.grace);
                if due {
                    s.released = true;
                    // The page is gone, and its state with it.
                    s.hover = None;
                    s.cursor = Cursor::Arrow;
                    s.pending = Updates::default();
                }
                due
            };
            if release {
                view.log(Call::Released);
            }
        }
        if views.is_empty() { Duration::from_millis(250) } else { self.grace.min(Duration::from_millis(250)) }
    }
}

/// What the host holds; the engine keeps a weak reference for `pump`.
struct Handle(Rc<FakeView>);

struct FakeView {
    state: RefCell<State>,
    wake: Wake,
    journal: Rc<RefCell<Vec<Call>>>,
}

struct State {
    viewport: Viewport,
    bg: [u8; 3],
    /// In-page state: lost when the view is released.
    hover: Option<(f32, f32)>,
    visible: bool,
    hidden_since: Option<Instant>,
    released: bool,
    /// The last frame handed out no longer matches the state.
    stale: bool,
    cursor: Cursor,
    pending: Updates,
}

impl FakeView {
    fn log(&self, call: Call) {
        self.journal.borrow_mut().push(call);
    }

    /// Draw a frame if one is due and the view is visible, then wake the host.
    fn render(&self) {
        {
            let mut s = self.state.borrow_mut();
            if !s.visible || !s.stale {
                return;
            }
            s.stale = false;
            let frame = draw(s.viewport, s.bg, s.hover);
            s.pending.frame = Some(frame);
            s.pending.content_height = Some(CONTENT_HEIGHT);
        }
        (self.wake)();
    }

    fn set_cursor(&self, cursor: Cursor) {
        let changed = {
            let mut s = self.state.borrow_mut();
            let changed = s.cursor != cursor;
            if changed {
                s.cursor = cursor;
                s.pending.cursor = Some(cursor);
            }
            changed
        };
        if changed {
            (self.wake)();
        }
    }
}

impl View for Handle {
    fn resize(&self, viewport: Viewport) {
        self.0.resize(viewport)
    }
    fn input(&self, input: Input) {
        self.0.input(input)
    }
    fn set_theme(&self, theme_css: &str) {
        self.0.set_theme(theme_css)
    }
    fn set_visible(&self, visible: bool) {
        self.0.set_visible(visible)
    }
    fn take(&self) -> Updates {
        self.0.take()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.0.log(Call::Close);
    }
}

impl FakeView {
    fn resize(&self, viewport: Viewport) {
        self.log(Call::Resize(viewport));
        {
            let mut s = self.state.borrow_mut();
            if s.viewport == viewport {
                return;
            }
            s.viewport = viewport;
            s.stale = true;
        }
        self.render();
    }

    fn input(&self, input: Input) {
        self.log(Call::Input(input.clone()));
        if self.state.borrow().released {
            return;
        }
        let hover = match input {
            Input::Move { x, y, .. } => Some((x, y)),
            Input::Leave => None,
            _ => return,
        };
        let width = {
            let mut s = self.state.borrow_mut();
            if s.hover == hover {
                return;
            }
            s.hover = hover;
            s.stale = true;
            s.viewport.width as f32
        };
        let over_block = hover.is_some_and(|(x, y)| block_at(width, x, y).is_some());
        self.set_cursor(if over_block { Cursor::Pointer } else { Cursor::Arrow });
        self.render();
    }

    fn set_theme(&self, theme_css: &str) {
        self.log(Call::Theme(theme_css.to_owned()));
        {
            let mut s = self.state.borrow_mut();
            let bg = parse_bg(theme_css);
            if s.bg == bg {
                return;
            }
            s.bg = bg;
            s.stale = true;
        }
        self.render();
    }

    fn set_visible(&self, visible: bool) {
        self.log(Call::Visible(visible));
        {
            let mut s = self.state.borrow_mut();
            if s.visible == visible {
                return;
            }
            s.visible = visible;
            s.hidden_since = (!visible).then(Instant::now);
            if !visible {
                // Nothing produced while hidden reaches the host.
                s.pending.frame = None;
                return;
            }
            // "Showing it again repaints" (and a released view reloads).
            s.stale = true;
        }
        if std::mem::take(&mut self.state.borrow_mut().released) {
            self.log(Call::Reopened);
        }
        self.render();
    }

    fn take(&self) -> Updates {
        std::mem::take(&mut self.state.borrow_mut().pending)
    }
}

/// The block under a logical position, if any.
fn block_at(width: f32, x: f32, y: f32) -> Option<usize> {
    let height = CONTENT_HEIGHT as f32;
    BLOCKS.iter().position(|&(bx, by, bw, bh, _)| {
        x >= bx * width && x < (bx + bw) * width && y >= by * height && y < (by + bh) * height
    })
}

fn draw(viewport: Viewport, bg: [u8; 3], hover: Option<(f32, f32)>) -> Frame {
    let scale = viewport.scale;
    let width = (viewport.width as f32 * scale).round() as u32;
    let height = (CONTENT_HEIGHT as f32 * scale).round() as u32;
    let mut bgra = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..width * height {
        bgra.extend_from_slice(&[bg[2], bg[1], bg[0], 0xff]);
    }
    let mut fill = |x0: f32, y0: f32, x1: f32, y1: f32, rgb: [u8; 3]| {
        let clamp = |v: f32, max: u32| (v.round().max(0.0) as u32).min(max);
        let (x0, x1) = (clamp(x0 * scale, width), clamp(x1 * scale, width));
        let (y0, y1) = (clamp(y0 * scale, height), clamp(y1 * scale, height));
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * width + x) * 4) as usize;
                bgra[i..i + 4].copy_from_slice(&[rgb[2], rgb[1], rgb[0], 0xff]);
            }
        }
    };
    let (lw, lh) = (viewport.width as f32, CONTENT_HEIGHT as f32);
    for (bx, by, bw, bh, rgb) in BLOCKS {
        fill(bx * lw, by * lh, (bx + bw) * lw, (by + bh) * lh, rgb);
    }
    if let Some((x, y)) = hover {
        let r = HOVER_SIDE / 2.0;
        fill(x - r, y - r, x + r, y + r, HOVER);
    }
    Frame { width, height, bgra }
}

/// `--ferrite-bg: #rrggbb` (or `#rgb`) from a theme block.
fn parse_bg(css: &str) -> [u8; 3] {
    let parse = || -> Option<[u8; 3]> {
        let rest = &css[css.find("--ferrite-bg")? + "--ferrite-bg".len()..];
        let value = rest.trim_start().strip_prefix(':')?.trim_start().strip_prefix('#')?;
        let hex: String = value.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        let byte = |s: &str| u8::from_str_radix(s, 16).ok();
        match hex.len() {
            6 => Some([byte(&hex[0..2])?, byte(&hex[2..4])?, byte(&hex[4..6])?]),
            3 => {
                let d = |i: usize| byte(&hex[i..i + 1]).map(|v| v * 17);
                Some([d(0)?, d(1)?, d(2)?])
            }
            _ => None,
        }
    };
    parse().unwrap_or(FALLBACK_BG)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::Modifiers;
    use std::cell::Cell;

    const VIEWPORT: Viewport = Viewport { width: 200, scale: 2.0 };

    fn page(bg: &str) -> Page {
        Page { html: "<p>hi</p>".into(), theme_css: format!(":root {{ --ferrite-bg: {bg}; }}") }
    }

    fn open(engine: &FakeEngine) -> (Box<dyn View>, Rc<Cell<u32>>) {
        let wakes = Rc::new(Cell::new(0));
        let counter = wakes.clone();
        let view = engine.open(page("#102030"), VIEWPORT, Box::new(move || counter.set(counter.get() + 1)));
        (view, wakes)
    }

    fn pixel(frame: &Frame, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * frame.width + x) * 4) as usize;
        frame.bgra[i..i + 4].try_into().unwrap()
    }

    fn moved(x: f32, y: f32) -> Input {
        Input::Move { x, y, modifiers: Modifiers::default() }
    }

    #[test]
    fn the_first_frame_is_device_sized_bgra_on_the_theme_background() {
        let engine = FakeEngine::new();
        let (view, wakes) = open(&engine);
        assert!(wakes.get() >= 1, "opening wakes the host");
        let updates = view.take();
        assert_eq!(updates.content_height, Some(CONTENT_HEIGHT));
        let frame = updates.frame.expect("a first frame");
        assert_eq!((frame.width, frame.height), (400, 720));
        assert_eq!(frame.bgra.len(), 400 * 720 * 4);
        assert_eq!(pixel(&frame, 0, 0), [0x30, 0x20, 0x10, 0xff], "BGRA order");
        assert!(view.take().is_empty(), "take drains");
    }

    #[test]
    fn hover_draws_under_the_pointer_and_leave_restores() {
        let engine = FakeEngine::new();
        let (view, _) = open(&engine);
        let before = view.take().frame.unwrap();
        // Logical (100, 5) is background (blocks start at 8% of the height).
        view.input(moved(100.0, 5.0));
        let hovered = view.take().frame.expect("hover repaints");
        assert_ne!(pixel(&hovered, 200, 10), pixel(&before, 200, 10));
        assert_eq!(pixel(&hovered, 200, 10), [0xff, 0xff, 0xff, 0xff]);
        view.input(Input::Leave);
        let after = view.take().frame.expect("leave repaints");
        assert_eq!(after, before);
    }

    #[test]
    fn the_cursor_is_a_pointer_over_blocks_only() {
        let engine = FakeEngine::new();
        let (view, _) = open(&engine);
        view.take();
        view.input(moved(50.0, 100.0));
        assert_eq!(view.take().cursor, Some(Cursor::Pointer));
        view.input(moved(100.0, 2.0));
        assert_eq!(view.take().cursor, Some(Cursor::Arrow));
    }

    #[test]
    fn a_hidden_view_produces_nothing_until_shown() {
        let engine = FakeEngine::new();
        let (view, wakes) = open(&engine);
        view.take();
        view.set_visible(false);
        let woken = wakes.get();
        view.input(moved(10.0, 10.0));
        view.resize(Viewport { width: 300, scale: 2.0 });
        assert_eq!(view.take().frame, None);
        view.set_visible(true);
        assert!(wakes.get() > woken);
        let frame = view.take().frame.expect("showing repaints");
        assert_eq!(frame.width, 600, "at the size it was given while hidden");
    }

    fn released_and_reopened(engine: &FakeEngine) -> (usize, usize) {
        let journal = engine.journal();
        let count = |call: Call| journal.iter().filter(|c| **c == call).count();
        (count(Call::Released), count(Call::Reopened))
    }

    #[test]
    fn a_view_hidden_past_the_grace_period_is_released_by_pump() {
        let engine = FakeEngine::with_grace(Duration::ZERO);
        let (view, _) = open(&engine);
        view.take();
        engine.pump();
        assert_eq!((engine.live_views(), released_and_reopened(&engine)), (1, (0, 0)), "visible: kept");
        view.set_visible(false);
        engine.pump();
        assert_eq!(engine.live_views(), 0);
        assert_eq!(released_and_reopened(&engine), (1, 0));
        assert!(!engine.journal().contains(&Call::Close), "released, not closed: the host still holds it");
        engine.pump();
        assert_eq!(released_and_reopened(&engine), (1, 0), "released once");
    }

    #[test]
    fn a_view_shown_again_within_the_grace_period_is_not_released() {
        let engine = FakeEngine::with_grace(Duration::from_secs(3600));
        let (view, _) = open(&engine);
        view.input(moved(100.0, 5.0));
        view.take();
        view.set_visible(false);
        engine.pump();
        view.set_visible(true);
        assert_eq!(released_and_reopened(&engine), (0, 0));
        let frame = view.take().frame.expect("showing repaints");
        assert_eq!(pixel(&frame, 200, 10), [0xff, 0xff, 0xff, 0xff], "in-page state kept");
    }

    #[test]
    fn a_released_view_reloads_when_shown_and_loses_in_page_state_only() {
        let engine = FakeEngine::with_grace(Duration::ZERO);
        let (view, wakes) = open(&engine);
        let loaded = view.take().frame.unwrap();
        view.input(moved(100.0, 5.0));
        view.take();
        view.set_visible(false);
        engine.pump();
        // Told things while released: kept, applied on reload.
        view.input(moved(50.0, 50.0));
        view.set_theme(":root { --ferrite-bg: #123456; }");
        view.resize(Viewport { width: 300, scale: 2.0 });
        assert!(view.take().is_empty(), "nothing while released");
        let woken = wakes.get();
        view.set_visible(true);
        assert_eq!(released_and_reopened(&engine), (1, 1));
        assert_eq!(engine.live_views(), 1);
        assert!(wakes.get() > woken);
        let updates = view.take();
        assert_eq!(updates.content_height, Some(CONTENT_HEIGHT));
        let frame = updates.frame.expect("a fresh first frame");
        assert_eq!(frame.width, 600, "latest viewport");
        assert_eq!(pixel(&frame, 0, 0), [0x56, 0x34, 0x12, 0xff], "latest theme");
        assert_eq!(pixel(&frame, 200, 10), [0x56, 0x34, 0x12, 0xff], "no hover square: in-page state is gone");
        assert_ne!(pixel(&frame, 100, 100), [0xff, 0xff, 0xff, 0xff], "nor the move sent while released");
        assert_ne!(frame, loaded);
    }

    #[test]
    fn take_coalesces_to_the_latest_frame() {
        let engine = FakeEngine::new();
        let (view, _) = open(&engine);
        view.take();
        view.input(moved(100.0, 5.0));
        view.input(Input::Leave);
        let reference = {
            let other = FakeEngine::new();
            let (fresh, _) = open(&other);
            fresh.take().frame.unwrap()
        };
        let updates = view.take();
        assert_eq!(updates.frame, Some(reference), "one frame, the latest");
        assert!(view.take().frame.is_none());
    }

    #[test]
    fn theme_changes_repaint_the_background() {
        let engine = FakeEngine::new();
        let (view, _) = open(&engine);
        view.take();
        view.set_theme(":root { --ferrite-bg: #fff; }");
        assert_eq!(pixel(&view.take().frame.unwrap(), 0, 0), [0xff, 0xff, 0xff, 0xff]);
        view.set_theme(":root { --ferrite-bg: nonsense; }");
        let fallback = [FALLBACK_BG[2], FALLBACK_BG[1], FALLBACK_BG[0], 0xff];
        assert_eq!(pixel(&view.take().frame.unwrap(), 0, 0), fallback);
    }
}
