//! Setup sync (ADR 0048): the operator's agent setup kept the same on this
//! computer and each Machine, both ways, this computer the hub. What is
//! synced is [`ITEMS`], and nothing outside it is ever read or written.
//! Each side resolves the table against its own home directory as the
//! tree root `Root::Setup`, so a sync is tree ops over the protocol:
//! [`run`] scans both sides, [`reconcile`]s them with the state they last
//! agreed on, and moves what changed.

use crate::store::Provider;

mod portable;
mod reconcile;
mod run;
mod store;
#[cfg(test)]
mod tests;

pub use portable::Spelling;
pub use reconcile::{agreed, reconcile, Action, Hold};
pub use run::{run, Job, Kept, Outcome, Synced, Valve, BASE, CONFLICTS};
pub use store::{Home, Keychain};
pub(crate) use store::Place;

/// One part of the operator's setup, by its path from the home directory;
/// `.codex/…` is from `$CODEX_HOME` when that is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub path: &'static str,
    pub source: Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A folder and all it holds but the paths in `except`, from it, and
    /// any folder named in [`LEFT_OUT`].
    Tree { except: &'static [&'static str] },
    File,
    /// A JSON file whose strings that name a place in the home directory
    /// are carried as `<home>/…`.
    PortableJson,
    /// One key of a JSON file its agent keeps much else in, carried as
    /// portable JSON.
    JsonKey(&'static str),
    /// A sign-in: owner-only, never logged, and on macOS Claude's is the
    /// Keychain's item, not a file.
    Credential(Provider),
}

pub const ITEMS: &[Item] = &[
    Item { path: ".claude/skills", source: Source::Tree { except: &[] } },
    Item { path: ".claude/commands", source: Source::Tree { except: &[] } },
    Item { path: ".claude/agents", source: Source::Tree { except: &[] } },
    Item { path: ".claude/rules", source: Source::Tree { except: &[] } },
    Item {
        path: ".claude/plugins",
        source: Source::Tree { except: &["installed_plugins.json", "known_marketplaces.json"] },
    },
    Item { path: ".claude/CLAUDE.md", source: Source::File },
    Item { path: ".claude/keybindings.json", source: Source::File },
    Item { path: ".claude/settings.json", source: Source::PortableJson },
    Item { path: ".claude/plugins/installed_plugins.json", source: Source::PortableJson },
    Item { path: ".claude/plugins/known_marketplaces.json", source: Source::PortableJson },
    Item { path: ".claude.json", source: Source::JsonKey("mcpServers") },
    Item { path: ".claude/.credentials.json", source: Source::Credential(Provider::Claude) },
    // Codex writes its own bundled skills under `.system`, each side for
    // its own version.
    Item { path: ".codex/skills", source: Source::Tree { except: &[".system"] } },
    Item { path: ".codex/prompts", source: Source::Tree { except: &[] } },
    Item { path: ".codex/AGENTS.md", source: Source::File },
    Item { path: ".codex/config.toml", source: Source::File },
    Item { path: ".codex/auth.json", source: Source::Credential(Provider::Codex) },
];

/// Never synced inside a tree, at any depth: a clone's git state, which
/// each side's agent fetches into on its own (a plugin marketplace is a
/// clone). Its files are synced.
pub const LEFT_OUT: &[&str] = &[".git"];

/// Where macOS keeps Claude's sign-in: its Keychain item's service.
pub const CLAUDE_KEYCHAIN: &str = "Claude Code-credentials";
