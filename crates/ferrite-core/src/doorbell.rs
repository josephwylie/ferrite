//! Work landing from threads of its own: a git read, a provider's start, an
//! attach, a Server's message, a command's end. Each hands the window its
//! result over a [`channel`] that rings as it does, so a test can wait for
//! the next landing instead of polling the wall clock. Outside tests a ring
//! does nothing.
//!
//! The bell is one for the process: another test's ring wakes a waiter
//! early, which only costs it one more look.

use std::sync::mpsc;
#[cfg(any(test, feature = "test-support"))]
use std::sync::{Condvar, Mutex};
use std::time::Duration;

#[cfg(any(test, feature = "test-support"))]
static RUNG: Mutex<u64> = Mutex::new(0);
#[cfg(any(test, feature = "test-support"))]
static BELL: Condvar = Condvar::new();

/// How long a waiter waits for a landing before it calls the work lost.
pub const LOST_AFTER: Duration = Duration::from_secs(60);

/// A channel for off-thread work to hand the window its result: the sender
/// rings with each result, and once more as it hangs up, so the window
/// finds the work ended.
pub fn channel<T>() -> (Sender<T>, mpsc::Receiver<T>) {
    let (sender, receiver) = mpsc::channel();
    (Sender(Some(sender)), receiver)
}

/// The sending half of a [`channel`].
pub struct Sender<T>(Option<mpsc::Sender<T>>);

impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), mpsc::SendError<T>> {
        let sent = match &self.0 {
            Some(sender) => sender.send(value),
            None => Err(mpsc::SendError(value)),
        };
        ring();
        sent
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        // Hang up before ringing, so the look the ring wakes sees it.
        self.0.take();
        ring();
    }
}

/// Something landed for the window to pick up.
#[inline]
fn ring() {
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
