# 0025 — Agents live in a headless Server; the window is its client

Status: accepted 2026-10-15 (Herdr benchmark, Foundation #1).

## Context

Every Session is a child of the window today. `ferrite::main` opens the
Store (taking its single-writer claim, ADR 0009), builds
`ferrite_core::cockpit::Cockpit` with the production `Spawner`, and the
window calls `Cockpit::pump` every frame. Quitting or crashing the window
ends every agent, and the at-launch self-update (ADR 0023) has to run
before any Session exists for the same reason. A second Ferrite gets a
read-only Store.

The plan (`docs/research/herdr-benchmark-2026-10-15.md`, §1) asks for
Herdr's split: a background server owns the agents, the app attaches to
it, and the agents keep running when it goes. It also settles the upgrade
path: restart the server when the agents are idle and resume each Thread,
never a live process handoff.

Three facts about the current code decide the shape:

- `Cockpit` is already headless. Everything that must keep running without
  a window is in it: Sessions, writers, the prompt queue, deferred finish,
  Handover, the memory watchdog, stale-CLI restarts, the titler, Notices.
  Only the Roster's per-window part (focus, View, fullscreen) and Drafts
  are about one window.
- The window reads a Thread through `ThreadView`: its `Activity` (which
  holds the `Transcript`) plus flags and menus. `Activity` is a pure
  function of the `ActivityInput`s applied to it, and the Store already
  replays a log into the same Activity a live run built
  (`a_flushed_turn_replays_identically_after_reopening`).
- The Store's `Record` is a versioned, append-only-compatible schema
  converted from `SessionEvent`, so it is already a stable wire form for
  everything durable. What it lacks are the live-only inputs: Decisions,
  tool drafts, MCP state, menus, queue acknowledgements, Notices.

## Decision

### One binary, a `server` mode

The Server is `ferrite server`, a mode of the existing binary, not a new
crate. The plan calls it `ferrited`; that is the process, not an artifact.

- One artifact means the app and its Server can never come from different
  builds inside one install, packaging and signing are unchanged, the
  LaunchAgent points at the bundle's own executable, and `cargo run -p
  ferrite` always has a Server to start.
- The entry point is `ferrite_core::server::run`. `ferrite/src/main.rs`
  dispatches `server` before `visual::boot`, the self-update and anything
  else that touches GPUI, so the Server process never initialises a
  window, Chromium or the self-update. Remote hosts (#6) will need a
  binary without GPUI's shared libraries: that becomes a second `[[bin]]`
  over the same `server::run` when #6 lands, not now.

### Layout

All new code is in `ferrite-core` (ADR 0001: no GPUI there).

| Module | Owns |
|---|---|
| `protocol` | The frozen generation-1 client contract: message types, error codes, `Unknown` fallbacks, conversions to and from core types. No I/O. Fixtures in `protocol/fixtures/gen1/`. |
| `engine` | What `Cockpit` does today minus the window: the Store and its claim, every Session, writers, the queue, pending Decisions, Notices, Groups, the registry, the shared half of the Roster, and every policy (Handover, sweep, stale-CLI restart, titler, suggestions, deferred finish). Emits protocol messages into per-connection outboxes; takes protocol commands. Single-threaded, the pump's thread. |
| `link` | Transport: newline-delimited JSON framing, the per-connection two-lane outbox, the Unix socket and Windows named pipe, the handshake, reconnect with replay, and an in-process pair for tests and `--demo`. |
| `server` | `run`: adopt the login environment, open diagnostics (`logs/server.log`), claim the Store, build `Engine` with the production spawner, listen, watch signals and macOS logout, drive the pump loop, restart itself. Also `agent`: LaunchAgent plist and `launchctl` calls, the Windows Run key, and `ensure_running` for the app. |
| `cockpit` | The client. Keeps its public API so `CockpitView` changes little; acts become protocol commands on a `Link`, and each Thread is a mirror (Activity + `ThreadState`) fed from the stream. Keeps the per-window Roster and Drafts. |

The production `Spawner` (`ferrite/src/session.rs`) and the login-shell
environment (`ferrite/src/shell.rs`) move to core, since the Server spawns.
The scripted demo spawner stays in the app; `--demo`, `--load` and every
app test run `Engine` in-process over the in-process link.

### Process lifecycle

**Where it listens.** `server.sock` in the Store's directory,
`~/.ferrite/threads/server.sock` (the directory 0700, the socket 0600), refused to any peer whose `getpeereid` uid is not ours. A
stale socket (nothing answers a connect) is removed before binding; a live
one means a Server already runs. On Windows, the pipe
`\\.\pipe\ferrite-server-<user SID>-<Store hash>` (FNV-1a of the
Store's path: a hash no toolchain changes, since an app and a Server from
different builds must find one pipe), created with
`FILE_FLAG_FIRST_PIPE_INSTANCE`, `PIPE_REJECT_REMOTE_CLIENTS` and a DACL
granting only the user; the client checks the pipe server's process owner
is itself before sending anything. Both use blocking I/O and a thread per
connection direction, like the rest of core: no async runtime (the pipe
is opened overlapped, each transfer waited for, so a connection's two
threads never serialize on its handle).
`FERRITE_STORE` moves the socket with the Store so tests and a second
install never meet the operator's Server.

**Starting.** The app calls `server::agent::ensure_running`: connect; if
nothing answers, on macOS `launchctl kickstart gui/<uid>/<label>` when the
LaunchAgent is installed and the Store is the operator's own
(`~/.ferrite/threads`: a dev or test Store never starts the operator's
Server), otherwise (and always on Windows) start
`<current_exe> server` detached: a new process group with null stdio on
Unix, `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW |
CREATE_BREAKAWAY_FROM_JOB` on Windows. Then it waits up to 5 s for the
socket. The Store's claim is the single-instance guarantee: a second
Server that cannot claim it exits 0 before binding (a failure exit would
have launchd respawn it every 10 s for as long as the first one runs).

**LaunchAgent (macOS).** Settings › Server › "Start at login", on by
default, installed at the first launch of a bundled app before
`ensure_running`, so the first Server is already launchd's (not for a
development build, which uses the detached start and does not offer the
setting: it would point the operator's LaunchAgent at `target/`). It writes
`~/Library/LaunchAgents/com.github.josephwylie.ferrite.server.plist` —
`ProgramArguments` = the bundle's executable and `server`, `RunAtLoad`,
`KeepAlive { SuccessfulExit = false }` (restarted after a crash, not after
a clean stop), no `ProcessType` (Background would throttle the agents'
builds) — then `launchctl bootstrap gui/<uid>` (a label already loaded
counts as done). Turning it off only deletes the plist: `bootout` would
SIGTERM the running Server, so the job stays loaded until logout and
nothing starts it at the next login. Each launch rewrites the plist if
the bundle moved; a job already loaded keeps the old path until logout
(re-bootstrapping it would SIGTERM a Server that may be running), so
when a kickstarted Server does not answer within 5 s, `ensure_running`
starts the detached one and waits again. Provider CLIs stay in the Server's process group, so when a crashed
Server's job ends launchd kills what it left (its default, no
`AbandonProcessGroup`) before starting the next one: an orphaned Codex
app-server would otherwise hold its thread's lock against the resume. A
LaunchAgent runs in the login session, so `claude`/`codex` see the
operator's keychain. `SMAppService` was rejected: it wants a signed
bundle and an Objective-C bridge, and releases may be unsigned.

**Windows.** The same setting writes or removes
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run\FerriteServer` =
`"<exe>" server`. A crashed Server is restarted by the next app launch or
reconnect; there is no supervisor.

**Running.** The pump loop owns `Engine` on one thread. It waits on the
command channel with a 16 ms timeout while any Session is live, or
indefinitely when none is. Each pass applies commands in arrival order,
drains Session events exactly as `Cockpit::pump` does now, and pushes
messages into outboxes. At boot it revives every Thread marked open (the
existing `.open` markers, staggered as in ADR 0018), so after a crash or
reboot the Threads resume without a window.

**Stopping.** SIGTERM and SIGINT set a flag the pump sees. On Windows so
do a console's Ctrl+C and close (a Server run by hand) and `WM_ENDSESSION`
at logout and shutdown: the Server the app starts has no console, so it
keeps a hidden top-level window for that message, and holds the process
up while the pump flushes. The pump flushes
every writer, ends every Session without parking it (the open markers
stay, so the next boot resumes the same Threads; resume ids are already in
the log as `init` records), and runs `Store::quit` with the existing
deadline. On macOS a thread registered with `notify(3)` for loginwindow's
`logoutNoReturn` / `shutdownNoReturn` does the flush first, because
loginwindow kills the provider CLIs 15–20 s before launchd's SIGTERM
(after Herdr's `platform/macos/shutdown.rs`; the keys are undocumented, so
a missing key falls back to SIGTERM). After that point the Engine freezes:
Sessions that close are not recorded as ending or restarted. `stop {when}`
from a client does the same, at once or when idle (as `restart` below),
then exits 0.

### Durability

- The Engine flushes a Thread's writer at turn end (as now), when a Decision
  is requested, and when one is answered or closed. Everything an agent
  did before it blocked is on disk while it waits.
- Decisions are never persisted (unchanged). A Decision pending when its
  Session ends is closed for every client (`session_ended`).
- `Input::Revived` closes an open turn as interrupted: a Thread whose log
  ends inside a turn (a crash, a kill, "restart now") reads "interrupted"
  instead of looking idle with no outcome. The transcript changes, not the
  log: nothing is written that the provider did not say.

### Protocol, generation 1

One JSON object per line, internally tagged by `type`, over the socket.
The connection opens with `hello` and nothing else; the Server answers
`welcome`, then `snapshot`. The automation API and CLI (plan §2) were
to share the listener with another `role` in `hello`; ADR 0027 gives them
a socket of their own instead. Nothing here depends on it.

The contract rules, frozen with generation 1 and kept for as long as an
app or Server of generation 1 can exist:

- Fields are never removed, renamed or retyped. New fields are optional
  (`#[serde(default)]`); unknown fields are ignored (no
  `deny_unknown_fields` anywhere in `protocol`).
- Every enum that crosses the wire has an `Unknown` fallback
  (`#[serde(other)]`). An unknown message `type` decodes to `Unknown` and
  is ignored; an unknown command with an `id` is answered `unsupported`.
- Optional behaviour is negotiated by name in `capabilities`, never by
  version comparison.
- A change that cannot follow these rules is generation 2, offered beside
  1, not instead of it.

Client → Server:

| Message | Meaning |
|---|---|
| `hello {generation, client:{name, version}, capabilities[]}` | Gen-1 capability: `render_visual` (this client can draw Visuals). |
| `watch {thread, resume?:{boot_id, after}}` / `unwatch {thread}` | Stream a Thread's inputs. Without `resume`, or when it cannot be met, the stream starts with its history. Only watched Threads stream; every Thread's `ThreadState` is always sent. |
| `command {id, op, …}` | One per act that changes shared or durable state: `open`, `bootstrap` (a Draft's first prompt; replied once the Thread starts), `cancel_bootstrap {request}`, `send`, `queue`, `unqueue`, `cancel_queued {edit?}`, `take_recovered`, `recall`, `interrupt`, `answer {thread, handle, answer}`, `control`, `set_model`, `set_effort`, `set_provider`, `rename`, `park`, `revive`, `revive_group`, `delete`, `import`, `group` (replied with what dissolved), `layout`, `project`, `notice` (open, dismiss, read, seen), `show_subject`, `load_subject`, `say`, `worktrees_listed`, `cli_replaced`, `settings_changed`, `search_files`, `restart {when}`, `stop {when}`. Each gets exactly one `reply`. |
| `rendered {request, png?, error?}` | The answer to a `render`. |

Server → Client:

| Message | Lane | Meaning |
|---|---|---|
| `welcome {generation, server:{version, executable}, boot_id, capabilities[], error?, uptime_ms?}` | — | `error.code = unsupported_generation` when the generations do not meet; the Server then closes. `uptime_ms` is the `at_ms` clock now. |
| `snapshot {boot_id, revision, threads[], roster, decisions[], groups?, registry?, notices?, providers?}` | — | Everything a window draws before any transcript. `roster` is the shared half: open Threads in order and the park order. |
| `history {thread, after, inputs[]}` … `history_end {thread, seq, live[]}` | reliable | A watched Thread's log, read off the pump thread up to a writer `checkpoint` taken on the pump thread at `seq`, in pieces of about 1 MiB (`after` counts the inputs before each; `0` starts the Thread over); then `live`, the live-only inputs that rebuild what the log never holds and is still in effect (the Session's `Connect`, pending Decisions, MCP state, rate limits). Live inputs continue after `seq`. |
| `input {thread, seq, at_ms, input}` | reliable | One `ActivityInput` the Engine applied, in order. |
| `decision {thread, subject?, handle, decision}` / `decision_closed {thread, handle, by}` | reliable | To every client, watching or not. `by`: `answered {client}`, `cancelled` (the provider withdrew it), `session_ended`, `server_restart` (the Server restarts or stops). A watcher also reads each Decision from its Thread's inputs. |
| `reply {id, ok?, error?:{code, message}}` | reliable | Codes: `already_answered`, `invalid_answer`, `unknown_thread`, `unsupported`, `restarting`, `failed`. |
| `thread {revision, state}` | latest-wins per Thread | `ThreadState`: everything `ThreadView` reads that is not Activity — Header facts, Attention, starting/busy/working/background, queued prompts, suggestion, command and model menus, permission modes, supported controls, store error, generation. |
| `tool_draft {thread, tool, input}` | latest-wins per tool | `SessionEvent::ToolDraft`, already a snapshot of the input so far. |
| `roster`, `groups`, `registry`, `notices`, `providers` `{revision, …}` | latest-wins | Replaces the snapshot's field. |
| `render {request, …}` | reliable | A Visual to draw, sent to one `render_visual` client (the one most recently focused). With none attached the agent is answered at once: no Ferrite window is open to draw it. |
| `allowance_warning {provider, window, threshold, used_fraction}` | reliable | A provider's allowance rose past one of the operator's thresholds. |
| `restarting {when}` / `stopping` | reliable | The client reconnects. |

An `input` carries `{"record": …}`, a Store `Record` exactly as the log
spells it and applied live; `{"replay": …}`, the same line replayed (a
revive's history, a Subagent's history loading); or `{"live": …}`, a
gen-1 DTO for what the Store never keeps: notices, answers, revival,
Decisions, Activity connect/disconnect, menus, MCP servers and
authorizations, rate limits, restored tool clocks. Queue
acknowledgements are `ThreadState`, not inputs. Durable
history is therefore the log's own lines, replayed as a reopen replays
them. The Store's on-disk rule is
unchanged: a log from a newer schema is refused whole. On the wire, a
record kind this client does not know decodes to `Unknown` and is
skipped, and the window says it is older than its Server.

`seq` counts a Thread's inputs within one `boot_id`. The Engine keeps the
last 8 MiB of each open Thread's encoded inputs, watched or not: a
reconnecting client has, by then, stopped watching. A `watch` whose
`resume` names this boot and a `seq` still held replays from there; any
other starts again from `history` and the client rebuilds that Thread's
mirror. `revision` counts the Server's committed changes; latest-wins
messages carry it so a client drops one older than its snapshot.
`at_ms` is the Server's monotonic clock since boot; the client offsets it
by the difference measured at `welcome`, so working timers agree across
clients.

**Lanes.** One connection, one outbox. Streaming text is never dropped,
not sent latest-wins as the plan sketched: Herdr's latest-wins lane
carries whole screen states, but a delta is not a state, and dropping one
loses text. Coalescing gives the same effect losslessly: adjacent text,
thinking and tool-output deltas of one Thread are concatenated while they
wait (the merged `input` carries the last `seq`). The latest-wins kinds
(`thread`, `tool_draft` per tool, `roster`, `groups`, `registry`,
`notices`, `providers`) take a keyed slot: a newer one drops the one of
its kind still waiting and joins the back of the queue, so it is never
delivered ahead of anything queued before it. They share the one ordered queue
rather than a second one, as first written here, because order carries
meaning between them: a `roster` that closes a Thread must not overtake
that Thread's last inputs, nor a tool draft the start that settles it. A
connection whose queue passes 32 MiB is closed with a `reply {id: 0,
error.code = lagged}`; the client reconnects and resumes, usually from
`history`, since 32 MiB outruns the ring. A history is not queued as
messages: it waits in the queue as one entry that the connection's writer
thread reads from the log and sends as it goes, so a log of any size (the
Store holds logs past 100 MB) neither trips the limit nor stalls the pump.
A connection the Server lets go sends what it already queued, then closes
and ends both its threads.

**Decisions.** The Engine is their only owner. `answer` names the
`DecisionHandle` the client drew (generation, serial and the provider's
request id), because a request id is unique only within one provider
process, and a replaced Session may reuse one. It is checked on the pump
thread: the Decision must be pending and not already submitting, the
answer must fit its kind (`DecisionKind::validate_answer`: the variants
each kind takes, a form's values and each question's picks), and the
whole handle must match, so a replaced Session's Decision never matches
its successor's. What only the provider's adapter can judge (a choice it
never offered) is refused there, before anything is sent, and replied
`invalid_answer` too. The first that passes is
delivered to the provider, recorded, flushed, replied `ok`, and broadcast as
`decision_closed {by: answered}` with the answering client. Every later
answer is replied `already_answered`; an invalid one `invalid_answer`,
with the Decision still pending. No one retries an answer: a client that
loses its connection with an answer in flight shows "the answer may not
have arrived", and after reconnecting draws whatever the snapshot says —
still pending means the operator answers again.

### Clients

- `Cockpit` holds a `Link` instead of a Store and a Spawner. Its mirrors
  apply exactly the `ActivityInput`s the Engine applied, so the window draws
  what it draws now. `PaneUpdate` is computed from the mirror, as the pump
  does today. Groups, the registry, Notices and the providers' state are
  read-only copies built from their latest-wins messages.
- The link is one of two. Attached (`Cockpit::attach`), an act is a
  command whose reply the window waits for, reading what arrives before it
  into an inbox, so the window is up to date when the act returns; a
  first send (`bootstrap`, answered once the Thread has started, and taken
  back by `cancel_bootstrap`) and a file search answer later. In-process
  (`--demo`, `--load`, the tests) an act stays a direct call, keeping its
  typed error; both read the same mirrors and copies. Acts only fixtures
  and tests use (injected inputs, fixture Notices, the launch-time revival
  the Server does itself) exist in-process only.
- What a parked Thread says (titles, its Project, last use, the parked
  list, Subagent counts) the window reads itself, through a Store handle
  that never takes the claim (`Store::reader`), so a Server started after
  the window still finds it free. So do the derived reads — Needs-you
  order, display titles, the last Project worked in. Discovery of CLI
  session files and workspace commands reads the operator's own files and
  stays in the window too.
- Per window: focus, View (Solo or a Group), fullscreen, Drafts, the
  Composer's prompt history, find. Shared through the Server: which
  Threads are open, Groups and Layouts, projects, Notices and whether they
  are seen (each window reports what it has on screen while focused).
- Several clients may attach. A second app process attaches instead of
  getting a read-only Store, so `Store::read_only` and its banner leave
  the app. Several windows in one app are not built here.
- Quitting or closing the window ends nothing, so ⌘Q no longer asks.
  Settings › About's Server group shows the Server's version, uptime and
  live Sessions, with "Start at login", "Restart" and "Stop" (both ask
  first while an agent works or a Decision waits — the question ⌘Q asks
  today).
- Without a Server the window shows a band — a notification that stays
  up, with the reason; clicking it retries now — and keeps reconnecting
  with backoff (from 250 ms to 5 s; 2 s after `restarting`, so a Server
  coming back from `exec` is not raced by a second one). It never falls
  back to running agents itself: two pumps on one Store is the bug the
  claim exists to stop.
- The window still lists each Thread's worktrees (it runs git for its
  branch labels anyway) and sends the list as `worktrees_listed`, with
  how long ago it was read, so the Server judges it against its own clock.
  A focused window reports a Thread seen only when looking at it would
  read something, since the check runs every frame.
- Settings stay a file the window writes. `settings_changed` makes the
  Engine reread it; it applies to the next Session, as today.

### Upgrading the Server

The plan's decision stands: restart when idle and resume, never hand
over live processes.

- `welcome` names the Server's version and executable. When the app's
  version is higher (semver; equal versions never restart on their own — a
  developer uses Settings › Server › Restart) and the Server runs the
  app's own executable, the app sends `restart {when: idle}`, once per app
  run, and shows "Ferrite's server will update when your agents are idle ·
  Restart now". "Restart now" sends `restart {when: now}`.
- Idle is the predicate `restart_idle_stale` already uses for a stale CLI:
  for every Thread no turn, no working Subagent or background task, no
  Decision and nothing queued. New prompts are not blocked while a
  restart waits. "Restart now" asks first while that predicate is false,
  as ⌘Q does today: queued prompts are not kept.
- The executable check matters: a Server always restarts into its own
  path, so a `cargo run` app attached to the installed Server (or one from
  a moved bundle) would otherwise ask again after every restart, cutting
  the agents off each time they idle. Such an app shows the mismatch in
  Settings › Server instead.
- On idle (or now), the Engine checks its executable still exists (else
  it replies `failed` and keeps running), closes pending Decisions
  (`server_restart`), broadcasts `restarting`, flushes and ends Sessions as on
  SIGTERM,
  then restarts itself: `exec` of its own executable on Unix (same pid,
  so launchd sees no exit; the claim's file is close-on-exec, so the new
  image takes it), on Windows a detached start of `server --after <pid>`
  and exit; the new Server waits for that pid to end before it claims the
  Store, or it would find the claim held and exit, leaving none. The new
  Server revives the open Threads (`claude --resume`, `thread/resume`);
  clients see a new `boot_id` and reload their mirrors. A turn cut off by
  "now" reads as interrupted.
- With ADR 0023: on macOS the installer replaces the bundle under a
  running Server, and the restart above follows. On Windows the setup
  cannot replace a running executable (and Inno's silent
  `CloseApplications` would kill the Server unflushed), so "restart to
  update", which already waits for every agent to be idle (or asks, for
  "now"), sends `stop {when: now}` and waits (up to 5 s) for the
  Server to let its windows go and its pipe to vanish with its process
  before the app quits; the relaunched
  app installs before it starts a Server, and the new Server resumes the
  Threads.
- ADR 0023's gate, "only the Ferrite holding the Store's claim updates",
  has no holder once the Server owns the Store: the app instead takes the
  same kind of lock on `~/.ferrite/update.claim` for its lifetime, and
  only its holder checks, downloads and installs. It sits beside the
  staging directory `update/`, which a download replaces whole.
- An app older than its Server (two installs, a downgrade) attaches; the
  contract allows it. It never asks for a restart.

### Failures

| Event | Before | With the Server |
|---|---|---|
| App quit or crash | All agents stop | Agents keep running; Notices wait for the next window |
| Client stalls | n/a | Closed as `lagged`, rebuilt from `history` (or `seq` if the ring still covers it) |
| Server crash | n/a | launchd (or the next app launch on Windows) restarts it; open Threads resume; the cut-off turn reads interrupted; launchd kills the old process group, a detached Server's CLIs exit on stdin EOF, and the Windows Job Object kills its tree |
| Logout, reboot | All agents stop | Flushed at loginwindow's point of no return; the LaunchAgent resumes them at login |
| Answer races | n/a | First valid wins; others `already_answered`; never retried |
| Ferrite update | Agents stop | Restart when idle (or now), then resume |

## Migration

Each step is one commit and leaves the app working.

1. This ADR, and **Server** in `CONTEXT.md`.
2. `Input::Revived` closes an open turn as interrupted (transcript only).
3. The pump flushes on Decision requested and answered (today's
   `Cockpit`, so it is true before the Server exists).
4. `protocol`: gen-1 types, conversions, `Unknown` fallbacks, frozen
   fixtures and their tests. Nothing uses it yet.
5. The production spawner and login-environment adoption move from the app
   to core, unchanged.
6. `engine` carved out of `Cockpit` in-process: `Cockpit` owns an `Engine` and
   calls it directly. A move; behaviour and API unchanged.
7. `Cockpit` becomes a mirror: the Engine emits protocol messages into an
   in-process outbox, and `Cockpit` builds each open Thread's Activity and
   `ThreadState` from them; every Cockpit test asserts the mirrors equal
   the Engine after each message. Still one process, and an act is still
   a direct call: the same acts become `command`s when the socket link
   carries them (step 9). Each Engine input's instant is rounded up to
   the millisecond the wire carries, so a mirror applies exactly it.
8. `link`: framing, outbox lanes, the Unix socket and the named pipe,
   handshake, replay ring, reconnect.
9. `server::run` and the `ferrite server` mode: claim, listen, pump loop,
   signals, the loginwindow watcher, `stop`. Not yet used by the app.
10. The app attaches: `ensure_running` (detached start), the socket `Link`,
    the no-Server band, ⌘Q without a question, Settings › Server status
    and Stop. Read-only Store handling leaves the app.
11. The LaunchAgent and the Windows Run key: "Start at login",
    install/uninstall.
12. Upgrade: version and executable check at `welcome`, `restart
    {when}`, self-`exec` (`--after` on Windows), the update band, the
    update claim; ADR 0023's Windows install waits for `stop {when}`.

## Test plan

Never the `live_*` tests. Scripted Sessions throughout.

- **Contract** (`protocol`): every gen-1 message and every `live` input
  kind has a fixture under `protocol/fixtures/gen1/`; each decodes, and
  re-encodes byte for byte. Fixtures are append-only: a gen-1 fixture is
  never edited, and a new optional field gets a new fixture beside the old
  one, which must still decode. Tolerance fixtures: unknown fields, an
  unknown enum value (`Unknown`), an unknown message type (ignored), an
  unknown command (`unsupported`), absent optional fields, a newer
  generation (`unsupported_generation`).
- **Mirror parity** (`engine` + in-process link): over every scripted
  scenario the Cockpit tests already run (turns, Subagents, Decisions,
  Handover, queue, interrupts), the mirror's Activity and `ThreadState`
  equal the Engine's after every pass; and in debug builds the in-process
  link encodes and decodes each message, so nothing escapes the contract.
- **Replay**: disconnect mid-turn and resume after `seq` — the transcript
  equals an uninterrupted watcher's; a different `boot_id`, or a `seq`
  past the ring, rebuilds from `history` to what a reopened Thread shows
  plus the `live` state (connection, pending Decisions); a stalled
  reader is closed `lagged` and recovers; deltas coalesce without loss.
- **Decisions**: two clients answer one Decision — one `ok`, one
  `already_answered`, both see `decision_closed`; an invalid answer leaves
  it pending; an answer in flight across a disconnect is not resent; an
  answer to a replaced Session's Decision is refused even when the new
  Session asks with the same request id.
- **Durability** (`store` durability tests): after a `DecisionRequested`
  with no turn end the log holds everything before it; a log ending
  mid-turn revives as interrupted; SIGTERM-path shutdown leaves open
  markers and flushed writers.
- **Lifecycle**: socket mode 0600, stale socket removed, live socket
  refused, foreign uid refused; a second Server exits on the claim; the
  binary smoke test starts `ferrite server` on a temporary
  `FERRITE_STORE`, says hello, gets a snapshot, sends `stop` and sees exit
  0; the idle predicate and the restart sequence with the relaunch
  injected, including the executable check and once-per-run rule; a
  second Server exits 0; the plist is snapshot-tested (no `launchctl` in
  tests).
- **Windows**: the pipe, the Run key and the detached start are
  `cfg(windows)`; Windows CI runs the link tests on the pipe.
- **App**: the existing `CockpitView` tests run unchanged over the
  in-process Engine; one test starts a window with no Server and sees the
  band, and that quitting asks nothing. A Cockpit attached over the socket
  to a Server with scripted Sessions (server tests) opens, sends, reads the
  stream, registers a Project, answers a Decision a second window then
  finds answered, and parks.
- **By hand** before step 12 ships: quit mid-turn and relaunch; `kill -9`
  the Server mid-turn; log out and in; update with a working agent, both
  "when idle" and "now".

## Consequences

- Transcripts are held twice, by the Engine and by each window's mirror.
  Acceptable at today's sizes (ADR 0008 trims both); a leaner Engine model
  is a later optimisation, not a contract change.
- The window no longer owns the Store, so every write it made is now a
  command; a client cannot corrupt a log.
- Notices still reach the operator only through a window: with none open
  they wait in the bell. A system notification from the Server is a
  separate decision.
- The Server is a GPUI-linked executable that never opens a window; its
  memory is measured in step 9's smoke test.
- Visuals need an attached window to be drawn; the agent is told so
  rather than kept waiting.
