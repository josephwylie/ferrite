---
status: accepted
date: 2026-10-15
---

# One bar per window, and a titlebar that says nothing

Follows the Herdr benchmark (`docs/research/herdr-benchmark-2026-10-15.md`,
"UI cleanliness" items 1, 2, 3 and 6).

## Context

The terminal-native chrome said most things twice. The titlebar band
restated the focused Thread's Project, title, state and branch (already in
its nav row) and carried `⌘K commands`, the sidebar toggle and the bell.
At the foot, the focused Pane's status line (mode, model, `ctx`, changed
files, `⇧⇥ mode · ? shortcuts`) sat directly on top of the window's bottom
bar (session, view tabs, usage, clock): two bars stacked. The placeholder
listed keys (`/ for commands · drop or paste images`, `⏎ sends when the turn
ends`), and every transcript opened on a three-line banner under a large
logo. Herdr, the calmer reference, has no top toolbar and one status line.

## Decision

- **The titlebar band says nothing.** It stays `WIN_CHROME_H` tall: on macOS
  AppKit drags the window by it and the host traffic lights sit in it, so a
  Pane head drawn there could not be dragged or clicked; on Windows it is
  the drag region and the caption buttons. It draws no location, no doors
  and no cell of its own over the nav: the nav's chrome runs from the
  window's top, and folded, the lights sit on the plane. Solo still has no
  head; the nav's selected row names the Thread.
- **One bar per window: the bottom bar.** Left to right: the sidebar toggle,
  the bell (its list now opens upward) and, off macOS, the Settings gear;
  `ferrite`; the view tabs; the **focused Pane's status line**; the stretch;
  usage; `?`; the clock. The status line was already the focused Pane's
  alone (an unfocused board Pane drew none), so moving it to the bar keeps
  per-Pane information per Pane: it follows focus, and every Pane's own
  Composer is now just its band. A draft's setup chips ride the same slot.
  Its popovers still open over the focused Pane's Composer.
- **`?` is the one way in to the keys.** No `⌘K commands`, no `⇧⇥ mode · ?
  shortcuts`. Placeholders are a couple of words (`Steer this thread`,
  `Queue a follow-up`, `Start a thread`). A landed prediction keeps
  `⇥ accept`: without it the ghost reads as typed text.
- **The banner is one muted line**: `~/ferrite on dev · started 7:18 pm`,
  the checkout still a path target. The title, provider and model are said
  by the nav row, the head and the status line.

## Consequences

- With the nav folded nothing on screen names a Solo Thread; the palette
  and `⌘1…9` still reach it. Accepted for the calmer chrome.
- The Composer's fixed height no longer depends on focus, so moving focus
  on a board resizes no Pane body.
- No chrome shows the focused Thread's working clock any more, so the frame
  stops riding it (ADR 0012's clock riders): the frame wakes for the bar's
  minute and the bell's ages only.
- `nav::chrome_band`, the folded titlebar cell and its cross-fade, the
  titlebar's `Location`/`Title` and the `⌘K` door are gone, with their
  tokens.
- The terminal-native prototype (`spikes/terminal-native/index.html`) is
  updated to match, so the parity scenes compare against the same chrome.
- Item 4 (the needs-you strip, still carrying its `⌘D`) and item 5 (nav
  heading styles) are a separate change.
