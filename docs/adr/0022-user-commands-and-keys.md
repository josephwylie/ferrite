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
  array, and an `output`: `toast` (the default) or `reader`.
  ```json
  "commands": [
    { "name": "open in editor", "run": ["code", "."] },
    { "name": "run tests", "run": "cargo test", "output": "reader" }
  ]
  ```
  They are palette rows after the built-in commands, the run line muted
  after the name, offered while the focused Thread has a working directory.
  A command runs there, off the UI thread, with `FERRITE_THREAD_ID` set and
  stdin empty (no terminal: a program that would ask fails rather than
  wait). A toast says it started; when it ends, a toast names it with the
  output's last lines, or — `reader` — the whole output opens in the reader
  beside the Pane it ran from (and a failure toasts as well). The running
  half (`ferrite_core::user_commands`) is UI-free.
- **Keys are an override map in Settings** (`keys`): an action's registered
  name to one keystroke, or `null` to unbind it.
  ```json
  "keys": { "palette::Toggle": "cmd-p", "cockpit::ToggleNav": null }
  ```
  `keymap::with_user_keys` lays it over the defaults at launch. A key
  replaces every default spelling of its action **in place**, in each
  context the action is bound in, so the table's same-depth precedence
  (the order `keymap`'s tests pin) is untouched. The operator's key is
  theirs: another action's default on the same keystroke in the same
  context gives it up. An unknown action, a keystroke gpui cannot parse,
  `cmd` on Windows, or one keystroke given to two actions is refused with a
  sentence naming the entry and why; that action keeps its defaults, the
  rest apply, and the window says the refusals once in a toast.
- **One bound table.** The applied table is installed process-wide
  (`keymap::install` / `keymap::active`); every drawn chord
  (`components::bound_chord`, palette keys) and the `?` sheet read it, so
  what the window prints is what the keys do.
- **Reset** is the palette's `reset keys` (shown while `keys` is not
  empty): it clears the map, saves, and rebinds the defaults at once.
- **The `?` sheet filters.** A filter line under its head holds the
  keyboard; typing keeps the rows whose label, section or chord match (the
  palette's matcher), ↑↓ still scroll and esc still closes. A typed `?` is
  text there, so `?` no longer closes the sheet; esc does.

## Consequences

- Keys are read at launch; an edit to `settings.json` applies on the next
  launch (reset applies at once). A Settings page for keys, and binding a
  key to a user command, are not built: commands are reached from the
  palette.
- Action names (`cockpit::ToggleNav`) are the vocabulary of `keys`; they are
  the registered names the key table already used, so a rename now also
  breaks the operator's settings (it is refused and said, never a panic).
- The key table is a process-wide value rather than a pure function of the
  platform. Tests keep using `keymap::bindings(platform)` and
  `with_user_keys` directly and never install a custom table.
