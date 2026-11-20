# Projects

A Project groups Threads by folder. The sidebar lists each Project with its Threads
under a heading; a fresh Store with no Project opens a `New project` dialog.

## Sub-features

- `project-empty` an empty Store shows the `New project` dialog.
- `project-open` registering a directory adds it to the sidebar.
- `project-list` lists Projects with open-Thread counts and the focused one.
- `project-rename` relabels the sidebar heading.
- `project-close` parks every open Thread of the Project.

## How to get to it (user POV)

- The `New project` dialog, or the pencil next to a Project in the project filter.
- Scripting: `ferrite project open|list|get|rename|close|focus`.

## Driving it with fv

Preconditions:

- Baseline run: Project `1` labelled `verify` already exists (seeded by `fv launch`).

- **Seeded.** Read `target/verify-ferrite/$FV_RUN/seed-project.json`: `label: "verify"`, `checkout_path` ends `/fv-$FV_RUN/repo`. `fv shot project-seeded`: sidebar heading `verify`, Pane reads `What should we build in verify?`.
- **List.** Run `fv cli -- project list`. One Project, `thread_count: 0`.
- **Open another.** `mkdir /tmp/fv-$FV_RUN/other && git -C /tmp/fv-$FV_RUN/other init -q`, then `fv cli -- project open --cwd /tmp/fv-$FV_RUN/other --label other --focus`. New id `2`; `fv shot project-other` shows `other` in the sidebar.
- **Rename.** Run `fv cli -- project rename 2 renamed`. Answer label `renamed`; sidebar heading changes.
- **Close.** Start a Thread in `other` (see threads.md), then `fv cli -- project close 2`. `fv cli -- thread list` no longer lists it.

## Gotchas

- `project-empty` cannot be seen through `fv launch`, which seeds a Project before the window opens. To prove it, start the window on an empty Store by hand with the same env `fv` uses; the dialog then stays up over later Threads until dismissed (mouse only).
- `project open` on a directory already registered returns the existing Project rather than a new one.
- On macOS `checkout_path` reports `/private/tmp/...`; `/tmp` is a symlink to it there.
