//! How fast a copy goes and how long it has left, as a person says them:
//! read from the bytes it had sent at each moment it was heard, on a clock
//! the caller passes in, so a test drives it with a clock of its own.

use std::collections::VecDeque;
use std::fmt;
use std::time::{Duration, Instant};

/// The rate is what was sent over this much of the latest time.
const SPAN: Duration = Duration::from_secs(5);
/// No rate until the copy has been heard sending for this long.
const FIRST_RATE: Duration = Duration::from_secs(1);
/// No time left until the copy has been heard sending for this long.
const FIRST_LEFT: Duration = Duration::from_secs(3);
/// Another time left shows only once it has held this long.
const HOLD: Duration = Duration::from_secs(3);

/// How long a copy has left, rounded as a person says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Left {
    UnderAMinute,
    Minutes(u64),
    Hours(u64),
}

impl Left {
    fn of(seconds: f64) -> Self {
        let minutes = seconds / 60.0;
        if minutes < 1.0 {
            return Left::UnderAMinute;
        }
        // Past ten minutes, to the nearest five: nobody says 23.
        let minutes = if minutes < 10.0 {
            minutes.round()
        } else {
            (minutes / 5.0).round() * 5.0
        };
        if minutes < 60.0 {
            Left::Minutes(minutes as u64)
        } else {
            Left::Hours((seconds / 3600.0).round().max(1.0) as u64)
        }
    }
}

impl fmt::Display for Left {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Left::UnderAMinute => write!(f, "less than a minute"),
            Left::Minutes(minutes) => write!(f, "about {minutes} min"),
            Left::Hours(hours) => write!(f, "about {hours} h"),
        }
    }
}

/// What the meter reads at a moment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Reading {
    /// Bytes a second, once there is enough to say.
    pub(crate) rate: Option<u64>,
    pub(crate) left: Option<Left>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Meter {
    /// When the copy was first heard sending.
    began: Option<Instant>,
    /// When each count of bytes sent was heard, oldest first: the span and
    /// the last one before it.
    heard: VecDeque<(Instant, u64)>,
    /// The time left shown.
    shown: Option<Left>,
    /// Another time left heard, and since when, until it holds.
    next: Option<(Option<Left>, Instant)>,
}

impl Meter {
    /// The copy had sent `sent` bytes at `now`. Fewer than before is a
    /// pass over again: the meter starts over.
    pub(crate) fn heard(&mut self, now: Instant, sent: u64) {
        if self.heard.back().is_some_and(|&(_, last)| sent < last) {
            *self = Self::default();
        }
        self.began.get_or_insert(now);
        self.heard.push_back((now, sent));
        while self.heard.len() > 2 && self.heard[1].0 + SPAN <= now {
            self.heard.pop_front();
        }
    }

    /// The rate and the time left at `now`, of a copy of `total` bytes. The
    /// rate is the bytes sent over the last `SPAN`, so a copy that stalls
    /// slows to nothing rather than holding its last pace. The time left
    /// shows once the copy has gone `FIRST_LEFT`, and another shows only
    /// once it has held for `HOLD`.
    pub(crate) fn read(&mut self, now: Instant, total: u64) -> Reading {
        let rate = self.rate(now);
        let gone = self.began.map(|began| now.saturating_duration_since(began));
        let heard = match (rate, gone) {
            (Some(rate), Some(gone)) if rate > 0 && gone >= FIRST_LEFT => {
                let sent = self.heard.back().map_or(0, |&(_, sent)| sent);
                Some(Left::of(total.saturating_sub(sent) as f64 / rate as f64))
            }
            _ => None,
        };
        Reading {
            rate,
            left: self.hold(heard, now),
        }
    }

    fn rate(&self, now: Instant) -> Option<u64> {
        let &(first, _) = self.heard.front()?;
        let &(_, last) = self.heard.back()?;
        let start = now.checked_sub(SPAN).filter(|&start| start > first);
        // The count at the span's start: the last heard by then.
        let (from, then) = match start {
            Some(start) => {
                let then = self
                    .heard
                    .iter()
                    .take_while(|&&(at, _)| at <= start)
                    .last()
                    .map_or(0, |&(_, sent)| sent);
                (start, then)
            }
            None => self.heard[0],
        };
        let gone = now.saturating_duration_since(from);
        if gone < FIRST_RATE {
            return None;
        }
        Some(((last - then) as f64 / gone.as_secs_f64()) as u64)
    }

    /// `heard`, once it has held: a first time left shows at once, another
    /// only once it has held for `HOLD`.
    fn hold(&mut self, heard: Option<Left>, now: Instant) -> Option<Left> {
        if heard == self.shown || self.shown.is_none() {
            self.shown = heard;
            self.next = None;
            return self.shown;
        }
        match self.next {
            Some((next, since)) if next == heard => {
                if now.saturating_duration_since(since) >= HOLD {
                    self.shown = heard;
                    self.next = None;
                }
            }
            _ => self.next = Some((heard, now)),
        }
        self.shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1_000_000;

    /// A clock of the test's own: seconds after a start.
    struct Clock(Instant);

    impl Clock {
        fn at(&self, seconds: f64) -> Instant {
            self.0 + Duration::from_secs_f64(seconds)
        }
    }

    /// The meter heard at 10 MB/s, every half second, from `from` to
    /// `until`.
    fn steady(meter: &mut Meter, clock: &Clock, from: f64, until: f64) {
        let mut at = from;
        while at <= until {
            meter.heard(clock.at(at), (at * 10.0 * MB as f64) as u64);
            at += 0.5;
        }
    }

    #[test]
    fn the_rate_is_the_average_over_the_last_few_seconds() {
        let clock = Clock(Instant::now());
        let mut meter = Meter::default();
        meter.heard(clock.at(0.0), 0);
        assert_eq!(meter.read(clock.at(0.5), 1000 * MB).rate, None, "too soon to say");
        steady(&mut meter, &clock, 0.5, 10.0);
        assert_eq!(meter.read(clock.at(10.0), 1000 * MB).rate, Some(10 * MB));
        // Faster for the last two seconds: the average moves, not jumps.
        let sent = 100 * MB;
        meter.heard(clock.at(11.0), sent + 30 * MB);
        meter.heard(clock.at(12.0), sent + 60 * MB);
        let rate = meter.read(clock.at(12.0), 1000 * MB).rate.unwrap();
        assert!(rate > 10 * MB && rate < 30 * MB, "{rate}");
    }

    #[test]
    fn a_copy_that_stalls_slows_to_nothing_and_says_no_time_left() {
        let clock = Clock(Instant::now());
        let mut meter = Meter::default();
        steady(&mut meter, &clock, 0.0, 10.0);
        let slowing = meter.read(clock.at(12.5), 1000 * MB);
        assert_eq!(slowing.rate, Some(5 * MB), "half the span sent nothing");
        let stalled = meter.read(clock.at(16.0), 1000 * MB);
        assert_eq!(stalled.rate, Some(0));
        // No time left is a change too, and holds before it shows.
        assert_eq!(stalled.left, Some(Left::Minutes(3)));
        assert_eq!(meter.read(clock.at(19.5), 1000 * MB).left, None);
    }

    #[test]
    fn the_time_left_shows_once_the_copy_has_gone_a_few_seconds() {
        let clock = Clock(Instant::now());
        let mut meter = Meter::default();
        steady(&mut meter, &clock, 0.0, 2.0);
        assert_eq!(meter.read(clock.at(2.0), 2000 * MB).left, None);
        steady(&mut meter, &clock, 2.5, 3.0);
        // 1970 MB at 10 MB/s: 197 s, three minutes as a person says it.
        assert_eq!(meter.read(clock.at(3.0), 2000 * MB).left, Some(Left::Minutes(3)));
    }

    #[test]
    fn the_time_left_holds_still_until_another_has_held_for_a_while() {
        let clock = Clock(Instant::now());
        let mut meter = Meter::default();
        // 2000 MB at 10 MB/s, read ten times a second: 3 min left at first,
        // 2 once under 150 s remain, the reading crossing between the two
        // as each count lands.
        let mut shown = Vec::new();
        let mut at = 0.0;
        while at <= 70.0 {
            meter.heard(clock.at(at), (at * 10.0 * MB as f64) as u64);
            for tick in 0..5 {
                let now = clock.at(at + f64::from(tick) * 0.1);
                if let Some(left) = meter.read(now, 2000 * MB).left {
                    if shown.last() != Some(&left) {
                        shown.push(left);
                    }
                }
            }
            at += 0.5;
        }
        assert_eq!(shown, [Left::Minutes(3), Left::Minutes(2)], "one change, no flicker");
        // A burst that would say less than a minute for a moment does not
        // show.
        let mut burst = meter.clone();
        burst.heard(clock.at(70.5), 1400 * MB);
        assert_eq!(burst.read(clock.at(70.5), 2000 * MB).left, Some(Left::Minutes(2)));
        burst.heard(clock.at(71.0), 1405 * MB);
        assert_eq!(burst.read(clock.at(71.0), 2000 * MB).left, Some(Left::Minutes(2)));
    }

    #[test]
    fn a_new_pass_starts_the_meter_over() {
        let clock = Clock(Instant::now());
        let mut meter = Meter::default();
        steady(&mut meter, &clock, 0.0, 10.0);
        meter.heard(clock.at(10.5), 0);
        assert_eq!(meter.read(clock.at(10.6), 10 * MB), Reading::default());
    }

    #[test]
    fn the_time_left_reads_as_a_person_says_it() {
        assert_eq!(Left::of(20.0).to_string(), "less than a minute");
        assert_eq!(Left::of(80.0).to_string(), "about 1 min");
        assert_eq!(Left::of(200.0).to_string(), "about 3 min");
        assert_eq!(Left::of(23.0 * 60.0).to_string(), "about 25 min");
        assert_eq!(Left::of(58.0 * 60.0).to_string(), "about 1 h");
        assert_eq!(Left::of(150.0 * 60.0).to_string(), "about 3 h");
    }
}
