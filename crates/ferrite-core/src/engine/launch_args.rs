//! A Thread's launch arguments (`thread.start --args --env --launcher`,
//! ADR 0030, ADR 0031), saved beside its log as its queue is: a revive,
//! and so a Server's restart, starts the Thread with them again, as the
//! safety and profile arguments Projects launches with must.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::AgentLaunch;
use crate::store::Provider;
use crate::ThreadId;

/// The arguments, environment and launcher, and the Provider they were
/// written for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct LaunchArgs {
    pub provider: Provider,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub launcher: Vec<String>,
}

impl LaunchArgs {
    /// What a Thread of `provider` saves of `launch`: nothing when it adds
    /// nothing to Ferrite's own.
    pub fn of(provider: Provider, launch: &super::Launch) -> Option<Self> {
        let saved = Self {
            provider,
            args: launch.args.clone(),
            env: launch.env.clone(),
            launcher: launch.launcher.clone(),
        };
        (!saved.args.is_empty() || !saved.env.is_empty() || !saved.launcher.is_empty())
            .then_some(saved)
    }
}

/// What a Session of `provider` starts with: the saved launch, for its
/// Provider only.
pub(super) fn for_provider(saved: Option<&LaunchArgs>, provider: Provider) -> AgentLaunch<'_> {
    match saved {
        Some(saved) if saved.provider == provider => AgentLaunch {
            args: &saved.args,
            env: &saved.env,
            launcher: &saved.launcher,
        },
        _ => AgentLaunch::default(),
    }
}

/// Where `thread`'s are saved, in the store at `dir`.
pub(super) fn path(dir: &Path, thread: ThreadId) -> PathBuf {
    dir.join(thread.to_string()).join("args.json")
}

/// Saved beside and renamed over, behind a barrier, as the queue is;
/// nothing saved is no file.
pub(super) fn save(path: &Path, saved: Option<&LaunchArgs>) -> io::Result<()> {
    let Some(saved) = saved else {
        return match std::fs::remove_file(path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    };
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
