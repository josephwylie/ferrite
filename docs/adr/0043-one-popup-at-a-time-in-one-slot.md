---
status: accepted
date: 2026-11-18
---

# One popup at a time, in one slot

Ferrite has five popups: the context menu (which is also the Factory's
pickers, the nav's menus and Snooze), the permission mode menu, the plan
card, the checks card and the session controls card. They kept their open
state in eight fields on the Cockpit's root. Nothing closed them in one
place: about 45 writes closed one by hand, and seven openers each closed a
different set of the others. Escape was handled four ways and a press
outside two ways. Escape on the mode menu or the plan card stopped the
agent's turn, and with a Decision waiting on an empty Composer it denied
the Decision. The mode menu outlived the model picker it hung from, and
the plan card came back on a Thread focused again later.

## Decision

- **One slot holds the popup that is up.** `Popups`
  (`cockpit/popups.rs`) holds one popup, either up or fading out after a
  close. Its fields are private to that module, so nothing else can write
  the slot, and two popups up at once cannot be written down.
- **A closed enum.** `Popup` has one variant per kind, holding what that
  kind draws from: the context menu's rows and cursor, the mode menu's
  Session and preview, a card's Thread.
- **One open path.** `show_popup` replaces whatever is up, with no exit
  for the one replaced. It ends a rename in progress and closes the
  Composer's popover, apart from the model picker that a mode menu opened
  from its footer hangs from.
- **One close path.** `close_popup` closes the popup up and reports whether
  one was. Escape, a press outside, a pick, a sheet opening or closing and
  a popup with nothing left to draw all reach it.
- **A popup holds the keyboard.** While one is up the root holds focus and
  its key context carries `Popup`, so the Cockpit's own keys stand down and
  no field under it hears a key. Focus goes back to what had it once the
  popup closes. The root takes Escape and closes the popup. The arrows,
  Enter and the one-letter keys stay the context menu's. `interrupt` closes
  a popup before it does anything else, so no path stops a turn while one
  is up.
- **One outside press.** Every popup's surface closes it on a press
  outside, in the capture phase, so no handler under the press can keep it
  up. A press on the mode menu's own chip is left to the chip, which shuts
  it, and a press on a context menu's submenu is the menu's.
- **One rule for a popup with nothing to draw.** The root's render closes
  a popup that no longer stands: a Thread's popup whose Thread is not
  focused or is under a sheet, a Session's whose Session has gone, the
  checks card with no checks, the plan card with no plan, a mode menu
  whose model picker shut. A page's dialog taking the keyboard closes any
  popup.
- **Sheets and popups.** A sheet opening or closing closes the popup up. A
  context menu opened from inside a sheet still opens over it: the first
  Escape closes the menu, the second the sheet.
- **One draw.** The popup up is drawn in one place, `deferred` at
  `FLOAT_PRIORITY`, over the sheets.
- **Out of the slot.** The Composer's popover and the model picker, the
  attach menu, the palette and shortcuts, the Thread details card, the
  lightbox and the transcript's own menus keep their own state. Where they
  closed a popup, they now call `close_popup`.

## Precedent

ADR 0042's sheet host is the same shape one layer up: one slot, a closed
enum, one open path that replaces, one close path. A popup has no veil and
no view of its own, so the slot is a plain value on the root, not a view.

## Words

A **popup** is a menu or small card that hangs from a point or a control
over the window, with no veil. _Avoid_ float and overlay.
