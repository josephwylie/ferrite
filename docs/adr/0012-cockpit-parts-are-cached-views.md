# 0012 — The Cockpit's nav and Panes are cached parts

Status: accepted 2026-10-07 (performance plan, Track A, phase A-3). Builds
on 0011.

## Context

0011 cached the Cockpit as one view, so a frame that changed only a loop
replayed it. Every other change still rebuilt all of it: a keystroke, a
stream delta, a working clock's second, a nav hover fade, an arrow in a
model picker each rebuilt every Pane on the board. Typing 20 characters
into one Pane of a four-Pane Group built each of the four 21 times; a nav
hover sweep built each Pane 135 times in 5s.

## Decision

- Under the loops overlay the nav's rows and each Pane's cell are a cached
  view of their own, `PartView` (`cockpit/parts.rs`), mounted in the box the
  Cockpit's layout gave the inline part. A part's view draws by calling back
  into the Cockpit (`render_part`), so every element and listener in it is
  the Cockpit's as before, and it tracks what it reads (vendored gpui,
  `tracking_reads`).
- Conservative by default: every part reads the Cockpit, so the Cockpit's
  own notify still rebuilds them all. Only the hot paths are precise:
  `notify_part` (a keystroke its Pane, a stream delta its Pane and the nav,
  a nav hover the nav), `notify_thread_pane`, and `notify_frame`, which
  notifies a `FrameTick` entity only the Cockpit reads, redrawing the frame
  around the parts (titlebar, bottom bar, floats) and none of them. The
  watchdog sweep is precise only when nothing shared moved (account
  limits, needs-you, unread, attention, restarts, models); otherwise it
  notifies the Cockpit.
- Each clock rider rides the view that shows it: a working Pane's clock
  its Pane, the nav's rows the nav, the focused clock, notices and the
  bottom bar's wall-clock minute the frame.
- Loops are owned per view in the overlay (0011): a part replayed from its
  cache keeps its marks. A float's loops (the palette's caret) are drawn in
  an overlay layer at the float's `deferred` priority, so a blink redraws
  the overlay, not the Cockpit that renders the float.
- Choice menus and a settled transcript notify the view that draws them,
  never `window.refresh()`, which re-renders every cached view in the
  window.
- Without the overlay (`FERRITE_LOOPS_OVERLAY=0`) the parts are drawn
  inline and every precise notify falls back to the Cockpit's.

## Consequences

- Four-Pane Group, Pane builds per Pane (before → after): typing 20
  characters 21 each → 20 for the typed-into Pane, 0 for its siblings;
  streaming 20 deltas 43 each → 41–44 for the streaming Pane, 0 for the
  rest; a working clock over 3s 8 each → 6–10 for the working Pane, 0 for
  the rest; 4 picker arrows 5 each → 4, 0, 0, 0; an open palette for 5s
  30 Cockpit renders and 30 builds each → 0 and 0.
- A nav hover sweep: Pane builds 135 → 0, nav builds 135 → 120. The
  Cockpit still renders on each fade frame (115 → 100 in 5s): gpui redraws
  a cached view's ancestors with it, so a part's notify re-renders the
  Cockpit's frame and replays the other parts. That render is cheap (the
  frame's elements only) but the frame still pays gpui's replay of the
  whole window: in the test build the sweep's 120 frames take 669ms
  instead of 918ms, where a frame that redraws only the overlay costs ~3ms.
  Drawing only the nav would need gpui to replay a clean ancestor around a
  dirty child.
- Root render counts no longer measure work; the render-performance tests
  meter per-Pane builds (`cockpit::drawn`), and the stale-chrome tests check
  that what each part last drew follows every way chrome changes.
- A precise path that changes something another part shows leaves that
  part stale. Anything shared goes through the Cockpit's notify; new
  precise paths need a stale-chrome test.
- Pixel parity holds: `ferrite --loops-parity` compares 66 instants with
  and without the overlay (and parts), with and without the palette open;
  all are identical.
