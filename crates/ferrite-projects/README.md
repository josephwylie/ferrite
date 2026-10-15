# ferrite-projects

Ferrite's coordinator ("Projects"): a coordinator conversation, parallel
worker Threads, shared memory and an overview of what needs the operator.

Vendored from [herdr-projects](https://github.com/eliasstravik/herdr-projects)
at commit `4e4548c3c43888e6be1c96906215998f07dc63f0` (version 0.2.34),
MIT licensed, Copyright (c) 2026 Elias Stravik — see `LICENSE`. Its `src/`,
`tests/`, `skill/`, `docs/` and `build.rs` are copied verbatim; the package is renamed `ferrite-projects` and keeps
upstream's edition.

Ferrite changes upstream files only at the seams, each marked `Ferrite:` in
a comment. `src/herdr.rs` is the adapter: upstream's types and signatures,
answered by Ferrite's Server API through the `ferrite` CLI. How the seams
are drawn and how upstream updates are merged is in
`docs/adr/0030-projects-vendored-from-herdr-projects.md`.

## Ferrite changes

What differs from upstream, beyond the adapter.

### Install, setup and health

- **Shipped beside `ferrite`.** The binary is `ferrite-projects`, in the
  app bundle's `Contents/MacOS` (`scripts/install-app.sh`), signed with the
  app. It is found as `FERRITE_BIN` is: `$FERRITE_PROJECTS_BIN`, else the
  running executable's sibling named `ferrite-projects`
  (`paths::binary`, and `server/projects.rs` in `ferrite-core`). That
  canonical path is what `AGENTS.md`, the hooks and the ticker name. On a
  machine reached over ssh it is installed beside `ferrited`:
  `cargo install --path crates/ferrite-projects`. It is not built for
  Windows (it is Unix-only upstream).
- **Started by the Server.** Herdr's plugin `[[startup]]` hook and the
  `startup` subcommand are gone: on start the Server runs
  `ferrite-projects migrate-ferrite-coordinators <store>/coordinators.json`
  when that file is there, then `ferrite-projects ticker start` (which does
  nothing without projects), with `FERRITE_BIN` and `FERRITE_SOCKET` set.
  Failures are logged; the Server never waits for them.
- **`update` and the command link are removed** (`src/update.rs`,
  `src/command_link.rs`, `scripts/link-command.sh`, the `update` subcommand,
  `doctor`'s update and command checks, and their tests): Ferrite ships the
  binary and updates itself (ADR 0023).
- **`configure`** keeps the progress hooks (Claude Code, Codex, Droid,
  Gemini CLI, Copilot CLI) and the `autoproject` skill links, with the same
  journal, rollback and byte-identical `unconfigure`. The hook entries and
  Copilot's file (`hooks/ferrite-projects.json`) are recognised by the name
  `ferrite-projects`. Herdr's `config.toml` edits are removed with their
  tests: the sidebar card layout, the popup key and its conflict check
  (`herdr --default-config`), `herdr config check`, `server reload-config`,
  the default agent view, `--key` and `--hooks-only` (`sidebar.rs` loses
  its `config.toml` section). Ferrite's Projects view and nav rows replace
  them natively. `setup::configure_ferrite` is what the Projects view runs.
- **The skill is built in.** With no plugin checkout to link to, the
  `autoproject` skill's files are compiled into the binary and written to
  `<config dir>/skill/autoproject` (`setup::skill_source`) whenever they
  differ, and linked from there as upstream links its checkout.
- **`doctor`** returns its report (`doctor::run`) for the Projects view; the
  CLI prints it and still fails with `some checks failed`. It checks Ferrite
  (`ferrite --version` against the minimum) and the Server at
  `$FERRITE_SOCKET` instead of Herdr and its session; the sidebar check is
  removed. Every other check is upstream's.

### Machines

- Saved machines are Ferrite's: `ferrite machine list` (its reply's
  `result`, `[{id, label, target}]`), with `[machines.X] ssh` in
  `config.toml` as before. Errors name `ferrite machine list`.
- On another machine the binary is `ferrite-projects` (`profile resolve`,
  `profile list`); an old or missing one says to update Ferrite there.

### Skill wording

Only references to Herdr's UI that Ferrite does not have:

- `COORDINATOR.md`, `THREAD.md`: "a Herdr project" is "a Ferrite project".
- `COORDINATOR.md`: the popup (`prefix+a`) is Ferrite's Projects view, in
  the profile, `TASKS.md`, task notes, safety and routines rules;
  `herdr agent prompt/read/send-keys` is `ferrite agent prompt/read` (Ferrite
  has no send-keys); `herdr machine list` is `ferrite machine list`.
- `autoproject/` is unchanged.
