---
status: accepted
date: 2026-11-24
---

# A directory tree moves to a Machine over the protocol

The operator picks which environment a new chat runs on: this computer or
one of their Machines. A Machine that lacks the Draft's Project gets a copy
of the Project folder, put where the operator says. Their agent setup
(skills, commands, agents, instructions, rules, plugins, settings and
sign-ins) follows them to every Machine and stays in sync both ways.

Three pieces do this, in three PRs. The first is built. The other two are
planned and recorded here so they build on it as designed:

1. **The tree primitive** (built): one way to make a directory on a
   Machine hold what one here holds, over the Ferrite protocol.
2. **The environment chip and Placement** (planned): the Draft's choice of
   environment, and the Project copy it starts.
3. **Setup sync** (planned): the operator's agent setup kept the same on
   this computer and each Machine.

"Machine" is the word in code and docs, "environment" in the window's words,
as Settings › Connections has it. `CONTEXT.md` defines **Placement** and
**Setup sync**.

## Shape: the Draft keeps one owner of its Machine

A Draft's Machine is its Project's Machine (ADR 0045: `DraftBinding.project`
is a `ProjectKey`, and the first send starts the Thread there). Two shapes
were weighed:

- **A** (chosen): the environment chip is a second way to choose the Draft's
  Project. Picking environment E swaps the Draft's Project for its
  counterpart on E, and makes the counterpart (copy and register) when E
  has none. `DraftBinding` is unchanged.
- **B**: `DraftBinding` gains a Machine field beside its Project. Rejected:
  the Draft's Machine would have two owners that can disagree (the field and
  `project.machine()`), and every reader would have to pick one.

Everything that moves files uses one primitive, so a Project copy and Setup
sync are two callers of the same code. It runs over the Ferrite protocol,
not scp or rsync, so it works over ssh, over a remote link, and from a
Windows window.

## 1. The tree primitive (built)

`ferrite_core::tree`:

- **A `Manifest`** is what a tree holds, by path: relative, `/`-separated,
  never absolute, never through `.` or `..` (`check_path`). Each `Entry` is
  a `Dir`, a `File { size, sha256, executable }` or a `Symlink { target }`.
  `modes` says whether that side keeps an executable bit. Windows keeps
  none, so the bit is a difference only between two sides that both keep
  one: Unix to Windows and back does not rewrite every script on each run.
- **A `Root`** is what an op works on, and each side resolves it itself.
  `Root::Path` is a directory of the Server's computer, `~` its home. PR 3
  adds `Root::Setup` as a variant, not a second protocol.
- **The wire** is `Op::Tree { request }` answered `Done::Tree { reply }`,
  additive under ADR 0025. Every wire enum ends in `#[serde(other)]
  Unknown`, so an older Server answers what it does not know `unsupported`,
  and a newer root or request is refused as unsupported, never misread.
  Fixtures are frozen under `protocol/fixtures/gen1/*tree*`.

  | Request | Reply | Does |
  | --- | --- | --- |
  | `scan { root, skip }` | `scanned { manifest }` | A root not there yet holds nothing. |
  | `begin { root, path, size, executable }` | `begun { write }` | A temporary file beside `path`. |
  | `chunk { write, offset, data }` | `written { offset }` | Base64, at most 256 KiB, in order. |
  | `finish { write, sha256 }` | `done` | Verified, synced, renamed into place. |
  | `read { root, path, offset }` | `bytes { data, size }` | Up to a chunk of a file. |
  | `make_dir`, `link`, `remove { root, path, … }` | `done` | Each a no-op when already so. |

- **One chunked core.** `ferrite_core::chunked` is attachment upload's
  chunked, hashed write, taken out of `attachment_transfer`, which now uses
  it too: a temporary file of its own per write, each chunk at the offset
  the receiver reached, the SHA-256 checked before it is handed over, at
  most a few writes per connection, and a write nothing was heard of for 10
  minutes dropped with its file. Attachments link the result into their
  store; a tree renames it into place.
- **`plan(source, dest) -> Vec<Step>` is pure.** First what must go, the
  outermost entry of each removal only: what only `dest` holds, and an entry
  whose kind or link target differs. Then what is made, in path order, so a
  directory is made before what it holds. A file that differs is rewritten
  in place.
- **`send` takes the steps through an `Exchange`**, one request at a time,
  reading each file from the source tree, and reports `Progress` and stops
  on its `Cancellation`. An error names the path it was on. `mirror` is
  `scan` here, `scan` there, `plan`, `send`.
- **A rerun sends only what still differs.** The steps come from the two
  manifests alone, never from a record of an earlier run, so a send cut off
  anywhere is finished by the next one. A file is only ever whole: a
  half-sent one is a temporary file the Server drops when its connection
  closes or the write expires.
- **A scan reads only what moved.** It hashes a file only when its size or
  modification time changed since this process last listed that root. A
  file modified within 2 seconds of the scan is hashed every time, since
  it may change again within one tick of the clock unseen. Symlinks are
  listed, never followed. What is none of a file, a directory or a symlink
  is left out.
- **Only a Server answers**, as `list_dir`. It runs each op off the pump
  (`read_off_pump`) with that connection's writes, at most 16 connections at
  once, and drops them when the connection ends. While a read is out it
  waits 1 ms for the next message instead of a frame (16 ms) or 250 ms idle,
  since a transfer asks one request after another.
- **The Server keeps a tree inside its root.** A path through a symlink or
  a non-directory is refused, as are `..`, an absolute path, and a root that
  is not absolute. No root may be the filesystem's root, the home directory
  or an ancestor of it: a mirror would empty it. A Windows Server makes no
  symlinks: it refuses `link` as unsupported.
- **The window** reaches it as `Cockpit::tree(machine)` beside
  `upload_attachment(machine, …)`. It answers a `tree::Connect`, which opens
  a connection of its own to that Server when run, off the window's thread,
  so a long copy holds up neither the window nor its acts. Attachments
  share the same side connection.
- **The CLI** proves and scripts it with no window:
  `ferrite [--machine <name>] tree send <local-dir> <remote-dir> [--skip
  <path>]…` (ADR 0027's exit codes and JSON lines). It prints what it did
  (`{"removed":0,"made":46,"written":693,"linked":0,"bytes":7132898}`) and
  a refusal as `{"error":{"code","message"}}` on stderr. PowerShell turns
  a leading `~` in an argument into its own home, so from PowerShell a
  Machine's path is given whole. A tree moves over
  the window's protocol, not the API, so on a machine it goes through the
  window's bridge (`ferrited bridge`), not `api-bridge`. `--skip` paths are
  left as each side has them.
- **Live proofs** use the local WSL distro as a Machine:
  `scripts/wsl-machine.ps1` builds `ferrited` there, runs an unprivileged
  sshd under a temporary HOME, and gives Windows an ssh shim with its own
  config, so the operator's keys, ssh config and Machines are never used.

## 2. The environment chip and Placement (planned)

- **A counterpart is computed, not stored.** The counterpart of Project P
  on Machine E is the Project there with the same `workspace::repository_key`
  (as delegation matches, ADR 0047; a copied folder carries its `.git`, so
  it matches itself). Only a Project with no repository key needs a record:
  a **Placement** `{ local_root, remote_root }` per Machine at
  `machine::dir(..)/placements.json`. One lookup answers "counterpart of P
  on E", for the chip, and nothing else asks.
- **Each Machine has a projects folder**, `projects_dir` on `Machine`
  (default `~/projects`), edited on its Connections row.
- **The chip** replaces the machine word at the front of the draft band
  (`pane::env_machine`) whenever more than one environment is on. It names
  the Draft's environment and lists this computer and each enabled Machine,
  a check on the Draft's, then "Add environment…" (Settings › Connections).
  It is the Project chip's band chip and popup, pressed through the shared
  control (ADR 0044). The setup sheet and its state are a view of their own
  (ADR 0041 to 0043).
- **Picking environment E** on a Draft whose Project is P:
  1. a counterpart exists: `choose_project(counterpart)`. The text,
     attachments (re-aimed at E), provider and effort stay;
  2. none, and P is on this computer: the setup sheet. A destination field
     prefilled `<projects_dir>/<name>` completes against E's `list_dir`; P's
     git-ignored entries are listed with their sizes, unchecked; Copy shows
     progress and can be cancelled. On finish: `register_project(E,
     destination)`, a Placement if P has no repository key, and
     `choose_project(new)`;
  3. none, and P is on another Machine: the Project card on E, since there
     is no folder here to copy.
- **What is there is never overwritten silently.** A destination that holds
  something offers "Use what is there" (register, no copy) or another path.
- **Windows to Linux**: after the copy, the source repo's effective
  `core.autocrlf` and `core.filemode=false` go into the copy's local config,
  so git there does not see the whole tree as modified.
- **A Machine not reached or not set up** is listed dimmed with its state
  and opens the Connections flow.
- **The Draft's "Add project…"** opens on the Draft's Machine, not always on
  this computer.
- **The sheet is one state machine**: `Choosing → Scanning →
  Copying { progress } → Registering → Done | Failed { reason }`.
- `CONTEXT.md`'s Draft entry and ADR 0045's "locked environment control"
  line change with it: the environment is a choice until the first send.

## 3. Setup sync (planned)

- **`Root::Setup`** joins the tree ops, and each side resolves the same
  table itself, relative to its home:

  | Paths | Kind |
  | --- | --- |
  | `.claude/{skills,commands,agents,rules,plugins}` | tree |
  | `.claude/{CLAUDE.md,keybindings.json}` | file |
  | `.claude/settings.json`, `.claude/plugins/{installed_plugins,known_marketplaces}.json` | portable JSON |
  | `.claude.json` | its `mcpServers` key only |
  | Claude's sign-in | credential: `.claude/.credentials.json` on Linux and Windows, the Keychain item `Claude Code-credentials` on macOS |
  | `$CODEX_HOME` (else `.codex`)`/{skills,prompts}` | tree |
  | `.codex/{AGENTS.md,config.toml,auth.json}` | file |

  Never transcripts, history, todos, logs or caches of session state.
- **Portable JSON**: a string that starts with this side's home is kept as
  `~/…` with `/` separators and written out in the receiver's. Hashes are of
  the portable form, so the same file on Windows and Linux compares equal.
- **Reconcile is pure and three-way**: `reconcile(base, here, there) ->
  Vec<Action>`, each `Push`, `Pull`, `RemoveHere`, `RemoveThere` or
  `Conflict`. The base is the manifest of the last good sync with that
  Machine, at `machine::dir(..)/setup-sync.json`. The first sync, with no
  base, is the union and removes nothing. A conflict (both sides changed) is
  won by the newer; the loser is kept under
  `~/.ferrite/sync-conflicts/<stamp>/` and a toast names it.
- **This computer is the hub**: each Machine syncs with it, so a change on
  one Machine reaches the others through it.
- **When**: as a Machine connects, before each bootstrap there, on a local
  change (debounced), and every 5 minutes while it is connected, for edits
  made there. One sync per Machine at a time.
- **Settings › Connections**: each Machine's row has "Sync my setup" (on by
  default) and a line saying in sync, syncing n files, or the last error.
- It works for a Machine with no agent setup yet.

## Tradeoffs

- **One request at a time.** A chunk waits for the one before it to be
  answered, so a transfer is bound by the round trip. From Windows to the
  local WSL distro over ssh, a 128 MiB file moved at 2.7 to 2.9 MiB/s (3
  runs): 90 ms a chunk, 64 ms of it the round trip of a 350 KB line and
  its reply through Windows' `ssh -C` alone. A Project's 693 files (7 MB)
  took 18.7 s, three requests a file at 8.8 ms each. To this computer's
  own Windows Server, with no ssh, the file moved at 15.3 MiB/s: a
  Windows wait of 1 ms sleeps a timer tick (about 16 ms). That is simple,
  resumable and enough for a Project; overlapping chunks or files is the
  lever if it is not.
- **Every file is synced to disk before it is renamed into place.** A
  power cut never leaves a torn file under a real name, at a cost per file
  on trees of many small ones.
- **No cancel request.** A cancelled send stops between requests and closes
  its connection, which drops the half-sent file there.
- **The hash cache is per process** and kept only for files settled 2
  seconds before the scan. A new process (each CLI run, a restarted Server)
  hashes every file once.
- **A tree is only as consistent as its source while it is read.** A file
  that changes while it is sent fails that file's hash or length check; a
  rerun sends it again.
