//! The agent backend a Thread runs on (CONTEXT.md: Provider), as its log
//! records it and the protocol names it.

use serde::{Deserialize, Serialize};

/// Which agent backend serves this Thread — persisted so a restart knows
/// which provider to revive the Thread on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Codex,
}

impl Provider {
    /// Every provider, in the order surfaces list them.
    pub const ALL: [Provider; 2] = [Provider::Claude, Provider::Codex];
}

impl std::fmt::Display for Provider {
    /// The provider's name as a Notice says it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Provider::Claude => "Claude",
            Provider::Codex => "Codex",
        })
    }
}

/// A provider and the model to serve it with — what the #25 picker picks,
/// whole. `None` is the provider's own default, which is also what every
/// Thread starts on.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProviderChoice {
    pub provider: Provider,
    pub model: Option<String>,
}
