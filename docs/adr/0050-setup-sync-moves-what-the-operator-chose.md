---
status: accepted
date: 2026-11-25
---

# Setup sync moves what the operator chose, and nothing a tool derives

This supersedes the table of ADR 0048 section 3, with its seeded
marketplaces, its portable JSON files and its whole `config.toml`. The rest
of that section stands: this computer is the hub, the base is three-way, and
the valve, sign-ins, timing, row and toast work as it says.

## Why

The first real run of ADR 0048's table, across a Mac, a Windows computer and
a Linux Machine, raised thousands of false conflicts. Plugin downloads and
caches differed only in line ends. Install records named each side's own
paths. Claude's account sync kept its bookkeeping under `skills/synced`.
Codex's `config.toml` went whole from one computer to another, carrying that
computer's `projects`, `notify` and skill paths. Three machines fought over
the plugin records. The kept copies sat under `.claude/…`, which Finder
hides, so "Open folder" showed an empty folder. And `~/.agents/skills`, which
holds most of the operator's skills (`~/.claude/skills/<name>` is usually a
link into it), was not synced at all.

The table treated everything under an agent's folder as the operator's.
Three kinds of thing live there.

## Three kinds

- **Authored**: files the operator writes. They travel as files, three-way.
- **Chosen**: keys inside a file a tool keeps much else in: a model, a plugin
  turned on, an MCP server. Each key travels three-way on its own. The rest
  of the file is never read into a manifest and never written.
- **Derived**: what a tool builds for itself, such as plugin downloads,
  caches, install records and Claude's account sync. It never travels. Each
  side's tool builds its own.

`setup_sync::ITEMS` says which is which, as a `Source`: `Tree { except }`,
`File { owner_only }`, `Keys { format, keys }` or `Credential`. A key is a
`Key { name, shape }`, where `Whole` means its value is one entry and `Each`
means each entry of the object or table it holds is one. `PortableJson`,
`JsonKey` and `Seed` are gone.

## The table

| Paths | Kind |
| --- | --- |
| `.agents/skills` | tree |
| `.claude/skills` | tree, except `synced` and `.trash` (Claude's account sync) |
| `.claude/{commands,agents,rules,hooks,output-styles}` | tree |
| `.claude/{CLAUDE.md,keybindings.json,statusline.sh}` | file |
| `.claude/settings.json` | keys: `enabledPlugins` and `extraKnownMarketplaces` each, every other top-level key whole |
| `.claude.json` | keys: `mcpServers` each |
| `.codex/skills` | tree, except `.system` |
| `.codex/{prompts,rules}` | tree |
| `.codex/AGENTS.md` | file |
| `.codex/config.toml` | keys: `model`, `model_reasoning_effort`, `model_reasoning_summary`, `model_verbosity` and `plan_mode_reasoning_effort` whole; `plugins`, `marketplaces` and `mcp_servers` each |
| `.claude/.credentials.json`, `.codex/auth.json` | sign-in, as before |

`config.toml` is an allowlist. `projects`, `notify`, `skills.config` and
every other key name this computer's own places or state, and are never
read. All of `.claude/plugins`, `.codex/plugins`, `.agents/.skill-lock.json`
and every cache, lock, log, session and database are derived.

## Keys

- A key is an entry named `<file>#<key>`, and an `Each` key's entry is
  `<file>#<key>/<entry>`, as in
  `.claude/settings.json#enabledPlugins/pstack@cursor-plugins`. A name holding
  `%`, `/`, `\`, `:` or a control character, or one of dots only, is spelled
  with `%` and its hex. An empty name is not synced.
- The entry hashes the key's canonical value: JSON with each object's keys
  sorted, pretty-printed, and the home spelled `<home>` as portable JSON
  spells it. A TOML value is carried as that JSON. A tool that reorders or
  re-indents its file, as `claude plugin marketplace add` does, changes
  nothing.
- A write sets one key and keeps every other byte. A JSON file is edited by
  the span of that one value, keeping the file's indent and line ends. A new
  key goes after the last one. A TOML file goes through `toml_edit`, which
  keeps its comments, a value's trailing comment and the order of the rest.
  A removal takes the one key, or the one entry, and an `Each` key's object
  stays even when empty.
- **A value that names an absolute path outside the home** is that
  computer's own, such as a hook or a status line run from `/usr/local/bin`,
  or an MCP server in `C:\tools`. It is held unsaid and never moves, on
  either side. This holds for every chosen key, not only `config.toml`'s.
- **The first sync** unions `Each` entries, so two sides that each turned a
  plugin on both keep both. A key both sides hold differently, with no base,
  takes this computer's value. No copy is kept, and the toast says what it
  was: "`model` in ~/.claude/settings.json was set differently on wsl. It is
  this computer's on both now. It was "sonnet" on wsl." With a base, of two
  changes the newer wins, by the file's modification time, and the toast
  says the same.

## Text

A UTF-8 authored file is listed with a second hash, `text`, of its bytes
with each CR before an LF left out. Two sides compare by it when both have
one, so `CLAUDE.md` with CRLF on Windows and LF on Linux is the same file.
The bytes are sent as they are: an edit made on Windows reaches Linux with
its CRLF. A file that is not UTF-8 has no `text` and compares by its bytes.
Only a Setup scan computes it.

## Links

A link inside a tree whose target lies in an item of the table is listed as
a link (`Entry::Symlink`), with its target as the path from its folder in
the table's paths: `.claude/skills/x -> ../../.agents/skills/x`. The other
side makes the link to where that place is in its own home, relative where
both sit under one root. On Windows it makes a symlink, or a junction when
Windows lets the account make no symlink. Links are made after every file,
so the target is there first. A link out of the table is followed and
listed as before, and nothing is written through it (`Hold::Linked`).

## Installing what was chosen

A plugin turned on is a key. The download is derived, so each side's tool
must install it. Prototyped in homes of their own, with no sign-in:

- **Codex 0.171.0 installs by itself.** As a thread starts (`codex exec` or
  `codex app-server`'s `thread/start`), it clones each marketplace its
  config names and installs each plugin it enables, before any sign-in is
  checked. Ferrite does nothing more for it.
- **Claude Code 2.1.330 does not.** A plugin and its marketplace declared in
  `settings.json` stayed uninstalled through `claude plugin list` and
  `claude -p`. `claude plugin install` refused a plugin of a marketplace it
  had not added. `claude plugin marketplace add cursor/plugins` took about
  4.4 s, then `claude plugin install pstack@cursor-plugins --json` about
  4.6 s. A second install answered "already installed" with outcome `ok`.

So the side that writes one of Claude's `enabledPlugins` or
`extraKnownMarketplaces` keys runs Claude's installer (`Installs`): for each
plugin on and not in `plugins/installed_plugins.json`, `claude plugin
marketplace add <source>` when `known_marketplaces.json` lacks its
marketplace, taken from `extraKnownMarketplaces`, then `claude plugin install
<plugin> --json`. It runs on a thread of its own, one run at a time, and a
write during a run asks for one more. It never runs on the pump. A scan waits
up to 120 s for a run under way, so the sync that turned a plugin on says
what failed, as a problem at `.claude/plugins/<plugin>`. The row reads
"pstack could not be installed on wsl: …" until a later write installs it.
The hub does the same for itself after a pull. There is no new protocol op.
Nothing is ever uninstalled: a plugin turned off is a key set false, which
Claude reads itself.

## The table's version

`setup_sync::TABLE` is 2, and the Setup root carries it:
`{"kind":"setup_table","table":2}`. A Server refuses any other. One by a
newer table is refused as `unsupported`, which the hub's row reads as
"Update Ferrite on <name> to sync your setup". One by an older table, or
ADR 0048's `{"kind":"setup"}`, is refused with "Update Ferrite on this
computer to sync your setup: the Machine's Ferrite is newer". A Server from
before this ADR reads `setup_table` as a root it does not know and refuses
it as `unsupported`, so a new hub never syncs with it. Its row says to
update it, and nothing moves either way.

A Machine's `ferrited` is updated by its setup. Settings › Connections runs
`machine/bootstrap.sh` there, which puts this app's version at
`~/.ferrite/runtime/versions/<version>/ferrited`. The bridge runs that
binary when it is there, else `ferrited` on the PATH. A Server already
running keeps its version until it restarts. The setup says so when the two
differ.

## Old bases

A base written by the first table still names plugin downloads,
`settings.json` whole and the rest. It is read with every entry this table
does not name left out, so no removal is ever planned from one. Its entries
for what this table still names, a skill for one, stand.

## Kept copies and trash

A conflict's losing copy and a removal's trash are kept under the path with
its first dot dropped, as `sync-conflicts/<stamp>/claude/CLAUDE.md` and
`sync-trash/<stamp>/agents/skills/x`, so Finder and Explorer show them. A
trashed key is a file of its value, its name the key's path, with what
Windows refuses spelled `_`.

## Tradeoffs

- **A key's time is its file's.** The newer of two changed keys is the one
  whose file changed later, which may be for another key.
- **Own values never move.** An absolute path outside the home is assumed
  to be this computer's only. A hook the operator keeps at the same path on
  every computer is held on each, unsaid, and set by hand.
- **A Whole key at the first sync loses the Machine's value.** It is said
  once in the toast and not kept. The other choice, the newer, made the
  first run's thousands of conflicts.
- **The installer is Claude's CLI.** It needs the network and `claude` on
  the side's PATH. A Machine without it says so on the row. A run that
  hangs holds the scan 120 s at most, and later scans say what it found.
- **A link out of the table is still followed.** A skills folder kept in a
  dotfiles checkout keeps syncing, at the cost of its contents travelling
  as files.
- **A file link on Windows** needs a symlink, which Windows lets only some
  accounts make. A folder link falls back to a junction. A file link that
  cannot be made fails that sync, and says where.
