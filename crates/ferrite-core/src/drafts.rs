//! What an operator authored in a Composer and has not sent. It outlives a
//! Pane closing and a restart, so a Thread comes back with its line, quotes,
//! files and review comments as they were left, and a new Thread's first
//! prompt comes back on the Project, workspace and provider it was aimed at.
//! Kept in one small versioned file beside `settings.json`, which every
//! machine's window shares: a Thread's draft is under its `SavedThread`, so
//! Thread 3 here and Thread 3 on a machine keep their own (ADR 0045).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::draft::SavedBinding;
use crate::draft_content::DraftContent;
use crate::roster::DraftId;
use crate::{MachineKey, SavedThread, ThreadId};

/// A new Thread's first prompt, before it has a Thread: the choices it is
/// aimed at, its Project by its machine's saved name, and what is written.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FreshDraft {
    pub binding: SavedBinding,
    pub content: DraftContent,
}

/// Every unsent draft: each Thread's, whichever machine it is on, and each
/// new Pane's by its `DraftId`. A draft with nothing written has no entry.
#[derive(Debug, Default, PartialEq)]
pub struct Drafts {
    threads: BTreeMap<SavedThread, DraftContent>,
    fresh: BTreeMap<DraftId, FreshDraft>,
}

/// The file as written. Bump `VERSION` when a part changes shape.
#[derive(Serialize, Deserialize)]
struct Stored {
    version: u32,
    threads: Vec<StoredThread>,
    fresh: Vec<FreshDraft>,
}

#[derive(Serialize, Deserialize)]
struct StoredThread {
    thread: SavedThread,
    content: DraftContent,
}

/// The second version, from before Threads were saved with their machine:
/// each draft under its bare id.
#[derive(Deserialize)]
struct Version2 {
    threads: BTreeMap<ThreadId, DraftContent>,
    fresh: Vec<FreshDraft>,
}

impl Drafts {
    /// The file's name under the store directory.
    pub const FILE: &'static str = "drafts.json";
    const VERSION: u32 = 3;

    /// Read `<dir>/drafts.json`. A missing or unreadable file is no drafts:
    /// they are conveniences, and a damaged file is replaced at the next
    /// save, never worth refusing to start for. A draft saved under a bare
    /// id, before Threads were saved with their machine, is this
    /// computer's: most were, and a wrong one is only a draft on another
    /// Thread, which the operator can clear. The first version stored one
    /// string per Thread; those are imported here, the only place that text
    /// is split into parts, and the file is rewritten in the current
    /// version.
    pub fn load(dir: &Path) -> Self {
        let Ok(bytes) = fs::read(dir.join(Self::FILE)) else {
            return Self::default();
        };
        if let Ok(stored) = serde_json::from_slice::<Stored>(&bytes) {
            let threads = stored
                .threads
                .into_iter()
                .map(|stored| (stored.thread, stored.content));
            return Self::with(threads, stored.fresh);
        }
        if let Ok(stored) = serde_json::from_slice::<Version2>(&bytes) {
            let threads = stored
                .threads
                .into_iter()
                .map(|(thread, content)| (this_computers(thread), content));
            return Self::with(threads, stored.fresh);
        }
        let Ok(legacy) = serde_json::from_slice::<BTreeMap<ThreadId, String>>(&bytes) else {
            return Self::default();
        };
        let imported = Self::with(
            legacy
                .into_iter()
                .map(|(thread, text)| (this_computers(thread), DraftContent::from_prompt(text)))
                .filter(|(_, content)| !content.is_empty()),
            Vec::new(),
        );
        let _ = imported.save(dir);
        imported
    }

    fn with(
        threads: impl IntoIterator<Item = (SavedThread, DraftContent)>,
        fresh: Vec<FreshDraft>,
    ) -> Self {
        Self {
            threads: threads.into_iter().collect(),
            // Ids are minted per launch: the roster knows nothing of these
            // until `take_fresh` hands them back to be reopened.
            fresh: fresh
                .into_iter()
                .enumerate()
                .map(|(at, draft)| (DraftId::new(at as u64), draft))
                .collect(),
        }
    }

    /// Write `<dir>/drafts.json`, creating `dir` if needed, so that a crash
    /// at any moment leaves the old file or the new one.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let stored = Stored {
            version: Self::VERSION,
            threads: self
                .threads
                .iter()
                .map(|(thread, content)| StoredThread {
                    thread: thread.clone(),
                    content: content.clone(),
                })
                .collect(),
            fresh: self.fresh.values().cloned().collect(),
        };
        crate::store::replace_file(
            &dir.join(Self::FILE),
            &serde_json::to_vec(&stored).map_err(io::Error::other)?,
        )
    }

    pub fn thread(&self, thread: &SavedThread) -> Option<&DraftContent> {
        self.threads.get(thread)
    }

    /// Keep `content` as the Thread's draft; nothing written is no draft.
    /// Says whether anything changed, so a caller saves only then.
    pub fn keep_thread(&mut self, thread: SavedThread, content: DraftContent) -> bool {
        if content.is_empty() {
            return self.threads.remove(&thread).is_some();
        }
        self.threads.insert(thread, content.clone()) != Some(content)
    }

    /// Keep a new Pane's prompt with the choices it is aimed at.
    pub fn keep_fresh(
        &mut self,
        draft: DraftId,
        binding: &SavedBinding,
        content: DraftContent,
    ) -> bool {
        if content.is_empty() {
            return self.fresh.remove(&draft).is_some();
        }
        let kept = FreshDraft {
            binding: binding.clone(),
            content,
        };
        self.fresh.insert(draft, kept.clone()) != Some(kept)
    }

    /// What new Pane `draft` is kept with, when it holds any words.
    pub fn fresh(&self, draft: DraftId) -> Option<&FreshDraft> {
        self.fresh.get(&draft)
    }

    /// A new Pane was discarded unsent: its prompt is not kept for the next
    /// launch. Says whether anything changed, so a caller saves only then.
    pub fn discard_fresh(&mut self, draft: DraftId) -> bool {
        self.fresh.remove(&draft).is_some()
    }

    /// The first send made a Thread of `draft`: what the Composer still
    /// holds (typing done while the Thread started) moves to the Thread in
    /// the one step, so a save sees it under exactly one key.
    pub fn promote(&mut self, draft: DraftId, thread: SavedThread, content: DraftContent) -> bool {
        let left = self.fresh.remove(&draft).is_some();
        self.keep_thread(thread, content) || left
    }

    /// The new Panes' drafts the last launch left, handed over once to be
    /// reopened; each is kept again under its Pane's new id.
    pub fn take_fresh(&mut self) -> Vec<FreshDraft> {
        std::mem::take(&mut self.fresh).into_values().collect()
    }
}

/// A Thread saved under its bare id, from before Threads were saved with
/// their machine.
fn this_computers(thread: ThreadId) -> SavedThread {
    SavedThread {
        machine: MachineKey(None),
        thread,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draft::{DraftBinding, DraftTarget};
    use crate::engine::ProviderChoice;
    use crate::replies::Reply;
    use crate::store::Provider;
    use crate::workspace::registry::ProjectId;

    fn scratch(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ferrite-drafts-{name}-{}", std::process::id()))
    }

    fn here(thread: u64) -> SavedThread {
        this_computers(ThreadId::new(thread))
    }

    fn on_box(thread: u64) -> SavedThread {
        SavedThread {
            machine: MachineKey(Some("me@box".into())),
            thread: ThreadId::new(thread),
        }
    }

    fn words(text: &str) -> DraftContent {
        DraftContent {
            text: text.into(),
            ..DraftContent::default()
        }
    }

    /// Everything a Composer holds at once, literal lookalikes included.
    fn everything() -> DraftContent {
        DraftContent {
            text: "> not a quote\n\n\u{21a9} not a comment\n@\"/tmp/looks-like-a-file\"".into(),
            files: vec!["/tmp/a b.png".into()],
            replies: vec![
                Reply::new("first\n\nsecond", "line one\nline two"),
                Reply::new("> already quoted", ""),
            ],
            review: vec![(&crate::draft_content::tests::comment()).into()],
            annotations: Vec::new(),
            chips: vec!["@src/lib.rs".into(), "/review".into()],
            local: Vec::new(),
        }
    }

    /// A draft survives a restart, and a line sent (emptied) is forgotten.
    #[test]
    fn a_draft_outlives_a_restart_and_a_send_forgets_it() {
        let dir = scratch("restart");
        let (first, second) = (here(1), here(2));
        let mut drafts = Drafts::default();
        assert!(drafts.keep_thread(first.clone(), everything()));
        assert!(drafts.keep_thread(second.clone(), words("another")));
        assert!(
            !drafts.keep_thread(second, words("another")),
            "unchanged: nothing to save"
        );
        drafts.save(&dir).unwrap();

        let mut restarted = Drafts::load(&dir);
        assert_eq!(restarted, drafts);
        assert_eq!(restarted.thread(&first), Some(&everything()));
        assert!(restarted.keep_thread(first.clone(), DraftContent::default()));
        assert_eq!(restarted.thread(&first), None);
        assert!(
            !restarted.keep_thread(first, DraftContent::default()),
            "nothing to forget"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A new Pane's prompt comes back with the choices it was aimed at, and
    /// the first send moves what is left to the Thread in one step.
    #[test]
    fn a_new_panes_prompt_keeps_its_binding_and_is_promoted_to_its_thread() {
        let dir = scratch("fresh");
        let binding = DraftBinding::new(
            ProviderChoice {
                provider: Provider::Codex,
                model: Some("gpt".into()),
            },
            crate::ProjectKey::fixture(0, serde_json::from_str::<ProjectId>("7").unwrap()),
            DraftTarget::Branch {
                name: "topic".into(),
            },
        )
        .saved(MachineKey(Some("me@box".into())));
        let mut drafts = Drafts::default();
        assert!(!drafts.keep_fresh(DraftId::new(4), &binding, DraftContent::default()));
        assert!(drafts.keep_fresh(DraftId::new(4), &binding, everything()));
        drafts.save(&dir).unwrap();

        let mut restarted = Drafts::load(&dir);
        let fresh = restarted.take_fresh();
        assert_eq!(
            fresh,
            vec![FreshDraft {
                binding,
                content: everything()
            }]
        );
        assert!(restarted.take_fresh().is_empty(), "handed over once");

        let mut drafts = Drafts::default();
        drafts.keep_fresh(DraftId::new(4), &fresh[0].binding, everything());
        assert!(drafts.promote(DraftId::new(4), here(9), words("typed during startup")));
        assert_eq!(drafts.thread(&here(9)), Some(&words("typed during startup")));
        assert!(drafts.take_fresh().is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// The first version's one-string-per-Thread file is split into parts
    /// once and rewritten in the current version.
    #[test]
    fn a_legacy_file_is_imported_and_rewritten() {
        let dir = scratch("legacy");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(Drafts::FILE),
            r#"{"1":"> quoted\n\n\u21a9 why\n\nbody","2":"plain"}"#,
        )
        .unwrap();
        let drafts = Drafts::load(&dir);
        let imported = drafts.thread(&here(1)).unwrap();
        assert_eq!(imported.replies, vec![Reply::new("quoted", "why")]);
        assert_eq!(imported.text, "body");
        assert_eq!(drafts.thread(&here(2)), Some(&words("plain")));

        let rewritten = fs::read_to_string(dir.join(Drafts::FILE)).unwrap();
        assert!(rewritten.contains("\"version\":3"));
        assert_eq!(Drafts::load(&dir), drafts);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Thread 3 here and Thread 3 on a machine are two Threads, each with
    /// its own draft, across a restart too.
    #[test]
    fn a_draft_keeps_two_machines_threads_apart() {
        let dir = scratch("machines");
        let mut drafts = Drafts::default();
        assert!(drafts.keep_thread(here(3), words("mine")));
        assert!(drafts.keep_thread(on_box(3), words("the box's")));
        drafts.save(&dir).unwrap();
        let written = fs::read_to_string(dir.join(Drafts::FILE)).unwrap();
        assert!(written.contains(r#"{"machine":"me@box","thread":3}"#), "{written}");

        let mut restarted = Drafts::load(&dir);
        assert_eq!(restarted.thread(&here(3)), Some(&words("mine")));
        assert_eq!(restarted.thread(&on_box(3)), Some(&words("the box's")));
        assert!(restarted.keep_thread(on_box(3), DraftContent::default()));
        assert_eq!(restarted.thread(&here(3)), Some(&words("mine")));
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A draft saved under its bare id, before Threads were saved with their
    /// machine, is this computer's Thread's.
    #[test]
    fn a_draft_saved_before_machines_reads_as_this_computers() {
        let dir = scratch("version2");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(Drafts::FILE),
            r#"{"version":2,"threads":{"3":{"text":"left here","files":[],"replies":[],"review":[],"annotations":[],"chips":[]}},"fresh":[]}"#,
        )
        .unwrap();
        let drafts = Drafts::load(&dir);
        assert_eq!(drafts.thread(&here(3)), Some(&words("left here")));
        assert_eq!(drafts.thread(&on_box(3)), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A missing or damaged file is no drafts, not a failed start.
    #[test]
    fn a_missing_or_damaged_file_is_no_drafts() {
        let dir = scratch("bad");
        assert_eq!(Drafts::load(&dir), Drafts::default());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(Drafts::FILE), "{not json").unwrap();
        assert_eq!(Drafts::load(&dir), Drafts::default());
        fs::remove_dir_all(&dir).unwrap();
    }
}
