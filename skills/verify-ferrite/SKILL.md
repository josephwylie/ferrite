---
name: verify-ferrite
description: Launch an isolated Ferrite (the GPUI desktop app that runs Claude/Codex agents as Threads) from the current checkout, drive its real window headlessly (keys, clicks, typing, screenshots on macOS) and through its `ferrite` CLI, and keep transcripts and screenshots as proof. Never touches the operator's Ferrite, screen or focus. Use when you must show a Ferrite change working in the real app, such as Threads, prompts, Decisions, Projects, worktrees or toasts, not just in unit tests.
---

# Verify Ferrite

Ferrite is a GPUI desktop app whose window talks to a **Server** process over a Unix
socket in its Store. The same binary is a CLI for that Server (`ferrite thread start`,
`agent prompt`, `decision answer`, `window key`, …; reference in `skills/ferrite/SKILL.md`).
`ferrite --headless` (ADR 0038) runs the production window off-screen, attached to a
Server, and takes keys, clicks, typing and screenshots through `ferrite window …`.

This tracked folder is the only copy of the skill, for macOS and Linux alike; a machine
exposes it to Claude Code with a symlink from `.claude/skills/verify-ferrite` to it.

One helper does everything. Use the copy in the checkout you are verifying: it builds
and runs the checkout it sits in, so a worktree verifies its own code.

```sh
FV=skills/verify-ferrite/scripts/fv   # from the checkout's root; python3, no packages
```

## You are probably inside Ferrite already

If you are an agent in this repo, the operator's own Ferrite is running you, and your
shell exports `FERRITE_SOCKET`, `FERRITE_THREAD_ID` and `FERRITE_BIN` pointing at
**their** Server. A bare `ferrite …` or `target/debug/ferrite …` call drives the
operator's real Threads. Never do that for verification. `fv` strips those variables and
sets `FERRITE_STORE` to the run's own Store on every call, so always go through `fv`.
Never kill a `ferrite` or `ferrited` process by name: the operator's app and Server are
among them.

## Platforms

| | macOS | Linux |
| --- | --- | --- |
| launch, doctor, stop, `cli` | yes | yes |
| `key`, `type`, `click`, `find`, `resize`, `drag` (headless) | yes | yes |
| `shot` | yes, 2880x1800 | fails: `screenshots need macOS for now` |
| `launch --window` (the visible window) | yes | refused |

Linux has no off-screen renderer yet, so a Linux proof of window state is `fv find`
bounds plus a CLI read-back (`agent read`, `thread get`), never a picture.

## Launch

```sh
$FV launch                 # headless (default). Or: $FV launch <run-id>
$FV launch --window [id]   # macOS: the visible window, only for what headless can't show
export FV_RUN=<id>         # launch prints this line; every other command needs it
```

`launch` does, in order:

1. `cargo build --locked -p ferrite --features headless` (seconds when nothing changed).
2. Creates `/tmp/fv-<id>/` with a Store at `threads/` (`FERRITE_STORE`), scratch
   `settings.json` beside it (haiku at low effort, permission mode `default` so tools
   ask, Codex `on-request` + `read-only`; no login item, update checks, system
   notifications, sounds, auto-titles or prompt suggestions), and a one-commit git repo
   at `repo/`.
3. Starts `ferrite server` detached and waits until the API answers.
4. Registers `repo/` as Project `verify` (`project open`), so the window skips the
   "New project" dialog. The answer is kept as `seed-project.json`.
5. Starts `ferrite --headless` and waits until it answers (`window shot` on macOS,
   `window find` on Linux). With `--window`, waits until the window is on screen.

Ready means launch printed `export FV_RUN=…` and `$FV doctor` is all `ok`. Runs coexist:
each has its own Store, sockets and window. `--headless` refuses a Store outside the temp
directory, and `fv` refuses a run id that would put its Store anywhere but `/tmp/fv-<id>`.

## Doctor

```sh
$FV doctor
```

Read-only. Checks that both pids are alive and still run this checkout's binary, in the
right role, on this run's Store; that the Server owns `<store>/api.sock`; that the API
answers; and that the window answers. `--window` adds: on screen, not covered, `python3`
has `Quartz`, Accessibility granted. Prints the version, checkout, commit and open
Threads. Run it first whenever anything looks off. Any `FAIL` means stop and relaunch
rather than drive.

## Drive

```sh
$FV cli -- <ferrite args…>   # any CLI call, transcript kept
$FV key cmd-, escape         # gpui keystrokes, in order: cmd-shift-n, enter, shift-tab
$FV type "Reply with MANGO"  # into whatever holds the keyboard
$FV click --selector composer-block            # centre of what that element drew
$FV click --x 848 --y 811 [--double|--right]   # a point, in window points (1440x900)
$FV find approval-prompt     # where an element was drawn: {x,y,width,height}
$FV resize 700 900           # answers the size the window now has
$FV drag --from-selector panel-edge --to-x 1000 --to-y 400   # left button, moved in steps
$FV shot <name>              # PNG into the evidence dir (macOS)
```

Answers are one JSON line. Refusals are JSON on stderr with exit 1; a bad command line
exits 2.

- **Selectors are the stable handle.** They are gpui `debug_selector`s in the source:
  `rg -o 'debug_selector\(\|\| "[^"]+"' crates`. Examples: `composer-block`,
  `approval-prompt`, `decision-actions`, `settings-search`, `settings-card`.
  `find` answers `nothing drawn as …` when the last frame has no such element. Points
  are a fallback and move when the layout does.
- **Shots are 2880x1800 at scale 2.** Divide pixel positions by 2 for `--x/--y`.
- `key` presses each key and lets it go, as a keyboard does, so `enter` or `space` on
  a focused control (a Panel tab, `+`, a pull request's segment) clicks it from the
  release. It reports `unhandled` keys nothing took, the press or the release: the
  focus is not where you think.
  The keymap is the platform's: on Linux `ctrl` stands where macOS has `cmd`
  (`ctrl-,` opens Settings; `cmd-,` comes back unhandled).
- `resize` never goes under the app window's least size; the answer says the size it
  has. Shots and points follow it.
- `drag` presses at the `--from-…` place, moves to the `--to-…` place in steps with the
  button held, and lets go. A tab needs 6 points of travel to drag; less is a click.

Core recipe (a real agent, a few cents of haiku):

```sh
$FV cli -- thread start --cwd /tmp/fv-$FV_RUN/repo --title Probe --focus --ready \
  --timeout-ms 60000 --args=--model --args=haiku
$FV click --selector composer-block
$FV type "Reply with exactly the word PINEAPPLE."
$FV key enter
$FV cli -- agent wait 1 --timeout-ms 90000
$FV cli -- agent read 1 --lines 20
$FV shot replied             # macOS
```

- Thread ids start at `1` in a fresh Store and count up.
- `--args` takes one value per flag: `--args=--model --args=haiku`. `--args --model haiku`
  is refused.
- Start Threads through the CLI with that pin. A Thread started from the composer or the
  API without it ignores the scratch `claude_model` and runs the CLI's default model.
- `agent wait` settles on `done`, `idle` or `blocked`. `blocked` is a Decision, not done.

### What needs `--window` (macOS)

Headless runs the production window, but not the system around it. Use
`fv launch --window` for the native menu bar, window chrome, focus and activation, agent
visuals and the browser (CEF is off headless), system notifications and IME input.
Dropping files from another app has no helper in either mode; report it as unverified.

- `key`, `type` and `click --x --y` post events to the window's pid
  (`scripts/fv_input.py`, logged to `input.log`). `find`, `resize`, `drag` and
  `--selector` are headless only.
- It needs Accessibility for the app that runs `fv` (Ferrite itself for a Ferrite
  agent). Releases are unsigned, so an update can leave the switch on but stale. The fix
  is `tccutil reset Accessibility <bundle id>` (the `CFBundleIdentifier` in Ferrite.app's
  `Info.plist`), then the operator re-adds Ferrite under Privacy & Security ›
  Accessibility. Only the operator can do that.
- The window shows on the operator's screen. When another window covers it GPUI stops
  drawing it, so `fv shot` refuses. Ask the operator to uncover it; never raise it.

## Evidence

Everything lands in `target/verify-ferrite/<run-id>/` of the checkout (gitignored,
survives cleanup):

- `cli-NNN.txt`: every call (command, stdout, stderr, exit code), the build and the
  window commands included.
- `<name>.png`: `$FV shot <name>`.
- `seed-project.json`: the seeded Project.
- `input.log`: input sent in `--window` mode.
- `logs/`: the Server's and window's logs, their stdout (`server.out`, `window.out`),
  `settings.json` and `run.env`, copied in by `fv stop`.

Proof standards:

- Drive the path the user takes. Window features are driven with `key/type/click`.
  `cli` is the user path only for scripting features; elsewhere it is setup or a
  read-back of state. Say which.
- Capture before and after: the state the action starts from and the state it made.
- Verify side effects independently: files in `/tmp/fv-$FV_RUN/repo`, `git worktree list`,
  `agent read`, the spawned CLI's argv (the Server pid is `SERVER_PID` in
  `/tmp/fv-$FV_RUN/run.env`).
- Agents are the real `claude`/`codex` CLIs. The fakes in
  `crates/ferrite-core/tests/fixtures/` are for unit and visual tests.
- Look at each screenshot before claiming it proves something: a toast or dialog can
  cover what you meant to show.

## Cleanup

```sh
$FV stop
```

Sends SIGTERM to the run's window, then its Server (which ends the agent CLIs it
spawned), each only after checking the pid still runs this checkout's binary on this
run's Store. Copies the logs into the evidence dir, then deletes `/tmp/fv-<id>/`. If a
pid does not exit, `stop` keeps the directory and says so: inspect the pid before
anything harsher. Run `stop` after every failed launch too.

## Features

The maintained map is [`features/README.md`](features/README.md). Read it, then the
feature's file, before driving.
