//! A Thread's launch arguments (`thread.start --args`, ADR 0030), saved
//! beside its log as its queue is: a revive, and so a Server's restart,
//! starts the Thread with them again, as the safety and profile arguments
//! Projects launches with must.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::Provider;
use crate::ThreadId;

/// The arguments, and the Provider they were written for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct LaunchArgs {
    pub provider: Provider,
    pub args: Vec<String>,
}

/// What a Session of `provider` starts with: the saved arguments, for
/// their Provider only.
pub(super) fn for_provider(saved: Option<&LaunchArgs>, provider: Provider) -> &[String] {
    match saved {
        Some(saved) if saved.provider == provider => &saved.args,
        _ => &[],
    }
}

/// Where `thread`'s are saved, in the store at `dir`.
pub(super) fn path(dir: &Path, thread: ThreadId) -> PathBuf {
    dir.join(thread.to_string()).join("args.json")
}

/// Saved beside and renamed over, behind a barrier, as the queue is.
pub(super) fn save(path: &Path, saved: &LaunchArgs) -> io::Result<()> {
    let temporary = path.with_extension("json.tmp");
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(&serde_json::to_vec(saved)?)?;
    crate::store::sync_ordered(&file)?;
    std::fs::rename(temporary, path)
}

/// The saved arguments; `Ok(None)` for a Thread started without any, and
/// why they could not be read otherwise.
pub(super) fn load(path: &Path) -> Result<Option<LaunchArgs>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| error.to_string()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}
