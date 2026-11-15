//! What a restart brings back of each Thread's Files: the files open in its
//! editor, the one shown, and the preferences the `⋯` menu holds. Written to
//! `<prefs dir>/files.json` whenever it changes; unsaved text is not part of
//! it, so a file comes back as it is on disk.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::editor::Settings;

const FILE_NAME: &str = "files.json";
const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct OpenFile {
    pub(super) path: PathBuf,
    pub(super) preview: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct WorkingSet {
    pub(super) files: Vec<OpenFile>,
    pub(super) active: Option<usize>,
    pub(super) settings: Settings,
    pub(super) preview_tabs: bool,
    pub(super) show_tree: bool,
}

#[derive(Serialize, Deserialize)]
struct Persisted {
    version: u32,
    threads: BTreeMap<u64, WorkingSet>,
}

#[derive(Default)]
pub(crate) struct WorkingSets {
    sets: BTreeMap<u64, WorkingSet>,
}

impl WorkingSets {
    pub(crate) fn load(dir: &Path) -> Self {
        let read = || -> io::Result<BTreeMap<u64, WorkingSet>> {
            let bytes = match std::fs::read(dir.join(FILE_NAME)) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
                Err(error) => return Err(error),
            };
            let stored: Persisted = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            if stored.version != SCHEMA_VERSION {
                return Err(io::Error::other(format!(
                    "version {} is not supported",
                    stored.version
                )));
            }
            Ok(stored.threads)
        };
        let sets = read().unwrap_or_else(|error| {
            eprintln!("ferrite: the Files tabs were not read: {error}");
            BTreeMap::new()
        });
        Self { sets }
    }

    pub(super) fn get(&self, thread: u64) -> Option<&WorkingSet> {
        self.sets.get(&thread)
    }

    pub(super) fn sync(&mut self, live: impl Iterator<Item = (u64, WorkingSet)>) -> bool {
        let mut changed = false;
        for (thread, set) in live {
            if self.sets.get(&thread) != Some(&set) {
                self.sets.insert(thread, set);
                changed = true;
            }
        }
        changed
    }

    pub(super) fn retain(&mut self, known: &HashSet<u64>) {
        if !known.is_empty() {
            self.sets.retain(|thread, _| known.contains(thread));
        }
    }

    pub(crate) fn save(&self, dir: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(&Persisted {
            version: SCHEMA_VERSION,
            threads: self.sets.clone(),
        })
        .map_err(io::Error::other)?;
        std::fs::create_dir_all(dir)?;
        ferrite_core::store::replace_file(&dir.join(FILE_NAME), &bytes)
    }
}
