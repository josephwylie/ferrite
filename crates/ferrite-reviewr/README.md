# ferrite-reviewr

[herdr-reviewr](https://github.com/persiyanov/herdr-reviewr)'s review core,
vendored for Ferrite's Review slot (ADR 0021). MIT, © 2026 Dmitry Persiyanov
([LICENSE](LICENSE)); not covered by Ferrite's own licence.

- Upstream: `https://github.com/persiyanov/herdr-reviewr`
- Taken at: `859dd5b11a5280fe282de56b52378b20a54fa10a` (2026-10-06), version
  0.46.0
- Edition 2024, MSRV 1.97, upstream's lints, `rustfmt.toml` and
  `clippy.toml`.

## What is here

Verbatim from upstream (`src/`): `azure_devops.rs`, `diff.rs`, `forge.rs`,
`git.rs`, `gitlab.rs`, `keymap.rs`, `log.rs`, `model.rs`, `proc.rs`,
`search.rs`, `snippet.rs`, `test_support.rs`, `text.rs`; and
`assets/*.tmTheme`, the syntax themes the highlighter's tests load (each under
its own licence: Catppuccin Mocha, Rosé Pine, ayu and Everforest MIT, Tokyo
Night Apache-2.0).

Changed from upstream:

- `highlight.rs`: adds `Highlighter::plain`, a highlighter with no syntax
  theme — Ferrite colours code with its own lexer.
- `export.rs`: keeps the comment formatting (`format_comment`, `format_all`)
  and its tests; the export destinations (`ExportTarget`, the clipboard, the
  herdr agent) and `send_failure` are gone. Ferrite's destination is the
  Composer.
- `lib.rs`: Ferrite's, declaring only the modules kept.

Ferrite's own:

- `ferrite.rs`: the adapter, in place of upstream's herdr adapter
  (`herdr.rs`, `herdr_socket.rs`): each scope's changeset and sides (after
  upstream's `world::scope_build` and `App::content_sides`), the file view,
  the turn baseline, the commit list, the comment anchor (upstream's
  `app::anchor`), the prompt text, the per-checkout search workers and the
  pull request read with its notes (upstream's `ui::pr_empty_msg`).
- `config.rs`, `theme.rs`, `wake.rs`: stand-ins for upstream modules of the
  same names that were dropped, holding only what the kept modules call
  (the forge host set and host grammar; the syntax theme choice; the search
  worker's sender).

Dropped: the terminal UI and its loop (`app.rs`, `ui.rs`, `input*`,
`wake.rs`, `theme.rs`, `roles.rs`, `markdown.rs`, `marks.rs`, `rendered.rs`,
`selection.rs`, `file_list.rs`, `browser.rs`, `editor.rs`, `schedule.rs`,
`world.rs`, `watch.rs`, `turn.rs`, `actions.rs`, `config.rs`, `main.rs`), the
herdr adapter (`herdr.rs`, `herdr_socket.rs`), the plugin manifest and
scripts. Ferrite draws the review with GPUI (`crates/ferrite/src/review.rs`)
and starts a turn's baseline itself (`ferrite::turn_started`).

Tests: every unit test of the kept modules, `tests/git_repo.rs` (its scope
reads go through the adapter's `changed` instead of upstream's world build)
and `tests/pr_candidates.rs` (without the two tests that write a reviewr
config file, which Ferrite does not read). The others drive the terminal UI or
herdr, or check upstream's own source style (`comment_shape.rs`).

## Updating

1. Clone upstream at the new commit next to this checkout; read its
   CHANGELOG.
2. For each verbatim file, `diff` upstream's old and new copies and apply the
   change here (they should apply clean). For `highlight.rs` and `export.rs`,
   apply upstream's change around the edits listed above.
3. When a kept module starts calling into a dropped one, extend the matching
   stand-in (or keep the module it now needs, verbatim).
4. When upstream's `world::scope_build`, `App::content_sides`, `app::anchor`
   or `ui::pr_empty_msg` changed, carry the change into `ferrite.rs`.
5. Record the new commit above, then `cargo test -p ferrite-reviewr`.
