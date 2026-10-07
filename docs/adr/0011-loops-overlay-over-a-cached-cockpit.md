# 0011 — Loops draw in an overlay over a cached Cockpit

Status: accepted 2026-10-07 (performance plan, Track A, phase A-2). Builds
on 0010.

## Context

0010 cut the loops' frames to the instants their picture changes, but each
of those frames still rebuilt the whole Cockpit: a notify redraws the view
and every view above it, and the Cockpit is one view holding the nav, every
Pane's chrome and every Composer. A focused, untouched window rebuilt it 33
times in 5s for the caret's fades; a working board with the keyboard on a
working Pane 155 times (the shimmer moves every tick). In the shipped app a
Cockpit frame costs ~11ms of CPU with 11 Panes, so the caret alone held a
third of a core.

## Decision

- The window's content is `CockpitWindow`: the Cockpit as a cached view,
  and `LoopsOverlay`, a sibling drawn after it (`loops_overlay.rs`).
- The Cockpit lays every loop out as before — the blinking caret and the
  text over it, the braille and working spinners, the working caption's
  shimmer, Ferrite's starting mark — paints it invisibly, and records its
  box, content mask, text style and opacity. The overlay builds the same
  element there each time the loop changes and declares itself on the pulse
  clock (0010). A blink redraws the overlay and replays the Cockpit.
- The Composer's line hands the overlay its block and its text together:
  the block is painted under the glyphs, so the overlay paints both, with
  the line's own shaping and paint.
- The Cockpit is cached so that it behaves as when its parent redrew it
  (vendored gpui, `ViewElement::tracking_reads`): it is redrawn when it, a
  view in it, or any entity it read in its last render (the kit Root's
  layers among them) is notified, and its nested caches (the transcripts)
  survive its redraws. Window state (activation, focus) refreshes it; time
  reaches it through the clock riders and the sweep (0010).
- A loop inside a `deferred` float, under reduced motion, in a held capture
  or during a drag is drawn in place: the overlay paints above the main
  tree and below every float. `FERRITE_LOOPS_OVERLAY=0` draws every loop in
  place and leaves the Cockpit uncached.
- Elapsed and age labels stay in the Cockpit and redraw it at their
  rollover (once a second per working Thread): their text changes the
  row's layout.

## Consequences

- Focused idle window: 33 → 1 Cockpit render in 5s; a pending Decision the
  same. A working board: 155 → 14–20 with the keyboard on the working Pane,
  105 → ~15 on an idle one.
- Pixel parity is a capture, not a claim: `ferrite --loops-parity <dir>`
  (feature `visual-reference`) draws one scene with and without the overlay
  at 66 instants and compares framebuffers; all 66 are identical.
- Anything the Cockpit draws from state that changes without a notify (a
  global, shared cell, the clock) must reach it another way; such state was
  previously masked by the Cockpit being rebuilt on every frame. One such
  bug was found and fixed (a render that notified itself).
