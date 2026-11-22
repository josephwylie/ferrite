//! What a restart brings back of each Thread's Files: the files open in its
//! editor, the one shown, and the preferences the `⋯` menu holds. Written to
//! `<prefs dir>/files.json` whenever it changes; unsaved text is not part of
//! it, so a file comes back as it is on disk. Each Thread is saved with
//! its machine (`SavedThread`, ADR 0045): the window carries the one file
//! from machine to machine, and their Thread ids repeat.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use ferrite_core::{MachineKey, SavedThread, ThreadId};
use serde::{Deserialize, Serialize};

use super::editor::Settings;

const FILE_NAME: &str = "files.json";
/// 2 names each Thread with its machine (ADR 0045); 1 named it by its id
/// alone.
const SCHEMA_VERSION: u32 = 2;

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

/// One Thread's working set as files.json holds it:
/// `{"machine":"me@box","thread":3,"files":[..],..}`.
#[derive(Serialize, Deserialize)]
struct Saved {
    #[serde(flatten)]
    thread: SavedThread,
    #[serde(flatten)]
    set: WorkingSet,
}

#[derive(Serialize, Deserialize)]
struct Persisted {
    version: u32,
    threads: Vec<Saved>,
}

/// files.json before it named machines: each Thread by its bare id, read
/// as this computer's.
#[derive(Deserialize)]
struct Legacy {
    threads: BTreeMap<u64, WorkingSet>,
}

#[derive(Deserialize)]
struct Version {
    version: u32,
}

/// Every Thread's saved working set, on every machine the window has been
/// on.
#[derive(Clone, Default)]
pub(crate) struct WorkingSets {
    sets: BTreeMap<SavedThread, WorkingSet>,
    revision: u64,
}

impl WorkingSets {
    pub(crate) fn load(dir: &Path) -> Self {
        let read = || -> io::Result<BTreeMap<SavedThread, WorkingSet>> {
            let bytes = match std::fs::read(dir.join(FILE_NAME)) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
                Err(error) => return Err(error),
            };
            let Version { version } = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            match version {
                1 => {
                    let Legacy { threads } = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                    Ok(threads
                        .into_iter()
                        .map(|(thread, set)| {
                            let thread = SavedThread {
                                machine: MachineKey(None),
                                thread: ThreadId::new(thread),
                            };
                            (thread, set)
                        })
                        .collect())
                }
                SCHEMA_VERSION => {
                    let stored: Persisted = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                    Ok(stored.threads.into_iter().map(|saved| (saved.thread, saved.set)).collect())
                }
                version => Err(io::Error::other(format!("version {version} is not supported"))),
            }
        };
        let sets = read().unwrap_or_else(|error| {
            ferrite_core::log!("the Files tabs were not read: {error}");
            BTreeMap::new()
        });
        Self { sets, revision: 0 }
    }

    pub(super) fn get(&self, thread: &SavedThread) -> Option<&WorkingSet> {
        self.sets.get(thread)
    }

    pub(super) fn sync(&mut self, live: impl Iterator<Item = (SavedThread, WorkingSet)>) -> bool {
        let mut changed = false;
        for (thread, set) in live {
            if self.sets.get(&thread) != Some(&set) {
                self.sets.insert(thread, set);
                changed = true;
            }
        }
        if changed {
            self.revision += 1;
        }
        changed
    }

    /// `machine`'s sets keep only the Threads it still knows. Another
    /// machine's sets wait for that machine's own list.
    pub(super) fn retain(&mut self, machine: &MachineKey, known: &HashSet<ThreadId>) {
        if !known.is_empty() {
            let before = self.sets.len();
            self.sets
                .retain(|saved, _| saved.machine != *machine || known.contains(&saved.thread));
            if self.sets.len() != before {
                self.revision += 1;
            }
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn save(&self, dir: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(&Persisted {
            version: SCHEMA_VERSION,
            threads: self
                .sets
                .iter()
                .map(|(thread, set)| Saved {
                    thread: thread.clone(),
                    set: set.clone(),
                })
                .collect(),
        })
        .map_err(io::Error::other)?;
        std::fs::create_dir_all(dir)?;
        ferrite_core::store::replace_file(&dir.join(FILE_NAME), &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrite-working-set-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn set(path: &str) -> WorkingSet {
        WorkingSet {
            files: vec![OpenFile {
                path: PathBuf::from(path),
                preview: false,
            }],
            active: Some(0),
            settings: Settings::default(),
            preview_tabs: true,
            show_tree: true,
        }
    }

    fn saved(machine: Option<&str>, thread: u64) -> SavedThread {
        SavedThread {
            machine: MachineKey(machine.map(str::to_string)),
            thread: ThreadId::new(thread),
        }
    }

    #[test]
    fn a_files_json_saved_before_machines_reads_as_this_computers() {
        let dir = scratch("legacy");
        let settings = serde_json::to_value(Settings::default()).unwrap();
        let legacy = serde_json::json!({
            "version": 1,
            "threads": {
                "3": {
                    "files": [{"path": "/repo/src/lib.rs", "preview": false}],
                    "active": 0,
                    "settings": settings,
                    "preview_tabs": true,
                    "show_tree": true,
                },
            },
        });
        std::fs::write(dir.join(FILE_NAME), serde_json::to_vec(&legacy).unwrap()).unwrap();
        let sets = WorkingSets::load(&dir);
        assert_eq!(sets.get(&saved(None, 3)), Some(&set("/repo/src/lib.rs")));
        assert_eq!(sets.get(&saved(Some("me@box"), 3)), None);
        sets.save(&dir).unwrap();
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join(FILE_NAME)).unwrap()).unwrap();
        assert_eq!(written["version"], SCHEMA_VERSION);
        assert_eq!(written["threads"][0]["machine"], serde_json::Value::Null);
        assert_eq!(written["threads"][0]["thread"], 3);
    }

    #[test]
    fn a_working_set_keeps_two_machines_threads_apart() {
        let dir = scratch("two-machines");
        let mut sets = WorkingSets::default();
        let (here, there) = (saved(None, 3), saved(Some("me@box"), 3));
        sets.sync([(here.clone(), set("here.rs")), (there.clone(), set("there.rs"))].into_iter());
        sets.save(&dir).unwrap();
        let mut sets = WorkingSets::load(&dir);
        assert_eq!(sets.get(&here), Some(&set("here.rs")));
        assert_eq!(sets.get(&there), Some(&set("there.rs")));
        sets.retain(&MachineKey(None), &HashSet::from([ThreadId::new(4)]));
        assert_eq!(sets.get(&here), None, "this computer no longer has its 3");
        assert_eq!(sets.get(&there), Some(&set("there.rs")), "box's 3 is box's to judge");
    }
}
