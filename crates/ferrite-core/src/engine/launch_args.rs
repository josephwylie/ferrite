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
    /// What the first Session of a Thread of `provider` starts with of
    /// `launch`: nothing when it adds nothing to Ferrite's own.
    pub fn of(provider: Provider, launch: &super::Launch) -> Option<Self> {
        Self {
            provider,
            args: launch.args.clone(),
            env: launch.env.clone(),
            launcher: launch.launcher.clone(),
        }
        .adding()
    }

    /// What every later Session starts with, and so what is saved: the
    /// same, less the id of the conversation the first one began. Claude
    /// refuses `--session-id` once that conversation exists ("Session ID …
    /// is already in use"; beside `--resume`, "--session-id can only be
    /// used with --continue or --resume if --fork-session is also
    /// specified"), so a later Session carries it on by the Thread's own
    /// resume instead. Codex is never given one: herdr names a new Codex
    /// conversation by no flag.
    pub fn later(self) -> Option<Self> {
        let mut args = Vec::with_capacity(self.args.len());
        let mut words = self.args.into_iter();
        while let Some(word) = words.next() {
            if self.provider == Provider::Claude {
                if word == "--session-id" {
                    words.next();
                    continue;
                }
                if word.starts_with("--session-id=") {
                    continue;
                }
            }
            args.push(word);
        }
        Self { args, ..self }.adding()
    }

    /// These, when they add anything to Ferrite's own.
    fn adding(self) -> Option<Self> {
        (!self.args.is_empty() || !self.env.is_empty() || !self.launcher.is_empty()).then_some(self)
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
/// why they could not be read otherwise. Those an earlier Ferrite saved
/// whole are read as a later Session's (`LaunchArgs::later`).
pub(super) fn load(path: &Path) -> Result<Option<LaunchArgs>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<LaunchArgs>(&bytes)
            .map(LaunchArgs::later)
            .map_err(|error| error.to_string()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(provider: Provider, args: &[&str]) -> LaunchArgs {
        LaunchArgs {
            provider,
            args: args.iter().map(|arg| arg.to_string()).collect(),
            env: Vec::new(),
            launcher: Vec::new(),
        }
    }

    /// The Coordinator's start (herdr-projects' `start_args`): the
    /// conversation's id is the first Session's alone; the rest stays.
    #[test]
    fn a_later_claude_session_does_not_name_the_conversation_again() {
        for args in [
            &["--session-id", "4a76", "--dangerously-skip-permissions"][..],
            &["--session-id=4a76", "--dangerously-skip-permissions"],
        ] {
            let later = launch(Provider::Claude, args).later().unwrap();
            assert_eq!(later.args, ["--dangerously-skip-permissions"], "{args:?}");
        }
    }

    #[test]
    fn a_claude_launch_of_only_a_session_id_leaves_nothing_for_later() {
        assert_eq!(
            launch(Provider::Claude, &["--session-id", "4a76"]).later(),
            None
        );
    }

    /// Codex takes no such flag: its arguments are kept as they are.
    #[test]
    fn a_later_codex_session_starts_with_the_same_arguments() {
        let args = ["--dangerously-bypass-approvals-and-sandbox", "--no-daemon"];
        let later = launch(Provider::Codex, &args).later().unwrap();
        assert_eq!(later.args, args);
    }

    /// args.json from before: saved whole, so its `--session-id` would
    /// make every revive fail. Read, it no longer has it.
    #[test]
    fn arguments_saved_whole_are_loaded_as_a_later_session_s() {
        let dir = std::env::temp_dir().join(format!("ferrite-launch-args-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("args.json");
        std::fs::write(
            &path,
            r#"{"provider":"claude","args":["--session-id","4a76","--dangerously-skip-permissions"]}"#,
        )
        .unwrap();
        let loaded = load(&path).unwrap().unwrap();
        assert_eq!(loaded.args, ["--dangerously-skip-permissions"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
