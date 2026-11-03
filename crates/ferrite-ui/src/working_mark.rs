//! The working mark: Ferrite's own running glyph, the answer to Claude's
//! breathing four-dot glyph. The mark's two shards (`ferrite-upper`,
//! `ferrite-lower`) part along the normal of the seam between them, turn a
//! quarter-ish twist together — a key turning in the lock — and seat home
//! again, then the whole mark rests before the next turn.
//!
//! One turn is `MOTION_WORKING_MARK_MS` (1.6 s): about two and a half of
//! Claude's 0.6 s breaths, so a board of working Threads reads as steady
//! work, not alarm, while the motion inside the turn (1.2 s) is still quick
//! enough to read as alive at 12 px. The turn, as fractions of it:
//!
//! - `0 .. WORKING_MARK_REST` — assembled and still (the clock draws
//!   nothing: the pulse clock wakes at the rest's end).
//! - Over the remaining motion `u ∈ [0, 1)`:
//!   - the shards part over `u ∈ [0, WORKING_MARK_PART)` and seat over
//!     `[1 - WORKING_MARK_PART, 1)`, each by `WORKING_MARK_SPREAD`;
//!   - they twist over `[WORKING_MARK_TWIST.0, WORKING_MARK_TWIST.1)`, out
//!     to `WORKING_MARK_TWIST_DEG` and back, starting once they have half
//!     parted and done before they touch again, so no shard passes through
//!     the other.
//!
//! Every move is a sine in-out ease, so velocity is zero at each joint: the
//! curve is continuous with no steps and no snap where the turn wraps. The
//! shards turn about their own centroids; the mark is drawn from the same
//! two SVGs as the still mark (`icons::ferrite_icon`), so at phase 0 — and
//! under reduced motion — it *is* the still mark.
//!
//! Colour is the caller's: theme rule 6 retires the accent, so the mark
//! wears the state's ink (the provider's on the working line, `RUNNING` in
//! the nav). The two SVGs paint as alpha masks in that ink.

use std::f32::consts::PI;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{div, point, px, rgb, svg, AnyElement, App, Transformation, Window};

use crate::icons;
use crate::motion;
use crate::theme;

/// The mark's coordinate system: the SVGs' square viewBox side.
const VIEW: f32 = 1254.0;

/// The upper shard's outline in mark units (`ferrite-upper.svg`).
pub const UPPER: [(f32, f32); 4] = [(628., 50.), (950., 572.), (901., 658.), (469., 307.)];
/// The lower shard's outline in mark units (`ferrite-lower.svg`).
pub const LOWER: [(f32, f32); 5] = [
    (437., 363.),
    (305., 568.),
    (619., 1139.),
    (806., 834.),
    (751., 767.),
];

/// The unit normal of the seam between the shards, pointing from the lower
/// shard to the upper: the seam runs (469, 307) → (901, 658), so the normal
/// is that direction turned a quarter, `(351, -432) / |…|`.
const SEAM_NORMAL: (f32, f32) = (0.630_6, -0.776_1);

/// A shard's displacement from the still mark at one phase: moved by
/// `(dx, dy)` mark units and turned `angle` radians (clockwise on screen)
/// about its centroid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub dx: f32,
    pub dy: f32,
    pub angle: f32,
}

impl Pose {
    pub const STILL: Pose = Pose {
        dx: 0.0,
        dy: 0.0,
        angle: 0.0,
    };

    /// Where this pose puts `p`, a point of the shard whose centroid is
    /// `pivot`, in mark units.
    pub fn apply(self, pivot: (f32, f32), p: (f32, f32)) -> (f32, f32) {
        let (sin, cos) = self.angle.sin_cos();
        let (x, y) = (p.0 - pivot.0, p.1 - pivot.1);
        (
            pivot.0 + x * cos - y * sin + self.dx,
            pivot.1 + x * sin + y * cos + self.dy,
        )
    }
}

/// The area centroid of a simple polygon.
fn centroid(points: &[(f32, f32)]) -> (f32, f32) {
    let (mut area, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for (i, &(x0, y0)) in points.iter().enumerate() {
        let (x1, y1) = points[(i + 1) % points.len()];
        let cross = x0 * y1 - x1 * y0;
        area += cross;
        cx += (x0 + x1) * cross;
        cy += (y0 + y1) * cross;
    }
    (cx / (3.0 * area), cy / (3.0 * area))
}

/// Each shard's centroid, upper then lower: what it turns about.
pub fn pivots() -> [(f32, f32); 2] {
    [centroid(&UPPER), centroid(&LOWER)]
}

/// Sine in-out on `[0, 1]`, clamped: zero velocity at both ends.
fn ease(x: f32) -> f32 {
    0.5 - 0.5 * (PI * x.clamp(0.0, 1.0)).cos()
}

/// The two shards' poses, upper then lower, at `phase` (any real; one turn
/// per unit). Phase 0 is the still mark.
pub fn poses(phase: f32) -> [Pose; 2] {
    let phase = phase.rem_euclid(1.0);
    if phase < theme::WORKING_MARK_REST {
        return [Pose::STILL; 2];
    }
    let u = (phase - theme::WORKING_MARK_REST) / (1.0 - theme::WORKING_MARK_REST);
    let part = theme::WORKING_MARK_PART;
    let spread = ease(u / part) * (1.0 - ease((u - (1.0 - part)) / part));
    let (twist_from, twist_to) = theme::WORKING_MARK_TWIST;
    let twist = (PI * ease((u - twist_from) / (twist_to - twist_from))).sin();
    let reach = theme::WORKING_MARK_SPREAD * spread;
    let angle = theme::WORKING_MARK_TWIST_DEG.to_radians() * twist;
    let (nx, ny) = SEAM_NORMAL;
    [
        Pose {
            dx: nx * reach,
            dy: ny * reach,
            angle,
        },
        Pose {
            dx: -nx * reach,
            dy: -ny * reach,
            angle,
        },
    ]
}

/// The working mark, `size` square (the still mark's box: the shards fill
/// its height), in `ink`. Drawn by the loops overlay (`loops_overlay::hosted`)
/// on the shared pulse clock: once a tick while the shards move, not at all
/// through the rest or once the mark leaves the window. Under reduced
/// motion it is the still mark.
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
    /// The phase the mark last drew.
    static DRAWN_PHASE: std::cell::Cell<Option<f32>> = const { std::cell::Cell::new(None) };
}

impl RenderOnce for WorkingMark {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let phase = if motion::reduced_motion(cx) {
            0.0
        } else {
            let turn = Duration::from_millis(theme::MOTION_WORKING_MARK_MS);
            // Drawn again only while the shards move.
            let moves = motion::Motion::Spans(&[(theme::WORKING_MARK_REST, 1.0)]);
            motion::script_phase(turn, moves, window.current_view(), cx)
        };
        #[cfg(test)]
        DRAWN_PHASE.with(|drawn| drawn.set(Some(phase)));
        let size = self.size;
        let scale = size / VIEW;
        let centre = VIEW / 2.0;
        let shard = |path, pivot: (f32, f32), pose: Pose| {
            // gpui turns an SVG about its box's centre; the shard turns
            // about its own centroid, so shift by where the pose carries
            // the box's centre.
            let (to_x, to_y) = pose.apply(pivot, (centre, centre));
            let (shift_x, shift_y) = (to_x - centre, to_y - centre);
            svg()
                .absolute()
                .top_0()
                .left_0()
                .size(px(size))
                .path(path)
                .text_color(rgb(self.ink))
                .with_transformation(
                    Transformation::rotate(gpui::radians(pose.angle))
                        .with_translation(point(px(shift_x * scale), px(shift_y * scale))),
                )
        };
        let [upper, lower] = poses(phase);
        let [upper_pivot, lower_pivot] = pivots();
        div()
            .debug_selector(|| "working-mark".into())
            .relative()
            .flex_shrink_0()
            .size(px(size))
            .child(shard(icons::FERRITE_UPPER, upper_pivot, upper))
            .child(shard(icons::FERRITE_LOWER, lower_pivot, lower))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shards_at(phase: f32) -> Vec<(f32, f32)> {
        let [up, low] = poses(phase);
        let [up_pivot, low_pivot] = pivots();
        UPPER
            .iter()
            .map(|&p| up.apply(up_pivot, p))
            .chain(LOWER.iter().map(|&p| low.apply(low_pivot, p)))
            .collect()
    }

    fn farthest(a: &[(f32, f32)], b: &[(f32, f32)]) -> f32 {
        a.iter()
            .zip(b)
            .map(|(p, q)| (p.0 - q.0).hypot(p.1 - q.1))
            .fold(0.0, f32::max)
    }

    /// At phase 0 the shards sit exactly where the still mark draws them.
    #[test]
    fn the_turn_starts_on_the_still_mark() {
        assert_eq!(poses(0.0), [Pose::STILL; 2]);
        let still: Vec<_> = UPPER.iter().chain(&LOWER).copied().collect();
        assert_eq!(shards_at(0.0), still);
    }

    /// The seam normal is the seam turned a quarter, unit length.
    #[test]
    fn the_shards_part_across_their_seam() {
        let (sx, sy) = (901.0 - 469.0, 658.0 - 307.0_f32);
        let (nx, ny) = SEAM_NORMAL;
        assert!((nx.hypot(ny) - 1.0).abs() < 1e-3);
        assert!((nx * sx + ny * sy).abs() / sx.hypot(sy) < 1e-3);
        // The upper shard moves up and away; the lower down and away.
        let [up, low] = poses(theme::WORKING_MARK_REST + 0.3);
        assert!(up.dy < 0.0 && low.dy > 0.0);
    }

    /// One turn per unit of phase, and it wraps without a jump: the end of
    /// a turn is the still mark the next one starts on.
    #[test]
    fn the_loop_is_periodic() {
        // `p + 1.0` rounds `p` to f32's grid near 1, so compare shapes
        // to a hair, not bits.
        for i in 0..200 {
            let p = i as f32 / 200.0;
            assert!(
                farthest(&shards_at(p), &shards_at(p + 1.0)) < 1e-2,
                "at {p}"
            );
            assert!(
                farthest(&shards_at(p), &shards_at(p - 3.0)) < 1e-2,
                "at {p}"
            );
        }
        assert!(farthest(&shards_at(0.99999), &shards_at(0.0)) < 1e-2);
    }

    /// Eased and continuous: no step between neighbouring instants anywhere
    /// in the turn (a frame apart at 1000 fps moves well under a pixel at
    /// 48pt), and the shards really do part and twist.
    #[test]
    fn the_motion_is_continuous() {
        let steps = theme::MOTION_WORKING_MARK_MS as usize;
        let mut previous = shards_at(0.0);
        let mut widest: f32 = 0.0;
        for i in 1..=steps {
            let here = shards_at(i as f32 / steps as f32);
            assert!(farthest(&here, &previous) < 6.0, "a jump at {i}ms");
            widest = widest.max(farthest(&here, &shards_at(0.0)));
            previous = here;
        }
        assert!(widest > theme::WORKING_MARK_SPREAD, "the shards part");
        let peak = (1.0 + theme::WORKING_MARK_REST) / 2.0;
        let [up, low] = poses(peak);
        assert!((up.angle - theme::WORKING_MARK_TWIST_DEG.to_radians()).abs() < 1e-3);
        assert_eq!(up.angle, low.angle, "they twist together");
    }

    /// Under reduced motion the mark draws its still phase and asks the
    /// pulse clock for nothing.
    #[gpui::test]
    fn reduced_motion_holds_the_still_mark(cx: &mut gpui::TestAppContext) {
        struct Board;
        impl gpui::Render for Board {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                div().child(working_mark(16.0, theme::RUNNING))
            }
        }
        // The kit runs live, so only the operator's setting holds it.
        motion::testing::drive();
        cx.update(|cx| cx.set_reduce_motion(true));
        cx.add_window(|_, _| Board);
        cx.run_until_parked();
        for _ in 0..10 {
            cx.executor().advance_clock(Duration::from_millis(97));
            cx.run_until_parked();
        }
        assert_eq!(DRAWN_PHASE.with(|drawn| drawn.get()), Some(0.0));
        cx.update(|cx| assert!(motion::pulse_parked(cx), "nothing leased the clock"));
    }

    /// Live, the mark is drawn once a tick while its shards move and not
    /// at all through the rest.
    #[gpui::test]
    fn the_mark_is_drawn_only_while_its_shards_move(cx: &mut gpui::TestAppContext) {
        motion::testing::drive();
        struct Board(std::rc::Rc<std::cell::Cell<usize>>);
        impl gpui::Render for Board {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                self.0.set(self.0.get() + 1);
                div().child(working_mark(16.0, theme::RUNNING))
            }
        }
        let renders = std::rc::Rc::new(std::cell::Cell::new(0));
        let counted = renders.clone();
        cx.add_window(move |_, _| Board(counted));
        cx.run_until_parked();
        let before = renders.get();
        let turn = theme::MOTION_WORKING_MARK_MS;
        for _ in 0..turn {
            cx.executor().advance_clock(Duration::from_millis(1));
            cx.run_until_parked();
        }
        let drawn = renders.get() - before;
        let tick = theme::MOTION_PULSE_TICK_MS as f32;
        let moving = (turn as f32 * (1.0 - theme::WORKING_MARK_REST) / tick) as usize;
        let every_tick = (turn as f32 / tick) as usize;
        assert!(
            (moving - 2..=moving + 2).contains(&drawn),
            "{drawn} draws in a turn; {moving} ticks move"
        );
        assert!(drawn < every_tick, "the rest draws nothing");
    }

    /// The preview frames (`docs/research/craft/working-mark/`): with
    /// `WORKING_MARK_FRAMES=<file>`, one line per frame at 60 fps over a
    /// turn — the phase, then every shard vertex in mark units, upper
    /// then lower — for the rasterizer to draw.
    #[test]
    fn write_preview_frames() {
        let Ok(path) = std::env::var("WORKING_MARK_FRAMES") else {
            return;
        };
        let frames = (theme::MOTION_WORKING_MARK_MS as f32 * 60.0 / 1000.0).round() as usize;
        let lines: Vec<String> = (0..frames)
            .map(|i| {
                let phase = i as f32 / frames as f32;
                let points: Vec<String> = shards_at(phase)
                    .iter()
                    .map(|(x, y)| format!("{x:.3},{y:.3}"))
                    .collect();
                format!("{phase:.5} {}", points.join(" "))
            })
            .collect();
        std::fs::write(path, lines.join("\n")).unwrap();
    }
}
