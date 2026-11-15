//! Where the passages a sent prompt quoted came from. The prompt's text
//! carries only the quote (`replies::compose`); the Block each one was cited
//! from stays on Ferrite's side, in one small versioned file beside
//! `drafts.json`, so a sent quote can still offer Go to source after a
//! restart. A quote is found again by its words: citing the same passage
//! from two messages keeps the later.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::replies::Reply;
use crate::transcript::BlockId;
use crate::ThreadId;

/// Each Thread's cited passages and the Block that showed each.
#[derive(Debug, Default, PartialEq)]
pub struct CitedSources {
    threads: BTreeMap<ThreadId, BTreeMap<String, BlockId>>,
    revision: u64,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    version: u32,
    threads: BTreeMap<ThreadId, BTreeMap<String, BlockId>>,
}

/// `quote` as it reads back out of a prompt (`replies::split`): the same
/// lines, none of them with an end the prompt text would not keep.
pub fn key(quote: &str) -> String {
    quote.lines().collect::<Vec<_>>().join("\n")
}

impl CitedSources {
    pub const FILE: &'static str = "cited-sources.json";
    const VERSION: u32 = 1;
    /// Quotes kept per Thread; past it the oldest passages' sources go.
    const PER_THREAD: usize = 200;

    /// Read `<dir>/cited-sources.json`. A missing or unreadable file is no
    /// sources: a quote then simply has none to offer.
    pub fn load(dir: &Path) -> Self {
        let threads = fs::read(dir.join(Self::FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
            .map(|stored| stored.threads)
            .unwrap_or_default();
        Self {
            threads,
            revision: 0,
        }
    }

    /// Write `<dir>/cited-sources.json`, so that a crash at any moment
    /// leaves the old file or the new one.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let stored = Stored {
            version: Self::VERSION,
            threads: self.threads.clone(),
        };
        crate::store::replace_file(
            &dir.join(Self::FILE),
            &serde_json::to_vec(&stored).map_err(io::Error::other)?,
        )
    }

    /// `replies` were sent on `thread`: keep where the cited ones came from.
    /// Says whether anything changed, so a caller saves only then.
    pub fn record(&mut self, thread: ThreadId, replies: &[Reply]) -> bool {
        let mut changed = false;
        for reply in replies {
            let Some(source) = reply.source.filter(|source| source.thread == thread) else {
                continue;
            };
            let kept = self.threads.entry(thread).or_default();
            changed |= kept.insert(key(&reply.quote), source.block) != Some(source.block);
            while kept.len() > Self::PER_THREAD {
                let oldest = kept
                    .iter()
                    .min_by_key(|(_, block)| **block)
                    .map(|(key, _)| key.clone());
                oldest.and_then(|oldest| kept.remove(&oldest));
            }
        }
        self.revision += u64::from(changed);
        changed
    }

    /// Only `live` Threads keep sources: the rest were deleted while no one
    /// was looking. Says whether any went.
    pub fn retain_threads(&mut self, live: &[ThreadId]) -> bool {
        let before = self.threads.len();
        self.threads.retain(|thread, _| live.contains(thread));
        let dropped = self.threads.len() != before;
        self.revision += u64::from(dropped);
        dropped
    }

    /// `thread` is deleted: its sources go. Says whether it had any.
    pub fn forget(&mut self, thread: ThreadId) -> bool {
        let had = self.threads.remove(&thread).is_some();
        self.revision += u64::from(had);
        had
    }

    /// Changes whenever a mapping is added, re-pointed or dropped: what a
    /// drawn row compares to know its source went stale.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The Blocks `thread`'s sent quotes were cited from, by `key`.
    pub fn of(&self, thread: ThreadId) -> Option<&BTreeMap<String, BlockId>> {
        self.threads.get(&thread)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replies::{compose, split, Source};

    fn block(id: u64) -> BlockId {
        serde_json::from_str(&id.to_string()).unwrap()
    }

    fn cited(quote: &str, id: u64) -> Reply {
        Reply::new(quote, "").cited_from(Source {
            thread: ThreadId::new(1),
            block: block(id),
        })
    }

    #[test]
    fn a_sent_quote_is_found_by_its_words_after_a_reload() {
        let thread = ThreadId::new(1);
        let reply = cited("first\n\nsecond\n", 7);
        let mut sources = CitedSources::default();
        assert!(sources.record(thread, std::slice::from_ref(&reply)));
        assert!(!sources.record(thread, &[reply.clone()]));
        let dir = std::env::temp_dir().join(format!("ferrite-cited-{}", std::process::id()));
        sources.save(&dir).unwrap();
        let loaded = CitedSources::load(&dir);
        let _ = fs::remove_dir_all(&dir);
        let sent = compose(&[reply], "go on");
        let seen = split(&sent).0.remove(0);
        assert_eq!(
            loaded
                .of(thread)
                .and_then(|kept| kept.get(&key(&seen.quote))),
            Some(&block(7))
        );
    }

    #[test]
    fn a_quote_without_a_source_is_not_kept() {
        let mut sources = CitedSources::default();
        assert!(!sources.record(ThreadId::new(1), &[Reply::new("plain", "")]));
        assert_eq!(sources.of(ThreadId::new(1)), None);
    }

    #[test]
    fn a_thread_keeps_a_bounded_number_of_sources_the_newest_ones() {
        let thread = ThreadId::new(1);
        let mut sources = CitedSources::default();
        for n in 0..(CitedSources::PER_THREAD as u64 + 5) {
            sources.record(thread, &[cited(&format!("quote {n}"), n)]);
        }
        let kept = sources.of(thread).unwrap();
        assert_eq!(kept.len(), CitedSources::PER_THREAD);
        assert!(!kept.contains_key("quote 0") && kept.contains_key("quote 204"));
    }

    #[test]
    fn threads_that_no_longer_exist_lose_their_sources() {
        let (kept, gone) = (ThreadId::new(1), ThreadId::new(2));
        let mut sources = CitedSources::default();
        sources.record(kept, &[cited("a", 1)]);
        sources.record(
            gone,
            &[Reply::new("b", "").cited_from(Source {
                thread: gone,
                block: block(2),
            })],
        );
        let before = sources.revision();
        assert!(sources.retain_threads(&[kept]));
        assert!(sources.of(gone).is_none() && sources.of(kept).is_some());
        assert!(sources.revision() > before);
        assert!(!sources.retain_threads(&[kept]));
    }

    #[test]
    fn the_revision_moves_when_a_mapping_is_added_repointed_or_dropped() {
        let thread = ThreadId::new(1);
        let mut sources = CitedSources::default();
        let start = sources.revision();
        sources.record(thread, &[cited("same", 1)]);
        let added = sources.revision();
        sources.record(thread, &[cited("same", 1)]);
        assert_eq!(sources.revision(), added, "nothing changed");
        sources.record(thread, &[cited("same", 2)]);
        let repointed = sources.revision();
        assert!(added > start && repointed > added);
        assert!(sources.forget(thread));
        assert!(sources.revision() > repointed);
        assert_eq!(sources.of(thread), None);
        assert!(!sources.forget(thread));
    }
}
