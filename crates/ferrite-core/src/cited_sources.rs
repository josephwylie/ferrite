//! Where the passages a sent prompt quoted came from. The prompt's text
//! carries only the quote (`replies::compose`); the Block each one was cited
//! from stays on Ferrite's side, in one small versioned file beside
//! `drafts.json`, so a sent quote can still offer Go to source after a
//! restart. A quote is found again by its words: citing the same passage
//! from two messages keeps the later. A Thread is saved with its machine
//! (`SavedThread`, ADR 0045), since the file is the window's, whichever
//! machine it is on.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::replies::Reply;
use crate::transcript::BlockId;
use crate::{MachineKey, SavedThread, ThreadId};

type Sources = BTreeMap<String, BlockId>;

/// Each Thread's cited passages and the Block that showed each.
#[derive(Debug, Default, PartialEq)]
pub struct CitedSources {
    threads: BTreeMap<SavedThread, Sources>,
    revision: u64,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    version: u32,
    threads: Vec<Kept>,
}

/// One Thread's sources as the file holds them.
#[derive(Serialize, Deserialize)]
struct Kept {
    machine: MachineKey,
    thread: ThreadId,
    sources: Sources,
}

/// The file before Threads were saved with their machine: each Thread by
/// its bare id.
#[derive(Deserialize)]
struct StoredV1 {
    threads: BTreeMap<ThreadId, Sources>,
}

#[derive(Deserialize)]
struct Version {
    version: u32,
}

/// `quote` as it reads back out of a prompt (`replies::split`): the same
/// lines, none of them with an end the prompt text would not keep.
pub fn key(quote: &str) -> String {
    quote.lines().collect::<Vec<_>>().join("\n")
}

impl CitedSources {
    pub const FILE: &'static str = "cited-sources.json";
    const VERSION: u32 = 2;
    /// Quotes kept per Thread; past it the oldest passages' sources go.
    const PER_THREAD: usize = 200;

    /// Read `<dir>/cited-sources.json`. A missing or unreadable file is no
    /// sources: a quote then simply has none to offer. A file from before
    /// Threads were saved with their machine reads as this computer's.
    pub fn load(dir: &Path) -> Self {
        let threads = fs::read(dir.join(Self::FILE))
            .ok()
            .and_then(|bytes| Self::read(&bytes))
            .unwrap_or_default();
        Self {
            threads,
            revision: 0,
        }
    }

    fn read(bytes: &[u8]) -> Option<BTreeMap<SavedThread, Sources>> {
        Some(match serde_json::from_slice::<Version>(bytes).ok()?.version {
            1 => serde_json::from_slice::<StoredV1>(bytes)
                .ok()?
                .threads
                .into_iter()
                .map(|(thread, sources)| {
                    let machine = MachineKey(None);
                    (SavedThread { machine, thread }, sources)
                })
                .collect(),
            Self::VERSION => serde_json::from_slice::<Stored>(bytes)
                .ok()?
                .threads
                .into_iter()
                .map(|kept| {
                    let Kept { machine, thread, sources } = kept;
                    (SavedThread { machine, thread }, sources)
                })
                .collect(),
            _ => return None,
        })
    }

    /// Write `<dir>/cited-sources.json`, so that a crash at any moment
    /// leaves the old file or the new one.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let stored = Stored {
            version: Self::VERSION,
            threads: self
                .threads
                .iter()
                .map(|(saved, sources)| Kept {
                    machine: saved.machine.clone(),
                    thread: saved.thread,
                    sources: sources.clone(),
                })
                .collect(),
        };
        crate::disk::replace_file(
            &dir.join(Self::FILE),
            &serde_json::to_vec(&stored).map_err(io::Error::other)?,
        )
    }

    /// `replies` were sent on `thread`: keep where the cited ones came from.
    /// Says whether anything changed, so a caller saves only then.
    pub fn record(&mut self, thread: &SavedThread, replies: &[Reply]) -> bool {
        let mut changed = false;
        for reply in replies {
            let Some(source) = reply.source.filter(|source| source.thread == thread.thread) else {
                continue;
            };
            let kept = self.threads.entry(thread.clone()).or_default();
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

    /// `thread` is deleted: its sources go. Says whether it had any.
    pub fn forget(&mut self, thread: &SavedThread) -> bool {
        let had = self.threads.remove(thread).is_some();
        self.revision += u64::from(had);
        had
    }

    /// Changes whenever a mapping is added, re-pointed or dropped: what a
    /// drawn row compares to know its source went stale.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The Blocks `thread`'s sent quotes were cited from, by `key`.
    pub fn of(&self, thread: &SavedThread) -> Option<&Sources> {
        self.threads.get(thread)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replies::{compose, split, Source};

    fn block(id: u64) -> BlockId {
        serde_json::from_str(&id.to_string()).unwrap()
    }

    fn here(thread: u64) -> SavedThread {
        SavedThread {
            machine: MachineKey(None),
            thread: ThreadId::new(thread),
        }
    }

    fn cited(quote: &str, id: u64) -> Reply {
        Reply::new(quote, "").cited_from(Source {
            thread: ThreadId::new(1),
            block: block(id),
        })
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ferrite-cited-{name}-{}", std::process::id()))
    }

    #[test]
    fn a_sent_quote_is_found_by_its_words_after_a_reload() {
        let thread = here(1);
        let reply = cited("first\n\nsecond\n", 7);
        let mut sources = CitedSources::default();
        assert!(sources.record(&thread, std::slice::from_ref(&reply)));
        assert!(!sources.record(&thread, &[reply.clone()]));
        let dir = scratch("reload");
        sources.save(&dir).unwrap();
        let loaded = CitedSources::load(&dir);
        let _ = fs::remove_dir_all(&dir);
        let sent = compose(&[reply], "go on");
        let seen = split(&sent).0.remove(0);
        assert_eq!(
            loaded
                .of(&thread)
                .and_then(|kept| kept.get(&key(&seen.quote))),
            Some(&block(7))
        );
    }

    #[test]
    fn cited_sources_keep_two_machines_threads_apart() {
        let on_box = SavedThread {
            machine: MachineKey(Some("me@box".into())),
            thread: ThreadId::new(1),
        };
        let mut sources = CitedSources::default();
        sources.record(&here(1), &[cited("here", 1)]);
        sources.record(&on_box, &[cited("there", 2)]);
        let dir = scratch("machines");
        sources.save(&dir).unwrap();
        let mut loaded = CitedSources::load(&dir);
        let _ = fs::remove_dir_all(&dir);
        let quotes = |sources: &CitedSources, thread: &SavedThread| {
            sources.of(thread).map(|kept| kept.keys().cloned().collect::<Vec<_>>())
        };
        assert_eq!(quotes(&loaded, &here(1)), Some(vec!["here".to_string()]));
        assert_eq!(quotes(&loaded, &on_box), Some(vec!["there".to_string()]));
        assert!(loaded.forget(&on_box));
        assert_eq!(quotes(&loaded, &here(1)), Some(vec!["here".to_string()]));
    }

    #[test]
    fn a_file_saved_before_machines_reads_as_this_computers() {
        let dir = scratch("v1");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(CitedSources::FILE),
            r#"{"version":1,"threads":{"3":{"first":7}}}"#,
        )
        .unwrap();
        let loaded = CitedSources::load(&dir);
        loaded.save(&dir).unwrap();
        let rewritten = fs::read_to_string(dir.join(CitedSources::FILE)).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            loaded.of(&here(3)).and_then(|kept| kept.get("first")),
            Some(&block(7))
        );
        assert_eq!(
            rewritten,
            r#"{"version":2,"threads":[{"machine":null,"thread":3,"sources":{"first":7}}]}"#
        );
    }

    #[test]
    fn a_quote_without_a_source_is_not_kept() {
        let mut sources = CitedSources::default();
        assert!(!sources.record(&here(1), &[Reply::new("plain", "")]));
        assert_eq!(sources.of(&here(1)), None);
    }

    #[test]
    fn a_thread_keeps_a_bounded_number_of_sources_the_newest_ones() {
        let thread = here(1);
        let mut sources = CitedSources::default();
        for n in 0..(CitedSources::PER_THREAD as u64 + 5) {
            sources.record(&thread, &[cited(&format!("quote {n}"), n)]);
        }
        let kept = sources.of(&thread).unwrap();
        assert_eq!(kept.len(), CitedSources::PER_THREAD);
        assert!(!kept.contains_key("quote 0") && kept.contains_key("quote 204"));
    }

    #[test]
    fn the_revision_moves_when_a_mapping_is_added_repointed_or_dropped() {
        let thread = here(1);
        let mut sources = CitedSources::default();
        let start = sources.revision();
        sources.record(&thread, &[cited("same", 1)]);
        let added = sources.revision();
        sources.record(&thread, &[cited("same", 1)]);
        assert_eq!(sources.revision(), added, "nothing changed");
        sources.record(&thread, &[cited("same", 2)]);
        let repointed = sources.revision();
        assert!(added > start && repointed > added);
        assert!(sources.forget(&thread));
        assert!(sources.revision() > repointed);
        assert_eq!(sources.of(&thread), None);
        assert!(!sources.forget(&thread));
    }
}
