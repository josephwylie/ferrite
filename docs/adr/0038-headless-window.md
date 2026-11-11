---
status: accepted
date: 2026-11-11
---

# A headless window the Server's API drives

Follows the Server's API (ADR 0027) and the client contract (ADR 0025).

## Context

An agent checking a change to Ferrite's UI has to see the real window
respond to real keys and clicks. A visible window fails the operator
working beside it. It covers their screen and takes focus. Its input
needs the Accessibility grant. GPUI stops painting it when it is covered,
so a capture can show a stale frame. The live reference (`--live-reference`)
already drives the production Cockpit off-screen through gpui's test
support, but only from its own script and only in-process. A spike
(`spike/headless-window`) showed the same window attached to a real
`ferrite server` takes the Server's updates, real keys and clicks, and
2880x1800 captures, with no Dock icon, no frontmost window and no CG
window.

## Decision

- **`ferrite --headless`** opens the production window (`CockpitView` in
  `CockpitWindow` and the kit Root, 1440x900, the operator's settings and
  keys) in a `HeadlessAppContext`. It attaches to this Store's Server as
  any window does and never appears on screen. A loop moves the window's
  clock in real time. The window draws only when something in it changed,
  as on screen; a shot draws its frame first.
- **One driver.** `ferrite_cockpit::headless::Driver` holds the input,
  capture and clock primitives: keys, typing, clicks at a `debug_selector`
  or a point, bounds, and shots. The live reference and `--headless` both
  use it.
- **Behind a feature.** The primitives exist only under `gpui/test-support`,
  so they sit behind a `headless` cargo feature that release builds never
  turn on. `visual-reference` builds on it. It is not the other way round,
  because `visual-reference` also turns off the window's `gh` reads, its
  free-disk check and its chimes, and a headless window must behave as
  the production one does.
- **Driven through the API.** `window.key`, `window.type`, `window.click`,
  `window.find` and `window.shot` are API operations like the others.
  Their parameters are `ferrite_core::drive`'s action types, so the CLI
  reads them from the schema. The Server hands each one to the one
  attached window whose hello declared the `driven` capability, as a
  `drive` with a request number. That window's `driven` answers the
  caller. `server/drive.rs` keeps the request numbers the way
  `server/render.rs` keeps the visuals owed. Only `--headless` declares
  `driven`; a normal window is never handed an action. With none attached,
  the caller is refused `no_headless_window`. If the window goes before it
  answers, the caller is refused `failed`: the action may have happened.
- **Only on a throwaway Store.** `--headless` refuses a `FERRITE_STORE`
  outside the temp directory, the operator's included. A driven window on
  their Server would let any API caller type into their Threads and answer
  their Decisions.
- `window.shot` names a path on the window's computer, and the window
  writes the PNG. Its answer carries the path, the size and the scale, not
  the image.

## Consequences

- An agent verifies a UI change by running `ferrite server` and
  `ferrite --headless` on a throwaway Store, then using the `ferrite window`
  commands, without disturbing the operator.
- The headless window is the production window, with its throwaway
  Store's settings. It chimes and posts notifications only as those say.
- Each new kind of input is a `drive::Action` variant, an operation, and a
  case in `headless::act`.
- A `debug_selector` is the stable handle for a click. An element without
  one is reached by point, which moves when the layout does.
