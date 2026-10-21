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

- **The titlebar band says nothing and remains draggable across the window.**
  On macOS and Windows an empty `WIN_CHROME_H` strip spans the top;
  the host traffic lights sit over its nav side on macOS. The board starts
  below it, so Pane heads, transcript text and controls keep their own mouse
  gestures. Expanding or folding the nav never removes the drag region.
  macOS keeps `app_owns_titlebar_drag`: the empty strip explicitly starts
  the native window move, and a double-click uses the system titlebar action.
  Menus and other overlays disable the strip while they cover it. Windows
  keeps its caption controls and top resize edge.
  This restores full-width dragging after the nav-only version made the
  window appear immovable from its top edge.
- **One bar per window: the bottom bar.** Left to right: the sidebar toggle,
  the bell (its list now opens upward) and, off macOS, the Settings gear;
  `ferrite`; the view tabs; the **focused Pane's status line**; the stretch;
  usage; `?`; the clock. The status line was already the focused Pane's
  alone (an unfocused board Pane drew none), so moving it to the bar keeps
  per-Pane information per Pane: it follows focus, and every Pane's own
  Composer is now just its band. A draft's setup chips ride the same slot.
  Its popovers (the pickers, the `ctx` and changed-files cards, a draft's
  Project and Workspace menus) hang off their triggers in the bar, the way
  the bell's list does: their foot rests on the bar, their right edge
  inside the window. They may cover the nav or a neighbouring Pane — they
  belong to the bar, not to a Pane. A Pane's own cards (the plan, checks,
  session controls) and its `/` and `@` menus still hang off its head or
  Composer.
- **`?` is the one way in to the keys.** No `⌘K commands`, no `⇧⇥ mode · ?
  shortcuts`. Placeholders are a couple of words (`Steer this thread`,
  `Queue a follow-up`, `Answer above`, `Start a thread`). A landed prediction keeps
  `⇥ accept`: without it the ghost reads as typed text. An open menu keeps
  its key footer (`↑↓ select · ↵ run · esc dismiss`, the Project menu's
  `type a path`): it shows only inside a surface the operator just opened,
  names the keys that surface alone answers, and leaves with it. What goes
  is ambient hinting — keys printed on chrome that is always on screen.
- **The banner is one muted line**: `~/ferrite on dev · started 7:18 pm`,
  the checkout still a path target. The title, provider and model are said
  by the nav row, the head and the status line.

## Consequences

- With the nav folded nothing on screen names a Solo Thread; the palette
  and `⌘1…9` still reach it. Accepted for the calmer chrome.
- On a board, an unfocused Pane's mode, model and `ctx` show nowhere — as
  before this change, when an unfocused Pane drew no status line either.
  The spec's "per Pane but minimal" is met by the Pane's own head (its
  title and state word) and by the bar naming whichever Pane has focus;
  a second, per-Pane status line would bring the stacked bars back.
- The Composer's fixed height no longer depends on focus, so moving focus
  on a board resizes no Pane body.
- No chrome shows the focused Thread's working clock any more, so the frame
  stops riding it (ADR 0012's clock riders): the frame wakes for the bar's
  minute and the bell's ages only.
- The empty top strip uses 32 pixels above the board on macOS and Windows;
  its consistent window-drag target takes priority over reclaiming that space.
- `nav::chrome_band`, the folded titlebar cell and its cross-fade, the
  titlebar's `Location`/`Title` and the `⌘K` door are gone, with their
  tokens.
- The terminal-native prototype (`spikes/terminal-native/index.html`) is
  updated to match, so the parity scenes compare against the same chrome.
- Item 4 (the needs-you strip, still carrying its `⌘D`) and item 5 (nav
  heading styles) are a separate change.
