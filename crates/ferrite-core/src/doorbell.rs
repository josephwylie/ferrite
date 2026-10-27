//! Work landing from threads of its own: a git read, a provider's start, an
//! attach, a Server's message, a command's end. Each rings as it hands the
//! window its result, so a test can wait for the next landing instead of
//! polling the wall clock. Outside tests a ring does nothing.
//!
//! The bell is one for the process: another test's ring wakes a waiter
//! early, which only costs it one more look.

#[cfg(any(test, feature = "test-support"))]
use std::sync::{Condvar, Mutex};
use std::time::Duration;

#[cfg(any(test, feature = "test-support"))]
static RUNG: Mutex<u64> = Mutex::new(0);
#[cfg(any(test, feature = "test-support"))]
static BELL: Condvar = Condvar::new();

/// How long a waiter waits for a landing before it calls the work lost.
pub const LOST_AFTER: Duration = Duration::from_secs(60);

/// Something landed for the window to pick up.
#[inline]
pub fn ring() {
    #[cfg(any(test, feature = "test-support"))]
    {
        *RUNG.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        BELL.notify_all();
    }
}

/// How many times the bell has rung: what a waiter has seen.
#[cfg(any(test, feature = "test-support"))]
pub fn rung() -> u64 {
    *RUNG.lock().unwrap_or_else(|e| e.into_inner())
}

/// Wait until the bell rings past `seen`; false when nothing lands within
/// [`LOST_AFTER`].
#[cfg(any(test, feature = "test-support"))]
pub fn wait_past(seen: u64) -> bool {
    let rung = RUNG.lock().unwrap_or_else(|e| e.into_inner());
    let (rung, waited) = BELL
        .wait_timeout_while(rung, LOST_AFTER, |rung| *rung <= seen)
        .unwrap_or_else(|e| e.into_inner());
    drop(rung);
    !waited.timed_out()
}

/// Run `look` until it says done, waiting for a landing between looks.
/// Panics with `lost` when nothing lands for [`LOST_AFTER`].
#[cfg(any(test, feature = "test-support"))]
pub fn until(lost: &str, mut look: impl FnMut() -> bool) {
    loop {
        let seen = rung();
        if look() {
            return;
        }
        assert!(wait_past(seen), "{lost}");
    }
}
