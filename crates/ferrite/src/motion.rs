//! The motion kit: Zeron's animation catalog as helpers over gpui's
//! [`Animation`], plus the two clocks gpui does not give us — a throttled,
//! shared pulse for loops and a hover blend for colour. Ported from Zeron
//! (MIT, (c) 2026 Wing). The numbers are tokens in `theme.rs` (the motion
//! section), which also holds the rules; this module is the machinery.
//!
//! # Catalog
//!
//! Each Zeron animation, the Ferrite surface it applies to, and whether it
//! is on now or lands once the surface's owner has merged ("after merge").
//!
//! | Zeron | Spec | Ferrite surface | When |
//! | --- | --- | --- | --- |
//! | `transition-colors` hover | [`HOVER_FADE`] 150ms | nav rows: the hover wash fades in and out, the selected row's step-up likewise; a press is instant | now |
//! | selection move | none | the nav's one `FILL` moves at once: selection is keyboard-rate, a high-frequency interaction | now (kept instant) |
//! | sidebar width | [`RESIZE`] 200ms ease-out | nav collapse ⇄ rail: an interruptible [`Tween`] on the column's width, the content fading up from `MOTION_NAV_CONTENT_FROM` | now |
//! | toasts | kit-owned | enter and exit are gpui-component's own (see below); the `+N` bubble fades in on [`FADE_QUICK`] | now |
//! | working indicator | pulse clock | the working line's Ferrite mark and every breathing status dot ride [`pulse_phase`] (~30fps, one tick, parks) instead of a per-frame repeat | now |
//! | `menu-in` / `menu-out` | [`MENU_IN`] 140ms / [`MENU_OUT`] 100ms | menus and popovers (Composer menus, pickers, context menu, nav filter and order menus) via [`menu_in`] | after merge |
//! | `dialog-in` | [`DIALOG_IN`] 180ms | sheets (Settings, the Project editor) via [`dialog_in`] | after merge |
//! | chevron rotate | [`CHEVRON`] 150ms | disclosure chevrons: `svg` rotation is available ([`gpui::Transformation::rotate`]) | after merge |
//! | collapse | [`COLLAPSE`] 180ms | Group expand/collapse in the nav and tool-group disclosures, as a height [`Tween`] | after merge |
//! | icon swap | [`ICON_SWAP`] 300ms | the Composer's send ⇄ stop: both glyphs stay mounted and cross-fade, opacity 0→1 with `svg` scale 0.25→1 | after merge |
//! | `fade-in` | [`FADE_IN`] 500ms, 4px rise | a transcript block appended live; never on first paint or scroll-back | after merge |
//!
//! # What gpui here cannot do, and what stands in
//!
//! Zeron runs a gpui fork; Ferrite runs gpui-kit 0.6 on gpui-pre. Dropped or
//! approximated:
//!
//! - **`div` scale.** Only `svg` has a transform. `menu-in` and `dialog-in`
//!   carry their 0.96 → 1 scale as a 2px shift plus the fade, as Zeron does.
//! - **Blur.** No filter on elements, so the icon swap's `blur(4px)` is
//!   dropped; opacity and scale carry it.
//! - **Frost and edge fades.** Fork-only paint effects; not ported.
//! - **Toast enter and exit.** gpui-component's `Notification` hard-codes a
//!   400ms rise from 96px below and a 200ms exit; Ferrite configures only the
//!   stack (`DefaultToastMotion`), so the toast's own motion stays the kit's.
//!
//! # Reduced motion
//!
//! gpui's `App::reduce_motion` flag snaps every `with_animation` element (a
//! one-shot to its end state, a loop to its start) and every kit spring. The
//! clocks here follow the same rule: [`pulse_phase`] returns 0 and leases
//! nothing, a [`Tween`] reads its target, and a hover snaps. [`init`] sets
//! the flag from the system setting at launch.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::{
    px, Animation, AnimationElement, AnimationExt, App, ElementId, EntityId, Global, Hsla,
    IntoElement, Rgba, SharedString, Styled, Window,
};

use crate::theme;

// ---------------------------------------------------------------------------
// Cubic bezier
// ---------------------------------------------------------------------------

/// A CSS `cubic-bezier(x1, y1, x2, y2)` timing function, endpoints fixed at
/// (0,0) and (1,1). Solves x(t) = input by Newton iteration with a bisection
/// fallback.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl CubicBezier {
    pub const fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    /// A curve from its `theme.rs` token, `[x1, y1, x2, y2]`.
    pub const fn from_points(points: [f32; 4]) -> Self {
        Self::new(points[0], points[1], points[2], points[3])
    }

    fn coefficients(a: f32, b: f32) -> (f32, f32, f32) {
        let c = 3.0 * a;
        let bb = 3.0 * (b - a) - c;
        let aa = 1.0 - c - bb;
        (aa, bb, c)
    }

    fn sample_x(&self, t: f32) -> f32 {
        let (a, b, c) = Self::coefficients(self.x1, self.x2);
        ((a * t + b) * t + c) * t
    }

    fn sample_y(&self, t: f32) -> f32 {
        let (a, b, c) = Self::coefficients(self.y1, self.y2);
        ((a * t + b) * t + c) * t
    }

    fn sample_x_derivative(&self, t: f32) -> f32 {
        let (a, b, c) = Self::coefficients(self.x1, self.x2);
        (3.0 * a * t + 2.0 * b) * t + c
    }

    fn solve_t_for_x(&self, x: f32) -> f32 {
        let mut t = x;
        for _ in 0..8 {
            let err = self.sample_x(t) - x;
            if err.abs() < 1e-6 {
                return t;
            }
            let d = self.sample_x_derivative(t);
            if d.abs() < 1e-6 {
                break;
            }
            t -= err / d;
        }
        // x(t) is monotonic for every valid CSS bezier.
        let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
        for _ in 0..32 {
            let mid = (lo + hi) / 2.0;
            if self.sample_x(mid) < x {
                lo = mid
            } else {
                hi = mid
            }
        }
        (lo + hi) / 2.0
    }

    /// Eased output for input progress `x`, clamped to `[0, 1]` at both
    /// ends: f32 rounding can land a hair past 1.0 near the tail, and gpui
    /// asserts on an animation delta outside the unit interval.
    pub fn eval(&self, x: f32) -> f32 {
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        self.sample_y(self.solve_t_for_x(x)).clamp(0.0, 1.0)
    }
}

pub const EASE_OUT_EXPO: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE_OUT_EXPO);
pub const EASE_OUT: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE_OUT);
pub const EASE: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE);
pub const EASE_STANDARD: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE_STANDARD);
pub const EASE_ICON: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE_ICON);

// ---------------------------------------------------------------------------
// The catalog
// ---------------------------------------------------------------------------

/// One catalog entry: a duration and its curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionSpec {
    pub duration_ms: u64,
    pub curve: CubicBezier,
}

impl MotionSpec {
    pub const fn new(duration_ms: u64, curve: CubicBezier) -> Self {
        Self { duration_ms, curve }
    }

    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.duration_ms)
    }

    /// Eased progress for a raw timeline fraction. Pure.
    pub fn progress(&self, raw: f32) -> f32 {
        if self.duration_ms == 0 {
            return 1.0;
        }
        self.curve.eval(raw)
    }

    /// Eased progress `elapsed` into the timeline.
    pub fn progress_at(&self, elapsed: Duration) -> f32 {
        if self.duration_ms == 0 {
            return 1.0;
        }
        self.progress(elapsed.as_secs_f32() / self.duration().as_secs_f32())
    }

    /// This entry as a one-shot gpui [`Animation`]. gpui snaps it to its end
    /// state under reduced motion.
    pub fn animation(&self) -> Animation {
        let spec = *self;
        Animation::new(spec.duration()).with_easing(move |raw| spec.progress(raw))
    }
}

pub const FADE_IN: MotionSpec = MotionSpec::new(theme::MOTION_FADE_IN_MS, EASE_OUT_EXPO);
pub const FADE_QUICK: MotionSpec = MotionSpec::new(theme::MOTION_FADE_QUICK_MS, EASE);
pub const MENU_IN: MotionSpec = MotionSpec::new(theme::MOTION_MENU_IN_MS, EASE);
pub const MENU_OUT: MotionSpec = MotionSpec::new(theme::MOTION_MENU_OUT_MS, EASE);
pub const DIALOG_IN: MotionSpec = MotionSpec::new(theme::MOTION_DIALOG_IN_MS, EASE);
pub const RESIZE: MotionSpec = MotionSpec::new(theme::MOTION_RESIZE_MS, EASE_OUT);
#[allow(dead_code)] // after merge: Group and tool-group disclosures
pub const COLLAPSE: MotionSpec = MotionSpec::new(theme::MOTION_COLLAPSE_MS, EASE_OUT);
#[allow(dead_code)] // after merge: disclosure chevrons
pub const CHEVRON: MotionSpec = MotionSpec::new(theme::MOTION_CHEVRON_MS, EASE);
pub const HOVER_FADE: MotionSpec = MotionSpec::new(theme::MOTION_HOVER_FADE_MS, EASE_STANDARD);
pub const ICON_SWAP: MotionSpec = MotionSpec::new(theme::MOTION_ICON_SWAP_MS, EASE_ICON);

/// Linear interpolation.
pub fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

// ---------------------------------------------------------------------------
// Element helpers
// ---------------------------------------------------------------------------
//
// A rise or drop is a relative `top` inset: taffy applies relative insets
// after layout, so, like a CSS transform, siblings never move. Each helper
// plays once when its element id first mounts, so a caller mounts it only
// for a change the operator watched (never first paint, never scroll-back).

/// `fade-in`: opacity 0 → 1 rising `MOTION_FADE_IN_RISE` into place.
#[allow(dead_code)] // after merge: live-appended transcript blocks
pub fn fade_in<E>(id: impl Into<ElementId>, element: E) -> AnimationElement<E>
where
    E: Styled + IntoElement + 'static,
{
    element.with_animation(id, FADE_IN.animation(), |el, t| {
        el.relative()
            .opacity(t)
            .top(px(theme::MOTION_FADE_IN_RISE * (1.0 - t)))
    })
}

/// `fade-quick`: opacity only.
pub fn fade_quick<E>(id: impl Into<ElementId>, element: E) -> AnimationElement<E>
where
    E: Styled + IntoElement + 'static,
{
    element.with_animation(id, FADE_QUICK.animation(), |el, t| el.opacity(t))
}

/// `menu-in`: fades up from `MOTION_MENU_FROM_OPACITY` while dropping
/// `MOTION_MENU_SHIFT` from its opener.
#[allow(dead_code)] // after merge: menus and popovers
pub fn menu_in<E>(id: impl Into<ElementId>, element: E) -> AnimationElement<E>
where
    E: Styled + IntoElement + 'static,
{
    element.with_animation(id, MENU_IN.animation(), |el, t| {
        el.relative()
            .opacity(lerp(theme::MOTION_MENU_FROM_OPACITY, 1.0, t))
            .top(px(-theme::MOTION_MENU_SHIFT * (1.0 - t)))
    })
}

/// `dialog-in`: opacity 0 → 1 rising `MOTION_DIALOG_RISE`.
#[allow(dead_code)] // after merge: sheets
pub fn dialog_in<E>(id: impl Into<ElementId>, element: E) -> AnimationElement<E>
where
    E: Styled + IntoElement + 'static,
{
    element.with_animation(id, DIALOG_IN.animation(), |el, t| {
        el.relative()
            .opacity(t)
            .top(px(theme::MOTION_DIALOG_RISE * (1.0 - t)))
    })
}

// ---------------------------------------------------------------------------
// Tweens: interruptible width and height transitions
// ---------------------------------------------------------------------------

/// A value moving `from → to` along a spec, sampled at render time. Unlike a
/// `with_animation` keyed on the state, it is interruptible: a flip mid-flight
/// starts a new tween from the value on screen ([`Tween::retarget`]). The
/// owner reads [`Tween::running`] after rendering to keep frames coming.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tween {
    pub from: f32,
    pub to: f32,
    pub spec: MotionSpec,
    started: Instant,
}

impl Tween {
    pub fn new(from: f32, to: f32, spec: MotionSpec, now: Instant) -> Self {
        Self {
            from,
            to,
            spec,
            started: now,
        }
    }

    /// A tween toward `to` from wherever `previous` has the value now, or
    /// from `at_rest` when nothing was moving.
    pub fn retarget(
        previous: Option<Tween>,
        at_rest: f32,
        to: f32,
        spec: MotionSpec,
        now: Instant,
        reduced: bool,
    ) -> Self {
        let from = previous.map_or(at_rest, |tween| tween.value(now, reduced));
        Self::new(from, to, spec, now)
    }

    /// The value at `now`; exactly `to` once finished or under reduced motion.
    pub fn value(&self, now: Instant, reduced: bool) -> f32 {
        if !self.running(now, reduced) {
            return self.to;
        }
        let elapsed = now.saturating_duration_since(self.started);
        lerp(self.from, self.to, self.spec.progress_at(elapsed))
    }

    /// Mid-flight: the owner must ask for another frame.
    pub fn running(&self, now: Instant, reduced: bool) -> bool {
        !reduced
            && self.from != self.to
            && now.saturating_duration_since(self.started) < self.spec.duration()
    }
}

// ---------------------------------------------------------------------------
// Pulse clock: one throttled drive for every loop
// ---------------------------------------------------------------------------
//
// A `with_animation(...repeat())` loop asks for a frame on every display
// frame for as long as it is mounted: one working Thread held the whole
// cockpit at the display's refresh rate. The clock replaces that with one
// ~30fps tick. A loop reads its phase from [`pulse_phase`], which also
// leases the painting view onto the clock; each tick notifies the leased
// views, and a view that stops painting a loop stops renewing, lapses, and
// drops off. With no lease left the clock parks: no timer, no frame. All
// loops share one epoch, so two marks on screen stay phase-locked.

fn pulse_tick() -> Duration {
    Duration::from_millis(theme::MOTION_PULSE_TICK_MS)
}

fn pulse_lease() -> Duration {
    Duration::from_millis(theme::MOTION_PULSE_LEASE_MS)
}

/// The clock's bookkeeping, pure over an explicit `now` so the lease and
/// park rules are testable without a window.
#[derive(Debug, Default)]
struct PulseLeases {
    until: HashMap<EntityId, Instant>,
}

impl PulseLeases {
    fn renew(&mut self, view: EntityId, now: Instant) {
        self.until.insert(view, now + pulse_lease());
    }

    /// One tick: drop lapsed leases, then the views to notify — `None` when
    /// nothing is leased and the clock should park.
    fn tick(&mut self, now: Instant) -> Option<Vec<EntityId>> {
        self.until.retain(|_, until| *until > now);
        (!self.until.is_empty()).then(|| self.until.keys().copied().collect())
    }
}

struct PulseClock {
    epoch: Option<Instant>,
    leases: PulseLeases,
    running: bool,
}

impl Global for PulseClock {}

impl Default for PulseClock {
    fn default() -> Self {
        Self {
            epoch: None,
            leases: PulseLeases::default(),
            running: false,
        }
    }
}

/// The phase `[0, 1)` of a loop with this `period`, leasing `view` onto the
/// clock so it re-renders on the next tick. Call it only while painting the
/// loop. Reduced motion returns the loop's start (0) and leases nothing.
pub fn pulse_phase(period: Duration, view: EntityId, cx: &mut App) -> f32 {
    if reduced_motion(cx) || period.is_zero() {
        return 0.0;
    }
    let now = cx.background_executor().now();
    let clock = cx.default_global::<PulseClock>();
    let epoch = *clock.epoch.get_or_insert(now);
    clock.leases.renew(view, now);
    let start = !clock.running;
    clock.running = true;
    if start {
        cx.spawn(async move |cx| loop {
            cx.background_executor().timer(pulse_tick()).await;
            let parked = cx.update(|cx| {
                let now = cx.background_executor().now();
                let clock = cx.default_global::<PulseClock>();
                match clock.leases.tick(now) {
                    Some(views) => {
                        for view in views {
                            cx.notify(view);
                        }
                        false
                    }
                    None => {
                        clock.running = false;
                        true
                    }
                }
            });
            if parked {
                break;
            }
        })
        .detach();
    }
    let elapsed = now.saturating_duration_since(epoch);
    let period = period.as_nanos();
    (elapsed.as_nanos() % period) as f32 / period as f32
}

/// The clock is parked: no view holds a lease and no timer is armed.
pub fn pulse_parked(cx: &App) -> bool {
    cx.try_global::<PulseClock>()
        .is_none_or(|clock| !clock.running)
}

// ---------------------------------------------------------------------------
// Hover blend: `transition-colors` for gpui's snapping hover
// ---------------------------------------------------------------------------
//
// gpui's `.hover()` style applies the frame the pointer enters. The blend
// fades it over `HOVER_FADE` instead: a per-key progress, advanced from wall
// time wherever it is read, flipped by an `on_hover` listener, re-anchored at
// its current value when the pointer reverses mid-flight. The pressed face
// stays a gpui `.active()` refinement, which overrides the blended ground
// at once: a press never waits on the fade.
//
// The store is a main-thread `thread_local`, so row builders without a `cx`
// can blend. An element that unmounts mid-hover never hears its leave, so
// every read stamps the entry with the frame counter and
// [`hover_fades_active`] (once per frame, at the root view's tail) prunes
// an entry a full frame goes unread.

#[derive(Debug, Clone, Copy)]
struct FadeEntry {
    origin: f32,
    target: f32,
    started: Instant,
    seen: u64,
}

impl FadeEntry {
    fn value(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed >= HOVER_FADE.duration() {
            return self.target;
        }
        lerp(self.origin, self.target, HOVER_FADE.progress_at(elapsed))
    }

    fn settled(&self, now: Instant) -> bool {
        self.origin == self.target
            || now.saturating_duration_since(self.started) >= HOVER_FADE.duration()
    }
}

/// Per-key hover progress. Pure over an explicit `now`.
#[derive(Default)]
pub struct HoverFades {
    entries: HashMap<SharedString, FadeEntry>,
    frame: u64,
}

impl HoverFades {
    /// The pointer entered (`hovered`) or left the element behind `key`.
    /// Reduced motion snaps to the endpoint.
    pub fn set_at(&mut self, key: &SharedString, hovered: bool, reduced: bool, now: Instant) {
        let target = if hovered { 1.0 } else { 0.0 };
        let Some(current) = self.entries.get(key).map(|entry| entry.value(now)) else {
            if hovered {
                let origin = if reduced { target } else { 0.0 };
                self.insert(key, origin, target, now);
            }
            // A leave for a key never entered: nothing to fade.
            return;
        };
        let origin = if reduced { target } else { current };
        self.insert(key, origin, target, now);
    }

    fn insert(&mut self, key: &SharedString, origin: f32, target: f32, now: Instant) {
        let entry = FadeEntry {
            origin,
            target,
            started: now,
            seen: self.frame,
        };
        self.entries.insert(key.clone(), entry);
    }

    /// Hover progress (0..1) for `key` at `now`; stamps it as mounted.
    pub fn value_at(&mut self, key: &str, now: Instant) -> f32 {
        let frame = self.frame;
        self.entries.get_mut(key).map_or(0.0, |entry| {
            entry.seen = frame;
            entry.value(now)
        })
    }

    /// Once per frame: advance the counter, drop entries at rest or unread
    /// for a whole frame, and say whether a fade is still mid-flight.
    pub fn tick_at(&mut self, now: Instant) -> bool {
        self.frame += 1;
        let frame = self.frame;
        let mut active = false;
        self.entries.retain(|_, entry| {
            if entry.seen + 1 < frame {
                return false;
            }
            let settled = entry.settled(now);
            active |= !settled;
            !(settled && entry.target == 0.0)
        });
        active
    }
}

thread_local! {
    static HOVER_FADES: RefCell<HoverFades> = RefCell::new(HoverFades::default());
}

/// Hover progress (0..1) for `key` this frame.
pub fn hover_t(key: &str) -> f32 {
    HOVER_FADES.with(|fades| fades.borrow_mut().value_at(key, Instant::now()))
}

/// An `on_hover` listener driving the blend for `key`: pair it with a
/// [`hover_blend`] read of the same key on the same element.
pub fn hover_listener(key: SharedString) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    move |hovered, window, cx| {
        let reduced = reduced_motion(cx);
        HOVER_FADES.with(|fades| {
            fades
                .borrow_mut()
                .set_at(&key, *hovered, reduced, Instant::now())
        });
        // Dispatch runs outside any view's draw, so `request_animation_frame`
        // cannot name a view here: refresh (what a gpui `.hover()` style does
        // on the same event), and the root's tail keeps frames coming.
        window.refresh();
    }
}

/// The frame hook: call once per window frame, at the root view's render
/// tail. True while a blend is mid-flight and frames must keep coming.
pub fn hover_fades_active() -> bool {
    HOVER_FADES.with(|fades| fades.borrow_mut().tick_at(Instant::now()))
}

/// Blend two colours the way a browser transitions them: sRGB components
/// with premultiplied alpha, so a wash fading in from transparent brightens
/// without passing through grey.
pub fn mix(from: Hsla, to: Hsla, t: f32) -> Hsla {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return from;
    }
    if t >= 1.0 {
        return to;
    }
    let (f, g) = (Rgba::from(from), Rgba::from(to));
    let a = lerp(f.a, g.a, t);
    if a <= f32::EPSILON {
        return Hsla::from(Rgba { a: 0.0, ..g });
    }
    Hsla::from(Rgba {
        r: lerp(f.r * f.a, g.r * g.a, t) / a,
        g: lerp(f.g * f.a, g.g * g.a, t) / a,
        b: lerp(f.b * f.a, g.b * g.a, t) / a,
        a,
    })
}

/// The rest → hover colour at `key`'s current progress.
pub fn hover_blend(key: &str, rest: Hsla, hover: Hsla) -> Hsla {
    mix(rest, hover, hover_t(key))
}

// ---------------------------------------------------------------------------
// Reduced motion
// ---------------------------------------------------------------------------

/// The one reduced-motion switch: gpui's flag, which every `with_animation`
/// and kit spring already honours.
pub fn reduced_motion(cx: &App) -> bool {
    cx.reduce_motion()
}

/// Launch: adopt the system's Reduce Motion setting. gpui reads no platform
/// setting itself, so without this the flag is never set. Read off the main
/// thread; `FERRITE_REDUCE_MOTION=1` (or `0`) overrides it.
pub fn init(cx: &mut App) {
    if let Some(forced) = std::env::var("FERRITE_REDUCE_MOTION")
        .ok()
        .map(|value| value.trim() == "1")
    {
        cx.set_reduce_motion(forced);
        return;
    }
    if !cfg!(target_os = "macos") {
        return;
    }
    let read = cx.background_executor().spawn(async {
        std::process::Command::new("defaults")
            .args(["read", "com.apple.universalaccess", "reduceMotion"])
            .output()
            .is_ok_and(|out| out.status.success() && out.stdout.trim_ascii() == b"1")
    });
    cx.spawn(async move |cx| {
        if read.await {
            cx.update(|cx| cx.set_reduce_motion(true));
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{div, prelude::*, Context, TestAppContext};

    fn close(actual: f32, expected: f32, tol: f32, what: &str) {
        assert!(
            (actual - expected).abs() <= tol,
            "{what}: got {actual}, expected {expected} ±{tol}"
        );
    }

    const CURVES: [CubicBezier; 5] = [EASE_OUT_EXPO, EASE_OUT, EASE, EASE_STANDARD, EASE_ICON];

    #[test]
    fn a_linear_bezier_is_the_identity() {
        let linear = CubicBezier::new(0.0, 0.0, 1.0, 1.0);
        for x in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            close(linear.eval(x), x, 1e-4, "linear");
        }
    }

    /// Reference values computed independently with 80-step bisection
    /// (Zeron's table).
    #[test]
    fn the_catalog_curves_hit_their_css_values() {
        let cases: [(&str, CubicBezier, [f32; 5]); 3] = [
            (
                "expo",
                EASE_OUT_EXPO,
                [0.494391, 0.825622, 0.971779, 0.997677, 0.999878],
            ),
            (
                "ease-out",
                EASE_OUT,
                [0.160572, 0.378138, 0.684643, 0.906535, 0.982973],
            ),
            (
                "ease",
                EASE,
                [0.094796, 0.408511, 0.802403, 0.960459, 0.994316],
            ),
        ];
        for (name, curve, expected) in cases {
            for (x, want) in [0.1, 0.25, 0.5, 0.75, 0.9].into_iter().zip(expected) {
                close(curve.eval(x), want, 1e-3, name);
            }
        }
    }

    #[test]
    fn every_curve_is_monotonic_and_never_leaves_the_unit_interval() {
        for curve in CURVES {
            assert_eq!(curve.eval(0.0), 0.0);
            assert_eq!(curve.eval(1.0), 1.0);
            assert_eq!(curve.eval(-0.5), 0.0);
            assert_eq!(curve.eval(1.5), 1.0);
            let mut last = 0.0;
            for i in 0..=20_000u32 {
                let y = curve.eval(i as f32 / 20_000.0);
                assert!((0.0..=1.0).contains(&y), "{curve:?} escaped at {i}: {y}");
                assert!(y >= last - 1e-4, "{curve:?} not monotonic at {i}");
                last = y;
            }
            for x in [0.999_999f32, 0.999_999_9, 1.0 - f32::EPSILON] {
                assert!((0.0..=1.0).contains(&curve.eval(x)));
            }
        }
    }

    #[test]
    fn the_catalog_keeps_zerons_numbers() {
        assert_eq!(FADE_IN.duration_ms, 500);
        assert_eq!(FADE_IN.curve, CubicBezier::new(0.16, 1.0, 0.3, 1.0));
        assert_eq!(theme::MOTION_FADE_IN_RISE, 4.0);
        assert_eq!(FADE_QUICK.duration_ms, 150);
        assert_eq!(MENU_IN.duration_ms, 140);
        assert_eq!(DIALOG_IN.duration_ms, 180);
        assert_eq!(RESIZE.duration_ms, 200);
        assert_eq!(RESIZE.curve, CubicBezier::new(0.0, 0.0, 0.58, 1.0));
        assert_eq!(HOVER_FADE.duration_ms, 150);
        assert_eq!(HOVER_FADE.curve, CubicBezier::new(0.4, 0.0, 0.2, 1.0));
        assert_eq!(ICON_SWAP.curve, CubicBezier::new(0.2, 0.0, 0.0, 1.0));
        assert_eq!(theme::MOTION_ICON_SWAP_SCALE, 0.25);
        // Exits are softer than entrances.
        assert!(MENU_OUT.duration_ms < MENU_IN.duration_ms);
    }

    #[test]
    fn a_tween_eases_to_its_target_and_retargets_from_where_it_is() {
        let t0 = Instant::now();
        let ms = |m: u64| t0 + Duration::from_millis(m);
        let open = Tween::new(52.0, 286.0, RESIZE, t0);
        assert_eq!(open.value(t0, false), 52.0);
        assert!(open.running(ms(100), false));
        let mid = open.value(ms(100), false);
        assert!(mid > 52.0 && mid < 286.0, "mid-flight {mid}");
        assert_eq!(open.value(ms(200), false), 286.0);
        assert!(!open.running(ms(200), false), "done at the spec's end");

        // Flipped back mid-flight: the new tween starts on screen, not at
        // the far end.
        let back = Tween::retarget(Some(open), 286.0, 52.0, RESIZE, ms(100), false);
        close(back.from, mid, 1e-3, "continuity");
        assert_eq!(back.value(ms(300), false), 52.0);

        // Nothing moving: from the resting value.
        let rest = Tween::retarget(None, 286.0, 52.0, RESIZE, t0, false);
        assert_eq!(rest.from, 286.0);
    }

    #[test]
    fn reduced_motion_snaps_a_tween_to_its_end_state() {
        let t0 = Instant::now();
        let tween = Tween::new(52.0, 286.0, RESIZE, t0);
        assert_eq!(tween.value(t0, true), 286.0);
        assert!(!tween.running(t0, true), "no frame is asked for");
    }

    #[test]
    fn the_hover_blend_ramps_and_reverses_without_a_jump() {
        let mut fades = HoverFades::default();
        let key = SharedString::from("row");
        let t0 = Instant::now();
        let ms = |m: u64| t0 + Duration::from_millis(m);

        fades.set_at(&key, true, false, t0);
        assert_eq!(fades.value_at("row", t0), 0.0);
        let rising = fades.value_at("row", ms(75));
        assert!(rising > 0.0 && rising < 1.0, "mid-flight {rising}");
        assert_eq!(fades.value_at("row", ms(150)), 1.0);

        fades.set_at(&key, true, false, t0);
        let at_flip = fades.value_at("row", ms(75));
        fades.set_at(&key, false, false, ms(75));
        close(fades.value_at("row", ms(75)), at_flip, 1e-4, "continuity");
        assert!(fades.value_at("row", ms(140)) < at_flip, "falls back");
        assert_eq!(fades.value_at("row", ms(225)), 0.0, "lands at rest");
    }

    #[test]
    fn reduced_motion_snaps_the_hover_blend() {
        let mut fades = HoverFades::default();
        let key = SharedString::from("row");
        let t0 = Instant::now();
        fades.set_at(&key, true, true, t0);
        assert_eq!(fades.value_at("row", t0), 1.0, "enter snaps");
        assert!(!fades.tick_at(t0), "and asks for no frame");
        fades.set_at(&key, false, true, t0);
        assert_eq!(fades.value_at("row", t0), 0.0, "leave snaps");
    }

    #[test]
    fn the_hover_tick_reports_flight_and_prunes_what_is_gone() {
        let mut fades = HoverFades::default();
        let key = SharedString::from("a");
        let t0 = Instant::now();
        let ms = |m: u64| t0 + Duration::from_millis(m);

        fades.set_at(&SharedString::from("ghost"), false, false, t0);
        assert!(
            fades.entries.is_empty(),
            "a leave without an enter is inert"
        );

        fades.set_at(&key, true, false, t0);
        fades.value_at("a", ms(50));
        assert!(fades.tick_at(ms(50)), "mid-flight keeps frames coming");
        fades.value_at("a", ms(200));
        assert!(!fades.tick_at(ms(200)), "settled hovered: no frame");
        assert_eq!(fades.entries.len(), 1, "a hovered entry is kept");

        fades.set_at(&key, false, false, ms(250));
        fades.value_at("a", ms(450));
        assert!(!fades.tick_at(ms(450)));
        assert!(fades.entries.is_empty(), "rest entries are pruned");

        // Unmounted mid-hover: its leave never comes; one unread frame
        // drops it.
        fades.set_at(&key, true, false, ms(500));
        fades.tick_at(ms(516));
        fades.tick_at(ms(532));
        assert!(fades.entries.is_empty(), "unread entry evicted");
    }

    #[test]
    fn mix_blends_in_premultiplied_srgb() {
        let rest: Hsla = gpui::rgb(theme::GROUND).into();
        let hover: Hsla = gpui::rgb(theme::HOVER).into();
        assert_eq!(mix(rest, hover, 0.0), rest);
        assert_eq!(mix(rest, hover, 1.0), hover);
        let wash = gpui::transparent_black();
        let half = mix(wash, hover, 0.5);
        close(half.a, 0.5, 1e-4, "alpha ramps");
        let (h, half) = (Rgba::from(hover), Rgba::from(half));
        close(half.r, h.r, 1e-3, "the wash keeps its colour");
    }

    #[test]
    fn a_lease_lapses_unless_renewed_and_an_empty_clock_parks() {
        let mut leases = PulseLeases::default();
        let view = EntityId::from(1u64);
        let t0 = Instant::now();
        let ms = |m: u64| t0 + Duration::from_millis(m);
        assert_eq!(leases.tick(t0), None, "nothing leased: park");
        leases.renew(view, t0);
        assert_eq!(leases.tick(ms(33)), Some(vec![view]));
        leases.renew(view, ms(200));
        assert_eq!(leases.tick(ms(450)), Some(vec![view]), "renewed at 200");
        assert_eq!(leases.tick(ms(500)), None, "unpainted: lapses and parks");
    }

    struct Loop {
        painting: bool,
        renders: usize,
        phase: f32,
    }

    impl Render for Loop {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.renders += 1;
            if self.painting {
                self.phase = pulse_phase(Duration::from_millis(1_000), cx.entity_id(), cx);
            }
            div().size_4()
        }
    }

    /// A mounted loop re-renders its view at the clock's ~30fps, not the
    /// display's rate; unmounted, its lease lapses, the clock parks, and
    /// the window is left with no timer and no frame.
    #[gpui::test]
    fn the_pulse_clock_ticks_while_leased_and_parks_when_unmounted(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| Loop {
            painting: true,
            renders: 0,
            phase: 0.0,
        });
        cx.run_until_parked();
        assert!(
            !cx.update(|_, cx| pulse_parked(cx)),
            "a lease starts the clock"
        );
        let before = view.read_with(cx, |view, _| view.renders);
        for _ in 0..10 {
            cx.executor().advance_clock(pulse_tick());
            cx.run_until_parked();
        }
        let after = view.read_with(cx, |view, _| view.renders);
        assert_eq!(after - before, 10, "one render per tick");
        let phase = view.read_with(cx, |view, _| view.phase);
        assert!(phase > 0.0 && phase < 1.0, "the phase advances: {phase}");
        assert_eq!(
            cx.update(|window, cx| window.simulate_next_frame(cx)),
            0,
            "the clock asks for no display frame of its own"
        );

        view.update(cx, |view, cx| {
            view.painting = false;
            cx.notify();
        });
        cx.run_until_parked();
        cx.executor()
            .advance_clock(pulse_lease() + pulse_tick() * 2);
        cx.run_until_parked();
        assert!(
            cx.update(|_, cx| pulse_parked(cx)),
            "the lapsed clock parks"
        );
        let parked = view.read_with(cx, |view, _| view.renders);
        cx.executor().advance_clock(Duration::from_secs(2));
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |view, _| view.renders),
            parked,
            "and stays quiet"
        );
    }

    /// Reduced motion holds a loop at its start and leases nothing.
    #[gpui::test]
    fn reduced_motion_holds_a_loop_at_its_start(cx: &mut TestAppContext) {
        cx.update(|cx| cx.set_reduce_motion(true));
        let (view, cx) = cx.add_window_view(|_, _| Loop {
            painting: true,
            renders: 0,
            phase: 0.5,
        });
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |view, _| view.phase), 0.0);
        assert!(cx.update(|_, cx| pulse_parked(cx)), "nothing is scheduled");
    }
}
