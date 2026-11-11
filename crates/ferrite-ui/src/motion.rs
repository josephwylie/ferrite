//! The motion kit: Claude Desktop's motion, read from its source,
//! as one-shot elements on
//! the executor's clock ([`Played`]), plus the clocks gpui does not give
//! us — a throttled, shared pulse for loops, a hover blend for colour, and
//! interruptible tweens. Every clock asks for its frames through the view
//! that redraws it most cheaply ([`frames_through`]). The machinery was
//! ported from Zeron (MIT, (c) 2026 Wing). The numbers are tokens in
//! `theme.rs` (the motion section), which also holds the rules; this module
//! is the machinery.
//!
//! # Catalog
//!
//! Each animation, its spec, and the Ferrite surface it applies to.
//!
//! | Motion | Spec | Ferrite surface |
//! | --- | --- | --- |
//! | button hover | [`HOVER_FADE`] 60ms, the quart | a button's ground (`components::faded_button`, the pointer roles' controls, a board seam's line, the transcript's links and jump button): it blends in and out ([`hover_listener`]) |
//! | button press | [`PRESS_IN`] 60ms quart to 0.975 / [`PRESS_OUT`] 450ms overshoot | a kit button (`components::faded_button`) squishes while held and springs back ([`pressable`]) |
//! | row hover | [`HOVER_SNAP`], at once | a nav row, the sidebar's own buttons and headings, a menu row, a float or palette row ([`hover_snap`]), as Claude's rows do |
//! | selection move | none | the nav's one selection ground (`paint::SELECTED`) moves at once: selection is keyboard-rate |
//! | toasts | [`TOAST_IN`] 200ms quart, its own height / [`TOAST_OUT`] 200ms quart to 0.85 | the Needs-you toast (FL-18), hung top right under the window's head, slides out from under it ([`toast_in`]); one that goes on its own fades away shrinking, drawn as a face that answers no pointer ([`toast_out`], [`Exits`]). The kit's toasts (the Server and update toasts) move the same way on the same two tokens (`DefaultToastMotion`; the card through Ferrite's patch to the vendored gpui-component) |
//! | loops | pulse clock / display | the Composer's caret blink and the spinners ride [`loop_phase`] (one timer on a ~30fps grid, woken only when a loop's picture changes, parks) instead of a per-frame repeat; the working marks (the working line, nav, Pane heads, plan steps), whose orbs move at every instant, and the working line's shimmer, whose crest does, ride [`frame_loop`]: the display's rate, one request a frame for the window, only while one can be seen; no dot breathes |
//! | menus, popovers | [`MENU_IN`] 0ms: a snap | every Ferrite-drawn menu and popover, open and shut ([`menu_in`]), as Claude's `Menu`, `ContextMenu` and `Popover` |
//! | dialog | [`DIALOG_IN`] / [`DIALOG_OUT`] 200ms quart, 0.98 ↔ 1 | the Settings and Project sheets, in ([`dialog_in`]) and out ([`dialog_out`]), over their veil fading both ways over [`VEIL`] ([`veil_in`], [`veil_out`]) |
//! | palette | [`DIALOG_IN`] in from its top, [`PALETTE_OUT`] 120ms out | the palette and the shortcuts sheet ([`dialog_in_casting`], [`float_out`]), their veil fading over [`PALETTE_VEIL`] 50ms both ways |
//! | tooltip | [`TOOLTIP_IN`] 120ms ease-snap, 0.98 → 1 | every tooltip (`menu::tooltip`, [`tooltip_in`]) after gpui's 300ms delay; the next one within 200ms shows at once, with no entrance. No exit: gpui drops a tooltip on its frame |
//! | sidebar (cmd-B) | none | the column snaps between its width and the rail: no ride, no fade, no per-frame relayout of the board |
//! | nav card recede | [`RECEDE`] 150ms standard, opacity only | a receded working Thread card (70%) comes up to full under the pointer or the nav's keyboard cursor and goes back as it leaves; its fill and its hover strip snap; reduced motion snaps it too |
//! | nav folds | [`FOLD`] 150ms `ease-out`, height and opacity | a Project section, a Group's members and Pinned open and shut ([`fold_body`], one [`Reveal`] per fold); only the nav part redraws |
//! | right panel | none | the Panel (`cockpit/panel.rs`) snaps open and shut and between its own width and a preview's, as Claude's does |
//! | (Ferrite's) slot slide | [`SLOT_OPEN`] 240ms / [`SLOT_CLOSE`] 200ms, [`EASE_SNAP`] | the Review beside its Pane, in the reader slot (`cockpit/beside.rs`): a [`Reveal`] on its split's share (`Tree::reveal`) slides it in from its far edge while the Pane narrows, every frame, its content laid out at its resting size and clipped so it never re-wraps; closing slides it out, a little quicker, and only then is the slot removed; a flip mid-slide turns around; a seam drag lands it |
//! | zoom | [`IMAGE_ZOOM`] 240ms | the image viewer's zoom (`image_viewer.rs`): a click between fit and actual size and ⌘= ⌘- ⌘0 ease the picture's scale with an interruptible [`Tween`] on its logarithm, the point under the pointer held still; a pinch and a pan follow the hand with no ease |
//! | image in | [`FADE_QUICK`] 150ms | an image that lands in the viewer after its preview is up fades in; one replacing another crossfades with it (opacity only) |
//! | chevron rotate | [`CHEVRON`] 60ms | every disclosure chevron (the transcript's, the nav's, the Panel's folds) turns its quarter as its fold flips ([`settled`]); the folds themselves snap, as Claude's tool steps do |
//! | switch | [`SWITCH`] 120ms overshoot | a Settings on/off switch (`components::switch`): its knob slides, its track snaps |
//! | stream fade | [`STREAM_FADE`] 400ms linear, opacity only | every transcript row appended at the tail while the operator watches, and each run of words streaming into an answer (`TextView::fresh`); a row's clock starts on the frame that first draws it, and a row that arrives whole plays its whole fade; at turn end a streamed run with more than [`STREAM_LAND`] left lands within it; never first paint, a history window growing at its head, or scroll-back |
//! | bubble pop | [`BUBBLE_IN`] 450ms overshoot | the operator's prompt bubble appended at the tail: opacity, scale 0.92 → 1 from its right edge, a 6px rise ([`bubble_face`]) |
//! | approval promote | [`PROMOTE_IN`] 220ms ease-out cubic | the approval card: from 0.75, 6px above, scale 0.97 from its top ([`promote_in`]) |
//! | label roll | [`MORPH`] 180ms | the working line's words when they change ([`morph`]) |
//! | count tick | [`COUNT`] 120ms quart | the working line's token count ticks to each new reading: only the cells that change cross-fade, drifting `MOTION_COUNT_DRIFT` (2pt) up, tabular so nothing shifts; a new turn's count lands; drawn by the loops overlay, so only the count redraws (`odometer::rolling_count`) |
//! | message actions | [`ACTIONS_IN`] 120ms after 100ms / [`ACTIONS_OUT`] 60ms, ease-snap | a message's actions under the pointer ([`hover_listener_with`]) |
//! | send scroll | `MOTION_SCROLL_MAX_MS` 200ms at most, [`EASE_STANDARD`] | a send's prompt eases up to the top of the transcript (`TX_SEND_ANCHOR_OFFSET` below it) for Chromium's smooth-scroll length, `min(√Δpx, 12) / 60` s, re-measured each frame so rows growing under it are absorbed; any scroll by the operator stops it where it stands; the bubble's pop starts when it lands (`transcript/scroll.rs`) |
//! | scroll to end | shows `SCROLL_END_SHOW_DELAY_MS` 150ms after it is wanted, hides at once; its click rides the send scroll's ease to the tail | the transcript's Scroll to end pill: no fade either way, its ring blending over [`HOVER_FADE`] |
//! | chip in | [`CHIP_IN`] 300ms ease-snap, 0.95 → 1 about its centre | an attachment added to a draft's tray ([`chip_in`]); never what the tray held when it was first drawn, nor a sent prompt's |
//! | find bar | [`FADE_QUICK`] 150ms, opacity only | the transcript's find bar fades in ([`fade_in`]); it goes at once |
//! | notices | [`FADE_QUICK`] 150ms, opacity only | a notice over or in the Composer (a draft's failed start) fades in ([`fade_in`]); it goes at once. The approval card keeps [`PROMOTE_IN`], the queue strip none |
//! | usage ring | [`USAGE_ARC`] 300ms / [`USAGE_INK`] 200ms, quart | the footer's plan-usage ring moving to a new reading ([`settle`]); first paint lands |
//! | row-in | [`ROW_IN`] 180ms, opacity only | a preview opening in the Panel, or another document replacing it, and a Panel tab's body when the operator picks the tab, from 0.3 ([`swap_in`]) |
//! | scrollbar | `MOTION_SCROLLBAR_LINGER_MS` 1.4s + [`SCROLLBAR_FADE`] 100ms | a thumb appears on the first scroll frame, holds 1.4s, then fades, and widens 4 → 6px under the pointer (`scrollbar::step_widen`); idle, nothing is drawn |
//!
//! Kit-drawn surfaces (gpui-kit's `PopupMenu`, its dialogs) keep the kit's
//! own behaviour. A tool group's disclosure in the transcript opens without
//! a height tween, as Claude's tool steps do: its rows are one item of a
//! virtualized list inside a kit `Collapsible`, and an item mid-reveal
//! would clip rows the pointer can already reach.
//!
//! **What stays still** (T3 Code leaves these unanimated; do not add
//! motion to them): menus, context menus and the `/` and `@` menus
//! (`MENU_IN` 0ms); the Composer's height after a send clears it; the
//! scroll on a send's release to the tail, on Composer or footer growth
//! while following, on idle tail growth, on the anchor holding its place
//! as its own row resizes, and on restoring a thread's position; Scroll to
//! end's show and hide; a tool group's body (only its chevron turns);
//! streamed text's place (it fades, it never slides or scales); and the
//! sidebar's, the Panel's and the terminal drawer's widths.
//!
//! **What a frame costs.** gpui asks for an animation's next frame by
//! notifying the view drawing it, and redraws that view's ancestors with
//! it. A one-shot, a settled flip and a hover blend in the Cockpit's own
//! tree (its floats, the frame's controls) notify its `FrameTick` instead
//! ([`draw_frames_through`]), so they rebuild no cached part; one inside a
//! part (a Pane's Composer menu, a nav fold) rebuilds that part alone. A
//! loop draws in the loops overlay (ADR 0011). The Panel's ride and a
//! Review's slide redraw the frame around the parts instead, so only the
//! parts whose cells move are built again.
//!
//! # What gpui here cannot do, and what stands in
//!
//! Zeron runs a gpui fork; Ferrite runs gpui-kit 0.6 on gpui-pre. Dropped or
//! approximated:
//!
//! - **Scale is painted only.** `Styled::scale` (Ferrite's gpui patch,
//!   `Window::with_element_scale`) scales what an element paints about its
//!   centre, or about `Styled::scale_origin` (a CSS `transform-origin`).
//!   Its layout and hit area stay at rest, and a `deferred` draw or a loop
//!   hosted in the loops overlay inside it is not scaled.
//! - **Blur.** No filter on elements, so the icon swap's `blur(4px)` is
//!   dropped; opacity and scale carry it.
//! - **Springs.** Claude's button release (`--cds-btn-spring`, a
//!   piecewise-linear spring) rides `EASE_OVERSHOOT` over 450ms.
//! - **Frost.** A fork-only paint effect; not ported. An edge fade is a
//!   `linear_gradient` overlay in the surface's own fill (the transcript's
//!   top fade, `EDGE_FADE`).
//!
//! # Reduced motion
//!
//! gpui's `App::reduce_motion` flag snaps every `with_animation` element (a
//! one-shot to its end state, a loop to its start) and every kit spring. The
//! clocks here follow the same rule, as Claude's `prefers-reduced-motion`
//! does: a [`Played`] one-shot lands on its first frame (an exit is not
//! drawn at all), [`loop_phase`] returns 0 and declares nothing, a [`Tween`]
//! reads its target, a hover snaps, a press does not squish and a label
//! swaps its words at once. [`init`] sets the flag from the system setting
//! at launch.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ferrite_core::cadence;
use gpui::{
    div, prelude::FluentBuilder, px, AnyElement, App, ElementId, EntityId, Global, Hsla,
    InteractiveElement, IntoElement, ParentElement, Rgba, SharedString, Styled, Window,
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

    /// Eased output for input progress `x`, `0` and `1` at the ends. Between
    /// them it is clamped to the curve's own reach: `[0, 1]` for a curve
    /// that stays inside it (f32 rounding can land a hair past 1.0 near the
    /// tail), past 1 for one that overshoots (`EASE_OVERSHOOT`, whose y
    /// rises over 1 before it settles). No caller hands an overshooting
    /// curve to gpui's `with_animation`, which asserts on a delta outside
    /// the unit interval.
    pub fn eval(&self, x: f32) -> f32 {
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        let low = self.y1.min(self.y2).min(0.0);
        let high = self.y1.max(self.y2).max(1.0);
        self.sample_y(self.solve_t_for_x(x)).clamp(low, high)
    }
}

pub const EASE_OUT_EXPO: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE_OUT_EXPO);
pub const EASE: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE);
pub const EASE_OUT: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE_OUT);
pub const EASE_STANDARD: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE_STANDARD);
pub const EASE_OUT_QUART: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE_OUT_QUART);
pub const EASE_SNAP: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE_SNAP);
pub const EASE_OVERSHOOT: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE_OVERSHOOT);
pub const EASE_MORPH: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE_MORPH);
pub const EASE_OUT_CUBIC: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE_OUT_CUBIC);
pub const LINEAR: CubicBezier = CubicBezier::from_points(theme::MOTION_EASE_LINEAR);

// ---------------------------------------------------------------------------
// The catalog
// ---------------------------------------------------------------------------

/// One catalog entry: a duration and its curve, after an optional delay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionSpec {
    pub duration_ms: u64,
    pub curve: CubicBezier,
    /// How long it holds at its start before it moves (a message's actions
    /// wait under the pointer before they show).
    pub delay_ms: u64,
}

impl MotionSpec {
    pub const fn new(duration_ms: u64, curve: CubicBezier) -> Self {
        Self {
            duration_ms,
            curve,
            delay_ms: 0,
        }
    }

    /// The same, held at its start for `delay_ms` first.
    pub const fn after(self, delay_ms: u64) -> Self {
        Self { delay_ms, ..self }
    }

    /// How long it takes to land, its delay included.
    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.delay_ms + self.duration_ms)
    }

    /// Eased progress for a raw timeline fraction (of the move, after the
    /// delay). Pure.
    pub fn progress(&self, raw: f32) -> f32 {
        if self.duration_ms == 0 {
            return 1.0;
        }
        self.curve.eval(raw)
    }

    /// Eased progress `elapsed` into the timeline, its delay first.
    pub fn progress_at(&self, elapsed: Duration) -> f32 {
        if self.duration_ms == 0 {
            return if elapsed >= self.duration() { 1.0 } else { 0.0 };
        }
        let moving = elapsed.saturating_sub(Duration::from_millis(self.delay_ms));
        self.progress(moving.as_secs_f32() / (self.duration_ms as f32 / 1000.0))
    }
}

const fn spec(duration_ms: u64, curve: CubicBezier) -> MotionSpec {
    MotionSpec::new(duration_ms, curve)
}

pub const ROW_IN: MotionSpec = spec(theme::MOTION_ROW_IN_MS, EASE_OUT_EXPO);
/// Streamed text, by the run: Claude's 400ms linear `delta-fade`.
pub const STREAM_FADE: MotionSpec = spec(theme::MOTION_STREAM_FADE_MS, LINEAR);
/// How soon a run still fading lands once its turn ends.
pub const STREAM_LAND: Duration = Duration::from_millis(theme::MOTION_STREAM_LAND_MS);
/// The sent bubble's pop (`bubble_face`).
pub const BUBBLE_IN: MotionSpec = spec(theme::MOTION_BUBBLE_FADE_MS, EASE_OVERSHOOT);
pub const FADE_QUICK: MotionSpec = spec(theme::MOTION_FADE_QUICK_MS, EASE);
pub const MENU_IN: MotionSpec = spec(theme::MOTION_MENU_IN_MS, EASE);
pub const DIALOG_IN: MotionSpec = spec(theme::MOTION_DIALOG_IN_MS, EASE_OUT_QUART);
pub const DIALOG_OUT: MotionSpec = spec(theme::MOTION_DIALOG_OUT_MS, EASE_OUT_QUART);
/// A sheet's veil, darkening in and lifting out with it.
pub const VEIL: MotionSpec = spec(theme::MOTION_DIALOG_IN_MS, EASE_OUT_QUART);
/// The palette's exit, quicker than its entrance, and its veil both ways.
pub const PALETTE_OUT: MotionSpec = spec(theme::MOTION_PALETTE_OUT_MS, EASE_OUT_QUART);
pub const PALETTE_VEIL: MotionSpec = spec(theme::MOTION_PALETTE_VEIL_MS, EASE_OUT_QUART);
pub const TOOLTIP_IN: MotionSpec = spec(theme::MOTION_TOOLTIP_IN_MS, EASE_SNAP);
/// The approval card settling into place (`promote_in`).
pub const PROMOTE_IN: MotionSpec = spec(theme::MOTION_PROMOTE_MS, EASE_OUT_CUBIC);
/// The working line's label roll (`morph`).
pub const MORPH: MotionSpec = spec(theme::MOTION_MORPH_MS, EASE_MORPH);
/// The working line's token count ticking to its next reading
/// (`odometer::rolling_count`).
pub const COUNT: MotionSpec = spec(theme::MOTION_COUNT_MS, EASE_OUT_QUART);
/// The image viewer's zoom, tweened on the scale's logarithm so every
/// step of it reads as the same zoom.
pub const IMAGE_ZOOM: MotionSpec = spec(theme::MOTION_IMAGE_ZOOM_MS, EASE_OUT_EXPO);
/// A button's hover blend (`hover_listener`).
pub const HOVER_FADE: MotionSpec = spec(theme::MOTION_HOVER_FADE_MS, EASE_OUT_QUART);
/// A row's hover: at once (`hover_snap`).
pub const HOVER_SNAP: MotionSpec = spec(0, EASE_OUT_QUART);
/// A button's press and release (`pressable`).
pub const PRESS_IN: MotionSpec = spec(theme::MOTION_DUR_FAST_MS, EASE_OUT_QUART);
pub const PRESS_OUT: MotionSpec = spec(theme::MOTION_DUR_SLOW_MS, EASE_OVERSHOOT);
/// A message's actions under the pointer: in after a beat, out at once.
pub const ACTIONS_IN: MotionSpec =
    spec(theme::MOTION_DUR_SNAP_MS, EASE_SNAP).after(theme::MOTION_ACTIONS_DELAY_MS);
pub const ACTIONS_OUT: MotionSpec = spec(theme::MOTION_DUR_FAST_MS, EASE_SNAP);
/// A disclosure chevron's quarter turn.
pub const CHEVRON: MotionSpec = spec(theme::MOTION_CHEVRON_MS, EASE_STANDARD);
/// A transcript work row's chevron (T3's `transition-transform
/// duration-200`): the nav's and the Panel's keep `CHEVRON`.
pub const WORK_CHEVRON: MotionSpec = spec(theme::MOTION_WORK_CHEVRON_MS, EASE_STANDARD);
/// A turn fold's words brightening under the pointer, both ways.
pub const FOLD_HOVER: MotionSpec = spec(theme::MOTION_FOLD_HOVER_MS, EASE_STANDARD);
/// A receded working nav card coming up to full opacity under the pointer.
pub const RECEDE: MotionSpec = spec(theme::MOTION_RECEDE_MS, EASE_STANDARD);
/// A nav fold opening and shutting (`fold_body`).
pub const FOLD: MotionSpec = spec(theme::MOTION_FOLD_MS, EASE_OUT);
pub const TAB_SCROLL: MotionSpec = spec(theme::MOTION_TAB_SCROLL_MS, EASE_STANDARD);
/// An attachment added to a draft growing in (`chip_in`).
pub const CHIP_IN: MotionSpec = spec(theme::MOTION_CHIP_IN_MS, EASE_SNAP);
/// The usage ring's arc and ink moving to a new reading.
pub const USAGE_ARC: MotionSpec = spec(theme::MOTION_USAGE_ARC_MS, EASE_OUT_QUART);
pub const USAGE_INK: MotionSpec = spec(theme::MOTION_USAGE_INK_MS, EASE_OUT_QUART);
/// A switch's knob.
pub const SWITCH: MotionSpec = spec(theme::MOTION_SWITCH_MS, EASE_OVERSHOOT);
/// The model picker rail's pill sliding between providers (T3's
/// `transition-[top] duration-200 ease-out`).
pub const RAIL_SLIDE: MotionSpec = spec(theme::MOTION_DUR_BASE_MS, EASE_OUT);
/// A scrollbar thumb fading out and widening.
pub const SCROLLBAR_FADE: MotionSpec = spec(theme::MOTION_SCROLLBAR_FADE_MS, EASE_STANDARD);
/// A board slot sliding in beside its owner, and out again a little
/// quicker (a [`Reveal`]): the Review beside its Pane.
pub const SLOT_OPEN: MotionSpec = spec(theme::MOTION_SLOT_OPEN_MS, EASE_SNAP);
pub const SLOT_CLOSE: MotionSpec = spec(theme::MOTION_SLOT_CLOSE_MS, EASE_SNAP);
// A slot's exit is quicker than its entrance.
const _: () = assert!(theme::MOTION_SLOT_CLOSE_MS < theme::MOTION_SLOT_OPEN_MS);

/// The lightbox's picture growing in from where it was opened.
pub const LIGHTBOX_GROW: MotionSpec = spec(theme::MOTION_DUR_BASE_MS, EASE_OUT_QUART);

/// The Composer's box easing to a new row count.
pub const COMPOSER_GROW: MotionSpec = spec(theme::MOTION_COMPOSER_GROW_MS, EASE_OUT_QUART);

/// A toast coming in under the window's head, and going when it goes on
/// its own: the toast tokens, on the quart.
pub const TOAST_IN: MotionSpec = spec(theme::MOTION_TOAST_IN_MS, EASE_OUT_QUART);
pub const TOAST_OUT: MotionSpec = spec(theme::MOTION_TOAST_OUT_MS, EASE_OUT_QUART);

/// Linear interpolation.
pub fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

/// Where a scale is about, as fractions of the element's box (gpui's
/// `scale_origin`, Ferrite's patch).
pub type ScaleFrom = (f32, f32);
pub const FROM_CENTRE: ScaleFrom = (0.5, 0.5);
pub const FROM_TOP: ScaleFrom = (0.5, 0.0);
pub const FROM_TOP_LEFT: ScaleFrom = (0.0, 0.0);
pub const FROM_RIGHT: ScaleFrom = (1.0, 0.5);

/// `el` painted at `scale` about `from`; nothing at 1.
fn scaled<E: Styled>(el: E, scale: f32, from: ScaleFrom) -> E {
    if scale == 1.0 {
        return el;
    }
    el.scale(scale).scale_origin(from.0, from.1)
}

// ---------------------------------------------------------------------------
// Element helpers
// ---------------------------------------------------------------------------
//
// A rise or drop is a relative `top` inset: taffy applies relative insets
// after layout, so, like a CSS transform, siblings never move. A scale is
// painted only (Ferrite's gpui patch). The entrances play once when their
// element id first mounts, so a caller mounts one only for a change the
// operator watched. A menu or a popover has no exit: it is gone at once, as
// Claude's are. A sheet, the palette and a toast leave as a face that
// answers no pointer ([`float_out`], [`dialog_out`], [`toast_out`]), drawn
// from the instant they went until their exit has run; the caller stops
// drawing it then ([`Exits`]).
//
// Each plays on the executor's clock ([`Played`]), not gpui's wall-clock
// `with_animation`, so a test steps it like every other clock here, and it
// asks for its frames through [`frames_through`]: a menu the Cockpit draws
// redraws the frame around the cached parts, never every Pane.
//
// gpui paints a surface's shadow under it and has no group compositing, so
// a surface still fading in would show its own shadow through it as a dark
// slab. The float faces ramp the shadow's ink by the square of the fade,
// which the element's own opacity takes to the cube (the kit's toasts do
// the same).

/// Which way a floating surface opens from its opener: it starts
/// `MOTION_MENU_SHIFT` back toward the opener and settles away from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opens {
    /// Below its opener (the nav's menus, the context menu, the bell).
    Down,
    /// Above its opener (the Composer's menus, the footer cards).
    Up,
    /// Where it stands, without settling: a float that measures its own
    /// box to place itself (the Composer's choice menus) would chase the
    /// shift and re-place itself every frame.
    InPlace,
}

impl Opens {
    fn shift(self, t: f32) -> f32 {
        let toward = match self {
            Opens::Down => -1.0,
            Opens::Up => 1.0,
            Opens::InPlace => 0.0,
        };
        toward * theme::MOTION_MENU_SHIFT * (1.0 - t)
    }
}

/// A floating surface at `opacity`, its float shadow ramped under it.
fn float_face<E: Styled>(el: E, opacity: f32) -> E {
    faded(el, opacity, crate::components::float_shadow())
}

/// A surface at `opacity` casting `shadow`, the shadow's ink ramped by the
/// square of the fade.
fn faded<E: Styled>(el: E, opacity: f32, shadow: Vec<gpui::BoxShadow>) -> E {
    let opacity = opacity.clamp(0.0, 1.0);
    el.opacity(opacity)
        .shadow(crate::components::shadow_faded(shadow, opacity * opacity))
}

/// A floating surface `t` of the way through growing to its size from
/// `MOTION_FLOAT_FROM_SCALE`, about `from`. The scale is painted only,
/// so a float that places itself by its own box (`Opens::InPlace`) holds
/// still, and it answers the pointer where it rests.
fn grown<E: Styled>(el: E, t: f32, from: ScaleFrom) -> E {
    scaled(el, lerp(theme::MOTION_FLOAT_FROM_SCALE, 1.0, t), from)
}

/// `menu-in` for a floating surface in flow (inside `anchored` or a
/// wrapper): fades up from `MOTION_MENU_FROM_OPACITY` and grows from
/// `MOTION_FLOAT_FROM_SCALE` while settling `MOTION_MENU_SHIFT` away from
/// its opener. At 0ms (`MOTION_MENU_IN_MS`, a snap) it is the element
/// itself: no entrance state, no face, no frame.
pub fn menu_in<E>(id: impl Into<ElementId>, element: E, opens: Opens) -> AnyElement
where
    E: Styled + IntoElement + 'static,
{
    if MENU_IN.duration_ms == 0 {
        return element.into_any_element();
    }
    entrance(id, MENU_IN, element, move |el, t| {
        grown(
            float_face(el, lerp(theme::MOTION_MENU_FROM_OPACITY, 1.0, t)),
            t,
            FROM_CENTRE,
        )
        .relative()
        .top(px(opens.shift(t)))
    })
    .into_any_element()
}

/// `dialog-in` for a sheet: opacity 0 → 1, growing from
/// `MOTION_FLOAT_FROM_SCALE` about its centre, over `DIALOG_IN`.
pub fn dialog_in<E>(id: impl Into<ElementId>, element: E) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    entrance(id, DIALOG_IN, element, |el, t| {
        grown(float_face(el, t), t, FROM_CENTRE)
            .relative()
            .top(px(theme::MOTION_DIALOG_RISE * (1.0 - t)))
    })
}

/// `dialog_in` for a sheet that casts its own `shadow` rather than the
/// float's and grows from `from` (the palette's dialog, from its top,
/// `components::dialog_shadow`).
pub fn dialog_in_casting<E>(
    id: impl Into<ElementId>,
    element: E,
    shadow: fn() -> Vec<gpui::BoxShadow>,
    from: ScaleFrom,
) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    entrance(id, DIALOG_IN, element, move |el, t| {
        grown(faded(el, t, shadow()), t, from)
            .relative()
            .top(px(theme::MOTION_DIALOG_RISE * (1.0 - t)))
    })
}

/// `dialog-out` for a sheet that went at `left`: it fades to 0 and shrinks
/// to `MOTION_FLOAT_FROM_SCALE` about its centre over `DIALOG_OUT`, its
/// float shadow ramped with it, under a cover that takes every press — a
/// face no pointer finds. The caller stops drawing it once `DIALOG_OUT`
/// has passed ([`Exits`]).
pub fn dialog_out<E>(left: Instant, element: E) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    unreachable_face(Played {
        origin: Origin::At(left),
        spec: DIALOG_OUT,
        element,
        face: |el: E, t: f32| grown(float_face(el, 1.0 - t), 1.0 - t, FROM_CENTRE),
    })
}

/// A whole floating view that went at `left` (the palette, the shortcuts
/// sheet): it fades to 0 and shrinks to `MOTION_FLOAT_FROM_SCALE` about
/// `from` over `spec`, under a cover that takes every press. Its own
/// shadow fades with it.
pub fn float_out(
    left: Instant,
    spec: MotionSpec,
    element: impl IntoElement,
    from: ScaleFrom,
) -> impl IntoElement {
    unreachable_face(Played {
        origin: Origin::At(left),
        spec,
        element: div().child(element),
        face: move |el: gpui::Div, t: f32| grown(el.opacity(1.0 - t), 1.0 - t, from),
    })
}

/// `face` under a cover laid over it last, so the topmost hitbox there is
/// the cover's: a press on a leaving face lands nowhere.
fn unreachable_face(face: impl IntoElement) -> impl IntoElement {
    div()
        .relative()
        .child(face)
        .child(div().absolute().inset_0().occlude())
}

/// `row-in` for content arriving in a place already open (a preview in the
/// Panel, another document replacing it): opacity alone, up from
/// `MOTION_MENU_FROM_OPACITY` so its first frame is already legible; the
/// place itself holds still.
pub fn swap_in<E>(id: impl Into<ElementId>, element: E) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    entrance(id, ROW_IN, element, |el, t| {
        el.opacity(lerp(theme::MOTION_MENU_FROM_OPACITY, 1.0, t))
    })
}

/// `toast-in` for a toast hung under the window's head: it fades up from
/// 0 while sliding its own height down out from under the edge it hangs
/// on, over `TOAST_IN` (Claude's toasts rise their own height from the
/// foot; Ferrite's hang at the head, so they drop). While it moves it is
/// clipped to its resting box, so it never draws over the head.
pub fn toast_in<E>(id: impl Into<ElementId>, element: E) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    ToastIn {
        id: id.into(),
        element,
    }
}

struct ToastIn<E> {
    id: ElementId,
    element: E,
}

impl<E: Styled + IntoElement + 'static> IntoElement for ToastIn<E> {
    type Element = gpui::ViewElement<Self>;

    #[track_caller]
    fn into_element(self) -> Self::Element {
        gpui::ViewElement::new(self)
    }
}

impl<E: Styled + IntoElement + 'static> gpui::RenderOnce for ToastIn<E> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = play(Origin::Mount(self.id), TOAST_IN, window, cx);
        // A relative inset resolves against the wrapper, which is the
        // toast's own height: -100% is one toast up.
        let toast = float_face(self.element, t)
            .relative()
            .top(gpui::relative(-(1.0 - t)));
        div()
            .when(t < 1.0, |clip| clip.overflow_hidden())
            .child(toast)
    }
}

/// `toast-out` for a toast that went on its own at `left`: it fades away
/// shrinking to `MOTION_TOAST_OUT_SCALE` over `TOAST_OUT`. The caller draws
/// it as a face that answers no pointer, and stops once [`TOAST_OUT`] has
/// passed.
pub fn toast_out<E>(left: Instant, element: E) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    Played {
        origin: Origin::At(left),
        spec: TOAST_OUT,
        element,
        face: |el: E, t: f32| {
            scaled(
                float_face(el, 1.0 - t),
                lerp(1.0, theme::MOTION_TOAST_OUT_SCALE, t),
                FROM_CENTRE,
            )
        },
    }
}

/// `fade-in` over `spec`: opacity 0 → `to`, nothing moves — a mark or a
/// chip arriving in a line that is already drawn.
pub fn fade_in<E>(
    id: impl Into<ElementId>,
    element: E,
    spec: MotionSpec,
    to: f32,
) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    entrance(id, spec, element, move |el, t| el.opacity(lerp(0.0, to, t)))
}

/// An attachment added to a draft (T3's tray, `@starting-style`): opacity
/// up and scale from `MOTION_CHIP_FROM_SCALE` about its centre over
/// [`CHIP_IN`].
pub fn chip_in<E>(id: impl Into<ElementId>, element: E) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    entrance(id, CHIP_IN, element, |el, t| {
        scaled(
            el.opacity(t),
            lerp(theme::MOTION_CHIP_FROM_SCALE, 1.0, t),
            FROM_CENTRE,
        )
    })
}

/// `code-approval-promote` for the approval card: from
/// `MOTION_PROMOTE_FROM_OPACITY`, `MOTION_PROMOTE_DROP` above its place
/// and `MOTION_PROMOTE_FROM_SCALE` about its top, it settles over
/// `PROMOTE_IN`.
pub fn promote_in<E>(id: impl Into<ElementId>, element: E) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    entrance(id, PROMOTE_IN, element, |el, t| {
        scaled(
            el.opacity(lerp(theme::MOTION_PROMOTE_FROM_OPACITY, 1.0, t)),
            lerp(theme::MOTION_PROMOTE_FROM_SCALE, 1.0, t),
            FROM_TOP,
        )
        .relative()
        .top(px(-theme::MOTION_PROMOTE_DROP * (1.0 - t)))
    })
}

/// `code-user-bubble-enter`: the sent bubble `t` along `BUBBLE_IN` (an
/// overshoot, so `t` passes 1 before it settles) — opacity up to 1,
/// scale from `MOTION_BUBBLE_FROM_SCALE` about its right edge, rising
/// `MOTION_BUBBLE_RISE`. The caller holds the clock (the transcript's
/// arrivals) and draws its frames.
pub fn bubble_face<E: Styled>(el: E, t: f32) -> E {
    scaled(
        el.opacity(t.clamp(0.0, 1.0)),
        lerp(theme::MOTION_BUBBLE_FROM_SCALE, 1.0, t),
        FROM_RIGHT,
    )
    .relative()
    .top(px(theme::MOTION_BUBBLE_RISE * (1.0 - t)))
}

/// A tooltip fading and growing in from `MOTION_FLOAT_FROM_SCALE` about
/// its top left (where the pointer stands) over `TOOLTIP_IN`.
pub fn tooltip_in<E>(id: impl Into<ElementId>, element: E) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    entrance(id, TOOLTIP_IN, element, |el, t| {
        grown(el.opacity(t), t, FROM_TOP_LEFT)
    })
}

/// A sheet's veil darkening in over `spec`: only its ground fades, so the
/// sheet on it keeps its own entrance.
pub fn veil_in<E>(id: impl Into<ElementId>, spec: MotionSpec, element: E) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    entrance(id, spec, element, |el, t| el.bg(veil_at(t)))
}

/// A lightbox's picture growing from the spot it was opened at (`from`, as
/// fractions of the box), fading up as it grows: the lightbox's own entrance
/// over the dim's (Codex's, from the thumbnail).
pub fn grow_from<E>(id: impl Into<ElementId>, element: E, from: ScaleFrom) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    entrance(id, LIGHTBOX_GROW, element, move |el, t| {
        scaled(
            el.opacity(t),
            lerp(theme::MOTION_LIGHTBOX_FROM_SCALE, 1.0, t),
            from,
        )
    })
}

/// `veil_in` to a dim of the caller's (`theme::LIGHTBOX_VEIL`), over `VEIL`.
pub fn dim_in<E>(id: impl Into<ElementId>, element: E, dim: u32) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    entrance(id, VEIL, element, move |el, t| {
        el.bg(mix(gpui::transparent_black(), gpui::rgba(dim).into(), t))
    })
}

/// The veil of a sheet that went at `left`, lifting over `spec`: a ground
/// alone, answering no pointer, laid where the veil stood.
pub fn veil_out(left: Instant, spec: MotionSpec, element: gpui::Div) -> impl IntoElement {
    Played {
        origin: Origin::At(left),
        spec,
        element,
        face: |el: gpui::Div, t: f32| el.bg(veil_at(1.0 - t)),
    }
}

fn veil_at(t: f32) -> Hsla {
    mix(gpui::transparent_black(), gpui::rgba(theme::VEIL).into(), t)
}

fn entrance<E, F>(id: impl Into<ElementId>, spec: MotionSpec, element: E, face: F) -> Played<E, F>
where
    E: IntoElement + 'static,
    F: FnOnce(E, f32) -> E + 'static,
{
    Played {
        origin: Origin::Mount(id.into()),
        spec,
        element,
        face,
    }
}

/// Where a [`Played`] one-shot starts.
enum Origin {
    /// When the element under this id first mounts (an entrance): kept in
    /// its element state, so it plays once and a remount plays it again.
    Mount(ElementId),
    /// At an instant the caller holds (an exit: when its subject went).
    At(Instant),
}

/// A one-shot drawn at its spec's eased progress on the executor's clock:
/// `face` draws `element` at 0..1, asking for the next frame while it
/// plays and none once it lands. Reduced motion lands it at once.
pub struct Played<E, F> {
    origin: Origin,
    spec: MotionSpec,
    element: E,
    face: F,
}

impl<E, F> gpui::RenderOnce for Played<E, F>
where
    E: IntoElement + 'static,
    F: FnOnce(E, f32) -> E + 'static,
{
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = play(self.origin, self.spec, window, cx);
        (self.face)(self.element, t)
    }
}

impl<E, F> IntoElement for Played<E, F>
where
    E: IntoElement + 'static,
    F: FnOnce(E, f32) -> E + 'static,
{
    type Element = gpui::ViewElement<Self>;

    #[track_caller]
    fn into_element(self) -> Self::Element {
        gpui::ViewElement::new(self)
    }
}

/// How far a one-shot from `origin` has played along `spec` now.
fn play(origin: Origin, spec: MotionSpec, window: &mut Window, cx: &mut App) -> f32 {
    if reduced_motion(cx) {
        return 1.0;
    }
    let now = cx.background_executor().now();
    let started = match origin {
        Origin::At(at) => at,
        Origin::Mount(id) => window.with_global_id(id, |id, window| {
            window.with_element_state(id, |started: Option<Instant>, _| {
                let started = started.unwrap_or(now);
                (started, started)
            })
        }),
    };
    let elapsed = now.saturating_duration_since(started);
    if elapsed >= spec.duration() {
        return 1.0;
    }
    request_frame(window);
    spec.progress_at(elapsed)
}

// ---------------------------------------------------------------------------
// Frames: who an animation's next frame redraws
// ---------------------------------------------------------------------------
//
// gpui's `request_animation_frame` notifies the view drawing the animation.
// For the Cockpit that is the costliest notify in the window: its cached
// parts (the nav, every Pane) all read it, so a menu fading in over 140ms
// rebuilt every Pane on each of its frames. A view names a cheaper one that
// redraws the same elements ([`draw_frames_through`]); every clock here —
// the one-shots, the settled flips, the hover blends — asks for its frames
// through it.

thread_local! {
    static FRAMES_THROUGH: RefCell<HashMap<EntityId, EntityId>> = RefCell::default();
}

/// `view`'s animations draw their frames by notifying `through` instead of
/// `view` itself; `None` forgets it. The Cockpit names the frame around its
/// cached parts (ADR 0012's `FrameTick`): its floats and frame controls
/// redraw without rebuilding a part.
pub fn draw_frames_through(view: EntityId, through: Option<EntityId>) {
    FRAMES_THROUGH.with(|map| match through {
        Some(through) => map.borrow_mut().insert(view, through),
        None => map.borrow_mut().remove(&view),
    });
}

/// The view to notify for a frame of an animation `view` draws.
pub fn frames_through(view: EntityId) -> EntityId {
    FRAMES_THROUGH.with(|map| map.borrow().get(&view).copied().unwrap_or(view))
}

/// Ask for the next frame for an animation the current view is drawing,
/// through the view that redraws it most cheaply.
pub fn request_frame(window: &Window) {
    let view = frames_through(window.current_view());
    window.on_next_frame(move |_, cx| cx.notify(view));
}

// ---------------------------------------------------------------------------
// Tweens: interruptible value transitions
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
// Reveal: a slot that slides open and shut
// ---------------------------------------------------------------------------

/// How far a slot that opens and closes is in — 0 shut, 1 open at rest —
/// riding a [`Tween`] that eases in over `enter` and out over the shorter
/// `exit`, and turns around from where it is when flipped mid-flight. The
/// owner lays the slot out at [`Reveal::value`] (a share of a split, a
/// column's width), asks for frames while it is [`Reveal::running`], and
/// keeps the slot until it is [`Reveal::shut`]: a slot outlives its close
/// by its exit. The Review beside its Pane rides one (`SLOT_OPEN`,
/// `SLOT_CLOSE`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reveal {
    open: bool,
    tween: Tween,
    enter: MotionSpec,
    exit: MotionSpec,
}

impl Reveal {
    /// A slot opening now, from shut.
    pub fn opening(enter: MotionSpec, exit: MotionSpec, now: Instant) -> Self {
        Self {
            open: true,
            tween: Tween::new(0.0, 1.0, enter, now),
            enter,
            exit,
        }
    }

    /// A slot already open, at rest: what first paint finds arrives in
    /// place, with no entrance.
    pub fn open(enter: MotionSpec, exit: MotionSpec, now: Instant) -> Self {
        Self {
            open: true,
            tween: Tween::new(1.0, 1.0, enter, now),
            enter,
            exit,
        }
    }

    /// A slot shut, at rest.
    pub fn closed(enter: MotionSpec, exit: MotionSpec, now: Instant) -> Self {
        Self {
            open: false,
            tween: Tween::new(0.0, 0.0, exit, now),
            enter,
            exit,
        }
    }

    /// Open or close it, from wherever it is on screen; no change is a
    /// no-op.
    pub fn set(&mut self, open: bool, now: Instant, reduced: bool) {
        if self.open == open {
            return;
        }
        self.open = open;
        let (to, spec) = if open {
            (1.0, self.enter)
        } else {
            (0.0, self.exit)
        };
        self.tween = Tween::retarget(Some(self.tween), 1.0 - to, to, spec, now, reduced);
    }

    /// Land it where it is going (a seam grabbed mid-slide).
    pub fn land(&mut self, now: Instant) {
        let to = self.tween.to;
        self.tween = Tween::new(to, to, self.tween.spec, now);
    }

    /// Whether it is open, or opening.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// How far in it is at `now`.
    pub fn value(&self, now: Instant, reduced: bool) -> f32 {
        self.tween.value(now, reduced)
    }

    /// Mid-slide: the owner must ask for another frame.
    pub fn running(&self, now: Instant, reduced: bool) -> bool {
        self.tween.running(now, reduced)
    }

    /// Closed and done sliding: the owner lets the slot go.
    pub fn shut(&self, now: Instant, reduced: bool) -> bool {
        !self.open && !self.running(now, reduced)
    }
}

// ---------------------------------------------------------------------------
// Settled: a state change that eases, held in element state
// ---------------------------------------------------------------------------
//
// A two-state control (a disclosure's turn, send ⇄ stop, a fold) animates
// between its states only when the state *changes under it*: the element
// remembers the target it last drew, and on its first mount — first paint,
// a row scrolled back into view — it lands on the state it is given, with
// no entrance. A flip mid-flight turns around from where it is.

#[derive(Debug, Clone, Copy)]
struct SettleState {
    target: f32,
    tween: Option<Tween>,
}

/// The value `id` draws this frame on its way to `target` along `spec`: a
/// change of `target` eases from wherever it is on screen, and the first
/// draw lands on `target` (a reading first shown arrives in place). It
/// asks for frames while it moves. The usage ring's arc and ink ride it.
pub fn settle(
    id: impl Into<ElementId>,
    target: f32,
    spec: MotionSpec,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    settle_with(id, target, spec, false, window, cx)
}

/// `settle`, where `reveal_only` snaps any move toward 0 (a fold that
/// closes goes at once) and eases only the opening.
fn settle_with(
    id: impl Into<ElementId>,
    target: f32,
    spec: MotionSpec,
    reveal_only: bool,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    let now = cx.background_executor().now();
    let reduced = reduced_motion(cx);
    let (value, running) = window.with_global_id(id.into(), |id, window| {
        window.with_element_state(id, |state: Option<SettleState>, _| {
            let mut state = state.unwrap_or(SettleState {
                target,
                tween: None,
            });
            if state.target != target {
                let snap = reveal_only && target < state.target;
                state.tween = (!snap).then(|| {
                    Tween::retarget(state.tween, state.target, target, spec, now, reduced)
                });
                state.target = target;
            }
            let value = state
                .tween
                .map_or(target, |tween| tween.value(now, reduced));
            let running = state.tween.is_some_and(|tween| tween.running(now, reduced));
            ((value, running), state)
        })
    });
    if running {
        request_frame(window);
    }
    value
}

/// An element drawn at [`settle`]'s value: `build` receives it (0..1 between
/// the two states) and returns what to paint.
pub struct Settled<F> {
    id: ElementId,
    target: f32,
    spec: MotionSpec,
    reveal_only: bool,
    build: F,
}

impl<F> Settled<F> {
    /// Only the opening eases; a close lands at once.
    pub fn reveal_only(mut self) -> Self {
        self.reveal_only = true;
        self
    }
}

/// Ease `build`'s element between two states (`on` is the target).
pub fn settled<F, E>(id: impl Into<ElementId>, on: bool, spec: MotionSpec, build: F) -> Settled<F>
where
    F: FnOnce(f32) -> E + 'static,
    E: IntoElement,
{
    Settled {
        id: id.into(),
        target: if on { 1.0 } else { 0.0 },
        spec,
        reveal_only: false,
        build,
    }
}

impl<F, E> gpui::RenderOnce for Settled<F>
where
    F: FnOnce(f32) -> E + 'static,
    E: IntoElement,
{
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let value = settle_with(
            self.id,
            self.target,
            self.spec,
            self.reveal_only,
            window,
            cx,
        );
        (self.build)(value)
    }
}

impl<F, E> IntoElement for Settled<F>
where
    F: FnOnce(f32) -> E + 'static,
    E: IntoElement,
{
    type Element = gpui::ViewElement<Self>;

    #[track_caller]
    fn into_element(self) -> Self::Element {
        gpui::ViewElement::new(self)
    }
}

// ---------------------------------------------------------------------------
// Folds, label rolls and exits
// ---------------------------------------------------------------------------

/// A fold's body `shown` of the way open (0 shut, 1 open at rest): its
/// height clipped to that share of what it measures, at that opacity — a
/// CSS `grid-template-rows: 0fr → 1fr` with its fade. The body keeps its
/// own height inside the clip, so nothing in it re-wraps; it is measured
/// on every draw, so a fold that starts to shut knows how far it has to
/// go. The owner holds the clock (a [`Reveal`] per fold) and draws the body
/// until it is shut.
pub fn fold_body(id: impl Into<ElementId>, shown: f32, body: impl IntoElement) -> impl IntoElement {
    FoldBody {
        id: id.into(),
        shown,
        body: body.into_any_element(),
    }
}

#[derive(IntoElement)]
struct FoldBody {
    id: ElementId,
    shown: f32,
    body: AnyElement,
}

impl gpui::RenderOnce for FoldBody {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let natural = window.with_global_id(self.id, |id, window| {
            window.with_element_state(id, |cell: Option<Rc<Cell<gpui::Pixels>>>, _| {
                let cell = cell.unwrap_or_default();
                (cell.clone(), cell)
            })
        });
        let measured = natural.clone();
        let shown = self.shown.clamp(0.0, 1.0);
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .when(shown < 1.0, |clip| {
                clip.h(natural.get() * shown)
                    .overflow_hidden()
                    .opacity(shown)
            })
            .on_children_prepainted(move |bounds, _, _| {
                if let Some(body) = bounds.first() {
                    measured.set(body.size.height);
                }
            })
            .child(div().flex().flex_col().flex_shrink_0().child(self.body))
    }
}

/// A label that rolls when its words change (Claude's `cds-morph`): the
/// old face leaves to opacity 0 and `MOTION_MORPH_SHIFT` up while the new
/// one comes in from as far below, each over `MORPH`. `build` draws a face
/// for its words. Its first draw lands in place; reduced motion swaps the
/// words at once. Frames are asked for only while a roll plays.
pub fn morph(
    id: impl Into<ElementId>,
    face: SharedString,
    build: impl Fn(SharedString) -> AnyElement + 'static,
) -> impl IntoElement {
    Morph {
        id: id.into(),
        face,
        build: Box::new(build),
    }
}

#[derive(IntoElement)]
struct Morph {
    id: ElementId,
    face: SharedString,
    build: Box<dyn Fn(SharedString) -> AnyElement>,
}

/// What a [`Morph`] drew last, and the face leaving since when.
#[derive(Clone)]
struct MorphState {
    face: SharedString,
    leaving: Option<(SharedString, Instant)>,
}

impl gpui::RenderOnce for Morph {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let now = cx.background_executor().now();
        let reduced = reduced_motion(cx);
        let face = self.face.clone();
        let leaving = window.with_global_id(self.id, |id, window| {
            window.with_element_state(id, |state: Option<MorphState>, _| {
                let mut state = state.unwrap_or(MorphState {
                    face: face.clone(),
                    leaving: None,
                });
                if state.face != face {
                    let old = std::mem::replace(&mut state.face, face.clone());
                    state.leaving = (!reduced).then_some((old, now));
                }
                state.leaving = state
                    .leaving
                    .take()
                    .filter(|(_, at)| now.saturating_duration_since(*at) < MORPH.duration());
                (state.leaving.clone(), state)
            })
        });
        let Some((old, at)) = leaving else {
            return div().flex().min_w_0().child((self.build)(self.face));
        };
        request_frame(window);
        let t = MORPH.progress_at(now.saturating_duration_since(at));
        let shift = theme::MOTION_MORPH_SHIFT;
        div()
            .relative()
            .flex()
            .min_w_0()
            .child(
                div()
                    .relative()
                    .flex()
                    .min_w_0()
                    .opacity(t)
                    .top(px(shift * (1.0 - t)))
                    .child((self.build)(self.face)),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(-shift * t))
                    .flex()
                    .min_w_0()
                    .opacity(1.0 - t)
                    .child((self.build)(old)),
            )
    }
}

/// What a surface that leaves with an exit (a toast, a sheet, the palette)
/// showed, and the one leaving since when: each frame the owner says what
/// stands now, and gets back what is still on its way out.
pub struct Exits<T> {
    shown: Option<T>,
    leaving: Option<(T, Instant)>,
}

impl<T> Default for Exits<T> {
    fn default() -> Self {
        Self {
            shown: None,
            leaving: None,
        }
    }
}

impl<T: Clone> Exits<T> {
    /// This frame's `shown`, at `now`: one that went since the last frame
    /// (gone, or replaced by one not `same`) leaves now, and is returned
    /// with when it went while its `exit` runs. Under reduced motion
    /// nothing leaves.
    pub fn stage(
        &mut self,
        shown: Option<&T>,
        same: impl Fn(&T, &T) -> bool,
        exit: MotionSpec,
        now: Instant,
        reduced: bool,
    ) -> Option<(T, Instant)> {
        let went = match (self.shown.take(), shown) {
            (Some(was), Some(next)) if same(&was, next) => None,
            (was, _) => was,
        };
        if let Some(went) = went.filter(|_| !reduced) {
            self.leaving = Some((went, now));
        }
        self.shown = shown.cloned();
        let leaving =
            |left: Instant| !reduced && now.saturating_duration_since(left) < exit.duration();
        if !self
            .leaving
            .as_ref()
            .is_some_and(|(_, left)| leaving(*left))
        {
            self.leaving = None;
        }
        self.leaving.clone()
    }

    /// What stood goes at once, with no exit (the operator answered it).
    pub fn dismiss(&mut self) {
        self.shown = None;
        self.leaving = None;
    }
}

// ---------------------------------------------------------------------------
// Pulse clock: one throttled drive for every loop
// ---------------------------------------------------------------------------
//
// A `with_animation(...repeat())` loop asks for a frame on every display
// frame for as long as it is mounted: one working Thread held the whole
// cockpit at the display's refresh rate. The clock replaces that with one
// timer on a ~30fps grid. A loop reads its phase from [`loop_phase`] (or
// [`pulse_phase`], [`css_phase`], [`script_phase`]), which also declares
// when the loop's picture next changes (`ferrite_core::cadence`): the
// caret's fades and not its plateaus, a spinner's steps and not the
// instants between them. The timer wakes at the earliest declared change,
// snapped to the grid from the clock's epoch, and notifies only the views
// due then. A view that stops painting a loop declares nothing more and
// lapses after its last wake; with nothing declared the clock parks: no
// timer, no frame. All loops share one epoch, so two marks on screen stay
// phase-locked, and every frame drawn is the one the fixed rate drew at
// that instant.

fn pulse_tick() -> Duration {
    Duration::from_millis(theme::MOTION_PULSE_TICK_MS)
}

#[derive(Default)]
struct PulseClock {
    epoch: Option<Instant>,
    schedule: cadence::Schedule<EntityId>,
    /// The armed wake and its timer; `None` when parked.
    timer: Option<(Instant, gpui::Task<()>)>,
}

impl Global for PulseClock {}

impl PulseClock {
    fn grid(&mut self, now: Instant) -> cadence::Grid {
        cadence::Grid {
            epoch: *self.epoch.get_or_insert(now),
            tick: pulse_tick(),
        }
    }
}

/// How a loop's picture moves through its turn (`cadence::Motion`).
pub use ferrite_core::cadence::Motion;

/// The phase `[0, 1)` of a loop with this `period` and `motion`, on the
/// shared epoch, declaring `view`'s next draw for the instant its picture
/// next changes. Call it only while painting the loop. Reduced motion
/// returns the loop's start (0) and declares nothing.
pub fn loop_phase(period: Duration, motion: Motion, view: EntityId, cx: &mut App) -> f32 {
    run_loop(None, period, motion, view, cx)
}

/// [`loop_phase`] from the loop's own `origin` rather than the shared
/// epoch: the caret's blink restarts solid on every focus and edit.
pub fn loop_phase_from(
    origin: Instant,
    period: Duration,
    motion: Motion,
    view: EntityId,
    cx: &mut App,
) -> f32 {
    run_loop(Some(origin), period, motion, view, cx)
}

/// A loop whose picture moves at every instant ([`loop_phase`] with
/// `Motion::Continuous`): `view` is drawn on every tick of the grid. (The
/// app's loops name their motion; the clock's own tests drive this one.)
#[cfg_attr(not(test), allow(dead_code))]
pub fn pulse_phase(period: Duration, view: EntityId, cx: &mut App) -> f32 {
    loop_phase(period, Motion::Continuous, view, cx)
}

fn run_loop(
    origin: Option<Instant>,
    period: Duration,
    motion: Motion,
    view: EntityId,
    cx: &mut App,
) -> f32 {
    if reduced_motion(cx) {
        hold_still(view, cx);
        return 0.0;
    }
    if period.is_zero() {
        return 0.0;
    }
    let now = cx.background_executor().now();
    let clock = cx.default_global::<PulseClock>();
    let grid = clock.grid(now);
    let looped = cadence::Loop {
        origin: origin.unwrap_or(grid.epoch),
        period,
        motion,
    };
    if let Some(change) = looped.next_change(now).filter(|_| !quiet()) {
        clock.schedule.declare(view, grid.wake(change, now), now);
        arm(now, cx);
    }
    looped.phase(now)
}

/// `view` draws its loops still (reduced motion): its lease from before
/// goes now, and with nothing else waiting the clock parks, rather than
/// serving one more wake of a loop that no longer moves.
pub fn hold_still(view: EntityId, cx: &mut App) {
    let clock = cx.default_global::<PulseClock>();
    clock.schedule.withdraw(view);
    if clock.schedule.next_wake().is_none() {
        clock.timer = None;
    }
}

thread_local! {
    static QUIET: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Lay `f`'s loops out at their phase now without declaring them on the
/// clock: their view is not drawn again for them, because something else
/// draws them (the loops overlay, `loops_overlay::hosted`).
pub fn quietly<R>(f: impl FnOnce() -> R) -> R {
    QUIET.with(|quiet| quiet.set(quiet.get() + 1));
    let result = f();
    QUIET.with(|quiet| quiet.set(quiet.get() - 1));
    result
}

pub(crate) fn quiet() -> bool {
    QUIET.with(|quiet| quiet.get() > 0)
}

/// Text in `view` that changes with time but is no loop (a working clock's
/// `12s`, a nav row's `2m`) next changes at `at`: while a loop keeps the
/// clock running, `view` is drawn on the grid then, as the fixed rate drew
/// it; otherwise it waits for whatever next draws `view`. Nothing rides
/// under reduced motion or a held capture.
pub fn ride(view: EntityId, at: Instant, cx: &mut App) {
    if reduced_motion(cx) || held_loops().is_some() {
        return;
    }
    let now = cx.background_executor().now();
    let clock = cx.default_global::<PulseClock>();
    let wake = clock.grid(now).wake(at, now);
    clock.schedule.ride(view, wake, now);
    arm(now, cx);
}

/// Text in `view` that must move on its own at `at` whatever else runs
/// (a live tool call's `3s` → `4s`): `view` is drawn on the grid then,
/// under reduced motion too — a clock is no motion. Under reduced motion a
/// loop drawn in the same view withdraws its wakes (`run_loop`), so call
/// this after the view's loops.
pub fn wake_at(view: EntityId, at: Instant, cx: &mut App) {
    let now = cx.background_executor().now();
    let clock = cx.default_global::<PulseClock>();
    let wake = clock.grid(now).wake(at, now);
    clock.schedule.declare(view, wake, now);
    arm(now, cx);
}

/// Arm the one timer for the schedule's earliest wake, unless it is armed
/// for that or sooner. Nothing declared: it stays parked.
fn arm(now: Instant, cx: &mut App) {
    let clock = cx.default_global::<PulseClock>();
    let Some(wake) = clock.schedule.next_wake() else {
        return;
    };
    if clock
        .timer
        .as_ref()
        .is_some_and(|(armed, _)| *armed <= wake)
    {
        return;
    }
    let delay = wake.saturating_duration_since(now);
    let timer = cx.spawn(async move |cx| {
        cx.background_executor().timer(delay).await;
        cx.update(fire);
    });
    cx.default_global::<PulseClock>().timer = Some((wake, timer));
}

/// The timer's wake: notify each view due, then arm for the next.
fn fire(cx: &mut App) {
    let now = cx.background_executor().now();
    let clock = cx.default_global::<PulseClock>();
    clock.timer = None;
    let due = clock.schedule.take_due(now);
    if !due.is_empty() {
        #[cfg(any(test, feature = "test-support"))]
        testing::PULSE_TICKS.with(|ticks| ticks.set(ticks.get() + 1));
        for view in due {
            cx.notify(view);
        }
    }
    arm(now, cx);
}

/// The two clocks a browser runs its loops on, held at a fixed time: a
/// capture reproducing one instant of the prototype holds every loop where
/// the prototype's shot caught it. Its CSS animations (the caret's blink,
/// the working caption's shimmer) and its scripted ones (the spinners'
/// frames, stepped by timers) start a few milliseconds apart, so each runs
/// on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldLoops {
    /// Elapsed on the CSS loops' clock: the caret's blink, the shimmer.
    pub css: Duration,
    /// Elapsed on the scripted loops' clock: the spinners.
    pub script: Duration,
}

thread_local! {
    static HELD_LOOPS: std::cell::Cell<Option<HeldLoops>> = const { std::cell::Cell::new(None) };
}

/// Hold every loop at `held` (a capture), or let them run (`None`).
pub fn hold_loops(held: Option<HeldLoops>) {
    HELD_LOOPS.with(|cell| cell.set(held));
}

/// The loops' held time, if a capture holds them.
pub fn held_loops() -> Option<HeldLoops> {
    HELD_LOOPS.with(std::cell::Cell::get)
}

fn phase_at(elapsed: Duration, period: Duration) -> f32 {
    cadence::phase_after(elapsed, period)
}

/// A CSS loop's phase (the caret's blink, the shimmer): the held capture
/// time's, else the pulse clock's (`loop_phase`).
pub fn css_phase(period: Duration, motion: Motion, view: EntityId, cx: &mut App) -> f32 {
    match held_loops() {
        Some(held) if !reduced_motion(cx) => phase_at(held.css, period),
        _ => loop_phase(period, motion, view, cx),
    }
}

/// A scripted loop's phase (the spinners' frames): the held capture time's,
/// else the pulse clock's (`loop_phase`).
pub fn script_phase(period: Duration, motion: Motion, view: EntityId, cx: &mut App) -> f32 {
    match held_loops() {
        Some(held) if !reduced_motion(cx) => phase_at(held.script, period),
        _ => loop_phase(period, motion, view, cx),
    }
}

// ---------------------------------------------------------------------------
// Frame loops: a loop drawn at the display's rate
// ---------------------------------------------------------------------------
//
// The working mark's orbs and the working caption's shimmer move at every
// instant, and a 30fps tick shows as steps. They read their phase on the pulse clock's epoch, so they stay in
// step with every other loop, and ask the display for the next frame. Every
// frame loop in a window shares one request: the first lease in a frame
// asks, later ones join it, and the frame notifies each view that leased.
// A loop that is not drawn, or is clipped out of sight, does not lease
// again, so the window falls quiet a frame later. macOS stops the frames
// of a window that cannot be seen.

/// One draw of a frame loop: its phase `[0, 1)`, and whether the draw
/// should lease the next frame ([`lease_frame`]). It does not when a
/// capture holds the loops, or when it is laid out quietly for the loops
/// overlay, which draws it itself.
#[derive(Clone, Copy, Debug)]
pub struct FrameLoop {
    pub phase: f32,
    pub leases: bool,
}

/// Which of a browser's two loop clocks a frame loop reproduces when a
/// capture holds the loops (`HeldLoops`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopClock {
    /// A CSS animation's: the shimmer.
    Css,
    /// A scripted animation's: the working mark.
    Script,
}

/// A frame loop with this `period`, now, on the shared epoch; `None` under
/// reduced motion, where the loop draws its still state.
pub fn frame_loop(period: Duration, clock: LoopClock, cx: &mut App) -> Option<FrameLoop> {
    if reduced_motion(cx) {
        return None;
    }
    if let Some(held) = held_loops() {
        let elapsed = match clock {
            LoopClock::Css => held.css,
            LoopClock::Script => held.script,
        };
        return Some(FrameLoop {
            phase: phase_at(elapsed, period),
            leases: false,
        });
    }
    let now = cx.background_executor().now();
    let epoch = cx.default_global::<PulseClock>().grid(now).epoch;
    Some(FrameLoop {
        phase: phase_at(now.saturating_duration_since(epoch), period),
        leases: !quiet(),
    })
}

/// The views each window's frame loops redraw on its next frame; a window
/// is listed while its one request is out.
#[derive(Default)]
struct FrameLeases(HashMap<gpui::WindowId, Vec<EntityId>>);

impl Global for FrameLeases {}

/// Draw the current view's frame loop again on the display's next frame,
/// through the view that redraws it most cheaply. Call at prepaint, when
/// the loop can be seen. All of a window's leases share one request.
pub fn lease_frame(window: &mut Window, cx: &mut App) {
    let view = frames_through(window.current_view());
    let id = window.window_handle().window_id();
    let leases = cx.default_global::<FrameLeases>().0.entry(id).or_default();
    let ask = leases.is_empty();
    if !leases.contains(&view) {
        leases.push(view);
    }
    if ask {
        window.on_next_frame(move |_, cx| {
            let due = cx
                .default_global::<FrameLeases>()
                .0
                .remove(&id)
                .unwrap_or_default();
            for view in due {
                cx.notify(view);
            }
        });
    }
}

/// The clock is parked: nothing is declared and no timer is armed.
#[cfg(any(test, feature = "test-support"))]
pub fn pulse_parked(cx: &App) -> bool {
    cx.try_global::<PulseClock>()
        .is_none_or(|clock| clock.timer.is_none())
}

// ---------------------------------------------------------------------------
// Hover blend: `transition-colors` for gpui's snapping hover
// ---------------------------------------------------------------------------
//
// gpui's `.hover()` style applies the frame the pointer enters. The blend
// fades it over its spec instead (a button's `HOVER_FADE`; a row's
// `HOVER_SNAP` lands at once, but rides the same store so it notifies only
// the view that painted it): a per-key progress, advanced from wall
// time wherever it is read, flipped by an `on_hover` listener, re-anchored at
// its current value when the pointer reverses mid-flight. The pressed face
// stays a gpui `.active()` refinement, which overrides the blended ground
// at once: a press never waits on the fade.
//
// The store is a main-thread `thread_local`, so row builders without a `cx`
// can blend; the root view stamps each frame's time on it first
// ([`hover_frame_start`]). The pointer's flip notifies only the view that
// painted the listener (`Window::hover_listener_view`), so a hover in the
// nav leaves every cached transcript as it is; while a blend is mid-flight
// that view is drawn again each frame ([`hover_fades_active`]).
//
// A blend read by a cached view goes unread for as long as that view is
// reused, so frames say nothing about whether its element is still
// mounted: an entry is forgotten once it is unread for `HOVER_FORGET` (an
// element that unmounted mid-hover never hears its leave), or once it has
// faded back to rest.

/// How long a blend nobody draws is kept: past any cached view's quiet
/// spell while it streams, short of a hover anyone would see come back.
const HOVER_FORGET: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy)]
struct FadeEntry {
    origin: f32,
    target: f32,
    started: Instant,
    /// When a draw last read it, or the pointer set it.
    read: Instant,
    /// The blend's timing: `HOVER_FADE`, unless the surface names its own
    /// ([`hover_listener_with`]).
    spec: MotionSpec,
    /// The view whose listener set it: drawn again while it fades.
    view: Option<EntityId>,
}

impl FadeEntry {
    fn value(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed >= self.spec.duration() {
            return self.target;
        }
        lerp(self.origin, self.target, self.spec.progress_at(elapsed))
    }

    fn settled(&self, now: Instant) -> bool {
        self.origin == self.target
            || now.saturating_duration_since(self.started) >= self.spec.duration()
    }
}

/// Per-key hover progress. Pure over an explicit `now`.
#[derive(Default)]
pub struct HoverFades {
    entries: HashMap<SharedString, FadeEntry>,
    /// The views painting a blend still mid-flight, as of the last tick.
    fading: Vec<EntityId>,
}

impl HoverFades {
    /// `set_with` on a button's blend, from no view.
    #[cfg(test)]
    pub fn set_at(&mut self, key: &SharedString, hovered: bool, reduced: bool, now: Instant) {
        self.set_with(key, hovered, reduced, now, HOVER_FADE, None);
    }

    /// The pointer entered (`hovered`) or left the element behind `key`,
    /// painted by `view`, blending on `spec` (`HOVER_FADE`, or a surface's
    /// own timing). Reduced motion snaps to the endpoint.
    pub fn set_with(
        &mut self,
        key: &SharedString,
        hovered: bool,
        reduced: bool,
        now: Instant,
        spec: MotionSpec,
        view: Option<EntityId>,
    ) {
        let target = if hovered { 1.0 } else { 0.0 };
        let Some(current) = self.entries.get(key).map(|entry| entry.value(now)) else {
            if hovered {
                let origin = if reduced { target } else { 0.0 };
                self.insert(key, origin, target, now, spec, view);
            }
            // A leave for a key never entered: nothing to fade.
            return;
        };
        let origin = if reduced { target } else { current };
        self.insert(key, origin, target, now, spec, view);
    }

    fn insert(
        &mut self,
        key: &SharedString,
        origin: f32,
        target: f32,
        now: Instant,
        spec: MotionSpec,
        view: Option<EntityId>,
    ) {
        let entry = FadeEntry {
            origin,
            target,
            started: now,
            read: now,
            spec,
            view,
        };
        self.entries.insert(key.clone(), entry);
    }

    /// Hover progress (0..1) for `key` at `now`; stamps it as drawn.
    pub fn value_at(&mut self, key: &str, now: Instant) -> f32 {
        self.entries.get_mut(key).map_or(0.0, |entry| {
            entry.read = now;
            entry.value(now)
        })
    }

    /// Once per frame: drop entries back at rest or unread for
    /// `HOVER_FORGET`, note the views whose blend is mid-flight, and say
    /// whether any is.
    pub fn tick_at(&mut self, now: Instant) -> bool {
        let mut active = false;
        let fading = &mut self.fading;
        fading.clear();
        self.entries.retain(|_, entry| {
            if now.saturating_duration_since(entry.read) > HOVER_FORGET {
                return false;
            }
            let settled = entry.settled(now);
            if !settled {
                active = true;
                fading.extend(entry.view);
            }
            !(settled && entry.target == 0.0)
        });
        fading.sort_unstable();
        fading.dedup();
        active
    }
}

thread_local! {
    static HOVER_FADES: RefCell<HoverFades> = RefCell::new(HoverFades::default());
    /// The frame's clock, stamped by [`hover_frame_start`]: every blend in a
    /// frame reads one time, the executor's, so tests can step it.
    static HOVER_NOW: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

fn hover_now() -> Instant {
    HOVER_NOW.with(|now| now.get()).unwrap_or_else(Instant::now)
}

/// The frame hook's first half: call once per window frame, at the top of
/// the root view's render, before any blend is read.
pub fn hover_frame_start(cx: &App) {
    let now = cx.background_executor().now();
    HOVER_NOW.with(|cell| cell.set(Some(now)));
}

/// Forget every hover blend and the frame's clock: the blends live on the
/// thread, not the window, so a new window on the same thread (each
/// visual-reference scene's) starts with nothing under the pointer.
pub fn hover_reset() {
    HOVER_FADES.with(|fades| *fades.borrow_mut() = HoverFades::default());
    HOVER_NOW.with(|now| now.set(None));
}

/// Hover progress (0..1) for `key` this frame.
pub fn hover_t(key: &str) -> f32 {
    let now = hover_now();
    HOVER_FADES.with(|fades| fades.borrow_mut().value_at(key, now))
}

/// An `on_hover` listener driving the blend for `key`: pair it with a
/// [`hover_blend`] read of the same key on the same element. A button's
/// blend: `HOVER_FADE`, both ways.
pub fn hover_listener(key: SharedString) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    hover_listener_with(key, HOVER_FADE, HOVER_FADE)
}

/// [`hover_listener`] for a row (a nav row, a menu row, the sidebar's own
/// buttons): the hover lands on the frame the pointer arrives
/// (`HOVER_SNAP`), as Claude's rows do. It still notifies only the view
/// that painted it.
pub fn hover_snap(key: SharedString) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    hover_listener_with(key, HOVER_SNAP, HOVER_SNAP)
}

/// [`hover_listener`] blending in over `enter` and out over `leave` (a
/// message's actions: in after a beat, out at once).
pub fn hover_listener_with(
    key: SharedString,
    enter: MotionSpec,
    leave: MotionSpec,
) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    move |hovered, window, cx| {
        let reduced = reduced_motion(cx);
        let now = cx.background_executor().now();
        let view = window.hover_listener_view();
        let spec = if *hovered { enter } else { leave };
        HOVER_FADES.with(|fades| {
            fades
                .borrow_mut()
                .set_with(&key, *hovered, reduced, now, spec, view)
        });
        // Only the view that painted the blend draws it: notifying that
        // view (through the one that redraws it most cheaply) redraws it
        // and its ancestors, and reuses every other cached view (the
        // transcripts, the Cockpit's parts). Called from outside a painted
        // listener, refresh, as a gpui `.hover()` style does on the same
        // event.
        match view {
            Some(view) => cx.notify(frames_through(view)),
            None => window.refresh(),
        }
    }
}

/// The press blend of the element whose hover blend is `key`.
fn press_key(key: &str) -> SharedString {
    format!("press:{key}").into()
}

/// A button's press (Claude's `.cds-btn-squish`): `element`, whose hover
/// blend is `key`, squishes to `MOTION_PRESS_SCALE` about its centre over
/// `PRESS_IN` while the primary button is down on it, and springs back
/// over `PRESS_OUT` when it comes up, wherever. The scale is painted only,
/// so the press lands where it rests. Its frames ride the hover blends'
/// (the view that painted the hover draws them). Reduced motion: no squish.
pub fn pressable<E: InteractiveElement + Styled>(element: E, key: SharedString) -> E {
    pressable_lifting(element, key, 1.0)
}

/// [`pressable`] that also grows to `lift` with `key`'s hover blend.
pub fn pressable_lifting<E: InteractiveElement + Styled>(
    element: E,
    key: SharedString,
    lift: f32,
) -> E {
    let pressed = hover_t(&press_key(&key));
    let hovered = if lift == 1.0 { 0.0 } else { hover_t(&key) };
    let mut element = scaled(
        element,
        lerp(1.0, lift, hovered) * lerp(1.0, theme::MOTION_PRESS_SCALE, pressed),
        FROM_CENTRE,
    );
    let (down, up, away) = (key.clone(), key.clone(), key);
    let listeners = element.interactivity();
    listeners.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
        press(&down, true, window, cx)
    });
    listeners.on_mouse_up(gpui::MouseButton::Left, move |_, window, cx| {
        press(&up, false, window, cx)
    });
    listeners.on_mouse_up_out(gpui::MouseButton::Left, move |_, window, cx| {
        press(&away, false, window, cx)
    });
    element
}

/// The button behind `key` went down or came up: its press blend turns,
/// drawn by the view that painted its hover. A release of a button never
/// pressed (every button hears a release anywhere) does nothing.
fn press(key: &SharedString, down: bool, window: &mut Window, cx: &mut App) {
    if reduced_motion(cx) {
        return;
    }
    let now = cx.background_executor().now();
    let turned = HOVER_FADES.with(|fades| {
        let mut fades = fades.borrow_mut();
        let pressed = press_key(key);
        let was = fades
            .entries
            .get(&pressed)
            .is_some_and(|entry| entry.target == 1.0);
        if was == down {
            return None;
        }
        let view = fades.entries.get(key).and_then(|entry| entry.view);
        let spec = if down { PRESS_IN } else { PRESS_OUT };
        fades.set_with(&pressed, down, false, now, spec, view);
        Some(view)
    });
    match turned {
        Some(Some(view)) => cx.notify(frames_through(view)),
        Some(None) => window.refresh(),
        None => {}
    }
}

/// The frame hook's second half: call once per window frame, at the root
/// view's render tail. True while a blend is mid-flight and frames must keep
/// coming; the views painting one are [`hover_fading_views`].
pub fn hover_fades_active() -> bool {
    let now = hover_now();
    HOVER_FADES.with(|fades| fades.borrow_mut().tick_at(now))
}

/// The views to draw again for the blends still mid-flight, as of the last
/// [`hover_fades_active`] (each painting view through [`frames_through`]):
/// the root notifies them with its next frame (a cached view would
/// otherwise be reused, its blend frozen).
pub fn hover_fading_views() -> Vec<EntityId> {
    let mut views: Vec<EntityId> = HOVER_FADES.with(|fades| {
        fades
            .borrow()
            .fading
            .iter()
            .map(|view| frames_through(*view))
            .collect()
    });
    views.sort_unstable();
    views.dedup();
    views
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
/// and kit spring already honours — and, in unit tests, the kit itself
/// unless a test drives it ([`live`]).
pub fn reduced_motion(cx: &App) -> bool {
    cx.reduce_motion() || !live()
}

/// Whether the kit animates at all. Always, except in unit tests, where it
/// rests unless a test asks for it (`testing::drive`). The suite asserts
/// layout and hit targets straight after a state change, on a clock it
/// steps by hand: live entrances would put those readings mid-flight, and
/// the kit's own springs and cards (gpui's `with_animation`) run on the
/// wall clock besides. A pulse tick nobody asked for would also blank the
/// transcripts' test
/// selectors (gpui's test window keeps no `debug_bounds` for a cached view
/// it reuses). Resting, the kit behaves as it does under reduced motion.
pub(crate) fn live() -> bool {
    #[cfg(any(test, feature = "test-support"))]
    return testing::DRIVE.with(std::cell::Cell::get);
    #[cfg(not(any(test, feature = "test-support")))]
    true
}

#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    thread_local! {
        pub(super) static DRIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        pub(super) static PULSE_TICKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    /// How many times the pulse clock has ticked views on this thread: the
    /// budget a window of loops is measured against.
    pub fn pulse_ticks() -> usize {
        PULSE_TICKS.with(std::cell::Cell::get)
    }

    /// This test (its thread) runs the motion kit live.
    pub fn drive() {
        DRIVE.with(|drive| drive.set(true));
    }

    /// Every hover blend held now, and its value this frame (read without
    /// stamping it as drawn).
    pub fn hover_values() -> Vec<(gpui::SharedString, f32)> {
        let now = super::hover_now();
        super::HOVER_FADES.with(|fades| {
            fades
                .borrow()
                .entries
                .iter()
                .map(|(key, entry)| (key.clone(), entry.value(now)))
                .collect()
        })
    }

    /// Whether `window`'s frame loops have their one request out
    /// ([`super::lease_frame`]): the frame they ask for, among those the
    /// window waits on.
    pub fn frame_leased(window: &gpui::Window, cx: &gpui::App) -> bool {
        cx.try_global::<super::FrameLeases>().is_some_and(|leases| {
            leases
                .0
                .get(&window.window_handle().window_id())
                .is_some_and(|views| !views.is_empty())
        })
    }

    /// The pulse clock's grid: its epoch and tick, once anything declared.
    pub fn grid(cx: &gpui::App) -> Option<(std::time::Instant, std::time::Duration)> {
        let epoch = cx.try_global::<super::PulseClock>()?.epoch?;
        Some((epoch, super::pulse_tick()))
    }
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

    const CURVES: [CubicBezier; 4] = [
        EASE_OUT_EXPO,
        CubicBezier::new(0.0, 0.0, 0.58, 1.0),
        EASE,
        EASE_STANDARD,
    ];

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
                // CSS `ease-out`: no catalog entry rides it any more, but the
                // solver still answers its reference values.
                "ease-out",
                CubicBezier::new(0.0, 0.0, 0.58, 1.0),
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

    /// The overshoot curves pass their end and come back (a spring's
    /// settle): `eval` lets them, and still lands on 1.
    #[test]
    fn an_overshoot_passes_its_end_and_settles() {
        let peak = (0..=100)
            .map(|i| EASE_OVERSHOOT.eval(i as f32 / 100.0))
            .fold(0.0f32, f32::max);
        assert!(peak > 1.0 && peak < 1.2, "peak {peak}");
        assert_eq!(EASE_OVERSHOOT.eval(1.0), 1.0);
        assert_eq!(EASE_OVERSHOOT.eval(0.0), 0.0);
    }

    /// A delayed spec holds at its start, then moves over its own time;
    /// a 0ms spec lands the instant it starts.
    #[test]
    fn a_delay_holds_then_moves() {
        let ms = Duration::from_millis;
        assert_eq!(ACTIONS_IN.duration(), ms(220));
        assert_eq!(ACTIONS_IN.progress_at(ms(0)), 0.0);
        assert_eq!(ACTIONS_IN.progress_at(ms(100)), 0.0, "still waiting");
        let mid = ACTIONS_IN.progress_at(ms(160));
        assert!(mid > 0.0 && mid < 1.0, "moving {mid}");
        assert_eq!(ACTIONS_IN.progress_at(ms(220)), 1.0);
        assert_eq!(HOVER_SNAP.progress_at(ms(0)), 1.0, "a snap lands at once");
    }

    /// What stood leaves as it goes, for its exit and no longer; the same
    /// one standing on never leaves; a dismissal and reduced motion leave
    /// nothing behind.
    #[test]
    fn an_exit_plays_for_its_spec_and_no_longer() {
        let t0 = Instant::now();
        let ms = |m: u64| t0 + Duration::from_millis(m);
        let same = |a: &u32, b: &u32| a == b;
        let mut exits = Exits::default();
        assert!(exits
            .stage(Some(&1), same, PALETTE_OUT, t0, false)
            .is_none());
        assert!(exits
            .stage(Some(&1), same, PALETTE_OUT, ms(50), false)
            .is_none());
        assert_eq!(
            exits.stage(None, same, PALETTE_OUT, ms(100), false),
            Some((1, ms(100)))
        );
        assert!(exits
            .stage(None, same, PALETTE_OUT, ms(219), false)
            .is_some());
        assert!(
            exits
                .stage(None, same, PALETTE_OUT, ms(220), false)
                .is_none(),
            "done"
        );
        // Replaced: the old one leaves under the new.
        exits.stage(Some(&2), same, PALETTE_OUT, ms(300), false);
        assert_eq!(
            exits.stage(Some(&3), same, PALETTE_OUT, ms(310), false),
            Some((2, ms(310)))
        );
        exits.dismiss();
        assert!(exits
            .stage(None, same, PALETTE_OUT, ms(320), false)
            .is_none());
        let mut reduced = Exits::default();
        reduced.stage(Some(&1), same, PALETTE_OUT, t0, true);
        assert!(reduced
            .stage(None, same, PALETTE_OUT, ms(1), true)
            .is_none());
    }

    #[test]
    fn a_tween_eases_to_its_target_and_retargets_from_where_it_is() {
        let t0 = Instant::now();
        let ms = |m: u64| t0 + Duration::from_millis(m);
        let open = Tween::new(52.0, 286.0, DIALOG_IN, t0);
        assert_eq!(open.value(t0, false), 52.0);
        assert!(open.running(ms(100), false));
        let mid = open.value(ms(100), false);
        assert!(mid > 52.0 && mid < 286.0, "mid-flight {mid}");
        assert_eq!(open.value(ms(200), false), 286.0);
        assert!(!open.running(ms(200), false), "done at the spec's end");

        // Flipped back mid-flight: the new tween starts on screen, not at
        // the far end.
        let back = Tween::retarget(Some(open), 286.0, 52.0, DIALOG_IN, ms(100), false);
        close(back.from, mid, 1e-3, "continuity");
        assert_eq!(back.value(ms(300), false), 52.0);

        // Nothing moving: from the resting value.
        let rest = Tween::retarget(None, 286.0, 52.0, DIALOG_IN, t0, false);
        assert_eq!(rest.from, 286.0);
    }

    #[test]
    fn reduced_motion_snaps_a_tween_to_its_end_state() {
        let t0 = Instant::now();
        let tween = Tween::new(52.0, 286.0, DIALOG_IN, t0);
        assert_eq!(tween.value(t0, true), 286.0);
        assert!(!tween.running(t0, true), "no frame is asked for");
    }

    /// A slot slides in over `SLOT_OPEN` and out over `SLOT_CLOSE`, turns
    /// around mid-flight from where it is, and is shut only once its exit
    /// has run.
    #[test]
    fn a_reveal_slides_open_and_shut_and_turns_around() {
        let t0 = Instant::now();
        let ms = |m: u64| t0 + Duration::from_millis(m);
        let mut slot = Reveal::opening(SLOT_OPEN, SLOT_CLOSE, t0);
        assert_eq!(slot.value(t0, false), 0.0, "it starts shut");
        let mid = slot.value(ms(60), false);
        assert!(mid > 0.0 && mid < 1.0 && slot.running(ms(60), false));
        assert!(mid > 0.5, "an ease-out is past halfway at a quarter: {mid}");

        slot.set(false, ms(60), false);
        close(
            slot.value(ms(60), false),
            mid,
            1e-4,
            "turns around on screen",
        );
        assert!(
            !slot.shut(ms(60), false),
            "a closing slot outlives its close"
        );
        assert!(slot.shut(ms(60 + theme::MOTION_SLOT_CLOSE_MS), false));
        assert_eq!(slot.value(ms(60 + theme::MOTION_SLOT_CLOSE_MS), false), 0.0);

        slot.set(true, ms(400), false);
        slot.land(ms(410));
        assert_eq!(slot.value(ms(410), false), 1.0, "landed where it was going");
        assert!(!slot.running(ms(410), false));

        // At rest from the start: no entrance.
        let rest = Reveal::open(SLOT_OPEN, SLOT_CLOSE, t0);
        assert_eq!(rest.value(t0, false), 1.0);
        assert!(!rest.running(t0, false));
    }

    #[test]
    fn reduced_motion_snaps_a_reveal() {
        let t0 = Instant::now();
        let mut slot = Reveal::opening(SLOT_OPEN, SLOT_CLOSE, t0);
        assert_eq!(slot.value(t0, true), 1.0);
        assert!(!slot.running(t0, true));
        slot.set(false, t0, true);
        assert!(slot.shut(t0, true), "closed at once: the slot can go");
    }

    #[test]
    fn the_hover_blend_ramps_and_reverses_without_a_jump() {
        let mut fades = HoverFades::default();
        let key = SharedString::from("row");
        let t0 = Instant::now();
        let ms = |m: u64| t0 + Duration::from_millis(m);

        fades.set_at(&key, true, false, t0);
        assert_eq!(fades.value_at("row", t0), 0.0);
        let rising = fades.value_at("row", ms(20));
        assert!(rising > 0.0 && rising < 1.0, "mid-flight {rising}");
        assert_eq!(fades.value_at("row", ms(60)), 1.0, "Claude's 60ms");

        fades.set_at(&key, true, false, t0);
        let at_flip = fades.value_at("row", ms(20));
        fades.set_at(&key, false, false, ms(20));
        close(fades.value_at("row", ms(20)), at_flip, 1e-4, "continuity");
        assert!(fades.value_at("row", ms(40)) < at_flip, "falls back");
        assert_eq!(fades.value_at("row", ms(80)), 0.0, "lands at rest");
    }

    /// A row's hover lands the frame the pointer arrives, and leaves as
    /// quickly, asking for no frame of its own.
    #[test]
    fn a_snapped_hover_lands_at_once() {
        let mut fades = HoverFades::default();
        let key = SharedString::from("row");
        let t0 = Instant::now();
        fades.set_with(&key, true, false, t0, HOVER_SNAP, None);
        assert_eq!(fades.value_at("row", t0), 1.0);
        assert!(!fades.tick_at(t0), "no frame");
        fades.set_with(&key, false, false, t0, HOVER_SNAP, None);
        assert_eq!(fades.value_at("row", t0), 0.0);
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

        // Unmounted mid-hover: its leave never comes; unread past
        // `HOVER_FORGET`, it is forgotten.
        fades.set_at(&key, true, false, ms(500));
        fades.tick_at(ms(516));
        assert_eq!(fades.entries.len(), 1, "a frame unread is no unmount");
        fades.tick_at(ms(500) + HOVER_FORGET + Duration::from_millis(16));
        assert!(fades.entries.is_empty(), "unread entry evicted");
    }

    /// A hovered link in a cached transcript goes unread while the rest of
    /// the window draws: frames pass without it, and when the transcript
    /// draws again (the next streamed line) it still reads hovered.
    #[test]
    fn a_hovered_blend_survives_frames_its_cached_view_skips() {
        let mut fades = HoverFades::default();
        let key = SharedString::from("link");
        let t0 = Instant::now();
        let ms = |m: u64| t0 + Duration::from_millis(m);
        fades.set_with(&key, true, false, t0, HOVER_FADE, None);
        assert_eq!(fades.value_at("link", ms(150)), 1.0);
        for frame in 1..=30 {
            fades.tick_at(ms(150 + frame * 33));
        }
        assert_eq!(
            fades.value_at("link", ms(1_200)),
            1.0,
            "still under the pointer, still underlined"
        );
    }

    /// The views that painted a blend still mid-flight are named, once each,
    /// so the root can draw them again with its next frame.
    #[test]
    fn the_hover_tick_names_the_views_mid_flight() {
        let mut fades = HoverFades::default();
        let t0 = Instant::now();
        let ms = |m: u64| t0 + Duration::from_millis(m);
        let (a, b) = (EntityId::from(1u64), EntityId::from(2u64));
        fades.set_with(&"one".into(), true, false, t0, HOVER_FADE, Some(a));
        fades.set_with(&"two".into(), true, false, t0, HOVER_FADE, Some(a));
        fades.set_with(&"three".into(), true, false, ms(100), HOVER_FADE, Some(b));
        assert!(fades.tick_at(ms(50)));
        assert_eq!(fades.fading, [a, b]);
        assert!(fades.tick_at(ms(150)), "b is still fading");
        assert_eq!(fades.fading, [b]);
        assert!(!fades.tick_at(ms(300)));
        assert!(fades.fading.is_empty());
    }

    #[test]
    fn mix_blends_in_premultiplied_srgb() {
        let rest: Hsla = gpui::rgb(theme::FRAME).into();
        let hover: Hsla = gpui::rgb(theme::RAISED).into();
        assert_eq!(mix(rest, hover, 0.0), rest);
        assert_eq!(mix(rest, hover, 1.0), hover);
        let wash = gpui::transparent_black();
        let half = mix(wash, hover, 0.5);
        close(half.a, 0.5, 1e-4, "alpha ramps");
        let (h, half) = (Rgba::from(hover), Rgba::from(half));
        close(half.r, h.r, 1e-3, "the wash keeps its colour");
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
        testing::drive();
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
        // Its last declared wake (the next tick) passes undeclared.
        cx.executor().advance_clock(pulse_tick() * 2);
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

    struct Switch {
        on: bool,
        drawn: std::rc::Rc<std::cell::Cell<f32>>,
    }

    impl Render for Switch {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let drawn = self.drawn.clone();
            let fold = settled("switch", self.on, FADE_QUICK, move |value| {
                drawn.set(value);
                div().size_4()
            });
            div().child(fold)
        }
    }

    fn switch(
        cx: &mut TestAppContext,
        on: bool,
    ) -> (
        gpui::Entity<Switch>,
        std::rc::Rc<std::cell::Cell<f32>>,
        &mut gpui::VisualTestContext,
    ) {
        let drawn = std::rc::Rc::new(std::cell::Cell::new(-1.0));
        let seen = drawn.clone();
        let (view, cx) = cx.add_window_view(move |_, _| Switch { on, drawn: seen });
        cx.run_until_parked();
        (view, drawn, cx)
    }

    fn flip(view: &gpui::Entity<Switch>, on: bool, cx: &mut gpui::VisualTestContext) {
        view.update(cx, |view, cx| {
            view.on = on;
            cx.notify();
        });
        cx.run_until_parked();
    }

    fn next_frame(cx: &mut gpui::VisualTestContext) -> usize {
        let frames = cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        frames
    }

    /// A settled element lands on the state it is first drawn in; a flip
    /// eases over its spec with frames only while it moves, and a flip back
    /// mid-flight turns around from where it is.
    #[gpui::test]
    fn a_settled_element_lands_on_first_paint_and_eases_a_flip(cx: &mut TestAppContext) {
        testing::drive();
        let (view, drawn, cx) = switch(cx, true);
        assert_eq!(drawn.get(), 1.0, "first paint lands open");
        assert_eq!(next_frame(cx), 0, "and asks for nothing");

        flip(&view, false, cx);
        cx.executor().advance_clock(Duration::from_millis(75));
        assert!(next_frame(cx) > 0, "mid-flight asks for frames");
        let mid = drawn.get();
        assert!(mid > 0.0 && mid < 1.0, "mid-flight {mid}");

        flip(&view, true, cx);
        close(drawn.get(), mid, 1e-3, "turns around from where it is");
        cx.executor()
            .advance_clock(Duration::from_millis(theme::MOTION_FADE_QUICK_MS));
        next_frame(cx);
        assert_eq!(drawn.get(), 1.0);
        assert_eq!(next_frame(cx), 0, "settled: no frames");
    }

    /// Reduced motion lands every flip.
    #[gpui::test]
    fn reduced_motion_lands_a_settled_flip(cx: &mut TestAppContext) {
        testing::drive();
        cx.update(|cx| cx.set_reduce_motion(true));
        let (view, drawn, cx) = switch(cx, false);
        flip(&view, true, cx);
        assert_eq!(drawn.get(), 1.0);
        assert_eq!(next_frame(cx), 0);
    }

    struct Entering {
        mounted: bool,
        left: Option<Instant>,
        drawn: std::rc::Rc<std::cell::Cell<f32>>,
    }

    impl Render for Entering {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let drawn = self.drawn.clone();
            let face = move |el: gpui::Div, t: f32| {
                drawn.set(t);
                el
            };
            div().when(self.mounted, |root| match self.left {
                None => root.child(entrance("entering", DIALOG_IN, div().size_4(), face)),
                Some(left) => root.child(Played {
                    origin: Origin::At(left),
                    spec: TOAST_OUT,
                    element: div().size_4(),
                    face,
                }),
            })
        }
    }

    fn entering(
        cx: &mut TestAppContext,
        left: Option<Instant>,
    ) -> (
        gpui::Entity<Entering>,
        std::rc::Rc<std::cell::Cell<f32>>,
        &mut gpui::VisualTestContext,
    ) {
        let drawn = std::rc::Rc::new(std::cell::Cell::new(-1.0));
        let seen = drawn.clone();
        let (view, cx) = cx.add_window_view(move |_, _| Entering {
            mounted: true,
            left,
            drawn: seen,
        });
        cx.run_until_parked();
        (view, drawn, cx)
    }

    /// An entrance plays from its element's first mount on the executor's
    /// clock, asking for frames only while it plays; unmounted and mounted
    /// again, it plays again.
    #[gpui::test]
    fn an_entrance_plays_from_its_first_mount_and_then_asks_for_nothing(cx: &mut TestAppContext) {
        testing::drive();
        let (view, drawn, cx) = entering(cx, None);
        assert_eq!(drawn.get(), 0.0, "its first frame is its first");
        cx.executor().advance_clock(Duration::from_millis(70));
        assert!(next_frame(cx) > 0, "it asks for the next frame");
        let mid = drawn.get();
        assert!(mid > 0.0 && mid < 1.0, "mid-flight at 70ms: {mid}");
        cx.executor()
            .advance_clock(Duration::from_millis(theme::MOTION_DIALOG_IN_MS));
        next_frame(cx);
        assert_eq!(drawn.get(), 1.0, "landed");
        assert_eq!(next_frame(cx), 0, "and asks for nothing");

        for mounted in [false, true] {
            view.update(cx, |view, cx| {
                view.mounted = mounted;
                cx.notify();
            });
            cx.run_until_parked();
        }
        assert_eq!(drawn.get(), 0.0, "mounted again, it plays again");
    }

    /// An exit plays from the instant its subject went, not from its mount.
    #[gpui::test]
    fn an_exit_plays_from_the_instant_its_subject_went(cx: &mut TestAppContext) {
        testing::drive();
        let left = cx.executor().now() - Duration::from_millis(50);
        let (_view, drawn, cx) = entering(cx, Some(left));
        let at = drawn.get();
        assert!(at > 0.0 && at < 1.0, "50ms into its 200ms: {at}");
        cx.executor()
            .advance_clock(Duration::from_millis(theme::MOTION_TOAST_OUT_MS));
        assert!(next_frame(cx) > 0);
        assert_eq!(drawn.get(), 1.0, "gone");
        assert_eq!(next_frame(cx), 0, "and asks for nothing");
    }

    /// Reduced motion lands an entrance on its first frame.
    #[gpui::test]
    fn reduced_motion_lands_an_entrance(cx: &mut TestAppContext) {
        testing::drive();
        cx.update(|cx| cx.set_reduce_motion(true));
        let (_view, drawn, cx) = entering(cx, None);
        assert_eq!(drawn.get(), 1.0);
        assert_eq!(next_frame(cx), 0);
    }

    /// A view that draws its frames through another notifies that one for
    /// its animation's next frame, not itself.
    #[gpui::test]
    fn an_animation_draws_its_frames_through_the_view_named(cx: &mut TestAppContext) {
        testing::drive();
        let (view, drawn, cx) = entering(cx, None);
        let through = cx.update(|_, cx| cx.new(|_| ()));
        let notified = std::rc::Rc::new(std::cell::Cell::new(0));
        let seen = notified.clone();
        cx.update(|_, cx| {
            cx.observe(&through, move |_, _| seen.set(seen.get() + 1))
                .detach()
        });
        draw_frames_through(view.entity_id(), Some(through.entity_id()));
        // The frame its first draw asked for redraws it, and that draw asks
        // through the view named.
        next_frame(cx);
        notified.set(0);
        let before = drawn.get();
        cx.executor().advance_clock(Duration::from_millis(30));
        assert!(next_frame(cx) > 0);
        assert_eq!(notified.get(), 1, "the frame went to the view named");
        assert_eq!(drawn.get(), before, "and did not redraw the drawing view");
        draw_frames_through(view.entity_id(), None);
        assert_eq!(frames_through(view.entity_id()), view.entity_id());
    }

    struct Rolling {
        face: SharedString,
        built: std::rc::Rc<std::cell::RefCell<Vec<SharedString>>>,
    }

    impl Render for Rolling {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let built = self.built.clone();
            built.borrow_mut().clear();
            div().child(morph("roll", self.face.clone(), move |face| {
                built.borrow_mut().push(face.clone());
                div().child(face).into_any_element()
            }))
        }
    }

    fn rolling(
        cx: &mut TestAppContext,
    ) -> (
        gpui::Entity<Rolling>,
        std::rc::Rc<std::cell::RefCell<Vec<SharedString>>>,
        &mut gpui::VisualTestContext,
    ) {
        let built = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = built.clone();
        let (view, cx) = cx.add_window_view(move |_, _| Rolling {
            face: "Reading".into(),
            built: seen,
        });
        cx.run_until_parked();
        (view, built, cx)
    }

    fn reword(view: &gpui::Entity<Rolling>, face: &str, cx: &mut gpui::VisualTestContext) {
        let face = SharedString::from(face.to_owned());
        view.update(cx, |view, cx| {
            view.face = face;
            cx.notify();
        });
        cx.run_until_parked();
    }

    /// A label lands in place on its first draw; new words roll in over the
    /// old for `MORPH`, asking for frames only while they do.
    #[gpui::test]
    fn a_label_rolls_its_new_words_in_over_the_old(cx: &mut TestAppContext) {
        testing::drive();
        let (view, built, cx) = rolling(cx);
        assert_eq!(*built.borrow(), ["Reading"], "first draw: one face");
        assert_eq!(next_frame(cx), 0, "and no frame");
        reword(&view, "Writing", cx);
        assert_eq!(*built.borrow(), ["Writing", "Reading"], "both faces");
        cx.executor().advance_clock(Duration::from_millis(90));
        assert!(next_frame(cx) > 0, "mid-roll asks for frames");
        cx.executor()
            .advance_clock(Duration::from_millis(theme::MOTION_MORPH_MS));
        next_frame(cx);
        assert_eq!(*built.borrow(), ["Writing"], "the old face is gone");
        assert_eq!(next_frame(cx), 0, "at rest: no frame");
    }

    /// Reduced motion swaps the words at once.
    #[gpui::test]
    fn reduced_motion_swaps_a_label_at_once(cx: &mut TestAppContext) {
        testing::drive();
        cx.update(|cx| cx.set_reduce_motion(true));
        let (view, built, cx) = rolling(cx);
        reword(&view, "Writing", cx);
        assert_eq!(*built.borrow(), ["Writing"]);
        assert_eq!(next_frame(cx), 0);
    }

    struct Folding {
        shown: f32,
        height: std::rc::Rc<std::cell::Cell<f32>>,
    }

    impl Render for Folding {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let height = self.height.clone();
            div()
                .flex()
                .flex_col()
                .on_children_prepainted(move |bounds, _, _| {
                    height.set(f32::from(bounds[0].size.height));
                })
                .child(fold_body("fold", self.shown, div().w(px(10.)).h(px(40.))))
        }
    }

    /// A fold's body is clipped to its share of what it measures: whole at
    /// rest, half at half, nothing shut. It asks for no frame of its own
    /// (its owner holds the clock).
    #[gpui::test]
    fn a_fold_clips_its_body_to_how_far_open_it_is(cx: &mut TestAppContext) {
        testing::drive();
        let height = std::rc::Rc::new(std::cell::Cell::new(-1.0));
        let seen = height.clone();
        let (view, cx) = cx.add_window_view(move |_, _| Folding {
            shown: 1.0,
            height: seen,
        });
        cx.run_until_parked();
        assert_eq!(height.get(), 40.0, "open at rest");
        for (shown, want) in [(0.5, 20.0), (0.0, 0.0), (1.0, 40.0)] {
            view.update(cx, |view, cx| {
                view.shown = shown;
                cx.notify();
            });
            cx.run_until_parked();
            assert_eq!(height.get(), want, "at {shown}");
            assert_eq!(next_frame(cx), 0, "no frame of its own");
        }
    }

    /// A press squishes over `PRESS_IN` and springs back over `PRESS_OUT`;
    /// a release of a button never pressed does nothing, and reduced motion
    /// never squishes.
    #[gpui::test]
    fn a_press_squishes_and_springs_back(cx: &mut TestAppContext) {
        testing::drive();
        let (_view, _drawn, cx) = switch(cx, true);
        let key = SharedString::from("press-test");
        let read = |cx: &mut gpui::VisualTestContext| {
            let now = cx.executor().now();
            HOVER_FADES.with(|fades| fades.borrow_mut().value_at(&press_key(&key), now))
        };
        cx.update(|window, cx| press(&key, true, window, cx));
        assert_eq!(read(cx), 0.0, "it starts at rest");
        cx.executor()
            .advance_clock(Duration::from_millis(theme::MOTION_DUR_FAST_MS));
        assert_eq!(read(cx), 1.0, "squished");
        cx.update(|window, cx| press(&key, false, window, cx));
        cx.executor().advance_clock(Duration::from_millis(100));
        let back = read(cx);
        assert!(back < 1.0, "springing back: {back}");
        cx.executor()
            .advance_clock(Duration::from_millis(theme::MOTION_DUR_SLOW_MS));
        assert_eq!(read(cx), 0.0, "at rest");
        cx.update(|window, cx| press(&key, false, window, cx));
        assert_eq!(read(cx), 0.0, "a stray release turns nothing");

        cx.update(|_, cx| cx.set_reduce_motion(true));
        cx.update(|window, cx| press(&key, true, window, cx));
        assert_eq!(read(cx), 0.0, "reduced motion: no squish");
    }

    /// Reduced motion holds a loop at its start and leases nothing.
    #[gpui::test]
    fn reduced_motion_holds_a_loop_at_its_start(cx: &mut TestAppContext) {
        testing::drive();
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
