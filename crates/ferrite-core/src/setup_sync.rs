//! Setup sync (ADR 0048): the operator's agent setup kept the same on this
//! computer and each Machine, both ways, this computer the hub. What is
//! synced is [`ITEMS`]; nothing outside it is written, and nothing outside
//! it is read but what a link inside it points at. Each side resolves the
//! table against its own home directory as the tree root `Root::Setup`, so
//! a sync is tree ops over the protocol: [`run`] scans both sides,
//! [`reconcile`]s them with the state they last agreed on, and moves what
//! changed.

use crate::store::Provider;

mod portable;
mod reconcile;
mod run;
mod store;
#[cfg(test)]
mod tests;

pub use portable::Spelling;
pub use reconcile::{agreed, reconcile, Action, Hold};
pub use run::{run, Job, Kept, Names, Outcome, Synced, Valve, BASE, CONFLICTS, TRASH};
pub use store::{Home, Keychain};
pub(crate) use store::Place;

/// One part of the operator's setup, by its path from the home directory;
/// `.claude/…` is from `$CLAUDE_CONFIG_DIR` and `.codex/…` from
/// `$CODEX_HOME` when those are set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub path: &'static str,
    pub source: Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A folder and all it holds but the paths in `except`, from it (a
    /// last name of `*.lock` is any name ending so), and any folder named
    /// in [`LEFT_OUT`].
    Tree { except: &'static [&'static str] },
    /// A folder whose folders are each copied whole, `.git` with it, to a
    /// side that has none, and otherwise left as each side has them: a
    /// plugin marketplace is a clone each side's agent keeps up itself.
    Seed,
    /// A file; an `owner_only` one is made owner-only when it is new.
    File { owner_only: bool },
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

const FILE: Source = Source::File { owner_only: false };

pub const ITEMS: &[Item] = &[
    Item { path: ".claude/skills", source: Source::Tree { except: &[] } },
    Item { path: ".claude/commands", source: Source::Tree { except: &[] } },
    Item { path: ".claude/agents", source: Source::Tree { except: &[] } },
    Item { path: ".claude/rules", source: Source::Tree { except: &[] } },
    Item { path: ".claude/hooks", source: Source::Tree { except: &[] } },
    Item { path: ".claude/output-styles", source: Source::Tree { except: &[] } },
    Item {
        path: ".claude/plugins",
        source: Source::Tree {
            except: &["installed_plugins.json", "known_marketplaces.json", "marketplaces", "store/*.lock"],
        },
    },
    Item { path: ".claude/plugins/marketplaces", source: Source::Seed },
    Item { path: ".claude/CLAUDE.md", source: FILE },
    Item { path: ".claude/keybindings.json", source: FILE },
    Item { path: ".claude/statusline.sh", source: FILE },
    Item { path: ".claude/settings.json", source: Source::PortableJson },
    Item { path: ".claude/plugins/installed_plugins.json", source: Source::PortableJson },
    Item { path: ".claude/plugins/known_marketplaces.json", source: Source::PortableJson },
    Item { path: ".claude.json", source: Source::JsonKey("mcpServers") },
    Item { path: ".claude/.credentials.json", source: Source::Credential(Provider::Claude) },
    // Codex writes its own bundled skills under `.system`, each side for
    // its own version.
    Item { path: ".codex/skills", source: Source::Tree { except: &[".system"] } },
    Item { path: ".codex/prompts", source: Source::Tree { except: &[] } },
    Item { path: ".codex/rules", source: Source::Tree { except: &[] } },
    Item { path: ".codex/AGENTS.md", source: FILE },
    Item { path: ".codex/config.toml", source: Source::File { owner_only: true } },
    Item { path: ".codex/auth.json", source: Source::Credential(Provider::Codex) },
];

/// Never synced inside a tree, at any depth: a clone's git state, which
/// each side's agent fetches into on its own. Its files are synced.
pub const LEFT_OUT: &[&str] = &[".git"];

/// Where macOS keeps Claude's sign-in: its Keychain item's service, with
/// `-<8 hex of the folder's SHA-256>` after it when `CLAUDE_CONFIG_DIR`
/// moves Claude's folder.
pub const CLAUDE_KEYCHAIN: &str = "Claude Code-credentials";

/// Claude's sign-in, the one sign-in a Keychain may keep.
pub(crate) const CLAUDE_SIGN_IN: &str = ".claude/.credentials.json";

/// The table's paths a sync that reads no sign-ins leaves unread: Claude's,
/// which macOS keeps in a Keychain whose every read may ask the operator.
pub const UNREAD_WITHOUT_SIGN_INS: &[&str] = &[CLAUDE_SIGN_IN];

/// The table's item at `path`, when `path` is one, not a path in one.
pub(crate) fn item_at(path: &str) -> Option<&'static Item> {
    ITEMS.iter().find(|item| item.path == path)
}

/// Whether `path` is a sign-in.
pub(crate) fn sign_in(path: &str) -> bool {
    item_at(path).is_some_and(|item| matches!(item.source, Source::Credential(_)))
}
