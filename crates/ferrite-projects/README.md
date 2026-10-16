# herdr-projects (shipped as `ferrite-projects`)

herdr-projects 1.0's command line crate: `src/` (`cli/`, the `sw` library
that passes on every crate below it as its own, and the docs generator) and
its tests (`tests/`, one test program over `sw`). Its binary is what
agents, hooks and Ferrite's Server run.

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
  the `[[bin]]` is named `ferrite-projects`; `sw-board` in place of
  `sw-web`.
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
  Ferrite's Server runs it once at every start, before the keeper. Like
  any command it runs upstream's start first (`take_over::at_start`, the
  home folder's move, the old copy, the settings' move); then it prints the
  line upstream keeps for the human until it is said, once, for the
  Server's toast (with `--json`, `{"said": …}`, empty once said).
- **Tests.** `CARGO_BIN_EXE_herdr-projects` is `CARGO_BIN_EXE_ferrite-projects`
  throughout. Not taken: the web board's tests (`web_*.rs` but `web_link`
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

`scripts/install-app.sh` builds it (`cargo build -p herdr-projects --bin
ferrite-projects`) and copies it into the app bundle's `Contents/MacOS`,
beside `ferrite`, signed with the app. The Server finds it there (or at
`FERRITE_PROJECTS_BIN`), and so does the code inside the app
(`store::files::bin`), so the launcher agents and hooks are handed runs the
shipped program. It is Unix-only, as upstream is.

Tested in CI on macOS: `cargo test -p sw-core -p sw-flow -p sw-board -p
herdr-projects`.
