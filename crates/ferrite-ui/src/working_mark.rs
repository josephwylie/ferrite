//! The working mark: four soft white orbs moving exactly as Claude
//! Desktop's four-dot working glyph does (`code.loop`).
//!
//! At rest the orbs sit on a square. Every 625ms one diagonal pair swaps
//! corners: the two orbs turn half a revolution about the centre together,
//! pulling inward, so that halfway through all four stand in one line. The
//! TL/BR pair turns counter-clockwise (`/` at its middle), the TR/BL pair
//! clockwise (`\`), alternating, four swaps in a `MOTION_WORKING_MARK_MS`
//! (2.5s) loop. The orb that starts on the right swells about 5% as it
//! comes forward, and its partner shrinks as it goes back.
//!
//! The motion is [`SWAP`], the decoded swap sampled at Claude's frames,
//! interpolated with a cubic every frame. It is drawn at the display's rate
//! (`motion::frame_loop`), never stepped at Claude's 30fps, and it leaves
//! out Claude's snap to its rest frame where the loop wraps. Claude's loop
//! never quite rests, because consecutive swaps overlap. Ours starts at its
//! stillest instant (`FIRST_SWAP_MS`), each orb within about 1.6% of the
//! box of its rest corner. Reduced motion draws the rest square itself.
//!
//! Each orb is one sprite: a white core with a soft rim and a gentle two-layer
//! halo, rasterised once per size from [`orb_svg`] and placed by a transform
//! every frame, so it moves at sub-pixel precision without being laid out
//! again. The halo reaches past the mark's box.

use std::sync::LazyLock;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    canvas, div, point, px, rgb, size, AnyElement, App, Bounds, Pixels, ScaledPixels,
    TransformationMatrix, Window,
};

use crate::motion;
use crate::theme;

/// One orb: its centre and radius, as fractions of the mark's box (the
/// centre of the box is `(0.5, 0.5)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orb {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

/// An orb's distance from the box's centre, at rest.
const REST_REACH: f32 = 0.3508;
/// An orb's radius, at rest.
const REST_RADIUS: f32 = 0.0949;
/// Half the rest square's side: `REST_REACH / √2`.
const REST_CORNER: f32 = REST_REACH * std::f32::consts::FRAC_1_SQRT_2;

/// The rest square: TL, TR, BR, BL.
pub const REST: [Orb; 4] = [
    rest(-1.0, -1.0),
    rest(1.0, -1.0),
    rest(1.0, 1.0),
    rest(-1.0, 1.0),
];

const fn rest(sx: f32, sy: f32) -> Orb {
    Orb {
        x: 0.5 + sx * REST_CORNER,
        y: 0.5 + sy * REST_CORNER,
        r: REST_RADIUS,
    }
}

/// One swap every quarter loop.
const SWAP_MS: f32 = theme::MOTION_WORKING_MARK_MS as f32 / 4.0;

/// The first swap's mid-point, in the loop. Claude's loop has its first
/// swap's mid-point at 256.7ms; ours starts 17ms earlier in Claude's loop,
/// at the instant its orbs come closest to the rest square.
const FIRST_SWAP_MS: f32 = 273.7;

/// The canonical swap, decoded from Claude's sprite (the doc's table). Each
/// row is `[τ, front a, front b, front r, back a, back b, back r]`:
///
/// - `τ` is in Claude's 30fps frames from the swap's mid-point;
/// - `a` is how far along the start corner's direction from the centre;
/// - `b` is how far along that direction turned a quarter toward travel;
/// - `r` is the orb's radius.
///
/// The front orb is the one that starts on the right (BR or TR).
const SWAP: [[f32; 7]; 27] = [
    [-18.75, 0.3508, 0.0000, 0.0949, 0.3508, 0.0000, 0.0949],
    [-12.0, 0.3521, 0.0004, 0.0952, 0.3497, 0.0003, 0.0939],
    [-10.0, 0.3511, 0.0018, 0.0958, 0.3453, 0.0016, 0.0935],
    [-8.0, 0.3445, 0.0058, 0.0967, 0.3340, 0.0047, 0.0931],
    [-7.0, 0.3374, 0.0103, 0.0972, 0.3238, 0.0088, 0.0927],
    [-6.0, 0.3256, 0.0174, 0.0978, 0.3086, 0.0154, 0.0923],
    [-5.0, 0.3067, 0.0282, 0.0984, 0.2866, 0.0250, 0.0917],
    [-4.0, 0.2773, 0.0438, 0.0990, 0.2551, 0.0388, 0.0910],
    [-3.0, 0.2334, 0.0649, 0.0995, 0.2120, 0.0575, 0.0904],
    [-2.0, 0.1717, 0.0918, 0.0999, 0.1552, 0.0817, 0.0898],
    [-1.5, 0.1337, 0.1074, 0.1000, 0.1211, 0.0955, 0.0896],
    [-1.0, 0.0914, 0.1237, 0.1000, 0.0832, 0.1099, 0.0895],
    [-0.5, 0.0453, 0.1399, 0.1000, 0.0420, 0.1243, 0.0894],
    [0.0, -0.0033, 0.1548, 0.0999, -0.0017, 0.1377, 0.0895],
    [0.5, -0.0531, 0.1674, 0.0997, -0.0467, 0.1494, 0.0896],
    [1.0, -0.1021, 0.1766, 0.0995, -0.0916, 0.1584, 0.0898],
    [1.5, -0.1485, 0.1816, 0.0992, -0.1350, 0.1641, 0.0901],
    [2.0, -0.1908, 0.1822, 0.0989, -0.1754, 0.1660, 0.0905],
    [3.0, -0.2588, 0.1700, 0.0980, -0.2427, 0.1579, 0.0914],
    [4.0, -0.3035, 0.1449, 0.0971, -0.2894, 0.1368, 0.0923],
    [5.0, -0.3286, 0.1152, 0.0963, -0.3184, 0.1099, 0.0931],
    [6.0, -0.3412, 0.0869, 0.0958, -0.3346, 0.0830, 0.0938],
    [8.0, -0.3497, 0.0441, 0.0951, -0.3472, 0.0414, 0.0946],
    [10.0, -0.3508, 0.0197, 0.0950, -0.3510, 0.0179, 0.0951],
    [12.0, -0.3508, 0.0082, 0.0951, -0.3516, 0.0065, 0.0952],
    [15.0, -0.3509, 0.0019, 0.0951, -0.3516, 0.0004, 0.0950],
    [18.75, -0.3508, 0.0000, 0.0949, -0.3508, 0.0000, 0.0949],
];

/// Claude's sprite frame, in ms.
const CLAUDE_FRAME_MS: f32 = 1000.0 / 30.0;

/// Column `column` of [`SWAP`] at `tau` frames: a cubic Hermite through
/// the rows, its slope at each row weighted from its neighbours' secants
/// (rows are unevenly spaced) and flat at both ends, where the orbs rest.
/// Continuous in value and slope, so the motion has no steps.
fn swap_at(tau: f32, column: usize) -> f32 {
    let last = SWAP.len() - 1;
    let tau = tau.clamp(SWAP[0][0], SWAP[last][0]);
    let i = SWAP[..last]
        .iter()
        .rposition(|row| row[0] <= tau)
        .unwrap_or(0);
    let secant = |k: usize| (SWAP[k + 1][column] - SWAP[k][column]) / (SWAP[k + 1][0] - SWAP[k][0]);
    let slope = |k: usize| {
        if k == 0 || k == last {
            return 0.0;
        }
        let (before, after) = (SWAP[k][0] - SWAP[k - 1][0], SWAP[k + 1][0] - SWAP[k][0]);
        (after * secant(k - 1) + before * secant(k)) / (before + after)
    };
    let h = SWAP[i + 1][0] - SWAP[i][0];
    let t = (tau - SWAP[i][0]) / h;
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * SWAP[i][column]
        + (t3 - 2.0 * t2 + t) * h * slope(i)
        + (-2.0 * t3 + 3.0 * t2) * SWAP[i + 1][column]
        + (t3 - t2) * h * slope(i + 1)
}

/// The four orbs at `ms` into the loop (any real; periodic): TL, TR, BR,
/// BL by the corner each started the loop on.
pub fn orbs_at(ms: f32) -> [Orb; 4] {
    let mut orbs = REST;
    // (the pair's front and back slots, the turn's sense: -1 is
    // counter-clockwise on screen)
    for (pair, (front, back), sense) in [(0, (2, 0), -1.0), (1, (1, 3), 1.0)] {
        // Each pair swaps every other quarter; its windows of ±one
        // quarter around its mid-points tile the loop.
        let first = FIRST_SWAP_MS + SWAP_MS * pair as f32;
        let since = ms - first + SWAP_MS;
        let tau_ms = since.rem_euclid(2.0 * SWAP_MS) - SWAP_MS;
        let tau = tau_ms / CLAUDE_FRAME_MS;
        // Each swap ends with the pair on each other's corners: after an
        // odd number of them, each orb is the one that began on the other.
        let swapped = (since / (2.0 * SWAP_MS)).floor().rem_euclid(2.0) == 1.0;
        for (start, columns) in [(front, 1), (back, 4)] {
            let corner = REST[start];
            let slot = match (swapped, start == front) {
                (false, _) => start,
                (true, true) => back,
                (true, false) => front,
            };
            let (vx, vy) = ((corner.x - 0.5) / REST_REACH, (corner.y - 0.5) / REST_REACH);
            let (ux, uy) = (-vy * sense, vx * sense);
            let a = swap_at(tau, columns);
            let b = swap_at(tau, columns + 1);
            orbs[slot] = Orb {
                x: 0.5 + a * vx + b * ux,
                y: 0.5 + a * vy + b * uy,
                r: swap_at(tau, columns + 2),
            };
        }
    }
    orbs
}

// ---------------------------------------------------------------- the orb

/// How far an orb's sprite reaches, in orb radii: past it the halo is
/// below one step of an 8-bit alpha.
const ORB_EXTENT: f32 = 6.0;

/// The ground the orb's light is matched on (`#191919`, the reference
/// renderer's and near the app's grounds).
const ORB_GROUND: f32 = 0x19 as f32 / 255.0;

/// An orb's alpha at `d` radii from its centre. This is the approved
/// renderer's light: a solid core whose rim rolls off over a tenth of the
/// radius, under two gentle gaussian halos. The renderer screens that light
/// onto the ground in linear light; gpui blends in sRGB, so the alpha is
/// the one that gives the same sRGB pixel over `ORB_GROUND`.
fn orb_alpha(d: f32) -> f32 {
    let core = 1.0 / (1.0 + ((d - 1.0) / 0.1).exp());
    let glow = 0.30 * (-(d / 1.45).powi(2)).exp() + 0.06 * (-(d / 2.6).powi(2)).exp();
    let halo = 1.0 - (-glow).exp();
    let light = core + (1.0 - core) * halo;
    let linear = |c: f32| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let srgb = |c: f32| {
        if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    };
    let ground = linear(ORB_GROUND);
    let pixel = srgb(ground + (1.0 - ground) * light);
    ((pixel - ORB_GROUND) / (1.0 - ORB_GROUND)).clamp(0.0, 1.0)
}

/// The orb as an SVG alpha mask: a radial gradient out to `ORB_EXTENT`
/// radii, its stops dense across the rim.
fn orb_svg() -> String {
    let mut stops = String::new();
    let mut d: f32 = 0.0;
    while d < ORB_EXTENT {
        stops += &format!(
            r##"<stop offset="{:.4}" stop-color="#fff" stop-opacity="{:.4}"/>"##,
            d / ORB_EXTENT,
            orb_alpha(d)
        );
        d += if (0.6..1.5).contains(&d) { 0.025 } else { 0.1 };
    }
    stops += r##"<stop offset="1" stop-color="#fff" stop-opacity="0"/>"##;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><defs><radialGradient id="o" cx="50" cy="50" r="50" gradientUnits="userSpaceOnUse">{stops}</radialGradient></defs><circle cx="50" cy="50" r="50" fill="url(#o)"/></svg>"##
    )
}

static ORB_SVG: LazyLock<String> = LazyLock::new(orb_svg);

/// The orb's key in gpui's sprite atlas (no asset: the bytes are
/// `ORB_SVG`).
const ORB_PATH: &str = "ferrite://working-orb";

/// Paint `orbs` over `bounds` in `ink`: one sprite each, rasterised once
/// for this size and moved and scaled by its transform.
fn paint_orbs(bounds: Bounds<Pixels>, orbs: &[Orb; 4], ink: u32, window: &mut Window, cx: &App) {
    let scale = window.scale_factor();
    let side = f32::from(bounds.size.width);
    // The sprite, centred on the box at a whole device pixel and an even
    // number of them across, so gpui places it exactly; each orb's
    // transform carries it from there.
    let centre = point(
        ((f32::from(bounds.origin.x) + side / 2.0) * scale).round(),
        ((f32::from(bounds.origin.y) + side / 2.0) * scale).round(),
    );
    let half = (ORB_EXTENT * REST_RADIUS * side * scale).ceil();
    let sprite = Bounds::new(
        point(px((centre.x - half) / scale), px((centre.y - half) / scale)),
        size(px(2.0 * half / scale), px(2.0 * half / scale)),
    );
    let origin = (
        f32::from(bounds.origin.x) * scale,
        f32::from(bounds.origin.y) * scale,
    );
    let at = |x: f32, y: f32| point(ScaledPixels(x), ScaledPixels(y));
    for orb in orbs {
        let to = at(
            origin.0 + orb.x * side * scale,
            origin.1 + orb.y * side * scale,
        );
        let grow = orb.r / REST_RADIUS;
        let transform = TransformationMatrix::unit()
            .translate(to)
            .scale(size(grow, grow))
            .translate(at(-centre.x, -centre.y));
        let _ = window.paint_svg(
            sprite,
            ORB_PATH.into(),
            Some(ORB_SVG.as_bytes()),
            transform,
            rgb(ink).into(),
            cx,
        );
    }
}

// ---------------------------------------------------------------- the mark

/// The working mark, `size` square (the halo reaches past it), in `ink`.
/// Drawn by the loops overlay (`loops_overlay::hosted`) at the display's
/// rate while it is on screen, and not at all once it leaves the window or
/// is clipped out of sight. Under reduced motion it is the rest square.
pub fn working_mark(size: f32, ink: u32) -> AnyElement {
    let mark = WorkingMark { size, ink };
    crate::loops_overlay::hosted(move || mark.into_any_element()).into_any_element()
}

#[derive(IntoElement, Clone, Copy)]
struct WorkingMark {
    size: f32,
    ink: u32,
}

#[cfg(test)]
thread_local! {
    /// The orbs the mark last drew.
    static DRAWN: std::cell::Cell<Option<[Orb; 4]>> = const { std::cell::Cell::new(None) };
}

impl RenderOnce for WorkingMark {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let period = Duration::from_millis(theme::MOTION_WORKING_MARK_MS);
        let frame = motion::frame_loop(period, motion::LoopClock::Script, cx);
        let orbs = frame.map_or(REST, |frame| {
            orbs_at(frame.phase * theme::MOTION_WORKING_MARK_MS as f32)
        });
        let leases = frame.is_some_and(|frame| frame.leases);
        #[cfg(test)]
        DRAWN.with(|drawn| drawn.set(Some(orbs)));
        let (side, ink) = (self.size, self.ink);
        div()
            .debug_selector(|| "working-mark".into())
            .flex_shrink_0()
            .size(px(side))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        // The next frame, only while the orbs can be seen:
                        // a mark clipped out of its scroller asks for none.
                        let reach = bounds.dilate(px(side * 0.5));
                        if leases && window.content_mask().bounds.intersects(&reach) {
                            motion::lease_frame(window, cx);
                        }
                    },
                    move |bounds, _, window, cx| {
                        // Laid out invisibly for the overlay: nothing to paint.
                        if window.element_opacity() > 0.0 {
                            paint_orbs(bounds, &orbs, ink, window, cx);
                        }
                    },
                )
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    fn distance(a: Orb, b: Orb) -> f32 {
        (a.x - b.x).hypot(a.y - b.y)
    }

    fn farthest(a: &[Orb; 4], b: &[Orb; 4]) -> f32 {
        a.iter()
            .zip(b)
            .map(|(p, q)| distance(*p, *q))
            .fold(0.0, f32::max)
    }

    /// The table's ends are the rest square: an orb ends a swap on its
    /// partner's corner.
    #[test]
    fn a_swap_starts_and_ends_on_the_square() {
        for column in [1, 4] {
            assert_eq!(swap_at(-18.75, column), REST_REACH);
            assert_eq!(swap_at(18.75, column), -REST_REACH);
            assert_eq!(swap_at(-18.75, column + 1), 0.0);
            assert_eq!(swap_at(18.75, column + 2), REST_RADIUS);
        }
        // Through every row, exactly.
        for row in SWAP {
            assert!((swap_at(row[0], 1) - row[1]).abs() < 1e-5);
            assert!((swap_at(row[0], 5) - row[5]).abs() < 1e-5);
        }
    }

    /// t = 0 is the loop's stillest instant: every orb within 2% of the box
    /// of its rest corner, where reduced motion draws it exactly.
    #[test]
    fn the_loop_starts_on_the_rest_square() {
        let drift = farthest(&orbs_at(0.0), &REST);
        assert!(drift < 0.02, "{drift}");
        // The rest square is a square about the centre.
        for orb in REST {
            assert!(((orb.x - 0.5).abs() - REST_CORNER).abs() < 1e-6);
            assert!(((orb.y - 0.5).abs() - REST_CORNER).abs() < 1e-6);
            assert_eq!(orb.r, REST_RADIUS);
        }
        // Nowhere in the loop is it nearer the square (sampled at 1ms).
        let loop_ms = theme::MOTION_WORKING_MARK_MS as usize;
        let stillest = (0..loop_ms)
            .map(|ms| farthest(&orbs_at(ms as f32), &REST))
            .fold(f32::MAX, f32::min);
        assert!(drift - stillest < 2e-3, "{drift} against {stillest}");
    }

    /// The loop repeats every `MOTION_WORKING_MARK_MS` and is continuous
    /// everywhere, the wrap included: no step between instants 1ms apart.
    #[test]
    fn the_loop_is_periodic_and_continuous() {
        let loop_ms = theme::MOTION_WORKING_MARK_MS as f32;
        assert_eq!(loop_ms, 2_500.0);
        for ms in 0..2_500 {
            let ms = ms as f32;
            let here = orbs_at(ms);
            assert!(farthest(&here, &orbs_at(ms + loop_ms)) < 1e-4, "at {ms}");
            assert!(
                farthest(&here, &orbs_at(ms - 3.0 * loop_ms)) < 1e-4,
                "at {ms}"
            );
            assert!(
                farthest(&here, &orbs_at(ms + 1.0)) < 0.006,
                "a jump at {ms}"
            );
        }
        assert!(farthest(&orbs_at(loop_ms - 0.5), &orbs_at(0.0)) < 0.006);
    }

    /// At each swap's mid-point the four orbs stand in one line: `/` while
    /// the TL/BR pair swaps, `\` while the TR/BL pair does. The front orb
    /// swells, the back one shrinks.
    #[test]
    fn each_swap_lines_the_orbs_up_on_its_diagonal() {
        for swap in 0..4 {
            let orbs = orbs_at(FIRST_SWAP_MS + SWAP_MS * swap as f32);
            for orb in orbs {
                let off = if swap % 2 == 0 {
                    orb.x + orb.y - 1.0
                } else {
                    orb.x - orb.y
                };
                assert!(off.abs() < 0.01, "swap {swap}: {orb:?} is off the line");
            }
            let (one, other) = if swap % 2 == 0 { (2, 0) } else { (1, 3) };
            let (big, small) = (
                orbs[one].r.max(orbs[other].r),
                orbs[one].r.min(orbs[other].r),
            );
            assert!(big > REST_RADIUS && small < REST_RADIUS, "swap {swap}");
        }
        // Between swaps the moving pair is off the line: the line is a
        // passing shape.
        let between = orbs_at(FIRST_SWAP_MS - SWAP_MS / 2.0);
        assert!(between.iter().any(|orb| (orb.x + orb.y - 1.0).abs() > 0.1));
    }

    /// The sprite: solid at the core, soft past the rim, gone at its edge.
    #[test]
    fn the_orb_is_a_core_under_a_soft_halo() {
        assert!(orb_alpha(0.0) > 0.99);
        assert!(orb_alpha(0.8) > 0.95);
        let rim = orb_alpha(1.0);
        assert!((0.5..0.9).contains(&rim), "the rim rolls off: {rim}");
        assert!((0.1..0.5).contains(&orb_alpha(1.5)), "the halo glows");
        assert!(orb_alpha(ORB_EXTENT) < 1.0 / 255.0);
        let svg = orb_svg();
        assert!(svg.contains("radialGradient") && svg.ends_with("</svg>"));
    }

    struct Board {
        marks: usize,
        shown: Rc<Cell<bool>>,
        renders: Rc<Cell<usize>>,
    }

    impl gpui::Render for Board {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            div()
                .size(px(400.))
                .when(self.shown.get(), |board| {
                    board.children((0..self.marks).map(|_| working_mark(16.0, theme::INK_STRONG)))
                })
                // Clipped out of its scroller's sight: drawn, never seen.
                .child(
                    div().size(px(20.)).overflow_hidden().child(
                        div()
                            .mt(px(200.))
                            .child(working_mark(16.0, theme::INK_STRONG)),
                    ),
                )
        }
    }

    fn board(
        marks: usize,
        cx: &mut gpui::TestAppContext,
    ) -> (Rc<Cell<bool>>, Rc<Cell<usize>>, gpui::WindowHandle<Board>) {
        let shown = Rc::new(Cell::new(true));
        let renders = Rc::new(Cell::new(0));
        let (s, r) = (shown.clone(), renders.clone());
        let window = cx.add_window(move |_, _| Board {
            marks,
            shown: s,
            renders: r,
        });
        cx.run_until_parked();
        (shown, renders, window)
    }

    fn next_frame(window: gpui::WindowHandle<Board>, cx: &mut gpui::TestAppContext) -> usize {
        let frames = window
            .update(cx, |_, window, cx| window.simulate_next_frame(cx))
            .unwrap();
        cx.run_until_parked();
        frames
    }

    /// Under reduced motion the mark draws the rest square and asks for no
    /// frame, from the display or the pulse clock.
    #[gpui::test]
    fn reduced_motion_holds_the_rest_square(cx: &mut gpui::TestAppContext) {
        // The kit runs live, so only the operator's setting holds it.
        motion::testing::drive();
        cx.update(|cx| cx.set_reduce_motion(true));
        let (_, _, window) = board(1, cx);
        for _ in 0..10 {
            cx.executor().advance_clock(Duration::from_millis(97));
            assert_eq!(next_frame(window, cx), 0, "no frame asked for");
            assert_eq!(DRAWN.with(Cell::get), Some(REST));
        }
        cx.update(|cx| assert!(motion::pulse_parked(cx), "nothing leased the clock"));
    }

    /// Live, every mark in the window shares one display frame request a
    /// frame and moves on every frame, never held on a 30fps step. Once
    /// the marks are gone the window asks for nothing more, and a mark
    /// clipped out of sight never asked.
    #[gpui::test]
    fn the_marks_ask_for_one_frame_and_none_once_hidden(cx: &mut gpui::TestAppContext) {
        motion::testing::drive();
        let (shown, renders, window) = board(5, cx);
        let mut last = DRAWN.with(Cell::get);
        for _ in 0..12 {
            cx.executor().advance_clock(Duration::from_millis(8));
            let before = renders.get();
            assert_eq!(next_frame(window, cx), 1, "one request for five marks");
            assert_eq!(renders.get(), before + 1, "and one draw");
            let drawn = DRAWN.with(Cell::get);
            assert_ne!(drawn, last, "the orbs moved within 8ms");
            last = drawn;
        }
        cx.update(|cx| assert!(motion::pulse_parked(cx), "no pulse tick"));

        shown.set(false);
        window.update(cx, |_, _, cx| cx.notify()).unwrap();
        cx.run_until_parked();
        // The frame asked for before the marks left still arrives, once.
        next_frame(window, cx);
        let before = renders.get();
        for _ in 0..10 {
            cx.executor().advance_clock(Duration::from_millis(16));
            assert_eq!(next_frame(window, cx), 0, "the clipped mark asks nothing");
        }
        assert_eq!(renders.get(), before, "nothing redraws");
    }
}
