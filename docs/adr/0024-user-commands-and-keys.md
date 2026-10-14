---
status: accepted
date: 2026-10-15
---

# The operator's own palette commands and keys

Follows the Herdr benchmark (`docs/research/herdr-benchmark-2026-10-15.md`,
"Custom palette commands and user keymap").

## Context

Herdr lets the operator add commands (`[[keys.command]]`: a shell line, run
from the focused pane's directory with its ids in the environment) and
rebind every action. Ferrite's palette held only its own verbs, and its key
table (`keymap::bindings`) was fixed: the operator could not reach `open in
editor`, `run tests` or `gh pr create` without leaving the window, nor move
a key that collides with their muscle memory.

## Decision

- **Commands live in Settings** (`settings.json` `commands`), each a `name`,
  a `run` that is a shell line (`sh -c`, `cmd /d /c` on Windows) or an argv
  array, and an `output`: `toast` (the default) or `reader`. On Windows an
  argv's bare program found on PATH only as a `.cmd`/`.bat` shim (`code`,
  the npm tools) runs as that shim, as the provider CLIs do.
  ```json
  "commands": [
    { "name": "open in editor", "run": ["code", "."] },
    { "name": "run tests", "run": "cargo test", "output": "reader" }
  ]
  ```
  They are palette rows after the built-in commands, the run line muted
  after the name, offered while the focused Thread has a working directory.
  A command runs there, on a thread of its own (a watcher may run all
  session, so it holds no executor worker), with `FERRITE_THREAD_ID` set and
  stdin empty (no terminal: a program that would ask fails rather than
  wait). There is no timeout — a test suite may rightly take minutes — but
  while it runs its palette row reads `stop <name>`, and picking it kills
  the command's whole process group on Unix (on Windows the child alone; a
  pipe a survivor holds is waited on half a second at most). Only the last
  1 MiB of each stream is kept. A toast says it started; when it ends, a
  toast names it with the output's last lines, or — `reader` — the output
  opens in the reader beside the Pane it ran from (and a failure or a stop
  toasts as well). The running half (`ferrite_core::user_commands`) is
  UI-free.
- **Keys are an override map in Settings** (`keys`): an action's registered
  name to one keystroke, or `null` to unbind it.
  ```json
  "keys": { "palette::Toggle": "cmd-p", "cockpit::ToggleNav": null }
  ```
  `keymap::with_user_keys` lays it over the defaults at launch, every
  `null` first. A key replaces every default spelling of its action **in
  place**, in each context the action is bound in, so the table's
  same-depth precedence (the order `keymap`'s tests pin) is untouched. The
  operator's key is theirs: another action's default on the same keystroke
  in the same context gives it up. Refused, with a sentence naming the
  entry and why: an unknown action, a keystroke gpui cannot parse, `cmd` on
  Windows, one keystroke given to two actions; a key that types (a
  character or space, alone or with shift) for an action bound anywhere a
  text field may hold the keyboard — only the no-text-field root, the
  Review slot, a Decision and the wall take plain keys; and a key another
  action holds where one of the two is bound with no context
  (`cockpit::ToggleNav` on `cmd-a`, which `composer::SelectAll` holds in a
  Composer). gpui's precedence there turns on depth and table order, so
  one of the two would silently lose; the sentence names the other action,
  and unbinding it (`null`) frees the key. A refused action keeps its
  defaults, the rest apply, and the window says the launch's refusals once
  in a toast.
- **One bound table.** The applied table is installed process-wide
  (`keymap::install` / `keymap::active`); every drawn chord
  (`components::bound_chord`, palette keys) and the `?` sheet read it, so
  what the window prints is what the keys do.
- **Binding replaces only the table.** Ferrite's rows are tagged
  (`KeyBindingMetaIndex`); gpui removes no single binding, so binding a
  table rebuilds the keymap with every other binding (the kit's inputs,
  text views, dialogs; `rich`) kept in order and the new table in the old
  one's place.
- **Reset** is the palette's `reset keys` (shown while `keys` is not
  empty): it clears the map, saves, and rebinds the defaults at once.
- **The `?` sheet filters.** A filter line under its head holds the
  keyboard; typing keeps the rows whose label, section or chord match (the
  palette's matcher), ↑↓ still scroll, ⏎ does nothing (the bare row would
  send the Pane's draft from under the sheet) and esc still closes. A typed `?` is
  text there, so `?` no longer closes the sheet; esc does.

## Consequences

- Keys are read at launch; an edit to `settings.json` applies on the next
  launch (reset applies at once). No setting is read back from the file
  while the window runs (the window is what writes it), so live keys would
  need a file watcher the rest of Settings does not have. A Settings page
  for keys, binding a key to a user command, and binding an action the
  default table does not bind are not built: commands are reached from
  the palette, and `keys` only moves keys the table already has.
- Two keys in different named contexts that meet on one focused node (a
  Palette's field is also a Composer) are left to the table's order; only
  a clash with a no-context key is refused.
- Action names (`cockpit::ToggleNav`) are the vocabulary of `keys`; they are
  the registered names the key table already used, so a rename now also
  breaks the operator's settings (it is refused and said, never a panic).
- The key table is a process-wide value rather than a pure function of the
  platform. Tests keep using `keymap::bindings(platform)` and
  `with_user_keys` directly and never install a custom table.
