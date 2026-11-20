# Toasts

Ferrite shows short, silent toasts in the top right of every window: when an agent
finishes while you look elsewhere, when a CLI update is available, or when a script
or agent asks for one.

## Sub-features

- `toast-show` `notification show` puts a titled toast with a body in every window.
- `toast-done` a Thread finishing while another Thread is focused toasts and checks its sidebar row.

## How to get to it (user POV)

- Passive: they appear on their own. Clicking a toast jumps to its Thread.
- Scripting: `ferrite notification show <title> [--body …]`.

## Driving it with fv

Preconditions:

- Baseline run; `fv doctor` all `ok`.

- **Show.** Run `fv cli -- notification show "Verify toast" --body "from fv"`. Answer `windows: 1`. Within ~1 s, `fv shot toast`: a toast titled `Verify toast` with body `from fv` at the top right.
- **Done toast.** Start Threads `1` and `2` (threads.md), focus `2` with `fv cli -- thread focus 2`, prompt `1` with `--wait`. `fv shot toast-done`: a toast names Thread `1` and its sidebar row shows a check mark.

## Gotchas

- Toasts fade after a few seconds; screenshot right after the command, not after other slow steps.
- A `Claude CLI … is available` toast can stack with yours right after launch.
- `toast-done` does not fire for the focused Thread, for subagents, or for an interrupted turn.
- `system_notifications` is off in the scratch settings, so nothing reaches the system's notification centre; only in-window toasts are in scope.
