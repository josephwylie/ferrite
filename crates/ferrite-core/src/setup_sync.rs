//! Setup sync (ADR 0048, ADR 0050): the operator's agent setup kept the same
//! on this computer and each Machine, both ways, this computer the hub.
//! What is synced is [`ITEMS`], an allowlist of what the operator owns:
//! files they write (authored) and choices they made, kept as keys of a
//! file their tool keeps other things in (chosen). What a tool builds for
//! itself (plugin downloads, caches, install records, its own account
//! sync) is derived, has no entry, and never moves: each side's tool builds
//! its own. A sign-in is each computer's own, and has no entry either.
//! Nothing outside the table is written, and nothing outside it is read but
//! what a link inside it points at. Each side resolves the table against
//! its own home directory as the tree root `Root::Setup`, so a sync
//! is tree ops over the protocol: [`run`] scans both sides, [`reconcile`]s
//! them with the state they last agreed on, and moves what changed.

mod installs;
mod keys;
mod links;
mod lock;
mod portable;
mod reconcile;
mod run;
mod store;
#[cfg(test)]
mod tests;

pub use installs::{installing, Installer, Installs};
pub(crate) use lock::Held;
pub use portable::Spelling;
pub use reconcile::{agreed, reconcile, Action, Hold};
pub use run::{run, Job, Kept, Names, Outcome, Replaced, Synced, Valve, BASE, CONFLICTS};
pub use store::{Home, TRASH};
pub(crate) use store::Place;

/// Which table this Ferrite syncs by. Both sides of a sync must name the
/// same one: a Server refuses the Setup root of any other, so two Ferrites
/// whose tables differ move nothing, and a base is read only by the table
/// that wrote it. The first table, 0.4.0's, synced plugin downloads and
/// whole tool files. The second, never released, carried every key of
/// Claude's settings and removed on a Machine with no trash. The third
/// carried each side's Claude and Codex sign-in, which renew themselves
/// and retire every other copy.
pub const TABLE: u32 = 4;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Toml,
}

/// A top-level key of a chosen file. A key the table does not name is
/// never read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub name: &'static str,
    pub shape: Shape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Its value is one entry.
    Whole,
    /// Each entry of the object or table it holds is an entry of its own,
    /// `<name>/<entry>`: two sides that each add a plugin both keep theirs.
    Each(Entries),
}

/// What each entry of an [`Shape::Each`] key is, and so when it travels.
/// One that does not is held, unsaid, as a value naming a place of this
/// computer's own is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entries {
    /// A plugin, `<plugin>@<marketplace>`: it travels when its
    /// marketplace, the entry of that name of the key `markets`, does.
    Plugins { markets: &'static str },
    /// A marketplace: it travels when its source is remote (a GitHub repo,
    /// a git URL, a URL), never one in a folder or bundled with its tool.
    /// It carries `carried` alone when that is given, so what its tool
    /// keeps of it for itself stays each side's.
    Markets { carried: Option<&'static [&'static str]> },
    /// An MCP server: it travels unless what it runs names a path outside
    /// what the table syncs, which may not be there on another side.
    Servers,
}

impl Key {
    const fn whole(name: &'static str) -> Self {
        Self { name, shape: Shape::Whole }
    }

    const fn each(name: &'static str, entries: Entries) -> Self {
        Self { name, shape: Shape::Each(entries) }
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
    // An allowlist: a key that names a command a computer runs (`hooks`,
    // `statusLine`, `apiKeyHelper` and the like), its environment, or
    // how this computer shows Claude is that computer's own.
    Item {
        path: ".claude/settings.json",
        source: Source::Keys {
            format: Format::Json,
            keys: &[
                Key::each("enabledPlugins", Entries::Plugins { markets: "extraKnownMarketplaces" }),
                Key::each("extraKnownMarketplaces", Entries::Markets { carried: None }),
                Key::whole("permissions"),
                Key::whole("model"),
                Key::whole("effortLevel"),
                Key::whole("outputStyle"),
                Key::whole("skillOverrides"),
                Key::whole("modelSettings"),
                Key::whole("includeCoAuthoredBy"),
            ],
        },
    },
    Item {
        path: ".claude.json",
        source: Source::Keys { format: Format::Json, keys: &[Key::each("mcpServers", Entries::Servers)] },
    },
    // Codex writes its own bundled skills under `.system`, each side for
    // its own version.
    Item { path: ".codex/skills", source: tree(&[".system"]) },
    Item { path: ".codex/prompts", source: tree(&[]) },
    Item { path: ".codex/rules", source: tree(&[]) },
    Item { path: ".codex/AGENTS.md", source: FILE },
    // An allowlist: `projects`, `notify` and `skills.config` name this
    // computer's own places, and are never read. A marketplace carries
    // where it is fetched from, never what Codex notes of its fetches.
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
                Key::each("plugins", Entries::Plugins { markets: "marketplaces" }),
                Key::each(
                    "marketplaces",
                    Entries::Markets { carried: Some(&["source_type", "source", "ref", "sparse_paths"]) },
                ),
                Key::each("mcp_servers", Entries::Servers),
            ],
        },
    },
];

/// Never synced inside a tree, at any depth, in any case: a clone's git
/// state, which each side keeps itself (its files are synced), and what a
/// file browser leaves in a folder it showed.
pub const LEFT_OUT: &[&str] = &[".git", ".DS_Store", "Thumbs.db", "desktop.ini"];

/// Whether the name `name` is one of [`LEFT_OUT`].
pub(crate) fn left_out(name: &str) -> bool {
    LEFT_OUT.iter().any(|left| left.eq_ignore_ascii_case(name))
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
