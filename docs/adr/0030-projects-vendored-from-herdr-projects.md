---
status: superseded by 0031
date: 2026-10-15
supersedes: 0028
---

# Projects: herdr-projects vendored as Ferrite's coordinator

> Superseded by ADR 0031: herdr-projects 1.0 is vendored in place of
> 0.2.34, and what this ADR built leaves with it.

Follows the Herdr benchmark (`docs/research/herdr-benchmark-2026-10-15.md`)
and replaces Ferrite's own Coordinator Threads (ADR 0028). Builds on the
Server (ADR 0025) and its API and CLI (ADR 0027).

## Context

ADR 0028 rebuilt herdr-projects' pattern from its ideas: a coordinator per
Project, workers in worktrees, a memory file, PR follow-up. It took a
fraction of the behaviour; herdr-projects carries far more (tasks,
routines, inbox, adoption, auto-resolve, remote machines, safety
settings, nudges), each rule, threshold and brief settled in use and
tested. Re-deriving them one at a time is slower and worse than
taking them. herdr-projects is MIT licensed and talks to Herdr through one
module, `src/herdr.rs` ("Typed calls to the herdr CLI"), and to the
operator through a terminal UI and Herdr's sidebar.

## Decision

### Vendored verbatim

`crates/ferrite-projects` is herdr-projects at upstream commit
`4e4548c3c43888e6be1c96906215998f07dc63f0` (0.2.34): `src/`, `tests/`,
`skill/`, `docs/`, `build.rs`, `scripts/link-command.sh` (its tests run
it) and `LICENSE` (MIT, Elias Stravik), with upstream's edition (2024).
Only the package name changes. Herdr's plugin manifest, release and
install scripts, and `assets/` are not taken. Every behaviour, rule,
brief and skill wording, threshold and test is kept.

Inside vendored files a change is the smallest that works, and carries a
`Ferrite:` comment, so an upstream update is a three-way merge with few
conflicts. Seams are clean code; vendored code is upstream's.

### Seam 1: the adapter

`src/herdr.rs` keeps upstream's types (`Herdr`, `Agent`, `Pane`,
`HerdrError`, …) and every public signature, so callers are untouched.
Its calls go to the Server through the `ferrite` CLI: `bin` is
`$FERRITE_BIN`, the socket is `$FERRITE_SOCKET`, `--machine` is the CLI's
own (ADR 0027), and a pane, tab or agent id is a Thread id. Herdr's agent
states and Ferrite's Attention are the same four words, so an agent's
status is its Thread's Attention. `Herdr::call` takes the herdr argv
callers build themselves and answers it from a `ferrite` command line;
the typed methods do the same for theirs. A call with no Ferrite meaning
is refused with the code `not_wired` before anything runs, which callers
already treat as any refusal. Each refusal is retired by wiring it, not
by working round it in a caller. A Thread is born with its agent, so a
placement (`tab create`, `workspace create`, `worktree create`, `worktree
open`) stages where it will start and `agent start` starts it; the
crate's README lists every mapping and what is still refused.

Screen reading (`agent read --format ansi`: `prompt_box`, `trust_screen`,
the brief and steps checks) has no Ferrite counterpart and gets none:
Ferrite knows exactly what a Terminal only shows (a pending Decision, a
held or queued prompt, the transcript), so those checks read that state
instead. Likewise there are no shells, keys or panes to type into:
`pane send-text`, `pane run`, `pane process-info` and most `agent
send-keys` uses become the operation that says what was meant.

### Seam 2: the API grows what Projects needs

An operation herdr-projects needs and Ferrite lacks is added to the API
(ADR 0027): declared once in the `operations!` table, so the CLI binding,
catalog and schema follow, with a v1 fixture. Expected additions:
Projects (list, get, open, rename, close: Herdr's Spaces), thread close,
worktree branch/base on `thread.start` and worktree removal (never
forced), extra agent arguments and focus on `thread.start`, delivery
confirmation on `agent.prompt` (answer once seen working or blocked, not
settled), an interrupt, the provider session id and an Attention change
counter on a Thread, saved-machine listing, and `ferrite --version`.

### Seam 3: native UI instead of the terminal

The crate's terminal surfaces (the popup and its tabs, the overview,
trust and prompt-box screens, `configure`/`doctor`/command linking, which
edit Herdr's config) are not ported. The crate becomes a library as well
as a binary; GPUI views are fed by its data functions, and its sidebar
metadata and tab-bar text become Ferrite nav rows and the bottom bar.
The `ferrite-projects` binary stays for what agents run from the skill
(`thread start`, `report`, `context`, …).

### Ferrite's own coordinator is removed

ADR 0028 is superseded. `ferrite-core`'s `coordinator.rs`,
`server/coordinate.rs`, the `coordinator.*` and `worker.*` operations and
their fixtures, the coordinator palette and nav code, and the skill's
coordinator section go. The API's names are append-only once released;
these four never were (the API is not on `main`), so they are removed
rather than kept as aliases. CONTEXT.md's Coordinator, Worker and Memory
entries are rewritten to herdr-projects' terms.

A one-time migration runs on first start when `coordinators.json` is
beside the Store: each entry becomes a project under the Projects root
(`PROJECT.md` from its goal, standing instructions and the configured
default profiles, `MEMORY.md` from its memory file, the `pr-followup`
routine enabled when it followed PRs, and each open worker adopted as
`thread adopt` would, from the Server's own `thread.list`, re-briefed once
it is ready). The Server must answer first; the file is then renamed
`coordinators.json.migrated` before anything is created, so the migration
never runs twice, and an entry or worker that fails becomes a note: a
parked worker Thread is not listed, so it is left for `thread adopt` once
it runs. With no file there is nothing to do. A name a project under the
root or an earlier entry already slugs is numbered (`app`, `app-2`). The
old coordinator Thread works in its repository, not the project folder,
so it is not carried over as a coordinator, nor prompted; `open` starts
one. The Server runs it at start as `ferrite-projects
migrate-ferrite-coordinators <store_dir>/coordinators.json`, logs its
notes and shows them as one toast once a window is there to show it.

### Upstream updates

The vendored commit is recorded in `crates/ferrite-projects/README.md`.
To take an upstream release: diff upstream from the recorded commit to the
new one, apply it to the crate (`git apply --3way` on the crate's path),
resolve conflicts at the `Ferrite:` seams, record the new commit, and run
the crate's tests. A new herdr call upstream starts using lands in
`herdr.rs` (or a refusal) before it ships.

## Consequences

- Ferrite carries a second edition (2024) and herdr-projects'
  dependencies (`clap`, `crossterm`, `jiff`, `toml_edit`, …) until the
  terminal UI is gone.
- Until each seam is wired, the crate's tests that drive Herdr's argv
  through the fake runner fail; they are ported with the call they test.
- The coordinator is an ordinary agent following herdr-projects' skill;
  the Server keeps no coordinator book of its own.
