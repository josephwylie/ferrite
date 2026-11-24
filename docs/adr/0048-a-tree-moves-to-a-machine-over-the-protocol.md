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

Three pieces do this, in three PRs, all built:

1. **The tree primitive** (built): one way to make a directory on a
   Machine hold what one here holds, over the Ferrite protocol.
2. **The environment chip and Placement** (built): the Draft's choice of
   environment, and the Project copy it starts.
3. **Setup sync** (built): the operator's agent setup kept the same on
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
  a `Dir`, a `File { size, sha256, mode }` or a `Symlink { target }`.
  `mode` is the file's permission bits on a side that keeps them, `None`
  on Windows. Beside the entries are its `problems`: each path the side
  could not list (unreadable, or a name that is not UTF-8, named as near
  as it can be) and why.
- **A `Root`** is what an op works on, and each side resolves it itself.
  `Root::Path { path }` is a directory of the Server's computer, `~` its
  home. It is internally tagged (`{"kind":"path","path":…}`), so a kind a
  Server does not know reads as `Unknown` whatever fields it carries, and
  PR 3 adds `Root::Setup` as a variant, not a second protocol.
- **The wire** is `Op::Tree { request }` answered `Done::Tree { reply }`,
  additive under ADR 0025. Every wire enum ends in `#[serde(other)]
  Unknown`, so an older Server answers what it does not know `unsupported`,
  and a newer root or request is refused as unsupported, never misread.
  Fixtures are frozen under `protocol/fixtures/gen1/*tree*`.

  | Request | Reply | Does |
  | --- | --- | --- |
  | `scan { root, skip, after }` | `scanned { manifest, more }` | A page of the entries and problems after `after`, in path order. A root not there yet holds nothing. |
  | `begin { write, root, path, size, mode }` | `done` | A temporary file beside `path`, numbered `write` by the sender. |
  | `chunk { write, offset, data }` | `written { offset }` | Base64, at most 256 KiB, in order. |
  | `finish { write, sha256 }` | `done` | Verified and renamed into place; either way the write is over. |
  | `read { root, path, offset }` | `bytes { data, size }` | Up to a chunk of a file. |
  | `make_dir`, `link`, `remove { root, path, … }` | `done`, or `unchanged` when already so | |

  The sender numbers each write, never the same number twice in its
  process, so a file's `begin`, chunks and `finish` go out together, with
  no wait for the Server to name it. A `chunk` or `finish` of a write that
  failed, expired or never began is refused, so what was asked after a
  failure never puts part of a file in place; a `begin` of a number already
  under way is refused and drops that write.
- **A scan reply comes in pages** of at most 4 MiB of JSON. A tree with a
  `node_modules` lists about 190,000 entries, over the 32 MiB a connection
  may fall behind by before it is closed (`link::LAGGED_BYTES`). The Server
  keeps the connection's scan until its last page is asked, so the pages
  are of one scan. `mirror` asks for each in turn.
- **One chunked core.** `ferrite_core::chunked` is attachment upload's
  chunked, hashed write, taken out of `attachment_transfer`, which now uses
  it too. The sender reads a file as `Pieces`, whose hash is had only once
  the file ended at its size: one that grew or shrank as it was read
  fails. The receiver keeps a temporary file per write number, made with
  the bits the file will have, each chunk at the offset it reached, the
  SHA-256 checked before it is handed over, at most a few writes per
  connection, and a write nothing was heard of for 10 minutes dropped with
  its file and refused any later chunk. Attachments number their uploads
  on the Server, as their wire always has, and sync the result to disk
  before they link it into their store; a tree renames it into place
  unsynced (see Tradeoffs).
- **`plan(source, dest, skip) -> Vec<Step>` is pure.** First what must go,
  the outermost entry of each removal only: what only `dest` holds, and an
  entry whose kind or link target differs. Then what is made, in path
  order, so a directory is made before what it holds. A file that differs
  is rewritten in place, its `Step::Write` carrying the size, SHA-256 and
  bits the scan listed. What `skip` names and what either side could not
  list are held: left as each side has them, with all under them, and so is
  each directory that holds one, so `--skip keep/inner.txt` never removes
  `keep`. An entry of a kind this side does not know is left too.
- **Permission bits.** A file already there keeps its own bits when it is
  rewritten, so a 0600 file stays 0600. Its executable bits follow the
  sender's where both sides keep bits, and only then does a different
  executable bit make a file differ; a script edited on Windows keeps its
  `+x` there. A new file takes the sender's bits, or the receiver's umask
  from a side that keeps none. A directory a tree op makes, the root and
  those on the way to it too, is 0755 whatever the Server's umask (077,
  for its own state): a manifest lists no directory's bits to send.
- **`send` takes the steps through an `Exchange`, asking ahead of the
  replies**: a request is written at once (`ask`) and its reply read later
  (`answer`), in the order asked, with at most 64 requests or 4 MiB of
  files out at once. It reads each file from the source tree, reports
  `Progress` as the far side acknowledges bytes, and stops on its
  `Cancellation`. A file that cannot be read here as its scan listed it
  (gone, held by another program, or its bytes no longer hashing to what
  was listed: "it changed after it was listed") is left as the far side
  has it, a `Side::Here` problem of the send, and the send goes on. Its
  `finish` carries the listed hash, so the Server never puts a torn file in
  place. Every write begun is finished, however its send ends, so a failed
  send never holds a write slot of its connection. A send that stops
  returns `Stopped { error, sent }`: the error names the entry whose
  request it answered, and `sent` is what was taken there once the replies
  already on their way are heard, unless the line itself is down. What was
  asked after it may still be taken there, since every step is whole on its
  own, and an `answer` passes over replies nobody waited for, so the
  connection carries on. `mirror` asks for the scan there, scans here while
  it runs, then `plan`, `send`, and returns what it did with both sides'
  problems.
- **A rerun sends only what still differs.** The steps come from the two
  manifests alone, never from a record of an earlier run, so a send cut off
  anywhere is finished by the next one, however much it had asked ahead. A
  file is only ever whole: a half-sent one is a temporary file the Server
  drops when its connection closes or the write expires.
- **A scan reads only what moved.** It hashes a file only when what its
  metadata says moved since this process last listed that root: its size
  and modification time, and on Unix its change time and inode, which
  `cp -p`, `tar` and npm's fixed times do not hide. Windows gives its
  creation time instead. A file that moved within 2 seconds of the scan is
  hashed every time, since it may change again within one tick of the clock
  unseen. At most 200,000 files' hashes are kept, the roots listed longest
  ago dropped first. Symlinks are listed, never followed. An entry that
  goes while it is listed is left out; one that cannot be read is a
  problem, and every error names its path, the root's included. What is
  none of a file, a directory or a symlink is left out.
- **A write's temporary file** is `.ferrite-<process>-<micros>-<nonce>.part`,
  each a number, so a user's `.ferrite-notes.part` is an ordinary file and
  is sent. A scan leaves temporary files out, so a write under way there is
  neither copied nor removed. One nothing wrote to for 10 minutes belongs
  to no write, in this process or one that stopped mid-write, since none
  takes a chunk after that: the Server's scan of a tree removes it.
- **Only a Server answers**, as `list_dir`. Each connection's tree ops run
  in the order asked on a thread of their own (a `Lane`), at most 16
  connections at once: a read off the pump takes a thread per request, and
  those race, so they could not keep a write's chunks in order. Each
  answer, the lane's and every read off the pump's, is heard on the pump's
  own channel (`Heard::Answered`) and replied to at once, so a reply never
  waits for the pump's next tick (a frame, 16 ms, or 250 ms idle). A
  connection that ends takes with it what it asked and its lane had not
  begun, and the files it was writing. The pump's own sender keeps that
  channel open, so the accept loop says when it stops (`Heard::Stopped`)
  and the Server ends with status 1.
- **The Server keeps a tree inside its root.** A root is resolved once:
  its deepest ancestor that is there made canonical, links followed, and on
  Windows in its true case without `\\?\`. Every op works on that. A root
  is refused when it is the filesystem's root, the home directory or an
  ancestor of it, compared without case on Windows and macOS and, on Unix,
  also by device and inode, which a bind mount or a macOS firmlink spells
  twice: a mirror would empty it. Inside the root, a path through a
  symlink or a non-directory is refused, as are `..` and an absolute path.
  A Windows Server makes no symlinks: it refuses `link` as unsupported.
- **The window** reaches it as `Cockpit::tree(machine)` beside
  `upload_attachment(machine, …)`. It answers a `tree::Connect`, which opens
  a connection of its own to that Server when run, off the window's thread,
  so a long copy holds up neither the window nor its acts. Attachments open
  their side connection with the same code.
- **The CLI** proves and scripts it with no window:
  `ferrite [--machine <name>] tree send <local-dir> <remote-dir> [--skip
  <path>]…` (ADR 0027's exit codes and JSON lines). It prints what it did
  (`{"removed":0,"made":46,"written":693,"linked":0,"bytes":7132898}`, and
  `problems` when a side could not list something) and a refusal as
  `{"error":{"code","message"}}` on stderr. With no `--machine` it refuses
  a destination that is its source, inside it or around it. PowerShell
  turns a leading `~` in an argument into its own home, so from PowerShell
  a Machine's path is given whole. A tree moves over the window's
  protocol, not the API, so on a machine it goes through the window's
  bridge (`ferrited bridge`), not `api-bridge`. `--skip` paths are left as
  each side has them.
- **Live proofs** use the local WSL distro as a Machine:
  `scripts/wsl-machine.ps1` builds `ferrited` there, runs an unprivileged
  sshd under a temporary HOME, and gives Windows an ssh shim with its own
  config, so the operator's keys, ssh config and Machines are never used.

## 2. The environment chip and Placement (built)

- **A counterpart is computed, not stored.** The counterpart of Project P
  on Machine E is the Project there with the same `workspace::repository_key`
  (as delegation matches, ADR 0047; a copied folder carries its `.git`, so
  it matches itself), else the one a **Placement** names. Only a Project
  with no repository key needs that record: `{ local_root, remote_root }`
  per Machine at `machine::dir(..)/placements.json`, read both ways (this
  computer to E and back). `placement::counterpart` is the one lookup. The
  window learns each machine's repositories by asking its Server
  (`Op::Repositories`, additive under ADR 0025; an older Server answers
  none), so a pick waits for both answers before it moves the Draft.
- **Each Machine has a projects folder**, `projects_dir` on `Machine`
  (default `~/projects`, not written while it is the default), changed on
  its Connections row: its ⋯ menu's "Projects folder…" opens a field in the
  row, saved through Connections' own save.
- **The chip** stands at the front of the draft band in place of the
  machine word (`pane::env_machine`) whenever more than one environment is
  on, the laptop or the server mark and the environment's name ("Local"
  for this computer). Its menu lists this computer and each enabled
  Machine, a check on the Draft's, then "Add environment…" (Settings ›
  Connections). It is the Project chip's band chip and popup, pressed
  through the shared control (ADR 0044), and Tab reaches it between the
  model chip and the Project chip (`BandChip::Environment`). With one
  environment on, the band keeps the machine word.
- **Picking environment E** on a Draft whose Project is P
  (`draft_environment.rs`):
  1. a counterpart exists: the Draft's Project becomes it. Its text,
     attachments, provider and effort stay; its attachments are aimed at E
     and their upload there starts at once;
  2. none, and P is on this computer: the setup sheet (`PlacementView`).
     Where the copy goes is chosen by browsing E, never by typing a path
     (though one can be typed): see **The folder browser** below. The copy
     goes to `<folder browsed>/<name>`, its name P's folder name until
     edited ("Copies to"). P's git-ignored entries are listed with their
     sizes, unticked; Copy runs on a thread of its own over a connection
     of its own (`Cockpit::tree`), with its progress and Cancel. Then the
     window registers the destination on E, names a Project made there as
     P is named, so both read as one Project (a Project already there
     keeps its name), keeps a Placement if P has no repository key, and
     chooses the new Project on the Draft;
  3. none, and P is on another Machine: the Project card on E (a folder
     dialog when E is this computer), which chooses what it makes on the
     Draft.
- **The folder browser** (`FolderBrowser`, a view of its own) shows E's
  folders as a file explorer would, read by E's Server one directory at a
  time (`list_dir`, ADR 0026) on a thread of its own, so a slow listing
  holds up nothing: "Reading …" stands in its rows until it lands. It
  opens at `projects_dir`, or at E's home when that cannot be read. A
  line names the folder shown; under it, `..` and that folder's folders,
  never its files, and a dot folder only once a dot is typed. A press goes
  into a row, the arrows walk them and ↵ goes into the one they rest on,
  so the sheet is finished with the mouse or with the arrows and ↵ alone.
  "New folder" names a folder in the one shown, made by the copy; one
  that cannot be read says so in place, and `..` leads out of it. The
  Project card on a Machine uses the same browser in place of its path
  field: "+ Add directory" under it, or ↵ once it has moved, adds the
  folder shown, and the browser goes back to the folder that one is in.
- **What is there is never overwritten.** Before it writes, the copy reads
  the destination as the mirror would (`placement::occupied`, through the
  same exchange). One that holds anything stops at **Occupied**: the sheet
  says so and offers "Use what is there" (register, no copy); another
  folder or name chooses afresh. Only a destination this sheet already copied into
  (a retry, or a copy cancelled part way) is mirrored again.
- **A destination is kept sane by the sheet**: `/` or `~/` first (or a
  drive, for a Server on Windows), not the home or a root itself, nothing
  through `..` (`placement::refuse_destination`).
- **A file that changes or cannot be read** during the copy is left by its
  name, sent once more, and if it still cannot be, listed when the copy is
  in: the sheet stays up on Done to list it.
- **Windows to Linux**: Windows keeps no permission bits, so the copy
  gives each file its own (`placement::give_modes`): 0755 for one the git
  index has as executable (100755), 0600 for one git ignores, this
  computer's own as a `.env` is, 0644 for the rest. From a side that keeps
  bits they go as they are. After the copy, the source repo's effective
  `core.autocrlf` and `core.filemode=false` go into the copy's local config
  (`placement::align_git`), as a Windows checkout has them, so git there
  reads the tree as unchanged.
- **A Machine not reached or not set up** is listed dimmed with why
  ("connecting", "sign in", "not set up", "offline"). A press on it opens
  Connections; the arrows pass over it, as over every dimmed row.
- **The Draft's "Add project…"** opens the card on the Draft's Machine.
- **The sheet is one state machine**: `Choosing → Scanning → Copying
  { progress } → Registering → Done { skipped } | Failed { reason }`, with
  `Occupied` after `Scanning` when the destination holds something.
  Cancel is `Failed { "Cancelled" }`, and Retry from any failure copies
  again. Closing the sheet cancels its copy. From Copy on, where the copy
  goes is held: the browser, the copy's name and the ticks take no press
  or key until it fails. How it stands is one line under "Copies to",
  there in every stage, so the card keeps one size; over a draft the card
  is centred in the room above its band and never reaches it.
- `CONTEXT.md`'s Draft entry and ADR 0045's line on the strip over the
  Composer say the environment is a choice until the first send.

## 3. Setup sync (built)

`ferrite_core::setup_sync`, run by the window (`cockpit/setup_sync.rs`).

- **`Root::Setup`** joins the tree ops, and each side resolves one table,
  `setup_sync::ITEMS`, against its own home. On Windows that is
  `USERPROFILE`, else `HOME`: Node's `os.homedir()` (Claude Code) and
  Codex read the profile folder, and `HOME` there is often Git Bash's own.
  Elsewhere it is `HOME`, else `USERPROFILE`. `.claude` is
  `$CLAUDE_CONFIG_DIR` and `.codex` is `$CODEX_HOME` when set. A path
  outside the table is refused by the Server. Nothing outside the table
  is written, and nothing outside it is read except what a link inside it
  points at. A missing item counts as empty, so a Machine with no agent
  setup yet syncs like any other.

  | Paths | Kind |
  | --- | --- |
  | `.claude/{skills,commands,agents,rules,hooks,output-styles,plugins}` | tree |
  | `.claude/plugins/marketplaces` | seed: each marketplace, `.git` and all, copied whole to a side without it, then left to each side's agent |
  | `.claude/{CLAUDE.md,keybindings.json,statusline.sh}` | file |
  | `.claude/settings.json`, `.claude/plugins/{installed_plugins,known_marketplaces}.json` | portable JSON |
  | `.claude.json` | its `mcpServers` key only, as portable JSON |
  | `.claude/.credentials.json` | sign-in: a file on Linux and Windows, the Keychain item `Claude Code-credentials` (with `-<8 hex>` for a `CLAUDE_CONFIG_DIR`) on macOS |
  | `.codex/{skills,prompts,rules}` | tree, except `skills/.system`, which each Codex writes for its own version |
  | `.codex/AGENTS.md` | file |
  | `.codex/config.toml` | file, owner-only when it is new |
  | `.codex/auth.json` | sign-in file |

  Not synced: transcripts (`projects`, `sessions`), history, todos, logs,
  caches, backups, IDE and shell state, the rest of `.claude.json`, `.git`
  at any depth outside a seeded marketplace, `.claude/plugins/store/*.lock`,
  and anything else the table does not name.
- **A Ferrite on a Store of its own** (`FERRITE_STORE`: a test, an fv run)
  keeps Setup sync off unless its home is not the account's real one and
  `.claude`, `.claude.json` and `.codex`, wherever `CLAUDE_CONFIG_DIR` and
  `CODEX_HOME` put them, all lie inside it. The row says why and draws the
  switch off, and the switch does nothing. fv gives each run a home of its
  own.
- **A symlinked skill is followed**: its contents are sent and the link is
  recorded. Nothing is ever written through a link on its own side. A
  change the other side made under it is held and named on the row, and
  in a conflict the linked side wins. A removal that lands on a link
  removes the link and never what it points at. An item behind a link to
  nothing, or one that does not read, is held and named: it is never read
  as removed.
- **Portable JSON**: the home anywhere in a string, in either separator or
  JSON's doubled `\\`, is carried as `<home>/…` with `/` separators and
  written out in the receiver's spelling. A hook's command line counts.
  Every other byte is kept, including a `~` the operator wrote. Hashes are
  of the carried form, so the same file on Windows and Linux compares
  equal. A Windows `C:/Users/me/…` comes back as `C:\Users\me\…`: the
  same path, spelled the Windows way. JSON that does not parse is held,
  never sent torn.
- **Sign-ins** are bytes only: never logged, and never named by content
  in an error, an outcome or a toast. A sign-in is parsed only to check
  that it reads whole; one read mid-write waits for the next sync, unsaid.
  On Unix the file is 0600 in a 0700 folder however it arrives. **A sync
  never removes a sign-in**: a side without one receives the other's, base
  or no base, so signing out on one side lasts until the next sync. To
  stay signed out there, the operator turns "Sync my setup" off, as its
  label's hover says. Both sides without one is nothing to do. A sign-in is
  never in a removal plan, so it never trips the valve, and each side's
  store refuses to remove one or move it to `sync-trash`; no copy of one is
  kept in `sync-conflicts`. A Keychain sign-in is never written to a file.
  With no agreed base, two that differ are held, and the row offers "Use
  this computer's" and "Use <name>'s". Nothing moves until one is pressed.
  With a base the newer wins, and no copy is kept.
- **Reconcile is pure and three-way**: `reconcile(base, here, there) ->
  Vec<Action>`, each `Push`, `Pull`, `RemoveHere`, `RemoveThere`,
  `Conflict { winner }` or `Hold`. The base is the manifest of the last
  good sync, at `machine::dir(..)/setup-sync.json`. The first sync, with
  no base, is the union and removes nothing. A conflict is won by the
  newer modification time, or by this computer on a tie. The loser is kept
  under `sync-conflicts/<stamp>/` in Ferrite's settings folder, 0600, once
  per content: a conflict that a cut-short sync left is not kept twice.
- **The valve**, per item: with a base, a sync that would remove more than
  half of an item's files on either side, or any of an item the other side
  lost whole, does nothing. The row says which item and how many files, and
  "Sync anyway" runs the sync that stopped again as it was asked (with the
  sign-ins if that one read them), whole and with removals allowed, so it
  clears any stop. A file item is an item, so removing `CLAUDE.md` asks
  too. What a sync removes on this computer is moved to
  `sync-trash/<stamp>/` in the settings folder, 0700, and pruned after 14
  days. A sync that did not read a path (one after a change here does not
  read Claude's sign-in) leaves a stop or hold about it standing.
- **Idempotent**: each write goes through a temporary file that is synced
  to disk before it is renamed, and one left by a crash is swept. A pull
  writes as many files at once as a Session takes, however many it pulls.
  Once both sides are done, each is rescanned, and only then is the base
  written, from what the two now hold alike. A sync cut short anywhere
  leaves the old base, and the next one finishes it. A file that changes
  while it crosses, either way, is left out of the base and named, and the
  next sync carries it. A lock file keeps one sync per Machine at a time
  across processes: a sync that finds it held is busy, shows "Syncing…" and
  tries again 3 seconds later.
- **Folders are the owner's**: every folder Setup sync makes is 0700
  whatever the umask, and a sign-in's folder is made 0700 even if it was
  already there. A tree sent to a `Root::Path` keeps its own rule.
- **Names**: a Machine's name this computer cannot hold (a Windows device
  name, a character Windows refuses, or two names that differ only in case
  where case is not told apart) holds only that path, named on the row.
- **This computer is the hub**: each Machine syncs with it, so a change on
  one Machine reaches the others through it.
- **When**: everything, sign-ins with it, as a Machine connects and every
  60 seconds while it is connected, a clock that nothing else moves. 3
  seconds after the setup here last changed (a file watcher on the table's
  folders), it syncs what changed here and what changed there, but does
  not read the sign-ins. As a Thread starts there it syncs without waiting
  for it. A change here that leaves the table's bytes as they were never
  reaches the Machine.
- **Settings › Connections**: each Machine's row has "Sync my setup" (on
  by default, `sync_setup` on `Machine`), lined up with the environment's
  own switch, and a line: "In sync", "In sync except …", "Syncing n
  files", what stopped it, why it is off, or the last error. The line names
  the environment ("gone from wsl", "on wsl"), and an error says the path
  from `~/`, the side by name and the reason in plain words. A Machine
  whose Ferrite predates `Root::Setup` says to update it. When it is off,
  nothing syncs.
- **One toast** for all of Setup sync: it folds each Machine's conflicts,
  stops and failure into one, and a new one never hides the last. A failure
  raises it once until a sync succeeds. "Open folder" opens where the
  losing copies are kept, named as Ferrite's `sync-conflicts` folder, and
  sits under the words. A folded toast ends with one line saying where to
  act. Toasts draw over the Settings sheet, at their own height and short
  of its close button, which stays in reach.
- **The CLI**: `ferrite --machine <name> setup sync [--allow-removals]`
  runs one sync and prints its outcome as JSON.
- **macOS's Keychain** is reached through the `keyring` crate behind a
  `Keychain` trait, whose errors are replaced by fixed words. It belongs
  to the account, not to `HOME`, so it holds the sign-in only when `HOME`
  is the account's own home directory. Another `HOME` (a test, an
  isolated run) keeps it as a file and never reaches the operator's
  Keychain. It is read only as a Machine connects and on the 60 second
  clock, since each read may ask the operator. The tests use a fake one
  that counts reads. A live run on the operator's Mac is still needed.

## Tradeoffs

- **Asking ahead, bound by ssh.** With up to 64 requests or 4 MiB out at
  once, the round trip no longer sets the pace. Measured from Windows to
  the local WSL distro over ssh, 3 runs each, one request at a time before
  and asking ahead after:

  | Tree | Before | After |
  | --- | --- | --- |
  | `crates/ferrite-core`: 706 files, 46 directories, 7.1 MB | 25.5 to 31.6 s | 1.11 to 1.12 s |
  | One 128 MiB file | 2.41 to 2.45 MiB/s | 5.56 to 5.62 MiB/s |
  | A clone of this repo with its `.git`: 2,849 files, 326 directories, 449 MiB | not run | 85 to 103 s |

  To this computer's own Windows Server, with no ssh, the crate took 34.2
  to 34.9 s before and 0.66 to 0.89 s after, and the file moved at 15 MiB/s
  before and 57 to 58 MiB/s after. Before, each reply waited for a Windows
  timer tick (about 16 ms); the pump now hears each answer as it is made.
  Over ssh the limit is Windows' `ssh -C`, a whole core during each run,
  so a large file moves near 5.6 MiB/s however far ahead it asks. Fed the
  same base64 chunk lines with nothing else, it carries 5.5 MiB/s of file
  data, and 15.3 MiB/s without `-C`. Every Ferrite ssh is `-C`, and
  on Unix one master carries all of a Machine's connections, so a tree
  cannot drop it alone. Raw bytes on the wire, or no `-C`, are the levers
  if this is not enough.
- **What was asked after an error may still be taken.** The Server runs
  each request in order and the sender stops at the first error, so steps
  after it may land there too. Each is whole on its own: the tree is only
  further along, `Stopped.sent` counts them, and a rerun finishes it.
- **A tree's files are not synced to disk before they are renamed into
  place.** A sync per file took 0.66 to 4.1 s per 706 small files on WSL's
  ext4 (median 0.94 ms, up to 158 ms each), most of a crate's send. A
  power cut may leave a file there short or empty under its name; its hash
  then differs, and the next send rewrites it. Neither rsync nor cp syncs
  by default. An attachment is still synced: its store is named by hash,
  and nothing would rewrite a torn one. That holds for a mirror, which the
  next send repairs, and not for Setup sync: a torn `settings.json` or
  sign-in is what an agent reads next, so `Root::Setup` writes sync before
  they rename.
- **No cancel request.** A cancelled send stops between requests and closes
  its connection, which drops the half-sent file there.
- **The hash cache is per process** and kept only for files settled 2
  seconds before the scan. A new process (each CLI run, a restarted Server)
  hashes every file once. Windows gives no change time, so there a file
  rewritten in place at its old size with its old modification time put
  back (`touch -r`, a restore) keeps its old hash until the process ends.
- **A tree is only as consistent as its source while it is read.** A file
  that changes after it is listed, or while it is read (a live log, a git
  index), is left out of that send by its name; the setup sheet sends what
  was left once more and lists what still is.
- **A Windows Server makes no symlinks**, so a tree that holds one never
  completes there: each send fails at its first link, though what it asked
  ahead lands. Making one there needs Developer Mode or an elevated
  process, and Ferrite's Machines are Unix.
- **Names that differ only in case never converge** on a destination that
  ignores case (Windows, a default macOS volume): both are written to one
  file, and every send writes it again. A git checkout there has the same
  trouble.
- **Each tree answer wakes the Server's loop for a whole pass**, its
  broadcast included, as any message does; the loop takes up to 64 heard
  messages before it runs one. The crate's 706 files still moved in 1.1 s,
  so it is not the limit; a lane that replied to its client itself would
  skip the pass if it becomes one.
- **The Server ends when its listener stops.** It used to learn that from
  its channel closing, which never happened while a sender it holds lived
  (the API's tools on `main`, each lane and read here), so a Server whose
  accept failed ran on with no way in. The accept loop now says so
  (`Heard::Stopped`). No test makes an accept fail.
- **Only the filesystem's root, the home directory and its ancestors are
  refused.** `~/.ssh`, `/etc` or a Project's own folder is a root like any
  other, and a mirror there removes what the source does not hold. Callers
  keep destinations sane: the setup sheet's is a new folder under
  `projects_dir`, and Setup sync's are its table's.
- **Setup sync writes `.claude.json` while Claude Code may.** Only its
  `mcpServers` key is replaced, through a temporary file and a rename. A
  Claude Code that rewrites the file from what it read before the rename
  puts the old key back, and the next sync sees that as an edit there.
- **An MCP conflict goes to whichever `.claude.json` was written last.**
  Claude Code rewrites the file on nearly every run (counters, caches), so
  its modification time says whose Claude ran last, not whose
  `mcpServers` edit is newer. The other copy is kept.
- **A sign-in refreshed on both sides goes to the newer.** A provider that
  rotates refresh tokens retires the old one at its first use. So when both
  sides refresh from one token, one of them fails and may ask to sign in
  again, until a sync carries the winner across.
- **A conflict's losing file is kept, but a sign-in's is not**: only the
  newer sign-in is of use.
- **Signing out does not last while Setup sync is on.** The other side's
  sign-in comes back at the next sync; the switch is how to stay out.
- **"Sync anyway" runs whatever removals exist when it is pressed**, not
  only the ones the row named, and what it removes on the Machine has no
  trash: `sync-trash` is this computer's.
- **Many small deletions can empty an item** down to its last file before
  the valve stops one: each removes under half of what is left.
- **A marketplace removed here is seeded back** from a side that still
  has it.
- **Writing a sign-in sets an existing `~/.claude` to 0700.**
- **Setup sync reads `USERPROFILE` first on Windows**, while the rest of
  Ferrite reads `HOME` before it (`workspace::expand_home`).
- **The first scan of a cold 20,000-file home takes over a minute**, with
  the row on "Syncing…" all the while.
- **A conflict whose toast never showed**, because the window was killed,
  is not announced later; its losing copy is still kept.
- **`.codex/config.toml` is a plain file.** Its `[projects."C:\\…"]` trust
  entries cross to Linux, and Linux ones come to Windows. Each side's Codex
  ignores the other's paths, but trusts nothing new. No home path in it is
  rewritten.
- **Following a link reads outside the table.** A skill linked to a
  checkout is read where it points, so what it points at must be meant to
  sync.
- **A local write that fails stops the sync** before the base is written:
  a full disk or a refused folder here would fail every file the same way.
  The next sync, at most a minute later, tries again from the old base. A
  file the Machine would not read, or one that changed while it crossed, is
  left and named instead.
- **The valve counts files only**: removing an empty folder never trips it.
- **The 60 second check scans both sides whole.** On 22,468 files (a
  plugin cache of 22,000), with each side's hashes cached, a check that
  finds nothing to do took 0.82 to 1.28 s in one process on Windows; it
  rescans neither side and leaves the base as it was. Over ssh to the WSL
  Machine, `ferrite --machine wsl setup sync` took 1.46 to 1.50 s, a new
  process each time, so every file here was hashed again. A sync after a
  change here that changed nothing reads only this side: 0.32 to 0.34 s.
