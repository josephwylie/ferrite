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

    /// Hold exactly `paths`, `local` among them, and `uploaded`'s copies of
    /// them on the machine aimed at.
    pub(crate) fn replace(
        &mut self,
        paths: Vec<PathBuf>,
        local: Vec<PathBuf>,
        uploaded: Vec<(PathBuf, PathBuf)>,
    ) {
        self.uploads.clear();
        self.paths = paths;
        self.local = local;
        self.copies_there(uploaded);
        self.inspect_all();
    }

    /// Hold `paths` too, ahead of those held when `prepend`.
    pub(crate) fn merge(
        &mut self,
        mut paths: Vec<PathBuf>,
        local: Vec<PathBuf>,
        uploaded: Vec<(PathBuf, PathBuf)>,
        prepend: bool,
    ) {
        paths.retain(|path| !self.contains(path));
        self.local
            .extend(local.into_iter().filter(|path| paths.contains(path)));
        if prepend {
            self.paths.splice(0..0, paths);
        } else {
            self.paths.extend(paths);
        }
        self.copies_there(uploaded);
        self.inspect_all();
    }

    /// The machine aimed at holds `uploaded`'s copies of files of this
    /// computer: they are not sent again.
    fn copies_there(&mut self, uploaded: Vec<(PathBuf, PathBuf)>) {
        if self.destination != Destination::Remote {
            return;
        }
        for (source, copy) in uploaded {
            if self.source(&source) == Source::Local && !self.uploads.contains_key(&source) {
                self.uploads.insert(source, UploadState::Ready(copy));
            }
        }
    }

    fn source(&self, path: &Path) -> Source {
        if self.local.iter().any(|kept| kept == path) {
            Source::Local
        } else {
            Source::Workspace
        }
    }

    /// Look at `path` again: a file restored, repaired or replaced since
    /// it was added reads as it is now. A file of this computer bound for a
    /// machine is uploaded from here, unless it is no longer here and the
    /// machine has no copy of it.
    pub(crate) fn inspect(&mut self, path: &Path) {
        let remote = self.destination == Destination::Remote && self.source(path) == Source::Local;
        let uploaded = matches!(self.uploads.get(path), Some(UploadState::Ready(_)));
        let readiness = if remote && !uploaded && !path.exists() {
            self.uploads.remove(path);
            Readiness::Failed(Failure::Gone)
        } else if remote {
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

    /// What a saved draft keeps: each file as it was added, one of this
    /// computer by its path here, never in its place the machine's copy,
    /// which is kept beside it. Put away, put back or moved, the draft can
    /// send it to either side.
    pub(crate) fn content_paths(&self) -> (Vec<PathBuf>, Vec<PathBuf>, Vec<(PathBuf, PathBuf)>) {
        let mut uploaded: Vec<_> = self.uploaded_paths().into_iter().collect();
        uploaded.sort();
        (self.paths.clone(), self.local.clone(), uploaded)
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
        self.replace(paths, local, Vec::new());
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

    /// A file of this computer, there to upload.
    fn here(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("draft-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, name).unwrap();
        path
    }

    fn remote() -> DraftFiles {
        let mut files = DraftFiles::new();
        files.aim(Destination::Remote, std::env::temp_dir());
        files
    }

    #[test]
    fn remote_upload_failure_preserves_source_and_retry_sends_only_receipt() {
        let mut files = remote();
        let source = here("screenshot.png");
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
            (vec![source.clone()], vec![source.clone()], vec![])
        );
        assert!(files.pending_uploads().is_empty());
        files.retry(&source);
        let [(_, second)] = files.pending_uploads().try_into().unwrap();
        assert_ne!(first, second);
        assert!(!files.uploaded(&source, first, Ok(receipt.clone())));
        assert!(files.uploaded(&source, second, Ok(receipt.clone())));
        assert!(files.blocked().is_none());
        assert_eq!(files.wire_paths(), [receipt]);
        assert_eq!(files.paths(), [source]);
    }

    /// A file of this computer, uploaded to a machine, is saved by its path
    /// here, its copy there beside it: put back on that machine it is the
    /// copy, gone from here or not, and on this computer it is sent from
    /// here.
    #[test]
    fn a_saved_draft_keeps_its_files_of_this_computer_for_either_side() {
        let source = here("notes.md");
        let copy = PathBuf::from("/server-only/attachments/hash/notes.md");
        let mut files = remote();
        files.add(&source, Source::Local);
        let [(_, attempt)] = files.pending_uploads().try_into().unwrap();
        assert!(files.uploaded(&source, attempt, Ok(copy.clone())));
        let (paths, local, uploaded) = files.content_paths();
        assert_eq!(paths, [source.clone()]);
        assert_eq!(local, [source.clone()]);
        assert_eq!(uploaded, [(source.clone(), copy.clone())]);

        let mut back = remote();
        back.replace(paths.clone(), local.clone(), uploaded.clone());
        assert!(back.pending_uploads().is_empty(), "the machine has it");
        assert_eq!(back.wire_paths(), [copy]);
        back.aim(Destination::Local, std::env::temp_dir());
        assert_eq!(back.readiness().get(&source), Some(&Readiness::Ready), "sent from here");

        std::fs::remove_file(&source).unwrap();
        let mut gone = remote();
        gone.replace(paths, local, uploaded);
        assert!(gone.blocked().is_none(), "the machine's copy still goes");
    }

    /// A file of this computer no longer here says so on either side, and
    /// asks for no upload; put back, its retry uploads it.
    #[test]
    fn a_file_gone_from_this_computer_says_so_on_either_side() {
        let source = here("attach-note.txt");
        std::fs::remove_file(&source).unwrap();
        let mut files = DraftFiles::new();
        files.replace(vec![source.clone()], vec![source.clone()], vec![]);
        let gone = Some(&Readiness::Failed(Failure::Gone));
        assert_eq!(files.readiness().get(&source), gone, "here");
        files.aim(Destination::Remote, std::env::temp_dir());
        assert_eq!(files.readiness().get(&source), gone, "on a machine");
        assert!(files.pending_uploads().is_empty());
        assert_eq!(
            Readiness::Failed(Failure::Gone).label(),
            Some("Gone from this computer")
        );

        std::fs::write(&source, "back").unwrap();
        files.retry(&source);
        assert_eq!(files.pending_uploads().len(), 1, "put back, it uploads");
    }

    #[test]
    fn remove_readd_and_draft_replacement_reject_old_uploads() {
        let mut files = remote();
        let source = here("file.txt");
        files.add(&source, Source::Local);
        let [(_, removed)] = files.pending_uploads().try_into().unwrap();
        files.remove(&source);
        assert!(!files.uploading(&source, removed));
        files.add(&source, Source::Local);
        let [(_, replaced)] = files.pending_uploads().try_into().unwrap();
        assert!(!files.uploaded(&source, removed, Ok("/server/stale.txt".into())));
        files.replace(vec![source.clone()], vec![source.clone()], vec![]);
        let [(_, current)] = files.pending_uploads().try_into().unwrap();
        assert!(!files.progress(&source, replaced, 100, 100));
        assert!(!files.uploaded(&source, replaced, Ok("/server/stale.txt".into())));
        assert!(files.uploading(&source, current));
        assert!(files.wire_paths().is_empty());
    }

    #[test]
    fn changing_destination_discards_receipts_and_inflight_attempts() {
        let mut files = remote();
        let source = here("file.txt");
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
