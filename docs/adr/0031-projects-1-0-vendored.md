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
| the command line (`src/`, `tests/`) | `crates/ferrite-projects` | `herdr-projects` (`sw`) |

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
2024 and formatting. Its binary is named `ferrite-projects` (upstream's
is `herdr-projects`); it is what agents and hooks run, and every command,
answer and refusal is upstream's. `sw` re-exports `sw_board` in place of `sw_web`. Upstream's
text keeps its own name, `herdr-projects`: it has no one name constant
to set at a seam.

What differs, each at a `Ferrite:` mark (`crates/ferrite-projects/README.md`
lists them):

- `install` and `update` answer that Ferrite ships the program and
  updates it with the app, and change nothing; `doctor` still takes over
  older installs and says where things stand.
- `take-over`, for Ferrite's Server to run (below), says once what the
  take-over at the start of every command did.
- `store::files::bin`, the program handed to agents and hooks, is the
  shipped `ferrite-projects` (beside the app's `ferrite`, links resolved,
  or `FERRITE_PROJECTS_BIN`) when the code runs inside the app.
- The tests of the web board and of upstream's own repository tooling
  (`scripts/`, the plugin's `bin/run.sh` and `herdr-plugin.toml`, the
  source guard) are not taken; a test that also checked the web board
  keeps its terminal part.

It ships with the app: `scripts/install-app.sh` builds it (`cargo build
-p herdr-projects --bin ferrite-projects`) and copies it into the app
bundle's `Contents/MacOS`, beside `ferrite`, signed with it; the Server
runs `ferrite-projects take-over` at start, then starts the keeper (Seam
4). The 0.2 crate, its Server glue, its CI step, the window's 0.2 view
and 0.2's nav focus (`nav.focus`) are gone.

### Seam 1: the herdr adapter

`core`'s `herdr/` keeps every type and public signature (`Herdr`,
`HerdrError`, `HRes`, `cli::*`, `socket::*`). Two functions reach herdr:
`cli::launch` runs every herdr command and `socket::ask` sends every
request on herdr's socket. After a test's stand-in (`set_fake`) has had
its turn, both now go to `herdr/ferrite.rs`, which answers each call in
herdr's words and shapes from Ferrite's Server, a request on its API
socket as the `ferrite` CLI (ADR 0027) sends it, and refuses one with no
Ferrite counterpart with the code `not_wired` before anything runs,
which callers treat as any refusal of herdr's. A refusal is retired by
wiring it, never by working round it in a caller. Three more `Ferrite:`
lines reach the seam: `cli::current_socket` is `HERDR_SOCKET_PATH` when
set, else the Server's API socket (`FERRITE_SOCKET`, else `api.sock` in
the Store), so there is one session, `default`, and `Herdr::alive` is the Server answering;
`Herdr::pane_quit` parks the pane's Thread instead of signalling a
process; and `agents/start_failed.rs` explains an agent kind with no
Provider.

A herdr Space is a Project, by its id; or a Space the board made inside
one; or a linked worktree, by its path, as herdr's worktree Spaces are.
herdr's `workspace create` always makes a new Space, where a Ferrite
folder has one Project (the nearest registered one holding it, the home
folder's for any folder under it): so a Space it makes is kept by an id
of its own (`s1`, `s2`, …) with its Project, folder and label, renamed
and closed alone, and gone with its last pane, as herdr's is. Closing or
renaming one task's Space never touches another's, the Coordinator's or
the Project. A tab, a pane and an agent are one Thread. herdr places a pane at a shell and
starts an agent in it later, and again after it ended, under the same
pane id; a Thread is born with its agent. So a pane the board places is
kept by an id of its own (`p1`, `p2`, …) in `ferrite-panes.json`, with
the Spaces the board made, beside
the board's data, with its Space, folder, tab label and `--env`, and that
id names the Thread started there from then on, and the same Thread
again when its agent is started anew. Any other Thread is a pane named by
its Thread's id. A pane whose Thread is not open is at its shell.

How each herdr call is answered:

| herdr call | Ferrite |
|---|---|
| `workspace list`, `get` | `project.list` (`repo_root` is its `repo_key`), then each Space the board made (its Project's checkout), then each linked worktree a pane is in |
| `workspace create --cwd --label --env` | `project.open` of the folder, and a new Space in that Project, labelled `--label` or after its folder (the home folder `~`, as herdr does), with a pane placed there with that `env` |
| `workspace rename` | a Project's own Space: `project.rename`; a made Space's, or a worktree's, label is its own and the Project keeps its label |
| `workspace focus` | a Project's own Space: **added** `project.focus`; any other: `thread.focus` of its first open Thread. With no open Thread there is nothing to show, and nothing moves |
| `workspace close` | `thread.close` of each Thread in a made Space or a worktree's Space; in a Project's own Space only those the board placed (a Thread of the operator's there stays open, and so does the Project) |
| `tab create`, `pane split`, `worktree open` | a placed pane (`worktree open` of a Space a pane is in answers that pane, `already_open`) |
| `worktree create` | `worktree.create`, then a pane placed in its Space |
| `worktree remove --workspace` | `worktree.remove` by the Space's path; `--force` and a Project's Space are refused |
| `tab list`, `pane list`, `agent list`, `tab get`, `pane get`, `agent get` | `thread.list` and the placed panes; an agent is a Thread, its Attention herdr's status word for word, `attention_seq` its `state_change_seq` and **added** `completion_seq` its own (absent until a turn ended, as herdr's), its `terminal_id` `<thread>@<boot>` (**added** `boot`): one per Thread while the Server runs, another after it restarts, as herdr's terminal ids and their counter are; `agent get` of a pane at its shell is `agent_not_found` |
| `tab rename` | the placed pane's label; any other Thread: `thread.rename` |
| `tab close`, `pane close` | `thread.close`, and the placed pane forgotten |
| `tab focus`, `agent focus`, `agent attach`, `Herdr::pane_focus` | `thread.focus` (a pane at its shell has nothing to land on); `pane_focus`, which zooms a pane in herdr, reaches the seam by a `Ferrite:` line, as `pane_quit` does |
| `agent start` | `thread.start --ready --timeout-ms` (within the call's deadline) (`--args`, `--resume`, `--project`, **added** `--env`); in a pane whose agent of that kind ran before, **added** `thread.revive --ready`, so the Thread keeps its id as the pane does. `claude` and `codex` only: any other kind is refused `no_provider` before anything starts |
| `pane quit` | `thread.close` (the `Ferrite:` line in `Herdr::pane_quit`) |
| `pane run` (`env HERDR_PROJECTS_AGENT=<name> bwrap … -- <program> …`) | `agent start` with the mount as **added** `--launcher`, ready within the call's deadline less a margin (so a slow start is refused `agent_not_ready` naming its Thread, which the pane keeps, rather than lost to the socket's timeout); any other command is refused. Only Linux machines with `bwrap` get here: elsewhere upstream refuses a plan in a repo itself |
| `agent prompt` | `agent.prompt --until working --until blocked`, its wait the call's deadline less a margin |
| `agent read`, `pane read --lines` | `agent.read --lines`; a pane at its shell reads empty |
| `agent send-keys` | `agent.interrupt` (Esc, Ctrl+C), `decision.answer` (a menu's keys: enter, digits, `y`, `n`); enter with nothing pending, and any key at a shell, does nothing; any other key is refused before anything is sent |
| `pane send-text` | `decision.answer` (a typed answer); at a shell, nothing |
| `pane process-info` (`pane_idle`, `pane_childless`) | answered from the pane, no process asked: its shell alone at a shell, a program in front where its Thread is open, under a pid no system gives |
| `pane report-metadata`, `workspace report-metadata` | nothing asked: the nav draws its rows from the board's data |
| `integration status` | nothing missing: Ferrite's Providers report Attention themselves |
| `machine list --json` | `machine.list`, in its order, each `session` `default`, `enabled` |
| `pane move` (a coordinator from before it had a tab) | refused: no Ferrite install has that layout |
| `plugin list`, `config-dir`, `disable` | refused: the web UI and herdr-superwork, neither of which Ferrite has (`webui/find.rs` finds no web UI, `take_over`'s `old_plugin_off` is false) |
| `plugin pane open`, `pane layout`, `pane zoom`, `terminal session control`, `popup.close`, `tab.get`, `events.subscribe` | refused: the board's terminal runtime, replaced by Seam 3. `socket::on_tab_shown` asks the seam for its subscription by a `Ferrite:` line and, refused, connects nowhere (upstream's popup test in `board_stay_keys.rs`, which would wait on herdr's answer for ever, is ignored; sw-board's `tests/ferrite_tab_watch.rs` holds the watch to asking nothing and closing nothing) |
| `--default-config`, `config check`, `server reload-config` | refused: herdr's first-run setup |

herdr's error codes come back as herdr gives them: no open Thread is
`pane_not_found`, `tab_not_found` or `agent_not_found` by the command, no
Project `workspace_not_found`; a Server that is not running `no_server`,
one that does not answer in time `timeout` ("herdr did not answer in
time"), and `cli::launch` still refuses an argument over 131072 bytes
`too_long` first. `flow`'s `extras/config.rs` and `board`'s
`job_far.rs` start herdr themselves; only the terminal
runtime and herdr's first-run setup reach them, and Ferrite runs
neither.

### Seam 2: the API grows what the board needs

What 1.0 needs and Ferrite lacks is added to the API (ADR 0027):
declared once in the `operations!` table so the CLI binding, catalog and
schema follow, with a v1 fixture. The additions are those marked above:

| Operation | Does |
|---|---|
| `thread.start` | gains `env` (set in the agent's environment, under Ferrite's own) and `launcher` (a program the provider's CLI runs through, and its arguments: `bwrap … --`), each for every Session of that provider and saved with its `args`, so a revive or a Server restart starts it the same way; fixture `thread_start_launch` |
| `project.focus` | every window lands on the Project's first open Thread, as `project.open --focus` does by directory, and the answer names it (`thread`). A Project with no open Thread has nothing for a window to show: nothing moves and no `thread` is named |
| `ThreadInfo.completion_seq` | `attention_seq` when the Thread's last turn ended (its Attention came to rest, idle or done, from working or blocked): herdr's `completion_seq`, which the hook, the finish line, the Coordinator's pump and clean-up compare to tell a turn ended. Absent until a turn ended since the Thread last opened |
| `ThreadInfo.boot`, one `attention_seq` | herdr's `state_change_seq` is one counter for its whole server, so a turn after a pane's agent restarted always counts past the turn handled before (`flow/hook.rs` `after_handled`). So `attention_seq` is one count for every Thread of a boot, never reset by a park or a revive, and `boot` (the Engine's boot id, the one a window's resume names) says which run it counts in: the adapter makes it part of `terminal_id`, which herdr changes when its counter starts over |
| `thread.revive` | a parked Thread's agent started again in place, keeping its id (`unknown_thread` if no parked Thread has it; an open one is refused `invalid_request`: close it first): its `args`, `env` and `launcher` replace those it was started with, it carries on `resume`'s conversation or starts a new one under its history, and `ready`, `timeout_ms` are `thread.start`'s. `thread.close` stays the stop half |

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

In Ferrite this is `crates/ferrite/src/board` (`runtime`, `keys`,
`render`, `mouse`, `links`, `machines`). Its seams:

- **Runtime.** The view owns the `State` from `run/start.rs`'s `view`
  (the saved view, the tasks read from disk alone, so the first frame needs
  nothing slow), reads on the background executor once a second and right
  after an action (`data::load`, the selected task and the chat's `snap`),
  ticks once a second, runs each job on the background executor and saves
  the view with `run/prefs.rs` (`view.json`, the popup's
  `view-popup.json`). `settle_ms` is 0: a resize lays out at once.
- **Keys.** A keystroke is said as `run/input.rs` says crossterm's
  (`keys::press_of`): Option is alt, so Option+letter is `alt+<letter>`,
  not the character it composes; ⌘ stays the window's, but ⌘V, one paste.
  Under the keymap's `Projects` context every keystroke the table binds
  anywhere is masked (`keymap::board_reads_its_keys`, again after the
  operator's keys), so nothing bound around the board acts first: not a
  wall's y/n/a and digits, not Submit's enter, not Windows' ctrl keys,
  which are the board's there as in a terminal (ctrl+c closes it). The
  window keeps ⌘ and `projects::Toggle`. The board's `?` menu stays the
  list of its keys. Text is read key by key, as a terminal with Option as
  Meta reads it: the board registers no input handler, so an input method
  (CJK) and dead keys compose nothing in its text box and its
  `Screen::cursor` places no candidate window. An input handler would
  hand Option's dead keys (Option+E, U, I, N, `) to the input method in
  place of the board's alt keys; text composed elsewhere comes in whole
  by ⌘V.
- **Inks.** `board.theme`'s `herdr` (and `auto`, `dark`) is Ferrite's own
  colours given as herdr's tokens and finished by the board's own rules
  (`theme::greys`, `theme::picks`: the greys held readable, its review
  blue, queue pink, todo grey, automations purple); `light` and a herdr
  theme's name are that palette on its own ground. `NO_COLOR` paints
  nothing. Every piece starts on its cell; a glyph Geist Mono lacks gets a
  box of its own cells.
- **Placement.** `board.open: tab` draws the board in the board's place,
  kept while you are on a Thread; `popup`, a sheet over a veil, 90% of the
  board (all of it under 110×30 cells). The projects key shows it, and on
  it, goes back to where you came from; landing on a Thread moves a tab's
  board aside. `board.notify: bell` rings the system bell.
- **Opening an agent.** Every agent runs in a Ferrite window, so
  `cli::in_herdr` is true (a `Ferrite:` mark): Enter's job focuses the
  agent through the adapter (`agent focus`, `thread.focus`) and says
  "Opened <id>'s agent", or closes the popup; it never hands over a
  terminal. The popup opens on the task whose agent runs in the focused
  Thread (`Ctx::from`, the Thread's id, which is its agent's pane id to
  the adapter).
- **What the runtime leaves out.** `Ctx::own_key` is unset: herdr hands
  its popup the key that opened it, which the board must catch to close;
  here `projects::Toggle` acts before the board. The board does not start
  the keeper (`keeper::ensure_keeper` runs the `herdr-projects keeper`
  command, which Ferrite's binary is not): starting it is the flow
  crate's seam, and until a keeper runs the loader does its tick
  (`keeper::tick_once`). herdr's dots (`ui.status_indicators = "dots"`)
  are a herdr setting Ferrite has no counterpart for: the board always
  draws its marks; the vendored dots stay drawable for upstream's
  merges.
- **Mouse** (`mouse`). `run/mouse.rs` for GPUI: the wheel and a click
  go to `update` as the board's own events at their cell (`update/mouse.rs`
  decides: the chat's message under the pointer scrolls two lines a step,
  elsewhere the wheel is ↑↓, a click selects a row or folds a heading,
  none of it in the menu, the whole task or a question). One wheel event
  per line the wheel or trackpad moves, so a faster swipe goes further. A
  drag selects the board's own text, drawn in reverse, and letting go
  copies it to the clipboard where the terminal wrote OSC 52 (`Copied N
  characters.`); a double click copies a word, never the frames. A
  double click is upstream's, a second press on the same cell within
  400 ms, not the system's click count, which also counts a quick
  second click a few pixels away.
- **Links** (`links`). The words `view/` marks with an address (a pull
  request, `↗ Open review`, `Open report`, `Open plan`, a piece of
  evidence), which a terminal opens as OSC 8 links, open on a click. The
  evidence and report pages stay on `flow`'s own page server (`serve`,
  127.0.0.1, shared with `tailscale serve` where it is installed).
- **Other machines** (`machines`). As `run/mod.rs`'s `poll`: the list of
  machines (`remote::machines`, Herdr's machine list, answered from
  `machine.list` in its order) at most once a minute, each machine in
  view over upstream's ssh (`herdr-projects remote list`) whenever the
  board polls (every 5 s with other machines shown), one that did not
  answer only after 30 s unless the poll is for now. The words that name
  herdr's way to a machine say Ferrite's instead (`Ferrite:`): no
  machine yet is added in Settings, under Machines, not by `herdr machine
  add` (the board's message and `remote check`'s); the machines the board
  knows are `ferrite machine list`'s, not `herdr machine list`'s (an
  unreachable machine's why, a wait `--after` a machine it does not
  know); another machine's agent brought to the front there is gone to
  from the nav, not herdr's sidebar. A machine has one Server, whose
  session is `default`, so the header never says `session <name>`.

Not drawn, being a terminal's alone: the first-run setup (`view/setup.rs`,
herdr's config blocks and board key), the held-size warning
(`view/mod.rs`'s `held_lines`: no remote view holds a window), OSC 7501
records (the nav draws its rows from the board's data), attaching the
terminal to another machine's agent (the board says it cannot), the web
UI's offer, and loading a new binary in place (the board is part of
Ferrite, which updates whole, so `herdr-projects was updated` is never
said).

herdr's sidebar rows become Ferrite's nav rows, drawn from the same data
(`flow`'s `flow/sidebar*.rs`). The key that opens the board, herdr's
`prefix+a`, is Ferrite's existing `projects::Toggle`; the board's own
keys are its own, as in the terminal.

The nav reads the board as `sync_sidebar` does, at most once a second
and at once when an open Thread's Attention changes, off the UI thread:
the board's rows are `flow::live::board_for`'s, read through the adapter
as the board reads them (`board_seen` hands the nav the same session), so
the nav and the board never disagree. A pane's id is the adapter's, and
its Thread is the agent the adapter lists there (its terminal), so only
an open Thread is a live agent. Each live agent's Thread (a task's agent,
its reviewer during a review, the Coordinator) is drawn as its entry
where the Thread already sits in the tree: row 1 the task's number and
title beside Ferrite's own Attention dot, then row 2 (who is on it and
where it stands, its time last and dropped first on a short row) in the
task's status ink (the Coordinator's in plain text), then row 3, dim (its
numbers, or the Coordinator's latest message). A task's own space names
the heading whose placed Threads all share that Space, as the adapter
lists each agent's (a Space the board made, a linked worktree, a
Project's own), its branch still at the right and its numbers under it;
while the task waits for a pick the name stays and the numbers go, as
upstream leaves them. Status inks are Ferrite's theme equivalents behind
one function (`sidebar::ink`). Nothing is reported to
herdr, so `report-metadata` is answered by no call; a task archived or
removed while its Thread lives simply has no entry. A window on another
machine draws none of this computer's board. The bottom bar says the
board header's tally (`3 need you · 1 ready · 2 working`) and opens the
board. Upstream raises no pop-up of its own when a task needs you, is
ready or fails: `board.notify` alone decides (a bell where a board is
open, or a command), so the window adds none.

### Migration from 0.2

1.0 never converts 0.2: its take-over (`sw_flow::take_over::at_start`,
which runs `take_over` with `take_over::live::Live`) moves 0.2's projects
root, `~/.herdr-projects`, whole to `~/.herdr-projects-legacy` and its
settings, `~/.config/herdr-projects`, to `-legacy` beside it, after
stopping its ticker, and takes 0.2's hooks and `autoproject` skill links
out of the agent programs' settings, backing each file up first.

Ferrite's 0.2 kept its projects in that same root and its settings in that
same folder, with its own journal, `ferrite-owned.json`; its hooks ran
`ferrite-projects … hook --agent …`, Copilot's in `hooks/ferrite-projects.json`.
Those names are recognised at `Ferrite:` marks beside upstream's (`legacy.rs`
and `take_over/outside.rs`), so the same take-over sets Ferrite's 0.2 aside:
nothing deleted, nothing converted, each file it changes backed up first.

It runs at every Server start, before the keeper, as `ferrite-projects
take-over`. As any command does, it first runs upstream's start (`take_over::at_start`, then
the home folder's move, the copy of older data and the settings' move);
then it prints the line upstream keeps for the human until it is said
(`move_note`, `move_told`), so the take-over is done once and said once.
The Server logs what it prints and shows it as one toast once a window is
there (`server/projects.rs`). Between the two, the Server runs the
startup hook that starts the keeper (Seam 4). ADR 0028's
`coordinators.json` and `ferrite_migrate.rs` left with the 0.2 crate:
neither 0.2 nor ADR 0028 ever shipped, and the projects that migration made are set aside with
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
started Ferrite (`spawn::INHERITED_MARKERS`). The board in Ferrite's
app, which is handed none of them, links the Server's API socket all the
same: `cli::current_socket` falls back to it (Seam 1). `HERDR_PLUGIN_ID` is not
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
- The 0.2.34 crate is gone with its binary, its Server glue (the
  coordinators' migration, the ticker), its CI step, the window's 0.2
  Projects view and its API operation `nav.focus` (ADR 0027): 1.0's
  take-over sets an install of it aside, so nothing of it is needed to
  migrate.
- The startup hook, which starts the keeper, runs after `take-over` has
  returned, on the same thread, and the events' hooks only after it
  (`server/projects.rs`): every command takes over at its start, and two
  at once would race over the 0.2 folders.
- Every Thread event runs a short process, as every pane event does
  under herdr; the hook keeps its own housekeeping to once in two
  seconds, and where no task was ever made only sees that a keeper
  runs. So a keeper runs on every machine with a Server, as on every
  machine with the plugin, idle while there are no tasks.
