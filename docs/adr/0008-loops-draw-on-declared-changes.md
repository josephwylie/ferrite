# 0008 — Loops draw on declared changes

Status: accepted 2026-10-07 (performance plan, Track A, phase A-1).

## Context

Every loop on screen (the Composer's soft caret blink, the braille and
working spinners, the working caption's shimmer, Ferrite's starting mark)
leased its view onto one pulse clock that notified it every 33ms. A notify
marks the view and all its ancestors dirty, and the Cockpit is one large
view, so a focused, untouched window rebuilt the whole Cockpit 157 times in
5s. Most of those frames repainted pixels already on screen: the caret holds
solid for 45% of its 1.1s turn and dim for 40%; a spinner holds each glyph
for a whole 80 or 120ms step. Hover blends refreshed the whole window
(`Window::refresh`), which also re-renders every cached transcript.

## Decision

A loop declares when its picture can next change, and the clock wakes only
then (`ferrite_core::cadence`, pure and headless beside `clock`; its gpui
adapter is `motion`'s pulse clock):

- A loop is a phase over a period from an origin, moving `Continuous`ly, in
  equal `Frames`, or only inside `Spans` of its phase (the caret's fall and
  rise, the starting mark's pull and snap).
- The wake snaps to the same 33ms grid, from the same epoch, the fixed rate
  drew on, and each view is woken once per declaration. Every frame drawn is
  the frame the fixed rate drew at that instant; only repeats are dropped.
  The soft blink keeps its look; no hard blink is needed.
- Text that changes with time but is no loop (a working clock's seconds, a
  nav row's age) *rides*: it is woken on the grid only while a loop keeps the
  clock running, as the fixed rate redrew it, and is otherwise brought up to
  date by the 2s sweep, which now redraws only for a changed fact, a working
  Thread, or an age turning over.
- A live tool call's trail second wakes its transcript on the same grid
  whatever else runs (a clock is no motion).
- An `on_hover` listener learns the view that painted it (a vendored gpui
  patch, `Window::hover_listener_view`) and notifies that view instead of
  refreshing the window; a blend mid-flight in a cached view is drawn again
  with the root's next frame. A blend is forgotten once unread for 2s, not
  after two frames, so a cached transcript's hovered link keeps its
  underline.

Reduced motion and held captures keep their semantics: a loop holds its
start and declares nothing.

## Consequences

- Focused idle window: 157 → 33 Cockpit renders in 5s (the caret's fades
  alone). A working board with the keyboard on an idle Pane: 157 → 107. A
  focused working Pane still draws every tick while its shimmer's crest
  moves (155): its pixels really change.
- The remaining cost is that a loop's notify rebuilds the Cockpit. Taking the
  loops off the Cockpit (an overlay view, A-2) is the next step; this module
  is what it schedules with.
- A view that stops painting a loop is woken once more, at its last declared
  change, before the clock parks.
