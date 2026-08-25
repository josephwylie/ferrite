# Dogfood v1 — one week on the wall

The bar Ferrite must clear before the v1 announce: the operator runs their own
swarm **in Ferrite, not alongside it**, for one full week (five working days).
Real work only — no demo Threads count. The leader opens the tracking issue
from this file; boxes get checked there.

## Daily bar (every working day)

- [ ] At least 5 real Threads driven to a useful result
- [ ] At least 1 of them a worktree Thread (`cmd-shift-n`)
- [ ] Both Providers (Claude and Codex) used at least once
- [ ] Every Decision that day answered from the wall (`y`/`n`/`a` on the L3
      badge) without zooming the Pane to L1 first
- [ ] At least one queued prompt: a follow-up sent while the Thread was busy

## Weekly bar (at least once during the week)

- [ ] Two or more `--import`s of existing Claude or Codex CLI session files,
      each continued as a Ferrite Thread
- [ ] A full 24-pane wall held through a real working session
- [ ] Threads parked (`cmd-w`) and revived (`cmd-o`) across a relaunch

## Continuous invariants (hold all week)

- [ ] Zero data loss across relaunches: every Thread's transcript intact after
      quitting and reopening Ferrite, checked daily
- [ ] Session RSS stays under the watchdog limit (4 GiB per Session); any
      watchdog restart is visible and gets an issue
- [ ] Every crash, freeze, or dropped stream gets a GitHub issue the same day
- [ ] No falling back to the bare CLIs for work Ferrite claims to cover — a
      fallback means filing an issue for why Ferrite could not do it

## Exit

The week passes when every box is checked and no same-day-ticketed crash issue
remains open without triage.

**The v1 announce is blocked on completing this checklist.**
