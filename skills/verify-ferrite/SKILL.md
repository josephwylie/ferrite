---
name: verify-ferrite
description: Launch an isolated Ferrite (the GPUI desktop app that runs Claude/Codex agents as Threads) from the current checkout on macOS, Linux or Windows, drive its real window headlessly (keys, clicks, typing, screenshots on macOS) and through its `ferrite` CLI, and keep transcripts and screenshots as proof. Never touches the operator's Ferrite, screen or focus. Use when you must show a Ferrite change working in the real app, such as Threads, prompts, Decisions, Projects, worktrees or toasts, not just in unit tests.
---

# Verify Ferrite

Ferrite is a GPUI desktop app whose window talks to a **Server** process over a Unix
socket in its Store. The same binary is a CLI for that Server (`ferrite thread start`,
`agent prompt`, `decision answer`, `window key`, …; reference in `skills/ferrite/SKILL.md`).
`ferrite --headless` (ADR 0038) runs the production window off-screen, attached to a
Server, and takes keys, clicks, typing and screenshots through `ferrite window …`.

This tracked folder is the only copy of the skill, for macOS, Linux and Windows; a machine
exposes it to Claude Code with a symlink from `.claude/skills/verify-ferrite` to it (on
Windows, a junction: `mklink /J .claude\skills\verify-ferrite skills\verify-ferrite`).

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

| | macOS | Linux | Windows (`fv_windows.py`) |
| --- | --- | --- | --- |
| launch, doctor, stop, `cli` | yes | yes | yes |
| `key`, `type`, `click`, `find`, `text`, `focus`, `resize`, `drag` (headless) | yes | yes | yes |
| `shot` | yes, 2880x1800 | fails: `screenshots need macOS for now` | yes, 2880x1800 |
| `launch --window` (the visible window) | yes | refused | refused |

Linux has no off-screen renderer yet, so a Linux proof of window state is `fv find`
bounds, `fv text` and `fv focus` read-backs, and a CLI read-back (`agent read`,
`thread get`), never a picture.

## Windows

Same commands, through a different helper. Run it from Git Bash at the checkout's root
(`python`, not `python3`, in PowerShell):

```sh
FV="python3 skills/verify-ferrite/scripts/fv_windows.py"   # Python 3.12+, no packages
```

`fv_windows.py` loads `fv` unchanged and replaces what reads processes and sockets
through ps, lsof or /proc:

- **Paths.** The run lives at `%TEMP%\fv-<id>` (`C:\Users\<you>\AppData\Local\Temp\fv-<id>`),
  not `/tmp`: `ferrite --headless` takes only a Store under the temp directory. Wherever a
  recipe says `/tmp/fv-$FV_RUN`, use `$(cygpath -w "$TEMP")\fv-$FV_RUN` in Git Bash, and
  give `--cwd` that Windows path. The binary is `target\debug\ferrite.exe`.
- **Whose process.** Windows lends no process's argv or environment cheaply, so
  `doctor` and `stop` trust a pid only while it runs `ferrite.exe` of this checkout and
  has the creation time `launch` kept (`SERVER_STARTED`, `WINDOW_STARTED` in `run.env`).
  A reused pid has another creation time.
- **The API is a named pipe**, `\\.\pipe\ferrite-server-<user SID>-<hash of
  <store>\api.sock>`. Doctor's `server owns …\api.sock` line asks the pipe for its
  server's pid.
- **Stop is TerminateProcess**: Windows has no SIGTERM to catch. The agent CLIs end with
  the Server, since they sit in kill-on-close jobs it holds. `stop` checks every process
  the run spawned is gone before it deletes the run, and clears the read-only bit git
  puts on its objects so the delete can go through.
- **Shots** are drawn as on macOS, off-screen at 2880x1800 and scale 2, by gpui's
  DirectX renderer with no window (`DirectXHeadlessRenderer`, patched into
  `vendor/gpui-pre-windows`). A shot with every surface and icon but no word means the
  headless text fell back to no glyphs: `window.out` says why.
- **Keys** are the Linux keymap: `ctrl` where a step says `cmd` (`ctrl-,` opens Settings).
- **The spawned CLI's argv**, for checks like a pinned model, in PowerShell:
  `Get-CimInstance Win32_Process -Filter "ParentProcessId=<SERVER_PID>" | Select-Object ProcessId, CommandLine`.
- **`the Ferrite server did not start: Access is denied (os error 5)`** means a window was
  started without the run's Server, from a shell inside a job that forbids breakaway (an
  agent's shell, usually): the window tries to start a Server detached and Windows
  refuses. `fv` starts the Server first, so a run never meets this. Never start Ferrite
  outside `fv` to "fix" it.

`fv_windows.py` uses `fv`'s names `BINARY`, `TEMP`, `disowned`, `listens_on`, `start`,
`Run` and `COMMANDS`. After renaming any of them in `fv`, run a Windows launch and doctor.

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
$FV text composer-block      # the lines of text it shows: {"lines":[…]}
$FV focus                    # what holds the keyboard: {contexts,selector,label}
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
- A click or drag by selector is refused while the element's centre is scrolled or
  clipped out of sight, such as a Panel tab half out of its strip with its middle under
  the strip's chevron. A press there would land on whatever shows in its place. Bring
  it into view first (`panel-tabs-right`), or click a point on the part that shows.
- `text` answers each line of text painted inside that element's bounds in the last
  frame, in paint order, with anything drawn over it and none scrolled or clipped
  away. Two elements that draw one selector are told apart by what they show. The
  Terminal reads as its painted rows; the Browser's page, images and icons are not text.
- `focus` answers `contexts`, the key contexts a key goes through (innermost last:
  `Composer`, `PanelControl`, …), `selector`, the nearest `debug_selector` around the
  focused element, and `label`, the Panel stop by the name its row gives it
  (`Overview`, `Files`), else `null`. Read it before blaming a key: `unhandled` from
  `key` means the keyboard is elsewhere, and `focus` says where.
- **Which Thread the window shows** is `fv find view-thread-<id>`: bounds while Thread
  `<id>` is the shown one, `nothing drawn as …` otherwise. One Pane is drawn at a time,
  so at most one `view-thread-*` answers (none on a draft or the empty board). `focused`
  in `thread get` is not this: it means the operator is looking, needs the window to
  have the system's focus, and is `false` in every headless run.
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
  (`scripts/fv_input.py`, logged to `input.log`). `find`, `text`, `focus`, `resize`,
  `drag` and `--selector` are headless only.
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
