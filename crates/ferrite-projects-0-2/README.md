# ferrite-projects (0.2.34, leaving)

> Being replaced by herdr-projects 1.0 (ADR 0031), whose command line is
> `crates/ferrite-projects`. Until the board replaces the window's 0.2
> Projects view, this crate is still what that view runs and what ships as
> `ferrite-projects`, with its Server glue (`server/projects.rs`); all of it
> leaves in the board's merge. Its README is as it was before the move.

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
  on the adapter (`herdr::version` reads `ferrite --version` against
  `herdr::MIN_VERSION`, 0.4.0; `resolve_session` reads `FERRITE_SOCKET`, else
  `~/.ferrite/threads/api.sock`).
- **Tested in CI** on macOS (`cargo test -p ferrite-projects`); it is
  Unix-only.

### The adapter

`src/herdr.rs` answers every herdr call upstream makes from Ferrite's API
(ADR 0027, and the operations ADR 0030 adds), through the `ferrite` CLI:

- **Panes, tabs and agents are Threads** (`thread list`/`get`), their state
  the Thread's Attention, `state_change_seq` its `attention_seq`, the agent
  session its `provider_session`. A Space is a Ferrite Project (`project
  list`/`get`/`open`/`rename`/`close`), except a Thread in a linked worktree,
  whose Space is the worktree, named by its path.
- **Placement is staged.** A Thread is born with its agent, so `tab create`,
  `workspace create` (which opens the Project) and `worktree open` return a
  staged pane, `new:<focus>:<space>:<cwd>`, and `worktree create` has the
  Server make the worktree first (`ferrite worktree create`, in its
  registry's layout, forwarded with `--machine` to a machine's Server), so
  the brief is written into it, then stages its Thread there. `agent start`
  starts the Thread (`thread start --ready`; a resume among the agent's
  arguments, `agents::resume_args`, becomes its `--resume`, the rest go as
  `--args`, written as for the agent's terminal UI; Ferrite's Codex
  Provider turns Codex's into app-server settings, ADR 0027). A Thread
  staged in a Project's Space starts in it (`--project`), working in its
  own cwd. The ticker and `open` record the started Thread as the pane
  (`ticker.rs`, `coordinator.rs`), also when the start answers
  `agent_not_ready`: that refusal names the Thread it left, as Herdr's pane
  stays. A staged pane counts as a shell at its prompt (`thread.rs`,
  `pane_idle_shell`): Escape, Ctrl+C or Enter there does nothing, and any
  other call that needs its Thread is refused `agent_not_started` before
  anything runs (a refusal before typing). A start whose answer was lost is
  found by title and directory, never started twice.
- **Prompts** go whole (`agent prompt`); a confirmed one waits `--until
  working --until blocked` (`agent_prompt_stalled`, `timeout` and no reply
  are unconfirmed; `agent_blocked`, `agent_starting`, `agent_not_started`,
  `pane_not_found` and `unreachable` are refusals before sending).
- **No screen is read.** `agent read` is the transcript's tail; the input
  box and trust screens are read from the Thread's exact state
  (`PaneState`: a draft, a held or queued prompt, the pending Decisions):
  `prompt_box::Screen`, which a Terminal's screen still implements.
- **Keys mean what they meant**: `esc` and `ctrl+c` interrupt (`agent
  interrupt`); a menu key answers the pending Decision (`decision answer`)
  as its menu would: Enter the first item, a digit that item (an approval's
  allow, its suggestions, deny; a question's options; a single choice
  field's options), `y` allow and `n` deny (a yes/no field's answer), and
  Enter on a form its defaults. Enter with nothing pending sends nothing, as
  in an empty input box. Anything else is refused, `no_decision` or
  `invalid_answer`, before anything is sent. `pane send-text` answers the
  pending Decision that takes typed text (a question's own answer, a form's
  single text field); there is no input box to type into otherwise
  (`no_decision`). A Decision whose answer is being submitted is not
  pending (`decision.list` leaves it out), so `thread keys --text X enter`
  answers once and its Enter sends nothing.
- **Closing**: `tab`/`pane close` is `thread close`; `workspace close` is
  `project close`, or closes the Threads of a worktree's Space; `worktree
  remove` is `ferrite worktree remove` of the Space's path (never forced:
  `dirty_worktree_requires_force`; a Project's Space is no worktree,
  `worktree_not_found`).
- `notification show` keeps its sound (`request` is important), `focus`/
  `unfocus` are `nav focus` (the project's slug, passed on unread), `machine
  list` is `ferrite machine list` (`result.machines`).
- **Silent successes**: `report-metadata` and `workspace.move` (the nav
  draws and orders its rows from the crate's data).
- **Refused** (`not_wired`, nothing runs): `pane process-info` (a Thread is
  never a shell; `pane_idle_shell` answers from the staged id, true only
  before its agent starts), and any call upstream may add before it is
  mapped.

Ferrite error codes reach callers under Herdr's names: `unknown_thread`
is `pane_not_found`, `project_not_found` `workspace_not_found`,
`server_unavailable` `unreachable`. A kind Ferrite has no Provider for
(only `claude` and `codex` run) fails `no_provider`, as a missing binary
does. `open --here` never runs a coordinator on the caller's terminal: a
Thread is never a shell.

A **checkout thread** works in the user's own repository, so its Thread
belongs to that repository's Ferrite project, not to the coordinator's
(upstream it is a tab in the coordinator's Space; `thread start --project`
refuses a cwd outside the Project, so its start goes without it): archiving the project
(`lifecycle::close_workspaces`, `project close` on the coordinator's
project) leaves it open, and it is closed on its own (`thread close`).

### Machines

- Saved machines are Ferrite's: `ferrite machine list` (its reply's
  `result.machines`, `[{id, label, target}]`), with `[machines.X] ssh` in
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
