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

/// A wall time as every stamp prints it: `7:31 pm`.
pub fn label(at: DateTime<Local>) -> String {
    at.format("%-I:%M %P").to_string()
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
