---
status: accepted
date: 2026-11-18
---

# One sheet at a time, in one host

Ferrite has five sheets: Settings, the Project card, the Background work
dialog, the New task sheet and a Task's Rework or Direct this task sheet.
Each drew its own veil and wrote its own outside press, fade, focus and
Escape. Only Settings and the Project card closed each other, by hand, in
their openers. Two of the five have no Escape.

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
- **Settings and the Project card are the first two.**
- **The Background work dialog is the third.** `BackgroundDialogView`
  (`cockpit/background_dialog_view.rs`) owns which Thread it shows and its
  focus. It draws that Thread's running work and where its stops stand,
  and emits `BackgroundDialogEvent`: stop, open an agent's thread, close.
  The stopping stays the root's, because the background line under the
  transcript stops the same work. A dialog whose Thread is no longer open
  closes.
- **The New task sheet is the fourth.** `NewTaskView`
  (`cockpit/new_task_view.rs`) owns its instructions and PR fields, which
  of them holds the keyboard, the process chosen, the request that makes a
  second Start the same Task, and a refusal. It emits `NewTaskEvent`:
  start, open the Process menu, close. Starting the Task stays the
  root's. The sheet is its Factory's: once that Factory no longer shows,
  the sheet closes, and a Task started closes it. Escape now closes it,
  and a menu it opened closes before it.
- **A Task's Rework or Direct this task sheet is the fifth.** The two
  kinds share a Task, one text field, a card and their close, so they are
  one view, `DirectView` (`cockpit/direct_view.rs`), and one `Sheet` variant.
  Which of the two it is, with what it chooses (the Rework's recipient;
  the Step a redirect sends to, the Steps it skips, what follows), is its
  `Choice`. It owns its Task, its field, which holds the keyboard, and
  whether a send was tried. It emits `DirectEvent`: send the rework,
  apply the redirect, open a chooser's menu, close. A Step or a recipient
  picked in that menu reaches the view through the root. Sending stays
  the root's, as does a refusal, said on the Task's panel with the sheet
  left up. The answer box stays on the panel: it is drawn there, not on
  a sheet. The sheet is its Factory's and its Task's: once that Factory
  no longer shows or the Task leaves the board, it closes, and a send
  taken closes it. Escape now closes it, and a menu it opened closes
  before it.
- **All five sheets are in the host.** No sheet draws its own veil, and
  `hide_sheet` is the one close path for every one of them. ADR 0043 does
  the same for the menus and cards that hang over the window.
- **What was typed outlives its sheet as plain values.** The view is
  dropped on a close, so what must survive the close leaves with the
  sheet (`sheet_gone`) as plain values, kept by what outlives the sheet,
  and seeds the next view. A view is never kept alive out of sight. The
  New task sheet keeps each field's text and attachments
  (`DraftContent`), its process and its request on its Factory
  (`KeptNewTask`), so they go when the Factory does, and a Task started
  spends them. A refusal and which field held the keyboard are not kept.
  Nor are the caret, the selection or the undo history: the sheet reopens
  on its instructions with the caret at the end. A Task's Rework and
  Direct this task keep, per Task on its Factory (`KeptDirection`), each
  kind's text and choices apart from the other's, and a send taken spends
  its own kind's only. Whether a send was tried is not kept.

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
