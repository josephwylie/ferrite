//! How long ago, in the nav's own shorthand: a Thread's row, the empty
//! board's Recent, a notice's time, the palette's and a pull request's.

use std::time::{Duration, SystemTime};

use gpui::SharedString;

/// How long ago, in the nav's own shorthand: `1m`, `40m`, `2h`, `3d`, `1w`,
/// then `12mo` and `2y`. One unit, never two — the row has a line's tail to
/// spend, and "2h" is the whole answer at a glance. Under a minute it says
/// nothing at all (C10: never `now`), and callers keep the slot's width so
/// the first minute moves nothing. A time in the future (a clock that
/// moved) says nothing rather than a negative.
pub fn since_label(last_used: SystemTime, now: SystemTime) -> SharedString {
    let secs = now.duration_since(last_used).unwrap_or_default().as_secs();
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    const WEEK: u64 = 7 * DAY;
    // A month is the mean Gregorian month, and a year twelve of them: the
    // row says "3mo", not a date, so the calendar's irregularity is below
    // what it claims.
    const MONTH: u64 = 2_629_746;
    const YEAR: u64 = 12 * MONTH;
    let text = match secs {
        s if s < MINUTE => String::new(),
        s if s < HOUR => format!("{}m", s / MINUTE),
        s if s < DAY => format!("{}h", s / HOUR),
        s if s < WEEK => format!("{}d", s / DAY),
        s if s < MONTH => format!("{}w", s / WEEK),
        s if s < YEAR => format!("{}mo", s / MONTH),
        s => format!("{}y", s / YEAR),
    };
    SharedString::from(text)
}

/// How long until `since_label(last_used, ·)` next reads differently:
/// the next whole unit of the band it is in, or the band's end. `None`
/// once it reads in years (nothing on screen waits that long).
pub fn since_label_changes_in(last_used: SystemTime, now: SystemTime) -> Option<Duration> {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    const WEEK: u64 = 7 * DAY;
    const MONTH: u64 = 2_629_746;
    const YEAR: u64 = 12 * MONTH;
    let secs = |n: u64| Duration::from_secs(n);
    let elapsed = match now.duration_since(last_used) {
        Ok(elapsed) => elapsed,
        // A time ahead of the clock says nothing until a minute past it.
        Err(ahead) => return Some(ahead.duration() + secs(MINUTE)),
    };
    let (unit, end) = match elapsed.as_secs() {
        s if s < MINUTE => return Some(secs(MINUTE) - elapsed),
        s if s < HOUR => (MINUTE, HOUR),
        s if s < DAY => (HOUR, DAY),
        s if s < WEEK => (DAY, WEEK),
        s if s < MONTH => (WEEK, MONTH),
        s if s < YEAR => (MONTH, YEAR),
        _ => return None,
    };
    let rollover = ferrite_core::cadence::next_rollover(elapsed, secs(unit));
    Some(rollover.min(secs(end) - elapsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ago(secs: u64) -> SharedString {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10 * 365 * 24 * 3600);
        since_label(now - Duration::from_secs(secs), now)
    }

    /// An age turns over where `since_label` does: a minute in, then each
    /// minute, each hour from an hour, each day from a day.
    #[test]
    fn an_age_says_when_it_next_reads_differently() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10 * 365 * 24 * 3600);
        let in_ = |ago: Duration| since_label_changes_in(now - ago, now);
        let s = Duration::from_secs;
        assert_eq!(in_(s(0)), Some(s(60)), "nothing until a minute");
        assert_eq!(
            in_(Duration::from_millis(59_500)),
            Some(Duration::from_millis(500))
        );
        assert_eq!(in_(s(90)), Some(s(30)), "1m turns 2m at 120s");
        assert_eq!(in_(s(3_599)), Some(s(1)), "59m turns 1h");
        assert_eq!(in_(s(3_600 + 30)), Some(s(3_600 - 30)), "1h turns 2h");
        assert_eq!(in_(s(6 * 86_400 + 10)), Some(s(86_400 - 10)), "6d turns 1w");
        for ago in [0u64, 59, 60, 61, 3_599, 3_600, 7_300, 86_399, 90_000] {
            let next = in_(s(ago)).unwrap();
            let before = since_label(now - s(ago), now);
            let at = now + next;
            assert_ne!(
                since_label(now - s(ago), at),
                before,
                "{ago}s: changes then"
            );
            if next > s(1) {
                let just_before = at - s(1);
                assert_eq!(
                    since_label(now - s(ago), just_before),
                    before,
                    "{ago}s: not sooner"
                );
            }
        }
        assert_eq!(
            since_label_changes_in(now + s(30), now),
            Some(s(90)),
            "a clock that moved back: a minute past the stamp"
        );
    }

    #[test]
    fn the_shorthand_climbs_one_unit_at_a_time() {
        assert_eq!(ago(0), "");
        assert_eq!(ago(59), "");
        assert_eq!(ago(60), "1m");
        let then = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        assert_eq!(since_label(then, then + Duration::from_secs(90)), "1m");
        assert_eq!(ago(40 * 60), "40m");
        assert_eq!(ago(2 * 3600), "2h");
        assert_eq!(ago(3 * 24 * 3600), "3d");
        assert_eq!(ago(7 * 24 * 3600), "1w");
        assert_eq!(ago(62 * 24 * 3600), "2mo");
        assert_eq!(ago(800 * 24 * 3600), "2y");
    }

    /// A clock that moved backwards must not print a negative age.
    #[test]
    fn a_future_stamp_says_nothing() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        assert_eq!(since_label(now + Duration::from_secs(500), now), "");
    }
}
