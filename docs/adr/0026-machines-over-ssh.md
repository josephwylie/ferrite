# 0026 — Machines: another computer's Server, over ssh

Status: accepted 2026-10-15 (Herdr benchmark, §6 Remote over SSH).

## Context

ADR 0025 split the agents into a Server with a frozen generation-1
client contract. The plan (`docs/research/herdr-benchmark-2026-10-15.md`,
§6) asks for the same window on another computer's Server: `ferrite
--remote <target>` runs `ssh <target> ferrited bridge`, bridged to a local
socket as Herdr's `SshStdioBridge` does; saved machines appear in one nav;
only the selected machine streams transcripts. `ferrited` must already be
on the remote PATH. A sign-in that needs the operator (a password, an MFA
code, a new host key) happens in ssh itself, and Ferrite keeps no
credential.

## Decision

### `ferrited`

A second `[[bin]]` in `ferrite-core` (`src/bin/ferrited.rs`), as ADR 0025
foresaw: the Server without the window's libraries, so it builds and runs
on a headless Linux box. `ferrited server` is `server::run`;
`ferrited bridge` makes sure the machine's own Server runs
(`agent::ensure_running`, which starts `ferrited server` detached) and
then carries stdin to its socket and the socket to stdout until either
ends. What fails is said on stderr, which ssh hands back. Install it with
`cargo install --path crates/ferrite-core --bin ferrited`.

A Server started detached gets a session of its own (`setsid`), so the
hangup when the ssh session that started it ends never reaches it. That
does not escape systemd-logind: on a Linux host with
`KillUserProcesses=yes`, everything in the login session's cgroup is
killed when it ends, Server and agents with it. There the operator
enables lingering (`loginctl enable-linger`) or starts `ferrited server`
as a user service; Ferrite does not reach into the host's init.

`store_dir` and `settings_dir` move from the app to `server`, shared by
both binaries.

### The bridge (`machine::Bridge`)

The window never speaks ssh. Each machine has a local socket per Ferrite
process (`~/.ferrite/machines/<hash>/bridge-<pid>.sock`, made by
`link::listen`: 0700 directory, 0600 socket, peer uid checked; the ones
left by processes that are gone are swept when the bridge is made), and
each connection to it is one run of

    ssh -T -o BatchMode=yes -C -o ServerAliveInterval=15 -o ServerAliveCountMax=4
        -o ControlMaster=auto -o ControlPath=~/.ferrite/machines/<hash>/c
        -o ControlPersist=600 -- <target> ferrited bridge

so `link::Client::attach` is unchanged and the client contract is the
frozen one. `BatchMode=yes`: a bridge never prompts; a connection that
needs the operator fails. `ControlPath` is Ferrite's own (the operator's
`~/.ssh/config` still applies otherwise): one sign-in carries every
connection for ten idle minutes after the last. Windows' OpenSSH has no
multiplexing, so there the control options are left out and a machine
needs key-based auth. A target is one word that is never an option
(`machine::check_target`), and `--` precedes it.

How a run ended is read from its exit status and stderr
(`machine::Failure`): 127 is "`ferrited` is not on the machine's PATH";
255 with `Permission denied`, `Host key verification failed` (and the
like) is "sign in"; anything else is ssh's last line. The bridge records
it before it closes the window's end, so the attach that fails carries it
(`io::Error` wrapping the `Failure`). That is why the socket is the
process's own rather than shared between Ferrites: the failure is held
in the process serving the socket, and a window attaching through
another Ferrite's socket would never hear it — no sign-in band, and
retries that never stop to wait for the operator. Sharing would save
nothing else: the ssh master (`ControlPath`) is shared either way. If the
socket's accepting stops, the next attach listens again.

The tunnel is a seam (`machine::Dial`): production spawns ssh; tests
dial the in-process Server's socket, or end at once with a scripted
failure.

### Signing in

"Sign in" (the band, Settings › Machines) writes a script beside the
master's socket and opens it in a terminal (`open` on macOS,
`x-terminal-emulator` on Linux): `ssh` with the same master options and
no `BatchMode`, running `true`. ssh asks for whatever it needs; the
master it leaves carries the bridge. Ferrite polls `ssh -O check` for up
to ten minutes and attaches as soon as the master is up. A machine that
needs a sign-in is never asked again on its own — each ask would be a
refused login on it, which some servers count — only after a sign-in or
when the operator clicks. Other failures back off from 250 ms to 30 s
(5 s for this computer's Server).

### Attaching (`cockpit::Endpoint`)

`Attach` names its Server: `Endpoint::Local { executable }` (ADR 0025's
behaviour, attached at once) or `Endpoint::Machine(Arc<Bridge>)`,
attached in the background from the first `poll`, since ssh can take
seconds. `ServerState::Detached` carries `attention`, the `Failure` only
the operator can fix. A machine's window reads parked Threads from an
empty Store of its own (`~/.ferrite/machines/<hash>/threads`): a
machine's log stays on the machine.

### One machine on screen

`cockpit::ServerStatus` is a link that never sends `watch`: it hears the
snapshot, every Thread's `thread` state, the Roster and the Decisions —
never a history or an input. The app's `Machines` keeps one per enabled
machine the window is not on, and none for the one it is on, whose
Cockpit watches its open Threads.

Going to another machine builds the window's `CockpitView` afresh on a
Cockpit attached there (`CockpitWindow::go_to`, on the view's
`GoToMachine` event), carrying the current settings. Rebuilding rather
than swapping the Cockpit inside the view was chosen because the view
keeps per-Thread state in dozens of places keyed by `ThreadId`, and ids
are per Store: two machines reuse them. The machine left behind starts
its status link; bridges and their masters live for the run.

### The nav

With more than this computer, the nav's tree is headed by the machine
the window is on — a heading in the heading style: triangle, connection
glyph, name, and a word when it is not up — and the other machines
follow the tree, each a heading and, unfolded, its open Threads with
their marks (working, needs you). The glyphs: `●` green online, `○`
connecting, `●` yellow needs you (sign in, or no `ferrited`), faint `○`
off. A press on another machine's heading or row goes there.

A machine whose Server is not there stays visible and read-only: its
board, its tree (smart sections and Projects) and Parked dimmed
(`MACHINE_LOST_OPACITY`) under covers that take every press; the
window's root holds the keyboard, so no Composer or control takes a key;
and the root stops listening for what acts on a Thread (send, the
answers, close, reopen, paste; escape no longer interrupts). A band says
why — "Sign in to box" (a click signs in), "box has no
ferrited", or "box is not connected" once it was. A machine still being
reached for the first time says so only by its glyph. Off-screen
machines that go keep their last rows, dimmed.

### Settings

`settings.machines`: `{name, target, enabled}` in nav order. A machine
turned off is never connected to: `--remote` naming one starts the
window on this computer, the machine listed as off, and turning off the
machine the window is on puts the window on this computer. Settings ›
About › Machines lists each with sign in (when it needs one), turn
on/off, remove — or save, for the `--remote` machine of this run — and a
field to add one by its ssh target. "Start at login" is hidden while the
window is on a machine: it is this computer's.

### Projects and new Threads on a machine

A window on a machine never reads a path of the machine on this
computer: the paths look the same, and may even exist here, but they are
another computer's. What the Server can answer, it answers; what it
cannot, the window does without.

- **Folders.** The Project card's folder dialog could only browse this
  computer, so on a machine it is not offered. A path field stands in for
  it, starting at `~/`: what is typed completes (a press, or Tab) against
  the directory it names, listed by the machine's Server through a new
  gen-1 command, `list_dir {path}`, replied `listing {path, entries[{name,
  directory}]}` with `~` spelled out (`workspace::list_dir`). ↵ in the
  field adds the directory to the card; Create registers the Project
  through the existing `project` command. Additive under ADR 0025's rules:
  an older Server answers `unsupported`, and the field then completes
  nothing. A listing is one `read_dir` on the pump thread, never a walk.
- **`~`.** A path goes to the Server as typed; the registry spells out
  `~` against its own computer's home before canonicalizing
  (`workspace::expand_home`), so `path ~/src/x` on a draft's Project chip
  and the card's field mean the machine's home. The window's own
  expansion is gone: it would have meant this computer's.
- **Drafts.** The Project chip lists the machine's Projects (the
  registry is the Server's), and a first send starts the Thread there, as
  on this computer. The workspace chip's worktrees are the ones the
  Server registered for the Project, each on its branch; git's own
  listing and the repo's branches are not offered. Whether a chosen
  worktree still stands is the Server's to say when it starts
  (`DraftBinding::resolve(registry, here)`).
- **No git here.** The branch labels and status (`workspace::branch_status`),
  the worktree listing that lets a binding follow its agent
  (`refresh_branches`), the checkout and default-branch lookups, a
  review's uncommitted and branch scopes (the last turn still reads), and
  the `@` menu's walk of the checkout are skipped on a machine.

The window's tests reach a machine whose Server runs in the test's own
process (`server::serve_in_thread`, behind ferrite-core's `test-support`
feature, which only the app's dev-dependencies turn on).

## Not done here

- On a machine, a Thread's branch label and status, and its binding
  following the agent into a worktree it made: both need git on the
  machine, which the Server does not yet run for a window.
- On a machine, a file opened beside a Pane and the operator's own
  commands (ADR 0024) still read or run on this computer; a review reads
  only the last turn.
- A machine's parked Threads are not listed (its log is on the machine).
- Notifications (sound, system) for Decisions on a machine off screen.
- Phone access (plan §6, "later").
