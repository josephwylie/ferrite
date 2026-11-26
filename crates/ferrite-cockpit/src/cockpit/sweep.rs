//! The watchdog's sweep: the pump's look, every `SWEEP_INTERVAL` on the
//! executor's clock, at what moves with time and not with any event — the
//! Sessions' health, the Facts, the clock text drawn, free disk, the
//! minute the message times are drawn in. This keeps when each is due;
//! what each does is the pump's.

use std::time::{Duration, Instant, SystemTime};

/// How often the watchdog sweeps. Leaks grow over seconds, not frames; a
/// sweep per frame would spawn a `ps`/`tasklist` per Session per tick.
pub(super) const SWEEP_INTERVAL: Duration = Duration::from_secs(2);

/// How often the sweep re-reads free disk for the nav's foot.
const FREE_DISK_EVERY: Duration = Duration::from_secs(60);

pub(super) struct Sweep {
    /// When the watchdog last swept, on the executor's clock (the wall
    /// clock in the app, the fake one under test, so a loaded test machine
    /// cannot drop a sweep into the middle of a test). Measurements are
    /// cached from the RSS worker, so a sweep never waits for an
    /// operating-system query.
    at: Instant,
    /// When the next nav age (`2m`) turns over, as of the last draw: the
    /// sweep redraws an otherwise quiet window then.
    ages_turn_at: Option<SystemTime>,
    /// When the frame's own clock text (a notice's age) next turns over,
    /// as of the last draw.
    frame_turn_at: Option<SystemTime>,
    /// When free disk was last asked for, on the executor's clock: at
    /// launch, then by the sweep once a minute (`read_free_disk`).
    free_disk_asked: Option<Instant>,
    /// The wall-clock minute the transcripts' message times (`52 minutes
    /// ago`) were last drawn in: the sweep redraws them as it turns over.
    message_minute: i64,
}

impl Sweep {
    /// A sweep last taken at `now`, the executor's.
    pub(super) fn new(now: Instant) -> Self {
        Self {
            at: now,
            ages_turn_at: None,
            frame_turn_at: None,
            free_disk_asked: None,
            message_minute: wall_minute(),
        }
    }

    /// How long from `now` until the next sweep is due.
    pub(super) fn due_in(&self, now: Instant) -> Duration {
        SWEEP_INTERVAL.saturating_sub(now.duration_since(self.at))
    }

    /// Whether a sweep is due at `now`: if so, it is taken then.
    pub(super) fn take(&mut self, now: Instant) -> bool {
        if now.duration_since(self.at) < SWEEP_INTERVAL {
            return false;
        }
        self.at = now;
        true
    }

    /// When the clock text the last draw showed next turns over: a
    /// Thread's age (`ages`), a notice's in the frame (`frame`).
    pub(super) fn clock_text_turns(&mut self, ages: Option<SystemTime>, frame: Option<SystemTime>) {
        self.ages_turn_at = ages;
        self.frame_turn_at = frame;
    }

    /// Whether a Thread's age drawn has turned over by `wall`.
    pub(super) fn ages_turned(&self, wall: SystemTime) -> bool {
        self.ages_turn_at.is_some_and(|at| wall >= at)
    }

    /// Whether the frame's own clock text has turned over by `wall`.
    pub(super) fn frame_turned(&self, wall: SystemTime) -> bool {
        self.frame_turn_at.is_some_and(|at| wall >= at)
    }

    /// Whether free disk is due to be read at `now` — first asked, or a
    /// minute since: if so, it is asked for then.
    pub(super) fn ask_free_disk(&mut self, now: Instant) -> bool {
        if self
            .free_disk_asked
            .is_some_and(|at| now.duration_since(at) < FREE_DISK_EVERY)
        {
            return false;
        }
        self.free_disk_asked = Some(now);
        true
    }

    /// Whether the wall-clock minute moved since the message times were
    /// last drawn: if so, they are drawn in this one.
    pub(super) fn minute_turned(&mut self) -> bool {
        let minute = wall_minute();
        if minute == self.message_minute {
            return false;
        }
        self.message_minute = minute;
        true
    }

    /// When the watchdog last swept.
    #[cfg(test)]
    pub(super) fn swept_at(&self) -> Instant {
        self.at
    }

    /// The next pump at `now` sweeps, without the clock moving.
    #[cfg(test)]
    pub(super) fn fall_due(&mut self, now: Instant) {
        self.at = now - SWEEP_INTERVAL;
    }
}

fn wall_minute() -> i64 {
    ferrite_core::clock::now().timestamp().div_euclid(60)
}
