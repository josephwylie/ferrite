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

    /// Read `<dir>/cited-sources.json`. A missing or unreadable file is no
    /// sources: a quote then simply has none to offer.
    pub fn load(dir: &Path) -> Self {
        let threads = fs::read(dir.join(Self::FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
            .map(|stored| stored.threads)
            .unwrap_or_default();
        Self { threads }
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
        }
        changed
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
}
