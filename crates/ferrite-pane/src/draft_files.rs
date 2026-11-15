//! The files a draft carries, each with what a send would make of it
//! (`ferrite_core::attachment`). The Composer sends only while none has
//! failed, and sends the bytes `retain` kept.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ferrite_core::attachment::{self, Destination, Failure, Readiness, Source};

pub(crate) struct DraftFiles {
    paths: Vec<PathBuf>,
    /// The `paths` that are files of this computer.
    local: Vec<PathBuf>,
    readiness: HashMap<PathBuf, Readiness>,
    destination: Destination,
    /// Where retained copies live.
    store: PathBuf,
}

impl DraftFiles {
    pub(crate) fn new() -> Self {
        Self {
            paths: Vec::new(),
            local: Vec::new(),
            readiness: HashMap::new(),
            destination: Destination::Local,
            store: std::env::temp_dir().join("ferrite-attachments"),
        }
    }

    pub(crate) fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub(crate) fn local(&self) -> &[PathBuf] {
        &self.local
    }

    pub(crate) fn store(&self) -> &Path {
        &self.store
    }

    pub(crate) fn readiness(&self) -> &HashMap<PathBuf, Readiness> {
        &self.readiness
    }

    /// Where the Thread's agent is, and where retained copies go. Files
    /// already held are inspected again against the new destination.
    pub(crate) fn aim(&mut self, destination: Destination, store: PathBuf) {
        self.destination = destination;
        self.store = store;
        self.inspect_all();
    }

    pub(crate) fn contains(&self, path: &Path) -> bool {
        self.paths.iter().any(|kept| kept == path)
    }

    /// Whether `path` was added.
    pub(crate) fn add(&mut self, path: &Path, source: Source) -> bool {
        if self.contains(path) {
            return false;
        }
        self.paths.push(path.to_path_buf());
        if source == Source::Local {
            self.local.push(path.to_path_buf());
        }
        self.inspect(path);
        true
    }

    pub(crate) fn remove(&mut self, path: &Path) {
        self.paths.retain(|kept| kept != path);
        self.local.retain(|kept| kept != path);
        self.readiness.remove(path);
    }

    pub(crate) fn clear(&mut self) {
        self.paths.clear();
        self.local.clear();
        self.readiness.clear();
    }

    /// Hold exactly `paths`, `local` among them.
    pub(crate) fn replace(&mut self, paths: Vec<PathBuf>, local: Vec<PathBuf>) {
        self.paths = paths;
        self.local = local;
        self.inspect_all();
    }

    /// Hold `paths` too, ahead of those held when `prepend`.
    pub(crate) fn merge(&mut self, mut paths: Vec<PathBuf>, local: Vec<PathBuf>, prepend: bool) {
        paths.retain(|path| !self.contains(path));
        self.local
            .extend(local.into_iter().filter(|path| paths.contains(path)));
        if prepend {
            self.paths.splice(0..0, paths);
        } else {
            self.paths.extend(paths);
        }
        self.inspect_all();
    }

    fn source(&self, path: &Path) -> Source {
        if self.local.iter().any(|kept| kept == path) {
            Source::Local
        } else {
            Source::Workspace
        }
    }

    /// Look at `path` again: a file restored, repaired or replaced since
    /// it was added reads as it is now.
    pub(crate) fn inspect(&mut self, path: &Path) {
        let readiness = attachment::inspect(path, self.source(path), self.destination);
        self.readiness.insert(path.to_path_buf(), readiness);
    }

    pub(crate) fn inspect_all(&mut self) {
        self.readiness.clear();
        for path in self.paths.clone() {
            self.inspect(&path);
        }
    }

    /// The first file a send cannot take, and why.
    pub(crate) fn blocked(&self) -> Option<(&Path, &Readiness)> {
        self.paths.iter().find_map(|path| {
            let readiness = self.readiness.get(path)?;
            readiness.blocks().then_some((path.as_path(), readiness))
        })
    }

    /// Swap each file of this computer for its retained copy, so what is
    /// sent is what is kept. A workspace file stays the live path the agent
    /// edits, and on a machine nothing is copied, for the paths are the
    /// machine's. A file that cannot be copied is marked `Unreadable` and
    /// nothing is swapped: whether the send may go ahead.
    pub(crate) fn retain(&mut self) -> bool {
        if self.destination == Destination::Remote {
            return true;
        }
        let mut paths = Vec::with_capacity(self.paths.len());
        let mut local = Vec::new();
        for path in &self.paths {
            let kept = match self.source(path) {
                Source::Workspace => path.clone(),
                Source::Local => match attachment::retain(path, &self.store) {
                    Ok(copy) => {
                        local.push(copy.clone());
                        copy
                    }
                    Err(error) => {
                        let failure = Readiness::Failed(Failure::Unreadable(error.to_string()));
                        self.readiness.insert(path.clone(), failure);
                        return false;
                    }
                },
            };
            if !paths.contains(&kept) {
                paths.push(kept);
            }
        }
        self.replace(paths, local);
        true
    }
}
