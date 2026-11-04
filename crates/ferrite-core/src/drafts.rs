//! The words an operator typed into a Thread's Composer and has not sent.
//! They outlive a Pane closing and a restart, so a Thread comes back with
//! its line as it was left. Kept in one small file beside `settings.json`.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::ThreadId;

/// Every Thread's unsent line, by Thread. A Thread with nothing typed has no
/// entry.
#[derive(Debug, Default, PartialEq)]
pub struct Drafts {
    texts: BTreeMap<ThreadId, String>,
}

impl Drafts {
    /// The file's name under the store directory.
    pub const FILE: &'static str = "drafts.json";

    /// Read `<dir>/drafts.json`. A missing or unreadable file is no drafts:
    /// they are conveniences, and a damaged file is replaced at the next
    /// save, never worth refusing to start for.
    pub fn load(dir: &Path) -> Self {
        let texts = fs::read(dir.join(Self::FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { texts }
    }

    /// Write `<dir>/drafts.json`, creating `dir` if needed, so that a crash
    /// at any moment leaves the old file or the new one.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        crate::store::replace_file(
            &dir.join(Self::FILE),
            &serde_json::to_vec(&self.texts).map_err(io::Error::other)?,
        )
    }

    pub fn get(&self, thread: ThreadId) -> Option<&str> {
        self.texts.get(&thread).map(String::as_str)
    }

    /// Keep `text` as the Thread's draft; an empty line is no draft. Says
    /// whether anything changed, so a caller saves only then.
    pub fn set(&mut self, thread: ThreadId, text: &str) -> bool {
        if text.is_empty() {
            return self.texts.remove(&thread).is_some();
        }
        if self.get(thread) == Some(text) {
            return false;
        }
        self.texts.insert(thread, text.to_owned());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A draft survives a restart, and a line sent (emptied) is forgotten.
    #[test]
    fn a_draft_outlives_a_restart_and_a_send_forgets_it() {
        let dir = std::env::temp_dir().join(format!("ferrite-drafts-{}", std::process::id()));
        let (first, second) = (ThreadId::new(1), ThreadId::new(2));
        let mut drafts = Drafts::default();
        assert!(drafts.set(first, "half a thought\nover two lines"));
        assert!(drafts.set(second, "another"));
        assert!(!drafts.set(second, "another"), "unchanged: nothing to save");
        drafts.save(&dir).unwrap();

        let mut restarted = Drafts::load(&dir);
        assert_eq!(restarted, drafts);
        assert_eq!(restarted.get(first), Some("half a thought\nover two lines"));
        assert!(restarted.set(first, ""));
        assert_eq!(restarted.get(first), None);
        assert!(!restarted.set(first, ""), "nothing to forget");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A missing or damaged file is no drafts, not a failed start.
    #[test]
    fn a_missing_or_damaged_file_is_no_drafts() {
        let dir = std::env::temp_dir().join(format!("ferrite-drafts-bad-{}", std::process::id()));
        assert_eq!(Drafts::load(&dir), Drafts::default());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(Drafts::FILE), "{not json").unwrap();
        assert_eq!(Drafts::load(&dir), Drafts::default());
        fs::remove_dir_all(&dir).unwrap();
    }
}
