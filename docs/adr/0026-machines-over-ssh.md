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
`cargo install --path crates/ferrite-core --bin ferrited`, then, on
Linux, `ferrited service install`.

A Server started detached gets a session of its own (`setsid`), so the
hangup when the ssh session that started it ends never reaches it. That
does not escape systemd-logind: on a Linux host with
`KillUserProcesses=yes`, everything in the login session's cgroup is
killed when it ends, Server and agents with it, and nothing restarts a
Server that crashed. So on Linux the Server runs under `systemd --user`,
as the plan's §1 asks — the LaunchAgent's counterpart (`agent::systemd`):

- `ferrited service install` writes
  `$XDG_CONFIG_HOME/systemd/user/ferrited.service` (`~/.config` by
  default; rewritten when `ferrited` moved) — `ExecStart` the running
  `ferrited` with `server`, `Restart=on-failure` as launchd's
  `SuccessfulExit = false`, `WantedBy=default.target`, and the default
  `KillMode`, so the agents stop with the Server as on macOS — then runs
  `systemctl --user daemon-reload` and `enable --now`. When
  `loginctl show-user` reports `Linger=no` it says so: without lingering
  the user manager, and the Server with it, stops at the last logout, so
  the operator runs `loginctl enable-linger`. When a Server already
  answers and the unit is not active — one `ferrited bridge` started
  detached — the unit's Server finds the Store taken and stops; it says
  that too: the service takes over at the first connection after that
  Server stops. A unit already running keeps the old `ferrited` until
  its Server next starts, as the LaunchAgent does. The path is the ssh
  shell's `XDG_CONFIG_HOME`, not the user manager's; where the two
  differ, `enable` finds no unit and install fails, saying so.
- `ferrited service uninstall` removes the unit and its
  `default.target.wants` link by hand rather than `systemctl disable`,
  which needs both the user bus and the unit file, then reloads the user
  manager if it can. As on macOS, the running Server is not stopped:
  that would end its agents.
- `agent::ensure_running`, for the operator's own Store, runs
  `systemctl --user start ferrited.service` when the unit is installed
  and waits for the socket, as the macOS path kickstarts the LaunchAgent;
  with no unit, or a start that fails, it starts the Server detached as
  before.

A Server that cannot claim the Store exits 0, so systemd, like launchd,
does not respawn it in a loop. The `systemctl` and `loginctl` calls go
through a seam (`systemd::Run`) the tests script. Nothing is added to the
window's "Start at login" on Linux: the window's own Server is not this
work's subject.

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
the operator can fix.

### Parked Threads

A machine's logs stay on the machine, so its window has no Store to
read parked Threads from. Its Server publishes them instead: `parked
{revision, threads[]}`, latest-wins, and the snapshot's `parked`, a
gen-1 addition (ADR 0025). Each row is what the window would have read
off the log — provider, title, the first prompt's title, binding,
Project, last use (ms since the epoch), turns and Subagents — or only
its age when the log cannot be read.

Every window lists its parked Threads this way, this computer's too: a
window that read this computer's Store itself peeked every parked log
on the UI thread whenever the parked set moved — about three header
reads, a `stat` and a first-prompt read per Thread, 155–480 ms with the
operator's ~500 parked Threads, after every delete, park or rename. A
window still says `parked_threads` in its hello, for a Server from
before that sent rows only to a window that did; a Server now keeps and
publishes them always.

The Engine reads each row once, on a thread of its own rather than the
pump (`engine::parked::ParkedListing`): a row waits for a park's final
mark, and a log from before schema 13 is replayed once for its counts
(`Store::summary`). It reads a row again only for a Thread that was
parked, or changed while parked (a rename, a provider change, a retired
worktree); a Thread imported is read because it has no row, and a
Thread revived or deleted loses its row unread. A row that went stale
while its read was out is read again; the rows are published once none
is due, when they differ from the last published. An Engine its window
pumps in-process (`--demo`, `--load`, the tests) reads them in place, so
a park is listed by the pump that parked it.

`Cockpit` reads parked Threads through one seam (`cockpit::Parked`):
the rows its Server last published, from which the nav's names,
providers, Projects, ages, turns and Subagent counts come with no log
read. A row that moved tells the window to rebuild its parked rows
(`take_parked_changed`); a delete drops the row from the window's copy
at once. On this computer the window keeps a read-only Store for what
an open Thread's header and last write say (its Project, its age),
which no message carries yet. Reopening is the `revive` command, as for
any Thread.

The empty per-machine Store is gone: nothing reads a machine's Threads
from `Attach::store`, which only names the directory the window's
copies of its Groups and Projects belong to, and nothing creates it.
The window's copy of a machine's Groups checks a drop's shape only;
whether its Threads exist and have a Project is the machine's Server's
to check when it applies the change.

### One machine on screen

`cockpit::ServerStatus` is a link that never sends `watch`: it hears the
snapshot, every Thread's `thread` state, the Roster, the Decisions and
the Notices — never a history or an input. The app's `Machines` keeps
one per enabled machine the window is not on, and none for the one it
is on, whose Cockpit watches its open Threads.

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

### Alerts from machines off screen

A request on box reaches the operator while the window shows another
machine: the window's bell hears each machine off screen through its
status link and rings for it as it does here (ADR 0017, "Machines off
screen") — a toast and a chime, and a system notification while the
window is in the background, each naming the machine. Opening one goes
to the machine (`GoToMachine` carries the alert) and the Cockpit built
there opens it once attached (`CockpitView::land`). The bell goes with
the window (`Bell::moved`): nothing rings twice, what was waiting still
rings, toasts stand on, and a system notification posted before the
move still opens on its own machine. `mark all read` reaches every
machine off screen through its status link — the one act a status link
sends.

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
  it, starting at `~/`, whose listing is asked for as the card opens:
  what is typed completes (a press, or Tab) against the directory it
  names, listed by the machine's Server through a new gen-1 command,
  `list_dir {path}`, replied `listing {path, entries[{name, directory}]}`
  with `~` spelled out (`workspace::list_dir`). ↵ in the field adds the
  directory to the card and empties the field, so a second ↵ confirms
  the card rather than adding anything more; Create registers the
  Project through the existing `project` command. The card stages the
  path as the listing in hand spelled it out (of it, or of its parent),
  so `~` is named for the home's leaf and one directory typed two ways
  stages once; with no listing in hand it goes as typed, and the
  registry spells `~` out. Additive under ADR 0025's rules: an older
  Server answers `unsupported`, and the field then completes nothing. A
  listing is one `read_dir`, never a walk, on a thread of its own as
  `list_repo`'s git is, never the pump: a huge folder or a hung mount on
  the machine holds up only the field. A directory that cannot be read
  is not asked about again until the field leaves it. Only a Server
  answers `list_dir`, as `list_repo`: the Server, not the Engine, owns
  both, so a window that runs its agents itself never shows the field.
- **`~`.** A path goes to the Server as typed; the registry spells out
  `~` against its own computer's home before canonicalizing
  (`workspace::expand_home`), so `path ~/src/x` on a draft's Project chip
  and the card's field mean the machine's home. The window's own
  expansion is gone: it would have meant this computer's.
- **Drafts.** The Project chip lists the machine's Projects (the
  registry is the Server's), and a first send starts the Thread there, as
  on this computer. The workspace chip asks the machine's Server what the
  repo holds through a second gen-1 command, `list_repo {project}`,
  replied `repo_listing {checked_out?, worktrees[{path, branch?}],
  branches[]}`: the same `workspace::repo_listing` the window runs here
  for a local repo, git's own listing, so worktrees made outside Ferrite
  and the repo's branches are offered too. Its two git calls run on a
  thread of their own, never the pump. The chip names the checkout's
  branch from that answer, asked once for each draft's Project as soon
  as a draft shows it (one at a time), and `main` when the machine
  cannot say. Whether a chosen worktree still stands is the Server's to say
  when it starts (`DraftBinding::resolve(registry, here)`).
- **No git here.** The branch labels and status (`workspace::branch_status`),
  the worktree listing that lets a binding follow its agent
  (`refresh_branches`), the checkout and default-branch lookups, and
  the `@` menu's walk of the checkout are skipped on a machine. A review
  reads every scope: git runs on the machine (ADR 0021, `review_diff`).
- **A Thread's PR label** (`#123 ✓`, below) is read by `gh` where the
  checkout is: the window asks the Server that owns the Thread with a
  gen-1 command, `pull_request {path}`, replied `pull_request {json?}` —
  `gh pr view`'s JSON as printed, none when `gh` has nothing to say — on
  a thread of its own as `list_repo`'s git is. On a machine that is the
  machine's `gh`, logged in there; only an in-process Engine runs `gh`
  in the window. An older Server answers `unsupported`, which reads as
  no PR.

### The PR label on Thread rows

Matched from the herdr-plugin-gh-pr README, not its code (the repository
carries no license). Each Thread row shows its checkout's branch PR after
its drift: the number and one symbol — `✓` passing CI, `✗` failing, `●`
pending, `◆` merged, `⊘` closed, none for an open PR with no checks — and
no label without a PR. While a reading is out the symbol is `⟳`. A
reading is asked for when a Thread is focused, when its Pane opens, and
when its binding follows its agent into a worktree, at most once per
Thread every 30 seconds (`pr::AUTO_EVERY`); `refresh PR status` asks at
once and `open PR in browser` opens the PR's URL from the last reading,
in this computer's browser. Both are palette commands and rows of the
Thread's right-click menu, and actions with no default key the operator
may bind (`cockpit::RefreshPrStatus`, `cockpit::OpenPrInBrowser`, ADR
0024). No `gh`, no login and no PR all read as no label; a row never shows
an error. This replaces the CI glyph the row drew from the branch status,
whose PR was read by the window on the 60-second `PR_TTL` tick: the label's
reading (`ThreadFacts::pr`) is the one the checks card reads too, and the
branch status is git alone.
- **No Finder.** "Reveal in Finder" is not offered for a machine's
  Thread: its path is not on this computer. "Copy path" stays.

The window's tests reach a machine whose Server runs in the test's own
process (`server::fixture_machine`, behind ferrite-core's `test-support`
feature, which only the app's dev-dependencies turn on).

## Not done here

- On a machine, a Thread's branch label and drift (its PR label is
  read there), and its binding
  following the agent into a worktree it made: both need git on the
  machine read as the Thread works, not once per chip open as
  `list_repo` is.
- On a machine, a file opened beside a Pane and the operator's own
  commands (ADR 0024) still read or run on this computer.
- An open Thread's Project on a machine: the `thread` state does not
  carry it, so a machine's open Threads group under no Project. A drop
  the Server refuses (a Thread with no Project) shows as allowed while
  dragged, and is refused when it lands.
- Phone access (plan §6, "later").
