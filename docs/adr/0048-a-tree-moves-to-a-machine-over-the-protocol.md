---
status: accepted
date: 2026-11-24
---

# A directory tree moves to a Machine over the protocol

The operator picks which environment a new chat runs on: this computer or
one of their Machines. A Machine that lacks the Draft's Project gets a copy
of the Project folder, put where the operator says. Their agent setup
(skills, commands, agents, instructions, rules, plugins and settings)
follows them to every Machine and stays in sync both ways.

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
  | `begin`, `finish`, `make_dir`, `link` | `unmade { why }` | This entry cannot be made there: `leads_out`, `leads_nowhere`, `name`, or `failed { error }`. The send leaves it and goes on. |
  | `hold { root }` | `done`, or `busy` | The Setup root's home held for this connection's sync, until it goes or asks nothing for 90 s (ADR 0050). Every other Setup op needs it. |

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
  order, so a directory is made before what it holds. Links come last,
  each after the links it leads through, so a Windows Server finds what a
  link leads to already there. A link there is the one this side holds
  when its target is spelled the same but for `.` and empty steps, or,
  for a link to a file, when it is a file equal to that one, as a Windows
  Server may make it. A file that differs
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
  `Root::dir_mode` is the one place that says so.
- **`send` takes the steps through an `Exchange`, asking ahead of the
  replies**: a request is written at once (`ask`) and its reply read later
  (`answer`), in the order asked, with at most 64 requests or 4 MiB of
  files out at once. It reads each file through a `tree::Source`, a plain
  root or a Project's `placement::Checkout` (section 2), reports
  `Progress` as the far side acknowledges bytes, and stops on its
  `Cancellation`. A file that cannot be read here as its scan listed it
  (gone, held by another program, or its bytes no longer hashing to what
  was listed: "it changed after it was listed") is left as the far side
  has it, a `Side::Here` problem of the send, and the send goes on. Its
  `finish` carries the listed hash, so the Server never puts a torn file in
  place. An entry the far side answers `unmade` is left too, with all
  under it, and listed in `Sent.unmade` with why: no one entry stops a
  send. Every write begun is finished, however its send ends, so a failed
  send never holds a write slot of its connection. A send that stops
  returns `Stopped { error, sent }`: the error names the entry whose
  request it answered, and `sent` is what was taken there once the replies
  already on their way are heard, unless the line itself is down. A line
  that goes while they are heard is what the send stopped for, so a
  `Stopped` whose error is not the line's has heard every reply: a copy
  cancelled as its line went never reads as having landed nothing. What was
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
  ago dropped first. Symlinks are listed, never followed. On Windows a
  junction is listed as a symlink too, and a link to a place inside the
  root is listed from its own folder, `/`-separated, as a Unix side spells
  it, so a link a Windows Server made reads as the one it was sent. An
  entry that
  goes while it is listed is left out; one that cannot be read is a
  problem, and every error names its path, the root's included. What is
  none of a file, a directory or a symlink is left out.
- **A write's temporary file** is `.ferrite-<process>-<micros>-<nonce>.part`,
  each a number, so a user's `.ferrite-notes.part` is an ordinary file and
  is sent. A scan leaves temporary files out, so a write under way there is
  neither copied nor removed. One whose process is gone, or that nothing
  wrote to for 10 minutes, belongs to no write, since none takes a chunk
  after that: the Server's scan of a tree removes it, so the Resume after
  a lost connection leaves none of the old Server's behind.
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
  A Windows Server makes a link to a folder as a junction, which needs no
  privilege, and a link to a file as a file symlink when the process may
  make one, else as a copy of that file. It makes either only to a place
  inside the root that is there. A link that leads out of the root or to
  nothing, a name Windows cannot hold (`aux`, a trailing dot), and a
  write the filesystem refuses are answered `unmade`; only a root that
  cannot be resolved fails a request. `Root::Setup` makes no links.
- **The window** reaches it as `Cockpit::tree(machine)` beside
  `upload_attachment(machine, …)`. It answers a `tree::Connect`, which opens
  a connection of its own to that Server when run, off the window's thread,
  so a long copy holds up neither the window nor its acts. Attachments open
  their side connection with the same code.
- **The CLI** proves and scripts it with no window:
  `ferrite [--machine <name>] tree send <local-dir> <remote-dir> [--skip
  <path>]…` (ADR 0027's exit codes and JSON lines). It prints what it did
  (`{"removed":0,"made":46,"written":693,"linked":0,"bytes":7132898}`,
  `problems` when a side could not list something, and `unmade` when the
  far side could not make an entry) and a refusal as
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
  on Machine E is the Project there with the same `repository::repository_key`
  (as delegation matches, ADR 0047; a copied folder carries its `.git`, so
  it matches itself), else the one a **Placement** names. Every copy keeps
  that record, so where it went is known whatever git later says of either
  side: `{ local_root, remote_root }` per Machine at
  `machine::dir(..)/placements.json`, read both ways (this computer to E
  and back). A Machine set up again keeps its projects folder, and one
  whose target is edited takes what this computer keeps of it to its new
  folder (`placement::follow`). `placement::counterpart` is the one lookup. The
  window learns each machine's repositories by asking its Server
  (`Op::Repositories`, additive under ADR 0025; an older Server answers
  none), so a pick waits for both answers before it moves the Draft.
  `placement` is on the shared side (ADR 0049): it names no Engine module,
  so `counterpart` takes the Projects there by id and root, and the
  answer's type is the protocol's (`protocol::Repositories`).
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
  model chip and the Project chip (`BandChip::Environment`). One control
  of a Draft holds the ring at a time, a file or a band chip, and none
  once a sheet is up; Send sends whichever holds it. With one environment
  on, the band keeps the machine word.
- **Turning a Machine off puts its Drafts away**, and Connections says so
  ("Your draft on E is put away until E is on again."); turned on again,
  each comes back with its words.
- **Picking environment E** on a Draft whose Project is P
  (`draft_environment.rs`):
  1. a counterpart exists: the Draft's Project becomes it. Its text,
     attachments, provider and effort stay; its attachments are aimed at E
     and their upload there starts at once. A Draft keeps a file of this
     computer by its path here, and a Machine's copy of it beside it
     (`DraftContent.uploaded`), never in its place: put away and back on
     that Machine it sends the copy without uploading again, and picked
     onto this computer it sends the file here. One no longer here says
     "Gone from this computer" on either side and asks for no upload;
     put back, its Retry sends it;
  2. none, and P is on this computer: the copy card (`PlacementView`).
     It opens on one line, where the copy goes under `~/`, the folder
     muted and the name in ink, with "Change…" beside it, which opens
     **The folder browser** below in place. The browser opens by itself
     only when that is in doubt: the folder could not be read, so the
     browser went home, or the name is taken there by what the copy would
     not resume. Where the copy goes is chosen by browsing E, never by
     typing a path (though one can be typed). The copy goes to
     `<folder browsed>/<name>`, its name P's folder name until edited
     ("Copies to"). Under it, "Include files git ignores": one tick, none,
     some (a dash) or all, with how many and their size, and a press
     ticks all from none or some and none from all. Under that each entry
     has its own tick and size, unticked, scrolling inside the card; a
     folder git ignores whole is one row, nothing in it listed apart, and
     ticked it carries all it holds; past 20, the rest are one row, "and N
     more", that ticks them all. The button says how much goes: "Copy
     2.2 GB". Worktrees inside P
     are never copied nor listed, and the sheet says so; a P that is
     itself a linked worktree is refused, since its `.git` names a path
     here, and the sheet names the folder of its main checkout where that
     `.git` file leads to one. Copy
     runs on a thread of its own over a connection of its own
     (`Cockpit::tree`), as an entry of the window's copies (below). Then
     the window
     registers the destination on E, names a Project made there as P is
     named, so both read as one Project (one there before keeps its name,
     as the registry's answer says), keeps a Placement, and chooses the
     new Project on the Draft, while it is still an unsent Draft on P;
  3. none, and P is on another Machine: the Project card on E (a folder
     dialog when E is this computer), which chooses what it makes on the
     Draft.
- **The folder browser** (`FolderBrowser`, a view of its own) shows E's
  folders as a file explorer would, read by E's Server one directory at a
  time (`list_dir`, ADR 0026) on a thread of its own, so a slow listing
  holds up nothing: "Reading …" stands in its rows until it lands. It
  opens at `projects_dir`. One E does not have yet, as a fresh Machine
  lacks `~/projects`, is shown as the folder the copy makes when the
  folder it is in holds nothing of its name; else the browser opens at
  E's home. A line names the folder shown; under it, `..` and that
  folder's folders, never its files, and a dot folder only once a dot is
  typed. Only the rows in view are drawn, so a folder of 20,000 folders
  opens and scrolls as one of 20. A press goes into a row, the arrows walk
  them and ↵ goes into the one they rest on, so the sheet is finished with
  the mouse or with the arrows and ↵ alone. Tab finishes the line's last
  name to the first folder it starts, and with none to finish moves on.
  "New folder" names a folder in the one shown, which "The copy makes
  this folder" marks; one that cannot be read says so in place, in two
  lines at most, and `..` leads out of it. It reads its folder again once
  a copy ends or is cancelled. On the setup sheet the list is
  as tall as the folder's rows, up to eight, and more scroll, so a folder
  with almost nothing in it leaves no well under them. The
  Project card on a Machine uses the same browser in place of its path
  field: "+ Add directory" under it, or ↵ once it has moved, adds the
  folder shown, and the browser goes back to the folder that one is in.
- **What is there is never overwritten.** Before it writes, a copy reads
  the destination as the mirror would, through the same exchange. A first
  copy into one that holds anything sends nothing and stops at
  **Occupied**: the sheet says so and offers "Use what is there"
  (register, no copy); another folder or name chooses afresh.
- **An unfinished copy can be resumed.** Before its first byte a copy is
  kept on this computer as **unfinished**, at
  `machine::dir(..)/unfinished.json` beside the Placements, by P's root
  and the destination alone. It is cleared once the copy is added there
  (`placement::added_there`, by the window once its Placement is kept),
  or once it stops with nothing landed (`placement::copy`, one run at a
  time for a destination, so a run cancelled before anything landed
  never clears what the next run wrote). A copy whose files all landed
  but that was never added (its Machine went, the registration failed,
  or the window closed first) is still unfinished, so its Resume finds
  it and adds it. The Placements and the unfinished copies of a Machine
  are each read, changed and written back whole by one writer at a time
  in this process (`placement::keeping`), so a copy beginning while the
  window adds another to the same Machine never writes over it. A copy cancelled, cut off, or stopped with the app is resumed by the
  next copy of P to that destination, which reads what is done off the
  destination rather than a list: a file there as the copy sends it is
  done, one not there is sent, and one there that differs is someone
  else's, left as it is and listed under Not copied as "changed on E".
  A resume removes nothing. A destination that holds another repository
  by then (its `.git/config` names another remote) takes no resume, and
  its record goes. One that holds another Project's unfinished copy is
  that Project's: the sheet says "D holds an unfinished copy of Q." and
  offers nothing to use there. The sheet reads the records: for a P with
  one it opens on that destination, under `~/`, says "An unfinished copy
  of P is here." and offers Resume, never "Use what is there"; to start
  elsewhere, the operator browses away or makes a New folder. Removing a
  Machine in Settings forgets its Placements and unfinished copies.
- **A destination is kept sane by the sheet**: `/` or `~/` first (or a
  drive, for a Server on Windows), not the home or a root itself, nothing
  through `.` or `..` (`placement::refuse_destination`).
- **A file that changes or cannot be read** during the copy is left by its
  name, sent once more, and if it still cannot be, listed when the copy is
  in: the card's last face lists it under Not copied, "Left as it was",
  and with the card gone, the toast says how many ("1 item not copied",
  counts grouped in thousands as everywhere on the card: "and 2,981
  more") and its Show opens the card on them. Each is listed with why: "in use here", "unreadable on E" or "changed on
  E". An entry E cannot make is listed there too: "a link outside the
  Project, not made on E", "a link to nothing on E, not made", "a name E
  cannot hold", or "not made on E:" and the reason. More than three alike
  are one line, by the folder they share: "312 links in
  apps/web/node_modules lead outside the Project and were not made on
  E". A copy that leaves some is still in: the Project is registered and
  the Draft moves to it.
- **A copy sends what git would check out on the Machine**
  (`placement::Checkout`, the copy's `tree::Source`). From Windows, a file
  `git ls-files --eol` has as `i/lf w/crlf` goes with each CRLF made LF,
  uncommitted edits and all, when git itself converts it: its attribute
  is `text` or `text=auto`, or it has none and `core.autocrlf` is true.
  One that is `-text`, binary, `eol=crlf` or has a
  `working-tree-encoding` goes as it is; each
  file gets bits of its own, which Windows does not keep: 0755 for one the
  index has as 100755, 0600 for one git ignores (this computer's own, as a
  `.env` is), 0644 for the rest; the copy's `.git/index` lists each file
  sent as LF with no size, as git lists a racily clean file, so git there
  reads what it holds rather than the CRLF size it had here; and its
  `.git/config` goes without `core.autocrlf`, `core.filemode` and
  `core.ignorecase`, which say how this computer checks out. Everything
  else goes byte for byte, and the manifest's sizes and hashes are of what
  is sent. So git there reads the tree as unchanged and a script checked
  out here with CRLF runs there. From a side that keeps bits, everything
  goes as it is.
- **A Machine not reached or not set up** is listed dimmed with why
  ("connecting", "sign in", "not set up", "offline"). A press on it opens
  Connections; the arrows pass over it, as over every dimmed row.
- **The Draft's "Add project…"** opens the card on the Draft's Machine.
- **A copy is one entry of the window's copies** (`Copies`, by Project
  and the Machine as it is saved, `MachineKey`, one at a time for each:
  turned off and on, a Machine is held under another `MachineId` and its
  copies are still its own, each started, resumed and shown on it as the
  window holds it then), a state machine: `Listing { files } → Copying {
  sent, total, file } → Finishing → Done { took, not copied } | Failed {
  reason, again } | Stopping → Cancelled { again } | Failed`, with `Occupied` after `Listing`
  when a first copy finds the destination holding something, and `again`
  what its button does: Retry, Resume, or Add for one whose files all
  landed; none once its Machine was removed.
  The card, a toast and the Draft's environment chip are three views of
  it; none keeps a copy's state of its own. Copy again for a copy under
  way opens the card on it.
- **A copy is of all of P.** What an earlier run left there counts as
  sent from the first reading, so a Resume's bar starts where the last
  run stopped (`placement::copy` counts the bytes found there in each
  `Progress` it reports); its rate and time left are of what remains, and
  its result names the whole size and the time this run took. Once it
  sends, how far it is never goes back nor loses its percentage: the
  second pass that sends what could not be read lists P again, and the
  window hears that listing only while the copy still lists.
- **The card has three faces.** Choosing (above). Copying: the same card
  resizes to the compact width with the sheet's motion (`DIALOG_IN`),
  "Copying to E" over "P → ~/projects/app", a bar, "134 MB of 2.2 GB" with
  "11 MB/s · about 3 min left", and the file being sent on one muted line,
  cut in its middle. While the files are listed the bar runs with no end
  and the line counts them, "Listing files… 18,402". Copied: "Copied P to
  E", as its toast says, "P is at ~/projects/app on E", "2.2 GB in 3 min
  40 s", "This chat now runs on E." when the Draft moved, and Not copied,
  with Done. A stopped copy's card leads with how it stopped, in the ink
  of what is read (a failure's in red), over where it goes: "Copy
  cancelled at 42%. Resume finishes it.", and reads Close and Resume (or
  Retry). The card is as
  tall as what it shows, centred in the room above the band and never
  reaching it.
- **The rate and the time left** are one pure function of what was heard
  when (`copies::meter`, tested on a clock of its own): the rate is what
  was sent over the last five seconds, so a copy that stalls slows to
  nothing; the time left shows after three seconds of sending, said as a
  person says it ("less than a minute", "about 3 min", past ten minutes to
  the nearest five), and another shows once the one shown has been wrong
  for three seconds, so it does not flicker; one that would say too
  little by more than a step of that saying shows at once, so a copy that
  slows is never said to end much sooner than it will.
- **Only Cancel stops a copy.** ✕, Esc, a press outside the card and
  "Continue in background" let the card go and the copy go on. Cancel
  keeps what was sent for its Resume and says so once the copy's thread
  has ended: until it has heard the replies to what it had already asked
  (64 requests or 4 MiB at most), the copy is **Stopping**, still under
  way, and the card and its toast say "Cancelling… waiting for E to
  answer" with neither Cancel nor Resume, since a file asked for may
  still land and a Resume would follow a run that has not ended. A
  reply is waited for 30 s at most (`tree::PATIENCE`) and a line that
  goes ends the wait at once, so a Machine that answers nothing more
  leaves it stopping until 30 s pass with no reply, or its link drops
  (ssh gives up on a silent one after a minute). Then the Cancel is not
  known to have stopped it: a write it had asked for may still land, so
  the card does not say cancelled but "E stopped answering, so a file
  already sent may still land there." in a failure's ink, keeps the
  copy unfinished, and offers Resume. The copy's own connection gives
  its Server the same 30 s to welcome it (`link::Client::attach_within`),
  so a Cancel while it connects ends too, cancelled with Retry; the
  window's own connection waits as long as it takes. A
  copy is cancelled or
  done, never both (`Cancellation::end`): one that has asked all it sends
  and waits on the last replies stops as any other and stays unfinished,
  and a Cancel once all of it landed comes too late and changes nothing,
  so the copy is added. While the card is gone
  a toast that does not hide by itself says "Copying P to E", a thin bar,
  "38% · about 3 min left" and Show, which brings the card back; closing
  it does not stop the copy. Toasts standing together (two copies under
  way) each show whole, one above the other. **Where toasts stand:** the
  kit's toasts stand over the Composer of the board's lower right Pane,
  their right edge on its box's and their foot just over its stack. They
  rise only when they would cover what the operator answers at the
  transcript's foot, a Decision's card or a failed turn's Retry, in view
  and with no sheet up: then their foot stands just over it, never over
  the Pane's head unless only that keeps Allow and Deny clear. With no
  Composer there, they stand in the board's lower right corner
  (`toast_home`). A sheet holds the keys and the pointer, so the row
  under it waits and the toasts stay home rather than rise into its
  card. So they read
  as anchored in every layout (a draft, a Thread, the panel or the
  Details card open, a sheet up), and never cover an answer the operator
  owes. They draw over any sheet and under every menu (ADR 0043's
  ladder), edged as every float is (`FLOAT_EDGE`), so one over a sheet's
  card reads as its own card. Since the board's menus (a message's, a
  Decision's, a citation's comment, the Composer's) draw over sheets too,
  a sheet or the palette coming up closes them first: no menu outlives
  the surface it belongs to. The Composer's menu does not open again
  while a sheet, the palette or the shortcuts are up, whatever moves its
  line or lists its commands meanwhile; the first edit once they go opens
  it. A citation's comment closed that way keeps its words on the reply,
  even over the limit, since a sheet can come up unasked (a copy's
  notification clicked); words over the limit hold the send back, and a
  send held back opens the comment to say so. Leaving the app while a copy runs asks first ("Copying P to
  E. Leaving stops it; Resume finishes it later.").
- **A copy is always a pick away.** Every Draft's environment chip lists
  each copy the window keeps under a "Copies" heading of its own, after
  its environments and "Add environment…", the Draft's own
  Project first: "Copying P to E" and how far, "Copied P to E" and done,
  or "Copy of P to E" and stopped; a pick opens its card. The menu open
  reads each as it is now, as the chip and the toast do. The chip stands
  whenever a copy runs, since its Machine is on, so with the toast closed
  and the Draft gone or sent here the copy is still reached; a new Draft
  (⌘N) is its home when there is no other. A copy whose Draft's Pane
  closes waits for no Draft; one that stopped stays listed until the
  operator closes its card.
- **The Draft waits on its copy until it is in.** While it runs, the
  Draft's chip shows a ring and the percentage, "Send once copied"
  stands beside it, and there is no Send: a first send starts nothing.
  A copy that stopped short (cancelled, its Machine off or removed, the
  connection lost, a failure) keeps the Draft waiting: the chip keeps E,
  "Copy stopped" stands beside it, still no Send, until its Resume puts
  it in or the operator picks where the Draft runs. Closing its card or
  toast does not let the Draft go; it only means no toast tells of the
  stop again. The chip's menu ticks E, "copying 42%" or "copy stopped",
  whose pick brings the card back; another environment ("Local", "send
  here instead") lets the Draft go there, the copy carrying on. Once the
  copy is in, the Draft is on it and Send is back. Nothing is queued.
- **A copy that ends with the card gone still says so.** A toast "Copied
  P to E", whether or not the progress toast was closed, with how much in
  how long, which goes by itself as other toasts do unless it left items
  as they were; one for a copy that stopped stays until it is closed.
  A finished copy is kept only while it leaves the operator something to
  do or see: the operator closing its toast, or acting on it, lets it go,
  and so does its toast going by itself once its Draft moved there and it
  left nothing as it was. A copy let go takes its system notification
  with it, so no click opens nothing. Otherwise a toast that went by itself leaves the
  copy, its system notification and its row on the chip as they were, so
  "Start a chat there" or what it left is still on its card. With the window in the background, the chime and the system
  notification too (Settings › Notifications), card up or not, whose
  click opens the card; it is taken back once the operator, in the
  window, has the card or a toast show how the copy ended. A Draft that
  has since moved or been sent stays where it is, and the toast and the
  card's last face, marked with a check, offer "Start a chat there";
  closing the card leaves the toast's. A failure turns the toast into
  "Copy of P to E stopped", how it stands, Show and Resume (or Retry).
- **A failure is said in the operator's words**, the card and the toast
  alike (`Entry::standing`): "The connection to E was lost." when the
  connection went, else the folder that could not be written and why,
  under `~/`: "Can't write in ~/projects/app: permission denied." A
  Machine turned off in Settings stops its copies at once: "E was turned
  off. Resume finishes the copy once it is back.", and once E went and
  answers again, plainly, not as a failure, "E is back. Resume finishes
  the copy." A connection lost reads the same way once E has been seen
  gone and answers again; one whose Server never went is never said to
  be back. A Machine removed takes its copies with it: "E was removed.",
  no Resume, no Retry, no Change…, only Close. A copy whose Machine went
  goes on being heard until its thread ends, so how it truly ended
  counts: one whose files had all landed keeps them, with what it left as
  it was, and its Resume only adds it, never copying again. So does one
  that landed but could not be added on E ("Copied, but not added on E:
  …"), and one added whose Placement this computer could not keep
  ("Copied to E, but this computer could not keep where it went: …"):
  never a dead end said only in the log. Through a first copy its button
  reads Copy; only a resume's reads Resume. Retry from a failure copies
  again.
- `CONTEXT.md`'s Draft entry and ADR 0045's line on the strip over the
  Composer say the environment is a choice until the first send.

## 3. Setup sync (built)

> Superseded in part by ADR 0050 (2026-11-25): what is synced is what the
> operator wrote, as files, and what they chose, as keys of their tools'
> files. Nothing a tool derives travels: the table below, its seeded
> marketplaces, its portable JSON files and its whole `config.toml` are
> replaced. Text compares without its line ends, a link into the table is
> made again as a link, the side that turns a Claude plugin on installs it,
> and the Setup root carries the table's version. No sign-in is synced:
> each computer keeps its own (ADR 0050, Sign-ins), so what this section
> says of sign-ins and the Keychain no longer holds. The rest of this
> section stands.

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
- **A Windows Server makes links as Windows tools do**, not as Unix does.
  A junction holds an absolute path, so a copied folder moved on Windows
  keeps links into its old place until the next copy remakes them. A file
  symlink needs Developer Mode or an elevated process; without one the
  link is a copy of its file, refreshed when that file's hash changes, and
  git there may list a tracked file link as modified. A link a Unix side
  spells with a needless `..` (`a/../b`) is removed and made again by
  every send. Links take
  little of a copy: to this computer's own Windows Server, a debug build,
  600 pnpm junctions took 1.1 to 1.7 s, against 0.29 to 0.41 s for a send
  with nothing to do: about 1.5 ms a link, asked ahead as any request is
  and made one after another by the connection's lane.
- **A window older than its Server stops at `unmade`**, which it reads as
  an unknown reply, as it stopped at `link` before. A newer window and an
  older Windows Server stop at the first link as before, and a Resume
  after the Server is updated finishes the copy.
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
- **A submodule goes as this computer checked it out.** Its files' line
  ends, bits and checkout settings come from the Project's own index and
  config, which list a submodule as one entry; the submodule's own are not
  read. From Windows an executable in one lands 0644 and its text keeps
  CRLF under its own `core.autocrlf`, so a script there does not run until
  the submodule is checked out afresh on the Machine.
- **A converted file reads modified on the Machine under
  `core.splitIndex`.** The copy clears the sizes of what it converts in
  `.git/index`, and a split index keeps most entries in
  `sharedindex.*`, which it sends as it is; `git status` there lists
  those files until the index is refreshed.
- **`core.eol` goes in the copy's `.git/config`.** Only `core.autocrlf`,
  `core.filemode` and `core.ignorecase` are taken out; a `core.eol=crlf`
  set here makes git there check out CRLF.
- **"Use what is there" registers what is there under P's name.** The
  destination may hold an unrelated repository; the sheet registers it as
  the operator asked, and the window names a Project made there as P is
  named, as for a copy.
- **Submodules were not tried live**: the tests and the WSL Machine runs
  copy Projects without one.
- **Switching a Draft's environment sets its workspace target back to
  Main**, and whether the Draft's provider is installed on the Machine is
  not checked until the first send.
- **A Machine's Server remembers a missing agent CLI until it restarts**:
  one installed there after the Server looked is still missing to it.
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
