---
status: accepted
date: 2026-11-18
---

# One popup at a time, in one slot

Ferrite had five popups: the context menu (which is also the Factory's
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
  Session and preview, a card's Thread, the model picker's and band
  menu's rows and cursor, the attach menu's Pane.
- **One open path.** `show_popup` replaces whatever is up, with no exit
  for the one replaced. It ends a rename in progress and closes the
  Composer's text menu.
- **One close path.** `close_popup` closes the popup up and reports whether
  one was. Escape, a press outside, a pick, a sheet opening or closing and
  a popup with nothing left to draw all reach it.
- **Each popup says where the keyboard sits.** `Popup::keys` answers
  `Root`, its own field (the model picker's search line, the attach
  menu's list), or the Composer under it (a band menu, so tab still walks
  the band). The slot moves focus there while the popup is up and back to
  what had it once it closes; the focus pump has no case for any popup.
  While the root or a popup's own field holds the keys, the root's key
  context carries `Popup`, so the Cockpit's own keys stand down. Escape
  closes the popup. The arrows, Enter and the one-letter keys stay the
  popup's own. `interrupt` closes a popup before it does anything else, so
  no path stops a turn while one is up.
- **One outside press.** Every popup's surface closes it on a press
  outside, in the capture phase, so no handler under the press can keep it
  up. A press on the control that toggles a popup (the mode chip, the
  model chip, a band chip, a context menu's own button) is left to the
  control, which shuts it, and a press on a context menu's submenu or the
  picker's nested mode menu is the popup's. The attach menu is the kit's choice menu and takes its own
  outside press.
- **One rule for a popup with nothing to draw.** The root's render closes
  a popup that no longer stands: a Thread's popup whose Thread is not
  focused or is under a sheet, a Session's whose Session has gone, the
  checks card with no checks, the plan card with no plan, a Composer's
  popup whose Pane is no longer the one shown, a band menu whose draft
  has started. A page's dialog taking the keyboard closes any popup.
- **Sheets and popups.** A sheet opening or closing closes the popup up. A
  context menu opened from inside a sheet still opens over it: the first
  Escape closes the menu, the second the sheet.
- **One draw.** The popup up is drawn in one place, `deferred` at
  `FLOAT_PRIORITY`, over the sheets and the kit's toasts. The window's
  layers are one ladder (`ferrite_ui::layer`), bottom to top: sheet,
  toast, menu, lightbox. Every menu the operator opens, in the slot or
  out of it (a Composer's text menu, a transcript row's, a Decision's),
  is a menu there, so no toast ever hides one; a sheet stays under the
  toasts.
- **Out of the slot.** The Composer's text menus (`/` and `@`), which
  follow what is typed, the palette and shortcuts, the Thread details
  card, the lightbox and the transcript's own menus keep their own state.
  Where they closed a popup, they now call `close_popup`.

## The Composer's popups

The model picker, a draft's band menus and the attach (`+`) menu joined
the slot as `Popup::ModelPicker`, `Popup::Band` and `Popup::Attach`. The
permission mode menu opened from the picker's footer is no longer a
popup of its own: it nests inside the picker, so the first Escape closes
it and the second the picker, and it cannot outlive what it hangs from.
The picker's own state later moved into a view the slot holds
(`ModelPickerView`, ADR 0041); the slot keeps its Pane and the nested
mode menu.
The Composer's `popover` field keeps only the text menus, and the bounds
of the controls popups hang from moved into the slot. Five faults the
old shape hid are pinned in `cockpit/tests/popups.rs`: a sheet opening
left the attach menu up, Escape could not close it, Escape on it denied
a waiting Decision, its arrows and Enter did nothing, and a context menu
opened over it left both up.

## Menu buttons

A button that opens the context menu is drawn by one function,
`Doors::button` (`cockpit/menu_button.rs`), on the kit's `MenuButton`
(`ferrite-ui`). The caller gives the button's face, the menu's target,
the edge the menu hangs from and the tooltip's words; the function owns
the rest. The button records its bounds in the slot under a key from its
target, and the menu hangs under them, `MENU_UNDER_GAP` below, its edge
on the button's. It opens as the pointer goes down and from Enter or
Space. The menu remembers it came from its button (`from_door`), so
`Popup::owns` leaves a second press there to the button, which shuts it.
While the menu is up the button wears `CHIP_OPEN` and has no tooltip.
The nav, a view of its own, asks for the press with `NavEvent::Door`.
The nav's Filter and Factory, a nav card's Snooze, the view title's
chevron, the more doors of Files, Changes, Terminal and the PR, and the
Panel's `+` are drawn this way. A page card's `⋯` lives in the transcript,
a view in `ferrite-pane` that holds its own menus and cannot reach the
slot: it is the same kit `MenuButton`, and the transcript hangs the card's
menu under the bounds the button records, its right edge on the
button's. Before, three ways hung these menus: from
a canvas each button drew itself, at the pointer, over the button, or
from the Panel's own bounds map. A second press closed the menu and
opened it again, and the tooltip drew over the menu. Those faults are
pinned in `cockpit/tests/menu_buttons.rs`.

## Precedent

ADR 0042's sheet host is the same shape one layer up: one slot, a closed
enum, one open path that replaces, one close path. A popup has no veil and
no view of its own, so the slot is a plain value on the root, not a view.

## Words

A **popup** is a menu or small card that hangs from a point or a control
over the window, with no veil. _Avoid_ float and overlay.
