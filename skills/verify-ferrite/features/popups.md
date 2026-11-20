# Popups

A popup is a menu or small card that hangs from a point or a control with no veil.
Ferrite has five in one slot (ADR 0043): the context menu, the permission mode menu, the
plan card, the CI checks card and the session controls card. One is up at a time. While
it is up it holds the keyboard. Escape or a press outside closes it.

## Sub-features

- `popup-mode` the permission mode menu on the Composer footer's mode chip.
- `popup-context` the right-press menu on nav rows, the transcript and the tools.
- `popup-plan` the plan card of the focused Thread.
- `popup-session` the session controls card.
- `popup-checks` the CI checks card (needs a pull request with checks).
- `popup-one-at-a-time` opening one popup over another leaves one drawn.
- `popup-escape` Escape closes the popup and never stops a turn or answers a Decision.

## How to get to it (user POV)

- Mode menu: press the mode chip in the Composer's footer (it reads the mode's name,
  such as "Manual"). Also the permissions button in the model picker's footer.
- Context menu: right press a nav row or a transcript message.
- Plan card: `cmd-k`, "Show plan", on a Thread whose agent wrote a plan.
- Session controls card: `cmd-k`, "Background tasks".
- Checks card: `cmd-k`, the checks command, on a Thread whose branch has a pull request.

## Driving it with fv

Preconditions:

- Baseline run; `fv doctor` all `ok`. One haiku Thread started with `--focus`.

- **Selectors.** `mode-menu`, `mode-segments`, `mode-segment-N`, `mode-preview`,
  `plan-card`. Take a mode segment's point from `fv find mode-segments`: a guessed point
  misses.
- **Holds the keyboard.** With a popup up, `fv focus` has `Popup` in its root context;
  `fv type hello`, and `fv text composer-block` has no `hello` (a shot on macOS). After
  `escape`, `Popup` is gone from `fv focus`; type again: it lands.
- **Escape keeps the turn.** Start a turn of 60 s or more, open the mode menu, `escape`
  once: `fv find mode-menu` says `nothing drawn` and `fv cli -- thread get <id>` still
  says `working`. Do not press `escape` twice: the second one stops the turn.
- **Escape keeps a Decision.** Leave a permission prompt unanswered with the Composer
  empty, open the mode menu, `escape`: `fv find approval-prompt` is still drawn and the
  Thread is still `blocked`.
- **A plan.** Prompt "Use the mcp__ferrite__update_plan tool to record a three step plan
  (steps: one, two, three, all pending), then reply PLANNED and stop." A plain "write a
  plan" can come back as a numbered list with no plan. Then `cmd-k`, "Show plan".
- **Under the details card.** `fv find plan-card` and `fv find details-card`: the plan
  card's top is below the details card's bottom, and their right edges match.
- **One at a time.** Context menu up, press the mode chip: `fv find` the two, only the
  mode menu is drawn. Then the other way round.
- **A menu over a sheet.** New task sheet, its Process chooser, `escape` twice: the first
  closes the chooser, the second the sheet. The chooser has no selector of its own:
  prove it with a shot.

## Gotchas

- There is no hover command, but a click leaves the pointer on the control: its hover
  ground shows in the next shot and its tooltip draws after about a second
  (`fv find tooltip`). A key press hides the tooltip and it does not come back, so
  click, wait, then look, with no key in between.
- A small light dash under the titlebar near the Pane's right edge shows in some shots,
  with and without a popup. It was there before ADR 0043.
- The Thread details card sits at the Pane's top right in every shot. It is not a popup.
- A fresh Thread's footer reads the default model until its first turn.
- `thread start --focus --ready` can answer `focused: false` although the window did
  switch.
