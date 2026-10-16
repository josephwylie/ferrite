# sw-core

herdr-projects 1.0's `crates/core`: the board's data (`store/`), the agents,
herdr, and the small tools every part uses.

## Vendored

Ferrite's Projects is [herdr-projects](https://github.com/eliasstravik/herdr-projects)
1.0, MIT licensed, Copyright (c) 2026 Elias Stravik (see `LICENSE`), at
commit `c019063b65b3ff6a68688345b9f304533c8fb83c` (`v1.0.1-8`). Three of its
crates are copied verbatim, each keeping its package name, library name and
edition (2024):

| Upstream | Here | Package |
|---|---|---|
| `crates/core` | `crates/ferrite-projects-core` | `sw-core` |
| `crates/flow` | `crates/ferrite-projects-flow` | `sw-flow` |
| `crates/board` | `crates/ferrite-projects-board` | `sw-board` |

Upstream's command line crate (`src/`, `tests/`) is
`crates/ferrite-projects` (`../ferrite-projects/README.md`). Not taken:
`crates/web` (the board in the herdr web UI; Ferrite has none), and
upstream's plugin manifest, scripts, assets and docs. Why, and how the rest
of Ferrite meets these crates: `docs/adr/0031-projects-1-0-vendored.md`.

## Ferrite changes

Inside vendored files a change is the smallest that works and is marked
`Ferrite:` in a comment, so an upstream update merges with few conflicts.
`rustfmt.toml` keeps upstream's formatting.

- **`Cargo.toml`** (each crate): upstream's `[workspace.package]` values
  are written in, as Ferrite's workspace has its own. The dependencies
  upstream's workspace declares are declared, as upstream does, in
  Ferrite's root `Cargo.toml`.
- **`src/herdr/ferrite.rs`** is the seam: `cli::launch` (every herdr
  command) and `socket::ask` (every request on herdr's socket) go to it
  once a test's stand-in has had its turn. Each call is answered from
  Ferrite's Server API as it is wired and refused `not_wired` until then.
  Every public signature of `src/herdr/` is upstream's. Left as they are:
  `socket::on_tab_shown`, `flow`'s `extras/config.rs` and `board`'s
  `job_far.rs`, which start herdr themselves, as only the terminal board
  and herdr's first-run setup reach them, and Ferrite runs neither.
- **`src/store/files.rs`**: `bin()`, the program handed to agents and
  hooks, is the shipped `ferrite-projects` when the code runs inside
  Ferrite's app (the executable is `ferrite`): its sibling, links resolved,
  or `FERRITE_PROJECTS_BIN`.
- **The take-over of 0.2** knows Ferrite's 0.2 by its own names beside
  upstream's: its settings folder by its journal `ferrite-owned.json`
  (core's `store/old_installs/legacy.rs`), its hooks by `ferrite-projects
  … hook --agent …` and Copilot's file `hooks/ferrite-projects.json`
  (flow's `take_over/outside.rs`). Tested in `crates/ferrite-projects`'s
  `tests/ferrite.rs`.

## Upstream updates

From the recorded commit to the new one, for each crate:

```sh
git -C <upstream> diff c019063..<new> -- crates/core \
  | git apply --3way -p3 --directory=crates/ferrite-projects-core
```

and likewise `flow` and `board`. Resolve conflicts at the `Ferrite:` marks,
wire (or refuse) any herdr call upstream starts making in
`src/herdr/ferrite.rs`, record the new commit here and in the other two
READMEs, and run `cargo test -p sw-core -p sw-flow -p sw-board`.
