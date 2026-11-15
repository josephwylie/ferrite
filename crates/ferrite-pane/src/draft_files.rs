//! The files a draft carries, each with what a send would make of it
//! (`ferrite_core::attachment`). The Composer sends only while none has
//! failed, and sends the bytes `retain` kept.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ferrite_core::attachment::{self, Destination, Readiness, Source};

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

    /// Swap each file for its retained copy, so what is sent is what is
    /// kept. A copy that cannot be made leaves the file a live reference; on
    /// a machine nothing is copied, for the paths are the machine's.
    pub(crate) fn retain(&mut self) {
        if self.destination == Destination::Remote {
            return;
        }
        let mut paths = Vec::with_capacity(self.paths.len());
        let mut local = Vec::new();
        for path in &self.paths {
            let copy = attachment::retain(path, &self.store).unwrap_or_else(|_| path.clone());
            if self.source(path) == Source::Local {
                local.push(copy.clone());
            }
            if !paths.contains(&copy) {
                paths.push(copy);
            }
        }
        self.replace(paths, local);
    }
}
