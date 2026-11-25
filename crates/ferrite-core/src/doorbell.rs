//! Work landing from threads of its own: a git read, a provider's start, an
//! attach, a Server's message, a command's end. Each hands the window its
//! result over a [`channel`] that rings as it does, and the window's pump
//! sleeps until the bell rings ([`rung_past`]) instead of polling on a
//! beat (ADR 0041). A test can wait for the next landing the same way
//! instead of polling the wall clock.
//!
//! A waiter says which rings wake it ([`Hears`]): gpui's test scheduler,
//! which the tests and `ferrite --headless` run on, refuses a wake from
//! any thread but its own, so a waiter there hears only its own thread's
//! rings and looks at [`rung`] for the rest.
//!
//! The bell is one for the process: another window's or another test's
//! ring wakes a waiter early, which only costs it one more look.

use std::future::Future;
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard};
use std::task::{Poll, Waker};
use std::thread::ThreadId;
use std::time::Duration;

/// How many times the bell has rung, and who waits for its next ring.
struct Bell {
    rung: u64,
    waiting: Vec<Waiting>,
}

struct Waiting {
    waker: Waker,
    /// The thread it waits on, when only that thread's rings wake it.
    only: Option<ThreadId>,
}

/// Which rings wake a waiter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hears {
    /// A ring from any thread.
    AnyThread,
    /// A ring from the thread it waits on.
    ThisThread,
}

static BELL: Mutex<Bell> = Mutex::new(Bell {
    rung: 0,
    waiting: Vec::new(),
});
#[cfg(any(test, feature = "test-support"))]
static RINGS: std::sync::Condvar = std::sync::Condvar::new();

fn bell() -> MutexGuard<'static, Bell> {
    BELL.lock().unwrap_or_else(|e| e.into_inner())
}

/// How long a waiter waits for a landing before it calls the work lost.
pub const LOST_AFTER: Duration = Duration::from_secs(60);

/// A channel for off-thread work to hand the window its result: the sender
/// rings with each result, and once more as it hangs up, so the window
/// finds the work ended.
pub fn channel<T>() -> (Sender<T>, mpsc::Receiver<T>) {
    let (sender, receiver) = mpsc::channel();
    (Sender(Some(sender)), receiver)
}

/// A bounded [`channel`]: a send waits while `bound` results are unread.
pub fn sync_channel<T>(bound: usize) -> (SyncSender<T>, mpsc::Receiver<T>) {
    let (sender, receiver) = mpsc::sync_channel(bound);
    (SyncSender(Some(sender)), receiver)
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

/// The sending half of a [`sync_channel`].
pub struct SyncSender<T>(Option<mpsc::SyncSender<T>>);

impl<T> SyncSender<T> {
    pub fn send(&self, value: T) -> Result<(), mpsc::SendError<T>> {
        let sent = match &self.0 {
            Some(sender) => sender.send(value),
            None => Err(mpsc::SendError(value)),
        };
        ring();
        sent
    }

    pub fn try_send(&self, value: T) -> Result<(), mpsc::TrySendError<T>> {
        let sent = match &self.0 {
            Some(sender) => sender.try_send(value),
            None => Err(mpsc::TrySendError::Disconnected(value)),
        };
        if sent.is_ok() {
            ring();
        }
        sent
    }
}

impl<T> Clone for SyncSender<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Drop for SyncSender<T> {
    fn drop(&mut self) {
        self.0.take();
        ring();
    }
}

/// Something landed for the window to pick up: every waiter that hears
/// this thread wakes.
pub fn ring() {
    let here = std::thread::current().id();
    let woken: Vec<Waker> = {
        let mut bell = bell();
        bell.rung += 1;
        let (woken, kept) = std::mem::take(&mut bell.waiting)
            .into_iter()
            .partition(|waiting: &Waiting| waiting.only.is_none_or(|only| only == here));
        bell.waiting = kept;
        woken.into_iter().map(|waiting| waiting.waker).collect()
    };
    #[cfg(any(test, feature = "test-support"))]
    RINGS.notify_all();
    for waker in woken {
        waker.wake();
    }
}

/// How many times the bell has rung: what a waiter has seen.
pub fn rung() -> u64 {
    bell().rung
}

/// Ready once the bell has rung past `seen`, woken by the rings it
/// `hears`: what the window's pump sleeps on between runs.
pub fn rung_past(seen: u64, hears: Hears) -> impl Future<Output = ()> {
    let only = match hears {
        Hears::AnyThread => None,
        Hears::ThisThread => Some(std::thread::current().id()),
    };
    std::future::poll_fn(move |cx| {
        let mut bell = bell();
        if bell.rung > seen {
            return Poll::Ready(());
        }
        if !bell.waiting.iter().any(|waiting| waiting.waker.will_wake(cx.waker())) {
            bell.waiting.push(Waiting {
                waker: cx.waker().clone(),
                only,
            });
        }
        Poll::Pending
    })
}

/// Wait until the bell rings past `seen`; false when nothing lands within
/// [`LOST_AFTER`].
#[cfg(any(test, feature = "test-support"))]
pub fn wait_past(seen: u64) -> bool {
    let (bell, waited) = RINGS
        .wait_timeout_while(bell(), LOST_AFTER, |bell| bell.rung <= seen)
        .unwrap_or_else(|e| e.into_inner());
    drop(bell);
    !waited.timed_out()
}

/// Run `look` until it says done, waiting for a landing between looks.
/// Every landing gets two looks after it: a window's pump may already be
/// past the queue a landing fills when it runs, so the look that starts
/// its run may not yet be the one that reads it.
/// Panics with `lost` when nothing lands for [`LOST_AFTER`], or when it
/// has looked for twice that: a look that rings the bell itself would
/// otherwise never be lost.
#[cfg(any(test, feature = "test-support"))]
#[track_caller]
pub fn until(lost: &str, mut look: impl FnMut() -> bool) {
    let started = std::time::Instant::now();
    let mut seen = None;
    loop {
        let before = rung();
        if look() {
            return;
        }
        let looked = started.elapsed();
        assert!(
            looked < LOST_AFTER * 2,
            "{lost}: still not so after {looked:.1?}, though something landed at least once a minute"
        );
        if let Some(seen) = seen {
            assert!(
                wait_past(seen),
                "{lost}: nothing landed for {LOST_AFTER:?}, {:.1?} after the first look",
                started.elapsed()
            );
        }
        seen = Some(before);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::task::{Context, Wake};

    struct Flag(AtomicBool);

    impl Wake for Flag {
        fn wake(self: Arc<Self>) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    fn waiter() -> (Arc<Flag>, Waker) {
        let flag = Arc::new(Flag(AtomicBool::new(false)));
        let waker = Waker::from(flag.clone());
        (flag, waker)
    }

    /// A waiter that hears any thread sleeps until a send rings past what
    /// it saw, and is woken by it from the sending thread.
    #[test]
    fn a_send_from_another_thread_wakes_a_waiter_that_hears_any() {
        let (flag, waker) = waiter();
        let mut cx = Context::from_waker(&waker);
        let (sender, receiver) = sync_channel(1);
        let mut waiting = std::pin::pin!(rung_past(rung(), Hears::AnyThread));
        assert!(waiting.as_mut().poll(&mut cx).is_pending(), "nothing rang yet");
        std::thread::spawn(move || sender.send(7).unwrap()).join().unwrap();
        assert!(flag.0.load(Ordering::SeqCst), "the send woke the waiter");
        assert!(waiting.as_mut().poll(&mut cx).is_ready());
        assert_eq!(receiver.try_recv(), Ok(7));
    }

    /// A waiter that hears only its own thread is not woken from another,
    /// yet finds the ring once it looks, and is woken by its own thread.
    #[test]
    fn a_waiter_that_hears_its_own_thread_is_woken_only_from_it() {
        let (flag, waker) = waiter();
        let mut cx = Context::from_waker(&waker);
        let seen = rung();
        let mut waiting = std::pin::pin!(rung_past(seen, Hears::ThisThread));
        assert!(waiting.as_mut().poll(&mut cx).is_pending(), "nothing rang yet");
        std::thread::spawn(ring).join().unwrap();
        assert!(!flag.0.load(Ordering::SeqCst), "another thread's ring wakes no one");
        assert!(rung() > seen, "but it counts");
        let mut later = std::pin::pin!(rung_past(rung(), Hears::ThisThread));
        assert!(later.as_mut().poll(&mut cx).is_pending());
        ring();
        assert!(flag.0.load(Ordering::SeqCst), "its own thread's ring wakes it");
        assert!(later.as_mut().poll(&mut cx).is_ready());
    }
}
