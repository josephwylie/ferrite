//! The cadence: when a loop's picture next changes, and when the one timer
//! serving every loop on screen has to wake for it.
//!
//! A loop (the caret's soft blink, a spinner's glyphs, the shimmer's crest,
//! Ferrite's starting mark) is a phase over a period from an origin. Most of
//! the time most loops hold still: the caret sits solid for 45% of its turn
//! and dim for 40%, a spinner holds each glyph for a whole step. Drawn at a
//! fixed ~30fps, those instants repaint the pixels already on screen. Here a
//! loop says when its picture can next differ ([`Loop::next_change`]), and
//! the timer wakes then, on the same grid from the same epoch the fixed
//! rate drew on ([`Grid::wake`]): every frame drawn is the frame the fixed
//! rate drew at that instant, and only repeats are dropped.
//!
//! Text that changes with time but is no loop (a working clock's `12s`, a
//! nav row's `2m`) rides along ([`Schedule::ride`]): it is woken on the grid
//! while a loop keeps the timer running, as the fixed rate redrew it, and
//! otherwise waits for whatever next draws its view.
//!
//! Pure over explicit instants: the app passes its executor's clock, a test
//! passes bare `Instant`s (or a [`crate::clock::Fixture`]'s). No renderer
//! here (ADR-0001).

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

/// The phase `[0, 1)` of a loop `period` long at `now`, counted from
/// `origin` (an `origin` after `now` reads its start).
pub fn phase(origin: Instant, period: Duration, now: Instant) -> f32 {
    let period = period.as_nanos().max(1);
    let elapsed = now.saturating_duration_since(origin).as_nanos();
    (elapsed % period) as f32 / period as f32
}

/// Which of `frames` a stepped loop shows at `phase` `[0, 1)` of its turn.
pub fn frame_at(phase: f32, frames: usize) -> usize {
    ((phase.rem_euclid(1.0) * frames as f32) as usize).min(frames.saturating_sub(1))
}

/// How a loop's picture moves through its period.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion<'a> {
    /// It moves at every instant (a crest sweeping without rest).
    Continuous,
    /// It holds one of this many frames per equal step ([`frame_at`]).
    Frames(usize),
    /// It moves only inside these spans of its phase, each `[start, end)`,
    /// and holds still between them (the caret's two fades).
    Spans(&'a [(f32, f32)]),
}

/// A loop: a [`Motion`] over `period`, phase counted from `origin`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loop<'a> {
    pub origin: Instant,
    pub period: Duration,
    pub motion: Motion<'a>,
}

impl Loop<'_> {
    /// The phase at `now` ([`phase`]): what the loop draws from.
    pub fn phase(&self, now: Instant) -> f32 {
        phase(self.origin, self.period, now)
    }

    /// The first instant after `now` at which the picture may differ from
    /// the one at `now`; `None` for a loop that never moves. Never late:
    /// where the float phase rounds across a boundary a hair off the exact
    /// instant, it answers the earlier one, and a wake there redraws the
    /// same picture rather than missing a new one.
    pub fn next_change(&self, now: Instant) -> Option<Instant> {
        let soon = now + Duration::from_nanos(1);
        let period = self.period.as_nanos();
        if period == 0 {
            return None;
        }
        let elapsed = now.saturating_duration_since(self.origin).as_nanos();
        let turn_start = now - nanos(elapsed % period);
        let at = |fraction: f64| turn_start + nanos((fraction * period as f64).floor() as u128);
        let change = match self.motion {
            Motion::Continuous => soon,
            Motion::Frames(frames) if frames <= 1 => return None,
            Motion::Frames(frames) => {
                let shown = frame_at(self.phase(now), frames);
                // Where the next frame starts, exactly: the float phase
                // can hold the old frame a few nanoseconds past it.
                let next =
                    turn_start + nanos(((shown as u128 + 1) * period).div_ceil(frames as u128));
                if frame_at(self.phase(next), frames) == shown {
                    next + Duration::from_nanos(1)
                } else {
                    next
                }
            }
            Motion::Spans(spans) => {
                let p = self.phase(now);
                if spans.iter().any(|(start, end)| (*start..*end).contains(&p)) {
                    soon
                } else {
                    let ahead = spans
                        .iter()
                        .map(|(start, _)| *start)
                        .filter(|start| *start > p)
                        .fold(None, |min: Option<f32>, start| {
                            Some(min.map_or(start, |min| min.min(start)))
                        });
                    match ahead {
                        Some(start) => at(start as f64),
                        None => {
                            let first = spans
                                .iter()
                                .map(|(start, _)| *start)
                                .fold(f32::INFINITY, f32::min);
                            if !first.is_finite() {
                                return None;
                            }
                            at(1.0 + first as f64)
                        }
                    }
                }
            }
        };
        Some(change.max(soon))
    }
}

fn nanos(n: u128) -> Duration {
    Duration::from_nanos(u64::try_from(n).unwrap_or(u64::MAX))
}

/// The instants a fixed-rate clock would draw on: every `tick` from `epoch`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub epoch: Instant,
    pub tick: Duration,
}

impl Grid {
    /// When to wake for a picture that may change at `change`: the first
    /// grid instant at or after it, and after `now`.
    pub fn wake(&self, change: Instant, now: Instant) -> Instant {
        let target = change.max(now + Duration::from_nanos(1));
        let tick = self.tick.as_nanos().max(1);
        let since = target.saturating_duration_since(self.epoch).as_nanos();
        self.epoch + nanos(since.div_ceil(tick) * tick)
    }
}

/// When each view next has to be drawn: one entry per view, its earliest
/// wake. A view declares on every draw that paints a loop; served, it is
/// forgotten, so a view that stops painting its loop lapses after one more
/// wake, and with nothing declared the timer parks.
#[derive(Debug)]
pub struct Schedule<K> {
    loops: HashMap<K, Instant>,
    riders: HashMap<K, Instant>,
}

impl<K> Default for Schedule<K> {
    fn default() -> Self {
        Self {
            loops: HashMap::new(),
            riders: HashMap::new(),
        }
    }
}

impl<K: Copy + Eq + Hash + Ord> Schedule<K> {
    /// A loop drawn in `view` at `now` wants it drawn again at `wake`. A
    /// wake this draw has already passed is replaced; a later one keeps
    /// the earlier.
    pub fn declare(&mut self, view: K, wake: Instant, now: Instant) {
        Self::keep_earliest(&mut self.loops, view, wake, now);
    }

    /// Time-driven text drawn in `view` at `now` next changes at `wake`: it
    /// is woken then only while a loop keeps the timer running.
    pub fn ride(&mut self, view: K, wake: Instant, now: Instant) {
        Self::keep_earliest(&mut self.riders, view, wake, now);
    }

    fn keep_earliest(map: &mut HashMap<K, Instant>, view: K, wake: Instant, now: Instant) {
        map.entry(view)
            .and_modify(|due| {
                if *due <= now || wake < *due {
                    *due = wake;
                }
            })
            .or_insert(wake);
    }

    /// The instant to arm the timer for, or `None` to park: no loop is
    /// waiting, whatever rides.
    pub fn next_wake(&self) -> Option<Instant> {
        let looping = self.loops.values().min().copied()?;
        Some(self.riders.values().fold(looping, |min, due| min.min(*due)))
    }

    /// The views due at `now`, each once, forgotten as they are served. A
    /// rider due while no loop waits is dropped: the draw that next comes
    /// for its view brings its text up to date.
    pub fn take_due(&mut self, now: Instant) -> Vec<K> {
        let looping = !self.loops.is_empty();
        let mut due: Vec<K> = Vec::new();
        self.loops.retain(|view, wake| {
            let ready = *wake <= now;
            if ready {
                due.push(*view);
            }
            !ready
        });
        self.riders.retain(|view, wake| {
            let ready = *wake <= now;
            if ready && looping {
                due.push(*view);
            }
            !ready
        });
        due.sort_unstable();
        due.dedup();
        due
    }

    /// No loop is waiting: the timer has nothing to serve.
    pub fn is_idle(&self) -> bool {
        self.loops.is_empty()
    }
}

/// How long until a count of `elapsed` next turns over a whole `unit` (a
/// working clock's `12s` → `13s`): never zero.
pub fn next_rollover(elapsed: Duration, unit: Duration) -> Duration {
    let unit_ns = unit.as_nanos().max(1);
    nanos(unit_ns - elapsed.as_nanos() % unit_ns)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICK: Duration = Duration::from_millis(33);

    fn ms(m: u64) -> Duration {
        Duration::from_millis(m)
    }

    /// The caret's soft blink: moving only through its two fades.
    const CARET: &[(f32, f32)] = &[(0.45, 0.55), (0.95, 1.0)];

    fn caret(origin: Instant) -> Loop<'static> {
        Loop {
            origin,
            period: ms(1_100),
            motion: Motion::Spans(CARET),
        }
    }

    /// `change` is `expected`, or a hair before it: a span's float start
    /// (0.45 is 0.44999998) turns into an instant never after the true one.
    fn at_or_just_before(change: Option<Instant>, expected: Instant) {
        let change = change.expect("a change ahead");
        assert!(
            change <= expected && expected - change < Duration::from_micros(1),
            "{change:?} is not just before {expected:?}"
        );
    }

    #[test]
    fn a_still_span_waits_for_the_next_fade_and_a_fade_wants_the_next_instant() {
        let t0 = Instant::now();
        let blink = caret(t0);
        // Solid from its start: the first change is the fall at 45%.
        at_or_just_before(blink.next_change(t0), t0 + ms(495));
        at_or_just_before(blink.next_change(t0 + ms(300)), t0 + ms(495));
        // Mid-fall: it moves now.
        let falling = t0 + ms(500);
        assert_eq!(
            blink.next_change(falling),
            Some(falling + Duration::from_nanos(1))
        );
        // Dim: the rise at 95%, then round again.
        at_or_just_before(blink.next_change(t0 + ms(700)), t0 + ms(1_045));
        at_or_just_before(blink.next_change(t0 + ms(1_100)), t0 + ms(1_100 + 495));
    }

    #[test]
    fn a_stepped_loop_changes_on_its_steps() {
        let t0 = Instant::now();
        let braille = Loop {
            origin: t0,
            period: ms(800),
            motion: Motion::Frames(10),
        };
        assert_eq!(braille.next_change(t0), Some(t0 + ms(80)));
        assert_eq!(braille.next_change(t0 + ms(79)), Some(t0 + ms(80)));
        assert_eq!(braille.next_change(t0 + ms(80)), Some(t0 + ms(160)));
        assert_eq!(braille.next_change(t0 + ms(799)), Some(t0 + ms(800)));
        let still = Loop {
            motion: Motion::Frames(1),
            ..braille
        };
        assert_eq!(still.next_change(t0), None, "one frame never moves");
    }

    /// Where the float phase rounds a boundary late (7.2/8 reads a hair
    /// under 0.9, so frame 8 still shows at exactly 720ms), the change is
    /// still ahead: never the frame after it.
    #[test]
    fn a_stepped_loop_never_skips_a_frame_the_float_phase_holds_late() {
        let t0 = Instant::now();
        let braille = Loop {
            origin: t0,
            period: ms(800),
            motion: Motion::Frames(10),
        };
        let boundary = t0 + ms(720);
        let shown = frame_at(braille.phase(boundary), 10);
        let next = braille.next_change(boundary).unwrap();
        if shown == 8 {
            assert!(next <= boundary + ms(1), "frame 9 is due: {next:?}");
        } else {
            assert_eq!(next, t0 + ms(800));
        }
        // From any instant, the declared change is the first one the drawn
        // frame sees, to the millisecond.
        for from in 0..1_600u64 {
            let now = t0 + ms(from);
            let next = braille.next_change(now).unwrap();
            let shown = frame_at(braille.phase(now), 10);
            for probe in from + 1..from + 200 {
                let at = t0 + ms(probe);
                if frame_at(braille.phase(at), 10) != shown {
                    assert!(
                        next <= at,
                        "from {from}ms the change at {probe}ms was missed"
                    );
                    break;
                }
            }
        }
    }

    #[test]
    fn a_continuous_loop_always_wants_the_next_instant() {
        let t0 = Instant::now();
        let crest = Loop {
            origin: t0,
            period: ms(2_400),
            motion: Motion::Continuous,
        };
        assert_eq!(
            crest.next_change(t0 + ms(7)),
            Some(t0 + ms(7) + Duration::from_nanos(1))
        );
    }

    #[test]
    fn the_wake_lands_on_the_grid_from_its_epoch_and_never_now() {
        let epoch = Instant::now();
        let grid = Grid { epoch, tick: TICK };
        assert_eq!(grid.wake(epoch + ms(495), epoch + ms(10)), epoch + ms(495));
        assert_eq!(grid.wake(epoch + ms(496), epoch + ms(10)), epoch + ms(528));
        // A change due now (or already past) is drawn on the next instant.
        assert_eq!(grid.wake(epoch + ms(33), epoch + ms(33)), epoch + ms(66));
        assert_eq!(grid.wake(epoch + ms(1), epoch + ms(40)), epoch + ms(66));
    }

    /// The caret's cadence over one turn: every grid instant inside a fade,
    /// the first one past each fade (it lands), and nothing on the
    /// plateaus — 6 draws, not 33.
    #[test]
    fn a_blinking_caret_wakes_through_its_fades_and_lands() {
        let epoch = Instant::now();
        let grid = Grid { epoch, tick: TICK };
        let blink = caret(epoch);
        let mut now = epoch;
        let mut wakes = Vec::new();
        while now < epoch + ms(1_100) {
            now = grid.wake(blink.next_change(now).unwrap(), now);
            wakes.push((now - epoch).as_millis());
        }
        assert_eq!(wakes, [495, 528, 561, 594, 627, 1056, 1089, 1122]);
    }

    #[test]
    fn a_schedule_serves_each_view_once_and_parks_when_nothing_is_declared() {
        let mut schedule = Schedule::default();
        let t0 = Instant::now();
        assert_eq!(schedule.next_wake(), None, "nothing declared: park");
        schedule.declare(1u64, t0 + ms(66), t0);
        schedule.declare(1u64, t0 + ms(33), t0);
        schedule.declare(2u64, t0 + ms(99), t0);
        assert_eq!(schedule.next_wake(), Some(t0 + ms(33)), "the earliest");
        assert_eq!(schedule.take_due(t0 + ms(33)), [1]);
        assert_eq!(schedule.take_due(t0 + ms(66)), Vec::<u64>::new());
        // Redrawn past its wake, a view's new declaration replaces it.
        schedule.declare(2u64, t0 + ms(200), t0 + ms(120));
        assert_eq!(schedule.next_wake(), Some(t0 + ms(200)));
        assert_eq!(schedule.take_due(t0 + ms(200)), [2]);
        assert_eq!(schedule.next_wake(), None, "undeclared: it lapses");
        assert!(schedule.is_idle());
    }

    #[test]
    fn riding_text_is_woken_only_while_a_loop_runs() {
        let mut schedule = Schedule::default();
        let t0 = Instant::now();
        schedule.ride(7u64, t0 + ms(400), t0);
        assert_eq!(schedule.next_wake(), None, "a rider alone parks");
        schedule.declare(1u64, t0 + ms(495), t0);
        assert_eq!(schedule.next_wake(), Some(t0 + ms(400)), "it rides");
        assert_eq!(schedule.take_due(t0 + ms(400)), [7]);
        assert_eq!(schedule.next_wake(), Some(t0 + ms(495)));
        // No loop waiting: a rider coming due is dropped, never served.
        let mut quiet = Schedule::default();
        quiet.ride(7u64, t0 + ms(400), t0);
        assert_eq!(quiet.take_due(t0 + ms(400)), Vec::<u64>::new());
        quiet.declare(1u64, t0 + ms(500), t0 + ms(450));
        assert_eq!(quiet.next_wake(), Some(t0 + ms(500)), "and gone");
    }

    #[test]
    fn a_count_rolls_over_at_its_next_whole_unit() {
        assert_eq!(next_rollover(ms(12_300), ms(1_000)), ms(700));
        assert_eq!(next_rollover(ms(12_000), ms(1_000)), ms(1_000));
        assert_eq!(next_rollover(ms(59_000), ms(60_000)), ms(1_000));
    }
}
