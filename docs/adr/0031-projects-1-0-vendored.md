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
door), which finds no web UI under Ferrite and so does nothing.

### Not yet taken: the command line

Upstream's command line crate (`src/`: `cli/`, the `sw` library that
re-exports every crate, and the made docs) and its tests (`tests/`, one
program over `sw`) are what agents and hooks run. They are taken when
the 0.2.34 crate leaves `crates/ferrite-projects`, whose binary name they
share, with `sw`'s re-export of `sw_web` dropped. Until then the three
crates' own tests run.

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
one function (`sidebar::ink`) until the board's GPUI drawing lands, when
that function takes the board theme's inks. The nav reads the
board whether or not 0.2's projects root resolves. Nothing is reported to
herdr, so `report-metadata` is answered by no call; a task archived or
removed while its Thread lives simply has no entry. A window on another
machine draws none of this computer's board. The bottom bar says the
board header's tally (`3 need you · 1 ready · 2 working`) and opens the
board. Upstream raises no pop-up of its own when a task needs you, is
ready or fails: `board.notify` alone decides (a bell where a board is
open, or a command), so the window adds none. 0.2's nav focus (`nav.focus`)
has no 1.0 counterpart; the window ignores it until the API drops it,
which waits on 0.2's crate: its own CLI still calls `nav focus`.

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

It runs where `migrate-ferrite-coordinators` ran (ADR 0030): once at
Server start, before the keeper, through the shipped binary, its lines
logged and shown as one toast. ADR 0028's `coordinators.json` and
`ferrite_migrate.rs` leave with the 0.2 crate: neither 0.2 nor ADR 0028
ever shipped, and the projects that migration made are set aside with
the rest of 0.2's.

### Upstream updates

The vendored commit is recorded in each crate's `README.md`. For each
crate, apply upstream's diff from that commit to the new one
(`git diff c019063..<new> -- crates/core | git apply --3way -p3
--directory=crates/ferrite-projects-core`), resolve conflicts at the
`Ferrite:` marks, answer (or refuse) any herdr call upstream starts
making in `herdr/ferrite.rs` before it ships, record the new commit, and
run the crates' tests.

## Consequences

- Ferrite carries ratatui and crossterm while the vendored board does;
  only `run/` uses them beyond colour types.
- The workspace's `serde_json` gains `float_roundtrip`, as upstream's
  data needs (a closing note's time read back is the time written);
  `preserve_order` was already unified in.
- The 0.2.34 crate, its Server glue and ADR 0030's terms in CONTEXT.md
  stay until the app runs on 1.0, then go together.
