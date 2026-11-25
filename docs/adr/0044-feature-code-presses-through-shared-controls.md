---
status: accepted
date: 2026-11-18
---

# Feature code presses through shared controls

Every button in Ferrite should look and behave the same, and that holds
only if feature code reaches for one shared control, the way a web app
has one Button component. `ferrite-ui` has those controls. Feature code
mostly does not use them. Clippy counts 299 raw presses in 71 files: 153
`on_click`, 139 `on_mouse_down` and 7 `on_mouse_up`, each on a `div()` or
similar. Cockpit has 243 of them in 51 files, Pane 46 in 16, the frame 9
in 3 and the terminal 1. Each one is a control built by hand, a little
different from the last. Agents write most of the code here, and advice
in a document did not stop them, so the build enforces the rule.

## Decision

- **The rule.** Each feature crate (`ferrite`, `ferrite-cockpit`,
  `ferrite-frame`, `ferrite-pane`, `ferrite-review`, `ferrite-factory`,
  `ferrite-tools`, `ferrite-terminal`) carries the same `clippy.toml`. Its
  `disallowed-methods` lists GPUI's press methods, so clippy flags each
  raw press and names this ADR. `ferrite-ui` has no such file: it is where
  presses are built. A test keeps the eight files identical.
- **What is on the list.** The lint sees which method is called, not
  why. So the list is every method that puts a press handler on the
  element itself: `on_click`, `on_aux_click`, `on_mouse_down`,
  `on_any_mouse_down` and `on_mouse_up`, in both their fluent and
  `Interactivity` forms, and `InteractiveText::on_click`. The unused ones
  are listed so that an agent cannot sidestep the rule by switching to a
  sibling method. The list leaves out capture-phase and `_out` handlers,
  which are a popup's press outside (ADR 0043), and mouse move, drag,
  drop, scroll and pressure, which are not presses. A mouse down that
  starts a drag, like the titlebar's, counts. It goes away when a shared
  control owns the drag too.
- **The ratchet.** No call site moves in this change. CI's `macOS
  (Metal)` job runs clippy on the feature crates with every other lint
  allowed and pipes its JSON to `scripts/check_raw_presses.py`. The
  script counts the flagged presses per file and compares them with
  `scripts/raw-presses.json`. It fails when a file has more than its
  baseline, or when a file missing from the baseline has any, and it
  names the file and lines. It also fails if clippy did not finish or a
  listed method no longer resolves, because either would leave a count
  that looks clean but is not. The count can only go down.
- **Lowering it.** Move a press onto a shared control. The script then
  prints which entries can be lowered. Rerun it with `--write` and commit
  the baseline. No `#[allow]` or `#[expect]` goes in source: the baseline
  is the only record of the debt.

## What it does not catch

- Code that does not compile on macOS with default features: `cfg(windows)`
  code and code behind `cef`, `headless`, `visual-reference` or
  `browser-smoke`. Clippy runs on macOS with default features.
- A control built by hand inside `ferrite-ui` itself.
- A press reached through a method not on the list: a capture-phase or
  `_out` handler used as a press, or a kit component's own `on_click`
  called from feature code (the kit's `Button` used directly instead of
  `ferrite-ui`'s wrapper).
- A new feature crate, until it gets the `clippy.toml` and joins the
  CI step's package list.
- Tests, which the step does not lint.
