---
status: accepted
date: 2026-11-25
---

# Setup sync moves what the operator chose, and nothing a tool derives

This supersedes the table of ADR 0048 section 3, with its seeded
marketplaces, its portable JSON files and its whole `config.toml`, and all
it says of sign-ins (see Sign-ins). The rest of that section stands: this
computer is the hub, the base is three-way, and the valve, timing, row and
toast work as it says.

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
`File { owner_only }` or `Keys { format, keys }`. A key is a
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
| `.claude/settings.json` | keys: `enabledPlugins` and `extraKnownMarketplaces` each; `permissions`, `model`, `effortLevel`, `outputStyle`, `skillOverrides`, `modelSettings` and `includeCoAuthoredBy` whole |
| `.claude.json` | keys: `mcpServers` each |
| `.codex/skills` | tree, except `.system` |
| `.codex/{prompts,rules}` | tree |
| `.codex/AGENTS.md` | file |
| `.codex/config.toml` | keys: `model`, `model_reasoning_effort`, `model_reasoning_summary`, `model_verbosity` and `plan_mode_reasoning_effort` whole; `plugins`, `marketplaces` (its `source_type`, `source`, `ref` and `sparse_paths` alone) and `mcp_servers` each |

Both chosen files are allowlists, and a key the table does not name is
never read. In `settings.json` that leaves out each key that holds a
command a computer runs (`hooks`, `statusLine`, `apiKeyHelper`,
`awsAuthRefresh`, `otelHeadersHelper`), its `env`, and how this computer
shows Claude (`tui`, `teammateMode`, `preferredNotifChannel`, `theme`). In
`config.toml` it leaves out `projects`, `notify`, `skills.config` and the
rest, which name this computer's own places or state. All of
`.claude/plugins`, `.codex/plugins`, `.agents/.skill-lock.json` and every
cache, lock, log, session and database are derived. Inside a tree,
`.git`, `.DS_Store`, `Thumbs.db` and `desktop.ini` are never synced, in any
case, at any depth.

## Sign-ins

A sign-in is each computer's own: no sync reads, writes or removes one.
Claude's (`.claude/.credentials.json`, on macOS the Keychain item
`Claude Code-credentials`) and Codex's (`.codex/auth.json`) are OAuth
sign-ins that renew themselves, and a renewal retires every other copy.
Table 3 carried them as ADR 0048 had it. Asked whose to keep, the operator
kept the Mac's, and each of the two Machines it went to failed at once:
"OAuth session expired and could not be refreshed". OpenAI's Codex docs
say not to share one `auth.json` between computers.

So neither is in the table, and the row's question, "Signed in
differently on each side", is gone. A sign-in already on either side stays
as it is: none is moved, removed or put back, whatever a base says. Setup
sync no longer reads the Keychain. Each computer is signed in on its own.

## What of an Each key travels

An entry of an `Each` key travels only when what it names is there on
every side:

- **A marketplace** travels only when it is fetched from elsewhere: a
  GitHub `owner/repo`, or a git URL or URL (`https://`, `ssh://`, `git@`).
  One in a folder, one held inline in the settings (Claude's `settings`
  source) and one bundled with its tool (Codex's under `.codex/.tmp` or
  `~/.cache/codex-runtimes`) stays its side's. Codex's carries where it is
  fetched from alone; what Codex notes of each fetch (`last_revision`,
  `last_updated`) stays each side's, and a write keeps it.
- **A plugin** (`<plugin>@<marketplace>`) travels only when its
  marketplace, in the same file, does. Claude's own, `@builtin`, never
  does.
- **An MCP server** stays its side's when its command or any word of it
  names a path outside the home, or a path in the home that the table does
  not sync (a plugin's download, Codex's bundle, a folder of the
  operator's own), since another side may have none there.

What stays is held unsaid, as an own value is.

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
- **A JSON file that holds a chosen key twice**, or an entry of one twice,
  is read one way by its tool (the last) and another by a span writer (the
  first). It is a problem, held, and nothing is written to it.
- **A value that names an absolute path outside the home** is that
  computer's own, such as a hook or a status line run from `/usr/local/bin`,
  or an MCP server in `C:\tools`. It is held unsaid and never moves, on
  either side. This holds for every chosen key, not only `config.toml`'s.
- **The first sync** unions `Each` entries, so two sides that each turned a
  plugin on both keep both. A key both sides hold differently, with no base,
  takes this computer's value. No conflict copy is kept (the old value is
  in that side's trash), and the toast says what it was and where it went:
  "Claude's model was set differently on wsl. It is this computer's on both
  now. It was "sonnet" on wsl, kept in its sync-trash folder." With a base,
  of two changes the newer wins, by the file's modification time, and the
  toast says the same. A toast names a key by what it sets and for which
  tool, and a file by what it is ("the skill both-edit", "the command
  team", "CLAUDE.md"), with a path only where nothing plainer names it.

## Text

A UTF-8 authored file is listed with a second hash, `text`, of its bytes
with each CR before an LF left out. Two sides compare by it when both have
one, so `CLAUDE.md` with CRLF on Windows and LF on Linux is the same file.
The bytes are sent as they are: an edit made on Windows reaches Linux with
its CRLF. A file that is not UTF-8 has no `text` and compares by its bytes.
Only a Setup scan computes it. A file's hash another kind of scan kept, with
no `text`, is not taken by a Setup scan: the file is read again.

## Links

A link inside a tree whose target is a folder in a tree of the table is
listed as a link (`Entry::Symlink`), with its target as the path from its
folder in the table's paths: `.claude/skills/x -> ../../.agents/skills/x`.
The other side makes the link to where that folder is in its own home,
from the link's folder, as a tree makes one (`tree::make_link`, ADR 0048):
a symlink on Unix, a junction on Windows, which needs no privilege. Links
are made after every file, so the target is there first. A link whose
target changed on one side is made anew on the other, its old link moved to
that side's trash. A link to a
file, or out of the table, is followed and listed as before, and nothing
is written through it (`Hold::Linked`): Windows makes a file link only for
some accounts, and a copy in its place would fight the link on the next
sync.

**A folder on one side where the other has a link** is never replaced,
base or no base, unless each entry under it is the same, line ends aside,
as the one at its place in the folder the link leads to on that same side.
Then the side that changed has its way, and with no base the link wins,
the folder going to that side's trash. Else it is held
(`Hold::FolderAndLink`), either way, and the row says so: "the skill reui
is a folder on josephpc and a link here: left as each has it". ADR 0048's
table made a real folder of every linked skill on a Machine, and some of
those were since written to there.

## Installing what was chosen

A plugin turned on is a key. The download is derived, so each side's tool
must install it. Prototyped in homes of their own, with no sign-in:

- **Codex 0.171.0 installs each plugin by itself**, as a thread starts
  (`codex exec` or `codex app-server`'s `thread/start`), before any sign-in
  is checked. But while a marketplace its config names has no copy fetched
  (`.codex/.tmp/marketplaces/<name>`), `codex plugin list` fails.
  `codex plugin marketplace upgrade -- <name>` fetches it and installs its
  enabled plugins.
- **Claude Code 2.1.330 does not.** A plugin and its marketplace declared in
  `settings.json` stayed uninstalled through `claude plugin list` and
  `claude -p`. `claude plugin install` refused a plugin of a marketplace it
  had not added. `claude plugin marketplace add cursor/plugins` took about
  4.4 s, then `claude plugin install pstack@cursor-plugins --json` about
  4.6 s. A second install answered "already installed" with outcome `ok`.
- **What they write.** Run in a copy of a real home, neither command
  removed anything outside `plugins/`. `install` of a plugin already on
  left `settings.json` byte for byte. `marketplace add` wrote the
  marketplace's `extraKnownMarketplaces` entry again, without the
  operator's `"autoUpdate": true`.

So the side a sync writes a plugin or marketplace key to runs the
installers (`Installs`) at its next scan, the one that ends the sync. A run
holds the side's home while it runs (see One sync at a time), so no
installer writes a file a sync is writing: on either side, a run a sync
wanted begins once that sync lets go, before the next sync of that home,
and says what failed at that next sync. Only a plugin of a marketplace
that travels is ever missing:

- **Claude**: each plugin on and not in `plugins/installed_plugins.json`.
  `claude plugin marketplace add -- <source>` runs when
  `known_marketplaces.json` lacks its marketplace, and then the
  marketplace's settings entry is put back as it was. Then `claude plugin
  install --json -- <plugin>`.
- **Codex**: `codex plugin marketplace upgrade -- <name>` for each
  marketplace an enabled plugin is of that has no copy fetched.

Each name handed over is a letter or a digit, then letters, digits, `.`,
`_` and `-`, and each source a GitHub `owner/repo` or a URL as above. Any
other is a failure, never run. Nothing a command starts may ask anyone
anything: git under it is told never to prompt (`GIT_TERMINAL_PROMPT=0`,
`GCM_INTERACTIVE=never`), and a command still running after 120 s is
stopped with all it started, a failure in plain words: "Claude Code's
installer did not finish in 2 minutes, so it was stopped". A run is on a
thread of its own, one at a time, and never on the pump. No scan waits for
a run: every scan is a sync's, which holds the home a run needs. So the
sync after the one that turned a plugin on says what failed, as a problem
at `.claude/plugins/<plugin>` or `.codex/plugins/<plugin>`. The row counts
them as plugins, apart from paths, and names the side as the toast does:
"pstack could not be installed on this computer: …". A failure stands
until the plugin is installed or turned off. It is tried again at the next
write of a plugin key and at the side's next start, which reads it from
`setup-installs.json` in Ferrite's settings folder; never in a loop. The
write's wish to install is kept in that file too, until a run for it
ends, so a sync cut short before its last scan, a write one Ferrite
process made, or a run its process did not outlive, as `ferrite setup
sync`'s, is installed by the next scan of that side in any process. The hub does the same for
itself after a pull. There is no new protocol op.
Nothing is ever uninstalled: a plugin turned off is a key set false, which
the tool reads itself. The runner is core's. What runs `claude` and
`codex` is the engine's (`providers::plugin_installs`), which finds each
the way a Session does, and each process that syncs a home hands it in:
the window, `ferrite setup sync` and the Server.

## One sync at a time

A home has one writer at a time, by any process: a file lock,
`setup-sync.lock`, in that side's Ferrite settings folder. A sync holds
both its homes from its first scan to its base, whichever computer drives
it: its own, and the Machine's by the tree op `hold`, which it asks before
it scans there. Syncs to two Machines ran side by side before: one scanned
the hub, the other took its Machine's edit to the hub, and the first then
wrote its own over it, unsaid. A computer that is a Machine to one hub and
a hub to its own Machine did the same: the Mac scanned Windows, Windows
took Linux's edit of CLAUDE.md, the Mac's write went over it, and Windows'
next sync carried the Mac's copy on to Linux.

Now a sync that finds either home held does nothing and is busy: nothing
was written on either side. The window asks it again 3 s later and up to
3 s more by chance, so the syncs queue, and two computers that are each
other's hub, each holding its own home as it asks for the other's, do not
meet again each time they ask. An install holds its home while it runs. A
sync's writes, on either side, are under its hold and take nothing more,
so none waits: a Machine's write that waited for that Machine's own
install, past the 30 s a hub waits for a reply, used to land after the hub
had given up.

The Machine's Server holds its home for the connection that asked, which
is the hub's for that sync alone. It lets it go as the connection goes: at
the sync's end, or when the hub crashes or its line drops. It lets it go
too when the connection asks nothing for 90 s (`tree::HOLD_QUIET`), as a
laptop hub asleep mid-sync does, so the Machine's own syncs and installs
wait no longer. A hub mid-sync asks again well within the 30 s it waits for
each reply, and each chunk of a long write is an ask. An unfinished write
is still kept for 10 minutes (`chunked::EXPIRY`). Every other Setup op of a
connection that does not hold the home is refused. One whose hold lapsed
reads "<Machine> stopped waiting for this computer; the next sync starts
over": what it asks was planned from a scan the home may no longer match.

## The table's version

`setup_sync::TABLE` is 4, and the Setup root carries it:
`{"kind":"setup_table","table":4}`. Table 2 was this ADR's first cut,
never released: it carried every key of `settings.json` and removed on a
Machine with no trash, so a Server by it must not take this hub's writes.
Table 3 carried each side's sign-in: a hub by it pushes its own to a
Machine that has none, and a Machine by it lists its own for a hub by this
table to pull, or to remove where a base names it, so the two must not
sync. A Server refuses any other table. One by a
newer table is refused as `unsupported`, which the hub's row reads as
"Update Ferrite on <name> to sync your setup". One by an older table, or
ADR 0048's `{"kind":"setup"}`, is refused with "Update Ferrite on this
computer to sync your setup: the Machine's Ferrite is newer". A Server from
before this ADR reads `setup_table` as a root it does not know and refuses
it as `unsupported`, so a new hub never syncs with it. Its row says to
update it, and nothing moves either way.

The hold is checked by its op, and the table stayed 3. A Server from before
the hold reads `hold` as an op it does not know and refuses it as
`unsupported`, so the row says "Update Ferrite on <name> to sync your
setup". A hub from before it scans without holding, and is refused with
"Update Ferrite on this computer to sync your setup: the Machine's Ferrite
is newer". Nothing moves either way, and every base stands: a new table
would have made the next sync with each Machine a first sync, which
removes nothing and gives each Whole key the hub's value.

A Machine's `ferrited` is updated by its setup. Settings › Connections runs
`machine/bootstrap.sh` there, which puts this app's version at
`~/.ferrite/runtime/versions/<version>/ferrited`. The bridge runs that
binary when it is there, else `ferrited` on the PATH. A Server already
running keeps its version until it restarts. The setup says so when the two
differ.

## Old bases

A base is stamped with the table that wrote it: `{"table": 4, "agreed":
<manifest>}`. A base with another table's number, or none, as every base
before this one, is not read at all, and the sync is a first sync: the
union of both sides, which removes nothing and, by the text hash, moves no
file that differs only in line ends. An old base is never read in part.
Its entries were agreed by other rules (the first table hashed bytes, not
text, and followed links), and a removal planned from one could take what
this table never synced: a base by table 3 names each side's sign-ins.

## Kept copies and trash

Nothing a sync removes or writes over, on either side, is deleted. Each
side's Server moves it to that side's own trash first, in its Ferrite
settings folder: `~/.ferrite/sync-trash/<stamp>/…`, a folder for each
sync, each kept 14 days. A removed file or folder, or a link, is moved
there. A file a write replaces is copied there first. A key removed or
written over is kept there as a file of its value.

A conflict's losing copy and the trash are kept under the path with its
first dot dropped, as `sync-conflicts/<stamp>/claude/CLAUDE.md` and
`sync-trash/<stamp>/agents/skills/x`, so Finder and Explorer show them. A
kept key is a file of its value, its name the key's path, with what
Windows refuses spelled `_`.

## Tradeoffs

- **Each computer is signed in on its own.** A new Machine has no Claude or
  Codex sign-in until the operator signs in there: no sync gives it one.
- **Table 4 starts each Machine over.** A base by table 3 is read as none,
  so the first sync once both sides run table 4 is a first sync: it
  removes nothing, and a Whole key the two hold differently takes the
  hub's value, the Machine's kept in its trash and said in the toast.
- **A key's time is its file's.** The newer of two changed keys is the one
  whose file changed later, which may be for another key.
- **Own values never move.** An absolute path outside the home is assumed
  to be this computer's only. A hook the operator keeps at the same path on
  every computer is held on each, unsaid, and set by hand.
- **Strict travel rules hold back some of the operator's own.** A
  marketplace held inline in `settings.json` (a `settings` source) and its
  plugins stay on the side that has them. So does an MCP server run from a
  folder of the operator's in the home that the table does not sync,
  `~/code/mcp/run` for one. Each is set by hand on another side.
- **A Whole key at the first sync loses the Machine's value.** It is kept
  in the Machine's trash and said once in the toast. The other choice, the
  newer, made the first run's thousands of conflicts.
- **The trash holds every file a sync writes over**, not only what it
  removes, for 14 days. A file edited often on one side leaves a copy for
  each sync on the other.
- **The installers are the tools' CLIs.** They need the network and
  `claude` or `codex` on the side's PATH. A side without one says so on the
  row. A command that hangs is stopped after 120 s, and a sync of that
  home is busy until then. Claude's `marketplace add` writes the whole settings
  file from what it read as it began. No sync writes that file meanwhile,
  but a key the operator changes on that side in those seconds can be
  lost, as when they run it themselves.
- **Syncs queue.** With two Machines, the second waits for the first, a
  few seconds more on each change, and a sync waits for an install under
  way in either of its homes. What an install found shows one sync later.
- **A hold is its connection's.** A hub whose line drops mid-sync starts
  again on a new connection, and so does one quiet for 90 s. A lease that
  outlived the connection would let it go on, at the cost of a token on
  every op, a clock the hub keeps renewing, and a Server-wide table of
  leases to expire.
- **A link out of the table is still followed.** A skills folder kept in a
  dotfiles checkout keeps syncing, at the cost of its contents travelling
  as files.
- **A file link travels as its file.** Only a link to a folder is made
  again as a link. A link to a file is followed, so the other side gets a
  copy, and an edit there is held as written through a link here.
- **A junction holds its folder's whole path.** Moving the home on
  Windows breaks each skill's junction, and a scan says so.
