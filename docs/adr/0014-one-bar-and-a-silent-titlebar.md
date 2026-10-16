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

- **The titlebar band says nothing, and lives in the nav.** It is
  `WIN_CHROME_H` tall at the top of the nav column: the host traffic lights
  sit in it on macOS, and the window drags by it. It draws no location, no
  doors and no cell of its own. The board beside it starts at the window's
  top, so Pane heads (and Solo's transcript) take the band's height back.
  Solo still has no head; the nav's selected row names the Thread.
  On macOS the window sets gpui's `app_owns_titlebar_drag`: otherwise
  AppKit moves the window from any press-and-drag in the titlebar's
  height, and a Pane head is dragged to rearrange the board, a transcript
  to select text. The nav's top chrome starts the window drag itself
  (`start_window_move`; a double-click takes the system's titlebar action).
  Two places keep the band across the board: on Windows, which has no
  lights but needs its caption buttons at the window's top-right and a drag
  region the whole width; and on macOS while the nav is folded to its rail,
  narrower than the lights, so they overhang the plane rather than the top
  Pane's head.
  (This revises the first version of this decision, which kept the band's
  height across the whole window because AppKit owned the drag.)
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
- With the nav open on macOS the window drags only by the nav's top
  chrome (and, folded, by the band across the board), not by the board's
  top edge.
- `nav::chrome_band`, the folded titlebar cell and its cross-fade, the
  titlebar's `Location`/`Title` and the `⌘K` door are gone, with their
  tokens.
- The terminal-native prototype (`spikes/terminal-native/index.html`) is
  updated to match, so the parity scenes compare against the same chrome.
- Item 4 (the needs-you strip, still carrying its `⌘D`) and item 5 (nav
  heading styles) are a separate change.
