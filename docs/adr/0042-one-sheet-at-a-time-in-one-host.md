---
status: accepted
date: 2026-11-18
---

# One sheet at a time, in one host

Ferrite has five sheets: Settings, the Project card, the Background work
dialog, the New task sheet and the Rework/Direct sheet. Each drew its own
veil and wrote its own outside press, fade, focus and Escape. Only
Settings and the Project card closed each other, by hand, in their
openers. Two of the five have no Escape.

## Decision

- **One host holds the sheet that is up.** `Sheets`
  (`cockpit/sheets.rs`) is a view on the Cockpit's root. It holds one
  sheet, either up or fading out after a close. Showing a sheet replaces
  the one up at once, with no exit. Two sheets up at once cannot be
  written down.
- **A closed enum, not a trait object.** `Sheet` has one variant per
  sheet, each holding that sheet's view. The set is small and known, so
  the enum gives typed access with no downcast.
- **A sheet's view exists only while it is drawn.** It is made when the
  sheet opens and dropped once its exit has run. It keeps no "am I open"
  state and no reset-on-open code. Settings opens on its first page with
  an empty search, and the Project card with an empty name, because each
  is new.
- **The host draws the veil, takes the press outside the card and runs
  the entrance and the exit, once.** A sheet hands the host only its card,
  and Settings also hands the connection form, which the host draws above
  the veil and the card. The host asks for the card in its own render
  rather than drawing the sheet as a child view, because the entrance and
  the exit style the card itself.
- **One close path, on the root.** `hide_sheet` closes whatever is up and
  does what goes with that sheet (closing Settings drops the connection
  being edited). Escape, a press on the veil and a sheet's own close
  button all reach it. Escape is one step of the root's `interrupt` chain,
  wherever focus is. An editor open inside Settings closes before it.
- **Settings and the Project card are the first two.** The Background
  work dialog, the New task sheet and the Rework/Direct sheet move in
  later, one per change.

## Precedent

The shape is Zed's modal layer. One slot holds the one modal that is up.
Showing another replaces it. The modal's view exists only while it is up,
and the layer draws the backdrop and takes the press outside. We took it
as a pattern only, and no Zed code is copied. Zed holds its modal as a
trait object because any crate may add one. Ferrite's set of sheets is
closed, so an enum replaces the trait object.

## Words

A **sheet** is a dialog that covers the whole window over a veil. _Avoid_
modal.
