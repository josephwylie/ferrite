# sw-board

herdr-projects 1.0's `crates/board`: the board's screen: its state (`board/state.rs`), what an event does (`board/update/`) and what it shows (`board/view/`), and the terminal runtime (`board/run/`) Ferrite does not run: Ferrite draws the board in GPUI
(`crates/ferrite/src/board`), reusing `run/start.rs`'s `view` and `run/prefs.rs`.

Vendored verbatim from [herdr-projects](https://github.com/eliasstravik/herdr-projects)
at commit `c019063b65b3ff6a68688345b9f304533c8fb83c` (1.0.1+8), MIT licensed,
Copyright (c) 2026 Elias Stravik — see `LICENSE`. The vendored crates, their
seams and how upstream updates merge are in `../ferrite-projects-core/README.md`
and `docs/adr/0031-projects-1-0-vendored.md`.

Ferrite does not draw what only a terminal board has: the first-run setup
(`view/setup.rs`), the held-size warning (`view/mod.rs`'s `held_lines`),
OSC 7501 records, attaching the terminal to an agent, the web UI's offer
and loading a new binary in place; nor herdr's dots, an input method's
composition, `Ctx::own_key` or the keeper's start (ADR 0031, Seam 3).
Ferrite's own tests of the crate are `tests/ferrite_*.rs`.
