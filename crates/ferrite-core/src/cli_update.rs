//! What a check of a provider CLI found, as the protocol carries it and
//! a window shows it: no copy, the newest release, or a newer one and
//! the one command that upgrades it. Checking and upgrading are the
//! Server's (`providers::update`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One upgrade, ready to run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upgrade {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Put ahead of PATH for the child: an nvm npm is a node script, and
    /// the app's PATH may not hold that node.
    pub path_prefix: Option<PathBuf>,
}

impl Upgrade {
    /// The command as an operator would type it.
    pub fn display(&self) -> String {
        let program = self
            .program
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.program.display().to_string());
        std::iter::once(program)
            .chain(self.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// What a check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// No copy is installed; there is nothing to upgrade.
    Missing,
    UpToDate {
        current: String,
    },
    Available {
        current: String,
        latest: String,
        /// `None`: Ferrite cannot tell how this copy was installed, and
        /// the operator upgrades it by hand.
        upgrade: Option<Upgrade>,
    },
}
