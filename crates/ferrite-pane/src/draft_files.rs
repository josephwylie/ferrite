use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ferrite_core::attachment::{self, Destination, Failure, Readiness, Source};

pub(crate) struct DraftFiles {
    paths: Vec<PathBuf>,
    /// The `paths` that are files of this computer.
    local: Vec<PathBuf>,
    readiness: HashMap<PathBuf, Readiness>,
    destination: Destination,
    uploads: HashMap<PathBuf, UploadState>,
    next_attempt: u64,
    /// Where retained copies live.
    store: PathBuf,
}

enum UploadState {
    Pending,
    Uploading { attempt: u64, sent: u64, total: u64 },
    Ready(PathBuf),
    Failed(String),
}

impl DraftFiles {
    pub(crate) fn new() -> Self {
        Self {
            paths: Vec::new(),
            local: Vec::new(),
            readiness: HashMap::new(),
            destination: Destination::Local,
            uploads: HashMap::new(),
            next_attempt: 0,
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
        self.uploads.clear();
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
        self.uploads.remove(path);
    }

    pub(crate) fn clear(&mut self) {
        self.paths.clear();
        self.local.clear();
        self.readiness.clear();
        self.uploads.clear();
    }

    /// Hold exactly `paths`, `local` among them.
    pub(crate) fn replace(&mut self, paths: Vec<PathBuf>, local: Vec<PathBuf>) {
        self.uploads.clear();
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
        let readiness = if self.destination == Destination::Remote
            && self.source(path) == Source::Local
        {
            match self
                .uploads
                .entry(path.to_path_buf())
                .or_insert(UploadState::Pending)
            {
                UploadState::Pending => Readiness::Pending,
                UploadState::Uploading { sent, total, .. } => Readiness::Uploading {
                    sent: *sent,
                    total: *total,
                },
                UploadState::Ready(_) => Readiness::Ready,
                UploadState::Failed(reason) => Readiness::Failed(Failure::Upload(reason.clone())),
            }
        } else {
            attachment::inspect(path, self.source(path), self.destination)
        };
        self.readiness.insert(path.to_path_buf(), readiness);
    }

    pub(crate) fn inspect_all(&mut self) {
        self.readiness.clear();
        for path in self.paths.clone() {
            self.inspect(&path);
        }
    }

    pub(crate) fn retry(&mut self, path: &Path) {
        if matches!(self.uploads.get(path), Some(UploadState::Failed(_))) {
            self.uploads
                .insert(path.to_path_buf(), UploadState::Pending);
        }
        self.inspect(path);
    }

    pub(crate) fn pending_uploads(&mut self) -> Vec<(PathBuf, u64)> {
        let mut pending = Vec::new();
        for path in &self.paths {
            if matches!(self.uploads.get(path), Some(UploadState::Pending)) {
                self.next_attempt += 1;
                let attempt = self.next_attempt;
                self.uploads.insert(
                    path.clone(),
                    UploadState::Uploading {
                        attempt,
                        sent: 0,
                        total: 0,
                    },
                );
                self.readiness
                    .insert(path.clone(), Readiness::Uploading { sent: 0, total: 0 });
                pending.push((path.clone(), attempt));
            }
        }
        pending
    }

    pub(crate) fn uploading(&self, path: &Path, attempt: u64) -> bool {
        matches!(self.uploads.get(path), Some(UploadState::Uploading { attempt: current, .. }) if *current == attempt)
    }

    pub(crate) fn progress(&mut self, path: &Path, attempt: u64, sent: u64, total: u64) -> bool {
        if !self.uploading(path, attempt) {
            return false;
        }
        self.uploads.insert(
            path.to_path_buf(),
            UploadState::Uploading {
                attempt,
                sent,
                total,
            },
        );
        self.inspect(path);
        true
    }

    pub(crate) fn uploaded(
        &mut self,
        path: &Path,
        attempt: u64,
        result: Result<PathBuf, String>,
    ) -> bool {
        if !self.uploading(path, attempt) {
            return false;
        }
        self.uploads.insert(
            path.to_path_buf(),
            match result {
                Ok(remote) => UploadState::Ready(remote),
                Err(reason) => UploadState::Failed(reason),
            },
        );
        self.inspect(path);
        true
    }

    pub(crate) fn uploaded_paths(&self) -> HashMap<PathBuf, PathBuf> {
        self.uploads
            .iter()
            .filter_map(|(path, state)| match state {
                UploadState::Ready(remote) => Some((path.clone(), remote.clone())),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn content_paths(&self) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let paths = self
            .paths
            .iter()
            .map(|path| match self.uploads.get(path) {
                Some(UploadState::Ready(remote)) => remote.clone(),
                _ => path.clone(),
            })
            .collect();
        let local = self
            .local
            .iter()
            .filter(|path| !matches!(self.uploads.get(*path), Some(UploadState::Ready(_))))
            .cloned()
            .collect();
        (paths, local)
    }

    pub(crate) fn wire_paths(&self) -> Vec<PathBuf> {
        self.paths
            .iter()
            .filter_map(|path| match self.uploads.get(path) {
                Some(UploadState::Ready(remote)) => Some(remote.clone()),
                Some(_) => None,
                None => Some(path.clone()),
            })
            .collect()
    }

    /// The first file a send cannot take, and why.
    pub(crate) fn blocked(&self) -> Option<(&Path, &Readiness)> {
        self.paths.iter().find_map(|path| {
            let readiness = self.readiness.get(path)?;
            readiness.blocks().then_some((path.as_path(), readiness))
        })
    }

    /// Swap each file of this computer for its retained copy, so what is
    /// sent is what is kept. A workspace file, or one of this computer that
    /// lies inside `workspace`, stays the live path the agent edits, and on a
    /// machine nothing is copied, for the paths are the machine's. A file
    /// that cannot be copied is marked `Unreadable` and nothing is swapped:
    /// whether the send may go ahead.
    pub(crate) fn retain(&mut self, workspace: Option<&Path>) -> bool {
        if self.destination == Destination::Remote {
            return true;
        }
        let mut paths = Vec::with_capacity(self.paths.len());
        let mut local = Vec::new();
        for path in &self.paths {
            let inside = workspace.is_some_and(|root| inside(path, root));
            let kept = match self.source(path) {
                Source::Workspace => path.clone(),
                Source::Local if inside => path.clone(),
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

/// Whether `path` lies under `root`, either as written or once symlinks such
/// as macOS's `/tmp` are resolved.
fn inside(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
        || match (path.canonicalize(), root.canonicalize()) {
            (Ok(path), Ok(root)) => path.starts_with(root),
            _ => false,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote() -> DraftFiles {
        let mut files = DraftFiles::new();
        files.aim(Destination::Remote, std::env::temp_dir());
        files
    }

    #[test]
    fn remote_upload_failure_preserves_source_and_retry_sends_only_receipt() {
        let mut files = remote();
        let source = PathBuf::from("/local-only/screenshot.png");
        let receipt = PathBuf::from("/server-only/attachments/hash/screenshot.png");
        files.add(&source, Source::Local);
        assert!(matches!(files.blocked(), Some((_, Readiness::Pending))));
        assert!(files.wire_paths().is_empty());
        let [(_, first)] = files.pending_uploads().try_into().unwrap();
        assert!(files.progress(&source, first, 4, 10));
        assert!(matches!(
            files.blocked(),
            Some((_, Readiness::Uploading { sent: 4, total: 10 }))
        ));
        assert!(files.uploaded(&source, first, Err("Connection closed".into())));
        files.inspect_all();
        assert!(matches!(
            files.blocked(),
            Some((_, Readiness::Failed(Failure::Upload(_))))
        ));
        assert_eq!(
            files.content_paths(),
            (vec![source.clone()], vec![source.clone()])
        );
        assert!(files.pending_uploads().is_empty());
        files.retry(&source);
        let [(_, second)] = files.pending_uploads().try_into().unwrap();
        assert_ne!(first, second);
        assert!(!files.uploaded(&source, first, Ok(receipt.clone())));
        assert!(files.uploaded(&source, second, Ok(receipt.clone())));
        assert!(files.blocked().is_none());
        assert_eq!(files.wire_paths(), [receipt.clone()]);
        assert_eq!(files.content_paths(), (vec![receipt], vec![]));
        assert_eq!(files.paths(), [source]);
    }

    #[test]
    fn remove_readd_and_draft_replacement_reject_old_uploads() {
        let mut files = remote();
        let source = PathBuf::from("/local-only/file.txt");
        files.add(&source, Source::Local);
        let [(_, removed)] = files.pending_uploads().try_into().unwrap();
        files.remove(&source);
        assert!(!files.uploading(&source, removed));
        files.add(&source, Source::Local);
        let [(_, replaced)] = files.pending_uploads().try_into().unwrap();
        assert!(!files.uploaded(&source, removed, Ok("/server/stale.txt".into())));
        files.replace(vec![source.clone()], vec![source.clone()]);
        let [(_, current)] = files.pending_uploads().try_into().unwrap();
        assert!(!files.progress(&source, replaced, 100, 100));
        assert!(!files.uploaded(&source, replaced, Ok("/server/stale.txt".into())));
        assert!(files.uploading(&source, current));
        assert!(files.wire_paths().is_empty());
    }

    #[test]
    fn changing_destination_discards_receipts_and_inflight_attempts() {
        let mut files = remote();
        let source = PathBuf::from("/local-only/file.txt");
        files.add(&source, Source::Local);
        let [(_, first)] = files.pending_uploads().try_into().unwrap();
        files.aim(Destination::Remote, std::env::temp_dir());
        assert!(!files.uploaded(&source, first, Ok("/first-server/file.txt".into())));
        let [(_, second)] = files.pending_uploads().try_into().unwrap();
        files.uploaded(&source, second, Ok("/second-server/file.txt".into()));
        files.aim(Destination::Remote, std::env::temp_dir());
        assert!(matches!(files.blocked(), Some((_, Readiness::Pending))));
        assert!(files.wire_paths().is_empty());
    }

    #[test]
    fn remote_workspace_references_never_require_local_files() {
        let mut files = remote();
        let path = PathBuf::from("/server-only/workspace/does-not-exist-here.png");
        files.add(&path, Source::Workspace);
        files.inspect_all();
        assert!(files.blocked().is_none());
        assert!(files.pending_uploads().is_empty());
        assert!(files.retain(None));
        assert_eq!(files.wire_paths(), [path]);
    }
}
