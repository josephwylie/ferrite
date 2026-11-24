//! Server wakes waiting for their Thread (`ferrite_core::wake`, ADR 0046):
//! at most one per Thread, sent as soon as its agent can take it, and
//! dropped once its news is stale. A watched PR's news is one kind; the
//! endings of a Thread's Delegates (ADR 0047) are the other.

use std::collections::BTreeMap;

use ferrite_core::engine::WakeTaken;
use ferrite_core::pr_watch;
use ferrite_core::pull_request::Stamp;
use ferrite_core::wake::Wake;
use ferrite_core::ThreadId;

use super::Server;

/// One part of a waiting PR wake: its news, and the watch it is about.
pub(super) struct Section {
    pub stamp: Stamp,
    pub wake: Wake,
}

/// A Thread's waiting wake.
enum Waiting {
    /// A watched PR's news, as it was when committed.
    Pr(Vec<Section>),
    /// Its Delegates' endings, read from their records when it is sent,
    /// so one that ended meanwhile goes in the same wake (T3's cohort).
    Delegates,
}

/// Each Thread's waiting wake.
#[derive(Default)]
pub(super) struct Wakes(BTreeMap<ThreadId, Waiting>);

impl Wakes {
    pub fn waiting(&self, thread: ThreadId) -> bool {
        self.0.contains_key(&thread)
    }

    /// Hold `thread`'s PR wake until it can be sent. A Thread has one wake
    /// at most: its senders commit no news while one waits.
    pub fn hold(&mut self, thread: ThreadId, sections: Vec<Section>) {
        debug_assert!(!self.waiting(thread), "one wake per Thread");
        self.0.insert(thread, Waiting::Pr(sections));
    }

    /// Hold a wake for `thread`'s Delegates that ended untold.
    pub fn hold_delegates(&mut self, thread: ThreadId) {
        debug_assert!(!self.waiting(thread), "one wake per Thread");
        self.0.insert(thread, Waiting::Delegates);
    }
}

impl Server {
    /// Send each waiting wake its Thread can take now. A PR section whose
    /// watch ended or moved on is dropped first, and a Delegate told
    /// another way is left out; a wake left with nothing is cancelled
    /// (T3's queued-wake check).
    pub(super) fn serve_wakes(&mut self) {
        let threads: Vec<ThreadId> = self.wakes.0.keys().copied().collect();
        for thread in threads {
            let (wake, told) = match self.wakes.0.get_mut(&thread).expect("listed") {
                Waiting::Pr(sections) => {
                    let Some(fact) = self.engine.pr_watch(thread) else {
                        self.wakes.0.remove(&thread);
                        continue;
                    };
                    sections.retain(|section| fact.owes(&section.stamp));
                    let wakes = sections.iter().map(|section| section.wake.clone()).collect();
                    (pr_watch::combine(wakes), Vec::new())
                }
                Waiting::Delegates => match self.delegates_wake(thread) {
                    Some((wake, told)) => (Some(wake), told),
                    None => (None, Vec::new()),
                },
            };
            let Some(wake) = wake else {
                self.wakes.0.remove(&thread);
                self.engine.set_wake_waiting(thread, false);
                continue;
            };
            match self.engine.send_wake(thread, &wake) {
                WakeTaken::NotYet => continue,
                WakeTaken::Taken | WakeTaken::Gone => {}
                // Said on the Thread; a Delegate's result stays readable.
                WakeTaken::Refused(why) => {
                    ferrite_core::log!("thread {thread}: a wake was not sent: {why}");
                }
            }
            self.wakes.0.remove(&thread);
            if !told.is_empty() {
                self.told_delegates(thread, &told);
            }
        }
    }
}
