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
herdr's words and shapes from Ferrite's Server, a request on its API
socket as the `ferrite` CLI (ADR 0027) sends it, and refuses one with no
Ferrite counterpart with the code `not_wired` before anything runs,
which callers treat as any refusal of herdr's. A refusal is retired by
wiring it, never by working round it in a caller. Three more `Ferrite:`
lines reach the seam: `cli::current_socket` is the Server's API socket
(`FERRITE_SOCKET`, else `api.sock` in the Store), so there is one
session, `default`, and `Herdr::alive` is the Server answering;
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
| `plugin pane open`, `pane layout`, `pane zoom`, `terminal session control`, `popup.close`, `tab.get`, `events.subscribe` | refused: the board's terminal runtime, replaced by Seam 3. `socket::on_tab_shown` asks the seam for its subscription by a `Ferrite:` line and, refused, connects nowhere (upstream's popup test in `board_stay_keys.rs` is ignored) |
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
there (`server/projects.rs`). ADR 0028's `coordinators.json` and
`ferrite_migrate.rs` leave with the 0.2 crate: neither 0.2 nor ADR 0028
ever shipped, and the projects that migration made are set aside with
the rest of 0.2's.

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
- Whatever starts the keeper at Server start runs after `take-over` has
  returned, on the same thread (`server/projects.rs`): every command takes
  over at its start, and two at once would race over the 0.2 folders.
