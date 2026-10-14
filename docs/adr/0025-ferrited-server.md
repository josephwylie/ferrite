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
| `host` | What `Cockpit` does today minus the window: the Store and its claim, every Session, writers, the queue, pending Decisions, Notices, Groups, the registry, the shared half of the Roster, and every policy (Handover, sweep, stale-CLI restart, titler, suggestions, deferred finish). Emits protocol messages into per-connection outboxes; takes protocol commands. Single-threaded, the pump's thread. |
| `link` | Transport: newline-delimited JSON framing, the per-connection two-lane outbox, the Unix socket and Windows named pipe, the handshake, reconnect with replay, and an in-process pair for tests and `--demo`. |
| `server` | `run`: adopt the login environment, open diagnostics (`logs/server.log`), claim the Store, build `Host` with the production spawner, listen, watch signals and macOS logout, drive the pump loop, restart itself. Also `agent`: LaunchAgent plist and `launchctl` calls, the Windows Run key, and `ensure_running` for the app. |
| `cockpit` | The client. Keeps its public API so `CockpitView` changes little; acts become protocol commands on a `Link`, and each Thread is a mirror (Activity + `ThreadState`) fed from the stream. Keeps the per-window Roster and Drafts. |

The production `Spawner` (`ferrite/src/session.rs`) and the login-shell
environment (`ferrite/src/shell.rs`) move to core, since the Server spawns.
The scripted demo spawner stays in the app; `--demo`, `--load` and every
app test run `Host` in-process over the in-process link.

### Process lifecycle

**Where it listens.** `~/.ferrite/server.sock` (the directory 0700, the
socket 0600), refused to any peer whose `getpeereid` uid is not ours. A
stale socket (nothing answers a connect) is removed before binding; a live
one means a Server already runs. On Windows, the pipe
`\\.\pipe\ferrite-server-<user SID>`, created with
`FILE_FLAG_FIRST_PIPE_INSTANCE`, `PIPE_REJECT_REMOTE_CLIENTS` and a DACL
granting only the user; the client checks the pipe server's process owner
is itself before sending anything. Both use blocking I/O and a thread per
connection direction, like the rest of core: no async runtime.
`FERRITE_STORE` moves the socket with the Store so tests and a second
install never meet the operator's Server.

**Starting.** The app calls `server::agent::ensure_running`: connect; if
nothing answers, on macOS `launchctl kickstart gui/<uid>/<label>` when the
LaunchAgent is installed, otherwise (and always on Windows) start
`<current_exe> server` detached: a new process group with null stdio on
Unix, `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW |
CREATE_BREAKAWAY_FROM_JOB` on Windows. Then it waits up to 5 s for the
socket. The Store's claim is the single-instance guarantee: a second
Server that cannot claim it exits before binding.

**LaunchAgent (macOS).** Settings › Server › "Start at login", on by
default, installed at the first launch of a bundled app (not for a bare
`target/` binary, which uses the detached start). It writes
`~/Library/LaunchAgents/com.github.josephwylie.ferrite.server.plist` —
`ProgramArguments` = the bundle's executable and `server`, `RunAtLoad`,
`KeepAlive { SuccessfulExit = false }` (restarted after a crash, not after
a clean stop), no `ProcessType` (Background would throttle the agents'
builds) — then `launchctl bootstrap gui/<uid>`. Turning it off runs
`launchctl bootout` and deletes the plist; the running Server is left
alone. Each launch rewrites the plist if the bundle moved. A LaunchAgent
runs in the login session, so `claude`/`codex` see the operator's
keychain. `SMAppService` was rejected: it wants a signed bundle and an
Objective-C bridge, and releases may be unsigned.

**Windows.** The same setting writes or removes
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run\FerriteServer` =
`"<exe>" server`. A crashed Server is restarted by the next app launch or
reconnect; there is no supervisor.

**Running.** The pump loop owns `Host` on one thread. It waits on the
command channel with a 16 ms timeout while any Session is live, or
indefinitely when none is. Each pass applies commands in arrival order,
drains Session events exactly as `Cockpit::pump` does now, and pushes
messages into outboxes. At boot it revives every Thread marked open (the
existing `.open` markers, staggered as in ADR 0018), so after a crash or
reboot the Threads resume without a window.

**Stopping.** SIGTERM and SIGINT (and `CTRL_CLOSE_EVENT` /
`CTRL_SHUTDOWN_EVENT` on Windows) set a flag the pump sees. The pump flushes
every writer, ends every Session without parking it (the open markers
stay, so the next boot resumes the same Threads; resume ids are already in
the log as `init` records), and runs `Store::quit` with the existing
deadline. On macOS a thread registered with `notify(3)` for loginwindow's
`logoutNoReturn` / `shutdownNoReturn` does the flush first, because
loginwindow kills the provider CLIs 15–20 s before launchd's SIGTERM
(after Herdr's `platform/macos/shutdown.rs`; the keys are undocumented, so
a missing key falls back to SIGTERM). After that point the Host freezes:
Sessions that close are not recorded as ending or restarted. `stop` from a
client does the same, then exits 0.

### Durability

- The Host flushes a Thread's writer at turn end (as now), when a Decision
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
`welcome`, then `snapshot`. The automation API and CLI (plan §2) will
share the listener with another `role` in `hello`; nothing here depends
on it.

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
| `command {id, op, …}` | One per act that changes shared or durable state: `open`, `bootstrap` (a Draft's first prompt), `send`, `queue`, `unqueue`, `cancel_queued`, `interrupt`, `answer {thread, decision, answer}`, `control`, `set_model`, `set_effort`, `set_provider`, `rename`, `park`, `revive`, `delete`, `import`, `group`, `layout`, `project`, `notice` (open, dismiss, read, seen), `settings_changed`, `search_files`, `restart {when}`, `stop`. Each gets exactly one `reply`. |
| `rendered {request, png?, error?}` | The answer to a `render`. |

Server → Client:

| Message | Lane | Meaning |
|---|---|---|
| `welcome {generation, server:{version}, boot_id, capabilities[], error?}` | — | `error.code = unsupported_generation` when the generations do not meet; the Server then closes. |
| `snapshot {boot_id, revision, threads[], roster, decisions[], groups?, registry?, notices?, providers?}` | — | Everything a window draws before any transcript. `roster` is the shared half: open Threads in order and the park order. |
| `history {thread, after, inputs[]}` … `history_end {thread, seq}` | reliable | A watched Thread's log, read off the pump thread up to a writer `checkpoint`; live inputs continue from `seq`. |
| `input {thread, seq, at_ms, input}` | reliable | One `ActivityInput` the Host applied, in order. |
| `decision {thread, subject?, decision}` / `decision_closed {thread, decision, by}` | reliable | `by`: `answered {client}`, `cancelled`, `session_ended`, `server_restart`. |
| `reply {id, ok?, error?:{code, message}}` | reliable | Codes: `already_answered`, `invalid_answer`, `unknown_thread`, `unsupported`, `restarting`, `failed`. |
| `thread {revision, state}` | latest-wins per Thread | `ThreadState`: everything `ThreadView` reads that is not Activity — Header facts, Attention, starting/busy/working/background, queued prompts, suggestion, command and model menus, permission modes, supported controls, store error, generation. |
| `tool_draft {thread, tool, input}` | latest-wins per tool | `SessionEvent::ToolDraft`, already a snapshot of the input so far. |
| `roster`, `groups`, `registry`, `notices`, `providers` `{revision, …}` | latest-wins | Replaces the snapshot's field. |
| `render {request, …}` | reliable | A Visual to draw, sent to one `render_visual` client (the one most recently focused). With none attached the agent is answered at once: no Ferrite window is open to draw it. |
| `restarting {when}` / `stopping` | reliable | The client reconnects. |

An `input` carries either `{"record": …}`, a Store `Record` exactly as
the log spells it, or `{"live": …}`, a gen-1 DTO for what the Store never
keeps: notices, answers, revival, Activity connect/disconnect, queue
acknowledgements, MCP servers and authorizations, rate limits. Durable
history is therefore the log's own lines, and `history` is read from the
log without decoding on the Server. The Store's on-disk rule is
unchanged: a log from a newer schema is refused whole. On the wire, a
record kind this client does not know decodes to `Unknown` and is
skipped, and the window says it is older than its Server.

`seq` counts a Thread's inputs within one `boot_id`. The Host keeps the
last 8 MiB of each watched Thread's encoded inputs. A `watch` whose
`resume` names this boot and a `seq` still held replays from there; any
other starts again from `history` and the client rebuilds that Thread's
mirror. `revision` counts the Server's committed changes; latest-wins
messages carry it so a client drops one older than its snapshot.
`at_ms` is the Server's monotonic clock since boot; the client offsets it
by the difference measured at `welcome`, so working timers agree across
clients.

**Lanes.** One connection, one outbox, two queues. The reliable queue
keeps order and never drops: adjacent text, thinking and tool-output
deltas of one Thread are concatenated while they wait (lossless), and a
connection whose reliable queue passes 32 MiB is closed with
`error.code = lagged`; the client reconnects and resumes. The latest-wins
queue holds one pending message per key and replaces it in place. The
writer interleaves them, so a state change never waits behind more than
one reliable batch.

**Decisions.** The Host is their only owner. `answer` is checked on the
pump thread: the Decision must be pending, the answer must fit its kind
(`validate_form` for forms and questions), and the Session must be the
generation that asked. The first that passes is delivered to the
provider, recorded, flushed, replied `ok`, and broadcast as
`decision_closed {by: answered}` with the answering client. Every later
answer is replied `already_answered`; an invalid one `invalid_answer`,
with the Decision still pending. No one retries an answer: a client that
loses its connection with an answer in flight shows "the answer may not
have arrived", and after reconnecting draws whatever the snapshot says —
still pending means the operator answers again.

### Clients

- `Cockpit` holds a `Link` instead of a Store and a Spawner. Its mirrors
  apply exactly the `ActivityInput`s the Host applied, so the window draws
  what it draws now. `PaneUpdate` is computed from the mirror, as the pump
  does today.
- Per window: focus, View (Solo or a Group), fullscreen, Drafts, the
  Composer's prompt history, find. Shared through the Server: which
  Threads are open, Groups and Layouts, projects, Notices and whether they
  are seen (each window reports what it has on screen while focused).
- Several clients may attach. A second app process attaches instead of
  getting a read-only Store, so `Store::read_only` and its banner leave
  the app. Several windows in one app are not built here.
- Quitting or closing the window ends nothing, so ⌘Q no longer asks.
  Settings › Server shows the Server's version, uptime and live Sessions,
  with "Start at login", "Restart" and "Stop" (stop asks first while an
  agent works or a Decision waits — the question ⌘Q asks today).
- Without a Server the window shows a band with the reason and Retry, and
  keeps reconnecting with backoff. It never falls back to running agents
  itself: two pumps on one Store is the bug the claim exists to stop.
- Settings stay a file the window writes. `settings_changed` makes the
  Host reread it; it applies to the next Session, as today.

### Upgrading the Server

The plan's decision stands: restart when idle and resume, never hand
over live processes.

- `welcome` names the Server's version. When the app's is higher
  (semver; equal versions never restart on their own — a developer uses
  Settings › Server › Restart), the app sends `restart {when: idle}` and
  shows "Ferrite's server will update when your agents are idle ·
  Restart now". "Restart now" sends `restart {when: now}`.
- Idle is the predicate `restart_idle_stale` already uses for a stale CLI:
  for every Thread no turn, no working Subagent or background task, no
  Decision and nothing queued. New prompts are not blocked while a
  restart waits.
- On idle (or now), the Host broadcasts `restarting`, closes pending
  Decisions (`server_restart`), flushes and ends Sessions as on SIGTERM,
  then restarts itself: `exec` of its own executable on Unix (same pid,
  so launchd sees no exit), a detached start and exit on Windows. The new
  Server revives the open Threads (`claude --resume`, `thread/resume`);
  clients see a new `boot_id` and reload their mirrors. A turn cut off by
  "now" reads as interrupted.
- With ADR 0023: on macOS the installer replaces the bundle under a
  running Server, and the restart above follows. On Windows the
  installer cannot replace a running executable, so the at-launch install
  sends `stop` first and waits for the Server to stop on its own when
  idle; the What's New sheet says the update waits for the agents.
- An app older than its Server (two installs, a downgrade) attaches; the
  contract allows it. It never asks for a restart.

### Failures

| Event | Before | With the Server |
|---|---|---|
| App quit or crash | All agents stop | Agents keep running; Notices wait for the next window |
| Client stalls | n/a | Closed as `lagged`, resumes from `seq` |
| Server crash | n/a | launchd (or the next app launch on Windows) restarts it; open Threads resume; the cut-off turn reads interrupted; provider CLIs exit on stdin EOF, and the Windows Job Object kills its tree |
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
6. `host` carved out of `Cockpit` in-process: `Cockpit` owns a `Host` and
   calls it directly. A move; behaviour and API unchanged.
7. `Cockpit` becomes a mirror: the Host emits protocol messages into an
   in-process outbox, `Cockpit` applies them and sends commands back.
   Still one process.
8. `link`: framing, outbox lanes, the Unix socket and the named pipe,
   handshake, replay ring, reconnect.
9. `server::run` and the `ferrite server` mode: claim, listen, pump loop,
   signals, the loginwindow watcher, `stop`. Not yet used by the app.
10. The app attaches: `ensure_running` (detached start), the socket `Link`,
    the no-Server band, ⌘Q without a question, Settings › Server status
    and Stop. Read-only Store handling leaves the app.
11. The LaunchAgent and the Windows Run key: "Start at login",
    install/uninstall.
12. Upgrade: version check at `welcome`, `restart {when}`, self-`exec`,
    the update band; ADR 0023's Windows install waits for `stop`.

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
- **Mirror parity** (`host` + in-process link): over every scripted
  scenario the Cockpit tests already run (turns, Subagents, Decisions,
  Handover, queue, interrupts), the mirror's Activity and `ThreadState`
  equal the Host's after every pass; and in debug builds the in-process
  link encodes and decodes each message, so nothing escapes the contract.
- **Replay**: disconnect mid-turn and resume after `seq` — the transcript
  equals an uninterrupted watcher's; a different `boot_id`, or a `seq`
  past the ring, rebuilds from `history` to the same result; a stalled
  reader is closed `lagged` and recovers; deltas coalesce without loss.
- **Decisions**: two clients answer one Decision — one `ok`, one
  `already_answered`, both see `decision_closed`; an invalid answer leaves
  it pending; an answer in flight across a disconnect is not resent.
- **Durability** (`store` durability tests): after a `DecisionRequested`
  with no turn end the log holds everything before it; a log ending
  mid-turn revives as interrupted; SIGTERM-path shutdown leaves open
  markers and flushed writers.
- **Lifecycle**: socket mode 0600, stale socket removed, live socket
  refused, foreign uid refused; a second Server exits on the claim; the
  binary smoke test starts `ferrite server` on a temporary
  `FERRITE_STORE`, says hello, gets a snapshot, sends `stop` and sees exit
  0; the idle predicate and the restart sequence with the relaunch
  injected; the plist is snapshot-tested (no `launchctl` in tests).
- **Windows**: the pipe, the Run key and the detached start are
  `cfg(windows)`; Windows CI runs the link tests on the pipe.
- **App**: the existing `CockpitView` tests run unchanged over the
  in-process Host; one test drops the link and sees the band and the
  resume.
- **By hand** before step 12 ships: quit mid-turn and relaunch; `kill -9`
  the Server mid-turn; log out and in; update with a working agent, both
  "when idle" and "now".

## Consequences

- Transcripts are held twice, by the Host and by each window's mirror.
  Acceptable at today's sizes (ADR 0008 trims both); a leaner Host model
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
