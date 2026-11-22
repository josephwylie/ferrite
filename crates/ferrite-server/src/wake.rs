//! Server wakes waiting for their Thread (`ferrite_core::wake`, ADR 0046):
//! at most one per Thread, sent as soon as its agent can take it, and
//! dropped once its news is stale.

use std::collections::BTreeMap;

use ferrite_core::engine::WakeTaken;
use ferrite_core::pr_watch::{self, Stamp};
use ferrite_core::wake::Wake;
use ferrite_core::ThreadId;

use super::Server;

/// One part of a waiting wake: its news, and the watch it is about.
pub(super) struct Section {
    pub stamp: Stamp,
    pub wake: Wake,
}

/// Each Thread's waiting wake, as its sections.
#[derive(Default)]
pub(super) struct Wakes(BTreeMap<ThreadId, Vec<Section>>);

impl Wakes {
    pub fn waiting(&self, thread: ThreadId) -> bool {
        self.0.contains_key(&thread)
    }

    /// Hold `thread`'s wake until it can be sent. A Thread has one at most:
    /// its sender commits no news while one waits.
    pub fn hold(&mut self, thread: ThreadId, sections: Vec<Section>) {
        debug_assert!(!self.waiting(thread), "one wake per Thread");
        self.0.insert(thread, sections);
    }
}

impl Server {
    /// Send each waiting wake its Thread can take now. A section whose
    /// watch ended or moved on is dropped first, and a wake left with none
    /// is cancelled (T3's queued-wake check).
    pub(super) fn serve_wakes(&mut self) {
        let threads: Vec<ThreadId> = self.wakes.0.keys().copied().collect();
        for thread in threads {
            let Some(fact) = self.engine.pr_watch(thread) else {
                self.wakes.0.remove(&thread);
                continue;
            };
            let sections = self.wakes.0.get_mut(&thread).expect("listed");
            sections.retain(|section| fact.owes(&section.stamp));
            let wakes = sections.iter().map(|section| section.wake.clone()).collect();
            let Some(wake) = pr_watch::combine(wakes) else {
                self.wakes.0.remove(&thread);
                continue;
            };
            match self.engine.send_wake(thread, &wake) {
                WakeTaken::NotYet => {}
                WakeTaken::Taken | WakeTaken::Gone => {
                    self.wakes.0.remove(&thread);
                }
                WakeTaken::Refused(why) => {
                    ferrite_core::log!("thread {thread}: a wake was not sent: {why}");
                    self.wakes.0.remove(&thread);
                }
            }
        }
    }
}
