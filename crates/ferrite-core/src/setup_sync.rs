//! Setup sync (ADR 0048, ADR 0050): the operator's agent setup kept the same
//! on this computer and each Machine, both ways, this computer the hub.
//! What is synced is [`ITEMS`], an allowlist of what the operator owns:
//! files they write (authored) and choices they made, kept as keys of a
//! file their tool keeps other things in (chosen). What a tool builds for
//! itself (plugin downloads, caches, install records, its own account
//! sync) is derived, has no entry, and never moves: each side's tool builds
//! its own. Nothing outside the table is written, and nothing outside it is
//! read but what a link inside it points at. Each side resolves the table
//! against its own home directory as the tree root `Root::Setup`, so a sync
//! is tree ops over the protocol: [`run`] scans both sides, [`reconcile`]s
//! them with the state they last agreed on, and moves what changed.

use crate::provider::Provider;

mod installs;
mod keys;
mod links;
mod portable;
mod reconcile;
mod run;
mod store;
#[cfg(test)]
mod tests;

pub use installs::{Installer, Installs};
pub use portable::Spelling;
pub use reconcile::{agreed, reconcile, Action, Hold};
pub use run::{run, Job, Kept, Names, Outcome, Replaced, Synced, Valve, BASE, CONFLICTS};
pub use store::{Home, Keychain, TRASH};
pub(crate) use store::Place;

/// Which table this Ferrite syncs by. Both sides of a sync must name the
/// same one: a Server refuses the Setup root of any other, so two Ferrites
/// whose tables differ move nothing, and a base is read only by the table
/// that wrote it. The first table, 0.4.0's, synced plugin downloads and
/// whole tool files. The second, never released, carried every key of
/// Claude's settings and removed on a Machine with no trash.
pub const TABLE: u32 = 3;

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
    /// Authored: a folder and all it holds but the paths in `except`, from
    /// it, and any folder named in [`LEFT_OUT`].
    Tree { except: &'static [&'static str] },
    /// Authored: a file; an `owner_only` one is made owner-only when new.
    File { owner_only: bool },
    /// Chosen: the keys of a file its tool keeps much else in. Each is an
    /// entry of its own, `<path>#<key>`, and the rest of the file is never
    /// read into a manifest and never written.
    Keys { format: Format, keys: &'static [Key] },
    /// A sign-in: owner-only, never logged, and on macOS Claude's is the
    /// Keychain's item, not a file.
    Credential(Provider),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Toml,
}

/// A top-level key of a chosen file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    /// Its name; [`OTHERS`] is every top-level key no other key names.
    pub name: &'static str,
    pub shape: Shape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Its value is one entry.
    Whole,
    /// Each entry of the object or table it holds is an entry of its own,
    /// `<name>/<entry>`: two sides that each add a plugin both keep theirs.
    Each,
}

/// The [`Key::name`] of every top-level key no other key of its file names.
pub const OTHERS: &str = "*";

impl Key {
    const fn whole(name: &'static str) -> Self {
        Self { name, shape: Shape::Whole }
    }

    const fn each(name: &'static str) -> Self {
        Self { name, shape: Shape::Each }
    }
}

const FILE: Source = Source::File { owner_only: false };

const fn tree(except: &'static [&'static str]) -> Source {
    Source::Tree { except }
}

pub const ITEMS: &[Item] = &[
    // Skills an installer keeps for every agent; `~/.claude/skills` links
    // into it.
    Item { path: ".agents/skills", source: tree(&[]) },
    // Claude's account sync keeps its own bookkeeping in `synced`.
    Item { path: ".claude/skills", source: tree(&["synced", ".trash"]) },
    Item { path: ".claude/commands", source: tree(&[]) },
    Item { path: ".claude/agents", source: tree(&[]) },
    Item { path: ".claude/rules", source: tree(&[]) },
    Item { path: ".claude/hooks", source: tree(&[]) },
    Item { path: ".claude/output-styles", source: tree(&[]) },
    Item { path: ".claude/CLAUDE.md", source: FILE },
    Item { path: ".claude/keybindings.json", source: FILE },
    Item { path: ".claude/statusline.sh", source: FILE },
    Item {
        path: ".claude/settings.json",
        source: Source::Keys {
            format: Format::Json,
            keys: &[Key::each("enabledPlugins"), Key::each("extraKnownMarketplaces"), Key::whole(OTHERS)],
        },
    },
    Item {
        path: ".claude.json",
        source: Source::Keys { format: Format::Json, keys: &[Key::each("mcpServers")] },
    },
    Item { path: ".claude/.credentials.json", source: Source::Credential(Provider::Claude) },
    // Codex writes its own bundled skills under `.system`, each side for
    // its own version.
    Item { path: ".codex/skills", source: tree(&[".system"]) },
    Item { path: ".codex/prompts", source: tree(&[]) },
    Item { path: ".codex/rules", source: tree(&[]) },
    Item { path: ".codex/AGENTS.md", source: FILE },
    // An allowlist: `projects`, `notify` and `skills.config` name this
    // computer's own places, and are never read.
    Item {
        path: ".codex/config.toml",
        source: Source::Keys {
            format: Format::Toml,
            keys: &[
                Key::whole("model"),
                Key::whole("model_reasoning_effort"),
                Key::whole("model_reasoning_summary"),
                Key::whole("model_verbosity"),
                Key::whole("plan_mode_reasoning_effort"),
                Key::each("plugins"),
                Key::each("marketplaces"),
                Key::each("mcp_servers"),
            ],
        },
    },
    Item { path: ".codex/auth.json", source: Source::Credential(Provider::Codex) },
];

/// Never synced inside a tree, at any depth: a clone's git state, which
/// each side keeps itself. Its files are synced.
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

/// The chosen file `path` is a key of, and the key: `<file>#<key>`.
pub fn chosen(path: &str) -> Option<(&'static Item, &str)> {
    ITEMS.iter().find_map(|item| match item.source {
        Source::Keys { .. } => Some((item, path.strip_prefix(item.path)?.strip_prefix('#')?)),
        _ => None,
    })
}

/// The file a setup path lies in: a chosen key's file, else the path.
pub fn file_of(path: &str) -> &str {
    chosen(path).map_or(path, |(item, _)| item.path)
}

/// The plugin a problem at `path` says could not be installed: its
/// `<plugin>@<marketplace>`.
pub fn installing(path: &str) -> Option<&str> {
    path.strip_prefix(installs::FAILED)
}
