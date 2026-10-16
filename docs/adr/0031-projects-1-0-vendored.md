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
2024 and formatting. Its binary keeps upstream's name until it ships
(below); it is what agents and hooks will run, and every command, answer
and refusal is upstream's. `sw` re-exports `sw_board` in place of `sw_web`. Upstream's
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

It ships with the view that replaces 0.2's, never before. The window's
Projects view is still 0.2's, and so is the program Ferrite ships as
`ferrite-projects`: that view runs 0.2 commands (`--root … adopt-workspace`,
`doctor`, `new`, `sweep`), its "configure" writes 0.2's hooks, which run
`ferrite-projects --root … hook`, and the Server starts 0.2's ticker. The
1.0 program refuses every one of them, and its take-over would set aside
the 0.2 data that view still works on. So until the board's merge the 1.0
program is built and tested, not bundled. In that merge, together:

- the `[[bin]]` is named `ferrite-projects`, and `scripts/install-app.sh`
  builds it (`cargo build -p herdr-projects --bin ferrite-projects`) in
  place of 0.2's and copies it into the app bundle's `Contents/MacOS`,
  beside `ferrite`, signed with it;
- the Server runs `ferrite-projects take-over` at start in place of 0.2's
  coordinators' migration and ticker (below), then starts the keeper;
- the 0.2 crate (`crates/ferrite-projects-0-2`: its library, binary,
  `configure`, skill and tests), its Server glue, its CI step and the
  window's 0.2 view leave.

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

In Ferrite this is `crates/ferrite/src/board` (`runtime`, `keys`,
`render`). Its seams:

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

It is to run where `migrate-ferrite-coordinators` runs (ADR 0030): at every
Server start, before the keeper, as `ferrite-projects take-over`, once the
1.0 program ships (above). As any
command does, it first runs upstream's start (`take_over::at_start`, then
the home folder's move, the copy of older data and the settings' move);
then it prints the line upstream keeps for the human until it is said
(`move_note`, `move_told`), so the take-over is done once and said once.
The Server is to log what it prints and show it as one toast once a window
is there, as `server/projects.rs` does with the coordinators' migration. ADR 0028's `coordinators.json` and
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
- The 0.2.34 crate stays, in `crates/ferrite-projects-0-2`, with its
  binary (shipped as `ferrite-projects`), its Server glue (the
  coordinators' migration, the ticker) and its CI step, only while the
  window's 0.2 Projects view still runs on it, and leaves with that view
  ("The command line"). CONTEXT.md already speaks 1.0's terms.
- Whatever starts the keeper at Server start is to run after `take-over`
  has returned, on the same thread (`server/projects.rs`): every command
  takes over at its start, and two at once would race over the 0.2
  folders.
