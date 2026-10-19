# herdr-projects (shipped as `ferrite-projects`)

herdr-projects 1.0's command line crate: `src/` (`cli/`, the `sw` library
that passes on every crate below it as its own, and the docs generator) and
its tests (`tests/`, one test program over `sw`). Its binary is what
agents, hooks and Ferrite's Server run (see "Shipped").

## Vendored

Copied verbatim from [herdr-projects](https://github.com/eliasstravik/herdr-projects)
at commit `c019063b65b3ff6a68688345b9f304533c8fb83c` (`v1.0.1-8`), MIT
licensed, Copyright (c) 2026 Elias Stravik (see `LICENSE`). The package and
library keep upstream's names (`herdr-projects`, `sw`), its edition (2024)
and its formatting (`rustfmt.toml` turns formatting off). The crates below
it are `../ferrite-projects-core`, `-flow` and `-board`; how the seams are
drawn and how upstream updates merge: `../ferrite-projects-core/README.md`
and `docs/adr/0031-projects-1-0-vendored.md`.

Upstream's text names itself `herdr-projects` (help, refusals, `version`):
it has no one name constant to set, so it is left as it is.

## Ferrite changes

Each is marked `Ferrite:` in a comment.

- **`Cargo.toml`**: upstream's `[workspace.package]` values written in;
  `sw-board` in place of `sw-web`. The `[[bin]]` is `ferrite-projects`,
  the name it ships by, so the tests that run it name it
  `CARGO_BIN_EXE_ferrite-projects`.
- **No web crate.** `crates/web` (the board inside herdr-web-ui, a herdr
  plugin that puts herdr's panes in a browser) is not taken: Ferrite has no
  herdr-web-ui and its board is a GPUI view. `src/lib.rs` re-exports
  `sw_board` instead of `sw_web`, so `sw::webui` is `flow`'s own module
  (finding the web UI, its link, its front door), which finds no web UI
  under Ferrite: `web-ui` answers as upstream does where none is installed.
  `serve` opens no front door (`cli/mod.rs`).
- **`install` and `update`** answer that Ferrite ships the program and
  updates it with the app, and change nothing (`cli/ferrite.rs`); with
  `--json`, as `{"said": …}`. `doctor`
  is upstream's: it takes over older installs and says where things stand.
- **`take-over`** (`cli/ferrite.rs`, and its flags in `cli/flags.rs`):
  for Ferrite's Server to run at every start, before the keeper. Like
  any command it runs upstream's start first (`take_over::at_start`, the
  home folder's move, the old copy, the settings' move); then it prints the
  line upstream keeps for the human until it is said, once, for the
  Server's toast (with `--json`, `{"said": …}`, empty once said).
- **Background jobs** (`flow/runner.rs`, `housekeeping.rs`, `review_page.rs`):
  detached runners, notifications and the evidence server keep a waiter until
  they exit. Detaching their session alone left zombies under the long-lived
  keeper and window, consuming process slots. The waiter starts before its
  child so a thread-creation failure cannot orphan an unowned child.
- **Coordinator handovers** (`flow/coordinator/{state,pump,look}.rs`):
  follow the native Thread's Provider before reading its conversation or
  restarting it. The Ferrite adapter remembers that Provider in its pane
  and scopes revive overrides with `launch_provider`, so stale Claude flags
  cannot replace a Codex Thread's saved launch (or the reverse).
- **Completed deliveries** (`flow/actions/talk.rs`, `start.rs`): ordinary `tell`
  refuses to reopen completed deliveries. `tell --follow-up` and explicit
  delegation still start genuine new work, retaining the previous PR, merge and
  review in `deliveries` before resetting the current delivery.
- **Merged reconciliation** (`flow/actions/reconcile_merged.rs`, `cli/tasks.rs`):
  `reconcile-merged <id> --pr <url> --review-head <full-sha> [--check]` repairs
  stopped cleanup-only reopenings. It requires the exact task branch, origin
  repository, GitHub MERGED head and commit, the original passing review, a clean
  worktree with no novel unmerged commits, and idle worker/reviewer and runner.
  Unknown or conflicting evidence refuses the repair. `--check` validates without
  task changes; repair saves the pre-repair task and GitHub evidence under the
  task's `reconciliation/` directory, then restores the original delivery under
  the normal task lock. Review history and counters are retained. Neither mode
  starts reviews, dependent tasks or cleanup; source work and worktree stay in
  place for separately coordinated cleanup. Review/merge gates are unchanged.
- **Tests.** Not taken: the web board's tests (`web_*.rs` but `web_link`
  and `web_link_ask`, and `web` and `web_as` in `common/board.rs`,
  `answered` and `ask_again` in `far/mod.rs`); a test that also checked the
  web board keeps its terminal part, the rest marked. Not taken either, as
  they test upstream's own repository rather than the program:
  `check_script`, `check_script_since`, `guard`, `guard_rest`, `linking`,
  `warm_start` (its scripts, linker and source guard), `bin/run.sh`'s part
  of `install.rs` (the plugin's launcher) and `herdr-plugin.toml`'s lines of
  `board_stay.rs`. `keys_list.rs` reads the sibling crates as upstream's
  `crates/`. `tests/ferrite.rs` tests Ferrite's changes, among them the
  take-over of Ferrite's own 0.2 on a scratch home folder.
- **`docs/`** holds only the made pages the docs generator and
  `keys_list.rs` read (`keys.md`, `settings.md`, `agents.md`), as upstream
  wrote them.

## Shipped

`scripts/install-app.sh` builds it (`-p herdr-projects --bin
ferrite-projects`) and copies it into the app bundle's `Contents/MacOS`
as `ferrite-projects`, beside `ferrite`, signed with the app. The Server
runs its `take-over` at start, then its startup hook, which starts the
keeper, and the code inside the app hands agents and hooks that program
(`store::files::bin`, beside `ferrite` or at `FERRITE_PROJECTS_BIN`). It
is Unix-only, as upstream is.

Tested in CI on macOS: `cargo test -p sw-core -p sw-flow -p sw-board -p
herdr-projects`.
