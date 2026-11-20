# Ferrite verification map

This directory is the maintained source for verifying Ferrite's user-facing behaviour.
Read this index before driving the app, then use the matching feature file as the recipe.

## Baseline preconditions

- A run launched with `fv launch` and `export FV_RUN=<id>`; `fv doctor` all `ok`.
  `fv` is `skills/verify-ferrite/scripts/fv` of the checkout under test (a worktree
  uses its own copy, which builds and runs its own code; `fv doctor` prints which).
- Fresh Store: no Threads, Project `verify` (id `1`) on `/tmp/fv-$FV_RUN/repo`, a git
  repo on `main` with one commit holding `README.md`.
- `claude` installed and logged in (Ferrite needs 2.1.224 ≤ version < 3.0.0). Codex
  journeys also need `codex` ≥ 0.149.1 and logged in.
- Never drive an instance this run did not start; never call `ferrite` outside `fv cli`.

## Driving conventions

- Runs are headless (`fv launch`) unless a step says `--window`. Drive window features
  with `fv key`, `fv type`, `fv click --selector …`. Use `fv cli -- <ferrite args>` for
  scripting features, for setup, and to read state back. On macOS every UI proof is
  `fv shot <name>`; on Linux, which has no off-screen renderer yet, it is `fv find`,
  `fv text` and `fv focus` on the element plus a CLI read-back, and the shot steps are
  reported as not driven.
- Keys are written for macOS. On Linux press `ctrl` where a step says `cmd`.
- Start Threads with `--focus --ready --args=--model --args=haiku` so the window shows
  them and the turn is cheap.
- Thread ids count from `1` per run; read ids from answers, don't assume them after
  the first.
- Wait with `--wait`/`agent wait`, never `sleep`, except ~1 s before a screenshot so
  the window has painted.
- Click by selector (`fv find <selector>` shows where it drew). Points are a fallback
  in window points; shots are 2x.
- Drag inside the window with `fv drag` (a column's edge, a tab along its strip, a
  scrollbar thumb). Dropping files from another app has no helper; report it as
  unverified.
- Resize the window mid-run with `fv resize <width> <height>`, so a fault that shows
  only after a narrow window is widened can be driven.
- Answer a permission prompt with `fv cli -- decision answer <id> --answer
  '{"kind":"allow","input":{...}}'`; `{"kind":"allow"}` alone is refused
  (`missing field input`).
- Icon buttons carry their id as selector: `nav-filter`, `nav-factory`, `nav-add-project`,
  `nav-new-thread`, `nav-settings`, `nav-pull-requests`, `nav-usage`, `panel-toggle`,
  `details-toggle`, `history-back`, `history-forward`, `view-title-menu`.

## Proof and skip reporting

- CLI proof is the `cli-NNN.txt` transcript: command, stdout, stderr, exit code.
- UI proof is a headless screenshot with the Thread or toast visible, before and after
  (macOS); on Linux, the `fv find`, `fv text` and `fv focus` answers and the CLI
  read-back.
- Side effects get a second, independent look (files in the repo, `git worktree list`,
  the spawned CLI's argv).
- Name the feature ID and entry point with every artifact.
- An entry point you could not drive is reported as such, with the command and the
  unmet precondition, never as verified through another path.

## Feature entry contract

Each feature file starts with an H1 and one paragraph of user-visible behaviour, then
exactly four H2s in order: `Sub-features`, `How to get to it (user POV)`,
`Driving it with fv`, `Gotchas`. Keep implementation details out.

## Features

- [Threads](./threads.md): start, prompt, read, rename, park and revive a Thread.
- [Decisions](./decisions.md): a permission request shows in the Pane and is answered.
- [Projects](./projects.md): register, list, rename and park a Project.
- [Worktrees](./worktrees.md): create a worktree, start a Thread in it, remove it.
- [Toasts](./toasts.md): `notification show` puts a toast in every window.
- [Sheets](./sheets.md): the five full-window dialogs, one at a time, and a cheap Factory Task.
- [Popups](./popups.md): the five menus and cards in one slot, the keyboard and Escape.
- [Files](./files.md): file tabs, moved along, shown and closed with the keyboard.
