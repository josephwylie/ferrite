---
status: accepted
date: 2026-10-16
supersedes: 0030
---

# Projects: herdr-projects 1.0 vendored

Replaces ADR 0030's vendoring of herdr-projects 0.2.34. Builds on the
Server (ADR 0025) and its API and CLI (ADR 0027).

## Context

herdr-projects 1.0 is a rewrite, not an update of 0.2. Where 0.2 had
projects, each a coordinator with worker threads, 1.0 has one board: a
list of tasks, each one delegated to an ordinary agent, carried through
review, the finish line and landing by a keeper, with a Coordinator to
talk to, automations and other machines. Upstream sets 0.2 aside rather
than converting it. It is MIT licensed (Copyright (c) 2026 Elias
Stravik) and split into crates stacked one on another: `core` (the data,
the agents, and herdr behind one module, `herdr/`), `flow` (the flow of
a task), `board` (the board's screen) and `web` (the board in the herdr
web UI), under a command line crate (`src/`, `tests/`).

The board is built Elm-style: one `State` (`board/state.rs`), one
`update(state, event) -> effects` (`board/update/`) that reads no file
and asks nothing of herdr, a view (`board/view/`) that lays the screen
out in the board's own words and named inks with its give-way rules,
and a terminal runtime (`board/run/`) that reads keys, runs jobs and
draws the view with ratatui. Only `run/` is terminal code.

## Decision

### Vendored verbatim

At upstream commit `c019063b65b3ff6a68688345b9f304533c8fb83c`
(`v1.0.1-8`):

| Upstream | Here | Package (lib) |
|---|---|---|
| `crates/core` | `crates/ferrite-projects-core` | `sw-core` (`sw_core`) |
| `crates/flow` | `crates/ferrite-projects-flow` | `sw-flow` (`sw_flow`) |
| `crates/board` | `crates/ferrite-projects-board` | `sw-board` (`sw_board`) |
| the command line (`src/`, `tests/`) | `crates/ferrite-projects` | `herdr-projects` (`sw`), its binary `ferrite-projects` |

Each keeps upstream's package and library names (no Ferrite crate
clashes with them, so their manifests and every `use` stay upstream's),
its edition (2024), `LICENSE` and formatting (`rustfmt.toml` turns
formatting off). The dependencies upstream's workspace declares are
declared in Ferrite's root manifest as upstream declares them.

The board is taken whole, `run/` too: ratatui is a dependency of `core`
and `flow` anyway (herdr's palette and the board's theme are ratatui
colours), so gating the terminal runtime behind a feature would buy
nothing and cost edits in vendored files. Ferrite never calls
`open_board`.

Every behaviour, rule, wording, threshold and test of these crates is
kept, and their unit tests run in CI. Inside vendored files a change is the
smallest that works and is marked `Ferrite:`, so an upstream update is a
three-way merge with few conflicts; seams are clean Ferrite code.

### Not taken: the web crate

`crates/web` serves the board inside herdr-web-ui, a herdr plugin that
puts herdr's panes in a browser. Ferrite has no herdr-web-ui and no web
front for its windows; its board is a GPUI view (Seam 3). `flow` keeps
its own small `webui/` module (finding the web UI, its link, its front
door), which finds no web UI under Ferrite and so does nothing:
`ferrite-projects web-ui` answers as upstream does where no web UI is
installed, and `serve` opens no front door.

### The command line

Upstream's command line crate (`src/`: `cli/`, the `sw` library that
re-exports every crate, and the docs generator) and its tests (`tests/`,
one program over `sw`) are taken verbatim into `crates/ferrite-projects`,
keeping the package and library names (`herdr-projects`, `sw`), edition
2024 and formatting. Its binary is named `ferrite-projects`; it is what
agents and hooks run, and every command, answer and refusal is
upstream's. `sw` re-exports `sw_board` in place of `sw_web`. Upstream's
text keeps its own name, `herdr-projects`: it has no one name constant
to set at a seam.

What differs, each at a `Ferrite:` mark (`crates/ferrite-projects/README.md`
lists them):

- `install` and `update` answer that Ferrite ships the program and
  updates it with the app, and change nothing; `doctor` still takes over
  older installs and says where things stand.
- `take-over`, which Ferrite's Server runs (below), says once what the
  take-over at the start of every command did.
- `store::files::bin`, the program handed to agents and hooks, is the
  shipped `ferrite-projects` (beside the app's `ferrite`, links resolved,
  or `FERRITE_PROJECTS_BIN`) when the code runs inside the app.
- The tests of the web board and of upstream's own repository tooling
  (`scripts/`, the plugin's `bin/run.sh` and `herdr-plugin.toml`, the
  source guard) are not taken; a test that also checked the web board
  keeps its terminal part.

The binary ships in the app bundle's `Contents/MacOS`, beside `ferrite`
(`scripts/install-app.sh`, `cargo build -p herdr-projects --bin
ferrite-projects`), and is signed with it.

### Seam 1: the herdr adapter

`core`'s `herdr/` keeps every type and public signature (`Herdr`,
`HerdrError`, `HRes`, `cli::*`, `socket::*`). Two functions reach herdr:
`cli::launch` runs every herdr command and `socket::ask` sends every
request on herdr's socket. After a test's stand-in (`set_fake`) has had
its turn, both now go to `herdr/ferrite.rs`, which answers each call in
herdr's words from Ferrite's Server through the `ferrite` CLI (ADR
0027) and refuses an unwired one with the code `not_wired` before
anything runs, which callers treat as any refusal of herdr's. A refusal
is retired by wiring it, never by working round it in a caller.

How each herdr call is answered:

| herdr call | Ferrite |
|---|---|
| `workspace list`, `get` | `project.list`, `project.get` (a worktree's Space is its path, from `thread.list`) |
| `workspace create --cwd --label` | `project.open`, staging the Space's first Thread |
| `workspace create … --env` | **added**: `env` on `thread.start` |
| `workspace rename`, `close` | `project.rename`, `project.close` |
| `workspace focus` | **added**: `project.focus` |
| `tab create`, `pane split`, `worktree open` | a staged Thread, started by `agent start` |
| `worktree create` | `worktree.create`, then a staged Thread there |
| `worktree remove --workspace` | `worktree.remove` (never forced) |
| `tab list`, `pane list`, `agent list` | `thread.list` |
| `tab get`, `pane get`, `agent get` | `thread.get` (its Attention is the agent's status) |
| `tab rename` | `thread.rename` |
| `tab close`, `pane close` | `thread.close` |
| `tab focus`, `agent focus`, `pane zoom` (focus) | `thread.focus` |
| `agent start` | `thread.start --ready` (`--args`, `--resume`, `--project`) |
| `pane quit` then `agent start` in that pane | **added**: `thread.revive`, a parked Thread's agent started again in place with new arguments, so its id lasts as a pane's does |
| `agent prompt` | `agent.prompt --until working --until blocked` |
| `agent read`, `pane read` | `agent.read --lines` |
| `agent send-keys` | `agent.interrupt` (Esc, Ctrl+C), `decision.answer` (a menu's keys) |
| `pane send-text` | `decision.answer` (a typed answer) |
| `pane process-info` (`pane_idle`, `pane_childless`) | answered from the staged Thread: no shell runs |
| `pane run` (a plan writer under bubblewrap) | **added**: a launcher prefix on `thread.start`, needed only on Linux machines with `bwrap` |
| `pane report-metadata`, `workspace report-metadata` | none: the nav draws its rows from the board's data |
| `pane move` (a coordinator from before it had a tab) | refused: no Ferrite install has that layout |
| `integration status` | none: Ferrite's Providers report Attention themselves |
| `machine list` | `machine.list` |
| `plugin list`, `config-dir`, `disable` | none: the web UI and herdr-superwork, neither of which Ferrite has |
| `plugin pane open`, `pane layout`, `terminal session control`, `popup.close`, `tab.get`, `events.subscribe` | none: the board's terminal runtime, replaced by Seam 3 |
| `--default-config`, `config check`, `server reload-config`, `agent attach` | none: herdr's first-run setup and attaching from a plain terminal; Ferrite's keymap binds the board |

An agent kind Ferrite has no Provider for (only `claude` and `codex`
run) fails `no_provider` at `agent start`, as a missing program does.
`socket::on_tab_shown`, `flow`'s `extras/config.rs` and `board`'s
`job_far.rs` start herdr themselves; only the terminal runtime and
herdr's first-run setup reach them, and Ferrite runs neither.

### Seam 2: the API grows what the board needs

What 1.0 needs and Ferrite lacks is added to the API (ADR 0027):
declared once in the `operations!` table so the CLI binding, catalog and
schema follow, with a v1 fixture. The additions are those marked above:
`env` and a launcher prefix on `thread.start`, `project.focus` and
`thread.revive`.

### Seam 3: the board in GPUI

The board is a GPUI view that replaces `board/run/` and nothing else.
It owns the vendored `State`, turns GPUI's keys, mouse, a one-second
tick and the loader's reads into the board's `Event`s, does the
`Effect`s `update` answers (each job on the background executor, its
answer fed back as an event, the state saved), and draws what `view/`
lays out: the list under its headings (`needs you`, `ready to approve`,
`working`, `to do`, `done`) with a mark per row and its columns right
aligned, the question line under a row that asks, the question box, the
chat at the bottom, the text box, the hint line, the `?` menu, and the
task, automation and settings panels. `view/` stays the authority on
what each part says and what gives way when room is short: GPUI draws
its lines in Ferrite's monospace font and theme, an ink per named ink,
so the board matches 1.0's design exactly. Its first-run setup (herdr's
config) and its size warning (a pane shared with a remote view) are
herdr's and are not drawn.

herdr's sidebar rows become Ferrite's nav rows, drawn from the same data
(`flow`'s `flow/sidebar*.rs`). The key that opens the board, herdr's
`prefix+a`, is Ferrite's existing `projects::Toggle`; the board's own
keys are its own, as in the terminal.

### Migration from 0.2

1.0 never converts 0.2: its take-over (`sw_flow::take_over::at_start`,
which runs `take_over` with `take_over::live::Live`) moves 0.2's projects
root, `~/.herdr-projects`, whole to `~/.herdr-projects-legacy` and its
settings, `~/.config/herdr-projects`, to `-legacy` beside it, after
stopping its ticker, and takes 0.2's hooks and `autoproject` skill links
out of the agent programs' settings, backing each file up first. Ferrite's
0.2 kept upstream's root and settings folders, so the same take-over
applies; Ferrite's own names for what 0.2 wrote (the `ferrite-projects`
hook command and Copilot's `hooks/ferrite-projects.json`, the
`ferrite-owned.json` journal) are recognised at a `Ferrite:` seam beside
upstream's.

Ferrite's 0.2 kept its projects in that same root and its settings in that
same folder, with its own journal, `ferrite-owned.json`; its hooks ran
`ferrite-projects … hook --agent …`, Copilot's in `hooks/ferrite-projects.json`.
Those names are recognised at `Ferrite:` marks beside upstream's (`legacy.rs`
and `take_over/outside.rs`), so the same take-over sets Ferrite's 0.2 aside:
nothing deleted, nothing converted, each file it changes backed up first.

It runs where `migrate-ferrite-coordinators` ran (ADR 0030): at every
Server start, before the keeper, as `ferrite-projects take-over`. As any
command does, it first runs upstream's start (`take_over::at_start`, then
the home folder's move, the copy of older data and the settings' move);
then it prints the line upstream keeps for the human until it is said
(`move_note`, `move_told`), so the take-over is done once and said once.
The Server logs what it prints and shows it as one toast once a window is
there (`server/projects.rs`). Between the two, the Server runs the
startup hook that starts the keeper (Seam 4). ADR 0028's `coordinators.json` and
`ferrite_migrate.rs` leave with the 0.2 crate: neither 0.2 nor ADR 0028
ever shipped, and the projects that migration made are set aside with
the rest of 0.2's.

### Seam 4: the keeper and hooks under the Server

herdr runs herdr-projects by its plugin manifest (`herdr-plugin.toml`):
`hook` at startup and on `pane.agent_status_changed`,
`pane.agent_detected`, `pane.closed` and `worktree.removed`. The Server
plays herdr's part (`server/projects.rs`), with the shipped
`ferrite-projects` (beside the executable, or `FERRITE_PROJECTS_BIN`;
none there, Projects are off and the Server says so in its log):

| herdr | the Server |
|---|---|
| `[[startup]]` | `hook`, `HERDR_PLUGIN_EVENT=startup`, once `take-over` returned |
| `pane.agent_detected` | a Thread opened |
| `pane.agent_status_changed` | a Thread's Attention changed (`idle`, `working`, `done`, `blocked`) |
| `pane.closed` | a Thread closed (parked or deleted) |
| `worktree.removed` | the Engine removed one: `worktree.remove`, or a deleted Thread's own unregistered tree (`Engine::take_removed_worktrees`) |

At Server start, on the `projects` thread, in this order: `take-over`
(Migration from 0.2), then the startup hook, waited for, then the
take-over's toast. Every command of the program takes over at its start
(`take_over::at_start`, which holds no lock), so nothing runs it beside
another: the startup hook only once `take-over` returned, and no event's
hook until the startup hook returned. The events before that run no
hook; the startup hook's housekeeping and the keeper's ticks look at
every task, as they do after a missed hook under herdr.

Each event is the API's own (`Api::pump` answers what it told its
subscribers), handed over in herdr's words: `HERDR_PLUGIN_EVENT`, and
`HERDR_PLUGIN_EVENT_JSON` as `{event, data}` with `pane_id` a Thread's
id, `workspace_id` its Project's (a worktree Thread's, its worktree's
path, as Seam 1 names a worktree's Space) and `agent` its Provider. The
fixtures in `server/fixtures/projects-hook/` hold both sides to it: the
Server's test writes them, and `sw-flow`'s `tests/ferrite_hook_events.rs`
feeds them to upstream's `hook`. The Server never waits for an event's
hook; each is reaped once it ends.

The keeper is upstream's, untouched. The startup hook starts it, as
every hook does when no keeper beats; one runs per machine (its own
lock and heartbeat); it ticks with the board open or closed; it ends
once this Server is gone (`keeper-work` finds herdr's socket, here the
Server's API socket, gone or not answering three times); and it hands
over to the new program once an update replaces the bundled one, as
upstream does. A Server restart restarts it: the old keeper ends after
three failed checks of the socket (about three seconds), and while it
still beats the new Server's startup hook skips, so the Server runs the
startup hook once more ten seconds after the first returned, past the
old one, and that starts a fresh keeper with this Server's environment.
A restart quicker than those checks keeps the old keeper, which answers
the same socket with the same program and the same environment.

Until the herdr adapter answers `Herdr::alive` (Seam 1), `keeper-work`
finds the socket refusing, ends after its three checks and exits 3, and
the next hook starts it again at most every four seconds
(`due("keeper-start", 4000)`): the keeper keeps running only once the
adapter does.

What the hook, the keeper and everything they start are handed:
`HERDR_SOCKET_PATH`, the Server's API socket, which every task's link
names as herdr's socket; `FERRITE_SOCKET` and `FERRITE_BIN` for the
`ferrite` CLI; and `FERRITE_PROJECTS_BIN`, the program, which
`store/files.rs`'s `bin()` returns, so the launcher
(`data/bin/herdr-projects`) and the turn-end hooks it writes run the
bundled program (`server::projects::environment`). Every Session's
agent, while Projects run, is told herdr's pane environment
(`server::projects::pane_environment`, through
`api::caller_environment`), as herdr sets it in every pane:
`HERDR_PANE_ID`, its Thread's id, the same `pane_id` its events' hooks
send, with `HERDR_SOCKET_PATH` and `FERRITE_PROJECTS_BIN`. With the
first two, upstream's `cli::caller()` knows a task's own agent (or
reviewer): its `report --state blocked` is its own (`selfBlocked`, and
the "Reported: you are waiting" reply), and what it adds is from its
task. Without `HERDR_SOCKET_PATH`, what an agent or the Coordinator
delegates would link herdr's default socket, which the hook's
`mine(socket)` and the keeper's `board_for(socket)` never match. All
three are caller context: a Session never inherits them from whatever
started Ferrite (`spawn::INHERITED_MARKERS`). Ferrite's app sets
`HERDR_SOCKET_PATH` in its own process before the board runs (the
board's lane), or a task the board makes links herdr's default socket
too. `HERDR_PLUGIN_ID` is not
set: upstream's `login_env` (asking a login shell for `PATH` and gh's
login) runs only under herdr. The Server already took the login shell's
environment when launched from the Finder (`login_shell`), and these
processes inherit it, so `gh`, `git`, `claude` and `codex` resolve and
gh's login is there without anything being written down.

The turn-end hooks reach Ferrite's Providers through `thread.start
--args` unchanged: Claude's `--settings` file (`Stop`, `StopFailure`,
and `PreToolUse` on `ExitPlanMode` for a plan) follows Ferrite's own
arguments, with herdr's pane environment for the program it runs
(`tests/claude_session.rs`,
`projects_settings_reach_the_cli_after_ferrites_flags`), and Codex's
`-c notify=[…]` goes to its app-server as written
(`providers::codex::launch` tests). That the CLIs then run them is
theirs and is checked only by a live run: Claude's hooks in its
stream-json print mode, where Ferrite also sets an effort through
`apply_flag_settings`, the same flag layer `--settings` fills, and
Codex's `notify` under the app-server rather than its terminal. Until a
live run shows both, the keeper's tick and the hook's
`agent_status_changed` (a Thread at `done` or `idle`) still find a
turn's end, as upstream does when a hook is missed.

A Thread parked is `pane.closed`, and the hook then reconciles its task,
which may find it gone. Under herdr, the fallback hand-over's `pane
quit` keeps the pane, so `thread.revive` (Seam 1) must keep the Thread
open across the hand-over, or `pane get` must answer for a parked
Thread, or a hand-over in two calls marks its task's agent gone mid-way.

What proves each flow behaviour, through Ferrite or still only against
herdr's fake, and what waits on another seam, is listed in
`crates/ferrite-projects-flow/README.md`.

`board.notify` stays upstream's: `bell` is the board view's to ring, a
command runs as upstream runs it, and nothing raises a notification with
the board closed, as upstream raises none. The take-over's toast is the
Server's, not the keeper's (Migration from 0.2).

### Upstream updates

The vendored commit is recorded in each crate's `README.md`. For each
crate, apply upstream's diff from that commit to the new one
(`git diff c019063..<new> -- crates/core | git apply --3way -p3
--directory=crates/ferrite-projects-core`; for the command line,
`git diff c019063..<new> -- src tests | git apply --3way
--directory=crates/ferrite-projects`, leaving out what touches only the
web crate's tests), resolve conflicts at the `Ferrite:` marks, answer
(or refuse) any herdr call upstream starts making in `herdr/ferrite.rs`
before it ships, record the new commit, and run the crates' tests
(`cargo test -p sw-core -p sw-flow -p sw-board -p herdr-projects`).

## Consequences

- Ferrite carries ratatui and crossterm while the vendored board does;
  only `run/` uses them beyond colour types.
- The workspace's `serde_json` gains `float_roundtrip`, as upstream's
  data needs (a closing note's time read back is the time written);
  `preserve_order` was already unified in.
- The 0.2.34 crate's binary, its Server glue (the coordinators'
  migration, the ticker) and ADR 0030's terms in CONTEXT.md are gone. Its
  library stays, in `crates/ferrite-projects-0-2`, only while the window's
  0.2 Projects view still draws from it, and leaves with that view.
  That view runs 0.2 commands the shipped 1.0 program refuses, and its
  "configure" writes 0.2's hooks, skill link and journal, which the next
  Server start sets aside into a further `-legacy` folder. So the 1.0
  program lands with the view that replaces it, never before: the 0.2
  library, its `configure` and its tests go in the same merge.
- The startup hook, which starts the keeper, runs after `take-over` has
  returned, on the same thread, and the events' hooks only after it
  (`server/projects.rs`): every command takes over at its start, and two
  at once would race over the 0.2 folders.
- Every Thread event runs a short process, as every pane event does
  under herdr; the hook keeps its own housekeeping to once in two
  seconds, and where no task was ever made only sees that a keeper
  runs. So a keeper runs on every machine with a Server, as on every
  machine with the plugin, idle while there are no tasks.
