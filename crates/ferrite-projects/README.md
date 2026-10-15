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
  sibling named `ferrite-projects` of the running executable, its links
  resolved first, so a `ferrite` linked onto `PATH` still finds the bundle's
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
  Failures are logged; the Server never waits for them. The migration's
  notes are logged and shown as one toast once a window is there. `startup`'s default
  agent view goes with it: the Projects view orders Threads natively.
- **`update` and the command link are removed** (`src/update.rs`,
  `src/command_link.rs`, `scripts/link-command.sh`, the `update` subcommand,
  `doctor`'s update and command checks, and their tests): Ferrite ships the
  binary and updates itself (ADR 0023).
- **`configure`** keeps the progress hooks (Claude Code, Codex, Droid,
  Gemini CLI, Copilot CLI) and the `autoproject` skill links, with the same
  journal, rollback and byte-identical `unconfigure`. The hook entries and
  Copilot's file (`hooks/ferrite-projects.json`) are recognised by the name
  `ferrite-projects`, or by the file name `$FERRITE_PROJECTS_BIN` gives the
  binary. The journal is `ferrite-owned.json`, not upstream's `owned.json`:
  the config directory (`~/.config/herdr-projects`) and the root
  (`~/.herdr-projects`) stay upstream's (`paths.rs`), so profiles, safety and
  projects are shared with an upstream install, but neither install
  restores, removes or takes over what the other configured. A skill link
  upstream made is left alone, as any link that is not ours. Herdr's `config.toml` edits are removed with their
  tests: the sidebar card layout, the popup key and its conflict check
  (`herdr --default-config`), `herdr config check`, `server reload-config`,
  the default agent view, `--key` and `--hooks-only` (`sidebar.rs` loses
  its `config.toml` section). Ferrite's Projects view and nav rows replace
  them natively. `setup::configure_ferrite` is what the Projects view runs.
- **The skill is built in.** With no plugin checkout to link to, the
  `autoproject` skill's files are compiled into the binary (`SKILL_FILES`; a
  test fails when `skill/autoproject` gains a file it lacks) and written to
  `<config dir>/skill/autoproject` (`setup::skill_source`) by `configure`
  and `doctor --fix` only (`setup::install_skill`; never by `--dry-run` or a
  plain `doctor`), then linked from there as upstream links its checkout.
  A Ferrite update leaves the older copy until then: `doctor` warns
  (`skill files`) and `--fix` rewrites it, as upstream's `update` did.
- **`doctor`** returns its report (`doctor::run`) for the Projects view; the
  CLI prints it and still fails with `some checks failed`. Its `binary:` and
  `version:` lines, and the ticker's comparison, are the shipped
  `ferrite-projects`'s (its `--version` when this runs inside `ferrite`). It
  checks Ferrite (`herdr::version`: `ferrite --version` against
  `herdr::MIN_VERSION`) and the Server at `$FERRITE_SOCKET`
  (`paths::resolve_session`) instead of Herdr and its session; the sidebar
  check is removed. Every other check is upstream's. Those two checks rest
  on the adapter and the API: `herdr::version`, Ferrite's minimum version
  and `resolve_session`'s `FERRITE_SOCKET` are the adapter's (`herdr.rs`,
  `paths.rs`), `ferrite --version` the CLI's. Until both are in, `version`
  answers `not_wired`, so `doctor` fails and so do its tests
  `new_ferrite_passes_and_warnings_do_not_fail`,
  `old_ferrite_fails_and_names_the_minimum` and
  `fix_links_the_skill_only_for_a_configured_harness…`.
- **Tested in CI** on macOS (`cargo test -p ferrite-projects`); it is
  Unix-only.

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
  `herdr machine list` is `ferrite machine list`.
- `COORDINATOR.md`: upstream forbids calling `herdr agent prompt`, `read`
  and `send-keys` directly; Ferrite has no `send-keys`, so it forbids
  `ferrite agent prompt`, `ferrite agent read` and `ferrite decision answer`
  (answering a Thread's questions), and hand-assembled commands for
  "answering its questions" instead of "sending keys".
- `autoproject/` is unchanged.

### Text agents and users read

Herdr's name is replaced where it means Ferrite; behaviour is unchanged.

- **`AGENTS.md` priming** names "the Ferrite project" and is "Written by
  Ferrite's Projects". A file carrying either that mark or upstream's
  "Written by herdr-projects" is ours and is refreshed, not kept aside as
  `AGENTS.md.before-herdr-projects`. `doctor`'s priming check reads only the
  binary prefix, so it is unchanged.
- **Errors and inbox items** say "Ferrite's Server" where upstream says the
  herdr session: not reachable, belongs to another Server, restarted. The
  commands they recommend are upstream's.
- **Remote Threads** are reached by selecting the machine in Ferrite's nav
  or with `ferrite --remote <ssh target>` (ADR 0026), not Herdr's sidebar or
  `herdr --remote`.
- **CLI text**: the command is `ferrite-projects` ("Projects for Ferrite"),
  errors are prefixed `ferrite-projects:` (the popup strips that prefix),
  help and messages name Ferrite panes and Ferrite agent kinds, and the
  rename note to a reopened coordinator is "Note from Ferrite's Projects".
  Every command the text tells someone to type is `ferrite-projects`: the
  `safety` refusal and listing, the inbox item on refused launch flags, the
  effort error and the Thread brief's progress line. The progress hook
  recognises the reporter's own call by that name too, as upstream does by
  its own.
- Left as upstream: `--session`/`--socket` (and `herdr --session` in its
  help), the workspace a slug resolves from ("current herdr workspace"),
  `focus`/`unfocus` and `adopt-workspace`, whose meaning is the adapter's;
  `doctor`'s advice to run `herdr plugin disable agent-progress`, which
  names Herdr's own plugin; the hostile `[herdr-projects ticker]` strings in
  tests; and `docs/*.md`, upstream's documentation of the Herdr plugin.
