# ferrite-focus-notify

Ferrite's system-notification delivery: a vendored copy of
[herdr-focus-notify](https://github.com/yankewei/herdr-focus-notify) by
yankewei, MIT (declared in upstream's `Cargo.toml`; the full text is in
`LICENSE`). Bundled agent icons keep their own notice in
`assets/icons/NOTICE.md`.

- Upstream commit: `94e8e1394e1bdbaa3dd55d4d83e0e3f98ecab743` (v0.8.0,
  2026-10-11).
- The first Ferrite commit touching this crate ("Vendor herdr-focus-notify
  verbatim at 94e8e13") is upstream unmodified; diff against it to see every
  local change, and merge a newer upstream onto it.

## What it does in Ferrite

ADR 0005 and 0017 decide what is a Notice and when the bell rings. This crate
decides how a rung alert reaches the operator outside the window and how its
click lands: a clickable alerter notification on macOS (title
`{Blocked|Done} · {Thread}`, the provider and git state as subtitle, the
provider's icon), skipped only when the Thread's Pane is focused and Ferrite is
frontmost (`notification_decision`). A click focuses the Thread in the app.
See ADR 0005's 2026-10-15 update.

## Local changes

- `focus.rs` is the Ferrite adapter (the Herdr adapter it replaces read
  Herdr's CLI and socket). It keeps upstream's `NotificationDecision` table
  and builds each notification from a Herdr-shaped event, so `event.rs`'s copy
  and icons are upstream's. Since a click comes back to the process that
  posted it, `Notifier::withdraw_all` takes back what is still up as Ferrite
  quits.
- `script.rs`: a click prints `focus` for Ferrite, which runs the script and
  waits on it, instead of exec'ing upstream's binary with `--focus-pane`; the
  Herdr binary and the pruning of Herdr workspace bindings are gone.
- `state.rs`: the state directory is `$FERRITE_FOCUS_NOTIFY_STATE_DIR`, else
  `$TMPDIR/ferrite-focus-notify`; Herdr workspace pruning is gone.
- `util.rs`: notification groups are `ferrite-…`.
- `executable.rs`: Herdr binary lookup is gone.
- Gone entirely: `main.rs` and `cli.rs` (the Herdr plugin entry point),
  `terminal/` (raising the iTerm2 or kitty window running a Herdr client),
  `tests/cli_test.rs` (the plugin binary's process contract; the parts that
  still apply, the git lookup budget among them, are ported as the adapter's
  tests), `herdr-plugin.toml`, and upstream's READMEs. Upstream's
  `CHANGELOG.md` stays, to tell a merge which versions it brings.
- New: `lib.rs`, this README, `LICENSE`. Helpers only Herdr paths called
  (terminal learning, focus-origin markers) stay, so merges stay small.

Upstream supports macOS only; on other platforms only `notification_decision`
is built and Ferrite posts through GPUI.
