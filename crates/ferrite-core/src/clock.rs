//! The one clock behind every time Ferrite shows: a prompt's send time, a
//! turn's completion stamp and elapsed, a running call's live counter, a
//! notification's age.
//!
//! Live it is the system clock. A visual-reference scene or a test installs
//! a [`Fixture`] — a process-global override, so every thread reads the same
//! frozen, steppable time — and the transcript then reads `7:18 pm` and `12s`
//! on every run. Dropping the fixture restores the system clock. A test that
//! shares its process with others freezes its own thread instead
//! ([`ThreadFixture`]), leaving every other test on the system clock.
//!
//! Replay never reads this clock: what a log says was observed (a stored
//! `completed_at`, a stored `sent_at`) is what is drawn, and an older log that
//! never stored one draws nothing rather than a fresh guess.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use chrono::{DateTime, Local};

/// The installed override: a wall time and the monotonic instant captured
/// with it, both moved together by `offset`.
struct Override {
    origin_local: DateTime<Local>,
    origin_instant: Instant,
    offset: Duration,
}

impl Override {
    /// Frozen at `at` until moved.
    fn at(at: DateTime<Local>) -> Override {
        Override {
            origin_local: at,
            origin_instant: Instant::now(),
            offset: Duration::ZERO,
        }
    }

    fn now(&self) -> DateTime<Local> {
        self.origin_local + chrono::Duration::from_std(self.offset).unwrap_or_default()
    }

    fn instant(&self) -> Instant {
        self.origin_instant + self.offset
    }
}

static OVERRIDE: Mutex<Option<Override>> = Mutex::new(None);

thread_local! {
    /// This thread's own override, ahead of the process's.
    static HERE: RefCell<Option<Override>> = const { RefCell::new(None) };
}

fn installed() -> MutexGuard<'static, Option<Override>> {
    OVERRIDE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `read` of the override this thread sees: its own, else the process's.
fn fixed<T>(read: impl Fn(&Override) -> T) -> Option<T> {
    HERE.with(|here| here.borrow().as_ref().map(&read))
        .or_else(|| installed().as_ref().map(read))
}

/// The local wall time now: the fixture's, when one is installed.
pub fn now() -> DateTime<Local> {
    fixed(Override::now).unwrap_or_else(Local::now)
}

/// A monotonic instant now, for elapsed spans (a turn's clock, a running
/// call's counter). Under a fixture it is the instant captured at install,
/// moved by everything the fixture advanced since.
pub fn instant() -> Instant {
    fixed(Override::instant).unwrap_or_else(Instant::now)
}

/// The wall time now as a `SystemTime`, for ages (`2 min ago`).
pub fn system_time() -> SystemTime {
    fixed(|fixed| SystemTime::from(fixed.now())).unwrap_or_else(SystemTime::now)
}

/// A forge's RFC 3339 stamp (`2026-10-21T01:25:30Z`) as a wall time, for
/// its age; `None` when malformed, so no age is invented.
pub fn parse_stamp(stamp: &str) -> Option<SystemTime> {
    DateTime::parse_from_rfc3339(stamp)
        .ok()
        .map(SystemTime::from)
}

/// A wall time as every stamp prints it: `7:31 pm`.
pub fn label(at: DateTime<Local>) -> String {
    at.format("%-I:%M %P").to_string()
}

/// The wall time now as a stored stamp (RFC 3339 with its offset,
/// `2026-11-04T19:18:02+11:00`): what a prompt's `sent_at` and a turn's
/// `completed_at` keep, so the transcript can say how long ago they were.
pub fn now_stamp() -> String {
    now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// A stored stamp ([`now_stamp`]) as a local wall time; `None` for one
/// written before stamps were kept (`7:18 pm`), which reads as it was.
pub fn parse_local(stamp: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(stamp)
        .ok()
        .map(|at| at.with_timezone(&Local))
}

/// How long ago `at` was, as Claude's message times read: `just now`,
/// `52 minutes ago`, `3 hours ago` the same day, `yesterday`, `4 days ago`
/// within the week, then the date (`28 Oct`, `28 Oct 2025` in another
/// year). It changes at most once a minute.
pub fn relative_label(at: DateTime<Local>, now: DateTime<Local>) -> String {
    let ago =
        |count: i64, unit: &str| format!("{count} {unit}{} ago", if count == 1 { "" } else { "s" });
    let minutes = now.signed_duration_since(at).num_minutes();
    let days = (now.date_naive() - at.date_naive()).num_days();
    match () {
        _ if minutes < 1 => "just now".into(),
        _ if minutes < 60 => ago(minutes, "minute"),
        _ if days == 0 => ago(minutes / 60, "hour"),
        _ if days == 1 => "yesterday".into(),
        _ if days < 7 => ago(days, "day"),
        _ => date_label(at, now),
    }
}

/// A day as a date reads beside today: `28 Oct`, with its year when it is
/// another year's (`28 Oct 2025`).
pub fn date_label(at: DateTime<Local>, now: DateTime<Local>) -> String {
    use chrono::Datelike;
    if at.year() == now.year() {
        at.format("%-d %b").to_string()
    } else {
        at.format("%-d %b %Y").to_string()
    }
}

/// A day as a long thread's date divider names it: `Wednesday 4 November`,
/// with its year when it is another year's.
pub fn day_heading(at: DateTime<Local>, now: DateTime<Local>) -> String {
    use chrono::Datelike;
    if at.year() == now.year() {
        at.format("%A %-d %B").to_string()
    } else {
        at.format("%A %-d %B %Y").to_string()
    }
}

/// A stamp's exact time, for the tooltip over its relative label:
/// `Wednesday 4 November 2026, 7:18 pm`.
pub fn exact_label(at: DateTime<Local>) -> String {
    format!("{}, {}", at.format("%A %-d %B %Y"), label(at))
}

/// The day `at` fell on, for a list grouped by day (the bell's): how many
/// days before `now` it was and its heading, `Today`, `Yesterday`, else the
/// weekday. A time after `now` (clocks disagree) counts as today. `None`
/// past the last six days, which no heading names.
pub fn recent_day(at: SystemTime, now: SystemTime) -> Option<(u32, String)> {
    let at = DateTime::<Local>::from(at).date_naive();
    let ago = (DateTime::<Local>::from(now).date_naive() - at).num_days();
    let ago = ago.max(0);
    let heading = match ago {
        0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        2..=6 => at.format("%A").to_string(),
        _ => return None,
    };
    Some((ago as u32, heading))
}

/// A wall time within the week ahead of `now`, as a reset reads one: the
/// stamp alone on the same day (`3:30 pm`), after its weekday otherwise
/// (`Tue 3:30 pm`).
pub fn day_label(at: SystemTime, now: SystemTime) -> String {
    let at = DateTime::<Local>::from(at);
    if at.date_naive() == DateTime::<Local>::from(now).date_naive() {
        label(at)
    } else {
        format!("{} {}", at.format("%a"), label(at))
    }
}

/// The wall time now, as a stamp prints it.
pub fn now_label() -> String {
    label(now())
}

/// A frozen, steppable clock for scenes and tests. Only one is installed at
/// a time; dropping it restores the system clock.
#[must_use = "the fixture holds the clock only while it lives"]
pub struct Fixture {
    _held: (),
}

impl Fixture {
    /// Freeze the clock at `at`.
    pub fn install(at: DateTime<Local>) -> Fixture {
        *installed() = Some(Override::at(at));
        Fixture { _held: () }
    }

    /// Move the clock to `at`. Monotonic spans move with it; a time before
    /// the current one re-anchors the wall clock without moving instants
    /// backwards.
    pub fn set(&self, at: DateTime<Local>) {
        if let Some(fixed) = installed().as_mut() {
            match (at - fixed.origin_local).to_std() {
                Ok(delta) if delta >= fixed.offset => fixed.offset = delta,
                _ => {
                    fixed.origin_instant += fixed.offset;
                    fixed.origin_local = at;
                    fixed.offset = Duration::ZERO;
                }
            }
        }
    }

    /// Step the clock forward by `by`.
    pub fn advance(&self, by: Duration) {
        if let Some(fixed) = installed().as_mut() {
            fixed.offset += by;
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        *installed() = None;
    }
}

/// A clock frozen for the installing thread alone: a test that counts what
/// the clock redraws reads one time however long the machine takes, while
/// the tests beside it keep theirs. Every other thread reads on unaffected.
#[must_use = "the fixture holds the clock only while it lives"]
pub struct ThreadFixture {
    /// Dropped on the thread it froze.
    _here: PhantomData<*const ()>,
}

impl ThreadFixture {
    /// Freeze this thread's clock at `at`.
    pub fn install(at: DateTime<Local>) -> ThreadFixture {
        HERE.with(|here| *here.borrow_mut() = Some(Override::at(at)));
        ThreadFixture { _here: PhantomData }
    }

    /// Step this thread's clock forward by `by`.
    pub fn advance(&self, by: Duration) {
        HERE.with(|here| {
            if let Some(fixed) = here.borrow_mut().as_mut() {
                fixed.offset += by;
            }
        });
    }
}

impl Drop for ThreadFixture {
    fn drop(&mut self) {
        HERE.with(|here| *here.borrow_mut() = None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn recent_days_are_named_for_six_days_and_no_further() {
        let at = |day: u32| {
            SystemTime::from(Local.with_ymd_and_hms(2026, 11, day, 9, 0, 0).unwrap())
        };
        let now = at(11);
        assert_eq!(recent_day(at(11), now), Some((0, "Today".into())));
        assert_eq!(recent_day(at(12), now), Some((0, "Today".into())), "a clock ahead");
        assert_eq!(recent_day(at(10), now), Some((1, "Yesterday".into())));
        assert_eq!(recent_day(at(9), now), Some((2, "Monday".into())));
        assert_eq!(recent_day(at(5), now), Some((6, "Thursday".into())));
        assert_eq!(recent_day(at(4), now), None);
    }

    #[test]
    fn a_fixture_freezes_and_steps_every_reading() {
        let at = Local
            .with_ymd_and_hms(2026, 10, 4, 19, 18, 0)
            .single()
            .expect("an unambiguous local time");
        let fixture = Fixture::install(at);
        let started = instant();
        assert_eq!(now_label(), "7:18 pm");
        fixture.advance(Duration::from_secs(12));
        assert_eq!(label(now()), "7:18 pm");
        assert_eq!(
            crate::progress::settled_duration_label(instant().saturating_duration_since(started)),
            "12s"
        );
        fixture.set(at + chrono::Duration::minutes(13));
        assert_eq!(now_label(), "7:31 pm");
        assert_eq!(
            instant().saturating_duration_since(started),
            Duration::from_secs(13 * 60)
        );
        let wall = system_time()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(wall, (at + chrono::Duration::minutes(13)).timestamp());
        drop(fixture);
        // Back on the system clock: it moves again.
        let before = instant();
        std::thread::sleep(Duration::from_millis(2));
        assert!(instant() > before);
    }

    #[test]
    fn a_day_label_names_the_weekday_past_today() {
        let at = |day, hour| {
            SystemTime::from(
                Local
                    .with_ymd_and_hms(2026, 11, day, hour, 30, 0)
                    .single()
                    .expect("an unambiguous local time"),
            )
        };
        // Tuesday 3 November 2026.
        assert_eq!(day_label(at(3, 15), at(3, 9)), "3:30 pm");
        assert_eq!(day_label(at(10, 3), at(3, 9)), "Tue 3:30 am");
    }

    #[test]
    fn a_stamp_reads_how_long_ago_it_was() {
        let at = |day, hour, minute| {
            Local
                .with_ymd_and_hms(2026, 11, day, hour, minute, 0)
                .single()
                .expect("an unambiguous local time")
        };
        let now = at(4, 19, 18);
        assert_eq!(
            relative_label(at(4, 19, 17) + chrono::Duration::seconds(30), now),
            "just now"
        );
        assert_eq!(relative_label(at(4, 19, 17), now), "1 minute ago");
        assert_eq!(relative_label(at(4, 18, 26), now), "52 minutes ago");
        assert_eq!(relative_label(at(4, 16, 10), now), "3 hours ago");
        assert_eq!(relative_label(at(3, 23, 50), now), "yesterday");
        assert_eq!(relative_label(at(1, 9, 0), now), "3 days ago");
        assert_eq!(
            relative_label(at(1, 9, 0) - chrono::Duration::days(7), now),
            "25 Oct"
        );
        let last_year = Local
            .with_ymd_and_hms(2025, 10, 28, 9, 0, 0)
            .single()
            .unwrap();
        assert_eq!(relative_label(last_year, now), "28 Oct 2025");
        assert_eq!(
            exact_label(at(4, 19, 18)),
            "Wednesday 4 November 2026, 7:18 pm"
        );
        assert_eq!(day_heading(at(3, 9, 0), now), "Tuesday 3 November");
        assert_eq!(day_heading(last_year, now), "Tuesday 28 October 2025");
        // A stamp round-trips; a label from before stamps does not parse.
        let _fixture = ThreadFixture::install(now);
        assert_eq!(parse_local(&now_stamp()), Some(now));
        assert_eq!(parse_local("7:18 pm"), None);
    }

    #[test]
    fn a_thread_fixture_freezes_its_thread_until_dropped() {
        let at = Local
            .with_ymd_and_hms(2026, 10, 4, 19, 18, 0)
            .single()
            .expect("an unambiguous local time");
        let fixture = ThreadFixture::install(at);
        let frozen = instant();
        assert_eq!(now_label(), "7:18 pm");
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(instant(), frozen, "a frozen thread reads one time");
        drop(fixture);
        assert!(instant() > frozen, "back on the system clock");
    }
}
