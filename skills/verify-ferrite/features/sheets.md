# Sheets

A sheet is a dialog that covers the whole window over a dimmed veil. Ferrite has five:
Settings, the Project card, Background work, the Factory's New task, and a Task's
Rework or Direct this task. Only one is up at a time. Escape, the sheet's close button
and a press on the veil each close it, and opening another replaces it.

## Sub-features

- `sheet-settings` Settings opens, pages, searches and closes.
- `sheet-project` the Project card creates, renames and removes a Project.
- `sheet-background` Background work lists a Thread's background work and stops it.
- `sheet-new-task` New task keeps what was typed across a close and starts a Task.
- `sheet-direct` Rework and Direct this task keep their own text and choices per Task.
- `sheet-one-at-a-time` opening one sheet over another leaves exactly one drawn.
- `sheet-escape` Escape closes the sheet and never stops a running turn.

## How to get to it (user POV)

- Settings: `cmd-,`, the nav's gear, or the empty board's "Open Connections".
- Project card: the nav's "Add project", or the palette's "New project" / "Edit project".
- Background work: "Details" on the line under a transcript while that Thread has
  background work.
- New task: the Factory board's "New task", or "Run task" on the Workflows surface.
- Rework / Direct this task: buttons on a Workflow Task's panel in the Factory.

## Driving it with fv

Preconditions:

- Baseline run; `fv doctor` all `ok`. One haiku Thread started with `--focus`.

- **Settings.** `fv key cmd-,`; `fv find settings-card` is drawn. Press `settings-search`
  before typing in it. Connections, "+ Add" draws `connection-dialog` above the card: the
  first `escape` closes only that form.
- **Project card.** Click the nav's "Add project"; `fv find project-editor-card` is drawn.
  Press the name line before typing. To edit, register a second Project with
  `fv cli -- project open <dir>`, then `cmd-k`, "Edit project".
- **Background work.** Prompt the Thread: "Use the Bash tool with run_in_background set
  to true to run `sleep 240`. Then reply STARTED and stop." When the turn ends, click
  "Details" on the line; `fv find background-dialog` is drawn.
- **Factory.** Click the nav's Factory button, then its Project item (about (150,75)
  then (171,113) in window points; shoot the menu to confirm). The board shows
  "New task".
- **A cheap Workflow Task.** Rework and Direct this task need a Task on a Workflow:

  ```sh
  $FV cli -- workflow create --project 1
  $FV cli -- step edit w-1 s-1 --instructions "Reply with the single word OK and do nothing else." --on-finished approval
  $FV cli -- workflow rename w-1 Tiny
  $FV cli -- workflow save w-1
  ```

  Then in the window: "New task", type the same instruction, choose Process "Tiny",
  Start. The Worker runs haiku (check its argv under the Server pid) and the Task is
  Ready to approve in seconds, with Approve, Request rework and Direct this task on its
  panel. `fv cli -- task list` reads the state back.
- **Settings rows.** A row's label is `settings-label-<row title>` and a fact's value is
  `settings-fact-<row title>` (titles with a space need quotes: `"settings-label-Codex CLI"`).
  About is the last item of the page list, about (376,649).
- **No work left.** With the Background work dialog open, wait for the work to end:
  `fv find background-dialog-empty` is drawn.
- **Closed.** After `escape`, the close button or a veil press, wait ~1 s and `fv find`
  the card: `nothing drawn`.
- **One at a time.** With one sheet up open another (`cmd-,`, or `cmd-k` "New project"),
  shoot at once, and `fv find` both cards: only the new one is drawn.
- **Escape keeps the turn.** Start a turn that runs 40 s or more, open a sheet, `escape`
  once: the sheet is gone and `fv cli -- thread get <id>` still says `working`.

## Gotchas

- The exit fade is 150 ms and a screenshot takes longer, so the fade itself cannot be
  shot. Prove the close with `fv find`.
- A press on the veil closes the sheet and does nothing else, including over the nav.
- `cmd-k` opens the palette over a sheet; choosing a command that opens another sheet
  then replaces the first.
- A foreground `sleep` is refused by the agent's harness; use
  `python3 -c 'import time; time.sleep(60)'` for a long turn. The default permission
  mode asks first: answer with `fv cli -- decision answer`.
- Nav rows have no selector of their own (`nav-card` is the whole card); use a point on
  the row and confirm with a shot.
- Starting a Task starts a real Worker. Start one, with instructions that finish at once.
- The CLI has no command that removes a Project (`project close` parks its Threads), so
  the Project card closing with its Project is proved by the tests only.
- "Open Connections" is not a palette command. Its button shows only on the empty board
  with no environment.
